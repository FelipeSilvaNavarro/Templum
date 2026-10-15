//! The `templum` command line (docs/specs/connections.md, The templum command line): the MCP
//! bridge, the agents' hooks, and a verb per tool.

use std::io::{self, BufRead as _, BufReader, Read, Write};

use serde_json::{Value, json};

use crate::bus::{self, Stream};
use crate::{hooks, rpc};

/// The first arguments that make `templum` a command instead of the app.
const VERBS: &[&str] = &[
    "mcp", "hook", "whoami", "blocks", "read", "send", "notify", "ask", "connect", "delegate",
    "reply", "wait", "spawn", "kanban", "note", "open", "git",
];
const READ: &str = "read <block> [-n <lines>] [--last] [--json]";
const ASK: &str = "ask <question> [--option <answer>]… [--json]";
const WAIT: &str = "wait <block> [--timeout <seconds>] [--json]";
const SPAWN: &str =
    "spawn <agent> [--folder <folder>] [--worktree <name>] [--prompt <text>] [--json]";
const KANBAN: &str = "kanban list [<board>] [--card <card>] | add <title> [--column <column>] \
                      [--description <text>] [--board <board>] | move <card> <column> | comment <card> <text> \
                      | claim <card> [--json]";
const NOTE: &str = "note read <block> | note append <block> <text> [--json]";

pub fn is_verb(arg: &str) -> bool {
    VERBS.contains(&arg)
}

/// Where a command runs: its terminal's `TEMPLUM_BUS` and `TEMPLUM_TERMINAL`.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub bus: Option<String>,
    pub terminal: Option<String>,
}

impl Env {
    pub fn of_process() -> Env {
        Env {
            bus: std::env::var("TEMPLUM_BUS").ok(),
            terminal: std::env::var("TEMPLUM_TERMINAL").ok(),
        }
    }
}

/// Runs the command `args` (the verb first) and returns its exit code: 0 done, 1 refused or
/// failed, 2 a usage error, outside Templum, or Templum gone. A hook always returns 0 and prints
/// nothing: an agent mustn't stop over Templum, and Claude Code reads a hook's output.
pub fn run(
    args: &[String],
    env: &Env,
    mut stdin: Box<dyn Read + Send>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let (verb, rest) = match args.split_first() {
        Some((verb, rest)) => (verb.as_str(), rest),
        None => ("", &[][..]),
    };
    if verb == "hook" {
        if let Some(event) = hook_event(rest, &mut *stdin) {
            let _ = send_event(env, &event);
        }
        return 0;
    }
    let call = if verb == "mcp" {
        None
    } else {
        match call_of(verb, rest) {
            Ok(call) => Some(call),
            Err(usage) => {
                let _ = writeln!(stderr, "usage: templum {usage}");
                return 2;
            }
        }
    };
    let client = if call.is_some() { "cli" } else { "mcp" };
    let stream = match open(env, client) {
        Ok(stream) => stream,
        Err(why) => {
            let _ = writeln!(stderr, "templum: {why}");
            return 2;
        }
    };
    match call {
        None => bridge(stdin, stdout, stream),
        Some(call) => one_call(stream, &call, stdout, stderr),
    }
}

/// A verb's tool call, and whether it prints the whole result.
struct CallArgs {
    name: &'static str,
    arguments: Value,
    json: bool,
}

