# Checklist blocks: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A checklist block: app-wide checklists (`checklists.json`) shown as tabs in blocks, items checked, edited in
place and dragged, a Checklists dialog, and three tools a wired agent reads, checks and adds items with.

**Architecture:** The model (`checklists.rs`) is pure and serde, beside `notes.rs` and `board.rs` in the app crate.
The block (`checklist.rs`) holds only the ids of its tabs and the one field being edited; everything it draws is read
from `Workspace::checklists`, so every block showing a checklist updates at once. `workspace/checklist_tabs.rs` opens,
closes and edits, and keeps one checklist's content before each change for Ctrl+Z (`edit_checklist`). The note
block's tab strip and Notes dialog become shared components (`tab_strip.rs`, `list_dialog.rs`) rather than copies.

**Tech Stack:** Rust 2024; GPUI (gpui-fast 0.1.8) through gpui-kit `=0.7.3`: `div`, `on_drag`/`drag_over`/`on_drop`
(as `kanban.rs`), `group`/`group_hover`, `line_through`, `capture_action`, gpui-kit's single-line `Input`/`InputState`
(`InputEvent::{Change, PressEnter, Blur}`), `IconName::ListChecks`; serde; `templum-mcp`'s tools and verbs.

**Spec:** [docs/specs/checklist.md](../specs/checklist.md).

## Global Constraints

- Only `crates/templum` depends on gpui-kit; the model is headless and unit-tested.
- Zoom scales like a photo: every size in a checklist block is a world size × `z`, the field's included (its padding
  and height are set through `Input`'s style, which gpui-kit applies after its fixed sizes: gpui-component-0.7.3
  `src/input/input.rs:782-803`).
- A new checklist is named `"Checklist"`; a block is 320 × 360, at least 220 × 160; the tab strip 28 world units; item
  text 13, the name 15 world units.
- Ids are unique across checklists and items and never reused; `checklists.json` is `{ "version": 1, "next_id",
  "checklists": [{ "id", "name", "changed", "items": [{ "id", "text", "done" }] }] }`, an unchecked item writing no
  `done`.
- Undo: 50 steps, one checklist's content each; adding, removing a blank and deleting a checklist are not steps.
- Each commit passes `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

## Review Focus

1. **Editing stops for any reason** (Esc, Enter, a click on another row, another block, closing the tab or block, an
   agent deleting it meanwhile): the item's text is kept, a blank item goes, and no field is left pointing at an item
   that is gone. Task 3 pins Esc, Enter, a click on another row, and closing the tab while editing.
2. **An agent changes a checklist while the person edits one of its items**: the field keeps the person's text and
   caret; the agent's new items show under it. Task 6 pins `add_items` during an edit.
3. **A file edited by hand**: `next_id` lagging behind the ids, a missing `done`, `items` or `name`. Task 1 pins them.
4. **Ctrl+Z after the checklist was deleted in the dialog**: skipped, never resurrecting it into a tab. Task 2 pins it.
5. **The same checklist named twice in a saved canvas, or open in another block** (a hand-edited file, a paste):
   it opens in one tab at most. Task 4 pins the restore.

---

### Task 1: The model and `checklists.json`

**Files:**
- Create: `crates/templum/src/checklists.rs`
- Modify: `crates/templum/src/main.rs` (the `mod` line)

**Interfaces:**
- Consumes: `session::{read, write_newest, Loaded}`, as `notes.rs` uses them.
- Produces:
  - `pub(crate) const NAME: &str = "Checklist";`, `pub(crate) const VERSION: u32 = 1;`
  - `pub(crate) struct Checklists { pub version: u32, pub next_id: u64, pub lists: Vec<List> }` (`lists` serialized
    as `"checklists"`); `pub(crate) struct List { pub id: u64, pub name: String, pub changed: u64, pub items:
    Vec<Item> }`; `pub(crate) struct Item { pub id: u64, pub text: String, pub done: bool }`; all `Clone, Debug,
    PartialEq, Serialize, Deserialize`, `Checklists: Default`.
  - `impl Checklists`: `add(&mut self, now: u64) -> u64` (named `NAME`, no items); `get(&self, id: u64) ->
    Option<&List>`; `remove(&mut self, id: u64)`; `duplicate(&mut self, id: u64, now: u64) -> Option<u64>` (new ids for
    it and its items); `rename(&mut self, id: u64, name: &str, now: u64)`; `add_item(&mut self, list: u64, after:
    Option<u64>, text: &str, done: bool, now: u64) -> Option<u64>` (after `None` or an id not there: at the end);
    `set_text(&mut self, list: u64, item: u64, text: &str, now: u64)`; `set_done(&mut self, list: u64, item: u64,
    done: bool, now: u64) -> bool` (false: no such item); `remove_item(&mut self, list: u64, item: u64, now: u64)`;
    `move_item(&mut self, list: u64, item: u64, before: Option<u64>, now: u64)` (`None`: to the end); `find(&self,
    query: &str) -> Vec<&List>`; `restore(&mut self, before: List) -> bool` (replaces the list with its id; false when
    it is gone).
  - `impl List`: `progress(&self) -> (usize, usize)` (checked, all); `name_shown(&self) -> &str` (`NAME` when blank);
    `title(&self) -> String`; `is_blank(&self) -> bool`; `item(&self, id: u64) -> Option<&Item>`.
  - `pub(crate) fn parse_items(text: &str) -> Vec<(String, bool)>`; `path`, `load(dir: &Path) ->
    Loaded<Checklists>`, `save(dir: &Path, checklists: &Checklists, generation: u64) -> io::Result<()>`, as
    `notes.rs`'s.

- [ ] **Step 1: Write the failing tests** in `checklists.rs`'s `mod tests`:

```rust
#[test]
fn ids_are_never_reused_across_lists_and_items_and_a_copy_gets_new_ones() {
    let mut c = Checklists::default();
    let a = c.add(1);
    let i = c.add_item(a, None, "x", false, 1).unwrap();
    c.remove(a);
    let b = c.add(2);
    assert!(b > i && b > a);
    let j = c.add_item(b, None, "y", true, 2).unwrap();
    let copy = c.duplicate(b, 3).unwrap();
    let copied = c.get(copy).unwrap();
    assert_eq!(copied.name, NAME);
    assert_eq!((copied.items[0].text.as_str(), copied.items[0].done), ("y", true));
    assert!(copied.items[0].id > j && copy > j);
    assert_eq!(c.duplicate(a, 4), None);
}

