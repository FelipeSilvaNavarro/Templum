# Agent Protocol: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kanban card 69 (with 68 and the protocol part of 58), phase 1: whatever the agent, a task is answered
through Templum. Every task says to answer with `reply`; `reply` needs no block and ends the task you are on (also
from a shell, `templum reply -`); the screen never ends the task of an agent that has Templum's tools, it only tells
its master once that it went quiet; a turn's end doesn't end a task while its agent waits on one it handed; the master
learns which agents have no Templum tools, and why; `reply` asks no permission in Claude Code, Codex and Copilot CLI.

**Architecture:** All in the existing plumbing. `templum-mcp`: `Call::ReplyTo`'s block becomes optional (`tools.rs`),
`templum reply -` reads stdin (`cli.rs`), Codex's flags gain `reply`'s approval (`lib.rs`). `templum-usage`: Claude
Code's settings file and Copilot CLI's words gain `reply`'s permission. `crates/templum/src/workspace/`: a task's
end on `reply` (`delegate.rs`, `finish_task`), the lines a task is typed with (`delegate.rs`, `spawn.rs`), the
server's instructions and `list_blocks` (`bus.rs`), the MCP connections seen per terminal (`mod.rs`, `bus.rs`,
`waits.rs`), the still screen of an agent with tools (`activity.rs`, `delegate.rs`), a turn's end while its agent
hands out a task (`bus.rs`).

**Tech Stack:** Rust 2024; gpui-kit's headless tests (`#[gpui_kit::test]`, `TestAppContext`) with the fake agents of
`crates/templum/src/workspace/tests/delegate.rs` (`two_agents`, `agents_running`, `me`, `found`, `asker`, `call`,
`call_later`, `answered`, `event`, `shows`, `ticks`, `screen_with`, `notices`, `notices_until`, `after`); `serde_json`.

**Spec:** [docs/specs/agent-protocol.md](../specs/agent-protocol.md). Read it first: it holds every decision this
plan follows and the exact words. [connections.md, Delegating](../specs/connections.md#delegating) explains what is
there today (tasks, `reply`, notices, `Handed`).

## Global Constraints

- Copy, verbatim from the spec (`{id}` is the task's number, `{uid}` a block id, `{title}` an agent's title):
  - `TASK_LEAD`: `Your task is in the pasted text: do it, then answer it with reply.`
  - `CHANGE_LEAD`: `The change is in the pasted text: do it, then answer the task with reply.`
  - `answer_how(id)`: `\n\n(When task t{id} is done, call Templum's reply tool once, with your whole answer as its
    text: that ends the task and hands your answer over. Without Templum's tools, run `templum reply -` in your shell,
    the answer on its standard input.)` (one line after the blank one; the backticks are in the text)
  - instructions, after "…it isn't the person writing.": ` A task handed to you begins [from …, task t7]: when it is
    done, answer it once with reply, your whole answer as its text. When you need the person, ask with ask_user, not
    on your screen.`
  - `reply` with no task: `You are on no task: reply answers the task an agent handed you; name a block to write to
    one.`; with a blank text: `reply needs your answer as text.`; its answer: `Answered task t{id}.`
  - quiet, to a waiting call (error result): `{uid} hasn't answered task t{id}: its screen has been still for 5
    seconds. It may be done without replying, stopped, or asking its person. The task stays open, and its reply still
    ends it: wait_for_idle waits for it, send_keys answers a dialog, and delegate to it again asks it for its answer.
    The last lines of its screen:\n` + the last `LAST_LINES` (40) lines
  - quiet, as a notice: `[task t{id} from {signature} hasn't answered: its screen is still] Its screen is in the
    pasted text.\n` + the last `TAIL_LINES` (20) lines (`SCREEN_LEAD` is that middle sentence)
  - `list_blocks`: `, no Templum tools` after an agent's state
  - `delegate`'s answer, appended: ` {uid} has no Templum tools ({why}): it can still answer with `templum reply -`
    in its shell; else its turn's end or its screen going still ends the task.`
  - the `why`s: the spec's list in [8](../specs/agent-protocol.md#8-templums-tools-checked), word for word
  - permissions: `mcp__templum__reply` (Claude Code), `mcp_servers.templum.tools.reply.approval_mode="approve"`
    (Codex), `--allow-tool=templum(reply)` (Copilot CLI)
- Only `crates/templum` uses gpui-kit; no new dependency, crate, module or trait; no `unsafe`.
- Tests never run a real agent: fake agents as above. PTY-bound tests are `#[cfg(unix)]` like their neighbors.
- Every task's tests fail without its change. Commit each task on `development` (Conventional Commits, `feat` and
  `fix` written for someone who uses Templum); add only your own files: another agent may commit on `development`.
