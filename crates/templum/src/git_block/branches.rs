//! The Git block's Branches tab: a left column (a filter, New branch…, the local branches, the
//! remote ones folded) and, on the right, the comparison of a branch with the default one.
//! See docs/specs/git.md, section 5.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, ScrollHandle,
    TestSupportExt, div, px,
};
use templum_canvas::BlockId;
use templum_git::Branch;

use super::widgets::{centered, field, icon_button, small};
use super::{Compare, FONT, Git, LISTS_W, ROW_H, diff_view, words};
use crate::editor::button;
use crate::menu::Target;
use crate::workspace::Workspace;

/// The Branches tab.
pub(super) fn branches(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = id.0 as usize;
    let (column, right) = match &git.compare {
        Some(c) => (
            compare_column(ws, id, c, &git.compare_scroll, z, cx),
            diff_view::panel(ws, id, git, z, cx),
        ),
        None => (
            branch_column(ws, id, git, z, cx),
            no_comparison(ws, id, git, z, cx),
        ),
    };
    div()
        .id(("git-branches", n))
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .child(column)
        .child(right)
        .test_support()
        .into_any_element()
}

/// The right side before a comparison is asked for, with Compare for the current branch.
fn no_comparison(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let (ui, n) = (ws.ui, id.0 as usize);
    let base = git
        .default_branch
        .as_deref()
        .map(words::local_name)
        .unwrap_or("main")
        .to_string();
    let current = git.status.branch.clone();
    centered(ui, z)
        .flex_1()
        .child(format!(
            "Right-click a branch and choose Compare with {base} to see what it has that {base} doesn't."
        ))
        .when_some(
            current.filter(|c| git.default_branch.is_some() && *c != base),
            |el, current| {
                el.child(
                    button(
                        ("git-compare-current", n),
                        format!("Compare {current} with {base}"),
                        ui,
                        z,
                    )
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.compare_branch(id, current.clone(), window, cx)
                    }))
                    .test_support(),
                )
            },
        )
        .into_any_element()
}

/// A comparison's column: ← back, its heading (a click shows every commit), the base, and the
/// commits newest first (a click narrows the panel to one).
fn compare_column(
    ws: &Workspace,
    id: BlockId,
    c: &Compare,
    scroll: &ScrollHandle,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let rows: Vec<AnyElement> = c
        .commits
        .iter()
        .enumerate()
        .map(|(i, commit)| {
            let hash = commit.hash.clone();
            let selected = c.narrowed.as_deref() == Some(commit.hash.as_str());
            div()
                .id(ElementId::Name(
                    format!("git-compare-commit-{n}-{i}").into(),
                ))
                .flex()
                .flex_col()
                .justify_center()
                .h(px(2.0 * ROW_H * z))
                .px(px(10.0 * z))
                .text_size(px(FONT * z))
                .text_color(ui.title)
                .when(selected, |el| el.bg(ui.card))
                .cursor_pointer()
                .hover(move |st| st.bg(ui.hover))
                .child(div().truncate().child(commit.subject.clone()))
                .child(
                    div()
                        .text_size(px(11.0 * z))
                        .text_color(ui.title_dim)
                        .child(format!(
                            "{} · {}",
                            commit.short,
                            templum_git::ago(now, commit.time)
                        )),
                )
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.narrow_compare(id, Some(hash.clone()), window, cx)
                }))
                .test_support()
                .into_any_element()
        })
        .collect();
    let empty = c.commits.is_empty().then(|| {
        div()
            .p(px(10.0 * z))
            .text_size(px(FONT * z))
            .text_color(ui.title_dim)
            .child(format!("{} has nothing {} doesn't", c.branch, c.base))
    });
    div()
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
                .border_b_1()
                .border_color(ui.hairline)
                .child(
                    icon_button(("git-compare-close", n), IconName::ArrowLeft, ui, z)
                        .tooltip(|window, cx| {
                            Tooltip::new("Back to the branches").build(window, cx)
                        })
                        .on_click(
                            cx.listener(move |ws, _: &ClickEvent, _, cx| ws.close_compare(id, cx)),
                        )
                        .test_support(),
                )
                .child(
                    div()
                        .id(("git-compare-all", n))
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(FONT * z))
                        .text_color(ui.title)
                        .cursor_pointer()
                        .child(format!("{} vs {}", c.branch, c.base))
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.narrow_compare(id, None, window, cx)
                        }))
                        .test_support(),
                )
                .child(
                    small(("git-compare-base", n), "Base ▾", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.choose_compare_base(id, window, cx)
                        }))
                        .test_support(),
                ),
        )
        .child(
            div()
                .id(("git-compare-list", n))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .children(rows)
                .children(empty)
                .test_support(),
        )
        .into_any_element()
}

