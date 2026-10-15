//! The Git block: history, branches and stashes.

use super::*;

fn commit_file(repo: &std::path::Path, name: &str, text: &str, message: &str) {
    std::fs::write(repo.join(name), text).unwrap();
    git_run(repo, &["add", name]);
    git_run(repo, &["commit", "-q", "-m", message]);
}

/// Opens a Git block on `repo` showing `tab`, loaded.
fn open_git_on(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    repo: &std::path::Path,
    tab: crate::git_block::Tab,
) -> BlockId {
    let id = open_git(handle, ws, cx, repo);
    cx.update_window(handle, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("git-tab-{}-{}", id.0, tab.name()).into()),
            cx,
        )
    })
    .unwrap();
    settle(handle, cx);
    id
}

#[gpui_kit::test]
fn history_lists_the_commits_newest_first_and_shows_one_with_its_files(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second");
    commit_file(repo.path(), "a.txt", "one\ntwo\n", "Third");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        let subjects: Vec<_> = g.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["Third", "Second", "First"]);
        assert!(g.commits[0].refs.contains(&"HEAD -> main".to_string()));
        assert!(!g.more);
    });
    cx.update_window(handle, |_, window, cx| {
        assert!(
            window.try_find(("git-commit", 2usize)).is_some(),
            "three rows"
        );
        window.click(("git-commit", 0usize), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        let shown = g.shown.as_ref().expect("shown");
        assert_eq!(shown.commit.subject, "Third");
        assert_eq!(shown.files.len(), 1);
        let p = g.diff();
        assert_eq!(p.title(), format!("Commit {} · 1 file", shown.commit.short));
        assert!(
            p.files[0].diff.hunks[0]
                .lines
                .iter()
                .any(|l| l.text == "two"),
            "its whole diff"
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("git-commit-file-{n}-0").into()),
            cx,
        );
        window.render_frame(cx);
        window.click(("git-commit-back", n), cx);
        window.render_frame(cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(g.shown.is_none(), "back to the list");
        assert!(g.history_panel.files.is_empty());
    });
}

