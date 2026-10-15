//! Agents' session usage and plan limits (docs/specs/usage.md).

use super::*;

/// The Claude Code Templum types gets Templum's status line, which hands Templum the limits.
#[gpui_kit::test]
fn the_claude_code_templum_opens_gets_its_status_line(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.update(cx, |ws, _| {
        let agent = |id: &str| ws.agents.iter().find(|a| a.id == id).cloned();
        let (claude, codex) = (agent("claude"), agent("codex"));
        let settings = data.path().join("claude-statusline.json");
        let arg =
            templum_usage::claude::typed_path(&settings.to_string_lossy(), cfg!(windows)).unwrap();
        assert_eq!(
            ws.typed(claude.as_ref(), "claude --continue"),
            format!("claude --continue --settings {arg}")
        );
        let written = std::fs::read_to_string(&settings).unwrap();
        assert!(written.contains("--claude-statusline"), "{written}");
        assert_eq!(ws.typed(codex.as_ref(), "codex"), "codex");
        assert_eq!(ws.typed(None, "claude"), "claude");
        ws.config.usage.claude_limits = false;
        assert_eq!(ws.typed(claude.as_ref(), "claude"), "claude");
        // Without a data folder, nothing would read what the relay writes.
        ws.config.usage.claude_limits = true;
        ws.data_dir = None;
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
