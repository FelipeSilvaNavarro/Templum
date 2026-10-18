//! The chat (docs/specs/templum-agent.md#the-chat): a floating card that rises from the pill, the
//! pill its footer; window chrome over the canvas, which works around it.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, BoxShadow, ClickEvent, Context, ElementId,
    MouseButton, MouseDownEvent, TestSupportExt, Window, div, point, px,
};

use crate::workspace::Workspace;

const WIDTH: f32 = 640.0;
const SIDES: f32 = 32.0;
const MIN_H: f32 = 240.0;
/// At most this much of the canvas's height.
const MAX_SHARE: f32 = 0.7;
const HEADER: f32 = 36.0;
/// It rises this far as it fades in.
const RISE: f32 = 12.0;
const APPEAR: Duration = Duration::from_millis(220);

/// What an empty conversation offers; a click puts its text in the field.
pub(crate) const SUGGESTIONS: [&str; 3] = [
    "Set up a workspace for a project, with a kanban and an agent per card",
    "Catch me up: what's going on?",
    "Tidy this workspace into a grid",
];

/// The chat, `canvas` the canvas's width and height, `footer` the pill; None while it is closed.
pub(crate) fn render(
    ws: &Workspace,
    (canvas_w, canvas_h): (f32, f32),
    footer: AnyElement,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Option<AnyElement> {
    let agent = &ws.templum_agent;
    if !agent.open {
        return None;
    }
    let ui = ws.ui;
    let rows = &agent.conversation.rows;
    let button = |id: &'static str, icon: IconName| {
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(24.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(move |s| s.bg(ui.hover))
            .child(Icon::new(icon).size(px(14.0)).text_color(ui.title_dim))
    };
    let header = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(HEADER))
        .px(px(12.0))
        .border_b_1()
        .border_color(ui.hairline)
        .child(
            Icon::new(IconName::Sparkles)
                .size(px(14.0))
                .text_color(ui.accent),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(ui.title)
                .child("Templum Agent"),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(ui.title_dim)
                .child(super::pill::engine(ws)),
        )
        .child(div().flex_1())
        .when(agent.conversation.working, |el| {
            el.child(
                button("agent-stop", IconName::CircleStop)
                    .on_click(cx.listener(|ws, _: &ClickEvent, _, cx| ws.stop_engine(cx)))
                    .test_support(),
            )
        })
        .child(
            button("agent-new", IconName::MessageSquarePlus)
                .on_click(cx.listener(|ws, _: &ClickEvent, _, cx| ws.new_chat(cx)))
                .test_support(),
        )
        .child(
            button("agent-close", IconName::ChevronDown)
                .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| ws.leave_agent(window, cx)))
                .test_support(),
        );
    // It follows its end while it is there; a row below the view says so.
    let scroll = &agent.scroll;
    let at_end = -scroll.offset().y >= scroll.max_offset().y - px(4.0);
    if at_end {
        agent.unseen.set(false);
        if rows.len() != agent.seen.get() || agent.conversation.working {
            scroll.scroll_to_bottom();
        }
    } else if rows.len() > agent.seen.get() {
        agent.unseen.set(true);
    }
    agent.seen.set(rows.len());
    let body = if rows.is_empty() {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0))
            .children(SUGGESTIONS.iter().enumerate().map(|(n, text)| {
                div()
                    .id(ElementId::Name(format!("agent-suggestion-{n}").into()))
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(ui.hairline)
                    .text_size(px(13.0))
                    .text_color(ui.title)
                    .cursor_pointer()
                    .hover(move |s| s.bg(ui.hover))
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.suggest_to_agent(text, window, cx)
                    }))
                    .child(*text)
                    .test_support()
            }))
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .children(
                rows.iter()
                    .enumerate()
                    .map(|(n, row)| super::rows::row(ws, n, row, cx)),
            )
            .into_any_element()
    };
    let new_below = agent.unseen.get().then(|| {
        div()
            .absolute()
            .bottom(px(8.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("agent-new-below")
                    .px(px(10.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(ui.accent)
                    .text_size(px(12.0))
                    .text_color(ui.canvas)
                    .cursor_pointer()
                    .on_click(cx.listener(|ws, _: &ClickEvent, _, cx| {
                        ws.templum_agent.scroll.scroll_to_bottom();
                        cx.notify();
                    }))
                    .child("↓ New")
                    .test_support(),
            )
    });
    let max_h = (canvas_h * MAX_SHARE).max(MIN_H);
    let chat = div()
        .id("agent-chat")
        .flex()
        .flex_col()
        .w(px(WIDTH.min(canvas_w - SIDES)))
        .min_h(px(MIN_H))
        .max_h(px(max_h))
        .rounded(px(14.0))
        .bg(ui.surface)
        .border_1()
        .border_color(ui.hairline)
        .shadow(vec![BoxShadow {
            color: ui.shadow,
            offset: point(px(0.0), px(10.0)),
            blur_radius: px(30.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .overflow_hidden()
        .on_key_down(cx.listener(Workspace::on_agent_key))
        // The canvas under it gets none of its presses; a press outside doesn't close it.
        .occlude()
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
            cx.stop_propagation()
        })
        .child(header)
        .child(
            div()
                .relative()
                .flex_1()
                .min_h(px(0.0))
                .child(
                    div()
                        .id("agent-body")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .child(body),
                )
                .children(new_below),
        )
        .child(
            div()
                .flex_none()
                .p(px(8.0))
                .border_t_1()
                .border_color(ui.hairline)
                .child(footer),
        )
        .test_support();
    Some(
        chat.with_animation(
            "agent-chat-rise",
            Animation::new(APPEAR).with_easing(crate::motion::slide),
            |el, t| el.opacity(t).mt(px(RISE * (1.0 - t))),
        )
        .into_any_element(),
    )
}
