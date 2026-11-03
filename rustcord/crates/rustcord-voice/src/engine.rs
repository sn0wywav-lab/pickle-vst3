//! The audio engine.
//!
//! Thread layout (no locks on the real-time path):
//!
//! ```text
//!  mic ──cpal cb──▶ [rtrb SPSC] ──▶ encoder thread ──▶ AudioSink (RTP/UDP)
//!                                      │ (loopback test)
//!  UDP rx ──────────▶ [channel] ──▶ mixer thread ──▶ [rtrb SPSC] ──cpal cb──▶ speakers
//! ```
//!
//! The capture → encoder path never touches the samples unless you ask it to:
//! no noise suppression, no echo cancellation, no automatic gain control,
//! no noise gate (unless enabled) and no high-pass filter. At 48 kHz capture
//! with 0 dB gain, the exact device samples are what Opus encodes.

use crate::devices;
use crate::opus::{Decoder, Encoder, MAX_PACKET};
use crate::resample::Resampler;
use crate::settings::AudioSettings;
use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// The 3-byte Opus silence frame Discord expects at the end of a talk spurt.
pub const OPUS_SILENCE: [u8; 3] = [0xF8, 0xFF, 0xFE];
/// SSRC used for the local "hear yourself" monitor.
pub const LOOPBACK_SSRC: u32 = 0;

/// Destination for encoded packets (implemented by the voice connection).
pub trait AudioSink: Send + Sync {
    /// Called on the encoder thread for each Opus packet.
    fn send_opus(&self, packet: &[u8], samples_48k: u32);
    /// Transmission started/stopped (maps to the voice gateway SPEAKING op).
    fn set_speaking(&self, speaking: bool);
}

/// A received audio packet (already transport/E2EE decrypted).
pub struct IncomingAudio {
    pub ssrc: u32,
    pub user_id: u64,
    pub seq: u16,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceFormat {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
}

pub type RepaintFn = Arc<dyn Fn() + Send + Sync>;

pub struct EngineShared {
    settings: Mutex<AudioSettings>,
    settings_gen: AtomicU64,
    pub muted: AtomicBool,
    pub deafened: AtomicBool,
    pub loopback: AtomicBool,
    pub transmitting: AtomicBool,
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    pub packets_sent: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub packets_received: AtomicU64,
    pub packets_concealed: AtomicU64,
    sink: Mutex<Option<Arc<dyn AudioSink>>>,
    /// user id -> (peak level, last time audible)
    pub user_levels: Mutex<HashMap<u64, (f32, Instant)>>,
    pub user_volumes: Mutex<HashMap<u64, f32>>,
    pub input_format: Mutex<Option<DeviceFormat>>,
    pub output_format: Mutex<Option<DeviceFormat>>,
    pub errors: Mutex<Vec<String>>,
    stop: AtomicBool,
    repaint: Mutex<Option<RepaintFn>>,
}

impl EngineShared {
    fn new(settings: AudioSettings) -> Self {
        Self {
            settings: Mutex::new(settings),
            settings_gen: AtomicU64::new(1),
            muted: AtomicBool::new(false),
            deafened: AtomicBool::new(false),
            loopback: AtomicBool::new(false),
            transmitting: AtomicBool::new(false),
            input_peak: AtomicU32::new(0),
            output_peak: AtomicU32::new(0),
            packets_sent: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            packets_received: AtomicU64::new(0),
            packets_concealed: AtomicU64::new(0),
            sink: Mutex::new(None),
            user_levels: Mutex::new(HashMap::new()),
            user_volumes: Mutex::new(HashMap::new()),
            input_format: Mutex::new(None),
            output_format: Mutex::new(None),
            errors: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
            repaint: Mutex::new(None),
        }
    }

    pub fn settings(&self) -> AudioSettings {
        self.settings.lock().clone()
    }

    /// Replace the settings; the encoder picks them up on its next frame.
    pub fn set_settings(&self, s: AudioSettings) {
        *self.settings.lock() = s;
        self.settings_gen.fetch_add(1, Ordering::Release);
    }

    pub fn set_repaint(&self, f: Option<RepaintFn>) {
        *self.repaint.lock() = f;
    }

