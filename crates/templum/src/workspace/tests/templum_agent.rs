//! The Templum Agent: its control server, the gate, its engine and its chat
//! (docs/specs/templum-agent.md).

use serde_json::{Value, json};
use templum_mcp::{Hello, Incoming, Message};

use super::bus::{me, screen_with};
use super::*;

/// The workspace's answer to `method`, asked by a control client whose hello carries `secret`.
pub(super) fn control_as(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    secret: &str,
    method: &str,
    params: Value,
) -> Value {
    let (reply, answers) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: String::new(),
            client: "control".into(),
            control: Some(secret.into()),
        },
        conn: 900,
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

/// A control tool's answer, asked with the workspace's secret: whether it is an error, and its
/// text.
pub(super) fn control(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    tool: &str,
    arguments: Value,
) -> (bool, String) {
    let secret = ws.read_with(cx, |ws, _| ws.templum_agent.secret.clone());
    let answer = control_as(
        handle,
        ws,
        cx,
        &secret,
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
    );
    let result = &answer["result"];
    (
        result["isError"] == true,
        result["content"][0]["text"].as_str().unwrap().to_string(),
    )
}

#[gpui_kit::test]
fn a_control_client_with_the_secret_gets_every_workspace(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let secret = ws.read_with(cx, |ws, _| ws.templum_agent.secret.clone());
    assert_eq!(secret.len(), 32);
    let init = control_as(
        handle,
        &ws,
        cx,
        &secret,
        "initialize",
        json!({"protocolVersion": "2025-11-25"}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "templum-control");
    let text = init["result"]["instructions"].as_str().unwrap();
    assert!(
        text.contains(", shown: 1 block") && text.contains("Call overview for the blocks."),
        "{text}"
    );
    let tools = control_as(handle, &ws, cx, &secret, "tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "overview"));
    assert!(!tools.iter().any(|t| t["name"] == "whoami"));
}

#[gpui_kit::test]
fn a_wrong_secret_is_not_the_templum_agent(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let init = control_as(handle, &ws, cx, "old", "initialize", json!({}));
    assert_eq!(
        init["result"]["instructions"],
        "This isn't the Templum Agent's process."
    );
    let called = control_as(
        handle,
        &ws,
        cx,
        "old",
        "tools/call",
        json!({"name": "overview", "arguments": {}}),
    );
    assert_eq!(called["result"]["isError"], true);
    assert_eq!(
        called["result"]["content"][0]["text"],
        "This isn't the Templum Agent's process."
    );
    // A block's own server never reaches the control tools.
    let first = first_id(&ws, cx);
    let terminal = me(&ws, cx, first).0;
    let answer = super::bus::ask_as(
        handle,
        &ws,
        cx,
        &terminal,
        "mcp",
        "tools/call",
        json!({"name": "overview", "arguments": {}}),
    );
    assert_eq!(answer["error"]["message"], "Unknown tool: overview");
}

#[gpui_kit::test]
fn overview_lists_the_blocks_of_every_workspace(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (first, first_uid) = ws.read_with(cx, |ws, _| {
        (
            ws.space.id.clone(),
            ws.space.uids[&ws.space.canvas.top().unwrap()].clone(),
        )
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.new_terminal(window, cx);
        })
    })
    .unwrap();
    let (second, second_uid) = ws.read_with(cx, |ws, _| {
        (
            ws.space.id.clone(),
            ws.space.uids[&ws.space.canvas.top().unwrap()].clone(),
        )
    });
    let (failed, text) = control(handle, &ws, cx, "overview", json!({}));
    assert!(!failed, "{text}");
    assert!(text.contains(&format!("({first})")), "{text}");
    assert!(text.contains(&format!("({second}), shown")), "{text}");
    assert!(text.contains(&format!("{first_uid}  terminal")), "{text}");
    assert!(text.contains(&format!("{second_uid}  terminal")), "{text}");
}

