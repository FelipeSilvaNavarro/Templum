# Every agent that can take it: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the gaps of the parity audit that the agents' own command lines allow (kanban card 44): the small ones
first (`spawn_agent`'s model and effort, Settings ▸ Connections' words, the switch reaching Gemini CLI and every
running agent, Gemini CLI's extension asked for once, Copilot CLI, Cursor CLI and Crush resuming their own session),
then Templum's server and hooks for GitHub Copilot CLI, OpenCode and Kilo Code, Qwen Code and Pi, opened by Templum or
typed by hand.

**Architecture:** Each agent's files go to `<data>/<agent>/`, written by a module of `templum-usage` that also returns
the words (or the environment variable) the agent takes; `Workspace::typed` appends the words for the agents Templum
opens, a block Templum opens gets the variable in its shell's environment, and `Workspace::agent_env` hands every new
terminal the same words and variables for the shells' functions, as card 42 does for `claude` and `codex`. Each
agent's hooks or plugin events reach `templum hook <agent>`, a parser per agent in `templum-mcp/src/hooks.rs` turning
them into the existing `AgentEvent`s; nothing in the workspace's state machine changes.

**Tech Stack:** Rust 2024; bash (3.2 up), zsh, fish, PowerShell 7 and 5.1; JavaScript for OpenCode's plugin and Pi's
extension (no dependency, run by the agents themselves; tested with `node` when it is installed); headless GPUI tests
(`workspace/tests/`), the real-shell PTY tests of `templum-terminal/src/integration.rs` (`mod shells`).

**Spec:** [docs/specs/connections.md](../specs/connections.md#every-agent-that-can-take-it) (read that section, and
[Agents typed in a terminal](../specs/connections.md#agents-typed-in-a-terminal) before it), [agents.md](../specs/agents.md#a-model-and-an-effort)
for the model and effort table, and [agent-parity.md](../specs/agent-parity.md#after-card-44) for what closes and what
stays.

## Global Constraints

- **Files** (each written only when it changed, `templum_usage::claude::write_changed`): `<data>/copilot/mcp.json`,
  `<data>/copilot/plugin/plugin.json`, `<data>/copilot/plugin/hooks.json`, `<data>/opencode/templum.js`,
  `<data>/qwen/mcp.json`, `<data>/qwen/settings.json`, `<data>/pi/templum.ts`. Nothing outside `<data>`.
- **Program:** every new file runs `<data>/bin/templum` (`templum.exe` on Windows; `templum_usage::bin`, Task 6), as
  Gemini CLI's extension does, so the new modules take `(data: &Path, windows: bool)` and the workspace gives these
  agents anything only when `Gives::on_path` (that link is on the terminal's `PATH`). A module returns `None` (no
  connections for that agent) when a path holds a `'` or a line break, or when `typed_path` refuses one.
- **Hook commands:** `'<data>/bin/templum' hook <words>` on Unix and macOS, `& '<data>/bin/templum' hook <words>` on
  Windows (`templum_usage::hook_command`, Task 6). Every hook or plugin call of `templum hook` prints nothing and
  exits 0.
- **Timeouts:** a tool call may wait seven days: `604800000` (ms) for Copilot CLI, OpenCode, Kilo Code and Qwen Code
  (`templum_usage::MCP_TIMEOUT_MS`), `604800` (s) for Pi. Hooks: 5 s.
- **Words** (raw, one per line in the variables; `typed` puts each through `typed_path`; Windows' `\` as `/`):
  - Copilot CLI: `--allow-all-mcp-server-instructions`, `--additional-mcp-config=@<data>/copilot/mcp.json`,
    `--plugin-dir=<data>/copilot/plugin`.
  - Qwen Code: `--mcp-config=<data>/qwen/mcp.json`.
  - Pi: `-e`, `<data>/pi/templum.ts`.
  - The last word always names a file in `<data>`: it is the shells' marker for "already given". Qwen Code's
    function also gives none when the person's words hold `--mcp-config` or `--mcp-config=…` (it takes one), and
    Copilot CLI's only when the person's first word is none or starts with `-` (no subcommand: AWS's `copilot`).
- **Environment variables an agent takes:** `OPENCODE_CONFIG_CONTENT`, `KILO_CONFIG_CONTENT` (the same JSON),
  `QWEN_CODE_SYSTEM_SETTINGS_PATH` (`<data>/qwen/settings.json`). Never over a non-empty one already there.
- **Terminal variables** (always set, empty when there is nothing to give, as `TEMPLUM_CLAUDE_ARGS`):
  `TEMPLUM_COPILOT_ARGS`, `TEMPLUM_OPENCODE_CONFIG`, `TEMPLUM_KILO_CONFIG`, `TEMPLUM_QWEN_ARGS`,
  `TEMPLUM_QWEN_SETTINGS`, `TEMPLUM_PI_ARGS`, and `TEMPLUM_HOOKED` (always empty).
- **Agents are named by constants** beside `Agent::CLAUDE` and `Agent::CODEX` (`GEMINI`, `COPILOT`, `OPENCODE`,
  `KILO`, `QWEN`, `PI`), each added by the first task that needs it, as `SPECS`' index.
- **Same rules as `typed`** (`Workspace::gives`): nothing without a data folder, a program, a bus and `[connections]`
  on, nothing under WSL. Unlike Codex's flags, these go into cmd and Windows PowerShell 5.1 too.
- **Copy, exact:** the switch's description "Agents opened from the canvas or typed in a terminal get Templum's server
  and hooks: they read and type into the blocks wired to them"; `config.toml`'s comment "The agents Templum opens, or
  that are typed in its terminals, get its MCP server and hooks: they can read and type into the blocks wired to them
  (docs/specs/connections.md)."; Other agents' "Grok Build, Cursor CLI, Kimi Code or your own: add this command to
  their MCP servers. It finds its block from the terminal it runs in; an agent that hands a server only the variables
  it names needs TEMPLUM_BUS and TEMPLUM_TERMINAL"; the switch off: `initialize`'s "Connections are off in Templum's
  settings: Templum's tools won't work here.", a tool call's "Connections are off: turn on Agents in Templum (Settings
  ▸ Connections) to use Templum's tools."; Gemini CLI's notice "Gemini CLI has no Templum tools or status dot" / "Click
  to install Templum's extension: every Gemini CLI you open after gets them."
- **No new crate dependency; no `unsafe`** (so no `std::env::set_var` in tests: pass lookups in); only
  `crates/templum` uses gpui-kit; bash code runs on bash 3.2 (no `mapfile`, no `local -n`).
- **Tests that need an agent installed** skip cleanly: the shells' tests use fakes; the one test that runs each
  installed agent's `--help` (Task 1) is `#[ignore]` (Copilot's and OpenCode's mise shims rewrite mise's global config
  at every run): run it with `cargo test -p templum-terminal -- --ignored installed_agents` after a task that changes
  an agent's words. The JS tests skip without `node` (GitHub's ubuntu and macOS runners have it: CI runs them; they
  are `#[cfg(unix)]` for their fake program).
- **Each commit** passes `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`, on `development`, titled as CLAUDE.md says (`feat(connections): …` for what a person sees). A task
  that changes behavior a spec describes updates those lines in the same commit (flip "from card 44" and "Planned" to
  the present where it lands), and deletes the minor it fixes from `docs/minors.md`. Nothing paints: no bench.
- **Spikes** (Tasks 5, 9, 12) are checks by hand on this machine; what they find goes into [Spike
  results](#spike-results) and corrects the spec's lines in the task's commit. When the agent can't run (no sign-in,
  no model), the task goes on with the spec's documented assumptions, the spec says "not checked by hand", and a minor
  is logged.

## Review Focus

1. **A data folder with a space** (macOS's `~/Library/Application Support/templum`): every word is one argument in
   every shell, typed or from a variable; the JSON escapes it; OpenCode's `file://` URL writes it `%20`. Tasks 6, 7,
   9 and 11 put one in their paths.
2. **A Templum started from a Templum terminal** (`cargo run` in one): its terminals' new variables, and
   `TEMPLUM_HOOKED`, are empty, never the first one's; else every OpenCode or Pi there would stay quiet. Tasks 7 and
   9 pin it.
3. **The person's own `OPENCODE_CONFIG_CONTENT` or `QWEN_CODE_SYSTEM_SETTINGS_PATH`**, in Templum's environment or the
   shell's: theirs is kept, opened or typed. Tasks 9, 10 and 11 pin it.
4. **Templum gone, or never installed** while an OpenCode or a Pi runs its plugin: an event never throws in the agent
   nor holds it more than the hook's limit (`spawnSync` with a timeout, its error ignored). Tasks 9 and 12 pin it with
   a program that doesn't exist.
5. **A nested agent or a subagent:** an `opencode run` or `pi -p` started under one that already reports, and an
   OpenCode subagent's session (`parentID`), say nothing for the block. Tasks 9 and 12 pin it.

---

### Task 1: `spawn_agent` takes a model and an effort for every agent that does

**Files:**
- Modify: `crates/templum-terminal/src/agent.rs` (`LAUNCH`, its doc comment)
- Modify: `crates/templum-mcp/src/tools.rs` (`spawn_agent`'s `model` and `effort` descriptions)
- Modify: `docs/specs/agents.md` (A model and an effort: present tense), `docs/specs/agent-parity.md` (matrix cells),
  `docs/minors.md` (delete "`spawn_agent` refuses a model or an effort for agents that take one")
- Test: `crates/templum-terminal/src/agent.rs` (tests)

**Interfaces:**
- Produces: `Agent::launch_args` (unchanged signature) for the rows of [agents.md](../specs/agents.md#a-model-and-an-effort).

- [ ] **Step 1: Write the failing tests** in `agent.rs`'s tests, extending `a_model_and_an_effort_become_each_agents_flags`
  (and replacing its assertion that Copilot is refused):

```rust
let ok = |s: &str| Ok(s.to_string());
assert_eq!(agent("copilot").launch_args(Some("gpt-5.4"), Some("none")), ok(" --model gpt-5.4 --reasoning-effort none"));
assert_eq!(agent("grok").launch_args(Some("grok-5"), Some("max")), ok(" --model grok-5 --reasoning-effort max"));
assert_eq!(agent("pi").launch_args(Some("anthropic/claude-opus-5-5"), Some("off")), ok(" --model anthropic/claude-opus-5-5 --thinking off"));
assert_eq!(agent("kiro").launch_args(Some("claude-sonnet-5-5"), Some("xhigh")), ok(" --model claude-sonnet-5-5 --effort xhigh"));
assert_eq!(agent("cline").launch_args(Some("gpt-5.4"), Some("none")), ok(" --model gpt-5.4 --thinking none"));
assert_eq!(agent("aider").launch_args(Some("o4"), Some("medium")), ok(" --model o4 --reasoning-effort medium"));
for id in ["opencode", "kilo", "cursor", "qwen", "kimi", "droid", "auggie"] {
    assert_eq!(agent(id).launch_args(Some("p/m-1"), None), ok(" --model p/m-1"), "{id}");
}
assert_eq!(agent("opencode").launch_args(None, Some("high")).unwrap_err(), "OpenCode takes no effort from Templum.");
assert_eq!(agent("aider").launch_args(None, Some("xhigh")).unwrap_err(), "Aider's effort is low, medium or high.");
assert_eq!(agent("amp").launch_args(Some("x"), None).unwrap_err(), "Templum doesn't give Amp a model or an effort.");
assert!(agent("crush").launch_args(Some("x"), None).is_err() && agent("goose").launch_args(Some("x"), None).is_err());
```

  And a new `#[ignore = "runs each installed agent's --help"]` test `installed_agents_take_the_flags_templum_types`:
  for every `LAUNCH` row whose program `find_program` finds on `PATH`, run its command with `--help` (Kiro CLI:
  `kiro-cli chat --help`; stdin null, 20 s limit), and assert the output names the model flag's word (`--model`, `-m`)
  and the effort flag's (`--reasoning-effort`, `--thinking`, `--effort`; not Codex's `-c`); print `skipped: <program>`
  for one not installed.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p templum-terminal a_model_and_an_effort`
Expected: FAIL (Copilot: "Templum doesn't give GitHub Copilot CLI a model or an effort.")

- [ ] **Step 3: Add the `LAUNCH` rows** in the table's order (Copilot CLI, Cursor CLI, OpenCode, Kiro CLI, Qwen Code,
  Grok Build, Kimi Code, Aider, Droid, Auggie, Kilo Code, Cline, Pi), each from agents.md's table: `model` the flag
  and a space (`--model ` for every new row), `effort` `None` with empty `efforts` where the table says "—". Update the doc comment's "Checked on"
  sentence (2026-10-18, the installed `--help`s, the references of the others). `spawn_agent`'s descriptions:

  - `model`: "Its model, as its own --model or -m takes it (provider/model for OpenCode, Kilo Code and Pi). Letters,
    digits, . - _ : and /."
  - `effort`: "Its reasoning effort. claude: low, medium, high, xhigh, max. codex: low, medium, high, xhigh, max,
    ultra. copilot and grok: none, minimal, low, medium, high, xhigh, max. pi: off, minimal, low, medium, high, xhigh,
    max. kiro: low, medium, high, xhigh, max. cline: none, low, medium, high, xhigh. aider: low, medium, high. The
    others take none."

- [ ] **Step 4: Run the tests** (`cargo test -p templum-terminal`, then `cargo test -p templum-terminal -- --ignored
  installed_agents`): PASS, the second naming the agents installed here (Copilot CLI, Cursor CLI, OpenCode, Grok
  Build, Pi) and skipping the others.

- [ ] **Step 5: Commit** — `feat(connections): spawn_agent starts Copilot CLI, Grok Build, Pi and ten more on a model and an effort`

---

### Task 2: "Agents in Templum" off reaches every agent; Settings' words

**Files:**
- Modify: `crates/templum/src/workspace/bus.rs` (`serve`, `instructions`)
- Modify: `crates/templum/src/workspace/preferences.rs` (`apply_config`)
- Modify: `crates/templum/src/settings/connections.rs` (the switch's and Other agents' descriptions)
- Modify: `crates/templum/src/config.rs` (the `[connections]` comment of the template)
- Modify: `docs/specs/connections.md` (the switch in Agents typed in a terminal, Config, Settings), `docs/minors.md`
  (delete "Gemini CLI stays connected with "Agents in Templum" off" and "Settings ▸ Connections tells the person to
  give Pi Templum's MCP command")
- Test: `crates/templum/src/workspace/tests/bus.rs`

**Interfaces:**
- Produces: `pub(super) const CONNECTIONS_OFF: &str` (the tool call's text) in `workspace/bus.rs`.

- [ ] **Step 1: Write the failing test** `the_switch_off_cuts_every_agent_but_not_the_command_line` in
  `tests/bus.rs`: with a data folder and a terminal, set `ws.config.connections.enabled = false` through
  `apply_config`; a `client: "mcp"` `initialize` (an `ask` with the client as a parameter: add `ask_as`) has
  `instructions` == "Connections are off in Templum's settings: Templum's tools won't work here."; its `tools/call`
  of `whoami` is `isError` with "Connections are off: turn on Agents in Templum (Settings ▸ Connections) to use
  Templum's tools."; a `Stopped` `event` leaves the pane's `state` as it was; a pane that `speaks` speaks no more;
  the same `whoami` as `client: "cli"` answers its id. On again (`apply_config`), the `mcp` call answers.

- [ ] **Step 2: Run it** — `cargo test -p templum the_switch_off` — FAIL (the `mcp` call answers).

- [ ] **Step 3: Implement** in `serve`: when `!self.config.connections.enabled` and `hello.client != "cli"`,
  `initialize` answers with the off sentence as its instructions, `tools/call` with `Reply::err(CONNECTIONS_OFF)`
  (before `allowed`), and a `templum/event` notification is dropped. In `apply_config`, when `connections.enabled`
  goes from on to off, every pane of every workspace gets `speaks = false`. Calls already waiting are left alone
  (no sweep of `waits.rs`): they end through the screen, the person or their timeout. The copy of the Global Constraints in
  `connections.rs` and the `config.rs` template (its own test of the template's text, if any, follows).

- [ ] **Step 4: Run** `cargo test -p templum` — PASS. Check by hand: Settings ▸ Connections shows both new
  descriptions (`cargo run -p templum -- --dev`).

- [ ] **Step 5: Commit** — `fix(connections): turning off Agents in Templum disconnects Gemini CLI and every agent already running`

---

### Task 3: A Gemini CLI opened without Templum's extension says so, once

**Files:**
- Modify: `crates/templum/src/workspace/blocks.rs` (`start_agent`), `crates/templum/src/workspace/mod.rs`
  (`Workspace::gemini_told: bool`, false at start)
- Modify: `crates/templum/src/settings/connections.rs` (`INSTALL` becomes `pub(crate)`)
- Modify: `docs/specs/connections.md` (Gemini CLI without its extension: present tense)
- Test: `crates/templum/src/workspace/tests/usage.rs`

**Interfaces:**
- Consumes: `Workspace::setup_gemini(install: bool, cx)`, `prompt::confirm`, `templum_usage::gemini::installed`.
- Produces: `pub(super) fn tell_gemini(&mut self, entry: Option<&AgentEntry>, window: &mut Window, cx: &mut Context<Self>)`.

- [ ] **Step 1: Write the failing test** `a_gemini_cli_without_the_extension_is_told_once`: with a data folder, open
  Gemini CLI twice through `start_agent` (its entry from `ws.agents`): `notification_count` grows by 1 when
  `templum_usage::gemini::installed(<home>/.gemini)` is false on this machine, by 0 when it is true; opening Claude
  Code adds none; with `connections.enabled` false, none; a restored Gemini CLI (`add_saved` of a terminal item with
  `agent: "gemini"`) none.

- [ ] **Step 2: Run it** — `cargo test -p templum a_gemini_cli_without` — FAIL (no notification).

- [ ] **Step 3: Implement `tell_gemini`**, called by `start_agent` after the agent is typed: only for
  `builtin == Some(Agent::GEMINI)` (add the constant beside `CLAUDE` and `CODEX`), connections on (`gives()`), a data
  folder, the extension not installed, and `!gemini_told`; a warning notification with the Global Constraints' title
  and message, delivered as `delivery(window)`, whose click asks `INSTALL`'s question and runs `setup_gemini(true)`.

- [ ] **Step 4: Run** `cargo test -p templum` — PASS. Check by hand: open Gemini CLI from the menu without the
  extension, click the notice, answer Install: Settings ▸ Connections shows it installed.

- [ ] **Step 5: Commit** — `feat(connections): a Gemini CLI opened without Templum's extension offers to install it`

---

### Task 4: Copilot CLI, Cursor CLI and Crush come back on their own session

**Files:**
- Modify: `crates/templum-terminal/src/agent.rs` (`SPECS`: Copilot's `resume_id` `Some("copilot --session-id {id}")`
  and `new_id` `Some("--session-id {id}")`; Cursor's `resume` `Some("cursor-agent --continue")`; Crush's `resume`
  `Some("crush --continue")`)
- Modify: `docs/specs/agents.md` (the table and One session per block: present tense), `docs/specs/persistence.md`
  (its agents' list, if it names them), `docs/specs/agent-parity.md` (Restore today cells)
- Test: `crates/templum-terminal/src/agent.rs` (tests), `crates/templum/src/workspace/tests/agents.rs`

- [ ] **Step 1: Write the failing tests:** in `only_a_resume_limited_to_the_folder_is_used`, replace the Copilot line
  with `by("copilot").resume_command() == "copilot"` (its `--continue` isn't kept to the folder), and add
  `by("copilot").resume_session(UUID) == Some("copilot --session-id <UUID>")`, `by("cursor").resume_command() ==
  "cursor-agent --continue"`, `by("crush").resume_command() == "crush --continue"`; in
  `a_session_is_read_from_the_flag_that_started_or_resumed_it` add `("copilot", &["copilot", "--session-id", id],
  Some(id))` and `("copilot", &["copilot", &format!("--session-id={id}")], Some(id))`, keeping `--resume` None; in
  `tests/agents.rs`, Copilot's `start_command(id)` is `copilot --session-id <id>`.

- [ ] **Step 2: Run them** — `cargo test -p templum-terminal only_a_resume a_session_is_read` — FAIL.

- [ ] **Step 3: Change the three `SPECS` rows.** Before committing, check by hand that `crush --help` and
  `cursor-agent --help` still list `--continue` (as on 2026-10-18).

- [ ] **Step 4: Run** `cargo test --workspace` — PASS.

- [ ] **Step 5: Commit** — `feat(agents): Copilot CLI, Cursor CLI and Crush come back on their own conversation after a restart`

---

### Task 5: `templum hook copilot`

**Files:**
- Modify: `crates/templum-mcp/src/hooks.rs` (`copilot`, `copilot_answer`)
- Modify: `crates/templum-mcp/src/cli.rs` (`hook_event`'s `copilot` arm; `fn tail(path: &str, max: u64) -> Option<String>`)
- Modify: `docs/specs/connections.md` (Copilot's hook table, from the spike; the `templum` command line table)
- Test: `crates/templum-mcp/src/hooks.rs`, `crates/templum-mcp/src/cli.rs`

**Interfaces:**
- Produces: `pub fn copilot(event: &str, input: &Value) -> Option<AgentEvent>`; `pub fn copilot_answer(transcript:
  &str) -> Option<String>`; the command `templum hook copilot <event>` (JSON on stdin), the event named as Copilot
  names its hook (`agentStop`).

- [ ] **Step 1: Spike S1 (by hand, if `copilot` is signed in here).** A probe plugin in the scratchpad
  (`plugin.json` `{"name":"probe"}`, `hooks.json` with every hook of the spec's table plus `permissionRequest`, each
  `bash`: `cat > <dir>/<event>-$(date +%s%N).json; env | grep TEMPLUM_ > <dir>/<event>.env`). Run `copilot
  --plugin-dir=<probe> -p "say hi"` in a Templum terminal, then an interactive `copilot --plugin-dir=<probe>`: a
  prompt, a command that asks permission (Allow, then another refused), Esc during a turn, `/exit`. Record in [Spike
  results](#spike-results): whether a hook sees `TEMPLUM_TERMINAL` (if not, Task 6's hook entries get the server's
  `env`), the payload keys (`notification_type` or `notificationType`, `message`, `transcriptPath`, `recoverable`),
  whether `userPromptSubmitted` comes at the Enter for a prompt typed while it works (then `at_enter: true`), the last
  `assistant.message` of `events.jsonl` being the turn's answer, and whether `--plugin-dir` asks anything at start.
  **What a finding changes:** hooks without `TEMPLUM_TERMINAL` → Task 6's hook entries get the server's `env` (Copilot
  expands `${VAR}` there); if that fails too, Task 6 ships the server, the instructions and the session without the
  plugin, and a minor says so; other payload keys → the parser and its tests follow them; a trust question for the
  plugin → asked once, the hooks' words never change (they name `<data>/bin/templum`): say so in the spec. Without a
  sign-in, go on with the SDK's types and log "Copilot CLI's hooks not checked by hand" in minors.

- [ ] **Step 2: Write the failing tests:** `copilots_hooks_become_events` in `hooks.rs`, one assertion per row of the
  spec's table (camelCase payloads as the SDK's `types.d.ts` gives them, corrected by S1): `sessionStart` → `Started`;
  `userPromptSubmitted` → `Submitted { at_enter: false }`; `postToolUse` and `postToolUseFailure` → `Working`;
  `notification` with `permission_prompt` and `"message": "Allow shell?"` → `NeedsYou { message: Some("Allow shell?") }`,
  with `elicitation_dialog` → `NeedsYou` with its message, with `idle_prompt` → None; `agentStop` with
  `transcriptPath` → `Stopped { message: None, transcript: Some(path) }`; `errorOccurred` with `"recoverable": false`
  → `Stopped { message: None, transcript: None }`, with `true` → None; `sessionEnd` → `Ended`; `preToolUse` and an
  unknown event → None. `copilot_answer` of three JSON lines (`assistant.message` "first", a `tool.execution_complete`,
  `assistant.message` "Done.") is `Some("Done.")`, of none or of a broken last line the last whole one, of no
  `assistant.message` None. In `cli.rs`, `a_hook_sends_its_event_prints_nothing_and_never_fails` gains: `hook copilot
  agentStop` with a temporary `events.jsonl` sends `Stopped` with message "Done." and that transcript; `hook copilot`
  without its event sends nothing and exits 0.

- [ ] **Step 3: Run them** — `cargo test -p templum-mcp copilot` — FAIL (`copilot` not defined).

- [ ] **Step 4: Implement** `copilot`, `copilot_answer` (each line parsed alone; `type == "assistant.message"`,
  `data.content` a non-empty string) and the `cli.rs` arm: `"copilot" => hooks::copilot(args.get(1)?, &on_stdin()?)`,
  then, for `Stopped` with a transcript and no message, the message from `copilot_answer` of the transcript's last
  256 KB (`tail`, from the first whole line).

- [ ] **Step 5: Run** `cargo test -p templum-mcp` — PASS.

- [ ] **Step 6: Commit** — `feat(connections): Templum hears GitHub Copilot CLI's hooks`

---

### Task 6: The Copilot CLI Templum opens gets its server, its hooks and its instructions

**Files:**
- Create: `crates/templum-usage/src/copilot.rs`
- Modify: `crates/templum-usage/src/lib.rs` (`pub mod copilot;`; `hook_command` moved here from `gemini.rs`, with the
  words after `hook`; `bin`, the path `gemini::extension` builds today), `crates/templum-usage/src/gemini.rs` (uses
  both)
- Modify: `crates/templum/src/workspace/usage.rs` (`typed`'s Copilot arm; `fn typed_words(words: &[String], windows:
  bool) -> Option<String>`)
- Modify: `docs/specs/connections.md` (GitHub Copilot CLI: present tense), `docs/ARCHITECTURE.md` (templum-usage's
  row), `docs/specs/agent-parity.md` (Copilot's cells)
- Test: `crates/templum-usage/src/copilot.rs`, `crates/templum-usage/src/gemini.rs`, `crates/templum/src/workspace/tests/usage.rs`

**Interfaces:**
- Produces: `pub fn templum_usage::bin(data: &Path, windows: bool) -> PathBuf` (`<data>/bin/templum`, `templum.exe`
  when `windows`); `pub fn templum_usage::hook_command(bin: &str, windows: bool, words: &str) -> Option<String>`
  (`'<bin>' hook <words>` / `& '<bin>' hook <words>`; None for an empty `bin` or one with `'`, `\n`, `\r`);
  `pub fn templum_usage::copilot::args(data: &Path, windows: bool) -> Option<Vec<String>>` (writes the three files,
  returns the three words of the Global Constraints, Windows' `\` as `/`).

- [ ] **Step 1: Write the failing tests:** `copilot_gets_the_server_and_a_plugin_of_hooks` in `copilot.rs`, with a
  data folder named `Templum Data`: the words exactly; `mcp.json` equals the spec's JSON (`bin(data)`, `["mcp"]`,
  `env` `${TEMPLUM_BUS}` and `${TEMPLUM_TERMINAL}`, `"tools": ["*"]`, `604800000`); `plugin.json` is
  `{"name":"templum"}`; `hooks.json` has `"version": 1` and, for each of `sessionStart`, `userPromptSubmitted`,
  `postToolUse`, `postToolUseFailure`, `agentStop`, `notification`, `errorOccurred`, `sessionEnd`, one entry
  `{"type":"command","bash":"'<bin>' hook copilot <event>","powershell":"& '<bin>' hook copilot <event>","timeoutSec":5}`
  (plus the server's `env` if S1 found hooks without it); a second call leaves the files' modified times; a data
  folder with `'` gives None. `gemini.rs`'s `hook_command` tests move to `lib.rs` with `"gemini"` as the words. In
  `tests/usage.rs`, `the_agents_templum_opens_get_its_server_hooks_and_status_line` gains: `ws.typed(copilot,
  "copilot --session-id 0d5e")` is `copilot --session-id 0d5e --allow-all-mcp-server-instructions
  --additional-mcp-config=@<mcp> --plugin-dir=<plugin>` (each word through `typed_path`), unchanged with
  `[connections]` off, under WSL and without `<data>/bin` on the terminal's `PATH` (`bus_env` without `PATH`, as the
  Codex test does).

- [ ] **Step 2: Run them** — `cargo test -p templum-usage copilot` — FAIL.

- [ ] **Step 3: Implement** `copilot::args` (beside `claude::mcp_file`, the same checks), `bin`, `hook_command`, and
  the `typed` arm: `builtin == Some(Agent::COPILOT)` (a new constant), `gives.connected` and `gives.on_path` →
  `typed_words` of `copilot::args`, nothing when it is None or a word can't be typed.

- [ ] **Step 4: Run** `cargo test --workspace` — PASS.

- [ ] **Step 5: Commit** — `feat(connections): the Copilot CLI Templum opens gets its MCP server, its hooks and its instructions`

---

### Task 7: A `copilot` typed in bash or zsh gets them

**Files:**
- Modify: `crates/templum/src/workspace/usage.rs` (`agent_env`: `TEMPLUM_COPILOT_ARGS`)
- Modify: `crates/templum-terminal/src/integration/templum.bash`, `crates/templum-terminal/src/integration/zsh/.zshenv`
- Modify: `crates/templum-terminal/src/integration.rs` (tests: the blanked keys, `fake_agents` with `copilot`,
  `COPILOT_WORDS`, `fn typed_copilot(term: &Terminal, home: &Home)`)
- Modify: `docs/specs/connections.md` (Typed by hand: the shells)
- Test: `crates/templum-terminal/src/integration.rs`, `crates/templum/src/workspace/tests/usage.rs`, `tests/bus.rs`

**Interfaces:**
- Produces: the variable `TEMPLUM_COPILOT_ARGS`; the marker rule every new function uses: no words when one of the
  person's words is the variable's last line (bash and zsh: `"${w##*$'\n'}"`).
- Consumes: `copilot::args` (Task 6).

- [ ] **Step 1: Write the failing tests:** `new_terminals_carry_what_templum_types_after_claude_and_codex` gains
  `TEMPLUM_COPILOT_ARGS` == the three words joined by `\n` (empty with connections off, under WSL, without a data
  folder; set under cmd and Windows PowerShell); `without_a_bus_terminals_name_none` prints
  `${TEMPLUM_COPILOT_ARGS-unset}` as empty. `typed_copilot` (called from the bash and zsh tests, with
  `COPILOT_WORDS` = `--allow-all-mcp-server-instructions\n--additional-mcp-config=@/tmp/Templum Data/copilot/mcp.json\n--plugin-dir=/tmp/Templum Data/copilot/plugin`):
  `copilot` gets the words; `copilot -p "hi there"` the words then `-p`, `hi there`; `copilot --plugin-dir=/mine` the
  words then it; `copilot '--plugin-dir=/tmp/Templum Data/copilot/plugin'` only that word (nothing twice); after
  `unset TEMPLUM_COPILOT_ARGS`, `copilot x` only `x`; exit code 3 reaches the prompt's mark. And the subcommand rule:
  `copilot --model gpt-5.4` the words then `--model`, `gpt-5.4`; `copilot plugin list` and `copilot app deploy` only
  their own words.

- [ ] **Step 2: Run them** — `cargo test -p templum-terminal shells` and `cargo test -p templum new_terminals_carry`
  — FAIL.

- [ ] **Step 3: Implement** the variable (beside `TEMPLUM_CODEX_ARGS`, same `gives` rules, `connected` only) and, in
  both scripts, `function copilot` as `claude`'s is (only when `TEMPLUM_COPILOT_ARGS` is set at start and there is no
  function of that name), clearing its words on the marker or when `$1` is set and doesn't start with `-`, then
  `__templum_agent copilot "$w" "$@"`.

- [ ] **Step 4: Run** `cargo test --workspace` — PASS (a shell not installed prints `skipped:`).

- [ ] **Step 5: Commit** — `feat(connections): a copilot typed in a bash or zsh terminal gets Templum's server and hooks`

---

### Task 8: A `copilot` typed in fish or PowerShell gets them

**Files:**
- Modify: `crates/templum-terminal/src/integration/templum.fish` (a `__templum_agent` helper: the not-found message,
  the words, `command`; `claude`, `codex` and `copilot` call it), `crates/templum-terminal/src/integration/templum.ps1`
  (`copilot` beside `claude`, through `__TemplumAgent`)
- Test: `crates/templum-terminal/src/integration.rs` (the fish and PowerShell tests call `typed_copilot`)

- [ ] **Step 1: Write the failing tests:** the fish and PowerShell tests call `typed_copilot` as bash's do; fish's
  existing `claude`/`codex` assertions (not installed, the person's function, the alias) stay green through the
  helper.

- [ ] **Step 2: Run them** — `cargo test -p templum-terminal shells` — FAIL in fish and PowerShell.

- [ ] **Step 3: Implement:** fish `function __templum_agent --argument-names name words` (its not-found branch as
  today's, then `command $name (string split --no-empty \n -- $words) $argv[3..]`), the marker
  `(string split \n -- $w)[-1]`; PowerShell's marker `($words -split "`n")[-1]`, piped input passed on as `claude`'s.

- [ ] **Step 4: Run** `cargo test --workspace` — PASS.

- [ ] **Step 5: Commit** — `feat(connections): a copilot typed in a fish or PowerShell terminal gets Templum's server and hooks`

---

### Task 9: The OpenCode and Kilo Code Templum opens get its server and a plugin

**Files:**
- Create: `crates/templum-usage/src/opencode.rs`, `crates/templum-usage/src/opencode.js` (the plugin, with
  `__TEMPLUM_PROGRAM__` where the program goes, as a JSON string)
- Modify: `crates/templum-usage/src/lib.rs` (`pub mod opencode;`)
- Modify: `crates/templum-mcp/src/hooks.rs` (`opencode`), `crates/templum-mcp/src/cli.rs` (the `opencode` arm)
- Modify: `crates/templum/src/workspace/usage.rs` (`agent_vars`; `agent_env`: `TEMPLUM_HOOKED` empty),
  `crates/templum/src/workspace/blocks.rs` (`spawn_block_with`; `start_agent` passes `agent_vars`),
  `crates/templum/src/workspace/persist.rs` (`add_saved` passes them for an agent it resumes)
- Modify: `docs/specs/connections.md` (OpenCode and Kilo Code: present tense, the spike's corrections),
  `docs/ARCHITECTURE.md`, `docs/specs/agent-parity.md`
- Test: `crates/templum-usage/src/opencode.rs`, `crates/templum-mcp/src/hooks.rs`, `crates/templum/src/workspace/tests/usage.rs`, `tests/bus.rs`

**Interfaces:**
- Produces: `pub fn templum_usage::opencode::config(data: &Path, windows: bool) -> Option<String>` (writes
  the plugin, returns the one-line JSON of the spec); `pub fn hooks::opencode(input: &Value) -> Option<AgentEvent>`
  for the plugin's line `{"type": "<event type, or chat.message>", "properties": {…}, "answer": "<text>"|null}`;
  `pub(super) fn agent_vars(&self, agent: Option<&AgentEntry>, set: &dyn Fn(&str) -> bool) -> Vec<(String, String)>`
  (`set(name)`: the person's environment already has a non-empty one; callers pass
  `&|k| std::env::var_os(k).is_some_and(|v| !v.is_empty())`); `pub(super) fn spawn_block_with(&mut self, rect: Rect,
  folder: Option<PathBuf>, vars: Vec<(String, String)>, window, cx) -> Option<BlockId>` (`spawn_block` calls it with
  none).

- [ ] **Step 1: Spike S2 (by hand, if OpenCode has a model it can use here).** The plugin of Step 4 written by hand
  to the scratchpad, with a fake `templum` (`cat >> <dir>/events.jsonl`); `OPENCODE_CONFIG_CONTENT='{"plugin":
  ["file://<it>"]}' opencode run "say hi"`, then the TUI: a prompt, a permission (`bash`), its question tool if a
  prompt calls it, a subagent (`@general`), Esc, an `opencode run` from its own shell tool. Record: the lines in
  order, that `chat.message` comes once per prompt, `session.error`'s name, that the subagent's events carry its
  `parentID` session, the answer read with `client.session.messages({ path: { id } })`, the nested run staying quiet.

- [ ] **Step 2: Write the failing tests:**
  - `opencode.rs`: `config` with a data folder `Templum Data` is exactly
    `{"mcp":{"templum":{"type":"local","command":["<bin>","mcp"],"timeout":604800000}},"plugin":["file://<data>/opencode/templum.js"]}`
    (the space as `%20`; Windows `file:///C:/…`); the plugin file holds `bin(data)` as a JSON string and no
    `__TEMPLUM_PROGRAM__`; a `'` in the data folder gives None.
  - `opencode.rs`, `#[cfg(unix)]`, skipped without `node`: `the_plugin_reports_the_main_sessions_turns` copies the
    plugin to `templum.mjs` with a fake program (a script appending stdin to a file), imports it with a fake `client`
    (`session.messages` resolving `[{info:{role:"assistant"},parts:[{type:"text",text:"Done."}]}]`) and feeds events:
    `chat.message`, `session.status` busy, `session.idle` reach the file as three lines, the last with `"answer":
    "Done."`; a session announced with a `parentID` says nothing; with `TEMPLUM_HOOKED` another pid nothing at all;
    with the program missing, no throw.
  - `hooks.rs` `opencodes_events_become_events`: `chat.message` → `Submitted { at_enter: false }`; `session.status`
    busy → `Working`, idle → None; `session.idle` with an answer → `Stopped { message: Some(answer), transcript: None }`;
    `session.error` → `Stopped { message: None, transcript: None }`; `permission.asked` with `"permission": "bash"` →
    `NeedsYou { message: Some("Allow bash?") }`; `question.asked` → `NeedsYou { message: Some("OpenCode asks you a question") }`;
    `permission.replied`, `question.replied`, `question.rejected` → `Working`; others None.
  - `tests/usage.rs` `blocks_templum_opens_get_their_agents_variable`: `agent_vars` of OpenCode is
    `[("OPENCODE_CONFIG_CONTENT", config)]`, of Kilo Code `[("KILO_CONFIG_CONTENT", config)]`, of Claude Code none,
    none when `set` says the person has one, none with connections off or without `<data>/bin` on `PATH`;
    `agent_env` has `("TEMPLUM_HOOKED", "")`, also without a data folder.
  - `tests/bus.rs`, `#[cfg(unix)]`: a block from `start_agent` for OpenCode prints its `OPENCODE_CONFIG_CONTENT` (a
    shell command as `terminals_get_the_bus_and_this_templum_first_on_their_path` does), and a plain terminal none.

- [ ] **Step 3: Run them** — `cargo test -p templum-usage opencode; cargo test -p templum-mcp opencode` — FAIL.

- [ ] **Step 4: Implement the plugin** (`opencode.js`, ESM, no import but `node:child_process`): `export const
  Templum = async ({ client }) => ({ event, "chat.message" })`; at load, `quiet = TEMPLUM_HOOKED` is set and isn't
  `String(process.pid)`, else it sets it; children's session ids kept from `session.created`/`session.updated` with
  `info.parentID`; `send(line)` = `spawnSync(PROGRAM, ["hook", "opencode"], { input: JSON.stringify(line), stdio:
  ["pipe", "ignore", "ignore"], timeout: 3000 })` in `try`; `session.idle`'s answer the last assistant message's text
  parts, joined; the event hook's body in `try`. Then `config`, `hooks::opencode`, the `cli.rs` arm (`"opencode" =>
  hooks::opencode(&on_stdin()?)`, Kilo Code's plugin sends the same), `agent_vars`, `spawn_block_with`, and the
  callers.

- [ ] **Step 5: Run** `cargo test --workspace` — PASS.

- [ ] **Step 6: Commit** — `feat(connections): the OpenCode and Kilo Code Templum opens get its MCP server and tell their turns`

---

### Task 10: An `opencode` or `kilo` typed in a terminal gets them

**Files:**
- Modify: `crates/templum/src/workspace/usage.rs` (`agent_env`: `TEMPLUM_OPENCODE_CONFIG`, `TEMPLUM_KILO_CONFIG`)
- Modify: the four integration scripts (the helper takes a variable's name and value: bash and zsh `__templum_agent
  name words var value "$@"`, fish `--argument-names name words var value`, PowerShell `__TemplumAgent($name, $words,
  $var, $value, $given)`; `claude`, `codex`, `copilot` pass empty ones), `crates/templum-terminal/src/integration.rs`
  (fakes `opencode` and `kilo` that also write `$OPENCODE_CONFIG_CONTENT` or `$KILO_CONFIG_CONTENT` to
  `<name>.env`; `fn typed_with_variable(term, home, name, var)`)
- Modify: `docs/specs/connections.md`
- Test: `crates/templum-terminal/src/integration.rs`, `crates/templum/src/workspace/tests/usage.rs`

- [ ] **Step 1: Write the failing tests:** `new_terminals_carry…` gains both variables (the `config` JSON; empty
  when connections are off). In each shell's test, `typed_with_variable` for `opencode`/`OPENCODE_CONFIG_CONTENT`
  and `kilo`/`KILO_CONFIG_CONTENT`: the fake sees the variable's JSON and the person's words unchanged; with
  `OPENCODE_CONFIG_CONTENT=mine` in the shell's environment it sees `mine`; after it ran, the shell's own
  `OPENCODE_CONFIG_CONTENT` is still unset (`printf '%s' "${OPENCODE_CONFIG_CONTENT-unset}"` in bash and zsh, fish's
  `set -q`, PowerShell's `Test-Path Env:\OPENCODE_CONFIG_CONTENT`); its exit code 3 reaches the mark.

- [ ] **Step 2: Run them** — `cargo test -p templum-terminal shells` — FAIL.

- [ ] **Step 3: Implement:** the helper sets the variable for that command only when the value is non-empty and the
  environment has none (bash and zsh `builtin local -x "$var=$value"`, fish `set -lx $var $value`, PowerShell sets
  `$env:` and puts it back in `finally`); `opencode` and `kilo` functions, each only while its variable is set at
  start and there is no function of that name.

- [ ] **Step 4: Run** `cargo test --workspace` — PASS.

- [ ] **Step 5: Commit** — `feat(connections): an opencode or kilo typed in a terminal gets Templum's server and plugin`

---

### Task 11: Qwen Code, opened or typed

**Files:**
- Create: `crates/templum-usage/src/qwen.rs`
- Modify: `crates/templum-usage/src/lib.rs`, `crates/templum-mcp/src/hooks.rs` (`qwen`), `crates/templum-mcp/src/cli.rs`
- Modify: `crates/templum/src/workspace/usage.rs` (`typed`'s Qwen arm; `agent_vars`: `QWEN_CODE_SYSTEM_SETTINGS_PATH`;
  `agent_env`: `TEMPLUM_QWEN_ARGS`, `TEMPLUM_QWEN_SETTINGS`)
- Modify: the four integration scripts (`qwen`: words and variable), `crates/templum-terminal/src/integration.rs`
- Modify: `docs/specs/connections.md` (Qwen Code: present tense, "not checked by hand: not installed"),
  `docs/ARCHITECTURE.md`, `docs/specs/agent-parity.md`
- Test: `qwen.rs`, `hooks.rs`, `tests/usage.rs`, `integration.rs`

**Interfaces:**
- Produces: `pub fn qwen::args(data: &Path, windows: bool) -> Option<Vec<String>>` (writes `mcp.json`, returns
  `--mcp-config=<path>`); `pub fn qwen::settings(data: &Path, windows: bool, system: &Path) ->
  Option<PathBuf>` (writes `settings.json`; None when `system` exists); `pub fn qwen::system_file() -> PathBuf` (the
  spec's three paths by OS); `pub fn hooks::qwen(input: &Value) -> Option<AgentEvent>`.

- [ ] **Step 1: Write the failing tests:** `qwen.rs`: `mcp.json` is the spec's JSON; `settings.json` is `{"hooks":
  {…}}` with `SessionStart`, `UserPromptSubmit`, `PermissionRequest`, `PostToolUse`, `Stop`, `StopFailure`,
  `Notification`, `SessionEnd`, each `[{"hooks":[{"type":"command","command":"<hook_command(bin, windows, "qwen")>","timeout":5}]}]`;
  `settings` with a `system` file that exists is None; a `'` gives None. `hooks.rs` `qwens_hooks_become_events`:
  as Claude Code's payloads, `UserPromptSubmit` → `Submitted { at_enter: false }`, `PermissionRequest` with
  `tool_name` `run_shell_command` → `NeedsYou { message: Some("Allow run_shell_command?") }`, `Stop` →
  `Stopped` with `last_assistant_message` and `transcript_path`, `Notification` `idle_prompt` → `Stopped` without an
  answer, `SessionStart` `compact` → None. `tests/usage.rs`: `typed(qwen, "qwen --resume 0d5e")` ends with
  ` --mcp-config=<path>`; `agent_vars` of Qwen Code is the settings path; `agent_env`'s two variables. The shells:
  `qwen` gets the word first and the variable; `qwen --mcp-config=mine.json` and `qwen --mcp-config mine.json` get
  no word (Qwen Code takes one) but still the variable; with `QWEN_CODE_SYSTEM_SETTINGS_PATH=mine` in the shell's
  environment it sees `mine`; a data folder named `Templum Data` stays one word.

- [ ] **Step 2: Run them** — FAIL.

- [ ] **Step 3: Implement** (the hook command through `hook_command`; `qwen` beside `opencode` in each script, with
  both words and variable).

- [ ] **Step 4: Run** `cargo test --workspace` — PASS.

- [ ] **Step 5: Commit** — `feat(connections): Qwen Code gets Templum's server and hooks, opened from the canvas or typed in a terminal`

---

### Task 12: Pi, opened or typed

**Files:**
- Create: `crates/templum-usage/src/pi.rs`, `crates/templum-usage/src/pi.ts` (the extension, `__TEMPLUM_PROGRAM__`
  for the program; no type annotations, so `node` runs it as `.mjs`)
- Modify: `crates/templum-usage/src/lib.rs`, `crates/templum-mcp/src/hooks.rs` (`pi`), `crates/templum-mcp/src/cli.rs`
- Modify: `crates/templum/src/workspace/usage.rs` (`typed`'s Pi arm; `agent_env`: `TEMPLUM_PI_ARGS`)
- Modify: the four integration scripts (`pi`), `crates/templum-terminal/src/integration.rs`
- Modify: `docs/specs/connections.md` (Pi: present tense, the spike's corrections), `docs/ARCHITECTURE.md`,
  `docs/specs/agent-parity.md`
- Test: `pi.rs`, `hooks.rs`, `tests/usage.rs`, `integration.rs`

**Interfaces:**
- Produces: `pub fn pi::args(data: &Path, windows: bool) -> Option<Vec<String>>` (`-e`, the file);
  `pub fn hooks::pi(input: &Value) -> Option<AgentEvent>` for the extension's line `{"type": "<pi event>", "answer"?:
  "<text>", "title"?: "<text>"}`.

- [ ] **Step 1: Spike S3 (by hand, if Pi has a model here).** A probe extension in the scratchpad that appends each
  event's name and keys to a file; `pi -e <probe> -p "say hi"`, then the TUI with a prompt and `/quit`. Record:
  the events in order, where `agent_end` keeps the last assistant text, whether `-e` asks to trust the project, and
  whether `pi.registerMcpServer` exists (0.85.1: no).

- [ ] **Step 2: Write the failing tests:** `pi.rs`: the words; the file holds `bin(data)` as a JSON string;
  `#[cfg(unix)]`, skipped without `node`, `the_extension_reports_pis_turns`: imported as `.mjs` with a fake `pi`
  (`on(name, f)` keeping handlers, `registerMcpServer` recording its call) and a fake program: `session_start`,
  `agent_start`, `agent_end` (with an assistant message "Done.") reach the file as three lines, the last with
  `"answer": "Done."`; `registerMcpServer("templum", { command: <bin>, args: ["mcp"], timeout: 604800 })` was
  called; without `registerMcpServer` no throw; with `TEMPLUM_HOOKED` another pid neither events nor a server; the
  program missing, no throw. `hooks.rs` `pis_events_become_events`: `session_start` → `Started`, `agent_start` →
  `Submitted { at_enter: false }`, `tool_result` → `Working`, `agent_end` → `Stopped { message: answer, transcript:
  None }`, `ui_prompt_start` with a title → `NeedsYou { message: Some(title) }`, without → `Some("Pi asks you
  something")`, `ui_prompt_end` → `Working`, `session_shutdown` → `Ended`. `tests/usage.rs`: `typed(pi, "pi
  --session-id 0d5e")` ends with ` -e <file>`. The shells: `pi` gets `-e`, the file, then the person's words; nothing
  twice on the file.

- [ ] **Step 3: Run them** — FAIL.

- [ ] **Step 4: Implement** the extension (`export default function (pi)`: the same `quiet` rule and `send` as
  OpenCode's plugin, with `["hook", "pi"]`; the server when `typeof pi.registerMcpServer === "function"`), `pi::args`,
  `hooks::pi`, the `cli.rs` arm, `typed`'s arm, the variable and the four functions.

- [ ] **Step 5: Run** `cargo test --workspace` — PASS.

- [ ] **Step 6: Commit** — `feat(connections): Pi tells Templum its turns, and gets its MCP server where Pi has MCP`

---

### Task 13: Checks by hand and the docs

**Files:**
- Modify: `docs/specs/connections.md` (Status, the section's "Planned" line, Manual checks), `docs/specs/agent-parity.md`
  (Status: "Implemented"), `docs/minors.md` (one entry per check that couldn't be made)

- [ ] **Step 1: Run `cargo test -p templum-terminal -- --ignored installed_agents`** — PASS.
- [ ] **Step 2: By hand, in `cargo run -p templum -- --dev`:** Copilot CLI from the menu (`/mcp` lists `templum`
  connected; "list the blocks" asks to allow `templum-list_blocks`, then works; the dot follows a turn and goes yellow
  at a permission; Claude Code delegating a task to it gets its answer; a restart brings its session back); a
  `copilot` typed in a bash terminal the same; OpenCode the same (`/mcp`, the dot, a delegated task's answer); Pi's
  dot and a delegated task's answer; the switch off: Gemini CLI's tools fail with the off sentence, on again they
  work. Each that can't be made (no sign-in, not installed: Qwen Code, Kilo Code) goes into `docs/minors.md` under
  Connections as "not checked by hand", with what to check.
- [ ] **Step 3: Run the full check** — `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings &&
  cargo test --workspace` — all green.
- [ ] **Step 4: Commit** — `docs: say every agent that can take Templum's server and hooks gets them`

---

## Spike results

*Filled in by Tasks 5, 9 and 12.*
