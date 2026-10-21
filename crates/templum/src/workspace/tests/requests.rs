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

/// The person presses Send on the question `key`, drawn (`shown`) before.
fn send(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext, key: u64) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.send_question(key, window, cx))
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

/// Scrolls the body of the balloon of block `n` until its element `what` shows: the body scrolls
/// past 80% of its terminal's height, between the header and the foot, which stay.
fn scroll_to(window: &mut Window, cx: &mut gpui_kit::App, n: usize, what: &str) {
    window.render_frame(cx);
    let target = window.find(crate::balloon::part(n, what)).bounds();
    let balloon = window.find(crate::balloon::part(n, "body")).bounds();
    // In the header or the foot, which stay in the balloon: nothing to scroll. In the body, a
    // part out of view lies past the balloon's edge.
    let frame = window.find(("balloon", n)).bounds();
    if frame.contains(&target.origin)
        && frame.contains(&target.bottom_right())
        && (target.top() >= balloon.bottom() || target.bottom() <= balloon.top())
    {
        return;
    }
    let by = if target.bottom() > balloon.bottom() {
        balloon.bottom() - target.bottom() - px(8.0)
    } else if target.top() < balloon.top() {
        balloon.top() - target.top() + px(8.0)
    } else {
        return;
    };
    // The pointer comes onto the balloon first, so its block has the keyboard: only a selected
    // block's content takes the wheel.
    window.hover(("balloon", n), cx);
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

/// Block `id` tall enough for a dialog of a question or two to show whole, as these tests click
/// its parts without scrolling the body: it scrolls past 80% of the terminal's height, between a
/// header and a foot that stay, which a 300-high block leaves little room for. The camera brings
/// the block to the window's top, where it fits.
fn roomy(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) {
    ws.update(cx, |ws, _| {
        let rect = &mut ws.space.canvas.get_mut(id).unwrap().rect;
        rect.h = 560.0;
        let at = Point::new(rect.x, rect.y);
        ws.space.canvas.camera = Camera::at(at, 1.0);
    });
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
    assert!(answers.try_recv().is_err(), "a pick is not an answer");
    send(handle, &ws, cx, key);
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
    send(handle, &ws, cx, key);
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
    roomy(&ws, cx, mine);
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
    click_part(handle, cx, n, "send");
    assert_eq!(
        notices(&ws, cx),
        [(
            terminal.clone(),
            "[answer to your question \"Ship it?\"] The answer is in the pasted text.\nYes"
                .to_string()
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
    ws.update(cx, |ws, cx| ws.answer_allow(key, true, cx));
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
    ws.update(cx, |ws, cx| ws.answer_allow(key, false, cx));
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
    ws.update(cx, |ws, cx| ws.answer_allow(key, true, cx));
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
    roomy(&ws, cx, mine);
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
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        question.try_recv(),
        Err(TryRecvError::Empty),
        "Send answers"
    );
    cx.update_window(handle, |_, window, cx| window.click(part(n, "send"), cx))
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
        let send = at(window, "send");
        click(window, cx, send, 1);
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
    ws.read_with(cx, |ws, _| match &ws.waits[0].what {
        Waiting::Question { form, .. } => assert!(!form.picked(0, 0), "nothing picked"),
        _ => panic!("a question"),
    });
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
    })
    .unwrap();
    // A key of its own, as the platform sends it: what was typed is heard first.
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
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
    for part in ["q0-option-0", "q0-option-1", "dismiss", "send"] {
        assert!(drawn(handle, cx, n, part), "{part}");
    }
    for part in ["q0-other", "note"] {
        assert!(!drawn(handle, cx, n, part), "{part}");
    }
    click_part(handle, cx, n, "q0-option-1");
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "Send answers");
    click_part(handle, cx, n, "send");
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

/// A click on an option, even of the only single-select question, only picks it: Send answers
/// (an accidental click mustn't).
#[gpui_kit::test]
fn a_click_on_an_option_only_picks_it_and_send_answers(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = json!({ "questions": [{ "question": "When?", "options": ["Now", "Later"] }] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let key = last_wait(&ws, cx);
    type_into(handle, &ws, cx, key, None, "after lunch");
    click_part(handle, cx, n, "send");
    assert_eq!(
        answers.try_recv(),
        Err(TryRecvError::Empty),
        "nothing to send"
    );
    click_part(handle, cx, n, "q0-option-0");
    click_part(handle, cx, n, "q0-option-1");
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "picked only");
    click_part(handle, cx, n, "send");
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

/// The scroll of the newest wait's body, pixels.
fn body_scroll(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> f32 {
    ws.read_with(cx, |ws, _| {
        f32::from(ws.waits.last().unwrap().body.handle.offset().y)
    })
}

/// A question of thousands of words and thirty options keeps its header and its foot in
/// the balloon, which is no taller than 80% of its terminal's area, and scrolls its body between
/// them: an option far down is reached, picked and sent, the header and the foot where they were.
#[gpui_kit::test]
fn a_huge_dialog_scrolls_its_body_between_a_header_and_a_foot_that_stay(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let part = crate::balloon::part;
    let words = "A question that goes on about the database. ".repeat(120);
    let options: Vec<Value> = (0..30)
        .map(|i| {
            json!({
                "label": format!("Option {i} {}", "with a long label ".repeat(2)),
                "description": "and a description that says more. ".repeat(2),
            })
        })
        .collect();
    let asked = json!({ "questions": [{
        "question": words, "header": "Database", "options": options
    }] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    shown(handle, cx);
    let frames = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let at = |id: gpui_kit::ElementId| window.find(id).bounds();
            [
                at(("balloon", n).into()),
                at(part(n, "waiting")),
                at(part(n, "body")),
                at(part(n, "send")),
            ]
        })
        .unwrap()
    };
    let before = frames(cx);
    let [balloon, head, body, send] = before;
    let area = 300.0 - chrome::TITLE_H;
    assert!(
        f32::from(balloon.size.height) <= area * 0.8 + 1.0,
        "{balloon:?}"
    );
    assert!(head.top() >= balloon.top() && head.bottom() <= body.top() + px(1.0));
    assert!(send.top() >= body.bottom() - px(1.0) && send.bottom() <= balloon.bottom());
    assert!(body.size.height > px(20.0), "the body keeps some room");
    // Far below the fold, in the body.
    assert!(
        !window_shows(handle, cx, n, "q0-option-29"),
        "the last option starts hidden"
    );
    click_part(handle, cx, n, "q0-option-29");
    assert!(body_scroll(&ws, cx) < -100.0, "the body scrolled");
    assert_eq!(frames(cx), before, "the header and the foot stay");
    click_part(handle, cx, n, "send");
    let (failed, text) = answered(&answers);
    assert!(!failed, "{text}");
    let answer = json_of(&text)["answers"][&words]
        .as_str()
        .unwrap()
        .to_string();
    assert!(answer.starts_with("Option 29 "), "{answer}");
}

/// A dialog in a short or a narrow block stays usable: where its terminal's area holds a header,
/// a foot and a body of 90 it keeps that body (the balloon takes the whole area before it gives
/// the body up); where even that is too short the whole frame scrolls. Send is never cut at the
/// side, and every size reaches an option and Send with the mouse.
#[gpui_kit::test]
fn a_dialog_in_a_small_block_keeps_its_body_and_reaches_send(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let min = ws.read_with(cx, |ws, _| chrome::min_block_size(ws.cell.size));
    // (width, height, whether the area holds the header, the foot and a body of 90)
    let sizes = [
        (min.w, min.h, false),
        (min.w, 150.0, false),
        (min.w, 220.0, false),
        (min.w, 300.0, false),
        (400.0, min.h, false),
        (400.0, 150.0, false),
        (400.0, 180.0, false),
        (400.0, 220.0, false),
        (400.0, 230.0, true),
        (400.0, 260.0, true),
        (400.0, 300.0, true),
    ];
    for (conn, (w, h, holds)) in sizes.into_iter().enumerate() {
        ws.update(cx, |ws, _| {
            let rect = &mut ws.space.canvas.get_mut(mine).unwrap().rect;
            (rect.w, rect.h) = (w, h);
            let at = Point::new(rect.x, rect.y);
            ws.space.canvas.camera = Camera::at(at, 1.0);
        });
        let asked = json!({ "question": "Ship it?", "options": ["Yes", "No"] });
        let answers = call_later(
            handle,
            &ws,
            cx,
            &terminal,
            conn as u64 + 1,
            "ask_user",
            asked,
        );
        shown(handle, cx);
        let [balloon, body, send] = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let at = |id: gpui_kit::ElementId| window.find(id).bounds();
                [
                    at(("balloon", n).into()),
                    at(crate::balloon::part(n, "body")),
                    at(crate::balloon::part(n, "send")),
                ]
            })
            .unwrap();
        let size = format!("{w} x {h}: {balloon:?} {body:?} {send:?}");
        assert!(
            f32::from(balloon.size.height) <= h - chrome::TITLE_H + 1.0,
            "the balloon fits its terminal, {size}"
        );
        assert!(
            send.left() >= balloon.left() && send.right() <= balloon.right(),
            "Send is cut at the side, {size}"
        );
        assert!(body.size.height > px(0.0), "{size}");
        if holds {
            assert!(body.size.height >= px(89.0), "the body keeps room, {size}");
            assert!(send.bottom() <= balloon.bottom(), "Send is cut, {size}");
        }
        click_part(handle, cx, n, "q0-option-1");
        click_part(handle, cx, n, "send");
        let (failed, text) = answered(&answers);
        assert!(!failed && text == "No", "{text}: {size}");
    }
}

/// The header, the foot and the stopped line are as tall as the balloon's fit assumes: where they
/// grow past it, the block that "holds" a body of 90 would cut it.
#[gpui_kit::test]
fn the_balloons_fixed_parts_are_as_tall_as_its_fit_assumes(cx: &mut TestAppContext) {
    use crate::balloon::{FOOT, FOOT_WRAPPED, HEAD, ONE_LINE, notice};
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    roomy(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let n = mine.0 as usize;
    let question = json!({ "question": "Ship it?", "options": ["Yes", "No"] });
    let _asked = call_later(handle, &ws, cx, &terminal, 3, "ask_user", question);
    let cancel = json!({ "requestId": 3, "reason": "The person pressed Esc" });
    bus_note(handle, &ws, cx, 3, "notifications/cancelled", cancel);
    let key = last_wait(&ws, cx);
    let heights = |cx: &mut TestAppContext, w: f32| {
        ws.update(cx, |ws, _| {
            ws.space.canvas.get_mut(mine).unwrap().rect.w = w
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let at = |id: gpui_kit::ElementId| f32::from(window.find(id).bounds().size.height);
            [
                at(crate::balloon::part(n, "waiting")),
                at(("balloon-foot", key).into()),
                at(crate::balloon::part(n, "stopped")),
            ]
        })
        .unwrap()
    };
    // A dialog just wide enough for the foot on one line, and wider ones: the stopped line wraps
    // on fewer lines (its margin, 12, is not in its height).
    for w in [ONE_LINE / 0.8, 400.0, 500.0, 900.0] {
        let [head, foot, stopped] = heights(cx, w);
        assert!((head, foot) == (HEAD, FOOT), "{w}: {head} {foot}");
        assert!(
            (stopped + 12.0 - notice(w * 0.8)).abs() < 1.0,
            "{w}: {stopped}"
        );
    }
    // And one narrower, which wraps the foot.
    let [_, wrapped, _] = heights(cx, 180.0);
    assert_eq!(wrapped, FOOT_WRAPPED);
}

/// Whether the balloon's element `what` is inside its body's viewport.
fn window_shows(handle: AnyWindowHandle, cx: &mut TestAppContext, n: usize, what: &str) -> bool {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let body = window.find(crate::balloon::part(n, "body")).bounds();
        let target = window.find(crate::balloon::part(n, what)).bounds();
        target.top() >= body.top() && target.bottom() <= body.bottom()
    })
    .unwrap()
}

