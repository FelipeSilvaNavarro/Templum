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
    /// It took a prompt: a turn starts or takes it in. `at_enter`: it says so at its Enter, also
    /// for a prompt it queues behind the turn under way (Claude Code); the others once a turn
    /// takes it.
    Submitted {
        #[serde(default)]
        at_enter: bool,
    },
    /// It works on: a tool just ran.
    Working,
    /// It finished its turn, with its last answer.
    Stopped {
        message: Option<String>,
        /// The transcript its hook named (`transcript_path`), for the notice of a task's end.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transcript: Option<String>,
    },
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
        "UserPromptSubmit" => AgentEvent::Submitted { at_enter: true },
        "PostToolUse" => AgentEvent::Working,
        // Esc while a tool runs ends the turn: no `Stop` comes.
        "PostToolUseFailure" if input["is_interrupt"] == true => AgentEvent::Stopped {
            message: None,
            transcript: text("transcript_path"),
        },
        // A tool that failed (an allowed `cargo test` that fails): it works on.
        "PostToolUseFailure" => AgentEvent::Working,
        "Stop" => AgentEvent::Stopped {
            message: text("last_assistant_message"),
            transcript: text("transcript_path"),
        },
        // The permission dialog just opened (its notification only comes after seconds
        // without typing). The hook prints nothing, so the dialog still decides.
        "PermissionRequest" => AgentEvent::NeedsYou {
            message: Some(match input["tool_name"].as_str() {
                // Claude's own dialogs: nothing to allow.
                Some("AskUserQuestion") => "Claude asks you a question".to_string(),
                Some("ExitPlanMode") => "Claude has a plan for you".to_string(),
                Some(tool) => format!("Allow {tool}?"),
                None => "Allow a tool?".to_string(),
            }),
        },
        // A turn an API error ended: no `Stop`, and no answer.
        "StopFailure" => AgentEvent::Stopped {
            message: None,
            transcript: text("transcript_path"),
        },
        "Notification" => match input["notification_type"].as_str()? {
            // The dialog `PermissionRequest` already said, seconds ago: its "Allow …?" stays.
            "permission_prompt" => AgentEvent::NeedsYou { message: None },
            "elicitation_dialog" | "agent_needs_input" => AgentEvent::NeedsYou {
                message: text("message"),
            },
            // The prompt has sat idle: whatever the turn was, it isn't working. Esc while the
            // model writes fires no hook of its own, so this is what ends its "Working". Not a
            // wait for the person: the agent asks nothing.
            "idle_prompt" => AgentEvent::Stopped {
                message: None,
                transcript: text("transcript_path"),
            },
            _ => return None,
        },
        "SessionEnd" => AgentEvent::Ended,
        _ => return None,
    })
}

/// A Codex hook's input (as of 0.154, docs/plans/2026-10-16-connections-batch-7.md, S1). None
/// for what Templum doesn't follow: `SessionEnd` also comes after 30 idle minutes.
pub fn codex(input: &Value) -> Option<AgentEvent> {
    let text = |key: &str| input[key].as_str().map(str::to_string);
    Some(match input["hook_event_name"].as_str()? {
        "SessionStart" if input["source"] == "compact" => return None,
        "SessionStart" => AgentEvent::Started,
        // Once a turn takes the prompt in: a new turn, or the one under way at its next tool.
        "UserPromptSubmit" => AgentEvent::Submitted { at_enter: false },
        "PostToolUse" => AgentEvent::Working,
        "PermissionRequest" => AgentEvent::NeedsYou {
            message: Some(match input["tool_name"].as_str() {
                Some(tool) => format!("Allow {tool}?"),
                None => "Allow a tool?".to_string(),
            }),
        },
        "Stop" => AgentEvent::Stopped {
            message: text("last_assistant_message"),
            transcript: text("transcript_path"),
        },
        // Esc or Ctrl+C: the turn ends, no `Stop` comes.
        "Interrupt" => AgentEvent::Stopped {
            message: None,
            transcript: text("transcript_path"),
        },
        _ => return None,
    })
}

