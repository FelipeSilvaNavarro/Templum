//! `spawn_agent`: an agent opened beside the caller (docs/specs/connections.md, Spawning). `cat`
//! stands in for the agents: every Unix has it, and it echoes what it is typed.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::json;

use super::bus::{after, answered, call, call_later, event, me, result_of, screen_with};
use super::*;

/// The startup terminal at (40, 100), 400 × 300, the camera at 100% on the origin, and Claude Code
/// and Codex started as `cat`.
fn spawner(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, BlockId) {
    let (handle, ws) = open(cx);
    let mine = first_id(&ws, cx);
    ws.update(cx, |ws, _| {
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

/// The block spawned beside `mine`: the other end of its one wire.
fn spawned(ws: &Entity<Workspace>, cx: &mut TestAppContext, mine: BlockId) -> BlockId {
    ws.read_with(cx, |ws, _| {
        let wire = ws
            .space
            .wires
            .iter()
            .find(|w| w.touches(mine))
            .expect("wired to the caller");
        if wire.a == mine { wire.b } else { wire.a }
    })
}

/// The answer that comes on `answers` within ten seconds, background jobs running meanwhile.
fn answer_soon(cx: &mut TestAppContext, answers: &Receiver<String>) -> (bool, String) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if let Ok(line) = answers.try_recv() {
            return result_of(&line);
        }
        assert!(Instant::now() < deadline, "no answer");
        std::thread::sleep(Duration::from_millis(20));
    }
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
    enter(cx);
    screen_with(&ws, cx, new, "say hisay hi");
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

#[gpui_kit::test]
fn another_agents_prompt_waits_for_its_process_and_a_still_screen(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "prompt": "say hi" });
    call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    let new = spawned(&ws, cx, mine);
    let look = crate::workspace::spawn::READY_LOOK.as_millis() as u64;
    let waiting = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().prompt.is_some())
    };
    // No snapshot found its process yet: nothing typed, however still its screen.
    for _ in 0..3 {
        after(cx, look);
    }
    assert!(waiting(cx));
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(new).unwrap().seen.pid = Some(1)
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
    enter(cx);
    screen_with(&ws, cx, new, "say hisay hi");
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
