//! The card for the theme in use, under Settings ▸ Appearance's rows: whose it is, and what can
//! be done with it.

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, Context, Window, div, px};

use super::{Row, card};
use crate::prompt;
use crate::workspace::Workspace;

pub(super) fn theme_card(ws: &Workspace, cx: &mut Context<Workspace>) -> AnyElement {
    let path = ws.theme_path();
    let description = match &path {
        Some(path) => format!(
            "Yours · themes/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        None => "Built in: duplicate it to change its colors".to_string(),
    };
    // Without a config folder there is nowhere to write a theme.
    let can_write = ws.config_dir.is_some();
    let buttons =
        div()
            .flex()
            .gap(px(8.0))
            .when(can_write && path.is_some(), |el| {
                el.child(
                    Button::new("theme-open-file")
                        .label("Open file")
                        .small()
                        .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| {
                            ws.open_theme_file(window, cx)
                        })),
                )
                .child(
                    Button::new("theme-new")
                        .label("New theme…")
                        .small()
                        .on_click(
                            cx.listener(|ws, _: &ClickEvent, window, cx| ask_name(ws, window, cx)),
                        ),
                )
            })
            .when(can_write && path.is_none(), |el| {
                el.child(
                    Button::new("theme-duplicate")
                        .label("Duplicate…")
                        .small()
                        .on_click(
                            cx.listener(|ws, _: &ClickEvent, window, cx| ask_name(ws, window, cx)),
                        ),
                )
            });
    let rows = vec![
        Row::new("theme.in-use", ws.theme.name.clone())
            .description(description)
            .control(buttons),
    ];
    div()
        .mt(px(12.0))
        .child(card(ws.ui, rows))
        .into_any_element()
}

/// The new theme's name; it starts as a copy of the theme in use.
fn ask_name(ws: &Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let entity = cx.entity();
    prompt::ask_text(
        format!("New theme from {}", ws.theme.name),
        "Create",
        format!("{} copy", ws.theme.name),
        move |name, window, cx| entity.update(cx, |ws, cx| ws.new_theme(&name, window, cx)),
        window,
        cx,
    );
}
