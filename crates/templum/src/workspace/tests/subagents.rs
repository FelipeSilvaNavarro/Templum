//! Claude Code's subagents (docs/specs/claude-subagents.md): their parent busy while they work, a
//! task answered by its real end. The hooks' inputs have the shapes Claude Code 2.1.286 sent
//! (docs/plans/2026-10-22-claude-subagents.md, The probe), turned into events as `templum hook
//! claude` does.

use serde_json::{Value, json};

use super::bus::{after, answered, call_later, event, me};
#[cfg(unix)]
use super::delegate::hand_over;
use super::delegate::two_agents;
use super::*;

/// The Claude Code hook input `input`, from the agent in `terminal`, as `templum hook claude`
/// hands it to the workspace.
fn hook(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    terminal: &str,
    input: Value,
) {
    let said = templum_mcp::hooks::claude(&input).expect("an event");
    event(
        handle,
        ws,
        cx,
        terminal,
        serde_json::to_value(said).unwrap(),
    );
}

/// A subagent's start, its type `kind`.
fn started(id: &str, kind: &str) -> Value {
    json!({ "hook_event_name": "SubagentStart", "agent_id": id, "agent_type": kind,
        "transcript_path": "/p/s1.jsonl" })
}

/// `Stop`, its turn's answer `said`, while the subagents `ids` still work.
fn stop(said: &str, ids: &[&str]) -> Value {
    let tasks: Vec<Value> = ids
        .iter()
        .map(|id| json!({ "id": id, "type": "subagent", "status": "running" }))
        .collect();
    json!({ "hook_event_name": "Stop", "last_assistant_message": said,
        "background_tasks": tasks })
}

/// A background subagent's stop: it is still among the running as it stops, with `others`.
fn stopped(id: &str, others: &[&str]) -> Value {
    let tasks: Vec<Value> = std::iter::once(&id)
        .chain(others)
        .map(|id| json!({ "id": id, "type": "subagent", "status": "running" }))
        .collect();
    json!({ "hook_event_name": "SubagentStop", "agent_id": id, "agent_type": "Explore",
        "background_tasks": tasks })
}

/// The prompt Claude Code gives itself with a subagent's report.
fn report(id: &str) -> Value {
    let prompt = format!(
        "<task-notification>\n<task-id>{id}</task-id>\n<status>completed</status>\n\
         <summary>Agent finished</summary>\n</task-notification>"
    );
    json!({ "hook_event_name": "UserPromptSubmit", "prompt": prompt })
}

/// A tool's end in the subagent `id`, or in the agent itself.
fn tool(id: Option<&str>) -> Value {
    match id {
        Some(id) => json!({ "hook_event_name": "PostToolUse", "tool_name": "Read",
            "agent_id": id, "agent_type": "Explore" }),
        None => json!({ "hook_event_name": "PostToolUse", "tool_name": "Read" }),
    }
}

/// What the dot of terminal block `id` says.
fn dot(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> Option<String> {
    ws.read_with(cx, |ws, _| {
        crate::block::status_dot(ws, ws.space.terminal(id).unwrap())
            .map(|(_, text)| text.to_string())
    })
}

/// A Claude Code whose turn ends while its background subagents work is still at work: the task
/// it was handed ends with the answer of the turn that takes the last report, not with the
/// interim message, also when a subagent stops during the turn that takes another's report and
/// that turn ends first (seen in the probe). Its prompt sitting idle meanwhile ends nothing.
#[gpui_kit::test]
fn a_task_ends_with_the_turn_after_its_subagents_reports(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "review the parser", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    let prompt = json!({ "hook_event_name": "UserPromptSubmit", "prompt": "[from …] review" });
    hook(handle, &ws, cx, &worker, prompt);
    hook(handle, &ws, cx, &worker, started("a1", "Explore"));
    hook(handle, &ws, cx, &worker, started("a2", "general-purpose"));
    hook(handle, &ws, cx, &worker, tool(Some("a1")));
    hook(
        handle,
        &ws,
        cx,
        &worker,
        stop("I've started two.", &["a1", "a2"]),
    );
    assert!(
        answers.try_recv().is_err(),
        "the interim message isn't its answer"
    );
    assert_eq!(
        dot(&ws, cx, theirs).as_deref(),
        Some("Working: 2 subagents")
    );
    let idle = json!({ "hook_event_name": "Notification", "notification_type": "idle_prompt" });
    hook(handle, &ws, cx, &worker, idle);
    assert!(
        answers.try_recv().is_err(),
        "an idle prompt while they work"
    );
    hook(handle, &ws, cx, &worker, stopped("a1", &["a2"]));
    hook(handle, &ws, cx, &worker, report("a1"));
    assert_eq!(dot(&ws, cx, theirs).as_deref(), Some("Working: 1 subagent"));
    // The other stops during the turn that takes the first's report, which ends before its own.
    hook(handle, &ws, cx, &worker, stopped("a2", &[]));
    hook(
        handle,
        &ws,
        cx,
        &worker,
        stop("Waiting for the other.", &[]),
    );
    assert!(answers.try_recv().is_err(), "a report is still to come");
    assert_eq!(dot(&ws, cx, theirs).as_deref(), Some("Working"));
    hook(handle, &ws, cx, &worker, report("a2"));
    hook(
        handle,
        &ws,
        cx,
        &worker,
        stop("Both found it: parse.rs:42.", &[]),
    );
    assert_eq!(
        answered(&answers),
        (false, "Both found it: parse.rs:42.".to_string())
    );
    assert_eq!(dot(&ws, cx, theirs), None, "at rest");
}

/// A subagent's report starts a turn of Claude Code's own: a task typed a moment before it isn't
/// taken by it (it waits for its own prompt's hook).
#[gpui_kit::test]
fn a_subagents_report_doesnt_take_a_task_in(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    hook(handle, &ws, cx, &worker, started("a1", "Explore"));
    hook(handle, &ws, cx, &worker, stop("Started one.", &["a1"]));
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    hook(handle, &ws, cx, &worker, stopped("a1", &[]));
    hook(handle, &ws, cx, &worker, report("a1"));
    let taken = ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(theirs)
            .unwrap()
            .handed
            .as_ref()
            .map(|h| h.taken)
    });
    assert_eq!(taken, Some(false));
    hook(handle, &ws, cx, &worker, stop("a1 found nothing.", &[]));
    assert!(
        answers.try_recv().is_err(),
        "the report's turn isn't the task's"
    );
}

