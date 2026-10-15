# Minor issues to fix later

Small issues found by the final reviews of the Git block's second version (`docs/specs/git.md`), batches 1 and 2,
and left for later on purpose: none loses work or breaks a main path. Each line says where it is, what a person sees,
and a way to fix it. Every minor left for later is added here; when one is fixed (with a test that fails without it,
AGENTS.md), its entry is deleted in the same commit.

Line numbers are those of `development` at a5fed2d (2026-10-13).

## Diff panel

- [ ] **An empty new file gets an empty "Line 1" hunk row.** `new_file_diff` makes `@@ -0,0 +1,0 @@` for an empty
  file, drawn as a hunk with no lines. Fix: no hunk for an empty file; a note row "Empty file".
- [ ] **An untracked nested repository shows an empty name.** `status -uall` lists it as `dir/`, and the file row's
  name (the part after the last `/`) is empty. Fix: trim the trailing `/` for the name, and say "Repository" in a note.
- [ ] **A binary's size is the working copy's in every section.** A deleted binary says "Binary file · 0 B"; a staged
  binary shows the working file's size, not the staged one. Fix: say "Binary file" without a size when the file is
  gone, and read the index blob's size (`git cat-file -s :path`) for the Staged section.
- [ ] **A drag released outside the panel stays on.** `dragging` is cleared only by the panel's mouse-up
  (`crates/templum/src/workspace/git_changes.rs:513`), which fires only over the panel; a later drag over line rows
  (button held from elsewhere) moves the old selection's head. Fix: `on_mouse_up_out` on the panel, or clear
  `dragging` on any mouse-up in the workspace.
- [ ] **The diff keeps its colors after a theme switch.** The highlight cache is cleared only when the diff changes
  (`crates/templum/src/git_block/diff_view/mod.rs:137`); a light/dark switch keeps the old syntax colors until then.
  Fix: clear every Git block's `panel.colors` where the theme is applied.
- [ ] **Any change re-reads everything: the selection and every hunk's colors go.** `DiffPanel::set`
  (`crates/templum/src/git_block/diff_view/mod.rs:137-138`) clears both whenever any file changed, so while an agent
  is writing, a selection vanishes every 2 seconds. Fix: key the colors by (section, path, hunk header) instead of
  indices, and keep a selection whose rows still hold the same lines.
- [ ] **A blame's panel shows Ignore whitespace, which changes nothing.** The header keeps the button for every
  source (`crates/templum/src/git_block/diff_view/mod.rs`, `panel`); with a blame shown, `reload_shown_diff` has no
  commit to read again, so the button lights up and the lines stay. Fix: hide it for `Source::Blame`, or read the
  blame again with `git blame -w`.
- [ ] **In Split, a press on an empty right cell selects on the left.** `DiffPanel::side_at`
  (`crates/templum/src/git_block/diff_view/mod.rs`) reads the side from the right cell as painted; an empty cell has
  none, so the selection starts at the end of the left text and a drag selects left lines. Fix: decide the side from
  the row's origin and the half's width, not from a painted cell.

## Hunks and the keyboard

- [ ] **A refused hunk offers a useless Run in a terminal.** `apply_hunk`'s failure goes through `notify_git_failure`
  (`crates/templum/src/workspace/git_changes.rs:603`), whose Run in a terminal types `git apply … /tmp/templum-….patch`,
  a file already deleted. Fix: a plain error notification for a refused patch (as Undo commit's refusal does).
- [ ] **The block's keys swallow a custom Alt or Shift shortcut.** `git_key` passes on only Ctrl, Cmd and Fn
  (`crates/templum/src/workspace/git_keys.rs:42`); a `[keys]` binding like `previous_prompt = "alt-up"` is taken by
  the block while it has the keyboard. Every default shortcut uses Ctrl/Cmd, so only custom ones collide. Fix: return
  early when `self.keymap.command(&e.keystroke)` is `Some`.
- [ ] **Holding `s` repeats the stage of a stale patch.** The keys ignore `e.is_held` and `acting > 0`
  (`crates/templum/src/workspace/git_keys.rs:34`), so a held key queues the same patch again and again, and each
  refusal leaves a sticky "The file changed; try again". Fix: ignore held keys, and do nothing while `acting > 0`
  (the buttons already hide then).
- [ ] **With Ignore whitespace on, `s` stages the cursor hunk's whole file without asking.** The hunk can't go alone
  then (`crates/templum/src/workspace/git_keys.rs:184`), and `s` falls back to the file silently (`d` asks). Fix: say
  so in a notification, or ask.
- [ ] **A focused Git block moved off screen keeps the keyboard until a click.** GPUI sends keys to the window's root
  when the focused element isn't drawn; `release_focus` runs only when a block is closed, a note's preview toggles or
  an editor's file changes (`crates/templum/src/workspace/blocks.rs`), not when a pan or zoom takes the block off
  screen. Editors and notes behave the same. Fix: release the focus of a block that leaves the viewport, for every
  kind.

## Commit

- [ ] **Commit & Push offers no Undo commit, even when the push fails or Publish is cancelled.** The Undo button is
  left out when `push` is set (`crates/templum/src/workspace/git_changes.rs:74`). Undo already checks whether the
  commit reached the upstream, so attaching it every time is safe. Fix: attach Undo commit in both cases.
- [ ] **Commit all changes adds an untracked nested repository as a gitlink.** `stage_all` runs `git add --all`
  (`crates/templum-git/src/repo.rs:407`), which adds `nested/` as an embedded repository with a warning nobody sees,
  and the label counts it as one file. Fix: leave out untracked entries ending in `/` (and say so), or ask.

## Branches, Sync and conflicts

From batches 3 and 4 (line numbers of `development` at 5046612).

- [ ] **A comparison's "⋯ 40 unchanged lines" does nothing when clicked.** `fold_row`
  (`crates/templum/src/git_block/diff_view/paint.rs:460`) is inert on a read-only panel: Compare's diff is read once
  with git's 3 lines of context. Fix: read that file of the range again with `-U` and its length (as `show_whole`
  does for the working changes).
