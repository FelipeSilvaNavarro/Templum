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
