//! Blocks on the canvas: opening, closing, moving, resizing, snapping and focus.

use super::*;

#[gpui_kit::test]
fn starts_with_one_focused_terminal(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 1);
        assert_eq!(ws.space.focused, ws.space.canvas.top());
        assert_eq!(ws.space.terminals().count(), 1);
    });
}

#[gpui_kit::test]
fn blocks_of_any_kind_share_one_map(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.new_kanban(window, cx));
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let kanban = ws.space.canvas.top().unwrap();
        assert_eq!(ws.space.blocks.len(), 2);
        assert!(ws.space.terminal(terminal).is_some());
        assert!(ws.space.kanban(terminal).is_none());
        assert!(ws.space.kanban(kanban).is_some());
        assert!(ws.space.terminal(kanban).is_none());
        assert_eq!(ws.space.terminals().count(), 1);
        assert_eq!(ws.space.focused, Some(kanban));
        assert!(ws.space.focused_terminal().is_none());
    });
}

#[gpui_kit::test]
fn startup_terminal_stays_centered_when_the_window_is_resized(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // Tiling compositors resize the window right after it opens.
    cx.simulate_window_resize(handle, size(px(600.), px(400.)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Measured, not computed with viewport(): the canvas area starts at the workspace's
        // origin (below the title bar) and runs to the window's edge.
        let full = window.viewport_size();
        let ws = ws.read(cx);
        let vp = templum_canvas::Size::new(
            f32::from(full.width - ws.origin.x),
            f32::from(full.height - ws.origin.y),
        );
        let b = ws.space.canvas.blocks()[0].rect;
        let center = ws
            .space
            .canvas
            .camera
            .to_screen(templum_canvas::Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0));
        assert!(
            (center.x - vp.w / 2.0).abs() < 1.0 && (center.y - vp.h / 2.0).abs() < 1.0,
            "startup block center on screen: {center:?}, canvas {vp:?}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn shortcuts_open_and_close_terminals(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 2);
        assert_eq!(
            ws.space.focused,
            ws.space.canvas.top(),
            "the new terminal is focused"
        );
        // The startup block already sits at the viewport center: the new one cascades.
        let [first, second] = ws.space.canvas.blocks() else {
            unreachable!()
        };
        assert_eq!(
            (second.rect.x - first.rect.x, second.rect.y - first.rect.y),
            (templum_canvas::CASCADE, templum_canvas::CASCADE)
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(CLOSE, cx);
        window.press(CLOSE, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.canvas.blocks().is_empty());
        assert!(ws.space.terminals().next().is_none());
        assert_eq!(ws.space.focused, None);
    });
}

#[gpui_kit::test]
fn double_click_on_empty_canvas_opens_a_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // The canvas starts below the title bar.
        let o = ws.read(cx).origin;
        let at = (f32::from(o.x) + 40.0, f32::from(o.y) + 40.0);
        click(window, cx, at, 1);
        click(window, cx, at, 2);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 2);
        let new = ws.space.canvas.get(ws.space.canvas.top().unwrap()).unwrap();
        assert_eq!((new.rect.x, new.rect.y), (40.0, 40.0));
    });
}

#[gpui_kit::test]
fn close_button_removes_the_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.hover(("block", id.0 as usize), cx);
        window.render_frame(cx);
        window.click(("close", id.0 as usize), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.space.canvas.blocks().is_empty()));
}

#[gpui_kit::test]
fn dragging_the_title_moves_the_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let before = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(("title", id.0 as usize)).bounds().center();
        window.drag(from, point(from.x + px(100.), from.y + px(50.)), cx);
    })
    .unwrap();
    let after = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    assert_eq!((after.x - before.x, after.y - before.y), (100.0, 50.0));
    assert_eq!((after.w, after.h), (before.w, before.h));
}

