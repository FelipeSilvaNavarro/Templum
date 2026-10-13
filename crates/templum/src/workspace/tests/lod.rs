//! Level of detail at far zoom: what each block draws at 33% and 25%, and what keeps working
//! there. See docs/specs/level-of-detail.md.

use std::time::Duration;

use gpui_kit::{ElementId, Focusable as _};

use super::*;

/// The camera at rest at `zoom`, its top-left corner at `at` (world units), and a frame drawn.
fn rest_at(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    at: Point,
    zoom: f32,
) {
    ws.update(cx, |ws, _| {
        ws.space.pristine = false;
        ws.space.canvas.camera = Camera::at(at, zoom);
        ws.space.flight = None;
    });
    frame(handle, cx);
}

/// No block focused, the workspace with the keyboard: as after a click on the empty canvas.
fn unfocus(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        let focus = ws.update(cx, |ws, _| {
            ws.space.focused = None;
            ws.focus_handle.clone()
        });
        window.focus(&focus, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// The element was drawn in the last frame.
fn shown(handle: AnyWindowHandle, cx: &mut TestAppContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(handle, |_, window, _| window.try_find(id).is_some())
        .unwrap()
}

/// A Git block's Changes tab: there while its body is built.
fn git_tabs(id: BlockId) -> ElementId {
    ElementId::Name(format!("git-tab-{}-changes", id.0).into())
}

/// The level follows the zoom the camera last came to rest at: a glide keeps it, through a pan
/// mid-glide, and it changes on the frame the glide ends.
#[gpui_kit::test]
fn the_rest_zoom_holds_through_a_glide_and_changes_when_it_ends(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.5);
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.rest, 0.5));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let pivot = ws.center(window);
            ws.zoom_to(-1, pivot, cx);
        })
    })
    .unwrap();
    ws.update(cx, restart_glide);
    std::thread::sleep(Duration::from_millis(40));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, _| ws.pan_by(Point::new(30.0, 0.0)));
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let z = ws.space.view.zoom();
        assert!(ws.space.flight.is_some() && z < 0.5, "mid-glide at {z}");
        assert_eq!(ws.space.rest, 0.5, "the zoom it rested at");
    });
    std::thread::sleep(crate::motion::FLIGHT + Duration::from_millis(50));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.flight.is_none(), "landed");
        assert_eq!(ws.space.rest, 0.33);
    });
}

/// A workspace shown by a glide before its first frame (Go to the terminal waiting for you, in
/// another workspace) is drawn at the level of the zoom its camera rests at, not of no zoom.
#[gpui_kit::test]
fn a_canvas_first_drawn_mid_glide_keeps_the_zoom_it_rests_at(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            assert_eq!(ws.space.rest, 0.0, "never drawn");
            let pivot = ws.center(window);
            ws.zoom_to(-1, pivot, cx);
        })
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.flight.is_some());
        assert_eq!(
            ws.space.rest, 1.0,
            "the camera rested at 100% before the glide"
        );
    });
}

/// Below Full a Git block is its card: no tabs, lists, diff or commit form. The block worked in is
/// Full at any zoom: the focused one, and one whose field holds the keyboard.
#[gpui_kit::test]
fn a_far_git_block_is_its_card_unless_it_is_worked_in(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    unfocus(handle, &ws, cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.5);
    assert!(shown(handle, cx, git_tabs(id)), "12 px at 50%: 6 px, Full");
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.33);
    assert!(!shown(handle, cx, git_tabs(id)), "3.96 px at 33%: its card");
    assert!(
        shown(handle, cx, ("block", id.0 as usize)),
        "the card is drawn"
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, git_tabs(id)), "focused: Full at 33%");
    // Its summary holds the keyboard, and no block is focused: still Full.
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        let field = summary.read(cx).focus_handle(cx);
        window.focus(&field, cx);
        ws.update(cx, |ws, _| ws.space.focused = None);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(
        shown(handle, cx, git_tabs(id)),
        "a field with the keyboard keeps it Full"
    );
    unfocus(handle, &ws, cx);
    assert!(!shown(handle, cx, git_tabs(id)), "left: its card again");
}

