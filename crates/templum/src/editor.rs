//! An editor block: a file tree, tabs of open files and gpui-kit's code editor, on one folder.
//! See docs/specs/cockpit.md, section 7.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use gpui_kit::Focusable as _;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Tree, TreeEntry, TreeEntryState, TreeEvent, TreeItem, TreeState};
use gpui_kit::component::Icon;
use gpui_kit::component::highlighter::Language;
use gpui_kit::component::input::{Editor as EditorView, EditorState, InputEvent};
use gpui_kit::component::text::TextView;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, Entity, MouseButton, ScrollWheelEvent,
    SharedString, Stateful, StyleRefinement, Subscription, TestSupportExt, Window, div, px,
    relative,
};
use templum_canvas::{BlockId, Rect, Size};

use crate::session;
use crate::theme::Ui;
use crate::workspace::Workspace;

/// A new editor's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size { w: 900.0, h: 600.0 };
pub(crate) const MIN: Size = Size { w: 480.0, h: 320.0 };
/// The tree's width and its rows' height, world units.
const TREE_W: f32 = 220.0;
const ROW_H: f32 = 22.0;

/// The text's size at zoom 1, world units.
pub(crate) const FONT: f32 = 13.0;
/// The tab row's height, world units.
const TAB_H: f32 = 28.0;
/// A NUL in the first 8 KB: not text.
pub(crate) const BINARY_PROBE: usize = 8 * 1024;
/// Over this, a file opens read-only; over `TOO_BIG_OVER`, it doesn't open.
pub(crate) const READ_ONLY_OVER: u64 = 4 * 1024 * 1024;
pub(crate) const TOO_BIG_OVER: u64 = 32 * 1024 * 1024;

/// The language gpui-kit highlights `path` with: by its extension, by its name for files like
/// Makefile, "text" when it has no grammar.
pub(crate) fn lang_of(path: &Path) -> &'static str {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match (name, ext.as_str()) {
        ("Makefile" | "makefile" | "GNUmakefile", _) | (_, "mk") => "make",
        (_, "h" | "hpp" | "hh" | "cc" | "cxx") => "cpp",
        (_, "mjs" | "cjs" | "jsx") => "javascript",
        (_, "htm") => "html",
        (_, "zsh" | "bash") => "bash",
        (_, "patch") => "diff",
        // gpui-kit knows the common extensions ("rs", "ts", "md", "yml"…); the rest is text.
        _ => Language::from_str(&ext).name(),
    }
}

pub(crate) fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_PROBE).any(|b| *b == 0)
}

/// "\r\n" when the first line ends that way, else "\n".
pub(crate) fn line_ending(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if text[..i].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

#[allow(dead_code)] // Save, which comes next, writes with it.
/// The editor's text as the file is written: the editor works in "\n"; the file keeps its line
/// ending, and ends with a newline only if it did.
pub(crate) fn encode(edited: &str, crlf: bool, newline: bool) -> String {
    let mut out = edited.replace("\r\n", "\n");
    if newline && !out.ends_with('\n') {
        out.push('\n');
    } else if !newline && out.ends_with('\n') {
        out.pop();
    }
    if crlf { out.replace('\n', "\r\n") } else { out }
}

/// The file's modification time; None when it is gone.
pub(crate) fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// "812 B", "12.3 KB", "4.0 MB".
pub(crate) fn human(bytes: u64) -> String {
    const K: f64 = 1024.0;
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / K),
        b => format!("{:.1} MB", b as f64 / (K * K)),
    }
}

/// A folder's entry as the tree shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub name: String,
    pub path: PathBuf,
    /// A link to a folder counts as a folder.
    pub is_dir: bool,
}

