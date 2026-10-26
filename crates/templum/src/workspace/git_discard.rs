//! A Git block's discards: asked first, measured, a copy kept in the trash when it fits, Undo on
//! the notification, Recently discarded… to put one back later. See docs/specs/git.md,
//! section 4, The trash.

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use std::rc::Rc;
use templum_git::ops::{self, Discarding, Failure};
use templum_git::trash::{self, Restored};

use crate::editor::human;
use crate::git_block;
use crate::prompt;

use super::{
    Extra, GitAt, Workspace, notify_git_error, notify_git_failure, notify_with, show_notification,
};

/// What a hunk's patch git refuses says: the file changed since it was read.
pub(crate) use templum_git::ops::CHANGED as REFUSED;
pub(crate) use templum_git::ops::thousands;

impl Workspace {
    /// Discard changes… and Discard all…: the unstaged changes of `paths`, after the question.
    pub(crate) fn discard(
        &mut self,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git() else {
            return;
        };
        let entries: Vec<templum_git::Entry> = git
            .status
            .unstaged()
            .into_iter()
            .filter(|e| paths.contains(&e.path))
            .cloned()
            .collect();
        let (title, done) = match entries.as_slice() {
            [] => return,
            [one] => (
                format!("Discard the changes to {}?", one.path),
                format!("Discarded {}", one.path),
            ),
            many => (
                format!("Discard {} files?", thousands(many.len())),
                format!("Discarded {} files", thousands(many.len())),
            ),
        };
        let paths = entries.iter().map(|e| e.path.clone()).collect();
        self.ask_discard(paths, title, done, Discarding::Files(entries), window, cx);
    }

    /// A hunk's Discard: `patch` reversed on `path`, after the question.
    pub(crate) fn discard_hunk(
        &mut self,
        path: String,
        patch: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = format!("Discard this change to {path}?");
        let done = format!("Discarded a change to {path}");
        self.ask_discard(vec![path], title, done, Discarding::Hunk(patch), window, cx);
    }

    /// Measures what a copy would take (in the background: a build folder is thousands of
    /// files), then asks, saying whether the discard can be undone.
    #[allow(clippy::too_many_arguments)]
    fn ask_discard(
        &mut self,
        paths: Vec<String>,
        title: String,
        done: String,
        what: Discarding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self
            .space
            .git()
            .and_then(|g| g.repo().map(|r| r.root.clone()))
        else {
            return;
        };
        let measured = paths.clone();
        self.git_job(
            self.here(),
            move || trash::measure(&root, &measured),
            move |ws, at, (files, bytes), window, cx| {
                let keep = ws.trash.is_some() && trash::fits(files, bytes);
                let why = if keep {
                    let them = if paths.len() == 1 { "it" } else { "them" };
                    format!("You can restore {them} from Recently discarded.")
                } else if ws.trash.is_some() {
                    let size = if files > trash::MAX_FILES {
                        format!("{} files", thousands(files))
                    } else {
                        human(bytes)
                    };
                    format!("Too much to keep a copy: {size}. This can't be undone.")
                } else {
                    "This can't be undone.".to_string()
                };
                let entity = cx.entity();
                prompt::confirm(
                    title,
                    why,
                    "Discard",
                    move |window, cx| {
                        let (paths, done, what, at) =
                            (paths.clone(), done.clone(), what.clone(), at.clone());
                        entity.update(cx, |ws, cx| {
                            ws.discard_now(at, paths, done, what, keep, window, cx)
                        })
                    },
                    window,
                    cx,
                );
            },
            window,
            cx,
        );
    }

    /// The discard, in the background: the copy first (with `keep`), then git, then the copy's
    /// seal. A copy that can't be made stops it before anything is thrown away.
    #[allow(clippy::too_many_arguments)]
    fn discard_now(
        &mut self,
        at: GitAt,
        paths: Vec<String>,
        done: String,
        what: Discarding,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.git_at(&at).and_then(|g| g.repo().cloned()) else {
            return;
        };
        let trash = self.trash.clone().filter(|_| keep);
        let hunk = matches!(what, Discarding::Hunk(_));
        let kept_as = done.clone();
        self.act_job(
            at,
            move || ops::discard(&repo, trash.as_ref(), &paths, &kept_as, &what),
            move |ws, at, result, window, cx| {
                let entity = cx.entity();
                match result {
                    Ok(Some(kept)) => {
                        let entry = kept.id;
                        let undo: Extra = (
                            "git-undo",
                            "Undo".into(),
                            Rc::new(
                                move |ws: &mut Workspace,
                                      window: &mut Window,
                                      cx: &mut Context<Workspace>| {
                                    ws.restore_discarded(entry.clone(), false, window, cx)
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
                    Err(Failure::Keep(e)) => {
                        let message =
                            format!("Couldn't keep a copy, so nothing was discarded: {e}");
                        eprintln!("templum: {message}");
                        show_notification(window, cx, Notification::error(message));
                    }
                    Err(Failure::Git(e)) if hunk => notify_git_failure(
                        &entity,
                        at.clone(),
                        &e,
                        REFUSED.to_string(),
                        Vec::new(),
                        window,
                        cx,
                    ),
                    Err(Failure::Git(e)) => {
                        notify_git_error(&entity, at.clone(), &e, Vec::new(), window, cx)
                    }
                    // A discard has no other way to fail.
                    Err(other) => {
                        show_notification(window, cx, Notification::error(other.sentence()))
                    }
                }
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    /// Puts discard `entry` back (Undo, or Restore in Recently discarded…). A file changed since
    /// the discard asks before it is written over (`force`).
    pub(crate) fn restore_discarded(
        &mut self,
        entry: String,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(trash) = self.trash.clone() else {
            return;
        };
        let job_entry = entry.clone();
        self.act_job(
            self.here(),
            move || trash.restore(&job_entry, force),
            move |ws, at, result, window, cx| {
                match result {
                    Ok(Restored::Done) => {}
                    Ok(Restored::Changed(paths)) => {
                        let title = match paths.as_slice() {
                            [one] => format!("{one} changed since it was discarded"),
                            many => {
                                format!("{} files changed since they were discarded", many.len())
                            }
                        };
                        let entity = cx.entity();
                        prompt::confirm(
                            title,
                            "Restoring puts the discarded version back over the changes.",
                            "Restore",
                            move |window, cx| {
                                let entry = entry.clone();
                                entity.update(cx, |ws, cx| {
                                    ws.restore_discarded(entry, true, window, cx)
                                })
                            },
                            window,
                            cx,
                        );
                    }
                    Err(e) => {
                        let message = format!("Couldn't restore: {e}");
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

    /// Recently discarded…: this repository's discards still in the trash, read in the
    /// background, then listed.
    pub(crate) fn show_discarded(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(trash), Some(root)) = (
            self.trash.clone(),
            self.space
                .git()
                .and_then(|g| g.repo().map(|r| r.root.clone())),
        ) else {
            return;
        };
        self.git_job(
            self.here(),
            move || ops::discarded(&trash, &root),
            |ws, _, kept, window, cx| {
                let (entity, dim) = (cx.entity(), ws.ui.title_dim);
                git_block::discarded::open(entity, kept, dim, window, cx);
            },
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn thousands_are_grouped_by_three() {
        assert_eq!(super::thousands(500), "500");
        assert_eq!(super::thousands(1240), "1,240");
        assert_eq!(super::thousands(1_000_000), "1,000,000");
    }
}
