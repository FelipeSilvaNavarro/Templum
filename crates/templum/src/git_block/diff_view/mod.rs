//! The diff panel: every working change in one scroll (docs/specs/git.md, section 2). The
//! rows are built from the files (`rows.rs`), drawn one by one (`paint.rs`) in a virtual
//! list; the text is selected by (row, byte) (`select.rs`).

mod paint;
mod rows;
pub(crate) mod select;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, Pixels, ShapedLine, TestSupportExt, UniformListScrollHandle,
    div, px, uniform_list,
};
use templum_canvas::BlockId;
use templum_git::WorkingDiff;

pub(crate) use rows::{Note, Row, Shown, rows};

use super::widgets::{bar, centered, small};
use super::{FONT, Git};
use crate::editor::keep_list_rows;
use crate::workspace::Workspace;

/// A row's height at zoom `z`: whole pixels, like an editor's rows (`block::exact_rows`).
pub(crate) fn row_height(z: f32) -> f32 {
    (1.5 * FONT * z).round()
}

/// A line row as it was last painted: its shaped text and where the text starts, so the
/// mouse maps a point to a byte (`ShapedLine::closest_index_for_x`).
// ponytail: read by the selection (plan Task 7), which removes this allow.
#[allow(dead_code)]
pub(crate) struct Shaped {
    pub line: ShapedLine,
    pub x: Pixels,
}

pub(crate) struct DiffPanel {
    /// The files shown, Changes then Staged (an Rc: the list's closure holds them).
    pub files: Rc<Vec<Shown>>,
    pub rows: Rc<Vec<Row>>,
    /// Files whose row alone shows.
    pub folded: HashSet<(bool, String)>,
    /// Large or generated files the user asked to see.
    pub large_shown: HashSet<(bool, String)>,
    /// Files read with every line as context (a fold was clicked).
    pub whole: Vec<(bool, String)>,
    /// `git diff -w`.
    pub ignore_ws: bool,
    pub scroll: UniformListScrollHandle,
    /// The row height when the list was last drawn (`keep_list_rows`).
    pub last_row: Cell<f32>,
    /// The line rows painted in the last frame, by row index.
    pub shaped: Rc<RefCell<HashMap<usize, Shaped>>>,
    /// Diffs taken so far (the tests count them).
    pub reads: u64,
}

impl DiffPanel {
    pub fn new() -> DiffPanel {
        DiffPanel {
            files: Rc::new(Vec::new()),
            rows: Rc::new(Vec::new()),
            folded: HashSet::new(),
            large_shown: HashSet::new(),
            whole: Vec::new(),
            ignore_ws: false,
            scroll: UniformListScrollHandle::new(),
            last_row: Cell::new(0.0),
            shaped: Rc::new(RefCell::new(HashMap::new())),
            reads: 0,
        }
    }

    /// Takes a read. False when it equals what is shown: nothing changes, so the scroll, the
    /// selection and the colors stay (the spec's data flow).
    pub fn set(&mut self, diff: WorkingDiff) -> bool {
        let files: Vec<Shown> = diff
            .unstaged
            .into_iter()
            .map(|diff| Shown {
                staged: false,
                diff,
            })
            .chain(
                diff.staged
                    .into_iter()
                    .map(|diff| Shown { staged: true, diff }),
            )
            .collect();
        if *self.files == files {
            return false;
        }
        let keys: HashSet<(bool, String)> = files.iter().map(Shown::key).collect();
        self.folded.retain(|k| keys.contains(k));
        self.large_shown.retain(|k| keys.contains(k));
        self.whole.retain(|k| keys.contains(k));
        self.files = Rc::new(files);
        self.rebuild();
        true
    }

