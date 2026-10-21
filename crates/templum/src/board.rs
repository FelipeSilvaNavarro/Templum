//! Kanban boards: columns of cards, every edit, and `boards.json` (see docs/specs/kanban.md).
//! Boards live outside the canvases: any kanban block, in any workspace, shows one by its id.

use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::checklists::Item;
use crate::session::{self, Loaded};

/// The file format's version: raised only when an older Templum couldn't read the new files.
pub(crate) const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Boards {
    pub version: u32,
    /// The last id given out: ids are unique across boards, columns, cards, checklist items and
    /// comments, never reused.
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub boards: Vec<Board>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Board {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub columns: Vec<Column>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Column {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub cards: Vec<Card>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Card {
    pub id: u64,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(
        default,
        deserialize_with = "known",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub labels: Vec<Label>,
    #[serde(
        default,
        deserialize_with = "known_priority",
        skip_serializing_if = "Option::is_none"
    )]
    pub priority: Option<Priority>,
    /// Who does it: none is anyone; `claim_card` makes it the claiming block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<Assignee>,
    #[serde(
        default,
        deserialize_with = "known",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub checklist: Vec<Item>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
}

/// Who a card is for: one block (one agent's session), or any session of an agent.
/// Saved as one string: a block's bare id (as before), or `agent:<id>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub(crate) enum Assignee {
    Block(String),
    Agent(String),
}

impl From<String> for Assignee {
    fn from(text: String) -> Self {
        match text.strip_prefix("agent:") {
            Some(agent) => Assignee::Agent(agent.to_string()),
            None => Assignee::Block(text),
        }
    }
}

impl From<Assignee> for String {
    fn from(assignee: Assignee) -> Self {
        match assignee {
            Assignee::Block(uid) => uid,
            Assignee::Agent(agent) => format!("agent:{agent}"),
        }
    }
}

/// A card's priority: a cue for the person and the agents; nothing sorts by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    pub const ALL: [Priority; 3] = [Priority::Low, Priority::Medium, Priority::High];

    pub fn name(self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
        }
    }

    /// The priority `name` names, trimmed, in either case; none for another word ("none").
    pub fn named(name: &str) -> Option<Priority> {
        let name = name.trim();
        Priority::ALL
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
    }

    pub fn title(self) -> &'static str {
        match self {
            Priority::Low => "Low",
            Priority::Medium => "Medium",
            Priority::High => "High",
        }
    }
}

/// A priority by its name; one Templum doesn't know is none (a typo mustn't lose the file).
fn known_priority<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Priority>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value
        .as_str()
        .and_then(|n| Priority::ALL.into_iter().find(|p| p.name() == n)))
}

/// A comment on a card: by an agent's block id, or `"you"` for the person; `at` in Unix seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Comment {
    /// How agents name it (`edit_comment`, `delete_comment`). 0 in a file from before: loading
    /// gives it a new one.
    #[serde(default)]
    pub id: u64,
    pub by: String,
    pub text: String,
    pub at: u64,
    /// Its author changed the text after writing it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,
}

impl Card {
    /// What sending it to a terminal types: its title, then its description.
    pub fn text(&self) -> String {
        if self.description.trim().is_empty() {
            self.title.clone()
        } else {
            format!("{}\n\n{}", self.title, self.description)
        }
    }

    /// Its checklist's (done, all).
    pub fn progress(&self) -> (usize, usize) {
        let done = self.checklist.iter().filter(|i| i.done).count();
        (done, self.checklist.len())
    }
}

/// A card's label: one of six colors, drawn with the theme's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Label {
    Red,
    Yellow,
    Green,
    Cyan,
    Blue,
    Magenta,
}

impl Label {
    pub const ALL: [Label; 6] = [
        Label::Red,
        Label::Yellow,
        Label::Green,
        Label::Cyan,
        Label::Blue,
        Label::Magenta,
    ];

    /// Its color in the theme's 16 ANSI colors.
    pub fn ansi(self) -> usize {
        match self {
            Label::Red => 1,
            Label::Green => 2,
            Label::Yellow => 3,
            Label::Blue => 4,
            Label::Magenta => 5,
            Label::Cyan => 6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Label::Red => "red",
            Label::Yellow => "yellow",
            Label::Green => "green",
            Label::Cyan => "cyan",
            Label::Blue => "blue",
            Label::Magenta => "magenta",
        }
    }
}

