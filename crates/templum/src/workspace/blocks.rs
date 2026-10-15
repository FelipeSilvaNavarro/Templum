//! Blocks: adding, focusing and closing them, and the news from their terminals.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::component::Icon;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, SharedString, Window};
use templum_canvas::{Block, BlockId, Point, Rect, chrome};
use templum_terminal::{SpawnOptions, Terminal};

use crate::alert::{self, Alert};
use crate::board::Boards;
use crate::config::WorkingDirectory;
use crate::kanban::{self, Kanban};
use crate::note::Note;
use crate::prompt;

use super::history::{Change, Then};
use super::{
    AGENT_COLS, AGENT_ROWS, BOARD_UNDO, Body, DEFAULT_COLS, DEFAULT_ROWS, Ghost, Pane, Workspace,
    remember, show_notification,
};

impl Workspace {
    /// Adds a block on top, focused, popping in.
    pub(super) fn add_block(&mut self, rect: Rect, body: Body) -> BlockId {
        let id = self.space.canvas.add(rect);
        self.space.blocks.insert(id, body);
        self.added(id);
        id
    }

    /// A block was just added: it is focused, pops in, and Ctrl+Z takes it away (unless the
    /// canvas is being restored or blocks come back by an undo: `quiet`). It gets a new id.
    fn added(&mut self, id: BlockId) {
        let uid = self.new_uid();
        self.space.uids.insert(id, uid);
        let before = self.space.focused;
        self.space.focused = Some(id);
        self.space.selected.clear();
        self.space.selected_wire = None;
        self.space.appearing.insert(id, Instant::now());
        if !self.quiet {
            self.keep(Change::Close(vec![id]), Then::New);
        }
        // In grid mode it takes half of the focused tile; its place on the canvas, for when the
        // grid ends, is off the others' places there.
        if self.space.grid.is_some() {
            let taken: Vec<Rect> = self
                .space
                .canvas
                .blocks()
                .iter()
                .filter(|b| b.id != id)
                .map(|b| self.space.home(b))
                .collect();
            if let Some(b) = self.space.canvas.get_mut(id) {
                b.rect = templum_canvas::cascade(b.rect, &taken);
            }
            self.join_grid(id, before);
        }
    }

    /// A block id no block of any workspace has: five letters and digits.
    // ponytail: a stopped workspace's saved ids aren't checked (1 in 60 million per pair); check
    // `Space::saved` too if one ever collides.
    fn new_uid(&self) -> String {
        loop {
            let uid = base36(uuid::Uuid::new_v4().as_u128() % 36u128.pow(5), 5);
            if !self.uid_taken(&uid) {
                return uid;
            }
        }
    }

    pub(super) fn uid_taken(&self, uid: &str) -> bool {
        self.spaces().any(|s| s.uids.values().any(|u| u == uid))
    }

    /// Block `id`, just restored, takes back its saved id `uid`; one that isn't five lowercase
    /// letters and digits, or is taken (a file edited by hand), leaves it the new one it got.
    pub(super) fn adopt_uid(&mut self, id: BlockId, uid: &str) {
        let valid = uid.len() == 5
            && uid
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        if valid && !self.uid_taken(uid) {
            self.space.uids.insert(id, uid.to_string());
        }
    }

    /// Changes the boards with `f`, keeping them as they were for Ctrl+Z (when it changed
    /// them).
    pub(crate) fn edit_boards<R>(&mut self, f: impl FnOnce(&mut Boards) -> R) -> R {
        let before = self.boards.clone();
        let result = f(&mut self.boards);
        if self.boards != before {
            self.boards_undo.push(before);
            if self.boards_undo.len() > BOARD_UNDO {
                self.boards_undo.remove(0);
            }
        }
        result
    }

    /// Ctrl+Z on a kanban block: the boards as before their last change. False: nothing to undo.
    pub(super) fn undo_boards(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(mut before) = self.boards_undo.pop() else {
            return false;
        };
        // Ids are never handed out twice: a block or a file may still name an undone board.
        before.next_id = before.next_id.max(self.boards.next_id);
        self.boards = before;
        cx.notify();
        true
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
        let anchor = self.space.home(self.space.canvas.get(block)?);
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

    /// Shows the terminal waiting for the person (it asked for attention): this workspace's
    /// first, then the others' in the sidebar's order, the oldest block first. Going there is
    /// seeing it, so the next time shows the next one. None waiting: a notification says so.
    pub(crate) fn go_to_waiting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.order();
        let spaces =
            std::iter::once(&self.space).chain(order.into_iter().filter(|s| s.id != self.space.id));
        let waiting = spaces
            .filter_map(|space| {
                space
                    .terminals()
                    .filter(|(_, pane)| matches!(pane.alert, Some(Alert::Attention(_))))
                    .map(|(id, _)| id)
                    .min_by_key(|id| id.0)
                    .map(|id| (space.id.clone(), id))
            })
            .next();
        let Some((space, id)) = waiting else {
            show_notification(
                window,
                cx,
                Notification::info("No terminal is waiting for you".to_string()),
            );
            return;
        };
        if let Some(pane) = self
            .space_by_id_mut(&space)
            .and_then(|s| s.terminal_mut(id))
        {
            pane.alert = None;
        }
        self.reveal(&space, id, window, cx);
    }

