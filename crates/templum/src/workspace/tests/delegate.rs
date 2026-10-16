//! Tasks handed between agents: `delegate`, `reply`, `wait_for_idle`, and the dot on the wire
//! (docs/specs/connections.md, Delegating).

use std::time::Instant;

use serde_json::json;
use templum_canvas::Wire;

#[cfg(unix)]
use super::bus::result_of;
use super::bus::{after, answered, call, call_later, event, me, screen_with};
use super::*;

/// Two terminals side by side and wired: mine at (40, 100) and theirs at (900, 100), 700 × 300
/// each, theirs running Codex as far as Templum knows; the camera at 100% on the origin.
fn two_agents(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, BlockId, BlockId) {
    agents_running(cx, None)
}

/// `two_agents`, theirs running `command` (`agent_cat` when None), each once its program is in
/// front, where a snapshot found theirs.
fn agents_running(
    cx: &mut TestAppContext,
    command: Option<(String, Vec<String>)>,
) -> (AnyWindowHandle, Entity<Workspace>, BlockId, BlockId) {
    let (handle, ws) = open_as(cx, Some(agent_cat()), None, None);
    let mine = first_id(&ws, cx);
    let theirs = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                if command.is_some() {
                    ws.command = command;
                }
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
    #[cfg(unix)]
    for id in [mine, theirs] {
        in_front(&ws, cx, id, true);
    }
    found(&ws, cx, theirs);
    (handle, ws, mine, theirs)
}

/// A snapshot found the agent of terminal block `id`: in front of its shell (Unix), or below it
/// (Windows, which has no process in front).
fn found(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) {
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(id).unwrap();
        let p = pane.term.processes();
        pane.seen.pid = p.foreground.or(p.shell);
    });
}

/// A snapshot that found `found` in the terminal block `id`, as one taken every few seconds.
pub(super) fn snapshot(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    found: templum_terminal::Inspection,
) {
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let saved: crate::session::Saved = serde_json::from_value(
        json!({ "version": 1, "camera": { "x": 0, "y": 0, "zoom": 1 }, "items": [] }),
    )
    .unwrap();
    let done = vec![(space, vec![(id, found)], saved, None)];
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.snapshot_done(done, None, None, window, cx))
    })
    .unwrap();
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
    // `cat` asks for no bracketed paste: the task comes as one line, echoed as it is typed, then
    // printed again once Enter sent it.
    screen_with(&ws, cx, theirs, "then push");
    enter(cx);
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
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
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
    found(&ws, cx, mine);
    let (failed, text) = call(handle, &ws, cx, &worker, "reply", reply);
    assert_eq!((failed, text), (false, format!("Typed into {uid}.")));
    screen_with(&ws, cx, mine, "Done.");
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
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
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
    // The agent's screen shows the task in real time; the test's clock would otherwise run the
    // look for it out first, and its Enter would never go.
    screen_with(&ws, cx, theirs, "think");
    after(cx, 2000);
    let (failed, text) = answered(&answers);
    assert!(
        failed
            && text.contains(&format!("No answer from {them} within 2 seconds"))
            && text.contains("last lines"),
        "{text}"
    );
}

