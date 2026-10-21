//! The balloon on a terminal block whose agent asks the person something
//! (docs/specs/connections.md, Requests from agents): a dialog centered on its terminal, with its
//! questions (docs/specs/ask-user.md, The dialog), a plan to approve, or a request with a yes and
//! a no. Several wait in turn: the oldest shows, with "1 of 3". Every kind has the same frame, in
//! the language of the kanban card's dialog and the Git popup: a header and a foot that stay, and
//! a body between them that scrolls.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, ElementId, FontWeight, Hsla, MouseButton,
    MouseDownEvent, ScrollWheelEvent, SharedString, Stateful, TestSupportExt, Window, div, point,
    px, relative,
};
use templum_canvas::{BlockId, chrome};
use templum_mcp::ask::{Ask, Choice, Form, Row, clean_preview};

use crate::git_block::{self, FONT};
use crate::grid::hsla;
use crate::kanban::scrollbar_over;
use crate::theme::Ui;
use crate::workspace::{Wait, Waiting, Workspace};

/// Its width, and its height at most (its body scrolls past that), of the terminal's.
const SHARE: f32 = 0.8;
/// What a question whose client stopped waiting says under the header.
const STOPPED: &str = "The agent stopped waiting: your answer will be typed into its terminal.";
/// Its text's size, world units: the questions.
const TEXT: f32 = 14.5;
/// An option's label.
const LABEL: f32 = 13.5;
/// The size of what is said about them, world units: descriptions, hints, the preview.
const SMALL: f32 = 12.0;
/// The header's and the foot's sides, and the body's.
const SIDE: f32 = 18.0;
/// A preview's line height, world units, and the most lines it shows (it scrolls past them).
const PREVIEW_LINE: f32 = 16.0;
const PREVIEW_LINES: f32 = 18.0;
/// A dialog narrower than this, world units, lays a preview under the options instead of beside
/// them.
const BESIDE: f32 = 520.0;
/// The most of the balloon's height a plan's frame takes; it scrolls past that.
const PLAN_SHARE: f32 = 0.6;
/// The most characters of a plan's title shown.
const PLAN_TITLE: usize = 60;

