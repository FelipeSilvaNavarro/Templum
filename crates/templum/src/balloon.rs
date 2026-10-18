//! The balloon on a terminal block whose agent asks the person something
//! (docs/specs/connections.md, Requests from agents): a dialog centered on its terminal, with its
//! questions (docs/specs/ask-user.md, The dialog), or a request to connect, with Allow and Deny.
//! Several wait in turn: the oldest shows, with "1 of 3".

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, ElementId, MouseButton, MouseDownEvent,
    ScrollWheelEvent, SharedString, Stateful, TestSupportExt, Window, div, point, px, relative,
};
use templum_canvas::{BlockId, chrome};
use templum_mcp::ask::{Ask, Choice, Form, Row, clean_preview};

use crate::git_block::{self, widgets::small};
use crate::grid::hsla;
use crate::theme::Ui;
use crate::workspace::{Wait, Waiting, Workspace};

/// Its width, and its height at most (it scrolls past that), of the terminal's.
const SHARE: f32 = 0.8;
/// What a question whose client stopped waiting says under its first line.
const STOPPED: &str = "The agent stopped waiting: your answer will be typed into its terminal.";
/// Its text's size, world units: the questions and the options' labels.
const TEXT: f32 = 14.0;
/// The size of what is said about them, world units: descriptions, headers, "Pick any", the
/// preview.
const SMALL: f32 = 12.0;
/// A preview's line height, world units, and the most lines it shows (it scrolls past them).
const PREVIEW_LINE: f32 = 16.0;
const PREVIEW_LINES: f32 = 18.0;
/// The most of the balloon's height a plan's frame takes; it scrolls past that.
const PLAN_SHARE: f32 = 0.6;
/// The most characters of a plan's title shown.
const PLAN_TITLE: usize = 60;

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
    // Presses count from TOO_SOON after it first shows.
    if wait.shown.get().is_none() {
        wait.shown.set(Some(cx.background_executor().now()));
    }
    let key = wait.key;
    let n = id.0 as usize;
    let ui = ws.ui;
    // The yellow of the dot of a block waiting for its person.
    let waiting = hsla(ws.theme.palette.ansi[3], 1.0);
    let body = match &wait.what {
        Waiting::Question {
            ask, form, scroll, ..
        } if ask.plan.is_some() => plan(ws, wait, ask, form, scroll, id, z, cx),
        Waiting::Question { ask, form, .. } => question(ws, wait, ask, form, n, z, cx),
        Waiting::Connect { .. } => connect(ws, wait, n, z, cx),
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
                    .on_any_mouse_down(cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        if e.button == MouseButton::Middle {
                            ws.begin_pan(e.position);
                        } else {
                            ws.press_on_balloon(key, id, window, cx);
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
                            .when(
                                matches!(wait.what, Waiting::Question { stopped: true, .. }),
                                |el| {
                                    el.child(
                                        div()
                                            .id(part(n, "stopped"))
                                            .text_color(ui.title_dim)
                                            .child(STOPPED)
                                            .test_support(),
                                    )
                                },
                            )
                            .child(body),
                    )
                    .test_support(),
            )
            .into_any_element(),
    )
}

/// The id of the balloon's element `what` on block `n`: `q<q>-option-<i>`, `q<q>-other`, `note`,
/// `send`, `dismiss`, `allow`, `deny`, `count`, `waiting`.
pub(crate) fn part(n: usize, what: &str) -> ElementId {
    ElementId::Name(format!("balloon-{n}-{what}").into())
}

