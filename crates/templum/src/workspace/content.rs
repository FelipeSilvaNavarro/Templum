//! The content tools (docs/specs/connections.md, Tools, batch 4): the cards of a kanban wired to the
//! caller, a wired note, a file opened in an editor, a wired Git block's status.

use gpui_kit::{App, Context, Window};
use templum_mcp::{Reply, tools};

use super::bus::{Outcome, flat};
use super::{BlockId, Space, Workspace};
use crate::board::{Card, Comment};
use crate::notes;

impl Workspace {
    /// The caller's workspace and block: `call` found it.
    fn caller_space(&self, from: &str) -> (&Space, BlockId) {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        (
            self.space_named(&space_id).expect("the caller's workspace"),
            me,
        )
    }

    /// The boards kanban blocks wired to `me` show, back to front, each once.
    fn wired_boards(&self, space: &Space, me: BlockId) -> Vec<u64> {
        let mut boards = Vec::new();
        for b in space.canvas.blocks() {
            if b.id != me
                && space.wired(me, b.id)
                && let Some(id) = space.kanban(b.id).and_then(|k| k.board)
                && self.boards.board(id).is_some()
                && !boards.contains(&id)
            {
                boards.push(id);
            }
        }
        boards
    }

    fn board_names(&self, boards: &[u64]) -> String {
        let names: Vec<String> = boards
            .iter()
            .filter_map(|b| self.boards.board(*b))
            .map(|b| format!("\"{}\" ({})", flat(&b.name), b.id))
            .collect();
        if names.is_empty() {
            "none".into()
        } else {
            names.join(", ")
        }
    }

    /// The board `name` names among those `me` may use; the only one when it is left out.
    fn board_for(&self, space: &Space, me: BlockId, name: Option<&str>) -> Result<u64, Reply> {
        let boards = self.wired_boards(space, me);
        match name {
            None => match boards.as_slice() {
                [one] => Ok(*one),
                [] => Err(Reply::err(
                    "No kanban connected to you shows a board. Ask the person with connect, or have \
                     them pick one.",
                )),
                _ => Err(Reply::err(format!(
                    "Several boards are connected to you: {}. Name one with board.",
                    self.board_names(&boards)
                ))),
            },
            Some(name) => {
                let lower = name.to_lowercase();
                boards
                    .iter()
                    .copied()
                    .find(|b| b.to_string() == name)
                    .or_else(|| {
                        boards.iter().copied().find(|b| {
                            self.boards
                                .board(*b)
                                .is_some_and(|x| x.name.trim().to_lowercase() == lower)
                        })
                    })
                    .ok_or_else(|| {
                        Reply::err(format!(
                            "No kanban connected to you shows board {name}. Connected boards: {}.",
                            self.board_names(&boards)
                        ))
                    })
            }
        }
    }

    /// The board holding `card`, when a kanban wired to `me` shows it. An unknown card and one
    /// elsewhere get the same answer.
    fn card_board(&self, space: &Space, me: BlockId, card: u64) -> Result<u64, Reply> {
        match self.boards.find_card(card) {
            Some((board, _)) if self.wired_boards(space, me).contains(&board) => Ok(board),
            _ => Err(Reply::err(format!(
                "No kanban connected to you has card {card}: list_cards lists them."
            ))),
        }
    }

    /// `b7k2m "Claude Code · api"`, `you`, or the id alone when no workspace has that block.
    fn who(&self, uid: &str, cx: &App) -> String {
        if uid == "you" {
            return "you".into();
        }
        match self.uid_title(uid, cx) {
            Some(title) => format!("{uid} {title:?}"),
            None => uid.to_string(),
        }
    }

    fn no_column(&self, board: u64, column: &str) -> Reply {
        let titles: Vec<String> = self
            .boards
            .board(board)
            .map(|b| b.columns.iter().map(|c| flat(&c.title)).collect())
            .unwrap_or_default();
        Reply::err(format!(
            "There's no column {column}. Its columns: {}.",
            titles.join(", ")
        ))
    }

    pub(super) fn list_cards(
        &self,
        from: &str,
        board: Option<&str>,
        card: Option<u64>,
        cx: &App,
    ) -> Reply {
        let (space, me) = self.caller_space(from);
        if let Some(card) = card {
            let board = match self.card_board(space, me, card) {
                Ok(b) => b,
                Err(refused) => return refused,
            };
            return Reply::ok(tools::tail(
                &self.card_whole(board, card, cx),
                tools::MAX_TEXT,
            ));
        }
        let board = match self.board_for(space, me, board) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        let Some(b) = self.boards.board(board) else {
            return Reply::err("That board was deleted.");
        };
        let mut text = format!("Board \"{}\" ({})", flat(&b.name), b.id);
        for column in &b.columns {
            text.push_str(&format!("\n{} ({}):", flat(&column.title), column.id));
            if column.cards.is_empty() {
                text.push_str(" no cards");
            }
            for c in &column.cards {
                text.push_str(&format!("\n  {}", self.card_line(c, cx)));
            }
        }
        Reply::ok(tools::tail(&text, tools::MAX_TEXT))
    }