/// A label by its name; one Templum doesn't know is none (a typo mustn't lose the file).
pub(crate) fn known_label<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Label>, D::Error> {
    let name = Option::<String>::deserialize(d)?;
    Ok(name.and_then(|n| Label::ALL.into_iter().find(|l| l.name() == n)))
}

/// A list's entries that read (labels this Templum knows, checklist items); others, from an
/// edited file, are dropped, and what isn't a list is none: a typo mustn't lose the file.
fn known<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Array(entries) => entries
            .into_iter()
            .filter_map(|e| serde_json::from_value(e).ok())
            .collect(),
        _ => Vec::new(),
    })
}

impl Default for Boards {
    fn default() -> Self {
        Boards {
            version: VERSION,
            next_id: 0,
            boards: Vec::new(),
        }
    }
}

impl Board {
    pub fn card_count(&self) -> usize {
        self.columns.iter().map(|c| c.cards.len()).sum()
    }
}

impl Boards {
    fn next(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn board(&self, id: u64) -> Option<&Board> {
        self.boards.iter().find(|b| b.id == id)
    }

    fn board_mut(&mut self, id: u64) -> Option<&mut Board> {
        self.boards.iter_mut().find(|b| b.id == id)
    }

    /// "Board n" with To do, Doing and Done; returns its id.
    pub fn add_board(&mut self) -> u64 {
        self.add_board_with(None, &["To do", "Doing", "Done"].map(String::from))
    }

    /// A board named `name` ("Board n" when None) with `columns`, in their order; returns its
    /// id.
    pub fn add_board_with(&mut self, name: Option<&str>, columns: &[String]) -> u64 {
        let name = name.map(str::to_string).unwrap_or_else(|| {
            (self.boards.len() + 1..)
                .map(|n| format!("Board {n}"))
                .find(|name| !self.boards.iter().any(|b| b.name == *name))
                .expect("an unused name")
        });
        let id = self.next();
        let columns = columns
            .iter()
            .map(|title| Column {
                id: self.next(),
                title: title.clone(),
                cards: Vec::new(),
            })
            .collect();
        self.boards.push(Board { id, name, columns });
        id
    }

    pub fn rename_board(&mut self, id: u64, name: &str) {
        if let Some(board) = self.board_mut(id) {
            board.name = name.to_string();
        }
    }

    pub fn remove_board(&mut self, id: u64) {
        self.boards.retain(|b| b.id != id);
    }

    /// The boards whose name or a card's title holds `query`, in any case; all of them for a
    /// blank one. In their order.
    pub fn find(&self, query: &str) -> Vec<&Board> {
        let query = query.trim().to_lowercase();
        let holds = |text: &str| text.to_lowercase().contains(&query);
        self.boards
            .iter()
            .filter(|b| {
                query.is_empty()
                    || holds(&b.name)
                    || b.columns
                        .iter()
                        .flat_map(|c| &c.cards)
                        .any(|c| holds(&c.title))
            })
            .collect()
    }

    /// "New column" after the last one; returns its id.
    pub fn add_column(&mut self, board: u64) -> Option<u64> {
        let id = self.next();
        let board = self.board_mut(board)?;
        board.columns.push(Column {
            id,
            title: "New column".into(),
            cards: Vec::new(),
        });
        Some(id)
    }

    fn column_mut(&mut self, board: u64, column: u64) -> Option<&mut Column> {
        self.board_mut(board)?
            .columns
            .iter_mut()
            .find(|c| c.id == column)
    }

    pub fn rename_column(&mut self, board: u64, column: u64, title: &str) {
        if let Some(column) = self.column_mut(board, column) {
            column.title = title.to_string();
        }
    }

    /// Removes a column and its cards.
    pub fn remove_column(&mut self, board: u64, column: u64) {
        if let Some(board) = self.board_mut(board) {
            board.columns.retain(|c| c.id != column);
        }
    }

    /// Puts `column` in the place of column `to`.
    pub fn move_column(&mut self, board: u64, column: u64, to: u64) {
        let Some(board) = self.board_mut(board) else {
            return;
        };
        let (Some(from), Some(at)) = (
            board.columns.iter().position(|c| c.id == column),
            board.columns.iter().position(|c| c.id == to),
        ) else {
            return;
        };
        let moved = board.columns.remove(from);
        board.columns.insert(at, moved);
    }

    /// "New card" at the end of `column`; returns its id.
    pub fn add_card(&mut self, board: u64, column: u64) -> Option<u64> {
        let id = self.next();
        self.column_mut(board, column)?.cards.push(Card {
            id,
            title: "New card".into(),
            description: String::new(),
            labels: Vec::new(),
            priority: None,
            assignee: None,
            checklist: Vec::new(),
            comments: Vec::new(),
        });
        Some(id)
    }

    /// The board and column holding `card`: ids are unique across boards.
    pub fn find_card(&self, card: u64) -> Option<(u64, u64)> {
        self.boards.iter().find_map(|b| {
            b.columns
                .iter()
                .find(|c| c.cards.iter().any(|x| x.id == card))
                .map(|c| (b.id, c.id))
        })
    }

    /// The column of `board` that `name` names: its id, else its title in either case (the first).
    pub fn column_named(&self, board: u64, name: &str) -> Option<u64> {
        let columns = &self.board(board)?.columns;
        let name = name.trim();
        let title = name.to_lowercase();
        columns
            .iter()
            .find(|c| c.id.to_string() == name)
            .or_else(|| {
                columns
                    .iter()
                    .find(|c| c.title.trim().to_lowercase() == title)
            })
            .map(|c| c.id)
    }

    pub fn card(&self, board: u64, card: u64) -> Option<&Card> {
        self.board(board)?
            .columns
            .iter()
            .flat_map(|c| &c.cards)
            .find(|c| c.id == card)
    }

    pub fn edit_card(&mut self, board: u64, card: u64, edit: impl FnOnce(&mut Card)) {
        if let Some(card) = self.board_mut(board).and_then(|b| {
            b.columns
                .iter_mut()
                .flat_map(|c| &mut c.cards)
                .find(|c| c.id == card)
        }) {
            edit(card);
        }
    }

    /// Adds items at the end of the card's checklist; returns their new ids (none when it's gone).
    pub fn add_items(&mut self, board: u64, card: u64, items: &[(String, bool)]) -> Vec<u64> {
        if self.card(board, card).is_none() {
            return Vec::new();
        }
        let items: Vec<Item> = items
            .iter()
            .map(|(text, done)| Item {
                id: self.next(),
                text: text.clone(),
                done: *done,
            })
            .collect();
        let ids = items.iter().map(|i| i.id).collect();
        self.edit_card(board, card, |c| c.checklist.extend(items));
        ids
    }

    /// Checks or unchecks an item; false when the card has no such item.
    pub fn set_item_done(&mut self, board: u64, card: u64, item: u64, done: bool) -> bool {
        let mut found = false;
        self.edit_card(board, card, |c| {
            if let Some(i) = c.checklist.iter_mut().find(|i| i.id == item) {
                i.done = done;
                found = true;
            }
        });
        found
    }

    pub fn remove_item(&mut self, board: u64, card: u64, item: u64) {
        self.edit_card(board, card, |c| c.checklist.retain(|i| i.id != item));
    }

    /// Adds a comment by `by` at the end of the card's thread; returns its id (None when the card
    /// is gone).
    pub fn add_comment(
        &mut self,
        board: u64,
        card: u64,
        by: &str,
        text: &str,
        at: u64,
    ) -> Option<u64> {
        self.card(board, card)?;
        let comment = Comment {
            id: self.next(),
            by: by.to_string(),
            text: text.to_string(),
            at,
            edited: false,
        };
        let id = comment.id;
        self.edit_card(board, card, |c| c.comments.push(comment));
        Some(id)
    }

    /// Changes a comment's text, which then reads as edited; its author and time stay.
    pub fn edit_comment(&mut self, board: u64, card: u64, comment: u64, text: &str) {
        self.edit_card(board, card, |c| {
            if let Some(c) = c.comments.iter_mut().find(|c| c.id == comment)
                && c.text != text
            {
                c.text = text.to_string();
                c.edited = true;
            }
        });
    }

    pub fn remove_comment(&mut self, board: u64, card: u64, comment: u64) {
        self.edit_card(board, card, |c| c.comments.retain(|c| c.id != comment));
    }

    /// Puts a deleted `comment` back, as it was (its id, author, text, time and edited flag), at
    /// `index` of the card's thread, or at its end when the thread is shorter now. False, and
    /// nothing changes, when the card is gone or already has a comment with its id.
    pub fn restore_comment(
        &mut self,
        board: u64,
        card: u64,
        index: usize,
        comment: Comment,
    ) -> bool {
        let mut restored = false;
        self.edit_card(board, card, |c| {
            if c.comments.iter().all(|x| x.id != comment.id) {
                c.comments.insert(index.min(c.comments.len()), comment);
                restored = true;
            }
        });
        restored
    }

    pub fn toggle_label(&mut self, board: u64, card: u64, label: Label) {
        self.edit_card(board, card, |card| {
            if let Some(i) = card.labels.iter().position(|l| *l == label) {
                card.labels.remove(i);
            } else {
                card.labels.push(label);
            }
        });
    }

    pub fn remove_card(&mut self, board: u64, card: u64) {
        if let Some(board) = self.board_mut(board) {
            for column in &mut board.columns {
                column.cards.retain(|c| c.id != card);
            }
        }
    }

    /// Moves `card` into `column`: into the place of card `to`, or at the end without one.
    pub fn move_card(&mut self, board: u64, card: u64, column: u64, to: Option<u64>) {
        let Some(board) = self.board_mut(board) else {
            return;
        };
        let Some(target) = board.columns.iter().position(|c| c.id == column) else {
            return;
        };
        let at = to.and_then(|to| board.columns[target].cards.iter().position(|c| c.id == to));
        let Some(moved) = board.columns.iter_mut().find_map(|c| {
            let i = c.cards.iter().position(|x| x.id == card)?;
            Some(c.cards.remove(i))
        }) else {
            return;
        };
        let cards = &mut board.columns[target].cards;
        let at = at.unwrap_or(cards.len()).min(cards.len());
        cards.insert(at, moved);
    }
}

pub(crate) fn path(dir: &Path) -> PathBuf {
    dir.join("boards.json")
}

pub(crate) fn load(dir: &Path) -> Loaded<Boards> {
    match session::read::<Boards>(&path(dir)) {
        Loaded::Saved(mut boards) => {
            let ids = boards.boards.iter_mut().flat_map(|b| {
                std::iter::once(&mut b.id).chain(b.columns.iter_mut().flat_map(|c| {
                    std::iter::once(&mut c.id).chain(c.cards.iter_mut().flat_map(|x| {
                        std::iter::once(&mut x.id)
                            .chain(x.checklist.iter_mut().map(|i| &mut i.id))
                            .chain(x.comments.iter_mut().map(|c| &mut c.id))
                    }))
                }))
            });
            session::unique_ids(ids, &mut boards.next_id);
            Loaded::Saved(boards)
        }
        other => other,
    }
}

/// Writes the boards; like canvases, a write older than the last one done is dropped.
pub(crate) fn save(dir: &Path, boards: &Boards, generation: u64) -> io::Result<()> {
    let text = serde_json::to_string_pretty(boards).map_err(io::Error::other)?;
    session::write_newest(&path(dir), &text, generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(boards: &Boards, board: u64, column: usize) -> Vec<String> {
        boards.board(board).unwrap().columns[column]
            .cards
            .iter()
            .map(|c| c.title.clone())
            .collect()
    }

    /// A board with cards A, B, C in its first column: (boards, board, column ids, card ids).
    fn abc() -> (Boards, u64, [u64; 3], [u64; 3]) {
        let mut boards = Boards::default();
        let board = boards.add_board();
        let columns: Vec<u64> = boards
            .board(board)
            .unwrap()
            .columns
            .iter()
            .map(|c| c.id)
            .collect();
        let cards = ["A", "B", "C"].map(|title| {
            let id = boards.add_card(board, columns[0]).unwrap();
            boards.edit_card(board, id, |c| c.title = title.to_string());
            id
        });
        (boards, board, columns.try_into().unwrap(), cards)
    }

    #[test]
    fn a_new_board_has_three_columns_and_a_free_name() {
        let mut boards = Boards::default();
        let first = boards.add_board();
        let board = boards.board(first).unwrap();
        assert_eq!(board.name, "Board 1");
        let names: Vec<&str> = board.columns.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(names, ["To do", "Doing", "Done"]);
        boards.rename_board(first, "Board 2");
        let second = boards.add_board();
        assert_eq!(
            boards.board(second).unwrap().name,
            "Board 3",
            "Board 2 is taken"
        );
    }

    #[test]
    fn find_takes_the_name_and_the_cards_titles_in_any_case() {
        let (mut boards, abc, ..) = abc();
        let other = boards.add_board();
        boards.rename_board(other, "Release");
        let ids = |found: Vec<&Board>| found.iter().map(|b| b.id).collect::<Vec<_>>();
        assert_eq!(ids(boards.find("  ")), [abc, other], "all, in their order");
        assert_eq!(ids(boards.find("RELEASE")), [other], "by the name");
        assert_eq!(ids(boards.find("c")), [abc], "by a card's title");
        assert!(boards.find("nothing").is_empty());
    }

    #[test]
    fn ids_are_never_reused() {
        let (mut boards, board, columns, cards) = abc();
        let mut seen: Vec<u64> = [board].into_iter().chain(columns).chain(cards).collect();
        boards.remove_board(board);
        let again = boards.add_board();
        seen.push(again);
        seen.extend(boards.board(again).unwrap().columns.iter().map(|c| c.id));
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), seen.len(), "{seen:?}");
    }