/// What a kind of request puts in the frame: a line under the title, the body (which scrolls)
/// and the foot (which stays).
struct Parts {
    sub: Option<String>,
    body: Div,
    foot: Div,
}

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
    // The balloon's width, world units: SHARE of the terminal's.
    let width = ws.space.canvas.get(id).map_or(0.0, |b| b.rect.w) * SHARE;
    let Parts { sub, body, foot } = match &wait.what {
        Waiting::Question {
            ask, form, scroll, ..
        } if ask.plan.is_some() => plan(ws, wait, ask, form, scroll, id, z, cx),
        Waiting::Question { ask, form, .. } => question(ws, wait, ask, form, n, width, z, cx),
        Waiting::Allow { then } => allow(ws, wait, then.buttons(), n, z, cx),
        Waiting::Turn { .. } | Waiting::Event { .. } => return None,
    };
    let tile = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .size(px(30.0 * z))
        .rounded(px(8.0 * z))
        .bg(waiting.opacity(0.16))
        .child(
            Icon::new(IconName::CircleQuestionMark)
                .size(px(16.0 * z))
                .text_color(waiting),
        );
    let head = div()
        .id(part(n, "waiting"))
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(10.0 * z))
        .px(px(SIDE * z))
        .py(px(10.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .child(tile)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(8.0 * z))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(13.5 * z))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Waiting for your answer"),
                )
                .when_some(sub, |el, sub| {
                    el.child(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(SMALL * z))
                            .text_color(ui.title_dim)
                            .child(sub),
                    )
                }),
        )
        .when(count > 1, |el| {
            el.child(
                div()
                    .id(part(n, "count"))
                    .flex_shrink_0()
                    .px(px(8.0 * z))
                    .py(px(2.0 * z))
                    .rounded(px(6.0 * z))
                    .bg(ui.hover)
                    .text_size(px(11.0 * z))
                    .text_color(ui.title_dim)
                    .child(format!("1 of {count}"))
                    .test_support(),
            )
        })
        .test_support();
    // Its client gave up on the call: said where it stays in sight.
    let stopped = matches!(wait.what, Waiting::Question { stopped: true, .. }).then(|| {
        div()
            .id(part(n, "stopped"))
            .flex()
            .items_start()
            .flex_shrink_0()
            .gap(px(8.0 * z))
            .mx(px(SIDE * z))
            .mt(px(12.0 * z))
            .px(px(10.0 * z))
            .py(px(8.0 * z))
            .rounded(px(8.0 * z))
            .bg(waiting.opacity(0.12))
            .text_size(px(SMALL * z))
            .child(
                div().mt(px(1.0 * z)).child(
                    Icon::new(IconName::CircleAlert)
                        .size(px(14.0 * z))
                        .text_color(waiting),
                ),
            )
            .child(div().flex_1().min_w(px(0.0)).child(STOPPED))
            .test_support()
    });
    // Its key in the body's and the foot's ids: a press on a request withdrawn before its release
    // is no click on the next one's button drawn in its place.
    let body = div()
        .id(part(n, "body"))
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .track_scroll(&wait.body.handle)
        .child(
            div()
                .id(("balloon-wait", key))
                .flex()
                .flex_col()
                .gap(px(10.0 * z))
                .px(px(SIDE * z))
                .py(px(16.0 * z))
                .child(body)
                .test_support(),
        )
        .test_support();
    wait.body.follow(z);
    // The scrollbar goes over the body, not in it.
    let scrolling = div()
        .relative()
        .flex()
        .flex_col()
        .min_h(px(0.0))
        .child(body)
        .child(scrollbar_over(&wait.body.handle, part(n, "scrollbar"), z));
    let foot = div()
        .id(("balloon-foot", key))
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(8.0 * z))
        .px(px(SIDE * z))
        .py(px(10.0 * z))
        .border_t_1()
        .border_color(ui.hairline)
        .child(foot);
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
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(12.0 * z))
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
                    .text_size(px(LABEL * z))
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
                    .child(head)
                    .children(stopped)
                    .child(scrolling)
                    .child(foot)
                    .test_support(),
            )
            .into_any_element(),
    )
}

/// The id of the balloon's element `what` on block `n`: `q<q>-option-<i>`, `q<q>-other`, `note`,
/// `send`, `dismiss`, `progress`, `allow`, `deny`, `count`, `waiting`, `stopped`, `body`.
pub(crate) fn part(n: usize, what: &str) -> ElementId {
    ElementId::Name(format!("balloon-{n}-{what}").into())
}

