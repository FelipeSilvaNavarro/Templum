//! A Git block's changes: staging, discarding, diffs and commits.

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use templum_canvas::BlockId;
use templum_git::{GitError, Repo};

use crate::git_block;
use crate::prompt;

use super::{GitAt, Workspace, notify_git_error, show_notification};

impl Workspace {
    /// Commits what is staged with the form's message; "Committed abc1234" and the fields
    /// clear.
    pub(crate) fn git_commit(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        if !git.can_commit(cx) {
            return;
        }
        let (message, amend) = (git.message(cx), git.amend);
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let at = GitAt {
            space: self.space.id.clone(),
            block: id,
        };
        self.act_job(
            at,
            move || repo.commit(&message, amend),
            |ws, at, result, window, cx| {
                match result {
                    Ok(short) => {
                        if let Some(git) = ws.git_at(&at) {
                            git.amend = false;
                            let (summary, description) =
                                (git.summary.clone(), git.description.clone());
                            summary.update(cx, |s, cx| s.set_value("", window, cx));
                            description.update(cx, |d, cx| d.set_value("", window, cx));
                        }
                        show_notification(
                            window,
                            cx,
                            Notification::success(format!("Committed {short}")),
                        );
                    }
                    Err(e) => {
                        let entity = cx.entity();
                        notify_git_error(&entity, at.clone(), &e, Vec::new(), window, cx)
                    }
                }
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    /// Ctrl+Enter in a block's summary or description.
    pub(crate) fn commit_from(
        &mut self,
        field: gpui_kit::EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self
            .space
            .blocks
            .iter()
            .find_map(|(id, b)| b.git().filter(|g| g.owns_field(field)).map(|_| *id))
        else {
            return;
        };
        self.git_commit(id, window, cx);
    }

    /// Amend on: the fields show the last commit's message; off: they clear.
    pub(crate) fn set_amend(
        &mut self,
        id: BlockId,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.amend = on;
        cx.notify();
        let (summary, description) = (git.summary.clone(), git.description.clone());
        if !on {
            summary.update(cx, |s, cx| s.set_value("", window, cx));
            description.update(cx, |d, cx| d.set_value("", window, cx));
            return;
        }
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let at = GitAt {
            space: self.space.id.clone(),
            block: id,
        };
        self.git_job(
            at,
            move || repo.last_message(),
            move |_, _, message, window, cx| {
                let Ok(message) = message else {
                    return;
                };
                let (first, rest) = message.split_once('\n').unwrap_or((&message, ""));
                summary.update(cx, |s, cx| s.set_value(first.to_string(), window, cx));
                description.update(cx, |d, cx| d.set_value(rest.trim().to_string(), window, cx));
            },
            window,
            cx,
        );
    }

    /// Asks the name, then the email, and writes both to the global config.
    pub(crate) fn set_git_author(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity();
        prompt::ask_text(
            "Your name for commits",
            "Next",
            "",
            move |name, window, cx| {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return false;
                }
                let ws = ws.clone();
                // The next dialog opens once this one has closed.
                window.defer(cx, move |window, cx| {
                    ws.update(cx, |ws, cx| ws.ask_git_email(id, name, window, cx))
                });
                true
            },
            window,
            cx,
        );
    }

    /// The second half of Set…: the email, then both are written.
    pub(super) fn ask_git_email(
        &mut self,
        id: BlockId,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity();
        let program = self.config.git.program().to_string();
        prompt::ask_text(
            "Your email for commits",
            "Set",
            "",
            move |email, window, cx| {
                let email = email.trim().to_string();
                if email.is_empty() {
                    return false;
                }
                let (name, program) = (name.clone(), program.clone());
                ws.update(cx, |ws, cx| {
                    ws.git_action(
                        id,
                        move |_| Repo::set_author(&program, &name, &email),
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

    /// Shows a row's diff on the right (read in the background).
    pub(crate) fn select_git_row(
        &mut self,
        id: BlockId,
        list: git_block::List,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.selected = Some((list, path));
        cx.notify();
        self.reload_git_diff(id, window, cx);
    }

    /// Reads the selected row's diff again; clears it when the row is gone.
    pub(crate) fn reload_git_diff(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let Some(entry) = git.selected_entry().cloned() else {
            git.selected = None;
            git.diff_line = None;
            if !git.diff.read(cx).value().is_empty() {
                git.diff.update(cx, |d, cx| d.set_value("", window, cx));
            }
            return;
        };
        let staged = git
            .selected
            .as_ref()
            .is_some_and(|s| s.0 == git_block::List::Staged);
        let at = GitAt {
            space: self.space.id.clone(),
            block: id,
        };
        self.git_job(
            at,
            move || repo.diff(&entry, staged),
            |ws, at, diff, window, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                let text = match diff {
                    Ok(text) => text,
                    Err(e) => e.to_string(),
                };
                git.diff_line = templum_git::first_hunk_line(&text);
                // The same text again (the 2-second read): left alone, so the reader's place and
                // selection stay.
                let view = git.diff.clone();
                if view.read(cx).value() != text {
                    view.update(cx, |d, cx| d.set_value(text, window, cx));
                }
            },
            window,
            cx,
        );
    }

    /// Runs `job` on the block's repository in the background; its failure is a notification,
    /// and the block is read again either way.
    pub(crate) fn git_action(
        &mut self,
        id: BlockId,
        job: impl FnOnce(&Repo) -> Result<(), GitError> + Send + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.space.git(id).and_then(|g| g.repo().cloned()) else {
            return;
        };
        let at = GitAt {
            space: self.space.id.clone(),
            block: id,
        };
        self.act_job(
            at,
            move || job(&repo),
            |ws, at, result, window, cx| {
                if let Err(e) = result {
                    let entity = cx.entity();
                    notify_git_error(&entity, at.clone(), &e, Vec::new(), window, cx);
                }
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    pub(crate) fn stage(
        &mut self,
        id: BlockId,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_action(id, move |repo| repo.stage(&refs(&paths)), window, cx);
    }

    pub(crate) fn unstage(
        &mut self,
        id: BlockId,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_action(id, move |repo| repo.unstage(&refs(&paths)), window, cx);
    }

    /// Asks, then throws the unstaged changes of `paths` away.
    pub(crate) fn discard(
        &mut self,
        id: BlockId,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        let entries: Vec<templum_git::Entry> = git
            .status
            .unstaged()
            .into_iter()
            .filter(|e| paths.contains(&e.path))
            .cloned()
            .collect();
        if entries.is_empty() {
            return;
        }
        let what = match entries.as_slice() {
            [one] => format!("Discard the changes to {}?", one.path),
            many => format!("Discard the changes to {} files?", many.len()),
        };
        let ws = cx.entity();
        prompt::confirm(
            what,
            "Untracked files are deleted. This can't be undone.",
            "Discard",
            move |window, cx| {
                let entries = entries.clone();
                ws.update(cx, |ws, cx| {
                    ws.git_action(
                        id,
                        move |repo| repo.discard(&entries.iter().collect::<Vec<_>>()),
                        window,
                        cx,
                    )
                })
            },
            window,
            cx,
        );
    }

    /// Appends the path to the repository's .gitignore.
    pub(crate) fn ignore_path(
        &mut self,
        id: BlockId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.space.git(id).and_then(|g| g.repo().cloned()) else {
            return;
        };
        // Not a git command: a failure is said plainly, with nothing to run in a terminal.
        self.act_job(
            self.here(id),
            move || repo.ignore(&path),
            |ws, at, written, window, cx| {
                if let Err(e) = written {
                    let message = format!("Couldn't write .gitignore: {e}");
                    eprintln!("templum: {message}");
                    show_notification(window, cx, Notification::error(message));
                }
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    pub(crate) fn resolve(
        &mut self,
        id: BlockId,
        path: String,
        side: templum_git::Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_action(id, move |repo| repo.resolve(&path, side), window, cx);
    }

    pub(crate) fn abort_operation(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(op) = self.space.git(id).and_then(|g| g.operation) else {
            return;
        };
        self.git_action(id, move |repo| repo.abort(op), window, cx);
    }

    pub(crate) fn continue_operation(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(op) = self.space.git(id).and_then(|g| g.operation) else {
            return;
        };
        self.git_action(id, move |repo| repo.continue_(op), window, cx);
    }

    /// Opens a repository path in an editor beside the Git block, at the shown diff's first hunk
    /// when it is the selected row.
    pub(crate) fn open_from_git(
        &mut self,
        id: BlockId,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        let Some(root) = git.repo().map(|r| r.root.clone()) else {
            return;
        };
        let line = git
            .selected
            .as_ref()
            .filter(|s| s.1 == path)
            .and(git.diff_line);
        self.open_in_editor(&root, &root.join(path), line, Some(id), window, cx);
    }
}

/// Borrows a list of paths as git's arguments.
fn refs(paths: &[String]) -> Vec<&str> {
    paths.iter().map(String::as_str).collect()
}
