//! Requests that wait for the person: an agent's question (`ask_user`), its request to connect,
//! and the balloon they open (docs/specs/connections.md, Requests from agents).

use std::sync::mpsc::TryRecvError;

use gpui_kit::Focusable as _;
use gpui_kit::component::input::InputState;
use serde_json::{Value, json};

use super::bus::{after, answered, bus_note, call, call_later, event, me};
use super::*;
use crate::workspace::Waiting;

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

/// The person picks option `i` of question `q` of the question `key`.
fn pick(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    key: u64,
    q: usize,
    i: usize,
) {
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.pick_option(key, q, i, window, cx))
    })
    .unwrap();
}

/// The balloons drawn, and long enough ago for presses to count (`TOO_SOON`).
fn shown(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    after(cx, 600);
}

/// The field of the question `key`: question `q`'s Other field, or with None its note.
fn field_of(ws: &Workspace, key: u64, field: Option<usize>) -> Entity<InputState> {
    let wait = ws.waits.iter().find(|w| w.key == key).expect("the wait");
    match (&wait.what, field) {
        (Waiting::Question { others, .. }, Some(q)) => others[q].0.clone(),
        (
            Waiting::Question {
                note: Some(note), ..
            },
            None,
        ) => note.0.clone(),
        _ => panic!("no such field"),
    }
}

/// The person types `text` in a field of the question `key` (`field_of`), the dialog drawn.
fn type_into(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    key: u64,
    field: Option<usize>,
    text: &str,
) {
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        let input = field_of(ws.read(cx), key, field);
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
        window.input(text, cx);
    })
    .unwrap();
}

/// Clicks the balloon's element `what` on block `n`, drawn first and scrolled into view.
fn click_part(handle: AnyWindowHandle, cx: &mut TestAppContext, n: usize, what: &str) {
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        scroll_to(window, cx, n, what);
        window.click(crate::balloon::part(n, what), cx);
    })
    .unwrap();
}

/// Scrolls the balloon of block `n` until its element `what` shows: it scrolls past 80% of its
/// terminal's height.
fn scroll_to(window: &mut Window, cx: &mut gpui_kit::App, n: usize, what: &str) {
    window.render_frame(cx);
    let target = window.find(crate::balloon::part(n, what)).bounds();
    let balloon = window.find(("balloon", n)).bounds();
    let by = if target.bottom() > balloon.bottom() {
        balloon.bottom() - target.bottom() - px(8.0)
    } else if target.top() < balloon.top() {
        balloon.top() - target.top() + px(8.0)
    } else {
        return;
    };
    // At its left edge: a preview pane, on the right, keeps its own wheel.
    let at = (
        f32::from(balloon.left()) + 4.0,
        f32::from(balloon.center().y),
    );
    wheel(
        window,
        cx,
        at,
        ScrollDelta::Pixels(point(px(0.0), by)),
        false,
    );
    window.render_frame(cx);
}

/// Whether the balloon of block `n` draws `what`.
fn drawn(handle: AnyWindowHandle, cx: &mut TestAppContext, n: usize, what: &str) -> bool {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(crate::balloon::part(n, what)).is_some()
    })
    .unwrap()
}

/// The spec's call: two questions, the second a multiSelect.
fn two_questions() -> Value {
    json!({
        "questions": [
            {
                "question": "Which database should the service use?",
                "header": "Database",
                "options": [
                    { "label": "Postgres", "description": "What production runs",
                      "preview": "DATABASE_URL=postgres://…\npool = 10" },
                    { "label": "SQLite", "description": "One file, no server" }
                ]
            },
            {
                "question": "Which features go in the first release?",
                "header": "Features",
                "options": [{ "label": "Auth" }, { "label": "Search" }, { "label": "Export" }],
                "multiSelect": true
            }
        ]
    })
}

/// The JSON of an answer.
fn json_of(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| panic!("JSON: {text}"))
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
    // The one shown is cancelled: it stays, its answer to be typed back.
    bus_note(
        handle,
        &ws,
        cx,
        1,
        "notifications/cancelled",
        json!({ "requestId": 1 }),
    );
    assert_eq!(shown(cx), Some(("One?".to_string(), 3)));
    // Its agent leaves: withdrawn, the next comes, counted again.
    bus_note(handle, &ws, cx, 1, templum_mcp::bus::CLOSED, Value::Null);
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
    pick(handle, &ws, cx, key, 0, 0);
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
    assert_eq!(dot_text(&ws, cx, first), None);
}

