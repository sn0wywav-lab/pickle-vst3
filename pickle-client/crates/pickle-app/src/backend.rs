//! Everything that talks to the network or to media devices runs here, on a
//! small tokio runtime, so the UI thread only ever renders.

use eframe::egui;
use pickle_discord::rest::{fetch_bytes, plain_client};
use pickle_discord::{
    ClientProperties, GatewayCommand, GatewayEvent, GatewayHandle, Message, Rest, RestError, Snowflake, User,
    VoiceServerUpdate, VoiceState,
};
use pickle_voice::connection::{VideoAttrs, VoiceShared};
use pickle_voice::devices::{self, DeviceInfo};
use pickle_voice::engine::EngineShared;
use pickle_voice::video::{CameraInfo, CameraMode};
use pickle_voice::{AudioEngine, AudioSettings, VideoSettings, VoiceConnectInfo, VoiceConnection};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use tracing::{info, warn};

#[cfg(feature = "camera")]
use pickle_voice::video::camera::{CameraStats, CameraStream};

pub enum Cmd {
    Login { token: String, props: ClientProperties },
    Logout,
    FetchMessages { channel: Snowflake, before: Option<Snowflake> },
    SendMessage { channel: Snowflake, content: String, nonce: Snowflake, reply_to: Option<Snowflake> },
    EditMessage { channel: Snowflake, message: Snowflake, content: String },
    DeleteMessage { channel: Snowflake, message: Snowflake },
    Typing(Snowflake),
    FetchImage { key: String, url: String, max: u32 },
    FetchText { key: String, url: String },
    PluginFetch { plugin: String, id: u32, url: String, method: String, body: Option<String> },
    JoinVoice { guild: Option<Snowflake>, channel: Snowflake },
    LeaveVoice,
    SetMute(bool),
    SetDeaf(bool),
    SetCamera(bool),
    Audio(AudioSettings),
    Video(VideoSettings),
    UserVolume(u64, f32),
    MicTest(bool),
    CameraPreview(bool),
    ListDevices,
    ProbeCamera(u32),
}

pub enum Event {
    LoggedIn(User),
    /// `bad_token` is false for transient failures (offline, timeouts).
    LoginFailed { message: String, bad_token: bool },
    Gateway(GatewayEvent),
    History { channel: Snowflake, result: Result<Vec<Message>, String>, requested: usize },
    SendFailed { channel: Snowflake, nonce: Snowflake, error: String },
    Image { key: String, result: Result<egui::ColorImage, String> },
    Text { key: String, result: Result<String, String> },
    PluginFetchDone { plugin: String, id: u32, result: Result<(u16, String), String> },
    Media(MediaHandles),
    Devices { inputs: Vec<DeviceInfo>, outputs: Vec<DeviceInfo>, cameras: Vec<CameraInfo> },
    CameraModes(Result<Vec<CameraMode>, String>),
    Notice(String),
}

/// Live handles the UI reads every frame (meters, speaking, video frames).
#[derive(Clone, Default)]
pub struct MediaHandles {
    pub engine: Option<Arc<EngineShared>>,
    pub voice: Option<Arc<VoiceShared>>,
    #[cfg(feature = "camera")]
    pub camera: Option<Arc<CameraStats>>,
    pub guild: Option<Snowflake>,
    pub channel: Option<Snowflake>,
    pub muted: bool,
    pub deafened: bool,
    pub camera_on: bool,
    pub mic_test: bool,
}

#[derive(Clone)]
struct Emitter {
    tx: crossbeam_channel::Sender<Event>,
    ctx: egui::Context,
}

impl Emitter {
    fn emit(&self, e: Event) {
        let _ = self.tx.send(e);
        self.ctx.request_repaint();
    }
}

pub struct Backend {
    tx: mpsc::UnboundedSender<Cmd>,
    _rt: tokio::runtime::Runtime,
}

