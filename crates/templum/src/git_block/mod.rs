//! A Git block: one repository, through the user's own `git`, run in the background. The
//! changes with their diffs, commits, history, branches, stashes, fetch, pull and push.
//! See docs/specs/cockpit.md, section 8.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use gpui_kit::component::input::{EditorState, InputEvent, InputState, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, ElementId, Entity, EntityId, FocusHandle, SharedString, Subscription,
    UniformListScrollHandle, Window,
};
use templum_canvas::{BlockId, Rect, Size};
use templum_git::{Branch, Commit, CommitFile, GitError, Operation, Repo, Stash, Status};

use crate::session;
use crate::workspace::Workspace;

mod branches;
mod changes;
mod commit_form;
pub(crate) mod diff_view;
pub(crate) mod discarded;
mod history;
mod stashes;
pub(crate) mod switcher;
mod view;
pub(crate) mod widgets;
pub(crate) mod words;

pub(crate) use view::body;
pub(crate) use widgets::field;

/// A new Git block's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size {
    w: 1200.0,
    h: 760.0,
};
pub(crate) const MIN: Size = Size { w: 640.0, h: 420.0 };
/// Past this many changed files, a Filter files field shows above the lists.
pub(crate) const FILTER_PAST: usize = 15;
/// The lists' column width and the rows' height, world units.
pub(crate) const LISTS_W: f32 = 300.0;
pub(crate) const ROW_H: f32 = 24.0;
/// The text's size at zoom 1, world units.
pub(crate) const FONT: f32 = 12.0;

/// How often the status is read while the block is on screen.
pub(crate) const POLL: Duration = Duration::from_secs(2);
/// How many commits History loads at a time.
pub(crate) const PAGE: usize = 200;
/// Past this many characters a summary is longer than `git log --oneline` shows well.
pub(crate) const SUMMARY_MAX: usize = 72;

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

/// Which of fetch, pull, push and sync runs: the bar's Sync button spins, and a click on it
/// cancels it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sync {
    Fetch,
    Pull,
    Push,
    /// Sync: pull, then push.
    Both,
    /// A clone, from the not-a-repository view or ⋯.
    Cloning,
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
    /// How many times the block changed folder (see `GitAt`).
    pub generation: u64,
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
    /// The diff panel of the Changes tab.
    pub panel: diff_view::DiffPanel,
    /// History's file diff and Stashes' diff: each tab has its own, so a 2-second read of the
    /// Changes diff never replaces what another tab shows.
    pub commit_diff: Entity<EditorState>,
    pub stash_diff: Entity<EditorState>,
    /// The row whose diff is shown.
    pub selected: Option<(List, String)>,
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
    /// The block's own keyboard focus (not a field's): its keys act while it has it.
    pub focus: FocusHandle,
    /// The left column's list, scrolled by ↑ and ↓.
    pub lists_scroll: UniformListScrollHandle,
    /// The left column is folded: the diff panel takes the whole width.
    pub lists_folded: bool,
    /// Filter files, over the Changes tab's lists (shown past FILTER_PAST files).
    pub files_filter: Entity<InputState>,
    _files_filter: Subscription,
    _search: Subscription,
}

