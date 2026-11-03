/**
 * @name BetterShrug
 * @author Pickle
 * @version 1.0.0
 * @description BetterDiscord-format example: toast on start, BdApi.Data counter, CSS injection and an outgoing-message hook that expands :shrug:.
 */
module.exports = class BetterShrug {
  constructor(meta) {
    this.meta = meta;
    this.api = new BdApi(meta.name);
  }

  start() {
    const starts = (this.api.Data.load("starts") || 0) + 1;
    this.api.Data.save("starts", starts);
    this.api.UI.showToast(`${this.meta.name} started (${starts}x)`, { type: "success" });

    // Webpack patching can't exist in a native client; this is a safe no-op.
    this.unpatch = BdApi.Patcher.after(this.meta.name, {}, "render", () => {});

    // CSS custom properties from plugins layer on top of the active theme.
    this.api.DOM.addStyle(":root { --status-positive: #5ad16b; }");

    // Native hook exposed to BetterDiscord plugins as BdApi.Pickle.
    this.unhook = BdApi.Pickle.onBeforeSend((channelId, msg) => {
      msg.content = msg.content.replaceAll(":shrug:", "¯\\_(ツ)_/¯");
    });
  }

  stop() {
    if (this.unhook) this.unhook();
    if (this.unpatch) this.unpatch();
    this.api.DOM.removeStyle();
  }

  onSwitch() {
    // Called whenever you open another channel (classic BD hook).
  }
};
