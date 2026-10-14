//! Moving around the canvas: pan, zoom, glides, fit, the hand, Alt and the middle button.

use super::*;

#[gpui_kit::test]
fn wheel_on_empty_canvas_pans_and_ctrl_wheel_zooms(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        wheel(
            window,
            cx,
            (f32::from(o.x) + 30.0, f32::from(o.y) + 30.0),
            ScrollDelta::Pixels(point(px(0.), px(100.))),
            false,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.origin.y, -100.0)
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        wheel(
            window,
            cx,
            (f32::from(o.x) + 30.0, f32::from(o.y) + 30.0),
            ScrollDelta::Lines(point(0.0, 1.0)),
            true,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.1));
}

#[cfg(target_os = "macos")]
const ZOOM_OUT: &str = "cmd--";
#[cfg(not(target_os = "macos"))]
const ZOOM_OUT: &str = "ctrl--";
#[cfg(target_os = "macos")]
const ZOOM_RESET: &str = "cmd-0";
#[cfg(not(target_os = "macos"))]
const ZOOM_RESET: &str = "ctrl-0";
#[cfg(target_os = "macos")]
const FIT: &str = "cmd-shift-1";
#[cfg(not(target_os = "macos"))]
const FIT: &str = "ctrl-shift-1";

#[gpui_kit::test]
fn zoom_keys_step_and_reset(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_IN, cx);
        window.press(ZOOM_IN, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.25));
    cx.update_window(handle, |_, window, cx| {
        window.press(ZOOM_OUT, cx);
        window.press(ZOOM_RESET, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.0));
}

#[gpui_kit::test]
fn fit_all_brings_every_block_into_view(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(
                templum_canvas::Rect::new(5000.0, 3000.0, 600.0, 400.0),
                window,
                cx,
            )
        });
        window.render_frame(cx);
        window.press(FIT, cx);
        let vp = ws.read(cx).viewport(window);
        assert_eq!(ws.read(cx).space.canvas.visible(vp).count(), 2);
    })
    .unwrap();
}

#[gpui_kit::test]
fn zoom_keys_glide_to_the_step(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_IN, cx);
        // The glide starts now, however long a slow machine took since the key: the frame
        // below must catch it on the way.
        ws.update(cx, restart_glide);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.canvas.camera.zoom(),
            1.1,
            "the target is set at once"
        );
        assert!(ws.space.flight.is_some(), "the view glides");
        assert!(
            ws.space.view.zoom() < 1.1,
            "drawn on the way: {}",
            ws.space.view.zoom()
        );
    });
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.flight.is_none(), "landed");
        assert_eq!(ws.space.view, ws.space.canvas.camera);
    });
}

#[gpui_kit::test]
fn a_wheel_pan_during_a_glide_moves_the_glide_too(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_IN, cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| {
        let (before, _) = ws.space.flight.unwrap();
        ws.pan_by(Point::new(30.0, 0.0));
        let (after, _) = ws.space.flight.expect("still gliding");
        assert_eq!(
            after.to, ws.space.canvas.camera,
            "the glide still lands on the camera"
        );
        let z = before.from.zoom();
        assert!((before.from.origin.x - after.from.origin.x - 30.0 / z).abs() < 1e-3);
    });
}

#[gpui_kit::test]
fn dragging_the_canvas_ends_a_glide_on_the_camera(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_OUT, cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| {
        ws.space.canvas.camera.pan_by(Point::new(5.0, 5.0))
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.flight.is_none());
        assert_eq!(ws.space.view, ws.space.canvas.camera);
    });
}

#[gpui_kit::test]
fn fit_all_glides_then_shows_every_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(
                templum_canvas::Rect::new(5000.0, 3000.0, 600.0, 400.0),
                window,
                cx,
            )
        });
        window.render_frame(cx);
        window.press(FIT, cx);
        // The glide starts now, however long a slow machine took since the key: the frame
        // below must catch it on the way.
        ws.update(cx, restart_glide);
        window.render_frame(cx);
        assert!(ws.read(cx).space.flight.is_some());
    })
    .unwrap();
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let vp = ws.read(cx).viewport(window);
        let ws = ws.read(cx);
        assert_eq!(ws.space.canvas.visible_from(&ws.space.view, vp).count(), 2);
    })
    .unwrap();
}

