//! The Git block's body: what the folder holds, or the repository's top bar, tabs and shown tab.

use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, KeyDownEvent, ScrollWheelEvent, SharedString,
    TestSupportExt, div, px,
};
use templum_canvas::BlockId;

use super::branches::branches;
use super::changes::changes;
use super::history::history;
use super::stashes::stashes;
use super::widgets::{bar, centered, icon_button};
use super::{FONT, Found, Git, Tab, words};
use crate::editor::button;
use crate::menu::Target;
use crate::workspace::Workspace;

/// How to get `git` on this OS.
pub(crate) fn install_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "Install it with `xcode-select --install` or `brew install git`."
    } else if cfg!(windows) {
        "Install it with `winget install Git.Git`, or from git-scm.com."
    } else {
        "Install it with your package manager: `sudo pacman -S git`, `sudo apt install git`."
    }
}

/// The body below the header, at the block's zoom `z`.
pub(crate) fn body(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let content: AnyElement = if !git.folder.is_dir() {
        centered(ui, z)
            .child(format!("{} is gone", git.folder.display()))
            .child(
                button(("choose-folder", n), "Choose another folder…", ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.choose_git_folder(id, window, cx)
                    }))
                    .test_support(),
            )
            .into_any_element()
    } else {
        match &git.found {
            Found::Looking => centered(ui, z)
                .child("Looking for the repository…")
                .into_any_element(),
            Found::NoGit(err) => centered(ui, z)
                .child(format!(
                    "Couldn't run {}: {}",
                    ws.config.git.program(),
                    err.first_line()
                ))
                .child(install_hint())
                .child(
                    button(("retry", n), "Try again", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.look_up_git(id, window, cx)
                        }))
                        .test_support(),
                )
                .into_any_element(),
            Found::NotARepo => centered(ui, z)
                .child(format!("{} is not a Git repository", git.folder.display()))
                .child(
                    div()
                        .flex()
                        .gap(px(8.0 * z))
                        .child(
                            button(("init", n), "Initialize", ui, z)
                                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                    ws.init_git(id, window, cx)
                                }))
                                .test_support(),
                        )
                        .child(
                            button(("choose-folder", n), "Choose another folder…", ui, z)
                                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                    ws.choose_git_folder(id, window, cx)
                                }))
                                .test_support(),
                        ),
                )
                .into_any_element(),
            Found::Repo(_) => repo_body(ws, id, git, z, cx),
        }
    };
    // The wheel scrolls the lists and stops there; with Ctrl it zooms the canvas.
    div()
        .size_full()
        .flex()
        .flex_col()
        .track_focus(&git.focus)
        .on_key_down(
            cx.listener(move |ws, e: &KeyDownEvent, window, cx| ws.git_key(id, e, window, cx)),
        )
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        .child(content)
        .into_any_element()
}

