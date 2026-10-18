//! A kanban block's body: the board list, or a board's columns and cards with drag and drop;
//! and the dialog of an open card. Everything is drawn at the block's zoom `z`, like a
//! terminal's text. See docs/specs/kanban.md.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::Confirm;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::{Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, BoxShadow, ClickEvent, Context, ElementId, FontWeight, MouseButton,
    MouseDownEvent, Render, ScrollWheelEvent, SharedString, TestSupportExt, Window, div, point, px,
    relative,
};
use templum_canvas::{BlockId, Size};

use crate::board::{Assignee, Board, Boards, Card, Column, Comment, Label, Priority};
use crate::checklists;
use crate::grid::hsla;
use crate::menu::Target;
use crate::notes;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// A new kanban block's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size { w: 900.0, h: 540.0 };
pub(crate) const MIN: Size = Size { w: 420.0, h: 280.0 };
const COLUMN_W: f32 = 260.0;

/// A kanban block: the board it shows, if any, and whether it shows the board list instead.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Kanban {
    pub board: Option<u64>,
    pub picking: bool,
}

impl Kanban {
    /// The board on screen: none while picking, or when it was deleted.
    pub fn shown<'a>(&self, ws: &'a Workspace) -> Option<&'a Board> {
        self.board
            .filter(|_| !self.picking)
            .and_then(|id| ws.boards.board(id))
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

fn name(prefix: &str, n: usize, id: u64) -> ElementId {
    ElementId::Name(format!("{prefix}-{n}-{id}").into())
}

/// The body below the header: the board, or the board list.
pub(crate) fn body(
    ws: &Workspace,
    block: BlockId,
    kanban: Kanban,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let view = match kanban.shown(ws) {
        Some(board) => board_view(ws, block, board, z, cx),
        None => list_view(ws, block, z, cx),
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

fn list_view(ws: &Workspace, block: BlockId, z: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let ui = ws.ui;
    let n = block.0 as usize;
    let rows: Vec<AnyElement> = ws
        .boards
        .boards
        .iter()
        .map(|board| {
            let id = board.id;
            let cards = board.card_count();
            let row = div()
                .id(name("board-row", n, id))
                .flex()
                .items_center()
                .justify_between()
                .px(px(12.0 * z))
                .py(px(10.0 * z))
                .rounded(px(8.0 * z))
                .border_1()
                .border_color(ui.hairline)
                .bg(ui.surface)
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover).border_color(ui.hover_border))
                .child(
                    div()
                        .text_size(px(14.0 * z))
                        .font_weight(FontWeight::MEDIUM)
                        .child(board.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(12.0 * z))
                        .text_color(ui.title_dim)
                        .child(if cards == 1 {
                            "1 card".to_string()
                        } else {
                            format!("{cards} cards")
                        }),
                )
                .on_click(
                    cx.listener(move |ws, _: &ClickEvent, _, cx| ws.show_board(block, id, cx)),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        ws.open_menu(Target::Board { block, board: id }, e.position, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .test_support();
            row.into_any_element()
        })
        .collect();
    let new = div()
        .id(("new-board", n))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .px(px(12.0 * z))
        .py(px(10.0 * z))
        .rounded(px(8.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .text_size(px(13.0 * z))
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .active(move |s| s.bg(ui.pressed))
        .child(Icon::new(IconName::Plus).size(px(14.0 * z)))
        .child("New board")
        .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.new_board(block, cx)))
        .test_support();
    div()
        .id(("board-list", n))
        .size_full()
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0 * z))
                .p(px(12.0 * z))
                .child(
                    div()
                        .text_size(px(18.0 * z))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Boards"),
                )
                .child(
                    div()
                        .mb(px(6.0 * z))
                        .text_size(px(12.0 * z))
                        .text_color(ui.title_dim)
                        .child("Pick the board this block shows. Every block can show any board."),
                )
                .children(rows)
                .child(new),
        )
        .test_support()
        .into_any_element()
}