    /// The rows again: after a fold, a Show diff.
    pub fn rebuild(&mut self) {
        self.rows = Rc::new(rows(&self.files, &self.folded, &self.large_shown));
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The row of a file's heading.
    pub fn row_of(&self, staged: bool, path: &str) -> Option<usize> {
        self.rows.iter().position(|r| {
            matches!(r, Row::File { file }
                if self.files[*file].staged == staged && self.files[*file].diff.path() == path)
        })
    }

    /// Where Open in editor goes for a file shown.
    pub fn first_line(&self, staged: bool, path: &str) -> Option<u32> {
        self.files
            .iter()
            .find(|s| s.staged == staged && s.diff.path() == path)
            .map(Shown::first_line)
    }

    /// "Working changes · 4 files · +120 −30".
    pub fn title(&self) -> String {
        let added: u32 = self.files.iter().map(|s| s.diff.added()).sum();
        let removed: u32 = self.files.iter().map(|s| s.diff.removed()).sum();
        let paths: HashSet<&str> = self.files.iter().map(|s| s.diff.path()).collect();
        format!(
            "Working changes · {} · +{added} −{removed}",
            files_word(paths.len())
        )
    }

    /// The file whose rows are at the top of the scroll: the sticky title.
    pub fn top_file(&self, row_h: f32) -> Option<usize> {
        let base = self.scroll.0.borrow().base_handle.clone();
        let top = (-f32::from(base.offset().y) / row_h).floor().max(0.0) as usize;
        self.rows
            .get(top.min(self.rows.len().checked_sub(1)?))?
            .file()
    }
}

/// "1 file", "4 files".
pub(crate) fn files_word(n: usize) -> String {
    if n == 1 {
        "1 file".to_string()
    } else {
        format!("{n} files")
    }
}

/// The panel: its header, the sticky title, and the rows.
pub(crate) fn panel(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let p = &git.panel;
    let row_h = row_height(z);
    let base = p.scroll.0.borrow().base_handle.clone();
    keep_list_rows(&base, &p.last_row, row_h);
    // Each frame paints the visible line rows again.
    p.shaped.borrow_mut().clear();
    let header = bar(ui, z)
        .child(
            div()
                .id(("git-panel-title", n))
                .truncate()
                .text_color(ui.title)
                .child(p.title())
                .test_support(),
        )
        .child(div().flex_1())
        .child(
            small(("git-ignore-ws", n), "Ignore whitespace", ui, z)
                .when(p.ignore_ws, |el| el.bg(ui.card))
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.toggle_ignore_whitespace(id, window, cx)
                }))
                .test_support(),
        )
        .child(
            small(("git-expand-all", n), "Expand all", ui, z)
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.fold_all(id, false, cx)))
                .test_support(),
        )
        .child(
            small(("git-collapse-all", n), "Collapse all", ui, z)
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.fold_all(id, true, cx)))
                .test_support(),
        );
    let body: AnyElement = if p.is_empty() {
        centered(ui, z)
            .flex_1()
            .child("Nothing changed since the last commit")
            .into_any_element()
    } else {
        let sticky = p.top_file(row_h).map(|file| {
            let s = &p.files[file];
            bar(ui, z)
                .child(
                    div()
                        .id(("git-sticky", n))
                        .truncate()
                        .text_color(ui.title)
                        .child(s.diff.path().to_string())
                        .test_support(),
                )
                .child(div().flex_1())
                .child(paint::counts(&s.diff, ui))
        });
        let (rows, files, entity) = (p.rows.clone(), p.files.clone(), cx.entity());
        let list = uniform_list(("git-panel", n), rows.len(), move |range, _, cx| {
            entity.update(cx, |ws, cx| {
                let ws = &*ws;
                range
                    .map(|ix| paint::row(ws, id, &files, &rows, ix, row_h, z, cx))
                    .collect()
            })
        })
        .track_scroll(&p.scroll)
        .flex_1()
        .min_h(px(0.0));
        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .children(sticky)
            .child(list)
            .into_any_element()
    };
    div()
        .id(("git-diff", n))
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(header)
        .child(body)
        .test_support()
        .into_any_element()
}
