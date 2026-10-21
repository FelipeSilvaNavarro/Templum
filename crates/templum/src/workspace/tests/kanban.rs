//! Kanban blocks and their boards.

use super::*;
use gpui_kit::component::WindowExt as _;

fn top_kanban(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> BlockId {
    ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        assert!(ws.space.kanban(id).is_some(), "the top block is a kanban");
        id
    })
}

#[gpui_kit::test]
fn the_board_button_opens_a_block_with_no_board_and_new_board_fills_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-kanban", cx);
        // The view landed where it glides to show the new block.
        ws.update(cx, |ws, _| ws.space.flight = None);
        window.render_frame(cx);
    })
    .unwrap();
    let block = top_kanban(&ws, cx);
    let n = block.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find(("kanban-empty", n)).is_some());
        window.click(("board-new-here", n), cx);
        window.render_frame(cx);
        assert!(
            window.try_find(("columns", n)).is_some(),
            "the new board shows"
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let board = ws.boards.boards[0].id;
        assert_eq!(ws.space.kanban(block).unwrap().board, Some(board));
        assert_eq!(ws.boards.board(board).unwrap().columns.len(), 3);
    });
}

#[gpui_kit::test]
fn two_blocks_on_one_board_show_the_same_cards(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (a, b, board, todo) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let a = ws.space.canvas.top().unwrap();
                ws.new_board(a, cx);
                ws.new_kanban(window, cx);
                let b = ws.space.canvas.top().unwrap();
                let board = ws.space.kanban(a).unwrap().board.unwrap();
                ws.show_board(b, board, cx);
                let todo = ws.boards.board(board).unwrap().columns[0].id;
                ws.space.appearing.clear();
                ws.space.flight = None;
                (a, b, board, todo)
            })
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let add = gpui_kit::ElementId::Name(format!("add-card-{}-{todo}", b.0).into());
        window.click(add, cx);
        window.render_frame(cx);
        let card = ws.read(cx).boards.board(board).unwrap().columns[0].cards[0].id;
        for block in [a, b] {
            let id = gpui_kit::ElementId::Name(format!("card-{}-{card}", block.0).into());
            assert!(window.try_find(id).is_some(), "block {block:?} shows it");
        }
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.delete_board(board, cx));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for block in [a, b] {
            let empty = ("kanban-empty", block.0 as usize);
            assert!(window.try_find(empty).is_some(), "no board left there");
        }
    })
    .unwrap();
}

fn board_row(id: u64) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(format!("board-row-{id}").into())
}

fn board_trash(id: u64) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(format!("board-delete-{id}").into())
}

/// The header's All boards: the dialog finds a board by a card's title, a click shows it in the
/// block, and its trash deletes it on a second click.
#[gpui_kit::test]
fn the_boards_dialog_finds_shows_and_deletes_a_board(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (block, a, b) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let block = ws.space.canvas.top().unwrap();
                let a = ws.boards.add_board();
                let b = ws.boards.add_board();
                let todo = ws.boards.board(b).unwrap().columns[0].id;
                let card = ws.boards.add_card(b, todo).unwrap();
                ws.boards
                    .edit_card(b, card, |c| c.title = "Ship the release".into());
                ws.space.appearing.clear();
                ws.space.flight = None;
                (block, a, b)
            })
        })
        .unwrap();
    let n = block.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(("boards", n), cx);
        window.render_frame(cx);
        window.find(board_row(a));
        window.find(board_row(b));
    })
    .unwrap();
    cx.simulate_input(handle, "RELEASE");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(board_row(a)).is_none(),
            "the search leaves it out"
        );
        window.click(board_row(b), cx);
        window.render_frame(cx);
        assert!(window.try_find(board_row(b)).is_none(), "the dialog closed");
    })
    .unwrap();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.space.kanban(block).unwrap().board),
        Some(b),
        "the block shows it"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click(("boards", n), cx);
        window.render_frame(cx);
        window.click(board_trash(b), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(
        ws.read_with(cx, |ws, _| ws.boards.board(b).is_some()),
        "one click asks"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click(board_trash(b), cx);
        window.render_frame(cx);
        assert!(
            window.try_find(("kanban-empty", n)).is_some(),
            "the block shows none"
        );
    })
    .unwrap();
    assert!(
        ws.read_with(cx, |ws, _| ws.boards.board(b).is_none()),
        "deleted"
    );
}

/// A kanban block's menu: All boards…, then Rename board… and Delete board… for the board it
/// shows.
#[gpui_kit::test]
fn the_blocks_menu_deletes_the_board_it_shows(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (block, board) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                ws.space.appearing.clear();
                ws.space.flight = None;
                (block, ws.space.kanban(block).unwrap().board.unwrap())
            })
        })
        .unwrap();
    let n = block.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.right_click(("title", n), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(2usize, cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        assert!(window.try_find(("kanban-empty", n)).is_some());
    })
    .unwrap();
    assert!(ws.read_with(cx, |ws, _| ws.boards.board(board).is_none()));
}

