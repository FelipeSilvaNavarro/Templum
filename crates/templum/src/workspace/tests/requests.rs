//! Requests that wait for the person: an agent's question (`ask_user`), its request to connect,
//! and the balloon they open (docs/specs/connections.md, Requests from agents).

use std::sync::mpsc::TryRecvError;

use serde_json::{Value, json};

use super::bus::{after, answered, bus_note, call, call_later, event, me};
use super::*;

/// The newest wait's key.
fn last_wait(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> u64 {
    ws.read_with(cx, |ws, _| ws.waits.last().expect("a wait").key)
}

/// What the dot before terminal `id`'s title says.
fn dot_text(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> Option<String> {
    ws.read_with(cx, |ws, _| {
        crate::block::status_dot(ws, ws.space.terminal(id).unwrap())
            .map(|(_, text)| text.to_string())
    })
}

/// The person answers the question `key` with `text`.
fn reply_with(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    key: u64,
    text: &str,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.answer_question(key, text.to_string(), window, cx)
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_balloon_shows_the_oldest_request_and_counts_them(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    for (conn, question) in [(1, "One?"), (2, "Two?"), (3, "Three?")] {
        let asked = json!({ "question": question, "options": ["Yes"] });
        call_later(handle, &ws, cx, &terminal, conn, "ask_user", asked);
    }
    let shown = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.balloon(&terminal)
                .map(|(wait, count)| (wait.summary(), count))
        })
    };
    assert_eq!(shown(cx), Some(("One?".to_string(), 3)));
    // The one shown is withdrawn: the next comes, counted again.
    bus_note(
        handle,
        &ws,
        cx,
        1,
        "notifications/cancelled",
        json!({ "requestId": 1 }),
    );
    assert_eq!(shown(cx), Some(("Two?".to_string(), 2)));
}

#[gpui_kit::test]
fn ask_user_waits_for_the_persons_answer(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "Ship it?\nThe tests pass.", "options": ["Yes", "No"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    assert!(answers.try_recv().is_err(), "it waits for the person");
    assert_eq!(
        dot_text(&ws, cx, first).as_deref(),
        Some("Waiting for you: Ship it?")
    );
    let key = last_wait(&ws, cx);
    reply_with(handle, &ws, cx, key, "Yes");
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
    assert_eq!(dot_text(&ws, cx, first), None);
}

#[gpui_kit::test]
fn a_question_without_options_takes_what_the_person_types(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "When?" });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let key = last_wait(&ws, cx);
    let send = |text: &'static str, cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let input = match &ws.waits[0].what {
                    crate::workspace::waits::Waiting::Question {
                        field: Some((input, _)),
                        ..
                    } => input.clone(),
                    _ => panic!("a question without options has a field"),
                };
                input.update(cx, |input, cx| input.set_value(text, window, cx));
                ws.send_field(key, window, cx);
            })
        })
        .unwrap();
    };
    send("   ", cx);
    assert!(answers.try_recv().is_err(), "a blank answer isn't sent");
    send("Friday", cx);
    assert_eq!(answered(&answers), (false, "Friday".to_string()));
}

#[gpui_kit::test]
fn a_question_no_one_answers_is_withdrawn_at_its_timeout(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "Still there?", "options": ["Yes"], "timeout": 1 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    after(cx, 900);
    assert!(answers.try_recv().is_err(), "not yet");
    after(cx, 200);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("withdrawn"), "{text}");
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
}

#[gpui_kit::test]
fn an_answer_and_its_timeout_answer_once(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "Now?", "options": ["Yes"], "timeout": 1 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let key = last_wait(&ws, cx);
    reply_with(handle, &ws, cx, key, "Yes");
    after(cx, 2000);
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
    assert!(answers.try_recv().is_err(), "answered once");
}

#[gpui_kit::test]
fn a_cancelled_call_drops_its_question_unanswered(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let question = json!({ "question": "Ship it?", "options": ["Yes"] });
    let first = call_later(handle, &ws, cx, &terminal, 3, "ask_user", question.clone());
    let _second = call_later(handle, &ws, cx, &terminal, 4, "ask_user", question);
    // Request 3 of another connection is another call.
    bus_note(
        handle,
        &ws,
        cx,
        4,
        "notifications/cancelled",
        json!({ "requestId": 3 }),
    );
    ws.read_with(cx, |ws, _| assert_eq!(ws.waits.len(), 2));
    let cancel = json!({ "requestId": 3, "reason": "The person pressed Esc" });
    bus_note(handle, &ws, cx, 3, "notifications/cancelled", cancel);
    ws.read_with(cx, |ws, _| assert_eq!(ws.waits.len(), 1));
    assert_eq!(
        first.try_recv(),
        Err(TryRecvError::Disconnected),
        "MCP sends no answer to a cancelled call"
    );
}

