//! Thin, safe wrapper over libopus with *every* encoder CTL exposed.

use crate::settings::{Application, AudioSettings, Bandwidth, Signal};
use audiopus_sys as sys;
use std::ffi::CStr;
use std::os::raw::c_int;
use std::ptr::NonNull;

pub const SAMPLE_RATE: i32 = 48_000;
/// Largest Opus packet (120 ms of multi-frame data).
pub const MAX_PACKET: usize = 1275 * 6 + 7;

#[derive(Debug, Clone, thiserror::Error)]
#[error("opus: {0}")]
pub struct OpusError(pub String);

fn check(code: c_int) -> Result<c_int, OpusError> {
    if code < 0 {
        // SAFETY: opus_strerror returns a static, NUL-terminated string.
        let msg = unsafe { CStr::from_ptr(sys::opus_strerror(code)) };
        Err(OpusError(msg.to_string_lossy().into_owned()))
    } else {
        Ok(code)
    }
}

pub fn version() -> String {
    // SAFETY: static string.
    unsafe { CStr::from_ptr(sys::opus_get_version_string()) }
        .to_string_lossy()
        .into_owned()
}

fn application_const(a: Application) -> c_int {
    match a {
        Application::Music => sys::OPUS_APPLICATION_AUDIO,
        Application::LowDelay => sys::OPUS_APPLICATION_RESTRICTED_LOWDELAY,
        Application::Voice => sys::OPUS_APPLICATION_VOIP,
    }
}

fn bandwidth_const(b: Bandwidth) -> c_int {
    match b {
        Bandwidth::Narrow => sys::OPUS_BANDWIDTH_NARROWBAND,
        Bandwidth::Medium => sys::OPUS_BANDWIDTH_MEDIUMBAND,
        Bandwidth::Wide => sys::OPUS_BANDWIDTH_WIDEBAND,
        Bandwidth::SuperWide => sys::OPUS_BANDWIDTH_SUPERWIDEBAND,
        Bandwidth::Full => sys::OPUS_BANDWIDTH_FULLBAND,
    }
}

pub struct Encoder {
    ptr: NonNull<sys::OpusEncoder>,
    channels: usize,
}

// SAFETY: the encoder state is only accessed through &mut self.
unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new(channels: usize, application: Application) -> Result<Self, OpusError> {
        let mut err: c_int = 0;
        // SAFETY: valid arguments; error checked below.
        let ptr = unsafe {
            sys::opus_encoder_create(
                SAMPLE_RATE,
                channels as c_int,
                application_const(application),
                &mut err,
            )
        };
        check(err)?;
        let ptr = NonNull::new(ptr).ok_or_else(|| OpusError("encoder alloc failed".into()))?;
        Ok(Self { ptr, channels })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    fn ctl(&mut self, request: c_int, value: c_int) -> Result<(), OpusError> {
        // SAFETY: every request used here takes a single opus_int32 argument.
        check(unsafe { sys::opus_encoder_ctl(self.ptr.as_ptr(), request, value) }).map(|_| ())
    }

    /// Push every setting into the encoder. Cheap; safe to call per frame.
    pub fn apply(&mut self, s: &AudioSettings) -> Result<(), OpusError> {
        let bps = (s.effective_bitrate_kbps() * 1000) as c_int;
        self.ctl(sys::OPUS_SET_BITRATE_REQUEST, bps)?;
        // BANDWIDTH forces the coded bandwidth; MAX_BANDWIDTH keeps the
        // encoder from silently going above it.
        self.ctl(sys::OPUS_SET_MAX_BANDWIDTH_REQUEST, bandwidth_const(s.bandwidth))?;
        self.ctl(sys::OPUS_SET_BANDWIDTH_REQUEST, bandwidth_const(s.bandwidth))?;
        self.ctl(sys::OPUS_SET_COMPLEXITY_REQUEST, s.complexity.min(10) as c_int)?;
        self.ctl(sys::OPUS_SET_VBR_REQUEST, s.vbr as c_int)?;
        self.ctl(sys::OPUS_SET_VBR_CONSTRAINT_REQUEST, s.constrained_vbr as c_int)?;
        self.ctl(sys::OPUS_SET_INBAND_FEC_REQUEST, s.fec as c_int)?;
        self.ctl(
            sys::OPUS_SET_PACKET_LOSS_PERC_REQUEST,
            if s.fec { s.expected_packet_loss.min(100) as c_int } else { 0 },
        )?;
        self.ctl(sys::OPUS_SET_DTX_REQUEST, s.dtx as c_int)?;
        self.ctl(
            sys::OPUS_SET_SIGNAL_REQUEST,
            match s.signal {
                Signal::Auto => sys::OPUS_AUTO,
                Signal::Music => sys::OPUS_SIGNAL_MUSIC,
                Signal::Voice => sys::OPUS_SIGNAL_VOICE,
            },
        )?;
        // Never let the encoder downmix stereo on its own.
        self.ctl(
            sys::OPUS_SET_FORCE_CHANNELS_REQUEST,
            if self.channels == 2 { 2 } else { sys::OPUS_AUTO },
        )?;
        self.ctl(sys::OPUS_SET_LSB_DEPTH_REQUEST, 24)?;
        self.ctl(
            sys::OPUS_SET_PHASE_INVERSION_DISABLED_REQUEST,
            s.phase_inversion_disabled as c_int,
        )?;
        Ok(())
    }

    /// Encode one frame of interleaved f32 PCM (frame size inferred).
    pub fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize, OpusError> {
        let frame = pcm.len() / self.channels;
        // SAFETY: pcm holds `frame * channels` samples, out is writable.
        let n = unsafe {
            sys::opus_encode_float(
                self.ptr.as_ptr(),
                pcm.as_ptr(),
                frame as c_int,
                out.as_mut_ptr(),
                out.len().min(i32::MAX as usize) as i32,
            )
        };
        check(n).map(|n| n as usize)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: pointer came from opus_encoder_create.
        unsafe { sys::opus_encoder_destroy(self.ptr.as_ptr()) }
    }
}

