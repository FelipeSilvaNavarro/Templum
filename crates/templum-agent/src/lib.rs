//! The Templum Agent (docs/specs/templum-agent.md): the headless CLI agent the person talks to
//! in the chat, its stream read into events, and the conversation they make. No UI.

pub mod claude;

/// The Templum Agent's word where a terminal's id would be: the asker of the tasks it hands and
/// the `opened_by` of the blocks it opens. No terminal's id is one (theirs are `<pid>-<n>`).
pub const ASKER: &str = "templum-agent";

/// What one line of the engine's stream says, for the chat.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatEvent {
    /// A turn starts in this session, on this model.
    Started {
        session: String,
        model: Option<String>,
    },
    /// The engine thinks before its text or its tool.
    Thinking,
    /// More of the text being written.
    TextDelta(String),
    /// A text block's whole text, which replaces what was streamed of it.
    Text(String),
    /// A tool's call begins: its id and its name, before its arguments.
    ToolStarted { id: String, name: String },
    /// A tool's call, whole.
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// A tool's answer.
    ToolResult {
        id: String,
        text: String,
        is_error: bool,
    },
    /// The engine took a message the person wrote: the turn that answers it starts.
    Taken(String),
    /// The turn ended: its time and cost when the engine says them, and its error when it failed.
    TurnEnded {
        ms: Option<u64>,
        cost_usd: Option<f64>,
        error: Option<String>,
    },
}
