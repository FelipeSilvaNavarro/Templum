//! The settings panel and its fields.

use super::*;

#[gpui_kit::test]
fn the_settings_shortcut_and_buttons_open_and_close_the_panel(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(panel_open(&ws, cx), "the shortcut opens it");
    cx.update_window(handle, |_, window, cx| {
        window.press(SETTINGS, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(!panel_open(&ws, cx), "and closes it");
    settle_slide(handle, cx);
    ws.read_with(cx, |ws, _| assert!(ws.settings.is_none(), "slid out"));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    assert!(panel_open(&ws, cx), "the gear opens it");
    cx.update_window(handle, |_, window, cx| window.click("settings-close", cx))
        .unwrap();
    assert!(!panel_open(&ws, cx), "the close button closes it");
}

#[gpui_kit::test]
fn the_panel_opens_on_appearance_and_tabs_switch_sections(cx: &mut TestAppContext) {
    let (handle, _ws) = open(cx);
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("field-theme.mode").is_some());
        assert!(window.try_find("shortcut-new_terminal").is_none());
        window.click("settings-tab-shortcuts", cx);
        window.render_frame(cx);
        assert!(window.try_find("shortcut-new_terminal").is_some());
        assert!(window.try_find("field-theme.mode").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_blink_switch_edits_the_config_and_its_reset_removes_the_key(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-terminal", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("reset-cursor.blink").is_none(),
            "no reset while at the default"
        );
        window.click("field-cursor.blink", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(ws.read(cx).config.cursor.blink, "the switch turned it on");
        assert!(window.try_find("reset-cursor.blink").is_some());
        window.click("reset-cursor.blink", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(!ws.read(cx).config.cursor.blink, "back to the default");
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap_or_default();
    assert!(!text.contains("blink"), "{text}");
}

#[gpui_kit::test]
fn the_integration_and_notification_switches_turn_them_off_and_back(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    let on = |ws: &Workspace| {
        (
            ws.config.shell.integration,
            ws.config.terminal.notifications,
            ws.config.usage.claude_limits,
            ws.config.connections.enabled,
            ws.config.connections.sound,
            ws.config.canvas.focus_follows_pointer,
        )
    };
    for (tab, field, reset, line) in [
        (
            "settings-tab-shell",
            "field-shell.integration",
            "reset-shell.integration",
            "integration = false",
        ),
        (
            "settings-tab-terminal",
            "field-terminal.notifications",
            "reset-terminal.notifications",
            "notifications = false",
        ),
        (
            "settings-tab-usage",
            "field-usage.claude_limits",
            "reset-usage.claude_limits",
            "claude_limits = false",
        ),
        (
            "settings-tab-connections",
            "field-connections.enabled",
            "reset-connections.enabled",
            "enabled = false",
        ),
        (
            "settings-tab-connections",
            "field-connections.sound",
            "reset-connections.sound",
            "sound = false",
        ),
        (
            "settings-tab-canvas",
            "field-canvas.focus_follows_pointer",
            "reset-canvas.focus_follows_pointer",
            "focus_follows_pointer = false",
        ),
    ] {
        cx.update_window(handle, |_, window, cx| {
            window.click(tab, cx);
            window.render_frame(cx);
            assert!(window.try_find(reset).is_none());
            // Under three rows of tabs, a switch may be below the fold of a small window.
            window.scroll(
                "settings-scroll",
                ScrollDelta::Pixels(point(px(0.), px(-5000.))),
                cx,
            );
            window.render_frame(cx);
            window.click(field, cx);
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .unwrap();
        ws.update(cx, |ws, _| ws.save());
        let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
        assert!(text.contains(line), "{text}");
        cx.update_window(handle, |_, window, cx| {
            window.click(reset, cx);
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .unwrap();
    }
    assert_eq!(
        ws.read_with(cx, |ws, _| on(ws)),
        (true, true, true, true, true, true),
        "back on"
    );
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        !text.contains("integration")
            && !text.contains("notifications")
            && !text.contains("claude_limits")
            && !text.contains("enabled")
            && !text.contains("sound")
            && !text.contains("focus_follows_pointer"),
        "{text}"
    );
}

#[gpui_kit::test]
fn connections_give_the_servers_command_to_copy_and_ask_before_gemini(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-connections", cx);
        window.render_frame(cx);
        assert!(window.try_find("connections-command").is_some());
        assert!(
            window
                .try_find("field-connections.question_timeout")
                .is_some(),
            "Questions wait"
        );
        window.click("connections-copy", cx);
        // Below the questions' rows, maybe below the fold of a small window.
        window.scroll(
            "settings-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-5000.))),
            cx,
        );
        window.render_frame(cx);
        window.click("gemini-setup", cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "it asks first");
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let bin = data.path().join("bin").join(if cfg!(windows) {
        "templum.exe"
    } else {
        "templum"
    });
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()),
        Some(format!("{} mcp", bin.display()))
    );
    ws.read_with(cx, |ws, _| {
        assert!(ws.gemini_job.is_none(), "Cancel runs nothing")
    });
}

#[gpui_kit::test]
fn the_shell_list_sets_the_program_and_its_arguments(cx: &mut TestAppContext) {
    use templum_terminal::ShellOption;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    // Two shells "found" on this computer.
    ws.update(cx, |ws, _| {
        ws.shells = vec![
            ShellOption {
                label: "Git Bash".into(),
                program: "C:\\Git\\bin\\bash.exe".into(),
                args: vec!["--login".into(), "-i".into()],
            },
            ShellOption {
                label: "PowerShell 7".into(),
                program: "pwsh".into(),
                args: vec!["-NoLogo".into()],
            },
        ];
        ws.default_shell = "Git Bash".into();
    });
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-shell", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("reset-shell.shell").is_none(),
            "the default"
        );
        window
            .within("field-shell.shell")
            .click("menu-shell.shell", cx);
        window.render_frame(cx);
        // Default (Git Bash), Git Bash, PowerShell 7.
        window.within("popup-menu").click(2usize, cx);
        window.render_frame(cx);
        window.render_frame(cx);
        let shell = ws.read(cx).config.shell.clone();
        assert_eq!(shell.program.as_deref(), Some("pwsh"));
        assert_eq!(shell.args, ["-NoLogo"]);
        assert!(window.try_find("reset-shell.shell").is_some());
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        text.contains("program = \"pwsh\"") && text.contains("-NoLogo"),
        "{text}"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click("reset-shell.shell", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        let shell = ws.read(cx).config.shell.clone();
        assert_eq!(
            (shell.program, shell.args.len()),
            (None, 0),
            "back to the default"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn esc_closes_the_panel_only_from_inside_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
        window.press("escape", cx);
    })
    .unwrap();
    assert!(!panel_open(&ws, cx), "Esc in the panel closes it");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
    })
    .unwrap();
    settle_slide(handle, cx);
    // Click the terminal's left part, clear of the panel on the right.
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
        window.press("escape", cx);
        let focused = ws.read(cx).focus_handle.is_focused(window);
        assert!(focused, "keys go to the terminal");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, Some(id)));
    assert!(panel_open(&ws, cx), "Esc went to the terminal");
}

#[gpui_kit::test]
fn closing_the_panel_saves_at_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("cursor", "blink", true),
            |c| c.cursor.blink = true,
            cx,
        )
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
    })
    .unwrap();
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("blink = true"), "{text}");
}

