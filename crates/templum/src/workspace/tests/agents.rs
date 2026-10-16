//! CLI agents: opening, resuming and their settings.

use super::*;

#[gpui_kit::test]
fn the_agents_tab_resets_ours_and_deletes_yours(cx: &mut TestAppContext) {
    use crate::agents::Draft;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").unwrap();
        let draft = Draft {
            command: "claude --model opus".into(),
            ..Draft::of(claude)
        };
        ws.save_agent(Some("claude"), &draft, cx).unwrap();
        let mine = Draft {
            name: "Mine".into(),
            command: "mine".into(),
            resume: String::new(),
            show: true,
            ..Draft::default()
        };
        ws.save_agent(None, &mine, cx).unwrap();
    });
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-agents", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("field-agents.mine").is_some(),
            "yours is listed"
        );
        assert!(
            window.try_find("reset-agents.codex").is_none(),
            "unchanged: no ↺"
        );
        window.click("reset-agents.claude", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        let claude = ws
            .read(cx)
            .agents
            .iter()
            .find(|a| a.id == "claude")
            .unwrap()
            .clone();
        assert_eq!(claude.command, "claude", "back to Templum's");
        // Yours is last: scroll down to it.
        window.scroll(
            "settings-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-5000.))),
            cx,
        );
        window.render_frame(cx);
        window.click("edit-agent-mine", cx);
        window.render_frame(cx);
    })
    .unwrap();
    // The dialog slides in for 0.25 s first: presses meanwhile are lost.
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("delete-agent", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(ws.read(cx).agents.iter().all(|a| a.id != "mine"), "deleted");
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_saved_agent_is_resumed_in_its_block(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let folder = folder.path().display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{folder}","agent":"claude"}},
                  {{"kind":"terminal","rect":{{"x":800,"y":0,"w":700,"h":450}},"folder":"{folder}","agent":"claude","session":"aaa-1"}},
                  {{"kind":"terminal","rect":{{"x":1600,"y":0,"w":700,"h":450}},"folder":"{folder}","agent":"claude","session":"bbb-2",
                    "opened_by":"b7k2m","model":"opus","effort":"high"}},
                  {{"kind":"terminal","rect":{{"x":2400,"y":0,"w":700,"h":450}},"folder":"{folder}","agent":"claude","session":"ccc-3","model":"a b"}}]}}"#
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let ids: Vec<_> = ws.read_with(cx, |ws, _| ws.space.terminals().map(|(id, _)| id).collect());
    // Without a session, the folder's latest; with one, each block its own.
    let wanted = [
        "claude --continue",
        "claude --resume aaa-1",
        "claude --resume bbb-2 --model opus --effort high",
        "claude --resume ccc-3",
    ];
    // The test shell echoes what it is typed.
    let deadline = Instant::now() + Duration::from_secs(10);
    let shown = loop {
        let shown: Vec<String> = ids
            .iter()
            .map(|&id| {
                ws.read_with(cx, |ws, _| {
                    let mut snap = Snapshot::default();
                    snap.refresh(
                        ws.space.terminal(id).unwrap().term.shared(),
                        &ws.theme.palette,
                    );
                    snap.text()
                })
            })
            .collect();
        let once = |want: &str| shown.iter().filter(|s| s.contains(want)).count() == 1;
        if wanted.iter().all(|w| once(w)) {
            break shown;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(
        !shown.iter().any(|s| s.contains("--model a")),
        "a model it can't type safely isn't typed: {shown:?}"
    );
    ws.read_with(cx, |ws, _| {
        let kept: Vec<_> = ws
            .space
            .terminals()
            .filter(|(_, p)| p.opened_by.is_some() || p.model.is_some() || p.effort.is_some())
            .map(|(_, p)| (p.opened_by.clone(), p.model.clone(), p.effort.clone()))
            .collect();
        assert_eq!(
            kept,
            [(
                Some("b7k2m".into()),
                Some("opus".into()),
                Some("high".into())
            )]
        );
    });
}

/// The model and the effort are its agent's: a snapshot that finds that agent gone, or another
/// one there (`claude` typed by hand after the spawned Codex quit), drops them.
#[gpui_kit::test]
fn an_agent_that_left_takes_its_model_and_effort_with_it(cx: &mut TestAppContext) {
    use super::delegate::snapshot;
    use templum_terminal::Inspection;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let spawned = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.update(cx, |ws, _| {
            let pane = ws.space.terminal_mut(id).unwrap();
            (pane.opened_by, pane.model, pane.effort) = (
                Some("b7k2m".into()),
                Some("gpt-5.6-luna".into()),
                Some("high".into()),
            );
            pane.flags_for = Some("codex".into());
        })
    };
    let kept = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let p = ws.space.terminal(id).unwrap();
            (p.opened_by.clone(), p.model.clone(), p.effort.clone())
        })
    };
    let found = |agent: Option<&str>, pid: Option<u32>| Inspection {
        agent: agent.map(str::to_string),
        pid,
        ..Default::default()
    };
    spawned(&ws, cx);
    // Not started yet, then running: kept.
    snapshot(handle, &ws, cx, id, found(None, None));
    snapshot(handle, &ws, cx, id, found(Some("codex"), Some(1)));
    assert_eq!(kept(&ws, cx).1.as_deref(), Some("gpt-5.6-luna"));
    snapshot(handle, &ws, cx, id, found(Some("claude"), Some(2)));
    assert_eq!(
        kept(&ws, cx),
        (Some("b7k2m".into()), None, None),
        "still opened by it"
    );
    spawned(&ws, cx);
    snapshot(handle, &ws, cx, id, found(None, None));
    assert_eq!(kept(&ws, cx).1, None, "its agent left");
    // A spawned Codex no snapshot ever found (it never started, or quit at once), and `claude`
    // typed by hand there: not Codex's model.
    spawned(&ws, cx);
    snapshot(handle, &ws, cx, id, found(None, None));
    snapshot(handle, &ws, cx, id, found(Some("claude"), Some(3)));
    assert_eq!(kept(&ws, cx).1, None, "another agent, never seen before it");
}

