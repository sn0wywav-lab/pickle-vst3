//! Application state and the per-frame update loop.

use crate::backend::{Backend, Cmd, Event, MediaHandles, ScreenInfo};
use crate::config::{AppConfig, Paths};
use crate::ui;
use crate::ui::theme::Colors;
use eframe::egui;
use rustcord_discord::{GatewayEvent, GatewayStatus, Snowflake, Store, User};
use rustcord_plugins::{HostCmd, HostConfig, PluginHost, PluginOut};
use rustcord_themes::{Theme, ThemeBuilder, ThemeEntry};
use rustcord_voice::devices::DeviceInfo;
use rustcord_voice::video::{CameraInfo, CameraMode};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Downloaded images as GPU textures, with a size cap so long sessions in
/// busy servers don't grow without bound.
#[derive(Default)]
pub struct Images {
    tex: HashMap<String, egui::TextureHandle>,
    order: VecDeque<String>,
    pending: HashSet<String>,
    failed: HashSet<String>,
}

const MAX_TEXTURES: usize = 1500;

impl Images {
    /// Texture for `key`, requesting a download the first time.
    pub fn get(&mut self, backend: &Backend, key: &str, url: &str, max: u32) -> Option<egui::TextureHandle> {
        if let Some(t) = self.tex.get(key) {
            return Some(t.clone());
        }
        if !self.pending.contains(key) && !self.failed.contains(key) {
            self.pending.insert(key.to_string());
            backend.send(Cmd::FetchImage { key: key.to_string(), url: url.to_string(), max });
        }
        None
    }

