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

/// What the person has: the shown workspace, its focused block and its camera.
fn person_view(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> (String, Option<BlockId>, Camera) {
    ws.read_with(cx, |ws, _| {
        (
            ws.space.id.clone(),
            ws.space.focused,
            ws.space.canvas.camera,
        )
    })
}

/// A tool's answer that names an id last, as `(w2).` or `as b7k2m`: that id.
fn id_in(text: &str) -> String {
    text.trim_end_matches('.')
        .rsplit(['(', ' '])
        .next()
        .unwrap()
        .trim_end_matches(')')
        .to_string()
}

#[gpui_kit::test]
fn create_workspace_makes_one_without_showing_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let before = person_view(&ws, cx);
    let dir = tempfile::tempdir().unwrap();
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "create_workspace",
        json!({"name": "API", "folder": dir.path(), "color": "blue"}),
    );
    assert_eq!(
        (failed, text.as_str()),
        (false, "Created workspace API (w2).")
    );
    assert_eq!(person_view(&ws, cx), before);
    ws.read_with(cx, |ws, _| {
        let space = ws.space_named("w2").unwrap();
        assert_eq!(space.name, "API");
        assert_eq!(space.folder.as_deref(), Some(dir.path()));
        assert_eq!(space.look.color.as_deref(), Some("blue"));
        assert_eq!(ws.templum_agent.conversation.own.workspaces, ["w2"]);
    });
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "create_workspace",
        json!({"name": "X", "color": "#ff0000"}),
    );
    assert!(failed && text.contains("red, yellow, green"), "{text}");
}

#[gpui_kit::test]
fn add_block_opens_each_kind_where_it_covers_nothing(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let folder = tempfile::tempdir().unwrap();
    control(
        handle,
        &ws,
        cx,
        "create_workspace",
        json!({"name": "API", "folder": folder.path()}),
    );
    let before = person_view(&ws, cx);
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "add_block",
        json!({"workspace": "api", "kind": "kanban", "board": "Sprint"}),
    );
    assert!(!failed, "{text}");
    let kanban = id_in(&text);
    let mut uids = vec![kanban.clone()];
    for args in [
        json!({"workspace": "w2", "kind": "note", "text": "plan", "near": kanban}),
        json!({"workspace": "w2", "kind": "checklist", "text": "Steps\nOne\nTwo"}),
        json!({"workspace": "w2", "kind": "terminal"}),
        json!({"workspace": "w2", "kind": "editor", "folder": folder.path()}),
    ] {
        let (failed, text) = control(handle, &ws, cx, "add_block", args);
        assert!(!failed, "{text}");
        uids.push(id_in(&text));
    }
    assert_eq!(
        person_view(&ws, cx),
        before,
        "the person's view and keyboard stay"
    );
    ws.read_with(cx, |ws, cx| {
        let space = ws.space_named("w2").unwrap();
        let rects: Vec<Rect> = uids
            .iter()
            .map(|uid| {
                space
                    .canvas
                    .get(space.block_by_uid(uid).unwrap())
                    .unwrap()
                    .rect
            })
            .collect();
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!a.intersects(b), "{a:?} and {b:?} overlap");
            }
        }
        // The note sits beside the kanban.
        let (k, n) = (rects[0], rects[1]);
        let gap = (n.x - (k.x + k.w)).abs().min((k.x - (n.x + n.w)).abs());
        let gap = gap
            .min((n.y - (k.y + k.h)).abs())
            .min((k.y - (n.y + n.h)).abs());
        assert!(gap <= templum_canvas::BESIDE + 1.0, "{k:?} {n:?}");
        let board = space
            .kanban(space.block_by_uid(&uids[0]).unwrap())
            .unwrap()
            .board
            .unwrap();
        let b = ws.boards.board(board).unwrap();
        assert_eq!(b.name, "Sprint");
        let columns: Vec<&str> = b.columns.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(columns, ["To do", "Doing", "Review", "Done"]);
        let note = space.blocks[&space.block_by_uid(&uids[1]).unwrap()]
            .note()
            .unwrap();
        assert_eq!(note.tab().unwrap().editor.read(cx).value().as_ref(), "plan");
        let list = space.blocks[&space.block_by_uid(&uids[2]).unwrap()]
            .checklist()
            .unwrap();
        let list = ws.checklists.get(list.list.unwrap()).unwrap();
        assert_eq!(list.name_shown(), "Steps");
        assert_eq!(list.items.len(), 2);
        assert!(
            space
                .terminal(space.block_by_uid(&uids[3]).unwrap())
                .is_some()
        );
        assert_eq!(ws.templum_agent.conversation.own.blocks, uids);
    });
    // A kanban on a board that is there shows it.
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "add_block",
        json!({"workspace": "w2", "kind": "kanban", "board": "sprint"}),
    );
    assert!(!failed, "{text}");
    assert_eq!(ws.read_with(cx, |ws, _| ws.boards.boards.len()), 1);
}

