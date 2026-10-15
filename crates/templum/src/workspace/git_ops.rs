//! Git blocks: opening a repository, reading it, and running its commands in the background.

use std::path::Path;
use std::rc::Rc;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Entity, SharedString, Window, div, px};
use templum_canvas::{BlockId, Point, Rect};
use templum_git::{GitError, Repo};

use crate::git_block::{self, Found, Git, Read};
use crate::launcher;
use crate::prompt;

use super::{Body, GitAt, Workspace, git_error_message, remember, show_notification};

impl Workspace {
    /// Asks for a folder, then opens a Git block on its repository at the viewport's center.
    pub(crate) fn new_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let center = self.space.camera().to_world(self.center(window));
        let size = git_block::SIZE;
        let at = self
            .space
            .canvas
            .place_new(
                size,
                Point::new(center.x - size.w / 2.0, center.y - size.h / 2.0),
            )
            .origin();
        self.ask_git_folder(at, window, cx);
    }

    /// The folder dialog; the block's top-left corner goes to `at` (world units).
    pub(crate) fn ask_git_folder(&self, at: Point, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        launcher::ask_folder_for(
            self,
            "Open a Git block".to_string(),
            None,
            Rc::new(move |folder, window, cx| {
                entity.update(cx, |ws, cx| ws.add_git_at(folder, at, window, cx))
            }),
            window,
            cx,
        );
    }

    /// A Git block on `folder`'s repository, its top-left corner at `at`; the folder becomes a
    /// recent one. The repository is looked up in the background.
    pub(crate) fn add_git_at(
        &mut self,
        folder: &Path,
        at: Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BlockId> {
        if !folder.is_dir() {
            let message = format!("{} isn't a folder", folder.display());
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
            return None;
        }
        self.space.pristine = false;
        let rect = Rect::new(at.x, at.y, git_block::SIZE.w, git_block::SIZE.h);
        let id = self.add_block(
            rect,
            Body::Git(Box::new(Git::new(
                folder.to_path_buf(),
                git_block::Tab::Changes,
                window,
                cx,
            ))),
        );
        remember(&mut self.space.recent, folder.to_path_buf());
        self.look_up_git(id, window, cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
        Some(id)
    }

    /// The folder dialog, to point a Git block at another folder: from its menu, or when its
    /// folder is gone or isn't a repository.
    pub(crate) fn choose_git_folder(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity();
        launcher::ask_folder_for(
            self,
            "Change the Git block's folder".to_string(),
            None,
            Rc::new(move |folder, window, cx| {
                entity.update(cx, |ws, cx| ws.set_git_folder(id, folder, window, cx))
            }),
            window,
            cx,
        );
    }

    /// Points a Git block at another folder, looked up afresh: nothing of the repository it
    /// showed stays (lists, diffs, the commit's fields), and what its commands still running
    /// return is dropped. A path that isn't a folder changes nothing.
    pub(crate) fn set_git_folder(
        &mut self,
        id: BlockId,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BlockId> {
        let (tab, generation) = self.space.git(id).map(|g| (g.tab, g.generation + 1))?;
        if !folder.is_dir() {
            let message = format!("{} isn't a folder", folder.display());
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
            return None;
        }
        let mut git = Git::new(folder.to_path_buf(), tab, window, cx);
        git.generation = generation;
        *self.space.git_mut(id)? = git;
        remember(&mut self.space.recent, folder.to_path_buf());
        self.look_up_git(id, window, cx);
        cx.notify();
        Some(id)
    }

    /// Points the block `at` names at `folder`, in its own workspace even when another one is on
    /// screen (a clone that ended there), and looks it up, as `set_git_folder` does. False when
    /// the block is gone or changed folder since.
    pub(super) fn set_git_folder_at(
        &mut self,
        at: &GitAt,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab) = self.git_at(at).map(|g| g.tab) else {
            return false;
        };
        let mut git = Git::new(folder.to_path_buf(), tab, window, cx);
        git.generation = at.generation + 1;
        let Some(space) = self.space_by_id_mut(&at.space) else {
            return false;
        };
        remember(&mut space.recent, folder.to_path_buf());
        let Some(slot) = space.git_mut(at.block) else {
            return false;
        };
        *slot = git;
        let now = GitAt {
            generation: at.generation + 1,
            ..at.clone()
        };
        self.look_up_at(now, window, cx);
        cx.notify();
        true
    }

    /// Finds the repository holding the block's folder, in the background, then reads it.
    pub(crate) fn look_up_git(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let at = self.here(id);
        self.look_up_at(at, window, cx);
    }

    /// `look_up_git` for the block `at` names, in whichever workspace.
    pub(super) fn look_up_at(&mut self, at: GitAt, window: &mut Window, cx: &mut Context<Self>) {
        let program = self.config.git.program().to_string();
        let Some(folder) = self.git_at(&at).map(|g| g.folder.clone()) else {
            return;
        };
        if !folder.is_dir() {
            return;
        }
        self.git_job(
            at,
            move || Repo::discover(&program, &folder),
            |ws, at, found, window, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                git.found = match found {
                    Ok(Some(repo)) => Found::Repo(repo),
                    Ok(None) => Found::NotARepo,
                    Err(e) => Found::NoGit(e),
                };
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    /// `git init` in the block's folder, then reads it.
    pub(crate) fn init_git(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        let (program, folder) = (self.config.git.program().to_string(), git.folder.clone());
        let at = self.here(id);
        self.act_job(
            at,
            move || Repo::init(&program, &folder),
            |ws, at, made, window, cx| match made {
                Ok(repo) => {
                    if let Some(git) = ws.git_at(&at) {
                        git.found = Found::Repo(repo);
                    }
                    ws.after_action(at, window, cx);
                }
                Err(e) => {
                    eprintln!("templum: {e}");
                    show_notification(window, cx, Notification::error(e.to_string()));
                }
            },
            window,
            cx,
        );
    }

    /// Shows a tab of a Git block, and reads its list.
    pub(crate) fn set_git_tab(
        &mut self,
        id: BlockId,
        tab: git_block::Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(git) = self.space.git_mut(id) {
            git.tab = tab;
            cx.notify();
        }
        let at = self.here(id);
        self.refresh_lists_at(at, window, cx);
    }

    /// The Git block `id` of the workspace on screen (the History list's closure builds it too).
    pub(crate) fn here(&self, id: BlockId) -> GitAt {
        GitAt {
            space: self.space.id.clone(),
            block: id,
            generation: self.space.git(id).map_or(0, |g| g.generation),
        }
    }

    /// Shows the Git block `at` names, switching to its workspace; false when it is gone.
    pub(crate) fn show_git_block(
        &mut self,
        at: &GitAt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.space.id != at.space {
            self.reveal(&at.space, at.block, window, cx);
        }
        // A block that changed folder since isn't the one the notification is about.
        self.space.id == at.space
            && self
                .space
                .git(at.block)
                .is_some_and(|g| g.generation == at.generation)
    }

    /// After a command: the status now, and the shown tab's list once the status is in.
    pub(super) fn after_action(&mut self, at: GitAt, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(git) = self.git_at(&at) {
            git.pending_lists = true;
        }
        self.refresh_at(at, window, cx);
    }

    /// Reads the list behind the shown tab (History, Branches or Stashes) in the background.
    pub(super) fn refresh_lists_at(
        &mut self,
        at: GitAt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.git_at(&at) else {
            return;
        };
        if git.busy {
            git.pending_lists = true;
            return;
        }
        let tab = git.tab;
        // The tab's list job queues behind it.
        self.load_default(at.clone(), window, cx);
        match tab {
            git_block::Tab::Changes => {}
            git_block::Tab::History => {
                let blame = self.git_at(&at).is_some_and(|g| g.blame);
                self.load_history(at.clone(), 0, window, cx);
                if blame {
                    self.load_blame(at, window, cx);
                }
            }
            git_block::Tab::Branches => {
                let comparing = self.git_at(&at).is_some_and(|g| g.compare.is_some());
                self.load_branches(at.clone(), window, cx);
                if comparing {
                    self.load_compare(at, window, cx);
                }
            }
            git_block::Tab::Stashes => self.load_stashes(at, window, cx),
        }
    }

    /// Reads the block's status in the background: at once, or when the running command ends.
    pub(crate) fn refresh_git(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let at = self.here(id);
        self.refresh_at(at, window, cx);
    }

    pub(super) fn refresh_at(&mut self, at: GitAt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(git) = self.git_at(&at) else {
            return;
        };
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        if git.busy {
            git.pending = true;
            return;
        }
        self.git_job(
            at,
            move || Read::of(&repo),
            |ws, at, read, window, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                git.reads += 1;
                match read {
                    Ok(read) => {
                        git.status = read.status;
                        git.operation = read.operation;
                        git.author = read.author;
                        git.error = None;
                    }
                    Err(e) => git.error = Some(e.to_string()),
                }
                // The panel follows the status (a file staged in a terminal, a file gone).
                if at.space == ws.space.id {
                    ws.reload_git_panel(at.block, window, cx);
                }
            },
            window,
            cx,
        );
    }

    /// Every git_block::POLL: reads the Git blocks in the viewport again.
    pub(super) fn poll_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        for body in self.space.blocks.values_mut() {
            if let Body::Git(git) = body {
                let gone = !git.folder.is_dir();
                changed |= gone != git.gone;
                git.gone = gone;
            }
        }
        if changed {
            cx.notify();
        }
        let vp = self.viewport(window);
        let due: Vec<BlockId> = self
            .space
            .canvas
            .visible_from(&self.space.view, vp)
            .map(|b| b.id)
            .filter(|id| self.space.shown(*id))
            // A block still reading is skipped: a slow repository never piles reads up.
            .filter(|id| {
                self.space
                    .git(*id)
                    .is_some_and(|g| g.repo().is_some() && !g.busy)
            })
            .collect();
        for id in due {
            self.refresh_git(id, window, cx);
        }
    }

    /// The Git block `at` names, in whichever workspace it is.
    /// The Git block `at` names, unless it changed folder since.
    pub(super) fn git_at(&mut self, at: &GitAt) -> Option<&mut Git> {
        self.space_by_id_mut(&at.space)?
            .git_mut(at.block)
            .filter(|g| g.generation == at.generation)
    }

    /// `git_job` for an action (not a read): while any action is asked and not done, the
    /// block's action buttons are disabled.
    pub(super) fn act_job<T: Send + 'static>(
        &mut self,
        at: GitAt,
        job: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Workspace, GitAt, T, &mut Window, &mut Context<Workspace>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.git_at(&at) else {
            return;
        };
        git.acting += 1;
        self.git_job(
            at,
            job,
            move |ws, at, result, window, cx| {
                if let Some(git) = ws.git_at(&at) {
                    git.acting = git.acting.saturating_sub(1);
                }
                done(ws, at, result, window, cx)
            },
            window,
            cx,
        );
    }

    /// Runs `job` on the background executor, the block marked busy meanwhile, then `done` on
    /// the UI thread with its result. A refresh asked while it ran follows, once. A block
    /// closed in the meantime gets nothing.
    pub(super) fn git_job<T: Send + 'static>(
        &mut self,
        at: GitAt,
        job: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Workspace, GitAt, T, &mut Window, &mut Context<Workspace>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.git_at(&at) else {
            return;
        };
        // One command at a time per block: this one waits for the running one.
        if git.busy {
            let at = at.clone();
            git.queued.push(Box::new(move |ws, window, cx| {
                ws.git_job(at, job, done, window, cx)
            }));
            return;
        }
        git.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |ws, cx| {
            let result = cx.background_executor().spawn(async move { job() }).await;
            ws.update_in(cx, |ws, window, cx| {
                if let Some(git) = ws.git_at(&at) {
                    git.busy = false;
                }
                done(ws, at.clone(), result, window, cx);
                let (again, lists) = ws.git_at(&at).map_or((false, false), |g| {
                    (
                        std::mem::take(&mut g.pending),
                        std::mem::take(&mut g.pending_lists),
                    )
                });
                if again {
                    if lists {
                        // Taken back: the status read's completion runs the lists.
                        if let Some(g) = ws.git_at(&at) {
                            g.pending_lists = true;
                        }
                    }
                    ws.refresh_at(at.clone(), window, cx);
                } else if lists {
                    ws.refresh_lists_at(at.clone(), window, cx);
                }
                // Then what waited, unless the refresh above took the turn.
                let next = ws
                    .git_at(&at)
                    .filter(|g| !g.busy && !g.queued.is_empty())
                    .map(|g| g.queued.remove(0));
                if let Some(next) = next {
                    next(ws, window, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// One more button on a failed command's notification: its element id, its label, what it does.
pub(crate) type Extra = (
    &'static str,
    SharedString,
    Rc<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>,
);

/// A failed git command: stderr's first line, Details for all of it, Run in a terminal (a
/// terminal beside the block with the command typed in), and `extra`'s buttons.
pub(crate) fn notify_git_error(
    ws: &Entity<Workspace>,
    at: GitAt,
    err: &GitError,
    extra: Vec<Extra>,
    window: &mut Window,
    cx: &mut App,
) {
    notify_git_failure(ws, at, err, git_error_message(err), extra, window, cx);
}

/// A failed git command said in `message`'s words (git's own stay under Details).
pub(crate) fn notify_git_failure(
    ws: &Entity<Workspace>,
    at: GitAt,
    err: &GitError,
    message: String,
    extra: Vec<Extra>,
    window: &mut Window,
    cx: &mut App,
) {
    eprintln!("templum: {err}");
    let (err, ws) = (err.clone(), ws.clone());
    let notification = Notification::error(message)
        .title(err.command.clone())
        .autohide(false)
        .content(move |_, _, cx| {
            let note = cx.entity();
            let (details, run) = (err.clone(), err.clone());
            let (ws_run, ws_details) = (ws.clone(), ws.clone());
            let (run_at, run_note) = (at.clone(), note.clone());
            let extras: Vec<AnyElement> = extra
                .iter()
                .map(|(id, label, act)| {
                    let (ws, act, note, at) = (ws.clone(), act.clone(), note.clone(), at.clone());
                    Button::new(*id)
                        .label(label.clone())
                        .small()
                        .primary()
                        .on_click(move |_, window, cx| {
                            // The block's own workspace first: block ids repeat across them.
                            ws.update(cx, |ws, cx| {
                                if ws.show_git_block(&at, window, cx) {
                                    act(ws, window, cx)
                                }
                            });
                            note.update(cx, |n, cx| n.dismiss(window, cx));
                        })
                        .into_any_element()
                })
                .collect();
            // Wraps: three buttons are wider than a notification, which clips what overflows.
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .mt(px(6.0))
                .child(
                    Button::new("git-details")
                        .label("Details")
                        .small()
                        .on_click(move |_, window, cx| {
                            // Read now: the caller may be updating the workspace.
                            let mono = ws_details.read(cx).cell.font.family.clone();
                            prompt::details(
                                details.command.clone(),
                                details.stderr.clone(),
                                mono,
                                window,
                                cx,
                            )
                        }),
                )
                .child(
                    Button::new("git-run")
                        .label("Run in a terminal")
                        .small()
                        .on_click(move |_, window, cx| {
                            let command = run.command.clone();
                            ws_run.update(cx, |ws, cx| {
                                if !ws.show_git_block(&run_at, window, cx) {
                                    return;
                                }
                                let block = run_at.block;
                                // The repository's root, else the block's folder (a failed
                                // clone or init has no repository).
                                let folder = ws.space.git(block).map(|g| {
                                    g.repo()
                                        .map_or_else(|| g.folder.clone(), |r| r.root.clone())
                                });
                                if let Some(folder) = folder {
                                    ws.terminal_beside(block, &folder, Some(&command), window, cx);
                                }
                            });
                            run_note.update(cx, |n, cx| n.dismiss(window, cx));
                        }),
                )
                .children(extras)
                .into_any_element()
        });
    show_notification(window, cx, notification);
}

/// A notification about something done ("Discarded src/x.rs"), with buttons that act on the
/// block it is about (Undo): each shows the block's workspace first, and does nothing when the
/// block changed folder since.
pub(crate) fn notify_with(
    ws: &Entity<Workspace>,
    at: GitAt,
    notification: Notification,
    buttons: Vec<Extra>,
    window: &mut Window,
    cx: &mut App,
) {
    let ws = ws.clone();
    let notification = notification.content(move |_, _, cx| {
        let note = cx.entity();
        div()
            .flex()
            .gap(px(6.0))
            .mt(px(6.0))
            .children(buttons.iter().map(|(id, label, act)| {
                let (ws, act, note, at) = (ws.clone(), act.clone(), note.clone(), at.clone());
                Button::new(*id)
                    .label(label.clone())
                    .small()
                    .primary()
                    .on_click(move |_, window, cx| {
                        ws.update(cx, |ws, cx| {
                            if ws.show_git_block(&at, window, cx) {
                                act(ws, window, cx)
                            }
                        });
                        note.update(cx, |n, cx| n.dismiss(window, cx));
                    })
                    .into_any_element()
            }))
            .into_any_element()
    });
    show_notification(window, cx, notification);
}