#[gpui_kit::test]
fn the_reads_need_no_wire(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, note, terminal) = three_blocks(handle, &ws, cx);
    let (note_uid, terminal_uid) = ws.read_with(cx, |ws, _| {
        (
            ws.space.uids[&note].clone(),
            ws.space.uids[&terminal].clone(),
        )
    });
    ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(terminal)
            .unwrap()
            .term
            .write(b"printed\r".to_vec())
    });
    screen_with(&ws, cx, terminal, "printed");
    assert_eq!(
        control(handle, &ws, cx, "read_note", json!({ "block": note_uid })),
        (false, "hello".to_string())
    );
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "read_terminal",
        json!({ "block": terminal_uid }),
    );
    assert!(!failed && text.contains("printed"), "{text}");
    let (failed, text) = control(handle, &ws, cx, "read_note", json!({ "block": "nope1" }));
    assert!(failed, "{text}");
    assert_eq!(text, "There's no block nope1: overview lists them.");
    let (failed, text) = control(handle, &ws, cx, "list_agents", json!({}));
    assert!(!failed && text.contains("claude"), "{text}");
}

#[gpui_kit::test]
fn cards_go_on_any_board_and_comments_are_signed_templum_agent(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let board = ws.update(cx, |ws, _| {
        let board = ws.edit_boards(crate::board::Boards::add_board);
        ws.edit_boards(|b| b.rename_board(board, "Sprint"));
        board
    });
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "create_card",
        json!({"board": "sprint", "title": "Login", "checklist": "One\nTwo"}),
    );
    assert!(!failed, "{text}");
    let card: u64 = text
        .strip_prefix("Created card ")
        .and_then(|t| t.strip_suffix('.'))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.boards.find_card(card).map(|(b, _)| b)),
        Some(board)
    );
    let last = ws.read_with(cx, |ws, _| {
        ws.boards
            .board(board)
            .unwrap()
            .columns
            .last()
            .unwrap()
            .title
            .clone()
    });
    assert_eq!(
        control(
            handle,
            &ws,
            cx,
            "move_card",
            json!({"card": card, "column": last})
        ),
        (false, format!("Moved card {card} to {last}.")),
        "no comment asked of the Templum Agent"
    );
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "comment_card",
        json!({"card": card, "text": "Done by Codex."}),
    );
    assert!(!failed, "{text}");
    let by = ws.read_with(cx, |ws, _| {
        ws.boards.card(board, card).unwrap().comments[0].by.clone()
    });
    assert_eq!(by, ::templum_agent::ASKER);
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "edit_card",
        json!({"card": card, "priority": "high"}),
    );
    assert!(!failed, "{text}");
    let (failed, whole) = control(handle, &ws, cx, "list_cards", json!({"card": card}));
    assert!(!failed, "{whole}");
    assert!(
        whole.contains("comment by Templum Agent") && whole.contains("priority high"),
        "{whole}"
    );
    let (failed, text) = control(handle, &ws, cx, "list_cards", json!({}));
    assert!(
        !failed && text.starts_with("Board \"Sprint\""),
        "the only board: {text}"
    );
    ws.update(cx, |ws, _| ws.edit_boards(crate::board::Boards::add_board));
    let (failed, text) = control(handle, &ws, cx, "list_cards", json!({}));
    assert!(failed && text.contains("Name one with board"), "{text}");
    let (failed, text) = control(handle, &ws, cx, "list_cards", json!({"board": board}));
    assert!(!failed && text.starts_with("Board \"Sprint\""), "{text}");
}

#[gpui_kit::test]
fn notes_and_checklists_take_writes_without_a_wire(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, note, _) = three_blocks(handle, &ws, cx);
    let note_uid = ws.read_with(cx, |ws, _| ws.space.uids[&note].clone());
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "append_note",
        json!({"block": note_uid, "text": "plan"}),
    );
    assert!(!failed, "{text}");
    cx.run_until_parked();
    let shown = ws.read_with(cx, |ws, cx| {
        ws.space.blocks[&note]
            .note()
            .unwrap()
            .tab()
            .unwrap()
            .editor
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(shown, "hello\n\nplan");
    cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("tool-checklist", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let list = ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        (
            ws.space.uids[&id].clone(),
            ws.space.blocks[&id].checklist().unwrap().list.unwrap(),
        )
    });
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "add_items",
        json!({"block": list.0, "text": "One\nTwo"}),
    );
    assert!(!failed, "{text}");
    let first = ws.read_with(cx, |ws, _| ws.checklists.get(list.1).unwrap().items[0].id);
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "check_item",
        json!({"block": list.0, "item": first}),
    );
    assert!(!failed, "{text}");
    let (failed, text) = control(handle, &ws, cx, "read_checklist", json!({"block": list.0}));
    assert!(
        !failed && text.contains("(1/2)") && text.contains("[x] One"),
        "{text}"
    );
}