    #[test]
    fn a_card_takes_the_place_of_the_card_it_is_dropped_on() {
        let (mut boards, board, columns, [a, b, c]) = abc();
        boards.move_card(board, a, columns[0], Some(c));
        assert_eq!(titles(&boards, board, 0), ["B", "C", "A"], "down: after it");
        boards.move_card(board, a, columns[0], Some(b));
        assert_eq!(titles(&boards, board, 0), ["A", "B", "C"], "up: before it");
        boards.move_card(board, b, columns[1], None);
        assert_eq!(titles(&boards, board, 0), ["A", "C"]);
        assert_eq!(
            titles(&boards, board, 1),
            ["B"],
            "to the end of another column"
        );
        boards.move_card(board, c, columns[1], Some(b));
        assert_eq!(
            titles(&boards, board, 1),
            ["C", "B"],
            "before a card of another column"
        );
        boards.move_card(board, a, columns[0], None);
        assert_eq!(
            titles(&boards, board, 0),
            ["A"],
            "onto its own column's end"
        );
    }

    #[test]
    fn a_column_takes_the_place_of_the_column_it_is_dropped_on() {
        let (mut boards, board, [todo, doing, done], _) = abc();
        boards.move_column(board, todo, done);
        let order: Vec<u64> = boards
            .board(board)
            .unwrap()
            .columns
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(order, [doing, done, todo]);
    }

