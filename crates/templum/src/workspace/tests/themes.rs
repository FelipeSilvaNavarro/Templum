//! Themes: the system appearance, the theme picker and the crossfade.

use super::*;

#[gpui_kit::test]
fn a_theme_change_crossfades_the_chrome(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    let light = ws.read_with(cx, |ws, _| {
        assert_eq!(ws.ui, ws.theme.ui, "the first theme shows at once");
        ws.theme.ui
    });
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    // The fade starts on the first frame that draws the new theme.
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.executor().advance_clock(crate::motion::THEME / 2);
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_ne!(ws.ui, ws.theme.ui, "fading");
        assert_ne!(ws.ui, light, "fading");
    });
    cx.executor()
        .advance_clock(crate::motion::THEME + std::time::Duration::from_millis(10));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.ui, ws.theme.ui, "settled"));
}

#[gpui_kit::test]
fn follows_the_system_appearance(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // The test window is light.
    ws.read_with(cx, |ws, _| assert_eq!(ws.theme.name, "Catppuccin Latte"));
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "Catppuccin Mocha");
        assert_eq!(ws.space.terminals().count(), 2);
        for pane in ws.space.terminals().map(|(_, p)| p) {
            assert_eq!(
                pane.term.palette(),
                ws.theme.palette,
                "old and new terminals"
            );
        }
    });
    cx.update(|cx| {
        let kit = gpui_kit::component::Theme::global(cx);
        assert!(kit.mode.is_dark());
    });
    ws.read_with(cx, |ws, cx| {
        assert_eq!(
            gpui_kit::component::Theme::global(cx).background,
            ws.theme.ui.canvas,
            "gpui-kit's root paints the opaque canvas"
        );
    });
}

#[gpui_kit::test]
fn config_names_pick_the_theme(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "[theme]\nlight = \"One Light\"\n",
    )
    .unwrap();
    let (_, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.read_with(cx, |ws, _| assert_eq!(ws.theme.name, "One Light"));
}

#[gpui_kit::test]
fn theme_picker_previews_cancels_and_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("One Dark")), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "One Dark", "hover previews")
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("escape", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "Catppuccin Latte", "Esc puts it back");
        assert!(ws.picker.is_none());
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "Catppuccin Frappé");
        assert!(ws.picker.is_none());
    });
    let saved = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(saved.contains("light = \"Catppuccin Frappé\""), "{saved}");
}

#[gpui_kit::test]
fn clicking_outside_the_picker_cancels(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("Apple Light")), cx);
        window.render_frame(cx);
        window.click("picker-backdrop", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "Catppuccin Latte");
        assert!(ws.picker.is_none());
        assert_eq!(
            ws.space.canvas.blocks().len(),
            1,
            "the click didn't reach the canvas"
        );
    });
}

#[gpui_kit::test]
fn a_broken_config_is_not_overwritten(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[theme\n").unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.click(("theme-item", builtin_index("One Dark")), cx);
        window.render_frame(cx);
        assert!(window.try_find("picker-error").is_some());
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "One Dark", "the choice applies anyway");
        assert!(ws.picker.is_some(), "the picker stays open to show why");
    });
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[theme\n");
}

#[gpui_kit::test]
fn appearance_change_closes_the_picker(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("One Light")), cx);
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_none());
        assert_eq!(ws.theme.name, "Catppuccin Mocha");
    });
}

#[gpui_kit::test]
fn appearance_event_without_a_mode_change_keeps_the_picker(cx: &mut TestAppContext) {
    // GPUI also reports appearance on Wayland decoration and window-control changes.
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("One Dark")), cx);
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.set_dark(false, cx));
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_some(), "still light: the picker stays open");
        assert_eq!(ws.theme.name, "One Dark", "and the preview stays");
    });
}

#[gpui_kit::test]
fn the_picker_shortcut_toggles_and_others_wait(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(PICKER, cx);
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_some());
        assert_eq!(
            ws.space.canvas.blocks().len(),
            1,
            "no new terminal under the picker"
        );
    });
    cx.update_window(handle, |_, window, cx| window.press(PICKER, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.picker.is_none(), "closed like Esc"));
}

#[gpui_kit::test]
fn choosing_in_dark_mode_saves_the_dark_theme(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.click(("theme-item", builtin_index("One Dark")), cx);
    })
    .unwrap();
    let saved = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(saved.contains("dark = \"One Dark\""), "{saved}");
    assert!(!saved.contains("light"), "{saved}");
}
