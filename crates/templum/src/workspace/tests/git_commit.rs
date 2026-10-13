//! The Git block: committing, syncing with a remote, and failures.

use super::*;

#[test]
fn a_failure_that_wanted_a_key_or_a_password_says_so() {
    let err = |stderr: &str| templum_git::GitError {
        command: "git push".to_string(),
        code: Some(128),
        stderr: stderr.to_string(),
        missing: false,
    };
    assert_eq!(
        crate::workspace::git_error_message(&err(
            "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n"
        )),
        "git@github.com: Permission denied (publickey). Git needs your key or password: run it in a terminal."
    );
    assert_eq!(
        crate::workspace::git_error_message(&err("fatal: not a git repository\n")),
        "fatal: not a git repository"
    );
}

#[cfg(target_os = "macos")]
const COMMIT_KEY: &str = "cmd-enter";
#[cfg(not(target_os = "macos"))]
const COMMIT_KEY: &str = "ctrl-enter";

#[gpui_kit::test]
fn commit_needs_something_staged_and_a_summary_then_commits(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let disabled = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, _| {
            window.try_find(("git-commit", n)).is_none()
        })
        .unwrap()
    };
    assert!(disabled(cx), "nothing staged");
    click_row_button(
        handle,
        cx,
        row_id(id, "unstaged", 0),
        git_button(id, "stage", "unstaged", 0),
    );
    assert!(disabled(cx), "no summary");
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.focus(window, cx));
        window.input("Add a", cx);
        window.render_frame(cx);
        assert!(window.try_find(("git-commit", n)).is_some());
        window.click(("git-commit", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.status.entries.is_empty(), "committed");
        assert!(g.status.oid.is_some());
    });
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%s"]),
        "Add a\n"
    );
    ws.read_with(cx, |_, cx| {
        assert_eq!(summary.read(cx).value(), "", "cleared")
    });
    assert_eq!(notification_count(handle, cx), 1, "Committed abc1234");
}

#[gpui_kit::test]
fn ctrl_enter_commits_with_the_description_and_amend_fills_the_fields(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
    git_run(repo.path(), &["add", "b.txt"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let (summary, description) =
        git_of(&ws, cx, id, |g| (g.summary.clone(), g.description.clone()));
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.focus(window, cx));
        window.input("Add b", cx);
        description.update(cx, |d, cx| d.focus(window, cx));
        window.input("Why b.", cx);
        window.render_frame(cx);
        window.press(COMMIT_KEY, cx);
    })
    .unwrap();
    settle(handle, cx);
    // %B is the message as stored, and log ends each record with a newline.
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%B"]).trim_end(),
        "Add b\n\nWhy b."
    );
    // Amend: the fields show the last message; off again: they clear.
    cx.update_window(handle, |_, window, cx| window.click(("git-amend", n), cx))
        .unwrap();
    settle(handle, cx);
    ws.read_with(cx, |ws, cx| {
        assert!(ws.space.git(id).unwrap().amend);
        assert_eq!(summary.read(cx).value(), "Add b");
        assert_eq!(description.read(cx).value(), "Why b.");
    });
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("Add b, amended", window, cx));
        window.render_frame(cx);
        window.click(("git-commit", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(repo.path(), &["rev-list", "--count", "HEAD"]),
        "2\n"
    );
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%s"]),
        "Add b, amended\n"
    );
    git_of(&ws, cx, id, |g| assert!(!g.amend, "off after the commit"));
}

#[gpui_kit::test]
fn without_an_author_commit_is_disabled_and_set_asks_for_one(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = git_repo();
    // The repository's own name and email would win over the global ones Set… writes.
    git_run(repo.path(), &["config", "--unset", "user.name"]);
    git_run(repo.path(), &["config", "--unset", "user.email"]);
    // The global config may name one: hide it from this test's git.
    let home = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let global = home.path().join("gitconfig");
    let wrapper = home.path().join(if cfg!(windows) {
        "git-no-global.cmd"
    } else {
        "git-no-global"
    });
    let script = if cfg!(windows) {
        format!(
            "@set HOME={}\r\n@set GIT_CONFIG_GLOBAL={}\r\n@git %*\r\n",
            home.path().display(),
            global.display()
        )
    } else {
        format!(
            "#!/bin/sh\nHOME='{}' GIT_CONFIG_GLOBAL='{}' exec git \"$@\"\n",
            home.path().display(),
            global.display()
        )
    };
    std::fs::write(&wrapper, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    write_config(
        config.path(),
        &format!(
            "[git]\nprogram = \"{}\"\n",
            wrapper.display().to_string().replace('\\', "\\\\")
        ),
    );
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    let (handle, ws) = open_with(cx, Some(config.path().to_path_buf()));
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| assert_eq!(g.author, None));
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("x", window, cx));
        window.render_frame(cx);
        assert!(window.try_find(("git-commit", n)).is_none(), "no author");
        window.click(("git-set-author", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        window.input("Grace Hopper", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    // The email's dialog opens once the name's has closed (deferred).
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "then the email");
        window.input("grace@example.com", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(
            g.author,
            Some(("Grace Hopper".to_string(), "grace@example.com".to_string()))
        )
    });
    let written = std::fs::read_to_string(&global).unwrap();
    assert!(written.contains("Grace Hopper"), "{written}");
}