#[test]
fn an_item_goes_after_another_or_at_the_end_and_moves_before_one_or_to_the_end() {
    let mut c = Checklists::default();
    let l = c.add(1);
    let a = c.add_item(l, None, "a", false, 1).unwrap();
    let b = c.add_item(l, None, "b", false, 1).unwrap();
    let x = c.add_item(l, Some(a), "x", false, 1).unwrap();
    let texts = |c: &Checklists| c.get(l).unwrap().items.iter().map(|i| i.text.clone()).collect::<Vec<_>>();
    assert_eq!(texts(&c), ["a", "x", "b"]);
    c.move_item(l, b, Some(a), 2);
    assert_eq!(texts(&c), ["b", "a", "x"]);
    c.move_item(l, b, None, 3);
    assert_eq!(texts(&c), ["a", "x", "b"]);
    c.move_item(l, x, Some(x), 4);
    assert_eq!(texts(&c), ["a", "x", "b"], "onto itself: stays");
}

#[test]
fn every_change_moves_its_time_and_progress_counts_the_checked() {
    let mut c = Checklists::default();
    let l = c.add(1);
    let a = c.add_item(l, None, "a", false, 2).unwrap();
    c.add_item(l, None, "b", false, 2);
    assert!(c.set_done(l, a, true, 5));
    assert_eq!(c.get(l).unwrap().changed, 5);
    assert!(!c.set_done(l, 999, true, 6));
    assert_eq!(c.get(l).unwrap().progress(), (1, 2));
    c.rename(l, "Deploy", 7);
    assert_eq!(c.get(l).unwrap().title(), "Deploy 1/2");
}

#[test]
fn the_title_is_the_name_and_the_progress_once_there_are_items() {
    let mut c = Checklists::default();
    let l = c.add(1);
    assert_eq!(c.get(l).unwrap().title(), "Checklist");
    c.rename(l, "  ", 2);
    assert_eq!(c.get(l).unwrap().name_shown(), "Checklist");
    c.add_item(l, None, "", false, 3);
    assert_eq!(c.get(l).unwrap().title(), "Checklist 0/1");
}

#[test]
fn a_list_is_blank_with_the_default_name_and_no_item_text() {
    let mut c = Checklists::default();
    let l = c.add(1);
    let i = c.add_item(l, None, "  ", false, 1).unwrap();
    assert!(c.get(l).unwrap().is_blank());
    c.set_text(l, i, "milk", 2);
    assert!(!c.get(l).unwrap().is_blank());
    c.set_text(l, i, "", 3);
    c.rename(l, "Groceries", 4);
    assert!(!c.get(l).unwrap().is_blank());
}

#[test]
fn find_takes_the_name_and_the_items_in_any_case_newest_first() {
    let mut c = Checklists::default();
    let old = c.add(1);
    c.rename(old, "Buy MILK", 1);
    let new = c.add(5);
    c.add_item(new, None, "milk tea", false, 5);
    c.add(9);
    let ids = |found: Vec<&List>| found.iter().map(|l| l.id).collect::<Vec<_>>();
    assert_eq!(ids(c.find(" Milk ")), [new, old]);
    assert_eq!(c.find("").len(), 3);
}

#[test]
fn restore_puts_back_a_list_that_is_there_and_refuses_a_gone_one() {
    let mut c = Checklists::default();
    let l = c.add(1);
    let before = c.get(l).unwrap().clone();
    c.add_item(l, None, "a", false, 2);
    assert!(c.restore(before.clone()));
    assert_eq!(c.get(l), Some(&before));
    c.remove(l);
    assert!(!c.restore(before));
    assert!(c.get(l).is_none());
}

#[test]
fn parse_items_takes_each_line_with_text_and_drops_list_markers() {
    assert_eq!(
        parse_items("- [ ] Build\n\n* [x] Test\n  [X] Ship  \n- \n1. Plain"),
        [("Build".into(), false), ("Test".into(), true), ("Ship".into(), true), ("1. Plain".into(), false)]
    );
}

