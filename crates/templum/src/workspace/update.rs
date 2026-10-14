//! New versions from GitHub Releases: the check at start and once a day, the download, the
//! notifications, Restart and Install, and the swap when Templum quits (docs/specs/updates.md).

use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, SharedString, Task, Window, div, px};
use templum_update::{Install, Ready, VERSION};

use super::{Workspace, show_notification};
use crate::settings::Section;
use crate::update::{Act, Blocked, Update, why_text};
use crate::{alert, notes, prompt};

/// How often a running Templum looks again.
const EVERY: Duration = Duration::from_secs(24 * 3600);

impl Workspace {
    /// Deletes what an earlier update left, then checks at start and every day while
    /// `[updates] check` is on. Nothing without a data folder (`--bench`), nor in tests.
    pub(super) fn update_loop(
        install: &Install,
        data_dir: Option<&PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let Some(updates) = data_dir.map(|dir| dir.join("updates")) else {
            return Task::ready(());
        };
        if cfg!(test) {
            return Task::ready(());
        }
        let install = install.clone();
        cx.spawn_in(window, async move |ws, cx| {
            cx.background_executor()
                .spawn(async move { templum_update::cleanup(&install, &updates, VERSION) })
                .await;
            loop {
                let checked = ws.update_in(cx, |ws, window, cx| {
                    if ws.config.updates.check {
                        ws.check_update(false, window, cx);
                    }
                });
                if checked.is_err() {
                    break;
                }
                cx.background_executor().timer(EVERY).await;
            }
        })
    }

