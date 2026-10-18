//! Agents' session usage and plan limits (docs/specs/usage.md).

use super::*;

/// The Claude Code and Codex Templum types get its server and hooks; Claude Code its status line
/// too (docs/specs/connections.md, What Templum types into each agent).
#[gpui_kit::test]
fn the_agents_templum_opens_get_its_server_hooks_and_status_line(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.update(cx, |ws, _| {
        let agent = |id: &str| ws.agents.iter().find(|a| a.id == id).cloned();
        let (claude, codex) = (agent("claude"), agent("codex"));
        let typed = |name: &str| {
            templum_usage::claude::typed_path(
                &data.path().join(name).to_string_lossy(),
                cfg!(windows),
            )
            .unwrap()
        };
        let (settings, mcp) = (typed("claude-statusline.json"), typed("mcp.json"));
        assert_eq!(
            ws.typed(claude.as_ref(), "claude --continue"),
            format!("claude --continue --settings {settings} --mcp-config {mcp}")
        );
        let written = std::fs::read_to_string(data.path().join("claude-statusline.json")).unwrap();
        assert!(
            written.contains("--claude-statusline") && written.contains(" hook claude"),
            "{written}"
        );
        let server: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(data.path().join("mcp.json")).unwrap())
                .unwrap();
        assert_eq!(
            server["mcpServers"]["templum"]["args"],
            serde_json::json!(["mcp"])
        );
        #[cfg(unix)]
        assert!(
            ws.typed(codex.as_ref(), "codex resume 0d5e")
                .starts_with("codex resume 0d5e -c 'mcp_servers.templum.command="),
            "{}",
            ws.typed(codex.as_ref(), "codex resume 0d5e")
        );
        #[cfg(unix)]
        {
            let line = ws.typed(codex.as_ref(), "codex");
            let stop = r#"-c 'hooks.Stop=[{hooks=[{type="command",command="templum hook codex",timeout=5}]}]'"#;
            assert!(line.contains(stop) && !line.contains("notify"), "{line}");
            // No `<data>/bin` on the terminal's PATH: the server, not hooks running a `templum`
            // it can't find.
            let env = ws.bus_env.clone();
            ws.bus_env.retain(|(k, _)| k != "PATH");
            let line = ws.typed(codex.as_ref(), "codex");
            assert!(
                line.contains("mcp_servers.templum") && !line.contains("hooks."),
                "{line}"
            );
            ws.bus_env = env;
        }
        assert_eq!(ws.typed(None, "claude"), "claude");
        // Windows PowerShell 5.1 passes `args=["mcp"]` on as `args=[mcp]`; pwsh 7 doesn't.
        let shell = ws.config.shell.program.clone();
        ws.config.shell.program = Some("powershell".into());
        assert_eq!(ws.typed(codex.as_ref(), "codex"), "codex");
        // Windows names its programs in any case.
        for program in ["PowerShell.exe", "CMD.EXE"] {
            ws.config.shell.program = Some(program.into());
            assert_eq!(ws.typed(codex.as_ref(), "codex"), "codex", "{program}");
        }
        ws.config.shell.program = Some("WSL.exe".into());
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
        ws.config.shell.program = Some("pwsh".into());
        assert!(
            ws.typed(codex.as_ref(), "codex")
                .contains("mcp_servers.templum")
        );
        ws.config.shell.program = shell;
        // Connections off: the status line only, and nothing for Codex.
        ws.config.connections.enabled = false;
        assert_eq!(
            ws.typed(claude.as_ref(), "claude"),
            format!("claude --settings {settings}")
        );
        assert_eq!(ws.typed(codex.as_ref(), "codex"), "codex");
        ws.config.usage.claude_limits = false;
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
        // Connections without the status line: the hooks still need the settings file.
        ws.config.connections.enabled = true;
        assert_eq!(
            ws.typed(claude.as_ref(), "claude"),
            format!("claude --settings {settings} --mcp-config {mcp}")
        );
        // A bus that couldn't start: no server, no hooks.
        let bus = ws.bus.take();
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
        ws.bus = bus;
        // Without a data folder, nothing would read what the relay writes.
        ws.data_dir = None;
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
    });
}

