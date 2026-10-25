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

use super::api::{port, state, tool, turn_on, with_data, with_token};
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

    // 1. r1 shows the first workspace's blocks at its rectangles: one viewer, one live note, one live kanban.
    pump_home(&ws, cx, "r1's blocks", |cx| {
        ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len() == 3)
    });
    ws.read_with(cx, |ws, _| {
        let remote = ws.space.remote.as_ref().unwrap();
        let (mut viewers, mut notes, mut kanbans) = (0, 0, 0);
        for body in ws.space.blocks.values() {
            match body {
                Body::Terminal(pane) if pane.remote.is_some() => viewers += 1,
                Body::Note(n) if n.remote => notes += 1,
                Body::Kanban(k) if k.remote => kanbans += 1,
                _ => panic!("a block that isn't home's"),
            }
        }
        assert_eq!((viewers, notes, kanbans), (1, 1, 1));
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

/// A shell at home, mirrored as `r1` and live: r1 shown, its camera on the terminal at 100%, the
/// viewer focused (it streams). The window, the workspace, home's terminal and its workspace's id,
/// the viewer, and what keeps the data folder and the keys.
#[cfg(unix)]
#[allow(clippy::type_complexity)]
fn shell_mirrored(
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    Entity<Workspace>,
    (BlockId, String, BlockId),
    (tempfile::TempDir, Arc<mock::Store>),
) {
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
    let (_first, store) = mirrored(handle, &ws, cx, dir.path());
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
    (handle, ws, (terminal, first, viewer), (dir, store))
}

/// Types `send` into the shell at home (the first workspace's terminal).
#[cfg(unix)]
fn home_shell(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    (terminal, first): (BlockId, &str),
    send: &str,
) {
    ws.read_with(cx, |ws, _| {
        let home = ws.spaces().find(|s| s.id == first).unwrap();
        home.terminal(terminal).unwrap().term.text(send);
    })
}

/// The last `lines` lines home's terminal printed.
#[cfg(unix)]
fn home_printed(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    (terminal, first): (BlockId, &str),
    lines: usize,
) -> Vec<String> {
    ws.read_with(cx, |ws, _| {
        let home = ws.spaces().find(|s| s.id == first).unwrap();
        home.terminal(terminal).unwrap().term.shared().lines(lines)
    })
}

#[cfg(unix)]
#[gpui_kit::test]
fn what_home_s_shell_prints_shows_on_the_mirror_and_keys_reach_it(cx: &mut TestAppContext) {
    use super::remote::{draw, shown};
    let (handle, ws, (terminal, first, viewer), _keep) = shell_mirrored(cx);
    // The shell at home prints; the viewer shows it. (The command's own echo says `from-%s`.)
    let at = (terminal, first.as_str());
    home_shell(&ws, cx, at, "printf 'from-%s\\n' home\r");
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
        home_printed(&ws, cx, at, 40)
            .iter()
            .any(|line| line.trim_end() == "from-mirror")
    });
    pump_home(&ws, cx, "the answer on the mirror", |cx| {
        draw(handle, cx);
        shown(&ws, cx, viewer).contains("from-mirror")
    });
}

/// Past the screen: the lines home printed are only in home's history, which Find reads when it
/// opens and searches again when it lands.
#[cfg(unix)]
#[gpui_kit::test]
fn three_hundred_lines_printed_at_home_are_found_from_the_mirror(cx: &mut TestAppContext) {
    use super::remote::{draw, shown};
    let (handle, ws, (terminal, first, viewer), _keep) = shell_mirrored(cx);
    let at = (terminal, first.as_str());
    home_shell(
        &ws,
        cx,
        at,
        "i=1; while [ $i -le 300 ]; do echo \"row $i.\"; i=$((i+1)); done\r",
    );
    pump_home(&ws, cx, "300 lines on the mirror", |cx| {
        draw(handle, cx);
        home_printed(&ws, cx, at, 40)
            .iter()
            .any(|line| line.trim_end() == "row 300.")
            && shown(&ws, cx, viewer).contains("row 300.")
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_find(window, cx))
    })
    .unwrap();
    cx.simulate_input(handle, "row 1.");
    pump_home(&ws, cx, "home's history to be searched", |cx| {
        draw(handle, cx);
        ws.read_with(cx, |ws, _| {
            ws.space
                .terminal(viewer)
                .unwrap()
                .term
                .shared()
                .selection_text()
                .as_deref()
                == Some("row 1.")
        })
    });
}

