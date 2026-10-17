//! A checklist block: tabs of checklists (`checklists.rs` keeps them, app-wide), each drawn from
//! `Workspace::checklists` at the block's zoom: its name, its items with their check boxes, and
//! + Add an item. With none open, a way to open one. See docs/specs/checklist.md.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::{Backspace, Escape, Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, Focusable as _, FontWeight,
    MouseButton, ScrollWheelEvent, SharedString, TestSupportExt, Window, div, px, relative,
};
use templum_canvas::{BlockId, Size};

use crate::checklists::{self, List};
use crate::note;
use crate::tab_strip::tab_strip;
use crate::workspace::Workspace;

/// A new checklist block's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size { w: 320.0, h: 360.0 };
pub(crate) const MIN: Size = Size { w: 220.0, h: 160.0 };
/// An item's text size and its name's, world units.
const FONT: f32 = 13.0;
const NAME_FONT: f32 = 15.0;
/// A check box's side, world units.
const BOX: f32 = 14.0;

/// A checklist block: the checklists it shows as tabs, by id, the one on screen, and the one
/// field being edited.
pub(crate) struct Checklist {
    pub tabs: Vec<u64>,
    pub active: usize,
    pub editing: Option<Editing>,
}

/// A name or an item edited in place, every key written to the checklist as it is typed.
pub(crate) struct Editing {
    pub list: u64,
    pub field: Field,
    pub input: Entity<InputState>,
    /// Its typing is an undo step already: the first key took it (or the add it came with).
    pub kept: bool,
}

/// What a field edits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Field {
    Name,
    Item(u64),
}

impl Checklist {
    /// The checklist on screen; None with no tab open.
    pub fn shown(&self) -> Option<u64> {
        self.tabs.get(self.active).copied()
    }

    /// The checklist on screen's title ("Deploy 2/5"); "Checklists" with none.
    pub fn title(&self, ws: &Workspace) -> SharedString {
        match self.shown().and_then(|id| ws.checklists.get(id)) {
            Some(list) => list.title().into(),
            None => "Checklists".into(),
        }
    }

    /// The keyboard is in its field.
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        self.editing.as_ref().is_some_and(|e| {
            e.input
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        })
    }

    /// The field editing `field` of checklist `list`, if it is.
    fn field(&self, list: u64, field: Field) -> Option<&Entity<InputState>> {
        self.editing
            .as_ref()
            .filter(|e| e.list == list && e.field == field)
            .map(|e| &e.input)
    }
}

fn name(what: &str, n: usize, id: u64) -> ElementId {
    ElementId::Name(format!("{what}-{n}-{id}").into())
}

/// The body below the header at the block's zoom `z`: the tab strip, then the checklist on
/// screen, or a way to open one.
pub(crate) fn body(
    ws: &Workspace,
    block: BlockId,
    checklist: &Checklist,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let titles = checklist
        .tabs
        .iter()
        .map(|id| {
            ws.checklists
                .get(*id)
                .map_or_else(|| checklists::NAME.to_string(), List::title)
                .into()
        })
        .collect();
    let strip = tab_strip(
        ws,
        block,
        "checklist",
        titles,
        checklist.active,
        z,
        Workspace::show_checklist_tab,
        Workspace::close_checklist_tab,
        Workspace::new_checklist_tab,
        cx,
    );
    let inner = match checklist.shown().and_then(|id| ws.checklists.get(id)) {
        Some(list) => list_view(ws, block, checklist, list, z, cx),
        None => empty(ws, block, z, cx),
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        // The wheel scrolls the checklist and stops there; with Ctrl it zooms the canvas.
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        .child(strip)
        .child(div().relative().flex_1().min_h(px(0.0)).child(inner))
        .into_any_element()
}

/// A checklist: its name, a row per item, then + Add an item; scrolling when it doesn't fit.
fn list_view(
    ws: &Workspace,
    block: BlockId,
    checklist: &Checklist,
    list: &List,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (ui, n, id) = (ws.ui, block.0 as usize, list.id);
    let rows: Vec<AnyElement> = list
        .items
        .iter()
        .map(|item| {
            let input = checklist.field(id, Field::Item(item.id));
            row(ws, block, id, item, input, z, cx)
        })
        .collect();
    let add = div()
        .id(("checklist-add", n))
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .py(px(4.0 * z))
        .text_size(px(FONT * z))
        .text_color(ui.title_dim)
        .cursor_pointer()
        .hover(move |s| s.text_color(ui.title))
        .child(Icon::new(IconName::Plus).size(px(BOX * z)))
        .child("Add an item")
        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
            ws.add_item_after(block, id, None, window, cx)
        }))
        .test_support();
    div()
        .id(("checklist-list", n))
        .size_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .px(px(12.0 * z))
        .py(px(8.0 * z))
        .child(
            div()
                .id(("checklist-name", n))
                .flex()
                .pb(px(4.0 * z))
                .text_size(px(NAME_FONT * z))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(ui.title)
                .map(|el| match checklist.field(id, Field::Name) {
                    Some(input) => el.child(field(ws, block, input, NAME_FONT, z, cx)),
                    None => el
                        .line_height(relative(1.5))
                        .cursor_text()
                        .child(list.name_shown().to_string())
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.edit_field(block, Field::Name, window, cx)
                        })),
                })
                .test_support(),
        )
        .children(rows)
        .child(add)
        .test_support()
        .into_any_element()
}

