//! A kanban block's body: a board's columns and cards with drag and drop, or a word that it shows
//! none; the Boards dialog, and the dialog of an open card. Everything is drawn at the block's zoom `z`, like a
//! terminal's text. See docs/specs/kanban.md.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::Confirm;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, BoxShadow, ClickEvent, ClipboardItem, Context, Div, DragMoveEvent, ElementId,
    Entity, FontWeight, Hsla, MouseButton, MouseDownEvent, Render, ScrollHandle, ScrollWheelEvent,
    SharedString, Stateful, TestSupportExt, Window, div, point, px, relative,
};
use templum_canvas::{BlockId, EDGE_ZONE, Point, Rect, Size, edge_scroll};

use crate::board::{Assignee, Board, Boards, Card, Column, Comment, Label, Priority};
use crate::checklists;
use crate::git_block::Check;
use crate::git_block::widgets::{check_box, field, icon_button};
use crate::grid::hsla;
use crate::list_dialog;
use crate::menu::Target;
use crate::note;
use crate::notes;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// A new kanban block's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size { w: 900.0, h: 540.0 };
pub(crate) const MIN: Size = Size { w: 420.0, h: 280.0 };
const COLUMN_W: f32 = 260.0;

/// A kanban block: the board it shows, if any.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Kanban {
    pub board: Option<u64>,
}

impl Kanban {
    /// The board on screen: none when it was deleted.
    pub fn shown<'a>(&self, ws: &'a Workspace) -> Option<&'a Board> {
        self.board.and_then(|id| ws.boards.board(id))
    }

    /// The block's title.
    pub fn title(&self, ws: &Workspace) -> SharedString {
        match self.shown(ws) {
            Some(board) => board.name.clone().into(),
            None => "Boards".into(),
        }
    }
}

/// A card being dragged.
struct CardDrag {
    board: u64,
    card: u64,
}

/// A column being dragged by its title.
struct ColumnDrag {
    board: u64,
    column: u64,
}

/// What follows the pointer while dragging: the card or column's title (a checklist's item), at
/// the canvas's zoom.
pub(crate) struct DragPreview {
    pub text: SharedString,
    pub width: f32,
    pub z: f32,
    pub ui: Ui,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (z, ui) = (self.z, self.ui);
        div()
            .w(px(self.width))
            .px(px(10.0 * z))
            .py(px(8.0 * z))
            .rounded(px(8.0 * z))
            .bg(ui.surface)
            .border_1()
            .border_color(ui.accent)
            .shadow(vec![BoxShadow {
                color: ui.shadow,
                offset: point(px(0.0), px(8.0 * z)),
                blur_radius: px(20.0 * z),
                spread_radius: px(0.0),
                inset: false,
            }])
            .text_size(px(13.0 * z))
            .text_color(ui.title)
            .opacity(0.92)
            .child(self.text.clone())
    }
}

/// How fast a drag at `pointer` scrolls a container at `bounds` (both in pixels) along its
/// axis, in world units a second, positive toward the end: the canvas's edge rule measured in
/// world units, so the zone and the speed scale with the zoom `z`. Zero when the pointer is
/// outside the container across the axis (over another column), or more than a zone past its
/// ends: every container hears every drag, another block's too.
pub(crate) fn drag_speed(pointer: Point, bounds: Rect, vertical: bool, z: f32) -> f32 {
    let local = Point::new((pointer.x - bounds.x) / z, (pointer.y - bounds.y) / z);
    let size = Size {
        w: bounds.w / z,
        h: bounds.h / z,
    };
    let ((across, width), (along, length)) = if vertical {
        ((local.x, size.w), (local.y, size.h))
    } else {
        ((local.y, size.h), (local.x, size.w))
    };
    if !(0.0..=width).contains(&across) || !(-EDGE_ZONE..=length + EDGE_ZONE).contains(&along) {
        return 0.0;
    }
    let speed = edge_scroll(local, size);
    if vertical { speed.y } else { speed.x }
}

/// A scroll container that scrolls while a drag holds the pointer near its edge: its handle,
/// the speed the last drag move gave (world units a second), and when it last moved.
#[derive(Default)]
struct EdgeScroll {
    handle: ScrollHandle,
    speed: f32,
    at: Option<Instant>,
}

/// The `EdgeScroll` of the container `key`, kept across frames, moved by this frame's step;
/// asks for the next frame while it scrolls.
fn edge_scroll_state(
    key: impl Into<ElementId>,
    vertical: bool,
    z: f32,
    window: &mut Window,
    cx: &mut App,
) -> Entity<EdgeScroll> {
    let edge = window.use_keyed_state(key, cx, |_, _| EdgeScroll::default());
    let dragging = cx.has_active_drag();
    if step(&edge, vertical, z, dragging, Instant::now(), cx) {
        window.request_animation_frame();
    }
    edge
}

/// One frame of scrolling while a drag is near an edge: the offset moves by the speed × `z` ×
/// the time since the last frame (at most 0.05 s, so a stalled frame doesn't jump; none the
/// first time), within the container's ends. True while it scrolls. A drag that ended, a
/// pointer out of the zone, or an end reached stops it at once.
fn step(
    edge: &Entity<EdgeScroll>,
    vertical: bool,
    z: f32,
    dragging: bool,
    now: Instant,
    cx: &mut App,
) -> bool {
    edge.update(cx, |e, _| {
        if !dragging || e.speed == 0.0 {
            e.speed = 0.0;
            e.at = None;
            return false;
        }
        let dt =
            e.at.map_or(0.0, |at| now.duration_since(at).as_secs_f32().min(0.05));
        e.at = Some(now);
        let (offset, max) = (e.handle.offset(), e.handle.max_offset());
        let by = px(e.speed * z * dt);
        let moved = if vertical {
            point(offset.x, (offset.y - by).clamp(-max.y, px(0.0)))
        } else {
            point((offset.x - by).clamp(-max.x, px(0.0)), offset.y)
        };
        e.handle.set_offset(moved);
        // At an end it stops, asking for no more frames until the pointer moves again.
        dt == 0.0 || moved != offset
    })
}

