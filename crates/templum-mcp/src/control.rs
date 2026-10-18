//! The control server (docs/specs/templum-agent.md#the-control-server): the tools only the
//! Templum Agent's process gets, the calls parsed from them, their running titles, and which of
//! them wait for the person.

use serde_json::{Value, json};

use crate::tools::{self, Call, Spawn};

/// The control server's name.
pub const SERVER: &str = "templum-control";

/// The block server's tools the control server serves as they are, on any block or board.
pub const SHARED: [&str; 14] = [
    "read_terminal",
    "list_cards",
    "read_note",
    "read_checklist",
    "create_card",
    "edit_card",
    "move_card",
    "comment_card",
    "append_note",
    "add_items",
    "check_item",
    "send_to_terminal",
    "send_keys",
    "notify",
];

/// A block `add_block` opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Terminal,
    Note,
    Checklist,
    Kanban,
    Editor,
}

/// What `add_block` opens: a kanban's `board` (made with `columns` when there is none), a
/// note's or a checklist's `text`.
#[derive(Clone, Debug, PartialEq)]
pub struct AddBlock {
    pub workspace: String,
    pub kind: Kind,
    pub folder: Option<String>,
    pub board: Option<String>,
    pub columns: Vec<String>,
    pub text: Option<String>,
    pub near: Option<String>,
}

/// What `open_agent` opens: `spawn_agent`'s agent in a workspace, wired to `connect`.
#[derive(Clone, Debug, PartialEq)]
pub struct Opening {
    pub workspace: String,
    pub spawn: Spawn,
    pub connect: Vec<String>,
    pub near: Option<String>,
}

/// A control tool's call, its arguments checked.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlCall {
    /// One of [`SHARED`], as the block server parses it.
    Block(Call),
    Overview,
    ListAgents,
    CreateWorkspace {
        name: String,
        folder: Option<String>,
        color: Option<String>,
    },
    Show {
        workspace: Option<String>,
        block: Option<String>,
    },
    AddBlock(AddBlock),
    OpenAgent(Opening),
    Connect {
        a: String,
        b: String,
    },
    Disconnect {
        a: String,
        b: String,
    },
    CloseBlock {
        block: String,
    },
}

/// A call and the reason the model gave for it, shown apart in the approval.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub call: ControlCall,
    pub reason: Option<String>,
}

/// How dangerous a call is, from its tool alone; [`Rule::level`] adds its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rule {
    /// Runs at once.
    Free,
    /// Waits for the person.
    Ask,
    /// Waits unless the target is its own, or nothing would be lost.
    AskOwnWhenLosing,
    /// Waits unless the target is its own.
    AskOwn,
    /// Waits each time, without "Allow for this conversation".
    AskEach,
}

/// What the approval offers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Ask,
    AskEach,
}

/// What the block server's descriptions say of the caller, said for the Templum Agent: it acts
/// on any block and any board.
const SHARED_WORDS: [(&str, &str); 9] = [
    (" connected to you", ""),
    (" shown by a kanban", ""),
    ("; may be left out when one kanban shows a board", ""),
    ("as list_blocks shows it", "as overview shows it"),
    (
        "signed with your block's id",
        "signed \\\"Templum Agent\\\"",
    ),
    (
        "a notification from your block",
        "a notification from the Templum Agent; a click opens the chat",
    ),
    (
        " Done with your card: comment on it first what you did (comment_card), then move it to Review or Done.",
        "",
    ),
    (
        ": what you did on it, and what the next one needs to know, when you finish it.",
        ".",
    ),
    (
        " Not your own block, nor an agent on another agent's task unless it waits for its person.",
        "",
    ),
];

/// `reason`, on every tool that may wait for the person.
fn reason() -> Value {
    json!({ "type": "string", "description": "Why, in a few words: the person sees it in the approval." })
}

