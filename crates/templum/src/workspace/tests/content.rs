//! The content tools: a wired kanban's cards, a wired note, a file opened in an editor, a wired
//! Git block's status (docs/specs/connections.md, batch 4).

use gpui_kit::component::input::EditorState;
use serde_json::json;

use super::bus::{call, me};
use super::*;

/// The startup terminal wired to a kanban block on a new board with one card ("New card") in To do:
/// (terminal's TEMPLUM_TERMINAL, its uid, the kanban block, the board, the card).
fn wired_board(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> (String, String, BlockId, u64, u64) {
    let terminal = first_id(ws, cx);
    let (kanban, board, card) = board_with_a_card(handle, ws, cx);
    ws.update(cx, |ws, cx| assert!(ws.connect(terminal, kanban, cx)));
    let (me, uid) = me(ws, cx, terminal);
    (me, uid, kanban, board, card)
}

#[gpui_kit::test]
fn list_cards_shows_the_wired_board_and_a_card_whole(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, uid, _, board, card) = wired_board(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            b.edit_card(board, card, |c| {
                c.title = "Fix\nlogin".into();
                c.description = "It 500s.\nOn Safari.".into();
                c.assignee = Some(uid.clone());
                c.comments.push(crate::board::Comment {
                    by: "you".into(),
                    text: "Please".into(),
                    at: 0,
                });
            })
        })
    });
    let (failed, text) = call(handle, &ws, cx, &me, "list_cards", json!({}));
    assert!(!failed, "{text}");
    assert!(text.starts_with("Board \"Board 1\" ("), "{text}");
    assert!(
        text.contains(&format!("\n  {card}  Fix login  assignee {uid}")),
        "one line per card: {text}"
    );
    assert!(text.contains("1 comment"), "{text}");
    assert!(
        text.contains("Doing (") && text.contains("no cards"),
        "{text}"
    );
    let (failed, whole) = call(handle, &ws, cx, &me, "list_cards", json!({ "card": card }));
    assert!(!failed, "{whole}");
    assert!(
        whole.contains("description:\n  It 500s.\n  On Safari."),
        "{whole}"
    );
    assert!(whole.contains("you") && whole.contains("Please"), "{whole}");
}

#[gpui_kit::test]
fn create_move_comment_and_claim_change_the_board(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, uid, _, board, card) = wired_board(handle, &ws, cx);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "create_card",
        json!({ "column": "doing", "title": "Write docs", "description": "README" }),
    );
    assert!(!failed, "{text}");
    // "Created card 7."
    let new: u64 = text
        .split_whitespace()
        .find_map(|w| w.trim_end_matches('.').parse().ok())
        .expect("its id");
    let (doing, done) = ws.read_with(cx, |ws, _| {
        let b = ws.boards.board(board).unwrap();
        (b.columns[1].id, b.columns[2].id)
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.boards.find_card(new)),
        Some((board, doing))
    );
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "move_card",
        json!({ "card": card, "column": "DONE" }),
    );
    assert!(!failed, "{text}");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.boards.find_card(card)),
        Some((board, done))
    );
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "move_card",
        json!({ "card": card, "column": "Review" }),
    );
    assert!(
        failed && text.contains("To do, Doing, Done"),
        "the columns listed: {text}"
    );
    assert!(
        !call(
            handle,
            &ws,
            cx,
            &me,
            "comment_card",
            json!({ "card": card, "text": "Done." })
        )
        .0
    );
    assert!(!call(handle, &ws, cx, &me, "claim_card", json!({ "card": card })).0);
    ws.read_with(cx, |ws, _| {
        let c = ws.boards.card(board, card).unwrap();
        assert_eq!(c.assignee.as_deref(), Some(uid.as_str()));
        assert_eq!(
            (c.comments[0].by.as_str(), c.comments[0].text.as_str()),
            (uid.as_str(), "Done.")
        );
    });
    assert!(
        call(
            handle,
            &ws,
            cx,
            &me,
            "comment_card",
            json!({ "card": card, "text": "  " })
        )
        .0,
        "blank"
    );
    assert!(
        call(handle, &ws, cx, &me, "create_card", json!({ "title": " " })).0,
        "blank"
    );
}

