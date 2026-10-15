//! A Git block's History and Stashes tabs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use templum_canvas::BlockId;
use templum_git::graph::graph;
use templum_git::trash::{self, Kept};
use templum_git::{Commit, CommitFile, FileDiff, GitError, HistoryQuery, Search};

use crate::git_block::{self, Git, Tab, diff_view::Source};
use crate::prompt;

use super::{Extra, GitAt, Workspace, notify_with, show_notification};

/// A page of History: its commits, the file at each (in a file's history), and the commits
/// not pushed (read with the first page).
type Page = (Vec<Commit>, Vec<CommitFile>, Option<HashSet<String>>);

/// What History lists now: its branches and its search.
fn history_query(git: &Git) -> HistoryQuery {
    HistoryQuery {
        all: git.all_branches,
        search: (!git.searched.is_empty()).then(|| (git.search_mode, git.searched.clone())),
    }
}

impl Workspace {
    pub(crate) fn load_stashes(&mut self, at: GitAt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repo) = self.git_at(&at).and_then(|g| g.repo().cloned()) else {
            return;
        };
        self.git_job(
            at,
            move || repo.stashes(),
            |ws, at, stashes, _, _| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                match stashes {
                    Ok(s) => {
                        if git
                            .stash_shown
                            .is_some_and(|i| !s.iter().any(|st| st.index == i))
                        {
                            git.stash_shown = None;
                            git.stash_panel.clear();
                        }
                        git.stashes = s;
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
            },
            window,
            cx,
        );
    }

    /// Stash changes…: a message and whether untracked files go too.
    pub(crate) fn stash_changes(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity();
        prompt::ask_text_and_flag(
            "Stash changes",
            "Stash",
            "Message",
            "Include untracked files",
            move |message, untracked, window, cx| {
                let message = message.trim().to_string();
                if message.is_empty() {
                    return false;
                }
                ws.update(cx, |ws, cx| {
                    ws.git_action(
                        id,
                        move |repo| repo.stash_push(&message, untracked),
                        window,
                        cx,
                    )
                });
                true
            },
            window,
            cx,
        );
    }

    /// The stash at `index` as the Stashes tab shows it (with its hash, so a list changed
    /// meanwhile by a terminal is caught rather than acted on).
    pub(super) fn shown_stash(&self, id: BlockId, index: usize) -> Option<templum_git::Stash> {
        self.space
            .git(id)?
            .stashes
            .iter()
            .find(|s| s.index == index)
            .cloned()
    }

    /// Apply (or Pop: apply and drop) a stash.
    pub(crate) fn stash_apply(
        &mut self,
        id: BlockId,
        index: usize,
        pop: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(stash) = self.shown_stash(id, index) else {
            return;
        };
        self.git_action(
            id,
            move |repo| {
                if pop {
                    repo.stash_pop(&stash)
                } else {
                    repo.stash_apply(&stash)
                }
            },
            window,
            cx,
        );
    }

    pub(crate) fn stash_drop(
        &mut self,
        id: BlockId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(stash) = self.shown_stash(id, index) else {
            return;
        };
        let ws = cx.entity();
        prompt::confirm(
            format!("Drop “{}”?", stash.message),
            "Its changes are lost. This can't be undone.",
            "Drop",
            move |window, cx| {
                let stash = stash.clone();
                ws.update(cx, |ws, cx| {
                    ws.git_action(id, move |repo| repo.stash_drop(&stash), window, cx)
                })
            },
            window,
            cx,
        );
    }