    #[test]
    fn removing_a_column_removes_its_cards() {
        let (mut boards, board, columns, cards) = abc();
        boards.remove_column(board, columns[0]);
        assert_eq!(boards.board(board).unwrap().columns.len(), 2);
        assert!(boards.card(board, cards[0]).is_none());
        assert_eq!(boards.board(board).unwrap().card_count(), 0);
    }

    #[test]
    fn labels_toggle_and_cards_edit() {
        let (mut boards, board, _, cards) = abc();
        boards.toggle_label(board, cards[1], Label::Green);
        boards.toggle_label(board, cards[1], Label::Red);
        boards.toggle_label(board, cards[1], Label::Green);
        boards.edit_card(board, cards[1], |c| c.description = "why".into());
        let card = boards.card(board, cards[1]).unwrap();
        assert_eq!(card.labels, [Label::Red]);
        assert_eq!(card.description, "why");
        boards.remove_card(board, cards[1]);
        assert!(boards.card(board, cards[1]).is_none());
    }

    #[test]
    fn an_older_write_never_replaces_a_newer_one() {
        let d = tempfile::tempdir().unwrap();
        let (older, _, _, _) = abc();
        let mut newer = older.clone();
        newer.add_board();
        save(d.path(), &newer, 7).unwrap();
        save(d.path(), &older, 6).unwrap();
        assert_eq!(load(d.path()), Loaded::Saved(newer));
    }