/// Codex never says it works: once a task's wait timed out, only the task, still unanswered, tells
/// that it does. `wait_for_idle` from the agent that handed it over waits for its answer, not
/// the turn before's.
#[gpui_kit::test]
fn wait_for_idle_waits_for_a_task_whose_wait_timed_out(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let stopped = |message: &str| json!({ "event": "stopped", "message": message });
    event(handle, &ws, cx, &worker, stopped("Old."));
    let task = json!({ "block": them, "task": "think", "wait": true, "timeout": 2 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    // The agent's screen shows the task in real time; the test's clock would otherwise run the
    // look for it out first, and its Enter would never go.
    screen_with(&ws, cx, theirs, "think");
    after(cx, 2000);
    assert!(answered(&answers).0, "timed out");
    // Its deadline passed.
    ws.update(cx, |ws, _| {
        let handed = ws.space.terminal_mut(theirs).unwrap().handed.as_mut();
        let handed = handed.expect("unanswered");
        handed.until = handed.since;
    });
    let idle = json!({ "block": them });
    let waiting = call_later(handle, &ws, cx, &terminal, 2, "wait_for_idle", idle);
    assert!(waiting.try_recv().is_err(), "it still works on the task");
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
    event(handle, &ws, cx, &worker, stopped("Done."));
    assert_eq!(answered(&waiting), (false, "Done.".to_string()));
}

#[gpui_kit::test]
fn a_turns_end_and_its_timeout_answer_once(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true, "timeout": 2 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
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

/// The turn an agent was on when a task was typed into it ends first, and isn't the task's
/// answer. Codex says it took the task once a turn takes it in; Claude Code says so at its Enter
/// (`at_enter`) and queues it behind the turn under way, which a tool's end takes it into.
#[gpui_kit::test]
fn a_turns_end_before_its_task_was_taken_doesnt_answer_it(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let send = |cx: &mut TestAppContext, e: serde_json::Value| event(handle, &ws, cx, &worker, e);
    let submitted = |at_enter: bool| json!({ "event": "submitted", "at_enter": at_enter });
    let stopped = |message: &str| json!({ "event": "stopped", "message": message });
    let task = json!({ "block": them, "task": "say B", "wait": true });
    // Codex at work on its person's prompt: the turn under way ends before taking the task.
    send(cx, submitted(false));
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task.clone());
    send(cx, stopped("A"));
    assert!(answers.try_recv().is_err(), "the turn it was on");
    send(cx, submitted(false));
    send(cx, stopped("B"));
    assert_eq!(answered(&answers), (false, "B".to_string()));
    // Claude Code queues it at its Enter; the turn under way ends with no tool after it.
    send(cx, submitted(true));
    let answers = call_later(handle, &ws, cx, &terminal, 2, "delegate", task.clone());
    send(cx, submitted(true));
    send(cx, stopped("1 to 300"));
    assert!(answers.try_recv().is_err(), "the turn it was queued behind");
    send(cx, stopped("B"));
    assert_eq!(answered(&answers), (false, "B".to_string()));
    // A tool's end takes it into the turn under way, whose end answers it.
    send(cx, submitted(true));
    let answers = call_later(handle, &ws, cx, &terminal, 3, "delegate", task);
    send(cx, submitted(true));
    send(cx, json!({ "event": "working" }));
    send(cx, stopped("A\nB"));
    assert_eq!(answered(&answers), (false, "A\nB".to_string()));
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
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
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
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "Done." }),
    );
    assert_eq!(answered(&answers), (false, "Done.".to_string()));
}

/// A screen the task doesn't show on (a dialog that ignores the paste, as Codex's approval does)
/// gets no Enter: it would pick the dialog's answer for the person. The task ends at once, and
/// the agent that handed it over hears so. A reply that isn't taken ends no task.
#[cfg(unix)]
#[gpui_kit::test]
fn no_enter_goes_to_a_screen_the_task_didnt_change(cx: &mut TestAppContext) {
    // A program that shows nothing it is typed; `cat` prints a line only once Enter sends it.
    let command = jobs("stty -echo; echo ready; cat; true");
    let (handle, ws, mine, theirs) = agents_running(cx, Some(command));
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    screen_with(&ws, cx, theirs, "ready");
    // `cat`, not `stty`, which the snapshot may have found in front.
    in_front(&ws, cx, theirs, true);
    found(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go on", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    after(cx, 5000);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let shown = ws.read_with(cx, |ws, _| ws.space.terminal(theirs).unwrap().screen());
    assert!(!shown.contains("go on"), "an Enter went: {shown:?}");
    let (failed, text) = answered(&answers);
    assert!(
        failed && text.starts_with(&format!("{them} didn't take the task")),
        "{text}"
    );
    let handed = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.space.terminal(theirs).unwrap().handed.is_some()
        })
    };
    assert!(!handed(cx), "the task ended");
    // A task another agent handed it stays, whatever happens to a reply typed into it.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(theirs).unwrap().handed = Some(crate::workspace::Handed::new(
            "another",
            std::time::Duration::from_secs(600),
        ));
    });
    let reply = json!({ "block": them, "text": "Done." });
    let (failed, text) = call(handle, &ws, cx, &terminal, "reply", reply);
    assert!(!failed, "{text}");
    after(cx, 5000);
    assert!(handed(cx), "a reply ends no task");
}

