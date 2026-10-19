//! The Git popup: its header (the folder, close), and its body: what the folder holds, or the
//! repository's top bar, tabs and shown tab.

use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, KeyDownEvent, ScrollWheelEvent, SharedString,
    TestSupportExt, div, px,
};
use templum_git::Operation;

use super::branches::branches;
use super::changes::changes;
use super::history::history;
use super::stashes::stashes;
use super::widgets::{bar, centered, icon_button};
use super::{FONT, Found, Git, Sync, Tab, words};
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

/// The popup over the canvas's bottom-left corner, `w` × `h` pixels: the folder (a click lists
/// the others), close, and the body.
pub(crate) fn popup(
    ws: &Workspace,
    git: &Git,
    w: f32,
    h: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let folder = git.repo().map_or(&git.folder, |r| &r.root);
    let (name, path) = (git.name(), SharedString::from(folder.display().to_string()));
    let header = bar(ui, 1.0)
        .id("git-popup-header")
        .child(
            div()
                .id("git-folder")
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(2.0))
                .min_w(px(0.0))
                .rounded(px(6.0))
                .text_size(px(13.0))
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(ui.title)
                .cursor_pointer()
                .hover(move |st| st.bg(ui.hover))
                .child(
                    Icon::new(IconName::Folder)
                        .size(px(13.0))
                        .text_color(ui.title_dim),
                )
                .child(div().truncate().child(name))
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(12.0))
                        .text_color(ui.title_dim),
                )
                .tooltip(move |window, cx| Tooltip::new(path.clone()).build(window, cx))
                .on_click(cx.listener(|ws, e: &ClickEvent, window, cx| {
                    ws.open_menu(Target::GitFolders, e.position(), window, cx)
                }))
                .test_support(),
        )
        .child(div().flex_1())
        .child(
            icon_button("git-close", IconName::Close, ui, 1.0)
                .tooltip(|window, cx| Tooltip::new("Close (Esc)").build(window, cx))
                .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| ws.close_git(window, cx)))
                .test_support(),
        );
    div()
        .id("git-popup")
        .w(px(w))
        .h(px(h))
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(ui.surface)
        .text_color(crate::grid::hsla(ws.theme.palette.foreground, 1.0))
        .border_1()
        .border_color(ui.hairline)
        .rounded(px(10.0))
        .shadow(vec![gpui_kit::BoxShadow {
            color: ui.shadow,
            offset: gpui_kit::point(px(0.0), px(12.0)),
            blur_radius: px(32.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        // Presses inside never reach the canvas below.
        .occlude()
        .child(header)
        .child(div().flex_1().min_h(px(0.0)).child(body(ws, git, 1.0, cx)))
        .test_support()
        .into_any_element()
}

/// The body below the header, at the zoom `z`.
pub(crate) fn body(ws: &Workspace, git: &Git, z: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let ui = ws.ui;
    let content: AnyElement = if git.gone {
        centered(ui, z)
            .child(format!("{} is gone", git.folder.display()))
            .child(
                button("choose-folder", "Choose another folder…", ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.choose_git_folder(window, cx)
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
                    button("retry", "Try again", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.look_up_git(window, cx)
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
                            button("init", "Initialize", ui, z)
                                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                    ws.init_git(window, cx)
                                }))
                                .test_support(),
                        )
                        .child({
                            // Spins while a clone runs; a click on it then cancels it.
                            let cloning = git.sync == Some(Sync::Cloning);
                            let label = if cloning {
                                "Cancel"
                            } else {
                                "Clone a repository…"
                            };
                            let el = button("git-clone", label, ui, z).when(cloning, |el| {
                                el.child(Spinner::new().with_size(UiSize::Size(px(12.0 * z))))
                            });
                            if cloning {
                                el.on_click(
                                    cx.listener(move |ws, _: &ClickEvent, _, cx| ws.cancel_git(cx)),
                                )
                            } else {
                                el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                    ws.ask_clone(window, cx)
                                }))
                            }
                            .test_support()
                        })
                        .child(
                            button("choose-folder", "Choose another folder…", ui, z)
                                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                    ws.choose_git_folder(window, cx)
                                }))
                                .test_support(),
                        ),
                )
                .into_any_element(),
            Found::Repo(_) => repo_body(ws, git, z, cx),
        }
    };
    // The wheel scrolls the lists and stops there; with Ctrl it zooms the canvas.
    div()
        .size_full()
        .flex()
        .flex_col()
        .track_focus(&git.focus)
        .on_key_down(cx.listener(move |ws, e: &KeyDownEvent, window, cx| ws.git_key(e, window, cx)))
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        .child(content)
        .into_any_element()
}

