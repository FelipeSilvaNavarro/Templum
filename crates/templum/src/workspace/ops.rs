//! The workspace operations both MCP servers call, each taking its target, never a caller
//! (docs/specs/agent-tools.md#shared-operations).

use gpui_kit::Context;
use templum_canvas::BlockId;

use super::Workspace;

/// What [`Workspace::stop_in`] ended: the program in front of the shell (its agent's name, else its
/// process's; empty when only what it left ran), and how many processes besides it.
pub(super) struct Stopped {
    pub program: String,
    pub processes: usize,
}

impl Workspace {
    /// Ends what runs in the terminal `block` of the workspace `space`: the program in front of
    /// its shell, everything below it, and what it left running after its parent quit; the shell
    /// stays, at its prompt (docs/specs/agent-tools.md#stop_agent). SIGTERM now, SIGKILL KILL_AFTER
    /// later to what is left.
    pub(super) fn stop_in(
        &mut self,
        space: &str,
        block: BlockId,
        cx: &mut Context<Self>,
    ) -> Result<Stopped, String> {
        let space = self.space_named(space).ok_or("That workspace is gone.")?;
        let uid = space.uids.get(&block).map_or("", String::as_str);
        let nothing = format!("Nothing runs in {uid}: its shell is at its prompt.");
        let pane = space
            .terminal(block)
            .ok_or_else(|| format!("{uid} isn't a terminal."))?;
        let shells = pane.term.shells();
        if shells.is_empty() {
            return Err(nothing);
        }
        let program = pane.term.program(&shells, pane.seen.pid);
        let front = program.as_ref().map(|(pid, _)| *pid);
        let found = templum_terminal::strays(pane.term.id(), &shells, front);
        if found.is_empty() {
            return Err(nothing);
        }
        let program = match program.filter(|(pid, _)| found.iter().any(|r| r.pid == *pid)) {
            Some((_, name)) => self.agent_name(pane).map_or(name, str::to_string),
            None => String::new(),
        };
        let processes = found.iter().filter(|r| Some(r.pid) != front).count();
        templum_terminal::terminate(&found);
        self.end_later(found, cx);
        Ok(Stopped { program, processes })
    }
}
