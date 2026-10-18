// Templum's plugin for OpenCode and Kilo Code (docs/specs/connections.md, OpenCode and Kilo
// Code): each event of the main session that tells its turns, handed to `templum hook opencode`
// as one JSON line, in order. Plain JavaScript, no dependency.
import { spawnSync } from "node:child_process";

const PROGRAM = __TEMPLUM_PROGRAM__;
// The events the hook turns into the block's state.
const TOLD = new Set([
  "session.status",
  "session.idle",
  "session.error",
  "permission.asked",
  "permission.replied",
  "question.asked",
  "question.replied",
  "question.rejected",
]);

export const Templum = async ({ client }) => {
  // A process this OpenCode starts (an `opencode run` from its shell tool) inherits the
  // variable: its plugin says nothing for the block.
  const hooked = process.env.TEMPLUM_HOOKED;
  const quiet = Boolean(hooked) && hooked !== String(process.pid);
  if (!hooked) process.env.TEMPLUM_HOOKED = String(process.pid);
  // Subagents' sessions: never the block's turn.
  const children = new Set();
  // Waits for the hook (events stay in order); it gives up within the limit when Templum is gone.
  const send = (line) => {
    try {
      spawnSync(PROGRAM, ["hook", "opencode"], {
        input: JSON.stringify(line),
        stdio: ["pipe", "ignore", "ignore"],
        timeout: 3000,
      });
    } catch {}
  };
  // The session's last assistant text.
  const answer = async (id) => {
    try {
      const got = await client.session.messages({ path: { id } });
      const messages = Array.isArray(got) ? got : (got?.data ?? []);
      const last = messages.filter((m) => m?.info?.role === "assistant").pop();
      const text = (last?.parts ?? [])
        .filter((p) => p?.type === "text" && p.text)
        .map((p) => p.text)
        .join("\n");
      return text || null;
    } catch {
      return null;
    }
  };
  return {
    event: async ({ event }) => {
      try {
        if (quiet) return;
        const properties = event?.properties ?? {};
        const info = properties.info;
        if (info?.parentID) children.add(info.id);
        if (!TOLD.has(event?.type)) return;
        const session = properties.sessionID ?? info?.id;
        if (children.has(session)) return;
        const said = event.type === "session.idle" && session ? await answer(session) : null;
        send({ type: event.type, properties, answer: said });
      } catch {}
    },
    "chat.message": async (input) => {
      try {
        if (quiet || children.has(input?.sessionID)) return;
        send({ type: "chat.message", properties: { sessionID: input?.sessionID }, answer: null });
      } catch {}
    },
  };
};