#[gpui_kit::test]
fn a_number_applies_on_enter_or_a_step_not_while_typing(cx: &mut TestAppContext) {
    use crate::settings::{NumberState, SCROLLBACK};
    use gpui_kit::component::input::{InputEvent, NumberInputEvent, StepAction};
    let (handle, ws) = open(cx);
    let state = cx
        .update_window(handle, |_, window, cx| {
            let ws = ws.clone();
            cx.new(|cx| NumberState::new(ws, &SCROLLBACK, 10_000.0, window, cx))
        })
        .unwrap();
    let scrollback =
        |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.config.terminal.scrollback);
    // Typing "20000" passes through "2": applied, it would trim every terminal's history.
    cx.update_window(handle, |_, window, cx| {
        let input = state.read(cx).input.clone();
        input.update(cx, |input, cx| input.set_value("2", window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(scrollback(cx), 10_000, "not while typing");
    cx.update_window(handle, |_, window, cx| {
        let input = state.read(cx).input.clone();
        input.update(cx, |input, cx| {
            input.set_value("20000", window, cx);
            cx.emit(InputEvent::PressEnter {
                secondary: false,
                shift: false,
            });
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(scrollback(cx), 20_000, "Enter applies it");
    cx.update_window(handle, |_, window, cx| {
        let input = state.read(cx).input.clone();
        input.update(cx, |_, cx| {
            cx.emit(NumberInputEvent::Step(StepAction::Decrement))
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(scrollback(cx), 19_000, "a step applies at once");
}

#[gpui_kit::test]
fn a_reset_or_another_folder_choice_drops_what_was_typed(cx: &mut TestAppContext) {
    use crate::settings::{
        args_error, args_text, folder_hint, folder_text, set_args, set_folder, set_folder_kind,
    };
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| set_args(ws, "--login \"a b", cx));
    frame(handle, cx);
    // What the field's "reset to default" does.
    ws.update(cx, |ws, cx| {
        ws.edit(Edit::remove("shell", "args"), |c| c.shell.args.clear(), cx)
    });
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(args_text(ws), "");
        assert_eq!(args_error(ws), None);
    });
    // "~" typed, then Home chosen, then "A folder…" again (it starts at the home folder).
    let steps: [fn(&mut Workspace, &mut Context<Workspace>); 4] = [
        |ws, cx| set_folder_kind(ws, "folder", cx),
        |ws, cx| set_folder(ws, "~", cx),
        |ws, cx| set_folder_kind(ws, "home", cx),
        |ws, cx| set_folder_kind(ws, "folder", cx),
    ];
    for step in steps {
        ws.update(cx, step);
        frame(handle, cx);
    }
    let home = std::env::home_dir().unwrap().display().to_string();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            folder_text(ws),
            home,
            "the folder in effect, not the old \"~\""
        );
        assert_eq!(folder_hint(ws), None);
    });
}

#[gpui_kit::test]
fn the_default_font_removes_the_family(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| {
        crate::settings::set_family(ws, "Some Mono", cx)
    });
    frame(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    let path = dir.path().join("config.toml");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("family = \"Some Mono\"")
    );
    ws.update(cx, |ws, cx| crate::settings::set_family(ws, "", cx));
    frame(handle, cx);
    ws.update(cx, |ws, _| {
        assert_eq!(ws.config.font.family, None);
        ws.save();
    });
    assert!(!std::fs::read_to_string(&path).unwrap().contains("family"));
}

#[gpui_kit::test]
fn an_unclosed_quote_waits_for_the_closing_one(cx: &mut TestAppContext) {
    use crate::settings::set_args;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| set_args(ws, "--login \"a b", cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert!(ws.config.shell.args.is_empty(), "not applied");
        assert_eq!(
            crate::settings::args_text(ws),
            "--login \"a b",
            "kept as typed"
        );
        assert_eq!(crate::settings::args_error(ws), Some("Unclosed quote"));
    });
    ws.update(cx, |ws, cx| set_args(ws, "--login \"a b\"", cx));
    frame(handle, cx);
    ws.update(cx, |ws, _| {
        assert_eq!(ws.config.shell.args, ["--login", "a b"]);
        assert_eq!(crate::settings::args_error(ws), None);
        ws.save();
    });
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("args = [\"--login\", \"a b\"]"), "{text}");
}

#[gpui_kit::test]
fn a_bare_tilde_waits_for_the_rest_of_the_path(cx: &mut TestAppContext) {
    use crate::config::WorkingDirectory;
    use crate::settings::{folder_hint, folder_text, set_folder, set_folder_kind};
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let target = dir.path().join("work");
    ws.update(cx, |ws, cx| set_folder_kind(ws, "folder", cx));
    frame(handle, cx);
    ws.update(cx, |ws, cx| set_folder(ws, "~", cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert!(
            matches!(ws.config.shell.working_directory, WorkingDirectory::Path(_)),
            "\"~\" alone doesn't switch to Home"
        );
        assert_eq!(folder_text(ws), "~", "kept as typed");
        assert!(folder_hint(ws).is_some(), "and the field says why");
    });
    let typed = target.display().to_string();
    ws.update(cx, |ws, cx| set_folder(ws, &typed, cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.config.shell.working_directory,
            WorkingDirectory::Path(target.clone())
        );
        assert_eq!(folder_text(ws), typed);
        assert_eq!(folder_hint(ws), None);
    });
    // Written now, while the temporary folder still exists.
    ws.update(cx, |ws, _| ws.save());
}

#[gpui_kit::test]
fn start_folder_and_program_choices_write_the_file(cx: &mut TestAppContext) {
    use crate::settings::{set_folder_kind, set_program};
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let path = dir.path().join("config.toml");
    let mut check = |change: &dyn Fn(&mut Workspace, &mut Context<Workspace>), want: &str| {
        ws.update(cx, |ws, cx| change(ws, cx));
        frame(handle, cx);
        ws.update(cx, |ws, _| ws.save());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(want), "want {want:?} in {text}");
    };
    check(
        &|ws, cx| set_folder_kind(ws, "home", cx),
        "working_directory = \"~\"",
    );
    check(
        &|ws, cx| set_folder_kind(ws, "inherit", cx),
        "working_directory = \"inherit\"",
    );
    check(&|ws, cx| set_program(ws, "fish", cx), "program = \"fish\"");
    ws.update(cx, |ws, cx| set_program(ws, "", cx));
    frame(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    assert!(!std::fs::read_to_string(&path).unwrap().contains("program"));
}

#[gpui_kit::test]
fn the_program_is_saved_trimmed_but_typing_keeps_its_spaces(cx: &mut TestAppContext) {
    use crate::settings::{program_text, set_program};
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    // Halfway through typing "C:\Program Files\…".
    ws.update(cx, |ws, cx| set_program(ws, "C:\\Program ", cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.shell.program.as_deref(), Some("C:\\Program"));
        assert_eq!(
            program_text(ws),
            "C:\\Program ",
            "the space stays in the field"
        );
    });
    ws.update(cx, |ws, cx| set_program(ws, " fish ", cx));
    frame(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("program = \"fish\""), "{text}");
    ws.update(cx, |ws, cx| set_program(ws, "  ", cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.config.shell.program, None));
}

