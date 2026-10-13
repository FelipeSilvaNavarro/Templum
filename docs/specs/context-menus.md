# Right-click menus

- **Date:** 2026-10-10
- **Status:** Implemented
- **Builds on:** [kanban.md](kanban.md) and [workspaces.md](workspaces.md)

## Goal

A right-click anywhere in Templum opens a menu for what is under the pointer, and the empty canvas gets one to add
blocks where you clicked. One system serves every menu, so a new block kind or action is one more entry.

**Success criteria**

1. Every menu below opens on Linux, macOS and Windows, at the pointer, and only one menu is ever open.
2. All tests in [Testing](#testing) pass in CI on the 3 OS.

## Before this spec

gpui-kit's `context_menu` was used in three places: the sidebar's tiles, the kanban board list's rows and the kanban
columns' titles. It opens a menu for every wrapped element under the pointer, so a menu on a card inside a block
would have opened together with the block's. The canvas and the blocks had no menu.

## The menus

| Right-click on | Items |
|---|---|
| The empty canvas | **AI agent ▸** (the agents), **Add ▸** Terminal, Kanban board, Note, Editor, Git. The block's top-left corner goes where you clicked. |
| A terminal block (header or body) | Copy (disabled without a selection), Paste, Close terminal |
| A kanban block (header, or its empty space) | Board list (or Back to the board, while the list shows), Close |
| A note block | Preview (or Edit), Copy all, Send to ▸ (the workspace's terminals), Color ▸ (None and the six label colors), Close |
| An editor block | Change folder…, Hide (or Show) the file tree, Turn soft wrap on (or off), Send to ▸, Close |
| A file in an editor's tree | Open, Reveal in file manager, File history, Blame (in the Git block open on its repository, else a new one beside the editor), Rename…, Delete… |
| A folder in an editor's tree | New file…, New folder…, Open a terminal here, Rename…, Delete…, Reveal in file manager |
| An editor tree's empty space | New file…, New folder…, Refresh |
| A Git block | Change folder…, Refresh, Open a terminal here, Close |
| The ⋯ of a Git block's top bar (a click) | Fetch, Pull, Push, Recently discarded… (with a data folder), Clone a repository… |
| A Git block's Publish branch, with several remotes (a click) | Publish to *remote*, one per remote, as `git remote` lists them |
| The ▾ beside a Git block's Commit | Commit (or Commit all changes), and the same & Push |
| The terminal count in a Git block's top bar | The terminals working in its repository, agents first; the one chosen is shown |
| A row of a Git block's Changes | Stage (or Unstage), Discard changes… (unstaged rows), Open, File history, Blame (not on a new file: it has no history), Copy path, Ignore; a conflict: Open in editor, Use the version from *HEAD*, Use the version from *the other side's label* (the markers' labels, never "ours" or "theirs"), Mark resolved (asks while markers are left), Copy path, Ignore |
| A line of a Git block's diff panel | Copy (disabled without a selection), Copy line, Copy hunk, Send to ▸ (the selection, else the hunk), Open in editor at this line |
| A commit in a Git block's History | Checkout…, New branch here…, Create tag…, Revert…, Undo this commit (the newest, not pushed), Copy hash, Copy message, Restore this version… (in a file's history, unless the commit deleted the file) |
| A file of the commit shown in a Git block's History | File history, Blame (as the file was in that commit), Copy path |
| A branch in a Git block | Compare with *main* (the default branch; not on itself), Merge into *current*…, Rename…, Delete… (an unmerged one asks again with its count), Publish (a local branch with no upstream), Copy name |
| A stash in a Git block | Apply, Pop, Drop…, Show |
| The toolbar's AI agent button | The agents; the chosen one opens in the middle of the canvas |
| A board in a block's board list | Open, Rename…, Delete… |
| A column's title | Rename…, Add a card, Delete |
| A card | Open, Send to ▸, Delete card (asks first) |
| A workspace tile in the sidebar | Rename…, Delete… (disabled for the last workspace) |
| A drawing (with the Select tool) | Delete |

- Right-clicking a block focuses it first, so Copy and Paste act on that terminal.
- The menu closes on a choice, Esc, or a click anywhere else. The keyboard goes back where it was.
- Right-clicking again, anywhere, replaces the open menu.
- **Send to ▸** types a note (or its selection), a card (title, then description) or an editor's selection (or
  its file and the cursor's line, `/path/file.rs:12`) into the chosen terminal, never pressing Enter: as a
  bracketed paste when the program asked for one, else on one line. The terminal is then shown with the keyboard.

## Architecture

| File | Change |
|---|---|
| `menu.rs` (new) | `Target` (what was right-clicked), `Workspace::open_menu`, each target's items, and drawing the open menu |
| `workspace.rs` | The open menu; the canvas's right-click; `add_terminal_at` and `add_kanban_at` |
| `block.rs`, `kanban.rs`, `sidebar.rs` | Each right-clickable element opens its target's menu and stops the event, so the innermost menu wins |

The menu itself is gpui-kit's `PopupMenu`, drawn the way its `context_menu` draws it.

**Adding to a menu later:** a new entry for something that already has a menu is one more item in its arm of
`Items::build`. A new block kind adds its item to the canvas's **Add ▸** submenu, and a new right-clickable thing
gets a `Target` variant, its `Items` arm, and a right-click handler that calls `open_menu` and stops the event.

## Testing

**Headless UI:**

- right-clicking the empty canvas, then **Add ▸ Terminal** or **Add ▸ Kanban board**, adds that block with its
  top-left corner at the click;
- right-clicking a card opens the card's menu, not its block's;
- a terminal's menu closes it;
- a second right-click replaces the first menu.