- [ ] **Changes left on a detached HEAD don't come back by themselves.** The switch question names the stash after
  the short hash (`templum: left on a1b2c3d`, `crates/templum/src/workspace/git_switch.rs:68`) and `Repo::switch`
  pops only a stash named after the branch it goes to (`crates/templum-git/src/repo.rs:1152`), so checking that
  commit out again leaves them in Stashes. Fix: pop by the commit on a detached checkout too, or offer Leave them
  only on a branch.
- [ ] **The default branch is read again after every action and tab change.** `refresh_lists_at` queues
  `load_default` each time (`crates/templum/src/workspace/git_ops.rs:254`): a few `rev-parse`s more per action.
  Fix: read it when the block opens and after a fetch, a sync or a branch change only.
- [ ] **A remote or branch name with an unusual character is read wrong.** `words::local_name`
  (`crates/templum/src/git_block/words.rs:91`) and `Repo::switch` take what follows the first `/` as the local name
  (a remote named `team/x` breaks Back to and Compare with), and `unmerged_count`'s `--exclude`
  (`crates/templum-git/src/repo.rs:1023`) reads `*?[` in a branch name as a pattern. Fix: match the remote's name
  against `git remote`; escape the glob characters.

- [ ] **A switch that fails after leaving the changes may keep them stashed without saying so.** When the checkout
  fails partway, putting them back is `let _ = self.pop_whole(…)` (`crates/templum-git/src/repo.rs:1170`, at
  340439a): if that pop conflicts with what the checkout left, the changes stay in Stashes and the notification
  shows only the switch's error. Fix: return the pop's failure with the switch's, and say "Your changes are in
  Stashes".
- [ ] **A region choice checks only its two parts.** `resolve_region`
  (`crates/templum/src/workspace/git_conflicts.rs:79`, at 340439a) compares the parts' lines with what was drawn, not
  the `<<<<<<<` and `>>>>>>>` lines: a region whose labels changed (another merge started meanwhile with the same
  text) is still resolved. Fix: carry the labels in `shown` and compare them too.
- [ ] **A comparison's binary file reads "Binary file · 0 B".** The size comes from the working file
  (`crates/templum/src/git_block/diff_view/paint.rs:515`, at 340439a), which a range doesn't read. Fix: leave the size
  out when `bytes` is None.

- [ ] **Publish is dropped without a word after a workspace switch.** Its remotes read waits in the block's queue;
  when it comes back with another workspace on screen, `publish`'s early `return`
  (`crates/templum/src/workspace/git_sync.rs:174`, at 150c656) drops it. Fix: let `publish_to` and `git_sync` run by
  `GitAt` off screen, or say it was dropped.
- [ ] **A conflict choice with no trash goes ahead without asking.** Without a data folder, or past the trash's cap,
  `resolve_region` and `resolve_file` (`crates/templum/src/workspace/git_conflicts.rs:88,141`, at 150c656) write
  the file with no copy and no question, where the trash rule (docs/specs/git.md, "When a copy can't be kept") asks
  first. Fix: ask as discards do ("This can't be undone").

## History

- [ ] **Load more in a file's history reads the earlier pages again.** `--skip` with `--follow` counts commits that
  don't touch the file (git 2.55), so `Repo::file_log` (`crates/templum-git/src/repo.rs`) reads the first
  `skip + count` entries and drops `skip` of them. A file with thousands of commits makes each Load more slower.
  Fix: remember where the last page ended (its commit and the path then) and go on from there with
  `git log --follow <hash>^ -- <path then>`.
- [ ] **File history and Blame are offered where git can't answer.** `menu/build.rs` leaves them out only for an
  untracked row: a staged new file (no history, "no such path in HEAD" for a blame), a file deleted in the working
  tree, and an untracked file in an editor's tree all end in an error under the top bar. Fix: leave the items out
  there, or blame those files at HEAD.