#[gpui_kit::test]
fn an_agent_that_quits_takes_its_questions_away(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let ask = |text: &str| json!({ "question": text, "options": ["Yes"] });
    let _a = call_later(handle, &ws, cx, &terminal, 5, "ask_user", ask("One?"));
    let _b = call_later(handle, &ws, cx, &terminal, 5, "ask_user", ask("Two?"));
    let _c = call_later(handle, &ws, cx, &terminal, 6, "ask_user", ask("Three?"));
    bus_note(handle, &ws, cx, 5, templum_mcp::bus::CLOSED, Value::Null);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.waits.len(), 1);
        assert_eq!(ws.waits[0].conn, 6);
    });
    assert_eq!(
        dot_text(&ws, cx, first).as_deref(),
        Some("Waiting for you: Three?")
    );
}

#[gpui_kit::test]
fn closing_the_asking_block_withdraws_its_question(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(first, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("closed"), "{text}");
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
}

#[gpui_kit::test]
fn a_question_off_screen_is_a_notification(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let before = notification_count(handle, cx);
    let asked = json!({ "question": "Here?", "options": ["Yes"] });
    let _here = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    assert_eq!(
        notification_count(handle, cx),
        before,
        "the balloon is in view"
    );
    // The asking block far outside the view.
    ws.update(cx, |ws, _| {
        ws.space.canvas.get_mut(mine).unwrap().rect.x += 100_000.0;
    });
    let asked = json!({ "question": "There?", "options": ["Yes"] });
    let _there = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before + 1);
}

/// Claude Code asks its person to allow a Templum tool, then calls it: the person allowed it, and
/// the agent works again, though the tool may wait for minutes before its end says so.
#[gpui_kit::test]
fn a_templum_tool_the_person_allowed_works_again(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let hook =
        json!({ "hook_event_name": "PermissionRequest", "tool_name": "mcp__templum__ask_user" });
    let asks = serde_json::to_value(templum_mcp::hooks::claude(&hook).unwrap()).unwrap();
    event(handle, &ws, cx, &terminal, asks);
    let state = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(first).unwrap().state.clone())
    };
    let asking = state(cx);
    assert!(
        matches!(asking, Some(crate::workspace::AgentState::NeedsYou(_))),
        "{asking:?}"
    );
    // Another tool's call: the dialog may be another one's of its subagents.
    call_later(handle, &ws, cx, &terminal, 1, "whoami", json!({}));
    assert_eq!(state(cx), asking);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let _answer = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    assert_eq!(state(cx), Some(crate::workspace::AgentState::Working));
}

/// A block in another workspace asks: the person can't see it, though its id there is the one of a
/// block in view here (ids count from 0 in each workspace).
#[gpui_kit::test]
fn a_question_from_another_workspace_is_a_notification(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let there = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.move_blocks(vec![note], None, window, cx);
                let there = ws.others.last().unwrap().id.clone();
                ws.move_blocks(vec![mine], Some(&there), window, cx);
                there
            })
        })
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.caller(&terminal),
            Some((there, kanban)),
            "the kanban's id here"
        )
    });
    let before = notification_count(handle, cx);
    let asked = json!({ "question": "Here?", "options": ["Yes"] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before + 1);
}

#[gpui_kit::test]
fn allow_wires_the_blocks_and_deny_refuses(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (k, n) = ws.read_with(cx, |ws, _| {
        (ws.space.uids[&kanban].clone(), ws.space.uids[&note].clone())
    });
    let answers = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "connect",
        json!({ "block": k }),
    );
    assert!(answers.try_recv().is_err(), "it waits for the person");
    assert_eq!(
        dot_text(&ws, cx, mine),
        Some(format!("Waiting for you: connect to {k}"))
    );
    let key = last_wait(&ws, cx);
    ws.update(cx, |ws, cx| ws.answer_connect(key, true, cx));
    assert_eq!(answered(&answers), (false, format!("Connected to {k}.")));
    ws.read_with(cx, |ws, _| assert!(ws.space.wired(mine, kanban)));
    // Wired as by hand: Ctrl+Z takes it back.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(!ws.space.wired(mine, kanban)));
    let answers = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "connect",
        json!({ "block": n }),
    );
    let key = last_wait(&ws, cx);
    ws.update(cx, |ws, cx| ws.answer_connect(key, false, cx));
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("didn't connect you"), "{text}");
    ws.read_with(cx, |ws, _| assert!(!ws.space.wired(mine, note)));
}