/// Every new terminal carries what Templum types after `claude` and `codex`, as raw words, for
/// the ones a person types there; empty when there is nothing to give (docs/specs/connections.md,
/// Agents typed in a terminal).
#[gpui_kit::test]
fn new_terminals_carry_what_templum_types_after_claude_and_codex(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    let data = parent.path().join("Templum Data");
    std::fs::create_dir(&data).unwrap();
    let (_, ws) = open_in(cx, None, Some(data.clone()));
    ws.update(cx, |ws, _| {
        let word = |ws: &Workspace, key: &str| {
            let env = ws.agent_env();
            env.into_iter().find(|(k, _)| k == key).unwrap().1
        };
        let p = |name: &str| {
            let path = data.join(name).to_string_lossy().into_owned();
            if cfg!(windows) {
                path.replace('\\', "/")
            } else {
                path
            }
        };
        let claude = format!(
            "--settings\n{}\n--mcp-config={}",
            p("claude-statusline.json"),
            p("mcp.json")
        );
        assert_eq!(word(ws, "TEMPLUM_CLAUDE_ARGS"), claude);
        #[cfg(unix)]
        {
            let exe = crate::workspace::usage::program().unwrap();
            let codex = templum_mcp::codex_args(&exe.to_string_lossy(), false, true).unwrap();
            assert_eq!(word(ws, "TEMPLUM_CODEX_ARGS"), codex.join("\n"));
        }
        ws.config.connections.enabled = false;
        assert_eq!(word(ws, "TEMPLUM_CODEX_ARGS"), "");
        assert_eq!(
            word(ws, "TEMPLUM_CLAUDE_ARGS"),
            format!("--settings\n{}", p("claude-statusline.json"))
        );
        ws.config.usage.claude_limits = false;
        assert_eq!(word(ws, "TEMPLUM_CLAUDE_ARGS"), "");
        assert_eq!(word(ws, "TEMPLUM_CODEX_ARGS"), "");
        ws.config.connections.enabled = true;
        ws.config.usage.claude_limits = true;
        for program in ["powershell", "CMD.EXE"] {
            ws.config.shell.program = Some(program.into());
            assert_eq!(word(ws, "TEMPLUM_CODEX_ARGS"), "", "{program}");
            assert_eq!(word(ws, "TEMPLUM_CLAUDE_ARGS"), claude, "{program}");
        }
        ws.config.shell.program = Some("WSL.exe".into());
        assert_eq!(word(ws, "TEMPLUM_CLAUDE_ARGS"), "");
        assert_eq!(word(ws, "TEMPLUM_CODEX_ARGS"), "");
    });
    // Without a data folder: both, empty, so none comes from a Templum this one was started from.
    let (_, bare) = open_in(cx, None, None);
    bare.update(cx, |ws, _| {
        assert_eq!(
            ws.agent_env(),
            [
                ("TEMPLUM_CLAUDE_ARGS".to_string(), String::new()),
                ("TEMPLUM_CODEX_ARGS".to_string(), String::new()),
            ]
        );
    });
}

/// A program path a shell would expand (`$`) can't be typed into the hooks: the server isn't
/// given either, or the agent would be connected with a dot that never moves.
#[test]
fn claude_code_gets_the_server_only_with_the_hooks() {
    use crate::workspace::usage::claude_flags;
    let data = tempfile::tempdir().unwrap();
    let flags = claude_flags(
        data.path(),
        std::path::Path::new("/opt/templum"),
        cfg!(windows),
        false,
        true,
    );
    assert!(
        flags.contains(" --settings ") && flags.contains(" --mcp-config "),
        "{flags}"
    );
    assert_eq!(
        claude_flags(
            data.path(),
            std::path::Path::new("/opt/$x/templum"),
            cfg!(windows),
            false,
            true
        ),
        ""
    );
}

/// A data folder a shell would expand (`$`): the agent starts as typed, without a broken line.
#[cfg(unix)]
#[gpui_kit::test]
fn an_untypeable_data_folder_gives_no_connections(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    let data = parent.path().join("a$b");
    std::fs::create_dir(&data).unwrap();
    let (_, ws) = open_in(cx, None, Some(data));
    ws.update(cx, |ws, _| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").cloned();
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
    });
}