/// Types `text` in History's search and presses Enter (an empty `text` clears the field).
fn search(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    text: &str,
) {
    let field = git_of(ws, cx, id, |g| g.search.clone());
    cx.update_window(handle, |_, window, cx| {
        field.update(cx, |s, cx| s.focus(window, cx));
        window.press("ctrl-a", cx);
        window.press("cmd-a", cx);
        window.press("backspace", cx);
        if !text.is_empty() {
            window.input(text, cx);
            window.press("enter", cx);
        }
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
}

fn subjects(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> Vec<String> {
    git_of(ws, cx, id, |g| {
        g.commits.iter().map(|c| c.subject.clone()).collect()
    })
}

#[gpui_kit::test]
fn search_modes_find_by_message_author_and_code(cx: &mut TestAppContext) {
    let repo = committed_repo();
    git_run(repo.path(), &["config", "user.name", "Grace"]);
    commit_file(
        repo.path(),
        "p.rs",
        "fn parse_line() {}\n",
        "Read the input",
    );
    git_run(repo.path(), &["config", "user.name", "Ada"]);
    commit_file(repo.path(), "q.rs", "fn other() {}\n", "Fix the parser");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    search(handle, &ws, cx, id, "PARSER");
    assert_eq!(
        subjects(&ws, cx, id),
        ["Fix the parser"],
        "messages, without case"
    );
    git_of(&ws, cx, id, |g| {
        assert!(g.graph.is_empty(), "no graph over a search")
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-search-author", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    search(handle, &ws, cx, id, "grace");
    assert_eq!(subjects(&ws, cx, id), ["Read the input"]);
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-search-code", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    search(handle, &ws, cx, id, "parse_line");
    assert_eq!(
        subjects(&ws, cx, id),
        ["Read the input"],
        "the commit that added it"
    );
}

#[gpui_kit::test]
fn search_takes_text_as_written_and_clearing_it_brings_all_back(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "fix(parser): read [wip] files");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    search(handle, &ws, cx, id, "fix(");
    assert_eq!(subjects(&ws, cx, id), ["fix(parser): read [wip] files"]);
    search(handle, &ws, cx, id, "[nothing");
    git_of(&ws, cx, id, |g| {
        assert!(g.commits.is_empty());
        assert!(g.error.is_none(), "git didn't fail: {:?}", g.error);
    });
    search(handle, &ws, cx, id, "");
    assert_eq!(subjects(&ws, cx, id).len(), 2, "every commit again");
}

#[gpui_kit::test]
fn load_more_continues_the_lanes(cx: &mut TestAppContext) {
    use crate::git_block::PAGE;
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "side"]);
    let old = std::process::Command::new("git")
        .args(["commit", "-q", "--allow-empty", "-m", "side"])
        .current_dir(repo.path())
        .env("GIT_COMMITTER_DATE", "1000000000 +0000")
        .env("GIT_AUTHOR_DATE", "1000000000 +0000")
        .status()
        .unwrap();
    assert!(old.success());
    git_run(repo.path(), &["checkout", "-q", "main"]);
    for i in 0..PAGE {
        git_run(
            repo.path(),
            &["commit", "-q", "--allow-empty", "-m", &format!("c{i}")],
        );
    }
    git_run(
        repo.path(),
        &["merge", "-q", "--no-ff", "--no-edit", "side"],
    );
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    let first = git_of(&ws, cx, id, |g| {
        assert_eq!(g.commits.len(), PAGE);
        assert_eq!(g.graph.len(), PAGE);
        assert_eq!(
            g.graph[0].down,
            [(0, 0), (0, 1)],
            "the merge opens side's lane"
        );
        assert!(
            g.graph[PAGE - 1].up.contains(&(1, 1)),
            "still open at the page's end"
        );
        (*g.graph).clone()
    });
    cx.update_window(handle, |_, window, cx| {
        window.scroll(
            ("git-commit", 0usize),
            ScrollDelta::Pixels(point(px(0.0), px(-100_000.0))),
            cx,
        );
        window.render_frame(cx);
        window.render_frame(cx);
        window.click(("git-load-more", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.commits.len(), PAGE + 3, "c0, side, First");
        assert_eq!(
            g.graph[..PAGE],
            first[..],
            "the first page's lanes don't move"
        );
        assert!(
            g.graph[PAGE].up.contains(&(1, 1)),
            "side's lane runs on into the second page: {:?}",
            g.graph[PAGE]
        );
        let side = g.commits.iter().position(|c| c.subject == "side").unwrap();
        assert_eq!(
            g.graph[side].lane, 1,
            "side's commit is in the lane opened for it"
        );
    });
}

#[gpui_kit::test]
fn all_branches_and_the_commits_not_pushed(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["branch", "other"]);
    git_run(repo.path(), &["checkout", "-q", "other"]);
    commit_file(repo.path(), "o.txt", "o\n", "On other");
    git_run(repo.path(), &["checkout", "-q", "main"]);
    commit_file(repo.path(), "b.txt", "b\n", "Not pushed");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        let subjects: Vec<_> = g.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["Not pushed", "First"]);
        assert!(g.unpushed.contains(&g.commits[0].hash));
        assert!(!g.unpushed.contains(&g.commits[1].hash), "on origin/main");
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-all-branches", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.all_branches);
        assert_eq!(g.commits.len(), 3, "other's commit too");
        assert_eq!(g.graph.len(), 3);
    });
}

#[gpui_kit::test]
fn a_commits_menu_branches_from_it_tags_it_and_copies_its_hash(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let first = git_of(&ws, cx, id, |g| g.commits[1].hash.clone());
    // Checkout…, New branch here…, Create tag…, Revert…, [Undo this commit: HEAD, not pushed],
    // Copy hash, Copy message.
    let menu = |cx: &mut TestAppContext, row: usize, item: usize| {
        cx.update_window(handle, |_, window, cx| {
            window.right_click(("git-commit", row), cx);
            window.render_frame(cx);
            window.within("popup-menu").click(item, cx);
            window.render_frame(cx);
        })
        .unwrap();
    };
    menu(cx, 1, 1);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
        window.input("feat", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"));
        assert_eq!(g.status.oid.as_deref(), Some(first.as_str()));
        assert!(
            g.commits[0].refs.iter().any(|r| r.contains("feat")),
            "{:?}",
            g.commits[0].refs
        );
    });
    menu(cx, 0, 2);
    cx.update_window(handle, |_, window, cx| {
        window.input("v1", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(
            g.commits[0].refs.contains(&"tag: v1".to_string()),
            "{:?}",
            g.commits[0].refs
        );
    });
    // Row 0 is `first` now (feat points at it), HEAD with no upstream: Undo this commit is 4.
    menu(cx, 0, 5);
    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .unwrap_or_default();
    assert_eq!(text, first);
}

