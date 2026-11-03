//! Discord voice connection: voice gateway v8 (websocket) + UDP media.
//!
//! The websocket runs as a tokio task. Media never touches the async
//! runtime: the encoder/camera threads encrypt and `send()` directly on a
//! connected std UDP socket, and a dedicated receive thread decrypts and
//! hands packets to the mixer / video decoder. That keeps scheduling jitter
//! out of the audio path.

use crate::crypto::{Cipher, CryptoMode};
use crate::engine::{AudioSink, IncomingAudio, IncomingSender};
use crate::rtp::{self, RtpHeader};
use crate::video::{VideoFrame, VideoSink};
use crate::{H264_PAYLOAD_TYPE, OPUS_PAYLOAD_TYPE};
use davey::{Codec, DaveSession, MediaType, ProposalsOperationType};
use futures_util::{SinkExt, StreamExt};
use parking_lot::{Mutex, RwLock};
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::net::UdpSocket;
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, info, warn};

const DAVE_MAGIC: [u8; 2] = [0xFA, 0xFA];

/// A normal voice connection, or a Go Live stream connection (sending or
/// watching). They speak the same protocol; streams use the `screen` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnKind {
    #[default]
    Voice,
    Stream,
}

impl ConnKind {
    fn stream_type(self) -> &'static str {
        match self {
            Self::Voice => "video",
            Self::Stream => "screen",
        }
    }
}

