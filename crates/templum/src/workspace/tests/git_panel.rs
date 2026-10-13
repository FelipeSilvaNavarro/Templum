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

#[gpui_kit::test]
fn the_colors_are_kept_across_a_read_that_changes_nothing_and_dropped_on_a_change(
    cx: &mut TestAppContext,
) {
    let repo = git_repo();
    std::fs::write(repo.path().join("x.rs"), "fn one() {}\n").unwrap();
    git_run(repo.path(), &["add", "x.rs"]);
    git_run(repo.path(), &["commit", "-q", "-m", "x"]);
    std::fs::write(repo.path().join("x.rs"), "fn one() {}\nfn two() {}\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let shown = git_of(&ws, cx, id, |g| g.panel.files[0].clone());
    let theme = gpui_kit::component::highlighter::HighlightTheme::default_dark();
    let first = git_of(&ws, cx, id, |g| {
        g.panel.colors.borrow_mut().of(0, 0, &shown, &theme)
    });
    assert!(
        first.lines[1].iter().any(|(r, _)| *r == (0..2)),
        "fn: {:?}",
        first.lines[1]
    );
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    let again = git_of(&ws, cx, id, |g| {
        g.panel.colors.borrow_mut().of(0, 0, &shown, &theme)
    });
    assert!(std::rc::Rc::ptr_eq(&first, &again), "kept");
    std::fs::write(repo.path().join("x.rs"), "fn one() {}\nfn three() {}\n").unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    let shown = git_of(&ws, cx, id, |g| g.panel.files[0].clone());
    let after = git_of(&ws, cx, id, |g| {
        g.panel.colors.borrow_mut().of(0, 0, &shown, &theme)
    });
    assert!(!std::rc::Rc::ptr_eq(&first, &after), "built again");
}

/// A window x inside the text of a line row: just past the number columns.
fn text_x(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    row: gpui_kit::Bounds<gpui_kit::Pixels>,
    digits: usize,
) -> f32 {
    let cw = ws.read_with(cx, |ws, _| {
        ws.cell.size.w * crate::git_block::FONT / ws.cell.font_size
    });
    f32::from(row.origin.x) + cw * (2 * digits + 5) as f32 + 1.0
}

#[gpui_kit::test]
fn dragging_selects_lines_and_copy_puts_their_source_text_on_the_clipboard(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{point, px};
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n\ttwo\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let (zero, two) = (
        line_row(&ws, cx, id, "zero"),
        line_row(&ws, cx, id, "\ttwo"),
    );
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let (a, b) = (
            window.find(("git-line", zero)).bounds(),
            window.find(("git-line", two)).bounds(),
        );
        // From the left edge of the first row (before the text: byte 0) to the right edge of
        // the last (past it: the end).
        window.drag(
            point(a.origin.x + px(1.0), a.center().y),
            point(b.origin.x + b.size.width - px(1.0), b.center().y),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert_eq!(
            g.panel.selected_text().as_deref(),
            Some("zero\none\n\ttwo"),
            "tabs come back"
        );
        assert!(!g.panel.dragging, "released");
    });
    // The block has the focus (the press focused it): the copy shortcut copies the selection.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        ws.update(cx, |ws, cx| ws.copy_selection(cx));
    })
    .unwrap();
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("zero\none\n\ttwo")
    );
}

#[gpui_kit::test]
fn a_double_click_selects_a_word_and_a_triple_click_the_line(cx: &mut TestAppContext) {
    use gpui_kit::{
        Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput, point, px,
    };
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "one\nfoo_bar baz\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let row = line_row(&ws, cx, id, "foo_bar baz");
    let digits = git_of(&ws, cx, id, |g| g.panel.files[0].digits());
    let bounds = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.find(("git-line", row)).bounds()
        })
        .unwrap();
    let x = text_x(&ws, cx, bounds, digits);
    let press = |cx: &mut TestAppContext, count: usize| {
        cx.update_window(handle, |_, window, cx| {
            let position = point(px(x), bounds.center().y);
            let modifiers = Modifiers::default();
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers,
                    click_count: count,
                    first_mouse: false,
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers,
                    click_count: count,
                }),
                cx,
            );
            window.render_frame(cx);
        })
        .unwrap();
    };
    press(cx, 2);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.panel.selected_text().as_deref(), Some("foo_bar"))
    });
    press(cx, 3);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.panel.selected_text().as_deref(), Some("foo_bar baz"))
    });
    // One click alone selects nothing.
    press(cx, 1);
    git_of(&ws, cx, id, |g| assert!(g.panel.selection.is_none()));
}