/// A question's dialog, `w` world units wide: each question with its header, text, options and
/// Other field, a hairline between them, the note; Dismiss and Send in the foot. Today's shape
/// (`ask.json` false) draws today's: a button per option, or the field. A click on an option only
/// picks it; Send, lit once every question has an answer, answers. Only the rows `Form::rows`
/// lists are drawn.
#[allow(clippy::too_many_arguments)]
fn question(
    ws: &Workspace,
    wait: &Wait,
    ask: &Ask,
    form: &Form,
    n: usize,
    w: f32,
    z: f32,
    cx: &mut Context<Workspace>,
) -> Parts {
    let (key, ui) = (wait.key, ws.ui);
    let Waiting::Question { others, note, .. } = &wait.what else {
        return Parts {
            sub: None,
            body: div(),
            foot: div(),
        };
    };
    let texts = ws.texts(wait, cx).map(|(texts, _)| texts);
    let ready = texts
        .as_ref()
        .is_some_and(|texts| form.answers(ask, texts).is_some());
    let answered = texts.map_or(0, |texts| form.answered(ask, &texts));
    let rows = Form::rows(ask);
    let mut body = div().flex().flex_col().gap(px(18.0 * z));
    for (q, question) in ask.questions.iter().enumerate() {
        if q > 0 {
            body = body.child(div().h(px(1.0 * z)).bg(ui.hairline));
        }
        let title = div()
            .flex()
            .flex_col()
            .gap(px(8.0 * z))
            .when_some(question.header_shown(), |el, header| {
                el.child(div().flex().child(chip(header, ui, z)))
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(TEXT * z))
                    .font_weight(FontWeight::MEDIUM)
                    .line_height(relative(1.4))
                    .child(question.text.clone()),
            )
            .when(question.multi, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0 * z))
                        .text_size(px(SMALL * z))
                        .text_color(ui.title_dim)
                        .child(
                            Icon::new(IconName::ListChecks)
                                .size(px(13.0 * z))
                                .text_color(ui.title_dim),
                        )
                        .child("Pick any"),
                )
            });
        let mut block = div().flex().flex_col().gap(px(10.0 * z)).child(title);
        // Its options and Other, in a faint panel; with a preview, a column beside it (or under
        // it, in a narrow dialog).
        let mut choices = div().flex().flex_col().gap(px(2.0 * z));
        for (i, choice) in question.options.iter().enumerate() {
            let id = part(n, &format!("q{q}-option-{i}"));
            let (picked, lit) = (form.picked(q, i), form.highlight == Some(Row::Option(q, i)));
            // Today's shape: a button, no radio.
            let mark = ask.json.then_some(question.multi);
            choices = choices.child(
                option(id, mark, picked, lit, choice, ui, z)
                    .on_click(cx.listener(once(move |ws, window, cx| {
                        ws.pick_option(key, q, i, window, cx)
                    })))
                    .on_hover(hover(key, Row::Option(q, i), cx))
                    .test_support(),
            );
        }
        if rows.contains(&Row::Other(q)) {
            let input = &others[q].0;
            let lit = form.highlight == Some(Row::Other(q));
            let mut other = div()
                .id(part(n, &format!("q{q}-other")))
                .flex()
                .items_center()
                .gap(px(8.0 * z))
                .w_full()
                .rounded(px(8.0 * z))
                .when(!question.options.is_empty(), |el| {
                    let (fill, _) = tint(form.other_picked(q), lit, ui);
                    el.px(px(10.0 * z))
                        .py(px(6.0 * z))
                        .when_some(fill, |el, fill| el.bg(fill))
                })
                .when(question.options.is_empty() && lit, |el| el.bg(ui.hover));
            if !question.options.is_empty() {
                // A press on its mark or its name puts the keyboard in its field.
                let focus = input.clone();
                other = other.child(
                    div()
                        .id(part(n, &format!("q{q}-other-mark")))
                        .flex()
                        .items_center()
                        .gap(px(10.0 * z))
                        .cursor_pointer()
                        .child(mark(question.multi, form.other_picked(q), ui, z))
                        .child(div().text_size(px(LABEL * z)).child("Other"))
                        .on_click(move |_, window, cx| {
                            focus.update(cx, |input, cx| input.focus(window, cx))
                        }),
                );
            }
            other = other.child(git_block::field(input, z).flex_1().min_w(px(0.0)));
            choices = choices.child(other.on_hover(hover(key, Row::Other(q), cx)).test_support());
        }
        let panel = |choices: Div| {
            if question.options.is_empty() {
                choices
            } else {
                choices.p(px(4.0 * z)).rounded(px(10.0 * z)).bg(ui.card)
            }
        };
        block = block.child(match ask.preview_of(q, form) {
            None => panel(choices),
            Some(shown) => {
                let preview = preview(ws, shown, n, q, z);
                if w >= BESIDE {
                    div()
                        .flex()
                        .items_start()
                        .gap(px(12.0 * z))
                        .child(panel(choices).w(relative(0.45)).flex_shrink_0())
                        .child(preview)
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.0 * z))
                        .child(panel(choices))
                        .child(preview)
                }
            }
        });
        body = body.child(block);
    }
    if let Some((input, _)) = note {
        body = body.child(div().h(px(1.0 * z)).bg(ui.hairline)).child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0 * z))
                .child(section(ui, IconName::StickyNote, "Note", "optional", z))
                .child(
                    div()
                        .id(part(n, "note"))
                        .w_full()
                        .rounded(px(8.0 * z))
                        .when(form.highlight == Some(Row::Note), |el| el.bg(ui.hover))
                        .child(git_block::field(input, z).w_full())
                        .on_hover(hover(key, Row::Note, cx))
                        .test_support(),
                ),
        );
    }
    let many = ask.json && ask.questions.len() > 1;
    let total = ask.questions.len();
    let foot = div()
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .w_full()
        .child(
            ghost(part(n, "dismiss"), "Dismiss", ui, z)
                .on_click(cx.listener(once(move |ws, window, cx| {
                    ws.dismiss_question(key, window, cx)
                })))
                .test_support(),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .justify_end()
                .when(many, |el| {
                    el.child(
                        div()
                            .id(part(n, "progress"))
                            .truncate()
                            .text_size(px(SMALL * z))
                            .text_color(ui.title_dim)
                            .child(format!("{answered} of {total} answered"))
                            .test_support(),
                    )
                }),
        )
        .child(
            send_button(
                part(n, "send"),
                ready,
                form.highlight == Some(Row::Send),
                ui,
                z,
            )
            .on_hover(hover(key, Row::Send, cx))
            .on_click(cx.listener(once(move |ws, window, cx| {
                ws.send_question(key, window, cx)
            })))
            .test_support(),
        );
    let sub = match ask.questions.len() {
        1 => "1 question".to_string(),
        n => format!("{n} questions"),
    };
    Parts {
        sub: Some(sub),
        body,
        foot,
    }
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
) -> Parts {
    let (key, ui, n) = (wait.key, ws.ui, id.0 as usize);
    let (Some(shown), Waiting::Question { note, .. }) = (&ask.plan, &wait.what) else {
        return Parts {
            sub: None,
            body: div(),
            foot: div(),
        };
    };
    let title: String = ask.summary().chars().take(PLAN_TITLE).collect();
    // The balloon's size, world units: SHARE of the terminal's area, less the body's padding.
    let rect = ws.space.canvas.get(id).map(|b| b.rect).unwrap_or_default();
    let (w, h) = (
        rect.w * SHARE - 2.0 * SIDE - 2.0,
        (rect.h - chrome::TITLE_H) * SHARE,
    );
    let head = div()
        .id(part(n, "plan-title"))
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .child(chip("Plan", ui, z))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .test_support();
    let frame = div()
        .flex()
        .flex_col()
        .max_h(px(h * PLAN_SHARE * z))
        .overflow_hidden()
        .rounded(px(8.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .bg(ui.canvas)
        .child(crate::markdown::preview(
            part(n, "plan-body"),
            ("plan-markdown", key).into(),
            shown.text.clone().into(),
            scroll,
            w,
            LABEL,
            z,
        ));
    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0 * z))
        .child(head)
        .child(frame)
        .when_some(note.as_ref(), |el, (input, _)| {
            el.child(
                div()
                    .id(part(n, "note"))
                    .w_full()
                    .rounded(px(8.0 * z))
                    .when(form.highlight == Some(Row::Note), |el| el.bg(ui.hover))
                    .child(git_block::field(input, z).w_full())
                    .on_hover(hover(key, Row::Note, cx))
                    .test_support(),
            )
        });
    let row = |i: usize| Row::Option(0, i);
    let reject = ghost(part(n, "plan-reject"), "Reject", ui, z)
        .when(form.highlight == Some(row(1)), |el| el.bg(ui.hover))
        .on_hover(hover(key, row(1), cx))
        .on_click(cx.listener(once(move |ws, window, cx| {
            ws.pick_option(key, 0, 1, window, cx)
        })))
        .test_support();
    let approve = solid(
        part(n, "plan-approve"),
        "Approve",
        Some(IconName::Check),
        ui,
        z,
    )
    .when(form.highlight == Some(row(0)), |el| {
        el.bg(ui.accent.opacity(0.85))
    })
    .on_hover(hover(key, row(0), cx))
    .on_click(cx.listener(once(move |ws, window, cx| {
        ws.pick_option(key, 0, 0, window, cx)
    })))
    .test_support();
    Parts {
        sub: Some("Plan to approve".into()),
        body,
        foot: div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .child(reject)
            .child(approve),
    }
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

/// Question `q`'s preview: a caption over `shown` in the terminal's font, never wrapped, in a
/// frame at most PREVIEW_LINES tall, scrolling both ways; None says "No preview".
fn preview(ws: &Workspace, shown: Option<&str>, n: usize, q: usize, z: f32) -> Div {
    let ui = ws.ui;
    let pad = 8.0 * z;
    // A short preview has nothing to scroll up and down: the wheel goes on to the dialog's body.
    let tall = shown.is_some_and(|text| text.lines().count() > PREVIEW_LINES as usize);
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(6.0 * z))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(5.0 * z))
                .text_size(px(11.0 * z))
                .text_color(ui.title_dim)
                .child(
                    Icon::new(IconName::Eye)
                        .size(px(12.0 * z))
                        .text_color(ui.title_dim),
                )
                .child("Preview"),
        )
        .child(
            div()
                .id(part(n, &format!("q{q}-preview")))
                .w_full()
                .max_h(px(PREVIEW_LINES * PREVIEW_LINE * z + 2.0 * pad))
                .overflow_scroll()
                .p(px(pad))
                .rounded(px(8.0 * z))
                .border_1()
                .border_color(ui.hairline)
                .bg(ui.canvas)
                .font_family(ws.cell.font.family.clone())
                .text_size(px(SMALL * z))
                .line_height(px(PREVIEW_LINE * z))
                .whitespace_nowrap()
                // The wheel scrolls it, not the dialog around it.
                .on_scroll_wheel(move |e: &ScrollWheelEvent, _, cx| {
                    let sideways = e.delta.pixel_delta(px(1.0)).x.abs() > px(0.0);
                    if !e.modifiers.control && (tall || sideways) {
                        cx.stop_propagation();
                    }
                })
                .child(match shown {
                    Some(text) => div().child(clean_preview(text)),
                    None => div().text_color(ui.title_dim).child("No preview"),
                })
                .test_support(),
        )
}

