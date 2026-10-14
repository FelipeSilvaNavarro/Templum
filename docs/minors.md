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

- [ ] **Dragging a selected block's title moves only that block.** `begin_move`
  (`crates/templum/src/workspace/input.rs`) moves one block, and the click focuses it, which drops the selection.
  Fix: when the pressed block is selected, a `Gesture` that moves every selected block by the same offset, snapping
  the group's bounds.
- [ ] **Delete on a focused note loses its text for good.** A note shown rendered, a kanban block or a Git block
  closes on Delete or Backspace (`delete_blocks`, `crates/templum/src/workspace/selection.rs`) as its close button
  does, with no undo. Fix: keep the closed blocks' `session::Item`s and bring them back with Ctrl+Z, as
  `paste_blocks` does.
- [ ] **A pasted terminal starts a plain shell.** Copy saves a terminal's folder without its agent (`Body::item`,
  `crates/templum/src/workspace/mod.rs`), so a copied Claude Code terminal pastes as a shell in that folder. Fix: copy
  `seen.agent` and type the agent's command (not its resume command) into the pasted shell.
