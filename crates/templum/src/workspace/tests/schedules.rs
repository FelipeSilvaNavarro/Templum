//! `schedule` and `cancel_schedule` (docs/specs/agent-tools.md#schedule-and-cancel_schedule):
//! texts delivered later. Unix only (`mod.rs`): `cat` stands in for the programs, as in `spawn.rs`.

use std::time::{Duration, SystemTime};

use serde_json::json;
use templum_canvas::Wire;

use super::bus::{after, call, me, screen_until};
use super::delegate::found;
use super::spawn::spawner;
use super::*;
use crate::workspace::Handed;
use crate::workspace::history::Then;

/// Schedule `id`'s time is now: the test's clock moves the timers, not the computer's.
fn due_now(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: u64) {
    ws.update(cx, |ws, _| {
        let s = ws.schedules.iter_mut().find(|s| s.id == id).unwrap();
        s.next = SystemTime::now() - Duration::from_secs(1);
    });
}

/// The next look at the schedules.
fn look(cx: &mut TestAppContext) {
    after(
        cx,
        crate::workspace::schedules::SCHEDULE_LOOK.as_millis() as u64,
    );
}

/// A terminal the person opened, wired to `mine`.
fn wired_plain(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    mine: BlockId,
) -> BlockId {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(Rect::new(40.0, 900.0, 400.0, 300.0), window, cx);
            let other = ws.space.canvas.top().unwrap();
            assert!(ws.connect(mine, other, cx));
            other
        })
    })
    .unwrap()
}

fn notices(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<String> {
    ws.read_with(cx, |ws, _| {
        ws.notices.iter().map(|n| n.text.clone()).collect()
    })
}

fn ids(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<u64> {
    ws.read_with(cx, |ws, _| ws.schedules.iter().map(|s| s.id).collect())
}

/// The caller runs an agent that takes no notice now: they wait in `notices`.
fn agent_without_notices(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) {
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().seen.agent = Some("claude".into())
    });
}

#[gpui_kit::test]
fn a_schedule_goes_at_its_time_to_the_caller_and_a_terminal(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "schedule",
        json!({ "block": uid, "text": "check the build", "after": 2 }),
    );
    assert_eq!(
        (failed, text.as_str()),
        (false, "Scheduled s1: to you in 2 minutes.")
    );
    let whoami = call(handle, &ws, cx, &terminal, "whoami", json!({})).1;
    assert!(whoami.contains("\nschedule s1: to you, next "), "{whoami}");
    look(cx);
    assert!(notices(&ws, cx).is_empty(), "not yet");
    due_now(&ws, cx, 1);
    look(cx);
    assert_eq!(
        notices(&ws, cx),
        ["[scheduled s1] Its text is in the pasted text.\ncheck the build"]
    );
    assert_eq!(ids(&ws, cx), Vec::<u64>::new(), "a one-time schedule ends");
    // A wired terminal running no agent, every minute: typed with Enter each time.
    let plain = wired_plain(handle, &ws, cx, mine);
    let (_, them) = me(&ws, cx, plain);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "schedule",
        json!({ "block": them, "text": "echo tick", "every": 1 }),
    );
    assert!(!failed, "{text}");
    assert!(
        text.starts_with(&format!(
            "Scheduled s2: to {them} in 1 minute, then every 1 minute until "
        )),
        "{text}"
    );
    for times in [2, 4] {
        due_now(&ws, cx, 2);
        look(cx);
        after(
            cx,
            crate::workspace::delegate::ENTER_AFTER.as_millis() as u64,
        );
        // The terminal echoes it, and `cat` prints it back once Enter came.
        screen_until(&ws, cx, plain, |shown| {
            shown.matches("echo tick").count() >= times
        });
    }
    assert_eq!(ids(&ws, cx), [2], "a repeating schedule stays");
    assert_eq!(
        call(
            handle,
            &ws,
            cx,
            &terminal,
            "cancel_schedule",
            json!({ "schedule": "s2" })
        ),
        (false, "Cancelled s2.".into())
    );
    assert_eq!(
        call(
            handle,
            &ws,
            cx,
            &terminal,
            "cancel_schedule",
            json!({ "schedule": "2" })
        ),
        (true, "s2 isn't one of your schedules.".into())
    );
}

#[gpui_kit::test]
fn a_schedule_that_cant_go_says_so_and_a_closed_caller_ends_its_schedules(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let other = wired_plain(handle, &ws, cx, mine);
    let (worker, them) = me(&ws, cx, other);
    let every = json!({ "block": them, "text": "go on", "every": 1 });
    assert!(!call(handle, &ws, cx, &terminal, "schedule", every.clone()).0);
    // Unwired: refused at its time, and the caller hears so; it tries again at its next.
    ws.update(cx, |ws, cx| {
        assert!(ws.disconnect(Wire::new(mine, other).unwrap(), cx))
    });
    due_now(&ws, cx, 1);
    look(cx);
    assert_eq!(
        notices(&ws, cx),
        [format!(
            "[schedule s1 to {them} didn't go: {them} isn't connected to you. Ask the person with \
             connect(\"{them}\").]"
        )]
    );
    assert_eq!(ids(&ws, cx), [1]);
    // An agent on a task, the caller's included, takes no schedule's text.
    in_front(&ws, cx, other, true);
    found(&ws, cx, other);
    ws.update(cx, |ws, cx| {
        assert!(ws.connect(mine, other, cx));
        let pane = ws.space.terminal_mut(other).unwrap();
        pane.seen.agent = Some("codex".into());
        pane.handed = Some(Handed {
            id: 7,
            ..Handed::new(&terminal, Duration::from_secs(600))
        });
        ws.notices.clear();
    });
    due_now(&ws, cx, 1);
    look(cx);
    assert_eq!(
        notices(&ws, cx),
        [format!(
            "[schedule s1 to {them} didn't go: {them} is on a task (t7): a schedule hands a task \
             only to an agent that has none.]"
        )]
    );
    assert!(
        ws.read_with(cx, |ws, _| ws
            .pane(&worker)
            .unwrap()
            .handed
            .as_ref()
            .unwrap()
            .id)
            == 7,
        "its task stays"
    );
    // With no task, the agent gets the text as a task the schedule names, whose end comes back.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(other).unwrap().handed = None;
        ws.notices.clear();
    });
    due_now(&ws, cx, 1);
    look(cx);
    assert!(notices(&ws, cx).is_empty(), "{:?}", notices(&ws, cx));
    screen_until(&ws, cx, other, |shown| shown.contains(", scheduled s1] "));
    ws.read_with(cx, |ws, _| {
        let handed = ws.pane(&worker).unwrap().handed.as_ref().unwrap();
        assert_eq!(handed.by, terminal, "its end comes back to the asker");
    });
    // Seven days on, a repeating schedule ends, and says so.
    ws.update(cx, |ws, _| {
        ws.notices.clear();
        ws.schedules[0].until = SystemTime::now() - Duration::from_secs(1);
    });
    due_now(&ws, cx, 1);
    look(cx);
    assert_eq!(notices(&ws, cx), ["[schedule s1 ended: 7 days]"]);
    assert_eq!(ids(&ws, cx), Vec::<u64>::new());
    // The caller's block closing ends its schedules, silently.
    assert!(!call(handle, &ws, cx, &terminal, "schedule", every).0);
    assert_eq!(ids(&ws, cx), [2]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close_now(&[mine], Then::New, window, cx))
    })
    .unwrap();
    look(cx);
    assert_eq!(ids(&ws, cx), Vec::<u64>::new());
}