#[gpui_kit::test]
fn pinch_over_a_block_zooms(cx: &mut TestAppContext) {
    use gpui_kit::PinchEvent;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let at = window.find(("body", id.0 as usize)).bounds().center();
        window.dispatch_event(
            PlatformInput::Pinch(PinchEvent {
                position: at,
                delta: 0.3,
                modifiers: Modifiers::default(),
                phase: TouchPhase::Moved,
            }),
            cx,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.1));
}

#[gpui_kit::test]
fn panning_the_canvas_keeps_the_focused_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let before = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            ws.read(cx).space.canvas.camera.origin
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        let o = ws.read(cx).origin;
        let from = point(o.x + px(40.), o.y + px(40.));
        window.drag(from, point(from.x + px(100.), from.y + px(50.)), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.focused, Some(id));
        let after = ws.space.canvas.camera.origin;
        assert_eq!(
            (before.x - after.x, before.y - after.y),
            (100.0, 50.0),
            "it panned"
        );
    });
}

#[gpui_kit::test]
fn the_hand_tool_pans_over_blocks_until_escape(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let before = ws.read_with(cx, |ws, _| {
        (
            ws.space.canvas.camera.origin,
            ws.space.canvas.get(id).unwrap().rect,
        )
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-hand", cx);
        window.render_frame(cx);
        window.find("hand");
        let from = window.find(("title", id.0 as usize)).bounds().center();
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            (f32::from(from.x), f32::from(from.y)),
            (f32::from(from.x) + 40.0, f32::from(from.y) + 10.0),
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tool, crate::workspace::Tool::Hand);
        assert_eq!(ws.space.canvas.get(id).unwrap().rect, before.1, "not moved");
        assert_ne!(ws.space.canvas.camera.origin, before.0, "panned");
    });
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("hand").is_none(), "the layer is gone");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tool, crate::workspace::Tool::Select)
    });
}

#[gpui_kit::test]
fn holding_alt_moves_the_canvas_over_blocks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    let before = ws.read_with(cx, |ws, _| ws.space.canvas.camera.origin);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("hand").is_none(), "not without Alt");
        hold_alt(window, cx, true);
        assert!(
            window.try_find("hand").is_some(),
            "the hand covers the canvas"
        );
        // A drag over the terminal: the canvas moves, nothing in the terminal is selected.
        let body = window.find(("body", id.0 as usize)).bounds().center();
        let from = (f32::from(body.x), f32::from(body.y));
        press_with(
            window,
            cx,
            MouseButton::Left,
            alt,
            from,
            (from.0 + 100.0, from.1 + 40.0),
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let moved = ws.space.canvas.camera.origin;
        assert_eq!((moved.x, moved.y), (before.x - 100.0, before.y - 40.0));
        let term = ws.space.terminal(id).unwrap().term.shared();
        assert!(term.selection_text().is_none(), "no selection");
        assert!(ws.drag.is_none(), "the drag ended");
    });
    // The wheel over the terminal zooms the canvas, like the hand tool, and leaves the
    // terminal's history alone.
    cx.update_window(handle, |_, window, cx| {
        let body = window.find(("body", id.0 as usize)).bounds().center();
        window.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: body,
                delta: ScrollDelta::Pixels(point(px(0.), px(-60.))),
                modifiers: alt,
                touch_phase: TouchPhase::Moved,
            }),
            cx,
        );
        hold_alt(window, cx, false);
        assert!(window.try_find("hand").is_none(), "gone when Alt is let go");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 0.9));
}

