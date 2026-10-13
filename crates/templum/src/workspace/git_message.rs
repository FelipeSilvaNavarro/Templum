//! Commit messages from an agent: Generate ✨ beside a Git block's summary, its ⚙ (the model,
//! this repository's instructions) and Settings ▸ Git's Try it. What runs is
//! `templum_git::message`. See docs/specs/git.md, section 7.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, SharedString, Window, div, px};
use templum_canvas::BlockId;
use templum_git::message::{self, Failure, Request};

use crate::config::Edit;
use crate::prompt;

use super::{GitAt, Workspace, show_notification};

/// The table Generate's settings live in.
const TABLE: &str = "git.commit_message";

/// `[git.repositories]`'s key for a repository: its root with `/` on every OS, as
/// `git rev-parse --show-toplevel` prints it.
pub(crate) fn repository_key(root: &Path) -> String {
    root.to_string_lossy().replace('\\', "/")
}

impl Workspace {
    /// Generate ✨: the first time, asks whether the diff may go to the agent; then writes the
    /// message in the background. A click while it runs stops it.
    pub(crate) fn generate_message(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        if let Some(cancel) = &git.generating {
            cancel.store(true, Ordering::Relaxed);
            return;
        }
        let Some(title) = self
            .commit_agent
            .as_ref()
            .filter(|a| a.ready)
            .map(|a| a.title.clone())
        else {
            return;
        };
        if self.config.git.commit_message.confirmed {
            self.generate_now(id, window, cx);
            return;
        }
        let entity = cx.entity();
        prompt::confirm(
            format!("Send your changes to {title}?"),
            format!(
                "Generate sends the diff of what you commit to {title}, which sends it to its \
                 provider and uses your account. Templum asks this once."
            ),
            "Generate",
            move |window, cx| {
                entity.update(cx, |ws, cx| {
                    ws.edit(
                        Edit::set(TABLE, "confirmed", true),
                        |c| c.git.commit_message.confirmed = true,
                        cx,
                    );
                    ws.generate_now(id, window, cx);
                })
            },
            window,
            cx,
        );
    }

    /// Reads what the commit would take, builds the request and runs the agent, off the UI
    /// thread and beside the block's own commands (it only reads, so they don't wait for it).
    fn generate_now(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self.commit_agent.clone() else {
            return;
        };
        let at = self.here(id);
        let settings = self.config.git.commit_message.clone();
        let repositories = &self.config.git.repositories;
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let Some(repo) = git.repo().cloned() else {
            return;
        };
        // This repository's own rules win over Settings ▸ Git's.
        let instructions = repositories
            .get(&repository_key(&repo.root))
            .and_then(|r| r.commit_instructions.clone())
            .filter(|i| !i.trim().is_empty())
            .unwrap_or(settings.instructions);
        let all = git.commit_all();
        let before = (
            git.summary.read(cx).value(),
            git.description.read(cx).value(),
        );
        let cancel = Arc::new(AtomicBool::new(false));
        git.generating = Some(cancel.clone());
        cx.notify();
        let template = settings
            .prompt
            .unwrap_or_else(|| message::PROMPT.to_string());
        let max_bytes = settings.max_diff_kb as usize * 1024;
        let timeout = Duration::from_secs(settings.timeout);
        let title = agent.title.clone();
        cx.spawn_in(window, async move |ws, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move {
                    let changes = message::changes(&repo, all, max_bytes).map_err(Failure::Git)?;
                    let request = message::request(
                        &template,
                        &Request {
                            instructions: &instructions,
                            files: &changes.files,
                            diff: &changes.diff,
                            recent: &changes.recent,
                            max_bytes,
                        },
                    );
                    message::run(
                        &agent.command,
                        &agent.model,
                        &request,
                        &repo.root,
                        timeout,
                        &cancel,
                    )
                })
                .await;
            ws.update_in(cx, |ws, window, cx| {
                ws.generated(at, &title, before, answer, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// The agent's answer: in the fields, unless the summary or the description changed since
    /// the click (what was typed is never replaced, and a commit made meanwhile cleared them);
    /// a failure says why, with Details; a cancel says nothing.
    // ponytail: a block closed or pointed at another folder meanwhile drops the answer, and its
    // agent runs on to its timeout; set `generating` in `set_git_folder` and on close if a slow
    // agent's cost matters.
    fn generated(
        &mut self,
        at: GitAt,
        agent: &str,
        before: (SharedString, SharedString),
        answer: Result<String, Failure>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.git_at(&at) else {
            return;
        };
        git.generating = None;
        let (summary, description) = (git.summary.clone(), git.description.clone());
        cx.notify();
        let parts = answer.and_then(|a| {
            message::split(&a).ok_or(Failure::Empty {
                stderr: String::new(),
            })
        });
        match parts {
            Err(Failure::Cancelled) => {}
            Err(failure) => self.notify_generate_failure(agent, &failure, window, cx),
            Ok((s, d)) => {
                if (summary.read(cx).value(), description.read(cx).value()) != before {
                    show_notification(
                        window,
                        cx,
                        Notification::info(format!(
                            "Kept your message: it changed while {agent} was writing one"
                        )),
                    );
                    return;
                }
                summary.update(cx, |field, cx| field.set_value(s, window, cx));
                description.update(cx, |field, cx| field.set_value(d, window, cx));
            }
        }
    }

    /// Generate failed: the sentence, and Details with the exit code and what the agent wrote
    /// to stderr. No Run in a terminal: the command's files are gone by now.
    fn notify_generate_failure(
        &self,
        agent: &str,
        failure: &Failure,
        window: &mut Window,
        cx: &mut App,
    ) {
        eprintln!("templum: {agent}: {failure}");
        let title = SharedString::from(format!("{agent}: commit message"));
        let (text, mono) = (failure.details(), self.cell.font.family.clone());
        let notification = Notification::error(failure.to_string())
            .title(title.clone())
            .autohide(false)
            .content(move |_, _, _| {
                let (title, text, mono) = (title.clone(), text.clone(), mono.clone());
                div()
                    .mt(px(6.0))
                    .child(
                        Button::new("generate-details")
                            .label("Details")
                            .small()
                            .on_click(move |_, window, cx| {
                                prompt::details(
                                    title.clone(),
                                    text.clone(),
                                    mono.clone(),
                                    window,
                                    cx,
                                )
                            }),
                    )
                    .into_any_element()
            });
        show_notification(window, cx, notification);
    }
}
