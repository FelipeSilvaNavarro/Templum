//! The Templum API's glue (docs/specs/api.md): the calls its server hands the UI thread, the
//! checks of a token's scope and workspaces, the state, and the notices.

use std::sync::mpsc::Receiver;

use serde_json::{Value, json};
use templum_api::store::Scope;
use templum_api::{Store, Token, codes};
use templum_mcp::{Hello, Incoming, Message};

#[cfg(unix)]
use super::bus::{after, event, screen_with};
use super::bus::{answered, call_later, me};
#[cfg(unix)]
use super::delegate::{agents_running, entered};
use super::*;
use crate::workspace::AgentState;

/// A token of `scope` (and `workspaces`) the workspace's store keeps, as Settings would make one.
pub(super) fn with_token(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    scope: Scope,
    workspaces: Option<Vec<&str>>,
) -> Token {
    ws.update(cx, |ws, _| {
        let store = ws.api.store.get_or_insert_with(Store::default);
        let workspaces = workspaces.map(|w| w.into_iter().map(String::from).collect());
        store.mint("phone", scope, None, workspaces, 0).unwrap();
        store.tokens.last().unwrap().clone()
    })
}

/// The channel the answer to `method` comes on, as the API's server asks it: the message of
/// connection `conn`, from the token `token`.
pub(super) fn api_later(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    token: &Token,
    conn: u64,
    method: &str,
    params: Value,
) -> Receiver<String> {
    let (reply, answers) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: token.id.to_string(),
            client: "api".into(),
            control: None,
        },
        conn,
        incoming: Incoming::Request {
            id: json!(conn),
            method: method.into(),
            params,
        },
        reply,
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.serve(message, window, cx))
    })
    .unwrap();
    answers
}

/// The answer line to `method`, asked at once.
pub(super) fn api_call(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    token: &Token,
    method: &str,
    params: Value,
) -> Value {
    let answers = api_later(handle, ws, cx, token, 900, method, params);
    serde_json::from_str(&answers.try_recv().expect("answered at once")).unwrap()
}

/// A tool's answer line.
pub(super) fn tool(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    token: &Token,
    name: &str,
    arguments: Value,
) -> Value {
    api_call(
        handle,
        ws,
        cx,
        token,
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )
}

/// The text of a tool's answer, and whether it refused.
fn said(line: &Value) -> (bool, String) {
    let result = &line["result"];
    (
        result["isError"] == true,
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

/// The state, as the token sees it.
fn state(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    token: &Token,
) -> templum_api::State {
    let line = api_call(handle, ws, cx, token, templum_api::STATE, Value::Null);
    serde_json::from_value(line["result"].clone()).unwrap_or_else(|_| panic!("{line}"))
}

#[gpui_kit::test]
fn the_state_shows_workspaces_blocks_agents_and_cards(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, terminal) = three_blocks(handle, &ws, cx);
    let (_, board, card) = board_with_a_card(handle, &ws, cx);
    let token = with_token(&ws, cx, Scope::Read, None);
    let seen = state(handle, &ws, cx, &token);
    assert_eq!(seen.app.api, 1);
    assert_eq!(seen.app.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(seen.workspaces.len(), 1);
    assert!(seen.workspaces[0].shown && !seen.workspaces[0].stopped);
    assert_eq!(
        seen.app.shown.as_deref(),
        Some(seen.workspaces[0].id.as_str())
    );
    let (canvas, uids) = ws.read_with(cx, |ws, _| {
        (ws.space.canvas.blocks().to_vec(), ws.space.uids.clone())
    });
    // Back to front, with the canvas's rects.
    assert_eq!(seen.blocks.len(), canvas.len());
    for (z, (view, block)) in seen.blocks.iter().zip(&canvas).enumerate() {
        assert_eq!((view.id.as_str(), view.z), (uids[&block.id].as_str(), z));
        assert_eq!(
            (view.rect.x, view.rect.y, view.rect.w, view.rect.h),
            (block.rect.x, block.rect.y, block.rect.w, block.rect.h)
        );
    }
    let kind = |id: BlockId| seen.blocks.iter().find(|b| b.id == uids[&id]).unwrap();
    assert_eq!(
        (kind(terminal).kind.as_str(), kind(kanban).kind.as_str()),
        ("terminal", "kanban")
    );
    assert_eq!(
        seen.boards.iter().find(|b| b.id == board).unwrap().columns[0].cards,
        [card]
    );
    let view = seen.cards.iter().find(|c| c.id == card).unwrap();
    assert_eq!(view.board, board);
    assert_eq!(
        view.column,
        seen.boards.iter().find(|b| b.id == board).unwrap().columns[0].id
    );
}

#[gpui_kit::test]
fn an_agents_state_is_the_agents_views(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (a, b) = super::agents_view::one_done(handle, &ws, cx);
    let token = with_token(&ws, cx, Scope::Read, None);
    let (uid_a, uid_b) = ws.read_with(cx, |ws, _| {
        (ws.space.uids[&a.0].clone(), ws.space.uids[&b.0].clone())
    });
    let agent_of = |seen: &templum_api::State, uid: &str| {
        seen.blocks
            .iter()
            .find(|x| x.id == uid)
            .unwrap()
            .agent
            .clone()
            .unwrap()
    };
    let watch = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.update(cx, |ws, cx| {
            ws.watch_agents(false, cx.background_executor().now())
        })
    };
    // Ended while not looked at: done. Looked at: idle.
    let seen = state(handle, &ws, cx, &token);
    assert_eq!(agent_of(&seen, &uid_b).state, "done");
    assert!(agent_of(&seen, &uid_b).since.is_some());
    assert_eq!(agent_of(&seen, &uid_a).state, "idle");
    assert_eq!(agent_of(&seen, &uid_a).agent, "claude");
    // At work, then asking: needs you, with what it asks.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(a.0).unwrap().state = Some(AgentState::Working)
    });
    watch(&ws, cx);
    assert_eq!(
        agent_of(&state(handle, &ws, cx, &token), &uid_a).state,
        "working"
    );
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(a.0).unwrap().state =
            Some(AgentState::NeedsYou(Some("Allow Bash?".into())))
    });
    watch(&ws, cx);
    let asking = agent_of(&state(handle, &ws, cx, &token), &uid_a);
    assert_eq!(
        (asking.state.as_str(), asking.asks.as_deref()),
        ("needs_you", Some("Allow Bash?"))
    );
}

#[gpui_kit::test]
fn a_question_waits_in_the_state(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, uid) = me(&ws, cx, first);
    let token = with_token(&ws, cx, Scope::Read, None);
    let asked = json!({ "questions": [{ "question": "Which database?", "header": "DB",
        "options": [{ "label": "Postgres" }, { "label": "SQLite" }] }] });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    assert!(waiting.try_recv().is_err());
    let seen = state(handle, &ws, cx, &token);
    assert_eq!(seen.requests.len(), 1);
    let request = &seen.requests[0];
    assert!(request.id.starts_with('r'));
    assert_eq!(
        (
            request.kind.as_str(),
            request.block.as_deref(),
            request.scope
        ),
        ("question", Some(uid.as_str()), Scope::Control)
    );
    assert_eq!(request.questions[0].question, "Which database?");
    assert_eq!(
        request.questions[0]
            .options
            .iter()
            .map(|o| o.label.as_str())
            .collect::<Vec<_>>(),
        ["Postgres", "SQLite"]
    );
    let plan = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "propose_plan",
        json!({ "plan": "Do the thing." }),
    );
    assert!(plan.try_recv().is_err());
    let seen = state(handle, &ws, cx, &token);
    let plan = seen.requests.iter().find(|r| r.kind == "plan").unwrap();
    assert_eq!(plan.plan.as_ref().unwrap().text, "Do the thing.");
}

#[gpui_kit::test]
fn a_read_token_cant_act_and_a_control_token_cant_type(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, terminal) = three_blocks(handle, &ws, cx);
    let (_, board, _) = board_with_a_card(handle, &ws, cx);
    let read = with_token(&ws, cx, Scope::Read, None);
    let control = with_token(&ws, cx, Scope::Control, None);
    let cards = ws.read_with(cx, |ws, _| {
        ws.boards.board(board).unwrap().columns[0].cards.len()
    });
    let line = tool(
        handle,
        &ws,
        cx,
        &read,
        "create_card",
        json!({ "board": board.to_string(), "title": "x" }),
    );
    assert_eq!(
        (
            line["error"]["code"].as_i64(),
            line["error"]["message"].as_str()
        ),
        (
            Some(i64::from(codes::SCOPE)),
            Some("create_card needs a Control token.")
        )
    );
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&terminal].clone());
    let line = tool(
        handle,
        &ws,
        cx,
        &control,
        "send_to_terminal",
        json!({ "block": uid, "text": "ls" }),
    );
    assert_eq!(
        (
            line["error"]["code"].as_i64(),
            line["error"]["message"].as_str()
        ),
        (
            Some(i64::from(codes::SCOPE)),
            Some("send_to_terminal needs a Full token.")
        )
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.boards.board(board).unwrap().columns[0]
            .cards
            .len()),
        cards
    );
    let _ = kanban;
    // A read token reads.
    let line = tool(handle, &ws, cx, &read, "overview", json!({}));
    assert!(!said(&line).0 && said(&line).1.contains(&uid));
}

