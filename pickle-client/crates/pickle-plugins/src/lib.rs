//! JavaScript plugin host.
//!
//! Every plugin runs in its own QuickJS context (separate globals) on one
//! dedicated thread, with a memory cap and a watchdog that interrupts
//! scripts stuck in a loop. Three plugin styles load:
//!
//! * **BetterDiscord** `*.plugin.js` (CommonJS `module.exports = class ...`)
//!   with a `BdApi` shim: toasts, alerts, `Data`, `DOM.addStyle`, `Logger`,
//!   `Net.fetch`, timers, `onMessage`/`onSwitch` hooks.
//! * **Vencord-style** ES modules (`import definePlugin from "@utils/types"`)
//!   with shims for `@api/Commands`, `@api/MessageEvents`, `@api/Settings`,
//!   `@utils/constants`, `@utils/Logger`, `@webpack/common` (FluxDispatcher).
//!   `commands`, `flux` handlers, pre-send listeners, `start`/`stop` work.
//! * **Pickle** scripts using the native `Pickle` API directly.
//!
//! What cannot work in any native client: webpack `patches`, React
//! components and DOM manipulation. Those calls are stubbed so plugins load
//! and run their supported parts instead of crashing.

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::{CaughtError, Context, Ctx, Function, Module, Object, Runtime, Value};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

const PRELUDE: &str = include_str!("js/prelude.js");
const VENCORD_SHIMS: &str = include_str!("js/vencord_shims.js");
pub const API_VERSION: &str = "pickle-plugin-api/1";
/// Longest a plugin may run JavaScript without yielding before it is interrupted.
pub const WATCHDOG: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginKind {
    BetterDiscord,
    Vencord,
    Pickle,
}

impl PluginKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::BetterDiscord => "BetterDiscord",
            Self::Vencord => "Vencord",
            Self::Pickle => "Pickle",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// File name; stable id used for enable state and storage.
    pub id: String,
    pub path: PathBuf,
    pub kind: PluginKind,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub running: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOption {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub plugin: String,
    pub name: String,
    pub description: String,
    pub options: Vec<CommandOption>,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub plugin: String,
    pub level: String,
    pub text: String,
}

/// Messages from plugins to the app.
#[derive(Debug, Clone)]
pub enum PluginOut {
    Toast { plugin: String, text: String, kind: String },
    Alert { plugin: String, title: String, body: String },
    /// Send a message as the user.
    SendMessage { channel_id: String, content: String },
    /// Show a local-only message (like Vencord's `sendBotMessage`).
    LocalMessage { plugin: String, channel_id: String, content: String },
    /// Add (`Some`) or remove (`None`) a stylesheet layer.
    Css { plugin: String, id: String, css: Option<String> },
    ClearCss { plugin: String },
    /// HTTP request on the plugin's behalf; answer with `HostCmd::FetchDone`.
    Fetch { plugin: String, id: u32, url: String, method: String, body: Option<String> },
    PluginsChanged,
}

/// Messages from the app to the host.
#[derive(Debug, Clone)]
pub enum HostCmd {
    Rescan,
    SetEnabled { id: String, enabled: bool },
    /// Flux-style event, e.g. `MESSAGE_CREATE` with a JSON payload.
    Event { name: String, json: String },
    /// Run outgoing-message hooks; answers with `PluginOut::SendMessage`
    /// (or nothing if a plugin cancelled).
    PreSend { channel_id: String, content: String },
    RunCommand { name: String, args_json: String, ctx_json: String },
    /// JSON for `Pickle.context()`.
    SetContext(String),
    FetchDone { plugin: String, id: u32, result: Result<(u16, String), String> },
    Shutdown,
}

#[derive(Default)]
pub struct HostState {
    plugins: Mutex<Vec<PluginInfo>>,
    commands: Mutex<Vec<CommandInfo>>,
    presend: Mutex<HashMap<String, usize>>,
    logs: Mutex<VecDeque<LogLine>>,
}