/// A Gemini CLI hook's input (as of 0.63), read as Claude Code's. None for what Templum doesn't
/// follow.
pub fn gemini(input: &Value) -> Option<AgentEvent> {
    let text = |key: &str| input[key].as_str().map(str::to_string);
    Some(match input["hook_event_name"].as_str()? {
        "SessionStart" => AgentEvent::Started,
        // Assumed once its turn takes the prompt, as Codex's (not checked: no login).
        "BeforeAgent" => AgentEvent::Submitted { at_enter: false },
        // After a permission the person granted, the tool's end says it works again.
        "AfterTool" => AgentEvent::Working,
        "AfterAgent" => AgentEvent::Stopped {
            message: text("prompt_response"),
            transcript: text("transcript_path"),
        },
        // Its permission dialog opened. The hook prints nothing, so the dialog still decides.
        "Notification" if input["notification_type"] == "ToolPermission" => AgentEvent::NeedsYou {
            message: text("message"),
        },
        "SessionEnd" => AgentEvent::Ended,
        _ => return None,
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
            Some(AgentEvent::Submitted { at_enter: true })
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
            Some(AgentEvent::Stopped {
                message: None,
                transcript: None
            }),
            "Esc during a tool ends the turn"
        );
        assert_eq!(
            event(json!({ "hook_event_name": "Stop", "last_assistant_message": "Done." })),
            Some(AgentEvent::Stopped {
                message: Some("Done.".into()),
                transcript: None,
            })
        );
        let path = "/home/u/.claude/projects/p/727bd1ac.jsonl";
        assert_eq!(
            event(
                json!({ "hook_event_name": "Stop", "last_assistant_message": "Done.",
                "transcript_path": path })
            ),
            Some(AgentEvent::Stopped {
                message: Some("Done.".into()),
                transcript: Some(path.into())
            }),
            "the notice of a task's end names it"
        );
        assert_eq!(
            event(
                json!({ "hook_event_name": "PostToolUseFailure", "is_interrupt": true,
                "transcript_path": path })
            ),
            Some(AgentEvent::Stopped {
                message: None,
                transcript: Some(path.into())
            })
        );
        assert_eq!(
            event(json!({ "hook_event_name": "Stop" })),
            Some(AgentEvent::Stopped {
                message: None,
                transcript: None
            }),
            "a Claude Code that doesn't say its answer"
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "permission_prompt",
                "message": "Claude needs your permission to use Bash",
            })),
            Some(AgentEvent::NeedsYou { message: None }),
            "the dialog PermissionRequest already said: what it asks stays"
        );
        for kind in ["elicitation_dialog", "agent_needs_input"] {
            assert_eq!(
                event(json!({
                    "hook_event_name": "Notification",
                    "notification_type": kind,
                    "message": "A server asks for your input",
                })),
                Some(AgentEvent::NeedsYou {
                    message: Some("A server asks for your input".into())
                }),
                "{kind} says what it asks"
            );
        }
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "auth_success",
                "message": "Signed in",
            })),
            None,
            "other notifications say nothing of the agent's state"
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
        for (tool, message) in [
            ("AskUserQuestion", "Claude asks you a question"),
            ("ExitPlanMode", "Claude has a plan for you"),
        ] {
            assert_eq!(
                event(json!({ "hook_event_name": "PermissionRequest", "tool_name": tool })),
                Some(AgentEvent::NeedsYou {
                    message: Some(message.into())
                }),
                "{tool} is Claude's own dialog, not a tool to allow"
            );
        }
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
            Some(AgentEvent::Stopped {
                message: None,
                transcript: None
            }),
            "an idle prompt means not working, never a wait for the person"
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "StopFailure",
                "error_type": "rate_limit",
                "error_message": "Rate limited",
            })),
            Some(AgentEvent::Stopped {
                message: None,
                transcript: None
            }),
            "a turn an API error ended"
        );
        assert_eq!(
            event(json!({ "hook_event_name": "SessionEnd" })),
            Some(AgentEvent::Ended)
        );
        assert_eq!(event(json!({ "hook_event_name": "PreCompact" })), None);
        assert_eq!(event(json!("garbage")), None);
    }

    /// As Codex 0.154 hands them over on stdin (docs/plans/2026-10-16-connections-batch-7.md, S1).
    #[test]
    fn codex_hooks_become_events() {
        let event = |input: Value| codex(&input);
        let turn = "01a14375-fd15-7fb2-b94b-e4a7c7102723";
        let rollout =
            "/home/u/.codex/sessions/2026/10/16/rollout-2026-10-16T03-44-39-01a14374.jsonl";
        let start = json!({ "hook_event_name": "SessionStart", "source": "startup",
            "session_id": "01a14374", "model": "gpt-5.6-luna", "permission_mode": "default" });
        assert_eq!(event(start), Some(AgentEvent::Started));
        assert_eq!(
            event(json!({ "hook_event_name": "SessionStart", "source": "compact" })),
            None
        );
        assert_eq!(
            event(
                json!({ "hook_event_name": "UserPromptSubmit", "turn_id": turn, "prompt": "also say B" })
            ),
            Some(AgentEvent::Submitted { at_enter: false }),
            "Codex says so once a turn takes the prompt in"
        );
        assert_eq!(
            event(
                json!({ "hook_event_name": "PermissionRequest", "turn_id": turn, "tool_name": "Bash",
                "tool_input": { "command": "touch f.txt", "description": "May I create f.txt?" } })
            ),
            Some(AgentEvent::NeedsYou {
                message: Some("Allow Bash?".into())
            })
        );
        assert_eq!(
            event(
                json!({ "hook_event_name": "PostToolUse", "turn_id": turn, "tool_name": "Bash",
                "tool_input": { "command": "sleep 12" }, "tool_use_id": "exec-bb2a" })
            ),
            Some(AgentEvent::Working)
        );
        assert_eq!(
            event(
                json!({ "hook_event_name": "Stop", "turn_id": turn, "stop_hook_active": false,
                "last_assistant_message": "A B", "transcript_path": rollout })
            ),
            Some(AgentEvent::Stopped {
                message: Some("A B".into()),
                transcript: Some(rollout.into()),
            })
        );
        assert_eq!(
            event(json!({ "hook_event_name": "Interrupt", "turn_id": turn,
                "transcript_path": rollout })),
            Some(AgentEvent::Stopped {
                message: None,
                transcript: Some(rollout.into())
            }),
            "Esc or Ctrl+C: no Stop comes"
        );
        // Also after 30 idle minutes: a snapshot tells when Codex quits.
        assert_eq!(event(json!({ "hook_event_name": "SessionEnd" })), None);
        let notify = json!({ "type": "agent-turn-complete", "last-assistant-message": "x" });
        assert_eq!(event(notify), None, "notify's argument is no hook");
    }

    #[test]
    fn gemini_clis_hooks_become_events_as_claude_codes() {
        let event = |input: Value| gemini(&input);
        for source in ["startup", "resume", "clear"] {
            assert_eq!(
                event(json!({ "hook_event_name": "SessionStart", "source": source })),
                Some(AgentEvent::Started),
                "{source}"
            );
        }
        assert_eq!(
            event(json!({ "hook_event_name": "BeforeAgent", "prompt": "hi" })),
            Some(AgentEvent::Submitted { at_enter: false })
        );
        // After a permission the person granted, the tool's end says it works again.
        assert_eq!(
            event(json!({ "hook_event_name": "AfterTool", "tool_name": "run_shell_command" })),
            Some(AgentEvent::Working)
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "AfterAgent",
                "prompt": "hi",
                "prompt_response": "Done.",
                "transcript_path": "/tmp/g.json",
            })),
            Some(AgentEvent::Stopped {
                message: Some("Done.".into()),
                transcript: Some("/tmp/g.json".into()),
            })
        );
        assert_eq!(
            event(json!({ "hook_event_name": "AfterAgent" })),
            Some(AgentEvent::Stopped {
                message: None,
                transcript: None
            })
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "ToolPermission",
                "message": "Tool Shell requires execution",
                "details": {},
            })),
            Some(AgentEvent::NeedsYou {
                message: Some("Tool Shell requires execution".into())
            }),
            "waiting for you as soon as the dialog opens"
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "Other",
                "message": "x",
            })),
            None,
            "other notifications say nothing of the agent's state"
        );
        assert_eq!(
            event(json!({ "hook_event_name": "SessionEnd", "reason": "exit" })),
            Some(AgentEvent::Ended)
        );
        for other in ["BeforeTool", "BeforeModel", "AfterModel", "PreCompress"] {
            assert_eq!(event(json!({ "hook_event_name": other })), None, "{other}");
        }
        assert_eq!(event(json!("garbage")), None);
    }

    #[test]
    fn events_go_over_the_bus_as_tagged_json() {
        assert_eq!(
            serde_json::to_value(AgentEvent::Stopped {
                message: Some("x".into()),
                transcript: None
            })
            .unwrap(),
            json!({ "event": "stopped", "message": "x" }),
            "no transcript, no key: the event an older Templum reads"
        );
        assert_eq!(
            serde_json::from_value::<AgentEvent>(
                json!({ "event": "stopped", "message": "x", "transcript": "/t" })
            )
            .unwrap(),
            AgentEvent::Stopped {
                message: Some("x".into()),
                transcript: Some("/t".into())
            }
        );
        assert_eq!(
            serde_json::from_value::<AgentEvent>(json!({ "event": "needs_you", "message": null }))
                .unwrap(),
            AgentEvent::NeedsYou { message: None }
        );
        assert_eq!(
            serde_json::from_value::<AgentEvent>(json!({ "event": "submitted" })).unwrap(),
            AgentEvent::Submitted { at_enter: false }
        );
    }
}
