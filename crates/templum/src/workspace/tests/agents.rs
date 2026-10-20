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
    // The test shell echoes what it is typed, wrapped at its width (cmd's long prompt splits
    // `--model opus`): its lines joined.
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
                    snap.text().replace('\n', "")
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

/// Claude Code 2, an account of Claude Code on Templum's folder for it.
pub(super) const CLAUDE2: &str = "[agents.claude2]\nof = \"claude\"\nname = \"Claude Code 2\"\n";

/// `ws`'s agents as config.toml's `[agents]` `text` makes them, on its data folder.
pub(super) fn with_agents(ws: &Entity<Workspace>, cx: &mut TestAppContext, text: &str) {
    ws.update(cx, |ws, _| {
        let config = crate::config::parse(text).unwrap();
        ws.agents = crate::agents::effective(&config.agents, &[], ws.data_dir.as_deref());
        ws.config.agents = config.agents;
    });
}

/// Opens the agent `id` in a new terminal in `folder`.
pub(super) fn launch(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: &str,
    folder: &std::path::Path,
) -> BlockId {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.launch_agent(id, folder, Point::new(0.0, 900.0), window, cx)
        })
    })
    .unwrap()
    .expect("a block")
}

/// Made-up shells (docs/specs/shells.md, A shell for each agent): Quiet runs the tests' quiet
/// program; WSL is a path that never runs. Returns Quiet. `pub(super)`: `tests/spawn.rs` uses it
/// too.
pub(super) fn agent_shells(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> templum_terminal::ShellOption {
    use templum_terminal::ShellOption;
    let (program, args) = quiet();
    let quiet_shell = ShellOption {
        label: "Quiet".into(),
        program,
        args,
    };
    let wsl = ShellOption {
        label: "WSL".into(),
        program: "C:/Windows/System32/wsl.exe".into(),
        args: vec![],
    };
    ws.update(cx, |ws, _| ws.shells = vec![quiet_shell.clone(), wsl]);
    quiet_shell
}

#[gpui_kit::test]
fn an_agent_opens_in_its_own_shell_and_its_block_keeps_it(cx: &mut TestAppContext) {
    let folder = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx);
    let quiet_shell = agent_shells(&ws, cx);
    with_agents(
        &ws,
        cx,
        "[agents.claude]\nshell = \"Quiet\"\n[agents.codex]\nshell = \"PowerShell 7\"\n",
    );
    let claude = launch(handle, &ws, cx, "claude", folder.path());
    let codex = launch(handle, &ws, cx, "codex", folder.path());
    let gemini = launch(handle, &ws, cx, "gemini", folder.path());
    let rect = templum_canvas::Rect::new(0.0, 0.0, 700.0, 450.0);
    let saved = ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(claude).unwrap();
        assert_eq!(pane.shell, Some(quiet_shell.clone()));
        let item = pane.item(rect, String::new());
        assert_eq!(item.shell.as_deref(), Some("Quiet"));
        let lost = ws.space.terminal(codex).unwrap();
        assert_eq!(lost.shell, None, "a label not found here: the default");
        assert_eq!(ws.space.terminal(gemini).unwrap().shell, None);
        item
    });
    // The setting is gone by the time the block comes back: it runs its own saved shell.
    with_agents(&ws, cx, "");
    let back = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.add_saved(&crate::session::Item::Terminal(saved), window, cx)
            })
        })
        .unwrap()
        .expect("a block");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(back).unwrap().shell, Some(quiet_shell));
    });
}

#[gpui_kit::test]
fn an_account_never_opens_in_wsl(cx: &mut TestAppContext) {
    let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let quiet_shell = agent_shells(&ws, cx);
    with_agents(
        &ws,
        cx,
        &format!(
            "{CLAUDE2}shell = \"WSL\"\n[agents.claude3]\nof = \"claude\"\nshell = \"Quiet\"\n"
        ),
    );
    ws.read_with(cx, |ws, _| {
        let claude2 = ws.agents.iter().find(|a| a.id == "claude2").unwrap();
        assert_eq!(
            claude2.shell.as_deref(),
            Some("WSL"),
            "in the file as written"
        );
        assert_eq!(ws.agent_shell(claude2), None, "never WSL for an account");
    });
    let claude2 = launch(handle, &ws, cx, "claude2", folder.path());
    let claude3 = launch(handle, &ws, cx, "claude3", folder.path());
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(claude2).unwrap();
        assert_eq!(
            (pane.shell.as_ref(), pane.account.as_deref()),
            (None, Some("claude2"))
        );
        assert_eq!(
            ws.space.terminal(claude3).unwrap().shell,
            Some(quiet_shell.clone())
        );
    });
    let login = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.log_in("claude3", window, cx))
        })
        .unwrap()
        .expect("a block");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(login).unwrap().shell, Some(quiet_shell));
    });
}

