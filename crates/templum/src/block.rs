//! One block, a continuous card: header (title, close button), body (a terminal's grid, or a
//! kanban board) and resize grip, in screen space. Every size is multiplied by the zoom, so text
//! is laid out at its real size. The close button and the grip show only while the pointer is
//! over the block.

use gpui_kit::assets::IconName;
use gpui_kit::base::animation::Lerp as _;
use gpui_kit::base::motion::{Transition, transition};
use gpui_kit::component::Icon;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, BoxShadow, ClickEvent, Context, ElementId, Hsla,
    MouseButton, MouseDownEvent, ScrollWheelEvent, SharedString, TestSupportExt, Window, div,
    point, px,
};
use templum_canvas::{Block, BlockId, Edges, Point, Rect, chrome};

use crate::alert::{self, Alert};
use crate::editor;
use crate::git_block;
use crate::grid::{self, hsla};
use crate::kanban;
use crate::menu::Target;
use crate::motion::{self, Look};
use crate::note;
use crate::workspace::{Body, FindBar, Pane, Workspace};

/// World units, like everything in a block.
const TITLE_FONT: f32 = 12.0;
const RADIUS: f32 = 10.0;
/// The focused block's ring, outside its border.
const RING: f32 = 2.0;
/// The card's border (`border_1`), screen pixels: not zoomed.
pub(crate) const BORDER: f32 = 1.0;
/// How thick the strips along a block's edges that resize it are, screen pixels, at most: thinner
/// when zoomed out, so the title stays easy to grab.
const EDGE: f32 = 6.0;

