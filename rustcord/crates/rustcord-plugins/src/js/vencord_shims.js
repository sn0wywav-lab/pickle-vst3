// Each block is "// @module <specifier>" followed by the module source.
// @module @utils/types
export default function definePlugin(p) { return globalThis.definePlugin(p); }
export const OptionType = { STRING: 0, NUMBER: 1, BIGINT: 2, BOOLEAN: 3, SELECT: 4, SLIDER: 5, COMPONENT: 6, CUSTOM: 7 };
export const StartAt = { Init: "Init", DOMContentLoaded: "DOMContentLoaded", WebpackReady: "WebpackReady" };
export const ReporterTestable = { None: 1, Start: 2, Patches: 4, FluxEvents: 8, All: 14 };
export function makeRange(start, end, step = 1) { const r = []; for (let i = start; i <= end; i += step) r.push(i); return r; }
// @module @api/Commands
export const ApplicationCommandInputType = { BUILT_IN: 0, BUILT_IN_TEXT: 1, BUILT_IN_INTEGRATION: 2, BOT: 3, PLACEHOLDER: 4 };
export const ApplicationCommandOptionType = { SUB_COMMAND: 1, SUB_COMMAND_GROUP: 2, STRING: 3, INTEGER: 4, BOOLEAN: 5, USER: 6, CHANNEL: 7, ROLE: 8, MENTIONABLE: 9, NUMBER: 10, ATTACHMENT: 11 };
export const ApplicationCommandType = { CHAT_INPUT: 1, USER: 2, MESSAGE: 3 };
export const RequiredMessageOption = { type: 3, name: "message", description: "", required: true };
export const OptionalMessageOption = { type: 3, name: "message", description: "", required: false };
export function sendBotMessage(channelId, message) { Rustcord.sendLocal(channelId, (message && message.content) || ""); return message; }
export function findOption(args, name, fallback) { const o = (args || []).find((a) => a.name === name); return o ? o.value : fallback; }
export const findOptionOrDefault = findOption;
export function registerCommand(cmd) { return Rustcord.commands.register(cmd); }
export function unregisterCommand(name) { Rustcord.commands.unregister(name); }
// @module @api/MessageEvents
const pre = new Map();
export function addMessagePreSendListener(fn) { pre.set(fn, Rustcord.onBeforeSend(fn)); return fn; }
export function removeMessagePreSendListener(fn) { const u = pre.get(fn); if (u) { u(); pre.delete(fn); } }
export const addPreSendListener = addMessagePreSendListener;
export const removePreSendListener = removeMessagePreSendListener;
export function addMessagePreEditListener(fn) { return fn; }
export function removeMessagePreEditListener() {}
export const addPreEditListener = addMessagePreEditListener;
export const removePreEditListener = removeMessagePreEditListener;
// @module @api/Settings
export function definePluginSettings(def) {
  const store = new Proxy({}, {
    get: (_, k) => { const v = Rustcord.storage.get("setting:" + String(k)); return v !== undefined ? v : def[k] && def[k].default; },
    set: (_, k, v) => { Rustcord.storage.set("setting:" + String(k), v); return true; },
  });
  return { def, store, withPrivateSettings() { return this; } };
}
export const Settings = { plugins: new Proxy({}, { get: () => ({ enabled: true }) }) };
export function useSettings() { return Settings; }
// @module @utils/constants
export const Devs = new Proxy({}, { get: (_, k) => ({ name: String(k), id: 0n }) });
export const EquicordDevs = Devs;
export const SUPPORT_CHANNEL_ID = "0";
// @module @utils/Logger
export class Logger {
  constructor(name, color) { this.name = name; this.color = color; }
  log(...a) { console.log(`[${this.name}]`, ...a); }
  info(...a) { console.info(`[${this.name}]`, ...a); }
  warn(...a) { console.warn(`[${this.name}]`, ...a); }
  error(...a) { console.error(`[${this.name}]`, ...a); }
  debug(...a) { console.debug(`[${this.name}]`, ...a); }
}
// @module @webpack
const nothing = () => undefined;
const lazy = () => new Proxy({}, { get: () => undefined });
export const findByProps = nothing, findByPropsLazy = lazy, findByCode = nothing, findByCodeLazy = lazy,
  findStore = nothing, findStoreLazy = lazy, findComponentByCode = nothing, findComponentByCodeLazy = lazy,
  findExportedComponentLazy = lazy, find = nothing, findLazy = lazy, findAll = () => [], mapMangledModuleLazy = lazy;
export const filters = { byProps: () => () => false, byCode: () => () => false, byStoreName: () => () => false };
export function waitFor() {}
// @module @webpack/common
export const FluxDispatcher = {
  subscribe: (t, fn) => Rustcord.on(t, fn),
  unsubscribe: (t, fn) => Rustcord.off(t, fn),
  dispatch() {},
};
export const UserStore = { getCurrentUser: () => Rustcord.context().me, getUser: () => undefined };
export const SelectedChannelStore = { getChannelId: () => Rustcord.context().channelId };
export const SelectedGuildStore = { getGuildId: () => Rustcord.context().guildId };
export const ChannelStore = { getChannel: (id) => ({ id }) };
export const Toasts = { show: (o) => Rustcord.toast(o && o.message), Type: { MESSAGE: 0, SUCCESS: 1, FAILURE: 2 }, genId: () => String(Math.random()) };
export const showToast = (t) => Rustcord.toast(t);
export const React = undefined;
export const RestAPI = undefined;
