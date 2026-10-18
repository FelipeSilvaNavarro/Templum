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
/// The longest `timeout` a call may ask, seconds. A question asked without one may wait longer:
/// the agents Templum opens wait a week for a tool (`codex_flags`, templum-usage's
/// `MCP_TIMEOUT_MS`).
pub const MAX_WAIT: u64 = 3600;
/// How long a request to connect waits for the person, seconds, unless asked. A question waits
/// until it is answered (docs/specs/ask-user.md, How long it waits).
pub const ASK_WAIT: u64 = 1800;
/// How long `delegate` waits for the answer, and how long a task stays handed over, seconds,
/// unless asked.
pub const TASK_WAIT: u64 = 600;
/// How long `wait_for_idle` and `wait_for` wait, seconds, unless asked.
pub const IDLE_WAIT: u64 = 300;
/// The most keys `send_keys` presses in one call.
pub const MAX_KEYS: usize = 16;

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
    /// `timeout`: the call's own, if it gave one.
    AskUser {
        ask: crate::ask::Ask,
        timeout: Option<u64>,
    },
    Connect {
        block: String,
        timeout: u64,
    },
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
    WaitFor {
        block: String,
        event: Event,
        timeout: u64,
    },
    SpawnAgent(Spawn),
    ListCards {
        board: Option<String>,
        card: Option<u64>,
    },
    CreateCard(NewCard),
    EditCard(CardEdit),
    MoveCard {
        card: u64,
        column: String,
    },
    CommentCard {
        card: u64,
        text: String,
    },
    ClaimCard {
        card: u64,
    },
    ReadNote {
        block: String,
    },
    AppendNote {
        block: String,
        text: String,
    },
    ReadChecklist {
        block: String,
    },
    CheckItem {
        block: String,
        item: u64,
        done: bool,
    },
    AddItems {
        block: String,
        text: String,
    },
    OpenFile {
        path: String,
        line: Option<u32>,
    },
    /// Presses keys in a terminal, by name (`templum_terminal::KEY_NAMES`, checked by the app).
    SendKeys {
        block: String,
        keys: Vec<String>,
    },
    /// Esc in an agent; ends the task the caller handed it.
    Interrupt {
        block: String,
    },
    /// Closes a block the caller opened with `spawn_agent`.
    CloseBlock {
        block: String,
    },
    /// Wires two blocks: `templum wire`'s alone, never listed to agents.
    Wire {
        a: String,
        b: String,
    },
}

/// What `wait_for` waits for in a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// The agent finishes its turn; at once when it is idle.
    Idle,
    /// The agent waits for its person (a permission, a question); at once when it already does.
    NeedsYou,
    /// The next command that finishes in the terminal (needs the shell's integration).
    CommandFinished,
    /// The terminal rings or sends a notification.
    Attention,
}

/// What `spawn_agent` opens: the agent (an id of Settings ▸ Agents), its prompt, its folder (the
/// caller's when left out), and the worktree it makes first.
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub agent: String,
    pub prompt: Option<String>,
    pub folder: Option<String>,
    pub worktree: Option<String>,
    /// The model and the reasoning effort its agent starts on, checked by the app.
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// What `create_card` adds. `priority` is lowercased, one of low, medium, high or none; `who`
/// (the key `for`) is checked by the app; `checklist` is items' text, a line each.
#[derive(Clone, Debug, PartialEq)]
pub struct NewCard {
    pub board: Option<String>,
    pub column: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub who: Option<String>,
    pub checklist: Option<String>,
}