#[gpui_kit::test]
fn the_lines_menu_copies_the_line_the_hunk_and_opens_the_editor_there(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "one\ntwo\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let row = line_row(&ws, cx, id, "two");
    // Copy (disabled), Copy line, Copy hunk, Send to ▸ (the startup terminal), —, Open in editor
    // at this line. A separator counts as an index.
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("git-line", row), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(1usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("two")
    );
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("git-line", row), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(2usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some(" one\n+two")
    );
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("git-line", row), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(5usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    ws.read_with(cx, |ws, cx| {
        let e = ws
            .space
            .blocks
            .values()
            .find_map(|b| b.editor())
            .expect("an editor opened");
        assert_eq!(e.active_tab().unwrap().name(), "a.txt");
        assert_eq!(
            e.active_tab()
                .unwrap()
                .editor
                .read(cx)
                .cursor_position()
                .line,
            1,
            "line 2"
        );
    });
}

/// Send to ▸ types the selection into the chosen terminal (as the note's does, tests/notes.rs).
#[cfg(unix)]
#[gpui_kit::test]
fn send_to_types_the_selection_into_a_terminal(cx: &mut TestAppContext) {
    use gpui_kit::{point, px};
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "one\ntwo\n").unwrap();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let row = line_row(&ws, cx, id, "two");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let b = window.find(("git-line", row)).bounds();
        window.drag(
            point(b.origin.x + px(1.0), b.center().y),
            point(b.origin.x + b.size.width - px(1.0), b.center().y),
            cx,
        );
        window.render_frame(cx);
        window.right_click(("git-line", row), cx);
        window.render_frame(cx);
        // Copy, Copy line, Copy hunk, Send to ▸, —, Open in editor at this line.
        window.within("popup-menu").click(3usize, cx);
        window.render_frame(cx);
        window
            .within("submenu")
            .within("popup-menu")
            .click(0usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, Some(terminal)));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(terminal).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("two") {
            assert!(!shown.contains("one"), "only the selection: {shown:?}");
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A committed file of 20 lines, with line 10 changed in the working tree.
fn twenty_lines_repo() -> tempfile::TempDir {
    let repo = git_repo();
    let lines: Vec<String> = (1..=20).map(|i| format!("line {i}\n")).collect();
    std::fs::write(repo.path().join("a.txt"), lines.concat()).unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "twenty"]);
    let mut changed = lines;
    changed[9] = "LINE 10\n".to_string();
    std::fs::write(repo.path().join("a.txt"), changed.concat()).unwrap();
    repo
}

#[gpui_kit::test]
fn a_fold_names_the_unchanged_lines_and_a_click_shows_the_whole_file(cx: &mut TestAppContext) {
    let repo = twenty_lines_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let folds: Vec<(usize, u32)> = git_of(&ws, cx, id, |g| {
        g.panel
            .rows
            .iter()
            .enumerate()
            .filter_map(|(ix, r)| match r {
                Row::Fold { lines, .. } => Some((ix, *lines)),
                _ => None,
            })
            .collect()
    });
    assert_eq!(folds.len(), 2, "{folds:?}");
    assert_eq!(folds[0].1, 6, "lines 1 to 6 before the hunk's context");
    assert_eq!(folds[1].1, 7, "lines 14 to 20 after it");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(("git-fold", folds[0].0), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.panel.whole, [(false, "a.txt".to_string())]);
        assert!(
            !g.panel.rows.iter().any(|r| matches!(r, Row::Fold { .. })),
            "shown whole"
        );
        let lines = g
            .panel
            .rows
            .iter()
            .filter(|r| matches!(r, Row::Line { .. }))
            .count();
        assert_eq!(lines, 21, "20 lines, the changed one twice");
    });
}

