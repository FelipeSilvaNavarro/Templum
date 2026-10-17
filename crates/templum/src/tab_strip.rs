//! The tab strip of note and checklist blocks: a tab per open note or checklist, the one on
//! screen underlined; a click shows one, its × closes it, + opens a new one in a new tab.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, SharedString, TestSupportExt, Window, div, px,
};
use templum_canvas::BlockId;

use crate::workspace::Workspace;

/// The strip's height, world units.
pub(crate) const TABS_H: f32 = 28.0;

/// What a tab's click, its × and the strip's + do.
pub(crate) type TabAction =
    fn(&mut Workspace, BlockId, usize, &mut Window, &mut Context<Workspace>);

/// The tabs `titles` of `block`, `active` on screen, at zoom `z`; their element ids start with
/// `prefix` (`{prefix}-tab-{n}-{i}`, `{prefix}-tab-close-{n}-{i}`, `({prefix}-tab-new, n)`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tab_strip(
    ws: &Workspace,
    block: BlockId,
    prefix: &'static str,
    titles: Vec<SharedString>,
    active: usize,
    z: f32,
    show: TabAction,
    close: TabAction,
    new: fn(&mut Workspace, BlockId, &mut Window, &mut Context<Workspace>),
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = block.0 as usize;
    let name = |what: &str, i: usize| ElementId::Name(format!("{prefix}-{what}-{n}-{i}").into());
    let tabs: Vec<AnyElement> =
        titles
            .into_iter()
            .enumerate()
            .map(|(i, title)| {
                let on = i == active;
                let close = div()
                    .id(name("tab-close", i))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(16.0 * z))
                    .rounded(px(4.0 * z))
                    .hover(move |s| s.bg(ui.hover))
                    .child(Icon::new(IconName::Close).size(px(10.0 * z)))
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        close(ws, block, i, window, cx)
                    }))
                    .test_support();
                div()
                    .id(name("tab", i))
                    .flex_shrink_0()
                    .max_w(px(170.0 * z))
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(4.0 * z))
                    .pl(px(10.0 * z))
                    .pr(px(4.0 * z))
                    .border_b_2()
                    .border_color(if on {
                        ui.accent
                    } else {
                        gpui_kit::transparent_black()
                    })
                    .text_size(px(12.0 * z))
                    .text_color(if on { ui.title } else { ui.title_dim })
                    .cursor_pointer()
                    .hover(move |s| s.bg(ui.hover))
                    .child(div().truncate().child(title))
                    .child(close)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        show(ws, block, i, window, cx)
                    }))
                    .test_support()
                    .into_any_element()
            })
            .collect();
    let plus = div()
        .id((SharedString::from(format!("{prefix}-tab-new")), n))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0 * z))
        .mx(px(4.0 * z))
        .rounded(px(5.0 * z))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(
            Icon::new(IconName::Plus)
                .size(px(12.0 * z))
                .text_color(ui.title),
        )
        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| new(ws, block, window, cx)))
        .test_support();
    div()
        .id((SharedString::from(format!("{prefix}-tabs")), n))
        .flex_shrink_0()
        .h(px(TABS_H * z))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(ui.hairline)
        .overflow_x_scroll()
        .children(tabs)
        .child(plus)
        .test_support()
        .into_any_element()
}
