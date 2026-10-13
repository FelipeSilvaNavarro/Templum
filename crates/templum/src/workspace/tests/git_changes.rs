//! The Git block's changes: staging, discarding, diffs and conflicts.

use super::*;
use gpui_kit::Focusable;

#[gpui_kit::test]
fn the_plus_button_stages_and_minus_unstages(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    click_row_button(
        handle,
        cx,
        row_id(id, "unstaged", 0),
        git_button(id, "stage", "unstaged", 0),
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.staged().len(), 1);
        assert_eq!(
            g.status.staged()[0].staged,
            Some(templum_git::Change::Added)
        );
        assert!(g.status.unstaged().is_empty());
    });
    click_row_button(
        handle,
        cx,
        row_id(id, "staged", 0),
        git_button(id, "unstage", "staged", 0),
    );
    git_of(&ws, cx, id, |g| {
        assert!(g.status.staged().is_empty());
        assert_eq!(g.status.unstaged().len(), 1);
    });
}

#[gpui_kit::test]
fn stage_all_and_unstage_all(cx: &mut TestAppContext) {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "a\n").unwrap();
    std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-stage-all", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.status.staged().len(), 2));
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-unstage-all", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.status.unstaged().len(), 2));
}

#[gpui_kit::test]
fn discard_asks_first_then_restores_the_file(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        window.hover(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.click(git_button(id, "discard", "unstaged", 0), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "asks first");
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "changed\n"
    );
    cx.update_window(handle, |_, window, cx| {
        window.hover(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.click(git_button(id, "discard", "unstaged", 0), cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "one\n"
    );
    git_of(&ws, cx, id, |g| assert!(g.status.entries.is_empty()));
}

#[gpui_kit::test]
fn an_untracked_file_shows_its_content_as_added_and_a_binary_one_says_so(cx: &mut TestAppContext) {
    use crate::git_block::diff_view::{Note, Row};
    use templum_git::Kind;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    std::fs::write(repo.path().join("b.bin"), b"\0\x01").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| {
        let p = &g.panel;
        assert_eq!(p.files[0].diff.path(), "a.txt");
        assert_eq!(p.files[1].diff.path(), "b.bin");
        let a = &p.files[0].diff.hunks[0].lines;
        assert_eq!(a.len(), 1);
        assert_eq!(
            (a[0].kind, a[0].text.as_str(), a[0].new_no),
            (Kind::Added, "one", Some(1))
        );
        assert!(p.files[1].diff.binary);
        assert!(p.rows.contains(&Row::Note {
            file: 1,
            note: Note::Binary
        }));
    });
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("git-diff", n)).is_some());
    })
    .unwrap();
}

/// `main` and `topic` both changed a.txt; `git merge topic` is in progress with a conflict.
fn conflicted_repo() -> tempfile::TempDir {
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "topic"]);
    std::fs::write(repo.path().join("a.txt"), "topic\n").unwrap();
    git_run(repo.path(), &["commit", "-q", "-am", "topic"]);
    git_run(repo.path(), &["checkout", "-q", "main"]);
    std::fs::write(repo.path().join("a.txt"), "main\n").unwrap();
    git_run(repo.path(), &["commit", "-q", "-am", "main"]);
    let merge = std::process::Command::new("git")
        .args(["merge", "topic"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(!merge.status.success(), "conflicts");
    repo
}

#[gpui_kit::test]
fn a_conflict_takes_a_side_is_marked_resolved_and_the_merge_continues(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = conflicted_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.operation, Some(templum_git::Operation::Merge));
        assert_eq!(g.status.conflicts().len(), 1);
    });
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find(("git-abort", n)).is_some(), "the banner");
        assert!(
            window.try_find(("git-continue", n)).is_none(),
            "not while conflicted"
        );
    })
    .unwrap();
    click_row_button(
        handle,
        cx,
        row_id(id, "conflicts", 0),
        git_button(id, "ours", "conflicts", 0),
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "main\n"
    );
    click_row_button(
        handle,
        cx,
        row_id(id, "conflicts", 0),
        git_button(id, "resolved", "conflicts", 0),
    );
    git_of(&ws, cx, id, |g| {
        assert!(g.status.conflicts().is_empty());
        assert!(
            g.status.staged().is_empty(),
            "ours is HEAD's content: nothing left to stage"
        );
        assert_eq!(g.operation, Some(templum_git::Operation::Merge));
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-continue", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.operation, None);
        assert!(g.status.entries.is_empty());
    });
    assert_eq!(
        git_run(repo.path(), &["rev-list", "--count", "HEAD"]),
        "4\n"
    );
}

