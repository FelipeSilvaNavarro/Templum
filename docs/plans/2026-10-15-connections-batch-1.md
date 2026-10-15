# Connections, batch 1 (the bus): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the Claude Code and Codex Templum opens get Templum's MCP server and hooks for that session; every block
has a lasting id; an agent can ask who it is, list its workspace's blocks, read and type into its own terminal and
notify the person; its block's dot shows whether it is working or waiting for the person; `templum <verb>` does the
same from any Templum terminal.

**Architecture:** a new UI-free crate, `templum-mcp`, holds the line protocol (`rpc.rs`), the tools (`tools.rs`), the
agents' hook events (`hooks.rs`), the socket (`bus.rs`: an accept thread, a reader and a writer thread per
connection, handing each line to a `futures` channel) and the command line (`cli.rs`: the MCP bridge, `hook`, the
verbs). The workspace drains the channel on the UI thread (`workspace/bus.rs`), finds the caller by its
`TEMPLUM_TERMINAL`, and answers. Blocks get a five-character id kept in `Space::uids` and saved on each item.
`typed()` adds `--settings` (status line and hooks) and `--mcp-config` for Claude Code, and `-c` flags for Codex.

**Tech Stack:** Rust 2024, std threads and sockets (`UnixListener`; `TcpListener` + token on Windows), `serde_json`,
`futures::channel::mpsc`, alacritty_terminal 0.26 (`Term::bounds_to_string`), GPUI through gpui-kit 0.7.3.

**Spec:** [docs/specs/connections.md](../specs/connections.md) (batch 1 of its Delivery).

## Global Constraints

- Only `crates/templum` depends on gpui-kit; `templum-mcp`, `templum-terminal`, `templum-usage` stay UI-free.
- No new dependency: `serde`, `serde_json`, `futures`, `uuid`, `tempfile` are in the tree (`cargo deny check`).
- No `unsafe`. Every logic change ships a test that fails without it.
- Nothing is written into an agent's own settings files: Templum passes everything on the line it types.
- MCP stdio: one JSON-RPC 2.0 message per line; `templum mcp` writes nothing else on stdout.
- `templum hook …` always exits 0 and prints nothing (Claude Code adds a hook's stdout to its context).
- Exit codes: 0 done, 1 refused or failed (text on stderr), 2 usage error / not inside Templum / Templum gone.
- `read_terminal`: `lines` defaults to 200, at most 2000; a result is at most 256 KB.
- Block ids: five characters of `0-9a-z`, unique among all workspaces; kept by restart, undo and moves; a pasted block
  gets a new one.
- Commits on `development`, Conventional Commits, `feat`/`fix` titles written for a person using Templum; every
  commit ends with the `Claude-Session:` line.
- Before saying done: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`; this batch touches `templum-terminal`, so `--bench` and `--bench-mixed` run too (p95 ≤ 16.7 ms).

## Review Focus

1. **Templum quits or crashes while an agent uses it:** `templum mcp` must end (EOF on the socket), never hang the
   agent; a hook must return at once. Manual check 4; the bridge's code path is the same as stdin's EOF (Task 4).
2. **A hook's stdout:** any byte printed by `templum hook claude` lands in Claude's context. Task 4's hook test
   asserts stdout stays empty, in Templum and outside it.
3. **Huge or odd terminal history** (a 1000-column line, a full 10 000-line history): `read_terminal` is capped at
   256 KB from the end. Task 1 tests `tail`; Task 8 uses it.
4. **Saved canvases from before, and hand-edited ones with duplicate or malformed ids:** every block still gets a
   unique, valid id. Task 7 tests a duplicate and a malformed `uid`.
5. **A data folder Templum can't type** (a `$` in it): the agent starts without connections and without a broken
   line. Task 10 tests it.

---

### Task 1: `templum-mcp`: the line protocol and the tools

**Files:**
- Create: `crates/templum-mcp/Cargo.toml`, `crates/templum-mcp/src/lib.rs`, `crates/templum-mcp/src/rpc.rs`,
  `crates/templum-mcp/src/tools.rs`
- Test: inline `#[cfg(test)] mod tests` in `rpc.rs`, `tools.rs`, `lib.rs`

**Interfaces:**
- Produces: `rpc::{Incoming, parse, result, error, notification, request, PARSE, INVALID, METHOD, PARAMS}`;
  `tools::{Call, Reply, list, known, parse, tool_result, initialize_result, tail, MAX_LINES, MAX_TEXT}`;
  `templum_mcp::codex_flags(exe: &str, windows: bool) -> Option<String>`.

- [ ] **Step 1: The crate**

`crates/templum-mcp/Cargo.toml`:

```toml
[package]
name = "templum-mcp"
description = "The bus Templum's blocks talk over: an MCP server, the agents' hooks and the templum command line. No UI."
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
publish.workspace = true

[dependencies]
# Already in the tree: the JSON-RPC lines.
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# Already in the tree (GPUI's): the channel that hands the bus's lines to the UI thread.
futures = "0.3"
# Already in the tree: the Windows bus's token.
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

`src/lib.rs` (the `bus`, `cli` and `hooks` modules arrive in Tasks 2–4; declare only what exists):

```rust
//! The bus Templum's blocks talk over (docs/specs/connections.md): JSON-RPC lines on a local
//! socket, the MCP tools, the agents' hook events and the `templum` command line. No UI.

pub mod rpc;
pub mod tools;

pub use rpc::Incoming;
pub use tools::{Call, Reply};

/// What the Codex Templum opens is given (docs/specs/connections.md): Templum's MCP server, the
/// variables it needs (Codex hands a server only those it is told to), and its turns' end
/// (`notify`), as `-c` flags to append to its command. None when `exe` can't go inside single
/// quotes and a TOML string.
pub fn codex_flags(exe: &str, windows: bool) -> Option<String> {
    let exe = if windows { exe.replace('\\', "/") } else { exe.to_string() };
    if exe.is_empty() || exe.contains(['\'', '"', '\\', '\n']) {
        return None;
    }
    Some(format!(
        " -c 'mcp_servers.templum.command=\"{exe}\"' -c 'mcp_servers.templum.args=[\"mcp\"]' \
         -c 'mcp_servers.templum.env_vars=[\"TEMPLUM_BUS\",\"TEMPLUM_TERMINAL\"]' \
         -c 'mcp_servers.templum.tool_timeout_sec=3600' \
         -c 'notify=[\"{exe}\",\"hook\",\"codex\"]'"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_gets_the_server_and_notify_in_single_quotes() {
        let flags = codex_flags("/opt/Templum App/templum", false).unwrap();
        assert!(
            flags.starts_with(" -c 'mcp_servers.templum.command=\"/opt/Templum App/templum\"'"),
            "{flags}"
        );
        assert!(flags.contains("-c 'mcp_servers.templum.args=[\"mcp\"]'"));
        assert!(flags.contains("env_vars=[\"TEMPLUM_BUS\",\"TEMPLUM_TERMINAL\"]"));
        assert!(flags.ends_with("-c 'notify=[\"/opt/Templum App/templum\",\"hook\",\"codex\"]'"));
        let windows = codex_flags(r"C:\Program Files\Templum\templum.exe", true).unwrap();
        assert!(windows.contains("command=\"C:/Program Files/Templum/templum.exe\""), "{windows}");
        assert_eq!(codex_flags("/home/o'neil/templum", false), None);
        assert_eq!(codex_flags("", false), None);
    }
}
```

- [ ] **Step 2: Write the failing tests for `rpc.rs` and `tools.rs`**

`src/rpc.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_notifications_and_responses() {
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#),
            Ok(Incoming::Request { id: json!(1), method: "ping".into(), params: Value::Null })
        );
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            Ok(Incoming::Notification {
                method: "notifications/initialized".into(),
                params: Value::Null
            })
        );
        assert_eq!(parse(r#"{"jsonrpc":"2.0","id":"a","result":{}}"#), Ok(Incoming::Response));
    }

    #[test]
    fn a_bad_line_is_answered_with_its_error() {
        let answer: Value = serde_json::from_str(&parse("{not json").unwrap_err()).unwrap();
        assert_eq!((answer["error"]["code"].as_i64(), &answer["id"]), (Some(PARSE.into()), &Value::Null));
        let answer: Value = serde_json::from_str(&parse(r#"{"id":7}"#).unwrap_err()).unwrap();
        assert_eq!((answer["error"]["code"].as_i64(), answer["id"].as_i64()), (Some(INVALID.into()), Some(7)));
        assert!(parse("[1,2]").is_err(), "MCP has no batches");
    }

    #[test]
    fn every_line_written_is_one_line() {
        let text = result(&json!(1), json!({ "text": "a\nb" }));
        assert!(text.ends_with('\n'));
        assert_eq!(text.matches('\n').count(), 1);
        assert_eq!(error(&json!(2), METHOD, "no").matches('\n').count(), 1);
    }
}
```

`src/tools.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_listed_parses_its_example() {
        let examples = [
            ("whoami", json!({})),
            ("list_blocks", json!({})),
            ("read_terminal", json!({ "block": "b7k2m", "lines": 10, "last_command": true })),
            ("send_to_terminal", json!({ "block": "b7k2m", "text": "ls", "enter": true })),
            ("notify", json!({ "text": "done" })),
        ];
        let listed: Vec<String> = list()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(listed, examples.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>());
        for (name, args) in &examples {
            assert!(known(name));
            assert!(parse(name, args).is_ok(), "{name}");
        }
        assert_eq!(
            parse("read_terminal", &json!({ "block": "b" })),
            Ok(Call::ReadTerminal { block: "b".into(), lines: 200, last_command: false })
        );
        assert_eq!(
            parse("read_terminal", &json!({ "block": "b", "lines": 99_999 })),
            Ok(Call::ReadTerminal { block: "b".into(), lines: MAX_LINES, last_command: false })
        );
        assert_eq!(
            parse("send_to_terminal", &json!({ "block": "b", "text": "x" })),
            Ok(Call::SendToTerminal { block: "b".into(), text: "x".into(), enter: false })
        );
    }

    #[test]
    fn bad_arguments_say_which() {
        let err = parse("read_terminal", &json!({})).unwrap_err();
        assert!(err.contains("\"block\""), "{err}");
        assert!(parse("read_terminal", &json!({ "block": "b", "lines": 0 })).is_err());
        assert!(parse("send_to_terminal", &json!({ "block": "b", "text": "x", "enter": "yes" })).is_err());
        assert!(!known("rm_rf"));
        assert!(parse("rm_rf", &json!({})).is_err());
    }

    #[test]
    fn initialize_speaks_the_clients_version_when_it_can() {
        let answer = initialize_result(&json!({ "protocolVersion": "2025-06-18" }), "hi");
        assert_eq!(answer["protocolVersion"], "2025-06-18");
        assert_eq!(answer["serverInfo"]["name"], "templum");
        assert_eq!(answer["instructions"], "hi");
        assert!(answer["capabilities"]["tools"].is_object());
        let newer = initialize_result(&json!({ "protocolVersion": "2099-01-01" }), "");
        assert_eq!(newer["protocolVersion"], VERSIONS[0]);
    }

    #[test]
    fn a_refusal_is_a_result_marked_as_an_error() {
        let refused: Value = serde_json::from_str(&tool_result(&json!(3), &Reply::err("no"))).unwrap();
        assert_eq!(refused["result"]["isError"], true);
        assert_eq!(refused["result"]["content"][0]["text"], "no");
        let done: Value = serde_json::from_str(&tool_result(&json!(4), &Reply::ok("yes"))).unwrap();
        assert!(done["result"].get("isError").is_none());
    }

    #[test]
    fn a_long_text_keeps_its_end() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("abc", 10), "abc");
        // Cut inside "é" (2 bytes): starts after it.
        assert_eq!(tail("abcé", 1), "");
        assert_eq!(tail("abcé", 2), "é");
    }
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p templum-mcp`
Expected: FAIL to compile (`parse`, `list`, … not defined).

- [ ] **Step 4: Implement `rpc.rs`**

```rust
//! JSON-RPC 2.0, one message per line: what the bus reads and writes.

use serde_json::{Value, json};

pub const PARSE: i32 = -32700;
pub const INVALID: i32 = -32600;
pub const METHOD: i32 = -32601;
pub const PARAMS: i32 = -32602;

/// A line read from a client.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    /// Wants an answer with its `id`.
    Request { id: Value, method: String, params: Value },
    Notification { method: String, params: Value },
    /// An answer to a request of ours: Templum sends none, so it is ignored.
    Response,
}

/// Reads one line. Err is the error line to answer it with.
pub fn parse(line: &str) -> Result<Incoming, String> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Err(error(&Value::Null, PARSE, "Parse error"));
    };
    let Some(object) = value.as_object() else {
        return Err(error(&Value::Null, INVALID, "Invalid Request"));
    };
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    let id = object.get("id").cloned();
    match (object.get("method").and_then(Value::as_str), id) {
        (Some(method), Some(id)) => Ok(Incoming::Request { id, method: method.to_string(), params }),
        (Some(method), None) => Ok(Incoming::Notification { method: method.to_string(), params }),
        (None, _) if object.contains_key("result") || object.contains_key("error") => {
            Ok(Incoming::Response)
        }
        (None, id) => Err(error(&id.unwrap_or(Value::Null), INVALID, "Invalid Request")),
    }
}

/// A request's answer, as a line.
pub fn result(id: &Value, result: Value) -> String {
    line(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

pub fn error(id: &Value, code: i32, message: &str) -> String {
    line(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }))
}

pub fn notification(method: &str, params: Value) -> String {
    line(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
}

pub fn request(id: u64, method: &str, params: Value) -> String {
    line(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
}

/// `value` on one line: serde_json escapes the newlines inside strings.
fn line(value: &Value) -> String {
    let mut text = value.to_string();
    text.push('\n');
    text
}
```

- [ ] **Step 5: Implement `tools.rs`**

```rust
//! The MCP tools Templum serves (docs/specs/connections.md, Tools): their schemas, the calls
//! parsed from `tools/call`, and the replies.

use serde_json::{Value, json};

/// The MCP versions Templum speaks, newest first: it serves tools only, whose messages none of
/// these changed.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// The most lines `read_terminal` returns.
pub const MAX_LINES: usize = 2000;
/// The most bytes of text a tool returns: its end is kept.
pub const MAX_TEXT: usize = 256 * 1024;

/// A tool call, its arguments checked.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Whoami,
    ListBlocks,
    ReadTerminal { block: String, lines: usize, last_command: bool },
    SendToTerminal { block: String, text: String, enter: bool },
    Notify { text: String },
}

/// What a tool answers: its text, and whether it is a refusal or a failure.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub text: String,
    pub is_error: bool,
}

