//! Input on the canvas: keys and shortcuts, the pointer, and the text input path.

use std::ops::Range;
use std::time::Instant;

use gpui_kit::component::notification::Notification;
use gpui_kit::{
    Bounds, ClipboardItem, Context, EntityInputHandler, KeyDownEvent, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, PinchEvent, Pixels, Point as PixelPoint, ScrollDelta,
    ScrollWheelEvent, UTF16Selection, Window, px,
};
use templum_canvas::{
    BlockId, Direction, Drawing, Edges, Gesture, Point, Rect, STRING_PX, chrome, constrain,
};
use templum_terminal::{Button, KeyInput, MouseInput, MouseKind, SelectionKind, Side, Target};

use crate::keys::{self, Command};
use crate::menu;

use super::{
    CLICK_SLOP, Drag, QUIT_AGAIN, Tool, Workspace, effective, option_types_text, secondary,
    show_notification, undo_key, workspace_digit,
};

impl Workspace {
    /// Starts moving the canvas with the pointer, wherever it is (the hand, the middle button).
    pub(crate) fn begin_pan(&mut self, pos: PixelPoint<Pixels>) {
        // Grid mode's camera stays put.
        if self.space.grid.is_some() {
            return;
        }
        self.space.pristine = false;
        self.drag = Some(Drag::Canvas(Gesture::pan(self.local(pos))));
        // Not a click on the canvas: letting go keeps the focus where it is.
        self.press = None;
    }

    /// A right-click on the empty canvas: its menu adds blocks there.
    pub(super) fn on_background_right_down(
        &mut self,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // In grid mode the gaps' menu adds blocks: they take half of the focused tile, and
        // their place on the canvas is in the middle of the view left there.
        if self.space.grid.is_some() {
            let at = self.space.camera().to_world(self.center(window));
            self.open_menu(menu::Target::Canvas(at), e.position, window, cx);
            return;
        }
        let at = self.space.canvas.camera.to_world(self.local(e.position));
        if let Some(index) = self.ink_at(at) {
            self.open_menu(menu::Target::Ink(index), e.position, window, cx);
            return;
        }
        self.open_menu(menu::Target::Canvas(at), e.position, window, cx);
    }

    pub(crate) fn hover_block(
        &mut self,
        id: BlockId,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = if hovered {
            Some(id)
        } else if self.space.hovered == Some(id) {
            None
        } else {
            self.space.hovered
        };
        if next != self.space.hovered {
            self.space.hovered = next;
            cx.notify();
        }
        self.follow_pointer(window, cx);
    }

    pub(super) fn local(&self, pos: PixelPoint<Pixels>) -> Point {
        Point::new(
            f32::from(pos.x - self.origin.x),
            f32::from(pos.y - self.origin.y),
        )
    }

    pub(crate) fn begin_move(&mut self, id: BlockId, pos: PixelPoint<Pixels>) {
        // A tile is dropped on another to trade places.
        if self.space.grid.is_some() {
            let at = self.local(pos);
            self.drag = Some(Drag::Swap {
                id,
                over: None,
                from: at,
                at,
            });
            return;
        }
        let p = self.local(pos);
        self.drag = Gesture::move_block(&self.space.canvas, id, p).map(Drag::Canvas);
        self.placing(&[id]);
    }

    /// Grid mode: tile `id` is held by its title and moved past a click (it is drawn picked up).
    pub(crate) fn held(&self, id: BlockId) -> bool {
        matches!(self.drag, Some(Drag::Swap { id: held, from, at, .. })
            if held == id && (at.x - from.x).abs().max((at.y - from.y).abs()) >= CLICK_SLOP)
    }

    /// Grid mode: the tile held by its title would trade places with tile `id` if let go now.
    pub(crate) fn dropping_on(&self, id: BlockId) -> bool {
        matches!(self.drag, Some(Drag::Swap { over: Some(over), .. }) if over == id)
    }

    /// Block `id` is being moved by a title: its own, or a selected block's; in grid mode, held
    /// to trade places.
    pub(crate) fn moving(&self, id: BlockId) -> bool {
        match &self.drag {
            Some(Drag::Canvas(Gesture::Move { id: moved, .. }) | Drag::Swap { id: moved, .. }) => {
                *moved == id
            }
            Some(Drag::Canvas(Gesture::MoveGroup { ids, .. })) => ids.contains(&id),
            _ => false,
        }
    }

