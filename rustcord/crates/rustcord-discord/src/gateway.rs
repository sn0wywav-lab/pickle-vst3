//! Main Discord gateway connection.
//!
//! Performance notes:
//! * `zlib-stream` transport compression with a single persistent inflater
//!   and a reused output buffer (READY payloads shrink ~10x on the wire).
//! * Two-phase parsing: only the envelope is parsed eagerly; the `d` payload
//!   is kept as a borrowed `RawValue` and is deserialized into a typed struct
//!   only for dispatch types we actually use. Everything else is skipped
//!   without allocating a JSON tree.

use crate::model::*;
use crate::props::ClientProperties;
use crate::snowflake::Snowflake;
use flate2::{Decompress, FlushDecompress};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval_at, Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, info, warn};

pub const GATEWAY_URL: &str = "wss://gateway.discord.gg";
const GATEWAY_QUERY: &str = "/?v=9&encoding=json&compress=zlib-stream";

/// Capabilities sent in IDENTIFY: lazy user notes, versioned read states /
/// guild settings and non-channel read states. We deliberately avoid the
/// capabilities that change the READY layout (user dedupe, proto settings,
/// client state v2) or that rotate the token (auth token refresh); the model
/// still copes with either layout.
pub const DEFAULT_CAPABILITIES: u64 = (1 << 0) | (1 << 2) | (1 << 3) | (1 << 7);

#[derive(Debug, Clone, PartialEq)]
pub enum GatewayStatus {
    Connecting,
    Connected,
    Reconnecting(String),
    /// Non-recoverable (bad token, ...). The task has exited.
    Fatal(String),
}

#[derive(Debug)]
pub enum GatewayEvent {
    Status(GatewayStatus),
    Ready(Box<Ready>),
    ReadySupplemental(Box<ReadySupplemental>),
    Resumed,
    GuildCreate(Box<GuildPayload>),
    GuildUpdate(Box<GuildPayload>),
    GuildDelete(GuildDelete),
    ChannelCreate(Box<Channel>),
    ChannelUpdate(Box<Channel>),
    ChannelDelete(Box<Channel>),
    MessageCreate(Box<Message>),
    MessageUpdate(Box<MessageUpdate>),
    MessageDelete(MessageDelete),
    VoiceStateUpdate(Box<VoiceState>),
    VoiceServerUpdate(VoiceServerUpdate),
    TypingStart(TypingStart),
    StreamCreate(StreamCreate),
    StreamServerUpdate(StreamServerUpdate),
    StreamDelete(StreamDelete),
}

#[derive(Debug, Clone)]
pub enum GatewayCommand {
    UpdateVoiceState {
        guild_id: Option<Snowflake>,
        channel_id: Option<Snowflake>,
        self_mute: bool,
        self_deaf: bool,
        self_video: bool,
    },
    /// Go Live: start streaming in the voice channel we're in.
    StreamCreate { guild_id: Option<Snowflake>, channel_id: Snowflake },
    StreamSetPaused { stream_key: String, paused: bool },
    StreamDelete { stream_key: String },
    /// Start watching someone else's stream.
    StreamWatch { stream_key: String },
    Shutdown,
}

pub type EventSink = Arc<dyn Fn(GatewayEvent) + Send + Sync>;

pub struct GatewayHandle {
    tx: mpsc::UnboundedSender<GatewayCommand>,
    task: tokio::task::JoinHandle<()>,
}

impl GatewayHandle {
    /// Spawn the gateway task on the current tokio runtime.
    pub fn spawn(token: String, props: ClientProperties, sink: EventSink) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(token, props, rx, sink));
        Self { tx, task }
    }

    pub fn send(&self, cmd: GatewayCommand) {
        let _ = self.tx.send(cmd);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(GatewayCommand::Shutdown);
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

impl Drop for GatewayHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(GatewayCommand::Shutdown);
    }
}

// ---------------------------------------------------------------------------
// zlib-stream inflater
// ---------------------------------------------------------------------------

