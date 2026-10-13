//! Workspaces: switching, renaming, deleting and bringing them back.

use super::*;

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
            assert!(ws.rename_space("default", "  ", window, cx).is_err());
            assert_eq!(ws.space.name, "Main", "an empty name is refused");
            ws.rename_space("default", " Work ", window, cx).unwrap();
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
            ws.rename_space("w2", "Side project", window, cx).unwrap();
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
fn enter_applies_a_typed_name(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        let target = ws.clone();
        ws.update(cx, |_, cx| {
            crate::prompt::ask_text(
                "Rename workspace",
                "Rename",
                "Main",
                move |text, window, cx| {
                    target
                        .update(cx, |ws, cx| ws.rename_space("default", &text, window, cx))
                        .is_ok()
                },
                window,
                cx,
            )
        });
        window.render_frame(cx);
        window.input("Work", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.name, "Work");
    })
    .unwrap();
}