#[gpui_kit::test]
fn the_tools_section_shows_gits_version_and_sets_the_program(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.toggle_settings(window, cx));
    })
    .unwrap();
    settle(handle, cx);
    // The panel slides in in real time: clicks land once it is at rest.
    settle_slide(handle, cx);
    ws.read_with(cx, |ws, _| {
        let version = ws.git_version.clone().unwrap().unwrap();
        assert!(version.starts_with("git version "), "{version}");
    });
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-tools", cx);
        window.render_frame(cx);
        assert!(window.try_find("field-git.program").is_some());
        assert!(
            window.try_find("reset-git.program").is_none(),
            "the default"
        );
    })
    .unwrap();
    // What the field calls on each change, as the shell program's tests do.
    ws.update(cx, |ws, cx| {
        crate::settings::set_git_program(ws, "/opt/git", cx)
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        assert_eq!(ws.read(cx).config.git.program, "/opt/git");
        assert!(window.try_find("reset-git.program").is_some());
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        text.contains("[git]") && text.contains("program = \"/opt/git\""),
        "{text}"
    );
}

// Re-graded from minor: a space can be typed in Settings ▸ Tools' Git field.
#[gpui_kit::test]
fn the_git_program_field_keeps_a_typed_space(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.update(cx, |ws, cx| {
        crate::settings::set_git_program(ws, "C:\\Program ", cx)
    });
    ws.update(cx, |ws, _| {
        let git = &ws.pending.as_ref().unwrap_or(&ws.config).git;
        assert_eq!(
            git.program, "C:\\Program ",
            "the field shows what was typed"
        );
        assert_eq!(git.program(), "C:\\Program", "what runs is trimmed");
    });
    ws.update(cx, |ws, cx| crate::settings::set_git_program(ws, "  ", cx));
    ws.update(cx, |ws, _| {
        assert_eq!(
            ws.pending.as_ref().unwrap_or(&ws.config).git.program(),
            "git"
        )
    });
}