/// Words with no space in them (a path, a hash) wrap by letters in the dialog's width: its
/// content never grows past the body, and the long option takes the lines it needs.
#[gpui_kit::test]
fn a_word_longer_than_the_dialog_wraps_inside_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let asked = json!({ "questions": [{
        "question": format!("Where is {}?", "a".repeat(400)),
        "header": "Path",
        "options": [
            { "label": "b".repeat(300), "description": "c".repeat(300) },
            { "label": "Short" },
        ],
    }] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let key = last_wait(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let at = |id: gpui_kit::ElementId| window.find(id).bounds();
        let (body, content) = (
            at(crate::balloon::part(n, "body")),
            at(("balloon-wait", key).into()),
        );
        assert!(
            content.left() >= body.left() && content.right() <= body.right(),
            "{content:?} in {body:?}"
        );
        let (long, short) = (
            at(crate::balloon::part(n, "q0-option-0")),
            at(crate::balloon::part(n, "q0-option-1")),
        );
        assert!(long.right() <= content.right(), "{long:?} in {content:?}");
        assert!(
            long.size.height > short.size.height * 4.0,
            "wrapped on many lines: {long:?} {short:?}"
        );
    })
    .unwrap();
}

/// A preview lies beside the options in a wide dialog and under them in a narrow one (a block
/// 400 wide gives a dialog of 320, too narrow for two columns).
#[gpui_kit::test]
fn a_preview_goes_under_the_options_of_a_narrow_dialog(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    roomy(&ws, cx, mine);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    let places = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let at = |what: &str| window.find(crate::balloon::part(n, what)).bounds();
            (at("q0-option-1"), at("q0-preview"))
        })
        .unwrap()
    };
    let (option, preview) = places(cx);
    assert!(
        preview.top() >= option.bottom(),
        "under: {preview:?} {option:?}"
    );
    assert!(preview.left() <= option.left() + px(20.0), "{preview:?}");
    ws.update(cx, |ws, _| {
        ws.space.canvas.get_mut(mine).unwrap().rect.w = 900.0
    });
    let (option, preview) = places(cx);
    assert!(
        preview.left() >= option.right(),
        "beside: {preview:?} {option:?}"
    );
}

