//! The title bar, the toolbar and the canvas menus.

use super::*;

#[gpui_kit::test]
fn title_bar_buttons_reset_zoom_and_open_terminals(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_IN, cx);
        window.render_frame(cx);
        window.click("zoom-level", cx);
        window.render_frame(cx);
        window.click("tool-terminal", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.zoom(), 1.0);
        assert_eq!(ws.space.canvas.blocks().len(), 2);
    });
}

#[gpui_kit::test]
fn the_toolbar_adds_blocks_and_the_title_bar_lost_its_add_buttons(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.find("toolbar");
        assert!(
            window.try_find("new-terminal").is_none(),
            "no + in the title bar"
        );
        assert!(window.try_find("new-kanban").is_none());
        window.click("tool-terminal", cx);
        window.render_frame(cx);
        window.click("tool-note", cx);
        window.render_frame(cx);
        window.click("tool-kanban", cx);
        window.render_frame(cx);
        window.find("tool-editor");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminals().count(), 2);
        let kinds = |f: fn(&crate::workspace::Body) -> bool| {
            ws.space.blocks.values().filter(|b| f(b)).count()
        };
        assert_eq!(kinds(|b| b.note().is_some()), 1);
        assert_eq!(kinds(|b| b.kanban().is_some()), 1);
        assert_eq!(
            ws.tool,
            crate::workspace::Tool::Select,
            "block tools don't stay on"
        );
    });
}

#[gpui_kit::test]
fn the_toolbar_agent_button_lists_the_agents(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-agent", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.menu.is_some(), "the agents menu is open")
    });
}

#[gpui_kit::test]
fn the_canvas_menu_adds_blocks_where_it_was_opened(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // Left of the startup terminal, then right of it: empty canvas both times.
    for (item, at) in [(0usize, (20.0, 600.0)), (1, (1100.0, 20.0))] {
        let world = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let o = ws.read(cx).origin;
                let at = (f32::from(o.x) + at.0, f32::from(o.y) + at.1);
                right_click(window, cx, at);
                window.render_frame(cx);
                assert!(ws.read(cx).menu.is_some(), "the menu opened");
                let ws = ws.read(cx);
                ws.space
                    .canvas
                    .camera
                    .to_world(ws.local(point(px(at.0), px(at.1))))
            })
            .unwrap();
        cx.update_window(handle, |_, window, cx| {
            // AI agent ▸, Add ▸ Terminal or Kanban board.
            window.within("popup-menu").click(1usize, cx);
            window.render_frame(cx);
            window
                .within("submenu")
                .within("popup-menu")
                .click(item, cx);
            window.render_frame(cx);
        })
        .unwrap();
        ws.read_with(cx, |ws, _| {
            let top = ws.space.canvas.top().unwrap();
            assert_eq!(ws.space.canvas.get(top).unwrap().rect.origin(), world);
            assert_eq!(ws.space.kanban(top).is_some(), item == 1);
            assert!(ws.menu.is_none(), "a choice closes it");
        });
    }
}

#[gpui_kit::test]
fn the_canvas_menu_opens_an_agent_in_a_recent_folder(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let folder = tempfile::tempdir().unwrap();
    ws.update(cx, |ws, _| {
        ws.space.recent = vec![folder.path().to_path_buf()];
        // One agent, installed everywhere: the menu's first, whatever this computer has.
        ws.agents = vec![crate::agents::AgentEntry {
            id: "mine".into(),
            title: "Mine".into(),
            command: quiet().0,
            resume: None,
            install: None,
            builtin: None,
            changed: false,
            show: true,
            generate: None,
            models: Vec::new(),
        }];
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        right_click(window, cx, (f32::from(o.x) + 20.0, f32::from(o.y) + 600.0));
        window.render_frame(cx);
        // AI agent ▸ its first agent.
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
        window
            .within("submenu")
            .within("popup-menu")
            .click(0usize, cx);
        window.render_frame(cx);
        assert!(
            window.try_find("choose-folder").is_some(),
            "the folder dialog"
        );
    })
    .unwrap();
    // The dialog slides in for 0.25 s first: presses meanwhile are lost.
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("recent-0", cx);
        window.render_frame(cx);
        assert!(window.try_find("choose-folder").is_none(), "it closed");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminals().count(), 2, "the agent's terminal");
        assert_eq!(ws.space.recent, [folder.path()]);
    });
}

#[gpui_kit::test]
fn the_innermost_menu_opens_and_acts_on_what_was_clicked(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (block, board, card) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.add_kanban_at(Point::new(900.0, 0.0), window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                let board = ws.space.kanban(block).unwrap().board.unwrap();
                let todo = ws.boards.board(board).unwrap().columns[0].id;
                let card = ws.boards.add_card(board, todo).unwrap();
                ws.space.appearing.clear();
                (block, board, card)
            })
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let card_el = gpui_kit::ElementId::Name(format!("card-{}-{card}", block.0).into());
        window.right_click(card_el, cx);
        window.render_frame(cx);
        // Open, Send to ▸, —, Delete card: the card's menu, not its block's (Board list, —,
        // Close). It asks first.
        window.within("popup-menu").click(3usize, cx);
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        let ws = ws.read(cx);
        assert!(ws.boards.card(board, card).is_none(), "the card went");
        assert!(ws.space.kanban(block).is_some(), "its block stayed");
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("body", terminal.0 as usize), cx);
        window.render_frame(cx);
        // A second right-click replaces the menu.
        window.right_click(("body", terminal.0 as usize), cx);
        window.render_frame(cx);
        // Copy, Paste, —, Fullscreen, Close terminal.
        window.within("popup-menu").click(4usize, cx);
        window.render_frame(cx);
        assert!(
            !ws.read(cx).space.terminal(terminal).is_some(),
            "the terminal closed"
        );
    })
    .unwrap();
}

#[test]
fn a_terminals_label_names_its_agent_title_and_folder_once() {
    use crate::workspace::blocks::label;
    let folder = std::path::Path::new("/code/Templum");
    assert_eq!(
        label(Some("Claude Code"), "✳ Fix tests", Some(folder)),
        "Claude Code · ✳ Fix tests · Templum"
    );
    assert_eq!(label(None, "Templum", Some(folder)), "Templum");
    assert_eq!(label(None, "", None), "");
}

#[gpui_kit::test]
fn send_to_lists_the_terminals_running_an_agent_first(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let second = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.add_terminal_at(Point::new(900.0, 0.0), window, cx);
                let id = ws.space.canvas.top().unwrap();
                ws.space.terminal_mut(id).unwrap().seen.agent = Some("claude".into());
                id
            })
        })
        .unwrap();
    ws.read_with(cx, |ws, _| {
        let list = ws.terminal_list(None);
        assert_eq!(
            list.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [second, first]
        );
        assert!(list[0].1.starts_with("Claude Code"), "{list:?}");
        // Only those working in a folder.
        let nowhere = std::path::Path::new("/nowhere");
        assert!(ws.terminal_list(Some(nowhere)).is_empty());
    });
}
