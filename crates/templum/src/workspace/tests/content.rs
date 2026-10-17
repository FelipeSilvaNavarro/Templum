//! The content tools: a wired kanban's cards, a wired note, a file opened in an editor, a wired
//! Git block's status (docs/specs/connections.md, batch 4).

use gpui_kit::Focusable as _;
use gpui_kit::component::input::EditorState;
use serde_json::json;
use templum_canvas::Wire;

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
        ("read_note", json!({ "block": kanban_uid })),
    ] {
        let (failed, text) = call(handle, &ws, cx, &me, tool, args.clone());
        assert!(failed, "{tool} {args}: {text}");
    }
}

#[gpui_kit::test]
fn the_checklist_tools_are_refused_without_a_wire(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (me, block, uid, _, build, _) = wired_checklist(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        assert!(ws.disconnect(Wire::new(terminal, block).unwrap(), cx))
    });
    for (tool, args) in [
        ("read_checklist", json!({ "block": uid })),
        ("check_item", json!({ "block": uid, "item": build })),
        ("add_items", json!({ "block": uid, "text": "x" })),
    ] {
        let (failed, text) = call(handle, &ws, cx, &me, tool, args.clone());
        assert!(
            failed && text.contains("isn't connected to you"),
            "{tool} {args}: {text}"
        );
    }
}

#[gpui_kit::test]
fn the_instructions_say_what_to_do_with_a_kanban_and_a_note(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (me, _) = me(&ws, cx, terminal);
    let text = ws.read_with(cx, |ws, cx| ws.instructions(&me, cx));
    // Batch 8's sentence about tasks comes after it.
    assert!(
        text.contains(
            "When a connected kanban has your task, move its card to Doing when you start and to \
             Review when you are done (Done when the board has no Review); write what the next agent \
             needs to know in the connected note, and check the items of a connected checklist as you \
             finish them."
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

/// A folder with a.rs (five lines) and b.bin (binary), canonicalized.
fn a_project() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
    std::fs::write(dir.path().join("b.bin"), [0u8, 159, 146, 150, 0, 1]).unwrap();
    let path = std::fs::canonicalize(dir.path()).unwrap();
    (dir, path)
}

/// A terminal started in `folder`, focused.
fn terminal_in(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    folder: &std::path::Path,
) -> BlockId {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let rect = Rect::new(40.0, 400.0, 600.0, 300.0);
            let id = ws
                .spawn_block(rect, Some(folder.to_path_buf()), window, cx)
                .unwrap();
            ws.space.appearing.clear();
            ws.focus_block(id, window, cx);
            id
        })
    })
    .unwrap()
}

#[gpui_kit::test]
fn open_file_lands_at_the_line_in_the_wired_editor(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let editor = open_editor(handle, &ws, cx, &folder);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(ws.connect(terminal, editor, cx));
            ws.focus_block(terminal, window, cx);
            ws.space.flight = None;
        })
    })
    .unwrap();
    let camera = ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    let (me, _) = me(&ws, cx, terminal);
    let path = folder.join("a.rs");
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "open_file",
        json!({ "path": path, "line": 3 }),
    );
    assert!(!failed, "{text}");
    cx.update_window(handle, |_, window, cx| {
        assert!(
            ws.read(cx).focus_handle.is_focused(window),
            "the keys stay with the canvas"
        )
    })
    .unwrap();
    ws.read_with(cx, |ws, cx| {
        let e = ws.space.editor(editor).unwrap();
        let tab = e.active_tab().unwrap();
        assert_eq!(tab.path, path);
        assert_eq!(tab.editor.read(cx).cursor_position().line, 2, "line 3");
        assert_eq!(ws.space.focused, Some(terminal), "the keyboard stays");
        assert!(ws.space.flight.is_none(), "the camera stays");
        assert_eq!(ws.space.canvas.camera, camera);
    });
}