#[gpui_kit::test]
fn alt_let_go_elsewhere_is_noticed_at_the_next_move(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        hold_alt(window, cx, true);
    })
    .unwrap();
    let redraw = Rc::new(Cell::new(false));
    let seen = redraw.clone();
    cx.update(|cx| cx.observe(&ws, move |_, _| seen.set(true)).detach());
    // Let go while another window had the keyboard: only the next move says so, and the
    // workspace redraws without its hand.
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: point(px(300.), px(300.)),
                pressed_button: None,
                modifiers: Modifiers::default(),
            }),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    assert!(redraw.get(), "the move asks for a frame");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("hand").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_middle_drag_moves_the_canvas_anywhere(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let before = ws.read_with(cx, |ws, _| ws.space.canvas.camera.origin);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let body = window.find(("body", id.0 as usize)).bounds().center();
        let from = (f32::from(body.x), f32::from(body.y));
        press_with(
            window,
            cx,
            MouseButton::Middle,
            Modifiers::default(),
            from,
            (from.0 - 50.0, from.1),
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.origin.x, before.x + 50.0);
        assert!(ws.drag.is_none());
    });
}

/// Every element inside `block` (registered for tests), relative to the block: its
/// position and size. The invisible resize strips along its edges are left out: they keep a
/// screen size, to be easy to grab at any zoom.
fn block_layout(window: &Window, block: BlockId) -> std::collections::BTreeMap<String, [f32; 4]> {
    let id = gpui_kit::ElementId::NamedInteger("block".into(), block.0);
    let origin = window.find(("block", block.0 as usize)).bounds().origin;
    gpui_kit::base::test_support::snapshots(window)
        .into_iter()
        .filter_map(|s| {
            let at = s.path().iter().position(|p| *p == id)?;
            let key = format!("{:?}", &s.path()[at..]);
            if key.contains("\"resize-") {
                return None;
            }
            let b = s.bounds();
            Some((
                key,
                [
                    f32::from(b.origin.x - origin.x),
                    f32::from(b.origin.y - origin.y),
                    f32::from(b.size.width),
                    f32::from(b.size.height),
                ],
            ))
        })
        .collect()
}

// The canvas zoom magnifies the editor and the Git block like a photo: everything inside
// doubles at zoom 2, nothing keeps a fixed size or moves around (hairline borders stay
// 1 px, hence the few pixels of slack).
#[gpui_kit::test]
fn zooming_scales_the_editor_and_git_blocks_like_a_photo(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "fn main() {\n    println!(\"hi\");\n}\n",
    )
    .unwrap();
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let editor = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    let git = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let id = ws
                    .add_git_at(repo.path(), Point::new(40.0, 700.0), window, cx)
                    .unwrap();
                ws.space.appearing.clear();
                ws.select_git_row(
                    id,
                    crate::git_block::List::Files,
                    "a.txt".to_string(),
                    window,
                    cx,
                );
                id
            })
        })
        .unwrap();
    settle(handle, cx);
    // Each block measured with the camera on it, at zoom 1 and 2.
    let layout = |cx: &mut TestAppContext, block: BlockId, at: Point, zoom: f32| {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera = Camera::at(at, zoom);
            ws.space.view = ws.space.canvas.camera;
        });
        frame(handle, cx);
        cx.update_window(handle, |_, window, _| block_layout(window, block))
            .unwrap()
    };
    let compare = |name: &str,
                   one: &std::collections::BTreeMap<String, [f32; 4]>,
                   two: &std::collections::BTreeMap<String, [f32; 4]>| {
        assert!(one.len() > 5, "{name}: measured {}", one.len());
        for (key, a) in one {
            let b = two
                .get(key)
                .unwrap_or_else(|| panic!("{name}: gone at zoom 2: {key}"));
            for (x1, x2) in a.iter().zip(b) {
                // Slack: hairlines that stay 1 px add up, more so for what sits at the
                // right edge. A fixed-size widget is off by its own size (14 px, 24 px…).
                let slack = 6.0 + x2.abs() * 0.01;
                assert!(
                    (x1 * 2.0 - x2).abs() <= slack,
                    "{name}: {key}\n zoom 1 {a:?}\n zoom 2 {b:?}"
                );
            }
        }
    };
    let tabs = [
        crate::git_block::Tab::Changes,
        crate::git_block::Tab::History,
        crate::git_block::Tab::Branches,
    ];
    let one = layout(cx, editor, Point::new(0.0, 0.0), 1.0);
    let two = layout(cx, editor, Point::new(0.0, 0.0), 2.0);
    compare("editor", &one, &two);
    for tab in tabs {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.set_git_tab(git, tab, window, cx))
        })
        .unwrap();
        settle(handle, cx);
        let one = layout(cx, git, Point::new(0.0, 660.0), 1.0);
        let two = layout(cx, git, Point::new(0.0, 680.0), 2.0);
        compare(tab.name(), &one, &two);
    }
}

