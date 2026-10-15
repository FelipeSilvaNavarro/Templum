//! The Git block's commit form: Summary, Description, Amend and Commit.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, Div, ElementId, SharedString, Stateful, TestSupportExt, div,
    px,
};
use templum_canvas::BlockId;

use super::widgets::{field, icon_button};
use super::words;
use super::{FONT, Git};
use crate::editor::button;
use crate::grid::hsla;
use crate::menu::Target;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// The commit form's Description, three rows high, at zoom 1.
const DESCRIPTION_H: f32 = 78.0;

/// Summary, Description, Amend, Commit; or why committing isn't possible.
pub(super) fn commit_form(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let can = git.can_commit(cx);
    let len = git.summary_len(cx);
    let amend = git.amend;
    div()
        .flex()
        .flex_col()
        .gap(px(6.0 * z))
        .p(px(8.0 * z))
        .border_t_1()
        .border_color(ui.hairline)
        .text_size(px(FONT * z))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0 * z))
                .child(field(&git.summary, z).flex_1())
                .when(len > 0, |el| {
                    el.child(
                        div()
                            .id(("git-summary-count", n))
                            .flex_shrink_0()
                            .text_size(px(11.0 * z))
                            .text_color(if git.summary_too_long(cx) {
                                hsla(ws.theme.palette.ansi[3], 1.0)
                            } else {
                                ui.title_dim
                            })
                            .child(len.to_string())
                            .test_support(),
                    )
                })
                .children(generate_button(ws, id, git, z, cx)),
        )
        .child(
            // Three rows, at the zoom: gpui-kit sizes it from the theme's fixed font otherwise.
            Textarea::new(&git.description)
                .text_size(px(FONT * z))
                .h(px(DESCRIPTION_H * z))
                .w_full(),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0 * z))
                .child(
                    check(("git-amend", n), "Amend the last commit", amend, ui, z)
                        .tooltip(|window, cx| Tooltip::new(words::tip("Amend")).build(window, cx))
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.set_amend(id, !amend, window, cx)
                        }))
                        .test_support(),
                )
                .child(div().flex_1())
                .when(can, |el| {
                    el.child(
                        button(("git-commit", n), git.commit_label(), ui, z)
                            .tooltip(|window, cx| {
                                Tooltip::new(words::tip("Commit")).build(window, cx)
                            })
                            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                ws.git_commit(id, false, window, cx)
                            }))
                            .test_support(),
                    )
                    .child(
                        icon_button(("git-commit-menu", n), IconName::ChevronDown, ui, z)
                            .on_click(cx.listener(move |ws, e: &ClickEvent, window, cx| {
                                ws.open_menu(Target::GitCommitMenu(id), e.position(), window, cx)
                            }))
                            .test_support(),
                    )
                }),
        )
        .when(git.author.is_none() && git.repo().is_some(), |el| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0 * z))
                    .text_color(ui.title_dim)
                    .child("Set user.name and user.email to commit")
                    .child(
                        button(("git-set-author", n), "Set…", ui, z)
                            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                ws.set_git_author(id, window, cx)
                            }))
                            .test_support(),
                    ),
            )
        })
        .into_any_element()
}

/// Generate ✨ and its ⚙, beside the summary: shown while the agent chosen in Settings ▸ Git is ready and
/// there is something to describe (what is staged, or every change for Commit all changes), or
/// while the agent writes, when it spins and a click stops it.
fn generate_button(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> Option<AnyElement> {
    let agent = ws.commit_agent.as_ref().filter(|a| a.ready)?;
    let running = git.generating.is_some();
    if git.repo().is_none() || (git.status.staged().is_empty() && !git.commit_all() && !running) {
        return None;
    }
    let (ui, n) = (ws.ui, id.0 as usize);
    let tip: SharedString = if running {
        "Stop the agent".into()
    } else {
        format!("Generate: {} writes the message", agent.title).into()
    };
    let generate = div()
        .id(("git-generate", n))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(22.0 * z))
        .rounded(px(5.0 * z))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(if running {
            Spinner::new()
                .with_size(UiSize::Size(px(12.0 * z)))
                .into_any_element()
        } else {
            Icon::new(IconName::Sparkles)
                .size(px(13.0 * z))
                .text_color(ui.title)
                .into_any_element()
        })
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(
            cx.listener(move |ws, _: &ClickEvent, window, cx| ws.generate_message(id, window, cx)),
        )
        .test_support();
    let menu = icon_button(("git-generate-menu", n), IconName::Settings2, ui, z)
        .flex_shrink_0()
        .tooltip(|window, cx| {
            Tooltip::new("The model, and this repository's instructions").build(window, cx)
        })
        .on_click(cx.listener(move |ws, e: &ClickEvent, window, cx| {
            ws.open_menu(Target::GitGenerateMenu(id), e.position(), window, cx)
        }))
        .test_support();
    Some(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .child(generate)
            .child(menu)
            .into_any_element(),
    )
}

/// A checkbox and its label at the zoom (gpui-kit's has a fixed size).
fn check(id: impl Into<ElementId>, label: &'static str, on: bool, ui: Ui, z: f32) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .text_size(px(FONT * z))
        .text_color(ui.title)
        .cursor_pointer()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(14.0 * z))
                .rounded(px(3.0 * z))
                .border_1()
                .border_color(if on { ui.accent } else { ui.hairline })
                .when(on, |el| el.bg(ui.accent))
                .when(on, |el| {
                    el.child(
                        Icon::new(IconName::Check)
                            .size(px(11.0 * z))
                            .text_color(ui.canvas),
                    )
                }),
        )
        .child(label)
}
