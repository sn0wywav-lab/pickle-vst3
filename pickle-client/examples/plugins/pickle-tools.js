/**
 * @name Pickle Tools
 * @author Pickle
 * @version 1.0.0
 * @description Native Pickle API example: /now and /count commands, events, timers and storage.
 */
Pickle.commands.register({
  name: "now",
  description: "Send the current time",
  execute: () => ({ content: `It is ${new Date().toUTCString()}` }),
});

Pickle.commands.register({
  name: "count",
  description: "How many messages this plugin has seen (local only)",
  execute: (_args, ctx) => {
    Pickle.sendLocal(ctx.channel.id, `Seen ${Pickle.storage.get("seen", 0)} messages so far.`);
  },
});

Pickle.on("MESSAGE_CREATE", () => {
  Pickle.storage.set("seen", Pickle.storage.get("seen", 0) + 1);
});

setTimeout(() => console.log("Pickle Tools ready"), 0);