/// The level changes only when the camera rests: a glide's frames keep the level it started at,
/// and the frame it ends draws the new one.
#[gpui_kit::test]
fn a_git_blocks_level_changes_when_a_glide_ends(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    unfocus(handle, &ws, cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.5);
    // 50% to 33%: Full until the glide ends; then 33% to 50%: the card until it ends.
    for (step, during, landed) in [(-1, true, false), (1, false, true)] {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let pivot = ws.center(window);
                ws.zoom_to(step, pivot, cx);
            })
        })
        .unwrap();
        ws.update(cx, restart_glide);
        std::thread::sleep(Duration::from_millis(40));
        frame(handle, cx);
        assert!(
            ws.read_with(cx, |ws, _| ws.space.flight.is_some()),
            "mid-glide"
        );
        assert_eq!(
            shown(handle, cx, git_tabs(id)),
            during,
            "mid-glide, step {step}"
        );
        std::thread::sleep(crate::motion::FLIGHT + Duration::from_millis(50));
        frame(handle, cx);
        assert_eq!(
            shown(handle, cx, git_tabs(id)),
            landed,
            "landed, step {step}"
        );
    }
}

/// Below Full a title is a label: 12 px text and an 8 px status dot at any zoom, 4 px in from the
/// card's border. At 100% the header is as it was.
#[gpui_kit::test]
fn a_far_title_is_a_label_of_a_fixed_size(cx: &mut TestAppContext) {
    use crate::alert::Alert;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    unfocus(handle, &ws, cx);
    // News nobody looked at: the dot shows.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().alert = Some(Alert::Attention(None))
    });
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    let corner = Point::new(rect.x - 10.0, rect.y - 10.0);
    let bounds = |cx: &mut TestAppContext, el: (&'static str, usize)| {
        cx.update_window(handle, |_, window, _| window.find(el).bounds())
            .unwrap()
    };
    rest_at(handle, &ws, cx, corner, 1.0);
    let block = bounds(cx, ("block", n));
    assert_eq!(
        bounds(cx, ("status", n)).size,
        size(px(7.0), px(7.0)),
        "the header's dot"
    );
    assert_eq!(
        bounds(cx, ("title", n)).size.width,
        block.size.width - px(2.0),
        "the header's title spans the card"
    );
    let text_at_100 = bounds(cx, ("title-text", n)).size.height;
    for zoom in [0.33, 0.25] {
        rest_at(handle, &ws, cx, corner, zoom);
        let (block, title) = (bounds(cx, ("block", n)), bounds(cx, ("title", n)));
        let (text, dot) = (bounds(cx, ("title-text", n)), bounds(cx, ("status", n)));
        assert_eq!(dot.size, size(px(8.0), px(8.0)), "an 8 px dot at {zoom}");
        assert_eq!(
            text.size.height, text_at_100,
            "12 px text at {zoom}, as at 100%"
        );
        let inset =
            |a: gpui_kit::Pixels, b: gpui_kit::Pixels| (f32::from(a - b) - 5.0).abs() < 0.01;
        assert!(
            inset(title.origin.x, block.origin.x) && inset(title.origin.y, block.origin.y),
            "1 px border and 4 px in at {zoom}: {title:?} in {block:?}"
        );
        assert!(
            title.size.width <= block.size.width - px(10.0),
            "inside the card at {zoom}"
        );
        assert!(
            dot.origin.x > title.origin.x && dot.right() < title.right(),
            "the dot is in the label at {zoom}"
        );
    }
}

/// A press on a far label moves the block and focuses it, and starts no selection in the terminal
/// under it; a right-click opens the block's menu; a double-click frames the block.
#[gpui_kit::test]
fn a_far_label_moves_opens_the_menu_and_frames(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(
        handle,
        &ws,
        cx,
        Point::new(rect.x - 10.0, rect.y - 10.0),
        0.25,
    );
    let c = cx
        .update_window(handle, |_, window, _| {
            window.find(("title", n)).bounds().center()
        })
        .unwrap();
    let at = (f32::from(c.x), f32::from(c.y));
    cx.update_window(handle, |_, window, cx| {
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            at,
            (at.0 + 40.0, at.1),
        );
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let moved = ws.space.canvas.get(id).unwrap().rect;
        assert!(
            moved.x > rect.x + 100.0,
            "40 px at 25%: {rect:?} -> {moved:?}"
        );
        assert_eq!(ws.space.focused, Some(id), "and focused");
        let term = ws.space.terminal(id).unwrap().term.shared();
        assert!(
            term.selection_text().is_none(),
            "no selection under the label"
        );
    });
    cx.update_window(handle, |_, window, cx| window.right_click(("title", n), cx))
        .unwrap();
    ws.update(cx, |ws, _| {
        assert!(ws.menu.is_some(), "its menu");
        ws.menu = None;
    });
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.double_click(("title", n), cx)
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let z = ws.space.canvas.camera.zoom();
        assert!(z > 0.25, "framed: the camera flies to {z}");
    });
}