#[test]
fn checklists_read_back_equal_and_a_hand_edited_file_is_read() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Checklists::default();
    let l = c.add(7);
    c.add_item(l, None, "a", true, 7);
    c.add_item(l, None, "b", false, 7);
    save(d.path(), &c, 1).unwrap();
    let text = std::fs::read_to_string(path(d.path())).unwrap();
    assert_eq!(text.matches("\"done\"").count(), 1, "an unchecked item writes no done");
    assert_eq!(load(d.path()), Loaded::Saved(c));
    std::fs::write(path(d.path()), r#"{"version":1,"checklists":[{"id":4,"items":[{"id":9,"text":"x"}]},{"id":2}]}"#).unwrap();
    let Loaded::Saved(read) = load(d.path()) else { panic!("not read") };
    assert_eq!(read.next_id, 9, "missing: the largest id, items' too");
    assert!(!read.lists[0].items[0].done);
    assert!(read.lists[1].items.is_empty() && read.lists[1].name.is_empty());
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum checklists::`
Expected: compile error (the module doesn't exist).

- [ ] **Step 3: Implement `checklists.rs`**

As `notes.rs`: `#[serde(default)]` on every field but ids; `done` with `skip_serializing_if = "std::ops::Not::not"`.
A title is `name_shown()` cut at 60 characters with "…" (as `notes::title`), then ` {checked}/{all}` when there are
items. `parse_items`: each line trimmed, then a leading `- ` or `* ` dropped, then a leading `[ ]`, `[x]` or `[X]`
dropped (the `x` sets done), trimmed again; empty results skipped. `load` raises `next_id` to the largest id of lists
and items. Add `mod checklists;` to `main.rs`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum checklists::`
Expected: PASS (unused-code warnings are fine until Task 2 uses it; clippy runs at the commit with `#[cfg_attr(not(test), allow(dead_code))]` on the module if needed, removed in Task 2).

- [ ] **Step 5: Commit**

```bash
git add crates/templum/src/checklists.rs crates/templum/src/main.rs
git commit -m "chore(checklist): add the checklist model and checklists.json"
```

---

### Task 2: A checklist block, its tabs and its rows

Opens from the toolbar and Add ▸, shows the name and the items, checks and deletes items, tabs, blank checklists go,
Ctrl+Z, its menu. No editing yet (Task 3), no saving (Task 4).

**Files:**
- Create: `crates/templum/src/checklist.rs`, `crates/templum/src/tab_strip.rs`,
  `crates/templum/src/workspace/checklist_tabs.rs`, `crates/templum/src/workspace/tests/checklists.rs`
- Modify: `crates/templum/src/note.rs` (its tab strip and header button move out), `block.rs`, `main.rs`,
  `workspace/mod.rs`, `workspace/blocks.rs` (`remove_block`), `workspace/input.rs` (Ctrl+Z), `workspace/bus.rs`
  (`about`), `workspace/selection.rs` (paste), `session.rs`, `workspace/persist.rs` (`add_saved`),
  `spaces_screen.rs`, `menu/mod.rs`, `menu/build.rs`, `toolbar.rs`, `workspace/tests/mod.rs` (`mod checklists;`)

**Interfaces:**
- Consumes: Task 1's `Checklists`, `List`, `Item`.
- Produces:
  - `checklist.rs`: `pub(crate) const SIZE: Size = Size { w: 320.0, h: 360.0 }`, `MIN: Size = Size { w: 220.0, h:
    160.0 }`; `pub(crate) struct Checklist { pub tabs: Vec<u64>, pub active: usize, pub editing: Option<Editing> }`
    with `shown(&self) -> Option<u64>`, `title(&self, ws: &Workspace) -> SharedString` ("Checklists" with no tab,
    else the list's `title()`), `focus(&self, window, cx) -> bool` and `has_focus(&self, window, cx) -> bool` (the
    field's input; false until Task 3); `pub(crate) struct Editing { pub list: u64, pub field: Field, pub input:
    Entity<InputState>, pub kept: bool }`; `#[derive(Clone, Copy, Debug, PartialEq)] pub(crate) enum Field { Name,
    Item(u64) }`; `pub(crate) fn body(ws: &Workspace, block: BlockId, checklist: &Checklist, z: f32, cx: &mut
    Context<Workspace>) -> AnyElement`.
  - `tab_strip.rs`: `pub(crate) fn tab_strip(ws: &Workspace, block: BlockId, prefix: &'static str, titles:
    Vec<SharedString>, active: usize, z: f32, show: fn(&mut Workspace, BlockId, usize, &mut Window, &mut
    Context<Workspace>), close: <same>, new: fn(&mut Workspace, BlockId, &mut Window, &mut Context<Workspace>), cx: &mut
    Context<Workspace>) -> AnyElement`, the note's strip as it is, its element ids `{prefix}-tab-{n}-{i}`,
    `{prefix}-tab-close-{n}-{i}`, `({prefix}-tab-new, n)`, `({prefix}-tabs, n)` (the note passes `"note"`, keeping
    today's ids).
  - `block.rs`: `pub(crate) fn header_button(ws: &Workspace, id: ElementId, icon: IconName, tip: &'static str, z:
    f32, click: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static, cx: &mut
    Context<Workspace>) -> AnyElement` (the note's `notes_button` body; the note uses it).
  - `Body::Checklist(Checklist)` with `checklist()`/`checklist_mut()`; `session::Item::Checklist(ChecklistItem {
    uid: String, rect: SavedRect, checklists: Vec<u64>, active: usize })` (`kind: "checklist"`, `checklists` skipped
    when empty).
  - `Workspace`: `pub(crate) checklists: Checklists`, `checklists_undo: Vec<List>` (at most `CHECKLIST_UNDO = 50`).
  - `checklist_tabs.rs` (`impl Workspace`): `pub(crate) fn new_checklist(&mut self, window, cx)`, `pub(crate) fn
    add_checklist_at(&mut self, at: Point, window, cx)`, `pub(crate) fn checklist_open_at(&self, id: u64) ->
    Option<(&Space, BlockId)>`, `pub(crate) fn open_checklist(&mut self, id: u64, into: Option<BlockId>, window, cx)`,
    `pub(crate) fn new_checklist_tab(&mut self, block: BlockId, window, cx)`, `pub(crate) fn show_checklist_tab(&mut
    self, block: BlockId, index: usize, window, cx)`, `pub(crate) fn close_checklist_tab(&mut self, block: BlockId,
    index: usize, window, cx)`, `pub(super) fn drop_checklist_if_blank(&mut self, id: u64)`, `pub(crate) fn
    edit_checklist<R>(&mut self, list: u64, f: impl FnOnce(&mut Checklists) -> R) -> R`, `pub(super) fn
    undo_checklists(&mut self, cx: &mut Context<Self>) -> bool`, `pub(crate) fn toggle_item(&mut self, list: u64,
    item: u64, cx)`, `pub(crate) fn delete_item(&mut self, list: u64, item: u64, cx)`.

- [ ] **Step 1: Write the failing tests** in `workspace/tests/checklists.rs` (helpers: `new_checklist(handle, ws, cx)
  -> BlockId` clicking the toolbar's `"tool-checklist"` as `notes.rs`'s `new_note` presses its key; `shown(ws, cx,
  block) -> u64`):

```rust
#[gpui_kit::test]
fn the_toolbar_opens_a_block_with_a_new_checklist(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_checklist(handle, &ws, cx);
    ws.read_with(cx, |ws, cx| {
        let rect = ws.space.canvas.get(id).unwrap().rect;
        assert_eq!((rect.w, rect.h), (320.0, 360.0));
        let list = ws.checklists.get(shown(ws, id)).unwrap();
        assert_eq!(list.name, "Checklist");
        assert_eq!(crate::block::title(ws, &ws.space.blocks[&id], cx).as_ref(), "Checklist");
    });
}

