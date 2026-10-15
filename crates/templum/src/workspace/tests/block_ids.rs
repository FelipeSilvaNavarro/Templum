//! Block ids: what agents and wires name blocks by (docs/specs/connections.md, Identity).

use super::*;
use crate::workspace::history::Then;

fn uid(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> String {
    ws.read_with(cx, |ws, _| ws.space.uids[&id].clone())
}

fn all_uids(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<String> {
    ws.read_with(cx, |ws, _| {
        ws.space
            .canvas
            .blocks()
            .iter()
            .map(|b| ws.space.uids[&b.id].clone())
            .collect()
    })
}

#[gpui_kit::test]
fn every_block_gets_its_own_id(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let ids = [kanban, note, terminal].map(|b| uid(&ws, cx, b));
    for id in &ids {
        assert_eq!(id.len(), 5, "{id}");
        assert!(
            id.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
            "{id}"
        );
    }
    assert!(
        ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2],
        "{ids:?}"
    );
}

#[gpui_kit::test]
fn a_blocks_id_comes_back_after_a_restart(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    three_blocks(handle, &ws, cx);
    let before = all_uids(&ws, cx);
    ws.update(cx, |ws, cx| ws.save_canvas_now(cx));
    let saved = std::fs::read(data.path().join("workspaces").join("default.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(saved["items"][0]["uid"], before[0].as_str(), "{saved}");
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    assert_eq!(all_uids(&again, cx), before);
}

#[gpui_kit::test]
fn closing_and_undoing_keeps_a_blocks_id(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, _, _) = three_blocks(handle, &ws, cx);
    let before = uid(&ws, cx, kanban);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.close_blocks(vec![kanban], Then::New, window, cx);
            assert!(
                !ws.space.uids.values().any(|u| *u == before),
                "gone with its block"
            );
            ws.undo_canvas(false, window, cx);
        })
    })
    .unwrap();
    assert!(all_uids(&ws, cx).contains(&before));
}

#[gpui_kit::test]
fn a_block_moved_to_another_workspace_keeps_its_id(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let before = uid(&ws, cx, first);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.switch_to(0, window, cx);
            ws.move_blocks(vec![first], Some("w2"), window, cx);
        })
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.uids.is_empty());
        assert_eq!(ws.others[0].uids.values().collect::<Vec<_>>(), [&before]);
    });
}

#[gpui_kit::test]
fn an_id_already_taken_or_malformed_is_replaced(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    // Two blocks claim one id (a file edited by hand), a third has one that isn't an id.
    let kanban = |x: f32, uid: &str| {
        format!(r#"{{"kind":"kanban","uid":"{uid}","rect":{{"x":{x},"y":0,"w":300,"h":200}}}}"#)
    };
    std::fs::create_dir_all(data.path().join("workspaces")).unwrap();
    std::fs::write(
        data.path().join("workspaces").join("default.json"),
        format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[{},{},{}]}}"#,
            kanban(0.0, "aaaaa"),
            kanban(400.0, "aaaaa"),
            kanban(800.0, "../x")
        ),
    )
    .unwrap();
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let ids = all_uids(&ws, cx);
    assert_eq!(ids[0], "aaaaa");
    assert_ne!(ids[1], "aaaaa");
    assert_ne!(ids[2], "../x");
    assert_eq!(ids[2].len(), 5);
}