#[gpui_kit::test]
fn a_question_without_options_takes_what_the_person_types(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let asked = json!({ "question": "When?" });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let key = last_wait(&ws, cx);
    let send = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.send_question(key, window, cx))
        })
        .unwrap();
    };
    type_into(handle, &ws, cx, key, Some(0), "   ");
    send(cx);
    assert!(answers.try_recv().is_err(), "a blank answer isn't sent");
    type_into(handle, &ws, cx, key, Some(0), "Friday");
    send(cx);
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
    pick(handle, &ws, cx, key, 0, 0);
    after(cx, 2000);
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
    assert!(answers.try_recv().is_err(), "answered once");
}

/// The asking terminal runs an agent that doesn't take notices yet: a notice to it waits in
/// `Workspace::notices`, to be read.
fn agent_without_notices(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) {
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().seen.agent = Some("claude".into())
    });
}

/// The notices waiting to be typed: to whom, and what.
fn notices(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<(String, String)> {
    ws.read_with(cx, |ws, _| {
        ws.notices
            .iter()
            .map(|n| (n.to.clone(), n.text.clone()))
            .collect()
    })
}

/// The oldest wait is a question its client stopped waiting for.
fn stopped(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> bool {
    ws.read_with(cx, |ws, _| {
        matches!(
            ws.waits.first().map(|w| &w.what),
            Some(Waiting::Question { stopped: true, .. })
        )
    })
}

#[gpui_kit::test]
fn a_cancelled_question_stays_and_its_answer_is_typed_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let n = mine.0 as usize;
    let question = json!({ "question": "Ship it?", "options": ["Yes", "No"] });
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
    assert!(!stopped(&ws, cx));
    assert!(!drawn(handle, cx, n, "stopped"));
    let cancel = json!({ "requestId": 3, "reason": "The person pressed Esc" });
    bus_note(handle, &ws, cx, 3, "notifications/cancelled", cancel);
    ws.read_with(cx, |ws, _| assert_eq!(ws.waits.len(), 2));
    assert!(stopped(&ws, cx));
    assert!(drawn(handle, cx, n, "stopped"), "the dialog says so");
    assert_eq!(first.try_recv(), Err(TryRecvError::Empty));
    click_part(handle, cx, n, "q0-option-0");
    assert_eq!(
        notices(&ws, cx),
        [(
            terminal.clone(),
            "[answer to your question \"Ship it?\"] Yes".to_string()
        )]
    );
    ws.read_with(cx, |ws, _| assert_eq!(ws.waits.len(), 1));
    assert_eq!(
        first.try_recv(),
        Err(TryRecvError::Disconnected),
        "MCP sends no answer to a cancelled call"
    );
}

#[gpui_kit::test]
fn dismissing_a_stopped_question_types_nothing(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    bus_note(
        handle,
        &ws,
        cx,
        1,
        "notifications/cancelled",
        json!({ "requestId": 1 }),
    );
    click_part(handle, cx, n, "dismiss");
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
    assert!(notices(&ws, cx).is_empty());
    assert_eq!(asked.try_recv(), Err(TryRecvError::Disconnected));
}

/// A cancelled question whose agent quits, or whose block closes, is withdrawn: nothing is typed,
/// nothing answered.
#[gpui_kit::test]
fn a_stopped_question_goes_with_its_agent(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let quits = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked.clone());
    bus_note(
        handle,
        &ws,
        cx,
        1,
        "notifications/cancelled",
        json!({ "requestId": 1 }),
    );
    bus_note(handle, &ws, cx, 1, templum_mcp::bus::CLOSED, Value::Null);
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
    assert!(notices(&ws, cx).is_empty());
    assert_eq!(quits.try_recv(), Err(TryRecvError::Disconnected));
    let closes = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    bus_note(
        handle,
        &ws,
        cx,
        2,
        "notifications/cancelled",
        json!({ "requestId": 2 }),
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(mine, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
    assert!(notices(&ws, cx).is_empty());
    assert_eq!(
        closes.try_recv(),
        Err(TryRecvError::Disconnected),
        "no answer to a cancelled call"
    );
}

#[gpui_kit::test]
fn a_stopped_questions_deadline_drops_it_silently(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"], "timeout": 1 });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    bus_note(
        handle,
        &ws,
        cx,
        1,
        "notifications/cancelled",
        json!({ "requestId": 1 }),
    );
    after(cx, 1100);
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
    assert!(notices(&ws, cx).is_empty());
    assert_eq!(answers.try_recv(), Err(TryRecvError::Disconnected));
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
    super::terminals::set_active(handle, true, cx);
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
    let alert = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(mine).unwrap().alert.clone())
    };
    assert_eq!(alert(cx), None, "seen: no alert");
    // Not the block the person works in either.
    ws.update(cx, |ws, _| ws.space.focused = None);
    let asked = json!({ "question": "There?", "options": ["Yes"] });
    let _there = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before + 1);
    // An alert too, as a bell's: its workspace's dot, and Ctrl+Shift+A goes there.
    assert_eq!(
        alert(cx),
        Some(crate::alert::Alert::Attention(Some(
            "Asks you: There?".into()
        )))
    );
    let id = ws.read_with(cx, |ws, _| ws.space.id.clone());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, _| ws.sidebar = true);
        window.render_frame(cx);
        assert!(
            window
                .try_find(gpui_kit::ElementId::Name(
                    format!("workspace-alert-{id}").into()
                ))
                .is_some(),
            "the workspace's dot"
        );
        ws.update(cx, |ws, cx| ws.go_to_waiting(window, cx));
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.focused, Some(mine), "revealed")
    });
    assert_eq!(alert(cx), None);
}