- [ ] **A Code search trims its text.** `run_search` (`crates/templum/src/workspace/git_history.rs`) trims the field,
  so `"fn "` searches `-Sfn`. Fix: trim only to decide whether the search is empty.
- [ ] **Two search-state slips.** `set_search_mode` runs the last searched text again, not the field's; and typing a
  character then deleting it (no search on) reads page 1 again, losing the pages Load more added. Fix: run with the
  field's text; skip a clear when nothing was searched.
- [ ] **In a file's history, a commit's other files look clickable and do nothing.** The panel holds that file's
  change alone, so `show_commit_file` finds no row for the others. Fix: dim them, or open that commit whole.
- [ ] **Restore this version… takes the old path's attributes.** `Repo::file_at` (`crates/templum-git/src/repo.rs`)
  runs `cat-file --filters <rev>:<then>`, so a rename that changed the extension gets the old name's line-ending
  rules. Fix: `--path=<now>`.
- [ ] **Restore this version… over a symbolic link, or a file past the trash's cap.** `restore_now` writes through
  a link at the path (the trash keeps only the link), and over 100 MB keeps no copy though the question said the file
  goes to Recently discarded. Fix: refuse a link; word the question from `trash::fits`, as discards do.
- [ ] **A restored file that was gone comes back without its executable bit.** `restore_now` writes plain bytes.
  Fix: read the mode from `git ls-tree` and set it.
- [ ] **History goes back to its first page after every action.** `refresh_lists_at` reads page 1 again (older than
  batch 5), so the pages Load more added go after a commit, a fetch or a switch. Fix: read as many commits as are
  loaded.

## Stashes

- [ ] **The stash shown keeps its index after a drop.** `stash_shown` is `stash@{N}`'s N (older than batch 5): after a
  drop in a terminal, N names the next stash while the panel still shows the old one. Fix: keep the stash's hash.
- [ ] **A stash's diff ignores `diff.srcPrefix` and `diff.dstPrefix`.** `Repo::stash_diff` resets `diff.noprefix` and
  `diff.mnemonicPrefix` (git 2.55 mangles `--src-prefix` with `--include-untracked`) but not the two prefix settings of
  git 2.45+. Fix: `-c diff.srcPrefix=a/ -c diff.dstPrefix=b/`, checked against the same mangling.

## Trash and discards

- [ ] **Discard all filters its files with `Vec::contains`.** `discard` (`crates/templum/src/workspace/git_discard.rs:69`)
  is O(n·m) on the UI thread, slow for thousands of files. Fix: a `HashSet` of the paths.
- [ ] **Two trash corner cases.** The counter in a discard's id isn't zero-padded
  (`crates/templum-git/src/trash.rs:101`), so two discards in the same millisecond past the 9th can sort wrong; and an
  entry left half-made by a crash has no manifest, so `list` skips it and `prune` never deletes it
  (`crates/templum-git/src/trash.rs:165`, `:239`). Fix: pad the counter (`{:06}`), and prune folders without a
  manifest older than a day.
- [ ] **Recently discarded doesn't name the files.** A row says "Discarded 7 files"
  (`crates/templum/src/git_block/discarded.rs:51`). Fix: the file names under it, folded past a few.

- [ ] **Two answers from batches 1 and 2 act on the bare block id in whichever workspace is shown.** Undo commit's
  "Revert {short}?" (`crates/templum/src/workspace/git_changes.rs:240`) and Restore's "changed since" question
  (`crates/templum/src/workspace/git_discard.rs:302`, at 150c656) call `git_action`/`restore_discarded` with
  `at.block`: answered after a switch, they act on the other workspace's block with that id. Fix: through
  `show_git_block(&at, …)`, as the batches 3 and 4 questions do.

## Tests

- [ ] **Three tests don't check the words they are about.** `discard_all_with_a_filter_on_takes_only_the_rows_shown`
  (`crates/templum/src/workspace/tests/git_layout.rs`) doesn't check the count in the question;
  `a_hunk_that_changed_since_it_was_read_is_refused` (`tests/git_hunks.rs`) checks the notification count, not "The
  file changed; try again"; `past_the_cap_a_discard_keeps_nothing` (`tests/git_trash.rs`) doesn't check "Too much to
  keep a copy". The batches 3 and 4 tests do the same: `without_a_remote_publish_says_so`,
  `a_failed_clone_says_why_and_leaves_no_folder` (`tests/git_sync.rs`), `leaving_a_branch_with_commits_not_pushed_offers_to_push_it`
  (`tests/git_switch.rs`), `a_region_changed_since_it_was_read_is_refused` and
  `a_whole_file_choice_whose_copy_fails_changes_nothing` (`tests/git_conflicts.rs`) count notifications. Fix: read
  the dialog's description and the notification's message in each (gpui-component's `Notification` keeps its message
  private: a test-only accessor, or an element id on the message).
