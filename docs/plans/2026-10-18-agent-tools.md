# Agent tools (card 35) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kanban card 35: every agent connected to Templum's MCP server gets `stop_agent`, `wait_for` with a pattern,
`create_block`, note and checklist fixes, forks, roles, `propose_plan`, schedules and the worktree tools, each with a
`templum` verb.

**Architecture:** The tools' schemas, parsing and verbs live in `templum-mcp` (`tools.rs`, `cli.rs`, `ask.rs`), headless.
Each workspace operation the tools need is one `Workspace` method that takes its target, never a caller, in the new
`crates/templum/src/workspace/ops.rs`, which card 43's control server calls too; the tool layer (`bus.rs`,
`delegate.rs`, `content.rs`, `spawn.rs`, `waits.rs`, the new `schedules.rs`) checks wires and openers, calls the
operation and words the answer. Process ending and the pattern watches are `templum-terminal`'s; worktree state is
`templum-git`'s.

**Tech Stack:** Rust 2024; GPUI (gpui-fast 0.1.8) through gpui-kit `=0.7.3` (balloon, menus, tooltips); `regex` 1.13
(already in the tree through gpui) in `templum-terminal`; `sysinfo` (`Process::kill_with(Signal::Term)`); `chrono`
(`Local`, in `templum` already); serde; the headless tests of `workspace/tests/`.

**Spec:** [docs/specs/agent-tools.md](../specs/agent-tools.md): every argument, answer and refusal is written there;
copy its sentences exactly. It builds on [docs/specs/connections.md](../specs/connections.md) (tasks, notices, spawning,
the balloon) and [docs/specs/ask-user.md](../specs/ask-user.md) (the dialog `propose_plan` reuses).

**Shared folder:** another agent may work in this folder. Never `git add -A` or `git add .`: add the task's files by
path. Before adding an operation of the spec's [Shared operations](../specs/agent-tools.md#shared-operations), look for
it on `development` (`git log --oneline -20`, `grep -n <name> crates/templum/src/workspace/*.rs`): card 43 may have
added it, and then the task calls it instead.

## Global Constraints

- Only `crates/templum` uses gpui-kit; `templum-mcp`, `templum-terminal`, `templum-git` stay UI-free and are tested
  headlessly. No new crate in `Cargo.lock`: `regex = "1"` in `templum-terminal/Cargo.toml` resolves to the 1.13.1
  already locked (check `git diff Cargo.lock` shows only the new edge). No `unsafe`.
- Every tool is listed for every caller (`tools::list()`); which blocks it may touch is checked at each call; a
  refusal is `Reply::err` with the spec's sentence. A tool that names a block refuses one not wired with today's
  `target()` sentence.
- `tools::list()` hits `json!`'s recursion limit when it grows: build new schemas in `let` bindings first, as `ask_user`'s
  `choice` and `question` are.