/// `dir`'s entries: folders first, then files, each sorted without case. Hidden files are
/// shown. An unreadable folder is empty.
pub(crate) fn read_dir(dir: &Path) -> Vec<Entry> {
    let mut entries: Vec<Entry> = std::fs::read_dir(dir)
        .map(|read| {
            read.filter_map(|e| {
                let e = e.ok()?;
                let path = e.path();
                Some(Entry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    is_dir: path.is_dir(),
                    path,
                })
            })
            .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// What a tab holds.
pub(crate) enum Content {
    /// Text; `crlf` and `newline` are how the file ends its lines and itself; `big` is over
    /// `READ_ONLY_OVER`, read-only.
    #[allow(dead_code)] // Save, which comes next, reads `crlf` and `newline`.
    Text {
        crlf: bool,
        newline: bool,
        big: bool,
    },
    Binary {
        size: u64,
    },
}

/// One open file.
pub(crate) struct Tab {
    pub path: PathBuf,
    pub editor: Entity<EditorState>,
    pub content: Content,
    /// Edited since it was read or saved (or gone from the disk: Save brings it back).
    pub dirty: bool,
    /// The file's modification time as last read or written; None when it is gone.
    #[allow(dead_code)] // Read by the check for changes on disk, which comes next.
    pub mtime: Option<SystemTime>,
    /// Changed on disk while it had unsaved edits: the bar asks what to do.
    #[allow(dead_code)]
    pub conflict: bool,
    /// A Markdown file shown rendered.
    pub preview: bool,
    _edits: Subscription,
}

impl Tab {
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }
}

/// An editor block: one folder, the file tree over it, the open files.
pub(crate) struct Editor {
    pub folder: PathBuf,
    pub tree: Entity<TreeState>,
    /// The folders read so far, by path: the root and each one expanded. A folder that was
    /// never expanded costs nothing (`node_modules`).
    pub dirs: HashMap<PathBuf, Vec<Entry>>,
    pub expanded: HashSet<PathBuf>,
    pub show_tree: bool,
    /// Soft wrap in every tab (a toggle in the block's menu).
    pub wrap: bool,
    pub tabs: Vec<Tab>,
    pub active: Option<usize>,
    /// When the tree was last read, for the refresh while the block is focused.
    pub tree_read: Instant,
    _expansions: Subscription,
}

impl Editor {
    pub fn new(folder: PathBuf, show_tree: bool, cx: &mut Context<Workspace>) -> Editor {
        let tree = cx.new(|cx| TreeState::new(cx));
        let _expansions = cx.subscribe(&tree, |ws, tree, event: &TreeEvent, cx| {
            ws.on_tree_event(&tree, event, cx)
        });
        let mut editor = Editor {
            folder,
            tree,
            dirs: HashMap::new(),
            expanded: HashSet::new(),
            show_tree,
            wrap: false,
            tabs: Vec::new(),
            active: None,
            tree_read: Instant::now(),
            _expansions,
        };
        editor.refresh(cx);
        editor
    }

    /// Reads the root and every expanded folder again; shows them when something changed.
    pub fn refresh(&mut self, cx: &mut App) -> bool {
        let dirs: HashMap<PathBuf, Vec<Entry>> = std::iter::once(&self.folder)
            .chain(&self.expanded)
            .map(|d| (d.clone(), read_dir(d)))
            .collect();
        self.tree_read = Instant::now();
        if dirs == self.dirs {
            return false;
        }
        self.dirs = dirs;
        self.show(cx);
        true
    }

    /// Rebuilds the tree's items from `dirs`, keeping the selected row.
    pub fn show(&self, cx: &mut App) {
        let items = self.items(&self.folder);
        self.tree.update(cx, |tree, cx| {
            let selected = tree.selected_item().map(|i| i.id.clone());
            tree.set_items(items, cx);
            let ix = selected.and_then(|id| tree.index_of(&id));
            tree.set_selected_index(ix, cx);
        });
    }

    /// `dir`'s items, with each folder's items under it. An item's id is its path.
    fn items(&self, dir: &Path) -> Vec<TreeItem> {
        let Some(entries) = self.dirs.get(dir) else {
            return Vec::new();
        };
        entries
            .iter()
            .map(|e| {
                let id = e.path.to_string_lossy().into_owned();
                let item = TreeItem::new(id.clone(), e.name.clone());
                if !e.is_dir {
                    return item;
                }
                // A folder keeps a child so the tree knows it is one: its entries once read,
                // else a placeholder that can't be clicked.
                let children = match self.dirs.get(&e.path) {
                    Some(read) if !read.is_empty() => self.items(&e.path),
                    Some(_) => vec![TreeItem::new(format!("{id}/"), "empty").disabled(true)],
                    None => vec![TreeItem::new(format!("{id}/"), "…").disabled(true)],
                };
                item.children(children)
                    .expanded(self.expanded.contains(&e.path))
            })
            .collect()
    }

    pub fn folder_name(&self) -> String {
        self.folder.file_name().map_or_else(
            || self.folder.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Opens `path` in a tab (or shows its tab) and returns the tab's index; Err says why it
    /// can't be opened. The tab becomes the active one.
    pub fn open_tab(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Result<usize, String> {
        if let Some(i) = self.tabs.iter().position(|t| t.path == path) {
            self.active = Some(i);
            return Ok(i);
        }
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let size = std::fs::metadata(path)
            .map_err(|e| format!("Couldn't open {name}: {e}"))?
            .len();
        if size > TOO_BIG_OVER {
            return Err(format!("{name} is over 32 MB, so Templum doesn't open it"));
        }
        let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read {name}: {e}"))?;
        let (content, text) = if is_binary(&bytes) {
            (Content::Binary { size }, String::new())
        } else {
            let raw = String::from_utf8_lossy(&bytes);
            let content = Content::Text {
                crlf: line_ending(&raw) == "\r\n",
                newline: raw.ends_with('\n'),
                big: size > READ_ONLY_OVER,
            };
            (content, raw.replace("\r\n", "\n"))
        };
        let (lang, wrap) = (lang_of(path), self.wrap);
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(lang)
                .line_number(true)
                .soft_wrap(wrap)
                .default_value(text)
        });
        let _edits = cx.subscribe(&editor, |ws, editor, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                ws.on_edit(&editor, cx);
            }
        });
        self.tabs.push(Tab {
            path: path.to_path_buf(),
            editor,
            content,
            dirty: false,
            mtime: mtime(path),
            conflict: false,
            preview: false,
            _edits,
        });
        self.active = Some(self.tabs.len() - 1);
        Ok(self.tabs.len() - 1)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active?)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active?)
    }

    /// Gives the keyboard to the active file's editor; false when there is none to type in.
    pub fn focus(&self, window: &mut Window, cx: &mut App) -> bool {
        match self.active_tab() {
            Some(tab) if !tab.preview && matches!(tab.content, Content::Text { .. }) => {
                tab.editor.update(cx, |e, cx| e.focus(window, cx));
                true
            }
            _ => false,
        }
    }

    /// The keyboard is in one of the tabs' editors.
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        self.tabs.iter().any(|t| {
            t.editor
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        })
    }

    /// The folder's name; with a file open, "name — folder", "● " before a changed file.
    pub fn title(&self) -> SharedString {
        match self.active_tab() {
            Some(tab) => format!(
                "{}{} — {}",
                if tab.dirty { "● " } else { "" },
                tab.name(),
                self.folder_name()
            )
            .into(),
            None => self.folder_name().into(),
        }
    }

    /// As the canvas file holds it: the open files relative to the folder (one outside it is
    /// left out), and the active one's position among them.
    pub fn item(&self, rect: Rect) -> session::EditorItem {
        let kept: Vec<(usize, String)> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let rel = t.path.strip_prefix(&self.folder).ok()?.to_str()?;
                Some((i, rel.replace('\\', "/")))
            })
            .collect();
        session::EditorItem {
            rect: rect.into(),
            folder: self.folder.to_string_lossy().into_owned(),
            active: self
                .active
                .and_then(|a| kept.iter().position(|(i, _)| *i == a)),
            open: kept.into_iter().map(|(_, p)| p).collect(),
            tree: self.show_tree,
        }
    }
}

