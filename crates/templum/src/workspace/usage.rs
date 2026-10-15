//! Agents' session usage and plan limits: the Claude Code status line Templum hands its relay,
//! reading what the agents write after each snapshot, and the notifications
//! (docs/specs/usage.md).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::notification::{Notification, NotificationDelivery, NotificationType};
use gpui_kit::{Context, SharedString, Window};
use templum_canvas::BlockId;
use templum_terminal::Agent;
use templum_usage::{Event, Limits, Proc, Session, Watch};

use super::{TryIt, Workspace, show_notification};
use crate::agents::AgentEntry;
use crate::config::Edit;
use crate::notes;
use crate::usage::{TURN_SECS, limit_name, meter_agents, resets_local};

/// How often Templum asks Claude Code for the plan's limits. Its `/usage` asks Anthropic, which
/// limits how often.
pub(super) const CLAUDE_EVERY: Duration = Duration::from_secs(5 * 60);

/// What a read found: each terminal's session, in the order asked, and each agent's limits.
pub(crate) type Read = (Vec<Option<Session>>, Vec<(String, Limits)>);

impl Workspace {
    /// `command` as typed to start `agent` in a terminal (docs/specs/connections.md, What Templum
    /// types into each agent): the Claude Code Templum opens gets its settings file (the status
    /// line of the plan's limits, and the hooks) and its MCP server; Codex gets the server and
    /// its `notify`, unless the shell is cmd (they go in single quotes) or Windows PowerShell 5.1
    /// (it strips the `"` inside them; pwsh 7 doesn't). Nothing under WSL, where Templum's paths
    /// aren't the shell's, nor without a data folder.
    pub(super) fn typed(&self, agent: Option<&AgentEntry>, command: &str) -> String {
        let builtin = agent.and_then(|a| a.builtin);
        let (Some(data), Some(exe), false) = (&self.data_dir, program(), self.shell_is("wsl"))
        else {
            return command.to_string();
        };
        let connected = self.config.connections.enabled && self.bus.is_some();
        let windows = cfg!(windows);
        let mut line = command.to_string();
        if builtin == Some(Agent::CLAUDE) {
            let limits = self.config.usage.claude_limits;
            line.push_str(&claude_flags(data, &exe, windows, limits, connected));
        } else if builtin == Some(Agent::CODEX)
            && connected
            && !self.shell_is("cmd")
            && !self.shell_is("powershell")
            && let Some(flags) = templum_mcp::codex_flags(&exe.to_string_lossy(), windows)
        {
            line.push_str(&flags);
        }
        line
    }

    /// New terminals run the shell named `name` (its program's file name, in any case:
    /// `PowerShell.exe` is `powershell`): the configured one, else the system's default.
    fn shell_is(&self, name: &str) -> bool {
        let program = self
            .config
            .shell
            .program
            .clone()
            .or_else(|| templum_terminal::default_shell().map(|s| s.program));
        program.is_some_and(|p| {
            std::path::Path::new(&p)
                .file_stem()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
        })
    }

