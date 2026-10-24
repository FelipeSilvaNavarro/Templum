//! The requests inbox's model: what it lists, what is selected, and why each is asked
//! (docs/specs/ask-inbox.md).

use std::time::Duration;

use serde_json::json;
use templum_mcp::Reply;

use super::bus::{call_later, me};
use super::*;
use crate::board::Assignee;
use crate::workspace::inbox::Why;
use crate::workspace::{Handed, Waiting};

/// A terminal opened apart from the others, at row `row` of the canvas.
fn open_terminal(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    row: u8,
) -> BlockId {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(
                Rect::new(40.0, 800.0 + f32::from(row) * 400.0, 400.0, 300.0),
                window,
                cx,
            );
            ws.space.appearing.clear();
            ws.space.canvas.top().unwrap()
        })
    })
    .unwrap()
}

/// The keys of the requests the inbox lists.
fn listed(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<u64> {
    ws.read_with(cx, |ws, _| ws.inbox().iter().map(|w| w.key).collect())
}

/// The key of the request the inbox has selected.
fn selected(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Option<u64> {
    ws.read_with(cx, |ws, _| ws.inbox_selected().map(|w| w.key))
}

fn open_inbox(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    at: Option<u64>,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_inbox(at, window, cx))
    })
    .unwrap();
}

/// The terminal `terminal` asks `ask_user` question `n` on connection `n`: its wait's key.
fn ask(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    terminal: &str,
    n: u64,
) -> u64 {
    let asked = json!({ "question": format!("Q{n}?"), "options": ["Yes"] });
    let _ = call_later(handle, ws, cx, terminal, n, "ask_user", asked);
    ws.read_with(cx, |ws, _| ws.waits.last().expect("a wait").key)
}

fn answer(ws: &Entity<Workspace>, cx: &mut TestAppContext, key: u64) {
    ws.update(cx, |ws, cx| ws.answer(key, Reply::ok("Yes"), cx));
}

/// The terminal `by` (wired to `block`) waits for a line of `block`'s screen that never comes: a
/// wait that is no request of the person's. Its key.
fn watching(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    by: &str,
    block: &str,
    conn: u64,
) -> u64 {
    let watch = json!({ "block": block, "pattern": "never", "wait": true, "timeout": 300 });
    let _ = call_later(handle, ws, cx, by, conn, "wait_for", watch);
    ws.read_with(cx, |ws, _| ws.waits.last().expect("a wait").key)
}

#[gpui_kit::test]
fn the_inbox_lists_this_workspaces_requests_oldest_first(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, a) = three_blocks(handle, &ws, cx);
    let b = open_terminal(handle, &ws, cx, 0);
    let c = open_terminal(handle, &ws, cx, 1);
    let d = open_terminal(handle, &ws, cx, 2);
    let e = open_terminal(handle, &ws, cx, 3);
    let (a_id, _) = me(&ws, cx, a);
    let (b_id, _) = me(&ws, cx, b);
    let (c_id, _) = me(&ws, cx, c);
    let (_, d_uid) = me(&ws, cx, d);
    let (e_id, _) = me(&ws, cx, e);
    let k = ws.read_with(cx, |ws, _| ws.space.uids[&kanban].clone());
    // C waits for a line of D's screen, which never comes: not a request of the person's.
    ws.update(cx, |ws, cx| assert!(ws.connect(c, d, cx)));
    let _watching = call_later(
        handle,
        &ws,
        cx,
        &c_id,
        3,
        "wait_for",
        json!({ "block": d_uid, "pattern": "never", "wait": true, "timeout": 300 }),
    );
    // E asks from a workspace of its own.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.move_blocks(vec![e], None, window, cx))
    })
    .unwrap();
    let first = ask(handle, &ws, cx, &a_id, 1);
    let _ = call_later(handle, &ws, cx, &b_id, 2, "connect", json!({ "block": k }));
    let second = ws.read_with(cx, |ws, _| ws.waits.last().unwrap().key);
    ask(handle, &ws, cx, &e_id, 4);
    ws.read_with(cx, |ws, _| {
        assert!(
            ws.waits
                .iter()
                .any(|w| matches!(w.what, Waiting::Event { .. })),
            "C's wait_for is a wait, not a request"
        );
        assert_eq!(ws.waits.len(), 4);
    });
    assert_eq!(listed(&ws, cx), [first, second]);
}