fn call_of(verb: &str, args: &[String]) -> Result<CallArgs, &'static str> {
    let json = args.iter().any(|a| a == "--json");
    let args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| *a != "--json")
        .collect();
    let (name, arguments) = match (verb, args.as_slice()) {
        ("whoami", []) => ("whoami", json!({})),
        ("blocks", []) => ("list_blocks", json!({})),
        ("read", [block, more @ ..]) => {
            let mut arguments = json!({ "block": block });
            let mut more = more.iter();
            while let Some(arg) = more.next() {
                match *arg {
                    "--last" => arguments["last_command"] = json!(true),
                    "-n" => {
                        let n: u64 = more.next().and_then(|n| n.parse().ok()).ok_or(READ)?;
                        arguments["lines"] = json!(n);
                    }
                    _ => return Err(READ),
                }
            }
            ("read_terminal", arguments)
        }
        ("send", [block, text @ ..]) if !text.is_empty() => {
            let enter = text.contains(&"--enter");
            let text: Vec<&str> = text.iter().copied().filter(|a| *a != "--enter").collect();
            (
                "send_to_terminal",
                json!({ "block": block, "text": text.join(" "), "enter": enter }),
            )
        }
        ("notify", text) if !text.is_empty() => ("notify", json!({ "text": text.join(" ") })),
        ("ask", words) => {
            let (mut question, mut options) = (Vec::new(), Vec::new());
            let mut words = words.iter();
            while let Some(word) = words.next() {
                if *word == "--option" {
                    options.push(*words.next().ok_or(ASK)?);
                } else {
                    question.push(*word);
                }
            }
            if question.is_empty() {
                return Err(ASK);
            }
            let mut arguments = json!({ "question": question.join(" ") });
            if !options.is_empty() {
                arguments["options"] = json!(options);
            }
            ("ask_user", arguments)
        }
        ("connect", [block]) => ("connect", json!({ "block": block })),
        ("delegate", [block, task @ ..]) if task.iter().any(|a| *a != "--wait") => {
            let wait = task.contains(&"--wait");
            let task: Vec<&str> = task.iter().copied().filter(|a| *a != "--wait").collect();
            (
                "delegate",
                json!({ "block": block, "task": task.join(" "), "wait": wait }),
            )
        }
        ("reply", [block, text @ ..]) if !text.is_empty() => {
            ("reply", json!({ "block": block, "text": text.join(" ") }))
        }
        ("wait", [block]) => ("wait_for_idle", json!({ "block": block })),
        ("wait", [block, "--timeout", seconds]) => {
            let seconds: u64 = seconds.parse().map_err(|_| WAIT)?;
            (
                "wait_for_idle",
                json!({ "block": block, "timeout": seconds }),
            )
        }
        ("spawn", [agent, more @ ..]) => {
            let mut arguments = json!({ "agent": agent });
            let mut more = more.iter();
            while let Some(flag) = more.next() {
                let key = match *flag {
                    "--folder" => "folder",
                    "--worktree" => "worktree",
                    "--prompt" => "prompt",
                    _ => return Err(SPAWN),
                };
                arguments[key] = json!(more.next().ok_or(SPAWN)?);
            }
            ("spawn_agent", arguments)
        }
        ("kanban", ["list", more @ ..]) => {
            let mut arguments = json!({});
            let mut more = more.iter();
            while let Some(arg) = more.next() {
                match *arg {
                    "--card" => arguments["card"] = json!(more.next().ok_or(KANBAN)?),
                    board if !board.starts_with("--") && arguments.get("board").is_none() => {
                        arguments["board"] = json!(board)
                    }
                    _ => return Err(KANBAN),
                }
            }
            ("list_cards", arguments)
        }
        ("kanban", ["add", more @ ..]) => {
            let (mut title, mut arguments) = (Vec::new(), json!({}));
            let mut more = more.iter();
            while let Some(word) = more.next() {
                let key = match *word {
                    "--column" => "column",
                    "--description" => "description",
                    "--board" => "board",
                    word => {
                        title.push(word);
                        continue;
                    }
                };
                arguments[key] = json!(more.next().ok_or(KANBAN)?);
            }
            if title.is_empty() {
                return Err(KANBAN);
            }
            arguments["title"] = json!(title.join(" "));
            ("create_card", arguments)
        }
        ("kanban", ["move", card, column @ ..]) if !column.is_empty() => (
            "move_card",
            json!({ "card": card, "column": column.join(" ") }),
        ),
        ("kanban", ["comment", card, text @ ..]) if !text.is_empty() => (
            "comment_card",
            json!({ "card": card, "text": text.join(" ") }),
        ),
        ("kanban", ["claim", card]) => ("claim_card", json!({ "card": card })),
        ("note", ["read", block]) => ("read_note", json!({ "block": block })),
        ("note", ["append", block, text @ ..]) if !text.is_empty() => (
            "append_note",
            json!({ "block": block, "text": text.join(" ") }),
        ),
        ("open", [target]) => {
            let mut arguments = json!({});
            match target.rsplit_once(':') {
                Some((path, line)) if !path.is_empty() && line.parse::<u64>().is_ok() => {
                    arguments["path"] = json!(path);
                    arguments["line"] = json!(line.parse::<u64>().unwrap_or(1));
                }
                _ => arguments["path"] = json!(target),
            }
            ("open_file", arguments)
        }
        ("git", ["status"]) => ("git_status", json!({})),
        ("git", ["status", block]) => ("git_status", json!({ "block": block })),
        ("whoami", _) => return Err("whoami [--json]"),
        ("blocks", _) => return Err("blocks [--json]"),
        ("read", _) => return Err(READ),
        ("send", _) => return Err("send <block> <text> [--enter] [--json]"),
        ("connect", _) => return Err("connect <block> [--json]"),
        ("spawn", _) => return Err(SPAWN),
        ("delegate", _) => return Err("delegate <block> <task> [--wait] [--json]"),
        ("reply", _) => return Err("reply <block> <text> [--json]"),
        ("wait", _) => return Err(WAIT),
        ("kanban", _) => return Err(KANBAN),
        ("note", _) => return Err(NOTE),
        ("open", _) => return Err("open <path>[:<line>] [--json]"),
        ("git", _) => return Err("git status [<block>] [--json]"),
        _ => return Err("notify <text> [--json]"),
    };
    Ok(CallArgs {
        name,
        arguments,
        json,
    })
}