- Timeouts: whole seconds 1 to `tools::MAX_WAIT` (3600); `wait_for` keeps `IDLE_WAIT` (300) as its default.
- Text an agent reads goes through `bus::flat`/`clean_title` when it comes from a program (titles, a terminal's line in
  a schedule's tooltip).
- Every new verb takes `--json` and its usage line says so (`every_verbs_usage_offers_json`).
- Every change from a tool to a note, a checklist, a block or a wire is one step of the Ctrl+Z it already has.
- Each commit passes `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`, on `development`, titled as CLAUDE.md says; `feat` and `fix` titles are written for someone who uses
  Templum.
- Each task that adds a tool also adds its row to connections.md's Tools table and its verb to connections.md's
  command-line table (with "card 35" in the Batch column), in the same commit.
- Batches 1, 3 and 4 end with `cargo run --release -p templum -- --bench` and `--bench-mixed` (p95_ms ≤ 16.7): their
  JSON lines go in that batch's pull request description.

## Review Focus

1. **A watch left behind:** a `wait_for` with a pattern that is answered, times out, is cancelled, or whose connection
   ends must remove its watch from the terminal, or the reader thread keeps lines for good. Task 3 pins it
   (`a_pattern_wait_leaves_no_watch_behind`).
2. **`stop_agent` on macOS, where a terminal's process is `login` and the shell its child,** and on a terminal whose
   agent quit a moment ago: the shell is never signalled, and a second call answers "Nothing runs in …". Task 1 pins
   both (`shells_of`'s unit test, the second `stop_agent` call).
3. **A long or odd line under a watch** (a progress bar's `\r` without `\n`, 1 MB without a newline, invalid UTF-8):
   no panic, the line is cut at 64 KB, and `\r` alone doesn't end a line. Task 2 pins it.
4. **A schedule whose caller closes, or whose target is unwired or moved to another workspace:** the caller's close
   ends its schedules silently; a target no longer reachable is refused at its next time and the caller hears so.
   Task 11 pins both.
5. **Two balloons at once on one block** (a plan, then a merge request, then a question): they wait in turn with "1 of
   3", each answered by its own buttons. Tasks 10 and 15 pin it.

---

## Batch 1: problems already seen

### Task 1: `stop_agent`

**Files:**
- Modify: `crates/templum-terminal/src/terminal.rs` (next to `kill_left`), `crates/templum-terminal/src/lib.rs` (export)
- Create: `crates/templum/src/workspace/ops.rs` (with its module doc: "The workspace operations both MCP servers
  call, each taking its target, never a caller (docs/specs/agent-tools.md#shared-operations).")
- Modify: `crates/templum/src/workspace/mod.rs` (`mod ops;`), `delegate.rs` (the tool), `bus.rs` (dispatch)
- Modify: `crates/templum-mcp/src/tools.rs`, `crates/templum-mcp/src/cli.rs`
- Modify: `docs/specs/connections.md`, `docs/ARCHITECTURE.md` (the `workspace/` list gains `ops.rs`)
- Test: `crates/templum-terminal/tests/spawn.rs`, `crates/templum-mcp/src/tools.rs`, `crates/templum-mcp/src/cli.rs`,
  `crates/templum/src/workspace/tests/spawn.rs`

**Interfaces:**
- Consumes: `templum_terminal::{tree, kill_left, Running}`, `Workspace::end_later(Vec<Running>, cx)` (spaces.rs),
  `Workspace::end_turn(terminal, Reply, cx)`, `target()`, `Pane::{opened_by, handed, prompt, seen, term}`.
- Produces:
  - `templum_terminal::shells_of(process: u32, name: &str, children: &[u32]) -> Vec<u32>` (pure): `[process]`, and its
    first child too when `name` is `login` (macOS); `Terminal::shells(&self) -> Vec<u32>` reads them with sysinfo.
  - `templum_terminal::strays(terminal: &str, shells: &[u32], program: Option<u32>) -> Vec<Running>`: one sysinfo
    refresh (with `environ`); `program` and every process below it, plus every process whose `TEMPLUM_TERMINAL` is
    `terminal` and whose parent is neither one of `shells` nor below them; never one of `shells`.
  - `templum_terminal::terminate(procs: &[Running])`: SIGTERM on Unix (`kill_with(Signal::Term)`), `kill()` where that
    returns None (Windows); the same start-time and zombie checks as `kill_left`.
  - `tools::Call::StopAgent { block: String }`; tool `stop_agent` (description: "Ends the agent, or the command, that
    runs in a terminal you opened (spawn_agent, create_block), and every process it started, background ones
    included; its shell stays. Only the person stops a block you didn't open.").
  - `pub(super) struct Stopped { pub program: String, pub processes: usize }` and
    `pub(super) fn stop_in(&mut self, space: &str, block: BlockId, cx: &mut Context<Self>) -> Result<Stopped, String>`
    in `ops.rs`: the program is `processes().foreground` when it isn't one of `term.shells()` (Unix), else
    `seen.pid` (Windows), else on Windows the shell's first child; `strays(term.id(), &shells, program)`; errs "Nothing
    runs in {uid}: its shell is at its prompt." when that is empty or the shell exited; else `terminate`, then
    `end_later`. `Stopped::program` is `agent_name(pane)`, else the foreground program's name, else empty (orphans
    only).
  - `pub(super) fn stop_agent(&mut self, from: &str, block: &str, cx) -> Reply` in `delegate.rs`: the spec's refusals
    (own block, not opened by the caller, via `target`), ends the caller's own task first as `interrupt` does
    (`handed = None`, `prompt = None`, `end_turn(.., Reply::err("Stopped by you."))`), then `stop_in`.
  - Verb `templum stop <block> [--json]`.

- [ ] **Step 1: Write the failing tests**

In `crates/templum-terminal/tests/spawn.rs`, beside `a_tree_is_ended_even_when_it_ignores_hangup` (Unix):

```rust
#[cfg(unix)]
#[test]
fn strays_are_the_program_what_it_left_in_a_session_of_its_own_and_never_the_shell() {
    let dir = tempfile::tempdir().unwrap();
    let flag = dir.path().join("f");
    // The "shell" runs a job of the person's (sleep 31) and the "program", whose tool shell leaves an orphan in a
    // session of its own (as Claude Code's Bash tool does) that would write a file later.
    let script = format!(
        "sleep 31 & sh -c '(setsid sh -c \"sleep 2 && touch {}\" &); exec sleep 33' & wait",
        flag.display()
    );
    let mut shell = std::process::Command::new("sh")
        .args(["-c", &script])
        .env("TEMPLUM_TERMINAL", "t-1")
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(400));
    let program = child_running(shell.id(), "33"); // the sleep 33, by its arguments, with sysinfo
    let person = child_running(shell.id(), "31");
    let found = templum_terminal::strays("t-1", &[shell.id()], Some(program));
    let pids: Vec<u32> = found.iter().map(|r| r.pid).collect();
    assert!(pids.contains(&program));
    assert!(!pids.contains(&shell.id()) && !pids.contains(&person), "{pids:?}");
    assert!(pids.len() >= 3, "the orphan sh and its sleep: {pids:?}");
    templum_terminal::terminate(&found);
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert!(!flag.exists(), "the orphan wrote its file");
    assert!(process_alive(person), "the person's job stays");
    let _ = shell.kill();
}

#[test]
fn the_shell_is_logins_child_when_the_terminal_runs_login() {
    assert_eq!(templum_terminal::shells_of(10, "login", &[11, 12]), [10, 11]);
    assert_eq!(templum_terminal::shells_of(10, "zsh", &[11]), [10]);
}
```

(`child_running` and `process_alive` are small sysinfo helpers of this test file.)

In `tools.rs`'s tests: `stop_agent` joins `every_tool_listed_parses_its_example` with `json!({ "block": "k9x1d" })`, and
`bad_arguments_say_which` gains `("stop_agent", json!({}), "stop_agent needs \"block\", a string")`.

In `cli.rs`'s `the_control_verbs_send_their_calls`: `["stop", "k9x1d"]` sends `stop_agent` with `{"block":"k9x1d"}`.

In `workspace/tests/spawn.rs` (Unix), using `spawner`, `spawned`, `me`, `call`, `found`:

```rust
#[gpui_kit::test]
fn stop_agent_ends_what_the_caller_opened_and_what_it_left_running(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let dir = tempfile::tempdir().unwrap();
    let flag = dir.path().join("f");
    // Its "agent" (`cat`) comes with a job that would write a file later; once they end, the shell waits in a
    // builtin `read`, at its prompt with nothing below it.
    ws.update(cx, |ws, _| {
        ws.command = Some(jobs(&format!(
            "printf '\\033[?2004h'; (sleep 2 && touch {}) & cat; while read -r x; do :; done",
            flag.display()
        )))
    });
    let spawn = json!({ "agent": "codex", "folder": dir.path(), "prompt": "go" });
    assert!(!call(handle, &ws, cx, &terminal, "spawn_agent", spawn).0);
    let new = spawned(&ws, cx, mine);
    let (_, them) = me(&ws, cx, new);
    in_front(&ws, cx, new, true);
    found(&ws, cx, new);
    let (failed, text) = call(handle, &ws, cx, &terminal, "stop_agent", json!({ "block": them }));
    assert!(!failed, "{text}");
    assert!(text.starts_with(&format!("Stopped {them}: ended ")), "{text}");
    assert!(text.ends_with("Its shell is at its prompt."), "{text}");
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(new).unwrap();
        assert!(pane.handed.is_none() && pane.prompt.is_none(), "its task ended");
        assert!(!pane.term.exited(), "the shell stays");
    });
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert!(!flag.exists());
    let again = call(handle, &ws, cx, &terminal, "stop_agent", json!({ "block": them }));
    assert_eq!(again, (true, format!("Nothing runs in {them}: its shell is at its prompt.")));
}

#[gpui_kit::test]
fn stop_agent_refuses_its_own_block_and_one_it_didnt_open(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let own = call(handle, &ws, cx, &terminal, "stop_agent", json!({ "block": uid }));
    assert_eq!(own, (true, "This block is yours: stop_agent stops another block.".into()));
    // A terminal the person opened, wired by them.
    let dir = tempfile::tempdir().unwrap();
    let other = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let id = ws.terminal_beside(Some(mine), dir.path(), None, window, cx).unwrap();
                assert!(ws.connect(mine, id, cx));
                ws.space.uids[&id].clone()
            })
        })
        .unwrap();
    let theirs = call(handle, &ws, cx, &terminal, "stop_agent", json!({ "block": other }));
    assert_eq!(
        theirs,
        (true, format!("Only the person stops {other}: you didn't open it. interrupt presses Esc in it."))
    );
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-terminal terminate_ends && cargo test -p templum-mcp stop && cargo test -p templum stop_agent`
Expected: FAIL to compile (`terminate`, `Call::StopAgent`, the tool don't exist).

- [ ] **Step 3: Implement** the Interfaces: `terminate`; the tool's schema, parse and verb; `stop_in` in the new
  `ops.rs`; `stop_agent` in `delegate.rs` beside `interrupt`; the `Call::StopAgent` arm in `bus.rs`'s `call`.
  Answer: `Stopped {uid}: ended {program} and {n} processes it started. Its shell is at its prompt.` (`1 process`;
  `and nothing else` for 0). Add the tool's row and verb to connections.md, `ops.rs` to ARCHITECTURE.md's
  `workspace/` list.

- [ ] **Step 4: Run the full check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-terminal/src/terminal.rs crates/templum-terminal/src/lib.rs crates/templum-terminal/tests/spawn.rs \
  crates/templum-mcp/src/tools.rs crates/templum-mcp/src/cli.rs crates/templum/src/workspace/ops.rs \
  crates/templum/src/workspace/mod.rs crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/bus.rs \
  crates/templum/src/workspace/tests/spawn.rs docs/specs/connections.md docs/ARCHITECTURE.md
git commit -m "feat(connections): stop_agent ends an agent you opened and everything it left running"
```

---

### Task 2: Watching a terminal's lines for a pattern

**Files:**
- Modify: `crates/templum-terminal/Cargo.toml` (`regex = "1"`), `src/tap.rs` (`Scanner` keeps the line), `src/terminal.rs`
  (`Shared::watches`, the `Terminal` methods), `src/lib.rs` (exports)
- Test: `crates/templum-terminal/src/tap.rs` (its `mod tests`, with `through`)

**Interfaces:**
- Produces:
  - `pub struct Pattern(regex::Regex)` with `pub fn new(text: &str) -> Result<Pattern, String>`: blank, or over 1000
    characters, or a regex error, is Err with the spec's words (`"pattern" isn't a regular expression: <first line>`);
    built with `RegexBuilder::size_limit(1 << 20)`; `pub fn is_match(&self, line: &str) -> bool`; `pub fn as_str()`.
  - `Terminal::watch(&self, pattern: Pattern) -> u64`, `Terminal::hit(&self, id: u64) -> Option<String>` (the first
    line that matched, once there is one), `Terminal::unwatch(&self, id: u64)`; `Terminal::watching(&self) -> usize`
    (tests).
  - In `Scanner::scan`: while `shared.watches` isn't empty, printable bytes outside escape sequences go to the line being
    kept; `\n` ends it (a `\r` just before it dropped), tests it (`String::from_utf8_lossy`) against every watch with no
    hit yet, and clears it; a lone `\r` (a progress bar) clears the line, as it returns to the line's start; the line is
    cut at 64 KB (bytes past it dropped until the `\n`). Without watches nothing is kept.

- [ ] **Step 1: Write the failing tests** in `tap.rs`'s tests (a `watched(pattern) -> (Arc<Shared>, u64)` helper adds a
  watch to a fresh `shared()` before `through`-style scanning):

```rust
#[test]
fn a_watch_hears_a_line_split_across_reads_without_its_escapes() {
    let (shared, id) = watched("Ready in \\d+ms");
    scan_into(&shared, &[b"\x1b[32mVITE\x1b[0m Rea", b"dy in 312ms\r\n"]);
    assert_eq!(hit(&shared, id).as_deref(), Some("VITE Ready in 312ms"));
}

#[test]
fn a_watch_sees_each_line_once_and_only_after_it_was_set() {
    let shared = shared();
    scan_into(&shared, &[b"ERROR before\n"]);
    let id = add_watch(&shared, "ERROR");
    scan_into(&shared, &[b"ok\nstill ok\n"]);
    assert_eq!(hit(&shared, id), None);
    scan_into(&shared, &[b"ERROR one\nERROR two\n"]);
    assert_eq!(hit(&shared, id).as_deref(), Some("ERROR one"));
}

#[test]
fn a_progress_bar_a_huge_line_and_bad_utf8_never_break_a_watch() {
    let (shared, id) = watched("^done$");
    scan_into(&shared, &[b"50%\r100%\rdone\n"]);
    assert_eq!(hit(&shared, id).as_deref(), Some("done"), "a lone \\r returns to the line's start");
    let (shared, id) = watched("tail");
    let mut huge = vec![b'x'; 1 << 20];
    huge.extend_from_slice(b"tail\n\xff\xfe tail\n");
    scan_into(&shared, &[&huge]);
    assert_eq!(hit(&shared, id).as_deref(), Some("\u{fffd}\u{fffd} tail"), "the long line was cut before its tail");
}

#[test]
fn nothing_is_kept_without_a_watch() {
    let shared = shared();
    scan_into(&shared, &[b"a line\n"]);
    assert!(shared.line_kept().is_empty());
}

#[test]
fn a_pattern_refuses_what_isnt_one() {
    assert!(Pattern::new("ERROR|Traceback").is_ok());
    assert_eq!(Pattern::new(" ").unwrap_err(), "\"pattern\" is blank");
    assert!(Pattern::new("(").unwrap_err().starts_with("\"pattern\" isn't a regular expression: "));
    assert_eq!(Pattern::new(&"a".repeat(1001)).unwrap_err(), "\"pattern\" is longer than 1000 characters");
}
```

(`scan_into`, `add_watch`, `hit` are test helpers over `Scanner::scan` and the `Shared` watch list; `line_kept` is a
`#[cfg(test)]` accessor.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-terminal tap::tests`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** the Interfaces. The watches are a `Mutex<Vec<(u64, Pattern, Option<String>)>>` in
  `Shared`, checked with one `lock` per scanned chunk (an `AtomicBool` "any watch" spares the lock while none is set).

- [ ] **Step 4: Run the full check** (as Task 1). Expected: PASS; `git diff Cargo.lock` adds only `regex` to
  `templum-terminal`'s dependencies.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-terminal/Cargo.toml Cargo.lock crates/templum-terminal/src/tap.rs \
  crates/templum-terminal/src/terminal.rs crates/templum-terminal/src/lib.rs
git commit -m "refactor(terminal): watch a terminal's printed lines for a pattern"
```

---

### Task 3: `wait_for` with a pattern

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (`Event::Output`, `pattern`, `lines`), `crates/templum-mcp/src/cli.rs`
  (`--match`, `--lines`)
- Modify: `crates/templum/src/workspace/waits.rs` (`Waiting::Event` gains the watch; `wait_for`, `look`, the end of a
  wait unwatches), `crates/templum/src/workspace/ops.rs` (`watch_add`)
- Modify: `docs/specs/connections.md` (`wait_for`'s row, `templum wait`'s), `docs/minors.md` (Connections: a watch on
  Windows reads ConPTY's drawing)
- Test: `crates/templum-mcp/src/tools.rs`, `cli.rs`, `crates/templum/src/workspace/tests/delegate.rs` (beside the
  `wait_for` tests)

**Interfaces:**
- Consumes: Task 2's `Pattern`, `Terminal::{watch, hit, unwatch}`.
- Produces:
  - `tools::Event::Output`; `Call::WaitFor { block, event, timeout, pattern: Option<String>, lines: usize }`
    (`lines` 0 unless given, at most `MAX_LINES`); `event` may be left out when `pattern` is given (schema:
    `required: ["block"]`, `"output"` in the enum, `pattern` and `lines` described as the spec says). Parse refusals:
    `wait_for needs "event": idle, needs_you, command_finished, output (with a pattern) or attention`,
    `"pattern" goes with event output`, `"lines" goes with a pattern`.
  - `Waiting::Event { of, uid, event, since, watch: Option<(u64, String)> }` (the watch id and the pattern's text).
  - `pub(super) fn watch_add(&self, terminal: &str, pattern: Pattern) -> Option<u64>` in `ops.rs`.
  - Whatever ends a `Waiting::Event` with a watch (answered, deadline, cancel, connection closed, block closed)
    calls `unwatch`: one place, in `Workspace::answer`'s and the drops' common path in `waits.rs`.

- [ ] **Step 1: Write the failing tests**

`tools.rs`: the example `("wait_for", json!({ "block": "t4q8p", "pattern": "Ready in", "lines": 20 }))` parses to
`event: Event::Output`; `wait_for_takes_the_four_events_it_lists` becomes five (`output` with a pattern); refusals for
`{"block":"t4q8p","event":"idle","pattern":"x"}`, `{"block":"t4q8p","event":"output"}`,
`{"block":"t4q8p","lines":5}`. `cli.rs`: `["wait", "t4q8p", "--match", "ERROR", "--lines", "5"]` sends
`{"block":"t4q8p","pattern":"ERROR","lines":5}`.

`workspace/tests/delegate.rs` (Unix; a wired plain terminal as the `wait_for(command_finished)` tests set one up):

```rust
#[gpui_kit::test]
fn wait_for_a_pattern_answers_on_the_printed_line(cx: &mut TestAppContext) {
    let (handle, ws, me_term, other, uid) = wired_pair(cx); // the setup the command_finished tests use
    let answers = call_later(handle, &ws, cx, &me_term, 7, "wait_for", json!({ "block": uid, "pattern": "Ready in \\d+" }));
    prints(&ws, cx, other, b"starting\r\n  Ready in 312 ms\r\n");
    let (failed, text) = answer_within(&ws, cx, &answers); // the helper the event tests use
    assert!(!failed);
    assert!(text.starts_with(&format!("{uid} printed a line matching \"Ready in \\d+\":\n  Ready in 312 ms\nIts last lines:\n")), "{text}");
}

#[gpui_kit::test]
fn wait_for_a_pattern_with_lines_answers_from_what_is_there(cx: &mut TestAppContext) {
    let (handle, ws, me_term, other, uid) = wired_pair(cx);
    prints(&ws, cx, other, b"Local: http://localhost:5173\r\n");
    screen_with(&ws, cx, other, "localhost:5173");
    let (failed, text) = call(handle, &ws, cx, &me_term, "wait_for",
        json!({ "block": uid, "pattern": "Local:", "lines": 20 }));
    assert!(!failed && text.contains("Local: http://localhost:5173"), "{text}");
    let bad = call(handle, &ws, cx, &me_term, "wait_for", json!({ "block": uid, "pattern": "(" }));
    assert!(bad.0 && bad.1.starts_with("\"pattern\" isn't a regular expression: "), "{bad:?}");
}

#[gpui_kit::test]
fn a_pattern_wait_leaves_no_watch_behind(cx: &mut TestAppContext) {
    let (handle, ws, me_term, other, uid) = wired_pair(cx);
    let watching = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(other).unwrap().term.watching())
    };
    // Its timeout.
    let answers = call_later(handle, &ws, cx, &me_term, 8, "wait_for", json!({ "block": uid, "pattern": "never", "timeout": 1 }));
    assert_eq!(watching(&ws, cx), 1);
    after(cx, 1_100);
    let (failed, text) = answered(&answers);
    assert!(!failed && text.starts_with(&format!("No line matching \"never\" in {uid} within 1 seconds.")), "{text}");
    assert_eq!(watching(&ws, cx), 0);
    // Its connection's end.
    let _answers = call_later(handle, &ws, cx, &me_term, 9, "wait_for", json!({ "block": uid, "pattern": "never" }));
    closed_connection(handle, &ws, cx, 9); // as the tests of `templum/closed` send it
    assert_eq!(watching(&ws, cx), 0);
}
```

(`wired_pair`, `answer_within` and `closed_connection` stand for the setup and helpers the existing `wait_for` and
`closed` tests in this file already use; use those, and make one `pub(super)` if it is private elsewhere.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp wait_for && cargo test -p templum pattern`
Expected: FAIL.

- [ ] **Step 3: Implement.** In `wait_for`, an `Output` event: `Pattern::new` (a refusal is its Err), then with
  `lines > 0` test `pane.term.shared().lines(lines)` oldest first and answer at once on a match; else `watch_add` and
  `Outcome::Wait`, `look_until_it_comes` as for the other events; `look` answers `Event::Output` when `hit` is Some.
  The timeout's text is `No line matching "{pattern}" in {uid} within {seconds} seconds. Its last lines:` and
  `LAST_LINES` lines (`awaited` can't say it: give `Output` its own sentence where the timeout is worded). Update
  connections.md's `wait_for` and `templum wait` rows.

- [ ] **Step 4: Run the full check.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-mcp/src/tools.rs crates/templum-mcp/src/cli.rs crates/templum/src/workspace/waits.rs \
  crates/templum/src/workspace/ops.rs crates/templum/src/workspace/tests/delegate.rs docs/specs/connections.md \
  docs/minors.md
git commit -m "feat(connections): wait_for waits for a line that matches a pattern, as \"Ready in\" or ERROR"
```

---

### Task 4: `create_block`

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs`, `cli.rs`
- Modify: `crates/templum/src/workspace/ops.rs` (`NewBlock`, `Place`, `add_block_in`), `content.rs` (the tool),
  `bus.rs` (dispatch; the instructions' new sentence), `spawn.rs` (`room_near` becomes `pub(super)`)
- Modify: `crates/templum-mcp/src/tools.rs`'s `create_card` description (the findings sentence)
- Modify: `docs/specs/connections.md` (rows; the instructions' quote)
- Test: `crates/templum-mcp/src/tools.rs`, `cli.rs`; `crates/templum/src/workspace/tests/content.rs`

**Interfaces:**
- Consumes: `Space::room_near`, `Workspace::{in_space, spawn_block_with, add_wire via space.add_wire, notes.add,
  checklists.add/rename/add_item, checklists::parse_items}`, the note and checklist block constructors of
  `note_tabs.rs` (`add_note_block`) and `checklist_blocks.rs` (`add_checklist_block`; make them `pub(super)`).
- Produces:
  - `tools::Call::CreateBlock(NewBlockArgs)` with `pub struct NewBlockArgs { pub kind: BlockKind, pub text:
    Option<String>, pub name: Option<String>, pub items: Option<String>, pub folder: Option<String>, pub command:
    Option<String> }` and `pub enum BlockKind { Note, Checklist, Terminal }`; the parse refusals of the spec
    (`kind is note, checklist or terminal`; `"text" is for a note`; `"name" and "items" are for a checklist`;
    `"folder" and "command" are for a terminal`; `"command" is one line of at most 4096 bytes`).
  - In `ops.rs`:
    `pub(crate) enum NewBlock { Note { text: String }, Checklist { name: String, items: Vec<(String, bool)> },
    Terminal { folder: PathBuf, command: Option<String> } }`, `pub(crate) enum Place { Near(BlockId) }` and
    `pub(crate) fn add_block_in(&mut self, space: &str, new: NewBlock, place: Place, opener: Option<String>, window:
    &mut Window, cx: &mut Context<Self>) -> Result<BlockId, String>`: one undo step (the block's add, as
    `open_agent`'s), the keyboard, selection and camera kept (the `kept` tuple of `open_agent`), grid mode's half tile;
    `opener` goes to `Pane::opened_by` (a terminal only); a terminal's command is written with `\r` after it, as
    `start_agent` writes an agent's.
  - `pub(super) fn create_block(&mut self, from: &str, args: tools::NewBlockArgs, window, cx) -> Reply` in
    `content.rs`: resolves the folder as `spawn_agent` does, calls `add_block_in`, wires it to the caller, answers.
  - Verb `templum new note [<text>] | new checklist <name> [--items <text>] | new terminal [--folder <f>] [--command
    <c>] [--json]` (`--folder` made absolute from the verb's folder, as `spawn --folder`).

- [ ] **Step 1: Write the failing tests**

`tools.rs`: examples for each kind; each refusal above. `cli.rs`: the three verbs send their arguments.

`workspace/tests/content.rs`:

```rust
#[gpui_kit::test]
fn create_block_opens_each_kind_beside_the_caller_wired_to_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let mine = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let kept = ws.read_with(cx, |ws, _| (ws.space.focused, ws.space.canvas.camera));
    let (failed, text) = call(handle, &ws, cx, &terminal, "create_block",
        json!({ "kind": "checklist", "name": "Plan", "items": "- [ ] Read\n- [x] Plan" }));
    assert!(!failed && text.starts_with("Opened a checklist as ") && text.ends_with(", connected to you."), "{text}");
    let list = newest_block(&ws, cx); // the block of the caller's newest wire, as spawn.rs's `spawned`
    ws.read_with(cx, |ws, _| {
        let shown = ws.space.blocks[&list].checklist().unwrap().list.unwrap();
        let l = ws.checklists.get(shown).unwrap();
        assert_eq!(l.name_shown(), "Plan");
        assert_eq!(l.items.iter().map(|i| (i.text.as_str(), i.done)).collect::<Vec<_>>(), [("Read", false), ("Plan", true)]);
        assert!(ws.space.wired(mine, list));
        assert_eq!((ws.space.focused, ws.space.canvas.camera), kept, "keyboard and camera stay");
        let (a, b) = (ws.space.canvas.get(mine).unwrap().rect, ws.space.canvas.get(list).unwrap().rect);
        assert!(!a.intersects(&b), "beside, covering nothing");
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "create_block", json!({ "kind": "note", "text": "notes" }));
    assert!(!failed, "{text}");
    let dir = tempfile::tempdir().unwrap();
    let (failed, text) = call(handle, &ws, cx, &terminal, "create_block",
        json!({ "kind": "terminal", "folder": dir.path(), "command": "echo hello-from-create" }));
    assert!(!failed && text.contains("running \"echo hello-from-create\""), "{text}");
    let term = newest_block(&ws, cx);
    screen_with(&ws, cx, term, "hello-from-create");
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(term).unwrap();
        assert_eq!(pane.opened_by.as_ref(), ws.space.uids.get(&mine), "the caller may stop and close it");
    });
    // One Ctrl+Z closes the newest block and its wire.
    cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))).unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.space.canvas.get(term).is_none() && !ws.space.wired(mine, term)));
}

#[gpui_kit::test]
fn create_block_refuses_what_its_kind_doesnt_take(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, _) = me(&ws, cx, first_id(&ws, cx));
    let blocks = ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len());
    for (args, says) in [
        (json!({ "kind": "terminal", "folder": "/no/such/folder" }), "/no/such/folder isn't a folder."),
        (json!({ "kind": "note", "items": "x" }), "\"name\" and \"items\" are for a checklist"),
    ] {
        assert_eq!(call(handle, &ws, cx, &terminal, "create_block", args), (true, says.to_string()));
    }
    assert_eq!(ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len()), blocks, "nothing opened");
}
```

`the_instructions_say_what_to_do_with_a_kanban_and_a_note` also checks "For work of several steps, open your own
checklist with create_block and check its items as you go."

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp create_block && cargo test -p templum create_block`
Expected: FAIL.

