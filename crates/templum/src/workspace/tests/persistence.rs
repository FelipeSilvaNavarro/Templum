//! Saving the canvas and bringing it back.

use super::*;
use crate::workspace::{MAX_COLS, MAX_ROWS};

#[gpui_kit::test]
fn a_saved_canvas_comes_back(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    // JSON strings: Windows backslashes doubled.
    let json = |p: &std::path::Path| p.display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":-100,"y":50,"zoom":1.1}},"focused":1,"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{}"}},
                  {{"kind":"terminal","rect":{{"x":800,"y":20,"w":600,"h":400}},"folder":"{}"}}]}}"#,
            json(a.path()),
            json(b.path())
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let (ids, rects, camera, focused) = ws.read_with(cx, |ws, _| {
        let blocks = ws.space.canvas.blocks();
        (
            blocks.iter().map(|b| b.id).collect::<Vec<_>>(),
            blocks.iter().map(|b| b.rect).collect::<Vec<_>>(),
            ws.space.canvas.camera,
            ws.space.focused,
        )
    });
    assert_eq!(
        rects,
        [
            Rect::new(0.0, 0.0, 700.0, 450.0),
            Rect::new(800.0, 20.0, 600.0, 400.0)
        ]
    );
    assert_eq!(camera.origin, Point::new(-100.0, 50.0));
    assert_eq!(camera.zoom(), 1.1);
    assert_eq!(focused, Some(ids[1]));
    assert_eq!(folder_of(&ws, ids[0], cx), a.path().canonicalize().unwrap());
    assert_eq!(folder_of(&ws, ids[1], cx), b.path().canonicalize().unwrap());
}

#[gpui_kit::test]
fn a_saved_canvas_with_no_blocks_opens_empty(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    write_canvas(
        data.path(),
        r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[]}"#,
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.canvas.blocks().is_empty());
        assert_eq!(ws.space.focused, None);
    });
}

#[gpui_kit::test]
fn a_folder_thats_gone_starts_at_home_and_says_so(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let gone = data
        .path()
        .join("gone")
        .display()
        .to_string()
        .replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{gone}"}}]}}"#
        ),
    );
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    assert_eq!(notification_count(handle, cx), 1);
    let id = first_id(&ws, cx);
    let home = std::env::home_dir().unwrap().canonicalize().unwrap();
    assert_eq!(folder_of(&ws, id, cx), home);
}

#[gpui_kit::test]
fn a_broken_canvas_file_is_set_aside(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    write_canvas(data.path(), "{nope");
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    assert_eq!(notification_count(handle, cx), 1);
    let bak =
        crate::session::path(data.path(), crate::session::DEFAULT_ID).with_extension("json.bak");
    assert_eq!(std::fs::read_to_string(bak).unwrap(), "{nope");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 1, "as on a first run")
    });
}

#[gpui_kit::test]
fn absurd_numbers_in_the_saved_canvas_are_made_safe(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    write_canvas(
        data.path(),
        r#"{"version":1,"camera":{"x":1e40,"y":0,"zoom":1e40},"focused":7,"items":[
              {"kind":"terminal","rect":{"x":1e40,"y":0,"w":700,"h":450}},
              {"kind":"terminal","rect":{"x":10,"y":10,"w":-5,"h":1e30}}]}"#,
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.read_with(cx, |ws, _| {
        let blocks = ws.space.canvas.blocks();
        assert_eq!(blocks.len(), 1, "the infinite one is skipped");
        let min = chrome::min_block_size(ws.cell.size);
        let max = chrome::block_size(MAX_COLS, MAX_ROWS, ws.cell.size);
        assert_eq!(blocks[0].rect, Rect::new(10.0, 10.0, min.w, max.h));
        assert_eq!(ws.space.focused, None, "index 7 doesn't exist");
        assert_eq!(
            ws.space.canvas.camera,
            Camera::default(),
            "an infinite camera isn't used"
        );
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_unreadable_canvas_file_is_never_written_over(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let data = tempfile::tempdir().unwrap();
    write_canvas(
        data.path(),
        r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[]}"#,
    );
    let path = crate::session::path(data.path(), crate::session::DEFAULT_ID);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    assert_eq!(notification_count(handle, cx), 1);
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.frozen, "its canvas won't be saved");
        assert_eq!(ws.space.canvas.blocks().len(), 1, "as on a first run");
    });
    // The rest still saves: the boards, a new workspace.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.boards.add_board();
            ws.new_space(window, cx);
        })
    })
    .unwrap();
    tick(cx);
    assert!(crate::board::path(data.path()).exists(), "boards saved");
    assert!(crate::session::path(data.path(), "w2").exists(), "w2 saved");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[]}"#,
        "never written over"
    );
}

