//! Keeping the canvas: restoring a saved one, and writing snapshots of every workspace.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Window};
use templum_canvas::{BlockId, Camera, Point, Rect, Size, chrome};

use crate::agents::AgentEntry;
use crate::board::{self, Boards};
use crate::editor::{self, Editor};
use crate::git_block::{self, Git};
use crate::kanban::{self, Kanban};
use crate::note::{self, Note};
use crate::notes;
use crate::session;

use super::{Body, MAX_COLS, MAX_ROWS, Workspace, show_notification};

impl Workspace {
    /// Puts a saved canvas back: the camera, then each block back to front, each in its folder.
    pub(super) fn restore(
        &mut self,
        saved: session::Saved,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space.pristine = false;
        // What was saved isn't a change to take back.
        self.quiet = true;
        let c = saved.camera;
        if c.x.is_finite() && c.y.is_finite() {
            self.space.canvas.camera = Camera::at(Point::new(c.x, c.y), c.zoom);
        }
        let mut ids = Vec::new();
        let mut gone = Vec::new();
        let mut failed = false;
        for item in &saved.items {
            let t = match item {
                session::Item::Terminal(t) => t,
                other => {
                    ids.push(self.add_saved(other, window, cx));
                    continue;
                }
            };
            let Some(rect) = self.restorable(t.rect.into()) else {
                ids.push(None);
                continue;
            };
            let folder = match t.folder.as_ref().map(PathBuf::from) {
                Some(f) if f.is_dir() => Some(f),
                Some(f) => {
                    gone.push(f.display().to_string());
                    None
                }
                None => None,
            };
            // An agent resumes the folder's latest conversation: anywhere else, it's another one.
            let in_its_folder = folder.is_some();
            let id = self.spawn_block(rect, folder, window, cx);
            failed |= id.is_none();
            if let (Some(id), true) = (id, in_its_folder) {
                self.resume_agent(id, t);
            }
            ids.push(id);
        }
        self.quiet = false;
        for (item, id) in saved.items.iter().zip(&ids) {
            if let Some(id) = id {
                self.adopt_uid(*id, item.uid());
            }
        }
        self.space.focused = saved.focused.and_then(|i| ids.get(i).copied().flatten());
        // Grid mode as it was left, over the canvas as it was.
        if let Some(tiles) = saved
            .grid
            .as_ref()
            .and_then(|g| g.tile(|i| ids.get(i).copied().flatten()))
        {
            self.tile_space(tiles, super::tiling::grid_area(self.viewport(window)));
            if !self.space.focused.is_some_and(|f| self.space.shown(f)) {
                self.space.focused = self.space.top();
            }
        }
        if failed {
            // A shell setting with a typo: fixing it must bring the canvas back, so it isn't
            // written over with the blocks that couldn't start.
            self.space.frozen = true;
            let message =
                "Some terminals couldn't start, so this session won't save their canvas: \
                           the saved one stays for the next start"
                    .to_string();
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
        }
        if !gone.is_empty() {
            let message = format!(
                "Gone, so their terminals start in the home folder: {}",
                gone.join(", ")
            );
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
        }
        self.space.ink = saved
            .ink
            .iter()
            .filter_map(session::InkItem::drawing)
            .collect();
        self.space.recent = saved.recent.iter().map(PathBuf::from).collect();
        self.space.saved = Some(saved);
    }

    /// Types what brings back the agent of the saved terminal `saved` (one Templum knows), its
    /// session when it was found, into the terminal `id`, just started. The shell reads it once
    /// it is ready.
    fn resume_agent(&mut self, id: BlockId, saved: &session::TerminalItem) {
        let Some(agent) = saved
            .agent
            .as_deref()
            .and_then(|a| self.agents.iter().find(|known| known.id == a))
        else {
            return;
        };
        let session = saved.session.as_deref();
        let kept = match (session, &self.data_dir) {
            (Some(id), Some(data)) => {
                templum_usage::session_kept(&agent.id, id, &templum_usage::Dirs::of(data))
            }
            _ => true,
        };
        let command = agent.resume_command_for(session, kept);
        let line = format!("{}\r", self.typed(Some(agent), &command));
        let agent = agent.id.clone();
        if let Some(pane) = self.space.terminal_mut(id) {
            pane.term.write(line.into_bytes());
            pane.seen.agent = Some(agent);
        }
    }