/// A limit nearing its end, read while Templum is in the background: one system notification,
/// not one per read.
#[cfg(unix)]
#[gpui_kit::test]
fn a_limit_near_its_end_is_told_once(cx: &mut TestAppContext) {
    use templum_usage::{Limits, Window};
    let (handle, ws) = open(cx);
    cx.update(|cx| cx.set_app_identity("test.Templum", "Templum"));
    super::terminals::set_active(handle, false, cx);
    let now = crate::notes::now();
    let limits = vec![(
        "claude".to_string(),
        Limits {
            windows: vec![Window {
                minutes: 300,
                used_percent: 92.0,
                resets_at: now + 2 * 3600,
                scope: None,
            }],
            at: now,
        },
    )];
    for _ in 0..2 {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.usage_done(Vec::new(), (Vec::new(), limits.clone()), window, cx)
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    let shown = cx.shown_system_notifications();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(
        shown[0].title.as_ref(),
        "Claude Code: 92% of the 5-hour limit used"
    );
    assert!(shown[0].body.starts_with("Resets "), "{:?}", shown[0].body);
    assert_eq!(ws.read_with(cx, |ws, _| ws.limits.clone()), limits);
}

/// The title bar's meter shows only with a limit known; it opens the usage, whose sessions show
/// their terminals. A terminal with usage has its chip.
#[gpui_kit::test]
fn the_meter_opens_the_usage_and_a_session_shows_its_terminal(cx: &mut TestAppContext) {
    use templum_usage::{Limits, Session, Window};
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("usage-meter").is_none(), "no limit known");
        window.press(NEW, cx);
    })
    .unwrap();
    let now = crate::notes::now();
    ws.update(cx, |ws, _| {
        ws.limits = vec![(
            "claude".into(),
            Limits {
                windows: vec![Window {
                    minutes: 300,
                    used_percent: 73.0,
                    resets_at: now + 3 * 3600,
                    scope: None,
                }],
                at: now,
            },
        )];
        let pane = ws.space.terminal_mut(first).unwrap();
        pane.seen.agent = Some("claude".into());
        pane.usage = Some(Session {
            context_percent: Some(42.0),
            cost_usd: Some(3.1),
            ..Session::default()
        });
        assert_ne!(ws.space.focused, Some(first));
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("usage-chip", first.0 as usize)).is_some());
        window.click("usage-meter", cx);
        window.render_frame(cx);
        assert!(window.try_find("usage-popover").is_some());
        window.click(("usage-session", 0_usize), cx);
        window.render_frame(cx);
        assert!(window.try_find("usage-popover").is_none());
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, Some(first)));
}

fn limits_of(agent: &str, used: f32) -> (String, templum_usage::Limits) {
    let now = crate::notes::now();
    (
        agent.to_string(),
        templum_usage::Limits {
            windows: vec![templum_usage::Window {
                minutes: 300,
                used_percent: used,
                resets_at: now + 3 * 3600,
                scope: None,
            }],
            at: now,
        },
    )
}

/// With several agents' limits known, the meter shows them in turn; one hidden in the settings
/// never shows there.
#[gpui_kit::test]
fn the_meter_takes_the_agents_in_turn_but_not_a_hidden_one(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    ws.update(cx, |ws, _| {
        ws.limits = vec![limits_of("claude", 27.0), limits_of("codex", 91.0)];
    });
    let shown = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            (
                window.try_find("usage-meter-claude").is_some(),
                window.try_find("usage-meter-codex").is_some(),
            )
        })
        .unwrap()
    };
    assert_eq!(shown(cx), (true, false), "Claude first");
    // A read after TURN_SECS turns it, unless the pointer is over the meter.
    let read = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.meter_turned = 0;
                let limits = ws.limits.clone();
                ws.usage_done(Vec::new(), (Vec::new(), limits), window, cx)
            })
        })
        .unwrap()
    };
    read(&ws, cx);
    assert_eq!(shown(cx), (false, true), "then Codex");
    ws.update(cx, |ws, _| ws.meter_hovered = true);
    read(&ws, cx);
    assert_eq!(shown(cx), (false, true), "held while hovered");
    ws.update(cx, |ws, _| {
        ws.meter_hovered = false;
        ws.config.usage.hide = vec!["codex".into()];
    });
    for _ in 0..3 {
        read(&ws, cx);
        assert_eq!(shown(cx), (true, false), "Codex hidden");
    }
}