/// One snapshot period, with the background read-and-write finished.
fn tick(cx: &mut TestAppContext) {
    cx.executor().advance_clock(crate::session::EVERY);
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_canvas_comes_back_as_it_was_left(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(Rect::new(900.0, 40.0, 600.0, 400.0), window, cx);
            let first = ws.space.canvas.blocks()[0].id;
            ws.space.canvas.get_mut(first).unwrap().rect = Rect::new(-50.0, 60.0, 700.0, 450.0);
            ws.focus_block(first, window, cx);
            ws.space.canvas.camera = Camera::at(Point::new(-40.0, 25.0), 1.25);
            ws.save_canvas_now(cx);
        })
    })
    .unwrap();
    let before = ws.read_with(cx, |ws, _| {
        let blocks = ws.space.canvas.blocks();
        let focused = blocks.iter().position(|b| Some(b.id) == ws.space.focused);
        (blocks.iter().map(|b| b.rect).collect::<Vec<_>>(), focused)
    });
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.name, "Main",
            "no index: the canvas is the first workspace"
        );
        let blocks = ws.space.canvas.blocks();
        assert_eq!(blocks.iter().map(|b| b.rect).collect::<Vec<_>>(), before.0);
        assert_eq!(
            blocks.iter().position(|b| Some(b.id) == ws.space.focused),
            before.1
        );
        assert_eq!(ws.space.canvas.camera.origin, Point::new(-40.0, 25.0));
        assert_eq!(ws.space.canvas.camera.zoom(), 1.25);
    });
}

#[gpui_kit::test]
fn snapshots_write_only_what_changed(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = crate::session::path(data.path(), crate::session::DEFAULT_ID);
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    tick(cx);
    assert!(path.exists(), "the first snapshot is written");
    std::fs::remove_file(&path).unwrap();
    tick(cx);
    assert!(!path.exists(), "nothing changed: nothing written");
    ws.update(cx, |ws, _| {
        ws.space.canvas.camera.pan_by(Point::new(30.0, 0.0))
    });
    tick(cx);
    assert!(path.exists(), "a pan is a change");
}

#[gpui_kit::test]
fn a_failing_canvas_write_says_so_once_and_recovers(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    // A file where the workspaces folder should be: every write fails.
    let blocker = data.path().join("workspaces");
    std::fs::write(&blocker, "").unwrap();
    let pan = |cx: &mut TestAppContext| {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera.pan_by(Point::new(10.0, 0.0))
        })
    };
    pan(cx);
    tick(cx);
    assert_eq!(notification_count(handle, cx), 1, "said once");
    // Still failing: retried every period without saying so again (the first hides after 5 s).
    for _ in 0..3 {
        pan(cx);
        tick(cx);
    }
    assert_eq!(notification_count(handle, cx), 0, "not again");
    std::fs::remove_file(&blocker).unwrap();
    tick(cx);
    assert!(
        crate::session::path(data.path(), crate::session::DEFAULT_ID).exists(),
        "retried"
    );
    assert_eq!(notification_count(handle, cx), 1, "and says it works again");
}