pub struct Decoder {
    ptr: NonNull<sys::OpusDecoder>,
    channels: usize,
}

// SAFETY: only accessed through &mut self.
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new(channels: usize) -> Result<Self, OpusError> {
        let mut err: c_int = 0;
        // SAFETY: valid arguments.
        let ptr = unsafe { sys::opus_decoder_create(SAMPLE_RATE, channels as c_int, &mut err) };
        check(err)?;
        let ptr = NonNull::new(ptr).ok_or_else(|| OpusError("decoder alloc failed".into()))?;
        Ok(Self { ptr, channels })
    }

    /// Decode a packet into interleaved f32; returns samples per channel.
    /// `out` must hold at least 5760 * channels samples for 120 ms packets.
    pub fn decode(&mut self, packet: &[u8], out: &mut [f32], fec: bool) -> Result<usize, OpusError> {
        let max_frame = out.len() / self.channels;
        // SAFETY: buffers sized as declared.
        let n = unsafe {
            sys::opus_decode_float(
                self.ptr.as_ptr(),
                packet.as_ptr(),
                packet.len() as i32,
                out.as_mut_ptr(),
                max_frame as c_int,
                fec as c_int,
            )
        };
        check(n).map(|n| n as usize)
    }

    /// Packet-loss concealment for `frame` samples per channel.
    pub fn conceal(&mut self, out: &mut [f32], frame: usize) -> Result<usize, OpusError> {
        let frame = frame.min(out.len() / self.channels);
        // SAFETY: NULL data requests PLC; buffer sized for `frame`.
        let n = unsafe {
            sys::opus_decode_float(
                self.ptr.as_ptr(),
                std::ptr::null(),
                0,
                out.as_mut_ptr(),
                frame as c_int,
                0,
            )
        };
        check(n).map(|n| n as usize)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: pointer came from opus_decoder_create.
        unsafe { sys::opus_decoder_destroy(self.ptr.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Channels, FrameDuration};

    fn sine_stereo(n: usize, f_l: f32, f_r: f32) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let t = i as f32 / 48_000.0;
                [
                    0.5 * (2.0 * std::f32::consts::PI * f_l * t).sin(),
                    0.5 * (2.0 * std::f32::consts::PI * f_r * t).sin(),
                ]
            })
            .collect()
    }

    #[test]
    fn stereo_roundtrip_keeps_channels_separate() {
        let s = AudioSettings { channels: Channels::Stereo, frame: FrameDuration::Ms20, ..Default::default() };
        let mut enc = Encoder::new(2, s.application).unwrap();
        enc.apply(&s).unwrap();
        let mut dec = Decoder::new(2).unwrap();
        let pcm = sine_stereo(48_000, 440.0, 3000.0);
        let mut pkt = vec![0u8; MAX_PACKET];
        let mut out = vec![0f32; 5760 * 2];
        let mut decoded = Vec::new();
        let mut total_bytes = 0;
        for frame in pcm.chunks_exact(960 * 2) {
            let n = enc.encode(frame, &mut pkt).unwrap();
            total_bytes += n;
            let got = dec.decode(&pkt[..n], &mut out, false).unwrap();
            decoded.extend_from_slice(&out[..got * 2]);
        }
        // ~510 kb/s for one second of audio with unconstrained VBR.
        let kbps = total_bytes * 8 / 1000;
        assert!(kbps > 300, "bitrate too low: {kbps}");
        // Left and right must differ (true stereo, not dual mono).
        let diff: f32 = decoded
            .chunks_exact(2)
            .skip(4800)
            .map(|lr| (lr[0] - lr[1]).abs())
            .sum::<f32>()
            / 40_000.0;
        assert!(diff > 0.1, "channels collapsed: {diff}");
    }

    #[test]
    fn bandwidth_setting_changes_size() {
        let s = AudioSettings { bitrate_kbps: 32, vbr: false, ..Default::default() };
        let mut enc = Encoder::new(2, s.application).unwrap();
        enc.apply(&s).unwrap();
        let pcm = sine_stereo(960, 440.0, 440.0);
        let mut pkt = vec![0u8; MAX_PACKET];
        let n = enc.encode(&pcm, &mut pkt).unwrap();
        // CBR 32 kb/s @ 20 ms == 80 bytes
        assert!((70..=90).contains(&n), "{n}");
    }

    #[test]
    fn plc_produces_samples() {
        let mut dec = Decoder::new(2).unwrap();
        let mut out = vec![0f32; 960 * 2];
        assert_eq!(dec.conceal(&mut out, 960).unwrap(), 960);
    }
}