/// The foot says how many of the questions have an answer, when there are several.
#[gpui_kit::test]
fn the_foot_counts_the_answers_of_several_questions(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    roomy(&ws, cx, mine);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", two_questions());
    assert!(drawn(handle, cx, n, "progress"));
    let _one = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "ask_user",
        json!({ "questions": [{ "question": "Ship it?", "options": ["Yes"] }] }),
    );
    let key = last_wait(&ws, cx);
    ws.update(cx, |ws, _| ws.waits.retain(|w| w.key == key));
    assert!(!drawn(handle, cx, n, "progress"), "one question needs none");
}

/// The body's scroll follows the zoom: the same words stay in view when the canvas zooms.
#[gpui_kit::test]
fn the_dialogs_scroll_follows_the_zoom(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let words = "A question that goes on about the database. ".repeat(120);
    let asked = json!({ "questions": [{ "question": words, "options": ["Yes", "No"] }] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let at = |cx: &mut TestAppContext, zoom: f32| {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera = Camera::at(Point::new(40.0, 400.0), zoom)
        });
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    };
    at(cx, 1.0);
    cx.update_window(handle, |_, window, cx| {
        window.hover(("balloon", n), cx);
        let body = window.find(crate::balloon::part(n, "body")).bounds();
        let to = (f32::from(body.left()) + 4.0, f32::from(body.center().y));
        wheel(
            window,
            cx,
            to,
            ScrollDelta::Pixels(point(px(0.0), px(-60.0))),
            false,
        );
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(body_scroll(&ws, cx), -60.0);
    at(cx, 2.0);
    assert_eq!(body_scroll(&ws, cx), -120.0);
}

/// The wheel over the preview of the newest question's first question, the pointer on the
/// balloon first (its block takes the wheel then), and how far the dialog's body scrolled.
fn wheel_over_preview(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    n: usize,
) -> f32 {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.hover(("balloon", n), cx);
        let preview = window.find(crate::balloon::part(n, "q0-preview")).bounds();
        let at = (f32::from(preview.center().x), f32::from(preview.center().y));
        wheel(
            window,
            cx,
            at,
            ScrollDelta::Pixels(point(px(0.0), px(-40.0))),
            false,
        );
        window.render_frame(cx);
    })
    .unwrap();
    body_scroll(ws, cx)
}