    /// Shows a stash's changes, its new files too, in the Stashes tab's panel.
    pub(crate) fn show_stash(
        &mut self,
        id: BlockId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(stash) = self.shown_stash(id, index) else {
            return;
        };
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.stash_shown = Some(index);
        let ignore_ws = git.stash_panel.ignore_ws;
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let title = format!("stash@{{{}}} · {}", stash.index, stash.message);
        self.git_job(
            self.here(id),
            move || repo.stash_diff(&stash, ignore_ws),
            move |ws, at, diff, _, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                // Another stash clicked meanwhile shows instead.
                if git.stash_shown != Some(index) {
                    return;
                }
                match diff {
                    Ok(files) => {
                        git.stash_panel.source = Source::Range { title };
                        git.stash_panel.set_files(files);
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
                cx.notify();
            },
            window,
            cx,
        );
    }

    /// The diff on screen, read again (Ignore whitespace changed): the working changes, the
    /// comparison, the commit shown, or the stash shown.
    pub(crate) fn reload_shown_diff(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        match git.tab {
            Tab::History => {
                if let Some(shown) = &git.shown {
                    let hash = shown.commit.hash.clone();
                    self.show_commit(id, hash, window, cx)
                }
            }
            Tab::Stashes => {
                if let Some(index) = git.stash_shown {
                    self.show_stash(id, index, window, cx)
                }
            }
            Tab::Branches if git.compare.is_some() => {
                let at = self.here(id);
                self.load_compare(at, window, cx)
            }
            _ => self.reload_git_panel(id, window, cx),
        }
    }

    /// Loads a page of History: the first replaces the list (and reads which commits aren't
    /// pushed), a later one extends it; the graph is laid out again over the whole list, so its
    /// lanes go on. A result for a query changed meanwhile is dropped.
    pub(crate) fn load_history(
        &mut self,
        at: GitAt,
        skip: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.git_at(&at) else {
            return;
        };
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let q = history_query(git);
        let file = git.file.clone();
        let asked = (q.clone(), file.clone());
        self.git_job(
            at,
            move || -> Result<Page, GitError> {
                let (page, paths) = match &file {
                    Some(path) => repo
                        .file_log(path, skip, git_block::PAGE)?
                        .into_iter()
                        .unzip(),
                    None => (repo.history(&q, skip, git_block::PAGE)?, Vec::new()),
                };
                let unpushed = if skip == 0 {
                    Some(repo.unpushed()?)
                } else {
                    None
                };
                Ok((page, paths, unpushed))
            },
            move |ws, at, read, _, _| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                // Another query asked since, or this page already added (Load more twice).
                if (history_query(git), git.file.clone()) != asked
                    || (skip > 0 && git.commits.len() != skip)
                {
                    return;
                }
                match read {
                    Ok((page, paths, unpushed)) => {
                        git.more = page.len() == git_block::PAGE;
                        let (mut all, mut at_each) = if skip == 0 {
                            (Vec::new(), Vec::new())
                        } else {
                            ((*git.commits).clone(), (*git.file_paths).clone())
                        };
                        all.extend(page);
                        at_each.extend(paths);
                        // A search's or a file's list lacks the parents the lanes need.
                        let (q, file) = &asked;
                        git.graph = Rc::new(if q.search.is_none() && file.is_none() {
                            graph(&all)
                        } else {
                            Vec::new()
                        });
                        git.commits = Rc::new(all);
                        git.file_paths = Rc::new(at_each);
                        if let Some(u) = unpushed {
                            git.unpushed = Rc::new(u);
                        }
                        git.error = None;
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
            },
            window,
            cx,
        );
    }

    /// File history: History narrowed to `path`, through its renames.
    pub(crate) fn file_history(
        &mut self,
        id: BlockId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.tab = Tab::History;
        git.file = Some(path);
        git.blame = false;
        git.blame_at = None;
        git.hide_shown();
        // The list read for another file (or the whole history) goes at once: a menu on one of
        // its rows would pair that commit with this file.
        git.commits = Rc::new(Vec::new());
        git.file_paths = Rc::new(Vec::new());
        git.graph = Rc::new(Vec::new());
        git.more = false;
        cx.notify();
        let at = self.here(id);
        // A block still looking its repository up reads the list once it has found it.
        self.refresh_lists_at(at, window, cx);
    }

    /// The file's chip ×: the whole history again.
    pub(crate) fn clear_file_history(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.file = None;
        git.blame = false;
        git.blame_at = None;
        git.hide_shown();
        git.commits = Rc::new(Vec::new());
        git.file_paths = Rc::new(Vec::new());
        git.graph = Rc::new(Vec::new());
        git.more = false;
        cx.notify();
        let at = self.here(id);
        self.load_history(at, 0, window, cx);
    }

    /// Restore this version…: asks, then writes the file as it was in `hash` (from `then`, its
    /// path at that commit) to its path now, after a copy of it in the trash.
    pub(crate) fn restore_version(
        &mut self,
        id: BlockId,
        hash: String,
        then: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        let (Some(now), Some(short)) = (
            git.file.clone(),
            git.commits
                .iter()
                .find(|c| c.hash == hash)
                .map(|c| c.short.clone()),
        ) else {
            return;
        };
        let why = if self.trash.is_some() {
            "The file as it is now goes to Recently discarded."
        } else {
            "The file as it is now is replaced. This can't be undone."
        };
        let (entity, at) = (cx.entity(), self.here(id));
        prompt::confirm(
            format!("Restore {now} as it was in {short}?"),
            why,
            "Restore",
            move |window, cx| {
                let (hash, then, now, short, at) = (
                    hash.clone(),
                    then.clone(),
                    now.clone(),
                    short.clone(),
                    at.clone(),
                );
                entity.update(cx, |ws, cx| {
                    if ws.show_git_block(&at, window, cx) {
                        ws.restore_now(at, hash, then, now, short, window, cx)
                    }
                })
            },
            window,
            cx,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn restore_now(
        &mut self,
        at: GitAt,
        hash: String,
        then: String,
        now: String,
        short: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.git_at(&at).and_then(|g| g.repo().cloned()) else {
            return;
        };
        let trash = self.trash.clone();
        let done = format!("Restored {now} from {short}");
        let what = done.clone();
        self.act_job(
            at,
            move || -> Result<Option<Kept>, String> {
                let bytes = repo.file_at(&hash, &then).map_err(|e| e.to_string())?;
                let paths = [now.clone()];
                let (files, size) = trash::measure(&repo.root, &paths);
                let kept = match trash.as_ref().filter(|_| trash::fits(files, size)) {
                    Some(t) => Some(
                        t.put(&repo.root, &paths, &what, SystemTime::now())
                            .map_err(|e| {
                                format!("Couldn't keep a copy, so nothing was restored: {e}")
                            })?,
                    ),
                    None => None,
                };
                let full = repo.root.join(&now);
                let written = match full.parent() {
                    Some(dir) => std::fs::create_dir_all(dir),
                    None => Ok(()),
                }
                .and_then(|_| std::fs::write(&full, bytes));
                if let Err(e) = written {
                    // Nothing was restored: an entry left would list a change that never was.
                    if let (Some(t), Some(k)) = (&trash, &kept) {
                        let _ = t.remove(&k.id);
                    }
                    return Err(format!("Couldn't write {now}: {e}"));
                }
                if let (Some(t), Some(k)) = (&trash, &kept) {
                    let _ = t.seal(k);
                    let _ = t.prune(SystemTime::now());
                }
                Ok(kept)
            },
            move |ws, at, result, window, cx| {
                let entity = cx.entity();
                match result {
                    Ok(Some(kept)) => {
                        let (block, entry) = (at.block, kept.id);
                        let undo: Extra = (
                            "git-undo",
                            "Undo".into(),
                            Rc::new(
                                move |ws: &mut Workspace,
                                      window: &mut Window,
                                      cx: &mut Context<Workspace>| {
                                    ws.restore_discarded(block, entry.clone(), false, window, cx)
                                },
                            ),
                        );
                        notify_with(
                            &entity,
                            at.clone(),
                            Notification::success(done),
                            vec![undo],
                            window,
                            cx,
                        );
                    }
                    Ok(None) => show_notification(window, cx, Notification::success(done)),
                    Err(message) => {
                        eprintln!("templum: {message}");
                        show_notification(window, cx, Notification::error(message));
                    }
                }
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    /// File history (or Blame) from an editor's tree: the Git block open on the file's
    /// repository, else a new one beside the editor; the canvas goes there.
    pub(crate) fn history_from_tree(
        &mut self,
        editor: BlockId,
        path: PathBuf,
        blame: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = templum_git::root_of(&path).map(Path::to_path_buf) else {
            let message = format!("{} isn't in a Git repository", path.display());
            show_notification(window, cx, Notification::warning(message));
            return;
        };
        let Ok(inside) = path.strip_prefix(&root) else {
            return;
        };
        let rel = inside
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let root_real = root.canonicalize().ok();
        let same = |a: &Path| a.canonicalize().ok() == root_real;
        let open = self.space.blocks.iter().find_map(|(id, b)| {
            b.git()
                .filter(|g| g.repo().map_or(same(&g.folder), |r| same(&r.root)))
                .map(|_| *id)
        });
        let id = match open {
            Some(id) => id,
            None => {
                let Some(anchor) = self.space.canvas.get(editor).map(|b| self.space.home(b)) else {
                    return;
                };
                let at = self
                    .space
                    .canvas
                    .place_beside(anchor, git_block::SIZE)
                    .origin();
                let Some(id) = self.add_git_at(&root, at, window, cx) else {
                    return;
                };
                id
            }
        };
        if blame {
            self.blame_file(id, rel, None, window, cx);
        } else {
            self.file_history(id, rel, window, cx);
        }
        let space = self.space.id.clone();
        self.reveal(&space, id, window, cx);
    }

    /// All branches on or off: the list read again.
    pub(crate) fn toggle_all_branches(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.all_branches = !git.all_branches;
        git.hide_shown();
        cx.notify();
        let at = self.here(id);
        self.load_history(at, 0, window, cx);
    }

    /// Enter in a History search field (or the field cleared): that block's list read again.
    pub(crate) fn search_from(
        &mut self,
        field: gpui_kit::EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.space.blocks.iter().find_map(|(id, b)| {
            b.git()
                .filter(|g| g.search.entity_id() == field)
                .map(|_| *id)
        }) else {
            return;
        };
        self.run_search(id, window, cx);
    }

    pub(crate) fn run_search(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let text = git.search.read(cx).value().trim().to_string();
        git.searched = text;
        git.hide_shown();
        cx.notify();
        let at = self.here(id);
        self.load_history(at, 0, window, cx);
    }

    /// A mode clicked: with a search on, it runs again in that mode.
    pub(crate) fn set_search_mode(
        &mut self,
        id: BlockId,
        mode: Search,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.search_mode = mode;
        let again = !git.searched.is_empty();
        cx.notify();
        if again {
            let at = self.here(id);
            self.load_history(at, 0, window, cx);
        }
    }

    /// A commit clicked (in the list, or a blame's group): the left column shows its message
    /// and files, the panel its whole diff (against its first parent).
    pub(crate) fn show_commit(
        &mut self,
        id: BlockId,
        hash: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let loaded = git.commits.iter().find(|c| c.hash == hash).cloned();
        let ignore_ws = git.history_panel.ignore_ws;
        let asked = hash.clone();
        if let Some(git) = self.space.git_mut(id) {
            git.showing = Some(hash.clone());
        }
        self.git_job(
            self.here(id),
            move || -> Result<(Commit, Vec<CommitFile>, Vec<FileDiff>), GitError> {
                let commit = match loaded {
                    Some(c) => c,
                    None => repo.commit_info(&hash)?,
                };
                Ok((
                    commit,
                    repo.commit_files(&hash)?,
                    repo.commit_changes(&hash, ignore_ws)?,
                ))
            },
            move |ws, at, read, _, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                // Left meanwhile (←, a search, another file…), or another commit clicked.
                if git.showing.as_deref() != Some(asked.as_str()) {
                    return;
                }
                match read {
                    Ok((commit, files, diffs)) => {
                        // In a file's history, that file's change alone, by its name then.
                        let then = git.file.as_ref().and_then(|_| {
                            git.commits
                                .iter()
                                .position(|c| c.hash == commit.hash)
                                .and_then(|i| git.file_paths.get(i))
                                .map(|f| f.path.clone())
                        });
                        let diffs = match then {
                            Some(then) => diffs.into_iter().filter(|d| d.path() == then).collect(),
                            None => diffs,
                        };
                        git.history_panel.source = Source::Range {
                            title: format!("Commit {}", commit.short),
                        };
                        git.history_panel.set_files(diffs);
                        // Another commit opens at the top of its details (the same one read
                        // again keeps its place).
                        if git
                            .shown
                            .as_ref()
                            .is_none_or(|s| s.commit.hash != commit.hash)
                        {
                            git.details_scroll.set_offset(Default::default());
                        }
                        git.shown = Some(git_block::Shown {
                            commit,
                            files,
                            file: None,
                        });
                        git.error = None;
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
                cx.notify();
            },
            window,
            cx,
        );
    }

    /// The commit column's ←: back to the list, the panel empty (or the file's blame again).
    pub(crate) fn hide_commit(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.hide_shown();
        let blame = git.blame;
        cx.notify();
        if blame {
            let at = self.here(id);
            self.load_blame(at, window, cx);
        }
    }

    /// Blame: History narrowed to the file, its panel showing who last changed each line (at
    /// `rev`, or in the working tree). A block still looking its repository up reads it once
    /// found (`refresh_lists_at` loads the blame while `blame` is on).
    pub(crate) fn blame_file(
        &mut self,
        id: BlockId,
        path: String,
        rev: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_history(id, path, window, cx);
        if let Some(git) = self.space.git_mut(id) {
            git.blame = true;
            git.blame_at = rev;
        }
        let at = self.here(id);
        self.load_blame(at, window, cx);
    }

    /// The file's chip's Blame: off, the panel empties.
    pub(crate) fn toggle_blame(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        if git.file.is_none() {
            return;
        }
        if git.blame {
            git.blame = false;
            git.blame_at = None;
            if git.shown.is_none() {
                git.history_panel.clear();
            }
            cx.notify();
        } else {
            git.blame = true;
            git.blame_at = None;
            git.hide_shown();
            cx.notify();
            let at = self.here(id);
            self.load_blame(at, window, cx);
        }
    }

    /// Reads the blame of History's file into its panel.
    pub(crate) fn load_blame(&mut self, at: GitAt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(git) = self.git_at(&at) else {
            return;
        };
        let (Some(repo), Some(path), true) = (git.repo().cloned(), git.file.clone(), git.blame)
        else {
            return;
        };
        let rev = git.blame_at.clone();
        let asked = (path.clone(), rev.clone());
        self.git_job(
            at,
            move || repo.blame(&path, rev.as_deref()),
            move |ws, at, lines, _, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                // Turned off, another file, or a commit opened meanwhile: not shown.
                if !git.blame
                    || git.shown.is_some()
                    || (git.file.as_ref(), git.blame_at.as_ref())
                        != (Some(&asked.0), asked.1.as_ref())
                {
                    return;
                }
                match lines {
                    Ok(lines) => {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs() as i64);
                        let (file, hashes) = templum_git::blame::blame_diff(&asked.0, &lines, now);
                        let title = match &asked.1 {
                            Some(rev) => {
                                format!("Blame · {} at {}", asked.0, &rev[..rev.len().min(7)])
                            }
                            None => format!("Blame · {}", asked.0),
                        };
                        git.history_panel.source = Source::Blame { title, hashes };
                        git.history_panel.set_files(vec![file]);
                        git.error = None;
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
                cx.notify();
            },
            window,
            cx,
        );
    }

    /// A file of the commit shown: the panel scrolls to it.
    pub(crate) fn show_commit_file(&mut self, id: BlockId, path: String, cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        if let Some(ix) = git.history_panel.row_of(false, &path) {
            git.history_panel
                .scroll
                .scroll_to_item(ix, gpui_kit::ScrollStrategy::Top);
        }
        if let Some(shown) = git.shown.as_mut() {
            shown.file = Some(path);
        }
        cx.notify();
    }

    /// Checkout… on a commit: warns about the detached state first.
    pub(crate) fn checkout_commit(
        &mut self,
        id: BlockId,
        hash: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity();
        prompt::confirm(
            format!("Check out {}?", &hash[..hash.len().min(7)]),
            "HEAD will be detached: commits made here belong to no branch until one is made from them.",
            "Check out",
            move |window, cx| {
                let hash = hash.clone();
                ws.update(cx, |ws, cx| {
                    ws.git_action(id, move |repo| repo.checkout_commit(&hash), window, cx)
                })
            },
            window,
            cx,
        );
    }

    /// New branch… from `from` (a commit; None: the current one): asks the name.
    pub(crate) fn branch_from(
        &mut self,
        id: BlockId,
        from: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity();
        prompt::ask_text(
            "New branch",
            "Create",
            "",
            move |name, window, cx| {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return false;
                }
                let from = from.clone();
                ws.update(cx, |ws, cx| {
                    ws.git_action(
                        id,
                        move |repo| repo.new_branch(&name, from.as_deref()),
                        window,
                        cx,
                    )
                });
                true
            },
            window,
            cx,
        );
    }

    pub(crate) fn tag_commit(
        &mut self,
        id: BlockId,
        hash: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity();
        prompt::ask_text(
            "New tag",
            "Create",
            "",
            move |name, window, cx| {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return false;
                }
                let hash = hash.clone();
                ws.update(cx, |ws, cx| {
                    ws.git_action(id, move |repo| repo.tag(&name, &hash), window, cx)
                });
                true
            },
            window,
            cx,
        );
    }

    pub(crate) fn revert_commit(
        &mut self,
        id: BlockId,
        hash: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity();
        prompt::confirm(
            format!("Revert {}?", &hash[..hash.len().min(7)]),
            "A new commit undoes its changes.",
            "Revert",
            move |window, cx| {
                let hash = hash.clone();
                ws.update(cx, |ws, cx| {
                    ws.git_action(id, move |repo| repo.revert(&hash), window, cx)
                })
            },
            window,
            cx,
        );
    }
}