/// The notification counts the call's other questions; it says the same as the alert.
#[gpui_kit::test]
fn a_notification_counts_the_other_questions(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    super::terminals::set_active(handle, true, cx);
    ws.update(cx, |ws, _| {
        ws.space.focused = None;
        ws.space.canvas.get_mut(mine).unwrap().rect.x += 100_000.0;
    });
    let before = notification_count(handle, cx);
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    assert_eq!(notification_count(handle, cx), before + 1);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.space.terminal(mine).unwrap().alert.clone()),
        Some(crate::alert::Alert::Attention(Some(
            "Asks you: Which database should the service use? (and 1 more)".into()
        )))
    );
}

/// A sound for a question or a request to connect, seen or not, at most once every 2 seconds.
#[gpui_kit::test]
fn a_request_chimes_once_every_two_seconds(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let chimed = |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.last_chime);
    assert_eq!(chimed(cx), None);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let _one = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked.clone());
    let first = chimed(cx).expect("a sound");
    let _two = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    assert_eq!(chimed(cx), Some(first), "not twice within 2 s");
    after(cx, 2100);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let _connect = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        3,
        "connect",
        json!({ "block": k }),
    );
    assert!(
        chimed(cx).is_some_and(|at| at > first),
        "a request to connect too"
    );
}

#[gpui_kit::test]
fn no_chime_with_the_sound_off(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    ws.update(cx, |ws, _| ws.config.connections.sound = false);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    ws.read_with(cx, |ws, _| assert_eq!(ws.last_chime, None));
}

/// The person doesn't see a question in view: the window is in the background, or the asking
/// block is too small on screen for its dialog to be drawn. A notification says what it asks.
#[gpui_kit::test]
fn a_question_the_person_cant_see_is_a_notification(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    super::terminals::set_active(handle, true, cx);
    // Zoomed out: in view, drawn as bars, no dialog.
    ws.update(cx, |ws, _| {
        ws.space.focused = None;
        ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), 0.33);
    });
    frame(handle, cx);
    let before = notification_count(handle, cx);
    let asked = json!({ "question": "Far?", "options": ["Yes"] });
    let _far = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before + 1);
    // Up close again, with the window in the background.
    ws.update(cx, |ws, _| {
        ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), 1.0)
    });
    frame(handle, cx);
    let asked = json!({ "question": "Near?", "options": ["Yes"] });
    let _near = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before + 1, "in view");
    super::terminals::set_active(handle, false, cx);
    let asked = json!({ "question": "Away?", "options": ["Yes"] });
    let _away = call_later(handle, &ws, cx, &terminal, 3, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before + 2);
}