impl Reply {
    pub fn ok(text: impl Into<String>) -> Reply {
        Reply { text: text.into(), is_error: false }
    }

    pub fn err(text: impl Into<String>) -> Reply {
        Reply { text: text.into(), is_error: true }
    }
}

/// `initialize`'s result: the client's version when Templum speaks it, else its newest.
pub fn initialize_result(params: &Value, instructions: &str) -> Value {
    let asked = params["protocolVersion"].as_str();
    let version = VERSIONS.iter().find(|v| Some(**v) == asked).unwrap_or(&VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "templum", "version": env!("CARGO_PKG_VERSION") },
        "instructions": instructions,
    })
}

/// `tools/list`'s result.
pub fn list() -> Value {
    let block = json!({ "type": "string", "description": "A block's id, as list_blocks shows it." });
    let none = json!({ "type": "object", "properties": {} });
    json!({ "tools": [
        {
            "name": "whoami",
            "description": "Your block on Templum's canvas: its id, kind, title, folder and workspace.",
            "inputSchema": none,
        },
        {
            "name": "list_blocks",
            "description": "Every block in your workspace: its id, kind, title and folder, whether it is \
                            connected to you, and an agent's state.",
            "inputSchema": none,
        },
        {
            "name": "read_terminal",
            "description": "The last lines of a terminal connected to you (its history and screen), or its \
                            last finished command and that command's output.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "block": block,
                    "lines": { "type": "integer", "minimum": 1, "maximum": MAX_LINES, "default": 200 },
                    "last_command": {
                        "type": "boolean",
                        "default": false,
                        "description": "Only the last finished command and its output (needs the shell's integration).",
                    },
                },
                "required": ["block"],
            },
        },
        {
            "name": "send_to_terminal",
            "description": "Types text into a terminal connected to you, as a paste; Enter after it when asked.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "block": block,
                    "text": { "type": "string" },
                    "enter": { "type": "boolean", "default": false },
                },
                "required": ["block", "text"],
            },
        },
        {
            "name": "notify",
            "description": "Shows the person a notification from your block.",
            "inputSchema": {
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"],
            },
        },
    ]})
}

/// `name` is one of the tools.
pub fn known(name: &str) -> bool {
    list()["tools"]
        .as_array()
        .is_some_and(|tools| tools.iter().any(|t| t["name"] == name))
}

/// The call `name` with its arguments `args`; Err says what is wrong with them.
pub fn parse(name: &str, args: &Value) -> Result<Call, String> {
    let text = |key: &str| {
        args[key]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("{name} needs \"{key}\", a string"))
    };
    let flag = |key: &str| match &args[key] {
        Value::Null => Ok(false),
        value => value.as_bool().ok_or_else(|| format!("\"{key}\" is true or false")),
    };
    Ok(match name {
        "whoami" => Call::Whoami,
        "list_blocks" => Call::ListBlocks,
        "read_terminal" => Call::ReadTerminal {
            block: text("block")?,
            lines: match &args["lines"] {
                Value::Null => 200,
                value => value
                    .as_u64()
                    .filter(|n| *n >= 1)
                    .map(|n| (n as usize).min(MAX_LINES))
                    .ok_or_else(|| format!("\"lines\" is a number from 1 to {MAX_LINES}"))?,
            },
            last_command: flag("last_command")?,
        },
        "send_to_terminal" => Call::SendToTerminal {
            block: text("block")?,
            text: text("text")?,
            enter: flag("enter")?,
        },
        "notify" => Call::Notify { text: text("text")? },
        _ => return Err(format!("Unknown tool: {name}")),
    })
}

/// A `tools/call` answer, as a line.
pub fn tool_result(id: &Value, reply: &Reply) -> String {
    let mut result = json!({ "content": [{ "type": "text", "text": reply.text }] });
    if reply.is_error {
        result["isError"] = json!(true);
    }
    crate::rpc::result(id, result)
}

/// The end of `text`, at most `max` bytes, cut where a character starts.
pub fn tail(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p templum-mcp`
Expected: PASS (codex flags, rpc, tools).

- [ ] **Step 7: Commit**

```bash
git add crates/templum-mcp Cargo.lock
git commit -m "feat(connections): describe the tools Templum offers agents over MCP" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 2: `templum-mcp`: the agents' hook events

**Files:**
- Create: `crates/templum-mcp/src/hooks.rs`
- Modify: `crates/templum-mcp/src/lib.rs` (`pub mod hooks; pub use hooks::AgentEvent;`)

**Interfaces:**
- Produces: `hooks::{AgentEvent, METHOD, claude(&Value) -> Option<AgentEvent>, codex(&Value) -> Option<AgentEvent>}`.
  `AgentEvent` serializes as `{"event":"started"}`, `{"event":"working"}`, `{"event":"stopped","message":…}`,
  `{"event":"needs_you","message":…}`, `{"event":"ended"}`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_codes_hooks_become_events() {
        let event = |input: Value| claude(&input);
        assert_eq!(
            event(json!({ "hook_event_name": "SessionStart", "source": "startup", "session_id": "s" })),
            Some(AgentEvent::Started)
        );
        assert_eq!(
            event(json!({ "hook_event_name": "UserPromptSubmit", "prompt": "hi" })),
            Some(AgentEvent::Working)
        );
        // After a permission the person granted, the next tool's end says it works again.
        assert_eq!(
            event(json!({ "hook_event_name": "PostToolUse", "tool_name": "Bash" })),
            Some(AgentEvent::Working)
        );
        assert_eq!(
            event(json!({ "hook_event_name": "Stop", "last_assistant_message": "Done." })),
            Some(AgentEvent::Stopped { message: Some("Done.".into()) })
        );
        assert_eq!(
            event(json!({ "hook_event_name": "Stop" })),
            Some(AgentEvent::Stopped { message: None }),
            "a Claude Code that doesn't say its answer"
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "permission_prompt",
                "message": "Claude needs your permission to use Bash",
            })),
            Some(AgentEvent::NeedsYou { message: Some("Claude needs your permission to use Bash".into()) })
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "idle_prompt",
                "message": "Claude is waiting for your input",
            })),
            None,
            "the reminder after a finished turn isn't a wait for the person"
        );
        assert_eq!(event(json!({ "hook_event_name": "SessionEnd" })), Some(AgentEvent::Ended));
        assert_eq!(event(json!({ "hook_event_name": "PreCompact" })), None);
        assert_eq!(event(json!("garbage")), None);
    }

    #[test]
    fn codexs_notify_is_a_finished_turn() {
        assert_eq!(
            codex(&json!({ "type": "agent-turn-complete", "last-assistant-message": "Fixed." })),
            Some(AgentEvent::Stopped { message: Some("Fixed.".into()) })
        );
        assert_eq!(codex(&json!({ "type": "something-else" })), None);
    }

    #[test]
    fn events_go_over_the_bus_as_tagged_json() {
        assert_eq!(
            serde_json::to_value(AgentEvent::Stopped { message: Some("x".into()) }).unwrap(),
            json!({ "event": "stopped", "message": "x" })
        );
        assert_eq!(
            serde_json::from_value::<AgentEvent>(json!({ "event": "needs_you", "message": null })).unwrap(),
            AgentEvent::NeedsYou { message: None }
        );
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp hooks`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

```rust
//! What the agents' hooks tell Templum about their turns (docs/specs/connections.md, Hooks).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The method of the notification `templum hook` sends.
pub const METHOD: &str = "templum/event";

/// An agent's turn, as its hooks report it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    /// A session started: it waits for a prompt.
    Started,
    /// It works on a prompt: one was sent, or a tool just ran.
    Working,
    /// It finished its turn, with its last answer.
    Stopped { message: Option<String> },
    /// It waits for the person: a permission, a question.
    NeedsYou { message: Option<String> },
    /// The session ended.
    Ended,
}

/// A Claude Code hook's input (https://code.claude.com/docs/en/hooks). None for what Templum
/// doesn't follow: the idle reminder after a finished turn is not a wait for the person.
pub fn claude(input: &Value) -> Option<AgentEvent> {
    let text = |key: &str| input[key].as_str().map(str::to_string);
    Some(match input["hook_event_name"].as_str()? {
        "SessionStart" => AgentEvent::Started,
        "UserPromptSubmit" | "PostToolUse" => AgentEvent::Working,
        "Stop" => AgentEvent::Stopped { message: text("last_assistant_message") },
        "Notification" => match input["notification_type"].as_str()? {
            "permission_prompt" | "elicitation_dialog" | "agent_needs_input" => {
                AgentEvent::NeedsYou { message: text("message") }
            }
            _ => return None,
        },
        "SessionEnd" => AgentEvent::Ended,
        _ => return None,
    })
}

