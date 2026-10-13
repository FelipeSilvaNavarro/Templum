//! Keeping the canvas: restoring a saved one, and writing snapshots of every workspace.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Window};
use templum_canvas::{Camera, Point, Rect, Size, chrome};

use crate::agents::AgentEntry;
use crate::board::{self, Boards};
use crate::editor::{self, Editor};
use crate::git_block::{self, Git};
use crate::kanban::{self, Kanban};
use crate::note::{self, Note};
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
                session::Item::Kanban(k) => {
                    let id = clamped_rect(k.rect.into(), kanban::MIN).map(|rect| {
                        self.add_block(
                            rect,
                            Body::Kanban(Kanban {
                                board: k.board,
                                picking: false,
                            }),
                        )
                    });
                    ids.push(id);
                    continue;
                }
                session::Item::Note(n) => {
                    let id = clamped_rect(n.rect.into(), note::MIN).map(|rect| {
                        let note = Note::new(&n.text, n.preview, window, cx);
                        self.add_block(rect, Body::Note(note))
                    });
                    ids.push(id);
                    continue;
                }
                session::Item::Git(g) => {
                    let id = clamped_rect(g.rect.into(), git_block::MIN).map(|rect| {
                        let git = Git::new(
                            PathBuf::from(&g.folder),
                            git_block::Tab::from_name(&g.tab),
                            window,
                            cx,
                        );
                        git.summary
                            .update(cx, |s, cx| s.set_value(g.summary.clone(), window, cx));
                        git.description
                            .update(cx, |s, cx| s.set_value(g.description.clone(), window, cx));
                        self.add_block(rect, Body::Git(Box::new(git)))
                    });
                    if let Some(id) = id {
                        self.look_up_git(id, window, cx);
                    }
                    ids.push(id);
                    continue;
                }
                session::Item::Editor(e) => {
                    let id = clamped_rect(e.rect.into(), editor::MIN).map(|rect| {
                        let folder = PathBuf::from(&e.folder);
                        let mut editor = Editor::new(folder.clone(), e.tree, cx);
                        for rel in &e.open {
                            let rel = Path::new(rel);
                            // Only files inside the folder, as they were saved: a path that
                            // climbs out or is absolute is not followed.
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
                    });
                    ids.push(id);
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
            // The shell reads it once it is ready.
            let agent = t
                .agent
                .as_ref()
                .and_then(|a| self.agents.iter().find(|known| known.id == *a));
            if let (Some(id), Some(agent), true) = (id, agent, in_its_folder) {
                let line = format!("{}\r", agent.resume_command());
                self.space
                    .terminal(id)
                    .expect("just spawned")
                    .term
                    .write(line.into_bytes());
            }
            ids.push(id);
        }
        self.space.focused = saved.focused.and_then(|i| ids.get(i).copied().flatten());
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

    /// The agents to look for in terminals, for a snapshot's background read.
    pub(super) fn known_agents(&self) -> Arc<Vec<templum_terminal::Detect>> {
        Arc::new(self.agents.iter().map(AgentEntry::detect).collect())
    }

    /// Every session::EVERY: reads the processes and writes each workspace's file in the
    /// background, when its snapshot differs from the last one written.
    pub(super) fn snapshot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.data_dir.clone() else {
            return;
        };
        if self.snapshotting {
            return;
        }
        self.snapshotting = true;
        self.generation += 1;
        let generation = self.generation;
        let known = self.known_agents();
        let jobs: Vec<_> = self
            .spaces()
            .filter(|s| !s.frozen)
            .map(|s| (s.id.clone(), s.layout(known.clone(), cx), s.saved.clone()))
            .collect();
        let boards = (!self.boards_frozen && self.boards_saved.as_ref() != Some(&self.boards))
            .then(|| self.boards.clone());
        cx.spawn_in(window, async move |ws, cx| {
            let (done, boards) = cx
                .background_executor()
                .spawn(async move {
                    let done = jobs
                        .into_iter()
                        .map(|(id, layout, last)| {
                            let saved = layout.snapshot();
                            let written = (last.as_ref() != Some(&saved))
                                .then(|| session::save(&dir, &id, &saved, generation));
                            (id, saved, written)
                        })
                        .collect::<Vec<_>>();
                    let boards = boards.map(|b| {
                        let written = board::save(&dir, &b, generation);
                        (b, written)
                    });
                    (done, boards)
                })
                .await;
            ws.update_in(cx, |ws, window, cx| {
                ws.snapshot_done(done, boards, window, cx)
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn snapshot_done(
        &mut self,
        done: Vec<(String, session::Saved, Option<std::io::Result<()>>)>,
        boards: Option<(Boards, std::io::Result<()>)>,
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
        for (id, saved, written) in done {
            match written {
                None => {}
                Some(Ok(())) => {
                    wrote = true;
                    // A workspace deleted meanwhile is gone: nothing to update.
                    if let Some(space) = self.space_by_id_mut(&id) {
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
        cx.notify();
    }

    /// When the window closes or Templum quits: the last snapshot of every workspace, read and
    /// written at once.
    pub(super) fn save_canvas_now(&mut self, cx: &App) {
        let Some(dir) = self.data_dir.clone() else {
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
        let known = self.known_agents();
        for space in std::iter::once(&mut self.space).chain(&mut self.others) {
            if space.frozen {
                continue;
            }
            let saved = space.layout(known.clone(), cx).snapshot();
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
