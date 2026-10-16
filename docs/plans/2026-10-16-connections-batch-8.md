# Connections, batch 8 (Async): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

## Context

Batches 1 to 7 of [docs/specs/connections.md](../specs/connections.md) are on `development` (a429ef2). Batch 8 lets an
agent hand work over and keep working, as Claude Code's own subagents do: every task gets an id (`t7`), its end is
typed back into the agent that handed it (time, what it spent, transcript, answer), a dialog it waits on is told too,
the asker can change its task while it runs, and a task past its timeout comes back as failed. Batch 7's spikes (S2,
S3: how Claude Code and Codex take a prompt typed while they work) already answer how a change goes in; one question
was left, what a task spent and when its agent has written it, and the spike below answered it and changed the design
(Decision 4). **This plan's commit already corrected the spec's batch-8 text** with it and with the copy the agents
read (Delegating 2, 4, 8, 10, 11, 12, Spawning, the Tools rows, the transcript after the hooks tables, the `delegate`
verb's row, Files, the Architecture row, Testing "Batch 8", Delivery, Risks); Task 11 only flips its status and the
minors.

**Goal:** `Handed::id` and `t7` in `delegate`'s and `spawn_agent`'s results, in the task's first line and in
`reply`'s; `AgentEvent::Stopped`'s `transcript`; a queue of notices (`Workspace::notices`) typed into the asker when it
can take them, never into a shell; the notice of a task's end (done or failed, its time, what it spent, its transcript,
its answer), of a dialog in a task, and of a task's timeout; a change to a task from its asker; the MCP instructions'
sentence and the tools' descriptions.

**Architecture:** `templum-mcp` passes the hook's `transcript_path` in `Stopped`. In the app, `delegate.rs` numbers
tasks (`Workspace::next_task`), turns every task's end (`end_turn`) into a notice unless a call of the asker waits for
it (`tell_end`), reads what the task spent a second later (`templum_usage::read` again, through `usage.rs`'s
`proc_of`), and keeps the notices in `Workspace::notices`, typed with `send_task` (`deliver_notices`) as each notice
comes and at each look (`activity::LOOK`), while the asker takes them (`Pane::takes_notice`). `waits.rs::needs_you`
tells a task's dialog; a timer per task (`time_out`) tells its timeout; `delegate` from the task's asker is a change;
`blocks.rs::remove_block` ends a closed block's task.

**Tech Stack:** Rust 2024; GPUI (gpui-fast 0.1.8) through gpui-kit `=0.7.3`, no new GPUI API (`cx.spawn`,
`background_executor().timer` and `.spawn`, as `send_task` and `read_usage` use them); `templum_usage::{read, Dirs,
Proc, Session}`; serde. Agents checked on 2026-10-16: Claude Code 2.1.286 (spike below), Codex 0.154.0 (batch 7's
logs); Gemini CLI 0.63.0 has no login here (not run).

**Spec:** [docs/specs/connections.md](../specs/connections.md) at this plan's commit: Delegating 2, 4, 5, 7, 8, 10, 11,
12; Spawning (the prompt's first line and the result); The MCP server (the batch-8 sentence of the instructions); the
Tools rows `spawn_agent`, `delegate`, `reply`; the paragraph after Codex's hooks table (the transcript); the
`templum delegate` row; Files ("Tasks"); Architecture (batch 8 row); Testing "Batch 8"; Delivery item 8; Risks.

## Spike results

**S8-1 — Claude Code's status line: when it has a turn's figures, and what they are.** Run on 2026-10-16 in tmux
(200 × 50) in batch 7's scratch folder (already trusted), `claude --model haiku --settings <scratch>/settings.json
--allowedTools 'Bash(sleep:*)'`, the settings giving a `statusLine` command and `UserPromptSubmit`, `PostToolUse` and
`Stop` hooks that logged `date +%s.%N` and what they were handed; 3 prompts.

| Prompt | `UserPromptSubmit` | `Stop` | status line after it | its figures |
|---|---|---|---|---|
| "Reply with just the letter A." | 585.093 | 588.085 | 588.392 (+307 ms) | in 37929, out 112, $0.0290 |
| "Run `sleep 3` …, then reply with just B." | 606.612 | 614.458 (`PostToolUse` 611.894) | 614.764 (+306 ms); one at 609.160 mid-turn | 38069 / 139 / $0.0338 mid-turn; 38233 / 29 / $0.0381 |
| "Reply with just the letter C." | 648.776 | 650.567 | 650.875 (+308 ms) | 38279 / 40 / $0.0477 |

- Claude Code runs its status line about 0.3 s **after** its `Stop`: a read at the `Stop` misses the turn's last call
  (here, the whole of a one-call task).
- `context_window.total_input_tokens` and `total_output_tokens` are **its last call's**, not the session's: the third
  status line said `"total_input_tokens":38279,"total_output_tokens":40,"current_usage":{"input_tokens":10,
  "output_tokens":40,"cache_creation_input_tokens":10,"cache_read_input_tokens":38259}` (10 + 10 + 38259 = 38279), and
  the output went 112, 139, 29, 40. `cost.total_cost_usd` grows with the session (0.0290, 0.0338, 0.0381, 0.0477). So
  what Templum reads as Claude Code's `Session::tokens` (`claude.rs::relayed`, the sum of the two) is its context's
  size; a difference of it says nothing of what a task spent. Its cost does.
- Every hook payload and the status line carry `transcript_path`.

**S8-2 — Codex's tokens, from batch 7's S1 logs** (the hooks' log and the rollout file of the same session, no new
run): each turn's last `token_count` event lands 5 to 7 ms before its `Stop` hook (…187.239 / …187.245, …235.435 /
…235.441, …237.237 / …237.242; and before an `Interrupt`, …252.143 / …252.150); its
`info.total_token_usage.total_tokens` grows with the session (13093, 27033, 41134, 55386, 69666, 83959, 98385), each
call adding its whole input: a read at the `Stop` would be right, but Templum's own read comes after a snapshot (every
5 s), so it may be one or two calls short (14k of 41k here).

**S8-3 — the transcript, from batch 7's S1/S2 logs:** Claude Code's `Stop` (and every other hook) has
`"transcript_path":"/home/winx/.claude/projects/<folder>/<session>.jsonl"`, Codex's
`"transcript_path":"/home/winx/.codex/sessions/2026/10/16/rollout-2026-10-16T03-44-39-<session>.jsonl"`. Gemini CLI's
is assumed (no login).

Nothing was written to the agents' configuration; Claude Code kept its own transcript of the 3 prompts.

## Global Constraints

- Only `crates/templum` depends on gpui-kit; `templum-mcp` stays UI-free and is tested headlessly. No new dependency.
  No `unsafe`. Every logic change ships a test that fails without it.
- Copy, verbatim (the spec at this commit):
  - task's first line `[from Claude Code · api (b7k2m), task t7] Your task is in the pasted text: do it.`; a spawn's
    `[from <caller>, task t8] Your task is in the pasted text: do it.`;
  - `delegate` without `wait`: `Handed to k9x1d as task t7.`; `spawn_agent`'s result ends `Its prompt is typed once
    it is ready, as task t8.`;
  - `[reply from <title> (<id>) to task t7] Its reply is in the pasted text.` when the replier is on that agent's task;
  - end: `[task t7 from Codex · api (k9x1d): done in 31 s, 60231 tokens, transcript /…/rollout.jsonl] Its answer is
    in the pasted text.` then the answer; `failed` and `What happened is in the pasted text.` then why;
  - dialog: `[task t7 from Codex · api (k9x1d) waits for its person: Allow Bash?] Its screen is in the pasted text.`
    then the last 20 lines of its screen;
  - change: `[from Claude Code · api (b7k2m), a change to task t7] The change is in the pasted text: do it.`; its
    answer without `wait`: `Typed into k9x1d as a change to task t7.`;
  - instructions: "A task you hand over without wait comes back typed into your terminal as [task …]: it isn't the
    person writing."
- What a task spent "is what its usage chip shows ([usage.md](usage.md)), since the task was handed over: Claude Code's
  cost (`$0.42`…), Codex's tokens; a spawned agent's task counts its whole session; nothing when unknown. Templum reads
  it again a second after the end".
- A notice waits while the asker "waits for its person, its agent is starting (no snapshot found it yet), its person
  typed into it in the last 3 seconds, or another notice went into it in the last 3 seconds …, oldest first. Into a
  terminal whose agent is gone (its shell is in front …), never: the notice is dropped."
- "once per wait; not while a call of the asker waits for the task"; "a task handed with `wait` gets no notice of its
  timeout"; "`interrupt` by the agent that handed the task over ends it without a notice …, and so does its
  `close_block`".
- Commits on `development` (never `main`), small, Conventional Commits; `feat`/`fix` titles are for a person using
  Templum. End each message with the executing session's `Claude-Session:` line when it has one.
- Before saying done: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`, `cargo deny check`; this batch types into agents, so `--bench` and `--bench-mixed` run too: `p95_ms`
  ≤ 16.7 each.

## Decisions

1. **Ids** count up in `Workspace::next_task` from 1 each time Templum starts; nothing about a task is saved (it never
   was). `Handed::new(by, timeout)` stays (tests use it) with `id: 0`; the two places that hand a task over set `id`
   with `Handed { id, ..Handed::new(…) }`. The first line names the task (the spec said only a change and a reply
   would, which named an id the agent working on it had never seen).
2. **Every task's end goes through `end_turn`**, which already answers the waiting calls: it takes the pane's `Handed`
   out, and when no call of the asker waits on that terminal (`Waiting::Turn` from `handed.by`) it calls `tell_end`.
   `done` or `failed` is the end's `Reply::is_error`, why is its text: "didn't take the task" (`not_taken`), "Its turn
   ended without an answer" (`Stopped` without a message), "Its agent ended before it answered." (`agent_left`), "never
   got its prompt" (`look_ready`), "was closed before it answered." (`remove_block`, new), the still screen's "Its
   screen has been still…" is `done`. `interrupt` and `close_block` by the asker clear `handed` first, so their
   `end_turn` tells nothing. `reply` with no call waiting is the end (it clears `handed` itself).
3. **Delivery** (`deliver_notices`) runs when a notice is queued and after each look (`activity::LOOK`), and types at
   most one notice per asker per pass, the oldest; a typed notice sets the asker's `Pane::noticed`, and the next waits
   `CONFIRM` (its Enter, and a second one, may still be due): two notices never share an Enter (the planning pass left
   it as a minor; it costs one field). Dropped: no pane, `seen.agent` None, `shell_in_front()`. Waiting: `seen.pid`
   None, `NeedsYou`, `keyed` within `TYPING`, `noticed` within `CONFIRM`. Typed with `send_task(to, text, false)`: no
   task ends if its Enter isn't confirmed.
4. **What a task spent** (spike S8-1, S8-2): the usage chip's own rule, cost when both ends have one, else tokens
   (`spent`), from `Handed::usage` (the pane's `usage` at hand-over; for a spawn, a zero cost and zero tokens: its
   session starts with its task) to a read made `SPENT_AFTER` (1 s) after the end: with a data folder, the same
   `templum_usage::read` the chip uses, for this one agent (`usage.rs::proc_of`); else, or when it finds nothing, the
   pane's `usage`. So the end's notice is queued a second after the end; its header (task, time, transcript) is made at
   the end. Not done (YAGNI): Claude Code's spending in tokens (its transcript's calls summed: a new reader), Gemini
   CLI's (no session file is read).