#[gpui_kit::test]
fn an_open_card_toggles_labels_and_can_be_deleted(cx: &mut TestAppContext) {
    use crate::board::Label;
    let (handle, ws) = open(cx);
    let (block, board, card) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                let board = ws.space.kanban(block).unwrap().board.unwrap();
                let todo = ws.boards.board(board).unwrap().columns[0].id;
                let card = ws.boards.add_card(board, todo).unwrap();
                ws.space.appearing.clear();
                (block, board, card)
            })
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let id = gpui_kit::ElementId::Name(format!("card-{}-{card}", block.0).into());
        assert!(window.try_find(id).is_some());
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, card).cloned().unwrap();
            crate::kanban::open_card(board, current, false, window, cx)
        });
        window.render_frame(cx);
        // The dialog slides in for 0.25 s first: presses meanwhile are lost.
        std::thread::sleep(std::time::Duration::from_millis(300));
        window.render_frame(cx);
        // The title has the keyboard when the card opens.
        window.input("!", cx);
    })
    .unwrap();
    // Subscribers hear the typing once that update ends.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let title = ws.read(cx).boards.card(board, card).unwrap().title.clone();
        assert!(title.contains('!'), "typing applies at once: {title:?}");
        window.click("label-green", cx);
        window.render_frame(cx);
        let labels = ws.read(cx).boards.card(board, card).unwrap().labels.clone();
        assert_eq!(labels, [Label::Green]);
        window.click("delete-card", cx);
        window.render_frame(cx);
        assert!(ws.read(cx).boards.card(board, card).is_none());
        assert!(
            window.try_find("delete-card").is_none(),
            "the dialog closed"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_wheel_over_a_kanban_block_stays_in_it_and_ctrl_zooms(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (block, doing) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                ws.space.appearing.clear();
                ws.space.flight = None;
                let board = ws.space.kanban(block).unwrap().board.unwrap();
                (block, ws.boards.board(board).unwrap().columns[1].id)
            })
        })
        .unwrap();
    let column = gpui_kit::ElementId::Name(format!("column-{}-{doing}", block.0).into());
    // As a pointer does: over the column first, then the wheel.
    let wheel_over = |window: &mut Window, cx: &mut gpui_kit::App, delta, control| {
        window.hover(column.clone(), cx);
        let at = window.find(column.clone()).bounds().center();
        wheel(
            window,
            cx,
            (f32::from(at.x), f32::from(at.y)),
            delta,
            control,
        );
        window.render_frame(cx);
    };
    let before = ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let x = window.find(column.clone()).bounds().origin.x;
        wheel_over(
            window,
            cx,
            ScrollDelta::Pixels(point(px(0.), px(-200.))),
            false,
        );
        assert_eq!(
            window.find(column.clone()).bounds().origin.x,
            x,
            "a vertical wheel doesn't slide the columns"
        );
        wheel_over(
            window,
            cx,
            ScrollDelta::Pixels(point(px(-200.), px(0.))),
            false,
        );
        assert!(
            window.find(column.clone()).bounds().origin.x < x,
            "a sideways one does"
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera, before, "no pan")
    });
    cx.update_window(handle, |_, window, cx| {
        wheel_over(window, cx, ScrollDelta::Lines(point(0.0, 1.0)), true);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.zoom(), 1.1, "Ctrl zooms")
    });
}

#[gpui_kit::test]
fn kanban_blocks_and_boards_come_back(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let board = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                let board = ws.space.kanban(block).unwrap().board.unwrap();
                let todo = ws.boards.board(board).unwrap().columns[0].id;
                ws.boards.add_card(board, todo);
                ws.new_kanban(window, cx);
                ws.save_canvas_now(cx);
                board
            })
        })
        .unwrap();
    let before = ws.read_with(cx, |ws, _| ws.boards.clone());
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        assert_eq!(ws.boards, before);
        let mut shown: Vec<Option<u64>> = ws
            .space
            .canvas
            .blocks()
            .iter()
            .filter_map(|b| ws.space.kanban(b.id).map(|k| k.board))
            .collect();
        shown.sort();
        assert_eq!(shown, [None, Some(board)]);
        assert_eq!(ws.space.terminals().count(), 1, "and the terminal");
    });
}

#[gpui_kit::test]
fn deleting_a_card_from_its_menu_asks_first(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (block, board, card) = board_with_a_card(handle, &ws, cx);
    let delete = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let el = gpui_kit::ElementId::Name(format!("card-{}-{card}", block.0).into());
            window.right_click(el, cx);
            window.render_frame(cx);
            // Open, Send to ▸, —, Delete card.
            window.within("popup-menu").click(3usize, cx);
        })
        .unwrap();
        // The menu fades out for a moment: a click meanwhile would land on it.
        std::thread::sleep(std::time::Duration::from_millis(300));
    };
    delete(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "it asks");
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
        assert!(ws.read(cx).boards.card(board, card).is_some(), "kept");
    })
    .unwrap();
    delete(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        assert!(ws.read(cx).boards.card(board, card).is_none(), "deleted");
    })
    .unwrap();
}

#[cfg(target_os = "macos")]
const UNDO: &str = "cmd-z";
#[cfg(not(target_os = "macos"))]
const UNDO: &str = "ctrl-z";

