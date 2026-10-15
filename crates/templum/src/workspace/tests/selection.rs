//! Selecting blocks (Shift+drag, Shift+click on a title), and what acts on them: a title's drag,
//! Delete, Copy and Paste; Ctrl+Z reopening closed blocks.

use super::*;
use crate::workspace::history::Then;

#[gpui_kit::test]
fn shift_drag_selects_the_blocks_it_touches_and_a_click_clears_them(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let (from, to) = (at(&ws, cx, (20.0, 20.0)), at(&ws, cx, (330.0, 100.0)));
    cx.update_window(handle, |_, window, cx| {
        press_with(window, cx, MouseButton::Left, SHIFT, from, to);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.selected, [kanban, note]);
        assert_eq!(ws.space.focused, None, "selecting takes the focus away");
        assert!(ws.drag.is_none());
    });
    // A plain drag pans and keeps them; a click on the empty canvas lets them go.
    let empty = at(&ws, cx, (800.0, 300.0));
    cx.update_window(handle, |_, window, cx| {
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            empty,
            (empty.0 + 30.0, empty.1),
        );
    })
    .unwrap();
    assert_eq!(ws.read_with(cx, |ws, _| ws.space.selected.len()), 2);
    cx.update_window(handle, |_, window, cx| click(window, cx, empty, 1))
        .unwrap();
    assert!(ws.read_with(cx, |ws, _| ws.space.selected.is_empty()));
}

#[gpui_kit::test]
fn shift_click_on_a_title_selects_a_block_and_again_lets_it_go(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, terminal) = three_blocks(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(terminal).unwrap().rect);
    let title = at(
        &ws,
        cx,
        (rect.x + rect.w / 2.0, rect.y + chrome::TITLE_H / 2.0),
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(kanban, window, cx));
        press_with(window, cx, MouseButton::Left, SHIFT, title, title);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.selected, [terminal]);
        assert_eq!(ws.space.focused, None);
        assert_eq!(
            ws.space.canvas.get(terminal).unwrap().rect,
            rect,
            "not moved"
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        press_with(window, cx, MouseButton::Left, SHIFT, title, title);
    })
    .unwrap();
    assert!(ws.read_with(cx, |ws, _| ws.space.selected.is_empty()));
}

#[gpui_kit::test]
fn delete_closes_the_focused_block_but_a_terminal_keeps_the_key(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(kanban, window, cx));
        window.press("delete", cx);
        ws.update(cx, |ws, cx| ws.focus_block(terminal, window, cx));
        window.press("delete", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.canvas.get(kanban).is_none());
        assert!(ws.space.canvas.get(note).is_some());
        assert!(ws.space.canvas.get(terminal).is_some());
    });
}

#[gpui_kit::test]
fn copy_paste_and_delete_selected_blocks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let (from, to) = (at(&ws, cx, (20.0, 20.0)), at(&ws, cx, (330.0, 100.0)));
    cx.update_window(handle, |_, window, cx| {
        press_with(window, cx, MouseButton::Left, SHIFT, from, to);
        window.press(COPY, cx);
        window.press(PASTE, cx);
    })
    .unwrap();
    let pasted = ws.read_with(cx, |ws, cx| {
        assert_eq!(ws.space.canvas.blocks().len(), 5);
        let pasted = ws.space.selected.clone();
        assert_eq!(pasted.len(), 2, "the pasted blocks are selected");
        assert!(!pasted.contains(&kanban) && !pasted.contains(&note));
        assert!(ws.space.kanban(pasted[0]).is_some());
        // A copy is another block for agents and wires (docs/specs/connections.md, Identity).
        assert_ne!(ws.space.uids[&pasted[0]], ws.space.uids[&kanban]);
        assert_eq!(ws.space.uids[&pasted[0]].len(), 5);
        let copy = ws.space.blocks[&pasted[1]].note().unwrap();
        assert_eq!(copy.text(cx), "hello");
        let original = ws.space.blocks[&note].note().unwrap();
        assert_ne!(
            copy.ids(),
            original.ids(),
            "a copy of the note, not the note"
        );
        // Kept side by side, somewhere else.
        let (a, b) = (
            ws.space.canvas.get(pasted[0]).unwrap().rect,
            ws.space.canvas.get(pasted[1]).unwrap().rect,
        );
        assert_eq!((b.x - a.x, b.y - a.y), (260.0, 0.0));
        assert_ne!(a, ws.space.canvas.get(kanban).unwrap().rect);
        pasted
    });
    cx.update_window(handle, |_, window, cx| window.press("delete", cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 3);
        assert!(pasted.iter().all(|id| ws.space.canvas.get(*id).is_none()));
        assert!(ws.space.selected.is_empty());
        assert!(ws.space.canvas.get(terminal).is_some());
    });
}