/// A drag move over a container of `block`: its speed from where the pointer is; none while
/// another block is on top there (every container hears every drag).
fn on_edge<T: 'static>(
    edge: &Entity<EdgeScroll>,
    block: BlockId,
    vertical: bool,
    z: f32,
    cx: &mut Context<Workspace>,
) -> impl Fn(&DragMoveEvent<T>, &mut Window, &mut App) + 'static {
    let edge = edge.clone();
    cx.listener(move |ws, e: &DragMoveEvent<T>, window, cx| {
        let (p, b) = (e.event.position, e.bounds);
        let pointer = Point::new(p.x.into(), p.y.into());
        let bounds = Rect {
            x: b.origin.x.into(),
            y: b.origin.y.into(),
            w: b.size.width.into(),
            h: b.size.height.into(),
        };
        let speed = if ws.block_under(p).is_some_and(|top| top != block) {
            0.0
        } else {
            drag_speed(pointer, bounds, vertical, z)
        };
        edge.update(cx, |e, _| e.speed = speed);
        if speed != 0.0 {
            window.refresh();
        }
    })
}

fn name(prefix: &str, n: usize, id: u64) -> ElementId {
    ElementId::Name(format!("{prefix}-{n}-{id}").into())
}

/// The body below the header: the board, or a word that there is none.
pub(crate) fn body(
    ws: &Workspace,
    block: BlockId,
    kanban: Kanban,
    z: f32,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let view = match kanban.shown(ws) {
        Some(board) => board_view(ws, block, board, z, window, cx),
        None => empty(ws, block, z, cx),
    };
    // The wheel scrolls what is in the block, and stops there; with Ctrl it zooms the canvas.
    div()
        .size_full()
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        .child(view)
        .into_any_element()
}

/// A block with no board: a new one, or one from the dialog.
fn empty(ws: &Workspace, block: BlockId, z: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let n = block.0 as usize;
    div()
        .id(("kanban-empty", n))
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
                .child("No board open here"),
        )
        .child(
            note::button(("board-new-here", n), IconName::Plus, "New board", ws, z)
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.new_board(block, cx)))
                .test_support(),
        )
        .child(
            note::button(
                ("board-open-here", n),
                IconName::LayoutDashboard,
                "Open a board…",
                ws,
                z,
            )
            .on_click(
                cx.listener(move |_, _: &ClickEvent, window, cx| open_boards(block, window, cx)),
            )
            .test_support(),
        )
        .test_support()
        .into_any_element()
}

/// The Boards dialog: every board, and a field that finds them by their name and their cards'
/// titles. A click, or New board, shows the board in the block `into`; its trash deletes it on a
/// second click, and the blocks that showed it show none.
pub(crate) fn open_boards(into: BlockId, window: &mut Window, cx: &mut Context<Workspace>) {
    let kind = list_dialog::Kind {
        title: "Boards",
        prefix: "board",
        find: "Find a board",
        none: "No boards yet.",
        no_match: "No board holds that.",
        new: "New board",
    };
    list_dialog::open(
        kind,
        rows,
        move |ws, id, _, cx| ws.show_board(into, id, cx),
        |ws, id, _, cx| ws.delete_board(id, cx),
        move |ws, _, cx| ws.new_board(into, cx),
        window,
        cx,
    );
}

/// The Boards dialog's rows for `query`: each board's name, then its cards and where it is open
/// ("12 cards · Open").
fn rows(ws: &Workspace, query: &str) -> Vec<list_dialog::Row> {
    ws.boards
        .find(query)
        .into_iter()
        .map(|board| {
            let cards = match board.card_count() {
                1 => "1 card".to_string(),
                n => format!("{n} cards"),
            };
            let at = ws.board_open_at(board.id);
            let detail: Vec<String> = [Some(cards), list_dialog::open_at(ws, at)]
                .into_iter()
                .flatten()
                .collect();
            list_dialog::Row {
                id: board.id,
                title: board.name.clone(),
                detail: detail.join(" · "),
                dot: None,
            }
        })
        .collect()
}

fn board_view(
    ws: &Workspace,
    block: BlockId,
    board: &Board,
    z: f32,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = block.0 as usize;
    let board_id = board.id;
    let columns: Vec<AnyElement> = board
        .columns
        .iter()
        .map(|column| column_view(ws, n, board_id, column, z, window, cx))
        .collect();
    let edge = edge_scroll_state(("columns-scroll", n), false, z, window, cx);
    let add = div()
        .id(("add-column", n))
        .flex_shrink_0()
        .w(px(COLUMN_W * z))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .px(px(12.0 * z))
        .py(px(10.0 * z))
        .rounded(px(10.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .text_size(px(13.0 * z))
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .active(move |s| s.bg(ui.pressed))
        .child(Icon::new(IconName::Plus).size(px(14.0 * z)))
        .child("Add a column")
        .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
            ws.edit_boards(|b| b.add_column(board_id));
            cx.notify();
        }))
        .test_support();
    // The columns are the scroll container's own children: GPUI measures what there is to
    // scroll from them.
    div()
        .id(("columns", n))
        .size_full()
        .flex()
        .items_start()
        .gap(px(10.0 * z))
        .p(px(4.0 * z))
        .overflow_x_scroll()
        .track_scroll(&edge.read(cx).handle)
        // A card, or a column by its title, dragged near the left or right scrolls the columns.
        .on_drag_move(on_edge::<CardDrag>(&edge, block, false, z, cx))
        .on_drag_move(on_edge::<ColumnDrag>(&edge, block, false, z, cx))
        .children(columns)
        .child(add)
        .test_support()
        .into_any_element()
}