#[gpui_kit::test]
fn double_clicking_a_title_frames_the_block_and_again_goes_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let (before, vp) = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let ws = ws.read(cx);
            (ws.space.canvas.camera, ws.viewport(window))
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.double_click(("title", id.0 as usize), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let camera = ws.space.canvas.camera;
        assert!(camera.zoom() > before.zoom(), "bigger: {}", camera.zoom());
        let r = camera.rect_to_screen(ws.space.canvas.get(id).unwrap().rect);
        assert!(
            (r.x + r.w / 2.0 - vp.w / 2.0).abs() < 1.0,
            "centered: {r:?}"
        );
        assert!(
            r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= vp.w,
            "on screen: {r:?}"
        );
        assert!(
            r.y + r.h <= vp.h - crate::toolbar::SPACE,
            "clear of the toolbar: {r:?}"
        );
        assert!(ws.drag.is_none(), "a double-click doesn't move the block");
    });
    // The glide ends first: a title still moving isn't where the pointer goes.
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.double_click(("title", id.0 as usize), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera, before, "back where it was")
    });
}

#[gpui_kit::test]
fn with_the_hand_the_wheel_zooms_over_blocks_and_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-hand", cx);
        window.render_frame(cx);
        let body = window.find(("body", id.0 as usize)).bounds().center();
        wheel(
            window,
            cx,
            (f32::from(body.x), f32::from(body.y)),
            ScrollDelta::Lines(point(0.0, 1.0)),
            false,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.1));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        wheel(
            window,
            cx,
            (f32::from(o.x) + 30.0, f32::from(o.y) + 30.0),
            ScrollDelta::Pixels(point(px(0.), px(-60.))),
            false,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.0));
}

#[gpui_kit::test]
fn holding_alt_lights_the_hand_in_the_toolbar_until_let_go(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let lit = |window: &Window, id: &'static str| window.find(id).selected();
        assert_eq!(lit(window, "tool-select"), Some(true));
        assert_eq!(lit(window, "tool-hand"), Some(false));
        hold_alt(window, cx, true);
        assert_eq!(lit(window, "tool-hand"), Some(true));
        assert_eq!(lit(window, "tool-select"), Some(false));
        hold_alt(window, cx, false);
        assert_eq!(lit(window, "tool-select"), Some(true));
        assert_eq!(lit(window, "tool-hand"), Some(false));
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.tool,
            crate::workspace::Tool::Select,
            "Alt doesn't change the tool"
        )
    });
}

