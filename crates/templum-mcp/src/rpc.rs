//! JSON-RPC 2.0, one message per line: what the bus reads and writes.

use serde_json::{Value, json};

pub const PARSE: i32 = -32700;
pub const INVALID: i32 = -32600;
pub const METHOD: i32 = -32601;
pub const PARAMS: i32 = -32602;

/// A line read from a client.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    /// Wants an answer with its `id`.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// An answer to a request of ours: Templum sends none, so it is ignored.
    Response,
}

/// Reads one line. Err is the error line to answer it with.
pub fn parse(line: &str) -> Result<Incoming, String> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Err(error(&Value::Null, PARSE, "Parse error"));
    };
    let Some(object) = value.as_object() else {
        return Err(error(&Value::Null, INVALID, "Invalid Request"));
    };
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    let id = object.get("id").cloned();
    match (object.get("method").and_then(Value::as_str), id) {
        (Some(method), Some(id)) => Ok(Incoming::Request {
            id,
            method: method.to_string(),
            params,
        }),
        (Some(method), None) => Ok(Incoming::Notification {
            method: method.to_string(),
            params,
        }),
        (None, _) if object.contains_key("result") || object.contains_key("error") => {
            Ok(Incoming::Response)
        }
        (None, id) => Err(error(
            &id.unwrap_or(Value::Null),
            INVALID,
            "Invalid Request",
        )),
    }
}

/// A request's answer, as a line.
pub fn result(id: &Value, result: Value) -> String {
    line(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// A request's error, as a line.
pub fn error(id: &Value, code: i32, message: &str) -> String {
    line(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }))
}

/// A message that wants no answer, as a line.
pub fn notification(method: &str, params: Value) -> String {
    line(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
}

/// A message that wants an answer with `id`, as a line.
pub fn request(id: u64, method: &str, params: Value) -> String {
    line(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
}

/// `value` on one line: serde_json escapes the newlines inside strings.
fn line(value: &Value) -> String {
    let mut text = value.to_string();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_notifications_and_responses() {
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#),
            Ok(Incoming::Request {
                id: json!(1),
                method: "ping".into(),
                params: Value::Null
            })
        );
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            Ok(Incoming::Notification {
                method: "notifications/initialized".into(),
                params: Value::Null
            })
        );
        assert_eq!(
            parse(r#"{"jsonrpc":"2.0","id":"a","result":{}}"#),
            Ok(Incoming::Response)
        );
    }

    #[test]
    fn a_bad_line_is_answered_with_its_error() {
        let answer: Value = serde_json::from_str(&parse("{not json").unwrap_err()).unwrap();
        assert_eq!(
            (answer["error"]["code"].as_i64(), &answer["id"]),
            (Some(PARSE.into()), &Value::Null)
        );
        let answer: Value = serde_json::from_str(&parse(r#"{"id":7}"#).unwrap_err()).unwrap();
        assert_eq!(
            (answer["error"]["code"].as_i64(), answer["id"].as_i64()),
            (Some(INVALID.into()), Some(7))
        );
        assert!(parse("[1,2]").is_err(), "MCP has no batches");
    }

    #[test]
    fn every_line_written_is_one_line() {
        let text = result(&json!(1), json!({ "text": "a\nb" }));
        assert!(text.ends_with('\n'));
        assert_eq!(text.matches('\n').count(), 1);
        assert_eq!(error(&json!(2), METHOD, "no").matches('\n').count(), 1);
    }
}
