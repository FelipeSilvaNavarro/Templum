//! One row of the diff panel as an element: a section heading, a file row with its actions, a
//! hunk row, a fold, a note, or a line painted on a canvas in the terminal's font.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Icon;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, ElementId, Font, Hsla, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, SharedString, TestSupportExt, TextAlign, TextRun,
    Window, canvas, div, fill, point, px, size,
};
use templum_canvas::BlockId;
use templum_git::{FileDiff, Kind};

use super::select::display;
use super::{Note, Row, Shaped, Shown, files_word};
use crate::editor::human;
use crate::git_block::FONT;
use crate::git_block::widgets::{badge, icon_button, small};
use crate::grid::hsla;
use crate::menu::Target;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// Row `ix` of the panel.
#[allow(clippy::too_many_arguments)]
pub(super) fn row(
    ws: &Workspace,
    id: BlockId,
    files: &[Shown],
    rows: &[Row],
    ix: usize,
    row_h: f32,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    match &rows[ix] {
        Row::Section { staged, files: n } => section(*staged, *n, ws.ui, row_h, z),
        Row::File { file } => file_row(ws, id, &files[*file], ix, row_h, z, cx),
        Row::Hunk { file, hunk } => hunk_row(&files[*file].diff.hunks[*hunk].title(), ws, row_h, z),
        Row::Fold { file, lines } => fold_row(ws, id, &files[*file], ix, *lines, row_h, z, cx),
        Row::Note { file, note } => note_row(ws, id, &files[*file], ix, *note, row_h, z, cx),
        Row::Line { file, hunk, line } => {
            line_row(ws, id, &files[*file], *file, *hunk, *line, ix, row_h, z, cx)
        }
    }
}

/// "+12 −3", dimmed.
pub(super) fn counts(d: &FileDiff, ui: Ui) -> Div {
    div()
        .flex_shrink_0()
        .text_color(ui.title_dim)
        .child(format!("+{} −{}", d.added(), d.removed()))
}

fn section(staged: bool, n: usize, ui: Ui, row_h: f32, z: f32) -> AnyElement {
    div()
        .w_full()
        .h(px(row_h))
        .flex()
        .items_center()
        .px(px(8.0 * z))
        .text_size(px(11.0 * z))
        .text_color(ui.title_dim)
        .child(format!(
            "{} · {}",
            if staged { "Staged" } else { "Changes" },
            files_word(n)
        ))
        .into_any_element()
}

/// The file's row: its fold arrow, badge, name (a rename as "old → new"), folder dimmed,
/// counts, and on hover its actions. A click folds it.
fn file_row(
    ws: &Workspace,
    id: BlockId,
    s: &Shown,
    ix: usize,
    row_h: f32,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let d = &s.diff;
    let (key, path) = (s.key(), d.path().to_string());
    let git = ws.space.git(id);
    let folded = git.is_some_and(|g| g.panel.folded.contains(&key));
    let busy = git.is_some_and(|g| g.acting > 0);
    // The badge from the status (an untracked file is U there, A in its diff); the diff's own
    // when the status hasn't caught up with the file.
    let entry = git.and_then(|g| g.status.entries.iter().find(|e| e.path == path));
    let change = entry
        .and_then(|e| if s.staged { e.staged } else { e.unstaged })
        .unwrap_or(d.status);
    let ids = |what: &str| ElementId::Name(format!("git-panel-{what}-{n}-{ix}").into());
    // Shown while the row is hovered; pressing one never folds the file.
    let hover_button = |what: &str, icon: IconName, tip: &'static str| {
        icon_button(ids(what), icon, ui, z)
            .invisible()
            .group_hover("git-file", |s| s.visible())
            .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    };
    let mut buttons: Vec<AnyElement> = Vec::new();
    if !busy {
        if s.staged {
            let p = path.clone();
            buttons.push(
                hover_button("unstage", IconName::Minus, "Unstage")
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.unstage(id, vec![p.clone()], window, cx)
                    }))
                    .test_support()
                    .into_any_element(),
            );
        } else {
            let (p, q) = (path.clone(), path.clone());
            buttons.push(
                hover_button("stage", IconName::Plus, "Stage")
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.stage(id, vec![p.clone()], window, cx)
                    }))
                    .test_support()
                    .into_any_element(),
            );
            buttons.push(
                hover_button("discard", IconName::Undo2, "Discard changes…")
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.discard(id, vec![q.clone()], window, cx)
                    }))
                    .test_support()
                    .into_any_element(),
            );
        }
        let p = path.clone();
        buttons.push(
            hover_button("open", IconName::FileCode, "Open in editor")
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.open_from_git(id, &p, window, cx)
                }))
                .test_support()
                .into_any_element(),
        );
    }
    let renamed = matches!((&d.old_path, &d.new_path), (Some(o), Some(n)) if o != n);
    let name = if renamed {
        format!(
            "{} → {}",
            d.old_path.as_deref().unwrap_or(""),
            d.new_path.as_deref().unwrap_or("")
        )
    } else {
        path.rsplit('/').next().unwrap_or(&path).to_string()
    };
    let folder = if renamed {
        String::new()
    } else {
        path.rsplit_once('/').map_or("", |(dir, _)| dir).to_string()
    };
    div()
        .id(("git-file", ix))
        .group("git-file")
        .w_full()
        .h(px(row_h))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .px(px(8.0 * z))
        .bg(ui.card)
        .text_size(px(FONT * z))
        .text_color(ui.title)
        .cursor_pointer()
        .child(
            Icon::new(if folded {
                IconName::ChevronRight
            } else {
                IconName::ChevronDown
            })
            .size(px(12.0 * z))
            .text_color(ui.title_dim),
        )
        .child(badge(ws, change, z))
        .child(div().truncate().child(name))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_color(ui.title_dim)
                .child(folder),
        )
        .child(counts(d, ui))
        .children(buttons)
        .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.toggle_fold(id, key.clone(), cx)))
        .test_support()
        .into_any_element()
}