/// A program at home that asks for the mouse hears the click made on the mirror.
#[cfg(unix)]
#[gpui_kit::test]
fn a_click_on_the_mirror_reaches_a_program_at_home_that_asked_for_the_mouse(
    cx: &mut TestAppContext,
) {
    use super::remote::draw;
    let (handle, ws, (terminal, first, viewer), _keep) = shell_mirrored(cx);
    let at = (terminal, first.as_str());
    home_shell(
        &ws,
        cx,
        at,
        "stty -icanon -echo min 1; printf '\\033[?1000h\\033[?1006h'; head -c 5 | cat -v; echo; \
         printf '\\033[?1000l\\033[?1006l'; stty sane\r",
    );
    pump_home(&ws, cx, "the mouse mode on the mirror", |cx| {
        draw(handle, cx);
        ws.read_with(cx, |ws, _| {
            ws.space
                .terminal(viewer)
                .unwrap()
                .term
                .shared()
                .modes()
                .mouse
        })
    });
    let body = cx
        .update_window(handle, |_, window, _| {
            window.find(("body", viewer.0 as usize)).bounds()
        })
        .unwrap();
    let click_at = (
        f32::from(body.origin.x) + 20.,
        f32::from(body.origin.y) + 20.,
    );
    cx.update_window(handle, |_, window, cx| click(window, cx, click_at, 1))
        .unwrap();
    // A left press, SGR, as the program read it.
    pump_home(&ws, cx, "the click to reach the program", |cx| {
        draw(handle, cx);
        home_printed(&ws, cx, at, 40)
            .iter()
            .any(|line| line.contains("^[[<0;"))
    });
}

/// A watch set at home through its API (`wait_for` with `wait: false`) names its block and
/// workspace, so the share the mirror holds hears it: it shows on the mirror as a notification.
#[cfg(unix)]
#[gpui_kit::test]
fn a_watch_set_at_home_through_the_api_shows_its_notice_on_the_mirror(cx: &mut TestAppContext) {
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
    let uid = ws.read_with(cx, |ws, _| {
        ws.spaces().find(|s| s.id == first).unwrap().uids[&terminal].clone()
    });
    let token = with_token(&ws, cx, Scope::Control, None);
    let line = tool(
        handle,
        &ws,
        cx,
        &token,
        "wait_for",
        json!({ "block": uid, "pattern": "DEPLOYED", "wait": false, "timeout": 300 }),
    );
    assert!(line["result"]["isError"] != true, "{line}");
    ws.read_with(cx, |ws, _| {
        let home = ws.spaces().find(|s| s.id == first).unwrap();
        home.terminal(terminal)
            .unwrap()
            .term
            .text("printf 'DEP%s\\n' LOYED\r");
    });
    let name = ws.read_with(cx, |ws, _| ws.space.name.clone());
    pump_home(&ws, cx, "the watch's notice on the mirror", |cx| {
        super::bus::after(cx, 250);
        ws.read_with(cx, |ws, _| {
            ws.said.iter().any(|told| {
                told.starts_with(&format!("{name} on "))
                    && told.contains("a line matches \"DEPLOYED\"")
            })
        })
    });
}

