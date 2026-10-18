//! `schedule` and `cancel_schedule` (docs/specs/agent-tools.md#schedule-and-cancel_schedule):
//! texts delivered later. Unix only (`mod.rs`): `cat` stands in for the programs, as in `spawn.rs`.

use std::time::{Duration, SystemTime};

use serde_json::json;
use templum_canvas::Wire;

use super::bus::{after, call, me, screen_until};
use super::delegate::{found, prints};
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

/// The block's menu, item `index`, then item `sub` of its submenu.
fn pick(handle: AnyWindowHandle, cx: &mut TestAppContext, id: BlockId, index: usize, sub: usize) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.right_click(("title", id.0 as usize), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(index, cx);
        window.render_frame(cx);
        window.within("submenu").within("popup-menu").click(sub, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// Whether the clock of the schedules of block `id` is drawn.
fn clock(handle: AnyWindowHandle, cx: &mut TestAppContext, id: BlockId) -> bool {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(gpui_kit::ElementId::Name(
                format!("schedules-{}", id.0).into(),
            ))
            .is_some()
    })
    .unwrap()
}

/// The caller's block shows a clock while it has schedules, and its menu's Cancel schedules ▸
/// cancels one, or All (docs/specs/agent-tools.md, What the person sees).
#[gpui_kit::test]
fn the_callers_block_shows_its_schedules_and_its_menu_cancels_them(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let plain = wired_plain(handle, &ws, cx, mine);
    let (_, them) = me(&ws, cx, plain);
    assert!(!clock(handle, cx, mine), "none yet");
    for args in [
        json!({ "block": them, "text": "go", "every": 10 }),
        json!({ "block": uid, "text": "look", "after": 60 }),
    ] {
        assert!(!call(handle, &ws, cx, &terminal, "schedule", args).0);
    }
    assert!(clock(handle, cx, mine));
    assert!(!clock(handle, cx, plain), "only on the asker's block");
    // Copy, Paste, —, Fullscreen, Disconnect from ▸, Cancel schedules ▸ (s1, s2, All).
    pick(handle, cx, mine, 5, 0);
    assert_eq!(ids(&ws, cx), [2]);
    // Cancel schedules ▸ (s2, All).
    pick(handle, cx, mine, 5, 1);
    assert_eq!(ids(&ws, cx), Vec::<u64>::new());
    assert!(!clock(handle, cx, mine));
}

/// The notices, once the terminals' threads and a few looks gave them time to come.
fn notices_once(ws: &Entity<Workspace>, cx: &mut TestAppContext, n: usize) -> Vec<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        after(cx, 250);
        let now = notices(ws, cx);
        if now.len() >= n {
            return now;
        }
        assert!(std::time::Instant::now() < deadline, "{now:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `wait_for` with a pattern and `wait: false` answers at once, and its first match, its timeout
/// or its block's closing comes back as one notice (docs/specs/agent-tools.md, wait_for with a
/// pattern).
#[gpui_kit::test]
fn a_watch_without_wait_types_its_match_back_once(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let other = wired_plain(handle, &ws, cx, mine);
    let (_, them) = me(&ws, cx, other);
    let watching = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.space.terminal(other).unwrap().term.watching()
        })
    };
    let watch = |pattern: &str, timeout: u64| json!({ "block": them, "pattern": pattern, "wait": false, "timeout": timeout });
    assert_eq!(
        call(handle, &ws, cx, &terminal, "wait_for", watch("ERROR", 300)),
        (
            false,
            format!("Watching {them} for \"ERROR\" as w1: a match is typed into your terminal.")
        )
    );
    prints(&ws, cx, other, b"ERROR one\r");
    let told = notices_once(&ws, cx, 1);
    assert!(
        told[0].starts_with(&format!(
            "[watch w1 on {them}: a line matches \"ERROR\"] The line is in the pasted text.\nERROR one\nIts last lines:\n"
        )),
        "{told:?}"
    );
    prints(&ws, cx, other, b"ERROR two\r");
    std::thread::sleep(Duration::from_millis(300));
    after(cx, 1_000);
    assert_eq!(notices(&ws, cx).len(), 1, "once");
    assert_eq!(watching(&ws, cx), 0, "no watch left");
    // Its timeout.
    ws.update(cx, |ws, _| ws.notices.clear());
    assert!(!call(handle, &ws, cx, &terminal, "wait_for", watch("never", 1)).0);
    after(cx, 1_300);
    assert_eq!(
        notices(&ws, cx),
        [format!(
            "[watch w2 on {them} ended: no line matched \"never\" in 1 s]"
        )]
    );
    assert_eq!(watching(&ws, cx), 0);
    // Its block closing.
    ws.update(cx, |ws, _| ws.notices.clear());
    assert!(!call(handle, &ws, cx, &terminal, "wait_for", watch("never", 300)).0);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close_now(&[other], Then::New, window, cx))
    })
    .unwrap();
    after(cx, 500);
    assert_eq!(
        notices(&ws, cx),
        [format!("[watch w3 on {them} ended: {them} was closed]")]
    );
    // The caller closing drops its watches, silently.
    let third = wired_plain(handle, &ws, cx, mine);
    let (_, three) = me(&ws, cx, third);
    let quiet = json!({ "block": three, "pattern": "never", "wait": false });
    assert!(!call(handle, &ws, cx, &terminal, "wait_for", quiet).0);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close_now(&[mine], Then::New, window, cx))
    })
    .unwrap();
    after(cx, 500);
    ws.read_with(cx, |ws, _| {
        assert!(ws.watches.is_empty());
        assert_eq!(ws.space.terminal(third).unwrap().term.watching(), 0);
    });
}

/// A schedule set for an agent never types its text into the shell that agent left: it comes back
/// refused, and the shell runs nothing.
#[gpui_kit::test]
fn a_schedule_for_an_agent_that_quit_isnt_typed_into_its_shell(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, _) = me(&ws, cx, mine);
    agent_without_notices(&ws, cx, mine);
    let other = wired_plain(handle, &ws, cx, mine);
    let (_, them) = me(&ws, cx, other);
    in_front(&ws, cx, other, true);
    found(&ws, cx, other);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(other).unwrap().seen.agent = Some("codex".into())
    });
    let every = json!({ "block": them, "text": "echo ran-in-the-shell", "every": 1 });
    assert!(!call(handle, &ws, cx, &terminal, "schedule", every).0);
    // Its agent quits: a snapshot finds none there.
    ws.update(cx, |ws, _| {
        let pane = ws.space.terminal_mut(other).unwrap();
        (pane.seen.agent, pane.seen.pid) = (None, None);
    });
    due_now(&ws, cx, 1);
    look(cx);
    assert_eq!(
        notices(&ws, cx),
        [format!(
            "[schedule s1 to {them} didn't go: {them} runs no agent now: a schedule for an agent \
             never types into its shell.]"
        )]
    );
    after(
        cx,
        crate::workspace::delegate::ENTER_AFTER.as_millis() as u64,
    );
    std::thread::sleep(Duration::from_millis(300));
    let shown = ws.read_with(cx, |ws, _| ws.space.terminal(other).unwrap().screen());
    assert!(!shown.contains("ran-in-the-shell"), "{shown}");
}
