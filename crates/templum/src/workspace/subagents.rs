//! Claude Code's subagents in blocks of their own (docs/specs/claude-subagents.md): one opens
//! beside its parent as it starts, wired to it, `templum follow` printing its transcript; it
//! closes a minute after it finished, unless the person reads it. Never saved: it belongs to a
//! live session.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{Context, Window};
use templum_canvas::chrome;
use templum_terminal::ShellOption;

use super::ops::Place;
use super::{Pane, Workspace};

/// A subagent block's size, in cells.
const COLS: u16 = 90;
const ROWS: u16 = 22;
/// How long a finished subagent's block stays once the person no longer reads it.
pub(super) const CLOSES_AFTER: Duration = Duration::from_secs(60);
/// How often a finished subagent's block is looked at.
const LOOK: Duration = Duration::from_secs(1);

/// The subagent a block shows: read-only, its parent's.
pub(crate) struct Follows {
    /// Its parent's terminal (`TEMPLUM_TERMINAL`).
    pub parent: String,
    /// The subagent's id.
    pub id: String,
    /// When it finished, or when the person last read it since: CLOSES_AFTER later it closes.
    pub done: Option<Instant>,
}

/// The transcript Claude Code writes for its subagent `id`, beside its session's `transcript`
/// (documented); none for an id that isn't one (its file name is made of it).
pub(super) fn transcript_of(transcript: &str, id: &str) -> Option<PathBuf> {
    let id_ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    id_ok.then(|| {
        Path::new(transcript)
            .with_extension("")
            .join("subagents")
            .join(format!("agent-{id}.jsonl"))
    })
}

/// What a subagent's block runs: this Templum (`exe`) following its transcript `file`, its type
/// `kind` for its title. Run as the block's program, never typed into a shell.
pub(super) fn follow(exe: &Path, file: &Path, kind: &str) -> ShellOption {
    ShellOption {
        label: "templum follow".to_string(),
        program: exe.to_string_lossy().into_owned(),
        args: vec![
            "follow".to_string(),
            file.to_string_lossy().into_owned(),
            kind.to_string(),
        ],
    }
}

impl Pane {
    /// It shows a subagent of the agent in `parent` (its terminal).
    pub fn follows_of(&self, parent: &str) -> bool {
        self.follows.as_ref().is_some_and(|f| f.parent == parent)
    }
}

impl Workspace {
    /// The block showing the subagent `id` of the agent in the terminal `parent`.
    fn follower_mut(&mut self, parent: &str, id: &str) -> Option<&mut Pane> {
        std::iter::once(&mut self.space)
            .chain(&mut self.others)
            .flat_map(|s| s.blocks.values_mut())
            .filter_map(super::Body::terminal_mut)
            .find(|p| {
                p.follows
                    .as_ref()
                    .is_some_and(|f| f.parent == parent && f.id == id)
            })
    }

