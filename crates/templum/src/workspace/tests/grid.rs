//! Grid mode: blocks tiled over the workspace at zoom 1, and back to the canvas as they were.

use super::*;
use crate::workspace::history::{Change, Then};
use templum_canvas::GAP;

#[cfg(target_os = "macos")]
pub(super) const TOGGLE: &str = "cmd-shift-enter";
#[cfg(not(target_os = "macos"))]
pub(super) const TOGGLE: &str = "ctrl-shift-enter";

fn press(handle: AnyWindowHandle, key: &str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// What the tiles fill: the workspace's area inside the gap.
fn area(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Rect {
    cx.update_window(handle, |_, window, cx| {
        let vp = ws.read(cx).viewport(window);
        Rect::new(GAP, GAP, vp.w - 2.0 * GAP, vp.h - 2.0 * GAP)
    })
    .unwrap()
}

fn rect_of(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> Rect {
    ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect)
}

fn grid_size(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> (u16, u16) {
    ws.read_with(cx, |ws, _| ws.space.terminal(id).unwrap().term.grid_size())
}

/// Away from the startup centering, at 50%: grid mode's zoom 1 is another camera.
fn far_camera(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Camera {
    let camera = Camera::at(Point::new(-300.0, -200.0), 0.5);
    ws.update(cx, |ws, _| {
        ws.space.pristine = false;
        ws.space.canvas.camera = camera;
    });
    camera
}

#[gpui_kit::test]
fn ctrl_shift_enter_fills_the_workspace_and_gives_the_block_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let camera = far_camera(&ws, cx);
    frame(handle, cx);
    let (rect, cells) = (rect_of(&ws, cx, id), grid_size(&ws, cx, id));
    press(handle, TOGGLE, cx);
    assert_eq!(rect_of(&ws, cx, id), area(handle, &ws, cx));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera, Camera::default(), "zoom 1");
        assert_eq!(ws.space.camera(), camera, "the canvas's, kept");
        assert_eq!(ws.space.focused, Some(id));
    });
    let tiled = grid_size(&ws, cx, id);
    assert!(
        tiled.0 > cells.0 && tiled.1 > cells.1,
        "{tiled:?} after {cells:?}"
    );
    press(handle, TOGGLE, cx);
    assert_eq!(rect_of(&ws, cx, id), rect);
    assert_eq!(grid_size(&ws, cx, id), cells);
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.grid.is_none());
        assert_eq!(ws.space.canvas.camera, camera);
    });
}

#[gpui_kit::test]
fn a_window_resize_tiles_again(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    press(handle, TOGGLE, cx);
    let wide = grid_size(&ws, cx, id);
    cx.simulate_window_resize(handle, size(px(900.), px(600.)));
    frame(handle, cx);
    assert_eq!(rect_of(&ws, cx, id), area(handle, &ws, cx));
    assert!(grid_size(&ws, cx, id).0 < wide.0, "fewer columns");
}

#[gpui_kit::test]
fn the_selection_fills_the_workspace_left_to_right(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    ws.update(cx, |ws, _| ws.space.selected = vec![note, kanban]);
    press(handle, TOGGLE, cx);
    let area = area(handle, &ws, cx);
    let half = ((area.w - GAP) / 2.0).floor();
    assert_eq!(
        rect_of(&ws, cx, kanban),
        Rect::new(area.x, area.y, half, area.h),
        "the left one first"
    );
    assert_eq!(rect_of(&ws, cx, note).right(), area.right());
    cx.update_window(handle, |_, window, _| {
        assert!(
            window.try_find(("block", terminal.0 as usize)).is_none(),
            "hidden"
        );
        window.find(("block", kanban.0 as usize));
        assert!(window.try_find("toolbar").is_none());
    })
    .unwrap();
    assert_eq!(
        rect_of(&ws, cx, terminal),
        Rect::new(40.0, 400.0, 400.0, 300.0),
        "untouched on the canvas"
    );
}

