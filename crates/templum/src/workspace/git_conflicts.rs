//! A Git block's conflicts: a region's part chosen, a whole file's version chosen, Mark
//! resolved. Each choice keeps a copy of the file in the trash first. See docs/specs/git.md,
//! section 5, Conflicts.

use std::time::SystemTime;

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use templum_canvas::BlockId;
use templum_git::conflict::{Region, Take, regions, resolve};
use templum_git::trash;

use crate::prompt;

use super::git_discard::REFUSED;
use super::{Workspace, notify_git_error, show_notification};

/// Why a choice didn't happen.
enum Refused {
    /// The region isn't what was shown any more.
    Changed,
    /// The copy in the trash couldn't be made: nothing was changed.
    Keep(std::io::Error),
    Io(std::io::Error),
    Git(templum_git::GitError),
}

/// A refusal said: "The file changed; try again", why the copy failed, or git's error.
fn tell(at: super::GitAt, refused: Refused, window: &mut Window, cx: &mut Context<Workspace>) {
    let message = match refused {
        Refused::Changed => REFUSED.to_string(),
        Refused::Keep(e) => {
            format!("Couldn't keep a copy of the file, so nothing was changed: {e}")
        }
        Refused::Io(e) => format!("Couldn't write the file: {e}"),
        Refused::Git(e) => {
            let entity = cx.entity();
            notify_git_error(&entity, at, &e, Vec::new(), window, cx);
            return;
        }
    };
    eprintln!("templum: {message}");
    show_notification(window, cx, Notification::error(message));
}

impl Workspace {
    /// A region's part (or both) replaces the region in the file, after a copy in the trash;
    /// refused when the region isn't the one shown (`shown`: its two parts' lines as drawn).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_region(
        &mut self,
        id: BlockId,
        path: String,
        region: usize,
        take: Take,
        shown: (Vec<String>, Vec<String>),
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
        let keep = self.trash.clone();
        self.act_job(
            self.here(id),
            move || -> Result<(), Refused> {
                let full = root.join(&path);
                let bytes = std::fs::read(&full).map_err(Refused::Io)?;
                let text = String::from_utf8(bytes).map_err(|_| Refused::Changed)?;
                let r = regions(&text)
                    .into_iter()
                    .nth(region)
                    .ok_or(Refused::Changed)?;
                if (
                    Region::lines(&text, &r.first),
                    Region::lines(&text, &r.second),
                ) != shown
                {
                    return Err(Refused::Changed);
                }
                let resolved = resolve(&text, region, take).ok_or(Refused::Changed)?;
                let paths = [path.clone()];
                let (files, size) = trash::measure(&root, &paths);
                let kept = match keep.as_ref().filter(|_| trash::fits(files, size)) {
                    Some(t) => {
                        let what = format!("Chose a part of {path}");
                        let k = t
                            .put(&root, &paths, &what, SystemTime::now())
                            .map_err(Refused::Keep)?;
                        Some((t, k))
                    }
                    None => None,
                };
                if let Err(e) = std::fs::write(&full, resolved) {
                    if let Some((t, k)) = &kept {
                        let _ = t.remove(&k.id);
                    }
                    return Err(Refused::Io(e));
                }
                if let Some((t, k)) = kept {
                    let _ = t.seal(&k);
                    let _ = t.prune(SystemTime::now());
                }
                Ok(())
            },
            |ws, at, done, window, cx| {
                if let Err(refused) = done {
                    tell(at.clone(), refused, window, cx);
                }
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    /// A whole file's version (`git checkout --ours` / `--theirs`), after a copy in the trash.
    /// A copy that fails stops it: the file may hold hand edits git keeps nowhere.
    pub(crate) fn resolve_file(
        &mut self,
        id: BlockId,
        path: String,
        side: templum_git::Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.space.git(id).and_then(|g| g.repo().cloned()) else {
            return;
        };
        let keep = self.trash.clone();
        self.act_job(
            self.here(id),
            move || -> Result<(), Refused> {
                let paths = [path.clone()];
                let (files, size) = trash::measure(&repo.root, &paths);
                let what = format!("Chose a version of {path}");
                let kept = match keep.as_ref().filter(|_| trash::fits(files, size)) {
                    Some(t) => {
                        let k = t
                            .put(&repo.root, &paths, &what, SystemTime::now())
                            .map_err(Refused::Keep)?;
                        Some((t, k))
                    }
                    None => None,
                };
                if let Err(e) = repo.resolve(&path, side) {
                    if let Some((t, k)) = &kept {
                        let _ = t.remove(&k.id);
                    }
                    return Err(Refused::Git(e));
                }
                if let Some((t, k)) = kept {
                    let _ = t.seal(&k);
                    let _ = t.prune(SystemTime::now());
                }
                Ok(())
            },
            |ws, at, done, window, cx| {
                if let Err(refused) = done {
                    tell(at.clone(), refused, window, cx);
                }
                ws.after_action(at, window, cx);
            },
            window,
            cx,
        );
    }

    /// Mark resolved: stages the file; with conflict markers still in it, asks first.
    pub(crate) fn mark_resolved(
        &mut self,
        id: BlockId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        if !git.markers_left(&path) {
            self.stage(id, vec![path], window, cx);
            return;
        }
        let (entity, at) = (cx.entity(), self.here(id));
        prompt::confirm(
            format!("Mark {path} resolved?"),
            "It still has conflict markers: Git will record them as they are.",
            "Mark resolved",
            move |window, cx| {
                let (path, at) = (path.clone(), at.clone());
                entity.update(cx, |ws, cx| {
                    if ws.show_git_block(&at, window, cx) {
                        ws.stage(at.block, vec![path], window, cx)
                    }
                })
            },
            window,
            cx,
        );
    }
}
