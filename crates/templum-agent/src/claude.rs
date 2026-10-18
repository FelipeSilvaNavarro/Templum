//! Claude Code as the engine (docs/specs/templum-agent.md#claude-code-batch-1): its stream of
//! JSON lines read into [`ChatEvent`]s.

use serde_json::Value;

use crate::ChatEvent;

/// The prefix Claude Code gives the control server's tools.
const PREFIX: &str = "mcp__templum-control__";

/// The events one line of Claude Code's stream gives: none for a line that isn't JSON or a kind
/// the chat doesn't use, several for an `assistant` or `user` line with several blocks.
pub fn parse(line: &str) -> Vec<ChatEvent> {
    let Ok(line) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    match line["type"].as_str() {
        Some("system") if line["subtype"] == "init" => vec![ChatEvent::Started {
            session: text(&line["session_id"]),
            model: line["model"].as_str().map(str::to_owned),
        }],
        Some("stream_event") => {
            let event = &line["event"];
            let block = &event["content_block"];
            match (event["type"].as_str(), block["type"].as_str()) {
                (Some("content_block_start"), Some("thinking")) => vec![ChatEvent::Thinking],
                (Some("content_block_start"), Some("tool_use")) => vec![ChatEvent::ToolStarted {
                    id: text(&block["id"]),
                    name: text(&block["name"]),
                }],
                (Some("content_block_delta"), _) if event["delta"]["type"] == "text_delta" => {
                    vec![ChatEvent::TextDelta(text(&event["delta"]["text"]))]
                }
                _ => Vec::new(),
            }
        }
        Some("assistant") => blocks(&line)
            .filter_map(|block| match block["type"].as_str() {
                Some("text") => Some(ChatEvent::Text(text(&block["text"]))),
                Some("tool_use") => Some(ChatEvent::ToolCall {
                    id: text(&block["id"]),
                    name: text(&block["name"]),
                    input: block["input"].clone(),
                }),
                _ => None,
            })
            .collect(),
        Some("user") => {
            let mut events: Vec<ChatEvent> = blocks(&line)
                .filter(|block| block["type"] == "tool_result")
                .map(|block| ChatEvent::ToolResult {
                    id: text(&block["tool_use_id"]),
                    text: texts(&block["content"]),
                    is_error: block["is_error"].as_bool().unwrap_or(false),
                })
                .collect();
            // Only a replay is the person's message: the interrupt writes a text line of its own.
            if line["isReplay"] == true {
                events.push(ChatEvent::Taken(texts(&line["message"]["content"])));
            }
            events
        }
        Some("result") => {
            let error = (line["is_error"] == true).then(|| {
                line["result"]
                    .as_str()
                    .or(line["subtype"].as_str())
                    .unwrap_or("error")
                    .to_owned()
            });
            vec![ChatEvent::TurnEnded {
                ms: line["duration_ms"].as_u64(),
                cost_usd: line["total_cost_usd"].as_f64(),
                error,
            }]
        }
        _ => Vec::new(),
    }
}

/// A message's content blocks.
fn blocks(line: &Value) -> impl Iterator<Item = &Value> {
    line["message"]["content"].as_array().into_iter().flatten()
}

/// A content's text: itself when it is a string, else its `text` blocks joined by new lines.
fn texts(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A tool's name without the control server's prefix: `mcp__templum-control__add_block` is
/// `add_block`; a built-in tool's name stays.
pub fn tool_name(name: &str) -> &str {
    name.strip_prefix(PREFIX).unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(fixture: &str) -> Vec<ChatEvent> {
        fixture.lines().flat_map(parse).collect()
    }

    #[test]
    fn a_text_turn_is_its_start_its_words_and_its_end() {
        let events = events(include_str!("../tests/fixtures/claude-text.jsonl"));
        assert!(matches!(&events[0], ChatEvent::Started { session, .. } if !session.is_empty()));
        let streamed: String = events
            .iter()
            .filter_map(|e| match e {
                ChatEvent::TextDelta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        let whole = events
            .iter()
            .find_map(|e| match e {
                ChatEvent::Text(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(streamed.trim(), whole.trim());
        assert!(matches!(
            events.last(),
            Some(ChatEvent::TurnEnded {
                error: None,
                cost_usd: Some(_),
                ms: Some(_)
            })
        ));
    }

    #[test]
    fn a_tool_is_started_called_and_answered_under_one_id() {
        let events = events(include_str!("../tests/fixtures/claude-tools.jsonl"));
        let started = events
            .iter()
            .position(|e| matches!(e, ChatEvent::ToolStarted { name, .. } if name == "Read"))
            .expect("the Read starts");
        let ChatEvent::ToolStarted { id, .. } = &events[started] else {
            unreachable!()
        };
        let called = events
            .iter()
            .position(|e| {
                matches!(e, ChatEvent::ToolCall { id: i, input, .. }
                if i == id && input.get("file_path").is_some())
            })
            .expect("the Read is called with its file");
        let answered = events
            .iter()
            .position(|e| {
                matches!(e, ChatEvent::ToolResult { id: i, text, is_error: false }
                if i == id && text.contains("hello-fixture"))
            })
            .expect("the Read is answered with the file");
        assert!(started < called && called < answered);
        assert!(events.contains(&ChatEvent::Thinking));
    }

    #[test]
    fn a_replayed_message_is_taken() {
        let events = events(include_str!("../tests/fixtures/claude-queued.jsonl"));
        let taken: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                ChatEvent::Taken(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            taken,
            [
                "Count slowly from 1 to 30, one number per line.",
                "Then say the word done."
            ]
        );
    }

    #[test]
    fn an_mcp_tools_answer_is_its_texts_joined() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Created workspace API (w3)."},{"type":"text","text":"It is not shown."}],"is_error":true}]}}"#;
        assert_eq!(
            parse(line),
            vec![ChatEvent::ToolResult {
                id: "t1".into(),
                text: "Created workspace API (w3).\nIt is not shown.".into(),
                is_error: true,
            }]
        );
    }

    #[test]
    fn what_isnt_a_line_it_knows_is_nothing() {
        assert!(parse("running 1 test").is_empty());
        assert!(parse("{\"type\":\"rate_limit_event\"}").is_empty());
        // The interrupt's own line isn't a replay of the person's message.
        assert!(parse(r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#).is_empty());
        assert_eq!(
            parse("{\"type\":\"result\",\"is_error\":true,\"result\":\"Overloaded\"}"),
            vec![ChatEvent::TurnEnded {
                ms: None,
                cost_usd: None,
                error: Some("Overloaded".into())
            }]
        );
        assert_eq!(tool_name("mcp__templum-control__add_block"), "add_block");
        assert_eq!(tool_name("Read"), "Read");
    }
}
