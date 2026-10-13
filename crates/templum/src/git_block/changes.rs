//! The Git block's Changes tab: the conflicts, the checklist of changed files (checked: it goes
//! into the next commit), and the diff panel.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, SharedString,
    TestSupportExt, div, px, uniform_list,
};
use templum_canvas::BlockId;
use templum_git::{Change, Entry};

use super::commit_form::commit_form;
use super::diff_view;
use super::widgets::{badge, check_box, field, icon_button, small};
use super::{Check, FONT, Git, LISTS_W, List, ROW_H, row_id, words};
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
            let (conflicts, files) = (s.conflicts(), s.files());
            range
                .map(|ix| {
                    let (which, i) = items[ix];
                    let entries = match which {
                        List::Conflicts => &conflicts,
                        List::Files => &files,
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

/// A list's header: its title with the count; the checklist's has its box (every file shown
/// checked, or none) and Discard all….
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
    let paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    let disabled = ws.space.git(id).is_some_and(|g| g.acting > 0) || paths.is_empty();
    let checks: Vec<Check> = entries.iter().map(|e| Check::of(e)).collect();
    let all = if checks.iter().all(|c| *c == Check::Checked) {
        Check::Checked
    } else if checks.iter().all(|c| *c == Check::Unchecked) {
        Check::Unchecked
    } else {
        Check::Partial
    };
    let unstaged: Vec<String> = entries
        .iter()
        .filter(|e| e.unstaged.is_some())
        .map(|e| e.path.clone())
        .collect();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .h(px(ROW_H * z))
        .px(px(10.0 * z))
        .text_size(px(11.0 * z))
        .text_color(ui.title_dim)
        .when(which == List::Files && !paths.is_empty(), |el| {
            let tip = if all == Check::Checked {
                "Unstage"
            } else {
                "Stage"
            };
            let b = check_box(("git-check-all", n), all, ui, z)
                .tooltip(move |window, cx| Tooltip::new(words::named(tip)).build(window, cx));
            el.child(if disabled {
                b.test_support()
            } else {
                let paths = paths.clone();
                b.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.check_all(id, paths.clone(), window, cx)
                }))
                .test_support()
            })
        })
        .child(match which {
            List::Conflicts => format!("Conflicts · {}", entries.len()),
            List::Files => format!("Changes · {}", entries.len()),
        })
        .child(div().flex_1())
        .when(
            !disabled && which == List::Files && !unstaged.is_empty(),
            |el| {
                el.child(
                    small(("git-discard-all", n), "Discard all…", ui, z)
                        .tooltip(|window, cx| Tooltip::new(words::tip("Discard")).build(window, cx))
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.discard(id, unstaged.clone(), window, cx)
                        }))
                        .test_support(),
                )
            },
        )
        .into_any_element()
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
        // What the working file shows: its unstaged change, else its staged one.
        List::Files => e.unstaged.or(e.staged).unwrap_or(Change::Modified),
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
    let hover_button = |what: &str, icon: IconName, tip: SharedString| {
        icon_button(ids(what), icon, ui, z)
            .invisible()
            .group_hover("git-row", |s| s.visible())
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    };
    let mut buttons: Vec<AnyElement> = Vec::new();
    if !busy {
        match which {
            List::Conflicts => {
                let (ours, theirs, resolved) = (path.clone(), path.clone(), path.clone());
                // Named after the markers' labels: in a rebase "ours" and "theirs" are
                // backwards.
                let (first, second) = git.map_or_else(
                    || ("HEAD".to_string(), "the other side".to_string()),
                    |g| g.conflict_labels(&e.path),
                );
                buttons.push(
                    hover_button(
                        "ours",
                        IconName::ArrowLeft,
                        format!("Use the version from {first}").into(),
                    )
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.resolve_file(id, ours.clone(), templum_git::Side::Ours, window, cx)
                    }))
                    .test_support()
                    .into_any_element(),
                );
                buttons.push(
                    hover_button(
                        "theirs",
                        IconName::ArrowRight,
                        format!("Use the version from {second}").into(),
                    )
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.resolve_file(id, theirs.clone(), templum_git::Side::Theirs, window, cx)
                    }))
                    .test_support()
                    .into_any_element(),
                );
                buttons.push(
                    hover_button("resolved", IconName::Check, "Mark resolved".into())
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.mark_resolved(id, resolved.clone(), window, cx)
                        }))
                        .test_support()
                        .into_any_element(),
                );
            }
            // The box stages and unstages; Discard while something is left unstaged.
            List::Files => {
                if e.unstaged.is_some() {
                    let discard = path.clone();
                    buttons.push(
                        hover_button("discard", IconName::Undo2, words::named("Discard").into())
                            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                ws.discard(id, vec![discard.clone()], window, cx)
                            }))
                            .test_support()
                            .into_any_element(),
                    );
                }
            }
        }
        let open = path.clone();
        buttons.push(
            hover_button("open", IconName::FileCode, "Open".into())
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.open_from_git(id, &open, window, cx)
                }))
                .test_support()
                .into_any_element(),
        );
    }
    let click = path.clone();
    let check = (which == List::Files).then(|| {
        let state = Check::of(e);
        let tip = if state == Check::Checked {
            "Unstage"
        } else {
            "Stage"
        };
        let b = check_box(ids("check"), state, ui, z)
            .tooltip(move |window, cx| Tooltip::new(words::named(tip)).build(window, cx))
            // Its press is the box's, never a click on the row.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        if busy {
            b.test_support()
        } else {
            let path = path.clone();
            b.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                ws.toggle_check(id, path.clone(), window, cx)
            }))
            .test_support()
        }
    });
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
        .children(check)
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
