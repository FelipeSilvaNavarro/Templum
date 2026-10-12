//! A Git block: one repository, through the user's own `git`, run in the background. The
//! changes with their diffs, commits, history, branches, stashes, fetch, pull and push.
//! See docs/specs/cockpit.md, section 8.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use gpui_kit::Focusable as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{EditorState, InputEvent, InputState, TextareaState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, EntityId, ScrollWheelEvent,
    SharedString, Subscription, TestSupportExt, Window, div, px,
};
use templum_canvas::{BlockId, Rect, Size};
use templum_git::{Branch, Commit, CommitFile, Entry, GitError, Operation, Repo, Stash, Status};

use crate::editor::button;
use crate::session;
use crate::workspace::Workspace;
use branches::branches;
use changes::changes;
use history::history;
use stashes::stashes;
use widgets::{bar, centered, icon_button};

mod branches;
mod changes;
mod commit_form;
mod history;
mod stashes;
mod widgets;

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
/// How many commits History loads at a time.
pub(crate) const PAGE: usize = 200;

/// The commit shown on the right of the History tab.
pub(crate) struct Shown {
    pub commit: Commit,
    pub files: Vec<CommitFile>,
    /// The file whose diff is shown.
    pub file: Option<String>,
}

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

/// Which of fetch, pull and push runs: its button shows a spinner, the others are disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sync {
    Fetch,
    Pull,
    Push,
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

/// A command waiting for the block's running one.
pub(crate) type Queued = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

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
    /// Actions asked and not done yet (reads don't count): while any, the action buttons are
    /// disabled.
    pub acting: u32,
    /// Kills the running fetch, pull or push (its spinning button).
    pub cancel: Option<Arc<AtomicBool>>,
    /// A refresh was asked while busy: one runs when the command ends.
    pub pending: bool,
    /// Other commands asked while busy, run in turn as each one ends.
    pub queued: Vec<Queued>,
    /// Reads completed so far (the tests count them).
    pub reads: u64,
    /// The fetch, pull or push running.
    pub sync: Option<Sync>,
    /// Why the last read failed, shown under the top bar.
    pub error: Option<String>,
    /// The read-only diff on the right of the Changes tab, with the `diff` grammar.
    pub diff: Entity<EditorState>,
    /// History's file diff and Stashes' diff: each tab has its own, so a 2-second read of the
    /// Changes diff never replaces what another tab shows.
    pub commit_diff: Entity<EditorState>,
    pub stash_diff: Entity<EditorState>,
    /// The row whose diff is shown.
    pub selected: Option<(List, String)>,
    /// The diff's first hunk's line in the new file, for Open in editor.
    pub diff_line: Option<u32>,
    pub summary: Entity<InputState>,
    pub description: Entity<TextareaState>,
    /// Commit replaces the last commit; the fields were filled with its message.
    pub amend: bool,
    /// Ctrl+Enter in either field commits.
    _enter: Vec<Subscription>,
    /// The loaded commits, newest first (an Rc: the virtual list's closure holds them).
    pub commits: Rc<Vec<Commit>>,
    /// The last page was full: Load more shows.
    pub more: bool,
    pub search: Entity<InputState>,
    pub shown: Option<Shown>,
    /// A list read was asked while a command ran.
    pub pending_lists: bool,
    pub branches: Vec<Branch>,
    pub stashes: Vec<Stash>,
    /// The stash whose diff is shown, by its index in `stash@{N}`.
    pub stash_shown: Option<usize>,
    /// The Branches tab's filter.
    pub filter: Entity<InputState>,
    _filter: Subscription,
    _search: Subscription,
}