/// A question is a dialog centered on the asking terminal, 80% of its width and at most 80% of its
/// height, that says its agent waits for an answer and shows every option whole, wrapped on its
/// lines. It scales with the zoom like a photo: the same lines at every zoom.
#[gpui_kit::test]
fn a_question_is_a_dialog_over_the_asking_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let part = crate::balloon::part;
    let long = "Yes, and rewrite each test of the parser to say what it checks. ".repeat(3);
    let asked = json!({ "question": "Ship it?", "options": ["No", long] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let at = |cx: &mut TestAppContext, zoom: f32| {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera = Camera::at(Point::new(40.0, 400.0), zoom)
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(part(n, "waiting")).is_some(), "it says so");
            let bounds = |id: gpui_kit::ElementId| window.find(id).bounds();
            (
                bounds(("block", n).into()),
                bounds(("balloon", n).into()),
                bounds(part(n, "q0-option-0")),
                bounds(part(n, "q0-option-1")),
            )
        })
        .unwrap()
    };
    let close =
        |a: gpui_kit::Pixels, b: gpui_kit::Pixels| (f32::from(a) - f32::from(b)).abs() <= 2.0;
    let mut shapes = Vec::new();
    for zoom in [1.0, 1.5] {
        let (block, dialog, short, long) = at(cx, zoom);
        let title = px(chrome::TITLE_H * zoom);
        let body = block.size.height - title;
        assert!(
            close(dialog.size.width, block.size.width * 0.8),
            "{dialog:?}"
        );
        assert!(close(dialog.center().x, block.center().x), "{dialog:?}");
        assert!(
            close(dialog.center().y, block.top() + title + body / 2.0),
            "{dialog:?}"
        );
        assert!(dialog.size.height <= body * 0.8 + px(1.0), "{dialog:?}");
        assert!(
            long.left() >= dialog.left() && long.right() <= dialog.right(),
            "whole: {long:?} in {dialog:?}"
        );
        assert!(
            long.size.height > short.size.height * 1.5,
            "wrapped: {long:?}"
        );
        shapes.push(f32::from(long.size.height) / f32::from(dialog.size.width));
    }
    assert!((shapes[0] - shapes[1]).abs() < 0.02, "{shapes:?}");
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

/// Claude Code's own notification about the same dialog comes about six seconds after it opens:
/// the dot keeps its "Allow …?", and allowing the tool afterwards still makes it work again.
#[gpui_kit::test]
fn a_templum_tool_allowed_after_its_notification_works_again(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let hook = |input: Value| serde_json::to_value(templum_mcp::hooks::claude(&input)).unwrap();
    let tool = "mcp__templum__wait_for_idle";
    let asks = json!({ "hook_event_name": "PermissionRequest", "tool_name": tool });
    event(handle, &ws, cx, &terminal, hook(asks));
    let state = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(first).unwrap().state.clone())
    };
    let asking = state(cx);
    let notified = json!({
        "hook_event_name": "Notification",
        "notification_type": "permission_prompt",
        "message": format!("Claude needs your permission to use {tool}"),
    });
    event(handle, &ws, cx, &terminal, hook(notified.clone()));
    assert_eq!(state(cx), asking);
    let waits = json!({ "block": "k9x1d" });
    call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", waits);
    assert_eq!(state(cx), Some(crate::workspace::AgentState::Working));
    // The notification alone (its `PermissionRequest` hook didn't run) still says it waits.
    event(handle, &ws, cx, &terminal, hook(notified));
    assert_eq!(
        state(cx),
        Some(crate::workspace::AgentState::NeedsYou(None))
    );
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
            Some((there.clone(), kanban)),
            "the kanban's id here"
        )
    });
    let before = notification_count(handle, cx);
    let asked = json!({ "question": "Here?", "options": ["Yes"] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    assert_eq!(notification_count(handle, cx), before + 1);
    // Its tile's dot lights, and Ctrl+Shift+A goes there.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, _| ws.sidebar = true);
        window.render_frame(cx);
        let dot = format!("workspace-alert-{there}");
        assert!(
            window
                .try_find(gpui_kit::ElementId::Name(dot.into()))
                .is_some()
        );
        ws.update(cx, |ws, cx| ws.go_to_waiting(window, cx));
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.id, there);
        assert_eq!(
            ws.space.focused,
            Some(kanban),
            "the terminal, by its id there"
        );
    });
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
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(part(n, "count")).is_some(), "1 of 2");
        window.click(part(n, "q0-option-1"), cx);
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

/// The balloon's presses never reach a program under it that asked for the mouse: a right one
/// opens no block menu either, and a middle drag pans the canvas, as anywhere.
#[gpui_kit::test]
fn a_press_on_the_balloon_stays_in_it_and_a_middle_drag_pans(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    #[cfg(unix)]
    super::terminals::program_asks("\x1b[?1002h\x1b[?1006h", handle, &ws, cx, |m| m.mouse);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let camera = ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let corner = window.find(("balloon", n)).bounds().origin;
        let at = (f32::from(corner.x) + 4.0, f32::from(corner.y) + 4.0);
        let (position, modifiers) = (point(px(at.0), px(at.1)), Modifiers::default());
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        // A program's press would be held until its release.
        assert!(ws.read(cx).drag.is_none(), "the program heard it");
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Right,
                position,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
        let to = (at.0 + 100.0, at.1);
        press_with(window, cx, MouseButton::Middle, modifiers, at, to);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.menu.is_none(), "no menu");
        assert_ne!(ws.space.canvas.camera, camera, "a middle drag pans");
    });
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
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let yes = at(window, "q0-option-0");
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