/// Codex's `notify` argument: only `agent-turn-complete` exists.
pub fn codex(input: &Value) -> Option<AgentEvent> {
    (input["type"] == "agent-turn-complete").then(|| AgentEvent::Stopped {
        message: input["last-assistant-message"].as_str().map(str::to_string),
    })
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-mcp`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-mcp
git commit -m "feat(connections): understand when Claude Code and Codex start, work, wait and finish" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 3: `templum-mcp`: the bus

**Files:**
- Create: `crates/templum-mcp/src/bus.rs`
- Modify: `crates/templum-mcp/src/lib.rs` (`pub mod bus; pub use bus::{Bus, Hello, Message, Replier};`)

**Interfaces:**
- Consumes: `rpc::{parse, notification, Incoming}` (Task 1).
- Produces:
  - `pub fn start(data: &Path, tx: futures::channel::mpsc::UnboundedSender<Message>) -> io::Result<Bus>`
  - `pub struct Bus { pub env: String, .. }` (Drop stops it and removes its socket)
  - `pub struct Hello { pub terminal: String, pub client: String }`
  - `pub struct Message { pub hello: Hello, pub incoming: Incoming, pub reply: Replier }`
  - `pub type Replier = std::sync::mpsc::Sender<String>`
  - `pub enum Stream` (`Read + Write`, `try_clone`, `finish` = shut its writing half)
  - `pub fn connect(env: &str) -> io::Result<(Stream, Option<String>)>` (the token for the hello)
  - `pub fn hello_line(terminal: &str, client: &str, token: Option<&str>) -> String`
  - `#[cfg(test)] pub(crate) fn test_bus(answer: impl Fn(Message) + Send + 'static) -> (tempfile::TempDir, Bus)`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn pong(message: Message) {
        if let Incoming::Request { id, .. } = &message.incoming {
            let answer = json!({ "terminal": message.hello.terminal, "client": message.hello.client });
            let _ = message.reply.send(rpc::result(id, answer));
        }
    }

    fn client(bus: &Bus) -> (Stream, BufReader<Stream>) {
        let (mut stream, token) = connect(&bus.env).unwrap();
        stream
            .write_all(hello_line("123-4", "cli", token.as_deref()).as_bytes())
            .unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        (stream, reader)
    }

    fn answer(reader: &mut BufReader<Stream>) -> Value {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn a_request_is_answered_on_its_connection() {
        let (_dir, bus) = test_bus(pong);
        let scheme = if cfg!(unix) { "unix:" } else { "tcp:" };
        assert!(bus.env.starts_with(scheme), "{}", bus.env);
        let (mut stream, mut reader) = client(&bus);
        stream.write_all(rpc::request(1, "ping", Value::Null).as_bytes()).unwrap();
        let answer = answer(&mut reader);
        assert_eq!(answer["id"], 1);
        assert_eq!(answer["result"], json!({ "terminal": "123-4", "client": "cli" }));
    }

    #[test]
    fn a_slow_answer_never_holds_back_the_next() {
        // The first request is answered after the second.
        let held = std::sync::Mutex::new(None::<Message>);
        let (_dir, bus) = test_bus(move |message| {
            let mut held = held.lock().unwrap();
            match held.take() {
                None => *held = Some(message),
                Some(first) => {
                    pong(message);
                    pong(first);
                }
            }
        });
        let (mut stream, mut reader) = client(&bus);
        stream.write_all(rpc::request(1, "slow", Value::Null).as_bytes()).unwrap();
        stream.write_all(rpc::request(2, "quick", Value::Null).as_bytes()).unwrap();
        let ids = [answer(&mut reader)["id"].clone(), answer(&mut reader)["id"].clone()];
        assert_eq!(ids, [json!(2), json!(1)]);
    }

    #[test]
    fn a_bad_line_is_answered_and_the_next_still_served() {
        let (_dir, bus) = test_bus(pong);
        let (mut stream, mut reader) = client(&bus);
        stream.write_all(b"{oops\n").unwrap();
        stream.write_all(rpc::request(2, "ping", Value::Null).as_bytes()).unwrap();
        assert_eq!(answer(&mut reader)["error"]["code"], rpc::PARSE);
        assert_eq!(answer(&mut reader)["id"], 2);
    }

    #[test]
    fn a_connection_without_its_hello_is_closed() {
        let (_dir, bus) = test_bus(pong);
        for first in [rpc::request(1, "ping", Value::Null), hello_line("1-1", "cli", Some("wrong"))] {
            let (mut stream, _) = connect(&bus.env).unwrap();
            stream.write_all(first.as_bytes()).unwrap();
            // It may already be closed.
            let _ = stream.write_all(rpc::request(2, "ping", Value::Null).as_bytes());
            let mut rest = String::new();
            assert_eq!(BufReader::new(stream).read_line(&mut rest).unwrap_or(0), 0, "{rest}");
        }
    }

    #[test]
    fn a_client_done_writing_gets_its_answers_then_the_end() {
        let (_dir, bus) = test_bus(pong);
        let (mut stream, mut reader) = client(&bus);
        stream.write_all(rpc::request(1, "ping", Value::Null).as_bytes()).unwrap();
        stream.finish().unwrap();
        assert_eq!(answer(&mut reader)["id"], 1);
        let mut rest = String::new();
        assert_eq!(reader.read_line(&mut rest).unwrap(), 0, "closed after its answers");
    }

    #[test]
    fn nothing_to_connect_to_is_an_error() {
        assert!(connect("nonsense").is_err());
        #[cfg(unix)]
        assert!(connect("unix:/nonexistent/templum.sock").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_goes_with_the_bus_and_a_stale_one_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join("bus").join("1-0.sock");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        // A socket file nobody listens on, as a Templum that crashed leaves it.
        drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
        assert!(stale.exists());
        let (tx, _rx) = futures::channel::mpsc::unbounded();
        let bus = start(dir.path(), tx).unwrap();
        assert!(!stale.exists());
        let path = PathBuf::from(bus.env.strip_prefix("unix:").unwrap());
        assert!(path.exists());
        drop(bus);
        assert!(!path.exists());
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp bus`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

```rust
//! The local socket every MCP server, hook and `templum` command started in Templum's terminals
//! talks to (docs/specs/connections.md, The bus). A thread accepts connections; each has a
//! thread that reads its lines and one that writes its answers, so the UI thread only ever
//! sends to a channel.

use std::io::{self, BufRead as _, BufReader, Read, Write};
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::mpsc::UnboundedSender;
use serde_json::json;

use crate::rpc::{self, Incoming};

/// The first line every client sends.
pub const HELLO: &str = "templum/hello";

/// Who a connection is for: the terminal it runs in (its `TEMPLUM_TERMINAL`), and what it is
/// ("mcp", "cli" or "hook").
#[derive(Clone, Debug, PartialEq)]
pub struct Hello {
    pub terminal: String,
    pub client: String,
}

/// Sends an answer line back to the connection a request came on.
pub type Replier = std::sync::mpsc::Sender<String>;

/// A line from a client, for the UI thread.
pub struct Message {
    pub hello: Hello,
    pub incoming: Incoming,
    /// Answers it. A connection closes once its client is done writing and every replier of
    /// it is dropped.
    pub reply: Replier,
}

/// The bus, while Templum runs. Dropping it stops taking connections and removes its socket.
pub struct Bus {
    /// Its address, as terminals get it in `TEMPLUM_BUS`.
    pub env: String,
    /// Unix: its socket file.
    path: Option<PathBuf>,
    closed: Arc<AtomicBool>,
}

/// Starts the bus in Templum's data folder `data`: every client's lines go to `tx`.
pub fn start(data: &Path, tx: UnboundedSender<Message>) -> io::Result<Bus> {
    let (listener, env, path, token) = listen(data)?;
    let closed = Arc::new(AtomicBool::new(false));
    let stop = closed.clone();
    std::thread::Builder::new()
        .name("templum-bus".into())
        .spawn(move || {
            loop {
                let stream = listener.accept();
                // Drop wakes this with a connection of its own.
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (tx, token) = (tx.clone(), token.clone());
                let _ = std::thread::Builder::new()
                    .name("templum-bus-reader".into())
                    .spawn(move || serve(stream, &tx, token.as_deref()));
            }
        })?;
    Ok(Bus { env, path, closed })
}

impl Drop for Bus {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let _ = connect(&self.env);
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Reads a connection's lines: its hello first, then each request or notification, handed to
/// `tx` with a replier; a line that isn't JSON-RPC is answered at once.
fn serve(stream: Stream, tx: &UnboundedSender<Message>, token: Option<&str>) {
    let Ok(writer) = stream.try_clone() else { return };
    let (reply, replies) = std::sync::mpsc::channel::<String>();
    let _ = std::thread::Builder::new()
        .name("templum-bus-writer".into())
        .spawn(move || write_lines(writer, &replies));
    let mut lines = BufReader::new(stream).lines();
    let Some(hello) = lines
        .next()
        .and_then(Result::ok)
        .and_then(|line| hello_of(&line, token))
    else {
        return;
    };
    for line in lines {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        match rpc::parse(&line) {
            Ok(incoming) => {
                let message = Message { hello: hello.clone(), incoming, reply: reply.clone() };
                if tx.unbounded_send(message).is_err() {
                    break;
                }
            }
            Err(answer) => {
                let _ = reply.send(answer);
            }
        }
    }
}

/// Writes the answers until every replier is gone, then closes the connection.
fn write_lines(mut stream: Stream, lines: &std::sync::mpsc::Receiver<String>) {
    for line in lines {
        if stream.write_all(line.as_bytes()).and_then(|()| stream.flush()).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

fn hello_of(line: &str, token: Option<&str>) -> Option<Hello> {
    let Ok(Incoming::Notification { method, params }) = rpc::parse(line) else {
        return None;
    };
    (method == HELLO && params["token"].as_str() == token).then(|| Hello {
        terminal: params["terminal"].as_str().unwrap_or_default().to_string(),
        client: params["client"].as_str().unwrap_or_default().to_string(),
    })
}

/// A client's first line.
pub fn hello_line(terminal: &str, client: &str, token: Option<&str>) -> String {
    rpc::notification(HELLO, json!({ "terminal": terminal, "client": client, "token": token }))
}

/// Connects to the bus at `env` (a `TEMPLUM_BUS`): the stream, and the token its hello carries.
pub fn connect(env: &str) -> io::Result<(Stream, Option<String>)> {
    let bad = || io::Error::new(io::ErrorKind::InvalidInput, format!("not a bus address: {env}"));
    if let Some(rest) = env.strip_prefix("tcp:") {
        let (address, token) = rest.rsplit_once(':').ok_or_else(bad)?;
        let address: std::net::SocketAddr = address.parse().map_err(|_| bad())?;
        let stream =
            std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(1))?;
        return Ok((Stream::Tcp(stream), Some(token.to_string())));
    }
    #[cfg(unix)]
    if let Some(path) = env.strip_prefix("unix:") {
        let stream = std::os::unix::net::UnixStream::connect(path)?;
        return Ok((Stream::Unix(stream), None));
    }
    Err(bad())
}

/// A connection to the bus.
pub enum Stream {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    Tcp(std::net::TcpStream),
}

impl Stream {
    pub fn try_clone(&self) -> io::Result<Stream> {
        Ok(match self {
            #[cfg(unix)]
            Stream::Unix(s) => Stream::Unix(s.try_clone()?),
            Stream::Tcp(s) => Stream::Tcp(s.try_clone()?),
        })
    }

    /// Writes no more: the other side reads the end, and may still answer.
    pub fn finish(&self) -> io::Result<()> {
        self.shutdown(Shutdown::Write)
    }

    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.shutdown(how),
            Stream::Tcp(s) => s.shutdown(how),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
            Stream::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
            Stream::Tcp(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
            Stream::Tcp(s) => s.flush(),
        }
    }
}

enum Listener {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixListener),
    #[cfg(windows)]
    Tcp(std::net::TcpListener),
}

impl Listener {
    fn accept(&self) -> io::Result<Stream> {
        match self {
            #[cfg(unix)]
            Listener::Unix(l) => l.accept().map(|(s, _)| Stream::Unix(s)),
            #[cfg(windows)]
            Listener::Tcp(l) => l.accept().map(|(s, _)| Stream::Tcp(s)),
        }
    }
}

/// Unix: a socket at `<data>/bus/<pid>-<n>.sock` in a folder only the user can open. The
/// counter keeps two buses of one process apart (tests open several workspaces).
#[cfg(unix)]
fn listen(data: &Path) -> io::Result<(Listener, String, Option<PathBuf>, Option<String>)> {
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::atomic::AtomicU64;
    let folder = data.join("bus");
    std::fs::create_dir_all(&folder)?;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?;
    remove_stale(&folder);
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!("{}-{}.sock", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let mut path = folder.join(&name);
    // macOS takes socket paths of 104 bytes at most.
    if path.as_os_str().len() > 100 {
        path = std::env::temp_dir().join(format!("templum-{name}"));
    }
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let env = format!("unix:{}", path.display());
    Ok((Listener::Unix(listener), env, Some(path), None))
}

/// Windows: TCP on the loopback at a port the system picks, and a token every hello carries.
#[cfg(windows)]
fn listen(_: &Path) -> io::Result<(Listener, String, Option<PathBuf>, Option<String>)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let env = format!("tcp:127.0.0.1:{port}:{token}");
    Ok((Listener::Tcp(listener), env, None, Some(token)))
}

/// Removes the socket files no Templum listens on any more (one that crashed).
#[cfg(unix)]
fn remove_stale(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for path in entries.flatten().map(|e| e.path()) {
        let refused = path.extension().is_some_and(|e| e == "sock")
            && std::os::unix::net::UnixStream::connect(&path)
                .is_err_and(|e| e.kind() == io::ErrorKind::ConnectionRefused);
        if refused {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// A bus in a temporary folder whose messages `answer` handles on a thread of its own.
#[cfg(test)]
pub(crate) fn test_bus(answer: impl Fn(Message) + Send + 'static) -> (tempfile::TempDir, Bus) {
    use futures::StreamExt as _;
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let bus = start(dir.path(), tx).unwrap();
    std::thread::spawn(move || {
        while let Some(message) = futures::executor::block_on(rx.next()) {
            answer(message);
        }
    });
    (dir, bus)
}
```

Notes for the implementer: `Value` is used by the tests (`use serde_json::Value;` inside `mod tests`, with `json`
from the module). If clippy reports `Stream::Tcp` as never built on Unix, it isn't: `connect` builds it for a
`tcp:` address on every OS.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-mcp`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-mcp
git commit -m "feat(connections): open a local socket the programs in Templum's terminals can talk to" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 4: `templum-mcp`: the command line and the MCP bridge

**Files:**
- Create: `crates/templum-mcp/src/cli.rs`
- Modify: `crates/templum-mcp/src/lib.rs` (`pub mod cli;`)

**Interfaces:**
- Consumes: `bus::{connect, hello_line, Stream, test_bus}`, `hooks::{claude, codex, METHOD}`, `rpc`, `tools::tool_result`.
- Produces:
  - `pub fn is_verb(arg: &str) -> bool` (`mcp`, `hook`, `whoami`, `blocks`, `read`, `send`, `notify`)
  - `pub struct Env { pub bus: Option<String>, pub terminal: Option<String> }` with `Env::of_process()`
  - `pub fn run(args: &[String], env: &Env, stdin: Box<dyn Read + Send>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32`
    (`args` without the program: the verb first)

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{Bus, test_bus};
    use crate::rpc::Incoming;
    use crate::tools::{Reply, tool_result};

    fn env(bus: &Bus) -> Env {
        Env { bus: Some(bus.env.clone()), terminal: Some("123-4".into()) }
    }

    fn run_with(args: &[&str], env: &Env, stdin: &str) -> (i32, String, String) {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let stdin = Box::new(io::Cursor::new(stdin.as_bytes().to_vec()));
        let code = run(&args, env, stdin, &mut out, &mut err);
        (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
    }

    #[test]
    fn the_bridge_says_hello_then_copies_both_ways() {
        let (_dir, bus) = test_bus(|m| {
            if let Incoming::Request { id, .. } = &m.incoming {
                let hello = json!({ "terminal": m.hello.terminal, "client": m.hello.client });
                let _ = m.reply.send(rpc::result(id, hello));
            }
        });
        let (code, out, _) = run_with(&["mcp"], &env(&bus), &rpc::request(7, "ping", Value::Null));
        assert_eq!(code, 0);
        let answer: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(answer["id"], 7);
        assert_eq!(answer["result"], json!({ "terminal": "123-4", "client": "mcp" }));
    }

    #[test]
    fn a_verb_is_one_tool_call_and_prints_its_text() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let reply = if params["arguments"]["block"] == "nope" {
                    Reply::err("nope isn't connected to you")
                } else {
                    Reply::ok("line 1\nline 2")
                };
                let _ = m.reply.send(tool_result(id, &reply));
            }
        });
        let (code, out, _) = run_with(&["read", "b7k2m", "-n", "10"], &env(&bus), "");
        assert_eq!((code, out.as_str()), (0, "line 1\nline 2\n"));
        assert_eq!(
            calls.recv().unwrap(),
            json!({ "name": "read_terminal", "arguments": { "block": "b7k2m", "lines": 10 } })
        );
        let (code, out, err) = run_with(&["read", "nope"], &env(&bus), "");
        assert_eq!((code, out.as_str(), err.as_str()), (1, "", "nope isn't connected to you\n"));
        run_with(&["send", "b7k2m", "make", "test", "--enter"], &env(&bus), "");
        assert_eq!(
            calls.recv().unwrap()["arguments"],
            json!({ "block": "b7k2m", "text": "make test", "enter": true })
        );
        let (code, out, _) = run_with(&["blocks", "--json"], &env(&bus), "");
        assert_eq!(code, 0);
        assert_eq!(serde_json::from_str::<Value>(&out).unwrap()["content"][0]["text"], "line 1\nline 2");
    }

    #[test]
    fn a_hook_sends_its_event_prints_nothing_and_never_fails() {
        let (seen, events) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            let _ = seen.send((m.hello.client, m.incoming));
        });
        let stop = r#"{"hook_event_name":"Stop","last_assistant_message":"Done."}"#;
        assert_eq!(run_with(&["hook", "claude"], &env(&bus), stop), (0, String::new(), String::new()));
        assert_eq!(
            events.recv().unwrap(),
            (
                "hook".to_string(),
                Incoming::Notification {
                    method: "templum/event".into(),
                    params: json!({ "event": "stopped", "message": "Done." }),
                }
            )
        );
        let codex = r#"{"type":"agent-turn-complete","last-assistant-message":"Fixed."}"#;
        assert_eq!(run_with(&["hook", "codex", codex], &env(&bus), "").0, 0);
        assert_eq!(events.recv().unwrap().1, Incoming::Notification {
            method: "templum/event".into(),
            params: json!({ "event": "stopped", "message": "Fixed." }),
        });
        // Outside Templum, and with what Templum doesn't follow: nothing, quietly.
        assert_eq!(run_with(&["hook", "claude"], &Env::default(), stop), (0, String::new(), String::new()));
        assert_eq!(run_with(&["hook", "claude"], &env(&bus), "not json"), (0, String::new(), String::new()));
    }

    #[test]
    fn outside_templum_or_with_it_gone_a_command_exits_2() {
        let (code, out, err) = run_with(&["blocks"], &Env::default(), "");
        assert_eq!((code, out.as_str()), (2, ""));
        assert!(err.contains("not inside a Templum terminal"), "{err}");
        let gone = Env {
            bus: Some(if cfg!(unix) { "unix:/nonexistent/templum.sock".into() } else { "tcp:127.0.0.1:9:x".into() }),
            terminal: Some("1-1".into()),
        };
        let (code, out, err) = run_with(&["mcp"], &gone, "");
        assert_eq!((code, out.as_str()), (2, ""));
        assert!(err.contains("Templum isn't running"), "{err}");
        let (code, _, err) = run_with(&["read"], &gone, "");
        assert_eq!(code, 2);
        assert!(err.starts_with("usage: templum read"), "{err}");
    }

    #[test]
    fn only_the_verbs_are_commands() {
        assert!(is_verb("mcp") && is_verb("hook") && is_verb("read"));
        assert!(!is_verb("--bench") && !is_verb("--claude-statusline") && !is_verb(""));
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp cli`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

```rust
//! The `templum` command line (docs/specs/connections.md, The templum command line): the MCP
//! bridge, the agents' hooks, and a verb per tool.

use std::io::{self, BufRead as _, BufReader, Read, Write};

use serde_json::{Value, json};

use crate::bus::{self, Stream};
use crate::{hooks, rpc};

/// The first arguments that make `templum` a command instead of the app.
const VERBS: [&str; 7] = ["mcp", "hook", "whoami", "blocks", "read", "send", "notify"];
const READ: &str = "read <block> [-n <lines>] [--last] [--json]";

pub fn is_verb(arg: &str) -> bool {
    VERBS.contains(&arg)
}

/// Where a command runs: its terminal's `TEMPLUM_BUS` and `TEMPLUM_TERMINAL`.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub bus: Option<String>,
    pub terminal: Option<String>,
}

impl Env {
    pub fn of_process() -> Env {
        Env {
            bus: std::env::var("TEMPLUM_BUS").ok(),
            terminal: std::env::var("TEMPLUM_TERMINAL").ok(),
        }
    }
}

/// Runs the command `args` (the verb first) and returns its exit code: 0 done, 1 refused or
/// failed, 2 a usage error, outside Templum, or Templum gone. A hook always returns 0 and prints
/// nothing: an agent mustn't stop over Templum, and Claude Code reads a hook's output.
pub fn run(
    args: &[String],
    env: &Env,
    mut stdin: Box<dyn Read + Send>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let (verb, rest) = match args.split_first() {
        Some((verb, rest)) => (verb.as_str(), rest),
        None => ("", &[][..]),
    };
    if verb == "hook" {
        if let Some(event) = hook_event(rest, &mut *stdin) {
            let _ = send_event(env, &event);
        }
        return 0;
    }
    let call = if verb == "mcp" {
        None
    } else {
        match call_of(verb, rest) {
            Ok(call) => Some(call),
            Err(usage) => {
                let _ = writeln!(stderr, "usage: templum {usage}");
                return 2;
            }
        }
    };
    let client = if call.is_some() { "cli" } else { "mcp" };
    let stream = match open(env, client) {
        Ok(stream) => stream,
        Err(why) => {
            let _ = writeln!(stderr, "templum: {why}");
            return 2;
        }
    };
    match call {
        None => bridge(stdin, stdout, stream),
        Some(call) => one_call(stream, &call, stdout, stderr),
    }
}

/// A verb's tool call, and whether it prints the whole result.
struct CallArgs {
    name: &'static str,
    arguments: Value,
    json: bool,
}

fn call_of(verb: &str, args: &[String]) -> Result<CallArgs, &'static str> {
    let json = args.iter().any(|a| a == "--json");
    let args: Vec<&str> = args.iter().map(String::as_str).filter(|a| *a != "--json").collect();
    let (name, arguments) = match (verb, args.as_slice()) {
        ("whoami", []) => ("whoami", json!({})),
        ("blocks", []) => ("list_blocks", json!({})),
        ("read", [block, more @ ..]) => {
            let mut arguments = json!({ "block": block });
            let mut more = more.iter();
            while let Some(arg) = more.next() {
                match *arg {
                    "--last" => arguments["last_command"] = json!(true),
                    "-n" => {
                        let n: u64 = more.next().and_then(|n| n.parse().ok()).ok_or(READ)?;
                        arguments["lines"] = json!(n);
                    }
                    _ => return Err(READ),
                }
            }
            ("read_terminal", arguments)
        }
        ("send", [block, text @ ..]) if !text.is_empty() => {
            let enter = text.contains(&"--enter");
            let text: Vec<&str> = text.iter().copied().filter(|a| *a != "--enter").collect();
            ("send_to_terminal", json!({ "block": block, "text": text.join(" "), "enter": enter }))
        }
        ("notify", text) if !text.is_empty() => ("notify", json!({ "text": text.join(" ") })),
        ("whoami", _) => return Err("whoami [--json]"),
        ("blocks", _) => return Err("blocks [--json]"),
        ("read", _) => return Err(READ),
        ("send", _) => return Err("send <block> <text> [--enter] [--json]"),
        _ => return Err("notify <text> [--json]"),
    };
    Ok(CallArgs { name, arguments, json })
}

