//! The Templum Agent (docs/specs/templum-agent.md): the chat the person has with an agent that
//! builds and runs the canvas, and its state on the workspace.

use templum_agent::Conversation;

/// The Templum Agent's state: one per Templum.
pub(crate) struct TemplumAgent {
    pub conversation: Conversation,
    /// The secret its engine's process says hello with (`TEMPLUM_CONTROL`): new at each start
    /// of the engine, so no older hello matches.
    pub secret: String,
}

impl Default for TemplumAgent {
    fn default() -> Self {
        Self {
            conversation: Conversation::new("claude"),
            secret: uuid::Uuid::new_v4().simple().to_string(),
        }
    }
}