/// One of a question's options: its radio (`mark` false) or checkbox (`mark` true) and its label,
/// and its description under it; with no mark, a button.
fn option(
    id: ElementId,
    mark: Option<bool>,
    picked: bool,
    lit: bool,
    choice: &Choice,
    ui: Ui,
    z: f32,
) -> Stateful<Div> {
    let (fill, over) = tint(picked, lit, ui);
    let multi = mark;
    div()
        .id(id)
        .flex()
        .items_start()
        .gap(px(10.0 * z))
        .w_full()
        .px(px(10.0 * z))
        .py(px(8.0 * z))
        .rounded(px(8.0 * z))
        .cursor_pointer()
        .hover(move |s| s.bg(over))
        .when_some(fill, |el, fill| el.bg(fill))
        // On the label's first line.
        .when_some(multi, |el, multi| {
            el.child(
                div()
                    .mt(px(1.5 * z))
                    .child(self::mark(multi, picked, ui, z)),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0 * z))
                .child(
                    div()
                        .text_size(px(LABEL * z))
                        .line_height(relative(1.4))
                        .when(picked && mark.is_none(), |el| el.text_color(ui.accent))
                        .child(choice.label.clone()),
                )
                .when_some(choice.description.clone(), |el, description| {
                    el.child(
                        div()
                            .text_size(px(SMALL * z))
                            .line_height(relative(1.45))
                            .text_color(ui.title_dim)
                            .child(description),
                    )
                }),
        )
}