#[gpui_kit::test]
fn undo_this_commit_from_history_puts_it_back(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second\n\nWhy.");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("git-commit", 0usize), cx);
        window.render_frame(cx);
        // Checkout…, New branch here…, Create tag…, Revert…, Undo this commit.
        window.within("popup-menu").click(4usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(repo.path(), &["rev-list", "--count", "HEAD"]),
        "1\n"
    );
    let summary = git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.staged().len(), 1, "its changes stay staged");
        g.summary.clone()
    });
    ws.read_with(cx, |_, cx| assert_eq!(summary.read(cx).value(), "Second"));
}

#[gpui_kit::test]
fn copy_message_copies_the_subject_and_the_body(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    commit_file(repo.path(), "b.txt", "b\n", "Second\n\nWhy it is.");
    git_run(repo.path(), &["push", "-q"]);
    let (handle, ws) = open(cx);
    let _id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("git-commit", 0usize), cx);
        window.render_frame(cx);
        // Pushed: no Undo. Checkout…, New branch here…, Create tag…, Revert…, Copy hash, Copy message.
        window.within("popup-menu").click(5usize, cx);
    })
    .unwrap();
    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .unwrap_or_default();
    assert_eq!(text, "Second\n\nWhy it is.");
}

/// Reads asked while another runs wait for it (spec §8: one at a time per block, the second
/// waits); none is dropped.
#[gpui_kit::test]
fn a_commit_clicked_while_a_read_runs_is_still_shown(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let hash = git_of(&ws, cx, id, |g| g.commits[0].hash.clone());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(id, window, cx);
            ws.show_commit(id, hash.clone(), window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        let shown = g
            .shown
            .as_ref()
            .expect("the commit waited for the read, then showed");
        assert_eq!(shown.commit.subject, "First");
        assert_eq!(shown.files.len(), 1);
    });
}

fn branch_row(id: BlockId, remote: bool, i: usize) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(
        format!(
            "git-branch-{}-{}-{i}",
            id.0,
            if remote { "remote" } else { "local" }
        )
        .into(),
    )
}

#[gpui_kit::test]
fn branches_are_listed_with_the_current_one_and_a_click_checks_out(cx: &mut TestAppContext) {
    let repo = committed_repo();
    git_run(repo.path(), &["branch", "feat"]);
    let (handle, ws) = open(cx);
    let id = open_git_on(
        handle,
        &ws,
        cx,
        repo.path(),
        crate::git_block::Tab::Branches,
    );
    git_of(&ws, cx, id, |g| {
        let names: Vec<_> = g
            .branches
            .iter()
            .map(|b| (b.name.as_str(), b.current))
            .collect();
        assert_eq!(names, [("feat", false), ("main", true)]);
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(branch_row(id, false, 0), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"));
        assert!(g.branches[0].current, "the list followed");
    });
}

#[gpui_kit::test]
fn new_branch_creates_and_checks_out_and_a_bad_name_is_refused(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git_on(
        handle,
        &ws,
        cx,
        repo.path(),
        crate::git_block::Tab::Branches,
    );
    let n = id.0 as usize;
    let create = |cx: &mut TestAppContext, name: &str| {
        cx.update_window(handle, |_, window, cx| {
            window.click(("git-new-branch", n), cx);
            window.render_frame(cx);
            assert!(window.has_active_dialog(cx));
            window.input(name, cx);
            window.press("enter", cx);
            window.render_frame(cx);
        })
        .unwrap();
        settle(handle, cx);
    };
    create(cx, "feature/one");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feature/one"));
        assert_eq!(g.branches.len(), 2);
    });
    create(cx, "bad name");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feature/one"), "unchanged");
        assert_eq!(g.branches.len(), 2);
    });
    assert_eq!(notification_count(handle, cx), 1, "git's own message");
}

