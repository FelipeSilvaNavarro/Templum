//! The main view: the canvas of terminal blocks.

mod blocks;
mod drawing;
mod file_tree;
mod files;
mod git_branches;
mod git_changes;
mod git_history;
mod git_ops;
mod persist;
mod preferences;
mod spaces;
mod view;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui_kit::base::motion::{MotionStatus, Transition, transition_with_status};
use gpui_kit::component::WindowExt;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, AnyElement, App, Bounds, ClipboardItem, Context, DispatchPhase, ElementInputHandler,
    EntityInputHandler, FocusHandle, Focusable as _, FontWeight, Hsla, KeyBinding, KeyDownEvent,
    Keystroke, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PinchEvent, Pixels, Point as PixelPoint, ScrollDelta, ScrollWheelEvent, SharedString,
    Subscription, Task, TestSupportExt, UTF16Selection, Window, WindowAppearance, canvas, div,
    fill, point, px, size,
};
use templum_canvas::{
    Block, BlockId, Camera, CameraFlight, Canvas, Color, Drawing, Gesture, Guide, Point, STRING_PX,
    ShapeKind, Size, chrome, constrain, dot_grid, dot_size,
};
use templum_git::GitError;
use templum_terminal::{KeyInput, Notify, SelectionKind, Side, Snapshot, Terminal};

use crate::agents::{self, AgentEntry};
use crate::alert::Alert;
use crate::block;
use crate::board::Boards;
use crate::config::{self, Config, Edit, ThemeConfig};
use crate::editor::{self, Editor};
use crate::git_block::{self, Git};
use crate::grid::CellMetrics;
use crate::ink;
use crate::kanban::{self, Kanban};
use crate::keys::{self, Command, Keymap};
use crate::menu;
use crate::motion;
use crate::note::{self, Note};
use crate::session;
use crate::settings::{self, Panel};
use crate::sidebar;
use crate::theme::{self, Entry, Theme, Ui};
use crate::theme_picker::{self, Picker};
use crate::title_bar;
use crate::toolbar;

pub(crate) use git_ops::{Extra, notify_git_error};

pub(crate) const DEFAULT_COLS: u16 = 80;
pub(crate) const DEFAULT_ROWS: u16 = 24;
/// An agent's terminal: agents draw wide interfaces.
const AGENT_COLS: u16 = 110;
const AGENT_ROWS: u16 = 32;
/// How many folders a workspace remembers for agents.
pub(crate) const RECENT: usize = 8;
/// The largest block a saved canvas restores: a corrupt size mustn't allocate a giant grid.
const MAX_COLS: u16 = 2000;
const MAX_ROWS: u16 = 1000;
/// A press that moves less than this (screen pixels) before release is a click, not a pan.
const CLICK_SLOP: f32 = 4.0;
/// Drawing changes kept for undo, per workspace.
pub(crate) const INK_UNDO: usize = 50;
/// Cursor blink half-period.
const BLINK: Duration = Duration::from_millis(500);
/// Quitting takes a second press of its shortcut within this long.
const QUIT_AGAIN: Duration = Duration::from_secs(2);
/// A settings change is written to config.toml this long after the last one.
const SAVE_DELAY: Duration = Duration::from_millis(300);

pub fn bind_keys(cx: &mut App) {
    // gpui-kit's Root binds tab/shift-tab to focus navigation: give them back to the shell.
    cx.bind_keys([
        KeyBinding::new("tab", gpui_kit::NoAction {}, Some("Root")),
        KeyBinding::new("shift-tab", gpui_kit::NoAction {}, Some("Root")),
    ]);
    // Templum's own shortcuts are matched by the workspace's keymap, so config.toml can change
    // them.
}

pub(crate) struct Pane {
    pub term: Terminal,
    pub snapshot: Rc<RefCell<Snapshot>>,
    /// The terminal's finished and attention counts when last heard (`Workspace::hear`).
    pub heard: (u64, u64),
    /// News nobody has looked at yet; gone once the block is focused in the active window.
    pub alert: Option<Alert>,
}

/// A Git block of any workspace: block ids count from 0 in every one, so a result that comes
/// back after a switch still finds its block.
#[derive(Clone, Debug)]
pub(crate) struct GitAt {
    pub space: String,
    pub block: BlockId,
}

/// A closed block, still drawn while it fades out. A terminal's shell lives until then.
pub(crate) struct Ghost {
    pub block: Block,
    pub body: Body,
    since: Instant,
}

/// What a block holds. Every block on a canvas is one of these.
pub(crate) enum Body {
    Terminal(Pane),
    Kanban(Kanban),
    Note(Note),
    Editor(Editor),
    /// Boxed: its fields (the editors of its diff and commit form) outweigh the other kinds.
    Git(Box<Git>),
}

impl Body {
    pub fn terminal(&self) -> Option<&Pane> {
        match self {
            Body::Terminal(pane) => Some(pane),
            _ => None,
        }
    }

