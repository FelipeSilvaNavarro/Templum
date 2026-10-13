//! The Git block's Changes tab: the conflicts, staged and unstaged lists, and the row's diff.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, TestSupportExt, div,
    px, uniform_list,
};
use templum_canvas::BlockId;
use templum_git::{Change, Entry};

use super::commit_form::commit_form;
use super::diff_view;
use super::widgets::{badge, field, icon_button, small};
use super::{FONT, Git, LISTS_W, List, ROW_H, row_id};
use crate::menu::Target;
use crate::workspace::Workspace;

/// The Changes tab: the lists on the left, the selected row's diff on the right.
pub(super) fn changes(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    // Each list's header, then its rows, all one row tall: a virtual list builds only the ones
    // on screen, however many files changed.
    let items = git.listed(cx);
    let entity = cx.entity();
    let lists = uniform_list(("git-lists", n), items.len(), move |range, _, cx| {
        entity.update(cx, |ws, cx| {
            let ws = &*ws;
            let Some(git) = ws.space.git(id) else {
                return Vec::new();
            };
            let s = &git.status;
            let (conflicts, staged, unstaged) = (s.conflicts(), s.staged(), s.unstaged());
            range
                .map(|ix| {
                    let (which, i) = items[ix];
                    let entries = match which {
                        List::Conflicts => &conflicts,
                        List::Staged => &staged,
                        List::Unstaged => &unstaged,
                    };
                    match i {
                        None => {
                            let shown: Vec<&Entry> = items
                                .iter()
                                .filter(|(w, _)| *w == which)
                                .filter_map(|(_, i)| i.map(|i| entries[i]))
                                .collect();
                            header(ws, id, which, &shown, z, cx)
                        }
                        Some(i) => row(ws, id, which, i, entries[i], z, cx),
                    }
                })
                .collect()
        })
    })
    .track_scroll(&git.lists_scroll)
    .flex_1()
    .min_h(px(0.0));
    let body = div().flex_1().min_h(px(0.0)).flex();
    let body = if git.lists_folded {
        body
    } else {
        let top = div()
            .flex()
            .items_center()
            .gap(px(6.0 * z))
            .p(px(6.0 * z))
            .border_b_1()
            .border_color(ui.hairline)
            .when(git.filtering(), |el| {
                el.child(field(&git.files_filter, z).flex_1())
            })
            .when(!git.filtering(), |el| el.child(div().flex_1()))
            .child(
                icon_button(("git-fold-lists", n), IconName::PanelLeftClose, ui, z)
                    .tooltip(|window, cx| Tooltip::new("Fold the lists").build(window, cx))
                    .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.toggle_lists(id, cx)))
                    .test_support(),
            );
        body.child(
            div()
                .w(px(LISTS_W * z))
                .h_full()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .border_r_1()
                .border_color(ui.hairline)
                .child(top)
                .child(lists)
                .child(commit_form(ws, id, git, z, cx)),
        )
    };
    body.child(diff_view::panel(ws, id, git, z, cx))
        .into_any_element()
}

/// A list's header: its title with the count, and its buttons.
fn header(
    ws: &Workspace,
    id: BlockId,
    which: List,
    entries: &[&Entry],
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let title = match which {
        List::Conflicts => "Conflicts",
        List::Staged => "Staged",
        List::Unstaged => "Changes",
    };
    let paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    let disabled = ws.space.git(id).is_some_and(|g| g.acting > 0) || paths.is_empty();
    let header = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .h(px(ROW_H * z))
        .px(px(10.0 * z))
        .text_size(px(11.0 * z))
        .text_color(ui.title_dim)
        .child(format!("{title} · {}", entries.len()))
        .child(div().flex_1())
        .when(!disabled, |el| match which {
            List::Conflicts => el,
            List::Staged => {
                let paths = paths.clone();
                el.child(
                    small(("git-unstage-all", n), "Unstage all", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.unstage(id, paths.clone(), window, cx)
                        }))
                        .test_support(),
                )
            }
            List::Unstaged => {
                let (stage, discard) = (paths.clone(), paths.clone());
                el.child(
                    small(("git-stage-all", n), "Stage all", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.stage(id, stage.clone(), window, cx)
                        }))
                        .test_support(),
                )
                .child(
                    small(("git-discard-all", n), "Discard all…", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.discard(id, discard.clone(), window, cx)
                        }))
                        .test_support(),
                )
            }
        });
    header.into_any_element()
}