/// A row's fill, and its fill under the pointer: a picked one is tinted with the accent (more
/// under the pointer or the keys' highlight), one only highlighted takes the hover color.
fn tint(picked: bool, lit: bool, ui: Ui) -> (Option<Hsla>, Hsla) {
    if picked {
        let over = ui.accent.opacity(0.22);
        (Some(if lit { over } else { ui.accent.opacity(0.14) }), over)
    } else {
        (lit.then_some(ui.hover), ui.hover)
    }
}

/// A header of the dialog's chips: a faint accent badge, as the kanban card's number.
fn chip(label: impl Into<SharedString>, ui: Ui, z: f32) -> Div {
    div()
        .flex_shrink_0()
        .px(px(8.0 * z))
        .py(px(2.0 * z))
        .rounded(px(6.0 * z))
        .bg(ui.accent.opacity(0.16))
        .text_size(px(11.5 * z))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(ui.accent)
        .child(label.into())
}

/// A section's header, as the kanban card's: its icon, its name and, at the right, `aside`.
fn section(ui: Ui, icon: IconName, label: &'static str, aside: &'static str, z: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .child(Icon::new(icon).size(px(13.0 * z)).text_color(ui.title_dim))
        .child(
            div()
                .text_size(px(SMALL * z))
                .font_weight(FontWeight::MEDIUM)
                .text_color(ui.title)
                .child(label),
        )
        .child(
            div()
                .ml_auto()
                .text_size(px(11.0 * z))
                .text_color(ui.title_dim)
                .child(aside),
        )
}

