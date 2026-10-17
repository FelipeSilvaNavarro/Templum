//! Checklist blocks' tabs: new checklists, opening one where it is or in a block, closing a tab
//! (a blank checklist goes), the field that edits a name or an item in place, and the changes to
//! a checklist's content, each kept for Ctrl+Z (`edit_checklist`). A checklist is open in one tab
//! at most: opening it again shows that tab.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Focusable as _, Window};
use templum_canvas::{BlockId, Point, Rect};

use crate::checklist::{self, Checklist, Editing, Field};
use crate::checklists::Checklists;
use crate::notes;

use super::{Body, CHECKLIST_UNDO, Space, Workspace};

impl Workspace {
    /// A checklist block at the viewport's center with a new checklist.
    pub(crate) fn new_checklist(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let center = self.space.camera().to_world(self.center(window));
        let size = checklist::SIZE;
        let near = Point::new(center.x - size.w / 2.0, center.y - size.h / 2.0);
        let at = self.space.canvas.place_new(size, near).origin();
        self.add_checklist_at(at, window, cx);
    }

    /// A checklist block with a new checklist, its top-left corner at `at` (world units), its
    /// name being edited.
    pub(crate) fn add_checklist_at(
        &mut self,
        at: Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.checklists.add(notes::now());
        let block = self.add_checklist_block(at, vec![id], window, cx);
        self.edit_field(block, Field::Name, window, cx);
    }