/// The repository's view: the top bar, the tabs, the shown tab.
fn repo_body(ws: &Workspace, git: &Git, z: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let ui = ws.ui;
    let s = &git.status;
    let branch: String = match (&s.branch, &s.oid) {
        (Some(b), _) => b.clone(),
        (None, Some(oid)) => format!("detached at {}", &oid[..oid.len().min(7)]),
        (None, None) => "no commits".to_string(),
    };
    let (state, tip) = words::server_state(s);
    // Compare with main: while there is a default branch and this branch isn't it.
    let compare_with = git
        .default_branch
        .as_deref()
        .map(words::local_name)
        .filter(|b| s.branch.as_deref().is_some_and(|c| c != *b))
        .map(str::to_string);
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
        let el = button("git-sync", label, ui, z)
            .when(running, |el| {
                el.child(Spinner::new().with_size(UiSize::Size(px(12.0 * z))))
            })
            .tooltip(move |window, cx| Tooltip::new(words::tip(word)).build(window, cx));
        if running {
            el.on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.cancel_git(cx)))
                .test_support()
                .into_any_element()
        } else if git.acting > 0 {
            el.opacity(0.5).test_support().into_any_element()
        } else {
            let branch = on_branch.clone().unwrap_or_default();
            el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                if publish {
                    ws.publish(branch.clone(), window, cx)
                } else {
                    ws.sync_git(window, cx)
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
    let top = div()
        .id("git-top")
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
                .id("branch")
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
                    ws.open_branch_switcher(window, cx)
                }))
                .test_support(),
        )
        .when(terminals > 0, |el| {
            el.child(
                div()
                    .id("git-terminals")
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
                        ws.open_menu(Target::GitTerminals, e.position(), window, cx)
                    }))
                    .test_support(),
            )
        })
        .child(div().flex_1())
        .when(!state.is_empty(), |el| {
            let tip = SharedString::from(tip.clone());
            el.child(
                div()
                    .id("git-state")
                    .text_size(px(FONT * z))
                    .text_color(ui.title_dim)
                    .child(state.clone())
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)),
            )
        })
        .when_some(compare_with, |el, base| {
            let current = s.branch.clone().unwrap_or_default();
            el.child(
                button("git-compare-main", format!("Compare with {base}"), ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.compare_branch(current.clone(), window, cx)
                    }))
                    .test_support(),
            )
        })
        .children(sync_button)
        .child(
            icon_button("git-more", IconName::Ellipsis, ui, z)
                .tooltip(|window, cx| Tooltip::new("More").build(window, cx))
                .on_click(cx.listener(move |ws, e: &ClickEvent, window, cx| {
                    ws.open_menu(Target::GitMore, e.position(), window, cx)
                }))
                .test_support(),
        )
        .child(
            icon_button("refresh", IconName::RefreshCw, ui, z)
                .on_click(
                    cx.listener(move |ws, _: &ClickEvent, window, cx| ws.refresh_git(window, cx)),
                )
                .test_support(),
        )
        .test_support();
    let detached = s.branch.is_none() && s.oid.is_some();
    let banner = if let Some(op) = git.operation {
        let conflicts = s.conflicts().len();
        // The Git word behind the sentence, explained on hover.
        let word = match op {
            Operation::Rebase => words::tip("Rebase"),
            Operation::Merge => words::tip("Merge"),
            _ => "",
        };
        Some(
            bar(ui, z)
                .id("git-banner")
                .child(
                    div()
                        .id("git-banner-text")
                        .flex_1()
                        .min_w(px(0.0))
                        .child(words::operation_sentence(op, conflicts))
                        .when(!word.is_empty(), |el| {
                            el.tooltip(move |window, cx| Tooltip::new(word).build(window, cx))
                        }),
                )
                .child(
                    button("git-abort", "Abort", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.abort_operation(window, cx)
                        }))
                        .test_support(),
                )
                .when(conflicts == 0, |el| {
                    el.child(
                        button("git-continue", "Continue", ui, z)
                            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                ws.continue_operation(window, cx)
                            }))
                            .test_support(),
                    )
                })
                .test_support(),
        )
    } else if detached {
        let back = git.default_branch.clone();
        Some(
            bar(ui, z)
                .id("git-banner")
                .child(div().flex_1().min_w(px(0.0)).child(
                    "You're looking at an old version. Commits made here won't belong to any branch.",
                ))
                .child(
                    button("git-branch-here", "Create a branch here", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.branch_from(None, window, cx)
                        }))
                        .test_support(),
                )
                .when_some(back, |el, back| {
                    let label = format!("Back to {}", words::local_name(&back));
                    let remote = back.contains('/');
                    el.child(
                        button("git-back", label, ui, z)
                            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                ws.switch_branch(back.clone(), remote, window, cx)
                            }))
                            .test_support(),
                    )
                })
                .test_support(),
        )
    } else {
        None
    };
    let tabs = div()
        .id("git-tabs")
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(28.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .children(Tab::ALL.map(|tab| {
            let active = git.tab == tab;
            div()
                .id(ElementId::Name(format!("git-tab-{}", tab.name()).into()))
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
                    ws.set_git_tab(tab, window, cx)
                }))
                .test_support()
        }));
    let area: AnyElement = match git.tab {
        Tab::Changes => changes(ws, git, z, cx),
        Tab::History => history(ws, git, z, cx),
        Tab::Branches => branches(ws, git, z, cx),
        Tab::Stashes => stashes(ws, git, z, cx),
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