/// The header's small buttons (a note's preview eye here) hide behind a far label.
#[gpui_kit::test]
fn the_headers_buttons_hide_behind_a_far_label(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let note = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.add_note_at(Point::new(5000.0, 0.0), window, cx);
                ws.space.appearing.clear();
                ws.space.canvas.top().unwrap().0 as usize
            })
        })
        .unwrap();
    rest_at(handle, &ws, cx, Point::new(4990.0, -10.0), 1.0);
    assert!(shown(handle, cx, ("preview", note)));
    rest_at(handle, &ws, cx, Point::new(4990.0, -10.0), 0.25);
    assert!(
        !shown(handle, cx, ("preview", note)),
        "the label sits there"
    );
}

/// A focused terminal at 25% is Full: its letters, its cursor (the IME popup follows it) and the
/// keys. When the focus moves to another block, it is bars on the next frame.
#[cfg(unix)]
#[gpui_kit::test]
fn a_focused_far_terminal_draws_its_letters_and_the_one_left_bars(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    ws.update(cx, |ws, _| ws.ime_cursor = None);
    rest_at(
        handle,
        &ws,
        cx,
        Point::new(rect.x - 10.0, rect.y - 10.0),
        0.25,
    );
    assert!(
        !shown(handle, cx, ("silhouette", n)),
        "focused: its letters"
    );
    ws.read_with(cx, |ws, _| {
        assert!(ws.ime_cursor.is_some(), "its cursor is drawn")
    });
    cx.update_window(handle, |_, window, cx| window.input("z", cx))
        .unwrap();
    super::terminals::until("the key to show", handle, &ws, cx, |ws| {
        ws.space
            .terminal(id)
            .unwrap()
            .snapshot
            .borrow()
            .text()
            .contains('z')
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.add_note_at(Point::new(rect.x + rect.w + 100.0, rect.y), window, cx)
        })
    })
    .unwrap();
    frame(handle, cx);
    assert!(shown(handle, cx, ("silhouette", n)), "left: bars");
}

/// A far terminal's bars are its colored runs, painted on its grid's cells (the grid's area is
/// where the letters start); a theme switch while far gives them the new palette at once.
#[cfg(unix)]
#[gpui_kit::test]
fn a_far_terminals_bars_are_its_colored_runs_on_its_cells(cx: &mut TestAppContext) {
    use templum_terminal::{Bar, Span};
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    // `cat` prints the line back: "red" in red, then " ok". Waited for in the block's own
    // snapshot: a snapshot of its own (`shown_in`) would take the rows' damage from it.
    ws.read_with(cx, |ws, _| {
        let term = &ws.space.terminal(id).unwrap().term;
        term.write(b"\x1b[31mred\x1b[0m ok\r".to_vec())
    });
    super::terminals::until("red ok", handle, &ws, cx, |ws| {
        let pane = ws.space.terminal(id).unwrap();
        pane.snapshot.borrow().text().contains("\nred ok")
    });
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(
        handle,
        &ws,
        cx,
        Point::new(rect.x - 10.0, rect.y - 10.0),
        0.25,
    );
    let bars = |ws: &Workspace| {
        let snap = ws.space.terminal(id).unwrap().snapshot.borrow();
        let row = snap
            .rows
            .iter()
            .rposition(|r| r.text == "red ok")
            .expect("the row");
        snap.rows[row].bars()
    };
    ws.read_with(cx, |ws, _| {
        let p = &ws.theme.palette;
        assert_eq!(
            bars(ws),
            vec![
                Bar {
                    cols: Span { start: 0, len: 3 },
                    fg: p.ansi[1],
                    dim: false
                },
                Bar {
                    cols: Span { start: 4, len: 2 },
                    fg: p.foreground,
                    dim: false
                },
            ]
        );
    });
    cx.update_window(handle, |_, window, _| {
        let grid = window.find(("silhouette", n)).bounds();
        let body = window.find(("body", n)).bounds();
        let pad = px(chrome::PAD * 0.25);
        assert_eq!(grid.origin, point(body.origin.x + pad, body.origin.y + pad));
    })
    .unwrap();
    let red = ws.read_with(cx, |ws, _| ws.theme.palette.ansi[1]);
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        let now = ws.theme.palette.ansi[1];
        assert_ne!(now, red, "another theme");
        assert_eq!(bars(ws)[0].fg, now, "the bars take its red at once");
    });
}

