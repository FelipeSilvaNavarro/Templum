//! The Automations popup (docs/specs/automations.md): the person's rules at the left, the selected
//! one's editor at the right, drawn like the Agents popup; and the sidebar's button with the count
//! of the rules whose last run went wrong. Window chrome: the canvas's zoom never scales it. Its
//! model is `workspace/automations.rs`.

pub(crate) mod editor;

use std::time::SystemTime;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, ElementId, FontWeight, KeyDownEvent,
    MouseButton, ScrollWheelEvent, Stateful, TestSupportExt, div, point, px,
};

use crate::automations::Went;
use crate::automations::clock::when_words;
use crate::git_block::widgets::icon_button;
use crate::grid::hsla;
use crate::inbox::{count_badge, section};
use crate::workspace::Workspace;
use crate::workspace::automations::NEW;

/// A row of the list.
const ROW: f32 = 48.0;
/// The text's sizes: the names, and what is said about them.
pub(super) const TEXT: f32 = 13.0;
pub(super) const SMALL: f32 = 11.5;
/// The most pixels of a row's last run: the words after it give way.
const LAST_W: f32 = 150.0;

/// The popup, `w` by `h` pixels: the list at the left, the selected automation at the right.
pub(crate) fn popup(ws: &Workspace, w: f32, h: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let ui = ws.ui;
    let fg = hsla(ws.theme.palette.foreground, 1.0);
    let selected = ws.automation_selected();
    div()
        .id("automations")
        .w(px(w))
        .h(px(h))
        .flex()
        .overflow_hidden()
        .bg(ui.surface)
        .text_color(fg)
        .text_size(px(TEXT))
        .border_1()
        .border_color(ui.hairline)
        .rounded(px(12.0))
        .shadow(vec![BoxShadow {
            color: ui.shadow,
            offset: point(px(0.0), px(12.0)),
            blur_radius: px(32.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        // Presses inside never reach the canvas below.
        .occlude()
        .track_focus(&ws.automations_focus)
        // Before the fields, which may have the keyboard, hear them.
        .capture_key_down(
            cx.listener(|ws, e: &KeyDownEvent, window, cx| ws.on_automations_key(e, window, cx)),
        )
        // The wheel scrolls the lists and stops there: the popup occludes the canvas under it.
        .on_scroll_wheel(|_: &ScrollWheelEvent, _, cx| cx.stop_propagation())
        .child(list(ws, selected, cx))
        .child(div().w(px(1.0)).h_full().flex_shrink_0().bg(ui.hairline))
        .child(match selected {
            Some(id) => editor::editor(ws, id, cx),
            None => div().flex_1().into_any_element(),
        })
        .test_support()
        .into_any_element()
}

/// The left column: a header, the automations, then what agents asked for.
fn list(ws: &Workspace, selected: Option<u64>, cx: &mut Context<Workspace>) -> Div {
    let ui = ws.ui;
    let header = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(10.0))
        .child(
            Icon::new(IconName::AlarmClock)
                .size(px(15.0))
                .text_color(ui.accent),
        )
        .child(
            div()
                .flex_1()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Automations"),
        )
        .child(
            div()
                .id("automation-new")
                .flex()
                .items_center()
                .gap(px(4.0))
                .h(px(24.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child(
                    Icon::new(IconName::Plus)
                        .size(px(14.0))
                        .text_color(ui.title),
                )
                .child("New")
                .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| {
                    ws.new_automation(cx);
                    ws.select_automation(NEW, window, cx);
                }))
                .test_support(),
        )
        .child(
            icon_button("automations-close", IconName::Close, ui, 1.0)
                .tooltip(|window, cx| Tooltip::new("Close (Esc)").build(window, cx))
                .on_click(
                    cx.listener(|ws, _: &ClickEvent, window, cx| ws.close_automations(window, cx)),
                )
                .test_support(),
        );
    let now = SystemTime::now();
    let ids = ws.automation_ids();
    let rows: Vec<_> = ids
        .iter()
        .map(|id| automation_row(ws, *id, selected == Some(*id), now, cx).test_support())
        .collect();
    let empty = ids.is_empty().then(|| {
        div()
            .id("automations-empty")
            .px(px(16.0))
            .py(px(24.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .text_color(ui.title_dim)
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child("No automations yet"),
            )
            .child(div().text_size(px(SMALL)).text_center().child(
                "Type a prompt, run a command, start an agent or notify you at a time you choose.",
            ))
            .test_support()
    });
    let asked = ws.schedule_rows();
    let asked = (!asked.is_empty()).then(|| {
        let rows =
            asked.into_iter().map(|(id, from, about)| {
                div()
                    .id(ElementId::Name(format!("schedule-{id}").into()))
                    .w_full()
                    .h(px(ROW))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(div().truncate().font_weight(FontWeight::MEDIUM).child(from))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(SMALL))
                                    .text_color(ui.title_dim)
                                    .child(about),
                            ),
                    )
                    .child(
                        crate::balloon::ghost(
                            ElementId::Name(format!("schedule-cancel-{id}").into()),
                            "Cancel",
                            ui,
                            1.0,
                        )
                        .flex_shrink_0()
                        .tooltip(move |window, cx| {
                            Tooltip::new(format!("Cancel s{id}")).build(window, cx)
                        })
                        .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                            ws.cancel_schedule_now(id, cx)
                        }))
                        .test_support(),
                    )
            });
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(
                div()
                    .px(px(12.0))
                    .pt(px(14.0))
                    .pb(px(6.0))
                    .child(section(ws, "Asked by agents")),
            )
            .children(rows)
    });
    div()
        .w(px(crate::agents_view::LIST_W))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .child(header)
        .child(div().h(px(1.0)).flex_shrink_0().bg(ui.hairline))
        .child(
            div()
                .id("automations-rows")
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .children(rows)
                .children(empty)
                .children(asked),
        )
}