    pub fn terminal_mut(&mut self) -> Option<&mut Pane> {
        match self {
            Body::Terminal(pane) => Some(pane),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn kanban(&self) -> Option<&Kanban> {
        match self {
            Body::Kanban(kanban) => Some(kanban),
            _ => None,
        }
    }

    pub fn kanban_mut(&mut self) -> Option<&mut Kanban> {
        match self {
            Body::Kanban(kanban) => Some(kanban),
            _ => None,
        }
    }

    /// The smallest block of this kind.
    pub fn min_size(&self, cell: Size) -> Size {
        match self {
            Body::Terminal(_) => chrome::min_block_size(cell),
            Body::Kanban(_) => kanban::MIN,
            Body::Note(_) => note::MIN,
            Body::Editor(_) => editor::MIN,
            Body::Git(_) => git_block::MIN,
        }
    }

    pub fn note(&self) -> Option<&Note> {
        match self {
            Body::Note(note) => Some(note),
            _ => None,
        }
    }

    pub fn editor(&self) -> Option<&Editor> {
        match self {
            Body::Editor(editor) => Some(editor),
            _ => None,
        }
    }

    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match self {
            Body::Editor(editor) => Some(editor),
            _ => None,
        }
    }

    pub fn git(&self) -> Option<&Git> {
        match self {
            Body::Git(git) => Some(git),
            _ => None,
        }
    }

    pub fn git_mut(&mut self) -> Option<&mut Git> {
        match self {
            Body::Git(git) => Some(git),
            _ => None,
        }
    }

    /// Gives the keyboard to what types inside the block: a note's editor, an editor's open
    /// file. False when nothing does.
    pub fn focus_inner(&self, window: &mut Window, cx: &mut App) -> bool {
        match self {
            Body::Note(note) if !note.preview => {
                note.focus(window, cx);
                true
            }
            Body::Editor(e) => e.focus(window, cx),
            _ => false,
        }
    }

    /// The folder a terminal opened from this block starts in: a terminal's current one, an
    /// editor's, a Git block's repository. None for a note or a kanban block.
    pub fn folder(&self) -> Option<PathBuf> {
        match self {
            Body::Terminal(pane) => pane.term.cwd(),
            Body::Editor(e) => Some(e.folder.clone()),
            Body::Git(g) => Some(
                g.repo()
                    .map_or_else(|| g.folder.clone(), |r| r.root.clone()),
            ),
            Body::Kanban(_) | Body::Note(_) => None,
        }
    }

    /// The keyboard is inside the block.
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        match self {
            Body::Note(note) => note
                .editor
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx),
            Body::Editor(e) => e.has_focus(window, cx),
            Body::Git(g) => g.has_focus(window, cx),
            _ => false,
        }
    }
}

enum Drag {
    Canvas(Gesture),
    /// Selecting text in a block's terminal.
    Select(BlockId),
    /// Drawing with a pen, a highlighter or a shape tool.
    Ink(Drawing),
    /// Erasing: whether anything was erased yet (for undo), and the pointer's last world point.
    Erase {
        erased: bool,
        last: Point,
    },
    /// Moving a selected drawing: `last` is the pointer's previous world point; `moved` once it
    /// has moved (and the undo step is taken).
    MoveInk {
        index: usize,
        last: Point,
        moved: bool,
    },
}

/// One canvas and everything on it: its blocks, their terminals, the focus, and what is
/// animating.
#[derive(Default)]
pub(crate) struct Space {
    /// Names its file; chosen once, never changed.
    pub id: String,
    pub name: String,
    pub canvas: Canvas,
    /// What each block of the canvas holds. Inserted and removed together with `canvas`'s blocks.
    pub blocks: HashMap<BlockId, Body>,
    pub focused: Option<BlockId>,
    /// True until the user first moves the camera or touches a block: meanwhile the startup
    /// terminal stays centered, since tiling compositors resize the window after it opens.
    pub pristine: bool,
    /// The canvas as last read or written, to write only what changed.
    pub saved: Option<session::Saved>,
    /// Its file is kept as it is this session: it couldn't be read, or its terminals couldn't
    /// all start, and writing would lose what it holds.
    pub frozen: bool,
    /// The block under the pointer: it shows its close button and resize grip.
    pub hovered: Option<BlockId>,
    /// The snaps in effect during a drag, drawn as lines; empty otherwise.
    pub guides: Vec<Guide>,
    /// The camera drawn this frame: `canvas.camera` (where it is going), or a frame of `flight`.
    pub view: Camera,
    /// The view gliding to `canvas.camera`, since when.
    pub flight: Option<(CameraFlight, Instant)>,
    /// Blocks still popping in, since when.
    pub appearing: HashMap<BlockId, Instant>,
    /// Closed blocks still fading out.
    pub ghosts: Vec<Ghost>,
    /// The block a double-click on its title framed, and the camera before: double-clicking it
    /// again goes back there.
    pub framed: Option<(BlockId, Camera)>,
    /// The folders agents were opened in on this canvas, newest first.
    pub recent: Vec<PathBuf>,
    /// The drawings, above the blocks.
    pub ink: Vec<Drawing>,
    /// The selected drawing, by its index in `ink`.
    pub selected_ink: Option<usize>,
    /// `ink` before each change, newest last, at most INK_UNDO; and the undone ones.
    pub ink_undo: Vec<Vec<Drawing>>,
    pub ink_redo: Vec<Vec<Drawing>>,
}

impl Space {
    /// The canvas as a snapshot needs it: rects and process ids, cheap on the UI thread.
    fn layout(&self, agents: Arc<Vec<templum_terminal::Detect>>, cx: &App) -> session::Layout {
        let blocks = self.canvas.blocks();
        session::Layout {
            camera: self.canvas.camera,
            blocks: blocks
                .iter()
                .map(|b| match &self.blocks[&b.id] {
                    Body::Terminal(pane) => {
                        session::LayoutBlock::Terminal(b.rect, pane.term.processes())
                    }
                    Body::Kanban(k) => {
                        session::LayoutBlock::Saved(session::Item::Kanban(session::KanbanItem {
                            rect: b.rect.into(),
                            board: k.board,
                        }))
                    }
                    Body::Note(n) => {
                        session::LayoutBlock::Saved(session::Item::Note(session::NoteItem {
                            rect: b.rect.into(),
                            text: n.text(cx),
                            preview: n.preview,
                        }))
                    }
                    Body::Editor(e) => {
                        session::LayoutBlock::Saved(session::Item::Editor(e.item(b.rect)))
                    }
                    Body::Git(g) => session::LayoutBlock::Saved(session::Item::Git(g.item(b.rect))),
                })
                .collect(),
            focused: self
                .focused
                .and_then(|id| blocks.iter().position(|b| b.id == id)),
            recent: self.recent.clone(),
            agents,
            ink: self.ink.iter().map(session::InkItem::of).collect(),
        }
    }

    /// The last snapshot found one of `agents` running in one of its terminals.
    pub fn has_agent(&self, agents: &[AgentEntry]) -> bool {
        self.saved.as_ref().is_some_and(|saved| {
            saved.items.iter().any(|item| match item {
                session::Item::Terminal(t) => t
                    .agent
                    .as_ref()
                    .is_some_and(|id| agents.iter().any(|a| a.id == *id)),
                _ => false,
            })
        })
    }

    /// Every terminal block, in no order.
    pub fn terminals(&self) -> impl Iterator<Item = (BlockId, &Pane)> {
        self.blocks
            .iter()
            .filter_map(|(id, body)| Some((*id, body.terminal()?)))
    }