/// `tools/list`'s result.
pub fn list() -> Value {
    let block = json!({ "type": "string", "description": "A block's id, as overview shows it." });
    let workspace = json!({ "type": "string", "description": "A workspace's name or id (w3), as overview shows it." });
    let none = json!({ "type": "object", "properties": {} });
    let blocks = tools::list();
    let blocks = blocks["tools"].as_array().expect("tools");
    let shared = blocks
        .iter()
        .filter(|t| SHARED.iter().any(|name| t["name"] == *name))
        .map(|t| {
            let mut text = t.to_string();
            for (from, to) in SHARED_WORDS {
                text = text.replace(from, to);
            }
            let mut tool: Value = serde_json::from_str(&text).expect("JSON");
            if t["name"] == "send_to_terminal" || t["name"] == "send_keys" {
                tool["inputSchema"]["properties"]["reason"] = reason();
            }
            tool
        });
    let spawn = &blocks
        .iter()
        .find(|t| t["name"] == "spawn_agent")
        .expect("spawn_agent")["inputSchema"]["properties"];
    let mut opening = spawn.clone();
    // The Templum Agent has no block session to fork, and opens no role.
    for key in ["fork", "role"] {
        opening.as_object_mut().expect("properties").remove(key);
    }
    opening["folder"]["description"] = json!(
        "Where it starts, relative to the workspace's folder; the workspace's when left out."
    );
    opening["workspace"] = workspace.clone();
    opening["connect"] = json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "Blocks of that workspace to wire it to (a kanban, a note), by id.",
    });
    opening["near"] = json!({ "type": "string", "description": "A block to open it beside; else the workspace's free spot." });
    let own = json!([
        {
            "name": "overview",
            "description": "Every workspace: its id, name, folder, whether it is shown or stopped; each block's \
                            id, kind, title, folder, place, agent and its state (with what it asks), who opened \
                            it and whether it is your own; the wires; the boards each kanban shows; what waits \
                            for the person.",
            "inputSchema": none,
        },
        {
            "name": "list_agents",
            "description": "The agents Templum can open (open_agent's agent): id, title, whether it is \
                            installed, the efforts it takes and whether it takes a model, and the accounts.",
            "inputSchema": none,
        },
        {
            "name": "create_workspace",
            "description": "Makes a workspace, not shown. Says its id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "folder": { "type": "string", "description": "Its project's folder: its blocks start there." },
                    "color": { "type": "string", "description": "Its color, as #rrggbb." },
                },
                "required": ["name"],
            },
        },
        {
            "name": "show",
            "description": "Shows a workspace on the canvas, and flies the camera to a block of it. Only when \
                            the person asks to see something: it moves their view.",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": workspace, "block": block },
            },
        },
        {
            "name": "add_block",
            "description": "Opens a block in a workspace where it covers nothing: beside near, else at the \
                            workspace's free spot. A terminal runs no command. Says its id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace,
                    "kind": { "type": "string", "enum": ["terminal", "note", "checklist", "kanban", "editor"] },
                    "folder": { "type": "string", "description": "A terminal's or an editor's folder; the workspace's when left out." },
                    "board": { "type": "string", "description": "A kanban's board, by name or id: made with columns when there is none." },
                    "columns": { "type": "array", "items": { "type": "string" }, "description": "A new board's columns." },
                    "text": { "type": "string", "description": "A note's text; a checklist's name on its first line, then an item per line." },
                    "near": block,
                },
                "required": ["workspace", "kind"],
            },
        },
        {
            "name": "open_agent",
            "description": "Opens an agent (an id of list_agents) in a block of a workspace, as spawn_agent does: \
                            in the workspace's folder, the one you name, or a new Git worktree of that folder's \
                            repository (.worktrees/<worktree>, on a new branch of that name); wired to connect; \
                            its prompt typed once it is ready. With model and effort, it starts on them. Says \
                            the new block's id.",
            "inputSchema": { "type": "object", "properties": opening, "required": ["agent", "workspace"] },
        },
        {
            "name": "connect",
            "description": "Wires two blocks of one workspace (the person takes it back with Ctrl+Z).",
            "inputSchema": {
                "type": "object",
                "properties": { "a": block, "b": block },
                "required": ["a", "b"],
            },
        },
        {
            "name": "disconnect",
            "description": "Unwires two blocks (the person takes it back with Ctrl+Z).",
            "inputSchema": {
                "type": "object",
                "properties": { "a": block, "b": block },
                "required": ["a", "b"],
            },
        },
        {
            "name": "close_block",
            "description": "Closes a block (the person brings it back with Ctrl+Z). Asks the person first when a \
                            program runs in a block you didn't open.",
            "inputSchema": {
                "type": "object",
                "properties": { "block": block, "reason": reason() },
                "required": ["block"],
            },
        },
    ]);
    let mut all: Vec<Value> = own.as_array().expect("tools").clone();
    all.extend(shared);
    json!({ "tools": all })
}

