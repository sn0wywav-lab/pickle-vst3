//! User-tunable media settings. Everything Discord normally hides or caps is
//! exposed here, with defaults set to "maximum quality, zero processing".

use serde::{Deserialize, Serialize};

/// Opus coded audio bandwidth. Expressed to the user as the effective
/// sample rate of the coded signal (the "kHz" setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Bandwidth {
    /// 8 kHz sample rate, 4 kHz audio bandwidth.
    Narrow,
    /// 12 kHz sample rate, 6 kHz audio bandwidth.
    Medium,
    /// 16 kHz sample rate, 8 kHz audio bandwidth.
    Wide,
    /// 24 kHz sample rate, 12 kHz audio bandwidth.
    SuperWide,
    /// 48 kHz sample rate, 20 kHz audio bandwidth (full range).
    #[default]
    Full,
}

impl Bandwidth {
    pub const ALL: [Bandwidth; 5] = [
        Bandwidth::Narrow,
        Bandwidth::Medium,
        Bandwidth::Wide,
        Bandwidth::SuperWide,
        Bandwidth::Full,
    ];

    pub fn sample_rate_khz(self) -> u32 {
        match self {
            Self::Narrow => 8,
            Self::Medium => 12,
            Self::Wide => 16,
            Self::SuperWide => 24,
            Self::Full => 48,
        }
    }

