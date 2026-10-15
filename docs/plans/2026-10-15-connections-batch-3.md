# Connections, batch 3 (agents together): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** an agent asks the person a question, or to be wired to another block, in a balloon on its block; hands a
task to a wired agent and gets its answer back (the end of that agent's turn, or its `reply`); waits for a wired
agent to finish; and opens a new agent beside itself, wired to it, in a new Git worktree if it asks. A dot travels
along the wire while a task is handed over.

**Architecture:** `templum-mcp` gains the six tools' schemas and typed calls (`tools.rs`), their verbs (`cli.rs`), a
number per connection and a `templum/closed` message when a connection ends (`bus.rs`). In the app a tool call either
answers now or becomes a `Wait` in `Workspace::waits` (`workspace/waits.rs`) that keeps its reply channel until the
person answers in the balloon (`balloon.rs`, drawn by `block.rs` at the Full level), the agent waited on ends its turn
(a hook event), the call is cancelled or its connection ends, its block closes (checked each frame), or its deadline's
timer fires. `workspace/delegate.rs` types tasks (one paste, then Enter), keeps the task handed to each agent on its
`Pane`, and answers waits from the agents' events; `workspace/wires.rs` places the travelling dot, which `wires.rs`
paints. `workspace/spawn.rs` opens an agent beside the caller, making its worktree on the background executor through
`templum-git`'s new `Repo::add_worktree`. The main thread never touches a socket, and a call that waits never holds
back the next one on its connection (each connection already has its own reader and writer threads).

**Tech Stack:** Rust 2024 (toolchain 1.99), GPUI (gpui-fast 0.1.8) through gpui-kit 0.7.3: `cx.spawn` and
`background_executor().timer` for deadlines, the delayed Enter and the dot's frames (the app's pattern,
`crates/templum/src/workspace/mod.rs:1003-1011`); `InputState` and `InputEvent::PressEnter`
(`gpui-base-0.7.3/src/input/base/state.rs:127-132`, used as the find field does,
`crates/templum/src/workspace/find.rs:41-55`); `fill(..).corner_radii(..)` for a round dot
(`gpui-fast-0.1.8/src/window.rs:7829`, `:7881`; as `crates/templum/src/git_block/history.rs:442-448` paints graph
dots); `App::reduce_motion` (`gpui-fast-0.1.8/src/app.rs:1296`, and `request_animation_frame`'s note on decorative
motion, `src/window.rs:2741-2749`); `serde_json`; the user's `git` through `templum-git`.

**Spec:** [docs/specs/connections.md](../specs/connections.md): batch 3 of its Delivery: §Requests from agents,
§Delegating, §Spawning, the Wires bullet on the travelling dot, the Tools rows and the command-line rows marked 3,
§Architecture's paragraph on requests that wait, §Testing's items marked batch 3, and the Risks rows on long calls,
cancelled calls and typing into a busy agent.

## Global Constraints

- Only `crates/templum` depends on gpui-kit; `templum-mcp`, `templum-git` and `templum-terminal` stay UI-free and
  are tested headlessly. No new dependency ("`futures`, `serde`, `serde_json` and `uuid` are in the tree"). No
  `unsafe`. Every logic change ships a test that fails without it.
- Spec, verbatim: "A question (`ask_user`) and a request to connect (`connect`) open a **balloon** on the asking
  block's top right corner, over its terminal, at the Full level of detail: a question shows its text and a button
  per option, or a text field and **Send** when it has none; a request to connect says "*Claude Code · api* wants to
  connect to *Dev server*" with **Allow** and **Deny**; Allow wires them; several requests from one block wait in
  turn: the balloon shows the oldest, with "1 of 3"; when the block isn't on screen or is in another workspace, a
  notification says the agent is asking; clicking it reveals the block; below the Full level, the block's dot shows
  it waiting for the person; a request no one answers is withdrawn after its timeout (30 minutes unless the agent
  asked for another), and the agent is told so; closing the block withdraws its requests."
- "Templum types into Codex's terminal, as one pasted message, a first line `[from Claude Code · api (b7k2m)]` and
  the task, then Enter. The person sees it arrive." "When Codex finishes its turn, its last answer becomes the result
  of Claude Code's `delegate` (when it asked to wait), or of a later `wait_for_idle`. If Codex calls `reply` first,
  that text is the answer instead." "Templum refuses to type into an agent that is waiting for its person (a
  permission prompt would take the text as its answer), and into a terminal running no agent." "An agent Templum gets
  no events from answers only through `reply`, or the wait ends at its timeout with the agent's last screen lines."
- "`spawn_agent` opens a new agent block beside the caller, wired to it, in the caller's folder or the one it names.
  With a worktree name, Templum first runs `git worktree add -b <name> <repository>/.worktrees/<name>` from the
  caller's repository (and adds `.worktrees/` to the repository's `.git/info/exclude` once) … A prompt is typed once
  the agent is ready: when Claude Code reports its session started; for the others, when the agent's process is seen
  and its screen has been still for a second. The result names the new block and its folder."
- "While a task is handed over along a wire, a dot travels along it from the agent that handed it to the one
  working, until the answer comes back."
- Tools, as the spec's table: `connect` (`block`, `timeout`), `spawn_agent` (`agent`, `prompt`, `folder`,
  `worktree`), `delegate` (`block`, `task`, `wait` false, `timeout` 600 s; wired, an agent), `reply` (`block`, `text`;
  wired), `wait_for_idle` (`block`, `timeout` 300 s; wired), `ask_user` (`question`, `options`, `timeout` 1800 s).
  "The descriptions of the tools that wait say they may take minutes."
- Verbs: `templum connect <block>`, `templum spawn <agent> [--folder <f>] [--worktree <name>] [--prompt <text>]`,
  `templum delegate <block> <task> [--wait]`, `templum reply <block> <text>`, `templum wait <block> [--timeout <s>]`
  (`--for` is batch 5's), `templum ask <question> [--option <o>]…`.
- Now that `connect` exists, the refusal is "`k9x1d isn't connected to you. Ask the person with
  connect("k9x1d").`" and the instructions say "In this workspace, not connected (ask with connect): …".
- "Requests are answered on Templum's main thread …; a request that waits (a question, a delegation) never holds
  back the others." "The main thread never touches a socket."
- Not in this batch: kanban, note, `open_file` and `git_status` tools (batch 4); `wait_for`, wire labels,
  Settings ▸ Connections, Gemini CLI, `templum wire`, `--json` everywhere (batch 5).
- Commits on `development`, Conventional Commits, `feat`/`fix` titles written for a person using Templum; every
  commit message ends with the line `Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ`.
- Before saying done: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`; this batch paints (the balloon and the dot), so `--bench` and `--bench-mixed` run too (p95 ≤ 16.7 ms).

## Decisions

The spec leaves these open; the plan takes them as written here.

1. **`Workspace::waits`, not `Workspace::pending`:** `pending` already holds the settings changes for the next frame
   (`crates/templum/src/workspace/mod.rs:854`). The spec's Architecture paragraph is updated (Task 9).
2. **Deadlines are timers, not a per-frame check.** An idle window draws no frames, so a deadline checked each frame
   would never fire. Each wait starts a detached timer (`cx.spawn` + `background_executor().timer`) that answers it
   if it is still there; answered earlier, the timer finds nothing. A wait's block being closed *is* checked each
   frame (`drop_orphans` in `render`): that one check covers every way a block goes (closed, its shell exits, its
   workspace stopped), and a block moved to another workspace keeps its terminal, so its waits stay.
3. **What names what:** a wait names the terminal that asked (`TEMPLUM_TERMINAL`, kept by a block moved to another
   workspace); a request to connect names its target by block id; a turn's wait names the agent's terminal (hook
   events come by terminal).
4. **Every connection's end is reported** (`templum/closed`, with its number): the MCP bridge's when its agent closed
   it (agent quit), a verb's when the command exits, a hook's right after its event (nothing waits there). The app
   drops that connection's waits unanswered. For this the command line no longer stops writing after its one call
   (`cli.rs` `one_call`): it stays connected until its answer, so Ctrl+C on `templum ask` withdraws its balloon.
5. **`notifications/cancelled`** (MCP) drops the wait with that `requestId` on that connection, unanswered (MCP says
   the receiver should send no response).
6. **Timeouts:** whole seconds from 1 to 3600 (Codex gives up on a tool call after an hour: the
   `tool_timeout_sec=3600` Templum types for it); a larger one is clamped, as `lines` is. Defaults: `connect` and
   `ask_user` 1800, `delegate` 600, `wait_for_idle` 300. Replies say durations in seconds.
7. **The dot says it at every level:** a terminal whose agent asks the person shows the yellow dot "Waiting for you:
   *the question*" ("… connect to *b7k2m*" for a request), after a terminal's own alert, at every level (the balloon
   shows up close too). The notification comes when the asking block isn't shown: another workspace, outside the
   view, or hidden by grid mode. Not for a window in the background with the block on screen.
8. **`connect`** answers at once when there is nothing to ask: already wired (ok), its own block, an unknown block,
   or a request for that block already waiting (refusals). Allow wires them as a step of Ctrl+Z, like wiring by hand;
   Deny and the timeout leave nothing.
