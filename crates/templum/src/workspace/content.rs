//! The content tools (docs/specs/connections.md, Tools, batch 4): the cards of a kanban wired to the
//! caller, a wired note, a wired checklist, a file opened in an editor.

use std::path::Path;

use gpui_kit::component::input::{EditorState, Position, RopeExt as _};
use gpui_kit::{App, Context, Entity, Window};
use templum_mcp::{Reply, tools};

use super::bus::{flat, target};
use super::{BlockId, Body, Space, Workspace};
use crate::board::{Assignee, Card, Comment, Priority};
use crate::checklists;
use crate::editor::{Content, Editor};
use crate::notes;

/// The caller as cards name it: its block id, and the agent it runs.
type Caller = (String, Option<String>);

fn caller(space: &Space, me: BlockId) -> Caller {
    let uid = space.uids.get(&me).cloned().unwrap_or_default();
    let agent = space
        .terminal(me)
        .and_then(|p| p.agent())
        .map(str::to_string);
    (uid, agent)
}

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

    /// Whether a block running `pane_agent` runs `agent` to a card: the same agent, or one with
    /// its program (the first word of their commands).
    fn runs(&self, pane_agent: Option<&str>, agent: &str) -> bool {
        let program = |id: &str| {
            self.agents
                .iter()
                .find(|a| a.id == id)
                .and_then(|a| a.command.split_whitespace().next())
        };
        pane_agent.is_some_and(|pane| {
            pane == agent || program(pane).is_some_and(|p| program(agent) == Some(p))
        })
    }

    /// Whether `assignee` is the caller's: its block, or an agent it runs.
    fn is_callers(&self, assignee: &Assignee, caller: &Caller) -> bool {
        match assignee {
            Assignee::Block(uid) => *uid == caller.0,
            Assignee::Agent(id) => self.runs(caller.1.as_deref(), id),
        }
    }

    /// Ok when the caller may take `card`: it is for anyone, the caller's block, or an agent it
    /// runs; otherwise `claim_card`'s refusal, saying why.
    fn may_take(&self, card: &Card, caller: &Caller, cx: &App) -> Result<(), Reply> {
        let id = card.id;
        match &card.assignee {
            Some(Assignee::Block(uid)) if *uid != caller.0 => Err(Reply::err(format!(
                "Card {id} is taken by {}. Ask the person to free it (the ✕ by its assignee in the \
                 open card), or take another card.",
                self.who(uid, cx)
            ))),
            Some(a @ Assignee::Agent(agent)) if !self.is_callers(a, caller) => {
                let wanted = self.agent_title(agent);
                Err(Reply::err(match &caller.1 {
                    None => format!(
                        "Card {id} is for any {wanted}, and Templum can't tell yet which agent you \
                         are: try again in a few seconds."
                    ),
                    Some(mine) => format!(
                        "Card {id} is for any {wanted}, and you are {}: take a card for you or for \
                         anyone.",
                        self.agent_title(mine)
                    ),
                }))
            }
            _ => Ok(()),
        }
    }

    /// Who a card is for, as `move_card` says it: `b7k2m "Codex · api"`, or `any codex`.
    fn for_whom(&self, assignee: &Assignee, cx: &App) -> String {
        match assignee {
            Assignee::Block(uid) => self.who(uid, cx),
            Assignee::Agent(id) => format!("any {id}"),
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
        let caller = caller(space, me);
        if let Some(card) = card {
            let board = match self.card_board(space, me, card) {
                Ok(b) => b,
                Err(refused) => return refused,
            };
            return Reply::ok(tools::tail(
                &self.card_whole(board, card, &caller, cx),
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
                text.push_str(&format!("\n  {}", self.card_line(c, &caller, cx)));
            }
        }
        Reply::ok(tools::tail(&text, tools::MAX_TEXT))
    }

    /// `5  Fix login  [red, green]  priority high  assignee b7k2m "Codex"  checklist 2/5  2 comments`
    fn card_line(&self, c: &Card, caller: &Caller, cx: &App) -> String {
        let mut line = format!("{}  {}", c.id, flat(&c.title));
        if !c.labels.is_empty() {
            let names: Vec<&str> = c.labels.iter().map(|l| l.name()).collect();
            line.push_str(&format!("  [{}]", names.join(", ")));
        }
        if let Some(p) = c.priority {
            line.push_str(&format!("  priority {}", p.name()));
        }
        match &c.assignee {
            Some(Assignee::Block(uid)) => {
                line.push_str(&format!("  assignee {}", self.who(uid, cx)))
            }
            Some(Assignee::Agent(id)) => line.push_str(&format!("  for any {id}")),
            None => {}
        }
        if c.assignee
            .as_ref()
            .is_some_and(|a| self.is_callers(a, caller))
        {
            line.push_str(" (you)");
        }
        if !c.checklist.is_empty() {
            let (done, all) = c.progress();
            line.push_str(&format!("  checklist {done}/{all}"));
        }
        match c.comments.len() {
            0 => {}
            1 => line.push_str("  1 comment"),
            n => line.push_str(&format!("  {n} comments")),
        }
        line
    }

    fn card_whole(&self, board: u64, card: u64, caller: &Caller, cx: &App) -> String {
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
            self.card_line(c, caller, cx)
        );
        if !c.description.trim().is_empty() {
            text.push_str(&format!("\ndescription:\n{}", indent(&c.description)));
        }
        if !c.checklist.is_empty() {
            text.push_str("\nchecklist:");
            for item in &c.checklist {
                let mark = if item.done { 'x' } else { ' ' };
                text.push_str(&format!("\n  {} [{mark}] {}", item.id, flat(&item.text)));
            }
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

    /// Who `text` names for a card: anyone (None); an agent by its id or title in either case,
    /// matched first; or a terminal block by its id, in any workspace.
    fn resolve_for(&self, text: &str) -> Result<Option<Assignee>, Reply> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("anyone") {
            return Ok(None);
        }
        if let Some(agent) = self
            .agents
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(text) || a.title.eq_ignore_ascii_case(text))
        {
            return Ok(Some(Assignee::Agent(agent.id.clone())));
        }
        let terminal = self.spaces().any(|s| {
            s.block_by_uid(text)
                .is_some_and(|id| s.terminal(id).is_some())
        });
        if terminal {
            return Ok(Some(Assignee::Block(text.to_string())));
        }
        let ids: Vec<&str> = self.agents.iter().map(|a| a.id.as_str()).collect();
        Err(Reply::err(format!(
            "There's no agent or terminal block {text}. Agents: {}; or a terminal's block id, or \
             anyone.",
            ids.join(", ")
        )))
    }

    pub(super) fn create_card(
        &mut self,
        from: &str,
        new: tools::NewCard,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space, me) = self.caller_space(from);
        let board = match self.board_for(space, me, new.board.as_deref()) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        if new.title.trim().is_empty() {
            return Reply::err("A card needs a title.");
        }
        let assignee = match new.who.as_deref().map(|w| self.resolve_for(w)) {
            Some(Ok(who)) => who,
            Some(Err(refused)) => return refused,
            None => None,
        };
        let priority = new.priority.as_deref().and_then(Priority::named);
        let items = new
            .checklist
            .as_deref()
            .map(checklists::parse_items)
            .unwrap_or_default();
        let column = match new.column.as_deref() {
            Some(name) => match self.boards.column_named(board, name) {
                Some(c) => c,
                None => return self.no_column(board, name),
            },
            None => match self.boards.board(board).and_then(|b| b.columns.first()) {
                Some(c) => c.id,
                None => return Reply::err("That board has no columns."),
            },
        };
        let title = new.title.trim().to_string();
        let id = self.edit_boards(|b| {
            let id = b.add_card(board, column)?;
            b.edit_card(board, id, |c| {
                c.title = title;
                c.description = new.description.unwrap_or_default();
                c.priority = priority;
                c.assignee = assignee;
            });
            b.add_items(board, id, &items);
            Some(id)
        });
        cx.notify();
        match id {
            Some(id) => Reply::ok(format!("Created card {id}.")),
            None => Reply::err("That column is gone."),
        }
    }

    /// Sets a card's priority and who it is for, adds items, checks and unchecks them: every
    /// part is checked first, then all apply as one Ctrl+Z step.
    pub(super) fn edit_card(
        &mut self,
        from: &str,
        edit: tools::CardEdit,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space, me) = self.caller_space(from);
        let card = edit.card;
        let board = match self.card_board(space, me, card) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        let assignee = match edit.who.as_deref().map(|w| self.resolve_for(w)) {
            Some(Ok(who)) => Some(who),
            Some(Err(refused)) => return refused,
            None => None,
        };
        // Its `for` follows claim_card: never a card another block holds freed or handed on,
        // nor one for another agent given to the caller.
        if let Some(who) = &assignee
            && let Some(c) = self.boards.card(board, card)
        {
            let caller = caller(space, me);
            let held = matches!(&c.assignee, Some(Assignee::Block(uid)) if *uid != caller.0);
            let names_caller = who.as_ref().is_some_and(|w| self.is_callers(w, &caller));
            if (held || names_caller)
                && let Err(refused) = self.may_take(c, &caller, cx)
            {
                return refused;
            }
        }
        let has = |item: &u64| {
            self.boards
                .card(board, card)
                .is_some_and(|c| c.checklist.iter().any(|i| i.id == *item))
        };
        if let Some(item) = edit.check.iter().chain(&edit.uncheck).find(|i| !has(i)) {
            return Reply::err(format!(
                "Card {card} has no item {item}: list_cards with card shows its items."
            ));
        }
        let priority = edit.priority.as_deref().map(Priority::named);
        let items = edit
            .add_items
            .as_deref()
            .map(checklists::parse_items)
            .unwrap_or_default();
        let added = self.edit_boards(|b| {
            b.edit_card(board, card, |c| {
                if let Some(p) = priority {
                    c.priority = p;
                }
                if let Some(who) = assignee.clone() {
                    c.assignee = who;
                }
            });
            for (ids, done) in [(&edit.check, true), (&edit.uncheck, false)] {
                for item in ids {
                    b.set_item_done(board, card, *item, done);
                }
            }
            b.add_items(board, card, &items)
        });
        cx.notify();
        let list = |ids: &[u64]| {
            ids.iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut said = Vec::new();
        if let Some(p) = priority {
            said.push(format!("priority {}", p.map_or("none", |p| p.name())));
        }
        match &assignee {
            Some(None) => said.push("for anyone".into()),
            Some(Some(Assignee::Agent(id))) => said.push(format!("for any {id}")),
            Some(Some(Assignee::Block(uid))) => said.push(format!("for {}", self.who(uid, cx))),
            None => {}
        }
        match added.as_slice() {
            [] => {}
            [one] => said.push(format!("added item {one}")),
            more => said.push(format!("added items {}", list(more))),
        }
        if !edit.check.is_empty() {
            said.push(format!("checked {}", list(&edit.check)));
        }
        if !edit.uncheck.is_empty() {
            said.push(format!("unchecked {}", list(&edit.uncheck)));
        }
        Reply::ok(format!("Card {card}: {}.", said.join("; ")))
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
        let caller = caller(space, me);
        let mine = caller.0.clone();
        // Its last column, or Review: the card is done, and says so only through a comment.
        let finished = self.boards.board(board).is_some_and(|b| {
            b.columns.last().is_some_and(|c| c.id == to)
                || b.columns
                    .iter()
                    .any(|c| c.id == to && c.title.trim().eq_ignore_ascii_case("review"))
        });
        let said = self
            .boards
            .card(board, card)
            .and_then(|c| c.comments.last())
            .is_some_and(|c| c.by == mine);
        // Moving stays open to every wired agent; it says when the card is someone else's.
        let theirs = self
            .boards
            .card(board, card)
            .and_then(|c| c.assignee.as_ref())
            .filter(|a| !self.is_callers(a, &caller))
            .map(|a| self.for_whom(a, cx));
        self.edit_boards(|b| b.move_card(board, card, to, None));
        cx.notify();
        let mut text = format!("Moved card {card} to {column}.");
        if finished && !said {
            text.push_str(
                " Say on it what you did and what the next one needs to know: comment_card.",
            );
        }
        if let Some(who) = theirs {
            text.push_str(&format!(" It is for {who}, not you."));
        }
        Reply::ok(text)
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
        let caller = caller(space, me);
        let Some(c) = self.boards.card(board, card) else {
            return Reply::err(format!("Card {card} is gone."));
        };
        let mine = Assignee::Block(caller.0.clone());
        if c.assignee.as_ref() == Some(&mine) {
            return Reply::ok(format!("Card {card} is already yours."));
        }
        if let Err(refused) = self.may_take(c, &caller, cx) {
            return refused;
        }
        self.edit_boards(|b| b.edit_card(board, card, |c| c.assignee = Some(mine)));
        cx.notify();
        Reply::ok(format!("Card {card} is yours."))
    }

    /// The note block `block` wired to the caller, and the editor of the note it shows.
    fn wired_note(&self, from: &str, block: &str) -> Result<Entity<EditorState>, Reply> {
        let (space, me) = self.caller_space(from);
        let id = target(space, me, block)?;
        let Some(note) = space.blocks.get(&id).and_then(|b| b.note()) else {
            return Err(Reply::err(format!("{block} isn't a note.")));
        };
        note.tab()
            .map(|t| t.editor.clone())
            .ok_or_else(|| Reply::err(format!("{block} shows no note.")))
    }

    pub(super) fn read_note(&self, from: &str, block: &str, cx: &App) -> Reply {
        match self.wired_note(from, block) {
            Ok(editor) => Reply::ok(tools::tail(&editor.read(cx).value(), tools::MAX_TEXT)),
            Err(refused) => refused,
        }
    }

    /// Adds `text` at the note's end after a blank line, as one step of the note's own Ctrl+Z; the
    /// person's caret and selection stay where they were.
    pub(super) fn append_note(
        &mut self,
        from: &str,
        block: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let editor = match self.wired_note(from, block) {
            Ok(editor) => editor,
            Err(refused) => return refused,
        };
        if text.trim().is_empty() {
            return Reply::err("There's nothing to add.");
        }
        editor.update(cx, |e, cx| {
            let value = e.value();
            let sep = if value.is_empty() || value.ends_with("\n\n") {
                ""
            } else if value.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            let (kept, end) = (e.selected_range(), value.len());
            e.set_selected_range(end..end, cx);
            e.insert(format!("{sep}{}", text.trim_end()), window, cx);
            e.set_selected_range(kept, cx);
        });
        Reply::ok(format!("Added to the note in {block}."))
    }

    /// The checklist block `block` wired to the caller, and the checklist it shows.
    fn wired_checklist(&self, from: &str, block: &str) -> Result<u64, Reply> {
        let (space, me) = self.caller_space(from);
        let id = target(space, me, block)?;
        let Some(checklist) = space.blocks.get(&id).and_then(|b| b.checklist()) else {
            return Err(Reply::err(format!("{block} isn't a checklist.")));
        };
        checklist
            .list
            .filter(|list| self.checklists.get(*list).is_some())
            .ok_or_else(|| Reply::err(format!("{block} shows no checklist.")))
    }

    /// The checklist's name and progress, then a line per item: its id, [x] or [ ], its text.
    pub(super) fn read_checklist(&self, from: &str, block: &str) -> Reply {
        let list = match self.wired_checklist(from, block) {
            Ok(list) => list,
            Err(refused) => return refused,
        };
        let Some(list) = self.checklists.get(list) else {
            return Reply::err(format!("{block} shows no checklist."));
        };
        let (done, all) = list.progress();
        let mut text = format!("{} ({done}/{all})", list.name_shown());
        for item in &list.items {
            let mark = if item.done { 'x' } else { ' ' };
            text.push_str(&format!("\n{} [{mark}] {}", item.id, item.text));
        }
        Reply::ok(tools::tail(&text, tools::MAX_TEXT))
    }

    /// Checks (or unchecks) an item of the checklist `block` shows: one Ctrl+Z step.
    pub(super) fn check_item(
        &mut self,
        from: &str,
        block: &str,
        item: u64,
        done: bool,
        cx: &mut Context<Self>,
    ) -> Reply {
        let list = match self.wired_checklist(from, block) {
            Ok(list) => list,
            Err(refused) => return refused,
        };
        if !self.edit_checklist(list, |c| c.set_done(list, item, done, notes::now())) {
            let name = self.checklists.get(list).map_or("", |l| l.name_shown());
            return Reply::err(format!(
                "There's no item {item} in \"{name}\". read_checklist shows its items."
            ));
        }
        self.agent_changed(list);
        cx.notify();
        let state = if done { "checked" } else { "unchecked" };
        Reply::ok(format!("Item {item} is {state}."))
    }

    /// Adds an item per line of `text` at the end of the checklist `block` shows: one Ctrl+Z
    /// step.
    pub(super) fn add_items(
        &mut self,
        from: &str,
        block: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Reply {
        let list = match self.wired_checklist(from, block) {
            Ok(list) => list,
            Err(refused) => return refused,
        };
        let items = checklists::parse_items(text);
        if items.is_empty() {
            return Reply::err("There's nothing to add.");
        }
        let now = notes::now();
        let ids: Vec<String> = self.edit_checklist(list, |c| {
            items
                .iter()
                .filter_map(|(text, done)| c.add_item(list, None, text, *done, now))
                .map(|id| id.to_string())
                .collect()
        });
        self.agent_changed(list);
        cx.notify();
        match ids.as_slice() {
            [one] => Reply::ok(format!("Added item {one}.")),
            many => Reply::ok(format!("Added items {}.", many.join(", "))),
        }
    }

    /// `open_file` from `from`: the front-most wired editor whose folder holds the file, else a
    /// new editor beside the caller, wired to it. Neither the keyboard nor the camera moves; an
    /// editor that has the keyboard gives it to the tab opened.
    pub(super) fn open_file_for(
        &mut self,
        from: &str,
        path: &str,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space, me) = self.caller_space(from);
        let here = space.blocks[&me]
            .folder()
            .or_else(|| space.terminal(me)?.seen.folder.clone());
        let named = crate::launcher::expand(path);
        let given = match &here {
            Some(here) if named.is_relative() => here.join(named),
            _ => named,
        };
        if given.is_relative() {
            return Reply::err("Templum doesn't know your folder yet: give the file's whole path.");
        }
        let Ok(real) = std::fs::canonicalize(&given) else {
            return Reply::err(format!("There's no file {}.", given.display()));
        };
        if !real.is_file() {
            return Reply::err(format!("{} isn't a file.", given.display()));
        }
        match crate::editor::read_file(&real) {
            Err(why) => return Reply::err(why),
            Ok((Content::Binary { .. }, _)) => {
                return Reply::err(format!(
                    "{} is binary: an editor shows no text for it.",
                    given.display()
                ));
            }
            Ok(_) => {}
        }
        // The file as `folder` names it, when `folder` holds it. Canonical paths only to compare:
        // an editor's tabs are named under its folder as it was given (a link, Windows' `C:\`),
        // and a tab is found again, and saved with the block, by that name. Only the file's
        // folder is made canonical: a linked file (CLAUDE.md -> AGENTS.md) keeps its own name.
        let (dir, name) = (
            given.parent().and_then(|p| std::fs::canonicalize(p).ok()),
            given.file_name(),
        );
        let under = |folder: &Path| {
            let inside = dir
                .as_deref()?
                .strip_prefix(std::fs::canonicalize(folder).ok()?)
                .ok()?;
            Some(folder.join(inside).join(name?))
        };
        let wired = space
            .canvas
            .blocks()
            .iter()
            .rev()
            .map(|b| b.id)
            .filter(|id| *id != me && space.wired(me, *id))
            .find_map(|id| Some((id, under(&space.editor(id)?.folder)?)));
        let (folder, path) = here
            .and_then(|here| under(&here).map(|path| (here, path)))
            .unwrap_or_else(|| {
                (
                    given.parent().unwrap_or(&given).to_path_buf(),
                    given.clone(),
                )
            });
        let space_id = space.id.clone();
        let opened = self.in_space(&space_id, |ws| {
            let (id, path, new) = match wired {
                Some((id, path)) => (id, path, false),
                None => {
                    let anchor = ws.space.home(ws.space.canvas.get(me)?);
                    let rect = ws.space.room_beside(&[anchor], crate::editor::SIZE);
                    let kept = (
                        ws.space.focused,
                        ws.space.selected.clone(),
                        ws.space.selected_wire,
                    );
                    // In grid mode a new block takes half of the focused tile: the caller's.
                    ws.space.focused = Some(me);
                    ws.space.pristine = false;
                    let editor = Editor::new(folder.clone(), true, cx);
                    let id = ws.add_block(rect, Body::Editor(Box::new(editor)));
                    (ws.space.focused, ws.space.selected, ws.space.selected_wire) = kept;
                    ws.space.add_wire(me, id);
                    (id, path, true)
                }
            };
            let had_keys = ws.space.blocks.get(&id)?.has_focus(window, cx);
            let editor = ws.space.editor_mut(id)?;
            let i = editor.open_tab(&path, window, cx).ok()?;
            if let Some(line) = line {
                let at = Position {
                    line: line - 1,
                    character: 0,
                };
                editor.tabs[i].editor.update(cx, |e, cx| {
                    // Not set_cursor_position: that would take the keyboard.
                    let offset = e.text().position_to_offset(&at);
                    e.set_selected_range(offset..offset, cx);
                });
            }
            // A tab with no text to type in (a Markdown preview): the canvas takes the keys, as
            // `release_focus` does, rather than leave them in a hidden tab.
            if had_keys && !editor.focus(window, cx) {
                window.focus(&ws.focus_handle, cx);
            }
            Some((ws.space.uids.get(&id)?.clone(), path, new))
        });
        cx.notify();
        let at = line.map(|n| format!(" at line {n}")).unwrap_or_default();
        match opened.flatten() {
            Some((uid, path, false)) => {
                Reply::ok(format!("Opened {}{at} in {uid}.", path.display()))
            }
            Some((uid, path, true)) => Reply::ok(format!(
                "Opened {}{at} in a new editor {uid} beside you, connected to you.",
                path.display()
            )),
            None => Reply::err(format!("Couldn't open {}.", given.display())),
        }
    }
}
