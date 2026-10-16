//! `spawn_agent`: an agent opened beside the caller (docs/specs/connections.md, Spawning). `cat`
//! stands in for the agents: every Unix has it, and it echoes what it is typed. Unix only (`mod.rs`).

use std::time::Duration;

use serde_json::json;
use templum_canvas::Wire;

use super::bus::{
    after, answer_soon, answered, call, call_later, event, me, result_of, screen_until, screen_with,
};
use super::delegate::{found, prints, snapshot};
use super::*;

/// The prompt "say hi" went with its Enter: the terminal echoed it and `cat` printed it back,
/// maybe after its echo of the command typed before it (`cat` may start reading late).
#[track_caller]
fn said_twice(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) {
    screen_until(ws, cx, id, |shown| shown.matches("say hi").count() >= 2);
}

/// The startup terminal at (40, 100), 400 × 300, the camera at 100% on the origin, and Claude Code
/// and Codex started as `cat` in front of the new terminals' shells (`agent_cat`).
fn spawner(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, BlockId) {
    let (handle, ws) = open(cx);
    let mine = first_id(&ws, cx);
    ws.update(cx, |ws, _| {
        // Its agents' input boxes bracket pastes.
        ws.command = Some(jobs("printf '\\033[?2004h'; cat; true"));
        ws.space.pristine = false;
        ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), 1.0);
        ws.space.canvas.get_mut(mine).unwrap().rect = Rect::new(40.0, 100.0, 400.0, 300.0);
        ws.space.focused = Some(mine);
        for agent in &mut ws.agents {
            if agent.id == "claude" || agent.id == "codex" {
                agent.command = "cat".into();
            }
        }
    });
    (handle, ws, mine)
}

/// The block spawned beside `mine`: the other end of its newest wire.
fn spawned(ws: &Entity<Workspace>, cx: &mut TestAppContext, mine: BlockId) -> BlockId {
    ws.read_with(cx, |ws, _| {
        let wire = ws
            .space
            .wires
            .iter()
            .rfind(|w| w.touches(mine))
            .expect("wired to the caller");
        if wire.a == mine { wire.b } else { wire.a }
    })
}

/// The Enter a typed prompt waits for.
fn enter(cx: &mut TestAppContext) {
    after(
        cx,
        crate::workspace::delegate::ENTER_AFTER.as_millis() as u64,
    );
}