    pub fn audio_bandwidth_khz(self) -> u32 {
        match self {
            Self::Narrow => 4,
            Self::Medium => 6,
            Self::Wide => 8,
            Self::SuperWide => 12,
            Self::Full => 20,
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|b| *b == self).unwrap_or(4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Channels {
    Mono,
    #[default]
    Stereo,
}

impl Channels {
    pub fn count(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

/// libopus application mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Application {
    /// `OPUS_APPLICATION_AUDIO`: full-fidelity mode. No speech high-pass
    /// filter, no speech emphasis. This is the filterless choice.
    #[default]
    Music,
    /// `OPUS_APPLICATION_RESTRICTED_LOWDELAY`: CELT only, lowest latency,
    /// also filterless.
    LowDelay,
    /// `OPUS_APPLICATION_VOIP`: what Discord uses. Applies a speech
    /// high-pass filter inside the encoder.
    Voice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Signal {
    Auto,
    #[default]
    Music,
    Voice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FrameDuration {
    Ms2_5,
    Ms5,
    Ms10,
    #[default]
    Ms20,
    Ms40,
    Ms60,
}

impl FrameDuration {
    pub const ALL: [FrameDuration; 6] = [
        Self::Ms2_5,
        Self::Ms5,
        Self::Ms10,
        Self::Ms20,
        Self::Ms40,
        Self::Ms60,
    ];

    /// Samples per channel at 48 kHz.
    pub fn samples_48k(self) -> usize {
        match self {
            Self::Ms2_5 => 120,
            Self::Ms5 => 240,
            Self::Ms10 => 480,
            Self::Ms20 => 960,
            Self::Ms40 => 1920,
            Self::Ms60 => 2880,
        }
    }

    pub fn millis(self) -> f32 {
        self.samples_48k() as f32 / 48.0
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ms2_5 => "2.5 ms",
            Self::Ms5 => "5 ms",
            Self::Ms10 => "10 ms",
            Self::Ms20 => "20 ms",
            Self::Ms40 => "40 ms",
            Self::Ms60 => "60 ms",
        }
    }
}

/// Opus' hard limit is 300 kb/s per channel.
pub const OPUS_MAX_KBPS_PER_CHANNEL: u32 = 300;
pub const MIN_BITRATE_KBPS: u32 = 6;
/// Slider ceiling (stereo at <=10 ms frames can really use 600 kb/s).
pub const MAX_BITRATE_KBPS: u32 = 600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Device names as reported by the OS. `None` = system default.
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    /// Requested capture rate in Hz (0 = prefer 48 kHz). 48 kHz capture is
    /// bit-transparent; anything else is resampled to Opus' 48 kHz clock.
    pub capture_sample_rate: u32,
    pub bitrate_kbps: u32,
    pub bandwidth: Bandwidth,
    pub channels: Channels,
    pub application: Application,
    pub signal: Signal,
    pub frame: FrameDuration,
    /// 0..=10
    pub complexity: u8,
    pub vbr: bool,
    pub constrained_vbr: bool,
    pub fec: bool,
    /// 0..=100 (only meaningful with FEC)
    pub expected_packet_loss: u8,
    pub dtx: bool,
    pub phase_inversion_disabled: bool,
    /// Digital input gain. 0 dB means samples go to the encoder untouched.
    pub input_gain_db: f32,
    pub output_volume: f32,
    /// Optional transmit gate. Off by default ("open mic", truly unprocessed).
    pub voice_gate: bool,
    pub voice_gate_threshold_db: f32,
    /// Output jitter buffer target in milliseconds.
    pub jitter_ms: u32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            capture_sample_rate: 0,
            bitrate_kbps: 510,
            bandwidth: Bandwidth::Full,
            channels: Channels::Stereo,
            application: Application::Music,
            signal: Signal::Music,
            frame: FrameDuration::Ms20,
            complexity: 10,
            vbr: true,
            constrained_vbr: false,
            fec: false,
            expected_packet_loss: 0,
            dtx: false,
            phase_inversion_disabled: false,
            input_gain_db: 0.0,
            output_volume: 1.0,
            voice_gate: false,
            voice_gate_threshold_db: -50.0,
            jitter_ms: 40,
        }
    }
}

impl AudioSettings {
    /// The bitrate libopus will actually use: limited by the per-channel cap
    /// and by the 1275-byte maximum Opus frame.
    pub fn effective_bitrate_kbps(&self) -> u32 {
        let per_channel = OPUS_MAX_KBPS_PER_CHANNEL * self.channels.count() as u32;
        let frames_per_sec = 48_000.0 / self.frame.samples_48k() as f32;
        let packet_cap = (1275.0 * 8.0 * frames_per_sec / 1000.0) as u32;
        // 40/60 ms packets are encoded as multiple 20 ms frames, so the
        // per-frame cap is that of 20 ms.
        let packet_cap = if self.frame.samples_48k() > 960 { 510 } else { packet_cap };
        self.bitrate_kbps
            .clamp(MIN_BITRATE_KBPS, MAX_BITRATE_KBPS)
            .min(per_channel)
            .min(packet_cap)
    }

    pub fn is_filterless(&self) -> bool {
        self.application != Application::Voice
            && !self.voice_gate
            && self.input_gain_db == 0.0
    }

    /// Settings that need a new encoder instance (not just a CTL).
    pub fn encoder_identity(&self) -> (Channels, Application) {
        (self.channels, self.application)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PixelFormatPref {
    #[default]
    Auto,
    Mjpeg,
    Yuyv,
    Nv12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EncoderEffort {
    Fast,
    #[default]
    Balanced,
    Quality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    /// Camera index (as reported by the OS enumeration).
    pub camera_index: u32,
    pub camera_name: Option<String>,
    /// "Max out": pick the camera's highest resolution, then its highest
    /// frame rate at that resolution. Overrides width/height/fps.
    pub max_out: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub pixel_format: PixelFormatPref,
    pub bitrate_kbps: u32,
    pub keyframe_interval_s: f32,
    pub effort: EncoderEffort,
    /// 0 = automatic.
    pub encoder_threads: u16,
    /// Max RTP payload size; lower it on networks with a small MTU.
    pub mtu_payload: usize,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            camera_index: 0,
            camera_name: None,
            max_out: true,
            width: 1920,
            height: 1080,
            fps: 60,
            pixel_format: PixelFormatPref::Auto,
            bitrate_kbps: 12_000,
            keyframe_interval_s: 2.0,
            effort: EncoderEffort::Balanced,
            encoder_threads: 0,
            mtu_payload: 1100,
        }
    }
}

pub const MAX_VIDEO_BITRATE_KBPS: u32 = 100_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_bitrate_caps() {
        let mut s = AudioSettings { bitrate_kbps: 600, ..Default::default() };
        assert_eq!(s.effective_bitrate_kbps(), 510); // 20 ms frame cap
        s.frame = FrameDuration::Ms10;
        assert_eq!(s.effective_bitrate_kbps(), 600); // 2 ch * 300
        s.channels = Channels::Mono;
        assert_eq!(s.effective_bitrate_kbps(), 300);
        s.bitrate_kbps = 1;
        assert_eq!(s.effective_bitrate_kbps(), MIN_BITRATE_KBPS);
    }

    #[test]
    fn defaults_are_filterless_stereo_fullband() {
        let s = AudioSettings::default();
        assert!(s.is_filterless());
        assert_eq!(s.channels, Channels::Stereo);
        assert_eq!(s.bandwidth.sample_rate_khz(), 48);
    }
}