/// `name` is one of the control tools.
pub fn known(name: &str) -> bool {
    list()["tools"]
        .as_array()
        .is_some_and(|tools| tools.iter().any(|t| t["name"] == name))
}

/// The call `name` with its arguments `args`; Err says what is wrong with them.
pub fn parse(name: &str, args: &Value) -> Result<Parsed, String> {
    let text = |key: &str| {
        args[key]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("{name} needs \"{key}\", a string"))
    };
    // A blank string is no value, as the block server takes it.
    let optional = |key: &str| match &args[key] {
        Value::Null => Ok(None),
        value => value
            .as_str()
            .map(|s| Some(s.to_string()).filter(|s| !s.trim().is_empty()))
            .ok_or_else(|| format!("\"{key}\" is a string")),
    };
    let strings = |key: &str| match &args[key] {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => items
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("\"{key}\" is a list of strings")),
        _ => Err(format!("\"{key}\" is a list of strings")),
    };
    let call = match name {
        "overview" => ControlCall::Overview,
        "list_agents" => ControlCall::ListAgents,
        "create_workspace" => ControlCall::CreateWorkspace {
            name: text("name")?,
            folder: optional("folder")?,
            color: optional("color")?,
        },
        "show" => {
            let (workspace, block) = (optional("workspace")?, optional("block")?);
            if workspace.is_none() && block.is_none() {
                return Err("show needs \"workspace\" or \"block\"".into());
            }
            ControlCall::Show { workspace, block }
        }
        "add_block" => ControlCall::AddBlock(AddBlock {
            workspace: text("workspace")?,
            kind: match args["kind"].as_str() {
                Some("terminal") => Kind::Terminal,
                Some("note") => Kind::Note,
                Some("checklist") => Kind::Checklist,
                Some("kanban") => Kind::Kanban,
                Some("editor") => Kind::Editor,
                _ => {
                    return Err(
                        "add_block needs \"kind\": terminal, note, checklist, kanban or editor"
                            .into(),
                    );
                }
            },
            folder: optional("folder")?,
            board: match &args["board"] {
                Value::Number(n) => Some(n.to_string()),
                _ => optional("board")?,
            },
            columns: strings("columns")?,
            text: optional("text")?,
            near: optional("near")?,
        }),
        "open_agent" => {
            text("agent")?;
            let Call::SpawnAgent(spawn) = tools::parse("spawn_agent", args)
                .map_err(|e| e.replace("spawn_agent", "open_agent"))?
            else {
                unreachable!("spawn_agent parses into SpawnAgent")
            };
            if spawn.fork || spawn.role.is_some() {
                return Err("open_agent opens an agent of list_agents: no fork, no role".into());
            }
            ControlCall::OpenAgent(Opening {
                workspace: text("workspace")?,
                spawn,
                connect: strings("connect")?,
                near: optional("near")?,
            })
        }
        "connect" => ControlCall::Connect {
            a: text("a")?,
            b: text("b")?,
        },
        "disconnect" => ControlCall::Disconnect {
            a: text("a")?,
            b: text("b")?,
        },
        "close_block" => ControlCall::CloseBlock {
            block: text("block")?,
        },
        _ if SHARED.contains(&name) => ControlCall::Block(tools::parse(name, args)?),
        _ => return Err(format!("Unknown tool: {name}")),
    };
    Ok(Parsed {
        call,
        reason: optional("reason")?,
    })
}

