//! Drawing on the canvas: pen, highlighter, shapes, eraser, selection and undo.

use super::*;

#[cfg(target_os = "macos")]
const UNDO: &str = "cmd-z";
#[cfg(not(target_os = "macos"))]
const UNDO: &str = "ctrl-z";
#[cfg(target_os = "macos")]
const REDO: &str = "cmd-shift-z";
#[cfg(not(target_os = "macos"))]
const REDO: &str = "ctrl-shift-z";

#[test]
fn undo_keys_only_count_while_drawing_or_with_a_selection() {
    let key = |s: &str| gpui_kit::Keystroke::parse(s).unwrap();
    assert_eq!(
        crate::workspace::undo_key(&key("ctrl-z"), false),
        Some(false)
    );
    assert_eq!(
        crate::workspace::undo_key(&key("ctrl-shift-z"), false),
        Some(true)
    );
    assert_eq!(crate::workspace::undo_key(&key("cmd-z"), true), Some(false));
    assert_eq!(
        crate::workspace::undo_key(&key("cmd-shift-z"), true),
        Some(true)
    );
    assert_eq!(
        crate::workspace::undo_key(&key("ctrl-z"), true),
        None,
        "Cmd on macOS"
    );
    assert_eq!(crate::workspace::undo_key(&key("ctrl-alt-z"), false), None);
    assert_eq!(crate::workspace::undo_key(&key("z"), false), None);
    // The workspace asks only while a tool draws or something is selected: see
    // ctrl_z_reaches_the_terminal_with_nothing_to_undo.
}

#[gpui_kit::test]
fn clicking_a_stroke_selects_it_and_delete_removes_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        click(
            window,
            cx,
            (f32::from(o.x) + 70.0, f32::from(o.y) + 22.0),
            1,
        );
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.selected_ink, Some(0));
        assert_eq!(
            ws.read(cx).space.focused,
            None,
            "Delete and Backspace go to the drawing, so no terminal keeps the keyboard"
        );
        assert_eq!(ws.read(cx).space.ink_undo.len(), 1, "a click isn't a step");
        // Dragging it moves it.
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            (f32::from(o.x) + 70.0, f32::from(o.y) + 22.0),
            (f32::from(o.x) + 70.0, f32::from(o.y) + 122.0),
        );
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.ink[0].bounds().y.round(), 118.0);
        assert_eq!(ws.read(cx).space.selected_ink, Some(0), "still selected");
        assert_eq!(ws.read(cx).space.ink_undo.len(), 2, "the move is");
        window.press("escape", cx);
        assert_eq!(ws.read(cx).space.selected_ink, None, "Esc deselects");
        click(
            window,
            cx,
            (f32::from(o.x) + 70.0, f32::from(o.y) + 122.0),
            1,
        );
        assert_eq!(ws.read(cx).space.selected_ink, Some(0));
        window.press("delete", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.ink.is_empty());
        assert_eq!(ws.space.selected_ink, None);
    });
    // A click on empty canvas with nothing there selects nothing and pans as before.
    cx.update_window(handle, |_, window, cx| {
        let o = ws.read(cx).origin;
        click(
            window,
            cx,
            (f32::from(o.x) + 70.0, f32::from(o.y) + 22.0),
            1,
        );
        assert_eq!(ws.read(cx).space.selected_ink, None);
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_z_undoes_and_redoes_drawing_changes(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    draw(handle, &ws, cx, (20.0, 60.0), (120.0, 60.0));
    cx.update_window(handle, |_, window, cx| {
        window.press(UNDO, cx);
        assert_eq!(ws.read(cx).space.ink.len(), 1);
        window.press(UNDO, cx);
        assert!(ws.read(cx).space.ink.is_empty());
        window.press(REDO, cx);
        assert_eq!(ws.read(cx).space.ink.len(), 1);
        // The eraser over nothing adds no step: undo goes back to the empty canvas.
        window.click("tool-eraser", cx);
        window.render_frame(cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (300.0, 5.0), (300.0, 40.0));
    cx.update_window(handle, |_, window, cx| {
        window.press(UNDO, cx);
        assert!(
            ws.read(cx).space.ink.is_empty(),
            "the eraser drag wasn't a step"
        );
    })
    .unwrap();
}

// Linux: `cat` is a session leader, so its group is orphaned and the terminal's Ctrl+Z
// doesn't stop it; the PTY echoes the key as ^Z.
#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn ctrl_z_reaches_the_terminal_with_nothing_to_undo(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // Nothing drawn, Select on: the key goes to the terminal.
    let shown = echo_after(UNDO, handle, &ws, cx);
    assert!(shown.contains("^Z"), "{shown:?}");
}

#[gpui_kit::test]
fn a_stroke_over_a_terminal_doesnt_steal_its_clicks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let body = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("tool-pen", cx);
            window.render_frame(cx);
            window.find(("body", id.0 as usize)).bounds()
        })
        .unwrap();
    let (x, y) = (
        f32::from(body.origin.x) + 30.0,
        f32::from(body.origin.y) + 30.0,
    );
    cx.update_window(handle, |_, window, cx| {
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            (x, y),
            (x + 100.0, y),
        );
        window.render_frame(cx);
        window.press("escape", cx);
        window.render_frame(cx);
        ws.update(cx, |ws, _| ws.space.focused = None);
        // With Select on, a click on the stroke over the terminal goes to the terminal.
        click(window, cx, (x + 50.0, y), 1);
        window.render_frame(cx);
        let ws_now = ws.read(cx);
        assert_eq!(ws_now.space.ink.len(), 1);
        assert_eq!(ws_now.space.selected_ink, None);
        assert_eq!(ws_now.space.focused, Some(id));
    })
    .unwrap();
}