9. **Typing a task:** the header uses the caller's title as `whoami` gives it (cleaned) and its block id:
   `[from <title> (<id>)]`, a new line, the task. It is typed with `Terminal::type_in` (one bracketed paste when the
   agent asked for one, ESC dropped so the task can't end the paste early; otherwise one line), and Enter 150 ms
   later: agents may take keys that come in one burst for a paste, where Enter is a new line.
10. **One task at a time per agent:** a delegation to an agent already on another agent's task is refused (Codex runs
    typed input as its next turn: the first turn's end would answer the wrong task). A task to an agent that is
    working on its own prompt is allowed (spec): its answer is the end of the next turn that ends.
11. **A task is handed over** (`Pane::handed`) until its answer comes (Stop, Codex's notify, `reply`), its agent's
    session ends, its block closes, or its `timeout` passes (`delegate`'s, also without `wait`); then its dot stops
    and another task may be handed over. A wait that times out leaves the agent working: `wait_for_idle` waits again.
12. **`reply`:** the waits of the named block on the caller (`delegate` with `wait`, `wait_for_idle`) get the text.
    With none, the text is typed into that block as `[reply from <title> (<id>)]` + the text + Enter, but only into
    an agent not waiting for its person (never a shell, where it would run as a command). Either way the task that
    block handed the caller is answered, and the text is the caller's last answer.
13. **`wait_for_idle`** waits while the agent is on a task, working, waiting for its person, or before its spawn
    prompt is typed; otherwise it answers at once with the agent's last answer (or says it has none). An agent
    without events counts as idle unless it is on a task.
14. **Turn ends:** `Stopped` with a message answers with it; without one (an API error, an interrupt) with an error;
    `Ended` (the session) with an error. `Started` ends nothing (a spawned agent's first start must not end the task
    its prompt is). A turn's wait that times out gives the agent's last 40 screen lines, for every agent.
15. **`spawn_agent`:** the keyboard, the selection and the camera stay where they were; one Ctrl+Z closes the new
    block with its wire (the wire is part of the add). A caller in a hidden workspace gets the block there. `folder`
    is relative to the caller's folder and `~` is the home folder; the agent must be installed. The worktree comes
    from the repository of the folder the agent would start in, always under the main working tree's `.worktrees/`
    (also from a linked worktree), on a new branch from that folder's HEAD; names are letters, digits, `-`, `_`, `.`
    and `/` (git's own rules refuse the rest); an existing branch or folder is git's refusal.
16. **When a spawned agent is ready:** Claude Code (Templum's own `claude`) on its `SessionStart` only, which comes
    after its question about trusting the folder (a typed Enter would answer it); the others as the spec says: their
    process seen by a snapshot and two looks at their screen, a second apart, alike. A spawn's prompt is a task the
    caller handed over: the dot travels, and `wait_for_idle` waits for its answer.
17. **The dot:** 4 world units of radius, never under 2.5 px, in the accent; one trip from the agent that handed the
    task to the one working takes 1.2 s, then it starts again; drawn about 30 times a second while one is in view
    (a timer, not the display's rate); still at the wire's middle with reduced motion; only along a wire between the
    two (unwired, no dot, and the answer still comes back); none in grid mode.
18. **The balloon:** 300 world units wide, over the terminal's top right corner, clipped by its block; its buttons
    wrap; its clicks never reach the terminal; while its field has the keyboard, no key reaches the shell of the
    focused terminal, and the block stays at the Full level (as a block worked in does).
19. **Templum quitting with calls waiting:** nothing answers them; the bridges see their connection end and exit
    (batch 1's behavior), and the agents report the server gone.

## Review Focus

1. **An agent quits while its question is in the balloon** (its bridge's connection ends): the balloon goes, and
   nothing is left to answer. Task 1's `a_connection_that_ends_says_so_with_its_number` and Task 2's
   `an_agent_that_quits_takes_its_questions_away`.
2. **A deadline that fires as the answer comes** (a turn's end at the delegation's timeout, a click as a question
   expires): the agent gets exactly one answer. Task 2's `an_answer_and_its_timeout_answer_once` and Task 5's
   `a_turns_end_and_its_timeout_answer_once`.
3. **A second task handed to an agent already on one, from another agent:** refused, so no agent gets another's
   answer. Task 5's `delegate_refuses_what_it_mustnt_type_into`.
4. **Keys typed in the balloon's field while a terminal is focused** (Ctrl+L, Enter): none reaches the shell. Task 4's
   `keys_in_the_balloons_field_never_reach_the_terminal`.
5. **A task whose text holds new lines or the paste's end sequence:** it arrives as one paste, and only Templum's
   Enter sends it. Task 5's `a_task_cant_end_its_paste_early` (and `delegate_types_the_header_the_task_and_enter`
   with a two-line task).

## File Structure

| File | Change | Task |
|---|---|---|
| `crates/templum-mcp/src/bus.rs` | `Message::conn`, `CLOSED`, the end of every connection reported | 1 |
| `crates/templum-mcp/src/cli.rs` | a verb stays connected until its answer; `ask`, `connect`, `delegate`, `reply`, `wait`, `spawn` | 1, 2, 3, 5, 8 |
| `crates/templum-mcp/src/tools.rs` | the six tools: schemas, `Call` variants, parsing, timeouts | 2, 3, 5, 8 |
| `crates/templum/src/workspace/waits.rs` (new) | `Wait`, `Waiting`: keeping, answering, expiring, cancelling, orphans; the person's answers; the notification | 2, 3, 4, 5 |
| `crates/templum/src/workspace/bus.rs` | `Outcome`; `serve` and `call` hand waits over; `pane`/`pane_mut`; events answer turns; `notify_from`; refusal and instructions | 1, 2, 3, 5, 8 |
| `crates/templum/src/workspace/mod.rs` | `Workspace::waits`, `next_wait`, `_dot_frame`; `Handed`, `Prompt`; `Pane::handed`, `answer`, `prompt` | 2, 4, 5, 6, 8 |
| `crates/templum/src/block.rs` | the dot waits; the balloon at Full; a block typing in its balloon stays Full | 2, 4 |
| `crates/templum/src/balloon.rs` (new) | drawing the balloon | 4 |
| `crates/templum/src/git_block/widgets.rs` | `small` takes any label | 4 |
| `crates/templum/src/workspace/input.rs` | the balloon's field keeps its keys | 4 |
| `crates/templum/src/workspace/spaces.rs` | `in_space` | 3 |
| `crates/templum/src/workspace/delegate.rs` (new) | `delegate`, `reply`, `wait_for_idle`, typing a task, ending a turn | 5, 8 |
| `crates/templum/src/workspace/wires.rs`, `crates/templum/src/wires.rs`, `workspace/render.rs` | the dot: where, painting, frames | 6 |
| `crates/templum-git/src/repo.rs` | `Repo::add_worktree` | 7 |
| `crates/templum/src/workspace/spawn.rs` (new), `workspace/blocks.rs` | `spawn_agent`; `start_agent` split out of `launch_agent` | 8 |
| `crates/templum/src/workspace/tests/{requests,delegate,spawn}.rs` (new), `tests/bus.rs`, `tests/mod.rs` | headless tests and shared helpers | 2–8 |

---
### Task 1: The bus numbers its connections and reports each one's end

**Files:**
- Modify: `crates/templum-mcp/src/bus.rs:10` (imports), `:36-43` (`Message`), `:97-137` (`serve`), `:314` (the
  `use` inside `listen` goes: the module imports `AtomicU64` now)
- Modify: `crates/templum-mcp/src/cli.rs:163-182` (`one_call`), `:342-346` (the hook test's handler)
- Modify: `crates/templum/src/workspace/bus.rs:119-123` (`serve` destructures the new field)
- Modify: `crates/templum/src/workspace/tests/bus.rs:20-31` and `:446-456` (two `Message` literals)
- Test: inline in `crates/templum-mcp/src/bus.rs` and `crates/templum-mcp/src/cli.rs`

**Interfaces:**
- Produces: `templum_mcp::Message { pub hello: Hello, pub conn: u64, pub incoming: Incoming, pub reply: Replier }`;
  `templum_mcp::bus::CLOSED: &str = "templum/closed"`, sent as `Incoming::Notification { method: CLOSED, params:
  Value::Null }` with the connection's `conn` once its client is gone.

- [ ] **Step 1: Write the failing tests**

In `crates/templum-mcp/src/bus.rs`'s `mod tests`, after `a_client_done_writing_gets_its_answers_then_the_end`:

```rust
    /// A connection that ends tells the UI, with its number, whoever the client was: an agent's
    /// bridge closed by its agent, a command that exited.
    #[test]
    fn a_connection_that_ends_says_so_with_its_number() {
        let (seen, messages) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            let _ = seen.send((m.conn, m.hello.client, m.incoming));
        });
        let limit = Duration::from_secs(5);
        let (mut bridge, token) = connect(&bus.env).unwrap();
        let lines = hello_line("123-4", "mcp", token.as_deref()) + &rpc::request(1, "ping", Value::Null);
        bridge.write_all(lines.as_bytes()).unwrap();
        // The agent closes the bridge's input: done.
        bridge.finish().unwrap();
        let (conn, client, first) = messages.recv_timeout(limit).unwrap();
        assert_eq!(client, "mcp");
        assert!(matches!(first, Incoming::Request { .. }), "{first:?}");
        let (ended, _, closed) = messages.recv_timeout(limit).unwrap();
        assert_eq!(ended, conn);
        assert_eq!(
            closed,
            Incoming::Notification {
                method: CLOSED.into(),
                params: Value::Null
            }
        );
        // A command that exits: its socket goes.
        let (mut command, token) = connect(&bus.env).unwrap();
        command
            .write_all(hello_line("123-4", "cli", token.as_deref()).as_bytes())
            .unwrap();
        command
            .write_all(rpc::request(1, "ping", Value::Null).as_bytes())
            .unwrap();
        let (other, client, _) = messages.recv_timeout(limit).unwrap();
        assert_eq!(client, "cli");
        assert_ne!(other, conn, "each connection has its own number");
        drop(command);
        let (ended, _, closed) = messages.recv_timeout(limit).unwrap();
        assert_eq!(ended, other);
        assert!(
            matches!(&closed, Incoming::Notification { method, .. } if method == CLOSED),
            "{closed:?}"
        );
    }
```

In `crates/templum-mcp/src/cli.rs`'s `mod tests`, after `a_verb_is_one_tool_call_and_prints_its_text`:

```rust
    /// A command stays connected until its answer comes: Templum hears it end only then. One
    /// stopped with Ctrl+C ends it early, and what it waited for is withdrawn.
    #[test]
    fn a_command_stays_connected_until_its_answer() {
        let (seen, order) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| match &m.incoming {
            Incoming::Request { id, .. } => {
                let (id, reply, seen) = (id.clone(), m.reply.clone(), seen.clone());
                let _ = seen.send("asked");
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let _ = seen.send("answered");
                    let _ = reply.send(tool_result(&id, &Reply::ok("Yes")));
                });
            }
            Incoming::Notification { method, .. } if method == crate::bus::CLOSED => {
                let _ = seen.send("closed");
            }
            _ => {}
        });
        assert_eq!(run_with(&["whoami"], &env(&bus), "").1, "Yes\n");
        let order: Vec<&str> = (0..3).map(|_| order.recv_timeout(LIMIT).unwrap()).collect();
        assert_eq!(order, ["asked", "answered", "closed"]);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp -- connection_that_ends command_stays_connected`
Expected: compile errors: no field `conn` on `Message`, cannot find value `CLOSED`.

- [ ] **Step 3: Number the connections and report their end**

`crates/templum-mcp/src/bus.rs`, the imports (line 10):

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
```

and remove the now redundant `use std::sync::atomic::AtomicU64;` at the top of `listen` (line 314).

After `pub const HELLO`, add:

```rust
/// What the bus tells the UI when a connection ends (its client is gone: an agent closed its
/// bridge, a command exited): nothing it waits for has anyone to answer.
pub const CLOSED: &str = "templum/closed";
```

`Message` (lines 36-43) becomes:

```rust
/// A line from a client, for the UI thread.
pub struct Message {
    pub hello: Hello,
    /// Its connection's number: one per connection, never reused while Templum runs. A cancel
    /// names a request by it and the request's id.
    pub conn: u64,
    pub incoming: Incoming,
    /// Answers it. A connection closes once its client is done writing and every replier of
    /// it is dropped.
    pub reply: Replier,
}
```

In `serve` (lines 97-137): before `let mut reader`, add `static NEXT: AtomicU64 = AtomicU64::new(0);`; after the
writer thread is spawned, `let conn = NEXT.fetch_add(1, Ordering::Relaxed);`; the `Message` built in the loop gains
`conn,`; and after the loop:

```rust
    // Its client is gone: an agent closed its bridge, a command exited, a hook said its event.
    let _ = tx.unbounded_send(Message {
        hello,
        conn,
        incoming: Incoming::Notification {
            method: CLOSED.into(),
            params: serde_json::Value::Null,
        },
        reply,
    });
```

- [ ] **Step 4: The command line stays connected until its answer**

`crates/templum-mcp/src/cli.rs`, `one_call` (lines 163-182): its doc comment gains "It stays connected until the
answer: its end tells Templum it is gone (Ctrl+C), and what it waited for is withdrawn.", and the write no longer
finishes the stream:

```rust
    if stream.write_all(request.as_bytes()).is_err() {
        let _ = writeln!(stderr, "templum: Templum isn't running");
        return 2;
    }
```

The hook test's handler (`a_hook_sends_its_event_prints_nothing_and_never_fails`, lines 344-346) now hears each
hook connection's end too; it keeps only the events:

```rust
        let (_dir, bus) = test_bus(move |m| {
            // Each hook's connection ends after its event: that isn't an event.
            if !matches!(&m.incoming, Incoming::Notification { method, .. } if method == crate::bus::CLOSED)
            {
                let _ = seen.send((m.hello.client, m.incoming));
            }
        });
```

- [ ] **Step 5: The app reads the new field**

`crates/templum/src/workspace/bus.rs:119-123`:

```rust
        let Message {
            hello,
            incoming,
            reply,
            ..
        } = message;
```

`crates/templum/src/workspace/tests/bus.rs`: both `Message { … }` literals (in `ask`, lines 20-31, and in `event`,
lines 446-456) gain `conn: 0,` after `hello`.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p templum-mcp && cargo test -p templum bus::`
Expected: PASS (the new tests, and every bus, command-line and app bus test as before).

- [ ] **Step 7: Commit**

```bash
git add crates/templum-mcp/src/bus.rs crates/templum-mcp/src/cli.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/tests/bus.rs
git commit -m "refactor(connections): number the bus's connections and report each one's end" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

### Task 2: An agent's question waits for the person (`ask_user`), and everything that waits

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (constants, `Call::AskUser`, `list`, `parse`, tests)
- Modify: `crates/templum-mcp/src/cli.rs:12` (`VERBS` becomes a slice), `call_of` (`ask`), tests
- Create: `crates/templum/src/workspace/waits.rs`
- Modify: `crates/templum/src/workspace/mod.rs:1-32` (`mod waits;`), `:922` and `:1214` (fields)
- Modify: `crates/templum/src/workspace/bus.rs:118-157` (`serve`), `:196` and `:285` (`space_named`, `about` become
  `pub(super)`), `:200-281` (`call`), `:415` (`target` becomes `pub(super)`)
- Modify: `crates/templum/src/workspace/render.rs:310` (orphans each frame)
- Modify: `crates/templum/src/block.rs:806-843` (`status_dot`)
- Modify: `crates/templum/src/workspace/tests/bus.rs` (shared helpers; `tools/list`)
- Create: `crates/templum/src/workspace/tests/requests.rs`; register it in `crates/templum/src/workspace/tests/mod.rs`

**Interfaces:**
- Consumes: Task 1's `Message::conn` and `bus::CLOSED`.
- Produces (templum-mcp): `tools::MAX_WAIT: u64 = 3600`, `tools::ASK_WAIT: u64 = 1800`; `Call::AskUser { question:
  String, options: Vec<String>, timeout: u64 }`. In `parse`, closures later tasks reuse: `seconds(default: u64) ->
  Result<u64, String>` (the `timeout` argument). In `list`, `seconds(default: u64) -> Value` (its schema).
- Produces (app, `workspace/waits.rs`): `pub(crate) struct Wait { pub key: u64, pub conn: u64, pub id: Value, reply:
  Replier, pub from: String, pub what: Waiting }`; `pub(crate) enum Waiting { Question { text: String, options:
  Vec<String>, field: Option<(Entity<InputState>, Subscription)> } }`; `pub(super) struct Asked { conn, id, reply,
  from }`; `Wait::summary(&self) -> String`; on `Workspace`: `wait(&mut self, Asked, Waiting, seconds: u64, &mut
  Window, &mut Context<Self>)`, `answer(&mut self, key: u64, Reply, cx) -> bool`, `cancel(conn, &Value, cx)`,
  `closed(conn, cx)`, `drop_orphans(cx)`, `pub(crate) answer_question(key, String, window, cx)`, `pub(crate)
  send_field(key, window, cx)`, `pub(crate) balloon(&self, terminal: &str) -> Option<(&Wait, usize)>`. Private:
  `expire(key, seconds, cx)`, `orphaned(&self, &Wait) -> Option<Reply>`, `tell_asking(&self, &Wait, window, cx)`,
  `field_focused(&Wait, &Window, &App) -> bool`.
- Produces (app, `workspace/bus.rs`): `pub(super) enum Outcome { Now(Reply), Wait(Waiting, u64) }`;
  `call(&mut self, terminal, Call, window, cx) -> Outcome`; `pub(super) fn notify_from(&self, space: String, me:
  BlockId, title: String, text: String, window, cx)`; `about`, `space_named`, `target` are `pub(super)`.
- Produces (tests, `tests/bus.rs`, all `pub(super)`): `call`, `me`, `event`, `screen_with` (Unix); `call_later(handle,
  ws, cx, terminal, conn, tool, arguments) -> Receiver<String>` (its request's id is `conn`), `result_of(&str) ->
  (bool, String)`, `bus_note(handle, ws, cx, conn, method, params)`, `answered(&Receiver<String>) -> (bool,
  String)` (the answer already there), `after(cx, millis)` (the test's clock moves on, and what waited runs).

- [ ] **Step 1: Write the failing tool tests**

In `crates/templum-mcp/src/tools.rs`'s tests, `every_tool_listed_parses_its_example`'s `examples` gains, after
`notify`'s:

```rust
            (
                "ask_user",
                json!({ "question": "Ship it?", "options": ["Yes", "No"], "timeout": 60 }),
            ),
```

and a new test after it:

```rust
    #[test]
    fn the_tools_that_wait_have_their_defaults_and_limits() {
        assert_eq!(
            parse("ask_user", &json!({ "question": "Why?" })),
            Ok(Call::AskUser {
                question: "Why?".into(),
                options: Vec::new(),
                timeout: ASK_WAIT
            })
        );
        assert_eq!(
            parse("ask_user", &json!({ "question": "Why?", "timeout": 99_999 })),
            Ok(Call::AskUser {
                question: "Why?".into(),
                options: Vec::new(),
                timeout: MAX_WAIT
            })
        );
        for bad in [
            json!({}),
            json!({ "question": "Why?", "options": "yes" }),
            json!({ "question": "Why?", "options": [1] }),
            json!({ "question": "Why?", "timeout": 0 }),
        ] {
            assert!(parse("ask_user", &bad).is_err(), "{bad}");
        }
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp tools::`
Expected: compile error: no variant `AskUser`, cannot find `ASK_WAIT`.

- [ ] **Step 3: The tool**

`crates/templum-mcp/src/tools.rs`, after `MAX_TEXT`:

```rust
/// The longest a tool waits, seconds: Codex gives up on a call after an hour (the
/// `tool_timeout_sec` of `codex_flags`).
pub const MAX_WAIT: u64 = 3600;
/// How long a question or a request to connect waits for the person, seconds, unless asked.
pub const ASK_WAIT: u64 = 1800;
```

`Call` gains, after `Notify`:

```rust
    AskUser {
        question: String,
        options: Vec<String>,
        timeout: u64,
    },
```

In `list`, after `let none = …`:

```rust
    let seconds = |default: u64| {
        json!({
            "type": "integer",
            "minimum": 1,
            "maximum": MAX_WAIT,
            "default": default,
            "description": "Seconds to wait before giving up.",
        })
    };
```

and after `notify`'s entry:

```rust
        {
            "name": "ask_user",
            "description": "Asks the person a question in a balloon on your block: a button per option, or a \
                            text field when there are none. Waits for the answer and returns it; it may take \
                            minutes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "question": { "type": "string" },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "The answers to offer; none for a text field.",
                    },
                    "timeout": seconds(ASK_WAIT),
                },
                "required": ["question"],
            },
        },
```

In `parse`, after `flag`:

```rust
    let seconds = |default: u64| match &args["timeout"] {
        Value::Null => Ok(default),
        value => value
            .as_u64()
            .filter(|n| *n >= 1)
            .map(|n| n.min(MAX_WAIT))
            .ok_or_else(|| format!("\"timeout\" is a number of seconds from 1 to {MAX_WAIT}")),
    };
```

and, after `notify`'s arm:

```rust
        "ask_user" => Call::AskUser {
            question: text("question")?,
            options: match &args["options"] {
                Value::Null => Vec::new(),
                Value::Array(items) => items
                    .iter()
                    .map(|o| o.as_str().map(str::to_string))
                    .collect::<Option<_>>()
                    .ok_or_else(|| "\"options\" is a list of strings".to_string())?,
                _ => return Err("\"options\" is a list of strings".into()),
            },
            timeout: seconds(ASK_WAIT)?,
        },
```

Run: `cargo test -p templum-mcp tools::` → PASS.

- [ ] **Step 4: Write the failing verb test**

In `crates/templum-mcp/src/cli.rs`'s tests:

```rust
    #[test]
    fn ask_sends_the_question_and_its_options() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let _ = m.reply.send(tool_result(id, &Reply::ok("Yes")));
            }
        });
        let args = ["ask", "Ship", "it?", "--option", "Yes", "--option", "Not yet"];
        let (code, out, _) = run_with(&args, &env(&bus), "");
        assert_eq!((code, out.as_str()), (0, "Yes\n"));
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap(),
            json!({ "name": "ask_user", "arguments": { "question": "Ship it?", "options": ["Yes", "Not yet"] } })
        );
        run_with(&["ask", "When?"], &env(&bus), "");
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap()["arguments"],
            json!({ "question": "When?" })
        );
        let (code, _, err) = run_with(&["ask", "--option", "Yes"], &env(&bus), "");
        assert_eq!(code, 2);
        assert!(err.starts_with("usage: templum ask"), "{err}");
    }
```

and `only_the_verbs_are_commands` gains `assert!(is_verb("ask"));`.

Run: `cargo test -p templum-mcp cli::` → FAIL (`ask` is no verb: `run_with` hits the `notify` usage).

- [ ] **Step 5: The verb**

`crates/templum-mcp/src/cli.rs:12-13`:

```rust
/// The first arguments that make `templum` a command instead of the app.
const VERBS: &[&str] = &["mcp", "hook", "whoami", "blocks", "read", "send", "notify", "ask"];
const READ: &str = "read <block> [-n <lines>] [--last] [--json]";
const ASK: &str = "ask <question> [--option <answer>]… [--json]";
```

In `call_of`, after the `("notify", text) if !text.is_empty()` arm:

```rust
        ("ask", words) => {
            let (mut question, mut options) = (Vec::new(), Vec::new());
            let mut words = words.iter();
            while let Some(word) = words.next() {
                if *word == "--option" {
                    options.push(*words.next().ok_or(ASK)?);
                } else {
                    question.push(*word);
                }
            }
            if question.is_empty() {
                return Err(ASK);
            }
            let mut arguments = json!({ "question": question.join(" ") });
            if !options.is_empty() {
                arguments["options"] = json!(options);
            }
            ("ask_user", arguments)
        }
```

Run: `cargo test -p templum-mcp` → PASS.

- [ ] **Step 6: Write the failing app tests**

`crates/templum/src/workspace/tests/bus.rs`: `call`, `me`, `event` and (Unix) `screen_with` become `pub(super) fn`;
`initialize_lists_the_tools_and_says_who_the_agent_is`'s count becomes the whole list:

```rust
    assert_eq!(tools["result"], templum_mcp::tools::list());
```

and, after `call`, the helpers for calls that wait:

```rust
/// Calls `tool` from `terminal` on connection `conn` (its request's id is `conn` too): the channel
/// its answer comes on, now or later.
pub(super) fn call_later(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    terminal: &str,
    conn: u64,
    tool: &str,
    arguments: Value,
) -> std::sync::mpsc::Receiver<String> {
    let (reply, answers) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: terminal.into(),
            client: "mcp".into(),
        },
        conn,
        incoming: Incoming::Request {
            id: json!(conn),
            method: "tools/call".into(),
            params: json!({ "name": tool, "arguments": arguments }),
        },
        reply,
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.serve(message, window, cx))
    })
    .unwrap();
    answers
}

/// A tool's answer line: whether it is an error, and its text.
pub(super) fn result_of(line: &str) -> (bool, String) {
    let answer: Value = serde_json::from_str(line).unwrap();
    let result = &answer["result"];
    (
        result["isError"] == true,
        result["content"][0]["text"].as_str().unwrap().to_string(),
    )
}

/// A notification `method` on connection `conn`, as an MCP client or the bus sends it.
pub(super) fn bus_note(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    conn: u64,
    method: &str,
    params: Value,
) {
    let (reply, _) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: String::new(),
            client: "mcp".into(),
        },
        conn,
        incoming: Incoming::Notification {
            method: method.into(),
            params,
        },
        reply,
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.serve(message, window, cx))
    })
    .unwrap();
}

/// The answer that came on `answers`: whether it is an error, and its text.
pub(super) fn answered(answers: &std::sync::mpsc::Receiver<String>) -> (bool, String) {
    result_of(&answers.try_recv().expect("answered"))
}

/// Lets `millis` pass on the test's clock, and what waited on them run.
pub(super) fn after(cx: &mut TestAppContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
}
```

Register the new file in `crates/templum/src/workspace/tests/mod.rs`, after `mod persistence;`: `mod requests;`.

Create `crates/templum/src/workspace/tests/requests.rs`:

```rust
//! Requests that wait for the person: an agent's question (`ask_user`), its request to connect,
//! and the balloon they open (docs/specs/connections.md, Requests from agents).

use std::sync::mpsc::TryRecvError;

use serde_json::{Value, json};

use super::bus::{after, answered, bus_note, call_later, me};
use super::*;

/// The newest wait's key.
fn last_wait(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> u64 {
    ws.read_with(cx, |ws, _| ws.waits.last().expect("a wait").key)
}

/// What the dot before terminal `id`'s title says.
fn dot_text(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> Option<String> {
    ws.read_with(cx, |ws, _| {
        crate::block::status_dot(ws, ws.space.terminal(id).unwrap()).map(|(_, text)| text.to_string())
    })
}

/// The person answers the question `key` with `text`.
fn reply_with(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    key: u64,
    text: &str,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.answer_question(key, text.to_string(), window, cx))
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_balloon_shows_the_oldest_request_and_counts_them(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, _) = me(&ws, cx, first_id(&ws, cx));
    for (conn, question) in [(1, "One?"), (2, "Two?"), (3, "Three?")] {
        let asked = json!({ "question": question, "options": ["Yes"] });
        call_later(handle, &ws, cx, &terminal, conn, "ask_user", asked);
    }
    let shown = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.balloon(&terminal).map(|(wait, count)| (wait.summary(), count))
        })
    };
    assert_eq!(shown(cx), Some(("One?".to_string(), 3)));
    // The one shown is withdrawn: the next comes, counted again.
    bus_note(handle, &ws, cx, 1, "notifications/cancelled", json!({ "requestId": 1 }));
    assert_eq!(shown(cx), Some(("Two?".to_string(), 2)));
}

#[gpui_kit::test]
fn ask_user_waits_for_the_persons_answer(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "Ship it?\nThe tests pass.", "options": ["Yes", "No"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    assert!(answers.try_recv().is_err(), "it waits for the person");
    assert_eq!(
        dot_text(&ws, cx, first).as_deref(),
        Some("Waiting for you: Ship it?")
    );
    reply_with(handle, &ws, cx, last_wait(&ws, cx), "Yes");
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
    assert_eq!(dot_text(&ws, cx, first), None);
}

#[gpui_kit::test]
fn a_question_without_options_takes_what_the_person_types(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, _) = me(&ws, cx, first_id(&ws, cx));
    let asked = json!({ "question": "When?" });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let key = last_wait(&ws, cx);
    let send = |text: &'static str, cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let input = match &ws.waits[0].what {
                    crate::workspace::waits::Waiting::Question {
                        field: Some((input, _)),
                        ..
                    } => input.clone(),
                    _ => panic!("a question without options has a field"),
                };
                input.update(cx, |input, cx| input.set_value(text, window, cx));
                ws.send_field(key, window, cx);
            })
        })
        .unwrap();
    };
    send("   ", cx);
    assert!(answers.try_recv().is_err(), "a blank answer isn't sent");
    send("Friday", cx);
    assert_eq!(answered(&answers), (false, "Friday".to_string()));
}

#[gpui_kit::test]
fn a_question_no_one_answers_is_withdrawn_at_its_timeout(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, _) = me(&ws, cx, first_id(&ws, cx));
    let asked = json!({ "question": "Still there?", "options": ["Yes"], "timeout": 1 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    after(cx, 900);
    assert!(answers.try_recv().is_err(), "not yet");
    after(cx, 200);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("withdrawn"), "{text}");
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
}

#[gpui_kit::test]
fn an_answer_and_its_timeout_answer_once(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, _) = me(&ws, cx, first_id(&ws, cx));
    let asked = json!({ "question": "Now?", "options": ["Yes"], "timeout": 1 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    reply_with(handle, &ws, cx, last_wait(&ws, cx), "Yes");
    after(cx, 2000);
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
    assert!(answers.try_recv().is_err(), "answered once");
}

#[gpui_kit::test]
fn a_cancelled_call_drops_its_question_unanswered(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, _) = me(&ws, cx, first_id(&ws, cx));
    let question = json!({ "question": "Ship it?", "options": ["Yes"] });
    let first = call_later(handle, &ws, cx, &terminal, 3, "ask_user", question.clone());
    let _second = call_later(handle, &ws, cx, &terminal, 4, "ask_user", question);
    // Request 3 of another connection is another call.
    bus_note(handle, &ws, cx, 4, "notifications/cancelled", json!({ "requestId": 3 }));
    ws.read_with(cx, |ws, _| assert_eq!(ws.waits.len(), 2));
    let cancel = json!({ "requestId": 3, "reason": "The person pressed Esc" });
    bus_note(handle, &ws, cx, 3, "notifications/cancelled", cancel);
    ws.read_with(cx, |ws, _| assert_eq!(ws.waits.len(), 1));
    assert_eq!(
        first.try_recv(),
        Err(TryRecvError::Disconnected),
        "MCP sends no answer to a cancelled call"
    );
}

