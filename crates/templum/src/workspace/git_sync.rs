//! A Git block's Sync and Publish (docs/specs/git.md, section 4, Sync).

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use templum_canvas::BlockId;
use templum_git::Synced;

use crate::git_block::{self, words};
use crate::menu::Target;
use crate::prompt::{self, Choice};

use super::{Extra, Workspace, notify_git_error, notify_with, show_notification};

impl Workspace {
    /// Sync: fetch, fast-forward, push, then "Got 3 commits, sent 2". Both sides new: asks how
    /// to join them.
    pub(crate) fn sync_git(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.run_sync(id, None, window, cx);
    }

    /// A diverged Sync, joined by a rebase (`rebase`) or a merge, then pushed.
    pub(crate) fn sync_join(
        &mut self,
        id: BlockId,
        rebase: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_sync(id, Some(rebase), window, cx);
    }

    fn run_sync(
        &mut self,
        id: BlockId,
        join: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity();
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        git.sync = Some(git_block::Sync::Both);
        git.cancel = Some(cancel.clone());
        let repo = repo.with_cancel(cancel);
        self.act_job(
            self.here(id),
            move || match join {
                None => repo.sync(),
                Some(rebase) => repo.sync_join(rebase),
            },
            move |ws, at, result, window, cx| {
                if let Some(git) = ws.git_at(&at) {
                    git.sync = None;
                    git.cancel = None;
                }
                match result {
                    Ok(Synced::Done { got, sent }) => show_notification(
                        window,
                        cx,
                        Notification::success(words::synced(got, sent)),
                    ),
                    Ok(Synced::Diverged { .. }) => {
                        let (rebase, merge) = (entity.clone(), entity.clone());
                        let block = at.block;
                        prompt::choose(
                            "Your branch and the server's both have new commits.",
                            "",
                            vec![
                                Choice {
                                    id: "git-rebase",
                                    label: "Put my commits on top (rebase)".into(),
                                    primary: true,
                                    act: Rc::new(move |window, cx| {
                                        rebase.update(cx, |ws, cx| {
                                            ws.sync_join(block, true, window, cx)
                                        })
                                    }),
                                },
                                Choice {
                                    id: "git-merge",
                                    label: "Merge".into(),
                                    primary: false,
                                    act: Rc::new(move |window, cx| {
                                        merge.update(cx, |ws, cx| {
                                            ws.sync_join(block, false, window, cx)
                                        })
                                    }),
                                },
                                Choice {
                                    id: "cancel",
                                    label: "Cancel".into(),
                                    primary: false,
                                    act: Rc::new(|_, _| {}),
                                },
                            ],
                            window,
                            cx,
                        );
                    }
                    // Cancelling isn't a failure: nothing to tell.
                    Err(e) if e.cancelled() => {}
                    Err(e) => notify_git_error(&entity, at.clone(), &e, Vec::new(), window, cx),
                }
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    /// Publish branch: to the repository's only remote at once; with several, a menu of them at
    /// the mouse; with none, "No remote" with Run in a terminal (`git remote add origin `
    /// typed, for the user to finish).
    pub(crate) fn publish(
        &mut self,
        id: BlockId,
        branch: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.space.git(id).and_then(|g| g.repo().cloned()) else {
            return;
        };
        let at_mouse = window.mouse_position();
        self.git_job(
            self.here(id),
            move || repo.remotes(),
            move |ws, at, remotes, window, cx| {
                let block = at.block;
                let entity = cx.entity();
                match remotes {
                    Ok(r) if r.len() == 1 => ws.publish_to(block, r[0].clone(), branch, window, cx),
                    Ok(r) if r.is_empty() => {
                        let run: Extra = (
                            "git-add-remote",
                            "Run in a terminal".into(),
                            Rc::new(move |ws, window, cx| {
                                let root = ws
                                    .space
                                    .git(block)
                                    .and_then(|g| g.repo().map(|r| r.root.clone()));
                                if let Some(root) = root {
                                    ws.terminal_beside(
                                        block,
                                        &root,
                                        Some("git remote add origin "),
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        );
                        notify_with(
                            &entity,
                            at,
                            Notification::warning("No remote: add one to publish this branch."),
                            vec![run],
                            window,
                            cx,
                        );
                    }
                    Ok(remotes) => ws.open_menu(
                        Target::GitRemotes {
                            block,
                            branch,
                            remotes,
                        },
                        at_mouse,
                        window,
                        cx,
                    ),
                    Err(e) => notify_git_error(&entity, at, &e, Vec::new(), window, cx),
                }
            },
            window,
            cx,
        );
    }

    /// `git push -u <remote> <branch>`, its button spinning.
    pub(crate) fn publish_to(
        &mut self,
        id: BlockId,
        remote: String,
        branch: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_sync(
            id,
            git_block::Sync::Push,
            move |repo| repo.publish(&remote, &branch),
            |_, at, e, window, cx| {
                let ws = cx.entity();
                notify_git_error(&ws, at, e, Vec::new(), window, cx)
            },
            window,
            cx,
        );
    }
}