/// Connects to the bus of the terminal this runs in and says hello.
fn open(env: &Env, client: &str) -> Result<Stream, &'static str> {
    let (Some(address), Some(terminal)) = (&env.bus, &env.terminal) else {
        return Err("not inside a Templum terminal");
    };
    let gone = "Templum isn't running";
    let (mut stream, token) = bus::connect(address).map_err(|_| gone)?;
    stream
        .write_all(bus::hello_line(terminal, client, token.as_deref()).as_bytes())
        .map_err(|_| gone)?;
    Ok(stream)
}

/// Copies the MCP client's lines (stdin) to the bus and the bus's back (stdout) until the bus
/// closes: once the client is done writing and answered, or when Templum is gone.
fn bridge(mut stdin: Box<dyn Read + Send>, stdout: &mut dyn Write, stream: Stream) -> i32 {
    let Ok(mut to_bus) = stream.try_clone() else {
        return 2;
    };
    std::thread::spawn(move || {
        let _ = io::copy(&mut stdin, &mut to_bus);
        let _ = to_bus.finish();
    });
    let mut from_bus = stream;
    let _ = io::copy(&mut from_bus, stdout);
    let _ = stdout.flush();
    0
}

/// Sends one tool call and prints its answer: its text, or with `--json` the whole result. It
/// stays connected until the answer: its end tells Templum it is gone (Ctrl+C), and what it
/// waited for is withdrawn.
fn one_call(
    mut stream: Stream,
    call: &CallArgs,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let request = rpc::request(
        1,
        "tools/call",
        json!({ "name": call.name, "arguments": call.arguments }),
    );
    if stream.write_all(request.as_bytes()).is_err() {
        let _ = writeln!(stderr, "templum: Templum isn't running");
        return 2;
    }
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let Ok(answer) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if answer["id"] != 1 {
            continue;
        }
        if let Some(message) = answer["error"]["message"].as_str() {
            let _ = writeln!(stderr, "templum: {message}");
            return 1;
        }
        let result = &answer["result"];
        let failed = result["isError"] == true;
        let text = if call.json {
            result.to_string()
        } else {
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        };
        let out: &mut dyn Write = if failed { stderr } else { stdout };
        let _ = write!(out, "{text}");
        if !text.ends_with('\n') {
            let _ = writeln!(out);
        }
        return i32::from(failed);
    }
    let _ = writeln!(stderr, "templum: Templum closed the connection");
    2
}

/// The event an agent's hook hands over: Claude Code's on stdin, Codex's as an argument.
fn hook_event(args: &[String], stdin: &mut dyn Read) -> Option<hooks::AgentEvent> {
    match args.first()?.as_str() {
        "claude" => {
            let mut input = String::new();
            stdin.read_to_string(&mut input).ok()?;
            hooks::claude(&serde_json::from_str(&input).ok()?)
        }
        "codex" => hooks::codex(&serde_json::from_str(args.get(1)?).ok()?),
        _ => None,
    }
}