#[gpui_kit::test]
fn a_limited_token_sees_and_touches_only_its_workspaces(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (first_space, first_uid) = ws.read_with(cx, |ws, _| {
        (ws.space.id.clone(), ws.space.uids[&first].clone())
    });
    let (_, text) = super::templum_agent::control(
        handle,
        &ws,
        cx,
        "create_workspace",
        json!({ "name": "Other" }),
    );
    assert!(text.contains("Created workspace"), "{text}");
    let (_, text) = super::templum_agent::control(
        handle,
        &ws,
        cx,
        "add_block",
        json!({ "workspace": "w2", "kind": "terminal" }),
    );
    let far = text
        .trim_end_matches('.')
        .rsplit(' ')
        .next()
        .unwrap()
        .to_string();
    let token = with_token(&ws, cx, Scope::Control, Some(vec![first_space.as_str()]));
    let seen = state(handle, &ws, cx, &token);
    assert_eq!(
        seen.workspaces
            .iter()
            .map(|w| w.id.as_str())
            .collect::<Vec<_>>(),
        [first_space.as_str()]
    );
    assert!(
        seen.blocks.iter().all(|b| b.workspace == first_space)
            && seen.blocks.iter().any(|b| b.id == first_uid)
    );
    assert!(seen.blocks.iter().all(|b| b.id != far));
    // What is outside is answered as what doesn't exist.
    let refused = |line: &Value| {
        (
            line["error"]["code"].as_i64(),
            line["error"]["message"].as_str().map(String::from),
        )
    };
    let outside = tool(
        handle,
        &ws,
        cx,
        &token,
        "read_terminal",
        json!({ "block": far }),
    );
    let nothing = tool(
        handle,
        &ws,
        cx,
        &token,
        "read_terminal",
        json!({ "block": "zzzzz" }),
    );
    assert_eq!(refused(&outside).0, Some(i64::from(codes::NOT_FOUND)));
    assert_eq!(
        refused(&outside).1,
        Some(format!(
            "There's no block {far}: GET /v1/blocks lists them."
        ))
    );
    assert_eq!(
        refused(&nothing).1,
        Some("There's no block zzzzz: GET /v1/blocks lists them.".to_string())
    );
    let made = tool(
        handle,
        &ws,
        cx,
        &token,
        "create_workspace",
        json!({ "name": "Mine" }),
    );
    assert_eq!(refused(&made).0, Some(i64::from(codes::SCOPE)));
    let workspace = tool(
        handle,
        &ws,
        cx,
        &token,
        "show",
        json!({ "workspace": "w2" }),
    );
    assert_eq!(refused(&workspace).0, Some(i64::from(codes::NOT_FOUND)));
    // Moved into its workspace, the block is its own.
    let (_, text) =
        super::templum_agent::control(handle, &ws, cx, "show", json!({ "workspace": "w2" }));
    assert!(text.contains("Showing"), "{text}");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let id = ws.space.block_by_uid(&far).unwrap();
            ws.move_blocks(vec![id], Some(&first_space), window, cx);
        })
    })
    .unwrap();
    let inside = tool(
        handle,
        &ws,
        cx,
        &token,
        "read_terminal",
        json!({ "block": far }),
    );
    assert!(inside["error"].is_null(), "{inside}");
}

#[gpui_kit::test]
fn an_api_call_skips_the_gate_and_acts_as_the_person(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, terminal) = three_blocks(handle, &ws, cx);
    let (_, board, card) = board_with_a_card(handle, &ws, cx);
    let full = with_token(&ws, cx, Scope::Full, None);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&terminal].clone());
    let line = tool(
        handle,
        &ws,
        cx,
        &full,
        "send_to_terminal",
        json!({ "block": uid, "text": "ls" }),
    );
    assert!(!said(&line).0, "{line}");
    ws.read_with(cx, |ws, _| {
        assert!(ws.templum_agent.approvals.is_empty(), "no approval waits")
    });
    let line = tool(
        handle,
        &ws,
        cx,
        &full,
        "comment_card",
        json!({ "card": card, "text": "hi" }),
    );
    assert!(!said(&line).0, "{line}");
    let by = ws.read_with(cx, |ws, _| {
        ws.boards.board(board).unwrap().columns[0].cards[0]
            .comments
            .last()
            .unwrap()
            .by
            .clone()
    });
    assert_eq!(by, "you");
    let rows = ws.read_with(cx, |ws, _| ws.templum_agent.conversation.rows.len());
    let line = tool(
        handle,
        &ws,
        cx,
        &full,
        "create_card",
        json!({ "board": board.to_string(), "title": "n" }),
    );
    assert!(!said(&line).0);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.templum_agent.conversation.rows.len()),
        rows,
        "the chat is untouched"
    );
    assert!(ws.read_with(cx, |ws, _| {
        ws.templum_agent.conversation.own.cards.is_empty()
    }));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_task_from_the_api_is_typed_from_it_and_its_end_is_a_notice(cx: &mut TestAppContext) {
    let (handle, ws, _, theirs) = agents_running(cx, None);
    let (worker, them) = me(&ws, cx, theirs);
    let token = with_token(&ws, cx, Scope::Control, None);
    // Answered once the agent takes the task: the call isn't waited for here.
    let _ = api_later(
        handle,
        &ws,
        cx,
        &token,
        7,
        "tools/call",
        json!({
            "name": "delegate", "arguments": { "block": them, "task": "Green." }
        }),
    );
    screen_with(&ws, cx, theirs, "[from API · phone, task t");
    entered(&ws, cx, theirs);
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "Green." }),
    );
    after(cx, 1000);
    let notices = ws.read_with(cx, |ws, _| ws.api.notices.clone());
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].text.contains("task t1") && notices[0].text.contains("Green."),
        "{}",
        notices[0].text
    );
    assert_eq!(notices[0].block.as_deref(), Some(them.as_str()));
    assert!(notices[0].workspace.is_some());
    ws.read_with(cx, |ws, _| {
        assert!(ws.notices.is_empty(), "nothing is queued to type")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_closed_api_call_drops_its_wait(cx: &mut TestAppContext) {
    let (handle, ws, _, theirs) = agents_running(cx, None);
    let (_, them) = me(&ws, cx, theirs);
    let token = with_token(&ws, cx, Scope::Control, None);
    let waiting = api_later(
        handle,
        &ws,
        cx,
        &token,
        5,
        "tools/call",
        json!({
            "name": "delegate", "arguments": { "block": them, "task": "Slow.", "wait": true }
        }),
    );
    assert!(waiting.try_recv().is_err(), "it waits");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.waits.len(), 1);
        assert!(ws.space.terminal(theirs).unwrap().handed.is_some());
    });
    let (reply, _) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: token.id.to_string(),
            client: "api".into(),
            control: None,
        },
        conn: 5,
        incoming: Incoming::Notification {
            method: templum_mcp::bus::CLOSED.into(),
            params: Value::Null,
        },
        reply,
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.serve(message, window, cx))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.waits.is_empty(), "the wait is gone");
        assert!(
            ws.space.terminal(theirs).unwrap().handed.is_none(),
            "and so is the task's master"
        );
    });
}

#[gpui_kit::test]
fn a_revoked_token_is_answered_auth(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let token = with_token(&ws, cx, Scope::Read, None);
    assert!(tool(handle, &ws, cx, &token, "overview", json!({}))["error"].is_null());
    ws.update(cx, |ws, _| ws.api.store.as_mut().unwrap().revoke_all());
    let line = tool(handle, &ws, cx, &token, "overview", json!({}));
    assert_eq!(line["error"]["code"].as_i64(), Some(i64::from(codes::AUTH)));
    assert_eq!(line["error"]["message"], "This token was revoked.");
    let line = api_call(handle, &ws, cx, &token, templum_api::STATE, Value::Null);
    assert_eq!(line["error"]["code"].as_i64(), Some(i64::from(codes::AUTH)));
}

/// The ids of the requests in the state.
fn requests(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    token: &Token,
) -> Vec<String> {
    state(handle, ws, cx, token)
        .requests
        .into_iter()
        .map(|r| r.id)
        .collect()
}

/// `answer_request`'s answer line.
fn answer_request(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    token: &Token,
    arguments: Value,
) -> Value {
    tool(handle, ws, cx, token, "answer_request", arguments)
}

fn question() -> Value {
    json!({ "questions": [{ "question": "Which database?", "header": "DB",
        "options": [{ "label": "Postgres" }, { "label": "SQLite" }] }] })
}

#[gpui_kit::test]
fn answering_a_question_from_the_api(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let token = with_token(&ws, cx, Scope::Control, None);
    let asker = call_later(handle, &ws, cx, &terminal, 1, "ask_user", question());
    let id = requests(handle, &ws, cx, &token).remove(0);
    let line = answer_request(
        handle,
        &ws,
        cx,
        &token,
        json!({ "request": id, "answers": [["Postgres"]] }),
    );
    assert_eq!(said(&line), (false, "Answered.".to_string()));
    let (failed, text) = answered(&asker);
    assert!(!failed);
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({ "answers": { "Which database?": "Postgres" } })
    );
    assert!(requests(handle, &ws, cx, &token).is_empty());
    let again = answer_request(
        handle,
        &ws,
        cx,
        &token,
        json!({ "request": id, "answers": [["Postgres"]] }),
    );
    assert_eq!(
        again["error"]["code"].as_i64(),
        Some(i64::from(codes::NOT_FOUND))
    );
    assert_eq!(
        again["error"]["message"],
        format!("There's no request {id}: GET /v1/requests lists them.")
    );
}

