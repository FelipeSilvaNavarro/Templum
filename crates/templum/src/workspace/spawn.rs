//! `spawn_agent` (docs/specs/connections.md, Spawning): a new agent block beside the caller, wired
//! to it, in its folder, another, or a new Git worktree; its prompt typed once it is ready.

use std::path::Path;
use std::time::Duration;

use gpui_kit::{Context, Window};
use templum_canvas::{BlockId, Rect, Size, chrome};
use templum_mcp::{Reply, tools};
use templum_terminal::Agent;

use super::bus::{Outcome, target};
use super::delegate::TASK_LEAD;
use super::history::Then;
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
        let Some(entry) = self.agents.iter().find(|a| a.id == spawn.agent).cloned() else {
            let ids: Vec<&str> = self.agents.iter().map(|a| a.id.as_str()).collect();
            return Outcome::Now(Reply::err(format!(
                "There's no agent {}. These are: {}.",
                spawn.agent,
                ids.join(", ")
            )));
        };
        // Typed into a shell: anything it can't type safely opens nothing, a worktree included.
        if let Err(why) = entry.launch_args(spawn.model.as_deref(), spawn.effort.as_deref()) {
            return Outcome::Now(Reply::err(why));
        }
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
        let folder = match (spawn.folder.as_deref(), here) {
            (Some(named), here) => {
                let named = crate::launcher::expand(named);
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
        let Some(name) = spawn.worktree.clone() else {
            return Outcome::Now(self.open_agent(from, &entry, &folder, &spawn, window, cx));
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
                        ws.open_agent(&from, &entry, &path, &spawn, window, cx)
                    })
                    .unwrap_or_else(|_| Reply::err("Templum closed.")),
                Err(why) => Reply::err(why),
            }
        }))
    }

    /// Opens `entry` in `folder` beside the caller (terminal `from`), where it overlaps nothing,
    /// toward the blocks wired to the caller (`Space::room_near`), wired to it: one Ctrl+Z
    /// closes both. The keyboard, the selection and the camera stay. Its agent starts on the
    /// spawn's model and effort (checked by `spawn_agent`), and its prompt is typed once the
    /// agent is ready, as a task the caller handed over.
    fn open_agent(
        &mut self,
        from: &str,
        entry: &AgentEntry,
        folder: &Path,
        spawn: &tools::Spawn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let Some((space_id, me)) = self.caller(from) else {
            return Reply::err("Your block was closed before the agent could open.");
        };
        let (model, effort) = (spawn.model.as_deref(), spawn.effort.as_deref());
        let args = entry.launch_args(model, effort).unwrap_or_default();
        // A task the caller hands over, under its `[from …]` line, as `delegate` types one.
        let by = self
            .space_named(&space_id)
            .map(|space| self.signature(space, me, cx))
            .unwrap_or_default();
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
                let id =
                    ws.start_agent(Some(entry), &entry.command, &args, folder, rect, window, cx);
                (ws.space.focused, ws.space.selected, ws.space.selected_wire) = kept;
                let id = id?;
                ws.space.add_wire(me, id);
                // The caller may close it (`close_block`); a resume types its model again.
                let opener = ws.space.uids.get(&me).cloned();
                let pane = ws.space.terminal_mut(id)?;
                pane.opened_by = opener;
                (pane.model, pane.effort) = (spawn.model.clone(), spawn.effort.clone());
                pane.flags_for = Some(entry.id.clone());
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
        let on = match (model, effort) {
            (Some(m), Some(e)) => format!(", on {m} ({e})"),
            (Some(m), None) => format!(", on {m}"),
            (None, Some(e)) => format!(", with {e} effort"),
            (None, None) => String::new(),
        };
        let mut said = format!(
            "Opened {} as {uid} in {}, connected to you{on}.",
            entry.title,
            folder.display()
        );
        if let Some(prompt) = spawn.prompt.as_ref() {
            self.next_task += 1;
            let id = self.next_task;
            let text = format!("[from {by}, task t{id}] {TASK_LEAD}\n{prompt}");
            let handed = Handed {
                id,
                // Its session starts with its task: all it spends is the task's.
                usage: Some(templum_usage::Session {
                    cost_usd: Some(0.0),
                    tokens: Some(0),
                    ..Default::default()
                }),
                ..Handed::new(from, Duration::from_secs(tools::TASK_WAIT))
            };
            let until = handed.until;
            if let Some(pane) = self.pane_mut(&terminal) {
                pane.prompt = Some(Prompt {
                    text,
                    screen: None,
                    gated: entry.builtin == Some(Agent::CLAUDE)
                        || entry.builtin == Some(Agent::CODEX),
                });
                pane.handed = Some(handed);
            }
            self.time_out(terminal.clone(), id, until, tools::TASK_WAIT, cx);
            // Every agent is looked at, hooks or not: Codex's SessionStart comes only with its
            // first prompt, and Claude Code's while it doesn't take pastes yet, after asking
            // whether to trust the folder (a typed Enter would answer that).
            self.watch_ready(terminal, cx);
            said.push_str(&format!(
                " Its prompt is typed once it is ready, as task t{id}."
            ));
        }
        cx.notify();
        Reply::ok(said)
    }

    /// `close_block` from the terminal `from`: the block `block`, which it opened with
    /// `spawn_agent` and is still wired to (unwiring it takes the consent back), closed without
    /// asking the person, as one step of Ctrl+Z, also in a workspace not shown. The task the
    /// caller handed it ends untold: the caller knows (docs/specs/connections.md, Delegating 12).
    pub(super) fn close_block(
        &mut self,
        from: &str,
        block: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let id = match target(space, me, block) {
            Ok(id) => id,
            Err(refused) => return refused,
        };
        // Any other kind of block was opened by no agent.
        let opener = space.terminal(id).and_then(|p| p.opened_by.as_ref());
        if opener.is_none() || opener != space.uids.get(&me) {
            return Reply::err(format!(
                "Only the person closes {block}: you didn't open it."
            ));
        }
        let terminal = space.terminal(id).map(|p| p.term.id().to_string());
        if let Some(pane) = terminal.as_deref().and_then(|t| self.pane_mut(t))
            && pane.handed.as_ref().is_some_and(|h| h.by == from)
        {
            pane.handed = None;
        }
        self.in_space(&space_id, |ws| {
            ws.close_now(&[id], Then::New, window, cx);
            // Its shell and agent end now: a ghost waits for a frame of its workspace, which one
            // not shown, or a window not drawn, doesn't have.
            ws.space.ghosts.retain(|g| g.block.id != id);
        });
        cx.notify();
        Reply::ok(format!(
            "Closed {block}. The person can bring it back with Ctrl+Z."
        ))
    }

    /// Looks, every READY_LOOK, whether the agent in `terminal` is ready for its prompt, until it
    /// is typed, its block is gone, a snapshot dropped it, or a minute (STARTS_WITHIN) passed.
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

    /// One look: once a snapshot saw the agent's process, it is still in front of its shell, its
    /// screen is as it was a look ago, and (Claude Code, Codex; not on Windows) it takes typing,
    /// its prompt is typed. A minute (STARTS_WITHIN) after the spawn it is dropped instead, its
    /// task ending with the screen. False once there is nothing more to look for.
    fn look_ready(&mut self, terminal: &str, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.pane_mut(terminal) else {
            return false;
        };
        if pane.prompt.is_none() {
            return false;
        }
        let late = pane.handed.as_ref();
        if late.is_some_and(|h| h.since.elapsed() >= STARTS_WITHIN) {
            pane.prompt = None;
            let lines = pane.tail();
            let tail = tools::tail(&lines, tools::MAX_TEXT);
            let uid = self.uid_of(terminal);
            let why = format!(
                "{uid} never got its prompt: a minute after it opened, it still asks its person \
                 something (trusting its folder, Codex's hooks), or its input wasn't ready. Look \
                 at its block. The last lines of its screen:\n{tail}"
            );
            self.end_turn(terminal, Reply::err(why), cx);
            return false;
        }
        let seen = pane.seen.pid.is_some() && !pane.shell_in_front();
        let screen = seen.then(|| pane.screen());
        // Windows' console doesn't pass the cursor's mode on: its still screen alone tells.
        let typing = cfg!(windows) || pane.term.shared().modes().typing;
        let Some(prompt) = pane.prompt.as_mut() else {
            return false;
        };
        if screen.is_none() || prompt.screen != screen || (prompt.gated && !typing) {
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