/// Connects to the bus of the terminal this runs in and says hello.
fn open(env: &Env, client: &str) -> Result<Stream, &'static str> {
    let (Some(address), Some(terminal)) = (&env.bus, &env.terminal) else {
        return Err("not inside a Templum terminal");
    };
    let gone = "Templum isn't running";
    let (mut stream, token) = bus::connect(address).map_err(|_| gone)?;
    stream
        .write_all(bus::hello_line(terminal, client, token.as_deref()).as_bytes())
        .map_err(|_| gone)?;
    Ok(stream)
}

/// Copies the MCP client's lines (stdin) to the bus and the bus's back (stdout) until the bus
/// closes: once the client is done writing and answered, or when Templum is gone.
fn bridge(mut stdin: Box<dyn Read + Send>, stdout: &mut dyn Write, stream: Stream) -> i32 {
    let Ok(mut to_bus) = stream.try_clone() else {
        return 2;
    };
    std::thread::spawn(move || {
        let _ = io::copy(&mut stdin, &mut to_bus);
        let _ = to_bus.finish();
    });
    let mut from_bus = stream;
    let _ = io::copy(&mut from_bus, stdout);
    let _ = stdout.flush();
    0
}

/// Sends one tool call and prints its answer: its text, or with `--json` the whole result.
fn one_call(mut stream: Stream, call: &CallArgs, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let request = rpc::request(1, "tools/call", json!({ "name": call.name, "arguments": call.arguments }));
    if stream.write_all(request.as_bytes()).and_then(|()| stream.finish()).is_err() {
        let _ = writeln!(stderr, "templum: Templum isn't running");
        return 2;
    }
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let Ok(answer) = serde_json::from_str::<Value>(&line) else { continue };
        if answer["id"] != 1 {
            continue;
        }
        if let Some(message) = answer["error"]["message"].as_str() {
            let _ = writeln!(stderr, "templum: {message}");
            return 1;
        }
        let result = &answer["result"];
        let failed = result["isError"] == true;
        let text = if call.json {
            result.to_string()
        } else {
            result["content"][0]["text"].as_str().unwrap_or_default().to_string()
        };
        let out: &mut dyn Write = if failed { stderr } else { stdout };
        let _ = write!(out, "{text}");
        if !text.ends_with('\n') {
            let _ = writeln!(out);
        }
        return i32::from(failed);
    }
    let _ = writeln!(stderr, "templum: Templum closed the connection");
    2
}

/// The event an agent's hook hands over: Claude Code's on stdin, Codex's as an argument.
fn hook_event(args: &[String], stdin: &mut dyn Read) -> Option<hooks::AgentEvent> {
    match args.first()?.as_str() {
        "claude" => {
            let mut input = String::new();
            stdin.read_to_string(&mut input).ok()?;
            hooks::claude(&serde_json::from_str(&input).ok()?)
        }
        "codex" => hooks::codex(&serde_json::from_str(args.get(1)?).ok()?),
        _ => None,
    }
}

