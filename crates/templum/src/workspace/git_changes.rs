//! A Git block's changes: staging, discarding, diffs and commits.

use std::rc::Rc;

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use templum_canvas::BlockId;
use templum_git::{GitError, Repo};

use crate::git_block;
use crate::prompt;

use super::git_discard::REFUSED;
use super::{
    Extra, GitAt, Workspace, notify_git_error, notify_git_failure, notify_with, show_notification,
};

/// What Undo commit found.
enum Undone {
    /// Taken back: its message, for the fields.
    Done(String),
    /// On the upstream: Revert… instead.
    Pushed,
    /// HEAD isn't that commit any more.
    Moved,
}

impl Workspace {
    /// Commits with the form's message: what is staged, or every change (`Git::commit_all`).
    /// "Committed abc1234" with Undo commit, and the fields clear; with `push`, then pushes (a
    /// branch with no upstream asks to publish, as Push does).
    pub(crate) fn git_commit(
        &mut self,
        id: BlockId,
        push: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        if !git.can_commit(cx) {
            return;
        }
        let (message, amend, all) = (git.message(cx), git.amend, git.commit_all());
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let at = self.here(id);
        self.act_job(
            at,
            move || {
                if all {
                    repo.stage_all()?;
                }
                // An Amend's Undo goes back to the commit it replaces: read it before.
                let replaced = if amend { Some(repo.head()?) } else { None };
                Ok::<_, GitError>((repo.commit(&message, amend)?, replaced))
            },
            move |ws, at, result, window, cx| {
                let entity = cx.entity();
                // Only a commit that happened is pushed.
                let mut pushed = false;
                match result {
                    Ok((short, replaced)) => {
                        if let Some(git) = ws.git_at(&at) {
                            git.amend = false;
                            let (summary, description) =
                                (git.summary.clone(), git.description.clone());
                            summary.update(cx, |s, cx| s.set_value("", window, cx));
                            description.update(cx, |d, cx| d.set_value("", window, cx));
                        }
                        let committed = Notification::success(format!("Committed {short}"));
                        if push {
                            // It goes to the server next: nothing to take back here.
                            show_notification(window, cx, committed);
                            pushed = true;
                        } else {
                            let block = at.block;
                            let undo: Extra = (
                                "git-undo-commit",
                                "Undo commit",
                                Rc::new(
                                    move |ws: &mut Workspace,
                                          window: &mut Window,
                                          cx: &mut Context<Workspace>| {
                                        ws.undo_commit(
                                            block,
                                            short.clone(),
                                            replaced.clone(),
                                            window,
                                            cx,
                                        )
                                    },
                                ),
                            );
                            notify_with(&entity, at.clone(), committed, vec![undo], window, cx);
                        }
                    }
                    Err(e) => notify_git_error(&entity, at.clone(), &e, Vec::new(), window, cx),
                }
                let block = at.block;
                ws.after_action(at, window, cx);
                if pushed {
                    ws.push(block, window, cx);
                }
            },
            window,
            cx,
        );
    }

    /// The left column's fold button: the diff takes the whole width, or gives it back.
    pub(crate) fn toggle_lists(&mut self, id: BlockId, cx: &mut Context<Self>) {
        if let Some(git) = self.space.git_mut(id) {
            git.lists_folded = !git.lists_folded;
            cx.notify();
        }
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
        self.git_commit(id, false, window, cx);
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
        let at = self.here(id);
        self.git_job(
            at,
            move || repo.last_message(),
            move |ws, at, message, window, cx| {
                let Ok(message) = message else {
                    return;
                };
                ws.fill_message(&at, &message, window, cx);
            },
            window,
            cx,
        );
    }

    /// The fields hold `message`: its first line the summary, the rest the description.
    fn fill_message(
        &mut self,
        at: &GitAt,
        message: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.git_at(at) else {
            return;
        };
        let (summary, description) = (git.summary.clone(), git.description.clone());
        let (first, rest) = message.split_once('\n').unwrap_or((message, ""));
        summary.update(cx, |s, cx| s.set_value(first.to_string(), window, cx));
        description.update(cx, |d, cx| d.set_value(rest.trim().to_string(), window, cx));
    }