/// What `edit_card` changes on `card`: at least one of the others.
#[derive(Clone, Debug, PartialEq)]
pub struct CardEdit {
    pub card: u64,
    pub priority: Option<String>,
    pub who: Option<String>,
    pub add_items: Option<String>,
    pub check: Vec<u64>,
    pub uncheck: Vec<u64>,
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
    let board = json!({
        "type": "string",
        "description": "A board's name or id; may be left out when one kanban connected to you shows a board.",
    });
    let card = json!({ "type": "integer", "description": "A card's id, as list_cards shows it." });
    let column =
        json!({ "type": "string", "description": "A column's title (either case) or id." });
    let none = json!({ "type": "object", "properties": {} });
    let priority = json!({ "type": "string", "enum": ["low", "medium", "high", "none"] });
    let who = json!({
        "type": "string",
        "description": "Who it is for: anyone, an agent's id (claude, codex, …) for any of its sessions, or a \
                        terminal block's id for that session.",
    });
    let items = json!({
        "type": "string",
        "description": "Checklist items, one per line; a leading - or *, then [ ] or [x], is dropped ([x] \
                        adds it checked).",
    });
    let item_ids = json!({
        "anyOf": [
            { "type": "integer" },
            { "type": "array", "items": { "type": "integer" } },
        ],
        "description": "An item's id, or a list of them, as list_cards with card shows them.",
    });
    // Claude Code shows these whole from the start, beside its own Agent tool, where the others
    // wait behind its tool search by name only.
    let always_load = json!({ "anthropic/alwaysLoad": true });
    let seconds = |default: u64| {
        json!({
            "type": "integer",
            "minimum": 1,
            "maximum": MAX_WAIT,
            "default": default,
            "description": "Seconds to wait before giving up.",
        })
    };
    // ask_user's, apart: one json! of every tool reaches the macro's recursion limit.
    let choice = json!({
        "type": "object",
        "properties": {
            "label": { "type": "string" },
            "description": { "type": "string" },
            "preview": {
                "type": "string",
                "description": "Shown beside the options, in a monospace frame.",
            },
        },
        "required": ["label"],
    });
    let question = json!({
        "type": "object",
        "properties": {
            "question": { "type": "string" },
            "header": {
                "type": "string",
                "description": "A short label, shown up to 12 characters.",
            },
            "options": { "type": "array", "items": choice },
            "multiSelect": { "type": "boolean", "default": false },
        },
        "required": ["question"],
    });
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
            "description": "Asks the person in a dialog on your block, as AskUserQuestion does: questions, \
                            each with a header, options (label, description, preview) and multiSelect; every \
                            question also gets an Other field, and the person may add a note. Returns \
                            {\"answers\": {question: answer, or a list for multiSelect}, \"note\": …}. Waits \
                            until the person answers: no limit unless you give timeout (seconds) or the \
                            person set one; if your client stops waiting, the answer is typed into your \
                            terminal later. The older question with options (strings) still works and \
                            returns the answer as text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "questions": { "type": "array", "minItems": 1, "items": question },
                    "question": {
                        "type": "string",
                        "description": "The older shape: one question, without questions.",
                    },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "The older shape's answers to offer; none for a text field.",
                    },
                    "timeout": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_WAIT,
                        "description": "Seconds to wait before giving up; none waits until the person answers.",
                    },
                },
            },
        },
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
        {
            "name": "delegate",
            "description": "Hands a task to an agent connected to you: Templum types it into its terminal, after \
                            a line saying it is from you and its id (t7), and sends it. Its answer is its last \
                            message when it finishes its turn, or what it sends you with reply. With wait, waits \
                            for that answer (it may take minutes); without, says its id at once, and its end comes \
                            back typed into your terminal as [task t7 …] with its time, what it spent and its \
                            transcript, as does a dialog it waits on. To the agent on your task, it is a change to \
                            that task, typed at once. Refused while the agent waits for its person or is on \
                            another agent's task.",
            "_meta": always_load,
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
                            (<id>), task t7]): the agent waiting for it gets your text now. When it isn't waiting, the \
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
        {
            "name": "wait_for",
            "description": "Waits for an event in a block connected to you. idle: the agent finishes its turn \
                            (at once when it is idle), with its last answer. needs_you: the agent waits for its \
                            person, at a permission prompt or a question (at once when it already does). \
                            command_finished: the next command that finishes in the terminal, with its output \
                            (needs the shell's integration). attention: the terminal rings or sends a \
                            notification. It may take minutes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "block": block,
                    "event": {
                        "type": "string",
                        "enum": ["idle", "needs_you", "command_finished", "attention"],
                    },
                    "timeout": seconds(IDLE_WAIT),
                },
                "required": ["block", "event"],
            },
        },
        {
            "name": "spawn_agent",
            "description": "Opens a new agent block beside yours, connected to you: in your folder, the one you \
                            name, or a new Git worktree of that folder's repository (.worktrees/<worktree>, on a \
                            new branch of that name). A prompt is typed once the agent is ready, as a task you \
                            handed it (the result says its id): its end comes back as delegate's does, or \
                            wait_for_idle gets it. Says the new block's id and folder. \
                            With model and effort, its command line starts it on them, and so does every resume.",
            "_meta": always_load,
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
                    "model": {
                        "type": "string",
                        "description": "Its model, as its own --model or -m takes it (provider/model for OpenCode, \
                                        Kilo Code and Pi). Letters, digits, . - _ : and /.",
                    },
                    "effort": {
                        "type": "string",
                        "description": "Its reasoning effort. claude: low, medium, high, xhigh, max. codex: low, medium, \
                                        high, xhigh, max, ultra. copilot and grok: none, minimal, low, medium, high, \
                                        xhigh, max. pi: off, minimal, low, medium, high, xhigh, max. kiro: low, medium, \
                                        high, xhigh, max. cline: none, low, medium, high, xhigh. aider: low, medium, \
                                        high. The others take none.",
                    },
                },
                "required": ["agent"],
            },
        },
        {
            "name": "list_cards",
            "description": "A board shown by a kanban connected to you: its columns, and each card's id, title, \
                            labels, priority, assignee ((you) on yours), checklist progress and comment count. \
                            With card, that card whole: its description, checklist items and comments too.",
            "inputSchema": { "type": "object", "properties": { "board": board, "card": card } },
        },
        {
            "name": "create_card",
            "description": "Adds a card at the end of a column (the board's first when left out) of a board shown \
                            by a kanban connected to you. Says its id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "board": board,
                    "column": column,
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "priority": priority,
                    "for": who,
                    "checklist": items,
                },
                "required": ["title"],
            },
        },
        {
            "name": "edit_card",
            "description": "Sets a card's priority and who it is for, adds checklist items, checks and unchecks \
                            them: one step of the person's Ctrl+Z. Check an item as you finish it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "card": card,
                    "priority": priority,
                    "for": who,
                    "add_items": items,
                    "check": item_ids,
                    "uncheck": item_ids,
                },
                "required": ["card"],
            },
        },
        {
            "name": "move_card",
            "description": "Moves a card to the end of a column of its board. Done with your card: comment \
                            on it first what you did (comment_card), then move it to Review or Done.",
            "inputSchema": {
                "type": "object",
                "properties": { "card": card, "column": column },
                "required": ["card", "column"],
            },
        },
        {
            "name": "comment_card",
            "description": "Adds a comment to a card, signed with your block's id: what you did on it, \
                            and what the next one needs to know, when you finish it.",
            "inputSchema": {
                "type": "object",
                "properties": { "card": card, "text": { "type": "string" } },
                "required": ["card", "text"],
            },
        },
        {
            "name": "claim_card",
            "description": "Makes you the card's assignee. Refused when another block holds it.",
            "inputSchema": { "type": "object", "properties": { "card": card }, "required": ["card"] },
        },
        {
            "name": "read_note",
            "description": "The text of the note a note block connected to you shows.",
            "inputSchema": { "type": "object", "properties": { "block": block }, "required": ["block"] },
        },
        {
            "name": "append_note",
            "description": "Adds text at the end of the note a note block connected to you shows, after a blank \
                            line. The person sees it at once and can take it back.",
            "inputSchema": {
                "type": "object",
                "properties": { "block": block, "text": { "type": "string" } },
                "required": ["block", "text"],
            },
        },
        {
            "name": "read_checklist",
            "description": "The checklist a checklist block connected to you shows: its name and progress, \
                            then each item's id, [x] or [ ], and text.",
            "inputSchema": { "type": "object", "properties": { "block": block }, "required": ["block"] },
        },
        {
            "name": "check_item",
            "description": "Checks an item of the checklist a checklist block connected to you shows; \
                            unchecks it with done false.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "block": block,
                    "item": { "type": "integer", "description": "An item's id, as read_checklist shows it." },
                    "done": { "type": "boolean", "default": true },
                },
                "required": ["block", "item"],
            },
        },
        {
            "name": "add_items",
            "description": "Adds items at the end of the checklist a checklist block connected to you shows: \
                            one per line with text; a leading - or *, then [ ] or [x], is dropped ([x] adds it \
                            checked). The person sees them at once and can take them back.",
            "inputSchema": {
                "type": "object",
                "properties": { "block": block, "text": { "type": "string" } },
                "required": ["block", "text"],
            },
        },
        {
            "name": "open_file",
            "description": "Shows a file, at a line, in the editor connected to you whose folder holds it; with \
                            none, in a new editor beside your block, connected to you. The person's keyboard and \
                            view stay where they are.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The file, relative to your folder or whole." },
                    "line": { "type": "integer", "minimum": 1, "description": "The line, from 1." },
                },
                "required": ["path"],
            },
        },
        {
            "name": "send_keys",
            "description": "Presses keys in a terminal connected to you, one at a time, as a keyboard does (never \
                            as a paste): to answer an agent's dialog (a permission, a trust question) or drive a \
                            program. Keys: enter, escape, tab, shift+tab, backspace, delete, up, down, left, right, \
                            home, end, page_up, page_down, space, ctrl+<letter>, or one character (2, y). Returns \
                            the last 20 lines of its screen after them. Not your own block, nor an agent on another \
                            agent's task unless it waits for its person.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "block": block,
                    "keys": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "maxItems": MAX_KEYS,
                    },
                },
                "required": ["block", "keys"],
            },
        },
        {
            "name": "interrupt",
            "description": "Presses Esc once in an agent connected to you while it works or waits for its person, \
                            and ends the task you handed it: its waits get \"Interrupted by you.\" Returns its \
                            screen after. Refused while it is idle or on another agent's task.",
            "inputSchema": { "type": "object", "properties": { "block": block }, "required": ["block"] },
        },
        {
            "name": "close_block",
            "description": "Closes a block you opened with spawn_agent and are still connected to, without asking \
                            the person (they bring it back with Ctrl+Z, its agent resumed). Only the person closes \
                            any other block.",
            "inputSchema": { "type": "object", "properties": { "block": block }, "required": ["block"] },
        },
    ]})
}