const ZLIB_SUFFIX: [u8; 4] = [0x00, 0x00, 0xFF, 0xFF];

/// Incremental zlib-stream decoder. Discord compresses the whole connection as
/// one zlib stream and flushes with Z_SYNC_FLUSH after each message.
pub struct Inflater {
    z: Decompress,
    pending: Vec<u8>,
    out: Vec<u8>,
}

impl Default for Inflater {
    fn default() -> Self {
        Self::new()
    }
}

impl Inflater {
    pub fn new() -> Self {
        Self {
            z: Decompress::new(true),
            pending: Vec::with_capacity(64 * 1024),
            out: Vec::with_capacity(256 * 1024),
        }
    }

    /// Feed one websocket frame; returns the decompressed message once the
    /// sync-flush suffix has been seen.
    pub fn push(&mut self, data: &[u8]) -> Result<Option<&[u8]>, String> {
        self.pending.extend_from_slice(data);
        if self.pending.len() < 4 || self.pending[self.pending.len() - 4..] != ZLIB_SUFFIX {
            return Ok(None);
        }
        self.out.clear();
        let mut input: &[u8] = &self.pending;
        loop {
            if self.out.capacity() - self.out.len() < 32 * 1024 {
                self.out.reserve((input.len() * 4).max(64 * 1024));
            }
            let in_before = self.z.total_in();
            let out_before = self.z.total_out();
            self.z
                .decompress_vec(input, &mut self.out, FlushDecompress::Sync)
                .map_err(|e| format!("zlib: {e}"))?;
            let consumed = (self.z.total_in() - in_before) as usize;
            let produced = (self.z.total_out() - out_before) as usize;
            input = &input[consumed..];
            let out_full = self.out.len() == self.out.capacity();
            if input.is_empty() && !out_full {
                break;
            }
            if consumed == 0 && produced == 0 && !out_full {
                return Err("zlib: no progress".into());
            }
        }
        self.pending.clear();
        // Don't keep a giant READY-sized buffer around forever.
        if self.pending.capacity() > 4 * 1024 * 1024 {
            self.pending.shrink_to(64 * 1024);
        }
        Ok(Some(&self.out))
    }
}

// ---------------------------------------------------------------------------
// Envelope parsing
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Envelope<'a> {
    op: u8,
    #[serde(borrow, default)]
    d: Option<&'a RawValue>,
    #[serde(default)]
    s: Option<u64>,
    #[serde(borrow, default)]
    t: Option<Cow<'a, str>>,
}

/// Parse a dispatch payload into a typed event. Unknown event names return
/// `Ok(None)` without touching the payload.
pub fn parse_dispatch(name: &str, d: &RawValue) -> Result<Option<GatewayEvent>, serde_json::Error> {
    let s = d.get();
    use serde_json::from_str as p;
    Ok(Some(match name {
        "READY" => GatewayEvent::Ready(p(s)?),
        "READY_SUPPLEMENTAL" => GatewayEvent::ReadySupplemental(p(s)?),
        "RESUMED" => GatewayEvent::Resumed,
        "GUILD_CREATE" => GatewayEvent::GuildCreate(p(s)?),
        "GUILD_UPDATE" => GatewayEvent::GuildUpdate(p(s)?),
        "GUILD_DELETE" => GatewayEvent::GuildDelete(p(s)?),
        "CHANNEL_CREATE" => GatewayEvent::ChannelCreate(p(s)?),
        "CHANNEL_UPDATE" => GatewayEvent::ChannelUpdate(p(s)?),
        "CHANNEL_DELETE" => GatewayEvent::ChannelDelete(p(s)?),
        "THREAD_CREATE" => GatewayEvent::ChannelCreate(p(s)?),
        "MESSAGE_CREATE" => GatewayEvent::MessageCreate(p(s)?),
        "MESSAGE_UPDATE" => GatewayEvent::MessageUpdate(p(s)?),
        "MESSAGE_DELETE" => GatewayEvent::MessageDelete(p(s)?),
        "VOICE_STATE_UPDATE" => GatewayEvent::VoiceStateUpdate(p(s)?),
        "VOICE_SERVER_UPDATE" => GatewayEvent::VoiceServerUpdate(p(s)?),
        "TYPING_START" => GatewayEvent::TypingStart(p(s)?),
        "STREAM_CREATE" => GatewayEvent::StreamCreate(p(s)?),
        "STREAM_SERVER_UPDATE" => GatewayEvent::StreamServerUpdate(p(s)?),
        "STREAM_DELETE" => GatewayEvent::StreamDelete(p(s)?),
        _ => return Ok(None),
    }))
}