#[test]
fn recent_folders_are_newest_first_without_repeats_and_few() {
    use std::path::PathBuf;
    let mut recent = Vec::new();
    for i in 0..12 {
        crate::workspace::remember(&mut recent, PathBuf::from(format!("/p{i}")));
    }
    crate::workspace::remember(&mut recent, PathBuf::from("/p8"));
    assert_eq!(recent.len(), crate::workspace::RECENT);
    assert_eq!(recent[0], PathBuf::from("/p8"));
    assert_eq!(recent[1], PathBuf::from("/p11"));
    assert_eq!(
        recent
            .iter()
            .filter(|p| *p == std::path::Path::new("/p8"))
            .count(),
        1
    );
}

#[gpui_kit::test]
fn an_agent_opens_in_its_folder_and_the_folder_becomes_recent(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let (handle, ws) = open(cx);
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (gemini, id) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let at = Point::new(0.0, 1800.0);
                let gemini = ws.launch_agent("gemini", b.path(), at, window, cx);
                ws.launch_agent("codex", b.path(), Point::new(0.0, 900.0), window, cx);
                let claude = ws.launch_agent("claude", a.path(), Point::new(40.0, 0.0), window, cx);
                (gemini.expect("a block"), claude)
            })
        })
        .unwrap();
    let id = id.expect("a block");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.recent, [a.path(), b.path()]);
        assert_eq!(
            ws.space.canvas.get(id).unwrap().rect.origin(),
            Point::new(40.0, 0.0)
        );
        assert_eq!(ws.space.focused, Some(id));
        assert_eq!(
            ws.space.terminal(id).unwrap().seen.agent.as_deref(),
            Some("claude"),
            "known at once, not at the next snapshot"
        );
    });
    assert_eq!(folder_of(&ws, id, cx), a.path().canonicalize().unwrap());
    // The test shell echoes what it is typed.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("claude") {
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    // An agent that takes a session's id is started on one, to resume that one.
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(gemini).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("gemini --session-id ") {
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn agents_are_changed_reset_added_and_deleted_from_the_settings(cx: &mut TestAppContext) {
    use crate::agents::Draft;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let frame = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap()
    };
    let mine = ws.update(cx, |ws, cx| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").unwrap();
        let mut draft = Draft::of(claude);
        draft.command = "claude --model opus".into();
        ws.save_agent(Some("claude"), &draft, cx).unwrap();
        let new = Draft {
            name: "My Agent".into(),
            command: "my-agent --tui".into(),
            resume: String::new(),
            show: true,
            ..Draft::default()
        };
        assert_eq!(
            ws.save_agent(
                None,
                &Draft {
                    command: " ".into(),
                    ..new.clone()
                },
                cx
            ),
            Err("An agent needs a command")
        );
        ws.save_agent(None, &new, cx).unwrap()
    });
    assert_eq!(mine, "my-agent");
    frame(cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        text.contains("claude --model opus") && text.contains("my-agent --tui"),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").unwrap();
        assert_eq!(
            (claude.command.as_str(), claude.changed),
            ("claude --model opus", true)
        );
        assert_eq!(ws.agents.last().unwrap().title, "My Agent");
    });
    ws.update(cx, |ws, cx| {
        ws.remove_agent("claude", cx);
        ws.remove_agent("my-agent", cx);
    });
    frame(cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        !text.contains("claude --model") && !text.contains("my-agent"),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").unwrap();
        assert_eq!((claude.command.as_str(), claude.changed), ("claude", false));
        assert!(ws.agents.iter().all(|a| a.id != "my-agent"));
    });
}

#[gpui_kit::test]
fn a_changed_resume_command_is_what_comes_back(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write_config(
        config.path(),
        "[agents.claude]\nresume = \"claude -c --fast\"\n",
    );
    let folder = tempfile::tempdir().unwrap();
    let folder = folder.path().display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{folder}","agent":"claude"}}]}}"#
        ),
    );
    let (_, ws) = open_in(
        cx,
        Some(config.path().to_path_buf()),
        Some(data.path().to_path_buf()),
    );
    let id = first_id(&ws, cx);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("claude -c --fast") {
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn an_agent_whose_folder_is_gone_isnt_resumed_elsewhere(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let data = tempfile::tempdir().unwrap();
    let here = tempfile::tempdir().unwrap();
    let json = |p: &std::path::Path| p.display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{}","agent":"claude"}},
                  {{"kind":"terminal","rect":{{"x":800,"y":0,"w":700,"h":450}},"folder":"{}","agent":"claude"}}]}}"#,
            json(&data.path().join("gone")),
            json(here.path())
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let ids: Vec<BlockId> = ws.read_with(cx, |ws, _| {
        ws.space.canvas.blocks().iter().map(|b| b.id).collect()
    });
    let shown = |id: BlockId, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        })
    };
    // Both would be typed at once: once the second shows it, the first would too.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !shown(ids[1], cx).contains("claude --continue") {
        assert!(Instant::now() < deadline, "the block in its folder resumes");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !shown(ids[0], cx).contains("claude"),
        "not in the home folder: that's another conversation"
    );
}
