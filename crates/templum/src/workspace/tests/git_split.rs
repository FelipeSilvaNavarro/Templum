//! The diff panel side by side.

use super::*;
use crate::git_block::diff_view::Row;

#[gpui_kit::test]
fn split_shows_both_sides_and_the_block_remembers_it(cx: &mut TestAppContext) {
    let repo = three_hunk_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-split", n), cx);
        window.render_frame(cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(g.split);
        assert!(g.panel.rows.iter().any(|r| matches!(
            r,
            Row::Pair {
                old: Some(_),
                new: Some(_),
                ..
            }
        )));
        assert!(!g.panel.rows.iter().any(|r| matches!(r, Row::Line { .. })));
        assert!(
            g.history_panel.split && g.stash_panel.split,
            "every panel of the block"
        );
    });
    let item = ws.read_with(cx, |ws, cx| {
        let rect = ws.space.canvas.get(id).unwrap().rect;
        ws.space.git(id).unwrap().item(rect, cx)
    });
    assert_eq!(item.diff, "split");
    cx.update_window(handle, |_, window, cx| window.click(("git-unified", n), cx))
        .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(g.panel.rows.iter().all(|r| !matches!(r, Row::Pair { .. })))
    });
}

#[gpui_kit::test]
fn a_selection_in_split_copies_one_side(cx: &mut TestAppContext) {
    let repo = three_hunk_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, _, cx| {
        ws.update(cx, |ws, cx| ws.set_split(id, true, cx))
    })
    .unwrap();
    frame(handle, cx);
    // Line 2 changed: "line 2" on the left, "LINE 2" on the right.
    let row = git_of(&ws, cx, id, |g| {
        g.panel
            .rows
            .iter()
            .position(|r| {
                matches!(r, Row::Pair { old: Some(_), new: Some(_), file: 0, hunk: 0, .. }
            if g.panel.display_of_side(r, true).as_deref() == Some("LINE 2"))
            })
            .expect("the changed pair")
    });
    let (left, right) = git_of(&ws, cx, id, |g| {
        let shaped = g.panel.shaped.borrow();
        (shaped[&(row, false)].x, shaped[&(row, true)].x)
    });
    for (x, want) in [(right, "LINE 2"), (left, "line 2")] {
        cx.update_window(handle, |_, _, cx| {
            ws.update(cx, |ws, cx| {
                ws.diff_press(id, row, x + gpui_kit::px(2.0), 3, cx)
            })
        })
        .unwrap();
        git_of(&ws, cx, id, |g| {
            assert_eq!(g.panel.selected_text().as_deref(), Some(want))
        });
    }
}
