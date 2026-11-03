// Runs first in every plugin context. `__native` is provided by the host.
(() => {
  "use strict";
  const N = globalThis.__native;
  const P = (globalThis.__rustcord = {
    listeners: Object.create(null),
    commands: [],
    presend: [],
    fetches: Object.create(null),
    nextFetch: 1,
    timers: Object.create(null),
    nextTimer: 1,
    warned: Object.create(null),
    plugin: null,
  });

  const fmt = (args) =>
    args
      .map((a) => {
        if (typeof a === "string") return a;
        if (a instanceof Error) return a.stack || String(a);
        try { return JSON.stringify(a); } catch { return String(a); }
      })
      .join(" ");
  globalThis.console = {
    log: (...a) => N.log("info", fmt(a)),
    info: (...a) => N.log("info", fmt(a)),
    debug: (...a) => N.log("debug", fmt(a)),
    warn: (...a) => N.log("warn", fmt(a)),
    error: (...a) => N.log("error", fmt(a)),
    trace: (...a) => N.log("debug", fmt(a)),
  };
  globalThis.window = globalThis;
  globalThis.global = globalThis;
  globalThis.self = globalThis;

  // ---- timers -------------------------------------------------------------
  const addTimer = (fn, ms, repeat, args) => {
    const id = P.nextTimer++;
    P.timers[id] = { fn, args, repeat };
    N.timer(id, Math.max(0, Number(ms) || 0), repeat);
    return id;
  };
  const clear = (id) => { if (P.timers[id]) { delete P.timers[id]; N.clearTimer(id); } };
  globalThis.setTimeout = (fn, ms, ...args) => addTimer(fn, ms, false, args);
  globalThis.setInterval = (fn, ms, ...args) => addTimer(fn, ms, true, args);
  globalThis.clearTimeout = clear;
  globalThis.clearInterval = clear;
  globalThis.queueMicrotask = (fn) => Promise.resolve().then(fn);
  P.fireTimer = (id) => {
    const t = P.timers[id];
    if (!t) return;
    if (!t.repeat) delete P.timers[id];
    try { if (typeof t.fn === "function") t.fn(...(t.args || [])); }
    catch (e) { console.error("timer:", e); }
  };

  // ---- events / hooks -----------------------------------------------------
  const on = (ev, fn) => { (P.listeners[ev] ||= []).push(fn); return () => off(ev, fn); };
  const off = (ev, fn) => { const l = P.listeners[ev]; if (l) P.listeners[ev] = l.filter((f) => f !== fn); };
  P.dispatch = (ev, json) => {
    const l = P.listeners[ev];
    if (!l || !l.length) return;
    const payload = JSON.parse(json);
    for (const fn of l.slice()) {
      try { fn(payload); } catch (e) { console.error(`${ev} handler:`, e); }
    }
  };
  P.runPresend = (channelId, content) => {
    const msg = { content, channelId, tts: false, invalidEmojis: [], validNonShortcutEmojis: [] };
    let cancel = false;
    for (const fn of P.presend.slice()) {
      try {
        const r = fn(channelId, msg, {});
        if (r === false || (r && r.cancel)) cancel = true;
      } catch (e) { console.error("pre-send listener:", e); }
    }
    return JSON.stringify({ content: String(msg.content ?? ""), cancel });
  };
  const syncCommands = () =>
    N.commandsChanged(JSON.stringify(P.commands.map((c) => ({
      name: String(c.name),
      description: String(c.description || ""),
      options: (c.options || []).map((o) => ({ name: String(o.name), description: String(o.description || ""), required: !!o.required })),
    }))));
  P.runCommand = (name, argsJson, ctxJson) => {
    const cmd = P.commands.find((c) => c.name === name);
    if (!cmd) return false;
    const args = JSON.parse(argsJson);
    const ctx = JSON.parse(ctxJson);
    Promise.resolve()
      .then(() => cmd.execute(args, ctx))
      .then((r) => {
        if (r && typeof r.content === "string" && r.content.length) N.send(String(ctx.channel.id), r.content);
      })
      .catch((e) => { console.error(`/${name}:`, e); N.toast(`/${name} failed: ${e}`, "error"); });
    return true;
  };
  P.fetchDone = (id, ok, status, body) => {
    const f = P.fetches[id];
    if (!f) return;
    delete P.fetches[id];
    if (!ok) return f.reject(new Error(body));
    f.resolve({
      ok: status >= 200 && status < 300,
      status,
      text: async () => body,
      json: async () => JSON.parse(body),
    });
  };

  // ---- native Rustcord API --------------------------------------------------
  const Rustcord = (globalThis.Rustcord = {
    version: N.version(),
    on, off,
    onBeforeSend(fn) {
      P.presend.push(fn);
      N.presendChanged(P.presend.length);
      return () => { P.presend = P.presend.filter((f) => f !== fn); N.presendChanged(P.presend.length); };
    },
    commands: {
      register(cmd) {
        if (!cmd || !cmd.name || typeof cmd.execute !== "function") throw new Error("command needs name + execute()");
        P.commands = P.commands.filter((c) => c.name !== cmd.name);
        P.commands.push(cmd);
        syncCommands();
        return () => Rustcord.commands.unregister(cmd.name);
      },
      unregister(name) { P.commands = P.commands.filter((c) => c.name !== name); syncCommands(); },
    },
    sendMessage(channelId, content) { N.send(String(channelId), String(content)); },
    sendLocal(channelId, content) { N.local(String(channelId), String(content)); },
    toast(text, type) { N.toast(String(text), String(type || "info")); },
    alert(title, body) { N.alert(String(title), String(body ?? "")); },
    css: {
      set(id, css) { N.css(String(id), String(css)); },
      remove(id) { N.css(String(id), null); },
    },
    storage: {
      get(k, d) { const v = N.storeGet(String(k)); return v === undefined || v === null ? d : JSON.parse(v); },
      set(k, v) { N.storeSet(String(k), JSON.stringify(v)); },
      delete(k) { N.storeDel(String(k)); },
    },
    context() { return JSON.parse(N.context()); },
    fetch(url, opts = {}) {
      return new Promise((resolve, reject) => {
        const id = P.nextFetch++;
        P.fetches[id] = { resolve, reject };
        N.fetch(id, String(url), String(opts.method || "GET"), opts.body == null ? null : String(opts.body));
      });
    },
  });
  globalThis.fetch = Rustcord.fetch;

  // ---- Vencord-style lifecycle --------------------------------------------
  globalThis.definePlugin = (p) => { P.plugin = p; return p; };
  P.startVencord = (p) => {
    if (!p || typeof p !== "object") throw new Error("plugin did not export definePlugin({...})");
    P.plugin = p;
    if (Array.isArray(p.patches) && p.patches.length)
      console.warn(`${p.name}: ${p.patches.length} webpack patch(es) skipped (native client has no Discord bundle)`);
    for (const c of p.commands || []) Rustcord.commands.register(c);
    for (const [ev, fn] of Object.entries(p.flux || {})) on(ev, fn.bind(p));
    if (typeof p.onBeforeMessageSend === "function") Rustcord.onBeforeSend((ch, msg) => p.onBeforeMessageSend(ch, msg));
    if (typeof p.start === "function") p.start();
  };
  P.stop = () => {
    const p = P.plugin;
    if (p && typeof p.stop === "function") p.stop();
  };

  // ---- BetterDiscord BdApi shim ------------------------------------------
  const warnOnce = (k, msg) => { if (!P.warned[k]) { P.warned[k] = 1; console.warn(msg); } };
  const unpatch = () => {};
  const unsupported = (what) => () => { warnOnce(what, `BdApi.${what} is not available in a native client; ignored.`); return unpatch; };
  const store = {
    load: (p, k) => Rustcord.storage.get(`${p}:${k}`),
    save: (p, k, v) => Rustcord.storage.set(`${p}:${k}`, v),
    delete: (p, k) => Rustcord.storage.delete(`${p}:${k}`),
  };
  const never = () => () => false;
  const makeApi = (bound) => {
    const toast = (t, o) => Rustcord.toast(t, (o && o.type) || "info");
    const alert = (title, c) => Rustcord.alert(title, typeof c === "string" ? c : "");
    const api = {
      version: "1.11.0-rustcord",
      UI: {
        showToast: toast,
        alert,
        showConfirmationModal: (title, c, o = {}) => { alert(title, c); if (typeof o.onConfirm === "function") warnOnce("confirm", "confirmation modals are shown as alerts"); return ""; },
        showNotice: (t) => { Rustcord.toast(typeof t === "string" ? t : "notice"); return () => {}; },
        createTooltip: () => ({ show() {}, hide() {} }),
        buildSettingsPanel: () => null,
        openDialog: async () => ({ cancelled: true }),
      },
      Data: bound
        ? { load: (k) => store.load(bound, k), save: (k, v) => store.save(bound, k, v), delete: (k) => store.delete(bound, k) }
        : store,
      DOM: {
        addStyle: (id, css) => { if (css === undefined) { css = id; id = bound || "bd"; } Rustcord.css.set(id, css); },
        removeStyle: (id) => Rustcord.css.remove(id || bound || "bd"),
        createElement: () => { throw new Error("DOM is not available in a native client"); },
        parseHTML: () => null,
        onRemoved: () => unpatch,
        animate: () => unpatch,
      },
      Logger: {
        log: (...a) => console.log(...(bound ? [`[${bound}]`] : []), ...a),
        info: (...a) => console.info(...(bound ? [`[${bound}]`] : []), ...a),
        warn: (...a) => console.warn(...(bound ? [`[${bound}]`] : []), ...a),
        error: (...a) => console.error(...(bound ? [`[${bound}]`] : []), ...a),
        debug: (...a) => console.debug(...(bound ? [`[${bound}]`] : []), ...a),
        stacktrace: (...a) => console.error(...a),
      },
      Patcher: { before: unsupported("Patcher"), after: unsupported("Patcher"), instead: unsupported("Patcher"), unpatchAll() {}, getPatchesByCaller: () => [] },
      Webpack: {
        getModule: () => undefined, getModules: () => [], getByKeys: () => undefined, getByPrototypeKeys: () => undefined,
        getByStrings: () => undefined, getBySource: () => undefined, getStore: () => undefined, getMangled: () => ({}),
        getWithKey: () => [undefined, undefined], getBulk: () => [],
        waitForModule: () => new Promise(() => {}),
        Filters: { byKeys: never, byProps: never, byPrototypeKeys: never, byStrings: never, byDisplayName: never, byStoreName: never, bySource: never, combine: never },
        modules: {},
      },
      ContextMenu: { patch: unsupported("ContextMenu"), unpatch() {}, buildItem: () => null, buildMenu: () => null, open() {}, close() {} },
      Plugins: { isEnabled: () => true, getAll: () => [], get: () => undefined, enable() {}, disable() {}, toggle() {}, reload() {} },
      Themes: { isEnabled: () => false, getAll: () => [], get: () => undefined, enable() {}, disable() {}, toggle() {}, reload() {} },
      Utils: {
        debounce: (fn, ms) => { let t; return (...a) => { clear(t); t = setTimeout(() => fn(...a), ms); }; },
        escapeHTML: (s) => String(s).replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`),
        extend: Object.assign,
        findInTree: () => undefined,
        getNestedValue: (o, p) => String(p).split(".").reduce((a, k) => (a == null ? a : a[k]), o),
        className: (...a) => a.flat().filter(Boolean).join(" "),
        semverCompare: (a, b) => String(a).localeCompare(String(b), undefined, { numeric: true }),
      },
      Net: { fetch: Rustcord.fetch },
      React: undefined,
      ReactDOM: undefined,
      Components: {},
      Rustcord,
    };
    Object.assign(api, {
      showToast: toast, alert, showConfirmationModal: api.UI.showConfirmationModal, showNotice: api.UI.showNotice,
      loadData: store.load, saveData: store.save, deleteData: store.delete, getData: store.load, setData: store.save,
      injectCSS: (id, css) => Rustcord.css.set(id, css), clearCSS: (id) => Rustcord.css.remove(id),
      findModule: () => undefined, findModuleByProps: () => undefined, findModuleByPrototypes: () => undefined,
      findModuleByDisplayName: () => undefined, findAllModules: () => [],
      monkeyPatch: unsupported("monkeyPatch"), onRemoved: () => unpatch, suppressErrors: (f) => f,
      isPluginEnabled: () => true, isThemeEnabled: () => false, getPlugin: () => undefined,
      testJSON: (s) => { try { JSON.parse(s); return true; } catch { return false; } },
      linkJS() {}, unlinkJS() {}, linkCSS() {}, unlinkCSS() {},
    });
    return api;
  };
  const BdApi = function BdApi(name) { return makeApi(String(name)); };
  Object.assign(BdApi, makeApi(null));
  globalThis.BdApi = BdApi;
  globalThis.ZeresPluginLibrary = undefined;

  // CommonJS host for *.plugin.js
  const unavailable = (m) => { throw new Error(`require("${m}") is not available in Rustcord`); };
  globalThis.require = (m) => {
    if (m === "process") return { platform: "rustcord", env: {} };
    return unavailable(m);
  };
  P.startBetterDiscord = (factory, meta) => {
    const module = { exports: {} };
    factory(module, module.exports, globalThis.require, meta);
    let exp = module.exports;
    if (exp && exp.default) exp = exp.default;
    let inst;
    if (typeof exp === "function") {
      try { inst = new exp(meta); } catch (e) { if (e instanceof TypeError) inst = exp(meta); else throw e; }
    } else inst = exp;
    if (!inst || typeof inst.start !== "function") throw new Error("BetterDiscord plugin has no start()");
    P.plugin = inst;
    if (typeof inst.onMessage === "function") on("MESSAGE_CREATE", (e) => inst.onMessage(e.message));
    if (typeof inst.onSwitch === "function") on("CHANNEL_SELECT", () => inst.onSwitch());
    if (typeof inst.load === "function") inst.load();
    inst.start();
  };
})();
