//! The Git block's diff panel: every working change in one scroll.

use super::*;
use crate::git_block::diff_view::Row;

/// a.txt committed as "one\n"; "zero" staged, "two" not; n.txt untracked.
fn mixed_repo() -> tempfile::TempDir {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    std::fs::write(repo.path().join("a.txt"), "zero\none\ntwo\n").unwrap();
    std::fs::write(repo.path().join("n.txt"), "new\n").unwrap();
    repo
}

/// The index of the first line row whose text is `text`.
fn line_row(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId, text: &str) -> usize {
    git_of(ws, cx, id, |g| {
        g.panel
            .rows
            .iter()
            .position(|r| {
                matches!(r, Row::Line { file, hunk, line }
                    if g.panel.files[*file].diff.hunks[*hunk].lines[*line].text == text)
            })
            .unwrap_or_else(|| panic!("no line {text:?}"))
    })
}

fn panel_button(id: BlockId, what: &str, ix: usize) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(format!("git-panel-{what}-{}-{ix}", id.0).into())
}

#[gpui_kit::test]
fn the_panel_lists_every_change_in_two_sections(cx: &mut TestAppContext) {
    let repo = mixed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| {
        let p = &g.panel;
        let shown: Vec<(bool, &str)> = p.files.iter().map(|s| (s.staged, s.diff.path())).collect();
        assert_eq!(shown, [(false, "a.txt"), (false, "n.txt"), (true, "a.txt")]);
        assert_eq!(p.title(), "Working changes · 2 files · +3 −0");
        assert_eq!(
            p.rows[0],
            Row::Section {
                staged: false,
                files: 2
            }
        );
        assert!(p.rows.contains(&Row::Section {
            staged: true,
            files: 1
        }));
        assert_eq!(
            p.row_of(true, "a.txt"),
            Some(p.rows.len() - 5),
            "heading, file, hunk, two lines, the fold after (the working file has one more line)"
        );
        assert_eq!(p.first_line(false, "a.txt"), Some(1));
        assert_eq!(p.first_line(false, "n.txt"), Some(1));
        assert!(p.reads >= 1);
    });
    let n = id.0 as usize;
    let two = line_row(&ws, cx, id, "two");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("git-diff", n)).is_some());
        assert!(
            window.try_find(("git-file", 1usize)).is_some(),
            "the first file row"
        );
        assert!(window.try_find(("git-line", two)).is_some());
        assert!(window.try_find(("git-panel-title", n)).is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_file_rows_buttons_stage_unstage_and_open(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    // Heading, file: the file row is row 1.
    click_row_button(
        handle,
        cx,
        ("git-file", 1usize).into(),
        panel_button(id, "stage", 1),
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.staged().len(), 1);
        assert!(
            g.panel.files.iter().all(|s| s.staged),
            "shown under Staged now"
        );
    });
    click_row_button(
        handle,
        cx,
        ("git-file", 1usize).into(),
        panel_button(id, "unstage", 1),
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.unstaged().len(), 1);
        assert!(g.panel.files.iter().all(|s| !s.staged));
    });
    click_row_button(
        handle,
        cx,
        ("git-file", 1usize).into(),
        panel_button(id, "open", 1),
    );
    ws.read_with(cx, |ws, cx| {
        let (eid, e) = ws
            .space
            .blocks
            .iter()
            .find_map(|(id, b)| Some((*id, b.editor()?)))
            .expect("an editor opened");
        assert_eq!(e.active_tab().unwrap().name(), "a.txt");
        assert_eq!(
            e.active_tab()
                .unwrap()
                .editor
                .read(cx)
                .cursor_position()
                .line,
            0
        );
        let (git, editor) = (
            ws.space.canvas.get(id).unwrap().rect,
            ws.space.canvas.get(eid).unwrap().rect,
        );
        assert_eq!(editor.x, git.x + git.w + templum_canvas::BESIDE);
    });
}

#[gpui_kit::test]
fn clicking_a_file_in_the_left_column_scrolls_the_panel_to_it(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    for i in 0..30 {
        std::fs::write(repo.path().join(format!("f{i:02}.txt")), "l\n".repeat(10)).unwrap();
    }
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    // Row 8 of the left list is drawn (the list is virtual: ~14 rows fit); its panel rows start
    // at 1 + 8 × 12 = 97, with enough rows after it for the list to scroll there.
    cx.update_window(handle, |_, window, cx| {
        window.click(row_id(id, "unstaged", 8), cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        let row = g.panel.row_of(false, "f08.txt").expect("a file row");
        assert_eq!(g.selected.as_ref().map(|s| s.1.as_str()), Some("f08.txt"));
        // The list's offset, in rows (uniform_list keeps no child bounds for
        // logical_scroll_top_index to read after a frame).
        let base = g.panel.scroll.0.borrow().base_handle.clone();
        let row_h = crate::git_block::diff_view::row_height(1.0);
        let top = (-f32::from(base.offset().y) / row_h).round() as usize;
        assert_eq!(top, row);
    });
}

#[gpui_kit::test]
fn a_read_that_changes_nothing_keeps_the_rows(cx: &mut TestAppContext) {
    let repo = mixed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let (rows, reads) = git_of(&ws, cx, id, |g| (g.panel.rows.clone(), g.panel.reads));
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.panel.reads > reads, "the poll read the diff again");
        assert!(
            std::rc::Rc::ptr_eq(&g.panel.rows, &rows),
            "and rebuilt nothing"
        );
    });
    std::fs::write(repo.path().join("a.txt"), "zero\none\ntwo\nthree\n").unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(
            !std::rc::Rc::ptr_eq(&g.panel.rows, &rows),
            "a change rebuilds them"
        );
    });
}

#[gpui_kit::test]
fn a_clean_tree_and_a_changed_folder_empty_the_panel(cx: &mut TestAppContext) {
    let repo = mixed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| assert!(!g.panel.is_empty()));
    git_run(repo.path(), &["add", "-A"]);
    git_run(repo.path(), &["commit", "-q", "-m", "all"]);
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.panel.is_empty());
        assert!(g.panel.rows.is_empty());
    });
    std::fs::write(repo.path().join("a.txt"), "again\n").unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.panel.files.len(), 1));
    let other = committed_repo();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.set_git_folder(id, other.path(), window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.panel.is_empty(), "the other repository is clean");
        assert_eq!(g.panel.reads, 1, "a fresh panel");
    });
}

#[gpui_kit::test]
fn a_file_in_the_diff_but_not_in_the_status_still_draws(cx: &mut TestAppContext) {
    let repo = mixed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    // The agent wrote between the two reads: the status says nothing about these files.
    ws.update(cx, |ws, _| {
        ws.space.git_mut(id).unwrap().status.entries.clear()
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("git-file", 1usize)).is_some());
    })
    .unwrap();
}
