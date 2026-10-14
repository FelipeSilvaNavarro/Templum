//! Workspaces: switching, renaming, deleting and bringing them back.

use super::*;
use crate::space_dialog::Look;

#[test]
fn a_workspace_shows_an_agent_from_its_last_snapshot() {
    use crate::session::{Item, Saved, SavedCamera, SavedRect, TerminalItem, VERSION};
    let with = |agent: Option<&str>| crate::workspace::Space {
        saved: Some(Saved {
            version: VERSION,
            camera: SavedCamera {
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
            },
            items: vec![Item::Terminal(TerminalItem {
                rect: SavedRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
                folder: None,
                agent: agent.map(String::from),
            })],
            focused: None,
            recent: Vec::new(),
            ink: Vec::new(),
            grid: None,
        }),
        ..crate::workspace::Space::default()
    };
    let known = crate::agents::effective(&Default::default(), &[]);
    assert!(with(Some("claude")).has_agent(&known));
    assert!(
        !with(Some("vim")).has_agent(&known),
        "not an agent Templum knows"
    );
    assert!(!with(None).has_agent(&known));
    assert!(
        !crate::workspace::Space::default().has_agent(&known),
        "no snapshot yet"
    );
}

#[test]
fn ctrl_digits_pick_a_workspace() {
    let digit = |k: &str, macos| {
        crate::workspace::workspace_digit(&gpui_kit::Keystroke::parse(k).unwrap(), macos)
    };
    assert_eq!(digit("ctrl-1", false), Some(0));
    assert_eq!(digit("ctrl-9", false), Some(8));
    assert_eq!(digit("cmd-2", true), Some(1));
    assert_eq!(digit("ctrl-2", true), None, "Cmd on macOS");
    assert_eq!(digit("ctrl-0", false), None, "zoom reset");
    assert_eq!(digit("ctrl-shift-1", false), None, "fit all");
    assert_eq!(digit("ctrl-alt-1", false), None);
    assert_eq!(digit("1", false), None);
}

#[cfg(target_os = "macos")]
const SECOND: &str = "cmd-2";
#[cfg(not(target_os = "macos"))]
const SECOND: &str = "ctrl-2";

#[gpui_kit::test]
fn a_new_workspace_is_empty_and_the_first_keeps_running(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.new_space(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!((ws.active, ws.others.len()), (1, 1));
        assert_eq!(ws.space.name, "Workspace 2");
        assert!(ws.space.canvas.blocks().is_empty(), "a new one is empty");
        assert_eq!(ws.others[0].name, "Main");
    });
    cx.update_window(handle, |_, window, cx| {
        window.find("workspace-name");
        window.click("workspace-default", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.active, 0);
        assert_eq!(ws.space.focused, Some(first));
        assert!(
            !ws.space.terminal(first).unwrap().term.exited(),
            "still running"
        );
    });
}

#[gpui_kit::test]
fn workspace_keys_cycle_and_jump(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.new_space(window, cx);
        });
        window.render_frame(cx);
        assert_eq!(ws.read(cx).active, 2);
        window.press("ctrl-tab", cx);
        assert_eq!(ws.read(cx).active, 0, "wraps around");
        window.press("ctrl-shift-tab", cx);
        assert_eq!(ws.read(cx).active, 2);
        window.press(SECOND, cx);
        assert_eq!(ws.read(cx).active, 1);
    })
    .unwrap();
}

#[gpui_kit::test]
fn hidden_workspaces_get_theme_changes(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.new_space(window, cx));
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.others[0].terminal(first).unwrap().term.palette(),
            ws.theme.palette
        );
    });
}

#[gpui_kit::test]
fn renaming_and_deleting_workspaces(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let edit =
                |ws: &mut Workspace, name, window: &mut Window, cx: &mut Context<Workspace>| {
                    ws.edit_space("default", name, Look::default(), None, window, cx)
                };
            assert!(edit(ws, "  ", window, cx).is_err());
            assert_eq!(ws.space.name, "Main", "an empty name is refused");
            edit(ws, " Work ", window, cx).unwrap();
            assert_eq!(ws.space.name, "Work");
            ws.delete_space("default", window, cx);
            assert_eq!(ws.others.len(), 0, "the last one stays");
            ws.new_space(window, cx);
            ws.open(Rect::new(0.0, 0.0, 600.0, 400.0), window, cx);
            ws.save_canvas_now(cx);
            ws.delete_space("w2", window, cx);
            assert_eq!((ws.active, ws.others.len()), (0, 0));
            assert_eq!(ws.space.name, "Work", "the one before it is shown");
        });
        window.render_frame(cx);
        let name = window.find("workspace-name");
        assert!(name.bounds().size.width > px(0.0));
    })
    .unwrap();
    let w2 = crate::session::path(data.path(), "w2");
    assert!(!w2.exists() && w2.with_extension("json.bak").exists());
    let index = crate::session::load_index(data.path());
    let crate::session::Loaded::Saved(index) = index else {
        panic!("no index: {index:?}");
    };
    assert_eq!(index.workspaces.len(), 1);
    assert_eq!(index.workspaces[0].name, "Work");
}