/// A question's dialog: each question with its header, text, options and Other field, a hairline
/// between them, the note, Dismiss and Send. Today's shape (`ask.json` false) draws today's: a
/// button per option, or the field and Send. Only the rows `Form::rows` lists are drawn.
fn question(
    ws: &Workspace,
    wait: &Wait,
    ask: &Ask,
    form: &Form,
    n: usize,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (key, ui) = (wait.key, ws.ui);
    let Waiting::Question { others, note, .. } = &wait.what else {
        return div().into_any_element();
    };
    let ready = ws
        .texts(wait, cx)
        .is_some_and(|(texts, _)| form.answers(ask, &texts).is_some());
    let send = small(part(n, "send"), "Send", ui, z)
        .when(!ready, |el| el.opacity(0.4))
        .when(form.highlight == Some(Row::Send), |el| el.bg(ui.hover))
        .on_hover(hover(key, Row::Send, cx))
        .on_click(cx.listener(once(move |ws, window, cx| {
            ws.send_question(key, window, cx)
        })))
        .test_support();
    let rows = Form::rows(ask);
    // Its key in its buttons' ids: a press on a request withdrawn before its release is no click
    // on the next one's button drawn in its place.
    let mut body = div()
        .id(("balloon-wait", key))
        .flex()
        .flex_col()
        .gap(px(10.0 * z));
    let mut send = Some(send);
    for (q, question) in ask.questions.iter().enumerate() {
        if q > 0 {
            body = body.child(div().h(px(1.0 * z)).bg(ui.hairline));
        }
        let title = div()
            .flex()
            .items_start()
            .gap(px(8.0 * z))
            .when_some(question.header_shown(), |el, header| {
                el.child(
                    div()
                        .flex_shrink_0()
                        .px(px(6.0 * z))
                        .rounded(px(4.0 * z))
                        .border_1()
                        .border_color(ui.title_dim)
                        .text_color(ui.title_dim)
                        .text_size(px(SMALL * z))
                        .child(header),
                )
            })
            .child(div().flex_1().child(question.text.clone()));
        let mut block =
            div()
                .flex()
                .flex_col()
                .gap(px(6.0 * z))
                .child(title)
                .when(question.multi, |el| {
                    el.child(
                        div()
                            .text_size(px(SMALL * z))
                            .text_color(ui.title_dim)
                            .child("Pick any"),
                    )
                });
        // Its options and Other; with a preview, a column beside it.
        let mut choices = div().flex().flex_col().gap(px(6.0 * z));
        for (i, choice) in question.options.iter().enumerate() {
            let id = part(n, &format!("q{q}-option-{i}"));
            let row = if ask.json {
                option(id, question.multi, form.picked(q, i), choice, ui, z)
            } else {
                button(id, choice.label.clone(), ui, z).w_full()
            }
            .when(form.highlight == Some(Row::Option(q, i)), |el| {
                el.bg(ui.hover)
            });
            choices = choices.child(
                row.on_click(cx.listener(once(move |ws, window, cx| {
                    ws.pick_option(key, q, i, window, cx)
                })))
                .on_hover(hover(key, Row::Option(q, i), cx))
                .test_support(),
            );
        }
        if rows.contains(&Row::Other(q)) {
            let input = &others[q].0;
            let mut other = div()
                .id(part(n, &format!("q{q}-other")))
                .flex()
                .items_center()
                .gap(px(8.0 * z))
                .w_full()
                .rounded(px(6.0 * z))
                .when(form.highlight == Some(Row::Other(q)), |el| el.bg(ui.hover));
            if !question.options.is_empty() {
                // A press on its mark or its name puts the keyboard in its field.
                let focus = input.clone();
                other = other.child(
                    div()
                        .id(part(n, &format!("q{q}-other-mark")))
                        .flex()
                        .items_center()
                        .gap(px(8.0 * z))
                        .px(px(6.0 * z))
                        .cursor_pointer()
                        .child(mark(question.multi, form.other_picked(q), ui, z))
                        .child("Other")
                        .on_click(move |_, window, cx| {
                            focus.update(cx, |input, cx| input.focus(window, cx))
                        }),
                );
            }
            other = other.child(git_block::field(input, z).flex_1());
            // Today's question without options: Send beside its field, as today.
            if !ask.json {
                other = other.children(send.take());
            }
            choices = choices.child(other.on_hover(hover(key, Row::Other(q), cx)).test_support());
        }
        block = block.child(match ask.preview_of(q, form) {
            None => choices,
            Some(shown) => div()
                .flex()
                .items_start()
                .gap(px(10.0 * z))
                .child(choices.w(relative(0.45)).flex_shrink_0())
                .child(preview(ws, shown, n, q, z)),
        });
        body = body.child(block);
    }
    if let Some((input, _)) = note {
        body = body.child(div().h(px(1.0 * z)).bg(ui.hairline)).child(
            div()
                .id(part(n, "note"))
                .w_full()
                .rounded(px(6.0 * z))
                .when(form.highlight == Some(Row::Note), |el| el.bg(ui.hover))
                .child(git_block::field(input, z).w_full())
                .on_hover(hover(key, Row::Note, cx))
                .test_support(),
        );
    }
    let dismiss = small(part(n, "dismiss"), "Dismiss", ui, z)
        .on_click(cx.listener(once(move |ws, window, cx| {
            ws.dismiss_question(key, window, cx)
        })))
        .test_support();
    let foot = div()
        .flex()
        .items_center()
        .justify_between()
        .child(dismiss)
        .when(ask.json, |el| el.children(send.take()));
    body.child(foot).into_any_element()
}

