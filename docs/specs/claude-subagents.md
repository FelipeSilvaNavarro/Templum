# Claude Code's subagents on the canvas

- **Date:** 2026-10-22
- **Status:** Implemented (kanban card 127, "Subagentes do Claude"), from the study
  [docs/plans/2026-10-22-claude-subagents.md](../plans/2026-10-22-claude-subagents.md): its option C1 (a parent
  honestly busy while its subagents work) and A1 (each subagent in a terminal-style block of its own), with the
  person's answers to its open questions.
- **Builds on:** [connections.md](connections.md) (hooks and agent state, delegating),
  [agent-protocol.md](agent-protocol.md) (what ends a task).

## Goal

Claude Code starts subagents of its own with its `Agent` tool, inside its process, in the background by default. A
person watching the canvas should see each of them work, and an agent that handed that Claude Code a task should get
its real answer, not the message of the turn that started the subagents.

**Not in it:** typing into a subagent, answering its dialog from its block, or stopping it there (it has no terminal
of its own: the dialog is in its parent's screen, the study's [Interacting with one](../plans/2026-10-22-claude-subagents.md#interacting-with-one));
the chat look (the study's A2); a switch that takes Claude Code's subagents away (B2); steering one through its
parent (C3); subagents of a Claude Code opened outside Templum, or of the Templum Agent (which has no `Agent` tool).
Templum's instructions to Claude Code about subagents stay as they are.

## What the person sees

### The parent

A Claude Code whose turn ends while subagents it started still work stays at work: its dot keeps the accent, and its
tooltip says how many work, "Working: 2 subagents" ("Working: 1 subagent"); `list_blocks` says `, working: 2
subagents`. Each subagent's report comes back to it as a turn of its own; once the last one is in and that turn ends,
it is at rest, and a task another agent handed it ends with that turn's answer.

A permission dialog a subagent opens is in its parent's screen: the parent's dot turns yellow, "Waiting for you: Allow
Bash?", and stays so until that subagent's tool ends (or it stops): another subagent's tool, or the parent's own,
doesn't turn it back to Working.

## Hooks

Templum's Claude Code settings file adds `SubagentStart` and `SubagentStop` to the hooks of
[connections.md](connections.md#hooks-and-agent-state). `templum hook claude` turns them into events
(`hooks::claude`, `crates/templum-mcp/src/hooks.rs`). The fields are the ones the study's
[probe](../plans/2026-10-22-claude-subagents.md#the-probe) saw on Claude Code 2.1.286.

| Hook | Event |
|---|---|
| any hook with `agent_id` and an empty `agent_type` | nothing: Claude Code's own agents (prompt suggestions, `/btw`) |
| `SubagentStart` | `SubagentStarted`: its `agent_id`, `agent_type`, and the session's `transcript_path` |
| `PostToolUse`, `PostToolUseFailure` with `agent_id` | `SubagentWorking`: never the parent's turn, also on `is_interrupt` |
| `PermissionRequest` with `agent_id` | `SubagentNeedsYou`, "Allow *tool*?" as the parent's own |
| `SubagentStop` | `SubagentStopped`; `reports` when its own `background_tasks` still lists it (it ran in the background, and its report comes as a turn) |
| `Stop` whose `background_tasks` lists a `subagent` | `AwaitsSubagents`, with their ids (a command left running isn't one) |
| `Stop` that lists none | `Stopped`, as before |
| `UserPromptSubmit` whose prompt starts with `<task-notification>` | `Reports`, with the ids of its `<task-id>`s: Claude Code took reports as a prompt of its own |

Any other hook with an `agent_id` says nothing.

## The parent's turn

Templum keeps, on the parent's pane (`Pane::subagents`), the subagents it heard start, each at work or **reported**:
done in the background, its report not yet taken.

- `SubagentStarted` adds one at work (a resumed one is at work again).
- `SubagentStopped` takes it off, or marks it reported when `reports`.
- `Reports` takes off every one it names, at work or reported: its report is in (one stopped from Claude Code's panel
  may say nothing else). It starts a turn that isn't a prompt anyone typed: a task waiting for its Enter isn't taken
  by it.
- `AwaitsSubagents` keeps only the reported ones and those it names, which are at work.
- `Stopped` with an answer (Claude Code's `Stop`, which named none at work) keeps only the reported ones.
- A new session (`Started`) or the agent leaving forgets them all.

A turn's end (`Stopped`, `AwaitsSubagents`) while any is left isn't an answer: the agent stays Working (or waiting
for its person, when a subagent's dialog is open), nothing waiting on its turn is answered, and its task stays open.
The turn a task was queued behind ends, as before, and the next turn's end is the task's. Otherwise the turn's end is
as before ([agent-protocol.md](agent-protocol.md#3-what-ends-a-task)). This covers the race the probe saw: a subagent
that stops while the turn taking another's report runs leaves its report to come, so that turn's `Stop` (no subagent
at work, an interim message) answers nothing.

Between that turn's end and the next turn (`Pane::awaits_subagents`):

- no turn is under way, so a task typed then starts one at once instead of being queued behind one;
- its screen isn't looked at ([An agent's state](connections.md#an-agents-state)): its prompt sits still, and its
  subagents' hooks tell. Claude Code's `idle_prompt` meanwhile (a `Stopped` without an answer) ends nothing.

The dialog a subagent opens (`SubagentNeedsYou`) is remembered with its id (`Pane::asked_by`): only that subagent's
`SubagentWorking` or `SubagentStopped` ends the wait, never another's tool nor the parent's own `Working`; the
parent's own dialog is ended by its own `Working`, as before.

## Testing

- `hooks.rs`: `claude_codes_subagents_are_told_apart_from_its_own_turn`, on the probe's shapes.
- `crates/templum/src/workspace/tests/subagents.rs`, each hook input turned into its event as `templum hook claude`
  does: a task answered by the turn after the last report, the race included, an idle prompt ending nothing; a report
  that doesn't take a task in; a subagent's dialog that only its own tool ends; a subagent's tool that doesn't take a
  queued task into the turn; one gone without a word that doesn't keep its parent busy; a still screen that neither
  ends nor sticks a parent awaiting its subagents.

## Risks

- **The report's prompt is Claude Code's own format** (`<task-notification>`, `<task-id>`), observed, not
  documented. If it changes, reported subagents are never taken off: the parent stays Working and its task open
  until its timeout. Only a reported one (it stopped during a turn) depends on it; the others are settled by `Stop`'s
  `background_tasks`, which is documented.
- **A foreground subagent** (fork mode off) is inferred not to be in `background_tasks` as it stops, so its stop
  takes it off at once. Not probed.