impl Backend {
    pub fn start(
        ctx: egui::Context,
        audio: AudioSettings,
        video: VideoSettings,
        volumes: HashMap<u64, f32>,
        props: ClientProperties,
    ) -> (Self, crossbeam_channel::Receiver<Event>) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("pickle-net")
            .enable_all()
            .build()
            .expect("tokio runtime");
        let (tx, rx) = mpsc::unbounded_channel();
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded();
        let emitter = Emitter { tx: ev_tx, ctx: ctx.clone() };
        rt.spawn(run(rx, emitter, ctx, audio, video, volumes, props));
        (Self { tx, _rt: rt }, ev_rx)
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

enum Internal {
    Gateway(GatewayEvent),
    NewGateway(GatewayHandle),
    CameraMode(Option<CameraMode>),
}

struct State {
    out: Emitter,
    ctx: egui::Context,
    rest: Option<Rest>,
    plain: reqwest::Client,
    gateway: Option<GatewayHandle>,
    me: Snowflake,
    token: String,
    images: Arc<Semaphore>,
    internal: mpsc::UnboundedSender<Internal>,
    voice: VoiceMgr,
}

async fn run(
    mut rx: mpsc::UnboundedReceiver<Cmd>,
    out: Emitter,
    ctx: egui::Context,
    audio: AudioSettings,
    video: VideoSettings,
    volumes: HashMap<u64, f32>,
    props: ClientProperties,
) {
    let (itx, mut irx) = mpsc::unbounded_channel();
    let mut st = State {
        out,
        ctx,
        rest: None,
        plain: plain_client(&props),
        gateway: None,
        me: Snowflake::NONE,
        token: String::new(),
        images: Arc::new(Semaphore::new(8)),
        internal: itx,
        voice: VoiceMgr::new(audio, video, volumes),
    };
    loop {
        tokio::select! {
            cmd = rx.recv() => match cmd {
                Some(c) => st.command(c),
                None => break,
            },
            Some(i) = irx.recv() => st.internal(i),
        }
    }
    st.voice.leave(None);
}

impl State {
    fn repaint_fn(&self) -> Arc<dyn Fn() + Send + Sync> {
        let ctx = self.ctx.clone();
        Arc::new(move || ctx.request_repaint())
    }

    fn publish(&self) {
        self.out.emit(Event::Media(self.voice.handles()));
    }

    fn internal(&mut self, i: Internal) {
        match i {
            Internal::Gateway(ev) => {
                match &ev {
                    GatewayEvent::Ready(r) => self.me = r.user.id,
                    GatewayEvent::VoiceStateUpdate(vs) if vs.user_id == self.me => {
                        let changed = self.voice.on_own_state(vs);
                        if changed {
                            self.try_connect_voice();
                            self.publish();
                        }
                    }
                    GatewayEvent::VoiceServerUpdate(vsu) => {
                        self.voice.on_server(vsu.clone());
                        self.try_connect_voice();
                        self.publish();
                    }
                    _ => {}
                }
                self.out.emit(Event::Gateway(ev));
            }
            Internal::NewGateway(h) => {
                if self.rest.is_none() {
                    h.shutdown(); // logged out while connecting
                } else if let Some(old) = self.gateway.replace(h) {
                    old.shutdown();
                }
            }
            Internal::CameraMode(mode) => {
                if let (Some(conn), Some(m)) = (&self.voice.conn, mode) {
                    conn.set_video(
                        Some(VideoAttrs { width: m.width, height: m.height, fps: m.fps, bitrate_kbps: self.voice.video.bitrate_kbps }),
                        self.voice.video.mtu_payload,
                    );
                }
            }
        }
    }

    fn try_connect_voice(&mut self) {
        let repaint = self.repaint_fn();
        if self.voice.try_connect(self.me, &self.token, repaint) {
            self.watch_camera_mode();
        }
    }