    pub(crate) fn begin_resize(&mut self, id: BlockId, edges: Edges, pos: PixelPoint<Pixels>) {
        let p = self.local(pos);
        let min = self
            .space
            .blocks
            .get(&id)
            .map_or(chrome::min_block_size(self.cell.size), |b| {
                b.min_size(self.cell.size)
            });
        self.drag = Gesture::resize_block(&self.space.canvas, id, edges, p, min).map(Drag::Canvas);
        self.placing(&[id]);
    }

    /// A terminal block's grid follows its rect.
    pub(super) fn fit_terminal(&mut self, id: BlockId, rect: Rect) {
        let (cols, rows) = chrome::grid_size(chrome::content_rect(rect).size(), self.cell.size);
        if let Some(pane) = self.space.terminal_mut(id) {
            pane.term.resize(cols, rows, self.cell.px());
        }
    }

    pub(super) fn drag_to(&mut self, pos: PixelPoint<Pixels>, shift: bool, cx: &mut Context<Self>) {
        let p = self.local(pos);
        match &mut self.drag {
            Some(Drag::Canvas(gesture)) => {
                let update = gesture.update(&mut self.space.canvas, p);
                self.space.guides = update.guides;
                if let Some((id, rect)) = update.resized {
                    self.fit_terminal(id, rect);
                }
            }
            Some(Drag::Select(id)) => {
                let id = *id;
                if let (Some((row, col, side)), Some(pane)) =
                    (self.cell_under(id, pos), self.space.terminal(id))
                {
                    pane.term.shared().select_update(row, col, side);
                }
            }
            Some(Drag::Program { id, button, cell }) => {
                let (id, button, was) = (*id, *button, *cell);
                if let Some(now) = self.cell_of(id, pos).filter(|now| *now != was)
                    && self.report(
                        id,
                        now,
                        Some(button),
                        MouseKind::Move,
                        &Modifiers::default(),
                    )
                    && let Some(Drag::Program { cell, .. }) = &mut self.drag
                {
                    *cell = now;
                }
                return;
            }
            Some(Drag::Ink(drawing)) => {
                let camera = &self.space.canvas.camera;
                let world = camera.to_world(p);
                match drawing {
                    Drawing::Stroke { .. } => {
                        if !drawing.push_point(world, STRING_PX / camera.zoom()) {
                            return;
                        }
                    }
                    Drawing::Shape { kind, from, to, .. } => {
                        *to = if shift {
                            constrain(*from, world, *kind)
                        } else {
                            world
                        };
                    }
                }
            }
            Some(Drag::Erase { erased, last }) => {
                let world = self.space.canvas.camera.to_world(p);
                let from = std::mem::replace(last, world);
                let before = *erased;
                if self.erase_along(from, world, before)
                    && let Some(Drag::Erase { erased, .. }) = &mut self.drag
                {
                    *erased = true;
                }
            }
            Some(Drag::Marquee { .. }) => self.marquee_to(p),
            Some(Drag::Swap { id, over, from, at }) => {
                *at = p;
                // Grid mode's camera is at the origin at zoom 1: canvas points are the pointer's.
                // A click isn't a drag: nothing is picked up until the pointer moves past it.
                let held = *id;
                let lifted = (p.x - from.x).abs().max((p.y - from.y).abs()) >= CLICK_SLOP;
                *over = self
                    .space
                    .canvas
                    .blocks()
                    .iter()
                    .filter(|_| lifted)
                    .find(|b| b.id != held && self.space.shown(b.id) && b.rect.contains(p))
                    .map(|b| b.id);
            }
            Some(Drag::MoveInk { index, last, moved }) => {
                let world = self.space.canvas.camera.to_world(p);
                let (dx, dy) = (world.x - last.x, world.y - last.y);
                if dx == 0.0 && dy == 0.0 {
                    return;
                }
                *last = world;
                let (index, first) = (*index, !*moved);
                *moved = true;
                // A click that doesn't drag isn't a step.
                if first {
                    self.remember_ink();
                }
                if let Some(d) = self.space.ink.get_mut(index) {
                    d.move_by(dx, dy);
                }
            }
            Some(Drag::Wire { to, .. }) => *to = self.space.canvas.camera.to_world(p),
            None => return,
        }
        cx.notify();
    }

