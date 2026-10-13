//! A Git block's keys while it has the keyboard, its own focus and not a field's
//! (docs/specs/git.md, section 1, Keyboard).

use gpui_kit::{Context, KeyDownEvent, ScrollStrategy, Window};
use templum_canvas::BlockId;

use crate::git_block::diff_view::Row;
use crate::git_block::{List, Tab};

use super::Workspace;

/// The hunk under the keyboard's cursor: its section, its path, its patch (None when it can't
/// go alone: a new or deleted file, whitespace ignored), its first line.
struct AtCursor {
    staged: bool,
    path: String,
    patch: Option<String>,
    line: u32,
}

impl Workspace {
    pub(crate) fn git_key(
        &mut self,
        id: BlockId,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        // A field has the keyboard: its letters are typing. Another tab shows: the files and
        // hunks the keys act on aren't on screen.
        if !git.focus.is_focused(window) || git.tab != Tab::Changes {
            return;
        }
        let k = &e.keystroke;
        let m = &k.modifiers;
        if k.key == "enter" && m.secondary() && !m.shift && !m.alt {
            self.git_commit(id, false, window, cx);
        } else if m.control || m.platform || m.function {
            // The workspace's shortcuts.
            return;
        } else {
            match k.key.as_str() {
                "up" => self.step_file(id, -1, cx),
                "down" => self.step_file(id, 1, cx),
                "[" => self.step_hunk(id, -1, cx),
                "]" => self.step_hunk(id, 1, cx),
                "s" => self.key_stage(id, false, window, cx),
                "u" => self.key_stage(id, true, window, cx),
                "d" => self.key_discard(id, window, cx),
                "space" => self.key_check(id, window, cx),
                "enter" => self.key_open(id, window, cx),
                // The summary is in the left column: shown first, so the keyboard goes to a
                // field that is drawn.
                "c" => self.focus_summary(id, window, cx),
                _ => return,
            }
        }
        cx.stop_propagation();
    }

    /// ↑ / ↓: the previous or next file of the left column. The panel scrolls to it, and the
    /// hunk cursor goes: `s`, `u` and `d` then act on the file.
    fn step_file(&mut self, id: BlockId, step: isize, cx: &mut Context<Self>) {
        let Some(git) = self.space.git(id) else {
            return;
        };
        let (conflicts, changed) = (git.status.conflicts(), git.status.files());
        let files: Vec<(usize, List, String)> = git
            .listed(cx)
            .into_iter()
            .enumerate()
            .filter_map(|(ix, (list, i))| {
                let entries = match list {
                    List::Conflicts => &conflicts,
                    List::Files => &changed,
                };
                Some((ix, list, entries.get(i?)?.path.clone()))
            })
            .collect();
        if files.is_empty() {
            return;
        }
        let at = git.selected.as_ref().and_then(|(l, p)| {
            files
                .iter()
                .position(|(_, list, path)| list == l && path == p)
        });
        let next = match at {
            Some(at) => at.saturating_add_signed(step).min(files.len() - 1),
            None if step > 0 => 0,
            None => files.len() - 1,
        };
        let (row, list, path) = files[next].clone();
        let staged = git.in_staged(&path);
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        git.panel.cursor = None;
        git.lists_scroll.scroll_to_item(row, ScrollStrategy::Center);
        if let Some(ix) = git.panel.row_of(staged, &path) {
            git.panel.scroll.scroll_to_item(ix, ScrollStrategy::Top);
        }
        git.selected = Some((list, path));
        cx.notify();
    }