fn board_view(
    ws: &Workspace,
    block: BlockId,
    board: &Board,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = block.0 as usize;
    let board_id = board.id;
    let columns: Vec<AnyElement> = board
        .columns
        .iter()
        .map(|column| column_view(ws, n, board_id, column, z, cx))
        .collect();
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
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0 * z))
                        .px(px(6.0 * z))
                        .pb(px(6.0 * z))
                        .children(cards),
                ),
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
            .rows(5)
            .default_value(current.description.clone())
    });
    let comment = cx.new(|cx| TextareaState::new(window, cx).rows(2));
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
        let toggles = Label::ALL.map(|label| {
            let on = shown.labels.contains(&label);
            div()
                .id(ElementId::Name(format!("label-{}", label.name()).into()))
                .w(px(40.0))
                .h(px(22.0))
                .rounded(px(6.0))
                .bg(hsla(
                    palette.ansi[label.ansi()],
                    if on { 1.0 } else { 0.35 },
                ))
                .border_2()
                .border_color(if on {
                    hsla(palette.foreground, 0.8)
                } else {
                    gpui_kit::transparent_black()
                })
                .cursor_pointer()
                .on_click(edit(Rc::new(move |b| b.toggle_label(board, card, label))))
                .test_support()
        });
        let chips = std::iter::once(None)
            .chain(Priority::ALL.map(Some))
            .map(|p| {
                let on = shown.priority == p;
                let word = p.map_or("None", Priority::title);
                div()
                    .id(ElementId::Name(
                        format!("priority-{}", p.map_or("none", Priority::name)).into(),
                    ))
                    .h(px(26.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(6.0))
                    .border_1()
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .map(|el| {
                        if on {
                            el.bg(ui.hover).border_color(ui.accent).text_color(ui.title)
                        } else {
                            el.border_color(ui.hairline)
                                .text_color(ui.title_dim)
                                .hover(move |s| s.bg(ui.hover))
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
            } else {
                ws_now.uid_title(by, cx).unwrap_or_else(|| by.to_string())
            }
        };
        let comments: Vec<_> = shown
            .comments
            .iter()
            .map(|c| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(ui.title_dim)
                            .child(format!("{} · {}", author(&c.by), notes::ago(c.at, now))),
                    )
                    .child(div().text_size(px(13.0)).child(c.text.clone()))
            })
            .collect();
        let add = {
            let (ws, comment) = (ws.clone(), comment.clone());
            Button::new("add-comment")
                .small()
                .label("Add comment")
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
        };
        let delete = {
            let ws = ws.clone();
            let red = hsla(palette.ansi[1], 1.0);
            div()
                .id("delete-card")
                .h(px(28.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(px(6.0))
                .text_size(px(13.0))
                .text_color(red)
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
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
        let section = |label: &'static str| {
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(ui.title_dim)
                .child(label)
        };
        // A section's header: its label at the left, a count at the right.
        let header = |label: &'static str, count: Option<String>| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(section(label))
                .when_some(count, |el, count| {
                    el.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(ui.title_dim)
                            .child(count),
                    )
                })
        };
        let property = |label: &'static str| {
            div()
                .min_h(px(28.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .w(px(84.0))
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .text_color(ui.title_dim)
                        .child(label),
                )
        };
        let who = shown
            .assignee
            .as_ref()
            .map_or_else(|| "Anyone".to_string(), |a| assignee_label(ws_now, a, cx));
        let unassign = shown.assignee.is_some().then(|| {
            div()
                .id("unassign")
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child(
                    Icon::new(IconName::Close)
                        .size(px(12.0))
                        .text_color(ui.title_dim),
                )
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
            let check = div()
                .id(ElementId::Name(format!("card-item-box-{id}").into()))
                .flex_shrink_0()
                .mt(px((13.0 * 1.5 - 14.0) / 2.0))
                .size(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.0))
                .border_1()
                .border_color(if done { ui.accent } else { ui.title_dim })
                .when(done, |el| {
                    el.bg(ui.accent).child(
                        Icon::new(IconName::Check)
                            .size(px(10.0))
                            .text_color(ui.surface),
                    )
                })
                .cursor_pointer()
                .on_click(edit(Rc::new(move |b| {
                    b.set_item_done(board, card, id, !done);
                })))
                .test_support();
            let delete = div()
                .id(ElementId::Name(format!("card-item-delete-{id}").into()))
                .flex_shrink_0()
                .mt(px((13.0 * 1.5 - 16.0) / 2.0))
                .size(px(16.0))
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
                        .size(px(10.0))
                        .text_color(ui.title_dim),
                )
                .on_click(edit(Rc::new(move |b| b.remove_item(board, card, id))))
                .test_support();
            div()
                .id(ElementId::Name(group.clone()))
                .group(group)
                .flex()
                .items_start()
                .gap(px(8.0))
                .py(px(4.0))
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
                .mb(px(4.0))
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
        alert
            .title(
                div()
                    .id("card-number")
                    .child(format!("Card #{card}"))
                    .test_support(),
            )
            .description(place)
            .w(px(560.0))
            .footer(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(delete)
                    .child(
                        Button::new("card-done")
                            .primary()
                            .label("Done")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .id("card-title")
                            .child(Input::new(&title))
                            .test_support(),
                    )
                    .child(
                        div()
                            .id("card-properties")
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                property("Assignee")
                                    .child(div().text_size(px(13.0)).child(who))
                                    .children(unassign),
                            )
                            .child(
                                property("Priority")
                                    .child(div().flex().gap(px(6.0)).children(chips)),
                            )
                            .child(
                                property("Labels")
                                    .child(div().flex().gap(px(6.0)).children(toggles)),
                            )
                            .test_support(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(section("Description"))
                            .child(Textarea::new(&description)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(header(
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
                                            .mt(px(4.0))
                                            // Enter adds the item: it mustn't confirm (close) the dialog.
                                            .on_action(|_: &Confirm, _, cx| cx.stop_propagation())
                                            .child(Input::new(&item))
                                            .test_support(),
                                    )
                                    .test_support(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(header("Comments", (count > 0).then(|| count.to_string())))
                            .child(div().flex().flex_col().gap(px(10.0)).children(comments))
                            .child(
                                div()
                                    .id("comment-field")
                                    .child(Textarea::new(&comment))
                                    .test_support(),
                            )
                            .child(div().mt(px(2.0)).flex().justify_end().child(add)),
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