- [ ] **Step 3: Implement** the Interfaces, the answers and the refusals as the spec writes them; the instructions'
  sentence after "…check the items of a connected checklist as you finish them."; `create_card`'s description ends
  "A review's findings: one card each, its priority by severity, file:line and what fails in its description.".

- [ ] **Step 4: Run the full check.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-mcp/src/tools.rs crates/templum-mcp/src/cli.rs crates/templum/src/workspace/ops.rs \
  crates/templum/src/workspace/content.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/spawn.rs \
  crates/templum/src/workspace/note_tabs.rs crates/templum/src/workspace/checklist_blocks.rs \
  crates/templum/src/workspace/tests/content.rs docs/specs/connections.md
git commit -m "feat(connections): agents open their own note, checklist or a terminal running a command"
```

---

### Task 5: `replace_note`, `edit_item`, `delete_item` (closes batch 1)

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs`, `cli.rs`; `crates/templum/src/workspace/ops.rs` (`note_replace`,
  `item_edit`), `content.rs` (the tools), `bus.rs`
- Modify: `docs/specs/connections.md`, `docs/specs/checklist.md` (its Agents section names the two new tools),
  `docs/specs/agent-tools.md` (Status: batch 1 implemented)
- Test: `crates/templum-mcp/src/tools.rs`, `cli.rs`, `crates/templum/src/workspace/tests/content.rs`