/// `look` scales the block around its center (it is drawn at zoom × scale) and fades it.
/// A block that isn't `live` is a ghost fading out: no controls, and clicks go through it.
pub(crate) fn render(
    ws: &Workspace,
    block: Block,
    body: &Body,
    look: Look,
    live: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let id = block.id;
    let n = id.0 as usize;
    let z = ws.space.view.zoom() * look.scale;
    // Bodies with gpui-kit editors glide too, but on the zooms where their rows are exact (see
    // `exact_rows`): their rows and their letters grow together. At rest, the zoom itself.
    let font = match body {
        Body::Editor(_) => editor::FONT,
        Body::Note(_) => note::FONT,
        Body::Git(_) => git_block::FONT,
        Body::Terminal(_) | Body::Kanban(_) => 0.0,
    };
    let settled = if font == 0.0 || (ws.space.flight.is_none() && look.scale == 1.0) {
        z
    } else {
        exact_rows(z, font)
    };
    let full = ws.space.view.rect_to_screen(block.rect);
    let r = Rect::new(
        full.x + full.w * (1.0 - look.scale) / 2.0,
        full.y + full.h * (1.0 - look.scale) / 2.0,
        full.w * look.scale,
        full.h * look.scale,
    );
    // Mid-glide, a text-heavy body is pulled toward the glide's pivot so the text there stays
    // under it, as a photo would: by how far its origin is from the pivot, times how much the
    // two zooms differ.
    let pull = match ws.space.flight {
        Some((flight, _)) if settled != z => {
            let origin = Point::new(
                r.x + BORDER + chrome::PAD * z,
                r.y + BORDER + (chrome::TITLE_H + chrome::PAD) * z,
            );
            let k = 1.0 - settled / z;
            Point::new(
                (flight.pivot.x - origin.x) * k,
                (flight.pivot.y - origin.y) * k,
            )
        }
        _ => Point::default(),
    };
    let focused = live && ws.space.focused == Some(id);
    let target = Target::Block(id);
    let hovered = live && ws.space.hovered == Some(id);
    let ui = ws.ui;
    // 0 at rest, 1 hovered; eased both ways.
    let lift = transition(
        ElementId::from(("lift", n)),
        if hovered { 1.0_f32 } else { 0.0 },
        Transition::new(motion::HOVER),
        window,
        cx,
    );
    let mut shadows = vec![shadow(ui.shadow, focused, lift, z)];
    if focused {
        shadows.push(BoxShadow {
            color: ui.ring,
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(RING * z),
            inset: false,
        });
    }

    let title = div()
        .id(("title", n))
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        // Clear of the close button on the right, and centered.
        .px(px(28.0 * z))
        .overflow_hidden()
        .child(
            div()
                .truncate()
                .text_size(px(TITLE_FONT * z))
                .text_color(if focused { ui.title } else { ui.title_dim })
                .child(match body {
                    Body::Terminal(pane) => SharedString::from(pane.term.title()),
                    Body::Kanban(k) => k.title(ws),
                    Body::Note(n) => n.title(cx),
                    Body::Editor(e) => e.title(),
                    Body::Git(g) => g.title(),
                }),
        )
        .when(live, |el| {
            el.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    // A double-click frames the block; the camera flies, so nothing moves.
                    if e.click_count == 2 {
                        ws.frame_block(id, window, cx);
                    } else {
                        ws.begin_move(id, e.position);
                    }
                }),
            )
        })
        .test_support();

    // Painted over the title: pressing it never starts a move.
    let close = div()
        .id(("close", n))
        .absolute()
        .top(px(4.0 * z))
        .right(px(4.0 * z))
        .flex()
        .items_center()
        .justify_center()
        .size(px(20.0 * z))
        .rounded(px(5.0 * z))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(
            Icon::new(IconName::Close)
                .size(px(12.0 * z))
                .text_color(ui.title),
        )
        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| ws.close(id, window, cx)))
        .test_support();

    // A terminal's state, on the left of its header.
    let dot = match body {
        Body::Terminal(pane) if live => status_dot(ws, pane).map(|(color, text)| {
            div()
                .id(("status", n))
                .absolute()
                .top(px((chrome::TITLE_H - 7.0) / 2.0 * z))
                .left(px(12.0 * z))
                .size(px(7.0 * z))
                .rounded_full()
                .bg(color)
                .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
                .test_support()
        }),
        _ => None,
    };

    // A kanban block's board list, on the left of its header (over the title: no move).
    let boards = match body {
        Body::Kanban(_) if live => Some(
            div()
                .id(("boards", n))
                .absolute()
                .top(px(4.0 * z))
                .left(px(4.0 * z))
                .flex()
                .items_center()
                .justify_center()
                .size(px(20.0 * z))
                .rounded(px(5.0 * z))
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child(
                    Icon::new(IconName::ChevronsUpDown)
                        .size(px(12.0 * z))
                        .text_color(ui.title),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.pick_board(id, cx)))
                .test_support(),
        ),
        _ => None,
    };
    // A note's preview toggle, on the left of its header (over the title: no move).
    let preview = match body {
        Body::Note(note) if live => {
            let on = note.preview;
            Some(
                div()
                    .id(("preview", n))
                    .absolute()
                    .top(px(4.0 * z))
                    .left(px(4.0 * z))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.0 * z))
                    .rounded(px(5.0 * z))
                    .cursor_pointer()
                    .hover(move |s| s.bg(ui.hover))
                    .child(
                        Icon::new(if on { IconName::EyeOff } else { IconName::Eye })
                            .size(px(12.0 * z))
                            .text_color(ui.title),
                    )
                    .tooltip(move |window, cx| {
                        Tooltip::new(if on { "Edit" } else { "Preview" }).build(window, cx)
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.toggle_preview(id, window, cx)
                    }))
                    .test_support(),
            )
        }
        _ => None,
    };
    // An editor's file-tree toggle, on the left of its header (over the title: no move).
    let tree_toggle = match body {
        Body::Editor(e) if live => {
            let shown = e.show_tree;
            Some(
                div()
                    .id(("tree-toggle", n))
                    .absolute()
                    .top(px(4.0 * z))
                    .left(px(4.0 * z))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.0 * z))
                    .rounded(px(5.0 * z))
                    .cursor_pointer()
                    .hover(move |s| s.bg(ui.hover))
                    .child(
                        Icon::new(if shown {
                            IconName::PanelLeftClose
                        } else {
                            IconName::PanelLeftOpen
                        })
                        .size(px(12.0 * z))
                        .text_color(ui.title),
                    )
                    .tooltip(move |window, cx| {
                        Tooltip::new(if shown {
                            "Hide the file tree"
                        } else {
                            "Show the file tree"
                        })
                        .build(window, cx)
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.toggle_tree(id, cx)))
                    .test_support(),
            )
        }
        _ => None,
    };
    // The resize strips, on the block under the pointer only (the pointer reaches an edge from
    // inside the block): the top ones under the header's buttons, the others over the body.
    let edge = (EDGE * z).clamp(2.0, EDGE);
    let (top_zones, other_zones) = if hovered {
        let mut zone = |edges| resize_zone(id, edges, edge, cx);
        (
            vec![
                zone(Edges::TOP),
                zone(Edges::TOP_LEFT),
                zone(Edges::TOP_RIGHT),
            ],
            vec![
                zone(Edges::LEFT),
                zone(Edges::RIGHT),
                zone(Edges::BOTTOM),
                zone(Edges::BOTTOM_LEFT),
                zone(Edges::BOTTOM_RIGHT),
            ],
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let header = div()
        .relative()
        .w_full()
        .h(px(chrome::TITLE_H * z))
        .child(title)
        .children(top_zones)
        .children(dot)
        .children(boards)
        .children(preview)
        .children(tree_toggle)
        .when(hovered, |el| el.child(fade_in(close, ("close-fade", n))));

    let frame = div()
        .id(("body", n))
        .absolute()
        .left(px(0.0))
        .top(px(chrome::TITLE_H * z))
        .w_full()
        .h(px((r.h - chrome::TITLE_H * z).max(0.0)))
        .p(px(chrome::PAD * z))
        .overflow_hidden();
    let content = match body {
        Body::Terminal(pane) => frame
            .when(live, |el| {
                el.cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            // Ctrl+click (Cmd on macOS) opens a path or a link; the block's own
                            // press would take the keyboard back from the editor it opened.
                            if e.modifiers.secondary() && ws.open_link(id, e, window, cx) {
                                cx.stop_propagation();
                            } else {
                                ws.begin_select(id, e);
                            }
                        }),
                    )
                    .on_scroll_wheel(cx.listener(move |ws, e: &ScrollWheelEvent, _, cx| {
                        ws.scroll_terminal(id, e, cx)
                    }))
            })
            .child(grid::body(
                pane.term.shared().clone(),
                pane.snapshot.clone(),
                ws.cell.clone(),
                z,
                focused,
                ws.marked.clone().filter(|_| focused),
                ws.theme.clone(),
                ws.blink_on,
                cx.entity(),
            ))
            .when_some(pane.find.as_ref().filter(|_| live), |el, bar| {
                el.child(find_bar(ws, n, bar, z))
            }),
        // A fading kanban block is drawn empty: its board is still live elsewhere.
        Body::Kanban(k) => frame.when(live, |el| el.child(kanban::body(ws, id, *k, z, cx))),
        // A fading note is drawn empty, like a kanban block.
        Body::Note(n) => frame.when(live, |el| {
            el.child(settled_box(
                block,
                settled,
                pull,
                note::body(ws, id, n, settled, cx),
            ))
        }),
        // A fading editor is drawn empty, like a note.
        Body::Editor(e) => frame.when(live, |el| {
            el.child(settled_box(
                block,
                settled,
                pull,
                editor::body(ws, id, e, settled, cx),
            ))
        }),
        // A fading Git block is drawn empty, like a note.
        Body::Git(g) => frame.when(live, |el| {
            el.child(settled_box(
                block,
                settled,
                pull,
                git_block::body(ws, id, g, settled, cx),
            ))
        }),
    }
    .test_support();

    let handle = div()
        .id(("handle", n))
        .absolute()
        .right(px(0.0))
        .bottom(px(0.0))
        .size(px(chrome::HANDLE * z))
        .flex()
        .items_center()
        .justify_center()
        .cursor_nwse_resize()
        .child(
            Icon::new(IconName::ResizeCorner)
                .size(px(10.0 * z))
                .text_color(ui.title_dim),
        )
        .on_mouse_down(
            MouseButton::Left,
            // The handle overlaps the body: stop here so the body doesn't start a selection.
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                ws.focus_block(id, window, cx);
                ws.begin_resize(id, Edges::BOTTOM_RIGHT, e.position);
                cx.stop_propagation();
            }),
        )
        .test_support();

    let root: ElementId = if live {
        ("block", n).into()
    } else {
        ("ghost", n).into()
    };
    div()
        .id(root)
        .absolute()
        .left(px(r.x))
        .top(px(r.y))
        .w(px(r.w))
        .h(px(r.h))
        .opacity(look.opacity)
        .rounded(px(RADIUS * z))
        // Always 1 px, so focusing never moves the grid.
        .border_1()
        .border_color(if focused {
            ui.accent
        } else {
            ui.hairline.lerp(&ui.hover_border, lift)
        })
        .bg(match body {
            Body::Note(note) => note
                .color
                .map_or(ws.ui.surface, |color| ws.theme.note_bg(color)),
            _ => ws.ui.surface,
        })
        .shadow(shadows)
        .overflow_hidden()
        .when(live, |el| {
            // Nothing behind a block reacts to its clicks; wheel events still bubble
            // (title bar pans).
            el.block_mouse_except_scroll()
                .on_hover(
                    cx.listener(move |ws, hovered: &bool, _, cx| ws.hover_block(id, *hovered, cx)),
                )
                // Blocking hides the workspace from pinch (it hit-tests like a click): zoom
                // here too.
                .on_pinch(cx.listener(|ws, e, window, cx| ws.on_pinch(e, window, cx)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        ws.focus_block(id, window, cx);
                        cx.stop_propagation();
                    }),
                )
                // The middle button moves the canvas, over blocks too.
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        ws.begin_pan(e.position);
                        cx.stop_propagation();
                    }),
                )
                // Its menu acts on it: it is focused first (copy, paste).
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        ws.focus_block(id, window, cx);
                        ws.open_menu(target.clone(), e.position, window, cx);
                        cx.stop_propagation();
                    }),
                )
        })
        .child(header)
        .child(content)
        .children(other_zones)
        .when(hovered, |el| el.child(fade_in(handle, ("handle-fade", n))))
        .test_support()
        .into_any_element()
}