/// A dialog that opens between the paste and its Enter (Claude Code asks to run a tool while it
/// works on a prompt of its own, its screen changing all along) gets no Enter: it would pick the
/// dialog's first answer for the person.
#[cfg(unix)]
#[gpui_kit::test]
fn no_enter_goes_to_an_agent_that_started_asking_its_person(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
    let task = json!({ "block": them, "task": "go on", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    screen_with(&ws, cx, theirs, "go on");
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "needs_you", "message": "Allow Bash?" }),
    );
    after(cx, 5000);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let shown = screen_with(&ws, cx, theirs, "go on");
    assert_eq!(
        shown.matches("go on").count(),
        1,
        "an Enter went: {shown:?}"
    );
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("didn't take the task"), "{text}");
}

/// An agent that quit leaves its shell in front, where typed text would run as a command; until a
/// snapshot misses it (up to 5 s), Templum still takes it to run there.
#[cfg(unix)]
#[gpui_kit::test]
fn nothing_is_typed_into_the_shell_an_agent_left(cx: &mut TestAppContext) {
    // Its agent's job ends at an end of file; its shell (reading as `cat` does) is then in front.
    let (handle, ws, mine, theirs) = agents_running(cx, Some(jobs("cat; exec cat")));
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(theirs).unwrap().term.write(&b"\x04"[..])
    });
    in_front(&ws, cx, theirs, false);
    ws.read_with(cx, |ws, _| {
        let seen = &ws.space.terminal(theirs).unwrap().seen;
        assert!(seen.agent.is_some(), "no snapshot missed it yet");
    });
    // Before a snapshot found it (a Claude Code's prompt typed at its start), its shell's own
    // process in front tells.
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(theirs).unwrap();
        let pid = pane.seen.pid.take();
        assert!(pane.shell_in_front(), "its shell is in front");
        pane.seen.pid = pid;
    });
    let task = json!({ "block": them, "task": "go" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task.clone());
    assert!(failed && text.contains("runs no agent"), "{text}");
    // An agent run in its shell's place (`exec claude`) is in front as the shell, where a
    // snapshot found it.
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(theirs).unwrap();
        pane.seen.pid = pane.term.processes().shell;
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(!failed, "{text}");
}

/// macOS starts a terminal's shell under `login`, and the shell takes a process group of its own:
/// once its agent quit, what is in front isn't the terminal's process. That the agent a snapshot
/// found is no longer in front tells.
#[cfg(unix)]
#[gpui_kit::test]
fn nothing_is_typed_into_a_shell_under_login_an_agent_left(cx: &mut TestAppContext) {
    // The first `sh` stands in for `login`, the second for the shell, its job for the agent.
    let shell = jobs(r#"sh -c 'set -m; sh -c "echo agent; exec cat"; exec cat'; true"#);
    let (handle, ws, mine, theirs) = agents_running(cx, Some(shell));
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    screen_with(&ws, cx, theirs, "agent");
    let front = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.space.terminal(theirs).unwrap().term.processes()
        })
    };
    let agent = front(cx).foreground;
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(theirs).unwrap();
        pane.seen.pid = agent;
        assert!(!pane.shell_in_front(), "the agent is in front");
        pane.term.write(&b"\x04"[..]);
    });
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    while front(cx).foreground == agent {
        assert!(Instant::now() < deadline, "the agent never quit");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let p = front(cx);
    assert_ne!(
        p.foreground, p.shell,
        "the shell isn't the terminal's process"
    );
    let task = json!({ "block": them, "task": "go" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "delegate", task);
    assert!(failed && text.contains("runs no agent"), "{text}");
}

/// An agent that goes without a word (Codex says nothing when it quits; one that crashed): the
/// first snapshot that misses its process ends its task.
#[gpui_kit::test]
fn an_agent_that_leaves_without_a_word_ends_its_task(cx: &mut TestAppContext) {
    use templum_terminal::Inspection;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    let snapshot = |cx: &mut TestAppContext, found| snapshot(handle, &ws, cx, theirs, found);
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

#[gpui_kit::test]
fn wait_for_needs_you_now_or_when_it_asks(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let needs_you = json!({ "block": them, "event": "needs_you" });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "wait_for", needs_you.clone());
    assert!(answers.try_recv().is_err(), "it waits");
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "needs_you", "message": "Allow Bash?" }),
    );
    let asks = format!("{them} is waiting for its person: Allow Bash?");
    assert_eq!(answered(&answers), (false, asks.clone()));
    let (failed, text) = call(handle, &ws, cx, &terminal, "wait_for", needs_you.clone());
    assert_eq!((failed, text), (false, asks), "at once while it still asks");
    // Its own question, in its balloon, is a wait for its person too.
    event(handle, &ws, cx, &worker, json!({ "event": "working" }));
    let answers = call_later(handle, &ws, cx, &terminal, 2, "wait_for", needs_you);
    let _question = call_later(
        handle,
        &ws,
        cx,
        &worker,
        3,
        "ask_user",
        json!({ "question": "Which branch?" }),
    );
    assert_eq!(
        answered(&answers),
        (
            false,
            format!("{them} is waiting for its person: Which branch?")
        )
    );
}

