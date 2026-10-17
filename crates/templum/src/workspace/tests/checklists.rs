//! Checklist blocks: checklists in tabs, kept app-wide (never lost with a block but for blank
//! ones), their items checked, edited and deleted, and Ctrl+Z on their content.

use gpui_kit::ElementId;

use super::*;

/// Opens a checklist block with the toolbar's button and returns it.
fn new_checklist(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> BlockId {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-checklist", cx);
        ws.update(cx, |ws, _| ws.space.appearing.clear());
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        assert!(
            ws.space
                .blocks
                .get(&id)
                .and_then(|b| b.checklist())
                .is_some(),
            "the top block is a checklist"
        );
        id
    })
}

/// The checklist on screen in `block`.
fn shown(ws: &Workspace, block: BlockId) -> u64 {
    ws.space.blocks[&block]
        .checklist()
        .unwrap()
        .shown()
        .unwrap()
}

fn name(what: &str, block: BlockId, i: u64) -> ElementId {
    ElementId::Name(format!("{what}-{}-{i}", block.0).into())
}

fn click(handle: AnyWindowHandle, cx: &mut TestAppContext, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// Items "a" and "b" added to the checklist on screen in `block`, not as an undo step: (list, a).
fn two_items(ws: &Entity<Workspace>, cx: &mut TestAppContext, block: BlockId) -> (u64, u64) {
    ws.update(cx, |ws, _| {
        let list = shown(ws, block);
        let a = ws.checklists.add_item(list, None, "a", false, 1).unwrap();
        ws.checklists.add_item(list, None, "b", false, 1);
        (list, a)
    })
}

fn done(ws: &Entity<Workspace>, cx: &mut TestAppContext, list: u64, item: u64) -> bool {
    ws.read_with(cx, |ws, _| {
        ws.checklists.get(list).unwrap().item(item).unwrap().done
    })
}

fn press_undo(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    block: BlockId,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(block, window, cx));
        window.press(UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_toolbar_opens_a_block_with_a_new_checklist(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_checklist(handle, &ws, cx);
    ws.read_with(cx, |ws, cx| {
        let rect = ws.space.canvas.get(id).unwrap().rect;
        assert_eq!((rect.w, rect.h), (320.0, 360.0));
        let list = ws.checklists.get(shown(ws, id)).unwrap();
        assert_eq!(list.name, "Checklist");
        assert_eq!(
            crate::block::title(ws, &ws.space.blocks[&id], cx).as_ref(),
            "Checklist"
        );
        assert_eq!(ws.space.focused, Some(id));
    });
}

#[gpui_kit::test]
fn add_checklist_in_the_canvas_menu_opens_one_where_it_was_asked(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let world = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let o = ws.read(cx).origin;
            let at = (f32::from(o.x) + 20.0, f32::from(o.y) + 600.0);
            right_click(window, cx, at);
            window.render_frame(cx);
            let ws = ws.read(cx);
            ws.space
                .canvas
                .camera
                .to_world(ws.local(point(px(at.0), px(at.1))))
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        // AI agent ▸, Add ▸ Terminal, Kanban board, Note, Checklist.
        window.within("popup-menu").click(1usize, cx);
        window.render_frame(cx);
        window
            .within("submenu")
            .within("popup-menu")
            .click(3usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let top = ws.space.canvas.top().unwrap();
        assert!(ws.space.blocks[&top].checklist().is_some());
        assert_eq!(ws.space.canvas.get(top).unwrap().rect.origin(), world);
    });
}

#[gpui_kit::test]
fn the_box_checks_an_item_and_the_title_counts_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_checklist(handle, &ws, cx);
    let (list, a) = two_items(&ws, cx, id);
    click(handle, cx, name("checklist-box", id, a));
    assert!(done(&ws, cx, list, a));
    ws.read_with(cx, |ws, cx| {
        let title = crate::block::title(ws, &ws.space.blocks[&id], cx);
        assert_eq!(title.as_ref(), "Checklist 1/2");
    });
    click(handle, cx, name("checklist-box", id, a));
    assert!(!done(&ws, cx, list, a), "a second click unchecks it");
}