/// Tells the bus about an agent's turn; nothing when Templum isn't there.
fn send_event(env: &Env, event: &hooks::AgentEvent) -> Option<()> {
    let mut stream = open(env, "hook").ok()?;
    let line = rpc::notification(hooks::METHOD, serde_json::to_value(event).ok()?);
    stream.write_all(line.as_bytes()).ok()?;
    stream.finish().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{Bus, test_bus};
    use crate::rpc::Incoming;
    use crate::tools::{Reply, tool_result};

    fn env(bus: &Bus) -> Env {
        Env {
            bus: Some(bus.env.clone()),
            terminal: Some("123-4".into()),
        }
    }

    /// A wait longer than this fails the test instead of hanging it.
    const LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

    fn run_with(args: &[&str], env: &Env, stdin: &str) -> (i32, String, String) {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let (env, stdin) = (env.clone(), stdin.as_bytes().to_vec());
        let (done, ran) = std::sync::mpsc::channel();
        // On a thread of its own: a command that never ends can't hang the test.
        std::thread::spawn(move || {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let code = run(
                &args,
                &env,
                Box::new(io::Cursor::new(stdin)),
                &mut out,
                &mut err,
            );
            let _ = done.send((
                code,
                String::from_utf8(out).unwrap(),
                String::from_utf8(err).unwrap(),
            ));
        });
        ran.recv_timeout(LIMIT).expect("the command ended")
    }

    #[test]
    fn the_bridge_says_hello_then_copies_both_ways() {
        let (_dir, bus) = test_bus(|m| {
            if let Incoming::Request { id, .. } = &m.incoming {
                let hello = json!({ "terminal": m.hello.terminal, "client": m.hello.client });
                let _ = m.reply.send(rpc::result(id, hello));
            }
        });
        let (code, out, _) = run_with(&["mcp"], &env(&bus), &rpc::request(7, "ping", Value::Null));
        assert_eq!(code, 0);
        let answer: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(answer["id"], 7);
        assert_eq!(
            answer["result"],
            json!({ "terminal": "123-4", "client": "mcp" })
        );
    }

    #[test]
    fn a_verb_is_one_tool_call_and_prints_its_text() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let reply = if params["arguments"]["block"] == "nope" {
                    Reply::err("nope isn't connected to you")
                } else {
                    Reply::ok("line 1\nline 2")
                };
                let _ = m.reply.send(tool_result(id, &reply));
            }
        });
        let (code, out, _) = run_with(&["read", "b7k2m", "-n", "10"], &env(&bus), "");
        assert_eq!((code, out.as_str()), (0, "line 1\nline 2\n"));
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap(),
            json!({ "name": "read_terminal", "arguments": { "block": "b7k2m", "lines": 10 } })
        );
        let (code, out, err) = run_with(&["read", "nope"], &env(&bus), "");
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (1, "", "nope isn't connected to you\n")
        );
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap()["arguments"],
            json!({ "block": "nope" })
        );
        run_with(
            &["send", "b7k2m", "make", "test", "--enter"],
            &env(&bus),
            "",
        );
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap()["arguments"],
            json!({ "block": "b7k2m", "text": "make test", "enter": true })
        );
        let (code, out, _) = run_with(&["blocks", "--json"], &env(&bus), "");
        assert_eq!(code, 0);
        assert_eq!(
            serde_json::from_str::<Value>(&out).unwrap()["content"][0]["text"],
            "line 1\nline 2"
        );
    }

    /// A command stays connected until its answer comes: Templum hears it end only then. One
    /// stopped with Ctrl+C ends it early, and what it waited for is withdrawn.
    #[test]
    fn a_command_stays_connected_until_its_answer() {
        let (seen, order) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| match &m.incoming {
            Incoming::Request { id, .. } => {
                let (id, reply, seen) = (id.clone(), m.reply.clone(), seen.clone());
                let _ = seen.send("asked");
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let _ = seen.send("answered");
                    let _ = reply.send(tool_result(&id, &Reply::ok("Yes")));
                });
            }
            Incoming::Notification { method, .. } if method == crate::bus::CLOSED => {
                let _ = seen.send("closed");
            }
            _ => {}
        });
        assert_eq!(run_with(&["whoami"], &env(&bus), "").1, "Yes\n");
        let order: Vec<&str> = (0..3).map(|_| order.recv_timeout(LIMIT).unwrap()).collect();
        assert_eq!(order, ["asked", "answered", "closed"]);
    }

    #[test]
    fn ask_sends_the_question_and_its_options() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let _ = m.reply.send(tool_result(id, &Reply::ok("Yes")));
            }
        });
        let args = [
            "ask", "Ship", "it?", "--option", "Yes", "--option", "Not yet",
        ];
        let (code, out, _) = run_with(&args, &env(&bus), "");
        assert_eq!((code, out.as_str()), (0, "Yes\n"));
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap(),
            json!({ "name": "ask_user", "arguments": { "question": "Ship it?", "options": ["Yes", "Not yet"] } })
        );
        run_with(&["ask", "When?"], &env(&bus), "");
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap()["arguments"],
            json!({ "question": "When?" })
        );
        run_with(&["connect", "k9x1d"], &env(&bus), "");
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap(),
            json!({ "name": "connect", "arguments": { "block": "k9x1d" } })
        );
        let (code, _, err) = run_with(&["ask", "--option", "Yes"], &env(&bus), "");
        assert_eq!(code, 2);
        assert!(err.starts_with("usage: templum ask"), "{err}");
    }

    #[test]
    fn delegate_reply_and_wait_send_their_calls() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let _ = m.reply.send(tool_result(id, &Reply::ok("ok")));
            }
        });
        for (args, sent) in [
            (
                &["delegate", "k9x1d", "fix", "the", "tests", "--wait"][..],
                json!({ "name": "delegate", "arguments": { "block": "k9x1d", "task": "fix the tests", "wait": true } }),
            ),
            (
                &["reply", "b7k2m", "all", "green"][..],
                json!({ "name": "reply", "arguments": { "block": "b7k2m", "text": "all green" } }),
            ),
            (
                &["wait", "k9x1d", "--timeout", "60"][..],
                json!({ "name": "wait_for_idle", "arguments": { "block": "k9x1d", "timeout": 60 } }),
            ),
            (
                &[
                    "spawn",
                    "codex",
                    "--worktree",
                    "fix-tests",
                    "--prompt",
                    "fix the tests",
                ][..],
                json!({ "name": "spawn_agent", "arguments": { "agent": "codex", "worktree": "fix-tests", "prompt": "fix the tests" } }),
            ),
        ] {
            assert_eq!(run_with(args, &env(&bus), "").0, 0, "{args:?}");
            assert_eq!(calls.recv_timeout(LIMIT).unwrap(), sent);
        }
        assert_eq!(
            run_with(&["delegate", "k9x1d", "--wait"], &env(&bus), "").0,
            2,
            "no task"
        );
        assert_eq!(
            run_with(&["spawn", "codex", "--folder"], &env(&bus), "").0,
            2,
            "no folder"
        );
        assert_eq!(
            run_with(&["wait", "k9x1d", "--for", "idle"], &env(&bus), "").0,
            2,
            "--for is batch 5's"
        );
    }

    #[test]
    fn a_hook_sends_its_event_prints_nothing_and_never_fails() {
        let (seen, events) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            // Each hook's connection ends after its event: that isn't an event.
            if !matches!(&m.incoming, Incoming::Notification { method, .. } if method == crate::bus::CLOSED)
            {
                let _ = seen.send((m.hello.client, m.incoming));
            }
        });
        let stop = r#"{"hook_event_name":"Stop","last_assistant_message":"Done."}"#;
        assert_eq!(
            run_with(&["hook", "claude"], &env(&bus), stop),
            (0, String::new(), String::new())
        );
        assert_eq!(
            events.recv_timeout(LIMIT).unwrap(),
            (
                "hook".to_string(),
                Incoming::Notification {
                    method: "templum/event".into(),
                    params: json!({ "event": "stopped", "message": "Done." }),
                }
            )
        );
        // A permission dialog: told to Templum, and nothing printed, so the dialog still decides.
        let asked = r#"{"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{}}"#;
        assert_eq!(
            run_with(&["hook", "claude"], &env(&bus), asked),
            (0, String::new(), String::new())
        );
        assert_eq!(
            events.recv_timeout(LIMIT).unwrap().1,
            Incoming::Notification {
                method: "templum/event".into(),
                params: json!({ "event": "needs_you", "message": "Allow Bash?" }),
            }
        );
        let codex = r#"{"type":"agent-turn-complete","last-assistant-message":"Fixed."}"#;
        assert_eq!(run_with(&["hook", "codex", codex], &env(&bus), "").0, 0);
        assert_eq!(
            events.recv_timeout(LIMIT).unwrap().1,
            Incoming::Notification {
                method: "templum/event".into(),
                params: json!({ "event": "stopped", "message": "Fixed." }),
            }
        );
        // Outside Templum, and with what Templum doesn't follow: nothing, quietly.
        assert_eq!(
            run_with(&["hook", "claude"], &Env::default(), stop),
            (0, String::new(), String::new())
        );
        assert_eq!(
            run_with(&["hook", "claude"], &env(&bus), "not json"),
            (0, String::new(), String::new())
        );
    }

    #[test]
    fn outside_templum_or_with_it_gone_a_command_exits_2() {
        let (code, out, err) = run_with(&["blocks"], &Env::default(), "");
        assert_eq!((code, out.as_str()), (2, ""));
        assert!(err.contains("not inside a Templum terminal"), "{err}");
        let gone = Env {
            bus: Some(if cfg!(unix) {
                "unix:/nonexistent/templum.sock".into()
            } else {
                "tcp:127.0.0.1:9:x".into()
            }),
            terminal: Some("1-1".into()),
        };
        let (code, out, err) = run_with(&["mcp"], &gone, "");
        assert_eq!((code, out.as_str()), (2, ""));
        assert!(err.contains("Templum isn't running"), "{err}");
        let (code, _, err) = run_with(&["read"], &gone, "");
        assert_eq!(code, 2);
        assert!(err.starts_with("usage: templum read"), "{err}");
    }

    #[test]
    fn only_the_verbs_are_commands() {
        assert!(is_verb("mcp") && is_verb("hook") && is_verb("read"));
        assert!(is_verb("ask"));
        assert!(is_verb("kanban") && is_verb("note") && is_verb("open") && is_verb("git"));
        assert!(!is_verb("--bench") && !is_verb("--claude-statusline") && !is_verb(""));
    }

    #[test]
    fn the_content_verbs_send_their_calls() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let _ = m.reply.send(tool_result(id, &Reply::ok("ok")));
            }
        });
        for (args, sent) in [
            (
                &["kanban", "list"][..],
                json!({ "name": "list_cards", "arguments": {} }),
            ),
            (
                &["kanban", "list", "Sprint", "--card", "12"][..],
                json!({ "name": "list_cards", "arguments": { "board": "Sprint", "card": "12" } }),
            ),
            (
                &[
                    "kanban",
                    "add",
                    "Fix",
                    "login",
                    "--column",
                    "Doing",
                    "--description",
                    "It 500s.",
                ][..],
                json!({ "name": "create_card", "arguments": { "title": "Fix login", "column": "Doing", "description": "It 500s." } }),
            ),
            (
                &["kanban", "move", "12", "In", "review"][..],
                json!({ "name": "move_card", "arguments": { "card": "12", "column": "In review" } }),
            ),
            (
                &["kanban", "comment", "12", "On", "it."][..],
                json!({ "name": "comment_card", "arguments": { "card": "12", "text": "On it." } }),
            ),
            (
                &["kanban", "claim", "12"][..],
                json!({ "name": "claim_card", "arguments": { "card": "12" } }),
            ),
            (
                &["note", "read", "4qm2a"][..],
                json!({ "name": "read_note", "arguments": { "block": "4qm2a" } }),
            ),
            (
                &["note", "append", "4qm2a", "Next:", "tests"][..],
                json!({ "name": "append_note", "arguments": { "block": "4qm2a", "text": "Next: tests" } }),
            ),
            (
                &["open", "src/main.rs:12"][..],
                json!({ "name": "open_file", "arguments": { "path": "src/main.rs", "line": 12 } }),
            ),
            (
                &["open", "README.md"][..],
                json!({ "name": "open_file", "arguments": { "path": "README.md" } }),
            ),
            (
                &["open", r"C:\src\a.rs"][..],
                json!({ "name": "open_file", "arguments": { "path": r"C:\src\a.rs" } }),
            ),
            (
                &["git", "status"][..],
                json!({ "name": "git_status", "arguments": {} }),
            ),
            (
                &["git", "status", "g1t2k"][..],
                json!({ "name": "git_status", "arguments": { "block": "g1t2k" } }),
            ),
        ] {
            assert_eq!(run_with(args, &env(&bus), "").0, 0, "{args:?}");
            assert_eq!(calls.recv_timeout(LIMIT).unwrap(), sent, "{args:?}");
        }
        for (args, usage) in [
            (&["kanban"][..], "usage: templum kanban"),
            (
                &["kanban", "add", "--column", "Doing"][..],
                "usage: templum kanban",
            ),
            (&["kanban", "move", "12"][..], "usage: templum kanban"),
            (&["kanban", "list", "--card"][..], "usage: templum kanban"),
            (&["note", "append", "4qm2a"][..], "usage: templum note"),
            (&["open"][..], "usage: templum open"),
            (&["git", "log"][..], "usage: templum git"),
        ] {
            let (code, _, err) = run_with(args, &env(&bus), "");
            assert_eq!(code, 2, "{args:?}");
            assert!(err.starts_with(usage), "{args:?}: {err}");
        }
    }
}