#[gpui_kit::test]
fn spawn_agent_opens_a_wired_agent_beside_the_caller(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    // The person selected a wire.
    let wire = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(Rect::new(40.0, 500.0, 400.0, 300.0), window, cx);
                let other = ws.space.canvas.top().unwrap();
                assert!(ws.connect(mine, other, cx));
                let wire = Wire::new(mine, other);
                (ws.space.focused, ws.space.selected_wire) = (Some(mine), wire);
                wire
            })
        })
        .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path() });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(!failed, "{text}");
    let new = spawned(&ws, cx, mine);
    let (_, uid) = me(&ws, cx, new);
    assert!(
        text.contains(&uid) && text.contains(&folder.path().display().to_string()),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        let rect = ws.space.canvas.get(new).unwrap().rect;
        assert!(
            rect.x > 440.0 && rect.y == 100.0,
            "beside the caller: {rect:?}"
        );
        assert_eq!(ws.space.focused, Some(mine), "the keyboard stays");
        assert_eq!(ws.space.selected_wire, wire, "the selection stays");
        assert_eq!(
            ws.space.terminal(new).unwrap().seen.agent.as_deref(),
            Some("codex")
        );
    });
    assert_eq!(
        folder_of(&ws, new, cx),
        folder.path().canonicalize().unwrap()
    );
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "spawn_agent",
        json!({ "agent": "nope" }),
    );
    assert!(failed && text.contains("no agent nope"), "{text}");
    let nowhere = json!({ "agent": "codex", "folder": "/no/such/folder" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", nowhere);
    assert!(failed && text.contains("isn't a folder"), "{text}");
}

/// The model and the effort go right after the agent's command (`cat` here), before what Templum
/// adds; anything it can't type safely opens nothing, a worktree included.
#[gpui_kit::test]
fn spawn_agent_types_its_model_and_effort_first(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "model": "gpt-5.6-luna", "effort": "high" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(
        !failed && text.ends_with("connected to you, on gpt-5.6-luna (high)."),
        "{text}"
    );
    let new = spawned(&ws, cx, mine);
    screen_with(
        &ws,
        cx,
        new,
        "cat -m gpt-5.6-luna -c model_reasoning_effort=high",
    );
    ws.read_with(cx, |ws, _| {
        let p = ws.space.terminal(new).unwrap();
        assert_eq!(
            (
                p.opened_by.as_deref(),
                p.model.as_deref(),
                p.effort.as_deref()
            ),
            (Some(uid.as_str()), Some("gpt-5.6-luna"), Some("high"))
        );
    });
    let spawn =
        json!({ "agent": "claude", "folder": folder.path(), "model": "opus", "effort": "max" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(!failed, "{text}");
    let second = spawned(&ws, cx, mine);
    screen_with(&ws, cx, second, "cat --model opus --effort max");
    let repo = committed_repo();
    let blocks = ws.read_with(cx, |ws, _| ws.space.blocks.len());
    for (bad, why) in [
        (
            json!({ "agent": "codex", "model": "opus;rm -rf ~" }),
            "letters, digits",
        ),
        (json!({ "agent": "gemini", "effort": "high" }), "no effort"),
        (
            json!({ "agent": "copilot", "model": "gpt-5" }),
            "a model or an effort",
        ),
        (
            json!({ "agent": "codex", "effort": "extreme", "folder": repo.path(), "worktree": "w" }),
            "low, medium",
        ),
    ] {
        let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", bad.clone());
        assert!(failed && text.contains(why), "{bad}: {text}");
    }
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.space.blocks.len()),
        blocks,
        "nothing opened"
    );
    assert!(!repo.path().join(".worktrees").exists(), "no worktree made");
}

/// An agent closes the blocks it opened while it is wired to them, as one step of Ctrl+Z, which
/// brings one back still opened by it, its agent resumed with its model.
#[gpui_kit::test]
fn close_block_closes_only_what_the_caller_opened(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "model": "gpt-5.6-luna" });
    assert!(!call(handle, &ws, cx, &terminal, "spawn_agent", spawn).0);
    let new = spawned(&ws, cx, mine);
    let (_, them) = me(&ws, cx, new);
    // The last snapshot found its session: Ctrl+Z resumes it, not the folder's latest (the
    // caller's, in the same folder).
    let session = "0d5e5f4b-1c2d-4e3f-8a9b-0c1d2e3f4a5b";
    ws.update(cx, |ws, _| {
        ws.space.saved = Some(
            serde_json::from_value(json!({
                "version": 1, "camera": { "x": 0, "y": 0, "zoom": 1 },
                "items": [{ "kind": "terminal", "uid": them, "rect": { "x": 0, "y": 0, "w": 1, "h": 1 },
                            "agent": "codex", "session": session }],
            }))
            .unwrap(),
        )
    });
    let close = json!({ "block": them });
    let (failed, text) = call(handle, &ws, cx, &terminal, "close_block", close.clone());
    assert!(
        !failed && text.starts_with(&format!("Closed {them}.")),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.block_by_uid(&them).is_none());
        // Its shell ends now, not after a fade a workspace not shown never draws.
        assert!(
            ws.space.ghosts.iter().all(|g| g.block.id != new),
            "no ghost"
        );
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))
    })
    .unwrap();
    let back = spawned(&ws, cx, mine);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.uids[&back], them);
        let pane = ws.space.terminal(back).unwrap();
        assert_eq!(pane.opened_by.as_deref(), Some(uid.as_str()));
    });
    screen_with(
        &ws,
        cx,
        back,
        &format!("codex resume {session} -m gpt-5.6-luna"),
    );
    // A block the person opened, the caller's own, and one no longer wired: the person's.
    let other = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(Rect::new(40.0, 900.0, 400.0, 300.0), window, cx);
                let other = ws.space.canvas.top().unwrap();
                assert!(ws.connect(mine, other, cx));
                other
            })
        })
        .unwrap();
    let (_, theirs) = me(&ws, cx, other);
    for block in [&theirs, &uid] {
        let (failed, text) = call(
            handle,
            &ws,
            cx,
            &terminal,
            "close_block",
            json!({ "block": block }),
        );
        assert!(
            failed && text == format!("Only the person closes {block}: you didn't open it."),
            "{text}"
        );
    }
    ws.update(cx, |ws, cx| {
        assert!(ws.disconnect(Wire::new(mine, back).unwrap(), cx))
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "close_block", close);
    assert!(failed && text.contains("isn't connected to you"), "{text}");
}

