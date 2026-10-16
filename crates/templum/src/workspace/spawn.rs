//! `spawn_agent` (docs/specs/connections.md, Spawning): a new agent block beside the caller, wired
//! to it, in its folder, another, or a new Git worktree; its prompt typed once it is ready.

use std::path::Path;
use std::time::{Duration, Instant};

use gpui_kit::{Context, Window};
use templum_canvas::{BlockId, Rect, Size, chrome};
use templum_mcp::{Reply, tools};
use templum_terminal::Agent;

use super::bus::Outcome;
use super::{AGENT_COLS, AGENT_ROWS, Handed, Prompt, Space, Workspace};
use crate::agents::AgentEntry;

/// How often a spawned agent's readiness for its prompt is looked at.
pub(super) const READY_LOOK: Duration = Duration::from_secs(1);
/// How long after the spawn a shell may take to start its agent (its startup files): a snapshot
/// that still finds none there lets it go, prompt and task (its folder's trust was declined, a
/// question of its shell took the command).
pub(super) const STARTS_WITHIN: Duration = Duration::from_secs(60);

impl Workspace {
    /// `spawn_agent` from the terminal `from`. With a worktree, git makes it on the background
    /// executor, and the call is answered once the agent opened there.
    pub(super) fn spawn_agent(
        &mut self,
        from: &str,
        spawn: tools::Spawn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let tools::Spawn {
            agent,
            prompt,
            folder,
            worktree,
        } = spawn;
        let Some(entry) = self.agents.iter().find(|a| a.id == agent).cloned() else {
            let ids: Vec<&str> = self.agents.iter().map(|a| a.id.as_str()).collect();
            return Outcome::Now(Reply::err(format!(
                "There's no agent {agent}. These are: {}.",
                ids.join(", ")
            )));
        };
        if !templum_terminal::installed(entry.program()) {
            return Outcome::Now(Reply::err(format!(
                "{} isn't installed on this computer ({} wasn't found).",
                entry.title,
                entry.program()
            )));
        }
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let here = space.blocks[&me]
            .folder()
            .or_else(|| space.terminal(me)?.seen.folder.clone());
        let folder = match (folder, here) {
            (Some(named), here) => {
                let named = crate::launcher::expand(&named);
                match here {
                    Some(here) if named.is_relative() => here.join(named),
                    _ => named,
                }
            }
            (None, Some(here)) => here,
            (None, None) => {
                return Outcome::Now(Reply::err(
                    "Templum doesn't know your folder yet: name one with folder.",
                ));
            }
        };
        if !folder.is_dir() {
            return Outcome::Now(Reply::err(format!("{} isn't a folder.", folder.display())));
        }
        let Some(name) = worktree else {
            return Outcome::Now(self.open_agent(from, &entry, &folder, prompt, window, cx));
        };
        let (program, from) = (self.config.git.program().to_string(), from.to_string());
        Outcome::Later(cx.spawn_in(window, async move |ws, cx| {
            let made = cx
                .background_executor()
                .spawn(async move {
                    match templum_git::Repo::discover(&program, &folder) {
                        Ok(Some(repo)) => repo
                            .add_worktree(&name)
                            .map_err(|e| format!("Couldn't make the worktree: {}", e.first_line())),
                        Ok(None) => Err(format!(
                            "{} isn't in a Git repository: a worktree needs one.",
                            folder.display()
                        )),
                        Err(e) => Err(format!("Couldn't run git: {}", e.first_line())),
                    }
                })
                .await;
            match made {
                Ok(path) => ws
                    .update_in(cx, |ws, window, cx| {
                        ws.open_agent(&from, &entry, &path, prompt, window, cx)
                    })
                    .unwrap_or_else(|_| Reply::err("Templum closed.")),
                Err(why) => Reply::err(why),
            }
        }))
    }