    /// The terminal cell under a window position: (row, col, side), clamped by the terminal.
    pub(super) fn cell_under(
        &self,
        id: BlockId,
        pos: PixelPoint<Pixels>,
    ) -> Option<(usize, usize, Side)> {
        let block = self.space.canvas.get(id)?;
        let world = self.space.canvas.camera.to_world(self.local(pos));
        let (row, col, right) = chrome::cell_at(block.rect, world, self.cell.size);
        let side = if right { Side::Right } else { Side::Left };
        Some((row, col, side))
    }

    fn cell_of(&self, id: BlockId, pos: PixelPoint<Pixels>) -> Option<(usize, usize)> {
        self.cell_under(id, pos).map(|(row, col, _)| (row, col))
    }

    /// Tells the program in terminal `id` about the mouse at `cell` (row, column): false when it
    /// didn't ask for such events.
    fn report(
        &self,
        id: BlockId,
        (row, col): (usize, usize),
        button: Option<Button>,
        kind: MouseKind,
        m: &Modifiers,
    ) -> bool {
        let input = MouseInput {
            button,
            kind,
            row,
            col,
            ctrl: m.control,
            alt: m.alt,
            shift: m.shift,
        };
        self.space
            .terminal(id)
            .is_some_and(|pane| pane.term.mouse(&input))
    }

    /// A press in terminal `id`'s body goes to its program when it asked for the mouse (a TUI,
    /// Claude Code full screen), unless Shift is held: Shift keeps selecting text. Its moves and
    /// its release follow it.
    pub(crate) fn press_program(&mut self, id: BlockId, e: &MouseDownEvent) -> bool {
        let Some(button) = button_of(e.button).filter(|_| !e.modifiers.shift) else {
            return false;
        };
        let Some(cell) = self.cell_of(id, e.position) else {
            return false;
        };
        if !self.report(id, cell, Some(button), MouseKind::Press, &e.modifiers) {
            return false;
        }
        self.drag = Some(Drag::Program { id, button, cell });
        true
    }

    /// A button let go while a program holds the press: it hears the release.
    pub(super) fn release_program(&mut self, e: &MouseUpEvent) -> bool {
        let Some(Drag::Program { id, cell, .. }) = self.drag else {
            return false;
        };
        if let Some(button) = button_of(e.button) {
            let at = self.cell_of(id, e.position).unwrap_or(cell);
            self.report(id, at, Some(button), MouseKind::Release, &e.modifiers);
        }
        self.end_drag();
        true
    }

    /// The pointer moved with no button down: a program that asked for every move hears it over
    /// its terminal's text, once per cell.
    pub(super) fn hover_program(&mut self, pos: PixelPoint<Pixels>, m: &Modifiers) {
        let Some(id) = self
            .space
            .hovered
            .filter(|id| self.space.terminal(*id).is_some())
        else {
            return;
        };
        let world = self.space.canvas.camera.to_world(self.local(pos));
        let over_text = self
            .space
            .canvas
            .get(id)
            .is_some_and(|b| chrome::content_rect(b.rect).contains(world));
        let Some(cell) = self.cell_of(id, pos).filter(|_| over_text) else {
            return;
        };
        if self.hover_cell != Some((id, cell.0, cell.1)) {
            self.hover_cell = Some((id, cell.0, cell.1));
            self.report(id, cell, None, MouseKind::Move, m);
        }
    }

    /// Mouse down in a body: 1 click selects by character, 2 by word, 3 by line.
    pub(crate) fn begin_select(&mut self, id: BlockId, e: &MouseDownEvent) {
        let Some((row, col, side)) = self.cell_under(id, e.position) else {
            return;
        };
        let Some(pane) = self.space.terminal(id) else {
            return;
        };
        let kind = match e.click_count {
            1 => SelectionKind::Simple,
            2 => SelectionKind::Word,
            _ => SelectionKind::Line,
        };
        pane.term.shared().select_start(row, col, side, kind);
        self.drag = Some(Drag::Select(id));
    }

    /// Opens the link under a press in a terminal: a URL in the browser, a file in an editor
    /// beside the terminal (at its line), its repository's root as the editor's folder. False
    /// when there is none (or the file isn't there).
    pub(crate) fn open_link(
        &mut self,
        id: BlockId,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((row, col, _)) = self.cell_under(id, e.position) else {
            return false;
        };
        let Some(pane) = self.space.terminal(id) else {
            return false;
        };
        match pane.term.shared().link_at(row, col) {
            Some(Target::Url(url)) => {
                cx.open_url(&url);
                true
            }
            Some(Target::File { path, line }) => {
                let home = std::env::home_dir();
                let Some(file) =
                    templum_terminal::resolve(&path, pane.term.cwd().as_deref(), home.as_deref())
                else {
                    return false;
                };
                let Some(folder) = templum_git::root_of(&file).or(file.parent()) else {
                    return false;
                };
                let folder = folder.to_path_buf();
                self.open_in_editor(&folder, &file, line, Some(id), window, cx);
                true
            }
            None => false,
        }
    }