// ---------------------------------------------------------------------------
// Connection loop
// ---------------------------------------------------------------------------

enum Exit {
    Shutdown,
    Fatal(String),
    Reconnect { resumable: bool, reason: String },
}

struct Session {
    id: String,
    resume_url: String,
}

async fn run(
    token: String,
    props: ClientProperties,
    mut cmds: mpsc::UnboundedReceiver<GatewayCommand>,
    sink: EventSink,
) {
    let mut session: Option<Session> = None;
    let mut seq: Option<u64> = None;
    let mut backoff = Duration::from_millis(500);
    let mut first = true;
    // Voice state is re-sent after a fresh IDENTIFY so a reconnect doesn't drop calls.
    let mut last_voice: Option<GatewayCommand> = None;

    loop {
        sink(GatewayEvent::Status(if first {
            GatewayStatus::Connecting
        } else {
            GatewayStatus::Reconnecting("reconnecting".into())
        }));
        first = false;

        let base = session
            .as_ref()
            .map(|s| s.resume_url.clone())
            .unwrap_or_else(|| GATEWAY_URL.to_string());
        let url = format!("{}{}", base.trim_end_matches('/'), GATEWAY_QUERY);

        let mut conn = Conn {
            token: &token,
            props: &props,
            session: &mut session,
            seq: &mut seq,
            sink: &sink,
            last_voice: &mut last_voice,
            healthy: false,
        };
        let exit = conn.run(&url, &mut cmds).await;
        let healthy = conn.healthy;
        match exit {
            Exit::Shutdown => return,
            Exit::Fatal(msg) => {
                warn!("gateway fatal: {msg}");
                sink(GatewayEvent::Status(GatewayStatus::Fatal(msg)));
                return;
            }
            Exit::Reconnect { resumable, reason } => {
                info!("gateway reconnect (resumable={resumable}): {reason}");
                if !resumable {
                    session = None;
                    seq = None;
                }
                if healthy {
                    backoff = Duration::from_millis(500);
                }
                sink(GatewayEvent::Status(GatewayStatus::Reconnecting(reason)));
                let jitter = Duration::from_millis(rand::random::<u64>() % 750);
                // Allow shutdown during the backoff sleep.
                tokio::select! {
                    _ = tokio::time::sleep(backoff + jitter) => {}
                    cmd = cmds.recv() => match cmd {
                        None | Some(GatewayCommand::Shutdown) => return,
                        Some(c @ GatewayCommand::UpdateVoiceState { .. }) => last_voice = Some(c),
                        // Stream commands are meaningless while disconnected.
                        Some(_) => {}
                    }
                }
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        }
    }
}

struct Conn<'a> {
    token: &'a str,
    props: &'a ClientProperties,
    session: &'a mut Option<Session>,
    seq: &'a mut Option<u64>,
    sink: &'a EventSink,
    last_voice: &'a mut Option<GatewayCommand>,
    healthy: bool,
}