/// The body below the header, at the block's zoom `z`: the tree on the left (when shown) and
/// the files on the right.
pub(crate) fn body(
    ws: &Workspace,
    id: BlockId,
    editor: &Editor,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    if !editor.folder.is_dir() {
        return gone(id, &editor.folder, ui, z, cx);
    }
    let tree = editor.show_tree.then(|| tree_panel(id, editor, ui, z, cx));
    // The wheel scrolls the tree or the file and stops there; with Ctrl it zooms the canvas.
    div()
        .size_full()
        .flex()
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        .children(tree)
        .child(files(ws, id, editor, z, cx))
        .into_any_element()
}

/// The folder isn't there: say so, and offer another.
fn gone(id: BlockId, folder: &Path, ui: Ui, z: f32, cx: &mut Context<Workspace>) -> AnyElement {
    let n = id.0 as usize;
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.0 * z))
        .text_size(px(13.0 * z))
        .text_color(ui.title_dim)
        .child(format!("{} is gone", folder.display()))
        .child(
            button(("choose-folder", n), "Choose another folder…", ui, z)
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.choose_editor_folder(id, window, cx)
                }))
                .test_support(),
        )
        .into_any_element()
}

/// A small outlined button, at the zoom.
fn button(id: impl Into<ElementId>, label: &'static str, ui: Ui, z: f32) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(10.0 * z))
        .py(px(5.0 * z))
        .rounded(px(8.0 * z))
        .border_1()
        .border_color(ui.hairline)
        .text_size(px(12.0 * z))
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(label)
}