/// A request withdrawn between a button's press and its release: the next one's button, drawn in
/// its place, isn't clicked by the release.
#[gpui_kit::test]
fn a_press_on_a_withdrawn_requests_button_answers_no_other(cx: &mut TestAppContext) {
    use gpui_kit::{MouseDownEvent, MouseUpEvent, PlatformInput};
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    // Three: the next one shows "1 of 2" as the first showed "1 of 3", its Yes in the same place.
    let _first = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked.clone());
    let next = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked.clone());
    let _last = call_later(handle, &ws, cx, &terminal, 3, "ask_user", asked);
    let yes = |window: &mut Window| {
        window
            .find(crate::balloon::part(n, "q0-option-0"))
            .bounds()
            .center()
    };
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let position = yes(window);
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
    })
    .unwrap();
    // Its agent leaves (a cancelled question would stay).
    bus_note(handle, &ws, cx, 1, templum_mcp::bus::CLOSED, Value::Null);
    // Long enough for a press on the next one to count, were it a click on it.
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let position = yes(window);
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
            }),
            cx,
        );
    })
    .unwrap();
    assert_eq!(next.try_recv(), Err(TryRecvError::Empty), "it waits");
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
    })
    .unwrap();
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        let key = ws.read(cx).waits[0].key;
        let input = field_of(ws.read(cx), key, Some(0));
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

#[test]
fn deadline_takes_the_shorter() {
    use crate::workspace::waits::deadline;
    assert_eq!(deadline(None, 0), None);
    assert_eq!(deadline(Some(30), 0), Some(30));
    assert_eq!(deadline(None, 1), Some(60));
    assert_eq!(deadline(Some(120), 1), Some(60));
    assert_eq!(deadline(Some(30), 1), Some(30));
}

#[gpui_kit::test]
fn a_questions_call_answers_json(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    let key = last_wait(&ws, cx);
    for part in ["q0-option-1", "q1-option-0", "q1-option-2"] {
        click_part(handle, cx, n, part);
    }
    assert!(answers.try_recv().is_err(), "it waits for Send");
    type_into(handle, &ws, cx, key, None, "keep it small");
    click_part(handle, cx, n, "send");
    let (failed, text) = answered(&answers);
    assert!(!failed, "{text}");
    assert_eq!(
        json_of(&text),
        json!({
            "answers": {
                "Which database should the service use?": "SQLite",
                "Which features go in the first release?": ["Auth", "Export"],
            },
            "note": "keep it small",
        })
    );
}

#[gpui_kit::test]
fn send_waits_for_every_question(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    click_part(handle, cx, n, "q0-option-0");
    click_part(handle, cx, n, "send");
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "it waits");
    assert!(drawn(handle, cx, n, "send"));
}

#[gpui_kit::test]
fn todays_call_draws_todays_dialog(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "Not yet"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    for part in ["q0-option-0", "q0-option-1", "dismiss"] {
        assert!(drawn(handle, cx, n, part), "{part}");
    }
    for part in ["q0-other", "note", "send"] {
        assert!(!drawn(handle, cx, n, part), "{part}");
    }
    click_part(handle, cx, n, "q0-option-1");
    assert_eq!(answered(&answers), (false, "Not yet".to_string()));
    let answers = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "ask_user",
        json!({ "question": "When?" }),
    );
    let key = last_wait(&ws, cx);
    for part in ["q0-other", "send", "dismiss"] {
        assert!(drawn(handle, cx, n, part), "{part}");
    }
    assert!(!drawn(handle, cx, n, "note"));
    type_into(handle, &ws, cx, key, Some(0), "   ");
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    assert!(answers.try_recv().is_err(), "a blank answer isn't sent");
    type_into(handle, &ws, cx, key, Some(0), "Friday");
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    assert_eq!(answered(&answers), (false, "Friday".to_string()));
}