/// A plan's dialog (docs/specs/agent-tools.md, propose_plan): the chip "Plan" and its title, its
/// Markdown in a frame on the canvas color that scrolls on its own, at most PLAN_SHARE of the
/// balloon's height, the comment, then Reject on the left and Approve on the right.
#[allow(clippy::too_many_arguments)]
fn plan(
    ws: &Workspace,
    wait: &Wait,
    ask: &Ask,
    form: &Form,
    scroll: &crate::markdown::PreviewScroll,
    id: BlockId,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (key, ui, n) = (wait.key, ws.ui, id.0 as usize);
    let Some(shown) = &ask.plan else {
        return div().into_any_element();
    };
    let title: String = ask.summary().chars().take(PLAN_TITLE).collect();
    // The balloon's size, world units: SHARE of the terminal's area, less its padding.
    let rect = ws.space.canvas.get(id).map(|b| b.rect).unwrap_or_default();
    let (w, h) = (rect.w * SHARE - 30.0, (rect.h - chrome::TITLE_H) * SHARE);
    let head = div()
        .id(part(n, "plan-title"))
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .child(
            div()
                .flex_shrink_0()
                .px(px(6.0 * z))
                .rounded(px(4.0 * z))
                .border_1()
                .border_color(ui.title_dim)
                .text_color(ui.title_dim)
                .text_size(px(SMALL * z))
                .child("Plan"),
        )
        .child(div().flex_1().child(title))
        .test_support();
    let body = div()
        .flex()
        .flex_col()
        .max_h(px(h * PLAN_SHARE * z))
        .overflow_hidden()
        .rounded(px(6.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .bg(ui.canvas)
        .child(crate::markdown::preview(
            part(n, "plan-body"),
            ("plan-markdown", key).into(),
            shown.text.clone().into(),
            scroll,
            w,
            TEXT,
            z,
        ));
    let Waiting::Question { note, .. } = &wait.what else {
        return div().into_any_element();
    };
    let mut answer = |i: usize, what: &str, label: &'static str| {
        let row = Row::Option(0, i);
        button(part(n, what), label, ui, z)
            .when(form.highlight == Some(row), |el| el.bg(ui.hover))
            .on_hover(hover(key, row, cx))
            .on_click(cx.listener(once(move |ws, window, cx| {
                ws.pick_option(key, 0, i, window, cx)
            })))
            .test_support()
    };
    let foot = div()
        .flex()
        .items_center()
        .justify_between()
        .child(answer(1, "plan-reject", "Reject"))
        .child(
            answer(0, "plan-approve", "Approve")
                .border_color(ui.accent)
                .text_color(ui.accent),
        );
    div()
        .id(("balloon-wait", key))
        .flex()
        .flex_col()
        .gap(px(10.0 * z))
        .child(head)
        .child(body)
        .when_some(note.as_ref(), |el, (input, _)| {
            el.child(
                div()
                    .id(part(n, "note"))
                    .w_full()
                    .rounded(px(6.0 * z))
                    .when(form.highlight == Some(Row::Note), |el| el.bg(ui.hover))
                    .child(git_block::field(input, z).w_full())
                    .on_hover(hover(key, Row::Note, cx))
                    .test_support(),
            )
        })
        .child(foot)
        .into_any_element()
}

/// Hovering a row of the question `key` highlights it.
fn hover(
    key: u64,
    row: Row,
    cx: &mut Context<Workspace>,
) -> impl Fn(&bool, &mut Window, &mut gpui_kit::App) + 'static {
    cx.listener(move |ws, hovered: &bool, _, cx| {
        if *hovered {
            ws.highlight_row(key, row, cx);
        }
    })
}