    #[test]
    fn boards_read_back_equal_and_unknown_labels_go() {
        let (mut boards, board, _, cards) = abc();
        boards.toggle_label(board, cards[0], Label::Cyan);
        let d = tempfile::tempdir().unwrap();
        save(d.path(), &boards, 1).unwrap();
        assert_eq!(load(d.path()), Loaded::Saved(boards));
        let text = r#"{"version":1,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T",
            "cards":[{"id":3,"title":"x","labels":["green","plaid"]}]}]}]}"#;
        std::fs::write(path(d.path()), text).unwrap();
        let Loaded::Saved(read) = load(d.path()) else {
            panic!("not read");
        };
        assert_eq!(read.boards[0].columns[0].cards[0].labels, [Label::Green]);
        assert_eq!(read.next_id, 3, "missing: after the largest id");
    }

    #[test]
    fn a_card_is_found_by_its_id_alone() {
        let (mut boards, board, columns, cards) = abc();
        let other = boards.add_board();
        let todo = boards.board(other).unwrap().columns[0].id;
        let far = boards.add_card(other, todo).unwrap();
        assert_eq!(boards.find_card(cards[1]), Some((board, columns[0])));
        assert_eq!(boards.find_card(far), Some((other, todo)));
        assert_eq!(boards.find_card(9_999), None);
    }

    #[test]
    fn a_column_is_named_by_its_id_or_its_title_in_either_case() {
        let (boards, board, columns, _) = abc();
        assert_eq!(boards.column_named(board, "doing"), Some(columns[1]));
        assert_eq!(boards.column_named(board, " DONE "), Some(columns[2]));
        assert_eq!(
            boards.column_named(board, &columns[0].to_string()),
            Some(columns[0])
        );
        assert_eq!(boards.column_named(board, "Review"), None);
        assert_eq!(boards.column_named(9_999, "Doing"), None);
    }

    #[test]
    fn assignee_and_comments_read_back_and_an_older_card_has_none() {
        let (mut boards, board, _, cards) = abc();
        boards.edit_card(board, cards[0], |c| {
            c.assignee = Some(Assignee::Block("b7k2m".into()));
        });
        boards.add_comment(
            board,
            cards[0],
            "b7k2m",
            "On it.\nTwo lines.",
            1_760_000_000,
        );
        boards.add_comment(board, cards[0], "you", "Thanks", 1_760_000_100);
        let d = tempfile::tempdir().unwrap();
        save(d.path(), &boards, 1).unwrap();
        assert_eq!(load(d.path()), Loaded::Saved(boards));
        let text = std::fs::read_to_string(path(d.path())).unwrap();
        assert!(
            text.contains("\"assignee\": \"b7k2m\"") && text.contains("\"by\": \"you\""),
            "{text}"
        );
        // A card without them writes neither key (an older Templum reads the file as before).
        let fresh = r#"{"version":1,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T","cards":[{"id":3,"title":"x"}]}]}]}"#;
        std::fs::write(path(d.path()), fresh).unwrap();
        let Loaded::Saved(read) = load(d.path()) else {
            panic!("not read")
        };
        let card = &read.boards[0].columns[0].cards[0];
        assert_eq!((card.assignee.clone(), card.comments.len()), (None, 0));
        save(d.path(), &read, 2).unwrap();
        let again = std::fs::read_to_string(path(d.path())).unwrap();
        assert!(
            !again.contains("assignee") && !again.contains("comments"),
            "{again}"
        );
    }

    #[test]
    fn priority_checklist_and_an_agent_assignee_read_back_and_older_cards_have_none() {
        let (mut boards, board, _, cards) = abc();
        boards.add_items(
            board,
            cards[0],
            &[("Model".into(), false), ("Tools".into(), true)],
        );
        boards.edit_card(board, cards[0], |c| {
            c.priority = Some(Priority::High);
            c.assignee = Some(Assignee::Agent("codex".into()));
        });
        boards.edit_card(board, cards[1], |c| {
            c.assignee = Some(Assignee::Block("b7k2m".into()))
        });
        let d = tempfile::tempdir().unwrap();
        save(d.path(), &boards, 1).unwrap();
        assert_eq!(load(d.path()), Loaded::Saved(boards));
        let text = std::fs::read_to_string(path(d.path())).unwrap();
        for key in [
            "\"priority\": \"high\"",
            "\"assignee\": \"agent:codex\"",
            "\"assignee\": \"b7k2m\"",
            "\"text\": \"Model\"",
            "\"done\": true",
        ] {
            assert!(text.contains(key), "{key}: {text}");
        }
        let old = r#"{"version":1,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T",
            "cards":[{"id":3,"title":"x","assignee":"b7k2m","priority":"urgent"}]}]}]}"#;
        std::fs::write(path(d.path()), old).unwrap();
        let Loaded::Saved(read) = load(d.path()) else {
            panic!("not read")
        };
        let card = &read.boards[0].columns[0].cards[0];
        assert_eq!(
            card.assignee,
            Some(Assignee::Block("b7k2m".into())),
            "an older assignee is a block"
        );
        assert_eq!(
            (card.priority, card.checklist.len()),
            (None, 0),
            "an unknown priority is none"
        );
        // Nor a priority that isn't a word: the file still loads.
        let odd = r#"{"version":1,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T",
            "cards":[{"id":3,"title":"x","priority":2},{"id":4,"title":"y","priority":null}]}]}]}"#;
        std::fs::write(path(d.path()), odd).unwrap();
        let Loaded::Saved(odd) = load(d.path()) else {
            panic!("a priority of 2 set the file aside")
        };
        assert!(
            odd.boards[0].columns[0]
                .cards
                .iter()
                .all(|c| c.priority.is_none())
        );
        let mut fresh = read.clone();
        fresh.boards[0].columns[0].cards[0].assignee = None;
        save(d.path(), &fresh, 2).unwrap();
        let again = std::fs::read_to_string(path(d.path())).unwrap();
        assert!(
            !again.contains("priority")
                && !again.contains("checklist")
                && !again.contains("assignee"),
            "{again}"
        );
    }

    #[test]
    fn comments_get_ids_are_edited_in_place_and_go() {
        let (mut boards, board, _, [card, ..]) = abc();
        let first = boards
            .add_comment(board, card, "b7k2m", "On it.", 10)
            .unwrap();
        let second = boards
            .add_comment(board, card, "you", "Thanks", 20)
            .unwrap();
        assert!(first != second && second == boards.next_id);
        assert_eq!(boards.add_comment(board, 9_999, "you", "lost", 0), None);
        let thread = |boards: &Boards| {
            boards
                .card(board, card)
                .unwrap()
                .comments
                .iter()
                .map(|c| (c.id, c.text.clone(), c.edited))
                .collect::<Vec<_>>()
        };
        // Its text changes; its place, author and time stay, and it reads as edited.
        boards.edit_comment(board, card, first, "Done.");
        let c = &boards.card(board, card).unwrap().comments[0];
        assert_eq!(
            (c.by.as_str(), c.at, c.text.as_str(), c.edited),
            ("b7k2m", 10, "Done.", true)
        );
        // The same text is no edit; an unknown id changes nothing.
        boards.edit_comment(board, card, second, "Thanks");
        boards.edit_comment(board, card, 9_999, "x");
        assert_eq!(
            thread(&boards),
            [
                (first, "Done.".into(), true),
                (second, "Thanks".into(), false)
            ]
        );
        let gone = boards.card(board, card).unwrap().comments[0].clone();
        boards.remove_comment(board, card, first);
        assert_eq!(thread(&boards), [(second, "Thanks".into(), false)]);
        // A deleted comment comes back whole at its place, once; a shorter thread takes it at the end.
        assert!(boards.restore_comment(board, card, 0, gone.clone()));
        assert!(
            !boards.restore_comment(board, card, 0, gone.clone()),
            "it is there"
        );
        assert_eq!(boards.card(board, card).unwrap().comments[0], gone);
        assert_eq!(
            thread(&boards),
            [
                (first, "Done.".into(), true),
                (second, "Thanks".into(), false)
            ]
        );
        boards.remove_comment(board, card, first);
        assert!(boards.restore_comment(board, card, 7, gone));
        assert_eq!(
            thread(&boards),
            [
                (second, "Thanks".into(), false),
                (first, "Done.".into(), true)
            ]
        );
        boards.remove_comment(board, card, first);
        assert!(
            !boards.restore_comment(
                board,
                999,
                0,
                Comment {
                    id: first,
                    by: "you".into(),
                    text: "x".into(),
                    at: 1,
                    edited: false
                }
            ),
            "a card that is gone takes none"
        );
        // Its id is never given again.
        let third = boards.add_comment(board, card, "you", "More", 30).unwrap();
        assert!(third > second);
    }

    #[test]
    fn a_file_from_before_gives_its_comments_ids_and_keeps_them_apart_from_others() {
        let d = tempfile::tempdir().unwrap();
        // Comments with no id (written before they had one), beside a card, an item and a comment
        // that repeats the card's id.
        let text = r#"{"version":1,"next_id":4,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T",
            "cards":[{"id":3,"title":"x","checklist":[{"id":4,"text":"i"}],
            "comments":[{"by":"you","text":"a","at":1},{"by":"b7k2m","text":"b","at":2},
                        {"id":3,"by":"you","text":"c","at":3}]}]}]}]}"#;
        std::fs::write(path(d.path()), text).unwrap();
        let Loaded::Saved(read) = load(d.path()) else {
            panic!("not read")
        };
        let comments = &read.boards[0].columns[0].cards[0].comments;
        let ids: Vec<u64> = comments.iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            [5, 6, 7],
            "new ones, in the file's order; text and authors kept"
        );
        assert_eq!(
            comments.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert!(comments.iter().all(|c| !c.edited));
        assert_eq!(read.next_id, 7);
        // A comment that was never edited writes no `edited`.
        save(d.path(), &read, 1).unwrap();
        let again = std::fs::read_to_string(path(d.path())).unwrap();
        assert!(
            again.contains("\"id\": 5") && !again.contains("edited"),
            "{again}"
        );
    }

    #[test]
    fn checklist_items_get_new_ids_check_and_go() {
        let (mut boards, board, columns, cards) = abc();
        let ids = boards.add_items(board, cards[1], &[("a".into(), false), ("b".into(), true)]);
        assert!(
            ids.iter().all(|i| *i > cards[2] && *i > columns[2]),
            "{ids:?}"
        );
        assert_eq!(boards.card(board, cards[1]).unwrap().progress(), (1, 2));
        assert!(boards.set_item_done(board, cards[1], ids[0], true));
        assert!(!boards.set_item_done(board, cards[1], 9_999, true));
        assert!(
            !boards.set_item_done(board, cards[0], ids[0], true),
            "another card's item"
        );
        boards.remove_item(board, cards[1], ids[1]);
        assert_eq!(boards.card(board, cards[1]).unwrap().progress(), (1, 1));
        assert!(
            boards
                .add_items(board, 9_999, &[("x".into(), false)])
                .is_empty()
        );
        // A hand-edited file whose item ids pass next_id: none is given out twice.
        let d = tempfile::tempdir().unwrap();
        let text = r#"{"version":1,"next_id":3,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T",
            "cards":[{"id":3,"title":"x","checklist":[{"id":50,"text":"y"}]}]}]}]}"#;
        std::fs::write(path(d.path()), text).unwrap();
        let Loaded::Saved(read) = load(d.path()) else {
            panic!("not read")
        };
        assert_eq!(read.next_id, 50);
    }

    #[test]
    fn a_hand_edited_checklist_never_sets_the_file_aside() {
        // No list, labels that aren't words or a list, an item with no id, a `done` that isn't true
        // or false:
        // the file loads with the items that read, an item with no id gets a new one.
        let d = tempfile::tempdir().unwrap();
        let text = r#"{"version":1,"next_id":9,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T",
            "cards":[{"id":3,"title":"a","checklist":null,"labels":[2,"red"]},
            {"id":4,"title":"b","checklist":[{"text":"Build"},{"id":5,"text":"Test","done":"yes"},
                {"id":6,"text":"Ship","done":true}]},
            {"id":7,"title":"c","checklist":"Build","labels":null}]}]}]}"#;
        std::fs::write(path(d.path()), text).unwrap();
        let Loaded::Saved(read) = load(d.path()) else {
            panic!("a bad checklist set the file aside")
        };
        let cards = &read.boards[0].columns[0].cards;
        assert_eq!(cards.len(), 3);
        assert!(cards[0].checklist.is_empty() && cards[2].checklist.is_empty());
        assert_eq!(cards[0].labels, [Label::Red]);
        let items: Vec<_> = cards[1]
            .checklist
            .iter()
            .map(|i| (i.id, i.text.as_str(), i.done))
            .collect();
        assert_eq!(items, [(10, "Build", false), (6, "Ship", true)]);
        assert_eq!(read.next_id, 10);
    }

    #[test]
    fn ids_given_twice_by_hand_get_new_ones_after_the_first() {
        let d = tempfile::tempdir().unwrap();
        let text = r#"{"version":1,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T",
            "cards":[{"id":3,"title":"x","checklist":[{"id":5,"text":"a"},{"id":5,"text":"b"}]},
            {"id":3,"title":"y"}]}]}]}"#;
        std::fs::write(path(d.path()), text).unwrap();
        let Loaded::Saved(mut read) = load(d.path()) else {
            panic!("not read")
        };
        let column = &read.boards[0].columns[0];
        let items: Vec<u64> = column.cards[0].checklist.iter().map(|i| i.id).collect();
        assert_eq!(items, [5, 6]);
        assert_eq!(column.cards[1].id, 7);
        assert_eq!(read.next_id, 7);
        // Ticking or deleting the second item leaves the first alone.
        assert!(read.set_item_done(1, 3, 6, true));
        assert_eq!(read.card(1, 3).unwrap().progress(), (1, 2));
        read.remove_item(1, 3, 6);
        assert_eq!(read.card(1, 3).unwrap().checklist[0].text, "a");
    }
}