/// Tells the bus about an agent's turn; nothing when Templum isn't there.
fn send_event(env: &Env, event: &hooks::AgentEvent) -> Option<()> {
    let mut stream = open(env, "hook").ok()?;
    let line = rpc::notification(hooks::METHOD, serde_json::to_value(event).ok()?);
    stream.write_all(line.as_bytes()).ok()?;
    stream.finish().ok()
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-mcp`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-mcp
git commit -m "feat(connections): add the templum command: an MCP server for agents, their hooks, and verbs" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 5: `templum-terminal`: extra variables, history lines, the last command

**Files:**
- Modify: `crates/templum-terminal/src/terminal.rs` (`SpawnOptions` :505, env :554-566, `impl SharedTerm` :207,
  imports :13)
- Modify every `SpawnOptions { … }` literal: `crates/templum-terminal/src/integration.rs:412`,
  `crates/templum-terminal/tests/spawn.rs` (10 literals), `crates/templum/src/workspace/blocks.rs:344` (add
  `env: Vec::new()` for now; Task 8 fills it)
- Test: `crates/templum-terminal/src/terminal.rs` (`mod tests`), `crates/templum-terminal/tests/spawn.rs`

**Interfaces:**
- Produces: `SpawnOptions.env: Vec<(String, String)>`; `SharedTerm::lines(&self, count: usize) -> Vec<String>`;
  `SharedTerm::last_command(&self) -> Option<String>`.

- [ ] **Step 1: Write the failing tests**

In `terminal.rs`'s `mod tests`:

```rust
    #[test]
    fn lines_reach_into_the_history_without_the_blank_screen_below() {
        let (term, _) = headless(20, 4);
        for n in 1..=10 {
            feed(&term, format!("{n}\r\n").as_bytes());
        }
        // 10 lines on a 4-row screen: 7 in the history, the cursor on a blank last row.
        assert_eq!(term.lines(3), ["8", "9", "10"]);
        let all = term.lines(100);
        assert_eq!((all.len(), all[0].as_str()), (10, "1"));
        assert!(term.lines(0).is_empty());
    }

    #[test]
    fn the_last_command_is_its_prompt_and_output_up_to_the_next_prompt() {
        let (term, _) = headless(30, 8);
        feed_heard(
            &term,
            b"\x1b]133;A\x07$ ls\r\n\x1b]133;C\x07a b\r\n\x1b]133;D;0\x07\
              \x1b]133;A\x07$ make\r\n\x1b]133;C\x07error\r\n\x1b]133;D;2\x07\
              \x1b]133;A\x07$ ",
        );
        assert_eq!(term.last_command().as_deref(), Some("$ make\nerror"));
        let (fresh, _) = headless(30, 8);
        assert_eq!(fresh.last_command(), None, "no shell integration, no command");
    }
```

In `tests/spawn.rs`, after `each_terminal_gets_its_own_id_in_its_environment`:

```rust
/// What Templum adds to a terminal's environment reaches its programs (the bus's address).
#[test]
fn extra_variables_reach_the_shell() {
    #[cfg(unix)]
    let command = ("sh".to_string(), vec!["-c".to_string(), "printf 'bus=%s;' \"$TEMPLUM_BUS\"".to_string()]);
    #[cfg(windows)]
    let command = ("cmd".to_string(), vec!["/C".to_string(), "echo bus=%TEMPLUM_BUS%;".to_string()]);
    let term = Terminal::spawn(
        SpawnOptions {
            cols: 60,
            rows: 5,
            cell_px: (8, 16),
            command: Some(command),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: None,
            integration: None,
            env: vec![("TEMPLUM_BUS".into(), "unix:/x.sock".into())],
        },
        Arc::new(|| {}),
    )
    .expect("spawn");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !term.exited() {
        assert!(Instant::now() < deadline, "the command never exited");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut snapshot = Snapshot::default();
    snapshot.refresh(term.shared(), &Palette::default());
    assert!(snapshot.text().contains("bus=unix:/x.sock;"), "{:?}", snapshot.text());
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-terminal`
Expected: FAIL to compile (`lines`, `last_command`, `env` unknown).

- [ ] **Step 3: Implement**

`SpawnOptions` gains, after `integration`:

```rust
    /// More variables for the shell and its programs (`TEMPLUM_BUS`, `PATH`), set after
    /// Templum's own.
    pub env: Vec<(String, String)>,
```

In `Terminal::spawn`, after `env.insert("TEMPLUM_TERMINAL".into(), id.clone());`:

```rust
        env.extend(opts.env.iter().cloned());
```

Add `Line` to the `alacritty_terminal::index` import, then in `impl SharedTerm`:

```rust
    /// The last `count` lines of the history and the screen, as text, without the blank rows
    /// under the last one written. A wrapped line is one line.
    pub fn lines(&self, count: usize) -> Vec<String> {
        let term = self.0.lock_unfair();
        let (top, bottom) = (term.topmost_line().0, term.bottommost_line().0);
        // The blank rows under the cursor are at most a screen: read that much more.
        let from = (bottom + 1 - (count + term.screen_lines()) as i32).max(top);
        let mut lines = text_between(&term, from, bottom);
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        let skip = lines.len().saturating_sub(count);
        lines.drain(..skip);
        lines
    }

    /// The last command that finished (shell integration marks it), with its output, up to the
    /// next prompt. None without one.
    pub fn last_command(&self) -> Option<String> {
        let term = self.0.lock_unfair();
        let grid = term.grid();
        let starts = crate::tap::prompt_lines(grid);
        let marks = self.1.marks.lock().unwrap();
        let ended = |line: i32| {
            crate::tap::line_mark(grid, Line(line))
                .and_then(|prompt| marks.get(&prompt))
                .is_some_and(|mark| matches!(mark, Mark::Ended(_)))
        };
        let i = starts.iter().rposition(|&line| ended(line))?;
        let end = starts.get(i + 1).map_or(term.bottommost_line().0, |next| next - 1);
        let mut lines = text_between(&term, starts[i], end);
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        Some(lines.join("\n"))
    }
```

And a free function after `grid_point`:

```rust
/// The text of the grid's lines `top` to `bottom` (history lines are negative), one string per
/// line, trailing spaces trimmed.
fn text_between<T>(term: &Term<T>, top: i32, bottom: i32) -> Vec<String> {
    if top > bottom {
        return Vec::new();
    }
    let text = term.bounds_to_string(
        GridPoint::new(Line(top), Column(0)),
        GridPoint::new(Line(bottom), term.last_column()),
    );
    text.lines().map(|line| line.trim_end().to_string()).collect()
}
```

Then add `env: Vec::new(),` to every other `SpawnOptions` literal listed under **Files**.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-terminal && cargo build -p templum`
Expected: PASS, and the app still builds.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-terminal crates/templum/src/workspace/blocks.rs
git commit -m "feat(terminal): read a terminal's history and its last command as text" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 6: `templum-usage`: the hooks and the MCP config Claude Code is given

**Files:**
- Modify: `crates/templum-usage/src/claude.rs` (`settings_arg` :179, new `mcp_arg`, consts by `SETTINGS` :23)
- Modify: `crates/templum/src/workspace/usage.rs:41` (the one caller: `settings_arg(data, &exe, cfg!(windows),
  true, false)` until Task 10)
- Test: `crates/templum-usage/src/claude.rs` tests (:684 and new)

**Interfaces:**
- Produces: `settings_arg(data: &Path, exe: &Path, windows: bool, status_line: bool, hooks: bool) -> Option<String>`;
  `mcp_arg(data: &Path, exe: &Path, windows: bool) -> Option<String>`.

- [ ] **Step 1: Write the failing tests**

Update the existing `the_settings_file_names_templum_as_the_status_line` to call `settings_arg(…, false, true,
false)` (three places), then add:

```rust
    #[cfg(unix)]
    #[test]
    fn the_settings_file_holds_the_status_line_the_hooks_or_both() {
        let dir = tempfile::tempdir().unwrap();
        let exe = Path::new("/opt/templum");
        let read = || {
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(dir.path().join(SETTINGS)).unwrap())
                .unwrap()
        };
        settings_arg(dir.path(), exe, false, false, true).unwrap();
        let written = read();
        assert!(written.get("statusLine").is_none());
        let hooks = written["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), HOOKED.len());
        assert_eq!(hooks["Stop"][0]["hooks"][0]["command"], "/opt/templum hook claude");
        assert_eq!(hooks["PostToolUse"][0]["hooks"][0]["timeout"], 5);
        settings_arg(dir.path(), exe, false, true, true).unwrap();
        assert!(read()["statusLine"].is_object() && read()["hooks"].is_object());
        assert_eq!(settings_arg(dir.path(), exe, false, false, false), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_mcp_config_names_templums_server() {
        let dir = tempfile::tempdir().unwrap();
        let typed = mcp_arg(dir.path(), Path::new("/opt/Templum App/templum"), false).unwrap();
        assert_eq!(typed, dir.path().join(MCP).display().to_string());
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join(MCP)).unwrap()).unwrap();
        // Not typed in a shell: the path as it is.
        assert_eq!(
            written["mcpServers"]["templum"],
            serde_json::json!({ "type": "stdio", "command": "/opt/Templum App/templum", "args": ["mcp"] })
        );
        assert_eq!(mcp_arg(Path::new("/home/$x"), Path::new("/opt/templum"), false), None);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-usage claude`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

Beside `SETTINGS`:

```rust
/// The MCP config the Claude Code Templum opens is given (`--mcp-config`).
const MCP: &str = "mcp.json";
/// What each hook runs after Templum's program (docs/specs/connections.md, Hooks).
const HOOK: &str = "hook claude";
/// The Claude Code hooks Templum follows a session's turns with.
const HOOKED: [&str; 6] = [
    "SessionStart",
    "UserPromptSubmit",
    "PostToolUse",
    "Stop",
    "Notification",
    "SessionEnd",
];
```

Replace `settings_arg` (keep its doc comment, extended):

```rust
/// Writes the settings file the Claude Code Templum opens is given, with Templum's data folder
/// `data` and its program `exe`: Templum's status line when `status_line`, and the hooks that
/// tell Templum its turns when `hooks`. Returns its path as typed after `--settings` in a shell;
/// None when there is nothing to give, or a path can't be typed safely in every shell.
pub fn settings_arg(data: &Path, exe: &Path, windows: bool, status_line: bool, hooks: bool) -> Option<String> {
    let path = data.join(SETTINGS);
    let typed = typed_path(&path.to_string_lossy(), windows)?;
    // Linux names a program replaced while it runs (an upgrade) "<path> (deleted)": the new one
    // is at the path.
    let exe = exe.to_string_lossy();
    let exe = typed_path(exe.strip_suffix(" (deleted)").unwrap_or(&exe), windows)?;
    let mut settings = serde_json::Map::new();
    if status_line {
        // The relay is told the folder: Claude Code's environment may name another one.
        let folder = typed_path(&data.to_string_lossy(), windows)?;
        settings.insert(
            "statusLine".into(),
            serde_json::json!({
                "type": "command",
                "command": format!("{exe} {RELAY_FLAG} {folder}"),
                "padding": 0,
            }),
        );
    }
    if hooks {
        let hook = serde_json::json!([{
            "hooks": [{ "type": "command", "command": format!("{exe} {HOOK}"), "timeout": 5 }]
        }]);
        let all: serde_json::Map<String, serde_json::Value> =
            HOOKED.iter().map(|event| (event.to_string(), hook.clone())).collect();
        settings.insert("hooks".into(), all.into());
    }
    if settings.is_empty() {
        return None;
    }
    write_changed(&path, &serde_json::Value::Object(settings))?;
    Some(typed)
}

/// Writes the MCP config the Claude Code Templum opens is given: Templum's own server, `exe
/// mcp`. Returns its path as typed after `--mcp-config` in a shell; None when it can't be.
pub fn mcp_arg(data: &Path, exe: &Path, windows: bool) -> Option<String> {
    let path = data.join(MCP);
    let typed = typed_path(&path.to_string_lossy(), windows)?;
    let exe = exe.to_string_lossy();
    let exe = exe.strip_suffix(" (deleted)").unwrap_or(&exe);
    let config = serde_json::json!({
        "mcpServers": { "templum": { "type": "stdio", "command": exe, "args": ["mcp"] } }
    });
    write_changed(&path, &config)?;
    Some(typed)
}

/// Writes `value` to `path` unless the file already holds it.
fn write_changed(path: &Path, value: &serde_json::Value) -> Option<()> {
    let text = serde_json::to_vec_pretty(value).expect("JSON");
    if std::fs::read(path).ok().as_deref() != Some(&text[..]) {
        write_atomic(path, &text).ok()?;
    }
    Some(())
}
```

Change the caller in `crates/templum/src/workspace/usage.rs:41` to `settings_arg(data, &exe, cfg!(windows), true,
false)`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-usage && cargo test -p templum usage`
Expected: PASS (the app's `the_claude_code_templum_opens_gets_its_status_line` unchanged).

- [ ] **Step 5: Commit**

```bash
git add crates/templum-usage crates/templum/src/workspace/usage.rs
git commit -m "feat(agents): write the hooks and the MCP server the Claude Code Templum opens can be given" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 7: Block ids

**Files:**
- Modify: `crates/templum/src/session.rs` (the five item structs :433-607, `Item` :421, `LayoutBlock` :640-645,
  `Layout::snapshot` :659-703, the tests' literals :866-1021 and :1455-1460)
- Modify: `crates/templum/src/workspace/mod.rs` (`Space` :386, `Space::layout` :481, `Space::empty` :531,
  `Body::item` :281 literals)
- Modify: `crates/templum/src/editor.rs:540`, `crates/templum/src/git_block/mod.rs:643` (item literals)
- Modify: `crates/templum/src/workspace/blocks.rs` (`added` :37, `close_blocks` :491, `take_block` :529)
- Modify: `crates/templum/src/workspace/history.rs` (`reopen` :123)
- Modify: `crates/templum/src/workspace/persist.rs` (`restore` :68)
- Modify: `crates/templum/src/workspace/spaces.rs` (`move_blocks` :296-325)
- Modify the tests' item literals: `workspace/tests/mod.rs:575-586`, `workspace/tests/selection.rs:277`,
  `workspace/tests/workspaces.rs:17`
- Create: `crates/templum/src/workspace/tests/block_ids.rs` (and `mod block_ids;` in `tests/mod.rs`)

**Interfaces:**
- Produces: `Space::uids: HashMap<BlockId, String>`; `Space::block_by_uid(&self, uid: &str) -> Option<BlockId>`;
  `Space::item_of(&self, block: &Block, cx: &App) -> session::Item`; `Workspace::adopt_uid(&mut self, id: BlockId,
  uid: &str)`; `session::Item::{uid, set_uid}`; `LayoutBlock::Terminal(Rect, Processes, String)`.

- [ ] **Step 1: Write the failing tests**

`crates/templum/src/workspace/tests/block_ids.rs`:

```rust
//! Block ids: what agents and wires name blocks by (docs/specs/connections.md, Identity).

use super::*;
use crate::workspace::history::Then;

fn uid(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> String {
    ws.read_with(cx, |ws, _| ws.space.uids[&id].clone())
}

fn all_uids(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<String> {
    ws.read_with(cx, |ws, _| {
        ws.space.canvas.blocks().iter().map(|b| ws.space.uids[&b.id].clone()).collect()
    })
}

#[gpui_kit::test]
fn every_block_gets_its_own_id(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let ids = [kanban, note, terminal].map(|b| uid(&ws, cx, b));
    for id in &ids {
        assert_eq!(id.len(), 5, "{id}");
        assert!(id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()), "{id}");
    }
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2], "{ids:?}");
}

#[gpui_kit::test]
fn a_blocks_id_comes_back_after_a_restart(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    three_blocks(handle, &ws, cx);
    let before = all_uids(&ws, cx);
    ws.update(cx, |ws, cx| ws.save_canvas_now(cx));
    let saved = std::fs::read_to_string(data.path().join("workspaces").join("default.json")).unwrap();
    assert!(saved.contains(&format!("\"uid\":\"{}\"", before[0])), "{saved}");
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    assert_eq!(all_uids(&again, cx), before);
}

#[gpui_kit::test]
fn closing_and_undoing_keeps_a_blocks_id(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, _) = three_blocks(handle, &ws, cx);
    let before = uid(&ws, cx, kanban);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.close_blocks(vec![kanban], Then::New, window, cx);
            assert!(!ws.space.uids.values().any(|u| *u == before), "gone with its block");
            ws.undo_canvas(false, window, cx);
        })
    })
    .unwrap();
    assert!(all_uids(&ws, cx).contains(&before));
}

#[gpui_kit::test]
fn a_block_moved_to_another_workspace_keeps_its_id(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let before = uid(&ws, cx, first);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.switch_to(0, window, cx);
            ws.move_blocks(vec![first], Some("w2"), window, cx);
        })
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.uids.is_empty());
        assert_eq!(ws.others[0].uids.values().collect::<Vec<_>>(), [&before]);
    });
}

#[gpui_kit::test]
fn an_id_already_taken_or_malformed_is_replaced(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    // Two blocks claim one id (a file edited by hand), a third has one that isn't an id.
    let kanban = |x: f32, uid: &str| {
        format!(r#"{{"kind":"kanban","uid":"{uid}","rect":{{"x":{x},"y":0,"w":300,"h":200}}}}"#)
    };
    std::fs::create_dir_all(data.path().join("workspaces")).unwrap();
    std::fs::write(
        data.path().join("workspaces").join("default.json"),
        format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[{},{},{}]}}"#,
            kanban(0.0, "aaaaa"),
            kanban(400.0, "aaaaa"),
            kanban(800.0, "../x")
        ),
    )
    .unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let ids = all_uids(&ws, cx);
    assert_eq!(ids[0], "aaaaa");
    assert_ne!(ids[1], "aaaaa");
    assert_ne!(ids[2], "../x");
    assert_eq!(ids[2].len(), 5);
}
```

(A pasted block getting a new id needs no code: copying uses `Body::item`, which carries none, and `add_saved` never
adopts one. `an_id_already_taken_or_malformed_is_replaced` covers adopting.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum block_ids`
Expected: FAIL to compile (`uids` unknown).

- [ ] **Step 3: Implement**

`session.rs`, first field of `TerminalItem`, `KanbanItem`, `NoteItem`, `EditorItem`, `GitItem`:

```rust
    /// The block's id for agents and wires (docs/specs/connections.md); a file from before has
    /// none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uid: String,
```

`impl Item`, beside `rect_mut`:

```rust
    pub fn uid(&self) -> &str {
        match self {
            Item::Terminal(t) => &t.uid,
            Item::Kanban(k) => &k.uid,
            Item::Note(n) => &n.uid,
            Item::Editor(e) => &e.uid,
            Item::Git(g) => &g.uid,
        }
    }

    pub fn set_uid(&mut self, uid: String) {
        match self {
            Item::Terminal(t) => t.uid = uid,
            Item::Kanban(k) => k.uid = uid,
            Item::Note(n) => n.uid = uid,
            Item::Editor(e) => e.uid = uid,
            Item::Git(g) => g.uid = uid,
        }
    }
```