/// Local branches, then the remote ones under a heading that folds them. Each row: the current
/// one checked, the name, ↑↓ against its upstream; under it, the last commit's subject and age.
fn branch_column(
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
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let shown = |remote: bool| -> Vec<&Branch> {
        git.branches
            .iter()
            .filter(|b| {
                b.remote == remote && (query.is_empty() || b.name.to_lowercase().contains(&query))
            })
            .collect()
    };
    let rows = |remote: bool, cx: &mut Context<Workspace>| -> Vec<AnyElement> {
        shown(remote)
            .into_iter()
            .enumerate()
            .map(|(i, b)| {
                let (click, menu) = (b.name.clone(), b.name.clone());
                let sync = match (&b.upstream, b.gone, b.ahead, b.behind) {
                    (Some(_), true, _, _) => "gone".to_string(),
                    (Some(_), false, 0, 0) | (None, ..) => String::new(),
                    (Some(_), false, a, z) => format!("↑{a} ↓{z}"),
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
                    .flex_col()
                    .justify_center()
                    .h(px(2.0 * ROW_H * z))
                    .px(px(10.0 * z))
                    .text_size(px(FONT * z))
                    .text_color(ui.title)
                    .cursor_pointer()
                    .hover(move |st| st.bg(ui.hover))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0 * z))
                            .child(div().w(px(14.0 * z)).flex_shrink_0().when(b.current, |el| {
                                el.child(
                                    Icon::new(IconName::Check)
                                        .size(px(12.0 * z))
                                        .text_color(ui.accent),
                                )
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .when(b.current, |el| {
                                        el.font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    })
                                    .child(b.name.clone()),
                            )
                            .child(div().flex_shrink_0().text_color(ui.title_dim).child(sync)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0 * z))
                            .pl(px(20.0 * z))
                            .text_size(px(11.0 * z))
                            .text_color(ui.title_dim)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .child(b.subject.clone()),
                            )
                            .child(div().flex_shrink_0().child(templum_git::ago(now, b.time))),
                    )
                    .when(!busy, |el| {
                        el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.switch_branch(id, click.clone(), remote, window, cx)
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
                    .into_any_element()
            })
            .collect()
    };
    let heading = |title: String| {
        div()
            .h(px(ROW_H * z))
            .px(px(10.0 * z))
            .flex()
            .items_center()
            .text_size(px(11.0 * z))
            .text_color(ui.title_dim)
            .child(title)
    };
    let remote_count = shown(true).len();
    let folded = git.remote_folded;
    let local_rows = rows(false, cx);
    // The current branch alone: nothing to switch to.
    let no_other = query.is_empty() && git.branches.iter().all(|b| b.remote || b.current);
    let remote_rows = if folded { Vec::new() } else { rows(true, cx) };
    div()
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
                .border_b_1()
                .border_color(ui.hairline)
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
                .child(heading("Local".to_string()))
                .children(local_rows)
                .when(no_other, |el| {
                    el.child(heading("No other branches".to_string()).pl(px(30.0 * z)))
                })
                .child(
                    div()
                        .id(("git-remote-fold", n))
                        .flex()
                        .items_center()
                        .gap(px(4.0 * z))
                        .pl(px(10.0 * z))
                        .cursor_pointer()
                        .hover(move |st| st.bg(ui.hover))
                        .child(
                            Icon::new(if folded {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            })
                            .size(px(12.0 * z))
                            .text_color(ui.title_dim),
                        )
                        .child(heading(format!("Remote · {remote_count}")).px(px(0.0)))
                        .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                            ws.toggle_remote_branches(id, cx)
                        }))
                        .test_support(),
                )
                .children(remote_rows),
        )
        .into_any_element()
}
