//! The Git block's keys while it has the keyboard.

use gpui_kit::Focusable as _;

use super::*;

#[cfg(target_os = "macos")]
const COMMIT: &str = "cmd-enter";
#[cfg(not(target_os = "macos"))]
const COMMIT: &str = "ctrl-enter";

/// The block takes the keyboard, as a click on it does.
fn focus(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
}

fn press(handle: AnyWindowHandle, cx: &mut TestAppContext, key: &str) {
    cx.update_window(handle, |_, window, cx| window.press(key, cx))
        .unwrap();
    settle(handle, cx);
}

fn cursor(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
) -> Option<(bool, String, usize)> {
    git_of(ws, cx, id, |g| g.panel.cursor.clone())
}

fn staged(repo: &tempfile::TempDir) -> String {
    git_run(repo.path(), &["diff", "--cached", "--numstat"])
}

#[gpui_kit::test]
fn brackets_move_the_hunk_cursor_and_s_stages_that_hunk(cx: &mut TestAppContext) {
    let repo = three_hunk_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    focus(handle, &ws, cx, id);
    press(handle, cx, "]");
    assert_eq!(cursor(&ws, cx, id), Some((false, "a.txt".to_string(), 0)));
    press(handle, cx, "]");
    assert_eq!(cursor(&ws, cx, id), Some((false, "a.txt".to_string(), 1)));
    press(handle, cx, "[");
    press(handle, cx, "]");
    press(handle, cx, "s");
    assert_eq!(staged(&repo), "1\t1\ta.txt\n", "the middle hunk alone");
    git_of(&ws, cx, id, |g| {
        let row = g.panel.cursor_row().expect("still on a.txt's changes");
        assert!(matches!(
            g.panel.rows[row],
            crate::git_block::diff_view::Row::Hunk { hunk: 1, .. }
        ));
    });
}

#[gpui_kit::test]
fn u_unstages_the_cursors_hunk_and_d_asks_before_discarding(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = three_hunk_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    focus(handle, &ws, cx, id);
    press(handle, cx, "]");
    press(handle, cx, "s");
    assert_eq!(staged(&repo), "1\t1\ta.txt\n");
    // The cursor stayed on Changes' hunk 0 (the former hunk 1); then Changes' hunk 1, then
    // Staged's one: two steps down to it.
    press(handle, cx, "]");
    press(handle, cx, "]");
    assert_eq!(cursor(&ws, cx, id), Some((true, "a.txt".to_string(), 0)));
    press(handle, cx, "s");
    assert_eq!(
        staged(&repo),
        "1\t1\ta.txt\n",
        "s on a staged hunk does nothing"
    );
    press(handle, cx, "u");
    assert_eq!(staged(&repo), "");
    press(handle, cx, "[");
    press(handle, cx, "d");
    cx.update_window(handle, |_, window, cx| {
        assert!(window.has_active_dialog(cx), "asks first");
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn arrows_walk_the_files_and_enter_opens_at_the_hunk(cx: &mut TestAppContext) {
    use crate::git_block::List;
    let repo = three_hunk_repo();
    std::fs::write(repo.path().join("b.txt"), "x\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let selected = |cx: &mut TestAppContext| git_of(&ws, cx, id, |g| g.selected.clone());
    focus(handle, &ws, cx, id);
    press(handle, cx, "down");
    assert_eq!(selected(cx), Some((List::Unstaged, "a.txt".to_string())));
    press(handle, cx, "down");
    assert_eq!(selected(cx), Some((List::Unstaged, "b.txt".to_string())));
    press(handle, cx, "up");
    assert_eq!(selected(cx), Some((List::Unstaged, "a.txt".to_string())));
    press(handle, cx, "]");
    press(handle, cx, "]");
    press(handle, cx, "enter");
    ws.read_with(cx, |ws, cx| {
        let e = ws
            .space
            .blocks
            .values()
            .find_map(|b| b.editor())
            .expect("an editor opened");
        assert_eq!(e.active_tab().unwrap().name(), "a.txt");
        assert_eq!(
            e.active_tab()
                .unwrap()
                .editor
                .read(cx)
                .cursor_position()
                .line,
            11,
            "the second hunk starts at line 12"
        );
    });
}

#[gpui_kit::test]
fn c_goes_to_the_summary_and_the_commit_key_commits(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let (summary, block) = git_of(&ws, cx, id, |g| (g.summary.clone(), g.focus.clone()));
    focus(handle, &ws, cx, id);
    press(handle, cx, "c");
    cx.update_window(handle, |_, window, cx| {
        assert!(summary.read(cx).focus_handle(cx).is_focused(window));
        summary.update(cx, |s, cx| s.set_value("By keyboard", window, cx));
        // Back to the block itself: the key acts there, not in the field.
        block.focus(window, cx);
        window.render_frame(cx);
    })
    .unwrap();
    press(handle, cx, COMMIT);
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%s"]),
        "By keyboard\n"
    );
}

#[gpui_kit::test]
fn a_field_with_the_keyboard_keeps_the_letters(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    let repo = three_hunk_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    focus(handle, &ws, cx, id);
    press(handle, cx, "]");
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.focus(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    press(handle, cx, "s");
    press(handle, cx, "d");
    assert_eq!(staged(&repo), "", "s typed, nothing staged");
    cx.update_window(handle, |_, window, cx| {
        assert!(!window.has_active_dialog(cx), "d typed, nothing discarded");
        // A click into the block (it focuses the block) keeps the field's keyboard.
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        assert!(summary.read(cx).focus_handle(cx).is_focused(window));
    })
    .unwrap();
}