**Interfaces:**
- Consumes: `wired_note`, `wired_checklist`, `Workspace::{edit_checklist, agent_changed, delete_item}`,
  `Checklists::{set_text, item}`.
- Produces:
  - `Call::ReplaceNote { block, text, old: Option<String> }`, `Call::EditItem { block, item: u64, text }`,
    `Call::DeleteItem { block, item: u64 }`.
  - `pub(super) fn note_replace(&mut self, space: &str, block: BlockId, text: &str, old: Option<&str>, window, cx)
    -> Result<(), String>` (the note the block shows; one insert over the selected range, so one step of the note's
    Ctrl+Z; the person's selection put back as `append_note` does, clamped to the new length); `pub(super) fn item_edit(&mut self, list:
    u64, item: u64, text: &str) -> bool`.
  - Tool methods in `content.rs`: `replace_note`, `edit_item`, `remove_item` (the name `delete_item` is the row's ×,
    which `remove_item` calls).
  - Verbs `templum note replace <block> <text> [--old <text>]`, `templum checklist edit <block> <item> <text> | delete
    <block> <item>`.

- [ ] **Step 1: Write the failing tests** (`tools.rs` examples and blank/missing refusals; the verbs; then in
  `workspace/tests/content.rs`, with `wired_note`, `note_editor`, `wired_checklist`):

```rust
#[gpui_kit::test]
fn replace_note_replaces_the_text_or_one_passage_and_ctrl_z_takes_it_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, note, uid) = wired_note(handle, &ws, cx);
    let editor = note_editor(&ws, cx, note);
    set_note(handle, &editor, cx, "Plan\n- build\n- build\n- ship"); // as the append test fills it
    assert_eq!(
        call(handle, &ws, cx, &me, "replace_note", json!({ "block": uid, "old": "- build", "text": "x" })),
        (true, "\"- build\" is in the note 2 times: give more of the text around it.".into())
    );
    assert_eq!(
        call(handle, &ws, cx, &me, "replace_note", json!({ "block": uid, "old": "nope", "text": "x" })),
        (true, "\"nope\" isn't in the note: read_note shows it.".into())
    );
    let ok = call(handle, &ws, cx, &me, "replace_note", json!({ "block": uid, "old": "- ship", "text": "- ship it" }));
    assert_eq!(ok, (false, format!("Replaced 1 passage of the note in {uid}.")));
    let ok = call(handle, &ws, cx, &me, "replace_note", json!({ "block": uid, "text": "All new" }));
    assert_eq!(ok, (false, format!("Replaced the note in {uid}.")));
    assert_eq!(read(&editor, cx), "All new");
    assert_eq!(
        call(handle, &ws, cx, &me, "replace_note", json!({ "block": uid, "text": " " })),
        (true, "That would empty the note: the person does that.".into())
    );
    undo_note(handle, &editor, cx); // the note editor's own Ctrl+Z, as the append test presses it
    assert_eq!(read(&editor, cx), "Plan\n- build\n- build\n- ship it");
}

#[gpui_kit::test]
fn edit_item_and_delete_item_fix_a_checklist_one_ctrl_z_step_each(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _block, uid, list, build, tests) = wired_checklist(handle, &ws, cx);
    assert_eq!(
        call(handle, &ws, cx, &me, "edit_item", json!({ "block": uid, "item": build, "text": "Build it" })),
        (false, format!("Item {build} is now \"Build it\"."))
    );
    assert_eq!(
        call(handle, &ws, cx, &me, "delete_item", json!({ "block": uid, "item": tests })),
        (false, format!("Deleted item {tests}."))
    );
    assert_eq!(
        call(handle, &ws, cx, &me, "edit_item", json!({ "block": uid, "item": 999, "text": "x" })),
        (true, "There's no item 999 in \"Deploy\". read_checklist shows its items.".into())
    );
    assert!(call(handle, &ws, cx, &me, "edit_item", json!({ "block": uid, "item": build, "text": " " })).0);
    ws.update(cx, |ws, cx| assert!(ws.undo_checklists(cx)));
    ws.read_with(cx, |ws, _| assert!(ws.checklists.get(list).unwrap().item(tests).is_some(), "the delete taken back"));
    ws.update(cx, |ws, cx| assert!(ws.undo_checklists(cx)));
    ws.read_with(cx, |ws, _| assert_eq!(ws.checklists.get(list).unwrap().item(build).unwrap().text, "Build"));
}
```

(`set_note`, `read`, `undo_note` are what `append_note_shows_at_once_keeps_the_caret_and_one_ctrl_z_takes_it_back`
already does inline: lift them into helpers of `tests/content.rs`.)

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p templum replace_note edit_item`. Expected: FAIL.

- [ ] **Step 3: Implement** the Interfaces with the spec's answers; update connections.md, checklist.md's Agents
  section, and agent-tools.md's Status ("Batch 1 implemented").

- [ ] **Step 4: Run the full check, then the batch's benches**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, then
`cargo run --release -p templum -- --bench` and `-- --bench-mixed`.
Expected: PASS; both JSON lines with `p95_ms` ≤ 16.7 (kept for the pull request; Task 2 touched the reader thread).

- [ ] **Step 5: By hand** (a `--dev` Templum): Claude Code `spawn_agent`s a Codex, which runs `(sleep 60 && touch ~/x)
  &`; `stop_agent` on it; after a minute no `~/x`. `create_block` a terminal running `npx vite` in a project, then
  `wait_for` with `pattern: "ready in"` and `lines: 50`.

- [ ] **Step 6: Commit**

```bash
git add crates/templum-mcp/src/tools.rs crates/templum-mcp/src/cli.rs crates/templum/src/workspace/ops.rs \
  crates/templum/src/workspace/content.rs crates/templum/src/workspace/bus.rs \
  crates/templum/src/workspace/tests/content.rs docs/specs/connections.md docs/specs/checklist.md docs/specs/agent-tools.md
git commit -m "feat(connections): agents replace a note's text and edit or delete a checklist's items"
```

---

## Batch 2: forks and roles

### Task 6: Spike: forking by hand

**Files:**
- Modify: `docs/specs/agent-tools.md` (a "Spike results" list under `spawn_agent` with `fork`)

- [ ] **Step 1: Check by hand**, each in a Templum terminal, and write down what happened:
  1. Claude Code in `~/a` (a repository), two turns; then from `~/a/.worktrees/w` (made with `git worktree add`):
     `claude --resume <its id> --fork-session`. Does it open the conversation? (If not: the spec's refusal of
     `folder`/`worktree` for a Claude Code fork stands; if yes: delete that refusal.)
  2. The same while the original is mid-turn (its last entry a tool call): does the fork open, and does it answer a
     new prompt?
  3. `codex fork <id>` from another folder.
  4. Is the fork's new session id found by the next snapshot, for Claude Code and for Codex (a restart resumes the
     fork, not the original)? If not for one of them, its fork waits: drop it from Task 7 and the spec.
- [ ] **Step 2: Correct the spec** with the results (the refusal kept or dropped; any agent whose flag differs).
- [ ] **Step 3: Commit**

```bash
git add docs/specs/agent-tools.md
git commit -m "docs: what forking an agent's session does, checked by hand"
```

---

### Task 7: `spawn_agent` with `fork`