    /// `5  Fix login  [red, green]  assignee b7k2m "Codex"  2 comments`
    fn card_line(&self, c: &Card, cx: &App) -> String {
        let mut line = format!("{}  {}", c.id, flat(&c.title));
        if !c.labels.is_empty() {
            let names: Vec<&str> = c.labels.iter().map(|l| l.name()).collect();
            line.push_str(&format!("  [{}]", names.join(", ")));
        }
        if let Some(uid) = &c.assignee {
            line.push_str(&format!("  assignee {}", self.who(uid, cx)));
        }
        match c.comments.len() {
            0 => {}
            1 => line.push_str("  1 comment"),
            n => line.push_str(&format!("  {n} comments")),
        }
        line
    }

    fn card_whole(&self, board: u64, card: u64, cx: &App) -> String {
        let (Some(b), Some(c)) = (self.boards.board(board), self.boards.card(board, card)) else {
            return String::new();
        };
        let column = b
            .columns
            .iter()
            .find(|col| col.cards.iter().any(|x| x.id == card));
        let indent = |text: &str| {
            text.lines()
                .map(|l| format!("  {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mut text = format!(
            "Card {} on board \"{}\" ({}), column \"{}\"\n{}",
            c.id,
            flat(&b.name),
            b.id,
            column.map(|col| flat(&col.title)).unwrap_or_default(),
            self.card_line(c, cx)
        );
        if !c.description.trim().is_empty() {
            text.push_str(&format!("\ndescription:\n{}", indent(&c.description)));
        }
        let now = notes::now();
        for comment in &c.comments {
            text.push_str(&format!(
                "\ncomment by {}, {}:\n{}",
                self.who(&comment.by, cx),
                notes::ago(comment.at, now),
                indent(&comment.text)
            ));
        }
        text
    }

    pub(super) fn create_card(
        &mut self,
        from: &str,
        board: Option<&str>,
        column: Option<&str>,
        title: &str,
        description: Option<String>,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space, me) = self.caller_space(from);
        let board = match self.board_for(space, me, board) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        if title.trim().is_empty() {
            return Reply::err("A card needs a title.");
        }
        let column = match column {
            Some(name) => match self.boards.column_named(board, name) {
                Some(c) => c,
                None => return self.no_column(board, name),
            },
            None => match self.boards.board(board).and_then(|b| b.columns.first()) {
                Some(c) => c.id,
                None => return Reply::err("That board has no columns."),
            },
        };
        let title = title.trim().to_string();
        let id = self.edit_boards(|b| {
            let id = b.add_card(board, column)?;
            b.edit_card(board, id, |c| {
                c.title = title;
                c.description = description.unwrap_or_default();
            });
            Some(id)
        });
        cx.notify();
        match id {
            Some(id) => Reply::ok(format!("Created card {id}.")),
            None => Reply::err("That column is gone."),
        }
    }

    pub(super) fn move_card(
        &mut self,
        from: &str,
        card: u64,
        column: &str,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space, me) = self.caller_space(from);
        let board = match self.card_board(space, me, card) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        let Some(to) = self.boards.column_named(board, column) else {
            return self.no_column(board, column);
        };
        self.edit_boards(|b| b.move_card(board, card, to, None));
        cx.notify();
        Reply::ok(format!("Moved card {card} to {column}."))
    }

    pub(super) fn comment_card(
        &mut self,
        from: &str,
        card: u64,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space, me) = self.caller_space(from);
        let board = match self.card_board(space, me, card) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        if text.trim().is_empty() {
            return Reply::err("A comment needs text.");
        }
        let by = space.uids.get(&me).cloned().unwrap_or_default();
        let comment = Comment {
            by,
            text: text.trim_end().to_string(),
            at: notes::now(),
        };
        self.edit_boards(|b| b.edit_card(board, card, |c| c.comments.push(comment)));
        cx.notify();
        Reply::ok(format!("Commented on card {card}."))
    }

    pub(super) fn claim_card(&mut self, from: &str, card: u64, cx: &mut Context<Self>) -> Reply {
        let (space, me) = self.caller_space(from);
        let board = match self.card_board(space, me, card) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        let mine = space.uids.get(&me).cloned().unwrap_or_default();
        match self
            .boards
            .card(board, card)
            .and_then(|c| c.assignee.clone())
        {
            Some(holder) if holder == mine => {
                return Reply::ok(format!("Card {card} is already yours."));
            }
            Some(holder) => {
                return Reply::err(format!(
                    "Card {card} is taken by {}. Ask the person to free it (the ✕ by its assignee in \
                     the open card), or take another card.",
                    self.who(&holder, cx)
                ));
            }
            None => {}
        }
        self.edit_boards(|b| b.edit_card(board, card, |c| c.assignee = Some(mine)));
        cx.notify();
        Reply::ok(format!("Card {card} is yours."))
    }

    pub(super) fn read_note(&self, _from: &str, _block: &str, _cx: &App) -> Reply {
        Reply::err("Not yet.")
    }

    pub(super) fn append_note(
        &mut self,
        _from: &str,
        _block: &str,
        _text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Reply {
        Reply::err("Not yet.")
    }

    pub(super) fn open_file_for(
        &mut self,
        _from: &str,
        _path: &str,
        _line: Option<u32>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Reply {
        Reply::err("Not yet.")
    }

    pub(super) fn git_status(
        &self,
        _from: &str,
        _block: Option<&str>,
        _cx: &mut Context<Self>,
    ) -> Outcome {
        Outcome::Now(Reply::err("Not yet."))
    }
}