#[gpui_kit::test]
fn the_box_checks_an_item_and_the_title_counts_it(cx: &mut TestAppContext) {
    // Two items added through the model ("a", "b"), a frame drawn, then a click on
    // ("checklist-box", n, a)'s bounds (window.bounds_of / the test-support id): the item is done and the block's
    // title reads "Checklist 1/2"; a second click unchecks it.
}

#[gpui_kit::test]
fn the_rows_x_deletes_its_item(cx: &mut TestAppContext) { /* click checklist-item-delete-{n}-{a}: gone */ }

#[gpui_kit::test]
fn closing_a_blank_checklists_tab_removes_it_and_a_named_one_stays(cx: &mut TestAppContext) {
    // + opens a second tab (new_checklist_tab); rename the first to "Groceries"; close both tabs: the named one is
    // still in ws.checklists, the blank one is not; closing the block drops its blank checklists too.
}

#[gpui_kit::test]
fn ctrl_z_on_a_checklist_block_undoes_a_check_and_skips_a_deleted_checklist(cx: &mut TestAppContext) {
    // toggle_item(a); press ctrl-z (cmd-z on macOS) with the block focused: a is unchecked again.
    // Then toggle a in a second checklist, remove that checklist from ws.checklists, toggle a again in the first,
    // ctrl-z twice: the first's check is undone, the second never comes back (ws.checklists.get(second) is None).
}

#[gpui_kit::test]
fn a_checklist_is_open_in_one_tab_at_most(cx: &mut TestAppContext) {
    // open_checklist(id, Some(other_block)) for an id open in the first block: no new tab, the first block's tab
    // shows it and the first block is focused.
}

#[gpui_kit::test]
fn a_pasted_checklist_block_shows_copies(cx: &mut TestAppContext) {
    // select the block, Copy and Paste (as tests/selection.rs does for notes): the new block's tab is a different id
    // whose list has the same name and items.
}
```

`session.rs`'s tests: `a_canvas_with_a_checklist_reads_back_equal` (an `Item::Checklist` with `checklists: vec![3,
5], active: 1` written and read; written without `checklists` when empty).

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum checklist`
Expected: compile errors (`Body::Checklist`, `new_checklist` missing).

- [ ] **Step 3: Move the note's tab strip to `tab_strip.rs` and its header button to `block.rs::header_button`**,
  the note calling them with `"note"`, `show_note_tab`, `close_note_tab`, `new_note_tab` and `open_notes`. Run
  `cargo test -p templum notes` — the note tests pass unchanged.

