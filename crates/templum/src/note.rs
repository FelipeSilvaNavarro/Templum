//! A note block: tabs of plain-text notes (`notes.rs` keeps them, app-wide), each edited in
//! gpui-kit's editor; with none open, a way to open one. And the Notes dialog, where every note is
//! found, opened and deleted. See docs/specs/cockpit.md, section 5.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_kit::component::{Icon, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, FontWeight, MouseButton,
    ScrollWheelEvent, SharedString, TestSupportExt, Window, div, px, relative,
};
use templum_canvas::{BlockId, Level, Size, chrome};

use crate::block::BORDER;
use crate::grid::{BAR_OPACITY, hsla};
use crate::notes;
use crate::silhouette::{self, Far};
use crate::tab_strip::{TABS_H, tab_strip};
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
pub(crate) const FIXED_LEFT: f32 = 6.0 + 6.0 + 18.0;
const FIXED_RIGHT: f32 = 10.0 + 10.0;
pub(crate) const FIXED_TOP: f32 = 8.0;
const FIXED_BOTTOM: f32 = 8.0;

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
            "note",
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
        None if level == Level::Full => empty(ws, id, z, cx),
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
        .border_color(ui.hairline)
        .text_size(px(13.0 * z))
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .active(move |s| s.bg(ui.pressed))
        .child(Icon::new(icon).size(px(14.0 * z)))
        .child(label)
}

/// A block with no note open: a new one, or one of those there are.
fn empty(ws: &Workspace, block: BlockId, z: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let n = block.0 as usize;
    div()
        .id(("note-empty", n))
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.0 * z))
        .child(
            div()
                .text_size(px(13.0 * z))
                .text_color(ws.ui.title_dim)
                .child("No note open here"),
        )
        .child(
            button(("note-new-here", n), IconName::Plus, "New note", ws, z)
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.new_note_tab(block, window, cx)
                }))
                .test_support(),
        )
        .child(
            button(
                ("note-open-here", n),
                IconName::NotebookText,
                "Open a note…",
                ws,
                z,
            )
            .on_click(
                cx.listener(move |_, _: &ClickEvent, window, cx| {
                    open_notes(Some(block), window, cx)
                }),
            )
            .test_support(),
        )
        .test_support()
        .into_any_element()
}

