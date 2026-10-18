//! The workspace operations both MCP servers call, each taking its target, never a caller
//! (docs/specs/agent-tools.md#shared-operations).

use std::path::PathBuf;

use gpui_kit::{Context, Window};
use templum_canvas::{BlockId, chrome};
use templum_terminal::Pattern;

use super::{Body, DEFAULT_COLS, DEFAULT_ROWS, Workspace};
use crate::checklist::{self, Checklist};
use crate::note::{self, Note};
use crate::notes;

/// A block [`Workspace::add_block_in`] opens.
pub(crate) enum NewBlock {
    /// A new note with `text`.
    Note { text: String },
    /// A new checklist named `name` ("Checklist" when blank) with `items`, each checked or not.
    Checklist {
        name: String,
        items: Vec<(String, bool)>,
    },
    /// A terminal in `folder`, `command` typed and run once its shell reads it.
    Terminal {
        folder: PathBuf,
        command: Option<String>,
    },
}

/// Where [`Workspace::add_block_in`] opens a block.
pub(crate) enum Place {
    /// Beside the block, where it covers no block, toward the blocks wired to it.
    Near(BlockId),
}

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

    /// Looks for `pattern` in each line the terminal `terminal` (its `TEMPLUM_TERMINAL`) prints
    /// from now on; the watch's id, None when no block of any workspace shows that terminal.
    pub(super) fn watch_add(&self, terminal: &str, pattern: Pattern) -> Option<u64> {
        Some(self.pane(terminal)?.term.watch(pattern))
    }

    /// Opens `new` in the workspace `space` at `place`, as one step of Ctrl+Z: the keyboard, the
    /// selection and the camera stay where they were; in grid mode it takes half of the tile of
    /// the block it is near. A terminal's `opened_by` is `opener`, which may stop and close it.
    pub(crate) fn add_block_in(
        &mut self,
        space: &str,
        new: NewBlock,
        place: Place,
        opener: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<BlockId, String> {
        let Place::Near(near) = place;
        let added = self.in_space(space, |ws| {
            let size = match &new {
                NewBlock::Note { .. } => note::SIZE,
                NewBlock::Checklist { .. } => checklist::SIZE,
                NewBlock::Terminal { .. } => {
                    chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, ws.cell.size)
                }
            };
            let rect = ws.space.room_near(near, size)?;
            let kept = (
                ws.space.focused,
                ws.space.selected.clone(),
                ws.space.selected_wire,
            );
            // In grid mode a new block takes half of the focused tile.
            ws.space.focused = Some(near);
            ws.space.pristine = false;
            let now = notes::now();
            // Built here, not by the menus' constructors, which give the new block the keyboard.
            let id = match new {
                NewBlock::Note { text } => {
                    let note = ws.notes.add(text, None, now);
                    let tabs = ws.note_tab(note, window, cx).into_iter().collect();
                    Some(ws.add_block(rect, Body::Note(Note { tabs, active: 0 })))
                }
                NewBlock::Checklist { name, items } => {
                    let list = ws.checklists.add(now);
                    if !name.trim().is_empty() {
                        ws.checklists.rename(list, name.trim(), now);
                    }
                    for (text, done) in &items {
                        ws.checklists.add_item(list, None, text, *done, now);
                    }
                    let shown = Checklist {
                        list: Some(list),
                        editing: None,
                    };
                    Some(ws.add_block(rect, Body::Checklist(shown)))
                }
                NewBlock::Terminal { folder, command } => {
                    let id = ws.spawn_block(rect, Some(folder), window, cx);
                    if let Some(pane) = id.and_then(|id| ws.space.terminal_mut(id)) {
                        pane.opened_by = opener;
                        // The shell reads it once it is ready.
                        if let Some(command) = command {
                            pane.term.write(format!("{command}\r").into_bytes());
                        }
                    }
                    id
                }
            };
            (ws.space.focused, ws.space.selected, ws.space.selected_wire) = kept;
            id
        });
        cx.notify();
        added
            .flatten()
            .ok_or_else(|| "Couldn't open the block.".to_string())
    }
}
