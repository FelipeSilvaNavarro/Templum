//! An editor block's file tree: expanding it, and creating, renaming and deleting entries.

use std::path::{Path, PathBuf};

use gpui_kit::base::{TreeEvent, TreeState};
use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Entity, Window};
use templum_canvas::BlockId;

use crate::editor;

use super::{Body, Workspace, show_notification};

impl Workspace {
    pub(crate) fn toggle_tree(&mut self, id: BlockId, cx: &mut Context<Self>) {
        if let Some(editor) = self.space.editor_mut(id) {
            editor.show_tree = !editor.show_tree;
            cx.notify();
        }
    }

    pub(crate) fn refresh_tree(&mut self, block: BlockId, cx: &mut Context<Self>) {
        if let Some(editor) = self.space.editor_mut(block) {
            editor.refresh(cx);
            cx.notify();
        }
    }

    /// Creates a file (and opens it) or a folder in `dir`; false (the dialog stays) for an empty
    /// name or one with a separator. A name already taken says so.
    pub(crate) fn new_entry(
        &mut self,
        block: BlockId,
        dir: &Path,
        folder: bool,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = name.trim();
        if name.is_empty() || name.contains(['/', '\\']) {
            return false;
        }
        let path = dir.join(name);
        let made = if folder {
            std::fs::create_dir(&path)
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map(|_| ())
        };
        if let Err(err) = made {
            let message = format!("Couldn't create {name}: {err}");
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
            return false;
        }
        if let Some(editor) = self.space.editor_mut(block) {
            editor.expanded.insert(dir.to_path_buf());
        }
        self.refresh_tree(block, cx);
        if !folder {
            self.open_file(block, &path, None, window, cx);
        }
        true
    }

    /// Renames a file or folder in place; open tabs under it follow it.
    pub(crate) fn rename_entry(
        &mut self,
        block: BlockId,
        path: &Path,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = name.trim();
        if name.is_empty() || name.contains(['/', '\\']) {
            return false;
        }
        let to = path.with_file_name(name);
        if to != path && to.symlink_metadata().is_ok() && !editor::same_file(path, &to) {
            let message = format!("{name} already exists");
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
            return false;
        }
        if let Err(err) = std::fs::rename(path, &to) {
            let message = format!("Couldn't rename {}: {err}", path.display());
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
            return false;
        }
        if let Some(editor) = self.space.editor_mut(block) {
            for tab in &mut editor.tabs {
                if let Some(moved) = editor::moved(&tab.path, path, &to) {
                    tab.path = moved;
                }
            }
            editor.expanded = editor
                .expanded
                .drain()
                .map(|d| editor::moved(&d, path, &to).unwrap_or(d))
                .collect();
        }
        self.refresh_tree(block, cx);
        true
    }

    /// Deletes a file, or a folder with everything in it; tabs on what was deleted close.
    pub(crate) fn delete_entry(
        &mut self,
        block: BlockId,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        if let Err(err) = removed {
            let message = format!("Couldn't delete {}: {err}", path.display());
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::warning(message));
            return;
        }
        let gone: Vec<usize> = self
            .space
            .editor(block)
            .map(|e| {
                e.tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.path.starts_with(path))
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        // Highest first, so the lower indices stay right.
        for i in gone.into_iter().rev() {
            self.remove_tab(block, i, window, cx);
        }
        if let Some(editor) = self.space.editor_mut(block) {
            editor.expanded.retain(|d| !d.starts_with(path));
        }
        self.refresh_tree(block, cx);
    }

    /// A folder of an editor's tree was expanded (read it, once) or collapsed.
    pub(crate) fn on_tree_event(
        &mut self,
        tree: &Entity<TreeState>,
        event: &TreeEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.space.blocks.values_mut().find_map(|b| match b {
            Body::Editor(e) if e.tree == *tree => Some(e),
            _ => None,
        }) else {
            return;
        };
        match event {
            TreeEvent::Expanded(id) => {
                let path = PathBuf::from(id.as_ref());
                if !editor.dirs.contains_key(&path) {
                    editor.dirs.insert(path.clone(), editor::read_dir(&path));
                }
                editor.expanded.insert(path);
            }
            TreeEvent::Collapsed(id) => {
                editor.expanded.remove(Path::new(id.as_ref()));
            }
        }
        editor.show(cx);
        cx.notify();
    }
}
