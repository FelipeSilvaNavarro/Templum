//! The Templum Agent (docs/specs/templum-agent.md): the chat the person has with an agent that
//! builds and runs the canvas, and its state on the workspace.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "the pill and the chat call it (chat/)")
)]

use std::collections::{BTreeSet, VecDeque};

use gpui_kit::Task;
use serde_json::Value;
use templum_agent::{Conversation, Engine};
use templum_mcp::Replier;
use templum_mcp::control::Parsed;

/// The Templum Agent's state: one per Templum.
pub(crate) struct TemplumAgent {
    pub conversation: Conversation,
    /// The secret its engine's process says hello with (`TEMPLUM_CONTROL`): new at each start
    /// of the engine, so no older hello matches.
    pub secret: String,
    /// The tools "Allow for this conversation" let through, until New chat or quit.
    pub grants: BTreeSet<String>,
    /// The dangerous calls waiting for the person, oldest first.
    pub approvals: Vec<Approval>,
    pub next_approval: u64,
    /// It wrote something, or asks something, since the chat was last open.
    pub unread: bool,
    /// The chat is open.
    pub open: bool,
    /// Its engine's process, while it runs.
    pub engine: Option<Engine>,
    /// Counts the engine's starts: lines of an engine already gone are dropped.
    pub engine_gen: u64,
    /// Reads the engine's lines on the UI thread.
    pub lines: Option<Task<()>>,
    /// The engine's last lines on stderr, for the failure row when it ends.
    pub stderr: VecDeque<String>,
    /// The person pressed Stop: the turn's end is theirs, not a failure.
    pub stopping: bool,
    /// The settings changed while it worked: its process ends at the turn's end.
    pub restart: bool,
    /// The fake engine the tests run instead of Claude Code.
    #[cfg(test)]
    pub launch: Option<templum_agent::Launch>,
    /// The words of each start of the engine, as Claude Code would have got them.
    #[cfg(test)]
    pub launched: Vec<Vec<String>>,
}

impl Default for TemplumAgent {
    fn default() -> Self {
        Self {
            conversation: Conversation::new("claude"),
            secret: uuid::Uuid::new_v4().simple().to_string(),
            grants: BTreeSet::new(),
            approvals: Vec::new(),
            next_approval: 0,
            unread: false,
            open: false,
            engine: None,
            engine_gen: 0,
            lines: None,
            stderr: VecDeque::new(),
            stopping: false,
            restart: false,
            #[cfg(test)]
            launch: None,
            #[cfg(test)]
            launched: Vec::new(),
        }
    }
}

/// A dangerous call held until the person answers (docs/specs/templum-agent.md#approvals): its
/// connection and request, to answer it; the call; and what the popup says of it, written by
/// Templum from the call.
pub(crate) struct Approval {
    pub id: u64,
    pub conn: u64,
    pub request: Value,
    pub reply: Replier,
    pub parsed: Parsed,
    /// The call's arguments as the engine sent them: its row in the chat.
    pub args: Value,
    pub tool: String,
    /// The action in one sentence: `Close "Codex · api"`.
    pub said: String,
    /// Exactly what will happen: the text to type, the keys, what stops.
    pub details: String,
}

/// The person's answer to an approval.
#[cfg_attr(not(test), expect(dead_code, reason = "the approval popup answers it"))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Answer {
    Once,
    Conversation,
    /// Denied, with their words when they wrote some.
    Deny(Option<String>),
}