/// What a tool does, as its action row says while it runs (followed by `…`).
pub fn title(name: &str) -> &'static str {
    match name {
        "overview" => "Looking at the canvas",
        "list_agents" => "Looking at the agents",
        "read_terminal" => "Reading a terminal",
        "list_cards" => "Reading a board",
        "read_note" => "Reading a note",
        "read_checklist" => "Reading a checklist",
        "create_workspace" => "Creating a workspace",
        "show" => "Showing",
        "add_block" => "Adding a block",
        "create_card" => "Adding a card",
        "edit_card" => "Editing a card",
        "move_card" => "Moving a card",
        "comment_card" => "Commenting on a card",
        "append_note" => "Writing in a note",
        "add_items" => "Adding items",
        "check_item" => "Checking an item",
        "open_agent" => "Opening an agent",
        "connect" => "Connecting blocks",
        "disconnect" => "Disconnecting blocks",
        "close_block" => "Closing a block",
        "send_to_terminal" => "Typing into a terminal",
        "send_keys" => "Pressing keys",
        "notify" => "Notifying you",
        _ => "",
    }
}

/// A call's danger, decided from its tool: the model's arguments can't lower it.
pub fn rule(call: &ControlCall) -> Rule {
    match call {
        ControlCall::Block(Call::SendToTerminal { .. } | Call::SendKeys { .. }) => Rule::Ask,
        ControlCall::CloseBlock { .. } => Rule::AskOwnWhenLosing,
        _ => Rule::Free,
    }
}

