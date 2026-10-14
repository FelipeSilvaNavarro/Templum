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
