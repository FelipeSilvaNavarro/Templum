//! The workspaces: loading, switching, adding, renaming and deleting them, and their index.

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, ImageSource, Window};
use templum_canvas::{Rect, chrome};

use crate::alert;
use crate::board;
use crate::notes;
use crate::prompt;
use crate::session;
use crate::space_dialog::{self, Look};

use super::{DEFAULT_COLS, DEFAULT_ROWS, Space, Workspace, show_notification};

/// How long a workspace's processes have, once hung up, before what is left is killed.
const KILL_AFTER: Duration = Duration::from_secs(2);

/// Every process of a workspace's terminals, read now, while they still name their parents.
fn tree_of(space: &Space) -> Vec<templum_terminal::Running> {
    let shells: Vec<u32> = space
        .terminals()
        .filter_map(|(_, pane)| pane.term.processes().shell)
        .collect();
    if shells.is_empty() {
        return Vec::new();
    }
    templum_terminal::tree(&shells)
}

impl Workspace {
    /// Opens every workspace of the index (without one: `default`, as the version without
    /// workspaces saved it), each canvas as it was saved, and shows the active one. A first run
    /// gets one terminal, centered.
    pub(super) fn load_spaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use session::Loaded;
        match self.data_dir.as_deref().map(board::load) {
            Some(Loaded::Saved(boards)) => {
                self.boards_saved = Some(boards.clone());
                self.boards = boards;
            }
            Some(Loaded::Moved { to, why }) => {
                let message = format!(
                    "Couldn't read the boards ({why}); they were kept as {}",
                    to.display()
                );
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::warning(message));
            }
            Some(Loaded::Unreadable(why)) => {
                let message =
                    format!("Couldn't read the boards ({why}); they won't be saved this session");
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::error(message));
                self.boards_frozen = true;
            }
            Some(Loaded::Nothing) | None => {}
        }
        match self.data_dir.as_deref().map(notes::load) {
            Some(Loaded::Saved(notes)) => {
                self.notes_saved = Some(notes.clone());
                self.notes = notes;
            }
            Some(Loaded::Moved { to, why }) => {
                let message = format!(
                    "Couldn't read the notes ({why}); they were kept as {}",
                    to.display()
                );
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::warning(message));
            }
            Some(Loaded::Unreadable(why)) => {
                let message = format!(
                    "Couldn't read the notes ({why}); neither they nor the canvas will be saved this session"
                );
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::error(message));
                self.notes_frozen = true;
            }
            Some(Loaded::Nothing) | None => {}
        }
        let (index, no_index) = match self.data_dir.as_deref().map(session::load_index) {
            Some(Loaded::Saved(index)) => (index, false),
            Some(Loaded::Moved { to, why }) => {
                let message = format!(
                    "Couldn't read the workspace list ({why}); it was kept as {}",
                    to.display()
                );
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::warning(message));
                (session::Index::first_run(), true)
            }
            Some(Loaded::Unreadable(why)) => {
                let message = format!(
                    "Couldn't read the workspace list ({why}); it won't be saved this session"
                );
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::error(message));
                self.index_frozen = true;
                (session::Index::first_run(), true)
            }
            Some(Loaded::Nothing) | None => (session::Index::first_run(), true),
        };
        let mut spaces = Vec::new();
        let mut first_saved = false;
        for (i, entry) in index.workspaces.iter().enumerate() {
            self.space = Space {
                id: entry.id.clone(),
                name: entry.name.clone(),
                look: Look {
                    icon: entry.icon.as_deref().and_then(space_dialog::icon),
                    color: entry.color.clone(),
                    image: self
                        .data_dir
                        .as_deref()
                        .zip(entry.image.as_ref())
                        .map(|(d, name)| session::icons_dir(d).join(name)),
                },
                folder: entry.folder.clone(),
                ..Space::default()
            };
            match self
                .data_dir
                .as_deref()
                .map(|d| session::load(d, &entry.id))
            {
                // A stopped one keeps its canvas to start when it is opened.
                Some(Loaded::Saved(saved)) if entry.stopped => self.space.saved = Some(saved),
                Some(Loaded::Saved(saved)) => {
                    first_saved |= i == 0;
                    self.restore(saved, window, cx);
                }
                Some(Loaded::Moved { to, why }) => {
                    let message = format!(
                        "Couldn't read the saved canvas ({why}); it was kept as {}",
                        to.display()
                    );
                    eprintln!("templum: {message}");
                    show_notification(window, cx, Notification::warning(message));
                }
                Some(Loaded::Unreadable(why)) => {
                    let message = format!(
                        "Couldn't read the saved canvas ({why}); it won't be saved this session"
                    );
                    eprintln!("templum: {message}");
                    show_notification(window, cx, Notification::error(message));
                    self.space.frozen = true;
                }
                Some(Loaded::Nothing) | None => {}
            }
            self.space.stopped = entry.stopped;
            // Its file may hold notes' text only it keeps until notes.json can be written.
            self.space.frozen |= self.notes_frozen;
            spaces.push(std::mem::take(&mut self.space));
        }
        self.sidebar = index.sidebar;
        self.active = index.active.min(spaces.len() - 1);
        self.space = spaces.remove(self.active);
        self.others = spaces;
        if no_index && !first_saved {
            // A first run: one terminal, centered.
            self.space.pristine = true;
            let vp = self.viewport(window);
            let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, self.cell.size);
            let rect = Rect::new((vp.w - size.w) / 2.0, (vp.h - size.h) / 2.0, size.w, size.h);
            // Templum's, not a change of the person's to take back.
            self.quiet = true;
            self.open(rect, window, cx);
            self.quiet = false;
        }
    }

    /// Shows workspace `to` (a position in the sidebar). What only makes sense on screen ends
    /// with the one left: its fading blocks, its animations, a drag.
    pub(crate) fn switch_to(&mut self, to: usize, window: &mut Window, cx: &mut Context<Self>) {
        if to == self.active || to > self.others.len() {
            return;
        }
        // A menu for something on the canvas left behind, and the screen it was picked on.
        self.menu = None;
        if let Some(screen) = self.screen.take() {
            self.screen = Some(if screen.closing {
                screen
            } else {
                screen.reversed(std::time::Instant::now())
            });
        }
        let left = &mut self.space;
        // Their shells close now.
        left.ghosts.clear();
        left.appearing.clear();
        left.hovered = None;
        left.guides.clear();
        left.flight = None;
        left.view = left.canvas.camera;
        left.selected_ink = None;
        self.drag = None;
        self.press = None;
        self.marked = None;
        let current = std::mem::take(&mut self.space);
        self.others.insert(self.active, current);
        self.space = self.others.remove(to);
        self.active = to;
        if self.space.stopped {
            self.start_space(window, cx);
        }
        self.space.view = self.space.canvas.camera;
        window.focus(&self.focus_handle, cx);
        self.write_index(window, cx);
        cx.notify();
    }

    /// The next (`step` 1) or previous (-1) workspace, wrapping around.
    pub(super) fn cycle_space(&mut self, step: i32, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.others.len() as i32 + 1;
        let to = (self.active as i32 + step).rem_euclid(n) as usize;
        self.switch_to(to, window, cx);
    }

    /// Creates an empty workspace after the last one and shows it.
    pub(crate) fn new_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.data_dir.clone();
        let id = session::next_id(|id| {
            self.spaces().any(|s| s.id == id)
                || dir.as_deref().is_some_and(|d| session::id_in_folder(d, id))
        });
        let total = self.others.len() + 1;
        let name = (total + 1..)
            .map(|n| format!("Workspace {n}"))
            .find(|name| !self.spaces().any(|s| s.name == *name))
            .expect("an unused name");
        self.others.push(Space {
            id,
            name,
            ..Space::default()
        });
        self.switch_to(total, window, cx);
    }

    /// A new workspace, shown, with its Edit… dialog open to name it.
    pub(crate) fn add_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_space(window, cx);
        let id = self.space.id.clone();
        space_dialog::open(self, &id, window, cx);
    }

    /// Gives the workspace with `id` a name (an empty one is refused), a look and a folder. A
    /// new image is copied into the data folder, and one taken away removed from it.
    pub(crate) fn edit_space(
        &mut self,
        id: &str,
        name: &str,
        mut look: Look,
        folder: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A workspace needs a name".into());
        }
        let Some(old) = self
            .spaces()
            .find(|s| s.id == id)
            .map(|s| s.look.image.clone())
        else {
            return Ok(());
        };
        if let Some(dir) = self.data_dir.clone()
            && look.image != old
        {
            match &look.image {
                Some(file) => {
                    let kept = session::copy_icon(&dir, id, file).map_err(|e| e.to_string())?;
                    let kept = session::icons_dir(&dir).join(kept);
                    // The same file name may still show the image it replaced.
                    ImageSource::from(kept.clone()).remove_asset(cx);
                    look.image = Some(kept);
                }
                None => {
                    if let Some(gone) = old.as_ref().and_then(|p| p.file_name()) {
                        session::remove_icon(&dir, &gone.to_string_lossy())
                            .map_err(|e| format!("Couldn't remove the image: {e}"))?;
                    }
                }
            }
        }
        if let Some(space) = self.space_by_id_mut(id) {
            space.name = name.to_string();
            space.look = look;
            space.folder = folder;
        }
        self.write_index(window, cx);
        cx.notify();
        Ok(())
    }

    /// Deletes the workspace with `id`: its processes end (see `end_later`), its file is kept
    /// as `.bak` and its image removed. The last one stays.
    pub(crate) fn delete_space(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.order().iter().position(|s| s.id == id) else {
            return;
        };
        if self.others.is_empty() {
            return;
        }
        if at == self.active {
            self.switch_to(if at > 0 { at - 1 } else { 1 }, window, cx);
        }
        let space = self
            .others
            .remove(if at < self.active { at } else { at - 1 });
        if at < self.active {
            self.active -= 1;
        }
        if let Some(dir) = &self.data_dir {
            if let Err(err) = session::set_aside(dir, &space.id) {
                eprintln!("templum: couldn't set {}.json aside: {err}", space.id);
            }
            if let Some(image) = space.look.image.as_ref().and_then(|p| p.file_name())
                && let Err(err) = session::remove_icon(dir, &image.to_string_lossy())
            {
                eprintln!("templum: couldn't remove {}'s image: {err}", space.id);
            }
        }
        let tree = tree_of(&space);
        // Dropping its terminals hangs their shells up.
        drop(space);
        self.end_later(tree, cx);
        self.write_index(window, cx);
        cx.notify();
    }

    /// Stops the workspace with `id` (see `stop_now`); a terminal at work or an unsaved file
    /// asks first, naming them.
    pub(crate) fn stop_space(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(space) = self.spaces().find(|s| s.id == id) else {
            return;
        };
        if !self.can_stop(space) {
            return;
        }
        let (name, busy, unsaved) = (space.name.clone(), self.busy_in(space), space.unsaved(None));
        if busy.is_empty() && unsaved.is_empty() {
            self.stop_now(id, window, cx);
            return;
        }
        let ws = cx.entity();
        let id = id.to_string();
        let title = format!("Stop {name}?");
        let then = "Opening it starts its terminals again.";
        if unsaved.is_empty() {
            prompt::confirm(
                title,
                format!("{} {then}", busy.join(" ")),
                "Stop",
                move |window, cx| ws.update(cx, |ws, cx| ws.stop_now(&id, window, cx)),
                window,
                cx,
            );
            return;
        }
        let (saver, save_id) = (ws.clone(), id.clone());
        let mut why = format!("Unsaved: {}.", unsaved.join(", "));
        for b in &busy {
            why = format!("{why} {b}");
        }
        prompt::save_or_discard(
            title,
            why,
            move |window, cx| {
                saver.update(cx, |ws, cx| {
                    let saved = ws.save_spaces(Some(&save_id), window, cx);
                    if saved {
                        ws.stop_now(&save_id, window, cx);
                    }
                    saved
                })
            },
            move |window, cx| ws.update(cx, |ws, cx| ws.stop_now(&id, window, cx)),
            window,
            cx,
        );
    }

    /// A workspace can be stopped unless it is the last one, or stopped already.
    pub(crate) fn can_stop(&self, space: &Space) -> bool {
        !self.others.is_empty() && !space.stopped
    }

    /// Ends every program of the workspace with `id`, as quitting would: its canvas is written
    /// (the agents in it found, to resume), its blocks dropped and its processes ended. The one
    /// shown first gives its place to the one before it. Opening it starts it again.
    pub(crate) fn stop_now(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.order().iter().position(|s| s.id == id) else {
            return;
        };
        if self.others.is_empty() {
            return;
        }
        if at == self.active {
            self.switch_to(if at > 0 { at - 1 } else { 1 }, window, cx);
        }
        let known = self.known_agents();
        // A snapshot still being written is older: it can't replace this one.
        self.generation += 1;
        let generation = self.generation;
        let dir = self.data_dir.clone();
        let Some(space) = self.others.iter_mut().find(|s| s.id == id) else {
            return;
        };
        let (saved, _) = space.layout(known, cx).snapshot();
        let mut failed = None;
        if let Some(dir) = dir.filter(|_| !space.frozen && space.saved.as_ref() != Some(&saved))
            && let Err(err) = session::save(&dir, &space.id, &saved, generation)
        {
            failed = Some(format!("Couldn't save {}'s canvas: {err}", space.name));
        }
        space.saved = Some(saved);
        let tree = tree_of(space);
        space.empty();
        space.stopped = true;
        if let Some(message) = failed {
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::error(message));
        }
        self.end_later(tree, cx);
        self.write_index(window, cx);
        cx.notify();
    }

    /// Brings the shown workspace, stopped, back as Templum starts one: its canvas, terminals in
    /// their folders, agents resumed.
    fn start_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.space.stopped = false;
        if let Some(saved) = self.space.saved.clone() {
            self.restore(saved, window, cx);
        }
    }

    /// Kills, KILL_AFTER from now, what of `tree` the hangup left running.
    pub(super) fn end_later(&self, tree: Vec<templum_terminal::Running>, cx: &mut Context<Self>) {
        if tree.is_empty() {
            return;
        }
        let executor = cx.background_executor().clone();
        let timer = executor.timer(KILL_AFTER);
        executor
            .spawn(async move {
                timer.await;
                templum_terminal::kill_left(&tree);
            })
            .detach();
    }

    /// Why ending a workspace's terminals would lose work: each one at work, by its title.
    pub(crate) fn busy_in(&self, space: &Space) -> Vec<String> {
        space
            .terminals()
            .filter_map(|(_, pane)| {
                let why = alert::busy(&pane.term.status(), self.agent_name(pane))?;
                Some(format!("{}: {why}", pane.term.title()))
            })
            .collect()
    }

    /// Puts the workspace with `id` at position `to`, the others following; the one shown
    /// stays shown.
    pub(crate) fn move_space(
        &mut self,
        id: &str,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.order().iter().position(|s| s.id == id) else {
            return;
        };
        if from == to || to > self.others.len() {
            return;
        }
        let shown = self.space.id.clone();
        let mut all = std::mem::take(&mut self.others);
        all.insert(self.active, std::mem::take(&mut self.space));
        let moved = all.remove(from);
        all.insert(to, moved);
        self.active = all.iter().position(|s| s.id == shown).unwrap_or(0);
        self.space = all.remove(self.active);
        self.others = all;
        self.write_index(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar = !self.sidebar;
        self.write_index(window, cx);
        cx.notify();
    }

    /// The workspaces as the index lists them.
    pub(super) fn index(&self) -> session::Index {
        session::Index {
            version: session::VERSION,
            workspaces: self
                .order()
                .iter()
                .map(|s| session::IndexEntry {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    icon: s.look.icon.map(|i| space_dialog::icon_name(i).to_string()),
                    color: s.look.color.clone(),
                    image: s
                        .look
                        .image
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned()),
                    folder: s.folder.clone(),
                    stopped: s.stopped,
                })
                .collect(),
            active: self.active,
            sidebar: self.sidebar,
        }
    }

    /// Writes the index at once: it changes only on a user's action.
    pub(super) fn write_index(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.data_dir.as_ref().filter(|_| !self.index_frozen) else {
            return;
        };
        if let Err(err) = session::save_index(dir, &self.index()) {
            let message = format!("Couldn't save the workspace list: {err}");
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::error(message));
        }
    }

    /// Every workspace: the active one, then the hidden ones.
    pub(crate) fn spaces(&self) -> impl Iterator<Item = &Space> {
        std::iter::once(&self.space).chain(&self.others)
    }

    /// The workspaces in the sidebar's order.
    pub(crate) fn order(&self) -> Vec<&Space> {
        let mut order: Vec<&Space> = self.others.iter().collect();
        order.insert(self.active, &self.space);
        order
    }

    pub(super) fn space_by_id_mut(&mut self, id: &str) -> Option<&mut Space> {
        std::iter::once(&mut self.space)
            .chain(&mut self.others)
            .find(|s| s.id == id)
    }
}