/// Ctrl+Z with a kanban block focused undoes the last board change; with a terminal focused,
/// the boards are left alone (the key is the terminal's).
#[gpui_kit::test]
fn ctrl_z_undoes_board_changes_in_a_focused_kanban(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (block, board, card) = board_with_a_card(handle, &ws, cx);
    let press_undo = |cx: &mut TestAppContext, focus: BlockId| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.focus_block(focus, window, cx));
            window.press(UNDO, cx);
            window.render_frame(cx);
        })
        .unwrap();
    };
    ws.update(cx, |ws, _| ws.edit_boards(|b| b.remove_card(board, card)));
    press_undo(cx, terminal);
    assert!(ws.read_with(cx, |ws, _| ws.boards.card(board, card).is_none()));
    press_undo(cx, block);
    assert!(
        ws.read_with(cx, |ws, _| ws.boards.card(board, card).is_some()),
        "the card is back"
    );
}

/// Typing in a card's dialog is one undo step of its own: Ctrl+Z undoes it, not the change
/// before it.
#[gpui_kit::test]
fn a_card_dialogs_typing_is_one_undo_step(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (block, board, first) = board_with_a_card(handle, &ws, cx);
    let second = ws.update(cx, |ws, _| {
        let todo = ws.boards.board(board).unwrap().columns[0].id;
        let second = ws.edit_boards(|b| b.add_card(board, todo)).unwrap();
        ws.edit_boards(|b| b.remove_card(board, first));
        second
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, second).cloned().unwrap();
            crate::kanban::open_card(board, current, true, window, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_input(handle, "Renamed");
    cx.update_window(handle, |_, window, cx| {
        window.close_dialog(cx);
        window.render_frame(cx);
        ws.update(cx, |ws, cx| ws.focus_block(block, window, cx));
        window.press(UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.boards.card(board, second).unwrap().title, "New card");
        assert!(
            ws.boards.card(board, first).is_none(),
            "the removal before stays"
        );
    });
}

/// Undoing never hands out an id twice: a block or a file may still name an undone board.
#[gpui_kit::test]
fn undoing_a_new_board_keeps_its_id_taken(cx: &mut TestAppContext) {
    let (_handle, ws) = open(cx);
    ws.update(cx, |ws, cx| {
        let board = ws.edit_boards(crate::board::Boards::add_board);
        assert!(ws.undo_boards(cx));
        assert!(ws.boards.board(board).is_none());
        let again = ws.edit_boards(crate::board::Boards::add_board);
        assert_ne!(again, board);
    });
}

#[gpui_kit::test]
fn a_card_shows_its_assignee_and_how_many_comments(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (block, board, card) = board_with_a_card(handle, &ws, cx);
    let shown = |cx: &mut TestAppContext, what: &str| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let id = gpui_kit::ElementId::Name(format!("{what}-{}-{card}", block.0).into());
            window.try_find(id).is_some()
        })
        .unwrap()
    };
    assert!(!shown(cx, "assignee") && !shown(cx, "comments"));
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&terminal].clone());
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            b.edit_card(board, card, |c| {
                c.assignee = Some(crate::board::Assignee::Block(uid.clone()));
            });
            b.add_comment(board, card, "you", "hi", 0)
        })
    });
    assert!(shown(cx, "assignee") && shown(cx, "comments"));
    // The title it shows is the block's, as agents see it.
    assert!(ws.read_with(cx, |ws, cx| ws.uid_title(&uid, cx)).is_some());
}

#[gpui_kit::test]
fn the_open_card_adds_a_comment_and_frees_its_assignee(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (_, board, card) = board_with_a_card(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            b.edit_card(board, card, |c| {
                c.assignee = Some(crate::board::Assignee::Block("zzzzz".into()))
            })
        })
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, card).cloned().unwrap();
            crate::kanban::open_card(board, current, false, window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    // The dialog slides in for 0.25 s first: presses meanwhile are lost.
    std::thread::sleep(std::time::Duration::from_millis(300));
    // Type into the comment field, then Add comment.
    cx.update_window(handle, |_, window, cx| {
        window.click("comment-field", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_input(handle, "Looks good");
    cx.update_window(handle, |_, window, cx| {
        window.click("add-comment", cx);
        window.render_frame(cx);
        window.click("unassign", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let c = ws.boards.card(board, card).unwrap();
        assert_eq!(c.comments.len(), 1);
        assert_eq!(
            (c.comments[0].by.as_str(), c.comments[0].text.as_str()),
            ("you", "Looks good")
        );
        assert_eq!(c.assignee, None);
    });
    // Each is a step of the kanban's Ctrl+Z: the assignee comes back first.
    ws.update(cx, |ws, cx| assert!(ws.undo_boards(cx)));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws
            .boards
            .card(board, card)
            .unwrap()
            .assignee
            .clone()),
        Some(crate::board::Assignee::Block("zzzzz".into()))
    );
}

#[gpui_kit::test]
fn a_card_shows_its_id_always_and_its_priority_and_checklist_when_it_has_them(
    cx: &mut TestAppContext,
) {
    use crate::board::{Assignee, Priority};
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (block, board, card) = board_with_a_card(handle, &ws, cx);
    let shown = |cx: &mut TestAppContext, what: &str| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let id = gpui_kit::ElementId::Name(format!("{what}-{}-{card}", block.0).into());
            window.try_find(id).is_some()
        })
        .unwrap()
    };
    assert!(shown(cx, "card-id"));
    assert!(!shown(cx, "priority") && !shown(cx, "checklist"));
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            b.edit_card(board, card, |c| c.priority = Some(Priority::High));
            b.add_items(board, card, &[("a".into(), false), ("b".into(), true)]);
        })
    });
    assert!(shown(cx, "priority") && shown(cx, "checklist"));
    ws.read_with(cx, |ws, cx| {
        let uid = ws.space.uids[&terminal].clone();
        let label = |a: Assignee| crate::kanban::assignee_label(ws, &a, cx);
        assert_eq!(label(Assignee::Agent("codex".into())), "Any Codex");
        assert_eq!(
            label(Assignee::Block("zzzzz".into())),
            "zzzzz",
            "no workspace has it"
        );
        assert_eq!(
            label(Assignee::Block(uid.clone())),
            ws.uid_title(&uid, cx).unwrap()
        );
    });
}

