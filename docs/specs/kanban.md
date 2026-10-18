# Kanban boards

- **Date:** 2026-10-10
- **Status:** Implemented; [version 2](#version-2-kanban-card-40) (who a card is for, priority, a checklist, the
  card's id, scrolling while dragging, the open card redone) implemented
- **Builds on:** [workspaces.md](workspaces.md) and [persistence.md](persistence.md)

## Goal

The canvas becomes a whiteboard for work, not only terminals. A **kanban block** sits on the canvas like a terminal
and shows a **board**, Trello-style: columns of cards that you drag around. Boards are the user's projects. They
live outside any canvas, so any number of blocks, in any workspace, can show any board, and two blocks showing the
same board stay in sync.

**Success criteria**

1. Every behavior below works on Linux, macOS and Windows. CI checks this on every OS, and Linux (Hyprland) is also
   checked by hand.
2. Boards and kanban blocks come back after a restart.
3. All tests in [Testing](#testing) pass in CI on the 3 OS, and `--bench` stays at p95 ≤ 16.7 ms.

## Out of scope

- Due dates, members, attachments, search, and archiving. (A card's checklist came with
  [version 2](#version-2-kanban-card-40).)
- Label names: a label is one of six colors.

## A kanban block

- **Opening one:** the title bar's board button (gpui-kit's dashboard icon), or the `new_kanban` command
  (Ctrl+Shift+K, Cmd+Shift+K on macOS). It opens at the viewport's center, 900 × 540, and pops in like a terminal.
- **The card around it** is a terminal's: the header with the title, ✕ and the resize grip, moving, resizing,
  focus, hover lift and the pop-in and fade-out. It can't be smaller than 420 × 280.
- **Its title** is the board's name. The title stays the handle for moving the block; a ⇅ button at the left of
  the header shows the board list in the block (and goes back to the board).
- **Everything inside scales with the zoom**, like a terminal's text.
- **Undo:** with a kanban block focused, Ctrl+Z (Cmd+Z on macOS) undoes the last change to the boards (a card or a
  column added, moved, renamed or deleted, a label, a board added or deleted), up to 50. Typing in a card's dialog
  is undone in its fields. Delete card in a card's menu asks first. A comment added, an assignee freed, and an agent's
  change ([connections.md](connections.md#tools)) are steps of this Ctrl+Z too.

### The board list

Shown by a new block, by the ⇅ button, and when its board was deleted.

- One row per board: the name, and how many cards it has. Clicking a row shows that board in the block.
- **+ New board** creates "Board *n*" with three columns, To do, Doing and Done, and shows it.
- **Right-click a row:** Rename… and Delete…. Deleting asks first, saying how many cards go with it. Blocks showing
  a deleted board show the list.

### The board

- **Columns** sit side by side, 260 wide, and scroll sideways when they don't fit. Each has its title and its
  number of cards. **+ Add a column** after the last one adds "New column".
  - **Right-click a column's title:** Rename…, Add a card and Delete. Deleting a column with cards asks first.
    Every menu is listed in [context-menus.md](context-menus.md).
  - **Dragging a column's title** onto another column moves it to that column's place.
- **Cards** stack in their column, which scrolls when they don't fit.
  - A card shows its labels as small colored bars, its title, a line icon when it has a description, how many
    comments it has ("2 comments"), and its assignee's block title at the right. (Version 2 adds its id, priority
    and checklist: [its face](#a-cards-face).)
  - **+ Add a card** at the bottom of a column adds "New card" there and opens it.
  - **Dragging a card** onto another card puts it in that card's place; onto a column's empty space, at the end
    of that column. While dragged, the card follows the pointer and the drop target is highlighted.
  - **Clicking a card** opens it.

### An open card

[Version 2](#the-open-card) redoes this dialog; this is the first one.

A dialog in the middle of the window, at the window's scale:

- the title, in a text field;
- the description, in a multi-line text field;
- the six label colors (red, yellow, green, cyan, blue, magenta, from the theme), each a toggle;
- **Assignee**, when an agent claimed the card (`claim_card`, [connections.md](connections.md#tools)): its block's
  title, and a ✕ that frees the card;
- **Comments**, oldest first, each with its author (an agent's block title, or "You") and how long ago it was written;
  a text field under them and **Add comment**, which adds the person's. An agent that finishes its card says there
  what it did and what the next one needs to know (`comment_card`, [connections.md](connections.md#tools));
- **Delete card**, and **Done** (or Esc) to close.

Every change applies as it is typed or clicked, so every block showing the board updates at once.

## Files

- **`<data folder>/boards.json`** holds every board:

  ```json
  {
    "version": 1,
    "next_id": 12,
    "boards": [
      {
        "id": 1,
        "name": "Templum",
        "columns": [
          { "id": 2, "title": "To do", "cards": [
            { "id": 5, "title": "Kanban", "description": "Trello-like", "labels": ["green"],
              "assignee": "b7k2m",
              "comments": [{ "by": "b7k2m", "text": "On it.", "at": 1760000000 }] }
          ] }
        ]
      }
    ]
  }
  ```

  - Ids are unique across boards, columns and cards, and never reused (`next_id`). A file edited by hand keeps them
    so: an id given twice gets a new one after its first, and `next_id` passes them all.
  - Missing `description`, `labels` and `comments` mean empty, a missing `assignee` none; a card with no assignee
    or comments writes neither key. An unknown label color is dropped.
  - `assignee` is the block id of the agent that claimed the card. A comment's `by` is an agent's block id, or
    `"you"` for the person; `at` is in Unix seconds.
- **A canvas file** gets a second kind of item: `{ "kind": "kanban", "rect": {…}, "board": 1 }`. `board` is
  missing for a block showing the board list. A Templum from before this spec sets such a file aside
  ([persistence.md](persistence.md#the-file)), as designed.
- **Saving:** `boards.json` is written by the 5-second snapshot when the boards changed, and on quit, through the
  same temporary file and rename. A file Templum can't read is kept as `boards.json.bak` and the boards start empty.

## Architecture

| File | Change |
|---|---|
| `board.rs` (new) | The model (`Boards`, `Board`, `Column`, `Card`, `Label`) and every edit: add, rename, remove, move. Serde, as `session.rs` does. Pure and unit-tested. |
| `kanban.rs` (new) | The kanban block's body (board list, columns, cards, drag and drop) and the card dialog. |
| `prompt.rs` (new) | The text-field and confirmation dialogs, shared with the sidebar. |
| `block.rs` | The card around any block; the terminal's body moves to its own function. |
| `session.rs` | `Item::Kanban`; the layout and snapshot of kanban blocks; reading and writing `boards.json`. |
| `workspace.rs` | A space's kanban blocks; the boards, saved with the snapshot; the `new_kanban` command. |
| `title_bar.rs` | The board button. |

The board model lives in the app crate with its serde types, as `session.rs` and `config.rs` do: it is pure and
tested headlessly, and a copy without serde in `templum-canvas` would have to be kept in step by hand.

## Testing

Tests are written first, as always.

**Unit (`board`):**

- a new board has the three columns;
- ids are never reused, across boards and after removals;
- moving a card within a column and to another column, to the end and before another card;
- moving a column;
- removing a column removes its cards;
- toggling a label, and editing a card;
- a file written and read back is equal; an unknown label is dropped.

**Unit (`session`):** a canvas with a kanban item reads back equal.

**Headless UI:**

- the board button opens a kanban block showing the board list;
- **+ New board** creates a board and shows it in the block;
- two blocks showing the same board both show a card added through one of them;
- deleting a board sends its blocks back to the list;
- kanban blocks and boards come back after a restart.

**Manual (Hyprland):** drag cards and columns at zoom 0.5, 1 and 2; open, edit and delete a card; scroll a long
column.

## Delivery

1. This spec.
2. The board model and `boards.json`.
3. The kanban block: list, board, drag and drop, the card dialog, the docs.

## Risks

- **Drag and drop on a zoomed canvas:** the dragged card is drawn at the canvas's zoom, and drop targets come from
  the element tree, so they follow the zoom by themselves.
- **Wheel events** over a kanban block stay in it, as over a terminal: a vertical wheel scrolls a column's cards,
  a sideways one the row of columns, and Ctrl+wheel zooms the canvas. GPUI measures a scroll container's range
  from its own children, so the columns are the row's children, with no wrapper in between.

## Version 2 (kanban card 40)

*Implemented (kanban card 40): [docs/plans/2026-10-18-kanban-v2.md](../plans/2026-10-18-kanban-v2.md).* The card, as
the person wrote it:

> aprimorar sistema kanban, tipo quero poder ter tarefas para o claude e tarefas para o codex, delegar tarefas para
> agentes específicos, ou caso tarefa não tiver nenhum agente, qualquer um pode pegar ela, ou sistema de sessão
> conectada, tipo eu posso ter 2 agentes claude mas tipo eu poderia querer que uma task fosse feita por uma sessão
> específica do claude. Também ter sistema de prioridade (low, medium e high), poder ter checklist dentro da tarefa.
> Corrija também quando estou com uma task no drag and drop e arrasto ela deve ir movimentando o kanban, tanto dentro
> da coluna ou entre colunas, para tipo movimentar a tela para scroll automático. Mostrar o ID da task do kanban no
> card e nos detalhes do card. Melhorar visualmente o detalhes da task do kanban, está bem feio e botão espaço errado.

### Goal

The board becomes the person's way to hand work to the agents on the canvas. A card can be for anyone, for any session
of one agent (Claude Code, Codex, any agent Templum knows), or for one session (one of two Claude Code blocks); agents
see which cards are theirs and `claim_card` refuses one meant for another. A card gets a priority and a checklist, and
shows its id. Dragging a card near the edge of a column or of the board scrolls it. The open card is redone, clean and
in Templum's look, with its buttons where they belong.

**Success criteria**

1. Everything below works on Linux, macOS and Windows at zoom 0.25, 0.5, 1 and 2 (the dialog at the window's scale).
2. A `boards.json` from before loads unchanged, and a card that uses none of the new things writes none of their keys.
3. All tests in [Testing](#testing-1) pass in CI on the 3 OS; `--bench` and `--bench-mixed` stay at p95 ≤ 16.7 ms.

### Who a card is for

A card's **assignee** says who does it. It is one field, as before, with one more kind of value:

| Assignee | Means | Shown on the card | Saved as |
|---|---|---|---|
| none | anyone may take it | nothing | no `assignee` key |
| an agent | any session of that agent may take it | "Any Codex" | `"assignee": "agent:codex"` (the agent's id) |
| a block | that block's agent does it: one session | its block's title ("Claude Code · api"), else its id | `"assignee": "b7k2m"` (its block id, as before) |

- **An agent** is one of `Workspace::agents`: Templum's own and the person's `[agents]`, named by id (`claude`,
  `codex`, an `[agents]` key) and shown by title. An account of an agent (`claude2`) is one too; a card for it is
  its own ([accounts.md](accounts.md#cards-for-it), planned).
- **A block is one session.** A block keeps its id across restarts, Ctrl+Z and a move to another workspace, and its
  agent resumes its own session in it ([agents.md](agents.md#the-agents), "One session per block"), so a card for a
  block stays for that session. A card for a closed block keeps the id; the person changes it.
- **Which agent a block runs:** the one Templum sees running in it (`Pane::seen.agent`, read every 5 s), else the one
  Templum started there (`Pane::flags_for`, set by `spawn_agent` before the first read). A block with neither runs no
  agent Templum can name yet.
- **A block runs "any X"** when its agent is X, or runs the same program as X (the first word of their commands,
  without its folder or `.exe`, in any case): a person's `[agents]` entry `claude-work = "~/bin/claude --settings …"`
  runs `claude`, which Templum sees as Claude Code, so a
  card for any claude-work is taken by a Claude Code block and the other way round. Otherwise a card for such an entry
  could never be taken.
- **Claiming** turns the assignee into the claiming block (as before): a card for any Codex, claimed by a Codex block,
  becomes that block's. Choosing another assignee in the open card replaces it, whoever held it.

#### In the open card

The **Assignee** row ([layout](#the-open-card)) holds a dropdown button showing the current value ("Anyone", "Any
Codex", a block's title), and, when there is one, a ✕ (`unassign`) that sets Anyone. Its menu:

1. **Anyone**.
2. A separator, the label **Sessions**, then one item per terminal block running an agent (as above), in every
   workspace: the active one first, then the others in the sidebar's order, each back to front. An item reads the
   block's title as agents see it (`Workspace::uid_title`), followed by " — {workspace name}" when the block is in
   another workspace. The current block, when it is in none of them (closed, or its agent quit), comes first, by its
   title or its id. The section is left out when it would be empty.
3. A separator and a submenu **Any session of**, listing every agent of `Workspace::agents` in their order, by title.

The current value is checked. Each choice, and the ✕, is one step of the kanban's Ctrl+Z.

#### For agents

Tools ([connections.md](connections.md#cards-for-an-agent)): `list_cards` says each card's assignee and marks the
caller's own with `(you)`; `create_card` and the new `edit_card` take `for`; `claim_card` refuses a card meant for
another agent or another block, saying why, and so does `edit_card`'s `for` (an agent hands out cards nobody holds,
and never gives itself one meant for another); `move_card`, `comment_card`, and `edit_card`'s priority and checklist
stay open to every wired agent (an agent that hands work out, or one that reviews, moves, ticks and comments on others'
cards), and `move_card` says when the card is for someone else.

**Assigning a card tells nobody.** It types nothing into the agent and wires nothing: an agent learns of its cards in
`list_cards` on a kanban wired to it, as before, and the person hands a card over as before (Send to ▸, a delegation,
the prompt).

### Priority

- **None, Low, Medium or High.** Saved as `"priority": "low" | "medium" | "high"`; missing is none; a value Templum
  doesn't know reads as none (a typo mustn't lose the file).
- **On the card:** a signal icon in its meta row ([the face](#a-cards-face)): lucide `signal-low` in `title_dim`,
  `signal-medium` in the theme's yellow (ANSI 3), `signal-high` in its red (ANSI 1).
- **In the open card:** the Priority row's four chips, None, Low, Medium, High; a click sets it, one Ctrl+Z step.
- **Nothing is sorted by it.** A column's order is the person's plan, set by dragging; reordering on its own would
  fight drag and drop and move cards while someone looks at them. Priority is a cue, for the person and for agents.
- **Agents** read it in `list_cards` and set it with `create_card` and `edit_card`.

### A checklist in a card

- **Items** have an id, a text and whether they are done: the same item as a checklist block's
  (`checklists::Item`), read from an agent's text the same way (`checklists::parse_items`: one item per line, a
  leading `- ` or `* ` dropped, `[x]` checks it). A card's items live in the card, in `boards.json`, with ids from the
  boards' `next_id` (unique across boards, columns, cards and items, never reused). A checklist block can't show a
  card's checklist, nor a card a checklist block's: two places for one list would have to be kept in step.
- **Saved** as `"checklist": [{ "id": 41, "text": "Build" }, { "id": 42, "text": "Test", "done": true }]`; missing is
  none; an unchecked item writes no `done`. A hand edit mustn't lose the file: a `checklist` that isn't a list is none,
  an item that doesn't read (`"done": "yes"`) is dropped, and one with no id gets a new one. `labels` that aren't a
  list are none too.
- **On the card:** its progress, "2/5", in the meta row, in the accent color once every item is done; nothing without
  items.
- **In the open card:** its section ([layout](#the-open-card)): a progress bar, one row per item (a check box, its
  text, a ✕ under the pointer that deletes it), and a field that adds what is typed when Enter is pressed (several
  lines pasted make several items) and keeps the dialog open. Checking, adding and deleting are each one Ctrl+Z step.
- **Not in version 2:** editing an item's text in place, or dragging items. Delete it and add it again.
- **Agents** read the items in `list_cards` with `card`, add them with `create_card`'s `checklist` and `edit_card`'s
  `add_items`, and check or uncheck them with `edit_card`.

### The card's id

- **On the card:** "#40", first in its meta row, which every card now has.
- **In the open card:** its title, "Card #40".
- Agents already name cards by it.

### Scrolling while dragging

While a card is dragged, a container under it scrolls when the pointer is near its edge:

- **A column's cards** scroll up or down when the pointer is near the top or bottom of the column's card list, and
  over the column sideways (another column's pointer never scrolls this one).
- **The board's columns** scroll sideways when the pointer is near the left or right of the block's body, and within
  its height. A column dragged by its title scrolls them too.
- **How fast:** the canvas's own edge rule for a marquee (`templum_canvas::edge_scroll`: a 40-unit zone, faster the
  nearer the edge, 900 a second at the edge or past it), measured in **world units**: zone and speed scale with the
  zoom like everything in the block, so the board moves the same at zoom 0.25 and 2 (only resizing changes a board's
  layout, never the zoom). The pointer may leave the container on that side, by up to
  one zone: full speed; further off it stops, and over another block at once, since every container hears every
  drag.
- It runs each frame while the pointer holds still (a frame asked for while it moves), from the time since the last
  frame, at most 0.05 s, so a stalled frame doesn't jump; it stops at the ends and when the drag ends or the pointer
  leaves the zone. Drop targets follow what scrolled under the pointer.

### A card's face

World units, each × the zoom `z` (as now: padding 10 × 8, gap 6, radius 8, the hairline border, the light shadow):

1. **Labels** (when it has any): unchanged, bars 32 × 6, gap 4.
2. **Title:** unchanged, 13.
3. **Meta row**, always: items centered, gap 8, text 11 in `title_dim`, in this order:
   - `#40` (element `card-id-{block}-{card}`);
   - the priority's signal icon, 12 (`priority-{block}-{card}`), when it has one;
   - the checklist: lucide `list-checks` 12 and "2/5", gap 3 (`checklist-{block}-{card}`), accent once all are done,
     when it has items;
   - the description's `file-text` icon, 12, as now, when it has one;
   - the comments: lucide `message-square` 12 and their number, gap 3 (`comments-{block}-{card}`, as now), when it has
     any. It replaces "2 comments", which no longer fits beside the rest;
   - at the right (`ml_auto`, truncated): the assignee as the table above shows it (`assignee-{block}-{card}`).

### The open card

A dialog at the window's scale (z = 1), **560 wide** (was 520), its body scrolling past the window's height (gpui-kit
does: the header and the footer stay). Sizes in pixels.

- **Header:** the dialog's title "Card #40"; its description "In {column} on {board}" (gpui-kit's muted line), read
  each frame, so an agent's move shows.
- **Body**, a column with a gap of 16 between these blocks:
  1. **Title:** the single-line field (`card-title`), gpui-kit's default size. Enter closes the dialog (as now).
  2. **Properties:** a column with a gap of 8; each row items centered, a gap of 12, at least 28 high, a label 84 wide
     (text 12, `title_dim`), then its control:
     - **Assignee:** the dropdown button (gpui-kit `Button`, small, outline, with a caret; `card-assignee`) and, when
       set, the ✕: 24 × 24, rounded 6, gpui-kit's close icon at 12 in `title_dim`, `ui.hover` under the pointer
       (`unassign`).
     - **Priority:** four chips with a gap of 6 (`priority-none`, `-low`, `-medium`, `-high`): 26 high, padding 10
       sideways, rounded 6, text 12, a gap of 4 between the icon (12, in its color; None has none) and the word. The
       current one: `ui.hover` fill, `ui.accent` border, text `ui.title`; the others: `ui.hairline` border, text
       `ui.title_dim`, `ui.hover` fill under the pointer.
     - **Labels:** the six toggles as now (40 × 22, rounded 6, gap 6, `label-{name}`).
  3. **Description:** the section label (text 12, medium weight, `title_dim`), a gap of 6, the multi-line field
     (5 rows), as now.
  4. **Checklist:** a header row, the section label "Checklist" at the left and, with items, "2/5" at the right (text
     12, `title_dim`); a gap of 6; with items, a progress bar 4 high, rounded, `ui.hover` behind and `ui.accent` for the
     done part; then the item rows (each padding 4 up and down, gap 8, items at the top): the check box as a checklist
     block draws it (14, rounded 3, a `title_dim` border; done: filled with `ui.accent` and a check at 10 in
     `ui.surface`; `card-item-box-{item}`), the text at 13 (done: `title_dim`, struck through), and at the right a 16
     × 16 ✕ (close icon 10, `title_dim`, rounded 4, `ui.hover` under the pointer, `card-item-delete-{item}`) shown
     while the pointer is over its row; last, the single-line field with the placeholder "Add an item"
     (`card-item-field`).
  5. **Comments:** a header row, "Comments" at the left and their number at the right when there are any; a gap of 6;
     each comment, oldest first, a gap of 10 between them: its author and age (text 11, `title_dim`: "You · 3m ago"),
     a gap of 2, its text at 13; then the comment field (2 rows, `comment-field`); a gap of 8; at the right, the small
     gpui-kit button **Add comment** (`add-comment`).
- **Footer** (gpui-kit's, under the body), one row spread across: at the left **Delete card** (`delete-card`), a
  ghost button: no fill or border, gpui-kit's `trash` icon at 14 and the words at 13, both in the theme's red (ANSI 1),
  28 high, padding 10, rounded 6, `ui.hover` under the pointer; it deletes the card and closes the dialog, as before
  (Ctrl+Z brings it back). At the right **Done** (`card-done`), gpui-kit's primary button, which closes it. Esc closes
  it too.

**The button in the wrong place** was Delete card: a red word in the body, right-aligned just above Done, so it read as
a second footer button stacked on the first. It moves to the footer's other end, away from Done. Add comment, a bare
word under the comment field, becomes a real button.

### Agents' tools

[connections.md](connections.md#cards-for-an-agent) has the tools' arguments and answers. In short: `list_cards` shows
each card's priority, assignee (`(you)` on the caller's) and checklist progress, and with `card` its items;
`create_card` takes `priority`, `for` and `checklist`; `edit_card` (new) sets `priority` and `for`, adds items and
checks or unchecks them; `claim_card` follows [who a card is for](#who-a-card-is-for). The command line's
`templum kanban add` and `templum kanban edit` do the same.

### Files

`boards.json` keeps `"version": 1`: older Templums read the new files (serde skips keys it doesn't know). A card may
now hold:

```json
{ "id": 40, "title": "Kanban v2", "priority": "high", "assignee": "agent:codex",
  "checklist": [{ "id": 41, "text": "Model" }, { "id": 42, "text": "Tools", "done": true }],
  "comments": [] }
```

An older Templum shows `agent:codex` as an assignee it can't find, and refuses claims on it. It reads the file, but
its next save drops every priority and checklist (it keeps only the keys it knows): going back to it, then forward
again, loses them.

### Calls made for card 40

The person was away; each can be changed.

1. **One field for who does a card.** "For any Codex" and "claimed by this block" are the same `assignee`, so claiming
   keeps working as before and there is no second rule to explain; a claimed card forgets that it was for any Codex
   (the person sees who holds it, and can set it back).
2. **A block is a session.** No session ids on cards: the block id already outlives restarts, and its agent resumes
   its session there.
3. **`move_card`, `comment_card` and `edit_card`'s priority and items aren't gated, its `for` is:** agents that hand out or review work need them on others' cards,
   and the person's Ctrl+Z undoes a wrong move. `move_card`'s answer says when the card is someone else's. Without a
   gate on `for`, an agent refused by `claim_card` could set `for` to itself, or free a card another block holds.
10. **Assigning tells nobody** (above): delivering a card to its agent (typing it, wiring the block) is a flow of its
   own, and the existing ones still work.
11. **Same program, same agent** (above), so no entry of the menu is a card nobody can take.
4. **Priority sorts nothing** (above).
5. **A card's checklist is its own**, sharing only the item type and the parsing with checklist blocks.
6. **One new tool, `edit_card`,** rather than three (priority, who, items): each tool costs every agent context.
7. **Scrolling speed in world units**, from the canvas's edge rule, rather than a second set of constants.
8. **The meta row on every card**, for the id: a bare card grows by one line; ids in the same place on every card
   read better than an id at the start of each title.
9. **The assignee menu's agents in a submenu:** Templum knows 20 and more agents; the sessions running now are what
   the person picks most.
12. **`templum kanban edit`'s `--add` may come more than once**, an item each, like `--check` and `--uncheck`.
13. **A dialog's own Ctrl+Z is the board's:** what the dialog changes (a chip, a box, an item, the menu) is one step
    of the kanban's Ctrl+Z, which keeps `next_id` (an undone item's id is never given out again).
14. **Several lines pasted into "Add an item" are added at once,** an item each: the field is one line, so they
    can't wait there for Enter.
15. **Past an edge, one zone at most, and never over another block:** a drag further off, or over a block beside
    (tiles 8 apart), scrolls nothing, so a card dragged in one block never scrolls another block's board.

### Considered and left out

- **A separate "for" beside the assignee:** two fields to keep consistent (a card for Codex held by a Claude block?).
- **Gating every tool on the assignee:** an agent handing out work couldn't move the cards it handed out.
- **Showing a checklist block's list in a card** (a card naming a `checklists.json` id): deleting it in the
  Checklists dialog would empty a card, and Ctrl+Z would have two stores to step back.
- **Columns sorted by priority, a filter "cards for me" in the block:** nobody asked; the badge and `(you)` cover it.
- **Send to ▸ assigning the card** to the terminal it types into: the person also sends a card to an agent just to
  read it.

### Testing

Tests are written first, as always.

**Unit (`board`):** a card's priority, checklist and an agent assignee read back equal and write the keys above; a
card from before reads with none (`"assignee": "b7k2m"` is a block) and writes none; an unknown priority reads as
none; items get ids never given before, check, uncheck and go; a file whose item ids pass `next_id` loads with
`next_id` after them.

**Unit (`kanban`):** the scroll speed is zero in the middle and across another column, grows toward an edge, is full
past it, and its zone doubles in pixels at zoom 2.

**Unit (`templum-mcp`):** `create_card` and `edit_card` parse their new arguments (a priority in either case, `check`
as one id or a list), refuse an unknown priority and an `edit_card` with nothing to change; the command line's verbs
build them.

**Headless UI:**

- a card shows its id always, and its priority and checklist progress when it has them;
- the open card sets a priority (one Ctrl+Z step), adds an item with Enter and stays open, checks and deletes it,
  frees its assignee, deletes the card from the footer, and closes with Done;
- the assignee menu offers Anyone, the agent sessions of every workspace and every agent;
- dragging a card near a long column's bottom scrolls its cards; near the board's right edge, its columns;
- the tools: `list_cards` shows priority, assignee, `(you)` and progress, and the items with `card`; `create_card` and
  `edit_card` set them; `claim_card` takes a card for anyone or for its agent (or an agent running the same program),
  refuses one for another agent or block, and says when it can't tell which agent it is; `edit_card`'s `for` refuses
  the same; `move_card` says when the card is someone else's.

**Manual (Hyprland):** drag cards to every edge at zoom 0.5, 1 and 2, in a long column and a wide board; the open
card in light and dark themes, a long checklist and many comments (the body scrolls, the footer stays); the assignee
menu with two Claude Code blocks and a Codex one, one of them in another workspace.

### Risks

- **A menu inside a dialog:** gpui-kit's dropdown opens over the dialog; if a click on its item reaches the dialog's
  overlay, the menu is the first thing to check by hand. The ✕ and the tools still set the assignee.
- **Enter in the "Add an item" field:** gpui-kit's single-line input propagates Enter, which confirms (closes) an alert
  dialog. The field's wrapper takes the dialog's `Confirm` action so Enter only adds; a test holds it.
- **Scrolling and drop targets:** GPUI highlights drop targets from the last frame's hit boxes; scrolling moves them
  every frame, so the highlight may trail by a frame. Harmless.