/// Mid-glide, a note's text glides with the zoom, on zooms where its rows are whole pixels: its
/// rows, its letters and its box grow together, like a photo of it, and the text under the
/// glide's pivot stays there.
#[gpui_kit::test]
fn a_notes_rows_and_letters_glide_together(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.add_note_at(Point::new(40.0, 40.0), window, cx);
            ws.space.appearing.clear();
        });
        window.render_frame(cx);
    })
    .unwrap();
    let (n, note, inner) = ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        let note = ws.space.blocks[&id]
            .note()
            .unwrap()
            .tab()
            .unwrap()
            .editor
            .clone();
        let rect = ws.space.canvas.get(id).unwrap().rect;
        (
            id.0 as usize,
            note,
            templum_canvas::chrome::content_rect(rect),
        )
    });
    let line_height = |cx: &mut TestAppContext| {
        note.read_with(cx, |e, _| f32::from(e.line_height().expect("laid out")))
    };
    // The text's box, the frame's padded area and the frame's zoom.
    let drawn = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            (
                window.find(("note-editor", n)).bounds(),
                window.find(("body", n)).bounds().origin,
                ws.read(cx).space.view.zoom(),
            )
        })
        .unwrap()
    };
    let at_100 = line_height(cx);
    // Two steps, the second asked mid-glide (as two wheel notches do): the target is 1.25,
    // the glide passes through zooms in between. Called directly: the note has the keyboard.
    // The frame is drawn 40 ms into the glide, however long the machine took.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        ws.update(cx, |ws, cx| {
            let pivot = ws.center(window);
            ws.zoom_to(1, pivot, cx);
            ws.zoom_to(1, pivot, cx);
            if let Some((_, since)) = &mut ws.space.flight {
                *since = std::time::Instant::now() - std::time::Duration::from_millis(40);
            }
        });
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.zoom(), 1.25);
        let z = ws.space.view.zoom();
        assert!(
            ws.space.flight.is_some() && z > 1.0 && z < 1.25,
            "mid-glide at {z}"
        );
    });
    let (mid, (text, frame, z)) = (line_height(cx), drawn(cx));
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.space.flight.is_none(), "landed"));
    let (landed, (_, frame_landed, z_landed)) = (line_height(cx), drawn(cx));
    assert!(
        at_100 < mid && mid < landed,
        "the rows glide: {at_100} at 100%, {mid} mid-glide, {landed} landed"
    );
    // The zoom the text is drawn at, from its box (under the tabs): its rows are 1.5 x its
    // letters, exactly.
    let tabs = crate::note::TABS_H;
    let s = f32::from(text.size.height) / (inner.h - tabs);
    assert!(
        (mid - 1.5 * crate::note::FONT * s).abs() < 0.05,
        "rows of {mid} px at zoom {s}"
    );
    // The note's point under the pivot doesn't move on screen while the frame glides; the body
    // drawn at `s` (the text below its tabs) puts that same point on the same pixel.
    let pad = templum_canvas::chrome::PAD;
    for (t, f, f_landed) in [
        (f32::from(text.origin.x), frame.x, frame_landed.x),
        (f32::from(text.origin.y) - tabs * s, frame.y, frame_landed.y),
    ] {
        let (f, f_landed) = (f32::from(f) + pad * z, f32::from(f_landed));
        let still = (f_landed + pad * z_landed - f) / (z - z_landed);
        let off = t - f + still * (s - z);
        assert!(off.abs() < 1.0, "the text under the pivot moved {off} px");
    }
}

