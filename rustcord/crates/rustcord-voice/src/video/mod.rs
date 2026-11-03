//! Camera video: capture, pixel conversion, H.264 encode/decode.

pub mod convert;

#[cfg(feature = "video")]
pub mod codec;

#[cfg(feature = "camera")]
pub mod camera;

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

#[cfg(feature = "screen")]
pub mod screen;

/// Live stats shared by the camera and screen capture threads.
#[derive(Default)]
pub struct CaptureStats {
    pub mode: Mutex<Option<CameraMode>>,
    pub frames: AtomicU64,
    pub bytes: AtomicU64,
    pub encode_us: AtomicU32,
    pub preview: Mutex<Option<VideoFrame>>,
    pub error: Mutex<Option<String>>,
}

/// A decoded RGBA frame ready to upload as a texture.
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
    /// Incremented for each new frame so the UI only re-uploads on change.
    pub serial: u64,
}

/// Destination for encoded camera frames (implemented by the voice connection).
pub trait VideoSink: Send + Sync {
    /// `annexb` is one access unit with 4-byte start codes.
    fn send_frame(&self, annexb: &[u8], rtp_timestamp: u32);
}

/// A capture mode offered by a camera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraMode {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: String,
}

impl CameraMode {
    pub fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    pub fn label(&self) -> String {
        format!("{}x{} @ {} fps ({})", self.width, self.height, self.fps, self.format)
    }
}

#[derive(Debug, Clone)]
pub struct CameraInfo {
    pub index: u32,
    pub name: String,
}

/// "Max out": highest resolution, then highest frame rate, then prefer
/// uncompressed formats (no MJPEG artifacts) at equal size/rate.
pub fn best_mode(modes: &[CameraMode]) -> Option<CameraMode> {
    modes
        .iter()
        .max_by_key(|m| (m.pixels(), m.fps, (m.format != "MJPEG") as u8))
        .cloned()
}

/// Mode closest to a requested size/rate.
pub fn closest_mode(modes: &[CameraMode], w: u32, h: u32, fps: u32) -> Option<CameraMode> {
    modes
        .iter()
        .min_by_key(|m| {
            let dp = (m.pixels() as i64 - (w as i64 * h as i64)).unsigned_abs();
            let df = (m.fps as i64 - fps as i64).unsigned_abs();
            (dp, df, (m.format == "MJPEG") as u8)
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(w: u32, h: u32, fps: u32, f: &str) -> CameraMode {
        CameraMode { width: w, height: h, fps, format: f.into() }
    }

    #[test]
    fn max_out_picks_biggest_then_fastest() {
        let modes = vec![m(1280, 720, 60, "MJPEG"), m(1920, 1080, 30, "MJPEG"), m(1920, 1080, 60, "MJPEG"), m(640, 480, 120, "YUYV")];
        assert_eq!(best_mode(&modes).unwrap(), m(1920, 1080, 60, "MJPEG"));
        assert_eq!(closest_mode(&modes, 1280, 720, 30).unwrap(), m(1280, 720, 60, "MJPEG"));
    }
}