#[gpui_kit::test]
fn a_branchs_menu_renames_merges_publishes_and_deletes_asking_to_force(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["checkout", "-q", "-b", "feat"]);
    commit_file(repo.path(), "b.txt", "b\n", "b");
    git_run(repo.path(), &["checkout", "-q", "main"]);
    let (handle, ws) = open(cx);
    let id = open_git_on(
        handle,
        &ws,
        cx,
        repo.path(),
        crate::git_block::Tab::Branches,
    );
    // feat, main; feat's menu: Compare with main, Merge into main…, Rename…, Delete…, Publish,
    // Copy name.
    let menu = |cx: &mut TestAppContext, row: usize, item: usize| {
        cx.update_window(handle, |_, window, cx| {
            window.right_click(branch_row(id, false, row), cx);
            window.render_frame(cx);
            window.within("popup-menu").click(item, cx);
            window.render_frame(cx);
        })
        .unwrap();
    };
    menu(cx, 0, 2);
    cx.update_window(handle, |_, window, cx| {
        window.press("ctrl-a", cx);
        window.press("cmd-a", cx);
        window.input("feature", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.branches[0].name, "feature"));
    // Delete… before it has an upstream: unmerged anywhere, so git refuses.
    menu(cx, 0, 3);
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.branches.len(), 3, "not merged: still there")
    });
    // "Delete feature anyway?": not now.
    cx.update_window(handle, |_, window, cx| {
        assert!(window.has_active_dialog(cx), "asks again, with the count");
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
    })
    .unwrap();
    menu(cx, 0, 4);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.branches[0].upstream.as_deref(), Some("origin/feature"))
    });
    // Published: merged into its upstream, so `git branch -d` takes it.
    menu(cx, 0, 3);
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(
            !g.branches.iter().any(|b| b.name == "feature"),
            "{:?}",
            g.branches
        );
    });
    // Merge the remote one into main: origin/feature is the only remote row besides origin/main.
    let remote_rows = git_of(&ws, cx, id, |g| {
        g.branches.iter().filter(|b| b.remote).count()
    });
    assert_eq!(remote_rows, 2);
    cx.update_window(handle, |_, window, cx| {
        // The remote rows are folded at first.
        window.click(("git-remote-fold", id.0 as usize), cx);
        window.render_frame(cx);
        window.right_click(branch_row(id, true, 0), cx);
        window.render_frame(cx);
        // Compare with main, Merge into main…, Copy name.
        window.within("popup-menu").click(1usize, cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert!(repo.path().join("b.txt").is_file(), "merged");
}

fn stash_row(id: BlockId, i: usize) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(format!("git-stash-{}-{i}", id.0).into())
}

#[gpui_kit::test]
fn stash_changes_asks_for_a_message_and_lists_the_stash(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::Stashes);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-stash-changes", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        window.input("my work", cx);
        window.within("dialog").click("flag", cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.stashes.len(), 1);
        assert_eq!(g.stashes[0].message, "On main: my work");
        assert!(g.status.entries.is_empty(), "untracked went too");
    });
    assert!(!repo.path().join("new.txt").exists());
}

#[gpui_kit::test]
fn a_stash_is_shown_applied_popped_and_dropped(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    git_run(repo.path(), &["stash", "push", "-q", "-m", "one"]);
    std::fs::write(repo.path().join("a.txt"), "other\n").unwrap();
    git_run(repo.path(), &["stash", "push", "-q", "-m", "two"]);
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::Stashes);
    git_of(&ws, cx, id, |g| assert_eq!(g.stashes.len(), 2));
    cx.update_window(handle, |_, window, cx| window.click(stash_row(id, 1), cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.stash_shown, Some(1));
        let p = g.diff();
        assert!(
            std::ptr::eq(p, &g.stash_panel),
            "the Stashes tab's own panel"
        );
        assert_eq!(p.files.len(), 1);
        assert_eq!(p.files[0].diff.path(), "a.txt");
        assert!(
            p.files[0].diff.hunks[0]
                .lines
                .iter()
                .any(|l| l.text == "changed")
        );
    });
    // Apply, Pop, Drop…, Show.
    let menu = |cx: &mut TestAppContext, row: usize, item: usize| {
        cx.update_window(handle, |_, window, cx| {
            window.right_click(stash_row(id, row), cx);
            window.render_frame(cx);
            window.within("popup-menu").click(item, cx);
            window.render_frame(cx);
        })
        .unwrap();
        settle(handle, cx);
    };
    menu(cx, 1, 0);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "changed\n"
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.stashes.len(), 2, "apply keeps it")
    });
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    menu(cx, 0, 1);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "other\n"
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.stashes.len(), 1, "pop drops it")
    });
    menu(cx, 0, 2);
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert!(g.stashes.is_empty()));
}

