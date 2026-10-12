//! Shortcuts: the hint, recording new ones, and quitting.

use super::*;
use crate::keys::{Command, Keys};

#[test]
fn the_hint_shows_the_shortcuts_in_effect() {
    use crate::keys::{Keymap, Keys, KeysConfig};
    let (keymap, _) = Keymap::new(&KeysConfig::default(), false).unwrap();
    let rows = crate::workspace::hint_keys(&keymap);
    assert_eq!(rows[0].as_ref().map(|k| k.key.as_str()), Some("t"));
    assert_eq!(rows[1].as_ref().map(|k| k.key.as_str()), Some(","));
    let keys = KeysConfig {
        new_terminal: Some(Keys::Many(vec![])),
        ..KeysConfig::default()
    };
    let (keymap, _) = Keymap::new(&keys, false).unwrap();
    assert!(
        crate::workspace::hint_keys(&keymap)[0].is_none(),
        "no shortcut, no keys"
    );
}

/// Opens the panel (unless it is open) and starts recording `command`'s shortcut.
fn start_recording(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    command: Command,
    cx: &mut TestAppContext,
) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Pressed again, the shortcut would close the panel.
        if !ws.read(cx).settings.as_ref().is_some_and(|p| !p.closing) {
            window.press(SETTINGS, cx);
        }
        window.render_frame(cx);
        ws.update(cx, |ws, cx| ws.record(command, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn recording_a_free_shortcut_applies_and_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    start_recording(handle, &ws, Command::NewTerminal, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("ctrl-alt-n", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| {
        assert_eq!(ws.recording, None);
        let pressed = gpui_kit::Keystroke::parse("ctrl-alt-n").unwrap();
        assert_eq!(ws.keymap.command(&pressed), Some(Command::NewTerminal));
        assert_eq!(
            ws.keymap.keystrokes(Command::NewTerminal).len(),
            1,
            "the only one"
        );
        ws.save();
    });
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("new_terminal = \"ctrl-alt-n\""), "{text}");
}

#[gpui_kit::test]
fn a_taken_shortcut_keeps_recording(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    cx.update_window(handle, |_, window, cx| window.press(NEW, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.recording, Some(Command::Copy));
        let shortcut = if cfg!(target_os = "macos") {
            "Cmd+T"
        } else {
            "Ctrl+Shift+T"
        };
        assert_eq!(
            ws.key_note.as_deref(),
            Some(format!("{shortcut} is already New terminal").as_str())
        );
        assert_eq!(ws.space.canvas.blocks().len(), 1, "no shortcut ran");
        assert!(ws.config.keys.copy.is_none(), "nothing changed");
    });
}

#[gpui_kit::test]
fn esc_cancels_recording_and_backspace_clears(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.recording, None);
        assert!(ws.config.keys.copy.is_none());
    });
    assert!(
        panel_open(&ws, cx),
        "Esc cancelled the recording, not the panel"
    );
    start_recording(handle, &ws, Command::Copy, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("backspace", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.keys.copy, Some(Keys::Many(vec![])));
        assert!(ws.keymap.keystrokes(Command::Copy).is_empty());
    });
}

#[gpui_kit::test]
fn a_modifier_alone_keeps_recording(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    // What the platforms send for a modifier key: no key press, a change of modifiers.
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::ModifiersChanged(gpui_kit::ModifiersChangedEvent {
                modifiers: Modifiers::shift(),
                capslock: Default::default(),
            }),
            cx,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.recording, Some(Command::Copy));
        assert_eq!(ws.key_note, None);
    });
}

/// A key held down: the platform repeats its press.
fn hold(key: &str, window: &mut Window, cx: &mut gpui_kit::App) {
    window.dispatch_event(
        PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
            keystroke: gpui_kit::Keystroke::parse(key).unwrap(),
            is_held: true,
            prefer_character_input: false,
        }),
        cx,
    );
}

#[gpui_kit::test]
fn ctrl_backspace_and_ctrl_esc_record_like_other_keys(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    for key in ["ctrl-backspace", "ctrl-escape"] {
        start_recording(handle, &ws, Command::Copy, cx);
        cx.update_window(handle, |_, window, cx| {
            window.press(key, cx);
            window.render_frame(cx);
        })
        .unwrap();
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.config.keys.copy, Some(Keys::One(key.into())))
        });
    }
}

#[gpui_kit::test]
fn a_held_key_runs_nothing_from_the_panel(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Settings, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("ctrl-alt-s", cx);
        window.render_frame(cx);
        // Still held after the recording took it: the new shortcut mustn't run.
        hold("ctrl-alt-s", window, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.config.keys.settings,
            Some(Keys::One("ctrl-alt-s".into()))
        )
    });
    assert!(panel_open(&ws, cx), "the repeat didn't close the panel");
}

#[gpui_kit::test]
fn the_theme_picker_gets_its_keys_while_the_panel_has_the_focus(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
        window.press(PICKER, cx);
        window.render_frame(cx);
        window.press("down", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_some(), "the shortcut opened it");
        assert_ne!(ws.theme.name, "Catppuccin Latte", "↓ previews the next one");
    });
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_none());
        assert_eq!(ws.theme.name, "Catppuccin Latte");
    });
    assert!(panel_open(&ws, cx), "Esc closed the picker, not the panel");
}