#[gpui_kit::test]
fn a_stopped_workspace_is_refused(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    control(handle, &ws, cx, "create_workspace", json!({"name": "API"}));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.stop_now("w2", window, cx))
    })
    .unwrap();
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "add_block",
        json!({"workspace": "API", "kind": "note"}),
    );
    assert_eq!(
        (failed, text.as_str()),
        (true, "Workspace API is stopped: show it first (show).")
    );
    assert!(ws.read_with(cx, |ws, _| {
        ws.space_named("w2").unwrap().canvas.blocks().is_empty()
    }));
    let (failed, text) = control(handle, &ws, cx, "show", json!({"workspace": "API"}));
    assert!(!failed, "{text}");
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "add_block",
        json!({"workspace": "API", "kind": "note"}),
    );
    assert!(!failed, "{text}");
}

#[gpui_kit::test]
fn show_switches_and_reveals(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    control(handle, &ws, cx, "create_workspace", json!({"name": "API"}));
    let (_, text) = control(
        handle,
        &ws,
        cx,
        "add_block",
        json!({"workspace": "w2", "kind": "note"}),
    );
    let note = id_in(&text);
    let (failed, text) = control(handle, &ws, cx, "show", json!({"block": note}));
    assert!(!failed, "{text}");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.id, "w2");
        assert_eq!(ws.space.focused, ws.space.block_by_uid(&note));
    });
    let (failed, text) = control(handle, &ws, cx, "show", json!({"workspace": "nowhere"}));
    assert!(
        failed && text.contains("There's no workspace nowhere"),
        "{text}"
    );
}