/// A row: the status badge, the name, its folder dimmed, and on hover its buttons. A click
/// shows its diff; a right-click, its menu.
fn row(
    ws: &Workspace,
    id: BlockId,
    which: List,
    i: usize,
    e: &Entry,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let change = match which {
        List::Conflicts => Change::Conflict,
        List::Staged => e.staged.unwrap_or(Change::Modified),
        List::Unstaged => e.unstaged.unwrap_or(Change::Modified),
    };
    let path = e.path.clone();
    let git = ws.space.git(id);
    let selected = git
        .and_then(|g| g.selected.as_ref())
        .is_some_and(|s| s.0 == which && s.1 == e.path);
    let busy = git.is_some_and(|g| g.acting > 0);
    let ids =
        |what: &str| ElementId::Name(format!("git-{what}-{}-{}-{i}", id.0, which.name()).into());
    // Shown while the row is hovered; pressing one never counts as a click on the row.
    let hover_button = |what: &str, icon: IconName, tip: &'static str| {
        icon_button(ids(what), icon, ui, z)
            .invisible()
            .group_hover("git-row", |s| s.visible())
            .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    };
    let mut buttons: Vec<AnyElement> = Vec::new();
    if !busy {
        match which {
            List::Conflicts => {
                let (ours, theirs, resolved) = (path.clone(), path.clone(), path.clone());
                buttons.push(
                    hover_button("ours", IconName::ArrowLeft, "Use ours")
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.resolve(id, ours.clone(), templum_git::Side::Ours, window, cx)
                        }))
                        .test_support()
                        .into_any_element(),
                );
                buttons.push(
                    hover_button("theirs", IconName::ArrowRight, "Use theirs")
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.resolve(id, theirs.clone(), templum_git::Side::Theirs, window, cx)
                        }))
                        .test_support()
                        .into_any_element(),
                );
                buttons.push(
                    hover_button("resolved", IconName::Check, "Mark resolved")
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.stage(id, vec![resolved.clone()], window, cx)
                        }))
                        .test_support()
                        .into_any_element(),
                );
            }
            List::Staged => {
                let unstage = path.clone();
                buttons.push(
                    hover_button("unstage", IconName::Minus, "Unstage")
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.unstage(id, vec![unstage.clone()], window, cx)
                        }))
                        .test_support()
                        .into_any_element(),
                );
            }
            List::Unstaged => {
                let (stage, discard) = (path.clone(), path.clone());
                buttons.push(
                    hover_button("stage", IconName::Plus, "Stage")
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.stage(id, vec![stage.clone()], window, cx)
                        }))
                        .test_support()
                        .into_any_element(),
                );
                buttons.push(
                    hover_button("discard", IconName::Undo2, "Discard changes…")
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.discard(id, vec![discard.clone()], window, cx)
                        }))
                        .test_support()
                        .into_any_element(),
                );
            }
        }
        let open = path.clone();
        buttons.push(
            hover_button("open", IconName::FileCode, "Open")
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.open_from_git(id, &open, window, cx)
                }))
                .test_support()
                .into_any_element(),
        );
    }
    let click = path.clone();
    div()
        .id(row_id(id, which.name(), i))
        .group("git-row")
        .w_full()
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .h(px(ROW_H * z))
        .px(px(10.0 * z))
        .text_size(px(FONT * z))
        .text_color(ui.title)
        .when(selected, |el| el.bg(ui.card))
        .cursor_pointer()
        .hover(move |st| st.bg(ui.hover))
        .child(badge(ws, change, z))
        .child(div().truncate().child(e.name().to_string()))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_color(ui.title_dim)
                .child(e.folder().to_string()),
        )
        .children(buttons)
        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
            ws.select_git_row(id, which, click.clone(), window, cx)
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, ev: &MouseDownEvent, window, cx| {
                ws.open_menu(
                    Target::GitRow {
                        block: id,
                        list: which,
                        index: i,
                    },
                    ev.position,
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .test_support()
        .into_any_element()
}