#[gpui_kit::test]
fn a_stash_shows_its_new_files_and_survives_the_two_second_read(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    git_run(
        repo.path(),
        &["stash", "push", "-q", "-u", "-m", "with new"],
    );
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::Stashes);
    cx.update_window(handle, |_, window, cx| window.click(stash_row(id, 0), cx))
        .unwrap();
    settle(handle, cx);
    let paths = |cx: &mut TestAppContext| {
        git_of(&ws, cx, id, |g| {
            g.stash_panel
                .files
                .iter()
                .map(|s| s.diff.path().to_string())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(paths(cx), ["a.txt", "new.txt"]);
    // An agent writes meanwhile: the read changes the Changes panel only.
    std::fs::write(repo.path().join("x.txt"), "agent\n").unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    assert_eq!(paths(cx), ["a.txt", "new.txt"]);
}

// Final review, C1: the 2-second read must leave the diffs alone.
#[gpui_kit::test]
fn a_commits_file_diff_survives_the_two_second_read(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "a.txt", "one\ntwo\n", "Second");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-commit", 0usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("git-commit-file-{n}-0").into()),
            cx,
        )
    })
    .unwrap();
    settle(handle, cx);
    let files = |cx: &mut TestAppContext| git_of(&ws, cx, id, |g| g.history_panel.files.len());
    assert_eq!(files(cx), 1);
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    assert_eq!(files(cx), 1, "still shown");
}

/// `main` with one commit, and `feature` one commit ahead of it (unmerged).
fn repo_with_unmerged_feature() -> tempfile::TempDir {
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "feature"]);
    commit_file(repo.path(), "f.txt", "f\n", "feature work");
    git_run(repo.path(), &["checkout", "-q", "main"]);
    repo
}

// Final review, C2: a notification's buttons act on its own block, in its own workspace.
#[gpui_kit::test]
fn delete_anyway_after_a_switch_acts_on_its_own_repository(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let (x, y) = (repo_with_unmerged_feature(), repo_with_unmerged_feature());
    let (handle, ws) = open(cx);
    let in_x = open_git(handle, &ws, cx, x.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.delete_now(in_x, "feature".to_string(), false, window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(
            window.has_active_dialog(cx),
            "refused: not merged, asks again"
        )
    })
    .unwrap();
    // A new workspace whose first block takes id 0, like X's terminal: Y's Git block gets
    // X's id.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.add_note_at(Point::new(3000.0, 3000.0), window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    let in_y = open_git(handle, &ws, cx, y.path());
    assert_eq!(in_y, in_x, "block ids repeat across workspaces");
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert!(
        !git_run(y.path(), &["branch", "--list", "feature"]).is_empty(),
        "the other workspace's repository is untouched"
    );
    assert!(
        git_run(x.path(), &["branch", "--list", "feature"]).is_empty(),
        "deleted in X"
    );
}