- While iterating, run only the affected tests: `cargo test -p <crate> <filter>`. Run the full
  `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` once, in Task
  8. Two tests used to read the real `~/.claude` and fail on this machine only:
  `agents::a_saved_agent_is_resumed_in_its_block` and
  `persistence::two_blocks_one_folder_two_accounts_resume_their_own`. Card 66's `dd50039` stopped that; if they still
  fail here, they are not this plan's.
- Windows: `cargo clippy -p templum -p templum-usage -p templum-terminal -p templum-mcp --all-targets --target
  x86_64-pc-windows-msvc -- -D warnings` with
  `CC_x86_64_pc_windows_msvc=/tmp/claude-1000/-home-winx-Documents-github-Templum/44a7470c-c569-44f5-9abb-567b507ba8d9/scratchpad/fake-cc.sh`
  and `AR_x86_64_pc_windows_msvc=llvm-lib`.
- Never wait on a process with a `pgrep -f "<its command>"` loop (it matches the waiting shell). Never send keys or
  mouse events to a window. Checks by hand run in a scratch folder, never in the person's agent files, and spend
  as little of an agent's usage as they can.
- No painting changes: no bench.

## Review Focus

1. **A subagent replies twice.** The second is refused ("You are on no task…") and its master hears one end only.
   Test: Task 1.
2. **A blank reply** (`""`, `"  "`) would end a task with nothing. Refused, the task still open. Test: Task 1.
3. **A quiet subagent then replies.** Exactly one end comes, the reply's; a second still screen tells nothing. Test:
   Task 6.
4. **The master changes a quiet task** (`delegate` again). Quiet starts over: a later still screen tells it once more.
   Test: Task 6.
5. **The agent's server goes away** (it quit its MCP, the bridge died): `list_blocks` says `no Templum tools` again,
   and its next still screen ends the task as an agent without tools. Test: Task 4 and Task 6.

---