#[gpui_kit::test]
fn the_camera_stays_put_in_grid_mode(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    press(handle, TOGGLE, cx);
    let tile = rect_of(&ws, cx, id);
    let at = (tile.x + 20.0, tile.y + 4.0);
    let at = {
        let o = ws.read_with(cx, |ws, _| ws.origin);
        (f32::from(o.x) + at.0, f32::from(o.y) + at.1)
    };
    cx.update_window(handle, |_, window, cx| {
        wheel(
            window,
            cx,
            at,
            ScrollDelta::Pixels(point(px(40.), px(40.))),
            false,
        );
        wheel(
            window,
            cx,
            at,
            ScrollDelta::Pixels(point(px(0.), px(200.))),
            true,
        );
        press_with(
            window,
            cx,
            MouseButton::Middle,
            Modifiers::default(),
            at,
            (at.0 + 90.0, at.1 + 60.0),
        );
        window.press(ZOOM_IN, cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera, Camera::default());
        assert!(ws.space.flight.is_none());
    });
    assert_eq!(rect_of(&ws, cx, id), tile);
}

#[gpui_kit::test]
fn a_snapshot_in_grid_mode_keeps_the_canvas(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(dir.path().to_path_buf()));
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let camera = far_camera(&ws, cx);
    let before: Vec<Rect> = ws.read_with(cx, |ws, _| {
        ws.space.canvas.blocks().iter().map(|b| b.rect).collect()
    });
    ws.update(cx, |ws, _| ws.space.selected = vec![kanban, note]);
    press(handle, TOGGLE, cx);
    ws.update(cx, |ws, cx| ws.save_canvas_now(cx));
    let crate::session::Loaded::Saved(saved) =
        crate::session::load(dir.path(), crate::session::DEFAULT_ID)
    else {
        panic!("not saved");
    };
    let rects: Vec<Rect> = saved
        .items
        .iter()
        .map(|item| match item {
            crate::session::Item::Terminal(t) => t.rect.into(),
            crate::session::Item::Kanban(k) => k.rect.into(),
            crate::session::Item::Note(n) => n.rect.into(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(rects, before, "the canvas rects, not the tiles");
    assert_eq!(
        (saved.camera.x, saved.camera.y),
        (camera.origin.x, camera.origin.y)
    );
    assert_eq!(saved.camera.zoom, camera.zoom());
}

#[gpui_kit::test]
fn closing_a_tile_gives_its_place_and_the_last_one_leaves(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let camera = far_camera(&ws, cx);
    ws.update(cx, |ws, _| ws.space.selected = vec![kanban, note]);
    press(handle, TOGGLE, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close(note, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        rect_of(&ws, cx, kanban),
        area(handle, &ws, cx),
        "the whole area"
    );
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, None));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close(kanban, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.grid.is_none(), "the canvas again");
        assert_eq!(ws.space.canvas.camera, camera);
        assert!(ws.space.shown(terminal));
    });
}

/// Ctrl+Z in grid mode moves a tile's place on the canvas, not its tile.
#[gpui_kit::test]
fn ctrl_z_in_grid_mode_moves_the_block_on_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, _) = three_blocks(handle, &ws, cx);
    let was = Rect::new(1.0, 2.0, 220.0, 200.0);
    ws.update(cx, |ws, _| {
        ws.keep(Change::Place(vec![(kanban, was)]), Then::New);
        ws.space.focused = Some(kanban);
    });
    press(handle, TOGGLE, cx);
    let tile = rect_of(&ws, cx, kanban);
    press(handle, UNDO, cx);
    assert_eq!(rect_of(&ws, cx, kanban), tile, "the tile stays");
    press(handle, TOGGLE, cx);
    assert_eq!(rect_of(&ws, cx, kanban), was);
}

#[test]
fn the_grid_key_is_ctrl_shift_enter() {
    let keymap = crate::keys::Keymap::new(&Default::default(), cfg!(target_os = "macos"))
        .unwrap()
        .0;
    let typed = gpui_kit::Keystroke::parse(TOGGLE).unwrap();
    assert_eq!(
        keymap.command(&typed),
        Some(crate::keys::Command::ToggleGrid)
    );
}

/// The blocks added since `before`, oldest first.
fn added_since(ws: &Entity<Workspace>, cx: &mut TestAppContext, before: usize) -> Vec<BlockId> {
    ws.read_with(cx, |ws, _| {
        let mut ids: Vec<BlockId> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
        ids.sort();
        ids[before..].to_vec()
    })
}