/// An item's row: its check box, its text (struck through and dimmed once checked), and, under
/// the pointer, a × that deletes it.
fn row(
    ws: &Workspace,
    block: BlockId,
    list: u64,
    item: &checklists::Item,
    input: Option<&Entity<InputState>>,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (ui, n, id, done) = (ws.ui, block.0 as usize, item.id, item.done);
    let group = SharedString::from(format!("checklist-row-{n}-{id}"));
    // The box sits on the text's first line.
    let line = FONT * 1.5;
    let check = div()
        .id(name("checklist-box", n, id))
        .flex_shrink_0()
        .mt(px((line - BOX) / 2.0 * z))
        .size(px(BOX * z))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.0 * z))
        .border_1()
        .border_color(if done { ui.accent } else { ui.title_dim })
        .when(done, |el| {
            el.bg(ui.accent).child(
                Icon::new(IconName::Check)
                    .size(px(10.0 * z))
                    .text_color(ui.surface),
            )
        })
        .cursor_pointer()
        .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.toggle_item(list, id, cx)))
        .test_support();
    let text = div()
        .id(name("checklist-text", n, id))
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .text_size(px(FONT * z))
        .text_color(if done { ui.title_dim } else { ui.title })
        .map(|el| match input {
            // One line while it is edited: a long item scrolls sideways in its field.
            Some(input) => el.child(field(ws, block, input, FONT, z, cx)),
            None => el
                .line_height(relative(1.5))
                .when(done, |el| el.line_through())
                .cursor_text()
                .child(item.text.clone())
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.edit_field(block, Field::Item(id), window, cx)
                })),
        })
        .test_support();
    let delete = div()
        .id(name("checklist-item-delete", n, id))
        .flex_shrink_0()
        .mt(px((line - 16.0) / 2.0 * z))
        .size(px(16.0 * z))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0 * z))
        .opacity(0.0)
        .group_hover(group.clone(), |s| s.opacity(1.0))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(
            Icon::new(IconName::Close)
                .size(px(10.0 * z))
                .text_color(ui.title_dim),
        )
        .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.delete_item(list, id, cx)))
        .test_support();
    div()
        .id(ElementId::Name(group.clone()))
        .group(group)
        .flex()
        .items_start()
        .gap(px(8.0 * z))
        .py(px(4.0 * z))
        .child(check)
        .child(text)
        .child(delete)
        .test_support()
        .into_any_element()
}

/// The field editing a name or an item, in its text's place: one line of the text's `size` at
/// zoom `z`, its padding and height set to scale with the zoom. Esc stops editing; Backspace in an
/// empty item deletes it.
fn field(
    ws: &Workspace,
    block: BlockId,
    input: &Entity<InputState>,
    size: f32,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        // A press in the field is its own: the block would take the keyboard from it.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        // What the field leaves: Esc with nothing of its own to undo, Backspace at its start.
        .on_action(
            cx.listener(move |ws, _: &Escape, window, cx| ws.stop_editing(block, window, cx)),
        )
        .on_action(cx.listener(move |ws, _: &Backspace, window, cx| {
            ws.backspace_at_start(block, window, cx)
        }))
        .child(
            // Not `Input::h`: gpui-kit's own, for a multi-line input only.
            Styled::h(Input::new(input), px(size * 1.5 * z))
                .appearance(false)
                .px(px(0.0))
                .py(px(0.0))
                .text_size(px(size * z))
                .line_height(relative(1.5))
                .text_color(ws.ui.title),
        )
        .into_any_element()
}

/// A block with no checklist open: a new one.
fn empty(ws: &Workspace, block: BlockId, z: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let n = block.0 as usize;
    div()
        .id(("checklist-empty", n))
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
                .child("No checklist open here"),
        )
        .child(
            note::button(
                ("checklist-new-here", n),
                IconName::Plus,
                "New checklist",
                ws,
                z,
            )
            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                ws.new_checklist_tab(block, window, cx)
            }))
            .test_support(),
        )
        .test_support()
        .into_any_element()
}