`LayoutBlock::Terminal(Rect, Processes, String)` (doc: "A terminal's rect, its processes, read in the background,
and its block id."); in `Layout::snapshot` match `LayoutBlock::Terminal(_, p, _)` and
`LayoutBlock::Terminal(rect, _, uid)` building `TerminalItem { uid: uid.clone(), … }`. Every other item literal
(including the tests' and `editor.rs`/`git_block`'s) gets `uid: String::new(),`.

`workspace/mod.rs`, `Space`, after `blocks`:

```rust
    /// Each block's id for agents and wires (docs/specs/connections.md): unique in every
    /// workspace, kept by restarts, undo and moves.
    pub uids: HashMap<BlockId, String>,
```

`impl Space`:

```rust
    /// The block whose id is `uid`.
    pub fn block_by_uid(&self, uid: &str) -> Option<BlockId> {
        self.uids.iter().find(|(_, u)| *u == uid).map(|(id, _)| *id)
    }

    /// A block as a saved item at its rect on the canvas, with its id.
    fn item_of(&self, block: &Block, cx: &App) -> session::Item {
        let mut item = self.blocks[&block.id].item(self.home(block), cx);
        item.set_uid(self.uids.get(&block.id).cloned().unwrap_or_default());
        item
    }
```

`Space::layout`: `Body::Terminal(pane) => session::LayoutBlock::Terminal(self.home(b), pane.term.processes(),
self.uids.get(&b.id).cloned().unwrap_or_default())`, and `_ => session::LayoutBlock::Saved(self.item_of(b, cx))`.
`Space::empty`: `self.uids.clear();`.

`workspace/blocks.rs`:

```rust
    /// A block was just added: it is focused, pops in, and Ctrl+Z takes it away (unless the
    /// canvas is being restored or blocks come back by an undo: `quiet`). It gets a new id.
    fn added(&mut self, id: BlockId) {
        let uid = self.new_uid();
        self.space.uids.insert(id, uid);
        // … the rest as it is
    }

    /// A block id no block of any workspace has: five letters and digits.
    // ponytail: a stopped workspace's saved ids aren't checked (1 in 60 million per pair); check
    // `Space::saved` too if one ever collides.
    fn new_uid(&self) -> String {
        loop {
            let uid = base36(uuid::Uuid::new_v4().as_u128() % 36u128.pow(5), 5);
            if !self.uid_taken(&uid) {
                return uid;
            }
        }
    }

    fn uid_taken(&self, uid: &str) -> bool {
        self.spaces().any(|s| s.uids.values().any(|u| u == uid))
    }

    /// Block `id`, just restored, takes back its saved id `uid`; one that is malformed or taken
    /// (a file edited by hand) leaves it the new one it got.
    pub(super) fn adopt_uid(&mut self, id: BlockId, uid: &str) {
        let valid = (1..=12).contains(&uid.len())
            && uid.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        if valid && !self.uid_taken(uid) {
            self.space.uids.insert(id, uid.to_string());
        }
    }
```

and, at the end of the file:

```rust
/// `n` in base 36 (digits, then lowercase letters), `width` characters, zeros in front.
fn base36(mut n: u128, width: usize) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut text = vec![b'0'; width];
    for c in text.iter_mut().rev() {
        *c = DIGITS[(n % 36) as usize];
        n /= 36;
    }
    String::from_utf8(text).expect("ASCII")
}
```

`close_blocks` (:491): `.map(|b| (b.id, self.space.item_of(b, cx)))`. `take_block`: after
`let body = self.space.blocks.remove(&id);` add `self.space.uids.remove(&id);`.

`history.rs` `reopen`, after `let added = self.add_selected(&items, window, cx);`:

```rust
        for (item, new) in items.iter().zip(&added) {
            if let Some(new) = new {
                self.adopt_uid(*new, item.uid());
            }
        }
```

`persist.rs` `restore`, after `self.quiet = false;`:

```rust
        for (item, id) in saved.items.iter().zip(&ids) {
            if let Some(id) = id {
                self.adopt_uid(*id, item.uid());
            }
        }
```

`spaces.rs` `move_blocks`: take each block's uid before `take_block` and give it back after `add_block`:

```rust
        let taken: Vec<(Rect, Body, Option<String>)> = order
            .into_iter()
            .filter_map(|id| {
                let uid = self.space.uids.get(&id).cloned();
                let (_, home, body) = self.take_block(id, window, cx)?;
                Some((home, body, uid))
            })
            .collect();
        let Some(bounds) = taken.iter().map(|(r, _, _)| *r).reduce(|a, b| a.union(&b)) else {
            return;
        };
        // … inside with_space:
            for (r, mut body, uid) in taken {
                // … the match as it is
                let id = ws.add_block(Rect::new(r.x + dx, r.y + dy, r.w, r.h), body);
                if let Some(uid) = uid {
                    ws.space.uids.insert(id, uid);
                }
                moved.push(id);
            }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum block_ids && cargo test -p templum persistence && cargo test -p templum moving &&
cargo test -p templum history`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum
git commit -m "feat(canvas): give every block an id that lasts through restarts, undo and moves" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 8: The bus in the app: `[connections]`, the verbs, `TEMPLUM_BUS`, and the tools

**Files:**
- Modify: `crates/templum/Cargo.toml` (`templum-mcp = { path = "../templum-mcp" }` with a comment like the others)
- Modify: `crates/templum/src/main.rs:47-52` (the verbs, before any window)
- Modify: `crates/templum/src/config.rs` (`ConnectionsConfig`, `Config.connections`, the template after
  `[updates]`, a test)
- Modify: `crates/templum/src/workspace/mod.rs` (`mod bus;`, `Workspace.bus`, `bus_env`, `_bus`, built in `with`)
- Modify: `crates/templum/src/workspace/blocks.rs:344` (`env: self.bus_env.clone()`)
- Modify: `crates/templum/src/workspace/usage.rs` (`pub(super) fn program()`)
- Create: `crates/templum/src/workspace/bus.rs`
- Create: `crates/templum/src/workspace/tests/bus.rs` (and `mod bus;` in `tests/mod.rs`)

**Interfaces:**
- Consumes: `templum_mcp::{bus, cli, rpc, tools, Message, Hello, Incoming, Call, Reply}`; `Space::{uids,
  block_by_uid}` (Task 7); `SharedTerm::{lines, last_command}` (Task 5).
- Produces: `Workspace.bus: Option<templum_mcp::Bus>`; `Workspace::serve(&mut self, Message, &mut Window, &mut
  Context<Self>)`; `Workspace::caller(&self, terminal: &str) -> Option<(String, BlockId)>`; `workspace::usage::program()
  -> Option<PathBuf>`; `config.connections.enabled`.

- [ ] **Step 1: Write the failing tests**

`config.rs` tests:

```rust
    #[test]
    fn connections_are_on_unless_turned_off() {
        assert!(parse("").unwrap().connections.enabled);
        assert!(!parse("[connections]\nenabled = false\n").unwrap().connections.enabled);
        let err = parse("[connections]\nmcp = true\n").unwrap_err();
        assert!(err.contains("mcp"), "{err}");
    }
```

`crates/templum/src/workspace/tests/bus.rs`:

```rust
//! The bus: agents and commands asking the workspace (docs/specs/connections.md).

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use templum_mcp::{Hello, Incoming, Message};

use super::*;

/// The workspace's answer to `method`, asked as the terminal `terminal` would.
fn ask(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    terminal: &str,
    method: &str,
    params: Value,
) -> Value {
    let (reply, answers) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello { terminal: terminal.into(), client: "cli".into() },
        incoming: Incoming::Request { id: json!(1), method: method.into(), params },
        reply,
    };
    cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.serve(message, window, cx)))
        .unwrap();
    serde_json::from_str(&answers.try_recv().expect("answered at once")).unwrap()
}

/// A tool's answer: whether it is an error, and its text.
fn call(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    terminal: &str,
    tool: &str,
    arguments: Value,
) -> (bool, String) {
    let answer = ask(handle, ws, cx, terminal, "tools/call", json!({ "name": tool, "arguments": arguments }));
    let result = &answer["result"];
    (result["isError"] == true, result["content"][0]["text"].as_str().unwrap().to_string())
}

/// The terminal `id`'s `TEMPLUM_TERMINAL` and block id.
fn me(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> (String, String) {
    ws.read_with(cx, |ws, _| {
        (ws.space.terminal(id).unwrap().term.id().to_string(), ws.space.uids[&id].clone())
    })
}

/// The terminal's screen once it shows `needle`.
fn screen_with(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = templum_terminal::Snapshot::default();
            snap.refresh(ws.space.terminal(id).unwrap().term.shared(), &ws.theme.palette);
            snap.text()
        });
        if shown.contains(needle) {
            return shown;
        }
        assert!(Instant::now() < deadline, "no {needle:?} in {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn whoami_names_the_callers_block_and_workspace(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, uid) = me(&ws, cx, first_id(&ws, cx));
    let (failed, text) = call(handle, &ws, cx, &terminal, "whoami", json!({}));
    assert!(!failed, "{text}");
    assert!(text.contains(&format!("id: {uid}")), "{text}");
    assert!(text.contains("kind: terminal") && text.contains("workspace: Main"), "{text}");
    let (failed, text) = call(handle, &ws, cx, "0-0", "whoami", json!({}));
    assert!(failed && text.contains("isn't a block"), "{text}");
}

#[gpui_kit::test]
fn initialize_lists_the_tools_and_says_who_the_agent_is(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, uid) = me(&ws, cx, first_id(&ws, cx));
    let init = ask(handle, &ws, cx, &terminal, "initialize", json!({ "protocolVersion": "2025-06-18" }));
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains(&format!("You are block {uid}")), "{instructions}");
    let tools = ask(handle, &ws, cx, &terminal, "tools/list", Value::Null);
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 5);
    assert_eq!(ask(handle, &ws, cx, &terminal, "ping", Value::Null)["result"], json!({}));
    let unknown = ask(handle, &ws, cx, &terminal, "resources/list", Value::Null);
    assert_eq!(unknown["error"]["code"], templum_mcp::rpc::METHOD);
    let unknown = ask(handle, &ws, cx, &terminal, "tools/call", json!({ "name": "rm_rf" }));
    assert_eq!(unknown["error"]["code"], templum_mcp::rpc::PARAMS);
    let (failed, text) = call(handle, &ws, cx, &terminal, "read_terminal", json!({}));
    assert!(failed && text.contains("\"block\""), "{text}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_terminal_types_into_itself_and_reads_it_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (terminal, uid) = me(&ws, cx, first_id(&ws, cx));
    let typed = json!({ "block": uid, "text": "hello there", "enter": true });
    let (failed, text) = call(handle, &ws, cx, &terminal, "send_to_terminal", typed);
    assert!(!failed, "{text}");
    screen_with(&ws, cx, first_id(&ws, cx), "hello there");
    let (failed, text) = call(handle, &ws, cx, &terminal, "read_terminal", json!({ "block": uid, "lines": 5 }));
    assert!(!failed && text.contains("hello there"), "{text}");
}

#[gpui_kit::test]
fn another_block_is_refused_until_it_is_connected(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // `three_blocks` puts the note on top: the terminal is the one it returns.
    let (kanban, _, block) = three_blocks(handle, &ws, cx);
    let (terminal, uid) = me(&ws, cx, block);
    let kanban = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let (failed, text) = call(handle, &ws, cx, &terminal, "read_terminal", json!({ "block": kanban }));
    assert!(failed && text.contains("isn't connected to you"), "{text}");
    let (failed, text) = call(handle, &ws, cx, &terminal, "read_terminal", json!({ "block": "zzzzz" }));
    assert!(failed && text.contains("no block zzzzz"), "{text}");
    let (_, list) = call(handle, &ws, cx, &terminal, "list_blocks", json!({}));
    let line = |id: &str| list.lines().find(|l| l.starts_with(id)).unwrap_or_default().to_string();
    assert!(line(&uid).ends_with("you"), "{list}");
    assert!(line(&kanban).contains("kanban") && line(&kanban).ends_with("not connected"), "{list}");
}

#[gpui_kit::test]
fn the_bus_answers_over_its_socket(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let (terminal, uid) = me(&ws, cx, first_id(&ws, cx));
    let env = ws.read_with(cx, |ws, _| ws.bus.as_ref().expect("a bus with a data folder").env.clone());
    let (tx, answers) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::{BufRead as _, Write as _};
        let (mut stream, token) = templum_mcp::bus::connect(&env).unwrap();
        let hello = templum_mcp::bus::hello_line(&terminal, "cli", token.as_deref());
        let call = templum_mcp::rpc::request(1, "tools/call", json!({ "name": "whoami", "arguments": {} }));
        stream.write_all(format!("{hello}{call}").as_bytes()).unwrap();
        let mut line = String::new();
        std::io::BufReader::new(stream).read_line(&mut line).unwrap();
        let _ = tx.send(line);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let line = loop {
        cx.run_until_parked();
        if let Ok(line) = answers.try_recv() {
            break line;
        }
        assert!(Instant::now() < deadline, "no answer");
        std::thread::sleep(Duration::from_millis(20));
    };
    let answer: Value = serde_json::from_str(&line).unwrap();
    let text = answer["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains(&format!("id: {uid}")), "{text}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn terminals_get_the_bus_and_this_templum_first_on_their_path(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let script = "printf 'bus=%s;\\n' \"$TEMPLUM_BUS\"; command -v templum; exec cat";
    let command = ("sh".to_string(), vec!["-c".to_string(), script.to_string()]);
    let (_, ws) = open_as(cx, Some(command), None, Some(data.path().to_path_buf()));
    let env = ws.read_with(cx, |ws, _| ws.bus.as_ref().unwrap().env.clone());
    let shown = screen_with(&ws, cx, first_id(&ws, cx), "bin/templum");
    assert!(shown.contains(&format!("bus={env};")), "{shown}");
    let link = data.path().join("bin").join("templum");
    assert_eq!(std::fs::read_link(&link).unwrap(), crate::workspace::usage::program().unwrap());
}

#[gpui_kit::test]
fn without_a_data_folder_there_is_no_bus(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.read_with(cx, |ws, _| assert!(ws.bus.is_none()));
}
```

(`workspace::usage` is a private module; the test reaches it as a descendant of `workspace`. If the path doesn't
resolve, make `program` `pub(crate)` and `mod usage` stays private.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum bus:: && cargo test -p templum connections_are_on`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `[connections]`**

After `UpdatesConfig` in `config.rs`:

```rust
/// `[connections]`: Templum's MCP server and hooks for the agents it opens
/// (docs/specs/connections.md).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ConnectionsConfig {
    /// The Claude Code and Codex Templum opens get its MCP server and hooks.
    pub enabled: bool,
}

impl Default for ConnectionsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}
```

`Config` gains `pub connections: ConnectionsConfig,` after `updates`. The template, after the `[updates]` lines:

```rust
        "[connections]\n",
        "## The Claude Code and Codex Templum opens get its MCP server and hooks: they can read and\n",
        "## type into the blocks wired to them (docs/specs/connections.md).\n",
        "# enabled = true\n\n",
```

- [ ] **Step 4: Implement the verbs in `main.rs`**

After the `--claude-statusline` branch:

```rust
    // `templum mcp`, `templum hook claude`, `templum read <block>`…: no window
    // (docs/specs/connections.md).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| templum_mcp::cli::is_verb(a)) {
        let code = templum_mcp::cli::run(
            &args,
            &templum_mcp::cli::Env::of_process(),
            Box::new(std::io::stdin()),
            &mut std::io::stdout(),
            &mut std::io::stderr(),
        );
        std::process::exit(code);
    }
