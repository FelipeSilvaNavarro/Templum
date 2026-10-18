//! `remove_worktree` and `merge_worktree` (docs/specs/agent-tools.md#remove_worktree-and-merge_worktree):
//! the worktrees `spawn_agent` makes, removed or merged back, asking the person first when it
//! would lose work or change their branch.

use gpui_kit::Context;
use templum_mcp::{Reply, tools};

use super::Workspace;
use super::bus::Outcome;
use super::waits::{Approved, Waiting};

impl Workspace {
    /// The worktree `name` of the repository of `folder` (the caller's when None, relative to it),
    /// and its state; refused when there is none, or a terminal block works in it.
    fn worktree_for(
        &self,
        from: &str,
        name: &str,
        folder: Option<&str>,
    ) -> Result<(templum_git::Repo, templum_git::WorktreeState), Reply> {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let folder = space.folder_for(me, folder).map_err(Reply::err)?;
        let (repo, state) = self.worktree_state(&folder, name).map_err(Reply::err)?;
        let root = state
            .path
            .canonicalize()
            .unwrap_or_else(|_| state.path.clone());
        // Any workspace's: a terminal there would lose its folder.
        let worker = self.spaces().find_map(|space| {
            space
                .terminals()
                .find(|(_, pane)| pane.works_in(&state.path) || pane.works_in(&root))
                .map(|(id, _)| space.uids.get(&id).cloned().unwrap_or_default())
        });
        if let Some(uid) = worker {
            return Err(Reply::err(format!(
                "{uid} works in it: stop_agent or close_block it first."
            )));
        }
        // Its commits, or another branch checked out there, would be lost or left out.
        match state.branch.as_deref() {
            Some(branch) if branch == name => Ok((repo, state)),
            Some(other) => Err(Reply::err(format!(
                "The worktree {name} is on {other} now, not on its branch {name}: check out {name} \
                 there first."
            ))),
            None => Err(Reply::err(format!(
                "The worktree {name} has a detached HEAD, not its branch {name}: check out {name} \
                 there first."
            ))),
        }
    }

    /// `remove_worktree` from the terminal `from`: at once when the worktree has no changes and
    /// its commits are merged; with `discard`, once the person said Delete.
    pub(super) fn remove_worktree(
        &mut self,
        from: &str,
        name: &str,
        folder: Option<&str>,
        discard: bool,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let (repo, state) = match self.worktree_for(from, name, folder) {
            Ok(found) => found,
            Err(refused) => return Outcome::Now(refused),
        };
        if state.changed == 0 && state.unmerged == 0 {
            return Outcome::Later(self.worktree_remove(repo, name.to_string(), false, cx));
        }
        if !discard {
            return Outcome::Now(Reply::err(format!(
                "{name} has {} changed files and {} commits not merged into {}: merge_worktree \
                 merges them, discard: true loses them.",
                state.changed, state.unmerged, state.into
            )));
        }
        let then = Approved::RemoveWorktree {
            repo,
            name: name.to_string(),
            changed: state.changed,
            unmerged: state.unmerged,
        };
        Outcome::Wait(Box::new(Waiting::Allow { then }), Some(tools::ASK_WAIT))
    }

    /// `merge_worktree` from the terminal `from`: the worktree's branch merged into the branch of
    /// `folder`'s working tree, once the person said Merge; refused first when there is nothing
    /// to merge, or changes not committed on either side, or another merge under way there.
    pub(super) fn merge_worktree(
        &mut self,
        from: &str,
        name: &str,
        folder: Option<&str>,
    ) -> Outcome {
        let (repo, state) = match self.worktree_for(from, name, folder) {
            Ok(found) => found,
            Err(refused) => return Outcome::Now(refused),
        };
        let into = state.into.clone();
        let refused = if state.changed > 0 {
            Some(format!(
                "{name} has {} changed files. Commit them first: a merge takes commits.",
                state.changed
            ))
        } else if state.into_changed > 0 {
            Some(format!(
                "{into} has changes not committed in {}: commit or stash them first.",
                repo.root.display()
            ))
        } else if into == "HEAD" {
            Some(format!(
                "{} has a detached HEAD: check out the branch to merge into first.",
                repo.root.display()
            ))
        } else if state.busy {
            Some(format!(
                "A merge, rebase or cherry-pick is under way in {}: it ends first.",
                repo.root.display()
            ))
        } else if state.unmerged == 0 {
            Some(format!("{name} has nothing {into} doesn't."))
        } else {
            None
        };
        if let Some(refused) = refused {
            return Outcome::Now(Reply::err(refused));
        }
        let then = Approved::MergeWorktree {
            repo,
            name: name.to_string(),
            unmerged: state.unmerged,
            into,
        };
        Outcome::Wait(Box::new(Waiting::Allow { then }), Some(tools::ASK_WAIT))
    }
}