/// Opens Settings ▸ Agents, scrolled to its end: the accounts.
fn open_accounts(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-agents", cx);
        window.render_frame(cx);
        window.scroll(
            "settings-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-8000.))),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
}

/// Picks the option at `index` of the dropdown `id`.
fn pick(handle: AnyWindowHandle, cx: &mut TestAppContext, id: &'static str, index: usize) {
    cx.update_window(handle, |_, window, cx| {
        window.click(id, cx);
        window.render_frame(cx);
        window.within("popup-menu").click(index, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// The Add an account dialog with Claude Code chosen.
fn add_claude_account(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        window.click("add-account", cx);
        window.render_frame(cx);
        assert!(gpui_kit::component::WindowExt::has_active_dialog(
            window, cx
        ));
    })
    .unwrap();
    // The dialog slides in for a quarter of a second: a click while it moves can miss.
    std::thread::sleep(std::time::Duration::from_millis(300));
    frame(handle, cx);
    let claude = ws.read_with(cx, |ws, _| {
        crate::settings::accounts::agent_choices(&ws.agents)
            .iter()
            .position(|(id, _)| id == "claude")
            .unwrap()
    });
    pick(handle, cx, "account-agent", claude);
}

fn save_dialog(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn adding_an_account_in_settings_writes_it_and_its_links(cx: &mut TestAppContext) {
    let own = tempfile::tempdir().unwrap();
    crate::agents::OWN_FOLDERS.set(Some(own.path().to_path_buf()));
    let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (handle, ws) = open_in(
        cx,
        Some(config.path().to_path_buf()),
        Some(data.path().to_path_buf()),
    );
    open_accounts(handle, cx);
    add_claude_account(handle, &ws, cx);
    // The agent's own folder comes first.
    pick(handle, cx, "account-share-all", 0);
    save_dialog(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(config.path().join(crate::config::FILE)).unwrap();
    let agents = crate::config::parse(&text).unwrap().agents;
    let claude2 = &agents["claude2"];
    assert_eq!(claude2.of.as_deref(), Some("claude"));
    assert_eq!(claude2.name.as_deref(), Some("Claude Code 2"));
    let share = claude2.share.as_ref().unwrap();
    let kind = templum_usage::accounts::kind("claude").unwrap();
    assert_eq!(share.len(), kind.items.len());
    assert!(share.values().all(|to| to == "claude"), "{text}");
    let account = data.path().join("accounts").join("claude2");
    assert_eq!(
        std::fs::read_link(account.join("skills")).unwrap(),
        own.path().join("claude").join("skills")
    );
    assert!(
        !account.join(".credentials.json").exists(),
        "its login is its own"
    );
}

#[gpui_kit::test]
fn the_agents_rows_leave_accounts_out_and_reset_all_leaves_them(cx: &mut TestAppContext) {
    let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(
        config.path().join(crate::config::FILE),
        format!(
            "[agents.claude]\ncommand = \"claude --model opus\"\n{}",
            super::agents::CLAUDE2
        ),
    )
    .unwrap();
    let (handle, ws) = open_in(
        cx,
        Some(config.path().to_path_buf()),
        Some(data.path().to_path_buf()),
    );
    open_accounts(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(
            window.try_find("field-agents.claude2").is_none(),
            "not among the agents"
        );
        assert!(
            window.try_find("field-accounts.claude2").is_some(),
            "under Accounts"
        );
        window.scroll(
            "settings-scroll",
            ScrollDelta::Pixels(point(px(0.), px(8000.))),
            cx,
        );
        window.render_frame(cx);
        window.click("settings-reset-section", cx);
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let agents = &ws.pending.as_ref().unwrap_or(&ws.config).agents;
        assert!(
            !agents.contains_key("claude"),
            "Claude Code back to Templum's"
        );
        assert!(agents.contains_key("claude2"), "the account stays");
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_folder_set_up_by_hand_keeps_its_sharing(cx: &mut TestAppContext) {
    let own = tempfile::tempdir().unwrap();
    crate::agents::OWN_FOLDERS.set(Some(own.path().to_path_buf()));
    let skills = own.path().join("claude").join("skills");
    std::fs::create_dir_all(&skills).unwrap();
    let mine = tempfile::tempdir().unwrap();
    let dotfiles = mine.path().join("dotfiles").join("agents");
    std::fs::create_dir_all(&dotfiles).unwrap();
    let folder = mine.path().join("work");
    std::fs::create_dir_all(&folder).unwrap();
    std::os::unix::fs::symlink(&skills, folder.join("skills")).unwrap();
    std::os::unix::fs::symlink(&dotfiles, folder.join("agents")).unwrap();
    let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (handle, ws) = open_in(
        cx,
        Some(config.path().to_path_buf()),
        Some(data.path().to_path_buf()),
    );
    open_accounts(handle, cx);
    add_claude_account(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("account-folder", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_input(handle, &folder.display().to_string());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("account-share-skills").is_some());
        assert!(
            window.try_find("account-share-agents").is_none(),
            "the person's link: a line in place of its dropdown"
        );
    })
    .unwrap();
    save_dialog(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(config.path().join(crate::config::FILE)).unwrap();
    let agents = crate::config::parse(&text).unwrap().agents;
    assert_eq!(
        agents["claude2"].share.as_ref().unwrap()["skills"],
        "claude",
        "{text}"
    );
    assert_eq!(std::fs::read_link(folder.join("skills")).unwrap(), skills);
    assert_eq!(std::fs::read_link(folder.join("agents")).unwrap(), dotfiles);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_folder_change_waits_for_the_blocks_and_keeps_the_old_one(cx: &mut TestAppContext) {
    let (data, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    super::agents::with_agents(
        &ws,
        cx,
        "[agents.claude2]\nof = \"claude\"\n[agents.claude3]\nof = \"claude\"\n",
    );
    let folder = tempfile::tempdir().unwrap();
    let block = super::agents::launch(handle, &ws, cx, "claude2", folder.path());
    let old = data.path().join("accounts").join("claude2");
    let new = elsewhere.path().join("w");
    let moved = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let entry = ws
                    .agents
                    .iter()
                    .find(|a| a.id == "claude2")
                    .unwrap()
                    .clone();
                let draft = crate::agents::AccountDraft {
                    folder: Some(new.display().to_string()),
                    ..crate::agents::AccountDraft::of(&entry)
                };
                ws.save_account(Some("claude2"), &draft, window, cx)
            })
        })
        .unwrap()
    };
    let refused = moved(&ws, cx).unwrap_err();
    assert_eq!(
        refused,
        "Claude Code (claude2) has 1 block: close them first to change its folder"
    );
    ws.update(cx, |ws, cx| {
        ws.space.blocks.remove(&block);
        ws.space.canvas.remove(block);
        cx.notify();
    });
    assert_eq!(moved(&ws, cx), Ok("claude2".to_string()));
    assert!(new.is_dir(), "its choices applied in the new folder");
    assert!(old.is_dir(), "the old one, with its login, stays");
}

#[cfg(unix)]
#[gpui_kit::test]
fn log_in_opens_a_block_of_the_account(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let script = "echo \"dir=$CLAUDE_CONFIG_DIR;\"; exec cat";
    let command = ("sh".to_string(), vec!["-c".to_string(), script.to_string()]);
    let (handle, ws) = open_as(cx, Some(command), None, Some(data.path().to_path_buf()));
    super::agents::with_agents(&ws, cx, super::agents::CLAUDE2);
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.log_in("claude2", window, cx))
        })
        .unwrap()
        .expect("a block");
    let account = data.path().join("accounts").join("claude2");
    super::bus::screen_with(&ws, cx, id, &format!("dir={};", account.display()));
    // The test's shell echoes it: no agent ever runs here.
    super::bus::screen_with(&ws, cx, id, "claude auth login");
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.terminal(id).unwrap().account.as_deref(),
            Some("claude2")
        );
    });
}