#[gpui_kit::test]
fn wait_for_idle_is_wait_for_idle_and_the_rest_need_a_wire(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "wait_for",
        json!({ "block": them, "event": "idle" }),
    );
    assert!(!failed && text.contains("is idle"), "{text}");
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "wait_for",
        json!({ "block": uid, "event": "attention" }),
    );
    assert!(failed && text.contains("yours"), "{text}");
    ws.update(cx, |ws, cx| {
        ws.disconnect(Wire::new(mine, theirs).unwrap(), cx);
    });
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "wait_for",
        json!({ "block": them, "event": "attention" }),
    );
    assert!(failed && text.contains("isn't connected to you"), "{text}");
}

/// Keys go as a keyboard sends them, never as a paste: a dialog takes `2` as its option, where a
/// pasted one would land in the agent's input.
#[cfg(unix)]
#[gpui_kit::test]
fn send_keys_presses_keys_not_a_paste(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    // `cat` prints the sequence back: the terminal now brackets pastes, as Claude Code's does.
    prints(&ws, cx, theirs, b"\x1b[?2004hon\r");
    screen_with(&ws, cx, theirs, "2004honon");
    let keys = json!({ "block": them, "keys": ["2", "escape"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "send_keys", keys);
    let (failed, text) = heard(cx, &answers);
    assert!(
        !failed && text.starts_with(&format!("Pressed 2 keys in {them}.")),
        "{text}"
    );
    assert!(text.ends_with("2^["), "the screen after them: {text}");
    let shown = screen_with(&ws, cx, theirs, "2^[");
    assert!(!shown.contains("200~"), "never a paste: {shown}");
}

/// Keys go into an agent waiting for its person, as the person's: once its screen moves, it works.
#[cfg(unix)]
#[gpui_kit::test]
fn send_keys_takes_a_dialog_as_the_persons_keys(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let asks = json!({ "event": "needs_you", "message": "Allow Bash?" });
    event(handle, &ws, cx, &worker, asks);
    let keys = json!({ "block": them, "keys": ["2"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "send_keys", keys);
    let (failed, text) = heard(cx, &answers);
    assert!(
        !failed && text.starts_with(&format!("Pressed 1 key in {them}.")),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        let (by, _) = ws
            .space
            .terminal(theirs)
            .unwrap()
            .keys_from
            .clone()
            .expect("who");
        assert!(by.ends_with(&format!("({uid})")), "{by}");
    });
    ticks(&ws, cx, theirs, 0, 4);
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working));
}

#[cfg(unix)]
#[gpui_kit::test]
fn send_keys_refuses_its_own_block_an_unknown_key_and_another_agents_task(cx: &mut TestAppContext) {
    use crate::workspace::Handed;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let pressing = |block: &str, keys: serde_json::Value| json!({ "block": block, "keys": keys });
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "send_keys",
        pressing(&uid, json!(["y"])),
    );
    assert!(failed && text.contains("yours"), "{text}");
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "send_keys",
        pressing(&them, json!(["y", "foo"])),
    );
    assert!(
        failed && text.contains("\"foo\"") && text.contains("page_up"),
        "{text}"
    );
    let typed = ws.read_with(cx, |ws, _| ws.space.terminal(theirs).unwrap().typed.get());
    assert!(!typed, "no key went before the refusal");
    // On another agent's task: refused while it works, not while it waits for its person.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(theirs).unwrap().handed = Some(Handed::new(
            "elsewhere",
            std::time::Duration::from_secs(600),
        ));
    });
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "send_keys",
        pressing(&them, json!(["y"])),
    );
    assert!(failed && text.contains("another agent"), "{text}");
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "needs_you", "message": "Allow Bash?" }),
    );
    let y = pressing(&them, json!(["y"]));
    let answers = call_later(handle, &ws, cx, &terminal, 2, "send_keys", y);
    assert!(
        !heard(cx, &answers).0,
        "it waits for its person: its dialog takes keys"
    );
    // Its block closes between two keys: the call ends there.
    let ab = pressing(&them, json!(["a", "b"]));
    let answers = call_later(handle, &ws, cx, &terminal, 3, "send_keys", ab);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.close_now(&[theirs], crate::workspace::history::Then::New, window, cx)
        })
    })
    .unwrap();
    let (failed, text) = heard(cx, &answers);
    assert!(failed && text.contains("closed"), "{text}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn interrupt_presses_esc_and_ends_the_callers_task(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let stop = json!({ "block": them });
    let (failed, text) = call(handle, &ws, cx, &terminal, "interrupt", stop.clone());
    assert!(failed && text.contains("idle"), "{text}");
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "interrupt",
        json!({ "block": uid }),
    );
    assert!(failed && text.contains("yours"), "{text}");
    let task = json!({ "block": them, "task": "count to a million", "wait": true });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    // A spawn's prompt still waiting is that task: it goes with it.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(theirs).unwrap().prompt = Some(crate::workspace::Prompt {
            text: "count to a million".into(),
            screen: None,
        })
    });
    let stopped = call_later(handle, &ws, cx, &terminal, 2, "interrupt", stop.clone());
    assert_eq!(
        answered(&waiting),
        (true, "Interrupted by you.".to_string())
    );
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(theirs).unwrap();
        assert!(pane.handed.is_none() && pane.prompt.is_none())
    });
    let (failed, text) = heard(cx, &stopped);
    assert!(
        !failed
            && text.starts_with(&format!(
                "Pressed Esc in {them}, and ended the task you handed it."
            ))
            && text.contains("^["),
        "{text}"
    );
    // Another agent's task: only that agent stops it.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(theirs).unwrap().handed = Some(crate::workspace::Handed::new(
            "elsewhere",
            std::time::Duration::from_secs(600),
        ));
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "interrupt", stop);
    assert!(failed && text.contains("another agent"), "{text}");
}

