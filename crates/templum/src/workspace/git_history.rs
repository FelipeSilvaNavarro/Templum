//! A Git block's History and Stashes tabs.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::{Context, Window};
use templum_canvas::BlockId;
use templum_git::graph::graph;
use templum_git::{Commit, CommitFile, FileDiff, GitError, HistoryQuery, Search};

use crate::git_block::{self, Git, Tab, diff_view::Source};
use crate::prompt;

use super::{GitAt, Workspace};

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
                            git.stash_panel.set_files(Vec::new());
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
        let asked = q.clone();
        self.git_job(
            at,
            move || -> Result<(Vec<Commit>, Option<HashSet<String>>), GitError> {
                let page = repo.history(&q, skip, git_block::PAGE)?;
                let unpushed = if skip == 0 {
                    Some(repo.unpushed()?)
                } else {
                    None
                };
                Ok((page, unpushed))
            },
            move |ws, at, read, _, _| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                if history_query(git) != asked {
                    return;
                }
                match read {
                    Ok((page, unpushed)) => {
                        git.more = page.len() == git_block::PAGE;
                        let mut all = if skip == 0 {
                            Vec::new()
                        } else {
                            (*git.commits).clone()
                        };
                        all.extend(page);
                        git.graph = Rc::new(if asked.search.is_none() {
                            graph(&all)
                        } else {
                            Vec::new()
                        });
                        git.commits = Rc::new(all);
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
        git.shown = None;
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
        git.shown = None;
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
                match read {
                    Ok((commit, files, diffs)) => {
                        git.history_panel.source = Source::Range {
                            title: format!("Commit {}", commit.short),
                        };
                        git.history_panel.set_files(diffs);
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

    /// The commit column's ←: back to the list, the panel empty.
    pub(crate) fn hide_commit(&mut self, id: BlockId, cx: &mut Context<Self>) {
        if let Some(git) = self.space.git_mut(id) {
            git.shown = None;
            git.history_panel.set_files(Vec::new());
            cx.notify();
        }
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