```

- [ ] **Step 5: Implement `program()` in `workspace/usage.rs`**

```rust
/// Templum's program as an agent should run it: the AppImage when it runs from one (its mount
/// goes away when it quits), else the running executable, without the " (deleted)" Linux adds
/// to one an upgrade replaced.
pub(super) fn program() -> Option<PathBuf> {
    if let Some(image) = std::env::var_os("APPIMAGE") {
        return Some(image.into());
    }
    let exe = std::env::current_exe().ok()?;
    Some(match exe.to_str().and_then(|e| e.strip_suffix(" (deleted)")) {
        Some(path) => PathBuf::from(path),
        None => exe,
    })
}
```

- [ ] **Step 6: Implement `workspace/bus.rs`**

```rust
//! The bus's messages, answered on the UI thread (docs/specs/connections.md): who calls, the
//! tools, and the agents' turns.

use std::path::{Path, PathBuf};

use futures::StreamExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Task, Window};
use serde_json::{Value, json};
use templum_canvas::BlockId;
use templum_mcp::{Bus, Call, Incoming, Message, Reply, rpc, tools};

use super::{Body, Space, Workspace, show_notification};

/// Starts the bus in the data folder, and the task that hands its messages to the workspace.
/// Without a data folder (the bench), or when the socket can't be made, there is none.
pub(super) fn start(
    data: Option<&Path>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> (Option<Bus>, Vec<(String, String)>, Task<()>) {
    let Some(data) = data else {
        return (None, Vec::new(), Task::ready(()));
    };
    let (tx, mut rx) = futures::channel::mpsc::unbounded::<Message>();
    let bus = match templum_mcp::bus::start(data, tx) {
        Ok(bus) => bus,
        Err(err) => {
            eprintln!("templum: couldn't start the bus in {}: {err}", data.display());
            return (None, Vec::new(), Task::ready(()));
        }
    };
    let env = terminal_env(&bus, data);
    let task = cx.spawn_in(window, async move |ws, cx| {
        while let Some(message) = rx.next().await {
            if ws.update_in(cx, |ws, window, cx| ws.serve(message, window, cx)).is_err() {
                break;
            }
        }
    });
    (Some(bus), env, task)
}

/// What every terminal gets while the bus runs: its address and, on Unix, Templum's `bin`
/// folder first in PATH, where `templum` is this Templum.
fn terminal_env(bus: &Bus, data: &Path) -> Vec<(String, String)> {
    let mut env = vec![("TEMPLUM_BUS".to_string(), bus.env.clone())];
    if let Some(path) = templum_first(data) {
        env.push(("PATH".to_string(), path));
    }
    env
}

/// PATH with `<data>/bin` in front, where a link named `templum` points at this Templum.
// ponytail: one link for every Templum sharing the data folder: the last one started wins.
#[cfg(unix)]
fn templum_first(data: &Path) -> Option<String> {
    let exe = super::usage::program()?;
    let bin = data.join("bin");
    std::fs::create_dir_all(&bin).ok()?;
    let link = bin.join("templum");
    if std::fs::read_link(&link).ok().as_deref() != Some(exe.as_path()) {
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&exe, &link).ok()?;
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let paths = std::iter::once(bin).chain(std::env::split_paths(&path));
    std::env::join_paths(paths).ok()?.into_string().ok()
}

#[cfg(not(unix))]
fn templum_first(_: &Path) -> Option<String> {
    None
}

impl Workspace {
    /// Answers one message from the bus.
    pub(super) fn serve(&mut self, message: Message, window: &mut Window, cx: &mut Context<Self>) {
        let Message { hello, incoming, reply } = message;
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
                        if tools::known(name) {
                            let reply = match tools::parse(name, &params["arguments"]) {
                                Ok(call) => self.call(&hello.terminal, call, window, cx),
                                Err(why) => Reply::err(why),
                            };
                            tools::tool_result(&id, &reply)
                        } else {
                            rpc::error(&id, rpc::PARAMS, &format!("Unknown tool: {name}"))
                        }
                    }
                    _ => rpc::error(&id, rpc::METHOD, &format!("Method not found: {method}")),
                };
                let _ = reply.send(answer);
            }
            // Task 9 adds `templum/event`. `notifications/initialized` and the rest need nothing.
            Incoming::Notification { .. } | Incoming::Response => {}
        }
    }

    /// The block whose terminal is `terminal` (its `TEMPLUM_TERMINAL`): its workspace's id and
    /// its id. Looked up at each call: a block moved to another workspace keeps its terminal.
    pub(super) fn caller(&self, terminal: &str) -> Option<(String, BlockId)> {
        self.spaces().find_map(|space| {
            space
                .terminals()
                .find(|(_, pane)| pane.term.id() == terminal)
                .map(|(id, _)| (space.id.clone(), id))
        })
    }

    fn space_named(&self, id: &str) -> Option<&Space> {
        self.spaces().find(|s| s.id == id)
    }

    fn call(&mut self, terminal: &str, call: Call, window: &mut Window, cx: &mut Context<Self>) -> Reply {
        let Some((space_id, me)) = self.caller(terminal) else {
            return Reply::err("This terminal isn't a block of this Templum.");
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        match call {
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
            Call::ReadTerminal { block, lines, last_command } => {
                let id = match target(space, me, &block) {
                    Ok(id) => id,
                    Err(reply) => return reply,
                };
                let Some(pane) = space.terminal(id) else {
                    return Reply::err(format!("{block} isn't a terminal."));
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
                    Reply::ok(tools::tail(&shared.lines(lines).join("\n"), tools::MAX_TEXT))
                }
            }
            Call::SendToTerminal { block, text, enter } => {
                let id = match target(space, me, &block) {
                    Ok(id) => id,
                    Err(reply) => return reply,
                };
                let Some(pane) = space.terminal(id) else {
                    return Reply::err(format!("{block} isn't a terminal."));
                };
                pane.term.type_in(&text);
                if enter {
                    pane.term.write(&b"\r"[..]);
                }
                Reply::ok(format!("Typed into {block}."))
            }
            Call::Notify { text } => {
                let (_, title, _) = self.about(&space.blocks[&me], cx);
                let ws = cx.entity().downgrade();
                let notification = Notification::new()
                    .title(title)
                    .message(text)
                    .delivery(self.delivery(window))
                    .on_click(move |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.reveal(&space_id, me, window, cx)).ok();
                    });
                show_notification(window, cx, notification);
                Reply::ok("Shown.")
            }
        }
    }

    /// A block's kind, title and folder, as `whoami` and `list_blocks` say them.
    fn about(&self, body: &Body, cx: &App) -> (&'static str, String, Option<PathBuf>) {
        let title = || crate::block::title(self, body, cx).to_string();
        match body {
            Body::Terminal(pane) => (
                "terminal",
                self.agent_name(pane).map_or_else(|| pane.term.title(), str::to_string),
                pane.seen.folder.clone(),
            ),
            Body::Kanban(_) => ("kanban", title(), None),
            Body::Note(_) => ("note", title(), None),
            Body::Editor(e) => ("editor", title(), Some(e.folder.clone())),
            Body::Git(_) => ("git", title(), body.folder()),
        }
    }

    /// One line per block of `space`, back to front: id, kind, title, folder, and its tie to
    /// the caller `me`.
    fn block_lines(&self, space: &Space, me: BlockId, cx: &App) -> Vec<String> {
        space
            .canvas
            .blocks()
            .iter()
            .map(|b| {
                let (kind, title, folder) = self.about(&space.blocks[&b.id], cx);
                let uid = space.uids.get(&b.id).map_or("", String::as_str);
                let folder = folder.map(|f| format!("  {}", f.display())).unwrap_or_default();
                let tie = if b.id == me { "you" } else { "not connected" };
                format!("{uid}  {kind}  {title}{folder}  {tie}")
            })
            .collect()
    }

    /// What `initialize` tells the agent in `terminal` about itself.
    fn instructions(&self, terminal: &str, cx: &App) -> String {
        let Some((space_id, me)) = self.caller(terminal) else {
            return "This terminal isn't a block of this Templum: its tools won't work here.".into();
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let (_, title, _) = self.about(&space.blocks[&me], cx);
        let uid = space.uids.get(&me).map_or("", String::as_str);
        let others: Vec<String> = space
            .canvas
            .blocks()
            .iter()
            .filter(|b| b.id != me)
            .map(|b| {
                let (kind, title, _) = self.about(&space.blocks[&b.id], cx);
                format!("{} ({kind} '{title}')", space.uids.get(&b.id).map_or("", String::as_str))
            })
            .collect();
        format!(
            "You are block {uid}, '{title}', in Templum workspace '{}'. Other blocks there: {}. Name \
             blocks by these ids. You may read and type into your own block; the person connects \
             others to you by wiring them on the canvas. list_blocks gives the current list.",
            space.name,
            if others.is_empty() { "none".to_string() } else { others.join(", ") },
        )
    }
}

/// The block `uid` names in `space`, when the caller `me` may use it: until wires (batch 2),
/// only its own.
fn target(space: &Space, me: BlockId, uid: &str) -> Result<BlockId, Reply> {
    let Some(id) = space.block_by_uid(uid) else {
        return Err(Reply::err(format!(
            "There's no block {uid} in your workspace: list_blocks lists them."
        )));
    };
    if id != me {
        return Err(Reply::err(format!(
            "{uid} isn't connected to you. Ask the person to wire it to your block on the canvas."
        )));
    }
    Ok(id)
}
```

Note: `Value` is only needed if a helper uses it; drop the import if clippy says it is unused.

- [ ] **Step 7: Wire it into the workspace**

`workspace/mod.rs`: `mod bus;` with the other modules. `Workspace` gains, before `_wake`:

```rust
    /// The bus the programs in Templum's terminals talk to (docs/specs/connections.md); None
    /// without a data folder.
    pub(crate) bus: Option<templum_mcp::Bus>,
    /// What every new terminal's environment gets while the bus runs.
    bus_env: Vec<(String, String)>,
    _bus: Task<()>,