fn column_view(
    ws: &Workspace,
    n: usize,
    board: u64,
    column: &Column,
    z: f32,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let col = column.id;
    let title = SharedString::from(column.title.clone());
    let preview_title = title.clone();
    let count = column.cards.len();
    let header = div()
        .id(name("column-title", n, col))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.0 * z))
        .px(px(10.0 * z))
        .pt(px(10.0 * z))
        .pb(px(6.0 * z))
        .cursor_grab()
        .child(
            div()
                .truncate()
                .text_size(px(13.0 * z))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.clone()),
        )
        .child(
            div()
                .px(px(6.0 * z))
                .rounded_full()
                .bg(ui.hover)
                .text_size(px(11.0 * z))
                .text_color(ui.title_dim)
                .child(count.to_string()),
        )
        .on_drag(ColumnDrag { board, column: col }, move |_, _, _, cx| {
            let text = preview_title.clone();
            cx.new(|_| DragPreview {
                text,
                width: COLUMN_W * z,
                z,
                ui,
            })
        })
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                // Its block first, so Ctrl+Z after the menu undoes on the board.
                ws.focus_block(BlockId(n as u64), window, cx);
                let target = Target::Column { board, column: col };
                ws.open_menu(target, e.position, window, cx);
                cx.stop_propagation();
            }),
        );
    let cards: Vec<AnyElement> = column
        .cards
        .iter()
        .map(|card| card_view(ws, n, board, col, card, z, cx))
        .collect();
    let edge = edge_scroll_state(name("cards-scroll", n, col), true, z, window, cx);
    let add = div()
        .id(name("add-card", n, col))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .mx(px(6.0 * z))
        .mb(px(6.0 * z))
        .px(px(8.0 * z))
        .py(px(6.0 * z))
        .rounded(px(6.0 * z))
        .text_size(px(12.0 * z))
        .text_color(ui.title_dim)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover).text_color(ui.title))
        .active(move |s| s.bg(ui.pressed))
        .child(Icon::new(IconName::Plus).size(px(13.0 * z)))
        .child("Add a card")
        .on_click(
            cx.listener(move |ws, _: &ClickEvent, window, cx| add_card(ws, board, col, window, cx)),
        )
        .test_support();
    div()
        .id(name("column", n, col))
        .flex_shrink_0()
        .w(px(COLUMN_W * z))
        .max_h_full()
        .flex()
        .flex_col()
        .rounded(px(10.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .bg(ui.card)
        .drag_over::<CardDrag>(move |s, _, _, _| s.border_color(ui.accent))
        .drag_over::<ColumnDrag>(move |s, _, _, _| s.border_color(ui.accent))
        // A card dropped on the column's empty space goes to its end.
        .on_drop(cx.listener(move |ws, drag: &CardDrag, _, cx| {
            ws.edit_boards(|b| b.move_card(drag.board, drag.card, col, None));
            cx.notify();
        }))
        .on_drop(cx.listener(move |ws, drag: &ColumnDrag, _, cx| {
            ws.edit_boards(|b| b.move_column(drag.board, drag.column, col));
            cx.notify();
        }))
        // A vertical wheel scrolls the cards, never the columns sideways.
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            let d = e.delta.pixel_delta(px(16.0));
            if !e.modifiers.control && d.y.abs() >= d.x.abs() {
                cx.stop_propagation();
            }
        })
        .child(header)
        .child(
            div()
                .id(name("cards", n, col))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(&edge.read(cx).handle)
                // A card dragged near the top or bottom of the list scrolls it.
                .on_drag_move(on_edge::<CardDrag>(&edge, BlockId(n as u64), true, z, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0 * z))
                        .px(px(6.0 * z))
                        .pb(px(6.0 * z))
                        .children(cards),
                )
                .test_support(),
        )
        .child(add)
        .test_support()
        .into_any_element()
}

/// Who `assignee` is, as a card and its dialog show it: a block's title (its id when no
/// workspace has it), or "Any Codex".
pub(crate) fn assignee_label(ws: &Workspace, assignee: &Assignee, cx: &App) -> String {
    match assignee {
        Assignee::Block(uid) => ws.uid_title(uid, cx).unwrap_or_else(|| uid.clone()),
        Assignee::Agent(id) => format!("Any {}", ws.agent_title(id)),
    }
}

/// A priority's signal icon, in its color.
fn priority_icon(ws: &Workspace, priority: Priority, size: f32) -> Icon {
    let palette = ws.theme.palette;
    let (icon, color) = match priority {
        Priority::Low => (IconName::SignalLow, ws.ui.title_dim),
        Priority::Medium => (IconName::SignalMedium, hsla(palette.ansi[3], 1.0)),
        Priority::High => (IconName::SignalHigh, hsla(palette.ansi[1], 1.0)),
    };
    Icon::new(icon).size(px(size)).text_color(color)
}

