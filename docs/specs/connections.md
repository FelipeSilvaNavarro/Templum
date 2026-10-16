# Connections: blocks that talk to each other

- **Date:** 2026-10-15
- **Status:** Batches 1 to 6 implemented; 7 and 8 (orchestration) specified
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
9. Claude Code runs another agent as its own subagents run (batches 6 to 8): it picks the agent's model and effort,
   hands it a task and keeps working, hears its end typed back with its time and tokens, changes the task while it
   runs, answers its dialogs with real keys, stops it, and closes the blocks it opened.

## Out of scope

- Editing an agent's own settings files (`~/.claude/settings.json`, `~/.codex/config.toml`, Gemini's): Templum
  passes everything on the command line it types, for that session only. Gemini CLI gets an extension instead, below,
  only after the person agrees, and never through its settings file.
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
- **Labels:** **Rename wire…** in a wire's menu names it (blank removes the name; it is put on one line and cut at
  60 characters), a step of Ctrl+Z. The label sits in a pill at the curve's middle, in the wire's color, scaled with
  the canvas at the Full level and at its own size at the Silhouette level, as a block's title is; grid mode hides it.
  It comes back with its wire: Ctrl+Z, a closed block brought back, a restart.

### An agent's state

A terminal running an agent Templum hears from ([Hooks](#hooks-and-agent-state)) shows it in its status dot:

| State | Dot | Tooltip |
|---|---|---|
| Working on a prompt | accent | "Working" |
| Waiting for the person (a permission dialog, a question) | yellow | "Waiting for you: *the agent's message*" ("Allow Bash?" as a permission dialog opens) |
| Done with its turn | none (its running command would otherwise read "Running") | — |

**From its screen.** Where its hooks say nothing, its screen tells: once a second Templum looks at the last 100 lines of
every agent's terminal, and each event of its hooks starts the looks over (the spinner that ran until its turn ended or
a dialog opened says nothing after). One that changed at 3 of the last 4 looks (a spinner, Codex's "Working (5s)") is an
agent at work. An agent whose hooks spoke and whose screen stayed still for 30 looks while it worked **seems stuck**
(batch 7): yellow, "Waiting for you: it seems stuck, its screen still for 30 seconds" (a dialog no hook tells of, as a
trust question or Codex's choice of model, looks like this; Claude Code's spinner and Codex's "Working (5s)" move every
second while they work); its task stays handed over, `wait_for(needs_you)` answers, the agent that handed it hears so
([Delegating](#delegating) 10), and its screen moving again turns it back to Working. An agent without hooks, or one the
person typed into since its last hook (an Esc while Claude Code writes, which no hook tells of), whose screen stays
still for 5 looks while it works is at rest instead, and the task it was on ends with its last 40 screen lines. A wait
for the person ends from the screen only once the person typed into the terminal since it began (they answered the
dialog): a subagent drawing under an open dialog doesn't end it, nor does a still screen. A look right after the person
typed, pasted, clicked or scrolled in it, or its block was resized (its agent redraws at the new size), tells nothing,
and the looks before it count no more. A spawned agent whose prompt still waits isn't looked at: its screen is its start
and its first questions, which [Spawning](#spawning) watches, and the minute there ends its task.

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
2. Templum types into Codex's terminal a first line, `[from Claude Code · api (b7k2m)] Your task is in the pasted text:
   do it.`, as a paste of its own, then the task as a second paste (batch 7; before, both as one: Claude Code takes a
   paste of 4 lines or more than 800 characters for data it was shown, not for what it is asked, unless typed text comes
   with it), then Enter, a moment (150 ms) after the paste: an agent may take keys that come in one burst for a paste,
   where Enter is a new line. The Enter goes only once the agent's screen shows the paste (Templum looks for about 2
   seconds), and only if Templum still may type into it (5): a dialog that ignores pastes, such as Codex's approval,
   which Templum hears nothing of, or a permission prompt that opened meanwhile, gets no Enter that would answer it. The
   task then ends at once: the delegating agent is told the agent didn't take it, and to look at its block. The person
   sees it arrive. A resize of the agent's block meanwhile redraws it at its new size, which shows nothing of the paste:
   its Enter waits until its screen stays the same for two looks. An agent whose hooks say when a turn starts (Claude
   Code, Gemini CLI, Codex once its hooks spoke) then gets it, as one whose dialog would be heard; for another that
   settled screen is the new start, and only a change after it is the paste.
   **Confirmed** (batch 7): an agent whose hooks say when it takes a prompt (Claude Code's `UserPromptSubmit`, Codex's,
   Gemini CLI's `BeforeAgent`) must take the task within 3 seconds of its Enter, or open a dialog; one at work when the
   task was typed queued it (Codex says it took it only once a turn takes it in, maybe minutes later). Nothing else
   confirms it: not a turn's end, a notification (Claude Code's `idle_prompt`, a minute after a turn), nor a moving
   screen. With no word, it gets its Enter once more (the first may have joined the paste as a new line), only while
   Templum still may type into it, its screen hasn't changed since 150 ms after the first (a dialog no hook tells of
   would have drawn) and the person hasn't typed into it for 3 seconds; otherwise, or with still no word 3 seconds
   later, the task ends untaken, with the last 20 lines of its screen. A turn that ends before the agent took the task
   (the turn it was on) doesn't answer it. The agents without hooks, and a Codex whose hooks haven't spoken yet (its
   `SessionStart` comes with its first prompt), get no confirmation: a second Enter, blind, could answer a dialog
   Templum doesn't hear.
3. The wire's dot travels from Claude Code to Codex while Codex works.
4. When Codex finishes its turn, its last answer becomes the result of Claude Code's `delegate` (when it asked to
   wait), or of a later `wait_for_idle`. If Codex calls `reply` first, that text is the answer instead. Each task
   has an id (`t7`, batch 8), which `delegate` and `spawn_agent` say; `delegate` without `wait` answers at once:
   "Handed to k9x1d as task t7."
5. Templum refuses to type into an agent that is waiting for its person (a permission prompt would take the text
   as its answer; the refusal says what it asks and gives the last 20 lines of its screen, the dialog as drawn, so
   the caller can answer it with `send_keys`), into a terminal running no agent (`send_to_terminal` is for those),
   into an agent no snapshot has found yet (one Templum just typed may not have started at all; the caller is told
   to try again in a few seconds), and into an agent already on another agent's task (`wait_for_idle` waits for its
   end; the agent whose task it is changes it instead, 11).
6. An agent Templum gets no events from (see the table under [Hooks](#hooks-and-agent-state)) answers through
   `reply`, or its task ends once its screen has been still for 5 seconds after working, with its last screen lines,
   or at its timeout.
7. A task stays handed over until its answer comes, the agent's session ends (or a snapshot no longer finds the agent
   it found before: Codex says nothing when it quits), its block closes, or its timeout passes (`delegate`'s, 600 s
   unless asked, also without `wait`). Past its timeout the agent that handed it over still waits for its answer
   with `wait_for_idle`, and hears that it timed out (10); the wire's dot stops, and another agent may hand it a
   task.
8. `reply` with no call waiting types `[reply from <title> (<id>)]` and the text, then Enter, into the agent named;
   never into a terminal running no agent, or an agent no snapshot has found yet. From batch 7 the first line goes as
   a task's does (2): `[reply from <title> (<id>)] Its reply is in the pasted text.`, then the text. From batch 8 the
   line names the task: `[reply from <title> (<id>) to task t7]`.
9. A task typed into an agent at work on a prompt of its own is answered by the end of the turn that took it
   (batch 7, once its hooks say when it takes a prompt). Codex takes it into the turn under way once a tool ends or
   its answer is written, and says so then, in that turn. Claude Code says so at its Enter and queues it: a tool's
   end takes it into the turn under way, whose end answers it; a turn that ends first, or an Esc, leaves it to the
   next turn, which no hook says starts, and that turn's end answers it.
10. **Its end comes back** (batch 8). When a task ends and no call of the agent that handed it over waits for it
    (`delegate` with `wait`, `wait_for_idle`), Templum types into that agent, as a reply is typed:
    `[task t7 from Codex · api (k9x1d): done in 31 s, 60231 tokens, transcript /…/rollout.jsonl]` and its answer
    on the lines after; `failed` instead of `done`, then why: it didn't take the task, its agent left, its block
    closed, or it gave no answer within its timeout (it may still work: a later answer comes too). Tokens are its
    session's since the task was handed over, as Templum reads them for its usage chip (Claude Code with its status
    line, Codex); none for the others. A task whose agent starts waiting for its person, or seems stuck, sends
    `[task t7 from Codex · api (k9x1d) waits for its person: Allow Bash?]` and the last 20 lines of its screen, once
    per wait. While the asker waits for its person, its agent is starting, or its person typed into it in the last
    3 seconds, a notice waits, oldest first. Into a terminal whose agent is gone (its shell is in front, as when
    `templum delegate` ran from a script), never: the notice is dropped.
11. **A change** (batch 8). `delegate` from the agent whose task it is changes that task: typed at once under
    `[from Claude Code · api (b7k2m), a change to task t7]`, even while it works, as Claude Code's queued messages
    are; the task's answer is the end of the turn that took the change. Any other agent is still refused.
12. `interrupt` by the agent that handed the task over ends it without a notice ([Keys](#keys-stopping-and-closing)).

### Spawning

`spawn_agent` opens a new agent block beside the caller, wired to it, in the caller's folder or the one it names.
The block goes where it covers no block nor drawing, beside the caller or a block wired to it: of the spots right of,
below, left of or above them (at their edge, or past what is in the way along that side), the one nearest the caller
and its connections all together, so they draw it their way. Any block Templum opens beside another (a terminal from
an editor's tree, a Git block, `open_file`'s editor) covers nothing either.
With a worktree name, Templum first runs `git worktree add -b <name> <repository>/.worktrees/<name>` from that
folder's repository (and adds `.worktrees/` to the repository's `.git/info/exclude` once), and the agent starts
there, so several agents can change one project at once without stepping on each other. A prompt is typed once the
agent is ready: its process is seen and its screen has been still for a second; from batch 7, Claude Code and Codex
also take typing then: they bracket pastes and show their cursor, which both hide in the questions they ask first
(trusting the folder; Codex's "Hooks need review"), where the prompt's Enter would answer for the person (on
Windows, whose console may not pass those on, the still screen alone). Neither one's session start tells: Codex's
`SessionStart` comes only with its first prompt, and Claude Code's a moment before its input box is ready, its
bracketed paste off and on again. A prompt not typed within a minute of the spawn (its person hasn't answered) is
dropped, and its task ends with the last 20 lines of its screen. From batch 7 it is typed under a first line
`[from <caller>] Your task is in the pasted text: do it.`, as a task is ([Delegating](#delegating) 2). The result
names the new block and its folder.

`spawn_agent` takes a `model` and an `effort` (batch 6), typed after the agent's command and before what Templum
adds (Claude Code's `--mcp-config` takes every word after it):

| Agent | `model` | `effort` |
|---|---|---|
| Claude Code | `--model <m>` | `--effort <e>`: `low`, `medium`, `high`, `xhigh`, `max` |
| Codex | `-m <m>` | `-c model_reasoning_effort=<e>`: `low`, `medium`, `high`, `xhigh`, `max`, `ultra` (Codex 0.154's models; `ultra` only some of them), unquoted: Codex reads a word that isn't TOML as a string |
| Gemini CLI | `-m <m>` | — |

They are typed into a shell: a model is 1 to 64 letters, digits, `.`, `-`, `_`, `:` and `/`, beginning with a letter
or a digit (no option can hide in it); an effort is one of its agent's. Anything else, an effort for Gemini CLI, or
either for another agent is refused before anything opens, a worktree included. The block keeps them: a restart, or
Ctrl+Z after a close, resumes its agent with them while that agent runs there. A snapshot that finds that agent gone,
or another agent there, drops them (also from the file it writes then): they remember the agent they were typed for, so
one typed by hand where a spawned agent never started doesn't get them. Templum's agents get them also when the person
changed their command. The result names them ("…, connected to you, on gpt-5.6-luna (high).").

The keyboard, the selection and the camera stay where they were; one Ctrl+Z closes the new block and its wire. The
worktree comes from the repository of the folder the agent starts in, always under its main working tree's
`.worktrees/`, on a new branch from that folder's HEAD; a name other than letters, digits, `-`, `_`, `.` and `/`, or one
already a branch, is refused. The prompt is a task the caller handed over: the dot travels, `wait_for_idle` waits for
its answer, and a prompt the agent doesn't take ends at once, as a delegated task does. An agent no snapshot has found a
minute after the spawn never started (its folder's trust declined, a question of its shell took the command): its prompt
is dropped and its task ends, as when an agent leaves.

### Keys, stopping and closing

Batch 6.

- **`send_keys`** presses keys in a terminal wired to the caller, one at a time and 50 ms apart (an Esc and the key
  after it in one burst read as Alt+key): `enter`, `escape`, `tab`, `shift+tab`, `backspace`, `delete`, `up`,
  `down`, `left`, `right`, `home`, `end`, `page_up`, `page_down`, `space`, `ctrl+<letter>`, or one printable
  character (`2`, `y`). At most 16 a call; a name it doesn't know refuses the call before any key goes. A key goes as
  the program asked for keys (application cursor keys, the kitty keyboard), a character as typed, never as a paste:
  a dialog takes `2` as its option, where a pasted `2` lands in the agent's input. The result ends with the last 20
  lines of the screen, 300 ms after the last key.
- **Dialogs.** Keys go into an agent waiting for its person too: the wire is the person's consent that the agents
  on it answer each other's dialogs. Keys from another block count as the person's keys for its state ([An agent's
  state](#an-agents-state)), and the block shows **answered by** *Claude Code (b7k2m)* (the caller as its `[from …]`
  line names it) after its dot for 10 seconds, in place of its usage chip, and in its dot's tooltip, whatever it was
  waiting for: a trust question no hook tells of is no wait Templum knows. Refused: the caller's own block, and an
  agent on another agent's task unless it waits for its person.
- **`interrupt`** presses Esc once in an agent wired to the caller while it works or waits for its person (a second
  Esc at Claude Code's empty prompt opens its rewind), and ends the task the caller handed it (a spawned agent's
  prompt still waiting is dropped with it): its waits get "Interrupted by you." Refused while it is idle, and while
  it is on another agent's task. Its result ends with the screen, as `send_keys`'s.
- **`close_block`** closes a block the caller opened with `spawn_agent` and is still wired to (unwiring it takes
  the consent back), without asking the person, as one step of Ctrl+Z; its agent ends at once, with no fade (one in a
  workspace not shown would wait for a frame). Ctrl+Z brings it back as any closed block, its agent resumed on the
  session the last snapshot found it on (not the folder's latest, the caller's when they share it), with its model.
  Any other: "Only the person closes k9x1d: you didn't open it." One it opened but is no longer wired to is refused
  as any block not connected: connect asks the person again.
- `send_to_terminal`'s Enter goes 150 ms after its text, as a task's does, and its answer after its Enter: a caller
  that waits for each answer (a script's `templum send … --enter` twice) gets its texts and Enters in order. A block
  closed meanwhile gets no Enter, and the answer says so.

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

  > You are block b7k2m, "Claude Code · api", in Templum workspace "Main". Connected to you: t4q8p (terminal "dev
  > server"), k9x1d (kanban "Sprint"). In this workspace, not connected (ask with connect): 4qm2a (note "Ideas"). Name
  > blocks by these ids. You may only read or write your own block and those connected to you; connect asks the person
  > for another. list_blocks gives the current list. When a connected kanban has your task, move its card to Doing
  > when you start and to Review when you are done (Done when the board has no Review); write what the next agent
  > needs to know in the connected note.

  The kanban and note sentence comes with their tools (batch 4). From batch 8 they add: "A task you hand over
  without wait comes back typed into your terminal as [task …]: it isn't the person writing."

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
| `list_blocks` | — | Every block of the caller's workspace: id, kind, title, folder, agent and its state (from batch 7 with what it asks), connected or not | — | 1 |
| `read_terminal` | `block`, `lines` (200, at most 2000), `last_command` (false) | The last lines of the terminal's history and screen, or its last finished command and its output (needs [shell integration](shell-integration.md)) | wired | 1 |
| `send_to_terminal` | `block`, `text`, `enter` (false) | Types the text, as a paste; Enter after it when asked | wired | 1 |
| `notify` | `text` | A notification from the caller ([Requests](#requests-from-agents)) | — | 1 |
| `connect` | `block`, `timeout` (1800 s) | Asks the person to wire the caller to the block; waits for the answer | — | 3 |
| `spawn_agent` | `agent` (an id of [agents.md](agents.md) or of `[agents]`), `prompt`, `folder`, `worktree`, `model`, `effort` | [Spawning](#spawning) | — | 3, 6 |
| `delegate` | `block`, `task`, `wait` (false), `timeout` (600 s) | [Delegating](#delegating); without `wait`, says the task's id at once, and its end is typed into the caller's terminal | wired, an agent | 3, 8 |
| `reply` | `block`, `text` | Answers the task the block handed the caller; with none waiting, types `[reply from …]` and the text into it | wired | 3 |
| `wait_for_idle` | `block`, `timeout` (300 s) | Waits until the agent finishes its turn, and for the answer to a task the caller handed it, past `delegate`'s timeout too; its last answer | wired | 3 |
| `ask_user` | `question`, `options`, `timeout` (1800 s) | A question in a balloon ([Requests](#requests-from-agents)); the answer | — | 3 |
| `list_cards` | `board`, `card` | The wired kanban block's board: columns, and each card's id, title, labels, assignee and comment count; with `card`, that card whole: its description and comments | wired kanban | 4 |
| `create_card` | `board`, `column`, `title`, `description` | A new card at the column's end (the board's first column when left out); its id | wired kanban | 4 |
| `move_card` | `card`, `column` | Moves a card to the end of a column | wired kanban | 4 |
| `comment_card` | `card`, `text` | Adds a comment signed with the caller's id | wired kanban | 4 |
| `claim_card` | `card` | Makes the caller the card's assignee; refused when another block holds it | wired kanban | 4 |
| `read_note` | `block` | The text of the note the note block shows | wired note | 4 |
| `append_note` | `block`, `text` | Adds the text at the note's end, after a blank line; the note shows it at once and Ctrl+Z takes it back | wired note | 4 |
| `open_file` | `path`, `line` | Opens the file at the line in the wired editor block whose folder holds it; with none, in a new editor block beside the caller, wired to it. Neither the keyboard nor the camera moves | — | 4 |
| `git_status` | `block` (may be left out when one Git block is wired) | The wired Git block's branch, ahead and behind, and changed files as `git status --short --branch` prints them | wired Git | 4 |
| `wait_for` | `block`, `event` (`idle`, `needs_you`, `command_finished`, `attention`), `timeout` | Waits for the event in the block: `idle` as `wait_for_idle`; `needs_you`, its agent or its balloon asking the person (at once when it already does), with what it asks and, from batch 7, the last 20 lines of its screen; `command_finished`, the next command to finish there, with its output (needs [shell integration](shell-integration.md)); `attention`, its next bell or notification | wired | 5 |
| `send_keys` | `block`, `keys` (1 to 16) | Presses the keys ([Keys](#keys-stopping-and-closing)); the screen after them | wired | 6 |
| `interrupt` | `block` | Esc in its agent; ends the task the caller handed it | wired, an agent | 6 |
| `close_block` | `block` | Closes a block the caller opened | wired, opened by the caller | 6 |

- A block is named by its id. A **board** is named by its name or id, and may be left out when one wired kanban
  block shows one; a **column** by its title (either case) or id; a **card** by its id, as `list_cards` shows it.
  `board`, `column` and `card` may be numbers or strings; a blank title, text or comment is refused.
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
| Codex | hooks in the `-c` flags Templum types (JSON on stdin), once its person trusted them (batch 7; before, `notify`: done with its last answer) | the table below |
| Gemini CLI | the hooks of Templum's extension (below), JSON on stdin | `SessionStart` started, `BeforeAgent` took a prompt: working, `AfterTool` working again after a permission, `AfterAgent` done with `prompt_response`, `Notification` (`ToolPermission`) waiting for the person, `SessionEnd` ended |
| Others | — | none: their screen ([An agent's state](#an-agents-state)), and [Delegating](#delegating) point 6 |

Claude Code's hooks:

| Hook | Event |
|---|---|
| `SessionStart` | started; nothing when its `source` is `compact`: compacting, automatically in the middle of a turn, leaves the agent working |
| `UserPromptSubmit` | took a prompt: working (what confirms a task's Enter, [Delegating](#delegating) 2). It comes at the Enter, also for a prompt Claude Code queues behind the turn under way, with that turn's `prompt_id` (batch 7's spikes) |
| `PermissionRequest` | waiting for the person, "Allow *tool*?" ("Claude asks you a question" for `AskUserQuestion`, "Claude has a plan for you" for `ExitPlanMode`), as soon as the permission dialog opens. The hook prints nothing and exits 0, so the dialog still decides |
| `PostToolUse` | working (again, after a permission); from batch 7 never a new turn: only a prompt taken starts one |
| `PostToolUseFailure` | working (again, after a permission: an allowed tool may fail), or done without an answer when `is_interrupt` is true (Esc while the tool ran) |
| `Stop` | done, with its last answer (`last_assistant_message`) |
| `StopFailure` | done, without an answer: an API error ended the turn, and `Stop` doesn't come |
| `Notification` | waiting for the person on `permission_prompt`, `elicitation_dialog` and `agent_needs_input` (they come only after about six seconds without typing, hence `PermissionRequest`); `permission_prompt` says the dialog `PermissionRequest` already did, so its "Allow *tool*?" stays; not working on `idle_prompt` (the prompt has sat idle): never a wait for the person, since the agent asks nothing; the others nothing |
| `SessionEnd` | ended |

Esc while a tool runs ends Working at once (`PostToolUseFailure` with `is_interrupt`). Esc while the model writes
fires no hook of its own: the dot stays on Working until its screen has been still for 5 seconds
([An agent's state](#an-agents-state)).
A Templum tool's call works again at once after its own "Allow mcp__templum__*tool*?": the call arriving says the
person allowed it, and `delegate` or `wait_for_idle` may take minutes before their `PostToolUse`.

Codex's hooks (batch 7; checked with Codex 0.154: JSON on stdin with `hook_event_name`, `session_id`, `turn_id`
but for `SessionStart`, `transcript_path`, `cwd`, `model` and `permission_mode`):

| Hook | Event |
|---|---|
| `SessionStart` | started; nothing when its `source` is `compact`. It comes with the first prompt, just before its `UserPromptSubmit`, not when Codex opens |
| `UserPromptSubmit` | took a prompt: working. One typed while Codex works (Enter) comes once the turn under way takes it in, after a tool or once its answer is written, with that turn's `turn_id`; one queued with Tab, as the next turn |
| `PermissionRequest` | waiting for the person, "Allow *tool*?" (`tool_name`: `Bash` for a command), as its dialog opens; the hook prints nothing, so the dialog still decides |
| `PostToolUse` | working (again, after a permission); never a new turn: a command an interrupt left running ends later with its own |
| `Stop` | done, with its last answer (`last_assistant_message`) |
| `Interrupt` | done, without an answer: Esc or Ctrl+C stopped the turn, and no `Stop` comes. Codex gives it 3 s at most |

Codex's `SessionEnd` isn't followed (Codex also sends it after 30 idle minutes): a snapshot tells when Codex quits.

Claude Code merges hooks from every settings source, so the person's own hooks still run; Codex merges those of its
`-c` flags with the person's `hooks.json` too (its `/hooks` lists both, each active).

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

**Codex:** five flags until batch 7, in single quotes (so not under `cmd.exe`; not under Windows PowerShell 5.1
either, whose legacy argument passing strips the `"` inside them, `args=["mcp"]` reaching Codex as `args=[mcp]`, while
pwsh 7 is fine; and not when a path has a `'`); `env_vars` because Codex hands a server only the variables it is told
to:

```
codex -c 'mcp_servers.templum.command="/opt/templum"' -c 'mcp_servers.templum.args=["mcp"]' -c 'mcp_servers.templum.env_vars=["TEMPLUM_BUS","TEMPLUM_TERMINAL"]' -c 'mcp_servers.templum.tool_timeout_sec=3600' -c 'notify=["/opt/templum","hook","codex"]'
```

From batch 7 the four `mcp_servers` flags stay, and `notify` gives way to one flag per hook of the table
[above](#hooks-and-agent-state), when `<data>/bin` is on the terminal's `PATH` ([Identity](#identity)); `Interrupt`'s
timeout is 3 s, the most Codex gives it (it says so at every start otherwise):

```
-c 'hooks.SessionStart=[{hooks=[{type="command",command="templum hook codex",timeout=5}]}]' … -c 'hooks.Stop=[…]' -c 'hooks.Interrupt=[{hooks=[{type="command",command="templum hook codex",timeout=3}]}]'
```

The hooks run `templum` from the terminal's `PATH` (`<data>/bin` comes first, [Identity](#identity)), not Templum's
path: Codex asks its person to trust a hook it hasn't seen, and again when its command changes, so the same words mean
one question, whatever Templum's version or where it runs from. Codex asks once, at its start and before its composer
("Hooks need review": Review hooks, **Trust all and continue**, Continue without trusting), and keeps that trust in its
own `config.toml` (`[hooks.state."/<session-flags>/config.toml:stop:0:0"]`, its `trusted_hash`), so the same flags are
trusted at every later start; Templum writes nothing there and never passes `--dangerously-bypass-hook-trust`, which
would run the person's own untrusted hooks too. Until the person trusts them, and in a Codex with its hooks feature off,
Templum hears nothing from it and reads its screen. With no `notify` flag, the person's own `notify` from
`~/.codex/config.toml` runs again.

**Gemini CLI:** it has no flag for MCP servers or hooks, and ignores a system settings file root doesn't own.
**Settings ▸ Connections ▸ Gemini CLI ▸ Install**, after a question, writes `<data>/gemini/gemini-extension.json` (the
server, `<data>/bin/templum mcp`, with an hour's timeout) and `<data>/gemini/hooks/hooks.json` (the hooks,
`'<data>/bin/templum' hook gemini`; `& '…'` on Windows, where Gemini CLI runs hooks through PowerShell), and runs
`gemini extensions link <data>/gemini --consent`; **Remove** runs `gemini extensions uninstall templum`.
`~/.gemini/settings.json` is never touched. Every Gemini CLI the person opens then has the server and the hooks, typed
by hand or not, whatever `[connections]` says; outside Templum the server shows as disconnected and the hooks do
nothing. Gemini CLI starts no stdio server in a folder it doesn't trust; it asks when it first opens one.

**Others** (Grok Build, Pi, OpenCode, the person's own): **Settings ▸ Connections** shows `<data>/bin/templum mcp`
with a Copy button. They know their block anyway: the server inherits `TEMPLUM_TERMINAL`.

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
| `templum hook claude` / `templum hook codex <json>` | an agent's hook (`templum hook codex` reads its JSON on stdin from batch 7; an argument, an older Templum's `notify`, is ignored, and stdin, then Codex's terminal, isn't read) | 1, 7 |
| `templum hook gemini` | Gemini CLI's hooks (JSON on stdin) | 5 |
| `templum connect <block>` | `connect` | 3 |
| `templum spawn <agent> [--folder <f>] [--worktree <name>] [--prompt <text>] [--model <m>] [--effort <e>]` | `spawn_agent` | 3, 6 |
| `templum delegate <block> <task> [--wait]` | `delegate` | 3 |
| `templum reply <block> <text>` | `reply` | 3 |
| `templum wait <block> [--for <event>] [--timeout <s>]` | `wait_for_idle`, `wait_for` | 3, 5 |
| `templum ask <question> [--option <o>]…` | `ask_user` | 3 |
| `templum kanban list [<board>] [--card <card>] \| add <title> [--column <c>] [--description <d>] [--board <b>] \| move <card> <column> \| comment <card> <text> \| claim <card>` | the kanban tools | 4 |
| `templum note read\|append <block> [text]` | `read_note`, `append_note` | 4 |
| `templum open <path>[:<line>]` | `open_file` | 4 |
| `templum git status [<block>]` | `git_status` | 4 |
| `templum keys <block> <key>…` | `send_keys` | 6 |
| `templum interrupt <block>` | `interrupt` | 6 |
| `templum close <block>` | `close_block` | 6 |
| `templum wire <a> <b>` | wires two blocks of the workspace of the terminal it runs in, a step of Ctrl+Z; not a tool (an agent's MCP server never lists it); it asks no one, so an agent's shell tool can wire its own block (the person's choice) | 5 |

A verb stays connected until its answer: stopped with Ctrl+C, what it waited for (a balloon) is withdrawn.
`templum open` and `templum spawn --folder` read a relative path from the folder they run in (an agent's shell tool may
not be in the agent's); `templum open` takes `<path>:<line>:<column>` as compilers and grep print it, the column
ignored.

Any other first argument opens Templum, as today. On Windows the release program has no console, so the verbs'
output reaches a pipe (`templum blocks | more`) but not a console directly.

## Config

```toml
[connections]
## Templum's MCP server and hooks are given to the Claude Code and Codex it opens.
# enabled = true
```

Off, nothing is typed into agents; the bus still serves the command line. **Settings ▸ Connections** (batch 5) has
the switch, the server's command for other agents with a Copy button, and Gemini CLI's Install and Remove.

## Files

- **Workspace files** ([persistence.md](persistence.md#the-file)): every item gains its block id, and the canvas its
  wires, by block id. A wire whose block didn't come back, one of a block to itself, and a second one between the
  same two blocks are dropped. An older Templum ignores both. A label is trimmed and cut at 60 characters; a blank
  one is none.

  ```json
  { "version": 1,
    "items": [{ "kind": "terminal", "uid": "b7k2m", "rect": {…}, … }, { "kind": "note", "uid": "4qm2a", … }],
    "wires": [{ "a": "b7k2m", "b": "4qm2a" }, { "a": "b7k2m", "b": "t4q8p", "label": "dev server" }] }
  ```

- **`boards.json`** (batch 4, [kanban.md](kanban.md#files)): a card gains `assignee` (a block id) and `comments`, each
  `{ "by": "<block id or 'you'>", "text": "…", "at": <unix seconds> }`. An older file loads with none; an older
  Templum ignores them. The card shows the assignee's block title and the comment count; its dialog lists the
  comments, adds the person's, and frees the assignee (✕), each a step of the kanban's Ctrl+Z; `claim_card` refuses a
  card another block holds until the person frees it.
- **Terminal items** (batch 6) also keep `opened_by` (the block id of the agent that spawned it), `model` and
  `effort` (what `spawn_agent` was given). A pasted block has none; an older file loads with none; an older Templum
  ignores them.
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
| `templum/src/workspace/wires.rs` (new), `templum/src/wires.rs` (new) | wiring, unwiring, selecting, undo, labels (`Change::Label`); painting wires, the dot and the labels | 2, 5 |
| `templum/src/workspace/waits.rs`, `delegate.rs`, `spawn.rs` (new) | the requests that wait and the balloon's answers; tasks handed over, `reply`, `wait_for_idle`; `spawn_agent` | 3 |
| `templum/src/workspace/content.rs` (new) | the kanban, note, file and Git tools | 4 |
| `templum/src/balloon.rs` (new) | the balloon | 3 |
| `templum-git/src/repo.rs` | `Repo::add_worktree` | 3 |
| `templum/src/toolbar.rs`, `menu/`, `workspace/history.rs`, `workspace/input.rs`, `workspace/render.rs` | the Wire tool, the menus, the undo steps, the pointer | 2 |
| `templum/src/board.rs`, `kanban.rs` | a card's assignee and comments, saved in `boards.json`; the card's face and the open card show them | 4 |
| `templum-usage/src/gemini.rs` (new) | the Gemini CLI extension | 5 |
| `templum/src/workspace/activity.rs` (new) | an agent's state from its screen | 5 |
| `templum/src/settings/connections.rs` (new), `workspace/usage.rs` | Settings ▸ Connections; running Gemini CLI | 5 |
| `templum-terminal/src/input.rs`, `terminal.rs`, `agent.rs`; `templum/src/agents.rs` | named keys and `Terminal::press`; each agent's model and effort flags (`launch_args`) | 6 |
| `templum/src/workspace/delegate.rs`, `spawn.rs`, `bus.rs`, `mod.rs`, `blocks.rs`, `persist.rs`, `selection.rs`; `session.rs`, `block.rs` | `send_keys`, `interrupt`, `close_block`; `opened_by`, `model` and `effort` saved and resumed; "answered by" | 6 |
| `templum-mcp/src/hooks.rs`, `lib.rs`, `cli.rs`; `templum-terminal/src/terminal.rs`; `templum/src/workspace/bus.rs`, `delegate.rs`, `activity.rs`, `spawn.rs`, `waits.rs`, `input.rs`, `mod.rs` | Codex's hooks; a prompt taken (`Submitted`) confirms a task and decides which turn answers it; the first line as a paste of its own; a prompt waits for its agent's input (`Modes::typing`); seems stuck; dialogs as drawn | 7 |
| `templum/src/workspace/delegate.rs`, `waits.rs`, `bus.rs` | task ids, the notices typed back, changes to a task, a task's timeout | 8 |

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
nothing on stdout. Every read and wait in these tests has a 5-second limit. (batch 5) `wait_for` takes its four events
and refuses another; `wire` is parsed but never listed; Gemini CLI's hooks become their events; `wait --for` and `wire`
send their calls, and every verb's usage offers `--json`.

**Unit (`templum-usage`, batch 5):** the extension's two files exactly, not rewritten unchanged, Windows' hook
command, a `'` in the path gives none.

**Unit (`templum`, batch 5):** the screen's looks (3 of 4 changed: at work; 5 still: at rest; a wait for the person
stays); a label's scale per level.

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
matches the repository, and every tool is refused without a wire; (batch 5) `wait_for` answers `needs_you` at once or
when the agent or its balloon asks, a command's end and code, a bell and a notification, its timeout with the last
lines, a closed block, and shows no balloon; `templum wire` wires a step Ctrl+Z takes back and an MCP client can't
call it; a changing screen says Working and a still one ends the task, the person's typing not counting; Rename
wire… labels, Ctrl+Z, a disconnect, a close and a restart keep it, a hand-edited file's label is trimmed or dropped;
the pill shows at 100%, 50% and 25% and not in grid mode; Settings ▸ Connections' switch, its Copy button and
Gemini's question; Gemini CLI's run, its failure and its absence.

**Batch 6:** each named key's bytes, under the kitty keyboard and application cursor keys, a character as text and an
unknown name refused; each agent's model and effort flags and every unsafe model refused; `send_keys` parses 1 to 16
keys; keys reach a terminal in bracketed-paste mode as keys, never `ESC[200~`; keys into a waiting agent are taken and
say who answered; the caller's own block and another agent's task are refused; `interrupt` presses Esc and ends the
caller's task; `send_to_terminal`'s Enter comes after its text, and two in a row stay two lines; `spawn_agent` types
its model and effort first and a bad one opens nothing; `close_block` closes only what the caller opened and Ctrl+Z
brings it back; `opened_by`, `model` and `effort` survive a round trip, and an older file loads; a snapshot that finds
another agent drops the model and the effort; a model Templum can't type in a hand-edited file isn't typed.

**Batch 7:** Codex's hook payloads become their events (`compact` and `SessionEnd` none, an older `notify`
argument none, stdin unread); Codex's hook flags, none without `<data>/bin` on PATH; a turn that ends before its task
was taken doesn't answer it, nor the turn Claude Code queued it behind unless a tool's end took it in; a tool's end
starts no turn; only a prompt taken confirms a task (not a `Stopped` event, not a moving screen); no second Enter into
a changed screen, nor while the person types; the first line goes as a paste of its own; a prompt waits for its
agent's input to take pastes and show its cursor, Claude Code's without any hook, and one not typed within a minute
ends its task with its screen; a hooked agent gone still keeps its task and seems stuck, unless the person typed into
it; `wait_for(needs_you)` and `delegate`'s refusal show the dialog as drawn, `list_blocks` what it asks.

**Batch 8:** a task handed without `wait` comes back typed, with its id, time and tokens; a waiting call gets the answer
and no notice; a notice waits while its asker asks its person, and never goes into a shell; a dialog in a task is told
once; a change from the asker goes into its task and another agent is still refused; a task past its timeout comes
back as failed.

**Manual (Hyprland):** Claude Code from the menu: `/mcp` lists `templum` connected, "list the blocks" and "read the
last 30 lines of the dev server" work, the dot follows its turns and a permission prompt; Codex: `/mcp` lists it;
`templum blocks` in a plain terminal; `templum mcp` in another terminal emulator exits 2; wiring by tool and menu,
restart; Claude Code delegating to Codex and getting its answer; a question and a connect request in the balloon;
`spawn_agent` with a worktree; (batch 5) Gemini CLI's Install, `/mcp`, dot and Remove; Claude Code delegating to
Gemini CLI; Codex's dot turning accent within about 3 s of a prompt; a permission dialog left 15 s still yellow, an
allowed `sleep 20` turning accent within about 3 s, Esc while Claude Code writes ending Working within about 5 s, a
long prompt typed into an idle agent showing no accent; `templum wait --for command_finished` and `--for attention`;
`templum wire` and Ctrl+Z; a label at 50% and 25%, hidden in grid mode, kept by a restart, and Ctrl+Z after a rename
taking the name, not the wire; Settings ▸ Connections' switch (no `--mcp-config` once off) and Copy; on Windows,
Gemini CLI's dot following its turns. (batches 6 to 8) The user's own test again: Claude Code spawns a Codex
with a model and an effort, hands it a task without waiting, changes it while it works, and hears its end typed back
with its time and tokens; Codex's trust question for Templum's hooks asked once and answered with `send_keys ["2"]`,
nothing left in Codex's input, "answered by" shown; a still Codex dialog turning yellow as stuck; `interrupt`, then
`close_block`, then Ctrl+Z.

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
6. **Control:** `send_keys`, `interrupt`, `close_block` and `opened_by`, `spawn_agent`'s `model` and `effort`,
   "answered by", `send_to_terminal`'s later Enter.
7. **State:** Codex's hooks, `Submitted` confirming a task, a prompt waiting for its agent's input, seems stuck,
   dialogs as drawn.
8. **Async:** task ids, the notices typed back with time and tokens, changes to a task, a task's timeout told.

Batches 2, 3 and 6 to 8 paint or type into agents, so their pull requests carry `--bench` and `--bench-mixed`. Batches 7
and 8 start with checks by hand of the agents themselves (what Codex takes on its command line and the payloads of its
hooks; when Claude Code and Codex fire `UserPromptSubmit` for a prompt typed while they work; the key that stops a turn;
when each agent turns bracketed paste on), and their plans correct this spec with what they find (batch 7's: [Spike
results](../plans/2026-10-16-connections-batch-7.md#spike-results)).

## Risks

| Risk | What we do |
|---|---|
| Claude Code moves a tool call past 2 minutes to the background, and stops waiting for it | The tools that wait say so; a cancelled call (`notifications/cancelled`) drops its request (tested); that a late result reaches the agent is checked by hand |
| Typing into an agent that is busy | Refused while it waits for its person; while it works, Claude Code and Codex queue typed input (checked by hand in batch 3); the `[from …]` line keeps it readable; Claude Code fires `UserPromptSubmit` for it at the Enter and queues it behind the turn under way unless a tool's end takes it in, Codex once a turn takes it in (batch 7's spikes), which a change to a task (batch 8) relies on |
| An agent answers another's trust question (a folder's, or Codex's for hooks, which may list a repository's own hooks) | The wire is the person's consent; "answered by" names the agent; the result shows the screen after the keys |
| Keys into a shell | The same trust as `send_to_terminal`; at most 16 keys a call |
| A model or effort typed into a shell | A strict set of characters, no leading `-`, an effort from its agent's list; refused before anything opens |
| A dialog no hook tells of (a trust question, Codex's choice of model) | No blind second Enter into a changed screen; a hooked agent gone still for 30 seconds seems stuck, and its asker hears so |
| Codex's hooks need its person's trust | Asked once, about one fixed command (`templum hook codex`); never `--dangerously-bypass-hook-trust` |
| A notice typed into an agent the person is typing into | It waits until the person stopped typing for 3 seconds; never into a shell |
| A spawned agent's prompt typed while it asks a first-run question (trusting a new folder, Codex's hooks) | From batch 7 Claude Code's and Codex's wait until they bracket pastes and show their cursor, which both hide in those questions (checked by hand); on Windows the still screen alone |
| Claude Code takes a long paste for data it was shown, not for what it is asked (batch 7's spikes) | The first line, `[from …]` and a sentence, goes as a paste of its own, which Claude Code reads as typed |
| A command an interrupt left running ends later, with Codex's `PostToolUse` | A tool's end never starts a turn (batch 7) |
| A task typed into a Codex at work is answered by the turn it was on | One task at a time per agent; from batch 7, a turn answers a task only once the agent took it (`UserPromptSubmit`) |
| An agent without events never says it is done | `reply`; its screen going still ends its task (`activity.rs`); else the wait's timeout with the last screen lines |
| Gemini CLI ignores a system settings file root doesn't own | An extension the person installs once from Settings ▸ Connections |
| A spinner drawn under a permission dialog would turn its yellow dot to Working | The screen ends a wait for the person only once the person typed into the terminal after it began; checked by hand with Claude Code and Gemini CLI (manual check 4) |
| Windows: PowerShell may not wait for, or hand stdin to, `templum.exe` (a windowed program) in a Gemini hook | Checked by hand on Windows; else `$input \| & '<bin>' hook gemini` |
| Codex's `-c` under `cmd.exe` and Windows PowerShell 5.1 | Not typed there (Git Bash is the default shell on Windows; pwsh 7 takes them) |
| A path Templum can't type safely (`"`, `$`, `'`, a backtick…) | No connections for that agent, and a notification once per session (deferred) |
| macOS's 104-byte socket path | A private folder in the temporary folder past 100 bytes |
| The AppImage's executable goes away when it quits | `$APPIMAGE` is the program typed |
| Reading history while the terminal's thread parses | The unfair lock waits only for the parse under way, as painting does; at most 2000 lines and 256 KB |
| A hook slows the agent when Templum is gone | One-second connect limit, exit 0; each hook has a 5-second timeout |
| Painting wires costs frames | One curve per wire per frame, below the blocks; cached per zoom step as drawings are if the benches ask for it |
| A process of the same user names another terminal | The documented trust: it could already type into it |
