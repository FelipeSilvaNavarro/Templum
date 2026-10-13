//! Right-click menus: one at a time, built for what was right-clicked, drawn at the pointer.
//! Each right-clickable element opens its target's menu and stops the event, so the innermost
//! one wins. See docs/specs/context-menus.md.

mod build;

use std::path::PathBuf;

use crate::agents::AgentEntry;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, DismissEvent, Entity, Focusable as _, Pixels, Point as PixelPoint,
    ScrollWheelEvent, Subscription, Window, anchored, deferred, div, px,
};
use templum_canvas::{BlockId, Point};
use templum_git::{Branch, Commit, Entry};

use crate::git_block;
use crate::workspace::{Body, Workspace};

/// What was right-clicked.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// The empty canvas, at a world point: blocks are added there.
    Canvas(Point),
    /// A block, whatever it holds: the menu is built for its kind.
    Block(BlockId),
    /// The AI agents alone (the toolbar): the chosen one opens at this world point.
    Agents(Point),
    /// A row of a kanban block's board list.
    Board {
        block: BlockId,
        board: u64,
    },
    Column {
        board: u64,
        column: u64,
    },
    Card {
        board: u64,
        card: u64,
    },
    /// A workspace's tile in the sidebar, by its id.
    Space(String),
    /// A drawing, by its index in the space's ink.
    Ink(usize),
    /// A stash in a Git block's Stashes tab, by N in `stash@{N}`.
    GitStash {
        block: BlockId,
        index: usize,
    },
    /// A branch in a Git block's Branches tab, by its name.
    GitBranch {
        block: BlockId,
        name: String,
    },
    /// A commit in a Git block's History, by its hash.
    GitCommit {
        block: BlockId,
        hash: String,
    },
    /// A row of a Git block's Changes lists, by its list and its index in it.
    GitRow {
        block: BlockId,
        list: git_block::List,
        index: usize,
    },
    /// The terminals working in a Git block's repository (its top bar).
    GitTerminals(BlockId),
    /// A row of an editor's file tree (a file or a folder), or its empty space (None).
    Tree {
        block: BlockId,
        path: Option<PathBuf>,
    },
}

/// The open menu.
pub(crate) struct OpenMenu {
    view: Entity<PopupMenu>,
    /// Window position of the right-click.
    at: PixelPoint<Pixels>,
    _dismissed: Subscription,
}

impl Workspace {
    /// Opens `target`'s menu at `at`, replacing any open one.
    pub(crate) fn open_menu(
        &mut self,
        target: Target,
        at: PixelPoint<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity();
        // Esc or a choice gives the keyboard back to what had it.
        let back = window
            .focused(cx)
            .unwrap_or_else(|| self.focus_handle.clone());
        let Some(items) = self.items(&target, cx) else {
            return;
        };
        let view = PopupMenu::build(window, cx, move |menu, window, cx| {
            items.build(&ws, menu.action_context(back), window, cx)
        });
        let _dismissed = cx.subscribe_in(&view, window, |ws, view, _: &DismissEvent, _, cx| {
            if ws.menu.as_ref().is_some_and(|m| m.view == *view) {
                ws.menu = None;
                cx.notify();
            }
        });
        view.focus_handle(cx).focus(window, cx);
        self.menu = Some(OpenMenu {
            view,
            at,
            _dismissed,
        });
        cx.notify();
    }

    /// The agents the menus list, and whether each one's program was found.
    fn agents_for_menu(&self) -> Vec<(AgentEntry, bool)> {
        self.agents
            .iter()
            .filter(|a| a.show)
            .map(|a| (a.clone(), templum_terminal::installed(a.program())))
            .collect()
    }

    /// A menu's Send to: `text`, and the terminals it can go to.
    fn send_to_menu(&self, text: String) -> SendTo {
        SendTo {
            text,
            to: self.terminal_list(None),
        }
    }