/// a.txt, renamed to b.txt, then changed; then another file. History of b.txt has three commits.
fn renamed_repo() -> tempfile::TempDir {
    let repo = git_repo();
    commit_file(
        repo.path(),
        "a.txt",
        "one\ntwo\nthree\nfour\nfive\n",
        "add a",
    );
    git_run(repo.path(), &["mv", "a.txt", "b.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "rename to b"]);
    commit_file(
        repo.path(),
        "b.txt",
        "one\nTWO\nthree\nfour\nfive\n",
        "change b",
    );
    commit_file(repo.path(), "o.txt", "o\n", "other");
    repo
}

#[gpui_kit::test]
fn file_history_follows_a_rename_and_shows_each_commits_change(cx: &mut TestAppContext) {
    let repo = renamed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.file_history(id, "b.txt".to_string(), window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.tab, crate::git_block::Tab::History);
        let subjects: Vec<_> = g.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["change b", "rename to b", "add a"]);
        let paths: Vec<_> = g.file_paths.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["b.txt", "b.txt", "a.txt"]);
        assert!(g.graph.is_empty());
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-commit", 2usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        let files: Vec<_> = g
            .history_panel
            .files
            .iter()
            .map(|s| s.diff.path().to_string())
            .collect();
        assert_eq!(
            files,
            ["a.txt"],
            "the file's change in that commit, by its name then"
        );
    });
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-commit-back", n), cx);
        window.render_frame(cx);
        window.click(("git-file-clear", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.file, None);
        assert_eq!(g.commits.len(), 4, "the whole history again");
    });
}

#[gpui_kit::test]
fn restore_this_version_writes_the_old_path_to_the_new_one(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = renamed_repo();
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.file_history(id, "b.txt".to_string(), window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("git-commit", 2usize), cx);
        window.render_frame(cx);
        // Checkout…, New branch here…, Create tag…, Revert…, Copy hash, Copy message, Restore this version….
        window.within("popup-menu").click(6usize, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "asks first");
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("b.txt")).unwrap(),
        "one\ntwo\nthree\nfour\nfive\n",
        "a.txt's text then, written to b.txt"
    );
    assert!(
        !repo.path().join("a.txt").exists(),
        "the old name isn't brought back"
    );
    let kept = ws.read_with(cx, |ws, _| ws.trash.as_ref().unwrap().list());
    assert_eq!(kept.len(), 1, "the version before is in Recently discarded");
    assert_eq!(kept[0].files[0].0, "b.txt");
}

#[gpui_kit::test]
fn file_history_from_an_editors_tree_opens_a_git_block(cx: &mut TestAppContext) {
    let repo = renamed_repo();
    let (handle, ws) = open(cx);
    let editor = open_editor(handle, &ws, cx, repo.path());
    let file = repo.path().join("b.txt");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.history_from_tree(editor, file.clone(), false, window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    settle(handle, cx);
    let git = ws.read_with(cx, |ws, _| {
        ws.space
            .blocks
            .iter()
            .find_map(|(id, b)| b.git().map(|_| *id))
    });
    let id = git.expect("a Git block beside the editor");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.file.as_deref(), Some("b.txt"));
        assert_eq!(g.commits.len(), 3);
    });
    // Asked again: the same block, not a second one.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.history_from_tree(editor, file.clone(), false, window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    let blocks = ws.read_with(cx, |ws, _| {
        ws.space
            .blocks
            .iter()
            .filter(|(_, b)| b.git().is_some())
            .count()
    });
    assert_eq!(blocks, 1);
}

#[gpui_kit::test]
fn file_history_from_a_changes_row(cx: &mut TestAppContext) {
    let repo = renamed_repo();
    std::fs::write(repo.path().join("b.txt"), "changed\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        window.right_click(crate::git_block::row_id(id, "files", 0), cx);
        window.render_frame(cx);
        // Stage, Discard changes…, Open, File history (a new file has none).
        window.within("popup-menu").click(3usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.tab, crate::git_block::Tab::History);
        assert_eq!(g.file.as_deref(), Some("b.txt"));
        assert_eq!(g.commits.len(), 3);
    });
}

#[gpui_kit::test]
fn blame_groups_lines_by_commit_and_a_click_opens_it(cx: &mut TestAppContext) {
    use crate::git_block::diff_view::{Row, Source};
    let repo = git_repo();
    commit_file(repo.path(), "a.txt", "one\ntwo\n", "first");
    commit_file(repo.path(), "a.txt", "one\nTWO\n", "second");
    std::fs::write(repo.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.blame_file(id, "a.txt".to_string(), None, window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    let rows = git_of(&ws, cx, id, |g| {
        assert_eq!(g.tab, crate::git_block::Tab::History);
        let p = g.diff();
        assert!(matches!(&p.source, Source::Blame { hashes, .. } if hashes.len() == 3));
        let h = &p.files[0].diff.hunks;
        assert!(h[0].context.ends_with("· first"));
        assert!(h[1].context.ends_with("· second"));
        assert_eq!(h[2].context, "Not committed yet");
        (*p.rows).clone()
    });
    let at = |hunk: usize| {
        rows.iter()
            .position(|r| matches!(r, Row::Hunk { hunk: h, .. } if *h == hunk))
            .unwrap()
    };
    // Not committed: nothing to open.
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-hunk", at(2)), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.shown.is_none());
        assert!(g.error.is_none(), "nothing was asked of git: {:?}", g.error);
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-hunk", at(1)), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(
            g.shown.as_ref().map(|s| s.commit.subject.as_str()),
            Some("second")
        )
    });
    // Back: the blame again.
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-commit-back", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(matches!(g.diff().source, Source::Blame { .. }))
    });
}

