//! A note block: tabs of plain-text notes (`notes.rs` keeps them, app-wide), each edited in
//! gpui-kit's editor; with none open, a way to open one. And the Notes dialog, where every note is
//! found, opened and deleted. See docs/specs/cockpit.md, section 5.

use std::cell::RefCell;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, ScrollWheelEvent, SharedString,
    TestSupportExt, Window, div, px, relative,
};
use templum_canvas::{BlockId, Level, Size, chrome};

use crate::block::BORDER;
use crate::grid::{BAR_OPACITY, hsla};
use crate::list_dialog;
use crate::notes;
use crate::silhouette::{self, Far};
use crate::tab_strip::{LABELS_IN, TABS_H, tab_strip};
use crate::workspace::Workspace;

/// A new note's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size { w: 360.0, h: 280.0 };
pub(crate) const MIN: Size = Size { w: 200.0, h: 140.0 };
/// The text's size at zoom 1, world units.
pub(crate) const FONT: f32 = 13.0;
/// What gpui-kit's code editor keeps at fixed pixel sizes at any zoom: its padding
/// (gpui-component-0.7.3 src/input/input.rs:591-600 and sizing.rs:147-166, size Medium: 6 left
/// in a code editor, 10 right, 8 top and bottom) and, inside it, left of the text, the line
/// numbers' margin (6) and the fold icons (18), and right of it a margin its wrap width leaves
/// out (10) (gpui-base-0.7.3 src/input/base/element.rs:50-53, 1102-1124, 2562).
const FIXED_LEFT: f32 = 6.0 + 6.0 + 18.0;
const FIXED_RIGHT: f32 = 10.0 + 10.0;
pub(crate) const FIXED_TOP: f32 = 8.0;
const FIXED_BOTTOM: f32 = 8.0;
/// Where the text starts from the left of the area under the tabs, world units: in line with the
/// tabs' labels.
pub(crate) const TEXT_LEFT: f32 = LABELS_IN;
/// An empty block shows its icon when the area under the tabs is `ROOMY` tall (world units) and
/// its words from `TIGHT`; the smallest block's keeps to the two buttons.
const ROOMY: f32 = 170.0;
const TIGHT: f32 = 110.0;

/// A note block: the notes it shows as tabs, and the one on screen.
pub(crate) struct Note {
    pub tabs: Vec<NoteTab>,
    pub active: usize,
}

/// A note open in a block: its id in `Workspace::notes`, and its editor, whose every change is
/// written there.
pub(crate) struct NoteTab {
    pub id: u64,
    pub editor: Entity<EditorState>,
    /// Its word map, drawn far away (`silhouette.rs`).
    pub far: RefCell<Far>,
}

impl Note {
    pub fn tab(&self) -> Option<&NoteTab> {
        self.tabs.get(self.active)
    }

    /// The notes it shows, in tab order.
    pub fn ids(&self) -> Vec<u64> {
        self.tabs.iter().map(|t| t.id).collect()
    }