/// A dialog a subagent opened in its parent's screen stays its parent's state until that
/// subagent's tool ends: another subagent's tool, or the parent's own, doesn't close it.
#[gpui_kit::test]
fn a_subagents_dialog_stays_until_its_own_tool_ends(cx: &mut TestAppContext) {
    let (handle, ws, _, theirs) = two_agents(cx);
    let (worker, _) = me(&ws, cx, theirs);
    let prompt = json!({ "hook_event_name": "UserPromptSubmit", "prompt": "go" });
    hook(handle, &ws, cx, &worker, prompt);
    hook(handle, &ws, cx, &worker, started("a1", "Explore"));
    hook(handle, &ws, cx, &worker, started("a2", "Plan"));
    let asks = json!({ "hook_event_name": "PermissionRequest", "tool_name": "Bash",
        "agent_id": "a1", "agent_type": "Explore" });
    hook(handle, &ws, cx, &worker, asks);
    let waiting = Some("Waiting for you: Allow Bash?".to_string());
    assert_eq!(dot(&ws, cx, theirs), waiting);
    hook(handle, &ws, cx, &worker, tool(Some("a2")));
    assert_eq!(dot(&ws, cx, theirs), waiting, "another subagent's tool");
    hook(handle, &ws, cx, &worker, tool(None));
    assert_eq!(dot(&ws, cx, theirs), waiting, "its parent's own tool");
    hook(handle, &ws, cx, &worker, tool(Some("a1")));
    assert_eq!(
        dot(&ws, cx, theirs).as_deref(),
        Some("Working: 2 subagents")
    );
}

/// A subagent's tool end doesn't take a task Claude Code queued behind its turn into that turn:
/// the turn's end is still not the task's answer.
#[gpui_kit::test]
fn a_subagents_tool_end_doesnt_take_a_queued_task_in(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let prompt = json!({ "hook_event_name": "UserPromptSubmit", "prompt": "the person's" });
    hook(handle, &ws, cx, &worker, prompt);
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    // Its Enter, while the turn works: queued behind it.
    let prompt = json!({ "hook_event_name": "UserPromptSubmit", "prompt": "[from …] go" });
    hook(handle, &ws, cx, &worker, prompt);
    hook(handle, &ws, cx, &worker, tool(Some("a1")));
    hook(handle, &ws, cx, &worker, stop("The person's answer.", &[]));
    assert!(answers.try_recv().is_err(), "the turn it was queued behind");
    hook(handle, &ws, cx, &worker, stop("The task's answer.", &[]));
    assert_eq!(
        answered(&answers),
        (false, "The task's answer.".to_string())
    );
}

/// A subagent stopped without a word (from Claude Code's panel) doesn't keep its parent busy: the
/// next turn's end, which names none at work, answers.
#[gpui_kit::test]
fn a_subagent_gone_without_a_word_doesnt_keep_its_parent_busy(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go", "wait": true });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    let prompt = json!({ "hook_event_name": "UserPromptSubmit", "prompt": "[from …] go" });
    hook(handle, &ws, cx, &worker, prompt);
    hook(handle, &ws, cx, &worker, started("a1", "Explore"));
    hook(handle, &ws, cx, &worker, stop("Done.", &[]));
    assert_eq!(answered(&answers), (false, "Done.".to_string()));
}

/// While its subagents work after its turn ended, its still screen, typed into or not, neither
/// ends its task nor makes it seem stuck: their hooks tell.
#[cfg(unix)]
#[gpui_kit::test]
fn a_parent_awaiting_its_subagents_isnt_ended_by_its_screen(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let task = json!({ "block": them, "task": "go" });
    let (failed, text) = hand_over(handle, &ws, cx, (&terminal, theirs), task, true);
    assert!(!failed, "{text}");
    hook(handle, &ws, cx, &worker, started("a1", "Explore"));
    hook(handle, &ws, cx, &worker, stop("Started one.", &["a1"]));
    // The person typed into it: its screen alone would rest it after five still looks.
    ws.update(cx, |ws, _| {
        ws.space.terminal(theirs).unwrap().typed.set(true)
    });
    for _ in 0..35 {
        after(cx, 1000);
    }
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(theirs).unwrap();
        assert_eq!(pane.state, Some(AgentState::Working));
        assert!(pane.handed.is_some(), "no answer from its screen");
    });
}