#[gpui_kit::test]
fn a_wrong_answer_is_refused_and_the_request_stays(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let token = with_token(&ws, cx, Scope::Control, None);
    let asker = call_later(handle, &ws, cx, &terminal, 1, "ask_user", question());
    let id = requests(handle, &ws, cx, &token).remove(0);
    let line = answer_request(
        handle,
        &ws,
        cx,
        &token,
        json!({ "request": id, "answers": [["Postgres", "SQLite"]] }),
    );
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(templum_mcp::rpc::PARAMS))
    );
    assert_eq!(line["error"]["message"], "Question 1 takes one answer.");
    let line = answer_request(
        handle,
        &ws,
        cx,
        &token,
        json!({ "request": id, "allow": true }),
    );
    assert_eq!(
        line["error"]["message"],
        "A question takes answers or dismiss."
    );
    assert_eq!(requests(handle, &ws, cx, &token), [id], "it still waits");
    assert!(asker.try_recv().is_err());
}

#[gpui_kit::test]
fn a_plan_is_rejected_with_a_comment(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let token = with_token(&ws, cx, Scope::Control, None);
    let asker = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "propose_plan",
        json!({ "plan": "Everything." }),
    );
    let id = requests(handle, &ws, cx, &token).remove(0);
    let line = answer_request(
        handle,
        &ws,
        cx,
        &token,
        json!({ "request": id, "answers": [["Reject"]], "note": "too big" }),
    );
    assert_eq!(said(&line), (false, "Answered.".to_string()));
    assert_eq!(
        answered(&asker),
        (true, "Rejected.\nThe person's comment: too big".to_string())
    );
}

#[gpui_kit::test]
fn dismiss_and_allow(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, mine) = three_blocks(handle, &ws, cx);
    let (terminal, _) = me(&ws, cx, mine);
    let token = with_token(&ws, cx, Scope::Control, None);
    let asker = call_later(handle, &ws, cx, &terminal, 1, "ask_user", question());
    let id = requests(handle, &ws, cx, &token).remove(0);
    let line = answer_request(
        handle,
        &ws,
        cx,
        &token,
        json!({ "request": id, "dismiss": true }),
    );
    assert!(!said(&line).0, "{line}");
    assert_eq!(
        answered(&asker),
        (
            true,
            "The person dismissed the question without answering.".to_string()
        )
    );
    // A request to connect is answered with allow.
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    let connecting = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "connect",
        json!({ "block": k }),
    );
    let request = state(handle, &ws, cx, &token).requests.remove(0);
    assert_eq!(
        (request.kind.as_str(), request.buttons),
        ("connect", Some(["Allow".to_string(), "Deny".to_string()]))
    );
    let line = answer_request(
        handle,
        &ws,
        cx,
        &token,
        json!({ "request": request.id, "answers": [["x"]] }),
    );
    assert_eq!(line["error"]["message"], "This request takes allow.");
    let line = answer_request(
        handle,
        &ws,
        cx,
        &token,
        json!({ "request": request.id, "allow": true }),
    );
    assert!(!said(&line).0, "{line}");
    assert!(!answered(&connecting).0);
    ws.read_with(cx, |ws, _| assert!(ws.space.wired(mine, kanban)));
}

#[gpui_kit::test]
fn an_approval_needs_a_full_token(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, terminal) = three_blocks(handle, &ws, cx);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&terminal].clone());
    let control = with_token(&ws, cx, Scope::Control, None);
    let full = with_token(&ws, cx, Scope::Full, None);
    let waiting = super::templum_agent::control_later(
        handle,
        &ws,
        cx,
        7,
        "send_to_terminal",
        json!({ "block": uid, "text": "ls", "enter": false }),
    );
    assert!(waiting.try_recv().is_err(), "it waits for approval");
    let request = state(handle, &ws, cx, &full).requests.remove(0);
    assert!(request.id.starts_with('a'), "{}", request.id);
    assert_eq!(
        (
            request.kind.as_str(),
            request.tool.as_deref(),
            request.scope
        ),
        ("approval", Some("send_to_terminal"), Scope::Full)
    );
    assert!(request.said.is_some());
    // A limited token never sees it; a Control token sees it and can't answer it.
    let limited = with_token(&ws, cx, Scope::Full, Some(vec!["w1"]));
    assert!(state(handle, &ws, cx, &limited).requests.is_empty());
    let line = answer_request(
        handle,
        &ws,
        cx,
        &control,
        json!({ "request": request.id, "allow": true }),
    );
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(codes::SCOPE))
    );
    assert!(waiting.try_recv().is_err());
    let line = answer_request(
        handle,
        &ws,
        cx,
        &full,
        json!({ "request": request.id, "allow": true }),
    );
    assert_eq!(said(&line), (false, "Answered.".to_string()), "{line}");
    cx.run_until_parked();
    let (failed, _) = super::bus::result_of(&waiting.try_recv().expect("the call ran"));
    assert!(!failed);
    assert!(state(handle, &ws, cx, &full).requests.is_empty());
}

#[gpui_kit::test]
fn automations_from_the_api(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let control = with_token(&ws, cx, Scope::Control, None);
    let full = with_token(&ws, cx, Scope::Full, None);
    let notify = json!({ "name": "Ping", "workspace": space,
        "when": { "every": { "minutes": 10 } }, "do": { "notify": { "text": "hello" } } });
    let line = tool(
        handle,
        &ws,
        cx,
        &control,
        "save_automation",
        json!({ "automation": notify }),
    );
    assert_eq!(
        said(&line),
        (false, "Saved automation 1.".to_string()),
        "{line}"
    );
    let listed = state(handle, &ws, cx, &control).automations;
    assert_eq!(
        (listed[0].id, listed[0].name.as_str(), listed[0].on),
        (1, "Ping", true)
    );
    assert!(listed[0].sentence.contains("10"), "{}", listed[0].sentence);
    assert_eq!(listed[0].act, json!({ "notify": { "text": "hello" } }));
    assert!(listed[0].next.is_some());
    // Replaced, not added.
    let again = json!({ "name": "Pong", "workspace": space,
        "when": { "every": { "minutes": 5 } }, "do": { "notify": { "text": "hi" } } });
    let line = tool(
        handle,
        &ws,
        cx,
        &control,
        "save_automation",
        json!({ "automation": again, "id": 1 }),
    );
    assert_eq!(said(&line).1, "Saved automation 1.");
    assert_eq!(state(handle, &ws, cx, &control).automations.len(), 1);
    // The editor's checks and words.
    let blank = json!({ "name": "x", "workspace": space, "when": { "every": { "minutes": 5 } }, "do": { "notify": { "text": " " } } });
    let line = tool(
        handle,
        &ws,
        cx,
        &control,
        "save_automation",
        json!({ "automation": blank }),
    );
    assert_eq!(line["error"]["message"], "Write the text.");
    let line = tool(
        handle,
        &ws,
        cx,
        &control,
        "set_automation",
        json!({ "automation": 1, "on": false }),
    );
    assert_eq!(said(&line).1, "Turned automation 1 off.");
    assert!(!state(handle, &ws, cx, &control).automations[0].on);
    let line = tool(
        handle,
        &ws,
        cx,
        &control,
        "run_automation",
        json!({ "automation": 1 }),
    );
    assert_eq!(said(&line).1, "Ran automation 1.", "{line}");
    assert!(
        state(handle, &ws, cx, &control).automations[0]
            .last
            .is_some()
    );
    // A command is a Full token's; deleting is too.
    let command = json!({ "name": "Run", "workspace": space,
        "when": { "every": { "minutes": 5 } }, "do": { "command": { "text": "ls" } } });
    let line = tool(
        handle,
        &ws,
        cx,
        &control,
        "save_automation",
        json!({ "automation": command }),
    );
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(codes::SCOPE))
    );
    let line = tool(
        handle,
        &ws,
        cx,
        &control,
        "delete_automation",
        json!({ "automation": 1 }),
    );
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(codes::SCOPE))
    );
    let line = tool(
        handle,
        &ws,
        cx,
        &full,
        "delete_automation",
        json!({ "automation": 1 }),
    );
    assert_eq!(said(&line).1, "Deleted automation 1.");
    assert!(state(handle, &ws, cx, &full).automations.is_empty());
    let line = tool(
        handle,
        &ws,
        cx,
        &full,
        "delete_automation",
        json!({ "automation": 1 }),
    );
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(codes::NOT_FOUND))
    );
}

/// An event rule saved through the API shows its cooldown in the state; a time rule has none.
#[gpui_kit::test]
fn an_event_rules_cooldown_is_in_the_state(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let control = with_token(&ws, cx, Scope::Control, None);
    let event = json!({ "name": "Done", "workspace": space, "cooldown": 3,
        "when": { "agent": { "event": "idle", "who": { "kind": "claude" } } },
        "do": { "notify": { "text": "a Claude finished" } } });
    let time = json!({ "name": "Ping", "workspace": space,
        "when": { "every": { "minutes": 10 } }, "do": { "notify": { "text": "hello" } } });
    for rule in [event, time] {
        let line = tool(
            handle,
            &ws,
            cx,
            &control,
            "save_automation",
            json!({ "automation": rule }),
        );
        assert!(!said(&line).0, "{line}");
    }
    let listed = state(handle, &ws, cx, &control).automations;
    assert_eq!(
        listed[0].when,
        json!({ "agent": { "event": "idle", "who": { "kind": "claude" } } })
    );
    assert_eq!((listed[0].cooldown, listed[1].cooldown), (Some(3), None));
}