- [ ] **A test that writes a script and runs it can fail once with "Text file busy".** Another test's thread may
  fork while the script is still open for writing, and Linux then refuses to run it (ETXTBSY):
  `a_cancelled_command_is_killed_and_says_so` (`crates/templum-git/tests/repo.rs`) did once in a full run, and the
  fake agent of `crates/templum/src/workspace/tests/git_message.rs` is written the same way. Fix: have `sh` read
  the script (`sh <path> …`) instead of running the file itself.
- [ ] **`an_agent_past_its_timeout_is_stopped` counts the notifications, not what they say.**
  (`crates/templum/src/workspace/tests/git_message.rs`.) Fix: check it is "didn't answer within".

## Release and benches

- [ ] **The CHANGELOG will list three features twice.** Batch 2 has a `feat` commit for the library and another for
  the UI of the same feature: 726079b with c7e13c6 (hunks), 1d5bc9a with cb26cfd (the trash), efdd911 with 6362120
  (commit all, undo commit). Fix: edit the generated section at the next release so each feature reads once.
  Batch 6's `fix(git)` commit after its review fixes Generate ✨ before any release: drop it from the section too.
- [ ] **`--bench-zoom=git` measures an empty panel.** It runs from `~/Documents/github/DaemonQuest`, whose tree is
  usually clean, so the Git blocks it zooms show "Nothing changed since the last commit". `--bench-git` covers a full
  diff; for the zoom, leave a few files changed in that folder before running it, or note the state with the result.

## Commit messages

- [ ] **An agent installed while Templum runs gets no ✨ until the config changes.** Whether its program is
  installed is read when the config changes (`Workspace::commit_agent`, `crates/templum/src/workspace/mod.rs`),
  not each frame. Fix: look again when the settings panel opens, or on the Git block's focus.
- [ ] **On Windows, Details may say "exit code 1" for any failing agent.** PowerShell's `-Command` reports 1, not
  the program's code, and a native program's standard input through it is unchecked (`login_shell`,
  `crates/templum-git/src/message.rs`). Fix: append `; exit $LASTEXITCODE`, and test Claude Code's preset on
  Windows.
- [ ] **A cancelled or timed-out agent may run on under fish or PowerShell.** They keep the command as a child,
  and only the shell is killed (`until_cancelled`, `crates/templum-git/src/repo.rs`); bash and zsh `exec` it. The
  agent then finishes on its own, using the person's account. Fix: kill the process group (needs a dependency or
  `unsafe`).
- [ ] **A login profile that prints something comes before Claude Code's answer.** The command runs through
  `$SHELL -l -c`, and Claude Code's preset answers on the standard output, where a profile's greeting lands too.
  Fix: drop what comes before the answer's first line when it matches the profile's output read once, or ask
  Claude Code for a file (it has no such flag today).
- [ ] **A Git block closed or pointed elsewhere while Generate runs leaves its agent running to the timeout.** The
  answer is dropped (`generated`, `crates/templum/src/workspace/git_message.rs`), the agent isn't stopped. Fix:
  set `generating` when the block closes and in `set_git_folder`.
- [ ] **Clearing a repository's instructions leaves an empty `[git.repositories."…"]` line.** `remove_key` takes
  the key, not its table (`set_repo_instructions`). Fix: remove a table in `save_edits` when its last key goes.
- [ ] **A nested table's last key removed may move its comments too far.** `remove_key` looks for the next header
  among the top-level tables only (`crates/templum/src/config.rs`). Fix: walk every table for the next header.
- [ ] **Generate's answer can't be undone in the fields.** It replaces what they held at the click with
  `set_value`, which clears their undo history (`generated`, `crates/templum/src/workspace/git_message.rs`): a draft
  typed before clicking ✨ is gone. Fix: `replace_all`, which keeps it, on both fields.
- [ ] **With Amend, Generate describes only what is newly staged.** The request is the staged diff
  (`message::changes`), and the answer then replaces the whole amended commit's message. Fix: with Amend, diff the
  index against `HEAD~1` (the empty tree for the first commit), or hide ✨ while Amend is on.
- [ ] **Emptying Settings ▸ Git ▸ Prompt puts the default back at once.** Empty text removes the key and the next
  frame fills the area with `PROMPT`, the caret at its start (`set_commit_prompt`, `crates/templum/src/settings/git.rs`);
  select all and type still works. Fix: keep a draft while the area is empty, as Another model does.
- [ ] **A new file's diff for the request is built twice.** `message::changes` reads untracked files and builds
  their diff as `untracked_diff` (`crates/templum-git/src/repo.rs`) does. Fix: one helper both call.
- [ ] **The docs don't say all of it.** `docs/specs/agents.md` doesn't list `[agents]`'s `generate` and `models`, and
  the README doesn't say Codex's models come from its `~/.codex/models_cache.json` (else one is typed). Fix: a line
  in each.