/// The wheel over a preview of a few lines scrolls the dialog's body (the preview has nothing to
/// scroll up and down); over a tall one it scrolls the preview and leaves the body alone.
#[gpui_kit::test]
fn the_wheel_scrolls_the_dialog_over_a_short_preview_and_the_preview_over_a_tall_one(
    cx: &mut TestAppContext,
) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    roomy(&ws, cx, mine);
    // Wide enough to lay the preview beside the options.
    ws.update(cx, |ws, _| {
        ws.space.canvas.get_mut(mine).unwrap().rect.w = 900.0
    });
    let with = |preview: String| {
        let options: Vec<Value> = (0..12)
            .map(|i| json!({ "label": format!("Option {i}"), "preview": preview }))
            .collect();
        json!({ "questions": [{ "question": "Which?", "options": options }] })
    };
    let _short = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "ask_user",
        with("two\nlines".into()),
    );
    assert!(
        wheel_over_preview(handle, &ws, cx, n) < 0.0,
        "the body scrolled"
    );
    let tall: Vec<String> = (0..300).map(|i| format!("line {i}")).collect();
    let _tall = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "ask_user",
        with(tall.join("\n")),
    );
    ws.update(cx, |ws, _| {
        ws.waits.remove(0);
    });
    assert_eq!(
        wheel_over_preview(handle, &ws, cx, n),
        0.0,
        "the preview did"
    );
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
    // Enter picks No and puts the highlight on Send; the next Enter sends.
    press(handle, cx, &["down", "down", "enter"]);
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "picked only");
    assert_eq!(highlight(&ws, cx), Some(templum_mcp::ask::Row::Send));
    press(handle, cx, &["enter"]);
    assert_eq!(answered(&answers), (false, "No".to_string()));
}