/// Keys from another block name it after the dot, in place of the usage chip, and in the dot's
/// tooltip, for ten seconds.
#[gpui_kit::test]
fn keys_name_who_pressed_them_for_ten_seconds(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, id);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().seen.agent = Some("codex".into())
    });
    event(handle, &ws, cx, &terminal, json!({ "event": "submitted" }));
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().keys_from =
            Some(("Claude Code (b7k2m)".into(), Instant::now()))
    });
    let tip = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            crate::block::status_dot(ws, ws.space.terminal(id).unwrap()).map(|(_, t)| t.to_string())
        })
    };
    let chip = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(("answered-by", id.0 as usize)).is_some()
        })
        .unwrap()
    };
    assert_eq!(
        tip(&ws, cx).as_deref(),
        Some("Working · answered by Claude Code (b7k2m)")
    );
    assert!(chip(cx));
    ws.update(cx, |ws, _| {
        let at = &mut ws
            .space
            .terminal_mut(id)
            .unwrap()
            .keys_from
            .as_mut()
            .unwrap()
            .1;
        *at = at.checked_sub(std::time::Duration::from_secs(11)).unwrap();
    });
    assert_eq!(tip(&ws, cx).as_deref(), Some("Working"));
    assert!(!chip(cx));
}

/// `cat` in block `id` prints `bytes` back, as a program would.
#[cfg(unix)]
fn prints(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId, bytes: &'static [u8]) {
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(id).unwrap().term.write(bytes.to_vec())
    });
}