/// A card's meta row, on every card: its id, priority, checklist, description, comments and,
/// at the right, who it is for.
fn meta_row(ws: &Workspace, n: usize, card: &Card, z: f32, cx: &App) -> impl IntoElement {
    let ui = ws.ui;
    let id = card.id;
    let (done, all) = card.progress();
    let count = card.comments.len();
    let icon = |name: IconName| Icon::new(name).size(px(12.0 * z)).text_color(ui.title_dim);
    div()
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .text_size(px(11.0 * z))
        .text_color(ui.title_dim)
        .child(
            div()
                .id(name("card-id", n, id))
                .child(format!("#{id}"))
                .test_support(),
        )
        .when_some(card.priority, |el, p| {
            el.child(
                div()
                    .id(name("priority", n, id))
                    .flex()
                    .child(priority_icon(ws, p, 12.0 * z))
                    .test_support(),
            )
        })
        .when(all > 0, |el| {
            let color = if done == all { ui.accent } else { ui.title_dim };
            el.child(
                div()
                    .id(name("checklist", n, id))
                    .flex()
                    .items_center()
                    .gap(px(3.0 * z))
                    .text_color(color)
                    .child(
                        Icon::new(IconName::ListChecks)
                            .size(px(12.0 * z))
                            .text_color(color),
                    )
                    .child(format!("{done}/{all}"))
                    .test_support(),
            )
        })
        .when(!card.description.is_empty(), |el| {
            el.child(icon(IconName::FileText))
        })
        .when(count > 0, |el| {
            el.child(
                div()
                    .id(name("comments", n, id))
                    .flex()
                    .items_center()
                    .gap(px(3.0 * z))
                    .child(icon(IconName::MessageSquare))
                    .child(count.to_string())
                    .test_support(),
            )
        })
        .when_some(card.assignee.as_ref(), |el, a| {
            el.child(
                div()
                    .id(name("assignee", n, id))
                    .ml_auto()
                    .min_w_0()
                    .truncate()
                    .child(assignee_label(ws, a, cx))
                    .test_support(),
            )
        })
}

fn card_view(
    ws: &Workspace,
    n: usize,
    board: u64,
    column: u64,
    card: &Card,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let palette = ws.theme.palette;
    let id = card.id;
    let title = SharedString::from(card.title.clone());
    let preview_title = title.clone();
    div()
        .id(name("card", n, id))
        .flex()
        .flex_col()
        .gap(px(6.0 * z))
        .px(px(10.0 * z))
        .py(px(8.0 * z))
        .rounded(px(8.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .bg(ui.surface)
        .shadow(vec![BoxShadow {
            color: ui.shadow.opacity(0.35),
            offset: point(px(0.0), px(1.0 * z)),
            blur_radius: px(2.0 * z),
            spread_radius: px(0.0),
            inset: false,
        }])
        .cursor_pointer()
        .hover(move |s| s.border_color(ui.hover_border))
        .when(!card.labels.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .gap(px(4.0 * z))
                    .children(card.labels.iter().map(|label| {
                        div()
                            .w(px(32.0 * z))
                            .h(px(6.0 * z))
                            .rounded_full()
                            .bg(hsla(palette.ansi[label.ansi()], 1.0))
                    })),
            )
        })
        .child(div().text_size(px(13.0 * z)).child(title))
        .child(meta_row(ws, n, card, z, cx))
        .drag_over::<CardDrag>(move |s, _, _, _| s.border_color(ui.accent))
        .on_drag(CardDrag { board, card: id }, move |_, _, _, cx| {
            let text = preview_title.clone();
            cx.new(|_| DragPreview {
                text,
                width: (COLUMN_W - 12.0) * z,
                z,
                ui,
            })
        })
        // Dropped on a card, a card takes its place; the column doesn't get the drop too.
        .on_drop(cx.listener(move |ws, drag: &CardDrag, _, cx| {
            ws.edit_boards(|b| b.move_card(drag.board, drag.card, column, Some(id)));
            cx.stop_propagation();
            cx.notify();
        }))
        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
            if let Some(current) = ws.boards.card(board, id).cloned() {
                open_card(board, current, false, window, cx)
            }
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                ws.focus_block(BlockId(n as u64), window, cx);
                ws.open_menu(Target::Card { board, card: id }, e.position, window, cx);
                cx.stop_propagation();
            }),
        )
        .test_support()
        .into_any_element()
}

/// "New card" at the end of `column`, opened with its title selected.
pub(crate) fn add_card(
    ws: &mut Workspace,
    board: u64,
    column: u64,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(card) = ws.edit_boards(|b| b.add_card(board, column))
        && let Some(current) = ws.boards.card(board, card).cloned()
    {
        cx.notify();
        open_card(board, current, true, window, cx);
    }
}

/// A change typed in a card's dialog: the first one keeps the boards for undo.
fn edit_typed(ws: &mut Workspace, typed: &Cell<bool>, f: impl FnOnce(&mut Boards)) {
    if typed.replace(true) {
        f(&mut ws.boards);
    } else {
        ws.edit_boards(f);
    }
}

/// The open card's assignee button, showing `who`: its menu gives the card to anyone, one
/// agent session running now and wired to a kanban block of the board, or any session of an
/// agent. Each choice is one Ctrl+Z step.
fn assignee_menu(
    ws: &Entity<Workspace>,
    board: u64,
    card: u64,
    current: Option<Assignee>,
    who: String,
) -> impl IntoElement {
    let ws = ws.clone();
    Button::new("card-assignee")
        .small()
        .outline()
        .dropdown_caret(true)
        .label(who)
        .dropdown_menu(move |menu, window, cx| {
            let set = |to| give(ws.clone(), board, card, to);
            let ws_now = ws.read(cx);
            let mut sessions = ws_now.agent_sessions(board, cx);
            // The current block first when it runs no agent now (closed, or its agent quit).
            if let Some(Assignee::Block(uid)) = &current
                && !sessions.iter().any(|(s, _)| s == uid)
            {
                let label = assignee_label(ws_now, &Assignee::Block(uid.clone()), cx);
                sessions.insert(0, (uid.clone(), label));
            }
            let agents: Vec<(String, String)> = ws_now
                .agents
                .iter()
                .map(|a| (a.id.clone(), a.title.clone()))
                .collect();
            let mut menu = menu.item(
                PopupMenuItem::new("Anyone")
                    .checked(current.is_none())
                    .on_click(set(None)),
            );
            if !sessions.is_empty() {
                menu = menu.separator().label("Sessions");
                for (uid, label) in sessions {
                    let to = Assignee::Block(uid);
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(current.as_ref() == Some(&to))
                            .on_click(set(Some(to))),
                    );
                }
            }
            let (ws, current) = (ws.clone(), current.clone());
            menu.separator()
                .submenu("Any session of", window, cx, move |mut menu, _, _| {
                    for (id, title) in &agents {
                        let to = Assignee::Agent(id.clone());
                        menu = menu.item(
                            PopupMenuItem::new(title.clone())
                                .checked(current.as_ref() == Some(&to))
                                .on_click(give(ws.clone(), board, card, Some(to))),
                        );
                    }
                    menu
                })
        })
}