#[gpui_kit::test]
fn why_says_the_card_the_task_and_the_agents_words(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_, _, a) = three_blocks(handle, &ws, cx);
    let (_, board, early) = board_with_a_card(handle, &ws, cx);
    let (terminal, uid) = me(&ws, cx, a);
    // The card it works on is the newest one not in the last column; a card Done, though newer,
    // is finished.
    let (working, done) = ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            let columns: Vec<u64> = b
                .board(board)
                .unwrap()
                .columns
                .iter()
                .map(|c| c.id)
                .collect();
            let working = b.add_card(board, columns[1]).unwrap();
            let done = b.add_card(board, columns[2]).unwrap();
            for (card, title) in [(early, "Old"), (working, "Parser"), (done, "Shipped")] {
                b.edit_card(board, card, |c| {
                    c.title = title.into();
                    c.assignee = Some(Assignee::Block(uid.clone()));
                });
            }
            (working, done)
        })
    });
    assert!(early < working && working < done);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(a).unwrap().handed = Some(Handed {
            text: "Write the parser\nthen tests\nthen docs\nthen release".into(),
            ..Handed::new("x", Duration::from_secs(600))
        });
    });
    let asked = json!({ "question": "Which format?", "options": ["A"], "why": "Need the format" });
    let _ = call_later(handle, &ws, cx, &terminal, 1, "ask_user", asked);
    let why = ws.read_with(cx, |ws, _| ws.why(ws.inbox_selected().unwrap()));
    assert_eq!(
        why,
        Why {
            card: Some((working, "Parser".into())),
            task: Some("Write the parser\nthen tests\nthen docs".into()),
            agent: Some("Need the format".into()),
        }
    );
    // Nothing known, nothing said.
    let b = open_terminal(handle, &ws, cx, 0);
    let (other, _) = me(&ws, cx, b);
    let _ = call_later(
        handle,
        &ws,
        cx,
        &other,
        2,
        "ask_user",
        json!({ "question": "Q?" }),
    );
    let why = ws.read_with(cx, |ws, _| ws.why(ws.inbox().into_iter().last().unwrap()));
    assert_eq!(
        why,
        Why {
            card: None,
            task: None,
            agent: None
        }
    );
}

#[gpui_kit::test]
fn answering_the_selected_request_selects_the_next(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let keys: Vec<u64> = (1..=3)
        .map(|n| ask(handle, &ws, cx, &terminal, n))
        .collect();
    assert_eq!(
        selected(&ws, cx),
        Some(keys[0]),
        "the first, as none is chosen"
    );
    open_inbox(handle, &ws, cx, Some(keys[1]));
    assert_eq!(selected(&ws, cx), Some(keys[1]));
    answer(&ws, cx, keys[1]);
    assert_eq!(selected(&ws, cx), Some(keys[2]));
    answer(&ws, cx, keys[2]);
    assert_eq!(selected(&ws, cx), Some(keys[0]), "none after it: the first");
    // A new request joins the end and doesn't take the selection.
    let newer = ask(handle, &ws, cx, &terminal, 4);
    assert_eq!(listed(&ws, cx), [keys[0], newer]);
    assert_eq!(selected(&ws, cx), Some(keys[0]));
}