#[gpui_kit::test]
fn an_agent_that_quits_takes_its_questions_away(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let ask = |text: &str| json!({ "question": text, "options": ["Yes"] });
    let _a = call_later(handle, &ws, cx, &terminal, 5, "ask_user", ask("One?"));
    let _b = call_later(handle, &ws, cx, &terminal, 5, "ask_user", ask("Two?"));
    let _c = call_later(handle, &ws, cx, &terminal, 6, "ask_user", ask("Three?"));
    bus_note(handle, &ws, cx, 5, templum_mcp::bus::CLOSED, Value::Null);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.waits.len(), 1);
        assert_eq!(ws.waits[0].conn, 6);
    });
    assert_eq!(
        dot_text(&ws, cx, first).as_deref(),
        Some("Waiting for you: Three?")
    );
}

#[gpui_kit::test]
fn closing_the_asking_block_withdraws_its_question(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(first, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("closed"), "{text}");
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
}

#[gpui_kit::test]
fn a_question_off_screen_is_a_notification(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let before = notification_count(handle, cx);
    let asked = json!({ "question": "Here?", "options": ["Yes"] });
    let _here = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before, "the balloon is in view");
    // The asking block far outside the view.
    ws.update(cx, |ws, _| {
        ws.space.canvas.get_mut(mine).unwrap().rect.x += 100_000.0;
    });
    let asked = json!({ "question": "There?", "options": ["Yes"] });
    let _there = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before + 1);
}
```

- [ ] **Step 7: Run them to see them fail**

Run: `cargo test -p templum requests::`
Expected: compile errors: no field `waits` on `Workspace`, no method `answer_question`, `send_field`.

- [ ] **Step 8: The waits**

Create `crates/templum/src/workspace/waits.rs`:

```rust
//! Tool calls that wait (docs/specs/connections.md, Architecture): an agent's question for the
//! person. Each keeps its reply channel in `Workspace::waits` until it is answered, its deadline
//! passes, its call is cancelled or its connection ends, or its block closes.

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, Entity, Focusable as _, Subscription, Window};
use serde_json::Value;
use templum_mcp::{Replier, Reply, tools};

use super::Workspace;

/// A tool call waiting for its answer.
pub(crate) struct Wait {
    /// Names it for its deadline and the balloon's buttons; counts up, so the oldest is first.
    pub key: u64,
    /// The connection it came on, and its JSON-RPC id: a cancel names both.
    pub conn: u64,
    pub id: Value,
    reply: Replier,
    /// The terminal that asked (its `TEMPLUM_TERMINAL`): its block keeps it when it moves to
    /// another workspace.
    pub from: String,
    pub what: Waiting,
}

/// What a wait waits for.
pub(crate) enum Waiting {
    /// `ask_user`: one of `options`, or what the person types in `field` when there are none.
    Question {
        text: String,
        options: Vec<String>,
        field: Option<(Entity<InputState>, Subscription)>,
    },
}

/// A tool call that waits, as `serve` hands it over: where its answer goes.
pub(super) struct Asked {
    pub conn: u64,
    pub id: Value,
    pub reply: Replier,
    pub from: String,
}

impl Wait {
    /// What it asks the person, on one line: the dot's tooltip.
    pub fn summary(&self) -> String {
        match &self.what {
            Waiting::Question { text, .. } => text.lines().next().unwrap_or_default().to_string(),
        }
    }
}

impl Workspace {
    /// Keeps `asked` until what it waits for comes, or `seconds` pass; a question without options
    /// gets its field. The person hears of a request when its block isn't on screen.
    pub(super) fn wait(
        &mut self,
        asked: Asked,
        mut what: Waiting,
        seconds: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.next_wait += 1;
        let key = self.next_wait;
        if let Waiting::Question { options, field, .. } = &mut what
            && options.is_empty()
        {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Your answer"));
            let events = cx.subscribe_in(&input, window, move |ws, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    ws.send_field(key, window, cx);
                }
            });
            *field = Some((input, events));
        }
        // ponytail: one timer per wait, left to run out once it is answered (it then finds
        // nothing); a call waits an hour at most.
        cx.spawn(async move |ws, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(seconds))
                .await;
            ws.update(cx, |ws, cx| ws.expire(key, seconds, cx)).ok();
        })
        .detach();
        let wait = Wait {
            key,
            conn: asked.conn,
            id: asked.id,
            reply: asked.reply,
            from: asked.from,
            what,
        };
        self.tell_asking(&wait, window, cx);
        self.waits.push(wait);
        cx.notify();
    }

    /// Answers the wait `key`, once: it is gone after. False when it already was.
    pub(super) fn answer(&mut self, key: u64, reply: Reply, cx: &mut Context<Self>) -> bool {
        let Some(i) = self.waits.iter().position(|w| w.key == key) else {
            return false;
        };
        let wait = self.waits.remove(i);
        let _ = wait.reply.send(tools::tool_result(&wait.id, &reply));
        cx.notify();
        true
    }

    /// The deadline of the wait `key`: withdrawn, and the agent told so.
    fn expire(&mut self, key: u64, seconds: u64, cx: &mut Context<Self>) {
        let Some(wait) = self.waits.iter().find(|w| w.key == key) else {
            return;
        };
        let reply = match &wait.what {
            Waiting::Question { .. } => Reply::err(format!(
                "No one answered within {seconds} seconds; the request was withdrawn."
            )),
        };
        self.answer(key, reply, cx);
    }

    /// MCP's `notifications/cancelled` for the call `id` of connection `conn`: dropped, with no
    /// answer (the client wants none).
    pub(super) fn cancel(&mut self, conn: u64, id: &Value, cx: &mut Context<Self>) {
        self.waits.retain(|w| w.conn != conn || w.id != *id);
        cx.notify();
    }

    /// The connection `conn` ended (its agent quit, its command exited): what it waited for has
    /// no one to answer.
    pub(super) fn closed(&mut self, conn: u64, cx: &mut Context<Self>) {
        self.waits.retain(|w| w.conn != conn);
        cx.notify();
    }

    /// Each frame: a wait whose block is gone (closed, its shell exited, its workspace stopped)
    /// is withdrawn. A block moved to another workspace keeps its terminal, and its waits.
    pub(super) fn drop_orphans(&mut self, cx: &mut Context<Self>) {
        let orphans: Vec<(u64, Reply)> = self
            .waits
            .iter()
            .filter_map(|w| Some((w.key, self.orphaned(w)?)))
            .collect();
        for (key, reply) in orphans {
            self.answer(key, reply, cx);
        }
    }

    /// Why `wait` can't be answered any more, if so.
    fn orphaned(&self, wait: &Wait) -> Option<Reply> {
        if self.caller(&wait.from).is_none() {
            return Some(Reply::err("Your block was closed; the request was withdrawn."));
        }
        match &wait.what {
            Waiting::Question { .. } => None,
        }
    }

    /// The person's answer to the question `key`: an option, or what they typed. The keyboard
    /// goes back to the canvas when the question's field had it.
    pub(crate) fn answer_question(
        &mut self,
        key: u64,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typing = self
            .waits
            .iter()
            .any(|w| w.key == key && field_focused(w, window, cx));
        if self.answer(key, Reply::ok(text), cx) && typing {
            window.focus(&self.focus_handle, cx);
        }
    }

    /// Send, or Enter in a question's field: its text answers, unless it is blank.
    pub(crate) fn send_field(&mut self, key: u64, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self
            .waits
            .iter()
            .find(|w| w.key == key)
            .and_then(|w| match &w.what {
                Waiting::Question {
                    field: Some((input, _)),
                    ..
                } => Some(input.read(cx).value().to_string()),
                _ => None,
            });
        if let Some(text) = typed.filter(|t| !t.trim().is_empty()) {
            self.answer_question(key, text, window, cx);
        }
    }

    /// What the agent in terminal `terminal` asks the person, oldest first: the first, and how
    /// many.
    pub(crate) fn balloon(&self, terminal: &str) -> Option<(&Wait, usize)> {
        let mut asking = self.waits.iter().filter(|w| w.from == terminal);
        let first = asking.next()?;
        Some((first, 1 + asking.count()))
    }

    /// The person isn't looking at the asking block (it is in another workspace, outside the
    /// view, or hidden by grid mode): a notification says what it asks; clicking it shows the
    /// block.
    fn tell_asking(&self, wait: &Wait, window: &mut Window, cx: &mut Context<Self>) {
        let Some((space_id, me)) = self.caller(&wait.from) else {
            return;
        };
        let shown = space_id == self.space.id
            && self.space.shown(me)
            && self
                .space
                .canvas
                .visible_from(&self.space.view, self.viewport(window))
                .any(|b| b.id == me);
        if shown {
            return;
        }
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let (_, title, _) = self.about(&space.blocks[&me], cx);
        let text = match &wait.what {
            Waiting::Question { text, .. } => format!("Asks you: {text}"),
        };
        self.notify_from(space_id, me, title, text, window, cx);
    }
}

/// The question's field has the keyboard.
fn field_focused(wait: &Wait, window: &Window, cx: &App) -> bool {
    matches!(
        &wait.what,
        Waiting::Question { field: Some((input, _)), .. }
            if input.read(cx).focus_handle(cx).is_focused(window)
    )
}
```

`crates/templum/src/workspace/mod.rs`: `mod waits;` after `mod view;`. `Workspace` gains, after `_bus: Task<()>,`
(line 922):

```rust
    /// The tool calls waiting for an answer, oldest first (`waits.rs`).
    waits: Vec<waits::Wait>,
    /// The last wait's key.
    next_wait: u64,
```

and the constructor, after `_bus: bus_task,` (line 1214): `waits: Vec::new(), next_wait: 0,`.

- [ ] **Step 9: `serve` and `call` hand waits over**

`crates/templum/src/workspace/bus.rs`: the imports gain `use super::waits::{Asked, Waiting};`; `space_named` (line
196), `about` (line 285) and `target` (line 415) become `pub(super)`. After `start`, add:

```rust
/// What a tool call comes to.
pub(super) enum Outcome {
    /// Its answer, now.
    Now(Reply),
    /// It waits for this, at most these many seconds (`waits.rs`).
    Wait(Waiting, u64),
}
```

`serve` (lines 118-157) becomes:

```rust
    /// Answers one message from the bus. A tool call that waits is kept (`waits.rs`) and answered
    /// later; meanwhile the connection's next lines are served.
    pub(super) fn serve(&mut self, message: Message, window: &mut Window, cx: &mut Context<Self>) {
        let Message {
            hello,
            conn,
            incoming,
            reply,
        } = message;
        match incoming {
            Incoming::Request { id, method, params } => {
                let answer = match method.as_str() {
                    "initialize" => {
                        let instructions = self.instructions(&hello.terminal, cx);
                        rpc::result(&id, tools::initialize_result(&params, &instructions))
                    }
                    "ping" => rpc::result(&id, json!({})),
                    "tools/list" => rpc::result(&id, tools::list()),
                    "tools/call" => {
                        let name = params["name"].as_str().unwrap_or_default();
                        if !tools::known(name) {
                            rpc::error(&id, rpc::PARAMS, &format!("Unknown tool: {name}"))
                        } else {
                            let outcome = match tools::parse(name, &params["arguments"]) {
                                Ok(call) => self.call(&hello.terminal, call, window, cx),
                                Err(why) => Outcome::Now(Reply::err(why)),
                            };
                            match outcome {
                                Outcome::Now(done) => tools::tool_result(&id, &done),
                                Outcome::Wait(what, seconds) => {
                                    let asked = Asked {
                                        conn,
                                        id,
                                        reply,
                                        from: hello.terminal,
                                    };
                                    self.wait(asked, what, seconds, window, cx);
                                    return;
                                }
                            }
                        }
                    }
                    _ => rpc::error(&id, rpc::METHOD, &format!("Method not found: {method}")),
                };
                let _ = reply.send(answer);
            }
            Incoming::Notification { method, params } if method == templum_mcp::hooks::METHOD => {
                if let Ok(event) = serde_json::from_value(params) {
                    self.agent_event(&hello.terminal, event, cx);
                }
            }
            // The client gave up on a call (MCP): no answer goes to it.
            Incoming::Notification { method, params } if method == "notifications/cancelled" => {
                self.cancel(conn, &params["requestId"], cx);
            }
            Incoming::Notification { method, .. } if method == templum_mcp::bus::CLOSED => {
                self.closed(conn, cx);
            }
            // `notifications/initialized` and the rest need nothing.
            Incoming::Notification { .. } | Incoming::Response => {}
        }
    }
