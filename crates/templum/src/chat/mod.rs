//! The Templum Agent (docs/specs/templum-agent.md): the chat the person has with an agent that
//! builds and runs the canvas, and its state on the workspace.

use std::collections::BTreeSet;

use serde_json::Value;
use templum_agent::Conversation;
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
        }
    }
}

/// A dangerous call held until the person answers (docs/specs/templum-agent.md#approvals): its
/// connection and request, to answer it; the call; and what the popup says of it, written by
/// Templum from the call.
#[cfg_attr(not(test), expect(dead_code, reason = "the approval popup answers it"))]
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