#[gpui_kit::test]
fn connect_wires_two_blocks_and_ctrl_z_takes_it_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let (a, b, t) = ws.read_with(cx, |ws, _| {
        (
            ws.space.uids[&kanban].clone(),
            ws.space.uids[&note].clone(),
            ws.space.uids[&terminal].clone(),
        )
    });
    let (failed, text) = control(handle, &ws, cx, "connect", json!({"a": a, "b": t}));
    assert!(!failed, "{text}");
    assert!(ws.read_with(cx, |ws, _| ws.space.wired(kanban, terminal)));
    cx.update_window(handle, |_, window, cx| {
        window.press(UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(
        !ws.read_with(cx, |ws, _| ws.space.wired(kanban, terminal)),
        "Ctrl+Z unwires"
    );
    control(handle, &ws, cx, "connect", json!({"a": b, "b": t}));
    let (failed, text) = control(handle, &ws, cx, "disconnect", json!({"a": t, "b": b}));
    assert!(!failed, "{text}");
    assert!(!ws.read_with(cx, |ws, _| ws.space.wired(note, terminal)));
    control(handle, &ws, cx, "create_workspace", json!({"name": "API"}));
    let (_, text) = control(
        handle,
        &ws,
        cx,
        "add_block",
        json!({"workspace": "w2", "kind": "note"}),
    );
    let far = id_in(&text);
    let (failed, text) = control(handle, &ws, cx, "connect", json!({"a": a, "b": far}));
    assert_eq!(
        (failed, text),
        (true, format!("{a} and {far} aren't in one workspace."))
    );
}

/// Claude Code and Codex started as `cat` in front of new terminals' shells, their input boxes
/// bracketing pastes (as the spawn tests' `spawner`).
#[cfg(unix)]
fn cat_agents(ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    ws.update(cx, |ws, _| {
        ws.command = Some(jobs("printf '\\033[?2004h'; cat; true"));
        for agent in &mut ws.agents {
            if agent.id == "claude" || agent.id == "codex" {
                agent.command = "cat".into();
            }
        }
    });
}

/// The block id an `open_agent` answer names: `Opened … as <uid> in …`.
fn opened_uid(text: &str) -> String {
    text.split(" as ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_string()
}

#[cfg(unix)]
#[gpui_kit::test]
fn open_agent_opens_in_a_workspace_not_shown_wired_to_its_board(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cat_agents(&ws, cx);
    let folder = tempfile::tempdir().unwrap();
    control(
        handle,
        &ws,
        cx,
        "create_workspace",
        json!({"name": "API", "folder": folder.path()}),
    );
    let (_, text) = control(
        handle,
        &ws,
        cx,
        "add_block",
        json!({"workspace": "w2", "kind": "kanban", "board": "Sprint"}),
    );
    let kanban = id_in(&text);
    let before = person_view(&ws, cx);
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "open_agent",
        json!({"agent": "codex", "workspace": "API", "connect": [kanban], "near": kanban,
               "model": "gpt-5.6", "effort": "high"}),
    );
    assert!(!failed, "{text}");
    let uid = opened_uid(&text);
    assert_eq!(
        text,
        format!(
            "Opened Codex as {uid} in {}, on gpt-5.6 (high), connected to {kanban}.",
            folder.path().display()
        )
    );
    assert_eq!(
        person_view(&ws, cx),
        before,
        "the person's view and keyboard stay"
    );
    ws.read_with(cx, |ws, _| {
        let space = ws.space_named("w2").unwrap();
        let (id, board) = (
            space.block_by_uid(&uid).unwrap(),
            space.block_by_uid(&kanban).unwrap(),
        );
        assert!(space.wired(id, board));
        let pane = space.terminal(id).unwrap();
        assert_eq!(pane.opened_by.as_deref(), Some(::templum_agent::ASKER));
        assert_eq!(
            (pane.model.as_deref(), pane.effort.as_deref()),
            (Some("gpt-5.6"), Some("high"))
        );
        assert!(ws.templum_agent.conversation.own.blocks.contains(&uid));
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn its_prompt_is_typed_under_the_templum_agents_line(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cat_agents(&ws, cx);
    let folder = tempfile::tempdir().unwrap();
    let shown = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "open_agent",
        json!({"agent": "codex", "workspace": shown, "folder": folder.path(), "prompt": "say hi"}),
    );
    assert!(!failed, "{text}");
    assert!(
        text.ends_with(" Its prompt is typed once it is ready, as task t1."),
        "{text}"
    );
    let new = ws.read_with(cx, |ws, _| {
        ws.space.block_by_uid(&opened_uid(&text)).unwrap()
    });
    in_front(&ws, cx, new, true);
    super::delegate::found(&ws, cx, new);
    let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
    let mut looks = 0;
    while ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().prompt.is_some()) {
        assert!(looks < 10, "never typed");
        std::thread::sleep(std::time::Duration::from_millis(100));
        super::bus::after(cx, look);
        looks += 1;
    }
    screen_with(&ws, cx, new, "[from Templum Agent, task t1]");
    let by = ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(new)
            .unwrap()
            .handed
            .as_ref()
            .unwrap()
            .by
            .clone()
    });
    assert_eq!(by, ::templum_agent::ASKER);
}

#[gpui_kit::test]
fn an_unknown_agent_or_a_bad_effort_opens_nothing(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let shown = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let blocks = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len())
    };
    let before = blocks(&ws, cx);
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "open_agent",
        json!({"agent": "nope", "workspace": shown}),
    );
    assert!(failed && text.contains("There's no agent nope"), "{text}");
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "open_agent",
        json!({"agent": "claude", "workspace": shown, "effort": "ultra"}),
    );
    assert!(failed && text.contains("effort"), "{text}");
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "open_agent",
        json!({"agent": "claude", "workspace": shown, "connect": ["nope1"]}),
    );
    assert!(failed && text.contains("nope1"), "{text}");
    assert_eq!(blocks(&ws, cx), before);
}