/// The first workspace's block at `at`, if it has one.
fn block_at(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    space: &str,
    at: (f32, f32),
) -> Option<BlockId> {
    ws.read_with(cx, |ws, _| {
        let home = ws.spaces().find(|s| s.id == space)?;
        home.canvas
            .blocks()
            .iter()
            .find(|b| (b.rect.x, b.rect.y) == at)
            .map(|b| b.id)
    })
}

#[gpui_kit::test]
fn a_terminal_added_on_the_mirror_is_there_at_home_has_the_keyboard_and_closes_there(
    cx: &mut TestAppContext,
) {
    let (handle, ws, dir) = with_data(cx);
    let folder = tempfile::tempdir().unwrap();
    ws.update(cx, |ws, _| {
        ws.space.folder = Some(folder.path().to_path_buf())
    });
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let _ = (kanban, note);
    let first_uid = ws.read_with(cx, |ws, _| ws.space.uids[&terminal].clone());
    let (first, _store) = mirrored(handle, &ws, cx, dir.path());
    pump_home(&ws, cx, "r1's blocks", |cx| {
        ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len() == 3)
    });
    // 1. A move made on r1 first, which Ctrl+Z must not reach past the add.
    let viewer = ws.read_with(cx, |ws, _| remote_of(ws).homes[&first_uid]);
    let before = rect_in(&ws, cx, &first, terminal);
    ws.update(cx, |ws, _| {
        ws.placing(&[viewer]);
        ws.space.canvas.get_mut(viewer).unwrap().rect.x += 100.0;
        ws.end_drag();
    });
    let moved = Rect::new(before.x + 100.0, before.y, before.w, before.h);
    pump_home(&ws, cx, "home's terminal to move", |cx| {
        rect_in(&ws, cx, &first, terminal) == moved
    });
    // 2. A terminal added on r1 is added at home, where it was put, and takes the keyboard.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_shell(None, Some(Point::new(1234.0, 567.0)), window, cx)
        })
    })
    .unwrap();
    let mut added = None;
    pump_home(&ws, cx, "the new terminal at home", |cx| {
        added = block_at(&ws, cx, &first, (1234.0, 567.0));
        added.is_some()
    });
    let added = added.unwrap();
    let uid = ws.read_with(cx, |ws, _| {
        let home = ws.spaces().find(|s| s.id == first).unwrap();
        assert!(home.terminal(added).is_some(), "a terminal");
        home.uids[&added].clone()
    });
    pump_home(&ws, cx, "the new block to get the keyboard", |cx| {
        ws.read_with(cx, |ws, _| {
            remote_of(ws)
                .homes
                .get(&uid)
                .is_some_and(|id| ws.space.focused == Some(*id))
        })
    });
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.uids.is_empty(), "never a relay")
    });
    // 3. Ctrl+Z stops at the add, and leaves both the move and the terminal.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let computer = crate::mirror::computer_of(remote_of(ws));
        assert_eq!(
            ws.said.last().cloned(),
            Some(format!(
                "On a mirror, Ctrl+Z undoes moves only: undo the rest on {computer}."
            ))
        );
    });
    assert_eq!(rect_in(&ws, cx, &first, terminal), moved);
    assert!(block_at(&ws, cx, &first, (1234.0, 567.0)).is_some());
    // 4. Closing it on r1 closes it at home.
    let new_viewer = ws.read_with(cx, |ws, _| remote_of(ws).homes[&uid]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close(new_viewer, window, cx))
    })
    .unwrap();
    pump_home(&ws, cx, "the terminal to close at home", |cx| {
        block_at(&ws, cx, &first, (1234.0, 567.0)).is_none()
    });
}