#[gpui_kit::test]
fn a_limited_token_saves_automations_only_for_its_workspaces(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let all = with_token(&ws, cx, Scope::Control, None);
    let limited = with_token(&ws, cx, Scope::Control, Some(vec![space.as_str()]));
    let rule = |workspace: Option<&str>| {
        let mut rule = json!({ "name": "Ping", "when": { "every": { "minutes": 5 } }, "do": { "notify": { "text": "hi" } } });
        if let Some(w) = workspace {
            rule["workspace"] = json!(w);
        }
        json!({ "automation": rule })
    };
    // For all workspaces, or for one it doesn't see: no.
    let line = tool(handle, &ws, cx, &limited, "save_automation", rule(None));
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(codes::SCOPE))
    );
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "save_automation",
        rule(Some("w9")),
    );
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(codes::NOT_FOUND))
    );
    // Its own workspace: yes. And another's rule can't be replaced or run by it.
    assert!(
        !said(&tool(
            handle,
            &ws,
            cx,
            &limited,
            "save_automation",
            rule(Some(&space))
        ))
        .0
    );
    assert!(!said(&tool(handle, &ws, cx, &all, "save_automation", rule(None))).0);
    let theirs = state(handle, &ws, cx, &all)
        .automations
        .iter()
        .find(|a| a.workspace.is_none())
        .unwrap()
        .id;
    let limited_full = with_token(&ws, cx, Scope::Full, Some(vec![space.as_str()]));
    for (name, arguments) in [
        ("run_automation", json!({ "automation": theirs })),
        ("delete_automation", json!({ "automation": theirs })),
    ] {
        let line = tool(handle, &ws, cx, &limited_full, name, arguments);
        assert_eq!(
            line["error"]["code"].as_i64(),
            Some(i64::from(codes::NOT_FOUND)),
            "{name}"
        );
    }
    let mut replace = rule(Some(&space));
    replace["id"] = json!(theirs);
    let line = tool(handle, &ws, cx, &limited, "save_automation", replace);
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(codes::NOT_FOUND))
    );
    // And it can't type into a block it doesn't see.
    let mut typed = rule(Some(&space));
    typed["automation"]["do"] = json!({ "prompt": { "block": "zzzzz", "text": "x" } });
    let line = tool(handle, &ws, cx, &limited, "save_automation", typed);
    assert_eq!(
        line["error"]["code"].as_i64(),
        Some(i64::from(codes::NOT_FOUND))
    );
}

// Turning it on: the store, the server, the tick, Settings ▸ API and the pill.

use std::time::Duration;
use templum_api::store::{Listen, hash};

/// A workspace with a data folder (the API keeps its file there).
fn with_data(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(dir.path().to_path_buf()));
    (handle, ws, dir)
}

/// The API on, on an ephemeral port of this computer.
fn turn_on(ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    ws.update(cx, |ws, cx| {
        ws.api_edit(
            |s| {
                s.enabled = true;
                s.port = 0;
            },
            cx,
        )
    });
}

fn port(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> u16 {
    ws.read_with(cx, |ws, _| {
        ws.api.server.as_ref().expect("listening").addr().port()
    })
}

/// The status of `GET path` with `token` (none: no header), over a plain connection. Only what the
/// server alone answers (401, 404) is asked: the UI thread isn't pumped while this waits.
fn status(port: u16, path: &str, token: Option<&str>) -> u16 {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: x\r\n{auth}Connection: close\r\n\r\n"
    )
    .unwrap();
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    answer
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn pill(handle: AnyWindowHandle, cx: &mut TestAppContext) -> bool {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.try_find("api-pill").is_some()
    })
    .unwrap()
}

#[gpui_kit::test]
fn turning_the_api_on_listens_and_off_stops(cx: &mut TestAppContext) {
    let (handle, ws, dir) = with_data(cx);
    ws.read_with(cx, |ws, _| {
        assert!(ws.api.server.is_none() && ws.api.store.is_some())
    });
    assert!(!pill(handle, cx), "off, no pill");
    turn_on(&ws, cx);
    assert_ne!(port(&ws, cx), 0);
    let file =
        std::fs::read_to_string(templum_api::store::folder(dir.path()).join("api.json")).unwrap();
    assert!(file.contains("\"enabled\": true"), "{file}");
    assert!(pill(handle, cx), "the pill shows the door is open");
    ws.update(cx, |ws, cx| ws.api_edit(|s| s.enabled = false, cx));
    ws.read_with(cx, |ws, _| {
        assert!(ws.api.server.is_none() && ws.api.error.is_none())
    });
    assert!(!pill(handle, cx));
    // Settings read the same file at the next start.
    drop((handle, ws));
}

#[gpui_kit::test]
fn a_new_token_is_shown_once_and_saved_as_a_hash(cx: &mut TestAppContext) {
    let (_, ws, dir) = with_data(cx);
    turn_on(&ws, cx);
    ws.update(cx, |ws, cx| {
        ws.mint_token("phone", Scope::Control, None, None, cx)
    })
    .unwrap();
    let text = ws
        .read_with(cx, |ws, _| ws.api.shown.clone())
        .expect("shown once");
    assert!(text.starts_with("tpl_"));
    let file =
        std::fs::read_to_string(templum_api::store::folder(dir.path()).join("api.json")).unwrap();
    assert!(
        file.contains(&hash(&text)) && !file.contains(&text[4..]),
        "{file}"
    );
    ws.update(cx, |ws, _| ws.token_seen());
    ws.read_with(cx, |ws, _| assert!(ws.api.shown.is_none()));
    let port = port(&ws, cx);
    assert_eq!(
        status(port, "/v1/nope", Some(&text)),
        404,
        "the server knows the token"
    );
    assert_eq!(status(port, "/v1/nope", None), 401);
    let blank = ws.update(cx, |ws, cx| {
        ws.mint_token("  ", Scope::Read, None, None, cx)
    });
    assert_eq!(blank, Err("A token needs a name.".to_string()));
}

#[gpui_kit::test]
fn revoke_and_revoke_all(cx: &mut TestAppContext) {
    let (_, ws, _dir) = with_data(cx);
    turn_on(&ws, cx);
    let mut texts = Vec::new();
    for name in ["a", "b"] {
        ws.update(cx, |ws, cx| {
            ws.mint_token(name, Scope::Read, None, None, cx)
        })
        .unwrap();
        texts.push(ws.read_with(cx, |ws, _| ws.api.shown.clone()).unwrap());
    }
    let (first, port) = (
        ws.read_with(cx, |ws, _| ws.api.store.as_ref().unwrap().tokens[0].id),
        port(&ws, cx),
    );
    ws.update(cx, |ws, cx| ws.revoke_token(first, cx));
    assert_eq!(status(port, "/v1/nope", Some(&texts[0])), 401);
    assert_eq!(status(port, "/v1/nope", Some(&texts[1])), 404);
    ws.update(cx, |ws, cx| ws.revoke_all_tokens(cx));
    assert_eq!(status(port, "/v1/nope", Some(&texts[1])), 401);
    ws.read_with(cx, |ws, _| {
        assert!(ws.api.store.as_ref().unwrap().tokens.is_empty())
    });
}

#[gpui_kit::test]
fn plain_http_is_never_offered_off_loopback(cx: &mut TestAppContext) {
    let (_, ws, _dir) = with_data(cx);
    ws.update(cx, |ws, cx| {
        ws.api_edit(
            |s| {
                s.enabled = true;
                s.port = 0;
                s.listen = Listen::All;
            },
            cx,
        )
    });
    ws.read_with(cx, |ws, _| {
        let settings = &ws.api.store.as_ref().unwrap().settings;
        assert_eq!(
            settings.tls,
            templum_api::store::Tls::Own,
            "HTTPS is on, not offered off"
        );
        assert!(ws.api.error.is_none(), "{:?}", ws.api.error);
        assert!(ws.api.server.as_ref().unwrap().addr().ip().is_unspecified());
        assert!(
            ws.api.fingerprint.as_ref().is_some_and(|f| f.len() == 95),
            "its fingerprint is shown"
        );
    });
}

#[gpui_kit::test]
fn a_port_in_use_says_so(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = with_data(cx);
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy = held.local_addr().unwrap().port();
    ws.update(cx, |ws, cx| {
        ws.api_edit(
            |s| {
                s.enabled = true;
                s.port = busy;
            },
            cx,
        )
    });
    ws.read_with(cx, |ws, _| {
        assert!(ws.api.server.is_none());
        assert_eq!(
            ws.api.error.as_deref(),
            Some(format!("Port {busy} is in use: choose another port.").as_str())
        );
    });
    assert!(pill(handle, cx), "it reads API off");
}

