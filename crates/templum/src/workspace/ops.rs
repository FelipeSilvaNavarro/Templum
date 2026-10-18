//! The workspace operations both MCP servers call, each taking its target, never a caller
//! (docs/specs/agent-tools.md#shared-operations).

use std::path::{Path, PathBuf};

use gpui_kit::{Context, Task, Window};
use templum_canvas::{BlockId, chrome};
use templum_mcp::Reply;
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
        let program = pane
            .term
            .program(&shells, pane.seen.pid.zip(pane.seen.started));
        let front = program.as_ref().map(|(pid, _)| *pid);
        let found = templum_terminal::strays(pane.term.id(), &shells, front, pane.agent_since());
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

    /// The note the note block `block` of the workspace `space` shows: its whole text becomes
    /// `text`, or with `old`, its one occurrence of `old` does. One step of the note's Ctrl+Z; the
    /// person's selection stays, within the new text.
    pub(super) fn note_replace(
        &mut self,
        space: &str,
        block: BlockId,
        text: &str,
        old: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let editor = self
            .space_named(space)
            .and_then(|s| {
                s.blocks
                    .get(&block)?
                    .note()?
                    .tab()
                    .map(|t| t.editor.clone())
            })
            .ok_or("That block shows no note.")?;
        let value = editor.read(cx).value().to_string();
        let range = match old {
            None if text.trim().is_empty() => {
                return Err("That would empty the note: the person does that.".into());
            }
            None => 0..value.len(),
            Some(old) => match value.matches(old).count() {
                0 => return Err(format!("\"{old}\" isn't in the note: read_note shows it.")),
                1 => {
                    let at = value.find(old).unwrap_or_default();
                    at..at + old.len()
                }
                n => {
                    return Err(format!(
                        "\"{old}\" is in the note {n} times: give more of the text around it."
                    ));
                }
            },
        };
        editor.update(cx, |e, cx| {
            let kept = e.selected_range();
            e.set_selected_range(range, cx);
            e.replace(text.to_string(), window, cx);
            // Clipped to a character's boundary there.
            let end = e.value().len();
            e.set_selected_range(kept.start.min(end)..kept.end.min(end), cx);
        });
        Ok(())
    }

    /// The text of item `item` of the checklist `list` becomes `text`, as one step of its Ctrl+Z.
    /// False when the checklist has no such item.
    pub(super) fn item_edit(&mut self, list: u64, item: u64, text: &str) -> bool {
        if self
            .checklists
            .get(list)
            .and_then(|l| l.item(item))
            .is_none()
        {
            return false;
        }
        self.edit_checklist(list, |c| c.set_text(list, item, text, notes::now()));
        self.agent_changed(list);
        true
    }

    /// Keeps `schedule`, to be delivered at its time (`schedules.rs`); its id, counting from 1.
    pub(super) fn schedule_add(
        &mut self,
        mut schedule: super::schedules::Schedule,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.next_schedule += 1;
        schedule.id = self.next_schedule;
        self.schedules.push(schedule);
        self.look_at_schedules(cx);
        cx.notify();
        self.next_schedule
    }

    /// Drops schedule `id`; false when there is none.
    pub(super) fn schedule_cancel(&mut self, id: u64) -> bool {
        let before = self.schedules.len();
        self.schedules.retain(|s| s.id != id);
        self.schedules.len() < before
    }

    /// Keeps `watch`, its match, its timeout or its block's end typed back to its asker
    /// (`schedules.rs`); its id, counting from 1 (`w1`).
    pub(super) fn watch_keep(
        &mut self,
        mut watch: super::schedules::Watch,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.next_watch += 1;
        watch.id = self.next_watch;
        self.watches.push(watch);
        self.look_at_watches(cx);
        self.next_watch
    }

    /// The worktree `name` that `spawn_agent` made in the repository of `folder`, against
    /// `folder`'s working tree (where it goes back), and that repository; why not.
    // ponytail: read on the UI thread (a few git commands); on the background executor if a big
    // repository makes it slow.
    pub(super) fn worktree_state(
        &self,
        folder: &Path,
        name: &str,
    ) -> Result<(templum_git::Repo, templum_git::WorktreeState), String> {
        let program = self.config.git.program();
        let repo = match templum_git::Repo::discover(program, folder) {
            Ok(Some(repo)) => repo,
            Ok(None) => return Err(format!("{} isn't in a Git repository.", folder.display())),
            Err(e) => return Err(format!("Couldn't run git: {}", e.first_line())),
        };
        match repo.worktree_state(name) {
            Ok(Some(state)) => Ok((repo, state)),
            Ok(None) => Err(format!(
                "There's no worktree {name} in this repository: git worktree list shows them."
            )),
            Err(e) => Err(format!("Couldn't read the worktree: {}", e.first_line())),
        }
    }

    /// Removes the worktree `name` of `repo` and its branch on the background executor; with
    /// `discard`, its changes and commits not merged are lost.
    pub(super) fn worktree_remove(
        &self,
        repo: templum_git::Repo,
        name: String,
        discard: bool,
        cx: &mut Context<Self>,
    ) -> Task<Reply> {
        cx.background_executor().spawn(async move {
            match repo.remove_worktree(&name, discard) {
                Ok(()) => Reply::ok(format!("Removed the worktree {name} and its branch.")),
                Err(e) => Reply::err(format!("Couldn't remove {name}: {}", e.first_line())),
            }
        })
    }

    /// Merges the worktree `name`'s `unmerged` commits into `into`, the branch of `repo`'s working
    /// tree, on the background executor; a conflict is aborted, naming its files.
    pub(super) fn worktree_merge(
        &self,
        repo: templum_git::Repo,
        name: String,
        into: String,
        unmerged: u32,
        cx: &mut Context<Self>,
    ) -> Task<Reply> {
        cx.background_executor().spawn(async move {
            let commits = match unmerged {
                1 => "1 commit".to_string(),
                n => format!("{n} commits"),
            };
            match repo.merge_from(&name) {
                Ok(Ok(())) => Reply::ok(format!(
                    "Merged {name} into {into} ({commits}). remove_worktree removes it."
                )),
                Ok(Err(files)) => Reply::err(format!(
                    "The merge would conflict in {}: merge {into} into {name} and resolve them \
                     there, then merge again.",
                    files.join(", ")
                )),
                Err(e) => Reply::err(format!("Couldn't merge {name}: {}", e.first_line())),
            }
        })
    }
}
