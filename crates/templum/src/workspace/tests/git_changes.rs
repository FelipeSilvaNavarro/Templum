//! The Git block's changes: staging, discarding, diffs and conflicts.

use super::*;

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
fn a_row_shows_its_diff_and_open_in_editor_opens_the_file_beside_the_block(
    cx: &mut TestAppContext,
) {
    use crate::git_block::row_id;
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(row_id(id, "unstaged", 0), cx)
    })
    .unwrap();
    settle(handle, cx);
    let diff = git_of(&ws, cx, id, |g| {
        assert_eq!(g.selected.as_ref().map(|s| s.1.as_str()), Some("a.txt"));
        assert_eq!(g.diff_line, Some(1));
        g.diff.clone()
    });
    let text = ws.read_with(cx, |_, cx| diff.read(cx).value().to_string());
    assert!(text.contains("+zero"), "{text}");
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find(("git-diff", n)).is_some());
        window.click(("git-open-diff", n), cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, cx| {
        let (eid, e) = ws
            .space
            .blocks
            .iter()
            .find_map(|(id, b)| Some((*id, b.editor()?)))
            .expect("an editor opened");
        assert_eq!(e.active_tab().unwrap().name(), "a.txt");
        assert_eq!(
            e.active_tab()
                .unwrap()
                .editor
                .read(cx)
                .cursor_position()
                .line,
            0
        );
        let (git, editor) = (
            ws.space.canvas.get(id).unwrap().rect,
            ws.space.canvas.get(eid).unwrap().rect,
        );
        assert_eq!(editor.x, git.x + git.w + templum_canvas::BESIDE);
        assert_eq!(editor.y, git.y);
    });
}

#[gpui_kit::test]
fn an_untracked_file_shows_its_content_as_added_and_a_binary_one_says_so(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    std::fs::write(repo.path().join("b.bin"), b"\0\x01").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let shown = |cx: &mut TestAppContext, i: usize| {
        cx.update_window(handle, |_, window, cx| {
            window.click(row_id(id, "unstaged", i), cx)
        })
        .unwrap();
        settle(handle, cx);
        let diff = git_of(&ws, cx, id, |g| g.diff.clone());
        ws.read_with(cx, |_, cx| diff.read(cx).value().to_string())
    };
    assert_eq!(shown(cx, 0), "+++ b/a.txt\n@@ -0,0 +1,1 @@\n+one\n");
    assert_eq!(shown(cx, 1), "Binary file");
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

#[gpui_kit::test]
fn the_changes_diff_isnt_reset_by_a_read_that_changes_nothing(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    use gpui_kit::component::input::Position;
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        window.click(row_id(id, "unstaged", 0), cx)
    })
    .unwrap();
    settle(handle, cx);
    let diff = git_of(&ws, cx, id, |g| g.diff.clone());
    cx.update_window(handle, |_, window, cx| {
        diff.update(cx, |d, cx| {
            d.set_cursor_position(Position::new(2, 0), window, cx)
        })
    })
    .unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    ws.read_with(cx, |_, cx| {
        assert_eq!(
            diff.read(cx).cursor_position().line,
            2,
            "where the reader left it"
        )
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
