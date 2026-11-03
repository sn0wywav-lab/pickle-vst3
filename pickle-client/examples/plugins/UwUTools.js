// Vencord-style plugin written as plain JavaScript (ES module).
// The same imports a Vencord userplugin uses are provided by Pickle's shims.
import definePlugin from "@utils/types";
import { ApplicationCommandInputType, ApplicationCommandOptionType, findOption, sendBotMessage } from "@api/Commands";
import { addMessagePreSendListener, removeMessagePreSendListener } from "@api/MessageEvents";
import { Devs } from "@utils/constants";

const uwuify = (s) =>
  s.replace(/[rl]/g, "w").replace(/[RL]/g, "W").replace(/n([aeiou])/g, "ny$1");

const preSend = (_channelId, msg) => {
  if (msg.content.startsWith("!uwu ")) msg.content = uwuify(msg.content.slice(5));
};

export default definePlugin({
  name: "UwUTools",
  description: "Vencord-style example: /uwu and /coinflip commands plus a '!uwu ' pre-send prefix.",
  authors: [Devs.Pickle],
  commands: [
    {
      name: "uwu",
      description: "uwuify some text and send it",
      inputType: ApplicationCommandInputType.BUILT_IN,
      options: [{ name: "text", description: "what to say", type: ApplicationCommandOptionType.STRING, required: true }],
      execute: (args) => ({ content: uwuify(findOption(args, "text", "")) }),
    },
    {
      name: "coinflip",
      description: "Flip a coin (only you see the result)",
      execute: (_args, ctx) => {
        sendBotMessage(ctx.channel.id, { content: Math.random() < 0.5 ? "Heads!" : "Tails!" });
      },
    },
  ],
  flux: {
    MESSAGE_CREATE({ message }) {
      if (message && /\buwu\b/i.test(message.content || "")) console.log("someone said uwu in", message.channel_id);
    },
  },
  start() {
    addMessagePreSendListener(preSend);
  },
  stop() {
    removeMessagePreSendListener(preSend);
  },
});
