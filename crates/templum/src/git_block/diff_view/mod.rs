//! The diff panel: every working change in one scroll (docs/specs/git.md, section 2). The
//! rows are built from the files (`rows.rs`), drawn one by one (`paint.rs`) in a virtual
//! list; the text is selected by (row, byte) (`select.rs`).

mod highlight;
mod paint;
mod rows;
pub(crate) mod select;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, Div, ListHorizontalSizingBehavior, MouseButton, MouseUpEvent,
    Pixels, ShapedLine, Stateful, TestSupportExt, UniformListScrollHandle, div, px, uniform_list,
};
use templum_git::{FileDiff, WorkingDiff};

pub(crate) use rows::{Note, Row, Shown, rows, split_rows};

use super::widgets::{bar, centered, icon_button};
use super::{FONT, Git, Tab};
use crate::editor::keep_list_rows;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// The panel's keys, named by its keyboard button.
const KEYS: &str = if cfg!(target_os = "macos") {
    "↑↓ file · [ ] hunk · s stage · u unstage · d discard · ⏎ open · c message · ⌘⏎ commit"
} else {
    "↑↓ file · [ ] hunk · s stage · u unstage · d discard · ⏎ open · c message · Ctrl+⏎ commit"
};

/// A small icon button of the panel's header, named by its tooltip; `on`: tinted with the accent.
fn toggle(
    id: &'static str,
    icon: IconName,
    tip: &'static str,
    on: bool,
    ui: Ui,
    z: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(24.0 * z))
        .rounded(px(6.0 * z))
        .cursor_pointer()
        .when(on, |el| el.bg(ui.accent.opacity(0.16)))
        .hover(move |st| {
            st.bg(if on {
                ui.accent.opacity(0.24)
            } else {
                ui.hover
            })
        })
        .child(
            Icon::new(icon)
                .size(px(14.0 * z))
                .text_color(if on { ui.accent } else { ui.title }),
        )
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
}

/// A row's height at zoom `z`: whole pixels, like an editor's rows (`block::exact_rows`).
pub(crate) fn row_height(z: f32) -> f32 {
    (1.5 * FONT * z).round()
}

/// A line row as it was last painted: its shaped text and where the text starts, so the
/// mouse maps a point to a byte (`ShapedLine::closest_index_for_x`).
pub(crate) struct Shaped {
    pub line: ShapedLine,
    pub x: Pixels,
    /// Where its side's cell starts: Split's right half begins here.
    pub left: Pixels,
}

/// What the panel shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// The working tree: Changes then Staged, with their buttons (the Changes tab).
    Working,
    /// A read-only list of files, `title` naming it ("feature vs main · 7 commits"): no
    /// sections, no Stage or Discard, no whole-file folds.
    Range { title: String },
    /// A file's blame: one hunk per group of lines from one commit, its heading naming it;
    /// `hashes[hunk]` is that commit (a click opens it). Read-only, never Split.
    Blame { title: String, hashes: Vec<String> },
}

pub(crate) struct DiffPanel {
    /// What it shows.
    pub source: Source,
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
    /// The line rows painted in the last frame, by row index and side (Split's right one: true;
    /// a unified row is its left).
    pub shaped: Rc<RefCell<HashMap<(usize, bool), Shaped>>>,
    /// The text selected in the line rows.
    pub selection: Option<select::Selection>,
    /// The mouse is down since a press on a line row.
    pub dragging: bool,
    /// The hunks' colors, built on first draw (a RefCell: the rows are built from `&Workspace`).
    pub colors: RefCell<highlight::Highlights>,
    /// The row with the most columns: the list is as wide as it, and scrolls sideways.
    pub widest: usize,
    /// Diffs taken so far (the tests count them).
    pub reads: u64,
    /// The keyboard's hunk (`[` and `]`): its section, its file and its index in the file.
    pub cursor: Option<(bool, String, usize)>,
    /// What the panel says while it has no file, when not the source's own words ("Click a
    /// commit to see its changes").
    pub hint: Option<&'static str>,
    /// Old and new side by side (Split).
    pub split: bool,
    /// Split: the columns of one side, the widest side's.
    pub half: usize,
    /// Split: the selection is on the right side.
    pub split_right: bool,
}

impl DiffPanel {
    pub fn new() -> DiffPanel {
        DiffPanel {
            source: Source::Working,
            files: Rc::new(Vec::new()),
            rows: Rc::new(Vec::new()),
            folded: HashSet::new(),
            large_shown: HashSet::new(),
            whole: Vec::new(),
            ignore_ws: false,
            scroll: UniformListScrollHandle::new(),
            last_row: Cell::new(0.0),
            shaped: Rc::new(RefCell::new(HashMap::new())),
            selection: None,
            dragging: false,
            colors: RefCell::new(highlight::Highlights::default()),
            widest: 0,
            reads: 0,
            cursor: None,
            hint: None,
            split: false,
            half: 0,
            split_right: false,
        }
    }

