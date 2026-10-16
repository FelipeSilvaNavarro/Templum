//! `spawn_agent`: an agent opened beside the caller (docs/specs/connections.md, Spawning). `cat`
//! stands in for the agents: every Unix has it, and it echoes what it is typed. Unix only (`mod.rs`).

use std::time::Duration;

use serde_json::json;
use templum_canvas::Wire;

use super::bus::{
    after, answer_soon, answered, call, call_later, event, me, result_of, screen_until, screen_with,
};
use super::delegate::snapshot;
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
        ws.command = Some(agent_cat());
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

#[gpui_kit::test]
fn a_claude_codes_prompt_is_typed_once_it_started(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(
        !failed && text.ends_with("Its prompt is typed once it is ready."),
        "{text}"
    );
    let new = spawned(&ws, cx, mine);
    in_front(&ws, cx, new, true);
    let (theirs, uid) = me(&ws, cx, new);
    // Handed over by the caller already: wait_for_idle waits for its answer.
    let waiting = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "wait_for_idle",
        json!({ "block": uid }),
    );
    // Not typed before Claude Code says it started: it may be asking whether to trust the folder.
    after(cx, 5000);
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(new).unwrap().prompt.is_some())
    });
    event(handle, &ws, cx, &theirs, json!({ "event": "started" }));
    screen_with(&ws, cx, new, "say hi");
    enter(cx);
    said_twice(&ws, cx, new);
    assert!(waiting.try_recv().is_err(), "it works on it");
    event(
        handle,
        &ws,
        cx,
        &theirs,
        json!({ "event": "stopped", "message": "hi" }),
    );
    assert_eq!(answered(&waiting), (false, "hi".to_string()));
}

/// A snapshot taken while the new shell still starts (its rc files) finds no agent in it: the
/// Claude Code Templum typed there stays known, and its prompt is sent once it started.
#[gpui_kit::test]
fn a_snapshot_before_the_agent_started_doesnt_stop_its_prompt(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let (theirs, _) = me(&ws, cx, new);
    snapshot(
        handle,
        &ws,
        cx,
        new,
        templum_terminal::Inspection::default(),
    );
    in_front(&ws, cx, new, true);
    event(handle, &ws, cx, &theirs, json!({ "event": "started" }));
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
    // Claude Code shows nothing it is typed.
    ws.update(cx, |ws, _| {
        ws.command = Some(jobs("stty -echo; echo ready; cat; true"))
    });
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let (theirs, uid) = me(&ws, cx, new);
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
    event(handle, &ws, cx, &theirs, json!({ "event": "started" }));
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
