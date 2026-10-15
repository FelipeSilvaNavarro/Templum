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
/// doesn't follow.
pub fn claude(input: &Value) -> Option<AgentEvent> {
    let text = |key: &str| input[key].as_str().map(str::to_string);
    Some(match input["hook_event_name"].as_str()? {
        // Compaction (auto or /compact) starts the session again in the middle of a turn: it
        // is still working.
        "SessionStart" if input["source"] == "compact" => return None,
        "SessionStart" => AgentEvent::Started,
        "UserPromptSubmit" | "PostToolUse" => AgentEvent::Working,
        // Esc while a tool runs ends the turn: no `Stop` comes.
        "PostToolUseFailure" if input["is_interrupt"] == true => {
            AgentEvent::Stopped { message: None }
        }
        // A tool that failed (an allowed `cargo test` that fails): it works on.
        "PostToolUseFailure" => AgentEvent::Working,
        "Stop" => AgentEvent::Stopped {
            message: text("last_assistant_message"),
        },
        // The permission dialog just opened (its notification only comes after seconds
        // without typing). The hook prints nothing, so the dialog still decides.
        "PermissionRequest" => AgentEvent::NeedsYou {
            message: Some(match input["tool_name"].as_str() {
                Some(tool) => format!("Allow {tool}?"),
                None => "Allow a tool?".to_string(),
            }),
        },
        // A turn an API error ended: no `Stop`, and no answer.
        "StopFailure" => AgentEvent::Stopped { message: None },
        "Notification" => match input["notification_type"].as_str()? {
            "permission_prompt" | "elicitation_dialog" | "agent_needs_input" => {
                AgentEvent::NeedsYou {
                    message: text("message"),
                }
            }
            // The prompt has sat idle: whatever the turn was, it isn't working. Esc while the
            // model writes fires no hook of its own, so this is what ends its "Working". Not a
            // wait for the person: the agent asks nothing.
            "idle_prompt" => AgentEvent::Stopped { message: None },
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
            event(json!({ "hook_event_name": "SessionStart", "source": "compact" })),
            None,
            "an auto-compaction mid-turn leaves the turn working"
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
            event(json!({
                "hook_event_name": "PostToolUseFailure",
                "tool_name": "Bash",
                "error": "exit code 101",
                "is_interrupt": false,
            })),
            Some(AgentEvent::Working),
            "a tool that failed after a permission: back to work, not still waiting"
        );
        assert_eq!(
            event(json!({ "hook_event_name": "PostToolUseFailure", "tool_name": "Bash" })),
            Some(AgentEvent::Working)
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "PostToolUseFailure",
                "tool_name": "Bash",
                "is_interrupt": true,
            })),
            Some(AgentEvent::Stopped { message: None }),
            "Esc during a tool ends the turn"
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
                "hook_event_name": "PermissionRequest",
                "tool_name": "Bash",
                "tool_input": { "command": "rm -rf /tmp/build" },
            })),
            Some(AgentEvent::NeedsYou {
                message: Some("Allow Bash?".into())
            }),
            "waiting for you as soon as the dialog opens"
        );
        assert_eq!(
            event(json!({ "hook_event_name": "PermissionRequest" })),
            Some(AgentEvent::NeedsYou {
                message: Some("Allow a tool?".into())
            })
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "idle_prompt",
                "message": "Claude is waiting for your input",
            })),
            // Not None: after an Esc, nothing else says the turn is over.
            Some(AgentEvent::Stopped { message: None }),
            "an idle prompt means not working, never a wait for the person"
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "StopFailure",
                "error_type": "rate_limit",
                "error_message": "Rate limited",
            })),
            Some(AgentEvent::Stopped { message: None }),
            "a turn an API error ended"
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