**Files:**
- Modify: `crates/templum-terminal/src/agent.rs` (`Spec::fork_id`, `Agent::fork_session`), `crates/templum/src/agents.rs`
  (`AgentEntry::fork_command`)
- Modify: `crates/templum-mcp/src/tools.rs` (`Spawn::fork`, `agent` optional), `cli.rs` (`--fork`, the agent optional)
- Modify: `crates/templum/src/workspace/spawn.rs` (the fork path), `mod.rs` (`Space::session_of`, used by `item_of` too)
- Modify: `docs/specs/connections.md` (`spawn_agent`'s row), `docs/specs/agent-parity.md` (the "Forking a session" row:
  works for Claude Code and Codex), `docs/minors.md` (Connections: Pi and Grok Build can't be forked yet)
- Test: `crates/templum-terminal/src/agent.rs`, `crates/templum-mcp/src/tools.rs`, `cli.rs`,
  `crates/templum/src/workspace/tests/spawn.rs`

**Interfaces:**
- Produces:
  - `Spec { …, pub fork_id: Option<&'static str> }`: Claude Code `"claude --resume {id} --fork-session"`, Codex `"codex
    fork {id}"`; None for every other.
  - `Agent::fork_session(self, id: &str) -> Option<String>` (`typable(id)` as `resume_session`).
  - `AgentEntry::fork_command(&self, session: &str) -> Option<String>`: its builtin's `fork_session`; None for one of
    the person's agents.
  - `tools::Spawn { …, pub agent: Option<String>, pub fork: bool }`; parse: `spawn_agent needs "agent", or fork`, and
    `A fork needs a prompt: it tells the copy what to do.`
  - `start_agent` takes the command to type for one of Templum's agents (`Option<&str>`, None: its `start_command` as
    today), so a fork's command isn't replaced by the start command.
  - `Space::session_of(&self, uid: &str, agent: &str) -> Option<String>` (the lookup `item_of` does, shared).
  - In `spawn.rs`: a fork resolves the caller's entry (its account if it has one: `Pane::account`, else its seen
    agent), refuses as the spec writes, and types `fork_command(session)` + `launch_args` through `typed`, instead of
    `start_command`; its prompt's text starts with "You are a copy of the agent in {caller uid}, started from its
    conversation so far, in a block of your own.\n\n". `open_agent` takes the command to type as an argument so both
    paths share it (or `open_agent_in`, if card 43 landed it).

- [ ] **Step 1: Write the failing tests**

`agent.rs`: `fork_session` gives Claude Code's and Codex's commands, None for `gemini`, `copilot`, `opencode`, `pi`,
`grok`, `qwen`, and None for an id that isn't typable (`"a;b"`). `tools.rs`: `{"fork": true, "prompt": "x"}` parses with `agent:
None`; `{}` refuses with `spawn_agent needs "agent", or fork`. `cli.rs`: `["spawn", "--fork", "--prompt", "x"]`.

`workspace/tests/spawn.rs`:

```rust
#[gpui_kit::test]
fn a_fork_types_the_callers_fork_command_with_its_session(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let session = "0d5e5f4b-1c2d-4e3f-8a9b-0c1d2e3f4a5b";
    // The caller runs Claude Code, and the last snapshot found its session.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(mine).unwrap().seen.agent = Some("claude".into());
        ws.space.saved = Some(saved_with(&uid, "claude", session)); // the JSON close_block's test builds
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", json!({ "fork": true, "prompt": "go on" }));
    assert!(!failed && text.starts_with("Opened a fork of your session as "), "{text}");
    let new = spawned(&ws, cx, mine);
    screen_with(&ws, cx, new, &format!("--resume {session} --fork-session"));
    enter(cx);
    screen_with(&ws, cx, new, &format!("You are a copy of the agent in {uid}"));
}

#[gpui_kit::test]
fn a_fork_refuses_another_agent_and_a_session_not_found(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    ws.update(cx, |ws, _| ws.space.terminal_mut(mine).unwrap().seen.agent = Some("claude".into()));
    assert_eq!(
        call(handle, &ws, cx, &terminal, "spawn_agent", json!({ "fork": true })),
        (true, "A fork needs a prompt: it tells the copy what to do.".into())
    );
    let go = |agent: Option<&str>| match agent {
        Some(a) => json!({ "fork": true, "prompt": "x", "agent": a }),
        None => json!({ "fork": true, "prompt": "x" }),
    };
    assert_eq!(
        call(handle, &ws, cx, &terminal, "spawn_agent", go(None)),
        (true, "Templum doesn't know your session yet: try again in a few seconds.".into())
    );
    assert_eq!(
        call(handle, &ws, cx, &terminal, "spawn_agent", go(Some("codex"))),
        (true, "A fork is a copy of your own agent, claude: leave agent out.".into())
    );
    ws.update(cx, |ws, _| ws.space.terminal_mut(mine).unwrap().seen.agent = Some("gemini".into()));
    assert_eq!(
        call(handle, &ws, cx, &terminal, "spawn_agent", go(None)),
        (true, "Gemini CLI can't fork a session: forks are for Claude Code and Codex.".into())
    );
}
```

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p templum-terminal fork && cargo test -p templum fork`.
  Expected: FAIL.
- [ ] **Step 3: Implement** the Interfaces; the `folder`/`worktree` refusal for a Claude Code fork only if Task 6 kept
  it.
- [ ] **Step 4: Run the full check.** Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/templum-terminal/src/agent.rs crates/templum/src/agents.rs crates/templum-mcp/src/tools.rs \
  crates/templum-mcp/src/cli.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/mod.rs \
  crates/templum/src/workspace/tests/spawn.rs docs/specs/connections.md docs/specs/agent-parity.md docs/minors.md
git commit -m "feat(connections): spawn_agent forks your own session into a new agent block"
```

---

### Task 8: Roles (closes batch 2)

**Files:**
- Modify: `crates/templum/src/config.rs` (`[roles]`, its template), `crates/templum/src/session.rs` (`TerminalItem::role`),
  `crates/templum/src/workspace/mod.rs` (`Pane::role`, `item`), `persist.rs` (typed again on restore), `spawn.rs`
  (`role`), `bus.rs` (the instructions' roles line)
- Modify: `crates/templum-mcp/src/tools.rs` (`Spawn::role`), `cli.rs` (`--role`)
- Modify: `docs/specs/config.md` (`[roles]`), `docs/specs/connections.md`, `docs/specs/agent-tools.md` (Status: batch 2)
- Test: `crates/templum/src/config.rs`'s tests, `crates/templum/src/session.rs`'s tests,
  `crates/templum/src/workspace/tests/spawn.rs`, `tests/persistence.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, Deserialize, PartialEq)] pub struct Role { pub agent: String, pub model: Option<String>,
    pub effort: Option<String>, pub args: Option<String>, pub instructions: Option<String> }`;
    `Config::roles: BTreeMap<String, Role>` (validated: an id of letters, digits, `-`, `_`; `agent` not blank; `args`
    one line; else the config error `roles.<id>: …`).
  - `impl Role { pub fn spawn(&self, id: &str, spawn: tools::Spawn) -> Result<tools::Spawn, String> }`: the call's
    `agent`, `model`, `effort` win; the prompt becomes `"Your role, {id}:\n{instructions}\n\nYour task:\n{prompt}"`
    when there are instructions (refused without a prompt as the spec writes); the role's `args` ride along
    (`Spawn::args: Option<String>`, a new field only roles set).
  - `tools::Spawn { …, pub role: Option<String>, pub args: Option<String> }` (`args` never parsed from a call:
    `parse` leaves it None).
  - `AgentEntry::launch_args` gains the role's words after the model and effort (`launch_args_with(model, effort,
    args)`).
  - `Pane::role: Option<String>`, saved as `role` (skipped when None); `restore_terminal` adds the role's `args` again
    when the role still exists.
  - The first line of its task: `[from {by}, task t{id}, as {role}]`.
  - The instructions: `Roles for spawn_agent: reviewer (claude), tester (codex).` when `config.roles` isn't empty (at
    most 10, then `…`), after "list_blocks gives the current list.".

- [ ] **Step 1: Write the failing tests**

`config.rs`: a `[roles.reviewer]` with every key reads back; a role without `agent`, with `args` on two lines, or an id
`re view` is an error naming `roles.reviewer` (or `roles.re view`). `session.rs`: `role` round-trips and an older item
has none.

`workspace/tests/spawn.rs`:

```rust
#[gpui_kit::test]
fn a_role_opens_its_agent_with_its_words_and_its_instructions_go_with_the_prompt(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    ws.update(cx, |ws, _| {
        ws.config.roles.insert("reviewer".into(), crate::config::Role {
            agent: "codex".into(), model: Some("gpt-5.6-luna".into()), effort: Some("high".into()),
            args: Some("-s read-only".into()), instructions: Some("Never edit files.".into()),
        });
    });
    let dir = tempfile::tempdir().unwrap();
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent",
        json!({ "role": "reviewer", "folder": dir.path(), "prompt": "review HEAD" }));
    assert!(!failed, "{text}");
    let new = spawned(&ws, cx, mine);
    screen_with(&ws, cx, new, "-m gpt-5.6-luna -c model_reasoning_effort=high -s read-only");
    enter(cx);
    screen_with(&ws, cx, new, "as reviewer]");
    screen_with(&ws, cx, new, "Your role, reviewer:");
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.terminal(new).unwrap().role.as_deref(), Some("reviewer")));
    assert_eq!(
        call(handle, &ws, cx, &terminal, "spawn_agent", json!({ "role": "reviewer" })),
        (true, "The role reviewer's instructions go with a prompt: give one.".into())
    );
    assert_eq!(
        call(handle, &ws, cx, &terminal, "spawn_agent", json!({ "role": "tester", "prompt": "x" })),
        (true, "There's no role tester. Roles: reviewer.".into())
    );
}
```

`tests/persistence.rs`: a saved terminal item with `"agent": "codex", "role": "reviewer"` restores typing `-s
read-only` again, and none once the role is removed from the config.

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p templum role`. Expected: FAIL.
- [ ] **Step 3: Implement** the Interfaces; `config.toml`'s template gains the spec's commented `[roles.reviewer]`;
  config.md documents `[roles]`; the instructions' line; agent-tools.md's Status ("Batches 1 and 2 implemented").
