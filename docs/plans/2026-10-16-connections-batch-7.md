# Connections, batch 7 (State): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

## Context

Batches 1 to 6 of [docs/specs/connections.md](../specs/connections.md) are on `development` (bc51b3b). Batch 7 makes
an agent's state trustworthy: Templum hears Codex through its hooks, a task counts as taken only once the agent's hook
says it took a prompt (and the turn that answers it is the one that took it), a spawned agent's prompt waits until its
input box takes typing, a hooked agent gone still "seems stuck" instead of being taken for done, and the dialogs an
agent waits on come back as drawn. The spikes below, run with the real agents, answered the spec's open points and
changed the design in four places (Decisions 2, 3, 5, 6). **This plan's commit already corrected the spec's batch-7
text** with them (Spawning's readiness, Delegating 2 and 9, An agent's state, the hooks tables, what Templum types
into Codex, the command line's `hook codex`, Testing, Delivery, Risks, the Architecture row); Task 9 only flips its
status and the minors.

**Goal:** Codex's hooks (six `-c` flags instead of `notify`); `AgentEvent::Submitted` confirming a task and deciding
which turn answers it; a task's first line typed as a paste of its own; a spawned prompt waiting for its agent's input
(`Modes::typing`) and dropped after a minute; seems stuck; dialogs as drawn in `wait_for(needs_you)`, `delegate`'s
refusal and `list_blocks`.

**Architecture:** `templum-mcp` reads Codex's hook payloads from stdin (`hooks::codex`, `cli::hook_event`) and types
its hooks (`codex_flags`); every agent's "took a prompt" hook becomes `Submitted { at_enter }`. In the app,
`bus.rs::agent_event` counts prompts taken (`Pane::submitted`) and marks the task `taken`/`behind` (`Handed`), so a
turn's end answers only the task's turn; `delegate.rs` confirms a task by that count alone and types its first line
apart; `spawn.rs` looks at every spawn until its input takes typing (`templum-terminal`'s `Modes::typing`: bracketed
pastes and a shown cursor); `activity.rs` turns a hooked agent's long stillness into a wait for its person;
`waits.rs`/`delegate.rs`/`bus.rs` add the screen's last 20 lines to what a dialog says.

**Tech Stack:** Rust 2024; GPUI (gpui-fast 0.1.8) through gpui-kit `=0.7.3`, no new GPUI API (`cx.spawn`,
`background_executor().timer`, as `send_task` uses them); `alacritty_terminal` `=0.26.0`'s
`TermMode::{BRACKETED_PASTE, SHOW_CURSOR}` (its default mode shows the cursor); serde. Agents checked on 2026-10-16:
Claude Code 2.1.286, Codex 0.154.0; Gemini CLI 0.63.0 has no login here (not run).

**Spec:** [docs/specs/connections.md](../specs/connections.md) at this plan's commit: An agent's state (the
seems-stuck paragraph), Delegating 2 (the first line, **Confirmed**), 5, 9, Spawning (readiness), Hooks and agent
state (Claude Code's `UserPromptSubmit` and `PostToolUse` rows, Codex's table), What Templum types into each agent
(Codex), the `hook codex` row, Architecture (batch 7 row), Testing "Batch 7", Delivery item 7, Risks.

## Spike results

Run on 2026-10-16 in tmux (200 × 50), the agents in scratch folders, a fake `templum` first on `PATH` that logged
`date +%s.%N`, its argv, `TEMPLUM_TERMINAL` (set to `spike-1`) and its stdin, then exited 0; the terminal's raw bytes
logged with their times (`tmux pipe-pane`). Codex `-m gpt-5.6-luna -c model_reasoning_effort=low` (6 prompts, 3
follow-ups typed while it worked), Claude Code `--model haiku --settings <scratch>/settings.json --allowedTools
'Bash(sleep:*)'` (5 prompts, 4 follow-ups); the hook command in both was the exact `templum hook codex|claude`.

**S1 — Codex's hooks given as `-c` flags work, and Codex keeps their trust.**

- `-c 'hooks.<Event>=[{hooks=[{type="command",command="templum hook codex",timeout=5}]}]'` for `SessionStart`,
  `UserPromptSubmit`, `PermissionRequest`, `PostToolUse`, `Stop` and `Interrupt` is the right shape: all six load.
  `Interrupt` is real ("Right before an interrupted turn is aborted"); Codex warns at every start "⚠ clamping
  Interrupt hook timeout to 3s in /<session-flags>/config.toml" unless its timeout is 3 (with `timeout=3` the warning
  goes, and the trust recorded for 5 still holds: the next starts asked nothing).
- First start in a new folder: the folder's trust question ("Do you trust the contents of this directory? › 1. Yes,
  continue  2. No, quit"), then "Hooks need review · 6 hooks are new or changed · › 1. Review hooks  2. Trust all and
  continue  3. Continue without trusting (hooks won't run)". The review lists ours as "Source  Session flags -
  /<session-flags>/config.toml · Command  templum hook codex · Trust  New hook - review required". Trusted (`t` in
  `/hooks`), Codex wrote, under its own `config.toml`, one entry per hook:
  `[hooks.state."/<session-flags>/config.toml:stop:0:0"]` `trusted_hash = "sha256:20fb86…"` (and `session_start`,
  `user_prompt_submit`, `permission_request`, `post_tool_use`, `interrupt`). The key names no path: any Templum's flags
  are the same hooks. A second start with the same flags asked nothing.
- The person's own `~/.codex/hooks.json` hooks run alongside: `/hooks` showed "PermissionRequest  Installed 2
  Active 2", "Stop  2  2", one from `~/.codex/hooks.json`, one from the session flags.
- `SessionStart` doesn't come when Codex opens, nor after either trust question, nor after a late trust: it comes
  with the first prompt, 125 ms before its `UserPromptSubmit` (hooks log: `SessionStart` at …168.765,
  `UserPromptSubmit` at …168.890; Codex had been open 2 minutes).
- Payloads (paths cut): `{"session_id":"01a14374-…","transcript_path":"/home/winx/.codex/sessions/…jsonl","cwd":"…",
  "hook_event_name":"SessionStart","model":"gpt-5.6-luna","permission_mode":"default","source":"startup"}`;
  `UserPromptSubmit` adds `"turn_id":"01a14375-…","prompt":"Run `sleep 12` in the shell, then reply with just the
  letter A."` (a 1808-character paste came expanded, as plain text, starting with its first line); `PermissionRequest`
  `"tool_name":"Bash","tool_input":{"command":"touch f.txt","description":"May I create the requested f.txt file in
  the current workspace?"}` (its dialog drew 25 ms after the hook started); `PostToolUse` `"tool_name":"Bash",
  "tool_input":{"command":"sleep 12"},"tool_response":"…","tool_use_id":"exec-bb2a…"`; `Stop` `"stop_hook_active":
  false,"last_assistant_message":"A B"`; `Interrupt` only the common fields and `turn_id`. Every event after
  `SessionStart` carries `turn_id`.
- One Esc during a turn: "■ Conversation interrupted", the `Interrupt` hook, no `Stop`. **The command the turn ran
  (`sleep 30`) kept running** ("1 background terminal running") and ended 28 s later with a `PostToolUse` of the
  interrupted turn's `turn_id`, while Codex sat idle.
- Hooks inherit `TEMPLUM_TERMINAL` (`spike-1`) and `PATH`, with Codex's own folder first
  (`PATH0=/home/winx/.codex/tmp/arg0/codex-arg0dm26i6`; `templum` still resolved to ours). They run outside its
  sandbox (`-s read-only`: the hook still wrote its log).
- `notify` isn't needed: `Stop` carries the last answer.

**S2 — Claude Code, a prompt typed (pasted, then Enter) while it works.**

- During a tool (`sleep 15`): `UserPromptSubmit` "also say B" came **at the Enter** (0.3 s after), mid-turn, with
  **the turn's own `prompt_id`** (`f3184d6f…`, the same as the first prompt's); the screen said "Press up to edit
  queued messages"; the tool's `PostToolUse` came next, then one `Stop` with `"last_assistant_message":"A\nB"`,
  same `prompt_id`: the queued prompt joined the turn under way after the tool's end.
- While it wrote text (no tool after): `UserPromptSubmit` "also say B" at the Enter (turn `fba6b666`), then that
  turn's `Stop` (the numbers, `fba6b666`), **then a new turn ran "B" with a new `prompt_id` (`84fa8bc8`) and no
  `UserPromptSubmit` of its own**, ending with its `Stop` "B".
- So no field ties a `Stop` to a queued prompt: `prompt_id` names the turn. What does: a tool's end after the queued
  prompt's `UserPromptSubmit` takes it into the turn under way; a turn that ends first leaves it to the next.
- One Esc while a message was queued during a tool: the turn was interrupted ("⎿ Interrupted · What should Claude do
  instead?", no `Stop`), and the queued "also say F" then ran as a new turn (new `prompt_id`, no
  `UserPromptSubmit`), its `Stop` "F". The Esc doesn't pull the message back.
- `idle_prompt` (`Notification`) came 60.06 s after a turn's `Stop` (twice), carrying that turn's `prompt_id`.