    /// A checklist block of the default size at `at` showing `tabs`, focused.
    fn add_checklist_block(
        &mut self,
        at: Point,
        tabs: Vec<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> BlockId {
        self.space.pristine = false;
        let rect = Rect::new(at.x, at.y, checklist::SIZE.w, checklist::SIZE.h);
        let checklist = Checklist {
            tabs,
            active: 0,
            editing: None,
        };
        let id = self.add_block(rect, Body::Checklist(checklist));
        self.focus_block(id, window, cx);
        id
    }

    /// The workspace and block where checklist `id` is open, if it is.
    pub(crate) fn checklist_open_at(&self, id: u64) -> Option<(&Space, BlockId)> {
        self.spaces().find_map(|space| {
            space.blocks.iter().find_map(|(block, body)| {
                body.checklist()
                    .is_some_and(|c| c.tabs.contains(&id))
                    .then_some((space, *block))
            })
        })
    }

    /// Opens checklist `id`: where it is open, that tab is shown; otherwise as a new tab of the
    /// checklist block `into`, or in a new checklist block at the view's center without one.
    pub(crate) fn open_checklist(
        &mut self,
        id: u64,
        into: Option<BlockId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((space, block)) = self.checklist_open_at(id) {
            let space = space.id.clone();
            if let Some(checklist) = self
                .space_by_id_mut(&space)
                .and_then(|s| s.blocks.get_mut(&block))
                .and_then(Body::checklist_mut)
            {
                checklist.active = checklist.tabs.iter().position(|t| *t == id).unwrap_or(0);
            }
            self.reveal(&space, block, window, cx);
            return;
        }
        if self.checklists.get(id).is_none() {
            return;
        }
        let into = into.filter(|b| {
            self.space
                .blocks
                .get(b)
                .is_some_and(|b| b.checklist().is_some())
        });
        match into {
            Some(block) => self.add_checklist_tab(block, id, window, cx),
            None => {
                let center = self.space.camera().to_world(self.center(window));
                let size = checklist::SIZE;
                let near = Point::new(center.x - size.w / 2.0, center.y - size.h / 2.0);
                let at = self.space.canvas.place_new(size, near).origin();
                self.add_checklist_block(at, vec![id], window, cx);
            }
        }
    }

    /// Checklist `id` after the block's last tab, shown.
    fn add_checklist_tab(
        &mut self,
        block: BlockId,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_editing(block, window, cx);
        if let Some(checklist) = self.space.checklist_mut(block) {
            checklist.tabs.push(id);
            checklist.active = checklist.tabs.len() - 1;
        }
        self.focus_block(block, window, cx);
    }

    /// The tab strip's +: a new checklist in a new tab of `block`, its name being edited.
    pub(crate) fn new_checklist_tab(
        &mut self,
        block: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.checklists.add(notes::now());
        self.add_checklist_tab(block, id, window, cx);
        self.edit_field(block, Field::Name, window, cx);
    }

    /// Shows the block's tab `index`.
    pub(crate) fn show_checklist_tab(
        &mut self,
        block: BlockId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_editing(block, window, cx);
        if let Some(checklist) = self.space.checklist_mut(block)
            && index < checklist.tabs.len()
        {
            checklist.active = index;
        }
        self.focus_block(block, window, cx);
    }

    /// Closes the block's tab `index`. The checklist stays in the Checklists dialog, unless it is
    /// blank: nothing is lost then.
    pub(crate) fn close_checklist_tab(
        &mut self,
        block: BlockId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_editing(block, window, cx);
        let Some(checklist) = self.space.checklist_mut(block) else {
            return;
        };
        if index >= checklist.tabs.len() {
            return;
        }
        let id = checklist.tabs.remove(index);
        if checklist.active > index || checklist.active >= checklist.tabs.len() {
            checklist.active = checklist.active.saturating_sub(1);
        }
        self.drop_checklist_if_blank(id);
        self.focus_block(block, window, cx);
    }

    /// The Checklists dialog's delete: checklist `id` goes for good, from every block that shows
    /// it.
    pub(crate) fn delete_checklist(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((space, block)) = self.checklist_open_at(id) {
            let space = space.id.clone();
            if space == self.space.id {
                self.stop_editing(block, window, cx);
            }
            if let Some(checklist) = self
                .space_by_id_mut(&space)
                .and_then(|s| s.blocks.get_mut(&block))
                .and_then(Body::checklist_mut)
                && let Some(index) = checklist.tabs.iter().position(|t| *t == id)
            {
                checklist.tabs.remove(index);
                if checklist.active > index || checklist.active >= checklist.tabs.len() {
                    checklist.active = checklist.active.saturating_sub(1);
                }
                // A field of a hidden workspace's block goes with its checklist.
                checklist.editing.take_if(|e| e.list == id);
            }
        }
        self.checklists.remove(id);
        cx.notify();
    }

    /// Checklist `id` is no more open: a blank one goes.
    pub(super) fn drop_checklist_if_blank(&mut self, id: u64) {
        if self.checklists.get(id).is_some_and(|l| l.is_blank()) {
            self.checklists.remove(id);
        }
    }

    /// Changes checklist `list` with `f`, keeping it as it was for Ctrl+Z (when it changed).
    pub(crate) fn edit_checklist<R>(
        &mut self,
        list: u64,
        f: impl FnOnce(&mut Checklists) -> R,
    ) -> R {
        let before = self.checklists.get(list).cloned();
        let result = f(&mut self.checklists);
        if let Some(before) = before
            && self.checklists.get(list) != Some(&before)
        {
            self.checklists_undo.push(before);
            if self.checklists_undo.len() > CHECKLIST_UNDO {
                self.checklists_undo.remove(0);
            }
        }
        result
    }

    /// Ctrl+Z on a checklist block: the last changed checklist as it was; a step whose checklist
    /// was deleted since is skipped. False: nothing to undo.
    pub(super) fn undo_checklists(&mut self, cx: &mut Context<Self>) -> bool {
        while let Some(before) = self.checklists_undo.pop() {
            if self.checklists.restore(before) {
                cx.notify();
                return true;
            }
        }
        false
    }

    /// Edits `field` of the checklist on screen in `block` in place, with the keyboard; a field
    /// editing there stops first. Its every key is written to the checklist at once.
    pub(crate) fn edit_field(
        &mut self,
        block: BlockId,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_editing(block, window, cx);
        let Some(list) = self
            .space
            .blocks
            .get(&block)
            .and_then(|b| b.checklist()?.shown())
        else {
            return;
        };
        let text = match (self.checklists.get(list), field) {
            (Some(l), Field::Name) => l.name.clone(),
            (Some(l), Field::Item(item)) => match l.item(item) {
                Some(item) => item.text.clone(),
                None => return,
            },
            (None, _) => return,
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(text));
        // A name is selected, to be typed over; an item's caret goes at its end.
        input.update(cx, |input, cx| match field {
            Field::Name => input.select_all(window, cx),
            Field::Item(_) => {
                let end = input.value().len();
                input.set_selected_range(end..end, cx);
            }
        });
        cx.subscribe_in(
            &input,
            window,
            move |ws, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => ws.field_changed(block, input, cx),
                    InputEvent::PressEnter { .. } => ws.field_enter(block, input, window, cx),
                    // A click elsewhere: on the canvas, another block, another row of this one.
                    InputEvent::Blur if ws.editing_with(block, input) => {
                        ws.stop_editing(block, window, cx)
                    }
                    _ => {}
                }
            },
        )
        .detach();
        self.focus_block(block, window, cx);
        if let Some(checklist) = self.space.checklist_mut(block) {
            checklist.editing = Some(Editing {
                list,
                field,
                input: input.clone(),
                kept: false,
            });
        }
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// `block`'s field is `input`.
    fn editing_with(&self, block: BlockId, input: &Entity<InputState>) -> bool {
        self.space
            .blocks
            .get(&block)
            .and_then(|b| b.checklist()?.editing.as_ref())
            .is_some_and(|e| &e.input == input)
    }