    /// The note on screen's text; "" without one.
    pub fn text(&self, cx: &App) -> String {
        self.tab()
            .map(|t| t.editor.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// What sending it to a terminal types: the selection, or the whole note.
    pub fn to_send(&self, cx: &App) -> String {
        let Some(tab) = self.tab() else {
            return String::new();
        };
        let selected = tab.editor.read(cx).selected_value();
        if selected.trim().is_empty() {
            self.text(cx)
        } else {
            selected.to_string()
        }
    }

    /// The note on screen's title; "Notes" without one.
    pub fn title(&self, cx: &App) -> SharedString {
        match self.tab() {
            Some(_) => notes::title(&self.text(cx)).into(),
            None => "Notes".into(),
        }
    }

    /// Typing goes to the note on screen; false without one.
    pub fn focus(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some(tab) = self.tab() else {
            return false;
        };
        tab.editor.update(cx, |editor, cx| editor.focus(window, cx));
        true
    }
}

/// A note's editor: plain text, wrapped at the block's width.
pub(crate) fn editor(text: &str, window: &mut Window, cx: &mut App) -> Entity<EditorState> {
    let text = text.to_string();
    cx.new(|cx| {
        EditorState::new(window, cx)
            .soft_wrap(true)
            .line_number(false)
            .default_value(text)
    })
}

/// The body below the header at the block's zoom `z`: the tab strip, then the note on screen
/// (below Full, its text's bars), or a way to open one.
pub(crate) fn body(
    ws: &Workspace,
    id: BlockId,
    note: &Note,
    z: f32,
    level: Level,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = id.0 as usize;
    let content = ws
        .space
        .canvas
        .get(id)
        .map_or(Size::default(), |b| chrome::content_rect(b.rect).size());
    // The text's area, below the tabs.
    let area = Size::new(content.w, (content.h - TABS_H).max(0.0));
    // Far away the tabs' place stays, empty: the bars stay where the text was.
    let strip = if level == Level::Full {
        let titles = note
            .tabs
            .iter()
            .map(|t| notes::title(&t.editor.read(cx).value()).into())
            .collect();
        tab_strip(
            ws,
            id,
            titles,
            note.active,
            z,
            Workspace::show_note_tab,
            Workspace::close_note_tab,
            Workspace::new_note_tab,
            cx,
        )
    } else {
        div().h(px(TABS_H * z)).into_any_element()
    };
    let inner = match note.tab() {
        None if level == Level::Full => empty(ws, id, area, z, cx),
        None => div().into_any_element(),
        Some(tab) => {
            // What its word map is of.
            let taken = silhouette::Taken {
                len: tab.editor.read(cx).text().len(),
                size: (area.w, area.h),
                font: ws.cell.font.family.clone(),
                wrap: true,
                tree: false,
            };
            if level == Level::Full {
                tab.far.borrow_mut().full = Some((z, taken));
                editor_box(ws, n, &tab.editor, area, z, cx)
            } else {
                // Its words as bars; the wheel moves the canvas.
                silhouette::body(
                    ("silhouette", n).into(),
                    &tab.editor,
                    &tab.far,
                    taken,
                    z,
                    FONT,
                    silhouette::note_origin,
                    hsla(ws.theme.palette.foreground, BAR_OPACITY),
                    |cx| editor_box(ws, n, &tab.editor, area, z, cx),
                    cx,
                )
            }
        }
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .text_size(px(FONT * z))
        // The wheel scrolls the note and stops there; with Ctrl it zooms the canvas.
        .when(level == Level::Full, |el| {
            el.on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
                if !e.modifiers.control {
                    cx.stop_propagation();
                }
            })
        })
        .child(strip)
        .child(div().relative().flex_1().min_h(px(0.0)).child(inner))
        .into_any_element()
}

/// A button of the empty block and of the Notes dialog: an icon and a label.
pub(crate) fn button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: &'static str,
    ws: &Workspace,
    z: f32,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    styled(id, icon, label, false, ws, z)
}

/// `button`, in the accent's tint: the one thing a block asks for first.
fn lit_button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: &'static str,
    ws: &Workspace,
    z: f32,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    styled(id, icon, label, true, ws, z)
}

/// Both buttons: `lit` tints it with the accent, else it is outlined by a hairline.
fn styled(
    id: impl Into<ElementId>,
    icon: IconName,
    label: &'static str,
    lit: bool,
    ws: &Workspace,
    z: f32,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let ui = ws.ui;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .px(px(12.0 * z))
        .py(px(8.0 * z))
        .rounded(px(8.0 * z))
        .border_1()
        .text_size(px(13.0 * z))
        .cursor_pointer()
        .map(|el| {
            if lit {
                el.border_color(gpui_kit::transparent_black())
                    .bg(ui.accent.opacity(0.16))
                    .text_color(ui.accent)
                    .hover(move |s| s.bg(ui.accent.opacity(0.24)))
                    .active(move |s| s.bg(ui.accent.opacity(0.30)))
            } else {
                el.border_color(ui.hairline)
                    .text_color(ui.title)
                    .hover(move |s| s.bg(ui.hover))
                    .active(move |s| s.bg(ui.pressed))
            }
        })
        .child(Icon::new(icon).size(px(14.0 * z)))
        .child(label)
}