#[gpui_kit::test]
fn a_new_block_takes_half_of_the_focused_tile(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    press(handle, TOGGLE, cx);
    press(handle, NEW, cx);
    let [second] = added_since(&ws, cx, 1)[..] else {
        panic!("one new terminal");
    };
    let area = area(handle, &ws, cx);
    let (a, b) = (rect_of(&ws, cx, first), rect_of(&ws, cx, second));
    assert_eq!((a.x, a.y, a.h), (area.x, area.y, area.h), "side by side");
    assert_eq!((b.right(), b.h), (area.right(), area.h));
    // The new one has the keyboard, and is tall: the next one goes below it.
    press(handle, NEW, cx);
    let third = added_since(&ws, cx, 2)[0];
    let (b, c) = (rect_of(&ws, cx, second), rect_of(&ws, cx, third));
    assert_eq!((b.x, b.w), (c.x, c.w), "one above the other");
    assert!(c.y > b.bottom());
    assert_eq!(c.bottom(), area.bottom());
}

/// New blocks get places on the canvas where the view was, off each other's.
#[gpui_kit::test]
fn new_blocks_land_apart_on_the_canvas_in_the_view_left(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let camera = far_camera(&ws, cx);
    press(handle, TOGGLE, cx);
    press(handle, NEW, cx);
    press(handle, NEW, cx);
    let [second, third] = added_since(&ws, cx, 1)[..] else {
        panic!("two new terminals");
    };
    press(handle, TOGGLE, cx);
    let (b, c) = (rect_of(&ws, cx, second), rect_of(&ws, cx, third));
    assert_eq!(
        (c.x - b.x, c.y - b.y),
        (templum_canvas::CASCADE, templum_canvas::CASCADE)
    );
    let center = cx
        .update_window(handle, |_, window, cx| {
            let ws = ws.read(cx);
            camera.to_world(ws.center(window))
        })
        .unwrap();
    assert!(
        (b.x + b.w / 2.0 - center.x).abs() < 1.0 && (b.y + b.h / 2.0 - center.y).abs() < 1.0,
        "{b:?} around {center:?}"
    );
}

#[gpui_kit::test]
fn a_terminal_beside_a_tile_lands_beside_its_place_on_the_canvas(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx);
    let editor = open_editor(handle, &ws, cx, dir.path());
    let home = rect_of(&ws, cx, editor);
    ws.update(cx, |ws, _| ws.space.focused = Some(editor));
    press(handle, TOGGLE, cx);
    let terminal = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.terminal_beside(editor, dir.path(), None, window, cx)
            })
        })
        .unwrap()
        .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.space.shown(terminal), "tiled"));
    press(handle, TOGGLE, cx);
    let r = rect_of(&ws, cx, terminal);
    assert_eq!((r.x, r.y), (home.right() + templum_canvas::BESIDE, home.y));
}

#[gpui_kit::test]
fn going_to_a_block_on_the_canvas_brings_it_into_the_grid(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, terminal) = three_blocks(handle, &ws, cx);
    ws.update(cx, |ws, _| ws.space.focused = Some(kanban));
    press(handle, TOGGLE, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let space = ws.space.id.clone();
            ws.reveal(&space, terminal, window, cx);
        })
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.shown(terminal));
        assert_eq!(ws.space.focused, Some(terminal));
    });
    let area = area(handle, &ws, cx);
    assert_eq!(
        rect_of(&ws, cx, terminal).right(),
        area.right(),
        "beside it"
    );
    press(handle, TOGGLE, cx);
    assert_eq!(
        rect_of(&ws, cx, terminal),
        Rect::new(40.0, 400.0, 400.0, 300.0),
        "where it was"
    );
}

#[gpui_kit::test]
fn ctrl_z_brings_a_closed_tile_back_into_the_grid(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    ws.update(cx, |ws, _| ws.space.selected = vec![kanban, note]);
    press(handle, TOGGLE, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close(note, window, cx))
    })
    .unwrap();
    press(handle, UNDO, cx);
    ws.read_with(cx, |ws, _| {
        let tiles = ws.space.grid.as_ref().unwrap().tiles.ids();
        assert_eq!(tiles.len(), 2, "back in the grid: {tiles:?}");
        assert!(tiles.contains(&kanban));
    });
}