/// Deletes the account `id`, as its dialog's Delete does once asked.
fn delete(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: &str) -> Result<(), String> {
    ws.update(cx, |ws, cx| ws.delete_account(id, cx))
}

#[gpui_kit::test]
fn deleting_an_account_removes_its_folder_only(cx: &mut TestAppContext) {
    let own = tempfile::tempdir().unwrap();
    crate::agents::OWN_FOLDERS.set(Some(own.path().to_path_buf()));
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    super::agents::with_agents(
        &ws,
        cx,
        "[agents.claude2]\nof = \"claude\"\nshare = { skills = \"claude\" }\n",
    );
    let folder = tempfile::tempdir().unwrap();
    let block = super::agents::launch(handle, &ws, cx, "claude2", folder.path());
    ws.update(cx, |ws, cx| {
        ws.space.blocks.remove(&block);
        ws.space.canvas.remove(block);
        cx.notify();
    });
    let keep = own.path().join("claude").join("skills").join("keep");
    std::fs::write(&keep, "mine").unwrap();
    let account = data.path().join("accounts").join("claude2");
    assert!(account.join("skills").is_symlink());
    assert_eq!(delete(&ws, cx, "claude2"), Ok(()));
    assert!(!account.exists(), "its folder goes");
    assert_eq!(
        std::fs::read(&keep).unwrap(),
        b"mine",
        "what it shared stays"
    );
    ws.read_with(cx, |ws, _| {
        let agents = &ws.pending.as_ref().unwrap_or(&ws.config).agents;
        assert!(!agents.contains_key("claude2"));
    });
}