/// None of Templum's words cross into WSL (docs/specs/shells.md, What works in WSL).
#[cfg(unix)]
#[gpui_kit::test]
fn an_agent_set_to_wsl_gets_none_of_templums_words(cx: &mut TestAppContext) {
    use templum_terminal::ShellOption;
    let (data, folder, bin) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    // A shell named `wsl` that only echoes what is typed.
    let wsl = bin.path().join("wsl");
    std::os::unix::fs::symlink(templum_terminal::locate("cat").unwrap(), &wsl).unwrap();
    let (handle, ws) = open_as(cx, Some(quiet()), None, Some(data.path().to_path_buf()));
    ws.update(cx, |ws, _| {
        ws.shells = vec![ShellOption {
            label: "WSL".into(),
            program: wsl.display().to_string(),
            args: vec![],
        }]
    });
    with_agents(&ws, cx, "[agents.claude]\nshell = \"WSL\"\n");
    let id = launch(handle, &ws, cx, "claude", folder.path());
    let shown = super::bus::screen_with(&ws, cx, id, "claude");
    assert!(!shown.contains("--settings"), "{shown}");
}

#[gpui_kit::test]
fn an_agent_that_runs_in_wsl_counts_as_installed(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    agent_shells(&ws, cx);
    with_agents(
        &ws,
        cx,
        "[agents.mine]\nname = \"Mine\"\ncommand = \"templum-no-such-agent\"\nshell = \"WSL\"\n\
         [agents.theirs]\nname = \"Theirs\"\ncommand = \"templum-no-such-agent\"\n\
         [agents.quiet]\nname = \"Quiet\"\ncommand = \"templum-no-such-agent\"\nshell = \"Quiet\"\n",
    );
    let listed = |ws: &Workspace, id: &str| {
        ws.agents_for_menu()
            .into_iter()
            .find(|(a, _)| a.id == id)
            .unwrap()
            .1
    };
    ws.read_with(cx, |ws, _| {
        assert!(listed(ws, "mine"));
        assert!(!listed(ws, "theirs"));
    });
    ws.update(cx, |ws, _| {
        ws.config.shell.program = Some("C:/Windows/System32/wsl.exe".into());
    });
    ws.read_with(cx, |ws, _| {
        assert!(
            listed(ws, "theirs"),
            "Settings ▸ Shell is WSL: every agent runs there"
        );
        assert!(!listed(ws, "quiet"), "except one with a shell of its own");
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_welcome(false, window, cx))
    })
    .unwrap();
    // It reads the shells of this computer again, so only what Settings ▸ Shell says counts here.
    ws.read_with(cx, |ws, _| {
        let installed = &ws.welcome.as_ref().unwrap().installed;
        assert!(installed.iter().any(|id| id == "theirs"), "{installed:?}");
    });
}