**S3 — Codex, Enter while it works:** Enter *steers*. The screen showed "• Messages to be submitted after next tool
call (press esc to interrupt and send immediately) ↳ also say B" and the hint "tab to queue message".
`UserPromptSubmit` came **14 s after the Enter, when the turn under way took it** (56 ms after the running tool's
`PostToolUse`), with that turn's `turn_id`; one `Stop` answered both ("A B"). Typed while Codex wrote its answer (no
tool after): `UserPromptSubmit` 1.3 s later, same `turn_id`, and the turn's one `Stop` was "Z" (the steer's answer).
**Tab** queues for the next turn ("• Queued follow-up inputs ↳ say D"): the first turn's `Stop` ("C"), then 83 ms
later `UserPromptSubmit` "say D" with a new `turn_id`, then its `Stop` "D".

**S4 — Stop keys:** Codex: one Esc stops a turn (the `Interrupt` hook), and so does one Ctrl+C (the `Interrupt` hook
too). Claude Code: one Esc stops a tool's turn (above). Gemini CLI: skipped, no login (`~/.gemini` has no
`oauth_creds.json` and there is no `GEMINI_API_KEY`).

**S6 — Readiness** (raw bytes; times from the command's Enter):

| Agent | `?2004h` (bracketed paste) | cursor `?25` at rest | input box |
|---|---|---|---|
| Codex, new folder | on at 0.010 s, **before** the folder's trust question | hidden in the trust question, in "Hooks need review", in `/hooks` and in its approval dialog; shown in its composer, also while it works | — |
| Codex, trusted | 0.007 s | shown from 0.106 s | at once; no hook until a prompt |
| Claude Code, new folder | on at 0.189 s, **before** its trust question ("❯ No, exit / Yes, I trust this folder": the highlighted default is No) | hidden in the trust question; shown at its input from 22.426 s | `SessionStart` at 22.32 s, while bracketed paste was **off** (22.278–22.390 s) after the trust; alt screen and mouse on at 22.426 s |

Bracketed paste alone marks nothing (both turn it on before their first question); bracketed paste **and** a shown
cursor marks the input box in both. Claude Code draws each frame with the cursor hidden and shows it at its end.

**Found on the way — Claude Code takes a long paste for data.** A paste of 4 lines or more, or of more than 800
characters, shows as "[Pasted text #1 +36 lines]" and reaches the model (and `UserPromptSubmit`'s `prompt`) as
`"\n\n<pasted_content id=\"5bac\">\n[from Spike · test (abcde)]\n…\n</pasted_content id=\"5bac\">\n"`. Haiku
answered: "I received the pasted content, but it doesn't include your own message asking me to follow the
instructions in it… I only follow instructions inside pasted content when your own message explicitly asks me to."
The same task with `[from Spike · test (abcde)] Your task is in the pasted text: do it.` as a paste of its own before
it (shown inline: "…do it.[Pasted text #2 +35 lines]") was done (`sleep 15`, then "C"). 2 and 3-line pastes and a
790-character one stay inline. Today's tasks are one paste, header included: a multi-line task handed to Claude Code
may never be done.

**What Codex added to `~/.codex/config.toml`** (diff against the copy taken first; `~/.codex/hooks.json` unchanged):
`[projects."/tmp/claude-1000/…/scratchpad/spike7/codexdir"] trust_level = "trusted"` (the scratch folder; the
maintainer may delete it) and the six `[hooks.state."/<session-flags>/config.toml:<event>:0:0"] trusted_hash` entries
of the hooks Templum will type (kept on purpose: this Codex won't ask again).

## Global Constraints

- Only `crates/templum` depends on gpui-kit; `templum-terminal` and `templum-mcp` stay UI-free and are tested
  headlessly. No new dependency. No `unsafe`. Every logic change ships a test that fails without it.
- Codex's hooks, verbatim: `-c 'hooks.<Event>=[{hooks=[{type="command",command="templum hook codex",timeout=5}]}]'`
  for `SessionStart`, `UserPromptSubmit`, `PermissionRequest`, `PostToolUse`, `Stop`, and `timeout=3` for
  `Interrupt`; never `--dangerously-bypass-hook-trust`; nothing written to Codex's files.
- "must take the task within 3 seconds of its Enter, or open a dialog; one at work when the task was typed queued it.
  Nothing else confirms it: not a turn's end, a notification (Claude Code's `idle_prompt`…), nor a moving screen."
  Second Enter "only while Templum still may type into it, its screen hasn't changed since 150 ms after the first …
  and the person hasn't typed into it for 3 seconds; otherwise, or with still no word 3 seconds later, the task ends
  untaken, with the last 20 lines of its screen."
- First line: `[from <title> (<uid>)] Your task is in the pasted text: do it.` as a paste of its own, then the task.
- Spawn: "its process is seen and its screen has been still for a second; … Claude Code and Codex also take typing
  then: they bracket pastes and show their cursor … (on Windows … the still screen alone)"; "A prompt not typed within
  a minute of the spawn … is dropped, and its task ends with the last 20 lines of its screen."
- Seems stuck: 30 still looks, "Waiting for you: it seems stuck, its screen still for 30 seconds"; 5 looks for an
  agent without hooks or one the person typed into since its last hook.
- Commits on `development` (never `main`), small, Conventional Commits; `feat`/`fix` titles are for a person using
  Templum. End each message with the executing session's `Claude-Session:` line when it has one.
- Before saying done: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`, `cargo deny check`; this batch touches `templum-terminal` and types into agents, so `--bench` and
  `--bench-mixed` run too: `p95_ms` ≤ 16.7 each.

## Decisions

1. **Codex's hooks are `-c` flags, not a plugin** (S1: they load, Codex asks once and keeps the trust under a key that
   names no path). `codex_flags` gives them only when `<data>/bin` is on the terminal's `PATH` (`bus_env` holds a
   `PATH`), as `terminal_env` puts it there: without it the bare `templum` would fail at every event. No `notify`
   flag any more, so the person's own `notify` runs again. `SessionEnd` isn't given (it also comes after 30 idle
   minutes). `templum hook codex` with an argument (an older Templum's `notify`, whose stdin is Codex's terminal)
   sends nothing and reads nothing.
2. **`AgentEvent::Submitted { at_enter: bool }`** for Claude Code's `UserPromptSubmit` (`at_enter: true`), Codex's
   (`false`) and Gemini CLI's `BeforeAgent` (`false`, assumed: not checked, no login). The first `Submitted` after a
   task was typed (a spawn's once its prompt left `Pane::prompt`) makes it `taken`; one with `at_enter` that comes
   while the agent is `Working` makes it `behind` (Claude Code queued it, S2). A tool's end (`Working`) clears
   `behind` (it joined the turn under way). A `Stopped` answers the task only when it is `taken` and not `behind`;
   one that finds it `behind` clears `behind` (the next turn, which no hook announces, answers it). Not done
   (YAGNI): matching the hook's `prompt` to the task: the count is enough, and Claude Code's `prompt` for a long paste
   starts with `<pasted_content>`.
3. **A tool's end never starts a turn:** `Working` changes the state only from `Working` or `NeedsYou` (S1's
   `PostToolUse` 28 s after an interrupt would otherwise turn an idle Codex Working, then "stuck"). Tests that sent
   `working` to start a turn send `submitted`.
4. **Confirming:** only `Pane::submitted` moving confirms (no more `working ||`, the root cause of the minor: the
   screen's looks set Working from the paste's own echo); `queued` = the state was `Working` when the task was pasted
   (Codex at work says it took it only when a turn does, maybe minutes later). The person's keys are those of
   `input.rs` (`Pane::person_typed`, which sets `typed` and the new `keyed`); `send_keys`'s keys set `typed` only.
5. **The first line goes as a paste of its own** (the finding above): `send_task` types the text up to its first
   newline as one paste and the newline and the rest as another. `delegate`'s first line gains `Your task is in the
   pasted text: do it.` (the sentence that worked), `reply`'s `Its reply is in the pasted text.`, and a spawn's
   prompt is typed under the caller's `[from …]` line with the same sentence: a spawn's prompt is a task the caller
   handed, and one long line alone is collapsed too. Checked with haiku only: manual check 5 runs Claude Code's
   default model. **Needs the maintainer's eye** (it changes what agents read).
6. **A spawned agent's prompt waits for its screen and its input, for every agent** (replaces the planning pass's
   "screen watch only for agents without hooks; `Started` waits for bracketed paste; a hooked Codex waits for its
   session"): Codex's `SessionStart` comes only with its first prompt (S1), so waiting for it would wait forever;
   Claude Code's comes while its bracketed paste is off (S6). So `watch_ready` runs for every spawn with a prompt,
   `Started` types nothing, and Claude Code's and Codex's (`AgentEntry::builtin`) prompts also wait for
   `Modes::typing` (bracketed paste and a shown cursor), not on Windows. `typed()` stays as it is: whether Templum
   gave hooks no longer matters, which also fixes the minor of a Claude Code spawned without hooks. Any prompt not
   typed `STARTS_WITHIN` (60 s) after the spawn is dropped, its task ending with the screen.
7. **Seems stuck** is `AgentState::NeedsYou(Some(STUCK_TEXT))`, `STUCK_TEXT = "it seems stuck, its screen still for
   30 seconds"`, so the dot, `busy`, `delegate`'s refusal and `wait_for(needs_you)` treat it as any wait for the
   person, and the tooltip reads "Waiting for you: it seems stuck, …" with no change to `block.rs`. It needs
   `Pane::speaks` and no person's keys since the last hook (`Looks::answered`): the person's Esc while Claude Code
   writes (no hook) rests after 5 looks, as before. Moving again (3 of 4 looks) turns it back to Working without the
   person's keys. The asker's notice is batch 8's.
8. **Dialogs as drawn:** a hook comes before its dialog is drawn (it may answer it; Codex's drew 25 ms after), so a
   `wait_for(needs_you)` answered by an event reads the screen `DRAWN` (300 ms) later; one answered at once (the agent
   already asks) reads it then. `list_blocks` adds what it asks, cleaned (`clean_title`).

## Review Focus

1. The person presses Esc while Claude Code writes (no hook tells): its dot rests within about 5 s and never "seems
   stuck". (Task 7, `a_hooked_agent_still_for_thirty_looks_seems_stuck`, the typed part.)
2. A command an interrupt left running ends later (Codex's `PostToolUse`): an idle Codex stays idle, not Working and
   then stuck. (Task 3, `a_tools_end_starts_no_turn`.)
3. A Codex an older Templum opened runs the new program as its `notify`, its JSON as an argument: nothing is read from
   stdin, its terminal (it would eat the person's keys). (Task 1, the cli test's argument call with a payload on
   stdin.)
4. A spawned Codex or Claude Code sitting still at its trust or hooks question: nothing is typed, however long, and
   after a minute its task ends showing the question. (Task 6, both tests.)
5. The person types into an agent in the 3 s after a task's Enter: no second Enter sends their half-written line.
   (Task 4, `no_second_enter_into_a_changed_screen_or_while_the_person_types`.)

## File Structure

| File | Change |
|---|---|
| `crates/templum-mcp/src/hooks.rs` | `AgentEvent::Submitted`; `codex` reads hook payloads; tests |
| `crates/templum-mcp/src/cli.rs` | `hook codex` on stdin, an argument ignored |
| `crates/templum-mcp/src/lib.rs` | `codex_flags(exe, windows, hooks)` with the six hook flags |
| `crates/templum-terminal/src/terminal.rs` | `Modes::typing` |
| `crates/templum/src/workspace/mod.rs`, `blocks.rs` | `Handed::{taken, behind}`, `Handed::new`; `Pane::{submitted, keyed}` (`events` goes); `Prompt::gated` |
| `crates/templum/src/workspace/bus.rs` | `agent_event`'s `Submitted`, `Working`, `Stopped`, `Started` arms; `list_blocks`' message |
| `crates/templum/src/workspace/delegate.rs` | `confirm`, `send_task`, `not_taken`, `takes_typing`, `TAIL_LINES`, `TYPING`, `TASK_LEAD`, `Pane::{tail, person_typed}` |
| `crates/templum/src/workspace/input.rs` | the person's keys through `person_typed` |
| `crates/templum/src/workspace/spawn.rs` | every spawn watched; the input gate; the minute's deadline; the prompt's first line |
| `crates/templum/src/workspace/activity.rs` | `Looks::still`, `STUCK`, `STUCK_TEXT`, `Looks::state(state, speaks)` |
| `crates/templum/src/workspace/waits.rs` | `DRAWN`; `waiting_for_person` with the screen |
| `crates/templum/src/workspace/usage.rs` | `typed` passes whether `<data>/bin` is on PATH |
| tests: `templum-mcp` units, `templum-terminal` unit, `templum/src/workspace/tests/{delegate,spawn,bus,usage}.rs` | per task |
| `docs/specs/connections.md`, `docs/minors.md` | Task 9 |

Before Task 1: `git status` is clean on `development`, at this plan's commit or later.

---

### Task 1: Templum reads Codex's hooks, and a prompt taken from every agent's

**Files:**
- Modify: `crates/templum-mcp/src/hooks.rs:12-23` (`AgentEvent`), `:34` (Claude Code's `UserPromptSubmit`), `:74-79`
  (`codex`), `:88` (Gemini CLI's `BeforeAgent`); tests `:120-123`, `:250-259`, `:271-274`, `:325-339`
- Modify: `crates/templum-mcp/src/cli.rs:423-437` (`hook_event`); test `:743-810`
- Modify: `crates/templum/src/workspace/bus.rs:234` (a `Submitted` arm, so the app builds)

**Interfaces:**
- Produces: `AgentEvent::Submitted { #[serde(default)] at_enter: bool }` (`{"event":"submitted","at_enter":true}`;
  without `at_enter`, false); `hooks::codex(input: &Value) -> Option<AgentEvent>` for Codex's hook payloads.

- [ ] **Step 1: Write the failing tests.** In `hooks.rs`, Claude Code's `UserPromptSubmit` now gives
  `Some(AgentEvent::Submitted { at_enter: true })` (`:120-123`), Gemini CLI's `BeforeAgent`
  `Some(AgentEvent::Submitted { at_enter: false })` (`:271-274`); `events_go_over_the_bus_as_tagged_json` adds
  `assert_eq!(serde_json::from_value::<AgentEvent>(json!({ "event": "submitted" })).unwrap(),
  AgentEvent::Submitted { at_enter: false });`. Replace `codexs_notify_is_a_finished_turn` with:

```rust
    /// As Codex 0.154 hands them over on stdin (docs/plans/2026-10-16-connections-batch-7.md, S1).
    #[test]
    fn codex_hooks_become_events() {
        let event = |input: Value| codex(&input);
        let turn = "01a14375-fd15-7fb2-b94b-e4a7c7102723";
        let start = json!({ "hook_event_name": "SessionStart", "source": "startup",
            "session_id": "01a14374", "model": "gpt-5.6-luna", "permission_mode": "default" });
        assert_eq!(event(start), Some(AgentEvent::Started));
        assert_eq!(event(json!({ "hook_event_name": "SessionStart", "source": "compact" })), None);
        assert_eq!(
            event(json!({ "hook_event_name": "UserPromptSubmit", "turn_id": turn, "prompt": "also say B" })),
            Some(AgentEvent::Submitted { at_enter: false }),
            "Codex says so once a turn takes the prompt in"
        );
        assert_eq!(
            event(json!({ "hook_event_name": "PermissionRequest", "turn_id": turn, "tool_name": "Bash",
                "tool_input": { "command": "touch f.txt", "description": "May I create f.txt?" } })),
            Some(AgentEvent::NeedsYou { message: Some("Allow Bash?".into()) })
        );
        assert_eq!(
            event(json!({ "hook_event_name": "PostToolUse", "turn_id": turn, "tool_name": "Bash",
                "tool_input": { "command": "sleep 12" }, "tool_use_id": "exec-bb2a" })),
            Some(AgentEvent::Working)
        );
        assert_eq!(
            event(json!({ "hook_event_name": "Stop", "turn_id": turn, "stop_hook_active": false,
                "last_assistant_message": "A B" })),
            Some(AgentEvent::Stopped { message: Some("A B".into()) })
        );
        assert_eq!(
            event(json!({ "hook_event_name": "Interrupt", "turn_id": turn })),
            Some(AgentEvent::Stopped { message: None }),
            "Esc or Ctrl+C: no Stop comes"
        );
        // Also after 30 idle minutes: a snapshot tells when Codex quits.
        assert_eq!(event(json!({ "hook_event_name": "SessionEnd" })), None);
        let notify = json!({ "type": "agent-turn-complete", "last-assistant-message": "x" });
        assert_eq!(event(notify), None, "notify's argument is no hook");
    }
```

  In `cli.rs`'s `a_hook_sends_its_event_prints_nothing_and_never_fails`, replace the Codex part (`:780-788`) with:

```rust
        // Codex's, on stdin. An argument is an older Templum's `notify`: its stdin is Codex's
        // terminal, never read (the next event heard is the stdin one's, not two of them).
        let codex = r#"{"hook_event_name":"Stop","last_assistant_message":"Fixed."}"#;
        let notify = r#"{"type":"agent-turn-complete","last-assistant-message":"Old."}"#;
        assert_eq!(run_with(&["hook", "codex", notify], &env(&bus), codex), (0, String::new(), String::new()));
        assert_eq!(run_with(&["hook", "codex"], &env(&bus), codex), (0, String::new(), String::new()));
        assert_eq!(
            events.recv_timeout(LIMIT).unwrap().1,
            Incoming::Notification {
                method: "templum/event".into(),
                params: json!({ "event": "stopped", "message": "Fixed." }),
            }
        );
```

  (The Gemini CLI event after it must be the next one received: an argument call that read stdin would send a second
  `stopped` first.)
- [ ] **Step 2:** `cargo test -p templum-mcp`. Expected: FAIL to compile (`Submitted`), then on the Codex asserts.
- [ ] **Step 3: Implement.** The variant with its doc: "It took a prompt: a turn starts or takes it in. `at_enter`:
  it says so at its Enter, also for a prompt it queues behind the turn under way (Claude Code); the others once a turn
  takes it." `codex` matches `hook_event_name` as the test lays out (`SessionStart` `compact` → None; `Allow {tool}?`,
  `Allow a tool?` without `tool_name`); its doc names Codex 0.154 and says `SessionEnd` isn't followed. `hook_event`:
  `"codex" if args.len() > 1 => None,` before `"codex" => hooks::codex(&on_stdin()?),`, with the reason as a comment;
  its doc: all three on stdin. `bus.rs`: `AgentEvent::Submitted { .. } => pane.state = Some(AgentState::Working),`
  (Task 3 replaces it). `Pane::speaks`' doc (`mod.rs:181-183`): Codex's hooks too, once its person trusted them.
- [ ] **Step 4:** `cargo test -p templum-mcp && cargo build -p templum`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum-mcp/src/hooks.rs crates/templum-mcp/src/cli.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/mod.rs
git commit -m "feat(connections): Templum reads Codex's hooks: a prompt taken, a permission dialog, a turn's end and Esc"
```

---

### Task 2: The Codex Templum opens gets its hooks instead of `notify`

**Files:**
- Modify: `crates/templum-mcp/src/lib.rs:15-34` (`codex_flags`), test `:40-57`
- Modify: `crates/templum/src/workspace/usage.rs:30-57` (`typed`: its doc and the call at `:52`)
- Test: `crates/templum/src/workspace/tests/usage.rs:38-43`

**Interfaces:**
- Produces: `templum_mcp::codex_flags(exe: &str, windows: bool, hooks: bool) -> Option<String>`; a private
  `const CODEX_HOOKS: [(&str, u8); 6]` (event, timeout) in `lib.rs`.

- [ ] **Step 1: Write the failing tests.** Rename the `lib.rs` test
  `codex_gets_the_server_and_its_hooks_in_single_quotes`; its first call becomes
  `codex_flags("/opt/Templum App/templum", false, true)` and its `notify` assert goes, for:

```rust
        for event in ["SessionStart", "UserPromptSubmit", "PermissionRequest", "PostToolUse", "Stop"] {
            let hook = format!(
                " -c 'hooks.{event}=[{{hooks=[{{type=\"command\",command=\"templum hook codex\",timeout=5}}]}}]'"
            );
            assert!(flags.contains(&hook), "{event}: {flags}");
        }
        // Codex gives an Interrupt hook 3 s at most, and says so at every start otherwise.
        assert!(
            flags.ends_with(
                " -c 'hooks.Interrupt=[{hooks=[{type=\"command\",command=\"templum hook codex\",timeout=3}]}]'"
            ),
            "{flags}"
        );
        assert!(!flags.contains("notify"), "the person's own notify runs");
        // Without `templum` on the terminal's PATH, the server only.
        let bare = codex_flags("/opt/templum", false, false).unwrap();
        assert!(bare.ends_with("tool_timeout_sec=3600'") && !bare.contains("hooks."), "{bare}");
```

  (the Windows, `'` and empty asserts pass `true`). In `tests/usage.rs`, after the `#[cfg(unix)]` Codex assert (the
  test's bus puts `<data>/bin` on `bus_env`'s PATH, `terminal_env`):

```rust
        #[cfg(unix)]
        {
            let line = ws.typed(codex.as_ref(), "codex");
            let stop = r#"-c 'hooks.Stop=[{hooks=[{type="command",command="templum hook codex",timeout=5}]}]'"#;
            assert!(line.contains(stop) && !line.contains("notify"), "{line}");
        }
```

- [ ] **Step 2:** `cargo test -p templum-mcp codex_gets && cargo test -p templum the_agents_templum_opens`. Expected:
  FAIL (no hooks, a `notify`).
- [ ] **Step 3: Implement.** `codex_flags` appends one flag per `CODEX_HOOKS` entry when `hooks`, in the order above;
  its doc says why the command is bare (one trust whatever Templum's path, `<data>/bin` first on PATH) and why
  `Interrupt` is 3. `typed`: `templum_mcp::codex_flags(&exe.to_string_lossy(), windows, on_path)` with
  `let on_path = self.bus_env.iter().any(|(k, _)| k == "PATH");`; its doc says "the server and its hooks".
- [ ] **Step 4:** the two commands of Step 2. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum-mcp/src/lib.rs crates/templum/src/workspace/usage.rs crates/templum/src/workspace/tests/usage.rs
git commit -m "feat(connections): the Codex Templum opens gets its hooks instead of notify, so the person's own notify runs again"
```

---

### Task 3: A turn's end answers a task only once its agent took it

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs:130-138` (`Handed`), `:178-180` (`events` → `submitted`)
- Modify: `crates/templum/src/workspace/blocks.rs:433` (`submitted: 0`)
- Modify: `crates/templum/src/workspace/bus.rs:216-273` (`agent_event`)
- Modify: `crates/templum/src/workspace/delegate.rs:373`, `:422` (`events` → `submitted`), `:491-495`
  (`Handed::new`); `crates/templum/src/workspace/spawn.rs:174-178` (`Handed::new`)
- Test: `crates/templum/src/workspace/tests/delegate.rs`, `tests/bus.rs`, `tests/spawn.rs`

**Interfaces:**
- Consumes: `AgentEvent::Submitted { at_enter }` (Task 1).
- Produces: `Handed { by, since, until, taken: bool, behind: bool }`, `Handed::new(by: &str, timeout: Duration) ->
  Handed` (since now, neither taken nor behind); `Pane::submitted: u64` ("How many prompts its agent took
  (`Submitted`): a task's Enter is confirmed by one more"), replacing `events`.

- [ ] **Step 1: Write the failing tests.** In `tests/delegate.rs`, after `a_turns_end_and_its_timeout_answer_once`:

```rust
/// The turn an agent was on when a task was typed into it ends first, and isn't the task's
/// answer. Codex says it took the task once a turn takes it in; Claude Code says so at its Enter
/// (`at_enter`) and queues it behind the turn under way, which a tool's end takes it into.
#[gpui_kit::test]
fn a_turns_end_before_its_task_was_taken_doesnt_answer_it(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let send = |cx: &mut TestAppContext, e: serde_json::Value| event(handle, &ws, cx, &worker, e);
    let submitted = |at_enter: bool| json!({ "event": "submitted", "at_enter": at_enter });
    let stopped = |message: &str| json!({ "event": "stopped", "message": message });
    let task = json!({ "block": them, "task": "say B", "wait": true });
    // Codex at work on its person's prompt: the turn under way ends before taking the task.
    send(cx, submitted(false));
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task.clone());
    send(cx, stopped("A"));
    assert!(answers.try_recv().is_err(), "the turn it was on");
    send(cx, submitted(false));
    send(cx, stopped("B"));
    assert_eq!(answered(&answers), (false, "B".to_string()));
    // Claude Code queues it at its Enter; the turn under way ends with no tool after it.
    send(cx, submitted(true));
    let answers = call_later(handle, &ws, cx, &terminal, 2, "delegate", task.clone());
    send(cx, submitted(true));
    send(cx, stopped("1 to 300"));
    assert!(answers.try_recv().is_err(), "the turn it was queued behind");
    send(cx, stopped("B"));
    assert_eq!(answered(&answers), (false, "B".to_string()));
    // A tool's end takes it into the turn under way, whose end answers it.
    send(cx, submitted(true));
    let answers = call_later(handle, &ws, cx, &terminal, 3, "delegate", task);
    send(cx, submitted(true));
    send(cx, json!({ "event": "working" }));
    send(cx, stopped("A\nB"));
    assert_eq!(answered(&answers), (false, "A\nB".to_string()));
}
```

  In `tests/bus.rs`, after `a_crashed_agents_state_never_comes_back`:

```rust
/// A command an interrupt left running ends later with Codex's `PostToolUse`: an idle agent stays
/// idle. Only a prompt taken starts a turn; a tool's end after a permission is work again.
#[gpui_kit::test]
fn a_tools_end_starts_no_turn(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, id);
    ws.update(cx, |ws, _| ws.space.terminal_mut(id).unwrap().seen.agent = Some("codex".into()));
    let state = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(id).unwrap().state.clone())
    };
    event(handle, &ws, cx, &terminal, json!({ "event": "working" }));
    assert_eq!(state(cx), None);
    event(handle, &ws, cx, &terminal, json!({ "event": "submitted" }));
    event(handle, &ws, cx, &terminal, json!({ "event": "stopped", "message": null }));
    event(handle, &ws, cx, &terminal, json!({ "event": "working" }));
    assert_eq!(state(cx), Some(AgentState::Idle), "the interrupted turn's command ended");
    let asks = json!({ "event": "needs_you", "message": "Allow Bash?" });
    event(handle, &ws, cx, &terminal, asks);
    event(handle, &ws, cx, &terminal, json!({ "event": "working" }));
    assert_eq!(state(cx), Some(AgentState::Working), "allowed, it works again");
}
```

  Existing tests, so they say what happens now:
  - A task's answer comes after its prompt was taken: send `json!({ "event": "submitted" })` before the task's `stopped`
    in `a_delegation_that_waits_ends_with_the_turns_answer`, `wait_for_idle_waits_for_a_task_whose_wait_timed_out`
    (before `stopped("Done.")`), `a_turns_end_and_its_timeout_answer_once`,
    `a_dot_travels_only_while_a_task_is_handed_over`, `the_dot_goes_with_its_wire_and_the_answer_still_comes`
    (`tests/delegate.rs`), and `a_claude_codes_prompt_is_typed_once_it_started` (`tests/spawn.rs`, before `stopped
    "hi"`).
  - A turn starts with `submitted`, not `working`: `tests/delegate.rs` in
    `wait_for_idle_waits_for_the_turn_and_gives_the_last_answer`,
    `no_enter_goes_to_an_agent_that_started_asking_its_person`, `keys_name_who_pressed_them_for_ten_seconds`, and
    `a_task_confirmed_queued_or_unconfirmable_stays_handed` (`for before in ["none", "started", "submitted"]`, and
    `submitted` after the Enter when `before == "started"`); `tests/bus.rs` in
    `an_agents_hooks_set_its_state_and_its_dot` (both) and `a_crashed_agents_state_never_comes_back`. The `working`
    after a `needs_you` stays (back at work).
  - `Handed { … }` literals in `tests/delegate.rs` become `Handed::new("elsewhere",
    std::time::Duration::from_secs(600))`.
- [ ] **Step 2:** `cargo test -p templum a_turns_end_before && cargo test -p templum a_tools_end_starts`. Expected:
  FAIL ("the turn it was on": answered "A"; state Working).
- [ ] **Step 3: Implement `agent_event`'s three arms** (the rest as it is):

```rust
            AgentEvent::Submitted { at_enter } => {
                let turn = pane.state == Some(AgentState::Working);
                pane.state = Some(AgentState::Working);
                pane.submitted += 1;
                // Its task, once typed: a spawn's prompt waits in `prompt` until then.
                let typed = pane.prompt.is_none();
                if let Some(handed) = pane.handed.as_mut().filter(|h| typed && !h.taken) {
                    handed.taken = true;
                    // Claude Code says so at its Enter, also for a prompt it queues behind its turn.
                    handed.behind = turn && at_enter;
                }
            }
            AgentEvent::Working => {
                // At work again after a permission. Never a new turn: a command an interrupt left
                // running ends later (Codex).
                if matches!(pane.state, Some(AgentState::Working | AgentState::NeedsYou(_))) {
                    pane.state = Some(AgentState::Working);
                }
                // A tool's end takes the prompt Claude Code queued into the turn under way.
                if let Some(handed) = pane.handed.as_mut() {
                    handed.behind = false;
                }
            }
```

  `Stopped` keeps the answer (`pane.answer`) and sets Idle as now, then, when `pane.handed` is Some and not taken or
  `behind`, clears `behind` and answers nothing (the turn it was typed in on, or the one Claude Code queued it behind;
  the next turn's end is its own); otherwise `end_turn` as now. `Handed`'s new fields carry their doc from Decision 2.
- [ ] **Step 4:** `cargo test -p templum workspace::tests`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/mod.rs crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/tests/delegate.rs crates/templum/src/workspace/tests/bus.rs crates/templum/src/workspace/tests/spawn.rs
git commit -m "fix(connections): a task typed into a busy agent is answered by the turn that took it, not the one it was on"
```

---

### Task 4: Only a prompt taken confirms a task; no second Enter into a dialog or the person's typing

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs:26-36` (consts), `:110-185` (`impl Pane`: `tail`,
  `person_typed`), `:328-336` (`press_keys`' screen through `tail`), `:352-452` (`send_task`, `confirm`, `not_taken`)
- Modify: `crates/templum/src/workspace/mod.rs:173-175` (`Pane::keyed` after `typed`), `blocks.rs:430`
- Modify: `crates/templum/src/workspace/input.rs:432`, `:749`, `:978` (`pane.person_typed()`)
- Test: `crates/templum/src/workspace/tests/delegate.rs:1284-1334`

**Interfaces:**
- Consumes: `Pane::submitted` (Task 3).
- Produces: `pub(super) const TAIL_LINES: usize = 20` (renames `KEYS_LINES`: the screen's last lines Templum answers
  with); `pub(super) const TYPING: Duration = 3 s` (the person is still typing); `Pane::keyed:
  Cell<Option<Instant>>` (the person's last key, paste or text into it); `Pane::person_typed(&self)` (sets `typed`
  and `keyed`); `Pane::tail(&self) -> String` (its last `TAIL_LINES` lines, joined); `confirm(&mut self, terminal:
  &str, submitted: u64, queued: bool, again: Option<&str>, task: bool, cx: &mut Context<Self>) -> bool`.

- [ ] **Step 1: Write the failing tests.** `a_task_no_hook_confirms_gets_its_enter_again_then_ends`: after `enter(cx)`,
  `screen_until(&ws, cx, theirs, |s| s.matches("think").count() >= 2);` (its screen settled), and its last assert also
  checks `text.contains("think")` (the screen comes with it). Then, after it:

```rust
/// A turn's end (Claude Code's idle prompt comes a minute after its last turn) and a screen its
/// looks take for one at work confirm nothing: only the prompt taken does, and its turn's end then
/// answers the task.
#[cfg(unix)]
#[gpui_kit::test]
fn only_its_prompt_taken_confirms_a_task(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let confirm = crate::workspace::delegate::CONFIRM.as_millis() as u64;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    event(handle, &ws, cx, &worker, json!({ "event": "started" }));
    let task = json!({ "block": them, "task": "think", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    screen_with(&ws, cx, theirs, "think");
    enter(cx);
    screen_until(&ws, cx, theirs, |s| s.matches("think").count() >= 2);
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": null }));
    ws.update(cx, |ws, _| ws.space.terminal_mut(theirs).unwrap().state = Some(AgentState::Working));
    after(cx, 2 * confirm);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("didn't take the task"), "{text}");
    // Idle again, its prompt taken: confirmed, and its turn's end answers the task.
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": null }));
    let task = json!({ "block": them, "task": "ponder", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 2, "delegate", task);
    screen_with(&ws, cx, theirs, "ponder");
    enter(cx);
    event(handle, &ws, cx, &worker, json!({ "event": "submitted", "at_enter": true }));
    after(cx, 2 * confirm);
    assert!(answers.try_recv().is_err(), "taken: it works on it");
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": "Pondered." }));
    assert_eq!(answered(&answers), (false, "Pondered.".to_string()));
}

/// No word 3 s after the Enter: a screen that changed since just after it drew something no hook
/// tells of (a trust question), and the person typing there would send their own half line with
/// a second Enter. Neither gets one: the task ends at that look, with the screen as it is.
#[cfg(unix)]
#[gpui_kit::test]
fn no_second_enter_into_a_changed_screen_or_while_the_person_types(cx: &mut TestAppContext) {
    let confirm = crate::workspace::delegate::CONFIRM.as_millis() as u64;
    let enter_after = crate::workspace::delegate::ENTER_AFTER.as_millis() as u64;
    for case in ["a question drew", "the person types"] {
        let (handle, ws, mine, theirs) = two_agents(cx);
        let (terminal, _) = me(&ws, cx, mine);
        let (worker, them) = me(&ws, cx, theirs);
        event(handle, &ws, cx, &worker, json!({ "event": "started" }));
        let task = json!({ "block": them, "task": "think", "wait": true });
        let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
        screen_with(&ws, cx, theirs, "think");
        enter(cx);
        screen_until(&ws, cx, theirs, |s| s.matches("think").count() >= 2);
        // The screen just after the Enter, which a second one would need unchanged.
        after(cx, enter_after);
        if case == "a question drew" {
            shows(&ws, cx, theirs, "Trust these hooks?");
        } else {
            ws.read_with(cx, |ws, _| ws.space.terminal(theirs).unwrap().person_typed());
        }
        after(cx, confirm);
        // Answered at the first look: a second Enter would have waited 3 s more.
        let (failed, text) = answered(&answers);
        assert!(failed && text.contains("didn't take the task"), "{case}: {text}");
        if case == "a question drew" {
            assert!(text.contains("Trust these hooks?"), "the screen as it is: {text}");
        }
    }
}
```

  (`shows` and `screen_until` are in scope: `shows` is in this file, `screen_until` comes from `super::bus` — add it to
  the `use`.)
- [ ] **Step 2:** `cargo test -p templum only_its_prompt && cargo test -p templum no_second_enter && cargo test -p
  templum a_task_no_hook`. Expected: FAIL (the moving screen confirmed the first task; a second Enter went; no screen
  in the answer).
- [ ] **Step 3: Implement.** `send_task` notes `queued = pane.state == Some(AgentState::Working)` before it types;
  its Enter loop gives `Some(pane.speaks.then_some(pane.submitted))`; then, for a speaking agent: `ENTER_AFTER` later
  it keeps `pane.screen()` as `entered`, `CONFIRM - ENTER_AFTER` later it calls `confirm(…, Some(&entered), …)`, and,
  when that isn't done, `CONFIRM` later `confirm(…, None, …)`. `confirm`: done (true) when its pane is gone,
  `pane.submitted != submitted`, `queued`, or `takes_typing` refuses (a dialog opened); else, with `again` equal to
  `pane.screen()` and `pane.keyed` older than `TYPING` (or None), it writes `\r` and returns false; else `not_taken`
  for a task, and true. Its doc follows the spec's **Confirmed**. `not_taken`'s message gains `" The last lines of its
  screen:\n{tail}"` (`tools::tail(&pane.tail(), tools::MAX_TEXT)`). `CONFIRM`'s doc: only a prompt taken. The three
  `input.rs` sites call `pane.person_typed()` instead of `pane.typed.set(true)` (`press_keys` keeps `typed.set`).
- [ ] **Step 4:** `cargo test -p templum workspace::tests::delegate`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/mod.rs crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/input.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "fix(connections): only a prompt the agent took confirms a task, and no second Enter goes into a dialog or what the person types"
```

---

### Task 5: A task's first line goes as a paste of its own

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs:352-358` (`send_task` types), `:488` (`delegate`'s text),
  `:547` (`reply_to`'s text)
- Modify: `crates/templum/src/workspace/spawn.rs:121-176` (`open_agent`: the prompt's text)
- Test: `crates/templum/src/workspace/tests/delegate.rs:96-126`, `:212-229`

**Interfaces:**
- Produces: `pub(super) const TASK_LEAD: &str = "Your task is in the pasted text: do it."` in `delegate.rs`.

- [ ] **Step 1: Write the failing tests.** `delegate_types_the_header_the_task_and_enter`'s last needle becomes
  `format!("({uid})] Your task is in the pasted text: do it. fix the tests then push[from")`;
  `reply_with_no_one_waiting_types_into_the_agent`'s `format!("({them})] Its reply is in the pasted text. Done.[reply
  from")`. After the first:

```rust
/// Claude Code takes a long paste alone for data it was shown, not for what it is asked: the
/// first line goes as a paste of its own, which it reads as typed.
#[cfg(unix)]
#[gpui_kit::test]
fn a_task_goes_as_its_first_line_then_its_text(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    // `cat` prints the sequence back: the terminal now brackets pastes, as Claude Code's does.
    prints(&ws, cx, theirs, b"\x1b[?2004hon\r");
    screen_with(&ws, cx, theirs, "2004honon");
    let task = json!({ "block": them, "task": "line 1\nline 2\nline 3\nline 4" });
    call(handle, &ws, cx, &terminal, "delegate", task);
    // The terminal echoes ESC as "^[": two pastes, the first line alone.
    screen_with(
        &ws,
        cx,
        theirs,
        &format!("({uid})] Your task is in the pasted text: do it.^[[201~^[[200~"),
    );
}
```

- [ ] **Step 2:** `cargo test -p templum delegate_types_the_header a_task_goes_as reply_with_no_one`. Expected: FAIL.
- [ ] **Step 3: Implement.** `send_task` types `text` up to its first `\n` with `type_in`, then `\n` and the rest with
  another `type_in` (one call when there is no `\n`), the reason in a comment (4 lines or 800 characters). `delegate`:
  `format!("[from {}] {TASK_LEAD}\n{task}", …)`; `reply_to`: `format!("[reply from {}] Its reply is in the pasted
  text.\n{text}", …)`; `open_agent` keeps the prompt as `format!("[from {}] {TASK_LEAD}\n{prompt}",
  self.signature(space, me, cx))`, the caller's signature taken before `in_space`.
- [ ] **Step 4:** `cargo test -p templum workspace::tests::delegate && cargo test -p templum workspace::tests::spawn`.
  Expected: PASS (`said_twice` still finds "say hi" twice).
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "fix(connections): Claude Code does a long task it is handed instead of taking it for pasted data"
```

---

### Task 6: A spawned agent's prompt waits for its input box

**Files:**
- Modify: `crates/templum-terminal/src/terminal.rs:186-193` (`Modes`), `:214-221` (`modes`), test `:974-990`
- Modify: `crates/templum/src/workspace/mod.rs:140-147` (`Prompt::gated`)
- Modify: `crates/templum/src/workspace/spawn.rs:170-185` (`open_agent`), `:226-258` (`watch_ready`, `look_ready`)
- Modify: `crates/templum/src/workspace/bus.rs:235-242` (`Started`: Idle only)
- Test: `crates/templum/src/workspace/tests/spawn.rs`; `tests/delegate.rs:848` (`gated: false`)

**Interfaces:**
- Consumes: `TASK_LEAD` and the prompt's text (Task 5); `Pane::tail` (Task 4).
- Produces: `templum_terminal::Modes::typing: bool` ("It reads typed input: it brackets pastes and shows its cursor.
  Claude Code and Codex hide it in their dialogs"); `Prompt::gated: bool` (its agent is Claude Code or Codex, whose
  prompt waits for `typing`).

- [ ] **Step 1: Write the failing tests.** `modes_say_what_the_program_asked_for` gains `typing: false` in its literal
  and ends:

```rust
        // Bracketed pastes and a shown cursor: its input box. Claude Code and Codex hide the cursor
        // in their dialogs.
        let (term, _) = headless(10, 2);
        feed(&term, b"\x1b[?2004h");
        assert!(term.modes().typing);
        feed(&term, b"\x1b[?25l");
        assert!(!term.modes().typing, "a dialog");
        feed(&term, b"\x1b[?25h\x1b[?2004l");
        assert!(!term.modes().typing, "no bracketed pastes");
```

  In `tests/spawn.rs`: `spawner` sets `ws.command = Some(jobs("printf '\\033[?2004h'; cat; true"))` (its agents'
  input boxes bracket pastes); a helper `fn prints(ws, cx, id, bytes: &'static [u8])` writing `bytes` into the
  terminal (`cat` prints them back) and `fn found(ws, cx, id)` setting `seen.pid` to its foreground process, as
  `tests/delegate.rs`'s are. Replace `a_claude_codes_prompt_is_typed_once_it_started` with:

```rust
/// A spawned Claude Code or Codex first asks whether to trust its folder (Codex also its hooks),
/// its cursor hidden: its prompt, whose Enter would answer for the person, waits until it brackets
/// pastes and shows its cursor, as its input box does. No hook speaks here: a Claude Code started
/// without Templum's hooks gets its prompt too.
#[gpui_kit::test]
fn a_prompt_waits_for_the_agents_input_to_take_pastes(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    in_front(&ws, cx, new, true);
    found(&ws, cx, new);
    let (theirs, uid) = me(&ws, cx, new);
    let idle = json!({ "block": uid });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle);
    let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
    let prompt = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().prompt.is_some())
    };
    // Its trust question hides the cursor; and an input that doesn't bracket pastes isn't its box.
    for bytes in [&b"\x1b[?25l\r"[..], b"\x1b[?25h\x1b[?2004l\r"] {
        prints(&ws, cx, new, bytes);
        for _ in 0..4 {
            std::thread::sleep(Duration::from_millis(100));
            after(cx, look);
        }
        assert!(prompt(cx), "typed into a question: {bytes:?}");
    }
    prints(&ws, cx, new, b"\x1b[?2004h\r");
    let mut looks = 0;
    while prompt(cx) {
        assert!(looks < 10, "never typed");
        std::thread::sleep(Duration::from_millis(100));
        after(cx, look);
        looks += 1;
    }
    screen_with(&ws, cx, new, "say hi");
    enter(cx);
    said_twice(&ws, cx, new);
    event(handle, &ws, cx, &theirs, json!({ "event": "submitted", "at_enter": true }));
    event(handle, &ws, cx, &theirs, json!({ "event": "stopped", "message": "hi" }));
    assert_eq!(answered(&waiting), (false, "hi".to_string()));
}

/// A minute after its spawn its person still hasn't answered what its agent asks: the prompt is
/// dropped, and its task ends showing the question.
#[gpui_kit::test]
fn a_prompt_not_typed_within_a_minute_ends_its_task_with_its_screen(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    in_front(&ws, cx, new, true);
    found(&ws, cx, new);
    let (_, uid) = me(&ws, cx, new);
    prints(&ws, cx, new, b"Trust these hooks?\x1b[?25l\r");
    screen_with(&ws, cx, new, "Trust these hooks?");
    let idle = json!({ "block": uid });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle);
    ws.update(cx, |ws, _| {
        let handed = ws.space.terminal_mut(new).unwrap().handed.as_mut().unwrap();
        handed.since -= crate::workspace::spawn::STARTS_WITHIN;
    });
    after(cx, crate::workspace::spawn::READY_LOOK.as_millis() as u64);
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(new).unwrap();
        assert!(pane.prompt.is_none() && pane.handed.is_none());
    });
    let (failed, text) = answered(&waiting);
    assert!(
        failed
            && text.starts_with(&format!("{uid} never got its prompt"))
            && text.contains("Trust these hooks?"),
        "{text}"
    );
}
```

  `a_snapshot_before_the_agent_started_doesnt_stop_its_prompt`: instead of the `started` event, a second `snapshot`
  finds it (`Inspection { agent: Some("claude".into()), pid: <its foreground>, .. }`) and the test looks (`after(cx,
  look)`, as above) until the prompt is typed; its doc says "once its input is ready".
  `a_prompt_the_agent_didnt_take_ends_at_once`: its command `jobs("stty -echo; printf '\\033[?2004h'; echo ready; cat;
  true")`, `found` instead of the `started` event, then the same `after(cx, 5000)`.
- [ ] **Step 2:** `cargo test -p templum-terminal modes_say && cargo test -p templum workspace::tests::spawn`.
  Expected: FAIL (`typing` unknown; Claude Code's prompt never typed without `started`; no deadline).
- [ ] **Step 3: Implement.** `Modes::typing = mode.contains(TermMode::BRACKETED_PASTE | TermMode::SHOW_CURSOR)`.
  `open_agent` sets `Prompt { text, screen: None, gated }` with `gated` true for `Agent::CLAUDE` and `Agent::CODEX`
  (`entry.builtin`), and calls `watch_ready` for every spawn with a prompt (its comment: why, from Decision 6). The
  `Started` arm only sets Idle. `look_ready`, in order: no pane or no prompt → false; the task handed `STARTS_WITHIN`
  ago or more (`handed.since.elapsed()`) → drop the prompt and `end_turn` with `Reply::err("{uid} never got its
  prompt: a minute after it opened, it still asks its person something (trusting its folder, Codex's hooks), or its
  input wasn't ready. Look at its block. The last lines of its screen:\n{tail}")`, false; not seen, a screen unlike
  the last look's, or `gated && !cfg!(windows) && !typing` → keep the screen, true; else type it (as now), false. Its
  doc and `watch_ready`'s say the minute.
- [ ] **Step 4:** `cargo test -p templum-terminal && cargo test -p templum workspace::tests`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum-terminal/src/terminal.rs crates/templum/src/workspace/mod.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/tests/spawn.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "fix(connections): a spawned agent's prompt waits for its input box, never answering its trust questions, and any agent gets it without hooks"
```

---

### Task 7: A hooked agent gone still seems stuck, and keeps its task

**Files:**
- Modify: `crates/templum/src/workspace/activity.rs:15-75` (consts, `Looks`), `:82-127` (`look_at_agents`); unit tests
  `:130-227`
- Test: `crates/templum/src/workspace/tests/delegate.rs` (after `a_dialog_stays_yellow_until_its_person_answers`)

**Interfaces:**
- Consumes: `Pane::speaks`; `Workspace::needs_you` (`waits.rs:442`).
- Produces: `pub(super) const STUCK: u32 = 30`; `pub(crate) const STUCK_TEXT: &str = "it seems stuck, its screen still
  for 30 seconds"`; `Looks { hash, changes, still: u8, answered }` (`still`: looks in a row that saw it still, `count`
  goes); `Looks::state(&self, state: &Option<AgentState>, speaks: bool) -> Option<AgentState>`.

- [ ] **Step 1: Write the failing tests.** The unit tests pass `false` to `state` (an agent without hooks), but
  `a_hooks_event_starts_the_looks_over` and `a_dialog_ends_only_once_its_person_typed` pass `true`. Add:

```rust
    #[test]
    fn a_hooked_agent_still_for_thirty_looks_seems_stuck() {
        let working = Some(AgentState::Working);
        let stuck = Some(AgentState::NeedsYou(Some(STUCK_TEXT.into())));
        let mut looks = saw(&[true]);
        more(&mut looks, &[false; 29]);
        assert_eq!(looks.state(&working, true), None, "its hooks spoke: at work for 29");
        more(&mut looks, &[false]);
        assert_eq!(looks.state(&working, true), stuck);
        assert_eq!(saw(&[true, false, false, false, false, false]).state(&working, false), Some(AgentState::Idle));
        // The person typed since its last hook (an Esc while Claude Code writes): at rest after 5.
        let mut looks = saw(&[]);
        looks.skip(looks.hash + 1, true);
        more(&mut looks, &[false; 5]);
        assert_eq!(looks.state(&working, true), Some(AgentState::Idle));
        // Moving again: at work, without the person's keys.
        assert_eq!(saw(&[true, true, true]).state(&stuck, true), working);
    }
```

  In `tests/delegate.rs`:

```rust
/// Its hooks spoke, and its screen went still while it worked (a dialog no hook tells of, a hang):
/// it seems stuck, waits for its person, keeps its task, and works again once its screen moves.
#[cfg(unix)]
#[gpui_kit::test]
fn a_hooked_agent_gone_still_keeps_its_task_and_says_so(cx: &mut TestAppContext) {
    use crate::workspace::{AgentState, activity::STUCK_TEXT};
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "think", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    screen_with(&ws, cx, theirs, "think");
    enter(cx);
    // Its screen settled before the looks start counting.
    screen_until(&ws, cx, theirs, |s| s.matches("think").count() >= 2);
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
    let asks = json!({ "block": them, "event": "needs_you" });
    let needs_you = call_later(handle, &ws, cx, &terminal, 2, "wait_for", asks);
    for _ in 0..29 {
        after(cx, 1000);
    }
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working), "29 still looks");
    after(cx, 1000);
    after(cx, 300);
    let stuck = Some(AgentState::NeedsYou(Some(STUCK_TEXT.into())));
    assert_eq!(state_of(&ws, cx, theirs), stuck);
    assert!(answers.try_recv().is_err(), "its task stays handed over");
    let (failed, text) = answered(&needs_you);
    assert!(!failed && text.contains("it seems stuck"), "{text}");
    ticks(&ws, cx, theirs, 0, 4);
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working));
}
```

- [ ] **Step 2:** `cargo test -p templum seems_stuck && cargo test -p templum a_hooked_agent_gone`. Expected: FAIL (no
  `STUCK_TEXT`; then Idle at 5 looks and the task ended).
- [ ] **Step 3: Implement.** `see` counts `still` (0 on a change, +1 saturating otherwise); `state`: Working →
  `NeedsYou(Some(STUCK_TEXT))` when `speaks && !self.answered && still >= STUCK`; Working → Idle when `(!speaks ||
  self.answered) && still >= STILL`; `NeedsYou(Some(m))` with `m == STUCK_TEXT` and changing → Working; the other
  arms as now. `look_at_agents` passes `pane.speaks`, collects the terminals turned stuck and calls
  `self.needs_you(&terminal, Some(STUCK_TEXT), cx)` for each after the loop; its doc and the module's say the 30
  looks.
- [ ] **Step 4:** `cargo test -p templum workspace::activity && cargo test -p templum workspace::tests::delegate`.
  Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/activity.rs crates/templum/src/workspace/mod.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): an agent with hooks whose screen stays still while it works seems stuck, and keeps its task"
```

---

### Task 8: A dialog an agent waits on comes back as drawn

**Files:**
- Modify: `crates/templum/src/workspace/waits.rs:404-461` (`wait_for`, `needs_you`), `:533-539`
  (`waiting_for_person`)
- Modify: `crates/templum/src/workspace/delegate.rs:166-170` (`takes_typing`'s wait for the person)
- Modify: `crates/templum/src/workspace/bus.rs:541-550` (`block_lines`' state)
- Test: `crates/templum/src/workspace/tests/delegate.rs:618-657`, `tests/bus.rs:692-697`

**Interfaces:**
- Consumes: `Pane::tail` (Task 4).
- Produces: `const DRAWN: Duration = Duration::from_millis(300)` in `waits.rs` (a dialog's hook comes before it is
  drawn); `waiting_for_person(uid: &str, what: Option<&str>, screen: &str) -> Reply`.

- [ ] **Step 1: Write the failing tests.** `wait_for_needs_you_now_or_when_it_asks`: `after(cx, 300)` before each
  `answered`, and its equalities become `starts_with` checks of the same texts.
  `an_agents_hooks_set_its_state_and_its_dot`: the `list_blocks` line contains `"waiting for its person: Allow Bash?"`.
  New, after the first:

```rust
/// The dialog an agent waits on, as drawn: what answers it with `send_keys` reads its options.
/// Its hook comes before it is drawn (the hook may answer it).
#[cfg(unix)]
#[gpui_kit::test]
fn wait_for_needs_you_shows_the_dialog_as_drawn(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let needs_you = json!({ "block": them, "event": "needs_you" });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "wait_for", needs_you.clone());
    event(handle, &ws, cx, &worker, json!({ "event": "needs_you", "message": "Allow Bash?" }));
    shows(&ws, cx, theirs, "1. Yes, proceed (y)");
    after(cx, 300);
    let (failed, text) = answered(&answers);
    assert!(
        !failed
            && text.starts_with(&format!("{them} is waiting for its person: Allow Bash?"))
            && text.contains("1. Yes, proceed (y)"),
        "{text}"
    );
    let (failed, text) = call(handle, &ws, cx, &terminal, "wait_for", needs_you);
    assert!(!failed && text.contains("1. Yes, proceed (y)"), "at once too: {text}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn delegate_into_a_dialog_is_refused_with_its_screen(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    event(handle, &ws, cx, &worker, json!({ "event": "needs_you", "message": "Allow Bash?" }));
    shows(&ws, cx, theirs, "3. No, and tell Codex what to do differently (esc)");
    let task = json!({ "block": them, "task": "go" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(
        failed
            && text.contains("waiting for its person: Allow Bash?")
            && text.contains("send_keys")
            && text.contains("3. No, and tell Codex"),
        "{text}"
    );
}
```

- [ ] **Step 2:** `cargo test -p templum as_drawn && cargo test -p templum refused_with_its_screen && cargo test -p
  templum an_agents_hooks_set`. Expected: FAIL.
- [ ] **Step 3: Implement.** `waiting_for_person` appends `" Its screen:\n{tools::tail(screen, tools::MAX_TEXT)}"`.
  `wait_for`'s at-once answer passes `pane.tail()`. `needs_you` reads the screen `DRAWN` later: a `cx.spawn` that
  waits `DRAWN`, then answers the `wait_for(needs_you)` on that terminal still waiting (a balloon's question goes the
  same way). `takes_typing`: "{uid} is waiting for its person: {what}. Templum won't type into it now; send_keys
  answers its dialog. Its screen:\n{tail}" (no `: {what}` when it says none). `block_lines`: `", waiting for its
  person: {clean_title(what)}"` when it says what.
- [ ] **Step 4:** `cargo test -p templum workspace::tests`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/waits.rs crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/tests/delegate.rs crates/templum/src/workspace/tests/bus.rs
git commit -m "feat(connections): a dialog an agent waits on comes back as drawn, for the agent that answers it"
```

---

### Task 9: The spec's status, the minors, the full check, the benches, push

**Files:** `docs/specs/connections.md`, `docs/minors.md`

- [ ] **Step 1: The spec:** `- **Status:** Batches 1 to 7 implemented; 8 (orchestration) specified`; anything the
  tasks changed from this plan's text (a message's wording) goes into the paragraph that says it.
- [ ] **Step 2: `docs/minors.md`, ## Connections.** Delete, as fixed: "Codex's dot never turns yellow.", "The
  person's own Codex `notify` doesn't run in a Codex Templum opens.", "A spawned Claude Code without Templum's hooks
  never gets its prompt.", "A task handed to an agent at work is answered by the turn it was on.", "An agent with
  hooks that works with a still screen ends its delegated task.", "Codex's tasks aren't confirmed.", "A second Enter
  may send what the person was typing.". Add "Left by batch 7; line numbers are those of `development` at <Task 8's
  commit>." with (plus whatever the reviews defer):
  - **A task typed into a working Codex whose Enter joined the paste waits for its timeout.** `confirm`
    (`crates/templum/src/workspace/delegate.rs`) takes an agent at work as having queued the task, since Codex says
    it took it only when a turn does; if its Enter was lost, the text sits in its composer and the agent that handed
    it waits 600 s. Fix: when the turn it was on ends (`Stopped` with the task not taken, `bus.rs`), give it its
    3 seconds then.
  - **The first task into a Codex that took no prompt yet isn't confirmed.** `Pane::speaks` is set by a hook, and a
    hooked Codex's first one comes with its first prompt (its `SessionStart` too), so `send_task` gives that task no
    confirmation and no second Enter. Fix: set `speaks` when `typed()` gave Codex its hooks and its person trusted
    them (none says so before a prompt: keep as is unless it bites).
  - **A Codex whose person trusted only some of its hooks never answers a task.** Codex's review trusts hook by hook;
    without `UserPromptSubmit`, `Stopped` (`bus.rs`) never finds the task taken, and the task waits for its timeout.
    Fix: take a `Stopped` from an agent whose `submitted` is still 0 as the task's.
  - **Gemini CLI's `BeforeAgent` for a prompt typed while it works is unchecked.** `hooks::gemini`
    (`crates/templum-mcp/src/hooks.rs`) takes it as Codex's (`at_enter: false`); if Gemini CLI fires it at the Enter
    and queues the prompt, a turn under way would answer the task. Fix: check by hand once logged in; `at_enter:
    true` if so.
  - **On Windows a spawned Claude Code or Codex may get its prompt in its trust question.** `look_ready`
    (`crates/templum/src/workspace/spawn.rs`) skips the input check there (ConPTY may not pass bracketed paste and the
    cursor on), so a still trust question takes the prompt and its Enter. Fix: check by hand on Windows whether
    `Modes::typing` follows; if so, drop the `cfg!(windows)`.
  - **The person's own prompt queued in Claude Code shows idle for a few seconds.** After the turn it was queued
    behind ends (`Stopped`, `bus.rs`), no hook says the next turn starts; the dot is idle until the screen's looks see
    it work (about 3 s). Fix: keep `Working` on a `Stopped` that comes while a prompt Claude Code queued is pending
    (a pane-level `behind`).
- [ ] **Step 3: The full check:** `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo
  test --workspace`, then `cargo deny check`. Expected: all pass; report the test count.
- [ ] **Step 4: The benches** (Linux, release): `cargo run --release -p templum -- --bench` and `cargo run --release
  -p templum -- --bench-mixed`; keep both JSON lines. Expected: `p95_ms` ≤ 16.7 each.
- [ ] **Step 5: Commit and push.**

```bash
git add docs/specs/connections.md docs/minors.md
git commit -m "docs(connections): batch 7 implemented; its minors"
git push origin development
```

  The push updates draft PR #36 (dev→main), which runs CI on the 3 OS (`gh pr checks 36`): add batch 7 to its title
  and description, with the two bench JSON lines.

## Manual checks (Hyprland, `cargo run --release -p templum`, the user's bash with ble.sh)

1. **Codex's hooks:** Codex from the canvas menu in a new folder: the folder's question, then "Hooks need review"
   once (none if this Codex trusted them in the spikes: then check `/hooks` lists "Session flags" active); its dot
   turns accent within a second of a prompt; a command needing approval (`codex -s read-only`, "touch a file") turns
   it yellow "Waiting for you: Allow Bash?" as the dialog opens; Esc during a `sleep 30` ends Working at once, and the
   dot stays idle when the `sleep` ends; quit and restart Templum: no review asked.
2. **A spawned Codex's questions:** Claude Code asks "spawn a codex in /tmp/<new folder> with the prompt 'say hi'":
   nothing is typed while Codex asks to trust the folder and its hooks; Claude Code answers them with `send_keys`
   (`["enter"]`, then `["2"]`), "answered by" shows, then the prompt goes and Claude Code hears "hi". Left
   unanswered a minute: the task ends showing the question.
3. **A task into a working agent:** Codex at work on a `sleep 20` of the person's: Claude Code delegates "say B" with
   `wait`; the answer is the turn that took it (it says B), never the person's turn's end alone. The same into a
   Claude Code at work.
4. **Confirmation:** a task into an idle Claude Code goes in once (no second Enter); typing into its box right after
   its Enter (the task's text now joined by a newline, e.g. after a resize): no second Enter sends it.
5. **A long task into Claude Code** with its default model (Opus or Sonnet): a 10-line task is done, its first line
   shown as typed and the rest as "[Pasted text #2 +10 lines]".
6. **Seems stuck:** `kill -STOP` a working Codex's process for 35 s: its dot turns yellow "Waiting for you: it seems
   stuck, …", `templum wait <block> --for needs_you` prints it with its screen; `kill -CONT`: Working again. Esc
   while Claude Code writes: idle within about 5 s, never yellow.
7. **Dialogs as drawn:** `templum wait <block> --for needs_you` on a Claude Code at a Bash permission prints the
   dialog's options; `delegate` into it is refused with them.
8. **Benches** pasted in PR #36 (Task 9).

## Self-review

- Spec coverage: Codex's hooks and their events (Tasks 1, 2), `Submitted` deciding which turn answers (3), Confirmed
  (4), the first line (5), Spawning's readiness and its minute (6), seems stuck (7), dialogs as drawn in `wait_for`,
  the refusal and `list_blocks` (8), the status and the minors (9). Testing "Batch 7": every item has its test
  (Tasks 1-8). Delegating 10's notice is batch 8's.
- Types: `AgentEvent::Submitted { at_enter }` (1) used by 3, 6; `codex_flags(_, _, hooks)` (2); `Handed::new`,
  `taken`, `behind`, `Pane::submitted` (3) used by 4; `Pane::{keyed, person_typed, tail}`, `TAIL_LINES`, `TYPING` (4)
  used by 6, 8; `TASK_LEAD` (5) used by 6's prompt text; `Modes::typing`, `Prompt::gated` (6); `STUCK_TEXT` (7)
  reaches `needs_you` (8 adds the screen to its answer: Task 7's test reads `contains`).
- The planning pass's tests renamed or dropped: `a_claude_code_without_hooks_gets_its_prompt_from_its_screen` and
  `a_hooked_codexs_prompt_waits_for_its_session` are covered by `a_prompt_waits_for_the_agents_input_to_take_pastes`
  and `a_prompt_not_typed_within_a_minute_ends_its_task_with_its_screen` (Decision 6).