fn tree_panel(
    id: BlockId,
    editor: &Editor,
    ui: Ui,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = id.0 as usize;
    let entity = cx.entity();
    div()
        .id(("tree", n))
        .w(px(TREE_W * z))
        .h_full()
        .flex_shrink_0()
        .border_r_1()
        .border_color(ui.hairline)
        .overflow_hidden()
        .child(
            Tree::new(&editor.tree)
                .item(move |ix, entry, state, _, _| row(&entity, id, ix, entry, state, ui, z))
                // The virtual list needs a height of its own, as gpui-component's Tree gives it.
                .list_style(StyleRefinement::default().flex_grow_1().size_full())
                .relative()
                .size_full(),
        )
        .test_support()
        .into_any_element()
}

/// One row of the tree: indented by its depth, a chevron for a folder, a file icon for a file.
/// The tree itself selects and expands on mouse down; a placeholder row does nothing.
fn row(
    entity: &Entity<Workspace>,
    block: BlockId,
    ix: usize,
    entry: &TreeEntry,
    state: TreeEntryState,
    ui: Ui,
    z: f32,
) -> AnyElement {
    let item = entry.item();
    let disabled = item.is_disabled();
    let is_dir = item.is_folder();
    let icon = if !is_dir {
        IconName::File
    } else if entry.is_expanded() {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    div()
        .id(("entry", ix))
        .flex()
        .items_center()
        .gap(px(4.0 * z))
        .h(px(ROW_H * z))
        .pl(px((8.0 + 14.0 * entry.depth() as f32) * z))
        .pr(px(6.0 * z))
        .text_size(px(12.0 * z))
        .text_color(if disabled { ui.title_dim } else { ui.title })
        .when(state.is_selected(), |el| el.bg(ui.card))
        .when(!disabled, |el| {
            el.cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child(Icon::new(icon).size(px(12.0 * z)).text_color(ui.title_dim))
        })
        .child(div().truncate().child(item.label.clone()))
        .when(!is_dir && !disabled, |el| {
            let open = entity.clone();
            let path = PathBuf::from(item.id.as_ref());
            el.on_click(move |_: &ClickEvent, window, cx| {
                open.update(cx, |ws, cx| ws.open_file(block, &path, None, window, cx))
            })
        })
        .test_support()
        .into_any_element()
}

/// The right side: the tabs, and the active file (or what keeps it from being edited).
fn files(
    ws: &Workspace,
    id: BlockId,
    editor: &Editor,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = id.0 as usize;
    let ui = ws.ui;
    let markdown = editor
        .active_tab()
        .is_some_and(|t| lang_of(&t.path) == "markdown");
    let preview = editor.active_tab().is_some_and(|t| t.preview);
    let tabs = div()
        .id(("tabs", n))
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(TAB_H * z))
        .border_b_1()
        .border_color(ui.hairline)
        .overflow_hidden()
        .children(
            editor
                .tabs
                .iter()
                .enumerate()
                .map(|(i, tab)| tab_el(id, i, tab, editor.active == Some(i), ui, z, cx)),
        )
        // A Markdown file gets the note's preview toggle, at the row's right end.
        .when(markdown, |el| {
            el.child(div().flex_1()).child(
                div()
                    .id(("md-preview", n))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.0 * z))
                    .mr(px(4.0 * z))
                    .rounded(px(5.0 * z))
                    .cursor_pointer()
                    .hover(move |s| s.bg(ui.hover))
                    .child(
                        Icon::new(if preview {
                            IconName::EyeOff
                        } else {
                            IconName::Eye
                        })
                        .size(px(12.0 * z))
                        .text_color(ui.title),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                        ws.toggle_file_preview(id, window, cx)
                    }))
                    .test_support(),
            )
        });
    let area: AnyElement = match editor.active_tab() {
        None => div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.0 * z))
            .text_color(ui.title_dim)
            .child("Click a file to open it")
            .into_any_element(),
        Some(tab) => match tab.content {
            Content::Binary { size } => {
                let path = tab.path.clone();
                div()
                    .id(("binary", n))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0 * z))
                    .text_size(px(13.0 * z))
                    .text_color(ui.title_dim)
                    .child(format!("Binary file · {}", human(size)))
                    .child(
                        button(("reveal", n), "Reveal in file manager", ui, z)
                            .on_click(move |_: &ClickEvent, _, cx| cx.reveal_path(&path))
                            .test_support(),
                    )
                    .test_support()
                    .into_any_element()
            }
            Content::Text { big, .. } => {
                let view: AnyElement = if tab.preview {
                    div()
                        .id(("file-preview", n))
                        .size_full()
                        .overflow_y_scroll()
                        .text_size(px(FONT * z))
                        .px(px(6.0 * z))
                        .child(TextView::markdown(
                            ("file-markdown", n),
                            tab.editor.read(cx).value(),
                        ))
                        .test_support()
                        .into_any_element()
                } else {
                    div()
                        .id(("code", n))
                        .size_full()
                        .child(
                            // The element sets the theme's code font and size itself, and the
                            // read-only flag on every frame: they go on it, not on a parent.
                            EditorView::new(&tab.editor)
                                .appearance(false)
                                .bordered(false)
                                .readonly(big)
                                .font_family(ws.cell.font.family.clone())
                                .text_size(px(FONT * z))
                                .h(relative(1.0)),
                        )
                        .test_support()
                        .into_any_element()
                };
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .when(big, |el| el.child(bar(ui, z).child("Over 4 MB: read-only")))
                    .child(div().flex_1().min_h(px(0.0)).child(view))
                    .into_any_element()
            }
        },
    };
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(tabs)
        .child(area)
        .into_any_element()
}