/// A block with no note open: a new one, or one of those there are. Roomy enough, an icon tops
/// it; the new note is the tinted button.
fn empty(
    ws: &Workspace,
    block: BlockId,
    area: Size,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = block.0 as usize;
    let ui = ws.ui;
    let (roomy, tight) = (area.h >= ROOMY, area.h < TIGHT);
    let wide = |button: gpui_kit::Stateful<gpui_kit::Div>| {
        button
            .w(px(148.0 * z))
            .justify_center()
            .when(tight, |el| el.py(px(5.0 * z)))
    };
    let new = wide(lit_button(
        ("note-new-here", n),
        IconName::Plus,
        "New note",
        ws,
        z,
    ))
    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| ws.new_note_tab(block, window, cx)))
    .test_support();
    let open = wide(button(
        ("note-open-here", n),
        IconName::NotebookText,
        "Open a note…",
        ws,
        z,
    ))
    .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| open_notes(Some(block), window, cx)))
    .test_support();
    div()
        .id(("note-empty", n))
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.0 * z))
        .when(roomy, |el| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(36.0 * z))
                    .mb(px(4.0 * z))
                    .rounded(px(10.0 * z))
                    .bg(ui.accent.opacity(0.14))
                    .child(
                        Icon::new(IconName::NotebookText)
                            .size(px(18.0 * z))
                            .text_color(ui.accent),
                    ),
            )
        })
        .when(!tight, |el| {
            el.child(
                div()
                    .text_size(px(13.0 * z))
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .text_color(ui.title)
                    .child("No note open here"),
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.0 * z))
                .when(!tight, |el| el.mt(px(8.0 * z)))
                .child(new)
                .child(open),
        )
        .test_support()
        .into_any_element()
}

/// The note's editor as Full draws it, at zoom `z`, in the text's `area`. Its fixed parts would
/// make the text start further in, wrap at other words and show fewer lines the further the
/// canvas is zoomed out: the editor is as much larger (or smaller) and moved by its fixed left
/// and top parts' share, so the text starts (`TEXT_LEFT` in), wraps and fits as if they zoomed
/// too. Its lines change only when the block is resized. At zoom 1, the editor ends where the
/// text's area does.
fn editor_box(
    ws: &Workspace,
    n: usize,
    editor: &Entity<EditorState>,
    area: Size,
    z: f32,
    cx: &mut App,
) -> AnyElement {
    let (across, down) = (FIXED_LEFT + FIXED_RIGHT, FIXED_TOP + FIXED_BOTTOM);
    let wrap = area.w - 2.0 * BORDER - TEXT_LEFT - FIXED_RIGHT;
    crate::editor::keep_rows(editor, FONT, z, cx);
    div()
        .id(("note-editor", n))
        .size_full()
        .font_family(ws.cell.font.family.clone())
        .child(
            div()
                .id(("note-text", n))
                .relative()
                .left(px(TEXT_LEFT * z - FIXED_LEFT))
                .top(px(-FIXED_TOP * (1.0 - z)))
                .w(px((wrap * z + across).max(0.0)))
                .h(px(((area.h - down) * z + down).max(0.0)))
                .child(
                    // On the editor itself: it sets the theme's code size on itself, over a
                    // parent's.
                    Editor::new(editor)
                        .appearance(false)
                        .bordered(false)
                        .text_size(px(FONT * z))
                        .h(relative(1.0)),
                )
                .test_support(),
        )
        .test_support()
        .into_any_element()
}

/// The Notes dialog: every note, the most recently changed first, and a field that finds them by
/// their text. A click opens one: in the block `into` it was asked from, where it is open
/// already, or in a new block. Its trash deletes it on a second click, from every block too.
pub(crate) fn open_notes(into: Option<BlockId>, window: &mut Window, cx: &mut Context<Workspace>) {
    let kind = list_dialog::Kind {
        title: "Notes",
        prefix: "note",
        find: "Find a note",
        none: "No notes yet.",
        no_match: "No note holds that.",
        new: "New note",
    };
    list_dialog::open(
        kind,
        note_rows,
        move |ws, id, window, cx| ws.open_note(id, into, window, cx),
        |ws, id, window, cx| ws.delete_note(id, window, cx),
        move |ws, window, cx| match into {
            Some(block) => ws.new_note_tab(block, window, cx),
            None => ws.new_note(window, cx),
        },
        window,
        cx,
    );
}

/// The Notes dialog's rows for `query`: each note's title, then when it changed, where it is
/// open and its next line ("5 min ago · Open in Workspace 2 · milk"), its color's dot.
fn note_rows(ws: &Workspace, query: &str) -> Vec<list_dialog::Row> {
    let now = notes::now();
    ws.notes
        .find(query)
        .into_iter()
        .map(|note| {
            let at = list_dialog::open_at(ws, ws.note_open_at(note.id).map(|(space, _)| space));
            let line = note
                .text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .nth(1)
                .map(str::to_string);
            let detail: Vec<String> = [Some(notes::ago(note.changed, now)), at, line]
                .into_iter()
                .flatten()
                .collect();
            list_dialog::Row {
                id: note.id,
                title: notes::title(&note.text),
                detail: detail.join(" · "),
                dot: note
                    .color
                    .map(|color| hsla(ws.theme.palette.ansi[color.ansi()], 1.0)),
            }
        })
        .collect()
}