    /// The terminals of this workspace, labeled, those running an agent first: every one, or
    /// those whose folder is inside `root`.
    pub(crate) fn terminal_list(&self, root: Option<&Path>) -> Vec<(BlockId, String)> {
        let mut found: Vec<_> = self
            .space
            .terminals()
            .filter(|(_, pane)| root.is_none_or(|root| pane.works_in(root)))
            .map(|(id, pane)| {
                let agent = self.agent_name(pane);
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

    /// The agent running in a terminal, by its title (its id when Templum doesn't know it).
    pub(crate) fn agent_name<'a>(&'a self, pane: &'a Pane) -> Option<&'a str> {
        let agent = pane.seen.agent.as_ref()?;
        Some(
            self.agents
                .iter()
                .find(|a| a.id == *agent)
                .map_or(agent.as_str(), |a| a.title.as_str()),
        )
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
        let center = self.space.camera().to_world(self.center(window));
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
        let entry = self.agents.iter().find(|a| a.command == command);
        let command = entry.map_or_else(
            || command.to_string(),
            |a| a.start_command(&uuid::Uuid::new_v4().to_string()),
        );
        // The shell reads it once it is ready.
        let line = format!("{}\r", self.typed(entry, &command));
        // Known at once (Shift+Enter, closing), not at the next snapshot.
        let agent = entry.map(|a| a.id.clone());
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
        let board = self.edit_boards(Boards::add_board);
        self.show_board(block, board, cx);
    }

    /// Deletes a board and its cards; the blocks that showed it show the board list.
    pub(crate) fn delete_board(&mut self, board: u64, cx: &mut Context<Self>) {
        self.edit_boards(|b| b.remove_board(board));
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
            env: self.bus_env.clone(),
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
                            ..Default::default()
                        },
                        usage: None,
                        find: None,
                        state: None,
                    }),
                );
                self.added(id);
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

    /// Closes a block. An editor with unsaved edits asks first: Save all, Don't save, Cancel; a
    /// terminal at work (an agent, a command running) too: Cancel or Close.
    pub(crate) fn close(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.close_blocks(vec![id], Then::New, window, cx);
    }

    /// Closes blocks as `close` does one; several ask once. `then` says where Ctrl+Z keeps them:
    /// an undo or a redo closes only if its step is still next when the person answers.
    pub(super) fn close_blocks(
        &mut self,
        ids: Vec<BlockId>,
        then: Then,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let one = ids.len() == 1;
        let busy: Vec<String> = ids
            .iter()
            .filter_map(|id| {
                let pane = self.space.terminal(*id)?;
                let why = alert::busy(&pane.term.status(), self.agent_name(pane))?;
                Some(if one {
                    why
                } else {
                    format!("{}: {why}", pane.term.title())
                })
            })
            .collect();
        // Only this workspace: block ids count from 0 in every one.
        let unsaved: Vec<String> = ids
            .iter()
            .flat_map(|id| self.space.unsaved(Some(*id)))
            .collect();
        if busy.is_empty() && unsaved.is_empty() {
            self.close_now(&ids, then, window, cx);
            return;
        }
        let ws = cx.entity();
        let close = {
            let (ws, ids) = (ws.clone(), ids.clone());
            move |window: &mut Window, cx: &mut App| {
                ws.update(cx, |ws, cx| ws.close_now(&ids, then, window, cx))
            }
        };
        let busy = busy.join(" ");
        if unsaved.is_empty() {
            let title = if one {
                "Close the terminal?".to_string()
            } else {
                format!("Close {} blocks?", ids.len())
            };
            prompt::confirm(title, busy, "Close", close, window, cx);
            return;
        }
        let mut why = format!("Unsaved: {}", unsaved.join(", "));
        if !busy.is_empty() {
            why = format!("{why}. {busy}");
        }
        let save = close.clone();
        prompt::save_or_discard(
            "Save changes?",
            why,
            move |window, cx| {
                // Every one is tried, even after a failure.
                let saved = ws.update(cx, |ws, cx| {
                    let mut ok = true;
                    for id in &ids {
                        ok &= ws.save_all(*id, window, cx);
                    }
                    ok
                });
                if saved {
                    save(window, cx);
                }
                saved
            },
            close,
            window,
            cx,
        );
    }