fn hunk_row(title: &str, ws: &Workspace, row_h: f32, z: f32) -> AnyElement {
    let ui = ws.ui;
    div()
        .w_full()
        .h(px(row_h))
        .flex()
        .items_center()
        .px(px(8.0 * z))
        .bg(ui.hover)
        .font_family(ws.cell.font.family.clone())
        .text_size(px(FONT * z))
        .text_color(ui.title_dim)
        .child(format!("@@ {title}"))
        .into_any_element()
}

/// "⋯ 40 unchanged lines": a click shows the file whole.
#[allow(clippy::too_many_arguments)]
fn fold_row(
    ws: &Workspace,
    id: BlockId,
    s: &Shown,
    ix: usize,
    lines: u32,
    row_h: f32,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let key = s.key();
    div()
        .id(("git-fold", ix))
        .w_full()
        .h(px(row_h))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.0 * z))
        .text_color(ui.title_dim)
        .cursor_pointer()
        .hover(move |st| st.bg(ui.hover))
        .child(format!(
            "⋯ {lines} unchanged line{}",
            if lines == 1 { "" } else { "s" }
        ))
        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
            ws.show_whole(id, key.clone(), window, cx)
        }))
        .test_support()
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn note_row(
    ws: &Workspace,
    id: BlockId,
    s: &Shown,
    ix: usize,
    note: Note,
    row_h: f32,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let d = &s.diff;
    let (key, path) = (s.key(), d.path().to_string());
    let text = match note {
        Note::Binary => format!("Binary file · {}", human(d.bytes.unwrap_or(0))),
        Note::TooLarge => "Too large to show".to_string(),
        Note::Large if d.generated() => "Generated file".to_string(),
        Note::Large => format!("{} changed lines", d.changed()),
        Note::Mode => {
            let (old, new) = d.mode.clone().unwrap_or_default();
            format!("Mode {old} → {new}")
        }
    };
    let row = div()
        .id(("git-note", ix))
        .w_full()
        .h(px(row_h))
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .px(px(24.0 * z))
        .text_size(px(FONT * z))
        .text_color(ui.title_dim)
        .child(text);
    let row =
        match note {
            Note::Large => row.child(
                small(("git-show-diff", ix), "Show diff", ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                        ws.show_large(id, key.clone(), cx)
                    }))
                    .test_support(),
            ),
            Note::TooLarge => row.child(
                small(("git-open-large", ix), "Open in editor", ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.open_from_git(id, &path, window, cx)
                    }))
                    .test_support(),
            ),
            Note::Binary | Note::Mode => row,
        };
    row.test_support().into_any_element()
}

/// What a line row paints.
pub(super) struct LinePaint {
    pub text: SharedString,
    pub digits: usize,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub sign: char,
    /// The soft green or red behind an added or removed line.
    pub bg: Option<Hsla>,
    /// The language's colors, byte ranges of `text` (Task 6).
    pub colors: Rc<Vec<(Range<usize>, Hsla)>>,
    /// The changed words, a stronger green or red (Task 6).
    pub marks: Rc<Vec<Range<usize>>>,
    pub mark: Hsla,
    /// The selected bytes (Task 7).
    pub selection: Option<Range<usize>>,
    pub font: Font,
    /// One glyph's advance, screen pixels.
    pub cw: f32,
    pub row_h: f32,
    /// The text's size, screen pixels.
    pub size: f32,
    pub ui: Ui,
}