#[gpui_kit::test]
fn open_file_with_no_wired_editor_opens_one_beside_the_caller(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    let (handle, ws) = open(cx);
    // An editor holding the file, but not wired: not the one used.
    let unwired = open_editor(handle, &ws, cx, &folder);
    // The caller's folder can't hold the file's (the startup terminal's, home, holds the
    // temporary folder on Windows).
    let elsewhere = tempfile::tempdir().unwrap();
    let terminal = terminal_in(handle, &ws, cx, elsewhere.path());
    let (me, _) = me(&ws, cx, terminal);
    let blocks = ws.read_with(cx, |ws, _| ws.space.blocks.len());
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "open_file",
        json!({ "path": folder.join("a.rs"), "line": 2 }),
    );
    assert!(!failed && text.contains("new editor"), "{text}");
    cx.update_window(handle, |_, window, cx| {
        assert!(
            ws.read(cx).focus_handle.is_focused(window),
            "the keys stay with the canvas"
        )
    })
    .unwrap();
    let new = ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.blocks.len(), blocks + 1);
        let wire = ws
            .space
            .wires
            .iter()
            .rfind(|w| w.touches(terminal))
            .expect("wired to the caller");
        let new = if wire.a == terminal { wire.b } else { wire.a };
        assert_ne!(new, unwired);
        assert_eq!(
            ws.space.editor(new).unwrap().folder,
            folder,
            "the file's folder"
        );
        assert_eq!(ws.space.focused, Some(terminal), "the keyboard stays");
        new
    });
    // One Ctrl+Z closes it with its wire.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.undo_canvas(false, window, cx))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(!ws.space.blocks.contains_key(&new));
        assert!(!ws.space.wires.iter().any(|w| w.touches(new)));
    });
}

/// A relative path is read from the caller's folder, `..` and all.
#[gpui_kit::test]
fn open_file_reads_a_relative_path_from_the_callers_folder(cx: &mut TestAppContext) {
    let (dir, folder) = a_project();
    std::fs::create_dir(folder.join("sub")).unwrap();
    let (handle, ws) = open(cx);
    // The plain path: cmd.exe can't start in a canonical \\?\ one.
    let terminal = terminal_in(handle, &ws, cx, dir.path());
    let (me, _) = me(&ws, cx, terminal);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "open_file",
        json!({ "path": "sub/../a.rs", "line": 2 }),
    );
    assert!(!failed, "{text}");
    ws.read_with(cx, |ws, cx| {
        let wire = ws
            .space
            .wires
            .iter()
            .rfind(|w| w.touches(terminal))
            .expect("a new editor, wired to the caller");
        let new = if wire.a == terminal { wire.b } else { wire.a };
        let e = ws.space.editor(new).unwrap();
        let tab = e.active_tab().unwrap();
        assert!(tab.path.starts_with(&e.folder), "{:?}", tab.path);
        assert_eq!(
            std::fs::canonicalize(&tab.path).unwrap(),
            folder.join("a.rs")
        );
        assert_eq!(tab.editor.read(cx).cursor_position().line, 1, "line 2");
    });
}

/// A file named by its real path shows in the tab an editor on a linked folder already has.
#[cfg(unix)]
#[gpui_kit::test]
fn open_file_shows_the_tab_already_open_under_a_linked_folder(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    let links = tempfile::tempdir().unwrap();
    let link = links.path().join("project");
    std::os::unix::fs::symlink(&folder, &link).unwrap();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let editor = open_editor(handle, &ws, cx, &link);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(ws.connect(terminal, editor, cx));
            let e = ws.space.editor_mut(editor).unwrap();
            e.open_tab(&link.join("a.rs"), window, cx).unwrap();
        })
    })
    .unwrap();
    let (me, _) = me(&ws, cx, terminal);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "open_file",
        json!({ "path": folder.join("a.rs"), "line": 2 }),
    );
    assert!(!failed, "{text}");
    ws.read_with(cx, |ws, cx| {
        let e = ws.space.editor(editor).unwrap();
        assert_eq!(e.tabs.len(), 1, "the tab already open, no second one");
        assert_eq!(e.active, Some(0));
        assert_eq!(e.tabs[0].editor.read(cx).cursor_position().line, 1);
    });
    assert!(
        text.contains(&link.join("a.rs").display().to_string()),
        "as the editor names it: {text}"
    );
}

