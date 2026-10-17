//! The Git block: setting it up and reading the repository's status.

use super::*;

#[cfg(target_os = "macos")]
const NEW_GIT: &str = "cmd-shift-g";
#[cfg(not(target_os = "macos"))]
const NEW_GIT: &str = "ctrl-shift-g";

#[gpui_kit::test]
fn a_git_block_on_a_fresh_repository_lists_an_untracked_file(cx: &mut TestAppContext) {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, |g| {
        assert!(g.repo().is_some(), "found");
        assert_eq!(g.status.branch.as_deref(), Some("main"));
        assert_eq!(g.status.entries.len(), 1);
        assert_eq!(g.status.entries[0].path, "a.txt");
        assert_eq!(
            g.status.entries[0].unstaged,
            Some(templum_git::Change::Untracked)
        );
        assert_eq!(g.author.as_ref().map(|a| a.0.as_str()), Some("Ada"));
    });
    cx.update_window(handle, |_, window, _| {
        assert!(
            window
                .try_find(crate::git_block::row_id("files", 0))
                .is_some()
        );
        assert!(
            window
                .try_find(crate::git_block::row_id("files", 1))
                .is_none()
        );
        assert!(window.try_find("branch").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_folder_outside_a_repository_offers_initialize(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, dir.path());
    git_of(&ws, cx, |g| {
        assert!(matches!(g.found, crate::git_block::Found::NotARepo))
    });
    cx.update_window(handle, |_, window, cx| window.click("init", cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, |g| {
        assert!(g.repo().is_some(), "initialized");
        assert_eq!(g.status.oid, None, "no commits");
    });
    assert!(dir.path().join(".git").is_dir());
}

#[gpui_kit::test]
fn without_git_the_block_says_so_and_tries_again_when_asked(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    write_config(config.path(), "[git]\nprogram = \"templum-no-such-git\"\n");
    let repo = git_repo();
    let (handle, ws) = open_with(cx, Some(config.path().to_path_buf()));
    open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, |g| {
        assert!(matches!(&g.found, crate::git_block::Found::NoGit(e) if e.missing))
    });
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("retry").is_some());
    })
    .unwrap();
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("git", "program", "git"),
            |c| c.git.program = "git".to_string(),
            cx,
        )
    });
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| window.click("retry", cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, |g| assert!(g.repo().is_some()));
}

#[gpui_kit::test]
fn the_status_is_read_again_every_two_seconds_while_the_popup_shows(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, |g| g.reads);
    std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, |g| {
        assert_eq!(g.reads, reads + 1);
        assert_eq!(g.status.entries[0].path, "b.txt");
    });
    // Closed: not read.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close_git(window, cx))
    })
    .unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, |g| assert_eq!(g.reads, reads + 1));
}

#[gpui_kit::test]
fn a_refresh_asked_while_one_runs_waits_and_runs_once(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, |g| g.reads);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            for _ in 0..3 {
                ws.refresh_git(window, cx);
            }
        })
    })
    .unwrap();
    git_of(&ws, cx, |g| assert!(g.busy && g.pending));
    settle(handle, cx);
    git_of(&ws, cx, |g| {
        assert_eq!(g.reads, reads + 2, "the first, then one for the two asked");
        assert!(!g.busy && !g.pending);
    });
}

#[gpui_kit::test]
fn a_deleted_folder_shows_after_the_poll_not_on_the_next_frame(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    std::fs::remove_dir_all(repo.path()).unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("choose-folder").is_none(),
            "drawing doesn't read the disk"
        );
    })
    .unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    cx.update_window(handle, |_, window, _| {
        window.find("choose-folder");
    })
    .unwrap();
}

// A canvas saved by an older Templum, with a Git block between two blocks: nothing is lost, the
// other blocks come back and the Git block becomes the workspace's Git, on its tab.
#[gpui_kit::test]
fn an_old_git_block_becomes_the_workspaces_git_and_the_canvas_keeps_the_rest(
    cx: &mut TestAppContext,
) {
    no_motion(cx);
    let repo = git_repo();
    let data = tempfile::tempdir().unwrap();
    let json = repo.path().display().to_string().replace('\\', "\\\\");
    let rect = r#""rect":{"x":0,"y":0,"w":800,"h":560}"#;
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"focused":2,"items":[
                  {{"kind":"note",{rect}}},
                  {{"kind":"git",{rect},"folder":"{json}","tab":"branches"}},
                  {{"kind":"kanban",{rect}}}]}}"#
        ),
    );
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    settle(handle, cx);
    ws.read_with(cx, |ws, _| {
        let kinds: Vec<_> = ws
            .space
            .canvas
            .blocks()
            .iter()
            .map(|b| &ws.space.blocks[&b.id])
            .collect();
        assert_eq!(kinds.len(), 2, "the note and the kanban block");
        assert!(kinds[0].note().is_some() && kinds[1].kanban().is_some());
        let top = ws.space.canvas.blocks()[1].id;
        assert_eq!(
            ws.space.focused,
            Some(top),
            "the focus by its index, past the Git block"
        );
        assert!(!ws.git_open, "closed until asked for");
    });
    git_of(&ws, cx, |g| {
        assert!(g.repo().is_some());
        assert_eq!(g.tab, crate::git_block::Tab::Branches);
    });
    let item = ws.read_with(cx, |ws, cx| ws.space.git().unwrap().saved(cx));
    assert_eq!(item.tab, "branches");
    cx.update_window(handle, |_, window, cx| {
        window.press(NEW_GIT, cx);
        window.render_frame(cx);
        assert!(
            window.try_find("git-popup").is_some(),
            "its shortcut opens it"
        );
    })
    .unwrap();
}