#[gpui_kit::test]
fn the_open_card_sets_a_priority_has_delete_left_of_done_and_survives_its_card(
    cx: &mut TestAppContext,
) {
    use crate::board::Priority;
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (_, board, card) = board_with_a_card(handle, &ws, cx);
    let open_it = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let current = ws.boards.card(board, card).cloned().unwrap();
                crate::kanban::open_card(board, current, false, window, cx)
            });
            window.render_frame(cx);
        })
        .unwrap()
    };
    let priority = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.boards.card(board, card).unwrap().priority)
    };
    open_it(cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("card-number").is_some());
        window.click("priority-high", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(priority(cx), Some(Priority::High));
    ws.update(cx, |ws, cx| assert!(ws.undo_boards(cx)));
    assert_eq!(priority(cx), None, "one Ctrl+Z step");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("priority-medium", cx);
        window.render_frame(cx);
        // The fix of the misplaced button: Delete card at the footer's left, on Done's row.
        let (delete, done) = (
            window.find("delete-card").bounds(),
            window.find("card-done").bounds(),
        );
        assert!(delete.right() < done.left(), "{delete:?} {done:?}");
        assert!(
            (delete.center().y - done.center().y).abs() < px(4.0),
            "one row"
        );
        window.click("card-done", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
    })
    .unwrap();
    assert_eq!(priority(cx), Some(Priority::Medium));
    // Deleted meanwhile (an agent, another block): the dialog still draws and closes.
    open_it(cx);
    ws.update(cx, |ws, _| ws.edit_boards(|b| b.remove_card(board, card)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("card-done", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_long_open_card_keeps_its_footer_on_screen(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (_, board, card) = board_with_a_card(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            for i in 0..30 {
                let text = format!("Comment {i}\nwith a second line");
                b.add_comment(board, card, "you", &text, 0);
            }
        })
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, card).cloned().unwrap();
            crate::kanban::open_card(board, current, false, window, cx)
        });
        window.render_frame(cx);
        let screen = window.viewport_size();
        let done = window.find("card-done");
        assert!(done.visible(), "{:?}", done.bounds());
        assert!(
            done.bounds().bottom() <= screen.height,
            "{:?} {screen:?}",
            done.bounds()
        );
        assert!(window.find("card-number").bounds().top() >= px(0.0));
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_open_card_adds_checks_and_deletes_checklist_items(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (_, board, card) = board_with_a_card(handle, &ws, cx);
    let items = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let c = ws.boards.card(board, card).unwrap();
            c.checklist
                .iter()
                .map(|i| (i.id, i.text.clone(), i.done))
                .collect::<Vec<_>>()
        })
    };
    let el = |what: &str, id: u64| gpui_kit::ElementId::Name(format!("{what}-{id}").into());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, card).cloned().unwrap();
            crate::kanban::open_card(board, current, false, window, cx)
        });
        window.render_frame(cx);
        window.click("card-item-field", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_input(handle, "Build");
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.simulate_input(handle, "Test");
    cx.update_window(handle, |_, window, cx| {
        window.press("enter", cx);
        assert!(window.has_active_dialog(cx), "Enter adds, it doesn't close");
    })
    .unwrap();
    let now = items(cx);
    assert_eq!(
        now.iter()
            .map(|(_, t, d)| (t.as_str(), *d))
            .collect::<Vec<_>>(),
        [("Build", false), ("Test", false)],
        "the field empties after each"
    );
    let (build, test) = (now[0].0, now[1].0);
    cx.update_window(handle, |_, window, cx| {
        window.click(el("card-item-box", build), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(items(cx)[0].2);
    ws.update(cx, |ws, cx| assert!(ws.undo_boards(cx)));
    assert!(!items(cx)[0].2, "one Ctrl+Z step");
    // An agent's item shows while the dialog is open.
    let theirs = ws.update(cx, |ws, _| {
        ws.edit_boards(|b| b.add_items(board, card, &[("Docs".into(), false)]))
    })[0];
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(el("card-item-box", theirs)).is_some());
        // The dialog grew and re-centred: the pointer, left on Build's box, may now be on Test.
        window.hover("card-number", cx);
        window.render_frame(cx);
        assert!(
            !window.find(el("card-item-delete", test)).visible(),
            "under the pointer only"
        );
        window.hover(el("card-item-row", test), cx);
        window.render_frame(cx);
        window.click(el("card-item-delete", test), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        items(cx).iter().map(|i| i.0).collect::<Vec<_>>(),
        [build, theirs]
    );
}

#[gpui_kit::test]
fn the_assignee_menu_offers_the_agent_sessions_wired_to_the_board(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let main = first_id(&ws, cx);
    let (kanban, board, _) = board_with_a_card(handle, &ws, cx);
    let main_uid = ws.update(cx, |ws, cx| {
        ws.space.terminal_mut(main).unwrap().seen.agent = Some("claude".into());
        assert!(ws.connect(main, kanban, cx));
        ws.space.uids[&main].clone()
    });
    let other_uid = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_terminal(window, cx);
                let loose = ws.space.canvas.top().unwrap();
                ws.space.terminal_mut(loose).unwrap().flags_for = Some("codex".into()); // not wired
                // Another workspace's kanban block shows the same board.
                ws.new_space(window, cx);
                ws.add_kanban_at(Point::new(900.0, 0.0), window, cx);
                let shows = ws.space.canvas.top().unwrap();
                ws.show_board(shows, board, cx);
                ws.new_terminal(window, cx);
                let id = ws.space.canvas.top().unwrap();
                ws.space.terminal_mut(id).unwrap().flags_for = Some("codex".into());
                assert!(ws.connect(id, shows, cx));
                ws.new_terminal(window, cx); // runs no agent: not offered
                ws.space.uids[&id].clone()
            })
        })
        .unwrap();
    ws.read_with(cx, |ws, cx| {
        let sessions = ws.agent_sessions(board, cx);
        let ids: Vec<&str> = sessions.iter().map(|(uid, _)| uid.as_str()).collect();
        assert_eq!(
            ids,
            [other_uid.as_str(), main_uid.as_str()],
            "the wired ones, the active workspace first"
        );
        let title = ws.uid_title(&other_uid, cx).unwrap();
        assert!(sessions[0].1.starts_with(&title), "{}", sessions[0].1);
        assert!(
            sessions[0].1.ends_with(&format!(" · {other_uid}")),
            "{}",
            sessions[0].1
        );
        assert!(
            sessions[1].1.ends_with(&format!(" · {main_uid} — Main")),
            "{}",
            sessions[1].1
        );
        assert!(
            ws.agent_sessions(board + 1, cx).is_empty(),
            "another board's"
        );
    });
}