- [ ] **Step 4: Implement the block**

- `checklist.rs::body`: the tab strip (`"checklist"`, titles from each tab's list `title()`); with no tab, "No
  checklist open here" with **New checklist** and **Open a checklist…** (`note::button`, made `pub(crate)`; the second
  is a no-op until Task 5); else, in a `overflow_y_scroll` column stopping the wheel unless Ctrl (as `note::body`):
  the name (`name_shown()`, 15 × z, `FontWeight::SEMIBOLD`, id `("checklist-name", n)`), one row per item (id
  `checklist-row-{n}-{item}`, `group` of that name: a 14 × z box `("checklist-box", ...)` bordered `ui.hairline`,
  filled `ui.accent` with `IconName::Check` when done; the text 13 × z, `line_through` and `ui.title_dim` when done,
  id `checklist-text-{n}-{item}`; a `IconName::Close` button `checklist-item-delete-{n}-{item}` shown on the group's
  hover), then **+ Add an item** (`("checklist-add", n)`, adding an item at the end through `edit_checklist`). Paddings
  and gaps in world units × z: rows `py 4`, `gap 8`, column `px 12`, `py 8`.
- `block.rs`: font `0.0` and level `Level::Full` (the kanban's arms), the body arm, the header button (`ListChecks`,
  "All checklists", id `("checklists", n)`, a no-op until Task 5), `title`.
- `workspace/mod.rs`: every `Body` match; `item` gives `ChecklistItem`; the `Workspace` fields and constructor.
- `checklist_tabs.rs`: as `note_tabs.rs`, a tab being an id; `edit_checklist` clones `self.checklists.get(list)`
  before `f`, pushes it when the list changed (dropping the oldest past 50); `undo_checklists` pops until
  `self.checklists.restore(before)` is true.
