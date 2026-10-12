//! The Git block's Stashes tab: Stash changes…, the stashes, and the shown stash's diff.

use gpui_kit::component::input::Editor as EditorView;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, TestSupportExt, div,
    px, relative,
};
use templum_canvas::BlockId;

use super::widgets::centered;
use super::{FONT, Git, LISTS_W, ROW_H};
use crate::editor::button;
use crate::menu::Target;
use crate::workspace::Workspace;

/// The Stashes tab: Stash changes…, the list, and the shown stash's diff on the right.
pub(super) fn stashes(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let rows = git.stashes.iter().enumerate().map(|(i, s)| {
        let index = s.index;
        div()
            .id(ElementId::Name(format!("git-stash-{n}-{i}").into()))
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
            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                ws.show_stash(id, index, window, cx)
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    ws.open_menu(
                        Target::GitStash { block: id, index },
                        e.position,
                        window,
                        cx,
                    );
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
            div().p(px(6.0 * z)).child(
                button(("git-stash-changes", n), "Stash changes…", ui, z)
                    .when(git.status.entries.is_empty() || git.acting > 0, |el| {
                        el.opacity(0.5)
                    })
                    .when(!git.status.entries.is_empty() && git.acting == 0, |el| {
                        el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.stash_changes(id, window, cx)
                        }))
                    })
                    .test_support(),
            ),
        )
        .child(
            div()
                .id(("git-stash-list", n))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .children(rows),
        );
    let right: AnyElement = match git.stash_shown {
        None => centered(ui, z)
            .flex_1()
            .child("Click a stash to see its diff")
            .into_any_element(),
        Some(_) => div()
            .flex_1()
            .min_w(px(0.0))
            .child(
                EditorView::new(&git.stash_diff)
                    .appearance(false)
                    .bordered(false)
                    .readonly(true)
                    .font_family(ws.cell.font.family.clone())
                    .text_size(px(FONT * z))
                    .h(relative(1.0)),
            )
            .into_any_element(),
    };
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .child(left)
        .child(right)
        .into_any_element()
}