/// The block asked for moved to another workspace while the person was asked: Allow can't wire
/// it from here.
#[gpui_kit::test]
fn allow_refuses_a_block_moved_to_another_workspace(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let ask = json!({ "block": k });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "connect", ask);
    let key = last_wait(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.move_blocks(vec![kanban], None, window, cx))
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.answer_connect(key, true, cx));
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("isn't in your workspace"), "{text}");
    ws.read_with(cx, |ws, _| {
        assert!(ws.spaces().all(|s| s.wires.is_empty()), "nothing wired")
    });
}

#[gpui_kit::test]
fn connect_answers_at_once_when_there_is_nothing_to_ask(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, mine) = three_blocks(handle, &ws, cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (k, n) = ws.read_with(cx, |ws, _| {
        (ws.space.uids[&kanban].clone(), ws.space.uids[&note].clone())
    });
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "connect",
        json!({ "block": uid }),
    );
    assert!(failed && text.contains("your own block"), "{text}");
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "connect",
        json!({ "block": "zzzzz" }),
    );
    assert!(failed && text.contains("no block zzzzz"), "{text}");
    ws.update(cx, |ws, cx| assert!(ws.connect(mine, kanban, cx)));
    let (failed, text) = call(handle, &ws, cx, &terminal, "connect", json!({ "block": k }));
    assert!(!failed && text.contains("already connected"), "{text}");
    // A second request for one block while the first waits.
    let _first = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "connect",
        json!({ "block": n }),
    );
    let second = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "connect",
        json!({ "block": n }),
    );
    let (failed, text) = answered(&second);
    assert!(failed && text.contains("already asked"), "{text}");
}

#[gpui_kit::test]
fn closing_the_block_asked_for_withdraws_the_request(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let answers = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "connect",
        json!({ "block": k }),
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(kanban, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    let (failed, text) = answered(&answers);
    assert!(failed && text.contains("was closed"), "{text}");
}

#[gpui_kit::test]
fn the_balloons_buttons_answer_a_question_then_a_request(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let n = mine.0 as usize;
    let part = crate::balloon::part;
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "Not yet"] });
    let question = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let request = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "connect",
        json!({ "block": k }),
    );
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(part(n, "count")).is_some(), "1 of 2");
        window.click(part(n, "option-1"), cx);
    })
    .unwrap();
    assert_eq!(answered(&question), (false, "Not yet".to_string()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(part(n, "count")).is_none(), "the last one");
        window.click(part(n, "allow"), cx);
    })
    .unwrap();
    assert_eq!(answered(&request), (false, format!("Connected to {k}.")));
    ws.read_with(cx, |ws, _| assert!(ws.space.wired(mine, kanban)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(("balloon", n)).is_none(),
            "nothing left to ask"
        );
    })
    .unwrap();
}

/// A double click answers one request: once the question is answered, the next request's buttons
/// are drawn under the pointer, and the second click mustn't choose for the person.
#[gpui_kit::test]
fn a_double_click_answers_one_request(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let n = mine.0 as usize;
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "Not yet"] });
    let question = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let request = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "connect",
        json!({ "block": k }),
    );
    let at = |window: &Window, what: &str| {
        let c = window.find(crate::balloon::part(n, what)).bounds().center();
        (f32::from(c.x), f32::from(c.y))
    };
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let yes = at(window, "option-0");
        click(window, cx, yes, 1);
        window.render_frame(cx);
        let allow = at(window, "allow");
        click(window, cx, allow, 2);
    })
    .unwrap();
    assert_eq!(answered(&question), (false, "Yes".to_string()));
    assert_eq!(request.try_recv(), Err(TryRecvError::Empty), "it waits");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let allow = at(window, "allow");
        click(window, cx, allow, 1);
    })
    .unwrap();
    assert_eq!(answered(&request), (false, format!("Connected to {k}.")));
}

/// Ctrl+L and Enter typed in the balloon's field while the asking terminal is focused: the field
/// takes them, the shell never sees them.
#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn keys_in_the_balloons_field_never_reach_the_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let answers = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "ask_user",
        json!({ "question": "Name?" }),
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(mine, window, cx));
        window.render_frame(cx);
        let input = match &ws.read(cx).waits[0].what {
            crate::workspace::Waiting::Question {
                field: Some((input, _)),
                ..
            } => input.clone(),
            _ => panic!("a question without options has a field"),
        };
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
        window.input("Ada", cx);
        window.press("ctrl-l", cx);
        window.press("enter", cx);
    })
    .unwrap();
    assert_eq!(answered(&answers), (false, "Ada".to_string()));
    // `cat`'s tty would have echoed Ctrl+L as ^L.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let shown = ws.read_with(cx, |ws, _| {
        let mut snap = templum_terminal::Snapshot::default();
        snap.refresh(
            ws.space.terminal(mine).unwrap().term.shared(),
            &ws.theme.palette,
        );
        snap.text()
    });
    assert!(!shown.contains("^L") && !shown.contains("Ada"), "{shown:?}");
}