impl HostState {
    fn log(&self, plugin: &str, level: &str, text: &str) {
        match level {
            "error" => warn!("[plugin {plugin}] {text}"),
            _ => info!("[plugin {plugin}] {text}"),
        }
        let mut l = self.logs.lock();
        if l.len() >= 500 {
            l.pop_front();
        }
        l.push_back(LogLine { plugin: plugin.into(), level: level.into(), text: text.into() });
    }
}

pub struct HostConfig {
    pub plugins_dir: PathBuf,
    pub data_dir: PathBuf,
    pub enabled: HashSet<String>,
}

pub type RepaintFn = Arc<dyn Fn() + Send + Sync>;

pub struct PluginHost {
    tx: Sender<HostCmd>,
    out: Receiver<PluginOut>,
    state: Arc<HostState>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PluginHost {
    pub fn start(cfg: HostConfig, repaint: Option<RepaintFn>) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (out_tx, out) = crossbeam_channel::unbounded();
        let state = Arc::new(HostState::default());
        let st = state.clone();
        let thread = std::thread::Builder::new()
            .name("pickle-plugins".into())
            .stack_size(8 << 20)
            .spawn(move || {
                let mut host = match Thread::new(cfg, st, out_tx, repaint) {
                    Ok(h) => h,
                    Err(e) => {
                        warn!("plugin runtime failed to start: {e}");
                        return;
                    }
                };
                host.run(rx);
            })
            .ok();
        Self { tx, out, state, thread }
    }

    pub fn send(&self, cmd: HostCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn drain(&self) -> Vec<PluginOut> {
        self.out.try_iter().collect()
    }

    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.state.plugins.lock().clone()
    }

    pub fn commands(&self) -> Vec<CommandInfo> {
        self.state.commands.lock().clone()
    }

    pub fn has_presend_hooks(&self) -> bool {
        self.state.presend.lock().values().any(|n| *n > 0)
    }

    pub fn logs(&self) -> Vec<LogLine> {
        self.state.logs.lock().iter().cloned().collect()
    }

    pub fn clear_logs(&self) {
        self.state.logs.lock().clear();
    }
}

impl Drop for PluginHost {
    fn drop(&mut self) {
        let _ = self.tx.send(HostCmd::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin thread
// ---------------------------------------------------------------------------

struct TimerReq {
    plugin: String,
    id: u32,
    ms: f64,
    repeat: bool,
    cancel: bool,
}

#[derive(Default)]
struct Local {
    timers: Vec<TimerReq>,
    context_json: String,
}

struct Loaded {
    ctx: Context,
}

struct Timer {
    due: Instant,
    plugin: String,
    id: u32,
    every: Option<Duration>,
}

struct Thread {
    rt: Runtime,
    cfg: HostConfig,
    state: Arc<HostState>,
    out: Sender<PluginOut>,
    repaint: Option<RepaintFn>,
    loaded: BTreeMap<String, Loaded>,
    local: Rc<RefCell<Local>>,
    timers: Vec<Timer>,
    deadline: Rc<Cell<Option<Instant>>>,
}

/// Split the bundled shim file into (specifier, source) pairs.
fn shim_modules() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in VENCORD_SHIMS.lines() {
        if let Some(name) = line.strip_prefix("// @module ") {
            out.push((name.trim().to_string(), String::new()));
        } else if let Some((_, src)) = out.last_mut() {
            src.push_str(line);
            src.push('\n');
        }
    }
    out
}

fn parse_meta(src: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let Some(start) = src.find("/**") else { return m };
    let Some(len) = src[start..].find("*/") else { return m };
    for line in src[start + 3..start + len].lines() {
        let line = line.trim().trim_start_matches('*').trim();
        if let Some(rest) = line.strip_prefix('@') {
            let (k, v) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            m.insert(k.to_string(), v.trim().to_string());
        }
    }
    m
}

pub fn detect_kind(path: &Path, src: &str) -> PluginKind {
    let name = path.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name.ends_with(".plugin.js") || src.contains("module.exports") {
        PluginKind::BetterDiscord
    } else if src.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("import ") || l.starts_with("export default") || l.starts_with("import{")
    }) {
        PluginKind::Vencord
    } else {
        PluginKind::Pickle
    }
}