    /// What the menu for `target` needs to know, read now: building it can't read the workspace.
    fn items(&self, target: &Target, cx: &App) -> Option<Items> {
        Some(match *target {
            Target::Canvas(at) => Items::Canvas {
                at,
                agents: self.agents_for_menu(),
            },
            Target::Agents(at) => Items::Agents {
                at,
                agents: self.agents_for_menu(),
            },
            Target::Block(block) => match self.space.blocks.get(&block)? {
                Body::Terminal(pane) => Items::Terminal {
                    block,
                    selection: pane.term.shared().selection_text().is_some(),
                },
                Body::Kanban(k) => Items::Kanban {
                    block,
                    listing: k.shown(self).is_none() && k.board.is_some(),
                },
                Body::Note(n) => Items::Note {
                    block,
                    preview: n.preview,
                    send: self.send_to_menu(n.to_send(cx)),
                },
                Body::Editor(e) => Items::Editor {
                    block,
                    tree: e.show_tree,
                    wrap: e.wrap,
                    send: self.send_to_menu(e.to_send(cx).unwrap_or_default()),
                },
                body @ Body::Git(_) => Items::Git {
                    block,
                    folder: body.folder(),
                },
            },
            Target::Board { block, board } => Items::Board {
                block,
                board,
                name: self
                    .boards
                    .board(board)
                    .map_or_else(String::new, |b| b.name.clone()),
                cards: self.boards.board(board).map_or(0, |b| b.card_count()),
            },
            Target::Column { board, column } => {
                let found = self
                    .boards
                    .board(board)
                    .and_then(|b| b.columns.iter().find(|c| c.id == column));
                Items::Column {
                    board,
                    column,
                    title: found.map_or_else(String::new, |c| c.title.clone()),
                    cards: found.map_or(0, |c| c.cards.len()),
                }
            }
            Target::Card { board, card } => Items::Card {
                board,
                card,
                send: self.send_to_menu(
                    self.boards
                        .card(board, card)
                        .map_or_else(String::new, |c| c.text()),
                ),
            },
            Target::Ink(index) => Items::Ink { index },
            Target::GitRow { block, list, index } => {
                let git = self.space.git(block)?;
                let entries = match list {
                    git_block::List::Conflicts => git.status.conflicts(),
                    git_block::List::Staged => git.status.staged(),
                    git_block::List::Unstaged => git.status.unstaged(),
                };
                Items::GitRow {
                    block,
                    list,
                    entry: (*entries.get(index)?).clone(),
                }
            }
            Target::GitStash { block, index } => {
                self.space
                    .git(block)?
                    .stashes
                    .iter()
                    .find(|s| s.index == index)?;
                Items::GitStash { block, index }
            }
            Target::GitBranch { block, ref name } => {
                let git = self.space.git(block)?;
                Items::GitBranch {
                    block,
                    branch: git.branches.iter().find(|b| b.name == *name)?.clone(),
                    current: git.status.branch.clone(),
                }
            }
            Target::GitCommit { block, ref hash } => Items::GitCommit {
                block,
                commit: self
                    .space
                    .git(block)?
                    .commits
                    .iter()
                    .find(|c| c.hash == *hash)?
                    .clone(),
            },
            Target::GitTerminals(block) => Items::Reveal {
                terminals: self.terminal_list(Some(&self.space.git(block)?.repo()?.root)),
            },
            Target::Tree { block, ref path } => {
                let is_dir = path.as_ref().is_some_and(|p| p.is_dir());
                Items::Tree {
                    block,
                    folder: self.space.editor(block)?.folder.clone(),
                    path: path.clone(),
                    is_dir,
                    entries: path
                        .as_ref()
                        .filter(|_| is_dir)
                        .map_or(0, |p| std::fs::read_dir(p).map_or(0, Iterator::count)),
                }
            }
            Target::Space(ref id) => Items::Space {
                id: id.clone(),
                name: self
                    .spaces()
                    .find(|s| s.id == *id)
                    .map_or_else(String::new, |s| s.name.clone()),
                only_one: self.others.is_empty(),
                unsaved: self
                    .spaces()
                    .find(|s| s.id == *id)
                    .map_or_else(Vec::new, |s| s.unsaved(None)),
            },
        })
    }
}

/// A Send to submenu: the text, and the terminals listed (see `Workspace::terminal_list`).
struct SendTo {
    text: String,
    to: Vec<(BlockId, String)>,
}

/// A target's menu, with what it shows read from the workspace.
enum Items {
    Canvas {
        at: Point,
        /// Each agent the menu lists, and whether its program was found.
        agents: Vec<(AgentEntry, bool)>,
    },
    Agents {
        at: Point,
        agents: Vec<(AgentEntry, bool)>,
    },
    Terminal {
        block: BlockId,
        selection: bool,
    },
    Kanban {
        block: BlockId,
        /// The board list shows over a board the block has.
        listing: bool,
    },
    Note {
        block: BlockId,
        /// It shows rendered.
        preview: bool,
        send: SendTo,
    },
    Editor {
        block: BlockId,
        tree: bool,
        wrap: bool,
        send: SendTo,
    },
    Git {
        block: BlockId,
        /// Its repository's root (its folder while there is none).
        folder: Option<PathBuf>,
    },
    GitRow {
        block: BlockId,
        list: git_block::List,
        entry: Entry,
    },
    GitCommit {
        block: BlockId,
        commit: Commit,
    },
    GitBranch {
        block: BlockId,
        branch: Branch,
        current: Option<String>,
    },
    GitStash {
        block: BlockId,
        index: usize,
    },
    Board {
        block: BlockId,
        board: u64,
        name: String,
        cards: usize,
    },
    Column {
        board: u64,
        column: u64,
        title: String,
        cards: usize,
    },
    Card {
        board: u64,
        card: u64,
        send: SendTo,
    },
    Space {
        id: String,
        name: String,
        only_one: bool,
        /// Its editors' changed files.
        unsaved: Vec<String>,
    },
    Ink {
        index: usize,
    },
    /// Terminals to show, one item each.
    Reveal {
        terminals: Vec<(BlockId, String)>,
    },
    Tree {
        block: BlockId,
        /// The editor's folder: where New file… goes from the empty space.
        folder: PathBuf,
        path: Option<PathBuf>,
        is_dir: bool,
        /// A folder's entries, for what Delete… says.
        entries: usize,
    },
}

/// The open menu, above everything, kept inside the window; the wheel doesn't reach what is
/// under it while it is open.
pub(crate) fn render(menu: &OpenMenu, window: &Window) -> AnyElement {
    let size = window.bounds().size;
    deferred(
        anchored().child(
            div()
                .w(size.width)
                .h(size.height)
                .on_scroll_wheel(|_: &ScrollWheelEvent, _, cx| cx.stop_propagation())
                .child(
                    anchored()
                        .position(menu.at)
                        .snap_to_window_with_margin(px(8.0))
                        .child(menu.view.clone()),
                ),
        ),
    )
    .with_priority(gpui_kit::base::POPUP_PRIORITY)
    .into_any_element()
}