/// A linked file (CLAUDE.md -> AGENTS.md) shows the tab open under its own name.
#[cfg(unix)]
#[gpui_kit::test]
fn open_file_shows_the_tab_already_open_under_a_linked_files_name(cx: &mut TestAppContext) {
    let (dir, folder) = a_project();
    std::fs::write(folder.join("AGENTS.md"), "# Agents\n").unwrap();
    std::os::unix::fs::symlink("AGENTS.md", folder.join("CLAUDE.md")).unwrap();
    let (handle, ws) = open(cx);
    let terminal = terminal_in(handle, &ws, cx, dir.path());
    let editor = open_editor(handle, &ws, cx, &folder);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(ws.connect(terminal, editor, cx));
            let e = ws.space.editor_mut(editor).unwrap();
            e.open_tab(&folder.join("CLAUDE.md"), window, cx).unwrap();
        })
    })
    .unwrap();
    let (me, _) = me(&ws, cx, terminal);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "open_file",
        json!({ "path": "CLAUDE.md" }),
    );
    assert!(!failed, "{text}");
    ws.read_with(cx, |ws, _| {
        let e = ws.space.editor(editor).unwrap();
        let tabs: Vec<_> = e.tabs.iter().map(|t| t.path.clone()).collect();
        assert_eq!(tabs, [folder.join("CLAUDE.md")], "no second tab");
        assert_eq!(e.active, Some(0));
    });
}

#[gpui_kit::test]
fn open_file_refuses_what_it_cant_open_and_opens_no_block(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (me, _) = me(&ws, cx, terminal);
    let blocks = ws.read_with(cx, |ws, _| ws.space.blocks.len());
    for path in [folder.join("nope.rs"), folder.clone(), folder.join("b.bin")] {
        let (failed, text) = call(handle, &ws, cx, &me, "open_file", json!({ "path": path }));
        assert!(failed, "{}: {text}", path.display());
    }
    assert_eq!(ws.read_with(cx, |ws, _| ws.space.blocks.len()), blocks);
}

#[gpui_kit::test]
fn an_editor_with_the_keyboard_gives_it_to_the_tab_opened(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    std::fs::write(folder.join("c.rs"), "c\n").unwrap();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let editor = open_editor(handle, &ws, cx, &folder);
    let (me, _) = me(&ws, cx, terminal);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(ws.connect(terminal, editor, cx));
            let e = ws.space.editor_mut(editor).unwrap();
            e.open_tab(&folder.join("c.rs"), window, cx).unwrap();
            assert!(ws.space.editor(editor).unwrap().focus(window, cx));
        })
    })
    .unwrap();
    call(
        handle,
        &ws,
        cx,
        &me,
        "open_file",
        json!({ "path": folder.join("a.rs") }),
    );
    cx.update_window(handle, |_, window, cx| {
        let ws = ws.read(cx);
        let tab = ws.space.editor(editor).unwrap().active_tab().unwrap();
        assert!(tab.path.ends_with("a.rs"));
        // The active tab's own editor has the keys (not the hidden c.rs tab's).
        assert!(
            tab.editor.read(cx).focus_handle(cx).is_focused(window),
            "the new tab has the keys"
        );
    })
    .unwrap();
}