#[gpui_kit::test]
fn last_uses_are_kept_at_the_snapshot(cx: &mut TestAppContext) {
    let (handle, ws, dir) = with_data(cx);
    turn_on(&ws, cx);
    ws.update(cx, |ws, cx| {
        ws.mint_token("phone", Scope::Read, None, None, cx)
    })
    .unwrap();
    let text = ws.read_with(cx, |ws, _| ws.api.shown.clone()).unwrap();
    assert_eq!(status(port(&ws, cx), "/v1/nope", Some(&text)), 404);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.snapshot(window, cx))
    })
    .unwrap();
    let last = ws
        .read_with(cx, |ws, _| {
            ws.api.store.as_ref().unwrap().tokens[0].last.clone()
        })
        .expect("a last use");
    assert_eq!(last.addr, "127.0.0.1");
    let file =
        std::fs::read_to_string(templum_api::store::folder(dir.path()).join("api.json")).unwrap();
    assert!(file.contains("\"addr\": \"127.0.0.1\""), "saved: {file}");
    // A second use within the minute is kept, not written (the file is synced to disk).
    assert_eq!(status(port(&ws, cx), "/v1/nope", Some(&text)), 404);
    let snapshot = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.snapshot(window, cx))
        })
        .unwrap()
    };
    snapshot(cx);
    ws.read_with(cx, |ws, _| assert!(ws.api.dirty, "waiting for its minute"));
    cx.executor().advance_clock(Duration::from_secs(61));
    snapshot(cx);
    ws.read_with(cx, |ws, _| assert!(!ws.api.dirty, "a minute on, written"));
    // And at quit, whatever is left.
    assert_eq!(status(port(&ws, cx), "/v1/nope", Some(&text)), 404);
    snapshot(cx);
    ws.read_with(cx, |ws, _| assert!(ws.api.dirty));
    ws.update(cx, |ws, _| ws.flush_api());
    ws.read_with(cx, |ws, _| assert!(!ws.api.dirty, "flushed"));
}

#[gpui_kit::test]
fn a_use_since_the_last_snapshot_is_written_at_quit(cx: &mut TestAppContext) {
    let (_, ws, dir) = with_data(cx);
    turn_on(&ws, cx);
    ws.update(cx, |ws, cx| {
        ws.mint_token("phone", Scope::Read, None, None, cx)
    })
    .unwrap();
    let text = ws.read_with(cx, |ws, _| ws.api.shown.clone()).unwrap();
    assert_eq!(status(port(&ws, cx), "/v1/nope", Some(&text)), 404);
    // No snapshot ran: the use is only in the server. Quitting takes it.
    ws.update(cx, |ws, _| ws.flush_api());
    let file =
        std::fs::read_to_string(templum_api::store::folder(dir.path()).join("api.json")).unwrap();
    assert!(file.contains("\"addr\": \"127.0.0.1\""), "saved: {file}");
}

#[gpui_kit::test]
fn the_pill_counts_the_clients_the_server_tells_of(cx: &mut TestAppContext) {
    use std::io::{Read, Write};
    let (_, ws, _dir) = with_data(cx);
    turn_on(&ws, cx);
    ws.update(cx, |ws, cx| {
        ws.mint_token("phone", Scope::Read, None, None, cx)
    })
    .unwrap();
    let text = ws.read_with(cx, |ws, _| ws.api.shown.clone()).unwrap();
    let clients = |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.api.clients);
    assert_eq!(clients(cx), 0);
    // A connection kept open after its answer: a client.
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port(&ws, cx))).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "GET /v1/nope HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {text}\r\n\r\n"
    )
    .unwrap();
    let mut head = [0u8; 12];
    stream.read_exact(&mut head).unwrap();
    wait_until(cx, "the pill to count it", |cx| clients(cx) == 1);
    drop(stream);
    wait_until(cx, "the pill to see it go", |cx| clients(cx) == 0);
}

/// Pumps the UI thread until `condition` holds (the server's messages come on its channel).
fn wait_until(
    cx: &mut TestAppContext,
    what: &str,
    condition: impl Fn(&mut TestAppContext) -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if condition(cx) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui_kit::test]
fn an_api_field_is_taken_on_enter_not_at_each_key(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = with_data(cx);
    turn_on(&ws, cx);
    let port = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.api.store.as_ref().unwrap().settings.port)
    };
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-api", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("field-api.port", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(port(cx), 0);
    cx.simulate_input(handle, "47311");
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert_eq!(port(cx), 0, "typed, not taken");
    cx.simulate_keystrokes(handle, "enter");
    assert_eq!(port(cx), 47311, "Enter takes it");
}

/// Settings ▸ API with the port field focused and `typed` in it.
fn type_a_port(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    typed: &str,
) {
    // Focus listeners (a field's blur) only hear an active window.
    super::terminals::set_active(handle, true, cx);
    turn_on(ws, cx);
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-api", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("field-api.port", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_input(handle, typed);
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
}

#[gpui_kit::test]
fn an_api_field_is_taken_when_the_keyboard_leaves_it(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = with_data(cx);
    type_a_port(handle, &ws, cx, "47312");
    let port = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.api.store.as_ref().unwrap().settings.port)
    };
    assert_eq!(port(cx), 0, "typed, not taken");
    cx.update_window(handle, |_, window, cx| {
        // The next field takes the keyboard from this one, as a mouse down on it does.
        window.click("menu-api-listen", cx);
        window.blur(cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(port(cx), 47312, "Blur takes it");
}

#[gpui_kit::test]
fn a_port_that_is_refused_is_put_back_and_said(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = with_data(cx);
    type_a_port(handle, &ws, cx, "80");
    let port = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.api.store.as_ref().unwrap().settings.port)
    };
    cx.simulate_keystrokes(handle, "enter");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(gpui_kit::ElementId::Name("commit-api-port-refused".into()))
                .is_some(),
            "says why"
        );
    })
    .unwrap();
    assert_eq!(port(cx), 0, "unchanged");
    // The field shows the setting again: what is typed next is added to it, not to the "80".
    cx.simulate_input(handle, "47313");
    cx.simulate_keystrokes(handle, "enter");
    assert_eq!(port(cx), 47313);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(gpui_kit::ElementId::Name("commit-api-port-refused".into()))
                .is_none(),
            "taken: nothing to say"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn an_edit_that_changes_nothing_restarts_nothing(cx: &mut TestAppContext) {
    let (_, ws, _dir) = with_data(cx);
    turn_on(&ws, cx);
    let before = port(&ws, cx);
    ws.update(cx, |ws, cx| ws.api_edit(|s| s.port = 0, cx));
    assert_eq!(port(&ws, cx), before, "the same server, on the same port");
}

#[gpui_kit::test]
fn the_tick_runs_only_while_a_stream_is_open(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = with_data(cx);
    turn_on(&ws, cx);
    let token = with_token(&ws, cx, Scope::Read, None);
    cx.run_until_parked();
    ws.read_with(cx, |ws, _| assert!(!ws.api.ticking, "no stream, no tick"));
    // A stream opened: the server says so, and the tick starts.
    let (reply, _) = std::sync::mpsc::channel();
    let message = Message {
        hello: Hello {
            terminal: token.id.to_string(),
            client: "api".into(),
            control: None,
        },
        conn: 3,
        incoming: Incoming::Notification {
            method: templum_api::WATCH.into(),
            params: Value::Null,
        },
        reply,
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.serve(message, window, cx))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.api.ticking));
    // At its next look there is no stream: it ends.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    ws.read_with(cx, |ws, _| {
        assert!(!ws.api.ticking, "it ended at the first look with none")
    });
}

#[gpui_kit::test]
fn settings_show_the_api_its_fields_and_its_tokens(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = with_data(cx);
    turn_on(&ws, cx);
    ws.update(cx, |ws, cx| {
        ws.mint_token("phone", Scope::Read, None, None, cx)
    })
    .unwrap();
    let id = ws.read_with(cx, |ws, _| ws.api.store.as_ref().unwrap().tokens[0].id);
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-api", cx);
        window.render_frame(cx);
        for found in [
            "field-api.enabled",
            "field-api.listen",
            "field-api.port",
            "field-api.tls",
            "api-new-token",
            "api-revoke-all",
            "api-listening",
        ] {
            assert!(window.try_find(found).is_some(), "{found}");
        }
        // Its row is below the fold of the test window: found, not clicked.
        assert!(
            window
                .try_find(gpui_kit::ElementId::Name(format!("api-revoke-{id}").into()))
                .is_some()
        );
    })
    .unwrap();
}

// The review's findings: what a limited token or a big number could reach.

