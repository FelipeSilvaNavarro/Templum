//! A Git block's History and Stashes tabs.

use std::rc::Rc;

use gpui_kit::{Context, Window};
use templum_canvas::BlockId;

use crate::git_block;
use crate::prompt;

use super::{GitAt, Workspace};

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

    /// Shows a stash's diff on the right.
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
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        self.git_job(
            self.here(id),
            move || repo.stash_show(&stash),
            |ws, at, diff, window, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                let text = diff.unwrap_or_else(|e| e.to_string());
                let view = git.stash_diff.clone();
                view.update(cx, |d, cx| d.set_value(text, window, cx));
            },
            window,
            cx,
        );
    }

    /// Loads a page of History: the first page replaces the list, a later one extends it.
    pub(crate) fn load_history(
        &mut self,
        at: GitAt,
        skip: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.git_at(&at).and_then(|g| g.repo().cloned()) else {
            return;
        };
        self.git_job(
            at,
            move || repo.log(skip, git_block::PAGE),
            move |ws, at, log, _, _| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                match log {
                    Ok(page) => {
                        git.more = page.len() == git_block::PAGE;
                        let mut all = if skip == 0 {
                            Vec::new()
                        } else {
                            (*git.commits).clone()
                        };
                        all.extend(page);
                        git.commits = Rc::new(all);
                        git.error = None;
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
            },
            window,
            cx,
        );
    }

    /// Shows a commit's message and files on the right of History.
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
        let Some(commit) = git.commits.iter().find(|c| c.hash == hash).cloned() else {
            return;
        };
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let files_of = hash.clone();
        self.git_job(
            self.here(id),
            move || repo.commit_files(&files_of),
            move |ws, at, files, window, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                match files {
                    Ok(files) => {
                        git.shown = Some(git_block::Shown {
                            commit,
                            files,
                            file: None,
                        });
                        let view = git.commit_diff.clone();
                        view.update(cx, |d, cx| d.set_value("", window, cx));
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
            },
            window,
            cx,
        );
    }

    /// Shows one file's diff in the shown commit.
    pub(crate) fn show_commit_file(
        &mut self,
        id: BlockId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let Some(shown) = &mut git.shown else {
            return;
        };
        let Some(file) = shown.files.iter().find(|f| f.path == path).cloned() else {
            return;
        };
        shown.file = Some(path);
        let hash = shown.commit.hash.clone();
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        self.git_job(
            self.here(id),
            move || repo.commit_diff(&hash, &file),
            |ws, at, diff, window, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                let text = diff.unwrap_or_else(|e| e.to_string());
                let view = git.commit_diff.clone();
                view.update(cx, |d, cx| d.set_value(text, window, cx));
            },
            window,
            cx,
        );
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