#[gpui_kit::test]
fn two_blocks_wired_and_named_on_the_mirror_are_wired_and_named_at_home(cx: &mut TestAppContext) {
    use templum_canvas::Wire;
    let (handle, ws, dir) = with_data(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let _ = note;
    let uids = ws.read_with(cx, |ws, _| {
        [terminal, kanban].map(|id| ws.space.uids[&id].clone())
    });
    let (first, _store) = mirrored(handle, &ws, cx, dir.path());
    pump_home(&ws, cx, "r1's blocks", |cx| {
        ws.read_with(cx, |ws, _| ws.space.canvas.blocks().len() == 3)
    });
    let [viewer, card] = ws.read_with(cx, |ws, _| uids.clone().map(|u| remote_of(ws).homes[&u]));
    ws.update(cx, |ws, cx| assert!(ws.connect(viewer, card, cx)));
    let at_home = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let home = ws.spaces().find(|s| s.id == first).unwrap();
            (
                home.wired(terminal, kanban),
                home.label(Wire::new(terminal, kanban).unwrap())
                    .map(str::to_string),
            )
        })
    };
    pump_home(&ws, cx, "the wire at home", |cx| at_home(cx).0);
    let wire = Wire::new(viewer, card).unwrap();
    ws.update(cx, |ws, cx| assert!(ws.rename_wire(wire, "deploys", cx)));
    pump_home(&ws, cx, "the name at home", |cx| {
        at_home(cx).1.as_deref() == Some("deploys")
    });
    pump_home(&ws, cx, "the name on the mirror", |cx| {
        ws.read_with(cx, |ws, _| ws.space.label(wire) == Some("deploys"))
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_task_handed_on_the_mirror_is_typed_at_home_and_its_end_shows_on_the_mirror(
    cx: &mut TestAppContext,
) {
    use super::bus::{after, event, me, screen_with};
    use super::delegate::{agents_running_in, entered};
    let dir = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let (handle, ws, _mine, theirs) = agents_running_in(cx, None, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, _| {
        ws.space.folder = Some(folder.path().to_path_buf())
    });
    let (worker, _) = me(&ws, cx, theirs);
    let their_uid = ws.read_with(cx, |ws, _| ws.space.uids[&theirs].clone());
    let (first, _store) = mirrored(handle, &ws, cx, dir.path());
    pump_home(&ws, cx, "r1's blocks", |cx| {
        ws.read_with(cx, |ws, _| remote_of(ws).homes.contains_key(&their_uid))
    });
    let viewer = ws.read_with(cx, |ws, _| remote_of(ws).homes[&their_uid]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.remote_hand_task(viewer, "Green.".into(), window, cx)
        })
    })
    .unwrap();
    // Home is shown to look at its terminal, as the API's tests do.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.switch_to(0, window, cx))
    })
    .unwrap();
    assert_eq!(ws.read_with(cx, |ws, _| ws.space.id.clone()), first);
    pump_home(&ws, cx, "the task to reach home", |cx| {
        after(cx, 50);
        ws.read_with(cx, |ws, _| {
            ws.space
                .terminal(theirs)
                .is_some_and(|t| t.handed.is_some())
        })
    });
    screen_with(&ws, cx, theirs, "[from API · Itself, task t");
    entered(&ws, cx, theirs);
    event(handle, &ws, cx, &worker, json!({ "event": "submitted" }));
    // Home answers the call once the agent took the task: a few rounds, and nothing is said.
    let mut rounds = 0;
    pump_home(&ws, cx, "home to answer the call", |cx| {
        after(cx, 250);
        rounds += 1;
        rounds > 4
    });
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "stopped", "message": "Green." }),
    );
    let title = ws.read_with(cx, |ws, _| {
        let r1 = ws.spaces().find(|s| s.id == "r1").unwrap();
        format!(
            "{} on {}",
            r1.name,
            crate::mirror::computer_of(r1.remote.as_deref().unwrap())
        )
    });
    pump_home(&ws, cx, "the end of the task on the mirror", |cx| {
        after(cx, 250);
        ws.read_with(cx, |ws, _| {
            // The notice shows its first line: the task's number and who ended it.
            ws.said
                .iter()
                .any(|s| s.starts_with(&title) && s.contains("task t1") && s.contains("done in"))
        })
    });
    ws.read_with(cx, |ws, _| {
        assert!(
            ws.said
                .iter()
                .all(|s| !s.contains("before it took the task")),
            "a task taken says nothing: {:?}",
            ws.said
        )
    });
}