fn refused(line: &Value) -> (Option<i64>, String) {
    (
        line["error"]["code"].as_i64(),
        line["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

/// The first workspace, with a second one and a block in it: (the second's id, the block's uid).
fn another_workspace(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> (String, String) {
    super::templum_agent::control(
        handle,
        ws,
        cx,
        "create_workspace",
        json!({ "name": "Other" }),
    );
    let (_, text) = super::templum_agent::control(
        handle,
        ws,
        cx,
        "add_block",
        json!({ "workspace": "w2", "kind": "terminal" }),
    );
    (
        "w2".to_string(),
        text.trim_end_matches('.')
            .rsplit(' ')
            .next()
            .unwrap()
            .to_string(),
    )
}

#[gpui_kit::test]
fn a_limited_token_cant_overview_what_it_doesnt_see(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let (_, far) = another_workspace(handle, &ws, cx);
    let limited = with_token(&ws, cx, Scope::Read, Some(vec![first.as_str()]));
    let line = tool(handle, &ws, cx, &limited, "overview", json!({}));
    let (code, message) = refused(&line);
    assert_eq!(code, Some(i64::from(codes::SCOPE)), "{line}");
    assert!(
        message.contains("GET /v1/state") && !message.contains(&far),
        "{message}"
    );
    // An unlimited token still has it.
    let all = with_token(&ws, cx, Scope::Read, None);
    assert!(
        said(&tool(handle, &ws, cx, &all, "overview", json!({})))
            .1
            .contains(&far)
    );
}

#[gpui_kit::test]
fn a_limited_token_names_its_board_and_never_hears_of_others(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let (_, board, card) = board_with_a_card(handle, &ws, cx);
    let secret = ws.update(cx, |ws, _| {
        ws.edit_boards(|b| b.add_board_with(Some("Secret plans"), &["To do".to_string()]))
    });
    let limited = with_token(&ws, cx, Scope::Control, Some(vec![first.as_str()]));
    for (name, arguments) in [
        ("list_cards", json!({})),
        ("create_card", json!({ "title": "x" })),
    ] {
        let line = tool(handle, &ws, cx, &limited, name, arguments);
        let (code, message) = refused(&line);
        assert_eq!(
            code,
            Some(i64::from(templum_mcp::rpc::PARAMS)),
            "{name}: {line}"
        );
        assert!(
            message.contains("board") && !message.contains("Secret"),
            "{name}: {message}"
        );
    }
    // A board it doesn't see is a board that doesn't exist, by id and by name.
    for unseen in [secret.to_string(), "Secret plans".to_string()] {
        let line = tool(
            handle,
            &ws,
            cx,
            &limited,
            "create_card",
            json!({ "board": unseen, "title": "x" }),
        );
        let (code, message) = refused(&line);
        assert_eq!(code, Some(i64::from(codes::NOT_FOUND)));
        assert!(message.starts_with("There's no board"), "{message}");
    }
    assert!(ws.read_with(cx, |ws, _| {
        ws.boards.board(secret).unwrap().columns[0].cards.is_empty()
    }));
    // Its own works; so does a card.
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "create_card",
        json!({ "board": board.to_string(), "title": "x" }),
    );
    assert!(!said(&line).0 && line["error"].is_null(), "{line}");
    assert!(
        !said(&tool(
            handle,
            &ws,
            cx,
            &limited,
            "list_cards",
            json!({ "card": card })
        ))
        .0
    );
}

#[gpui_kit::test]
fn a_big_move_is_clamped_to_the_biggest_grid(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, terminal) = three_blocks(handle, &ws, cx);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&terminal].clone());
    let token = with_token(&ws, cx, Scope::Control, None);
    let line = tool(
        handle,
        &ws,
        cx,
        &token,
        "move_block",
        json!({ "block": uid, "w": 1e9, "h": 1e9 }),
    );
    assert!(!said(&line).0 && line["error"].is_null(), "{line}");
    let (rect, grid, max) = ws.read_with(cx, |ws, _| {
        let max = templum_canvas::chrome::block_size(
            crate::workspace::MAX_COLS,
            crate::workspace::MAX_ROWS,
            ws.cell.size,
        );
        (
            ws.space.canvas.get(terminal).unwrap().rect,
            ws.space.terminal(terminal).unwrap().term.grid_size(),
            max,
        )
    });
    assert_eq!((rect.w, rect.h), (max.w, max.h));
    assert!(
        grid.0 <= crate::workspace::MAX_COLS && grid.1 <= crate::workspace::MAX_ROWS,
        "{grid:?}"
    );
    // And a place far beyond any canvas is refused as a number, not stored.
    let line = tool(
        handle,
        &ws,
        cx,
        &token,
        "move_block",
        json!({ "block": uid, "x": 1e12 }),
    );
    assert_eq!(
        refused(&line).0,
        Some(i64::from(templum_mcp::rpc::PARAMS)),
        "{line}"
    );
}

#[gpui_kit::test]
fn a_command_automation_is_a_full_tokens_whoever_touches_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let control = with_token(&ws, cx, Scope::Control, None);
    let full = with_token(&ws, cx, Scope::Full, None);
    let rule = |act: Value| json!({ "name": "R", "workspace": space, "when": { "every": { "minutes": 5 } }, "do": act });
    let command = json!({ "command": { "text": "ls" } });
    let notify = json!({ "notify": { "text": "hi" } });
    assert!(
        !said(&tool(
            handle,
            &ws,
            cx,
            &full,
            "save_automation",
            json!({ "automation": rule(command) })
        ))
        .0
    );
    assert!(
        !said(&tool(
            handle,
            &ws,
            cx,
            &control,
            "save_automation",
            json!({ "automation": rule(notify.clone()) })
        ))
        .0
    );
    // 1 is the command, 2 the notice.
    for (name, arguments) in [
        ("set_automation", json!({ "automation": 1, "on": false })),
        ("run_automation", json!({ "automation": 1 })),
        (
            "save_automation",
            json!({ "automation": rule(notify.clone()), "id": 1 }),
        ),
    ] {
        let line = tool(handle, &ws, cx, &control, name, arguments);
        let (code, message) = refused(&line);
        assert_eq!(code, Some(i64::from(codes::SCOPE)), "{name}: {line}");
        assert_eq!(message, format!("{name} needs a Full token."));
    }
    ws.read_with(cx, |ws, _| {
        assert!(ws.automations.get(1).unwrap().on, "untouched")
    });
    // The notice is a control token's.
    for (name, arguments) in [
        ("set_automation", json!({ "automation": 2, "on": false })),
        ("run_automation", json!({ "automation": 2 })),
        (
            "save_automation",
            json!({ "automation": rule(notify), "id": 2 }),
        ),
    ] {
        assert!(
            tool(handle, &ws, cx, &control, name, arguments)["error"].is_null(),
            "{name}"
        );
    }
    // A full token still does it all.
    assert!(
        !said(&tool(
            handle,
            &ws,
            cx,
            &full,
            "set_automation",
            json!({ "automation": 1, "on": false })
        ))
        .0
    );
}

#[gpui_kit::test]
fn a_name_that_is_another_workspaces_id_resolves_as_the_tool_does(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let (_, far) = another_workspace(handle, &ws, cx);
    // The first workspace is named "w2", which is the id of the other.
    ws.update(cx, |ws, _| ws.space.name = "w2".into());
    let limited = with_token(&ws, cx, Scope::Control, Some(vec![first.as_str()]));
    let before = ws.read_with(cx, |ws, _| {
        ws.space_named("w2").unwrap().canvas.blocks().len()
    });
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "add_block",
        json!({ "workspace": "w2", "kind": "note" }),
    );
    assert_eq!(
        refused(&line).0,
        Some(i64::from(codes::NOT_FOUND)),
        "{line}"
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws
            .space_named("w2")
            .unwrap()
            .canvas
            .blocks()
            .len()),
        before
    );
    let _ = far;
    // The same for a board whose name is another board's id.
    let (_, mine, _) = board_with_a_card(handle, &ws, cx);
    let theirs = ws.update(cx, |ws, _| {
        ws.edit_boards(|b| b.add_board_with(Some("Theirs"), &["To do".to_string()]))
    });
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            b.boards.iter_mut().find(|x| x.id == mine).unwrap().name = theirs.to_string()
        })
    });
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "create_card",
        json!({ "board": theirs.to_string(), "title": "x" }),
    );
    assert_eq!(
        refused(&line).0,
        Some(i64::from(codes::NOT_FOUND)),
        "{line}"
    );
    let in_theirs = ws.read_with(cx, |ws, _| {
        ws.boards.board(theirs).unwrap().columns[0].cards.len()
    });
    let in_mine = ws.read_with(cx, |ws, _| {
        ws.boards.board(mine).unwrap().columns[0].cards.len()
    });
    assert_eq!(
        (in_theirs, in_mine),
        (0, 1),
        "the id wins in the tool, so a board it doesn't see isn't touched"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_limited_tokens_folder_stays_inside_its_workspaces(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let (home, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::create_dir(home.path().join("inside")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), home.path().join("link")).unwrap();
    ws.update(cx, |ws, _| {
        ws.space.folder = Some(home.path().to_path_buf())
    });
    let limited = with_token(&ws, cx, Scope::Control, Some(vec![first.as_str()]));
    let blocks = |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len());
    let n = blocks(cx);
    for outside in [
        elsewhere.path().display().to_string(),
        "..".to_string(),
        format!(
            "{}/../{}",
            home.path().display(),
            elsewhere.path().file_name().unwrap().to_string_lossy()
        ),
        "link".to_string(),
        "/".to_string(),
    ] {
        for (kind, extra) in [("terminal", json!({})), ("editor", json!({}))] {
            let mut args = json!({ "workspace": first, "kind": kind, "folder": outside });
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let line = tool(handle, &ws, cx, &limited, "add_block", args);
            assert_eq!(
                refused(&line).0,
                Some(i64::from(codes::SCOPE)),
                "{kind} {outside}: {line}"
            );
        }
        let line = tool(
            handle,
            &ws,
            cx,
            &limited,
            "open_agent",
            json!({ "agent": "claude", "workspace": first, "folder": outside }),
        );
        assert_eq!(
            refused(&line).0,
            Some(i64::from(codes::SCOPE)),
            "agent {outside}: {line}"
        );
    }
    assert_eq!(blocks(cx), n, "nothing opened");
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "add_block",
        json!({ "workspace": first, "kind": "editor", "folder": "inside" }),
    );
    assert!(line["error"].is_null() && !said(&line).0, "{line}");
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "add_block",
        json!({ "workspace": first, "kind": "editor" }),
    );
    assert!(
        line["error"].is_null() && !said(&line).0,
        "without a folder it is the workspace's: {line}"
    );
    // A workspace with no folder gives a terminal nowhere inside to start.
    ws.update(cx, |ws, _| ws.space.folder = None);
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "add_block",
        json!({ "workspace": first, "kind": "terminal" }),
    );
    assert_eq!(refused(&line).0, Some(i64::from(codes::SCOPE)), "{line}");
    // An unlimited token goes anywhere.
    let all = with_token(&ws, cx, Scope::Control, None);
    let line = tool(
        handle,
        &ws,
        cx,
        &all,
        "add_block",
        json!({ "workspace": first, "kind": "editor", "folder": elsewhere.path().display().to_string() }),
    );
    assert!(line["error"].is_null() && !said(&line).0, "{line}");
}

