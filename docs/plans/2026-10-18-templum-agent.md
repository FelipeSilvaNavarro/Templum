# Templum Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kanban card 43: a floating pill opens a chat with the Templum Agent, a headless Claude Code with its own MCP
server (`templum control`) that does what the person does by hand, orchestrates agent blocks, and waits for the
person's click before anything dangerous.

**Architecture:** A new UI-free crate, `templum-agent`, runs the engine (Claude Code's `-p` with stream-json both ways),
reads its stream into `ChatEvent`s and folds them into a `Conversation` of rows saved in `<data>/agent/`. The control
server is a second client kind on the existing bus: `templum control` (in `templum-mcp`'s `cli.rs`) says hello as
`control` with a per-start secret, `templum-mcp/src/control.rs` holds its tools and their danger rules, and
`crates/templum/src/workspace/control.rs` serves them through shared `Workspace` operations (`workspace/ops.rs`) that
the block server's tools also call. `workspace/approvals.rs` holds dangerous calls until the person answers in
`chat/approval.rs`. The pill and the chat are window chrome in `crates/templum/src/chat/`, drawn by `render.rs` like the
Git popup.

**Tech Stack:** Rust 2024; GPUI (gpui-fast 0.1.8) through gpui-kit `=0.7.3`: `InputState::auto_grow(1, 8)`,
`TextView` Markdown (as `markdown.rs` uses it), `Icon` with lucide assets (`sparkles`, `orbit`, `circle-stop`,
`message-square-plus`, `chevron-down`, `layout-grid`, `square-terminal`, `bot`, `kanban`, `sticky-note`,
`list-checks`, `cable`, `eye`, `settings`), `Tooltip`, `cx.spawn_in`; `std::process::Command` with piped stdio and
reader threads; `futures::channel::mpsc` (as `workspace/bus.rs`); serde/serde_json; `uuid` v4; the headless tests of
`workspace/tests/`.

**Spec:** [docs/specs/templum-agent.md](../specs/templum-agent.md). Read it whole before any task; UI tasks follow its
"What a person sees" sizes and copy exactly. The block server's tools and tasks:
[docs/specs/connections.md](../specs/connections.md).

## Global Constraints

- **Coordination with card 35.** This branch is rebased on `development` before it merges, and card 35 adds tools to
  the block server meanwhile. Before a task adds a method of the spec's [Shared operations](../specs/templum-agent.md#shared-operations)
  table, `git log development --oneline -20` and `grep -rn "fn <name>" crates/templum/src/workspace/`: when card 35
  already added it, call it and skip the steps that add it. Prefer new files (`workspace/ops.rs`, `workspace/control.rs`,
  `chat/`) over edits to `bus.rs`, `content.rs`, `spawn.rs`, `delegate.rs` and `waits.rs`; edit those only where a task
  says so.
- Names: crate `templum-agent` (`templum_agent`); server `templum-control`; verb `templum control`; config table
  `[templum_agent]`; command `agent` ("Templum Agent"); the reserved asker and `opened_by` word
  `templum_agent::ASKER = "templum-agent"`; the secret's variable `TEMPLUM_CONTROL`; the folder `<data>/agent/` with
  `mcp.json` and `conversation.json`.
- Shortcut `agent`: `ctrl-shift-space` (Linux, Windows), `cmd-shift-space` (macOS).
- Copy, exactly:
  - bridge without its variables: `templum: the control server is only for the Templum Agent` (stderr, exit 2);
  - a control hello with a wrong secret: instructions `This isn't the Templum Agent's process.`, every call fails with
    the same sentence;
  - denied: `The person denied this. Don't try it another way: ask them what they want.` then, when they wrote
    something, a new line `They said: <their words>`;
  - an approval ended by Stop, New chat, the engine's end or quitting: `The person stopped you before answering.`;
  - rows: `Stopped by you.`, `New conversation.`, `<Engine> stopped: <last error line>. Your next message starts it
    again.`, `The turn failed: <message>`, `You denied this.`, `Queued`, approval records `Allowed once`, `Allowed for
    this conversation`, `Denied`;
  - placeholder `Ask Templum to build, run or tidy your canvas…`; rest text `Ask Templum`; suggestions `Set up a
    workspace for a project, with a kanban and an agent per card`, `Catch me up: what's going on?`, `Tidy this
    workspace into a grid`;
  - approval: first line `Templum Agent wants to`, buttons `Allow once`, `Allow for this conversation`, `Deny`, field
    `Or tell it what to do instead`, `Its reason:`; notification `Templum Agent asks: <said>`.
- Sizes: pill 300 × 36 at rest, 640 wide typing (≤ canvas width − 32), radius 18, 8 px above the toolbar; chat 640
  wide, 240 px ≤ height ≤ 70% of the canvas, radius 14, header 36 px; pill grows over 0.18 s, chat rises 12 px over
  0.22 s; glow 1.2 s; approval keys ignored for 0.5 s; mark 10 world units × zoom.
- Colors are `crate::theme::Ui` tokens: `popover`, `hairline`, `accent`, `title`, `title_dim`, `hover`, `shadow`;
  waiting yellow `palette.ansi[3]`, red `palette.ansi[1]`, as the balloon and the kanban use them.
- Every control tool's answer that made or touched something names its id. Its running title is
  `templum_mcp::control::title(name)` followed by `…`.
- Only `crates/templum` uses gpui-kit; `templum-agent` and `templum-mcp` stay UI-free. No new dependency outside the
  tree (`serde`, `serde_json`, `uuid`, `futures`, `tempfile` for tests are there). No `unsafe`.
- Each commit passes `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`, titled as CLAUDE.md says (`feat(agent): …`, `test(agent): …`, `docs: …`), with its test, on the branch
  the card is executed on (`templum-agent`, rebased on `development`, unless the maintainer says `development`). Minors
  left for later go into `docs/minors.md` under a new `## Templum Agent` heading.

## Review Focus

1. **The engine dies while an approval waits** (a crash, a kill from outside): the approval is denied with `The person
   stopped you before answering.`, the running action rows turn failed, a failure row says so, and the next message
   resumes the session. Task 13 pins it.
2. **The target goes away while its approval waits** (the person closes the block, deletes the workspace): Allow
   answers `There's no block <uid>: overview lists them.` and nothing else happens. Task 12 pins it.
3. **The person writes while an approval waits:** the approval is denied with their words, and their message is sent.
   Task 13 pins it.
4. **`conversation.json` is half-written or from a newer Templum** (`"version": 2`): it is set aside as
   `conversation.json.bak` and the chat starts empty; the app never fails to start. Task 3 pins it.
5. **A tool aims at a stopped workspace** (`add_block`, `open_agent` into one): refused with `Workspace <name> is
   stopped: show it first (show).`, nothing opened. Task 10 pins it.

---

## Batch 1: Talk to Templum

### Task 1: The `templum-agent` crate and Claude Code's stream

**Files:**
- Create: `crates/templum-agent/Cargo.toml`, `crates/templum-agent/src/lib.rs`, `crates/templum-agent/src/claude.rs`
- Create: `crates/templum-agent/tests/fixtures/claude-text.jsonl`, `claude-tools.jsonl`, `claude-queued.jsonl`
- Modify: `docs/specs/templum-agent.md` (the **Checked** paragraph under Claude Code)

**Interfaces:**
- Produces (`lib.rs`):
  - `pub const ASKER: &str = "templum-agent";`
  - `#[derive(Clone, Debug, PartialEq)] pub enum ChatEvent { Started { session: String, model: Option<String> },
    Thinking, TextDelta(String), Text(String), ToolStarted { id: String, name: String }, ToolCall { id: String, name:
    String, input: serde_json::Value }, ToolResult { id: String, text: String, is_error: bool }, Taken(String),
    TurnEnded { ms: Option<u64>, cost_usd: Option<f64>, error: Option<String> } }`
- Produces (`claude.rs`): `pub fn parse(line: &str) -> Vec<ChatEvent>`; `pub fn tool_name(name: &str) -> &str` (strips
  `mcp__templum-control__`).

- [ ] **Step 1: Record the fixtures by hand.** In a scratch folder, with Claude Code 2.1.286 (or the installed one: write
  its version), run
  `claude -p --verbose --input-format stream-json --output-format stream-json --include-partial-messages
  --replay-user-messages --tools Read,Glob,Grep --session-id $(uuidgen)` and write on its stdin:
  1. `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Say hello in five words."}]}}` →
     save stdout to `claude-text.jsonl`;
  2. a new session, "Read Cargo.toml and tell me the crate's name." → `claude-tools.jsonl` (a `tool_use` of `Read` and
     its `tool_result`);
  3. a new session, a first message "Count slowly from 1 to 30.", and a second one written while it answers →
     `claude-queued.jsonl` (does the second come back as a replayed `user` line, taken into the turn or after it?).
  Also try the interrupt request on stdin during a turn: `{"type":"control_request","request_id":"r1","request":
  {"subtype":"interrupt"}}`: does the turn end with a `result` and the process stay?
  Trim each file's text fields if long; keep every line's shape.
- [ ] **Step 2: Write what you saw** in the spec's **Checked** paragraph: the version, the line kinds seen, how a
  queued message behaves, and whether the interrupt works (if it does, Task 13's Stop sends it instead of ending the
  process: write that there). If a flag of the spec's command line is refused, fix the command line in the spec.
- [ ] **Step 3: Write the failing tests** in `claude.rs`:

```rust
#[test]
fn a_text_turn_is_its_start_its_words_and_its_end() {
    let events: Vec<ChatEvent> = include_str!("../tests/fixtures/claude-text.jsonl")
        .lines().flat_map(parse).collect();
    assert!(matches!(&events[0], ChatEvent::Started { session, .. } if !session.is_empty()));
    let streamed: String = events.iter().filter_map(|e| match e {
        ChatEvent::TextDelta(t) => Some(t.as_str()), _ => None }).collect();
    let whole = events.iter().find_map(|e| match e { ChatEvent::Text(t) => Some(t.clone()), _ => None }).unwrap();
    assert_eq!(streamed.trim(), whole.trim());
    assert!(matches!(events.last(), Some(ChatEvent::TurnEnded { error: None, .. })));
}

#[test]
fn a_tool_is_started_called_and_answered_under_one_id() { /* claude-tools.jsonl: ToolStarted{id,name:"Read"},
   then ToolCall with the same id and an input holding "file_path", then ToolResult with that id, is_error false */ }

#[test]
fn a_replayed_message_is_taken() { /* claude-queued.jsonl: a Taken("<the second message's text>") */ }

#[test]
fn what_isnt_a_line_it_knows_is_nothing() {
    assert!(parse("running 1 test").is_empty());
    assert!(parse("{\"type\":\"rate_limit_event\"}").is_empty());
    assert!(parse("{\"type\":\"result\",\"is_error\":true,\"result\":\"Overloaded\"}") ==
        vec![ChatEvent::TurnEnded { ms: None, cost_usd: None, error: Some("Overloaded".into()) }]);
    assert_eq!(tool_name("mcp__templum-control__add_block"), "add_block");
    assert_eq!(tool_name("Read"), "Read");
}
```

- [ ] **Step 4: Run** `cargo test -p templum-agent`: fails (no crate code).
- [ ] **Step 5: Implement.** `Cargo.toml` as `templum-mcp`'s (workspace fields, `[lints] workspace = true`),
  description "Runs the Templum Agent's headless CLI agent, reads its stream into a chat, and keeps the conversation.
  No UI.", dependencies `serde`, `serde_json`, `uuid`, `futures` (each with the "already in the tree" comment
  `templum-mcp` uses), dev `tempfile`. `parse` reads the [stream table](../specs/templum-agent.md#the-stream): one line
  may give several events (an `assistant` line with text and a `tool_use`); a `user` line whose content is text (not a
  `tool_result`) is the replay, `Taken`. A `tool_result`'s `content` may be a string or a list of `{type:"text"}`:
  join the texts.
- [ ] **Step 6: Run** `cargo test -p templum-agent`: passes.
- [ ] **Step 7: Commit** `feat(agent): read Claude Code's stream for the Templum Agent's chat`.

### Task 2: Claude Code's words and the control server's file

**Files:**
- Modify: `crates/templum-agent/src/claude.rs`

**Interfaces:**
- Produces: `pub enum Session { New(String), Resume(String) }`; `pub struct Options { pub session: Session, pub model:
  Option<String>, pub effort: Option<String>, pub add_dirs: Vec<std::path::PathBuf>, pub mcp_file: PathBuf }`;
  `pub fn args(o: &Options) -> Vec<String>`; `pub const ROLE: &str` (the spec's role, verbatim, one paragraph);
  `pub fn write_mcp(dir: &Path, exe: &Path) -> std::io::Result<PathBuf>` (`<dir>/mcp.json`, written only when its
  text changed); `pub fn message(text: &str) -> String` (the stream-json user line, ending in `\n`).

- [ ] **Step 1: Write the failing tests:**

```rust
#[test]
fn a_new_conversation_gets_every_word_in_order() {
    let o = Options { session: Session::New("u1".into()), model: Some("opus".into()), effort: Some("high".into()),
        add_dirs: vec!["/p/a".into(), "/p/b".into()], mcp_file: "/d/agent/mcp.json".into() };
    let a = args(&o);
    assert_eq!(&a[..10], ["-p", "--verbose", "--input-format", "stream-json", "--output-format", "stream-json",
        "--include-partial-messages", "--replay-user-messages", "--session-id", "u1"]);
    for pair in [["--model", "opus"], ["--effort", "high"], ["--tools", "Read,Glob,Grep"],
        ["--add-dir", "/p/a"], ["--add-dir", "/p/b"], ["--mcp-config", "/d/agent/mcp.json"],
        ["--allowedTools", "mcp__templum-control"], ["--permission-prompts", "none"], ["--append-system-prompt", ROLE]] {
        assert!(a.windows(2).any(|w| w == pair), "{pair:?} in {a:?}");
    }
    assert!(a.contains(&"--strict-mcp-config".to_string()));
}

#[test]
fn a_resumed_one_names_its_session_and_no_model_when_none() {
    let a = args(&Options { session: Session::Resume("u1".into()), model: None, effort: None, add_dirs: vec![],
        mcp_file: "/m.json".into() });
    assert!(a.windows(2).any(|w| w == ["--resume", "u1"]));
    assert!(!a.iter().any(|w| w == "--session-id" || w == "--model" || w == "--effort" || w == "--add-dir"));
}

#[test]
fn the_server_file_names_templum_control_and_is_left_alone_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_mcp(dir.path(), Path::new("/opt/templum")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(v["mcpServers"]["templum-control"]["args"], serde_json::json!(["control"]));
    assert_eq!(v["mcpServers"]["templum-control"]["timeout"], 604_800_000);
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_mcp(dir.path(), Path::new("/opt/templum")).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
}

#[test]
fn a_message_is_one_json_line() {
    let line = message("two\nlines \"quoted\"");
    assert!(line.ends_with('\n') && line.matches('\n').count() == 1);
    let v: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(v["message"]["content"][0]["text"], "two\nlines \"quoted\"");
}
```

- [ ] **Step 2: Run** `cargo test -p templum-agent claude`: fails.
- [ ] **Step 3: Implement.** The `" (deleted)"` suffix is stripped from `exe` as `templum_usage::claude::mcp_file` does;
  write through a temporary file renamed over it.
- [ ] **Step 4: Run** the tests: pass.
- [ ] **Step 5: Commit** `feat(agent): the words Claude Code is started with as the Templum Agent`.

### Task 3: The conversation

**Files:**
- Create: `crates/templum-agent/src/conversation.rs`

**Interfaces:**
- Consumes: `ChatEvent`, `claude::tool_name`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum Row { Person { text: String, queued: bool }, Text { text: String }, Thinking, Action { id: String, tool:
    String, input: Value, state: ActionState, said: String, target: Option<Target> }, Reads { count: u32 }, Approval {
    said: String, answer: String }, End { ms: Option<u64>, cost_usd: Option<f64> }, Failure { text: String }, Note {
    text: String } }` (optional fields `#[serde(default)]`).
  - `pub enum ActionState { Running, Done, Failed, Denied }` (serde snake_case);
    `pub struct Target { pub workspace: Option<String>, pub block: Option<String> }`;
    `pub struct Own { pub workspaces: Vec<String>, pub blocks: Vec<String>, pub boards: Vec<u64>, pub cards: Vec<u64>,
    pub notes: Vec<u64>, pub checklists: Vec<u64> }` (all `Default`, serde).
  - `pub struct Conversation { pub version: u32, pub engine: String, pub session: Option<String>, pub rows:
    Vec<Row>, pub own: Own, #[serde(skip)] pub working: bool }` with `pub fn new(engine: &str) -> Self`, `pub fn
    person(&mut self, text: &str)` (queued when `working`; sets `working`), `pub fn apply(&mut self, e: ChatEvent)`,
    `pub fn touched(&mut self, tool: &str, input: &Value, target: Target)`, `pub fn ended(&mut self, why: &str)` (the
    engine is gone: running actions Failed, a `Failure { text: why }`, `working = false`), `pub fn note(&mut self,
    text: &str)`, `pub fn cost(&self) -> f64`, `pub fn load(path: &Path) -> Load` with `pub enum Load { None,
    Some(Conversation), Unreadable }` (Unreadable after renaming the file to `conversation.json.bak`), `pub fn
    save(&self, path: &Path) -> std::io::Result<()>`.
  - `pub fn is_read(tool: &str) -> bool`: `overview`, `list_agents`, `read_terminal`, `list_cards`, `read_note`,
    `read_checklist`, `Read`, `Glob`, `Grep` (after `tool_name`).

- [ ] **Step 1: Write the failing tests:**

```rust
fn turn(lines: &str) -> Conversation {
    let mut c = Conversation::new("claude");
    c.person("hi");
    for e in lines.lines().flat_map(crate::claude::parse) { c.apply(e) }
    c
}

#[test]
fn deltas_then_the_whole_text_make_one_text_row() {
    let mut c = Conversation::new("claude");
    c.person("hi");
    for e in [ChatEvent::Thinking, ChatEvent::TextDelta("Hel".into()), ChatEvent::TextDelta("lo".into()),
              ChatEvent::Text("Hello!".into()), ChatEvent::TurnEnded { ms: Some(1200), cost_usd: Some(0.04), error: None }] {
        c.apply(e);
    }
    assert_eq!(c.rows, vec![Row::Person { text: "hi".into(), queued: false }, Row::Text { text: "Hello!".into() },
        Row::End { ms: Some(1200), cost_usd: Some(0.04) }]);
    assert!(!c.working);
}

#[test]
fn an_action_runs_then_says_the_first_line_of_its_answer() {
    let mut c = Conversation::new("claude");
    c.person("make it");
    c.apply(ChatEvent::ToolStarted { id: "t1".into(), name: "mcp__templum-control__create_workspace".into() });
    c.apply(ChatEvent::ToolCall { id: "t1".into(), name: "mcp__templum-control__create_workspace".into(),
        input: json!({"name": "API"}) });
    assert!(matches!(&c.rows[1], Row::Action { state: ActionState::Running, tool, .. } if tool == "create_workspace"));
    c.touched("create_workspace", &json!({"name": "API"}), Target { workspace: Some("w3".into()), block: None });
    c.apply(ChatEvent::ToolResult { id: "t1".into(), text: "Created workspace API (w3).\nmore".into(), is_error: false });
    assert!(matches!(&c.rows[1], Row::Action { state: ActionState::Done, said, target: Some(t), .. }
        if said == "Created workspace API (w3)." && t.workspace.as_deref() == Some("w3")));
}

#[test]
fn reads_in_a_row_fold_into_one() { /* ToolStarted Read, ToolStarted mcp__templum-control__overview,
   ToolStarted Glob → one Row::Reads { count: 3 }; then a text and another Read → a second Reads { count: 1 } */ }

#[test]
fn a_message_while_it_works_is_queued_until_taken() {
    let mut c = Conversation::new("claude");
    c.person("first");
    c.person("second");
    assert_eq!(c.rows[1], Row::Person { text: "second".into(), queued: true });
    c.apply(ChatEvent::Taken("second".into()));
    assert_eq!(c.rows[1], Row::Person { text: "second".into(), queued: false });
}

#[test]
fn the_engine_ending_fails_what_runs() { /* person, ToolStarted non-read, ended("Claude Code stopped: x. Your next
   message starts it again.") → the action Failed, last row Failure with that text, working false */ }

#[test]
fn the_file_round_trips_without_thinking_and_with_running_actions_failed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("conversation.json");
    let mut c = turn(include_str!("../tests/fixtures/claude-tools.jsonl"));
    c.own.blocks.push("b7k2m".into());
    c.session = Some("u1".into());
    c.save(&path).unwrap();
    let Load::Some(back) = Conversation::load(&path) else { panic!() };
    assert_eq!((back.session.as_deref(), back.own.blocks.clone()), (Some("u1"), vec!["b7k2m".to_string()]));
    assert!(!back.rows.contains(&Row::Thinking));
}

#[test]
fn an_unreadable_or_newer_file_is_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("conversation.json");
    for text in ["{\"version\":1,\"rows\":[", "{\"version\":2,\"engine\":\"claude\",\"rows\":[]}"] {
        std::fs::write(&path, text).unwrap();
        assert!(matches!(Conversation::load(&path), Load::Unreadable));
        assert!(path.with_extension("json.bak").exists() && !path.exists());
    }
    assert!(matches!(Conversation::load(&path), Load::None));
}
```

- [ ] **Step 2: Run** `cargo test -p templum-agent conversation`: fails.
- [ ] **Step 3: Implement.** `apply`: `Thinking` pushes a `Thinking` row unless the last is one, and any other event
  first drops a trailing `Thinking`; `TextDelta` appends to the last row when it is the turn's open `Text`, else pushes
  one; `Text` replaces the open text row's text (pushes when none); any other event closes the open text; `ToolStarted`
  of a read adds to the last `Reads` (or pushes `Reads { count: 1 }`), else pushes a running `Action` (`tool` is
  `tool_name(name)`); `ToolCall` sets that action's `input`; `ToolResult` sets `Done` (or `Failed` when `is_error`)
  and `said` to the result's first line; `Taken(t)` unqueues the oldest queued `Person` with that text; `Started`
  sets `session`; `TurnEnded` pushes `End` and, with an error, `Failure { text: "The turn failed: <error>" }`, then
  `working = false` unless a queued `Person` waits. `touched` gives the oldest action of that tool and input without a
  target its target. `save` writes `version: 1`, skips `Thinking`, turns `Running` into `Failed` with `said` `Templum
  quit.`, through a temporary file renamed over it.
- [ ] **Step 4: Run** the tests: pass.
- [ ] **Step 5: Commit** `feat(agent): keep the Templum Agent's conversation as rows, saved across restarts`.

### Task 4: The engine's process and a fake one for tests

**Files:**
- Create: `crates/templum-agent/src/engine.rs`, `crates/templum-agent/src/fake.rs`

**Interfaces:**
- Produces (`engine.rs`): `pub struct Launch { pub program: PathBuf, pub args: Vec<String>, pub cwd: PathBuf, pub
  env: Vec<(String, String)>, pub env_remove: Vec<String> }`; `pub enum EngineLine { Out(String), Err(String), Ended
  }`; `pub struct Engine`; `impl Engine { pub fn start(launch: &Launch, lines:
  futures::channel::mpsc::UnboundedSender<EngineLine>) -> std::io::Result<Engine>; pub fn send(&mut self, line: &str)
  -> std::io::Result<()>; pub fn stop(&mut self) }`; `Drop` stops it.
- Produces (`fake.rs`): `pub const VAR: &str = "TEMPLUM_FAKE_ENGINE";` `pub fn run(script: &str, input: impl BufRead,
  output: impl Write)` (for each line read, writes the next turn of `script`, turns separated by a line `---`, flushing
  each line; after the last turn it reads until EOF); `pub fn launch(test: &str, script: &Path, cwd: &Path) -> Launch`
  (the current test binary, args `[test, "--exact", "--ignored", "--nocapture", "--test-threads=1", "-q"]`, env
  `[(VAR, script)]`).
- Every test binary that uses the fake engine defines this test, which `fake::launch` names:

```rust
#[test]
#[ignore = "the fake engine the engine tests start"]
fn fake_engine() {
    if let Ok(script) = std::env::var(templum_agent::fake::VAR) {
        let script = std::fs::read_to_string(script).unwrap();
        templum_agent::fake::run(&script, std::io::stdin().lock(), std::io::stdout().lock());
    }
}
```

- [ ] **Step 1: Write the failing tests** in `engine.rs` (with the `fake_engine` test above in its `mod tests`, so its
  name is `engine::tests::fake_engine`):

```rust
#[test]
fn a_message_gets_its_turns_lines_in_order_then_stop_ends_it() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("turns.jsonl");
    std::fs::write(&script, "{\"n\":1}\n{\"n\":2}\n---\n{\"n\":3}\n").unwrap();
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let mut engine = Engine::start(&crate::fake::launch("engine::tests::fake_engine", &script, dir.path()), tx).unwrap();
    engine.send("{\"m\":1}\n").unwrap();
    let mut outs = Vec::new();
    while outs.len() < 2 {
        match futures::executor::block_on(rx.next()).unwrap() {
            EngineLine::Out(l) if l.starts_with('{') => outs.push(l),
            _ => {}
        }
    }
    assert_eq!(outs, ["{\"n\":1}", "{\"n\":2}"]);
    engine.stop();
    // Every later line is the end, or the test harness's own words, never a turn's.
    let rest: Vec<EngineLine> = futures::executor::block_on(rx.collect());
    assert!(matches!(rest.last(), Some(EngineLine::Ended)));
    assert!(!rest.iter().any(|l| matches!(l, EngineLine::Out(o) if o == "{\"n\":3}")));
}
```

  and in `fake.rs`: `run` on `"a\n---\nb\nc\n"` with input `"x\ny\n"` writes `"a\nb\nc\n"`.
- [ ] **Step 2: Run** `cargo test -p templum-agent engine fake`: fails.
- [ ] **Step 3: Implement.** `start`: `Command::new(program)` with piped stdin, stdout and stderr, `current_dir`, the
  env added and removed; on Windows `creation_flags(0x0800_0000)` (no console window; through
  `std::os::windows::process::CommandExt`), and a program ending in `.cmd` or `.bat` runs as `cmd /C <program>
  <args>`; one thread reads stdout lines into `Out` then sends `Ended` at its end, one reads stderr into `Err`. `send`
  writes and flushes. `stop` kills the child and waits for it (so the stdout thread sees the end).
- [ ] **Step 4: Run** the tests on your OS: pass.
- [ ] **Step 5: Commit** `feat(agent): run the Templum Agent's engine as a child process`.

### Task 5: The control server's tools and their danger

**Files:**
- Create: `crates/templum-mcp/src/control.rs`
- Modify: `crates/templum-mcp/src/lib.rs` (`pub mod control;`)

**Interfaces:**
- Consumes: `tools::{Call, Spawn, list, parse}`.
- Produces:
  - `pub const SERVER: &str = "templum-control";`
  - `pub enum Kind { Terminal, Note, Checklist, Kanban, Editor }`;
    `pub struct AddBlock { pub workspace: String, pub kind: Kind, pub folder: Option<String>, pub board:
    Option<String>, pub columns: Vec<String>, pub text: Option<String>, pub near: Option<String> }`;
    `pub struct Opening { pub workspace: String, pub spawn: tools::Spawn, pub connect: Vec<String>, pub near:
    Option<String> }`.
  - `pub enum ControlCall { Block(tools::Call), Overview, ListAgents, CreateWorkspace { name: String, folder:
    Option<String>, color: Option<String> }, Show { workspace: Option<String>, block: Option<String> },
    AddBlock(AddBlock), OpenAgent(Opening), Connect { a: String, b: String }, Disconnect { a: String, b: String },
    CloseBlock { block: String } }`; `pub struct Parsed { pub call: ControlCall, pub reason: Option<String> }`.
  - `pub const SHARED: [&str; 14]` = `read_terminal`, `list_cards`, `read_note`, `read_checklist`, `create_card`,
    `edit_card`, `move_card`, `comment_card`, `append_note`, `add_items`, `check_item`, `send_to_terminal`,
    `send_keys`, `notify`: parsed by `tools::parse` into `ControlCall::Block`, their schemas taken from
    `tools::list()`.
  - `pub fn list() -> Value`; `pub fn known(name: &str) -> bool`; `pub fn parse(name: &str, args: &Value) ->
    Result<Parsed, String>`; `pub fn title(name: &str) -> &'static str`.
  - `pub enum Rule { Free, Ask, AskOwnWhenLosing, AskOwn, AskEach }`, `pub enum Level { Ask, AskEach }`, `pub fn
    rule(call: &ControlCall) -> Rule`, `impl Rule { pub fn level(self, own: bool, losing: bool) -> Option<Level> }`.
    Batch 1: `send_to_terminal`, `send_keys` → `Ask`; `close_block` → `AskOwnWhenLosing`; the rest `Free`.
- Titles: overview "Looking at the canvas", list_agents "Looking at the agents", read_terminal "Reading a terminal",
  list_cards "Reading a board", read_note "Reading a note", read_checklist "Reading a checklist", create_workspace
  "Creating a workspace", show "Showing", add_block "Adding a block", create_card "Adding a card", edit_card "Editing a
  card", move_card "Moving a card", comment_card "Commenting on a card", append_note "Writing in a note", add_items
  "Adding items", check_item "Checking an item", open_agent "Opening an agent", connect "Connecting blocks", disconnect
  "Disconnecting blocks", close_block "Closing a block", send_to_terminal "Typing into a terminal", send_keys "Pressing
  keys", notify "Notifying you".

- [ ] **Step 1: Write the failing tests:**

```rust
#[test]
fn every_tool_parses_its_example_and_names_what_is_missing() {
    let examples = [
        ("overview", json!({})), ("list_agents", json!({})),
        ("create_workspace", json!({"name": "API", "folder": "~/proj"})),
        ("show", json!({"workspace": "API", "block": "b7k2m"})),
        ("add_block", json!({"workspace": "w3", "kind": "kanban", "board": "Sprint", "columns": ["To do", "Done"]})),
        ("open_agent", json!({"agent": "codex", "workspace": "w3", "folder": "~/proj", "prompt": "Take card 4",
            "connect": ["k9x1d"], "model": "gpt-5.6", "effort": "high"})),
        ("connect", json!({"a": "b7k2m", "b": "k9x1d"})), ("disconnect", json!({"a": "b7k2m", "b": "k9x1d"})),
        ("close_block", json!({"block": "b7k2m", "reason": "done"})),
        ("send_to_terminal", json!({"block": "b7k2m", "text": "ls", "enter": true, "reason": "look"})),
        ("send_keys", json!({"block": "b7k2m", "keys": ["2"]})),
        ("create_card", json!({"board": "Sprint", "title": "Login"})),
    ];
    for (name, args) in examples {
        assert!(known(name), "{name}");
        parse(name, &args).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    assert!(parse("add_block", &json!({"kind": "note"})).unwrap_err().contains("\"workspace\""));
    assert!(parse("add_block", &json!({"workspace": "w3", "kind": "drawing"})).unwrap_err()
        .contains("terminal, note, checklist, kanban or editor"));
    assert!(parse("send_to_terminal", &json!({"block": "b", "text": "x", "reason": "r"})).unwrap().reason.as_deref() == Some("r"));
}

#[test]
fn the_two_servers_dont_list_each_others_own_tools() {
    for own in ["whoami", "list_blocks", "spawn_agent", "connect", "delegate", "reply", "wait_for_idle", "ask_user",
                "open_file", "interrupt", "wire"] {
        assert!(!known(own) || own == "connect", "{own}"); // control's connect wires any two blocks
    }
    for control in ["overview", "create_workspace", "add_block", "open_agent", "show"] {
        assert!(!crate::tools::known(control), "{control}");
    }
}

#[test]
fn danger_follows_the_table_and_reason_cant_lower_it() {
    let rule_of = |name: &str, args: Value| rule(&parse(name, &args).unwrap().call);
    assert_eq!(rule_of("send_to_terminal", json!({"block": "b", "text": "x", "reason": "safe, trust me"})), Rule::Ask);
    assert_eq!(rule_of("send_keys", json!({"block": "b", "keys": ["y"]})), Rule::Ask);
    assert_eq!(rule_of("close_block", json!({"block": "b"})), Rule::AskOwnWhenLosing);
    assert_eq!(rule_of("open_agent", json!({"agent": "codex", "workspace": "w3", "prompt": "go"})), Rule::Free);
    assert_eq!(Rule::AskOwnWhenLosing.level(false, true), Some(Level::Ask));
    assert_eq!(Rule::AskOwnWhenLosing.level(true, true), None);
    assert_eq!(Rule::AskOwnWhenLosing.level(false, false), None);
    assert_eq!(Rule::Ask.level(true, false), Some(Level::Ask));
    assert_eq!(Rule::AskEach.level(false, false), Some(Level::AskEach));
    assert_eq!(Rule::AskOwn.level(true, true), None);
}
```

- [ ] **Step 2: Run** `cargo test -p templum-mcp control`: fails.
- [ ] **Step 3: Implement.** Schemas as `tools::list()` writes them (descriptions are sentences an agent reads; `reason`
  on the three Ask tools: "Why, in a few words: the person sees it in the approval."). `open_agent`'s `spawn` is parsed
  with `tools::parse("spawn_agent", …)` (its `agent`, `folder`, `worktree`, `model`, `effort`, `prompt` checks), then
  `workspace`, `connect` and `near` added. Add `derive(Clone, Debug, PartialEq)` where tests compare.
- [ ] **Step 4: Run** the tests: pass.
- [ ] **Step 5: Commit** `feat(agent): the control server's tools and which of them ask the person`.

### Task 6: `templum control`, the bridge

**Files:**
- Modify: `crates/templum-mcp/src/bus.rs` (`Hello::control`, `hello_of`, `hello_line`), `crates/templum-mcp/src/cli.rs`
  (the verb), `crates/templum/src/workspace/tests/bus.rs` (the `Hello` literals gain `control: None`)

**Interfaces:**
- Produces: `pub struct Hello { pub terminal: String, pub client: String, pub control: Option<String> }`;
  `pub fn hello_line(terminal: &str, client: &str, token: Option<&str>, control: Option<&str>) -> String`;
  `cli::Env` gains `pub control: Option<String>` (from `TEMPLUM_CONTROL` in `of_process`); `"control"` is a verb.

- [ ] **Step 1: Write the failing tests** in `cli.rs`'s and `bus.rs`'s test modules:
  - `templum_control_without_its_variable_says_so_and_exits_2`: `run(["control"], Env { bus: Some(addr), terminal:
    None, control: None }, …)` returns 2, stdout empty, stderr `templum: the control server is only for the Templum
    Agent\n`; the same with `bus: None`.
  - `templum_control_says_hello_with_its_secret_then_bridges`: against a test bus (as the `mcp` bridge test does), the
    first message the UI side gets has `hello.client == "control"`, `hello.control == Some("s3cret")` and
    `hello.terminal == ""`, and a request written on stdin comes through.
  - `a_hello_without_control_has_none`: the existing hello test asserts `control: None`.
- [ ] **Step 2: Run** `cargo test -p templum-mcp`: fails.
- [ ] **Step 3: Implement.** In `run`, `"control"` takes no arguments and needs `env.bus` (not empty) and
  `env.control`, else the sentence above and 2; it opens the stream with `hello_line("", "control", token,
  Some(secret))` and runs `bridge`. Every existing `hello_line` call passes `None`.
- [ ] **Step 4: Run** `cargo test --workspace`: passes (the six `Hello` literals updated).
- [ ] **Step 5: Commit** `feat(agent): templum control, the Templum Agent's own MCP bridge`.

### Task 7: `[templum_agent]`, its settings section and its shortcut

**Files:**
- Modify: `crates/templum/src/config.rs` (the table, its checks, the template), `crates/templum/src/keys.rs`
  (`Command::Agent`), `crates/templum/src/settings/mod.rs` and `sections.rs` (the section after Connections)
- Create: `crates/templum/src/settings/templum_agent.rs`
- Test: `config.rs` tests, `crates/templum/src/workspace/tests/settings_edits.rs`

**Interfaces:**
- Produces: `pub(crate) struct TemplumAgentConfig { pub agent: String, pub model: String, pub effort: String, pub
  pill: bool, pub max_agents: u32 }` (defaults `"claude"`, `""`, `""`, `true`, `4`), `Config::templum_agent`;
  `Command::Agent` (name `agent`, title "Templum Agent", defaults `ctrl-shift-space` / `cmd-shift-space`).

- [ ] **Step 1: Write the failing tests:**
  - `config.rs`: `the_templum_agent_runs_on_claude_with_its_pill_unless_set` (empty file → `agent == "claude"`,
    `pill`, `max_agents == 4`); `[templum_agent]\nagent = "gemini"\n` fails with `templum_agent.agent is claude`;
    `effort = "ultra"` fails naming `templum_agent.effort` and Claude Code's efforts; `model = "opus; rm -rf /"` fails
    naming `templum_agent.model`; `max_agents = 17` fails `templum_agent.max_agents must be between 1 and 16`; the
    template contains `[templum_agent]` and every key commented, as the spec's [Config](../specs/templum-agent.md#config)
    writes it.
  - `keys.rs`: `Command::Agent.defaults(false) == ["ctrl-shift-space"]`, `defaults(true) == ["cmd-shift-space"]`.
  - `settings_edits.rs`: `the_templum_agent_section_edits_its_keys`: switching the pill off writes
    `templum_agent.pill = false`; picking an effort writes it; Default removes the key.
- [ ] **Step 2: Run** `cargo test -p templum config keys settings_edits`: fails.
- [ ] **Step 3: Implement** as `[connections]` is (struct, `Default`, `range` checks, template lines). Model check:
  `templum_terminal::Agent`'s model rule (the one `launch_args` applies); effort: Claude Code's list from the same
  table. The section's rows follow `settings/connections.rs`'s pattern; the Agent dropdown lists Claude Code (with
  "(not installed)" when `templum_terminal::installed("claude")` is false).
- [ ] **Step 4: Run** the tests: pass.
- [ ] **Step 5: Commit** `feat(agent): Settings ▸ Templum Agent: its agent, model, effort and pill`.

### Task 8: Serving the control server: the gatekeeping, `overview` and the reads

**Files:**
- Create: `crates/templum/src/chat/mod.rs` (`TemplumAgent`, the state; the UI comes later), `crates/templum/src/workspace/control.rs`,
  `crates/templum/src/workspace/tests/templum_agent.rs`
- Modify: `crates/templum/src/main.rs` (`mod chat;`), `crates/templum/src/workspace/mod.rs` (one field,
  `pub(crate) templum_agent: crate::chat::TemplumAgent`, and `mod control;`), `crates/templum/src/workspace/bus.rs`
  (one arm at `serve`'s top), `crates/templum/src/workspace/tests/mod.rs` (`mod templum_agent;`)

**Interfaces:**
- Consumes: `templum_mcp::control::{list, known, parse, Parsed, ControlCall, SERVER}`, `templum_agent::Conversation`.
- Produces:
  - `pub(crate) struct TemplumAgent { pub conversation: Conversation, pub secret: String, pub grants:
    BTreeSet<String>, pub approvals: Vec<Approval>, pub next_approval: u64, pub open: bool, pub unread: bool, pub
    glow: HashMap<String, Instant> }` with `Default` (`secret` is a new `uuid` v4 simple string, so no stale hello
    matches before the engine's first start; `conversation` is `Conversation::new("claude")`). (`Approval` is defined
    in Task 12; until then the field is absent.)
  - `impl Workspace { pub(super) fn serve_control(&mut self, hello: Hello, conn: u64, incoming: Incoming, reply:
    Replier, window: &mut Window, cx: &mut Context<Self>); fn control_call(&mut self, parsed: Parsed, window: &mut
    Window, cx: &mut Context<Self>) -> (Outcome, Option<templum_agent::Target>) }`.
  - Test helper in `tests/templum_agent.rs`: `fn control(handle, ws, cx, tool: &str, args: Value) -> (bool, String)`
    (a control client with the workspace's current secret, as `bus.rs`'s `call` does), and `fn control_as(…, secret:
    &str, method, params) -> Value`.

- [ ] **Step 1: Write the failing tests** in `tests/templum_agent.rs`:

```rust
#[gpui_kit::test]
fn a_control_client_with_the_secret_gets_every_workspace(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let secret = ws.read_with(cx, |ws, _| ws.templum_agent.secret.clone());
    let init = control_as(handle, &ws, cx, &secret, "initialize", json!({"protocolVersion": "2025-11-25"}));
    assert_eq!(init["result"]["serverInfo"]["name"], "templum-control");
    let text = init["result"]["instructions"].as_str().unwrap();
    assert!(text.contains(", shown: ") && text.contains("Call overview for the blocks."), "{text}");
    let tools = control_as(handle, &ws, cx, &secret, "tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "overview"));
}

#[gpui_kit::test]
fn a_wrong_secret_is_not_the_templum_agent(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let init = control_as(handle, &ws, cx, "old", "initialize", json!({}));
    assert_eq!(init["result"]["instructions"], "This isn't the Templum Agent's process.");
    let called = control_as(handle, &ws, cx, "old", "tools/call", json!({"name": "overview", "arguments": {}}));
    assert_eq!(called["result"]["isError"], true);
}

#[gpui_kit::test]
fn overview_lists_the_blocks_of_every_workspace(cx: &mut TestAppContext) {
    /* two workspaces (a second made with `new_space`), a terminal in each: overview names both workspaces by name and
       id, each block's id and kind, and which workspace is shown */
}

#[gpui_kit::test]
fn the_reads_need_no_wire(cx: &mut TestAppContext) {
    /* a note block with text "plan" and a terminal that printed "hello": read_note and read_terminal answer them with no
       wire anywhere; list_agents names "claude" */
}
```

- [ ] **Step 2: Run** `cargo test -p templum templum_agent`: fails.
- [ ] **Step 3: Implement.** In `serve`, before anything else: `if hello.client == "control" { return
  self.serve_control(hello, conn, incoming, reply, window, cx); }`. `serve_control` answers `initialize` (through
  `tools::initialize_result` with `serverInfo.name` replaced by `SERVER`), `ping`, `tools/list` (`control::list()`),
  `tools/call`; compares `hello.control` with `templum_agent.secret` at every message. When a call returns a target,
  `serve_control` calls `templum_agent.conversation.touched(name, &arguments, target)` so the chat's row can show it. The reads call today's code
  paths: `overview` and its workspace lines are new (`block_lines`'s cleaning: `clean_title`); `read_terminal`,
  `read_note`, `read_checklist`, `list_cards` reach Task 9's shared operations, so in this task they reuse the logic
  `bus.rs`'s and `content.rs`'s tools run, through a `target`-free lookup of the block in any workspace
  (`fn find_block(&self, uid: &str) -> Option<(String, BlockId)>` in `control.rs`); `notify` uses `notify_from` with no
  block to reveal (a click opens the chat once Task 14 lands). Answers for an unknown block: `There's no block <uid>:
  overview lists them.`
- [ ] **Step 4: Run** the tests: pass.
- [ ] **Step 5: Commit** `feat(agent): Templum serves the control server, its overview and its reads`.

### Task 9: Shared operations: cards, notes and checklists without a wire

**Files:**
- Create: `crates/templum/src/workspace/ops.rs`
- Modify: `crates/templum/src/workspace/content.rs` (its tools call `ops.rs` after their wire check),
  `crates/templum/src/workspace/control.rs`, `crates/templum/src/workspace/mod.rs` (`mod ops;`)
- Test: `crates/templum/src/workspace/tests/templum_agent.rs`; `tests/content.rs` stays green unchanged

**Interfaces:**
- Look on `development` first ([Global Constraints](#global-constraints)): card 35 may have split `content.rs` already.
- Produces (`ops.rs`, `impl Workspace`, `pub(super)`): `cards_of(&self, board: u64, card: Option<u64>, viewer:
  Option<&Caller>, cx: &App) -> Reply`; `card_new(&mut self, board: u64, new: NewCard, cx) -> Result<u64, Reply>`;
  `card_edit(&mut self, card: u64, edit: CardEdit, caller: Option<&Caller>, cx) -> Reply`; `card_move(&mut self, card:
  u64, column: &str, cx) -> Reply`; `card_comment(&mut self, card: u64, text: &str, by: &str, cx) -> Reply`;
  `note_text(&self, block: (String, BlockId), cx) -> Reply`; `note_append(&mut self, block: (String, BlockId), text:
  &str, window, cx) -> Reply`; `checklist_text`, `item_check`, `items_add` likewise; `board_named(&self, name: &str)
  -> Result<u64, Reply>` (any board by name or id). `by` is a block id or `templum_agent::ASKER`; a comment by
  `ASKER` shows as "Templum Agent".

- [ ] **Step 1: Write the failing tests:** `cards_go_on_any_board_and_comments_are_signed_templum_agent`
  (`create_card` on a board no kanban shows, by name; `move_card` to Review answers without asking for a comment;
  `comment_card` adds a comment whose author is `templum-agent`, and `list_cards` with `card` shows "Templum Agent");
  `notes_and_checklists_take_writes_without_a_wire` (`append_note` shows in the open note; `add_items` then
  `check_item` on a checklist block).
- [ ] **Step 2: Run:** fails.
- [ ] **Step 3: Implement** by moving each tool's body after its wire check out of `content.rs` into `ops.rs`, so
  `content.rs` keeps its checks and calls them; `control.rs` calls them with no check.
- [ ] **Step 4: Run** `cargo test -p templum content templum_agent`: pass.
- [ ] **Step 5: Commit** `refactor: card, note and checklist operations any server can call` then, if the control
  tools are a separate change, `feat(agent): the Templum Agent writes cards, notes and checklists`.

### Task 10: Workspaces, blocks and wires

**Files:**
- Modify: `crates/templum/src/workspace/ops.rs`, `crates/templum/src/workspace/control.rs`
- Test: `crates/templum/src/workspace/tests/templum_agent.rs`

**Interfaces:**
- Look on `development` first for `add_block_in` (card 35's `create_block`).
- Produces (`ops.rs`): `pub(crate) enum NewBlock { Terminal { folder: Option<PathBuf> }, Note { text: String },
  Checklist { name: String, items: String }, Kanban { board: u64 }, Editor { folder: PathBuf } }` (card 35 adds
  `command` and `background` to `Terminal`); `pub(crate) enum Place { Near(BlockId), Free }`; `pub(super) fn
  add_block_in(&mut self, space: &str, block: NewBlock, place: Place, window, cx) -> Result<BlockId, Reply>` (through
  `in_space`; `Near` uses `Space::room_beside` with the block's rect, `Free` `canvas.place_new`; the keyboard, the
  selection and the camera stay); `pub(super) fn space_of(&self, name_or_id: &str) -> Result<String, Reply>` (by id,
  then by name in either case; a stopped one is refused with `Workspace <name> is stopped: show it first (show).`
  unless the caller is `show`).
- Control tools: `create_workspace` (`make_space`, then its name, folder and color; `Created workspace <name>
  (<id>).`), `show` (`switch_to` its index, then `reveal`), `add_block` (a kanban's board by `board_named`, or a new one
  with `columns`, default `To do`, `Doing`, `Review`, `Done`), `connect` and `disconnect` (`connect`/`disconnect`
  through `in_space`; two blocks of different workspaces: `<a> and <b> aren't in one workspace.`). Each records what it
  made in `templum_agent.conversation.own` and returns its `Target`.

- [ ] **Step 1: Write the failing tests:**
  - `create_workspace_makes_one_without_showing_it`: the shown workspace and the focused block are unchanged; the
    answer is `Created workspace API (w2).`; `own.workspaces == ["w2"]`.
  - `add_block_opens_each_kind_where_it_covers_nothing`: in `w2`, a kanban (new board "Sprint" with the default
    columns), a note with "plan", a checklist "Steps\nOne\nTwo", a terminal, an editor on a temporary folder; no two
    rects overlap; `near` puts the note beside the kanban; the camera's position is unchanged.
  - `a_stopped_workspace_is_refused`: `stop_now` a workspace, then `add_block` into it answers `Workspace <name> is
    stopped: show it first (show).` and adds nothing; `show` it, then `add_block` works.
  - `show_switches_and_reveals`; `connect_wires_two_blocks_and_ctrl_z_takes_it_back`.
- [ ] **Step 2: Run:** fails. **Step 3: Implement.** **Step 4: Run:** passes.
- [ ] **Step 5: Commit** `feat(agent): the Templum Agent makes workspaces, blocks and wires`.

### Task 11: Opening agents

**Files:**
- Modify: `crates/templum/src/workspace/spawn.rs` (`open_agent` → `open_agent_in`), `crates/templum/src/workspace/control.rs`
- Test: `crates/templum/src/workspace/tests/templum_agent.rs`; `tests/spawn.rs` stays green unchanged

**Interfaces:**
- Look on `development` first: card 35's fork changes `spawn_agent`; keep its arguments flowing through.
- Produces: `pub(super) fn open_agent_in(&mut self, space: &str, entry: &AgentEntry, folder: &Path, spawn:
  &tools::Spawn, place: Place, wire_to: &[BlockId], opener: Opener, window, cx) -> Reply` with `pub(super) enum Opener
  { Block { terminal: String, id: BlockId }, TemplumAgent }`: `Block` keeps today's `[from <signature>, task t<n>]`
  line and `opened_by` the caller's uid; `TemplumAgent` types `[from Templum Agent, task t<n>]`, sets `opened_by` to
  `templum_agent::ASKER` and the task's `by` to `ASKER`. `spawn_agent` resolves the caller and calls it with
  `Place::Near(me)`, `wire_to: &[me]`, `Opener::Block`.
- Control `open_agent`: the spawn's checks of `spawn_agent` (the agent exists, its model and effort, installed, the
  folder; a worktree through the same background job), then `open_agent_in` in the named workspace, `near` or `Free`,
  wired to `connect`; `own.blocks` gets it. Its answer: `Opened <title> as <uid> in <folder>[, on <m> (<e>)][,
  connected to <a>, <b>].` and, with a prompt, ` Its prompt is typed once it is ready, as task t<n>.`

- [ ] **Step 1: Write the failing tests:** `open_agent_opens_in_a_workspace_not_shown_wired_to_its_board` (with the
  `agent_cat` stand-in the spawn tests use as an agent: the block is in `w2`, wired to the kanban named in `connect`,
  `opened_by == Some("templum-agent")`, the shown workspace and keyboard unchanged); `its_prompt_is_typed_under_the_templum_agents_line`
  (once ready, the terminal shows `[from Templum Agent, task t1]`); `an_unknown_agent_or_a_bad_effort_opens_nothing`.
- [ ] **Step 2: Run:** fails. **Step 3: Implement.** **Step 4: Run** `cargo test -p templum spawn templum_agent`:
  passes.
- [ ] **Step 5: Commit** `feat(agent): the Templum Agent opens agents with their model, effort and prompt`.

### Task 12: The gate

**Files:**
- Create: `crates/templum/src/workspace/approvals.rs`
- Modify: `crates/templum/src/chat/mod.rs` (`Approval`, `Answer`), `crates/templum/src/workspace/control.rs`
  (`close_block`, `send_to_terminal`, `send_keys`, and the gate before every call), `crates/templum/src/workspace/ops.rs`
  (`terminal_text`, `type_text`, `press_keys`, `would_lose`)
- Test: `crates/templum/src/workspace/tests/templum_agent.rs`

**Interfaces:**
- Produces (`chat/mod.rs`): `pub(crate) struct Approval { pub id: u64, pub conn: u64, pub request: Value, pub reply:
  Replier, pub parsed: Parsed, pub tool: String, pub level: Level, pub said: String, pub details: String, pub shown:
  Option<Instant> }`; `pub(crate) enum Answer { Once, Conversation, Deny(Option<String>) }`.
- Produces (`approvals.rs`, `impl Workspace`): `pub(super) fn gate(&mut self, conn: u64, request: Value, reply:
  Replier, tool: &str, parsed: Parsed, cx) -> Option<(Replier, Value, Parsed)>` (None: it waits; Some: run it now);
  `pub(crate) fn answer_approval(&mut self, id: u64, answer: Answer, window, cx)`; `pub(super) fn deny_waiting(&mut
  self, why: &str, cx)`; `pub(super) fn drop_approval(&mut self, conn: u64, request: &Value)` (cancelled) and
  `drop_approvals_of(&mut self, conn: u64)` (closed).
- Produces (`ops.rs`): `would_lose(&self, space: &str, id: BlockId, cx) -> Option<String>` (`alert::busy`'s sentence for
  a terminal at work, "unsaved edits" for an editor), `terminal_text`, `type_text`, `press_keys` (the bodies of
  `read_terminal`, `send_to_terminal`, `send_keys` without the caller).
- The approval's words, from the call: `close_block` said `Close "<title>"`, details `<would_lose>`; `send_to_terminal`
  said `Type into "<title>"`, details the text then `⏎ Enter` when `enter`; `send_keys` said `Press keys in "<title>"`,
  details the keys joined by spaces, after `It asks: <what>` when its agent waits for its person. A denial answers the
  Global Constraints' sentence; an answer records a `Row::Approval { said, answer }` (`Allowed once`, `Allowed for this
  conversation`, `Denied`) and a denied action's row `Denied`. A grant adds the tool to `grants`; New chat clears them.

- [ ] **Step 1: Write the failing tests** (answering through `answer_approval`; the popup is Task 16):
  - `an_idle_shell_closes_at_once_a_running_program_waits`: `close_block` on a `quiet()` shell answers `Closed …` at
    once; on an `agent_cat` block it gives no answer, `approvals.len() == 1` with said `Close "<title>"`;
    `Answer::Once` closes it and answers `Closed …`.
  - `deny_answers_the_refusal_with_the_persons_words`: `Answer::Deny(Some("leave it"))` answers `isError` with `The
    person denied this. Don't try it another way: ask them what they want.\nThey said: leave it`, and the block is
    still there.
  - `a_grant_lets_the_tool_through`: `Answer::Conversation` on `send_to_terminal`; the next `send_to_terminal` types
    at once, and `grants` holds `send_to_terminal` (Task 13's `new_chat` test checks that a new chat clears it).
  - `its_own_running_agent_closes_without_asking`: a block opened by `open_agent` (own) closes at once.
  - `a_target_gone_meanwhile_is_answered_as_gone`: the person closes the block while its approval waits; `Answer::Once`
    answers `There's no block <uid>: overview lists them.`
  - `a_cancel_or_the_engines_end_drops_it`: `notifications/cancelled` with the request's id removes it (no answer);
    `templum/closed` for its connection removes every approval of that connection.
  - `send_keys_shows_what_the_agent_asks`: details start with `It asks: ` for an agent whose state is `NeedsYou`.
- [ ] **Step 2: Run:** fails. **Step 3: Implement**: `serve_control`'s `tools/call` parses, then `gate`: the rule's
  `level(own, losing)` (own: the target's uid in `own.blocks`; losing: `would_lose` is Some), None or a grant runs it,
  else an `Approval` is pushed (`unread = true`, `cx.notify()`) and nothing is answered. `answer_approval` re-checks
  that the target still exists, runs the call through `control_call`, and answers. **Step 4: Run:** passes.
- [ ] **Step 5: Commit** `feat(agent): dangerous calls wait for the person's approval`.

### Task 13: The engine's life

**Files:**
- Create: `crates/templum/src/workspace/agent.rs`
- Modify: `crates/templum/src/workspace/mod.rs` (`mod agent;`, restoring the conversation at start, saving it when
  Templum quits, next to where the canvas is saved), `crates/templum/src/workspace/preferences.rs` (a change of
  `templum_agent.agent`, `model` or `effort` calls `engine_settings_changed`), `crates/templum/src/chat/mod.rs`
  (`engine`, `stderr`, `lines` task, `#[cfg(test)] launch`)
- Test: `crates/templum/src/workspace/tests/templum_agent.rs` (with the `fake_engine` test of Task 4, named
  `workspace::tests::templum_agent::fake_engine`), fixtures under `crates/templum/src/workspace/tests/fixtures/agent-*.jsonl`
  copied from Task 1's

**Interfaces:**
- Consumes: `templum_agent::{claude, Engine, Launch, EngineLine, Conversation, Load, fake}`.
- Produces (`impl Workspace`): `pub(crate) fn send_to_agent(&mut self, text: String, window, cx)`; `pub(crate) fn
  stop_engine(&mut self, cx)`; `pub(crate) fn new_chat(&mut self, cx)`; `pub(super) fn restore_conversation(&mut
  self)`; `pub(super) fn save_conversation(&self)`; `pub(super) fn engine_settings_changed(&mut self, cx)`; `fn
  start_engine(&mut self, window, cx) -> Result<(), String>`; `fn engine_line(&mut self, line: EngineLine, window,
  cx)`.
- Behavior: `send_to_agent` denies waiting approvals with the text as the person's words, adds the person's row,
  starts the engine when none runs (a new secret; `claude::write_mcp(<data>/agent, program())`; `claude::args` with
  `Session::New(uuid)` or `Resume(session)`, the settings' model and effort, `add_dirs` the deduped folders of every
  block; the program `templum_terminal::locate("claude")`; cwd `<data>/agent`; env adds `TEMPLUM_BUS` and
  `TEMPLUM_CONTROL`, removes `TEMPLUM_TERMINAL`), and writes `claude::message(text)`. `engine_line` keeps stderr's
  last 20 lines, parses `Out` with `claude::parse` into `conversation.apply`, sets `unread` while the chat is closed,
  saves the file at each `TurnEnded`; `Ended` while working calls `conversation.ended("Claude Code stopped: <last
  stderr line>. Your next message starts it again.")`, denies the approvals with `The person stopped you before
  answering.`, and drops the engine. `stop_engine`: Task 1's finding decides (interrupt request, or kill and `Stopped
  by you.`). `new_chat`: stop, `Conversation::new`, a `New conversation.` note, grants and approvals cleared, file
  saved. `engine_settings_changed`: an idle engine is stopped quietly; a working one is stopped at its turn's end.

- [ ] **Step 1: Write the failing tests** (the engine is the fake one: `templum_agent.launch = Some(fake::launch(…))`):
  - `a_message_starts_the_engine_and_its_turn_shows`: `send_to_agent("hi")` → the rows are the person's, the text of
    `agent-text.jsonl`, and an `End`; `conversation.json` exists in the data folder with them.
  - `the_conversation_comes_back_after_a_restart`: reopen the workspace on the same data folder; the rows and session
    are there; the next message's launch has `--resume <session>`.
  - `a_message_while_an_approval_waits_denies_it_with_the_message`.
  - `the_engine_ending_mid_turn_fails_the_turn_and_denies_what_waits` (a script whose turn ends without a `result`
    line, then the fake engine exits).
  - `new_chat_forgets_the_session_grants_and_own_things`.
  - `a_settings_change_restarts_an_idle_engine` (edit `templum_agent.model`; the next launch has `--model`).
- [ ] **Step 2: Run:** fails. **Step 3: Implement.** **Step 4: Run** `cargo test -p templum templum_agent`: passes.
- [ ] **Step 5: Commit** `feat(agent): talk to the Templum Agent: its engine started, resumed, stopped and saved`.

### Task 14: The pill

**Files:**
- Create: `crates/templum/src/chat/pill.rs`
- Modify: `crates/templum/src/workspace/render.rs` (the pill above the toolbar), `crates/templum/src/workspace/input.rs`
  (`Command::Agent`), `crates/templum/src/chat/mod.rs` (`input: Option<Entity<InputState>>`, `focus: FocusHandle`,
  `back: Option<BlockId>` the block that had the keyboard)
- Test: `crates/templum/src/workspace/tests/templum_agent.rs`

**Interfaces:**
- Produces: `pub(crate) fn render(ws: &Workspace, window: &mut Window, cx: &mut Context<Workspace>) -> AnyElement` in
  `pill.rs` (element ids `"agent-pill"`, `"agent-send"`, `"agent-engine"`); `impl Workspace { pub(crate) fn
  toggle_agent(&mut self, window, cx); pub(crate) fn leave_agent(&mut self, window, cx) }`.
- Behavior and sizes: the spec's [The pill](../specs/templum-agent.md#the-pill) table, exactly. It is drawn where the
  toolbar is (not when `tiled`), bottom `toolbar::SPACE + 8` (+ the stroke options' height while a drawing tool is
  on), and not when `config.templum_agent.pill` is false unless it has the keyboard. Enter calls `send_to_agent` and
  clears the field; the engine chip opens Settings at the Templum Agent section.

- [ ] **Step 1: Write the failing tests:** `the_shortcut_takes_the_keyboard_and_esc_gives_it_back` (a focused terminal;
  the `agent` key: the pill's input is focused and no key reaches the terminal; Esc: the terminal has the keyboard
  again); `enter_sends_and_shift_enter_is_a_new_line` (with the fake engine: typing "hi", Shift+Enter, "there", Enter →
  the person's row is `hi\nthere`); `the_pill_is_hidden_when_turned_off` (no `"agent-pill"` element with `pill =
  false`, one after the shortcut); `not_installed_says_so` (a `templum_agent.agent` whose program isn't found: the rest
  text is `Claude Code isn't installed`, through a `#[cfg(test)] installed: Option<bool>` seam on `TemplumAgent` over
  `templum_terminal::installed("claude")`, so the test doesn't depend on the machine).
- [ ] **Step 2: Run:** fails. **Step 3: Implement.** **Step 4: Run:** passes.
- [ ] **Step 5: Commit** `feat(agent): the prompt pill: Ctrl+Shift+Space to ask Templum`.

### Task 15: The chat and its rows

**Files:**
- Create: `crates/templum/src/chat/panel.rs`, `crates/templum/src/chat/rows.rs`
- Modify: `crates/templum/src/workspace/render.rs` (the chat between the Git popup and the welcome),
  `crates/templum/src/chat/pill.rs` (the pill as the chat's footer while it is open)
- Test: `crates/templum/src/workspace/tests/templum_agent.rs`

**Interfaces:**
- Produces: `pub(crate) fn render(ws: &Workspace, now: Instant, window, cx) -> Option<AnyElement>` in `panel.rs`
  (ids `"agent-chat"`, `"agent-stop"`, `"agent-new"`, `"agent-close"`, `"agent-row-<n>"`, `"agent-suggestion-<n>"`);
  `rows.rs`: `pub(crate) fn row(ws: &Workspace, n: usize, row: &Row, window, cx) -> AnyElement` and `fn icon(tool:
  &str) -> IconName` (the spec's icon per kind).
- Behavior: the spec's [The chat](../specs/templum-agent.md#the-chat) and [Rows](../specs/templum-agent.md#rows). A
  click on an action or a notice with a target calls `reveal` (switching workspaces first). The agent's text uses
  `TextView` Markdown with the styles of `markdown.rs` at zoom 1; re-parsed at most once a frame. The body follows its
  end unless scrolled up (a `ScrollHandle`; "↓ New" when rows came below the view).

- [ ] **Step 1: Write the failing tests:** `a_turn_shows_its_rows` (fake engine; `"agent-chat"` exists; the rows'
  count matches the conversation's); `a_rows_click_reveals_its_block_in_another_workspace` (an `add_block` action in
  `w2` with its target: clicking `"agent-row-1"` switches to `w2` with the block in view); `new_chat_and_stop_buttons`
  (`"agent-stop"` exists only while working; `"agent-new"` empties the rows but a `New conversation.` note);
  `suggestions_fill_the_field` (an empty conversation shows three; a click puts its text in the input).
- [ ] **Step 2: Run:** fails. **Step 3: Implement.** **Step 4: Run:** passes.
- [ ] **Step 5: Commit** `feat(agent): the Templum Agent's chat, its rows linked to the canvas`.

### Task 16: The approval popup

**Files:**
- Create: `crates/templum/src/chat/approval.rs`
- Modify: `crates/templum/src/chat/panel.rs`, `crates/templum/src/chat/pill.rs` (its yellow state),
  `crates/templum/src/workspace/approvals.rs` (telling the person: notification, sound)
- Test: `crates/templum/src/workspace/tests/templum_agent.rs`

**Interfaces:**
- Produces: `pub(crate) fn render(ws: &Workspace, approval: &Approval, of: (usize, usize), now: Instant, window, cx) ->
  AnyElement` (ids `"approval-once"`, `"approval-conversation"`, `"approval-deny"`, `"approval-words"`).
- Behavior: the spec's [Approvals](../specs/templum-agent.md#approvals). Keys `1`, `2`, `3`, Esc only while it has the
  keyboard and 0.5 s after `shown`; `2` absent for `Level::AskEach`; Allow once red for `AskEach`. The notification
  (`Templum Agent asks: <said>`) and the sound as the balloon's (`[connections] sound`, at most one every 2 s) when the
  window isn't active or the pill can't be seen; a click on it opens the chat.

- [ ] **Step 1: Write the failing tests:** `the_popup_answers_with_its_buttons_and_keys` (click `"approval-once"`
  runs the call; on another, `3` within 0.5 s does nothing and after it denies); `it_never_takes_a_terminals_keyboard`
  (a focused terminal while an approval arrives keeps the keyboard; typing `1` goes to the terminal);
  `the_words_field_denies_with_them`; `a_background_window_gets_a_notification` (as `requests.rs` checks the balloon's).
- [ ] **Step 2: Run:** fails. **Step 3: Implement.** **Step 4: Run:** passes.
- [ ] **Step 5: Commit** `feat(agent): approve or deny the Templum Agent's dangerous actions`.

### Task 17: Its mark and its glow

**Files:**
- Modify: `crates/templum/src/block.rs`, `crates/templum/src/workspace/control.rs` (a touch records `glow[uid] = now`)
- Test: `crates/templum/src/workspace/tests/templum_agent.rs`

- [ ] **Step 1: Write the failing tests:** `its_blocks_wear_its_mark` (a terminal with `opened_by ==
  Some("templum-agent")` has an element `"agent-mark-<uid>"` at the Full level and none at Card; it comes back after a
  restart); `a_touch_glows_for_a_moment` (`glow` has the block after `add_block`; 1.3 s later the block's border is its
  usual color; with less motion, steady accent during the 1.2 s).
- [ ] **Step 2: Run:** fails. **Step 3: Implement** as the spec's [On the canvas](../specs/templum-agent.md#on-the-canvas)
  says; frames are asked only while a glow lasts. **Step 4: Run:** passes.
- [ ] **Step 5: Commit** `feat(agent): blocks the Templum Agent made wear its mark and glow when it acts`.

### Task 18: Docs, minors and benches

**Files:**
- Modify: `docs/ARCHITECTURE.md` (the crate's row; the control server under Runtime's bus paragraph; `chat/`,
  `workspace/control.rs`, `approvals.rs`, `agent.rs`, `ops.rs` under Source layout), `docs/specs/templum-agent.md`
  (Status: batch 1 implemented), `docs/specs/connections.md` (a line under The MCP server pointing to the second
  server), `crates/templum/src/bench.rs` (`--bench-mixed` opens the chat on a 30-row conversation and appends a text
  delta each frame), `README.md` if it lists features, `docs/minors.md`

- [ ] **Step 1:** Run `cargo run --release -p templum -- --bench` and `-- --bench-mixed`: p95 ≤ 16.7 ms; keep both JSON
  lines for the PR.
- [ ] **Step 2:** Do the manual check of the spec's [Testing](../specs/templum-agent.md#testing) with a real Claude
  Code; write what didn't work as minors or fixes.
- [ ] **Step 3:** Run the full check command; commit `docs: the Templum Agent's first batch is implemented`.

---

## Batch 2: It orchestrates

Before starting: rebase on `development`; when card 35 landed, read its `stop_agent`, `wait_for` pattern and saved
roles first. Re-read the spec's [Orchestration](../specs/templum-agent.md#orchestration).

### Task 19: Its tasks' ends come back into the chat

**Files:** Modify `crates/templum/src/workspace/delegate.rs` (`deliver_notices`: a notice to `templum_agent::ASKER`
goes to `workspace/agent.rs` instead of a terminal), `crates/templum/src/workspace/agent.rs` (`agent_notice(text)`),
`crates/templum-agent/src/conversation.rs` (`Row::Notice { text, level, target }`, `pub enum NoticeLevel { Done,
Failed, Waiting }`), `crates/templum/src/chat/rows.rs`, `crates/templum-agent/src/claude.rs` (`ROLE` gains the spec's
batch-2 sentence).

**Interfaces:** `pub(super) fn agent_notice(&mut self, text: String, target: Target, level: NoticeLevel, window, cx)`:
a `Notice` row, and the text sent to the engine as a message (at once; the engine queues it when it works).

- [ ] Tests (`tests/templum_agent.rs`): `a_tasks_end_wakes_the_templum_agent` (an `open_agent` with a prompt to an
  `agent_cat` stand-in whose turn ends: a `Notice` row `Done` with its target, and the fake engine received a message
  starting `[task t1 from`); `waiting_for_its_person_is_told_once` (a `NeedsYou` event: one `Waiting` notice; a second
  event of the same wait: none); `a_failed_task_is_red`.
- [ ] Implement, run, commit `feat(agent): the agents the Templum Agent runs tell it when they finish`.

### Task 20: `delegate`, `wait_for`, `interrupt`, `stop_agent`

**Files:** `crates/templum-mcp/src/control.rs` (the four in `SHARED` or as own calls; `interrupt` and `stop_agent`
`Rule::AskOwn`), `crates/templum/src/workspace/control.rs`, `crates/templum/src/workspace/ops.rs` (the asker-free
bodies: `hand_task(asker: &str, block, task, wait)`, waits with `Asked { from: ASKER }`).

- [ ] Tests: `delegate_hands_a_task_under_the_templum_agents_line`; `wait_for_answers_needs_you`;
  `interrupting_the_persons_agent_asks_its_own_doesnt`; card 35's `stop_agent` likewise.
- [ ] Implement, run, commit `feat(agent): the Templum Agent hands tasks to agents and waits for them`.

### Task 21: Agents at once

**Files:** `crates/templum/src/workspace/control.rs` (the count of tasks whose `by` is `ASKER` and that are under way),
Settings ▸ Templum Agent's **Agents at once** row (`settings/templum_agent.rs`).

- [ ] Tests: `the_fifth_task_is_refused_while_four_run` (the spec's sentence exactly; after one ends, it goes);
  `max_agents_from_the_settings`.
- [ ] Implement, run, commit `feat(agent): the Templum Agent runs at most N agents at once`.

### Task 22: The run meter and Catch me up

**Files:** `crates/templum/src/chat/panel.rs` (header: `"<n> working · $<cost>"`, a click lists the tasks),
`crates/templum-agent/src/conversation.rs` (`cost()` adds notices' costs), the suggestion chip and the header button.

- [ ] Tests: `the_meter_counts_its_tasks_and_cost`; `catch_me_up_sends_its_sentence`.
- [ ] Implement, run, bench (`--bench-mixed`), docs (spec status), commit `feat(agent): see what the Templum Agent's
  run costs, and ask it to catch you up`.

---

## Batch 3: Everything by hand

Before starting: rebase on `development`; card 35 should have landed (its note, checklist, command, background
terminal and worktree tools). Each task adds tools to `control.rs` (schemas, parse, `rule`, titles, tests as Task 5's)
and their serving in `workspace/control.rs` through card 35's methods or `ops.rs`.

### Task 23: Codex's stream, checked and read

**Files:** `crates/templum-agent/src/codex.rs` (`parse(line) -> Vec<ChatEvent>`, `args(&Options, prompt: &str) ->
Vec<String>` for `exec --json` and `exec resume <id> --json`), fixtures `codex-*.jsonl`, the spec's Codex paragraph.

- [ ] Step 1 records Codex 0.154's `exec --json` lines by hand (a text turn, a turn calling an MCP tool of a fake
  server, a resume) and whether an MCP call asks to be approved in `exec`; write it in the spec.
- [ ] Tests as Task 1's and Task 2's for Codex; implement; commit `feat(agent): read Codex's stream for the Templum
  Agent`.

### Task 24: Codex as the engine

**Files:** `crates/templum/src/workspace/agent.rs` (one process per message for Codex; a message during a turn waits
for its end), `config.rs` (`agent = "codex"` accepted; its efforts), `settings/templum_agent.rs`.

- [ ] Tests: `codex_runs_one_process_per_message_and_resumes_its_thread` (fake engine); `a_message_during_a_codex_turn_waits`.
- [ ] Implement, run, commit `feat(agent): the Templum Agent can run on Codex`.

### Task 25: Workspaces edited, stopped and deleted

**Files:** `control.rs` (both crates): `edit_workspace` (Free), `stop_workspace` (AskOwn), `delete_workspace`
(`AskEach`, AskOwn when empty) through `edit_space`, `stop_now`, `delete_space`; the approval's details list the blocks
and the agents that stop.

- [ ] Tests: each tool; deleting the person's non-empty workspace has no `"approval-conversation"`; its own empty one
  goes at once. Commit `feat(agent): the Templum Agent edits, stops and deletes workspaces`.

### Task 26: Blocks moved and arranged, grid and fit

**Files:** `ops.rs` (`place_block(space, id, Rect)`: one Ctrl+Z step through `placing`/`end_placing`), `control.rs`:
`move_block`, `arrange` (`grid`, `row`, `column`, without overlap, keeping sizes), `grid`, `fit`.

- [ ] Tests: `move_block_is_one_undo_step`; `arrange_grid_overlaps_nothing`; `move_block_to_another_workspace_keeps_it_running`.
  Commit `feat(agent): the Templum Agent moves and arranges blocks`.

### Task 27: Boards: names, columns and deletes

**Files:** `control.rs`: `rename_board`, `add_column`, `rename_column` (Free), `delete_card`, `delete_column` (AskOwn),
`delete_board` (`AskEach`), through `edit_boards` and `Boards`'s methods.

- [ ] Tests: each; a delete of the person's card asks, of its own doesn't; Ctrl+Z on the kanban takes each back.
  Commit `feat(agent): the Templum Agent manages boards and columns`.

### Task 28: Card 35's note, checklist, command and worktree tools

**Files:** `control.rs`: `replace_note`, `edit_item`, `delete_item`, `delete_note`, `delete_checklist` (AskOwn), `run`
(Ask; `NewBlock::Terminal { command, background }`), `remove_worktree`, `merge_worktree` (`AskEach`), all through card
35's methods.

- [ ] Tests: each one's rule and one success path; `run`'s approval shows the command. Commit `feat(agent): the
  Templum Agent edits notes and checklists, runs commands and manages worktrees`.

### Task 29: Settings and `[agents]` entries

**Files:** `control.rs`: `read_settings` (Free), `set_setting` (Ask; one `config::Edit` through `Workspace::edit`, the
details `<table.key>: <old> → <new>`), `save_agent`, `remove_agent` (Ask; through `save_agent`/`remove_agent` of
`preferences.rs`; the details show the command in the terminal's font).

- [ ] Tests: `set_setting_asks_then_applies_and_saves` (`config.toml` keeps its comments); `an_invalid_value_is_refused_before_asking`
  (`font.size = 200`: the config's own error, no approval); `save_agent_adds_an_entry_the_menu_lists`.
  Commit `feat(agent): the Templum Agent changes settings and adds agents, with your approval`.

### Task 30: Docs and benches for batches 2 and 3

- [ ] ARCHITECTURE.md, the spec's status, minors; `--bench` and `--bench-mixed`; commit `docs: the Templum Agent
  orchestrates and does everything by hand`.

---

## Batch 4: Later ideas

Not planned here: the plan card with limits, templates, schedules, @ mentions, slash commands, history, the chat as a
block, accounts and voice ([the spec's Later ideas](../specs/templum-agent.md#later-ideas)). Once batch 3 lands,
brainstorm them with the person and write their own plan.