fn command_payload(cmd: &GatewayCommand) -> Option<String> {
    let v = match cmd {
        GatewayCommand::StreamCreate { guild_id, channel_id } => serde_json::json!({
            "op": 18,
            "d": {
                "type": if guild_id.is_some() { "guild" } else { "call" },
                "guild_id": guild_id,
                "channel_id": channel_id,
                "preferred_region": serde_json::Value::Null,
            }
        }),
        GatewayCommand::StreamSetPaused { stream_key, paused } => {
            serde_json::json!({"op": 22, "d": {"stream_key": stream_key, "paused": paused}})
        }
        GatewayCommand::StreamDelete { stream_key } => serde_json::json!({"op": 19, "d": {"stream_key": stream_key}}),
        GatewayCommand::StreamWatch { stream_key } => serde_json::json!({"op": 20, "d": {"stream_key": stream_key}}),
        _ => return voice_payload(cmd),
    };
    Some(v.to_string())
}

fn voice_payload(cmd: &GatewayCommand) -> Option<String> {
    match cmd {
        GatewayCommand::UpdateVoiceState {
            guild_id,
            channel_id,
            self_mute,
            self_deaf,
            self_video,
        } => Some(
            serde_json::json!({
                "op": 4,
                "d": {
                    "guild_id": guild_id,
                    "channel_id": channel_id,
                    "self_mute": self_mute,
                    "self_deaf": self_deaf,
                    "self_video": self_video
                }
            })
            .to_string(),
        ),
        _ => None,
    }
}

fn close_exit(code: u16, reason: &str) -> Exit {
    match code {
        4004 => Exit::Fatal("Authentication failed: the token is invalid or expired.".into()),
        4010..=4014 => Exit::Fatal(format!("Gateway rejected the session ({code}): {reason}")),
        4007 | 4009 => Exit::Reconnect {
            resumable: false,
            reason: format!("session expired ({code})"),
        },
        _ => Exit::Reconnect {
            resumable: true,
            reason: format!("closed ({code}) {reason}"),
        },
    }
}