fn caught(ctx: &Ctx<'_>, e: rquickjs::Error) -> String {
    CaughtError::from_error(ctx, e).to_string()
}

impl Thread {
    fn new(cfg: HostConfig, state: Arc<HostState>, out: Sender<PluginOut>, repaint: Option<RepaintFn>) -> Result<Self, String> {
        let rt = Runtime::new().map_err(|e| e.to_string())?;
        rt.set_memory_limit(256 << 20);
        rt.set_max_stack_size(4 << 20);
        let mut resolver = BuiltinResolver::default();
        let mut loader = BuiltinLoader::default();
        for (name, src) in shim_modules() {
            resolver.add_module(name.clone());
            loader.add_module(name, src);
        }
        rt.set_loader(resolver, loader);
        let deadline: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
        {
            let d = deadline.clone();
            rt.set_interrupt_handler(Some(Box::new(move || d.get().map(|t| Instant::now() > t).unwrap_or(false))));
        }
        let _ = std::fs::create_dir_all(&cfg.plugins_dir);
        let _ = std::fs::create_dir_all(&cfg.data_dir);
        let mut t = Self {
            rt,
            cfg,
            state,
            out,
            repaint,
            loaded: BTreeMap::new(),
            local: Rc::new(RefCell::new(Local { context_json: "{}".into(), ..Default::default() })),
            timers: Vec::new(),
            deadline,
        };
        t.rescan();
        Ok(t)
    }

    fn emit(&self, o: PluginOut) {
        let _ = self.out.send(o);
        if let Some(r) = &self.repaint {
            r();
        }
    }