/// Under 2.5 px (8-point text at 25%) a terminal is its card: its grid isn't painted, so it isn't
/// re-armed, and its output wakes the UI once and then waits, as off screen.
#[cfg(unix)]
#[gpui_kit::test]
fn a_terminal_drawn_as_its_card_isnt_rearmed(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    // 8 × 0.25 = 2 px: the Card level. The grid keeps its cells; only the level changes.
    ws.update(cx, |ws, _| ws.cell.font_size = 8.0);
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(
        handle,
        &ws,
        cx,
        Point::new(rect.x - 10.0, rect.y - 10.0),
        0.25,
    );
    assert!(shown(handle, cx, ("block", n)), "on screen");
    assert!(!shown(handle, cx, ("silhouette", n)), "no bars: its card");
    let armed = |ws: &Workspace| ws.space.terminal(id).unwrap().term.shared().armed();
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(id).unwrap().term.write(b"x\r".to_vec())
    });
    super::terminals::until("its output to wake the UI once", handle, &ws, cx, |ws| {
        !armed(ws)
    });
    for _ in 0..3 {
        frame(handle, cx);
    }
    ws.read_with(cx, |ws, _| assert!(!armed(ws), "a card isn't re-armed"));
}

/// Over a far terminal the wheel moves the canvas and Ctrl+wheel zooms; once it is clicked
/// (focused: Full), the wheel is the terminal's again.
#[gpui_kit::test]
fn the_wheel_over_a_far_terminal_moves_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    let corner = Point::new(rect.x - 10.0, rect.y - 10.0);
    rest_at(handle, &ws, cx, corner, 0.25);
    let c = cx
        .update_window(handle, |_, window, _| {
            window.find(("silhouette", n)).bounds().center()
        })
        .unwrap();
    let at = (f32::from(c.x), f32::from(c.y));
    let camera = |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    let down = ScrollDelta::Pixels(point(px(0.0), px(40.0)));
    let before = camera(cx);
    cx.update_window(handle, |_, window, cx| wheel(window, cx, at, down, false))
        .unwrap();
    assert_ne!(camera(cx).origin, before.origin, "the canvas moved");
    rest_at(handle, &ws, cx, corner, 0.25);
    cx.update_window(handle, |_, window, cx| {
        wheel(window, cx, at, ScrollDelta::Lines(point(0.0, 1.0)), true)
    })
    .unwrap();
    assert_eq!(camera(cx).zoom(), 0.33, "Ctrl+wheel zooms");
    rest_at(handle, &ws, cx, corner, 0.25);
    cx.update_window(handle, |_, window, cx| {
        click(window, cx, at, 1);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(!shown(handle, cx, ("silhouette", n)), "clicked: Full");
    let before = camera(cx);
    cx.update_window(handle, |_, window, cx| wheel(window, cx, at, down, false))
        .unwrap();
    assert_eq!(camera(cx), before, "the wheel is the terminal's");
}

/// Ctrl+click on a URL in an unfocused far terminal opens it: its bars keep the press, which
/// reads the grid. The URL is printed 10 rows down (the echo of the line is a single row),
/// below the rows the label covers.
#[cfg(unix)]
#[gpui_kit::test]
fn ctrl_click_on_a_url_in_a_far_terminal_opens_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    ws.read_with(cx, |ws, _| {
        let term = &ws.space.terminal(id).unwrap().term;
        term.write(b"\x1b[10Bsee https://example.com/x\r".to_vec())
    });
    super::terminals::until("the URL", handle, &ws, cx, |ws| {
        let pane = ws.space.terminal(id).unwrap();
        pane.snapshot
            .borrow()
            .text()
            .contains("\nsee https://example.com/x")
    });
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(
        handle,
        &ws,
        cx,
        Point::new(rect.x - 10.0, rect.y - 10.0),
        0.25,
    );
    assert!(shown(handle, cx, ("silhouette", id.0 as usize)));
    let row = ws.read_with(cx, |ws, _| {
        let snap = ws.space.terminal(id).unwrap().snapshot.borrow();
        snap.rows
            .iter()
            .rposition(|r| r.text.starts_with("see https"))
            .unwrap()
    });
    super::terminals::ctrl_click_cell(handle, &ws, cx, id, (row, 8));
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/x"));
}

