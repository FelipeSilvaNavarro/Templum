//! A workspace of this very process mirrored by another of its workspaces, over loopback
//! (docs/specs/templum-to-templum.md, Constraints ▸ Tests): one headless `Workspace` is both
//! ends. Its API listens on this computer, a Full share of its first workspace is made with
//! `share_workspace` and connected into `r1` with `connect_code` (keys in a mock keyring). The
//! share is limited to the first workspace, so `r1` is never in what the mirror sees.

use keyring_core::mock;
use serde_json::json;
use std::sync::Arc;
use templum_api::client::{Keys, SERVICE_DEV, Status, Why};
use templum_api::code;
use templum_api::store::{Scope, Way};

use super::api::{port, state, turn_on, with_data, with_token};
use super::bus::{call_later, me};
use super::remote::{pump_home, remote_of};
use super::templum_agent::control;
use super::*;
use crate::workspace::api::ShareDraft;
use crate::workspace::remote::Connecting;
use crate::workspace::{Body, Waiting};

/// The Full share of the first workspace, connected here as `r1` and live: the mirror's own keys
/// in a mock keyring, set before its connections start.
fn mirrored(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    dir: &std::path::Path,
) -> (String, Arc<mock::Store>) {
    turn_on(ws, cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    let address = format!("http://127.0.0.1:{}", port(ws, cx));
    let (_, shared) = ws
        .update(cx, |ws, cx| {
            ws.share_workspace(
                ShareDraft {
                    workspace: first.clone(),
                    name: "Itself".into(),
                    scope: Scope::Full,
                    expires: None,
                    way: Way::Public,
                    address,
                },
                cx,
            )
        })
        .expect("shared");
    let store = mock::Store::new().unwrap();
    ws.update(cx, |ws, _| {
        ws.keys = Keys::with_store(store.clone(), SERVICE_DEV, Some(dir));
    });
    let draft = Connecting {
        code: code::decode(&shared).unwrap(),
        access: None,
        proxy: None,
        file: false,
    };
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.connect_code(draft, window, cx))
        })
        .unwrap()
        .expect("connecting");
    assert_eq!(id, "r1");
    pump_home(ws, cx, "r1 to go live", |cx| {
        ws.read_with(cx, |ws, _| {
            let remote = remote_of(ws);
            remote.status == Status::Live && remote.home.is_some() && ws.space.id == "r1"
        })
    });
    (first, store)
}

/// The rectangle of block `id` in workspace `space`.
fn rect_at(ws: &Workspace, space: &str, id: BlockId) -> Rect {
    ws.spaces()
        .find(|s| s.id == space)
        .unwrap()
        .canvas
        .get(id)
        .unwrap()
        .rect
}

fn rect_in(ws: &Entity<Workspace>, cx: &mut TestAppContext, space: &str, id: BlockId) -> Rect {
    ws.read_with(cx, |ws, _| rect_at(ws, space, id))
}