#[gpui_kit::test]
fn the_rows_x_deletes_its_item(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_checklist(handle, &ws, cx);
    let (list, a) = two_items(&ws, cx, id);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            !window.find(name("checklist-item-delete", id, a)).visible(),
            "shown only under the pointer"
        );
        window.hover(name("checklist-row", id, a), cx);
        window.render_frame(cx);
    })
    .unwrap();
    click(handle, cx, name("checklist-item-delete", id, a));
    ws.read_with(cx, |ws, _| {
        let list = ws.checklists.get(list).unwrap();
        assert!(list.item(a).is_none());
        assert_eq!(list.items.len(), 1);
    });
}

#[gpui_kit::test]
fn closing_a_blank_checklists_tab_removes_it_and_a_named_one_stays(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_checklist(handle, &ws, cx);
    let first = ws.read_with(cx, |ws, _| shown(ws, id));
    click(handle, cx, ("checklist-tab-new", id.0 as usize));
    let second = ws.read_with(cx, |ws, _| shown(ws, id));
    assert_ne!(first, second);
    ws.update(cx, |ws, _| ws.checklists.rename(first, "Groceries", 1));
    click(handle, cx, name("checklist-tab-close", id, 1));
    click(handle, cx, name("checklist-tab-close", id, 0));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.blocks[&id].checklist().unwrap().tabs.is_empty());
        assert!(ws.checklists.get(second).is_none(), "blank: gone");
        assert!(ws.checklists.get(first).is_some(), "named: kept");
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.find(("checklist-empty", id.0 as usize));
    })
    .unwrap();
    // Closing a block drops its blank checklists too.
    let other = new_checklist(handle, &ws, cx);
    let blank = ws.read_with(cx, |ws, _| shown(ws, other));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close(other, window, cx))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.checklists.get(blank).is_none()));
}

#[gpui_kit::test]
fn ctrl_z_on_a_checklist_block_undoes_a_check_and_skips_a_deleted_checklist(
    cx: &mut TestAppContext,
) {
    let (handle, ws) = open(cx);
    let id = new_checklist(handle, &ws, cx);
    let (list, a) = two_items(&ws, cx, id);
    ws.update(cx, |ws, cx| ws.toggle_item(list, a, cx));
    assert!(done(&ws, cx, list, a));
    press_undo(handle, &ws, cx, id);
    assert!(!done(&ws, cx, list, a), "undone");
    // A step of a checklist deleted since is skipped, and never brings it back.
    ws.update(cx, |ws, cx| ws.toggle_item(list, a, cx));
    let other = ws.update(cx, |ws, cx| {
        let other = ws.checklists.add(1);
        let b = ws.checklists.add_item(other, None, "b", false, 1).unwrap();
        ws.toggle_item(other, b, cx);
        ws.checklists.remove(other);
        other
    });
    press_undo(handle, &ws, cx, id);
    assert!(!done(&ws, cx, list, a), "the step before it is undone");
    ws.read_with(cx, |ws, _| assert!(ws.checklists.get(other).is_none()));
}

#[gpui_kit::test]
fn a_checklist_is_open_in_one_tab_at_most(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = new_checklist(handle, &ws, cx);
    let list = ws.update(cx, |ws, _| {
        let list = shown(ws, first);
        ws.checklists.rename(list, "Here", 1);
        list
    });
    let second = new_checklist(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_checklist(list, Some(second), window, cx)
        })
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.focused, Some(first), "shown where it is");
        assert_eq!(ws.space.blocks[&second].checklist().unwrap().tabs.len(), 1);
    });
}

#[gpui_kit::test]
fn a_pasted_checklist_block_shows_copies(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_checklist(handle, &ws, cx);
    let (list, _) = two_items(&ws, cx, id);
    ws.update(cx, |ws, _| ws.checklists.rename(list, "Deploy", 1));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.space.focused = None;
            ws.space.selected = vec![id];
            window.focus(&ws.focus_handle, cx);
        });
        window.press(COPY, cx);
        window.press(PASTE, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let pasted = ws.space.selected.clone();
        assert_eq!(pasted.len(), 1);
        assert_ne!(pasted[0], id);
        let copy = shown(ws, pasted[0]);
        assert_ne!(copy, list, "a copy, not the checklist");
        let (copy, original) = (
            ws.checklists.get(copy).unwrap(),
            ws.checklists.get(list).unwrap(),
        );
        assert_eq!(copy.name, "Deploy");
        let texts = |l: &crate::checklists::List| {
            l.items.iter().map(|i| i.text.clone()).collect::<Vec<_>>()
        };
        assert_eq!(texts(copy), texts(original));
    });
}