#[gpui_kit::test]
fn a_selected_requests_block_moved_away_leaves_the_inbox(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let a = first_id(&ws, cx);
    let b = open_terminal(handle, &ws, cx, 0);
    let c = open_terminal(handle, &ws, cx, 1);
    let keys: Vec<u64> = [a, b, c]
        .into_iter()
        .zip(1..)
        .map(|(id, n)| {
            let (terminal, _) = me(&ws, cx, id);
            ask(handle, &ws, cx, &terminal, n)
        })
        .collect();
    open_inbox(handle, &ws, cx, Some(keys[1]));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.move_blocks(vec![b], None, window, cx))
    })
    .unwrap();
    assert_eq!(listed(&ws, cx), [keys[0], keys[2]]);
    assert_eq!(selected(&ws, cx), Some(keys[2]), "the next");
    // Its block closes: the request ends with it, and nothing panics.
    open_inbox(handle, &ws, cx, Some(keys[2]));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(c, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    assert_eq!(listed(&ws, cx), [keys[0]]);
    assert_eq!(selected(&ws, cx), Some(keys[0]));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.waits.len(), 2, "A's, and B's in its workspace");
    });
}

#[gpui_kit::test]
fn the_inbox_and_the_git_popup_are_never_open_together(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    ws.read_with(cx, |ws, _| assert!(ws.git_open && !ws.inbox_open));
    open_inbox(handle, &ws, cx, None);
    ws.read_with(cx, |ws, _| assert!(ws.inbox_open && !ws.git_open));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_git(window, cx))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.git_open && !ws.inbox_open));
    // The inbox's own toggle opens it, then closes it.
    for open in [true, false] {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.toggle_inbox(window, cx))
        })
        .unwrap();
        ws.read_with(cx, |ws, _| assert_eq!(ws.inbox_open, open));
    }
}

/// Another workspace's requests are not the shown one's: switching clears the selection, and a
/// request that is not listed (answered, or another workspace's) can't be selected. Nothing
/// ending elsewhere moves what the person is reading.
#[gpui_kit::test]
fn an_unrelated_wait_ending_leaves_the_selection(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let a = first_id(&ws, cx);
    let [b, c, d, e, f] = [0, 1, 2, 3, 4].map(|row| open_terminal(handle, &ws, cx, row));
    let ids = [a, b, c, d].map(|id| me(&ws, cx, id).0);
    let keys: Vec<u64> = ids
        .iter()
        .zip(1..)
        .map(|(terminal, n)| ask(handle, &ws, cx, terminal, n))
        .collect();
    let (watcher, _) = me(&ws, cx, e);
    let (_, watched) = me(&ws, cx, f);
    ws.update(cx, |ws, cx| assert!(ws.connect(e, f, cx)));
    let unrelated = watching(handle, &ws, cx, &watcher, &watched, 9);
    ws.read_with(cx, |ws, _| {
        let wait = ws.waits.iter().find(|w| w.key == unrelated).unwrap();
        assert!(matches!(wait.what, Waiting::Event { .. }), "it waits");
    });
    // C and D, and their requests, go to a workspace of their own.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.move_blocks(vec![c, d], None, window, cx))
    })
    .unwrap();
    open_inbox(handle, &ws, cx, Some(keys[1]));
    assert_eq!(selected(&ws, cx), Some(keys[1]));
    // Not listed here: the selection stays.
    open_inbox(handle, &ws, cx, Some(keys[2]));
    assert_eq!(selected(&ws, cx), Some(keys[1]));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.switch_to(1, window, cx))
    })
    .unwrap();
    assert_eq!(listed(&ws, cx), [keys[2], keys[3]]);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.inbox_at, None, "switching clears it")
    });
    assert_eq!(selected(&ws, cx), Some(keys[2]), "the first there");
    ws.update(cx, |ws, cx| {
        ws.answer(unrelated, Reply::err("timed out"), cx)
    });
    assert_eq!(
        selected(&ws, cx),
        Some(keys[2]),
        "not the last: 12 is not 20"
    );
    // A request answered since can't be selected either.
    answer(&ws, cx, keys[2]);
    open_inbox(handle, &ws, cx, Some(keys[2]));
    assert_eq!(selected(&ws, cx), Some(keys[3]));
}