    pub fn terminal(&self, id: BlockId) -> Option<&Pane> {
        self.blocks.get(&id)?.terminal()
    }

    pub fn terminal_mut(&mut self, id: BlockId) -> Option<&mut Pane> {
        self.blocks.get_mut(&id)?.terminal_mut()
    }

    #[cfg(test)]
    pub fn kanban(&self, id: BlockId) -> Option<&Kanban> {
        self.blocks.get(&id)?.kanban()
    }

    pub fn kanban_mut(&mut self, id: BlockId) -> Option<&mut Kanban> {
        self.blocks.get_mut(&id)?.kanban_mut()
    }

    pub fn editor(&self, id: BlockId) -> Option<&Editor> {
        self.blocks.get(&id)?.editor()
    }

    pub fn editor_mut(&mut self, id: BlockId) -> Option<&mut Editor> {
        self.blocks.get_mut(&id)?.editor_mut()
    }

    pub fn git(&self, id: BlockId) -> Option<&Git> {
        self.blocks.get(&id)?.git()
    }

    pub fn git_mut(&mut self, id: BlockId) -> Option<&mut Git> {
        self.blocks.get_mut(&id)?.git_mut()
    }

    /// The changed files' names: of one block, or of every block of this workspace.
    pub fn unsaved(&self, block: Option<BlockId>) -> Vec<String> {
        self.blocks
            .iter()
            .filter(|(id, _)| block.is_none_or(|b| b == **id))
            .filter_map(|(_, b)| b.editor())
            .flat_map(|e| e.tabs.iter())
            .filter(|t| t.dirty)
            .map(editor::Tab::name)
            .collect()
    }

    /// The focused block's terminal, when the focused block is one.
    pub fn focused_terminal(&self) -> Option<&Pane> {
        self.focused.and_then(|id| self.terminal(id))
    }
}

/// What the pointer does on the canvas. Block tools act at once and don't stay on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tool {
    #[default]
    Select,
    /// Pans with any drag or wheel, over blocks too, like holding Alt.
    Hand,
    Pen,
    Highlighter,
    Shape(ShapeKind),
    Eraser,
}

impl Tool {
    /// A tool that draws: it stays on, and a layer over the blocks takes the pointer.
    pub fn draws(self) -> bool {
        matches!(
            self,
            Tool::Pen | Tool::Highlighter | Tool::Shape(_) | Tool::Eraser
        )
    }
}

/// Called at the start of every render; returning false removes it (the bench drives frames).
pub(crate) type FrameHook =
    Box<dyn FnMut(&mut Workspace, &mut Window, &mut Context<Workspace>) -> bool>;

pub struct Workspace {
    /// The workspace on screen.
    pub(crate) space: Space,
    /// The hidden ones, in the sidebar's order without the active one. Their terminals run on.
    pub(crate) others: Vec<Space>,
    /// The active workspace's position in the sidebar.
    pub(crate) active: usize,
    /// Every kanban board, shown by kanban blocks in any workspace.
    pub(crate) boards: Boards,
    /// The boards as last read or written, to write only what changed.
    boards_saved: Option<Boards>,
    /// `boards.json`, or the workspace list, couldn't be read: kept as it is this session.
    boards_frozen: bool,
    index_frozen: bool,
    /// The sidebar is shown.
    pub(crate) sidebar: bool,
    pub(crate) cell: CellMetrics,
    /// IME pre-edit text, drawn at the cursor until committed.
    pub(crate) marked: Option<String>,
    pub(crate) ime_cursor: Option<Bounds<Pixels>>,
    pub(crate) on_frame: Option<FrameHook>,
    drag: Option<Drag>,
    /// Where a press on the empty canvas started: released in place, it's a click.
    press: Option<Point>,
    /// The block the last click on the empty canvas unfocused: a double-click's terminal starts
    /// in its folder.
    unfocused: Option<BlockId>,
    pub(crate) focus_handle: FocusHandle,
    /// Window position of the workspace's top-left: screen coordinates are relative to it.
    origin: PixelPoint<Pixels>,
    notify: Notify,
    /// Sub-step accumulators: wheel zoom, terminal scrolling (in lines).
    zoom_acc: f32,
    scroll_acc: f32,
    /// Program for new terminals instead of the login shell (tests).
    command: Option<(String, Vec<String>)>,
    /// Where config.toml and the themes folder live (None: defaults only, nothing saved).
    pub(crate) config_dir: Option<PathBuf>,
    /// Where the canvas is saved (None: it isn't kept).
    pub(crate) data_dir: Option<PathBuf>,
    /// Where the shell integration's scripts were written (None: not written, no integration).
    integration: Option<PathBuf>,
    /// Counts snapshots: a write older than the last one done is dropped (session::save).
    generation: u64,
    /// A background snapshot is reading or writing: the next waits for the next period.
    snapshotting: bool,
    /// The last canvas write failed (and said so); the next good one says it works again.
    canvas_save_failed: bool,
    _snapshots: Task<()>,
    pub(crate) themes: Vec<Entry>,
    /// The system's appearance.
    dark: bool,
    /// What everything is drawn with.
    pub(crate) theme: Rc<Theme>,
    /// The chrome's colors this frame: the theme's, or on their way to them after a change.
    /// Terminal text and gpui-kit's widgets use the theme's at once.
    pub(crate) ui: Ui,
    /// The active config.toml: the last valid one.
    pub(crate) config: Config,
    /// Its shortcuts.
    pub(crate) keymap: Keymap,
    /// The AI agents in effect: Templum's, with the config's changes, and the user's.
    pub(crate) agents: Vec<AgentEntry>,
    /// What config.toml looked like at the last read, for polling.
    config_stamp: config::Stamp,
    /// The last read failed: the next good one says so.
    config_failed: bool,
    /// The system's appearance. `dark` is what's in effect, since config.toml may pin a mode.
    system_dark: bool,
    _poll: Task<()>,
    /// Settings changes for the next frame to apply (applying a font measures text in the window).
    pending: Option<Config>,
    /// Settings changes not yet written to config.toml, one per key.
    unsaved: Vec<Edit>,
    /// Writes `unsaved` SAVE_DELAY after the last change.
    _save: Task<()>,
    /// Why the last write of config.toml failed; cleared by the next good write or reload.
    pub(crate) save_error: Option<String>,
    /// Write `unsaved` when the window closes or the app quits.
    _flush: Vec<Subscription>,
    /// Installed monospaced families, measured when the settings panel opens.
    pub(crate) fonts: Vec<SharedString>,
    /// The shells found on this computer, and the default's name, read when the panel opens.
    pub(crate) shells: Vec<templum_terminal::ShellOption>,
    pub(crate) default_shell: String,
    /// What the user typed in the panel's text fields that can't apply yet (an unclosed quote),
    /// by the config key it is for.
    pub(crate) drafts: HashMap<&'static str, String>,
    /// When the quit shortcut was pressed once: a second press soon after quits.
    quit_armed: Option<Instant>,
    /// The settings panel, open or sliding out.
    pub(crate) settings: Option<Panel>,
    /// The panel's keyboard focus: Esc and the settings shortcut close it from there.
    pub(crate) settings_focus: FocusHandle,
    /// The command whose shortcut the panel is recording: keys go there before anything else.
    pub(crate) recording: Option<Command>,
    /// Why the last key pressed while recording wasn't taken.
    pub(crate) key_note: Option<String>,
    /// Shortcut resets asked for in this event ("Reset All" asks for several), decided together.
    resetting: Vec<Command>,
    /// The open theme picker.
    pub(crate) picker: Option<Picker>,
    /// The open right-click menu.
    pub(crate) menu: Option<menu::OpenMenu>,
    /// Alt was held at the last frame (the hand layer is drawn).
    pub(crate) hand: bool,
    /// The toolbar's active tool.
    pub(crate) tool: Tool,
    /// The drawing tools' color and width.
    pub(crate) ink_color: Color,
    pub(crate) ink_width: ink::Width,
    /// The shape last picked: the Shapes button shows it.
    pub(crate) last_shape: Option<ShapeKind>,
    /// The shapes pill is open above the toolbar.
    pub(crate) shapes_open: bool,
    /// The drawings' paths, built once per zoom step.
    ink_cache: Rc<RefCell<ink::Cache>>,
    /// The focused terminal's cursor blinks (painting reports it); and the blink phase.
    pub(crate) cursor_blinks: bool,
    pub(crate) blink_on: bool,
    _blink: Task<()>,
    /// Checks open files for changes on disk every editor::POLL.
    _files: Task<()>,
    /// Reads every Git block on screen every git_block::POLL.
    _git: Task<()>,
    /// `git --version` with the configured program, read when the settings panel opens: Ok is
    /// the version, Err why it can't run. None while it is being read.
    pub(crate) git_version: Option<Result<String, String>>,
    _wake: Task<()>,
}

