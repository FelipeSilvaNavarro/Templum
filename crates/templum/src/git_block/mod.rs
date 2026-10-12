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
use gpui_kit::component::input::{
    Editor as EditorView, EditorState, InputEvent, InputState, Textarea, TextareaState,
};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, Entity, EntityId, MouseButton,
    MouseDownEvent, ScrollWheelEvent, SharedString, Stateful, Subscription, TestSupportExt, Window,
    div, px, relative, uniform_list,
};
use templum_canvas::{BlockId, Rect, Size};
use templum_git::{
    Branch, Change, Commit, CommitFile, Entry, GitError, Operation, Repo, Stash, Status,
};

use crate::editor::button;
use crate::menu::Target;
use crate::session;
use crate::theme::Ui;
use crate::workspace::Workspace;
use widgets::{badge_color, bar, centered, field, icon_button};

mod widgets;

/// A new Git block's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size { w: 800.0, h: 560.0 };
pub(crate) const MIN: Size = Size { w: 480.0, h: 320.0 };
/// The lists' column width and the rows' height, world units.
pub(crate) const LISTS_W: f32 = 300.0;
pub(crate) const ROW_H: f32 = 24.0;
/// The text's size at zoom 1, world units.
pub(crate) const FONT: f32 = 12.0;
/// The commit form's Description, three rows high, at zoom 1.
const DESCRIPTION_H: f32 = 78.0;

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

/// The Changes tab: the lists on the left, the selected row's diff on the right.
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
        .flex_1()
        .min_h(px(0.0))
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
    let column = div()
        .w(px(LISTS_W * z))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(ui.hairline)
        .child(lists)
        .child(commit_form(ws, id, git, z, cx));
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .child(column)
        .child(diff_view(ws, id, git, z, cx))
        .into_any_element()
}

/// Summary, Description, Amend, Commit; or why committing isn't possible.
fn commit_form(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let can = git.can_commit(cx);
    let amend = git.amend;
    div()
        .flex()
        .flex_col()
        .gap(px(6.0 * z))
        .p(px(8.0 * z))
        .border_t_1()
        .border_color(ui.hairline)
        .text_size(px(FONT * z))
        .child(field(&git.summary, z).w_full())
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
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.set_amend(id, !amend, window, cx)
                        }))
                        .test_support(),
                )
                .child(div().flex_1())
                .when(can, |el| {
                    el.child(
                        button(("git-commit", n), "Commit", ui, z)
                            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                                ws.git_commit(id, window, cx)
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

/// The selected row's diff, read-only, with Open in editor above it.
fn diff_view(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let Some((_, path)) = git.selected.clone() else {
        return centered(ui, z)
            .flex_1()
            .child("Click a file to see its diff")
            .into_any_element();
    };
    let open = path.clone();
    div()
        .id(("git-diff", n))
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(
            bar(ui, z)
                .child(div().truncate().text_color(ui.title).child(path))
                .child(div().flex_1())
                .child(
                    button(("git-open-diff", n), "Open in editor", ui, z)
                        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.open_from_git(id, &open, window, cx)
                        }))
                        .test_support(),
                ),
        )
        .child(
            div().flex_1().min_h(px(0.0)).child(
                // The element sets the theme's font and the read-only flag every frame.
                EditorView::new(&git.diff)
                    .appearance(false)
                    .bordered(false)
                    .readonly(true)
                    .font_family(ws.cell.font.family.clone())
                    .text_size(px(FONT * z))
                    .h(relative(1.0)),
            ),
        )
        .test_support()
        .into_any_element()
}

/// One list: its header with the count and its buttons, then a row per entry.
fn list(
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
    div()
        .flex()
        .flex_col()
        .child(header)
        .children(
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| row(ws, id, which, i, e, z, cx)),
        )
        .into_any_element()
}

/// A text button in a list's header.
fn small(id: impl Into<ElementId>, label: &'static str, ui: Ui, z: f32) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(6.0 * z))
        .py(px(2.0 * z))
        .rounded(px(4.0 * z))
        .text_size(px(11.0 * z))
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(label)
}