    fn run(&mut self, rx: Receiver<HostCmd>) {
        loop {
            let wait = self
                .timers
                .iter()
                .map(|t| t.due.saturating_duration_since(Instant::now()))
                .min()
                .unwrap_or(Duration::from_secs(1))
                .min(Duration::from_secs(1));
            match rx.recv_timeout(wait) {
                Ok(HostCmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.fire_timers();
            self.apply_local();
        }
        let ids: Vec<String> = self.loaded.keys().cloned().collect();
        for id in ids {
            self.unload(&id);
        }
    }

    fn handle(&mut self, cmd: HostCmd) {
        match cmd {
            HostCmd::Rescan => self.rescan(),
            HostCmd::SetEnabled { id, enabled } => {
                if enabled {
                    self.cfg.enabled.insert(id.clone());
                    self.load(&id);
                } else {
                    self.cfg.enabled.remove(&id);
                    self.unload(&id);
                }
                self.set_info(&id, |p| p.enabled = enabled);
                self.emit(PluginOut::PluginsChanged);
            }
            HostCmd::Event { name, json } => {
                let ids: Vec<String> = self.loaded.keys().cloned().collect();
                for id in ids {
                    self.call(&id, "dispatch", (name.as_str(), json.as_str()));
                }
            }
            HostCmd::PreSend { channel_id, content } => {
                #[derive(Deserialize)]
                struct R {
                    content: String,
                    cancel: bool,
                }
                let mut content = content;
                let ids: Vec<String> = self.loaded.keys().cloned().collect();
                for id in ids {
                    if self.state.presend.lock().get(&id).copied().unwrap_or(0) == 0 {
                        continue;
                    }
                    if let Some(r) = self.call_ret::<String, _>(&id, "runPresend", (channel_id.as_str(), content.as_str())) {
                        if let Ok(r) = serde_json::from_str::<R>(&r) {
                            if r.cancel {
                                self.emit(PluginOut::Toast { plugin: id, text: "Message cancelled by a plugin".into(), kind: "info".into() });
                                return;
                            }
                            content = r.content;
                        }
                    }
                }
                if !content.trim().is_empty() {
                    self.emit(PluginOut::SendMessage { channel_id, content });
                }
            }
            HostCmd::RunCommand { name, args_json, ctx_json } => {
                let owner = self.state.commands.lock().iter().find(|c| c.name == name).map(|c| c.plugin.clone());
                if let Some(id) = owner {
                    self.call(&id, "runCommand", (name.as_str(), args_json.as_str(), ctx_json.as_str()));
                }
            }
            HostCmd::SetContext(json) => self.local.borrow_mut().context_json = json,
            HostCmd::FetchDone { plugin, id, result } => {
                let (ok, status, body) = match result {
                    Ok((s, b)) => (true, s as u32, b),
                    Err(e) => (false, 0, e),
                };
                self.call(&plugin, "fetchDone", (id, ok, status, body));
            }
            HostCmd::Shutdown => {}
        }
    }

    fn set_info(&self, id: &str, f: impl FnOnce(&mut PluginInfo)) {
        if let Some(p) = self.state.plugins.lock().iter_mut().find(|p| p.id == id) {
            f(p);
        }
    }

    fn rescan(&mut self) {
        let mut found = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.cfg.plugins_dir) {
            for e in rd.flatten() {
                let path = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if !(name.ends_with(".js") || name.ends_with(".mjs")) {
                    continue;
                }
                let src = std::fs::read_to_string(&path).unwrap_or_default();
                let meta = parse_meta(&src);
                let kind = detect_kind(&path, &src);
                found.push(PluginInfo {
                    id: name.clone(),
                    path,
                    kind,
                    name: meta.get("name").cloned().unwrap_or_else(|| name.trim_end_matches(".js").trim_end_matches(".plugin").to_string()),
                    author: meta.get("author").cloned().unwrap_or_default(),
                    version: meta.get("version").cloned().unwrap_or_default(),
                    description: meta.get("description").cloned().unwrap_or_default(),
                    enabled: self.cfg.enabled.contains(&name),
                    running: false,
                    error: None,
                });
            }
        }
        found.sort_by_key(|a| a.name.to_lowercase());
        // Restart everything so edited files take effect.
        let ids: Vec<String> = self.loaded.keys().cloned().collect();
        for id in ids {
            self.unload(&id);
        }
        *self.state.plugins.lock() = found;
        let enabled: Vec<String> = self.cfg.enabled.iter().cloned().collect();
        for id in enabled {
            self.load(&id);
        }
        self.emit(PluginOut::PluginsChanged);
    }

    fn storage_path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).collect();
        self.cfg.data_dir.join(format!("{safe}.json"))
    }

    fn install_natives(&self, ctx: &Ctx<'_>, id: &str) -> rquickjs::Result<()> {
        let n = Object::new(ctx.clone())?;
        let pid = id.to_string();
        let out = self.out.clone();
        let repaint = self.repaint.clone();
        let state = self.state.clone();
        let emit = {
            let out = out.clone();
            let repaint = repaint.clone();
            Rc::new(move |o: PluginOut| {
                let _ = out.send(o);
                if let Some(r) = &repaint {
                    r();
                }
            })
        };

        {
            let (pid, state) = (pid.clone(), state.clone());
            n.set("log", Function::new(ctx.clone(), move |level: String, msg: String| state.log(&pid, &level, &msg))?)?;
        }
        n.set("version", Function::new(ctx.clone(), || API_VERSION.to_string())?)?;
        {
            let (pid, emit) = (pid.clone(), emit.clone());
            n.set(
                "toast",
                Function::new(ctx.clone(), move |text: String, kind: String| emit(PluginOut::Toast { plugin: pid.clone(), text, kind }))?,
            )?;
        }
        {
            let (pid, emit) = (pid.clone(), emit.clone());
            n.set(
                "alert",
                Function::new(ctx.clone(), move |title: String, body: String| emit(PluginOut::Alert { plugin: pid.clone(), title, body }))?,
            )?;
        }
        {
            let emit = emit.clone();
            n.set(
                "send",
                Function::new(ctx.clone(), move |channel_id: String, content: String| {
                    emit(PluginOut::SendMessage { channel_id, content })
                })?,
            )?;
        }
        {
            let (pid, emit) = (pid.clone(), emit.clone());
            n.set(
                "local",
                Function::new(ctx.clone(), move |channel_id: String, content: String| {
                    emit(PluginOut::LocalMessage { plugin: pid.clone(), channel_id, content })
                })?,
            )?;
        }
        {
            let (pid, emit) = (pid.clone(), emit.clone());
            n.set(
                "css",
                Function::new(ctx.clone(), move |sid: String, css: Option<String>| {
                    emit(PluginOut::Css { plugin: pid.clone(), id: sid, css })
                })?,
            )?;
        }
        {
            let (pid, emit) = (pid.clone(), emit.clone());
            n.set(
                "fetch",
                Function::new(ctx.clone(), move |fid: u32, url: String, method: String, body: Option<String>| {
                    emit(PluginOut::Fetch { plugin: pid.clone(), id: fid, url, method, body })
                })?,
            )?;
        }

        // Persistent per-plugin storage.
        let path = self.storage_path(id);
        let data: Rc<RefCell<serde_json::Map<String, serde_json::Value>>> = Rc::new(RefCell::new(
            std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
        ));
        {
            let data = data.clone();
            n.set(
                "storeGet",
                Function::new(ctx.clone(), move |k: String| data.borrow().get(&k).map(|v| v.to_string()))?,
            )?;
        }
        {
            let (data, path) = (data.clone(), path.clone());
            n.set(
                "storeSet",
                Function::new(ctx.clone(), move |k: String, v: String| {
                    let v: serde_json::Value = serde_json::from_str(&v).unwrap_or(serde_json::Value::Null);
                    data.borrow_mut().insert(k, v);
                    let _ = std::fs::write(&path, serde_json::to_vec_pretty(&*data.borrow()).unwrap_or_default());
                })?,
            )?;
        }
        {
            let (data, path) = (data.clone(), path.clone());
            n.set(
                "storeDel",
                Function::new(ctx.clone(), move |k: String| {
                    data.borrow_mut().remove(&k);
                    let _ = std::fs::write(&path, serde_json::to_vec_pretty(&*data.borrow()).unwrap_or_default());
                })?,
            )?;
        }

        {
            let local = self.local.clone();
            n.set("context", Function::new(ctx.clone(), move || local.borrow().context_json.clone())?)?;
        }
        {
            let (local, pid) = (self.local.clone(), pid.clone());
            n.set(
                "timer",
                Function::new(ctx.clone(), move |tid: u32, ms: f64, repeat: bool| {
                    local.borrow_mut().timers.push(TimerReq { plugin: pid.clone(), id: tid, ms, repeat, cancel: false })
                })?,
            )?;
        }
        {
            let (local, pid) = (self.local.clone(), pid.clone());
            n.set(
                "clearTimer",
                Function::new(ctx.clone(), move |tid: u32| {
                    local.borrow_mut().timers.push(TimerReq { plugin: pid.clone(), id: tid, ms: 0.0, repeat: false, cancel: true })
                })?,
            )?;
        }
        {
            let (state, pid) = (state.clone(), pid.clone());
            n.set(
                "presendChanged",
                Function::new(ctx.clone(), move |count: u32| {
                    state.presend.lock().insert(pid.clone(), count as usize);
                })?,
            )?;
        }
        {
            let (state, pid) = (state.clone(), pid.clone());
            n.set(
                "commandsChanged",
                Function::new(ctx.clone(), move |json: String| {
                    #[derive(Deserialize)]
                    struct C {
                        name: String,
                        #[serde(default)]
                        description: String,
                        #[serde(default)]
                        options: Vec<CommandOption>,
                    }
                    let list: Vec<C> = serde_json::from_str(&json).unwrap_or_default();
                    let mut cmds = state.commands.lock();
                    cmds.retain(|c| c.plugin != pid);
                    cmds.extend(list.into_iter().map(|c| CommandInfo {
                        plugin: pid.clone(),
                        name: c.name,
                        description: c.description,
                        options: c.options,
                    }));
                })?,
            )?;
        }
        ctx.globals().set("__native", n)?;
        Ok(())
    }

    fn arm(&self) {
        self.deadline.set(Some(Instant::now() + WATCHDOG));
    }

    fn disarm(&self) {
        self.deadline.set(None);
    }

    fn load(&mut self, id: &str) {
        if self.loaded.contains_key(id) {
            return;
        }
        let Some(info) = self.state.plugins.lock().iter().find(|p| p.id == id).cloned() else { return };
        let src = match std::fs::read_to_string(&info.path) {
            Ok(s) => s,
            Err(e) => {
                self.set_info(id, |p| p.error = Some(e.to_string()));
                return;
            }
        };
        let ctx = match Context::full(&self.rt) {
            Ok(c) => c,
            Err(e) => {
                self.set_info(id, |p| p.error = Some(e.to_string()));
                return;
            }
        };
        self.arm();
        let result: Result<Option<String>, String> = ctx.with(|ctx| {
            self.install_natives(&ctx, id).map_err(|e| caught(&ctx, e))?;
            ctx.eval::<(), _>(PRELUDE).map_err(|e| caught(&ctx, e))?;
            let p: Object = ctx.globals().get("__pickle").map_err(|e| caught(&ctx, e))?;
            match info.kind {
                PluginKind::BetterDiscord => {
                    let meta = serde_json::json!({
                        "name": info.name, "author": info.author, "version": info.version,
                        "description": info.description, "filename": info.id,
                    })
                    .to_string();
                    let wrapped = format!("(function (module, exports, require, __meta) {{\n{src}\n}})");
                    let factory: Function = ctx.eval(wrapped).map_err(|e| caught(&ctx, e))?;
                    let meta = ctx.json_parse(meta).map_err(|e| caught(&ctx, e))?;
                    let start: Function = p.get("startBetterDiscord").map_err(|e| caught(&ctx, e))?;
                    start.call::<_, ()>((factory, meta)).map_err(|e| caught(&ctx, e))?;
                }
                PluginKind::Vencord => {
                    let module = Module::declare(ctx.clone(), id, src).map_err(|e| caught(&ctx, e))?;
                    let (module, promise) = module.eval().map_err(|e| caught(&ctx, e))?;
                    promise.finish::<()>().map_err(|e| caught(&ctx, e))?;
                    let ns = module.namespace().map_err(|e| caught(&ctx, e))?;
                    let mut def: Value = ns.get("default").map_err(|e| caught(&ctx, e))?;
                    if def.is_undefined() {
                        def = p.get("plugin").map_err(|e| caught(&ctx, e))?;
                    }
                    let start: Function = p.get("startVencord").map_err(|e| caught(&ctx, e))?;
                    start.call::<_, ()>((def,)).map_err(|e| caught(&ctx, e))?;
                }
                PluginKind::Pickle => {
                    ctx.eval::<(), _>(src).map_err(|e| caught(&ctx, e))?;
                    let def: Value = p.get("plugin").map_err(|e| caught(&ctx, e))?;
                    if def.is_object() {
                        let start: Function = p.get("startVencord").map_err(|e| caught(&ctx, e))?;
                        start.call::<_, ()>((def,)).map_err(|e| caught(&ctx, e))?;
                    }
                }
            }
            // Pull descriptive fields from the plugin object if it has them.
            let desc: Option<String> = ctx
                .eval(
                    "(() => { const p = __pickle.plugin; if (!p || typeof p !== 'object') return undefined; \
                     return JSON.stringify({ name: p.name, description: p.description, version: p.version, \
                     authors: Array.isArray(p.authors) ? p.authors.map(a => a && a.name || String(a)).join(', ') : undefined }); })()",
                )
                .unwrap_or(None);
            Ok(desc)
        });
        self.disarm();
        self.drain_jobs();
        match result {
            Ok(desc) => {
                self.loaded.insert(id.to_string(), Loaded { ctx });
                #[derive(Deserialize)]
                struct D {
                    name: Option<String>,
                    description: Option<String>,
                    version: Option<String>,
                    authors: Option<String>,
                }
                let d: Option<D> = desc.and_then(|s| serde_json::from_str(&s).ok());
                self.set_info(id, |p| {
                    p.running = true;
                    p.error = None;
                    if let Some(d) = d {
                        if let Some(n) = d.name.filter(|s| !s.is_empty()) {
                            p.name = n;
                        }
                        if let Some(n) = d.description.filter(|s| !s.is_empty()) {
                            p.description = n;
                        }
                        if let Some(n) = d.version.filter(|s| !s.is_empty()) {
                            p.version = n;
                        }
                        if let Some(n) = d.authors.filter(|s| !s.is_empty()) {
                            p.author = n;
                        }
                    }
                });
                self.state.log(id, "info", "started");
            }
            Err(e) => {
                self.state.log(id, "error", &format!("failed to start: {e}"));
                self.set_info(id, |p| {
                    p.running = false;
                    p.error = Some(e);
                });
                self.forget(id);
            }
        }
    }

    /// Remove everything a plugin registered with the host.
    fn forget(&mut self, id: &str) {
        self.state.commands.lock().retain(|c| c.plugin != id);
        self.state.presend.lock().remove(id);
        self.timers.retain(|t| t.plugin != id);
        self.emit(PluginOut::ClearCss { plugin: id.to_string() });
    }

    fn unload(&mut self, id: &str) {
        if self.loaded.contains_key(id) {
            self.call(id, "stop", ());
            self.loaded.remove(id);
            self.state.log(id, "info", "stopped");
        }
        self.forget(id);
        self.set_info(id, |p| p.running = false);
    }

    fn call<A>(&mut self, id: &str, func: &str, args: A)
    where
        A: for<'js> rquickjs::function::IntoArgs<'js>,
    {
        let _ = self.call_ret::<(), A>(id, func, args);
    }

    fn call_ret<R, A>(&mut self, id: &str, func: &str, args: A) -> Option<R>
    where
        R: for<'js> rquickjs::FromJs<'js>,
        A: for<'js> rquickjs::function::IntoArgs<'js>,
    {
        let loaded = self.loaded.get(id)?;
        self.arm();
        let r = loaded.ctx.with(|ctx| -> Result<R, String> {
            let p: Object = ctx.globals().get("__pickle").map_err(|e| caught(&ctx, e))?;
            let f: Function = p.get(func).map_err(|e| caught(&ctx, e))?;
            f.call::<A, R>(args).map_err(|e| caught(&ctx, e))
        });
        self.disarm();
        self.drain_jobs();
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.state.log(id, "error", &format!("{func}: {e}"));
                None
            }
        }
    }

    fn drain_jobs(&mut self) {
        self.arm();
        for _ in 0..10_000 {
            match self.rt.execute_pending_job() {
                Ok(true) => continue,
                Ok(false) => break,
                Err(e) => warn!("plugin job failed: {e:?}"),
            }
        }
        self.disarm();
        self.apply_local();
    }

    fn apply_local(&mut self) {
        let reqs = std::mem::take(&mut self.local.borrow_mut().timers);
        for r in reqs {
            if r.cancel {
                self.timers.retain(|t| !(t.plugin == r.plugin && t.id == r.id));
            } else {
                let d = Duration::from_millis(r.ms.max(if r.repeat { 4.0 } else { 0.0 }) as u64);
                self.timers.push(Timer { due: Instant::now() + d, plugin: r.plugin, id: r.id, every: r.repeat.then_some(d) });
            }
        }
    }

    fn fire_timers(&mut self) {
        let now = Instant::now();
        let mut due: Vec<(String, u32)> = Vec::new();
        for t in &mut self.timers {
            if t.due <= now {
                due.push((t.plugin.clone(), t.id));
                if let Some(e) = t.every {
                    t.due = now + e;
                }
            }
        }
        self.timers.retain(|t| t.due > now);
        for (plugin, id) in due {
            self.call(&plugin, "fireTimer", (id,));
        }
    }
}

#[cfg(test)]
mod tests;
