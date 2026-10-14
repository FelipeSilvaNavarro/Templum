//! The card for the theme in use, under Settings ▸ Appearance's rows: whose it is, what can be
//! done with it, and, for one of the user's, its colors to change.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, Hsla, SharedString, Subscription,
    TestSupportExt, Window, div, px,
};
use templum_terminal::Rgb;

use super::{Row, card};
use crate::grid::hsla;
use crate::prompt;
use crate::theme::{self, COLORS, Ui};
use crate::workspace::Workspace;

/// The editor's groups: a title and the colors in it (`theme::key` order).
const GROUPS: [(&str, std::ops::Range<usize>); 4] = [
    ("Primary", 0..4),
    ("Normal", 4..12),
    ("Bright", 12..20),
    ("Interface", 20..COLORS),
];

/// Each color's name, in `theme::key` order.
const NAMES: [&str; COLORS] = [
    "Background",
    "Foreground",
    "Cursor",
    "Selection",
    "Black",
    "Red",
    "Green",
    "Yellow",
    "Blue",
    "Magenta",
    "Cyan",
    "White",
    "Black",
    "Red",
    "Green",
    "Yellow",
    "Blue",
    "Magenta",
    "Cyan",
    "White",
    "Accent",
    "Canvas",
    "Dots",
];

pub(super) fn theme_card(
    ws: &Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
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
    let mut rows = vec![
        Row::new("theme.in-use", ws.theme.name.clone())
            .description(description)
            .control(buttons),
    ];
    if path.is_some() {
        let colors = ws.theme.colors();
        // The theme's own colors come first in each picker.
        let featured: Vec<Hsla> = colors[..20].iter().map(|&c| hsla(c, 1.0)).collect();
        let entity = cx.entity();
        for (title, range) in GROUPS {
            let swatches = range.map(|i| {
                let reset = i >= 20 && ws.theme.interface[i - 20].is_some();
                swatch(i, colors[i], &featured, reset, ws.ui, &entity, window, cx)
            });
            rows.push(
                Row::new(format!("theme.colors.{title}"), title)
                    .below(div().flex().flex_wrap().gap(px(10.0)).children(swatches)),
            );
        }
    }
    div()
        .mt(px(12.0))
        .child(card(ws.ui, rows))
        .into_any_element()
}

/// One color's picker; it lives as long as the panel shows it.
struct Swatch {
    state: Entity<ColorPickerState>,
    _change: Subscription,
}

#[allow(clippy::too_many_arguments)]
/// Color `i` as a swatch with its name; a change goes to the workspace. `reset`: a ↺ beside it
/// takes the `[templum]` color out of the file.
fn swatch(
    i: usize,
    color: Rgb,
    featured: &[Hsla],
    reset: bool,
    ui: Ui,
    entity: &Entity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let e = entity.clone();
    let swatch = window.use_keyed_state(
        SharedString::from(format!("theme-color-{i}")),
        cx,
        move |window, cx| {
            let state =
                cx.new(|cx| ColorPickerState::new(window, cx).default_value(hsla(color, 1.0)));
            let _change = cx.subscribe_in(
                &state,
                window,
                move |_, _, event: &ColorPickerEvent, window, cx| {
                    if let ColorPickerEvent::Change(Some(c)) = event {
                        let c = theme::rgb(*c);
                        e.update(cx, |ws, cx| ws.set_theme_color(i, Some(c), window, cx));
                    }
                },
            );
            Swatch { state, _change }
        },
    );
    let state = swatch.read(cx).state.clone();
    // What the theme shows wins (another theme, a hand edit). Compared rounded: the picker's
    // own color, mid-drag, is finer than the file's, and putting it back would fight the drag.
    if state.read(cx).value().map(theme::rgb) != Some(color) {
        state.update(cx, |state, cx| {
            state.set_value(hsla(color, 1.0), window, cx)
        });
    }
    let e = entity.clone();
    div()
        .id(ElementId::Name(format!("theme-color-{i}").into()))
        .flex()
        .items_center()
        .gap(px(2.0))
        // The picker's label takes the text size around it: the panel's.
        .text_size(px(13.0))
        .child(
            ColorPicker::new(&state)
                .small()
                .label(NAMES[i])
                .featured_colors(featured.to_vec()),
        )
        .when(reset, |el| {
            el.child(
                div()
                    .id(ElementId::Name(format!("theme-color-reset-{i}").into()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.0))
                    .rounded_full()
                    .cursor_pointer()
                    .hover(move |s| s.bg(ui.hover))
                    .active(move |s| s.bg(ui.pressed))
                    .child(
                        Icon::new(IconName::Undo2)
                            .size(px(12.0))
                            .text_color(ui.title_dim),
                    )
                    .on_click(move |_: &ClickEvent, window, cx| {
                        e.update(cx, |ws, cx| ws.set_theme_color(i, None, window, cx))
                    })
                    .test_support(),
            )
        })
        .test_support()
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