#[gpui_kit::test]
fn a_session_chosen_in_the_assignee_menu_is_one_step_and_the_dialog_stays(cx: &mut TestAppContext) {
    use crate::board::Assignee;
    no_motion(cx);
    let (handle, ws) = open(cx);
    let main = first_id(&ws, cx);
    let uid = ws.update(cx, |ws, _| {
        ws.space.terminal_mut(main).unwrap().seen.agent = Some("claude".into());
        ws.space.uids[&main].clone()
    });
    let (kanban, board, card) = board_with_a_card(handle, &ws, cx);
    ws.update(cx, |ws, cx| assert!(ws.connect(main, kanban, cx)));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, card).cloned().unwrap();
            crate::kanban::open_card(board, current, false, window, cx)
        });
        window.render_frame(cx);
        window.click("card-assignee", cx);
        window.render_frame(cx);
        // Anyone, a separator, "Sessions", then the one session.
        window.within("popup-menu").click(3usize, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "the dialog stays open");
    })
    .unwrap();
    let assignee = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.boards.card(board, card).unwrap().assignee.clone()
        })
    };
    assert_eq!(assignee(cx), Some(Assignee::Block(uid)));
    ws.update(cx, |ws, cx| assert!(ws.undo_boards(cx)));
    assert_eq!(assignee(cx), None, "one Ctrl+Z step");
}

/// Presses on `from`, moves 10 px down (the drag starts) and on to `to`, and holds it there:
/// three frames 100 ms apart, the pointer still.
fn hold_drag(
    handle: AnyWindowHandle,
    cx: &mut TestAppContext,
    from: gpui_kit::Point<gpui_kit::Pixels>,
    to: gpui_kit::Point<gpui_kit::Pixels>,
) {
    let moved = |position| {
        PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        })
    };
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(moved(from + point(px(0.0), px(10.0))), cx);
        window.dispatch_event(moved(to), cx);
        window.render_frame(cx);
    })
    .unwrap();
    for _ in 0..2 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
}

fn release(
    handle: AnyWindowHandle,
    cx: &mut TestAppContext,
    at: gpui_kit::Point<gpui_kit::Pixels>,
) {
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: Modifiers::default(),
                click_count: 1,
            }),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn dragging_a_card_near_a_long_columns_bottom_scrolls_its_cards(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (block, board, first) = board_with_a_card(handle, &ws, cx);
    let n = block.0;
    let (todo, cards) = ws.update(cx, |ws, _| {
        let todo = ws.boards.board(board).unwrap().columns[0].id;
        let cards: Vec<u64> = (0..29)
            .map(|_| ws.edit_boards(|b| b.add_card(board, todo)).unwrap())
            .collect();
        (todo, cards)
    });
    let el = |what: &str, id: u64| gpui_kit::ElementId::Name(format!("{what}-{n}-{id}").into());
    let (from, to, top) = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let list = window.find(el("cards", todo)).bounds();
            let card = window.find(el("card", first)).bounds();
            (
                card.center(),
                point(list.center().x, list.bottom() - px(8.0)),
                card.top(),
            )
        })
        .unwrap();
    hold_drag(handle, cx, from, to);
    let now = |cx: &mut TestAppContext, card: u64| {
        cx.update_window(handle, |_, window, _| {
            window.find(el("card", card)).bounds().top()
        })
        .unwrap()
    };
    assert!(now(cx, first) < top, "the cards scrolled up");
    release(handle, cx, to);
    let before = now(cx, cards[3]);
    std::thread::sleep(std::time::Duration::from_millis(100));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert_eq!(now(cx, cards[3]), before, "scrolling stopped with the drag");
}