#[gpui_kit::test]
fn closing_the_window_saves_the_canvas(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.update(cx, |ws, _| {
        let first = ws.space.canvas.blocks()[0].id;
        ws.space.canvas.get_mut(first).unwrap().rect = Rect::new(-500.0, 0.0, 700.0, 450.0);
    });
    // The test's handle would keep the workspace alive.
    drop(ws);
    cx.update_window(handle, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    let text = std::fs::read_to_string(crate::session::path(
        data.path(),
        crate::session::DEFAULT_ID,
    ))
    .unwrap();
    assert!(text.contains("-500.0"), "{text}");
}

/// Quitting writes the canvas even when it looks unchanged: the file may hold an older one
/// written since (a snapshot still in flight).
#[gpui_kit::test]
fn quitting_writes_the_canvas_even_when_it_looks_unchanged(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.update(cx, |ws, cx| ws.save_canvas_now(cx));
    let path = crate::session::path(data.path(), crate::session::DEFAULT_ID);
    let ours = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[]}"#,
    )
    .unwrap();
    ws.update(cx, |ws, cx| ws.save_canvas_now(cx));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), ours);
}

#[gpui_kit::test]
fn without_a_data_folder_nothing_is_kept(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    tick(cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.saved, None));
}

#[gpui_kit::test]
fn shells_that_cant_start_leave_the_saved_canvas_alone(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let text = r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[
              {"kind":"terminal","rect":{"x":0,"y":0,"w":700,"h":450}}]}"#;
    write_canvas(data.path(), text);
    // A shell setting with a typo: no block can start.
    let broken = Some(("/nonexistent/templum-shell".to_string(), Vec::new()));
    let (_, ws) = open_as(cx, broken, None, Some(data.path().to_path_buf()));
    tick(cx);
    ws.read_with(cx, |ws, _| assert!(ws.space.canvas.blocks().is_empty()));
    assert_eq!(
        std::fs::read_to_string(crate::session::path(
            data.path(),
            crate::session::DEFAULT_ID
        ))
        .unwrap(),
        text,
        "kept for the start after the setting is fixed"
    );
}

/// A restored shell that ends as it starts couldn't start, as one whose spawn fails: its block
/// stays, marked, the saved canvas is kept for the next start, and closing the block asks
/// nothing.
#[cfg(unix)]
#[gpui_kit::test]
fn a_restored_shell_that_ends_at_once_leaves_the_saved_canvas_alone(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let folder = data.path().display().to_string().replace('\\', "\\\\");
    let text = format!(
        r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
              {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{folder}",
                "agent":"claude","session":"abc-1"}}]}}"#
    );
    write_canvas(data.path(), &text);
    // A shell whose profile ends it.
    let ends = Some(("sh".to_string(), vec!["-c".into(), "exit 1".into()]));
    let (handle, ws) = open_as(cx, ends, None, Some(data.path().to_path_buf()));
    let id = first_id(&ws, cx);
    super::terminals::until("the shell to end", handle, &ws, cx, |ws| {
        ws.space.terminal(id).is_none_or(|p| p.term.exited())
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, cx| {
        assert!(ws.space.terminal(id).is_some(), "its block stays");
        assert_eq!(
            crate::block::title(ws, &ws.space.blocks[&id], cx),
            "The shell ended"
        );
        assert!(ws.space.frozen, "its canvas won't be saved");
    });
    assert_eq!(notification_count(handle, cx), 1);
    // Its agent, typed before the shell ended, isn't running: no question.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close(id, window, cx))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.space.terminal(id).is_none()));
    tick(cx);
    let path = crate::session::path(data.path(), crate::session::DEFAULT_ID);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "kept");
}

