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
        assert_eq!(shown.files[0].path, "a.txt");
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("git-commit-file-{n}-0").into()),
            cx,
        )
    })
    .unwrap();
    settle(handle, cx);
    let diff = git_of(&ws, cx, id, |g| g.commit_diff.clone());
    let text = ws.read_with(cx, |_, cx| diff.read(cx).value().to_string());
    assert!(text.contains("+two"), "{text}");
}

#[gpui_kit::test]
fn the_search_filters_by_subject_and_author_without_case(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second");
    git_run(repo.path(), &["config", "user.name", "Grace"]);
    commit_file(repo.path(), "c.txt", "c\n", "Third");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let search = git_of(&ws, cx, id, |g| g.search.clone());
    let filtered = |cx: &mut TestAppContext, text: &str| {
        cx.update_window(handle, |_, window, cx| {
            search.update(cx, |s, cx| s.set_value(text, window, cx));
            window.render_frame(cx);
        })
        .unwrap();
        ws.read_with(cx, |ws, cx| ws.space.git(id).unwrap().filtered(cx))
    };
    assert_eq!(filtered(cx, "SECOND"), [1]);
    assert_eq!(filtered(cx, "grace"), [0]);
    assert_eq!(filtered(cx, ""), [0, 1, 2]);
}

#[gpui_kit::test]
fn load_more_appends_the_next_page(cx: &mut TestAppContext) {
    use crate::git_block::PAGE;
    let repo = committed_repo();
    for i in 0..PAGE {
        git_run(
            repo.path(),
            &["commit", "-q", "--allow-empty", "-m", &format!("c{i}")],
        );
    }
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.commits.len(), PAGE);
        assert!(g.more);
    });
    // The list is virtual: Load more is drawn once the end is scrolled into view.
    cx.update_window(handle, |_, window, cx| {
        // Over a row: the wheel reaches the list around it.
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
        assert_eq!(g.commits.len(), PAGE + 1);
        assert!(!g.more);
        assert_eq!(g.commits[PAGE].subject, "First");
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
    // Checkout…, New branch from here…, Create tag…, Revert…, Copy hash.
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
    menu(cx, 0, 4);
    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .unwrap_or_default();
    assert_eq!(text, first);
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
    let diff = git_of(&ws, cx, id, |g| g.commit_diff.clone());
    let text =
        |cx: &mut TestAppContext| ws.read_with(cx, |_, cx| diff.read(cx).value().to_string());
    assert!(text(cx).contains("+two"));
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    assert!(text(cx).contains("+two"), "still shown: {:?}", text(cx));
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