/// The answer on `answers`, the test's clock running a look at a time: the terminal's own thread
/// reads what `cat` prints back.
#[cfg(unix)]
fn heard(cx: &mut TestAppContext, answers: &std::sync::mpsc::Receiver<String>) -> (bool, String) {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        after(cx, 250);
        if let Ok(line) = answers.try_recv() {
            return result_of(&line);
        }
        assert!(Instant::now() < deadline, "no answer");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[gpui_kit::test]
fn wait_for_a_finished_command_or_a_call_for_attention(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let event = |name: &str| json!({ "block": them, "event": name });
    let finished = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "wait_for",
        event("command_finished"),
    );
    let attention = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "wait_for",
        event("attention"),
    );
    after(cx, 500);
    assert!(
        finished.try_recv().is_err() && attention.try_recv().is_err(),
        "nothing yet"
    );
    // A command starts and ends with code 3: OSC 133 C, then D;3.
    prints(&ws, cx, theirs, b"\x1b]133;C\x07\x1b]133;D;3\x07\r");
    let (failed, text) = heard(cx, &finished);
    assert!(
        !failed && text.starts_with(&format!("A command finished in {them}, with exit code 3.")),
        "{text}"
    );
    assert!(
        attention.try_recv().is_err(),
        "a command isn't a call for attention"
    );
    prints(&ws, cx, theirs, b"\x1b]777;notify;Build;done\x07\r");
    assert_eq!(
        heard(cx, &attention),
        (false, format!("{them} asks for attention: done"))
    );
    let bell = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        3,
        "wait_for",
        event("attention"),
    );
    prints(&ws, cx, theirs, b"\x07\r");
    assert_eq!(heard(cx, &bell), (false, format!("{them} rang its bell.")));
}

#[gpui_kit::test]
fn a_wait_for_times_out_goes_with_its_block_and_shows_no_balloon(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let soon = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "wait_for",
        json!({ "block": them, "event": "attention", "timeout": 1 }),
    );
    assert!(
        ws.read_with(cx, |ws, _| ws.balloon(&terminal).is_none()),
        "a wait asks the person nothing"
    );
    after(cx, 1000);
    let (failed, text) = answered(&soon);
    assert!(
        failed
            && text.starts_with(&format!(
                "No call for attention in {them} within 1 seconds."
            )),
        "{text}"
    );
    let closed = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "wait_for",
        json!({ "block": them, "event": "command_finished" }),
    );
    // Its block goes, and no frame is drawn (a hidden window draws none): `take_block` asks for
    // none, where `remove_block` would, and a test draws each window that asked.
    let _gone = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.take_block(theirs, window, cx))
        })
        .unwrap();
    after(cx, 250);
    assert_eq!(
        answered(&closed),
        (true, format!("{them} was closed while you waited."))
    );
}