    /// Reads, in the background, what the agents in every workspace's terminals wrote, from the
    /// processes the last snapshot found. Without a data folder, nothing.
    pub(super) fn read_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(data) = self.data_dir.clone() else {
            return;
        };
        let mut keys = Vec::new();
        let mut procs = Vec::new();
        for space in self.spaces() {
            for (id, pane) in space.terminals() {
                let seen = &pane.seen;
                if let (Some(agent), Some(pid), Some(started)) =
                    (&seen.agent, seen.pid, seen.started)
                {
                    keys.push((space.id.clone(), id));
                    procs.push(Proc {
                        agent: agent.clone(),
                        pid,
                        started,
                        cwd: seen.folder.clone(),
                        terminal: pane.term.id().to_string(),
                    });
                }
            }
        }
        // The next read replaces this one: a slow disk never piles reads up.
        self._usage = cx.spawn_in(window, async move |ws, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let dirs = templum_usage::Dirs::of(&data);
                    templum_usage::read(&procs, &dirs, notes::now())
                })
                .await;
            ws.update_in(cx, |ws, window, cx| ws.usage_done(keys, read, window, cx))
                .ok();
        });
    }

    /// Keeps what a read found for the terminals `keys` (workspace, block), then tells the user
    /// what the limits mean now.
    pub(crate) fn usage_done(
        &mut self,
        keys: Vec<(String, BlockId)>,
        (sessions, limits): Read,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut limits = limits;
        if let Some(polled) = &self.claude_polled {
            templum_usage::merge(&mut limits, "claude", polled);
        }
        let mut changed = self.limits != limits;
        self.limits = limits;
        // The meter's next agent, every few seconds.
        let now = notes::now();
        if meter_agents(self, now).len() > 1
            && !self.meter_hovered
            && now >= self.meter_turned + TURN_SECS
        {
            self.meter_turn = self.meter_turn.wrapping_add(1);
            self.meter_turned = now;
            changed = true;
        }
        for space in std::iter::once(&mut self.space).chain(&mut self.others) {
            let ids: Vec<BlockId> = space.terminals().map(|(id, _)| id).collect();
            for id in ids {
                let found = keys
                    .iter()
                    .position(|(s, b)| *s == space.id && *b == id)
                    .and_then(|i| sessions.get(i).cloned().flatten());
                let pane = space.terminal_mut(id).expect("a terminal");
                if pane.usage != found {
                    pane.usage = found;
                    changed = true;
                }
            }
        }
        let usage = &self.config.usage;
        let events = self.watch.step(now, &self.limits, usage.reset, usage.warn);
        for event in events {
            self.tell(event, window, cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// A notification about a plan limit; clicking it shows a terminal running that agent.
    fn tell(&self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let now = notes::now();
        let (agent, title, message, kind) = match &event {
            Event::Warn { agent, window } => (
                agent,
                format!(
                    "{}: {:.0}% of the {} limit used",
                    self.agent_title(agent),
                    window.used_percent,
                    limit_name(window.minutes, window.scope.as_deref())
                ),
                format!("Resets {}", resets_local(window.resets_at, now)),
                NotificationType::Warning,
            ),
            Event::Reset {
                agent,
                minutes,
                scope,
                ..
            } => (
                agent,
                format!(
                    "{}'s {} limit has reset",
                    self.agent_title(agent),
                    limit_name(*minutes, scope.as_deref())
                ),
                "It can work again".to_string(),
                NotificationType::Success,
            ),
        };
        let at = self.running(agent);
        let ws = cx.entity().downgrade();
        let notification = Notification::new()
            .id1::<Watch>(SharedString::from(format!("usage-{agent}-{title}")))
            .with_type(kind)
            .title(title)
            .message(message)
            .delivery(self.delivery(window))
            .on_click(move |_, window, cx| {
                if let Some((space, block)) = &at {
                    ws.update(cx, |ws, cx| ws.reveal(space, *block, window, cx))
                        .ok();
                }
            });
        show_notification(window, cx, notification);
    }

    /// The Claude Code to ask for the plan's limits, and the empty folder to ask in; None when
    /// the user turned it off, without a data folder, or when it isn't installed.
    pub(super) fn claude_poll_job(&self) -> Option<(PathBuf, PathBuf)> {
        let data = self.data_dir.as_ref()?;
        if !self.config.usage.claude_limits {
            return None;
        }
        let claude = self
            .agents
            .iter()
            .find(|a| a.builtin == Some(Agent::CLAUDE))?;
        let program = claude.command.split_whitespace().next()?;
        let program = if program.contains(['/', '\\']) {
            PathBuf::from(program)
        } else {
            templum_terminal::locate(program)?
        };
        Some((program, data.join("usage").join("probe")))
    }

    /// Shows `agent`'s limits in the title bar, or stops (Settings ▸ Usage).
    pub(crate) fn show_in_title_bar(&mut self, agent: &str, shown: bool, cx: &mut Context<Self>) {
        let current = self.pending.as_ref().unwrap_or(&self.config);
        let mut hide: Vec<String> = current
            .usage
            .hide
            .iter()
            .filter(|a| *a != agent)
            .cloned()
            .collect();
        if !shown {
            hide.push(agent.to_string());
        }
        let edit = if hide.is_empty() {
            Edit::remove("usage", "hide")
        } else {
            Edit::set(
                "usage",
                "hide",
                hide.iter()
                    .map(String::as_str)
                    .collect::<toml_edit::Array>(),
            )
        };
        self.edit(edit, move |c| c.usage.hide = hide, cx);
    }

    /// Opens or closes the title bar's usage popover.
    pub(crate) fn toggle_usage(&mut self, cx: &mut Context<Self>) {
        self.usage_open = !self.usage_open;
        cx.notify();
    }

    /// Shows the terminal of a session listed in the usage popover, and closes it.
    pub(crate) fn show_session(
        &mut self,
        space: &str,
        block: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.usage_open = false;
        self.reveal(space, block, window, cx);
    }

    /// An agent's name in menus, from its id.
    pub(crate) fn agent_title(&self, id: &str) -> String {
        self.agents
            .iter()
            .find(|a| a.id == id)
            .map_or_else(|| id.to_string(), |a| a.title.clone())
    }

    /// A terminal running the agent `id`, in the active workspace first.
    fn running(&self, id: &str) -> Option<(String, BlockId)> {
        self.spaces().find_map(|space| {
            space
                .terminals()
                .find(|(_, pane)| pane.seen.agent.as_deref() == Some(id))
                .map(|(block, _)| (space.id.clone(), block))
        })
    }

    /// How news reaches the user: also as a system notification while Templum is in the
    /// background, unless turned off.
    pub(super) fn delivery(&self, window: &Window) -> NotificationDelivery {
        if self.config.terminal.notifications && !window.is_window_active() {
            NotificationDelivery::InAppAndSystem
        } else {
            NotificationDelivery::InApp
        }
    }

    /// Settings ▸ Connections ▸ Gemini CLI: installs Templum's extension (`install`) or removes
    /// it, in the background; Settings shows what went wrong, if anything did.
    pub(crate) fn setup_gemini(&mut self, install: bool, cx: &mut Context<Self>) {
        let Some(data) = self.data_dir.clone() else {
            return;
        };
        if matches!(self.gemini_job, Some(TryIt::Running(_))) {
            return;
        }
        self.gemini_job = Some(TryIt::Running(Arc::default()));
        cx.notify();
        cx.spawn(async move |ws, cx| {
            let done = cx
                .background_executor()
                .spawn(
                    async move { gemini_run(templum_terminal::locate("gemini"), &data, install) },
                )
                .await;
            ws.update(cx, |ws, cx| {
                ws.gemini_job = done.err().map(TryIt::Failed);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// Runs Gemini CLI (`gemini`, where Templum found it, if it did) to link Templum's extension,
/// written first (`install`), or to uninstall it; what went wrong, whole, if anything did.
// ponytail: no deadline; a Gemini CLI that hangs keeps the button spinning until Templum quits.
pub(super) fn gemini_run(
    gemini: Option<PathBuf>,
    data: &Path,
    install: bool,
) -> Result<(), String> {
    use templum_usage::gemini;
    let program = gemini.ok_or("Templum didn't find Gemini CLI (gemini) on this computer.")?;
    let args: Vec<OsString> = if install {
        let folder = gemini::extension(data, cfg!(windows)).ok_or(
            "Templum couldn't write its extension: its folder's path holds a ' or a line break, \
             or can't be written.",
        )?;
        gemini::link_args(&folder)
    } else {
        gemini::UNLINK.iter().map(OsString::from).collect()
    };
    let mut command = std::process::Command::new(program);
    command.args(args).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = command
        .output()
        .map_err(|e| format!("Couldn't run Gemini CLI: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Err(format!(
        "Gemini CLI failed ({}):\n{}",
        out.status,
        said.trim()
    ))
}

/// What the Claude Code Templum opens gets after its command, given Templum's data folder and
/// program: the settings file (the status line when `limits`, the hooks when `connected`), then
/// the MCP server, only when that file carries the hooks: without them its dot would never move.
pub(super) fn claude_flags(
    data: &Path,
    exe: &Path,
    windows: bool,
    limits: bool,
    connected: bool,
) -> String {
    let mut flags = String::new();
    let settings = templum_usage::claude::settings_arg(data, exe, windows, limits, connected);
    if let Some(arg) = &settings {
        flags.push_str(&format!(" --settings {arg}"));
    }
    // Last: it takes every word after it.
    if connected
        && settings.is_some()
        && let Some(arg) = templum_usage::claude::mcp_arg(data, exe, windows)
    {
        flags.push_str(&format!(" --mcp-config {arg}"));
    }
    flags
}

/// Templum's program as an agent should run it: the AppImage when it runs from one (its mount
/// goes away when it quits), else the running executable, without the " (deleted)" Linux adds
/// to one an upgrade replaced.
pub(super) fn program() -> Option<PathBuf> {
    if let Some(image) = std::env::var_os("APPIMAGE") {
        return Some(image.into());
    }
    let exe = std::env::current_exe().ok()?;
    Some(
        match exe.to_str().and_then(|e| e.strip_suffix(" (deleted)")) {
            Some(path) => PathBuf::from(path),
            None => exe,
        },
    )
}