```

`call` (lines 200-281) becomes, every arm of batch 1 as it was but answering `Outcome::Now`:

```rust
    /// A tool call from the terminal `terminal`: its answer now, or what it waits for.
    fn call(
        &mut self,
        terminal: &str,
        call: Call,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let Some((space_id, me)) = self.caller(terminal) else {
            return Outcome::Now(Reply::err("This terminal isn't a block of this Templum."));
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        Outcome::Now(match call {
            Call::Whoami => {
                let (kind, title, folder) = self.about(&space.blocks[&me], cx);
                let mut text = format!(
                    "id: {}\nkind: {kind}\ntitle: {title}\n",
                    space.uids.get(&me).map_or("", String::as_str)
                );
                if let Some(folder) = folder {
                    text.push_str(&format!("folder: {}\n", folder.display()));
                }
                text.push_str(&format!("workspace: {}", space.name));
                Reply::ok(text)
            }
            Call::ListBlocks => Reply::ok(self.block_lines(space, me, cx).join("\n")),
            Call::ReadTerminal {
                block,
                lines,
                last_command,
            } => {
                let id = match target(space, me, &block) {
                    Ok(id) => id,
                    Err(reply) => return Outcome::Now(reply),
                };
                let Some(pane) = space.terminal(id) else {
                    return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
                };
                let shared = pane.term.shared();
                if last_command {
                    match shared.last_command() {
                        Some(text) => Reply::ok(tools::tail(&text, tools::MAX_TEXT)),
                        None => Reply::err(format!(
                            "No command has finished in {block} yet (it needs the shell's integration)."
                        )),
                    }
                } else {
                    Reply::ok(tools::tail(
                        &shared.lines(lines).join("\n"),
                        tools::MAX_TEXT,
                    ))
                }
            }
            Call::SendToTerminal { block, text, enter } => {
                let id = match target(space, me, &block) {
                    Ok(id) => id,
                    Err(reply) => return Outcome::Now(reply),
                };
                let Some(pane) = space.terminal(id) else {
                    return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
                };
                pane.term.type_in(&text);
                if enter {
                    pane.term.write(&b"\r"[..]);
                }
                Reply::ok(format!("Typed into {block}."))
            }
            Call::Notify { text } => {
                let (_, title, _) = self.about(&space.blocks[&me], cx);
                self.notify_from(space_id, me, title, text, window, cx);
                Reply::ok("Shown.")
            }
            Call::AskUser {
                question,
                options,
                timeout,
            } => {
                let what = Waiting::Question {
                    text: question,
                    options,
                    field: None,
                };
                return Outcome::Wait(what, timeout);
            }
        })
    }

    /// A notification from block `me` of workspace `space`, titled `title`; clicking it shows the
    /// block. In the background it is a system notification too.
    pub(super) fn notify_from(
        &self,
        space: String,
        me: BlockId,
        title: String,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity().downgrade();
        let notification = Notification::new()
            .title(title)
            .message(text)
            .delivery(self.delivery(window))
            .on_click(move |_, window, cx| {
                ws.update(cx, |ws, cx| ws.reveal(&space, me, window, cx))
                    .ok();
            });
        show_notification(window, cx, notification);
    }
```

- [ ] **Step 10: Orphans each frame, and the dot**

`crates/templum/src/workspace/render.rs`, after `self.hear(window, cx);` (line 310):

```rust
        // A tool call whose block is gone is withdrawn (docs/specs/connections.md).
        self.drop_orphans(cx);
```

`crates/templum/src/block.rs`, `status_dot` (lines 806-843) becomes:

```rust
/// The dot before a terminal's title, and what it says. News first: yellow when it asked for
/// attention, red or green when a long command ended. Then a question its agent asks the person
/// (yellow; the balloon shows it up close). Then an agent's turn (its hooks) before the shell's:
/// yellow while it waits for the person, the accent while it works. Then a command running, then
/// a last command that failed. Nothing when the shell doesn't say (no shell integration).
pub(crate) fn status_dot(ws: &Workspace, pane: &Pane) -> Option<(Hsla, SharedString)> {
    let ansi = |i: usize| hsla(ws.theme.palette.ansi[i], 1.0);
    let (color, text) = match &pane.alert {
        Some(a @ Alert::Attention(_)) => (ansi(3), a.text()),
        Some(a) if a.failed() => (ansi(1), a.text()),
        Some(a) => (ansi(2), a.text()),
        None => match ws.balloon(pane.term.id()) {
            Some((wait, _)) => (ansi(3), format!("Waiting for you: {}", wait.summary())),
            // An agent the last snapshot no longer sees (it crashed) says nothing of its turn.
            None => match pane.state.as_ref().filter(|_| pane.seen.agent.is_some()) {
                Some(AgentState::NeedsYou(asks)) => (
                    ansi(3),
                    asks.as_ref().map_or_else(
                        || "Waiting for you".to_string(),
                        |a| format!("Waiting for you: {a}"),
                    ),
                ),
                Some(AgentState::Working) => (ws.ui.accent, "Working".to_string()),
                Some(AgentState::Idle) => return None,
                None => {
                    let status = pane.term.status();
                    match (status.running, status.last) {
                        (Some(since), _) => (
                            ws.ui.accent,
                            format!("Running for {}", alert::took(since.elapsed())),
                        ),
                        (None, Some(last)) if last.code.is_some_and(|c| c != 0) => {
                            (ansi(1), alert::finished(&last))
                        }
                        _ => return None,
                    }
                }
            },
        },
    };
    Some((color, text.into()))
}
```

- [ ] **Step 11: Run the tests**

Run: `cargo test -p templum -- requests:: bus::`
Expected: PASS (the eight new tests, and the bus tests as before).

- [ ] **Step 12: Commit**

```bash
git add crates/templum-mcp/src crates/templum/src/workspace crates/templum/src/block.rs
git commit -m "feat(connections): let an agent ask you a question and wait for your answer" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

### Task 3: An agent asks to be wired (`connect`)

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (`Call::Connect`, `list`, `parse`, tests)
- Modify: `crates/templum-mcp/src/cli.rs` (`connect`, test)
- Modify: `crates/templum/src/workspace/waits.rs` (`Waiting::Connect`; `ask_to_connect`, `answer_connect`,
  `connect_line`; the arms of `summary`, `expire`, `orphaned`, `tell_asking`)
- Modify: `crates/templum/src/workspace/spaces.rs:360-368` (`in_space`, next to `with_space`)
- Modify: `crates/templum/src/workspace/blocks.rs:78` (`uid_taken` becomes `pub(super)`)
- Modify: `crates/templum/src/workspace/bus.rs` (`call`'s arm; `instructions` at `:369-377`; `target`'s refusal at
  `:421-426`)
- Modify: `crates/templum/src/workspace/tests/bus.rs:195` and `:279-283` (the refusal and the instructions)
- Test: `crates/templum/src/workspace/tests/requests.rs`

**Interfaces:**
- Consumes: Task 2's `Outcome`, `Wait`, `Waiting`, `answer`, `seconds` closures, `call_later`, `result_of`.
- Produces: `Call::Connect { block: String, timeout: u64 }`; `Waiting::Connect { to: String }` (a block id);
  `Workspace::ask_to_connect(&self, from: &str, block: &str, timeout: u64) -> Outcome`; `pub(crate) fn
  answer_connect(&mut self, key: u64, allow: bool, cx: &mut Context<Self>)`; `pub(crate) fn connect_line(&self,
  wait: &Wait, cx: &App) -> String`; `pub(super) fn in_space<R>(&mut self, id: &str, f: impl FnOnce(&mut Self) -> R)
  -> Option<R>`; `pub(super) fn uid_taken(&self, uid: &str) -> bool`.

- [ ] **Step 1: Write the failing tests**

`crates/templum-mcp/src/tools.rs`: the examples gain `("connect", json!({ "block": "k9x1d", "timeout": 60 })),`
after `ask_user`'s, and `the_tools_that_wait_have_their_defaults_and_limits` gains:

```rust
        assert_eq!(
            parse("connect", &json!({ "block": "b" })),
            Ok(Call::Connect {
                block: "b".into(),
                timeout: ASK_WAIT
            })
        );
```

`crates/templum-mcp/src/cli.rs`'s `ask_sends_the_question_and_its_options` gains, before its last three lines:

```rust
        run_with(&["connect", "k9x1d"], &env(&bus), "");
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap(),
            json!({ "name": "connect", "arguments": { "block": "k9x1d" } })
        );
```

`crates/templum/src/workspace/tests/bus.rs`: in `another_block_is_refused_until_it_is_connected`, after the first
`assert!(failed && text.contains("isn't connected to you"), …)`:

```rust
    assert!(
        text.contains(&format!("Ask the person with connect(\"{kanban}\").")),
        "{text}"
    );
```

and in `a_wired_terminal_is_read_and_typed_into_until_it_is_unwired`, the instructions' assertion becomes:

```rust
    assert!(
        told.contains(&format!("not connected (ask with connect): {k} (kanban \""))
            && told.contains(&format!("{n} (note \"")),
        "{told}"
    );
```

`crates/templum/src/workspace/tests/requests.rs` gains (its `use super::bus::{…}` gains `call`):

```rust
#[gpui_kit::test]
fn allow_wires_the_blocks_and_deny_refuses(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (k, n) = ws.read_with(cx, |ws, _| {
        (ws.space.uids[&kanban].clone(), ws.space.uids[&note].clone())
    });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "connect", json!({ "block": k }));
    assert!(answers.try_recv().is_err(), "it waits for the person");
    assert_eq!(
        dot_text(&ws, cx, mine),
        Some(format!("Waiting for you: connect to {k}"))
    );
    let key = last_wait(&ws, cx);
    ws.update(cx, |ws, cx| ws.answer_connect(key, true, cx));
    assert_eq!(answered(&answers), (false, format!("Connected to {k}.")));
    ws.read_with(cx, |ws, _| assert!(ws.space.wired(mine, kanban)));
    // Wired as by hand: Ctrl+Z takes it back.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(!ws.space.wired(mine, kanban)));
    let answers = call_later(handle, &ws, cx, &terminal, 2, "connect", json!({ "block": n }));
    let key = last_wait(&ws, cx);
    ws.update(cx, |ws, cx| ws.answer_connect(key, false, cx));
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("didn't connect you"), "{text}");
    ws.read_with(cx, |ws, _| assert!(!ws.space.wired(mine, note)));
}

#[gpui_kit::test]
fn connect_answers_at_once_when_there_is_nothing_to_ask(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, mine) = three_blocks(handle, &ws, cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (k, n) = ws.read_with(cx, |ws, _| {
        (ws.space.uids[&kanban].clone(), ws.space.uids[&note].clone())
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "connect", json!({ "block": uid }));
    assert!(failed && text.contains("your own block"), "{text}");
    let (failed, text) = call(handle, &ws, cx, &terminal, "connect", json!({ "block": "zzzzz" }));
    assert!(failed && text.contains("no block zzzzz"), "{text}");
    ws.update(cx, |ws, cx| assert!(ws.connect(mine, kanban, cx)));
    let (failed, text) = call(handle, &ws, cx, &terminal, "connect", json!({ "block": k }));
    assert!(!failed && text.contains("already connected"), "{text}");
    // A second request for one block while the first waits.
    let _first = call_later(handle, &ws, cx, &terminal, 1, "connect", json!({ "block": n }));
    let second = call_later(handle, &ws, cx, &terminal, 2, "connect", json!({ "block": n }));
    let (failed, text) = answered(&second);
    assert!(failed && text.contains("already asked"), "{text}");
}

#[gpui_kit::test]
fn closing_the_block_asked_for_withdraws_the_request(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let answers = call_later(handle, &ws, cx, &terminal, 1, "connect", json!({ "block": k }));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(kanban, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("was closed"), "{text}");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp && cargo test -p templum -- requests:: bus::`
Expected: compile errors (no `Call::Connect`, no `answer_connect`); once those exist, the refusal and instructions
assertions fail.

- [ ] **Step 3: The tool and the verb**

`crates/templum-mcp/src/tools.rs`: `Call` gains `Connect { block: String, timeout: u64 },`; `list` gains, after
`ask_user`'s entry:

```rust
        {
            "name": "connect",
            "description": "Asks the person to connect your block to another block of your workspace (list_blocks \
                            shows which aren't), in a balloon on your block. Waits for Allow or Deny; it may take \
                            minutes.",
            "inputSchema": {
                "type": "object",
                "properties": { "block": block, "timeout": seconds(ASK_WAIT) },
                "required": ["block"],
            },
        },
```

and `parse`: `"connect" => Call::Connect { block: text("block")?, timeout: seconds(ASK_WAIT)? },`.

`crates/templum-mcp/src/cli.rs`: `VERBS` gains `"connect"`; `call_of` gains, after the `ask` arm,
`("connect", [block]) => ("connect", json!({ "block": block })),` and, with the other usage errors,
`("connect", _) => return Err("connect <block> [--json]"),`.

- [ ] **Step 4: The request, the person's answer, and the words agents read**

`crates/templum/src/workspace/blocks.rs:78`: `pub(super) fn uid_taken`.

`crates/templum/src/workspace/spaces.rs`, after `with_space`:

```rust
    /// Runs `f` with the workspace `id` as the one shown: at once when it is, else as `with_space`
    /// does. None when there is no such workspace.
    pub(super) fn in_space<R>(&mut self, id: &str, f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        if self.space.id == id {
            Some(f(self))
        } else {
            self.with_space(id, f)
        }
    }
```

`crates/templum/src/workspace/waits.rs`: the imports gain `use super::bus::Outcome;`. `Waiting` gains:

```rust
    /// `connect`: the person's Allow or Deny to wiring the asking block to block `to` (its block
    /// id).
    Connect { to: String },
```

`summary` gains `Waiting::Connect { to } => format!("connect to {to}"),`; `expire`'s arm becomes
`Waiting::Question { .. } | Waiting::Connect { .. } => Reply::err(…)` (the same text); `orphaned`'s match becomes:

```rust
        match &wait.what {
            Waiting::Connect { to } if !self.uid_taken(to) => Some(Reply::err(format!(
                "{to} was closed; the request was withdrawn."
            ))),
            Waiting::Question { .. } | Waiting::Connect { .. } => None,
        }
```

and `tell_asking`'s gains `Waiting::Connect { .. } => self.connect_line(wait, cx),`. Then, in the `impl Workspace`:

```rust
    /// `connect` from the terminal `from`: the person's answer to wait for, unless there is
    /// nothing to ask.
    pub(super) fn ask_to_connect(&self, from: &str, block: &str, timeout: u64) -> Outcome {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let Some(to) = space.block_by_uid(block) else {
            return Outcome::Now(Reply::err(format!(
                "There's no block {block} in your workspace: list_blocks lists them."
            )));
        };
        if to == me {
            return Outcome::Now(Reply::err(format!(
                "{block} is your own block: you may use it already."
            )));
        }
        if space.wired(me, to) {
            return Outcome::Now(Reply::ok(format!("{block} is already connected to you.")));
        }
        let asked = self.waits.iter().any(|w| {
            w.from == from && matches!(&w.what, Waiting::Connect { to } if to == block)
        });
        if asked {
            return Outcome::Now(Reply::err(format!(
                "You already asked to connect to {block}; the person hasn't answered yet."
            )));
        }
        Outcome::Wait(
            Waiting::Connect {
                to: block.to_string(),
            },
            timeout,
        )
    }

    /// The person's Allow (`allow`) or Deny to the request to connect `key`. Allow wires the two
    /// blocks, a step Ctrl+Z takes back, as wiring by hand is.
    pub(crate) fn answer_connect(&mut self, key: u64, allow: bool, cx: &mut Context<Self>) {
        let Some(wait) = self.waits.iter().find(|w| w.key == key) else {
            return;
        };
        let Waiting::Connect { to } = &wait.what else {
            return;
        };
        let to = to.clone();
        let reply = match self.caller(&wait.from) {
            _ if !allow => Reply::err(format!("The person didn't connect you to {to}.")),
            None => Reply::err("Your block was closed; the request was withdrawn."),
            Some((space_id, me)) => {
                let other = self
                    .space_named(&space_id)
                    .and_then(|space| space.block_by_uid(&to));
                match other {
                    Some(other) => {
                        self.in_space(&space_id, |ws| ws.connect(me, other, cx));
                        Reply::ok(format!("Connected to {to}."))
                    }
                    None => Reply::err(format!("{to} isn't in your workspace any more.")),
                }
            }
        };
        self.answer(key, reply, cx);
    }

    /// "*Claude Code* wants to connect to *Dev server*": a request to connect as the person reads
    /// it, the blocks by their titles.
    pub(crate) fn connect_line(&self, wait: &Wait, cx: &App) -> String {
        let Waiting::Connect { to } = &wait.what else {
            return String::new();
        };
        let Some((space_id, me)) = self.caller(&wait.from) else {
            return String::new();
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let title = |id| self.about(&space.blocks[&id], cx).1;
        let from = title(me);
        let target = space.block_by_uid(to).map_or_else(|| to.clone(), title);
        format!("{from} wants to connect to {target}")
    }
```

`crates/templum/src/workspace/bus.rs`: `call` gains, before `Call::AskUser`:

```rust
            Call::Connect { block, timeout } => {
                return self.ask_to_connect(terminal, &block, timeout);
            }
```

`target`'s refusal (lines 421-426) loses its `ponytail:` comment and becomes:

```rust
    if id != me && !space.wired(me, id) {
        return Err(Reply::err(format!(
            "{uid} isn't connected to you. Ask the person with connect(\"{uid}\")."
        )));
    }
```

and `instructions` (lines 369-377) says:

```rust
        format!(
            "You are block {uid}, {title:?}, in Templum workspace {:?}. Connected to you: {}. In this \
             workspace, not connected (ask with connect): {}. Name blocks by these ids. You may only read \
             or write your own block and those connected to you; connect asks the person for another. \
             list_blocks gives the current list.",
            space.name,
            list(connected),
            list(others),
        )
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p templum-mcp && cargo test -p templum -- requests:: bus::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/templum-mcp/src crates/templum/src/workspace
git commit -m "feat(connections): let an agent ask you to wire it to another block" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

### Task 4: The balloon

**Files:**
- Create: `crates/templum/src/balloon.rs`; `mod balloon;` in `crates/templum/src/main.rs` (after `mod alert;`)
- Modify: `crates/templum/src/block.rs:108-111` (a block whose balloon field has the keyboard stays Full), `:649-653`
  (the balloon over the terminal)
- Modify: `crates/templum/src/git_block/widgets.rs:93-110` (`small` takes any label)
- Modify: `crates/templum/src/workspace/input.rs:598-615` (the field keeps its keys)
- Modify: `crates/templum/src/workspace/waits.rs` (`balloon_typing`)
- Modify: `crates/templum/src/workspace/mod.rs:34` (`pub(crate) use waits::{Wait, Waiting};`)
- Test: `crates/templum/src/workspace/tests/requests.rs`

**Interfaces:**
- Consumes: Task 2's `balloon`, `answer_question`, `send_field`, `Waiting::Question`'s `field`; Task 3's
  `answer_connect`, `connect_line`.
- Produces: `crate::balloon::render(ws: &Workspace, id: BlockId, terminal: &str, z: f32, cx: &mut
  Context<Workspace>) -> Option<AnyElement>`; `crate::balloon::part(n: usize, what: &str) -> ElementId`
  (`"balloon-<n>-<what>"`: `option-<i>`, `send`, `allow`, `deny`, `count`); the balloon itself is `("balloon", n)`;
  `Workspace::balloon_typing(&self, &Window, &App) -> Option<&str>` (the terminal whose balloon field has the
  keyboard); `git_block::widgets::small(id, label: impl Into<SharedString>, ui, z)`.

- [ ] **Step 1: Write the failing tests**

`crates/templum/src/workspace/tests/requests.rs` gains:

```rust
#[gpui_kit::test]
fn the_balloons_buttons_answer_a_question_then_a_request(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let n = mine.0 as usize;
    let part = crate::balloon::part;
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "Not yet"] });
    let question = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let request = call_later(handle, &ws, cx, &terminal, 2, "connect", json!({ "block": k }));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(part(n, "count")).is_some(), "1 of 2");
        window.click(part(n, "option-1"), cx);
    })
    .unwrap();
    assert_eq!(answered(&question), (false, "Not yet".to_string()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(part(n, "count")).is_none(), "the last one");
        window.click(part(n, "allow"), cx);
    })
    .unwrap();
    assert_eq!(answered(&request), (false, format!("Connected to {k}.")));
    ws.read_with(cx, |ws, _| assert!(ws.space.wired(mine, kanban)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("balloon", n)).is_none(), "nothing left to ask");
    })
    .unwrap();
}

/// Ctrl+L and Enter typed in the balloon's field while the asking terminal is focused: the field
/// takes them, the shell never sees them.
#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn keys_in_the_balloons_field_never_reach_the_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", json!({ "question": "Name?" }));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(mine, window, cx));
        window.render_frame(cx);
        let input = match &ws.read(cx).waits[0].what {
            crate::workspace::Waiting::Question {
                field: Some((input, _)),
                ..
            } => input.clone(),
            _ => panic!("a question without options has a field"),
        };
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
        window.input("Ada", cx);
        window.press("ctrl-l", cx);
        window.press("enter", cx);
    })
    .unwrap();
    assert_eq!(answered(&answers), (false, "Ada".to_string()));
    // `cat`'s tty would have echoed Ctrl+L as ^L.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let shown = ws.read_with(cx, |ws, _| {
        let mut snap = templum_terminal::Snapshot::default();
        snap.refresh(ws.space.terminal(mine).unwrap().term.shared(), &ws.theme.palette);
        snap.text()
    });
    assert!(!shown.contains("^L") && !shown.contains("Ada"), "{shown:?}");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum requests::`
Expected: compile error: no `crate::balloon`, no `crate::workspace::Waiting`.

- [ ] **Step 3: `small` takes any label**

`crates/templum/src/git_block/widgets.rs`: the imports gain `SharedString`; `small` becomes:

```rust
/// A text button in a list's header, and the panel's.
pub(crate) fn small(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    ui: Ui,
    z: f32,
) -> Stateful<Div> {
    let label: SharedString = label.into();
    div()
        .id(id)
        .px(px(6.0 * z))
        .py(px(2.0 * z))
        .rounded(px(4.0 * z))
        .text_size(px(11.0 * z))
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(label)
}
```

- [ ] **Step 4: Draw the balloon**

Create `crates/templum/src/balloon.rs`:

```rust
//! The balloon on a terminal block whose agent asks the person something
//! (docs/specs/connections.md, Requests from agents): a question, with a button per option or a
//! field and Send, or a request to connect, with Allow and Deny. Several wait in turn: the oldest
//! shows, with "1 of 3".

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, Context, ElementId, MouseButton, TestSupportExt, div, px};
use templum_canvas::{BlockId, chrome};

use crate::git_block::{self, widgets::small};
use crate::workspace::{Wait, Waiting, Workspace};

/// Its width, world units.
const WIDTH: f32 = 300.0;

/// The balloon of the terminal block `id`, whose terminal is `terminal`, at zoom `z`, over the
/// terminal's top right corner: None when its agent asks nothing.
pub(crate) fn render(
    ws: &Workspace,
    id: BlockId,
    terminal: &str,
    z: f32,
    cx: &mut Context<Workspace>,
) -> Option<AnyElement> {
    let (wait, count) = ws.balloon(terminal)?;
    let n = id.0 as usize;
    let ui = ws.ui;
    let text = match &wait.what {
        Waiting::Question { text, .. } => text.clone(),
        Waiting::Connect { .. } => ws.connect_line(wait, cx),
    };
    let choices = choices(ws, wait, n, z, cx);
    Some(
        div()
            .id(("balloon", n))
            .absolute()
            .top(px((chrome::TITLE_H + chrome::PAD) * z))
            .right(px(chrome::PAD * z))
            .w(px(WIDTH * z))
            .flex()
            .flex_col()
            .gap(px(8.0 * z))
            .p(px(10.0 * z))
            .rounded(px(8.0 * z))
            .border_1()
            .border_color(ui.accent)
            .bg(ui.surface)
            .text_size(px(git_block::FONT * z))
            .text_color(ui.title)
            // Its clicks are its own: no selection starts in the terminal under it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when(count > 1, |el| {
                el.child(
                    div()
                        .id(part(n, "count"))
                        .text_color(ui.title_dim)
                        .child(format!("1 of {count}"))
                        .test_support(),
                )
            })
            .child(div().child(text))
            .child(choices)
            .test_support()
            .into_any_element(),
    )
}

/// The id of the balloon's element `what` on block `n`: `option-<i>`, `send`, `allow`, `deny`,
/// `count`.
pub(crate) fn part(n: usize, what: &str) -> ElementId {
    ElementId::Name(format!("balloon-{n}-{what}").into())
}

/// What the person answers with: a button per option, the field and Send, or Allow and Deny.
fn choices(
    ws: &Workspace,
    wait: &Wait,
    n: usize,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (key, ui) = (wait.key, ws.ui);
    let row = div().flex().flex_wrap().items_center().gap(px(6.0 * z));
    match &wait.what {
        Waiting::Question {
            field: Some((input, _)),
            ..
        } => row.child(git_block::field(input, z).flex_1()).child(
            small(part(n, "send"), "Send", ui, z)
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.send_field(key, window, cx)
                }))
                .test_support(),
        ),
        Waiting::Question { options, .. } => {
            row.children(options.iter().enumerate().map(|(i, option)| {
                let answer = option.clone();
                small(part(n, &format!("option-{i}")), option.clone(), ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.answer_question(key, answer.clone(), window, cx)
                    }))
                    .test_support()
            }))
        }
        Waiting::Connect { .. } => row
            .child(
                small(part(n, "allow"), "Allow", ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                        ws.answer_connect(key, true, cx)
                    }))
                    .test_support(),
            )
            .child(
                small(part(n, "deny"), "Deny", ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                        ws.answer_connect(key, false, cx)
                    }))
                    .test_support(),
            ),
    }
    .into_any_element()
}
```

`crates/templum/src/workspace/mod.rs:34`, after `pub(crate) use find::FindBar;`:
`pub(crate) use waits::{Wait, Waiting};`.

`crates/templum/src/workspace/waits.rs`, after `balloon`:

```rust
    /// The terminal whose balloon's field has the keyboard, if one does: the keys are the field's.
    pub(crate) fn balloon_typing(&self, window: &Window, cx: &App) -> Option<&str> {
        self.waits
            .iter()
            .find(|w| field_focused(w, window, cx))
            .map(|w| w.from.as_str())
    }
