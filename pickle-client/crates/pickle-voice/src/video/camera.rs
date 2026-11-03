//! Webcam capture thread: camera → I420 → H.264 → [`VideoSink`].

use super::codec::H264Encoder;
use super::convert::{self, I420};
use super::{best_mode, closest_mode, CameraInfo, CameraMode, VideoFrame, VideoSink};
use crate::settings::{PixelFormatPref, VideoSettings};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{
    ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution,
};
use nokhwa::Camera;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::info;

pub fn list_cameras() -> Vec<CameraInfo> {
    match nokhwa::query(ApiBackend::Auto) {
        Ok(list) => list
            .into_iter()
            .enumerate()
            .map(|(i, c)| CameraInfo {
                index: match c.index() {
                    CameraIndex::Index(n) => *n,
                    CameraIndex::String(_) => i as u32,
                },
                name: c.human_name(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn format_name(f: FrameFormat) -> &'static str {
    match f {
        FrameFormat::MJPEG => "MJPEG",
        FrameFormat::YUYV => "YUYV",
        FrameFormat::NV12 => "NV12",
        FrameFormat::GRAY => "GRAY",
        FrameFormat::RAWRGB => "RGB",
        FrameFormat::RAWBGR => "BGR",
    }
}

fn format_from_name(s: &str) -> FrameFormat {
    match s {
        "YUYV" => FrameFormat::YUYV,
        "NV12" => FrameFormat::NV12,
        "GRAY" => FrameFormat::GRAY,
        "RGB" => FrameFormat::RAWRGB,
        "BGR" => FrameFormat::RAWBGR,
        _ => FrameFormat::MJPEG,
    }
}

fn open(index: u32, req: RequestedFormatType) -> Result<Camera, String> {
    Camera::new(CameraIndex::Index(index), RequestedFormat::new::<RgbFormat>(req)).map_err(|e| e.to_string())
}

/// Every mode the camera reports (this briefly opens the device).
pub fn probe_modes(index: u32) -> Result<Vec<CameraMode>, String> {
    let mut cam = open(index, RequestedFormatType::None)?;
    let mut modes: Vec<CameraMode> = cam
        .compatible_camera_formats()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|f| CameraMode {
            width: f.width(),
            height: f.height(),
            fps: f.frame_rate(),
            format: format_name(f.format()).into(),
        })
        .collect();
    modes.sort_by_key(|m| std::cmp::Reverse((m.pixels(), m.fps)));
    modes.dedup();
    Ok(modes)
}

fn choose(modes: &[CameraMode], s: &VideoSettings) -> Option<CameraMode> {
    let filtered: Vec<CameraMode> = match s.pixel_format {
        PixelFormatPref::Auto => modes.to_vec(),
        p => {
            let want = match p {
                PixelFormatPref::Mjpeg => "MJPEG",
                PixelFormatPref::Yuyv => "YUYV",
                PixelFormatPref::Nv12 => "NV12",
                PixelFormatPref::Auto => unreachable!(),
            };
            let f: Vec<CameraMode> = modes.iter().filter(|m| m.format == want).cloned().collect();
            if f.is_empty() { modes.to_vec() } else { f }
        }
    };
    if s.max_out {
        best_mode(&filtered)
    } else {
        closest_mode(&filtered, s.width, s.height, s.fps)
    }
}

#[derive(Default)]
pub struct CameraStats {
    pub mode: Mutex<Option<CameraMode>>,
    pub frames: AtomicU64,
    pub bytes: AtomicU64,
    pub encode_us: AtomicU32,
    pub preview: Mutex<Option<VideoFrame>>,
    pub error: Mutex<Option<String>>,
}

pub struct CameraStream {
    stop: Arc<AtomicBool>,
    keyframe: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub stats: Arc<CameraStats>,
}

impl CameraStream {
    /// Start capturing. With `sink = None` this is a local preview only.
    pub fn start(
        settings: VideoSettings,
        sink: Option<Arc<dyn VideoSink>>,
        repaint: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let keyframe = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(CameraStats::default());
        let thread = {
            let (stop, keyframe, stats) = (stop.clone(), keyframe.clone(), stats.clone());
            std::thread::Builder::new()
                .name("pickle-camera".into())
                .spawn(move || {
                    if let Err(e) = run(settings, sink, repaint.clone(), &stop, &keyframe, &stats) {
                        *stats.error.lock() = Some(e);
                        if let Some(r) = repaint {
                            r();
                        }
                    }
                })
                .ok()
        };
        Self { stop, keyframe, thread, stats }
    }

    pub fn request_keyframe(&self) {
        self.keyframe.store(true, Ordering::Relaxed);
    }

    pub fn active_mode(&self) -> Option<CameraMode> {
        self.stats.mode.lock().clone()
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(
    settings: VideoSettings,
    sink: Option<Arc<dyn VideoSink>>,
    repaint: Option<Arc<dyn Fn() + Send + Sync>>,
    stop: &AtomicBool,
    keyframe: &AtomicBool,
    stats: &CameraStats,
) -> Result<(), String> {
    let modes = probe_modes(settings.camera_index).unwrap_or_default();
    let req = match choose(&modes, &settings) {
        Some(m) => RequestedFormatType::Exact(CameraFormat::new(
            Resolution::new(m.width, m.height),
            format_from_name(&m.format),
            m.fps,
        )),
        None if settings.max_out => RequestedFormatType::AbsoluteHighestResolution,
        None => RequestedFormatType::Closest(CameraFormat::new(
            Resolution::new(settings.width, settings.height),
            FrameFormat::MJPEG,
            settings.fps,
        )),
    };
    let mut cam = open(settings.camera_index, req)?;
    cam.open_stream().map_err(|e| e.to_string())?;
    let fmt = cam.camera_format();
    let (w, h, fps) = (fmt.width(), fmt.height(), fmt.frame_rate().max(1));
    let mode = CameraMode { width: w, height: h, fps, format: format_name(fmt.format()).into() };
    info!("camera streaming {}", mode.label());
    *stats.mode.lock() = Some(mode);

    let mut encoder = if sink.is_some() { Some(H264Encoder::new(w, h, fps, &settings)?) } else { None };
    let mut i420 = I420::new(w as usize, h as usize);
    let started = Instant::now();
    let mut serial = 0u64;
    let mut last_preview = Instant::now() - Duration::from_secs(1);

    while !stop.load(Ordering::Acquire) {
        let buf = match cam.frame() {
            Ok(b) => b,
            Err(e) => {
                *stats.error.lock() = Some(e.to_string());
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
        };
        let res = buf.resolution();
        if res.width() as usize != i420.width || res.height() as usize != i420.height {
            i420 = I420::new(res.width() as usize, res.height() as usize);
        }
        let src_w = res.width() as usize;
        match buf.source_frame_format() {
            FrameFormat::YUYV if buf.buffer().len() >= src_w * res.height() as usize * 2 => {
                convert::yuyv_to_i420(buf.buffer(), src_w, &mut i420)
            }
            FrameFormat::NV12 if buf.buffer().len() >= src_w * res.height() as usize * 3 / 2 => {
                convert::nv12_to_i420(buf.buffer(), src_w, res.height() as usize, &mut i420)
            }
            _ => match buf.decode_image::<RgbFormat>() {
                Ok(img) => convert::rgb_to_i420(img.as_raw(), img.width() as usize, &mut i420),
                Err(e) => {
                    *stats.error.lock() = Some(e.to_string());
                    continue;
                }
            },
        }

        if let (Some(enc), Some(sink)) = (encoder.as_mut(), sink.as_ref()) {
            if keyframe.swap(false, Ordering::Relaxed) {
                enc.force_keyframe();
            }
            let t0 = Instant::now();
            let au = enc.encode(&i420)?;
            stats.encode_us.store(t0.elapsed().as_micros() as u32, Ordering::Relaxed);
            if !au.is_empty() {
                let ts = (started.elapsed().as_secs_f64() * crate::RTP_VIDEO_CLOCK as f64) as u64 as u32;
                stats.bytes.fetch_add(au.len() as u64, Ordering::Relaxed);
                sink.send_frame(au, ts);
            }
        }
        stats.frames.fetch_add(1, Ordering::Relaxed);

        // Preview at <= 30 fps, <= 640 px wide to keep the UI thread idle.
        if last_preview.elapsed() >= Duration::from_millis(33) {
            last_preview = Instant::now();
            let (pw, ph, rgba) = convert::i420_preview_rgba(&i420, 640);
            serial += 1;
            *stats.preview.lock() = Some(VideoFrame {
                width: pw as u32,
                height: ph as u32,
                rgba: Arc::new(rgba),
                serial,
            });
            if let Some(r) = &repaint {
                r();
            }
        }
    }
    let _ = cam.stop_stream();
    Ok(())
}
