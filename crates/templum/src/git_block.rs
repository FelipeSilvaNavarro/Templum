//! A Git block: one repository, through the user's own `git`, run in the background. The
//! changes with their diffs, commits, history, branches, stashes, fetch, pull and push.
//! See docs/specs/cockpit.md, section 8.

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, Div, ElementId, ScrollWheelEvent, SharedString, Stateful,
    TestSupportExt, div, px,
};
use templum_canvas::{BlockId, Rect, Size};
use templum_git::{Change, Entry, GitError, Operation, Repo, Status};

use crate::editor::button;
use crate::grid::hsla;
use crate::session;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// A new Git block's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size { w: 800.0, h: 560.0 };
pub(crate) const MIN: Size = Size { w: 480.0, h: 320.0 };
/// The lists' column width and the rows' height, world units.
pub(crate) const LISTS_W: f32 = 300.0;
pub(crate) const ROW_H: f32 = 24.0;
/// The text's size at zoom 1, world units.
pub(crate) const FONT: f32 = 12.0;
/// How often the status is read while the block is on screen.
pub(crate) const POLL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    Changes,
    History,
    Branches,
    Stashes,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Changes, Tab::History, Tab::Branches, Tab::Stashes];

    /// As the canvas file names it.
    pub fn name(self) -> &'static str {
        match self {
            Tab::Changes => "changes",
            Tab::History => "history",
            Tab::Branches => "branches",
            Tab::Stashes => "stashes",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::Changes => "Changes",
            Tab::History => "History",
            Tab::Branches => "Branches",
            Tab::Stashes => "Stashes",
        }
    }

    /// Changes for a name it doesn't know.
    pub fn from_name(name: &str) -> Tab {
        Tab::ALL
            .into_iter()
            .find(|t| t.name() == name)
            .unwrap_or_default()
    }
}

/// What the folder turned out to hold.
pub(crate) enum Found {
    /// The look-up hasn't come back yet.
    Looking,
    /// `git` couldn't run.
    NoGit(GitError),
    NotARepo,
    Repo(Repo),
}

/// The three lists of the Changes tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum List {
    Conflicts,
    Staged,
    Unstaged,
}

impl List {
    pub fn name(self) -> &'static str {
        match self {
            List::Conflicts => "conflicts",
            List::Staged => "staged",
            List::Unstaged => "unstaged",
        }
    }
}

/// One refresh's result, read in the background.
pub(crate) struct Read {
    pub status: Status,
    pub operation: Option<Operation>,
    pub author: Option<(String, String)>,
}

impl Read {
    pub fn of(repo: &Repo) -> Result<Read, GitError> {
        Ok(Read {
            status: repo.status()?,
            operation: repo.operation(),
            author: repo.author()?,
        })
    }
}

/// A Git block's state: the folder, what it holds, and the last read.
pub(crate) struct Git {
    /// What the user chose; the repository's root may be above it.
    pub folder: PathBuf,
    pub found: Found,
    pub tab: Tab,
    pub status: Status,
    pub operation: Option<Operation>,
    /// `user.name` and `user.email`; None: not set, so Commit is disabled.
    pub author: Option<(String, String)>,
    /// A command runs in the background: reads wait, actions are disabled.
    pub busy: bool,
    /// A refresh was asked while busy: one runs when the command ends.
    pub pending: bool,
    /// Reads completed so far (the tests count them).
    pub reads: u64,
    /// Why the last read failed, shown under the top bar.
    pub error: Option<String>,
}

impl Git {
    pub fn new(folder: PathBuf, tab: Tab) -> Git {
        Git {
            folder,
            found: Found::Looking,
            tab,
            status: Status::default(),
            operation: None,
            author: None,
            busy: false,
            pending: false,
            reads: 0,
            error: None,
        }
    }

    pub fn repo(&self) -> Option<&Repo> {
        match &self.found {
            Found::Repo(repo) => Some(repo),
            _ => None,
        }
    }

    /// The repository's folder name, else the chosen folder's.
    pub fn name(&self) -> String {
        let dir = self.repo().map_or(&self.folder, |r| &r.root);
        dir.file_name().map_or_else(
            || dir.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    pub fn title(&self) -> SharedString {
        self.name().into()
    }

    pub fn item(&self, rect: Rect) -> session::GitItem {
        session::GitItem {
            rect: rect.into(),
            folder: self.folder.to_string_lossy().into_owned(),
            tab: self.tab.name().to_string(),
        }
    }
}

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

pub(crate) fn row_id(block: BlockId, list: &str, i: usize) -> ElementId {
    ElementId::Name(format!("git-row-{}-{list}-{i}", block.0).into())
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
                    ws.config.git.program,
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
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        .child(content)
        .into_any_element()
}

/// A message in the middle of the block.
pub(crate) fn centered(ui: Ui, z: f32) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.0 * z))
        .px(px(16.0 * z))
        .text_size(px(13.0 * z))
        .text_color(ui.title_dim)
}

