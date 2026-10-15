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
        "Stop" => AgentEvent::Stopped {
            message: text("last_assistant_message"),
        },
        "Notification" => match input["notification_type"].as_str()? {
            "permission_prompt" | "elicitation_dialog" | "agent_needs_input" => {
                AgentEvent::NeedsYou {
                    message: text("message"),
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_codes_hooks_become_events() {
        let event = |input: Value| claude(&input);
        assert_eq!(
            event(
                json!({ "hook_event_name": "SessionStart", "source": "startup", "session_id": "s" })
            ),
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
            Some(AgentEvent::Stopped {
                message: Some("Done.".into())
            })
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
            Some(AgentEvent::NeedsYou {
                message: Some("Claude needs your permission to use Bash".into())
            })
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
        assert_eq!(
            event(json!({ "hook_event_name": "SessionEnd" })),
            Some(AgentEvent::Ended)
        );
        assert_eq!(event(json!({ "hook_event_name": "PreCompact" })), None);
        assert_eq!(event(json!("garbage")), None);
    }

    #[test]
    fn codexs_notify_is_a_finished_turn() {
        assert_eq!(
            codex(&json!({ "type": "agent-turn-complete", "last-assistant-message": "Fixed." })),
            Some(AgentEvent::Stopped {
                message: Some("Fixed.".into())
            })
        );
        assert_eq!(codex(&json!({ "type": "something-else" })), None);
    }

    #[test]
    fn events_go_over_the_bus_as_tagged_json() {
        assert_eq!(
            serde_json::to_value(AgentEvent::Stopped {
                message: Some("x".into())
            })
            .unwrap(),
            json!({ "event": "stopped", "message": "x" })
        );
        assert_eq!(
            serde_json::from_value::<AgentEvent>(json!({ "event": "needs_you", "message": null }))
                .unwrap(),
            AgentEvent::NeedsYou { message: None }
        );
    }
}