    /// A read-only panel (`Source::Range`).
    pub fn range(title: String) -> DiffPanel {
        DiffPanel {
            source: Source::Range { title },
            ..DiffPanel::new()
        }
    }

    pub fn with_hint(mut self, hint: &'static str) -> DiffPanel {
        self.hint = Some(hint);
        self
    }

    pub fn working(&self) -> bool {
        self.source == Source::Working
    }

    /// A read-only panel back to nothing shown: no file, no title (a blame's goes too).
    pub fn clear(&mut self) {
        self.source = Source::Range {
            title: String::new(),
        };
        self.set_files(Vec::new());
    }

    /// What the panel says with no file: its hint, else the source's words.
    pub fn empty_text(&self) -> &'static str {
        if let Some(hint) = self.hint {
            hint
        } else if self.working() {
            "Nothing changed since the last commit"
        } else {
            "No changes"
        }
    }

    /// A read-only list of files (`Source::Range`); false when it equals what is shown.
    pub fn set_files(&mut self, files: Vec<FileDiff>) -> bool {
        self.set_shown(
            files
                .into_iter()
                .map(|diff| Shown {
                    staged: false,
                    diff,
                })
                .collect(),
        )
    }

    /// The cursor's hunk row: its hunk, else the file's last one (staging one shortens the
    /// file's list), else none (the file left the section).
    pub fn cursor_row(&self) -> Option<usize> {
        let (staged, path, hunk) = self.cursor.as_ref()?;
        let mut last = None;
        for (ix, row) in self.rows.iter().enumerate() {
            if let Row::Hunk { file, hunk: h } = row {
                let s = &self.files[*file];
                if s.staged == *staged && s.diff.path() == path {
                    if h == hunk {
                        return Some(ix);
                    }
                    last = Some(ix);
                }
            }
        }
        last
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
        self.set_shown(files)
    }

    /// Takes the files to show; false when they equal what is shown.
    pub fn set_shown(&mut self, files: Vec<Shown>) -> bool {
        if *self.files == files {
            return false;
        }
        let keys: HashSet<(bool, String)> = files.iter().map(Shown::key).collect();
        self.folded.retain(|k| keys.contains(k));
        self.large_shown.retain(|k| keys.contains(k));
        self.whole.retain(|k| keys.contains(k));
        self.files = Rc::new(files);
        self.colors.borrow_mut().clear();
        self.selection = None;
        self.rebuild();
        true
    }

    /// The rows again: after a fold, a Show diff.
    pub fn rebuild(&mut self) {
        let mut rows = rows(&self.files, &self.folded, &self.large_shown);
        if !self.working() {
            rows.retain(|r| !matches!(r, Row::Section { .. }));
        }
        // A blame stays unified: its groups aren't an old and a new side.
        if self.split && !matches!(self.source, Source::Blame { .. }) {
            rows = split_rows(rows, &self.files);
        }
        self.half = rows
            .iter()
            .filter_map(|r| match *r {
                Row::Pair {
                    file,
                    hunk,
                    old,
                    new,
                } => Some((file, hunk, [old, new])),
                _ => None,
            })
            .flat_map(|(file, hunk, sides)| {
                let s = &self.files[file];
                sides.into_iter().flatten().map(move |l| {
                    text_columns(s.digits())
                        + select::display(&s.diff.hunks[hunk].lines[l].text)
                            .chars()
                            .count()
                        + 1
                })
            })
            .max()
            .unwrap_or(0);
        self.rows = Rc::new(rows);
        // The rows a selection named are other rows now.
        self.selection = None;
        self.dragging = false;
        self.widest = (0..self.rows.len())
            .max_by_key(|&ix| self.columns(ix))
            .unwrap_or(0);
    }

    /// How many columns a row's text takes: a line row's numbers, sign and text; other rows
    /// fit the panel.
    pub fn columns(&self, row: usize) -> usize {
        match self.rows.get(row) {
            Some(Row::Line { file, hunk, line }) => {
                let s = &self.files[*file];
                // ponytail: a wide character (CJK) counts as one column, so such a line can
                // still be cut by a few cells; measure with the shaped text if it matters.
                text_columns(s.digits())
                    + select::display(&s.diff.hunks[*hunk].lines[*line].text)
                        .chars()
                        .count()
            }
            Some(Row::Pair { .. }) => 2 * self.half,
            _ => 0,
        }
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

    /// "Working changes · 4 files · +120 −30", or a range's title and its files.
    pub fn title(&self) -> String {
        let paths: HashSet<&str> = self.files.iter().map(|s| s.diff.path()).collect();
        let files = files_word(paths.len());
        match &self.source {
            Source::Working => {
                let added: u32 = self.files.iter().map(|s| s.diff.added()).sum();
                let removed: u32 = self.files.iter().map(|s| s.diff.removed()).sum();
                format!("Working changes · {files} · +{added} −{removed}")
            }
            // Nothing shown yet (History, Stashes): no title.
            Source::Range { title } if title.is_empty() => String::new(),
            Source::Range { title } => format!("{title} · {files}"),
            Source::Blame { title, .. } => title.clone(),
        }
    }

    /// The file whose rows are at the top of the scroll: the sticky title.
    pub fn top_file(&self, row_h: f32) -> Option<usize> {
        let base = self.scroll.0.borrow().base_handle.clone();
        let top = (-f32::from(base.offset().y) / row_h).floor().max(0.0) as usize;
        self.rows
            .get(top.min(self.rows.len().checked_sub(1)?))?
            .file()
    }

    /// The byte of row `row`'s display text under window x `x`, from the row as it was last
    /// painted; None for a row that isn't a line, or wasn't painted.
    pub fn col_at(&self, row: usize, x: Pixels) -> Option<usize> {
        let shaped = self.shaped.borrow();
        let s = shaped.get(&(row, self.split_right))?;
        Some(s.line.closest_index_for_x(x - s.x))
    }

    /// Split: window x `x` is on row `row`'s right side, as it was last painted.
    pub fn side_at(&self, row: usize, x: Pixels) -> bool {
        self.shaped
            .borrow()
            .get(&(row, true))
            .is_some_and(|s| x >= s.left)
    }

    /// A line row's display text; a pair row's, the side the selection is on (None for an
    /// empty cell).
    pub fn display_of(&self, row: usize) -> Option<String> {
        let (file, hunk, line) = match *self.rows.get(row)? {
            Row::Line { file, hunk, line } => (file, hunk, line),
            Row::Pair {
                file,
                hunk,
                old,
                new,
            } => (file, hunk, if self.split_right { new } else { old }?),
            _ => return None,
        };
        Some(select::display(
            &self.files[file].diff.hunks[hunk].lines[line].text,
        ))
    }

    /// A pair row's side's display text: None for an empty cell or another row.
    #[cfg(test)]
    pub fn display_of_side(&self, row: &Row, right: bool) -> Option<String> {
        let Row::Pair {
            file,
            hunk,
            old,
            new,
        } = *row
        else {
            return None;
        };
        let line = if right { new } else { old }?;
        Some(select::display(
            &self.files[file].diff.hunks[hunk].lines[line].text,
        ))
    }

    /// What Copy puts on the clipboard; None without a selection.
    pub fn selected_text(&self) -> Option<String> {
        let sel = self.selection.filter(|s| !s.is_empty())?;
        Some(select::selected_text(
            &self.rows,
            &self.files,
            &sel,
            self.split_right,
        ))
        .filter(|t| !t.is_empty())
    }
}