    fn report_error(&self, e: impl Into<String>) {
        let e = e.into();
        warn!("audio: {e}");
        let mut errs = self.errors.lock();
        if errs.len() > 20 {
            errs.remove(0);
        }
        errs.push(e);
    }

    /// Input peak in dBFS since the last call (resets).
    pub fn take_input_peak_db(&self) -> f32 {
        to_db(f32::from_bits(self.input_peak.swap(0, Ordering::Relaxed)))
    }

    pub fn take_output_peak_db(&self) -> f32 {
        to_db(f32::from_bits(self.output_peak.swap(0, Ordering::Relaxed)))
    }

    /// Users heard within the last `window`.
    pub fn is_user_speaking(&self, user: u64, window: Duration) -> bool {
        self.user_levels
            .lock()
            .get(&user)
            .map(|(_, t)| t.elapsed() < window)
            .unwrap_or(false)
    }

    pub fn set_user_volume(&self, user: u64, vol: f32) {
        self.user_volumes.lock().insert(user, vol.clamp(0.0, 4.0));
    }
}

fn to_db(lin: f32) -> f32 {
    if lin <= 1e-6 {
        -120.0
    } else {
        20.0 * lin.log10()
    }
}

fn max_atomic(a: &AtomicU32, v: f32) {
    let mut cur = a.load(Ordering::Relaxed);
    while f32::from_bits(cur) < v {
        match a.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(x) => cur = x,
        }
    }
}

pub struct AudioEngine {
    pub shared: Arc<EngineShared>,
    incoming: Sender<IncomingAudio>,
    threads: Vec<JoinHandle<()>>,
    waker: Arc<OnceLock<thread::Thread>>,
}

struct DeviceSetup {
    input: Option<(rtrb::Consumer<f32>, u32, usize)>,
    output: Option<(rtrb::Producer<f32>, u32, usize)>,
}

/// Which halves of the engine to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineMode {
    /// Open the speakers and mix received audio.
    pub playback: bool,
    /// Capture an output device's mix instead of a microphone (system
    /// audio for streams; WASAPI loopback on Windows).
    pub loopback: bool,
}

impl EngineMode {
    pub const VOICE: Self = Self { playback: true, loopback: false };
    pub const STREAM_AUDIO: Self = Self { playback: false, loopback: true };
}

impl AudioEngine {
    /// Open the configured devices and start all threads (voice mode).
    pub fn start(settings: AudioSettings) -> Self {
        Self::start_with(settings, EngineMode::VOICE)
    }

    /// Start with an explicit mode (e.g. capture-only stream audio).
    pub fn start_with(settings: AudioSettings, mode: EngineMode) -> Self {
        let shared = Arc::new(EngineShared::new(settings.clone()));
        let waker: Arc<OnceLock<thread::Thread>> = Arc::new(OnceLock::new());
        let (setup_tx, setup_rx) = crossbeam_channel::bounded::<DeviceSetup>(1);
        let (in_tx, in_rx) = crossbeam_channel::bounded::<IncomingAudio>(1024);

        let mut threads = Vec::new();
        {
            let shared = shared.clone();
            let waker = waker.clone();
            threads.push(
                thread::Builder::new()
                    .name("rustcord-audio-io".into())
                    .spawn(move || device_thread(shared, settings, mode, waker, setup_tx))
                    .expect("spawn audio io"),
            );
        }
        let setup = setup_rx
            .recv_timeout(Duration::from_secs(8))
            .unwrap_or(DeviceSetup { input: None, output: None });

        if let Some((cons, rate, ch)) = setup.input {
            let shared = shared.clone();
            let loop_tx = in_tx.clone();
            let h = thread::Builder::new()
                .name("rustcord-opus-enc".into())
                .spawn(move || encoder_thread(shared, cons, rate, ch, loop_tx))
                .expect("spawn encoder");
            let _ = waker.set(h.thread().clone());
            threads.push(h);
        }
        {
            let shared = shared.clone();
            threads.push(
                thread::Builder::new()
                    .name("rustcord-mixer".into())
                    .spawn(move || mixer_thread(shared, in_rx, setup.output))
                    .expect("spawn mixer"),
            );
        }
        Self { shared, incoming: in_tx, threads, waker }
    }