/// A paste into "Add an item": several lines are an item each, added at once (the field is
/// one line); a single line is pasted into it as usual.
fn paste_items(
    ws: Entity<Workspace>,
    board: u64,
    card: u64,
) -> impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static {
    move |clip, _, cx| {
        let Some(text) = clip.text().filter(|t| t.contains('\n')) else {
            return false;
        };
        let items = checklists::parse_items(&text);
        ws.update(cx, |ws, cx| {
            ws.edit_boards(|b| b.add_items(board, card, &items));
            cx.notify();
        });
        true
    }
}

/// A click that gives the card to `to`.
fn give(
    ws: Entity<Workspace>,
    board: u64,
    card: u64,
    to: Option<Assignee>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        let to = to.clone();
        ws.update(cx, |ws, cx| {
            ws.edit_boards(|b| b.edit_card(board, card, |c| c.assignee = to));
            cx.notify();
        })
    }
}

/// Where a card is, as its dialog says it: "In {column} on {board}".
fn place(boards: &Boards, board: u64, card: u64) -> String {
    let Some(b) = boards.board(board) else {
        return String::new();
    };
    let column = b
        .columns
        .iter()
        .find(|c| c.cards.iter().any(|x| x.id == card))
        .map_or("", |c| c.title.as_str());
    format!("In {column} on {}", b.name)
}

/// The card dialog's width, and the room at the sides of its header, body and footer. It is the
/// window's dialog: its sizes are pixels at the window's scale, whatever the canvas's zoom.
const DIALOG_W: f32 = 600.0;
const SIDE: f32 = 20.0;

/// A section's header in the card's dialog: its icon and name at the left and, at the right, what
/// it holds.
fn section(ui: Ui, icon: IconName, label: &'static str, count: Option<String>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(Icon::new(icon).size(px(13.0)).text_color(ui.title_dim))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(ui.title)
                .child(label),
        )
        .when_some(count, |el, count| {
            el.child(
                div()
                    .ml_auto()
                    .text_size(px(11.0))
                    .text_color(ui.title)
                    .child(count),
            )
        })
}

/// A property's row in the card's dialog: its icon and name, then what the caller adds.
fn property(ui: Ui, icon: IconName, label: &'static str) -> Div {
    div()
        .min_h(px(32.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .w(px(92.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.0))
                .text_color(ui.title)
                .child(Icon::new(icon).size(px(14.0)).text_color(ui.title_dim))
                .child(label),
        )
}

/// A button of the dialog's footer and comment field: `label` in `color` on `bg`, with an icon
/// before it, if any.
fn pill(
    id: impl Into<ElementId>,
    label: &'static str,
    icon: Option<IconName>,
    bg: Hsla,
    color: Hsla,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.0))
        .px(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(8.0))
        .bg(bg)
        .text_color(color)
        .text_size(px(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .when_some(icon, |el, icon| {
            el.child(Icon::new(icon).size(px(14.0)).text_color(color))
        })
        .child(label)
}

/// A comment in the thread: its author's round mark, then who and when above what was said.
/// `mine`: the person's own, in the theme's accent.
fn comment_view(ui: Ui, tint: Hsla, mine: bool, who: &str, when: String, text: String) -> Div {
    let initial = who
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    div()
        .flex()
        .items_start()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(26.0))
                .rounded_full()
                .bg(tint.opacity(0.16))
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(tint)
                .child(initial),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(who.to_string()),
                        )
                        .child(div().text_size(px(11.0)).text_color(ui.title).child(when)),
                )
                .child(
                    div()
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .bg(if mine {
                            ui.accent.opacity(0.10)
                        } else {
                            ui.card
                        })
                        .text_size(px(13.0))
                        .line_height(relative(1.5))
                        .child(text),
                ),
        )
}

