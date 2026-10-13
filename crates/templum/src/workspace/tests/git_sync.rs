//! The Git block's top bar: Sync, Publish, the ⋯ menu, and the server's state.

use super::*;

/// `repo` with an origin, and a second clone of it whose pushes `repo` can get.
fn other_clone(remote: &tempfile::TempDir) -> tempfile::TempDir {
    let other = tempfile::tempdir().unwrap();
    git_run(
        other.path(),
        &["clone", "-q", &remote.path().display().to_string(), "."],
    );
    git_run(other.path(), &["config", "user.name", "B"]);
    git_run(other.path(), &["config", "user.email", "b@example.com"]);
    other
}

fn commit_in(dir: &std::path::Path, name: &str) {
    std::fs::write(dir.join(name), name).unwrap();
    git_run(dir, &["add", name]);
    git_run(dir, &["commit", "-q", "-m", name]);
}

#[gpui_kit::test]
fn sync_gets_and_sends_in_one_click(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    let repo = committed_repo();
    let remote = add_remote(repo.path());
    let other = other_clone(&remote);
    commit_in(other.path(), "b.txt");
    git_run(other.path(), &["push", "-q"]);
    commit_in(repo.path(), "c.txt");
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    // Diverged: the question first. Put my commits on top.
    no_motion(cx);
    cx.update_window(handle, |_, window, cx| window.click(("git-sync", n), cx))
        .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.has_active_dialog(cx), "both sides have new commits");
        window.within("dialog").click("git-rebase", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!((g.status.ahead, g.status.behind), (0, 0), "joined and sent");
        assert_eq!(g.sync, None, "no spinner left");
    });
    assert!(repo.path().join("b.txt").is_file());
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%s"]),
        "c.txt\n",
        "mine on top"
    );
    // Then a plain one: the server got one more (the other clone takes ours first: no divergence there).
    git_run(other.path(), &["pull", "-q", "--ff-only"]);
    commit_in(other.path(), "d.txt");
    git_run(other.path(), &["push", "-q"]);
    cx.update_window(handle, |_, window, cx| window.click(("git-sync", n), cx))
        .unwrap();
    settle(handle, cx);
    assert!(repo.path().join("d.txt").is_file(), "fast-forwarded");
}

#[gpui_kit::test]
fn publish_goes_to_the_only_remote_and_asks_among_several(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["checkout", "-q", "-b", "topic"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| window.click(("git-sync", n), cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(
            g.status.upstream.as_deref(),
            Some("origin/topic"),
            "no question with one remote"
        )
    });
    // A second remote: Publish lists them.
    let backup = tempfile::tempdir().unwrap();
    git_run(backup.path(), &["init", "-q", "--bare"]);
    git_run(
        repo.path(),
        &[
            "remote",
            "add",
            "backup",
            &backup.path().display().to_string(),
        ],
    );
    git_run(repo.path(), &["checkout", "-q", "-b", "next"]);
    settle_read(handle, &ws, cx, id);
    cx.update_window(handle, |_, window, cx| window.click(("git-sync", n), cx))
        .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(ws.read(cx).menu.is_some(), "the remotes' menu");
        // backup, origin: as `git remote` lists them.
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.upstream.as_deref(), Some("backup/next"))
    });
}

#[gpui_kit::test]
fn without_a_remote_publish_says_so(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| window.click(("git-sync", n), cx))
        .unwrap();
    settle(handle, cx);
    assert_eq!(notification_count(handle, cx), 1, "No remote");
    git_of(&ws, cx, id, |g| assert_eq!(g.status.upstream, None));
}

#[gpui_kit::test]
fn a_gone_upstream_offers_publish_not_sync(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["checkout", "-q", "-b", "topic"]);
    git_run(repo.path(), &["push", "-q", "-u", "origin", "topic"]);
    git_run(repo.path(), &["push", "-q", "origin", "--delete", "topic"]);
    git_run(repo.path(), &["fetch", "-q", "--prune"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| {
        assert!(g.status.gone);
        assert_eq!(
            crate::git_block::words::server_state(&g.status).0,
            "not published"
        );
    });
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| window.click(("git-sync", n), cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert!(!g.status.gone, "published again"));
}

#[gpui_kit::test]
fn the_more_menu_fetches_and_lists_recently_discarded(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let remote = add_remote(repo.path());
    let other = other_clone(&remote);
    commit_in(other.path(), "b.txt");
    git_run(other.path(), &["push", "-q"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-more", n), cx);
        window.render_frame(cx);
        assert!(ws.read(cx).menu.is_some());
        // Fetch, Pull, Push, —, Clone a repository… (no data folder: no Recently discarded…).
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.status.behind, 1, "fetched"));
}

#[gpui_kit::test]
fn clone_from_a_folder_that_isnt_a_repository_opens_the_block_on_it(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    let remote = add_remote(repo.path());
    let empty = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, empty.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert!(matches!(g.found, crate::git_block::Found::NotARepo))
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-clone", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "URL and destination");
    })
    .unwrap();
    // The URL field has the keyboard; the destination holds the chosen folder.
    cx.simulate_input(handle, &remote.path().display().to_string());
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    let root = git_of(&ws, cx, id, |g| g.repo().map(|r| r.root.clone()));
    assert_eq!(
        root.unwrap().canonicalize().unwrap(),
        empty.path().canonicalize().unwrap(),
        "an empty folder takes the clone"
    );
    assert!(empty.path().join("a.txt").is_file());
}

#[gpui_kit::test]
fn a_failed_clone_says_why_and_leaves_no_folder(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    std::fs::write(parent.path().join("keep.txt"), "").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, parent.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.clone_repo(
                id,
                "/no/such/app.git".to_string(),
                parent.path().display().to_string(),
                window,
                cx,
            )
        })
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.sync, Some(crate::git_block::Sync::Cloning), "spinning")
    });
    settle(handle, cx);
    assert_eq!(notification_count(handle, cx), 1);
    assert!(!parent.path().join("app").exists(), "nothing half-made");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.sync, None);
        assert!(matches!(g.found, crate::git_block::Found::NotARepo));
    });
}
