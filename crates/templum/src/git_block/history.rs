//! The Git block's History tab: the commits, a search, and the shown commit with its files.

use std::rc::Rc;

use gpui_kit::component::input::Editor as EditorView;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, TestSupportExt, div,
    px, relative, uniform_list,
};
use templum_canvas::BlockId;

use super::widgets::{badge_color, centered, field};
use super::{FONT, Git, ROW_H};
use crate::editor::{button, keep_rows};
use crate::menu::Target;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// The History tab: the search and the virtual list on the left, the shown commit on the right.
pub(super) fn history(
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
                    .gap(px(8.0 * z))
                    .h(px(ROW_H * z))
                    .px(px(10.0 * z))
                    .text_size(px(FONT * z))
                    .text_color(ui.title)
                    .cursor_pointer()
                    .hover(move |st| st.bg(ui.hover))
                    .child(
                        div()
                            .font_family(mono.clone())
                            .text_color(ui.title_dim)
                            .child(c.short.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .child(c.subject.clone()),
                    )
                    .children(c.refs.iter().map(|r| ref_badge(r, ui, z)))
                    .child(div().text_color(ui.title_dim).child(c.author.clone()))
                    .child(
                        div()
                            .text_color(ui.title_dim)
                            .child(templum_git::ago(now, c.time)),
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
    let left = div()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(ui.hairline)
        .child(div().p(px(6.0 * z)).child(field(&git.search, z).w_full()))
        .child(list);
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .child(left)
        .child(commit_details(ws, id, git, z, cx))
        .into_any_element()
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

/// The shown commit: its message, author and date, its files, and the clicked file's diff.
fn commit_details(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let Some(shown) = &git.shown else {
        return centered(ui, z)
            .flex_1()
            .child("Click a commit")
            .into_any_element();
    };
    let c = &shown.commit;
    keep_rows(&git.commit_diff, FONT, z, cx);
    let files = shown.files.iter().enumerate().map(|(i, f)| {
        let path = f.path.clone();
        let active = shown.file.as_deref() == Some(&f.path);
        div()
            .id(ElementId::Name(format!("git-commit-file-{n}-{i}").into()))
            .flex()
            .items_center()
            .gap(px(6.0 * z))
            .h(px(ROW_H * z))
            .px(px(10.0 * z))
            .text_size(px(FONT * z))
            .when(active, |el| el.bg(ui.card))
            .cursor_pointer()
            .hover(move |st| st.bg(ui.hover))
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(16.0 * z))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0 * z))
                    .bg(badge_color(ws, f.change))
                    .text_size(px(10.0 * z))
                    .text_color(ui.canvas)
                    .child(f.change.letter().to_string()),
            )
            .child(div().truncate().child(f.path.clone()))
            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                ws.show_commit_file(id, path.clone(), window, cx)
            }))
            .test_support()
    });
    div()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0 * z))
                .p(px(10.0 * z))
                .border_b_1()
                .border_color(ui.hairline)
                .text_size(px(FONT * z))
                .child(
                    div()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child(c.subject.clone()),
                )
                .when(!c.body.is_empty(), |el| {
                    el.child(div().text_color(ui.title_dim).child(c.body.clone()))
                })
                .child(div().text_color(ui.title_dim).child(format!(
                    "{} <{}> · {} · {}",
                    c.author, c.email, c.date, c.short
                ))),
        )
        .child(div().flex_shrink_0().children(files))
        .child(
            div().flex_1().min_h(px(0.0)).child(
                EditorView::new(&git.commit_diff)
                    .appearance(false)
                    .bordered(false)
                    .readonly(true)
                    .font_family(ws.cell.font.family.clone())
                    .text_size(px(FONT * z))
                    .h(relative(1.0)),
            ),
        )
        .into_any_element()
}
