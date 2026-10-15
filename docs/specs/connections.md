# Connections: blocks that talk to each other

- **Date:** 2026-10-15
- **Status:** Batches 1 to 3 implemented; batches 4 and 5 planned
- **Builds on:** [agents.md](agents.md) (opening agents), [usage.md](usage.md) (the `--settings` Templum types for
  Claude Code), [shell-integration.md](shell-integration.md) (command marks, notifications),
  [persistence.md](persistence.md), [kanban.md](kanban.md) and [cockpit.md](cockpit.md) (notes, the editor, drawing)

## Goal

Templum is a cockpit for CLI agents, but each block is an island: an agent doesn't know it is on a canvas, can't
read the dev server's log in the block beside it, can't move its card, leaves nothing behind for the next agent,
and handing work to another agent means pasting a transcript. With connections, a person **wires** blocks together
on the canvas, and an agent can use every block wired to it: read a terminal, type into it, hand a task to another
agent and get its answer back, start a new agent beside itself, move kanban cards, write to a shared note, open a
file in the editor, and ask the person a question in a balloon next to its block.

Templum serves this as an **MCP server** the agents it opens are given, and as a **`templum` command line** for
agents without MCP, scripts and hooks.

**Success criteria**

1. The Claude Code and Codex Templum opens list Templum's MCP server as connected, without any setup and without
   Templum changing their settings files.
2. An agent knows which block it is, which blocks are wired to it, and the others it could ask to be wired to.
3. A person wires two blocks with the Wire tool or a block's menu; wires follow their blocks, can be selected,
   deleted and taken back, and come back after a restart.
4. An agent reads and writes only the blocks wired to it; asking for another shows a balloon the person answers.
5. Claude Code wired to Codex hands it a task and gets Codex's final answer back as the tool's result, and the task
   is visible in Codex's terminal.
6. A block's status dot shows whether its agent is working or waiting for the person, within a second.
7. `templum blocks`, `templum read <block>` and the other verbs work in any terminal Templum opened.
8. All tests in [Testing](#testing) pass in CI on the 3 OS; `--bench` and `--bench-mixed` keep **p95 ≤ 16.7 ms**.

## Out of scope

- Editing an agent's own settings files (`~/.claude/settings.json`, `~/.codex/config.toml`, Gemini's): Templum
  passes everything on the command line it types, for that session only. The one exception is Gemini CLI, below,
  and only after the person agrees.
- Wires between workspaces, and wires to drawings.
- A general message bus between programs: the bus serves Templum's own clients (the MCP bridge, hooks, the command
  line).
- Agents talking to blocks they aren't wired to, other than asking to be wired.
- Remote agents, other machines, other users.

## Words

- **Block id:** a short code that names a block for its whole life, through restarts, undo and moves between
  workspaces: five lowercase letters and digits (`b7k2m`). Agents and the command line name blocks by it.
- **Wire:** a connection between two blocks of one workspace. It has no direction: both ends may use each other.
- **The bus:** the local socket Templum listens on while it runs; every MCP server, hook and `templum` command
  started in one of its terminals talks to it.
- **Caller:** the block the MCP server or command runs in, found from its terminal.

## What a person sees

### Wires