    /// A block from a saved item, on top and focused. A terminal starts a fresh shell in the
    /// item's folder (the home folder when it is gone), resuming its agent there. None when its
    /// rect has a number that isn't finite, or its shell couldn't start.
    pub(super) fn add_saved(
        &mut self,
        item: &session::Item,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BlockId> {
        match item {
            session::Item::Terminal(t) => {
                let rect = self.restorable(t.rect.into())?;
                let folder = t.folder.as_ref().map(PathBuf::from).filter(|f| f.is_dir());
                // An agent resumes the folder's latest conversation: anywhere else, it's another one.
                let in_its_folder = folder.is_some();
                let id = self.spawn_block(rect, folder, window, cx)?;
                if in_its_folder {
                    self.resume_agent(id, t);
                }
                Some(id)
            }
            session::Item::Kanban(k) => clamped_rect(k.rect.into(), kanban::MIN).map(|rect| {
                self.add_block(
                    rect,
                    Body::Kanban(Kanban {
                        board: k.board,
                        picking: false,
                    }),
                )
            }),
            session::Item::Note(n) => {
                let rect = clamped_rect(n.rect.into(), note::MIN)?;
                let mut ids = n.notes.clone();
                // A file from before notes.json: its text becomes a note there.
                if ids.is_empty() && !n.text.trim().is_empty() {
                    ids.push(self.notes.add(n.text.clone(), n.color, notes::now()));
                }
                let shown = n.notes.get(n.active).copied();
                // A note is open in one tab at most; a deleted one is gone.
                ids.retain(|id| self.note_open_at(*id).is_none());
                let tabs: Vec<_> = ids
                    .into_iter()
                    .filter_map(|id| self.note_tab(id, window, cx))
                    .collect();
                let active = tabs.iter().position(|t| Some(t.id) == shown).unwrap_or(0);
                Some(self.add_block(rect, Body::Note(Note { tabs, active })))
            }
            session::Item::Git(g) => {
                let id = clamped_rect(g.rect.into(), git_block::MIN).map(|rect| {
                    let mut git = Git::new(
                        PathBuf::from(&g.folder),
                        git_block::Tab::from_name(&g.tab),
                        window,
                        cx,
                    );
                    git.lists_folded = g.lists == "folded";
                    git.set_split(g.diff == "split");
                    git.summary
                        .update(cx, |s, cx| s.set_value(g.summary.clone(), window, cx));
                    git.description
                        .update(cx, |s, cx| s.set_value(g.description.clone(), window, cx));
                    self.add_block(rect, Body::Git(Box::new(git)))
                });
                if let Some(id) = id {
                    self.look_up_git(id, window, cx);
                }
                id
            }
            session::Item::Editor(e) => clamped_rect(e.rect.into(), editor::MIN).map(|rect| {
                let folder = PathBuf::from(&e.folder);
                let mut editor = Editor::new(folder.clone(), e.tree, cx);
                for rel in &e.open {
                    let rel = Path::new(rel);
                    // Only files inside the folder, as they were saved: a path that climbs out
                    // or is absolute is not followed.
                    let inside = rel
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)));
                    let path = folder.join(rel);
                    if inside && path.is_file() {
                        editor.open_tab(&path, window, cx).ok();
                    }
                }
                if let Some(a) = e.active.filter(|a| *a < editor.tabs.len()) {
                    editor.active = Some(a);
                }
                self.add_block(rect, Body::Editor(editor))
            }),
        }
    }

    /// The agents to look for in terminals, for a snapshot's background read.
    pub(super) fn known_agents(&self) -> Arc<Vec<templum_terminal::Detect>> {
        Arc::new(self.agents.iter().map(AgentEntry::detect).collect())
    }

    /// Every session::EVERY: reads the processes and writes each workspace's file in the
    /// background, when its snapshot differs from the last one written.
    pub(super) fn snapshot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.data_dir.clone().filter(|_| !self.first_run_welcome()) else {
            return;
        };
        if self.snapshotting {
            return;
        }
        self.snapshotting = true;
        self.generation += 1;
        let generation = self.generation;
        let known = self.known_agents();
        // A stopped workspace's file holds its canvas until it starts: it isn't written.
        let jobs: Vec<_> = self
            .spaces()
            .filter(|s| !s.stopped)
            .map(|s| {
                let layout = s.layout(known.clone(), cx);
                // A frozen workspace's file is never written, but its terminals are read.
                let last = (!s.frozen).then(|| s.saved.clone());
                (s.id.clone(), s.terminal_order(), layout, last)
            })
            .collect();
        let boards = (!self.boards_frozen && self.boards_saved.as_ref() != Some(&self.boards))
            .then(|| self.boards.clone());
        let notes = (!self.notes_frozen && self.notes_saved.as_ref() != Some(&self.notes))
            .then(|| self.notes.clone());
        cx.spawn_in(window, async move |ws, cx| {
            let (done, boards, notes) = cx
                .background_executor()
                .spawn(async move {
                    let notes = notes.map(|n| {
                        let written = notes::save(&dir, &n, generation);
                        (n, written)
                    });
                    // A canvas names its notes by id: it is written once they are.
                    let notes_kept = !matches!(notes, Some((_, Err(_))));
                    let done = jobs
                        .into_iter()
                        .map(|(id, terminals, layout, last)| {
                            let (saved, found) = layout.snapshot(Some(&dir));
                            let terminals = terminals.into_iter().zip(found).collect();
                            let written = last
                                .filter(|last| notes_kept && last.as_ref() != Some(&saved))
                                .map(|_| session::save(&dir, &id, &saved, generation));
                            (id, terminals, saved, written)
                        })
                        .collect::<Vec<_>>();
                    let boards = boards.map(|b| {
                        let written = board::save(&dir, &b, generation);
                        (b, written)
                    });
                    (done, boards, notes)
                })
                .await;
            ws.update_in(cx, |ws, window, cx| {
                ws.snapshot_done(done, boards, notes, window, cx)
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn snapshot_done(
        &mut self,
        done: Vec<SpaceSnapshot>,
        boards: Option<(Boards, std::io::Result<()>)>,
        notes: Option<(notes::Notes, std::io::Result<()>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snapshotting = false;
        let mut wrote = false;
        let mut failed = None;
        match boards {
            Some((boards, Ok(()))) => {
                wrote = true;
                self.boards_saved = Some(boards);
            }
            Some((_, Err(err))) => failed = Some(err),
            None => {}
        }
        match notes {
            Some((notes, Ok(()))) => {
                wrote = true;
                self.notes_saved = Some(notes);
            }
            Some((_, Err(err))) => failed = Some(err),
            None => {}
        }
        for (id, terminals, saved, written) in done {
            if let Some(space) = self.space_by_id_mut(&id) {
                space.see(terminals);
            }
            match written {
                None => {}
                Some(Ok(())) => {
                    wrote = true;
                    // A workspace deleted meanwhile is gone, and one stopped meanwhile keeps the
                    // canvas it was stopped with: nothing to update.
                    if let Some(space) = self.space_by_id_mut(&id).filter(|s| !s.stopped) {
                        space.saved = Some(saved);
                    }
                }
                Some(Err(err)) => failed = Some(err),
            }
        }
        if let Some(err) = failed {
            eprintln!("templum: couldn't save the canvas: {err}");
            if !std::mem::replace(&mut self.canvas_save_failed, true) {
                let message = format!("Couldn't save the canvas: {err}");
                show_notification(window, cx, Notification::error(message));
            }
        } else if wrote && std::mem::take(&mut self.canvas_save_failed) {
            show_notification(
                window,
                cx,
                Notification::info("The canvas is saved again".to_string()),
            );
        }
        self.read_usage(window, cx);
        cx.notify();
    }

    /// When the window closes or Templum quits: the last snapshot of every workspace, read and
    /// written at once.
    pub(super) fn save_canvas_now(&mut self, cx: &App) {
        // A first run's welcome still open: the next start is a first run too.
        let Some(dir) = self.data_dir.clone().filter(|_| !self.first_run_welcome()) else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        if !self.boards_frozen && self.boards_saved.as_ref() != Some(&self.boards) {
            match board::save(&dir, &self.boards, generation) {
                Ok(()) => self.boards_saved = Some(self.boards.clone()),
                Err(err) => eprintln!("templum: couldn't save the boards: {err}"),
            }
        }
        if !self.notes_frozen && self.notes_saved.as_ref() != Some(&self.notes) {
            match notes::save(&dir, &self.notes, generation) {
                Ok(()) => self.notes_saved = Some(self.notes.clone()),
                Err(err) => {
                    // A canvas names its notes by id: it is written once they are.
                    eprintln!("templum: couldn't save the notes: {err}");
                    return;
                }
            }
        }
        let known = self.known_agents();
        for space in std::iter::once(&mut self.space).chain(&mut self.others) {
            if space.frozen || space.stopped {
                continue;
            }
            let (saved, _) = space.layout(known.clone(), cx).snapshot(Some(&dir));
            if space.saved.as_ref() == Some(&saved) {
                continue;
            }
            match session::save(&dir, &space.id, &saved, generation) {
                Ok(()) => space.saved = Some(saved),
                Err(err) => eprintln!("templum: couldn't save the canvas: {err}"),
            }
        }
    }

    /// A saved rect made safe: None with a number that isn't finite; its size kept between the
    /// smallest block and MAX_COLS x MAX_ROWS cells.
    pub(super) fn restorable(&self, r: Rect) -> Option<Rect> {
        if ![r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) {
            return None;
        }
        let min = chrome::min_block_size(self.cell.size);
        let max = chrome::block_size(MAX_COLS, MAX_ROWS, self.cell.size);
        Some(Rect::new(
            r.x,
            r.y,
            r.w.clamp(min.w, max.w),
            r.h.clamp(min.h, max.h),
        ))
    }
}

/// A saved rect made safe: None with a number that isn't finite; at least `min`.
fn clamped_rect(r: Rect, min: Size) -> Option<Rect> {
    [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()).then(|| {
        Rect::new(
            r.x,
            r.y,
            r.w.clamp(min.w, 20_000.0),
            r.h.clamp(min.h, 20_000.0),
        )
    })
}

/// A workspace's snapshot, read in the background: its id, what was found in each of its
/// terminals, the file's content, and the write's outcome (None: unchanged).
pub(super) type SpaceSnapshot = (
    String,
    Vec<(BlockId, templum_terminal::Inspection)>,
    session::Saved,
    Option<std::io::Result<()>>,
);