// Batch 2d: boards, live and acted on.

/// A board at home (the first workspace's kanban shows it) with To do `[A, B]` and Doing `[C, D]`,
/// then mirrored as `r1`: (the board, its columns To do, Doing, Done, the cards A, B, C, D).
type Boarded = (u64, [u64; 3], [u64; 4]);

fn boarded(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    dir: &std::path::Path,
) -> Boarded {
    let (_, board, a) = board_with_a_card(handle, ws, cx);
    let columns: Vec<u64> = ws.read_with(cx, |ws, _| {
        ws.boards
            .board(board)
            .unwrap()
            .columns
            .iter()
            .map(|c| c.id)
            .collect()
    });
    let cards = ws.update(cx, |ws, _| {
        let mut add = |title: &str, column: u64| {
            let id = ws.edit_boards(|b| b.add_card(board, column)).unwrap();
            ws.edit_boards(|b| b.edit_card(board, id, |c| c.title = title.into()));
            id
        };
        [
            a,
            add("B", columns[0]),
            add("C", columns[1]),
            add("D", columns[1]),
        ]
    });
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| b.edit_card(board, a, |c| c.title = "A".into()))
    });
    mirrored(handle, ws, cx, dir);
    pump_home(ws, cx, "r1 to copy the board", |cx| {
        ws.read_with(cx, |ws, _| remote_of(ws).boards.board(board).is_some())
    });
    (board, [columns[0], columns[1], columns[2]], cards)
}

/// A card's ids in each column of home's board.
fn home_columns(ws: &Entity<Workspace>, cx: &mut TestAppContext, board: u64) -> Vec<Vec<u64>> {
    ws.read_with(cx, |ws, _| {
        ws.boards
            .board(board)
            .unwrap()
            .columns
            .iter()
            .map(|c| c.cards.iter().map(|c| c.id).collect())
            .collect()
    })
}

fn act_on_r1(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    act: crate::workspace::BoardAct,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.act_on_boards("r1", act, window, cx))
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_card_dragged_on_the_mirror_lands_at_home_before_that_card(cx: &mut TestAppContext) {
    let (handle, ws, dir) = with_data(cx);
    let (board, [_, doing, _], [a, b, c, d]) = boarded(handle, &ws, cx, dir.path());
    act_on_r1(
        handle,
        &ws,
        cx,
        crate::workspace::BoardAct::MoveCard {
            board,
            card: a,
            column: doing,
            to: Some(d),
        },
    );
    pump_home(&ws, cx, "A before D at home", |cx| {
        home_columns(&ws, cx, board)[1] == [c, a, d]
    });
    assert_eq!(home_columns(&ws, cx, board)[0], [b]);
    pump_home(&ws, cx, "r1's copy to agree", |cx| {
        ws.read_with(cx, |ws, _| remote_of(ws).cards_moved.get(&a).is_none())
    });
    ws.read_with(cx, |ws, _| {
        let copy: Vec<u64> = remote_of(ws).boards.board(board).unwrap().columns[1]
            .cards
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(copy, [c, a, d]);
    });
}