/// Shell integration's `finished` count: a command run in the repository from a terminal of
/// this workspace refreshes the block.
#[cfg(unix)]
#[gpui_kit::test]
fn a_terminal_finishing_a_command_in_the_repository_refreshes_the_block(cx: &mut TestAppContext) {
    let ps0 = std::process::Command::new("bash")
        .args([
            "-c",
            "(( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 404 ))",
        ])
        .status()
        .is_ok_and(|s| s.success());
    use std::time::{Duration, Instant};
    if !ps0 {
        eprintln!("skipped: this bash doesn't report commands");
        return;
    }
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let data = tempfile::tempdir().unwrap();
    let bash = Some(("bash".to_string(), Vec::new()));
    let (handle, ws) = open_as(cx, bash, None, Some(data.path().to_path_buf()));
    let term = first_id(&ws, cx);
    open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, |g| g.reads);
    let line = format!("cd '{}' && git add a.txt\r", repo.path().display());
    ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(term)
            .unwrap()
            .term
            .write(line.into_bytes())
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while ws.read_with(cx, |ws, _| {
        ws.space.terminal(term).unwrap().term.status().finished
    }) < 1
    {
        assert!(Instant::now() < deadline, "the command never ended");
        std::thread::sleep(Duration::from_millis(50));
    }
    settle(handle, cx);
    git_of(&ws, cx, |g| {
        assert!(g.reads > reads, "refreshed by the terminal");
        assert_eq!(
            g.status.entries[0].staged,
            Some(templum_git::Change::Added),
            "{:?}",
            g.status.entries
        );
    });
}

#[gpui_kit::test]
fn the_timer_skips_a_block_whose_read_is_still_out(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(window, cx);
            ws.poll_git(window, cx);
        })
    })
    .unwrap();
    git_of(&ws, cx, |g| {
        assert!(g.busy);
        assert!(!g.pending, "a slow repository never piles reads up");
    });
}

#[gpui_kit::test]
fn changing_the_gits_folder_forgets_the_old_repository(cx: &mut TestAppContext) {
    no_motion(cx);
    let old = committed_repo();
    std::fs::write(old.path().join("a.txt"), "zero\none\n").unwrap();
    let new = committed_repo();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, old.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.select_git_row(
                crate::git_block::List::Files,
                "a.txt".to_string(),
                window,
                cx,
            );
            ws.space.recent = vec![new.path().to_path_buf()];
        })
    })
    .unwrap();
    settle(handle, cx);
    let stale = ws.read_with(cx, |ws, _| ws.here());
    cx.update_window(handle, |_, window, cx| {
        window.click("git-folder", cx);
        window.render_frame(cx);
        // The recent folder, —, Choose another folder…, Refresh, Open a terminal here.
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    // What a command started before the change returns is dropped.
    ws.update(cx, |ws, _| assert!(ws.git_at(&stale).is_none()));
    // Nor does a notification's button about the old repository act on the new one.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(!ws.show_git(&stale, window, cx)))
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, |g| {
        assert_eq!(g.folder, new.path());
        assert!(g.status.entries.is_empty(), "{:?}", g.status.entries);
        assert!(g.selected.is_none(), "the old repository's row");
    });
    git_of(&ws, cx, |g| {
        assert!(g.panel.is_empty(), "the old repository's diff")
    });
}

#[gpui_kit::test]
fn the_git_popup_opens_terminals_in_its_repository_and_lists_them(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        window.click("git-folder", cx);
        window.render_frame(cx);
        // Choose another folder…, Refresh, Open a terminal here.
        window.within("popup-menu").click(2usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let term = ws.read_with(cx, |ws, _| {
        let term = ws.space.focused.unwrap();
        assert!(ws.space.terminal(term).is_some() && term != first);
        term
    });
    assert_eq!(
        folder_of(&ws, term, cx),
        repo.path().canonicalize().unwrap()
    );
    // The top bar counts it (not the first terminal, elsewhere); choosing it shows it.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.space.canvas.camera = Camera::default();
            ws.space.flight = None;
            ws.open_git(window, cx);
        });
        window.render_frame(cx);
        window.click("git-terminals", cx);
        window.render_frame(cx);
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.focused, Some(term));
        let root = ws.space.git().unwrap().repo().unwrap().root.clone();
        assert_eq!(ws.terminal_list(Some(&root)).len(), 1);
    });
}