/// A snapshot taken before a restored agent is up (a slow shell, a question of its that took the
/// typed line) keeps its saved folder, agent and session: the next start resumes it still. Once
/// a snapshot finds an agent there, what it finds is saved.
#[gpui_kit::test]
fn a_snapshot_before_the_restored_agent_is_up_keeps_it(cx: &mut TestAppContext) {
    use crate::session::{Item, Saved};
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let folder = work.path().display().to_string();
    let json = folder.replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{json}",
                    "agent":"claude","session":"abc-1"}}]}}"#
        ),
    );
    // `cat` is the shell: the agent typed into it never starts.
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let id = first_id(&ws, cx);
    ws.update(cx, |ws, _| {
        ws.space.canvas.camera.pan_by(Point::new(30.0, 0.0))
    });
    tick(cx);
    let path = crate::session::path(data.path(), crate::session::DEFAULT_ID);
    let saved: Saved = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let Item::Terminal(t) = &saved.items[0] else {
        panic!("{saved:?}")
    };
    assert_eq!(saved.camera.x, -30.0, "written");
    assert_eq!(
        (
            t.folder.as_deref(),
            t.agent.as_deref(),
            t.session.as_deref()
        ),
        (Some(folder.as_str()), Some("claude"), Some("abc-1"))
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let found = templum_terminal::Inspection {
                folder: Some(work.path().to_path_buf()),
                agent: Some("claude".into()),
                pid: Some(42),
                ..Default::default()
            };
            let space = ws.space.id.clone();
            ws.snapshot_done(
                vec![(space, vec![(id, found)], saved.clone(), None)],
                None,
                None,
                None,
                window,
                cx,
            );
            assert!(ws.space.terminal(id).unwrap().resuming.is_none());
        })
    })
    .unwrap();
}

/// A snapshot's read of the processes tells each terminal its folder and its agent, with the
/// agent's process: what the menus, the Git block and the usage read.
#[gpui_kit::test]
fn a_snapshot_tells_each_terminal_its_folder_and_agent(cx: &mut TestAppContext) {
    use crate::session::{Saved, SavedCamera};
    use templum_terminal::Inspection;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let saved = Saved {
                version: 1,
                camera: SavedCamera {
                    x: 0.0,
                    y: 0.0,
                    zoom: 1.0,
                },
                items: Vec::new(),
                focused: None,
                recent: Vec::new(),
                ink: Vec::new(),
                grid: None,
                wires: Vec::new(),
                git: None,
            };
            let space = ws.space.id.clone();
            let found = Inspection {
                folder: Some("/srv/app".into()),
                agent: Some("claude".into()),
                pid: Some(42),
                started: Some(1_700_000_000),
                session: None,
            };
            ws.snapshot_done(
                vec![(
                    space.clone(),
                    vec![(id, found.clone())],
                    saved.clone(),
                    None,
                )],
                None,
                None,
                None,
                window,
                cx,
            );
            assert_eq!(ws.space.terminal(id).unwrap().seen, found);
            // A folder the next one couldn't read keeps the one known; the agent left.
            ws.snapshot_done(
                vec![(space, vec![(id, Inspection::default())], saved, None)],
                None,
                None,
                None,
                window,
                cx,
            );
            let seen = &ws.space.terminal(id).unwrap().seen;
            assert_eq!(
                seen.folder.as_deref(),
                Some(std::path::Path::new("/srv/app"))
            );
            assert_eq!((&seen.agent, seen.pid, seen.started), (&None, None, None));
        })
    })
    .unwrap();
}

/// A workspace frozen (its file couldn't be read) is never written, but its terminals are still
/// read: their folder and agent stay current.
#[gpui_kit::test]
fn a_frozen_workspaces_terminals_are_still_read(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (_handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let id = first_id(&ws, cx);
    ws.update(cx, |ws, _| {
        ws.space.frozen = true;
        ws.space.terminal_mut(id).unwrap().seen.agent = Some("gone".into());
    });
    tick(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.terminal(id).unwrap().seen.agent,
            None,
            "read again"
        );
    });
    let file = crate::session::path(data.path(), crate::session::DEFAULT_ID);
    assert!(!file.exists(), "not written");
}