#[gpui_kit::test]
fn a_card_made_on_the_mirror_has_its_labels_and_its_comment_at_home(cx: &mut TestAppContext) {
    use crate::board::{Card, Comment, Draft, Label};
    let (handle, ws, dir) = with_data(cx);
    let (board, [todo, _, _], _) = boarded(handle, &ws, cx, dir.path());
    let mut work = Card::new(0);
    work.title = "Login".into();
    work.labels = vec![Label::Red, Label::Blue];
    work.comments = vec![Comment {
        id: u64::MAX,
        by: "you".into(),
        text: "First".into(),
        at: 0,
        edited: false,
    }];
    let draft = Draft {
        at: todo,
        new: true,
        base: Card::new(0),
        work,
    };
    act_on_r1(
        handle,
        &ws,
        cx,
        crate::workspace::BoardAct::Finish { board, draft },
    );
    pump_home(&ws, cx, "the card at home", |cx| {
        ws.read_with(cx, |ws, _| {
            let last = ws.boards.board(board).unwrap().columns[0]
                .cards
                .last()
                .cloned();
            last.is_some_and(|c| c.title == "Login" && !c.comments.is_empty())
        })
    });
    ws.read_with(cx, |ws, _| {
        let card = ws.boards.board(board).unwrap().columns[0]
            .cards
            .last()
            .unwrap();
        assert_eq!(card.labels, [Label::Red, Label::Blue]);
        let comments: Vec<(&str, &str)> = card
            .comments
            .iter()
            .map(|c| (c.by.as_str(), c.text.as_str()))
            .collect();
        assert_eq!(comments, [("you", "First")]);
    });
}

#[gpui_kit::test]
fn a_column_added_and_renamed_on_the_mirror_is_at_home(cx: &mut TestAppContext) {
    use crate::workspace::BoardAct;
    let (handle, ws, dir) = with_data(cx);
    let (board, _, _) = boarded(handle, &ws, cx, dir.path());
    let last_title = |ws: &Entity<Workspace>, cx: &mut TestAppContext| -> (u64, String) {
        ws.read_with(cx, |ws, _| {
            let c = ws
                .boards
                .board(board)
                .unwrap()
                .columns
                .last()
                .unwrap()
                .clone();
            (c.id, c.title)
        })
    };
    act_on_r1(handle, &ws, cx, BoardAct::AddColumn { board });
    pump_home(&ws, cx, "the new column at home and on the mirror", |cx| {
        last_title(&ws, cx).1 == "New column"
            && ws.read_with(cx, |ws, _| {
                remote_of(ws)
                    .boards
                    .board(board)
                    .unwrap()
                    .columns
                    .last()
                    .is_some_and(|c| c.title == "New column")
            })
    });
    let (column, _) = last_title(&ws, cx);
    act_on_r1(
        handle,
        &ws,
        cx,
        BoardAct::RenameColumn {
            board,
            column,
            title: "Review".into(),
        },
    );
    pump_home(&ws, cx, "the rename at home", |cx| {
        last_title(&ws, cx).1 == "Review"
    });
}

// Batch 2e: checklists, notes and ink, live.