#[gpui_kit::test]
fn undo_keeps_the_last_50_drawing_changes(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.update(cx, |ws, _| {
        for _ in 0..crate::workspace::INK_UNDO + 10 {
            ws.remember_ink();
        }
        assert_eq!(ws.space.ink_undo.len(), 50);
    });
}

#[gpui_kit::test]
fn right_clicking_a_drawing_opens_its_menu(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        right_click(window, cx, (f32::from(o.x) + 70.0, f32::from(o.y) + 22.0));
        window.render_frame(cx);
        assert!(ws.read(cx).menu.is_some());
        ws.update(cx, |ws, cx| ws.delete_ink(0, cx));
        assert!(ws.read(cx).space.ink.is_empty());
    })
    .unwrap();
}

/// Drags on the empty canvas near its top-left corner, clear of the startup terminal.
fn draw(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    from: (f32, f32),
    to: (f32, f32),
) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        let at = |p: (f32, f32)| (f32::from(o.x) + p.0, f32::from(o.y) + p.1);
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            at(from),
            at(to),
        );
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_pen_draws_a_stroke_that_comes_back_after_a_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
        window.click("ink-color-red", cx);
        window.click("ink-width-thick", cx);
        window.render_frame(cx);
        window.find("ink-layer");
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (80.0, 50.0));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tool, crate::workspace::Tool::Pen, "the pen stays on");
        let [
            templum_canvas::Drawing::Stroke {
                points,
                width,
                color,
                highlight,
            },
        ] = ws.space.ink.as_slice()
        else {
            panic!("one stroke: {:?}", ws.space.ink);
        };
        assert_eq!(
            points.last(),
            Some(&ws.space.canvas.camera.to_world(Point::new(80.0, 50.0))),
            "the pen catches up with the pointer on release"
        );
        assert_eq!(
            (*width, *color, *highlight),
            (8.0, templum_canvas::Color::Red, false)
        );
        assert!(
            ws.space.focused.is_some(),
            "drawing doesn't unfocus the terminal"
        );
    });
    // A press without a move is nothing.
    draw(handle, &ws, cx, (200.0, 20.0), (200.0, 20.0));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.ink.len(), 1));
    cx.update_window(handle, |_, _, cx| {
        ws.update(cx, |ws, cx| ws.save_canvas_now(cx))
    })
    .unwrap();
    let (_, again) = open_in(cx, None, Some(dir.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        assert_eq!(ws.space.ink.len(), 1);
        assert_eq!(ws.space.ink[0].width(), 8.0);
    });
}

#[gpui_kit::test]
fn the_highlighter_is_wide_and_translucent(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-highlighter", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("ink-width-thin").is_none(),
            "one width only"
        );
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    ws.read_with(cx, |ws, _| {
        let [
            templum_canvas::Drawing::Stroke {
                width, highlight, ..
            },
        ] = ws.space.ink.as_slice()
        else {
            panic!("one stroke");
        };
        assert_eq!((*width, *highlight), (crate::ink::HIGHLIGHTER_WIDTH, true));
    });
}

#[gpui_kit::test]
fn the_shapes_pill_picks_an_arrow_and_shift_squares_a_rectangle(cx: &mut TestAppContext) {
    use templum_canvas::{Drawing, ShapeKind};
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-shapes", cx);
        window.render_frame(cx);
        window.find("ink-shapes");
        window.click("shape-arrow", cx);
        window.render_frame(cx);
        assert!(window.try_find("ink-shapes").is_none(), "closed on a pick");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tool, crate::workspace::Tool::Shape(ShapeKind::Arrow))
    });
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 60.0));
    ws.read_with(cx, |ws, _| {
        let [Drawing::Shape { kind, from, to, .. }] = ws.space.ink.as_slice() else {
            panic!("one shape: {:?}", ws.space.ink);
        };
        assert_eq!(*kind, ShapeKind::Arrow);
        assert_eq!((to.x - from.x, to.y - from.y), (100.0, 40.0));
    });
    // A rectangle with Shift held is a square.
    cx.update_window(handle, |_, window, cx| {
        window.click("tool-shapes", cx);
        window.render_frame(cx);
        window.click("shape-rect", cx);
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        let at = |p: (f32, f32)| (f32::from(o.x) + p.0, f32::from(o.y) + p.1);
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers {
                shift: true,
                ..Default::default()
            },
            at((200.0, 20.0)),
            at((300.0, 60.0)),
        );
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let Some(Drawing::Shape { kind, from, to, .. }) = ws.space.ink.last() else {
            panic!("a shape");
        };
        assert_eq!(*kind, ShapeKind::Rect);
        assert_eq!((to.x - from.x, to.y - from.y), (100.0, 100.0), "squared");
        assert_eq!(ws.last_shape, Some(ShapeKind::Rect));
    });
    // With Select on, Esc closes the pill.
    cx.update_window(handle, |_, window, cx| {
        window.click("tool-select", cx);
        window.click("tool-shapes", cx);
        window.render_frame(cx);
        window.find("ink-shapes");
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("ink-shapes").is_none(), "Esc closes it");
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_eraser_removes_the_stroke_it_passes_over(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    draw(handle, &ws, cx, (20.0, 200.0), (120.0, 200.0));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.ink.len(), 2));
    cx.update_window(handle, |_, window, cx| window.click("tool-eraser", cx))
        .unwrap();
    // Across the first stroke, well away from the second.
    draw(handle, &ws, cx, (70.0, 5.0), (70.0, 40.0));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.ink.len(), 1);
        assert_eq!(ws.space.ink[0].bounds().y.round(), 198.0);
    });
    // An eraser drag over nothing changes nothing (and, in Task 5, adds no undo step).
    draw(handle, &ws, cx, (300.0, 5.0), (300.0, 40.0));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.ink.len(), 1));
}