/// The columns before a line's text: [1][old][1][new][1][sign][1].
pub(crate) fn text_columns(digits: usize) -> usize {
    2 * digits + 5
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
pub(crate) fn panel(ws: &Workspace, git: &Git, z: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let ui = ws.ui;
    let p = git.diff();
    let row_h = row_height(z);
    let base = p.scroll.0.borrow().base_handle.clone();
    keep_list_rows(&base, &p.last_row, row_h);
    // Each frame paints the visible line rows again.
    p.shaped.borrow_mut().clear();
    // One toggle folds and unfolds the left column, in the same place (Branches has none).
    let fold = (git.tab != Tab::Branches).then(|| {
        let (id, icon, tip) = if git.lists_folded {
            (
                "git-unfold-lists",
                IconName::PanelLeftOpen,
                "Show the lists",
            )
        } else {
            ("git-fold-lists", IconName::PanelLeftClose, "Fold the lists")
        };
        icon_button(id, icon, ui, z)
            .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.toggle_lists(cx)))
            .test_support()
    });
    let header = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0 * z))
        .h(px(40.0 * z))
        .px(px(8.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .text_size(px(FONT * z))
        .children(fold)
        .child(
            div()
                .id("git-panel-title")
                .flex_1()
                .min_w(px(0.0))
                .px(px(4.0 * z))
                .truncate()
                .text_color(ui.title)
                .child(p.title())
                .test_support(),
        )
        // How the diff shows: only while there is one.
        .when(
            !p.is_empty() && !matches!(p.source, Source::Blame { .. }),
            |el| {
                el.child(
                    div()
                        .flex()
                        .p(px(2.0 * z))
                        .gap(px(2.0 * z))
                        .rounded(px(7.0 * z))
                        .bg(ui.card)
                        .child(
                            toggle("git-unified", IconName::Rows2, "Unified", !p.split, ui, z)
                                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                                    ws.set_split(false, cx)
                                }))
                                .test_support(),
                        )
                        .child(
                            toggle("git-split", IconName::Columns2, "Split", p.split, ui, z)
                                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                                    ws.set_split(true, cx)
                                }))
                                .test_support(),
                        ),
                )
            },
        )
        // On, it stays: the changes it hides could be all there are.
        .when(!p.is_empty() || p.ignore_ws, |el| {
            el.child(
                toggle(
                    "git-ignore-ws",
                    IconName::Space,
                    "Ignore whitespace",
                    p.ignore_ws,
                    ui,
                    z,
                )
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.toggle_ignore_whitespace(window, cx)
                }))
                .test_support(),
            )
        })
        .when(!p.is_empty(), |el| {
            el.child(
                toggle(
                    "git-expand-all",
                    IconName::ChevronsUpDown,
                    "Expand all",
                    false,
                    ui,
                    z,
                )
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.fold_all(false, cx)))
                .test_support(),
            )
            .child(
                toggle(
                    "git-collapse-all",
                    IconName::ChevronsDownUp,
                    "Collapse all",
                    false,
                    ui,
                    z,
                )
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.fold_all(true, cx)))
                .test_support(),
            )
        })
        // The keys act on the Changes tab alone.
        .when(p.working(), |el| {
            el.child(
                toggle("git-keys", IconName::Keyboard, KEYS, false, ui, z)
                    .cursor_default()
                    .test_support(),
            )
        });
    let body: AnyElement = if p.is_empty() {
        centered(ui, z)
            .flex_1()
            .child(p.empty_text())
            .into_any_element()
    } else {
        // Always drawn (empty with no file at the top): the list never jumps by its height.
        let top = p.top_file(row_h).map(|file| &p.files[file]);
        let sticky = bar(ui, z)
            .child(
                div()
                    .id("git-sticky")
                    .truncate()
                    .text_color(ui.title)
                    .child(top.map_or_else(|| " ".to_string(), |s| s.diff.path().to_string()))
                    .test_support(),
            )
            .child(div().flex_1())
            .children(top.map(|s| paint::counts(&s.diff, ui)));
        let (rows, files, entity) = (p.rows.clone(), p.files.clone(), cx.entity());
        let list = uniform_list("git-panel", rows.len(), move |range, _, cx| {
            entity.update(cx, |ws, cx| {
                let ws = &*ws;
                let cursor = ws.space.git().and_then(|g| g.diff().cursor_row());
                range
                    .map(|ix| paint::row(ws, &files, &rows, ix, cursor, row_h, z, cx))
                    .collect()
            })
        })
        .track_scroll(&p.scroll)
        // As wide as the widest row: a long line is read by scrolling sideways (shift+wheel).
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(Some(p.widest))
        .flex_1()
        .min_h(px(0.0));
        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(sticky)
            .child(list)
            .into_any_element()
    };
    div()
        .id("git-diff")
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(header)
        .child(body)
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |ws, _: &MouseUpEvent, _, cx| ws.diff_release(cx)),
        )
        .test_support()
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> templum_git::FileDiff {
        templum_git::parse_diff(
            &format!(
                "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-a\n+b\n"
            ),
            usize::MAX,
        )
        .pop()
        .unwrap()
    }

    #[test]
    fn a_range_has_no_sections_and_its_own_title() {
        let mut p = DiffPanel::range("feature vs main · 2 commits".to_string());
        assert!(!p.working());
        assert_eq!(p.empty_text(), "No changes");
        assert!(p.set_files(vec![file("x.rs"), file("y.rs")]));
        assert!(
            !p.set_files(vec![file("x.rs"), file("y.rs")]),
            "the same files change nothing"
        );
        assert!(
            matches!(p.rows[0], Row::File { file: 0 }),
            "no Changes heading: {:?}",
            p.rows[0]
        );
        assert!(p.rows.iter().all(|r| !matches!(r, Row::Section { .. })));
        assert_eq!(p.title(), "feature vs main · 2 commits · 2 files");
        let w = DiffPanel::new();
        assert!(w.working());
        assert_eq!(w.empty_text(), "Nothing changed since the last commit");
    }
}