#[gpui_kit::test]
fn an_account_in_use_or_shared_isnt_deleted(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    super::agents::with_agents(
        &ws,
        cx,
        concat!(
            "[agents.claude2]\nof = \"claude\"\nname = \"Claude Code 2\"\n",
            "[agents.claude4]\nof = \"claude\"\nname = \"Claude Code 4\"\n",
            "share = { skills = \"claude2\" }\n",
        ),
    );
    let folder = tempfile::tempdir().unwrap();
    let running = super::agents::launch(handle, &ws, cx, "claude2", folder.path());
    // Its agent quit there: only its shell, still the account's.
    let shell = super::agents::launch(handle, &ws, cx, "claude2", folder.path());
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(shell).unwrap().seen.agent = None
    });
    assert_eq!(
        delete(&ws, cx, "claude2"),
        Err("Claude Code 2 has 2 blocks: close them first".to_string())
    );
    ws.update(cx, |ws, cx| {
        for block in [running, shell] {
            ws.space.blocks.remove(&block);
            ws.space.canvas.remove(block);
        }
        cx.notify();
    });
    assert_eq!(
        delete(&ws, cx, "claude2"),
        Err("Claude Code 4 shares Claude Code 2's skills: change it first".to_string())
    );
    assert!(
        data.path().join("accounts").join("claude2").is_dir(),
        "nothing deleted"
    );
}

