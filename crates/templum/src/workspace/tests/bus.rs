//! The bus: agents and commands asking the workspace (docs/specs/connections.md).

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use templum_mcp::{Hello, Incoming, Message};

use super::*;

/// The workspace's answer to `method`, asked as the terminal `terminal` would.
fn ask(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    terminal: &str,
    method: &str,
    params: Value,
) -> Value {
    let (reply, answers) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: terminal.into(),
            client: "cli".into(),
        },
        incoming: Incoming::Request {
            id: json!(1),
            method: method.into(),
            params,
        },
        reply,
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.serve(message, window, cx))
    })
    .unwrap();
    serde_json::from_str(&answers.try_recv().expect("answered at once")).unwrap()
}

/// A tool's answer: whether it is an error, and its text.
fn call(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    terminal: &str,
    tool: &str,
    arguments: Value,
) -> (bool, String) {
    let answer = ask(
        handle,
        ws,
        cx,
        terminal,
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
    );
    let result = &answer["result"];
    (
        result["isError"] == true,
        result["content"][0]["text"].as_str().unwrap().to_string(),
    )
}

/// The terminal `id`'s `TEMPLUM_TERMINAL` and block id.
fn me(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> (String, String) {
    ws.read_with(cx, |ws, _| {
        (
            ws.space.terminal(id).unwrap().term.id().to_string(),
            ws.space.uids[&id].clone(),
        )
    })
}

/// The terminal's screen once it shows `needle`.
fn screen_with(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    needle: &str,
) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = templum_terminal::Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains(needle) {
            return shown;
        }
        assert!(Instant::now() < deadline, "no {needle:?} in {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn whoami_names_the_callers_block_and_workspace(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, uid) = me(&ws, cx, first);
    let (failed, text) = call(handle, &ws, cx, &terminal, "whoami", json!({}));
    assert!(!failed, "{text}");
    assert!(text.contains(&format!("id: {uid}")), "{text}");
    assert!(
        text.contains("kind: terminal") && text.contains("workspace: Main"),
        "{text}"
    );
    let (failed, text) = call(handle, &ws, cx, "0-0", "whoami", json!({}));
    assert!(failed && text.contains("isn't a block"), "{text}");
}

#[gpui_kit::test]
fn initialize_lists_the_tools_and_says_who_the_agent_is(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, uid) = me(&ws, cx, first);
    let init = ask(
        handle,
        &ws,
        cx,
        &terminal,
        "initialize",
        json!({ "protocolVersion": "2025-06-18" }),
    );
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(
        instructions.contains(&format!("You are block {uid}")),
        "{instructions}"
    );
    let tools = ask(handle, &ws, cx, &terminal, "tools/list", Value::Null);
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 5);
    assert_eq!(
        ask(handle, &ws, cx, &terminal, "ping", Value::Null)["result"],
        json!({})
    );
    let unknown = ask(handle, &ws, cx, &terminal, "resources/list", Value::Null);
    assert_eq!(unknown["error"]["code"], templum_mcp::rpc::METHOD);
    let unknown = ask(
        handle,
        &ws,
        cx,
        &terminal,
        "tools/call",
        json!({ "name": "rm_rf" }),
    );
    assert_eq!(unknown["error"]["code"], templum_mcp::rpc::PARAMS);
    let (failed, text) = call(handle, &ws, cx, &terminal, "read_terminal", json!({}));
    assert!(failed && text.contains("\"block\""), "{text}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_terminal_types_into_itself_and_reads_it_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, uid) = me(&ws, cx, first);
    let typed = json!({ "block": uid, "text": "hello there", "enter": true });
    let (failed, text) = call(handle, &ws, cx, &terminal, "send_to_terminal", typed);
    assert!(!failed, "{text}");
    screen_with(&ws, cx, first, "hello there");
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "read_terminal",
        json!({ "block": uid, "lines": 5 }),
    );
    assert!(!failed && text.contains("hello there"), "{text}");
}

#[gpui_kit::test]
fn another_block_is_refused_until_it_is_connected(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // `three_blocks` puts the note on top: the terminal is the one it returns.
    let (kanban, _, block) = three_blocks(handle, &ws, cx);
    let (terminal, uid) = me(&ws, cx, block);
    let kanban = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "read_terminal",
        json!({ "block": kanban }),
    );
    assert!(failed && text.contains("isn't connected to you"), "{text}");
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "read_terminal",
        json!({ "block": "zzzzz" }),
    );
    assert!(failed && text.contains("no block zzzzz"), "{text}");
    let (_, list) = call(handle, &ws, cx, &terminal, "list_blocks", json!({}));
    let line = |id: &str| {
        list.lines()
            .find(|l| l.starts_with(id))
            .unwrap_or_default()
            .to_string()
    };
    assert!(line(&uid).ends_with("you"), "{list}");
    assert!(
        line(&kanban).contains("kanban") && line(&kanban).ends_with("not connected"),
        "{list}"
    );
}

#[gpui_kit::test]
fn the_bus_answers_over_its_socket(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let first = first_id(&ws, cx);
    let (terminal, uid) = me(&ws, cx, first);
    let env = ws.read_with(cx, |ws, _| {
        ws.bus
            .as_ref()
            .expect("a bus with a data folder")
            .env
            .clone()
    });
    let (tx, answers) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::{BufRead as _, Write as _};
        let (mut stream, token) = templum_mcp::bus::connect(&env).unwrap();
        let hello = templum_mcp::bus::hello_line(&terminal, "cli", token.as_deref());
        let call = templum_mcp::rpc::request(
            1,
            "tools/call",
            json!({ "name": "whoami", "arguments": {} }),
        );
        stream
            .write_all(format!("{hello}{call}").as_bytes())
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(stream)
            .read_line(&mut line)
            .unwrap();
        let _ = tx.send(line);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let line = loop {
        cx.run_until_parked();
        if let Ok(line) = answers.try_recv() {
            break line;
        }
        assert!(Instant::now() < deadline, "no answer");
        std::thread::sleep(Duration::from_millis(20));
    };
    let answer: Value = serde_json::from_str(&line).unwrap();
    let text = answer["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains(&format!("id: {uid}")), "{text}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn terminals_get_the_bus_and_this_templum_first_on_their_path(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let script = "printf 'bus=%s;\\n' \"$TEMPLUM_BUS\"; command -v templum; exec cat";
    let command = ("sh".to_string(), vec!["-c".to_string(), script.to_string()]);
    let (_, ws) = open_as(cx, Some(command), None, Some(data.path().to_path_buf()));
    let env = ws.read_with(cx, |ws, _| ws.bus.as_ref().unwrap().env.clone());
    let first = first_id(&ws, cx);
    let shown = screen_with(&ws, cx, first, "bin/templum");
    assert!(shown.contains(&format!("bus={env};")), "{shown}");
    let link = data.path().join("bin").join("templum");
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        crate::workspace::usage::program().unwrap()
    );
}

#[gpui_kit::test]
fn without_a_data_folder_there_is_no_bus(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.read_with(cx, |ws, _| assert!(ws.bus.is_none()));
}
