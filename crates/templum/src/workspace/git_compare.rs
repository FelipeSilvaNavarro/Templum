//! A Git block's comparison: a branch against the default one (or a base chosen), its commits
//! on the left, its diff in the panel. See docs/specs/git.md, section 5, Comparison.

use std::rc::Rc;

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use templum_canvas::BlockId;
use templum_git::{Commit, FileDiff, GitError};

use crate::git_block::diff_view::{DiffPanel, Source};
use crate::git_block::{self, Compare, Tab, words};

use super::{GitAt, Workspace, show_notification};

impl Workspace {
    /// Compare `branch` with the default branch: the Branches tab shows it.
    pub(crate) fn compare_branch(
        &mut self,
        id: BlockId,
        branch: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let Some(base) = git.default_branch.clone() else {
            show_notification(
                window,
                cx,
                Notification::warning("No main branch to compare with."),
            );
            return;
        };
        git.compare = Some(Compare {
            branch,
            base,
            commits: Rc::new(Vec::new()),
            narrowed: None,
            panel: DiffPanel::range(String::new()),
        });
        git.tab = Tab::Branches;
        cx.notify();
        let at = self.here(id);
        self.refresh_lists_at(at, window, cx);
    }

    /// Reads the comparison: its commits, and the whole diff or the narrowed commit's. A result
    /// for a comparison changed meanwhile (another branch, base or commit) is dropped.
    pub(crate) fn load_compare(&mut self, at: GitAt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(git) = self.git_at(&at) else {
            return;
        };
        let (Some(repo), Some(c)) = (git.repo().cloned(), git.compare.as_ref()) else {
            return;
        };
        let asked = (c.branch.clone(), c.base.clone(), c.narrowed.clone());
        let ignore_ws = c.panel.ignore_ws;
        let (branch, base, narrowed) = asked.clone();
        self.git_job(
            at,
            move || -> Result<(Vec<Commit>, Vec<FileDiff>), GitError> {
                let commits = repo.compare_log(&base, &branch)?;
                let files = match &narrowed {
                    Some(hash) => repo.commit_changes(hash, ignore_ws)?,
                    None => repo.range_diff(&base, &branch, ignore_ws)?,
                };
                Ok((commits, files))
            },
            move |ws, at, read, _, cx| {
                let Some(git) = ws.git_at(&at) else {
                    return;
                };
                let Some(c) = git.compare.as_mut().filter(|c| {
                    (&c.branch, &c.base, &c.narrowed) == (&asked.0, &asked.1, &asked.2)
                }) else {
                    return;
                };
                match read {
                    Ok((commits, files)) => {
                        let title = match &c.narrowed {
                            Some(hash) => format!(
                                "{} vs {} · commit {}",
                                c.branch,
                                c.base,
                                &hash[..hash.len().min(7)]
                            ),
                            None => format!(
                                "{} vs {} · {}",
                                c.branch,
                                c.base,
                                words::commits(commits.len() as u32)
                            ),
                        };
                        c.panel.source = Source::Range { title };
                        c.commits = Rc::new(commits);
                        c.panel.set_files(files);
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

    /// A commit of the comparison (Some) narrows the panel to it; the heading (None) shows them
    /// all again.
    pub(crate) fn narrow_compare(
        &mut self,
        id: BlockId,
        hash: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(c) = self.space.git_mut(id).and_then(|g| g.compare.as_mut()) else {
            return;
        };
        c.narrowed = hash;
        let at = self.here(id);
        self.load_compare(at, window, cx);
    }

    /// The column's ←: back to the branches.
    pub(crate) fn close_compare(&mut self, id: BlockId, cx: &mut Context<Self>) {
        if let Some(git) = self.space.git_mut(id) {
            git.compare = None;
            cx.notify();
        }
    }

    /// The base button: any other branch, local or remote, from a searchable list.
    pub(crate) fn choose_compare_base(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        let Some(branch) = git.compare.as_ref().map(|c| c.branch.clone()) else {
            return;
        };
        let names: Vec<String> = git
            .branches
            .iter()
            .map(|b| b.name.clone())
            .filter(|b| *b != branch)
            .collect();
        git_block::switcher::pick(
            cx.entity(),
            "Compare with",
            names,
            move |ws, base, window, cx| {
                if let Some(c) = ws.space.git_mut(id).and_then(|g| g.compare.as_mut()) {
                    c.base = base;
                    c.narrowed = None;
                }
                let at = ws.here(id);
                ws.load_compare(at, window, cx);
            },
            window,
            cx,
        );
    }
}