## Far zoom (level of detail)

- [ ] **A soft-wrapped editor tab's bars keep the wrap of the zoom they were taken at.** An editor tab's wrap width
  keeps gpui-kit's fixed padding and line-number margin (`crates/templum/src/editor.rs`, `code` in `files`), so with
  Soft wrap on its words wrap a little differently at each zoom; far away its bars show the rows of its last Full
  frame, and back at 50% a word or two can move to the next row. Notes don't: their box makes up for those parts
  (`note.rs`, `editor_box`). Fix: give the editor tab's area the note's compensation (`FIXED_LEFT`, `FIXED_RIGHT`).
- [ ] **The wheel on the one unseen frame scrolls a far editor or note instead of the canvas.** When a far body has no
  word map, or a stale one (first drawn far away, resized, its file reloaded), it lays its editor out once at opacity
  0 (`crates/templum/src/silhouette.rs`, `body`), with its hitboxes: a wheel event on that frame scrolls the hidden
  text (gpui-base's editor takes the wheel and stops it) instead of moving the canvas, and the text shows scrolled
  when the block is next drawn in full. Fix: a capture-phase wheel listener on the unseen editor's wrapper that moves
  the canvas and stops the event before the editor sees it.

## Selecting blocks

- [ ] **A pasted terminal starts a plain shell.** Paste drops a copied terminal's agent (`paste_blocks`,
  `crates/templum/src/workspace/selection.rs`): resuming it would put two agents on one conversation, so a copied
  Claude Code terminal pastes as a shell in its folder. Fix: type the agent's own command (not its resume command)
  into the pasted shell.

## Notes

- [ ] **Undoing a blank note's close brings back an empty block.** A blank note goes when its block closes
  (`drop_if_blank`, `crates/templum/src/workspace/note_tabs.rs`), so Ctrl+Z after closing it, or Ctrl+Shift+Z after
  undoing a new note block, shows "No note open here". Fix: in `add_saved`, a note item whose notes are all gone opens
  a new blank note.
- [ ] **Two canvases naming one note both show it after a restart.** `note_open_at` looks in the workspaces already
  loaded (`self.spaces()`), but `load_spaces` keeps the ones it restored in a local list
  (`crates/templum/src/workspace/spaces.rs`), so a hand-edited file that names a note in two workspaces opens it twice,
  and the last tab typed in wins. Fix: look in that list too while loading.
- [ ] **A note block copied in another Templum pastes empty.** The clipboard names notes by their ids
  (`Body::item`, `crates/templum/src/workspace/mod.rs`), which another Templum's notes don't have. Fix: put each
  note's text in the copied item's `text`, and let `paste_blocks` make a note of it when the id isn't known.

## Grid mode

- [ ] **Ctrl+Z in grid mode can take back a hidden block's change.** The canvas's undo steps (`workspace/history.rs`)
  also name blocks left on the canvas, which grid mode doesn't show, so Ctrl+Z may move one there and nothing visible
  happens. Fix: skip steps whose blocks are all hidden while in grid mode, keeping them for the canvas.
- [ ] **A tile smaller than its block's minimum cuts it.** Splitting never stops (`Tile::insert`,
  `crates/templum-canvas/src/tile.rs`), so with many tiles in a small window a terminal stays at its 20 × 4 cells and
  is cut off, a kanban or Git block's body too. Fix: refuse a split that would make a tile smaller than its block's
  `min_size`, and say so.
- [ ] **Swapping and flipping tiles aren't Ctrl+Z steps.** `swap_tile`, the title drop and `flip_tile`
  (`crates/templum/src/workspace/tiling.rs`) change the tree without a history step. Fix: a `Change::Tiles` step
  holding the tree before.
- [ ] **Grid mode comes and goes without a glide, and a last closed tile vanishes at once.** `calm` clears the ghosts
  (`tiling.rs`). Fix: animate each block from its canvas rect to its tile and back, as camera flights do.
- [ ] **The title bar shows 100% in grid mode.** It reads the camera's zoom (`title_bar.rs`), which is grid mode's.
  Fix: show "Grid" there while a workspace is tiled.
- [ ] **A pasted group comes into the grid one block at a time.** Each pasted block splits the one before
  (`added`, `crates/templum/src/workspace/blocks.rs`), so their arrangement on the canvas is lost. Fix: tile a
  pasted group as a dwindle of its own in the focused tile's half.

## Agent usage

- [ ] **A user's own status line on Windows runs under `cmd`.** The relay (`run`,
  `crates/templum-usage/src/claude.rs`) runs it with `cmd /C`; Claude Code may run status lines with Git Bash there,
  so a bash one prints an error instead of the user's line. Fix: check by hand which shell Claude Code uses on
  Windows and run the same.
