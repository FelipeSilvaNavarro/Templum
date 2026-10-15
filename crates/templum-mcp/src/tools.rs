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
/// The longest a tool waits, seconds: Codex gives up on a call after an hour (the
/// `tool_timeout_sec` of `codex_flags`).
pub const MAX_WAIT: u64 = 3600;
/// How long a question or a request to connect waits for the person, seconds, unless asked.
pub const ASK_WAIT: u64 = 1800;

/// A tool call, its arguments checked.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Whoami,
    ListBlocks,
    ReadTerminal {
        block: String,
        lines: usize,
        last_command: bool,
    },
    SendToTerminal {
        block: String,
        text: String,
        enter: bool,
    },
    Notify {
        text: String,
    },
    AskUser {
        question: String,
        options: Vec<String>,
        timeout: u64,
    },
}

/// What a tool answers: its text, and whether it is a refusal or a failure.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub text: String,
    pub is_error: bool,
}

impl Reply {
    pub fn ok(text: impl Into<String>) -> Reply {
        Reply {
            text: text.into(),
            is_error: false,
        }
    }

    pub fn err(text: impl Into<String>) -> Reply {
        Reply {
            text: text.into(),
            is_error: true,
        }
    }
}

/// `initialize`'s result: the client's version when Templum speaks it, else its newest.
pub fn initialize_result(params: &Value, instructions: &str) -> Value {
    let asked = params["protocolVersion"].as_str();
    let version = VERSIONS
        .iter()
        .find(|v| Some(**v) == asked)
        .unwrap_or(&VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "templum", "version": env!("CARGO_PKG_VERSION") },
        "instructions": instructions,
    })
}

/// `tools/list`'s result.
pub fn list() -> Value {
    let block =
        json!({ "type": "string", "description": "A block's id, as list_blocks shows it." });
    let none = json!({ "type": "object", "properties": {} });
    let seconds = |default: u64| {
        json!({
            "type": "integer",
            "minimum": 1,
            "maximum": MAX_WAIT,
            "default": default,
            "description": "Seconds to wait before giving up.",
        })
    };
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
        value => value
            .as_bool()
            .ok_or_else(|| format!("\"{key}\" is true or false")),
    };
    let seconds = |default: u64| match &args["timeout"] {
        Value::Null => Ok(default),
        value => value
            .as_u64()
            .filter(|n| *n >= 1)
            .map(|n| n.min(MAX_WAIT))
            .ok_or_else(|| format!("\"timeout\" is a number of seconds from 1 to {MAX_WAIT}")),
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
        "notify" => Call::Notify {
            text: text("text")?,
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_listed_parses_its_example() {
        let examples = [
            ("whoami", json!({})),
            ("list_blocks", json!({})),
            (
                "read_terminal",
                json!({ "block": "b7k2m", "lines": 10, "last_command": true }),
            ),
            (
                "send_to_terminal",
                json!({ "block": "b7k2m", "text": "ls", "enter": true }),
            ),
            ("notify", json!({ "text": "done" })),
            (
                "ask_user",
                json!({ "question": "Ship it?", "options": ["Yes", "No"], "timeout": 60 }),
            ),
        ];
        let listed: Vec<String> = list()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            listed,
            examples
                .iter()
                .map(|(n, _)| n.to_string())
                .collect::<Vec<_>>()
        );
        for (name, args) in &examples {
            assert!(known(name));
            assert!(parse(name, args).is_ok(), "{name}");
        }
        assert_eq!(
            parse("read_terminal", &json!({ "block": "b" })),
            Ok(Call::ReadTerminal {
                block: "b".into(),
                lines: 200,
                last_command: false
            })
        );
        assert_eq!(
            parse("read_terminal", &json!({ "block": "b", "lines": 99_999 })),
            Ok(Call::ReadTerminal {
                block: "b".into(),
                lines: MAX_LINES,
                last_command: false
            })
        );
        assert_eq!(
            parse("send_to_terminal", &json!({ "block": "b", "text": "x" })),
            Ok(Call::SendToTerminal {
                block: "b".into(),
                text: "x".into(),
                enter: false
            })
        );
    }

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
            parse(
                "ask_user",
                &json!({ "question": "Why?", "timeout": 99_999 })
            ),
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

    #[test]
    fn bad_arguments_say_which() {
        let err = parse("read_terminal", &json!({})).unwrap_err();
        assert!(err.contains("\"block\""), "{err}");
        assert!(parse("read_terminal", &json!({ "block": "b", "lines": 0 })).is_err());
        assert!(
            parse(
                "send_to_terminal",
                &json!({ "block": "b", "text": "x", "enter": "yes" })
            )
            .is_err()
        );
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
        let refused: Value =
            serde_json::from_str(&tool_result(&json!(3), &Reply::err("no"))).unwrap();
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