/// Space picks without sending; Enter on the option already picked sends.
#[gpui_kit::test]
fn space_picks_and_enter_on_the_picked_option_sends(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "No"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    focus_dialog(handle, &ws, cx, mine);
    press(handle, cx, &["down", "space"]);
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "picked only");
    press(handle, cx, &["enter"]);
    assert_eq!(answered(&answers), (false, "Yes".to_string()));
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
    press(handle, cx, &["down", "enter", "enter"]);
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
    press(handle, cx, &["enter", "enter"]);
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

/// Space types in the dialog's fields: a free answer and the note keep their spaces.
#[gpui_kit::test]
fn space_types_in_a_field(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let asked = json!({ "questions": [{ "question": "Why?" }] });
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let key = last_wait(&ws, cx);
    focus_dialog(handle, &ws, cx, mine);
    press(
        handle,
        cx,
        &["tab", "a", "space", "b", "tab", "c", "space", "d"],
    );
    let texts = ws.read_with(cx, |ws, cx| {
        let text = |field| field_of(ws, key, field).read(cx).value().to_string();
        (text(Some(0)), text(None))
    });
    assert_eq!(texts, ("a b".to_string(), "c d".to_string()));
}

/// Enter in a field acts once: with a question still unanswered it moves there, and sends nothing.
#[gpui_kit::test]
fn enter_in_a_field_moves_on_once(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", singles(2));
    let key = last_wait(&ws, cx);
    focus_dialog(handle, &ws, cx, mine);
    type_into(handle, &ws, cx, key, Some(0), "X");
    press(handle, cx, &["enter"]);
    assert_eq!(
        answers.try_recv(),
        Err(TryRecvError::Empty),
        "Q1 unanswered"
    );
    assert_eq!(
        highlight(&ws, cx),
        Some(templum_mcp::ask::Row::Option(1, 0))
    );
}

/// Enter in the note, no question answered: the highlight goes to the question; nothing is
/// picked nor sent.
#[gpui_kit::test]
fn enter_in_the_note_moves_on_once(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", singles(1));
    let key = last_wait(&ws, cx);
    focus_dialog(handle, &ws, cx, mine);
    type_into(handle, &ws, cx, key, None, "n");
    press(handle, cx, &["enter"]);
    assert_eq!(
        answers.try_recv(),
        Err(TryRecvError::Empty),
        "Q0 unanswered"
    );
    assert_eq!(
        highlight(&ws, cx),
        Some(templum_mcp::ask::Row::Option(0, 0))
    );
}

/// A press in the dialog's field of a block that hadn't the keyboard: its block takes it, so the
/// keys stay the dialog's once the highlight leaves the field.
#[gpui_kit::test]
fn a_field_that_takes_the_keyboard_brings_its_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    super::terminals::set_active(handle, true, cx);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", singles(1));
    let key = last_wait(&ws, cx);
    focus_dialog(handle, &ws, cx, kanban);
    type_into(handle, &ws, cx, key, Some(0), "X");
    frame(handle, cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, Some(mine)));
    // Out of the field, onto B, picked; then sent.
    press(handle, cx, &["shift-tab", "space", "enter"]);
    assert_eq!(
        json_of(&answered(&answers).1),
        json!({ "answers": { "Q0": "B" } })
    );
}