/// A terminal's find field, over its top right corner; "No matches" beside it when so.
fn find_bar(ws: &Workspace, n: usize, bar: &FindBar, z: f32) -> AnyElement {
    div()
        .id(("find", n))
        .absolute()
        .top(px(chrome::PAD * z))
        .right(px(chrome::PAD * z))
        .w(px(260.0 * z))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .p(px(4.0 * z))
        .rounded(px(6.0 * z))
        .bg(ws.ui.surface)
        .border_1()
        .border_color(ws.ui.hairline)
        // Clicks here are the field's, not a selection's in the grid below.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(git_block::field(&bar.input, z).flex_1())
        .when(bar.missing, |el| {
            el.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(git_block::FONT * z))
                    .text_color(hsla(ws.theme.palette.ansi[1], 1.0))
                    .child("No matches"),
            )
        })
        .test_support()
        .into_any_element()
}

/// An invisible strip along a block's edge (`edge` thick), or a square at its corner, that resizes
/// it by those `edges`, like a window's border.
fn resize_zone(id: BlockId, edges: Edges, edge: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let corner = 2.0 * edge;
    let name = match (edges.top, edges.bottom, edges.left, edges.right) {
        (true, _, true, _) => "top-left",
        (true, _, _, true) => "top-right",
        (_, true, true, _) => "bottom-left",
        (_, true, _, true) => "bottom-right",
        (true, ..) => "top",
        (_, true, ..) => "bottom",
        (_, _, true, _) => "left",
        _ => "right",
    };
    let el = div()
        .id(ElementId::Name(format!("resize-{name}-{}", id.0).into()))
        .absolute();
    // Across: the edge's own side; along: clear of the corners' squares.
    let el = match (edges.left || edges.right, edges.top || edges.bottom) {
        (true, true) => el.size(px(corner)),
        (true, false) => el.w(px(edge)).top(px(corner)).bottom(px(corner)),
        _ => el.h(px(edge)).left(px(corner)).right(px(corner)),
    };
    let el = if edges.left {
        el.left_0()
    } else if edges.right {
        el.right_0()
    } else {
        el
    };
    let el = if edges.top {
        el.top_0()
    } else if edges.bottom {
        el.bottom_0()
    } else {
        el
    };
    let el = match name {
        "top-left" | "bottom-right" => el.cursor_nwse_resize(),
        "top-right" | "bottom-left" => el.cursor_nesw_resize(),
        "top" | "bottom" => el.cursor_ns_resize(),
        _ => el.cursor_ew_resize(),
    };
    el.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
            ws.focus_block(id, window, cx);
            ws.begin_resize(id, edges, e.position);
            cx.stop_propagation();
        }),
    )
    .test_support()
    .into_any_element()
}