/// A board with a To do column of 30 cards, the canvas `steps` zoom steps from 100% (instant):
/// the pointer goes to `from_right` pixels inside the right edge of that column's list, near its
/// bottom, and presses there. Returns how far the first card moved up, in pixels.
fn press_inside_the_lists_right_edge(
    cx: &mut TestAppContext,
    steps: i32,
    from_right: f32,
) -> (f32, f32) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (block, board, first) = board_with_a_card(handle, &ws, cx);
    let n = block.0;
    let todo = ws.update(cx, |ws, _| {
        let todo = ws.boards.board(board).unwrap().columns[0].id;
        for _ in 0..29 {
            ws.edit_boards(|b| b.add_card(board, todo)).unwrap();
        }
        todo
    });
    let el = |what: &str, id: u64| gpui_kit::ElementId::Name(format!("{what}-{n}-{id}").into());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let card = window.find(el("card", first)).bounds().center();
        ws.update(cx, |ws, cx| {
            ws.zoom_to(steps, Point::new(f32::from(card.x), f32::from(card.y)), cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    // The camera glides to its zoom; the bounds are read once it landed.
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    let list = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.find(el("cards", todo)).bounds()
        })
        .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.space.flight.is_none(), "landed"));
    let top = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.find(el("card", first)).bounds().top()
        })
        .unwrap()
    };
    let (z, before) = (ws.read_with(cx, |ws, _| ws.space.view.zoom()), top(cx));
    // The scrollbar covers the list, and the cards overflow it.
    let bar = cx
        .update_window(handle, |_, window, _| {
            window.find(el("scrollbar", todo)).bounds()
        })
        .unwrap();
    assert_eq!(bar, list, "the scrollbar is over the list");
    let at = point(list.right() - px(from_right), list.bottom() - px(12.0 * z));
    press(handle, cx, at);
    (z, f32::from(before - top(cx)))
}

/// The pointer moves to `at` and clicks there: over a scrollbar's track, it is shown first.
fn press(handle: AnyWindowHandle, cx: &mut TestAppContext, at: gpui_kit::Point<gpui_kit::Pixels>) {
    cx.update_window(handle, |_, window, cx| {
        let moved = PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Modifiers::default(),
        });
        window.dispatch_event(moved, cx);
        window.render_frame(cx);
        let down = MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        };
        window.dispatch_event(PlatformInput::MouseDown(down), cx);
        window.render_frame(cx);
    })
    .unwrap();
    release(handle, cx, at);
}

/// A column whose cards overflow has a scrollbar over its list, which pressing on scrolls.
#[gpui_kit::test]
fn a_long_column_has_a_scrollbar_that_scrolls_its_cards(cx: &mut TestAppContext) {
    let (z, moved) = press_inside_the_lists_right_edge(cx, 0, 6.0);
    assert_eq!(z, 1.0);
    assert!(
        moved > 100.0,
        "pressing on the track scrolled the cards: {moved}"
    );
}

/// The track scales with the zoom, like everything in the block: at half size it is 8 pixels
/// wide, so 12 pixels in is a card, not the track, and 3 pixels in is the track.
#[gpui_kit::test]
fn the_scrollbar_scales_with_the_zoom(cx: &mut TestAppContext) {
    let (z, moved) = press_inside_the_lists_right_edge(cx, -4, 3.0);
    assert_eq!(z, 0.5);
    assert!(moved > 50.0, "on the half-size track: {moved}");
    let (_, moved) = press_inside_the_lists_right_edge(cx, -4, 12.0);
    assert_eq!(moved, 0.0, "past the half-size track, over a card");
}

/// A card dragged over another long column's scrollbar, near the top of that column scrolled to
/// its end, scrolls the cards back up: the scrollbar over the list takes no part in a drag.
#[gpui_kit::test]
fn a_card_dragged_over_a_scrolled_columns_scrollbar_scrolls_it_back_up(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (block, board, first) = board_with_a_card(handle, &ws, cx);
    let n = block.0;
    let (doing, cards) = ws.update(cx, |ws, _| {
        let doing = ws.boards.board(board).unwrap().columns[1].id;
        let cards: Vec<u64> = (0..30)
            .map(|_| ws.edit_boards(|b| b.add_card(board, doing)).unwrap())
            .collect();
        (doing, cards)
    });
    let el = |what: &str, id: u64| gpui_kit::ElementId::Name(format!("{what}-{n}-{id}").into());
    let bounds = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.find(el("cards", doing)).bounds()
        })
        .unwrap();
    let top = |cx: &mut TestAppContext, card: u64| {
        cx.update_window(handle, |_, window, _| {
            window.find(el("card", card)).bounds().top()
        })
        .unwrap()
    };
    // Doing is scrolled to its end by a press on its track, near the bottom.
    press(
        handle,
        cx,
        point(bounds.right() - px(6.0), bounds.bottom() - px(12.0)),
    );
    let scrolled = top(cx, cards[0]);
    assert!(
        scrolled < bounds.top() - px(100.0),
        "the cards of Doing scrolled up"
    );
    // The first card of To do is held at the track's strip, at the top of Doing.
    let from = cx
        .update_window(handle, |_, window, _| {
            window.find(el("card", first)).bounds().center()
        })
        .unwrap();
    let to = point(bounds.right() - px(6.0), bounds.top() + px(8.0));
    hold_drag(handle, cx, from, to);
    assert!(
        top(cx, cards[0]) > scrolled,
        "dragging at the top scrolled Doing back up"
    );
    release(handle, cx, to);
}