    /// Asks GitHub for the newest release and, when it is newer and Templum can replace itself,
    /// downloads it. `manual` (Check now): the answer is always told.
    pub(crate) fn check_update(
        &mut self,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.update,
            Update::Checking | Update::Downloading { .. } | Update::Ready { .. }
        ) {
            return;
        }
        let Some(updates) = self.data_dir.as_ref().map(|dir| dir.join("updates")) else {
            return;
        };
        self.update = Update::Checking;
        cx.notify();
        let install = self.install.clone();
        // The download's progress, at most once per percent.
        let (progress, mut heard) = futures::channel::mpsc::unbounded::<(u64, u64)>();
        cx.spawn(async move |ws, cx| {
            while let Some((got, total)) = heard.next().await {
                if ws
                    .update(cx, |ws, cx| ws.downloaded(got, total, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        self._update = cx.spawn_in(window, async move |ws, cx| {
            let agent = templum_update::agent();
            let asked = agent.clone();
            let found = cx
                .background_executor()
                .spawn(async move { templum_update::latest(&asked) })
                .await;
            let state = match found {
                Err(error) => Update::Failed(error),
                Ok(release) if !release.newer_than(VERSION) => Update::UpToDate(notes::now()),
                Ok(release) => {
                    if let Install::Manual(why) = install {
                        Update::Manual {
                            release,
                            blocked: Blocked::Install(why),
                        }
                    } else {
                        let downloading = Update::Downloading {
                            release: release.clone(),
                            got: 0,
                            total: 0,
                        };
                        let shown = ws.update(cx, |ws, cx| {
                            ws.update = downloading;
                            cx.notify();
                        });
                        if shown.is_err() {
                            return;
                        }
                        let wanted = release.clone();
                        let prepared = cx
                            .background_executor()
                            .spawn(async move {
                                let mut last = None;
                                templum_update::prepare(
                                    &agent,
                                    &install,
                                    &wanted,
                                    &updates,
                                    &mut |got, total| {
                                        let percent = (got * 100).checked_div(total);
                                        if last != Some(percent) {
                                            last = Some(percent);
                                            let _ = progress.unbounded_send((got, total));
                                        }
                                    },
                                )
                            })
                            .await;
                        match prepared {
                            Ok(ready) => Update::Ready {
                                release,
                                ready,
                                typed: false,
                            },
                            Err(error) => Update::Manual {
                                release,
                                blocked: Blocked::Failed(error),
                            },
                        }
                    }
                }
            };
            ws.update_in(cx, |ws, window, cx| {
                ws.update_done(state, manual, window, cx)
            })
            .ok();
        });
    }

    fn downloaded(&mut self, got: u64, total: u64, cx: &mut Context<Self>) {
        if let Update::Downloading {
            got: g, total: t, ..
        } = &mut self.update
        {
            (*g, *t) = (got, total);
            cx.notify();
        }
    }

    /// Keeps where the update stands and tells it: news once per version, a check's answer
    /// when `manual`.
    pub(crate) fn update_done(
        &mut self,
        state: Update,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update = state;
        cx.notify();
        let version = |release: &templum_update::Release| release.version.to_string();
        let notes = |release: &templum_update::Release| {
            (
                "update-notes",
                "What's new",
                Act::Open(release.page.clone()),
            )
        };
        let (key, kind, title, message, buttons) = match &self.update {
            Update::Ready {
                release,
                ready: Ready::Terminal { .. },
                ..
            } => (
                format!("downloaded-{}", version(release)),
                NotificationType::Success,
                format!("Templum {} is downloaded", release.version),
                "Install it in a terminal: it asks for your password.".to_string(),
                vec![("update-install", "Install", Act::Install), notes(release)],
            ),
            Update::Ready { release, .. } => (
                format!("ready-{}", version(release)),
                NotificationType::Success,
                format!("Templum {} is ready", release.version),
                "Restart to use it.".to_string(),
                vec![("update-restart", "Restart", Act::Restart), notes(release)],
            ),
            Update::Manual {
                release,
                blocked: Blocked::Install(why),
            } => (
                format!("out-{}", version(release)),
                NotificationType::Info,
                format!("Templum {} is out", release.version),
                why_text(*why).to_string(),
                vec![notes(release)],
            ),
            Update::Manual {
                release,
                blocked: Blocked::Failed(error),
            } => (
                format!("failed-{}", version(release)),
                NotificationType::Error,
                format!("Couldn't update to {}", release.version),
                error.clone(),
                vec![notes(release)],
            ),
            Update::UpToDate(_) if manual => (
                "up-to-date".to_string(),
                NotificationType::Success,
                "Templum is up to date".to_string(),
                format!("Version {VERSION}"),
                Vec::new(),
            ),
            Update::Failed(error) if manual => (
                "check-failed".to_string(),
                NotificationType::Error,
                "Couldn't check for updates".to_string(),
                error.clone(),
                Vec::new(),
            ),
            _ => return,
        };
        if !manual && self.update_told.as_deref() == Some(key.as_str()) {
            return;
        }
        self.update_told = Some(key.clone());
        let ws = cx.entity();
        let notification = Notification::new()
            .id1::<Update>(SharedString::from(format!("update-{key}")))
            .with_type(kind)
            .title(title)
            .message(message)
            .delivery(self.delivery(window))
            .when(!buttons.is_empty(), |n| {
                n.content(move |_, _, cx| {
                    let note = cx.entity();
                    div()
                        .flex()
                        .gap(px(6.0))
                        .mt(px(6.0))
                        .children(buttons.iter().enumerate().map(|(i, (id, label, act))| {
                            let (ws, note, act) = (ws.clone(), note.clone(), act.clone());
                            Button::new(*id)
                                .label(*label)
                                .small()
                                .when(i == 0, |b| b.primary())
                                .on_click(move |_, window, cx| {
                                    ws.update(cx, |ws, cx| ws.update_act(&act, window, cx));
                                    note.update(cx, |n, cx| n.dismiss(window, cx));
                                })
                                .into_any_element()
                        }))
                        .into_any_element()
                })
            });
        show_notification(window, cx, notification);
    }

    /// What the pill, a notification's or Settings' button asked for.
    pub(crate) fn update_act(&mut self, act: &Act, window: &mut Window, cx: &mut Context<Self>) {
        match act {
            Act::Restart => self.restart_to_update(window, cx),
            Act::Install => self.install_update(window, cx),
            Act::Open(page) => cx.open_url(page),
            Act::Settings => {
                if self.settings.as_ref().is_none_or(|p| p.closing) {
                    self.toggle_settings(window, cx);
                }
                self.set_section(Section::Updates, cx);
            }
        }
    }

    /// Restarts into the downloaded version. A busy terminal (an agent, a command) asks first:
    /// Templum opens the terminals again, not what ran in them.
    pub(crate) fn restart_to_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.update, Update::Ready { .. }) {
            return;
        }
        let busy: Vec<String> = self
            .spaces()
            .flat_map(|space| space.terminals())
            .filter_map(|(_, pane)| {
                let why = alert::busy(&pane.term.status(), self.agent_name(pane))?;
                Some(format!("{}: {why}", pane.term.title()))
            })
            .collect();
        if busy.is_empty() {
            self.restart_now(window, cx);
            return;
        }
        let ws = cx.entity();
        prompt::confirm(
            "Restart Templum to update?",
            format!(
                "{} Templum opens the terminals again, not the programs in them.",
                busy.join(" ")
            ),
            "Restart",
            move |window, cx| ws.update(cx, |ws, cx| ws.restart_now(window, cx)),
            window,
            cx,
        );
    }

    /// Asks about unsaved files as quitting does, then restarts.
    fn restart_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ws = cx.entity();
        let then = move |cx: &mut App| ws.update(cx, |ws, cx| ws.restart_into_update(cx));
        if self.may_close_then(then, window, cx) {
            self.restart_into_update(cx);
        }
    }

    /// Quits, then opens the new version: GPUI's restart waits for this process to end. An
    /// installer runs once Templum has saved and quit ([`Workspace::finish_update`]).
    fn restart_into_update(&mut self, cx: &mut Context<Self>) {
        let Update::Ready { ready, .. } = &self.update else {
            return;
        };
        match ready {
            Ready::Swap { restart, .. } | Ready::Terminal { restart, .. } => {
                cx.set_restart_path(restart.clone());
                cx.restart();
            }
            Ready::Installer(launch) => {
                self.relaunch = Some(launch.clone());
                cx.quit();
            }
        }
    }

    /// Opens a terminal in the download's folder with the package manager's command typed, not
    /// run: the person reads it, presses Enter and gives their password.
    pub(crate) fn install_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Update::Ready {
            ready: Ready::Terminal {
                folder, command, ..
            },
            typed,
            ..
        } = &mut self.update
        else {
            return;
        };
        *typed = true;
        let (folder, command) = (folder.clone(), command.clone());
        let rect = self.new_terminal_rect(window);
        if let Some(pane) = self
            .spawn_block(rect, Some(folder), window, cx)
            .and_then(|id| self.space.terminal(id))
        {
            pane.term.write(command.into_bytes());
        }
        cx.notify();
    }

    /// On quit: a downloaded file takes the old one's place, and an installer asked for by
    /// Restart starts. After the canvas is saved: the NSIS installer closes every Templum.
    pub(super) fn finish_update(&mut self) {
        if let Update::Ready {
            ready: Ready::Swap { new, target, .. },
            ..
        } = &self.update
            && let Err(e) = templum_update::swap(new, target)
        {
            eprintln!(
                "templum: couldn't put the new version at {}: {e}",
                target.display()
            );
        }
        if let Some(launch) = self.relaunch.take()
            && let Err(e) = launch.spawn()
        {
            eprintln!("templum: couldn't start {}: {e}", launch.program.display());
        }
    }
}
