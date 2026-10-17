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
fn a_double_click_on_the_empty_canvas_opens_nothing(cx: &mut TestAppContext) {
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
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.blocks().len(), 1));
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
fn dragging_an_edge_or_a_corner_resizes_keeping_the_opposite_sides(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0;
    let rect =
        |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    let drag = |cx: &mut TestAppContext, zone: &str, by: (f32, f32)| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.hover(("block", n as usize), cx);
            window.render_frame(cx);
            let el = gpui_kit::ElementId::Name(format!("resize-{zone}-{n}").into());
            let from = window.find(el).bounds().center();
            let to = point(from.x + px(by.0), from.y + px(by.1));
            window.drag(from, to, cx);
        })
        .unwrap();
    };
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let left = gpui_kit::ElementId::Name(format!("resize-left-{n}").into());
        assert!(
            window.try_find(left).is_none(),
            "only on the block under the pointer"
        );
    })
    .unwrap();
    let before = rect(cx);
    // The left edge, 80 px to the left: wider, the right edge stays.
    drag(cx, "left", (-80.0, 30.0));
    let after = rect(cx);
    assert_eq!(after.x, before.x - 80.0);
    assert_eq!(after.right(), before.right());
    assert_eq!(
        (after.y, after.h),
        (before.y, before.h),
        "a side edge keeps the height"
    );
    let cell = ws.read_with(cx, |ws, _| ws.cell.size);
    let expected =
        templum_canvas::chrome::grid_size(templum_canvas::chrome::content_rect(after).size(), cell);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(id).unwrap().term.grid_size(), expected)
    });
    // The top-right corner, up and right: the bottom-left corner stays.
    let before = after;
    drag(cx, "top-right", (40.0, -50.0));
    let after = rect(cx);
    assert_eq!((after.x, after.bottom()), (before.x, before.bottom()));
    assert_eq!(after.right(), before.right() + 40.0);
    assert_eq!(after.y, before.y - 50.0);
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
        // A drag where the ghost is reaches the canvas: it pans.
        let at = window.find(("ghost", n)).bounds().center();
        let at = (f32::from(at.x), f32::from(at.y));
        let before = ws.read(cx).space.canvas.camera.origin.x;
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            at,
            (at.0 + 50.0, at.1),
        );
        assert_ne!(
            ws.read(cx).space.canvas.camera.origin.x,
            before,
            "the drag went through the ghost"
        );
    })
    .unwrap();
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

/// Closing a terminal an agent works in asks first; Cancel keeps it. An idle one closes at once.
#[gpui_kit::test]
fn closing_a_busy_terminal_asks_first(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().seen.agent = Some("claude".into())
    });
    cx.update_window(handle, |_, window, cx| {
        window.press(CLOSE, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "it asks");
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
        assert!(ws.read(cx).space.terminal(id).is_some(), "kept");
        window.press(CLOSE, cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        assert!(ws.read(cx).space.terminal(id).is_none(), "closed");
        // An idle one: no question.
        ws.update(cx, |ws, cx| {
            ws.add_terminal_at(Point::new(0.0, 0.0), window, cx);
            let idle = ws.space.canvas.top().unwrap();
            ws.close(idle, window, cx);
            assert!(ws.space.terminal(idle).is_none());
        });
        assert!(!window.has_active_dialog(cx));
    })
    .unwrap();
}

/// The pointer near a block's corner shows a dot on the corner, where a drag resizes it, that
/// fades in; away from the corner it fades out, and is gone once faded.
#[gpui_kit::test]
fn a_corner_under_the_pointer_shows_a_dot_that_fades(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let n = first_id(&ws, cx).0 as usize;
    let dot = || gpui_kit::ElementId::Name(format!("corner-dot-{n}").into());
    let zone = || gpui_kit::ElementId::Name(format!("resize-top-left-{n}").into());
    let near = || gpui_kit::ElementId::Name(format!("near-top-left-{n}").into());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.hover(("block", n), cx);
        window.render_frame(cx);
        assert!(window.try_find(dot()).is_none(), "away from its corners");
        window.hover(near(), cx);
        window.render_frame(cx);
        let at = window.find(dot()).bounds().center();
        assert!(
            window.find(zone()).bounds().contains(&at),
            "on the corner a drag resizes from"
        );
        let pointer = window.find(near()).bounds().center();
        assert!(
            !window.find(zone()).bounds().contains(&pointer),
            "shown before the pointer is on it"
        );
    })
    .unwrap();
    // Faded in.
    super::bus::after(cx, 200);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.hover(("title", n), cx);
        window.render_frame(cx);
        assert!(window.try_find(dot()).is_some(), "fading out");
    })
    .unwrap();
    super::bus::after(cx, 200);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(dot()).is_none(), "faded out");
    })
    .unwrap();
}