/// Zooming keeps the rows a scrolled note shows, as a photo would: its scroll is in pixels, and
/// its rows change size with the zoom.
#[gpui_kit::test]
fn zooming_keeps_the_rows_a_scrolled_note_shows(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let text: String = (1..=200).map(|i| format!("line {i}\n")).collect();
    let (rect, note) = cx
        .update_window(handle, |_, window, cx| {
            let found = ws.update(cx, |ws, cx| {
                ws.add_note_at(Point::new(40.0, 40.0), window, cx);
                ws.space.appearing.clear();
                let id = ws.space.canvas.top().unwrap();
                let note = ws.space.blocks[&id]
                    .note()
                    .unwrap()
                    .tab()
                    .unwrap()
                    .editor
                    .clone();
                (ws.space.canvas.get(id).unwrap().rect, note)
            });
            found
                .1
                .update(cx, |e, cx| e.set_value(text.clone(), window, cx));
            window.render_frame(cx);
            found
        })
        .unwrap();
    // Scrolled to its 60th row.
    cx.update_window(handle, |_, window, cx| {
        note.update(cx, |e, cx| {
            let row = e.line_height().expect("laid out");
            e.set_scroll_offset(gpui_kit::point(gpui_kit::px(0.0), -row * 60.0), cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    let top = |cx: &mut TestAppContext| {
        note.read_with(cx, |e, _| e.visible_row_range().expect("laid out").start)
    };
    assert_eq!(top(cx), 60);
    for zoom in [1.25, 0.5, 2.0, 1.0] {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, _| {
                let at = Point::new(rect.x - 10.0, rect.y - 10.0);
                ws.space.canvas.camera = Camera::at(at, zoom);
                ws.space.flight = None;
            });
            window.render_frame(cx);
        })
        .unwrap();
        assert_eq!(top(cx), 60, "the top row at {zoom}");
    }
}

/// At rest, a text-heavy body's box is the frame's inner area: inside its 1 px border and its
/// padding, at any zoom; a note's text is below its tabs.
#[gpui_kit::test]
fn a_notes_box_fills_its_frame_at_rest(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let n = cx
        .update_window(handle, |_, window, cx| {
            let id = ws.update(cx, |ws, cx| {
                ws.add_note_at(Point::new(40.0, 40.0), window, cx);
                ws.space.appearing.clear();
                ws.space.canvas.top().unwrap()
            });
            window.render_frame(cx);
            id.0 as usize
        })
        .unwrap();
    for zoom in [1.0, 0.25] {
        let (frame, inner) = cx
            .update_window(handle, |_, window, cx| {
                ws.update(cx, |ws, _| {
                    let camera = ws.space.canvas.camera;
                    ws.space.canvas.camera = Camera::at(camera.origin, zoom);
                    ws.space.flight = None;
                });
                window.render_frame(cx);
                (
                    window.find(("body", n)).bounds(),
                    window.find(("note-editor", n)).bounds(),
                )
            })
            .unwrap();
        let pad = templum_canvas::chrome::PAD * zoom;
        let tabs = crate::note::TABS_H * zoom;
        let close = |a: gpui_kit::Pixels, b: f32| (f32::from(a) - b).abs() < 0.01;
        assert!(
            close(inner.origin.x, f32::from(frame.origin.x) + pad)
                && close(inner.origin.y, f32::from(frame.origin.y) + pad + tabs)
                && close(inner.size.width, f32::from(frame.size.width) - 2.0 * pad)
                && close(
                    inner.size.height,
                    f32::from(frame.size.height) - 2.0 * pad - tabs
                ),
            "zoom {zoom}: frame {frame:?}, box {inner:?}"
        );
    }
}

/// A note wraps at the same words at every zoom, like a photo of it: its lines change only when
/// the block is resized. So the same lines of its text are on screen at any zoom.
#[gpui_kit::test]
fn a_note_wraps_the_same_at_every_zoom(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let text: String = (1..=40)
        .map(|i| {
            format!("{i:02} the quick brown fox jumps over the lazy dog and keeps on running far\n")
        })
        .collect();
    let (rect, note) = cx
        .update_window(handle, |_, window, cx| {
            let found = ws.update(cx, |ws, cx| {
                ws.add_note_at(Point::new(40.0, 40.0), window, cx);
                ws.space.appearing.clear();
                let id = ws.space.canvas.top().unwrap();
                let note = ws.space.blocks[&id]
                    .note()
                    .unwrap()
                    .tab()
                    .unwrap()
                    .editor
                    .clone();
                (ws.space.canvas.get(id).unwrap().rect, note)
            });
            found
                .1
                .update(cx, |e, cx| e.set_value(text.clone(), window, cx));
            window.render_frame(cx);
            found
        })
        .unwrap();
    let mut shown = Vec::new();
    for zoom in templum_canvas::ZOOM_STEPS {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, _| {
                // The note's corner near the view's: on screen at any zoom.
                let at = Point::new(rect.x - 10.0, rect.y - 10.0);
                ws.space.canvas.camera = Camera::at(at, zoom);
                ws.space.flight = None;
            });
            window.render_frame(cx);
        })
        .unwrap();
        shown.push((zoom, note.read_with(cx, |e, _| e.visible_row_range())));
    }
    let at_100 = shown
        .iter()
        .find(|(zoom, _)| *zoom == 1.0)
        .map(|(_, range)| range.clone())
        .unwrap();
    assert!(
        at_100.as_ref().is_some_and(|r| r.end < 40),
        "the text is longer than the note: {at_100:?}"
    );
    for (zoom, range) in &shown {
        assert_eq!(range, &at_100, "lines shown at {zoom} vs 1.0: {shown:?}");
    }
}

