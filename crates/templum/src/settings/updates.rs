//! Settings ▸ Updates: the version running, where an update stands with the button for its next
//! step, and the automatic check (docs/specs/updates.md).

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{ClickEvent, Entity, TestSupportExt, div, px};
use templum_update::VERSION;

use super::row::{Row, remove};
use crate::config::Edit;
use crate::update::{Act, Update};
use crate::workspace::Workspace;

pub(super) fn rows(ws: &Workspace, entity: &Entity<Workspace>) -> Vec<Row> {
    let ui = ws.ui;
    let busy = matches!(ws.update, Update::Checking | Update::Downloading { .. });
    let next = match ws.update.next() {
        Some(act @ (Act::Restart | Act::Install)) => Some(act),
        _ => None,
    };
    let label = match (&ws.update, &next) {
        (Update::Checking, _) => "Checking…",
        (Update::Downloading { .. }, _) => "Downloading…",
        (_, Some(Act::Restart)) => "Restart",
        (_, Some(Act::Install)) => "Install",
        _ => "Check now",
    };
    let e = entity.clone();
    let button = div()
        .id("update-check")
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(10.0))
        .py(px(3.0))
        .rounded_full()
        .border_1()
        .border_color(ui.hairline)
        .text_size(px(12.0))
        .when(busy, |el| {
            el.child(Spinner::new().with_size(UiSize::Size(px(12.0))))
        })
        .when(!busy, |el| {
            el.cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .active(move |s| s.bg(ui.pressed))
                .on_click(move |_: &ClickEvent, window, cx| {
                    e.update(cx, |ws, cx| match &next {
                        Some(act) => ws.update_act(act, window, cx),
                        None => ws.check_update(true, window, cx),
                    })
                })
        })
        .child(label)
        .test_support();
    let status = ws.update.status(checked_at);
    let on = ws.config.updates.check;
    let s = entity.clone();
    vec![
        Row::new("updates.version", format!("Version {VERSION}"))
            .description(status)
            .control(button),
        Row::new("updates.check", "Check for updates automatically")
            .description("At start and once a day; a new version downloads by itself")
            .control(Switch::new("updates-check-switch").checked(on).on_click(
                move |on: &bool, window, cx| {
                    let on = *on;
                    s.update(cx, |ws, cx| {
                        ws.edit(
                            Edit::set("updates", "check", on),
                            |c| c.updates.check = on,
                            cx,
                        );
                        // Turned on, it looks now rather than tomorrow.
                        if on {
                            ws.check_update(false, window, cx);
                        }
                    })
                },
            ))
            .reset(
                !on,
                remove(entity, "updates", "check", |c| c.updates.check = true),
            ),
    ]
}

/// "14:02": when the last check ran (`at`, seconds since the epoch), in local time.
fn checked_at(at: u64) -> String {
    chrono::DateTime::from_timestamp(i64::try_from(at).unwrap_or(i64::MAX), 0)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}