/// The repository's view: the top bar, the tabs, the shown tab.
fn repo_body(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let s = &git.status;
    let branch: String = match (&s.branch, &s.oid) {
        (Some(b), _) => b.clone(),
        (None, Some(oid)) => format!("detached at {}", &oid[..oid.len().min(7)]),
        (None, None) => "no commits".to_string(),
    };
    let (state, tip) = words::server_state(s);
    let on_branch = s.branch.clone().filter(|_| s.oid.is_some());
    let publish = s.upstream.is_none() || s.gone;
    let running = git.sync.is_some();
    // The running command's button spins, and a click on it cancels it (Fetch and Pull from ⋯
    // spin here too).
    let sync_button = (on_branch.is_some() || running).then(|| {
        let (label, word) = match (running, publish) {
            (true, _) => ("Cancel", "Sync"),
            (false, true) => ("Publish branch", "Push"),
            (false, false) => ("Sync", "Sync"),
        };
        let el = button(("git-sync", n), label, ui, z)
            .when(running, |el| {
                el.child(Spinner::new().with_size(UiSize::Size(px(12.0 * z))))
            })
            .tooltip(move |window, cx| Tooltip::new(words::tip(word)).build(window, cx));
        if running {
            el.on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.cancel_git(id, cx)))
                .test_support()
                .into_any_element()
        } else if git.acting > 0 {
            el.opacity(0.5).test_support().into_any_element()
        } else {
            let branch = on_branch.clone().unwrap_or_default();
            el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                if publish {
                    ws.publish(id, branch.clone(), window, cx)
                } else {
                    ws.sync_git(id, window, cx)
                }
            }))
            .test_support()
            .into_any_element()
        }
    });
    // The terminals working in the repository, as their last snapshot saw them: cheap.
    let terminals = git.repo().map_or(0, |repo| {
        ws.space
            .terminals()
            .filter(|(_, pane)| pane.works_in(&repo.root))
            .count()
    });
    let top =
        div()
            .id(("git-top", n))
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(8.0 * z))
            .h(px(32.0 * z))
            .px(px(8.0 * z))
            .border_b_1()
            .border_color(ui.hairline)
            .child(
                div()
                    .id(("branch", n))
                    .flex()
                    .items_center()
                    .gap(px(4.0 * z))
                    .px(px(8.0 * z))
                    .py(px(2.0 * z))
                    .rounded_full()
                    .bg(ui.card)
                    .text_size(px(FONT * z))
                    .text_color(ui.title)
                    .cursor_pointer()
                    .hover(move |st| st.bg(ui.hover))
                    .child(
                        Icon::new(IconName::GitBranch)
                            .size(px(12.0 * z))
                            .text_color(ui.title_dim),
                    )
                    .child(div().truncate().child(branch))
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.set_git_tab(id, Tab::Branches, window, cx)
                    }))
                    .test_support(),
            )
            .when(terminals > 0, |el| {
                el.child(
                    div()
                        .id(("git-terminals", n))
                        .flex()
                        .items_center()
                        .gap(px(4.0 * z))
                        .px(px(8.0 * z))
                        .py(px(2.0 * z))
                        .rounded_full()
                        .bg(ui.card)
                        .text_size(px(FONT * z))
                        .text_color(ui.title)
                        .cursor_pointer()
                        .hover(move |st| st.bg(ui.hover))
                        .child(
                            Icon::new(IconName::SquareTerminal)
                                .size(px(12.0 * z))
                                .text_color(ui.title_dim),
                        )
                        .child(terminals.to_string())
                        .tooltip(|window, cx| {
                            Tooltip::new("Terminals in this repository").build(window, cx)
                        })
                        .on_click(cx.listener(move |ws, e: &ClickEvent, window, cx| {
                            ws.open_menu(Target::GitTerminals(id), e.position(), window, cx)
                        }))
                        .test_support(),
                )
            })
            .child(div().flex_1())
            .when(!state.is_empty(), |el| {
                let tip = SharedString::from(tip.clone());
                el.child(
                    div()
                        .id(("git-state", n))
                        .text_size(px(FONT * z))
                        .text_color(ui.title_dim)
                        .child(state.clone())
                        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)),
                )
            })
            .children(sync_button)
            .child(
                icon_button(("git-more", n), IconName::Ellipsis, ui, z)
                    .tooltip(|window, cx| Tooltip::new("More").build(window, cx))
                    .on_click(cx.listener(move |ws, e: &ClickEvent, window, cx| {
                        ws.open_menu(Target::GitMore(id), e.position(), window, cx)
                    }))
                    .test_support(),
            )
            .child(
                icon_button(("refresh", n), IconName::RefreshCw, ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.refresh_git(id, window, cx)
                    }))
                    .test_support(),
            )
            .test_support();
    let banner = git.operation.map(|op| {
        let can_continue = git.status.conflicts().is_empty();
        bar(ui, z)
            .id(("git-banner", n))
            .child(format!("{} in progress", capitalized(op.name())))
            .child(div().flex_1())
            .child(
                button(("git-abort", n), "Abort", ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.abort_operation(id, window, cx)
                    }))
                    .test_support(),
            )
            .when(can_continue, |el| {
                el.child(
                    button(("git-continue", n), "Continue", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.continue_operation(id, window, cx)
                        }))
                        .test_support(),
                )
            })
            .test_support()
    });
    let tabs = div()
        .id(("git-tabs", n))
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(28.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .children(Tab::ALL.map(|tab| {
            let active = git.tab == tab;
            div()
                .id(ElementId::Name(
                    format!("git-tab-{n}-{}", tab.name()).into(),
                ))
                .flex()
                .items_center()
                .h_full()
                .px(px(12.0 * z))
                .text_size(px(FONT * z))
                .text_color(if active { ui.title } else { ui.title_dim })
                .when(active, |el| el.bg(ui.card))
                .cursor_pointer()
                .hover(move |st| st.bg(ui.hover))
                .child(tab.title())
                .when(tab == Tab::Stashes, |el| {
                    el.tooltip(|window, cx| Tooltip::new(words::tip("Stash")).build(window, cx))
                })
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.set_git_tab(id, tab, window, cx)
                }))
                .test_support()
        }));
    let area: AnyElement = match git.tab {
        Tab::Changes => changes(ws, id, git, z, cx),
        Tab::History => history(ws, id, git, z, cx),
        Tab::Branches => branches(ws, id, git, z, cx),
        Tab::Stashes => stashes(ws, id, git, z, cx),
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(top)
        .children(banner)
        .when_some(git.error.clone(), |el, e| el.child(bar(ui, z).child(e)))
        .child(tabs)
        .child(area)
        .into_any_element()
}

/// "merge" → "Merge".
fn capitalized(word: &str) -> String {
    let mut c = word.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
