//! Note blocks' tabs: new notes, opening one where it is or in a block, closing a tab, and
//! deleting a note for good (the Notes dialog). A note is open in one tab at most: opening it
//! again shows that tab. Every change to a note's text is written to `Workspace::notes` as it is
//! typed, and saved with the canvas.

use gpui_kit::component::input::InputEvent;
use gpui_kit::{Context, Window};
use templum_canvas::{BlockId, Point, Rect};

use crate::board::Label;
use crate::note::{self, Note, NoteTab};
use crate::notes;

use super::{Body, Space, Workspace};

impl Workspace {
    /// A note block at the viewport's center with a new note, its editor with the keyboard.
    pub(crate) fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let center = self.space.canvas.camera.to_world(self.center(window));
        let size = note::SIZE;
        let at = self
            .space
            .canvas
            .place_new(
                size,
                Point::new(center.x - size.w / 2.0, center.y - size.h / 2.0),
            )
            .origin();
        self.add_note_at(at, window, cx);
    }

    /// A note block with a new note, its top-left corner at `at` (world units), its editor with
    /// the keyboard.
    pub(crate) fn add_note_at(&mut self, at: Point, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.notes.add(String::new(), None, notes::now());
        let tab = self.note_tab(id, window, cx);
        self.add_note_block(at, tab.into_iter().collect(), window, cx);
    }

    /// A note block of the default size at `at` showing `tabs`, focused.
    fn add_note_block(
        &mut self,
        at: Point,
        tabs: Vec<NoteTab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space.pristine = false;
        let rect = Rect::new(at.x, at.y, note::SIZE.w, note::SIZE.h);
        let block = Body::Note(Note { tabs, active: 0 });
        if !block.focus_inner(window, cx) {
            window.focus(&self.focus_handle, cx);
        }
        self.add_block(rect, block);
        cx.notify();
    }

    /// A tab for note `id`: an editor on its text, whose every change is written to the notes.
    /// None when there is no such note.
    pub(super) fn note_tab(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<NoteTab> {
        let text = self.notes.get(id)?.text.clone();
        let editor = note::editor(&text, window, cx);
        cx.subscribe(&editor, move |ws, editor, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                let text = editor.read(cx).value().to_string();
                ws.notes.set_text(id, &text, notes::now());
            }
        })
        .detach();
        Some(NoteTab {
            id,
            editor,
            far: Default::default(),
        })
    }

    /// The workspace and block where note `id` is open, if it is.
    pub(crate) fn note_open_at(&self, id: u64) -> Option<(&Space, BlockId)> {
        self.spaces().find_map(|space| {
            space.blocks.iter().find_map(|(block, body)| {
                body.note()
                    .is_some_and(|n| n.tabs.iter().any(|t| t.id == id))
                    .then_some((space, *block))
            })
        })
    }

    /// Opens note `id`: where it is open, that tab is shown; otherwise as a new tab of the note
    /// block `into`, or in a new note block at the view's center without one.
    pub(crate) fn open_note(
        &mut self,
        id: u64,
        into: Option<BlockId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((space, block)) = self.note_open_at(id) {
            let space = space.id.clone();
            if let Some(note) = self
                .space_by_id_mut(&space)
                .and_then(|s| s.blocks.get_mut(&block))
                .and_then(Body::note_mut)
            {
                note.active = note.tabs.iter().position(|t| t.id == id).unwrap_or(0);
            }
            self.reveal(&space, block, window, cx);
            return;
        }
        let Some(tab) = self.note_tab(id, window, cx) else {
            return;
        };
        match into.filter(|b| self.space.blocks.get(b).is_some_and(|b| b.note().is_some())) {
            Some(block) => self.add_tab(block, tab, window, cx),
            None => {
                let center = self.space.canvas.camera.to_world(self.center(window));
                let near = Point::new(center.x - note::SIZE.w / 2.0, center.y - note::SIZE.h / 2.0);
                let at = self.space.canvas.place_new(note::SIZE, near).origin();
                self.add_note_block(at, vec![tab], window, cx);
            }
        }
    }

    /// `tab` after the block's last one, shown, with the keyboard.
    fn add_tab(
        &mut self,
        block: BlockId,
        tab: NoteTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_focus(block, window, cx);
        if let Some(note) = self.space.blocks.get_mut(&block).and_then(Body::note_mut) {
            note.tabs.push(tab);
            note.active = note.tabs.len() - 1;
        }
        self.focus_block(block, window, cx);
    }

    /// The tab strip's +: a new note in a new tab of `block`.
    pub(crate) fn new_note_tab(
        &mut self,
        block: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.notes.add(String::new(), None, notes::now());
        if let Some(tab) = self.note_tab(id, window, cx) {
            self.add_tab(block, tab, window, cx);
        }
    }

    /// Shows the block's tab `index`, with the keyboard.
    pub(crate) fn show_note_tab(
        &mut self,
        block: BlockId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_focus(block, window, cx);
        if let Some(note) = self.space.blocks.get_mut(&block).and_then(Body::note_mut)
            && index < note.tabs.len()
        {
            note.active = index;
        }
        self.focus_block(block, window, cx);
    }

    /// Closes the block's tab `index`. The note stays in the Notes dialog, unless it is blank:
    /// nothing is lost then.
    pub(crate) fn close_note_tab(
        &mut self,
        block: BlockId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_focus(block, window, cx);
        let Some(note) = self.space.blocks.get_mut(&block).and_then(Body::note_mut) else {
            return;
        };
        if index >= note.tabs.len() {
            return;
        }
        let tab = note.tabs.remove(index);
        if note.active > index || note.active >= note.tabs.len() {
            note.active = note.active.saturating_sub(1);
        }
        self.drop_if_blank(tab.id);
        self.focus_block(block, window, cx);
    }

    /// Note `id` is no more open: a blank one goes.
    pub(super) fn drop_if_blank(&mut self, id: u64) {
        if self.notes.get(id).is_some_and(|n| n.text.trim().is_empty()) {
            self.notes.remove(id);
        }
    }

    /// The Notes dialog's delete: note `id` goes for good, from every block that shows it.
    pub(crate) fn delete_note(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((space, block)) = self.note_open_at(id) {
            let space = space.id.clone();
            if space == self.space.id {
                self.release_focus(block, window, cx);
            }
            if let Some(note) = self
                .space_by_id_mut(&space)
                .and_then(|s| s.blocks.get_mut(&block))
                .and_then(Body::note_mut)
                && let Some(index) = note.tabs.iter().position(|t| t.id == id)
            {
                note.tabs.remove(index);
                if note.active > index || note.active >= note.tabs.len() {
                    note.active = note.active.saturating_sub(1);
                }
            }
        }
        self.notes.remove(id);
        cx.notify();
    }

    /// Tints the note on screen in `block` with `color` (None: the blocks' own).
    pub(crate) fn set_note_color(
        &mut self,
        block: BlockId,
        color: Option<Label>,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.space.blocks.get(&block).and_then(|b| b.note()?.tab()) {
            self.notes.set_color(tab.id, color);
            cx.notify();
        }
    }
}