/// `name` is one of the tools.
pub fn known(name: &str) -> bool {
    list()["tools"]
        .as_array()
        .is_some_and(|tools| tools.iter().any(|t| t["name"] == name))
}

/// `name` is a call only the command line makes (`templum wire`): not listed, so agents never
/// see it.
pub fn cli_only(name: &str) -> bool {
    name == "wire"
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
    let timeout = || match &args["timeout"] {
        Value::Null => Ok(None),
        value => value
            .as_u64()
            .filter(|n| *n >= 1)
            .map(|n| Some(n.min(MAX_WAIT)))
            .ok_or_else(|| format!("\"timeout\" is a number of seconds from 1 to {MAX_WAIT}")),
    };
    let seconds = |default: u64| timeout().map(|t| t.unwrap_or(default));
    // A blank string is no value: `"worktree": ""` names no worktree, `"prompt": " "` no task.
    let optional = |key: &str| match &args[key] {
        Value::Null => Ok(None),
        value => value
            .as_str()
            .map(|s| Some(s.to_string()).filter(|s| !s.trim().is_empty()))
            .ok_or_else(|| format!("\"{key}\" is a string")),
    };
    // Boards, columns and cards may come as numbers or as strings.
    let named = |key: &str| match &args[key] {
        Value::Null => Ok(None),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::String(s) => Ok(Some(s.trim().to_string()).filter(|s| !s.is_empty())),
        _ => Err(format!("\"{key}\" is a name or an id")),
    };
    // `shown_by`: the tool that lists them.
    let id = |key: &str, shown_by: &str| -> Result<Option<u64>, String> {
        let bad = || format!("\"{key}\" is an id, as {shown_by} shows it");
        match &args[key] {
            Value::Null => Ok(None),
            Value::Number(n) => n.as_u64().map(Some).ok_or_else(bad),
            Value::String(s) => s.trim().parse().map(Some).map_err(|_| bad()),
            _ => Err(bad()),
        }
    };
    let card =
        || id("card", "list_cards")?.ok_or_else(|| format!("{name} needs \"card\", a card's id"));
    let priority = || match optional("priority")? {
        Some(p) => {
            let p = p.trim().to_lowercase();
            ["low", "medium", "high", "none"]
                .contains(&p.as_str())
                .then_some(Some(p))
                .ok_or_else(|| "\"priority\" is low, medium, high or none".to_string())
        }
        None => Ok(None),
    };
    // One item's id (a number or its text), or a list of them.
    let item_ids = |key: &str| {
        let one = |v: &Value| match v {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        };
        match &args[key] {
            Value::Null => Some(Vec::new()),
            Value::Array(ids) => ids.iter().map(one).collect(),
            v => one(v).map(|id| vec![id]),
        }
        .ok_or_else(|| {
            format!("\"{key}\" is an item's id, or a list of them, as list_cards shows them")
        })
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
            ask: crate::ask::parse(args)?,
            timeout: timeout()?,
        },
        "connect" => Call::Connect {
            block: text("block")?,
            timeout: seconds(ASK_WAIT)?,
        },
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
        "wait_for" => Call::WaitFor {
            block: text("block")?,
            event: match args["event"].as_str() {
                Some("idle") => Event::Idle,
                Some("needs_you") => Event::NeedsYou,
                Some("command_finished") => Event::CommandFinished,
                Some("attention") => Event::Attention,
                _ => {
                    return Err(format!(
                        "{name} needs \"event\": idle, needs_you, command_finished or attention"
                    ));
                }
            },
            timeout: seconds(IDLE_WAIT)?,
        },
        "spawn_agent" => Call::SpawnAgent(Spawn {
            agent: text("agent")?,
            prompt: optional("prompt")?,
            folder: optional("folder")?,
            worktree: optional("worktree")?,
            model: optional("model")?,
            effort: optional("effort")?,
        }),
        "list_cards" => Call::ListCards {
            board: named("board")?,
            card: id("card", "list_cards")?,
        },
        "create_card" => Call::CreateCard(NewCard {
            board: named("board")?,
            column: named("column")?,
            title: text("title")?,
            description: optional("description")?,
            priority: priority()?,
            who: optional("for")?,
            checklist: optional("checklist")?,
        }),
        "edit_card" => {
            let edit = CardEdit {
                card: card()?,
                priority: priority()?,
                who: optional("for")?,
                add_items: optional("add_items")?,
                check: item_ids("check")?,
                uncheck: item_ids("uncheck")?,
            };
            if edit.priority.is_none()
                && edit.who.is_none()
                && edit.add_items.is_none()
                && edit.check.is_empty()
                && edit.uncheck.is_empty()
            {
                return Err(
                    "edit_card needs something to change: priority, for, add_items, check or uncheck"
                        .into(),
                );
            }
            Call::EditCard(edit)
        }
        "move_card" => Call::MoveCard {
            card: card()?,
            column: named("column")?
                .ok_or_else(|| format!("{name} needs \"column\", a title or an id"))?,
        },
        "comment_card" => Call::CommentCard {
            card: card()?,
            text: text("text")?,
        },
        "claim_card" => Call::ClaimCard { card: card()? },
        "read_note" => Call::ReadNote {
            block: text("block")?,
        },
        "append_note" => Call::AppendNote {
            block: text("block")?,
            text: text("text")?,
        },
        "read_checklist" => Call::ReadChecklist {
            block: text("block")?,
        },
        "check_item" => Call::CheckItem {
            block: text("block")?,
            item: id("item", "read_checklist")?
                .ok_or_else(|| format!("{name} needs \"item\", an item's id"))?,
            done: match &args["done"] {
                Value::Null => true,
                value => value
                    .as_bool()
                    .ok_or_else(|| "\"done\" is true or false".to_string())?,
            },
        },
        "add_items" => Call::AddItems {
            block: text("block")?,
            text: text("text")?,
        },
        "open_file" => Call::OpenFile {
            path: text("path")?,
            line: match &args["line"] {
                Value::Null => None,
                value => Some(
                    value
                        .as_u64()
                        .filter(|n| *n >= 1)
                        .map(|n| n.min(u64::from(u32::MAX)) as u32)
                        .ok_or_else(|| "\"line\" is a line number, from 1".to_string())?,
                ),
            },
        },
        "send_keys" => Call::SendKeys {
            block: text("block")?,
            keys: match &args["keys"] {
                Value::Array(keys) if (1..=MAX_KEYS).contains(&keys.len()) => keys
                    .iter()
                    .map(|k| k.as_str().map(str::to_string))
                    .collect::<Option<_>>(),
                _ => None,
            }
            .ok_or_else(|| {
                format!(
                    "{name} needs \"keys\", a list of 1 to {MAX_KEYS} key names (enter, escape, ctrl+c, 2…)"
                )
            })?,
        },
        "interrupt" => Call::Interrupt {
            block: text("block")?,
        },
        "close_block" => Call::CloseBlock {
            block: text("block")?,
        },
        "wire" => Call::Wire {
            a: text("a")?,
            b: text("b")?,
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
                json!({
                    "questions": [
                        {
                            "question": "Which database should the service use?",
                            "header": "Database",
                            "options": [
                                { "label": "Postgres", "description": "What production runs",
                                  "preview": "DATABASE_URL=postgres://…\npool = 10" },
                                { "label": "SQLite", "description": "One file, no server" }
                            ],
                            "multiSelect": false
                        },
                        {
                            "question": "Which features go in the first release?",
                            "header": "Features",
                            "options": [{ "label": "Auth" }, { "label": "Search" }, { "label": "Export" }],
                            "multiSelect": true
                        }
                    ],
                    "timeout": 600
                }),
            ),
            ("connect", json!({ "block": "k9x1d", "timeout": 60 })),
            (
                "delegate",
                json!({ "block": "k9x1d", "task": "fix the tests", "wait": true, "timeout": 900 }),
            ),
            ("reply", json!({ "block": "b7k2m", "text": "Done." })),
            ("wait_for_idle", json!({ "block": "k9x1d", "timeout": 60 })),
            (
                "wait_for",
                json!({ "block": "k9x1d", "event": "needs_you", "timeout": 60 }),
            ),
            (
                "spawn_agent",
                json!({ "agent": "codex", "prompt": "fix the tests", "folder": "../api", "worktree": "fix-tests",
                        "model": "gpt-5.6-luna", "effort": "high" }),
            ),
            ("list_cards", json!({ "board": "Sprint", "card": 12 })),
            (
                "create_card",
                json!({ "board": "1", "column": "To do", "title": "Fix login", "description": "It 500s.",
                        "priority": "high", "for": "codex", "checklist": "- [ ] Build" }),
            ),
            (
                "edit_card",
                json!({ "card": 12, "priority": "low", "for": "anyone", "add_items": "Docs", "check": 13,
                        "uncheck": [14] }),
            ),
            ("move_card", json!({ "card": 12, "column": "Doing" })),
            ("comment_card", json!({ "card": 12, "text": "On it." })),
            ("claim_card", json!({ "card": "12" })),
            ("read_note", json!({ "block": "4qm2a" })),
            (
                "append_note",
                json!({ "block": "4qm2a", "text": "Next: the tests." }),
            ),
            ("read_checklist", json!({ "block": "4qm2a" })),
            (
                "check_item",
                json!({ "block": "4qm2a", "item": 13, "done": false }),
            ),
            (
                "add_items",
                json!({ "block": "4qm2a", "text": "- [ ] Build\n- [ ] Test" }),
            ),
            ("open_file", json!({ "path": "src/main.rs", "line": 12 })),
            (
                "send_keys",
                json!({ "block": "k9x1d", "keys": ["2", "enter"] }),
            ),
            ("interrupt", json!({ "block": "k9x1d" })),
            ("close_block", json!({ "block": "k9x1d" })),
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
        let why = crate::ask::parse(&json!({ "question": "Why?" })).unwrap();
        assert_eq!(
            parse("ask_user", &json!({ "question": "Why?" })),
            Ok(Call::AskUser {
                ask: why.clone(),
                timeout: None
            }),
            "a question waits until it is answered"
        );
        assert_eq!(
            parse(
                "ask_user",
                &json!({ "question": "Why?", "timeout": 99_999 })
            ),
            Ok(Call::AskUser {
                ask: why,
                timeout: Some(MAX_WAIT)
            })
        );
        assert_eq!(
            parse("connect", &json!({ "block": "b" })),
            Ok(Call::Connect {
                block: "b".into(),
                timeout: ASK_WAIT
            })
        );
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
        assert_eq!(
            parse(
                "wait_for",
                &json!({ "block": "b", "event": "command_finished" })
            ),
            Ok(Call::WaitFor {
                block: "b".into(),
                event: Event::CommandFinished,
                timeout: IDLE_WAIT
            })
        );
        assert_eq!(
            parse(
                "wait_for",
                &json!({ "block": "b", "event": "idle", "timeout": 99_999 })
            ),
            Ok(Call::WaitFor {
                block: "b".into(),
                event: Event::Idle,
                timeout: MAX_WAIT
            })
        );
        assert!(parse("reply", &json!({ "block": "b" })).is_err(), "no text");
        assert_eq!(
            parse("spawn_agent", &json!({ "agent": "claude" })),
            Ok(Call::SpawnAgent(Spawn {
                agent: "claude".into(),
                prompt: None,
                folder: None,
                worktree: None,
                model: None,
                effort: None
            }))
        );
        assert_eq!(
            parse(
                "spawn_agent",
                &json!({ "agent": "claude", "prompt": "", "folder": "", "worktree": "", "model": "", "effort": "" })
            ),
            parse("spawn_agent", &json!({ "agent": "claude" })),
            "an empty string is no value"
        );
        assert_eq!(
            parse(
                "spawn_agent",
                &json!({ "agent": "claude", "prompt": " \n", "folder": " ", "worktree": "\t", "model": " ", "effort": "\t" })
            ),
            parse("spawn_agent", &json!({ "agent": "claude" })),
            "nor is a blank one: a blank prompt typed is no task"
        );
        assert!(parse("spawn_agent", &json!({ "agent": "claude", "prompt": 3 })).is_err());
        for bad in [
            json!({}),
            json!({ "question": "Why?", "options": "yes" }),
            json!({ "question": "Why?", "options": [1] }),
            json!({ "question": "Why?", "timeout": 0 }),
        ] {
            assert!(parse("ask_user", &bad).is_err(), "{bad}");
        }
    }

    /// Both shapes, neither required: a client that checks a call against the schema still sends
    /// today's. Options are objects only (no `anyOf`), though strings are taken.
    #[test]
    fn ask_users_schema_lists_both_shapes() {
        let tools = list();
        let ask = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "ask_user")
            .unwrap();
        let schema = &ask["inputSchema"];
        assert_eq!(schema["required"], Value::Null);
        for key in ["questions", "question", "options", "timeout"] {
            assert!(schema["properties"][key].is_object(), "{key}");
        }
        let question = &schema["properties"]["questions"]["items"]["properties"];
        for key in ["question", "header", "options", "multiSelect"] {
            assert!(question[key].is_object(), "{key}");
        }
        assert_eq!(question["options"]["items"]["type"], "object");
        assert!(
            ask["description"]
                .as_str()
                .unwrap()
                .starts_with("Asks the person in a dialog on your block, as AskUserQuestion does")
        );
    }

    /// The events tools/list offers are the ones wait_for takes; any other is refused, naming them.
    #[test]
    fn wait_for_takes_the_four_events_it_lists() {
        let tools = list();
        let listed = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "wait_for")
            .unwrap();
        assert_eq!(listed["inputSchema"]["required"], json!(["block", "event"]));
        let events: Vec<Event> = listed["inputSchema"]["properties"]["event"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(
                |name| match parse("wait_for", &json!({ "block": "b", "event": name })) {
                    Ok(Call::WaitFor { event, .. }) => event,
                    other => panic!("{name}: {other:?}"),
                },
            )
            .collect();
        assert_eq!(
            events,
            [
                Event::Idle,
                Event::NeedsYou,
                Event::CommandFinished,
                Event::Attention
            ]
        );
        for bad in [
            json!({ "block": "b" }),
            json!({ "block": "b", "event": "done" }),
            json!({ "block": "b", "event": 1 }),
        ] {
            let err = parse("wait_for", &bad).unwrap_err();
            assert!(
                err.contains("\"event\"") && err.contains("needs_you") && err.contains("attention"),
                "{err}"
            );
        }
        assert!(
            parse(
                "wait_for",
                &json!({ "block": "b", "event": "idle", "timeout": 0 })
            )
            .is_err()
        );
    }

    /// send_keys takes 1 to 16 keys, as strings; their names are the app's to check.
    #[test]
    fn send_keys_takes_one_to_sixteen_keys() {
        assert_eq!(
            parse("send_keys", &json!({ "block": "b", "keys": ["ctrl+c"] })),
            Ok(Call::SendKeys {
                block: "b".into(),
                keys: vec!["ctrl+c".into()]
            })
        );
        let (full, over) = (vec!["y"; MAX_KEYS], vec!["y"; MAX_KEYS + 1]);
        assert!(parse("send_keys", &json!({ "block": "b", "keys": full })).is_ok());
        for bad in [
            json!({ "block": "b" }),
            json!({ "block": "b", "keys": [] }),
            json!({ "block": "b", "keys": over }),
            json!({ "block": "b", "keys": "enter" }),
            json!({ "block": "b", "keys": [13] }),
        ] {
            let err = parse("send_keys", &bad).unwrap_err();
            assert!(
                err.contains("\"keys\"") && err.contains("16"),
                "{bad}: {err}"
            );
        }
        assert_eq!(
            parse("interrupt", &json!({ "block": "b" })),
            Ok(Call::Interrupt { block: "b".into() })
        );
        assert_eq!(
            parse("close_block", &json!({ "block": "b" })),
            Ok(Call::CloseBlock { block: "b".into() })
        );
        assert!(parse("close_block", &json!({})).is_err());
    }

    /// `wire` is the command line's: agents never see it listed, and only `templum wire` makes it.
    #[test]
    fn wire_is_the_command_lines_alone() {
        assert!(!known("wire") && cli_only("wire"));
        assert!(!cli_only("wait_for") && !cli_only("whoami"));
        assert_eq!(
            parse("wire", &json!({ "a": "b7k2m", "b": "k9x1d" })),
            Ok(Call::Wire {
                a: "b7k2m".into(),
                b: "k9x1d".into()
            })
        );
        let err = parse("wire", &json!({ "a": "b7k2m" })).unwrap_err();
        assert!(err.contains("\"b\""), "{err}");
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

    /// Starting and handing work to agents is listed whole in Claude Code, beside its Agent tool.
    #[test]
    fn the_agent_tools_alone_load_upfront() {
        let upfront: Vec<_> = list()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["_meta"]["anthropic/alwaysLoad"] == true)
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(upfront, ["delegate", "spawn_agent"]);
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

    #[test]
    fn create_and_edit_card_take_a_priority_who_and_checklist_items() {
        assert_eq!(
            parse(
                "create_card",
                &json!({ "title": "v2", "priority": " High", "for": "codex", "checklist": "- a" })
            ),
            Ok(Call::CreateCard(NewCard {
                board: None,
                column: None,
                title: "v2".into(),
                description: None,
                priority: Some("high".into()),
                who: Some("codex".into()),
                checklist: Some("- a".into()),
            }))
        );
        let edit = |args: Value| parse("edit_card", &args);
        let only = |check: Vec<u64>, uncheck: Vec<u64>| {
            Ok(Call::EditCard(CardEdit {
                card: 12,
                priority: None,
                who: None,
                add_items: None,
                check,
                uncheck,
            }))
        };
        assert_eq!(
            edit(json!({ "card": 12, "check": 3 })),
            only(vec![3], vec![])
        );
        assert_eq!(
            edit(json!({ "card": "12", "check": ["3", 4] })),
            only(vec![3, 4], vec![])
        );
        assert_eq!(
            edit(json!({ "card": 12, "uncheck": [5] })),
            only(vec![], vec![5])
        );
        assert_eq!(
            edit(json!({ "card": 12, "priority": "NONE", "for": "anyone", "add_items": "x" })),
            Ok(Call::EditCard(CardEdit {
                card: 12,
                priority: Some("none".into()),
                who: Some("anyone".into()),
                add_items: Some("x".into()),
                check: vec![],
                uncheck: vec![],
            }))
        );
        for (tool, args, says) in [
            (
                "create_card",
                json!({ "title": "x", "priority": "urgent" }),
                "\"priority\" is low, medium, high or none",
            ),
            (
                "edit_card",
                json!({ "card": 1, "check": "x" }),
                "\"check\" is an item's id, or a list of them, as list_cards shows them",
            ),
            (
                "edit_card",
                json!({ "card": 1, "uncheck": [true] }),
                "\"uncheck\" is an item's id, or a list of them, as list_cards shows them",
            ),
            (
                "edit_card",
                json!({ "card": 1 }),
                "edit_card needs something to change: priority, for, add_items, check or uncheck",
            ),
        ] {
            assert_eq!(parse(tool, &args), Err(says.to_string()), "{args}");
        }
        let tools = list();
        let edit_card = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "edit_card")
            .unwrap();
        assert_eq!(edit_card["inputSchema"]["required"], json!(["card"]));
    }

    #[test]
    fn the_content_tools_take_names_or_ids_and_leave_out_what_they_may() {
        assert_eq!(
            parse("list_cards", &json!({})),
            Ok(Call::ListCards {
                board: None,
                card: None
            })
        );
        assert_eq!(
            parse("list_cards", &json!({ "board": 1, "card": "7" })),
            Ok(Call::ListCards {
                board: Some("1".into()),
                card: Some(7)
            })
        );
        assert_eq!(
            parse("create_card", &json!({ "title": "x" })),
            Ok(Call::CreateCard(NewCard {
                board: None,
                column: None,
                title: "x".into(),
                description: None,
                priority: None,
                who: None,
                checklist: None,
            }))
        );
        assert_eq!(
            parse("move_card", &json!({ "card": 3, "column": 4 })),
            Ok(Call::MoveCard {
                card: 3,
                column: "4".into()
            })
        );
        assert_eq!(
            parse("claim_card", &json!({ "card": " 9 " })),
            Ok(Call::ClaimCard { card: 9 })
        );
        assert_eq!(
            parse("open_file", &json!({ "path": "a.rs" })),
            Ok(Call::OpenFile {
                path: "a.rs".into(),
                line: None
            })
        );
        for (tool, bad) in [
            ("move_card", json!({ "column": "Doing" })),
            ("move_card", json!({ "card": "x", "column": "Doing" })),
            ("move_card", json!({ "card": 1 })),
            ("comment_card", json!({ "card": 1 })),
            ("claim_card", json!({ "card": -1 })),
            ("create_card", json!({})),
            ("open_file", json!({ "path": "a.rs", "line": 0 })),
            ("open_file", json!({})),
            ("append_note", json!({ "block": "n" })),
        ] {
            assert!(parse(tool, &bad).is_err(), "{tool} {bad}");
        }
        let err = parse("move_card", &json!({ "column": "Doing" })).unwrap_err();
        assert!(err.contains("\"card\""), "{err}");
        assert_eq!(
            parse("check_item", &json!({ "block": "c", "item": "13" })),
            Ok(Call::CheckItem {
                block: "c".into(),
                item: 13,
                done: true
            })
        );
        for bad in [
            json!({ "block": "c" }),
            json!({ "block": "c", "item": "x" }),
            json!({ "item": 1 }),
            json!({ "block": "c", "item": 1, "done": "no" }),
        ] {
            assert!(parse("check_item", &bad).is_err(), "{bad}");
        }
        let err = parse("check_item", &json!({ "block": "c", "item": "x" })).unwrap_err();
        assert!(err.contains("read_checklist"), "{err}");
        assert!(parse("add_items", &json!({ "block": "c" })).is_err());
    }
}
