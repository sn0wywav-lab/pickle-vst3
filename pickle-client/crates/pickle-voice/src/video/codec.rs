//! OpenH264 encoder/decoder wrappers tuned for real-time camera video.

use super::convert::I420;
use crate::settings::{EncoderEffort, VideoSettings};
use openh264::decoder::Decoder;
use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Level, Profile,
    RateControlMode, UsageType,
};
use openh264::formats::{YUVSlices, YUVSource};
use openh264::OpenH264API;

/// Pick an H.264 level large enough for the resolution and frame rate
/// (otherwise high modes like 4K60 would be rejected or capped).
pub fn level_for(width: u32, height: u32, fps: u32) -> Level {
    let mbs = (width.div_ceil(16) * height.div_ceil(16)) as u64;
    let mbps = mbs * fps.max(1) as u64;
    match (mbs, mbps) {
        (s, r) if s <= 3_600 && r <= 108_000 => Level::Level_3_1,
        (s, r) if s <= 5_120 && r <= 216_000 => Level::Level_3_2,
        (s, r) if s <= 8_192 && r <= 245_760 => Level::Level_4_1,
        (s, r) if s <= 8_704 && r <= 522_240 => Level::Level_4_2,
        (s, r) if s <= 22_080 && r <= 589_824 => Level::Level_5_0,
        (s, r) if s <= 36_864 && r <= 983_040 => Level::Level_5_1,
        _ => Level::Level_5_2,
    }
}

pub struct H264Encoder {
    inner: Encoder,
    pub width: usize,
    pub height: usize,
    out: Vec<u8>,
}

impl H264Encoder {
    pub fn new(width: u32, height: u32, fps: u32, s: &VideoSettings) -> Result<Self, String> {
        let fps = fps.max(1);
        let keyint = ((s.keyframe_interval_s.max(0.25) * fps as f32) as u32).max(1);
        let threads = if s.encoder_threads == 0 {
            std::thread::available_parallelism()
                .map(|n| (n.get() / 2).clamp(1, 8) as u16)
                .unwrap_or(2)
        } else {
            s.encoder_threads
        };
        let cfg = EncoderConfig::new()
            .bitrate(BitRate::from_bps(s.bitrate_kbps.saturating_mul(1000)))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .usage_type(UsageType::CameraVideoRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .profile(Profile::Baseline)
            .level(level_for(width, height, fps))
            .complexity(match s.effort {
                EncoderEffort::Fast => Complexity::Low,
                EncoderEffort::Balanced => Complexity::Medium,
                EncoderEffort::Quality => Complexity::High,
            })
            .intra_frame_period(IntraFramePeriod::from_num_frames(keyint))
            .skip_frames(false)
            .num_threads(threads);
        let inner = Encoder::with_api_config(OpenH264API::from_source(), cfg).map_err(|e| e.to_string())?;
        Ok(Self {
            inner,
            width: (width & !1) as usize,
            height: (height & !1) as usize,
            out: Vec::with_capacity(512 * 1024),
        })
    }

    pub fn force_keyframe(&mut self) {
        self.inner.force_intra_frame();
    }

    /// Encode one frame; returns an Annex-B access unit (may be empty).
    pub fn encode(&mut self, frame: &I420) -> Result<&[u8], String> {
        let (y, u, v) = frame.planes();
        let src = YUVSlices::new((y, u, v), (frame.width, frame.height), (frame.width, frame.width / 2, frame.width / 2));
        let bs = self.inner.encode(&src).map_err(|e| e.to_string())?;
        self.out.clear();
        bs.write_vec(&mut self.out);
        Ok(&self.out)
    }
}

pub struct H264Decoder {
    inner: Decoder,
}

impl H264Decoder {
    pub fn new() -> Result<Self, String> {
        Ok(Self { inner: Decoder::new().map_err(|e| e.to_string())? })
    }

    /// Decode an access unit into RGBA. Returns (width, height) when a
    /// picture was produced.
    pub fn decode_rgba(&mut self, annexb: &[u8], rgba: &mut Vec<u8>) -> Result<Option<(usize, usize)>, String> {
        match self.inner.decode(annexb).map_err(|e| e.to_string())? {
            Some(yuv) => {
                let (w, h) = yuv.dimensions();
                rgba.resize(w * h * 4, 0);
                yuv.write_rgba8(rgba);
                Ok(Some((w, h)))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let s = VideoSettings { bitrate_kbps: 2000, ..Default::default() };
        let mut enc = H264Encoder::new(320, 240, 30, &s).unwrap();
        let mut frame = I420::new(320, 240);
        let mut dec = H264Decoder::new().unwrap();
        let mut rgba = Vec::new();
        let mut decoded = 0;
        for i in 0..10u8 {
            frame.data.iter_mut().enumerate().for_each(|(j, p)| *p = (j as u8).wrapping_add(i * 3));
            let au = enc.encode(&frame).unwrap().to_vec();
            if au.is_empty() {
                continue;
            }
            if i == 0 {
                assert!(crate::rtp::h264_is_keyframe(&au));
            }
            if let Some((w, h)) = dec.decode_rgba(&au, &mut rgba).unwrap() {
                assert_eq!((w, h), (320, 240));
                decoded += 1;
            }
        }
        assert!(decoded >= 8, "{decoded}");
        assert!(matches!(level_for(3840, 2160, 60), Level::Level_5_2));
    }
}