// Final review, I1: a commit's read that comes back after the user left it changes nothing.
#[gpui_kit::test]
fn a_commit_left_before_its_diff_came_back_stays_left(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-commit", 0usize), cx);
        ws.update(cx, |ws, cx| ws.hide_commit(id, window, cx));
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.shown.is_none(), "the list stays");
        assert!(g.history_panel.files.is_empty());
    });
}

// Final review, I2: while another file's history loads, the first file's commits are gone
// (Restore this version… would pair one file's commit with the other's path).
#[gpui_kit::test]
fn another_files_history_never_lists_the_first_files_commits(cx: &mut TestAppContext) {
    let repo = renamed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let history = |cx: &mut TestAppContext, path: &str| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.file_history(id, path.to_string(), window, cx)
            })
        })
        .unwrap();
    };
    history(cx, "b.txt");
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.commits.len(), 3));
    history(cx, "o.txt");
    git_of(&ws, cx, id, |g| {
        assert!(g.commits.is_empty(), "b.txt's commits are gone at once");
        assert!(g.file_paths.is_empty());
        assert!(!g.more);
    });
    settle(handle, cx);
    let subjects: Vec<String> = git_of(&ws, cx, id, |g| {
        g.commits.iter().map(|c| c.subject.clone()).collect()
    });
    assert_eq!(subjects, ["other"]);
}

// Final review, I4: a file's history (`git log --follow`) has no search and no All branches.
#[gpui_kit::test]
fn a_files_history_has_no_search_or_all_branches(cx: &mut TestAppContext) {
    let repo = renamed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.file_history(id, "b.txt".to_string(), window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("git-search-message", n)).is_none());
        assert!(window.try_find(("git-all-branches", n)).is_none());
        window.click(("git-file-clear", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(("git-search-message", n)).is_some(),
            "the whole history has them"
        );
    })
    .unwrap();
}

// Final review (re-graded): an empty History or Stashes panel names nothing, and a blame turned
// off leaves no blame's header.
#[gpui_kit::test]
fn an_empty_history_panel_has_no_title(cx: &mut TestAppContext) {
    use crate::git_block::diff_view::Source;
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.diff().title(), "");
        assert_eq!(g.stash_panel.title(), "");
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-commit", 0usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.diff().title().starts_with("Commit "))
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-commit-back", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.diff().title(), "", "back to the list")
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.blame_file(id, "a.txt".to_string(), None, window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.diff().title().starts_with("Blame"))
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.toggle_blame(id, window, cx))
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.diff().title(), "");
        assert!(
            !matches!(g.diff().source, Source::Blame { .. }),
            "Unified and Split come back"
        );
    });
}

// Final review (declined item, ruled a finding): Load more asked twice for one page adds it once.
#[gpui_kit::test]
fn a_second_load_more_for_the_same_page_adds_nothing(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second");
    commit_file(repo.path(), "c.txt", "c\n", "Third");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    // As if the first page had held one commit, and Load more were clicked twice.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let git = ws.space.git_mut(id).unwrap();
            git.commits = std::rc::Rc::new(git.commits[..1].to_vec());
            let at = ws.here(id);
            ws.load_history(at.clone(), 1, window, cx);
            ws.load_history(at, 1, window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    let subjects: Vec<String> = git_of(&ws, cx, id, |g| {
        g.commits.iter().map(|c| c.subject.clone()).collect()
    });
    assert_eq!(subjects, ["Third", "Second", "First"]);
}