/// The open card: title, description and labels apply as they change. `new`: the title is
/// selected, to type over "New card".
pub(crate) fn open_card(
    board: u64,
    current: Card,
    new: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let card = current.id;
    let ws = cx.entity();
    let title = cx.new(|cx| InputState::new(window, cx).default_value(current.title.clone()));
    let title_focus = title.clone();
    let description = cx.new(|cx| {
        TextareaState::new(window, cx)
            .rows(4)
            .placeholder("Add a description")
            .default_value(current.description.clone())
    });
    let comment = cx.new(|cx| {
        TextareaState::new(window, cx)
            .rows(2)
            .placeholder("Write a comment")
    });
    let item = cx.new(|cx| InputState::new(window, cx).placeholder("Add an item"));
    // Enter adds what is typed (a line per item) and empties the field; it needs the window.
    {
        let ws = ws.clone();
        window
            .subscribe(&item, cx, move |input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let items = checklists::parse_items(&input.read(cx).value());
                    if items.is_empty() {
                        return;
                    }
                    ws.update(cx, |ws, cx| {
                        ws.edit_boards(|b| b.add_items(board, card, &items));
                        cx.notify();
                    });
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                }
            })
            .detach();
    }
    // Subscribed through the app: the workspace these update isn't leased, and they need no
    // window. Enter in the title confirms the dialog by itself. The dialog's typing is one undo
    // step for the board (its fields undo each key): taken at its first change.
    let typed = Rc::new(Cell::new(false));
    {
        let (ws, typed) = (ws.clone(), typed.clone());
        App::subscribe(cx, &title, move |input, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                let text = input.read(cx).value().to_string();
                ws.update(cx, |ws, cx| {
                    edit_typed(ws, &typed, |b| b.edit_card(board, card, |c| c.title = text));
                    cx.notify();
                })
            }
        })
        .detach();
    }
    {
        let ws = ws.clone();
        App::subscribe(cx, &description, move |input, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                let text = input.read(cx).value().to_string();
                ws.update(cx, |ws, cx| {
                    edit_typed(ws, &typed, |b| {
                        b.edit_card(board, card, |c| c.description = text)
                    });
                    cx.notify();
                })
            }
        })
        .detach();
    }
    // What is typed in the comment field is no change of the card, but it lights Comment up.
    {
        let ws = ws.clone();
        App::subscribe(cx, &comment, move |_, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                ws.update(cx, |_, cx| cx.notify())
            }
        })
        .detach();
    }
    // A card gone meanwhile (an agent, another block, Ctrl+Z) draws its last values.
    let last = RefCell::new((current, String::new()));
    window.open_alert_dialog(cx, move |alert, _, cx| {
        let ws_now = ws.read(cx);
        let ui = ws_now.ui;
        let palette = ws_now.theme.palette;
        if let Some(c) = ws_now.boards.card(board, card) {
            *last.borrow_mut() = (c.clone(), place(&ws_now.boards, board, card));
        }
        let (shown, place) = last.borrow().clone();
        // Each change from the dialog is one step of the kanban's Ctrl+Z.
        let edit = {
            let ws = ws.clone();
            move |f: Rc<dyn Fn(&mut Boards)>| {
                let ws = ws.clone();
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    ws.update(cx, |ws, cx| {
                        ws.edit_boards(|b| f(b));
                        cx.notify();
                    })
                }
            }
        };
        // The six labels: a ring of the label's color, filled and checked while the card has it.
        let toggles = Label::ALL.map(|label| {
            let on = shown.labels.contains(&label);
            let color = hsla(palette.ansi[label.ansi()], 1.0);
            let name = label.name();
            div()
                .id(ElementId::Name(format!("label-{name}").into()))
                .flex()
                .items_center()
                .justify_center()
                .size(px(22.0))
                .rounded_full()
                .border_2()
                .border_color(color)
                .when(on, |el| {
                    el.bg(color).child(
                        Icon::new(IconName::Check)
                            .size(px(12.0))
                            .text_color(hsla(palette.background, 1.0)),
                    )
                })
                .cursor_pointer()
                .hover(move |s| s.bg(color.opacity(if on { 0.85 } else { 0.25 })))
                .tooltip(move |window, cx| {
                    Tooltip::new(format!("Toggle the {name} label")).build(window, cx)
                })
                .on_click(edit(Rc::new(move |b| b.toggle_label(board, card, label))))
                .test_support()
        });
        // The priorities side by side in one rounded strip; the card's own is lit.
        let chips = std::iter::once(None)
            .chain(Priority::ALL.map(Some))
            .map(|p| {
                let on = shown.priority == p;
                let word = p.map_or("None", Priority::title);
                div()
                    .id(ElementId::Name(
                        format!("priority-{}", p.map_or("none", Priority::name)).into(),
                    ))
                    .h(px(24.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .rounded(px(6.0))
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .map(|el| {
                        if on {
                            el.bg(ui.accent.opacity(0.16))
                                .text_color(ui.accent)
                                .font_weight(FontWeight::MEDIUM)
                        } else {
                            el.text_color(ui.title).hover(move |s| s.bg(ui.hover))
                        }
                    })
                    .when_some(p, |el, p| el.child(priority_icon(ws_now, p, 12.0)))
                    .child(word)
                    .on_click(edit(Rc::new(move |b| {
                        b.edit_card(board, card, |c| c.priority = p)
                    })))
                    .test_support()
            });
        let now = notes::now();
        let author = |by: &str| {
            if by == "you" {
                "You".to_string()
            } else if by == templum_agent::ASKER {
                "Templum Agent".to_string()
            } else {
                ws_now.uid_title(by, cx).unwrap_or_else(|| by.to_string())
            }
        };
        let agent_tint = hsla(palette.ansi[5], 1.0);
        let comments: Vec<_> = shown
            .comments
            .iter()
            .map(|c| {
                let mine = c.by == "you";
                let tint = if mine { ui.accent } else { agent_tint };
                comment_view(
                    ui,
                    tint,
                    mine,
                    &author(&c.by),
                    notes::ago(c.at, now),
                    c.text.clone(),
                )
            })
            .collect();
        let can_send = !comment.read(cx).value().trim().is_empty();
        let add = {
            let (ws, comment) = (ws.clone(), comment.clone());
            let (bg, color) = if can_send {
                (ui.accent, ui.surface)
            } else {
                (ui.hover, ui.title_dim)
            };
            pill("add-comment", "Comment", Some(IconName::ArrowUp), bg, color)
                .h(px(26.0))
                .px(px(10.0))
                .when(!can_send, |el| el.cursor_default())
                .tooltip(|window, cx| Tooltip::new("Add the comment").build(window, cx))
                .on_click(move |_: &ClickEvent, window, cx| {
                    let text = comment.read(cx).value().trim().to_string();
                    if text.is_empty() {
                        return;
                    }
                    ws.update(cx, |ws, cx| {
                        let said = Comment {
                            by: "you".into(),
                            text,
                            at: notes::now(),
                        };
                        ws.edit_boards(|b| b.edit_card(board, card, |c| c.comments.push(said)));
                        cx.notify();
                    });
                    comment.update(cx, |c, cx| c.set_value("", window, cx));
                })
                .test_support()
        };
        let delete = {
            let ws = ws.clone();
            let red = hsla(palette.ansi[1], 1.0);
            div()
                .id("delete-card")
                .h(px(30.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(px(8.0))
                .text_size(px(12.5))
                .text_color(red)
                .cursor_pointer()
                .hover(move |s| s.bg(red.opacity(0.12)))
                .child(Icon::new(IconName::Trash).size(px(14.0)).text_color(red))
                .child("Delete card")
                .on_click(move |_: &ClickEvent, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.edit_boards(|b| b.remove_card(board, card));
                        cx.notify();
                    });
                    window.close_dialog(cx);
                })
                .test_support()
        };
        let who = shown
            .assignee
            .as_ref()
            .map_or_else(|| "Anyone".to_string(), |a| assignee_label(ws_now, a, cx));
        let unassign = shown.assignee.is_some().then(|| {
            icon_button("unassign", IconName::Close, ui, 1.0)
                .tooltip(|window, cx| Tooltip::new("Give it to anyone").build(window, cx))
                .on_click(edit(Rc::new(move |b| {
                    b.edit_card(board, card, |c| c.assignee = None)
                })))
                .test_support()
        });
        let count = shown.comments.len();
        let (done, all) = shown.progress();
        let items = shown.checklist.iter().map(|i| {
            let (id, done) = (i.id, i.done);
            let group = SharedString::from(format!("card-item-row-{id}"));
            let check = check_box(
                ElementId::Name(format!("card-item-box-{id}").into()),
                if done {
                    Check::Checked
                } else {
                    Check::Unchecked
                },
                ui,
                1.0,
            )
            .mt(px((13.0 * 1.5 - 16.0) / 2.0))
            .on_click(edit(Rc::new(move |b| {
                b.set_item_done(board, card, id, !done);
            })))
            .test_support();
            let delete = div()
                .id(ElementId::Name(format!("card-item-delete-{id}").into()))
                .flex_shrink_0()
                .mt(px((13.0 * 1.5 - 18.0) / 2.0))
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .opacity(0.0)
                .group_hover(group.clone(), |s| s.opacity(1.0))
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child(
                    Icon::new(IconName::Close)
                        .size(px(11.0))
                        .text_color(ui.title_dim),
                )
                .tooltip(|window, cx| Tooltip::new("Remove the item").build(window, cx))
                .on_click(edit(Rc::new(move |b| b.remove_item(board, card, id))))
                .test_support();
            div()
                .id(ElementId::Name(group.clone()))
                .group(group)
                .flex()
                .items_start()
                .gap(px(10.0))
                .px(px(6.0))
                .py(px(5.0))
                .rounded(px(6.0))
                .hover(move |s| s.bg(ui.hover))
                .child(check)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(13.0))
                        .line_height(relative(1.5))
                        .when(done, |el| el.text_color(ui.title_dim).line_through())
                        .child(i.text.clone()),
                )
                .child(delete)
                .test_support()
        });
        let progress = (all > 0).then(|| {
            div()
                .h(px(4.0))
                .mb(px(6.0))
                .rounded_full()
                .bg(ui.hover)
                .child(
                    div()
                        .h_full()
                        .w(relative(done as f32 / all as f32))
                        .rounded_full()
                        .bg(ui.accent),
                )
        });
        let thread = if comments.is_empty() {
            div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .py(px(14.0))
                .rounded(px(8.0))
                .border_1()
                .border_dashed()
                .border_color(ui.hairline)
                .text_size(px(12.0))
                .text_color(ui.title)
                .child(
                    Icon::new(IconName::MessageSquare)
                        .size(px(13.0))
                        .text_color(ui.title_dim),
                )
                .child("No comments yet")
        } else {
            div().flex().flex_col().gap(px(14.0)).children(comments)
        };
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(SIDE))
            .pt(px(16.0))
            .pb(px(14.0))
            .border_b_1()
            .border_color(ui.hairline)
            .child(
                div()
                    .id("card-number")
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(6.0))
                    .bg(ui.accent.opacity(0.16))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(ui.accent)
                    .child(Icon::new(IconName::Kanban).size(px(12.0)))
                    .child(format!("Card #{card}"))
                    .test_support(),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(ui.title)
                    .child(place),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(SIDE))
            .py(px(12.0))
            .border_t_1()
            .border_color(ui.hairline)
            .child(delete)
            .child(
                pill("card-done", "Done", None, ui.accent, ui.surface)
                    .on_click(|_, window, cx| window.close_dialog(cx))
                    .test_support(),
            );
        alert
            .title(header)
            .width(px(DIALOG_W))
            // A block's own color, not the canvas's: the fields and the faint fills read on it.
            .bg(ui.surface)
            .pt(px(0.0))
            .pb(px(0.0))
            .pl(px(0.0))
            .pr(px(0.0))
            .footer(footer)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(22.0))
                    .px(px(SIDE))
                    .pt(px(10.0))
                    .pb(px(14.0))
                    .child(
                        div()
                            .id("card-title")
                            .child(Styled::h(
                                Input::new(&title).text_size(px(15.0)).px(px(12.0)),
                                px(38.0),
                            ))
                            .test_support(),
                    )
                    .child(
                        div()
                            .id("card-properties")
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .px(px(12.0))
                            .py(px(6.0))
                            .rounded(px(8.0))
                            .bg(ui.card)
                            .child(
                                property(ui, IconName::Bot, "Assignee")
                                    .child(assignee_menu(
                                        &ws,
                                        board,
                                        card,
                                        shown.assignee.clone(),
                                        who,
                                    ))
                                    .children(unassign),
                            )
                            .child(
                                property(ui, IconName::SignalHigh, "Priority").child(
                                    div()
                                        .flex()
                                        .gap(px(2.0))
                                        .p(px(2.0))
                                        .rounded(px(8.0))
                                        .border_1()
                                        .border_color(ui.hairline)
                                        .children(chips),
                                ),
                            )
                            .child(
                                property(ui, IconName::Tag, "Labels")
                                    .child(div().flex().gap(px(8.0)).children(toggles)),
                            )
                            .test_support(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(section(ui, IconName::FileText, "Description", None))
                            .child(Textarea::new(&description)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(section(
                                ui,
                                IconName::ListChecks,
                                "Checklist",
                                (all > 0).then(|| format!("{done}/{all}")),
                            ))
                            .child(
                                div()
                                    .id("card-checklist")
                                    .flex()
                                    .flex_col()
                                    .children(progress)
                                    .children(items)
                                    .child(
                                        div()
                                            .id("card-item-field")
                                            .mt(px(6.0))
                                            // Enter adds the item: it mustn't confirm (close) the dialog.
                                            .on_action(|_: &Confirm, _, cx| cx.stop_propagation())
                                            .child(
                                                field(&item, 1.0)
                                                    .prefix(
                                                        Icon::new(IconName::Plus)
                                                            .size(px(13.0))
                                                            .text_color(ui.title_dim),
                                                    )
                                                    .on_paste(paste_items(ws.clone(), board, card)),
                                            )
                                            .test_support(),
                                    )
                                    .test_support(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .child(section(
                                ui,
                                IconName::MessageSquare,
                                "Comments",
                                (count > 0).then(|| count.to_string()),
                            ))
                            .child(thread)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .rounded(px(8.0))
                                    .border_1()
                                    .border_color(ui.hairline)
                                    .bg(ui.card)
                                    .child(
                                        div()
                                            .id("comment-field")
                                            .child(
                                                Textarea::new(&comment)
                                                    .appearance(false)
                                                    .bordered(false),
                                            )
                                            .test_support(),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .justify_end()
                                            .px(px(8.0))
                                            .pb(px(8.0))
                                            .child(add),
                                    ),
                            ),
                    ),
            )
    });
    // After opening: the dialog takes the focus when it opens.
    title_focus.update(cx, |input, cx| {
        input.focus(window, cx);
        if new {
            input.select_all(window, cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use templum_canvas::EDGE_SPEED;

    #[test]
    fn a_drag_scrolls_faster_near_an_edge_in_world_units_and_never_across_another_column() {
        let column = Rect {
            x: 100.0,
            y: 50.0,
            w: 260.0,
            h: 400.0,
        };
        let at = Point::new;
        assert_eq!(
            drag_speed(at(230.0, 250.0), column, true, 1.0),
            0.0,
            "the middle"
        );
        let near = drag_speed(at(230.0, 440.0), column, true, 1.0);
        assert!(near > 0.0 && near < EDGE_SPEED);
        assert!(
            drag_speed(at(230.0, 445.0), column, true, 1.0) > near,
            "faster nearer"
        );
        assert_eq!(
            drag_speed(at(230.0, 470.0), column, true, 1.0),
            EDGE_SPEED,
            "just past the edge"
        );
        // Every container hears a drag: one far past its edge (another block's) stays still.
        assert_eq!(
            drag_speed(at(230.0, 600.0), column, true, 1.0),
            0.0,
            "far past the edge"
        );
        assert_eq!(
            drag_speed(at(230.0, 520.0), column, true, 2.0),
            EDGE_SPEED,
            "the zone past it is in world units too"
        );
        assert_eq!(
            drag_speed(at(230.0, 60.0), column, true, 1.0),
            -EDGE_SPEED * 0.75,
            "up"
        );
        assert_eq!(
            drag_speed(at(500.0, 445.0), column, true, 1.0),
            0.0,
            "over another column"
        );
        assert!(
            drag_speed(at(355.0, 250.0), column, false, 1.0) > 0.0,
            "sideways, for the row"
        );
        // The zone is 40 world units: 60 px from the bottom is in it at zoom 2 only.
        let tall = Rect { h: 800.0, ..column };
        assert_eq!(drag_speed(at(230.0, 790.0), tall, true, 1.0), 0.0);
        assert!(drag_speed(at(230.0, 790.0), tall, true, 2.0) > 0.0);
        // A list shorter than both zones (a small block far away) stays still at its middle.
        let short = Rect { h: 60.0, ..column };
        assert_eq!(drag_speed(at(230.0, 80.0), short, true, 1.0), 0.0);
    }

    #[gpui_kit::test]
    fn a_drag_held_at_a_lists_end_or_ended_asks_for_no_more_frames(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            let edge = cx.new(|_| EdgeScroll {
                speed: 500.0,
                ..EdgeScroll::default()
            });
            let t0 = Instant::now();
            let later = t0 + std::time::Duration::from_millis(16);
            assert!(
                step(&edge, true, 1.0, true, t0, cx),
                "the first frame starts the clock"
            );
            // An unlaid list has nothing to scroll: it is at its end, and stops there.
            assert!(!step(&edge, true, 1.0, true, later, cx), "at the end");
            assert!(!step(&edge, true, 1.0, false, later, cx), "the drag ended");
            assert_eq!(edge.read(cx).speed, 0.0);
        });
    }
}