#[gpui_kit::test]
fn abort_ends_the_merge(cx: &mut TestAppContext) {
    let repo = conflicted_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-abort", id.0 as usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.operation, None);
        assert!(g.status.entries.is_empty());
    });
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "main\n"
    );
}

#[gpui_kit::test]
fn a_rows_menu_copies_the_path_and_ignores_the_file(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::create_dir(repo.path().join("build")).unwrap();
    std::fs::write(repo.path().join("build/out.o"), "x").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    // Stage, Discard…, Open, Copy path, Ignore.
    cx.update_window(handle, |_, window, cx| {
        window.right_click(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(3usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .unwrap_or_default();
    assert_eq!(text, "build/out.o");
    cx.update_window(handle, |_, window, cx| {
        window.right_click(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(4usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".gitignore")).unwrap(),
        "/build/out.o\n"
    );
    git_of(&ws, cx, id, |g| {
        let paths: Vec<_> = g.status.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, [".gitignore"], "out.o is ignored now");
    });
}

// Final review I5: a read isn't an action: it hides no button and takes no action's turn.
#[gpui_kit::test]
fn the_rows_buttons_stay_while_a_read_runs(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.refresh_git(id, window, cx));
        window.render_frame(cx);
        window.hover(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        assert!(
            window
                .try_find(git_button(id, "stage", "unstaged", 0))
                .is_some()
        );
    })
    .unwrap();
    git_of(&ws, cx, id, |g| assert!(g.busy, "a read was out"));
}

/// Hundreds of changed files cost a frame only the rows on screen: the list is virtual.
#[gpui_kit::test]
fn a_long_changes_list_builds_only_the_rows_on_screen(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    for i in 0..300 {
        std::fs::write(repo.path().join(format!("f{i:03}.txt")), "x\n").unwrap();
    }
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| assert_eq!(g.status.unstaged().len(), 300));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(row_id(id, "unstaged", 0)).is_some());
        assert!(window.try_find(row_id(id, "unstaged", 299)).is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn opening_a_file_from_a_git_row_gives_its_editor_the_keyboard(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let editor_has_keyboard = |cx: &mut TestAppContext, how: &str| {
        cx.update_window(handle, |_, window, cx| {
            let ws = ws.read(cx);
            let (eid, e) = ws
                .space
                .blocks
                .iter()
                .find_map(|(id, b)| Some((*id, b.editor()?)))
                .expect("an editor opened");
            assert_eq!(ws.space.focused, Some(eid), "{how}: the editor is focused");
            assert!(
                e.active_tab()
                    .unwrap()
                    .editor
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window),
                "{how}: the file has the keyboard"
            );
        })
        .unwrap();
    };
    // The row's Open button.
    click_row_button(
        handle,
        cx,
        row_id(id, "unstaged", 0),
        git_button(id, "open", "unstaged", 0),
    );
    editor_has_keyboard(cx, "row button");
    // The row's menu: Stage, Discard changes…, Open.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            // Back to where the Git block shows: the editor's reveal moved the camera.
            ws.space.canvas.camera = Camera::default();
            ws.space.flight = None;
            ws.focus_block(id, window, cx);
        });
        window.render_frame(cx);
        window.right_click(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(2usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    editor_has_keyboard(cx, "row menu");
}