#[allow(clippy::too_many_arguments)]
fn line_row(
    ws: &Workspace,
    id: BlockId,
    s: &Shown,
    file: usize,
    hunk: usize,
    line: usize,
    ix: usize,
    row_h: f32,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let l = &s.diff.hunks[hunk].lines[line];
    let ansi = &ws.theme.palette.ansi;
    let (bg, mark) = match l.kind {
        Kind::Added => (Some(hsla(ansi[2], 0.15)), hsla(ansi[2], 0.35)),
        Kind::Removed => (Some(hsla(ansi[1], 0.15)), hsla(ansi[1], 0.35)),
        Kind::Context => (None, ui.selection),
    };
    let theme = cx.theme().highlight_theme.clone();
    let hunk_colors = ws
        .space
        .git(id)
        .map(|g| g.panel.colors.borrow_mut().of(file, hunk, s, &theme));
    let (colors, marks) = match &hunk_colors {
        Some(c) => (
            Rc::new(c.lines[line].clone()),
            Rc::new(c.marks[line].clone()),
        ),
        None => (Rc::new(Vec::new()), Rc::new(Vec::new())),
    };
    let text: SharedString = display(&l.text).into();
    let selection = ws
        .space
        .git(id)
        .and_then(|g| g.panel.selection)
        .and_then(|sel| sel.range_in(ix, text.len()));
    let paint = LinePaint {
        text,
        digits: s.digits(),
        old_no: l.old_no,
        new_no: l.new_no,
        sign: l.kind.sign(),
        bg,
        colors,
        marks,
        mark,
        selection,
        font: ws.cell.font.clone(),
        // The cell was measured at the terminal's size: scale its advance to the panel's.
        cw: ws.cell.size.w * FONT / ws.cell.font_size * z,
        row_h,
        size: FONT * z,
        ui,
    };
    let shaped = ws.space.git(id).map(|g| g.panel.shaped.clone());
    div()
        .id(("git-line", ix))
        .w_full()
        .h(px(row_h))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    let (line, x) = paint_line(&paint, bounds, window, cx);
                    if let Some(shaped) = &shaped {
                        shaped.borrow_mut().insert(ix, Shaped { line, x });
                    }
                },
            )
            .size_full(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                ws.diff_press(id, ix, e.position.x, e.click_count, cx)
            }),
        )
        .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
            if e.pressed_button == Some(MouseButton::Left) {
                ws.diff_drag(id, ix, e.position.x, cx)
            }
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                ws.open_menu(
                    Target::GitLine { block: id, row: ix },
                    e.position,
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .test_support()
        .into_any_element()
}

/// Paints one line: its background, the changed words, the selection, the numbers, the sign
/// and the text. Returns the shaped text and where it starts.
pub(super) fn paint_line(
    p: &LinePaint,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> (gpui_kit::ShapedLine, Pixels) {
    let (x0, y) = (bounds.origin.x, bounds.origin.y);
    // Columns: [1][old][1][new][1][sign][1][text].
    let col = |c: usize| x0 + px(p.cw * c as f32);
    let text_x = col(2 * p.digits + 5);
    if let Some(bg) = p.bg {
        window.paint_quad(fill(bounds, bg));
    }
    let runs = runs(&p.text, &p.colors, &p.font, p.ui.title);
    let line = window
        .text_system()
        .shape_line(p.text.clone(), px(p.size), &runs, None);
    let span = |r: &Range<usize>| {
        let (a, b) = (line.x_for_index(r.start), line.x_for_index(r.end));
        Bounds::new(point(text_x + a, y), size(b - a, bounds.size.height))
    };
    for r in p.marks.iter() {
        window.paint_quad(fill(span(r), p.mark));
    }
    if let Some(r) = p.selection.as_ref().filter(|r| r.start < r.end) {
        window.paint_quad(fill(span(r), p.ui.selection));
    }
    let mut dim = |s: String, at: usize| {
        let s: SharedString = s.into();
        let run = TextRun {
            len: s.len(),
            font: p.font.clone(),
            color: p.ui.title_dim,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let l = window.text_system().shape_line(s, px(p.size), &[run], None);
        let _ = l.paint(
            point(col(at), y),
            px(p.row_h),
            TextAlign::Left,
            None,
            window,
            cx,
        );
    };
    if let Some(n) = p.old_no {
        dim(format!("{n:>w$}", w = p.digits), 1);
    }
    if let Some(n) = p.new_no {
        dim(format!("{n:>w$}", w = p.digits), p.digits + 2);
    }
    if p.sign != ' ' {
        dim(p.sign.to_string(), 2 * p.digits + 3);
    }
    let _ = line.paint(
        point(text_x, y),
        px(p.row_h),
        TextAlign::Left,
        None,
        window,
        cx,
    );
    (line, text_x)
}

/// Text runs covering `text`: `colors` where they fall, `default` between them.
fn runs(text: &str, colors: &[(Range<usize>, Hsla)], font: &Font, default: Hsla) -> Vec<TextRun> {
    let run = |len: usize, color: Hsla| TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut out = Vec::new();
    let mut at = 0;
    for (r, color) in colors {
        let (start, end) = (r.start.max(at), r.end.min(text.len()));
        if start >= end {
            continue;
        }
        if start > at {
            out.push(run(start - at, default));
        }
        out.push(run(end - start, *color));
        at = end;
    }
    if at < text.len() {
        out.push(run(text.len() - at, default));
    }
    out
}