#[gpui_kit::test]
fn every_workspace_comes_back_in_order(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.open(Rect::new(10.0, 20.0, 600.0, 400.0), window, cx);
            ws.open(Rect::new(700.0, 20.0, 600.0, 400.0), window, cx);
            ws.edit_space("w2", "Side project", Look::default(), None, window, cx)
                .unwrap();
            ws.new_space(window, cx);
            ws.switch_to(1, window, cx);
            ws.toggle_sidebar(window, cx);
            ws.save_canvas_now(cx);
        })
    })
    .unwrap();
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        let names: Vec<&str> = ws.order().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Main", "Side project", "Workspace 3"]);
        assert_eq!(ws.active, 1);
        assert!(!ws.sidebar);
        assert_eq!(ws.space.canvas.blocks().len(), 2);
        assert_eq!(ws.others[0].canvas.blocks().len(), 1, "Main's terminal");
        assert!(ws.others[1].canvas.blocks().is_empty());
    });
}

#[gpui_kit::test]
fn enter_in_the_edit_dialog_applies_a_typed_name(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            crate::space_dialog::open(ws, "default", window, cx)
        });
        window.render_frame(cx);
        // The name is selected: typing replaces it.
        window.input("Work", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.name, "Work");
    })
    .unwrap();
}

/// The index as written in `data`.
fn written_index(data: &std::path::Path) -> crate::session::Index {
    match crate::session::load_index(data) {
        crate::session::Loaded::Saved(index) => index,
        other => panic!("no index: {other:?}"),
    }
}

#[gpui_kit::test]
fn editing_a_workspace_sets_its_look_and_cancel_keeps_the_old_one(cx: &mut TestAppContext) {
    use gpui_kit::assets::IconName;
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_menu(
                crate::workspace::menu::Target::Space("default".into()),
                point(px(100.0), px(100.0)),
                window,
                cx,
            )
        });
        window.render_frame(cx);
        // Edit….
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
        window.within("dialog").click("space-color-blue", cx);
        window.within("dialog").click("space-icon-rocket", cx);
        window.render_frame(cx);
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        assert_eq!(ws.read(cx).space.look, Look::default(), "Cancel keeps it");
        ws.update(cx, |ws, cx| {
            crate::space_dialog::open(ws, "default", window, cx)
        });
        window.render_frame(cx);
        window.within("dialog").click("space-color-blue", cx);
        window.within("dialog").click("space-icon-rocket", cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        let look = ws.read(cx).space.look.clone();
        assert_eq!(look.color.as_deref(), Some("blue"));
        assert_eq!(look.icon, Some(IconName::Rocket));
        // The title bar shows it.
        window.render_frame(cx);
    })
    .unwrap();
    let entry = &written_index(data.path()).workspaces[0];
    assert_eq!(entry.icon.as_deref(), Some("rocket"));
    assert_eq!(entry.color.as_deref(), Some("blue"));
}

#[gpui_kit::test]
fn an_empty_name_is_refused_in_the_edit_dialog(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            crate::space_dialog::open(ws, "default", window, cx)
        });
        window.render_frame(cx);
        // The name is selected: this empties it.
        window.press("backspace", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "it stays open");
        assert!(window.try_find("space-note").is_some(), "saying why");
        assert_eq!(ws.read(cx).space.name, "Main");
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_new_workspace_opens_its_edit_dialog_with_the_name_selected(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("new-workspace", cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        assert_eq!(ws.read(cx).space.name, "Workspace 2", "shown at once");
        window.input("Api", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.name, "Api");
    })
    .unwrap();
}

#[gpui_kit::test]
fn deleting_a_workspace_removes_its_image(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let pictures = tempfile::tempdir().unwrap();
    let png = pictures.path().join("me.png");
    std::fs::write(&png, b"not really a png").unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let kept = crate::session::icons_dir(data.path()).join("w2.png");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            let look = Look {
                image: Some(png.clone()),
                ..Look::default()
            };
            ws.edit_space("w2", "Me", look, None, window, cx).unwrap();
            assert_eq!(ws.space.look.image.as_ref(), Some(&kept), "a copy is shown");
        });
        // An image that can't be decoded shows the initials, and says nothing.
        window.render_frame(cx);
    })
    .unwrap();
    assert!(kept.exists());
    assert_eq!(
        written_index(data.path()).workspaces[1].image.as_deref(),
        Some("w2.png")
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.delete_space("w2", window, cx))
    })
    .unwrap();
    assert!(!kept.exists());
    assert!(png.exists(), "the file chosen stays");
}