/// A spawned Claude Code or Codex first asks whether to trust its folder (Codex also its hooks),
/// its cursor hidden: its prompt, whose Enter would answer for the person, waits until it brackets
/// pastes and shows its cursor, as its input box does. No hook speaks here: a Claude Code started
/// without Templum's hooks gets its prompt too.
#[gpui_kit::test]
fn a_prompt_waits_for_the_agents_input_to_take_pastes(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    for (conn, agent) in [(1, "claude"), (2, "codex")] {
        let folder = tempfile::tempdir().unwrap();
        let spawn = json!({ "agent": agent, "folder": folder.path(), "prompt": "say hi" });
        let (failed, said) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
        assert!(
            !failed
                && said.ends_with(&format!(
                    "Its prompt is typed once it is ready, as task t{conn}."
                )),
            "{said}"
        );
        let new = spawned(&ws, cx, mine);
        in_front(&ws, cx, new, true);
        found(&ws, cx, new);
        let (theirs, uid) = me(&ws, cx, new);
        let idle = json!({ "block": uid });
        let waiting = call_later(handle, &ws, cx, &terminal, conn, "wait_for_idle", idle);
        let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
        let prompt = |cx: &mut TestAppContext| {
            ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().prompt.is_some())
        };
        // Its trust question hides the cursor; and an input that doesn't bracket pastes isn't its
        // box.
        for bytes in [&b"\x1b[?25l\r"[..], b"\x1b[?25h\x1b[?2004l\r"] {
            prints(&ws, cx, new, bytes);
            for _ in 0..4 {
                std::thread::sleep(Duration::from_millis(100));
                after(cx, look);
            }
            assert!(prompt(cx), "{agent} typed into a question: {bytes:?}");
        }
        prints(&ws, cx, new, b"\x1b[?2004h\r");
        let mut looks = 0;
        while prompt(cx) {
            assert!(looks < 10, "{agent} never typed");
            std::thread::sleep(Duration::from_millis(100));
            after(cx, look);
            looks += 1;
        }
        // A task the caller hands over, under its own first line.
        screen_with(
            &ws,
            cx,
            new,
            &format!(", task t{conn}] Your task is in the pasted text: do it."),
        );
        screen_with(&ws, cx, new, "say hi");
        enter(cx);
        said_twice(&ws, cx, new);
        event(
            handle,
            &ws,
            cx,
            &theirs,
            json!({ "event": "submitted", "at_enter": true }),
        );
        event(
            handle,
            &ws,
            cx,
            &theirs,
            json!({ "event": "stopped", "message": "hi" }),
        );
        assert_eq!(answered(&waiting), (false, "hi".to_string()), "{agent}");
    }
}

/// The person types into a spawned agent before its prompt is (its trust question answered): that
/// turn isn't its task, and its end answers nothing.
#[gpui_kit::test]
fn a_turn_before_a_spawns_prompt_is_typed_isnt_its_task(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let (theirs, uid) = me(&ws, cx, new);
    let idle = json!({ "block": uid });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle);
    event(
        handle,
        &ws,
        cx,
        &theirs,
        json!({ "event": "submitted", "at_enter": true }),
    );
    event(
        handle,
        &ws,
        cx,
        &theirs,
        json!({ "event": "stopped", "message": "the person's answer" }),
    );
    assert!(waiting.try_recv().is_err(), "answered by the person's turn");
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(new).unwrap();
        assert!(pane.prompt.is_some() && !pane.handed.as_ref().unwrap().taken);
    });
}