    /// [ / ]: the keyboard's hunk moves to the previous or next hunk of the panel (from the
    /// selected file when there is none yet), selects its file, and the panel scrolls to it.
    fn step_hunk(&mut self, id: BlockId, step: isize, cx: &mut Context<Self>) {
        let Some(git) = self.space.git_mut(id) else {
            return;
        };
        let target = {
            let p = &git.panel;
            let hunks: Vec<usize> = p
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| matches!(r, Row::Hunk { .. }))
                .map(|(ix, _)| ix)
                .collect();
            if hunks.is_empty() {
                return;
            }
            let next = match p
                .cursor_row()
                .and_then(|row| hunks.iter().position(|&h| h == row))
            {
                Some(at) => at.saturating_add_signed(step).min(hunks.len() - 1),
                None => {
                    let from = git
                        .selected
                        .as_ref()
                        .and_then(|(_, path)| p.row_of(git.in_staged(path), path))
                        .unwrap_or(0);
                    if step > 0 {
                        hunks.iter().position(|&h| h > from).unwrap_or(0)
                    } else {
                        hunks
                            .iter()
                            .rposition(|&h| h < from)
                            .unwrap_or(hunks.len() - 1)
                    }
                }
            };
            let row = hunks[next];
            let Row::Hunk { file, hunk } = p.rows[row] else {
                return;
            };
            let s = &p.files[file];
            (row, s.staged, s.diff.path().to_string(), hunk)
        };
        let (row, staged, path, hunk) = target;
        git.panel.cursor = Some((staged, path.clone(), hunk));
        let list = if git.status.conflicts().iter().any(|e| e.path == path) {
            List::Conflicts
        } else {
            List::Files
        };
        git.selected = Some((list, path));
        git.panel.scroll.scroll_to_item(row, ScrollStrategy::Center);
        cx.notify();
    }

    fn at_cursor(&self, id: BlockId) -> Option<AtCursor> {
        let git = self.space.git(id)?;
        let p = &git.panel;
        let Row::Hunk { file, hunk } = p.rows[p.cursor_row()?] else {
            return None;
        };
        let s = &p.files[file];
        Some(AtCursor {
            staged: s.staged,
            path: s.diff.path().to_string(),
            patch: (!p.ignore_ws)
                .then(|| templum_git::hunk_patch(&s.diff, hunk))
                .flatten(),
            line: s.diff.hunks[hunk].new_start.max(1),
        })
    }

    /// `path` is in the Conflicts list.
    fn conflicted(&self, id: BlockId, path: &str) -> bool {
        self.space
            .git(id)
            .is_some_and(|g| g.status.conflicts().iter().any(|e| e.path == path))
    }

    /// s / u: the cursor's hunk (Stage in Changes, Unstage in Staged; one that can't go alone
    /// takes its whole file), else the selected file.
    fn key_stage(
        &mut self,
        id: BlockId,
        unstage: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.space.git(id).and_then(|g| g.selected.clone());
        // A conflict: s marks it resolved (asking while markers are left); u does nothing.
        let conflict = match (&selected, self.at_cursor(id)) {
            (_, Some(c)) if self.conflicted(id, &c.path) => Some(c.path),
            (Some((List::Conflicts, path)), None) => Some(path.clone()),
            _ => None,
        };
        if let Some(path) = conflict {
            if !unstage {
                self.mark_resolved(id, path, window, cx);
            }
            return;
        }
        match self.at_cursor(id) {
            // s on a staged hunk, u on an unstaged one: nothing to do.
            Some(c) if c.staged != unstage => {}
            Some(AtCursor {
                patch: Some(patch), ..
            }) => self.apply_hunk(id, patch, unstage, window, cx),
            Some(c) if unstage => self.unstage(id, vec![c.path], window, cx),
            Some(c) => self.stage(id, vec![c.path], window, cx),
            None => match (selected, unstage) {
                (Some((List::Files, path)), true) => self.unstage(id, vec![path], window, cx),
                (Some((List::Files, path)), false) => self.stage(id, vec![path], window, cx),
                _ => {}
            },
        }
    }

    /// d: the cursor's hunk in Changes (one that can't go alone takes its file), else the
    /// selected file's unstaged changes. Both ask first.
    fn key_discard(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.space.git(id).and_then(|g| g.selected.clone());
        // A conflicted file isn't discarded by key: its choices are its regions'.
        if self
            .at_cursor(id)
            .is_some_and(|c| self.conflicted(id, &c.path))
        {
            return;
        }
        match self.at_cursor(id) {
            Some(c) if c.staged => {}
            Some(AtCursor {
                path,
                patch: Some(patch),
                ..
            }) => self.discard_hunk(id, path, patch, window, cx),
            Some(c) => self.discard(id, vec![c.path], window, cx),
            None => {
                if let Some((List::Files, path)) = selected {
                    self.discard(id, vec![path], window, cx)
                }
            }
        }
    }

    /// Space: the selected file's box in the Changes checklist is checked, or unchecked.
    fn key_check(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((List::Files, path)) = self.space.git(id).and_then(|g| g.selected.clone()) {
            self.toggle_check(id, path, window, cx);
        }
    }

    /// Enter: the file in an editor beside the block, at the cursor's hunk, else at the selected
    /// file's first.
    fn key_open(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.space.git(id).and_then(|g| g.selected.clone());
        match (self.at_cursor(id), selected) {
            (Some(c), _) => self.open_from_git_at(id, &c.path, Some(c.line), window, cx),
            (None, Some((_, path))) => self.open_from_git(id, &path, window, cx),
            (None, None) => {}
        }
    }
}