#[gpui_kit::test]
fn fetch_shows_the_counts_and_pull_fast_forwards(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let remote = add_remote(repo.path());
    let other = tempfile::tempdir().unwrap();
    git_run(
        other.path(),
        &["clone", "-q", &remote.path().display().to_string(), "."],
    );
    git_run(other.path(), &["config", "user.name", "B"]);
    git_run(other.path(), &["config", "user.email", "b@example.com"]);
    std::fs::write(other.path().join("b.txt"), "b\n").unwrap();
    git_run(other.path(), &["add", "b.txt"]);
    git_run(other.path(), &["commit", "-q", "-m", "b"]);
    git_run(other.path(), &["push", "-q"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!((g.status.ahead, g.status.behind), (0, 0))
    });
    cx.update_window(handle, |_, window, cx| window.click(("git-fetch", n), cx))
        .unwrap();
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.sync, Some(crate::git_block::Sync::Fetch), "a spinner");
    });
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.sync, None);
        assert_eq!(g.status.behind, 1);
    });
    cx.update_window(handle, |_, window, cx| window.click(("git-pull", n), cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.status.behind, 0));
    assert!(repo.path().join("b.txt").is_file());
}

#[gpui_kit::test]
fn push_with_no_upstream_asks_then_publishes(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["checkout", "-q", "-b", "topic"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| assert_eq!(g.status.upstream, None));
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-push", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "asks to publish");
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.upstream.as_deref(), Some("origin/topic"));
    });
}

#[gpui_kit::test]
fn a_pull_that_cant_fast_forward_offers_merge_and_rebase(cx: &mut TestAppContext) {
    // The notification slides in: its buttons take clicks once it is at rest.
    no_motion(cx);
    let repo = committed_repo();
    let remote = add_remote(repo.path());
    let other = tempfile::tempdir().unwrap();
    git_run(
        other.path(),
        &["clone", "-q", &remote.path().display().to_string(), "."],
    );
    git_run(other.path(), &["config", "user.name", "B"]);
    git_run(other.path(), &["config", "user.email", "b@example.com"]);
    std::fs::write(other.path().join("b.txt"), "b\n").unwrap();
    git_run(other.path(), &["add", "b.txt"]);
    git_run(other.path(), &["commit", "-q", "-m", "b"]);
    git_run(other.path(), &["push", "-q"]);
    std::fs::write(repo.path().join("c.txt"), "c\n").unwrap();
    git_run(repo.path(), &["add", "c.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "c"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| window.click(("git-pull", n), cx))
        .unwrap();
    settle(handle, cx);
    assert_eq!(notification_count(handle, cx), 1);
    cx.update_window(handle, |_, window, cx| {
        window.click("git-pull-merge", cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(repo.path(), &["rev-list", "--count", "HEAD"]),
        "4\n",
        "a merge commit"
    );
    assert!(repo.path().join("b.txt").is_file());
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_failed_command_offers_details_and_a_terminal(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    no_motion(cx);
    let repo = committed_repo();
    git_run(repo.path(), &["remote", "add", "origin", "/no/such/remote"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| window.click(("git-fetch", n), cx))
        .unwrap();
    settle(handle, cx);
    assert_eq!(notification_count(handle, cx), 1);
    cx.update_window(handle, |_, window, cx| {
        window.click("git-details", cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "stderr in a dialog");
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        window.click("git-run", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let (term, rect) = ws.read_with(cx, |ws, _| {
        let (tid, _) = ws
            .space
            .terminals()
            .find(|(tid, _)| *tid != first_id_of(ws))
            .expect("a terminal beside the block");
        (tid, ws.space.canvas.get(tid).unwrap().rect)
    });
    let git = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    assert_eq!(rect.x, git.x + git.w + templum_canvas::BESIDE);
    // The quiet test shell (`cat`) echoes what was typed.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(term).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("git fetch --prune") {
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    // Typed, not run: the person reads it and presses Enter (cat would echo a run line
    // a second time).
    std::thread::sleep(Duration::from_millis(300));
    let shown = ws.read_with(cx, |ws, _| {
        let mut snap = Snapshot::default();
        snap.refresh(
            ws.space.terminal(term).unwrap().term.shared(),
            &ws.theme.palette,
        );
        snap.text()
    });
    assert_eq!(shown.matches("git fetch --prune").count(), 1, "{shown:?}");
}

#[cfg(unix)]
fn first_id_of(ws: &Workspace) -> BlockId {
    ws.space.canvas.blocks().first().map(|b| b.id).unwrap()
}

#[gpui_kit::test]
fn publish_asked_during_a_read_waits_for_it(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["checkout", "-q", "-b", "topic"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(id, window, cx);
            ws.publish_branch(id, "topic".to_string(), window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.upstream.as_deref(), Some("origin/topic"))
    });
}

// Final review I4 in the block: the spinning button cancels its command, quietly.
#[gpui_kit::test]
fn the_running_fetch_button_cancels_it(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-fetch", n), cx);
        window.render_frame(cx);
        window.click(("git-fetch", n), cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        let cancel = g.cancel.as_ref().expect("a running fetch can be cancelled");
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
    });
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.sync, None);
        assert!(g.cancel.is_none());
    });
    assert_eq!(
        notification_count(handle, cx),
        0,
        "cancelling isn't a failure"
    );
}

#[gpui_kit::test]
fn the_commit_message_being_written_comes_back_after_a_restart(cx: &mut TestAppContext) {
    let repo = git_repo();
    let data = tempfile::tempdir().unwrap();
    let json = repo.path().display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"git","rect":{{"x":0,"y":0,"w":800,"h":560}},"folder":"{json}",
                    "summary":"Fix the build","description":"It broke."}}]}}"#
        ),
    );
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    settle(handle, cx);
    let id = first_id(&ws, cx);
    let item = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, cx| {
            ws.space
                .git(id)
                .unwrap()
                .item(Rect::new(0.0, 0.0, 1.0, 1.0), cx)
        })
    };
    let saved = item(cx);
    assert_eq!(
        (saved.summary.as_str(), saved.description.as_str()),
        ("Fix the build", "It broke.")
    );
    // With Amend on, the fields hold the last commit's message: not a draft to keep.
    ws.update(cx, |ws, _| ws.space.git_mut(id).unwrap().amend = true);
    assert_eq!(item(cx).summary, "");
}