/// A click on the dialog of a block that hadn't the keyboard gives it to that block, as a press
/// on the block does: the keys are the dialog's, not the block focused before.
#[gpui_kit::test]
fn a_click_on_the_dialog_brings_its_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    roomy(&ws, cx, mine);
    ws.update(cx, |ws, _| ws.config.canvas.focus_follows_pointer = false);
    let answers = call_later(handle, &ws, cx, &terminal, 1, "ask_user", singles(2));
    focus_dialog(handle, &ws, cx, kanban);
    click_part(handle, cx, mine.0 as usize, "q0-option-1");
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, Some(mine)));
    press(handle, cx, &["enter"]);
    assert_eq!(
        highlight(&ws, cx),
        Some(templum_mcp::ask::Row::Option(1, 0))
    );
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty), "it waits");
}

/// A click in the dialog's field of a block that hadn't the keyboard leaves it in the field.
#[gpui_kit::test]
fn a_click_in_the_dialogs_field_keeps_the_keyboard_there(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    roomy(&ws, cx, mine);
    super::terminals::set_active(handle, true, cx);
    ws.update(cx, |ws, _| ws.config.canvas.focus_follows_pointer = false);
    let _asked = call_later(handle, &ws, cx, &terminal, 1, "ask_user", singles(1));
    let key = last_wait(&ws, cx);
    focus_dialog(handle, &ws, cx, kanban);
    click_part(handle, cx, mine.0 as usize, "note");
    cx.update_window(handle, |_, window, cx| {
        let note = field_of(ws.read(cx), key, None);
        assert!(note.read(cx).focus_handle(cx).is_focused(window));
    })
    .unwrap();
}

/// A plan is drawn rendered in its balloon; Reject with a comment, or the keys onto Approve,
/// answer it (docs/specs/agent-tools.md, propose_plan).
#[gpui_kit::test]
fn a_plan_is_approved_or_rejected_with_a_comment_from_its_dialog(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let answers = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        3,
        "propose_plan",
        json!({ "plan": "ok" }),
    );
    focus_dialog(handle, &ws, cx, mine);
    // Comment, Reject, Approve.
    press(handle, cx, &["down", "down", "down", "enter"]);
    assert_eq!(answered(&answers), (false, "Approved.".into()));
    let plan = json!({ "plan": "## Steps\n1. Replace the reader threads\n2. Test", "title": "Move the bus" });
    let answers = call_later(handle, &ws, cx, &terminal, 4, "propose_plan", plan);
    assert!(drawn(handle, cx, n, "plan-body"), "the plan is drawn");
    assert!(drawn(handle, cx, n, "plan-title"));
    assert!(!drawn(handle, cx, n, "dismiss"), "Reject, not Dismiss");
    let key = last_wait(&ws, cx);
    type_into(handle, &ws, cx, key, None, "do 2 first");
    click_part(handle, cx, n, "plan-reject");
    assert_eq!(
        answered(&answers),
        (true, "Rejected.\nThe person's comment: do 2 first".into())
    );
}

#[gpui_kit::test]
fn a_cancelled_plan_is_typed_back_once_answered(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let n = mine.0 as usize;
    let _first = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        3,
        "propose_plan",
        json!({ "plan": "## Steps" }),
    );
    bus_note(
        handle,
        &ws,
        cx,
        3,
        "notifications/cancelled",
        json!({ "requestId": 3 }),
    );
    assert!(stopped(&ws, cx));
    click_part(handle, cx, n, "plan-approve");
    assert_eq!(
        notices(&ws, cx),
        [(
            terminal.clone(),
            "[answer to your plan \"A plan\"] The answer is in the pasted text.\nApproved."
                .to_string()
        )]
    );
}