#[gpui_kit::test]
fn one_single_select_question_answers_on_a_click(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = json!({ "questions": [{ "question": "When?", "options": ["Now", "Later"] }] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let key = last_wait(&ws, cx);
    type_into(handle, &ws, cx, key, None, "after lunch");
    click_part(handle, cx, n, "q0-option-1");
    let (failed, text) = answered(&answers);
    assert!(!failed, "{text}");
    assert_eq!(
        json_of(&text),
        json!({ "answers": { "When?": "Later" }, "note": "after lunch" })
    );
}

#[gpui_kit::test]
fn dismiss_refuses_the_question(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    click_part(handle, cx, n, "dismiss");
    assert_eq!(
        answered(&answers),
        (
            true,
            "The person dismissed the question without answering.".to_string()
        )
    );
    ws.read_with(cx, |ws, _| assert!(ws.waits.is_empty()));
}

#[gpui_kit::test]
fn a_question_waits_without_a_deadline_by_default(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    after(cx, 7_200_000);
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "it waits");
    ws.read_with(cx, |ws, _| assert_eq!(ws.waits.len(), 1));
}

#[gpui_kit::test]
fn the_setting_sets_the_deadline_unless_the_call_is_shorter(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    ws.update(cx, |ws, _| ws.config.connections.question_timeout = 1);
    let ask = |timeout: Option<u64>| match timeout {
        Some(t) => json!({ "question": "Ship it?", "options": ["Yes"], "timeout": t }),
        None => json!({ "question": "Ship it?", "options": ["Yes"] }),
    };
    let short = call_later(handle, &ws, cx, &terminal, 1, "ask_user", ask(Some(30)));
    let long = call_later(handle, &ws, cx, &terminal, 2, "ask_user", ask(Some(120)));
    let none = call_later(handle, &ws, cx, &terminal, 3, "ask_user", ask(None));
    after(cx, 29_000);
    assert!(short.try_recv().is_err(), "not yet");
    after(cx, 2_000);
    let (failed, text) = answered(&short);
    assert!(failed && text.contains("within 30 seconds"), "{text}");
    after(cx, 28_000);
    assert!(
        long.try_recv().is_err() && none.try_recv().is_err(),
        "not yet"
    );
    after(cx, 2_000);
    for answers in [long, none] {
        let (failed, text) = answered(&answers);
        assert!(failed && text.contains("within 60 seconds"), "{text}");
    }
}

#[gpui_kit::test]
fn a_setting_changed_later_keeps_a_questions_deadline(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let asked = json!({ "question": "Ship it?", "options": ["Yes"] });
    let before = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked.clone());
    ws.update(cx, |ws, _| ws.config.connections.question_timeout = 1);
    after(cx, 90_000);
    assert_eq!(before.try_recv(), Err(TryRecvError::Empty), "it waits");
    let now = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    after(cx, 61_000);
    let (failed, text) = answered(&now);
    assert!(failed && text.contains("withdrawn"), "{text}");
    assert_eq!(
        before.try_recv(),
        Err(TryRecvError::Empty),
        "it still waits"
    );
}

#[gpui_kit::test]
fn two_calls_from_one_block_keep_their_own_answers(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let asked = json!({ "questions": [{ "question": "Name?" }] });
    let first = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked.clone());
    let second = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    let shown = ws.read_with(cx, |ws, _| ws.waits[0].key);
    type_into(handle, &ws, cx, shown, Some(0), "first");
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    assert_eq!(
        json_of(&answered(&first).1),
        json!({ "answers": { "Name?": "first" } })
    );
    let next = ws.read_with(cx, |ws, cx| {
        let wait = &ws.waits[0];
        assert_ne!(wait.key, shown);
        field_of(ws, wait.key, Some(0)).read(cx).value().to_string()
    });
    assert_eq!(next, "", "its own field");
    let key = last_wait(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.send_question(key, window, cx))
    })
    .unwrap();
    assert_eq!(second.try_recv(), Err(TryRecvError::Empty), "nothing typed");
}

/// Every size is a world size × the zoom: at twice the zoom, an option and the note are twice as
/// wide, but for the 1 px frames of the block and the balloon.
#[gpui_kit::test]
fn the_dialog_scales_with_the_zoom(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    let widths = |cx: &mut TestAppContext, zoom: f32| {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera = Camera::at(Point::new(40.0, 400.0), zoom)
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let width = |what: &str| {
                f32::from(
                    window
                        .find(crate::balloon::part(n, what))
                        .bounds()
                        .size
                        .width,
                )
            };
            (width("q0-option-0"), width("note"))
        })
        .unwrap()
    };
    let (option, note) = widths(cx, 1.0);
    let (option2, note2) = widths(cx, 2.0);
    assert!((option2 - 2.0 * option).abs() <= 4.0, "{option} {option2}");
    assert!((note2 - 2.0 * note).abs() <= 4.0, "{note} {note2}");
}