- [ ] **Step 4: Run the full check.** Expected: PASS.
- [ ] **Step 5: By hand:** a `[roles.reviewer]` with `agent = "claude"`, `args = "--tools Read,Grep,Glob"`; Claude
  Code `spawn_agent(role: "reviewer", prompt: "try to edit README.md")` can't edit; a restart keeps `--tools`.
- [ ] **Step 6: Commit**

```bash
git add crates/templum/src/config.rs crates/templum/src/session.rs crates/templum/src/workspace/mod.rs \
  crates/templum/src/workspace/persist.rs crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/bus.rs \
  crates/templum/src/agents.rs crates/templum-mcp/src/tools.rs crates/templum-mcp/src/cli.rs \
  crates/templum/src/workspace/tests/spawn.rs crates/templum/src/workspace/tests/persistence.rs \
  docs/specs/config.md docs/specs/connections.md docs/specs/agent-tools.md
git commit -m "feat(connections): saved roles open an agent with its model, flags and instructions"
```

---

## Batch 3: a plan the person approves

### Task 9: `Ask::plan`, the tool and its verb

**Files:**
- Modify: `crates/templum-mcp/src/ask.rs`, `tools.rs` (`propose_plan`), `cli.rs` (`templum plan`)
- Modify: `crates/templum/src/workspace/bus.rs` (dispatch: a `Waiting::Question` with the plan's `Ask`)
- Test: `crates/templum-mcp/src/ask.rs`, `tools.rs`, `cli.rs`

**Interfaces:**
- Consumes: `ask::{Ask, Question, Choice, Form}` and the answer/notice functions `ask.rs` has.
- Produces:
  - `pub struct Plan { pub title: Option<String>, pub text: String }` and `Ask { …, pub plan: Option<Plan> }`;
    `pub fn plan(title: Option<String>, text: String) -> Ask`: one single-select question `"Approve the plan?"` with
    options `Approve` and `Reject`, the note on, `questions` shape off.
  - The answer of a plan's form: `Reply::ok("Approved.")` or `Reply::err("Rejected.")`, each with
    `"\nThe person's comment: {note}"` when the note isn't blank (`ask::plan_reply(&Form) -> Reply`, or the place the
    other shapes are answered branches on `plan`).
  - The notice's first line: `[answer to your plan "{title or 'A plan'}"] The answer is in the pasted text.`
  - `Call::ProposePlan { plan: String, title: Option<String>, timeout: Option<u64> }` (blank or over 256 KB refused:
    `"plan" is blank`, `"plan" is longer than 256 KB`); dispatched as `Call::AskUser` is, with `deadline(timeout,
    question_timeout)`.
  - `templum plan <file> [--title <t>] [--json]`: reads the file (`-`: stdin) into `plan`; a rejection exits 1 (it is
    an error result).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_plans_form_answers_approved_or_rejected_with_the_comment() {
    let ask = plan(Some("Move the bus".into()), "## Steps\n1. a".into());
    let mut form = Form::new(&ask);
    assert!(!form.answered());
    form.pick(0, 0); // Approve
    assert_eq!(plan_reply(&form), Reply::ok("Approved."));
    form.pick(0, 1); // Reject
    form.set_note("do b first");
    assert_eq!(plan_reply(&form), Reply::err("Rejected.\nThe person's comment: do b first"));
    assert_eq!(
        notice_line(&ask),
        "[answer to your plan \"Move the bus\"] The answer is in the pasted text."
    );
}
```

(Use `Form`'s real picking and note methods; the names above stand for them.) `tools.rs`: the example parses; blank and
oversized refused. `cli.rs`: `["plan", <a temp file>, "--title", "T"]` sends the file's text and the title.

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p templum-mcp plan`. Expected: FAIL.
- [ ] **Step 3: Implement** the Interfaces. Until Task 10, the balloon draws a plan as a one-question dialog (the plan
  text as the question); that is fine for this commit.
- [ ] **Step 4: Run the full check.** Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/templum-mcp/src/ask.rs crates/templum-mcp/src/tools.rs crates/templum-mcp/src/cli.rs \
  crates/templum/src/workspace/bus.rs
git commit -m "feat(connections): propose_plan asks the person to approve or reject a plan"
```

---

### Task 10: The plan's dialog (closes batch 3)

**Files:**
- Modify: `crates/templum/src/balloon.rs` (a plan's body), `crates/templum/src/workspace/questions.rs` (Reject and
  Approve as the foot's buttons, their keys), `crates/templum/src/workspace/waits.rs` (the notification's words)
- Modify: `docs/specs/connections.md` (`propose_plan`'s row, `templum plan`), `docs/specs/agent-tools.md` (Status)
- Test: `crates/templum/src/workspace/tests/requests.rs`

**Interfaces:**
- Consumes: Task 9's `Ask::plan`; `crate::markdown::{preview, PreviewScroll}`; the balloon's world sizes × zoom.
- Produces: a `PreviewScroll` kept per plan wait (in `Waiting::Question`, `plan_scroll: Option<PreviewScroll>`);
  element ids `plan-approve-{key}` and `plan-reject-{key}` for the buttons, `plan-body-{key}` for the frame.
- Layout (spec): header chip "Plan" and the title (`A plan` when none, cut at 60 characters); the Markdown in a frame on
  the canvas color, at most 60% of the balloon's height, scrolling on its own; the Comment field (the note, placeholder
  `Comment (optional)`); Reject left, Approve right (primary). Keys as `ask_user`'s: ↓/Tab through Comment, Reject,
  Approve; Enter presses the highlighted button; Esc gives the keyboard back; TOO_SOON. Notification: `Asks you to
  approve a plan: {title}`.

- [ ] **Step 1: Write the failing tests** in `tests/requests.rs`, as the `ask_user` dialog tests drive the balloon:

```rust
#[gpui_kit::test]
fn a_plan_is_approved_or_rejected_with_a_comment_from_its_dialog(cx: &mut TestAppContext) {
    let (handle, ws, me_term, id) = asking_terminal(cx); // the setup the question tests use
    let answers = call_later(handle, &ws, cx, &me_term, 3, "propose_plan",
        json!({ "plan": "## Steps\n1. Replace the reader threads\n2. Test", "title": "Move the bus" }));
    frame(handle, cx);
    assert!(drawn(handle, cx, &format!("plan-body-{}", 1)), "the plan is drawn");
    after(cx, 600); // past TOO_SOON
    type_in_note(handle, &ws, cx, "do 2 first");
    click_id(handle, cx, "plan-reject-1");
    assert_eq!(answered(&answers), (true, "Rejected.\nThe person's comment: do 2 first".into()));
    let answers = call_later(handle, &ws, cx, &me_term, 4, "propose_plan", json!({ "plan": "ok" }));
    frame(handle, cx);
    after(cx, 600);
    press_keys(handle, cx, &["down", "down", "down", "enter"]); // Comment, Reject, Approve, Enter
    assert_eq!(answered(&answers), (false, "Approved.".into()));
    let _ = id;
}

#[gpui_kit::test]
fn a_cancelled_plan_is_typed_back_once_answered(cx: &mut TestAppContext) {
    // As `a_cancelled_question_stays_and_its_answer_is_typed_back`, with propose_plan: the typed notice starts
    // "[answer to your plan \"A plan\"] The answer is in the pasted text." and holds "Approved.".
}
```

(`asking_terminal`, `drawn`, `type_in_note`, `click_id`, `press_keys` stand for what the `ask_user` tests in
`requests.rs` already do; reuse their code. Spell out the second test from its sibling.) A plan, a question and a
connect request on one block wait in turn: extend the "1 of 3" test with a plan (Review Focus 5).

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p templum plan`. Expected: FAIL.
- [ ] **Step 3: Implement** the layout and keys; docs rows; agent-tools.md's Status ("Batches 1 to 3 implemented").
- [ ] **Step 4: Run the full check and the benches** (`--bench`, `--bench-mixed`). Expected: PASS, p95 ≤ 16.7 ms.
- [ ] **Step 5: By hand:** Claude Code calls `propose_plan` with a 150-line plan at 100% and 50% zoom (it scales like a
  photo, scrolls, wraps at the same words); Reject with a comment; Approve with Enter.
- [ ] **Step 6: Commit**

```bash
git add crates/templum/src/balloon.rs crates/templum/src/workspace/questions.rs crates/templum/src/workspace/waits.rs \
  crates/templum/src/workspace/tests/requests.rs docs/specs/connections.md docs/specs/agent-tools.md
git commit -m "feat(connections): a plan shows in its block's balloon, rendered, with Approve and Reject"
```

---

## Batch 4: later, typed back

### Task 11: `schedule` and `cancel_schedule`