- [ ] **A status line that refreshes on a timer refreshes less.** The settings file Templum writes
  (`settings_arg`, `claude.rs`) has no `refreshInterval`, so a user's status line that showed a clock updates only
  after each answer. Fix: copy the user's `refreshInterval` into it.
- [ ] **A wrapper as Claude Code's command still gets `--settings`.** `typed` (`crates/templum/src/workspace/usage.rs`)
  adds it to any command of the built-in Claude Code, so `claude; echo done` passes it to `echo`. Fix: add it only
  when the command's program is `claude`.
- [ ] **Two Codex resumed in one folder may swap sessions, outside Linux.** Without `/proc` (`files`,
  `crates/templum-usage/src/codex.rs`), a resumed session is matched by its folder and its file's last write, and a
  resumed one older than the scanned days isn't found: its meter shows the other's numbers, and on restore each block
  may get the other's conversation (or the folder's latest). Fix: ask the OS for the process's open files on macOS
  and Windows too.
- [ ] **A Codex session from another folder asks for a folder on restore.** `codex resume <id>` (`resume_id`,
  `crates/templum-terminal/src/agent.rs`) is typed in the folder the Codex process ran in; a session Codex started
  with `-C <dir>`, or one resumed from elsewhere, has its own folder, and Codex then shows its "Choose working
  directory" picker. Fix: save the session's `cwd` from its rollout and type `codex resume -C <cwd> <id>` when it
  differs, quoted as `typed_path` does.
- [ ] **Gemini, Grok and Pi come back on the session they were opened on.** Their id is read from the process's
  arguments (`session_in`, `crates/templum-terminal/src/agent.rs`); after `/clear`, `/new` or `/resume` inside the
  agent, the block still resumes the first one. Fix: read the current session from their files (Gemini's newest
  `chats/session-*.jsonl` for the folder, Grok's `~/.grok/sessions/<cwd>/`, Pi's `~/.pi/agent/sessions/--<cwd>--/`)
  and pick the one written since the process started.
- [ ] **A Gemini killed before its first message can't come back.** Killed with SIGKILL, Gemini leaves its empty
  session file; `session_kept` (`crates/templum-usage/src/lib.rs`) finds it, so restore types `gemini --resume <id>`,
  which Gemini refuses ("Invalid session identifier") and the block shows a shell. Fix: count the file's message lines
  too.
- [ ] **Codex's context reads higher than Codex's own.** `usage` (`codex.rs`) divides the last turn's input by the
  whole window; Codex leaves out a baseline it reserves. Fix: use Codex's formula once its source says it.
- [ ] **Limits of a session closed long ago still show.** The meter (`crates/templum/src/usage.rs`) shows a window
  until its reset time, even when the last session that read it closed hours ago and the user may have used it
  elsewhere. Fix: say "as of" with the read's time in the popover.

## Programs in a terminal

- [ ] **Programs never hear the focus come and go.** Claude Code and Codex turn on focus reporting (mode 1004), but
  no block sends `CSI I` / `CSI O` (`focus_block`, `crates/templum/src/workspace/blocks.rs`), so a program that
  waits for focus to redraw or to stop notifying doesn't know. Fix: send them to the terminal losing and the one
  getting the focus when its mode asks.
- [ ] **The kitty keyboard reports presses only.** `kitty_key` (`crates/templum-terminal/src/input.rs`) never sends
  repeat or release events nor a modifier key alone, and in every-key mode the arrows and F-keys keep their legacy
  bytes; a program that asked for them (a game, a key-chord editor) misses them. Fix: pass `is_held` and the
  workspace's key-up and modifiers-changed events to `Terminal::key`.
- [ ] **A program's drag loses Ctrl and Alt.** `drag_to` (`crates/templum/src/workspace/input.rs`) reports a held
  button's moves with no modifiers, since it only gets Shift. Fix: pass the move's `Modifiers` to `drag_to`.
- [ ] **The pointer stays a text cursor over a program that uses the mouse.** The body's `cursor_text`
  (`crates/templum/src/block.rs`) doesn't look at the mode, so htop's or Claude Code's buttons look like text to
  select. Fix: show the arrow when `modes().mouse`, read in the snapshot rather than under the lock while painting.

## Updates

- [ ] **Templum doesn't open again after an MSI update.** `Ready::Installer` for the MSI
  (`crates/templum-update/src/apply.rs`) runs `msiexec /i <msi> /passive`, which doesn't start the app it installed;
  the person opens it from the Start menu. Fix: run msiexec through a PowerShell `Start-Process -Wait`, then start
  `C:\Program Files\Templum\templum.exe`.
- [ ] **macOS updates aren't checked by hand yet.** `copy_app` and the bundle swap (`apply.rs`) are tested on CI's
  macOS (a dmg made with `hdiutil create`), never on a real Mac with Templum in /Applications. An ad-hoc-signed app is
  a new app to macOS at each version, so permissions like Full Disk Access are asked again. Fix: update a real
  install, and sign the app with a Developer ID.