    /// Opens `entry` in `folder` beside the caller (terminal `from`), where it overlaps nothing,
    /// toward the blocks wired to the caller (`Space::room_near`), wired to it: one Ctrl+Z
    /// closes both. The keyboard, the selection and the camera stay. `prompt` is typed once the
    /// agent is ready, as a task the caller handed over.
    fn open_agent(
        &mut self,
        from: &str,
        entry: &AgentEntry,
        folder: &Path,
        prompt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let Some((space_id, me)) = self.caller(from) else {
            return Reply::err("Your block was closed before the agent could open.");
        };
        let opened = self
            .in_space(&space_id, |ws| {
                let size = chrome::block_size(AGENT_COLS, AGENT_ROWS, ws.cell.size);
                let rect = ws.space.room_near(me, size)?;
                let kept = (
                    ws.space.focused,
                    ws.space.selected.clone(),
                    ws.space.selected_wire,
                );
                // In grid mode a new block takes half of the focused tile: the caller's.
                ws.space.focused = Some(me);
                let id = ws.start_agent(Some(entry), &entry.command, folder, rect, window, cx);
                (ws.space.focused, ws.space.selected, ws.space.selected_wire) = kept;
                let id = id?;
                ws.space.add_wire(me, id);
                let terminal = ws.space.terminal(id)?.term.id().to_string();
                Some((ws.space.uids.get(&id)?.clone(), terminal))
            })
            .flatten();
        let Some((uid, terminal)) = opened else {
            return Reply::err(format!(
                "Couldn't open {} in {}.",
                entry.title,
                folder.display()
            ));
        };
        let mut said = format!(
            "Opened {} as {uid} in {}, connected to you.",
            entry.title,
            folder.display()
        );
        if let Some(text) = prompt {
            let now = Instant::now();
            if let Some(pane) = self.pane_mut(&terminal) {
                pane.prompt = Some(Prompt { text, screen: None });
                pane.handed = Some(Handed {
                    by: from.to_string(),
                    since: now,
                    until: now + Duration::from_secs(tools::TASK_WAIT),
                });
            }
            // Claude Code says when it started, after asking whether to trust the folder (a typed
            // Enter would answer that); the others are looked at.
            if entry.builtin != Some(Agent::CLAUDE) {
                self.watch_ready(terminal, cx);
            }
            said.push_str(" Its prompt is typed once it is ready.");
        }
        cx.notify();
        Reply::ok(said)
    }

    /// Looks, every READY_LOOK, whether the agent in `terminal` is ready for its prompt, until it
    /// is typed, its block is gone, or a snapshot dropped it (STARTS_WITHIN).
    fn watch_ready(&self, terminal: String, cx: &mut Context<Self>) {
        cx.spawn(async move |ws, cx| {
            loop {
                cx.background_executor().timer(READY_LOOK).await;
                let looking = ws
                    .update(cx, |ws, cx| ws.look_ready(&terminal, cx))
                    .unwrap_or(false);
                if !looking {
                    break;
                }
            }
        })
        .detach();
    }

    /// One look: once a snapshot saw the agent's process, it is still in front of its shell, and
    /// its screen is as it was a look ago, its prompt is typed. False once there is nothing more
    /// to look for.
    fn look_ready(&mut self, terminal: &str, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.pane_mut(terminal) else {
            return false;
        };
        let seen = pane.seen.pid.is_some() && !pane.shell_in_front();
        let screen = seen.then(|| pane.screen());
        let Some(prompt) = pane.prompt.as_mut() else {
            return false;
        };
        if screen.is_none() || prompt.screen != screen {
            prompt.screen = screen;
            return true;
        }
        let text = pane.prompt.take().map(|p| p.text).unwrap_or_default();
        self.send_task(terminal, &text, true, cx);
        false
    }
}

impl Space {
    /// A spot for a block of `size` beside block `id` that overlaps nothing, drawn toward the
    /// blocks wired to it (`room_beside`). None when `id` isn't on the canvas.
    fn room_near(&self, id: BlockId, size: Size) -> Option<Rect> {
        let home = |id| Some(self.home(self.canvas.get(id)?));
        let wired = self.wires.iter().filter(|w| w.touches(id));
        let anchors: Vec<Rect> = std::iter::once(home(id)?)
            .chain(wired.filter_map(|w| home(if w.a == id { w.b } else { w.a })))
            .collect();
        Some(self.room_beside(&anchors, size))
    }
}