/// The note's editor as Full draws it, at zoom `z`, in the text's `area`. Its fixed parts would
/// make the text start further in, wrap at other words and show fewer lines the further the
/// canvas is zoomed out: the editor is as much larger (or smaller) and moved by its fixed left
/// and top parts' share, so the text starts, wraps and fits as if they zoomed too. Its lines
/// change only when the block is resized. At zoom 1, exactly the text's area.
fn editor_box(
    ws: &Workspace,
    n: usize,
    editor: &Entity<EditorState>,
    area: Size,
    z: f32,
    cx: &mut App,
) -> AnyElement {
    let (across, down) = (FIXED_LEFT + FIXED_RIGHT, FIXED_TOP + FIXED_BOTTOM);
    crate::editor::keep_rows(editor, FONT, z, cx);
    div()
        .id(("note-editor", n))
        .size_full()
        .font_family(ws.cell.font.family.clone())
        .child(
            div()
                .id(("note-text", n))
                .relative()
                .left(px(-FIXED_LEFT * (1.0 - z)))
                .top(px(-FIXED_TOP * (1.0 - z)))
                .w(px(((area.w - 2.0 * BORDER - across) * z + across).max(0.0)))
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
    let ws = cx.entity();
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Find a note"));
    let field = search.clone();
    {
        let ws = ws.clone();
        App::subscribe(cx, &search, move |_, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                ws.update(cx, |_, cx| cx.notify());
            }
        })
        .detach();
    }
    // The note whose trash was clicked once: the next click deletes it.
    let armed: Rc<Cell<Option<u64>>> = Rc::default();
    window.open_alert_dialog(cx, move |alert, _, cx| {
        let query = search.read(cx).value().to_string();
        let ws_now = ws.read(cx);
        let ui = ws_now.ui;
        let red = hsla(ws_now.theme.palette.ansi[1], 1.0);
        let now = notes::now();
        let rows: Vec<AnyElement> = ws_now
            .notes
            .find(&query)
            .into_iter()
            .map(|note| {
                let id = note.id;
                let at = ws_now.note_open_at(id).map(|(space, _)| {
                    if space.id == ws_now.space.id {
                        "Open".to_string()
                    } else {
                        format!("Open in {}", space.name)
                    }
                });
                let when = notes::ago(note.changed, now);
                let line = note
                    .text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let is_armed = armed.get() == Some(id);
                let trash = {
                    let (ws, armed) = (ws.clone(), armed.clone());
                    div()
                        .id(ElementId::Name(format!("note-delete-{id}").into()))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(6.0))
                        .text_size(px(12.0))
                        .text_color(if is_armed { red } else { ui.title_dim })
                        .cursor_pointer()
                        .hover(move |s| s.bg(ui.hover).text_color(red))
                        .child(Icon::new(IconName::Trash).size(px(14.0)))
                        .when(is_armed, |el| el.child("Delete for good"))
                        // The row behind opens the note: not on this press.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_: &ClickEvent, window, cx| {
                            if armed.get() == Some(id) {
                                armed.set(None);
                                ws.update(cx, |ws, cx| ws.delete_note(id, window, cx));
                            } else {
                                armed.set(Some(id));
                                ws.update(cx, |_, cx| cx.notify());
                            }
                        })
                        .test_support()
                };
                let open = ws.clone();
                div()
                    .id(ElementId::Name(format!("note-row-{id}").into()))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(ui.hairline)
                    .cursor_pointer()
                    .hover(move |s| s.bg(ui.hover).border_color(ui.hover_border))
                    .when_some(note.color, |el, color| {
                        el.child(
                            div()
                                .flex_shrink_0()
                                .size(px(10.0))
                                .rounded_full()
                                .bg(hsla(ws_now.theme.palette.ansi[color.ansi()], 1.0)),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(notes::title(&note.text)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .text_color(ui.title_dim)
                                    .child(match at {
                                        Some(at) => format!("{when} · {at}"),
                                        None => when,
                                    })
                                    .when(!line.is_empty(), |el| el.child(format!(" · {line}"))),
                            ),
                    )
                    .child(trash)
                    .on_click(move |_: &ClickEvent, window, cx| {
                        // First: closing gives the keyboard back to what had it before.
                        window.close_dialog(cx);
                        open.update(cx, |ws, cx| ws.open_note(id, into, window, cx));
                    })
                    .test_support()
                    .into_any_element()
            })
            .collect();
        let none = rows.is_empty().then(|| {
            div()
                .py(px(16.0))
                .text_size(px(13.0))
                .text_color(ui.title_dim)
                .child(if query.trim().is_empty() {
                    "No notes yet."
                } else {
                    "No note holds that."
                })
        });
        let new = {
            let ws = ws.clone();
            button("notes-new", IconName::Plus, "New note", ws_now, 1.0).on_click(
                move |_: &ClickEvent, window, cx| {
                    window.close_dialog(cx);
                    ws.update(cx, |ws, cx| match into {
                        Some(block) => ws.new_note_tab(block, window, cx),
                        None => ws.new_note(window, cx),
                    });
                },
            )
        };
        alert
            .title("Notes")
            .w(px(560.0))
            .button_props(DialogButtonProps::default().ok_text("Done"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("notes-find")
                            .child(Input::new(&search))
                            .test_support(),
                    )
                    .child(
                        div()
                            .id("notes-list")
                            .max_h(px(380.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .children(rows)
                            .children(none)
                            .test_support(),
                    )
                    .child(div().flex().child(new.test_support())),
            )
    });
    // After opening: the dialog takes the focus when it opens.
    field.update(cx, |input, cx| input.focus(window, cx));
}