/// Calls `tool` as the engine does, on connection `conn` (its request's id is `conn` too): the
/// channel its answer comes on, now or once the person answers.
fn control_later(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    conn: u64,
    tool: &str,
    arguments: Value,
) -> std::sync::mpsc::Receiver<String> {
    let secret = ws.read_with(cx, |ws, _| ws.templum_agent.secret.clone());
    let (reply, answers) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: String::new(),
            client: "control".into(),
            control: Some(secret),
        },
        conn,
        incoming: Incoming::Request {
            id: json!(conn),
            method: "tools/call".into(),
            params: json!({ "name": tool, "arguments": arguments }),
        },
        reply,
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.serve(message, window, cx))
    })
    .unwrap();
    answers
}

/// A notification on the control connection `conn`, as the engine or the bus sends it.
fn control_note(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    conn: u64,
    method: &str,
    params: Value,
) {
    let secret = ws.read_with(cx, |ws, _| ws.templum_agent.secret.clone());
    let (reply, _) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: String::new(),
            client: "control".into(),
            control: Some(secret),
        },
        conn,
        incoming: Incoming::Notification {
            method: method.into(),
            params,
        },
        reply,
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.serve(message, window, cx))
    })
    .unwrap();
}

fn answer(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    answer: crate::chat::Answer,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let id = ws.templum_agent.approvals[0].id;
            ws.answer_approval(id, answer, window, cx)
        })
    })
    .unwrap();
    cx.run_until_parked();
}

/// A new terminal in the shown workspace where Claude Code runs, as a snapshot would have found
/// it: closing it loses work. Its block and its id.
fn busy_terminal(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> (BlockId, String) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_terminal(window, cx);
            let id = ws.space.canvas.top().unwrap();
            ws.space.terminal_mut(id).unwrap().seen.agent = Some("claude".into());
            (id, ws.space.uids[&id].clone())
        })
    })
    .unwrap()
}

#[gpui_kit::test]
fn an_idle_shell_closes_at_once_a_running_program_waits(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let idle = first_id(&ws, cx);
    let idle_uid = ws.read_with(cx, |ws, _| ws.space.uids[&idle].clone());
    let (busy, uid) = busy_terminal(handle, &ws, cx);
    let (failed, text) = control(handle, &ws, cx, "close_block", json!({"block": idle_uid}));
    assert_eq!(
        (failed, text),
        (
            false,
            format!("Closed {idle_uid}. The person can bring it back with Ctrl+Z.")
        )
    );
    let answers = control_later(
        handle,
        &ws,
        cx,
        7,
        "close_block",
        json!({"block": uid, "reason": "done"}),
    );
    assert!(answers.try_recv().is_err(), "it waits for the person");
    ws.read_with(cx, |ws, _| {
        let approvals = &ws.templum_agent.approvals;
        assert_eq!(approvals.len(), 1);
        assert_eq!(approvals[0].said, "Close \"Claude Code\"");
        assert_eq!(approvals[0].details, "Claude Code is running in it.");
        assert!(ws.space.canvas.get(busy).is_some());
        assert!(ws.templum_agent.unread);
    });
    answer(handle, &ws, cx, crate::chat::Answer::Once);
    let (failed, text) = super::bus::result_of(&answers.try_recv().unwrap());
    assert!(
        !failed && text.starts_with(&format!("Closed {uid}.")),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.canvas.get(busy).is_none());
        assert!(ws.templum_agent.approvals.is_empty());
        assert!(
            ws.templum_agent
                .conversation
                .rows
                .contains(&::templum_agent::Row::Approval {
                    said: "Close \"Claude Code\"".into(),
                    answer: "Allowed once".into(),
                })
        );
    });
}

#[gpui_kit::test]
fn deny_answers_the_refusal_with_the_persons_words(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (busy, uid) = busy_terminal(handle, &ws, cx);
    let answers = control_later(handle, &ws, cx, 7, "close_block", json!({"block": uid}));
    answer(
        handle,
        &ws,
        cx,
        crate::chat::Answer::Deny(Some("leave it".into())),
    );
    let (failed, text) = super::bus::result_of(&answers.try_recv().unwrap());
    assert!(failed);
    assert_eq!(
        text,
        "The person denied this. Don't try it another way: ask them what they want.\nThey said: leave it"
    );
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.canvas.get(busy).is_some());
        assert!(
            ws.templum_agent
                .conversation
                .rows
                .contains(&::templum_agent::Row::Approval {
                    said: "Close \"Claude Code\"".into(),
                    answer: "Denied".into(),
                })
        );
    });
    let answers = control_later(handle, &ws, cx, 8, "close_block", json!({"block": uid}));
    answer(handle, &ws, cx, crate::chat::Answer::Deny(None));
    let (_, text) = super::bus::result_of(&answers.try_recv().unwrap());
    assert_eq!(
        text,
        "The person denied this. Don't try it another way: ask them what they want."
    );
}