/// A find field left open on a terminal that lost the focus at 33% is hidden, and comes back with
/// its text when the terminal is focused again.
#[gpui_kit::test]
fn a_find_field_hides_far_away_and_comes_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(
        handle,
        &ws,
        cx,
        Point::new(rect.x - 10.0, rect.y - 10.0),
        0.33,
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_find(window, cx));
        window.render_frame(cx);
        window.input("needle", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, ("find", n)), "focused: its field");
    unfocus(handle, &ws, cx);
    assert!(!shown(handle, cx, ("find", n)), "far and left: hidden");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, ("find", n)), "back");
    ws.read_with(cx, |ws, cx| {
        let bar = ws.space.terminal(id).unwrap().find.as_ref().unwrap();
        assert_eq!(bar.input.read(cx).value().as_ref(), "needle");
    });
}

/// "Go to the terminal waiting for you" focuses a far waiting terminal: it is drawn Full.
#[gpui_kit::test]
fn going_to_a_far_waiting_terminal_draws_it_in_full(cx: &mut TestAppContext) {
    use crate::alert::Alert;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    unfocus(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().alert = Some(Alert::Attention(None))
    });
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(
        handle,
        &ws,
        cx,
        Point::new(rect.x - 10.0, rect.y - 10.0),
        0.25,
    );
    assert!(shown(handle, cx, ("silhouette", n)), "waiting, far: bars");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.go_to_waiting(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert!(!shown(handle, cx, ("silhouette", n)), "gone to: Full");
}

/// Below Full, rendered Markdown is an empty area: a note shown rendered is its tinted card, an
/// editor's Markdown file keeps its tab row. A kanban block is drawn in full there.
#[gpui_kit::test]
fn far_rendered_markdown_is_left_out_and_kanban_drawn_in_full(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "# Hello\n\nSome *words*.\n").unwrap();
    let (handle, ws) = open(cx);
    let editor = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    let (note, kanban) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.toggle_file_preview(editor, window, cx);
                ws.add_note_at(Point::new(40.0, 900.0), window, cx);
                let note = ws.space.canvas.top().unwrap();
                ws.toggle_preview(note, window, cx);
                ws.add_kanban_at(Point::new(1200.0, 40.0), window, cx);
                let kanban = ws.space.canvas.top().unwrap();
                ws.space.appearing.clear();
                (note.0 as usize, kanban.0 as usize)
            })
        })
        .unwrap();
    let e = editor.0 as usize;
    unfocus(handle, &ws, cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.5);
    assert!(
        shown(handle, cx, ("note-preview", note)),
        "the note rendered at 50%"
    );
    assert!(
        shown(handle, cx, ("file-preview", e)),
        "the file rendered at 50%"
    );
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.33);
    assert!(
        !shown(handle, cx, ("note-preview", note)),
        "the note is its card"
    );
    assert!(
        !shown(handle, cx, ("file-preview", e)),
        "the file's area is empty"
    );
    assert!(
        shown(handle, cx, ElementId::Name(format!("tab-{e}-0").into())),
        "the tab row stays"
    );
    assert!(
        shown(handle, cx, ("board-list", kanban)),
        "the kanban block is drawn in full"
    );
}