#[gpui_kit::test]
fn a_file_row_folds_and_expand_and_collapse_all_fold_every_file(cx: &mut TestAppContext) {
    let repo = mixed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let total = git_of(&ws, cx, id, |g| g.panel.rows.len());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(("git-file", 1usize), cx);
        window.render_frame(cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(g.panel.folded.contains(&(false, "a.txt".to_string())));
        assert!(g.panel.rows.len() < total);
        assert!(
            matches!(g.panel.rows[2], Row::File { .. }),
            "n.txt's row follows a.txt's at once"
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-collapse-all", n), cx);
        window.render_frame(cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.panel.folded.len(), 3);
        assert_eq!(g.panel.rows.len(), 5, "two headings, three file rows");
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-expand-all", n), cx);
        window.render_frame(cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(g.panel.folded.is_empty());
        assert_eq!(g.panel.rows.len(), total);
    });
}

#[gpui_kit::test]
fn ignore_whitespace_hides_a_change_of_spaces_only(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "one  \n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| assert_eq!(g.panel.files.len(), 1));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(("git-ignore-ws", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.panel.ignore_ws);
        assert!(g.panel.is_empty(), "nothing but spaces changed");
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-ignore-ws", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.panel.files.len(), 1));
}

#[gpui_kit::test]
fn large_generated_binary_and_huge_files_show_a_note(cx: &mut TestAppContext) {
    use crate::git_block::diff_view::Note;
    let repo = git_repo();
    std::fs::write(repo.path().join("Cargo.lock"), "a\n").unwrap();
    git_run(repo.path(), &["add", "Cargo.lock"]);
    git_run(repo.path(), &["commit", "-q", "-m", "lock"]);
    std::fs::write(repo.path().join("Cargo.lock"), "b\n").unwrap();
    std::fs::write(repo.path().join("big.txt"), "l\n".repeat(2101)).unwrap();
    std::fs::write(repo.path().join("b.bin"), b"\0\x01").unwrap();
    std::fs::write(repo.path().join("huge.csv"), "a,b\n".repeat(1_310_720)).unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let notes: Vec<(usize, String, Note)> = git_of(&ws, cx, id, |g| {
        g.panel
            .rows
            .iter()
            .enumerate()
            .filter_map(|(ix, r)| match r {
                Row::Note { file, note } => {
                    Some((ix, g.panel.files[*file].diff.path().to_string(), *note))
                }
                _ => None,
            })
            .collect()
    });
    let kinds: Vec<(&str, Note)> = notes.iter().map(|(_, p, n)| (p.as_str(), *n)).collect();
    assert_eq!(
        kinds,
        [
            ("Cargo.lock", Note::Large),
            ("b.bin", Note::Binary),
            ("big.txt", Note::Large),
            ("huge.csv", Note::TooLarge),
        ]
    );
    // Show diff on big.txt: its 2,101 lines show.
    let big = notes[2].0;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(("git-show-diff", big), cx);
        window.render_frame(cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(
            g.panel
                .large_shown
                .contains(&(false, "big.txt".to_string()))
        );
        let lines = g.panel.rows.iter().filter(|r| matches!(r, Row::Line { file, .. } if g.panel.files[*file].diff.path() == "big.txt")).count();
        assert_eq!(lines, 2101);
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_mode_change_says_so(cx: &mut TestAppContext) {
    use crate::git_block::diff_view::Note;
    use std::os::unix::fs::PermissionsExt;
    let repo = committed_repo();
    std::fs::set_permissions(
        repo.path().join("a.txt"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| {
        assert_eq!(
            g.panel.files[0].diff.mode,
            Some(("100644".to_string(), "100755".to_string()))
        );
        assert!(g.panel.rows.contains(&Row::Note {
            file: 0,
            note: Note::Mode
        }));
    });
    let _ = handle;
}

#[gpui_kit::test]
fn the_sticky_title_names_the_file_at_the_top_of_the_scroll(cx: &mut TestAppContext) {
    use gpui_kit::{point, px};
    let repo = git_repo();
    for i in 0..30 {
        std::fs::write(repo.path().join(format!("f{i:02}.txt")), "l\n".repeat(10)).unwrap();
    }
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let row_h = crate::git_block::diff_view::row_height(1.0);
    git_of(&ws, cx, id, |g| {
        assert_eq!(
            g.panel.top_file(row_h),
            None,
            "a section heading at the top"
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(("git-sticky", n)).is_some(),
            "drawn with no file at the top too: the list never jumps"
        );
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        // Heading, then 12 rows per file (its row, its hunk, 10 lines): row 15 is inside f01.
        let base = g.panel.scroll.0.borrow().base_handle.clone();
        base.set_offset(point(px(0.0), px(-15.0 * row_h)));
        assert_eq!(g.panel.top_file(row_h), Some(1));
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("git-sticky", n)).is_some());
    })
    .unwrap();
}

/// The rows are whole pixels at every zoom step, 1.5 × the text: the panel scales like a photo,
/// as an editor's rows do.
#[gpui_kit::test]
fn the_rows_are_whole_pixels_at_every_zoom(cx: &mut TestAppContext) {
    use templum_canvas::{Camera, ZOOM_STEPS};
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let row = line_row(&ws, cx, id, "zero");
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    for zoom in ZOOM_STEPS {
        let height = cx
            .update_window(handle, |_, window, cx| {
                ws.update(cx, |ws, _| {
                    // The block's corner near the view's: on screen at any zoom.
                    ws.space.canvas.camera =
                        Camera::at(Point::new(rect.x - 10.0, rect.y - 10.0), zoom);
                    ws.space.flight = None;
                });
                window.render_frame(cx);
                f32::from(window.find(("git-line", row)).bounds().size.height)
            })
            .unwrap();
        let want = crate::git_block::diff_view::row_height(zoom);
        assert_eq!(height, want, "at {zoom}");
        assert_eq!(want, want.round(), "whole pixels at {zoom}");
    }
}

#[gpui_kit::test]
fn folding_drops_the_selection_so_copy_never_takes_other_lines(cx: &mut TestAppContext) {
    use crate::git_block::diff_view::select::{Pos, Selection};
    let repo = mixed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let two = line_row(&ws, cx, id, "two");
    ws.update(cx, |ws, cx| {
        ws.space.git_mut(id).unwrap().panel.selection = Some(Selection {
            anchor: Pos { row: two, col: 0 },
            head: Pos { row: two, col: 3 },
        });
        assert_eq!(
            ws.space.git(id).unwrap().panel.selected_text().as_deref(),
            Some("two")
        );
        ws.fold_all(id, true, cx);
    });
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.panel.selection, None, "the rows it named are gone");
        assert_eq!(g.panel.selected_text(), None);
    });
    let _ = handle;
}

