//! Screen / window capture for Go Live: capture → (box-scale) → I420 →
//! H.264 (screen-content mode) → [`VideoSink`].
//!
//! Monitors use the platform's push-based capture (DXGI Desktop
//! Duplication on Windows, ScreenCaptureKit on macOS, PipeWire/X11 on
//! Linux). Only the newest frame is ever encoded, so a slow encoder drops
//! frames instead of building latency.

use super::codec::H264Encoder;
use super::convert::{self, I420};
use super::{CameraMode, CaptureStats, VideoFrame, VideoSink};
use crate::settings::{StreamContent, StreamSettings, VideoSettings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::info;

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenSource {
    /// `monitor:<id>` or `window:<id>`.
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Monitor refresh rate (0 for windows).
    pub refresh_hz: f32,
    pub is_monitor: bool,
    pub primary: bool,
}

/// Every monitor plus visible, titled windows.
pub fn list_sources() -> Vec<ScreenSource> {
    let mut out = Vec::new();
    if let Ok(monitors) = xcap::Monitor::all() {
        for m in monitors {
            let Ok(id) = m.id() else { continue };
            let name = m.friendly_name().or_else(|_| m.name()).unwrap_or_else(|_| format!("Screen {id}"));
            out.push(ScreenSource {
                id: format!("monitor:{id}"),
                name,
                width: m.width().unwrap_or(0),
                height: m.height().unwrap_or(0),
                refresh_hz: m.frequency().unwrap_or(60.0),
                is_monitor: true,
                primary: m.is_primary().unwrap_or(false),
            });
        }
    }
    if let Ok(windows) = xcap::Window::all() {
        for w in windows {
            if w.is_minimized().unwrap_or(false) {
                continue;
            }
            let title = w.title().unwrap_or_default();
            let (Ok(id), Ok(width), Ok(height)) = (w.id(), w.width(), w.height()) else { continue };
            if title.trim().is_empty() || width < 64 || height < 64 {
                continue;
            }
            let app = w.app_name().unwrap_or_default();
            out.push(ScreenSource {
                id: format!("window:{id}"),
                name: if app.is_empty() || title.contains(&app) { title } else { format!("{title} — {app}") },
                width,
                height,
                refresh_hz: 0.0,
                is_monitor: false,
                primary: false,
            });
        }
    }
    out
}

pub struct ScreenStream {
    stop: Arc<AtomicBool>,
    keyframe: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub stats: Arc<CaptureStats>,
}

impl ScreenStream {
    /// Start capturing. With `sink = None` this only produces previews.
    pub fn start(
        settings: StreamSettings,
        sink: Option<Arc<dyn VideoSink>>,
        repaint: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let keyframe = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(CaptureStats::default());
        let thread = {
            let (stop, keyframe, stats) = (stop.clone(), keyframe.clone(), stats.clone());
            std::thread::Builder::new()
                .name("rustcord-screen".into())
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
}

impl Drop for ScreenStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

enum Source {
    Monitor { recorder: xcap::VideoRecorder, frames: std::sync::mpsc::Receiver<xcap::Frame> },
    Window(xcap::Window),
}

fn open_source(id: Option<&str>) -> Result<Source, String> {
    if let Some(wid) = id.and_then(|s| s.strip_prefix("window:")).and_then(|s| s.parse::<u32>().ok()) {
        let w = xcap::Window::all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|w| w.id().ok() == Some(wid))
            .ok_or("That window is gone")?;
        return Ok(Source::Window(w));
    }
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let wanted = id.and_then(|s| s.strip_prefix("monitor:")).and_then(|s| s.parse::<u32>().ok());
    let monitor = monitors
        .iter()
        .find(|m| wanted.is_some() && m.id().ok() == wanted)
        .or_else(|| monitors.iter().find(|m| m.is_primary().unwrap_or(false)))
        .or(monitors.first())
        .cloned()
        .ok_or("No screen to capture")?;
    let (recorder, frames) = monitor.video_recorder().map_err(|e| e.to_string())?;
    recorder.start().map_err(|e| e.to_string())?;
    Ok(Source::Monitor { recorder, frames })
}

fn run(
    settings: StreamSettings,
    sink: Option<Arc<dyn VideoSink>>,
    repaint: Option<Arc<dyn Fn() + Send + Sync>>,
    stop: &AtomicBool,
    keyframe: &AtomicBool,
    stats: &CaptureStats,
) -> Result<(), String> {
    let source = open_source(settings.source.as_deref())?;
    let fps = settings.fps.clamp(1, crate::settings::MAX_STREAM_FPS);
    let interval = Duration::from_secs_f64(1.0 / fps as f64);
    let enc_settings = VideoSettings {
        bitrate_kbps: settings.bitrate_kbps,
        keyframe_interval_s: settings.keyframe_interval_s,
        effort: settings.effort,
        encoder_threads: settings.encoder_threads,
        ..VideoSettings::default()
    };
    let screen_mode = settings.content == StreamContent::Clarity;

    let mut encoder: Option<H264Encoder> = None;
    let mut configured: Option<(u32, u32)> = None;
    let mut i420 = I420::new(2, 2);
    let mut scratch = Vec::new();
    let mut have_frame = false;
    let started = Instant::now();
    let mut next = Instant::now();
    let mut last_preview = Instant::now() - Duration::from_secs(1);
    let mut last_encode = Instant::now() - Duration::from_secs(1);
    let mut serial = 0u64;

    while !stop.load(Ordering::Acquire) {
        // ---- grab the newest frame ----
        let frame: Option<(u32, u32, Vec<u8>)> = match &source {
            Source::Monitor { frames, .. } => match frames.recv_timeout(interval.max(Duration::from_millis(5)) * 2) {
                Ok(mut f) => {
                    while let Ok(newer) = frames.try_recv() {
                        f = newer;
                    }
                    Some((f.width, f.height, f.raw))
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                Err(_) => return Err("Screen capture stopped".into()),
            },
            Source::Window(w) => match w.capture_image() {
                Ok(img) => Some((img.width(), img.height(), img.into_raw())),
                Err(e) => {
                    *stats.error.lock() = Some(e.to_string());
                    std::thread::sleep(Duration::from_millis(100));
                    None
                }
            },
        };

        if let Some((fw, fh, raw)) = frame {
            let (w, h) = settings.output_size(fw, fh);
            if w < 2 || h < 2 {
                continue;
            }
            if configured != Some((w, h)) {
                configured = Some((w, h));
                i420 = I420::new(w as usize, h as usize);
                *stats.mode.lock() = Some(CameraMode { width: w, height: h, fps, format: format!("{fw}x{fh} source") });
                info!("screen: {fw}x{fh} -> {w}x{h} @ {fps} fps");
                // Preview-only streams never encode.
                encoder = match &sink {
                    Some(_) => Some(H264Encoder::new(w, h, fps, &enc_settings, screen_mode)?),
                    None => None,
                };
            }
            convert::rgba_to_i420(&raw, fw as usize, fh as usize, &mut i420, &mut scratch);
            have_frame = true;
        } else if !(have_frame && last_encode.elapsed() >= Duration::from_millis(200)) {
            // Static screen: repeat the last picture a few times a second so
            // late viewers still get keyframes; otherwise wait for changes.
            continue;
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
            last_encode = Instant::now();
        }
        stats.frames.fetch_add(1, Ordering::Relaxed);

        if last_preview.elapsed() >= Duration::from_millis(100) {
            last_preview = Instant::now();
            let (pw, ph, rgba) = convert::i420_preview_rgba(&i420, 480);
            serial += 1;
            *stats.preview.lock() = Some(VideoFrame { width: pw as u32, height: ph as u32, rgba: Arc::new(rgba), serial });
            if let Some(r) = &repaint {
                r();
            }
        }

        // ---- pace to the target frame rate ----
        next += interval;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    }
    if let Source::Monitor { recorder, .. } = &source {
        let _ = recorder.stop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::codec::H264Decoder;
    use parking_lot::Mutex;

    struct Collect(Mutex<Vec<Vec<u8>>>);
    impl VideoSink for Collect {
        fn send_frame(&self, annexb: &[u8], _ts: u32) {
            self.0.lock().push(annexb.to_vec());
        }
    }

    /// Needs a display: `DISPLAY=:99 cargo test -p rustcord-voice -- --ignored`
    #[test]
    #[ignore]
    fn captures_and_encodes_the_desktop() {
        let sources = list_sources();
        assert!(sources.iter().any(|s| s.is_monitor), "{sources:?}");
        let sink = Arc::new(Collect(Mutex::new(Vec::new())));
        let settings = StreamSettings { fps: 30, max_height: 720, ..Default::default() };
        let stream = ScreenStream::start(settings, Some(sink.clone()), None);
        std::thread::sleep(Duration::from_secs(2));
        let mode = stream.stats.mode.lock().clone().expect("capture mode");
        assert!(mode.height <= 720);
        drop(stream);
        let frames = sink.0.lock().clone();
        assert!(frames.len() >= 5, "only {} frames", frames.len());
        let mut dec = H264Decoder::new().unwrap();
        let mut rgba = Vec::new();
        let decoded = frames.iter().filter_map(|f| dec.decode_rgba(f, &mut rgba).ok().flatten()).count();
        assert!(decoded >= 5, "{decoded}");
    }
}