    /// A key typed in the field: written to the checklist, its first one as an undo step.
    fn field_changed(
        &mut self,
        block: BlockId,
        input: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) {
        let Some(editing) = self
            .space
            .checklist_mut(block)
            .and_then(|c| c.editing.as_mut())
            .filter(|e| &e.input == input)
        else {
            return;
        };
        let (list, field, kept) = (editing.list, editing.field, editing.kept);
        editing.kept = true;
        let text = input.read(cx).value().to_string();
        let write = |c: &mut Checklists| match field {
            Field::Name => c.rename(list, &text, notes::now()),
            Field::Item(item) => c.set_text(list, item, &text, notes::now()),
        };
        if kept {
            write(&mut self.checklists);
        } else {
            self.edit_checklist(list, write);
        }
        cx.notify();
    }

    /// Enter in the field: a name is kept; after an item with text, a new one comes, edited; an
    /// empty item goes.
    fn field_enter(
        &mut self,
        block: BlockId,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editing) = self
            .space
            .blocks
            .get(&block)
            .and_then(|b| b.checklist()?.editing.as_ref())
            .filter(|e| &e.input == input)
        else {
            return;
        };
        match editing.field {
            Field::Item(item) if !input.read(cx).value().trim().is_empty() => {
                let list = editing.list;
                self.add_item_after(block, list, Some(item), window, cx)
            }
            _ => self.stop_editing(block, window, cx),
        }
    }

    /// A new empty item in checklist `list` after `after` (at the end: None), edited; adding it
    /// is the undo step its typing goes with.
    pub(crate) fn add_item_after(
        &mut self,
        block: BlockId,
        list: u64,
        after: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_editing(block, window, cx);
        let Some(item) =
            self.edit_checklist(list, |c| c.add_item(list, after, "", false, notes::now()))
        else {
            return;
        };
        self.edit_field(block, Field::Item(item), window, cx);
        if let Some(editing) = self
            .space
            .checklist_mut(block)
            .and_then(|c| c.editing.as_mut())
        {
            editing.kept = true;
        }
    }

    /// Backspace at the field's start: an empty item goes, and the one before it is edited (the
    /// first item's: the name), its caret at the end. A held Backspace so stops in the name and
    /// never reaches the canvas, where it closes the block.
    pub(crate) fn backspace_at_start(
        &mut self,
        block: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editing) = self
            .space
            .blocks
            .get(&block)
            .and_then(|b| b.checklist()?.editing.as_ref())
        else {
            return;
        };
        let (Field::Item(item), true) = (editing.field, editing.input.read(cx).value().is_empty())
        else {
            return;
        };
        let before = self.checklists.get(editing.list).and_then(|l| {
            let at = l.items.iter().position(|i| i.id == item)?;
            Some(l.items.get(at.checked_sub(1)?)?.id)
        });
        self.stop_editing(block, window, cx);
        let Some(before) = before else {
            self.edit_field(block, Field::Name, window, cx);
            let input = self
                .space
                .blocks
                .get(&block)
                .and_then(|b| Some(b.checklist()?.editing.as_ref()?.input.clone()));
            if let Some(input) = input {
                input.update(cx, |input, cx| {
                    let end = input.value().len();
                    input.set_selected_range(end..end, cx);
                });
            }
            return;
        };
        self.edit_field(block, Field::Item(before), window, cx);
    }

    /// Stops `block`'s field: its text is kept, but an item left empty goes (and a new one leaves
    /// no undo step). With the keyboard in the field, the block keeps it.
    pub(crate) fn stop_editing(
        &mut self,
        block: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editing) = self
            .space
            .checklist_mut(block)
            .and_then(|c| c.editing.take())
        else {
            return;
        };
        if editing.input.read(cx).focus_handle(cx).is_focused(window) {
            window.focus(&self.focus_handle, cx);
        }
        let list = editing.list;
        let blank = |item: u64, c: &Checklists| {
            c.get(list)
                .and_then(|l| l.item(item))
                .is_some_and(|i| i.text.trim().is_empty())
        };
        if let Field::Item(item) = editing.field
            && blank(item, &self.checklists)
        {
            self.checklists.remove_item(list, item, notes::now());
            // Back as it was before the step: the step was only that item.
            let same = |a: &crate::checklists::List, b: &crate::checklists::List| {
                a.id == b.id && a.name == b.name && a.items == b.items
            };
            if let (Some(step), Some(now)) =
                (self.checklists_undo.last(), self.checklists.get(list))
                && same(step, now)
                && let Some(step) = self.checklists_undo.pop()
            {
                self.checklists.restore(step);
            }
        }
        cx.notify();
    }

    /// An agent changed checklist `list`: what the person types next in its field is a step of its
    /// own, taken back before the agent's change.
    pub(super) fn agent_changed(&mut self, list: u64) {
        let Some((space, block)) = self.checklist_open_at(list) else {
            return;
        };
        let space = space.id.clone();
        if let Some(editing) = self
            .space_by_id_mut(&space)
            .and_then(|s| s.blocks.get_mut(&block))
            .and_then(Body::checklist_mut)
            .and_then(|c| c.editing.as_mut())
            .filter(|e| e.list == list)
        {
            editing.kept = false;
        }
    }

    /// The check box: checks item `item` of checklist `list`, or unchecks it.
    pub(crate) fn toggle_item(&mut self, list: u64, item: u64, cx: &mut Context<Self>) {
        self.edit_checklist(list, |c| {
            let done = c
                .get(list)
                .and_then(|l| l.item(item))
                .is_some_and(|i| i.done);
            c.set_done(list, item, !done, notes::now())
        });
        cx.notify();
    }

    /// A row's ×: deletes item `item` of checklist `list`.
    pub(crate) fn delete_item(&mut self, list: u64, item: u64, cx: &mut Context<Self>) {
        self.edit_checklist(list, |c| c.remove_item(list, item, notes::now()));
        cx.notify();
    }
}