/// A line across the block, for what holds for the whole repository.
pub(crate) fn bar(ui: Ui, z: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .px(px(10.0 * z))
        .py(px(4.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .bg(ui.card)
        .text_size(px(FONT * z))
        .text_color(ui.title_dim)
}

/// A small square button holding an icon.
pub(crate) fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    ui: Ui,
    z: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0 * z))
        .rounded(px(5.0 * z))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(Icon::new(icon).size(px(13.0 * z)).text_color(ui.title))
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
    let sync = match (&s.upstream, &s.oid) {
        (Some(_), _) => format!("↑{} ↓{}", s.ahead, s.behind),
        (None, Some(_)) => "no upstream".to_string(),
        (None, None) => String::new(),
    };
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
                    .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| {
                        ws.set_git_tab(id, Tab::Branches, cx)
                    }))
                    .test_support(),
            )
            .child(
                div()
                    .text_size(px(FONT * z))
                    .text_color(ui.title_dim)
                    .child(sync),
            )
            .child(div().flex_1())
            // Fetch, Pull and Push come with task 6.
            .child(
                icon_button(("refresh", n), IconName::RefreshCw, ui, z)
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.refresh_git(id, window, cx)
                    }))
                    .test_support(),
            )
            .test_support();
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
                .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.set_git_tab(id, tab, cx)))
                .test_support()
        }));
    let area: AnyElement = match git.tab {
        Tab::Changes => changes(ws, id, git, z, cx),
        // Tasks 7, 8 and 9.
        _ => div().flex_1().into_any_element(),
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(top)
        .when_some(git.error.clone(), |el, e| el.child(bar(ui, z).child(e)))
        .child(tabs)
        .child(area)
        .into_any_element()
}

/// The Changes tab: the lists on the left, the diff on the right (task 4).
fn changes(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let s = &git.status;
    let lists = div()
        .id(("git-lists", n))
        .w(px(LISTS_W * z))
        .h_full()
        .flex_shrink_0()
        .border_r_1()
        .border_color(ui.hairline)
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .flex_col()
                .when(!s.conflicts().is_empty(), |el| {
                    el.child(list(ws, id, List::Conflicts, &s.conflicts(), z, cx))
                })
                .child(list(ws, id, List::Staged, &s.staged(), z, cx))
                .child(list(ws, id, List::Unstaged, &s.unstaged(), z, cx)),
        )
        .test_support();
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .child(lists)
        // The diff view: task 4.
        .child(div().flex_1())
        .into_any_element()
}

/// One list: its header with the count, then a row per entry.
fn list(
    ws: &Workspace,
    id: BlockId,
    which: List,
    entries: &[&Entry],
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let title = match which {
        List::Conflicts => "Conflicts",
        List::Staged => "Staged",
        List::Unstaged => "Changes",
    };
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .h(px(ROW_H * z))
                .px(px(10.0 * z))
                .text_size(px(11.0 * z))
                .text_color(ui.title_dim)
                .child(format!("{title} · {}", entries.len())),
        )
        .children(
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| row(ws, id, which, i, e, z, cx)),
        )
        .into_any_element()
}

/// The badge's color for a change: the terminal's palette, so it fits the theme.
fn badge_color(ws: &Workspace, change: Change) -> gpui_kit::Hsla {
    let ansi = &ws.theme.palette.ansi;
    hsla(
        match change {
            Change::Modified => ansi[3],
            Change::Added | Change::Untracked => ansi[2],
            Change::Deleted | Change::Conflict => ansi[1],
            Change::Renamed => ansi[4],
            Change::TypeChanged => ansi[5],
        },
        1.0,
    )
}

/// A row: the status badge, the name, its folder dimmed.
fn row(
    ws: &Workspace,
    id: BlockId,
    which: List,
    i: usize,
    e: &Entry,
    z: f32,
    _cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let change = match which {
        List::Conflicts => Change::Conflict,
        List::Staged => e.staged.unwrap_or(Change::Modified),
        List::Unstaged => e.unstaged.unwrap_or(Change::Modified),
    };
    div()
        .id(row_id(id, which.name(), i))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .h(px(ROW_H * z))
        .px(px(10.0 * z))
        .text_size(px(FONT * z))
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |st| st.bg(ui.hover))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .size(px(16.0 * z))
                .rounded(px(4.0 * z))
                .bg(badge_color(ws, change))
                .text_size(px(10.0 * z))
                .text_color(ui.canvas)
                .child(change.letter().to_string()),
        )
        .child(div().truncate().child(e.name().to_string()))
        .child(
            div()
                .truncate()
                .text_color(ui.title_dim)
                .child(e.folder().to_string()),
        )
        .test_support()
        .into_any_element()
}
