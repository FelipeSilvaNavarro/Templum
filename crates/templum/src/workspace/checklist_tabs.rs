//! Checklist blocks' tabs: new checklists, opening one where it is or in a block, closing a tab
//! (a blank checklist goes), and the changes to a checklist's content, each kept for Ctrl+Z
//! (`edit_checklist`). A checklist is open in one tab at most: opening it again shows that tab.

use gpui_kit::{Context, Window};
use templum_canvas::{BlockId, Point, Rect};

use crate::checklist::{self, Checklist};
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

    /// A checklist block with a new checklist, its top-left corner at `at` (world units).
    pub(crate) fn add_checklist_at(
        &mut self,
        at: Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.checklists.add(notes::now());
        self.add_checklist_block(at, vec![id], window, cx);
    }

    /// A checklist block of the default size at `at` showing `tabs`, focused.
    fn add_checklist_block(
        &mut self,
        at: Point,
        tabs: Vec<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space.pristine = false;
        let rect = Rect::new(at.x, at.y, checklist::SIZE.w, checklist::SIZE.h);
        let id = self.add_block(rect, Body::Checklist(Checklist { tabs, active: 0 }));
        self.focus_block(id, window, cx);
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
    #[cfg_attr(not(test), allow(dead_code))] // The Checklists dialog (plan Task 5) opens with it.
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
        if let Some(checklist) = self.space.checklist_mut(block) {
            checklist.tabs.push(id);
            checklist.active = checklist.tabs.len() - 1;
        }
        self.focus_block(block, window, cx);
    }

    /// The tab strip's +: a new checklist in a new tab of `block`.
    pub(crate) fn new_checklist_tab(
        &mut self,
        block: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.checklists.add(notes::now());
        self.add_checklist_tab(block, id, window, cx);
    }

    /// Shows the block's tab `index`.
    pub(crate) fn show_checklist_tab(
        &mut self,
        block: BlockId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
