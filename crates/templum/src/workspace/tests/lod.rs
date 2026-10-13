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
