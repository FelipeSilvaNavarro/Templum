//! Kanban blocks and their boards.

use super::*;

fn top_kanban(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> BlockId {
    ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        assert!(ws.space.kanban(id).is_some(), "the top block is a kanban");
        id
    })
}

#[gpui_kit::test]
fn the_board_button_opens_a_block_with_the_board_list(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-kanban", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let block = top_kanban(&ws, cx);
    let n = block.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find(("board-list", n)).is_some());
        window.click(("new-board", n), cx);
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
                (a, b, board, todo)
            })
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Through the block on top: it covers most of the other.
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
            let list = ("board-list", block.0 as usize);
            assert!(window.try_find(list).is_some(), "back to the list");
        }
    })
    .unwrap();
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
