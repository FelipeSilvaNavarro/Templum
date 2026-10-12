//! Note blocks.

use super::*;

/// The kanban block on top.
#[cfg(target_os = "macos")]
const NEW_NOTE: &str = "cmd-shift-m";
#[cfg(not(target_os = "macos"))]
const NEW_NOTE: &str = "ctrl-shift-m";

fn top_note(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> BlockId {
    ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        assert!(
            matches!(
                ws.space.blocks.get(&id),
                Some(crate::workspace::Body::Note(_))
            ),
            "the top block is a note"
        );
        id
    })
}

/// Opens a note with the shortcut and returns it.
fn new_note(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) -> BlockId {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW_NOTE, cx);
        ws.update(cx, |ws, _| ws.space.appearing.clear());
        window.render_frame(cx);
    })
    .unwrap();
    top_note(ws, cx)
}

#[gpui_kit::test]
fn new_note_adds_a_focused_note_whose_heading_titles_it(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    let (handle, ws) = open(cx);
    let id = new_note(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        let ws_now = ws.read(cx);
        assert_eq!(ws_now.space.focused, Some(id));
        let rect = ws_now.space.canvas.get(id).unwrap().rect;
        assert_eq!((rect.w, rect.h), (crate::note::SIZE.w, crate::note::SIZE.h));
        let crate::workspace::Body::Note(note) = &ws_now.space.blocks[&id] else {
            unreachable!()
        };
        assert!(
            note.editor.read(cx).focus_handle(cx).is_focused(window),
            "the editor has the keyboard"
        );
    })
    .unwrap();
    // Typed text reaches the editor.
    cx.simulate_input(handle, "hi");
    ws.read_with(cx, |ws, cx| {
        let crate::workspace::Body::Note(note) = &ws.space.blocks[&id] else {
            unreachable!()
        };
        assert_eq!(note.text(cx), "hi");
        assert_eq!(note.title(cx).as_ref(), "Note");
    });
    // A heading becomes the title.
    cx.update_window(handle, |_, window, cx| {
        let crate::workspace::Body::Note(note) = &ws.read(cx).space.blocks[&id] else {
            unreachable!()
        };
        let editor = note.editor.clone();
        editor.update(cx, |e, cx| e.set_value("# Hello\nhi", window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, cx| {
        let crate::workspace::Body::Note(note) = &ws.space.blocks[&id] else {
            unreachable!()
        };
        assert_eq!(note.title(cx).as_ref(), "Hello");
    });
}

#[gpui_kit::test]
fn shortcuts_run_from_inside_a_note(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    new_note(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| window.press(NEW, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.blocks.len(), 3, "terminal, note, terminal");
        assert_eq!(ws.space.terminals().count(), 2);
    });
}

#[gpui_kit::test]
fn clicking_a_note_focuses_its_editor(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    let (handle, ws) = open(cx);
    let id = new_note(handle, &ws, cx);
    let terminal = ws.read_with(cx, |ws, _| ws.space.terminals().next().unwrap().0);
    cx.update_window(handle, |_, window, cx| {
        // The terminal gets the keyboard, but stays under the note (focus_block would raise
        // it, and it covers the note).
        ws.update(cx, |ws, cx| {
            ws.space.focused = Some(terminal);
            window.focus(&ws.focus_handle, cx);
        });
        window.render_frame(cx);
        let body = window.find(("note-editor", id.0 as usize)).bounds();
        click(
            window,
            cx,
            (
                f32::from(body.origin.x) + 20.0,
                f32::from(body.origin.y) + 20.0,
            ),
            1,
        );
        window.render_frame(cx);
        let ws_now = ws.read(cx);
        assert_eq!(ws_now.space.focused, Some(id));
        let crate::workspace::Body::Note(note) = &ws_now.space.blocks[&id] else {
            unreachable!()
        };
        assert!(
            note.editor.read(cx).focus_handle(cx).is_focused(window),
            "the editor keeps the keyboard"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_preview_button_shows_the_rendered_note(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_note(handle, &ws, cx);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.find(("note-editor", n));
        window.hover(("block", n), cx);
        window.render_frame(cx);
        window.click(("preview", n), cx);
        window.render_frame(cx);
        window.find(("note-preview", n));
        assert!(window.try_find(("note-editor", n)).is_none());
        let crate::workspace::Body::Note(note) = &ws.read(cx).space.blocks[&id] else {
            unreachable!()
        };
        assert!(note.preview);
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_a_focused_note_gives_the_keyboard_back_to_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    new_note(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press(CLOSE, cx);
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(
            ws.space.blocks.values().all(|b| b.note().is_none()),
            "closed"
        );
        assert_eq!(ws.space.terminals().count(), 2, "the shortcut still works");
    });
}

#[gpui_kit::test]
fn previewing_a_focused_note_gives_the_keyboard_back_to_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_note(handle, &ws, cx);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.hover(("block", n), cx);
        window.render_frame(cx);
        window.click(("preview", n), cx);
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminals().count(), 2, "the shortcut still works");
    });
}

#[gpui_kit::test]
fn a_note_comes_back_with_its_text_and_mode(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(dir.path().to_path_buf()));
    let id = new_note(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let crate::workspace::Body::Note(note) = &ws.space.blocks[&id] else {
                unreachable!()
            };
            note.editor
                .update(cx, |e, cx| e.set_value("# Keep me\n- x", window, cx));
            ws.toggle_preview(id, window, cx);
            ws.save_canvas_now(cx);
        });
    })
    .unwrap();
    // As the_canvas_comes_back_as_it_was_left does: a second workspace on the same folder.
    let (_, again) = open_in(cx, None, Some(dir.path().to_path_buf()));
    again.read_with(cx, |ws, cx| {
        let notes: Vec<_> = ws
            .space
            .blocks
            .values()
            .filter_map(|b| match b {
                crate::workspace::Body::Note(n) => Some((n.text(cx), n.preview)),
                _ => None,
            })
            .collect();
        assert_eq!(notes, [("# Keep me\n- x".to_string(), true)]);
    });
}