**Files:**
- Create: `crates/templum/src/workspace/schedules.rs` ("Deliveries an agent asked for later
  (docs/specs/agent-tools.md#schedule-and-cancel_schedule): kept in `Workspace::schedules`, each with its timer.")
- Modify: `crates/templum/src/workspace/mod.rs` (`Workspace::schedules`, `next_schedule`), `ops.rs`
  (`schedule_add`, `schedule_cancel`), `bus.rs` (dispatch; `whoami`'s lines), `delegate.rs` (`delegate`'s first line
  may name a schedule), `blocks.rs` (closing a block drops the schedules it asked for)
- Modify: `crates/templum-mcp/src/tools.rs`, `cli.rs`
- Modify: `docs/specs/connections.md`
- Test: `crates/templum-mcp/src/tools.rs`, `cli.rs`, `crates/templum/src/workspace/tests/delegate.rs` (or a new
  `tests/schedules.rs`, registered in `tests/mod.rs`)

**Interfaces:**
- Produces:
  - `tools::When { At { hour: u8, minute: u8 }, After { minutes: u64 }, None }` and `Call::Schedule { block, text, when:
    When, every: Option<u64> }`, `Call::CancelSchedule { schedule: String }`; parse refusals: `schedule needs at, after
    or every`, `"at" and "after" go apart`, `"at" is HH:MM, 00:00 to 23:59`, `"after" and "every" are minutes from 1 to
    10080`.
  - `pub(crate) struct Schedule { pub id: u64, pub by: String, pub to: String, pub text: String, pub next: SystemTime,
    pub every: Option<Duration>, pub until: SystemTime }` (`by` the asker's terminal, `to` the target's block id;
    `until` `next + 7 days` for a repeating one, `next` for a one-time one). The computer's clock, not `Instant`:
    GPUI's timers and `Instant` don't count time asleep.
  - `pub(super) fn schedule_add(&mut self, s: Schedule, cx) -> u64` and `pub(super) fn schedule_cancel(&mut self, id:
    u64) -> bool` in `ops.rs`; `schedules.rs` looks every `SCHEDULE_LOOK` (15 s) while `schedules` isn't empty (one
    loop, started by the first schedule, ended with the last) and delivers those whose `next` passed, once each; a
    repeating one's `next` becomes `now + every`.
  - `fn deliver(&mut self, id: u64, cx)` in `schedules.rs`: to the asker's own block a
    `queue_notice(by, "[scheduled s{id}] Its text is in the pasted text.\n{text}")`; to an agent on a task (any, the
    asker's included: `pane.task(now).is_some()`) the refusal `{to} is on a task (t{n}): a schedule hands a task only
    to an agent that has none.`; to an agent with none `delegate(by, to, text, false, TASK_WAIT, cx)` with the task's
    line naming `scheduled s{id}`; to a terminal running no agent its text and
    `\r` as `send_to_terminal` with `enter`. An `Outcome::Now(err)` becomes `queue_notice(by, "[schedule s{id} to {to}
    didn't go: {err}]")`; a closed target ends the schedule after that notice; a repeating one re-arms at `next +
    every` until `until`, then notices `[schedule s{id} ended: 7 days]`.
  - `at` becomes a `SystemTime`: the next local `HH:MM` after `chrono::Local::now()` (today or tomorrow), through
    `chrono::Local` for that day so a daylight-saving change counts.
  - Answer: `Scheduled s{id}: to {to} at {HH:MM} (in {n} minutes)[, then every {m} minutes until {d Mon HH:MM}].`
    (`after`: `in {n} minutes` without the `at`); the caller's own block reads `to you`.
  - `whoami` gains, per schedule of the caller: `schedule s3: to k9x1d every 10 min, next 15:10: {first line, 60
    chars}`.
  - Verbs `templum schedule <block> <text> [--at HH:MM] [--after <min>] [--every <min>]`, `templum unschedule <id>`
    (`s3` or `3`).

- [ ] **Step 1: Write the failing tests** (`tools.rs` parses each form and refuses each bad one; the verbs):

```rust
#[gpui_kit::test]
fn a_schedule_goes_at_its_time_to_an_agent_a_terminal_and_the_caller(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (failed, text) = call(handle, &ws, cx, &terminal, "schedule",
        json!({ "block": uid, "text": "check the build", "after": 2 }));
    assert!(!failed && text.starts_with("Scheduled s1: to you in 2 minutes"), "{text}");
    ws.read_with(cx, |ws, _| assert!(ws.notices.is_empty()));
    // The test's clock moves `Instant`s, not the computer's: the schedule's time is put in the past, then a look comes.
    due_now(&ws, cx, 1); // sets schedule 1's `next` to a second ago
    after(cx, 15_000);
    ws.read_with(cx, |ws, _| {
        assert!(ws.notices.iter().any(|n| n.text.starts_with("[scheduled s1] Its text is in the pasted text.\ncheck the build")));
    });
    // A wired plain terminal, every minute: typed with Enter each time.
    let plain = wired_plain_terminal(handle, &ws, cx, mine); // as the send_to_terminal tests wire one
    let (_, them) = me(&ws, cx, plain);
    assert!(!call(handle, &ws, cx, &terminal, "schedule", json!({ "block": them, "text": "echo tick", "every": 1 })).0);
    due_now(&ws, cx, 2);
    after(cx, 15_000);
    screen_with(&ws, cx, plain, "tick");
    assert_eq!(
        call(handle, &ws, cx, &terminal, "cancel_schedule", json!({ "schedule": "s2" })),
        (false, "Cancelled s2.".into())
    );
    assert_eq!(
        call(handle, &ws, cx, &terminal, "cancel_schedule", json!({ "schedule": "s2" })),
        (true, "s2 isn't one of your schedules.".into())
    );
}

#[gpui_kit::test]
fn a_schedule_that_cant_go_says_so_and_a_closed_caller_ends_its_schedules(cx: &mut TestAppContext) {
    // Wire a terminal, schedule to it `every` 1, unwire it; at its time (`due_now`, a look) a notice
    // "[schedule s1 to {them} didn't go: {them} isn't connected to you. Ask the person with connect(\"{them}\").]"
    // is queued, and the schedule is still there. An agent on the caller's task (a `Handed` set on its pane) doesn't
    // take a schedule's text: "[schedule s2 to {them} didn't go: {them} is on a task (t1): a schedule hands a task only
    // to an agent that has none.]". Then close the caller's block: `ws.schedules` is empty and no notice
    // comes at the next minute. A repeating schedule 7 days on: "[schedule s… ended: 7 days]" and it is gone.
}
```

(Spell out the second test from its comment, with `ws.space.remove_wire`/`unwire` as the wire tests call it and
`close_now` as `close_block` does. `ws.notices` is `Vec<delegate::Notice { to, text }>`. `due_now(ws, cx, id)` is a
helper of this test file.)

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p templum schedule`. Expected: FAIL.
- [ ] **Step 3: Implement** the Interfaces; connections.md's rows and verbs.
- [ ] **Step 4: Run the full check.** Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/templum/src/workspace/schedules.rs crates/templum/src/workspace/mod.rs \
  crates/templum/src/workspace/ops.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/delegate.rs \
  crates/templum/src/workspace/blocks.rs crates/templum-mcp/src/tools.rs crates/templum-mcp/src/cli.rs \
  crates/templum/src/workspace/tests/ docs/specs/connections.md
git commit -m "feat(connections): schedule hands a text to an agent, a terminal or yourself at a time or every few minutes"
```

---

### Task 12: The schedules on the caller's block

**Files:**
- Modify: `crates/templum/src/block.rs` (the clock and count after the title, its tooltip), `crates/templum/src/menu/`
  (the block's menu: **Cancel schedules ▸**), `crates/templum/src/workspace/schedules.rs` (`schedules_of(terminal) ->
  Vec<&Schedule>`, `cancel_all_of(terminal)`)
- Test: `crates/templum/src/workspace/tests/menus.rs`, `tests/schedules.rs` (or where Task 11 put its tests)

**Interfaces:**
- Consumes: Task 11's `Workspace::schedules`, `schedule_cancel`.
- Produces: element id `schedules-{block}` for the clock; menu rows `Cancel schedules ▸` → one row per schedule
  (`s3: to k9x1d every 10 min`) and `All`. The clock is `IconName::Clock` (lucide, registered through `AllAssets`) and
  the count, in `ui.title_dim`, after the title, at the Full level only.

- [ ] **Step 1: Write the failing tests:** with two schedules from the caller, a frame draws `schedules-{block}`; its
  menu lists `Cancel schedules ▸` with both and `All`; choosing one cancels it (one left), `All` cancels the other;
  with none, neither the clock nor the menu row is there. (Drive the menu as `tests/menus.rs` drives **Connect to ▸**.)
- [ ] **Step 2: Run them to see them fail.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the full check.** Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/templum/src/block.rs crates/templum/src/menu crates/templum/src/workspace/schedules.rs \
  crates/templum/src/workspace/tests/
git commit -m "feat(connections): a block shows the schedules its agent set, and its menu cancels them"
```

---

### Task 13: `wait_for` with `wait: false` (closes batch 4)

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (`wait` on `wait_for`, only with a pattern), `crates/templum/src/workspace/
  schedules.rs` (`Workspace::watches`: `Watch { id, by, of, uid, pattern, until }`, looked at every EVENT_LOOK),
  `ops.rs` (`watch_add` keeps a `Watch`), `waits.rs` (the `wait: false` answer), `blocks.rs` (a closed
  caller drops its watches)
- Modify: `docs/specs/connections.md`, `docs/specs/agent-tools.md` (Status: batches 1 to 4)
- Test: `crates/templum-mcp/src/tools.rs`, `crates/templum/src/workspace/tests/schedules.rs`

**Interfaces:**
- Consumes: Task 2's `Terminal::{watch, hit, unwatch}`, Task 3's `watch_add`, Task 11's file and its delivery by
  `queue_notice`.
- Produces: `Call::WaitFor { …, wait: bool }` (`"wait" goes with a pattern`); the answer `Watching {uid} for
  "{pattern}" as w{id}: a match is typed into your terminal.`; notices exactly as the spec writes them (a match, its
  timeout, its block closed). Ids `w1`, `w2`… from their own counter.

- [ ] **Step 1: Write the failing tests:** a `wait: false` watch answers at once; a printed matching line queues one
  notice starting `[watch w1 on {uid}: a line matches "ERROR"] The line is in the pasted text.\n` with the line; a
  second matching line queues nothing more and the terminal has no watch left (`watching() == 0`); a watch with
  `timeout: 1` queues `[watch w2 on {uid} ended: no line matched "never" in 1 s]`; closing the watched block queues
  `[watch w3 on {uid} ended: {uid} was closed]`; closing the caller drops its watches without a notice.
- [ ] **Step 2: Run them to see them fail.** Expected: FAIL.
- [ ] **Step 3: Implement**; docs; Status.
- [ ] **Step 4: Run the full check and the benches.** Expected: PASS, p95 ≤ 16.7 ms.
- [ ] **Step 5: By hand:** Claude Code sets `wait_for(pattern: "ERROR", wait: false)` on a dev server, keeps working, and
  gets the notice when a broken file is saved; a Codex `schedule`d `every: 1` minute to report `git status`.
- [ ] **Step 6: Commit**

```bash
git add crates/templum-mcp/src/tools.rs crates/templum/src/workspace/schedules.rs \
  crates/templum/src/workspace/ops.rs crates/templum/src/workspace/waits.rs crates/templum/src/workspace/blocks.rs \
  crates/templum/src/workspace/tests/ docs/specs/connections.md docs/specs/agent-tools.md
git commit -m "feat(connections): wait_for without wait watches a terminal and types the match back"
```

---

## Batch 5: worktrees

### Task 14: A worktree's state and removing one, in `templum-git`

**Files:**
- Modify: `crates/templum-git/src/repo.rs`
- Test: `crates/templum-git/src/repo.rs`'s tests (or its `tests/` file where `add_worktree` is tested)

**Interfaces:**
- Consumes: `Repo::{add_worktree, merge, delete_branch, unmerged_count, text, run}`, `worktree_name_ok`.
- Produces:
  - `pub struct WorktreeState { pub path: PathBuf, pub branch: String, pub into: String, pub changed: usize, pub
    unmerged: u32, pub into_changed: usize, pub busy: bool }`, read from the `Repo` of the folder it goes back to:
    `into` is that working tree's current branch (the main working tree's or a linked one's), `unmerged` the commits of
    `branch` not in `into`, `into_changed` its changes to tracked files (staged or not; untracked don't count), `busy` a
    merge, rebase or cherry-pick under way there.
  - `Repo::worktree_state(&self, name: &str) -> Result<Option<WorktreeState>, GitError>`: None when
    `.worktrees/<name>` (under the main working tree) isn't in `git worktree list --porcelain` or the name isn't
    `worktree_name_ok`.
  - `Repo::remove_worktree(&self, name: &str, force: bool) -> Result<(), GitError>`: `git worktree remove [--force]
    <path>` then `git branch -d|-D <name>`.
  - `Repo::merge_from(&self, branch: &str) -> Result<Result<(), Vec<String>>, GitError>` in this `Repo`'s working
    tree: `git merge --no-edit <branch>`; on a conflict the conflicted paths (`git diff --name-only --diff-filter=U`),
    then `git merge --abort`.

- [ ] **Step 1: Write the failing tests** (with the `git_repo`-style temporary repository these tests use):
  `worktree_state` of a fresh `add_worktree("w")` is `changed 0, unmerged 0, into "main", into_changed 0` (the repo's
  branch); after a file changed in it `changed 1`; after a commit there `unmerged 1`; a tracked file changed in the main
  working tree `into_changed 1`, an untracked one 0; read from a linked worktree `a` that made `w`, `into` is `a`'s
  branch and `unmerged` doesn't count `a`'s own commits; `remove_worktree("w", false)` fails with changes
  and succeeds with `true`, the branch gone; `merge_from("w")` of a clean commit merges it (`unmerged` back to 0); two
  commits changing one line on both sides give `Err(vec!["a.txt"])` and leave the main working tree with no merge under
  way and its file as before.
- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p templum-git worktree`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the full check.** Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/templum-git/src/repo.rs
git commit -m "refactor(git): read a worktree's changes and unmerged commits, remove it, and merge it back"
```

(Add its test file to `git add` if the tests live in `crates/templum-git/tests/`.)

---

### Task 15: `remove_worktree`, `merge_worktree`, and the request with two buttons (closes the card)

**Files:**
- Modify: `crates/templum/src/workspace/waits.rs` (`Waiting::Connect { to }` becomes one case of `Waiting::Allow {
  says: String, yes: &'static str, no: &'static str, then: Approved }`, `Approved::{Connect { to }, MergeWorktree {
  repo, name }, RemoveWorktree { repo, name }}`), `balloon.rs` (its two buttons' words), `ops.rs`
  (`worktree_state`, `worktree_remove`, `worktree_merge`, run on the background executor), `content.rs` or a new
  `workspace/worktrees.rs` (the two tools), `bus.rs`
- Modify: `crates/templum-mcp/src/tools.rs`, `cli.rs`
- Modify: `docs/specs/connections.md`, `docs/specs/git.md` (worktrees: an agent removes or merges one),
  `docs/specs/agent-tools.md` (Status: implemented), `docs/ARCHITECTURE.md` (the `workspace/` list)
- Test: `crates/templum-mcp/src/tools.rs`, `cli.rs`, `crates/templum/src/workspace/tests/requests.rs`,
  `tests/spawn.rs`

**Interfaces:**
- Consumes: Task 14's `WorktreeState`, `remove_worktree`, `merge_from`; `answer_connect` (generalized to
  `answer_allow(key, yes, cx)`; the balloon's buttons call it).
- Produces:
  - `Call::RemoveWorktree { worktree: String, folder: Option<String>, discard: bool }`, `Call::MergeWorktree {
    worktree: String, folder: Option<String> }` (`folder` resolved as `spawn_agent`'s: relative to the caller's).
  - `merge_worktree` refuses before asking with the spec's sentences: changes in the worktree (`changed > 0`), changes
    where it merges into (`into_changed > 0`: "{into} has changes not committed in {path}: commit or stash them
    first."), nothing to merge, `busy`.
  - The tools refuse first (no such worktree, a terminal working in it in any workspace: a pane whose
    `seen.folder` starts with its path), then: a clean, merged worktree is removed at once; a discard or a merge
    returns `Outcome::Wait(Waiting::Allow { … }, Some(ASK_WAIT))`; Allow runs the operation and answers its result,
    Deny answers `Reply::err("The person said no.")`.
  - The balloon words: remove `{caller} wants to delete the worktree {name}, with {n} changed files and {m} commits
    not merged.` (`Delete`, `Keep`); merge `{caller} wants to merge {name} ({m} commits) into {into}.` (`Merge`,
    `Cancel`); connect unchanged (`Allow`, `Deny`).
  - Verbs `templum worktree remove <name> [--folder <f>] [--discard] | merge <name> [--folder <f>]`.

- [ ] **Step 1: Write the failing tests**

`requests.rs`: every existing connect test still passes unchanged (the refactor keeps its words and buttons).

`tests/spawn.rs` (Unix, with `git_repo` and `spawn_agent`'s `worktree`):

```rust
#[gpui_kit::test]
fn merge_worktree_asks_the_person_then_merges_and_remove_worktree_removes_it(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let repo = git_repo();
    let spawn = json!({ "agent": "codex", "folder": repo.path(), "worktree": "w" });
    assert!(!call_settled(handle, &ws, cx, &terminal, "spawn_agent", spawn).0); // waits for the Later job
    let worker = spawned(&ws, cx, mine);
    let (_, them) = me(&ws, cx, worker);
    let wt = repo.path().join(".worktrees/w");
    std::fs::write(wt.join("a.txt"), "new\n").unwrap();
    git_run(&wt, &["commit", "-qam", "change"]);
    assert_eq!(
        call(handle, &ws, cx, &terminal, "merge_worktree", json!({ "worktree": "w" })),
        (true, format!("{them} works in it: stop_agent or close_block it first."))
    );
    call(handle, &ws, cx, &terminal, "close_block", json!({ "block": them }));
    std::fs::write(repo.path().join("a.txt"), "the person's\n").unwrap();
    let (failed, text) = call(handle, &ws, cx, &terminal, "merge_worktree", json!({ "worktree": "w" }));
    assert!(failed && text.contains("has changes not committed in"), "{text}");
    git_run(repo.path(), &["checkout", "--", "a.txt"]);
    let answers = call_later(handle, &ws, cx, &terminal, 5, "merge_worktree", json!({ "worktree": "w" }));
    assert!(answers.try_recv().is_err(), "it waits for the person");
    click_balloon(handle, &ws, cx, "Merge"); // as the connect tests press Allow
    settle(handle, cx);
    let (failed, text) = answered(&answers);
    assert!(!failed && text.starts_with("Merged w into "), "{text}");
    assert_eq!(
        call_settled(handle, &ws, cx, &terminal, "remove_worktree", json!({ "worktree": "w" })),
        (false, "Removed the worktree w and its branch.".into())
    );
    assert!(!wt.exists());
}

#[gpui_kit::test]
fn remove_worktree_with_work_in_it_asks_before_discarding_and_keep_keeps_it(cx: &mut TestAppContext) {
    // As above, without the merge: a changed file in the worktree; remove_worktree without discard is refused with
    // "w has 1 changed files and 0 commits not merged into …: merge_worktree merges them, discard: true loses them.";
    // with discard the balloon asks; Keep answers (true, "The person said no.") and the worktree is still there; a
    // second call and Delete removes it. A conflicting merge answers "The merge would conflict in a.txt: merge … into w
    // and resolve them there, then merge again." and leaves no merge under way.
}
```

(`call_settled`, `click_balloon` stand for the existing waits on an `Outcome::Later` call and the connect tests' Allow
press; spell out the second test from its comment.) A merge request and a question on one block wait in turn with "1
of 2" (Review Focus 5).

- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p templum worktree && cargo test -p templum connect`.
  Expected: FAIL to compile (the tools), the connect tests compile and pass before the refactor.
- [ ] **Step 3: Implement** the refactor first (connect tests green), then the tools, words and docs; agent-tools.md's
  Status: "Implemented".
- [ ] **Step 4: Run the full check.** Expected: PASS.
- [ ] **Step 5: By hand:** Claude Code spawns a Codex in a worktree, it commits, Claude Code `close_block`s it and
  `merge_worktree`s it (the balloon, Merge), then `remove_worktree`.
- [ ] **Step 6: Commit**

```bash
git add crates/templum/src/workspace/waits.rs crates/templum/src/balloon.rs crates/templum/src/workspace/ops.rs \
  crates/templum/src/workspace/content.rs crates/templum/src/workspace/bus.rs crates/templum-mcp/src/tools.rs \
  crates/templum-mcp/src/cli.rs crates/templum/src/workspace/tests/requests.rs crates/templum/src/workspace/tests/spawn.rs \
  docs/specs/connections.md docs/specs/git.md docs/specs/agent-tools.md docs/ARCHITECTURE.md
git commit -m "feat(connections): agents remove a worktree, or merge it back once you allow it"
```

(Add `crates/templum/src/workspace/worktrees.rs` and its `mod` line if the tools went there.)

---

## After each batch

- Every minor a review defers goes into `docs/minors.md` under **Connections**, with where it is, what a person sees and
  a fix. Task 3 adds the one the spec names: on Windows a watch reads ConPTY's drawing, so a resize may show a line
  again (a fix: ConPTY's passthrough mode, when alacritty_terminal offers it). Task 7 adds: Pi and Grok Build can't be
  forked yet (a fix: give a fork a `--session-id` Templum picks, once each agent takes one with its fork flag).
- The maintainer opens the pull request from `development`; the batch's bench lines (batches 1, 3, 4) go in it.