#[gpui_kit::test]
fn text_on_the_clipboard_is_not_pasted_as_blocks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    three_blocks(handle, &ws, cx);
    cx.update(|cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("ls\n".into())));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, _| ws.space.focused = None);
        window.press(PASTE, cx);
    })
    .unwrap();
    assert_eq!(ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len()), 3);
}

#[gpui_kit::test]
fn dragging_a_selected_title_moves_every_selected_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let before = ws.read_with(cx, |ws, _| {
        [kanban, note, terminal].map(|id| ws.space.canvas.get(id).unwrap().rect)
    });
    let (from, to) = (at(&ws, cx, (20.0, 20.0)), at(&ws, cx, (330.0, 100.0)));
    let title = at(&ws, cx, (150.0, 40.0 + chrome::TITLE_H / 2.0));
    cx.update_window(handle, |_, window, cx| {
        press_with(window, cx, MouseButton::Left, SHIFT, from, to);
        window.render_frame(cx);
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            title,
            (title.0 + 100.0, title.1 + 50.0),
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let moved = |id, r: Rect| {
            assert_eq!(
                ws.space.canvas.get(id).unwrap().rect,
                Rect::new(r.x + 100.0, r.y + 50.0, r.w, r.h)
            )
        };
        moved(kanban, before[0]);
        moved(note, before[1]);
        assert_eq!(ws.space.canvas.get(terminal).unwrap().rect, before[2]);
        assert_eq!(ws.space.selected, [kanban, note], "still selected");
        assert_eq!(ws.space.focused, None);
    });
}

#[gpui_kit::test]
fn ctrl_z_reopens_the_blocks_delete_closed_where_they_were(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let rects = ws.read_with(cx, |ws, _| {
        [kanban, note].map(|id| ws.space.canvas.get(id).unwrap().rect)
    });
    let (from, to) = (at(&ws, cx, (20.0, 20.0)), at(&ws, cx, (330.0, 100.0)));
    cx.update_window(handle, |_, window, cx| {
        press_with(window, cx, MouseButton::Left, SHIFT, from, to);
        window.press("delete", cx);
    })
    .unwrap();
    assert_eq!(ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len()), 1);
    cx.update_window(handle, |_, window, cx| window.press(UNDO, cx))
        .unwrap();
    ws.read_with(cx, |ws, cx| {
        let back = ws.space.selected.clone();
        assert_eq!(back.len(), 2, "reopened, selected");
        assert_eq!(ws.space.canvas.get(back[0]).unwrap().rect, rects[0]);
        assert_eq!(ws.space.canvas.get(back[1]).unwrap().rect, rects[1]);
        assert!(ws.space.kanban(back[0]).is_some());
        assert_eq!(ws.space.blocks[&back[1]].note().unwrap().text(cx), "hello");
        assert!(ws.space.undo.is_empty());
        assert_eq!(ws.space.redo.len(), 1, "Ctrl+Shift+Z closes them again");
    });
}

#[gpui_kit::test]
fn a_closed_block_leaves_no_focus_and_ctrl_z_never_reaches_past_a_terminal(
    cx: &mut TestAppContext,
) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        // As its close button does: the block is focused by the press, then closed.
        ws.update(cx, |ws, cx| {
            ws.focus_block(note, window, cx);
            ws.close(note, window, cx);
        });
        assert_eq!(
            ws.read(cx).space.focused,
            None,
            "the keyboard stays on the canvas"
        );
        // A focused terminal's Ctrl+Z is its shell's.
        ws.update(cx, |ws, cx| ws.focus_block(terminal, window, cx));
        window.press(UNDO, cx);
        assert_eq!(ws.read(cx).space.canvas.blocks().len(), 2);
        ws.update(cx, |ws, cx| ws.focus_block(kanban, window, cx));
        window.press(UNDO, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, cx| {
        assert_eq!(ws.space.canvas.blocks().len(), 3);
        let back = ws.space.selected[0];
        assert_eq!(ws.space.blocks[&back].note().unwrap().text(cx), "hello");
    });
}