- `blocks.rs::remove_block` drops a closed block's blank checklists; `input.rs` routes Ctrl+Z to
  `undo_checklists` when the focused block is a checklist and `!has_focus` (the field's own Ctrl+Z wins);
  `bus.rs::about` gives `"checklist"`; `selection.rs` pastes copies (`duplicate`); `persist.rs::add_saved` restores
  the tabs that exist and aren't open elsewhere, `active` by id; `spaces_screen.rs` counts `checklists` ("1
  checklist", "2 checklists", after boards).
- Menus: `Items::Checklist { block }` (the grid's items and Close, as every block; "All checklists…" comes in Task 5);
  `adding`'s Add ▸ **Checklist** (`ListChecks`, `add_checklist_at`) after Note; the toolbar's `"tool-checklist"`
  button after Note, tooltip "Checklist", calling `new_checklist`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p templum`
Expected: PASS, the note, kanban and menu tests included.

- [ ] **Step 6: Commit**

```bash
git add -A crates/templum
git commit -m "feat(checklist): add checklist blocks: tabs of checklists you tick, from the toolbar and Add"
```

---

### Task 3: Editing the name and the items in place

**Files:**
- Modify: `crates/templum/src/checklist.rs`, `crates/templum/src/workspace/checklist_tabs.rs`,
  `crates/templum/src/workspace/tests/checklists.rs`, `docs/specs/checklist.md` (one line, below)

**Interfaces:**
- Consumes: Task 2's `Checklist`, `Editing`, `Field`, `edit_checklist`.
- Produces: `pub(crate) fn edit_field(&mut self, block: BlockId, field: Field, window, cx)` and `pub(crate) fn
  stop_editing(&mut self, block: BlockId, window, cx)` in `checklist_tabs.rs`; a new block and a new tab start with
  `Field::Name` editing; `Checklist::focus`/`has_focus` now use the field.

- [ ] **Step 1: Write the failing tests**

```rust
#[gpui_kit::test]
fn a_new_checklist_edits_its_name_and_enter_keeps_it(cx: &mut TestAppContext) {
    // new_checklist; the field has the keyboard; select all, type "Deploy", press enter: name "Deploy", no field.
}

#[gpui_kit::test]
fn enter_in_an_item_adds_the_next_and_every_key_is_kept(cx: &mut TestAppContext) {
    // click ("checklist-add", n); type "Build" (the list shows "Build" before Enter); enter; type "Test"; esc:
    // items ["Build", "Test"], no field.
}

#[gpui_kit::test]
fn backspace_in_an_empty_item_deletes_it_and_edits_the_one_before(cx: &mut TestAppContext) {
    // items "a" and an empty one being edited; backspace: one item "a", editing Field::Item(a), caret at its end
    // (InputState::cursor() == 1).
}

#[gpui_kit::test]
fn an_item_left_empty_goes_and_enter_in_an_empty_item_stops(cx: &mut TestAppContext) {
    // + Add an item, enter at once: no item, no field. + Add an item, type "x", click another row's text: "x" kept,
    // that row is edited. Edit "x", delete its text, esc: "x"'s item is gone.
}

#[gpui_kit::test]
fn one_ctrl_z_takes_back_an_edits_typing(cx: &mut TestAppContext) {
    // edit item "a", type "bc" (text "abc"), esc, ctrl-z with the block focused: "a".
    // + Add an item, type "new", esc, ctrl-z: the item is gone (the add was the step).
}

#[gpui_kit::test]
fn closing_the_tab_while_editing_keeps_the_text(cx: &mut TestAppContext) {
    // rename the checklist "Groceries" (so it stays), edit an item, type "milk", close the tab: the list holds "milk".
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum checklists::`
Expected: FAIL (no field opens).

- [ ] **Step 3: Implement the field**

- `edit_field` stops any field first, then makes an `InputState` with the name or the item's text, subscribes to
  it: `Change` writes the text (the first change through `edit_checklist`, then `kept = true`, later ones straight
  to `self.checklists`); `PressEnter` on the name stops, on an empty item stops, on an item adds an empty item after it
  through `edit_checklist` and edits it with `kept = true` (the add is the step); `Blur` stops, but only when the
  block's `editing` still holds that input. It gives the field the keyboard (`focus_block` then the input's focus).
- `stop_editing` takes the field; a blank item is removed straight from `self.checklists`, and an undo step that now
  equals the list is dropped (a new item left empty leaves no step).
- The row of the field being edited draws `Input::new(&input).appearance(false).px(px(0.)).py(px(0.)).h(px(LINE *
  z)).text_size(px(13.0 * z))` in place of the text (one line), the name's field likewise at 15; `LINE` is the row's
  text line height (1.5 × 13, as the rows' divs use `line_height(relative(1.5))`). The field's wrapper captures the
  `Backspace` action (gpui-kit's input action; check its path in gpui-base-0.7.3 `src/input/base/state.rs:78`) when the
  text is empty, and the `escape` key, and stops them there.
- A click on the name or an item's text calls `edit_field`; `close_checklist_tab`, `remove_block` and Task 5's delete
  stop a field on that list first.
- Spec, under **Editing an item**: "**Enter** in an empty item stops editing (the item goes)."

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum`
Expected: PASS.

- [ ] **Step 5: Verify by hand** at zoom 0.5, 1 and 2 (`cargo run -p templum`): a one-line item's text stays where
  the row's was while it is edited; a long item's field scrolls sideways.

- [ ] **Step 6: Commit**

```bash
git add -A crates/templum docs/specs/checklist.md
git commit -m "feat(checklist): edit a checklist's name and items in place, Enter adding the next"
```

---

### Task 4: Saving checklists

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs`, `workspace/spaces.rs` (`load_spaces`), `workspace/persist.rs`
  (`snapshot`, `snapshot_done`, `save_canvas_now`), `workspace/tests/checklists.rs`

**Interfaces:**
- Consumes: Task 1's `load`/`save`.
- Produces: `Workspace::checklists_saved: Option<Checklists>`, `checklists_frozen: bool`, as the boards'.

- [ ] **Step 1: Write the failing tests**

```rust
#[gpui_kit::test]
fn checklists_and_their_tabs_come_back_after_a_restart(cx: &mut TestAppContext) {
    // open_in(cx, None, Some(dir)); a block with two tabs, the second shown, "Deploy" with an item checked;
    // save_canvas_now; open_in again: the block shows the same two ids, active 1, the list's items and done back.
}

#[gpui_kit::test]
fn a_checklist_named_twice_in_a_canvas_opens_once(cx: &mut TestAppContext) {
    // write a checklists.json with list 3 and a canvas (write_canvas) with two checklist items naming 3, and one
    // naming 3 twice: after open_in, 3 is in one tab only.
}

#[gpui_kit::test]
fn an_unparsable_checklists_json_is_set_aside_and_the_canvas_still_saves(cx: &mut TestAppContext) {
    // write "{" as checklists.json; open_in: checklists.json.bak exists, ws.checklists is empty; a terminal moved,
    // save_canvas_now: the canvas file is written.
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum checklists::`
Expected: FAIL (nothing is written).

- [ ] **Step 3: Implement** as `boards.json` throughout: `load_spaces` reads it (`Moved` warns "Couldn't read the
  checklists (…); they were kept as …", `Unreadable` freezes them with "…; they won't be saved this session"); the
  snapshot writes it in the background beside the boards when it changed, and `save_canvas_now` on quit; canvases
  never wait on it.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/templum
git commit -m "feat(checklist): keep checklists and their blocks across restarts"
```

---

### Task 5: The Checklists dialog

**Files:**
- Create: `crates/templum/src/list_dialog.rs`
- Modify: `crates/templum/src/note.rs` (`open_notes` uses it), `checklist.rs`, `workspace/checklist_tabs.rs`,
  `block.rs` (the header button opens it), `menu/mod.rs`, `menu/build.rs`, `workspace/tests/checklists.rs`

**Interfaces:**
- Consumes: Task 2's `open_checklist`, `new_checklist_tab`, `new_checklist`, `checklist_open_at`.
- Produces:
  - `list_dialog.rs`: `pub(crate) struct Row { pub id: u64, pub title: String, pub detail: String, pub dot:
    Option<Hsla> }`; `pub(crate) struct Kind { pub title: &'static str, pub prefix: &'static str, pub find: &'static
    str, pub none: &'static str, pub no_match: &'static str, pub new: &'static str }`; `pub(crate) fn open(kind: Kind,
    rows: fn(&Workspace, &str) -> Vec<Row>, open: fn(&mut Workspace, u64, &mut Window, &mut Context<Workspace>),
    delete: <same>, new: fn(&mut Workspace, &mut Window, &mut Context<Workspace>), window, cx: &mut
    Context<Workspace>)`, the Notes dialog's body as it is (row ids `{prefix}-row-{id}`, `{prefix}-delete-{id}`,
    `{prefix}s-find`, `{prefix}s-list`, `{prefix}s-new`, the trash's second click, "Delete for good"). `open`, `delete`
    and `new` are closures in practice (they carry `into`): take them as `impl Fn(...) + Clone + 'static` instead of
    `fn` where the block is captured.
  - `checklist.rs`: `pub(crate) fn open_checklists(into: Option<BlockId>, window, cx: &mut Context<Workspace>)`;
    `checklist_tabs.rs`: `pub(crate) fn delete_checklist(&mut self, id: u64, window, cx)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[gpui_kit::test]
fn the_dialog_lists_checklists_newest_first_and_finds_by_item_text(cx: &mut TestAppContext) {
    // two named checklists, the second with item "milk"; open_checklists(None); the rows checklist-row-{a} and
    // -{b} are drawn, b first; type "MILK" in checklists-find: only b's row.
}

#[gpui_kit::test]
fn the_dialog_opens_one_where_it_is_open_and_deletes_one_from_its_blocks(cx: &mut TestAppContext) {
    // a checklist open in block A; from a dialog asked from block B, click its row: A's tab shows it, no tab in B.
    // Its trash twice ("Delete for good" after the first): gone from ws.checklists and from A's tabs.
}
```

The note tests of the Notes dialog pass unchanged (`cargo test -p templum notes`).

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum checklists::`
Expected: FAIL.

- [ ] **Step 3: Implement** `list_dialog::open` from `open_notes`'s body; `open_notes` becomes a call with the notes'
  rows (the color dot, "5 min ago · Open in Workspace 2 · its next line"); `open_checklists` with the checklists'
  (`detail`: "2/5 · 5 min ago · Open", the progress left out with no items; dialog title "Checklists", find "Find a
  checklist", none "No checklists yet.", no match "No checklist holds that.", new "New checklist"). Wire **Open a
  checklist…**, the header button, **All checklists…** (`Items::Checklist`, `ListChecks`) and the canvas menu's
  **Checklists…** after Notes….

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/templum
git commit -m "feat(checklist): find, open and delete checklists in the Checklists dialog"
```

---

### Task 6: Dragging items

**Files:**
- Modify: `crates/templum/src/checklist.rs`, `crates/templum/src/kanban.rs` (`DragPreview` becomes `pub(crate)`)

**Interfaces:**
- Consumes: Task 1's `move_item`, Task 2's `edit_checklist`.
- Produces: `struct ItemDrag { list: u64, item: u64 }` in `checklist.rs`.

- [ ] **Step 1: Implement** as `kanban.rs`'s cards: each row `.on_drag(ItemDrag, DragPreview { text, width: (content
  width − 24) × z, z, ui })`, `.drag_over::<ItemDrag>` (accent border), `.on_drop` moving the dragged item before it
  (stop propagation); the list's column and **+ Add an item** take a drop at the end. A drag from another list is
  ignored. A click still edits (GPUI starts a drag only after the pointer moves).

- [ ] **Step 2: Verify by hand** (`cargo run -p templum`, Hyprland) at zoom 0.5, 1 and 2: drag an item onto another,
  onto the empty space under the rows, onto + Add an item; the preview follows the pointer at the zoom; Ctrl+Z puts
  it back. The order's logic is Task 1's `move_item` tests; GPUI's drag isn't driven headlessly here (the kanban's
  isn't either).

- [ ] **Step 3: Run the tests**

Run: `cargo test -p templum`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A crates/templum
git commit -m "feat(checklist): drag checklist items to reorder them"
```

---

### Task 7: The agents' tools

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs`, `crates/templum-mcp/src/cli.rs`, `crates/templum/src/workspace/content.rs`,
  `crates/templum/src/workspace/bus.rs` (dispatch, the instructions), `crates/templum/src/workspace/tests/content.rs`

**Interfaces:**
- Consumes: Task 1's `List`, `parse_items`, `set_done`, `add_item`; Task 2's `edit_checklist`.
- Produces: `Call::ReadChecklist { block: String }`, `Call::CheckItem { block: String, item: u64, done: bool }`,
  `Call::AddItems { block: String, text: String }`; `Workspace::{read_checklist, check_item, add_items}` in
  `content.rs`.

- [ ] **Step 1: Write the failing tests**

`tools.rs`: in `every_tool_listed_parses_its_example`, after `append_note`: `("read_checklist", json!({ "block":
"4qm2a" }))`, `("check_item", json!({ "block": "4qm2a", "item": 13, "done": false }))`, `("add_items", json!({
"block": "4qm2a", "text": "- [ ] Build\n- [ ] Test" }))`; and:

```rust
assert_eq!(parse("check_item", &json!({ "block": "c", "item": "13" })),
           Ok(Call::CheckItem { block: "c".into(), item: 13, done: true }));
for bad in [json!({ "block": "c" }), json!({ "block": "c", "item": "x" }), json!({ "item": 1 })] {
    assert!(parse("check_item", &bad).is_err(), "{bad}");
}
assert!(parse("add_items", &json!({ "block": "c" })).is_err());
```

`cli.rs`'s verb table: `["checklist", "read", "4qm2a"]` → `read_checklist {block}`; `["checklist", "add", "4qm2a",
"Run", "tests"]` → `add_items {block, text: "Run tests"}`; `["checklist", "check", "4qm2a", "13"]` → `check_item {block,
item: "13"}`; `["checklist", "uncheck", "4qm2a", "13"]` → `check_item {block, item: "13", done: false}`.

`tests/content.rs` (helper `wired_checklist(handle, ws, cx) -> (String, BlockId, String, u64)`: a checklist block with
"Deploy", items "Build" (checked) and "Run the tests", wired to the startup terminal):

```rust
#[gpui_kit::test]
fn read_checklist_reads_the_checklist_on_screen(cx: &mut TestAppContext) {
    // (false, "Deploy (1/2)\n{b} [x] Build\n{t} [ ] Run the tests"); on the caller's own terminal:
    // failed, "isn't a checklist".
}

#[gpui_kit::test]
fn check_item_and_add_items_show_at_once_and_ctrl_z_takes_them_back(cx: &mut TestAppContext) {
    // check_item {item: t} → "Item {t} is checked."; check_item {item: 999} → failed, "There's no item 999 in
    // \"Deploy\". read_checklist shows its items."; add_items {"- [ ] Ship\n[x] Tag"} → "Added items {s}, {g}.",
    // Tag done; a blank text → failed "There's nothing to add."; ctrl-z with the block focused twice: Ship and Tag
    // gone, then t unchecked.
}

#[gpui_kit::test]
fn add_items_while_the_person_edits_keeps_their_field(cx: &mut TestAppContext) {
    // the person edits "Build" (caret after "Bu"); add_items "Ship": the field still holds "Build", its caret at 2,
    // "Ship" is the last item.
}
```

and `read_checklist`, `check_item`, `add_items` added to `the_content_tools_are_refused_without_a_wire`, and the
instructions test checking "check the items of a connected checklist as you finish them".

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-mcp && cargo test -p templum content::`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `tools.rs`: the three entries after `append_note` (`item`: `{ "type": "integer", "description": "An item's id, as
  read_checklist shows it." }`; `done`: boolean, default true); descriptions "The checklist a checklist block connected
  to you shows: its name and progress, then each item's id, [x] or [ ], and text.", "Checks an item of the checklist a
  checklist block connected to you shows; unchecks it with done false.", "Adds items at the end of the checklist a
  checklist block connected to you shows: one per line with text; a leading - or *, then [ ] or [x], is dropped ([x]
  adds it checked). The person sees them at once and can take them back."; parsing with `id("item")` (its message
  "\"item\" is an id, as read_checklist shows it") and `flag`-like `done` defaulting to true.
