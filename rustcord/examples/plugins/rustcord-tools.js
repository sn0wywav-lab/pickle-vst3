/**
 * @name Rustcord Tools
 * @author Rustcord
 * @version 1.0.0
 * @description Native Rustcord API example: /now and /count commands, events, timers and storage.
 */
Rustcord.commands.register({
  name: "now",
  description: "Send the current time",
  execute: () => ({ content: `It is ${new Date().toUTCString()}` }),
});

Rustcord.commands.register({
  name: "count",
  description: "How many messages this plugin has seen (local only)",
  execute: (_args, ctx) => {
    Rustcord.sendLocal(ctx.channel.id, `Seen ${Rustcord.storage.get("seen", 0)} messages so far.`);
  },
});

Rustcord.on("MESSAGE_CREATE", () => {
  Rustcord.storage.set("seen", Rustcord.storage.get("seen", 0) + 1);
});

setTimeout(() => console.log("Rustcord Tools ready"), 0);