    pub fn set_sink(&self, sink: Option<Arc<dyn AudioSink>>) {
        *self.shared.sink.lock() = sink;
    }

    /// Channel used by the network receiver to deliver decrypted packets.
    pub fn incoming(&self) -> IncomingSender {
        IncomingSender(self.incoming.clone())
    }
}

#[derive(Clone)]
pub struct IncomingSender(Sender<IncomingAudio>);

impl IncomingSender {
    /// Never blocks: drops the packet if the mixer is far behind.
    pub fn push(&self, pkt: IncomingAudio) {
        if let Err(TrySendError::Full(_)) = self.0.try_send(pkt) {
            // dropped; mixer overloaded
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(t) = self.waker.get() {
            t.unpark();
        }
        for t in &self.threads {
            t.thread().unpark();
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Device thread: owns the (possibly !Send) cpal streams.
// ---------------------------------------------------------------------------

fn device_thread(
    shared: Arc<EngineShared>,
    settings: AudioSettings,
    mode: EngineMode,
    waker: Arc<OnceLock<thread::Thread>>,
    setup_tx: Sender<DeviceSetup>,
) {
    let mut setup = DeviceSetup { input: None, output: None };
    let mut streams: Vec<cpal::Stream> = Vec::new();

    // ---- input (mic, or an output device's mix in loopback mode) ----
    let input_dev = if mode.loopback {
        devices::find_output(settings.input_device.as_deref())
    } else {
        devices::find_input(settings.input_device.as_deref())
    };
    match input_dev {
        Some(dev) => match devices::pick_config(&dev, !mode.loopback, settings.capture_sample_rate) {
            Some(cfg) => {
                let rate = cfg.sample_rate();
                let ch = cfg.channels() as usize;
                // 500 ms of headroom
                let (prod, cons) = rtrb::RingBuffer::<f32>::new((rate as usize * ch / 2).max(8192));
                match build_input(&dev, &cfg, prod, waker.clone(), shared.clone()) {
                    Ok(s) => {
                        if let Err(e) = s.play() {
                            shared.report_error(format!("mic start failed: {e}"));
                        }
                        *shared.input_format.lock() = Some(DeviceFormat {
                            name: dev.to_string(),
                            sample_rate: rate,
                            channels: ch as u16,
                            sample_format: format!("{:?}", cfg.sample_format()),
                        });
                        info!("mic: {} @ {rate} Hz x{ch} {:?}", dev, cfg.sample_format());
                        streams.push(s);
                        setup.input = Some((cons, rate, ch));
                    }
                    Err(e) => shared.report_error(format!("mic open failed: {e}")),
                }
            }
            None => shared.report_error("microphone has no usable format"),
        },
        None => shared.report_error("no microphone found"),
    }

    // ---- output ----
    if !mode.playback {
        let _ = setup_tx.send(setup);
        while !shared.stop.load(Ordering::Acquire) {
            thread::park_timeout(Duration::from_millis(200));
        }
        drop(streams);
        return;
    }
    match devices::find_output(settings.output_device.as_deref()) {
        Some(dev) => match devices::pick_config(&dev, false, 48_000) {
            Some(cfg) => {
                let rate = cfg.sample_rate();
                let ch = cfg.channels() as usize;
                let (prod, cons) = rtrb::RingBuffer::<f32>::new((rate as usize * ch / 2).max(8192));
                match build_output(&dev, &cfg, cons, shared.clone()) {
                    Ok(s) => {
                        if let Err(e) = s.play() {
                            shared.report_error(format!("speaker start failed: {e}"));
                        }
                        *shared.output_format.lock() = Some(DeviceFormat {
                            name: dev.to_string(),
                            sample_rate: rate,
                            channels: ch as u16,
                            sample_format: format!("{:?}", cfg.sample_format()),
                        });
                        streams.push(s);
                        setup.output = Some((prod, rate, ch));
                    }
                    Err(e) => shared.report_error(format!("speaker open failed: {e}")),
                }
            }
            None => shared.report_error("output device has no usable format"),
        },
        None => shared.report_error("no output device found"),
    }

    let _ = setup_tx.send(setup);
    while !shared.stop.load(Ordering::Acquire) {
        thread::park_timeout(Duration::from_millis(200));
    }
    drop(streams);
}

fn build_input(
    dev: &cpal::Device,
    cfg: &cpal::SupportedStreamConfig,
    prod: rtrb::Producer<f32>,
    waker: Arc<OnceLock<thread::Thread>>,
    shared: Arc<EngineShared>,
) -> Result<cpal::Stream, cpal::Error> {
    fn typed<T>(
        dev: &cpal::Device,
        cfg: cpal::StreamConfig,
        mut prod: rtrb::Producer<f32>,
        waker: Arc<OnceLock<thread::Thread>>,
        shared: Arc<EngineShared>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: cpal::SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        dev.build_input_stream::<T, _, _>(
            cfg,
            move |data: &[T], _| {
                if let Ok(mut chunk) = prod.write_chunk_uninit(data.len().min(prod.slots())) {
                    let (a, b) = chunk.as_mut_slices();
                    let mut it = data.iter();
                    for slot in a.iter_mut().chain(b.iter_mut()) {
                        if let Some(s) = it.next() {
                            slot.write(<f32 as cpal::FromSample<T>>::from_sample_(*s));
                        }
                    }
                    // SAFETY: every slot of the chunk was written above.
                    unsafe { chunk.commit_all() };
                }
                if let Some(t) = waker.get() {
                    t.unpark();
                }
            },
            move |e| shared.report_error(format!("mic stream: {e}")),
            None,
        )
    }
    let sc = cfg.config();
    match cfg.sample_format() {
        cpal::SampleFormat::F32 => typed::<f32>(dev, sc, prod, waker, shared),
        cpal::SampleFormat::I16 => typed::<i16>(dev, sc, prod, waker, shared),
        cpal::SampleFormat::I32 => typed::<i32>(dev, sc, prod, waker, shared),
        cpal::SampleFormat::U16 => typed::<u16>(dev, sc, prod, waker, shared),
        cpal::SampleFormat::F64 => typed::<f64>(dev, sc, prod, waker, shared),
        _ => typed::<f32>(dev, sc, prod, waker, shared),
    }
}

fn build_output(
    dev: &cpal::Device,
    cfg: &cpal::SupportedStreamConfig,
    cons: rtrb::Consumer<f32>,
    shared: Arc<EngineShared>,
) -> Result<cpal::Stream, cpal::Error> {
    fn typed<T>(
        dev: &cpal::Device,
        cfg: cpal::StreamConfig,
        mut cons: rtrb::Consumer<f32>,
        shared: Arc<EngineShared>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: cpal::SizedSample + cpal::FromSample<f32> + Send + 'static,
    {
        dev.build_output_stream::<T, _, _>(
            cfg,
            move |data: &mut [T], _| {
                let n = data.len().min(cons.slots());
                let mut written = 0;
                if let Ok(chunk) = cons.read_chunk(n) {
                    let (a, b) = chunk.as_slices();
                    for (dst, src) in data.iter_mut().zip(a.iter().chain(b.iter())) {
                        *dst = T::from_sample_(*src);
                        written += 1;
                    }
                    chunk.commit_all();
                }
                for dst in &mut data[written..] {
                    *dst = T::EQUILIBRIUM;
                }
            },
            move |e| shared.report_error(format!("speaker stream: {e}")),
            None,
        )
    }
    let sc = cfg.config();
    match cfg.sample_format() {
        cpal::SampleFormat::F32 => typed::<f32>(dev, sc, cons, shared),
        cpal::SampleFormat::I16 => typed::<i16>(dev, sc, cons, shared),
        cpal::SampleFormat::I32 => typed::<i32>(dev, sc, cons, shared),
        cpal::SampleFormat::U16 => typed::<u16>(dev, sc, cons, shared),
        cpal::SampleFormat::F64 => typed::<f64>(dev, sc, cons, shared),
        _ => typed::<f32>(dev, sc, cons, shared),
    }
}

// ---------------------------------------------------------------------------
// Encoder thread
// ---------------------------------------------------------------------------

/// Map device channels to encoder channels without any filtering.
#[inline]
pub fn map_channels(input: &[f32], in_ch: usize, out_ch: usize, gain: f32, out: &mut Vec<f32>) {
    out.reserve(input.len() / in_ch * out_ch);
    let unity = gain == 1.0;
    for f in input.chunks_exact(in_ch) {
        match (in_ch, out_ch) {
            (1, 1) => out.push(f[0]),
            (1, _) => {
                out.push(f[0]);
                out.push(f[0]);
            }
            (_, 1) => out.push((f[0] + f[1]) * 0.5),
            (_, _) => {
                out.push(f[0]);
                out.push(f[1]);
            }
        }
    }
    if !unity {
        let start = out.len() - input.len() / in_ch * out_ch;
        for s in &mut out[start..] {
            *s *= gain;
        }
    }
}

fn encoder_thread(
    shared: Arc<EngineShared>,
    mut cons: rtrb::Consumer<f32>,
    dev_rate: u32,
    dev_ch: usize,
    loopback_tx: Sender<IncomingAudio>,
) {
    let mut gen = 0u64;
    let mut settings = shared.settings();
    let mut identity = None;
    let mut encoder: Option<Encoder> = None;
    let mut resampler = Resampler::new(dev_rate, 48_000, settings.channels.count());
    let mut raw: Vec<f32> = Vec::with_capacity(16_384);
    let mut mapped: Vec<f32> = Vec::with_capacity(16_384);
    let mut pending: VecDeque<f32> = VecDeque::with_capacity(32_768);
    let mut frame: Vec<f32> = Vec::with_capacity(2880 * 2);
    let mut packet = vec![0u8; MAX_PACKET];
    let mut speaking = false;
    let mut gate_until = Instant::now();
    let mut loop_seq: u16 = 0;
    let mut gain = 1.0f32;

    while !shared.stop.load(Ordering::Acquire) {
        let g = shared.settings_gen.load(Ordering::Acquire);
        if g != gen {
            gen = g;
            settings = shared.settings();
            gain = if settings.input_gain_db == 0.0 {
                1.0
            } else {
                10f32.powf(settings.input_gain_db / 20.0)
            };
            let id = settings.encoder_identity();
            if identity != Some(id) {
                identity = Some(id);
                encoder = match Encoder::new(settings.channels.count(), settings.application) {
                    Ok(e) => Some(e),
                    Err(e) => {
                        shared.report_error(e.to_string());
                        None
                    }
                };
                resampler = Resampler::new(dev_rate, 48_000, settings.channels.count());
                pending.clear();
            }
            if let Some(enc) = encoder.as_mut() {
                if let Err(e) = enc.apply(&settings) {
                    shared.report_error(e.to_string());
                }
            }
        }
        let enc_ch = settings.channels.count();

        // Pull everything the mic produced.
        let avail = cons.slots();
        if avail > 0 {
            if let Ok(chunk) = cons.read_chunk(avail) {
                let (a, b) = chunk.as_slices();
                raw.extend_from_slice(a);
                raw.extend_from_slice(b);
                chunk.commit_all();
            }
            let whole = raw.len() - raw.len() % dev_ch;
            mapped.clear();
            map_channels(&raw[..whole], dev_ch, enc_ch, gain, &mut mapped);
            raw.drain(..whole);
            if resampler.is_passthrough() {
                pending.extend(mapped.iter().copied());
            } else {
                let mut tmp = Vec::with_capacity(mapped.len() + 64);
                resampler.process(&mapped, &mut tmp);
                pending.extend(tmp);
            }
        }

        let frame_samples = settings.frame.samples_48k();
        let frame_len = frame_samples * enc_ch;
        while pending.len() >= frame_len {
            frame.clear();
            frame.extend(pending.drain(..frame_len));
            let peak = frame.iter().fold(0f32, |m, s| m.max(s.abs()));
            max_atomic(&shared.input_peak, peak);

            let muted = shared.muted.load(Ordering::Relaxed) || shared.deafened.load(Ordering::Relaxed);
            let open = if settings.voice_gate {
                if to_db(peak) > settings.voice_gate_threshold_db {
                    gate_until = Instant::now() + Duration::from_millis(400);
                }
                Instant::now() < gate_until
            } else {
                true
            };
            let sink = shared.sink.lock().clone();
            let loopback = shared.loopback.load(Ordering::Relaxed);

            if !muted && open {
                let Some(enc) = encoder.as_mut() else { break };
                let n = match enc.encode(&frame, &mut packet) {
                    Ok(n) => n,
                    Err(e) => {
                        shared.report_error(e.to_string());
                        continue;
                    }
                };
                if !speaking {
                    speaking = true;
                    shared.transmitting.store(true, Ordering::Relaxed);
                    if let Some(s) = &sink {
                        s.set_speaking(true);
                    }
                }
                if let Some(s) = &sink {
                    s.send_opus(&packet[..n], frame_samples as u32);
                    shared.packets_sent.fetch_add(1, Ordering::Relaxed);
                    shared.bytes_sent.fetch_add(n as u64, Ordering::Relaxed);
                }
                if loopback {
                    loop_seq = loop_seq.wrapping_add(1);
                    let _ = loopback_tx.try_send(IncomingAudio {
                        ssrc: LOOPBACK_SSRC,
                        user_id: 0,
                        seq: loop_seq,
                        payload: packet[..n].to_vec(),
                    });
                }
            } else if speaking {
                speaking = false;
                shared.transmitting.store(false, Ordering::Relaxed);
                if let Some(s) = &sink {
                    for _ in 0..5 {
                        s.send_opus(&OPUS_SILENCE, 960);
                    }
                    s.set_speaking(false);
                }
            }
        }
        thread::park_timeout(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------
// Mixer thread
// ---------------------------------------------------------------------------

const MIX_FRAME: usize = 960; // 20 ms @ 48 kHz

struct Source {
    user_id: u64,
    decoder: Decoder,
    packets: BTreeMap<u64, Vec<u8>>,
    last_ext: Option<u64>,
    next: u64,
    playing: bool,
    misses: u32,
    pcm: VecDeque<f32>,
    last_rx: Instant,
    last_frame: usize,
}

/// Extend a 16-bit RTP sequence number to 64 bits relative to `last`.
pub fn extend_seq(last: Option<u64>, seq: u16) -> u64 {
    let Some(last) = last else {
        return (1u64 << 32) + seq as u64;
    };
    let base = last & !0xFFFF;
    let cands = [base.wrapping_sub(0x1_0000) + seq as u64, base + seq as u64, base + 0x1_0000 + seq as u64];
    *cands
        .iter()
        .min_by_key(|c| (**c as i64 - last as i64).unsigned_abs())
        .unwrap()
}

impl Source {
    fn new(user_id: u64) -> Option<Self> {
        Some(Self {
            user_id,
            decoder: Decoder::new(2).ok()?,
            packets: BTreeMap::new(),
            last_ext: None,
            next: 0,
            playing: false,
            misses: 0,
            pcm: VecDeque::with_capacity(MIX_FRAME * 8),
            last_rx: Instant::now(),
            last_frame: MIX_FRAME,
        })
    }

    fn push(&mut self, seq: u16, payload: Vec<u8>) {
        let ext = extend_seq(self.last_ext, seq);
        self.last_ext = Some(self.last_ext.map_or(ext, |l| l.max(ext)));
        self.last_rx = Instant::now();
        if self.playing && ext < self.next {
            return; // too late
        }
        self.packets.insert(ext, payload);
        // Bound latency: never queue more than ~400 ms.
        while self.packets.len() > 20 {
            let first = *self.packets.keys().next().unwrap();
            self.packets.remove(&first);
            if self.playing && self.next <= first {
                self.next = first + 1;
            }
        }
    }

    /// Ensure at least one mix frame of PCM is buffered. Returns false when
    /// the source went quiet.
    fn fill(&mut self, scratch: &mut [f32], jitter_packets: usize, concealed: &AtomicU64) -> bool {
        if !self.playing {
            if self.packets.len() < jitter_packets.max(1) {
                return !self.pcm.is_empty();
            }
            self.playing = true;
            self.misses = 0;
            self.next = *self.packets.keys().next().unwrap();
        }
        while self.pcm.len() < MIX_FRAME * 2 {
            if let Some(pkt) = self.packets.remove(&self.next) {
                self.next += 1;
                self.misses = 0;
                match self.decoder.decode(&pkt, scratch, false) {
                    Ok(n) => {
                        self.last_frame = n.max(120);
                        self.pcm.extend(&scratch[..n * 2]);
                    }
                    Err(_) => {
                        // corrupt packet: conceal it
                        if let Ok(n) = self.decoder.conceal(scratch, self.last_frame) {
                            self.pcm.extend(&scratch[..n * 2]);
                        }
                    }
                }
            } else if !self.packets.is_empty() && self.misses < 5 {
                // gap: packet lost or late, conceal and move on
                self.misses += 1;
                self.next += 1;
                concealed.fetch_add(1, Ordering::Relaxed);
                match self.decoder.conceal(scratch, self.last_frame) {
                    Ok(n) => self.pcm.extend(&scratch[..n * 2]),
                    Err(_) => break,
                }
            } else if self.packets.is_empty() && self.misses < 2 && self.last_rx.elapsed() < Duration::from_millis(60) {
                self.misses += 1;
                match self.decoder.conceal(scratch, self.last_frame) {
                    Ok(n) => self.pcm.extend(&scratch[..n * 2]),
                    Err(_) => break,
                }
                self.next += 1;
            } else {
                // talk spurt over
                self.playing = false;
                if let Some((&k, _)) = self.packets.iter().next() {
                    self.next = k;
                }
                break;
            }
        }
        !self.pcm.is_empty()
    }
}

fn mixer_thread(
    shared: Arc<EngineShared>,
    rx: Receiver<IncomingAudio>,
    output: Option<(rtrb::Producer<f32>, u32, usize)>,
) {
    let mut sources: HashMap<u32, Source> = HashMap::new();
    let mut scratch = vec![0f32; 5760 * 2];
    let mut mix = vec![0f32; MIX_FRAME * 2];
    let mut resampled: Vec<f32> = Vec::with_capacity(MIX_FRAME * 4);
    let mut device_buf: Vec<f32> = Vec::with_capacity(MIX_FRAME * 8);
    let mut last_cleanup = Instant::now();
    let mut last_repaint = Instant::now();

    let (mut prod, out_rate, out_ch) = match output {
        Some((p, r, c)) => (Some(p), r, c),
        None => (None, 48_000, 2),
    };
    let capacity = prod.as_ref().map(|p| p.buffer().capacity()).unwrap_or(0);
    let mut resampler = Resampler::new(48_000, out_rate, 2);

    while !shared.stop.load(Ordering::Acquire) {
        let settings_jitter = shared.settings.lock().jitter_ms.clamp(0, 400) as usize;
        let jitter_packets = (settings_jitter / 20).max(1);
        let mut any_new = false;
        while let Ok(pkt) = rx.try_recv() {
            any_new = true;
            shared.packets_received.fetch_add(1, Ordering::Relaxed);
            let src = match sources.get_mut(&pkt.ssrc) {
                Some(s) => s,
                None => match Source::new(pkt.user_id) {
                    Some(s) => sources.entry(pkt.ssrc).or_insert(s),
                    None => continue,
                },
            };
            if pkt.user_id != 0 {
                src.user_id = pkt.user_id;
            }
            src.push(pkt.seq, pkt.payload);
        }

        let Some(p) = prod.as_mut() else {
            sources.clear();
            thread::park_timeout(Duration::from_millis(20));
            continue;
        };

        // Keep ~one jitter window + one frame queued in the device ring.
        let target = ((settings_jitter.max(20) + 20) * out_rate as usize / 1000) * out_ch;
        let target = target.min(capacity.saturating_sub(MIX_FRAME * out_ch * 2));
        let mut produced = false;
        while capacity - p.slots() < target {
            produced = true;
            mix.fill(0.0);
            let deaf = shared.deafened.load(Ordering::Relaxed);
            let volumes = shared.user_volumes.lock().clone();
            let master = shared.settings.lock().output_volume;
            let mut levels: Vec<(u64, f32)> = Vec::new();
            for src in sources.values_mut() {
                if !src.fill(&mut scratch, jitter_packets, &shared.packets_concealed) {
                    continue;
                }
                let vol = volumes.get(&src.user_id).copied().unwrap_or(1.0) * master;
                let take = src.pcm.len().min(MIX_FRAME * 2);
                let mut peak = 0f32;
                for (i, s) in src.pcm.drain(..take).enumerate() {
                    peak = peak.max(s.abs());
                    mix[i] += s * vol;
                }
                if peak > 0.01 {
                    levels.push((src.user_id, peak));
                }
            }
            if !levels.is_empty() {
                let mut map = shared.user_levels.lock();
                let now = Instant::now();
                for (u, l) in levels {
                    map.insert(u, (l, now));
                }
            }
            if deaf {
                mix.fill(0.0);
            }
            let peak = mix.iter().fold(0f32, |m, s| m.max(s.abs()));
            max_atomic(&shared.output_peak, peak);

            resampled.clear();
            resampler.process(&mix, &mut resampled);
            device_buf.clear();
            for f in resampled.chunks_exact(2) {
                match out_ch {
                    1 => device_buf.push((f[0] + f[1]) * 0.5),
                    2 => device_buf.extend_from_slice(f),
                    n => {
                        device_buf.extend_from_slice(f);
                        device_buf.extend(std::iter::repeat_n(0.0, n - 2));
                    }
                }
            }
            let _ = p.push_partial_slice(&device_buf);
        }

        if last_cleanup.elapsed() > Duration::from_secs(5) {
            last_cleanup = Instant::now();
            sources.retain(|_, s| s.last_rx.elapsed() < Duration::from_secs(30));
        }
        if (any_new || produced) && last_repaint.elapsed() > Duration::from_millis(100) {
            last_repaint = Instant::now();
            if let Some(r) = shared.repaint.lock().clone() {
                r();
            }
        }
        thread::park_timeout(Duration::from_millis(5));
    }
}

/// Test helper: an incoming-audio sender plus the receiving end.
#[cfg(test)]
pub(crate) fn test_incoming() -> (IncomingSender, Receiver<IncomingAudio>) {
    let (tx, rx) = crossbeam_channel::bounded(64);
    (IncomingSender(tx), rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seq_extension_wraps() {
        let a = extend_seq(None, 65_534);
        let b = extend_seq(Some(a), 65_535);
        let c = extend_seq(Some(b), 0);
        let d = extend_seq(Some(c), 1);
        assert_eq!(b, a + 1);
        assert_eq!(c, a + 2);
        assert_eq!(d, a + 3);
        // late packet from before the wrap
        assert_eq!(extend_seq(Some(d), 65_535), b);
    }

    #[test]
    fn channel_mapping_is_untouched_at_unity() {
        let input = [0.1, -0.2, 0.3, -0.4];
        let mut out = Vec::new();
        map_channels(&input, 2, 2, 1.0, &mut out);
        assert_eq!(out, input);
        out.clear();
        map_channels(&[0.5, 0.25], 1, 2, 1.0, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.25, 0.25]);
    }

    #[test]
    fn jitter_buffer_conceals_single_loss() {
        let mut enc = Encoder::new(2, crate::settings::Application::Music).unwrap();
        enc.apply(&AudioSettings::default()).unwrap();
        let mut src = Source::new(1).unwrap();
        let pcm = vec![0.1f32; 1920];
        let mut pkt = vec![0u8; MAX_PACKET];
        for seq in [10u16, 11, 13, 14] {
            let n = enc.encode(&pcm, &mut pkt).unwrap();
            src.push(seq, pkt[..n].to_vec());
        }
        let concealed = AtomicU64::new(0);
        let mut scratch = vec![0f32; 5760 * 2];
        let mut frames = 0;
        for _ in 0..4 {
            if src.fill(&mut scratch, 2, &concealed) {
                src.pcm.drain(..1920.min(src.pcm.len()));
                frames += 1;
            }
        }
        assert_eq!(frames, 4);
        assert_eq!(concealed.load(Ordering::Relaxed), 1);
    }
}
