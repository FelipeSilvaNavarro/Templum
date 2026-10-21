//! The Git block's Stashes tab: Stash changes…, the stashes, and the shown stash's diff.

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, TestSupportExt, div,
    px,
};

use super::{FONT, Git, LISTS_W, ROW_H, diff_view, words};
use crate::editor::button;
use crate::menu::Target;
use crate::workspace::Workspace;

/// The Stashes tab: Stash changes…, the list, and the shown stash's diff on the right.
pub(super) fn stashes(
    ws: &Workspace,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let rows = git.stashes.iter().enumerate().map(|(i, s)| {
        let index = s.index;
        div()
            .id(ElementId::Name(format!("git-stash-{i}").into()))
            .flex()
            .items_center()
            .gap(px(8.0 * z))
            .h(px(ROW_H * z))
            .px(px(10.0 * z))
            .text_size(px(FONT * z))
            .text_color(ui.title)
            .when(git.stash_shown == Some(index), |el| el.bg(ui.card))
            .cursor_pointer()
            .hover(move |st| st.bg(ui.hover))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .child(s.message.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(ui.title_dim)
                    .child(templum_git::ago(now, s.time)),
            )
            .on_click(
                cx.listener(move |ws, _: &ClickEvent, window, cx| ws.show_stash(index, window, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    ws.open_menu(Target::GitStash { index }, e.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .test_support()
    });
    let left = div()
        .w(px(LISTS_W * z))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(ui.hairline)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0 * z))
                .p(px(6.0 * z))
                .child(
                    button("git-stash-changes", "Stash changes…", ui, z)
                        .tooltip(|window, cx| Tooltip::new(words::tip("Stash")).build(window, cx))
                        .when(git.status.entries.is_empty() || git.acting > 0, |el| {
                            el.opacity(0.5)
                        })
                        .when(!git.status.entries.is_empty() && git.acting == 0, |el| {
                            el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                ws.stash_changes(window, cx)
                            }))
                        })
                        .test_support(),
                )
                .child(div().flex_1()),
        )
        .child(
            div()
                .id("git-stash-list")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(&git.stash_scroll)
                .children(rows)
                .test_support(),
        );
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .when(!git.lists_folded, |el| el.child(left))
        .child(diff_view::panel(ws, git, z, cx))
        .into_any_element()
}