/// Question `q`'s preview pane: `shown` in the terminal's font, never wrapped, at most
/// PREVIEW_LINES tall, scrolling both ways; None says "No preview".
fn preview(ws: &Workspace, shown: Option<&str>, n: usize, q: usize, z: f32) -> impl IntoElement {
    let ui = ws.ui;
    let pad = 8.0 * z;
    div()
        .id(part(n, &format!("q{q}-preview")))
        .flex_1()
        .min_w_0()
        .max_h(px(PREVIEW_LINES * PREVIEW_LINE * z + 2.0 * pad))
        .overflow_scroll()
        .p(px(pad))
        .rounded(px(6.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .bg(ui.canvas)
        .font_family(ws.cell.font.family.clone())
        .text_size(px(SMALL * z))
        .line_height(px(PREVIEW_LINE * z))
        .whitespace_nowrap()
        // The wheel scrolls it, not the dialog around it.
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        .child(match shown {
            Some(text) => div().child(clean_preview(text)),
            None => div().text_color(ui.title_dim).child("No preview"),
        })
        .test_support()
}

/// One of a question's options: its radio or checkbox, its label, and its description under it.
fn option(
    id: ElementId,
    multi: bool,
    picked: bool,
    choice: &Choice,
    ui: Ui,
    z: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_start()
        .gap(px(8.0 * z))
        .w_full()
        .px(px(6.0 * z))
        .py(px(3.0 * z))
        .rounded(px(6.0 * z))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        // On the label's first line.
        .child(div().pt(px(3.0 * z)).child(mark(multi, picked, ui, z)))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .child(choice.label.clone())
                .when_some(choice.description.clone(), |el, description| {
                    el.child(
                        div()
                            .text_size(px(SMALL * z))
                            .text_color(ui.title_dim)
                            .child(description),
                    )
                }),
        )
}

/// A radio (a circle) or a checkbox (a rounded square), filled with the accent when picked.
fn mark(multi: bool, picked: bool, ui: Ui, z: f32) -> Div {
    let size = 12.0 * z;
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded(px(if multi { 3.0 * z } else { size / 2.0 }))
        .border_1()
        .border_color(if picked { ui.accent } else { ui.title_dim })
        .when(picked, |el| el.bg(ui.accent))
}

/// A request to connect: what it asks, Allow and Deny.
fn connect(
    ws: &Workspace,
    wait: &Wait,
    n: usize,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (key, ui) = (wait.key, ws.ui);
    let buttons = div()
        .id(("balloon-wait", key))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .child(
            button(part(n, "allow"), "Allow", ui, z)
                .on_click(cx.listener(once(move |ws, _, cx| ws.answer_connect(key, true, cx))))
                .test_support(),
        )
        .child(
            button(part(n, "deny"), "Deny", ui, z)
                .on_click(cx.listener(once(move |ws, _, cx| ws.answer_connect(key, false, cx))))
                .test_support(),
        );
    div()
        .flex()
        .flex_col()
        .gap(px(10.0 * z))
        .child(div().child(ws.connect_line(wait, cx)))
        .child(buttons)
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