/// A block closes in a workspace not shown, while a `wait_for_idle` waits for it: that wait ends
/// inside the swap that shows its workspace for a moment, and the selection here stays.
#[gpui_kit::test]
fn a_block_closing_in_a_hidden_workspace_leaves_the_selection(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let a = first_id(&ws, cx);
    let b = open_terminal(handle, &ws, cx, 0);
    let c = open_terminal(handle, &ws, cx, 1);
    let keys: Vec<u64> = [a, b, c]
        .into_iter()
        .zip(1..)
        .map(|(id, n)| {
            let (terminal, _) = me(&ws, cx, id);
            ask(handle, &ws, cx, &terminal, n)
        })
        .collect();
    // The hidden workspace: Y opened X and waits for its turn.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.new_space(window, cx))
    })
    .unwrap();
    let (x, y) = (
        open_terminal(handle, &ws, cx, 0),
        open_terminal(handle, &ws, cx, 1),
    );
    let ((x_terminal, x_uid), (y_terminal, y_uid)) = (me(&ws, cx, x), me(&ws, cx, y));
    ws.update(cx, |ws, cx| {
        assert!(ws.connect(y, x, cx));
        let pane = ws.space.terminal_mut(x).unwrap();
        pane.seen.agent = Some("codex".into());
        pane.opened_by = Some(y_uid);
        pane.handed = Some(Handed::new(&y_terminal, Duration::from_secs(600)));
    });
    let idle = json!({ "block": x_uid });
    let _waiting = call_later(handle, &ws, cx, &y_terminal, 9, "wait_for_idle", idle);
    ws.read_with(cx, |ws, _| {
        assert!(
            ws.waits
                .iter()
                .any(|w| matches!(w.what, Waiting::Turn { .. }))
        )
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.switch_to(0, window, cx))
    })
    .unwrap();
    open_inbox(handle, &ws, cx, Some(keys[1]));
    let (failed, text) = super::bus::call(
        handle,
        &ws,
        cx,
        &y_terminal,
        "close_block",
        json!({ "block": x_uid }),
    );
    assert!(!failed, "{text}");
    ws.read_with(cx, |ws, _| {
        assert!(
            !ws.waits
                .iter()
                .any(|w| w.from == y_terminal && matches!(w.what, Waiting::Turn { .. })),
            "the wait ended"
        );
    });
    assert_eq!(selected(&ws, cx), Some(keys[1]));
    let _ = x_terminal;
}

/// Closing the inbox gives the keyboard back to the canvas only when the inbox had it.
#[gpui_kit::test]
fn closing_the_inbox_keeps_the_keyboard_where_it_went(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let toggle = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.toggle_inbox(window, cx))
        })
        .unwrap();
    };
    let focus = |cx: &mut TestAppContext, canvas: bool| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let handle = if canvas {
                    &ws.focus_handle
                } else {
                    &ws.inbox_focus
                };
                handle.contains_focused(window, cx)
            })
        })
        .unwrap()
    };
    // The inbox has the keyboard it took: closed, the canvas has it back.
    toggle(cx);
    assert!(focus(cx, false));
    toggle(cx);
    assert!(focus(cx, true));
    // The person clicked elsewhere (the settings field stands for it): closing leaves it there.
    toggle(cx);
    let elsewhere = ws.read_with(cx, |ws, _| ws.settings_focus.clone());
    cx.update_window(handle, |_, window, cx| elsewhere.focus(window, cx))
        .unwrap();
    toggle(cx);
    assert!(!focus(cx, true) && !focus(cx, false));
}

#[gpui_kit::test]
fn a_request_keeps_when_it_arrived(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, first);
    let key = ask(handle, &ws, cx, &terminal, 1);
    super::bus::after(cx, 90_000);
    let waited = ws.read_with(cx, |ws, cx| {
        let wait = ws.inbox().into_iter().find(|w| w.key == key).unwrap();
        cx.background_executor().now() - wait.since
    });
    assert_eq!(waited, Duration::from_secs(90));
}