/// A kanban block with a To do column of 30 cards, whose list has a scrollbar.
struct LongTodo {
    handle: AnyWindowHandle,
    ws: Entity<Workspace>,
    block: BlockId,
    board: u64,
    todo: u64,
    first: u64,
}

impl LongTodo {
    fn new(cx: &mut TestAppContext) -> Self {
        no_motion(cx);
        let (handle, ws) = open(cx);
        let (block, board, first) = board_with_a_card(handle, &ws, cx);
        let todo = ws.update(cx, |ws, _| {
            let todo = ws.boards.board(board).unwrap().columns[0].id;
            for _ in 0..29 {
                ws.edit_boards(|b| b.add_card(board, todo)).unwrap();
            }
            todo
        });
        Self {
            handle,
            ws,
            block,
            board,
            todo,
            first,
        }
    }

    fn el(&self, what: &str, id: u64) -> gpui_kit::ElementId {
        gpui_kit::ElementId::Name(format!("{what}-{}-{id}", self.block.0).into())
    }

    /// The list's bounds, as drawn now.
    fn list(&self, cx: &mut TestAppContext) -> gpui_kit::Bounds<gpui_kit::Pixels> {
        cx.update_window(self.handle, |_, window, cx| {
            window.render_frame(cx);
            window.find(self.el("cards", self.todo)).bounds()
        })
        .unwrap()
    }

    /// The first card's top: it moves up as the list scrolls.
    fn top(&self, cx: &mut TestAppContext) -> f32 {
        cx.update_window(self.handle, |_, window, cx| {
            window.render_frame(cx);
            f32::from(window.find(self.el("card", self.first)).bounds().top())
        })
        .unwrap()
    }

    /// Inside the scrollbar's track (16 pixels wide) and the first card (which ends 6 pixels
    /// short of the list's edge).
    fn strip_over_the_first_card(&self, cx: &mut TestAppContext) -> (f32, f32) {
        let list = self.list(cx);
        let card = cx
            .update_window(self.handle, |_, window, _| {
                window.find(self.el("card", self.first)).bounds()
            })
            .unwrap();
        (
            f32::from(list.right() - px(10.0)),
            f32::from(card.center().y),
        )
    }
}

/// The pointer moves to `at` and a frame is drawn: a scrollbar over its track is shown.
fn hover_at(window: &mut Window, cx: &mut gpui_kit::App, at: (f32, f32)) {
    window.dispatch_event(
        PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
            position: point(px(at.0), px(at.1)),
            pressed_button: None,
            modifiers: Modifiers::default(),
        }),
        cx,
    );
    window.render_frame(cx);
}

/// A long column's scrollbar takes the presses on its strip, but a right-click on a card there
/// still opens the card's menu.
#[gpui_kit::test]
fn a_right_click_on_a_long_columns_scrollbar_strip_opens_the_cards_menu(cx: &mut TestAppContext) {
    let long = LongTodo::new(cx);
    let at = long.strip_over_the_first_card(cx);
    cx.update_window(long.handle, |_, window, cx| {
        hover_at(window, cx, at);
        press_with(window, cx, MouseButton::Right, Modifiers::default(), at, at);
        window.render_frame(cx);
        // Open, Send to ▸, —, Delete card.
        window.within("popup-menu").click(3usize, cx);
    })
    .unwrap();
    // The menu fades out for a moment: a click meanwhile would land on it.
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update_window(long.handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "Delete card asks");
        window.within("dialog").click("ok", cx);
    })
    .unwrap();
    long.ws.read_with(cx, |ws, _| {
        assert!(ws.boards.card(long.board, long.first).is_none(), "deleted")
    });
}

/// The same strip lets a middle-button drag pan the canvas, as the rest of the block does.
#[gpui_kit::test]
fn a_middle_drag_from_a_long_columns_scrollbar_strip_pans_the_canvas(cx: &mut TestAppContext) {
    let long = LongTodo::new(cx);
    let at = long.strip_over_the_first_card(cx);
    let before = long.ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    cx.update_window(long.handle, |_, window, cx| {
        hover_at(window, cx, at);
        let to = (at.0 - 50.0, at.1);
        press_with(
            window,
            cx,
            MouseButton::Middle,
            Modifiers::default(),
            at,
            to,
        );
    })
    .unwrap();
    long.ws.read_with(cx, |ws, _| {
        assert_ne!(ws.space.canvas.camera, before, "a middle drag pans");
        assert!(ws.drag.is_none());
    });
}