/// The zoom nearest `z` at which gpui-kit's editor rows, 1.5 × their text size of `font × zoom`,
/// are whole pixels. GPUI rounds them to whole pixels (gpui-pre-0.3.8 src/style.rs:555,
/// gpui-component-0.7.1 src/input/editor.rs:145): at any other zoom the rows grow a pixel at a
/// time while the letters grow smoothly, and the text below the first rows jumps.
fn exact_rows(z: f32, font: f32) -> f32 {
    let row = 1.5 * font;
    (row * z).round().max(1.0) / row
}

/// A text-heavy body's box: its size at the zoom `settled` (see `render`), so its text and its
/// wrapping stay a photo of the block while a zoom glides, moved by `pull` so the text under the
/// glide's pivot stays there; the frame around it clips. At rest, exactly the frame's inner
/// area: inside the card's border (the frame's width is) and the padding.
fn settled_box(block: Block, settled: f32, pull: Point, body: AnyElement) -> impl IntoElement {
    let inner = chrome::content_rect(block.rect);
    div()
        .relative()
        .left(px(pull.x))
        .top(px(pull.y))
        .w(px((inner.w * settled - 2.0 * BORDER).max(0.0)))
        .h(px((inner.h * settled).max(0.0)))
        .child(body)
}

/// Hover-only controls fade in; they disappear at once. The wrapper covers its parent, since
/// absolute children are placed relative to their direct parent.
fn fade_in(el: impl IntoElement + 'static, id: impl Into<ElementId>) -> impl IntoElement {
    div().absolute().inset_0().child(el).with_animation(
        id,
        Animation::new(motion::HOVER),
        |el, t| el.opacity(t),
    )
}