impl Git {
    pub fn new(folder: PathBuf, tab: Tab, window: &mut Window, cx: &mut Context<Workspace>) -> Git {
        let mut editor_view = || {
            cx.new(|cx| {
                EditorState::new(window, cx)
                    .language("diff")
                    .line_number(false)
                    .soft_wrap(false)
                    // Its fold icons have a fixed size, which breaks the layout at a zoom.
                    .folding(false)
            })
        };
        let (commit_diff, stash_diff) = (editor_view(), editor_view());
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
        let files_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter files"));
        let _files_filter = cx.subscribe(&files_filter, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify()
            }
        });
        Git {
            folder,
            generation: 0,
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
            panel: diff_view::DiffPanel::new(),
            commit_diff,
            stash_diff,
            selected: None,
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
            focus: cx.focus_handle(),
            lists_scroll: UniformListScrollHandle::new(),
            lists_folded: false,
            files_filter,
            _files_filter,
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

    /// The Filter files field shows.
    pub fn filtering(&self) -> bool {
        self.status.entries.len() > FILTER_PAST
    }

    /// The left column's rows: each list's header (Conflicts only when there are some), then its
    /// entries matching Filter files without case, by their index in the whole list (what the
    /// rows' ids, buttons and menus use). A filter left from before narrows nothing once the
    /// field is gone.
    pub fn listed(&self, cx: &App) -> Vec<(List, Option<usize>)> {
        let query = if self.filtering() {
            self.files_filter.read(cx).value().trim().to_lowercase()
        } else {
            String::new()
        };
        let s = &self.status;
        let mut items = Vec::new();
        for (which, entries) in [
            (List::Conflicts, s.conflicts()),
            (List::Staged, s.staged()),
            (List::Unstaged, s.unstaged()),
        ] {
            if which == List::Conflicts && entries.is_empty() {
                continue;
            }
            items.push((which, None));
            items.extend(
                entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| query.is_empty() || e.path.to_lowercase().contains(&query))
                    .map(|(i, _)| (which, Some(i))),
            );
        }
        items
    }

    /// Commit takes every change (`git add --all` first): nothing staged, something changed, no
    /// conflict (adding one would mark it resolved), no Amend.
    pub fn commit_all(&self) -> bool {
        !self.amend
            && self.status.staged().is_empty()
            && !self.status.unstaged().is_empty()
            && self.status.conflicts().is_empty()
    }

    /// "Commit", or "Commit all changes (5 files)".
    pub fn commit_label(&self) -> String {
        if !self.commit_all() {
            return "Commit".to_string();
        }
        match self.status.unstaged().len() {
            1 => "Commit all changes (1 file)".to_string(),
            n => format!("Commit all changes ({n} files)"),
        }
    }

    pub fn summary_len(&self, cx: &App) -> usize {
        self.summary.read(cx).value().chars().count()
    }

    pub fn summary_too_long(&self, cx: &App) -> bool {
        self.summary_len(cx) > SUMMARY_MAX
    }

    /// Commit is possible: something staged (or every change, see `commit_all`), a summary, an
    /// author, nothing running. With Amend, nothing staged is fine (the message alone changes).
    pub fn can_commit(&self, cx: &App) -> bool {
        (self.amend || !self.status.staged().is_empty() || self.commit_all())
            && !self.summary.read(cx).value().trim().is_empty()
            && self.author.is_some()
            && self.acting == 0
    }

    /// The summary or the description is this entity: which block a Ctrl+Enter came from.
    pub fn owns_field(&self, field: EntityId) -> bool {
        self.summary.entity_id() == field || self.description.entity_id() == field
    }

    /// The diff panel on screen: the Changes tab's.
    pub fn diff(&self) -> &diff_view::DiffPanel {
        &self.panel
    }

    pub fn diff_mut(&mut self) -> &mut diff_view::DiffPanel {
        &mut self.panel
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

    /// As the canvas file holds it. The commit message being written is kept, unless Amend
    /// filled it with the last commit's.
    pub fn item(&self, rect: Rect, cx: &App) -> session::GitItem {
        let draft = |text: &str| {
            if self.amend {
                String::new()
            } else {
                text.to_string()
            }
        };
        session::GitItem {
            rect: rect.into(),
            folder: self.folder.to_string_lossy().into_owned(),
            tab: self.tab.name().to_string(),
            summary: draft(&self.summary.read(cx).value()),
            description: draft(&self.description.read(cx).value()),
            lists: if self.lists_folded {
                "folded".to_string()
            } else {
                String::new()
            },
        }
    }
}

pub(crate) fn row_id(block: BlockId, list: &str, i: usize) -> ElementId {
    ElementId::Name(format!("git-row-{}-{list}-{i}", block.0).into())
}