#[gpui_kit::test]
fn canvas_shortcuts_run_from_the_panel_but_terminal_ones_dont(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
        // Close acts on the focused terminal, which doesn't have the keyboard.
        window.press(CLOSE, cx);
        window.press(ZOOM_IN, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 1, "close didn't run");
        assert!(ws.space.canvas.camera.zoom() > 1.0, "zoom did");
    });
    cx.update_window(handle, |_, window, cx| window.press(NEW, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.blocks().len(), 2));
    assert!(panel_open(&ws, cx));
}

#[gpui_kit::test]
fn a_reset_that_would_share_a_shortcut_is_refused(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let taken = Command::NewTerminal.defaults(cfg!(target_os = "macos"))[0];
    write_config(
        dir.path(),
        &format!("[keys]\nnew_terminal = \"ctrl-alt-y\"\nclose_terminal = \"{taken}\"\n"),
    );
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let reset = |command: Command, cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.reset_shortcut(command, window, cx));
            window.render_frame(cx);
        })
        .unwrap();
    };
    reset(Command::NewTerminal, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.config.keys.new_terminal,
            Some(Keys::One("ctrl-alt-y".into())),
            "close_terminal has its default"
        )
    });
    assert_eq!(notification_count(handle, cx), 1, "and says so");
    reset(Command::CloseTerminal, cx);
    reset(Command::NewTerminal, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.keys.new_terminal, None, "free again: reset");
        assert_eq!(ws.config.keys.close_terminal, None);
    });
}

#[gpui_kit::test]
fn a_key_without_ctrl_alt_or_super_is_refused(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::NewTerminal, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("n", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.recording, Some(Command::NewTerminal), "still recording");
        assert!(ws.key_note.is_some(), "and says why");
        assert!(
            ws.config.keys.new_terminal.is_none(),
            "terminals keep their n"
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.press("f5", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.config.keys.new_terminal,
            Some(Keys::One("f5".into())),
            "F keys may stand alone"
        );
    });
}

#[gpui_kit::test]
fn clicking_elsewhere_in_the_panel_stops_recording(cx: &mut TestAppContext) {
    // No config folder: the footer shows its line, a place to click.
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| window.click("settings-status", cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.recording, None));
    assert!(panel_open(&ws, cx));
}

#[gpui_kit::test]
fn clicking_a_terminal_stops_recording(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    start_recording(handle, &ws, Command::Copy, cx);
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        let (o, r) = {
            let ws = ws.read(cx);
            let rect = ws.space.canvas.get(id).unwrap().rect;
            (ws.origin, ws.space.canvas.camera.rect_to_screen(rect))
        };
        let at = (
            f32::from(o.x) + r.x + 20.0,
            f32::from(o.y) + r.y + templum_canvas::chrome::TITLE_H + 20.0,
        );
        click(window, cx, at, 1);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.recording, None));
}

#[gpui_kit::test]
fn closing_the_panel_stops_recording(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.close_settings(window, cx);
            assert_eq!(ws.recording, None);
        })
    })
    .unwrap();
}

#[cfg(target_os = "macos")]
const QUIT: &str = "cmd-q";
#[cfg(not(target_os = "macos"))]
const QUIT: &str = "ctrl-shift-q";

#[gpui_kit::test]
fn quitting_takes_a_second_press(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(QUIT, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.quit_armed.is_some(), "the first press only warns")
    });
    assert_eq!(notification_count(handle, cx), 1);
    cx.update_window(handle, |_, window, cx| window.press(QUIT, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.quit_armed.is_none(), "the second one quits")
    });
    assert_eq!(notification_count(handle, cx), 1, "no second warning");
}

#[gpui_kit::test]
fn reset_all_untangles_shortcuts_that_traded_keys(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let macos = cfg!(target_os = "macos");
    let new = Command::NewTerminal.defaults(macos)[0];
    let close = Command::CloseTerminal.defaults(macos)[0];
    write_config(
        dir.path(),
        &format!("[keys]\nnew_terminal = \"{close}\"\nclose_terminal = \"{new}\"\n"),
    );
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    // "Reset All" calls each row's reset in one go, with no frame between them.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.reset_shortcut(Command::NewTerminal, window, cx);
            ws.reset_shortcut(Command::CloseTerminal, window, cx);
        })
    })
    .unwrap();
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.keys.new_terminal, None);
        assert_eq!(ws.config.keys.close_terminal, None);
    });
    assert_eq!(notification_count(handle, cx), 0, "nothing refused");
}

#[gpui_kit::test]
fn holding_the_quit_shortcut_is_not_a_second_press(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(QUIT, cx);
        hold(QUIT, window, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.quit_armed.is_some(), "still waiting for a second press")
    });
}

#[gpui_kit::test]
fn a_late_second_press_warns_again(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let long_ago = std::time::Instant::now()
        .checked_sub(crate::workspace::QUIT_AGAIN + std::time::Duration::from_millis(100))
        .unwrap();
    ws.update(cx, |ws, _| ws.quit_armed = Some(long_ago));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(QUIT, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.quit_armed.is_some_and(|at| at > long_ago), "armed again")
    });
    assert_eq!(notification_count(handle, cx), 1);
}

#[gpui_kit::test]
fn quit_works_from_the_settings_panel(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
        window.press(QUIT, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.quit_armed.is_some()));
}

#[test]
fn editor_only_commands_need_a_focused_editor() {
    use crate::keys::Command;
    assert_eq!(
        crate::workspace::effective(Some(Command::CloseTab), false),
        None
    );
    assert_eq!(
        crate::workspace::effective(Some(Command::CloseTab), true),
        Some(Command::CloseTab)
    );
    assert_eq!(
        crate::workspace::effective(Some(Command::NewTerminal), false),
        Some(Command::NewTerminal)
    );
}