    fn insert(&mut self, ctx: &egui::Context, key: String, result: Result<egui::ColorImage, String>) {
        self.pending.remove(&key);
        match result {
            Ok(img) => {
                let t = ctx.load_texture(&key, img, egui::TextureOptions::LINEAR);
                self.order.push_back(key.clone());
                self.tex.insert(key, t);
                while self.tex.len() > MAX_TEXTURES {
                    match self.order.pop_front() {
                        // Evicted entries are simply re-downloaded if needed again.
                        Some(old) => {
                            self.tex.remove(&old);
                        }
                        None => break,
                    }
                }
            }
            Err(_) => {
                self.failed.insert(key);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoginState {
    LoggedOut,
    Checking,
    Ready,
    Error(String),
}

pub struct Toast {
    pub text: String,
    pub kind: String,
    pub until: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Voice,
    Video,
    Streaming,
    Appearance,
    Plugins,
    Account,
    About,
}

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    pub tab: SettingsTab,
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
    pub cameras: Vec<CameraInfo>,
    pub camera_modes: Vec<CameraMode>,
    pub screens: Vec<ScreenInfo>,
    pub screens_requested: bool,
    pub camera_error: Option<String>,
    pub devices_requested: bool,
    pub theme_import: String,
    pub show_logs: bool,
}

pub struct RustcordApp {
    pub paths: Paths,
    pub cfg: AppConfig,
    cfg_dirty: bool,
    last_save: Instant,
    pub backend: Backend,
    events: crossbeam_channel::Receiver<Event>,
    pub store: Store,
    pub login: LoginState,
    pub token_input: String,
    pub gateway_status: GatewayStatus,
    pub selected_guild: Option<Snowflake>,
    pub selected_channel: Option<Snowflake>,
    pub last_channel: HashMap<Snowflake, Snowflake>,
    pub collapsed: HashSet<Snowflake>,
    pub drafts: HashMap<Snowflake, String>,
    pub reply_to: Option<Snowflake>,
    pub editing: Option<(Snowflake, String)>,
    pub images: Images,
    pub media: MediaHandles,
    pub colors: Colors,
    pub theme: Theme,
    pub theme_entries: Vec<ThemeEntry>,
    theme_dirty: bool,
    plugin_css: BTreeMap<(String, String), String>,
    import_cache: HashMap<String, String>,
    imports_requested: HashSet<String>,
    pub plugins: PluginHost,
    pub toasts: Vec<Toast>,
    pub alerts: VecDeque<(String, String)>,
    pub settings: SettingsUi,
    last_typing: Instant,
    pub video_tex: HashMap<u64, (u64, egui::TextureHandle)>,
    pub scroll_to_bottom: bool,
    pub input_meter_db: f32,
    pub stats_prev: (Instant, u64),
    pub bitrate_kbps: f32,
    /// Measured message row heights for the virtualised message list.
    pub row_heights: HashMap<(Snowflake, bool), f32>,
    pub row_width: f32,
    pub golive_open: bool,
}

impl RustcordApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cfg: AppConfig, paths: Paths) -> Self {
        let ctx = cc.egui_ctx.clone();
        let (backend, events) = Backend::start(
            ctx.clone(),
            crate::backend::BackendInit {
                audio: cfg.audio.clone(),
                video: cfg.video.clone(),
                stream: cfg.stream.clone(),
                volumes: cfg.user_volumes.clone(),
                props: cfg.client.clone(),
            },
        );
        let repaint_ctx = ctx.clone();
        let plugins = PluginHost::start(
            HostConfig {
                plugins_dir: paths.plugins.clone(),
                data_dir: paths.plugin_data.clone(),
                enabled: cfg.enabled_plugins.iter().cloned().collect(),
            },
            Some(std::sync::Arc::new(move || repaint_ctx.request_repaint())),
        );
        let theme = Theme::default();
        let colors = ui::theme::apply(&ctx, &theme, cfg.ui_scale);
        let mut app = Self {
            theme_entries: rustcord_themes::scan(&paths.themes),
            paths,
            cfg_dirty: false,
            last_save: Instant::now(),
            backend,
            events,
            store: Store::default(),
            login: LoginState::LoggedOut,
            token_input: String::new(),
            gateway_status: GatewayStatus::Connecting,
            selected_guild: None,
            selected_channel: None,
            last_channel: HashMap::new(),
            collapsed: HashSet::new(),
            drafts: HashMap::new(),
            reply_to: None,
            editing: None,
            images: Images::default(),
            media: MediaHandles::default(),
            colors,
            theme,
            theme_dirty: true,
            plugin_css: BTreeMap::new(),
            import_cache: HashMap::new(),
            imports_requested: HashSet::new(),
            plugins,
            toasts: Vec::new(),
            alerts: VecDeque::new(),
            settings: SettingsUi::default(),
            last_typing: Instant::now() - Duration::from_secs(60),
            video_tex: HashMap::new(),
            scroll_to_bottom: true,
            input_meter_db: -120.0,
            stats_prev: (Instant::now(), 0),
            bitrate_kbps: 0.0,
            row_heights: HashMap::new(),
            row_width: 0.0,
            golive_open: false,
            cfg,
        };
        if std::env::var_os("RUSTCORD_DEMO").is_some() {
            crate::demo::load(&mut app);
        } else if let Some(token) = app.cfg.token.clone().filter(|t| !t.is_empty()) {
            app.login_with(token);
        }
        app
    }

    pub fn mark_dirty(&mut self) {
        self.cfg_dirty = true;
    }

    pub fn save_now(&mut self) {
        self.cfg.enabled_plugins = self.plugins.plugins().into_iter().filter(|p| p.enabled).map(|p| p.id).collect();
        self.cfg.save(&self.paths);
        self.cfg_dirty = false;
        self.last_save = Instant::now();
    }

    pub fn login_with(&mut self, token: String) {
        let token = token.trim().trim_matches('"').to_string();
        if token.is_empty() {
            return;
        }
        self.cfg.token = Some(token.clone());
        self.login = LoginState::Checking;
        self.backend.send(Cmd::Login { token, props: self.cfg.client.clone() });
    }

    pub fn logout(&mut self) {
        self.backend.send(Cmd::Logout);
        self.cfg.token = None;
        self.store = Store::default();
        self.login = LoginState::LoggedOut;
        self.selected_channel = None;
        self.selected_guild = None;
        self.save_now();
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: &str) {
        self.toasts.push(Toast { text: text.into(), kind: kind.into(), until: Instant::now() + Duration::from_secs(4) });
    }

    // ---- navigation ------------------------------------------------------

    pub fn select_guild(&mut self, guild: Option<Snowflake>) {
        self.selected_guild = guild;
        let ch = match guild {
            Some(g) => self.last_channel.get(&g).copied().or_else(|| {
                self.store
                    .grouped_channels(g)
                    .iter()
                    .flat_map(|grp| grp.channels.iter())
                    .find(|c| c.kind().is_text_like() && !c.kind().is_voice())
                    .map(|c| c.id)
            }),
            None => self.last_channel.get(&Snowflake::NONE).copied(),
        };
        if let Some(c) = ch {
            self.select_channel(c);
        } else {
            self.selected_channel = None;
        }
        self.cfg.last_guild = guild.map(|g| g.0);
        self.mark_dirty();
    }

    pub fn select_channel(&mut self, channel: Snowflake) {
        self.selected_channel = Some(channel);
        self.last_channel.insert(self.selected_guild.unwrap_or_default(), channel);
        self.store.mark_read(channel);
        self.reply_to = None;
        self.editing = None;
        self.scroll_to_bottom = true;
        let cm = self.store.messages.entry(channel).or_default();
        if !cm.loaded_once && !cm.loading {
            cm.loading = true;
            self.backend.send(Cmd::FetchMessages { channel, before: None });
        }
        self.cfg.last_channel = Some(channel.0);
        self.mark_dirty();
        self.push_plugin_context();
        let json = serde_json::json!({
            "type": "CHANNEL_SELECT",
            "channelId": channel.to_string(),
            "guildId": self.selected_guild.map(|g| g.to_string()),
        })
        .to_string();
        self.plugins.send(HostCmd::Event { name: "CHANNEL_SELECT".into(), json });
    }

    pub fn load_older(&mut self, channel: Snowflake) {
        let cm = self.store.messages.entry(channel).or_default();
        if cm.loading || cm.reached_start || !cm.loaded_once {
            return;
        }
        cm.loading = true;
        let before = cm.oldest();
        self.backend.send(Cmd::FetchMessages { channel, before });
    }

    fn push_plugin_context(&self) {
        let me = self.store.me.as_ref();
        let json = serde_json::json!({
            "me": me.map(|u| serde_json::json!({"id": u.id.to_string(), "username": u.username, "globalName": u.global_name})),
            "channelId": self.selected_channel.map(|c| c.to_string()),
            "guildId": self.selected_guild.map(|g| g.to_string()),
        });
        self.plugins.send(HostCmd::SetContext(json.to_string()));
    }

    // ---- sending -----------------------------------------------------------

    /// Composer submit: built-in commands, plugin commands, plugin pre-send
    /// hooks, then the network.
    pub fn submit(&mut self, channel: Snowflake, text: String) {
        let text = text.trim_end().to_string();
        if text.trim().is_empty() {
            return;
        }
        if let Some(cmdline) = text.strip_prefix('/') {
            let (name, rest) = cmdline.split_once(char::is_whitespace).unwrap_or((cmdline, ""));
            let rest = rest.trim();
            let builtin = match name {
                "shrug" => Some(format!("{rest} ¯\\\\_(ツ)_/¯")),
                "tableflip" => Some(format!("{rest} (╯°□°)╯︵ ┻━┻")),
                "unflip" => Some(format!("{rest} ┬─┬ノ( º _ ºノ)")),
                "spoiler" => Some(format!("||{rest}||")),
                _ => None,
            };
            if let Some(b) = builtin {
                self.send_with_hooks(channel, b.trim().to_string());
                return;
            }
            if let Some(cmd) = self.plugins.commands().into_iter().find(|c| c.name == name) {
                let args = parse_command_args(&cmd.options, rest);
                let ctx = serde_json::json!({
                    "channel": {"id": channel.to_string(), "guild_id": self.selected_guild.map(|g| g.to_string())},
                    "guild": self.selected_guild.map(|g| serde_json::json!({"id": g.to_string()})),
                });
                self.plugins.send(HostCmd::RunCommand { name: cmd.name, args_json: args.to_string(), ctx_json: ctx.to_string() });
                return;
            }
        }
        self.send_with_hooks(channel, text);
    }

    fn send_with_hooks(&mut self, channel: Snowflake, content: String) {
        if self.plugins.has_presend_hooks() {
            self.plugins.send(HostCmd::PreSend { channel_id: channel.to_string(), content });
        } else {
            self.send_now(channel, content);
        }
    }

    pub fn send_now(&mut self, channel: Snowflake, content: String) {
        let nonce = Snowflake::generate_nonce();
        self.store.insert_pending(channel, nonce, content.clone());
        let reply_to = self.reply_to.take();
        self.backend.send(Cmd::SendMessage { channel, content, nonce, reply_to });
        self.scroll_to_bottom = true;
    }

    pub fn typing(&mut self, channel: Snowflake) {
        if self.last_typing.elapsed() > Duration::from_secs(8) {
            self.last_typing = Instant::now();
            self.backend.send(Cmd::Typing(channel));
        }
    }

    // ---- voice -------------------------------------------------------------

    pub fn join_voice(&mut self, guild: Option<Snowflake>, channel: Snowflake) {
        self.backend.send(Cmd::JoinVoice { guild, channel });
    }

    pub fn set_audio(&mut self, s: rustcord_voice::AudioSettings) {
        self.cfg.audio = s.clone();
        self.backend.send(Cmd::Audio(s));
        self.mark_dirty();
    }

    pub fn set_stream(&mut self, s: rustcord_voice::StreamSettings) {
        self.cfg.stream = s.clone();
        self.backend.send(Cmd::Stream(s));
        self.mark_dirty();
    }

    pub fn open_golive(&mut self) {
        self.golive_open = true;
        self.backend.send(Cmd::ListScreens);
    }

    pub fn set_video(&mut self, s: rustcord_voice::VideoSettings) {
        self.cfg.video = s.clone();
        self.backend.send(Cmd::Video(s));
        self.mark_dirty();
    }

    // ---- themes ------------------------------------------------------------

    pub fn set_theme(&mut self, id: String) {
        self.cfg.theme = id;
        self.theme_dirty = true;
        self.mark_dirty();
    }

    pub fn rescan_themes(&mut self) {
        self.theme_entries = rustcord_themes::scan(&self.paths.themes);
        self.theme_dirty = true;
    }

    fn rebuild_theme(&mut self, ctx: &egui::Context) {
        self.theme_dirty = false;
        let css = rustcord_themes::load_css(&self.cfg.theme).unwrap_or_else(|_| {
            self.toast("Theme file not found, using default", "error");
            rustcord_themes::BUILTIN[0].1.to_string()
        });
        let mut b = ThemeBuilder::new();
        // Defaults first so partial themes inherit sensible colors.
        b.push_css(rustcord_themes::BUILTIN[0].1);
        for url in rustcord_themes::css::imports(&css) {
            match self.import_cache.get(&url) {
                Some(text) => b.push_css(text),
                None => {
                    if self.imports_requested.insert(url.clone()) {
                        self.backend.send(Cmd::FetchText { key: url.clone(), url });
                    }
                }
            }
        }
        b.push_css(&css);
        for layer in self.plugin_css.values() {
            b.push_css(layer);
        }
        let mut theme = b.build();
        // The theme's own header names it, not the defaults layer.
        theme.meta = rustcord_themes::css::parse_meta(&css);
        self.colors = ui::theme::apply(ctx, &theme, self.cfg.ui_scale);
        self.theme = theme;
        self.row_heights.clear();
    }

    // ---- event pump --------------------------------------------------------

    fn process_events(&mut self, ctx: &egui::Context) {
        let events: Vec<Event> = self.events.try_iter().take(5000).collect();
        let plugins_running = self.plugins.plugins().iter().any(|p| p.running);
        for ev in events {
            match ev {
                Event::LoggedIn(user) => {
                    self.store.me = Some(user);
                    self.login = LoginState::Ready;
                    self.mark_dirty();
                }
                Event::LoginFailed { message, bad_token } => {
                    self.login = LoginState::Error(message);
                    if bad_token {
                        self.cfg.token = None;
                    }
                }
                Event::Gateway(gev) => {
                    if plugins_running {
                        self.forward_to_plugins(&gev);
                    }
                    let was_ready = matches!(gev, GatewayEvent::Ready(_));
                    if let GatewayEvent::Status(s) = &gev {
                        self.gateway_status = s.clone();
                        if let GatewayStatus::Fatal(msg) = s {
                            self.login = LoginState::Error(msg.clone());
                            self.cfg.token = None;
                        }
                    }
                    if let GatewayEvent::MessageCreate(m) = &gev {
                        if Some(m.channel_id) == self.selected_channel {
                            self.scroll_to_bottom |= m.author.id == self.store.me_id();
                        }
                    }
                    self.store.apply(gev, self.selected_channel);
                    if was_ready {
                        self.on_ready();
                    }
                }
                Event::History { channel, result, requested } => match result {
                    Ok(msgs) => self.store.add_history(channel, msgs, requested),
                    Err(e) => {
                        let cm = self.store.messages.entry(channel).or_default();
                        cm.loading = false;
                        cm.loaded_once = true;
                        cm.error = Some(e);
                    }
                },
                Event::SendFailed { channel, nonce, error } => self.store.mark_failed(channel, nonce, error),
                Event::Image { key, result } => self.images.insert(ctx, key, result),
                Event::Text { key, result } => {
                    match result {
                        Ok(text) => {
                            self.import_cache.insert(key, text);
                        }
                        Err(e) => self.toast(format!("Theme import failed: {e}"), "error"),
                    }
                    self.theme_dirty = true;
                }
                Event::PluginFetchDone { plugin, id, result } => self.plugins.send(HostCmd::FetchDone { plugin, id, result }),
                Event::Media(h) => {
                    if h.channel.is_none() {
                        self.video_tex.clear();
                    }
                    self.media = h;
                }
                Event::Devices { inputs, outputs, cameras } => {
                    self.settings.inputs = inputs;
                    self.settings.outputs = outputs;
                    self.settings.cameras = cameras;
                }
                Event::CameraModes(r) => match r {
                    Ok(m) => {
                        self.settings.camera_modes = m;
                        self.settings.camera_error = None;
                    }
                    Err(e) => self.settings.camera_error = Some(e),
                },
                Event::Screens(list) => self.settings.screens = list,
                Event::Notice(n) => self.toast(n, "info"),
            }
        }
    }

    fn forward_to_plugins(&self, ev: &GatewayEvent) {
        let (name, json) = match ev {
            GatewayEvent::MessageCreate(m) => (
                "MESSAGE_CREATE",
                serde_json::json!({
                    "type": "MESSAGE_CREATE",
                    "channelId": m.channel_id.to_string(),
                    "guildId": m.guild_id.map(|g| g.to_string()),
                    "message": &**m,
                    "optimistic": false,
                }),
            ),
            GatewayEvent::MessageDelete(d) => (
                "MESSAGE_DELETE",
                serde_json::json!({"type": "MESSAGE_DELETE", "id": d.id.to_string(), "channelId": d.channel_id.to_string()}),
            ),
            GatewayEvent::TypingStart(t) => (
                "TYPING_START",
                serde_json::json!({"type": "TYPING_START", "channelId": t.channel_id.to_string(), "userId": t.user_id.to_string()}),
            ),
            GatewayEvent::VoiceStateUpdate(v) => (
                "VOICE_STATE_UPDATES",
                serde_json::json!({"type": "VOICE_STATE_UPDATES", "voiceStates": [&**v]}),
            ),
            _ => return,
        };
        self.plugins.send(HostCmd::Event { name: name.into(), json: json.to_string() });
    }

    fn on_ready(&mut self) {
        self.login = LoginState::Ready;
        self.push_plugin_context();
        let guild = self.cfg.last_guild.map(Snowflake).filter(|g| self.store.guilds.iter().any(|x| x.id == *g));
        self.selected_guild = guild;
        if let Some(c) = self.cfg.last_channel.map(Snowflake).filter(|c| self.store.channels.contains_key(c)) {
            let in_guild = self.store.channels.get(&c).and_then(|ch| ch.guild_id) == guild;
            if in_guild {
                self.select_channel(c);
                return;
            }
        }
        self.select_guild(guild);
    }

    fn process_plugin_output(&mut self) {
        for o in self.plugins.drain() {
            match o {
                PluginOut::Toast { text, kind, .. } => self.toast(text, &kind),
                PluginOut::Alert { title, body, .. } => self.alerts.push_back((title, body)),
                PluginOut::SendMessage { channel_id, content } => {
                    if let Ok(c) = channel_id.parse::<u64>() {
                        self.send_now(Snowflake(c), content);
                    }
                }
                PluginOut::LocalMessage { plugin, channel_id, content } => {
                    if let Ok(c) = channel_id.parse::<u64>() {
                        let name = self.plugins.plugins().into_iter().find(|p| p.id == plugin).map(|p| p.name).unwrap_or(plugin);
                        let author = User { username: name, bot: true, ..Default::default() };
                        self.store.push_local(Snowflake(c), author, content);
                        self.scroll_to_bottom = true;
                    }
                }
                PluginOut::Css { plugin, id, css } => {
                    match css {
                        Some(c) => {
                            self.plugin_css.insert((plugin, id), c);
                        }
                        None => {
                            self.plugin_css.remove(&(plugin, id));
                        }
                    }
                    self.theme_dirty = true;
                }
                PluginOut::ClearCss { plugin } => {
                    let before = self.plugin_css.len();
                    self.plugin_css.retain(|(p, _), _| *p != plugin);
                    self.theme_dirty |= before != self.plugin_css.len();
                }
                PluginOut::Fetch { plugin, id, url, method, body } => {
                    self.backend.send(Cmd::PluginFetch { plugin, id, url, method, body })
                }
                PluginOut::PluginsChanged => self.mark_dirty(),
            }
        }
    }

    fn update_meters(&mut self) {
        if let Some(e) = &self.media.engine {
            let db = e.take_input_peak_db();
            // fast attack, slow release
            self.input_meter_db = if db > self.input_meter_db { db } else { self.input_meter_db - 1.5 };
            let el = self.stats_prev.0.elapsed();
            if el >= Duration::from_millis(1000) {
                let bytes = e.bytes_sent.load(std::sync::atomic::Ordering::Relaxed);
                self.bitrate_kbps = (bytes.saturating_sub(self.stats_prev.1)) as f32 * 8.0 / 1000.0 / el.as_secs_f32();
                self.stats_prev = (std::time::Instant::now(), bytes);
            }
        }
    }
}

/// `name:value` pairs, or the whole text for the first option.
pub fn parse_command_args(options: &[rustcord_plugins::CommandOption], rest: &str) -> serde_json::Value {
    let mut args = Vec::new();
    let mut named = false;
    for o in options {
        let key = format!("{}:", o.name);
        if let Some(pos) = rest.find(&key) {
            named = true;
            let after = &rest[pos + key.len()..];
            let end = options
                .iter()
                .filter_map(|x| after.find(&format!(" {}:", x.name)))
                .min()
                .unwrap_or(after.len());
            args.push(serde_json::json!({"name": o.name, "value": after[..end].trim()}));
        }
    }
    if !named && !rest.is_empty() {
        if let Some(first) = options.first() {
            args.push(serde_json::json!({"name": first.name, "value": rest}));
        } else {
            args.push(serde_json::json!({"name": "text", "value": rest}));
        }
    }
    serde_json::Value::Array(args)
}

impl eframe::App for RustcordApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.process_events(&ctx);
        self.process_plugin_output();
        if self.theme_dirty {
            self.rebuild_theme(&ctx);
        }
        self.update_meters();
        self.store.prune_typing();