#[gpui_kit::test]
fn the_agent_dialog_sets_its_shell_and_the_switch_keeps_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    // Opening Settings read this computer's shells: the made-up ones come after.
    agent_shells(&ws, cx);
    ws.update(cx, |ws, _| {
        ws.config.shell.program = None;
        ws.default_shell = "zsh".into();
    });
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-agents", cx);
        window.render_frame(cx);
    })
    .unwrap();
    // The dialog slides in for 0.25 s first: presses meanwhile are lost.
    let edit = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.click("edit-agent-claude", cx);
            window.render_frame(cx);
        })
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
    };
    // Picks the option at `index` of the dialog's Shell list; the note about WSL is there or not.
    let pick = |cx: &mut TestAppContext, index: usize, note: bool| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.within("dialog").click("menu-agent-shell", cx);
            window.render_frame(cx);
            window.within("popup-menu").click(index, cx);
            window.render_frame(cx);
            let shown = window
                .within("dialog")
                .try_find("agent-shell-wsl")
                .is_some();
            assert_eq!(shown, note, "the note under the list, only for WSL");
        })
        .unwrap();
    };
    let ok = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.within("dialog").click("ok", cx);
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .unwrap();
    };
    edit(cx);
    pick(cx, 2, true); // Default (zsh), Quiet, WSL
    pick(cx, 1, false);
    ok(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.agents["claude"].shell.as_deref(), Some("Quiet"));
    });
    // The row's switch saves the whole entry: the shell stays.
    cx.update_window(handle, |_, window, cx| {
        window.click("show-agent-claude", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let claude = &ws.config.agents["claude"];
        assert_eq!(claude.show, Some(false));
        assert_eq!(claude.shell.as_deref(), Some("Quiet"));
    });
    edit(cx);
    pick(cx, 0, false);
    ok(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.agents["claude"].shell, None, "Default: no key");
    });
    // A label this computer lacks opens the default, WSL when Settings ▸ Shell is: the note says so.
    with_agents(&ws, cx, "[agents.claude]\nshell = \"PowerShell 7\"\n");
    ws.update(cx, |ws, _| {
        ws.config.shell.program = Some("C:/Windows/System32/wsl.exe".into());
    });
    edit(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let note = window
            .within("dialog")
            .try_find("agent-shell-wsl")
            .is_some();
        assert!(note, "the agent opens in the default, which is WSL");
    })
    .unwrap();
}

/// A shell list read before the login shell's PATH arrived is read again when an agent opens: the
/// shell it is set to may be one only that PATH has (a menu or Settings may never have been
/// opened). It runs the machine's own list, so the shell is one every machine lists; a machine
/// whose list lacks it skips the check.
#[gpui_kit::test]
fn an_agent_reads_an_early_shell_list_again(cx: &mut TestAppContext) {
    let label = if cfg!(windows) {
        "Command Prompt"
    } else {
        "sh"
    };
    if !templum_terminal::available_shells()
        .iter()
        .any(|s| s.label == label)
    {
        eprintln!("skipped: this computer lists no {label}");
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx);
    with_agents(&ws, cx, &format!("[agents.claude]\nshell = \"{label}\"\n"));
    ws.update(cx, |ws, _| {
        // Not the real agent in the machine's shell.
        ws.agents
            .iter_mut()
            .find(|a| a.id == "claude")
            .unwrap()
            .command = "cat".into();
        ws.shells = Vec::new();
        ws.shells_early = true;
    });
    let id = launch(handle, &ws, cx, "claude", folder.path());
    ws.read_with(cx, |ws, _| {
        let shell = ws.space.terminal(id).unwrap().shell.as_ref();
        assert_eq!(shell.map(|s| s.label.as_str()), Some(label));
        assert!(!ws.shells_early);
    });
}

