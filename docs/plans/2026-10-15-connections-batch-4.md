# Connections, batch 4 (content): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** an agent uses the content of the blocks wired to it: it reads a kanban board (and one card whole), creates,
moves, comments on and claims cards; reads a note and adds to its end; opens a file at a line in an editor without
taking the keyboard or the camera; and reads a Git block's status. The person sees each card's assignee and comment
count, reads the comments in the open card, adds their own, and frees a card's assignee.

**Architecture:** `templum-mcp` gains the nine tools' schemas and typed calls (`tools.rs`) and the verbs `kanban`,
`note`, `open` and `git` (`cli.rs`). In the app, `board.rs` gives a card an `assignee` and `comments` and finds a card
or a column without its board; a new `workspace/content.rs` answers the nine calls on the main thread (`bus.rs`'s
`call` hands them over, as it hands `delegate` to `delegate.rs`): kanban edits go through `Workspace::edit_boards`
(so the person's Ctrl+Z in a kanban takes them back), a note's text changes through its editor's own `insert` (so
Ctrl+Z inside the note takes it back), a file opens through `Editor::open_tab` and `set_selected_range` (neither
focuses), and `git_status` runs `git status --short --branch` on the background executor (`Outcome::Later`).
`kanban.rs` draws the card's assignee and comment count, and the open card's comments, its comment field and the
assignee's ✕.