#[gpui_kit::test]
fn a_limited_tokens_rule_keeps_its_folder_as_the_real_path_and_inside(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let home = tempfile::tempdir().unwrap();
    // The simplified form, as the rule keeps it (`\\?\C:\x` on Windows is not one a shell can start in).
    let real = dunce::canonicalize(home.path()).unwrap();
    std::fs::create_dir(home.path().join("sub")).unwrap();
    ws.update(cx, |ws, _| ws.space.folder = Some(real.clone()));
    let limited = with_token(&ws, cx, Scope::Full, Some(vec![first.as_str()]));
    let rule = |act: Value| {
        json!({ "automation": { "name": "R", "workspace": first,
            "when": { "every": { "minutes": 5 } }, "do": act } })
    };
    let stored = |cx: &mut TestAppContext, id: u64| {
        ws.read_with(cx, |ws, _| match &ws.automations.get(id).unwrap().act {
            crate::automations::Do::Command { folder, .. }
            | crate::automations::Do::Agent { folder, .. } => folder.clone(),
            other => panic!("{other:?}"),
        })
    };
    // "." is the workspace's folder, written down as the real path: what was checked is what runs.
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "save_automation",
        rule(json!({ "command": { "folder": ".", "text": "ls" } })),
    );
    assert!(!said(&line).0 && line["error"].is_null(), "{line}");
    assert_eq!(stored(cx, 1), Some(real.display().to_string()));
    let line = tool(
        handle,
        &ws,
        cx,
        &limited,
        "save_automation",
        rule(json!({ "agent": { "agent": "claude", "folder": "sub" } })),
    );
    assert!(line["error"].is_null(), "{line}");
    assert_eq!(stored(cx, 2), Some(real.join("sub").display().to_string()));
    #[cfg(unix)]
    {
        let elsewhere = tempfile::tempdir().unwrap();
        // A link swapped in for the folder after the save: the next run is skipped.
        std::fs::remove_dir(home.path().join("sub")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), home.path().join("sub")).unwrap();
        let line = tool(
            handle,
            &ws,
            cx,
            &limited,
            "run_automation",
            json!({ "automation": 2 }),
        );
        let (_, said) = said(&line);
        assert!(said.contains("Target missing"), "{said}");
        ws.read_with(cx, |ws, _| {
            let last = ws.automations.get(2).unwrap().last.clone().unwrap();
            assert_eq!(last.went, crate::automations::Went::Skipped);
        });
    }
    // A rule that names no folder, in a workspace that has none, has nowhere inside to start.
    ws.update(cx, |ws, _| ws.space.folder = None);
    for act in [
        json!({ "command": { "text": "ls" } }),
        json!({ "agent": { "agent": "claude" } }),
    ] {
        let line = tool(handle, &ws, cx, &limited, "save_automation", rule(act));
        assert_eq!(refused(&line).0, Some(i64::from(codes::SCOPE)), "{line}");
    }
}

/// `open_agent` with a worktree, as a token limited to `first` asks it: the answer once git ran.
#[cfg(unix)]
fn worktree_for(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    limited: &Token,
    first: &str,
) -> Value {
    let args = json!({ "agent": "claude", "workspace": first, "worktree": "feature" });
    let answers = api_later(
        handle,
        ws,
        cx,
        limited,
        7,
        "tools/call",
        json!({ "name": "open_agent", "arguments": args }),
    );
    serde_json::from_str(&super::bus::answer_line_soon(cx, &answers)).unwrap()
}

/// A worktree refused for a limited token: `403 scope`, like any folder outside its workspaces.
#[cfg(unix)]
fn assert_outside(line: &Value) {
    let (code, text) = refused(line);
    assert_eq!(code, Some(i64::from(codes::SCOPE)), "{line}");
    assert!(text.contains("main tree"), "{text}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_limited_tokens_worktree_is_made_only_where_git_says_the_main_tree_is_inside(
    cx: &mut TestAppContext,
) {
    let (handle, ws, _) = super::spawn::spawner(cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let limited = with_token(&ws, cx, Scope::Control, Some(vec![first.as_str()]));
    // The workspace's folder is a linked worktree of a repository whose main tree lies elsewhere.
    let (main, sides) = (committed_repo(), tempfile::tempdir().unwrap());
    let linked = sides.path().canonicalize().unwrap().join("linked");
    git_run(
        main.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            linked.to_str().unwrap(),
        ],
    );
    ws.update(cx, |ws, _| ws.space.folder = Some(linked.clone()));
    assert_outside(&worktree_for(handle, &ws, cx, &limited, &first));
    assert!(!main.path().join(".worktrees").exists(), "nothing made");
    // A repository whose main tree is the workspace's folder: the worktree is made.
    let inside = committed_repo();
    ws.update(cx, |ws, _| {
        ws.space.folder = Some(inside.path().canonicalize().unwrap())
    });
    let (failed, text) = said(&worktree_for(handle, &ws, cx, &limited, &first));
    assert!(!failed, "{text}");
    assert!(inside.path().join(".worktrees/feature").is_dir());
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_planted_git_file_cant_move_a_limited_tokens_worktree_outside(cx: &mut TestAppContext) {
    let (handle, ws, _) = super::spawn::spawner(cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let limited = with_token(&ws, cx, Scope::Control, Some(vec![first.as_str()]));
    // The folder's `.git` file leads to a git directory made inside it, which names (commondir)
    // a repository outside: git works in that one, so that is where the worktree would go.
    let (outside, home) = (committed_repo(), tempfile::tempdir().unwrap());
    let (outside_git, home) = (
        outside.path().canonicalize().unwrap().join(".git"),
        home.keep().canonicalize().unwrap(),
    );
    let fake = home.join("fake/.git/worktrees/wt");
    std::fs::create_dir_all(&fake).unwrap();
    std::fs::write(fake.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        fake.join("commondir"),
        format!("{}\n", outside_git.display()),
    )
    .unwrap();
    std::fs::write(
        fake.join("gitdir"),
        format!("{}\n", home.join(".git").display()),
    )
    .unwrap();
    std::fs::write(home.join(".git"), format!("gitdir: {}\n", fake.display())).unwrap();
    assert_eq!(
        git_run(&home, &["worktree", "list", "--porcelain"])
            .lines()
            .next(),
        Some(
            format!(
                "worktree {}",
                outside.path().canonicalize().unwrap().display()
            )
            .as_str()
        ),
        "git follows the commondir"
    );
    ws.update(cx, |ws, _| ws.space.folder = Some(home.clone()));
    assert_outside(&worktree_for(handle, &ws, cx, &limited, &first));
    assert!(!outside.path().join(".worktrees").exists(), "nothing made");
}

#[gpui_kit::test]
fn a_deleted_workspace_leaves_its_tokens_lists(cx: &mut TestAppContext) {
    let (handle, ws, dir) = with_data(cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    another_workspace(handle, &ws, cx);
    let token = with_token(&ws, cx, Scope::Control, Some(vec![first.as_str(), "w2"]));
    let id = token.id;
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.delete_space("w2", window, cx))
    })
    .unwrap();
    let kept = ws.read_with(cx, |ws, _| {
        ws.api
            .store
            .as_ref()
            .unwrap()
            .tokens
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .workspaces
            .clone()
    });
    assert_eq!(kept, Some(vec![first.clone()]), "w2 is gone from the list");
    // The deleted canvas's `.bak` would keep its id taken: remove it, so a new workspace really
    // takes the freed id, and is not the token's.
    let _ = std::fs::remove_file(crate::session::path(dir.path(), "w2").with_extension("json.bak"));
    super::templum_agent::control(
        handle,
        &ws,
        cx,
        "create_workspace",
        json!({ "name": "New" }),
    );
    let token = ws.read_with(cx, |ws, _| {
        assert!(ws.spaces().any(|s| s.id == "w2"), "w2 is reused");
        ws.api
            .store
            .as_ref()
            .unwrap()
            .tokens
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .clone()
    });
    let seen = state(handle, &ws, cx, &token);
    assert_eq!(
        seen.workspaces
            .iter()
            .map(|w| w.id.as_str())
            .collect::<Vec<_>>(),
        [first.as_str()]
    );
}

/// Saves a notify rule named `name` as `token`, in `space`: its number.
fn save_rule_as(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    token: &Token,
    space: &str,
    name: &str,
) -> u64 {
    let rule = json!({ "name": name, "workspace": space,
        "when": { "every": { "minutes": 10 } }, "do": { "notify": { "text": "hi" } } });
    let line = tool(
        handle,
        ws,
        cx,
        token,
        "save_automation",
        json!({ "automation": rule }),
    );
    let (failed, text) = said(&line);
    assert!(!failed, "{line}");
    text.trim_start_matches("Saved automation ")
        .trim_end_matches('.')
        .parse()
        .unwrap()
}

fn rule_on(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: u64) -> bool {
    ws.read_with(cx, |ws, _| ws.automations.get(id).unwrap().on)
}

#[gpui_kit::test]
fn the_automations_of_a_revoked_or_expired_token_are_turned_off(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let tokens: Vec<Token> = (0..4)
        .map(|_| with_token(&ws, cx, Scope::Control, None))
        .collect();
    let rules: Vec<u64> = tokens
        .iter()
        .enumerate()
        .map(|(n, t)| save_rule_as(handle, &ws, cx, t, &space, &format!("R{n}")))
        .collect();
    ws.read_with(cx, |ws, _| {
        let saved = &ws.api.store.as_ref().unwrap().saved_by;
        for (rule, token) in rules.iter().zip(&tokens) {
            assert_eq!(saved.get(rule), Some(&token.id));
        }
    });
    assert!(rules.iter().all(|r| rule_on(&ws, cx, *r)));
    // Revoke: only that token's rule goes off, and the person is told.
    ws.update(cx, |ws, cx| ws.revoke_token(tokens[0].id, cx));
    assert_eq!(
        rules
            .iter()
            .map(|r| rule_on(&ws, cx, *r))
            .collect::<Vec<_>>(),
        [false, true, true, true]
    );
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.api.said,
            ["Automation R0, saved by token 'phone', was turned off."]
        );
        assert!(
            !ws.api
                .store
                .as_ref()
                .unwrap()
                .saved_by
                .contains_key(&rules[0])
        );
    });
    // Expiry: the sweep at the snapshot finds it.
    ws.update(cx, |ws, _| {
        let store = ws.api.store.as_mut().unwrap();
        store
            .tokens
            .iter_mut()
            .find(|t| t.id == tokens[1].id)
            .unwrap()
            .expires = Some(1);
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.snapshot(window, cx))
    })
    .unwrap();
    assert_eq!(
        rules
            .iter()
            .map(|r| rule_on(&ws, cx, *r))
            .collect::<Vec<_>>(),
        [false, false, true, true]
    );
    // The person's own edit makes a rule theirs: it outlives the token.
    ws.update(cx, |ws, _| ws.api_unconfine(rules[2]));
    ws.update(cx, |ws, cx| ws.revoke_all_tokens(cx));
    assert_eq!(
        rules
            .iter()
            .map(|r| rule_on(&ws, cx, *r))
            .collect::<Vec<_>>(),
        [false, false, true, false],
        "Revoke all turns off the rest, not the person's"
    );
    ws.read_with(cx, |ws, _| assert_eq!(ws.api.said.len(), 3));
}

