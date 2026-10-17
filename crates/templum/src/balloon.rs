//! The balloon on a terminal block whose agent asks the person something
//! (docs/specs/connections.md, Requests from agents): a dialog centered on its terminal, with the
//! question and a button per option or a field and Send, or a request to connect, with Allow and
//! Deny. Several wait in turn: the oldest shows, with "1 of 3".

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, ElementId, MouseButton, MouseDownEvent,
    ScrollWheelEvent, SharedString, Stateful, TestSupportExt, Window, div, point, px, relative,
};
use templum_canvas::{BlockId, chrome};

use crate::git_block::{self, widgets::small};
use crate::grid::hsla;
use crate::theme::Ui;
use crate::workspace::{Wait, Waiting, Workspace};

/// Its width, and its height at most (it scrolls past that), of the terminal's.
const SHARE: f32 = 0.8;
/// Its text's size, world units: the question and the options.
const TEXT: f32 = 14.0;

/// The balloon of the terminal block `id`, whose terminal is `terminal`, at zoom `z`, centered on
/// the terminal: None when its agent asks nothing.
pub(crate) fn render(
    ws: &Workspace,
    id: BlockId,
    terminal: &str,
    z: f32,
    cx: &mut Context<Workspace>,
) -> Option<AnyElement> {
    let (wait, count) = ws.balloon(terminal)?;
    let n = id.0 as usize;
    let ui = ws.ui;
    // The yellow of the dot of a block waiting for its person.
    let waiting = hsla(ws.theme.palette.ansi[3], 1.0);
    let text = match &wait.what {
        Waiting::Question { text, .. } => text.clone(),
        Waiting::Connect { .. } => ws.connect_line(wait, cx),
        Waiting::Turn { .. } | Waiting::Event { .. } => return None,
    };
    let head = div()
        .id(part(n, "waiting"))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .text_size(px(git_block::FONT * z))
        .text_color(waiting)
        .child(
            div()
                .flex_shrink_0()
                .size(px(8.0 * z))
                .rounded(px(4.0 * z))
                .bg(waiting),
        )
        .child(div().flex_1().child("Waiting for your answer"))
        .when(count > 1, |el| {
            el.child(
                div()
                    .id(part(n, "count"))
                    .text_color(ui.title_dim)
                    .child(format!("1 of {count}"))
                    .test_support(),
            )
        })
        .test_support();
    let choices = choices(ws, wait, n, z, cx);
    // The terminal's area, under the title bar, centers it; it takes no press: the terminal
    // around the dialog still does.
    Some(
        div()
            .absolute()
            .top(px(chrome::TITLE_H * z))
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id(("balloon", n))
                    .w(relative(SHARE))
                    .max_h(relative(SHARE))
                    .overflow_y_scroll()
                    .rounded(px(8.0 * z))
                    .border_1()
                    .border_color(waiting)
                    .bg(ui.surface)
                    // Lifted off the terminal it covers, as a block off the canvas.
                    .shadow(vec![BoxShadow {
                        color: ui.shadow,
                        offset: point(px(0.0), px(8.0 * z)),
                        blur_radius: px(20.0 * z),
                        spread_radius: px(0.0),
                        inset: false,
                    }])
                    .text_size(px(TEXT * z))
                    // The terminal's own text color: the question reads as the agent's words.
                    .text_color(hsla(ws.theme.palette.foreground, 1.0))
                    // Its presses are its own: no selection starts in the terminal under it, a
                    // program there that asked for the mouse hears none, the block's menu doesn't
                    // open. The middle button still pans, as anywhere. Not
                    // `block_mouse_except_scroll`: the block would lose its hover.
                    .on_any_mouse_down(cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        if e.button == MouseButton::Middle {
                            ws.begin_pan(e.position);
                        }
                        cx.stop_propagation();
                    }))
                    // The wheel scrolls it, not the terminal under it; with Ctrl it zooms the
                    // canvas.
                    .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
                        if !e.modifiers.control {
                            cx.stop_propagation();
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0 * z))
                            .p(px(14.0 * z))
                            .child(head)
                            .child(div().child(text))
                            .child(choices),
                    )
                    .test_support(),
            )
            .into_any_element(),
    )
}

/// The id of the balloon's element `what` on block `n`: `option-<i>`, `send`, `allow`, `deny`,
/// `count`.
pub(crate) fn part(n: usize, what: &str) -> ElementId {
    ElementId::Name(format!("balloon-{n}-{what}").into())
}

/// What the person answers with: a button per option, each on its own lines and whole, the field
/// and Send, or Allow and Deny.
fn choices(
    ws: &Workspace,
    wait: &Wait,
    n: usize,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (key, ui) = (wait.key, ws.ui);
    // Its key in its buttons' ids: a press on a request withdrawn before its release is no click
    // on the next one's button drawn in its place.
    let row = div()
        .id(("balloon-wait", key))
        .flex()
        .items_center()
        .gap(px(6.0 * z));
    match &wait.what {
        Waiting::Question {
            field: Some((input, _)),
            ..
        } => row.child(git_block::field(input, z).flex_1()).child(
            small(part(n, "send"), "Send", ui, z)
                .on_click(cx.listener(once(move |ws, window, cx| ws.send_field(key, window, cx))))
                .test_support(),
        ),
        Waiting::Question { options, .. } => {
            row.flex_col()
                .items_start()
                .children(options.iter().enumerate().map(|(i, option)| {
                    let answer = option.clone();
                    button(part(n, &format!("option-{i}")), option.clone(), ui, z)
                        .w_full()
                        .on_click(cx.listener(once(move |ws, window, cx| {
                            ws.answer_question(key, answer.clone(), window, cx)
                        })))
                        .test_support()
                }))
        }
        Waiting::Connect { .. } => row
            .child(
                button(part(n, "allow"), "Allow", ui, z)
                    .on_click(cx.listener(once(move |ws, _, cx| ws.answer_connect(key, true, cx))))
                    .test_support(),
            )
            .child(
                button(part(n, "deny"), "Deny", ui, z)
                    .on_click(cx.listener(once(move |ws, _, cx| ws.answer_connect(key, false, cx))))
                    .test_support(),
            ),
        // `balloon` never hands one over.
        Waiting::Turn { .. } | Waiting::Event { .. } => row,
    }
    .into_any_element()
}

/// One of the dialog's answers: its `label` in a frame, wrapped on as many lines as it needs.
fn button(id: ElementId, label: impl Into<SharedString>, ui: Ui, z: f32) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(10.0 * z))
        .py(px(6.0 * z))
        .rounded(px(6.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(label.into())
}

/// A button's click, but not a double click's second: once a request is answered, the next one's
/// buttons are drawn under the pointer, and that click would choose for the person.
fn once(
    answer: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> impl Fn(&mut Workspace, &ClickEvent, &mut Window, &mut Context<Workspace>) + 'static {
    move |ws, event, window, cx| {
        if event.click_count() <= 1 {
            answer(ws, window, cx);
        }
    }
}