/// An editor with the keyboard shows a file it has open rendered (a Markdown preview): no text
/// to type in, so the canvas takes the keys rather than leave them in the tab now hidden.
#[gpui_kit::test]
fn an_editor_with_the_keyboard_showing_a_preview_gives_it_to_the_canvas(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    std::fs::write(folder.join("c.md"), "# c\n").unwrap();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let editor = open_editor(handle, &ws, cx, &folder);
    let (me, _) = me(&ws, cx, terminal);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(ws.connect(terminal, editor, cx));
            let e = ws.space.editor_mut(editor).unwrap();
            let md = e.open_tab(&folder.join("c.md"), window, cx).unwrap();
            e.tabs[md].preview = true;
            e.open_tab(&folder.join("a.rs"), window, cx).unwrap();
            assert!(ws.space.editor(editor).unwrap().focus(window, cx));
        })
    })
    .unwrap();
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "open_file",
        json!({ "path": folder.join("c.md") }),
    );
    assert!(!failed, "{text}");
    cx.update_window(handle, |_, window, cx| {
        let ws = ws.read(cx);
        let e = ws.space.editor(editor).unwrap();
        assert!(e.active_tab().unwrap().preview);
        assert!(!e.has_focus(window, cx), "no keys in the hidden a.rs tab");
        assert!(ws.focus_handle.is_focused(window), "the canvas has them");
    })
    .unwrap();
}

/// The startup terminal wired to a checklist block showing "Deploy": "Build" (checked) and "Run
/// the tests": (TEMPLUM_TERMINAL, block, its uid, the checklist, Build, Run the tests).
fn wired_checklist(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> (String, BlockId, String, u64, u64, u64) {
    let terminal = first_id(ws, cx);
    let (block, list, build, tests) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let list = ws.checklists.add(1);
                ws.checklists.rename(list, "Deploy", 1);
                let build = ws
                    .checklists
                    .add_item(list, None, "Build", true, 1)
                    .unwrap();
                let tests = ws
                    .checklists
                    .add_item(list, None, "Run the tests", false, 1);
                let item = crate::session::Item::Checklist(crate::session::ChecklistItem {
                    uid: String::new(),
                    rect: Rect::new(900.0, 40.0, 320.0, 360.0).into(),
                    checklists: vec![list],
                    active: 0,
                });
                ws.quiet = true;
                let block = ws.add_saved(&item, window, cx).unwrap();
                ws.quiet = false;
                ws.space.appearing.clear();
                assert!(ws.connect(terminal, block, cx));
                (block, list, build, tests.unwrap())
            })
        })
        .unwrap();
    let (me, _) = me(ws, cx, terminal);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&block].clone());
    (me, block, uid, list, build, tests)
}

fn items_of(ws: &Entity<Workspace>, cx: &mut TestAppContext, list: u64) -> Vec<(String, bool)> {
    ws.read_with(cx, |ws, _| {
        let list = ws.checklists.get(list).unwrap();
        list.items
            .iter()
            .map(|i| (i.text.clone(), i.done))
            .collect()
    })
}

#[gpui_kit::test]
fn read_checklist_reads_the_checklist_on_screen(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, uid, _, build, tests) = wired_checklist(handle, &ws, cx);
    assert_eq!(
        call(
            handle,
            &ws,
            cx,
            &me,
            "read_checklist",
            json!({ "block": uid })
        ),
        (
            false,
            format!("Deploy (1/2)\n{build} [x] Build\n{tests} [ ] Run the tests")
        )
    );
    let (_, blocks) = call(handle, &ws, cx, &me, "list_blocks", json!({}));
    assert!(blocks.contains("checklist"), "{blocks}");
    let terminal = ws.read_with(cx, |ws, _| ws.space.terminals().next().unwrap().0);
    let (_, mine) = super::bus::me(&ws, cx, terminal);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "read_checklist",
        json!({ "block": mine }),
    );
    assert!(failed && text.contains("isn't a checklist"), "{text}");
}

