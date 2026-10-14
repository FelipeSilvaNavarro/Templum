//! Selecting blocks: a rectangle (Shift+drag on the empty canvas) or Shift+click on a title; and
//! what acts on the selected blocks, or on the focused one: a selected title's drag moves them
//! together, Delete closes them, Copy and Paste copy them.

use gpui_kit::{ClipboardItem, Context, Pixels, Point as PixelPoint, Window};
use serde::{Deserialize, Serialize};
use templum_canvas::{BlockId, Gesture, Point, Rect};

use crate::session;

use super::history::{Change, Then};
use super::{Drag, Workspace};

/// What Copy puts on the clipboard for blocks: JSON that Paste knows, here or in another
/// Templum. A terminal comes back as a fresh shell in its folder, without its agent.
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

    /// A press on a selected block's title: dragging moves every selected block.
    pub(crate) fn begin_group_move(&mut self, pos: PixelPoint<Pixels>) {
        let p = self.local(pos);
        self.space.pristine = false;
        self.drag =
            Gesture::move_blocks(&self.space.canvas, &self.space.selected, p).map(Drag::Canvas);
        self.placing(&self.space.selected.clone());
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
        for item in &mut items {
            let rect = item.rect_mut();
            rect.x += dx;
            rect.y += dy;
            // A second agent resuming the same conversation would fight over it.
            if let session::Item::Terminal(t) = item {
                t.agent = None;
            }
        }
        let pasted: Vec<_> = self
            .add_selected(&items, window, cx)
            .into_iter()
            .flatten()
            .collect();
        if !pasted.is_empty() {
            self.keep(Change::Close(pasted), Then::New);
        }
        true
    }

    /// Adds the blocks `items`, selected, with the keyboard on the canvas: what each one became
    /// (None: it couldn't come back). Ctrl+Z is the caller's to keep.
    pub(super) fn add_selected(
        &mut self,
        items: &[session::Item],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Option<BlockId>> {
        self.space.pristine = false;
        self.quiet = true;
        let added: Vec<_> = items
            .iter()
            .map(|item| self.add_saved(item, window, cx))
            .collect();
        self.quiet = false;
        self.space.focused = None;
        self.space.selected = added.iter().flatten().copied().collect();
        window.focus(&self.focus_handle, cx);
        cx.notify();
        added
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
        self.close_blocks(ids, Then::New, window, cx);
        true
    }
}
