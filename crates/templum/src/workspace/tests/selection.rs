//! Selecting blocks (Shift+drag, Shift+click on a title), and Delete, Copy and Paste on them.

use super::*;
use crate::session::{Item, KanbanItem, NoteItem};

#[cfg(target_os = "macos")]
const COPY: &str = "cmd-c";
#[cfg(not(target_os = "macos"))]
const COPY: &str = "ctrl-c";
#[cfg(target_os = "macos")]
const PASTE: &str = "cmd-v";
#[cfg(not(target_os = "macos"))]
const PASTE: &str = "ctrl-v";

const SHIFT: Modifiers = Modifiers {
    shift: true,
    control: false,
    alt: false,
    platform: false,
    function: false,
};

/// The startup terminal moved down, a kanban block and a note ("hello") beside each other above
/// it, the camera at 100% on the world's origin: (kanban, note, terminal).
fn three_blocks(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> (BlockId, BlockId, BlockId) {
    let terminal = first_id(ws, cx);
    cx.update_window(handle, |_, window, cx| {
        let ids = ws.update(cx, |ws, cx| {
            // Touched: the startup terminal no longer pulls the camera to it.
            ws.space.pristine = false;
            ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), 1.0);
            ws.space.canvas.get_mut(terminal).unwrap().rect = Rect::new(40.0, 400.0, 400.0, 300.0);
            let kanban = Item::Kanban(KanbanItem {
                rect: Rect::new(40.0, 40.0, 220.0, 200.0).into(),
                board: None,
            });
            let note = Item::Note(NoteItem {
                rect: Rect::new(300.0, 40.0, 220.0, 200.0).into(),
                text: "hello".into(),
                preview: true,
                color: None,
            });
            let kanban = ws.add_saved(&kanban, window, cx).unwrap();
            let note = ws.add_saved(&note, window, cx).unwrap();
            ws.space.appearing.clear();
            (kanban, note, terminal)
        });
        window.render_frame(cx);
        ids
    })
    .unwrap()
}

/// A local canvas point as a window position.
fn at(ws: &Entity<Workspace>, cx: &mut TestAppContext, p: (f32, f32)) -> (f32, f32) {
    let o = ws.read_with(cx, |ws, _| ws.origin);
    (f32::from(o.x) + p.0, f32::from(o.y) + p.1)
}

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
        let copy = ws.space.blocks[&pasted[1]].note().unwrap();
        assert_eq!(copy.text(cx), "hello");
        assert!(copy.preview);
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
