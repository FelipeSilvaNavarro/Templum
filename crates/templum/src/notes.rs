//! The notes: plain text, app-wide like the kanban boards, kept in `notes.json`. A note block
//! shows some of them as tabs; closing a tab or the block keeps the note, and only the Notes
//! dialog deletes one (docs/specs/cockpit.md, section 5).

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::board::Label;
use crate::session::{self, Loaded};

/// The file format's version: raised only when an older Templum couldn't read the new files.
pub(crate) const VERSION: u32 = 1;
/// A title's longest length, in characters.
const TITLE_MAX: usize = 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Notes {
    pub version: u32,
    /// The last id given out: never reused.
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub notes: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub id: u64,
    #[serde(default)]
    pub text: String,
    /// Its card's tint, a post-it's color; none: the blocks' own.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::board::known_label"
    )]
    pub color: Option<Label>,
    /// When its text last changed, in seconds since the Unix epoch.
    #[serde(default)]
    pub changed: u64,
}

impl Default for Notes {
    fn default() -> Self {
        Notes {
            version: VERSION,
            next_id: 0,
            notes: Vec::new(),
        }
    }
}

impl Notes {
    /// A new note; returns its id.
    pub fn add(&mut self, text: String, color: Option<Label>, now: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.notes.push(Entry {
            id,
            text,
            color,
            changed: now,
        });
        id
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.notes.iter().find(|n| n.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut Entry> {
        self.notes.iter_mut().find(|n| n.id == id)
    }

    /// Its text is `text` now; its time of change moves only when the text did.
    pub fn set_text(&mut self, id: u64, text: &str, now: u64) {
        if let Some(note) = self.get_mut(id)
            && note.text != text
        {
            note.text = text.to_string();
            note.changed = now;
        }
    }

    pub fn set_color(&mut self, id: u64, color: Option<Label>) {
        if let Some(note) = self.get_mut(id) {
            note.color = color;
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.notes.retain(|n| n.id != id);
    }

    /// A copy of note `id` as a new note (a pasted note block's); None when it is gone.
    pub fn duplicate(&mut self, id: u64, now: u64) -> Option<u64> {
        let note = self.get(id)?.clone();
        Some(self.add(note.text, note.color, now))
    }

    /// The notes whose text holds `query` (any case; every note when it is blank), the most
    /// recently changed first.
    pub fn find(&self, query: &str) -> Vec<&Entry> {
        let query = query.trim().to_lowercase();
        let mut found: Vec<&Entry> = self
            .notes
            .iter()
            .filter(|n| query.is_empty() || n.text.to_lowercase().contains(&query))
            .collect();
        found.sort_by_key(|n| std::cmp::Reverse((n.changed, n.id)));
        found
    }
}

/// A note's title: its first line with text, trimmed, cut at TITLE_MAX characters; "Untitled"
/// when it has none.
pub(crate) fn title(text: &str) -> String {
    let Some(line) = text.lines().map(str::trim).find(|l| !l.is_empty()) else {
        return "Untitled".to_string();
    };
    match line.char_indices().nth(TITLE_MAX) {
        Some((cut, _)) => format!("{}…", line[..cut].trim_end()),
        None => line.to_string(),
    }
}

/// Seconds since the Unix epoch.
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// How long ago `then` was, at `now` (both from `now()`): "Just now", "5 min ago", "3 h ago",
/// "2 days ago".
pub(crate) fn ago(then: u64, now: u64) -> String {
    let s = now.saturating_sub(then);
    match s {
        0..60 => "Just now".to_string(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86_400 => format!("{} h ago", s / 3600),
        86_400..172_800 => "Yesterday".to_string(),
        _ => format!("{} days ago", s / 86_400),
    }
}

pub(crate) fn path(dir: &Path) -> PathBuf {
    dir.join("notes.json")
}

pub(crate) fn load(dir: &Path) -> Loaded<Notes> {
    match session::read::<Notes>(&path(dir)) {
        Loaded::Saved(mut notes) => {
            // An edited file's next_id may lag behind its ids: never give one out twice.
            notes.next_id = notes
                .notes
                .iter()
                .map(|n| n.id)
                .fold(notes.next_id, u64::max);
            Loaded::Saved(notes)
        }
        other => other,
    }
}

/// Writes the notes; like canvases, a write older than the last one done is dropped.
pub(crate) fn save(dir: &Path, notes: &Notes, generation: u64) -> io::Result<()> {
    let text = serde_json::to_string_pretty(notes).map_err(io::Error::other)?;
    session::write_newest(&path(dir), &text, generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_never_reused_and_a_copy_is_a_new_note() {
        let mut notes = Notes::default();
        let a = notes.add("a".into(), Some(Label::Green), 1);
        notes.remove(a);
        let b = notes.add("b".into(), None, 2);
        assert_ne!(a, b);
        let c = notes.duplicate(b, 3).unwrap();
        assert_ne!(c, b);
        assert_eq!(notes.get(c).unwrap().text, "b");
        assert_eq!(notes.duplicate(a, 4), None, "a deleted note has no copy");
    }

    #[test]
    fn a_change_moves_its_time_and_the_same_text_does_not() {
        let mut notes = Notes::default();
        let id = notes.add("x".into(), None, 10);
        notes.set_text(id, "x", 20);
        assert_eq!(notes.get(id).unwrap().changed, 10);
        notes.set_text(id, "y", 30);
        assert_eq!(notes.get(id).unwrap().changed, 30);
        assert_eq!(notes.get(id).unwrap().text, "y");
    }

    #[test]
    fn find_takes_any_case_newest_first() {
        let mut notes = Notes::default();
        let old = notes.add("Buy MILK".into(), None, 1);
        let new = notes.add("milk tea".into(), None, 5);
        notes.add("bread".into(), None, 9);
        let ids = |found: Vec<&Entry>| found.iter().map(|n| n.id).collect::<Vec<_>>();
        assert_eq!(ids(notes.find(" Milk ")), [new, old]);
        assert_eq!(notes.find("").len(), 3);
    }

    #[test]
    fn the_title_is_the_first_line_with_text() {
        assert_eq!(title("\n  Plan for Monday  \nmore"), "Plan for Monday");
        assert_eq!(title("# Not a heading"), "# Not a heading", "plain text");
        assert_eq!(title(" \n\t\n"), "Untitled");
        let long = "é".repeat(TITLE_MAX + 5);
        assert_eq!(title(&long), format!("{}…", "é".repeat(TITLE_MAX)));
    }

    #[test]
    fn ago_says_how_long() {
        assert_eq!(ago(100, 130), "Just now");
        assert_eq!(ago(0, 300), "5 min ago");
        assert_eq!(ago(0, 3 * 3600), "3 h ago");
        assert_eq!(ago(0, 90_000), "Yesterday");
        assert_eq!(ago(0, 3 * 86_400), "3 days ago");
        assert_eq!(ago(500, 100), "Just now", "a clock set back");
    }

    #[test]
    fn notes_read_back_equal_and_next_id_follows_the_ids() {
        let d = tempfile::tempdir().unwrap();
        let mut notes = Notes::default();
        notes.add("one".into(), Some(Label::Cyan), 7);
        save(d.path(), &notes, 1).unwrap();
        assert_eq!(load(d.path()), Loaded::Saved(notes));
        std::fs::write(
            path(d.path()),
            r#"{"version":1,"notes":[{"id":9,"text":"x","color":"plaid"}]}"#,
        )
        .unwrap();
        let Loaded::Saved(read) = load(d.path()) else {
            panic!("not read");
        };
        assert_eq!(read.next_id, 9, "missing: the largest id");
        assert_eq!(read.notes[0].color, None, "an unknown color is none");
    }
}
