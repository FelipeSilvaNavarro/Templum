# Agent parity: what each agent gets of Templum

- **Date:** 2026-10-18
- **Status:** Audit (kanban card 44, "Melhorias task: Integrar MCP claude no terminal também"); the design that
  closes its gaps is [connections.md](connections.md#every-agent-that-can-take-it), planned in
  [docs/plans/2026-10-18-agent-parity.md](../plans/2026-10-18-agent-parity.md); [After card 44](#after-card-44) says
  what closes and what stays
- **Builds on:** [agents.md](agents.md) (the agents), [connections.md](connections.md) (the server, the hooks, what
  is typed, agents typed in a terminal), [usage.md](usage.md), [ask-user.md](ask-user.md) and
  [persistence.md](persistence.md#agents)

## The question

Card 42 gave a `claude` or `codex` typed by hand what the ones Templum opens get. Card 44 asks whether every other
agent gets everything a Claude Code gets, or whether something was forgotten between them.

**The answer, short:** no. Templum has three tiers today, and the code says so plainly:

1. **Claude Code and Codex** get everything: the server, the hooks (the status dot), a week for a tool call, the
   status line or rollout usage, plan limits, their own session back on restore, a model and an effort from
   `spawn_agent`, opened from the menu or typed by hand.
2. **Gemini CLI** gets the server and the hooks only once the person installs Templum's extension
   (Settings ▸ Connections), then every Gemini CLI has them, typed or not. It gets its session back and a model,
   but no usage, no limits, no effort, and a spawned one's prompt isn't held while it asks to trust a folder.
3. **The 17 others and the person's own agents** get what doesn't need the agent's help: the menu, being recognized,
   Shift+Enter, the question before closing, resume (by id for Qwen Code, Grok Build and Pi), and being handed a
   task, which they answer through their screen. No server unless the person adds Templum's command to the agent's
   own settings by hand, no hooks, no usage, no model or effort.

The tools themselves are agent-blind: every MCP client that has Templum's server gets the same 26 tools, checked by
wire, never by agent (`crates/templum-mcp/src/tools.rs:204`). What differs is who gets the server, who Templum hears
from, and what Templum reads of their files. Several of the others can take all of it for one session from the
command line (GitHub Copilot CLI above all, see [The gaps](#the-gaps)).

Checked on 2026-10-18 against the code at `565cbfd`, the installed `--help` of Gemini CLI 0.63.0, GitHub Copilot CLI
1.0.95, Cursor CLI 2026.10.01, OpenCode 1.19.0, Grok Build 1.0.50, Crush 0.98.1 and Pi 0.85.1, and each agent's
current documentation (links in [Sources](#sources)).

## How to read the cells

- **works**, with where the code does it (`file:line`, at `565cbfd`);
- **missing**: the agent could take it, Templum doesn't give it;
- **impossible**: the agent has no way to take it, with the document that shows it;
- **—**: not a thing for that agent (a Templum feature no agent uses).

## The matrix: Claude Code, Codex, Gemini CLI, the others

"The others" are the 17 other agents of [agents.md](agents.md#the-agents); Templum's code treats them alike except for
how they resume, which [the next table](#the-others-one-by-one) gives agent by agent. "Yours" are the agents added in
Settings ▸ Agents.

### Opening and identity

| Capability | Claude Code | Codex | Gemini CLI | The others | Yours |
|---|---|---|---|---|---|
| **AI agent ▸** in the canvas menu, opened in a folder | works: `crates/templum/src/menu/build.rs:49`, `crates/templum/src/workspace/blocks.rs:295` | works (same) | works (same) | works (same) | works (same) |
| Recognized in a terminal (its name in `list_blocks`, the snapshot) | works: `crates/templum-terminal/src/agent.rs:464` | works | works | works | works, by its command's program: `crates/templum/src/agents.rs:94` |
| Shift+Enter is a new line; closing asks first | works: `crates/templum/src/workspace/input.rs:782` | works | works | works | works |
| A starting cover with its title while it opens | works: `crates/templum/src/workspace/starting.rs:52` | works | works | works | works |
| Its own session back on restore (several blocks in one folder) | works, the id from its files: `crates/templum-usage/src/lib.rs:185`, `crates/templum-terminal/src/agent.rs:39` | works, from its rollouts: `crates/templum-usage/src/lib.rs:198` | works, opened with `--session-id`: `crates/templum-terminal/src/agent.rs:62`, `crates/templum-terminal/src/terminal.rs:436`; typed without it, the folder's latest | Qwen Code by its files, Grok Build, Pi and Copilot CLI by their arguments; 9 the folder's latest; 4 a new session (since card 44) ([table below](#the-others-one-by-one)) | its resume command, else a new session: `crates/templum/src/agents.rs:44` |
| A typed agent's restore gets Templum's flags | works: `crates/templum/src/workspace/persist.rs:189` types through `typed` | works | — (the extension) | — (nothing to add) | — |
| Forking a session | — | — | — | — | — |
| `spawn_agent`'s `model` | works, `--model`: `crates/templum-terminal/src/agent.rs:270` | works, `-m`: `crates/templum-terminal/src/agent.rs:276` | works, `-m`: `crates/templum-terminal/src/agent.rs:282` | works since card 44 for the 13 that take one, `--model` each: `LAUNCH`, `crates/templum-terminal/src/agent.rs` ([below](#the-others-one-by-one)) | refused: `crates/templum/src/agents.rs:80` |
| `spawn_agent`'s `effort` | works, `--effort`: `crates/templum-terminal/src/agent.rs:271` | works, `-c model_reasoning_effort=`: `crates/templum-terminal/src/agent.rs:277` | impossible: `gemini --help` (0.63) has no effort flag | works since card 44 for the 6 that take one: Copilot CLI, Grok Build, Pi, Kiro CLI, Aider, Cline (same table; Droid's TUI effort unconfirmed) | refused |
| The model and effort kept on restore and Ctrl+Z | works: `crates/templum/src/workspace/persist.rs:186` | works | works (model) | — | — |

Forking: Templum never forks a session, for any agent (a closed block brought back resumes the session the last
snapshot found, [connections.md](connections.md#keys-stopping-and-closing)), so there is nothing to bring to the
others. Claude Code (`--fork-session`), Codex (`codex fork`), OpenCode (`--fork`), Grok Build (`--fork-session`) and
Pi (`--fork`) could fork, if a future feature asks for it.

### The server and its tools

| Capability | Claude Code | Codex | Gemini CLI | The others | Yours |
|---|---|---|---|---|---|
| Templum's MCP server, opened from the menu, `spawn_agent` or a restore | works, `--mcp-config`: `crates/templum/src/workspace/usage.rs:68`, `crates/templum-usage/src/claude.rs:259` | works, `-c mcp_servers.templum.*`: `crates/templum/src/workspace/usage.rs:77`, `crates/templum-mcp/src/lib.rs:45` | works once the person clicks Install: `crates/templum-usage/src/gemini.rs:31`, `crates/templum/src/settings/connections.rs:125` | **missing**: only "Other agents", a command to copy into the agent's own settings: `crates/templum/src/settings/connections.rs:103` | the same copy; a command that runs `claude` or `codex` gets theirs through the shell function (next row), with shell integration on |
| Typed by hand in a terminal (card 42) | works: `crates/templum-terminal/src/integration/templum.bash:63`, `zsh/.zshenv:50`, `templum.fish:21`, `templum.ps1:75` | works: `templum.bash:72`, `zsh/.zshenv:59`, `templum.fish:42`, `templum.ps1:86` | works through the extension, typed or not | **missing** (only `TEMPLUM_CLAUDE_ARGS` and `TEMPLUM_CODEX_ARGS`: `crates/templum/src/workspace/usage.rs:113`) | — |
| A tool call may wait a week (`ask_user`, `wait_for_idle`, `delegate` with `wait`) | works, the server's `timeout`: `crates/templum-usage/src/claude.rs:266` | works, `tool_timeout_sec=604800`: `crates/templum-mcp/src/lib.rs:58` | works, the extension's `timeout`: `crates/templum-usage/src/gemini.rs:44` | their own limit ([ask-user.md](ask-user.md#how-long-it-waits)); a question's answer is then typed back | their own limit |
| The server's instructions (who it is, what is wired) | works: `crates/templum/src/workspace/bus.rs:603`, cut at 4096 by Claude Code | works, the first 512 characters weigh most | works | depends on the client: Copilot CLI reads them only from allowlisted servers unless `--allow-all-mcp-server-instructions` | depends |
| `spawn_agent` and `delegate` listed whole from the start (`anthropic/alwaysLoad`) | works: `crates/templum-mcp/src/tools.rs:218` | — (Codex lists every tool) | — | — | — |
| The 26 tools: `whoami`, `list_blocks`, `read_terminal`, `send_to_terminal`, `notify`, `connect`, `spawn_agent`, `delegate`, `reply`, `wait_for_idle`, `wait_for`, `ask_user` (v2: questions, headers, options, multiSelect, previews, a note), the kanban's 5, the note's 2, the checklist's 3, `open_file`, `send_keys`, `interrupt`, `close_block` | works: `crates/templum-mcp/src/tools.rs:256`–`594` | works (same list, same checks) | works | works for any of them given the server, nothing for one without it | the same |
| `ask_user` answered after the client stopped waiting (typed back) | works: [ask-user.md](ask-user.md#when-the-client-stops-waiting), `queue_notice` | works | works | works, typed into its terminal | works |
| A task's end typed back (`[task t7 …]`) when it handed one without `wait` | works: `crates/templum/src/workspace/delegate.rs:814` | works | works | works, for one with the server | works |
| The `templum` command line (every verb) in its terminal | works: `<data>/bin` first on PATH ([connections.md](connections.md#identity)) | works | works | works: its shell tool can run `templum reply`, `templum kanban move`…, but nothing tells it so | works |

### State: the status dot

| Capability | Claude Code | Codex | Gemini CLI | The others | Yours |
|---|---|---|---|---|---|
| Hooks Templum hears | works, 9 hooks in `--settings`: `crates/templum-mcp/src/hooks.rs:39` | works, 6 in `-c` flags once its person trusts them: `crates/templum-mcp/src/hooks.rs:97`, `crates/templum-mcp/src/lib.rs:18` | works, 6 in the extension: `crates/templum-mcp/src/hooks.rs:126`, `crates/templum-usage/src/gemini.rs:19` | **missing**: `templum hook` takes `claude`, `codex` and `gemini` only (`crates/templum-mcp/src/cli.rs:444`) | missing |
| Working (accent dot) | works: `UserPromptSubmit`, `PostToolUse` | works: same names | works: `BeforeAgent`, `AfterTool` | from its screen only: 3 of 4 looks changed (`crates/templum/src/workspace/activity.rs:80`) | from its screen |
| Waiting for you (yellow) on a permission prompt | works, `PermissionRequest`: `crates/templum-mcp/src/hooks.rs:61` | works, `PermissionRequest`: `crates/templum-mcp/src/hooks.rs:105` | works, `Notification` `ToolPermission`: `crates/templum-mcp/src/hooks.rs:139` | **missing**: a dialog is a still screen, read as at rest after 5 s (`crates/templum/src/workspace/activity.rs:88`): no yellow, and a task handed to it ends with its screen | missing |
| Waiting for you on the agent's own question | works, `AskUserQuestion` and `ExitPlanMode`: `crates/templum-mcp/src/hooks.rs:64` | not checked by hand (its own question tool may fire no `PermissionRequest`) | not checked by hand | missing | missing |
| Done (the turn ended, with its answer) | works, `Stop`, `StopFailure`, an interrupt: `crates/templum-mcp/src/hooks.rs:55` | works, `Stop`, `Interrupt`: `crates/templum-mcp/src/hooks.rs:111` | works, `AfterAgent`: `crates/templum-mcp/src/hooks.rs:134` | still screen for 5 s, the last 40 lines as its answer, or `reply`: `crates/templum/src/workspace/activity.rs:146` | the same |
| Ended | works, `SessionEnd` | the snapshot (Codex's `SessionEnd` also comes after 30 idle minutes: not followed, `crates/templum-mcp/src/hooks.rs:96`) | works, `SessionEnd` | the snapshot | the snapshot |
| Seems stuck (yellow after 30 still seconds at work) | works: `crates/templum/src/workspace/activity.rs:85` | works | works | — (needs hooks) | — |
| A task's Enter confirmed by a prompt taken | works, at its Enter | works, once a turn takes it | assumed as Codex's ([minors](../minors.md#connections)) | one blind Enter: [connections.md](connections.md#delegating) 2 | one blind Enter |
| The transcript named in a task's end | works: `crates/templum-mcp/src/hooks.rs:57` | works: `crates/templum-mcp/src/hooks.rs:113` | assumed (not checked: no login) | missing | missing |
| A spawned agent's prompt waits for its input box (not a trust question) | works: `crates/templum/src/workspace/spawn.rs:194` | works (same line) | **missing**: the still screen alone, so a first folder's trust question can take the prompt ([minors](../minors.md#connections)) | the still screen alone | the still screen alone |

### Usage

| Capability | Claude Code | Codex | Gemini CLI | The others | Yours |
|---|---|---|---|---|---|
| The session's usage chip | works, its status line, else its transcript: `crates/templum-usage/src/lib.rs:157` | works, its rollout's `token_count`: `crates/templum-usage/src/lib.rs:165` | missing ([usage.md](usage.md#later-batches) batch 2, planned) | missing (batches 2 to 4, planned) | missing |
| Plan limits in the title bar, and their notifications | works: `crates/templum-usage/src/lib.rs:49` | works (same) | none documented ([usage.md](usage.md#where-the-data-comes-from)) | batch 4 for Copilot CLI, Kiro CLI, Auggie, Amp, planned | — |
| What a task spent, in its end | works, its cost (with its status line): `crates/templum/src/workspace/delegate.rs:66` | works, its tokens | missing ([minors](../minors.md#connections)) | missing | missing |
| A status line from Templum | works, `statusLine` in `--settings`: `crates/templum-usage/src/claude.rs:226` | impossible: its status line is a list of built-in items, no command | impossible: no status line command | Copilot CLI (`copilot help config`), Qwen Code, Grok Build, Droid and Cursor CLI run a status line command, each only from its settings file | — |

### Other features

| Capability | Claude Code | Codex | Gemini CLI | The others | Yours |
|---|---|---|---|---|---|
| Git's **Generate** commit message preset | works: `crates/templum/src/agents.rs:171` | works: `crates/templum/src/agents.rs:175` | none preset; the person can fill its command ([git.md](git.md)) | the same | the same |
| `[connections] enabled` turns it off | works: `crates/templum/src/workspace/usage.rs:125` | works | no: the extension serves every Gemini CLI once installed | — | — |

## The others, one by one

What Templum does for each today, and what the agent itself could take. "Per session" means a flag or a variable for
one run, without editing the person's files, which [connections.md](connections.md#out-of-scope) requires.

Templum today, for every one of them: the menu, being recognized, Shift+Enter, the question before closing, the
screen's state, and being handed a task, as in the matrix; the column "Restore today" is what differs. The other
columns are what the agent offers, from its `--help` (installed here: Copilot CLI 1.0.95, Cursor CLI 2026.10.01,
OpenCode 1.19.0, Grok Build 1.0.50, Crush 0.98.1, Pi 0.85.1) or its docs; "unconfirmed" is a fact neither showed.

| Agent | Restore today | A session id Templum could use | Templum's server for one session | Hooks or events for one session | Model, effort | Its permission prompt, heard |
|---|---|---|---|---|---|---|
| GitHub Copilot CLI | by its id since card 44 (`--session-id`); typed without one, a new session | `--session-id <uuid>` starts or resumes one; `--continue` falls back to any folder's latest | `--additional-mcp-config @<file>` (its servers get only `PATH` and the variables their `env` names, `${VAR}` expanded; `timeout` 30 s unless set; it reads a server's instructions up front only with `--allow-all-mcp-server-instructions` or its own allowlist) | a plugin's hooks, `--plugin-dir`; a hook's payload doesn't name its event; `agentStop` carries no answer, only `transcriptPath` | `--model`, `--reasoning-effort` (`none` to `max`) | `notification` `permission_prompt` (`permissionRequest` comes before its own rules, for calls they allow too) |
| Cursor CLI | the folder's latest since card 44 (`--continue`) | `--resume <chatId>`, `--continue`, both within the folder's chats (re-checked: its chats are kept per folder); `create-chat` prints a new chat's id; a hidden `--new-session-id <uuid>` | a plugin, `--plugin-dir`, whose manifest declares `mcpServers` (a run unconfirmed), with `--approve-mcps` | the plugin's hooks (`sessionStart`, `beforeSubmitPrompt`, `stop`…); it also runs Claude Code's hooks from `~/.claude/settings.json` and a project's `.claude/settings.json` | `--model` (an effort is written in the model, `[effort=high]`, which Templum's model rule refuses) | none (a `before*` hook can only ask) |
| OpenCode | the folder's latest | `-s <id>`, an existing session only (a new one's can't be chosen) | `OPENCODE_CONFIG_CONTENT`, merged last and deeply with the person's config, `plugin` lists adding up (re-checked with 1.19.0 under `opencode serve`); a server's `timeout` covers its start and every call, else the MCP SDK's 60 s | JS plugins named in that variable (a `file://` path, no install): `session.idle`, `session.status`, `permission.asked`, `question.asked`, the `chat.message` hook | `-m provider/model`; effort only for `opencode run --variant` | `permission.asked` |
| Amp | the folder's latest | `amp threads continue <T-id>` | `--mcp-config '<json>'` | TS plugins (`agent.start`, `agent.end`, `tool.call`), installed with `amp plugins add`; per session unconfirmed | `--mode` only | — (Amp asks before no tool) |
| Kiro CLI | the folder's latest | `--resume-id <id>` | an agent's config (`--agent <name>`, a file under `~/.kiro/agents`), or `KIRO_HOME`, which moves its login too | the same agent config (`SessionStart`, `UserPromptSubmit`, `Stop`, `Pre`/`PostToolUse`) | `--model`, `--effort` | none |
| Qwen Code | its own session, from its files | `--session-id` (a new session only), `--resume <id>` | `--mcp-config <json\|file>`, never asked about (re-checked in its 0.25.2 source) | Claude Code's hooks, but only in its settings files (no `--settings`); `QWEN_CODE_SYSTEM_SETTINGS_PATH` moves its system file, with no check of its owner; its extensions (Gemini CLI's too) serve every Qwen Code once linked | `-m`; effort only in its settings | `PermissionRequest`; `Notification` `permission_prompt` |
| Grok Build | its own session (`--session-id`) | already used | none in 1.0.50's `--help` (its docs name a `--plugin-dir`); `GROK_HOME` moves its login too. It reads Claude Code's `~/.claude.json` and a project's `.mcp.json` (`grok inspect`, "Harness Compatibility") | hooks in `~/.grok/hooks/*.json` or `.claude/settings.json`, files only | `-m`, `--reasoning-effort` | unconfirmed (`Notification`) |
| Kimi Code | the folder's latest | `-S <id>` | files only (`~/.kimi-code/mcp.json`) | `[[hooks]]` in its config, files only | `-m` | `PermissionRequest` |
| Mistral Vibe | the folder's latest | `--resume <id>` | files only (`VIBE_HOME` moves everything) | `pre_tool`, `post_tool`, `post_agent`, files only | unconfirmed | none |
| Aider | `--restore-chat-history` | none | impossible: no MCP | `--notifications-command` (a turn's end) only | `--model`, `--reasoning-effort` | — |
| Goose | new session (its resume takes the newest of any folder) | unconfirmed | `goose session --with-extension "<command>"` | a plugin's hooks, no flag | `GOOSE_MODEL` (a variable) only | none |
| Crush | the project's latest since card 44 (`--continue`) | `-s <id>`, an existing session only; `-C` continues the latest of the project's `.crush` database (re-checked in its source) | none: only `CRUSH_GLOBAL_CONFIG`, which replaces its whole global folder | `PreToolUse` only | none for its TUI (`crush run -m`) | none |
| Droid | new session | `--resume <id>` | `--settings <path>` for one process (servers in it unconfirmed) | hooks in that file (unconfirmed); `Notification` | `-m`; `--reasoning-effort` documented for `droid exec`, its TUI's unconfirmed | `Notification` `permission_prompt` |
| Auggie | new session | `--resume <id>`, `--continue` | `--mcp-config <file\|json>` | a plugin's hooks, `--plugin-dir` | `--model` | `Notification` (unconfirmed) |
| Kilo Code | the folder's latest | `--session <id>` | `KILO_CONFIG_CONTENT` (an OpenCode fork) | plugins as OpenCode's (unconfirmed) | `--model` | unconfirmed |
| Cline | new session | `--id <id>` | `CLINE_MCP_SETTINGS_PATH`, a variable naming a servers file | `--hooks-dir <path>`; TS plugins | `-m`, `--thinking` | unconfirmed |
| Pi | its own session (`--session-id`) | already used | none in 0.85.1 ("No MCP", its README); built in since 0.99 (1.3.0 released 2026-10-15): `mcp.json` files, or `pi.registerMcpServer()` from an extension for one session | an extension, `-e <file.ts>`: `session_start`, `agent_start`, `agent_end`, `tool_result`, `ui_prompt_start`, `session_shutdown`, and `registerTool` | `--model`, `--thinking` | — (it has no permission prompts: its `docs/usage.md`) |

Two things the table shows beyond Templum: **Grok Build** already reads Claude Code's MCP servers and hooks from
Claude Code's own files, which Templum never writes ([connections.md](connections.md#out-of-scope)), so that road stays
closed; and **Qwen Code** installs Gemini CLI extensions, so Templum's Gemini extension may serve it as it is (not
tried).

## The gaps

Ranked by what a person gains for the work. Size: small (a day or less), medium (a few days), large (a batch of its
own). "Design" means it needs a plan in `docs/plans/` first (a checked-by-hand spike of the agent, as batches 7 and 8
had); "direct" means it follows a pattern already in the code.

| # | Gap | What a person sees today | The fix | Size | Kind |
|---|---|---|---|---|---|
| 1 | **GitHub Copilot CLI gets nothing, though it takes everything for one session** | Copilot opened from the menu has no Templum tools, its dot only follows its screen, a permission prompt shows no yellow, a task handed to it ends 5 s after it stops moving, a restart opens a new session, `spawn_agent` refuses a model | The way Codex got it: `--additional-mcp-config @<data>/copilot-mcp.json` (the server, `TEMPLUM_BUS` and `TEMPLUM_TERMINAL`, a week's `timeout`), a plugin in `<data>/copilot` with its hooks given by `--plugin-dir`, Templum's instructions let in (`--allow-all-mcp-server-instructions` or its allowlist), `templum hook copilot` (`permissionRequest`, `userPromptSubmitted`, `postToolUse`, `agentStop`, `sessionEnd`); `--session-id` as Gemini CLI's `new_id` and `resume_id`; then `TEMPLUM_COPILOT_ARGS` and a `copilot` function for typed ones | large | design: a spike of its hook payloads and of the instructions flag, as batch 7 did for Codex |
| 2 | **`spawn_agent` refuses a model or an effort for 13 agents that take one** | "Templum doesn't give Grok Build a model or an effort." for Copilot CLI, Grok Build, Pi, OpenCode, Cursor CLI, Qwen Code, Kiro CLI, Kimi Code, Droid, Auggie, Kilo Code, Cline and Aider | A row each in `LAUNCH` (`crates/templum-terminal/src/agent.rs:267`), each flag and effort list read from the installed `--help` as on 2026-10-16: Copilot `--model`/`--reasoning-effort`, Grok `-m`/`--reasoning-effort`, Pi `--model`/`--thinking`, OpenCode `-m`, Cursor `--model`, Qwen and Kimi `-m`, Auggie and Kilo `--model`, Kiro `--model`/`--effort`, Droid `-m`/`--reasoning-effort`, Cline `-m`/`--thinking`, Aider `--model`/`--reasoning-effort`; the tool's description names them | small | direct |
| 3 | **Agents without hooks look done at a dialog** | Any agent but the three asking "Allow?" shows no yellow dot and no alert; `wait_for(needs_you)` never answers; a task handed to it ends after 5 still seconds with its screen as if it answered | Hooks for the agents that hear their own permission prompt and take them for one session: Copilot CLI (1), OpenCode's `permission.asked` (5), Qwen Code's `PermissionRequest` (6), Droid's `Notification` (Auggie's unconfirmed); each a `templum hook <agent>` parser into the same events (`crates/templum-mcp/src/hooks.rs`). No generic fix: reading each agent's dialog off its screen is guesswork | large (one batch per agent, small once its hooks reach Templum) | design |
| 4 | **Some agents come back on a new session though they can resume their own** | Copilot CLI, Cursor CLI, Crush, Droid, Auggie, Cline: after a restart (or Ctrl+Z on a closed block) the block starts a new conversation | Copilot: `--session-id` as Pi's (`new_id` and `resume_id`, read back from its arguments). Crush (`-C`, per project) and Cursor (`--continue`): a `resume` once checked limited to the folder. The others: the id from their files, as Qwen Code's (`crates/templum-usage/src/lib.rs:185`) | small (Copilot, Crush, Cursor); medium (ids from files or databases) | direct; each "limited to the folder" checked by hand first ([agents.md](agents.md#the-agents)'s rule) |
| 5 | **OpenCode and Kilo Code: no server, no state** | OpenCode has no Templum tools unless the person edits its config by hand, and then every OpenCode outside Templum shows a failing server | `OPENCODE_CONFIG_CONTENT` (`KILO_CONFIG_CONTENT`) for the OpenCode Templum starts, and through a shell function for a typed one, as card 42 did: the server (a long `timeout`) and a small JS plugin that runs `templum hook opencode` on `session.status`, `session.idle`, `permission.asked`. Not in every terminal's environment: an `opencode run` an agent's shell tool starts would inherit it and speak for the block, the reason card 42 chose functions | medium | design: whether the variable merges with the person's `plugin` list, and the plugin's events by hand |
| 6 | **Qwen Code: no server, no state** | Qwen Code, a Gemini CLI fork with Claude Code's hooks, gets neither | `--mcp-config` for the server; for the hooks, either Templum's Gemini extension linked into Qwen Code (`qwen extensions` takes Gemini CLI's, as Install does for Gemini) or `QWEN_CODE_SYSTEM_SETTINGS_PATH` if Qwen doesn't refuse a file root doesn't own, as Gemini CLI does | medium | design |
| 7 | **Pi: no server by design, but an extension per session** | Pi has no Templum tools and only its screen's state; Settings ▸ Connections tells the person to add Templum's command to Pi's MCP servers, which Pi 0.85.1 doesn't have | (a) The description of **Other agents** (`crates/templum/src/settings/connections.rs:105`) names agents that have MCP: drop Pi. (b) A Templum extension in `<data>/pi/templum.ts`, given with `-e`: `registerTool` for each Templum tool (calling the bus, as `templum` verbs do) and `agent_start`, `agent_end`, `ui_prompt_start` sent as events | (a) small; (b) large | (a) direct; (b) design |
| 8 | **Gemini CLI is connected only after a click** | A person who never opens Settings ▸ Connections has a Gemini CLI without tools or a dot, and nothing says why | Asking once, when Gemini CLI is first opened from the menu or `spawn_agent` (the extension needs the person's consent: `--consent`), with the same question Install asks | small | design: where the question shows, and remembering a "no" |
| 9 | **Gemini CLI ignores `[connections] enabled`** | Turning "Agents in Templum" off leaves every Gemini CLI connected, its dot moving, its tools working | The bus answers a `templum mcp` and a `templum hook gemini` from a terminal while the switch is off as from a terminal it doesn't have: tools fail with "Connections are off in Templum's settings", events ignored | small | direct |
| 10 | **Amp, Auggie, Goose, Cline and Droid take a server for one session, and get none** | No Templum tools for them | Their flags, as Codex's: Amp `--mcp-config '<json>'`, Auggie `--mcp-config`, Goose `--with-extension "templum mcp"` (whether it hands an extension `TEMPLUM_BUS` and `TEMPLUM_TERMINAL` is unchecked), Cline `CLINE_MCP_SETTINGS_PATH` and `--hooks-dir`, Droid `--settings`; each its own `typed` arm and variable for typed ones | medium each | design (a spike each: the environment they pass a server, their timeout) |
| 11 | **Agents without the server aren't told about `templum`** | Aider, Pi and the others can run `templum reply`, `templum kanban move`, `templum note append` from their shell tool, but never do: nothing tells them they are in Templum | A short text for the agents that take one for a session: Pi `--append-system-prompt`, Aider `--read <file>`; the verbs' list as the server's instructions give it | medium | design: the words, and which agents |
| 12 | **Usage for the others** | No usage chip, no "spent" in a task's end, for any agent but Claude Code and Codex | Already planned: [usage.md](usage.md#later-batches) batches 2 to 4. New since: Grok Build's `grok usage`, Crush's `crush stats`, Copilot CLI's `statusLine` command (its settings file only) | as planned | as planned |
| 13 | **Codex's and Gemini CLI's own questions aren't checked** | Unknown: whether Codex's `request_user_input` or Gemini CLI's `ask_user` turns the dot yellow, as Claude Code's `AskUserQuestion` does | Check by hand; if a hook tells, map it as `hooks::claude` maps `AskUserQuestion` | small | direct |

**Already logged** in [docs/minors.md](../minors.md#connections), and not repeated above: a spawned Gemini CLI's prompt
in its folder-trust question, Gemini CLI's `BeforeAgent` for a prompt typed while it works, what Gemini CLI spent, an
agent typed in a terminal not an agent to `delegate` for its first seconds, and Gemini, Grok and Pi coming back on
the session they were opened on rather than the one they switched to.

Gaps 2, 7a, 9 and 13 are also in [docs/minors.md](../minors.md#connections), as the repository's rule asks.

**Order to do them in:** 2 and 7a now (small, direct); 1 next (Copilot CLI is the one agent that takes all of
Claude Code's parts for one session); then 5, 6 and 3 with them; 8 and 9 for Gemini CLI; 4 alongside 1; 10 and 11
when someone uses those agents.

## After card 44

What [connections.md](connections.md#every-agent-that-can-take-it) and its plan
([docs/plans/2026-10-18-agent-parity.md](../plans/2026-10-18-agent-parity.md)) do with each gap above. The facts were
checked again on 2026-10-18 (the installed `--help`s, Copilot CLI's bundled SDK types, OpenCode 1.19.0 under
`opencode serve` with a probe plugin, the sources of Qwen Code 0.25.2, Crush 0.98.1, Kilo Code 7.8 and Pi 1.3.0);
the table above carries the corrections.

| # | Gap | Card 44 | Plan task |
|---|---|---|---|
| 1 | Copilot CLI gets nothing | **Closes**: its server, its hooks (a plugin), its instructions, its session by id, opened or typed | 4–8 |
| 2 | `spawn_agent` refuses a model or an effort | **Closes**: a model for 13 agents, an effort for Copilot CLI, Grok Build, Pi, Kiro CLI, Cline and Aider ([agents.md](agents.md#a-model-and-an-effort), whose flags win over gap 2's) | 1 |
| 3 | Agents without hooks look done at a dialog | **Closes for the agents card 44 connects**, through their own hooks; **stays** for the others: no generic reader of dialogs off a screen | 5, 9, 11, 12 |
| 4 | Agents back on a new session | **Closes** for Copilot CLI (`--session-id`), Cursor CLI and Crush (`--continue`, each kept per folder); **stays** for Droid, Auggie and Cline (their ids from their files: medium) and OpenCode and Kilo Code (an id that can't be chosen) | 4 |
| 5 | OpenCode and Kilo Code | **Closes**: `OPENCODE_CONFIG_CONTENT` and `KILO_CONFIG_CONTENT`, the server and a plugin; Kilo Code from its source only | 9, 10 |
| 6 | Qwen Code | **Closes**, from its docs and source only (not installed here): `--mcp-config`, and hooks in a system settings file Templum moves | 11 |
| 7 | Pi | (a) **closes**: Settings no longer names it; (b) **closes** as an extension that registers the server where Pi has MCP (0.99 and later) and reports its events everywhere | 2, 12 |
| 8 | Gemini CLI connected only after a click | **Closes** as a notice once per run when it opens without the extension; its one-session road is still none | 3 |
| 9 | Gemini CLI ignores the switch | **Closes** for every agent: off, the bus refuses `templum mcp` and `templum hook` | 2 |
| 10 | Amp, Auggie, Goose, Cline, Droid take a server for one session | **Stays**: a spike each, when someone uses them | — |
| 11 | Agents without the server aren't told about `templum` | **Stays** | — |
| 12 | Usage for the others | **Stays** in [usage.md](usage.md#later-batches) (Copilot CLI's status line only from its settings file) | — |
| 13 | Codex's and Gemini CLI's own questions | **Stays**, a minor | — |
| 14 | *New:* Cursor CLI takes a plugin with servers and hooks (`--plugin-dir`, its manifest's `mcpServers` and `hooks`) | **Stays**: not run once, and not among the card's agents; the next one to connect | — |

**What a client can't do, left out** (each cell of the matrix above says it too):

- **An effort** for OpenCode and Kilo Code (only `run --variant`), Qwen Code (its settings only), Cursor CLI (in the
  model's name), Kimi Code (`--thinking` is on or off), Gemini CLI (no flag); Droid's and Auggie's are unconfirmed.
- **A session id Templum picks** for OpenCode, Kilo Code and Crush (`-s` takes an existing one); Cursor's
  `--new-session-id` is hidden from its help, so may go.
- **Copilot CLI's status line** for one session (its `settings.json` only), and an answer in its `agentStop` (read
  from its transcript instead).
- **A server for Pi 0.85.1** (no MCP before 0.99); permission prompts in Pi (it has none).
- **Crush:** no server for one session (`CRUSH_GLOBAL_CONFIG` replaces its whole global folder), no model for its TUI,
  `PreToolUse` its only hook.
- **Hooks for one Qwen Code session without a variable:** it has no `--settings`; and none at all when its own system
  settings file exists, which Templum's would hide.

## What doesn't need fixing

- **The tools.** None of the 26 is tied to an agent: once an agent has the server, `ask_user` v2, the kanban, the
  notes, the checklists, `delegate`, `reply`, `wait_for` and the control tools work for it as for Claude Code. Only
  `anthropic/alwaysLoad` is Claude Code's, and no other client needs it.
- **Being handed work.** Every agent can be the target of `delegate`, `send_keys` and `interrupt`; those without
  hooks answer through their screen or `templum reply`, as [connections.md](connections.md#delegating) 6 says.
- **Codex's missing `Ended` and late `Started`.** Deliberate: its `SessionEnd` comes after 30 idle minutes too, and
  its `SessionStart` with the first prompt; the snapshot tells when it quits.
- **Forking.** No Templum feature forks a session, for any agent.

## Sources

Read on 2026-10-18.

- **Installed `--help`:** `gemini --help` (0.63.0), `copilot --help`, `copilot help config`, `copilot help
  environment`, `copilot plugin --help` (1.0.95), `cursor-agent --help` (2026.10.01), `opencode --help` (1.19.0),
  `grok --help` and `grok inspect` (1.0.50), `crush --help` (0.98.1), `pi --help` and its `README.md`,
  `docs/extensions.md`, `docs/usage.md` (0.85.1).
- **Claude Code:** [CLI reference](https://code.claude.com/docs/en/cli-reference),
  [hooks](https://code.claude.com/docs/en/hooks), [status line](https://code.claude.com/docs/en/statusline).
- **Codex:** [config reference](https://developers.openai.com/codex/config-reference),
  [hooks](https://developers.openai.com/codex/hooks), [CLI reference](https://developers.openai.com/codex/cli/reference).
- **Gemini CLI:** [CLI reference](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/cli-reference.md),
  [hooks](https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/reference.md),
  [extensions](https://github.com/google-gemini/gemini-cli/blob/main/docs/extensions/reference.md),
  [configuration](https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/configuration.md).
- **Qwen Code:** [hooks](https://github.com/QwenLM/qwen-code/blob/main/docs/users/features/hooks.md),
  [MCP](https://github.com/QwenLM/qwen-code/blob/main/docs/users/features/mcp.md),
  [extensions](https://github.com/QwenLM/qwen-code/blob/main/docs/users/extension/introduction.md).
- **GitHub Copilot CLI:** [CLI reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference),
  [plugins](https://docs.github.com/copilot/concepts/agents/copilot-cli/about-cli-plugins).
- **Cursor CLI:** [docs](https://cursor.com/docs/cli). **OpenCode:** [config](https://opencode.ai/docs/config),
  [plugins](https://opencode.ai/docs/plugins). **Amp:** [manual](https://ampcode.com/manual).
  **Kiro CLI:** [docs](https://kiro.dev/docs/cli). **Grok Build:** [docs](https://docs.x.ai/build).
  **Droid:** [docs](https://docs.factory.ai/cli). **Crush:** [repository](https://github.com/charmbracelet/crush).
  **Goose:** [repository](https://github.com/aaif-goose/goose). **Kimi Code:**
  [repository](https://github.com/MoonshotAI/kimi-code). **Mistral Vibe:**
  [repository](https://github.com/mistralai/mistral-vibe). **Auggie:** [docs](https://docs.augmentcode.com/cli).
  **Kilo Code:** [docs](https://kilo.ai/docs). **Cline:** [docs](https://docs.cline.bot).
  **Aider:** [options](https://aider.chat/docs/config/options.html). **Pi:**
  [repository](https://github.com/earendil-works/pi).