/// Hovering an option highlights it, and the preview beside the options is its own.
#[gpui_kit::test]
fn hovering_an_option_shows_its_preview(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    let preview = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| match &ws.waits[0].what {
            Waiting::Question { ask, form, .. } => (
                form.highlight,
                ask.preview_of(0, form).map(|p| p.map(str::to_string)),
            ),
            _ => panic!("a question"),
        })
    };
    assert!(drawn(handle, cx, n, "q0-preview"));
    assert!(!drawn(handle, cx, n, "q1-preview"), "no previews there");
    let (_, shown) = preview(cx);
    assert!(
        shown.flatten().unwrap().starts_with("DATABASE_URL"),
        "the first one's"
    );
    cx.update_window(handle, |_, window, cx| {
        scroll_to(window, cx, n, "q0-option-1");
        window.hover(crate::balloon::part(n, "q0-option-1"), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        preview(cx),
        (Some(templum_mcp::ask::Row::Option(0, 1)), Some(None)),
        "No preview"
    );
}

/// A preview of 2000 lines stays 18 lines tall, scrolling in its pane.
#[gpui_kit::test]
fn a_long_preview_scrolls_in_its_pane(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let long: Vec<String> = (0..2000).map(|i| format!("line {i}")).collect();
    let asked = json!({ "questions": [{ "question": "Which?", "options": [
        { "label": "Long", "preview": long.join("\n") }, { "label": "Short" }
    ] }] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    for zoom in [1.0, 2.0] {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera = Camera::at(Point::new(40.0, 400.0), zoom)
        });
        let height = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                f32::from(
                    window
                        .find(crate::balloon::part(n, "q0-preview"))
                        .bounds()
                        .size
                        .height,
                )
            })
            .unwrap();
        // 18 lines of 16, its padding of 8 above and below, its 1 px frame.
        assert!(
            height <= (18.0 * 16.0 + 16.0) * zoom + 2.0,
            "{height} at {zoom}"
        );
        assert!(height >= 18.0 * 16.0 * zoom, "{height} at {zoom}");
    }
}

/// The asking block takes the keyboard, its dialog is drawn, and long enough ago for keys to
/// count.
fn focus_dialog(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx))
    })
    .unwrap();
    shown(handle, cx);
}

/// The person presses `keys`, the window drawn after each.
fn press(handle: AnyWindowHandle, cx: &mut TestAppContext, keys: &[&str]) {
    cx.update_window(handle, |_, window, cx| {
        for key in keys {
            window.press(key, cx);
            window.render_frame(cx);
        }
    })
    .unwrap();
}

/// The highlight of the newest question.
fn highlight(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Option<templum_mcp::ask::Row> {
    ws.read_with(cx, |ws, _| match &ws.waits.last().unwrap().what {
        Waiting::Question { form, .. } => form.highlight,
        _ => panic!("a question"),
    })
}

/// A `questions` call of `n` single-select questions ("Q0", …) of two options ("A", "B").
fn singles(n: usize) -> Value {
    let questions: Vec<Value> = (0..n)
        .map(|q| json!({ "question": format!("Q{q}"), "options": ["A", "B"] }))
        .collect();
    json!({ "questions": questions })
}

/// What the terminal of block `id` shows, once its program had time to echo.
#[cfg(unix)]
fn screen(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> String {
    std::thread::sleep(std::time::Duration::from_millis(300));
    ws.read_with(cx, |ws, _| {
        let mut snap = templum_terminal::Snapshot::default();
        snap.refresh(
            ws.space.terminal(id).unwrap().term.shared(),
            &ws.theme.palette,
        );
        snap.text()
    })
}

#[gpui_kit::test]
fn the_keys_answer_a_question(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "No"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    focus_dialog(handle, &ws, cx, mine);
    press(handle, cx, &["down", "down", "enter"]);
    assert_eq!(answered(&answers), (false, "No".to_string()));
}

/// No row is highlighted as the dialog opens: Space and Enter meant for the terminal answer
/// nothing.
#[gpui_kit::test]
fn enter_with_nothing_highlighted_does_nothing(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "No"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    focus_dialog(handle, &ws, cx, mine);
    press(handle, cx, &["enter", "space"]);
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "it waits");
    assert_eq!(highlight(&ws, cx), None);
}

#[gpui_kit::test]
fn space_checks_and_enter_sends_once_all_are_answered(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    focus_dialog(handle, &ws, cx, mine);
    // SQLite, then on to the second question.
    press(handle, cx, &["down", "down", "enter"]);
    assert_eq!(
        highlight(&ws, cx),
        Some(templum_mcp::ask::Row::Option(1, 0))
    );
    // Auth and Export.
    press(handle, cx, &["space", "down", "down", "space"]);
    assert_eq!(
        answers.try_recv(),
        Err(TryRecvError::Empty),
        "Space never sends"
    );
    press(handle, cx, &["enter"]);
    let (failed, text) = answered(&answers);
    assert!(!failed, "{text}");
    assert_eq!(
        json_of(&text),
        json!({ "answers": {
            "Which database should the service use?": "SQLite",
            "Which features go in the first release?": ["Auth", "Export"],
        } })
    );
}

#[gpui_kit::test]
fn enter_moves_to_the_next_unanswered_question(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", singles(2));
    focus_dialog(handle, &ws, cx, mine);
    press(handle, cx, &["down", "enter"]);
    assert_eq!(
        highlight(&ws, cx),
        Some(templum_mcp::ask::Row::Option(1, 0))
    );
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "it waits");
}

