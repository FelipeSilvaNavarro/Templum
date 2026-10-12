//! The workspaces: loading, switching, adding, renaming and deleting them, and their index.

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};
use templum_canvas::{Rect, chrome};

use crate::board;
use crate::session;

use super::{DEFAULT_COLS, DEFAULT_ROWS, Space, Workspace, show_notification};

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
                ..Space::default()
            };
            match self
                .data_dir
                .as_deref()
                .map(|d| session::load(d, &entry.id))
            {
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
            self.open(rect, window, cx);
        }
    }

    /// Shows workspace `to` (a position in the sidebar). What only makes sense on screen ends
    /// with the one left: its fading blocks, its animations, a drag.
    pub(crate) fn switch_to(&mut self, to: usize, window: &mut Window, cx: &mut Context<Self>) {
        if to == self.active || to > self.others.len() {
            return;
        }
        // A menu for something on the canvas left behind.
        self.menu = None;
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

    /// Renames the workspace with `id`; an empty name is refused.
    pub(crate) fn rename_space(
        &mut self,
        id: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A workspace needs a name");
        }
        if let Some(space) = self.space_by_id_mut(id) {
            space.name = name.to_string();
        }
        self.write_index(window, cx);
        cx.notify();
        Ok(())
    }

    /// Deletes the workspace with `id`, closing its terminals and keeping its file as `.bak`.
    /// The last one stays.
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
        if let Some(dir) = &self.data_dir
            && let Err(err) = session::set_aside(dir, &space.id)
        {
            eprintln!("templum: couldn't set {}.json aside: {err}", space.id);
        }
        // Dropping its terminals shuts their shells down.
        drop(space);
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