#[gpui_kit::test]
fn a_revoked_tokens_rule_doesnt_run_before_the_sweep_turns_it_off(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let token = with_token(&ws, cx, Scope::Control, None);
    let rule = save_rule_as(handle, &ws, cx, &token, &space, "R");
    let target = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.automation_target(ws.automations.get(rule).unwrap())
        })
    };
    assert_eq!(target(cx), Ok(()));
    // Revoked, and the sweep (every few seconds) hasn't looked yet.
    ws.update(cx, |ws, _| {
        ws.api.store.as_mut().unwrap().revoke(token.id);
    });
    assert_eq!(
        target(cx),
        Err("Target missing: its token was revoked".to_string())
    );
    // The person's own edit makes it theirs again.
    ws.update(cx, |ws, _| ws.api_unconfine(rule));
    assert_eq!(target(cx), Ok(()));
}

#[gpui_kit::test]
fn a_rule_the_token_turned_off_is_told_of_when_it_is_revoked(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let space = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let token = with_token(&ws, cx, Scope::Control, None);
    let rule = save_rule_as(handle, &ws, cx, &token, &space, "R");
    ws.update(cx, |ws, cx| ws.set_automation_on(rule, false, cx));
    assert!(!rule_on(&ws, cx, rule));
    ws.update(cx, |ws, cx| ws.revoke_token(token.id, cx));
    assert!(!rule_on(&ws, cx, rule), "it stays off");
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.api.said,
            ["Automation R, saved by token 'phone', stays off."]
        );
        assert!(ws.api.store.as_ref().unwrap().saved_by.is_empty());
    });
}

#[gpui_kit::test]
fn entries_for_automations_that_dont_exist_are_dropped_at_load(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    // `automations.json` was set aside and the numbers started again: only 1 exists now.
    std::fs::write(
        crate::automations::path(dir.path()),
        r#"{ "version": 1, "next_id": 2, "automations": [
            { "id": 1, "name": "R", "on": false, "when": { "every": { "minutes": 10 } },
              "do": { "notify": { "text": "hi" } }, "runs": 0, "since": 0 } ] }"#,
    )
    .unwrap();
    let mut store = Store::default();
    for id in [1, 7] {
        store.confined.insert(id, vec!["/home/x".into()]);
        store.saved_by.insert(id, 3);
    }
    templum_api::store::save(dir.path(), &store).unwrap();
    let (_, ws) = open_in(cx, None, Some(dir.path().to_path_buf()));
    let kept = ws.read_with(cx, |ws, _| {
        let store = ws.api.store.as_ref().unwrap();
        (
            store.confined.keys().copied().collect::<Vec<_>>(),
            store.saved_by.keys().copied().collect::<Vec<_>>(),
        )
    });
    assert_eq!(kept, (vec![1], vec![1]));
    // And the file says so too.
    let saved = templum_api::store::load(dir.path()).unwrap();
    assert_eq!(saved.confined.keys().copied().collect::<Vec<_>>(), [1]);
}

#[gpui_kit::test]
fn restarting_the_server_keeps_the_uses_it_holds(cx: &mut TestAppContext) {
    let (_, ws, _dir) = with_data(cx);
    turn_on(&ws, cx);
    ws.update(cx, |ws, cx| {
        ws.mint_token("phone", Scope::Read, None, None, cx)
    })
    .unwrap();
    let text = ws.read_with(cx, |ws, _| ws.api.shown.clone()).unwrap();
    assert_eq!(status(port(&ws, cx), "/v1/nope", Some(&text)), 404);
    ws.update(cx, |ws, _| ws.start_api());
    let last = ws.read_with(cx, |ws, _| {
        ws.api.store.as_ref().unwrap().tokens[0].last.clone()
    });
    assert_eq!(last.map(|u| u.addr), Some("127.0.0.1".to_string()));
}

#[test]
fn a_path_on_a_share_of_another_computer_is_refused_unless_a_folder_is_on_it() {
    use crate::workspace::api::refuses_share;
    use std::path::Prefix::{DeviceNS, Disk, UNC, Verbatim, VerbatimDisk, VerbatimUNC};
    let share =
        |server: &'static str, name: &'static str| Some(UNC(server.as_ref(), name.as_ref()));
    let attacker = share("attacker", "share");
    let disk = Some(Disk(b'C'));
    assert!(
        refuses_share(attacker, &[disk]),
        "a share, and no folder on it"
    );
    assert!(refuses_share(attacker, &[]));
    assert!(
        refuses_share(attacker, &[share("attacker", "other")]),
        "another share"
    );
    assert!(
        refuses_share(attacker, &[share("friend", "share")]),
        "another server"
    );
    assert!(
        !refuses_share(attacker, &[disk, share("ATTACKER", "Share")]),
        "a folder on the same share, in either case"
    );
    assert!(
        !refuses_share(
            Some(VerbatimUNC("attacker".as_ref(), "share".as_ref())),
            &[attacker]
        ),
        "verbatim or not, it is the same share"
    );
    assert!(refuses_share(
        Some(VerbatimUNC("attacker".as_ref(), "share".as_ref())),
        &[disk]
    ));
    assert!(
        refuses_share(Some(DeviceNS("pipe".as_ref())), &[disk]),
        "a device namespace"
    );
    // std matches `\\?\UNC\` case-sensitively: `\\?\unc\attacker\share` and
    // `\\?\GLOBALROOT\Device\Mup\…` parse as a plain `Verbatim` prefix, and still reach a share.
    assert!(
        refuses_share(Some(Verbatim("unc".as_ref())), &[disk]),
        "a lowercase verbatim UNC"
    );
    assert!(
        refuses_share(Some(Verbatim("GLOBALROOT".as_ref())), &[disk]),
        "the object manager's way to the redirector"
    );
    assert!(refuses_share(Some(Verbatim("unc".as_ref())), &[]));
    assert!(
        refuses_share(Some(Verbatim("unc".as_ref())), &[attacker]),
        "a UNC root is not the verbatim name"
    );
    assert!(
        !refuses_share(
            Some(Verbatim("GLOBALROOT".as_ref())),
            &[Some(Verbatim("globalroot".as_ref()))]
        ),
        "unless a folder is on the same one"
    );
    assert!(!refuses_share(disk, &[]), "a drive is the computer's own");
    assert!(!refuses_share(Some(VerbatimDisk(b'C')), &[]));
    assert!(!refuses_share(None, &[]), "a path with no prefix");
}