impl Git {
    pub fn new(folder: PathBuf, tab: Tab, window: &mut Window, cx: &mut Context<Workspace>) -> Git {
        let mut diff_view = || {
            cx.new(|cx| {
                EditorState::new(window, cx)
                    .language("diff")
                    .line_number(false)
                    .soft_wrap(false)
                    // Its fold icons have a fixed size, which breaks the layout at a zoom.
                    .folding(false)
            })
        };
        let (diff, commit_diff, stash_diff) = (diff_view(), diff_view(), diff_view());
        let summary = cx.new(|cx| InputState::new(window, cx).placeholder("Summary"));
        let description = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .placeholder("Description")
        });
        let (from_summary, from_description) = (summary.entity_id(), description.entity_id());
        let _enter = vec![
            cx.subscribe_in(
                &summary,
                window,
                move |ws, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter {
                        secondary: true, ..
                    } = event
                    {
                        ws.commit_from(from_summary, window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &description,
                window,
                move |ws, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter {
                        secondary: true, ..
                    } = event
                    {
                        ws.commit_from(from_description, window, cx);
                    }
                },
            ),
        ];
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search subject or author"));
        let _search = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify()
            }
        });
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter branches"));
        let _filter = cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify()
            }
        });
        Git {
            folder,
            found: Found::Looking,
            tab,
            status: Status::default(),
            operation: None,
            author: None,
            busy: false,
            acting: 0,
            cancel: None,
            pending: false,
            queued: Vec::new(),
            reads: 0,
            sync: None,
            error: None,
            diff,
            commit_diff,
            stash_diff,
            selected: None,
            diff_line: None,
            summary,
            description,
            amend: false,
            _enter,
            commits: Rc::new(Vec::new()),
            more: false,
            search,
            shown: None,
            pending_lists: false,
            branches: Vec::new(),
            stashes: Vec::new(),
            stash_shown: None,
            filter,
            _filter,
            _search,
        }
    }

    /// The commits matching the search (subject or author, without case): their indices.
    pub fn filtered(&self, cx: &App) -> Vec<usize> {
        let query = self.search.read(cx).value().trim().to_lowercase();
        self.commits
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                query.is_empty()
                    || c.subject.to_lowercase().contains(&query)
                    || c.author.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The keyboard is in the summary or the description.
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        self.summary
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx)
            || self
                .description
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
    }

    /// The summary, then a blank line and the description when there is one.
    pub fn message(&self, cx: &App) -> String {
        let summary = self.summary.read(cx).value().trim().to_string();
        let description = self.description.read(cx).value().trim().to_string();
        if description.is_empty() {
            summary
        } else {
            format!("{summary}\n\n{description}")
        }
    }

    /// Commit is possible: something staged, a summary, an author, nothing running. With
    /// Amend, nothing staged is fine (the message alone changes).
    pub fn can_commit(&self, cx: &App) -> bool {
        (self.amend || !self.status.staged().is_empty())
            && !self.summary.read(cx).value().trim().is_empty()
            && self.author.is_some()
            && self.acting == 0
    }

    /// The summary or the description is this entity: which block a Ctrl+Enter came from.
    pub fn owns_field(&self, field: EntityId) -> bool {
        self.summary.entity_id() == field || self.description.entity_id() == field
    }

    /// The selected entry, as the last status lists it; None once it is gone.
    pub fn selected_entry(&self) -> Option<&Entry> {
        let (list, path) = self.selected.as_ref()?;
        let entries = match list {
            List::Conflicts => self.status.conflicts(),
            List::Staged => self.status.staged(),
            List::Unstaged => self.status.unstaged(),
        };
        entries.into_iter().find(|e| e.path == *path)
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
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.set_git_tab(id, Tab::Branches, window, cx)
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
            .children([Sync::Fetch, Sync::Pull, Sync::Push].map(|which| {
                let (name, label) = match which {
                    Sync::Fetch => ("git-fetch", "Fetch"),
                    Sync::Pull => ("git-pull", "Pull"),
                    Sync::Push => ("git-push", "Push"),
                };
                let running = git.sync == Some(which);
                let disabled = git.acting > 0 && !running;
                // The running one spins, and a click on it cancels it.
                let el = button((name, n), if running { "Cancel" } else { label }, ui, z)
                    .when(running, |el| {
                        el.child(Spinner::new().with_size(UiSize::Size(px(12.0 * z))))
                    })
                    .when(disabled, |el| el.opacity(0.5));
                if disabled {
                    el.test_support().into_any_element()
                } else if running {
                    el.on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.cancel_git(id, cx)))
                        .test_support()
                        .into_any_element()
                } else {
                    el.on_click(
                        cx.listener(move |ws, _: &ClickEvent, window, cx| match which {
                            Sync::Fetch => ws.fetch(id, window, cx),
                            Sync::Pull => ws.pull(id, templum_git::Pull::FastForward, window, cx),
                            Sync::Push => ws.push(id, window, cx),
                        }),
                    )
                    .test_support()
                    .into_any_element()
                }
            }))
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
