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