/// The drop shadow's offset and blur (world units): the focused block floats higher, and a
/// hovered one (`lift` 1) rises a little more.
fn lift_metrics(focused: bool, lift: f32) -> (f32, f32) {
    let (y, blur) = if focused { (10.0, 30.0) } else { (4.0, 12.0) };
    (y + 3.0 * lift, blur + 8.0 * lift)
}

fn shadow(color: Hsla, focused: bool, lift: f32, z: f32) -> BoxShadow {
    let (y, blur) = lift_metrics(focused, lift);
    BoxShadow {
        color,
        offset: point(px(0.0), px(y * z)),
        blur_radius: px(blur * z),
        spread_radius: px(0.0),
        inset: false,
    }
}

/// The dot before a terminal's title, and what it says. News first: yellow when it asked for
/// attention, red or green when a long command ended. Then a command running, then a last command
/// that failed. Nothing when the shell doesn't say (no shell integration).
fn status_dot(ws: &Workspace, pane: &Pane) -> Option<(Hsla, SharedString)> {
    let ansi = |i: usize| hsla(ws.theme.palette.ansi[i], 1.0);
    let (color, text) = match &pane.alert {
        Some(a @ Alert::Attention(_)) => (ansi(3), a.text()),
        Some(a) if a.failed() => (ansi(1), a.text()),
        Some(a) => (ansi(2), a.text()),
        None => {
            let status = pane.term.status();
            match (status.running, status.last) {
                (Some(since), _) => (
                    ws.ui.accent,
                    format!("Running for {}", alert::took(since.elapsed())),
                ),
                (None, Some(last)) if last.code.is_some_and(|c| c != 0) => {
                    (ansi(1), alert::finished(&last))
                }
                _ => return None,
            }
        }
    };
    Some((color, text.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hovered_block_lifts() {
        assert_eq!(lift_metrics(false, 0.0), (4.0, 12.0));
        assert_eq!(lift_metrics(false, 1.0), (7.0, 20.0));
        assert_eq!(lift_metrics(true, 0.0), (10.0, 30.0));
        assert_eq!(lift_metrics(true, 1.0), (13.0, 38.0));
    }
}