/// A plan, a question and a request to connect on one block wait in turn, "1 of 3", each
/// answered by its own buttons (Review Focus 5).
#[gpui_kit::test]
fn a_plan_a_question_and_a_request_wait_in_turn(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    roomy(&ws, cx, mine);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let n = mine.0 as usize;
    let part = crate::balloon::part;
    let plan = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "propose_plan",
        json!({ "plan": "## Steps" }),
    );
    let asked = json!({ "question": "Ship it?", "options": ["Yes", "Not yet"] });
    let question = call_later(handle, &ws, cx, &terminal, 2, "ask_user", asked);
    let request = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        3,
        "connect",
        json!({ "block": k }),
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.balloon(&terminal).map(|(_, count)| count)),
        Some(3)
    );
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(part(n, "count")).is_some(), "1 of 3");
        window.click(part(n, "plan-approve"), cx);
    })
    .unwrap();
    assert_eq!(answered(&plan), (false, "Approved.".to_string()));
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(part(n, "plan-approve")).is_none());
        window.click(part(n, "q0-option-1"), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        question.try_recv(),
        Err(TryRecvError::Empty),
        "Send answers"
    );
    cx.update_window(handle, |_, window, cx| window.click(part(n, "send"), cx))
        .unwrap();
    assert_eq!(answered(&question), (false, "Not yet".to_string()));
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(part(n, "allow"), cx);
    })
    .unwrap();
    assert_eq!(answered(&request), (false, format!("Connected to {k}.")));
}

/// A long plan scrolls in its frame, which takes at most 60% of the body's height.
#[gpui_kit::test]
fn a_long_plan_scrolls_in_its_frame(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let long: String = (1..=150).map(|i| format!("{i}. Step {i}\n")).collect();
    let _answers = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        3,
        "propose_plan",
        json!({ "plan": long }),
    );
    shown(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let frame = window.find(crate::balloon::part(n, "plan-body")).bounds();
        let balloon = window.find(("balloon", n)).bounds();
        assert!(
            frame.size.height <= balloon.size.height * 0.6 + px(1.0),
            "{frame:?} in {balloon:?}"
        );
        assert!(
            window
                .try_find(crate::balloon::part(n, "plan-approve"))
                .is_some(),
            "its buttons stay in the balloon"
        );
    })
    .unwrap();
}

/// The wheel over a long plan scrolls the plan and leaves the dialog's body alone, even where
/// the body overflows too (a block this short): the plan's frame is sized from the body, not from
/// the whole balloon.
#[gpui_kit::test]
fn the_wheel_over_a_long_plan_scrolls_the_plan_not_the_body(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let n = mine.0 as usize;
    let long: String = (1..=150).map(|i| format!("{i}. Step {i}\n")).collect();
    let _plan = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        3,
        "propose_plan",
        json!({ "plan": long }),
    );
    shown(handle, cx);
    let key = last_wait(&ws, cx);
    let plan_scroll = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| match &ws.waits.last().unwrap().what {
            Waiting::Question { scroll, .. } => f32::from(scroll.handle.offset().y),
            _ => panic!("a plan"),
        })
    };
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let body = window.find(crate::balloon::part(n, "body")).bounds();
        let content = window.find(("balloon-wait", key)).bounds();
        let frame = window.find(crate::balloon::part(n, "plan-body")).bounds();
        assert!(
            content.size.height > body.size.height,
            "the body overflows too"
        );
        assert!(
            frame.size.height <= body.size.height * 0.6 + px(1.0),
            "{frame:?} in {body:?}"
        );
        window.hover(("balloon", n), cx);
        let at = (f32::from(frame.center().x), f32::from(frame.center().y));
        wheel(
            window,
            cx,
            at,
            ScrollDelta::Pixels(point(px(0.0), px(-40.0))),
            false,
        );
        window.render_frame(cx);
    })
    .unwrap();
    assert!(plan_scroll(cx) < 0.0, "the plan scrolled");
    assert_eq!(body_scroll(&ws, cx), 0.0, "the body did not");
}

/// A plan off screen is a notification that says so: "Asks you to approve a plan: …".
#[gpui_kit::test]
fn a_plan_off_screen_says_it_asks_for_an_approval(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    super::terminals::set_active(handle, true, cx);
    ws.update(cx, |ws, _| {
        ws.space.canvas.get_mut(mine).unwrap().rect.x += 100_000.0;
        ws.space.focused = None;
    });
    let plan = json!({ "plan": "## Steps", "title": "Move the bus" });
    let _plan = call_later(handle, &ws, cx, &terminal, 1, "propose_plan", plan);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.space.terminal(mine).unwrap().alert.clone()),
        Some(crate::alert::Alert::Attention(Some(
            "Asks you to approve a plan: Move the bus".into()
        )))
    );
}