/// Zooming keeps the rows a scrolled file tree shows, as a photo would: its scroll is in pixels,
/// and its rows change size with the zoom.
#[gpui_kit::test]
fn zooming_keeps_the_rows_a_scrolled_file_tree_shows(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..200 {
        std::fs::write(dir.path().join(format!("f{i:03}.txt")), "").unwrap();
    }
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    let n = id.0 as usize;
    let (rect, tree) = ws.read_with(cx, |ws, _| {
        (
            ws.space.canvas.get(id).unwrap().rect,
            ws.space.editor(id).unwrap().tree.clone(),
        )
    });
    // Scrolled so its 60th row is at the top.
    cx.update_window(handle, |_, window, cx| {
        let row = window.find(("entry", 0usize)).bounds().size.height;
        let scroll = tree.read(cx).scroll_handle().0.borrow().base_handle.clone();
        scroll.set_offset(gpui_kit::point(gpui_kit::px(0.0), -row * 60.0));
        window.render_frame(cx);
    })
    .unwrap();
    for zoom in [1.0, 1.25, 0.5, 0.67, 2.0, 1.0] {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, _| {
                let at = Point::new(rect.x - 10.0, rect.y - 10.0);
                ws.space.canvas.camera = Camera::at(at, zoom);
                ws.space.flight = None;
            });
            window.render_frame(cx);
            let top = window.find(("tree", n)).bounds().top();
            let row = window
                .try_find(("entry", 60usize))
                .unwrap_or_else(|| panic!("row 60 is shown at {zoom}"))
                .bounds();
            assert!(
                (f32::from(row.top()) - f32::from(top)).abs() < 0.5,
                "row 60 at the tree's top at {zoom}: {row:?}, tree top {top:?}"
            );
        })
        .unwrap();
    }
}

/// A Markdown file shown rendered zooms like a photo: its headings, code and spacing scale with
/// its text, so its content is as tall, measured at zoom 1, at every zoom.
#[gpui_kit::test]
fn a_markdown_preview_zooms_like_a_photo(cx: &mut TestAppContext) {
    let mut text =
        String::from("# A heading long enough to wrap over two lines, it goes on and on\n\n");
    for i in 1..=30 {
        text.push_str(&format!("Paragraph {i} with a few words.\n\n"));
    }
    text.push_str("## Code\n\n```\nlet x = 1;\n```\n");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), &text).unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    let rect = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.toggle_file_preview(id, window, cx);
                ws.space.canvas.get(id).unwrap().rect
            })
        })
        .unwrap();
    let mut heights = Vec::new();
    for zoom in [1.0, 0.5, 0.67, 1.5, 2.0] {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, _| {
                ws.space.canvas.camera = Camera::at(Point::new(rect.x - 10.0, rect.y - 10.0), zoom);
                ws.space.flight = None;
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        let height = ws.read_with(cx, |ws, _| {
            let editor = ws.space.editor(id).unwrap();
            let scroll = &editor.tabs[editor.active.unwrap()].preview_scroll.handle;
            (f32::from(scroll.max_offset().y) + f32::from(scroll.bounds().size.height)) / zoom
        });
        heights.push((zoom, height));
    }
    let base = heights[0].1;
    for (zoom, height) in &heights {
        // Each row's height is rounded to a whole pixel: up to a pixel per row.
        assert!(
            (height - base).abs() <= 40.0 / zoom,
            "{zoom}: {height} vs {base} at 1.0 ({heights:?})"
        );
    }
}