    fn watch_camera_mode(&self) {
        #[cfg(feature = "camera")]
        if let Some(stats) = self.voice.camera.as_ref().map(|c| c.stats.clone()) {
            let tx = self.internal.clone();
            tokio::spawn(async move {
                for _ in 0..100 {
                    if let Some(m) = stats.mode.lock().clone() {
                        let _ = tx.send(Internal::CameraMode(Some(m)));
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            });
        }
    }

    fn gw_voice_update(&self) {
        if let Some(gw) = &self.gateway {
            if let Some((guild, channel)) = self.voice.target {
                gw.send(GatewayCommand::UpdateVoiceState {
                    guild_id: guild,
                    channel_id: Some(channel),
                    self_mute: self.voice.muted || self.voice.deafened,
                    self_deaf: self.voice.deafened,
                    self_video: self.voice.camera_on,
                });
            }
        }
    }

    fn command(&mut self, c: Cmd) {
        match c {
            Cmd::Login { token, props } => {
                let rest = Rest::new(&token, &props);
                self.rest = Some(rest.clone());
                self.token = token.clone();
                self.plain = plain_client(&props);
                let out = self.out.clone();
                let itx = self.internal.clone();
                // Validate first so a bad token fails fast with a clear message.
                tokio::spawn(async move {
                    match rest.me().await {
                        Ok(user) => {
                            out.emit(Event::LoggedIn(user));
                            let events = itx.clone();
                            let sink: pickle_discord::gateway::EventSink = Arc::new(move |ev| {
                                let _ = events.send(Internal::Gateway(ev));
                            });
                            let _ = itx.send(Internal::NewGateway(GatewayHandle::spawn(token, props, sink)));
                        }
                        Err(RestError::Unauthorized) => out.emit(Event::LoginFailed {
                            message: "That token was rejected by Discord.".into(),
                            bad_token: true,
                        }),
                        Err(e) => out.emit(Event::LoginFailed { message: format!("Couldn't reach Discord: {e}"), bad_token: false }),
                    }
                });
            }
            Cmd::Logout => {
                self.voice.leave(self.gateway.as_ref());
                self.publish();
                if let Some(g) = self.gateway.take() {
                    g.shutdown();
                }
                self.rest = None;
                self.token.clear();
            }
            Cmd::FetchMessages { channel, before } => {
                let Some(rest) = self.rest.clone() else { return };
                let out = self.out.clone();
                tokio::spawn(async move {
                    let result = rest.messages(channel, before, 50).await.map_err(|e| e.to_string());
                    out.emit(Event::History { channel, result, requested: 50 });
                });
            }
            Cmd::SendMessage { channel, content, nonce, reply_to } => {
                let Some(rest) = self.rest.clone() else { return };
                let out = self.out.clone();
                tokio::spawn(async move {
                    if let Err(e) = rest.send_message(channel, &content, nonce, reply_to).await {
                        out.emit(Event::SendFailed { channel, nonce, error: e.to_string() });
                    }
                });
            }
            Cmd::EditMessage { channel, message, content } => {
                let Some(rest) = self.rest.clone() else { return };
                let out = self.out.clone();
                tokio::spawn(async move {
                    if let Err(e) = rest.edit_message(channel, message, &content).await {
                        out.emit(Event::Notice(format!("Edit failed: {e}")));
                    }
                });
            }
            Cmd::DeleteMessage { channel, message } => {
                let Some(rest) = self.rest.clone() else { return };
                let out = self.out.clone();
                tokio::spawn(async move {
                    if let Err(e) = rest.delete_message(channel, message).await {
                        out.emit(Event::Notice(format!("Delete failed: {e}")));
                    }
                });
            }
            Cmd::Typing(channel) => {
                let Some(rest) = self.rest.clone() else { return };
                tokio::spawn(async move {
                    let _ = rest.typing(channel).await;
                });
            }
            Cmd::FetchImage { key, url, max } => {
                let (client, out, sem) = (self.plain.clone(), self.out.clone(), self.images.clone());
                tokio::spawn(async move {
                    let _permit = sem.acquire_owned().await;
                    let result = match fetch_bytes(&client, &url).await {
                        Ok(bytes) => tokio::task::spawn_blocking(move || decode_image(&bytes, max))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string())),
                        Err(e) => Err(e.to_string()),
                    };
                    out.emit(Event::Image { key, result });
                });
            }
            Cmd::FetchText { key, url } => {
                let (client, out) = (self.plain.clone(), self.out.clone());
                tokio::spawn(async move {
                    let result = fetch_bytes(&client, &url)
                        .await
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                        .map_err(|e| e.to_string());
                    out.emit(Event::Text { key, result });
                });
            }
            Cmd::PluginFetch { plugin, id, url, method, body } => {
                let (client, out) = (self.plain.clone(), self.out.clone());
                tokio::spawn(async move {
                    let result = async {
                        let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
                        let mut req = client.request(m, &url);
                        if let Some(b) = body {
                            req = req.body(b);
                        }
                        let resp = req.send().await.map_err(|e| e.to_string())?;
                        let status = resp.status().as_u16();
                        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
                        if bytes.len() > 8 << 20 {
                            return Err("response too large".to_string());
                        }
                        Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
                    }
                    .await;
                    out.emit(Event::PluginFetchDone { plugin, id, result });
                });
            }
            Cmd::JoinVoice { guild, channel } => {
                self.voice.join(guild, channel);
                self.gw_voice_update();
                self.voice.ensure_engine(self.repaint_fn());
                self.publish();
            }
            Cmd::LeaveVoice => {
                self.voice.leave(self.gateway.as_ref());
                self.publish();
            }
            Cmd::SetMute(m) => {
                self.voice.muted = m;
                self.voice.sync_engine_flags();
                self.gw_voice_update();
                self.publish();
            }
            Cmd::SetDeaf(d) => {
                self.voice.deafened = d;
                self.voice.sync_engine_flags();
                self.gw_voice_update();
                self.publish();
            }
            Cmd::SetCamera(on) => {
                #[cfg(not(feature = "camera"))]
                if on {
                    self.out.emit(Event::Notice("This build has no camera support.".into()));
                    return;
                }
                self.voice.camera_on = on;
                let repaint = self.repaint_fn();
                self.voice.apply_camera(repaint);
                self.gw_voice_update();
                self.watch_camera_mode();
                self.publish();
            }
            Cmd::Audio(s) => {
                let repaint = self.repaint_fn();
                self.voice.update_audio(s, repaint);
                self.publish();
            }
            Cmd::Video(s) => {
                self.voice.video = s;
                if self.voice.camera_running() {
                    let repaint = self.repaint_fn();
                    self.voice.restart_camera(repaint);
                    self.watch_camera_mode();
                    self.publish();
                }
            }
            Cmd::UserVolume(u, v) => {
                self.voice.volumes.insert(u, v);
                if let Some(e) = &self.voice.engine {
                    e.shared.set_user_volume(u, v);
                }
            }
            Cmd::MicTest(on) => {
                self.voice.mic_test = on;
                let repaint = self.repaint_fn();
                if on {
                    self.voice.ensure_engine(repaint);
                } else if self.voice.target.is_none() {
                    self.voice.engine = None;
                }
                self.voice.sync_engine_flags();
                self.publish();
            }
            Cmd::CameraPreview(on) => {
                self.voice.camera_preview = on;
                let repaint = self.repaint_fn();
                self.voice.apply_camera(repaint);
                self.publish();
            }
            Cmd::ListDevices => {
                let out = self.out.clone();
                tokio::task::spawn_blocking(move || {
                    let inputs = devices::input_devices();
                    let outputs = devices::output_devices();
                    #[cfg(feature = "camera")]
                    let cameras = pickle_voice::video::camera::list_cameras();
                    #[cfg(not(feature = "camera"))]
                    let cameras = Vec::new();
                    out.emit(Event::Devices { inputs, outputs, cameras });
                });
            }
            Cmd::ProbeCamera(index) => {
                let out = self.out.clone();
                tokio::task::spawn_blocking(move || {
                    #[cfg(feature = "camera")]
                    let r = pickle_voice::video::camera::probe_modes(index);
                    #[cfg(not(feature = "camera"))]
                    let r = {
                        let _ = index;
                        Err("built without camera support".to_string())
                    };
                    out.emit(Event::CameraModes(r));
                });
            }
        }
    }
}

fn decode_image(bytes: &[u8], max: u32) -> Result<egui::ColorImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let img = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img };
    let rgba = img.to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