/// A minute after its spawn its person still hasn't answered what its agent asks: the prompt is
/// dropped, and its task ends showing the question.
#[gpui_kit::test]
fn a_prompt_not_typed_within_a_minute_ends_its_task_with_its_screen(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    in_front(&ws, cx, new, true);
    found(&ws, cx, new);
    let (_, uid) = me(&ws, cx, new);
    prints(&ws, cx, new, b"Trust these hooks?\x1b[?25l\r");
    screen_with(&ws, cx, new, "Trust these hooks?");
    let idle = json!({ "block": uid });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle);
    ws.update(cx, |ws, _| {
        let handed = ws.space.terminal_mut(new).unwrap().handed.as_mut().unwrap();
        handed.since -= crate::workspace::spawn::STARTS_WITHIN;
    });
    after(cx, crate::workspace::spawn::READY_LOOK.as_millis() as u64);
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(new).unwrap();
        assert!(pane.prompt.is_none() && pane.handed.is_none());
    });
    let (failed, text) = answered(&waiting);
    assert!(
        failed
            && text.starts_with(&format!("{uid} never got its prompt"))
            && text.contains("Trust these hooks?"),
        "{text}"
    );
}

/// A spawned agent's start moves its screen, then its trust question sits still: the screen's
/// looks tell nothing while its prompt waits, so its task isn't ended as at rest after 5 seconds,
/// and the minute still ends it, with the question.
#[gpui_kit::test]
fn the_looks_leave_a_spawn_whose_prompt_waits_alone(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    in_front(&ws, cx, new, true);
    found(&ws, cx, new);
    let (_, uid) = me(&ws, cx, new);
    prints(&ws, cx, new, b"Trust this folder?\x1b[?25l\r");
    screen_with(&ws, cx, new, "Trust this folder?");
    let idle = json!({ "block": uid });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle);
    // As the looks would take its start for work.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(new).unwrap().state = Some(AgentState::Working)
    });
    for _ in 0..6 {
        after(cx, 1000);
    }
    assert!(waiting.try_recv().is_err(), "its task stays handed over");
    ws.update(cx, |ws, _| {
        let handed = ws.space.terminal_mut(new).unwrap().handed.as_mut().unwrap();
        handed.since -= crate::workspace::spawn::STARTS_WITHIN;
    });
    after(cx, crate::workspace::spawn::READY_LOOK.as_millis() as u64);
    let (failed, text) = answered(&waiting);
    assert!(
        failed && text.starts_with(&format!("{uid} never got its prompt")),
        "{text}"
    );
}

/// A snapshot taken while the new shell still starts (its rc files) finds no agent in it: the
/// Claude Code Templum typed there stays known, and its prompt is sent once its input is ready.
#[gpui_kit::test]
fn a_snapshot_before_the_agent_started_doesnt_stop_its_prompt(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    snapshot(
        handle,
        &ws,
        cx,
        new,
        templum_terminal::Inspection::default(),
    );
    in_front(&ws, cx, new, true);
    // A second snapshot finds it.
    let p = ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().term.processes());
    let running = templum_terminal::Inspection {
        agent: Some("claude".into()),
        pid: p.foreground,
        ..Default::default()
    };
    snapshot(handle, &ws, cx, new, running);
    let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
    let mut looks = 0;
    while ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().prompt.is_some()) {
        assert!(looks < 10, "never typed");
        std::thread::sleep(Duration::from_millis(100));
        after(cx, look);
        looks += 1;
    }
    screen_with(&ws, cx, new, "say hi");
    enter(cx);
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(new).unwrap();
        assert!(pane.handed.is_some(), "not taken: {:?}", pane.screen());
    });
    said_twice(&ws, cx, new);
}

/// `wait_for_idle` right after a spawn waits for the prompt's answer, though a snapshot taken
/// while the new shell still started found no agent in it.
#[gpui_kit::test]
fn wait_for_idle_waits_on_an_agent_a_snapshot_found_no_trace_of_yet(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let (_, uid) = me(&ws, cx, new);
    snapshot(
        handle,
        &ws,
        cx,
        new,
        templum_terminal::Inspection::default(),
    );
    let idle = json!({ "block": uid });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle);
    if let Ok(line) = waiting.try_recv() {
        panic!("answered at once: {:?}", result_of(&line));
    }
}