### Task 1: `reply` without a block ends the task you are on

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (`Call::ReplyTo`, `parse`'s `"reply"`, `reply`'s schema and description)
- Modify: `crates/templum/src/workspace/delegate.rs` (`reply_to`, new `finish_task`)
- Modify: `crates/templum/src/workspace/bus.rs` (`call`'s `Call::ReplyTo` arm)
- Test: `crates/templum-mcp/src/tools.rs` (its tests), `crates/templum/src/workspace/tests/delegate.rs`,
  `crates/templum/src/workspace/tests/templum_agent.rs`

**Interfaces:**
- Produces: `Call::ReplyTo { block: Option<String>, text: String }`; `Workspace::finish_task(&mut self, terminal:
  &str, text: &str, cx: &mut Context<Self>) -> Reply` (pub(super)): ends the task `terminal`'s agent is on with `text`.

- [ ] **Step 1: Write the failing tests**

In `tools.rs`'s tests, `reply_needs_only_its_text`: `parse("reply", &json!({"text": "Line 42."}))` is
`Ok(Call::ReplyTo { block: None, text: "Line 42.".into() })`; with `"block": "k9x1d"` it is `Some("k9x1d")`; `list()`'s
`reply` has `required == ["text"]`.

In `tests/delegate.rs`, `#[cfg(unix)]`:

```rust
fn reply_without_a_block_answers_the_task_you_are_on(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let reply = |text: &str| json!({ "text": text });
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply("Done."));
    assert!(failed && text.starts_with("You are on no task"), "{text}");
    let task = json!({ "block": them, "task": "find the bug", "wait": true });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply("  "));
    assert_eq!((failed, text.as_str()), (true, "reply needs your answer as text."));
    assert!(waiting.try_recv().is_err(), "still open");
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply("Line 42."));
    assert_eq!((failed, text.as_str()), (false, "Answered task t1."));
    assert_eq!(answered(&waiting), (false, "Line 42.".to_string()));
    // Once: the task is over.
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply("Again."));
    assert!(failed && text.starts_with("You are on no task"), "{text}");
}
```

`a_reply_with_no_one_waiting_is_the_tasks_end` (`#[cfg(unix)]`): `two_agents`, `asker(&ws, cx, mine)`, `delegate`
without `wait`, then `reply` `{"text": "Line 42."}` from the worker, `after(cx, 1000)`: `notices(&ws, cx)` holds one
for mine whose text starts `[task t1 from `, contains `: done in ` and ends `Its answer is in the pasted text.\nLine
42.`. Change the second half of `reply_with_no_one_waiting_types_into_the_agent` (the reply to the task mine handed)
to expect that notice instead of `[reply from … to task t1]`; its first half (a reply to an agent that handed nothing)
stays.

In `tests/templum_agent.rs`, `a_reply_ends_the_templum_agents_task`: as `a_tasks_end_wakes_the_templum_agent`, but the
worker calls `reply` `{"text": "Card 4 is done."}` (`super::bus::call` from `terminal`) instead of `end_turn`; the
same `Done` notice row and the same `sent` lines.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp reply_needs_only && cargo test -p templum reply_ a_reply_`
Expected: FAIL (`reply needs "block"`; the typed `[reply from …]`).

- [ ] **Step 3: Implement**

- `parse`: `block: optional("block")?`, `text: text("text")?`. Schema: `"required": ["text"]`, `block` described
  "The agent to write to; leave it out to answer the task you are on.", and the spec's description of `reply` (2).
- `bus.rs`: `Call::ReplyTo { block, text } => self.reply_to(terminal, block.as_deref(), &text, cx)`.
- `reply_to(&mut self, from: &str, block: Option<&str>, text: &str, cx) -> Reply`: a blank `text` is refused first;
  `None` → `finish_task`; `Some(b)` whose terminal is the asker of `from`'s task (`Pane::handed.by`) → `finish_task`;
  any other block → today's code without its `to task` part (no task of the caller's ends).
- `finish_task`: none handed → the "no task" refusal; else take `handed`, set the pane's `answer`, answer the asker's
  waits on `terminal` (`Waiting::Turn` with `w.from == handed.by`) with `Reply::ok(text)`, or `tell_end(terminal,
  &handed, &Reply::ok(text), cx)` when none waits; `cx.notify()`; `Reply::ok("Answered task t{id}.")`. Other agents'
  waits on its turn stay for the turn's end. Existing tests that expect `Answered {block}.` from a reply to its asker
  (`reply_answers_the_task_before_its_turn_ends`) follow the new answer.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum-mcp tools && cargo test -p templum delegate templum_agent`
Expected: PASS.

- [ ] **Step 5: Commit**

`feat(connections): an agent answers the task it is on with reply, no block needed, and its master hears it as the task's end`

### Task 2: `templum reply -` reads the answer from standard input

**Files:**
- Modify: `crates/templum-mcp/src/cli.rs` (`call_of`'s `"reply"`, its usage line, `run`'s reading of `-`)
- Test: `crates/templum-mcp/src/cli.rs` (its tests)

**Interfaces:**
- Consumes: Task 1's `reply` arguments (`block` optional).

- [ ] **Step 1: Write the failing test**

`reply_reads_its_answer_from_stdin`, as `plan_sends_the_files_text`: `run_with(&["reply", "-"], &env(&bus), "Line
42.\n")` sends `tools/call` `reply` with arguments `{"text": "Line 42.\n"}` and no `block`; `run_with(&["reply",
"k9x1d", "-"], …, "Line 42.")` sends `{"block": "k9x1d", "text": "Line 42."}`; `["reply", "k9x1d", "Line", "42."]`
still sends `{"block": "k9x1d", "text": "Line 42."}`; `["reply"]` prints the usage `reply [<block>] <text>|- [--json]`
and exits 2 (as every usage error does).

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum-mcp reply_reads`
Expected: FAIL.

- [ ] **Step 3: Implement**

`call_of`: `("reply", ["-"])` → `{"text": "-"}`; `("reply", [block, text @ ..])` as today; the usage line above. In
`run`, beside `propose_plan`'s: a `reply` whose `text` is `"-"` gets stdin's text (an unreadable stdin: `templum:
couldn't read stdin: {e}`, exit 2).

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test -p templum-mcp cli`
Expected: PASS.

- [ ] **Step 5: Commit**

`feat(connections): templum reply - answers your task with what comes on standard input`

### Task 3: Every task says how to answer

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs` (`TASK_LEAD`, `CHANGE_LEAD`, new `answer_how`, `hand_task`)
- Modify: `crates/templum/src/workspace/spawn.rs` (the prompt's text)
- Modify: `crates/templum/src/workspace/bus.rs` (`instructions`)
- Modify: `crates/templum-mcp/src/tools.rs` (`delegate`'s description)
- Test: `crates/templum/src/workspace/tests/delegate.rs`, `tests/spawn.rs`, `tests/bus.rs`

**Interfaces:**
- Produces: `pub(super) fn answer_how(id: u64) -> String` in `delegate.rs`.

- [ ] **Step 1: Write the failing tests**

- `tests/delegate.rs`, `#[cfg(unix)]` `a_task_and_its_change_say_how_to_answer`: after `delegate` `{"block": them,
  "task": "find the bug"}`, the worker's screen shows (`screen_with`) `do it, then answer it with reply.` and
  `call Templum's reply tool once`; after a second `delegate` from mine (a change), `do it, then answer the task with
  reply.`. Assert `answer_how(7)` is the spec's text with `t7` (a plain `#[test]`).
- `tests/spawn.rs`: the prompt typed into a spawned agent (the test that checks its first line today) ends with
  `answer_how(<its id>)`.
- `tests/bus.rs`: the `initialize` test's `instructions` contain `answer it once with reply, your whole answer as its
  text` and `ask with ask_user, not on your screen`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum how_to_answer spawn instructions`
Expected: FAIL.

- [ ] **Step 3: Implement**

The two leads and the instructions' sentences as the Global Constraints give them; `hand_task` and `spawn.rs` append
`answer_how(id)` after the task's text (a change too). `delegate`'s description: "Its answer is what it sends with
reply (Templum asks it to), else its last message when its turn ends." in place of today's sentence. Existing tests
that assert the whole typed text or `TASK_LEAD` follow the constants.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum delegate spawn bus`
Expected: PASS.

- [ ] **Step 5: Commit**

`feat(connections): every task an agent gets asks for its answer with reply, and says how without Templum's tools`

### Task 4: Templum knows which agents have its tools

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs` (`Workspace::tool_conns`, its constructor)
- Modify: `crates/templum/src/workspace/bus.rs` (`serve`'s `initialize`, `has_tools`, `no_tools_why`, `block_lines`)
- Modify: `crates/templum/src/workspace/waits.rs` (`closed`)
- Modify: `crates/templum/src/workspace/delegate.rs` (`delegate`'s answer)
- Test: `crates/templum/src/workspace/tests/bus.rs`, `tests/delegate.rs`

**Interfaces:**
- Produces: `Workspace::tool_conns: HashMap<u64, String>` (a connection → its terminal);
  `pub(super) fn has_tools(&self, terminal: &str) -> bool`; `pub(super) fn no_tools_why(&self, pane: &Pane) -> String`.

- [ ] **Step 1: Write the failing tests**

Two helpers in `tests/bus.rs`, used by this task and Task 6: `pub(super) fn has_server(handle, ws, cx, terminal: &str,
conn: u64)` sends `initialize` from an `mcp` hello on connection `conn` (as `ask_as`, whose connection is always 0),
and `pub(super) fn server_gone(handle, ws, cx, terminal: &str, conn: u64)` sends the `templum_mcp::bus::CLOSED`
notification on it.

`tests/bus.rs`, `an_agents_tools_are_its_server_connected` (`#[cfg(unix)]`, `two_agents`): `list_blocks` from mine
says `, no Templum tools` on theirs' line; after `has_server(…, &worker, 7)` it doesn't; after `server_gone(…, &worker,
7)` it does again. An `initialize` from `cli` or `hook` doesn't count; nor one while `[connections] enabled` is false.

`tests/delegate.rs`, `delegate_says_why_an_agent_has_no_tools`: `delegate` to theirs (`seen.agent` `codex`, no
`initialize`) answers `Handed to {them} as task t1. {them} has no Templum tools (its server hasn't connected: …): it can
still answer…`; with theirs `seen.agent = Some("amp".into())`, the `why` is `Templum can't give Amp its server for one
session: add `templum mcp` to its own MCP servers (Settings ▸ Connections ▸ Other agents)`; after its `initialize`,
just `Handed to {them} as task t2.`. Unit-test `no_tools_why` for connections off, a WSL shell, `gemini` (no extension
in a temporary data folder) and `pi`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum tools_are no_tools`
Expected: FAIL.

- [ ] **Step 3: Implement**

`serve`: on `initialize` from `hello.client == "mcp"` while not `off`, `tool_conns.insert(conn, hello.terminal)`;
`closed(conn)` removes it. `has_tools`: any value equal to `terminal`. `block_lines`: an agent's line (its pane's
`seen.agent` is some) gets `, no Templum tools` after `agent_state` when `!has_tools`. `no_tools_why`: the spec's
first fitting reason (8); the agent by `templum_terminal::Agent::from_name` on `seen.agent`; WSL by the pane's shell
(`templum_terminal::is_shell(&shell.program, "wsl")`); Gemini CLI's extension as the notice in `workspace/usage.rs`
finds it (`templum_usage::gemini::installed`). `delegate` (not a change) appends the sentence when its target has no
tools.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum bus delegate`
Expected: PASS.

- [ ] **Step 5: Commit**

`feat(connections): list_blocks and delegate say which agents have no Templum tools, and why`

### Task 5: A turn's end doesn't end a task while its agent waits on one it handed

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs` (new `hands_out`)
- Modify: `crates/templum/src/workspace/bus.rs` (`agent_event`'s `Stopped` arm)
- Test: `crates/templum/src/workspace/tests/delegate.rs`

**Interfaces:**
- Produces: `pub(super) fn hands_out(&self, terminal: &str) -> bool`: some pane, in any workspace, has a `handed`
  whose `by` is `terminal` (past its timeout too; a spawn's prompt included).

- [ ] **Step 1: Write the failing test**

`a_subagent_waiting_on_its_own_task_isnt_done` (`#[cfg(unix)]`): `two_agents` plus `another_agent(…, theirs)` (a third
wired to theirs); `asker(&ws, cx, mine)`. Mine hands theirs `t1`; theirs takes it (`submitted`) and hands the third
`t2`; theirs' `stopped` with `"Waiting for k…"` ends nothing (`notices` has none for mine, theirs still has `handed`
`t1`). The third's `submitted` and `stopped` `"Found it."` end `t2` (a notice for theirs). Theirs' next `submitted`
and `stopped` `"Line 42."` end `t1`: one notice for mine, ending `Line 42.`.

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum its_own_task`
Expected: FAIL (`t1` ends with "Waiting for k…").

- [ ] **Step 3: Implement**

In `agent_event`, read `hands_out(terminal)` before borrowing the pane; in `Stopped`, after the arm that keeps a task
not taken or behind, a task whose agent `hands_out` stays handed (its `answer` is still set); else `end_turn` as today.

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test -p templum delegate`
Expected: PASS.

- [ ] **Step 5: Commit**

`fix(connections): a subagent that waits on its own subagent isn't taken for done when its turn ends`

### Task 6: Quiet: a still screen doesn't end the task of an agent with Templum's tools

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs` (`Handed::quiet: bool`, false in `Handed::new`)
- Modify: `crates/templum/src/workspace/activity.rs` (`look_at_agents`'s still screens)
- Modify: `crates/templum/src/workspace/delegate.rs` (new `went_quiet`; `hand_task`'s change sets `quiet` false)
- Test: `crates/templum/src/workspace/tests/delegate.rs`

**Interfaces:**
- Consumes: Task 4's `has_tools`, Task 5's `hands_out`, `activity::STILL` (make it `pub(super)`).
- Produces: `pub(super) fn went_quiet(&mut self, terminal: &str, cx: &mut Context<Self>)`.

- [ ] **Step 1: Write the failing tests**

All `#[cfg(unix)]`, theirs given tools by `has_server(…, &worker, 7)` (Task 4), its hooks silent:

- `a_quiet_agent_keeps_its_task_and_its_waiting_master_hears_so`: mine `delegate`s with `wait`; theirs `ticks` 4,
  then 5 still seconds: the call's answer is an error starting `{them} hasn't answered task t1: its screen has been
  still for 5 seconds.` and containing `tick 3`; theirs still has `handed`; mine's `wait_for_idle` waits (no answer);
  theirs' `reply` `"Line 42."` answers it with `Line 42.`.
- `a_quiet_agent_is_told_once_to_a_master_not_waiting`: `asker(…, mine)`, `delegate` without `wait`; quiet: one notice
  for mine starting `[task t1 from ` and containing `hasn't answered: its screen is still] Its screen is in the pasted
  text.`; `ticks` again then still again: no second one; `reply` `"Line 42."`: one more notice, the task's end,
  ending `Line 42.`, and nothing after it (Review Focus 3).
- `a_change_starts_quiet_over`: as the one above up to its first quiet notice; then mine `delegate`s to theirs again
  (a change to `t1`), theirs `ticks` and goes still: a second quiet notice (Review Focus 4).
- `a_quiet_agent_without_tools_ends_its_task_by_its_screen`: as the first, but `server_gone(…, &worker, 7)` comes
  before the still seconds: the call's answer is today's `Its screen has been still for 5 seconds…`, not an
  error (Review Focus 5).
- `a_quiet_agent_waiting_on_its_own_task_says_nothing`: theirs hands a third agent a task (Task 5's setup) before it
  goes still: no answer, no notice.

`a_screen_at_work_says_working_and_one_gone_still_ends_its_task` stays green as it is (no task, no tools).

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum quiet`
Expected: FAIL (the task ends with the screen).

- [ ] **Step 3: Implement**

`look_at_agents`: for each terminal gone still, when its pane doesn't `speak` and `has_tools`, `went_quiet` instead
of `end_turn`. `went_quiet`: nothing without a `handed`, when it is `quiet` already, or when `hands_out(terminal)`;
else set `quiet`, answer the asker's waits on it (`Waiting::Turn`, `w.from == handed.by`) with `Reply::err(<quiet
text>)` or, with none, `notice_to(handed.by, <quiet notice>, terminal, NoticeLevel::Waiting, cx)`, and answer the
other waits on its turn with today's still-screen text (`Reply::ok`). Factor today's still-screen `Reply` into one
function both use.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum delegate activity`
Expected: PASS.

- [ ] **Step 5: Commit**

`fix(connections): an agent with Templum's tools that goes still keeps its task, and its master hears once that it hasn't answered`

### Task 7: `reply` asks no permission in Claude Code, Codex and Copilot CLI

**Files:**
- Modify: `crates/templum-usage/src/claude.rs` (`settings_file`)
- Modify: `crates/templum-mcp/src/lib.rs` (`codex_args`)
- Modify: `crates/templum-usage/src/copilot.rs` (`args`)
- Test: the same files' tests; `crates/templum/src/workspace/tests/usage.rs` (what is typed)

- [ ] **Step 1: Write the failing tests**

- `claude.rs`, in `the_settings_file_holds_the_status_line_the_hooks_or_both`: with `hooks`, `written["permissions"]
  == json!({"allow": ["mcp__templum__reply"]})`; without, no `permissions`.
- `lib.rs`, in `codex_args_are_the_flags_unquoted` and `codex_gets_the_server_and_its_hooks_in_single_quotes`: the
  values hold `mcp_servers.templum.tools.reply.approval_mode="approve"` (with and without hooks), quoted `'…'` in the
  flags; adjust the assertion that the flags end with `tool_timeout_sec=604800'`.
- `copilot.rs`, `copilot_gets_the_server_and_a_plugin_of_hooks`: the words are `--allow-all-mcp-server-instructions`,
  `--allow-tool=templum(reply)`, then the two paths (the plugin's last: the shell function's "nothing twice" looks at
  the last word).
- `tests/usage.rs`: the line typed for a Copilot CLI holds `"--allow-tool=templum(reply)"` in double quotes
  (`typed_path`), and `TEMPLUM_COPILOT_ARGS` the bare word.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-usage settings_file copilot && cargo test -p templum-mcp codex && cargo test -p templum copilot`
Expected: FAIL.

- [ ] **Step 3: Implement**

The three words in the spec's table (9), in those functions; nothing else changes in them.

- [ ] **Step 4: Run them to see them pass, then check by hand**

Run: the same commands. Expected: PASS. By hand, in a scratch folder: `codex` started with the new flags (its line
from `TEMPLUM_CODEX_ARGS`, in a Templum terminal) opens without a new "Hooks need review" if its hooks were trusted
before; if it does ask again, say so in the commit body. Where Copilot CLI is signed in, one `copilot -p` turn of a
subagent that calls `templum-reply` asks nothing; if it isn't, write "not checked by hand" in the commit body.

- [ ] **Step 5: Commit**

`feat(connections): reply runs without asking in Claude Code, Codex and Copilot CLI`

### Task 8: The specs, the minors, the whole check

**Files:**
- Modify: `docs/specs/connections.md` (Delegating 2, 4, 6, 8 and 10; the Tools table's `reply` and `delegate` rows;
  the command line's `templum reply` row; An agent's state's sentence on a still screen ending a task)
- Modify: `docs/specs/agent-parity.md` (one line under After card 44 pointing at agent-protocol.md for what each agent
  answers with now)
- Modify: `docs/specs/agent-protocol.md` (Status: Implemented, phase 1; "Calls made while implementing it")
- Modify: `docs/minors.md` (remove an entry a task fixed; add what a task deferred)

- [ ] **Step 1:** Write the docs: what each task changed, in the specs' own words; nothing the code doesn't do.
- [ ] **Step 2:** Run `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`. Expected: every test passes (see the Global Constraints on the two that read `~/.claude`).
- [ ] **Step 3:** Run the Windows clippy of the Global Constraints. Expected: no warning.
- [ ] **Step 4: Commit** `docs: the agent protocol is implemented (phase 1)`, and report the check's output as it was.

---

## Phase 2 (outlined)

Each item is its own plan after a spike with the agent installed, as card 44's batches were; none blocks phase 1.

1. **OpenCode's TUI, by hand** (card 58): a subagent OpenCode in a Templum block on a free model: does the plugin's
   `chat.message` and `session.idle` come in the TUI, does the task's paste get its Enter, does `permission.asked`
   turn its dot yellow, does it call `reply`. Fix what doesn't; the rest into minors.
2. **Amp** (card 58): `--mcp-config '<json>'` for one session (its servers' environment, their timeout, its
   permission for a tool), a TS plugin's `agent.end` for its turn's end if one can be given for a session; an `amp`
   shell function as Copilot CLI's.
3. **Cline** (card 58): `CLINE_MCP_SETTINGS_PATH` naming a file of Templum's in the data folder, and `--hooks-dir`
   for its turns; a `cline` shell function.
4. **Cursor CLI:** a plugin (`--plugin-dir`) declaring the server and hooks, with `--approve-mcps`.
5. **The person sees it too:** "no Templum tools" on the block (its dot's tooltip) and a notification when an agent
   Templum gave its server hasn't connected 30 seconds after it was found.
6. **Codex's `request_user_input` and Gemini CLI's `ask_user`** heard as a wait for the person (agent-parity gap 13).
7. **Droid, Auggie, Goose, Kiro CLI:** their per-session server (agent-parity gap 10), when someone uses them.