    /// Copies the focused terminal's selection, or the focused Git block's diff selection.
    pub(crate) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let from_git = self
            .space
            .focused
            .and_then(|id| self.space.git(id))
            .and_then(|g| g.diff().selected_text());
        let text = from_git.or_else(|| {
            self.space
                .focused_terminal()
                .and_then(|pane| pane.term.shared().selection_text())
        });
        if let Some(text) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub(crate) fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if let Some(pane) = self.space.focused_terminal() {
            pane.term.paste(&text);
        }
    }

    pub(super) fn end_drag(&mut self) {
        self.drag = None;
        self.press = None;
        self.space.guides.clear();
        self.end_placing();
    }

    /// Mouse up: a press on the empty canvas that didn't pan is a click, and leaves no terminal
    /// focused (typing then reaches none; panning keeps the focus) and no block selected.
    pub(super) fn release(&mut self, pos: PixelPoint<Pixels>, cx: &mut Context<Self>) {
        // The Wire tool let go: on another block, they are wired.
        if let Some(Drag::Wire { from, .. }) = self.drag {
            let at = self.space.canvas.camera.to_world(self.local(pos));
            self.end_wire(from, at, cx);
        }
        // A tile let go over another trades places with it.
        if let Some(Drag::Swap { id, over, .. }) = self.drag {
            if let Some(other) = over {
                self.swap_tiles(id, other);
            }
            cx.notify();
        }
        // Its title's hand opens again.
        if self.space.canvas.blocks().iter().any(|b| self.moving(b.id)) {
            cx.notify();
        }
        if let Some(start) = self.press {
            let p = self.local(pos);
            let moved = (p.x - start.x).abs().max((p.y - start.y).abs());
            if moved < CLICK_SLOP {
                self.unfocused = self.space.focused.take();
                if self.unfocused.is_some() || !self.space.selected.is_empty() {
                    self.space.selected.clear();
                    cx.notify();
                }
            }
        }
        if let Some(Drag::Ink(mut drawing)) = self.drag.take() {
            // The pen catches up with the pointer: the stroke ends where it was let go.
            drawing.push_point(self.space.canvas.camera.to_world(self.local(pos)), 0.0);
            if !drawing.is_empty() {
                self.remember_ink();
                self.space.ink.push(drawing);
                cx.notify();
            }
        }
        self.end_drag();
    }

    pub(crate) fn run(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::NewTerminal => self.new_terminal(window, cx),
            Command::CloseTerminal => self.close_terminal(window, cx),
            Command::CloseTab => self.close_active_tab(window, cx),
            Command::Save => self.save_active(window, cx),
            Command::GoToLine => self.go_to_line(window, cx),
            Command::Copy => {
                if !self.copy_blocks(cx) {
                    self.copy_selection(cx);
                }
            }
            // Blocks on the clipboard are pasted on the canvas, unless a field has the keyboard.
            Command::Paste => {
                if !(self.canvas_has_keys(window) && self.paste_blocks(window, cx)) {
                    self.paste_clipboard(cx);
                }
            }
            Command::PreviousPrompt => self.jump_to_prompt(true, cx),
            Command::NextPrompt => self.jump_to_prompt(false, cx),
            Command::ZoomIn => self.zoom_in(window, cx),
            Command::ZoomOut => self.zoom_out(window, cx),
            Command::ZoomReset => self.zoom_reset(window, cx),
            Command::FitAll => self.fit_all(window, cx),
            Command::ToggleGrid => self.toggle_grid(window, cx),
            Command::FocusLeft => self.focus_tile(Direction::Left, window, cx),
            Command::FocusRight => self.focus_tile(Direction::Right, window, cx),
            Command::FocusUp => self.focus_tile(Direction::Up, window, cx),
            Command::FocusDown => self.focus_tile(Direction::Down, window, cx),
            Command::SwapLeft => self.swap_tile(Direction::Left, cx),
            Command::SwapRight => self.swap_tile(Direction::Right, cx),
            Command::SwapUp => self.swap_tile(Direction::Up, cx),
            Command::SwapDown => self.swap_tile(Direction::Down, cx),
            Command::ThemePicker => self.toggle_theme_picker(cx),
            Command::OpenConfig => self.open_config(window, cx),
            Command::Settings => self.toggle_settings(window, cx),
            Command::NewKanban => self.new_kanban(window, cx),
            Command::NewNote => self.new_note(window, cx),
            Command::NewEditor => self.new_editor(window, cx),
            Command::NewGit => self.new_git(window, cx),
            Command::NextWorkspace => self.cycle_space(1, window, cx),
            Command::PreviousWorkspace => self.cycle_space(-1, window, cx),
            Command::NewWorkspace => self.add_space(window, cx),
            Command::ToggleSidebar => self.toggle_sidebar(window, cx),
            Command::Workspaces => self.toggle_screen(window, cx),
            Command::GoToWaiting => self.go_to_waiting(window, cx),
            Command::Find => self.open_find(window, cx),
            Command::OpenFile => self.open_file_by_name(window, cx),
            Command::Quit => self.quit(window, cx),
        }
    }

    /// Scrolls the focused terminal to its previous or next prompt (shell integration marks them).
    pub(super) fn jump_to_prompt(&mut self, up: bool, cx: &mut Context<Self>) {
        let pane = self.space.focused_terminal();
        if pane.is_some_and(|pane| pane.term.jump_to_prompt(up)) {
            cx.notify();
        }
    }

    /// Quitting closes every terminal, and Q sits next to W (close one terminal): the first
    /// press only says to press again. Pending settings are saved on the way out.
    pub(super) fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self
            .quit_armed
            .take()
            .is_some_and(|at| now.duration_since(at) < QUIT_AGAIN)
        {
            if self.may_close(window, cx) {
                cx.quit();
            }
            return;
        }
        self.quit_armed = Some(now);
        let shortcut = self
            .keymap
            .first(Command::Quit)
            .map(keys::display)
            .unwrap_or_else(|| "the quit shortcut".to_string());
        show_notification(
            window,
            cx,
            Notification::info(format!("Press {shortcut} again to quit")),
        );
    }

    pub(super) fn on_key_down(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The cursor shows again on any key.
        self.blink_on = true;
        // Esc closes the usage popover.
        if self.usage_open && e.keystroke.key == "escape" {
            self.usage_open = false;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        // A program that took over the focused terminal gets the keys of the shortcuts that
        // yield to it (Ctrl+- undoes in Claude Code).
        let (modes, agent) = self
            .space
            .focused_terminal()
            .map(|pane| (pane.term.shared().modes(), pane.seen.agent.is_some()))
            .unwrap_or_default();
        let program = modes.full_screen || modes.kitty_keys || modes.mouse || agent;
        let command = effective(
            self.keymap.command(&e.keystroke),
            self.focused_editor().is_some(),
            self.space.grid.is_some(),
            program,
        );
        // A terminal's find field has the keyboard: what it leaves (Enter, Esc, shortcuts)
        // never reaches the shell. Copy and paste are the field's own; text goes on to it.
        if let Some(id) = self.finding(window, cx) {
            if e.keystroke.key == "escape" {
                self.close_find(id, window, cx);
                cx.stop_propagation();
            } else if command == Some(Command::Paste) {
                // The terminal's paste shortcut pastes into the field.
                self.paste_into_find(id, window, cx);
                cx.stop_propagation();
            } else if let Some(command) =
                command.filter(|c| !matches!(c, Command::Copy | Command::Paste))
            {
                self.command_key(e, Some(command), window, cx);
                cx.stop_propagation();
            }
            return;
        }
        if self.picker.is_some() || command.is_some() {
            // While the picker is open, typing and shortcuts don't reach the terminal.
            self.command_key(e, command, window, cx);
            cx.stop_propagation();
            return;
        }
        // Esc puts a tool that stays on back to Select, closes the shapes pill and deselects.
        if e.keystroke.key == "escape"
            && !e.keystroke.modifiers.modified()
            && (self.tool != Tool::Select
                || self.shapes_open
                || self.space.selected_ink.is_some()
                || !self.space.selected.is_empty())
        {
            self.space.selected.clear();
            self.set_tool(Tool::Select, window, cx);
            cx.stop_propagation();
            return;
        }
        let k = &e.keystroke;
        if let Some(index) = self.space.selected_ink
            && !k.modifiers.modified()
            && matches!(k.key.as_str(), "delete" | "backspace")
        {
            self.delete_ink(index, cx);
            cx.stop_propagation();
            return;
        }
        // The canvas has the keyboard and no terminal does: Delete closes the selected blocks or
        // the focused one; Ctrl+C and Ctrl+V (Cmd on macOS) copy and paste blocks.
        if self.space.focused_terminal().is_none() && self.canvas_has_keys(window) {
            let macos = cfg!(target_os = "macos");
            let done =
                if !k.modifiers.modified() && matches!(k.key.as_str(), "delete" | "backspace") {
                    self.delete_blocks(window, cx)
                } else if secondary(k, "c", macos) {
                    self.copy_blocks(cx)
                } else if secondary(k, "v", macos) {
                    self.paste_blocks(window, cx)
                } else {
                    false
                };
            if done {
                cx.stop_propagation();
                return;
            }
        }
        // Undo and redo are the terminal's keys unless a drawing tool is on or a drawing is
        // selected.
        if (self.tool.draws() || self.space.selected_ink.is_some())
            && let Some(redo) = undo_key(k, cfg!(target_os = "macos"))
        {
            if redo {
                self.redo_ink(cx);
            } else {
                self.undo_ink(cx);
            }
            cx.stop_propagation();
            return;
        }
        // Ctrl+Z on a kanban block undoes the last board change.
        if undo_key(k, cfg!(target_os = "macos")) == Some(false)
            && self
                .space
                .focused
                .and_then(|id| self.space.blocks.get(&id))
                .is_some_and(|b| b.kanban().is_some())
            && self.undo_boards(cx)
        {
            cx.stop_propagation();
            return;
        }
        // Ctrl+Z and Ctrl+Shift+Z on the canvas, no terminal's, nor a drawing's or a board's:
        // undo and redo the blocks' changes.
        if let Some(redo) = undo_key(k, cfg!(target_os = "macos"))
            && self.space.focused_terminal().is_none()
            && self.canvas_has_keys(window)
            && self.undo_canvas(redo, window, cx)
        {
            cx.stop_propagation();
            return;
        }
        if let Some(to) = workspace_digit(&e.keystroke, cfg!(target_os = "macos"))
            && to <= self.others.len()
        {
            self.switch_to(to, window, cx);
            cx.stop_propagation();
            return;
        }
        let Some(pane) = self.space.focused_terminal() else {
            return;
        };
        let k = &e.keystroke;
        // Shift+Enter starts a new line in an agent: with the kitty keyboard on (Claude Code,
        // Codex) it says Shift+Enter itself; otherwise ESC CR, as Alt+Enter sends. A shell still
        // gets Enter.
        let newline = k.key == "enter" && k.modifiers.shift && agent && !modes.kitty_keys;
        let input = KeyInput {
            key: &k.key,
            key_char: k.key_char.as_deref(),
            ctrl: k.modifiers.control,
            alt: newline
                || k.modifiers.alt
                    && !option_types_text(&k.key, k.key_char.as_deref(), cfg!(target_os = "macos")),
            shift: k.modifiers.shift,
        };
        if pane.term.key(&input) {
            // Keeps the platform from also feeding key_char to the text-input path.
            cx.stop_propagation();
        }
    }

    /// The theme picker's keys while it is open, else `command`.
    pub(crate) fn command_key(
        &mut self,
        e: &KeyDownEvent,
        command: Option<Command>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker.is_some() {
            if command == Some(Command::ThemePicker) {
                self.cancel_theme(cx);
            } else {
                match e.keystroke.key.as_str() {
                    "up" => self.move_theme(-1, cx),
                    "down" => self.move_theme(1, cx),
                    "enter" => {
                        if let Some(i) = self.picker.as_ref().and_then(|p| p.selected) {
                            self.choose_theme(i, cx);
                        }
                    }
                    "escape" => self.cancel_theme(cx),
                    _ => {}
                }
            }
        } else if let Some(command) = command {
            // Holding the quit shortcut repeats it: that's not a second press.
            if !(e.is_held && command == Command::Quit) {
                self.run(command, window, cx);
            }
        }
    }

    pub(super) fn on_background_down(
        &mut self,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Grid mode's gaps hold nothing.
        if self.space.grid.is_some() {
            return;
        }
        window.focus(&self.focus_handle, cx);
        self.space.pristine = false;
        let p = self.local(e.position);
        // Drawings are hit only here, where no block is (blocks stop the press).
        let world = self.space.canvas.camera.to_world(p);
        if e.modifiers.shift && e.click_count == 1 {
            self.space.selected_ink = None;
            self.begin_marquee(p);
            cx.notify();
            return;
        }
        if e.click_count == 1
            && self.tool == Tool::Select
            && let Some(index) = self.ink_at(world)
        {
            self.space.selected_ink = Some(index);
            self.space.focused = None;
            self.drag = Some(Drag::MoveInk {
                index,
                last: world,
                moved: false,
            });
            self.press = None;
            cx.notify();
            return;
        }
        if self.space.selected_ink.take().is_some() {
            cx.notify();
        }
        if e.click_count == 2 {
            let at = self.space.canvas.camera.to_world(p);
            // The first click took the focus: the new terminal still starts in its folder.
            if self.space.focused.is_none() {
                self.space.focused = self.unfocused.take();
            }
            self.add_terminal_at(at, window, cx);
        } else {
            self.drag = Some(Drag::Canvas(Gesture::pan(p)));
            self.press = Some(p);
        }
    }

    pub(super) fn on_background_scroll(
        &mut self,
        e: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Grid mode's camera stays put.
        if self.space.grid.is_some() {
            return;
        }
        self.space.pristine = false;
        let p = self.local(e.position);
        // The hand (its tool, or Alt held) zooms without Ctrl: dragging moves the canvas.
        if e.modifiers.control || e.modifiers.alt || self.tool == Tool::Hand {
            self.zoom_acc += match e.delta {
                ScrollDelta::Lines(d) => d.y * 50.0,
                ScrollDelta::Pixels(d) => f32::from(d.y),
            };
            if self.zoom_acc.abs() >= 50.0 {
                let delta = self.zoom_acc.signum() as i32;
                self.zoom_acc = 0.0;
                self.zoom_to(delta, p, cx);
            }
        } else {
            let d = e.delta.pixel_delta(px(self.cell.size.h));
            self.pan_by(Point::new(f32::from(d.x), f32::from(d.y)));
        }
        cx.notify();
    }

    pub(crate) fn on_pinch(&mut self, e: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.space.grid.is_some() {
            return;
        }
        self.space.pristine = false;
        self.zoom_acc += e.delta;
        if self.zoom_acc.abs() >= 0.15 {
            let p = self.local(e.position);
            let delta = self.zoom_acc.signum() as i32;
            self.zoom_acc = 0.0;
            self.zoom_to(delta, p, cx);
        }
    }

    /// Wheel over a terminal body scrolls it; with Ctrl it bubbles up and zooms the canvas.
    pub(crate) fn scroll_terminal(
        &mut self,
        id: BlockId,
        e: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        if e.modifiers.control {
            return;
        }
        cx.stop_propagation();
        let line = self.cell.size.h * self.space.canvas.camera.zoom();
        self.scroll_acc += f32::from(e.delta.pixel_delta(px(line)).y) / line;
        let lines = self.scroll_acc.trunc();
        self.scroll_acc -= lines;
        if lines != 0.0 {
            // A program that asked for the mouse gets the wheel; Shift scrolls the history.
            let button = if lines > 0.0 {
                Button::WheelUp
            } else {
                Button::WheelDown
            };
            let cell = self.cell_of(id, e.position).filter(|_| !e.modifiers.shift);
            let told = cell.is_some_and(|cell| {
                (0..lines.abs() as usize)
                    .all(|_| self.report(id, cell, Some(button), MouseKind::Press, &e.modifiers))
            });
            if !told && let Some(pane) = self.space.terminal(id) {
                pane.term.scroll(lines as i32);
            }
            cx.notify();
        }
    }
}

fn button_of(button: MouseButton) -> Option<Button> {
    match button {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Middle => Some(Button::Middle),
        MouseButton::Right => Some(Button::Right),
        _ => None,
    }
}

/// Text input path (IME, dead keys, composed chars): GPUI calls it for keys on_key_down left.
impl EntityInputHandler for Workspace {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|m| 0..m.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = None;
        if let Some(pane) = self.space.focused_terminal() {
            pane.term.write(text.as_bytes().to_vec());
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = (!text.is_empty()).then(|| text.to_string());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.ime_cursor
    }

    fn character_index_for_point(
        &mut self,
        _: PixelPoint<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