#[gpui_kit::test]
fn a_reopened_terminal_resumes_its_agent_but_a_pasted_one_does_not(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, terminal) = three_blocks(handle, &ws, cx);
    let folder = tempfile::tempdir().unwrap();
    let agent_of = |ws: &Workspace, id: BlockId| ws.space.terminal(id).unwrap().seen.agent.clone();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let pane = ws.space.terminal_mut(terminal).unwrap();
            pane.seen.agent = Some("claude".into());
            pane.seen.folder = Some(folder.path().to_path_buf());
            ws.close_now(&[terminal], Then::New, window, cx);
            assert!(ws.undo_canvas(false, window, cx));
        });
        let back = ws.read(cx).space.selected[0];
        assert_eq!(agent_of(ws.read(cx), back).as_deref(), Some("claude"));
        window.press(COPY, cx);
        window.press(PASTE, cx);
        let pasted = ws.read(cx).space.selected[0];
        assert_ne!(pasted, back);
        assert_eq!(agent_of(ws.read(cx), pasted), None);
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_rectangle_held_near_the_edge_moves_the_camera_to_blocks_beyond_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    three_blocks(handle, &ws, cx);
    let (far, vp) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let vp = ws.viewport(window);
                let beyond = crate::session::Item::Kanban(crate::session::KanbanItem {
                    uid: String::new(),
                    rect: Rect::new(vp.w + 200.0, 60.0, 220.0, 200.0).into(),
                    board: None,
                });
                ws.quiet = true;
                let far = ws.add_saved(&beyond, window, cx).unwrap();
                ws.quiet = false;
                (far, vp)
            })
        })
        .unwrap();
    let (from, to) = (at(&ws, cx, (20.0, 20.0)), at(&ws, cx, (vp.w - 2.0, 100.0)));
    let event = |p: (f32, f32)| point(px(p.0), px(p.1));
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: event(from),
                modifiers: SHIFT,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: event(to),
                pressed_button: Some(MouseButton::Left),
                modifiers: SHIFT,
            }),
            cx,
        );
    })
    .unwrap();
    // The pointer stays still at the edge: frames keep moving the camera.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ws.read_with(cx, |ws, _| ws.space.selected.contains(&far)) {
        assert!(std::time::Instant::now() < deadline, "never reached");
        frame(handle, cx);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(ws.read_with(cx, |ws, _| ws.space.canvas.camera.origin.x) > 0.0);
    // Let go: the camera stops.
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: event(to),
                modifiers: SHIFT,
                click_count: 1,
            }),
            cx,
        );
    })
    .unwrap();
    let stopped = ws.read_with(cx, |ws, _| ws.space.canvas.camera.origin);
    frame(handle, cx);
    std::thread::sleep(std::time::Duration::from_millis(40));
    frame(handle, cx);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.space.canvas.camera.origin),
        stopped
    );
    assert!(ws.read_with(cx, |ws, _| ws.space.selected.contains(&far)));
}

#[gpui_kit::test]
fn a_shift_press_near_the_edge_leaves_the_camera_still(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    three_blocks(handle, &ws, cx);
    let vp = cx
        .update_window(handle, |_, window, cx| ws.read(cx).viewport(window))
        .unwrap();
    let edge = at(&ws, cx, (vp.w - 2.0, 100.0));
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: point(px(edge.0), px(edge.1)),
                modifiers: SHIFT,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
    })
    .unwrap();
    for _ in 0..3 {
        frame(handle, cx);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.space.canvas.camera.origin),
        Point::new(0.0, 0.0)
    );
}

/// While a title is held, its block (or every selected block, from a selected title) is moving:
/// the title shows a closed hand; let go, an open one again.
#[gpui_kit::test]
fn a_held_title_moves_its_block_until_let_go(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let title = at(&ws, cx, (150.0, 40.0 + chrome::TITLE_H / 2.0));
    let press = |cx: &mut TestAppContext, down: bool| {
        let position = point(px(title.0), px(title.1));
        cx.update_window(handle, |_, window, cx| {
            let event = if down {
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                })
            } else {
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                })
            };
            window.dispatch_event(event, cx);
            window.render_frame(cx);
        })
        .unwrap();
    };
    let moving = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| [kanban, note, terminal].map(|id| ws.moving(id)))
    };
    press(cx, true);
    assert_eq!(moving(cx), [true, false, false]);
    press(cx, false);
    assert_eq!(moving(cx), [false, false, false]);
    ws.update(cx, |ws, _| ws.space.selected = vec![kanban, note]);
    press(cx, true);
    assert_eq!(moving(cx), [true, true, false], "the selection moves");
    press(cx, false);
    assert_eq!(moving(cx), [false, false, false]);
}