    /// Undo commit (the notification after a commit): takes it back, its changes staged and its
    /// message in the fields. Pushed meanwhile, it can't be taken back for others: Revert…
    /// instead, after saying why.
    pub(crate) fn undo_commit(
        &mut self,
        id: BlockId,
        short: String,
        replaced: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.space.git(id).and_then(|g| g.repo().cloned()) else {
            return;
        };
        let job_short = short.clone();
        self.act_job(
            self.here(id),
            move || -> Result<Undone, GitError> {
                // A commit made since: it is never the one taken back, pushed or not.
                if !repo.head().is_ok_and(|h| h.starts_with(&job_short)) {
                    return Ok(Undone::Moved);
                }
                if repo.on_upstream(&job_short)? {
                    return Ok(Undone::Pushed);
                }
                Ok(repo
                    .undo_commit(&job_short, replaced.as_deref())?
                    .map_or(Undone::Moved, Undone::Done))
            },
            move |ws, at, result, window, cx| {
                match result {
                    Ok(Undone::Done(message)) => {
                        if let Some(git) = ws.git_at(&at) {
                            git.amend = false;
                        }
                        ws.fill_message(&at, &message, window, cx);
                    }
                    Ok(Undone::Moved) => {
                        let message = format!(
                            "A commit was made since {short}; Undo commit only takes back the last commit."
                        );
                        eprintln!("templum: {message}");
                        show_notification(window, cx, Notification::error(message));
                    }
                    Ok(Undone::Pushed) => {
                        let (entity, block) = (cx.entity(), at.block);
                        prompt::confirm(
                            format!("Revert {short}?"),
                            "Pushed commits can't be removed for others; this adds a new commit that undoes it.",
                            "Revert",
                            move |window, cx| {
                                let short = short.clone();
                                entity.update(cx, |ws, cx| {
                                    ws.git_action(block, move |repo| repo.revert(&short), window, cx)
                                })
                            },
                            window,
                            cx,
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

    /// A click on the left column's row: it is marked, and the panel scrolls to that file.
    pub(crate) fn select_git_row(
        &mut self,
        id: BlockId,
        list: git_block::List,
        path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let staged = list == git_block::List::Staged;
        if let Some(ix) = git.panel.row_of(staged, &path) {
            git.panel
                .scroll
                .scroll_to_item(ix, gpui_kit::ScrollStrategy::Top);
        }
        git.selected = Some((list, path));
        cx.notify();
    }

    /// Reads the whole working diff again in the background (after every status read): a
    /// result equal to what the panel shows changes nothing.
    pub(crate) fn reload_git_panel(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let (status, ignore_ws, whole) = (
            git.status.clone(),
            git.panel.ignore_ws,
            git.panel.whole.clone(),
        );
        let at = self.here(id);
        self.git_job(
            at,
            move || repo.working_diff(&status, ignore_ws, &whole),
            |ws, at, diff, _, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                git.panel.reads += 1;
                match diff {
                    Ok(diff) => {
                        if git.panel.set(diff) {
                            cx.notify();
                        }
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
            },
            window,
            cx,
        );
    }

    /// The file row's arrow, or the row itself: its rows show or hide.
    pub(crate) fn toggle_fold(&mut self, id: BlockId, key: (bool, String), cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let p = git.diff_mut();
        if !p.folded.remove(&key) {
            p.folded.insert(key);
        }
        p.rebuild();
        cx.notify();
    }

    /// Expand all (false) or Collapse all (true).
    pub(crate) fn fold_all(&mut self, id: BlockId, folded: bool, cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let p = git.diff_mut();
        p.folded = if folded {
            p.files
                .iter()
                .map(git_block::diff_view::Shown::key)
                .collect()
        } else {
            Default::default()
        };
        p.rebuild();
        cx.notify();
    }

    /// A fold's click: the file is read again with every line as context.
    pub(crate) fn show_whole(
        &mut self,
        id: BlockId,
        key: (bool, String),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        if !git.panel.whole.contains(&key) {
            git.panel.whole.push(key);
        }
        self.reload_git_panel(id, window, cx);
    }

    /// Show diff on a large or generated file.
    pub(crate) fn show_large(&mut self, id: BlockId, key: (bool, String), cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let p = git.diff_mut();
        p.large_shown.insert(key);
        p.rebuild();
        cx.notify();
    }

    /// The mouse went down on a line row: one click starts a selection there, two select the
    /// word, three the line.
    pub(crate) fn diff_press(
        &mut self,
        id: BlockId,
        row: usize,
        x: gpui_kit::Pixels,
        clicks: usize,
        cx: &mut Context<Self>,
    ) {
        use crate::git_block::diff_view::select::{Pos, Selection, word_at};
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let p = git.diff_mut();
        let Some(col) = p.col_at(row, x) else {
            return;
        };
        let text = p.display_of(row).unwrap_or_default();
        let (a, b) = match clicks {
            2 => {
                let r = word_at(&text, col);
                (r.start, r.end)
            }
            n if n >= 3 => (0, text.len()),
            _ => (col, col),
        };
        p.selection = Some(Selection {
            anchor: Pos { row, col: a },
            head: Pos { row, col: b },
        });
        p.dragging = clicks == 1;
        cx.notify();
    }

    /// The mouse moved over a line row with the button down: the selection's head follows.
    pub(crate) fn diff_drag(
        &mut self,
        id: BlockId,
        row: usize,
        x: gpui_kit::Pixels,
        cx: &mut Context<Self>,
    ) {
        use crate::git_block::diff_view::select::Pos;
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let p = git.diff_mut();
        if !p.dragging {
            return;
        }
        let (Some(col), Some(sel)) = (p.col_at(row, x), p.selection.as_mut()) else {
            return;
        };
        sel.head = Pos { row, col };
        cx.notify();
    }

    /// The mouse came up: a selection that never moved is no selection.
    pub(crate) fn diff_release(&mut self, id: BlockId, cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let p = git.diff_mut();
        p.dragging = false;
        if p.selection.is_some_and(|s| s.is_empty()) {
            p.selection = None;
            cx.notify();
        }
    }

    /// Ignore whitespace on or off: the diff is read again with or without `-w`.
    pub(crate) fn toggle_ignore_whitespace(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let p = git.diff_mut();
        p.ignore_ws = !p.ignore_ws;
        cx.notify();
        self.reload_git_panel(id, window, cx);
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
        let at = self.here(id);
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

    /// A hunk's Stage (`unstage` false) or Unstage: `patch` applied on the index. One git
    /// refuses (the file changed since it was read) says so in the user's words, and the block
    /// reads again either way.
    pub(crate) fn apply_hunk(
        &mut self,
        id: BlockId,
        patch: String,
        unstage: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.space.git(id).and_then(|g| g.repo().cloned()) else {
            return;
        };
        self.act_job(
            self.here(id),
            move || repo.apply_patch(&patch, true, unstage),
            |ws, at, result, window, cx| {
                if let Err(e) = result {
                    let entity = cx.entity();
                    notify_git_failure(
                        &entity,
                        at.clone(),
                        &e,
                        REFUSED.to_string(),
                        Vec::new(),
                        window,
                        cx,
                    );
                }
                ws.after_action(at, window, cx);
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

    /// Opens a repository path in an editor beside the Git block, at the first hunk of the
    /// file as the panel shows it (the selected row's section, else the Changes one).
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
        let staged = git
            .selected
            .as_ref()
            .is_some_and(|s| s.1 == path && s.0 == git_block::List::Staged);
        let line = git
            .panel
            .first_line(staged, path)
            .or_else(|| git.panel.first_line(!staged, path));
        self.open_from_git_at(id, path, line, window, cx);
    }

    /// Opens a repository path in an editor beside the Git block, at `line` (the camera goes
    /// there).
    pub(crate) fn open_from_git_at(
        &mut self,
        id: BlockId,
        path: &str,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self
            .space
            .git(id)
            .and_then(|g| g.repo().map(|r| r.root.clone()))
        else {
            return;
        };
        self.open_in_editor(&root, &root.join(path), line, Some(id), window, cx);
    }
}

/// Borrows a list of paths as git's arguments.
fn refs(paths: &[String]) -> Vec<&str> {
    paths.iter().map(String::as_str).collect()
}
