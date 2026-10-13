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
