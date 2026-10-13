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

use crate::config::{self, Edit};
use crate::prompt;

use super::{GitAt, Workspace, show_notification};

/// The table Generate's settings live in.
const TABLE: &str = "git.commit_message";

/// Settings ▸ Git ▸ Try it: running (setting it stops the agent), or what came back.
#[derive(Debug)]
pub(crate) enum TryIt {
    Running(Arc<AtomicBool>),
    Answer(String),
    /// What went wrong, whole: the sentence, the exit code, stderr.
    Failed(String),
}

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

    /// The model Generate asks for; empty: the agent's recommended one (the key goes).
    pub(crate) fn set_commit_model(&mut self, model: &str, cx: &mut Context<Self>) {
        let model = model.trim();
        if model.is_empty() {
            self.edit(
                Edit::remove(TABLE, "model"),
                |c| c.git.commit_message.model = None,
                cx,
            );
        } else {
            let chosen = model.to_string();
            self.edit(
                Edit::set(TABLE, "model", model),
                |c| c.git.commit_message.model = Some(chosen),
                cx,
            );
        }
    }

    /// `[git.repositories."<root>"].commit_instructions`: they win over Settings ▸ Git's in
    /// this repository; empty: the global ones again.
    // ponytail: removing the last key leaves an empty `[git.repositories."…"]` line in the file;
    // drop the table in `save_edits` when someone minds it.
    pub(crate) fn set_repo_instructions(
        &mut self,
        root: &Path,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let key = repository_key(root);
        let table = config::repository_table(&key);
        let text = text.trim().to_string();
        if text.is_empty() {
            self.edit(
                Edit::remove(table, "commit_instructions"),
                |c| {
                    c.git.repositories.remove(&key);
                },
                cx,
            );
        } else {
            self.edit(
                Edit::set(table, "commit_instructions", text.as_str()),
                |c| {
                    c.git
                        .repositories
                        .entry(key)
                        .or_default()
                        .commit_instructions = Some(text)
                },
                cx,
            );
        }
    }

    /// The ⚙'s Instructions for this repository…: a field holding them, Save writes them.
    pub(crate) fn ask_repo_instructions(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self
            .space
            .git(id)
            .and_then(|g| g.repo())
            .map(|r| r.root.clone())
        else {
            return;
        };
        let current = self
            .config
            .git
            .repositories
            .get(&repository_key(&root))
            .and_then(|r| r.commit_instructions.clone())
            .unwrap_or_default();
        let entity = cx.entity();
        prompt::ask_text(
            "Instructions for this repository",
            "Save",
            current,
            move |text, _, cx| {
                entity.update(cx, |ws, cx| ws.set_repo_instructions(&root, &text, cx));
                true
            },
            window,
            cx,
        );
    }

    /// Settings ▸ Git ▸ Agent: Generate runs `id`; the model goes back to its recommended one
    /// (a model belongs to its agent).
    pub(crate) fn set_commit_agent(&mut self, id: &str, cx: &mut Context<Self>) {
        let chosen = id.to_string();
        self.edit(
            Edit::set(TABLE, "agent", id),
            |c| c.git.commit_message.agent = Some(chosen),
            cx,
        );
        self.edit(
            Edit::remove(TABLE, "model"),
            |c| c.git.commit_message.model = None,
            cx,
        );
    }

    /// Settings ▸ Git ▸ Try it: the agent on a small sample change (none of the person's code
    /// is sent), its answer or its whole error shown under the button. It runs even when the
    /// agent isn't ready: its error says why. A click while it runs stops it.
    pub(crate) fn try_message(&mut self, cx: &mut Context<Self>) {
        if let Some(TryIt::Running(cancel)) = &self.message_try {
            cancel.store(true, Ordering::Relaxed);
            return;
        }
        let Some(agent) = self.commit_agent.clone() else {
            return;
        };
        let settings = self.config.git.commit_message.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.message_try = Some(TryIt::Running(cancel.clone()));
        cx.notify();
        let template = settings
            .prompt
            .unwrap_or_else(|| message::PROMPT.to_string());
        let timeout = Duration::from_secs(settings.timeout);
        let instructions = settings.instructions;
        cx.spawn(async move |ws, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move {
                    let files = [message::SAMPLE_FILE.to_string()];
                    let request = message::request(
                        &template,
                        &Request {
                            instructions: &instructions,
                            files: &files,
                            diff: message::SAMPLE_DIFF,
                            recent: &[],
                            max_bytes: usize::MAX,
                        },
                    );
                    message::run(
                        &agent.command,
                        &agent.model,
                        &request,
                        &std::env::temp_dir(),
                        timeout,
                        &cancel,
                    )
                })
                .await;
            ws.update(cx, |ws, cx| {
                ws.message_try = match answer {
                    Err(Failure::Cancelled) => None,
                    Ok(answer) => Some(TryIt::Answer(answer.trim().to_string())),
                    Err(failure) => Some(TryIt::Failed(failure.details())),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
