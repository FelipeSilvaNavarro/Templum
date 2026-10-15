//! Wires between blocks (docs/specs/connections.md, Wires).

use templum_canvas::Wire;

use super::*;
use crate::workspace::history::Then;

/// A click on the empty canvas: no block keeps the keyboard.
fn unfocus(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    let empty = at(ws, cx, (700.0, 300.0));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        click(window, cx, empty, 1);
    })
    .unwrap();
}

/// The block ids (`uid`) of blocks `ids`.
fn uids<const N: usize>(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    ids: [BlockId; N],
) -> [String; N] {
    ws.read_with(cx, |ws, _| ids.map(|id| ws.space.uids[&id].clone()))
}

fn wires(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<Wire> {
    ws.read_with(cx, |ws, _| ws.space.wires.clone())
}

/// Right-clicks block `id`'s title, picks the menu's item `index` (separators count), then the
/// submenu's item `sub`.
fn pick(handle: AnyWindowHandle, cx: &mut TestAppContext, id: BlockId, index: usize, sub: usize) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.right_click(("title", id.0 as usize), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(index, cx);
        window.render_frame(cx);
        window.within("submenu").within("popup-menu").click(sub, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn two_blocks_are_wired_once_and_never_to_themselves(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        assert!(ws.connect(kanban, note, cx));
        assert!(
            !ws.connect(note, kanban, cx),
            "already wired, whichever end asks"
        );
        assert!(!ws.connect(kanban, kanban, cx), "a block to itself");
        assert!(
            !ws.connect(kanban, BlockId(999), cx),
            "a block that isn't there"
        );
        assert!(ws.space.wired(note, kanban));
        assert_eq!(ws.space.wires.len(), 1);
        assert_eq!(ws.space.undo.len(), 1, "the refused ones are no steps");
    });
}

#[gpui_kit::test]
fn ctrl_z_takes_a_wire_back_and_ctrl_shift_z_makes_it_again(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let wire = Wire::new(kanban, note).unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            assert!(ws.undo_canvas(false, window, cx));
            assert!(ws.space.wires.is_empty());
            assert!(ws.undo_canvas(true, window, cx));
            assert_eq!(ws.space.wires, [wire]);
            assert!(ws.disconnect(wire, cx));
            assert!(ws.space.wires.is_empty());
            assert!(ws.undo_canvas(false, window, cx), "unwiring is a step too");
            assert_eq!(ws.space.wires, [wire]);
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_a_block_takes_its_wires_and_undoing_brings_them_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let [kanban_uid] = uids(&ws, cx, [kanban]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            ws.connect(kanban, terminal, cx);
            ws.connect(note, terminal, cx);
            ws.close_now(&[kanban], Then::New, window, cx);
            assert_eq!(ws.space.wires, [Wire::new(note, terminal).unwrap()]);
            assert!(ws.undo_canvas(false, window, cx));
            let back = ws
                .space
                .block_by_uid(&kanban_uid)
                .expect("back, with its id");
            assert!(ws.space.wired(back, note) && ws.space.wired(back, terminal));
            assert_eq!(ws.space.wires.len(), 3);
            // Closed again (Ctrl+Shift+Z), back again (Ctrl+Z): its wires each time.
            assert!(ws.undo_canvas(true, window, cx));
            assert_eq!(ws.space.wires.len(), 1);
            assert!(ws.undo_canvas(false, window, cx));
            assert_eq!(ws.space.wires.len(), 3);
        })
    })
    .unwrap();
}

/// Two wired blocks closed one after the other, both brought back: each got a new canvas id, and
/// the wire comes back between them.
#[gpui_kit::test]
fn a_wire_comes_back_between_two_blocks_closed_one_after_the_other(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let [k, n] = uids(&ws, cx, [kanban, note]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            ws.close_now(&[kanban], Then::New, window, cx);
            ws.close_now(&[note], Then::New, window, cx);
            assert!(ws.space.wires.is_empty());
            assert!(ws.undo_canvas(false, window, cx));
            assert!(ws.space.wires.is_empty(), "its other end is still closed");
            assert!(ws.undo_canvas(false, window, cx));
            let (kanban, note) = (
                ws.space.block_by_uid(&k).unwrap(),
                ws.space.block_by_uid(&n).unwrap(),
            );
            assert_eq!(ws.space.wires, [Wire::new(kanban, note).unwrap()]);
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_block_moved_to_another_workspace_leaves_its_wires(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            ws.connect(note, terminal, cx);
            ws.move_blocks(vec![kanban], None, window, cx);
        })
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let counts: Vec<usize> = ws.spaces().map(|s| s.wires.len()).collect();
        assert_eq!(
            counts.iter().sum::<usize>(),
            1,
            "note–terminal stays: {counts:?}"
        );
    });
}

#[gpui_kit::test]
fn a_pasted_copy_of_a_wired_block_comes_without_wires(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    unfocus(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
        ws.space.selected = vec![kanban];
    });
    cx.update_window(handle, |_, window, cx| {
        window.press(COPY, cx);
        window.press(PASTE, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_ne!(ws.space.selected, [kanban], "the copy is selected");
    });
    assert_eq!(wires(&ws, cx), [Wire::new(kanban, note).unwrap()]);
}

#[gpui_kit::test]
fn a_blocks_menu_connects_and_disconnects_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    // Board list, —, Fullscreen, Connect to ▸ (the terminal, the note: back to front).
    pick(handle, cx, kanban, 3, 1);
    assert_eq!(wires(&ws, cx), [Wire::new(kanban, note).unwrap()]);
    // Board list, —, Fullscreen, Connect to ▸ (the terminal), Disconnect from ▸ (the note).
    pick(handle, cx, kanban, 4, 0);
    assert!(wires(&ws, cx).is_empty());
    // Both were steps: Ctrl+Z wires them again.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))
    })
    .unwrap();
    assert_eq!(wires(&ws, cx), [Wire::new(kanban, note).unwrap()]);
}
