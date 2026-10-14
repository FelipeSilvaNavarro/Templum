//! Agents' session usage and plan limits: the Claude Code status line Templum hands its relay,
//! reading what the agents write after each snapshot, and the notifications
//! (docs/specs/usage.md).

use gpui_kit::component::notification::{Notification, NotificationDelivery, NotificationType};
use gpui_kit::{Context, SharedString, Window};
use templum_canvas::BlockId;
use templum_terminal::Agent;
use templum_usage::{Event, Limits, Proc, Session, Watch};

use super::{Workspace, show_notification};
use crate::agents::AgentEntry;
use crate::notes;
use crate::usage::{resets_local, window_name};

/// What a read found: each terminal's session, in the order asked, and each agent's limits.
pub(crate) type Read = (Vec<Option<Session>>, Vec<(String, Limits)>);

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
        let mut changed = self.limits != limits;
        self.limits = limits;
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
        let usage = self.config.usage;
        let events = self
            .watch
            .step(notes::now(), &self.limits, usage.reset, usage.warn);
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
                    window_name(window.minutes)
                ),
                format!("Resets {}", resets_local(window.resets_at, now)),
                NotificationType::Warning,
            ),
            Event::Reset { agent, minutes, .. } => (
                agent,
                format!(
                    "{}'s {} limit has reset",
                    self.agent_title(agent),
                    window_name(*minutes)
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
}