/// A foot's quiet button (Dismiss, Reject, Deny): no fill, the hover color under the pointer.
fn ghost(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    ui: Ui,
    z: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.0 * z))
        .px(px(12.0 * z))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0 * z))
        .text_size(px(FONT * z + 0.5 * z))
        .font_weight(FontWeight::MEDIUM)
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(label.into())
}

/// A foot's solid button (Send, Approve, Allow): the accent, the surface's color on it.
fn solid(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: Option<IconName>,
    ui: Ui,
    z: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.0 * z))
        .px(px(14.0 * z))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0 * z))
        .rounded(px(8.0 * z))
        .bg(ui.accent)
        .text_size(px(FONT * z + 0.5 * z))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(ui.surface)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.accent.opacity(0.85)))
        .active(move |s| s.bg(ui.accent.opacity(0.7)))
        .when_some(icon, |el, icon| {
            el.child(Icon::new(icon).size(px(14.0 * z)).text_color(ui.surface))
        })
        .child(label.into())
}

/// Send: solid accent once every question has an answer, a faint fill and inert before, as the
/// kanban's Comment; `lit` while the keys' highlight is on it.
fn send_button(id: ElementId, ready: bool, lit: bool, ui: Ui, z: f32) -> Stateful<Div> {
    if ready {
        solid(id, "Send", Some(IconName::ArrowUp), ui, z)
            .when(lit, |el| el.bg(ui.accent.opacity(0.85)))
    } else {
        div()
            .id(id)
            .h(px(30.0 * z))
            .px(px(14.0 * z))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0 * z))
            .rounded(px(8.0 * z))
            .bg(ui.hover)
            .text_size(px(FONT * z + 0.5 * z))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(ui.title_dim)
            .child(
                Icon::new(IconName::ArrowUp)
                    .size(px(14.0 * z))
                    .text_color(ui.title_dim),
            )
            .child("Send")
    }
}

/// A radio (a circle with a dot) or a checkbox (a rounded square with a check), in the accent
/// when picked.
fn mark(multi: bool, picked: bool, ui: Ui, z: f32) -> Div {
    let size = 16.0 * z;
    let base = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .size(px(size))
        .rounded(px(if multi { 5.0 * z } else { size / 2.0 }))
        .border_1()
        .border_color(if picked { ui.accent } else { ui.title_dim });
    match (multi, picked) {
        (true, true) => base.bg(ui.accent).child(
            Icon::new(IconName::Check)
                .size(px(11.0 * z))
                .text_color(ui.surface),
        ),
        (false, true) => base.child(div().size(px(8.0 * z)).rounded_full().bg(ui.accent)),
        (_, false) => base,
    }
}

/// A request with two buttons: what it asks, its yes and its no (Allow and Deny for a request to
/// connect).
fn allow(
    ws: &Workspace,
    wait: &Wait,
    (yes, no): (&'static str, &'static str),
    n: usize,
    z: f32,
    cx: &mut Context<Workspace>,
) -> Parts {
    let (key, ui) = (wait.key, ws.ui);
    Parts {
        sub: None,
        body: div().min_w(px(0.0)).child(ws.allow_line(wait, cx)),
        foot: div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0 * z))
            .w_full()
            .child(
                ghost(part(n, "deny"), no, ui, z)
                    .on_click(cx.listener(once(move |ws, _, cx| ws.answer_allow(key, false, cx))))
                    .test_support(),
            )
            .child(
                solid(part(n, "allow"), yes, None, ui, z)
                    .on_click(cx.listener(once(move |ws, _, cx| ws.answer_allow(key, true, cx))))
                    .test_support(),
            ),
    }
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
