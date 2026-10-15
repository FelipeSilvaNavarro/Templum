//! Tasks handed between agents: `delegate`, `reply`, `wait_for_idle`, and the dot on the wire
//! (docs/specs/connections.md, Delegating).

use std::time::Instant;

use serde_json::json;
use templum_canvas::Wire;

#[cfg(unix)]
use super::bus::screen_with;
use super::bus::{after, answered, call, call_later, event, me};
use super::*;

/// Two terminals side by side and wired: mine at (40, 100) and theirs at (900, 100), 700 × 300
/// each, theirs running Codex as far as Templum knows; the camera at 100% on the origin.
fn two_agents(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, BlockId, BlockId) {
    let (handle, ws) = open(cx);
    let mine = first_id(&ws, cx);
    let theirs = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.space.pristine = false;
                ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), 1.0);
                ws.space.canvas.get_mut(mine).unwrap().rect = Rect::new(40.0, 100.0, 700.0, 300.0);
                ws.open(Rect::new(900.0, 100.0, 700.0, 300.0), window, cx);
                let theirs = ws.space.canvas.top().unwrap();
                ws.space.terminal_mut(theirs).unwrap().seen.agent = Some("codex".into());
                ws.space.appearing.clear();
                assert!(ws.connect(mine, theirs, cx));
                theirs
            })
        })
        .unwrap();
    frame(handle, cx);
    (handle, ws, mine, theirs)
}

/// The Enter a typed task waits for.
#[cfg(unix)]
fn enter(cx: &mut TestAppContext) {
    after(
        cx,
        crate::workspace::delegate::ENTER_AFTER.as_millis() as u64,
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn delegate_types_the_header_the_task_and_enter(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "fix the tests\nthen push" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(
        !failed && text.starts_with(&format!("Handed to {them}.")),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        let handed = ws
            .space
            .terminal(theirs)
            .unwrap()
            .handed
            .as_ref()
            .expect("handed over");
        assert_eq!(handed.by, terminal);
    });
    enter(cx);
    // `cat` asks for no bracketed paste: the task comes as one line, echoed as it is typed, then
    // printed again once Enter sent it.
    screen_with(
        &ws,
        cx,
        theirs,
        &format!("({uid})] fix the tests then push[from"),
    );
}

#[gpui_kit::test]
fn delegate_refuses_what_it_mustnt_type_into(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go" });
    // Waiting for its person: the typed task would answer its question.
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "needs_you", "message": "Allow Bash?" }),
    );
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task.clone());
    assert!(failed && text.contains("waiting for its person"), "{text}");
    // Working on its own prompt: typed in, the task waits its turn.
    event(handle, &ws, cx, &worker, json!({ "event": "working" }));
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task.clone());
    assert!(!failed, "{text}");
    // On a task already: a second one would take the first one's answer.
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task.clone());
    assert!(failed && text.contains("already on a task"), "{text}");
    // To itself, and to a terminal running no agent.
    let mine_task = json!({ "block": uid, "task": "go" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", mine_task);
    assert!(failed && text.contains("yours"), "{text}");
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(theirs).unwrap();
        pane.seen.agent = None;
        pane.handed = None;
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(failed && text.contains("runs no agent"), "{text}");
}

#[gpui_kit::test]
fn a_delegation_that_waits_ends_with_the_turns_answer(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "run the tests", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    assert!(answers.try_recv().is_err(), "it waits for the answer");
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "All green." }),
    );
    assert_eq!(answered(&answers), (false, "All green.".to_string()));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(theirs).unwrap().handed.is_none())
    });
}

#[gpui_kit::test]
fn reply_answers_the_task_before_its_turn_ends(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "find the bug", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    let reply = json!({ "block": uid, "text": "Line 42." });
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply);
    assert_eq!((failed, text), (false, format!("Answered {uid}.")));
    assert_eq!(answered(&answers), (false, "Line 42.".to_string()));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(theirs).unwrap().handed.is_none())
    });
    // Its turn's end comes later: nothing waits for it any more.
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "Fixed it too." }),
    );
    assert!(answers.try_recv().is_err(), "answered once");
}

#[cfg(unix)]
#[gpui_kit::test]
fn reply_with_no_one_waiting_types_into_the_agent(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (_, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let reply = json!({ "block": uid, "text": "Done." });
    // Mine runs no agent: the reply would run in its shell.
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply.clone());
    assert!(failed && text.contains("runs no agent"), "{text}");
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(mine).unwrap().seen.agent = Some("claude".into())
    });
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply);
    assert_eq!((failed, text), (false, format!("Typed into {uid}.")));
    enter(cx);
    screen_with(&ws, cx, mine, &format!("({them})] Done.[reply from"));
}