// ---------------------------------------------------------------------------
// Voice orchestration
// ---------------------------------------------------------------------------

struct VoiceMgr {
    audio: AudioSettings,
    video: VideoSettings,
    volumes: HashMap<u64, f32>,
    muted: bool,
    deafened: bool,
    camera_on: bool,
    mic_test: bool,
    camera_preview: bool,
    target: Option<(Option<Snowflake>, Snowflake)>,
    session_id: Option<String>,
    server: Option<VoiceServerUpdate>,
    conn: Option<VoiceConnection>,
    engine: Option<AudioEngine>,
    #[cfg(feature = "camera")]
    camera: Option<CameraStream>,
    camera_has_sink: bool,
}

impl VoiceMgr {
    fn new(audio: AudioSettings, video: VideoSettings, volumes: HashMap<u64, f32>) -> Self {
        Self {
            audio,
            video,
            volumes,
            muted: false,
            deafened: false,
            camera_on: false,
            mic_test: false,
            camera_preview: false,
            target: None,
            session_id: None,
            server: None,
            conn: None,
            engine: None,
            #[cfg(feature = "camera")]
            camera: None,
            camera_has_sink: false,
        }
    }

    fn handles(&self) -> MediaHandles {
        MediaHandles {
            engine: self.engine.as_ref().map(|e| e.shared.clone()),
            voice: self.conn.as_ref().map(|c| c.shared.clone()),
            #[cfg(feature = "camera")]
            camera: self.camera.as_ref().map(|c| c.stats.clone()),
            guild: self.target.and_then(|t| t.0),
            channel: self.target.map(|t| t.1),
            muted: self.muted,
            deafened: self.deafened,
            camera_on: self.camera_on,
            mic_test: self.mic_test,
        }
    }