#[gpui_kit::test]
fn claim_card_refuses_a_card_another_block_holds(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, _, board, card) = wired_board(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| b.edit_card(board, card, |c| c.assignee = Some("zzzzz".into())))
    });
    let (failed, text) = call(handle, &ws, cx, &me, "claim_card", json!({ "card": card }));
    assert!(
        failed && text.contains("zzzzz") && text.contains("free"),
        "{text}"
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws
            .boards
            .card(board, card)
            .unwrap()
            .assignee
            .clone())
            .as_deref(),
        Some("zzzzz")
    );
}

#[gpui_kit::test]
fn a_card_on_a_board_not_connected_is_refused(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, _, _, _) = wired_board(handle, &ws, cx);
    // A second kanban block, not wired, on a board of its own.
    let (_, other, theirs) = board_with_a_card(handle, &ws, cx);
    let before = ws.read_with(cx, |ws, _| ws.boards.clone());
    for (tool, args) in [
        ("move_card", json!({ "card": theirs, "column": "Doing" })),
        ("comment_card", json!({ "card": theirs, "text": "hi" })),
        ("claim_card", json!({ "card": theirs })),
        ("list_cards", json!({ "card": theirs })),
        ("list_cards", json!({ "board": other })),
    ] {
        let (failed, text) = call(handle, &ws, cx, &me, tool, args);
        assert!(failed, "{tool}: {text}");
    }
    let (_, unknown) = call(
        handle,
        &ws,
        cx,
        &me,
        "claim_card",
        json!({ "card": 99_999 }),
    );
    let (_, unwired) = call(
        handle,
        &ws,
        cx,
        &me,
        "claim_card",
        json!({ "card": theirs }),
    );
    assert_eq!(
        unknown.replace("99999", "N"),
        unwired.replace(&theirs.to_string(), "N"),
        "says no more"
    );
    assert_eq!(ws.read_with(cx, |ws, _| ws.boards.clone()), before);
}

#[gpui_kit::test]
fn a_board_left_out_with_two_wired_is_refused_and_both_named(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, _, board, _) = wired_board(handle, &ws, cx);
    let (second, other, _) = board_with_a_card(handle, &ws, cx);
    let terminal = ws.read_with(cx, |ws, _| ws.space.terminals().next().unwrap().0);
    ws.update(cx, |ws, cx| assert!(ws.connect(terminal, second, cx)));
    let (failed, text) = call(handle, &ws, cx, &me, "list_cards", json!({}));
    assert!(
        failed && text.contains(&format!("({board})")) && text.contains(&format!("({other})")),
        "{text}"
    );
    assert!(
        !call(
            handle,
            &ws,
            cx,
            &me,
            "list_cards",
            json!({ "board": other })
        )
        .0
    );
    assert!(
        !call(
            handle,
            &ws,
            cx,
            &me,
            "list_cards",
            json!({ "board": "board 1" })
        )
        .0,
        "by name, either case"
    );
}

#[gpui_kit::test]
fn ctrl_z_in_the_kanban_takes_back_an_agents_move(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, kanban, board, card) = wired_board(handle, &ws, cx);
    let todo = ws.read_with(cx, |ws, _| ws.boards.board(board).unwrap().columns[0].id);
    assert!(
        !call(
            handle,
            &ws,
            cx,
            &me,
            "move_card",
            json!({ "card": card, "column": "Doing" })
        )
        .0
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(kanban, window, cx));
        window.press(UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.boards.find_card(card)),
        Some((board, todo))
    );
}