#[gpui_kit::test]
fn an_item_checked_added_edited_and_deleted_on_the_mirror_is_at_home(cx: &mut TestAppContext) {
    use crate::workspace::ChecklistAct;
    let (handle, ws, dir) = with_data(cx);
    let (block, list) = checklist_block(handle, &ws, cx, "Release", &["Tag it", "Ship"]);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&block].clone());
    let (tag, ship) = ws.read_with(cx, |ws, _| {
        let items = &ws.checklists.get(list).unwrap().items;
        (items[0].id, items[1].id)
    });
    mirrored(handle, &ws, cx, dir.path());
    pump_home(&ws, cx, "r1 to copy the list", |cx| {
        ws.read_with(cx, |ws, _| remote_of(ws).checklists.get(list).is_some())
    });
    let c1 = ws.read_with(cx, |ws, _| remote_of(ws).homes[&uid]);
    let act = |cx: &mut TestAppContext, act: ChecklistAct| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.act_on_checklist("r1", c1, act, window, cx))
        })
        .unwrap();
    };
    let home = |ws: &Entity<Workspace>, cx: &mut TestAppContext| -> Vec<(String, bool)> {
        ws.read_with(cx, |ws, _| {
            let items = &ws.checklists.get(list).unwrap().items;
            items.iter().map(|i| (i.text.clone(), i.done)).collect()
        })
    };
    act(cx, ChecklistAct::Toggle { item: tag });
    pump_home(&ws, cx, "Tag it checked at home", |cx| home(&ws, cx)[0].1);
    act(
        cx,
        ChecklistAct::Add {
            text: "Notes".into(),
        },
    );
    pump_home(&ws, cx, "Notes added at home", |cx| {
        home(&ws, cx).last().is_some_and(|i| i.0 == "Notes")
    });
    act(
        cx,
        ChecklistAct::Edit {
            item: ship,
            text: "Ship it".into(),
        },
    );
    pump_home(&ws, cx, "Ship edited at home", |cx| {
        home(&ws, cx)[1].0 == "Ship it"
    });
    act(cx, ChecklistAct::Delete { item: tag });
    pump_home(&ws, cx, "Tag it gone at home", |cx| {
        home(&ws, cx).len() == 2
    });
    assert_eq!(
        home(&ws, cx),
        [("Ship it".to_string(), false), ("Notes".to_string(), false)]
    );
    // r1's copy agrees, and no hold is left.
    pump_home(&ws, cx, "r1's copy to agree", |cx| {
        ws.read_with(cx, |ws, _| {
            let remote = remote_of(ws);
            let copy: Vec<(String, bool)> = remote
                .checklists
                .get(list)
                .unwrap()
                .items
                .iter()
                .map(|i| (i.text.clone(), i.done))
                .collect();
            copy == [("Ship it".to_string(), false), ("Notes".to_string(), false)]
                && remote.items_checked.keys().is_empty()
                && remote.items_edited.keys().is_empty()
                && remote.items_deleted.keys().is_empty()
        })
    });
}

#[gpui_kit::test]
fn a_note_typed_at_home_shows_on_the_mirror(cx: &mut TestAppContext) {
    let (handle, ws, dir) = with_data(cx);
    let (_, note, _) = three_blocks(handle, &ws, cx);
    let (first, uid) = ws.read_with(cx, |ws, _| {
        (ws.space.id.clone(), ws.space.uids[&note].clone())
    });
    mirrored(handle, &ws, cx, dir.path());
    // Home's note editor takes the text, and its change writes the notes, as typing does.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.note_replace(&first, note, "hello there", None, window, cx)
        })
    })
    .unwrap()
    .expect("replaced");
    pump_home(&ws, cx, "r1's note to read it", |cx| {
        ws.read_with(cx, |ws, cx| {
            let remote = remote_of(ws);
            let shown = remote
                .homes
                .get(&uid)
                .and_then(|id| ws.space.blocks.get(id))
                .and_then(Body::note_view)
                .and_then(|n| n.tab())
                .map(|t| t.editor.read(cx).value().to_string());
            shown.as_deref() == Some("hello there")
                && remote.notes.notes.iter().any(|n| n.text == "hello there")
        })
    });
}

#[gpui_kit::test]
fn a_stroke_drawn_at_home_shows_on_the_mirror(cx: &mut TestAppContext) {
    use templum_canvas::{Color, Drawing};
    let (handle, ws, dir) = with_data(cx);
    let (first, _) = mirrored(handle, &ws, cx, dir.path());
    let stroke = Drawing::Stroke {
        points: vec![Point::new(0.0, 0.0), Point::new(80.0, 40.0)],
        width: 4.0,
        color: Color::Blue,
        highlight: false,
    };
    ws.update(cx, |ws, _| {
        ws.in_space(&first, |ws| ws.space.ink.push(stroke.clone()));
    });
    pump_home(&ws, cx, "r1 to paint the stroke", |cx| {
        ws.read_with(cx, |ws, _| ws.space.ink == [stroke.clone()])
    });
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.id, "r1");
        assert_eq!(ws.ink_at(Point::new(40.0, 20.0)), None, "it is not picked");
    });
}