        if self.login != LoginState::Ready {
            ui::login::show(self, ui);
        } else {
            ui::sidebar::background(self, ui);
            ui::sidebar::guild_rail(self, ui);
            ui::sidebar::channel_panel(self, ui);
            ui::chat::central(self, ui);
        }
        ui::settings::window(self, &ctx);
        ui::golive::dialog(self, &ctx);
        ui::widgets::overlays(self, &ctx);

        // Keep meters / speaking rings / video live while in a call.
        if self.media.engine.is_some() || self.media.voice.is_some() || self.media.screen_preview.is_some() {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if !self.toasts.is_empty() || !self.store.typing.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.cfg_dirty && self.last_save.elapsed() > Duration::from_secs(2) {
            self.save_now();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustcord_plugins::CommandOption;

    #[test]
    fn command_arg_parsing() {
        let opts = vec![
            CommandOption { name: "text".into(), description: String::new(), required: true },
            CommandOption { name: "times".into(), description: String::new(), required: false },
        ];
        assert_eq!(parse_command_args(&opts, "hello there"), serde_json::json!([{"name": "text", "value": "hello there"}]));
        assert_eq!(
            parse_command_args(&opts, "text:hi you times:3"),
            serde_json::json!([{"name": "text", "value": "hi you"}, {"name": "times", "value": "3"}])
        );
    }
}
