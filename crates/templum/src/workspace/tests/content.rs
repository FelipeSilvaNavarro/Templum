//! The content tools: a wired kanban's cards, a wired note, a file opened in an editor, a wired
//! Git block's status (docs/specs/connections.md, batch 4).

use gpui_kit::Focusable as _;
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
             Review when you are done (Done when the board has no Review); write what the next agent \
             needs to know in the connected note."
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
    let (_dir, folder) = a_project();
    std::fs::create_dir(folder.join("sub")).unwrap();
    let (handle, ws) = open(cx);
    let terminal = terminal_in(handle, &ws, cx, &folder);
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

#[gpui_kit::test]
fn git_status_matches_the_repository(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "x\n").unwrap();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let git = open_git(handle, &ws, cx, repo.path());
    let (me, _) = me(&ws, cx, terminal);
    let answers = super::bus::call_later(handle, &ws, cx, &me, 1, "git_status", json!({}));
    let (failed, text) = super::bus::answer_soon(cx, &answers);
    assert!(
        failed && text.contains("No Git block is connected to you"),
        "{text}"
    );
    ws.update(cx, |ws, cx| assert!(ws.connect(terminal, git, cx)));
    let answers = super::bus::call_later(handle, &ws, cx, &me, 2, "git_status", json!({}));
    let (failed, text) = super::bus::answer_soon(cx, &answers);
    assert!(!failed, "{text}");
    let expected = git_run(
        repo.path(),
        &["-c", "color.status=false", "status", "--short", "--branch"],
    );
    assert_eq!(text, expected.trim_end());
    assert!(
        text.starts_with("## main") && text.contains(" M a.txt") && text.contains("?? new.txt"),
        "{text}"
    );
}
