//! The Git block's Branches tab: a filter, New branch…, and the local and remote branches.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, TestSupportExt, div,
    px,
};
use templum_canvas::BlockId;

use super::widgets::field;
use super::{FONT, Git, ROW_H};
use crate::editor::button;
use crate::menu::Target;
use crate::workspace::Workspace;

/// The Branches tab: a filter and New branch…, then the local and the remote lists.
pub(super) fn branches(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let query = git.filter.read(cx).value().trim().to_lowercase();
    let busy = git.acting > 0;
    let rows = |remote: bool| {
        git.branches
            .iter()
            .filter(|b| {
                b.remote == remote && (query.is_empty() || b.name.to_lowercase().contains(&query))
            })
            .enumerate()
            .map(|(i, b)| {
                let (name, click, menu) = (b.name.clone(), b.name.clone(), b.name.clone());
                let sync = match (&b.upstream, b.gone) {
                    (Some(_), true) => "upstream gone".to_string(),
                    (Some(up), false) => format!("{up} ↑{} ↓{}", b.ahead, b.behind),
                    (None, _) => String::new(),
                };
                div()
                    .id(ElementId::Name(
                        format!(
                            "git-branch-{n}-{}-{i}",
                            if remote { "remote" } else { "local" }
                        )
                        .into(),
                    ))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * z))
                    .h(px(ROW_H * z))
                    .px(px(10.0 * z))
                    .text_size(px(FONT * z))
                    .text_color(ui.title)
                    .cursor_pointer()
                    .hover(move |st| st.bg(ui.hover))
                    .child(div().w(px(14.0 * z)).flex_shrink_0().when(b.current, |el| {
                        el.child(
                            Icon::new(IconName::Check)
                                .size(px(12.0 * z))
                                .text_color(ui.accent),
                        )
                    }))
                    .child(
                        div()
                            .when(b.current, |el| {
                                el.font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            })
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_color(ui.title_dim)
                            .child(b.subject.clone()),
                    )
                    .child(div().flex_shrink_0().text_color(ui.title_dim).child(sync))
                    .when(!busy, |el| {
                        el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.checkout_branch(id, click.clone(), remote, window, cx)
                        }))
                    })
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            ws.open_menu(
                                Target::GitBranch {
                                    block: id,
                                    name: menu.clone(),
                                },
                                e.position,
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .test_support()
            })
            .collect::<Vec<_>>()
    };
    let heading = |title: &'static str| {
        div()
            .h(px(ROW_H * z))
            .px(px(10.0 * z))
            .flex()
            .items_center()
            .text_size(px(11.0 * z))
            .text_color(ui.title_dim)
            .child(title)
    };
    div()
        .id(("git-branches", n))
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0 * z))
                .p(px(6.0 * z))
                .child(field(&git.filter, z).flex_1())
                .child(
                    button(("git-new-branch", n), "New branch…", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.branch_from(id, None, window, cx)
                        }))
                        .test_support(),
                ),
        )
        .child(
            div()
                .id(("git-branch-list", n))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .child(heading("Local"))
                .children(rows(false))
                .child(heading("Remote"))
                .children(rows(true)),
        )
        .test_support()
        .into_any_element()
}