#[gpui_kit::test]
fn commit_all_changes_takes_every_change_new_files_included(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.commit_label(), "Commit all changes (2 files)")
    });
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("Everything", window, cx));
        window.render_frame(cx);
        window.click(("git-commit", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.status.entries.is_empty(), "committed")
    });
    assert_eq!(
        git_run(repo.path(), &["show", "--name-only", "--format=%s", "HEAD"]),
        "Everything\n\na.txt\nnew.txt\n"
    );
}

#[gpui_kit::test]
fn commit_and_push_from_the_commit_menu(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let remote = add_remote(repo.path());
    std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("Sent at once", window, cx));
        window.render_frame(cx);
        window.click(("git-commit-menu", n), cx);
        window.render_frame(cx);
        // Commit, Commit & Push.
        window.within("popup-menu").click(1usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(remote.path(), &["log", "-1", "--format=%s", "main"]),
        "Sent at once\n"
    );
}

#[gpui_kit::test]
fn undo_commit_takes_it_back_and_puts_its_message_in_the_fields(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let (summary, description) =
        git_of(&ws, cx, id, |g| (g.summary.clone(), g.description.clone()));
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("Fix it", window, cx));
        description.update(cx, |d, cx| d.set_value("Why.", window, cx));
        window.render_frame(cx);
        window.click(("git-commit", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%s"]),
        "Fix it\n"
    );
    cx.update_window(handle, |_, window, cx| window.click("git-undo-commit", cx))
        .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%s"]),
        "First\n",
        "taken back"
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.staged().len(), 1, "its change still staged")
    });
    ws.read_with(cx, |_, cx| {
        assert_eq!(summary.read(cx).value(), "Fix it");
        assert_eq!(description.read(cx).value(), "Why.");
    });
}

#[gpui_kit::test]
fn the_summary_counter_warns_past_72_characters(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    let long = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, cx| ws.space.git(id).unwrap().summary_too_long(cx))
    };
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(("git-summary-count", n)).is_none(),
            "nothing typed"
        );
        summary.update(cx, |s, cx| s.set_value("x".repeat(72), window, cx));
        window.render_frame(cx);
        assert!(window.try_find(("git-summary-count", n)).is_some());
    })
    .unwrap();
    assert!(!long(cx), "72 fits");
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("x".repeat(73), window, cx))
    })
    .unwrap();
    assert!(long(cx));
}

#[gpui_kit::test]
fn undo_commit_once_pushed_offers_a_revert_instead(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("Shared", window, cx));
        window.render_frame(cx);
        window.click(("git-commit", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    // A terminal pushes it before Undo commit is clicked.
    git_run(repo.path(), &["push", "-q"]);
    cx.update_window(handle, |_, window, cx| window.click("git-undo-commit", cx))
        .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.has_active_dialog(cx), "says why it reverts");
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(repo.path(), &["log", "-2", "--format=%s"]),
        "Revert \"Shared\"\nShared\n",
        "a new commit undoes it; the pushed one stays"
    );
}
