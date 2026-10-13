//! The Git block's History tab: on the left the commits (with their graph) or the commit shown
//! (its message, author, date and files); on the right the diff panel.
//! See docs/specs/git.md, section 6.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, TestSupportExt, div,
    px, uniform_list,
};
use templum_canvas::BlockId;

use super::widgets::{badge, bar, field, icon_button};
use super::{FONT, Git, LISTS_W, ROW_H, Shown, diff_view};
use crate::editor::button;
use crate::menu::Target;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// The History tab: the list (or the commit shown) on the left, the diff panel on the right.
pub(super) fn history(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let column = match &git.shown {
        Some(shown) => commit_column(ws, id, shown, z, cx),
        None => list_column(ws, id, git, z, cx),
    };
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .when(!git.lists_folded, |el| el.child(column))
        .child(diff_view::panel(ws, id, git, z, cx))
        .into_any_element()
}

/// The left column: the search and the fold button over the commits' virtual list, each row
/// two lines (subject and refs; short hash, author, age).
fn list_column(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let commits = git.commits.clone();
    let filtered = Rc::new(git.filtered(cx));
    let (more, entity) = (git.more, cx.entity());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let mono = ws.cell.font.family.clone();
    let rows = filtered.len() + usize::from(more);
    let list = uniform_list(("git-history", n), rows, move |range, _, _| {
        range
            .map(|ix| {
                if ix == filtered.len() {
                    let entity = entity.clone();
                    return button(("git-load-more", n), "Load more", ui, z)
                        .m(px(6.0 * z))
                        .on_click(move |_: &ClickEvent, window, cx| {
                            entity.update(cx, |ws, cx| {
                                let at = ws.here(id);
                                let skip = ws.space.git(id).map_or(0, |g| g.commits.len());
                                ws.load_history(at, skip, window, cx)
                            })
                        })
                        .test_support()
                        .into_any_element();
                }
                let c = &commits[filtered[ix]];
                let (open, menu, hash) = (entity.clone(), entity.clone(), c.hash.clone());
                let menu_hash = hash.clone();
                div()
                    .id(("git-commit", ix))
                    .flex()
                    .items_center()
                    .h(px(2.0 * ROW_H * z))
                    .px(px(6.0 * z))
                    .gap(px(6.0 * z))
                    .text_size(px(FONT * z))
                    .text_color(ui.title)
                    .cursor_pointer()
                    .hover(move |st| st.bg(ui.hover))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0 * z))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .truncate()
                                            .child(c.subject.clone()),
                                    )
                                    .children(c.refs.iter().map(|r| ref_badge(r, ui, z))),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.0 * z))
                                    .text_size(px(11.0 * z))
                                    .text_color(ui.title_dim)
                                    .child(div().font_family(mono.clone()).child(c.short.clone()))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .truncate()
                                            .child(c.author.clone()),
                                    )
                                    .child(
                                        div().flex_shrink_0().child(templum_git::ago(now, c.time)),
                                    ),
                            ),
                    )
                    .on_click(move |_: &ClickEvent, window, cx| {
                        open.update(cx, |ws, cx| ws.show_commit(id, hash.clone(), window, cx))
                    })
                    .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
                        menu.update(cx, |ws, cx| {
                            ws.open_menu(
                                Target::GitCommit {
                                    block: id,
                                    hash: menu_hash.clone(),
                                },
                                e.position,
                                window,
                                cx,
                            )
                        });
                        cx.stop_propagation();
                    })
                    .test_support()
                    .into_any_element()
            })
            .collect()
    })
    .flex_1()
    .min_h(px(0.0));
    let top = div()
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .p(px(6.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .child(field(&git.search, z).flex_1())
        .child(fold_button(id, ui, z, cx));
    column(ui, z).child(top).child(list).into_any_element()
}

/// The commit shown: ← back to the list, its message, author and date, and its files (a click
/// scrolls the panel to one). The whole column scrolls: a long message never hides the files.
fn commit_column(
    ws: &Workspace,
    id: BlockId,
    shown: &Shown,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let c = &shown.commit;
    let files = shown
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (path, menu_path, hash) = (f.path.clone(), f.path.clone(), c.hash.clone());
            let active = shown.file.as_deref() == Some(&f.path);
            div()
                .id(ElementId::Name(format!("git-commit-file-{n}-{i}").into()))
                .flex()
                .items_center()
                .gap(px(6.0 * z))
                .h(px(ROW_H * z))
                .px(px(10.0 * z))
                .text_size(px(FONT * z))
                .text_color(ui.title)
                .when(active, |el| el.bg(ui.card))
                .cursor_pointer()
                .hover(move |st| st.bg(ui.hover))
                .child(badge(ws, f.change, z))
                .child(div().truncate().child(f.path.clone()))
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                    ws.show_commit_file(id, path.clone(), cx)
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        ws.open_menu(
                            Target::GitCommitFile {
                                block: id,
                                hash: hash.clone(),
                                path: menu_path.clone(),
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
        .collect::<Vec<_>>();
    let top = bar(ui, z)
        .child(
            icon_button(("git-commit-back", n), IconName::ArrowLeft, ui, z)
                .tooltip(|window, cx| Tooltip::new("Back to the commits").build(window, cx))
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.hide_commit(id, cx)))
                .test_support(),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_color(ui.title)
                .child(format!("Commit {}", c.short)),
        )
        .child(fold_button(id, ui, z, cx));
    let details = div()
        .flex()
        .flex_col()
        .gap(px(4.0 * z))
        .p(px(10.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .text_size(px(FONT * z))
        .text_color(ui.title)
        .child(
            div()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .whitespace_normal()
                .child(c.subject.clone()),
        )
        .when(!c.body.is_empty(), |el| {
            el.child(
                div()
                    .text_color(ui.title_dim)
                    .whitespace_normal()
                    .child(c.body.clone()),
            )
        })
        .child(
            div()
                .text_color(ui.title_dim)
                .whitespace_normal()
                .child(format!("{} <{}>", c.author, c.email)),
        )
        .child(div().text_color(ui.title_dim).child(c.date.clone()));
    column(ui, z)
        .child(top)
        .child(
            div()
                .id(("git-commit-details", n))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .child(details)
                .children(files),
        )
        .into_any_element()
}

/// The left column's frame.
fn column(ui: Ui, z: f32) -> gpui_kit::Div {
    div()
        .w(px(LISTS_W * z))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(ui.hairline)
}

/// Folds the left column: the panel takes the whole width.
fn fold_button(
    id: BlockId,
    ui: Ui,
    z: f32,
    cx: &mut Context<Workspace>,
) -> impl IntoElement + use<> {
    icon_button(
        ("git-fold-lists", id.0 as usize),
        IconName::PanelLeftClose,
        ui,
        z,
    )
    .tooltip(|window, cx| Tooltip::new("Fold the lists").build(window, cx))
    .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.toggle_lists(id, cx)))
    .test_support()
}

/// "HEAD -> main" is HEAD and main; "tag: v1" is a tag; the rest are branches.
fn ref_badge(r: &str, ui: Ui, z: f32) -> AnyElement {
    let (label, tag) = match r.strip_prefix("tag: ") {
        Some(t) => (t.to_string(), true),
        None => (r.replace("HEAD -> ", "HEAD, "), false),
    };
    div()
        .flex_shrink_0()
        .px(px(5.0 * z))
        .rounded(px(4.0 * z))
        .border_1()
        .border_color(if tag { ui.accent } else { ui.hairline })
        .text_size(px(10.0 * z))
        .text_color(ui.title_dim)
        .child(label)
        .into_any_element()
}