**Tech Stack:** Rust 2024, GPUI (gpui-fast 0.1.8) through gpui-kit 0.7.3: `InputBaseState::insert`,
`selected_range`, `set_selected_range` (`gpui-base-0.7.3/src/input/base/state.rs:1098`, `:3135`, `:3150`; `insert`
is one undo step, `EditIntent::Atomic`, and emits `InputEvent::Change`; `set_selected_range` doesn't focus);
`TextareaState` (as the card's description, `crates/templum/src/kanban.rs:546-550`); `serde`; the user's `git`
through `templum_git::Repo::run` (`crates/templum-git/src/repo.rs:313`).

**Spec:** [docs/specs/connections.md](../specs/connections.md): batch 4 of its Delivery: the Tools rows marked 4, the
command-line rows marked 4, the instructions' kanban and note sentence (The MCP server), Files' `boards.json` bullet,
§Architecture's row for `board.rs, kanban.rs, notes.rs, workspace/note_tabs.rs, workspace/files.rs`, and §Testing's
items marked batch 4. Also [kanban.md](../specs/kanban.md) (the board, the open card, undo).

## Global Constraints

- Only `crates/templum` depends on gpui-kit; `templum-mcp` and `templum-git` stay UI-free and tested headlessly. No new
  dependency. No `unsafe`. Every logic change ships a test that fails without it.
- Spec, verbatim, the tools: `list_cards` (`board`): "The wired kanban block's board: columns, and each card's id,
  title, labels, assignee and comment count"; `create_card` (`board`, `column`, `title`, `description`): "A new card at
  the column's end; its id"; `move_card` (`card`, `column`): "Moves a card to the end of a column"; `comment_card`
  (`card`, `text`): "Adds a comment signed with the caller's id"; `claim_card` (`card`): "Makes the caller the card's
  assignee; refused when another block holds it"; `read_note` (`block`): "The text of the note the note block shows";
  `append_note` (`block`, `text`): "Adds the text at the note's end, after a blank line; the note shows it at once and
  Ctrl+Z takes it back"; `open_file` (`path`, `line`): "Opens the file at the line in the wired editor block whose
  folder holds it; with none, in a new editor block beside the caller, wired to it. Neither the keyboard nor the
  camera moves"; `git_status` (`block`): "The wired Git block's branch, ahead and behind, and changed files as `git
  status --short` shows them".
- "A block is named by its id. A **board** is named by its name or id, and may be left out when one wired kanban block
  shows one; a **column** by its title (either case) or id; a **card** by its id, as `list_cards` shows it."
- `boards.json`: "a card gains `assignee` (a block id) and `comments`, each `{ "by": "<block id or 'you'>", "text":
  "…", "at": <unix seconds> }`. An older file loads with none; an older Templum ignores them. The card shows the
  assignee's block title and the comment count; its dialog lists the comments and adds the person's."
- The instructions gain: "When a connected kanban has your task, move its card to Doing when you start and to Review
  when you are done; write what the next agent needs to know in the connected note."
- Verbs: `templum kanban list|add|move|comment|claim …`, `templum note read|append <block> [text]`, `templum open
  <path>[:<line>]`, `templum git status [<block>]`.
- Not in this batch: `wait_for`, wire labels, Settings ▸ Connections, Gemini CLI, `templum wire`, `--json` everywhere
  (batch 5).
- Commits on `development`, Conventional Commits, `feat`/`fix` titles written for a person using Templum; every commit
  message ends with the line `Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ`.
- Before saying done: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`; this batch paints (the card's face), so `--bench` and `--bench-mixed` run too (p95 ≤ 16.7 ms).

## Decisions

The spec leaves these open; the plan takes them as written here (1 and 2 are the user's, 2026-10-15).

1. **`list_cards` takes an optional `card`:** without it, the board as the spec says (a line per card); with it, that
   card whole: title, labels, assignee, description and every comment. (The spec's list has no way to read a card's
   description or comments.) The CLI: `templum kanban list [<board>] [--card <card>]`.
2. **The person frees a card:** the open card shows "Assignee: *title*" with a ✕ (`#unassign`) that clears it, as a
   step of the kanban's Ctrl+Z. `claim_card` refuses a card another block holds, whether or not that block still
   exists, and says to ask the person to free it.
3. **Which boards a caller may use:** those shown by kanban blocks wired to it (`Kanban::board`, also while the block
   shows its board list). A card is found by its id alone (ids are unique across boards) and is usable only when its
   board is one of those; an unknown card and one on a board not wired get the same refusal, so nothing leaks.
4. **Names:** a board by id (`"1"` or `1`), else by name in either case (the first wired match); a column of that board
   by id, else by title in either case (the first, left to right); `card`, `board` and `column` accept a number or a
   string. `create_card`'s `column` may be left out: the board's first column. A blank title or text is refused.
5. **Kanban edits by agents** go through `Workspace::edit_boards`: each is a step the person's Ctrl+Z in a focused
   kanban takes back, and no older snapshot wipes it (undo restores whole snapshots). Comments are signed with the
   caller's block id; the person's with `"you"`. The comment's time is `notes::now()`; it is shown with
   `notes::ago`.
6. **Text agents get:** board, column and card titles are flattened to one line by the same cleaning as block titles,
   without the 60-character cut (`flat`, split out of `clean_title`); a description and comments come as written
   (multi-line, indented). An assignee or a comment's author is shown as `b7k2m "Claude Code · api"`, or the id
   alone when no workspace has that block; the person as `you`. Every reply is cut to `MAX_TEXT` from its end
   (`tools::tail`).
7. **`append_note`:** `read_note`/`append_note` need a note block wired to the caller showing a note (a block with
   no tab: "shows no note"). The text goes after the note's last character, with `\n\n` before it (`\n` when the note
   ends in one newline, nothing when it is empty or ends in a blank line), by `insert` at the end. The person's caret
   and selection are put back where they were, so typing in the note goes on where it was. `ws.notes` follows through
   the tab's existing `InputEvent::Change` subscription and the 5-second snapshot saves it.
8. **`open_file`:** `path` is relative to the caller's folder (as `spawn_agent`'s `folder`; `~` is home), must be a
   file Templum's editor can open (`editor::read_file`: refused before any block opens), and `line` is 1-based. The
   editor used is the front-most editor block wired to the caller whose folder holds the file (both paths
   canonicalized). Without one, a new editor opens beside the caller (`place_beside`, the grid's half-tile rule as
   `spawn_agent`), wired to it, on the caller's folder when it holds the file, else the file's folder; one Ctrl+Z
   closes it with its wire; it isn't added to the recent folders. The camera never moves and nothing focuses, except
   that an editor block that has the keyboard gives it to the tab just opened (a hidden tab would leave keys
   nowhere: `release_focus`'s note, `crates/templum/src/workspace/blocks.rs:229`).
9. **`git_status`:** `block` may be left out when exactly one Git block is wired to the caller. The answer is `git -c
   color.status=false status --short --branch` run in the block's repository on the background executor, as git
   prints it, its end trimmed: the first line `## main...origin/main [ahead 1, behind 2]` says branch, ahead and
   behind. A Git block still looking for its repository, or showing none, is refused.
10. **New file `workspace/content.rs`** for the nine calls (as batch 3 made `delegate.rs`); `bus.rs` only dispatches.

## Review Focus

1. **The person types in a note as an agent adds to it:** their caret stays where it was, the added text is at the end
   after a blank line, and one Ctrl+Z in the note takes back the agent's text only. Task 6's
   `append_note_shows_at_once_keeps_the_caret_and_one_ctrl_z_takes_it_back`.
2. **An agent names a card it saw elsewhere (a board not wired to it):** refused, the boards unchanged, the reply saying
   no more than for an unknown card. Task 4's `a_card_on_a_board_not_connected_is_refused`.
3. **Columns named loosely:** "doing", "DOING", the id as a string, an unknown "Review" (refused, the board's columns
   listed). Task 1's `a_column_is_named_by_its_id_or_its_title_in_either_case` and Task 4's
   `create_move_comment_and_claim_change_the_board`.
4. **A path that isn't a plain file in a wired editor's folder:** relative with `..`, missing, a folder, binary:
   refused without a new block; a wired editor whose folder holds it after canonicalizing is the one used. Task 7's
   `open_file_refuses_what_it_cant_open_and_opens_no_block` and `open_file_lands_at_the_line_in_the_wired_editor`.
5. **A wired editor that has the keyboard:** the new tab takes it; no key goes to a hidden tab. Task 7's
   `an_editor_with_the_keyboard_gives_it_to_the_tab_opened`.

## File Structure

| File | Change | Task |
|---|---|---|
| `crates/templum/src/board.rs` | `Card::assignee`, `Card::comments`, `Comment`; `Boards::find_card`, `column_named` | 1 |
| `crates/templum-mcp/src/tools.rs` | the nine tools: schemas, `Call` variants, parsing | 2 |
| `crates/templum-mcp/src/cli.rs` | verbs `kanban`, `note`, `open`, `git` | 3 |
| `crates/templum/src/workspace/content.rs` (new) | the nine calls | 4, 6, 7, 8 |
| `crates/templum/src/workspace/bus.rs` | dispatch; `flat`; `uid_title`; the instructions' sentence | 4 |
| `crates/templum/src/workspace/mod.rs` | `mod content;`; `Space::kanban` no longer test-only | 4 |
| `crates/templum/src/kanban.rs` | the card's assignee and comment count; the open card's comments, field and ✕ | 5 |
| `crates/templum/src/workspace/tests/content.rs` (new), `tests/mod.rs`, `tests/bus.rs`, `tests/spawn.rs`, `tests/kanban.rs` | headless tests; `board_with_a_card` and `answer_soon` shared | 4–8 |
| `docs/specs/connections.md`, `docs/ARCHITECTURE.md`, `README.md`, `docs/minors.md` | what was built | 9 |

---

### Task 0: Push `development`, save this plan

- [x] **Step 1:** `git push origin development` (the user agreed on 2026-10-15). Report the CI run's URL
  (`gh run list --branch development --limit 1`); its result is checked at the end (Task 9), not waited for.
- [x] **Step 2:** Save this plan, from "# Connections, batch 4" on, as `docs/plans/2026-10-15-connections-batch-4.md`;
  commit `docs(connections): plan batch 4, content` with the Claude-Session line.

### Task 1: A card's assignee and comments; finding a card or a column alone

**Files:** Modify `crates/templum/src/board.rs` (structs at 40-52, `add_card` at 233-242, tests at 330-484).

**Interfaces — Produces:**
- `pub(crate) struct Comment { pub by: String, pub text: String, pub at: u64 }` (Clone, Debug, PartialEq, Serialize,
  Deserialize).
- `Card { …, pub assignee: Option<String>, pub comments: Vec<Comment> }`.
- `Boards::find_card(&self, card: u64) -> Option<(u64 /*board*/, u64 /*column*/)>`.
- `Boards::column_named(&self, board: u64, name: &str) -> Option<u64>`.

- [ ] **Step 1: Failing tests** (in `board.rs`'s `mod tests`):

```rust
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
    assert_eq!(boards.column_named(board, &columns[0].to_string()), Some(columns[0]));
    assert_eq!(boards.column_named(board, "Review"), None);
    assert_eq!(boards.column_named(9_999, "Doing"), None);
}

#[test]
fn assignee_and_comments_read_back_and_an_older_card_has_none() {
    let (mut boards, board, _, cards) = abc();
    boards.edit_card(board, cards[0], |c| {
        c.assignee = Some("b7k2m".into());
        c.comments.push(Comment { by: "b7k2m".into(), text: "On it.\nTwo lines.".into(), at: 1_760_000_000 });
        c.comments.push(Comment { by: "you".into(), text: "Thanks".into(), at: 1_760_000_100 });
    });
    let d = tempfile::tempdir().unwrap();
    save(d.path(), &boards, 1).unwrap();
    assert_eq!(load(d.path()), Loaded::Saved(boards));
    let text = std::fs::read_to_string(path(d.path())).unwrap();
    assert!(text.contains("\"assignee\": \"b7k2m\"") && text.contains("\"by\": \"you\""), "{text}");
    // A card without them writes neither key (an older Templum reads the file as before).
    let fresh = r#"{"version":1,"boards":[{"id":1,"name":"B","columns":[{"id":2,"title":"T","cards":[{"id":3,"title":"x"}]}]}]}"#;
    std::fs::write(path(d.path()), fresh).unwrap();
    let Loaded::Saved(read) = load(d.path()) else { panic!("not read") };
    let card = &read.boards[0].columns[0].cards[0];
    assert_eq!((card.assignee.as_deref(), card.comments.len()), (None, 0));
    save(d.path(), &read, 2).unwrap();
    let again = std::fs::read_to_string(path(d.path())).unwrap();
    assert!(!again.contains("assignee") && !again.contains("comments"), "{again}");
}
```

- [ ] **Step 2:** `cargo test -p templum board::` → FAIL (no `Comment`, `find_card`, `column_named`).
- [ ] **Step 3: Implement.** After `Card`'s `labels` field:

```rust
    /// The block id of the agent working on it (`claim_card`); the person frees it in the open card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
}

/// A comment on a card: by an agent's block id, or `"you"` for the person; `at` in Unix seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Comment {
    pub by: String,
    pub text: String,
    pub at: u64,
}
```

`add_card`'s literal gains `assignee: None, comments: Vec::new()`. In `impl Boards`:

```rust
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
            .or_else(|| columns.iter().find(|c| c.title.trim().to_lowercase() == title))
            .map(|c| c.id)
    }
```

- [ ] **Step 4:** `cargo test -p templum board::` → PASS; `cargo build -p templum` (no other `Card` literal: confirm
  with `rg "Card \{" crates/templum/src`).
- [ ] **Step 5: Commit** `feat(kanban): give cards an assignee and comments in boards.json`.

### Task 2: The nine tools (`templum-mcp/src/tools.rs`)

**Interfaces — Produces** (`Call` variants, in this order after `SpawnAgent`):

```rust
    ListCards { board: Option<String>, card: Option<u64> },
    CreateCard { board: Option<String>, column: Option<String>, title: String, description: Option<String> },
    MoveCard { card: u64, column: String },
    CommentCard { card: u64, text: String },
    ClaimCard { card: u64 },
    ReadNote { block: String },
    AppendNote { block: String, text: String },
    OpenFile { path: String, line: Option<u32> },
    GitStatus { block: Option<String> },
```

- [ ] **Step 1: Failing tests.** In `every_tool_listed_parses_its_example`, append to `examples` (same order as
  `list()`):

```rust
            ("list_cards", json!({ "board": "Sprint", "card": 12 })),
            ("create_card", json!({ "board": "1", "column": "To do", "title": "Fix login", "description": "It 500s." })),
            ("move_card", json!({ "card": 12, "column": "Doing" })),
            ("comment_card", json!({ "card": 12, "text": "On it." })),
            ("claim_card", json!({ "card": "12" })),
            ("read_note", json!({ "block": "4qm2a" })),
            ("append_note", json!({ "block": "4qm2a", "text": "Next: the tests." })),
            ("open_file", json!({ "path": "src/main.rs", "line": 12 })),
            ("git_status", json!({ "block": "g1t2k" })),
```

and a new test:

```rust
    #[test]
    fn the_content_tools_take_names_or_ids_and_leave_out_what_they_may() {
        assert_eq!(parse("list_cards", &json!({})), Ok(Call::ListCards { board: None, card: None }));
        assert_eq!(
            parse("list_cards", &json!({ "board": 1, "card": "7" })),
            Ok(Call::ListCards { board: Some("1".into()), card: Some(7) })
        );
        assert_eq!(
            parse("create_card", &json!({ "title": "x" })),
            Ok(Call::CreateCard { board: None, column: None, title: "x".into(), description: None })
        );
        assert_eq!(
            parse("move_card", &json!({ "card": 3, "column": 4 })),
            Ok(Call::MoveCard { card: 3, column: "4".into() })
        );
        assert_eq!(parse("claim_card", &json!({ "card": " 9 " })), Ok(Call::ClaimCard { card: 9 }));
        assert_eq!(
            parse("open_file", &json!({ "path": "a.rs" })),
            Ok(Call::OpenFile { path: "a.rs".into(), line: None })
        );
        assert_eq!(parse("git_status", &json!({ "block": "" })), Ok(Call::GitStatus { block: None }));
        for (tool, bad) in [
            ("move_card", json!({ "column": "Doing" })),
            ("move_card", json!({ "card": "x", "column": "Doing" })),
            ("move_card", json!({ "card": 1 })),
            ("comment_card", json!({ "card": 1 })),
            ("claim_card", json!({ "card": -1 })),
            ("create_card", json!({})),
            ("open_file", json!({ "path": "a.rs", "line": 0 })),
            ("open_file", json!({})),
            ("append_note", json!({ "block": "n" })),
        ] {
            assert!(parse(tool, &bad).is_err(), "{tool} {bad}");
        }
        let err = parse("move_card", &json!({ "column": "Doing" })).unwrap_err();
        assert!(err.contains("\"card\""), "{err}");
    }
```

- [ ] **Step 2:** `cargo test -p templum-mcp tools::` → FAIL.
- [ ] **Step 3: Schemas.** In `list()`, next to `block`:

```rust
    let board = json!({
        "type": "string",
        "description": "A board's name or id; may be left out when one kanban connected to you shows a board.",
    });
    let card = json!({ "type": "integer", "description": "A card's id, as list_cards shows it." });
    let column = json!({ "type": "string", "description": "A column's title (either case) or id." });
```

and after `spawn_agent`'s entry:

```rust
        {
            "name": "list_cards",
            "description": "A board shown by a kanban connected to you: its columns, and each card's id, title, \
                            labels, assignee and comment count. With card, that card whole: its description and \
                            comments too.",
            "inputSchema": { "type": "object", "properties": { "board": board, "card": card } },
        },
        {
            "name": "create_card",
            "description": "Adds a card at the end of a column (the board's first when left out) of a board shown \
                            by a kanban connected to you. Says its id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "board": board,
                    "column": column,
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                },
                "required": ["title"],
            },
        },
        {
            "name": "move_card",
            "description": "Moves a card to the end of a column of its board.",
            "inputSchema": {
                "type": "object",
                "properties": { "card": card, "column": column },
                "required": ["card", "column"],
            },
        },
        {
            "name": "comment_card",
            "description": "Adds a comment to a card, signed with your block's id.",
            "inputSchema": {
                "type": "object",
                "properties": { "card": card, "text": { "type": "string" } },
                "required": ["card", "text"],
            },
        },
        {
            "name": "claim_card",
            "description": "Makes you the card's assignee. Refused when another block holds it.",
            "inputSchema": { "type": "object", "properties": { "card": card }, "required": ["card"] },
        },
        {
            "name": "read_note",
            "description": "The text of the note a note block connected to you shows.",
            "inputSchema": { "type": "object", "properties": { "block": block }, "required": ["block"] },
        },
        {
            "name": "append_note",
            "description": "Adds text at the end of the note a note block connected to you shows, after a blank \
                            line. The person sees it at once and can take it back.",
            "inputSchema": {
                "type": "object",
                "properties": { "block": block, "text": { "type": "string" } },
                "required": ["block", "text"],
            },
        },
        {
            "name": "open_file",
            "description": "Shows a file, at a line, in the editor connected to you whose folder holds it; with \
                            none, in a new editor beside your block, connected to you. The person's keyboard and \
                            view stay where they are.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The file, relative to your folder or whole." },
                    "line": { "type": "integer", "minimum": 1, "description": "The line, from 1." },
                },
                "required": ["path"],
            },
        },
        {
            "name": "git_status",
            "description": "The branch, how far ahead and behind its upstream, and the changed files of the \
                            repository a Git block connected to you shows, as git status --short --branch prints \
                            them. block may be left out when one Git block is connected to you.",
            "inputSchema": { "type": "object", "properties": { "block": block } },
        },
```

- [ ] **Step 4: Parsing.** In `parse`, after `optional`:

```rust
    // Boards, columns and cards may come as numbers or as strings.
    let named = |key: &str| match &args[key] {
        Value::Null => Ok(None),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::String(s) => Ok(Some(s.trim().to_string()).filter(|s| !s.is_empty())),
        _ => Err(format!("\"{key}\" is a name or an id")),
    };
    let id = |key: &str| -> Result<Option<u64>, String> {
        let bad = || format!("\"{key}\" is an id, as list_cards shows it");
        match &args[key] {
            Value::Null => Ok(None),
            Value::Number(n) => n.as_u64().map(Some).ok_or_else(bad),
            Value::String(s) => s.trim().parse().map(Some).map_err(|_| bad()),
            _ => Err(bad()),
        }
    };
    let card = || id("card")?.ok_or_else(|| format!("{name} needs \"card\", a card's id"));
```

and the arms:

```rust
        "list_cards" => Call::ListCards { board: named("board")?, card: id("card")? },
        "create_card" => Call::CreateCard {
            board: named("board")?,
            column: named("column")?,
            title: text("title")?,
            description: optional("description")?,
        },
        "move_card" => Call::MoveCard {
            card: card()?,
            column: named("column")?.ok_or_else(|| format!("{name} needs \"column\", a title or an id"))?,
        },
        "comment_card" => Call::CommentCard { card: card()?, text: text("text")? },
        "claim_card" => Call::ClaimCard { card: card()? },
        "read_note" => Call::ReadNote { block: text("block")? },
        "append_note" => Call::AppendNote { block: text("block")?, text: text("text")? },
        "open_file" => Call::OpenFile {
            path: text("path")?,
            line: match &args["line"] {
                Value::Null => None,
                value => Some(
                    value
                        .as_u64()
                        .filter(|n| *n >= 1)
                        .map(|n| n.min(u64::from(u32::MAX)) as u32)
                        .ok_or_else(|| "\"line\" is a line number, from 1".to_string())?,
                ),
            },
        },
        "git_status" => Call::GitStatus { block: optional("block")? },
```

- [ ] **Step 5:** So every commit builds, `Workspace::call` (`crates/templum/src/workspace/bus.rs:306-392`) gets one
  temporary arm before `Call::AskUser`, which Task 4 replaces:

```rust
            Call::ListCards { .. }
            | Call::CreateCard { .. }
            | Call::MoveCard { .. }
            | Call::CommentCard { .. }
            | Call::ClaimCard { .. }
            | Call::ReadNote { .. }
            | Call::AppendNote { .. }
            | Call::OpenFile { .. }
            | Call::GitStatus { .. } => Reply::err("Not in this Templum yet."),
```

  `cargo test -p templum-mcp` → PASS; `cargo build -p templum` → builds.
- [ ] **Step 6: Commit** `feat(connections): describe the kanban, note, file and Git tools to agents`.

### Task 3: The verbs `kanban`, `note`, `open`, `git` (`templum-mcp/src/cli.rs`)

- [ ] **Step 1: Failing test** (in `cli.rs`'s tests):

```rust
    #[test]
    fn the_content_verbs_send_their_calls() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let _ = m.reply.send(tool_result(id, &Reply::ok("ok")));
            }
        });
        for (args, sent) in [
            (&["kanban", "list"][..], json!({ "name": "list_cards", "arguments": {} })),
            (
                &["kanban", "list", "Sprint", "--card", "12"][..],
                json!({ "name": "list_cards", "arguments": { "board": "Sprint", "card": "12" } }),
            ),
            (
                &["kanban", "add", "Fix", "login", "--column", "Doing", "--description", "It 500s."][..],
                json!({ "name": "create_card", "arguments": { "title": "Fix login", "column": "Doing", "description": "It 500s." } }),
            ),
            (
                &["kanban", "move", "12", "In", "review"][..],
                json!({ "name": "move_card", "arguments": { "card": "12", "column": "In review" } }),
            ),
            (
                &["kanban", "comment", "12", "On", "it."][..],
                json!({ "name": "comment_card", "arguments": { "card": "12", "text": "On it." } }),
            ),
            (&["kanban", "claim", "12"][..], json!({ "name": "claim_card", "arguments": { "card": "12" } })),
            (&["note", "read", "4qm2a"][..], json!({ "name": "read_note", "arguments": { "block": "4qm2a" } })),
            (
                &["note", "append", "4qm2a", "Next:", "tests"][..],
                json!({ "name": "append_note", "arguments": { "block": "4qm2a", "text": "Next: tests" } }),
            ),
            (&["open", "src/main.rs:12"][..], json!({ "name": "open_file", "arguments": { "path": "src/main.rs", "line": 12 } })),
            (&["open", "README.md"][..], json!({ "name": "open_file", "arguments": { "path": "README.md" } })),
            (&["open", r"C:\src\a.rs"][..], json!({ "name": "open_file", "arguments": { "path": r"C:\src\a.rs" } })),
            (&["git", "status"][..], json!({ "name": "git_status", "arguments": {} })),
            (&["git", "status", "g1t2k"][..], json!({ "name": "git_status", "arguments": { "block": "g1t2k" } })),
        ] {
            assert_eq!(run_with(args, &env(&bus), "").0, 0, "{args:?}");
            assert_eq!(calls.recv_timeout(LIMIT).unwrap(), sent, "{args:?}");
        }
        for (args, usage) in [
            (&["kanban"][..], "usage: templum kanban"),
            (&["kanban", "add", "--column", "Doing"][..], "usage: templum kanban"),
            (&["kanban", "move", "12"][..], "usage: templum kanban"),
            (&["kanban", "list", "--card"][..], "usage: templum kanban"),
            (&["note", "append", "4qm2a"][..], "usage: templum note"),
            (&["open"][..], "usage: templum open"),
            (&["git", "log"][..], "usage: templum git"),
        ] {
            let (code, _, err) = run_with(args, &env(&bus), "");
            assert_eq!(code, 2, "{args:?}");
            assert!(err.starts_with(usage), "{args:?}: {err}");
        }
    }
```

and in `only_the_verbs_are_commands`: `assert!(is_verb("kanban") && is_verb("note") && is_verb("open") && is_verb("git"));`

- [ ] **Step 2:** `cargo test -p templum-mcp cli::` → FAIL.
- [ ] **Step 3: Implement.** `VERBS` gains `"kanban", "note", "open", "git"`; constants:

```rust
const KANBAN: &str = "kanban list [<board>] [--card <card>] | add <title> [--column <column>] \
                      [--description <text>] [--board <board>] | move <card> <column> | comment <card> <text> \
                      | claim <card> [--json]";
const NOTE: &str = "note read <block> | note append <block> <text> [--json]";
```

Arms in `call_of`, before the usage fallbacks:

```rust
        ("kanban", ["list", more @ ..]) => {
            let mut arguments = json!({});
            let mut more = more.iter();
            while let Some(arg) = more.next() {
                match *arg {
                    "--card" => arguments["card"] = json!(more.next().ok_or(KANBAN)?),
                    board if !board.starts_with("--") && arguments.get("board").is_none() => {
                        arguments["board"] = json!(board)
                    }
                    _ => return Err(KANBAN),
                }
            }
            ("list_cards", arguments)
        }
        ("kanban", ["add", more @ ..]) => {
            let (mut title, mut arguments) = (Vec::new(), json!({}));
            let mut more = more.iter();
            while let Some(word) = more.next() {
                let key = match *word {
                    "--column" => "column",
                    "--description" => "description",
                    "--board" => "board",
                    word => {
                        title.push(word);
                        continue;
                    }
                };
                arguments[key] = json!(more.next().ok_or(KANBAN)?);
            }
            if title.is_empty() {
                return Err(KANBAN);
            }
            arguments["title"] = json!(title.join(" "));
            ("create_card", arguments)
        }
        ("kanban", ["move", card, column @ ..]) if !column.is_empty() => {
            ("move_card", json!({ "card": card, "column": column.join(" ") }))
        }
        ("kanban", ["comment", card, text @ ..]) if !text.is_empty() => {
            ("comment_card", json!({ "card": card, "text": text.join(" ") }))
        }
        ("kanban", ["claim", card]) => ("claim_card", json!({ "card": card })),
        ("note", ["read", block]) => ("read_note", json!({ "block": block })),
        ("note", ["append", block, text @ ..]) if !text.is_empty() => {
            ("append_note", json!({ "block": block, "text": text.join(" ") }))
        }
        ("open", [target]) => {
            let mut arguments = json!({});
            match target.rsplit_once(':') {
                Some((path, line)) if !path.is_empty() && line.parse::<u64>().is_ok() => {
                    arguments["path"] = json!(path);
                    arguments["line"] = json!(line.parse::<u64>().unwrap_or(1));
                }
                _ => arguments["path"] = json!(target),
            }
            ("open_file", arguments)
        }
        ("git", ["status"]) => ("git_status", json!({})),
        ("git", ["status", block]) => ("git_status", json!({ "block": block })),
```

and the fallbacks: `("kanban", _) => return Err(KANBAN), ("note", _) => return Err(NOTE), ("open", _) => return
Err("open <path>[:<line>] [--json]"), ("git", _) => return Err("git status [<block>] [--json]"),`.

- [ ] **Step 4:** `cargo test -p templum-mcp` → PASS.
- [ ] **Step 5: Commit** `feat(connections): templum kanban, note, open and git status in a Templum terminal`.

### Task 4: The kanban tools in the app

**Files:** Create `crates/templum/src/workspace/content.rs`, `crates/templum/src/workspace/tests/content.rs`. Modify
`workspace/bus.rs` (`call` at 295-393, `clean_title` at 523-546, `instructions` at 473-513), `workspace/mod.rs`
(`mod content;`, `Space::kanban` at 701-704 loses `#[cfg(test)]`), `workspace/tests/mod.rs` (`mod content;`;
`board_with_a_card` moves here from `tests/kanban.rs:243-262` as `pub(super)`).

**Interfaces:**
- Consumes: Task 1's `find_card`, `column_named`, `Comment`; Task 2's `Call` variants; `target`, `about`, `caller`,
  `space_named` (`bus.rs`); `Workspace::edit_boards` (`blocks.rs:97`); `notes::now`, `notes::ago` (`notes.rs:131`,
  `:139`); `tools::tail`, `tools::MAX_TEXT`.
- Produces (in `content.rs`, `impl Workspace`): `pub(super) fn list_cards(&self, from: &str, board: Option<&str>,
  card: Option<u64>, cx: &App) -> Reply`; `create_card(&mut self, from, board: Option<&str>, column: Option<&str>,
  title: &str, description: Option<String>, cx: &mut Context<Self>) -> Reply`; `move_card(&mut self, from, card: u64,
  column: &str, cx) -> Reply`; `comment_card(&mut self, from, card: u64, text: &str, cx) -> Reply`;
  `claim_card(&mut self, from, card: u64, cx) -> Reply`. In `bus.rs`: `pub(super) fn flat(text: &str) -> String`;
  `pub(crate) fn uid_title(&self, uid: &str, cx: &App) -> Option<String>` (Task 5 uses it).

- [ ] **Step 1: Failing tests** (`tests/content.rs`, which starts with
  `use serde_json::json; use super::bus::{call, me}; use super::*;`):

```rust
/// The startup terminal wired to a kanban block on a new board with one card ("New card") in To do:
/// (terminal's TEMPLUM_TERMINAL, its uid, the kanban block, the board, the card).
fn wired_board(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> (String, String, BlockId, u64, u64) {
    let terminal = first_id(ws, cx);
    let (kanban, board, card) = board_with_a_card(handle, ws, cx);
    ws.update(cx, |ws, cx| assert!(ws.connect(terminal, kanban, cx)));
    let (me, uid) = me(ws, cx, terminal);
    (me, uid, kanban, board, card)
}

#[gpui_kit::test]
fn list_cards_shows_the_wired_board_and_a_card_whole(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, uid, _, board, card) = wired_board(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            b.edit_card(board, card, |c| {
                c.title = "Fix\nlogin".into();
                c.description = "It 500s.\nOn Safari.".into();
                c.assignee = Some(uid.clone());
                c.comments.push(crate::board::Comment { by: "you".into(), text: "Please".into(), at: 0 });
            })
        })
    });
    let (failed, text) = call(handle, &ws, cx, &me, "list_cards", json!({}));
    assert!(!failed, "{text}");
    assert!(text.starts_with("Board \"Board 1\" ("), "{text}");
    assert!(text.contains(&format!("\n  {card}  Fix login  assignee {uid}")), "one line per card: {text}");
    assert!(text.contains("1 comment"), "{text}");
    assert!(text.contains("Doing (") && text.contains("no cards"), "{text}");
    let (failed, whole) = call(handle, &ws, cx, &me, "list_cards", json!({ "card": card }));
    assert!(!failed, "{whole}");
    assert!(whole.contains("description:\n  It 500s.\n  On Safari."), "{whole}");
    assert!(whole.contains("you") && whole.contains("Please"), "{whole}");
}

#[gpui_kit::test]
fn create_move_comment_and_claim_change_the_board(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, uid, _, board, card) = wired_board(handle, &ws, cx);
    let (failed, text) = call(
        handle, &ws, cx, &me, "create_card",
        json!({ "column": "doing", "title": "Write docs", "description": "README" }),
    );
    assert!(!failed, "{text}");
    // "Created card 7."
    let new: u64 = text
        .split_whitespace()
        .find_map(|w| w.trim_end_matches('.').parse().ok())
        .expect("its id");
    let (doing, done) = ws.read_with(cx, |ws, _| {
        let b = ws.boards.board(board).unwrap();
        (b.columns[1].id, b.columns[2].id)
    });
    assert_eq!(ws.read_with(cx, |ws, _| ws.boards.find_card(new)), Some((board, doing)));
    let (failed, text) = call(handle, &ws, cx, &me, "move_card", json!({ "card": card, "column": "DONE" }));
    assert!(!failed, "{text}");
    assert_eq!(ws.read_with(cx, |ws, _| ws.boards.find_card(card)), Some((board, done)));
    let (failed, text) = call(handle, &ws, cx, &me, "move_card", json!({ "card": card, "column": "Review" }));
    assert!(failed && text.contains("To do, Doing, Done"), "the columns listed: {text}");
    assert!(!call(handle, &ws, cx, &me, "comment_card", json!({ "card": card, "text": "Done." })).0);
    assert!(!call(handle, &ws, cx, &me, "claim_card", json!({ "card": card })).0);
    ws.read_with(cx, |ws, _| {
        let c = ws.boards.card(board, card).unwrap();
        assert_eq!(c.assignee.as_deref(), Some(uid.as_str()));
        assert_eq!((c.comments[0].by.as_str(), c.comments[0].text.as_str()), (uid.as_str(), "Done."));
    });
    assert!(call(handle, &ws, cx, &me, "comment_card", json!({ "card": card, "text": "  " })).0, "blank");
    assert!(call(handle, &ws, cx, &me, "create_card", json!({ "title": " " })).0, "blank");
}

#[gpui_kit::test]
fn claim_card_refuses_a_card_another_block_holds(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, _, board, card) = wired_board(handle, &ws, cx);
    ws.update(cx, |ws, _| ws.edit_boards(|b| b.edit_card(board, card, |c| c.assignee = Some("zzzzz".into()))));
    let (failed, text) = call(handle, &ws, cx, &me, "claim_card", json!({ "card": card }));
    assert!(failed && text.contains("zzzzz") && text.contains("free"), "{text}");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.boards.card(board, card).unwrap().assignee.clone()).as_deref(),
        Some("zzzzz")
    );
}

#[gpui_kit::test]
fn a_card_on_a_board_not_connected_is_refused(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, _, _, _) = wired_board(handle, &ws, cx);
    // A second kanban block, not wired, on a board of its own.
    let (_, other, theirs) = board_with_a_card(handle, &ws, cx);
    let before = ws.read_with(cx, |ws, _| ws.boards.clone());
    for (tool, args) in [
        ("move_card", json!({ "card": theirs, "column": "Doing" })),
        ("comment_card", json!({ "card": theirs, "text": "hi" })),
        ("claim_card", json!({ "card": theirs })),
        ("list_cards", json!({ "card": theirs })),
        ("list_cards", json!({ "board": other })),
    ] {
        let (failed, text) = call(handle, &ws, cx, &me, tool, args);
        assert!(failed, "{tool}: {text}");
    }
    let (_, unknown) = call(handle, &ws, cx, &me, "claim_card", json!({ "card": 99_999 }));
    let (_, unwired) = call(handle, &ws, cx, &me, "claim_card", json!({ "card": theirs }));
    assert_eq!(unknown.replace("99999", "N"), unwired.replace(&theirs.to_string(), "N"), "says no more");
    assert_eq!(ws.read_with(cx, |ws, _| ws.boards.clone()), before);
}

#[gpui_kit::test]
fn a_board_left_out_with_two_wired_is_refused_and_both_named(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, _, board, _) = wired_board(handle, &ws, cx);
    let (second, other, _) = board_with_a_card(handle, &ws, cx);
    let terminal = first_id(&ws, cx);
    ws.update(cx, |ws, cx| assert!(ws.connect(terminal, second, cx)));
    let (failed, text) = call(handle, &ws, cx, &me, "list_cards", json!({}));
    assert!(failed && text.contains(&format!("({board})")) && text.contains(&format!("({other})")), "{text}");
    assert!(!call(handle, &ws, cx, &me, "list_cards", json!({ "board": other })).0);
    assert!(!call(handle, &ws, cx, &me, "list_cards", json!({ "board": "board 1" })).0, "by name, either case");
}

#[gpui_kit::test]
fn ctrl_z_in_the_kanban_takes_back_an_agents_move(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, _, kanban, board, card) = wired_board(handle, &ws, cx);
    let todo = ws.read_with(cx, |ws, _| ws.boards.board(board).unwrap().columns[0].id);
    assert!(!call(handle, &ws, cx, &me, "move_card", json!({ "card": card, "column": "Doing" })).0);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(kanban, window, cx));
        window.press(UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(ws.read_with(cx, |ws, _| ws.boards.find_card(card)), Some((board, todo)));
}

#[gpui_kit::test]
fn the_content_tools_are_refused_without_a_wire(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let (me, _) = me(&ws, cx, terminal);
    let uid = |id| ws.read_with(cx, |ws, _| ws.space.uids[&id].clone());
    let (kanban_uid, note_uid) = (uid(kanban), uid(note));
    for (tool, args) in [
        ("list_cards", json!({})),
        ("create_card", json!({ "title": "x" })),
        ("read_note", json!({ "block": note_uid })),
        ("append_note", json!({ "block": note_uid, "text": "x" })),
        ("git_status", json!({})),
        ("read_note", json!({ "block": kanban_uid })),
    ] {
        let (failed, text) = call(handle, &ws, cx, &me, tool, args.clone());
        assert!(failed, "{tool} {args}: {text}");
    }
}

#[gpui_kit::test]
fn the_instructions_say_what_to_do_with_a_kanban_and_a_note(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (me, _) = me(&ws, cx, terminal);
    let text = ws.read_with(cx, |ws, cx| ws.instructions(&me, cx));
    assert!(text.ends_with(
        "When a connected kanban has your task, move its card to Doing when you start and to Review when you are \
         done; write what the next agent needs to know in the connected note."
    ), "{text}");
    let _ = handle;
}
```

(`instructions` is private to `bus.rs`: make it `pub(super)`. `UNDO`, `three_blocks`, `first_id`, `open` come from
`tests/mod.rs`. In `the_content_tools_are_refused_without_a_wire` the note and Git rows pass on the stubs at first;
they keep passing, for the real reason, once Tasks 6 and 8 replace the stubs.)

- [ ] **Step 2:** `cargo test -p templum content::` → FAIL (doesn't compile).
- [ ] **Step 3: `bus.rs`.** Split `clean_title`:

```rust
/// Text as agents are told it, on one line: no control characters, no invisible ones (zero widths,
/// direction overrides, soft hyphens, tags, which can carry hidden text), its line breaks and runs of
/// blanks as one space.
pub(super) fn flat(text: &str) -> String {
    let invisible = |c: char| { /* the same matches! as today */ };
    let kept: String = text
        .chars()
        .filter(|&c| (c.is_whitespace() || !c.is_control()) && !invisible(c))
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A title as agents are told it: `flat`, at most 60 characters.
pub(super) fn clean_title(title: &str) -> String {
    let flat = flat(title);
    if flat.chars().count() <= TITLE_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(TITLE_CHARS - 1).collect();
    format!("{cut}…")
}
```

After `about`:

```rust
    /// The title agents see for the block `uid`, in any workspace; None when none has it.
    pub(crate) fn uid_title(&self, uid: &str, cx: &App) -> Option<String> {
        self.spaces().find_map(|s| {
            let id = s.block_by_uid(uid)?;
            Some(self.about(s.blocks.get(&id)?, cx).1)
        })
    }
```

`instructions` becomes `pub(super)` and its `format!` ends `… list_blocks gives the current list. When a connected
kanban has your task, move its card to Doing when you start and to Review when you are done; write what the next
agent needs to know in the connected note."`. In `call`, Task 2's temporary arm becomes:

```rust
            Call::ListCards { board, card } => self.list_cards(terminal, board.as_deref(), card, cx),
            Call::CreateCard { board, column, title, description } => {
                self.create_card(terminal, board.as_deref(), column.as_deref(), &title, description, cx)
            }
            Call::MoveCard { card, column } => self.move_card(terminal, card, &column, cx),
            Call::CommentCard { card, text } => self.comment_card(terminal, card, &text, cx),
            Call::ClaimCard { card } => self.claim_card(terminal, card, cx),
            Call::ReadNote { block } => self.read_note(terminal, &block, cx),
            Call::AppendNote { block, text } => self.append_note(terminal, &block, &text, window, cx),
            Call::OpenFile { path, line } => self.open_file_for(terminal, &path, line, window, cx),
            Call::GitStatus { block } => return self.git_status(terminal, block.as_deref(), cx),
```

(The note, file and Git arms call methods Tasks 6–8 write; until then give each a stub in `content.rs` returning
`Reply::err("Not yet.")` / `Outcome::Now(..)` so the crate builds, and replace it in its task.)

- [ ] **Step 4: `content.rs`.**

```rust
//! The content tools (docs/specs/connections.md, Tools, batch 4): the cards of a kanban wired to the
//! caller, a wired note, a file opened in an editor, a wired Git block's status.

use gpui_kit::{App, Context};
use templum_mcp::{Reply, tools};

use super::bus::{flat, target};
use super::{BlockId, Space, Workspace};
use crate::board::{Board, Card, Comment};
use crate::notes;

impl Workspace {
    /// The caller's workspace and block: `call` found it.
    fn here(&self, from: &str) -> (&Space, BlockId) {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        (self.space_named(&space_id).expect("the caller's workspace"), me)
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
        if names.is_empty() { "none".into() } else { names.join(", ") }
    }

    /// The board `name` names among those `me` may use; the only one when it is left out.
    fn board_for(&self, space: &Space, me: BlockId, name: Option<&str>) -> Result<u64, Reply> {
        let boards = self.wired_boards(space, me);
        match name {
            None => match boards.as_slice() {
                [one] => Ok(*one),
                [] => Err(Reply::err(
                    "No kanban connected to you shows a board. Ask the person with connect, or have them pick one.",
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
                            self.boards.board(*b).is_some_and(|x| x.name.trim().to_lowercase() == lower)
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
        Reply::err(format!("There's no column {column}. Its columns: {}.", titles.join(", ")))
    }

    pub(super) fn list_cards(&self, from: &str, board: Option<&str>, card: Option<u64>, cx: &App) -> Reply {
        let (space, me) = self.here(from);
        if let Some(card) = card {
            let board = match self.card_board(space, me, card) {
                Ok(b) => b,
                Err(refused) => return refused,
            };
            return Reply::ok(tools::tail(&self.card_whole(board, card, cx), tools::MAX_TEXT));
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
        let column = b.columns.iter().find(|col| col.cards.iter().any(|x| x.id == card));
        let indent = |text: &str| text.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n");
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
        let (space, me) = self.here(from);
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

    pub(super) fn move_card(&mut self, from: &str, card: u64, column: &str, cx: &mut Context<Self>) -> Reply {
        let (space, me) = self.here(from);
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

    pub(super) fn comment_card(&mut self, from: &str, card: u64, text: &str, cx: &mut Context<Self>) -> Reply {
        let (space, me) = self.here(from);
        let board = match self.card_board(space, me, card) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        if text.trim().is_empty() {
            return Reply::err("A comment needs text.");
        }
        let by = space.uids.get(&me).cloned().unwrap_or_default();
        let comment = Comment { by, text: text.trim_end().to_string(), at: notes::now() };
        self.edit_boards(|b| b.edit_card(board, card, |c| c.comments.push(comment)));
        cx.notify();
        Reply::ok(format!("Commented on card {card}."))
    }

    pub(super) fn claim_card(&mut self, from: &str, card: u64, cx: &mut Context<Self>) -> Reply {
        let (space, me) = self.here(from);
        let board = match self.card_board(space, me, card) {
            Ok(b) => b,
            Err(refused) => return refused,
        };
        let mine = space.uids.get(&me).cloned().unwrap_or_default();
        match self.boards.card(board, card).and_then(|c| c.assignee.clone()) {
            Some(holder) if holder == mine => return Reply::ok(format!("Card {card} is already yours.")),
            Some(holder) => {
                return Reply::err(format!(
                    "Card {card} is taken by {}. Ask the person to free it (the ✕ by its assignee in the open \
                     card), or take another card.",
                    self.who(&holder, cx)
                ));
            }
            None => {}
        }
        self.edit_boards(|b| b.edit_card(board, card, |c| c.assignee = Some(mine)));
        cx.notify();
        Reply::ok(format!("Card {card} is yours."))
    }
}
```

(Remove the unused `Board` import if clippy says so. `space.kanban` needs the `#[cfg(test)]` dropped in `mod.rs`.)

- [ ] **Step 5:** `cargo test -p templum content::` → the kanban tests PASS (the note/Git rows of
  `the_content_tools_are_refused_without_a_wire` too, since the stubs refuse; they stay green after Tasks 6 and 8).
  `cargo test -p templum` → the rest stays green (`kanban::` tests use `board_with_a_card` from `mod.rs` now).
- [ ] **Step 6: Commit** `feat(connections): agents read, add, move, comment on and claim cards of a wired kanban`.

### Task 5: The card's assignee and comment count; the open card's comments

**Files:** Modify `crates/templum/src/kanban.rs` (`card_view` 419-506, `open_card` 535-676), `workspace/tests/kanban.rs`.

- [ ] **Step 1: Failing tests** (in `tests/kanban.rs`):

```rust
#[gpui_kit::test]
fn a_card_shows_its_assignee_and_how_many_comments(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (block, board, card) = board_with_a_card(handle, &ws, cx);
    let shown = |cx: &mut TestAppContext, what: &str| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let id = gpui_kit::ElementId::Name(format!("{what}-{}-{card}", block.0).into());
            window.try_find(id).is_some()
        })
        .unwrap()
    };
    assert!(!shown(cx, "assignee") && !shown(cx, "comments"));
    let terminal = first_id(&ws, cx);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&terminal].clone());
    ws.update(cx, |ws, _| {
        ws.edit_boards(|b| {
            b.edit_card(board, card, |c| {
                c.assignee = Some(uid.clone());
                c.comments.push(crate::board::Comment { by: "you".into(), text: "hi".into(), at: 0 });
            })
        })
    });
    assert!(shown(cx, "assignee") && shown(cx, "comments"));
    // The title it shows is the block's, as agents see it.
    assert!(ws.read_with(cx, |ws, cx| ws.uid_title(&uid, cx)).is_some());
}

#[gpui_kit::test]
fn the_open_card_adds_a_comment_and_frees_its_assignee(cx: &mut TestAppContext) {
    no_motion(cx);
    let (handle, ws) = open(cx);
    let (block, board, card) = board_with_a_card(handle, &ws, cx);
    ws.update(cx, |ws, _| ws.edit_boards(|b| b.edit_card(board, card, |c| c.assignee = Some("zzzzz".into()))));
    // Opened as `tests/kanban.rs:100-112` opens one; the dialog slides in for 0.25 s first.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, card).cloned().unwrap();
            crate::kanban::open_card(board, current, false, window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let _ = block;
    // Type into the comment field, then Add comment.
    cx.update_window(handle, |_, window, cx| {
        window.click("comment-field", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_input(handle, "Looks good");
    cx.update_window(handle, |_, window, cx| {
        window.click("add-comment", cx);
        window.render_frame(cx);
        window.click("unassign", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let c = ws.boards.card(board, card).unwrap();
        assert_eq!(c.comments.len(), 1);
        assert_eq!((c.comments[0].by.as_str(), c.comments[0].text.as_str()), ("you", "Looks good"));
        assert_eq!(c.assignee, None);
    });
    // Each is a step of the kanban's Ctrl+Z: the assignee comes back first.
    ws.update(cx, |ws, cx| assert!(ws.undo_boards(cx)));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.boards.card(board, card).unwrap().assignee.clone()).as_deref(),
        Some("zzzzz")
    );
}
```

(`window.click` and `try_find` take `impl Into<ElementId>` (`gpui-kit-0.7.3/src/test.rs:27-29`); fixed ids such as
`"add-comment"` are clicked as `tests/kanban.rs:123` clicks `"label-green"`. `undo_boards` is `pub(super)` in
`workspace/blocks.rs:109`: reachable from `workspace/tests`.)

- [ ] **Step 2:** `cargo test -p templum kanban::` → FAIL.
- [ ] **Step 3: The card's face.** In `card_view`, replace the description-icon `.when(..)` with one footer row:

```rust
        .when(
            !card.description.is_empty() || !card.comments.is_empty() || card.assignee.is_some(),
            |el| {
                let count = card.comments.len();
                let who = card
                    .assignee
                    .as_ref()
                    .map(|uid| ws.uid_title(uid, cx).unwrap_or_else(|| uid.clone()));
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0 * z))
                        .text_size(px(11.0 * z))
                        .text_color(ui.title_dim)
                        .when(!card.description.is_empty(), |el| {
                            el.child(Icon::new(IconName::FileText).size(px(12.0 * z)).text_color(ui.title_dim))
                        })
                        .when(count > 0, |el| {
                            el.child(
                                div()
                                    .id(name("comments", n, id))
                                    .child(if count == 1 { "1 comment".to_string() } else { format!("{count} comments") })
                                    .test_support(),
                            )
                        })
                        .when_some(who, |el, who| {
                            el.child(
                                div()
                                    .id(name("assignee", n, id))
                                    .ml_auto()
                                    .min_w_0()
                                    .truncate()
                                    .child(who)
                                    .test_support(),
                            )
                        }),
                )
            },
        )
```

- [ ] **Step 4: The open card.** In `open_card`, next to `description`:
  `let comment = cx.new(|cx| TextareaState::new(window, cx).rows(2));`. In the dialog's closure, after `toggles`:

```rust
        let (assignee, comments) = ws_now
            .boards
            .card(board, card)
            .map(|c| (c.assignee.clone(), c.comments.clone()))
            .unwrap_or_default();
        let now = notes::now();
        let author = |by: &str| {
            if by == "you" {
                "You".to_string()
            } else {
                ws_now.uid_title(by, cx).unwrap_or_else(|| by.to_string())
            }
        };
        let assignee = assignee.map(|uid| (author(&uid), ws.clone()));
        let comments: Vec<_> = comments
            .iter()
            .map(|c| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(ui.title_dim)
                            .child(format!("{} · {}", author(&c.by), notes::ago(c.at, now))),
                    )
                    .child(div().text_size(px(13.0)).child(c.text.clone()))
            })
            .collect();
        let add = {
            let (ws, comment) = (ws.clone(), comment.clone());
            div()
                .id("add-comment")
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child("Add comment")
                .on_click(move |_: &ClickEvent, window, cx| {
                    let text = comment.read(cx).value().trim().to_string();
                    if text.is_empty() {
                        return;
                    }
                    ws.update(cx, |ws, cx| {
                        let said = Comment { by: "you".into(), text, at: notes::now() };
                        ws.edit_boards(|b| b.edit_card(board, card, |c| c.comments.push(said)));
                        cx.notify();
                    });
                    comment.update(cx, |c, cx| c.set_value("", window, cx));
                })
                .test_support()
        };
```

and in the dialog's column, before the delete row:

```rust
                    .when_some(assignee, |el, (who, ws)| {
                        el.child(field("Assignee")).child(
                            div().flex().items_center().gap(px(6.0)).child(who).child(
                                div()
                                    .id("unassign")
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(ui.hover))
                                    .child(Icon::new(IconName::Close).size(px(12.0)))
                                    .on_click(move |_: &ClickEvent, _, cx| {
                                        ws.update(cx, |ws, cx| {
                                            ws.edit_boards(|b| b.edit_card(board, card, |c| c.assignee = None));
                                            cx.notify();
                                        })
                                    })
                                    .test_support(),
                            ),
                        )
                    })
                    .child(field("Comments"))
                    .children(comments)
                    .child(div().id("comment-field").child(Textarea::new(&comment)).test_support())
                    .child(div().flex().justify_end().child(add))
```

(Use the icon name gpui-component has for a close mark: `rg "Close" ~/.cargo/registry/src/*/gpui-component-0.7.3/src/icon.rs`.
The comment field is a `Textarea`: Enter is a new line there, so it never confirms the dialog the way the title's
Enter does. `Comment` and `notes` are imported at the top of `kanban.rs`. Build the `author` strings before any
mutable use of `cx` so `ws_now`'s borrow ends.)

- [ ] **Step 5:** `cargo test -p templum kanban::` → PASS. Draw a card by hand once (`cargo run -p templum`, a board,
  `templum kanban claim <card>` from a wired terminal is Task 3+4 — or set the fields in a test) and note what you saw.
- [ ] **Step 6: Commit** `feat(kanban): show a card's assignee and comments, comment on it and free it`.

### Task 6: `read_note` and `append_note`

**Files:** Modify `workspace/content.rs` (replace the two stubs), `workspace/tests/content.rs`.

- [ ] **Step 1: Failing tests:**

```rust
/// The startup terminal wired to the note "hello" of `three_blocks`: (TEMPLUM_TERMINAL, note block, note's uid).
fn wired_note(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) -> (String, BlockId, String) {
    let (_, note, terminal) = three_blocks(handle, ws, cx);
    ws.update(cx, |ws, cx| assert!(ws.connect(terminal, note, cx)));
    let (me, _) = me(ws, cx, terminal);
    let uid = ws.read_with(cx, |ws, _| ws.space.uids[&note].clone());
    (me, note, uid)
}

fn note_editor(ws: &Entity<Workspace>, cx: &mut TestAppContext, note: BlockId) -> Entity<EditorState> {
    ws.read_with(cx, |ws, _| ws.space.blocks[&note].note().unwrap().tab().unwrap().editor.clone())
}

#[gpui_kit::test]
fn read_note_reads_what_the_note_shows(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, note, uid) = wired_note(handle, &ws, cx);
    assert_eq!(call(handle, &ws, cx, &me, "read_note", json!({ "block": uid })), (false, "hello".to_string()));
    let terminal = first_id(&ws, cx);
    let (_, mine) = super::bus::me(&ws, cx, terminal);
    let (failed, text) = call(handle, &ws, cx, &me, "read_note", json!({ "block": mine }));
    assert!(failed && text.contains("isn't a note"), "{text}");
    let _ = note;
}

#[gpui_kit::test]
fn append_note_shows_at_once_keeps_the_caret_and_one_ctrl_z_takes_it_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (me, note, uid) = wired_note(handle, &ws, cx);
    let editor = note_editor(&ws, cx, note);
    // The person is in the note, the caret after "he".
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(note, window, cx));
        editor.update(cx, |e, cx| e.set_selected_range(2..2, cx));
    })
    .unwrap();
    let (failed, text) = call(handle, &ws, cx, &me, "append_note", json!({ "block": uid, "text": "Next: tests" }));
    assert!(!failed, "{text}");
    cx.run_until_parked();
    let id = ws.read_with(cx, |ws, _| ws.space.blocks[&note].note().unwrap().tab().unwrap().id);
    editor.read_with(cx, |e, _| {
        assert_eq!(e.value().as_ref(), "hello\n\nNext: tests");
        assert_eq!(e.selected_range(), 2..2, "the caret stays");
    });
    assert_eq!(ws.read_with(cx, |ws, _| ws.notes.get(id).unwrap().text.clone()), "hello\n\nNext: tests");
    cx.update_window(handle, |_, window, cx| {
        window.press(UNDO, cx);
        window.render_frame(cx);
    })
    .unwrap();
    editor.read_with(cx, |e, _| assert_eq!(e.value().as_ref(), "hello"));
    // A note ending in a newline gets one more; an empty one none.
    editor.update(cx, |e, cx| e.set_selected_range(5..5, cx));
    cx.update_window(handle, |_, window, cx| {
        editor.update(cx, |e, cx| e.insert("\n", window, cx));
    })
    .unwrap();
    call(handle, &ws, cx, &me, "append_note", json!({ "block": uid, "text": "x" }));
    editor.read_with(cx, |e, _| assert_eq!(e.value().as_ref(), "hello\n\nx"));
    assert!(call(handle, &ws, cx, &me, "append_note", json!({ "block": uid, "text": " \n" })).0, "blank");
}
```

(If `e.value()` returns `SharedString`, compare with `.as_ref()` as written; `EditorState` is imported in
`tests/notes.rs` — import it the same way.)

- [ ] **Step 2:** `cargo test -p templum content::` → FAIL.
- [ ] **Step 3: Implement** in `content.rs`:

```rust
    /// The note block `block` wired to the caller, and the editor of the note it shows.
    fn wired_note(&self, from: &str, block: &str) -> Result<gpui_kit::Entity<gpui_kit::EditorState>, Reply> {
        let (space, me) = self.here(from);
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
```

(Use the `EditorState` path the crate already imports in `note.rs`. `Window` comes from `gpui_kit`.)

- [ ] **Step 4:** `cargo test -p templum content::` → PASS.
- [ ] **Step 5: Commit** `feat(connections): agents read a wired note and add to its end`.

### Task 7: `open_file`

**Files:** Modify `workspace/content.rs` (replace the stub `open_file_for`), `workspace/tests/content.rs`. Uses
`crate::editor::{read_file, Editor, SIZE}` (`editor.rs:93`, `:341`), `launcher::expand`, `Space::home`,
`canvas.place_beside`, `add_block`, `add_wire`, `in_space` (as `spawn.rs:122-141`), `Body::has_focus`
(`mod.rs:356`), `Editor::open_tab` (`editor.rs:425`), `Editor::focus` (`editor.rs:477`).

- [ ] **Step 1: Failing tests:**

```rust
/// A folder with a.rs (five lines) and b.bin (binary), canonicalized.
fn a_project() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
    std::fs::write(dir.path().join("b.bin"), [0u8, 159, 146, 150, 0, 1]).unwrap();
    let path = std::fs::canonicalize(dir.path()).unwrap();
    (dir, path)
}

#[gpui_kit::test]
fn open_file_lands_at_the_line_in_the_wired_editor(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let editor = open_editor(handle, &ws, cx, &folder);
    cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| {
        assert!(ws.connect(terminal, editor, cx));
        ws.focus_block(terminal, window, cx);
        ws.space.flight = None;
    }))
    .unwrap();
    let camera = ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    let (me, _) = me(&ws, cx, terminal);
    let path = folder.join("a.rs");
    let (failed, text) = call(handle, &ws, cx, &me, "open_file", json!({ "path": path, "line": 3 }));
    assert!(!failed, "{text}");
    ws.read_with(cx, |ws, cx| {
        let e = ws.space.editor(editor).unwrap();
        let tab = e.active_tab().unwrap();
        assert_eq!(tab.path, path);
        assert_eq!(tab.editor.read(cx).cursor_position().line, 2, "line 3");
        assert_eq!(ws.space.focused, Some(terminal), "the keyboard stays");
        assert!(ws.space.flight.is_none(), "the camera stays");
        assert_eq!(ws.space.canvas.camera, camera);
    });
}

#[gpui_kit::test]
fn open_file_with_no_wired_editor_opens_one_beside_the_caller(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    // An editor holding the file, but not wired: not the one used.
    let unwired = open_editor(handle, &ws, cx, &folder);
    cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.focus_block(terminal, window, cx))).unwrap();
    let (me, _) = me(&ws, cx, terminal);
    let blocks = ws.read_with(cx, |ws, _| ws.space.blocks.len());
    let (failed, text) = call(handle, &ws, cx, &me, "open_file", json!({ "path": folder.join("a.rs") }));
    assert!(!failed && text.contains("new editor"), "{text}");
    let new = ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.blocks.len(), blocks + 1);
        let wire = ws.space.wires.iter().rfind(|w| w.touches(terminal)).expect("wired to the caller");
        let new = if wire.a == terminal { wire.b } else { wire.a };
        assert_ne!(new, unwired);
        assert_eq!(ws.space.editor(new).unwrap().folder, folder, "the file's folder");
        assert_eq!(ws.space.focused, Some(terminal), "the keyboard stays");
        new
    });
    // One Ctrl+Z closes it with its wire.
    cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.undo_canvas(false, window, cx))).unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(!ws.space.blocks.contains_key(&new));
        assert!(!ws.space.wires.iter().any(|w| w.touches(new)));
    });
}

#[gpui_kit::test]
fn open_file_refuses_what_it_cant_open_and_opens_no_block(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (me, _) = me(&ws, cx, terminal);
    let blocks = ws.read_with(cx, |ws, _| ws.space.blocks.len());
    for path in [folder.join("nope.rs"), folder.clone(), folder.join("b.bin")] {
        let (failed, text) = call(handle, &ws, cx, &me, "open_file", json!({ "path": path }));
        assert!(failed, "{}: {text}", path.display());
    }
    assert_eq!(ws.read_with(cx, |ws, _| ws.space.blocks.len()), blocks);
}

#[gpui_kit::test]
fn an_editor_with_the_keyboard_gives_it_to_the_tab_opened(cx: &mut TestAppContext) {
    let (_dir, folder) = a_project();
    std::fs::write(folder.join("c.rs"), "c\n").unwrap();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let editor = open_editor(handle, &ws, cx, &folder);
    let (me, _) = me(&ws, cx, terminal);
    cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| {
        assert!(ws.connect(terminal, editor, cx));
        let e = ws.space.editor_mut(editor).unwrap();
        e.open_tab(&folder.join("c.rs"), window, cx).unwrap();
        assert!(ws.space.editor(editor).unwrap().focus(window, cx));
    }))
    .unwrap();
    call(handle, &ws, cx, &me, "open_file", json!({ "path": folder.join("a.rs") }));
    cx.update_window(handle, |_, window, cx| {
        let ws = ws.read(cx);
        let tab = ws.space.editor(editor).unwrap().active_tab().unwrap();
        assert!(tab.path.ends_with("a.rs"));
        // The active tab's own editor has the keys (not the hidden c.rs tab's).
        assert!(tab.editor.read(cx).focus_handle(cx).is_focused(window), "the new tab has the keys");
    })
    .unwrap();
}
```

(`Space::flight` / `canvas.camera` / `undo_canvas` are the names the existing tests use: `tests/editor.rs:312-388`,
`tests/bus.rs:307-378`. Find the input's focus-handle accessor in `gpui-base-0.7.3/src/input/base/state.rs`
(`impl Focusable`) before writing this assertion; the point is that the *active* tab's editor is focused.)

- [ ] **Step 2:** `cargo test -p templum content::open_file` → FAIL.
- [ ] **Step 3: Implement:**

```rust
    /// `open_file` from `from`: [Decision 8].
    pub(super) fn open_file_for(
        &mut self,
        from: &str,
        path: &str,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space, me) = self.here(from);
        let here = space.blocks[&me].folder().or_else(|| space.terminal(me)?.seen.folder.clone());
        let named = crate::launcher::expand(path);
        let path = match &here {
            Some(here) if named.is_relative() => here.join(named),
            _ => named,
        };
        if path.is_relative() {
            return Reply::err("Templum doesn't know your folder yet: give the file's whole path.");
        }
        let Ok(path) = std::fs::canonicalize(&path) else {
            return Reply::err(format!("There's no file {}.", path.display()));
        };
        if !path.is_file() {
            return Reply::err(format!("{} isn't a file.", path.display()));
        }
        if let Err(why) = crate::editor::read_file(&path) {
            return Reply::err(why);
        }
        let holds = |folder: &std::path::Path| {
            std::fs::canonicalize(folder).is_ok_and(|f| path.starts_with(f))
        };
        let wired = space
            .canvas
            .blocks()
            .iter()
            .rev()
            .map(|b| b.id)
            .filter(|id| *id != me && space.wired(me, *id))
            .find(|id| space.editor(*id).is_some_and(|e| holds(&e.folder)));
        let folder = here
            .filter(|h| holds(h))
            .unwrap_or_else(|| path.parent().unwrap_or(&path).to_path_buf());
        let space_id = space.id.clone();
        let opened = self.in_space(&space_id, |ws| {
            let (id, new) = match wired {
                Some(id) => (id, false),
                None => {
                    let anchor = ws.space.home(ws.space.canvas.get(me)?);
                    let rect = ws.space.canvas.place_beside(anchor, crate::editor::SIZE);
                    let kept = (ws.space.focused, ws.space.selected.clone(), ws.space.selected_wire);
                    // In grid mode a new block takes half of the focused tile: the caller's.
                    ws.space.focused = Some(me);
                    ws.space.pristine = false;
                    let editor = crate::editor::Editor::new(folder.clone(), true, cx);
                    let id = ws.add_block(rect, Body::Editor(Box::new(editor)));
                    (ws.space.focused, ws.space.selected, ws.space.selected_wire) = kept;
                    ws.space.add_wire(me, id);
                    (id, true)
                }
            };
            let had_keys = ws.space.blocks.get(&id)?.has_focus(window, cx);
            let editor = ws.space.editor_mut(id)?;
            let i = editor.open_tab(&path, window, cx).ok()?;
            if let Some(line) = line {
                let at = Position { line: line - 1, character: 0 };
                editor.tabs[i].editor.update(cx, |e, cx| {
                    // Not set_cursor_position: that would take the keyboard.
                    let offset = e.text().position_to_offset(&at);
                    e.set_selected_range(offset..offset, cx);
                });
            }
            if had_keys {
                editor.focus(window, cx);
            }
            Some((ws.space.uids.get(&id)?.clone(), new))
        });
        cx.notify();
        let at = line.map(|n| format!(" at line {n}")).unwrap_or_default();
        match opened.flatten() {
            Some((uid, false)) => Reply::ok(format!("Opened {}{at} in {uid}.", path.display())),
            Some((uid, true)) => Reply::ok(format!(
                "Opened {}{at} in a new editor {uid} beside you, connected to you.",
                path.display()
            )),
            None => Reply::err(format!("Couldn't open {}.", path.display())),
        }
    }
```

(`Position` is the type `files.rs:140` uses; import it the same way. `Body` from `super`. Check `in_space` swaps the
caller's workspace in when it is hidden, as `spawn.rs` relies on.)

- [ ] **Step 4:** `cargo test -p templum content::` → PASS.
- [ ] **Step 5: Commit** `feat(connections): agents open a file at a line in an editor without taking your keyboard`.

### Task 8: `git_status`

**Files:** Modify `workspace/content.rs` (replace the stub), `workspace/tests/content.rs`; move `answer_soon` from
`tests/spawn.rs:48-60` to `tests/bus.rs` as `pub(super)` (with its imports) and import it in `spawn.rs`.

- [ ] **Step 1: Failing test:**

```rust
#[gpui_kit::test]
fn git_status_matches_the_repository(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "x\n").unwrap();
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let git = open_git(handle, &ws, cx, repo.path());
    let (me, _) = me(&ws, cx, terminal);
    let answers = super::bus::call_later(handle, &ws, cx, &me, 1, "git_status", json!({}));
    let (failed, text) = super::bus::answer_soon(cx, &answers);
    assert!(failed && text.contains("No Git block is connected to you"), "{text}");
    ws.update(cx, |ws, cx| assert!(ws.connect(terminal, git, cx)));
    let answers = super::bus::call_later(handle, &ws, cx, &me, 2, "git_status", json!({}));
    let (failed, text) = super::bus::answer_soon(cx, &answers);
    assert!(!failed, "{text}");
    let expected = git_run(repo.path(), &["-c", "color.status=false", "status", "--short", "--branch"]);
    assert_eq!(text, expected.trim_end());
    assert!(text.starts_with("## main") && text.contains(" M a.txt") && text.contains("?? new.txt"), "{text}");
}
```

- [ ] **Step 2:** `cargo test -p templum content::git_status` → FAIL.
- [ ] **Step 3: Implement:**

```rust
    /// `git_status`: the wired Git block named, or the only one; git runs on the background executor.
    pub(super) fn git_status(&self, from: &str, block: Option<&str>, cx: &mut Context<Self>) -> Outcome {
        let (space, me) = self.here(from);
        let id = match block {
            Some(uid) => match target(space, me, uid) {
                Ok(id) => id,
                Err(refused) => return Outcome::Now(refused),
            },
            None => {
                let wired: Vec<BlockId> = space
                    .canvas
                    .blocks()
                    .iter()
                    .map(|b| b.id)
                    .filter(|id| *id != me && space.wired(me, *id) && space.git(*id).is_some())
                    .collect();
                match wired.as_slice() {
                    [one] => *one,
                    [] => {
                        return Outcome::Now(Reply::err(
                            "No Git block is connected to you. Ask the person with connect.",
                        ));
                    }
                    _ => {
                        let ids: Vec<&str> =
                            wired.iter().filter_map(|id| space.uids.get(id).map(String::as_str)).collect();
                        return Outcome::Now(Reply::err(format!(
                            "Several Git blocks are connected to you: {}. Name one with block.",
                            ids.join(", ")
                        )));
                    }
                }
            }
        };
        let uid = space.uids.get(&id).cloned().unwrap_or_default();
        let Some(git) = space.git(id) else {
            return Outcome::Now(Reply::err(format!("{uid} isn't a Git block.")));
        };
        let Some(repo) = git.repo().cloned() else {
            return Outcome::Now(Reply::err(format!(
                "{uid} shows no Git repository (yet: try again in a moment if it is still looking)."
            )));
        };
        Outcome::Later(cx.background_executor().spawn(async move {
            match repo.run(&["-c", "color.status=false", "status", "--short", "--branch"]) {
                Ok(out) => Reply::ok(tools::tail(String::from_utf8_lossy(&out).trim_end(), tools::MAX_TEXT)),
                Err(e) => Reply::err(format!("git status failed: {}", e.first_line())),
            }
        }))
    }
```

(`Outcome` from `super::bus`; `GitError::first_line` is what `spawn.rs:89` uses.)

- [ ] **Step 4:** `cargo test -p templum content::` → PASS (all of `content.rs`, the refusals test included);
  `cargo test -p templum spawn::` → PASS (on Linux; `answer_soon` moved).
- [ ] **Step 5: Commit** `feat(connections): agents read a wired Git block's status`.

### Task 9: Docs, minors, the full check, the benches, CI

**Files:** `docs/specs/connections.md`, `docs/ARCHITECTURE.md`, `README.md`, `docs/minors.md`.

- [ ] **Step 1: The spec says what was built.** **Status:** "Batches 1 to 4 implemented; batch 5 planned". Tools table:
  `list_cards`'s arguments "`board`, `card`" and its "Does" "… comment count; with `card`, that card whole: its
  description and comments"; `create_card`'s "… at the column's end (the board's first column when left out); its
  id"; `git_status`'s arguments "`block` (may be left out when one Git block is wired)" and "… as `git status --short
  --branch` prints them". The bullet on names gains: "`board`, `column` and `card` may be numbers or strings; a blank
  title, text or comment is refused." Command-line rows: `templum kanban list [<board>] [--card <card>] | add <title>
  [--column <c>] [--description <d>] [--board <b>] | move <card> <column> | comment <card> <text> | claim <card>`.
  Files' `boards.json` bullet ends: "… its dialog lists the comments, adds the person's, and frees the assignee (✕),
  each a step of the kanban's Ctrl+Z; `claim_card` refuses a card another block holds until the person frees it."
  Architecture table: `templum/src/workspace/content.rs` (new) | the kanban, note, file and Git tools | 4.
- [ ] **Step 2:** `docs/ARCHITECTURE.md`, workspace's source list after `spawn.rs (…)`: "`content.rs` (the tools on a
  wired block's content: a kanban's cards, a note, a file opened in an editor, a Git block's status)". `README.md`,
  "Agents that talk to the canvas": "An agent wired to a kanban reads its cards and creates, moves, comments on and
  claims them; it reads a wired note and adds to it, opens a file at a line in an editor, and reads a Git block's
  status." and the commands sentence gains `templum kanban`, `templum note`, `templum open`, `templum git status`.
- [ ] **Step 3: Minors** under `## Connections`, "Left by batch 4 (content); line numbers are those of `development`
  at X." with at least:
  - **`list_cards` lists a board left on a kanban showing its board list.** Decision 3 counts `Kanban::board` while the
    block picks: an agent reads a board the person no longer sees in that block. Fix: count `shown()` only, if that
    surprises anyone.
  - Any minor the implementation or a review defers, each with where, what a person sees and a fix.
- [ ] **Step 4: The full check:** `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo
  test --workspace` → all green.
- [ ] **Step 5: The benches** (this batch paints the card's face): `cargo build --release -p templum`, then
  `cargo run --release -p templum -- --bench` and `-- --bench-mixed` → each `p95_ms` ≤ 16.7; keep both JSON lines.
  Over the bar: once more; over again, stop and report.
- [ ] **Step 6: CI.** `gh run list --branch development --limit 3`: report Task 0's run (batch 3 on Windows and
  macOS). A failure there is fixed before batch 4 is called done (it's batch 3's), in its own `fix(…)` commit.
- [ ] **Step 7: Commit** `docs(connections): describe the kanban, note, file and Git tools` with the Claude-Session
  line.

## Manual checks (Hyprland)

1. **Kanban:** a Claude Code from **AI agent ▸** wired to a kanban on a board with To do, Doing, Done. Ask it to list
   the cards, take the first one, move it to Doing and comment. The card shows Claude Code's title and "1 comment";
   the open card lists the comment; add one of yours; ✕ frees the assignee; Ctrl+Z (kanban focused) brings it back.
2. **A second agent** (Codex) wired to the same kanban tries to claim that card: refused, told to ask you.
3. **Note:** wired to a note you are typing in; ask the agent to append "Next: …": it appears after a blank line,
   your caret stays; Ctrl+Z in the note removes it.
4. **open_file:** ask the agent to show `src/main.rs` at line 40: with a wired editor on the project it opens there,
   scrolled to line 40, the keyboard still in the agent's terminal and the camera still; with none, a new editor
   opens beside the agent, wired; Ctrl+Z closes it.
5. **git_status:** a Git block wired to the agent; change a file; ask for the status: it matches `git status -sb`.
6. **CLI:** in a plain Templum terminal wired to the kanban: `templum kanban list`, `templum kanban add "Try it"
   --column doing`, `templum open README.md:3`, `templum git status`.