#[gpui_kit::test]
fn a_grant_lets_the_tool_through(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&first].clone());
    let answers = control_later(
        handle,
        &ws,
        cx,
        7,
        "send_to_terminal",
        json!({"block": uid, "text": "ls"}),
    );
    assert!(answers.try_recv().is_err(), "it waits");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.templum_agent.approvals[0].details.clone()),
        "ls"
    );
    answer(handle, &ws, cx, crate::chat::Answer::Conversation);
    let (failed, text) = super::bus::result_of(&answers.try_recv().unwrap());
    assert_eq!(
        (failed, text.as_str()),
        (false, format!("Typed into {uid}.").as_str())
    );
    let (failed, text) = control(
        handle,
        &ws,
        cx,
        "send_to_terminal",
        json!({"block": uid, "text": "pwd"}),
    );
    assert!(!failed, "the grant lets it through: {text}");
    ws.read_with(cx, |ws, _| {
        assert!(ws.templum_agent.grants.contains("send_to_terminal"));
        assert!(ws.templum_agent.approvals.is_empty());
    });
}

#[gpui_kit::test]
fn its_own_running_agent_closes_without_asking(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (busy, uid) = busy_terminal(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.templum_agent.conversation.own.blocks.push(uid.clone())
    });
    let (failed, text) = control(handle, &ws, cx, "close_block", json!({"block": uid}));
    assert!(!failed, "{text}");
    assert!(ws.read_with(cx, |ws, _| ws.space.canvas.get(busy).is_none()));
}

#[gpui_kit::test]
fn a_target_gone_meanwhile_is_answered_as_gone(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (busy, uid) = busy_terminal(handle, &ws, cx);
    let answers = control_later(handle, &ws, cx, 7, "close_block", json!({"block": uid}));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.close_now(&[busy], super::super::history::Then::New, window, cx)
        })
    })
    .unwrap();
    answer(handle, &ws, cx, crate::chat::Answer::Once);
    let (failed, text) = super::bus::result_of(&answers.try_recv().unwrap());
    assert_eq!(
        (failed, text),
        (
            true,
            format!("There's no block {uid}: overview lists them.")
        )
    );
}

#[gpui_kit::test]
fn a_cancel_or_the_engines_end_drops_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, uid) = busy_terminal(handle, &ws, cx);
    let first = control_later(handle, &ws, cx, 7, "close_block", json!({"block": uid}));
    control_note(
        handle,
        &ws,
        cx,
        7,
        "notifications/cancelled",
        json!({"requestId": 7}),
    );
    assert!(ws.read_with(cx, |ws, _| ws.templum_agent.approvals.is_empty()));
    assert!(matches!(
        first.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
    let _second = control_later(handle, &ws, cx, 8, "close_block", json!({"block": uid}));
    let _third = control_later(handle, &ws, cx, 9, "close_block", json!({"block": uid}));
    control_note(handle, &ws, cx, 8, templum_mcp::bus::CLOSED, json!({}));
    let left: Vec<u64> = ws.read_with(cx, |ws, _| {
        ws.templum_agent.approvals.iter().map(|a| a.conn).collect()
    });
    assert_eq!(left, [9], "only connection 8's went");
}

#[gpui_kit::test]
fn send_keys_shows_what_the_agent_asks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (busy, uid) = busy_terminal(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(busy).unwrap().state = Some(crate::workspace::AgentState::NeedsYou(
            Some("Allow Edit?".into()),
        ));
    });
    let _answers = control_later(
        handle,
        &ws,
        cx,
        7,
        "send_keys",
        json!({"block": uid, "keys": ["2", "enter"]}),
    );
    ws.read_with(cx, |ws, _| {
        let approval = &ws.templum_agent.approvals[0];
        assert_eq!(approval.said, "Press keys in \"Claude Code\"");
        assert_eq!(approval.details, "It asks: Allow Edit?\n2 enter");
    });
}