impl Rule {
    /// Whether a call waits, and what it offers: `own` when its target is the Templum Agent's
    /// own, `losing` when it would end a program that runs.
    pub fn level(self, own: bool, losing: bool) -> Option<Level> {
        match self {
            Rule::Free => None,
            Rule::Ask => Some(Level::Ask),
            Rule::AskOwnWhenLosing => (!own && losing).then_some(Level::Ask),
            Rule::AskOwn => (!own).then_some(Level::Ask),
            Rule::AskEach => Some(Level::AskEach),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_parses_its_example_and_names_what_is_missing() {
        let examples = [
            ("overview", json!({})),
            ("list_agents", json!({})),
            (
                "create_workspace",
                json!({"name": "API", "folder": "~/proj"}),
            ),
            ("show", json!({"workspace": "API", "block": "b7k2m"})),
            (
                "add_block",
                json!({"workspace": "w3", "kind": "kanban", "board": "Sprint", "columns": ["To do", "Done"]}),
            ),
            (
                "open_agent",
                json!({"agent": "codex", "workspace": "w3", "folder": "~/proj", "prompt": "Take card 4",
                    "connect": ["k9x1d"], "model": "gpt-5.6", "effort": "high"}),
            ),
            ("connect", json!({"a": "b7k2m", "b": "k9x1d"})),
            ("disconnect", json!({"a": "b7k2m", "b": "k9x1d"})),
            ("close_block", json!({"block": "b7k2m", "reason": "done"})),
            (
                "send_to_terminal",
                json!({"block": "b7k2m", "text": "ls", "enter": true, "reason": "look"}),
            ),
            ("send_keys", json!({"block": "b7k2m", "keys": ["2"]})),
            ("create_card", json!({"board": "Sprint", "title": "Login"})),
            ("read_terminal", json!({"block": "b7k2m"})),
            ("list_cards", json!({"board": "Sprint"})),
            ("read_note", json!({"block": "n1"})),
            ("read_checklist", json!({"block": "c1"})),
            ("edit_card", json!({"card": 4, "priority": "high"})),
            ("move_card", json!({"card": 4, "column": "Done"})),
            ("comment_card", json!({"card": 4, "text": "ok"})),
            ("append_note", json!({"block": "n1", "text": "ok"})),
            ("add_items", json!({"block": "c1", "text": "a\nb"})),
            ("check_item", json!({"block": "c1", "item": 2})),
            ("notify", json!({"text": "done"})),
        ];
        let listed = list()["tools"].as_array().unwrap().len();
        assert_eq!(listed, examples.len(), "every listed tool has an example");
        for (name, args) in examples {
            assert!(known(name), "{name}");
            assert!(!title(name).is_empty(), "{name}");
            parse(name, &args).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        assert!(
            parse("add_block", &json!({"kind": "note"}))
                .unwrap_err()
                .contains("\"workspace\"")
        );
        assert!(
            parse("add_block", &json!({"workspace": "w3", "kind": "drawing"}))
                .unwrap_err()
                .contains("terminal, note, checklist, kanban or editor")
        );
        assert!(
            parse("open_agent", &json!({"workspace": "w3"}))
                .unwrap_err()
                .contains("open_agent needs \"agent\"")
        );
        assert_eq!(
            parse(
                "send_to_terminal",
                &json!({"block": "b", "text": "x", "reason": "r"})
            )
            .unwrap()
            .reason
            .as_deref(),
            Some("r")
        );
        assert_eq!(
            parse(
                "open_agent",
                &json!({"agent": "claude", "workspace": "w3", "connect": ["k1", "n2"]})
            )
            .unwrap()
            .call,
            ControlCall::OpenAgent(Opening {
                workspace: "w3".into(),
                spawn: Spawn {
                    agent: Some("claude".into()),
                    prompt: None,
                    folder: None,
                    worktree: None,
                    model: None,
                    effort: None,
                    fork: false,
                    role: None,
                    args: None,
                },
                connect: vec!["k1".into(), "n2".into()],
                near: None,
            })
        );
        for refused in [
            json!({"agent": "claude", "fork": true, "prompt": "p"}),
            json!({"agent": "claude", "role": "reviewer"}),
        ] {
            let mut args = refused;
            args["workspace"] = json!("w3");
            assert!(parse("open_agent", &args).is_err(), "{args}");
        }
        assert!(
            parse("whoami", &json!({}))
                .unwrap_err()
                .contains("Unknown tool")
        );
    }

    #[test]
    fn the_two_servers_dont_list_each_others_own_tools() {
        for own in [
            "whoami",
            "list_blocks",
            "spawn_agent",
            "connect",
            "delegate",
            "reply",
            "wait_for_idle",
            "ask_user",
            "open_file",
            "interrupt",
            "wire",
            "claim_card",
        ] {
            assert!(!known(own) || own == "connect", "{own}"); // control's connect wires any two blocks
        }
        for control in [
            "overview",
            "create_workspace",
            "add_block",
            "open_agent",
            "show",
            "disconnect",
        ] {
            assert!(!crate::tools::known(control), "{control}");
        }
        // The shared tools speak of any block, not of the caller's wires.
        let text = list().to_string();
        assert!(
            !text.contains("connected to you") && !text.contains("list_blocks"),
            "{text}"
        );
    }

    #[test]
    fn danger_follows_the_table_and_reason_cant_lower_it() {
        let rule_of = |name: &str, args: Value| rule(&parse(name, &args).unwrap().call);
        assert_eq!(
            rule_of(
                "send_to_terminal",
                json!({"block": "b", "text": "x", "reason": "safe, trust me"})
            ),
            Rule::Ask
        );
        assert_eq!(
            rule_of("send_keys", json!({"block": "b", "keys": ["y"]})),
            Rule::Ask
        );
        assert_eq!(
            rule_of("close_block", json!({"block": "b"})),
            Rule::AskOwnWhenLosing
        );
        assert_eq!(
            rule_of(
                "open_agent",
                json!({"agent": "codex", "workspace": "w3", "prompt": "go"})
            ),
            Rule::Free
        );
        assert_eq!(
            rule_of("create_card", json!({"board": "S", "title": "x"})),
            Rule::Free
        );
        assert_eq!(Rule::AskOwnWhenLosing.level(false, true), Some(Level::Ask));
        assert_eq!(Rule::AskOwnWhenLosing.level(true, true), None);
        assert_eq!(Rule::AskOwnWhenLosing.level(false, false), None);
        assert_eq!(Rule::Ask.level(true, false), Some(Level::Ask));
        assert_eq!(Rule::AskEach.level(false, false), Some(Level::AskEach));
        assert_eq!(Rule::AskOwn.level(true, true), None);
        assert_eq!(Rule::AskOwn.level(false, false), Some(Level::Ask));
        assert_eq!(Rule::Free.level(false, true), None);
    }
}
