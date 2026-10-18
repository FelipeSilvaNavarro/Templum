// Templum's extension for Pi (docs/specs/connections.md, Pi), loaded for one session with `-e`:
// Pi's turns handed to `templum hook pi` as one JSON line each, in order, and Templum's MCP
// server where Pi has MCP. Plain JavaScript, no dependency, no type annotation: Pi loads it as
// TypeScript, `node` as a module.
import { spawnSync } from "node:child_process";

const PROGRAM = __TEMPLUM_PROGRAM__;

export default function (pi) {
  // A process this Pi starts (a `pi -p` from its shell tool) inherits the variable: it says
  // nothing for the block.
  const hooked = process.env.TEMPLUM_HOOKED;
  if (hooked && hooked !== String(process.pid)) return;
  process.env.TEMPLUM_HOOKED = String(process.pid);
  if (typeof pi.registerMcpServer === "function") {
    try {
      // A tool call may wait seven days (seconds): a question waits for its person.
      pi.registerMcpServer("templum", { command: PROGRAM, args: ["mcp"], timeout: 604800 });
    } catch {}
  }
  // Waits for the hook (events stay in order); it gives up within the limit when Templum is gone.
  const send = (line) => {
    try {
      spawnSync(PROGRAM, ["hook", "pi"], {
        input: JSON.stringify(line),
        stdio: ["pipe", "ignore", "ignore"],
        timeout: 3000,
      });
    } catch {}
  };
  const tell = (type, fields) =>
    pi.on(type, async (event) => {
      try {
        send({ type, ...(fields ? fields(event) : {}) });
      } catch {}
    });
  // The last run's answer, told once Pi won't go on by itself (a retry, a compaction, a
  // follow-up): `agent_end` comes before those.
  let answer = null;
  pi.on("agent_end", async (event) => {
    try {
      const said = (event?.messages ?? []).filter((m) => m?.role === "assistant").pop();
      const text = (Array.isArray(said?.content) ? said.content : [])
        .filter((c) => c?.type === "text" && c.text)
        .map((c) => c.text)
        .join("\n");
      answer = text || null;
    } catch {}
  });
  tell("session_start");
  tell("agent_start", () => {
    answer = null;
    return {};
  });
  tell("tool_result");
  tell("agent_settled", () => ({ answer }));
  tell("ui_prompt_start", (event) => ({ title: event?.title ?? null }));
  tell("ui_prompt_end");
  tell("session_shutdown", (event) => ({ reason: event?.reason ?? null }));
}