#[gpui_kit::test]
fn the_content_tools_are_refused_without_a_wire(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let (me, _) = me(&ws, cx, terminal);
    let uid = |id| ws.read_with(cx, |ws, _| ws.space.uids[&id].clone());
    let (kanban_uid, note_uid) = (uid(kanban), uid(note));
    for (tool, args) in [
        ("list_cards", json!({})),
        ("create_card", json!({ "title": "x" })),
        ("read_note", json!({ "block": note_uid })),
        ("append_note", json!({ "block": note_uid, "text": "x" })),
        ("git_status", json!({})),
        ("read_note", json!({ "block": kanban_uid })),
    ] {
        let (failed, text) = call(handle, &ws, cx, &me, tool, args.clone());
        assert!(failed, "{tool} {args}: {text}");
    }
}

#[gpui_kit::test]
fn the_instructions_say_what_to_do_with_a_kanban_and_a_note(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (me, _) = me(&ws, cx, terminal);
    let text = ws.read_with(cx, |ws, cx| ws.instructions(&me, cx));
    assert!(
        text.ends_with(
            "When a connected kanban has your task, move its card to Doing when you start and to \
             Review when you are done; write what the next agent needs to know in the connected note."
        ),
        "{text}"
    );
}

/// The startup terminal wired to the note "hello" of `three_blocks`: (TEMPLUM_TERMINAL, note
/// block, note's uid).
fn wired_note(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> (String, BlockId, String) {
    let (_, note, terminal) = three_blocks(handle, ws, cx);
    ws.update(cx, |ws, cx| assert!(ws.connect(terminal, note, cx)));
    let (me, _) = me(ws, cx, terminal);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&note].clone());
    (me, note, uid)
}

fn note_editor(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    note: BlockId,
) -> Entity<EditorState> {
    ws.read_with(cx, |ws, _| {
        ws.space.blocks[&note]
            .note()
            .unwrap()
            .tab()
            .unwrap()
            .editor
            .clone()
    })
}

#[gpui_kit::test]
fn read_note_reads_what_the_note_shows(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, uid) = wired_note(handle, &ws, cx);
    assert_eq!(
        call(handle, &ws, cx, &me, "read_note", json!({ "block": uid })),
        (false, "hello".to_string())
    );
    let terminal = ws.read_with(cx, |ws, _| ws.space.terminals().next().unwrap().0);
    let (_, mine) = super::bus::me(&ws, cx, terminal);
    let (failed, text) = call(handle, &ws, cx, &me, "read_note", json!({ "block": mine }));
    assert!(failed && text.contains("isn't a note"), "{text}");
}

#[gpui_kit::test]
fn append_note_shows_at_once_keeps_the_caret_and_one_ctrl_z_takes_it_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, note, uid) = wired_note(handle, &ws, cx);
    let editor = note_editor(&ws, cx, note);
    // The person is in the note, the caret after "he".
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(note, window, cx));
        editor.update(cx, |e, cx| e.set_selected_range(2..2, cx));
    })
    .unwrap();
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "append_note",
        json!({ "block": uid, "text": "Next: tests" }),
    );
    assert!(!failed, "{text}");
    cx.run_until_parked();
    let id = ws.read_with(cx, |ws, _| {
        ws.space.blocks[&note].note().unwrap().tab().unwrap().id
    });
    editor.read_with(cx, |e, _| {
        assert_eq!(e.value().as_ref(), "hello\n\nNext: tests");
        assert_eq!(e.selected_range(), 2..2, "the caret stays");
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.notes.get(id).unwrap().text.clone()),
        "hello\n\nNext: tests"
    );
    cx.update_window(handle, |_, window, cx| {
        window.press(UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
    editor.read_with(cx, |e, _| assert_eq!(e.value().as_ref(), "hello"));
    // A note ending in a newline gets one more; an empty one none.
    editor.update(cx, |e, cx| e.set_selected_range(5..5, cx));
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |e, cx| e.insert("\n", window, cx));
    })
    .unwrap();
    call(
        handle,
        &ws,
        cx,
        &me,
        "append_note",
        json!({ "block": uid, "text": "x" }),
    );
    editor.read_with(cx, |e, _| assert_eq!(e.value().as_ref(), "hello\n\nx"));
    assert!(
        call(
            handle,
            &ws,
            cx,
            &me,
            "append_note",
            json!({ "block": uid, "text": " \n" })
        )
        .0,
        "blank"
    );
}