- `cli.rs`: `"checklist"` in `VERBS`, `const CHECKLIST: &str = "checklist read <block> | add <block> <text> | check
  <block> <item> | uncheck <block> <item> [--json]"`, the four arms and `("checklist", _) => return Err(CHECKLIST)`.
- `content.rs`: `fn wired_checklist(&self, from: &str, block: &str) -> Result<u64, Reply>` (the list on screen; "{block}
  isn't a checklist." / "{block} shows no checklist."); `read_checklist` (through `tools::tail(…, MAX_TEXT)`),
  `check_item` ("Item {id} is checked." / "… unchecked.") and `add_items` ("Added item 14." / "Added items 14, 15.")
  each through one `edit_checklist`, then `cx.notify()`. An agent's change never touches the field being edited.
- `bus.rs`: dispatch; the instructions' sentence becomes "…write what the next agent needs to know in the connected
  note, and check the items of a connected checklist as you finish them. A task you hand over…".

- [ ] **Step 4: Run the tests**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "feat(connections): let agents read a wired checklist, check its items and add new ones"
```

---

### Task 8: Docs, benches, the full check

**Files:**
- Modify: `docs/ARCHITECTURE.md` (the `Body` kinds sentence, `checklist.rs`, `checklists.rs`, `checklist_tabs.rs`,
  `tab_strip.rs`, `list_dialog.rs`), `docs/specs/connections.md` (the three Tools rows, the instructions' sentence, the
  verb row), `docs/specs/context-menus.md` (Add ▸ Checklist, Checklists…, the checklist block's menu),
  `docs/specs/level-of-detail.md` (a checklist block drawn in full, as the kanban), `docs/specs/checklist.md`
  (Status: Implemented), `docs/minors.md` (anything deferred)

- [ ] **Step 1: Update the docs** listed above, each in the file's own style.
- [ ] **Step 2: Run the benches** on `--release`: `cargo run --release -p templum -- --bench` and `-- --bench-mixed`;
  both `p95_ms` ≤ 16.7. Keep their JSON lines for the report.
- [ ] **Step 3: Run the full check**: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings &&
  cargo test --workspace`. Expected: clean, all tests pass.
- [ ] **Step 4: Commit**

```bash
git add docs
git commit -m "docs: document checklist blocks"
```
