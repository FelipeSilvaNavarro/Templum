//! An editor block: a file tree, tabs of open files and gpui-kit's code editor, on one folder.
//! See docs/specs/cockpit.md, section 7.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Tree, TreeEntry, TreeEntryState, TreeEvent, TreeItem, TreeState};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, Entity, ScrollWheelEvent, SharedString,
    Stateful, StyleRefinement, Subscription, TestSupportExt, div, px,
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

/// An editor block: one folder, the file tree over it, the open files.
pub(crate) struct Editor {
    pub folder: PathBuf,
    pub tree: Entity<TreeState>,
    /// The folders read so far, by path: the root and each one expanded. A folder that was
    /// never expanded costs nothing (`node_modules`).
    pub dirs: HashMap<PathBuf, Vec<Entry>>,
    pub expanded: HashSet<PathBuf>,
    pub show_tree: bool,
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

    pub fn title(&self) -> SharedString {
        self.folder_name().into()
    }

    /// As the canvas file holds it.
    pub fn item(&self, rect: Rect) -> session::EditorItem {
        session::EditorItem {
            rect: rect.into(),
            folder: self.folder.to_string_lossy().into_owned(),
            open: Vec::new(),
            active: None,
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
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0 * z))
                .text_color(ui.title_dim)
                .child("Click a file to open it"),
        )
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
    _entity: &Entity<Workspace>,
    _block: BlockId,
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
}