/// An agent no snapshot found yet may never have started (its folder's trust declined, its
/// shell's startup asking something): typed text could run in its shell, which macOS can't tell
/// is in front until a snapshot found the agent. What another agent hands it is refused until then.
#[gpui_kit::test]
fn nothing_is_handed_to_an_agent_no_snapshot_found_yet(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let (_, uid) = me(&ws, cx, new);
    snapshot(
        handle,
        &ws,
        cx,
        new,
        templum_terminal::Inspection::default(),
    );
    // Not the terminal's process in front, as macOS's shell under `login` isn't.
    in_front(&ws, cx, new, true);
    let reply = json!({ "block": uid, "text": "Done." });
    let (failed, text) = call(handle, &ws, cx, &terminal, "reply", reply);
    assert!(failed && text.contains("still starting"), "{text}");
    let task = json!({ "block": uid, "task": "go" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(failed && text.contains("still starting"), "{text}");
}

/// A spawned agent no snapshot found a while after its spawn never started: its prompt and its
/// task go, whoever waits on it hears so, and an agent started there later by hand gets no prompt.
#[gpui_kit::test]
fn a_spawned_agent_that_never_started_is_let_go(cx: &mut TestAppContext) {
    use templum_terminal::Inspection;
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let (_, uid) = me(&ws, cx, new);
    let idle = json!({ "block": uid });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle);
    // Spawned a while ago.
    ws.update(cx, |ws, _| {
        let handed = ws.space.terminal_mut(new).unwrap().handed.as_mut().unwrap();
        handed.since -= crate::workspace::spawn::STARTS_WITHIN;
    });
    snapshot(handle, &ws, cx, new, Inspection::default());
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(new).unwrap();
        assert!(pane.prompt.is_none() && pane.handed.is_none());
        assert_eq!(pane.seen.agent, None);
    });
    let (failed, text) = answered(&waiting);
    assert!(
        failed && text.contains("ended before it answered"),
        "{text}"
    );
    // Codex, started there by hand, is found in front.
    in_front(&ws, cx, new, true);
    let p = ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().term.processes());
    let running = Inspection {
        agent: Some("codex".into()),
        pid: p.foreground.or(p.shell),
        ..Default::default()
    };
    snapshot(handle, &ws, cx, new, running);
    let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(50));
        after(cx, look);
    }
    enter(cx);
    std::thread::sleep(Duration::from_millis(300));
    let shown = ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().screen());
    assert!(!shown.contains("say hi"), "typed: {shown:?}");
    assert!(waiting.try_recv().is_err(), "answered once");
}

#[gpui_kit::test]
fn another_agents_prompt_waits_for_its_process_and_a_still_screen(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    in_front(&ws, cx, new, true);
    let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
    let waiting = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().prompt.is_some())
    };
    // No snapshot found its process yet: nothing typed, however still its screen.
    for _ in 0..3 {
        after(cx, look);
    }
    assert!(waiting(cx));
    // A snapshot found it in front.
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(new).unwrap();
        pane.seen.pid = pane.term.processes().foreground;
    });
    let mut looks = 0;
    while waiting(cx) {
        assert!(looks < 10, "never typed");
        // What the shell prints lands between two looks.
        std::thread::sleep(Duration::from_millis(100));
        after(cx, look);
        looks += 1;
    }
    assert!(looks >= 2, "typed once two looks saw the same screen");
    screen_with(&ws, cx, new, "say hi");
    enter(cx);
    said_twice(&ws, cx, new);
}

/// In grid mode the new agent takes half of the caller's tile, not of the one the person works in,
/// which keeps the keyboard.
#[gpui_kit::test]
fn in_grid_mode_a_spawned_agent_takes_half_of_the_callers_tile(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let other = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(Rect::new(500.0, 100.0, 400.0, 300.0), window, cx);
                let other = ws.space.canvas.top().unwrap();
                ws.enter_grid(vec![mine, other], window, cx);
                other
            })
        })
        .unwrap();
    frame(handle, cx);
    let rect = |cx: &mut TestAppContext, id| {
        ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect)
    };
    let (half, theirs) = (rect(cx, mine), rect(cx, other));
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path() });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(!failed, "{text}");
    let new = spawned(&ws, cx, mine);
    let r = rect(cx, new);
    assert!(
        r.x >= half.x && r.right() <= half.right() && r.y >= half.y && r.bottom() <= half.bottom(),
        "in the caller's half: {r:?} in {half:?}"
    );
    assert_eq!(rect(cx, other), theirs, "the person's tile stays whole");
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, Some(other)));
}