    /// The subagent `id` of type `kind` started in the agent of the terminal `parent`, whose
    /// session's transcript is `transcript`: a block beside that agent's, wired to it, follows its
    /// transcript. The keyboard, the selection and the camera stay, and Ctrl+Z doesn't take it
    /// away. One already open for it (a subagent resumed) works again.
    pub(super) fn open_subagent(
        &mut self,
        parent: &str,
        (id, kind): (&str, &str),
        transcript: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.follower_mut(parent, id) {
            if let Some(follows) = pane.follows.as_mut() {
                follows.done = None;
            }
            return;
        }
        let Some(file) = transcript.and_then(|t| transcript_of(t, id)) else {
            return;
        };
        let Some((space, me)) = self.caller(parent) else {
            return;
        };
        // The tests' terminals run their own program.
        let shell = match (&self.command, super::usage::program()) {
            (None, Some(exe)) => Some(follow(&exe, &file, kind)),
            (None, None) => return,
            (Some(_), _) => None,
        };
        let folder = self
            .space_named(&space)
            .and_then(|s| s.terminal(me))
            .and_then(|p| p.seen.folder.clone());
        let (parent, id) = (parent.to_string(), id.to_string());
        self.in_space(&space, |ws| {
            let size = chrome::block_size(COLS, ROWS, ws.cell.size);
            let center = ws.space.camera().to_world(ws.center(window));
            let rect = ws.space.spot(&Place::Near(me), size, center)?;
            let kept = (
                ws.space.focused,
                ws.space.selected.clone(),
                ws.space.selected_wire,
            );
            // In grid mode it takes half of its parent's tile.
            ws.space.focused = Some(me);
            ws.quiet = true;
            let block = ws.spawn_block_with(rect, folder, Vec::new(), shell, window, cx);
            ws.quiet = false;
            (ws.space.focused, ws.space.selected, ws.space.selected_wire) = kept;
            let block = block?;
            ws.space.add_wire(me, block);
            let opener = ws.space.uids.get(&me).cloned();
            let pane = ws.space.terminal_mut(block)?;
            // Not a shell to open again elsewhere.
            pane.shell = None;
            pane.opened_by = opener;
            pane.follows = Some(Follows {
                parent,
                id,
                done: None,
            });
            Some(())
        });
        cx.notify();
    }

    /// The subagent `id` of the agent in the terminal `parent` finished, or every one of its
    /// subagents when None (its session ended): their blocks say so, and close CLOSES_AFTER later
    /// unless the person reads them.
    pub(super) fn subagents_done(
        &mut self,
        parent: &str,
        id: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        let finished: Vec<String> = std::iter::once(&mut self.space)
            .chain(&mut self.others)
            .flat_map(|s| s.blocks.values_mut())
            .filter_map(super::Body::terminal_mut)
            .filter_map(|pane| {
                let follows = pane.follows.as_mut()?;
                let ours = follows.parent == parent && id.is_none_or(|id| follows.id == id);
                if !ours || follows.done.is_some() {
                    return None;
                }
                follows.done = Some(now);
                Some(pane.term.id().to_string())
            })
            .collect();
        for terminal in finished {
            cx.spawn_in(window, async move |ws, cx| {
                loop {
                    cx.background_executor().timer(LOOK).await;
                    let looking = ws
                        .update_in(cx, |ws, window, cx| ws.close_unread(&terminal, window, cx))
                        .unwrap_or(false);
                    if !looking {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.notify();
    }

    /// A look at the finished subagent block of the terminal `terminal`: read now (focused,
    /// pointed at, selected, or scrolled back from its end), its minute starts over; unread for
    /// CLOSES_AFTER, it closes. False once there is nothing more to look for: it closed, it is
    /// gone, or it works again.
    fn close_unread(
        &mut self,
        terminal: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((space_id, block)) = self.caller(terminal) else {
            return false;
        };
        let now = cx.background_executor().now();
        let reading = self.space_named(&space_id).is_some_and(|space| {
            space.focused == Some(block)
                || space.hovered == Some(block)
                || space.selected.contains(&block)
                || space
                    .terminal(block)
                    .is_some_and(|p| p.term.scrolled_back())
        });
        let Some(follows) = self.pane_mut(terminal).and_then(|p| p.follows.as_mut()) else {
            return false;
        };
        let Some(done) = follows.done else {
            return false;
        };
        if reading {
            follows.done = Some(now);
            return true;
        }
        if now < done + CLOSES_AFTER {
            return true;
        }
        self.in_space(&space_id, |ws| ws.remove_block(block, window, cx));
        false
    }

    /// The blocks of the subagents of the agent in the terminal `parent`, closed with it.
    pub(super) fn close_subagents_of(
        &mut self,
        parent: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theirs: Vec<(String, templum_canvas::BlockId)> = self
            .spaces()
            .flat_map(|s| {
                s.terminals()
                    .filter(|(_, p)| p.follows_of(parent))
                    .map(|(id, _)| (s.id.clone(), id))
            })
            .collect();
        for (space, id) in theirs {
            self.in_space(&space, |ws| ws.remove_block(id, window, cx));
        }
    }
}
