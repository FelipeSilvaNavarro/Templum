//! The main view: the canvas of terminal blocks.

mod blocks;
mod drawing;
mod file_tree;
mod files;
mod find;
mod git_branches;
mod git_changes;
mod git_discard;
mod git_history;
mod git_keys;
mod git_message;
mod git_ops;
mod input;
mod persist;
mod preferences;
mod render;
mod spaces;
mod view;

pub(crate) use find::FindBar;
pub(crate) use git_message::TryIt;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, Bounds, Context, FocusHandle, Focusable as _, KeyBinding, Keystroke, Pixels,
    Point as PixelPoint, SharedString, Subscription, Task, Window, WindowAppearance, px,
};
use templum_canvas::{
    Block, BlockId, Camera, CameraFlight, Canvas, Color, Drawing, Gesture, Guide, Point, ShapeKind,
    Size, chrome,
};
use templum_git::GitError;
use templum_terminal::{Notify, Snapshot, Terminal};

use crate::agents::{self, AgentEntry};
use crate::alert::Alert;
use crate::board::Boards;
use crate::config::{self, Config, Edit, ThemeConfig};
use crate::editor::{self, Editor};
use crate::git_block::{self, Git};
use crate::grid::CellMetrics;
use crate::ink;
use crate::kanban::{self, Kanban};
use crate::keys::{Command, Keymap};
use crate::menu;
use crate::note::{self, Note};
use crate::session;
use crate::settings::Panel;
use crate::theme::{self, Entry, Theme, Ui};
use crate::theme_picker::Picker;

pub(crate) use git_ops::{Extra, notify_git_error, notify_git_failure, notify_with};

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
/// Board changes kept for undo, for the whole app (the boards are).
pub(crate) const BOARD_UNDO: usize = 50;
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
    /// Its folder and its agent as last read: at most a snapshot old (5 s), cheap to ask every
    /// frame. The folder is the start folder until then, and follows each finished command.
    pub seen: templum_terminal::Inspection,
    /// Its find field, while open.
    pub find: Option<Box<find::FindBar>>,
}

impl Pane {
    /// Its folder, as last seen, is `root` or inside it.
    pub fn works_in(&self, root: &std::path::Path) -> bool {
        self.seen
            .folder
            .as_ref()
            .is_some_and(|f| f.starts_with(root))
    }
}

/// A Git block of any workspace: block ids count from 0 in every one, so a result that comes
/// back after a switch still finds its block.
#[derive(Clone, Debug)]
pub(crate) struct GitAt {
    pub space: String,
    pub block: BlockId,
    /// The block's `Git::generation` when the command started: the result of one started
    /// before the block changed folder is dropped.
    pub generation: u64,
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
            // A field (the summary, a filter) keeps the keyboard it just took on the click;
            // otherwise the block takes it.
            Body::Git(g) => {
                if !g.focus.contains_focused(window, cx) {
                    g.focus.focus(window, cx);
                }
                true
            }
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
            Body::Git(g) => g.focus.contains_focused(window, cx),
            Body::Terminal(pane) => pane.find.as_ref().is_some_and(|b| b.has_focus(window, cx)),
            Body::Kanban(_) => false,
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
                            color: n.color,
                        }))
                    }
                    Body::Editor(e) => {
                        session::LayoutBlock::Saved(session::Item::Editor(e.item(b.rect)))
                    }
                    Body::Git(g) => {
                        session::LayoutBlock::Saved(session::Item::Git(g.item(b.rect, cx)))
                    }
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

    /// The terminal blocks back to front: the order of a snapshot's terminal items.
    fn terminal_order(&self) -> Vec<BlockId> {
        self.canvas
            .blocks()
            .iter()
            .filter(|b| self.terminal(b.id).is_some())
            .map(|b| b.id)
            .collect()
    }

    /// Tells the terminals `ids` (a snapshot's `terminal_order`) what `saved` found in them.
    fn see(&mut self, ids: &[BlockId], saved: &session::Saved) {
        let found = saved.items.iter().filter_map(|item| match item {
            session::Item::Terminal(t) => Some(t),
            _ => None,
        });
        for (id, t) in ids.iter().zip(found) {
            if let Some(pane) = self.terminal_mut(*id) {
                // A folder the snapshot couldn't read (or write as JSON) keeps the one known.
                let folder = t.folder.as_ref().map(PathBuf::from);
                pane.seen.folder = folder.or(pane.seen.folder.take());
                pane.seen.agent.clone_from(&t.agent);
            }
        }
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
    /// `boards` before each change, newest last, at most BOARD_UNDO (see `edit_boards`).
    boards_undo: Vec<Boards>,
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
    /// What discards keep (`trash/` in the data folder); None without one, as in `--bench`.
    pub(crate) trash: Option<templum_git::trash::Trash>,
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
    /// The agent Generate ✨ runs, and whether it is ready; read when the config changes, not
    /// each frame (finding a program looks in many folders).
    // ponytail: an agent installed while Templum runs shows its ✨ after the next config change
    // or a restart; look again when the settings panel opens if someone waits for it.
    pub(crate) commit_agent: Option<agents::CommitAgent>,
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
    /// Settings ▸ Git ▸ Try it, running or done.
    pub(crate) message_try: Option<TryIt>,
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
        let effective_agents = agents::effective(&config.agents, &agents::codex_models());
        let commit_agent = agents::commit_agent(
            &effective_agents,
            &config.git.commit_message,
            templum_terminal::installed,
        );
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
            boards_undo: Vec::new(),
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
            trash: data_dir
                .as_ref()
                .map(|dir| templum_git::trash::Trash::new(dir.join("trash"))),
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
            message_try: None,
            config,
            keymap,
            agents: effective_agents,
            commit_agent,
            _wake: wake,
        };
        // The trash's old discards go at startup (and after each discard).
        if let Some(trash) = ws.trash.clone() {
            cx.background_executor()
                .spawn(async move {
                    let _ = trash.prune(std::time::SystemTime::now());
                })
                .detach();
        }
        ws.load_spaces(window, cx);
        ws
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

fn is_dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

#[cfg(test)]
mod tests;