/// One automation of the list: its switch, name and next run; under them its When and its last
/// run. The three icon buttons show over its right while the pointer is on it, and on the one
/// selected.
fn automation_row(
    ws: &Workspace,
    id: u64,
    lit: bool,
    now: SystemTime,
    cx: &mut Context<Workspace>,
) -> Stateful<Div> {
    let ui = ws.ui;
    let Some(a) = ws.automation_draft(id) else {
        return div().id(ElementId::Name(format!("automation-row-{id}").into()));
    };
    let saved = ws.automations.get(id);
    let yellow = hsla(ws.theme.palette.ansi[3], 1.0);
    let red = hsla(ws.theme.palette.ansi[1], 1.0);
    let name = match a.name.as_str() {
        "" if id == NEW => "New automation".to_string(),
        "" => ws.automation_sentence(a, cx),
        name => name.to_string(),
    };
    let missing = saved.and_then(|s| ws.automation_target(s).err());
    let off = saved.is_some_and(|s| !s.on) || missing.is_some();
    let today = chrono::Local::now().date_naive();
    let when = when_words(&a.when, today);
    let (last, tone) = match ws.last_words(id, now) {
        Some(words) => {
            let tone = match saved.and_then(|s| s.last.as_ref()).map(|l| l.went) {
                _ if words.starts_with("Waiting") => Some(yellow),
                Some(Went::Failed) => Some(red),
                Some(Went::Skipped | Went::Missed) => Some(yellow),
                _ => None,
            };
            (Some(words), tone)
        }
        None => (None, None),
    };
    let next = if id == NEW {
        "Not saved".to_string()
    } else {
        ws.next_words(id, now)
    };
    let drafted = id != NEW && ws.automation_drafts.contains_key(&id);
    let switch = saved.map(|s| {
        let entity = cx.entity();
        Switch::new(ElementId::Name(format!("automation-switch-{id}").into()))
            .checked(s.on)
            .on_click(move |on: &bool, _, cx| {
                let on = *on;
                entity.update(cx, |ws, cx| ws.set_automation_on(id, on, cx));
            })
    });
    let buttons = saved.map(|_| {
        let button = |what: &'static str, icon: IconName, tip: &'static str| {
            icon_button(
                ElementId::Name(format!("automation-{what}-{id}").into()),
                icon,
                ui,
                1.0,
            )
            .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            // Pressing one never counts as a click on the row.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        div()
            .absolute()
            .top(px(0.0))
            .right(px(8.0))
            .h(px(ROW / 2.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .px(px(4.0))
            .bg(if lit { ui.hover } else { ui.surface })
            .when(!lit, |el| {
                el.invisible()
                    .group_hover("automation-row", |s| s.visible())
            })
            .child(
                button("run", IconName::Play, "Run now").on_click(cx.listener(
                    move |ws, _: &ClickEvent, window, cx| ws.run_automation_now(id, window, cx),
                )),
            )
            .child(
                button("duplicate", IconName::CopyPlus, "Duplicate").on_click(cx.listener(
                    move |ws, _: &ClickEvent, _, cx| {
                        ws.duplicate_automation(id, cx);
                    },
                )),
            )
            .child(
                button("delete", IconName::Trash, "Delete").on_click(cx.listener(
                    move |ws, _: &ClickEvent, window, cx| ws.ask_delete_automation(id, window, cx),
                )),
            )
    });
    let dim = if off { ui.hint } else { ui.title_dim };
    div()
        .id(ElementId::Name(format!("automation-row-{id}").into()))
        .group("automation-row")
        .relative()
        .w_full()
        .h(px(ROW))
        .flex_shrink_0()
        .overflow_hidden()
        .px(px(12.0))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(2.0))
        .cursor_pointer()
        .when(lit, |el| el.bg(ui.hover))
        .hover(move |s| s.bg(ui.hover))
        .on_click(
            cx.listener(move |ws, _: &ClickEvent, window, cx| ws.select_automation(id, window, cx)),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .children(switch)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .when(off, move |el| el.text_color(dim))
                        .child(name),
                )
                .children(drafted.then(|| {
                    div()
                        .id(ElementId::Name(format!("automation-draft-{id}").into()))
                        .size(px(6.0))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(ui.accent)
                        .test_support()
                }))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(SMALL))
                        .text_color(dim)
                        .child(next),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .gap(px(6.0))
                .text_size(px(SMALL))
                .text_color(ui.title_dim)
                .child(match &missing {
                    Some(why) => div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_color(yellow)
                        .child(why.clone()),
                    None => div().flex_1().min_w(px(0.0)).truncate().child(when),
                })
                .children(last.filter(|_| missing.is_none()).map(|words| {
                    // A held run says why it waits.
                    let why = ws.automation_held_why(id);
                    div()
                        .id(ElementId::Name(format!("automation-lastrun-{id}").into()))
                        .flex_shrink_0()
                        .max_w(px(LAST_W))
                        .truncate()
                        .text_color(tone.unwrap_or(ui.title_dim))
                        .child(words)
                        .when_some(why, |el, why| {
                            el.tooltip(move |window, cx| {
                                Tooltip::new(why.clone()).build(window, cx)
                            })
                        })
                })),
        )
        .children(buttons)
}

/// The sidebar's button: the popup toggled, and the count of the automations whose last run went
/// wrong since it was opened. It never pulses: an animation in the sidebar redraws the whole
/// window.
pub(crate) fn button(ws: &Workspace, size: f32, cx: &mut Context<Workspace>) -> Div {
    let ui = ws.ui;
    div()
        .relative()
        .child(
            div()
                .id("automations-button")
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(size))
                .rounded(px(10.0))
                .cursor_pointer()
                .when(ws.automations_open, move |el| el.bg(ui.pressed))
                .hover(move |s| s.bg(ui.hover))
                .active(move |s| s.bg(ui.pressed))
                .child(
                    Icon::new(IconName::AlarmClock)
                        .size(px(16.0))
                        .text_color(ui.title),
                )
                .tooltip(|window, cx| Tooltip::new("Automations").build(window, cx))
                .on_click(
                    cx.listener(|ws, _: &ClickEvent, window, cx| ws.toggle_automations(window, cx)),
                )
                .test_support(),
        )
        .children(count_badge(
            ws,
            ws.automations_unseen(),
            hsla(ws.theme.palette.ansi[1], 1.0),
            "automations-badge",
        ))
}
