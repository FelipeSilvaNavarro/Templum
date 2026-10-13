# Minor issues to fix later

Small issues found by the final reviews of the Git block's second version (`docs/specs/git.md`), batches 1 and 2,
and left for later on purpose: none loses work or breaks a main path. Each line says where it is, what a person sees,
and a way to fix it. Every minor left for later is added here; when one is fixed (with a test that fails without it,
AGENTS.md), its entry is deleted in the same commit.

Line numbers are those of `development` at a5fed2d (2026-10-13).

## Diff panel

- [ ] **A pure deletion's hunk is titled "Line 0".** `Hunk::title` (`crates/templum-git/src/diff.rs:123`) uses the
  new side; a deleted file, or any hunk with `new_len == 0`, has `new_start` 0. Fix: when `new_len` is 0, name the
  old side ("Lines 12–20" of `old_start`/`old_len`).
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
- [ ] **The sticky title makes the list jump.** The bar exists only while a file is at the top
  (`crates/templum/src/git_block/diff_view/mod.rs:307`), so the list shifts by the bar's height each time a section
  heading reaches the top. Fix: always draw the bar (empty when no file is at the top).
- [ ] **Any change re-reads everything: the selection and every hunk's colors go.** `DiffPanel::set`
  (`crates/templum/src/git_block/diff_view/mod.rs:137-138`) clears both whenever any file changed, so while an agent
  is writing, a selection vanishes every 2 seconds. Fix: key the colors by (section, path, hunk header) instead of
  indices, and keep a selection whose rows still hold the same lines.

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

## Tests

- [ ] **Three tests don't check the words they are about.** `discard_all_with_a_filter_on_takes_only_the_rows_shown`
  (`crates/templum/src/workspace/tests/git_layout.rs`) doesn't check the count in the question;
  `a_hunk_that_changed_since_it_was_read_is_refused` (`tests/git_hunks.rs`) checks the notification count, not "The
  file changed; try again"; `past_the_cap_a_discard_keeps_nothing` (`tests/git_trash.rs`) doesn't check "Too much to
  keep a copy". Fix: read the dialog's description and the notification's message in each.

## Release and benches

- [ ] **The CHANGELOG will list three features twice.** Batch 2 has a `feat` commit for the library and another for
  the UI of the same feature: 726079b with c7e13c6 (hunks), 1d5bc9a with cb26cfd (the trash), efdd911 with 6362120
  (commit all, undo commit). Fix: edit the generated section at the next release so each feature reads once.
- [ ] **`--bench-zoom=git` measures an empty panel.** It runs from `~/Documents/github/DaemonQuest`, whose tree is
  usually clean, so the Git blocks it zooms show "Nothing changed since the last commit". `--bench-git` covers a full
  diff; for the zoom, leave a few files changed in that folder before running it, or note the state with the result.
