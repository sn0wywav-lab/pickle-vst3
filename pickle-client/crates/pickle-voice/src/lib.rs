//! Real-time media for the Pickle client.
//!
//! * [`engine`] — microphone capture → Opus (every encoder knob exposed),
//!   network audio → jitter buffer → mixer → speakers. The signal path is
//!   *filterless*: no noise suppression, echo cancellation, AGC, gate or
//!   high-pass is ever inserted.
//! * [`connection`] — Discord voice gateway v8, UDP transport with
//!   `aead_aes256_gcm_rtpsize` / `aead_xchacha20_poly1305_rtpsize`, and DAVE
//!   end-to-end encryption (via `davey`).
//! * [`video`] — camera capture + H.264 encode/decode + RTP packetization.

pub mod connection;
pub mod crypto;
pub mod devices;
pub mod engine;
pub mod opus;
pub mod resample;
pub mod rtp;
pub mod settings;
pub mod video;

pub use connection::{VoiceConnectInfo, VoiceConnection, VoiceShared, VoiceStatus};
pub use engine::{AudioEngine, AudioSink, EngineShared};
pub use settings::*;

/// Opus on Discord always runs on a 48 kHz RTP clock.
pub const RTP_AUDIO_CLOCK: u32 = 48_000;
pub const RTP_VIDEO_CLOCK: u32 = 90_000;
pub const OPUS_PAYLOAD_TYPE: u8 = 120;
pub const H264_PAYLOAD_TYPE: u8 = 101;
pub const H264_RTX_PAYLOAD_TYPE: u8 = 102;