/// `cat` in block `id` prints `text` back, and the test waits until its screen shows it.
#[cfg(unix)]
fn shows(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId, text: &str) {
    let line = format!("{text}\r").into_bytes();
    ws.read_with(cx, |ws, _| ws.space.terminal(id).unwrap().term.write(line));
    screen_with(ws, cx, id, text);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_screen_at_work_says_working_and_one_gone_still_ends_its_task(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let state = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(theirs).unwrap().state.clone())
    };
    // The person types: their keys change the screen, and tell nothing.
    for i in 0..4 {
        ws.read_with(cx, |ws, _| {
            ws.space.terminal(theirs).unwrap().typed.set(true)
        });
        shows(&ws, cx, theirs, &format!("typed {i}"));
        after(cx, 1000);
    }
    assert_eq!(state(&ws, cx), None);
    // Its own output, a second apart: at work.
    for i in 0..4 {
        shows(&ws, cx, theirs, &format!("tick {i}"));
        after(cx, 1000);
    }
    assert_eq!(state(&ws, cx), Some(AgentState::Working));
    let idle = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "wait_for_idle",
        json!({ "block": them }),
    );
    assert!(idle.try_recv().is_err(), "it waits while the agent works");
    for _ in 0..5 {
        after(cx, 1000);
    }
    assert_eq!(state(&ws, cx), Some(AgentState::Idle));
    let (failed, text) = answered(&idle);
    assert!(
        !failed
            && text.starts_with("Its screen has been still for 5 seconds")
            && text.contains("tick 3"),
        "{text}"
    );
}

/// Their agent's screen shows a tick a second apart, `n` times: at work.
#[cfg(unix)]
fn ticks(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId, from: usize, n: usize) {
    for i in from..from + n {
        shows(ws, cx, id, &format!("tick {i}"));
        after(cx, 1000);
    }
}

/// The state of terminal block `id`'s agent.
fn state_of(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
) -> Option<crate::workspace::AgentState> {
    ws.read_with(cx, |ws, _| ws.space.terminal(id).unwrap().state.clone())
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_turns_end_right_after_its_spinner_stays_idle(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    ticks(&ws, cx, theirs, 0, 4);
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working));
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "All done." }),
    );
    for _ in 0..2 {
        after(cx, 1000);
        assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Idle));
    }
    let answer = call(
        handle,
        &ws,
        cx,
        &terminal,
        "wait_for_idle",
        json!({ "block": them }),
    );
    assert_eq!(answer, (false, "All done.".to_string()), "at once");
}

/// A dialog sat still for longer than a screen takes to say "at rest"; the person allowed the
/// Templum tool it asked for, which now runs: its agent works, and the stillness before says
/// nothing of it.
#[gpui_kit::test]
fn a_tool_allowed_after_a_still_dialog_works(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, _, theirs) = two_agents(cx);
    let (worker, _) = me(&ws, cx, theirs);
    let asks = json!({ "event": "needs_you", "message": "Allow mcp__templum__whoami?" });
    event(handle, &ws, cx, &worker, asks);
    for _ in 0..6 {
        after(cx, 1000);
    }
    assert!(matches!(
        state_of(&ws, cx, theirs),
        Some(AgentState::NeedsYou(_))
    ));
    call_later(handle, &ws, cx, &worker, 1, "whoami", json!({}));
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working));
    after(cx, 1000);
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working));
}

/// A dialog opens while the agent works: its dot stays yellow, and a task waiting on its turn
/// waits on, while the dialog sits still and while a subagent draws under it; once the person
/// typed into it (they answered), the screen at work says Working again.
#[cfg(unix)]
#[gpui_kit::test]
fn a_dialog_stays_yellow_until_its_person_answers(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    ticks(&ws, cx, theirs, 0, 4);
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working));
    let idle = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "wait_for_idle",
        json!({ "block": them }),
    );
    // The person was typing as the dialog opened: those keys answer nothing.
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(theirs).unwrap().typed.set(true)
    });
    let asks = json!({ "event": "needs_you", "message": "Allow Bash?" });
    event(handle, &ws, cx, &worker, asks);
    let asking = Some(AgentState::NeedsYou(Some("Allow Bash?".into())));
    for _ in 0..6 {
        after(cx, 1000);
    }
    assert_eq!(state_of(&ws, cx, theirs), asking, "still");
    ticks(&ws, cx, theirs, 4, 6);
    assert_eq!(state_of(&ws, cx, theirs), asking, "a subagent draws");
    assert!(idle.try_recv().is_err(), "it still waits");
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(theirs).unwrap().typed.set(true)
    });
    ticks(&ws, cx, theirs, 10, 4);
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working));
}