5. **The transcript** is the last `transcript_path` a `Stopped` carried (`Pane::transcript`), cleared by `Started` (a
   new session), flattened onto the header's line (`bus::flat`): a path can't break the line or end the paste.
6. **A dialog's notice** comes from `needs_you`, which already reads the screen `DRAWN` after the hook; its new `tell`
   says the wait is new: `agent_event` passes true unless the agent already waited for its person (Claude Code's
   `permission_prompt` and later notifications are the same dialog), `look_at_agents` passes true (seems stuck),
   `wait` passes true (a balloon of the task's agent). No notice while a call of the asker waits for the task: that
   agent is inside the call and would read it only after the end.
7. **Timeouts:** a timer per task handed without `wait` (`time_out`, as each wait has one), firing after the task's
   seconds; it tells (`tell_end` with the failed reply, `handed` kept) only if the task's `id` and `until` are those it
   was set for (a change sets another `until`) and no call of the asker waits for it (that call tells, or waits on for
   the answer). A task handed with `wait` gets no timer: its call's own deadline tells it; if its client cancels the
   call (Claude Code past 2 minutes) the timeout goes untold, its end still comes (a minor).
8. **A change** is `delegate` from `handed.by`, also past its deadline (still its task): refused while its agent waits
   for its person (`takes_handing`, as a task) and while a spawn's prompt still waits (it would come before its own
   task); typed with `send_task(…, true)`; `taken` and `behind` go false, `until` is now plus the call's timeout, and
   a new timer runs without `wait`. Not done: merging a change into a spawn's waiting prompt.
9. **Each notice's first line ends with a sentence**, as batch 7's `reply` line does: `Its answer is in the pasted
   text.`, `What happened is in the pasted text.`, `Its screen is in the pasted text.`, and a change's `The change is
   in the pasted text: do it.` (batch 7's finding: Claude Code takes a paste of 4 lines or more alone for data; the
   typed first line says what it is). With Decision 1's `, task t7` in the task's first line, this is copy the agents
   read that the spec didn't have: **needs the maintainer's eye**.

## Review Focus

1. A title, a question or a transcript path carrying a newline, an ESC or a paste's end: the notice's first line stays
   one line, and the paste isn't ended early (`clean_title`, `flat`; `type_in` keeps a paste end inside). (Task 5, the
   transcript `"/x/roll\nout\u{1b}.jsonl"`; Task 7, the question `"Allow\nEdit?"`.)
2. Claude Code stops waiting on a `delegate` past 2 minutes (it cancels the call): the task's end still comes back,
   typed. (Task 3, `a_waiting_call_gets_the_answer_and_no_notice`, its last part.)
3. Two tasks of one asker end in the same second: the second notice waits until the first's Enter can't be due any
   more, and they come oldest first. (Task 4.)
4. The asker's agent quits while a notice waits for it: the notice is dropped, never typed into its shell. (Task 4, its
   last part.)
5. A change to a spawned agent whose prompt still waits (its trust question): refused, nothing typed ahead of its
   prompt. (Task 9.)

## File Structure

| File | Change |
|---|---|
| `crates/templum-mcp/src/hooks.rs` | `Stopped { message, transcript }`; every `Stopped` takes `transcript_path` |
| `crates/templum-mcp/src/cli.rs` | its hook test carries a transcript |
| `crates/templum-mcp/src/tools.rs` | `delegate`'s and `spawn_agent`'s descriptions |
| `crates/templum/src/workspace/mod.rs` | `Handed::{id, usage}`; `Pane::{noticed, transcript}`; `Workspace::{next_task, notices}`; the looks loop delivers |
| `crates/templum/src/workspace/blocks.rs` | the new `Pane` fields; `remove_block` ends a closed block's task |
| `crates/templum/src/workspace/delegate.rs` | ids in `delegate` and `reply_to`; `Notice`, `tell_end`, `queue_notice`, `deliver_notices`, `Pane::takes_notice`, `signature_of`, `spent`, `tell_waiting`, `time_out`, `timed_out`; the change; consts |
| `crates/templum/src/workspace/waits.rs` | `waits_on`; `needs_you(…, tell, …)` |
| `crates/templum/src/workspace/bus.rs` | `Stopped`'s transcript, `Started` clears it, `NeedsYou`'s `tell`; the instructions' sentence |
| `crates/templum/src/workspace/activity.rs` | stuck tells |
| `crates/templum/src/workspace/spawn.rs` | the spawn's id, zero usage and timer; `close_block` ends its own task quietly |
| `crates/templum/src/workspace/usage.rs` | `proc_of` |
| tests: `templum-mcp` units, `templum/src/workspace/tests/{delegate,bus,spawn}.rs` | per task |
| `docs/specs/connections.md`, `docs/minors.md` | Task 11 |

Before Task 1: `git status` is clean on `development`, at this plan's commit or later.

---

### Task 1: A turn's end carries its transcript

**Files:**
- Modify: `crates/templum-mcp/src/hooks.rs:25` (`Stopped`), `:34-80` (`claude`: `:44-46`, `:49-51`, `:64`, `:74`),
  `:84-104` (`codex`: `:97-101`), `:108-126` (`gemini`: `:116-118`); tests `:133-414`
- Modify: `crates/templum-mcp/src/cli.rs:753-768` (test)
- Modify: `crates/templum/src/workspace/bus.rs:260` (`Stopped { message, .. }`, so the app builds)

**Interfaces:**
- Produces: `AgentEvent::Stopped { message: Option<String>, transcript: Option<String> }`, `transcript` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]` (an older Templum reads the event as before, and the
  JSON without it is unchanged).

- [ ] **Step 1: Write the failing tests.** Every existing `Stopped { message: … }` in `hooks.rs`'s tests gains
  `transcript: None`. Then, in `claude_codes_hooks_become_events`, after the `Stop` assert:

```rust
        let path = "/home/u/.claude/projects/p/727bd1ac.jsonl";
        assert_eq!(
            event(json!({ "hook_event_name": "Stop", "last_assistant_message": "Done.",
                "transcript_path": path })),
            Some(AgentEvent::Stopped { message: Some("Done.".into()), transcript: Some(path.into()) }),
            "the notice of a task's end names it"
        );
        assert_eq!(
            event(json!({ "hook_event_name": "PostToolUseFailure", "is_interrupt": true,
                "transcript_path": path })),
            Some(AgentEvent::Stopped { message: None, transcript: Some(path.into()) })
        );
```

  In `codex_hooks_become_events`, the `Stop` and `Interrupt` payloads gain `"transcript_path": rollout` (`let rollout =
  "/home/u/.codex/sessions/2026/10/16/rollout-2026-10-16T03-44-39-01a14374.jsonl";`) and their events
  `transcript: Some(rollout.into())`. In `gemini_clis_hooks_become_events_as_claude_codes`, the first `AfterAgent`
  gains `"transcript_path": "/tmp/g.json"` and `transcript: Some("/tmp/g.json".into())`. In
  `events_go_over_the_bus_as_tagged_json`:

```rust
        assert_eq!(
            serde_json::to_value(AgentEvent::Stopped { message: Some("x".into()), transcript: None }).unwrap(),
            json!({ "event": "stopped", "message": "x" }),
            "no transcript, no key: the event an older Templum reads"
        );
        assert_eq!(
            serde_json::from_value::<AgentEvent>(json!({ "event": "stopped", "message": "x", "transcript": "/t" }))
                .unwrap(),
            AgentEvent::Stopped { message: Some("x".into()), transcript: Some("/t".into()) }
        );
```

  (replacing its first assert). In `cli.rs`'s `a_hook_sends_its_event_prints_nothing_and_never_fails`, `stop` becomes
  `r#"{"hook_event_name":"Stop","last_assistant_message":"Done.","transcript_path":"/t.jsonl"}"#` and its event's
  params `json!({ "event": "stopped", "message": "Done.", "transcript": "/t.jsonl" })` (its last use, outside Templum,
  is unchanged).