- [ ] **No prereleases.** `latest` (`crates/templum-update/src/fetch.rs`) asks for `releases/latest`, which skips
  `-rc` tags, so nobody can test a release candidate through the updater. Fix: a `[updates] channel = "beta"` that
  lists releases and takes the newest one, prereleases included.
- [ ] **A proxy with its own certificate authority fails.** ureq trusts webpki-roots (Mozilla's list), not the
  system's, so a company proxy that re-signs HTTPS makes every check fail with a certificate error. Fix: ureq's
  `platform-verifier` feature (`RootCerts::PlatformVerifier` in `agent`).
- [ ] **A stalled download waits up to 30 minutes.** `agent` (`fetch.rs`) has a limit for the whole download, not
  for a pause in it, so a connection that stops sending keeps the pill at its percent until then. Fix: read the body
  through a reader that fails after 60 s without bytes.
- [ ] **Nothing is signed by Templum.** A download is trusted because GitHub serves it over HTTPS with the SHA-256
  it computed (`download`, `fetch.rs`); someone who could publish a release could publish a bad one. Fix: sign the
  files in `release.yml` (minisign) and check the signature with a key built into Templum.
- [ ] **A failed swap is only written to stderr.** `finish_update` (`crates/templum/src/workspace/update.rs`) prints
  why the new file couldn't take the old one's place (a folder made read-only after the download); Templum then
  opens the old version, which downloads again and offers Restart again. Fix: keep the error in the data folder and
  tell it at the next start.
- [ ] **The check's steps aren't tested end to end.** `check_update` (`workspace/update.rs`) calls GitHub directly,
  so the headless tests set the states by hand and never go Checking → Downloading → Ready, nor the offline case
  that keeps "available". Fix: hand `check_update` the release source, a local server in tests.

## Workspaces

- [ ] **A note open in a stopped workspace can be opened elsewhere, and then leaves the stopped one.**
  `note_open_at` (`crates/templum/src/workspace/note_tabs.rs`) looks at live blocks only, so a stopped workspace's
  note tabs (in its saved canvas) don't count as open; opening one elsewhere, then starting the stopped workspace,
  drops that tab there (`add_saved`, `persist.rs`, keeps a note in one tab at most). Fix: look in a stopped space's
  `saved` items too, and show "Open in *name* (stopped)".
- [ ] **Stop doesn't reach programs inside WSL.** `tree` (`crates/templum-terminal/src/terminal.rs`) walks Windows
  processes below the shell; what runs in a WSL distribution under `wsl.exe` isn't one of them, so a server started
  there may outlive the hangup. Fix: run `wsl.exe --terminate` for a distribution only that workspace's terminals
  use, or kill the session through `wsl.exe -e pkill -s`.
- [ ] **Stop and Delete may stutter for a moment with many processes.** `tree_of`
  (`crates/templum/src/workspace/spaces.rs`) reads the whole process table on the UI thread (and on Unix asks each
  process its session), once per Stop or Delete. Fix: read the tree on the background executor, then drop the
  workspace's blocks when it is read.
- [ ] **A program started after Stop by one that ignores the hangup keeps running.** Stop reads the process tree
  once (`tree_of`, `crates/templum/src/workspace/spaces.rs`) and two seconds later kills only what it read; a
  program that ignores the hangup and starts another meanwhile leaves that one alive. Fix: at the kill, read the
  tree again and also kill what is still in the stopped shells' sessions (Unix).

## Welcome

- [ ] **The frosted canvas shows the blocks' shapes, not their text.** GPUI's scene has no backdrop filter, so
  `frost` (`crates/templum/src/welcome.rs`) draws each block as a blurred shadow of its shape. Fix: blur the real
  canvas when GPUI gains a backdrop blur, or draws a frame to a texture outside tests.

## Themes

- [ ] **The color picker shows an alpha slider that does nothing.** gpui-kit's popover always has it, and theme
  colors are opaque (`crates/templum/src/settings/colors.rs`). Fix: hide it when gpui-kit's `ColorPicker` gains an
  option for that, or draw our own sliders.
- [ ] **Switching light and dark may show the other theme as it was for half a second.** When its file was saved
  while it wasn't in use, `refresh_theme` (`crates/templum/src/workspace/preferences.rs`) applies the copy read
  before, and `poll_theme` applies the file at the next poll. Fix: read the picked theme's file in `refresh_theme`.
- [ ] **The gallery's cache keeps themes GitHub no longer lists.** `refresh` (`crates/templum/src/gallery.rs`) only
  adds files to `<data folder>/theme-gallery/`; a theme changed upstream leaves its old file there (a few hundred
  bytes each). Fix: after a refresh, delete the `.toml` files `index.json` doesn't name.

## GPUI engine (gpui-fast)

- [ ] **A note tab's text is saved without telling the window.** The editor subscription in `note_tab`
  (`crates/templum/src/workspace/note_tabs.rs`) writes `ws.notes` without `cx.notify()`. Today the tab still follows
  typing because `Workspace`'s render reads the note's editor; if that read goes away, gpui-fast reuses the old frame
  and the tab looks stale until something else redraws. Fix: `cx.notify()` after `set_text`.
- [ ] **Zooming a note is 5% slower on gpui-fast.** `--bench-zoom=note` p95 51.6 → 54.2 ms (ADR-0003); a person sees
  the same stutter as before, slightly longer. Fix: the note re-wrap work already owed (hold the text size while the
  zoom glides, or the upstream gpui-kit fix), which removes the cost under both engines.
- [ ] **The diff list never gets gpui-fast's scroll cache.** `top_file`
  (`crates/templum/src/git_block/diff_view/mod.rs:327`) reads the list's scroll offset while `Workspace` renders, and
  gpui-fast repaints every scrolled frame of a list whose offset `render` read. A person sees nothing; scrolling a long
  diff costs what it did before. Fix: pick the sticky header's file in a `canvas` prepaint instead of in `render`.

## Connections

Left by batch 1 (the bus) of `docs/specs/connections.md`; line numbers are those of `development` at 2c3c809.

- [ ] **An agent that can't be given connections says nothing.** `typed` (`crates/templum/src/workspace/usage.rs:33`)
  leaves the flags out when the data folder or Templum's path can't be typed safely (a `$`, a `"`), and the Claude
  Code or Codex then starts without Templum's server: `/mcp` doesn't list it, and nothing says why. Fix: a
  notification once per session, from `launch_agent` and `resume_agent`, which have the window.
- [ ] **Windows terminals have no `templum` on their PATH.** `templum_first` (`crates/templum/src/workspace/bus.rs:78`)
  returns nothing on Windows, so `templum blocks` works there only when the installer put Templum on PATH. Fix: a
  `templum.exe` hard link in `<data>/bin`, or the installer's PATH.
- [ ] **Two Templums sharing a data folder share one `templum` link.** `templum_first`
  (`crates/templum/src/workspace/bus.rs:63`) points `<data>/bin/templum` at the last Templum started (an installed
  one and `cargo run`), so a terminal of the first runs the second's program; its `TEMPLUM_BUS` still names its own
  Templum. Fix: one link per bus (`<data>/bin/<pid>/templum`), removed with the bus.
- [ ] **The Claude Code settings file has its old name.** `claude-statusline.json`
  (`crates/templum-usage/src/claude.rs:23`) now holds the hooks too. A person reading `ps` sees a status-line file
  carry hooks. Fix: rename it `claude-settings.json` and delete the old one at start.
- [ ] **A stopped workspace's block ids aren't checked for a clash.** `new_uid`
  (`crates/templum/src/workspace/blocks.rs:68`) checks the running workspaces only; a new block may take an id a
  stopped workspace's saved canvas holds (1 in 60 million per pair), and that block gets a new id when the workspace
  starts again. Fix: check `Space::saved`'s items too.
- [ ] **Codex's dot never says "Working".** Codex only reports its turns' end (`codex`,
  `crates/templum-mcp/src/hooks.rs:49`): until its first turn ends its terminal shows the shell's "Running for …",
  and after it nothing, also while it works on the next prompt, and no yellow dot at a permission prompt. Fix: a
  Codex hook for those, when Codex has one; meanwhile the screen going still (batch 5).
- [ ] **The verbs print nothing in a Windows console.** The release program has no console
  (`crates/templum/src/main.rs:3`), so `templum blocks` (`main.rs:56`) prints only into a pipe:
  `templum blocks | more`. Fix: `AttachConsole(ATTACH_PARENT_PROCESS)` before running a verb (needs `unsafe` or a
  crate), or a small console `templum-cli.exe` beside it.
- [ ] **The bus spins when it can't accept.** The accept loop (`crates/templum-mcp/src/bus.rs:63`) goes straight
  back to `accept` after an error; one that repeats (out of file descriptors) keeps a core busy until it clears.
  A person sees Templum use a whole core. Fix: sleep 100 ms after a failed `accept`.
- [ ] **A socket in the temporary folder outlives a crash.** When the data folder's path is too long for a socket,
  the bus's socket goes to a private `templum-<random>/bus.sock` in the temporary folder
  (`crates/templum-mcp/src/bus.rs`, `listen`), and `remove_stale` only clears `<data>/bus`: a crash leaves that
  folder and its socket file there. Fix: at start, run `remove_stale` on the user's own `templum-*` folders in the
  temporary folder, and remove each one left empty.
- [ ] **No test says a pasted block gets a new id.** `copy_blocks` (`crates/templum/src/workspace/selection.rs:144`)
  copies items without their `uid`, so `paste_blocks` gives each a new one, but the spec's test list
  (docs/specs/connections.md, Testing) names a test for it that doesn't exist. Fix: copy a block, paste it, and
  assert the two ids differ.