#[gpui_kit::test]
fn a_workspace_mirrors_itself_over_loopback(cx: &mut TestAppContext) {
    let (handle, ws, dir) = with_data(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let (term_id, _) = me(&ws, cx, terminal);
    let uids = ws.read_with(cx, |ws, _| {
        [terminal, note, kanban].map(|id| ws.space.uids[&id].clone())
    });
    // A question asked from its terminal.
    let asked = json!({ "question": "Which database?", "options": ["Postgres", "SQLite"] });
    let answers = call_later(handle, &ws, cx, &term_id, 31, "ask_user", asked);
    let (first, _store) = mirrored(handle, &ws, cx, dir.path());

    // 1. r1 shows the first workspace's blocks at its rectangles: one viewer, two cards.
    pump_home(&ws, cx, "r1's blocks", |cx| {
        ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len() == 3)
    });
    ws.read_with(cx, |ws, _| {
        let remote = ws.space.remote.as_ref().unwrap();
        let (mut viewers, mut cards) = (0, 0);
        for body in ws.space.blocks.values() {
            match body {
                Body::Terminal(pane) if pane.remote.is_some() => viewers += 1,
                Body::Mirrored(_) => cards += 1,
                _ => panic!("a block that isn't home's"),
            }
        }
        assert_eq!((viewers, cards), (1, 2));
        for (home, uid) in [terminal, note, kanban].iter().zip(&uids) {
            let id = remote.homes[uid];
            let want = rect_at(ws, &first, *home);
            assert_eq!(ws.space.canvas.get(id).unwrap().rect, want);
        }
        assert!(
            ws.space.uids.is_empty(),
            "home's ids are never the mirror's"
        );
    });

    // 2. Moving r1's terminal moves the first workspace's.
    let viewer = ws.read_with(cx, |ws, _| {
        ws.space.remote.as_ref().unwrap().homes[&uids[0]]
    });
    let before = rect_in(&ws, cx, &first, terminal);
    ws.update(cx, |ws, _| {
        ws.placing(&[viewer]);
        ws.space.canvas.get_mut(viewer).unwrap().rect.x += 100.0;
        ws.end_drag();
    });
    let moved = Rect::new(before.x + 100.0, before.y, before.w, before.h);
    pump_home(&ws, cx, "the first workspace's terminal to move", |cx| {
        rect_in(&ws, cx, &first, terminal) == moved
    });

    // 3. The question shows in r1's inbox, and answering it there answers it in the first.
    pump_home(&ws, cx, "the question in r1's inbox", |cx| {
        ws.read_with(cx, |ws, _| ws.inbox().len() == 1)
    });
    let key = ws.read_with(cx, |ws, _| ws.inbox()[0].key);
    ws.read_with(cx, |ws, _| {
        assert!(matches!(&ws.inbox()[0].what, Waiting::Question { .. }));
        assert!(ws.inbox()[0].remote.is_some());
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.answer_question(key, vec![vec!["SQLite".into()]], "", window, cx)
        })
    })
    .unwrap();
    let mut reply = String::new();
    pump_home(&ws, cx, "the agent to be answered", |_| {
        match answers.try_recv() {
            Ok(line) => {
                reply = line;
                true
            }
            Err(_) => false,
        }
    });
    assert!(reply.contains("SQLite"), "{reply}");
    pump_home(&ws, cx, "both waits to go", |cx| {
        ws.read_with(cx, |ws, _| ws.waits.is_empty())
    });

    // 5. Never saved, and never a relay (before the share is revoked: r1 still has its blocks).
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.snapshot(window, cx))
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.save_canvas_now(cx));
    let workspaces = dir.path().join("workspaces");
    assert!(!workspaces.join("r1.json").exists(), "no canvas for r1");
    // The stores hold home's own: one note, and no checklist or board is added by the mirror.
    let stored = |file: &str, list: &str| -> usize {
        let text = std::fs::read_to_string(dir.path().join(file)).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v[list].as_array().map(Vec::len))
            .unwrap_or(0)
    };
    assert_eq!(stored("notes.json", "notes"), 1);
    assert_eq!(stored("checklists.json", "checklists"), 0);
    assert!(
        stored("boards.json", "boards") <= 1,
        "the kanban's own board"
    );
    let (error, overview) = control(handle, &ws, cx, "overview", json!({}));
    assert!(!error, "{overview}");
    assert!(overview.contains("(mirror of "), "{overview}");
    let token = with_token(&ws, cx, Scope::Full, None);
    let seen = state(handle, &ws, cx, &token);
    assert!(seen.workspaces.iter().any(|w| w.id == "r1"));
    assert!(
        seen.blocks.iter().all(|b| b.workspace != "r1"),
        "no r1 block"
    );
    assert_eq!(
        seen.blocks.iter().filter(|b| b.workspace == first).count(),
        3
    );
    ws.read_with(cx, |ws, _| {
        let (space, block) = ws
            .find_block(&uids[0])
            .expect("the first workspace's block");
        assert_eq!((space.as_str(), block), (first.as_str(), terminal));
        assert!(
            ws.api_terminals().iter().all(|t| t.workspace != "r1"),
            "no viewer in the API's terminals"
        );
    });

    // 4. Revoking the share in the first workspace cuts r1 and dims it.
    let revoked = ws.read_with(cx, |ws, _| {
        ws.api
            .store
            .as_ref()
            .unwrap()
            .tokens
            .iter()
            .find(|t| t.share.is_some())
            .unwrap()
            .id
    });
    ws.update(cx, |ws, cx| ws.revoke_token(revoked, cx));
    pump_home(&ws, cx, "r1 to be cut", |cx| {
        ws.read_with(cx, |ws, _| {
            remote_of(ws).status == Status::Cut(Why::Revoked)
        })
    });
    ws.read_with(cx, |ws, _| assert!(remote_of(ws).dim()));
    // Nothing of it was kept as a canvas either.
    assert!(!workspaces.join("r1.json").exists());
}

