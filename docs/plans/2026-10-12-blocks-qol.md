# Blocks: quality of life (2026-10-12)

The maintainer asked for four changes and a review of every block; five read-only agents reviewed
the blocks (terminal, editor, Git, note, kanban), four more designed the features chosen. Each
feature is one or a few small commits on `development`, each with a test that fails without it.

## Done

- `f2d140d` Showing a block (Open in editor, a notification, Run in a terminal) moves the camera
  just enough to show it whole (`Camera::reveal`), zooming out when it can't fit.
- `e21c94d` The editor's file tree keeps its rows through a zoom (`editor::keep_list_rows`: a
  list's scroll is in pixels, so it scales with its rows, whole pixels).
- `b8b284c` Every edge and corner of a block resizes it (`templum_canvas::Edges`,
  `snap_resize` snaps the dragged edge; `block::resize_zone`).
- `c4aaa7d` Change folder… in the editor's and the Git block's menus (`launcher::ask_folder_for`).
  A Git block is rebuilt (`Git::new`) and its `generation` drops what commands started before
  return (`GitAt::generation`, checked once in `git_at`).
- `d20fef7` A Git block's commit message (summary, description) is saved with the canvas.
- `f487ab2` Send to ▸ a terminal from a note, an editor, a kanban card. `Pane::seen` holds each
  terminal's folder and agent as the 5-s snapshot read them (`Space::see`), plus the folder a
  finished command reported; `Workspace::terminal_list` labels them, agents first;
  `Terminal::type_in` never presses Enter (`typed_bytes`).

## To do, in order

### Git block shows its terminals
- "Open a terminal here" in the Git block's menu (`terminal_beside` + `reveal`), done in the tree.
- A pill in the Git top bar (`git_block/view.rs`, before the spacer) when
  `terminal_list(Some(root))` isn't empty: a terminal icon and the count; a click opens
  `Target::GitTerminals(block)` → `Items::Reveal(list)` → `terminal_items` that reveal.
- Test: right-click the title, Open a terminal here (index 2), the terminal is focused beside the
  block and its folder is the repo; the pill lists it, and choosing it focuses it.

### Terminal: jump to the one waiting
- `Command::GoToWaiting` ("go_to_waiting", ctrl-shift-a / cmd-shift-a; Ctrl+Shift+letter sends
  the same byte as Ctrl+letter, so the shell loses nothing). `reveal` the first terminal whose
  `alert` is `Alert::Attention`: this workspace first, then the others in sidebar order, lowest id
  first. Focusing clears the alert (`hear`), so the next press goes to the next one. None: a
  notification "No terminal is waiting for you".
- Test: alerts in two workspaces and a `Finished` one; three presses visit the two, then notify.

### Terminal: confirm before closing a busy one
- In `Workspace::close` (every close path goes there): when a terminal runs a command
  (`status.running`) or an agent (`pane.seen.agent`), `prompt::confirm` "Close the terminal?" with
  why (`alert::busy(status, agent title)`), Cancel / Close. A shell that exited
  (render.rs ~188) is removed without asking (`remove_block`).
- Quitting with busy terminals: not done (agents come back on restore); ask the maintainer.

### Terminal: Shift+Enter is a newline in an agent
- In `workspace/input.rs` (key → bytes), Enter with Shift sends ESC CR (as Alt+Enter does) when
  `pane.seen.agent.is_some()`: Claude Code's /terminal-setup binds Shift+Enter to ESC CR; ble.sh
  binds M-RET to newline, plain bash leaves it unbound. Check Codex by hand (Alt+Enter there).
- Test (unix): with `cat`, Shift+Enter echoes "^[" only with an agent.

### Terminal: find in the history
- `Command::Find` (ctrl-shift-f / cmd-f; Ctrl+F stays readline's). A field at the body's top
  right (`git_block::widgets::field` scaled by the zoom). Typing selects the nearest match above
  the bottom and scrolls there (alacritty `RegexSearch`, literal query escaped, smart case);
  Enter older, Shift+Enter newer, wrapping; Esc closes and gives the terminal back, the match
  stays selected. Visible matches tinted: `Snapshot.find`, `Row.found` spans (viewport only, only
  on frames where the terminal changed), one quad each in grid.rs.
- Key guard at the start of `on_key_down`: when the field has focus, Enter/Esc never reach the
  shell. Bench (`--bench`) in the PR: it touches painting.

### Terminal: Ctrl+click paths and links
- `templum-terminal/src/link.rs`: `SharedTerm::link_at(row, col) -> Option<Link>` reads the Term
  under the lock on hover/click only (never the snapshot: no hyperlinks, no WRAPLINE, no
  history). OSC 8 links first (skip `templum:`), else the run of non-blank cells around the
  point across wrapped lines, parsed by a small parser (no regex): URLs, paths with `:L`, `:L:C`,
  `(L,C)` suffixes. `resolve(path, cwd, home)` → an existing file only.
- Ctrl/Cmd+click: URL → `cx.open_url`; file → `open_in_editor(root, file, line, Some(term))`,
  root = nearest folder with a `.git` entry (`templum_git::root_of`, no git process), else the
  file's folder. Otherwise the click selects as before. With Ctrl held the link under the pointer
  is underlined (one quad per row) and the cursor is a hand.

### Note: preview zooms like a photo
- A `ScaledRem` element lays out and paints the TextView with `window.with_rem_size(rem × z)`
  (rems drive its spacing); `TextViewStyle { heading_base_font_size: 14 × z, code_block text
  13 × z }`; a fixed width in world units × z; its scroll kept through zooms. Shared by notes and
  the editor's Markdown preview.

### Note: Markdown keys
- On the note body only (not code editors): `capture_action` for `Enter` (continue `- `, `* `,
  `+ `, `N. `, `- [ ] `; an empty item ends the list; Ctrl+Enter toggles `[ ]`/`[x]`), `on_key_down`
  for Ctrl/Cmd+B and +I (wrap the selection, or insert the pair). Pure functions `next_item`,
  `toggle_task`, `emphasis`, unit-tested. Main cursor only.

### Note: color
- `Note::color: Option<board::Label>`, saved in `NoteItem` (unknown name → None); the card's
  background is the theme's background mixed with the label's ANSI color (`Theme::note_bg`,
  readable in every built-in theme: WCAG ≥ 4.5). Color ▸ submenu in the note's menu.

### Kanban: confirm and undo
- Delete card from its menu asks first. `Workspace::edit_boards(f)` keeps `Boards` before each
  change (at most 50); Ctrl+Z with a kanban block focused undoes (right-clicks on columns and
  cards focus their block first).

### Editor: Ctrl+P
- `Command::OpenFile` (ctrl-p / cmd-p), editor-only (`effective`), so terminals keep Ctrl+P.
  Files from `ignore::WalkBuilder` (already in the tree; respects .gitignore, skips .git, cap
  10 000), filtered by a pure `pick` (name substring, path substring, subsequence; shorter first),
  shown in gpui-component's command palette with `filterable(false)`; Enter opens.

### Also found
- The same pixel-scroll drift as the file tree is in the Git lists, the kanban columns and both
  Markdown previews; the preview one is fixed with the note preview.
