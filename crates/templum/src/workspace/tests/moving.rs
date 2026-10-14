//! Moving blocks to another workspace: they keep running, arranged as they were.

use super::*;

/// A second workspace, "w2", empty; Main shown again.
fn second(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.switch_to(0, window, cx);
        })
    })
    .unwrap();
}

fn move_to(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    ids: Vec<BlockId>,
    to: Option<&str>,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.move_blocks(ids, to, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn moving_a_terminal_keeps_its_shell_running(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let pid = ws.read_with(cx, |ws, _| {
        ws.space.terminal(first).unwrap().term.processes().shell
    });
    second(handle, &ws, cx);
    move_to(handle, &ws, cx, vec![first], Some("w2"));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.blocks.is_empty(), "gone from Main");
        assert_eq!(ws.space.name, "Main", "Main stays shown");
        let (_, pane) = ws.others[0].terminals().next().expect("in w2");
        assert_eq!(pane.term.processes().shell, pid, "the same shell");
        assert!(!pane.term.exited());
    });
}

#[gpui_kit::test]
fn moving_a_group_keeps_its_arrangement(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let ids = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(Rect::new(0.0, 0.0, 600.0, 400.0), window, cx);
                let a = ws.space.focused.unwrap();
                ws.open(Rect::new(700.0, 50.0, 600.0, 400.0), window, cx);
                let b = ws.space.focused.unwrap();
                vec![a, b]
            })
        })
        .unwrap();
    second(handle, &ws, cx);
    move_to(handle, &ws, cx, ids, Some("w2"));
    ws.read_with(cx, |ws, _| {
        let rects: Vec<Rect> = ws.others[0]
            .canvas
            .blocks()
            .iter()
            .map(|b| b.rect)
            .collect();
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[1].x - rects[0].x, 700.0);
        assert_eq!(rects[1].y - rects[0].y, 50.0);
        assert_eq!(ws.space.blocks.len(), 1, "the startup terminal stays");
    });
}

#[gpui_kit::test]
fn moving_to_a_new_workspace_creates_it_without_showing_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    move_to(handle, &ws, cx, vec![first], None);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.name, "Main");
        assert_eq!(ws.others.len(), 1);
        assert_eq!(ws.others[0].name, "Workspace 2");
        assert_eq!(ws.others[0].terminals().count(), 1);
    });
}

#[gpui_kit::test]
fn a_block_moved_into_a_grid_workspace_joins_its_grid(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.open(Rect::new(0.0, 0.0, 600.0, 400.0), window, cx);
            let tiled = ws.space.focused.unwrap();
            ws.enter_grid(vec![tiled], window, cx);
            ws.switch_to(0, window, cx);
        })
    })
    .unwrap();
    move_to(handle, &ws, cx, vec![first], Some("w2"));
    ws.read_with(cx, |ws, _| {
        let w2 = &ws.others[0];
        let grid = w2.grid.as_ref().expect("still in grid mode");
        assert_eq!(grid.homes.len(), 2, "the moved terminal is a tile");
    });
}

#[gpui_kit::test]
fn a_git_block_moved_while_busy_reads_again_in_its_new_workspace(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    second(handle, &ws, cx);
    // A command in flight: its answer goes to the block it was.
    ws.update(cx, |ws, _| ws.space.git_mut(id).unwrap().busy = true);
    move_to(handle, &ws, cx, vec![id], Some("w2"));
    settle(handle, cx);
    ws.read_with(cx, |ws, _| {
        let w2 = &ws.others[0];
        let (_, git) = w2
            .blocks
            .iter()
            .find_map(|(id, b)| Some((*id, b.git()?)))
            .expect("in w2");
        assert!(!git.busy, "not waiting for an answer that never comes");
        assert!(git.repo().is_some());
    });
}

#[gpui_kit::test]
fn a_stopped_workspace_cant_receive_blocks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    second(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.stop_now("w2", window, cx))
    })
    .unwrap();
    move_to(handle, &ws, cx, vec![first], Some("w2"));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(first).is_some(), "it stays");
        assert!(ws.others[0].blocks.is_empty());
    });
}