/// OpenCode's and Kilo Code's server variable is judged by the shell the block runs: none in WSL,
/// for a block Templum opens and for one restored, pasted or undone (`add_saved`) with its own
/// saved shell. Both shells list the variables they were given.
#[cfg(unix)]
#[gpui_kit::test]
fn an_opencode_in_wsl_gets_no_server_variable(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    use templum_terminal::ShellOption;
    let (data, folder, bin) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let script = "#!/bin/sh\nenv | grep -E '^(OPENCODE|KILO)_CONFIG_CONTENT=' | cut -c1-40\n\
                  echo env-listed\nexec cat\n";
    let (plain, wsl) = (bin.path().join("plain"), bin.path().join("wsl"));
    for path in [&plain, &wsl] {
        std::fs::write(path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let (handle, ws) = open_as(
        cx,
        Some((plain.display().to_string(), vec![])),
        None,
        Some(data.path().to_path_buf()),
    );
    ws.update(cx, |ws, _| {
        ws.shells = vec![ShellOption {
            label: "WSL".into(),
            program: wsl.display().to_string(),
            args: vec![],
        }]
    });
    with_agents(&ws, cx, "[agents.opencode]\nshell = \"WSL\"\n");
    // Kilo Code, in the default shell, is the control: the variable is given there.
    let kilo = launch(handle, &ws, cx, "kilo", folder.path());
    let given = super::bus::screen_with(&ws, cx, kilo, "env-listed");
    assert!(given.contains("KILO_CONFIG_CONTENT="), "{given}");
    let opened = launch(handle, &ws, cx, "opencode", folder.path());
    let shown = super::bus::screen_with(&ws, cx, opened, "env-listed");
    assert!(!shown.contains("OPENCODE_CONFIG_CONTENT"), "{shown}");
    // A block saved with the shell: restored, it is judged by that one, not the setting now.
    let restore = |shell: Option<&str>, cx: &mut TestAppContext| {
        let item: crate::session::Item = serde_json::from_value(serde_json::json!({
            "kind": "terminal",
            "rect": { "x": 0.0, "y": 900.0, "w": 700.0, "h": 450.0 },
            "folder": folder.path(),
            "agent": "opencode",
            "shell": shell,
        }))
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.add_saved(&item, window, cx))
        })
        .unwrap()
        .expect("a block")
    };
    let saved = restore(Some("WSL"), cx);
    let shown = super::bus::screen_with(&ws, cx, saved, "env-listed");
    assert!(!shown.contains("OPENCODE_CONFIG_CONTENT"), "{shown}");
    let plain = restore(None, cx);
    let given = super::bus::screen_with(&ws, cx, plain, "env-listed");
    assert!(given.contains("OPENCODE_CONFIG_CONTENT="), "{given}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_account_opens_with_its_folder_in_its_shell(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let script = "echo \"dir=$CLAUDE_CONFIG_DIR;\"; exec cat";
    let command = ("sh".to_string(), vec!["-c".to_string(), script.to_string()]);
    let (handle, ws) = open_as(cx, Some(command), None, Some(data.path().to_path_buf()));
    with_agents(&ws, cx, CLAUDE2);
    let id = launch(handle, &ws, cx, "claude2", folder.path());
    let account = data.path().join("accounts").join("claude2");
    super::bus::screen_with(&ws, cx, id, &format!("dir={};", account.display()));
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(id).unwrap();
        assert_eq!(pane.account.as_deref(), Some("claude2"));
        assert_eq!(pane.agent(), Some("claude2"));
        assert_eq!(ws.agent_name(pane), Some("Claude Code 2"));
    });
    assert!(account.is_dir(), "made at its start");
    // Claude Code itself opens on its own folder: no variable of Templum's.
    let claude = launch(handle, &ws, cx, "claude", folder.path());
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(claude).unwrap().account, None);
    });
}

/// What a snapshot finds in an account's block: `agent`, as process `pid`.
pub(super) fn found(agent: Option<&str>, pid: Option<u32>) -> templum_terminal::Inspection {
    templum_terminal::Inspection {
        agent: agent.map(str::to_string),
        pid,
        ..Default::default()
    }
}

/// The snapshot sees `inspection` in the block `id`.
pub(super) fn see(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    inspection: templum_terminal::Inspection,
) {
    ws.update(cx, |ws, _| {
        let homes = crate::agents::homes(&ws.agents);
        ws.space.see(vec![(id, inspection)], &homes);
    });
}

#[gpui_kit::test]
fn the_snapshot_names_an_accounts_agent(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    with_agents(&ws, cx, CLAUDE2);
    let id = launch(handle, &ws, cx, "claude2", folder.path());
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(id).unwrap();
        (pane.model, pane.flags_for) = (Some("opus".into()), Some("claude2".into()));
    });
    see(&ws, cx, id, found(Some("claude"), Some(7)));
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(id).unwrap();
        assert_eq!(pane.agent(), Some("claude2"));
        assert_eq!(
            pane.model.as_deref(),
            Some("opus"),
            "the model is the account's"
        );
    });
    // Another agent typed there is that agent, on its own folder.
    see(&ws, cx, id, found(Some("codex"), Some(8)));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(id).unwrap().agent(), Some("codex"));
    });
}

#[gpui_kit::test]
fn claude_typed_again_in_its_block_is_the_account(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    with_agents(&ws, cx, CLAUDE2);
    let id = launch(handle, &ws, cx, "claude2", folder.path());
    see(&ws, cx, id, found(Some("claude"), Some(7)));
    // It quits: its shell alone, which still has the account's variable.
    see(&ws, cx, id, found(None, None));
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(id).unwrap();
        assert_eq!(pane.seen.agent, None);
        assert_eq!(pane.account.as_deref(), Some("claude2"));
    });
    see(&ws, cx, id, found(Some("claude"), Some(9)));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(id).unwrap().agent(), Some("claude2"));
    });
}