#[cfg(unix)]
#[gpui_kit::test]
fn what_home_s_shell_prints_shows_on_the_mirror_and_keys_reach_it(cx: &mut TestAppContext) {
    use super::remote::{draw, shown};
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_as(
        cx,
        Some(("sh".into(), Vec::new())),
        None,
        Some(dir.path().to_path_buf()),
    );
    let terminal = first_id(&ws, cx);
    let first = ws.read_with(cx, |ws, _| ws.space.id.clone());
    ws.update(cx, |ws, _| ws.space.pristine = false);
    let (_first, _store) = mirrored(handle, &ws, cx, dir.path());
    // r1 shown, its camera on the terminal at 100%, the viewer focused: it streams.
    let uid = ws.read_with(cx, |ws, _| {
        ws.spaces().find(|s| s.id == first).unwrap().uids[&terminal].clone()
    });
    pump_home(&ws, cx, "r1's viewer", |cx| {
        ws.read_with(cx, |ws, _| {
            ws.space.remote.as_ref().unwrap().homes.contains_key(&uid)
        })
    });
    let viewer = ws.read_with(cx, |ws, _| ws.space.remote.as_ref().unwrap().homes[&uid]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let at = ws.space.canvas.get(viewer).unwrap().rect;
            ws.space.pristine = false;
            ws.space.canvas.camera = Camera::at(Point::new(at.x - 20.0, at.y - 20.0), 1.0);
            ws.space.view = ws.space.canvas.camera;
            ws.focus_block(viewer, window, cx);
        })
    })
    .unwrap();
    // The shell at home prints; the viewer shows it. (The command's own echo says `from-%s`.)
    let home_shell = |cx: &mut TestAppContext, send: &str| {
        ws.read_with(cx, |ws, _| {
            let home = ws.spaces().find(|s| s.id == first).unwrap();
            home.terminal(terminal).unwrap().term.text(send);
        })
    };
    let printed = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let home = ws.spaces().find(|s| s.id == first).unwrap();
            home.terminal(terminal).unwrap().term.shared().lines(40)
        })
    };
    home_shell(cx, "printf 'from-%s\\n' home\r");
    pump_home(&ws, cx, "home's output on the mirror", |cx| {
        draw(handle, cx);
        shown(&ws, cx, viewer).contains("from-home")
    });
    // The mirror types into home's shell with the keyboard: the text path and a key.
    cx.simulate_input(handle, "echo from-mirror");
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    pump_home(&ws, cx, "the keys to reach home's shell", |cx| {
        draw(handle, cx);
        printed(cx)
            .iter()
            .any(|line| line.trim_end() == "from-mirror")
    });
    pump_home(&ws, cx, "the answer on the mirror", |cx| {
        draw(handle, cx);
        shown(&ws, cx, viewer).contains("from-mirror")
    });
}