/// A line above the file, for what holds for the whole file.
fn bar(ui: Ui, z: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0 * z))
        .px(px(10.0 * z))
        .py(px(4.0 * z))
        .border_b_1()
        .border_color(ui.hairline)
        .bg(ui.card)
        .text_size(px(12.0 * z))
        .text_color(ui.title_dim)
}

/// One tab: "● " before a changed file's name, and its ✕.
fn tab_el(
    block: BlockId,
    i: usize,
    tab: &Tab,
    active: bool,
    ui: Ui,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = block.0 as usize;
    let label = format!("{}{}", if tab.dirty { "● " } else { "" }, tab.name());
    div()
        .id(ElementId::Name(format!("tab-{n}-{i}").into()))
        .flex()
        .items_center()
        .gap(px(6.0 * z))
        .h_full()
        .px(px(10.0 * z))
        .text_size(px(12.0 * z))
        .text_color(if active { ui.title } else { ui.title_dim })
        .when(active, |el| el.bg(ui.card))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(div().truncate().child(label))
        .child(
            div()
                .id(ElementId::Name(format!("tab-close-{n}-{i}").into()))
                .flex()
                .items_center()
                .justify_center()
                .size(px(16.0 * z))
                .rounded(px(4.0 * z))
                .hover(move |s| s.bg(ui.pressed))
                .child(
                    Icon::new(IconName::Close)
                        .size(px(10.0 * z))
                        .text_color(ui.title_dim),
                )
                // Pressing the ✕ never counts as a click on the tab.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.close_tab(block, i, window, cx)
                }))
                .test_support(),
        )
        .on_click(
            cx.listener(move |ws, _: &ClickEvent, window, cx| {
                ws.activate_tab(block, i, window, cx)
            }),
        )
        .test_support()
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_come_first_then_files_sorted_without_case() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        std::fs::write(dir.path().join("A.txt"), "").unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();
        std::fs::create_dir(dir.path().join("zed")).unwrap();
        std::fs::create_dir(dir.path().join("Src")).unwrap();
        let names: Vec<_> = read_dir(dir.path())
            .into_iter()
            .map(|e| (e.name, e.is_dir))
            .collect();
        assert_eq!(
            names,
            [
                ("Src".to_string(), true),
                ("zed".to_string(), true),
                (".hidden".to_string(), false),
                ("A.txt".to_string(), false),
                ("b.txt".to_string(), false),
            ]
        );
        assert!(read_dir(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn languages_come_from_the_extension_or_the_name() {
        assert_eq!(lang_of(Path::new("src/main.rs")), "rust");
        assert_eq!(lang_of(Path::new("app.ts")), "typescript");
        assert_eq!(lang_of(Path::new("README.md")), "markdown");
        assert_eq!(lang_of(Path::new("Makefile")), "make");
        assert_eq!(lang_of(Path::new("lib.h")), "cpp");
        assert_eq!(
            lang_of(Path::new("UPPER.RS")),
            "rust",
            "the case doesn't count"
        );
        assert_eq!(
            lang_of(Path::new("Dockerfile")),
            "text",
            "no grammar: plain text"
        );
        assert_eq!(lang_of(Path::new("x.unknown")), "text");
    }

    #[test]
    fn a_nul_in_the_first_8_kb_means_binary() {
        assert!(is_binary(b"\x7fELF\0\0"));
        assert!(!is_binary("h\u{e9}llo\n".as_bytes()));
        let mut late = vec![b'a'; BINARY_PROBE];
        late.push(0);
        assert!(!is_binary(&late), "a NUL after the probe isn't seen");
    }

    #[test]
    fn line_endings_and_the_final_newline_are_kept() {
        assert_eq!(line_ending("a\r\nb\n"), "\r\n");
        assert_eq!(line_ending("a\nb\r\n"), "\n");
        assert_eq!(line_ending("no newline"), "\n");
        assert_eq!(encode("a\nb", true, true), "a\r\nb\r\n");
        assert_eq!(encode("a\nb\n", false, false), "a\nb");
        assert_eq!(
            encode("a\r\nb", false, true),
            "a\nb\n",
            "the editor's own CRLF is normalized"
        );
    }

    #[test]
    fn sizes_read_like_a_file_manager() {
        assert_eq!(human(812), "812 B");
        assert_eq!(human(12 * 1024 + 300), "12.3 KB");
        assert_eq!(human(4 * 1024 * 1024), "4.0 MB");
    }
}
