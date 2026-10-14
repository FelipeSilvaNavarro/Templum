//! Agents' session usage and plan limits: the Claude Code status line Templum hands its relay,
//! reading what the agents write, and the notifications (docs/specs/usage.md).

use templum_terminal::Agent;

use super::Workspace;
use crate::agents::AgentEntry;

impl Workspace {
    /// `command` as typed to start `agent` in a terminal: the Claude Code Templum opens gets
    /// Templum's status line, which hands Templum the plan's limits, unless the user turned it
    /// off.
    pub(super) fn typed(&self, agent: Option<&AgentEntry>, command: &str) -> String {
        let claude = agent.is_some_and(|a| a.builtin == Some(Agent::CLAUDE));
        // Under WSL, Templum's paths aren't the shell's.
        let wsl = self.config.shell.program.as_deref().is_some_and(|p| {
            std::path::Path::new(p)
                .file_stem()
                .is_some_and(|s| s == "wsl")
        });
        let arg = match (&self.data_dir, std::env::current_exe()) {
            (Some(data), Ok(exe)) if claude && !wsl && self.config.usage.claude_limits => {
                templum_usage::claude::settings_arg(data, &exe, cfg!(windows))
            }
            _ => None,
        };
        match arg {
            Some(arg) => format!("{command} --settings {arg}"),
            None => command.to_string(),
        }
    }
}