#[gpui_kit::test]
fn a_chosen_folder_stays(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let chosen = tempfile::tempdir().unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    super::agents::with_agents(
        &ws,
        cx,
        &format!(
            "[agents.claude2]\nof = \"claude\"\nfolder = \"{}\"\n",
            chosen.path().display()
        ),
    );
    assert_eq!(delete(&ws, cx, "claude2"), Ok(()));
    assert!(
        chosen.path().is_dir(),
        "a folder the person chose is theirs"
    );
}

#[gpui_kit::test]
fn an_account_folder_in_templums_folder_for_accounts_is_refused(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let claude2 = data.path().join("accounts").join("claude2");
    std::fs::create_dir_all(&claude2).unwrap();
    std::fs::write(claude2.join(".credentials.json"), "claude3's login").unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    // Written before Templum refused it: test agents are read without the data folder.
    super::agents::with_agents(
        &ws,
        cx,
        &format!(
            "[agents.claude2]\nof = \"claude\"\n[agents.claude3]\nof = \"claude\"\nname = \"Claude Code 3\"\nfolder = '{}'\n",
            claude2.display()
        ),
    );
    assert_eq!(
        delete(&ws, cx, "claude2"),
        Err("Claude Code 3's folder is in Claude Code (claude2)'s: change it first".to_string())
    );
    assert_eq!(
        std::fs::read(claude2.join(".credentials.json")).unwrap(),
        b"claude3's login"
    );
    let saved = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let entry = ws
                    .agents
                    .iter()
                    .find(|a| a.id == "claude3")
                    .unwrap()
                    .clone();
                let draft = crate::agents::AccountDraft::of(&entry);
                ws.save_account(Some("claude3"), &draft, window, cx)
            })
        })
        .unwrap();
    assert!(
        saved
            .unwrap_err()
            .contains("is Templum's folder for accounts: leave Folder empty")
    );
}