    fn join(&mut self, guild: Option<Snowflake>, channel: Snowflake) {
        if self.target.is_some() {
            self.conn = None;
        }
        self.target = Some((guild, channel));
        self.session_id = None;
        self.server = None;
    }

    fn leave(&mut self, gw: Option<&GatewayHandle>) {
        if let (Some(gw), Some((guild, _))) = (gw, self.target) {
            gw.send(GatewayCommand::UpdateVoiceState {
                guild_id: guild,
                channel_id: None,
                self_mute: false,
                self_deaf: false,
                self_video: false,
            });
        }
        if let Some(c) = self.conn.take() {
            c.disconnect();
        }
        if let Some(e) = &self.engine {
            e.set_sink(None);
        }
        self.target = None;
        self.session_id = None;
        self.server = None;
        self.camera_on = false;
        if !self.mic_test {
            self.engine = None;
        }
        #[cfg(feature = "camera")]
        if !self.camera_preview {
            self.camera = None;
        }
    }

    /// Our own VOICE_STATE_UPDATE. Returns true if something changed.
    fn on_own_state(&mut self, vs: &VoiceState) -> bool {
        let Some((_, target_channel)) = self.target else { return false };
        match vs.channel_id {
            None => {
                // Kicked or disconnected elsewhere.
                if self.conn.is_some() {
                    info!("voice: disconnected by server");
                    self.leave(None);
                    return true;
                }
                false
            }
            Some(c) => {
                if c != target_channel {
                    // Moved by a moderator: follow.
                    self.target = Some((vs.guild_id, c));
                    self.conn = None;
                }
                let changed = self.session_id.as_deref() != Some(vs.session_id.as_str());
                self.session_id = Some(vs.session_id.clone());
                changed
            }
        }
    }

    fn on_server(&mut self, vsu: VoiceServerUpdate) {
        if self.target.is_none() {
            return;
        }
        // A new server means the old connection is dead.
        self.conn = None;
        self.server = Some(vsu);
    }

    fn ensure_engine(&mut self, repaint: Arc<dyn Fn() + Send + Sync>) {
        if self.engine.is_none() {
            let e = AudioEngine::start(self.audio.clone());
            e.shared.set_repaint(Some(repaint));
            for (u, v) in &self.volumes {
                e.shared.set_user_volume(*u, *v);
            }
            self.engine = Some(e);
        }
        self.sync_engine_flags();
    }