#[derive(Debug, Clone)]
pub struct VoiceConnectInfo {
    pub endpoint: String,
    pub token: String,
    pub session_id: String,
    /// Guild id, or the channel id for DM / group calls.
    pub server_id: u64,
    pub channel_id: u64,
    pub user_id: u64,
    pub kind: ConnKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VoiceStatus {
    Connecting,
    Connected,
    Reconnecting(String),
    Disconnected(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoAttrs {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

pub type RepaintFn = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct RtpState {
    seq: u16,
    timestamp: u32,
    buf: Vec<u8>,
}

/// Encrypting UDP transport for one voice session.
pub struct Transport {
    socket: UdpSocket,
    cipher: Cipher,
    nonce: AtomicU32,
    audio_ssrc: u32,
    video_ssrc: u32,
    audio: Mutex<RtpState>,
    video: Mutex<RtpState>,
    stop: Arc<AtomicBool>,
    pub bytes_out: AtomicU64,
}

impl Transport {
    fn send_rtp(&self, st: &mut RtpState, header: RtpHeader, ext: Option<&[u8]>, payload: &[u8]) {
        let RtpState { buf, .. } = st;
        buf.clear();
        header.write(buf, ext);
        // With *_rtpsize modes only the extension preamble is authenticated
        // in the clear; its body is encrypted with the payload.
        let aad = rtp::RTP_HEADER_LEN + if ext.is_some() { 4 } else { 0 };
        buf.extend_from_slice(payload);
        let counter = self.nonce.fetch_add(1, Ordering::Relaxed);
        if self.cipher.seal(buf, aad, counter).is_ok() {
            if let Ok(n) = self.socket.send(buf) {
                self.bytes_out.fetch_add(n as u64, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

pub struct VoiceShared {
    pub info: VoiceConnectInfo,
    pub status: Mutex<VoiceStatus>,
    pub ping_ms: AtomicU32,
    pub mode: Mutex<Option<CryptoMode>>,
    pub dave_version: AtomicU16,
    pub privacy_code: Mutex<Option<String>>,
    pub ssrc_users: RwLock<HashMap<u32, u64>>,
    pub video_ssrcs: RwLock<HashMap<u32, u64>>,
    pub connected_users: Mutex<HashSet<u64>>,
    pub remote_video: Mutex<HashMap<u64, VideoFrame>>,
    pub decrypt_failures: AtomicU64,
    pub video_frames_in: AtomicU64,
    dave: Mutex<Option<DaveSession>>,
    transport: Mutex<Option<Arc<Transport>>>,
    audio_in: Mutex<Option<IncomingSender>>,
    ws_out: Mutex<Option<mpsc::UnboundedSender<WsMessage>>>,
    repaint: Mutex<Option<RepaintFn>>,
    video_attrs: Mutex<Option<VideoAttrs>>,
    rtx_ssrc: AtomicU32,
    mtu: AtomicU32,
}

impl VoiceShared {
    fn transport(&self) -> Option<Arc<Transport>> {
        self.transport.lock().clone()
    }

    fn repaint(&self) {
        if let Some(r) = self.repaint.lock().clone() {
            r();
        }
    }

    fn set_status(&self, s: VoiceStatus) {
        *self.status.lock() = s;
        self.repaint();
    }

    pub fn status(&self) -> VoiceStatus {
        self.status.lock().clone()
    }

    pub fn bytes_out(&self) -> u64 {
        self.transport().map(|t| t.bytes_out.load(Ordering::Relaxed)).unwrap_or(0)
    }

    fn send_ws(&self, v: Value) {
        if let Some(tx) = self.ws_out.lock().as_ref() {
            let _ = tx.send(WsMessage::Text(v.to_string().into()));
        }
    }

    fn send_ws_binary(&self, op: u8, payload: &[u8]) {
        let mut b = Vec::with_capacity(payload.len() + 1);
        b.push(op);
        b.extend_from_slice(payload);
        if let Some(tx) = self.ws_out.lock().as_ref() {
            let _ = tx.send(WsMessage::Binary(b.into()));
        }
    }

    /// DAVE-encrypt a frame if the E2EE session is active.
    fn e2ee_encrypt<'a>(&self, media: MediaType, codec: Codec, frame: &'a [u8]) -> Option<Cow<'a, [u8]>> {
        if self.dave_version.load(Ordering::Relaxed) == 0 {
            return Some(Cow::Borrowed(frame));
        }
        let mut guard = self.dave.lock();
        match guard.as_mut() {
            Some(s) if s.is_ready() => match s.encrypt(media, codec, frame) {
                Ok(Cow::Borrowed(b)) if b.len() == frame.len() => Some(Cow::Borrowed(frame)),
                Ok(c) => Some(Cow::Owned(c.into_owned())),
                Err(e) => {
                    debug!("dave encrypt: {e}");
                    None
                }
            },
            // Session not established yet: send transport-encrypted only,
            // receivers accept it during the transition window.
            _ => Some(Cow::Borrowed(frame)),
        }
    }
}

/// [`AudioSink`] that RTP-packetizes, encrypts and sends.
struct NetSink(Arc<VoiceShared>);

impl AudioSink for NetSink {
    fn send_opus(&self, packet: &[u8], samples_48k: u32) {
        let Some(t) = self.0.transport() else { return };
        let Some(payload) = self.0.e2ee_encrypt(MediaType::AUDIO, Codec::OPUS, packet) else { return };
        let mut st = t.audio.lock();
        let header = RtpHeader {
            marker: false,
            payload_type: OPUS_PAYLOAD_TYPE,
            seq: st.seq,
            timestamp: st.timestamp,
            ssrc: t.audio_ssrc,
        };
        st.seq = st.seq.wrapping_add(1);
        st.timestamp = st.timestamp.wrapping_add(samples_48k);
        t.send_rtp(&mut st, header, None, &payload);
    }

    fn set_speaking(&self, speaking: bool) {
        let ssrc = self.0.transport().map(|t| t.audio_ssrc).unwrap_or(0);
        self.0.send_ws(json!({"op": 5, "d": {"speaking": if speaking { 1 } else { 0 }, "delay": 0, "ssrc": ssrc}}));
    }
}

impl VideoSink for NetSink {
    fn send_frame(&self, annexb: &[u8], rtp_timestamp: u32) {
        let Some(t) = self.0.transport() else { return };
        let Some(frame) = self.0.e2ee_encrypt(MediaType::VIDEO, Codec::H264, annexb) else { return };
        let mtu = self.0.mtu.load(Ordering::Relaxed).clamp(500, 1400) as usize;
        let mut st = t.video.lock();
        let mut sent = 0usize;
        rtp::packetize_h264(&frame, mtu, |payload, last| {
            let header = RtpHeader {
                marker: last,
                payload_type: H264_PAYLOAD_TYPE,
                seq: st.seq,
                timestamp: rtp_timestamp,
                ssrc: t.video_ssrc,
            };
            st.seq = st.seq.wrapping_add(1);
            t.send_rtp(&mut st, header, Some(&rtp::VIDEO_HEADER_EXTENSION), payload);
            sent += 1;
            // Light pacing so a big keyframe doesn't overrun the NIC queue.
            if sent.is_multiple_of(48) {
                std::thread::yield_now();
            }
        });
    }
}

enum VoiceCmd {
    Video(Option<VideoAttrs>),
    Disconnect,
}

impl VoiceShared {
    fn new(info: VoiceConnectInfo, audio_in: Option<IncomingSender>, repaint: Option<RepaintFn>) -> Self {
        VoiceShared {
            info,
            status: Mutex::new(VoiceStatus::Connecting),
            ping_ms: AtomicU32::new(0),
            mode: Mutex::new(None),
            dave_version: AtomicU16::new(0),
            privacy_code: Mutex::new(None),
            ssrc_users: RwLock::new(HashMap::new()),
            video_ssrcs: RwLock::new(HashMap::new()),
            connected_users: Mutex::new(HashSet::new()),
            remote_video: Mutex::new(HashMap::new()),
            decrypt_failures: AtomicU64::new(0),
            video_frames_in: AtomicU64::new(0),
            dave: Mutex::new(None),
            transport: Mutex::new(None),
            audio_in: Mutex::new(audio_in),
            ws_out: Mutex::new(None),
            repaint: Mutex::new(repaint),
            video_attrs: Mutex::new(None),
            rtx_ssrc: AtomicU32::new(0),
            mtu: AtomicU32::new(1100),
        }
    }
}

pub struct VoiceConnection {
    pub shared: Arc<VoiceShared>,
    cmd: mpsc::UnboundedSender<VoiceCmd>,
    task: tokio::task::JoinHandle<()>,
}

impl VoiceConnection {
    /// Start connecting. Must be called inside a tokio runtime.
    pub fn connect(info: VoiceConnectInfo, audio_in: Option<IncomingSender>, repaint: Option<RepaintFn>) -> Self {
        let shared = Arc::new(VoiceShared::new(info, audio_in, repaint));
        let (cmd, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(shared.clone(), rx));
        Self { shared, cmd, task }
    }

    pub fn audio_sink(&self) -> Arc<dyn AudioSink> {
        Arc::new(NetSink(self.shared.clone()))
    }

    pub fn video_sink(&self) -> Arc<dyn VideoSink> {
        Arc::new(NetSink(self.shared.clone()))
    }

    /// Point received audio at a (new) audio engine.
    pub fn set_audio_input(&self, input: Option<IncomingSender>) {
        *self.shared.audio_in.lock() = input;
    }

    pub fn set_video(&self, attrs: Option<VideoAttrs>, mtu: usize) {
        self.shared.mtu.store(mtu as u32, Ordering::Relaxed);
        let _ = self.cmd.send(VoiceCmd::Video(attrs));
    }

    pub fn disconnect(&self) {
        let _ = self.cmd.send(VoiceCmd::Disconnect);
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

impl Drop for VoiceConnection {
    fn drop(&mut self) {
        let _ = self.cmd.send(VoiceCmd::Disconnect);
    }
}

// ---------------------------------------------------------------------------
// Websocket session
// ---------------------------------------------------------------------------

enum End {
    Disconnect,
    Fatal(String),
    Lost { reason: String, resumable: bool },
}

async fn run(shared: Arc<VoiceShared>, mut cmd: mpsc::UnboundedReceiver<VoiceCmd>) {
    let mut attempts = 0u32;
    let mut resume = false;
    // Kept across reconnects: RESUME must acknowledge the last sequence seen.
    let mut seq_ack: i64 = -1;
    loop {
        if !resume {
            seq_ack = -1;
        }
        let end = session(&shared, &mut cmd, resume, &mut seq_ack).await;
        match end {
            End::Disconnect => {
                shared.set_status(VoiceStatus::Disconnected("Left the call".into()));
                break;
            }
            End::Fatal(e) => {
                shared.set_status(VoiceStatus::Disconnected(e));
                break;
            }
            End::Lost { reason, resumable } => {
                // A session that got fully connected earns a fresh retry budget.
                if shared.status() == VoiceStatus::Connected {
                    attempts = 0;
                }
                attempts += 1;
                if attempts > 6 {
                    shared.set_status(VoiceStatus::Disconnected(reason));
                    break;
                }
                warn!("voice connection lost ({reason}), retry {attempts}");
                shared.set_status(VoiceStatus::Reconnecting(reason));
                resume = resumable && shared.transport().is_some();
                if !resume {
                    *shared.transport.lock() = None;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(500 * attempts as u64)) => {}
                    c = cmd.recv() => match c {
                        None | Some(VoiceCmd::Disconnect) => {
                            shared.set_status(VoiceStatus::Disconnected("Left the call".into()));
                            break;
                        }
                        Some(VoiceCmd::Video(v)) => *shared.video_attrs.lock() = v,
                    }
                }
            }
        }
    }
    *shared.transport.lock() = None;
    *shared.ws_out.lock() = None;
    *shared.dave.lock() = None;
}

#[derive(Deserialize)]
struct ReadyStream {
    #[serde(default)]
    ssrc: u32,
    #[serde(default)]
    rtx_ssrc: u32,
}

#[derive(Deserialize)]
struct Ready {
    ssrc: u32,
    ip: String,
    port: u16,
    modes: Vec<String>,
    #[serde(default)]
    streams: Vec<ReadyStream>,
}

#[derive(Deserialize)]
struct SessionDescription {
    #[serde(default)]
    mode: String,
    secret_key: Vec<u8>,
    #[serde(default)]
    dave_protocol_version: u16,
}

/// Bigger kernel buffers so high-bitrate video keyframes (hundreds of
/// packets in one burst) aren't dropped by the OS before we read them.
pub fn tune_socket(sock: &UdpSocket) {
    let s = socket2::SockRef::from(sock);
    let _ = s.set_recv_buffer_size(4 << 20);
    let _ = s.set_send_buffer_size(4 << 20);
}

fn ip_discovery(sock: &UdpSocket, ssrc: u32) -> std::io::Result<(String, u16)> {
    let mut req = [0u8; 74];
    req[0..2].copy_from_slice(&1u16.to_be_bytes());
    req[2..4].copy_from_slice(&70u16.to_be_bytes());
    req[4..8].copy_from_slice(&ssrc.to_be_bytes());
    sock.set_read_timeout(Some(Duration::from_millis(800)))?;
    let mut resp = [0u8; 128];
    let mut last_err = std::io::Error::other("no IP discovery response");
    for _ in 0..6 {
        sock.send(&req)?;
        match sock.recv(&mut resp) {
            Ok(n) if n >= 74 && resp[0..2] == [0, 2] => {
                let addr = &resp[8..72];
                let end = addr.iter().position(|b| *b == 0).unwrap_or(addr.len());
                let ip = String::from_utf8_lossy(&addr[..end]).into_owned();
                let port = u16::from_be_bytes([resp[72], resp[73]]);
                return Ok((ip, port));
            }
            Ok(_) => continue,
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct DaveState {
    pending: HashMap<u16, u16>,
}

async fn session(
    shared: &Arc<VoiceShared>,
    cmd: &mut mpsc::UnboundedReceiver<VoiceCmd>,
    resume: bool,
    seq_ack: &mut i64,
) -> End {
    let info = shared.info.clone();
    let host = info.endpoint.trim_start_matches("wss://").trim_end_matches('/');
    let url = format!("wss://{host}/?v=8");
    let ws = match tokio::time::timeout(Duration::from_secs(15), tokio_tungstenite::connect_async(&url)).await {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(e)) => return End::Lost { reason: format!("voice connect: {e}"), resumable: resume },
        Err(_) => return End::Lost { reason: "voice connect timed out".into(), resumable: resume },
    };
    let (mut tx, mut rx) = ws.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<WsMessage>();
    *shared.ws_out.lock() = Some(out_tx.clone());

    let first = if resume {
        json!({"op": 7, "d": {
            "server_id": info.server_id.to_string(),
            "session_id": info.session_id,
            "token": info.token,
            "seq_ack": *seq_ack,
        }})
    } else {
        json!({"op": 0, "d": {
            "server_id": info.server_id.to_string(),
            "user_id": info.user_id.to_string(),
            "session_id": info.session_id,
            "token": info.token,
            "video": true,
            "streams": [{"type": info.kind.stream_type(), "rid": "100", "quality": 100}],
            "max_dave_protocol_version": davey::DAVE_PROTOCOL_VERSION,
        }})
    };
    if let Err(e) = tx.send(WsMessage::Text(first.to_string().into())).await {
        return End::Lost { reason: e.to_string(), resumable: false };
    }

    let mut heartbeat = tokio::time::interval(Duration::from_secs(3600));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut have_hello = false;
    let mut dave_state = DaveState { pending: HashMap::new() };
    let mut last_ack = Instant::now();
    let mut hb_interval = Duration::from_secs(13);
    let mut pending_udp: Option<PendingSocket> = None;

    loop {
        tokio::select! {
            _ = heartbeat.tick(), if have_hello => {
                if last_ack.elapsed() > hb_interval * 3 {
                    return End::Lost { reason: "voice heartbeat timeout".into(), resumable: true };
                }
                let hb = json!({"op": 3, "d": {"t": now_ms(), "seq_ack": *seq_ack}});
                if tx.send(WsMessage::Text(hb.to_string().into())).await.is_err() {
                    return End::Lost { reason: "send failed".into(), resumable: true };
                }
            }
            out = out_rx.recv() => {
                if let Some(m) = out {
                    if tx.send(m).await.is_err() {
                        return End::Lost { reason: "send failed".into(), resumable: true };
                    }
                }
            }
            c = cmd.recv() => match c {
                None | Some(VoiceCmd::Disconnect) => {
                    let _ = tx.send(WsMessage::Close(None)).await;
                    return End::Disconnect;
                }
                Some(VoiceCmd::Video(v)) => {
                    *shared.video_attrs.lock() = v;
                    if let Some(t) = shared.transport() {
                        send_video_op(shared, &t, v);
                    }
                }
            },
            msg = rx.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return End::Lost { reason: e.to_string(), resumable: true },
                    None => return End::Lost { reason: "voice socket closed".into(), resumable: true },
                };
                match msg {
                    WsMessage::Text(t) => {
                        let v: Value = match serde_json::from_str(t.as_str()) { Ok(v) => v, Err(_) => continue };
                        if let Some(s) = v.get("seq").and_then(Value::as_i64) { *seq_ack = s; }
                        let op = v.get("op").and_then(Value::as_u64).unwrap_or(255);
                        let d = v.get("d").cloned().unwrap_or(Value::Null);
                        match op {
                            8 => {
                                let ms = d.get("heartbeat_interval").and_then(Value::as_f64).unwrap_or(13_750.0);
                                hb_interval = Duration::from_millis(ms as u64);
                                heartbeat = tokio::time::interval_at(tokio::time::Instant::now() + hb_interval / 2, hb_interval);
                                heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                                have_hello = true;
                                last_ack = Instant::now();
                            }
                            2 => {
                                let ready: Ready = match serde_json::from_value(d) {
                                    Ok(r) => r,
                                    Err(e) => return End::Fatal(format!("bad voice READY: {e}")),
                                };
                                match setup_udp(shared, &ready).await {
                                    Ok((sel, sock)) => {
                                        pending_udp = Some(sock);
                                        if tx.send(WsMessage::Text(sel.to_string().into())).await.is_err() {
                                            return End::Lost { reason: "send failed".into(), resumable: false };
                                        }
                                    }
                                    Err(e) => return End::Lost { reason: e, resumable: false },
                                }
                            }
                            4 => {
                                let desc: SessionDescription = match serde_json::from_value(d) {
                                    Ok(x) => x,
                                    Err(e) => return End::Fatal(format!("bad session description: {e}")),
                                };
                                if let Err(e) = on_session_description(shared, desc, pending_udp.take()) {
                                    return End::Fatal(e);
                                }
                            }
                            6 => {
                                last_ack = Instant::now();
                                if let Some(t) = d.get("t").and_then(Value::as_u64) {
                                    shared.ping_ms.store(now_ms().saturating_sub(t) as u32, Ordering::Relaxed);
                                }
                            }
                            9 => {
                                info!("voice session resumed");
                                shared.set_status(VoiceStatus::Connected);
                            }
                            5 => {
                                if let (Some(u), Some(ssrc)) = (
                                    d.get("user_id").and_then(Value::as_str).and_then(|s| s.parse::<u64>().ok()),
                                    d.get("ssrc").and_then(Value::as_u64),
                                ) {
                                    shared.ssrc_users.write().insert(ssrc as u32, u);
                                }
                            }
                            11 => {
                                if let Some(ids) = d.get("user_ids").and_then(Value::as_array) {
                                    let mut c = shared.connected_users.lock();
                                    for id in ids.iter().filter_map(Value::as_str).filter_map(|s| s.parse().ok()) {
                                        c.insert(id);
                                    }
                                }
                            }
                            12 => on_remote_video(shared, &d),
                            13 => {
                                if let Some(u) = d.get("user_id").and_then(Value::as_str).and_then(|s| s.parse::<u64>().ok()) {
                                    shared.connected_users.lock().remove(&u);
                                    shared.ssrc_users.write().retain(|_, v| *v != u);
                                    shared.video_ssrcs.write().retain(|_, v| *v != u);
                                    shared.remote_video.lock().remove(&u);
                                    shared.repaint();
                                }
                            }
                            21 => {
                                let tid = d.get("transition_id").and_then(Value::as_u64).unwrap_or(0) as u16;
                                let pv = d.get("protocol_version").and_then(Value::as_u64).unwrap_or(0) as u16;
                                dave_state.pending.insert(tid, pv);
                                if let Some(s) = shared.dave.lock().as_mut() {
                                    if let Err(e) = s.prepare_transition(tid, pv) {
                                        warn!("dave prepare transition: {e}");
                                    }
                                }
                                if tid == 0 {
                                    execute_transition(shared, &mut dave_state, 0);
                                } else {
                                    shared.send_ws(json!({"op": 23, "d": {"transition_id": tid}}));
                                }
                            }
                            22 => {
                                let tid = d.get("transition_id").and_then(Value::as_u64).unwrap_or(0) as u16;
                                execute_transition(shared, &mut dave_state, tid);
                            }
                            24 => {
                                let epoch = d.get("epoch").and_then(Value::as_u64).unwrap_or(0);
                                let pv = d.get("protocol_version").and_then(Value::as_u64).unwrap_or(0) as u16;
                                if epoch == 1 {
                                    shared.dave_version.store(pv, Ordering::Relaxed);
                                    reinit_dave(shared, pv);
                                }
                            }
                            _ => {}
                        }
                    }
                    WsMessage::Binary(b) => {
                        if b.len() < 3 { continue; }
                        *seq_ack = u16::from_be_bytes([b[0], b[1]]) as i64;
                        on_dave_binary(shared, &mut dave_state, b[2], &b[3..]);
                    }
                    WsMessage::Close(f) => {
                        let code = f.as_ref().map(|f| u16::from(f.code)).unwrap_or(1006);
                        let reason = f.map(|f| f.reason.to_string()).unwrap_or_default();
                        return match code {
                            4004 | 4011 | 4012 | 4016 => End::Fatal(format!("voice rejected ({code}) {reason}")),
                            4014 => End::Fatal("Disconnected from the channel".into()),
                            4006 | 4009 => End::Lost { reason: format!("voice session invalid ({code})"), resumable: false },
                            _ => End::Lost { reason: format!("voice closed ({code}) {reason}"), resumable: true },
                        };
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn setup_udp(shared: &Arc<VoiceShared>, ready: &Ready) -> Result<(Value, PendingSocket), String> {
    let mode = CryptoMode::negotiate(ready.modes.iter().map(String::as_str))
        .ok_or_else(|| format!("no supported encryption mode in {:?}", ready.modes))?;
    let addr = format!("{}:{}", ready.ip, ready.port);
    let ssrc = ready.ssrc;
    let (sock, (my_ip, my_port)) = tokio::task::spawn_blocking(move || -> std::io::Result<_> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        tune_socket(&sock);
        sock.connect(&addr)?;
        let found = ip_discovery(&sock, ssrc)?;
        Ok((sock, found))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("UDP setup failed: {e}"))?;

    let (video_ssrc, rtx_ssrc) = ready
        .streams
        .first()
        .filter(|s| s.ssrc != 0)
        .map(|s| (s.ssrc, s.rtx_ssrc))
        .unwrap_or((ssrc + 1, ssrc + 2));
    shared.rtx_ssrc.store(rtx_ssrc, Ordering::Relaxed);
    *shared.mode.lock() = Some(mode);

    let select = json!({"op": 1, "d": {
        "protocol": "udp",
        "data": {"address": my_ip, "port": my_port, "mode": mode.as_str()},
        "address": my_ip,
        "port": my_port,
        "mode": mode.as_str(),
        "codecs": [
            {"name": "opus", "type": "audio", "priority": 1000, "payload_type": OPUS_PAYLOAD_TYPE},
            {"name": "H264", "type": "video", "priority": 1000, "payload_type": H264_PAYLOAD_TYPE,
             "rtx_payload_type": crate::H264_RTX_PAYLOAD_TYPE, "encode": true, "decode": true}
        ],
        "rtc_connection_id": uuid::Uuid::new_v4().to_string(),
        "experiments": [],
    }});
    // The socket waits for SESSION_DESCRIPTION, which carries the key.
    Ok((select, (sock, ssrc, video_ssrc, mode)))
}

/// UDP socket + (audio ssrc, video ssrc, mode) between READY and SESSION_DESCRIPTION.
type PendingSocket = (UdpSocket, u32, u32, CryptoMode);

fn on_session_description(
    shared: &Arc<VoiceShared>,
    desc: SessionDescription,
    pending: Option<PendingSocket>,
) -> Result<(), String> {
    let Some((sock, audio_ssrc, video_ssrc, mode)) = pending else {
        // Resumed sessions can re-send the description; keep the old transport.
        return Ok(());
    };
    let mode = CryptoMode::parse(&desc.mode).unwrap_or(mode);
    let cipher = Cipher::new(mode, &desc.secret_key).map_err(|e| e.to_string())?;
    let recv_sock = sock.try_clone().map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(Transport {
        socket: sock,
        cipher,
        nonce: AtomicU32::new(rand::random()),
        audio_ssrc,
        video_ssrc,
        audio: Mutex::new(RtpState { seq: rand::random(), timestamp: rand::random(), buf: Vec::with_capacity(1500) }),
        video: Mutex::new(RtpState { seq: rand::random(), timestamp: 0, buf: Vec::with_capacity(1500) }),
        stop: stop.clone(),
        bytes_out: AtomicU64::new(0),
    });
    let rx_cipher = Cipher::new(mode, &desc.secret_key).map_err(|e| e.to_string())?;
    *shared.transport.lock() = Some(transport.clone());
    *shared.mode.lock() = Some(mode);
    shared.ssrc_users.write().insert(audio_ssrc, shared.info.user_id);

    // E2EE
    shared.dave_version.store(desc.dave_protocol_version, Ordering::Relaxed);
    if desc.dave_protocol_version > 0 {
        reinit_dave(shared, desc.dave_protocol_version);
    }

    spawn_receiver(shared.clone(), recv_sock, rx_cipher, stop);

    // Ask the SFU to forward every video stream at full quality.
    shared.send_ws(json!({"op": 15, "d": {"any": 100}}));
    let attrs = *shared.video_attrs.lock();
    send_video_op(shared, &transport, attrs);
    info!("voice connected ({}; DAVE v{})", mode.as_str(), desc.dave_protocol_version);
    shared.set_status(VoiceStatus::Connected);
    Ok(())
}

fn send_video_op(shared: &VoiceShared, t: &Transport, attrs: Option<VideoAttrs>) {
    let rtx = shared.rtx_ssrc.load(Ordering::Relaxed);
    let (active, a) = match attrs {
        Some(a) => (true, a),
        None => (false, VideoAttrs { width: 0, height: 0, fps: 0, bitrate_kbps: 0 }),
    };
    shared.send_ws(json!({"op": 12, "d": {
        "audio_ssrc": t.audio_ssrc,
        "video_ssrc": if active { t.video_ssrc } else { 0 },
        "rtx_ssrc": if active { rtx } else { 0 },
        "streams": [{
            "type": shared.info.kind.stream_type(),
            "rid": "100",
            "ssrc": t.video_ssrc,
            "active": active,
            "quality": 100,
            "rtx_ssrc": rtx,
            "max_bitrate": a.bitrate_kbps.saturating_mul(1000),
            "max_framerate": a.fps,
            "max_resolution": {"type": "fixed", "width": a.width, "height": a.height},
        }]
    }}));
}

fn on_remote_video(shared: &VoiceShared, d: &Value) {
    let Some(user) = d.get("user_id").and_then(Value::as_str).and_then(|s| s.parse::<u64>().ok()) else { return };
    if let Some(a) = d.get("audio_ssrc").and_then(Value::as_u64).filter(|s| *s != 0) {
        shared.ssrc_users.write().insert(a as u32, user);
    }
    let mut vs = shared.video_ssrcs.write();
    vs.retain(|_, u| *u != user);
    let mut any = false;
    if let Some(streams) = d.get("streams").and_then(Value::as_array) {
        for s in streams {
            let active = s.get("active").and_then(Value::as_bool).unwrap_or(true);
            if let Some(ssrc) = s.get("ssrc").and_then(Value::as_u64).filter(|s| *s != 0) {
                if active {
                    vs.insert(ssrc as u32, user);
                    any = true;
                }
            }
        }
    }
    if !any {
        if let Some(v) = d.get("video_ssrc").and_then(Value::as_u64).filter(|s| *s != 0) {
            vs.insert(v as u32, user);
            any = true;
        }
    }
    drop(vs);
    if !any {
        shared.remote_video.lock().remove(&user);
    }
    shared.repaint();
}

// ---------------------------------------------------------------------------
// DAVE (E2EE) handling
// ---------------------------------------------------------------------------

fn reinit_dave(shared: &VoiceShared, version: u16) {
    let mut guard = shared.dave.lock();
    let Some(v) = NonZeroU16::new(version) else {
        if let Some(s) = guard.as_mut() {
            let _ = s.reset();
            s.set_passthrough_mode(true, Some(10));
        }
        return;
    };
    let (uid, cid) = (shared.info.user_id, shared.info.channel_id);
    let kp = match guard.as_mut() {
        Some(s) => s.reinit(v, uid, cid, None).map_err(|e| e.to_string()).and_then(|_| s.create_key_package().map_err(|e| e.to_string())),
        None => match DaveSession::new(v, uid, cid, None) {
            Ok(mut s) => {
                let kp = s.create_key_package().map_err(|e| e.to_string());
                *guard = Some(s);
                kp
            }
            Err(e) => Err(e.to_string()),
        },
    };
    drop(guard);
    match kp {
        Ok(kp) => shared.send_ws_binary(26, &kp),
        Err(e) => warn!("DAVE init failed: {e}"),
    }
}

fn execute_transition(shared: &VoiceShared, st: &mut DaveState, tid: u16) {
    let Some(new_version) = st.pending.remove(&tid) else { return };
    let old = shared.dave_version.swap(new_version, Ordering::Relaxed);
    let mut guard = shared.dave.lock();
    if let Some(s) = guard.as_mut() {
        if let Err(e) = s.execute_transition(tid) {
            warn!("dave execute transition: {e}");
        }
        if tid > 0 && old == 0 && new_version != 0 {
            s.set_passthrough_mode(true, Some(10));
        }
        *shared.privacy_code.lock() = s.voice_privacy_code().map(String::from).filter(|c| !c.is_empty());
    }
    drop(guard);
    shared.repaint();
}

fn on_dave_binary(shared: &VoiceShared, st: &mut DaveState, op: u8, payload: &[u8]) {
    let known: Vec<u64> = {
        let mut v: Vec<u64> = shared.connected_users.lock().iter().copied().collect();
        v.push(shared.info.user_id);
        v
    };
    let version = shared.dave_version.load(Ordering::Relaxed);
    let mut guard = shared.dave.lock();
    let Some(s) = guard.as_mut() else { return };
    match op {
        25 => {
            if let Err(e) = s.set_external_sender(payload) {
                warn!("dave external sender: {e}");
            }
        }
        27 => {
            if payload.is_empty() {
                return;
            }
            let optype = if payload[0] == 1 { ProposalsOperationType::REVOKE } else { ProposalsOperationType::APPEND };
            match s.process_proposals(optype, &payload[1..], Some(&known)) {
                Ok(Some(cw)) => {
                    let mut buf = cw.commit;
                    if let Some(w) = cw.welcome {
                        buf.extend_from_slice(&w);
                    }
                    drop(guard);
                    shared.send_ws_binary(28, &buf);
                }
                Ok(None) => {}
                Err(e) => warn!("dave proposals: {e}"),
            }
        }
        29 | 30 => {
            if payload.len() < 2 {
                return;
            }
            let tid = u16::from_be_bytes([payload[0], payload[1]]);
            let body = &payload[2..];
            let ok = if op == 29 {
                s.process_commit(body).map_err(|e| e.to_string())
            } else {
                s.process_welcome(body, Some(&known)).map(|_| true).map_err(|e| e.to_string())
            };
            match ok {
                Ok(true) => {
                    if let Err(e) = s.prepare_transition(tid, version) {
                        warn!("dave prepare: {e}");
                    }
                    st.pending.insert(tid, version);
                    drop(guard);
                    if tid != 0 {
                        shared.send_ws(json!({"op": 23, "d": {"transition_id": tid}}));
                    } else {
                        execute_transition(shared, st, 0);
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    warn!("dave commit/welcome failed: {e}");
                    drop(guard);
                    shared.send_ws(json!({"op": 31, "d": {"transition_id": tid}}));
                    reinit_dave(shared, version);
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// UDP receive + video decode
// ---------------------------------------------------------------------------

fn spawn_receiver(shared: Arc<VoiceShared>, sock: UdpSocket, cipher: Cipher, stop: Arc<AtomicBool>) {
    #[cfg(feature = "video")]
    let video_tx = Some(spawn_video_decoder(shared.clone(), stop.clone()));
    #[cfg(not(feature = "video"))]
    let video_tx: Option<crossbeam_channel::Sender<(u64, u32, Vec<u8>)>> = None;

    let _ = std::thread::Builder::new().name("rustcord-udp-rx".into()).spawn(move || {
        let _ = sock.set_read_timeout(Some(Duration::from_millis(250)));
        let mut buf = vec![0u8; 65_536];
        let mut depack: HashMap<u32, rtp::H264Depacketizer> = HashMap::new();
        let mut audio_in = shared.audio_in.lock().clone();
        let mut last_refresh = Instant::now();
        while !stop.load(Ordering::Acquire) {
            let n = match sock.recv(&mut buf) {
                Ok(n) => n,
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
            };
            if last_refresh.elapsed() > Duration::from_millis(500) {
                last_refresh = Instant::now();
                audio_in = shared.audio_in.lock().clone();
            }
            let pkt = &mut buf[..n];
            if rtp::is_rtcp(pkt) {
                continue;
            }
            let Some(view) = rtp::parse(pkt) else { continue };
            let range = match cipher.open(pkt, view.aad_len) {
                Ok(r) => r,
                Err(_) => {
                    shared.decrypt_failures.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            };
            let start = range.start + view.ext_body_len;
            if start > range.end {
                continue;
            }
            let payload = &pkt[start..range.end];
            let h = view.header;
            match h.payload_type {
                OPUS_PAYLOAD_TYPE => {
                    let user = shared.ssrc_users.read().get(&h.ssrc).copied().unwrap_or(0);
                    if user == shared.info.user_id {
                        continue;
                    }
                    let Some(data) = e2ee_decrypt(&shared, user, MediaType::AUDIO, payload) else { continue };
                    if let Some(a) = &audio_in {
                        a.push(IncomingAudio { ssrc: h.ssrc, user_id: user, seq: h.seq, payload: data });
                    }
                }
                H264_PAYLOAD_TYPE => {
                    let Some(user) = shared.video_ssrcs.read().get(&h.ssrc).copied() else { continue };
                    let d = depack.entry(h.ssrc).or_default();
                    if let Some(frame) = d.push(h.seq, h.timestamp, h.marker, payload) {
                        let Some(frame) = e2ee_decrypt(&shared, user, MediaType::VIDEO, &frame) else { continue };
                        if let Some(tx) = &video_tx {
                            let _ = tx.try_send((user, h.ssrc, frame));
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

fn e2ee_decrypt(shared: &VoiceShared, user: u64, media: MediaType, data: &[u8]) -> Option<Vec<u8>> {
    let dave_on = shared.dave_version.load(Ordering::Relaxed) != 0;
    if !dave_on || data.len() < 2 || data[data.len() - 2..] != DAVE_MAGIC {
        return Some(data.to_vec());
    }
    if user == 0 {
        return None;
    }
    let mut guard = shared.dave.lock();
    let s = guard.as_mut()?;
    if !s.is_ready() {
        return None;
    }
    match s.decrypt(user, media, data) {
        Ok(v) => Some(v),
        Err(_) => {
            shared.decrypt_failures.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

#[cfg(feature = "video")]
fn spawn_video_decoder(shared: Arc<VoiceShared>, stop: Arc<AtomicBool>) -> crossbeam_channel::Sender<(u64, u32, Vec<u8>)> {
    use crate::video::codec::H264Decoder;
    let (tx, rx) = crossbeam_channel::bounded::<(u64, u32, Vec<u8>)>(16);
    let _ = std::thread::Builder::new().name("rustcord-video-dec".into()).spawn(move || {
        let mut decoders: HashMap<u32, (H264Decoder, bool)> = HashMap::new();
        let mut serial = 0u64;
        let mut rgba = Vec::new();
        while !stop.load(Ordering::Acquire) {
            let Ok((user, ssrc, frame)) = rx.recv_timeout(Duration::from_millis(250)) else { continue };
            // If we fell behind, wait for the next keyframe instead of
            // decoding a backlog (keeps latency bounded).
            let behind = rx.len() > 8;
            let entry = match decoders.get_mut(&ssrc) {
                Some(e) => e,
                None => match H264Decoder::new() {
                    Ok(d) => decoders.entry(ssrc).or_insert((d, true)),
                    Err(_) => continue,
                },
            };
            if behind {
                entry.1 = true;
            }
            let key = rtp::h264_is_keyframe(&frame);
            if entry.1 && !key {
                continue;
            }
            entry.1 = false;
            match entry.0.decode_rgba(&frame, &mut rgba) {
                Ok(Some((w, h))) => {
                    serial += 1;
                    shared.video_frames_in.fetch_add(1, Ordering::Relaxed);
                    shared.remote_video.lock().insert(
                        user,
                        VideoFrame { width: w as u32, height: h as u32, rgba: Arc::new(std::mem::take(&mut rgba)), serial },
                    );
                    shared.repaint();
                }
                Ok(None) => {}
                Err(_) => entry.1 = true,
            }
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::CryptoMode;

    /// Stand-in for Discord's SFU: decrypts each packet, rewrites the SSRC
    /// as if another user sent it, re-encrypts and sends it back.
    fn reflector(sock: UdpSocket, key: [u8; 32], map: HashMap<u32, u32>, stop: Arc<AtomicBool>) {
        std::thread::spawn(move || {
            let cipher = Cipher::new(CryptoMode::Aes256Gcm, &key).unwrap();
            sock.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
            let mut buf = vec![0u8; 65_536];
            let mut counter = 1_000u32;
            while !stop.load(Ordering::Relaxed) {
                let Ok(n) = sock.recv(&mut buf) else { continue };
                let pkt = &mut buf[..n];
                let view = rtp::parse(pkt).unwrap();
                let range = cipher.open(pkt, view.aad_len).unwrap();
                let mut out = pkt[..view.aad_len].to_vec();
                let new_ssrc = map[&view.header.ssrc];
                out[8..12].copy_from_slice(&new_ssrc.to_be_bytes());
                out.extend_from_slice(&pkt[range]);
                cipher.seal(&mut out, view.aad_len, counter).unwrap();
                counter += 1;
                sock.send(&out).unwrap();
            }
        });
    }

    #[test]
    fn audio_and_video_roundtrip_through_encrypted_udp() {
        let key = [7u8; 32];
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        tune_socket(&server);
        tune_socket(&client);
        client.connect(server.local_addr().unwrap()).unwrap();
        server.connect(client.local_addr().unwrap()).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        reflector(server, key, HashMap::from([(111, 222), (112, 223)]), stop.clone());

        let info = VoiceConnectInfo {
            endpoint: String::new(),
            token: String::new(),
            session_id: String::new(),
            server_id: 1,
            channel_id: 2,
            user_id: 1,
            kind: ConnKind::Voice,
        };
        let (tx, rx) = crate::engine::test_incoming();
        let shared = Arc::new(VoiceShared::new(info, Some(tx), None));
        *shared.transport.lock() = Some(Arc::new(Transport {
            socket: client.try_clone().unwrap(),
            cipher: Cipher::new(CryptoMode::Aes256Gcm, &key).unwrap(),
            nonce: AtomicU32::new(0),
            audio_ssrc: 111,
            video_ssrc: 112,
            audio: Mutex::new(RtpState::default()),
            video: Mutex::new(RtpState::default()),
            stop: stop.clone(),
            bytes_out: AtomicU64::new(0),
        }));
        shared.ssrc_users.write().insert(111, 1);
        shared.ssrc_users.write().insert(222, 42);
        shared.video_ssrcs.write().insert(223, 42);
        spawn_receiver(shared.clone(), client, Cipher::new(CryptoMode::Aes256Gcm, &key).unwrap(), stop.clone());

        // --- audio: real Opus frames, 510 kb/s stereo ---
        let sink = NetSink(shared.clone());
        let mut enc = crate::opus::Encoder::new(2, crate::settings::Application::Music).unwrap();
        enc.apply(&crate::settings::AudioSettings::default()).unwrap();
        let pcm: Vec<f32> = (0..1920).map(|i| ((i as f32) * 0.05).sin() * 0.3).collect();
        let mut pkt = vec![0u8; crate::opus::MAX_PACKET];
        let mut sent = Vec::new();
        for _ in 0..3 {
            let n = enc.encode(&pcm, &mut pkt).unwrap();
            sent.push(pkt[..n].to_vec());
            sink.send_opus(&pkt[..n], 960);
        }
        for (i, expect) in sent.iter().enumerate() {
            let got = rx.recv_timeout(Duration::from_secs(3)).expect("audio packet");
            assert_eq!(got.ssrc, 222);
            assert_eq!(got.user_id, 42);
            assert_eq!(&got.payload, expect, "packet {i}");
        }

        // --- video: a real 1280x720 H.264 keyframe, fragmented with FU-A ---
        #[cfg(feature = "video")]
        {
            use crate::video::codec::H264Encoder;
            use crate::video::convert::I420;
            let vs = crate::settings::VideoSettings { bitrate_kbps: 8_000, ..Default::default() };
            let mut venc = H264Encoder::new(1280, 720, 30, &vs, false).unwrap();
            let mut frame = I420::new(1280, 720);
            frame.data.iter_mut().enumerate().for_each(|(i, p)| *p = (i % 251) as u8);
            let au = venc.encode(&frame).unwrap().to_vec();
            assert!(au.len() > 2_000, "keyframe should need fragmentation");
            sink.send_frame(&au, 90_000);
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(f) = shared.remote_video.lock().get(&42).cloned() {
                    assert_eq!((f.width, f.height), (1280, 720));
                    assert_eq!(f.rgba.len(), 1280 * 720 * 4);
                    break;
                }
                assert!(Instant::now() < deadline, "no decoded video frame");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        assert_eq!(shared.decrypt_failures.load(Ordering::Relaxed), 0);
        stop.store(true, Ordering::Relaxed);
    }
}