#[gpui_kit::test]
fn wait_for_idle_waits_for_the_turn_and_gives_the_last_answer(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let idle = json!({ "block": them });
    let (failed, text) = call(handle, &ws, cx, &terminal, "wait_for_idle", idle.clone());
    assert!(!failed && text.contains("is idle"), "{text}");
    event(handle, &ws, cx, &worker, json!({ "event": "working" }));
    let answers = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", idle.clone());
    assert!(
        answers.try_recv().is_err(),
        "it waits while the agent works"
    );
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "Merged." }),
    );
    assert_eq!(answered(&answers), (false, "Merged.".to_string()));
    let (failed, text) = call(handle, &ws, cx, &terminal, "wait_for_idle", idle);
    assert_eq!(
        (failed, text),
        (false, "Merged.".to_string()),
        "its last answer, at once"
    );
}

#[gpui_kit::test]
fn a_wait_that_times_out_gives_the_last_lines(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "think", "wait": true, "timeout": 2 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    after(cx, 2000);
    let (failed, text) = answered(&answers);
    assert!(
        failed
            && text.contains(&format!("No answer from {them} within 2 seconds"))
            && text.contains("last lines"),
        "{text}"
    );
}

#[gpui_kit::test]
fn a_turns_end_and_its_timeout_answer_once(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true, "timeout": 2 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "Done." }),
    );
    after(cx, 3000);
    assert_eq!(answered(&answers), (false, "Done.".to_string()));
    assert!(answers.try_recv().is_err(), "answered once");
}

#[gpui_kit::test]
fn closing_the_agent_worked_on_answers_its_wait(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(theirs, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    let (failed, text) = answered(&answers);
    assert!(
        failed && text.contains("was closed before it answered"),
        "{text}"
    );
}

#[gpui_kit::test]
fn a_dot_travels_only_while_a_task_is_handed_over(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let dots = |cx: &mut TestAppContext, at: Instant, still: bool| {
        ws.read_with(cx, |ws, _| ws.wire_dots(at, still))
    };
    assert!(
        dots(cx, Instant::now(), false).is_empty(),
        "nothing handed over"
    );
    call(
        handle,
        &ws,
        cx,
        &terminal,
        "delegate",
        json!({ "block": them, "task": "go" }),
    );
    let since = ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(theirs)
            .unwrap()
            .handed
            .as_ref()
            .unwrap()
            .since
    });
    let trip = crate::wires::DOT_TRIP;
    // The wire runs from mine's right edge (740, 250) to theirs' left one (900, 250). The dot
    // leaves from the agent that handed the task over, is halfway at half a trip, and starts again.
    let near = |dots: Vec<Point>, x: f32| {
        dots.len() == 1 && (dots[0].x - x).abs() < 0.5 && (dots[0].y - 250.0).abs() < 0.5
    };
    assert!(near(dots(cx, since, false), 740.0));
    assert!(near(dots(cx, since + trip / 2, false), 820.0));
    assert!(near(dots(cx, since + trip, false), 740.0));
    assert!(
        near(dots(cx, since, true), 820.0),
        "still, at the middle, with reduced motion"
    );
    // Painted with the wires.
    frame(handle, cx);
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "Done." }),
    );
    assert!(
        dots(cx, Instant::now(), false).is_empty(),
        "the answer came back"
    );
}

#[gpui_kit::test]
fn the_dot_goes_with_its_wire_and_the_answer_still_comes(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    ws.update(cx, |ws, cx| {
        assert!(ws.disconnect(Wire { a: mine, b: theirs }, cx))
    });
    ws.read_with(cx, |ws, _| {
        assert!(ws.wire_dots(Instant::now(), false).is_empty())
    });
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "Done." }),
    );
    assert_eq!(answered(&answers), (false, "Done.".to_string()));
}

/// An agent that goes without a word (Codex says nothing when it quits; one that crashed): the
/// first snapshot that misses its process ends its task. One not found yet (still starting) has
/// nothing to end.
#[gpui_kit::test]
fn an_agent_that_leaves_without_a_word_ends_its_task(cx: &mut TestAppContext) {
    use templum_terminal::Inspection;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let saved: crate::session::Saved = serde_json::from_value(
        json!({ "version": 1, "camera": { "x": 0, "y": 0, "zoom": 1 }, "items": [] }),
    )
    .unwrap();
    let snapshot = |cx: &mut TestAppContext, found: Inspection| {
        let done = vec![(space.clone(), vec![(theirs, found)], saved.clone(), None)];
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.snapshot_done(done, None, None, window, cx))
        })
        .unwrap();
    };
    snapshot(cx, Inspection::default());
    assert!(
        answers.try_recv().is_err(),
        "not found yet: it may be starting"
    );
    let running = Inspection {
        agent: Some("codex".into()),
        pid: Some(42),
        ..Default::default()
    };
    snapshot(cx, running);
    assert!(answers.try_recv().is_err(), "at work");
    snapshot(cx, Inspection::default());
    let (failed, text) = answered(&answers);
    assert!(
        failed && text.contains("ended before it answered"),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(theirs).unwrap().handed.is_none());
        assert!(
            ws.wire_dots(Instant::now(), false).is_empty(),
            "the dot stops"
        );
    });
}
