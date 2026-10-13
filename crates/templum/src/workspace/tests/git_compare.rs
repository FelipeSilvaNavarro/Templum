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