```

`crates/templum/src/block.rs`, the level (lines 108-111):

```rust
    // The block worked in: focused, holding the keyboard, or its balloon's field holding it.
    let typing = live
        && matches!(body, Body::Terminal(pane)
            if ws.balloon_typing(window, cx) == Some(pane.term.id()));
    let level = if level != Level::Full
        && (focused || typing || (live && body.has_focus(window, cx)))
    {
```

and the root's last children (lines 649-653):

```rust
        .children(other_zones)
        .when(hovered && !tiled, |el| {
            el.child(fade_in(handle, ("handle-fade", n)))
        })
        // What its agent asks the person, over the terminal's top right corner, up close only.
        .children(match body {
            Body::Terminal(pane) if live && level == Level::Full => {
                crate::balloon::render(ws, id, pane.term.id(), z, cx)
            }
            _ => None,
        })
        .test_support()
```

- [ ] **Step 5: The field keeps its keys**

`crates/templum/src/workspace/input.rs`, in `on_key_down` right after the find field's block (line 615):

```rust
        // A balloon's field has the keyboard: what it leaves (shortcuts aside) never reaches the
        // shell of the focused terminal.
        if self.balloon_typing(window, cx).is_some() {
            if let Some(command) =
                command.filter(|c| !matches!(c, Command::Copy | Command::Paste))
            {
                self.command_key(e, Some(command), window, cx);
                cx.stop_propagation();
            }
            return;
        }
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p templum -- requests:: lod:: terminals::`
Expected: PASS (the new tests; the level-of-detail and terminal tests as before).

- [ ] **Step 7: Commit**

```bash
git add crates/templum/src
git commit -m "feat(connections): answer an agent's questions and connect requests in a balloon on its block" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

### Task 5: Handing a task over (`delegate`), answering it (`reply`), waiting for an agent (`wait_for_idle`)

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (constants, three `Call` variants, `list`, `parse`, tests)
- Modify: `crates/templum-mcp/src/cli.rs` (`delegate`, `reply`, `wait`, test)
- Modify: `crates/templum-terminal/src/input.rs` (a test pinning what `delegate` relies on)
- Create: `crates/templum/src/workspace/delegate.rs`
- Modify: `crates/templum/src/workspace/mod.rs` (`mod delegate;` after `mod bus;`; `Handed`; `Pane::handed`,
  `Pane::answer` at `:123-140`)
- Modify: `crates/templum/src/workspace/blocks.rs:395-407` (the `Pane` it builds)
- Modify: `crates/templum/src/workspace/bus.rs:14` (imports), `:160-183` (`agent_event`), `call`'s arms, `pane` and
  `pane_mut` next to `caller`
- Modify: `crates/templum/src/workspace/waits.rs` (`Waiting::Turn` and its arms), `crates/templum/src/balloon.rs`
  (the arms a turn never reaches)
- Create: `crates/templum/src/workspace/tests/delegate.rs`; register `mod delegate;` in `tests/mod.rs` after
  `mod config_reload;`

**Interfaces:**
- Consumes: Tasks 2-3 (`Outcome`, `Wait`, `answer`, `target`, `about`, `space_named`, `seconds`, the test helpers).
- Produces (templum-mcp): `tools::TASK_WAIT: u64 = 600`, `tools::IDLE_WAIT: u64 = 300`; `Call::Delegate { block,
  task, wait: bool, timeout: u64 }`, `Call::ReplyTo { block, text }`, `Call::WaitForIdle { block, timeout: u64 }`.
- Produces (app): `pub(crate) struct Handed { pub by: String, pub since: Instant, pub until: Instant }` (in
  `workspace/mod.rs`); `Pane { pub handed: Option<Handed>, pub answer: Option<String>, .. }`; `Pane::task(&self,
  Instant) -> Option<&Handed>`, `Pane::busy(&self, Instant) -> bool`; `Waiting::Turn { of: String /* terminal */,
  uid: String /* block id */ }`; on `Workspace`: `pane(&self, terminal: &str) -> Option<&Pane>`, `pane_mut`,
  `send_task(&self, terminal: &str, text: &str, cx)`, `delegate(&mut self, from, block, task, wait, timeout, cx) ->
  Outcome`, `reply_to(&mut self, from, block, text, cx) -> Reply`, `wait_for_idle(&self, from, block, timeout) ->
  Outcome`, `end_turn(&mut self, terminal: &str, reply: Reply, cx)`; `delegate::ENTER_AFTER: Duration` (150 ms).

- [ ] **Step 1: Write the failing tool and verb tests**

`crates/templum-mcp/src/tools.rs`: the examples gain, after `connect`'s:

```rust
            (
                "delegate",
                json!({ "block": "k9x1d", "task": "fix the tests", "wait": true, "timeout": 900 }),
            ),
            ("reply", json!({ "block": "b7k2m", "text": "Done." })),
            ("wait_for_idle", json!({ "block": "k9x1d", "timeout": 60 })),
```

and `the_tools_that_wait_have_their_defaults_and_limits` gains:

```rust
        assert_eq!(
            parse("delegate", &json!({ "block": "b", "task": "t" })),
            Ok(Call::Delegate {
                block: "b".into(),
                task: "t".into(),
                wait: false,
                timeout: TASK_WAIT
            })
        );
        assert_eq!(
            parse("wait_for_idle", &json!({ "block": "b" })),
            Ok(Call::WaitForIdle {
                block: "b".into(),
                timeout: IDLE_WAIT
            })
        );
        assert!(parse("reply", &json!({ "block": "b" })).is_err(), "no text");
```

`crates/templum-mcp/src/cli.rs`'s tests:

```rust
    #[test]
    fn delegate_reply_and_wait_send_their_calls() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let _ = m.reply.send(tool_result(id, &Reply::ok("ok")));
            }
        });
        for (args, sent) in [
            (
                &["delegate", "k9x1d", "fix", "the", "tests", "--wait"][..],
                json!({ "name": "delegate", "arguments": { "block": "k9x1d", "task": "fix the tests", "wait": true } }),
            ),
            (
                &["reply", "b7k2m", "all", "green"][..],
                json!({ "name": "reply", "arguments": { "block": "b7k2m", "text": "all green" } }),
            ),
            (
                &["wait", "k9x1d", "--timeout", "60"][..],
                json!({ "name": "wait_for_idle", "arguments": { "block": "k9x1d", "timeout": 60 } }),
            ),
        ] {
            assert_eq!(run_with(args, &env(&bus), "").0, 0, "{args:?}");
            assert_eq!(calls.recv_timeout(LIMIT).unwrap(), sent);
        }
        assert_eq!(run_with(&["delegate", "k9x1d", "--wait"], &env(&bus), "").0, 2, "no task");
        assert_eq!(
            run_with(&["wait", "k9x1d", "--for", "idle"], &env(&bus), "").0,
            2,
            "--for is batch 5's"
        );
    }
```

Run: `cargo test -p templum-mcp` → FAIL (no such variants; the verbs are unknown).

- [ ] **Step 2: The tools and the verbs**

`crates/templum-mcp/src/tools.rs`, after `ASK_WAIT`:

```rust
/// How long `delegate` waits for the answer, and how long a task stays handed over, seconds,
/// unless asked.
pub const TASK_WAIT: u64 = 600;
/// How long `wait_for_idle` waits, seconds, unless asked.
pub const IDLE_WAIT: u64 = 300;
```

`Call` gains:

```rust
    Delegate {
        block: String,
        task: String,
        wait: bool,
        timeout: u64,
    },
    ReplyTo {
        block: String,
        text: String,
    },
    WaitForIdle {
        block: String,
        timeout: u64,
    },
```

`list`, after `connect`'s entry:

```rust
        {
            "name": "delegate",
            "description": "Hands a task to an agent connected to you: Templum types it into its terminal, after \
                            a line saying it is from you, and sends it. Its answer is its last message when it \
                            finishes its turn, or what it sends you with reply. With wait, waits for that answer \
                            (it may take minutes); without, wait_for_idle gets it later. Refused while the agent \
                            waits for its person or is on another agent's task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "block": block,
                    "task": { "type": "string" },
                    "wait": { "type": "boolean", "default": false },
                    "timeout": seconds(TASK_WAIT),
                },
                "required": ["block", "task"],
            },
        },
        {
            "name": "reply",
            "description": "Answers the task an agent connected to you handed you (it began with [from <title> \
                            (<id>)]): the agent waiting for it gets your text now. When it isn't waiting, the \
                            text is typed into its terminal after a line saying it is from you.",
            "inputSchema": {
                "type": "object",
                "properties": { "block": block, "text": { "type": "string" } },
                "required": ["block", "text"],
            },
        },
        {
            "name": "wait_for_idle",
            "description": "Waits until an agent connected to you finishes its turn (or the task you handed it), \
                            and returns its last answer; at once when it is idle. It may take minutes.",
            "inputSchema": {
                "type": "object",
                "properties": { "block": block, "timeout": seconds(IDLE_WAIT) },
                "required": ["block"],
            },
        },
```

`parse`, after `connect`'s arm:

```rust
        "delegate" => Call::Delegate {
            block: text("block")?,
            task: text("task")?,
            wait: flag("wait")?,
            timeout: seconds(TASK_WAIT)?,
        },
        "reply" => Call::ReplyTo {
            block: text("block")?,
            text: text("text")?,
        },
        "wait_for_idle" => Call::WaitForIdle {
            block: text("block")?,
            timeout: seconds(IDLE_WAIT)?,
        },
```

`crates/templum-mcp/src/cli.rs`: `VERBS` gains `"delegate", "reply", "wait"`; after `ASK`:
`const WAIT: &str = "wait <block> [--timeout <seconds>] [--json]";`; `call_of` gains, after the `connect` arm:

```rust
        ("delegate", [block, task @ ..]) if task.iter().any(|a| *a != "--wait") => {
            let wait = task.contains(&"--wait");
            let task: Vec<&str> = task.iter().copied().filter(|a| *a != "--wait").collect();
            (
                "delegate",
                json!({ "block": block, "task": task.join(" "), "wait": wait }),
            )
        }
        ("reply", [block, text @ ..]) if !text.is_empty() => (
            "reply",
            json!({ "block": block, "text": text.join(" ") }),
        ),
        ("wait", [block]) => ("wait_for_idle", json!({ "block": block })),
        ("wait", [block, "--timeout", seconds]) => {
            let seconds: u64 = seconds.parse().map_err(|_| WAIT)?;
            (
                "wait_for_idle",
                json!({ "block": block, "timeout": seconds }),
            )
        }
```

and with the usage errors:

```rust
        ("delegate", _) => return Err("delegate <block> <task> [--wait] [--json]"),
        ("reply", _) => return Err("reply <block> <text> [--json]"),
        ("wait", _) => return Err(WAIT),
```

Run: `cargo test -p templum-mcp` → PASS.

- [ ] **Step 3: Pin the paste a task travels in**

`crates/templum-terminal/src/input.rs`'s tests, after `typed_text_never_presses_enter`:

```rust
    /// A delegated task: its header, its lines and a paste end of its own stay inside one paste,
    /// whose end (and the Enter after it) only Templum writes.
    #[test]
    fn a_task_cant_end_its_paste_early() {
        let task = "[from api (b7k2m)]\nfix it\x1b[201~\rrm -rf ~\n";
        assert_eq!(
            typed_bytes(task, true),
            b"\x1b[200~[from api (b7k2m)]\nfix it[201~\rrm -rf ~\x1b[201~"
        );
        assert_eq!(typed_bytes(task, false), b"[from api (b7k2m)] fix it [201~ rm -rf ~");
    }
```

Run: `cargo test -p templum-terminal a_task_cant_end_its_paste_early`
Expected: PASS as it is: it pins what `delegate` relies on (`type_in`), so a change to `typed_bytes` can't break it
unnoticed.

- [ ] **Step 4: Write the failing app tests**

`crates/templum/src/workspace/tests/delegate.rs`:

```rust
//! Tasks handed between agents: `delegate`, `reply`, `wait_for_idle`, and the dot on the wire
//! (docs/specs/connections.md, Delegating).

use serde_json::json;

#[cfg(unix)]
use super::bus::screen_with;
use super::bus::{after, answered, call, call_later, event, me};
use super::*;

/// Two terminals side by side and wired: mine at (40, 100) and theirs at (900, 100), 700 × 300
/// each, theirs running Codex as far as Templum knows; the camera at 100% on the origin.
fn two_agents(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, BlockId, BlockId) {
    let (handle, ws) = open(cx);
    let mine = first_id(&ws, cx);
    let theirs = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.space.pristine = false;
                ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), 1.0);
                ws.space.canvas.get_mut(mine).unwrap().rect = Rect::new(40.0, 100.0, 700.0, 300.0);
                ws.open(Rect::new(900.0, 100.0, 700.0, 300.0), window, cx);
                let theirs = ws.space.canvas.top().unwrap();
                ws.space.terminal_mut(theirs).unwrap().seen.agent = Some("codex".into());
                ws.space.appearing.clear();
                assert!(ws.connect(mine, theirs, cx));
                theirs
            })
        })
        .unwrap();
    frame(handle, cx);
    (handle, ws, mine, theirs)
}

/// The Enter a typed task waits for.
#[cfg(unix)]
fn enter(cx: &mut TestAppContext) {
    after(cx, crate::workspace::delegate::ENTER_AFTER.as_millis() as u64);
}

#[cfg(unix)]
#[gpui_kit::test]
fn delegate_types_the_header_the_task_and_enter(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "fix the tests\nthen push" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(!failed && text.starts_with(&format!("Handed to {them}.")), "{text}");
    ws.read_with(cx, |ws, _| {
        let handed = ws.space.terminal(theirs).unwrap().handed.as_ref().expect("handed over");
        assert_eq!(handed.by, terminal);
    });
    enter(cx);
    // `cat` asks for no bracketed paste: the task comes as one line, echoed as it is typed, then
    // printed again once Enter sent it.
    screen_with(&ws, cx, theirs, &format!("({uid})] fix the tests then push[from"));
}

#[gpui_kit::test]
fn delegate_refuses_what_it_mustnt_type_into(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go" });
    // Waiting for its person: the typed task would answer its question.
    event(handle, &ws, cx, &worker, json!({ "event": "needs_you", "message": "Allow Bash?" }));
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task.clone());
    assert!(failed && text.contains("waiting for its person"), "{text}");
    // Working on its own prompt: typed in, the task waits its turn.
    event(handle, &ws, cx, &worker, json!({ "event": "working" }));
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task.clone());
    assert!(!failed, "{text}");
    // On a task already: a second one would take the first one's answer.
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task.clone());
    assert!(failed && text.contains("already on a task"), "{text}");
    // To itself, and to a terminal running no agent.
    let mine_task = json!({ "block": uid, "task": "go" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", mine_task);
    assert!(failed && text.contains("yours"), "{text}");
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(theirs).unwrap();
        pane.seen.agent = None;
        pane.handed = None;
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(failed && text.contains("runs no agent"), "{text}");
}

#[gpui_kit::test]
fn a_delegation_that_waits_ends_with_the_turns_answer(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "run the tests", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    assert!(answers.try_recv().is_err(), "it waits for the answer");
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": "All green." }));
    assert_eq!(answered(&answers), (false, "All green.".to_string()));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(theirs).unwrap().handed.is_none())
    });
}

#[gpui_kit::test]
fn reply_answers_the_task_before_its_turn_ends(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "find the bug", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    let reply = json!({ "block": uid, "text": "Line 42." });
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply);
    assert_eq!((failed, text), (false, format!("Answered {uid}.")));
    assert_eq!(answered(&answers), (false, "Line 42.".to_string()));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(theirs).unwrap().handed.is_none())
    });
    // Its turn's end comes later: nothing waits for it any more.
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": "Fixed it too." }));
    assert!(answers.try_recv().is_err(), "answered once");
}

#[cfg(unix)]
#[gpui_kit::test]
fn reply_with_no_one_waiting_types_into_the_agent(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (_, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let reply = json!({ "block": uid, "text": "Done." });
    // Mine runs no agent: the reply would run in its shell.
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply.clone());
    assert!(failed && text.contains("runs no agent"), "{text}");
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(mine).unwrap().seen.agent = Some("claude".into())
    });
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply);
    assert_eq!((failed, text), (false, format!("Typed into {uid}.")));
    enter(cx);
    screen_with(&ws, cx, mine, &format!("({them})] Done.[reply from"));
}

#[gpui_kit::test]
fn wait_for_idle_waits_for_the_turn_and_gives_the_last_answer(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let idle = json!({ "block": them });
    let (failed, text) = call(handle, &ws, cx, &terminal, "wait_for_idle", idle.clone());
    assert!(!failed && text.contains("is idle"), "{text}");
    event(handle, &ws, cx, &worker, json!({ "event": "working" }));
    let answers = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle.clone());
    assert!(answers.try_recv().is_err(), "it waits while the agent works");
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": "Merged." }));
    assert_eq!(answered(&answers), (false, "Merged.".to_string()));
    let (failed, text) = call(handle, &ws, cx, &terminal, "wait_for_idle", idle);
    assert_eq!((failed, text), (false, "Merged.".to_string()), "its last answer, at once");
}

#[gpui_kit::test]
fn a_wait_that_times_out_gives_the_last_lines(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "think", "wait": true, "timeout": 2 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    after(cx, 2000);
    let (failed, text) = answered(&answers);
    assert!(
        failed
            && text.contains(&format!("No answer from {them} within 2 seconds"))
            && text.contains("last lines"),
        "{text}"
    );
}

#[gpui_kit::test]
fn a_turns_end_and_its_timeout_answer_once(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true, "timeout": 2 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": "Done." }));
    after(cx, 3000);
    assert_eq!(answered(&answers), (false, "Done.".to_string()));
    assert!(answers.try_recv().is_err(), "answered once");
}

#[gpui_kit::test]
fn closing_the_agent_worked_on_answers_its_wait(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(theirs, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("was closed before it answered"), "{text}");
}
```

Run: `cargo test -p templum delegate::`
Expected: compile errors: no field `handed` on `Pane`, no module `delegate` in `workspace`.

- [ ] **Step 5: A task handed over, on the agent's pane**

`crates/templum/src/workspace/mod.rs`, after `AgentState`:

```rust
/// A task handed to a terminal's agent (`delegate`, or `spawn_agent`'s prompt), while its answer
/// is awaited (docs/specs/connections.md, Delegating).
pub(crate) struct Handed {
    /// The terminal of the agent that handed it over.
    pub by: String,
    pub since: Instant,
    /// After this it is no longer awaited: its dot stops, and another task may be handed over.
    pub until: Instant,
}
```

`Pane` gains, after `state`:

```rust
    /// The task another agent handed its agent, while its answer is awaited.
    pub handed: Option<Handed>,
    /// Its agent's last answer: the message its last turn ended with, or its last `reply`.
    pub answer: Option<String>,
```

`crates/templum/src/workspace/blocks.rs:405-406`: the `Pane` built gains `handed: None, answer: None,`.
`mod delegate;` goes after `mod bus;`.

- [ ] **Step 6: Delegating, replying, waiting for an agent**

Create `crates/templum/src/workspace/delegate.rs`:

```rust
//! Tasks handed between agents (docs/specs/connections.md, Delegating): typed into an agent wired
//! to the one handing it over; its answer is the end of that agent's turn, or its `reply`.

use std::time::{Duration, Instant};

use gpui_kit::Context;
use templum_mcp::Reply;

use super::bus::{Outcome, target};
use super::waits::Waiting;
use super::{AgentState, Handed, Pane, Workspace};

/// How long after a task is typed its Enter goes: an agent may take keys that come in one burst
/// for a paste, where Enter is a new line.
pub(super) const ENTER_AFTER: Duration = Duration::from_millis(150);

impl Pane {
    /// The task its agent works on, while its answer is awaited.
    pub fn task(&self, now: Instant) -> Option<&Handed> {
        self.handed.as_ref().filter(|h| now < h.until)
    }

    /// Its agent is at work: on a task, on a turn, or on a question to its person.
    pub fn busy(&self, now: Instant) -> bool {
        self.task(now).is_some()
            || matches!(
                self.state,
                Some(AgentState::Working | AgentState::NeedsYou(_))
            )
    }

    /// Templum may type into it (its block id `uid`): an agent runs there, not waiting for its
    /// person (a permission dialog would take the text as its answer). Never a shell, where the
    /// text would run as a command.
    fn takes_typing(&self, uid: &str) -> Result<(), Reply> {
        if self.seen.agent.is_none() {
            return Err(Reply::err(format!(
                "{uid} runs no agent: send_to_terminal types into a terminal."
            )));
        }
        if matches!(self.state, Some(AgentState::NeedsYou(_))) {
            return Err(Reply::err(format!(
                "{uid} is waiting for its person: Templum won't type into it now. Try again later."
            )));
        }
        Ok(())
    }
}

impl Workspace {
    /// Types `text` into the agent in terminal `terminal` as one paste, then Enter a moment later.
    pub(super) fn send_task(&self, terminal: &str, text: &str, cx: &mut Context<Self>) {
        let Some(pane) = self.pane(terminal) else {
            return;
        };
        pane.term.type_in(text);
        let terminal = terminal.to_string();
        cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(ENTER_AFTER).await;
            ws.update(cx, |ws, _| {
                if let Some(pane) = ws.pane(&terminal) {
                    pane.term.write(&b"\r"[..]);
                }
            })
            .ok();
        })
        .detach();
    }

    /// `delegate` from the terminal `from`: `task` typed into the agent `block` under a line
    /// saying who hands it over; with `wait`, the call waits for its answer.
    pub(super) fn delegate(
        &mut self,
        from: &str,
        block: &str,
        task: &str,
        wait: bool,
        timeout: u64,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let now = Instant::now();
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return Outcome::Now(refused),
        };
        if to == me {
            return Outcome::Now(Reply::err(
                "This block is yours: hand the task to another agent.",
            ));
        }
        let Some(pane) = space.terminal(to) else {
            return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
        };
        if let Err(refused) = pane.takes_typing(block) {
            return Outcome::Now(refused);
        }
        if pane.task(now).is_some() {
            return Outcome::Now(Reply::err(format!(
                "{block} is already on a task another agent handed it: wait_for_idle(\"{block}\") \
                 waits for its end."
            )));
        }
        let worker = pane.term.id().to_string();
        let (_, title, _) = self.about(&space.blocks[&me], cx);
        let uid = space.uids.get(&me).cloned().unwrap_or_default();
        self.send_task(&worker, &format!("[from {title} ({uid})]\n{task}"), cx);
        if let Some(pane) = self.pane_mut(&worker) {
            pane.handed = Some(Handed {
                by: from.to_string(),
                since: now,
                until: now + Duration::from_secs(timeout),
            });
        }
        cx.notify();
        if wait {
            let what = Waiting::Turn {
                of: worker,
                uid: block.to_string(),
            };
            Outcome::Wait(what, timeout)
        } else {
            Outcome::Now(Reply::ok(format!(
                "Handed to {block}. wait_for_idle(\"{block}\") gives its answer once it is done, \
                 unless it replies sooner."
            )))
        }
    }

    /// `reply` from the terminal `from` to the agent `block`: its calls waiting on the caller
    /// (`delegate` with `wait`, `wait_for_idle`) get `text`; with none, the text is typed into it
    /// as a message. Either way the task it handed the caller is answered.
    pub(super) fn reply_to(
        &mut self,
        from: &str,
        block: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return refused,
        };
        if to == me {
            return Reply::err("This block is yours: reply to the agent that handed you the task.");
        }
        let Some(pane) = space.terminal(to) else {
            return Reply::err(format!("{block} isn't a terminal."));
        };
        let asker = pane.term.id().to_string();
        let waiting: Vec<u64> = self
            .waits
            .iter()
            .filter(|w| {
                w.from == asker && matches!(&w.what, Waiting::Turn { of, .. } if of == from)
            })
            .map(|w| w.key)
            .collect();
        if waiting.is_empty() {
            if let Err(refused) = pane.takes_typing(block) {
                return refused;
            }
            let (_, title, _) = self.about(&space.blocks[&me], cx);
            let uid = space.uids.get(&me).cloned().unwrap_or_default();
            self.send_task(&asker, &format!("[reply from {title} ({uid})]\n{text}"), cx);
        }
        if let Some(mine) = self.pane_mut(from) {
            if mine.handed.as_ref().is_some_and(|h| h.by == asker) {
                mine.handed = None;
            }
            mine.answer = Some(text.to_string());
        }
        let answered = !waiting.is_empty();
        for key in waiting {
            self.answer(key, Reply::ok(text), cx);
        }
        cx.notify();
        Reply::ok(if answered {
            format!("Answered {block}.")
        } else {
            format!("Typed into {block}.")
        })
    }

    /// `wait_for_idle` from the terminal `from` on the agent `block`: its last answer now when it
    /// is idle, else once its turn, or the task it is on, ends.
    pub(super) fn wait_for_idle(&self, from: &str, block: &str, timeout: u64) -> Outcome {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return Outcome::Now(refused),
        };
        if to == me {
            return Outcome::Now(Reply::err("This block is yours: wait for another agent."));
        }
        let Some(pane) = space.terminal(to) else {
            return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
        };
        if pane.seen.agent.is_none() {
            return Outcome::Now(Reply::err(format!(
                "{block} runs no agent: read_terminal reads its screen."
            )));
        }
        if pane.busy(Instant::now()) {
            let what = Waiting::Turn {
                of: pane.term.id().to_string(),
                uid: block.to_string(),
            };
            return Outcome::Wait(what, timeout);
        }
        Outcome::Now(Reply::ok(pane.answer.clone().unwrap_or_else(|| {
            format!("{block} is idle; Templum has heard no answer from it.")
        })))
    }

    /// The agent in the terminal `terminal` ended its turn, or its session: the task it was on is
    /// no longer handed over, and the calls waiting on it get `reply`.
    pub(super) fn end_turn(&mut self, terminal: &str, reply: Reply, cx: &mut Context<Self>) {
        if let Some(pane) = self.pane_mut(terminal) {
            pane.handed = None;
        }
        let waiting: Vec<u64> = self
            .waits
            .iter()
            .filter(|w| matches!(&w.what, Waiting::Turn { of, .. } if of == terminal))
            .map(|w| w.key)
            .collect();
        for key in waiting {
            self.answer(key, reply.clone(), cx);
        }
    }
}
```

- [ ] **Step 7: Turns in the waits**

`crates/templum/src/workspace/waits.rs`: after the imports, `const LAST_LINES: usize = 40;` with the doc "The
lines of an agent's screen a turn's wait that times out gives."; `Waiting` gains:

```rust
    /// `delegate` with `wait`, `wait_for_idle`: the end of the turn of the agent in the terminal
    /// `of` (the block `uid`).
    Turn { of: String, uid: String },
```

`summary` gains `Waiting::Turn { .. } => String::new(),` (the person is never asked); `wait` tells the person only
of what asks them:

```rust
        if !matches!(wait.what, Waiting::Turn { .. }) {
            self.tell_asking(&wait, window, cx);
        }
```

`expire`'s match gains:

```rust
            Waiting::Turn { of, uid } => {
                let lines = self
                    .pane(of)
                    .map(|pane| pane.term.shared().lines(LAST_LINES).join("\n"))
                    .unwrap_or_default();
                Reply::err(format!(
                    "No answer from {uid} within {seconds} seconds; it may still be working \
                     (wait_for_idle(\"{uid}\") waits again). The last lines of its screen:\n{}",
                    tools::tail(&lines, tools::MAX_TEXT)
                ))
            }
```

`orphaned`'s match gains, before its last arm (which becomes `Waiting::Question { .. } | Waiting::Connect { .. } |
Waiting::Turn { .. } => None`):

```rust
            Waiting::Turn { of, uid } if self.pane(of).is_none() => Some(Reply::err(format!(
                "{uid} was closed before it answered."
            ))),
```

`tell_asking`'s match gains `Waiting::Turn { .. } => return,`; and `balloon` lists only what asks the person:

```rust
        let mut asking = self
            .waits
            .iter()
            .filter(|w| w.from == terminal && !matches!(w.what, Waiting::Turn { .. }));
```

`crates/templum/src/balloon.rs`: `render`'s text match gains `Waiting::Turn { .. } => return None,` and `choices`'
match `Waiting::Turn { .. } => row,` (`balloon` never hands one over).

- [ ] **Step 8: The calls and the agents' events**

`crates/templum/src/workspace/bus.rs`: the imports become `use super::{AgentState, Body, Pane, Space, Workspace,
show_notification};`. After `caller`:

```rust
    /// The pane of the terminal `terminal` (its `TEMPLUM_TERMINAL`), in any workspace.
    pub(super) fn pane(&self, terminal: &str) -> Option<&Pane> {
        self.spaces()
            .flat_map(|s| s.blocks.values())
            .filter_map(Body::terminal)
            .find(|pane| pane.term.id() == terminal)
    }

    pub(super) fn pane_mut(&mut self, terminal: &str) -> Option<&mut Pane> {
        std::iter::once(&mut self.space)
            .chain(&mut self.others)
            .flat_map(|s| s.blocks.values_mut())
            .filter_map(Body::terminal_mut)
            .find(|pane| pane.term.id() == terminal)
    }
```

`agent_event` (lines 160-183) becomes:

```rust
    /// An agent's hook says where its turn is. Its turn's end answers what waited on it.
    fn agent_event(
        &mut self,
        terminal: &str,
        event: templum_mcp::AgentEvent,
        cx: &mut Context<Self>,
    ) {
        use templum_mcp::AgentEvent;
        let Some(pane) = self.pane_mut(terminal) else {
            return;
        };
        match event {
            AgentEvent::Working => pane.state = Some(AgentState::Working),
            AgentEvent::Started => pane.state = Some(AgentState::Idle),
            AgentEvent::Stopped { message } => {
                pane.state = Some(AgentState::Idle);
                if let Some(message) = &message {
                    pane.answer = Some(message.clone());
                }
                let reply = match message {
                    Some(message) => Reply::ok(message),
                    None => Reply::err(
                        "Its turn ended without an answer: an error, or it was interrupted.",
                    ),
                };
                self.end_turn(terminal, reply, cx);
            }
            AgentEvent::NeedsYou { message } => pane.state = Some(AgentState::NeedsYou(message)),
            AgentEvent::Ended => {
                pane.state = None;
                self.end_turn(terminal, Reply::err("Its agent ended before it answered."), cx);
            }
        }
        cx.notify();
    }
```

`call` gains, before `Call::AskUser`:

```rust
            Call::Delegate {
                block,
                task,
                wait,
                timeout,
            } => return self.delegate(terminal, &block, &task, wait, timeout, cx),
            Call::ReplyTo { block, text } => self.reply_to(terminal, &block, &text, cx),
            Call::WaitForIdle { block, timeout } => {
                return self.wait_for_idle(terminal, &block, timeout);
            }
```

- [ ] **Step 9: Run the tests**

Run: `cargo test -p templum -- delegate:: requests:: bus::`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/templum-mcp/src crates/templum-terminal/src/input.rs crates/templum/src
git commit -m "feat(connections): hand a task to a wired agent and get its answer back" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

### Task 6: The dot that travels along the wire

**Files:**
- Modify: `crates/templum/src/wires.rs` (the dot's sizes and trip; `paint` draws dots; a unit test)
- Modify: `crates/templum/src/workspace/wires.rs` (`wire_dots`)
- Modify: `crates/templum/src/workspace/render.rs:433-447` (the dots, their frames, painting them)
- Modify: `crates/templum/src/workspace/mod.rs` (`_dot_frame` field and its start value)
- Test: `crates/templum/src/workspace/tests/delegate.rs`, inline in `crates/templum/src/wires.rs`

**Interfaces:**
- Consumes: Task 5's `Pane::task`, `Handed { by, since, .. }`.
- Produces: `crate::wires::{DOT_TRIP, DOT_FRAME}: Duration`, `crate::wires::dot_radius(zoom: f32) -> f32`,
  `crate::wires::paint(curves, dots: &[Point], dot: Hsla, camera, bounds, window)`; `Workspace::wire_dots(&self, now:
  Instant, still: bool) -> Vec<Point>` (world points).

- [ ] **Step 1: Write the failing tests**

`crates/templum/src/wires.rs`'s tests:

```rust
    #[test]
    fn the_dot_is_4_units_and_never_under_2_5_px() {
        assert_eq!(dot_radius(1.0), 4.0);
        assert_eq!(dot_radius(2.0), 8.0);
        assert_eq!(dot_radius(0.25), 2.5);
    }
```

`crates/templum/src/workspace/tests/delegate.rs` (its imports gain `use std::time::Instant;` and
`use templum_canvas::Wire;`):

```rust
#[gpui_kit::test]
fn a_dot_travels_only_while_a_task_is_handed_over(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let dots = |cx: &mut TestAppContext, at: Instant, still: bool| {
        ws.read_with(cx, |ws, _| ws.wire_dots(at, still))
    };
    assert!(dots(cx, Instant::now(), false).is_empty(), "nothing handed over");
    call(handle, &ws, cx, &terminal, "delegate", json!({ "block": them, "task": "go" }));
    let since = ws.read_with(cx, |ws, _| {
        ws.space.terminal(theirs).unwrap().handed.as_ref().unwrap().since
    });
    let trip = crate::wires::DOT_TRIP;
    // The wire runs from mine's right edge (740, 250) to theirs' left one (900, 250). The dot
    // leaves from the agent that handed the task over, is halfway at half a trip, and starts again.
    let near = |dots: Vec<Point>, x: f32| {
        dots.len() == 1 && (dots[0].x - x).abs() < 0.5 && (dots[0].y - 250.0).abs() < 0.5
    };
    assert!(near(dots(cx, since, false), 740.0));
    assert!(near(dots(cx, since + trip / 2, false), 820.0));
    assert!(near(dots(cx, since + trip, false), 740.0));
    assert!(near(dots(cx, since, true), 820.0), "still, at the middle, with reduced motion");
    // Painted with the wires.
    frame(handle, cx);
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": "Done." }));
    assert!(dots(cx, Instant::now(), false).is_empty(), "the answer came back");
}

#[gpui_kit::test]
fn the_dot_goes_with_its_wire_and_the_answer_still_comes(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    ws.update(cx, |ws, cx| {
        assert!(ws.disconnect(Wire { a: mine, b: theirs }, cx))
    });
    ws.read_with(cx, |ws, _| {
        assert!(ws.wire_dots(Instant::now(), false).is_empty())
    });
    event(handle, &ws, cx, &worker, json!({ "event": "stopped", "message": "Done." }));
    assert_eq!(answered(&answers), (false, "Done.".to_string()));
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum dot`
Expected: compile errors: no `dot_radius`, no `DOT_TRIP`, no method `wire_dots`.

- [ ] **Step 3: Where the dots are**

`crates/templum/src/workspace/wires.rs`: the imports gain `use std::time::Instant;`; after `wire_curves`:

```rust
    /// The dots on the wires along which a task is handed over (docs/specs/connections.md, Wires):
    /// each on its way from the agent that handed the task over to the one working on it, a trip
    /// every DOT_TRIP; at the middle when `still` (reduced motion). None in grid mode, which hides
    /// wires, nor without a wire between the two.
    pub(super) fn wire_dots(&self, now: Instant, still: bool) -> Vec<Point> {
        if self.space.grid.is_some() {
            return Vec::new();
        }
        self.space
            .terminals()
            .filter_map(|(to, pane)| {
                let handed = pane.task(now)?;
                let (from, _) = self
                    .space
                    .terminals()
                    .find(|(_, p)| p.term.id() == handed.by)?;
                let wire = *self.space.wires.iter().find(|w| **w == Wire { a: from, b: to })?;
                let phase = if still {
                    0.5
                } else {
                    let gone = now.saturating_duration_since(handed.since).as_secs_f32();
                    (gone / crate::wires::DOT_TRIP.as_secs_f32()) % 1.0
                };
                // The curve runs from the wire's `a` to its `b`.
                let t = if wire.a == from { phase } else { 1.0 - phase };
                Some(self.space.curve(wire)?.at(t))
            })
            .collect()
    }
```

- [ ] **Step 4: Painting the dots**

`crates/templum/src/wires.rs`: the doc line becomes "Painting the wires between blocks, under them, and the dot
that travels along one while a task is handed over (docs/specs/connections.md, Wires)."; the imports become:

```rust
use std::time::Duration;

use gpui_kit::{Bounds, Hsla, PathBuilder, Pixels, Window, fill, point, px, size};
use templum_canvas::{Camera, Curve, Point, Rect};
```

after `WIDTH`:

```rust
/// The travelling dot's radius, world units.
const DOT: f32 = 4.0;
/// How long the dot takes from the agent that handed a task over to the one working on it.
pub(crate) const DOT_TRIP: Duration = Duration::from_millis(1200);
/// How often the dot is drawn while one travels in view: about 30 times a second, not at the
/// display's rate (a task can be handed over for minutes).
pub(crate) const DOT_FRAME: Duration = Duration::from_millis(33);
```

after `width`:

```rust
/// The dot's radius on screen at `zoom`, pixels: never under 2.5, so it shows on a 1 px wire.
pub(crate) fn dot_radius(zoom: f32) -> f32 {
    (DOT * zoom).max(2.5)
}
```

and `paint` takes the dots and their color, and draws them after the curves, round (as the history's graph dots,
`crates/templum/src/git_block/history.rs:442-448`):

```rust
/// Paints `curves` (world units) through `camera` into `bounds`, then a dot of the color `dot` at
/// each of `dots`; those off screen are skipped.
// ponytail: one path per wire per frame. gpui-fast reuses the triangles of a shape built before
// (path_cache.rs), but while the camera moves the shapes differ by fractions of a pixel and are
// built again; cache them per zoom step as ink.rs does if a bench asks for it.
pub(crate) fn paint(
    curves: &[(Curve, Hsla)],
    dots: &[Point],
    dot: Hsla,
    camera: &Camera,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let z = camera.zoom();
    let seen = Rect::new(
        camera.origin.x,
        camera.origin.y,
        f32::from(bounds.size.width) / z,
        f32::from(bounds.size.height) / z,
    );
    let at = |p: Point| {
        let s = camera.to_screen(p);
        point(bounds.origin.x + px(s.x), bounds.origin.y + px(s.y))
    };
    for (curve, color) in curves {
        if !curve.bounds().intersects(&seen) {
            continue;
        }
        let mut path = PathBuilder::stroke(px(width(z)));
        path.move_to(at(curve.from));
        path.cubic_bezier_to(at(curve.to), at(curve.c1), at(curve.c2));
        if let Ok(path) = path.build() {
            window.paint_path(path, *color);
        }
    }
    let r = px(dot_radius(z));
    for p in dots.iter().filter(|p| seen.contains(**p)) {
        let c = at(*p);
        let square = Bounds::new(point(c.x - r, c.y - r), size(r * 2.0, r * 2.0));
        window.paint_quad(fill(square, dot).corner_radii(r));
    }
}
```

- [ ] **Step 5: Frames while a dot travels in view**

`crates/templum/src/workspace/mod.rs`: `Workspace` gains, after `next_wait`:

```rust
    /// Draws the next frame while a dot travels along a wire in view (`wires::DOT_FRAME`).
    _dot_frame: Task<()>,
```

and the constructor `_dot_frame: Task::ready(()),`.

`crates/templum/src/workspace/render.rs`, after `let wires = self.wire_curves();` (line 438):

```rust
        // A dot on each wire along which a task is handed over: still when motion is reduced, else
        // drawn again about 30 times a second while one is in view (never at the display's rate).
        let still = cx.reduce_motion();
        let dots = self.wire_dots(now, still);
        let zoom = camera.zoom();
        let in_view = Rect::new(camera.origin.x, camera.origin.y, vp.w / zoom, vp.h / zoom);
        if !still && dots.iter().any(|p| in_view.contains(*p)) {
            self._dot_frame = cx.spawn(async move |ws, cx| {
                cx.background_executor().timer(crate::wires::DOT_FRAME).await;
                ws.update(cx, |_, cx| cx.notify()).ok();
            });
        }
```

and the layer's paint (line 447) becomes `crate::wires::paint(&wires, &dots, accent, &camera, bounds, window);`
(`accent` is the `let accent = self.ui.accent;` just above; `dot` there is the grid's dot color, another thing).

- [ ] **Step 6: Run the tests**

Run: `cargo test -p templum -- dot wires::`
Expected: PASS (the new tests and the wire tests as before).

- [ ] **Step 7: Commit**

```bash
git add crates/templum/src
git commit -m "feat(connections): show a dot travelling along the wire while a task is handed over" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

### Task 7: A worktree for a new branch (`templum-git`)

**Files:**
- Modify: `crates/templum-git/src/repo.rs` (`Repo::add_worktree` after `ignore` at `:421-445`; `worktree_name_ok`
  and `add_line` near `root_of` at `:1338`)
- Test: `crates/templum-git/tests/repo.rs`

**Interfaces:**
- Produces: `templum_git::Repo::add_worktree(&self, name: &str) -> Result<PathBuf, GitError>`: the new worktree's
  folder, `<main working tree>/.worktrees/<name>`, on the new branch `name` from this tree's HEAD; a refused name
  is a `GitError` whose `first_line()` starts "fatal: '<name>' can't name a worktree".

- [ ] **Step 1: Write the failing tests**

`crates/templum-git/tests/repo.rs`, after `ignore_appends_a_line_to_gitignore`:

```rust
/// `fresh()` with one commit: a branch can start from HEAD.
fn committed() -> (tempfile::TempDir, Repo) {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("First", false).unwrap();
    (dir, repo)
}

#[test]
fn a_worktree_gets_a_new_branch_under_dot_worktrees_kept_out_of_status() {
    let (dir, repo) = committed();
    let main = dir.path().canonicalize().unwrap();
    let path = repo.add_worktree("fix/login").unwrap();
    assert_eq!(
        path.canonicalize().unwrap(),
        main.join(".worktrees").join("fix").join("login")
    );
    assert!(path.join("a.txt").is_file());
    let made = Repo::discover("git", &path).unwrap().expect("a worktree");
    assert_eq!(made.status().unwrap().branch.as_deref(), Some("fix/login"));
    // From that worktree, the next one still goes under the main tree's .worktrees.
    let docs = made.add_worktree("docs").unwrap();
    assert_eq!(docs.canonicalize().unwrap(), main.join(".worktrees").join("docs"));
    let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
    assert_eq!(
        exclude.lines().filter(|l| *l == ".worktrees/").count(),
        1,
        "added once: {exclude}"
    );
    assert!(repo.status().unwrap().entries.is_empty(), "git status doesn't list them");
}

#[test]
fn a_worktree_name_that_cant_be_a_branch_or_is_taken_is_refused() {
    let (dir, repo) = committed();
    for bad in [
        "", "-f", "../out", "a/../b", ".hidden", "a b", "x.lock", "a//b", "end/", "dot.", "é",
    ] {
        let err = repo.add_worktree(bad).unwrap_err();
        assert!(err.first_line().contains("can't name a worktree"), "{bad:?}: {err}");
    }
    assert!(!dir.path().join(".worktrees").exists(), "nothing made");
    repo.add_worktree("api").unwrap();
    for taken in ["api", "main"] {
        let err = repo.add_worktree(taken).unwrap_err();
        assert!(err.first_line().contains("already exists"), "{taken}: {err}");
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-git worktree`
Expected: compile error: no method `add_worktree` on `Repo`.

- [ ] **Step 3: Make the worktree**

`crates/templum-git/src/repo.rs`, after `ignore`:

```rust
    /// A worktree for a new branch `name`, from HEAD, at `.worktrees/<name>` in the repository's
    /// main working tree (also when this is a linked worktree). `.worktrees/` goes once into the
    /// repository's `info/exclude`, so `git status` never lists it. Its folder.
    pub fn add_worktree(&self, name: &str) -> Result<PathBuf, GitError> {
        let refused = |why: String| GitError {
            command: shown(&self.program, &["worktree", "add", "-b", name]),
            code: None,
            stderr: format!("fatal: {why}"),
            missing: false,
        };
        if !worktree_name_ok(name) {
            return Err(refused(format!(
                "'{name}' can't name a worktree: use letters, digits, '-', '_', '.' and '/'"
            )));
        }
        // The main working tree is the list's first; a bare repository has none.
        let list = self.text(&["worktree", "list", "--porcelain"])?;
        let mut first = list.split("\n\n").next().unwrap_or_default().lines();
        let main = first
            .next()
            .and_then(|l| l.strip_prefix("worktree "))
            .map(PathBuf::from);
        let Some(main) = main.filter(|_| !first.any(|l| l == "bare")) else {
            return Err(refused(
                "a bare repository has no working tree to branch from".into(),
            ));
        };
        let common = self.text(&["rev-parse", "--git-common-dir"])?;
        let exclude = self.root.join(common.trim()).join("info").join("exclude");
        add_line(&exclude, ".worktrees/")
            .map_err(|e| refused(format!("couldn't write {}: {e}", exclude.display())))?;
        let path = main.join(".worktrees").join(name);
        let dest = path.to_string_lossy().into_owned();
        self.run(&["worktree", "add", "-q", "-b", name, &dest])?;
        Ok(path)
    }
```

and, before `root_of`:

```rust
/// `name` can name a branch and a folder under `.worktrees`: letters, digits, `-`, `_`, `.` and
/// `/` between its parts; no part empty, starting or ending with `.`; no `..`; no `.lock` at the
/// end; not starting with `-` (git would take it for an option). Git refuses whatever else a
/// branch can't be.
fn worktree_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('-')
        && !name.contains("..")
        && !name.ends_with(".lock")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
        && name
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with('.'))
}

/// Adds `line` to the file at `path` (made, with its folder, when missing) unless one of its
/// lines already is that.
fn add_line(path: &Path, line: &str) -> io::Result<()> {
    let mut text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if text.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
    text.push('\n');
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(path, text)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-git`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-git
git commit -m "feat(git): add a worktree for a new branch under .worktrees, kept out of git status" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

### Task 8: An agent opens another agent beside it (`spawn_agent`)

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (`Spawn`, `Call::SpawnAgent`, `list`, `parse`, tests)
- Modify: `crates/templum-mcp/src/cli.rs` (`spawn`, test)
- Create: `crates/templum/src/workspace/spawn.rs`
- Modify: `crates/templum/src/workspace/mod.rs` (`mod spawn;` after `mod spaces;`; `Prompt`; `Pane::prompt`)
- Modify: `crates/templum/src/workspace/blocks.rs:286-318` (`start_agent` split out of `launch_agent`), `:395-407`
  (the `Pane` it builds)
- Modify: `crates/templum/src/workspace/delegate.rs` (`busy` counts a prompt not typed yet)
- Modify: `crates/templum/src/workspace/bus.rs` (`Outcome::Later`; `serve`; `call`'s arm; `Started` types a
  Claude Code's prompt)
- Create: `crates/templum/src/workspace/tests/spawn.rs`; register `#[cfg(unix)] mod spawn;` in `tests/mod.rs` after
  `mod shortcuts;`

**Interfaces:**
- Consumes: Task 3's `in_space`; Task 5's `send_task`, `Handed`, `pane_mut`, `busy`; Task 7's `Repo::add_worktree`.
- Produces (templum-mcp): `pub struct tools::Spawn { pub agent: String, pub prompt: Option<String>, pub folder:
  Option<String>, pub worktree: Option<String> }` (`Clone, Debug, PartialEq`); `Call::SpawnAgent(Spawn)`.
- Produces (app): `pub(crate) struct Prompt { pub text: String, pub screen: Option<String> }`; `Pane::prompt:
  Option<Prompt>`; `Outcome::Later(Task<Reply>)` (a job that answers when done); `Workspace::start_agent(&mut self,
  entry: Option<&AgentEntry>, command: &str, folder: &Path, rect: Rect, window, cx) -> Option<BlockId>` (no
  keyboard change); `spawn_agent(&mut self, from: &str, spawn: tools::Spawn, window, cx) -> Outcome`;
  `spawn::READY_LOOK: Duration` (1 s).

- [ ] **Step 1: Write the failing tool and verb tests**

`crates/templum-mcp/src/tools.rs`: the examples gain, after `wait_for_idle`'s:

```rust
            (
                "spawn_agent",
                json!({ "agent": "codex", "prompt": "fix the tests", "folder": "../api", "worktree": "fix-tests" }),
            ),
```

and `the_tools_that_wait_have_their_defaults_and_limits` gains:

```rust
        assert_eq!(
            parse("spawn_agent", &json!({ "agent": "claude" })),
            Ok(Call::SpawnAgent(Spawn {
                agent: "claude".into(),
                prompt: None,
                folder: None,
                worktree: None
            }))
        );
        assert!(parse("spawn_agent", &json!({ "agent": "claude", "prompt": 3 })).is_err());
```

`crates/templum-mcp/src/cli.rs`'s `delegate_reply_and_wait_send_their_calls` gains, in its table:

```rust
            (
                &["spawn", "codex", "--worktree", "fix-tests", "--prompt", "fix the tests"][..],
                json!({ "name": "spawn_agent", "arguments": { "agent": "codex", "worktree": "fix-tests", "prompt": "fix the tests" } }),
            ),
```

and after the loop `assert_eq!(run_with(&["spawn", "codex", "--folder"], &env(&bus), "").0, 2, "no folder");`.

Run: `cargo test -p templum-mcp` → FAIL.

- [ ] **Step 2: The tool and the verb**

`crates/templum-mcp/src/tools.rs`, after `Call`:

```rust
/// What `spawn_agent` opens: the agent (an id of Settings ▸ Agents), its prompt, its folder (the
/// caller's when left out), and the worktree it makes first.
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub agent: String,
    pub prompt: Option<String>,
    pub folder: Option<String>,
    pub worktree: Option<String>,
}
```

`Call` gains `SpawnAgent(Spawn),`; `list`, after `wait_for_idle`'s entry:

```rust
        {
            "name": "spawn_agent",
            "description": "Opens a new agent block beside yours, connected to you: in your folder, the one you \
                            name, or a new Git worktree of that folder's repository (.worktrees/<worktree>, on a \
                            new branch of that name). A prompt is typed once the agent is ready, as a task you \
                            handed it: wait_for_idle gets its answer. Says the new block's id and folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agent": {
                        "type": "string",
                        "description": "Which agent: claude, codex, gemini, … (an id of Settings ▸ Agents).",
                    },
                    "prompt": { "type": "string" },
                    "folder": {
                        "type": "string",
                        "description": "Where it starts, relative to your folder; yours when left out.",
                    },
                    "worktree": {
                        "type": "string",
                        "description": "A new branch's and worktree's name: letters, digits, - _ . and /.",
                    },
                },
                "required": ["agent"],
            },
        },
```

`parse`, after `seconds`:

```rust
    let optional = |key: &str| match &args[key] {
        Value::Null => Ok(None),
        value => value
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| format!("\"{key}\" is a string")),
    };
```

and after `wait_for_idle`'s arm:

```rust
        "spawn_agent" => Call::SpawnAgent(Spawn {
            agent: text("agent")?,
            prompt: optional("prompt")?,
            folder: optional("folder")?,
            worktree: optional("worktree")?,
        }),
```

`crates/templum-mcp/src/cli.rs`: `VERBS` gains `"spawn"`; after `WAIT`:
`const SPAWN: &str = "spawn <agent> [--folder <folder>] [--worktree <name>] [--prompt <text>] [--json]";`;
`call_of` gains, after the `wait` arms:

```rust
        ("spawn", [agent, more @ ..]) => {
            let mut arguments = json!({ "agent": agent });
            let mut more = more.iter();
            while let Some(flag) = more.next() {
                let key = match *flag {
                    "--folder" => "folder",
                    "--worktree" => "worktree",
                    "--prompt" => "prompt",
                    _ => return Err(SPAWN),
                };
                arguments[key] = json!(more.next().ok_or(SPAWN)?);
            }
            ("spawn_agent", arguments)
        }
```

and with the usage errors `("spawn", _) => return Err(SPAWN),`.

Run: `cargo test -p templum-mcp` → PASS.

- [ ] **Step 3: Write the failing app tests**

`crates/templum/src/workspace/tests/spawn.rs`:

```rust
//! `spawn_agent`: an agent opened beside the caller (docs/specs/connections.md, Spawning). `cat`
//! stands in for the agents: every Unix has it, and it echoes what it is typed.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::json;

use super::bus::{after, answered, call, call_later, event, me, result_of, screen_with};
use super::*;

/// The startup terminal at (40, 100), 400 × 300, the camera at 100% on the origin, and Claude Code
/// and Codex started as `cat`.
fn spawner(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, BlockId) {
    let (handle, ws) = open(cx);
    let mine = first_id(&ws, cx);
    ws.update(cx, |ws, _| {
        ws.space.pristine = false;
        ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), 1.0);
        ws.space.canvas.get_mut(mine).unwrap().rect = Rect::new(40.0, 100.0, 400.0, 300.0);
        ws.space.focused = Some(mine);
        for agent in &mut ws.agents {
            if agent.id == "claude" || agent.id == "codex" {
                agent.command = "cat".into();
            }
        }
    });
    (handle, ws, mine)
}

/// The block spawned beside `mine`: the other end of its one wire.
fn spawned(ws: &Entity<Workspace>, cx: &mut TestAppContext, mine: BlockId) -> BlockId {
    ws.read_with(cx, |ws, _| {
        let wire = ws.space.wires.iter().find(|w| w.touches(mine)).expect("wired to the caller");
        if wire.a == mine { wire.b } else { wire.a }
    })
}

/// The answer that comes on `answers` within ten seconds, background jobs running meanwhile.
fn answer_soon(cx: &mut TestAppContext, answers: &Receiver<String>) -> (bool, String) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if let Ok(line) = answers.try_recv() {
            return result_of(&line);
        }
        assert!(Instant::now() < deadline, "no answer");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The Enter a typed prompt waits for.
fn enter(cx: &mut TestAppContext) {
    after(cx, crate::workspace::delegate::ENTER_AFTER.as_millis() as u64);
}

#[gpui_kit::test]
fn spawn_agent_opens_a_wired_agent_beside_the_caller(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path() });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(!failed, "{text}");
    let new = spawned(&ws, cx, mine);
    let (_, uid) = me(&ws, cx, new);
    assert!(
        text.contains(&uid) && text.contains(&folder.path().display().to_string()),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        let rect = ws.space.canvas.get(new).unwrap().rect;
        assert!(rect.x > 440.0 && rect.y == 100.0, "beside the caller: {rect:?}");
        assert_eq!(ws.space.focused, Some(mine), "the keyboard stays");
        assert_eq!(
            ws.space.terminal(new).unwrap().seen.agent.as_deref(),
            Some("codex")
        );
    });
    assert_eq!(folder_of(&ws, new, cx), folder.path().canonicalize().unwrap());
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", json!({ "agent": "nope" }));
    assert!(failed && text.contains("no agent nope"), "{text}");
    let nowhere = json!({ "agent": "codex", "folder": "/no/such/folder" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", nowhere);
    assert!(failed && text.contains("isn't a folder"), "{text}");
}

#[gpui_kit::test]
fn a_claude_codes_prompt_is_typed_once_it_started(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(!failed && text.ends_with("Its prompt is typed once it is ready."), "{text}");
    let new = spawned(&ws, cx, mine);
    let (theirs, uid) = me(&ws, cx, new);
    // Handed over by the caller already: wait_for_idle waits for its answer.
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", json!({ "block": uid }));
    // Not typed before Claude Code says it started: it may be asking whether to trust the folder.
    after(cx, 5000);
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(new).unwrap().prompt.is_some())
    });
    event(handle, &ws, cx, &theirs, json!({ "event": "started" }));
    enter(cx);
    screen_with(&ws, cx, new, "say hisay hi");
    assert!(waiting.try_recv().is_err(), "it works on it");
    event(handle, &ws, cx, &theirs, json!({ "event": "stopped", "message": "hi" }));
    assert_eq!(answered(&waiting), (false, "hi".to_string()));
}

#[gpui_kit::test]
fn another_agents_prompt_waits_for_its_process_and_a_still_screen(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
    let waiting = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().prompt.is_some())
    };
    // No snapshot found its process yet: nothing typed, however still its screen.
    for _ in 0..3 {
        after(cx, look);
    }
    assert!(waiting(cx));
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(new).unwrap().seen.pid = Some(1)
    });
    let mut looks = 0;
    while waiting(cx) {
        assert!(looks < 10, "never typed");
        // What the shell prints lands between two looks.
        std::thread::sleep(Duration::from_millis(100));
        after(cx, look);
        looks += 1;
    }
    assert!(looks >= 2, "typed once two looks saw the same screen");
    enter(cx);
    screen_with(&ws, cx, new, "say hisay hi");
}

#[gpui_kit::test]
fn spawn_agent_makes_its_worktree_first(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let repo = committed_repo();
    let spawn = json!({ "agent": "codex", "folder": repo.path(), "worktree": "fix-login" });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "spawn_agent", spawn);
    let (failed, text) = answer_soon(cx, &answers);
    let made = repo
        .path()
        .canonicalize()
        .unwrap()
        .join(".worktrees")
        .join("fix-login");
    assert!(!failed && made.is_dir(), "{text}");
    let new = spawned(&ws, cx, mine);
    assert_eq!(folder_of(&ws, new, cx), made);
    let plain = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": plain.path(), "worktree": "x" });
    let answers = call_later(handle, &ws, cx, &terminal, 2, "spawn_agent", spawn);
    let (failed, text) = answer_soon(cx, &answers);
    assert!(failed && text.contains("isn't in a Git repository"), "{text}");
}
```

Register in `crates/templum/src/workspace/tests/mod.rs`, after `mod shortcuts;`:

```rust
// `cat` stands in for the agents.
#[cfg(unix)]
mod spawn;
```

Run: `cargo test -p templum spawn::`
Expected: compile errors: no `Call::SpawnAgent` arm in `call`, no field `prompt` on `Pane`, no module `spawn`.

- [ ] **Step 4: A prompt that waits for its agent, and `start_agent`**

`crates/templum/src/workspace/mod.rs`, after `Handed`:

```rust
/// A prompt `spawn_agent` types into its agent once the agent is ready
/// (docs/specs/connections.md, Spawning).
pub(crate) struct Prompt {
    pub text: String,
    /// The agent's screen at the last look, once its process was seen: the same at the next look,
    /// the agent is ready.
    pub screen: Option<String>,
}
```

`Pane` gains, after `answer`:

```rust
    /// The prompt `spawn_agent` types once its agent is ready.
    pub prompt: Option<Prompt>,
```

`crates/templum/src/workspace/blocks.rs`: the `Pane` built at lines 395-407 gains `prompt: None,`; `launch_agent`
(lines 286-318) keeps its checks and its focus, and the rest becomes `start_agent`:

```rust
    pub(crate) fn launch_agent(
        &mut self,
        command: &str,
        folder: &std::path::Path,
        at: Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BlockId> {
        if !folder.is_dir() {
            let message = format!("{} isn't a folder", folder.display());
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
            return None;
        }
        self.space.pristine = false;
        let size = chrome::block_size(AGENT_COLS, AGENT_ROWS, self.cell.size);
        let rect = Rect::new(at.x, at.y, size.w, size.h);
        let entry = self.agents.iter().find(|a| a.command == command).cloned();
        let id = self.start_agent(entry.as_ref(), command, folder, rect, window, cx)?;
        window.focus(&self.focus_handle, cx);
        Some(id)
    }

    /// A terminal at `rect` in `folder` with the agent `entry`'s command (`command` for one
    /// Templum doesn't know) typed into its shell. It is focused on the canvas; the keyboard stays
    /// where it is. The folder becomes this workspace's newest recent one.
    pub(super) fn start_agent(
        &mut self,
        entry: Option<&AgentEntry>,
        command: &str,
        folder: &Path,
        rect: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BlockId> {
        let id = self.spawn_block(rect, Some(folder.to_path_buf()), window, cx)?;
        let command = entry.map_or_else(
            || command.to_string(),
            |a| a.start_command(&uuid::Uuid::new_v4().to_string()),
        );
        // The shell reads it once it is ready.
        let line = format!("{}\r", self.typed(entry, &command));
        // Known at once (Shift+Enter, closing), not at the next snapshot.
        let agent = entry.map(|a| a.id.clone());
        let pane = self.space.terminal_mut(id).expect("just spawned");
        pane.term.write(line.into_bytes());
        pane.seen.agent = agent;
        remember(&mut self.space.recent, folder.to_path_buf());
        Some(id)
    }
```

(blocks.rs imports `use crate::agents::AgentEntry;`.) In `crates/templum/src/workspace/delegate.rs`, `busy` also
counts a prompt not typed yet:

```rust
    /// Its agent is at work: on a task, on a turn, on a question to its person, or about to get
    /// its prompt.
    pub fn busy(&self, now: Instant) -> bool {
        self.task(now).is_some()
            || self.prompt.is_some()
            || matches!(
                self.state,
                Some(AgentState::Working | AgentState::NeedsYou(_))
            )
    }
```

- [ ] **Step 5: Spawning**

Create `crates/templum/src/workspace/spawn.rs`:

```rust
//! `spawn_agent` (docs/specs/connections.md, Spawning): a new agent block beside the caller, wired
//! to it, in its folder, another, or a new Git worktree; its prompt typed once it is ready.

use std::path::Path;
use std::time::{Duration, Instant};

use gpui_kit::{Context, Window};
use templum_canvas::chrome;
use templum_mcp::{Reply, tools};
use templum_terminal::Agent;

use super::bus::Outcome;
use super::{AGENT_COLS, AGENT_ROWS, Handed, Prompt, Workspace};
use crate::agents::AgentEntry;

/// How often a spawned agent's readiness for its prompt is looked at.
pub(super) const READY_LOOK: Duration = Duration::from_secs(1);
/// The lines of an agent's screen compared to tell that it is still.
const STILL_LINES: usize = 100;

impl Workspace {
    /// `spawn_agent` from the terminal `from`. With a worktree, git makes it on the background
    /// executor, and the call is answered once the agent opened there.
    pub(super) fn spawn_agent(
        &mut self,
        from: &str,
        spawn: tools::Spawn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let tools::Spawn {
            agent,
            prompt,
            folder,
            worktree,
        } = spawn;
        let Some(entry) = self.agents.iter().find(|a| a.id == agent).cloned() else {
            let ids: Vec<&str> = self.agents.iter().map(|a| a.id.as_str()).collect();
            return Outcome::Now(Reply::err(format!(
                "There's no agent {agent}. These are: {}.",
                ids.join(", ")
            )));
        };
        if !templum_terminal::installed(entry.program()) {
            return Outcome::Now(Reply::err(format!(
                "{} isn't installed on this computer ({} wasn't found).",
                entry.title,
                entry.program()
            )));
        }
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let here = space.blocks[&me]
            .folder()
            .or_else(|| space.terminal(me)?.seen.folder.clone());
        let folder = match (folder, here) {
            (Some(named), here) => {
                let named = crate::launcher::expand(&named);
                match here {
                    Some(here) if named.is_relative() => here.join(named),
                    _ => named,
                }
            }
            (None, Some(here)) => here,
            (None, None) => {
                return Outcome::Now(Reply::err(
                    "Templum doesn't know your folder yet: name one with folder.",
                ));
            }
        };
        if !folder.is_dir() {
            return Outcome::Now(Reply::err(format!("{} isn't a folder.", folder.display())));
        }
        let Some(name) = worktree else {
            return Outcome::Now(self.open_agent(from, &entry, &folder, prompt, window, cx));
        };
        let (program, from) = (self.config.git.program().to_string(), from.to_string());
        Outcome::Later(cx.spawn_in(window, async move |ws, cx| {
            let made = cx
                .background_executor()
                .spawn(async move {
                    match templum_git::Repo::discover(&program, &folder) {
                        Ok(Some(repo)) => repo
                            .add_worktree(&name)
                            .map_err(|e| format!("Couldn't make the worktree: {}", e.first_line())),
                        Ok(None) => Err(format!(
                            "{} isn't in a Git repository: a worktree needs one.",
                            folder.display()
                        )),
                        Err(e) => Err(format!("Couldn't run git: {}", e.first_line())),
                    }
                })
                .await;
            match made {
                Ok(path) => ws
                    .update_in(cx, |ws, window, cx| {
                        ws.open_agent(&from, &entry, &path, prompt, window, cx)
                    })
                    .unwrap_or_else(|_| Reply::err("Templum closed.")),
                Err(why) => Reply::err(why),
            }
        }))
    }

    /// Opens `entry` in `folder` beside the caller (terminal `from`), wired to it: one Ctrl+Z
    /// closes both. The keyboard, the selection and the camera stay. `prompt` is typed once the
    /// agent is ready, as a task the caller handed over.
    fn open_agent(
        &mut self,
        from: &str,
        entry: &AgentEntry,
        folder: &Path,
        prompt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let Some((space_id, me)) = self.caller(from) else {
            return Reply::err("Your block was closed before the agent could open.");
        };
        let opened = self
            .in_space(&space_id, |ws| {
                let anchor = ws.space.home(ws.space.canvas.get(me)?);
                let size = chrome::block_size(AGENT_COLS, AGENT_ROWS, ws.cell.size);
                let rect = ws.space.canvas.place_beside(anchor, size);
                let (focused, selected) = (ws.space.focused, ws.space.selected.clone());
                let id = ws.start_agent(Some(entry), &entry.command, folder, rect, window, cx)?;
                (ws.space.focused, ws.space.selected) = (focused, selected);
                ws.space.add_wire(me, id);
                let terminal = ws.space.terminal(id)?.term.id().to_string();
                Some((ws.space.uids.get(&id)?.clone(), terminal))
            })
            .flatten();
        let Some((uid, terminal)) = opened else {
            return Reply::err(format!(
                "Couldn't open {} in {}.",
                entry.title,
                folder.display()
            ));
        };
        let mut said = format!(
            "Opened {} as {uid} in {}, connected to you.",
            entry.title,
            folder.display()
        );
        if let Some(text) = prompt {
            let now = Instant::now();
            if let Some(pane) = self.pane_mut(&terminal) {
                pane.prompt = Some(Prompt { text, screen: None });
                pane.handed = Some(Handed {
                    by: from.to_string(),
                    since: now,
                    until: now + Duration::from_secs(tools::TASK_WAIT),
                });
            }
            // Claude Code says when it started, after asking whether to trust the folder (a typed
            // Enter would answer that); the others are looked at.
            if entry.builtin != Some(Agent::CLAUDE) {
                self.watch_ready(terminal, cx);
            }
            said.push_str(" Its prompt is typed once it is ready.");
        }
        cx.notify();
        Reply::ok(said)
    }

    /// Looks, every READY_LOOK, whether the agent in `terminal` is ready for its prompt, until it
    /// is typed or its block is gone.
    fn watch_ready(&self, terminal: String, cx: &mut Context<Self>) {
        // ponytail: an agent the snapshots never recognize keeps this looking once a second until
        // its block closes; give up after a while if that ever matters.
        cx.spawn(async move |ws, cx| {
            loop {
                cx.background_executor().timer(READY_LOOK).await;
                let looking = ws
                    .update(cx, |ws, cx| ws.look_ready(&terminal, cx))
                    .unwrap_or(false);
                if !looking {
                    break;
                }
            }
        })
        .detach();
    }

    /// One look: once a snapshot saw the agent's process and its screen is as it was a look ago,
    /// its prompt is typed. False once there is nothing more to look for.
    fn look_ready(&mut self, terminal: &str, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.pane_mut(terminal) else {
            return false;
        };
        let screen = pane
            .seen
            .pid
            .map(|_| pane.term.shared().lines(STILL_LINES).join("\n"));
        let Some(prompt) = pane.prompt.as_mut() else {
            return false;
        };
        if screen.is_none() || prompt.screen != screen {
            prompt.screen = screen;
            return true;
        }
        let text = pane.prompt.take().map(|p| p.text).unwrap_or_default();
        self.send_task(terminal, &text, cx);
        false
    }
}
```

`crates/templum/src/workspace/mod.rs`: `mod spawn;` after `mod spaces;`.

- [ ] **Step 6: Calls answered by a job, and a Claude Code's start**

`crates/templum/src/workspace/bus.rs`: `Outcome` gains:

```rust
    /// A job answers it when it is done (`spawn_agent` making a worktree).
    Later(Task<Reply>),
```

`serve`'s `match outcome` gains:

```rust
                                Outcome::Later(job) => {
                                    cx.spawn(async move |_, _| {
                                        let done = job.await;
                                        let _ = reply.send(tools::tool_result(&id, &done));
                                    })
                                    .detach();
                                    return;
                                }
```

`call` gains, before `Call::AskUser`:
`Call::SpawnAgent(spawn) => return self.spawn_agent(terminal, spawn, window, cx),`.

`agent_event`'s `Started` arm becomes:

```rust
            AgentEvent::Started => {
                pane.state = Some(AgentState::Idle);
                // A Claude Code spawn_agent opened is ready for its prompt: SessionStart comes after
                // its question about trusting the folder, which a typed Enter would answer.
                if let Some(prompt) = pane.prompt.take() {
                    self.send_task(terminal, &prompt.text, cx);
                }
            }
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -p templum -- spawn:: agents:: delegate::`
Expected: PASS (the new tests; `launch_agent`'s tests in `agents.rs` as before).

- [ ] **Step 8: Commit**

```bash
git add crates/templum-mcp/src crates/templum/src/workspace
git commit -m "feat(connections): let an agent open another agent beside it, in a new worktree if it asks" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

### Task 9: Docs, minors, the full check and the benches

**Files:**
- Modify: `docs/specs/connections.md`, `docs/ARCHITECTURE.md`, `README.md`, `docs/minors.md`

- [ ] **Step 1: The spec says what was built**

In `docs/specs/connections.md`:
- **Status:** "Batches 1 to 3 implemented; batches 4 and 5 planned".
- **Wires**, the "While a task is handed over" bullet gains: "It takes 1.2 s from one end to the other, then starts
  again; it stays at the wire's middle when the system asks for less motion. With no wire between the two, there is
  no dot (the answer still comes back)."
- **Requests from agents:** "below the Full level, the block's dot shows it waiting for the person;" becomes "the
  block's dot shows it waiting for the person (yellow, "Waiting for you: *the question*"), at every level;"; the
  notification bullet becomes "when the block isn't on screen (another workspace, outside the view, or hidden by grid
  mode), a notification says what the agent asks; clicking it reveals the block;"; after the "1 of 3" bullet: "while
  the balloon's field has the keyboard, no key reaches the terminal;"; the timeout bullet ends "…; closing the block
  withdraws its requests, and so does the agent leaving (its connection to Templum ends) or cancelling the call."; and
  after the request-to-connect bullet: "a request for a block already wired is answered at once; Allow is a step of
  Ctrl+Z, as wiring by hand is;".
- **Delegating:** point 2 ends "… then Enter, a moment (150 ms) after the paste: an agent may take keys that come in
  one burst for a paste, where Enter is a new line. The person sees it arrive."; point 5 gains "…, and into an agent
  already on another agent's task (`wait_for_idle` waits for its end)."; and after point 6: "7. A task stays handed
  over until its answer comes, the agent's session ends, its block closes, or its timeout passes (`delegate`'s,
  600 s unless asked, also without `wait`). 8. `reply` with no call waiting types `[reply from <title> (<id>)]` and the
  text, then Enter, into the agent named; never into a terminal running no agent. 9. A task typed into an agent working
  on a prompt of its own is answered by the end of the next turn: Codex runs typed input as its next turn, so that
  answer may be the turn it was on."
- **Spawning** gains: "The keyboard, the selection and the camera stay where they were; one Ctrl+Z closes the new
  block and its wire. The worktree comes from the repository of the folder the agent starts in, always under its main
  working tree's `.worktrees/`, on a new branch from that folder's HEAD; a name other than letters, digits, `-`, `_`,
  `.` and `/`, or one already a branch, is refused. Claude Code's prompt waits for its `SessionStart`, which comes
  after its question about trusting the folder. The prompt is a task the caller handed over: the dot travels and
  `wait_for_idle` waits for its answer."
- **The MCP server:** the instructions' example becomes "… In this workspace, not connected (ask with connect): 4qm2a
  (note "Ideas"). Name blocks by these ids. You may only read or write your own block and those connected to you;
  connect asks the person for another. list_blocks gives the current list. When a connected kanban …"; the paragraph
  "Until `connect` (batch 3) they say …" becomes "The kanban and note sentence comes with their tools (batch 4)."; the
  Results bullet loses "(until batch 3: …)".
- **Tools:** `connect`'s arguments are "`block`, `timeout` (1800 s)"; after the table's bullets: "- A `timeout` is
  whole seconds from 1 to 3600 (Codex gives up on a tool call after an hour); a longer one is cut to 3600."
- **The `templum` command line:** after the table: "A verb stays connected until its answer: stopped with Ctrl+C,
  what it waited for (a balloon) is withdrawn."
- **The bus**, the protocol bullet gains: "When a connection ends the bus tells Templum (`templum/closed`, with the
  connection's number), and what that client waited for is dropped."
- **Architecture:** the paragraph under the table becomes "Every request reaches the main thread through a channel
  the bus's threads write to, drained by a task as the terminals' wake-ups are; the main thread never touches a socket.
  A request that waits keeps its reply channel in `Workspace::waits` until an event, a button, its block's closing
  (checked each frame), its connection ending or its cancel (`notifications/cancelled`), or its deadline (a timer:
  with nothing on screen moving, no frame comes) answers or drops it." The table gains rows: "`templum/src/workspace/
  waits.rs`, `delegate.rs`, `spawn.rs` (new) | the requests that wait and the balloon's answers; tasks handed over,
  `reply`, `wait_for_idle`; `spawn_agent` | 3", "`templum/src/balloon.rs` (new) | the balloon | 3",
  "`templum-git/src/repo.rs` | `Repo::add_worktree` | 3".
- **Risks:** the first row's answer becomes "The tools that wait say so; a cancelled call (`notifications/cancelled`)
  drops its request (tested); that a late result reaches the agent is checked by hand"; new rows: "A spawned agent's
  prompt typed while it asks a first-run question (Codex trusting a new folder) | Claude Code's waits for its
  `SessionStart`, after that question; the others follow the rule above (process seen, screen still): checked by hand"
  and "A task typed into a Codex at work is answered by the turn it was on | One task at a time per agent; the rest is
  the person's to see in the agent's terminal".

- [ ] **Step 2: Architecture and the README**

- `docs/ARCHITECTURE.md`: the `templum-git` row gains "…, makes a worktree for a new branch under `.worktrees/`
  (`repo.rs`), …"; the bus paragraph (Runtime) gains "Each connection has a number; when one ends the UI hears it
  (`templum/closed`) and drops what it waited for."; in Source layout, `workspace/`'s list after `bus.rs (…)`:
  "`waits.rs` (tool calls that wait: a question, a request to connect, an agent's turn; their deadlines, cancels and
  the balloon's answers), `delegate.rs` (tasks handed between agents: typing them, their answers, `reply`,
  `wait_for_idle`), `spawn.rs` (`spawn_agent`: an agent beside the caller, its worktree, its prompt once it is
  ready)"; a top-level bullet "`balloon.rs`: the balloon on a terminal block whose agent asks the person something";
  the `wires.rs` bullet becomes "painting the wires in the layer under the blocks, with the dot grid, and the dot that
  travels along one while a task is handed over".
- `README.md`, "Agents that talk to the canvas", after "…and send you a notification.": "An agent can ask you a
  question, or to be wired to another block, in a balloon on its block; hand a task to an agent wired to it and get
  its answer back (a dot travels along the wire meanwhile); and open another agent beside itself, in a new Git
  worktree if it likes." and the command-line sentence gains "`templum ask`, `templum delegate`, `templum spawn`".

- [ ] **Step 3: Minors**

Under `## Connections` in `docs/minors.md`, a paragraph "Left by batch 3 (agents together); line numbers are those
of `development` at X." (X: `git rev-parse --short HEAD` once Task 8 is committed) and, each with the line numbers
of that commit:

- [ ] **A spawned Claude Code without Templum's hooks never gets its prompt.** `open_agent`
  (`crates/templum/src/workspace/spawn.rs`) leaves a Claude Code's prompt to its `SessionStart`; with
  `[connections] enabled = false` or a path Templum can't type, it gets no hooks, so `templum spawn claude --prompt …`
  opens it and nothing is typed, and `wait_for_idle` on it waits for its timeout. Fix: look at a Claude Code started
  without hooks as at the others (`typed` knows whether it gave them).
- [ ] **A crashed agent's waits run to their timeout.** `Space::see` (`crates/templum/src/workspace/mod.rs`) forgets a
  crashed agent's state but answers nothing, so a `delegate` with `wait` on it waits up to 10 minutes. A person sees
  the delegating agent stuck. Fix: end its turn (`end_turn`, an error) when a snapshot finds its agent gone.
- [ ] **A task handed to a Codex at work is answered by the turn it was on.** `delegate`
  (`crates/templum/src/workspace/delegate.rs`) allows a task to an agent working on its own prompt, and Codex runs
  typed input as its next turn: its current turn's end answers the task. A person sees the delegating agent get an
  answer to another question. Fix: once Codex says when it starts working, skip the turn that was under way.
- [ ] **The balloon covers the find field.** Both sit at the terminal's top right (`render`,
  `crates/templum/src/balloon.rs`; `find_bar`, `crates/templum/src/block.rs`). A person who opens find while an agent
  asks sees the balloon over the field. Fix: put the balloon under the find field while it is open.
- [ ] **A long question is cut by its block.** The balloon is clipped by the block (`render`,
  `crates/templum/src/balloon.rs`); in a small block a long question loses its end and its buttons. Fix: a scroll in
  the balloon's text, or let it overflow the block.
- [ ] **An agent the snapshots never recognize keeps a look running.** `watch_ready`
  (`crates/templum/src/workspace/spawn.rs`) looks once a second until the prompt is typed or the block closes. A
  person sees nothing (a small cost). Fix: give up after a few minutes.
- [ ] **`send_to_terminal`'s Enter comes in the same burst as its text.** `call`'s `SendToTerminal` arm
  (`crates/templum/src/workspace/bus.rs`) writes `\r` right after the paste; an agent may take it for a new line,
  where `delegate` waits 150 ms. A person sees the text sit unsent in Claude Code's or Codex's prompt. Fix: send its
  Enter with `send_task`'s delay (confirm with the manual check first).

Add any other minor the implementation or a review defers, each with where, what a person sees and a fix.

- [ ] **Step 4: The full check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all green.

- [ ] **Step 5: The benches** (this batch paints the balloon and the dot)

Run, each on its own, from the Templum folder: `cargo build --release -p templum`, then
`cargo run --release -p templum -- --bench` and `cargo run --release -p templum -- --bench-mixed`.
Expected: each prints a JSON line with `p95_ms` ≤ 16.7; keep both lines for the pull request. A line over the bar:
run it once more (other programs load the CPU); over again, stop and report it instead of committing.

- [ ] **Step 6: Commit**

```bash
git add docs README.md
git commit -m "docs(connections): describe agents asking, handing tasks over and spawning agents" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

## Manual checks (Hyprland)

1. **Delegating:** Claude Code and Codex from **AI agent ▸**, wired. Ask Claude Code: "hand <Codex's id> the task
   'say hello in three languages' and wait for its answer". Codex's terminal shows `[from Claude Code (…)]` and the
   task, and it runs (the Enter after the paste is taken as Enter); a dot travels from Claude Code to Codex; Claude
   Code's result is Codex's last message; the dot stops.
2. **A delegated call past 2 minutes:** the same with a task that takes longer ("count to 200, a line a second, then
   say done"). Claude Code moves the call to the background at 2 minutes: note whether the late result still reaches
   it, or whether it cancels the call (then the request is dropped, and `wait_for_idle` gets the answer).
3. **A task to a busy agent:** delegate to Codex while it works on a prompt of yours: it is queued; note which turn's
   end answers it.
4. **A question in the balloon:** ask Claude Code to ask you, with `ask_user`, whether to ship, options Yes and No:
   the balloon on its block's top right; click Yes: Claude Code gets "Yes". A question without options: type in the
   field, press Enter (nothing reaches the terminal). Zoom out below Full: the dot is yellow, "Waiting for you: …".
   Ask from another workspace: a notification; clicking it shows the block with the balloon.
5. **A connect request:** Claude Code asks to connect to a note: "Claude Code wants to connect to …" with Allow and
   Deny; Allow wires them and Ctrl+Z unwires; Deny refuses. Two requests: "1 of 2".
6. **Ctrl+C:** `templum ask "Ready?" --option Yes` in a plain terminal shows the balloon; Ctrl+C takes it away.
7. **`spawn_agent` with a worktree:** from a Claude Code in a Git repository, "spawn codex in a worktree named try-x
   with the prompt 'list the files'": `.worktrees/try-x` exists on branch `try-x`, `git status` in the repository
   doesn't list it, a Codex block opens beside Claude Code, wired, the keyboard where it was; the prompt is typed once
   Codex is ready. Watch for Codex's question about trusting the new folder: write down what the typed prompt does to
   it. The same with `claude`: its prompt waits until its trust question is answered.
8. **`send_to_terminal` with Enter** into Claude Code: does the line send (else confirm the minor)?
9. **The dot far away:** zoom to 25% during a delegation: the dot still shows on the 1 px wire; reduced motion (if
   the desktop sets it): the dot stays at the middle.
