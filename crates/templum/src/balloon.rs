//! The balloon on a terminal block whose agent asks the person something
//! (docs/specs/connections.md, Requests from agents): a question, with a button per option or a
//! field and Send, or a request to connect, with Allow and Deny. Several wait in turn: the oldest
//! shows, with "1 of 3".

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, Context, ElementId, TestSupportExt, Window, div, px};
use templum_canvas::{BlockId, chrome};

use crate::git_block::{self, widgets::small};
use crate::workspace::{Wait, Waiting, Workspace};

/// Its width, world units.
const WIDTH: f32 = 300.0;

/// The balloon of the terminal block `id`, whose terminal is `terminal`, at zoom `z`, over the
/// terminal's top right corner: None when its agent asks nothing.
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
    let text = match &wait.what {
        Waiting::Question { text, .. } => text.clone(),
        Waiting::Connect { .. } => ws.connect_line(wait, cx),
        Waiting::Turn { .. } => return None,
    };
    let choices = choices(ws, wait, n, z, cx);
    Some(
        div()
            .id(("balloon", n))
            .absolute()
            .top(px((chrome::TITLE_H + chrome::PAD) * z))
            .right(px(chrome::PAD * z))
            .w(px(WIDTH * z))
            .flex()
            .flex_col()
            .gap(px(8.0 * z))
            .p(px(10.0 * z))
            .rounded(px(8.0 * z))
            .border_1()
            .border_color(ui.accent)
            .bg(ui.surface)
            .text_size(px(git_block::FONT * z))
            .text_color(ui.title)
            // Its presses are its own: no selection starts in the terminal under it, a program
            // there that asked for the mouse hears none, the block's menu doesn't open, the
            // canvas doesn't pan. Not `block_mouse_except_scroll`: the block would lose its hover.
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .when(count > 1, |el| {
                el.child(
                    div()
                        .id(part(n, "count"))
                        .text_color(ui.title_dim)
                        .child(format!("1 of {count}"))
                        .test_support(),
                )
            })
            .child(div().child(text))
            .child(choices)
            .test_support()
            .into_any_element(),
    )
}

/// The id of the balloon's element `what` on block `n`: `option-<i>`, `send`, `allow`, `deny`,
/// `count`.
pub(crate) fn part(n: usize, what: &str) -> ElementId {
    ElementId::Name(format!("balloon-{n}-{what}").into())
}

/// What the person answers with: a button per option, the field and Send, or Allow and Deny.
fn choices(
    ws: &Workspace,
    wait: &Wait,
    n: usize,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (key, ui) = (wait.key, ws.ui);
    let row = div().flex().flex_wrap().items_center().gap(px(6.0 * z));
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
            row.children(options.iter().enumerate().map(|(i, option)| {
                let answer = option.clone();
                small(part(n, &format!("option-{i}")), option.clone(), ui, z)
                    .on_click(cx.listener(once(move |ws, window, cx| {
                        ws.answer_question(key, answer.clone(), window, cx)
                    })))
                    .test_support()
            }))
        }
        Waiting::Connect { .. } => row
            .child(
                small(part(n, "allow"), "Allow", ui, z)
                    .on_click(cx.listener(once(move |ws, _, cx| ws.answer_connect(key, true, cx))))
                    .test_support(),
            )
            .child(
                small(part(n, "deny"), "Deny", ui, z)
                    .on_click(cx.listener(once(move |ws, _, cx| ws.answer_connect(key, false, cx))))
                    .test_support(),
            ),
        // `balloon` never hands one over.
        Waiting::Turn { .. } => row,
    }
    .into_any_element()
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