#[gpui_kit::test]
fn dragging_a_card_near_the_boards_right_edge_scrolls_its_columns(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (block, board, card) = board_with_a_card(handle, &ws, cx);
    let n = block.0;
    let last = ws.update(cx, |ws, _| {
        (0..4)
            .map(|_| ws.edit_boards(|b| b.add_column(board)).unwrap())
            .last()
            .unwrap()
    });
    let el = |what: &str, id: u64| gpui_kit::ElementId::Name(format!("{what}-{n}-{id}").into());
    let (from, to, left) = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let row = window.find(("columns", n as usize)).bounds();
            (
                window.find(el("card", card)).bounds().center(),
                point(row.right() - px(8.0), row.center().y),
                window.find(el("column", last)).bounds().left(),
            )
        })
        .unwrap();
    hold_drag(handle, cx, from, to);
    let now = cx
        .update_window(handle, |_, window, _| {
            window.find(el("column", last)).bounds().left()
        })
        .unwrap();
    assert!(now < left, "the columns scrolled left: {now:?} {left:?}");
    release(handle, cx, to);
}

#[gpui_kit::test]
fn a_card_dragged_in_one_block_never_scrolls_the_block_beside_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (b, board, card) = board_with_a_card(handle, &ws, cx);
    // Block `a` shows the same wide board, 8 to the left of `b`.
    let (a, last) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let at = ws.space.canvas.get(b).unwrap().rect;
                ws.add_kanban_at(Point::new(at.x - at.w - 8.0, at.y), window, cx);
                let a = ws.space.canvas.top().unwrap();
                ws.show_board(a, board, cx);
                let last = (0..4)
                    .map(|_| ws.edit_boards(|b| b.add_column(board)).unwrap())
                    .last()
                    .unwrap();
                ws.space.appearing.clear();
                (a, last)
            })
        })
        .unwrap();
    let el = |what: &str, block: BlockId, id: u64| {
        gpui_kit::ElementId::Name(format!("{what}-{}-{id}", block.0).into())
    };
    let (from, to, left) = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let beside = window.find(("columns", a.0 as usize)).bounds();
            let row = window.find(("columns", b.0 as usize)).bounds();
            let to = point(row.left() + px(4.0), row.center().y);
            assert!(to.x - beside.right() < px(40.0), "within a's edge zone");
            (
                window.find(el("card", b, card)).bounds().center(),
                to,
                window.find(el("column", a, last)).bounds().left(),
            )
        })
        .unwrap();
    hold_drag(handle, cx, from, to);
    let now = cx
        .update_window(handle, |_, window, _| {
            window.find(el("column", a, last)).bounds().left()
        })
        .unwrap();
    assert_eq!(now, left, "the block beside stayed put");
    release(handle, cx, to);
}

#[gpui_kit::test]
fn lines_pasted_into_add_an_item_make_an_item_each(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (_, board, card) = board_with_a_card(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, card).cloned().unwrap();
            crate::kanban::open_card(board, current, false, window, cx)
        });
        window.render_frame(cx);
        window.click("card-item-field", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
        "- Build\n- [x] Test\nShip".into(),
    ));
    cx.update_window(handle, |_, window, cx| {
        window.press(PASTE, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
    })
    .unwrap();
    let items = ws.read_with(cx, |ws, _| {
        let c = ws.boards.card(board, card).unwrap();
        c.checklist
            .iter()
            .map(|i| (i.text.clone(), i.done))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        items,
        [
            ("Build".to_string(), false),
            ("Test".to_string(), true),
            ("Ship".to_string(), false)
        ]
    );
}

#[gpui_kit::test]
fn the_assignee_menu_lists_a_gone_block_first_and_every_agent_in_its_submenu(
    cx: &mut TestAppContext,
) {
    use crate::board::Assignee;
    no_motion(cx);
    let (handle, ws) = open(cx);
    let main = first_id(&ws, cx);
    let uid = ws.update(cx, |ws, _| {
        ws.space.terminal_mut(main).unwrap().seen.agent = Some("claude".into());
        ws.space.uids[&main].clone()
    });
    let (kanban, board, card) = board_with_a_card(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        assert!(ws.connect(main, kanban, cx));
        ws.edit_boards(|b| {
            b.edit_card(board, card, |c| {
                c.assignee = Some(Assignee::Block("zzzzz".into()))
            })
        })
    });
    let assignee = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.boards.card(board, card).unwrap().assignee.clone()
        })
    };
    let choose = |cx: &mut TestAppContext, pick: &dyn Fn(&mut Window, &mut gpui_kit::App)| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let current = ws.boards.card(board, card).cloned().unwrap();
                crate::kanban::open_card(board, current, false, window, cx)
            });
            window.render_frame(cx);
            window.click("card-assignee", cx);
            window.render_frame(cx);
            pick(window, cx);
            window.render_frame(cx);
            window.click("card-done", cx);
            window.render_frame(cx);
        })
        .unwrap()
    };
    // Anyone, a separator, "Sessions", the gone block, then the one session.
    choose(cx, &|window, cx| {
        window.within("popup-menu").click(4usize, cx)
    });
    assert_eq!(assignee(cx), Some(Assignee::Block(uid)));
    // Then "Any session of" (a separator before it has no index), whose submenu lists every
    // agent in order.
    let second = ws.read_with(cx, |ws, _| ws.agents[1].id.clone());
    choose(cx, &|window, cx| {
        window.within("popup-menu").hover(5usize, cx);
        window.render_frame(cx);
        window.within("submenu").click(1usize, cx);
    });
    assert_eq!(assignee(cx), Some(Assignee::Agent(second)));
}