impl Workspace {
    /// `data_dir`: where the canvas is kept (None: it isn't, as for `--bench`).
    pub fn new(data_dir: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with(None, config::dir(), data_dir, window, cx)
    }

    /// `command`: program for new terminals instead of the login shell (tests).
    pub fn with(
        command: Option<(String, Vec<String>)>,
        config_dir: Option<PathBuf>,
        data_dir: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // PTY threads notify through a channel; this task turns that into a redraw.
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
        let notify: Notify = Arc::new(move || {
            let _ = tx.unbounded_send(());
        });
        let wake = cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let (config, config_error) = match config::load(config_dir.as_deref()) {
            Ok(config) => (config, None),
            Err(message) => (Config::default(), Some(message)),
        };
        let config_stamp = config::Stamp::of(config_dir.as_deref());
        let themes = theme::load(config_dir.as_deref());
        for warning in missing_themes(&themes, &config.theme) {
            eprintln!("templum: {warning}");
            show_notification(window, cx, Notification::warning(warning));
        }
        let system_dark = is_dark(window.appearance());
        let dark = config.theme.mode.dark(system_dark);
        let theme = theme::pick(&themes, config.theme.name(dark), theme::default_name(dark));
        theme::apply_to_kit(&theme, cx);
        let (cell, font_warning) =
            CellMetrics::measure(window, config.font.family.as_deref(), config.font.size);
        if let Some(message) = &config_error {
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::error(message.clone()));
        }
        if let Some(warning) = font_warning {
            eprintln!("templum: {warning}");
            show_notification(window, cx, Notification::warning(warning));
        }
        let effective_agents = agents::effective(&config.agents);
        let integration = data_dir.as_deref().and_then(|dir| {
            templum_terminal::install(dir)
                .inspect_err(|err| {
                    let message = format!(
                        "Couldn't write the shell integration to {}: {err}",
                        dir.display()
                    );
                    eprintln!("templum: {message}");
                    show_notification(window, cx, Notification::warning(message));
                })
                .ok()
        });
        let (keymap, key_warnings) = Keymap::new(&config.keys, cfg!(target_os = "macos"))
            .expect("config::parse validated the shortcuts");
        for warning in key_warnings {
            eprintln!("templum: {warning}");
            show_notification(window, cx, Notification::warning(warning));
        }
        let poll = cx.spawn_in(window, async move |ws, cx| {
            loop {
                cx.background_executor().timer(config::POLL).await;
                if ws
                    .update_in(cx, |ws, window, cx| ws.poll_config(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let snapshots = cx.spawn_in(window, async move |ws, cx| {
            loop {
                cx.background_executor().timer(session::EVERY).await;
                if ws
                    .update_in(cx, |ws, window, cx| ws.snapshot(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        cx.observe_window_appearance(window, |ws, window, cx| {
            ws.set_dark(is_dark(window.appearance()), cx)
        })
        .detach();
        let blink = cx.spawn_in(window, async move |ws, cx| {
            loop {
                cx.background_executor().timer(BLINK).await;
                if ws.update(cx, |ws, cx| ws.tick_blink(cx)).is_err() {
                    break;
                }
            }
        });
        let files = cx.spawn_in(window, async move |ws, cx| {
            loop {
                cx.background_executor().timer(editor::POLL).await;
                if ws
                    .update_in(cx, |ws, window, cx| ws.check_files(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let git = cx.spawn_in(window, async move |ws, cx| {
            loop {
                cx.background_executor().timer(git_block::POLL).await;
                if ws
                    .update_in(cx, |ws, window, cx| ws.poll_git(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        // Closing the window with unsaved files asks first, as quitting does.
        let closing = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            closing
                .update(cx, |ws, cx| ws.may_close(window, cx))
                .unwrap_or(true)
        });
        let flush = vec![
            cx.on_release(|ws, cx| {
                ws.save();
                ws.save_canvas_now(cx);
            }),
            cx.on_app_quit(|ws, cx| {
                ws.save();
                ws.save_canvas_now(cx);
                async {}
            }),
        ];
        let mut ws = Self {
            space: Space::default(),
            others: Vec::new(),
            active: 0,
            boards: Boards::default(),
            boards_saved: None,
            boards_frozen: false,
            index_frozen: false,
            sidebar: true,
            cell,
            marked: None,
            ime_cursor: None,
            on_frame: None,
            drag: None,
            press: None,
            unfocused: None,
            focus_handle,
            origin: PixelPoint::default(),
            notify,
            zoom_acc: 0.0,
            scroll_acc: 0.0,
            command,
            config_dir,
            data_dir,
            integration,
            generation: 0,
            snapshotting: false,
            canvas_save_failed: false,
            _snapshots: snapshots,
            themes,
            dark,
            ui: theme.ui,
            theme,
            config_failed: config_error.is_some(),
            config_stamp,
            system_dark,
            _poll: poll,
            pending: None,
            unsaved: Vec::new(),
            _save: Task::ready(()),
            save_error: None,
            _flush: flush,
            fonts: Vec::new(),
            shells: Vec::new(),
            default_shell: String::new(),
            drafts: HashMap::new(),
            quit_armed: None,
            settings: None,
            settings_focus: cx.focus_handle(),
            recording: None,
            key_note: None,
            resetting: Vec::new(),
            picker: None,
            menu: None,
            hand: false,
            tool: Tool::default(),
            ink_color: Color::Blue,
            ink_width: ink::Width::default(),
            last_shape: None,
            shapes_open: false,
            ink_cache: Default::default(),
            cursor_blinks: false,
            blink_on: true,
            _blink: blink,
            _files: files,
            _git: git,
            git_version: None,
            config,
            keymap,
            agents: effective_agents,
            _wake: wake,
        };
        ws.load_spaces(window, cx);
        ws
    }

    /// Starts moving the canvas with the pointer, wherever it is (the hand, the middle button).
    pub(crate) fn begin_pan(&mut self, pos: PixelPoint<Pixels>) {
        self.space.pristine = false;
        self.drag = Some(Drag::Canvas(Gesture::pan(self.local(pos))));
        // Not a click on the canvas: letting go keeps the focus where it is.
        self.press = None;
    }

    /// A right-click on the empty canvas: its menu adds blocks there.
    fn on_background_right_down(
        &mut self,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self.space.canvas.camera.to_world(self.local(e.position));
        if let Some(index) = self.ink_at(at) {
            self.open_menu(menu::Target::Ink(index), e.position, window, cx);
            return;
        }
        self.open_menu(menu::Target::Canvas(at), e.position, window, cx);
    }

    /// Every BLINK: toggles a blinking cursor, and makes sure a steady one is shown.
    pub(crate) fn tick_blink(&mut self, cx: &mut Context<Self>) {
        let next = !self.cursor_blinks || !self.blink_on;
        if next != self.blink_on {
            self.blink_on = next;
            cx.notify();
        }
    }

    pub(crate) fn hover_block(&mut self, id: BlockId, hovered: bool, cx: &mut Context<Self>) {
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
    }

    fn local(&self, pos: PixelPoint<Pixels>) -> Point {
        Point::new(
            f32::from(pos.x - self.origin.x),
            f32::from(pos.y - self.origin.y),
        )
    }

    pub(crate) fn begin_move(&mut self, id: BlockId, pos: PixelPoint<Pixels>) {
        let p = self.local(pos);
        self.drag = Gesture::move_block(&self.space.canvas, id, p).map(Drag::Canvas);
    }

    pub(crate) fn begin_resize(&mut self, id: BlockId, pos: PixelPoint<Pixels>) {
        let p = self.local(pos);
        let min = self
            .space
            .blocks
            .get(&id)
            .map_or(chrome::min_block_size(self.cell.size), |b| {
                b.min_size(self.cell.size)
            });
        self.drag = Gesture::resize_block(&self.space.canvas, id, p, min).map(Drag::Canvas);
    }

    fn drag_to(&mut self, pos: PixelPoint<Pixels>, shift: bool, cx: &mut Context<Self>) {
        let p = self.local(pos);
        match &mut self.drag {
            Some(Drag::Canvas(gesture)) => {
                let update = gesture.update(&mut self.space.canvas, p);
                self.space.guides = update.guides;
                if let Some((id, rect)) = update.resized {
                    let (cols, rows) =
                        chrome::grid_size(chrome::content_rect(rect).size(), self.cell.size);
                    if let Some(pane) = self.space.terminal_mut(id) {
                        pane.term.resize(cols, rows, self.cell.px());
                    }
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
            None => return,
        }
        cx.notify();
    }

    /// The terminal cell under a window position: (row, col, side), clamped by the terminal.
    fn cell_under(&self, id: BlockId, pos: PixelPoint<Pixels>) -> Option<(usize, usize, Side)> {
        let block = self.space.canvas.get(id)?;
        let world = self.space.canvas.camera.to_world(self.local(pos));
        let (row, col, right) = chrome::cell_at(block.rect, world, self.cell.size);
        let side = if right { Side::Right } else { Side::Left };
        Some((row, col, side))
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

    pub(crate) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let text = self
            .space
            .focused_terminal()
            .and_then(|pane| pane.term.shared().selection_text());
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

    fn end_drag(&mut self) {
        self.drag = None;
        self.press = None;
        self.space.guides.clear();
    }

    /// Mouse up: a press on the empty canvas that didn't pan is a click, and leaves no terminal
    /// focused (typing then reaches none; panning keeps the focus).
    fn release(&mut self, pos: PixelPoint<Pixels>, cx: &mut Context<Self>) {
        if let Some(start) = self.press {
            let p = self.local(pos);
            let moved = (p.x - start.x).abs().max((p.y - start.y).abs());
            if moved < CLICK_SLOP {
                self.unfocused = self.space.focused.take();
                if self.unfocused.is_some() {
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

    fn run(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::NewTerminal => self.new_terminal(window, cx),
            Command::CloseTerminal => self.close_terminal(window, cx),
            Command::CloseTab => self.close_active_tab(window, cx),
            Command::Save => self.save_active(window, cx),
            Command::GoToLine => self.go_to_line(window, cx),
            Command::Copy => self.copy_selection(cx),
            Command::Paste => self.paste_clipboard(cx),
            Command::PreviousPrompt => self.jump_to_prompt(true, cx),
            Command::NextPrompt => self.jump_to_prompt(false, cx),
            Command::ZoomIn => self.zoom_in(window, cx),
            Command::ZoomOut => self.zoom_out(window, cx),
            Command::ZoomReset => self.zoom_reset(window, cx),
            Command::FitAll => self.fit_all(window, cx),
            Command::ThemePicker => self.toggle_theme_picker(cx),
            Command::OpenConfig => self.open_config(window, cx),
            Command::Settings => self.toggle_settings(window, cx),
            Command::NewKanban => self.new_kanban(window, cx),
            Command::NewNote => self.new_note(window, cx),
            Command::NewEditor => self.new_editor(window, cx),
            Command::NewGit => self.new_git(window, cx),
            Command::NextWorkspace => self.cycle_space(1, window, cx),
            Command::PreviousWorkspace => self.cycle_space(-1, window, cx),
            Command::NewWorkspace => self.new_space(window, cx),
            Command::ToggleSidebar => self.toggle_sidebar(window, cx),
            Command::Quit => self.quit(window, cx),
        }
    }

    /// Scrolls the focused terminal to its previous or next prompt (shell integration marks them).
    fn jump_to_prompt(&mut self, up: bool, cx: &mut Context<Self>) {
        let pane = self.space.focused_terminal();
        if pane.is_some_and(|pane| pane.term.jump_to_prompt(up)) {
            cx.notify();
        }
    }

    /// Quitting closes every terminal, and Q sits next to W (close one terminal): the first
    /// press only says to press again. Pending settings are saved on the way out.
    fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    /// The empty canvas: the name, a line about it, and how to start, with the shortcuts in
    /// effect.
    fn hint(&self) -> impl IntoElement {
        let ui = self.ui;
        let pill = move |text: &'static str| settings::chip(div(), ui).child(text);
        let row = |keys: AnyElement, label: &'static str| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.0))
                .w(px(280.0))
                .child(div().text_color(ui.title).child(label))
                .child(keys)
        };
        let [new, settings] = hint_keys(&self.keymap);
        let keys = |stroke: Option<Keystroke>| match stroke {
            Some(stroke) => settings::chip(Kbd::new(stroke), ui).into_any_element(),
            None => pill("No shortcut").into_any_element(),
        };
        div()
            .id("empty-hint")
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(28.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(crate::grid::hsla(self.theme.palette.foreground, 1.0))
                    .child("Templum"),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(ui.hint)
                    .child("An infinite canvas of terminals"),
            )
            .child(
                div()
                    .mt(px(20.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .text_size(px(13.0))
                    .child(row(keys(new), "New terminal"))
                    .child(row(
                        pill("Double-click").into_any_element(),
                        "Open one where you click",
                    ))
                    .child(row(
                        pill(if cfg!(target_os = "macos") {
                            "Hold Option"
                        } else {
                            "Hold Alt"
                        })
                        .into_any_element(),
                        "Move the canvas over blocks",
                    ))
                    .child(row(keys(settings), "Settings")),
            )
            .test_support()
    }

    fn on_key_down(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // The cursor shows again on any key.
        self.blink_on = true;
        let command = effective(
            self.keymap.command(&e.keystroke),
            self.focused_editor().is_some(),
        );
        if self.picker.is_some() || command.is_some() {
            // While the picker is open, typing and shortcuts don't reach the terminal.
            self.command_key(e, command, window, cx);
            cx.stop_propagation();
            return;
        }
        // Esc puts a tool that stays on back to Select, closes the shapes pill and deselects.
        if e.keystroke.key == "escape"
            && !e.keystroke.modifiers.modified()
            && (self.tool != Tool::Select || self.shapes_open || self.space.selected_ink.is_some())
        {
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
        let input = KeyInput {
            key: &k.key,
            key_char: k.key_char.as_deref(),
            ctrl: k.modifiers.control,
            alt: k.modifiers.alt
                && !option_types_text(&k.key, k.key_char.as_deref(), cfg!(target_os = "macos")),
            shift: k.modifiers.shift,
        };
        if pane.term.key(&input) {
            // Keeps the platform from also feeding key_char to the text-input path.
            cx.stop_propagation();
        }
    }

    /// The theme picker's keys while it is open, else `command`.
    fn command_key(
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

    fn on_background_down(
        &mut self,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.space.pristine = false;
        let p = self.local(e.position);
        // Drawings are hit only here, where no block is (blocks stop the press).
        let world = self.space.canvas.camera.to_world(p);
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

    fn on_background_scroll(
        &mut self,
        e: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            if let Some(pane) = self.space.terminal(id) {
                pane.term.scroll(lines as i32);
            }
            cx.notify();
        }
    }
}

/// On macOS, Option+key types the layout's character (`@` on German, `~`...), like Terminal.app.
/// Keys that produce no printable text (Option+arrows, Option+Backspace) stay Meta.
fn option_types_text(key: &str, key_char: Option<&str>, macos: bool) -> bool {
    macos && key_char.is_some_and(|c| c != key && !c.is_empty() && !c.chars().any(char::is_control))
}

/// "No theme named X" for each configured name that is neither built in nor in the themes
/// folder.
fn missing_themes(themes: &[Entry], config: &ThemeConfig) -> Vec<String> {
    [
        (config.light.as_str(), theme::DEFAULT_LIGHT),
        (config.dark.as_str(), theme::DEFAULT_DARK),
    ]
    .into_iter()
    .filter(|(name, _)| theme::find(themes, name).is_none())
    .map(|(name, default)| format!("No theme named \"{name}\"; using \"{default}\""))
    .collect()
}

/// Shows a notification at the bottom right, clear of the title bar's buttons. Deferred: at
/// startup the window's notification layer doesn't exist yet.
pub(crate) fn show_notification(window: &mut Window, cx: &mut App, notification: Notification) {
    let notification = notification
        .placement(Anchor::BottomRight)
        .rounded(px(10.0));
    window.defer(cx, move |window, cx| {
        window.push_notification(notification, cx)
    });
}

/// What a failed command's notification says: the line that says why, and when git would
/// have asked for a key or a password, that it needs one (spec §8, Credentials).
pub(crate) fn git_error_message(err: &GitError) -> String {
    let why = err.first_line().to_string();
    if err.needs_input() {
        format!("{why} Git needs your key or password: run it in a terminal.")
    } else {
        why
    }
}

/// `folder` becomes the newest recent folder: once, and only the RECENT newest are kept.
pub(crate) fn remember(recent: &mut Vec<PathBuf>, folder: PathBuf) {
    recent.retain(|f| *f != folder);
    recent.insert(0, folder);
    recent.truncate(RECENT);
}

/// Ctrl+Z / Ctrl+Shift+Z (Cmd on macOS): Some(false) undoes, Some(true) redoes.
/// The command a key press runs: one that only works in an editor is nothing without one, so
/// the key goes on to the terminal (Ctrl+S is XOFF, Ctrl+W erases a word).
fn effective(command: Option<Command>, in_editor: bool) -> Option<Command> {
    command.filter(|c| in_editor || !c.editor_only())
}

fn undo_key(k: &Keystroke, macos: bool) -> Option<bool> {
    let m = &k.modifiers;
    let held = if macos {
        m.platform && !m.control
    } else {
        m.control && !m.platform
    };
    if !held || m.alt || m.function || k.key != "z" {
        return None;
    }
    Some(m.shift)
}

/// Ctrl+1 to Ctrl+9 (Cmd on macOS), with no other modifier: the position of a workspace.
fn workspace_digit(k: &Keystroke, macos: bool) -> Option<usize> {
    let m = &k.modifiers;
    let held = if macos {
        m.platform && !m.control
    } else {
        m.control && !m.platform
    };
    if !held || m.alt || m.shift || m.function {
        return None;
    }
    let digit: usize = k.key.parse().ok()?;
    (1..=9).contains(&digit).then(|| digit - 1)
}

/// The first shortcut of the commands the empty canvas shows: new terminal, then settings.
fn hint_keys(keymap: &Keymap) -> [Option<Keystroke>; 2] {
    [Command::NewTerminal, Command::Settings].map(|c| keymap.keystrokes(c).into_iter().next())
}

/// The dot grid under the blocks: round dots centered on the world lattice.
fn paint_dots(camera: &Camera, color: Hsla, bounds: Bounds<Pixels>, window: &mut Window) {
    let grid = dot_grid(camera);
    let step = grid.spacing * camera.zoom();
    let d = dot_size(camera.zoom());
    let start = camera.to_screen(grid.first);
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let mut y = start.y;
    while y < h + d {
        let mut x = start.x;
        while x < w + d {
            let at = point(
                bounds.origin.x + px(x - d / 2.0),
                bounds.origin.y + px(y - d / 2.0),
            );
            window.paint_quad(
                fill(Bounds::new(at, size(px(d), px(d))), color).corner_radii(px(d / 2.0)),
            );
            x += step;
        }
        y += step;
    }
}

/// The snap guides: 1 px lines in the accent color, world units to screen.
fn paint_guides(
    camera: &Camera,
    color: Hsla,
    bounds: Bounds<Pixels>,
    guides: &[Guide],
    window: &mut Window,
) {
    for guide in guides {
        let (a, b) = match *guide {
            Guide::Vertical { x, y0, y1 } => (Point::new(x, y0), Point::new(x, y1)),
            Guide::Horizontal { y, x0, x1 } => (Point::new(x0, y), Point::new(x1, y)),
        };
        let (a, b) = (camera.to_screen(a), camera.to_screen(b));
        let rect = Bounds::new(
            point(bounds.origin.x + px(a.x), bounds.origin.y + px(a.y)),
            size(px((b.x - a.x).max(1.0)), px((b.y - a.y).max(1.0))),
        );
        window.paint_quad(fill(rect, color));
    }
}

fn is_dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(mut hook) = self.on_frame.take()
            && hook(self, window, cx)
        {
            self.on_frame = Some(hook);
        }
        // Settings changes apply here, where the window is at hand; the save waits for them to settle.
        if let Some(config) = self.pending.take() {
            self.apply_config(config, window, cx);
            self._save = cx.spawn_in(window, async move |ws, cx| {
                cx.background_executor().timer(SAVE_DELAY).await;
                ws.update_in(cx, |ws, window, cx| ws.save_or_notify(window, cx))
                    .ok();
            });
        }
        let fade = transition_with_status(
            "theme-ui",
            self.theme.ui,
            Transition::new(motion::THEME),
            window,
            cx,
        );
        // Blending ends a rounding error away from the theme: take the theme's own colors.
        self.ui = match fade.status {
            MotionStatus::Running | MotionStatus::Delayed => fade.value,
            MotionStatus::Idle | MotionStatus::Finished => self.theme.ui,
        };
        // The focused terminal's painting sets it again this frame, if its cursor blinks.
        self.cursor_blinks = false;
        let exited: Vec<BlockId> = self
            .space
            .terminals()
            .filter(|(_, pane)| pane.term.exited())
            .map(|(id, _)| id)
            .collect();
        for id in exited {
            self.close(id, window, cx);
        }
        // Re-arm once per frame: output is coalesced to frame rate.
        for (_, pane) in self.space.terminals() {
            pane.term.rearm();
        }
        self.hear(window, cx);

        let vp = self.viewport(window);
        if self.space.pristine
            && let Some(b) = self.space.canvas.bounds()
        {
            let z = self.space.canvas.camera.zoom();
            self.space.canvas.camera.origin = Point::new(
                b.x + b.w / 2.0 - vp.w / 2.0 / z,
                b.y + b.h / 2.0 - vp.h / 2.0 / z,
            );
        }
        // Animations: drop the finished ones, and keep frames coming while any runs.
        let now = Instant::now();
        self.space
            .appearing
            .retain(|_, since| motion::progress(*since, now, motion::OPEN) < 1.0);
        self.space
            .ghosts
            .retain(|g| motion::progress(g.since, now, motion::CLOSE) < 1.0);
        if let Some(panel) = &self.settings {
            let t = motion::progress(panel.since, now, motion::SLIDE);
            if t < 1.0 {
                window.request_animation_frame();
            } else if panel.closing {
                self.settings = None;
            }
        }
        if !self.space.appearing.is_empty() || !self.space.ghosts.is_empty() {
            window.request_animation_frame();
        }
        // The view glides to the camera. Anything that moved the camera itself (a drag, the
        // startup centering) ends the flight: the view is then the camera.
        self.space.view = match self.space.flight {
            Some((flight, since)) if flight.to == self.space.canvas.camera => {
                let t = motion::progress(since, now, motion::FLIGHT);
                if t < 1.0 {
                    window.request_animation_frame();
                    flight.at(motion::slide(t))
                } else {
                    self.space.flight = None;
                    self.space.canvas.camera
                }
            }
            _ => {
                self.space.flight = None;
                self.space.canvas.camera
            }
        };
        let visible: Vec<_> = self
            .space
            .canvas
            .visible_from(&self.space.view, vp)
            .copied()
            .collect();
        let mut blocks: Vec<_> = visible
            .into_iter()
            .map(|b| {
                let look = self
                    .space
                    .appearing
                    .get(&b.id)
                    .map_or(motion::Look::REST, |since| {
                        motion::opening(motion::progress(*since, now, motion::OPEN))
                    });
                let body = &self.space.blocks[&b.id];
                block::render(self, b, body, look, true, window, cx)
            })
            .collect();
        // Ghosts fade above the blocks, where they were.
        blocks.extend(self.space.ghosts.iter().map(|g| {
            let look = motion::closing(motion::progress(g.since, now, motion::CLOSE));
            block::render(self, g.block, &g.body, look, false, window, cx)
        }));

        let entity = cx.entity();
        let focus = self.focus_handle.clone();
        let camera = self.space.view;
        let dot = self.ui.dot;
        let guides = self.space.guides.clone();
        let accent = self.ui.accent;
        // A full-size layer under the blocks: the IME handler, the workspace origin, and
        // window-wide drag listeners (drags keep working outside the window).
        let layer = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                paint_dots(&camera, dot, bounds, window);
                paint_guides(&camera, accent, bounds, &guides, window);
                window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx);
                entity.update(cx, |ws, _| ws.origin = bounds.origin);
                let on_move = entity.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        on_move.update(cx, |ws, cx| {
                            // Alt let go while the window wasn't listening (another window had
                            // the keyboard): the next move puts the hand right.
                            if e.modifiers.alt != ws.hand {
                                cx.notify();
                            }
                            // Any button: the middle one pans too.
                            if e.pressed_button.is_some() {
                                ws.drag_to(e.position, e.modifiers.shift, cx);
                            } else {
                                ws.end_drag();
                            }
                        });
                    }
                });
                let on_up = entity.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        on_up.update(cx, |ws, cx| match e.button {
                            MouseButton::Left => ws.release(e.position, cx),
                            MouseButton::Middle => ws.end_drag(),
                            _ => {}
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full();

        self.hand = window.modifiers().alt;
        let hand = self.hand || self.tool == Tool::Hand;
        let panning = matches!(self.drag, Some(Drag::Canvas(Gesture::Pan { .. })));
        let area = div()
            .id("workspace")
            .relative()
            .flex_1()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_background_down))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(Self::on_background_right_down),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|ws, e: &MouseDownEvent, _, _| ws.begin_pan(e.position)),
            )
            .on_scroll_wheel(cx.listener(Self::on_background_scroll))
            .on_pinch(cx.listener(Self::on_pinch))
            .child(layer)
            .children(blocks)
            // The drawings, above the blocks; with a drawing tool on, a layer over them takes the
            // pointer.
            .child(self.ink_layer())
            .when(self.tool.draws(), |el| {
                el.child(
                    div()
                        .id("ink-layer")
                        .absolute()
                        .size_full()
                        .cursor_crosshair()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                ws.begin_ink(e);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .on_scroll_wheel(cx.listener(Self::on_background_scroll))
                        .on_pinch(cx.listener(Self::on_pinch))
                        .test_support(),
                )
            })
            // The hand: while Alt is held, a layer over the blocks takes the pointer, and a drag
            // or the wheel moves the canvas wherever it is.
            .when(hand, |el| {
                el.child(
                    div()
                        .id("hand")
                        .absolute()
                        .size_full()
                        .map(|el| {
                            if panning {
                                el.cursor_grabbing()
                            } else {
                                el.cursor_grab()
                            }
                        })
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                ws.begin_pan(e.position);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .on_scroll_wheel(cx.listener(Self::on_background_scroll))
                        .on_pinch(cx.listener(Self::on_pinch))
                        .test_support(),
                )
            })
            .when(self.space.canvas.blocks().is_empty(), |el| {
                el.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(self.hint()),
                )
            })
            .child(toolbar::render(self, cx));
        let picker = self
            .picker
            .as_ref()
            .map(|p| theme_picker::render(self, p, cx));
        if self.recording.is_some() && !self.settings_focus.is_focused(window) {
            self.recording = None;
            self.key_note = None;
        }
        let panel = self
            .settings
            .as_ref()
            .map(|p| settings::render(self, p, now, vp.w, window, cx));
        div()
            .id("window")
            // Alt held or let go: the hand comes or goes.
            .on_modifiers_changed(cx.listener(|_, _: &ModifiersChangedEvent, _, cx| cx.notify()))
            .size_full()
            .flex()
            .flex_col()
            // The settings panel slides in from beyond the right edge.
            .overflow_hidden()
            .bg(self.ui.canvas)
            .child(title_bar::render(self, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .when(self.sidebar, |el| el.child(sidebar::render(self, cx)))
                    .child(area),
            )
            .children(panel)
            .children(picker)
            .children(self.menu.as_ref().map(|m| menu::render(m, window)))
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

#[cfg(test)]
mod tests;