- [ ] **Step 2:** `cargo test -p templum-mcp`. Expected: FAIL to compile (`transcript`).
- [ ] **Step 3: Implement.** The field and its doc ("The transcript its hook named (`transcript_path`), for the notice
  of a task's end"). Each `Stopped` of `claude`, `codex` and `gemini` takes `transcript: text("transcript_path")`
  (`codex` gets the same `text` closure as the others). `bus.rs:260`: `AgentEvent::Stopped { message, .. }` (Task 5
  keeps it).
- [ ] **Step 4:** `cargo test -p templum-mcp && cargo build -p templum`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum-mcp/src/hooks.rs crates/templum-mcp/src/cli.rs crates/templum/src/workspace/bus.rs
git commit -m "feat(connections): Templum hears which transcript an agent's turn was written to"
```

---

### Task 2: Every task has an id

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs:132-158` (`Handed::id`), `:1090-1093` and `:1397-1399`
  (`Workspace::next_task`)
- Modify: `crates/templum/src/workspace/delegate.rs:540-561` (`delegate`), `:595-604` (`reply_to`)
- Modify: `crates/templum/src/workspace/spawn.rs:127-134`, `:174-194` (`open_agent`)
- Test: `crates/templum/src/workspace/tests/delegate.rs:96-126`, `:132-148`, `:235-257`; `tests/spawn.rs:279-335`

**Interfaces:**
- Produces: `Handed::id: u64` ("Its id, said `t7`; counts from 1 each time Templum starts (`Workspace::next_task`); 0
  for one made with `new`"); `Workspace::next_task: u64` ("The last task's id").

- [ ] **Step 1: Write the failing tests.** `delegate_types_the_header_the_task_and_enter`: its result assert becomes
  `assert_eq!((failed, text), (false, format!("Handed to {them} as task t1.")));`, the read of `handed` also checks
  `assert_eq!(handed.id, 1);`, and its needle `&format!("({uid}), task t1] Your task is in the pasted text: do it. fix
  the tests then push[from")`. `a_task_goes_as_its_first_line_then_its_text`: `&format!("({uid}), task t1] Your task
  is in the pasted text: do it.^[[201~^[[200~")`. `reply_with_no_one_waiting_types_into_the_agent` gains, at its end:

```rust
    // The reply to a task the agent it goes to handed: its line names the task.
    let (terminal, _) = me(&ws, cx, mine);
    let task = json!({ "block": them, "task": "find the bug" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(!failed, "{text}");
    let reply = json!({ "block": uid, "text": "Line 42." });
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply);
    assert_eq!((failed, text), (false, format!("Typed into {uid}.")));
    screen_with(&ws, cx, mine, "Line 42.");
    enter(cx);
    screen_with(
        &ws,
        cx,
        mine,
        &format!("({them}) to task t1] Its reply is in the pasted text. Line 42.[reply from"),
    );
```

  In `tests/spawn.rs`'s `a_prompt_waits_for_the_agents_input_to_take_pastes`, the spawn's call keeps its answer and
  the prompt's needle names the task (claude is `t1`, codex `t2`, one workspace):

```rust
        let (failed, said) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
        assert!(
            !failed && said.ends_with(&format!("Its prompt is typed once it is ready, as task t{conn}.")),
            "{said}"
        );
```

  and `screen_with(&ws, cx, new, &format!(", task t{conn}] Your task is in the pasted text: do it."));`.
- [ ] **Step 2:** `cargo test -p templum -- delegate_types_the_header a_task_goes_as reply_with_no_one a_prompt_waits`.
  Expected: FAIL (no id).
- [ ] **Step 3: Implement.** `delegate`: after the refusals, `self.next_task += 1; let id = self.next_task;`, the
  first line `format!("[from {}, task t{id}] {TASK_LEAD}\n{task}", self.signature(space, me, cx))`, `pane.handed =
  Some(Handed { id, ..Handed::new(from, Duration::from_secs(timeout)) })`, and without `wait`
  `Reply::ok(format!("Handed to {block} as task t{id}."))`. `open_agent`: keep the caller's signature (`by`) where the
  prompt is made now, and once the block is open take the next id and make the prompt `format!("[from {by}, task
  t{id}] {TASK_LEAD}\n{prompt}")`, `Handed { id, ..Handed::new(from, …TASK_WAIT) }`, and `said.push_str(&format!(" Its
  prompt is typed once it is ready, as task t{id}."))`. `reply_to`, with no call waiting: the replier's task from that
  agent, `self.pane(from).and_then(|p| p.handed.as_ref()).filter(|h| h.by == asker).map(|h| h.id)`, read before
  `handed` is cleared, gives `" to task t{id}"` after the signature in `[reply from {sig}{to}] Its reply is in the pasted
  text.\n{text}`.
- [ ] **Step 4:** `cargo test -p templum workspace::tests`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/mod.rs crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/tests/delegate.rs crates/templum/src/workspace/tests/spawn.rs
git commit -m "feat(connections): each task an agent hands over has an id, said in its first line and in the replies to it"
```

---

### Task 3: A task's end comes back typed into the agent that handed it

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs:15-43` (consts), `:118-206` (`Pane::takes_notice`), `:479-484`
  (`signature_of` beside `uid_of`), `:670-683` (`end_turn`; `tell_end`, `queue_notice`, `deliver_notices` after it)
- Modify: `crates/templum/src/workspace/waits.rs:198-216` (`waits_on`, beside `closed`)
- Modify: `crates/templum/src/workspace/mod.rs:1090-1093`, `:1397-1399` (`notices`), `:1197-1203` (the looks loop)
- Test: `crates/templum/src/workspace/tests/delegate.rs` (helpers after `enter`; tests after
  `a_turns_end_and_its_timeout_answer_once`)

**Interfaces:**
- Consumes: `Handed::id` (Task 2).
- Produces: `pub(crate) struct Notice { pub to: String, pub text: String }` ("What a task's agent did, to type into
  the agent that handed it (its terminal `to`)"); `Workspace::notices: Vec<delegate::Notice>` (oldest first);
  `pub(super) const DONE_LEAD: &str = "Its answer is in the pasted text."`, `pub(super) const FAILED_LEAD: &str = "What
  happened is in the pasted text."`; `Workspace::signature_of(&self, terminal: &str, cx: &App) -> String` (the
  `signature` of the block of `terminal`, empty when it is gone); `Workspace::waits_on(&self, from: &str, of: &str) ->
  bool` (a call from the terminal `from` waits for the turn of the agent in `of`: `Waiting::Turn`);
  `Workspace::tell_end(&mut self, terminal: &str, handed: &Handed, reply: &Reply, cx: &mut Context<Self>)`;
  `Workspace::queue_notice(&mut self, to: String, text: String, cx: &mut Context<Self>)` (pushes it, then delivers);
  `pub(super) fn deliver_notices(&mut self, cx: &mut Context<Self>)`; `Pane::takes_notice(&self) -> Option<bool>`
  ("None: never, no agent runs there (a shell would run the text); Some(false): not now").

- [ ] **Step 1: Write the failing tests.** Helpers in `tests/delegate.rs`, after `enter`:

```rust
/// Mine runs Claude Code too, found by a snapshot: what comes back is typed into it.
#[cfg(unix)]
fn asker(ws: &Entity<Workspace>, cx: &mut TestAppContext, mine: BlockId) {
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(mine).unwrap().seen.agent = Some("claude".into())
    });
    found(ws, cx, mine);
}

/// The notices waiting to be typed, oldest first: to whom, and what.
#[cfg(unix)]
fn notices(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<(String, String)> {
    ws.read_with(cx, |ws, _| {
        ws.notices.iter().map(|n| (n.to.clone(), n.text.clone())).collect()
    })
}

/// The terminal `from` hands block `theirs` the task `said` without waiting; its agent takes it
/// and ends with the answer `said`; then a second passes (what it spent is read then).
#[cfg(unix)]
fn done_task(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    from: &str,
    theirs: BlockId,
    said: &str,
) {
    let (worker, them) = me(ws, cx, theirs);
    let (failed, text) = call(handle, ws, cx, from, "delegate", json!({ "block": them, "task": said }));
    assert!(!failed, "{text}");
    screen_with(ws, cx, theirs, said);
    enter(cx);
    event(handle, ws, cx, &worker, json!({ "event": "submitted" }));
    event(handle, ws, cx, &worker, json!({ "event": "stopped", "message": said }));
    after(cx, 1000);
}
```

  Then the tests:

```rust
/// A task handed over without `wait` ends: the agent that handed it hears so in its terminal,
/// under a first line naming the task, its agent and its time.
#[cfg(unix)]
#[gpui_kit::test]
fn a_task_handed_without_wait_comes_back_typed(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    asker(&ws, cx, mine);
    done_task(handle, &ws, cx, &terminal, theirs, "All green.");
    // `cat` echoes what is typed: the notice, its first line and its answer as one line.
    let shown = screen_with(&ws, cx, mine, "All green.");
    assert!(
        shown.contains("[task t1 from ")
            && shown.contains(&format!("({them}): done in "))
            && shown.contains(" s] Its answer is in the pasted text. All green."),
        "{shown}"
    );
    assert!(notices(&ws, cx).is_empty());
}

/// A call of the agent that handed the task over waits for it: that call gets the answer, and
/// nothing is typed. One its client gave up on (Claude Code past 2 minutes) waits no more.
#[cfg(unix)]
#[gpui_kit::test]
fn a_waiting_call_gets_the_answer_and_no_notice(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    asker(&ws, cx, mine);
    let ends = |cx: &mut TestAppContext, said: &str| {
        screen_with(&ws, cx, theirs, said);
        enter(cx);
        event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
        event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": said }));
    };
    // `delegate` with `wait`.
    let task = json!({ "block": them, "task": "one", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    ends(cx, "one");
    assert_eq!(answered(&answers), (false, "one".to_string()));
    after(cx, 1000);
    // `wait_for_idle` after one without.
    call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "two" }));
    let idle = call_later(handle, &ws, cx, &terminal, 2, "wait_for_idle", json!({ "block": them }));
    ends(cx, "two");
    assert_eq!(answered(&idle), (false, "two".to_string()));
    after(cx, 1000);
    assert!(notices(&ws, cx).is_empty());
    // Cancelled: its end comes back typed.
    let task = json!({ "block": them, "task": "three", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 3, "delegate", task);
    super::bus::bus_note(handle, &ws, cx, 3, "notifications/cancelled", json!({ "requestId": 3 }));
    ends(cx, "three");
    assert!(answers.try_recv().is_err(), "no one waits");
    after(cx, 1000);
    let shown = screen_with(&ws, cx, mine, "[task t3 from ");
    assert!(!shown.contains("[task t1") && !shown.contains("[task t2"), "{shown}");
}

/// The agent that handed a task over isn't there to read its end: a script's `templum delegate`
/// in a shell, or an agent that quit. Nothing is typed into that shell: the notice is dropped.
#[cfg(unix)]
#[gpui_kit::test]
fn a_notice_never_goes_into_a_shell(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    // Mine runs no agent.
    done_task(handle, &ws, cx, &terminal, theirs, "One.");
    assert!(notices(&ws, cx).is_empty(), "dropped");
    // Its agent quit: its shell is in front, though the last snapshot still has it.
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(mine).unwrap();
        pane.seen.agent = Some("claude".into());
        pane.seen.pid = Some(1);
    });
    done_task(handle, &ws, cx, &terminal, theirs, "Two.");
    assert!(notices(&ws, cx).is_empty(), "dropped");
    // An agent again: typed, and that one only.
    found(&ws, cx, mine);
    done_task(handle, &ws, cx, &terminal, theirs, "Three.");
    let shown = screen_with(&ws, cx, mine, "[task t3 from ");
    assert!(!shown.contains("[task t1") && !shown.contains("[task t2"), "{shown}");
}
```

  (`bus_note` is `pub(super)` in `tests/bus.rs`.)
- [ ] **Step 2:** `cargo test -p templum -- comes_back_typed a_waiting_call_gets a_notice_never`. Expected: FAIL to compile
  (`notices`), then nothing typed into mine.
- [ ] **Step 3: Implement.**
  - `end_turn`: `let handed = self.pane_mut(terminal).and_then(|p| p.handed.take());`, the waiting keys as now, and
    before answering them, when `handed` is Some and `!self.waits_on(&handed.by, terminal)`, `self.tell_end(terminal,
    &handed, &reply, cx)`. Its doc: "…; the agent that handed the task over hears so, typed, when no call of its waits
    for it (docs/specs/connections.md, Delegating 10)."
  - `tell_end`: `queue_notice(handed.by, format!("[task t{} from {}: {word} in {} s] {lead}\n{}", handed.id,
    self.signature_of(terminal, cx), handed.since.elapsed().as_secs(), reply.text))`, `word`/`lead` `"failed"` and
    `FAILED_LEAD` when `reply.is_error`, else `"done"` and `DONE_LEAD`.
  - `takes_notice` (this task): `None` when `self.seen.agent.is_none() || self.shell_in_front()`, else `Some(true)`.
  - `deliver_notices`, the queue in order, one per asker per pass:

```rust
        let mut i = 0;
        while i < self.notices.len() {
            match self.pane(&self.notices[i].to).and_then(Pane::takes_notice) {
                None => {
                    self.notices.remove(i);
                }
                Some(false) => i += 1,
                Some(true) => {
                    let notice = self.notices.remove(i);
                    self.send_task(&notice.to, &notice.text, false, cx);
                }
            }
        }
```

    (Task 4 makes the asker wait after one: until then two notices of one asker may go in one pass.)
  - The looks loop (`mod.rs:1199-1202`): `ws.update(cx, |ws, cx| { ws.look_at_agents(cx); ws.deliver_notices(cx) })`.
- [ ] **Step 4:** `cargo test -p templum workspace::tests`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/waits.rs crates/templum/src/workspace/mod.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): a task handed over without waiting comes back typed into the agent that handed it"
```

---

### Task 4: A notice waits for its asker

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs:172-224` (`Pane::noticed`), `blocks.rs:418-441` (`noticed: None`)
- Modify: `crates/templum/src/workspace/delegate.rs` (`takes_notice`, `deliver_notices`)
- Test: `crates/templum/src/workspace/tests/delegate.rs` (after Task 3's)

**Interfaces:**
- Consumes: `Notice`, `takes_notice`, `deliver_notices`, `done_task`, `asker`, `notices` (Task 3).
- Produces: `Pane::noticed: Option<Instant>` ("When Templum last typed a notice into it: the next waits CONFIRM, its
  Enter, and a second one, may still be due").

- [ ] **Step 1: Write the failing test.**

```rust
/// What comes back waits while its asker can't take it: it asks its person, no snapshot found it
/// yet, or its person is typing into it. Oldest first, and the next only once the last one's Enter
/// can't be due any more. An agent that quit meanwhile gets nothing: its shell would run it.
#[cfg(unix)]
#[gpui_kit::test]
fn a_notice_waits_while_its_asker_asks_its_person(cx: &mut TestAppContext) {
    use crate::workspace::delegate::{CONFIRM, TYPING};
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    asker(&ws, cx, mine);
    let queued = |cx: &mut TestAppContext| -> Vec<String> {
        notices(&ws, cx)
            .iter()
            .map(|(_, text)| text.lines().last().unwrap().to_string())
            .collect()
    };
    let asks = json!({ "event": "needs_you", "message": "Allow Edit?" });
    event(handle, &ws, cx, &terminal, asks.clone());
    done_task(handle, &ws, cx, &terminal, theirs, "One.");
    done_task(handle, &ws, cx, &terminal, theirs, "Two.");
    assert_eq!(queued(cx), ["One.", "Two."]);
    // Answered, and restarted: no snapshot found its agent yet.
    event(handle, &ws, cx, &terminal, json!({ "event": "stopped", "message": null }));
    ws.update(cx, |ws, _| ws.space.terminal_mut(mine).unwrap().seen.pid = None);
    after(cx, 1000);
    assert_eq!(queued(cx).len(), 2, "starting");
    // Found; its person types into it.
    found(&ws, cx, mine);
    ws.read_with(cx, |ws, _| ws.space.terminal(mine).unwrap().person_typed());
    after(cx, 1000);
    assert_eq!(queued(cx).len(), 2, "the person types");
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(mine).unwrap();
        pane.keyed.set(Instant::now().checked_sub(TYPING));
    });
    after(cx, 1000);
    screen_with(&ws, cx, mine, "[task t1 from ");
    assert_eq!(queued(cx), ["Two."], "one, the oldest");
    after(cx, 1000);
    assert_eq!(queued(cx), ["Two."], "its Enter may still be due");
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(mine).unwrap().noticed = Instant::now().checked_sub(CONFIRM)
    });
    after(cx, 1000);
    screen_with(&ws, cx, mine, "[task t2 from ");
    assert!(queued(cx).is_empty());
    // It asks again, and quits while a notice waits: dropped.
    event(handle, &ws, cx, &terminal, asks);
    done_task(handle, &ws, cx, &terminal, theirs, "Three.");
    assert_eq!(queued(cx), ["Three."]);
    ws.update(cx, |ws, _| ws.space.terminal_mut(mine).unwrap().seen.pid = Some(1));
    after(cx, 1000);
    assert!(queued(cx).is_empty());
}
```

- [ ] **Step 2:** `cargo test -p templum a_notice_waits_while`. Expected: FAIL to compile (`noticed`), then typed while
  it asks its person.
- [ ] **Step 3: Implement.** `takes_notice`, in order: `seen.agent` None → None; `seen.pid` None → `Some(false)` (it is
  starting; macOS can't tell its shell is in front before a snapshot found it); `shell_in_front()` → None;
  `NeedsYou(_)` → `Some(false)`; `keyed` within `TYPING` → `Some(false)`; `noticed` within `CONFIRM` → `Some(false)`;
  else `Some(true)`. Its doc follows the spec's sentence. `deliver_notices` sets the asker's `noticed =
  Some(Instant::now())` after its `send_task`, which makes the asker's next notice in the same pass wait.
- [ ] **Step 4:** `cargo test -p templum workspace::tests::delegate`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/mod.rs crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "fix(connections): what comes back waits while its agent asks its person or is typed into, one at a time"
```

---

### Task 5: The end says what the task spent and its transcript

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs:132-158` (`Handed::usage`), `:172-224` (`Pane::transcript`),
  `blocks.rs:418-441` (`transcript: None`)
- Modify: `crates/templum/src/workspace/bus.rs:259-278` (`Started`, `Stopped`)
- Modify: `crates/templum/src/workspace/delegate.rs` (`SPENT_AFTER`, `spent`, `tell_end`; `delegate`'s `Handed`;
  unit test in its `tests` module)
- Modify: `crates/templum/src/workspace/spawn.rs:179-188` (a spawn's `Handed::usage`)
- Modify: `crates/templum/src/workspace/usage.rs:80-102` (`proc_of`)
- Test: `crates/templum/src/workspace/tests/delegate.rs`; `tests/spawn.rs:279-290`

**Interfaces:**
- Consumes: `tell_end`, `queue_notice` (Task 3); `AgentEvent::Stopped`'s `transcript` (Task 1).
- Produces: `Handed::usage: Option<templum_usage::Session>` ("Its agent's usage when it was handed over: what its end
  says it spent counts from it"); `Pane::transcript: Option<String>` ("The transcript its agent's last turn's end named,
  until its next session starts"); `pub(super) const SPENT_AFTER: Duration = Duration::from_secs(1)` ("How long after a
  task's end what it spent is read: Claude Code's status line gives its last cost about 0.3 s after its `Stop`");
  `fn spent(from: Option<&Session>, to: Option<&Session>) -> Option<String>` in `delegate.rs` (`"$0.42"` or `"60231
  tokens"`); `pub(super) fn proc_of(pane: &Pane) -> Option<templum_usage::Proc>` in `usage.rs` (its agent as the last
  snapshot found it; `read_usage` uses it too).

- [ ] **Step 1: Write the failing tests.** In `delegate.rs`'s `tests` module:

```rust
    #[test]
    fn what_a_task_spent_is_its_cost_else_its_tokens() {
        use templum_usage::Session;
        let s = |cost_usd, tokens| Session { cost_usd, tokens, ..Default::default() };
        // Claude Code's status line: its cost; its token totals are its last call's context.
        let (claude, later) = (s(Some(0.10), Some(38069)), s(Some(0.52), Some(38279)));
        assert_eq!(spent(Some(&claude), Some(&later)).as_deref(), Some("$0.42"));
        // Codex: its tokens.
        let (codex, later) = (s(None, Some(1000)), s(None, Some(61231)));
        assert_eq!(spent(Some(&codex), Some(&later)).as_deref(), Some("60231 tokens"));
        // Unknown at either end: nothing; never less than nothing.
        assert_eq!(spent(None, Some(&later)), None);
        assert_eq!(spent(Some(&s(None, None)), Some(&s(None, None))), None);
        assert_eq!(spent(Some(&later), Some(&codex)).as_deref(), Some("0 tokens"));
    }
```

  In `tests/delegate.rs`:

```rust
/// What a task spent is what its agent's usage chip shows, since it was handed over: its tokens
/// (Codex), its cost when it gives one (Claude Code). Read again a second after its end; with a
/// data folder, from the agent's own files. Its transcript is the one its last turn's end named,
/// on one line. The test's clock stays under 5 s: a snapshot, with a data folder, would look for
/// real agents.
#[cfg(unix)]
#[gpui_kit::test]
fn the_end_says_what_it_spent_and_its_transcript(cx: &mut TestAppContext) {
    use templum_usage::Session;
    let spent_after = crate::workspace::delegate::SPENT_AFTER.as_millis() as u64;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    asker(&ws, cx, mine);
    // Mine asks its person: the notices wait, to be read here.
    event(handle, &ws, cx, &terminal, json!({ "event": "needs_you", "message": "Allow Edit?" }));
    let usage = |cx: &mut TestAppContext, u: Session| {
        ws.update(cx, |ws, _| ws.space.terminal_mut(theirs).unwrap().usage = Some(u))
    };
    let send = |cx: &mut TestAppContext, e: serde_json::Value| event(handle, &ws, cx, &worker, e);
    usage(cx, Session { tokens: Some(1000), ..Default::default() });
    call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "count" }));
    screen_with(&ws, cx, theirs, "count");
    enter(cx);
    usage(cx, Session { tokens: Some(61231), ..Default::default() });
    send(cx, json!({ "event": "submitted" }));
    send(cx, json!({ "event": "stopped", "message": "Done.", "transcript": "/x/roll\nout\u{1b}.jsonl" }));
    assert!(notices(&ws, cx).is_empty(), "read SPENT_AFTER later");
    after(cx, spent_after);
    let told = notices(&ws, cx);
    assert!(
        told[0].1.starts_with("[task t1 from ")
            && told[0].1.ends_with(
                " s, 60231 tokens, transcript /x/roll out.jsonl] Its answer is in the pasted text.\nDone."
            ),
        "{}",
        told[0].1
    );
    // Claude Code with its status line: its cost, which its file has a moment after its end.
    let data = tempfile::tempdir().unwrap();
    ws.update(cx, |ws, _| {
        ws.data_dir = Some(data.path().to_path_buf());
        let pane = ws.space.terminal_mut(theirs).unwrap();
        pane.seen.agent = Some("claude".into());
        pane.seen.started = Some(crate::notes::now());
    });
    usage(cx, Session { cost_usd: Some(0.10), tokens: Some(38069), ..Default::default() });
    call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "again" }));
    screen_with(&ws, cx, theirs, "again");
    enter(cx);
    send(cx, json!({ "event": "submitted" }));
    send(cx, json!({ "event": "stopped", "message": "Again." }));
    let relay = templum_usage::relay_dir(data.path());
    std::fs::create_dir_all(&relay).unwrap();
    let status = r#"{"cost":{"total_cost_usd":0.52},"context_window":{"total_input_tokens":38279,"total_output_tokens":40}}"#;
    std::fs::write(relay.join(format!("{worker}.json")), status).unwrap();
    after(cx, spent_after);
    let told = notices_until(&ws, cx, 2);
    assert!(
        told[1].1.ends_with(" s, $0.42, transcript /x/roll out.jsonl] Its answer is in the pasted text.\nAgain."),
        "{}",
        told[1].1
    );
    ws.update(cx, |ws, _| ws.data_dir = None);
    // Its next session names its own.
    send(cx, json!({ "event": "started" }));
    ws.read_with(cx, |ws, _| assert!(ws.space.terminal(theirs).unwrap().transcript.is_none()));
}

/// The notices once there are `n`: a read on the background executor may come a moment later.
#[cfg(unix)]
fn notices_until(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    n: usize,
) -> Vec<(String, String)> {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let told = notices(ws, cx);
        if told.len() >= n {
            return told;
        }
        assert!(Instant::now() < deadline, "{told:?}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
```

  In `tests/spawn.rs`'s `a_prompt_waits_for_the_agents_input_to_take_pastes`, right after `let new = spawned(…)`:

```rust
        ws.read_with(cx, |ws, _| {
            let handed = ws.space.terminal(new).unwrap().handed.as_ref().unwrap();
            let usage = handed.usage.as_ref().unwrap();
            assert_eq!((usage.cost_usd, usage.tokens), (Some(0.0), Some(0)), "its session is its task's");
        });
```

- [ ] **Step 2:** `cargo test -p templum -- what_a_task_spent the_end_says a_prompt_waits`. Expected: FAIL to compile
  (`spent`, `usage`, `transcript`).
- [ ] **Step 3: Implement.**
  - `spent`: cost when both have one (`format!("${:.2}", (to - from).max(0.0))`), else tokens when both have them
    (`format!("{} tokens", to.saturating_sub(from))`), else None; its doc says why cost first (the spike).
  - `proc_of`: what `read_usage` builds from `seen` (`agent`, `pid`, `started`, `folder`) and the terminal's id;
    `read_usage` calls it.
  - `delegate`: `Handed { id, usage: pane.usage.clone(), .. }`; `open_agent`: `usage: Some(Session { cost_usd:
    Some(0.0), tokens: Some(0), ..Default::default() })`, commented ("its session starts with its task").
  - `bus.rs`: `Started` sets `pane.transcript = None`; `Stopped { message, transcript }` keeps a `Some` transcript in
    `pane.transcript` before anything else.
  - `tell_end` makes the header now and queues the notice `SPENT_AFTER` later:

```rust
        let pane = self.pane(terminal);
        let (word, lead) = if reply.is_error { ("failed", FAILED_LEAD) } else { ("done", DONE_LEAD) };
        let since = handed.since.elapsed().as_secs();
        let head = format!("[task t{} from {}: {word} in {since} s", handed.id, self.signature_of(terminal, cx));
        let transcript = pane
            .and_then(|p| p.transcript.as_deref())
            .map(|t| format!(", transcript {}", flat(t)))
            .unwrap_or_default();
        let proc = pane.and_then(super::usage::proc_of);
        let (data, from, to, body) =
            (self.data_dir.clone(), handed.usage.clone(), handed.by.clone(), reply.text.clone());
        let terminal = terminal.to_string();
        cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(SPENT_AFTER).await;
            let read = match (data, proc) {
                (Some(data), Some(proc)) => {
                    cx.background_executor()
                        .spawn(async move {
                            let dirs = templum_usage::Dirs::of(&data);
                            templum_usage::read(&[proc], &dirs, crate::notes::now()).0.pop().flatten()
                        })
                        .await
                }
                _ => None,
            };
            let _ = ws.update(cx, |ws, cx| {
                // Its block may be gone (closed): what it spent then is unknown.
                let now = read.or_else(|| ws.pane(&terminal).and_then(|p| p.usage.clone()));
                let spent = spent(from.as_ref(), now.as_ref()).map(|s| format!(", {s}")).unwrap_or_default();
                ws.queue_notice(to, format!("{head}{spent}{transcript}] {lead}\n{body}"), cx);
            });
        })
        .detach();
```

    (`flat` is `bus::flat`; `tell_end` now takes `&self`.)
- [ ] **Step 4:** `cargo test -p templum workspace::tests && cargo test -p templum workspace::delegate`. Expected:
  PASS (Tasks 3 and 4's tests wait a second after each end).
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/mod.rs crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/usage.rs crates/templum/src/workspace/tests/delegate.rs crates/templum/src/workspace/tests/spawn.rs
git commit -m "feat(connections): a task that comes back says what it cost or its tokens, and its transcript"
```

---

### Task 6: A task whose agent left or whose block closed comes back as failed

**Files:**
- Modify: `crates/templum/src/workspace/blocks.rs:582-601` (`remove_block`)
- Modify: `crates/templum/src/workspace/delegate.rs:298-306` (`interrupt`)
- Modify: `crates/templum/src/workspace/spawn.rs:215-227` (`close_block`)
- Test: `crates/templum/src/workspace/tests/delegate.rs` (helper after `two_agents`; test after Task 5's)

**Interfaces:**
- Consumes: `end_turn`'s notice (Tasks 3, 5).
- Produces (tests): `fn another_agent(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext, to:
  BlockId) -> BlockId`.

- [ ] **Step 1: Write the failing test.** The helper, after `agents_running`:

```rust
/// A third terminal at (40, 500), 700 × 300, running Codex as far as Templum knows, found in front
/// of its shell (Unix) and wired to `to`.
fn another_agent(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    to: BlockId,
) -> BlockId {
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(Rect::new(40.0, 500.0, 700.0, 300.0), window, cx);
                let id = ws.space.canvas.top().unwrap();
                ws.space.terminal_mut(id).unwrap().seen.agent = Some("codex".into());
                ws.space.appearing.clear();
                assert!(ws.connect(id, to, cx));
                id
            })
        })
        .unwrap();
    frame(handle, cx);
    #[cfg(unix)]
    in_front(ws, cx, id, true);
    found(ws, cx, id);
    id
}
```

  The test:

```rust
/// A task whose agent left, or whose block the person closed, comes back as failed, saying why.
/// One its asker ends itself (`interrupt`, `close_block`) comes back as nothing: it knows.
#[cfg(unix)]
#[gpui_kit::test]
fn a_task_whose_agent_left_or_block_closed_comes_back_as_failed(cx: &mut TestAppContext) {
    use templum_terminal::Inspection;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let extra = another_agent(handle, &ws, cx, mine);
    let (terminal, uid) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let (_, other) = me(&ws, cx, extra);
    asker(&ws, cx, mine);
    event(handle, &ws, cx, &terminal, json!({ "event": "needs_you", "message": "Allow Edit?" }));
    let hand = |cx: &mut TestAppContext, block: &str| {
        let task = json!({ "block": block, "task": "go" });
        let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
        assert!(!failed, "{text}");
    };
    // Its agent left: a snapshot no longer finds it.
    hand(cx, &them);
    snapshot(handle, &ws, cx, theirs, Inspection::default());
    ws.update(cx, |ws, _| ws.space.terminal_mut(theirs).unwrap().seen.agent = Some("codex".into()));
    found(&ws, cx, theirs);
    // Interrupted by its asker.
    hand(cx, &them);
    let _stopped = call_later(handle, &ws, cx, &terminal, 1, "interrupt", json!({ "block": them }));
    // Closed by the person.
    hand(cx, &them);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(theirs, window, cx))
    })
    .unwrap();
    // Closed by its asker, which opened it.
    ws.update(cx, |ws, _| ws.space.terminal_mut(extra).unwrap().opened_by = Some(uid.clone()));
    hand(cx, &other);
    assert!(!call(handle, &ws, cx, &terminal, "close_block", json!({ "block": other })).0);
    after(cx, 1000);
    let told: Vec<String> = notices(&ws, cx).into_iter().map(|(_, t)| t).collect();
    assert_eq!(told.len(), 2, "t1 and t3 only: {told:?}");
    // Both ended in the same instant of the test's clock: found by their ids.
    let task = |id: &str| told.iter().find(|t| t.starts_with(&format!("[task {id} from "))).unwrap();
    assert!(
        task("t1").contains(": failed in ")
            && task("t1").ends_with("] What happened is in the pasted text.\nIts agent ended before it answered."),
        "{told:?}"
    );
    assert!(task("t3").ends_with(&format!("\n{them} was closed before it answered.")), "{told:?}");
}
```

- [ ] **Step 2:** `cargo test -p templum a_task_whose_agent_left`. Expected: FAIL (2 notices expected, the closed one
  missing and the interrupted and closed-by-its-asker ones there).
- [ ] **Step 3: Implement.** `remove_block`, before `take_block`: for a terminal, `self.end_turn(&terminal,
  Reply::err(format!("{uid} was closed before it answered.")), cx)` with its block id from `space.uids` (the text
  `drop_orphans` already gives its waits). `interrupt`'s `mine` arm clears the pane's `handed` and `prompt` before its
  `end_turn`. `close_block`, once allowed, clears the pane's `handed` when its `by` is the caller, before `close_now`.
  Their docs say they tell the asker nothing (Delegating 12).
- [ ] **Step 4:** `cargo test -p templum workspace::tests`. Expected: PASS
  (`closing_the_agent_worked_on_answers_its_wait` still reads "was closed before it answered").
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): a task whose agent quit or whose block was closed comes back as failed, saying why"
```

---

### Task 7: A dialog in a task is told once

**Files:**
- Modify: `crates/templum/src/workspace/waits.rs:443-472` (`needs_you`), `:141` (its call in `wait`)
- Modify: `crates/templum/src/workspace/bus.rs:279-286` (`NeedsYou`), `activity.rs:152-154`
- Modify: `crates/templum/src/workspace/delegate.rs` (`SCREEN_LEAD`, `tell_waiting`)
- Test: `crates/templum/src/workspace/tests/delegate.rs` (after Task 6's), `:1388-1426`

**Interfaces:**
- Consumes: `queue_notice`, `signature_of`, `waits_on` (Task 3).
- Produces: `needs_you(&mut self, terminal: &str, what: Option<&str>, tell: bool, cx: &mut Context<Self>)` (`tell`: a
  new wait, the task's asker hears of it); `pub(super) const SCREEN_LEAD: &str = "Its screen is in the pasted text."`;
  `Workspace::tell_waiting(&mut self, terminal: &str, what: Option<&str>, screen: &str, cx: &mut Context<Self>)`.

- [ ] **Step 1: Write the failing tests.**

```rust
/// A task's agent starts waiting for its person: the agent that handed it hears so, with the
/// dialog as drawn, once per wait, what it asks on one line. Not while a call of its waits for
/// the task's end: it is inside that call.
#[cfg(unix)]
#[gpui_kit::test]
fn a_dialog_in_a_task_is_told_once(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    asker(&ws, cx, mine);
    event(handle, &ws, cx, &terminal, json!({ "event": "needs_you", "message": "Allow Edit?" }));
    let send = |cx: &mut TestAppContext, e: serde_json::Value| event(handle, &ws, cx, &worker, e);
    let asks = |m: serde_json::Value| json!({ "event": "needs_you", "message": m });
    let told = |cx: &mut TestAppContext| -> Vec<String> {
        notices(&ws, cx).into_iter().map(|(_, t)| t).collect()
    };
    call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "go" }));
    screen_with(&ws, cx, theirs, "go");
    enter(cx);
    send(cx, json!({ "event": "submitted" }));
    send(cx, asks(json!("Allow Bash?")));
    shows(&ws, cx, theirs, "1. Yes, proceed (y)");
    // The same wait: its notification, and another message for it.
    send(cx, asks(serde_json::Value::Null));
    send(cx, asks(json!("Claude needs your permission")));
    after(cx, 300);
    let t = told(cx);
    assert_eq!(t.len(), 1, "{t:?}");
    assert!(
        t[0].starts_with("[task t1 from ")
            && t[0].contains(&format!(
                "({them}) waits for its person: Allow Bash?] Its screen is in the pasted text.\n"
            ))
            && t[0].contains("1. Yes, proceed (y)"),
        "{}",
        t[0]
    );
    // Allowed, it works, and asks again: a new wait.
    send(cx, json!({ "event": "working" }));
    send(cx, asks(json!("Allow\nEdit?")));
    after(cx, 300);
    let t = told(cx);
    assert!(t.len() == 2 && t[1].contains("waits for its person: Allow Edit?]"), "{t:?}");
    // A call of its asker waits for the task's end.
    let _idle = call_later(handle, &ws, cx, &terminal, 2, "wait_for_idle", json!({ "block": them }));
    send(cx, json!({ "event": "working" }));
    send(cx, asks(json!("Allow Bash?")));
    after(cx, 300);
    assert_eq!(told(cx).len(), 2);
}
```

  `a_hooked_agent_gone_still_keeps_its_task_and_says_so` (`:1391`): its task goes without `wait` (`call`, not
  `call_later`) and mine is an asker waiting for its person (`asker(&ws, cx, mine);` and the `needs_you` event on mine
  before it), so its notice stays queued; `assert!(answers.try_recv().is_err(), "its task stays handed over")` becomes
  a read that `handed` is Some; after the `needs_you` answer:

```rust
    let told = notices(&ws, cx);
    assert!(
        told.len() == 1
            && told[0].1.contains(&format!(
                "({them}) waits for its person: it seems stuck, its screen still for 30 seconds]"
            )),
        "{told:?}"
    );
```

- [ ] **Step 2:** `cargo test -p templum -- a_dialog_in_a_task a_hooked_agent_gone`. Expected: FAIL (no notice).
- [ ] **Step 3: Implement.** `needs_you` gains `tell`, and in its `DRAWN`-later closure, after answering the waits,
  `if tell { ws.tell_waiting(&terminal, what.as_deref(), &screen, cx) }`. `tell_waiting`: the pane's `handed` (any,
  past its deadline too), none when `self.waits_on(&handed.by, terminal)`; else `queue_notice(handed.by, format!("[task
  t{} from {} waits for its person{asks}] {SCREEN_LEAD}\n{screen}", …))`, `asks` being `format!(": {}",
  clean_title(what))` for a `what` that isn't blank. Callers: `agent_event`'s `NeedsYou` arm passes `tell` computed
  before it sets the state, `!matches!(pane.state, Some(AgentState::NeedsYou(_)))`; `look_at_agents` passes `true`;
  `wait` passes `true` (a balloon's question).
- [ ] **Step 4:** `cargo test -p templum workspace::tests`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/waits.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/activity.rs crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): an agent hears when the task it handed over waits for its person, with the dialog as drawn"
```

---

### Task 8: A task's timeout comes back as failed

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs` (`time_out`, `timed_out`; `delegate` arms it)
- Modify: `crates/templum/src/workspace/spawn.rs:179-194` (`open_agent` arms it)
- Test: `crates/templum/src/workspace/tests/delegate.rs` (after Task 7's)

**Interfaces:**
- Consumes: `tell_end`, `waits_on`, `Handed::{id, until}`.
- Produces: `Workspace::time_out(&self, terminal: String, id: u64, until: Instant, seconds: u64, cx: &mut
  Context<Self>)` (a detached timer of `seconds`, then `timed_out`); `fn timed_out(&mut self, terminal: &str, id: u64,
  until: Instant, seconds: u64, cx: &mut Context<Self>)`.

- [ ] **Step 1: Write the failing test.**

```rust
/// A task handed over without `wait` gives no answer within its timeout: its asker hears it
/// failed, and later its end too. One it waits for hears its timeout from its call.
#[cfg(unix)]
#[gpui_kit::test]
fn a_task_past_its_timeout_comes_back_as_failed(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    asker(&ws, cx, mine);
    event(handle, &ws, cx, &terminal, json!({ "event": "needs_you", "message": "Allow Edit?" }));
    let told = |cx: &mut TestAppContext| -> Vec<String> {
        notices(&ws, cx).into_iter().map(|(_, t)| t).collect()
    };
    let task = json!({ "block": them, "task": "think", "timeout": 2 });
    call(handle, &ws, cx, &terminal, "delegate", task);
    // Its Enter goes while the test's clock runs: the screen shows the task in real time.
    screen_with(&ws, cx, theirs, "think");
    after(cx, 2000);
    after(cx, 1000);
    let t = told(cx);
    assert!(
        t.len() == 1
            && t[0].starts_with("[task t1 from ")
            && t[0].contains(": failed in ")
            && t[0].contains(&format!(
                "] What happened is in the pasted text.\nNo answer from {them} within 2 seconds; it may still be \
                 working, and its end comes back too."
            )),
        "{t:?}"
    );
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(theirs).unwrap().handed.as_ref().map(|h| h.id), Some(1))
    });
    // Its end, later.
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": "Late." }));
    after(cx, 1000);
    let t = told(cx);
    assert!(t.len() == 2 && t[1].contains(": done in ") && t[1].ends_with("\nLate."), "{t:?}");
    // With `wait`: its call says it timed out, and nothing is typed.
    let task = json!({ "block": them, "task": "ponder", "wait": true, "timeout": 2 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    screen_with(&ws, cx, theirs, "ponder");
    enter(cx);
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
    after(cx, 2000);
    assert!(answered(&answers).0, "timed out");
    after(cx, 1000);
    assert_eq!(told(cx).len(), 2);
}
```

- [ ] **Step 2:** `cargo test -p templum a_task_past_its_timeout`. Expected: FAIL (no notice at the timeout).
- [ ] **Step 3: Implement.** `delegate` without `wait`, after handing it over: `self.time_out(worker, id, until,
  timeout, cx)`; `open_agent` the same with `tools::TASK_WAIT`. `timed_out`: the pane's `handed` with that `id` and
  that `until` (a change sets another, a new task another id), and no call of its asker waiting on it
  (`waits_on`); then `tell_end(terminal, handed, &Reply::err(format!("No answer from {uid} within {seconds} seconds;
  it may still be working, and its end comes back too. The last lines of its screen:\n{}", tools::tail(&pane.tail(),
  tools::MAX_TEXT))), cx)`, `handed` left as it is (its end still comes). Its `ponytail:` comment, as `wait`'s: one
  timer per task, left to run out.
- [ ] **Step 4:** `cargo test -p templum workspace::tests`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): a task given no answer within its timeout comes back as failed, and its end later too"
```

---

### Task 9: The agent whose task it is changes it

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs:507-562` (`delegate`), consts
- Test: `crates/templum/src/workspace/tests/delegate.rs:150-184` (`delegate_refuses_what_it_mustnt_type_into`), new
  tests after Task 8's

**Interfaces:**
- Consumes: `time_out` (Task 8); `Handed::{id, taken, behind, until}`.
- Produces: `pub(super) const CHANGE_LEAD: &str = "The change is in the pasted text: do it."`.

- [ ] **Step 1: Write the failing tests.** In `delegate_refuses_what_it_mustnt_type_into`, the "On a task already"
  part becomes:

```rust
    // On the caller's own task: a change to it (another agent's: another_agents_task_is_still_refused).
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task.clone());
    assert_eq!((failed, text), (false, format!("Typed into {them} as a change to task t1.")));
```

  New:

```rust
/// The agent whose task it is changes it while its agent works: typed at once, under a line naming
/// the task; the end of the turn that took the change answers it, and its timeout starts again.
/// Before a spawned agent got its prompt nothing goes: it would come first.
#[cfg(unix)]
#[gpui_kit::test]
fn a_change_from_the_asker_goes_into_its_task(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let send = |cx: &mut TestAppContext, e: serde_json::Value| event(handle, &ws, cx, &worker, e);
    call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "say A" }));
    screen_with(&ws, cx, theirs, "say A");
    enter(cx);
    send(cx, json!({ "event": "submitted" }));
    let change = json!({ "block": them, "task": "also say B", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 2, "delegate", change);
    screen_with(
        &ws,
        cx,
        theirs,
        &format!("({uid}), a change to task t1] The change is in the pasted text: do it."),
    );
    ws.read_with(cx, |ws, _| {
        let handed = ws.space.terminal(theirs).unwrap().handed.as_ref().unwrap();
        assert!(handed.id == 1 && !handed.taken);
    });
    // The turn under way ends before taking it; Codex takes it into the next.
    send(cx, json!({ "event": "stopped", "message": "A" }));
    assert!(answers.try_recv().is_err(), "the turn before the change");
    send(cx, json!({ "event": "submitted" }));
    send(cx, json!({ "event": "stopped", "message": "B" }));
    assert_eq!(answered(&answers), (false, "B".to_string()));
    // Its timeout starts again from the change's.
    asker(&ws, cx, mine);
    event(handle, &ws, cx, &terminal, json!({ "event": "needs_you", "message": "Allow Edit?" }));
    call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "count", "timeout": 2 }));
    screen_with(&ws, cx, theirs, "count");
    enter(cx);
    send(cx, json!({ "event": "submitted" }));
    let change = json!({ "block": them, "task": "count to 9", "timeout": 5 });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", change);
    assert_eq!((failed, text), (false, format!("Typed into {them} as a change to task t2.")));
    // Its Enter goes while the test's clock runs: else no Enter, and the task ends untaken.
    screen_with(&ws, cx, theirs, "count to 9");
    after(cx, 2000);
    after(cx, 1000);
    assert!(notices(&ws, cx).is_empty(), "the first deadline passed quietly");
    after(cx, 2000);
    after(cx, 1000);
    let told = notices(&ws, cx);
    assert!(told.len() == 1 && told[0].1.contains("within 5 seconds"), "{told:?}");
    // A spawned agent whose prompt still waits.
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(theirs).unwrap();
        pane.handed = Some(crate::workspace::Handed {
            id: 7,
            ..crate::workspace::Handed::new(&terminal, std::time::Duration::from_secs(600))
        });
        pane.prompt = Some(crate::workspace::Prompt { text: "say hi".into(), screen: None, gated: false });
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "say ho" }));
    assert!(failed && text.contains("hasn't got task t7 yet"), "{text}");
}

/// Another agent's task: refused, whether it waits or not, and the task stays its asker's.
#[gpui_kit::test]
fn another_agents_task_is_still_refused(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let third = another_agent(handle, &ws, cx, theirs);
    let (terminal, _) = me(&ws, cx, mine);
    let (other, _) = me(&ws, cx, third);
    let (_, them) = me(&ws, cx, theirs);
    assert!(!call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "a" })).0);
    for wait in [false, true] {
        let task = json!({ "block": them, "task": "b", "wait": wait });
        let (failed, text) = call(handle, &ws, cx, &other, "delegate", task);
        assert!(failed && text.contains("already on a task another agent handed it"), "{text}");
    }
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(theirs).unwrap().handed.as_ref().unwrap().by, terminal)
    });
}
```

- [ ] **Step 2:** `cargo test -p templum -- delegate_refuses_what a_change_from another_agents_task`. Expected: FAIL ("is
  already on a task" for the caller's own).
- [ ] **Step 3: Implement.** In `delegate`, after `takes_handing`: `let mine = pane.handed.as_ref().filter(|h| h.by ==
  from).map(|h| h.id);`; the "already on a task" refusal only when `mine` is None; with `Some(id)`: a waiting `prompt`
  refuses `"{block} hasn't got task t{id} yet: it is starting. Change it once it has."`; else `send_task(&worker,
  &format!("[from {sig}, a change to task t{id}] {CHANGE_LEAD}\n{task}"), true, cx)`, then on its `handed` `taken =
  false`, `behind = false`, `until = Instant::now() + Duration::from_secs(timeout)`, a new `time_out` without `wait`,
  and the answer `Outcome::Wait(Waiting::Turn { … }, timeout)` with `wait`, else `Typed into {block} as a change to task
  t{id}.`. Its doc: Delegating 11.
- [ ] **Step 4:** `cargo test -p templum workspace::tests`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): an agent changes the task it handed over while it runs, instead of being refused"
```

---

### Task 10: Agents are told how tasks come back

**Files:**
- Modify: `crates/templum/src/workspace/bus.rs:614-624` (the instructions)
- Modify: `crates/templum-mcp/src/tools.rs:297-302` (`delegate`), `:358-362` (`spawn_agent`)
- Test: `crates/templum/src/workspace/tests/bus.rs:228-266`

- [ ] **Step 1: Write the failing test.** In `initialize_lists_the_tools_and_says_who_the_agent_is`, after the
  instructions' assert:

```rust
    assert!(
        instructions.ends_with(
            "A task you hand over without wait comes back typed into your terminal as [task …]: it isn't the \
             person writing."
        ),
        "{instructions}"
    );
```

  and after `tools/list`: `assert!(tools["result"].to_string().contains("[task t7"), "delegate says how its end comes
  back");`.
- [ ] **Step 2:** `cargo test -p templum initialize_lists`. Expected: FAIL.
- [ ] **Step 3: Implement.** The sentence at the instructions' end, after the note's. `delegate`'s description:
  "Hands a task to an agent connected to you: Templum types it into its terminal, after a line saying it is from you
  and its id (t7), and sends it. Its answer is its last message when it finishes its turn, or what it sends you with
  reply. With wait, waits for that answer (it may take minutes); without, says its id at once, and its end comes back
  typed into your terminal as [task t7 …] with its time, what it spent and its transcript, as does a dialog it waits
  on. To the agent on your task, it is a change to that task, typed at once. Refused while the agent waits for its
  person or is on another agent's task." `spawn_agent`'s: "A prompt is typed once the agent is ready, as a task you
  handed it (the result says its id): its end comes back as delegate's does, or wait_for_idle gets it."
- [ ] **Step 4:** `cargo test -p templum initialize_lists && cargo test -p templum-mcp`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/bus.rs crates/templum-mcp/src/tools.rs crates/templum/src/workspace/tests/bus.rs
git commit -m "feat(connections): agents are told a task's end comes back typed, and that it isn't their person writing"
```

---

### Task 11: The spec's status, the minors, the full check, the benches, push

**Files:** `docs/specs/connections.md`, `docs/minors.md`

- [ ] **Step 1: The spec:** `- **Status:** Batches 1 to 8 implemented`; anything the tasks changed from this plan's
  text (a message's wording) goes into the paragraph that says it.
- [ ] **Step 2: `docs/minors.md`.** Under ## Connections: delete "**No test hands a second task from another agent.**"
  (`another_agents_task_is_still_refused` pins it: the minor of the design's line 599); in "**`close_block` closes a
  block the person is typing in, or took over.**", "(the counter batch 8 adds for its notices)" becomes "(`Pane::keyed`,
  batch 7's)". Add "Left by batch 8; line numbers are those of `development` at <Task 10's commit>." with (plus whatever
  the reviews defer):
  - **What Claude Code spent comes back only with its status line, and Gemini CLI's never.** `spent`
    (`crates/templum/src/workspace/delegate.rs`) takes the usage chip's figures: a Claude Code without the status line
    (`usage.claude_limits` off) gives no cost, and its transcript-read session no tokens; Gemini CLI's session isn't
    read. A person sees `[task t7 …: done in 31 s, transcript …]` without a figure. Fix: sum the `usage` of the
    assistant lines its transcript (`transcript_path`) gained since the hand-over.
  - **A notice doesn't count its task's tool calls.** `tell_end` (`delegate.rs`) says time, spending and transcript;
    Claude Code's subagents also say how many tools they ran. Fix: count `Working` events (each tool's end) on
    `Handed` from the hand-over.
  - **A task past its timeout that another agent takes over never comes back.** Its notice said "its end comes back
    too", but `delegate` (`delegate.rs`) lets another agent hand that agent a task once the deadline passed, replacing
    `handed`; the first asker never hears. Fix: when replacing a timed-out task of another asker, tell that asker it
    failed ("{uid} took another agent's task").
  - **A waited task whose call its client dropped has its timeout told by no one.** `time_out` runs only for a task
    handed without `wait`; Claude Code stops waiting on a call past 2 minutes, and a task that then hangs is never
    said to have timed out (its end still comes). Fix: arm the timer for every task, telling only when no call waits.
  - **A notice reaches an asker inside another tool call only once that call returns.** Claude Code and Codex queue
    typed text while a tool runs (batch 7's S2, S3): an asker waiting on `delegate` with `wait` to another agent, or
    on `ask_user`, reads a notice of another task late. A person sees the asker answer it minutes after it was typed.
    Fix: none needed unless it bites; the notice's time says when the task ended.
  - Under ## Agent usage: **Claude Code's tokens in a terminal's usage tooltip are its context, not what it used.**
    `relayed` (`crates/templum-usage/src/claude.rs`) sums `context_window.total_input_tokens` and
    `total_output_tokens`, which batch 8's spike S8-1 found are its last call's (38279 = 10 + 10 + 38259 cached); the
    tooltip's "Tokens: 38k" reads as the session's. Fix: drop `tokens` for a relayed session (its cost is shown), or
    name it "Context".
- [ ] **Step 3: The full check:** `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo
  test --workspace`, then `cargo deny check`. Expected: all pass; report the test count.
- [ ] **Step 4: The benches** (Linux, release): `cargo run --release -p templum -- --bench` and `cargo run --release
  -p templum -- --bench-mixed`; keep both JSON lines. Expected: `p95_ms` ≤ 16.7 each.
- [ ] **Step 5: Commit and push.**

```bash
git add docs/specs/connections.md docs/minors.md
git commit -m "docs(connections): batch 8 implemented; its minors"
git push origin development
```

  The push updates draft PR #36 (dev→main), which runs CI on the 3 OS (`gh pr checks 36`): add batch 8 to its title
  and description, with the two bench JSON lines.

## Manual checks (Hyprland, `cargo run --release -p templum`, the user's bash with ble.sh)

1. **The user's own test:** Claude Code from the canvas menu spawns a Codex with `model gpt-5.6-luna` and `effort
   low`, without a prompt, then delegates without `wait` "run `sleep 20`, then say done": its answer is "Handed to
   <id> as task t1."; Claude Code goes on (or idles); about 20 s later its terminal gets `[task t1 from Codex … (<id>):
   done in 2x s, N tokens, transcript /…/rollout-….jsonl] Its answer is in the pasted text.` with Codex's answer
   pasted, and Claude Code takes it as the task's end, not as its person writing. N is close to the growth of Codex's
   own `/status` tokens; the transcript file exists.
2. **Claude Code as the worker:** the same task to a Claude Code (with `usage.claude_limits` on): `$0.0x`, about the
   change of its usage chip's cost; `/exit` in it mid-task: `failed … Its agent ended before it answered.`
3. **A change:** delegate "count to 30, one number a second with `sleep 1`", then, while it runs, "also say B at the
   end": the second is typed under `[from …, a change to task t1] The change is in the pasted text: do it.`; Codex
   steers into it; the one notice that comes is the end of the turn that took the change. Again into a Claude Code:
   its queued message, the same end.
4. **A dialog:** a Codex started with `-s read-only` handed "touch a file in this folder": the asker gets `[task t1 …
   waits for its person: Allow Bash?] Its screen is in the pasted text.` once, with the dialog's options; it answers
   with `send_keys`, and the end comes back. While the asker itself shows a permission dialog, a task ends: nothing
   is typed until the person answers it; typing into the asker: the notice comes 3 s after the last key.
5. **A timeout:** delegate with `timeout: 10` a `sleep 30` task: at 10 s `failed … No answer from <id> within 10
   seconds…`, at 30 s `done …`.
6. **Closing and stopping:** the person closes the worker's block mid-task: `failed … was closed before it answered.`;
   `interrupt`, then `close_block` by the asker on another task: nothing typed.
7. **A script:** `templum delegate <id> "say hi"` in a plain terminal prints `Handed to <id> as task tN.`; nothing is
   typed into that shell when it ends.
8. **Benches** pasted in PR #36 (Task 11).

## Self-review

- Spec coverage: ids, the first line, `delegate`'s and `spawn_agent`'s results, `reply`'s line (Task 2); a turn's
  transcript (Tasks 1, 5); the end's notice, waiting calls, cancelled calls, never into a shell (3); the asker's waits,
  oldest first, 3 s apart (4); what it spent, read again a second after (5); left, closed, `interrupt` and
  `close_block` silent (6); a dialog once per wait, seems stuck, a balloon, none while a call waits (7); the timeout,
  none for a waited task (8); a change, its timeout, a spawn's prompt, another agent refused (9); the instructions and
  the descriptions (10); the status, the minors, the design's minor 599 (11). Testing "Batch 8": every item has its
  test (Tasks 3 to 9).
- Types: `Stopped { message, transcript }` (1) used by 5; `Handed::id`, `next_task` (2) used by 3, 7, 8, 9; `Notice`,
  `notices`, `tell_end`, `queue_notice`, `deliver_notices`, `takes_notice`, `signature_of`, `waits_on` (3) used by 4 to
  9; `Pane::noticed` (4); `Handed::usage`, `Pane::transcript`, `SPENT_AFTER`, `spent`, `proc_of` (5); `needs_you(…,
  tell, …)`, `tell_waiting`, `SCREEN_LEAD` (7); `time_out`, `timed_out` (8) used by 9; `CHANGE_LEAD` (9). Test helpers:
  `asker`, `notices`, `done_task` (3), `notices_until` (5), `another_agent` (6).
- The design's test names kept or renamed: `a_task_handed_without_wait_comes_back_typed`,
  `a_waiting_call_gets_the_answer_and_no_notice`, `a_dialog_in_a_task_is_told_once`,
  `a_change_from_the_asker_goes_into_its_task`, `another_agents_task_is_still_refused`,
  `a_task_past_its_timeout_comes_back_as_failed` as named; `a_notice_waits_while_its_asker_asks_its_person` also
  covers starting, typing and the gap; `a_notice_never_goes_into_a_shell` also an agent that quit.