/// A prompt the agent's screen doesn't show (a dialog that ignores pastes) gets no Enter, and
/// whoever waits on it hears at once that the agent didn't take it.
#[gpui_kit::test]
fn a_prompt_the_agent_didnt_take_ends_at_once(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    // Claude Code shows nothing it is typed: not even the task's first line, which the newline
    // starting its second paste would have `cat` print.
    ws.update(cx, |ws, _| {
        ws.command = Some(jobs(
            "stty -echo; printf '\\033[?2004h'; echo ready; cat >/dev/null; true",
        ))
    });
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let (_, uid) = me(&ws, cx, new);
    screen_with(&ws, cx, new, "ready");
    in_front(&ws, cx, new, true);
    let waiting = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "wait_for_idle",
        json!({ "block": uid }),
    );
    found(&ws, cx, new);
    after(cx, 5000);
    let (failed, text) = answered(&waiting);
    assert!(
        failed && text.starts_with(&format!("{uid} didn't take the task")),
        "{text}"
    );
}

/// An agent a snapshot saw that quit before its prompt was typed: its shell, in front, would run
/// the prompt as a command.
#[gpui_kit::test]
fn no_prompt_is_typed_into_the_shell_an_agent_left(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    // New terminals' shells with nothing in front.
    ws.update(cx, |ws, _| ws.command = Some(quiet()));
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    in_front(&ws, cx, new, false);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(new).unwrap().seen.pid = Some(1)
    });
    let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
    for _ in 0..4 {
        after(cx, look);
    }
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(new).unwrap().prompt.is_some(), "typed")
    });
}

#[gpui_kit::test]
fn spawn_agent_makes_its_worktree_first(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let repo = committed_repo();
    let spawn = json!({ "agent": "codex", "folder": repo.path(), "worktree": "fix-login" });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "spawn_agent", spawn);
    let (failed, text) = answer_soon(cx, &answers);
    let made = repo
        .path()
        .canonicalize()
        .unwrap()
        .join(".worktrees")
        .join("fix-login");
    assert!(!failed && made.is_dir(), "{text}");
    let new = spawned(&ws, cx, mine);
    assert_eq!(folder_of(&ws, new, cx), made);
    let plain = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": plain.path(), "worktree": "x" });
    let answers = call_later(handle, &ws, cx, &terminal, 2, "spawn_agent", spawn);
    let (failed, text) = answer_soon(cx, &answers);
    assert!(
        failed && text.contains("isn't in a Git repository"),
        "{text}"
    );
}

/// The new agent overlaps nothing on the canvas, drawings included, and goes the way of the
/// caller's connections: here its right holds a block, and its connection is far to its left.
#[gpui_kit::test]
fn a_spawned_agent_fits_where_nothing_is_toward_the_callers_connections(cx: &mut TestAppContext) {
    use templum_canvas::{Color, Drawing, ShapeKind};
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(Rect::new(464.0, 100.0, 400.0, 300.0), window, cx);
            ws.open(Rect::new(-3000.0, 100.0, 400.0, 300.0), window, cx);
            let far = ws.space.canvas.top().unwrap();
            assert!(ws.connect(mine, far, cx));
            // A line where a block left of the caller would sit.
            ws.space.ink.push(Drawing::Shape {
                kind: ShapeKind::Line,
                from: Point::new(-200.0, 150.0),
                to: Point::new(-100.0, 150.0),
                width: 4.0,
                color: Color::Red,
            });
            ws.space.focused = Some(mine);
        })
    })
    .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path() });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(!failed, "{text}");
    let new = spawned(&ws, cx, mine);
    ws.read_with(cx, |ws, _| {
        let rect = ws.space.canvas.get(new).unwrap().rect;
        let others = ws.space.canvas.blocks().iter().filter(|b| b.id != new);
        for other in others
            .map(|b| b.rect)
            .chain(ws.space.ink.iter().map(Drawing::bounds))
        {
            assert!(!rect.intersects(&other), "{rect:?} over {other:?}");
        }
        let caller = ws.space.canvas.get(mine).unwrap().rect;
        assert!(
            rect.x + rect.w / 2.0 < caller.x + caller.w / 2.0,
            "toward its connection, on its left: {rect:?}"
        );
    });
}
