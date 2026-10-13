//! A Git block's changes: staging, discarding, diffs and commits.

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use templum_canvas::BlockId;
use templum_git::{GitError, Repo};

use crate::git_block;
use crate::prompt;

use super::{Workspace, notify_git_error, show_notification};

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
        let at = self.here(id);
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
        let at = self.here(id);
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
        if !git.panel.folded.remove(&key) {
            git.panel.folded.insert(key);
        }
        git.panel.rebuild();
        cx.notify();
    }

    /// Expand all (false) or Collapse all (true).
    pub(crate) fn fold_all(&mut self, id: BlockId, folded: bool, cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.panel.folded = if folded {
            git.panel
                .files
                .iter()
                .map(git_block::diff_view::Shown::key)
                .collect()
        } else {
            Default::default()
        };
        git.panel.rebuild();
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
        git.panel.large_shown.insert(key);
        git.panel.rebuild();
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
        let p = &mut git.panel;
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
        let p = &mut git.panel;
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
        git.panel.dragging = false;
        if git.panel.selection.is_some_and(|s| s.is_empty()) {
            git.panel.selection = None;
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
        git.panel.ignore_ws = !git.panel.ignore_ws;
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