#[gpui_kit::test]
fn tab_reaches_other_and_the_note(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", singles(1));
    let key = last_wait(&ws, cx);
    focus_dialog(handle, &ws, cx, mine);
    press(handle, cx, &["tab", "tab", "tab"]);
    cx.update_window(handle, |_, window, cx| {
        let other = field_of(ws.read(cx), key, Some(0));
        assert!(
            other.read(cx).focus_handle(cx).is_focused(window),
            "Other's field has the keyboard"
        );
        window.input("Friday", cx);
    })
    .unwrap();
    press(handle, cx, &["tab"]);
    cx.update_window(handle, |_, window, cx| {
        let note = field_of(ws.read(cx), key, None);
        assert!(
            note.read(cx).focus_handle(cx).is_focused(window),
            "then the note's"
        );
    })
    .unwrap();
    press(handle, cx, &["enter"]);
    assert_eq!(
        json_of(&answered(&answers).1),
        json!({ "answers": { "Q0": "Friday" } })
    );
}

/// Esc gives the keyboard back to the terminal, also once the block loses it and takes it again;
/// a click on the dialog takes it back.
#[cfg(unix)]
#[gpui_kit::test]
fn esc_gives_the_keyboard_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "No"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    focus_dialog(handle, &ws, cx, mine);
    press(handle, cx, &["escape"]);
    cx.update_window(handle, |_, window, cx| window.input("ls", cx))
        .unwrap();
    assert!(screen(&ws, cx, mine).contains("ls"), "the terminal's");
    focus_dialog(handle, &ws, cx, kanban);
    focus_dialog(handle, &ws, cx, mine);
    press(handle, cx, &["down"]);
    assert!(
        screen(&ws, cx, mine).contains("^[[B"),
        "still the terminal's"
    );
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "it waits");
    assert_eq!(highlight(&ws, cx), None);
    // A press on the dialog, not on a button.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(crate::balloon::part(n, "waiting"), cx);
        window.render_frame(cx);
    })
    .unwrap();
    press(handle, cx, &["down", "enter"]);
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
}

/// A key or click meant for the terminal as the dialog appears doesn't answer it.
#[gpui_kit::test]
fn keys_and_clicks_too_soon_do_nothing(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "No"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(mine, window, cx));
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        window.click(crate::balloon::part(n, "q0-option-0"), cx);
    })
    .unwrap();
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "too soon");
    after(cx, 600);
    // The pointer on the option highlights it.
    assert_eq!(
        highlight(&ws, cx),
        Some(templum_mcp::ask::Row::Option(0, 0))
    );
    press(handle, cx, &["enter"]);
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
}

/// While the dialog has the keyboard, its keys never reach the terminal: arrows, Space, Enter.
#[cfg(unix)]
#[gpui_kit::test]
fn keys_never_reach_the_terminal_while_the_dialog_has_them(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    focus_dialog(handle, &ws, cx, mine);
    press(handle, cx, &["down", "space", "x"]);
    cx.update_window(handle, |_, window, cx| window.input("ls", cx))
        .unwrap();
    let shown = screen(&ws, cx, mine);
    assert!(
        !shown.contains("^[[B") && !shown.contains("ls") && !shown.contains('x'),
        "{shown:?}"
    );
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "it waits");
}
