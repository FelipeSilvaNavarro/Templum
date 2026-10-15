//! The Branches tab and Compare.

use gpui_kit::component::WindowExt as _;

use super::*;

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

fn on_tab(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    tab: crate::git_block::Tab,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.set_git_tab(id, tab, window, cx))
    })
    .unwrap();
    settle(handle, cx);
}

/// main (a.txt) and feature with `n` commits of its own (f0.txt, f1.txt…), on main.
fn feature_repo(n: usize) -> tempfile::TempDir {
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "feature"]);
    for i in 0..n {
        let name = format!("f{i}.txt");
        std::fs::write(repo.path().join(&name), "f\n").unwrap();
        git_run(repo.path(), &["add", &name]);
        git_run(repo.path(), &["commit", "-q", "-m", &name]);
    }
    git_run(repo.path(), &["checkout", "-q", "main"]);
    repo
}

#[gpui_kit::test]
fn the_remote_branches_are_folded_until_asked(cx: &mut TestAppContext) {
    let repo = feature_repo(1);
    let _remote = add_remote(repo.path());
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    on_tab(handle, &ws, cx, id, crate::git_block::Tab::Branches);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        assert!(
            window.try_find(branch_row(id, false, 0)).is_some(),
            "feature"
        );
        assert!(window.try_find(branch_row(id, true, 0)).is_none(), "folded");
        window.click(("git-remote-fold", n), cx);
        window.render_frame(cx);
        assert!(
            window.try_find(branch_row(id, true, 0)).is_some(),
            "origin/main"
        );
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(g.branches.iter().all(|b| b.time > 0), "each has its age")
    });
}

#[gpui_kit::test]
fn deleting_an_unmerged_branch_asks_again_with_the_count(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = feature_repo(3);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.delete_branch(id, "feature".to_string(), false, window, cx)
        });
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert!(
        !git_run(repo.path(), &["branch", "--list", "feature"]).is_empty(),
        "refused once"
    );
    cx.update_window(handle, |_, window, cx| {
        assert!(
            window.has_active_dialog(cx),
            "Its 3 commits not merged anywhere will be lost."
        );
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert!(
        git_run(repo.path(), &["branch", "--list", "feature"]).is_empty(),
        "deleted"
    );
}

fn compare(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    branch: &str,
) {
    let branch = branch.to_string();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.compare_branch(id, branch, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
}

fn shown_paths(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> Vec<String> {
    git_of(ws, cx, id, |g| {
        g.diff()
            .files
            .iter()
            .map(|s| s.diff.path().to_string())
            .collect()
    })
}

#[gpui_kit::test]
fn compare_shows_what_a_branch_has_and_narrows_to_one_commit(cx: &mut TestAppContext) {
    let repo = feature_repo(2);
    // main moves on after feature left: not part of the comparison.
    std::fs::write(repo.path().join("a.txt"), "main moved\n").unwrap();
    git_run(repo.path(), &["commit", "-q", "-am", "main moved"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    compare(handle, &ws, cx, id, "feature");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.tab, crate::git_block::Tab::Branches);
        let c = g.compare.as_ref().unwrap();
        assert_eq!((c.branch.as_str(), c.base.as_str()), ("feature", "main"));
        assert_eq!(c.commits.len(), 2);
        assert_eq!(g.diff().title(), "feature vs main · 2 commits · 2 files");
    });
    assert_eq!(shown_paths(&ws, cx, id), ["f0.txt", "f1.txt"]);
    // The newest commit alone (f1.txt), then all of them again.
    cx.update_window(handle, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("git-compare-commit-{n}-0").into()),
            cx,
        )
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(shown_paths(&ws, cx, id), ["f1.txt"]);
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-compare-all", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(shown_paths(&ws, cx, id), ["f0.txt", "f1.txt"]);
    // Read-only: no Stage or Discard on a comparison's file row.
    cx.update_window(handle, |_, window, cx| {
        window.hover(("git-file", 0usize), cx);
        window.render_frame(cx);
        assert!(
            window
                .try_find(gpui_kit::ElementId::Name(
                    format!("git-panel-stage-{n}-0").into()
                ))
                .is_none()
        );
        window.click(("git-compare-close", n), cx);
        window.render_frame(cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| assert!(g.compare.is_none()));
}

#[gpui_kit::test]
fn the_comparison_survives_the_two_second_read(cx: &mut TestAppContext) {
    let repo = feature_repo(1);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    // The Changes panel has a file folded: it must stay so.
    std::fs::write(repo.path().join("w.txt"), "working\n").unwrap();
    settle_read(handle, &ws, cx, id);
    cx.update_window(handle, |_, _, cx| {
        ws.update(cx, |ws, cx| {
            ws.toggle_fold(id, (false, "w.txt".to_string()), cx)
        })
    })
    .unwrap();
    compare(handle, &ws, cx, id, "feature");
    std::fs::write(repo.path().join("x.txt"), "agent\n").unwrap();
    settle_read(handle, &ws, cx, id);
    assert_eq!(
        shown_paths(&ws, cx, id),
        ["f0.txt"],
        "the read never replaces the comparison"
    );
    on_tab(handle, &ws, cx, id, crate::git_block::Tab::Changes);
    let paths = shown_paths(&ws, cx, id);
    assert!(paths.contains(&"x.txt".to_string()), "{paths:?}");
    git_of(&ws, cx, id, |g| {
        assert!(g.panel.folded.contains(&(false, "w.txt".to_string())))
    });
}

#[gpui_kit::test]
fn the_top_bar_compares_the_current_branch_with_main(cx: &mut TestAppContext) {
    let repo = feature_repo(1);
    git_run(repo.path(), &["checkout", "-q", "feature"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-compare-main", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(
            g.compare.as_ref().map(|c| c.branch.as_str()),
            Some("feature")
        )
    });
    // On main itself there is nothing to compare: no button.
    git_run(repo.path(), &["checkout", "-q", "main"]);
    settle_read(handle, &ws, cx, id);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find(("git-compare-main", n)).is_none())
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_base_can_be_another_branch(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = feature_repo(2);
    git_run(repo.path(), &["branch", "half", "feature~1"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    on_tab(handle, &ws, cx, id, crate::git_block::Tab::Branches);
    compare(handle, &ws, cx, id, "feature");
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-compare-base", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
    })
    .unwrap();
    cx.simulate_input(handle, "half");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        let c = g.compare.as_ref().unwrap();
        assert_eq!(c.base, "half");
        assert_eq!(c.commits.len(), 1);
    });
}

