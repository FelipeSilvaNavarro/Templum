//! The Git block's shared pieces: messages, bars, icon buttons, badge colors, fields.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{Div, ElementId, Entity, Stateful, div, px};
use templum_git::Change;

use super::FONT;
use crate::grid::hsla;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// A one-line field's height at zoom 1, and its padding.
const FIELD_H: f32 = 24.0;

/// A message in the middle of the block.
pub(crate) fn centered(ui: Ui, z: f32) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.0 * z))
        .px(px(16.0 * z))
        .text_size(px(13.0 * z))
        .text_color(ui.title_dim)
}

/// A line across the block, for what holds for the whole repository.
pub(crate) fn bar(ui: Ui, z: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .px(px(10.0 * z))
        .py(px(4.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .bg(ui.card)
        .text_size(px(FONT * z))
        .text_color(ui.title_dim)
}

/// A small square button holding an icon.
pub(crate) fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    ui: Ui,
    z: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0 * z))
        .rounded(px(5.0 * z))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(Icon::new(icon).size(px(13.0 * z)).text_color(ui.title))
}

/// The badge's color for a change: the terminal's palette, so it fits the theme.
pub(super) fn badge_color(ws: &Workspace, change: Change) -> gpui_kit::Hsla {
    let ansi = &ws.theme.palette.ansi;
    hsla(
        match change {
            Change::Modified => ansi[3],
            Change::Added | Change::Untracked => ansi[2],
            Change::Deleted | Change::Conflict => ansi[1],
            Change::Renamed => ansi[4],
            Change::TypeChanged => ansi[5],
        },
        1.0,
    )
}

/// A one-line text field at the zoom: gpui-kit's fields keep a fixed height and padding, so
/// they are set here with the rest of the block's sizes.
pub(super) fn field(state: &Entity<InputState>, z: f32) -> Input {
    let input = Input::new(state)
        .small()
        .text_size(px(FONT * z))
        .px(px(8.0 * z))
        .py(px(2.0 * z));
    // Styled's height: Input's own `h` is for multi-line fields only.
    Styled::h(input, px(FIELD_H * z))
}
