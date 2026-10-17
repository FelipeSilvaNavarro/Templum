//! The checklists: lists of items you tick, app-wide like the notes, kept in `checklists.json`.
//! A checklist block shows some of them as tabs; closing a tab or the block keeps a checklist
//! that isn't blank, and only the Checklists dialog deletes one (docs/specs/checklist.md).

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::session::{self, Loaded};

/// A new checklist's name, and what a blank one reads.
pub(crate) const NAME: &str = "Checklist";
/// The file format's version: raised only when an older Templum couldn't read the new files.
pub(crate) const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Checklists {
    pub version: u32,
    /// The last id given out, to a checklist or an item: never reused.
    #[serde(default)]
    pub next_id: u64,
    #[serde(default, rename = "checklists")]
    pub lists: Vec<List>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct List {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    /// When it last changed, in seconds since the Unix epoch.
    #[serde(default)]
    pub changed: u64,
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Item {
    pub id: u64,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
}

impl Default for Checklists {
    fn default() -> Self {
        Checklists {
            version: VERSION,
            next_id: 0,
            lists: Vec::new(),
        }
    }
}

impl Checklists {
    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// A new checklist named NAME, with no items; returns its id.
    pub fn add(&mut self, now: u64) -> u64 {
        let id = self.new_id();
        self.lists.push(List {
            id,
            name: NAME.to_string(),
            changed: now,
            items: Vec::new(),
        });
        id
    }

    pub fn get(&self, id: u64) -> Option<&List> {
        self.lists.iter().find(|l| l.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut List> {
        self.lists.iter_mut().find(|l| l.id == id)
    }

    pub fn remove(&mut self, id: u64) {
        self.lists.retain(|l| l.id != id);
    }

    /// A copy of checklist `id`, with new ids for it and its items (a pasted block's); None when
    /// it is gone.
    pub fn duplicate(&mut self, id: u64, now: u64) -> Option<u64> {
        let mut list = self.get(id)?.clone();
        list.id = self.new_id();
        for item in &mut list.items {
            item.id = self.new_id();
        }
        list.changed = now;
        let id = list.id;
        self.lists.push(list);
        Some(id)
    }

    pub fn rename(&mut self, id: u64, name: &str, now: u64) {
        if let Some(list) = self.get_mut(id)
            && list.name != name
        {
            list.name = name.to_string();
            list.changed = now;
        }
    }

    /// A new item right after item `after`, at the end when that is None or not in the list;
    /// returns its id, None when the list is gone.
    pub fn add_item(
        &mut self,
        list: u64,
        after: Option<u64>,
        text: &str,
        done: bool,
        now: u64,
    ) -> Option<u64> {
        self.get(list)?;
        let id = self.new_id();
        let list = self.get_mut(list)?;
        let at = after
            .and_then(|a| list.items.iter().position(|i| i.id == a))
            .map_or(list.items.len(), |i| i + 1);
        let text = text.to_string();
        list.items.insert(at, Item { id, text, done });
        list.changed = now;
        Some(id)
    }

    fn item_mut(&mut self, list: u64, item: u64) -> Option<(&mut Item, &mut u64)> {
        let list = self.get_mut(list)?;
        let item = list.items.iter_mut().find(|i| i.id == item)?;
        Some((item, &mut list.changed))
    }

    pub fn set_text(&mut self, list: u64, item: u64, text: &str, now: u64) {
        if let Some((item, changed)) = self.item_mut(list, item)
            && item.text != text
        {
            item.text = text.to_string();
            *changed = now;
        }
    }

    /// Checks or unchecks an item; false when there is no such item.
    pub fn set_done(&mut self, list: u64, item: u64, done: bool, now: u64) -> bool {
        let Some((item, changed)) = self.item_mut(list, item) else {
            return false;
        };
        if item.done != done {
            item.done = done;
            *changed = now;
        }
        true
    }

    pub fn remove_item(&mut self, list: u64, item: u64, now: u64) {
        if let Some(list) = self.get_mut(list)
            && let Some(at) = list.items.iter().position(|i| i.id == item)
        {
            list.items.remove(at);
            list.changed = now;
        }
    }

    /// Puts an item in the place of item `before`, at the end when that is None or not there.
    pub fn move_item(&mut self, list: u64, item: u64, before: Option<u64>, now: u64) {
        if before == Some(item) {
            return;
        }
        let Some(list) = self.get_mut(list) else {
            return;
        };
        let Some(from) = list.items.iter().position(|i| i.id == item) else {
            return;
        };
        let moved = list.items.remove(from);
        let to = before
            .and_then(|b| list.items.iter().position(|i| i.id == b))
            .unwrap_or(list.items.len());
        list.items.insert(to, moved);
        list.changed = now;
    }

    /// The checklists whose name or an item's text holds `query` (any case; every checklist when
    /// it is blank), the most recently changed first.
    pub fn find(&self, query: &str) -> Vec<&List> {
        let query = query.trim().to_lowercase();
        let holds = |text: &str| text.to_lowercase().contains(&query);
        let mut found: Vec<&List> = self
            .lists
            .iter()
            .filter(|l| {
                query.is_empty() || holds(&l.name) || l.items.iter().any(|i| holds(&i.text))
            })
            .collect();
        found.sort_by_key(|l| std::cmp::Reverse((l.changed, l.id)));
        found
    }

    /// Puts back a checklist as it was (an undo step); false when it is gone since.
    pub fn restore(&mut self, before: List) -> bool {
        match self.get_mut(before.id) {
            Some(list) => {
                *list = before;
                true
            }
            None => false,
        }
    }
}

impl List {
    /// Its checked items and all of them.
    pub fn progress(&self) -> (usize, usize) {
        let done = self.items.iter().filter(|i| i.done).count();
        (done, self.items.len())
    }

    /// Its name, trimmed; NAME when it is blank.
    pub fn name_shown(&self) -> &str {
        match self.name.trim() {
            "" => NAME,
            name => name,
        }
    }

    /// A tab's title: its name, then its progress once it has items ("Deploy 2/5").
    pub fn title(&self) -> String {
        let name = crate::notes::title(self.name_shown());
        match self.progress() {
            (_, 0) => name,
            (done, all) => format!("{name} {done}/{all}"),
        }
    }

    /// Named NAME (or nothing) and no item with text: closing its last tab drops it, nothing lost.
    pub fn is_blank(&self) -> bool {
        self.name_shown() == NAME && self.items.iter().all(|i| i.text.trim().is_empty())
    }

    pub fn item(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
}

/// `add_items`' text as items: each line with text, a leading `- ` or `* ` dropped, then `[ ]` or
/// `[x]` (which checks it).
pub(crate) fn parse_items(text: &str) -> Vec<(String, bool)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let line = line
                .strip_prefix(['-', '*'])
                .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
                .unwrap_or(line)
                .trim_start();
            let (line, done) = if let Some(rest) = line.strip_prefix("[ ]") {
                (rest, false)
            } else if let Some(rest) = line.strip_prefix("[x]").or(line.strip_prefix("[X]")) {
                (rest, true)
            } else {
                (line, false)
            };
            let line = line.trim();
            (!line.is_empty()).then(|| (line.to_string(), done))
        })
        .collect()
}

pub(crate) fn path(dir: &Path) -> PathBuf {
    dir.join("checklists.json")
}

pub(crate) fn load(dir: &Path) -> Loaded<Checklists> {
    match session::read::<Checklists>(&path(dir)) {
        Loaded::Saved(mut c) => {
            // An edited file's next_id may lag behind its ids: never give one out twice.
            let ids = c
                .lists
                .iter()
                .flat_map(|l| std::iter::once(l.id).chain(l.items.iter().map(|i| i.id)));
            c.next_id = ids.fold(c.next_id, u64::max);
            Loaded::Saved(c)
        }
        other => other,
    }
}

/// Writes the checklists; like canvases, a write older than the last one done is dropped.
pub(crate) fn save(dir: &Path, checklists: &Checklists, generation: u64) -> io::Result<()> {
    let text = serde_json::to_string_pretty(checklists).map_err(io::Error::other)?;
    session::write_newest(&path(dir), &text, generation)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            (copied.items[0].text.as_str(), copied.items[0].done),
            ("y", true)
        );
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
        let texts = |c: &Checklists| {
            c.get(l)
                .unwrap()
                .items
                .iter()
                .map(|i| i.text.clone())
                .collect::<Vec<_>>()
        };
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
            [
                ("Build".into(), false),
                ("Test".into(), true),
                ("Ship".into(), true),
                ("1. Plain".into(), false)
            ]
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
        assert_eq!(
            text.matches("\"done\"").count(),
            1,
            "an unchecked item writes no done"
        );
        assert_eq!(load(d.path()), Loaded::Saved(c));
        std::fs::write(
            path(d.path()),
            r#"{"version":1,"checklists":[{"id":4,"items":[{"id":9,"text":"x"}]},{"id":2}]}"#,
        )
        .unwrap();
        let Loaded::Saved(read) = load(d.path()) else {
            panic!("not read")
        };
        assert_eq!(read.next_id, 9, "missing: the largest id, items' too");
        assert!(!read.lists[0].items[0].done);
        assert!(read.lists[1].items.is_empty() && read.lists[1].name.is_empty());
    }
}