/// "merge" → "Merge".
fn capitalized(word: &str) -> String {
    let mut c = word.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
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

/// The History tab: the search and the virtual list on the left, the shown commit on the right.
fn history(
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

/// The Branches tab: a filter and New branch…, then the local and the remote lists.
fn branches(
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
    let rows = |remote: bool| {
        git.branches
            .iter()
            .filter(|b| {
                b.remote == remote && (query.is_empty() || b.name.to_lowercase().contains(&query))
            })
            .enumerate()
            .map(|(i, b)| {
                let (name, click, menu) = (b.name.clone(), b.name.clone(), b.name.clone());
                let sync = match (&b.upstream, b.gone) {
                    (Some(_), true) => "upstream gone".to_string(),
                    (Some(up), false) => format!("{up} ↑{} ↓{}", b.ahead, b.behind),
                    (None, _) => String::new(),
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
                    .items_center()
                    .gap(px(8.0 * z))
                    .h(px(ROW_H * z))
                    .px(px(10.0 * z))
                    .text_size(px(FONT * z))
                    .text_color(ui.title)
                    .cursor_pointer()
                    .hover(move |st| st.bg(ui.hover))
                    .child(div().w(px(14.0 * z)).flex_shrink_0().when(b.current, |el| {
                        el.child(
                            Icon::new(IconName::Check)
                                .size(px(12.0 * z))
                                .text_color(ui.accent),
                        )
                    }))
                    .child(
                        div()
                            .when(b.current, |el| {
                                el.font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            })
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_color(ui.title_dim)
                            .child(b.subject.clone()),
                    )
                    .child(div().flex_shrink_0().text_color(ui.title_dim).child(sync))
                    .when(!busy, |el| {
                        el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.checkout_branch(id, click.clone(), remote, window, cx)
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
            })
            .collect::<Vec<_>>()
    };
    let heading = |title: &'static str| {
        div()
            .h(px(ROW_H * z))
            .px(px(10.0 * z))
            .flex()
            .items_center()
            .text_size(px(11.0 * z))
            .text_color(ui.title_dim)
            .child(title)
    };
    div()
        .id(("git-branches", n))
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0 * z))
                .p(px(6.0 * z))
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
                .child(heading("Local"))
                .children(rows(false))
                .child(heading("Remote"))
                .children(rows(true)),
        )
        .test_support()
        .into_any_element()
}

/// The Stashes tab: Stash changes…, the list, and the shown stash's diff on the right.
fn stashes(
    ws: &Workspace,
    id: BlockId,
    git: &Git,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let n = id.0 as usize;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let rows = git.stashes.iter().enumerate().map(|(i, s)| {
        let index = s.index;
        div()
            .id(ElementId::Name(format!("git-stash-{n}-{i}").into()))
            .flex()
            .items_center()
            .gap(px(8.0 * z))
            .h(px(ROW_H * z))
            .px(px(10.0 * z))
            .text_size(px(FONT * z))
            .text_color(ui.title)
            .when(git.stash_shown == Some(index), |el| el.bg(ui.card))
            .cursor_pointer()
            .hover(move |st| st.bg(ui.hover))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .child(s.message.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(ui.title_dim)
                    .child(templum_git::ago(now, s.time)),
            )
            .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                ws.show_stash(id, index, window, cx)
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    ws.open_menu(
                        Target::GitStash { block: id, index },
                        e.position,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .test_support()
    });
    let left = div()
        .w(px(LISTS_W * z))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(ui.hairline)
        .child(
            div().p(px(6.0 * z)).child(
                button(("git-stash-changes", n), "Stash changes…", ui, z)
                    .when(git.status.entries.is_empty() || git.acting > 0, |el| {
                        el.opacity(0.5)
                    })
                    .when(!git.status.entries.is_empty() && git.acting == 0, |el| {
                        el.on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.stash_changes(id, window, cx)
                        }))
                    })
                    .test_support(),
            ),
        )
        .child(
            div()
                .id(("git-stash-list", n))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .children(rows),
        );
    let right: AnyElement = match git.stash_shown {
        None => centered(ui, z)
            .flex_1()
            .child("Click a stash to see its diff")
            .into_any_element(),
        Some(_) => div()
            .flex_1()
            .min_w(px(0.0))
            .child(
                EditorView::new(&git.stash_diff)
                    .appearance(false)
                    .bordered(false)
                    .readonly(true)
                    .font_family(ws.cell.font.family.clone())
                    .text_size(px(FONT * z))
                    .h(relative(1.0)),
            )
            .into_any_element(),
    };
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .child(left)
        .child(right)
        .into_any_element()
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
