//! Note blocks' tabs: new notes, opening one where it is or in a block, closing a tab, and
//! deleting a note for good (the Notes dialog). A note is open in one tab at most: opening it
//! again shows that tab. Every change to a note's text is written to `Workspace::notes` as it is
//! typed, and saved with the canvas.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Focusable as _, Window};
use templum_canvas::{BlockId, Point, Rect};

use crate::board::Label;
use crate::note::{self, Note, NoteTab, Renaming};
use crate::notes;

use super::remote::Deed;
use super::{Body, Space, Workspace};

impl Workspace {
    /// A note block at the viewport's center with a new note, its editor with the keyboard.
    pub(crate) fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.why_not(Deed::AddNote).is_some() {
            return;
        }
        let at = self.room_in_view(note::SIZE, window, cx).origin();
        self.add_note_at(at, window, cx);
    }

    /// A note block with a new note, its top-left corner at `at` (world units), its editor with
    /// the keyboard.
    pub(crate) fn add_note_at(&mut self, at: Point, window: &mut Window, cx: &mut Context<Self>) {
        if self.why_not(Deed::AddNote).is_some() {
            return;
        }
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
        let block = Body::Note(Note {
            tabs,
            ..Default::default()
        });
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
        if self.why_not(Deed::OpenNote).is_some() {
            return;
        }
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
                let at = self.room_in_view(note::SIZE, window, cx).origin();
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
        // Home's notes are read-only here: nothing is added to this computer's for its block.
        if self.on_remote() {
            return;
        }
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
        // (A mirrored note's shown tab is the mirror's own choice.)
        if let Some(note) = self.space.note_view_mut(block)
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
        let tab = note.remove_tab(index);
        self.drop_if_blank(tab.id);
        self.focus_block(block, window, cx);
    }

    /// Note `id` is no more open: a blank one goes (no text, and never named).
    pub(super) fn drop_if_blank(&mut self, id: u64) {
        if self.notes.get(id).is_some_and(notes::Entry::is_blank) {
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
                note.remove_tab(index);
            }
        }
        self.notes.remove(id);
        cx.notify();
    }

    /// A double click on the block's tab `index`: its name is selected in the tab, to be typed over
    /// with the keyboard. Enter keeps it, and so does the keyboard leaving; Esc drops it.
    pub(crate) fn rename_note_tab(
        &mut self,
        block: BlockId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_note_rename(block, true, window, cx);
        let Some(id) = self
            .space
            .blocks
            .get(&block)
            .and_then(|b| b.note()?.tabs.get(index))
            .map(|t| t.id)
        else {
            return;
        };
        let name = self.notes.name(id).to_string();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        input.update(cx, |input, cx| input.select_all(window, cx));
        cx.subscribe_in(
            &input,
            window,
            move |ws, input, event: &InputEvent, window, cx| match event {
                // Enter, or a click elsewhere.
                InputEvent::PressEnter { .. } | InputEvent::Blur
                    if ws.renaming_with(block, input) =>
                {
                    ws.stop_note_rename(block, true, window, cx)
                }
                _ => {}
            },
        )
        .detach();
        if let Some(note) = self.space.blocks.get_mut(&block).and_then(Body::note_mut) {
            note.active = index;
            note.renaming = Some(Renaming {
                id,
                input: input.clone(),
            });
        }
        self.focus_block(block, window, cx);
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// `block`'s tab is being renamed in `input`.
    fn renaming_with(&self, block: BlockId, input: &Entity<InputState>) -> bool {
        self.space
            .blocks
            .get(&block)
            .and_then(|b| b.note()?.renaming.as_ref())
            .is_some_and(|r| &r.input == input)
    }

    /// Ends the rename of `block`'s tab: the name typed is kept, or with `keep` false dropped. The
    /// note takes the keyboard back from the field.
    pub(crate) fn stop_note_rename(
        &mut self,
        block: BlockId,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(renaming) = self
            .space
            .blocks
            .get_mut(&block)
            .and_then(Body::note_mut)
            .and_then(|n| n.renaming.take())
        else {
            return;
        };
        if keep {
            let name = renaming.input.read(cx).value().to_string();
            self.notes.rename(renaming.id, &name);
        }
        if renaming.input.read(cx).focus_handle(cx).is_focused(window) {
            self.focus_block(block, window, cx);
        }
        cx.notify();
    }

    /// Esc in the tab's field: the old name stays.
    pub(crate) fn cancel_note_rename(
        &mut self,
        block: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_note_rename(block, false, window, cx);
    }

    /// The menu's Rename: note `id` is called `name`.
    pub(crate) fn rename_note(&mut self, id: u64, name: &str, cx: &mut Context<Self>) {
        // A mirrored note is home's, by home's id: this computer's note of that id is another.
        if self.on_remote() {
            return;
        }
        self.notes.rename(id, name);
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