    /// Closes the blocks `ids` now, kept for Ctrl+Z (or Ctrl+Shift+Z, after an undo) as one
    /// step. Nothing is focused after: the keyboard stays on the canvas, so Ctrl+Z reaches it,
    /// not a terminal.
    pub(super) fn close_now(
        &mut self,
        ids: &[BlockId],
        then: Then,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.take_step(then) {
            return;
        }
        let items: Vec<_> = self
            .space
            .canvas
            .blocks()
            .iter()
            .filter(|b| ids.contains(&b.id))
            .map(|b| (b.id, self.space.item_of(b, cx)))
            .collect();
        // Their wires go with them; undoing the close brings them back.
        let wires: Vec<_> = self
            .space
            .wires
            .iter()
            .filter(|w| ids.iter().any(|id| w.touches(*id)))
            .copied()
            .collect();
        let was_focused = self.space.focused.is_some_and(|id| ids.contains(&id));
        for id in ids {
            self.remove_block(*id, window, cx);
        }
        if was_focused {
            self.space.focused = None;
        }
        if !items.is_empty() {
            self.keep(Change::Open(items, wires), then);
        }
    }

    /// Removes a block and kills its shell (dropping the Terminal shuts it down).
    /// The block leaves the canvas at once; it stays on screen as a ghost while it fades out.
    pub(super) fn remove_block(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((block, _, body)) = self.take_block(id, window, cx) {
            // Its notes stay in the Notes dialog, but for blank ones: nothing is lost.
            for note in body.note().map(Note::ids).unwrap_or_default() {
                self.drop_if_blank(note);
            }
            self.space.ghosts.push(Ghost {
                block,
                body,
                since: Instant::now(),
            });
        }
        cx.notify();
    }

    /// Takes a block off the canvas with what it holds, still running: the block, its rect on
    /// the canvas (also while it is a tile) and its body.
    pub(super) fn take_block(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Block, Rect, Body)> {
        self.release_focus(id, window, cx);
        let block = self.space.canvas.get(id).copied();
        let home = block.map(|b| self.space.home(&b));
        self.space.canvas.remove(id);
        self.space.appearing.remove(&id);
        let body = self.space.blocks.remove(&id);
        self.space.uids.remove(&id);
        self.space.unwire(id);
        if self.space.hovered == Some(id) {
            self.space.hovered = None;
        }
        self.space.selected.retain(|s| *s != id);
        // A tile's neighbour takes its place.
        self.untile(id);
        if self.space.focused == Some(id) {
            self.space.focused = self.space.top();
        }
        // A gesture on a block that's gone would do nothing anyway; end it.
        self.drag = None;
        Some((block?, home?, body?))
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
                        // Another workspace's news shows its icon.
                        let icon = space.look.icon.filter(|_| i > 0);
                        news.push((space.id.clone(), id, title, alert.clone(), icon));
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
        let delivery = self.delivery(window);
        let ws = cx.entity().downgrade();
        for (space, block, title, alert, icon) in news {
            let ws = ws.clone();
            let key = SharedString::from(format!("alert-{space}-{}", block.0));
            let notification = Notification::new().id1::<Alert>(key);
            // A notification's type would draw its own icon over the workspace's.
            let notification = match icon {
                Some(icon) => notification.icon(Icon::new(icon)),
                None => notification.with_type(alert.kind()),
            };
            let notification = notification
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

    /// Where a new terminal starts (None: the home folder): the focused block's folder when
    /// the settings say so, else the workspace's, else the settings'.
    pub(super) fn start_dir(&self, window: &mut Window, cx: &mut App) -> Option<PathBuf> {
        let setting = &self.config.shell.working_directory;
        if matches!(setting, WorkingDirectory::Inherit)
            && let Some(folder) = self
                .space
                .focused
                .and_then(|id| self.space.blocks.get(&id)?.folder())
        {
            return Some(folder);
        }
        // The workspace's choice is nearer than the settings'.
        if let Some(folder) = self.space.folder.clone().filter(|f| f.is_dir()) {
            return Some(folder);
        }
        match setting {
            WorkingDirectory::Inherit | WorkingDirectory::Home => None,
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
        self.space.selected.clear();
        self.space.selected_ink = None;
        self.space.selected_wire = None;
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
        let rect = self.new_terminal_rect(window);
        self.open(rect, window, cx);
    }

    /// Where a new terminal goes: the view's center, clear of the other blocks.
    pub(super) fn new_terminal_rect(&mut self, window: &Window) -> Rect {
        self.space.pristine = false;
        let vp = self.viewport(window);
        let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, self.cell.size);
        let center = self
            .space
            .camera()
            .to_world(Point::new(vp.w / 2.0, vp.h / 2.0));
        let near = Point::new(center.x - size.w / 2.0, center.y - size.h / 2.0);
        self.space.canvas.place_new(size, near)
    }

    pub(super) fn close_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.space.focused {
            self.close(id, window, cx);
            // The keyboard goes on to the block on top, as a terminal app's next tab.
            if self.space.focused.is_none() {
                self.space.focused = self.space.top();
            }
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

/// `n` in base 36 (digits, then lowercase letters), `width` characters, zeros in front.
fn base36(mut n: u128, width: usize) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut text = vec![b'0'; width];
    for c in text.iter_mut().rev() {
        *c = DIGITS[(n % 36) as usize];
        n /= 36;
    }
    String::from_utf8(text).expect("ASCII")
}