#[gpui_kit::test]
fn check_item_and_add_items_show_at_once_and_ctrl_z_takes_them_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, block, uid, list, _, tests) = wired_checklist(handle, &ws, cx);
    assert_eq!(
        call(
            handle,
            &ws,
            cx,
            &me,
            "check_item",
            json!({ "block": uid, "item": tests })
        ),
        (false, format!("Item {tests} is checked."))
    );
    assert!(items_of(&ws, cx, list)[1].1);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "check_item",
        json!({ "block": uid, "item": 999 }),
    );
    assert!(failed, "{text}");
    assert_eq!(
        text,
        "There's no item 999 in \"Deploy\". read_checklist shows its items."
    );
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "add_items",
        json!({ "block": uid, "text": "- [ ] Ship\n[x] Tag" }),
    );
    assert!(!failed, "{text}");
    let ids: Vec<u64> = ws.read_with(cx, |ws, _| {
        ws.checklists.get(list).unwrap().items[2..]
            .iter()
            .map(|i| i.id)
            .collect()
    });
    assert_eq!(text, format!("Added items {}, {}.", ids[0], ids[1]));
    assert_eq!(
        items_of(&ws, cx, list)[2..],
        [("Ship".to_string(), false), ("Tag".to_string(), true)]
    );
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "add_items",
        json!({ "block": uid, "text": " \n- " }),
    );
    assert!(failed && text == "There's nothing to add.", "{text}");
    // Each call is one step.
    let undo = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.focus_block(block, window, cx));
            window.press(super::UNDO, cx);
            window.render_frame(cx);
        })
        .unwrap();
    };
    undo(cx);
    assert_eq!(items_of(&ws, cx, list).len(), 2, "Ship and Tag gone");
    assert!(items_of(&ws, cx, list)[1].1);
    undo(cx);
    assert!(!items_of(&ws, cx, list)[1].1, "unchecked");
}

/// Review focus: an agent's change while the person edits an item keeps the field (its text and
/// caret), shows the new items under it, and the person's typing after it is its own step.
#[gpui_kit::test]
fn add_items_while_the_person_edits_keeps_their_field(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, block, uid, list, build, _) = wired_checklist(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.edit_field(block, crate::checklist::Field::Item(build), window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    let input = ws.read_with(cx, |ws, _| {
        let c = ws.space.blocks[&block].checklist().unwrap();
        c.editing.as_ref().unwrap().input.clone()
    });
    cx.simulate_input(handle, "!");
    input.update(cx, |i, cx| i.set_selected_range(2..2, cx));
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "add_items",
        json!({ "block": uid, "text": "Ship" }),
    );
    assert!(!failed, "{text}");
    input.read_with(cx, |i, _| {
        assert_eq!(i.value().as_ref(), "Build!");
        assert_eq!(i.cursor(), 2);
    });
    let texts = |cx: &mut TestAppContext| {
        items_of(&ws, cx, list)
            .into_iter()
            .map(|(t, _)| t)
            .collect::<Vec<_>>()
    };
    assert_eq!(texts(cx), ["Build!", "Run the tests", "Ship"]);
    // Typed after the agent's change: a step of its own, taken back before the agent's.
    input.update(cx, |i, cx| {
        let end = i.value().len();
        i.set_selected_range(end..end, cx)
    });
    cx.simulate_input(handle, "?");
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        ws.update(cx, |ws, cx| ws.focus_block(block, window, cx));
        window.press(super::UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(texts(cx), ["Build!", "Run the tests", "Ship"]);
}

/// A new item left empty goes from the undo steps an agent's change took while it was edited:
/// Ctrl+Z never brings back an empty row nobody edits.
#[gpui_kit::test]
fn undoing_an_agents_add_during_a_new_items_edit_brings_back_no_empty_item(
    cx: &mut TestAppContext,
) {
    let (handle, ws) = open(cx);
    let (me, block, uid, list, _, _) = wired_checklist(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.add_item_after(block, list, None, window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &me,
        "add_items",
        json!({ "block": uid, "text": "Ship" }),
    );
    assert!(!failed, "{text}");
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        ws.update(cx, |ws, cx| ws.focus_block(block, window, cx));
        window.press(super::UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let texts: Vec<_> = items_of(&ws, cx, list)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    assert_eq!(texts, ["Build", "Run the tests"]);
    assert!(
        !ws.update(cx, |ws, cx| ws.undo_checklists(cx)),
        "the new item's own step changes nothing now: skipped"
    );
}