impl Conn<'_> {
    fn identify(&self) -> String {
        serde_json::json!({
            "op": 2,
            "d": {
                "token": self.token,
                "capabilities": DEFAULT_CAPABILITIES,
                "properties": self.props.to_json(),
                "presence": {"status": "online", "since": 0, "activities": [], "afk": false},
                "compress": false,
                "client_state": {"guild_versions": {}}
            }
        })
        .to_string()
    }

    fn resume(&self, session_id: &str) -> String {
        serde_json::json!({
            "op": 6,
            "d": {"token": self.token, "session_id": session_id, "seq": *self.seq}
        })
        .to_string()
    }

    async fn run(&mut self, url: &str, cmds: &mut mpsc::UnboundedReceiver<GatewayCommand>) -> Exit {
        let mut req = match url.into_client_request() {
            Ok(r) => r,
            Err(e) => return Exit::Fatal(format!("bad gateway url: {e}")),
        };
        if let Ok(v) = self.props.browser_user_agent.parse() {
            req.headers_mut().insert("User-Agent", v);
        }
        req.headers_mut()
            .insert("Origin", "https://discord.com".parse().expect("static header"));

        let cfg = WebSocketConfig::default()
            .max_message_size(Some(512 << 20))
            .max_frame_size(Some(128 << 20));
        let connect = tokio_tungstenite::connect_async_with_config(req, Some(cfg), true);
        let ws = match tokio::time::timeout(Duration::from_secs(20), connect).await {
            Ok(Ok((ws, _))) => ws,
            Ok(Err(e)) => {
                return Exit::Reconnect {
                    resumable: true,
                    reason: format!("connect failed: {e}"),
                }
            }
            Err(_) => {
                return Exit::Reconnect {
                    resumable: true,
                    reason: "connect timed out".into(),
                }
            }
        };
        let (mut tx, mut rx) = ws.split();
        let mut inflater = Inflater::new();

        // Placeholder interval until HELLO arrives.
        let mut heartbeat = interval_at(Instant::now() + Duration::from_secs(3600), Duration::from_secs(3600));
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut got_hello = false;
        let mut acked = true;

        loop {
            let mut outgoing: Vec<String> = Vec::new();
            let mut exit: Option<Exit> = None;

            tokio::select! {
                _ = heartbeat.tick(), if got_hello => {
                    if !acked {
                        return Exit::Reconnect { resumable: true, reason: "heartbeat not acknowledged".into() };
                    }
                    acked = false;
                    outgoing.push(serde_json::json!({"op": 1, "d": *self.seq}).to_string());
                }
                cmd = cmds.recv() => match cmd {
                    None | Some(GatewayCommand::Shutdown) => {
                        let _ = tx.send(WsMessage::Close(None)).await;
                        return Exit::Shutdown;
                    }
                    Some(c) => {
                        if let Some(p) = command_payload(&c) { outgoing.push(p); }
                        if matches!(c, GatewayCommand::UpdateVoiceState { .. }) {
                            *self.last_voice = Some(c);
                        }
                    }
                },
                msg = rx.next() => {
                    let data: Option<Cow<'_, [u8]>> = match msg {
                        Some(Ok(WsMessage::Binary(b))) => match inflater.push(&b) {
                            Ok(Some(text)) => Some(Cow::Borrowed(text)),
                            Ok(None) => None,
                            Err(e) => return Exit::Reconnect { resumable: false, reason: e },
                        },
                        Some(Ok(WsMessage::Text(t))) => Some(Cow::Owned(t.as_str().as_bytes().to_vec())),
                        Some(Ok(WsMessage::Close(frame))) => {
                            let (code, reason) = frame
                                .map(|f| (u16::from(f.code), f.reason.to_string()))
                                .unwrap_or((u16::from(CloseCode::Abnormal), String::new()));
                            return close_exit(code, &reason);
                        }
                        Some(Ok(_)) => None,
                        Some(Err(e)) => return Exit::Reconnect { resumable: true, reason: format!("socket error: {e}") },
                        None => return Exit::Reconnect { resumable: true, reason: "socket closed".into() },
                    };
                    if let Some(data) = data {
                        let env: Envelope<'_> = match serde_json::from_slice(&data) {
                            Ok(e) => e,
                            Err(e) => { warn!("bad gateway frame: {e}"); continue; }
                        };
                        if let Some(s) = env.s { *self.seq = Some(s); }
                        match env.op {
                            0 => {
                                let name = env.t.as_deref().unwrap_or("");
                                if let Some(d) = env.d {
                                    match parse_dispatch(name, d) {
                                        Ok(Some(ev)) => {
                                            match &ev {
                                                GatewayEvent::Ready(r) => {
                                                    *self.session = Some(Session {
                                                        id: r.session_id.clone(),
                                                        resume_url: r.resume_gateway_url.clone().unwrap_or_else(|| GATEWAY_URL.into()),
                                                    });
                                                    self.healthy = true;
                                                    (self.sink)(GatewayEvent::Status(GatewayStatus::Connected));
                                                }
                                                GatewayEvent::Resumed => {
                                                    self.healthy = true;
                                                    (self.sink)(GatewayEvent::Status(GatewayStatus::Connected));
                                                }
                                                _ => {}
                                            }
                                            (self.sink)(ev);
                                        }
                                        Ok(None) => {}
                                        Err(e) => warn!("failed to parse {name}: {e}"),
                                    }
                                }
                            }
                            1 => outgoing.push(serde_json::json!({"op": 1, "d": *self.seq}).to_string()),
                            7 => exit = Some(Exit::Reconnect { resumable: true, reason: "server requested reconnect".into() }),
                            9 => {
                                let resumable = env.d.map(|d| d.get() == "true").unwrap_or(false);
                                exit = Some(Exit::Reconnect { resumable, reason: "invalid session".into() });
                            }
                            10 => {
                                #[derive(Deserialize)]
                                struct Hello { heartbeat_interval: u64 }
                                let interval = env.d
                                    .and_then(|d| serde_json::from_str::<Hello>(d.get()).ok())
                                    .map(|h| h.heartbeat_interval)
                                    .unwrap_or(41_250);
                                let period = Duration::from_millis(interval);
                                let jitter = period.mul_f64(rand::random::<f64>());
                                heartbeat = interval_at(Instant::now() + jitter, period);
                                heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
                                got_hello = true;
                                match self.session.as_ref() {
                                    Some(s) if self.seq.is_some() => {
                                        debug!("resuming session");
                                        outgoing.push(self.resume(&s.id));
                                    }
                                    _ => {
                                        outgoing.push(self.identify());
                                        // Re-join the voice channel we were in, if any.
                                        if let Some(p) = self.last_voice.as_ref().and_then(voice_payload) {
                                            outgoing.push(p);
                                        }
                                    }
                                }
                            }
                            11 => acked = true,
                            _ => {}
                        }
                    }
                }
            }

            for msg in outgoing {
                if let Err(e) = tx.send(WsMessage::Text(msg.into())).await {
                    return Exit::Reconnect {
                        resumable: true,
                        reason: format!("send failed: {e}"),
                    };
                }
            }
            if let Some(e) = exit {
                let _ = tx.send(WsMessage::Close(None)).await;
                return e;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    #[test]
    fn inflater_handles_split_frames_and_multiple_messages() {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        let msg1 = br#"{"op":10,"d":{"heartbeat_interval":41250}}"#;
        enc.write_all(msg1).unwrap();
        enc.flush().unwrap(); // sync flush => 00 00 ff ff
        let part1 = enc.get_ref().clone();
        let big = format!(r#"{{"op":0,"t":"X","s":1,"d":"{}"}}"#, "a".repeat(300_000));
        enc.write_all(big.as_bytes()).unwrap();
        enc.flush().unwrap();
        let part2 = enc.get_ref()[part1.len()..].to_vec();

        let mut inf = Inflater::new();
        assert_eq!(inf.push(&part1).unwrap().unwrap(), msg1);
        let (a, b) = part2.split_at(part2.len() / 2);
        assert!(inf.push(a).unwrap().is_none());
        assert_eq!(inf.push(b).unwrap().unwrap(), big.as_bytes());
    }

    #[test]
    fn dispatch_parsing_skips_unknown() {
        let raw: Box<RawValue> = serde_json::from_str(r#"{"anything":[1,2,3]}"#).unwrap();
        assert!(parse_dispatch("PRESENCE_UPDATE", &raw).unwrap().is_none());
        let raw: Box<RawValue> = serde_json::from_str(
            r#"{"id":"1","channel_id":"2","author":{"id":"3","username":"x"},"content":"yo","timestamp":""}"#,
        )
        .unwrap();
        match parse_dispatch("MESSAGE_CREATE", &raw).unwrap() {
            Some(GatewayEvent::MessageCreate(m)) => assert_eq!(m.content, "yo"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn stream_command_payloads() {
        let p = command_payload(&GatewayCommand::StreamCreate { guild_id: Some(Snowflake(1)), channel_id: Snowflake(2) }).unwrap();
        let v: serde_json::Value = serde_json::from_str(&p).unwrap();
        assert_eq!(v["op"], 18);
        assert_eq!(v["d"]["type"], "guild");
        assert_eq!(v["d"]["channel_id"], "2");
        let p = command_payload(&GatewayCommand::StreamWatch { stream_key: "guild:1:2:3".into() }).unwrap();
        assert!(p.contains("\"op\":20"));
        let raw: Box<RawValue> = serde_json::from_str(r#"{"stream_key":"guild:1:2:3","rtc_server_id":"999","viewer_ids":[]}"#).unwrap();
        assert!(matches!(parse_dispatch("STREAM_CREATE", &raw).unwrap(), Some(GatewayEvent::StreamCreate(s)) if s.rtc_server_id.as_deref() == Some("999")));
    }

    #[test]
    fn envelope_borrows() {
        let s = br#"{"op":0,"s":42,"t":"TYPING_START","d":{"channel_id":"1","user_id":"2"}}"#;
        let env: Envelope<'_> = serde_json::from_slice(s).unwrap();
        assert_eq!(env.s, Some(42));
        assert_eq!(env.t.as_deref(), Some("TYPING_START"));
    }
}