    fn sync_engine_flags(&self) {
        if let Some(e) = &self.engine {
            use std::sync::atomic::Ordering::Relaxed;
            e.shared.muted.store(self.muted, Relaxed);
            e.shared.deafened.store(self.deafened, Relaxed);
            e.shared.loopback.store(self.mic_test, Relaxed);
        }
    }

    /// Connect when we have target + session + server. Returns true if a
    /// connection was started.
    fn try_connect(&mut self, me: Snowflake, _token: &str, repaint: Arc<dyn Fn() + Send + Sync>) -> bool {
        if self.conn.is_some() {
            return false;
        }
        let (Some((guild, channel)), Some(session), Some(server)) = (self.target, self.session_id.clone(), self.server.clone()) else {
            return false;
        };
        let Some(endpoint) = server.endpoint.clone().filter(|e| !e.is_empty()) else {
            return false;
        };
        self.ensure_engine(repaint.clone());
        let info = VoiceConnectInfo {
            endpoint,
            token: server.token.clone(),
            session_id: session,
            server_id: guild.unwrap_or(channel).0,
            channel_id: channel.0,
            user_id: me.0,
        };
        info!("voice: connecting to {}", info.endpoint);
        let engine = self.engine.as_ref().expect("engine");
        let conn = VoiceConnection::connect(info, Some(engine.incoming()), Some(repaint.clone()));
        engine.set_sink(Some(conn.audio_sink()));
        self.conn = Some(conn);
        self.apply_camera(repaint);
        true
    }

    fn camera_running(&self) -> bool {
        #[cfg(feature = "camera")]
        return self.camera.is_some();
        #[cfg(not(feature = "camera"))]
        false
    }

    /// Start/stop the camera to match camera_on / preview / connection.
    fn apply_camera(&mut self, repaint: Arc<dyn Fn() + Send + Sync>) {
        #[cfg(feature = "camera")]
        {
            let want_send = self.camera_on && self.conn.is_some();
            let want_any = want_send || self.camera_preview || (self.camera_on && self.target.is_some());
            if !want_any {
                self.camera = None;
                if let Some(c) = &self.conn {
                    c.set_video(None, self.video.mtu_payload);
                }
                return;
            }
            if self.camera.is_some() && self.camera_has_sink == want_send {
                return;
            }
            self.camera = None;
            let sink = if want_send { self.conn.as_ref().map(|c| c.video_sink()) } else { None };
            self.camera_has_sink = sink.is_some();
            self.camera = Some(CameraStream::start(self.video.clone(), sink, Some(repaint)));
            if !want_send {
                if let Some(c) = &self.conn {
                    c.set_video(None, self.video.mtu_payload);
                }
            }
        }
        #[cfg(not(feature = "camera"))]
        let _ = repaint;
    }

    fn restart_camera(&mut self, repaint: Arc<dyn Fn() + Send + Sync>) {
        #[cfg(feature = "camera")]
        {
            self.camera = None;
        }
        self.apply_camera(repaint);
    }

    fn update_audio(&mut self, s: AudioSettings, repaint: Arc<dyn Fn() + Send + Sync>) {
        let needs_restart = s.input_device != self.audio.input_device
            || s.output_device != self.audio.output_device
            || s.capture_sample_rate != self.audio.capture_sample_rate;
        self.audio = s.clone();
        match &self.engine {
            Some(_) if needs_restart => {
                self.engine = None;
                self.ensure_engine(repaint);
                if let (Some(e), Some(c)) = (&self.engine, &self.conn) {
                    e.set_sink(Some(c.audio_sink()));
                    c.set_audio_input(Some(e.incoming()));
                }
            }
            Some(e) => e.shared.set_settings(s),
            None => {}
        }
    }
}

impl Drop for VoiceMgr {
    fn drop(&mut self) {
        if self.conn.is_some() {
            warn!("voice manager dropped while connected");
        }
    }
}
