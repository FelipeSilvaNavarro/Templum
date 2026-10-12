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
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| {
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
                .try_find(crate::git_block::row_id(id, "unstaged", 0))
                .is_some()
        );
        assert!(
            window
                .try_find(crate::git_block::row_id(id, "staged", 0))
                .is_none()
        );
        assert!(window.try_find(("branch", id.0 as usize)).is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_folder_outside_a_repository_offers_initialize(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, dir.path());
    git_of(&ws, cx, id, |g| {
        assert!(matches!(g.found, crate::git_block::Found::NotARepo))
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("init", id.0 as usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
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
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| {
        assert!(matches!(&g.found, crate::git_block::Found::NoGit(e) if e.missing))
    });
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find(("retry", id.0 as usize)).is_some());
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
    cx.update_window(handle, |_, window, cx| {
        window.click(("retry", id.0 as usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert!(g.repo().is_some()));
}

#[gpui_kit::test]
fn the_status_is_read_again_every_two_seconds_while_on_screen(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, id, |g| g.reads);
    std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.reads, reads + 1);
        assert_eq!(g.status.entries[0].path, "b.txt");
    });
    // Off screen: not read.
    ws.update(cx, |ws, _| {
        ws.space.canvas.camera = Camera::at(Point::new(100_000.0, 100_000.0), 1.0);
        ws.space.view = ws.space.canvas.camera;
    });
    frame(handle, cx);
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.reads, reads + 1));
}

#[gpui_kit::test]
fn a_refresh_asked_while_one_runs_waits_and_runs_once(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, id, |g| g.reads);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            for _ in 0..3 {
                ws.refresh_git(id, window, cx);
            }
        })
    })
    .unwrap();
    git_of(&ws, cx, id, |g| assert!(g.busy && g.pending));
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.reads, reads + 2, "the first, then one for the two asked");
        assert!(!g.busy && !g.pending);
    });
}

#[gpui_kit::test]
fn a_result_for_a_closed_block_is_dropped(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(id, window, cx);
            ws.close(id, window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    ws.read_with(cx, |ws, _| assert!(ws.space.git(id).is_none()));
}

#[gpui_kit::test]
fn new_git_asks_for_a_folder_and_a_saved_block_comes_back_on_its_tab(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = git_repo();
    let data = tempfile::tempdir().unwrap();
    let json = repo.path().display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"git","rect":{{"x":0,"y":0,"w":800,"h":560}},"folder":"{json}","tab":"branches"}}]}}"#
        ),
    );
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    settle(handle, cx);
    let id = first_id(&ws, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.repo().is_some());
        assert_eq!(g.tab, crate::git_block::Tab::Branches);
    });
    let item = ws.read_with(cx, |ws, _| {
        ws.space
            .git(id)
            .unwrap()
            .item(Rect::new(0.0, 0.0, 1.0, 1.0))
    });
    assert_eq!(item.tab, "branches");
    cx.update_window(handle, |_, window, cx| {
        window.press(NEW_GIT, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "the folder dialog");
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
    let id = open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, id, |g| g.reads);
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
    git_of(&ws, cx, id, |g| {
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
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(id, window, cx);
            ws.poll_git(window, cx);
        })
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(g.busy);
        assert!(!g.pending, "a slow repository never piles reads up");
    });
}