#[gpui_kit::test]
fn a_long_line_is_drawn_whole_and_the_panel_scrolls_sideways(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let long = "x".repeat(300);
    std::fs::write(repo.path().join("a.txt"), format!("{long}\none\n")).unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let row = line_row(&ws, cx, id, &long);
    let cw = ws.read_with(cx, |ws, _| {
        ws.cell.size.w * crate::git_block::FONT / ws.cell.font_size
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let panel = window.find(("git-diff", n)).bounds();
        let line = window.find(("git-line", row)).bounds();
        assert!(
            f32::from(line.size.width) >= cw * 300.0,
            "the row holds the whole text: {line:?}"
        );
        assert!(
            line.size.width > panel.size.width,
            "wider than the panel: it scrolls"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn show_diff_reads_a_new_file_left_unread(cx: &mut TestAppContext) {
    use crate::git_block::diff_view::Note;
    let repo = git_repo();
    for i in 0..201 {
        std::fs::write(repo.path().join(format!("f{i:03}.txt")), "x\n").unwrap();
    }
    // A staged file: its section follows, so the note isn't the list's last row (at the
    // block's clipped edge).
    std::fs::write(repo.path().join("z.txt"), "z\n").unwrap();
    git_run(repo.path(), &["add", "z.txt"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let ix = git_of(&ws, cx, id, |g| {
        g.panel
            .rows
            .iter()
            .position(|r| {
                matches!(
                    r,
                    Row::Note {
                        note: Note::Unread,
                        ..
                    }
                )
            })
            .expect("the 201st file is unread")
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, _| {
            let g = ws.space.git(id).unwrap();
            g.panel
                .scroll
                .scroll_to_item(ix, gpui_kit::ScrollStrategy::Top);
        });
        window.render_frame(cx);
        window.click(("git-show-diff", ix), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        let f = g
            .panel
            .files
            .iter()
            .find(|s| s.diff.path() == "f200.txt")
            .unwrap();
        assert!(!f.diff.unread);
        assert_eq!(f.diff.hunks[0].lines[0].text, "x");
    });
}