/// The person scrolls an agent's screen, or its program hears their mouse: the screen changes,
/// and tells nothing. Nor does the mouse answer a dialog: it may only pass over a TUI that hears
/// every move.
#[cfg(unix)]
#[gpui_kit::test]
fn the_wheel_and_the_mouse_tell_nothing(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, _, theirs) = two_agents(cx);
    let (worker, _) = me(&ws, cx, theirs);
    let scroll = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            // Its left: its middle is past the window's right edge.
            let body = window.find(("body", theirs.0 as usize)).bounds().origin;
            let at = (f32::from(body.x) + 20., f32::from(body.y) + 20.);
            wheel(window, cx, at, ScrollDelta::Lines(point(0., -1.)), false);
        })
        .unwrap();
    };
    for i in 0..4 {
        scroll(cx);
        shows(&ws, cx, theirs, &format!("scrolled {i}"));
        after(cx, 1000);
    }
    assert_eq!(state_of(&ws, cx, theirs), None, "its history scrolls");
    prints(&ws, cx, theirs, b"\x1b[?1002h\x1b[?1006h\r");
    super::terminals::until("the mouse", handle, &ws, cx, |ws| {
        ws.space
            .terminal(theirs)
            .unwrap()
            .term
            .shared()
            .modes()
            .mouse
    });
    for i in 0..4 {
        scroll(cx);
        shows(&ws, cx, theirs, &format!("heard {i}"));
        after(cx, 1000);
    }
    assert_eq!(state_of(&ws, cx, theirs), None, "its program hears it");
    let asks = json!({ "event": "needs_you", "message": "Allow Bash?" });
    event(handle, &ws, cx, &worker, asks);
    scroll(cx);
    ticks(&ws, cx, theirs, 0, 4);
    assert!(
        matches!(state_of(&ws, cx, theirs), Some(AgentState::NeedsYou(_))),
        "the mouse answers no dialog"
    );
}

/// An agent whose hooks speak (Claude Code, Gemini CLI) says it took a task: its turn starts. With
/// no word from it, Templum presses Enter once more (the first may have joined the paste, as a
/// new line); still none, the task ends untaken, and the agent that handed it over hears so.
#[cfg(unix)]
#[gpui_kit::test]
fn a_task_no_hook_confirms_gets_its_enter_again_then_ends(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    event(handle, &ws, cx, &worker, json!({ "event": "started" }));
    let task = json!({ "block": them, "task": "think", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    screen_with(&ws, cx, theirs, "think");
    enter(cx);
    let confirm = crate::workspace::delegate::CONFIRM.as_millis() as u64;
    after(cx, confirm);
    assert!(answers.try_recv().is_err(), "it gets its Enter again first");
    after(cx, confirm);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("didn't take the task"), "{text}");
}

/// A hook after the Enter (its turn started) confirms it; an agent at work queued the task; one
/// whose hooks never said a turn started (Codex, a `claude` typed by hand) can't confirm it, and
/// Templum never presses Enter blindly into it. None of them ends the task untaken.
#[cfg(unix)]
#[gpui_kit::test]
fn a_task_confirmed_queued_or_unconfirmable_stays_handed(cx: &mut TestAppContext) {
    let confirm = crate::workspace::delegate::CONFIRM.as_millis() as u64;
    for before in ["none", "started", "submitted"] {
        let (handle, ws, mine, theirs) = two_agents(cx);
        let (terminal, _) = me(&ws, cx, mine);
        let (worker, them) = me(&ws, cx, theirs);
        if before != "none" {
            event(handle, &ws, cx, &worker, json!({ "event": before }));
        }
        let task = json!({ "block": them, "task": "think", "wait": true });
        let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
        screen_with(&ws, cx, theirs, "think");
        enter(cx);
        if before == "started" {
            // Its turn starts: `UserPromptSubmit`.
            event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
        }
        after(cx, 2 * confirm);
        // The screen gone still may end a working agent's task: never as untaken.
        if let Ok(line) = answers.try_recv() {
            let (_, text) = result_of(&line);
            assert!(!text.contains("didn't take"), "{before}: {text}");
        }
    }
}