/// Settings ▸ Usage hides an agent's limits from the title bar, and ↺ shows them again.
#[gpui_kit::test]
fn an_agent_is_taken_off_the_title_bar_and_back(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-usage", cx);
        window.render_frame(cx);
        // The section's last row, below the fold of a small window.
        window.scroll(
            "settings-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-5000.))),
            cx,
        );
        window.render_frame(cx);
        window.click("field-usage.hide.codex", cx);
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(text.contains(r#"hide = ["codex"]"#), "{text}");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.config.usage.hide.clone()),
        ["codex"]
    );
    cx.update_window(handle, |_, window, cx| {
        window.click("reset-usage.hide.codex", cx);
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(!text.contains("hide"), "{text}");
}

/// Templum asks Claude Code for the plan's limits only with a data folder, when the user
/// wants them, and runs the program Claude Code's command names.
#[gpui_kit::test]
fn claude_code_is_asked_for_its_limits_when_wanted(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let program = data.path().join("my-claude");
    std::fs::write(&program, "").unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.update(cx, |ws, _| {
        let claude = ws.agents.iter_mut().find(|a| a.id == "claude").unwrap();
        claude.command = format!("{} --model opus", program.display());
        assert_eq!(
            ws.claude_poll_job(),
            Some((program.clone(), data.path().join("usage").join("probe")))
        );
        ws.config.usage.claude_limits = false;
        assert_eq!(ws.claude_poll_job(), None);
        ws.config.usage.claude_limits = true;
        ws.data_dir = None;
        assert_eq!(ws.claude_poll_job(), None);
    });
}

#[cfg(unix)]
#[test]
fn gemini_cli_links_the_extension_or_says_why_not() {
    use crate::workspace::usage::gemini_run;
    let data = tempfile::tempdir().unwrap();
    // `true` and `false` stand in for gemini: whatever they are given, they succeed or fail.
    assert_eq!(gemini_run(Some("true".into()), data.path(), true), Ok(()));
    assert!(
        data.path()
            .join("gemini")
            .join("gemini-extension.json")
            .exists(),
        "the extension is written first"
    );
    let why = gemini_run(Some("false".into()), data.path(), false).unwrap_err();
    assert!(why.starts_with("Gemini CLI failed"), "{why}");
    let why = gemini_run(None, data.path(), true).unwrap_err();
    assert!(why.contains("didn't find Gemini CLI"), "{why}");
}

/// A Gemini CLI opened without Templum's extension says so once per run, never on a restore
/// (docs/specs/connections.md, Gemini CLI without its extension).
#[gpui_kit::test]
fn a_gemini_cli_without_the_extension_is_told_once(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let installed = std::env::home_dir()
        .is_some_and(|home| templum_usage::gemini::installed(&home.join(".gemini")));
    let open = |id: &str, cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let entry = ws.agents.iter().find(|a| a.id == id).cloned().unwrap();
                let rect = Rect::new(0.0, 0.0, 400.0, 300.0);
                ws.start_agent(
                    Some(&entry),
                    &entry.command,
                    "",
                    folder.path(),
                    rect,
                    window,
                    cx,
                )
                .unwrap();
            })
        })
        .unwrap();
    };
    let before = notification_count(handle, cx);
    open("claude", cx);
    assert_eq!(notification_count(handle, cx), before, "Claude Code");
    // A restored Gemini CLI says nothing.
    let item: crate::session::Item = serde_json::from_value(serde_json::json!({
        "kind": "terminal",
        "rect": { "x": 0.0, "y": 400.0, "w": 400.0, "h": 300.0 },
        "folder": folder.path(),
        "agent": "gemini",
    }))
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.add_saved(&item, window, cx).unwrap());
    })
    .unwrap();
    assert_eq!(notification_count(handle, cx), before, "restored");
    // Connections off: nothing to install it for.
    ws.update(cx, |ws, _| ws.config.connections.enabled = false);
    open("gemini", cx);
    assert_eq!(notification_count(handle, cx), before, "off");
    ws.update(cx, |ws, _| ws.config.connections.enabled = true);
    open("gemini", cx);
    let told = usize::from(!installed);
    assert_eq!(notification_count(handle, cx), before + told, "once");
    open("gemini", cx);
    assert_eq!(notification_count(handle, cx), before + told, "not twice");
}
