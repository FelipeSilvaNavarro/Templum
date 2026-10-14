//! Selecting blocks: a rectangle (Shift+drag on the empty canvas) or Shift+click on a title; and
//! what acts on the selected blocks, or on the focused one: Delete closes them, Copy and Paste
//! copy them.

use gpui_kit::{App, ClipboardItem, Context, Window};
use serde::{Deserialize, Serialize};
use templum_canvas::{BlockId, Point, Rect};

use crate::{alert, prompt, session};

use super::{Drag, Workspace};

/// What Copy puts on the clipboard for blocks: JSON that Paste knows, here or in another
/// Templum. A terminal comes back as a fresh shell in its folder.
#[derive(Serialize, Deserialize)]
struct Clip {
    templum_blocks: Vec<session::Item>,
}

impl Workspace {
    /// Shift+press on the empty canvas at `at` (a world point): a rectangle from there selects
    /// the blocks it touches, with those selected before. Selecting takes the focus away.
    pub(super) fn begin_marquee(&mut self, at: Point) {
        self.space.focused = None;
        self.drag = Some(Drag::Marquee {
            base: self.space.selected.clone(),
            from: at,
            to: at,
        });
        self.press = None;
    }

    /// The rectangle's corner is at `to` (a world point).
    pub(super) fn marquee_to(&mut self, to: Point) {
        let Some(Drag::Marquee { base, from, to: at }) = &mut self.drag else {
            return;
        };
        *at = to;
        let mut selected = base.clone();
        for id in self.space.canvas.hits(Rect::corners(*from, to)) {
            if !selected.contains(&id) {
                selected.push(id);
            }
        }
        self.space.selected = selected;
    }

    /// Shift+click on a block's title: it joins the selection, or leaves it.
    pub(crate) fn toggle_selected(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space.focused = None;
        self.space.selected_ink = None;
        let selected = &mut self.space.selected;
        match selected.iter().position(|s| *s == id) {
            Some(i) => {
                selected.remove(i);
            }
            None => selected.push(id),
        }
        // Delete and Copy reach the workspace, not a field of a block.
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// The keyboard is on the canvas, not in a field: the workspace has it (a focused terminal's
    /// keys go through it), or a focused Git block's own handle does.
    pub(super) fn canvas_has_keys(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
            || self
                .space
                .focused
                .and_then(|id| self.space.git(id))
                .is_some_and(|g| g.focus.is_focused(window))
    }

    /// Copies the selected blocks to the clipboard. False when none is selected.
    pub(super) fn copy_blocks(&mut self, cx: &mut Context<Self>) -> bool {
        let items: Vec<_> = self
            .space
            .canvas
            .blocks()
            .iter()
            .filter(|b| self.space.selected.contains(&b.id))
            .map(|b| self.space.blocks[&b.id].item(b.rect, cx))
            .collect();
        if items.is_empty() {
            return false;
        }
        let clip = Clip {
            templum_blocks: items,
        };
        if let Ok(text) = serde_json::to_string(&clip) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        true
    }

    /// Pastes the blocks on the clipboard, centered in the view (a cascade step off blocks
    /// already there), and selects them. False when the clipboard holds no blocks.
    pub(super) fn paste_blocks(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(clip) = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .and_then(|text| serde_json::from_str::<Clip>(&text).ok())
        else {
            return false;
        };
        let mut items = clip.templum_blocks;
        let Some(bounds) = items
            .iter_mut()
            .map(|item| Rect::from(*item.rect_mut()))
            .reduce(|a, b| a.union(&b))
        else {
            return false;
        };
        let center = self.space.canvas.camera.to_world(self.center(window));
        let near = Point::new(center.x - bounds.w / 2.0, center.y - bounds.h / 2.0);
        let at = self.space.canvas.place_new(bounds.size(), near);
        let (dx, dy) = (at.x - bounds.x, at.y - bounds.y);
        self.space.pristine = false;
        let mut pasted = Vec::new();
        for item in &mut items {
            let rect = item.rect_mut();
            rect.x += dx;
            rect.y += dy;
            pasted.extend(self.add_saved(item, window, cx));
        }
        self.space.focused = None;
        self.space.selected = pasted;
        window.focus(&self.focus_handle, cx);
        cx.notify();
        true
    }

    /// Delete or Backspace on the canvas: closes the selected blocks, else the focused one. A
    /// focused terminal keeps the key (it deletes a character): false.
    pub(super) fn delete_blocks(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let ids: Vec<BlockId> = if self.space.selected.is_empty() {
            match self.space.focused {
                Some(id) if self.space.terminal(id).is_none() => vec![id],
                _ => return false,
            }
        } else {
            self.space
                .canvas
                .blocks()
                .iter()
                .map(|b| b.id)
                .filter(|id| self.space.selected.contains(id))
                .collect()
        };
        self.close_blocks(ids, window, cx);
        true
    }

    /// Closes blocks. One asks as its close button does; several ask once: Save, Don't save or
    /// Cancel when an editor has unsaved files, Close or Cancel when a terminal is at work.
    pub(crate) fn close_blocks(
        &mut self,
        ids: Vec<BlockId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let [id] = ids[..] {
            self.close(id, window, cx);
            return;
        }
        let busy: Vec<String> = ids
            .iter()
            .filter_map(|id| {
                let pane = self.space.terminal(*id)?;
                let why = alert::busy(&pane.term.status(), self.agent_name(pane))?;
                Some(format!("{}: {why}", pane.term.title()))
            })
            .collect();
        let unsaved: Vec<String> = ids
            .iter()
            .flat_map(|id| self.space.unsaved(Some(*id)))
            .collect();
        if busy.is_empty() && unsaved.is_empty() {
            for id in ids {
                self.remove_block(id, window, cx);
            }
            return;
        }
        let ws = cx.entity();
        let remove = {
            let (ws, ids) = (ws.clone(), ids.clone());
            move |window: &mut Window, cx: &mut App| {
                ws.update(cx, |ws, cx| {
                    for id in &ids {
                        ws.remove_block(*id, window, cx);
                    }
                })
            }
        };
        let busy = busy.join(" ");
        if unsaved.is_empty() {
            prompt::confirm(
                format!("Close {} blocks?", ids.len()),
                busy,
                "Close",
                remove,
                window,
                cx,
            );
            return;
        }
        let save = remove.clone();
        prompt::save_or_discard(
            "Save changes?",
            format!("Unsaved: {}. {busy}", unsaved.join(", "))
                .trim_end()
                .to_string(),
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
            remove,
            window,
            cx,
        );
    }
}