#[gpui_kit::test]
fn a_dragged_block_snaps_to_a_neighbour_and_shows_a_guide_until_release(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    // A small second block over the first one's top, its horizontal lines (30, 80 and 130
    // below the first's top) clear of the first's (0, half its height, its height).
    let second = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let r = ws.space.canvas.get(first).unwrap().rect;
                ws.open(Rect::new(r.x + 300.0, r.y + 30.0, 200.0, 100.0), window, cx);
                ws.space.appearing.clear();
                ws.space.canvas.top().unwrap()
            })
        })
        .unwrap();
    let (first_rect, second_rect) = ws.read_with(cx, |ws, _| {
        (
            ws.space.canvas.get(first).unwrap().rect,
            ws.space.canvas.get(second).unwrap().rect,
        )
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(("title", second.0 as usize)).bounds().center();
        // Left, so the second's left edge lands 5 px right of the first's: it snaps there.
        let to = point(from.x + px(first_rect.x - second_rect.x + 5.0), from.y);
        let modifiers = Modifiers::default();
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                modifiers,
            }),
            cx,
        );
        let ws_now = ws.read(cx);
        assert_eq!(
            ws_now.space.canvas.get(second).unwrap().rect.x,
            first_rect.x
        );
        assert_eq!(ws_now.space.guides.len(), 1, "a vertical guide");
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
        assert!(ws.read(cx).space.guides.is_empty(), "gone on release");
    })
    .unwrap();
}

#[gpui_kit::test]
fn dragging_the_handle_resizes_block_and_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let (before, cell) = ws.read_with(cx, |ws, _| {
        (ws.space.canvas.get(id).unwrap().rect, ws.cell.size)
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.hover(("block", id.0 as usize), cx);
        window.render_frame(cx);
        let from = window.find(("handle", id.0 as usize)).bounds().center();
        let to = point(from.x + px(cell.w * 10.0), from.y + px(cell.h * 3.0));
        window.drag(from, to, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let rect = ws.space.canvas.get(id).unwrap().rect;
        assert!(rect.w > before.w && rect.h > before.h);
        let expected = templum_canvas::chrome::grid_size(
            templum_canvas::chrome::content_rect(rect).size(),
            cell,
        );
        assert_eq!(ws.space.terminal(id).unwrap().term.grid_size(), expected);
        assert_eq!(
            expected,
            (
                crate::workspace::DEFAULT_COLS + 10,
                crate::workspace::DEFAULT_ROWS + 3
            )
        );
    });
}

#[gpui_kit::test]
fn close_button_appears_on_hover(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(("close", id.0 as usize)).is_none(),
            "hidden until hovered"
        );
        window.hover(("block", id.0 as usize), cx);
        window.render_frame(cx);
        assert!(window.try_find(("close", id.0 as usize)).is_some());
        assert!(window.try_find(("handle", id.0 as usize)).is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn new_terminals_pop_in_then_settle(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
        let ws = ws.read(cx);
        let new = ws.space.canvas.top().unwrap();
        assert!(
            ws.space.appearing.contains_key(&new),
            "the new block is animating"
        );
    })
    .unwrap();
    std::thread::sleep(crate::motion::OPEN + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.appearing.is_empty(), "settled")
    });
}

#[gpui_kit::test]
fn a_closed_block_fades_as_a_ghost_that_ignores_clicks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let n = first_id(&ws, cx).0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(CLOSE, cx);
        // The fade starts now, however long a slow machine took since the key: the frame below
        // must still draw the ghost.
        ws.update(cx, |ws, _| {
            for ghost in &mut ws.space.ghosts {
                ghost.since = std::time::Instant::now();
            }
        });
        window.render_frame(cx);
        // Checked right after the frame that drew it: the ghost lives 0.18 s, and a slow
        // machine may spend longer than that on the clicks below.
        assert_eq!(ws.read(cx).space.ghosts.len(), 1);
        // A double click where the ghost is reaches the canvas: a terminal opens there.
        let at = window.find(("ghost", n)).bounds().center();
        let at = (f32::from(at.x), f32::from(at.y));
        click(window, cx, at, 1);
        click(window, cx, at, 2);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.canvas.blocks().len(),
            1,
            "the click went through the ghost"
        );
    });
    std::thread::sleep(crate::motion::CLOSE + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.ghosts.is_empty(), "faded away")
    });
}

#[gpui_kit::test]
fn clicking_the_empty_canvas_unfocuses_terminals(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        click(
            window,
            cx,
            (f32::from(o.x) + 40.0, f32::from(o.y) + 40.0),
            1,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, None));
}
