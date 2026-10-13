//! Blocks: adding, focusing and closing them, and the news from their terminals.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::component::notification::{Notification, NotificationDelivery};
use gpui_kit::{App, Context, SharedString, Window};
use templum_canvas::{BlockId, Point, Rect, chrome};
use templum_terminal::{SpawnOptions, Terminal};

use crate::alert::{self, Alert};
use crate::config::WorkingDirectory;
use crate::kanban::{self, Kanban};
use crate::note::{self, Note};
use crate::prompt;

use super::{
    AGENT_COLS, AGENT_ROWS, Body, DEFAULT_COLS, DEFAULT_ROWS, Ghost, Pane, Workspace, remember,
    show_notification,
};

impl Workspace {
    /// Adds a block on top, focused, popping in.
    pub(super) fn add_block(&mut self, rect: Rect, body: Body) -> BlockId {
        let id = self.space.canvas.add(rect);
        self.space.blocks.insert(id, body);
        self.space.focused = Some(id);
        self.space.appearing.insert(id, Instant::now());
        id
    }

    /// A note at the viewport's center, focused, in edit mode.
    pub(crate) fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let center = self.space.canvas.camera.to_world(self.center(window));
        let size = note::SIZE;
        let at = self
            .space
            .canvas
            .place_new(
                size,
                Point::new(center.x - size.w / 2.0, center.y - size.h / 2.0),
            )
            .origin();
        self.add_note_at(at, window, cx);
    }

    /// An empty note, its top-left corner at `at` (world units), with the keyboard.
    pub(crate) fn add_note_at(&mut self, at: Point, window: &mut Window, cx: &mut Context<Self>) {
        self.space.pristine = false;
        let rect = Rect::new(at.x, at.y, note::SIZE.w, note::SIZE.h);
        let note = Note::new("", false, window, cx);
        note.focus(window, cx);
        self.add_block(rect, Body::Note(note));
        cx.notify();
    }

    /// A note shows its rendered text, or its editor again. Leaving the editor hands the keyboard
    /// to the canvas; going back to it gives the editor the keyboard.
    pub(crate) fn toggle_preview(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_focus(id, window, cx);
        if let Some(Body::Note(note)) = self.space.blocks.get_mut(&id) {
            note.preview = !note.preview;
            if !note.preview {
                note.focus(window, cx);
            }
            cx.notify();
        }
    }

    /// A terminal in `folder`, placed to the right of `block`, with `command` typed in (not
    /// run: Enter is the person's).
    pub(crate) fn terminal_beside(
        &mut self,
        block: BlockId,
        folder: &Path,
        command: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BlockId> {
        let anchor = self.space.canvas.get(block)?.rect;
        self.space.pristine = false;
        let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, self.cell.size);
        let rect = self.space.canvas.place_beside(anchor, size);
        let id = self.spawn_block(rect, Some(folder.to_path_buf()), window, cx);
        if let (Some(id), Some(command)) = (id, command) {
            // Typed, not run: the person reads it and presses Enter (it may delete or push).
            // The shell reads it once it is ready.
            if let Some(pane) = self.space.terminal(id) {
                pane.term.write(command.to_string().into_bytes());
            }
        }
        window.focus(&self.focus_handle, cx);
        id
    }

    /// The terminals of this workspace, labeled, those running an agent first: every one, or
    /// those whose folder is inside `root`.
    pub(crate) fn terminal_list(&self, root: Option<&Path>) -> Vec<(BlockId, String)> {
        let mut found: Vec<_> = self
            .space
            .terminals()
            .filter(|(_, pane)| root.is_none_or(|root| pane.works_in(root)))
            .map(|(id, pane)| {
                let agent = pane.seen.agent.as_ref().map(|agent| {
                    self.agents
                        .iter()
                        .find(|a| a.id == *agent)
                        .map_or(agent.as_str(), |a| a.title.as_str())
                });
                let label = label(agent, &pane.term.title(), pane.seen.folder.as_deref());
                (agent.is_none(), id, label)
            })
            .collect();
        found.sort_by_key(|(no_agent, id, _)| (*no_agent, id.0));
        found
            .into_iter()
            .map(|(_, id, label)| (id, label))
            .collect()
    }

    /// Types `text` into the terminal `to` for the person to send (never Enter), then shows it
    /// with the keyboard.
    pub(crate) fn send_to(
        &mut self,
        to: BlockId,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.space.terminal(to) else {
            return;
        };
        pane.term.type_in(text);
        let space = self.space.id.clone();
        self.reveal(&space, to, window, cx);
    }

    /// A block's editor is about to leave the frame: if it has the keyboard, the canvas takes it.
    /// GPUI sends keys to the window's root when the focused element isn't drawn, and the
    /// shortcuts would stop working until a click.
    pub(super) fn release_focus(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .space
            .blocks
            .get(&id)
            .is_some_and(|b| b.has_focus(window, cx))
        {
            window.focus(&self.focus_handle, cx);
        }
    }

    /// A kanban block at the viewport's center, showing the board list.
    pub(crate) fn new_kanban(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let center = self.space.canvas.camera.to_world(self.center(window));
        let size = kanban::SIZE;
        let at = self
            .space
            .canvas
            .place_new(
                size,
                Point::new(center.x - size.w / 2.0, center.y - size.h / 2.0),
            )
            .origin();
        self.add_kanban_at(at, window, cx);
    }

    /// A kanban block showing the board list, its top-left corner at `at` (world units).
    pub(crate) fn add_kanban_at(&mut self, at: Point, window: &mut Window, cx: &mut Context<Self>) {
        self.space.pristine = false;
        let rect = Rect::new(at.x, at.y, kanban::SIZE.w, kanban::SIZE.h);
        self.add_block(rect, Body::Kanban(Kanban::default()));
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// A terminal of the default size, its top-left corner at `at` (world units).
    pub(crate) fn add_terminal_at(
        &mut self,
        at: Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space.pristine = false;
        let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, self.cell.size);
        self.open(Rect::new(at.x, at.y, size.w, size.h), window, cx);
    }

    /// A terminal in `folder`, its top-left corner at `at`, with an agent's `command` typed into
    /// its shell;
    /// the folder becomes this workspace's newest recent one.
    pub(crate) fn launch_agent(
        &mut self,
        command: &str,
        folder: &std::path::Path,
        at: Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BlockId> {
        if !folder.is_dir() {
            let message = format!("{} isn't a folder", folder.display());
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
            return None;
        }
        self.space.pristine = false;
        let size = chrome::block_size(AGENT_COLS, AGENT_ROWS, self.cell.size);
        let rect = Rect::new(at.x, at.y, size.w, size.h);
        let id = self.spawn_block(rect, Some(folder.to_path_buf()), window, cx)?;
        // The shell reads it once it is ready.
        let line = format!("{command}\r");
        // Known at once (Shift+Enter, closing), not at the next snapshot.
        let agent = self
            .agents
            .iter()
            .find(|a| a.command == command)
            .map(|a| a.id.clone());
        let pane = self.space.terminal_mut(id).expect("just spawned");
        pane.term.write(line.into_bytes());
        pane.seen.agent = agent;
        remember(&mut self.space.recent, folder.to_path_buf());
        window.focus(&self.focus_handle, cx);
        Some(id)
    }

    /// The kanban block `block` shows `board`.
    pub(crate) fn show_board(&mut self, block: BlockId, board: u64, cx: &mut Context<Self>) {
        if let Some(k) = self.space.kanban_mut(block) {
            *k = Kanban {
                board: Some(board),
                picking: false,
            };
            cx.notify();
        }
    }

    /// The kanban block `block` shows the board list.
    pub(crate) fn pick_board(&mut self, block: BlockId, cx: &mut Context<Self>) {
        if let Some(k) = self.space.kanban_mut(block) {
            k.picking = !k.picking;
            cx.notify();
        }
    }

    /// A new board, shown in `block`.
    pub(crate) fn new_board(&mut self, block: BlockId, cx: &mut Context<Self>) {
        let board = self.boards.add_board();
        self.show_board(block, board, cx);
    }

    /// Deletes a board and its cards; the blocks that showed it show the board list.
    pub(crate) fn delete_board(&mut self, board: u64, cx: &mut Context<Self>) {
        self.boards.remove_board(board);
        cx.notify();
    }

    /// Starts a terminal in a new focused block, in the configured start folder.
    pub(crate) fn open(&mut self, rect: Rect, window: &mut Window, cx: &mut Context<Self>) {
        let folder = self.start_dir(window, cx);
        self.spawn_block(rect, folder, window, cx);
    }

    /// Starts a terminal in `folder` (None: the home folder) in a new focused block; on failure,
    /// a notification instead.
    pub(super) fn spawn_block(
        &mut self,
        rect: Rect,
        folder: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BlockId> {
        let (cols, rows) = chrome::grid_size(chrome::content_rect(rect).size(), self.cell.size);
        let command = self.command.clone().or_else(|| {
            let shell = &self.config.shell;
            shell
                .program
                .clone()
                .map(|program| (program, shell.args.clone()))
        });
        let opts = SpawnOptions {
            cols,
            rows,
            cell_px: self.cell.px(),
            command,
            palette: self.theme.palette,
            options: self.config.term_options(),
            working_directory: folder.clone(),
            integration: self
                .integration
                .clone()
                .filter(|_| self.config.shell.integration),
        };
        let start = folder.or_else(std::env::home_dir);
        let id = match Terminal::spawn(opts, self.notify.clone()) {
            Ok(term) => {
                let id = self.space.canvas.add(rect);
                self.space.blocks.insert(
                    id,
                    Body::Terminal(Pane {
                        term,
                        snapshot: Rc::default(),
                        heard: (0, 0),
                        alert: None,
                        seen: templum_terminal::Inspection {
                            folder: start,
                            agent: None,
                        },
                    }),
                );
                self.space.focused = Some(id);
                self.space.appearing.insert(id, Instant::now());
                Some(id)
            }
            Err(err) => {
                eprintln!("templum: couldn't start a shell: {err}");
                let message = format!("Couldn't start a shell: {err}");
                show_notification(window, cx, Notification::error(message));
                None
            }
        };
        cx.notify();
        id
    }

    /// Closes a block. An editor with unsaved edits asks first: Save all, Don't save, Cancel.
    pub(crate) fn close(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        // Only this workspace: block ids count from 0 in every one.
        let unsaved = self.space.unsaved(Some(id));
        if unsaved.is_empty() {
            self.remove_block(id, window, cx);
            return;
        }
        let ws = cx.entity();
        let saver = ws.clone();
        prompt::save_or_discard(
            "Save changes?",
            format!("Unsaved: {}", unsaved.join(", ")),
            move |window, cx| {
                saver.update(cx, |ws, cx| {
                    let saved = ws.save_all(id, window, cx);
                    if saved {
                        ws.remove_block(id, window, cx);
                    }
                    saved
                })
            },
            move |window, cx| ws.update(cx, |ws, cx| ws.remove_block(id, window, cx)),
            window,
            cx,
        );
    }

    /// Removes a block and kills its shell (dropping the Terminal shuts it down).
    /// The block leaves the canvas at once; it stays on screen as a ghost while it fades out.
    pub(super) fn remove_block(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_focus(id, window, cx);
        let block = self.space.canvas.get(id).copied();
        self.space.canvas.remove(id);
        self.space.appearing.remove(&id);
        let body = self.space.blocks.remove(&id);
        if let (Some(block), Some(body)) = (block, body) {
            self.space.ghosts.push(Ghost {
                block,
                body,
                since: Instant::now(),
            });
        }
        if self.space.hovered == Some(id) {
            self.space.hovered = None;
        }
        if self.space.focused == Some(id) {
            self.space.focused = self.space.canvas.top();
        }
        // A gesture on a block that's gone would do nothing anyway; end it.
        self.drag = None;
        cx.notify();
    }

    /// Finds the news in every workspace's terminals (see [`alert::news`]). A terminal being looked
    /// at, focused in the active window, has none, and loses its alert; another gets an alert and
    /// a notification (the first news until it is seen), also the system's when the window is in
    /// the background.
    pub(super) fn hear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = window.is_window_active();
        let several = !self.others.is_empty();
        let mut news = Vec::new();
        let mut finished_in: Vec<PathBuf> = Vec::new();
        let spaces = std::iter::once(&mut self.space).chain(&mut self.others);
        for (i, space) in spaces.enumerate() {
            let terminals = space
                .blocks
                .iter_mut()
                .filter_map(|(id, body)| Some((*id, body.terminal_mut()?)));
            for (id, pane) in terminals {
                let status = pane.term.status();
                let new = alert::news(&status, pane.heard);
                if status.finished > pane.heard.0 {
                    let reported = pane.term.processes().reported;
                    if reported.is_some() {
                        pane.seen.folder.clone_from(&reported);
                    }
                    if i == 0 {
                        finished_in.extend(reported);
                    }
                }
                pane.heard = (status.finished, status.attention);
                if active && i == 0 && space.focused == Some(id) {
                    pane.alert = None;
                } else if let Some(alert) = new {
                    // One notification until it is seen: a terminal ringing on and on doesn't flood.
                    if pane.alert.is_none() {
                        let mut title = pane.term.title();
                        if several {
                            title = format!("{title} · {}", space.name);
                        }
                        news.push((space.id.clone(), id, title, alert.clone()));
                    }
                    pane.alert = Some(alert);
                }
            }
        }
        // `git add` in a terminal shows in the block within a frame.
        let repos: Vec<BlockId> = self
            .space
            .blocks
            .iter()
            .filter_map(|(id, b)| {
                let root = &b.git()?.repo()?.root;
                finished_in
                    .iter()
                    .any(|f| f.starts_with(root))
                    .then_some(*id)
            })
            .collect();
        for id in repos {
            self.refresh_git(id, window, cx);
        }
        let delivery = if self.config.terminal.notifications && !active {
            NotificationDelivery::InAppAndSystem
        } else {
            NotificationDelivery::InApp
        };
        let ws = cx.entity().downgrade();
        for (space, block, title, alert) in news {
            let ws = ws.clone();
            let key = SharedString::from(format!("alert-{space}-{}", block.0));
            let notification = Notification::new()
                .id1::<Alert>(key)
                .with_type(alert.kind())
                .title(title)
                .message(alert.text())
                .delivery(delivery)
                .on_click(move |_, window, cx| {
                    ws.update(cx, |ws, cx| ws.reveal(&space, block, window, cx))
                        .ok();
                });
            show_notification(window, cx, notification);
        }
    }

    /// Where a new terminal starts (None: the home folder).
    pub(super) fn start_dir(&self, window: &mut Window, cx: &mut App) -> Option<PathBuf> {
        match &self.config.shell.working_directory {
            WorkingDirectory::Inherit => self
                .space
                .focused
                .and_then(|id| self.space.blocks.get(&id)?.folder()),
            WorkingDirectory::Home => None,
            WorkingDirectory::Path(path) if path.is_dir() => Some(path.clone()),
            WorkingDirectory::Path(path) => {
                let message = format!(
                    "{} isn't a folder; starting in the home folder",
                    path.display()
                );
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::warning(message));
                None
            }
        }
    }

    pub(crate) fn focus_block(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.space.pristine = false;
        self.space.canvas.raise(id);
        self.space.focused = Some(id);
        self.space.selected_ink = None;
        // What types inside the block takes the keyboard; anything else leaves it with the
        // workspace.
        if !self
            .space
            .blocks
            .get(&id)
            .is_some_and(|b| b.focus_inner(window, cx))
        {
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }

    pub(crate) fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.space.pristine = false;
        let vp = self.viewport(window);
        let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, self.cell.size);
        let center = self
            .space
            .canvas
            .camera
            .to_world(Point::new(vp.w / 2.0, vp.h / 2.0));
        let near = Point::new(center.x - size.w / 2.0, center.y - size.h / 2.0);
        let rect = self.space.canvas.place_new(size, near);
        self.open(rect, window, cx);
    }

    pub(super) fn close_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.space.focused {
            self.close(id, window, cx);
        }
    }
}

/// A terminal as a menu names it: its agent, its title and its folder's name, each once.
pub(super) fn label(agent: Option<&str>, title: &str, folder: Option<&Path>) -> String {
    let folder = folder
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned());
    let mut parts: Vec<&str> = Vec::new();
    for part in [agent, Some(title), folder.as_deref()]
        .into_iter()
        .flatten()
    {
        if !part.is_empty() && !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.join(" · ")
}