#[gpui_kit::test]
fn a_folder_of_other_files_isnt_taken_for_an_account(cx: &mut TestAppContext) {
    let own = tempfile::tempdir().unwrap();
    crate::agents::OWN_FOLDERS.set(Some(own.path().to_path_buf()));
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    // Like a home folder: its own projects/, which sharing sessions would keep aside.
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("projects").join("app")).unwrap();
    let draft = crate::agents::AccountDraft {
        of: "claude".to_string(),
        show: true,
        folder: Some(home.path().display().to_string()),
        share: [("sessions".to_string(), "claude".to_string())].into(),
        ..Default::default()
    };
    let saved = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.save_account(None, &draft, window, cx))
        })
        .unwrap();
    assert_eq!(
        saved,
        Err(format!(
            "{} already holds files and isn't a Claude Code folder: choose a new or empty folder",
            crate::agents::tilde(home.path())
        ))
    );
    assert!(home.path().join("projects").join("app").is_dir());
    assert!(!home.path().join("projects.own").exists(), "nothing moved");
}

/// The row's Log in opens a block of the account; its dialog's Delete asks, then deletes it.
#[gpui_kit::test]
fn an_accounts_row_logs_in_and_its_dialog_deletes_it(cx: &mut TestAppContext) {
    let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(
        config.path().join(crate::config::FILE),
        super::agents::CLAUDE2,
    )
    .unwrap();
    let (handle, ws) = open_in(
        cx,
        Some(config.path().to_path_buf()),
        Some(data.path().to_path_buf()),
    );
    open_accounts(handle, cx);
    let blocks = ws.read_with(cx, |ws, _| ws.space.blocks.len());
    cx.update_window(handle, |_, window, cx| {
        window.click("log-in-claude2", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let logged = ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.blocks.len(), blocks + 1);
        ws.space
            .terminals()
            .find(|(_, p)| p.account.as_deref() == Some("claude2"))
            .map(|(id, _)| id)
            .expect("a block of the account")
    });
    ws.update(cx, |ws, cx| {
        ws.space.blocks.remove(&logged);
        ws.space.canvas.remove(logged);
        cx.notify();
    });
    open_accounts(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("edit-account-claude2", cx);
        window.render_frame(cx);
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        // Below the items: the dialog scrolls.
        window.scroll(
            "account-name",
            ScrollDelta::Pixels(point(px(0.), px(-2000.))),
            cx,
        );
        window.render_frame(cx);
        window.click("delete-account", cx);
        window.render_frame(cx);
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    frame(handle, cx);
    let account = data.path().join("accounts").join("claude2");
    assert!(account.is_dir(), "asked first");
    cx.update_window(handle, |_, window, cx| {
        // The question, over the account's dialog.
        window.within("dialog-1").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(!account.exists());
    ws.read_with(cx, |ws, _| {
        let agents = &ws.pending.as_ref().unwrap_or(&ws.config).agents;
        assert!(!agents.contains_key("claude2"));
    });
}

/// Typing a folder keeps the choices made: an empty folder (or `/` on the way to one) has no
/// links to read, which isn't a choice to share nothing.
#[gpui_kit::test]
fn typing_a_folder_keeps_the_shares_chosen(cx: &mut TestAppContext) {
    let own = tempfile::tempdir().unwrap();
    crate::agents::OWN_FOLDERS.set(Some(own.path().to_path_buf()));
    let folder = tempfile::tempdir().unwrap();
    let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (handle, ws) = open_in(
        cx,
        Some(config.path().to_path_buf()),
        Some(data.path().to_path_buf()),
    );
    open_accounts(handle, cx);
    add_claude_account(handle, &ws, cx);
    pick(handle, cx, "account-share-all", 0);
    cx.update_window(handle, |_, window, cx| {
        window.click("account-folder", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_input(handle, &folder.path().display().to_string());
    frame(handle, cx);
    save_dialog(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(config.path().join(crate::config::FILE)).unwrap();
    let agents = crate::config::parse(&text).unwrap().agents;
    let kind = templum_usage::accounts::kind("claude").unwrap();
    assert_eq!(
        agents["claude2"].share.as_ref().map(|s| s.len()),
        Some(kind.items.len()),
        "{text}"
    );
    assert!(folder.path().join("skills").is_symlink());
}