- **The Wire tool** (a cable icon, in the toolbar's Pointer group, after Hand): press on a block, drag, and a curve
  follows the pointer; release on another block of the workspace to wire them. Released anywhere else, nothing
  happens. The tool stays on until another tool is picked. A press on the empty canvas does nothing. While the pointer
  is over another block the curve already runs to its edge, in the accent color. Wiring leaves no block with the
  keyboard, so Ctrl+Z takes the wire back.
- **A block's menu:** **Connect to ▸** lists the workspace's blocks not wired to it, by title; **Disconnect from ▸**
  the ones that are. They follow Fullscreen. A tile's menu has neither: grid mode hides wires.
- **A wire** is a curve from the edge of one block that faces the other to the edge of the other, drawn on the
  canvas below the blocks in the title's dim color, 2 units wide and never under a pixel. It follows its blocks as
  they move or are resized.
- **Selecting:** a click on a wire where no block or drawing covers it selects it (accent color); a drag from it
  moves the canvas as from anywhere, leaving the keyboard where it was and selecting nothing. Delete or Backspace
  removes it while no block has the keyboard (focusing, adding or selecting a block unselects it); Esc unselects;
  right-click on it offers **Disconnect**.
- **Undo:** connecting and disconnecting are steps of Ctrl+Z and Ctrl+Shift+Z. Closing a block removes its wires,
  and undoing the close brings them back.
- **A block wired twice** to the same block, or to itself, is refused silently.
- **While a task is handed over** along a wire ([Delegating](#delegating)), a dot travels along it from the agent
  that handed it to the one working, until the answer comes back. It takes 1.2 s from one end to the other, then starts
  again; it stays at the wire's middle when the system asks for less motion. With no wire between the two, there is
  no dot (the answer still comes back).
- **Far away** a wire is 1 pixel wide: from 50% down, where the Card level of
  [level-of-detail.md](level-of-detail.md) comes. **Grid mode** hides wires, as it hides drawings.
- **Moving a block to another workspace** drops its wires, those between blocks moved together too.
- **Labels** (batch 5): **Rename wire…** in a wire's menu; the label sits in a pill at the curve's middle, down to
  the Silhouette level.

### An agent's state

A terminal running an agent Templum hears from ([Hooks](#hooks-and-agent-state)) shows it in its status dot:

| State | Dot | Tooltip |
|---|---|---|
| Working on a prompt | accent | "Working" |
| Waiting for the person (a permission dialog, a question) | yellow | "Waiting for you: *the agent's message*" ("Allow Bash?" as a permission dialog opens) |
| Done with its turn | none (its running command would otherwise read "Running") | — |

An alert the terminal already shows (attention, a finished command) comes first. An agent the last snapshot doesn't
see (it crashed: no `SessionEnd`) shows no state, and its state is forgotten, so a `claude` typed there by hand
later starts with none.

### Requests from agents

- **Notify:** a notification titled with the block's title, the agent's text as its message; clicking it reveals the
  block. In the background it is a system notification, as a terminal's are.
- **A question** (`ask_user`) and **a request to connect** (`connect`) open a **balloon** on the asking block's top
  right corner, over its terminal, at the Full level of detail:
  - a question shows its text and a button per option, or a text field and **Send** when it has none;
  - a request to connect says "*Claude Code · api* wants to connect to *Dev server*" with **Allow** and **Deny**;
    Allow wires them;
  - a request for a block already wired is answered at once; Allow is a step of Ctrl+Z, as wiring by hand is;
  - several requests from one block wait in turn: the balloon shows the oldest, with "1 of 3";
  - while the balloon's field has the keyboard, no key reaches the terminal;
  - when the block isn't on screen (another workspace, outside the view, or hidden by grid mode), a notification says
    what the agent asks; clicking it reveals the block;
  - the block's dot shows it waiting for the person (yellow, "Waiting for you: *the question*"), at every level;
  - a request no one answers is withdrawn after its timeout (30 minutes unless the agent asked for another), and the
    agent is told so; closing the block withdraws its requests, and so does the agent leaving (its connection to
    Templum ends) or cancelling the call.

### Delegating

1. *Claude Code · api* is wired to *Codex · api*. It calls `delegate` with Codex's block id and a task.
2. Templum types into Codex's terminal, as one pasted message, a first line `[from Claude Code · api (b7k2m)]` and
   the task, then Enter, a moment (150 ms) after the paste: an agent may take keys that come in one burst for a
   paste, where Enter is a new line. The Enter goes only once the agent's screen shows the paste (Templum looks for
   about 2 seconds), and only if Templum still may type into it (5): a dialog that ignores pastes, such as Codex's
   approval, which Templum hears nothing of, or a permission prompt that opened meanwhile, gets no Enter that would
   answer it. The task then ends at once: the delegating agent is told the agent didn't take it, and to look at its
   block. The person sees it arrive.
3. The wire's dot travels from Claude Code to Codex while Codex works.
4. When Codex finishes its turn, its last answer becomes the result of Claude Code's `delegate` (when it asked to
   wait), or of a later `wait_for_idle`. If Codex calls `reply` first, that text is the answer instead.
5. Templum refuses to type into an agent that is waiting for its person (a permission prompt would take the text
   as its answer), into a terminal running no agent (`send_to_terminal` is for those), into an agent no snapshot has
   found yet (one Templum just typed may not have started at all; the caller is told to try again in a few
   seconds), and into an agent already on another agent's task (`wait_for_idle` waits for its end).
6. An agent Templum gets no events from (see the table under [Hooks](#hooks-and-agent-state)) answers only through
   `reply`, or the wait ends at its timeout with the agent's last screen lines.
7. A task stays handed over until its answer comes, the agent's session ends (or a snapshot no longer finds the agent
   it found before: Codex says nothing when it quits), its block closes, or its timeout passes (`delegate`'s, 600 s
   unless asked, also without `wait`).
8. `reply` with no call waiting types `[reply from <title> (<id>)]` and the text, then Enter, into the agent named;
   never into a terminal running no agent, or an agent no snapshot has found yet.
9. A task typed into an agent working on a prompt of its own is answered by the end of the next turn: Codex runs
   typed input as its next turn, so that answer may be the turn it was on.

### Spawning

`spawn_agent` opens a new agent block beside the caller, wired to it, in the caller's folder or the one it names.
With a worktree name, Templum first runs `git worktree add -b <name> <repository>/.worktrees/<name>` from the
caller's repository (and adds `.worktrees/` to the repository's `.git/info/exclude` once), and the agent starts
there, so several agents can change one project at once without stepping on each other. A prompt is typed once the
agent is ready: when Claude Code reports its session started; for the others, when the agent's process is seen and
its screen has been still for a second. The result names the new block and its folder.

The keyboard, the selection and the camera stay where they were; one Ctrl+Z closes the new block and its wire. The
worktree comes from the repository of the folder the agent starts in, always under its main working tree's
`.worktrees/`, on a new branch from that folder's HEAD; a name other than letters, digits, `-`, `_`, `.` and `/`, or
one already a branch, is refused. Claude Code's prompt waits for its `SessionStart`, which comes after its question
about trusting the folder. The prompt is a task the caller handed over: the dot travels, `wait_for_idle` waits for
its answer, and a prompt the agent doesn't take ends at once, as a delegated task does.

## Identity

- Every block gets a **block id** when it is added. It is saved with the block and kept when the canvas comes back,
  when a closed block is brought back with Ctrl+Z and when the block moves to another workspace. A pasted block,
  or one whose id is already taken, gets a new one. Ids are unique among all workspaces.
- Every terminal already gets `TEMPLUM_TERMINAL` ([usage.md](usage.md)); every program in it inherits it, the MCP
  server and hooks an agent starts included. The bus finds the caller by it, at each call, so a block moved to
  another workspace stays the same caller.
- Every terminal also gets `TEMPLUM_BUS`, the bus's address, when there is a bus, and a `PATH` that starts with
  `<data>/bin`, so `templum` in a Templum terminal is the Templum that opened it. There Templum keeps, on Unix, a
  link named `templum` to itself; on Windows `templum.exe`, a hard link to its program or, where none can be made
  (another drive), a copy, made again at start when its length or modified time aren't the program's. (The
  program's own folder may be Downloads, with no `templum.exe` and any other program in it.) Empty entries of
  Templum's own `PATH` (the current folder) are left out.

## The bus

- **Unix:** a socket at `<data folder>/bus/<Templum's pid>-<n>.sock`, in a folder only the user can open (0700).
  `TEMPLUM_BUS=unix:<path>`. When the path would pass 100 bytes (macOS's limit is 104), the socket goes to
  `bus.sock` in a folder of its own in the temporary folder instead, `templum-<12 random hex digits>`, made 0700 and
  only if it isn't there yet (`/tmp` is shared: no one can guess its name or make it first); it goes with the bus.
  A socket file left by a Templum that is gone is removed from `<data>/bus` when the bus starts.
- **Windows:** a TCP socket on `127.0.0.1` at a port the system picks, and a random token.
  `TEMPLUM_BUS=tcp:127.0.0.1:<port>:<token>`.
- **No data folder** (`--bench`), no bus: terminals get no `TEMPLUM_BUS`, agents get no MCP server or hooks.
- **Trust:** the bus trusts every process of the user that can reach it, as the user's shells can already type into
  every terminal. The folder's permissions (Unix) and the token (Windows) keep other users out. The terminal a
  client names is taken as is.
- **Several Templums** each have their own bus; a terminal's `TEMPLUM_BUS` names its own Templum's.
- **The protocol** is JSON-RPC 2.0, one message per line, for every client. A client's first line says who it is:

  ```json
  {"jsonrpc":"2.0","method":"templum/hello","params":{"terminal":"12345-3","client":"mcp","token":null}}
  ```

  `client` is `mcp` (the bridge), `cli` (a verb) or `hook`. A first line that isn't a hello, a wrong token, a
  first line over 4096 bytes, or one not whole 5 seconds after connecting (however its bytes trickle in) closes the
  connection; nothing is written to a connection before its hello is accepted, and once it is, the connection has
  no time limit. Hooks send `templum/event` notifications; the command line's verbs send `tools/call`. When a
  connection ends the bus tells Templum (`templum/closed`, with the connection's number), and what that client waited
  for is dropped.
- Requests are answered on Templum's main thread, in the order they arrive per connection; a request that waits (a
  question, a delegation) never holds back the others.

## The MCP server

`templum mcp` is a bridge: it connects to the bus, sends its hello, then copies lines from its stdin to the bus and
back to its stdout. Templum answers MCP's `initialize`, `ping`, `tools/list` and `tools/call` ([MCP 2025-11-25](
https://modelcontextprotocol.io/specification/2025-11-25)); tools only, no resources or prompts.

- **`initialize`** answers the client's protocol version when Templum knows it (`2025-11-25`, `2025-06-18`,
  `2025-03-26`, `2024-11-05`: none of them changed what tools exchange), else `2025-11-25`; server `templum`,
  Templum's version, and `instructions` written for the caller:

  > You are block b7k2m, "Claude Code · api", in workspace "Main". Connected to you: t4q8p (terminal "dev server"),
  > k9x1d (kanban "Sprint"). In this workspace, not connected (ask with connect): 4qm2a (note "Ideas"). Name blocks by
  > these ids. You may only read or write your own block and those connected to you; connect asks the person for
  > another. list_blocks gives the current list. When a connected kanban has your task, move its card to Doing when
  > you start and to Review when you are done; write what the next agent needs to know in the connected note.

  The kanban and note sentence comes with their tools (batch 4).

  Titles come from what runs in a block (a terminal's is whatever its programs set), so the instructions, `whoami`
  and `list_blocks` give them cleaned: no control characters, no invisible ones (the soft hyphen `U+00AD`, zero
  widths `U+200B`–`U+200F`, direction overrides `U+202A`–`U+202E` and `U+2066`–`U+2069`, the word joiner and
  invisible operators `U+2060`–`U+2064`, `U+FEFF`, and the tags `U+E0000`–`U+E007F`, which can spell hidden text),
  line breaks and runs of blanks as one space, at most 60 characters (cut with "…"), and the instructions put them in
  double quotes, a `"` inside escaped.
- **`tools/list`** is the same for every caller; which blocks a tool may touch is checked at each call.
- **Results** are text (`content: [{type: "text"}]`). A refusal or a failure is a result with `isError: true` and a
  sentence that says what to do next: `k9x1d isn't connected to you. Ask the person with connect("k9x1d").`
- **Outside Templum** (no `TEMPLUM_BUS` or `TEMPLUM_TERMINAL`): `templum: not inside a Templum terminal` on stderr,
  exit code 2, nothing on stdout. **Templum not running:** `templum: Templum isn't running`, exit code 2. **Templum
  quits** during a session: the bridge closes its stdout and exits 0; the agent reports the server as disconnected.
  **A terminal this Templum doesn't have:** `initialize` succeeds with instructions saying so, and every tool call
  fails.

### Tools

"Wired" means a wire between the caller and the block named; a caller may always use its own block.

| Tool | Arguments | Does | Needs | Batch |
|---|---|---|---|---|
| `whoami` | — | The caller's id, title, kind, folder and workspace | — | 1 |
| `list_blocks` | — | Every block of the caller's workspace: id, kind, title, folder, agent and its state, connected or not | — | 1 |
| `read_terminal` | `block`, `lines` (200, at most 2000), `last_command` (false) | The last lines of the terminal's history and screen, or its last finished command and its output (needs [shell integration](shell-integration.md)) | wired | 1 |
| `send_to_terminal` | `block`, `text`, `enter` (false) | Types the text, as a paste; Enter after it when asked | wired | 1 |
| `notify` | `text` | A notification from the caller ([Requests](#requests-from-agents)) | — | 1 |
| `connect` | `block`, `timeout` (1800 s) | Asks the person to wire the caller to the block; waits for the answer | — | 3 |
| `spawn_agent` | `agent` (an id of [agents.md](agents.md) or of `[agents]`), `prompt`, `folder`, `worktree` | [Spawning](#spawning) | — | 3 |
| `delegate` | `block`, `task`, `wait` (false), `timeout` (600 s) | [Delegating](#delegating) | wired, an agent | 3 |
| `reply` | `block`, `text` | Answers the task the block handed the caller; with none waiting, types `[reply from …]` and the text into it | wired | 3 |
| `wait_for_idle` | `block`, `timeout` (300 s) | Waits until the agent finishes its turn; its last answer | wired | 3 |
| `ask_user` | `question`, `options`, `timeout` (1800 s) | A question in a balloon ([Requests](#requests-from-agents)); the answer | — | 3 |
| `list_cards` | `board` | The wired kanban block's board: columns, and each card's id, title, labels, assignee and comment count | wired kanban | 4 |
| `create_card` | `board`, `column`, `title`, `description` | A new card at the column's end; its id | wired kanban | 4 |
| `move_card` | `card`, `column` | Moves a card to the end of a column | wired kanban | 4 |
| `comment_card` | `card`, `text` | Adds a comment signed with the caller's id | wired kanban | 4 |
| `claim_card` | `card` | Makes the caller the card's assignee; refused when another block holds it | wired kanban | 4 |
| `read_note` | `block` | The text of the note the note block shows | wired note | 4 |
| `append_note` | `block`, `text` | Adds the text at the note's end, after a blank line; the note shows it at once and Ctrl+Z takes it back | wired note | 4 |
| `open_file` | `path`, `line` | Opens the file at the line in the wired editor block whose folder holds it; with none, in a new editor block beside the caller, wired to it. Neither the keyboard nor the camera moves | — | 4 |
| `git_status` | `block` | The wired Git block's branch, ahead and behind, and changed files as `git status --short` shows them | wired Git | 4 |
| `wait_for` | `block`, `event` (`idle`, `needs_you`, `command_finished`, `attention`), `timeout` | Waits for the event in the block | wired | 5 |

- A block is named by its id. A **board** is named by its name or id, and may be left out when one wired kanban
  block shows one; a **column** by its title (either case) or id; a **card** by its id, as `list_cards` shows it.
- `read_terminal` reads the terminal's own history, never what is painted, so a terminal off screen reads as
  current. The result is at most 256 KB.
- Agents may stop waiting on long tool calls (Claude Code moves a call past 2 minutes to the background); the
  descriptions of the tools that wait say they may take minutes.
- A `timeout` is whole seconds from 1 to 3600 (Codex gives up on a tool call after an hour); a longer one is cut to
  3600.

## Hooks and agent state

Templum hears an agent's turns through its hooks. Each hook runs `templum hook <agent>`, which reads what the agent
hands it, turns it into an event, sends it to the bus with a one-second limit, and exits 0 at once, even when
Templum is gone.

| Agent | How Templum hears it | Events |
|---|---|---|
| Claude Code | hooks in the `--settings` Templum types (JSON on stdin) | started, working, done, waiting for the person, ended: the table below |
| Codex | `notify` in the `-c` flags Templum types (JSON as the last argument) | done with its last answer (`agent-turn-complete`, `last-assistant-message`) |
| Gemini CLI (batch 5) | `BeforeAgent`, `AfterAgent`, `Notification`, `SessionStart`, `SessionEnd` hooks | as Claude Code's |
| Others | — | none: [Delegating](#delegating) point 6 |

Claude Code's hooks:

| Hook | Event |
|---|---|
| `SessionStart` | started; nothing when its `source` is `compact`: compacting, automatically in the middle of a turn, leaves the agent working |
| `UserPromptSubmit` | working |
| `PermissionRequest` | waiting for the person, "Allow *tool*?" ("Claude asks you a question" for `AskUserQuestion`, "Claude has a plan for you" for `ExitPlanMode`), as soon as the permission dialog opens. The hook prints nothing and exits 0, so the dialog still decides |
| `PostToolUse` | working (again, after a permission) |
| `PostToolUseFailure` | working (again, after a permission: an allowed tool may fail), or done without an answer when `is_interrupt` is true (Esc while the tool ran) |
| `Stop` | done, with its last answer (`last_assistant_message`) |
| `StopFailure` | done, without an answer: an API error ended the turn, and `Stop` doesn't come |
| `Notification` | waiting for the person on `permission_prompt`, `elicitation_dialog` and `agent_needs_input` (they come only after about six seconds without typing, hence `PermissionRequest`); not working on `idle_prompt` (the prompt has sat idle): never a wait for the person, since the agent asks nothing; the others nothing |
| `SessionEnd` | ended |

Esc while a tool runs ends Working at once (`PostToolUseFailure` with `is_interrupt`). Esc while the model writes
fires no hook of its own: the dot stays on Working until the next prompt, or until `idle_prompt` comes after the
prompt has sat idle for Claude Code's own delay (about a minute). Deferred: [minors](../minors.md#connections).

Claude Code merges hooks from every settings source, so the person's own hooks still run. Codex keeps one `notify`:
see [Codex](#what-templum-types-into-each-agent) below.

## What Templum types into each agent

Only for an agent Templum opens (the canvas menu, `spawn_agent`, and resuming on restore), when `[connections]` is
on, there is a bus, the shell isn't WSL, and every path is typeable (`typed_path` of [usage.md](usage.md); when
one isn't, the agent starts without connections, and a notification says so once per session; the notification is
deferred: [minors](../minors.md#connections)). A `claude` typed by hand gets nothing.

**Claude Code:** `claude --settings <data>/claude-statusline.json --mcp-config <data>/mcp.json`. The settings file
keeps the status line of [usage.md](usage.md) when `usage.claude_limits` is on, and gains the hooks. The server
comes only with the hooks: when the settings file can't be given (Templum's program path can't be typed), neither is
`--mcp-config`, so no agent is connected with a dot that never moves.

```json
{
  "statusLine": { "type": "command", "command": "/opt/templum --claude-statusline /home/u/.local/share/templum", "padding": 0 },
  "hooks": {
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }],
    "PermissionRequest":[{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }],
    "PostToolUse":      [{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }],
    "PostToolUseFailure":[{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }],
    "StopFailure":      [{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }],
    "Notification":     [{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "/opt/templum hook claude", "timeout": 5 }] }]
  }
}
```

```json
{ "mcpServers": { "templum": { "type": "stdio", "command": "/opt/templum", "args": ["mcp"] } } }
```

**Codex:** five flags, in single quotes (so not under `cmd.exe`; not under Windows PowerShell 5.1 either, whose
legacy argument passing strips the `"` inside them, `args=["mcp"]` reaching Codex as `args=[mcp]`, while pwsh 7 is
fine; and not when a path has a `'`); `env_vars` because Codex hands a server only the variables it is told to:

```
codex -c 'mcp_servers.templum.command="/opt/templum"' -c 'mcp_servers.templum.args=["mcp"]' -c 'mcp_servers.templum.env_vars=["TEMPLUM_BUS","TEMPLUM_TERMINAL"]' -c 'mcp_servers.templum.tool_timeout_sec=3600' -c 'notify=["/opt/templum","hook","codex"]'
```

Codex keeps one `notify`, and a `-c` flag replaces the config's: the person's own `notify` from
`~/.codex/config.toml` doesn't run for a Codex Templum opens (Claude Code merges hooks; Codex doesn't). Deferred:
[minors](../minors.md#connections).

**Gemini CLI** (batch 5): it has no flag for MCP servers or hooks. Templum checks whether a system settings file named
by `GEMINI_CLI_SYSTEM_SETTINGS_PATH` adds to the person's settings or replaces them; if it adds, Templum types it;
otherwise **Settings ▸ Connections** offers to run `gemini mcp add -s user templum <templum> mcp` once, with the
person's consent.

**Others** (Grok Build, Pi, OpenCode, the person's own): **Settings ▸ Connections** shows the server's command to
copy into their configuration. They know their block anyway: the server inherits `TEMPLUM_TERMINAL`.

**Templum's program** is the AppImage when `$APPIMAGE` is set (its mounted executable goes away when it quits),
otherwise the running executable, without the ` (deleted)` an upgrade leaves on Linux. The status line uses it too.

## The `templum` command line

The same program, without a window, when its first argument is one of these verbs. Each verb is one tool call: it
prints the result's text, or with `--json` the whole result; a refusal goes to stderr with exit code 1; outside
Templum or with Templum gone, exit code 2 as [above](#the-mcp-server).

| Command | Tool | Batch |
|---|---|---|
| `templum whoami` | `whoami` | 1 |
| `templum blocks` | `list_blocks` | 1 |
| `templum read <block> [-n <lines>] [--last]` | `read_terminal` | 1 |
| `templum send <block> <text> [--enter]` | `send_to_terminal` | 1 |
| `templum notify <text>` | `notify` | 1 |
| `templum mcp` | the MCP bridge | 1 |
| `templum hook claude` / `templum hook codex <json>` | an agent's hook | 1 |
| `templum connect <block>` | `connect` | 3 |
| `templum spawn <agent> [--folder <f>] [--worktree <name>] [--prompt <text>]` | `spawn_agent` | 3 |
| `templum delegate <block> <task> [--wait]` | `delegate` | 3 |
| `templum reply <block> <text>` | `reply` | 3 |
| `templum wait <block> [--for <event>] [--timeout <s>]` | `wait_for_idle`, `wait_for` | 3, 5 |
| `templum ask <question> [--option <o>]…` | `ask_user` | 3 |
| `templum kanban list\|add\|move\|comment\|claim …` | the kanban tools | 4 |
| `templum note read\|append <block> [text]` | `read_note`, `append_note` | 4 |
| `templum open <path>[:<line>]` | `open_file` | 4 |
| `templum git status [<block>]` | `git_status` | 4 |
| `templum wire <a> <b>` | wires two blocks (only from a terminal of the same workspace) | 5 |

A verb stays connected until its answer: stopped with Ctrl+C, what it waited for (a balloon) is withdrawn.

Any other first argument opens Templum, as today. On Windows the release program has no console, so the verbs'
output reaches a pipe (`templum blocks | more`) but not a console directly.

## Config

```toml
[connections]
## Templum's MCP server and hooks are given to the Claude Code and Codex it opens.
# enabled = true
```

Off, nothing is typed into agents; the bus still serves the command line. **Settings ▸ Connections** (batch 5) has
the switch, the server's command for other agents, and Gemini CLI's setup.

## Files

- **Workspace files** ([persistence.md](persistence.md#the-file)): every item gains its block id, and the canvas its
  wires, by block id. A wire whose block didn't come back, one of a block to itself, and a second one between the
  same two blocks are dropped. An older Templum ignores both.

  ```json
  { "version": 1,
    "items": [{ "kind": "terminal", "uid": "b7k2m", "rect": {…}, … }, { "kind": "note", "uid": "4qm2a", … }],
    "wires": [{ "a": "b7k2m", "b": "4qm2a" }, { "a": "b7k2m", "b": "t4q8p", "label": "dev server" }] }
  ```

- **`boards.json`** (batch 4, [kanban.md](kanban.md#files)): a card gains `assignee` (a block id) and `comments`, each
  `{ "by": "<block id or 'you'>", "text": "…", "at": <unix seconds> }`. An older file loads with none; an older
  Templum ignores them. The card shows the assignee's block title and the comment count; its dialog lists the
  comments and adds the person's.
- **`<data>/mcp.json`** and the hooks in `<data>/claude-statusline.json` are written when an agent is opened, only
  when they changed, atomically.

## Architecture

| File | Change | Batch |
|---|---|---|
| `templum-mcp` (new crate, no UI) | `rpc.rs` (the line protocol), `tools.rs` (the tools' schemas, the typed calls, replies), `hooks.rs` (each agent's events), `bus.rs` (the socket: listener, a reader and a writer thread per connection, the client), `cli.rs` (the verbs, the MCP bridge), `lib.rs` (the `-c` flags Codex is given) | 1 |
| `templum-terminal/src/terminal.rs` | `SpawnOptions.env`; `Terminal::lines` and `last_command`, read from the grid | 1 |
| `templum-usage/src/claude.rs` | the settings file Claude Code is given holds the status line, the hooks, or both; `mcp.json`, the MCP config it is given | 1 |
| `templum/src/main.rs` | the verbs run before any window | 1 |
| `templum/src/workspace/bus.rs` (new) | the bus's messages answered on the main thread: the caller, the tools, the events, the requests that wait | 1, 3–5 |
| `templum/src/workspace/usage.rs` | what `typed()` adds for Claude Code and Codex | 1 |
| `templum/src/session.rs`, `workspace/persist.rs`, `workspace/blocks.rs`, `workspace/spaces.rs` | block ids: given, saved, restored, moved | 1 |
| `templum/src/block.rs` | the agent's state in the status dot; the balloon | 1, 3 |
| `templum/src/config.rs` | `[connections]` | 1 |
| `templum-canvas/src/wire.rs` (new) | `Wire`; the curve between two rects, a point on it, hitting it | 2 |
| `templum/src/workspace/wires.rs` (new), `templum/src/wires.rs` (new) | wiring, unwiring, selecting, undo; painting wires and the dot | 2 |
| `templum/src/workspace/waits.rs`, `delegate.rs`, `spawn.rs` (new) | the requests that wait and the balloon's answers; tasks handed over, `reply`, `wait_for_idle`; `spawn_agent` | 3 |
| `templum/src/balloon.rs` (new) | the balloon | 3 |
| `templum-git/src/repo.rs` | `Repo::add_worktree` | 3 |
| `templum/src/toolbar.rs`, `menu/`, `workspace/history.rs`, `workspace/input.rs`, `workspace/render.rs` | the Wire tool, the menus, the undo steps, the pointer | 2 |
| `templum/src/board.rs`, `kanban.rs`, `notes.rs`, `workspace/note_tabs.rs`, `workspace/files.rs` | comments, assignee and claiming; appending to a note; opening a file without focus | 4 |
| `templum/src/settings/` | Settings ▸ Connections | 5 |

Every request reaches the main thread through a channel the bus's threads write to, drained by a task as the
terminals' wake-ups are; the main thread never touches a socket. A request that waits keeps its reply channel in
`Workspace::waits` until an event, a button, its block's closing (checked each frame), its connection ending or its
cancel (`notifications/cancelled`), or its deadline (a timer: with nothing on screen moving, no frame comes) answers or
drops it.
No new dependency: `futures`, `serde`, `serde_json` and `uuid` are in the tree.

## Testing

**Unit (`templum-mcp`):** a request and a notification parse; bad JSON answers `-32700` with a null id; every line
written ends in one newline; every tool parses its own example arguments and names a missing one; an unknown tool fails;
`initialize`'s result; each Claude Code hook payload and Codex's `notify` become their event (`PermissionRequest` waits
for the person, `PostToolUseFailure` works on unless it is an interrupt, which is done, `StopFailure` and `idle_prompt`
are done, a `compact` start is nothing), unknown ones none; the bus: bind, hello, a request answered; a wrong first line
or token closes the connection, as does a hello too long or too slow, a byte at a time included; a client quiet
for longer than the hello's time after it is still answered; two requests on one connection both answered; the
socket file goes away with the bus and a stale one is replaced; a data folder too long for a socket puts it in a private
(0700) temporary folder that goes with the bus; the bridge sends its hello first and copies both ways; `hook claude`
sends one event and prints nothing; `read <block> -n 10` sends its call and prints the text; no bus is exit code 2 with
nothing on stdout. Every read and wait in these tests has a 5-second limit.

**Unit (`templum-terminal`):** `lines` reads history past the screen; `last_command` is the last finished command and
its output; `SpawnOptions.env` reaches the shell.

**Unit (`templum-canvas`, batch 2):** a wire's ends sit on the facing edges; a point on it hits, one 50 units off
doesn't; wires are symmetric.

**Headless UI:** a terminal's `TEMPLUM_BUS`; `whoami` names the caller's id and workspace, also over the real socket;
`read_terminal` of the caller sees what it printed; `send_to_terminal` with Enter runs; another block is refused until
wired; `list_blocks` says connected or not; titles reach agents cleaned and quoted; a `Stopped` event leaves the agent
idle and a `NeedsYou` one shows the yellow dot; a crashed agent's state is forgotten; `typed()` adds the flags for
Claude Code and Codex, none for Codex under Windows PowerShell 5.1, none when `[connections]` is off or there is no data
folder, and Claude Code's server only with its hooks; a block id survives saving, restoring, closing and undoing, and
moving to another workspace, and a pasted block gets a new one; (batch 2) the Wire tool and the menu wire two blocks,
undo takes it back, a selected wire is deleted with Delete, closing a block and undoing brings its wires back, wires
come back after a restart and a wire to a missing block is dropped; (batch 3) `delegate` types the header and the task
and Enter, `wait` ends with the `Stopped` message or its timeout, `reply` answers it, the balloon answers `ask_user`,
Allow wires and Deny refuses, `spawn_agent` opens a wired block beside the caller and types its prompt once it started,
closing a block answers what waited on it, the dot travels only while a task is handed over; (batch 4) comments and
assignee survive a round trip and an older file loads, `claim_card` refuses a card another block holds, `append_note`
shows in the open note, `open_file` lands at the line in the wired editor without taking the keyboard, `git_status`
matches the repository, and every tool is refused without a wire.

**Manual (Hyprland):** Claude Code from the menu: `/mcp` lists `templum` connected, "list the blocks" and "read the
last 30 lines of the dev server" work, the dot follows its turns and a permission prompt; Codex: `/mcp` lists it;
`templum blocks` in a plain terminal; `templum mcp` in another terminal emulator exits 2; wiring by tool and menu,
restart; Claude Code delegating to Codex and getting its answer; a question and a connect request in the balloon;
`spawn_agent` with a worktree.

## Delivery

One pull request per batch, each with its own plan in `docs/plans/`:

1. **The bus:** `templum-mcp`, the block id, `TEMPLUM_BUS`, `whoami`, `list_blocks`, `read_terminal` and
   `send_to_terminal` (the caller's own block until batch 2), `notify`, the hooks and the agent's state, what is
   typed into Claude Code and Codex, `[connections]`, the verbs of batch 1.
2. **Wires:** the model, painting, the Wire tool, the menus, selecting and deleting, undo, saving; tools check wires.
3. **Agents together:** `connect`, `spawn_agent` with worktrees, `delegate`, `reply`, `wait_for_idle`, `ask_user`,
   the balloon, the travelling dot.
4. **Content:** the kanban tools with comments, assignee and claiming; notes; `open_file`; `git_status`.
5. **The rest:** `wait_for`, wire labels, Settings ▸ Connections, Gemini CLI, `templum wire`, `--json` everywhere.

Batches 2 and 3 paint, so their pull requests carry `--bench` and `--bench-mixed`.

## Risks

| Risk | What we do |
|---|---|
| Claude Code moves a tool call past 2 minutes to the background, and stops waiting for it | The tools that wait say so; a cancelled call (`notifications/cancelled`) drops its request (tested); that a late result reaches the agent is checked by hand |
| Typing into an agent that is busy | Refused while it waits for its person; while it works, Claude Code and Codex queue typed input (checked by hand in batch 3); the `[from …]` line keeps it readable |
| A spawned agent's prompt typed while it asks a first-run question (Codex trusting a new folder) | Claude Code's waits for its `SessionStart`, after that question; the others follow the rule above (process seen, screen still): checked by hand |
| A task typed into a Codex at work is answered by the turn it was on | One task at a time per agent; the rest is the person's to see in the agent's terminal |
| An agent without events never says it is done | `reply`, or the wait's timeout with the last screen lines; batch 5 may add "the screen has been still for N seconds" |
| Gemini CLI can't be given an MCP server for one session | The system settings file if it adds to the person's; else a one-time `gemini mcp add` the person agrees to |
| Codex's `-c` under `cmd.exe` and Windows PowerShell 5.1 | Not typed there (Git Bash is the default shell on Windows; pwsh 7 takes them) |
| A path Templum can't type safely (`"`, `$`, `'`, a backtick…) | No connections for that agent, and a notification once per session (deferred) |
| macOS's 104-byte socket path | A private folder in the temporary folder past 100 bytes |
| The AppImage's executable goes away when it quits | `$APPIMAGE` is the program typed |
| Reading history while the terminal's thread parses | The unfair lock waits only for the parse under way, as painting does; at most 2000 lines and 256 KB |
| A hook slows the agent when Templum is gone | One-second connect limit, exit 0; each hook has a 5-second timeout |
| Painting wires costs frames | One curve per wire per frame, below the blocks; cached per zoom step as drawings are if the benches ask for it |
| A process of the same user names another terminal | The documented trust: it could already type into it |
