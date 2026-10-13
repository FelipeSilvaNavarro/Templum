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

// Final review, I3: however long a line, both sides fit the panel (a long line is cut at its
// side; Unified shows it whole).
#[gpui_kit::test]
fn a_long_line_never_pushes_the_right_side_out_of_the_panel(cx: &mut TestAppContext) {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "short\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "a"]);
    std::fs::write(repo.path().join("a.txt"), format!("{}\n", "x".repeat(300))).unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, _, cx| {
        ws.update(cx, |ws, cx| ws.set_split(id, true, cx))
    })
    .unwrap();
    frame(handle, cx);
    let (half, panel) = ws.read_with(cx, |ws, _| {
        let g = ws.space.git(id).unwrap();
        let row = g
            .panel
            .rows
            .iter()
            .position(|r| {
                matches!(
                    r,
                    Row::Pair {
                        old: Some(_),
                        new: Some(_),
                        ..
                    }
                )
            })
            .expect("the changed pair");
        let shaped = g.panel.shaped.borrow();
        let half = f32::from(shaped[&(row, true)].left - shaped[&(row, false)].left);
        let rect = ws.space.canvas.get(id).unwrap().rect;
        let z = ws.space.canvas.camera.zoom();
        (half, (rect.w - crate::git_block::LISTS_W) * z)
    });
    assert!(
        half <= panel / 2.0,
        "the right side starts at {half}, the panel is {panel} wide"
    );
}