// Review M7: a comparison's lines aren't the working file's: no Open in editor at this line.
#[gpui_kit::test]
fn a_comparisons_lines_offer_no_open_at_this_line(cx: &mut TestAppContext) {
    let repo = feature_repo(1);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    compare(handle, &ws, cx, id, "feature");
    let row = git_of(&ws, cx, id, |g| {
        g.diff()
            .rows
            .iter()
            .position(|r| matches!(r, crate::git_block::diff_view::Row::Line { .. }))
            .expect("a line")
    });
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("git-line", row), cx);
        window.render_frame(cx);
        // Copy, Copy line, Copy hunk, Send to ▸; no —, Open in editor at this line.
        assert!(
            window.within("popup-menu").try_find(3usize).is_some(),
            "the menu"
        );
        assert!(window.within("popup-menu").try_find(5usize).is_none());
    })
    .unwrap();
}

// Review M9: a branch whose upstream was deleted on the server can be published again.
#[gpui_kit::test]
fn a_branch_gone_from_the_server_offers_publish(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = feature_repo(1);
    let remote = add_remote(repo.path());
    git_run(repo.path(), &["push", "-q", "-u", "origin", "feature"]);
    git_run(remote.path(), &["branch", "-q", "-D", "feature"]);
    git_run(repo.path(), &["fetch", "-q", "--prune"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    on_tab(handle, &ws, cx, id, crate::git_block::Tab::Branches);
    git_of(&ws, cx, id, |g| {
        assert!(g.branches[0].gone, "{:?}", g.branches[0])
    });
    cx.update_window(handle, |_, window, cx| {
        window.right_click(branch_row(id, false, 0), cx);
        window.render_frame(cx);
        // Compare with main, Merge into main…, Rename…, Delete…, Publish, Copy name.
        window.within("popup-menu").click(4usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(remote.path(), &["rev-parse", "feature"]),
        git_run(repo.path(), &["rev-parse", "feature"]),
        "published again"
    );
}