```

In `with`, before `let mut ws = Self {`: `let (bus, bus_env, bus_task) = bus::start(data_dir.as_deref(), window,
cx);` and in the literal `bus, bus_env, _bus: bus_task,`. In `spawn_block` (blocks.rs:344) replace `env:
Vec::new()` with `env: self.bus_env.clone(),`.

- [ ] **Step 8: Run the tests**

Run: `cargo test -p templum bus:: && cargo test -p templum config::`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/templum Cargo.lock
git commit -m "feat(connections): let agents and the templum command list blocks, read and type into their terminal, and notify" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 9: An agent's turns: its state and its dot

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs` (`Pane` :111: `state`, `answer`; `AgentState`)
- Modify: `crates/templum/src/workspace/blocks.rs:363` (the `Pane` literal: `state: None, answer: None`)
- Modify: `crates/templum/src/workspace/bus.rs` (`templum/event`, `agent_event`; `block_lines` adds the state)
- Modify: `crates/templum/src/block.rs:808-835` (`status_dot` becomes `pub(crate)` and reads the state)
- Test: `crates/templum/src/workspace/tests/bus.rs`

**Interfaces:**
- Consumes: `templum_mcp::AgentEvent` (Task 2), `Workspace::serve` (Task 8).
- Produces: `pub(crate) enum AgentState { Working, Idle, NeedsYou(Option<String>) }`; `Pane.state:
  Option<AgentState>`; `Pane.answer: Option<String>` (batch 3 reads it); `block::status_dot` `pub(crate)`.

- [ ] **Step 1: Write the failing test**

In `tests/bus.rs`:

```rust
/// Hands the workspace an agent's event, as `templum hook` does.
fn event(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext, terminal: &str, event: Value) {
    let (reply, _) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello { terminal: terminal.into(), client: "hook".into() },
        incoming: Incoming::Notification { method: "templum/event".into(), params: event },
        reply,
    };
    cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.serve(message, window, cx)))
        .unwrap();
}

#[gpui_kit::test]
fn an_agents_hooks_set_its_state_and_its_dot(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let (terminal, uid) = me(&ws, cx, id);
    // As `launch_agent` sets it.
    ws.update(cx, |ws, _| ws.space.terminal_mut(id).unwrap().seen.agent = Some("claude".into()));
    let dot = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            crate::block::status_dot(ws, ws.space.terminal(id).unwrap()).map(|(_, text)| text.to_string())
        })
    };
    event(handle, &ws, cx, &terminal, json!({ "event": "working" }));
    assert_eq!(dot(&ws, cx).as_deref(), Some("Working"));
    event(handle, &ws, cx, &terminal, json!({ "event": "needs_you", "message": "Allow Bash?" }));
    assert_eq!(dot(&ws, cx).as_deref(), Some("Waiting for you: Allow Bash?"));
    let (_, list) = call(handle, &ws, cx, &terminal, "list_blocks", json!({}));
    assert!(list.lines().any(|l| l.starts_with(&uid) && l.contains("waiting for its person")), "{list}");
    event(handle, &ws, cx, &terminal, json!({ "event": "stopped", "message": "Done." }));
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(id).unwrap();
        assert_eq!((pane.state.clone(), pane.answer.as_deref()), (Some(AgentState::Idle), Some("Done.")));
    });
    assert_eq!(dot(&ws, cx), None, "done with its turn: the terminal's usual dot");
    // An agent the snapshot no longer sees (it crashed): its state is not shown.
    event(handle, &ws, cx, &terminal, json!({ "event": "working" }));
    ws.update(cx, |ws, _| ws.space.terminal_mut(id).unwrap().seen.agent = None);
    assert_eq!(dot(&ws, cx), None);
    event(handle, &ws, cx, &terminal, json!({ "event": "ended" }));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.terminal(id).unwrap().state, None));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum an_agents_hooks_set_its_state`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

`workspace/mod.rs`, above `Pane`:

```rust
/// What an agent's hooks last said of its turn (docs/specs/connections.md, Hooks).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AgentState {
    Working,
    /// Done with its turn, or not started on one.
    Idle,
    /// Waiting for the person, with what it asks.
    NeedsYou(Option<String>),
}
```

`Pane` gains:

```rust
    /// Its agent's turn, as the agent's hooks last said; None until one speaks, and after its
    /// session ends.
    pub state: Option<AgentState>,
    /// Its agent's last answer, as its hooks gave it.
    pub answer: Option<String>,
```

`workspace/bus.rs`: in `serve`, before the catch-all notification arm:

```rust
            Incoming::Notification { method, params } if method == templum_mcp::hooks::METHOD => {
                if let Ok(event) = serde_json::from_value(params) {
                    self.agent_event(&hello.terminal, event, cx);
                }
            }
```

and in `impl Workspace`:

```rust
    /// An agent's hook says where its turn is.
    fn agent_event(&mut self, terminal: &str, event: templum_mcp::AgentEvent, cx: &mut Context<Self>) {
        use templum_mcp::AgentEvent;
        let pane = std::iter::once(&mut self.space)
            .chain(&mut self.others)
            .flat_map(|s| s.blocks.values_mut())
            .filter_map(Body::terminal_mut)
            .find(|pane| pane.term.id() == terminal);
        let Some(pane) = pane else { return };
        match event {
            AgentEvent::Started => {
                pane.state = Some(AgentState::Idle);
                pane.answer = None;
            }
            AgentEvent::Working => pane.state = Some(AgentState::Working),
            AgentEvent::Stopped { message } => {
                pane.state = Some(AgentState::Idle);
                pane.answer = message;
            }
            AgentEvent::NeedsYou { message } => pane.state = Some(AgentState::NeedsYou(message)),
            AgentEvent::Ended => pane.state = None,
        }
        cx.notify();
    }
```

(`use super::AgentState;` at the top.) In `block_lines`, for a terminal whose `pane.seen.agent` is known, append its
state to the tie: `", working"`, `", idle"` or `", waiting for its person"`:

```rust
                let state = space
                    .terminal(b.id)
                    .filter(|pane| pane.seen.agent.is_some())
                    .and_then(|pane| pane.state.as_ref())
                    .map(|state| match state {
                        AgentState::Working => ", working",
                        AgentState::Idle => ", idle",
                        AgentState::NeedsYou(_) => ", waiting for its person",
                    })
                    .unwrap_or_default();
                format!("{uid}  {kind}  {title}{folder}  {tie}{state}")
```

`block.rs`: `pub(crate) fn status_dot`; its doc gains "an agent's turn (its hooks) before the shell's"; the `None`
arm becomes:

```rust
        None => match pane.state.as_ref().filter(|_| pane.seen.agent.is_some()) {
            Some(AgentState::NeedsYou(asks)) => (
                ansi(3),
                asks.as_ref().map_or_else(|| "Waiting for you".to_string(), |a| format!("Waiting for you: {a}")),
            ),
            Some(AgentState::Working) => (ws.ui.accent, "Working".to_string()),
            Some(AgentState::Idle) => return None,
            None => {
                let status = pane.term.status();
                match (status.running, status.last) {
                    // … as it is
                }
            }
        },
```

with `use crate::workspace::AgentState;` (make it `pub(crate) use` from `workspace` if the path is private).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum bus::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum
git commit -m "feat(connections): show in its dot whether an agent is working or waiting for you" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 10: What Templum types into Claude Code and Codex

**Files:**
- Modify: `crates/templum/src/workspace/usage.rs:31-49` (`typed`)
- Test: `crates/templum/src/workspace/tests/usage.rs:6-30`

**Interfaces:**
- Consumes: `settings_arg(…, status_line, hooks)`, `mcp_arg` (Task 6); `templum_mcp::codex_flags` (Task 1);
  `program()`, `Workspace.bus`, `config.connections` (Task 8).

- [ ] **Step 1: Write the failing tests**

Replace `the_claude_code_templum_opens_gets_its_status_line` with:

```rust
/// The Claude Code and Codex Templum types get its server and hooks; Claude Code its status line
/// too (docs/specs/connections.md, What Templum types into each agent).
#[gpui_kit::test]
fn the_agents_templum_opens_get_its_server_hooks_and_status_line(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.update(cx, |ws, _| {
        let agent = |id: &str| ws.agents.iter().find(|a| a.id == id).cloned();
        let (claude, codex) = (agent("claude"), agent("codex"));
        let typed = |name: &str| {
            templum_usage::claude::typed_path(&data.path().join(name).to_string_lossy(), cfg!(windows)).unwrap()
        };
        let (settings, mcp) = (typed("claude-statusline.json"), typed("mcp.json"));
        assert_eq!(
            ws.typed(claude.as_ref(), "claude --continue"),
            format!("claude --continue --settings {settings} --mcp-config {mcp}")
        );
        let written = std::fs::read_to_string(data.path().join("claude-statusline.json")).unwrap();
        assert!(written.contains("--claude-statusline") && written.contains(" hook claude"), "{written}");
        let server: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(data.path().join("mcp.json")).unwrap()).unwrap();
        assert_eq!(server["mcpServers"]["templum"]["args"], serde_json::json!(["mcp"]));
        #[cfg(unix)]
        assert!(
            ws.typed(codex.as_ref(), "codex resume 0d5e").starts_with(
                "codex resume 0d5e -c 'mcp_servers.templum.command="
            ),
            "{}",
            ws.typed(codex.as_ref(), "codex resume 0d5e")
        );
        assert_eq!(ws.typed(None, "claude"), "claude");
        // Connections off: the status line only, and nothing for Codex.
        ws.config.connections.enabled = false;
        assert_eq!(ws.typed(claude.as_ref(), "claude"), format!("claude --settings {settings}"));
        assert_eq!(ws.typed(codex.as_ref(), "codex"), "codex");
        ws.config.usage.claude_limits = false;
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
        // Connections without the status line: the hooks still need the settings file.
        ws.config.connections.enabled = true;
        assert_eq!(
            ws.typed(claude.as_ref(), "claude"),
            format!("claude --settings {settings} --mcp-config {mcp}")
        );
        // A bus that couldn't start: no server, no hooks.
        let bus = ws.bus.take();
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
        ws.bus = bus;
        // Without a data folder, nothing would read what the relay writes.
        ws.data_dir = None;
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
    });
}

/// A data folder a shell would expand (`$`): the agent starts as typed, without a broken line.
#[cfg(unix)]
#[gpui_kit::test]
fn an_untypeable_data_folder_gives_no_connections(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    let data = parent.path().join("a$b");
    std::fs::create_dir(&data).unwrap();
    let (_, ws) = open_in(cx, None, Some(data));
    ws.update(cx, |ws, _| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").cloned();
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
    });
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum usage::the_agents_templum_opens`
Expected: FAIL (no `--mcp-config`, no Codex flags).

- [ ] **Step 3: Implement**

```rust
    /// `command` as typed to start `agent` in a terminal (docs/specs/connections.md, What Templum
    /// types into each agent): the Claude Code Templum opens gets its settings file (the status
    /// line of the plan's limits, and the hooks) and its MCP server; Codex gets the server and
    /// its `notify`, unless the shell is cmd (they go in single quotes). Nothing under WSL, where
    /// Templum's paths aren't the shell's, nor without a data folder.
    pub(super) fn typed(&self, agent: Option<&AgentEntry>, command: &str) -> String {
        let builtin = agent.and_then(|a| a.builtin);
        let (Some(data), Some(exe), false) = (&self.data_dir, program(), self.shell_is("wsl")) else {
            return command.to_string();
        };
        let connected = self.config.connections.enabled && self.bus.is_some();
        let windows = cfg!(windows);
        let mut line = command.to_string();
        if builtin == Some(Agent::CLAUDE) {
            let limits = self.config.usage.claude_limits;
            if let Some(arg) = templum_usage::claude::settings_arg(data, &exe, windows, limits, connected) {
                line.push_str(&format!(" --settings {arg}"));
            }
            // Last: it takes every word after it.
            if connected && let Some(arg) = templum_usage::claude::mcp_arg(data, &exe, windows) {
                line.push_str(&format!(" --mcp-config {arg}"));
            }
        } else if builtin == Some(Agent::CODEX)
            && connected
            && !self.shell_is("cmd")
            && let Some(flags) = templum_mcp::codex_flags(&exe.to_string_lossy(), windows)
        {
            line.push_str(&flags);
        }
        line
    }

    /// New terminals run the shell named `name` (its program's file name): the configured one,
    /// else the system's default.
    fn shell_is(&self, name: &str) -> bool {
        let program = self
            .config
            .shell
            .program
            .clone()
            .or_else(|| templum_terminal::default_shell().map(|s| s.program));
        program.is_some_and(|p| std::path::Path::new(&p).file_stem().is_some_and(|s| s == name))
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum usage:: && cargo test -p templum agents`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum
git commit -m "feat(agents): give the Claude Code and Codex Templum opens its MCP server and hooks" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 11: Docs, the full check and the benches

**Files:**
- Modify: `docs/specs/connections.md`, `docs/ARCHITECTURE.md`, `docs/specs/persistence.md`, `docs/specs/usage.md`,
  `docs/specs/agents.md`, `docs/minors.md`, `README.md`

- [ ] **Step 1: The spec says what was built**

In `docs/specs/connections.md`:
- **Status:** "Batch 1 implemented; batches 2 to 5 planned".
- **Words ▸ Block id:** "five lowercase letters and digits (`b7k2m`)".
- **An agent's state:** the "Done with its turn" row: dot "none", tooltip "—" (an agent's running command would
  otherwise read "Running"); add "An agent the last snapshot didn't see (it crashed) shows no state."
- **Identity:** replace the PATH sentence with: "On Unix, every terminal's `PATH` starts with `<data>/bin`, where
  Templum keeps a link named `templum` to itself, so `templum` in a Templum terminal is the Templum that opened it."
- **Hooks table, Claude Code row:** hooks `SessionStart`, `UserPromptSubmit`, `PostToolUse` (working again after a
  permission), `Stop`, `Notification`, `SessionEnd`; waiting for the person is `permission_prompt`,
  `elicitation_dialog`, `agent_needs_input` (not `idle_prompt`: that is the reminder after a finished turn).
- **What Templum types ▸ Codex:** the five flags, `env_vars` included (Codex hands a server only the variables it is
  told to).
- **What Templum types**, the untypeable path: "the agent starts without connections (stderr says why)".

- [ ] **Step 2: Architecture and the other specs**

- `docs/ARCHITECTURE.md`: a crate row for `templum-mcp` ("The bus Templum's blocks talk over: JSON-RPC lines on a
  local socket (`bus.rs`), the MCP tools (`tools.rs`), the agents' hook events (`hooks.rs`) and the `templum` command
  line (`cli.rs`: the MCP bridge, `hook`, the verbs) ([specs/connections.md](specs/connections.md))"); "All six
  crates" → "All seven"; the `templum-usage` row: its settings file also holds Templum's hooks, and it writes
  `mcp.json`; Runtime: a bullet on the bus (an accept thread, a reader and a writer per connection; messages drained
  on the UI thread by `workspace/bus.rs`; every terminal gets `TEMPLUM_BUS` and, on Unix, `<data>/bin` first in
  PATH); Source layout: `workspace/bus.rs`; Configuration: `[connections]`; the persistence bullet: each item's
  `uid`.
- `docs/specs/persistence.md`, The file: every item may carry `"uid"` (five letters and digits); a missing, taken or
  malformed one is replaced; an older Templum ignores it.
- `docs/specs/usage.md`, the Claude Code bullet on `--settings`: the same file also holds Templum's hooks when
  `[connections]` is on, and `claude_limits = false` drops only the status line.
- `docs/specs/agents.md`, Out of scope: "passing them settings (except the status line, the hooks and the MCP server
  of [connections.md](connections.md))".
- `README.md`: a short section "Agents that talk to the canvas" (Claude Code and Codex opened from the menu get
  Templum's MCP server: they can list the workspace's blocks, read their terminal and notify you; `templum blocks`,
  `templum read <block>` in any Templum terminal; `[connections] enabled = false` turns it off).

- [ ] **Step 3: Minors**

Add a `## Connections` section to `docs/minors.md`, each entry with where, what a person sees, a fix:
- `workspace/usage.rs` `typed`: an untypeable data folder or program path only says so on stderr; a notification
  once per session would tell the person why the agent has no connections.
- `workspace/bus.rs` `templum_first`: Windows terminals get no `templum` on PATH; a `templum.exe` hard link or the
  installer's PATH would.
- `workspace/bus.rs` `templum_first`: two Templums sharing a data folder (an installed one and `cargo run`) point
  `<data>/bin/templum` at the last one started.
- `templum-usage/src/claude.rs`: `claude-statusline.json` holds the hooks too, under its old name; rename it to
  `claude-settings.json` (and prune the old one).
- `workspace/blocks.rs` `new_uid`: a stopped workspace's saved ids aren't checked for a clash (1 in 60 million).
- Codex reports only its turns' end: its dot never says "Working", and nothing says it waits for a permission.
- `templum` verbs on Windows print only to a pipe (the release program has no console): `templum blocks | more`.

- [ ] **Step 4: The full check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all green. Then `cargo deny check`.

- [ ] **Step 5: The benches** (this batch touches `templum-terminal`)

Run: `cargo run --release -p templum -- --bench` and `cargo run --release -p templum -- --bench-mixed`
Expected: each prints a JSON line with `p95_ms` ≤ 16.7; keep both lines for the pull request.

- [ ] **Step 6: Commit**

```bash
git add docs README.md
git commit -m "docs(connections): describe the bus, block ids and what agents are given" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

## Manual checks (Hyprland)

1. **Claude Code from AI agent ▸:** `ps` shows `--settings … --mcp-config …`; in it, `/mcp` lists `templum`
   connected. Ask "which block are you and what's around you?" (whoami, list_blocks), then "read the last 30 lines
   of your terminal".
2. **The dot:** while Claude Code answers, the accent dot "Working"; at a permission prompt, yellow "Waiting for
   you: …"; after the answer, the usual dot.
3. **Codex from AI agent ▸:** `/mcp` lists `templum`; `whoami` works (checks `env_vars`). After a turn, `templum
   blocks` in another terminal shows it "idle" (checks that `notify` inherits Codex's environment).
4. **Templum gone:** in a Templum terminal, run `templum mcp` and type
   `{"jsonrpc":"2.0","id":1,"method":"ping"}` (the answer comes back); quit Templum: the command ends at once
   instead of waiting for more input.
5. **A plain terminal:** `templum whoami`, `templum blocks`, `templum read <id> -n 20`, `templum notify hi` (the
   notification's click shows the block); `templum mcp` in Alacritty (outside Templum) exits 2 with its message.
6. **Restart:** the blocks keep their ids (`templum blocks` before and after).
7. **Your own hooks and status line** in `~/.claude/settings.json` still run in the Claude Code Templum opens.
