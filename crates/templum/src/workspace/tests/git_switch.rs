//! Switching branches: the switcher, the question about changes, the unpushed notice.

use gpui_kit::Focusable as _;
use gpui_kit::component::WindowExt as _;

use super::*;

/// main with a.txt, and feat (b.txt), on main.
fn two_branches() -> tempfile::TempDir {
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "feat"]);
    std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
    git_run(repo.path(), &["add", "b.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "b"]);
    git_run(repo.path(), &["checkout", "-q", "main"]);
    repo
}

fn switch(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    name: &str,
) {
    let name = name.to_string();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.switch_branch(id, name, false, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
}

fn answer(handle: AnyWindowHandle, cx: &mut TestAppContext, button: &'static str) {
    cx.update_window(handle, |_, window, cx| {
        assert!(window.has_active_dialog(cx), "the question");
        window.within("dialog").click(button, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
}

#[gpui_kit::test]
fn the_switcher_finds_a_branch_and_enter_switches(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = two_branches();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| window.click(("branch", n), cx))
        .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.has_active_dialog(cx), "the switcher");
    })
    .unwrap();
    cx.simulate_input(handle, "fea");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"))
    });
}

#[gpui_kit::test]
fn changes_left_on_a_branch_come_back_with_it(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = two_branches();
    std::fs::write(repo.path().join("a.txt"), "mine\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    switch(handle, &ws, cx, id, "feat");
    answer(handle, cx, "git-leave");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"));
        assert!(g.status.entries.is_empty(), "left on main");
    });
    assert!(git_run(repo.path(), &["stash", "list"]).contains("templum: left on main"));
    // Back to main: no question (nothing changed on feat), and the changes are back.
    switch(handle, &ws, cx, id, "main");
    cx.update_window(handle, |_, window, cx| {
        assert!(!window.has_active_dialog(cx))
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "mine\n"
    );
    assert_eq!(git_run(repo.path(), &["stash", "list"]), "", "popped");
}

#[gpui_kit::test]
fn bringing_changes_along_and_a_refusal_offers_leaving_them(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = two_branches();
    std::fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    switch(handle, &ws, cx, id, "feat");
    answer(handle, cx, "git-bring");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"));
        assert_eq!(g.status.entries.len(), 1, "carried");
    });
    // feat changes a.txt; a change to it on main can't come along.
    std::fs::remove_file(repo.path().join("new.txt")).unwrap();
    git_run(repo.path(), &["checkout", "-q", "main"]);
    git_run(repo.path(), &["checkout", "-q", "feat"]);
    std::fs::write(repo.path().join("a.txt"), "feat\n").unwrap();
    git_run(repo.path(), &["commit", "-q", "-am", "a on feat"]);
    git_run(repo.path(), &["checkout", "-q", "main"]);
    std::fs::write(repo.path().join("a.txt"), "dirty\n").unwrap();
    settle_read(handle, &ws, cx, id);
    switch(handle, &ws, cx, id, "feat");
    answer(handle, cx, "git-bring");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("main"), "refused")
    });
    assert_eq!(notification_count(handle, cx), 1);
    cx.update_window(handle, |_, window, cx| window.click("git-leave-them", cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"))
    });
    assert!(git_run(repo.path(), &["stash", "list"]).contains("templum: left on main"));
}

#[gpui_kit::test]
fn commit_first_stays_and_gives_the_summary_the_keyboard(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = two_branches();
    std::fs::write(repo.path().join("a.txt"), "mine\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    switch(handle, &ws, cx, id, "feat");
    answer(handle, cx, "git-commit-first");
    let summary = git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("main"));
        g.summary.clone()
    });
    cx.update_window(handle, |_, window, cx| {
        assert!(summary.read(cx).focus_handle(cx).is_focused(window));
    })
    .unwrap();
}

#[gpui_kit::test]
fn leaving_a_branch_with_commits_not_pushed_offers_to_push_it(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = two_branches();
    let remote = add_remote(repo.path());
    std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
    git_run(repo.path(), &["commit", "-q", "-am", "two"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    switch(handle, &ws, cx, id, "feat");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"), "no question")
    });
    assert_eq!(
        notification_count(handle, cx),
        1,
        "main has 1 commit not pushed yet"
    );
    cx.update_window(handle, |_, window, cx| window.click("git-push-left", cx))
        .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(remote.path(), &["rev-parse", "main"]),
        git_run(repo.path(), &["rev-parse", "main"])
    );
}

#[gpui_kit::test]
fn a_detached_head_says_so_and_goes_back_to_main(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = two_branches();
    git_run(repo.path(), &["checkout", "-q", "--detach", "feat"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.default_branch.as_deref(), Some("main"))
    });
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find(("git-banner", n)).is_some(), "the banner");
        window.click(("git-back", n), cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("main"))
    });
    cx.update_window(handle, |_, window, _| {
        assert!(
            window.try_find(("git-banner", n)).is_none(),
            "gone with the detached HEAD"
        )
    })
    .unwrap();
}

#[gpui_kit::test]
fn create_a_branch_here_keeps_the_commits_on_a_branch(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = two_branches();
    git_run(repo.path(), &["checkout", "-q", "--detach", "feat"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-branch-here", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        window.input("rescue", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("rescue"))
    });
}
