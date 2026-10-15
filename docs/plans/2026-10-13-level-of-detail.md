# Level of detail at far zoom — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** At 33% and 25% Templum stays smooth and readable from afar: an unfocused terminal is bars in its text's
colors, an editor or a note bars of its words, a Git block its card, and every block's title a fixed-size label with
the terminal's status dot; the level is picked from the text's size on screen at the zoom the camera rests at, and the
block you work in is always drawn in full.

**Architecture:** `templum-canvas` gains a pure `lod.rs` (`Level`, `level`, `GREEK_BELOW`, `CARD_BELOW`). The
workspace keeps `Space::rest`, the zoom the view last came to rest at; `block::render` asks `level` for each block
(Full for the block worked in), turns the title into a label below Full, and hands the level to the bodies.
Terminals paint `Row::bars()` (pure, `templum-terminal`) instead of shaped rows and are their cards under 2.5 px; a
Git block is its card below Full; editors and notes (Part 2) paint a word map taken from gpui-kit's last layout
(`templum/src/silhouette.rs`).

**Tech Stack:** Rust 2024, GPUI through `gpui-kit = "=0.7.1"` (gpui-pre 0.3.8, gpui-base 0.7.1, gpui-component
0.7.1), `alacritty_terminal` 0.26 inside `templum-terminal`. Tests: unit tests in each crate; `#[gpui_kit::test]`
workspace tests in `crates/templum/src/workspace/tests/` (`TestAppContext`, `gpui_kit::test::TestWindowExt`).

**Spec:** `docs/specs/level-of-detail.md` (approved; commit a05d4ca). Its Decisions section is settled: over bars or a
card the wheel pans the canvas and a click only focuses; a Git block is its card below Full; rendered Markdown is an
empty card; kanban is drawn in full at the Silhouette level; thresholds 5.5 px and 2.5 px.

**Two parts and a stop.** Part 1 (Tasks 1–7) is the spec's Delivery item 2 (bench, level, `Space::rest`, label,
terminal bars and card, Git card, wheel rule) plus rendered Markdown left out. **Task 8 is a checkpoint: the
executor reports to the maintainer and STOPS until the maintainer approves.** Part 2 (Tasks 9–13) is Delivery item 3
(the word map for notes and editors, the Git lists' scroll handles, the documentation).

## Where the code differs from the spec

Checked against HEAD a05d4ca. Every file:line the spec cites was re-read; these are the differences, and what the plan
does.

1. **"Recently discarded" is a dialog, not part of the Git block's body.** `git_block/discarded.rs:28-84` opens it
   with `window.open_alert_dialog`; the window's root draws it whatever the block's level. Only three lists lose
   their scroll when the block is a card: the commit details (`history.rs:366-372`), the comparison
   (`branches.rs:205-210`) and the stashes (`stashes.rs:113-118`). Task 11 gives those three a handle in `Git`.
2. **The Card level is reachable only for terminals.** Editors, notes and kanban blocks have 13 px text, 3.25 px at
   25% (the smallest step), over `CARD_BELOW` = 2.5; a Git block's 12 px is 3.0 px there, and it is a card below Full
   anyway. Only a terminal's font (6 to 72 pt) can go under 2.5 px (9 pt or less at 25%). So the plan writes a Card
   branch for terminals only (no speculative code, and none a test could reach), and a kanban block's level is never
   computed (it is drawn in full down to a Card it never reaches). The spec's minor "a kanban column's scroll resets
   at the Card level" can't happen: no `docs/minors.md` entry, unless the checkpoint (Task 8) decides kanban blocks
   become cards at the Silhouette level (spec, Decisions 2).
3. **The focus rule reuses `Body::has_focus`** (`workspace/mod.rs:265-278`). It calls `contains_focused`, which
   compares the handle itself with the window's focus first (`gpui-pre-0.3.8 src/key_dispatch.rs:346-349`, parent ==
   child) and only then looks for a descendant in the last frame's tree. The editors' and notes' handles, a Git
   block's own handle and a terminal's find field (`find.rs:22-24`, `is_focused`) are all matched directly. A Git
   block's field (summary, filter…) is matched through the tree: it holds the keyboard only after being drawn, and
   nothing focuses one on a block drawn as its card (`focus_summary` is reached from the block's own keys, or after
   `show_git_block` focused the block).
4. **`Space::rest` is also set in `fly`** (`workspace/view.rs:80-96`), not only in `render.rs`: a `Space` starts with
   `rest` 0, and a workspace shown by a glide before its first frame ("Go to the terminal waiting for you" in another
   workspace: `reveal` calls `switch_to` then `fly`) would draw that whole glide at the Card level.
5. **A label covers a far terminal's top rows,** as the spec says; the Ctrl+click test prints its URL below them.
6. **Rendered Markdown left out below Full is in Part 1** (Task 7), as requested for this plan; the spec's Delivery
   lists it in item 3. It doesn't depend on the word map.
7. `docs/plans/2026-10-12-performance-d.md` (cited as the style model) was deleted in 9b2a525; it reads with
   `git show 9b2a525^:docs/plans/2026-10-12-performance-d.md`.

## Global Constraints

- Only `crates/templum` depends on gpui-kit: `lod.rs` (`templum-canvas`) and `Row::bars` (`templum-terminal`) are
  pure and unit-tested headlessly. `gpui-kit = "=0.7.1"` stays pinned. No new dependency, no `unsafe`, no copyleft
  code.
- Thresholds are constants, never config: `GREEK_BELOW: f32 = 5.5`, `CARD_BELOW: f32 = 2.5`; Full at 5.5 px or more
  on screen, Silhouette from 2.5 px to under 5.5 px, Card under 2.5 px. The inputs are the text's size at zoom 1
  (terminal: `ws.cell.font_size`; editor and note: 13; Git block and title: 12) times `Space::rest`.
- The level changes only when the camera rests; the block worked in (focused, or holding the keyboard) is Full.
- Zoom scales like a photo: changing level moves nothing; bars sit on the letters' rows and columns within 1 px at
  every `ZOOM_STEPS` step; only a resize re-wraps.
- The label: 12 px text, an 8 px status dot (terminals), 4 px in from the card's border, on the card's background
  with a hairline border, at most the card's width less 8 px, cut with an ellipsis, never scaled; the same
  `("title", n)` and `("status", n)` elements. The header's small buttons hide while it is a label.
- A bar: half its row high (at least 1 px), centered on the row, over the cells (or the word) it stands for, at 60% of
  the text's opacity (`BAR_OPACITY = 0.6`); a dim run keeps the terminal's 0.66.
- Test element ids: `("silhouette", n)` wraps bars, `("title-text", n)` is a title's text.
- Every logic change ships a test that fails without it; UI-only parts say how they were checked by hand.
- Commits on `development` only, small, Conventional Commits with an imperative sentence; `feat`/`perf`/`fix` titles
  are written for someone who uses Templum. Every commit message ends with a blank line and then exactly:
  `Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV`
- Before every commit:
  `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  (chained with `&&`: a red check never commits).
- Any minor left out on purpose goes into `docs/minors.md` under its area (where, what a person sees, a fix), in the
  commit that leaves it out.
- Benches: bar `p95_ms` <= 16.7 on `--bench-far`; `--bench`, `--bench-mixed`, every `--bench-zoom=<kind>` and
  `--bench-git` no worse than before. Check `uptime` first (no other build running); compare two builds with
  alternating runs (the laptop drifts ~1 ms as it heats); run `--bench-zoom` and `--bench-far` from
  `~/Documents/github/DaemonQuest`.
- Read before writing: every GPUI/gpui-kit API used here is cited with its pinned source or is already used in this
  repository. Don't guess one.

## Review Focus

1. **A terminal full of colors, wide characters (CJK, emoji) or box drawing, seen at 25%:** one bar per run of
   non-blank cells of one color, a wide character's bar on its first column, blanks cutting bars, bold and italic
   not. Pinned in Task 5 (`row_bars` tests) and Task 6 (`a_far_terminals_bars_are_its_colored_runs_on_its_cells`).
2. **Focus moving between blocks at 33%, or held by a field:** the block left turns to bars or its card on the next
   frame; the one clicked is drawn in full; a find field or a Git field holding the keyboard keeps its block Full.
   Pinned in Task 3 (`a_far_git_block_is_its_card_unless_it_is_worked_in`) and Task 6
   (`a_focused_far_terminal_draws_its_letters_and_the_one_left_bars`, `a_find_field_hides_far_away_and_comes_back`).
3. **A canvas first drawn far away or mid-glide** (restored at 25%; Go to the terminal waiting for you, in a workspace
   not shown yet): the rest zoom is the camera's, never 0; a note or editor with no word map gets one after its first
   frames. Pinned in Task 3 (`a_canvas_first_drawn_mid_glide_keeps_the_zoom_it_rests_at`) and Task 9
   (`a_far_note_with_no_word_map_yet_gets_one_after_its_first_frames`).
4. **A theme switch while far:** a terminal's bars take the new palette at once (its snapshot is re-resolved);
   an editor's and a note's bars take the new foreground (no color is kept: `ws.theme` is read on every frame).
   Pinned in Task 6 (`a_far_terminals_bars_are_its_colored_runs_on_its_cells` switches to the dark theme); a
   painted color can't be read back headlessly, so the editors' and notes' half is a check by eye (Task 13, item 1).
5. **A block resized, or a file changed on disk, while far:** the word map is taken again (one unseen Full frame),
   never drawn from rows of another size or text. Pinned in Task 9
   (`a_far_notes_word_map_is_taken_again_after_a_resize`) and Task 10
   (`a_far_editor_shows_its_files_new_text_after_a_frame`).

## File Structure

`crates/templum-canvas` (no UI):

| File | Responsibility |
|---|---|
| `src/lod.rs` (new) | `Level`, `level(text, zoom)`, `GREEK_BELOW`, `CARD_BELOW`, unit tests |
| `src/lib.rs` | `mod lod;` and its exports |

`crates/templum-terminal` (no UI):

| File | Responsibility |
|---|---|
| `src/snapshot.rs` | `Bar`, `Row::bars()`, unit tests |
| `src/lib.rs` | exports `Bar` |

`crates/templum`:

| File | Responsibility |
|---|---|
| `src/bench.rs`, `src/main.rs` | `--bench-far`; `largest_files` replaces `sample_file` |
| `src/workspace/mod.rs` | `Space::rest` |
| `src/workspace/render.rs`, `src/workspace/view.rs` | `rest` set where the view is decided, and in `fly` |
| `src/block.rs` | each block's level (focus rule), the label, the header's buttons hidden, the wheel rule, the terminal's bars/card, the Git card, the level passed to bodies |
| `src/grid.rs` | `BAR_OPACITY`, `bar_quad`, bars painted in place of shaped rows |
| `src/note.rs`, `src/editor.rs` | rendered Markdown left out below Full; (Part 2) the word map's bars below Full, `Tab::reload` drops the map, the tree's empty panel |
| `src/silhouette.rs` (new, Part 2) | the word map: `Taken`, `Word`, `WordMap::take`/`cells`, `Far`, `body`, `note_origin`, `tab_origin`, `runs` |
| `src/git_block/{mod,history,branches,stashes}.rs` (Part 2) | three scroll handles in `Git` |
| `src/workspace/tests/lod.rs` (new), `tests/mod.rs`, `tests/terminals.rs` | headless tests; three terminal helpers made `pub(super)` |
| `docs/ARCHITECTURE.md`, `docs/specs/walking-skeleton.md`, `CLAUDE.md`, `docs/minors.md` (Part 2) | documentation |

---

# Part 1 — the level, the label, terminals and cards

### Task 1: `--bench-far`, and the numbers before

**Files:**
- Modify: `crates/templum/src/bench.rs:1-11` (module doc), `:135` (`run_zoom`'s sample file), `:229` (new `run_far`
  before `run_git`), `:373-401` (`sample_file` becomes `largest_files`)
- Modify: `crates/templum/src/main.rs:39-42` (flags), `:77-84` (dispatch)
- Modify: this plan's "Measurements" section (at its end)

**Interfaces:**
- Consumes: `Workspace::{open, close, add_editor_at, open_file, add_note_at, add_git_at, add_kanban_at, viewport,
  center, zoom_to, pan_by}`, `Space::{canvas, terminal, kanban_mut, blocks, focused, flight, pristine}`,
  `ws.focus_handle`, `bench::{TUI_LOOP, measure, sample_board, rss_mb}`.
- Produces: `templum --bench-far`; `pub(crate) fn run_far(window: AnyWindowHandle, ws: Entity<Workspace>, cx: &mut App)`;
  `fn largest_files(dir: &Path, n: usize) -> Vec<PathBuf>`; the binary `~/Documents/github/Templum/target/lod/before` and the script
  `~/Documents/github/Templum/target/lod/alternate.sh`, used by Tasks 8 and 13 (under `target/`, which git ignores and a reboot
  keeps: the checkpoint may stop the work for days).

This is a bench tool, as `--bench-zoom` was (performance-d, Task 1): it is checked by running it, not by a unit test.

- [ ] **Step 1: The n largest files**

In `crates/templum/src/bench.rs`, replace `sample_file` (lines 373-401, from its doc comment to its closing brace) with:

```rust
/// The `n` largest Rust, Markdown or TOML files under `dir` (not in `target` or a hidden
/// folder), largest first: text for the editors and the terminals to show.
fn largest_files(dir: &Path, n: usize) -> Vec<PathBuf> {
    let mut found: Vec<(u64, PathBuf)> = Vec::new();
    let mut folders = vec![(dir.to_path_buf(), 0)];
    while let Some((folder, depth)) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') || name == "target" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                if depth < 5 {
                    folders.push((path, depth + 1));
                }
            } else if path
                .extension()
                .is_some_and(|x| x == "rs" || x == "md" || x == "toml")
            {
                found.push((entry.metadata().map_or(0, |m| m.len()), path));
            }
        }
    }
    // A stable sort: of files of one size, the first found comes first, as before.
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().take(n).map(|(_, path)| path).collect()
}
```

In `run_zoom`, line 135, replace `let file = sample_file(&dir);` with:

```rust
            let file = largest_files(&dir, 1).into_iter().next();
```

- [ ] **Step 2: The scenario**

Insert before `/// \`--bench-git\`: see the module's doc.` (line 229):

```rust
/// `--bench-far`: see the module's doc.
pub(crate) fn run_far(window: AnyWindowHandle, ws: Entity<Workspace>, cx: &mut App) {
    let tuis = cx
        .update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                // Only the blocks measured: the startup terminal goes.
                let startup: Vec<_> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
                for id in startup {
                    ws.close(id, window, cx);
                }
                let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let files = largest_files(&dir, 6);
                let readme = std::fs::read_to_string(dir.join("README.md")).unwrap_or_default();
                let board = sample_board(&mut ws.boards);
                let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, ws.cell.size);
                // Six a row, in the mixed scenario's slots.
                let mut slots =
                    (0..).map(|i| Point::new((i % 6) as f32 * 950.0, (i / 6) as f32 * 750.0));
                let mut next = || slots.next().expect("endless");
                // 8 terminals: the first 5 get the TUI loop once their shells run, 3 show a file.
                let mut tuis = Vec::new();
                for i in 0..8 {
                    let p = next();
                    ws.open(Rect::new(p.x, p.y, size.w, size.h), window, cx);
                    let Some(id) = ws.space.canvas.top() else {
                        continue;
                    };
                    if i < 5 {
                        tuis.push(id);
                    } else if let (Some(pane), Some(file)) = (ws.space.terminal(id), files.first())
                    {
                        pane.term
                            .write(format!("cat '{}'\r", file.display()).into_bytes());
                    }
                }
                for i in 0..6 {
                    let at = next();
                    if let (Some(id), Some(file)) =
                        (ws.add_editor_at(&dir, at, window, cx), files.get(i))
                    {
                        ws.open_file(id, file, None, window, cx);
                    }
                }
                for _ in 0..4 {
                    ws.add_note_at(next(), window, cx);
                    let id = ws.space.canvas.top().expect("just added");
                    if let Some(note) = ws.space.blocks.get(&id).and_then(Body::note) {
                        note.editor
                            .update(cx, |e, cx| e.set_value(readme.clone(), window, cx));
                    }
                }
                for _ in 0..3 {
                    ws.add_git_at(&dir, next(), window, cx);
                }
                for _ in 0..3 {
                    ws.add_kanban_at(next(), window, cx);
                    let id = ws.space.canvas.top().expect("just added");
                    if let Some(k) = ws.space.kanban_mut(id) {
                        k.board = Some(board);
                    }
                }
                // No block focused: the workspace keeps the keyboard, so every block is drawn
                // at its own level.
                ws.space.focused = None;
                window.focus(&ws.focus_handle, cx);
                tuis
            })
        })
        .unwrap_or_default();
    cx.spawn(async move |cx| {
        // Let the shells start, the files open and the Git blocks read.
        cx.background_executor().timer(Duration::from_secs(3)).await;
        let idle_rss = rss_mb();
        let _ = cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                for id in &tuis {
                    if let Some(pane) = ws.space.terminal(*id) {
                        pane.term.write(TUI_LOOP);
                    }
                }
                ws.space.pristine = false;
                let center = ws
                    .space
                    .canvas
                    .bounds()
                    .map(|b| Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0))
                    .unwrap_or_default();
                let vp = ws.viewport(window);
                let far = ZOOM_STEPS[1];
                ws.space.canvas.camera = Camera::at(
                    Point::new(center.x - vp.w / 2.0 / far, center.y - vp.h / 2.0 / far),
                    far,
                );
                ws.space.flight = None;
                measure(ws, idle_rss, move |ws, window, cx, t| {
                    // 33% for 5 s, then one glide out to 25%, which it keeps.
                    if t >= 5.0 && ws.space.canvas.camera.zoom_index() > 0 {
                        let pivot = ws.center(window);
                        ws.zoom_to(-1, pivot, cx);
                    }
                    // Around the canvas's center, 1,500 world units out, a turn every 4 s:
                    // blocks keep entering and leaving the view.
                    let cam = ws.space.canvas.camera;
                    let z = cam.zoom();
                    let a = t * std::f32::consts::TAU / 4.0;
                    let vp = ws.viewport(window);
                    let want = Point::new(
                        center.x + 1500.0 * a.cos() - vp.w / 2.0 / z,
                        center.y + 1500.0 * a.sin() - vp.h / 2.0 / z,
                    );
                    ws.pan_by(Point::new(
                        (cam.origin.x - want.x) * z,
                        (cam.origin.y - want.y) * z,
                    ));
                });
                cx.notify();
            })
        });
    })
    .detach();
}

```

- [ ] **Step 3: The module's doc**

In the module doc (lines 1-11), insert before the line `//! Prints one JSON line (frame-interval percentiles, memory), then quits. Bar: p95 <= 16.7 ms.`:

```rust
//! `templum --bench-far`: an agent cockpit seen from afar, on the folder Templum runs from: 24
//! blocks, 6 a row (8 terminals, 5 redrawing the mixed scenario's colored screen and 3 showing
//! its largest file; 6 editors on its 6 largest files; 4 notes holding its README; 3 Git blocks;
//! 3 kanban blocks), none focused. The camera rests at 33% for 5 s, then glides to 25% and rests
//! there, while it circles 1,500 world units around the canvas's center.
```

- [ ] **Step 4: The flag**

In `crates/templum/src/main.rs`, replace lines 39-42 with:

```rust
    let mixed = std::env::args().any(|a| a == "--bench-mixed");
    let far = std::env::args().any(|a| a == "--bench-far");
    let zoom = std::env::args().find_map(|a| a.strip_prefix("--bench-zoom=").map(str::to_string));
    let git = std::env::args().any(|a| a == "--bench-git");
    let bench =
        mixed || far || zoom.is_some() || git || std::env::args().any(|a| a == "--bench");
```

and the dispatch (lines 77-83) with:

```rust
            if let Some(kind) = zoom {
                bench::run_zoom(window, ws, kind, cx);
            } else if git {
                bench::run_git(window, ws, cx);
            } else if far {
                bench::run_far(window, ws, cx);
            } else if bench {
                bench::run(window, ws, mixed, cx);
            }
```

- [ ] **Step 5: Check and run it once**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo build --release -p templum
cd ~/Documents/github/DaemonQuest && ~/Documents/github/Templum/target/release/templum --bench-far
```

Expected: the check is green. The window shows 24 blocks in 4 rows of 6, the 5 TUI terminals redraw colored lines,
the view rests at 33% (5 s), glides to 25%, circles; after ~13 s one JSON line
(`{"frames":…,"p50_ms":…,"p95_ms":…,…}`) and the app quits. Run `--bench-zoom=editor` once too (Step 1 changed its
file picker): it still opens the folder's largest file.

- [ ] **Step 6: Commit**

```bash
git add crates/templum/src/bench.rs crates/templum/src/main.rs
git commit -F - <<'EOF'
chore: add --bench-far, an agent cockpit seen from 33% and 25%

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

- [ ] **Step 7: Keep the "before" binary and the alternating script**

```bash
uptime
cargo build --release -p templum
mkdir -p ~/Documents/github/Templum/target/lod
cp ~/Documents/github/Templum/target/release/templum ~/Documents/github/Templum/target/lod/before
```

`uptime`'s load average must be low (under ~1.0) and no other build running; otherwise wait. Then write
`~/Documents/github/Templum/target/lod/alternate.sh` and `chmod +x` it:

```bash
#!/usr/bin/env bash
# Usage: templum-alternate.sh <rounds> <folder> <flag> <before> <after>
# Runs the two binaries one after the other, <rounds> times each, from <folder>, so the laptop's
# warming drifts both alike. Prints every JSON line, then each binary's medians.
set -euo pipefail
rounds=$1 dir=$2 flag=$3 before=$4 after=$5
out=$(mktemp -d)
for _ in $(seq "$rounds"); do
  for which in before after; do
    bin=$before
    [ "$which" = after ] && bin=$after
    line=$(cd "$dir" && "$bin" "$flag" 2>/dev/null | grep '^{"frames"')
    echo "$which $line"
    echo "$line" >> "$out/$which"
  done
done
for which in before after; do
  python3 - "$out/$which" "$which" <<'PY'
import json, statistics, sys
rows = [json.loads(line) for line in open(sys.argv[1])]
keys = ("frames", "p50_ms", "p95_ms", "p99_ms", "max_ms")
print(sys.argv[2], "median", {k: round(statistics.median(r[k] for r in rows), 2) for k in keys})
PY
done
```

- [ ] **Step 8: The numbers before**

Three runs each of the unchanged code (the script with the same binary on both sides gives 6 runs and two equal
medians; take the "before" line):

```bash
S=~/Documents/github/Templum/target/lod/alternate.sh B=~/Documents/github/Templum/target/lod/before
$S 3 ~/Documents/github/DaemonQuest --bench-far $B $B
$S 3 ~/Documents/github/Templum --bench $B $B
$S 3 ~/Documents/github/Templum --bench-mixed $B $B
```

Expected: `--bench-far`'s p95 well over the other scenarios' (that is the complaint this plan fixes); `--bench` and
`--bench-mixed` near their usual numbers. Paste the medians and the JSON lines into "Measurements → Before" at the end
of this plan (they are committed with the checkpoint, Task 8).

---

### Task 2: The level of detail, in `templum-canvas`

**Files:**
- Create: `crates/templum-canvas/src/lod.rs`
- Modify: `crates/templum-canvas/src/lib.rs:5-25` (`mod lod;`, exports)

**Interfaces:**
- Consumes: `crate::ZOOM_STEPS` (tests).
- Produces: `templum_canvas::{Level, level, GREEK_BELOW, CARD_BELOW}`:
  - `pub enum Level { Full, Silhouette, Card }` (`Clone, Copy, Debug, PartialEq, Eq`)
  - `pub const GREEK_BELOW: f32 = 5.5;` `pub const CARD_BELOW: f32 = 2.5;`
  - `pub fn level(text: f32, zoom: f32) -> Level` — `text`: the text's size at zoom 1.

- [ ] **Step 1: Write the failing tests**

Create `crates/templum-canvas/src/lod.rs` with its tests (the code comes in Step 3, above them):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ZOOM_STEPS;

    /// A terminal (14), an editor, a note or a kanban block (13), a Git block or a title (12).
    const DEFAULTS: [f32; 3] = [12.0, 13.0, 14.0];

    #[test]
    fn default_sizes_are_bars_at_25_and_33_and_letters_from_50() {
        for text in DEFAULTS {
            for zoom in [0.25, 0.33] {
                assert_eq!(level(text, zoom), Level::Silhouette, "{text} at {zoom}");
            }
            for zoom in ZOOM_STEPS.into_iter().filter(|&z| z >= 0.5) {
                assert_eq!(level(text, zoom), Level::Full, "{text} at {zoom}");
            }
        }
    }

    /// Float error can't flip a level: no step puts a default size within 0.25 px of a threshold.
    #[test]
    fn no_step_puts_a_default_size_near_a_threshold() {
        for text in DEFAULTS {
            for zoom in ZOOM_STEPS {
                for threshold in [GREEK_BELOW, CARD_BELOW] {
                    let off = (text * zoom - threshold).abs();
                    assert!(off >= 0.25, "{text} at {zoom}: {off} px from {threshold}");
                }
            }
        }
    }

    #[test]
    fn small_fonts_turn_to_cards_and_large_ones_keep_their_letters() {
        assert_eq!(level(9.0, 0.25), Level::Card, "2.25 px");
        assert_eq!(level(10.0, 0.25), Level::Silhouette, "2.5 px");
        assert_eq!(level(17.0, 0.33), Level::Full, "5.61 px");
    }
}
```

Add to `crates/templum-canvas/src/lib.rs`, after `mod ink;`:

```rust
mod lod;
```

and after the `pub use ink::{…};` statement:

```rust
pub use lod::{CARD_BELOW, GREEK_BELOW, Level, level};
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-canvas lod`
Expected: FAIL to compile (`cannot find function \`level\``, `cannot find type \`Level\``).

- [ ] **Step 3: The level**

Above the tests in `lod.rs`:

```rust
//! Level of detail: how much of a block is drawn, from how big its text is on screen (its size at
//! zoom 1 times the zoom), like InDesign's "Greek Type Below". Pure: unit-tested here.
//! See docs/specs/level-of-detail.md, section 1.

/// Text smaller than this on screen (pixels) is drawn as bars: about the largest default size at
/// 33% (4.62 px) and the smallest at 50% (6.0 px) apart, so every default block changes level at
/// the same step and none sits on the edge.
pub const GREEK_BELOW: f32 = 5.5;
/// Under this a row is about 3 px and its bars 1 px lines: the block is its card.
pub const CARD_BELOW: f32 = 2.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Drawn as it is: the text is readable.
    Full,
    /// The text is bars in its own colors, on the letters' rows and columns.
    Silhouette,
    /// The block's card with its title and status, nothing inside.
    Card,
}

/// The level of text `text` units high at zoom 1, drawn at `zoom`.
pub fn level(text: f32, zoom: f32) -> Level {
    let on_screen = text * zoom;
    if on_screen >= GREEK_BELOW {
        Level::Full
    } else if on_screen >= CARD_BELOW {
        Level::Silhouette
    } else {
        Level::Card
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum-canvas lod`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum-canvas/src/lod.rs crates/templum-canvas/src/lib.rs && \
git commit -F - <<'EOF'
refactor(canvas): pick a level of detail from the text's size on screen

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 3: The zoom at rest, each block's level, and the Git block's card

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs:325-327` (`Space::rest` after `flight`)
- Modify: `crates/templum/src/workspace/render.rs:239` (after the view is decided)
- Modify: `crates/templum/src/workspace/view.rs:85-91` (`fly`)
- Modify: `crates/templum/src/block.rs:17` (import), `:91` (the level), `:388-396` (the Git body), new `far_level`
  after `exact_rows` (`:585-592`)
- Create: `crates/templum/src/workspace/tests/lod.rs`
- Modify: `crates/templum/src/workspace/tests/mod.rs:528-529` (`mod lod;` between `mod kanban;` and `mod menus;`)

**Interfaces:**
- Consumes: `templum_canvas::{Level, level}` (Task 2); `Body::has_focus(&self, window: &Window, cx: &App) -> bool`
  (`workspace/mod.rs:265-278`); test helpers `open`, `open_git`, `committed_repo`, `git_of`, `frame`,
  `restart_glide`, `first_id` (`tests/mod.rs`).
- Produces:
  - `Space { pub rest: f32, … }` — the zoom the view last came to rest at.
  - in `block::render`: `let level: Level` for the body (Full for the block worked in), used by Tasks 4, 6, 7, 9, 10.
  - `fn far_level(ws: &Workspace, body: &Body) -> Level` (block.rs, private).
  - test helpers in `tests/lod.rs` (private to it, used by every later test there):
    `fn rest_at(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext, at: Point, zoom: f32)`,
    `fn unfocus(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext)`,
    `fn shown(handle: AnyWindowHandle, cx: &mut TestAppContext, id: impl Into<ElementId>) -> bool`,
    `fn git_tabs(id: BlockId) -> ElementId`.

- [ ] **Step 1: Write the failing tests**

Create `crates/templum/src/workspace/tests/lod.rs`:

```rust
//! Level of detail at far zoom: what each block draws at 33% and 25%, and what keeps working
//! there. See docs/specs/level-of-detail.md.

use std::time::Duration;

use gpui_kit::{ElementId, Focusable as _};

use super::*;

/// The camera at rest at `zoom`, its top-left corner at `at` (world units), and a frame drawn.
fn rest_at(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    at: Point,
    zoom: f32,
) {
    ws.update(cx, |ws, _| {
        ws.space.pristine = false;
        ws.space.canvas.camera = Camera::at(at, zoom);
        ws.space.flight = None;
    });
    frame(handle, cx);
}

/// No block focused, the workspace with the keyboard: as after a click on the empty canvas.
fn unfocus(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        let focus = ws.update(cx, |ws, _| {
            ws.space.focused = None;
            ws.focus_handle.clone()
        });
        window.focus(&focus, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// The element was drawn in the last frame.
fn shown(handle: AnyWindowHandle, cx: &mut TestAppContext, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(handle, |_, window, _| window.try_find(id).is_some())
        .unwrap()
}

/// A Git block's Changes tab: there while its body is built.
fn git_tabs(id: BlockId) -> ElementId {
    ElementId::Name(format!("git-tab-{}-changes", id.0).into())
}

/// The level follows the zoom the camera last came to rest at: a glide keeps it, through a pan
/// mid-glide, and it changes on the frame the glide ends.
#[gpui_kit::test]
fn the_rest_zoom_holds_through_a_glide_and_changes_when_it_ends(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.5);
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.rest, 0.5));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let pivot = ws.center(window);
            ws.zoom_to(-1, pivot, cx);
        })
    })
    .unwrap();
    ws.update(cx, restart_glide);
    std::thread::sleep(Duration::from_millis(40));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, _| ws.pan_by(Point::new(30.0, 0.0)));
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let z = ws.space.view.zoom();
        assert!(ws.space.flight.is_some() && z < 0.5, "mid-glide at {z}");
        assert_eq!(ws.space.rest, 0.5, "the zoom it rested at");
    });
    std::thread::sleep(crate::motion::FLIGHT + Duration::from_millis(50));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.flight.is_none(), "landed");
        assert_eq!(ws.space.rest, 0.33);
    });
}

/// A workspace shown by a glide before its first frame (Go to the terminal waiting for you, in
/// another workspace) is drawn at the level of the zoom its camera rests at, not of no zoom.
#[gpui_kit::test]
fn a_canvas_first_drawn_mid_glide_keeps_the_zoom_it_rests_at(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            assert_eq!(ws.space.rest, 0.0, "never drawn");
            let pivot = ws.center(window);
            ws.zoom_to(-1, pivot, cx);
        })
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.flight.is_some());
        assert_eq!(ws.space.rest, 1.0, "the camera rested at 100% before the glide");
    });
}

/// Below Full a Git block is its card: no tabs, lists, diff or commit form. The block worked in is
/// Full at any zoom: the focused one, and one whose field holds the keyboard.
#[gpui_kit::test]
fn a_far_git_block_is_its_card_unless_it_is_worked_in(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    unfocus(handle, &ws, cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.5);
    assert!(shown(handle, cx, git_tabs(id)), "12 px at 50%: 6 px, Full");
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.33);
    assert!(!shown(handle, cx, git_tabs(id)), "3.96 px at 33%: its card");
    assert!(shown(handle, cx, ("block", id.0 as usize)), "the card is drawn");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, git_tabs(id)), "focused: Full at 33%");
    // Its summary holds the keyboard, and no block is focused: still Full.
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        let field = summary.read(cx).focus_handle(cx);
        window.focus(&field, cx);
        ws.update(cx, |ws, _| ws.space.focused = None);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, git_tabs(id)), "a field with the keyboard keeps it Full");
    unfocus(handle, &ws, cx);
    assert!(!shown(handle, cx, git_tabs(id)), "left: its card again");
}

/// The level changes only when the camera rests: a glide's frames keep the level it started at,
/// and the frame it ends draws the new one.
#[gpui_kit::test]
fn a_git_blocks_level_changes_when_a_glide_ends(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    unfocus(handle, &ws, cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.5);
    // 50% to 33%: Full until the glide ends; then 33% to 50%: the card until it ends.
    for (step, during, landed) in [(-1, true, false), (1, false, true)] {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let pivot = ws.center(window);
                ws.zoom_to(step, pivot, cx);
            })
        })
        .unwrap();
        ws.update(cx, restart_glide);
        std::thread::sleep(Duration::from_millis(40));
        frame(handle, cx);
        assert!(ws.read_with(cx, |ws, _| ws.space.flight.is_some()), "mid-glide");
        assert_eq!(shown(handle, cx, git_tabs(id)), during, "mid-glide, step {step}");
        std::thread::sleep(crate::motion::FLIGHT + Duration::from_millis(50));
        frame(handle, cx);
        assert_eq!(shown(handle, cx, git_tabs(id)), landed, "landed, step {step}");
    }
}
```

In `crates/templum/src/workspace/tests/mod.rs`, add between `mod kanban;` and `mod menus;`:

```rust
mod lod;
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum --bin templum lod::`
Expected: FAIL to compile (`no field \`rest\` on type \`Space\``).

- [ ] **Step 3: `Space::rest`**

In `crates/templum/src/workspace/mod.rs`, after the `flight` field (line 327):

```rust
    /// The zoom the view last came to rest at. Blocks are drawn at its level of detail, so a
    /// glide keeps the level it started at, and the level changes on the frame the glide ends.
    pub rest: f32,
```

In `crates/templum/src/workspace/render.rs`, after the statement that sets `self.space.view` (it ends at line
239, `};`):

```rust
        // The level of detail changes only at rest (docs/specs/level-of-detail.md, section 1).
        if self.space.flight.is_none() {
            self.space.rest = self.space.view.zoom();
        }
```

In `crates/templum/src/workspace/view.rs`, replace lines 85-91 (the `from` statement in `fly`) with:

```rust
        // Mid-flight, from where it is drawn; otherwise the camera is what was last drawn (or
        // moved since, by a drag), at rest there until now, on a canvas not drawn yet too.
        let from = if self.space.flight.is_some() {
            self.space.view
        } else {
            self.space.rest = self.space.canvas.camera.zoom();
            self.space.canvas.camera
        };
```

- [ ] **Step 4: Each block's level, and the Git block's card**

In `crates/templum/src/block.rs`, line 17, import `Level`:

```rust
use templum_canvas::{Block, BlockId, Edges, Level, Point, Rect, chrome};
```

After line 91 (`let focused = live && ws.space.focused == Some(id);`):

```rust
    // How much of the body is drawn: the block worked in (focused, or holding the keyboard: GPUI
    // drops the focus of what isn't drawn) is always Full; the others by their text's size on
    // screen at the zoom the camera rests at.
    let level = if focused || (live && body.has_focus(window, cx)) {
        Level::Full
    } else {
        far_level(ws, body)
    };
```

Replace the Git arm of the `content` match (lines 388-396) with:

```rust
        // A fading Git block is drawn empty, like a note. Below Full it is its card: its body is
        // dozens of components whose shape says little from afar, and its diff rows and commit
        // fields would do their per-frame work for nothing.
        Body::Git(g) => frame.when(live && level == Level::Full, |el| {
            el.child(settled_box(
                block,
                settled,
                pull,
                git_block::body(ws, id, g, settled, cx),
            ))
        }),
```

After `exact_rows` (ends at line 592):

```rust
/// The level of a block nobody works in: its text's size at zoom 1 times the zoom the camera
/// rests at (docs/specs/level-of-detail.md, section 1).
fn far_level(ws: &Workspace, body: &Body) -> Level {
    let text = match body {
        Body::Terminal(_) => ws.cell.font_size,
        Body::Editor(_) => editor::FONT,
        Body::Note(_) => note::FONT,
        Body::Git(_) => git_block::FONT,
        // Drawn in full down to the Card level, which its 13 px never reach (3.25 px at 25%).
        Body::Kanban(_) => return Level::Full,
    };
    templum_canvas::level(text, ws.space.rest)
}
```

- [ ] **Step 5: Run them to see them pass**

Run: `cargo test -p templum --bin templum lod::`
Expected: PASS (4 tests). Then `cargo test -p templum --bin templum -- navigation git_` — the zoom and Git tests still
pass (`the_rows_are_whole_pixels_at_every_zoom` zooms a Git block to 25%: `open_git` focused it, so it is Full).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum/src/workspace/mod.rs crates/templum/src/workspace/render.rs \
  crates/templum/src/workspace/view.rs crates/templum/src/block.rs \
  crates/templum/src/workspace/tests/lod.rs crates/templum/src/workspace/tests/mod.rs && \
git commit -F - <<'EOF'
perf: draw Git blocks seen from far away as their cards

At 33% and 25% a Git block you aren't working in no longer builds its lists,
diff panel and commit form; the level changes when the camera comes to rest.

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 4: The label

**Files:**
- Modify: `crates/templum/src/block.rs:29-38` (constants), `:114-188` (the title, the close button, the dot: replaced),
  `:190-252` (the three header buttons' guards), `:313-323` (header), `:447-452` (background), `:490-491` (children
  order)
- Test: `crates/templum/src/workspace/tests/lod.rs`

**Interfaces:**
- Consumes: `Space::rest` and `level` (Task 3); test helpers `rest_at`, `unfocus`, `shown` (Task 3), `press_with`,
  `first_id` (`tests/mod.rs`); `crate::alert::Alert::Attention(Option<String>)`; `Pane::alert`.
- Produces: in `block::render`, `labeled: bool` (the title is a label); the `("title-text", n)` element; constants
  `LABEL_IN: f32 = 4.0` and `DOT: f32 = 8.0` (block.rs, private).

- [ ] **Step 1: Write the failing tests**

Append to `crates/templum/src/workspace/tests/lod.rs`:

```rust
/// Below Full a title is a label: 12 px text and an 8 px status dot at any zoom, 4 px in from the
/// card's border. At 100% the header is as it was.
#[gpui_kit::test]
fn a_far_title_is_a_label_of_a_fixed_size(cx: &mut TestAppContext) {
    use crate::alert::Alert;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    unfocus(handle, &ws, cx);
    // News nobody looked at: the dot shows.
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().alert = Some(Alert::Attention(None))
    });
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    let corner = Point::new(rect.x - 10.0, rect.y - 10.0);
    let bounds = |cx: &mut TestAppContext, el: (&'static str, usize)| {
        cx.update_window(handle, |_, window, _| window.find(el).bounds())
            .unwrap()
    };
    rest_at(handle, &ws, cx, corner, 1.0);
    let block = bounds(cx, ("block", n));
    assert_eq!(bounds(cx, ("status", n)).size, size(px(7.0), px(7.0)), "the header's dot");
    assert_eq!(
        bounds(cx, ("title", n)).size.width,
        block.size.width - px(2.0),
        "the header's title spans the card"
    );
    let text_at_100 = bounds(cx, ("title-text", n)).size.height;
    for zoom in [0.33, 0.25] {
        rest_at(handle, &ws, cx, corner, zoom);
        let (block, title) = (bounds(cx, ("block", n)), bounds(cx, ("title", n)));
        let (text, dot) = (bounds(cx, ("title-text", n)), bounds(cx, ("status", n)));
        assert_eq!(dot.size, size(px(8.0), px(8.0)), "an 8 px dot at {zoom}");
        assert_eq!(text.size.height, text_at_100, "12 px text at {zoom}, as at 100%");
        let inset = |a: gpui_kit::Pixels, b: gpui_kit::Pixels| (f32::from(a - b) - 5.0).abs() < 0.01;
        assert!(
            inset(title.origin.x, block.origin.x) && inset(title.origin.y, block.origin.y),
            "1 px border and 4 px in at {zoom}: {title:?} in {block:?}"
        );
        assert!(title.size.width <= block.size.width - px(10.0), "inside the card at {zoom}");
        assert!(
            dot.origin.x > title.origin.x && dot.right() < title.right(),
            "the dot is in the label at {zoom}"
        );
    }
}

/// A press on a far label moves the block and focuses it, and starts no selection in the terminal
/// under it; a right-click opens the block's menu; a double-click frames the block.
#[gpui_kit::test]
fn a_far_label_moves_opens_the_menu_and_frames(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(handle, &ws, cx, Point::new(rect.x - 10.0, rect.y - 10.0), 0.25);
    let c = cx
        .update_window(handle, |_, window, _| window.find(("title", n)).bounds().center())
        .unwrap();
    let at = (f32::from(c.x), f32::from(c.y));
    cx.update_window(handle, |_, window, cx| {
        press_with(window, cx, MouseButton::Left, Modifiers::default(), at, (at.0 + 40.0, at.1));
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let moved = ws.space.canvas.get(id).unwrap().rect;
        assert!(moved.x > rect.x + 100.0, "40 px at 25%: {rect:?} -> {moved:?}");
        assert_eq!(ws.space.focused, Some(id), "and focused");
        let term = ws.space.terminal(id).unwrap().term.shared();
        assert!(term.selection_text().is_none(), "no selection under the label");
    });
    cx.update_window(handle, |_, window, cx| window.right_click(("title", n), cx))
        .unwrap();
    ws.update(cx, |ws, _| {
        assert!(ws.menu.is_some(), "its menu");
        ws.menu = None;
    });
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| window.double_click(("title", n), cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        let z = ws.space.canvas.camera.zoom();
        assert!(z > 0.25, "framed: the camera flies to {z}");
    });
}

/// The header's small buttons (a note's preview eye here) hide behind a far label.
#[gpui_kit::test]
fn the_headers_buttons_hide_behind_a_far_label(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let note = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.add_note_at(Point::new(5000.0, 0.0), window, cx);
                ws.space.appearing.clear();
                ws.space.canvas.top().unwrap().0 as usize
            })
        })
        .unwrap();
    rest_at(handle, &ws, cx, Point::new(4990.0, -10.0), 1.0);
    assert!(shown(handle, cx, ("preview", note)));
    rest_at(handle, &ws, cx, Point::new(4990.0, -10.0), 0.25);
    assert!(!shown(handle, cx, ("preview", note)), "the label sits there");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum --bin templum -- lod::a_far_ lod::the_headers`
Expected: FAIL. `a_far_title_is_a_label_of_a_fixed_size` panics with `missing ElementId NamedInteger("title-text", …)`
(no such element yet); `a_far_label_moves_opens_the_menu_and_frames` fails on "no selection under the label" or on
the move (the terminal's press under the title starts a selection); `the_headers_buttons_hide_behind_a_far_label`
fails on "the label sits there".

- [ ] **Step 3: The constants**

In `crates/templum/src/block.rs`, after `const EDGE: f32 = 6.0;` (line 38):

```rust
/// Below Full a title is a label this far in from the card's border, and a terminal's status
/// dot in it is this wide: screen pixels, never zoomed.
const LABEL_IN: f32 = 4.0;
const DOT: f32 = 8.0;
```

- [ ] **Step 4: The title, the label and the dot**

Replace lines 114-188 (from `let title = div()` through the end of the `let dot = match body { … };` statement; the
close button's code in between is kept as it is, below) with:

```rust
    // The card's background, a note's tint included; a label sits on it too.
    let bg = match body {
        Body::Note(note) => note
            .color
            .map_or(ws.ui.surface, |color| ws.theme.note_bg(color)),
        _ => ws.ui.surface,
    };
    // Below Full the title is a label: never scaled, at the card's top-left corner, over the
    // header and the top of the body (docs/specs/level-of-detail.md, section 2).
    let labeled = templum_canvas::level(TITLE_FONT, ws.space.rest) != Level::Full;
    let title_text = div()
        .id(("title-text", n))
        .truncate()
        .when(labeled, |el| el.min_w(px(0.0)))
        .text_size(px(TITLE_FONT * if labeled { 1.0 } else { z }))
        .text_color(if focused { ui.title } else { ui.title_dim })
        .child(match body {
            Body::Terminal(pane) => SharedString::from(pane.term.title()),
            Body::Kanban(k) => k.title(ws),
            Body::Note(n) => n.title(cx),
            Body::Editor(e) => e.title(),
            Body::Git(g) => g.title(),
        })
        .test_support();
    // A terminal's state: on the left of its header, or in its label.
    let dot = match body {
        Body::Terminal(pane) if live => status_dot(ws, pane).map(|(color, text)| {
            div()
                .id(("status", n))
                .flex_shrink_0()
                .size(px(if labeled { DOT } else { 7.0 * z }))
                .rounded_full()
                .bg(color)
                .when(!labeled, |el| {
                    el.absolute()
                        .top(px((chrome::TITLE_H - 7.0) / 2.0 * z))
                        .left(px(12.0 * z))
                })
                .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
                .test_support()
        }),
        _ => None,
    };
    let (label_dot, header_dot) = if labeled { (dot, None) } else { (None, dot) };
    let title = if labeled {
        div()
            .id(("title", n))
            .absolute()
            .top(px(LABEL_IN))
            .left(px(LABEL_IN))
            .max_w(px((r.w - 2.0 * BORDER - 2.0 * LABEL_IN).max(0.0)))
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(6.0))
            .py(px(2.0))
            .overflow_hidden()
            .rounded(px(4.0))
            .border_1()
            .border_color(ui.hairline)
            .bg(bg)
            .children(label_dot)
            .child(title_text)
    } else {
        div()
            .id(("title", n))
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            // Clear of the close button on the right, and centered.
            .px(px(28.0 * z))
            .overflow_hidden()
            .child(title_text)
    }
    .when(live, |el| {
        el.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                // A double-click frames the block; the camera flies, so nothing moves.
                if e.click_count == 2 {
                    ws.frame_block(id, window, cx);
                } else {
                    ws.begin_move(id, e.position);
                }
                // A label lies over the body: it takes the press itself, so a terminal under it
                // starts no selection, and it focuses the block as the card would.
                if labeled {
                    ws.focus_block(id, window, cx);
                    cx.stop_propagation();
                }
            }),
        )
    })
    .test_support();

    // Painted over the title: pressing it never starts a move.
    let close = div()
        .id(("close", n))
        .absolute()
        .top(px(4.0 * z))
        .right(px(4.0 * z))
        .flex()
        .items_center()
        .justify_center()
        .size(px(20.0 * z))
        .rounded(px(5.0 * z))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .child(
            Icon::new(IconName::Close)
                .size(px(12.0 * z))
                .text_color(ui.title),
        )
        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| ws.close(id, window, cx)))
        .test_support();
```

- [ ] **Step 5: The buttons hide, the header, the background, the order**

In the three header buttons, add `&& !labeled` to each guard (the label sits where they are, and they would be 5 px
wide at 25%):

```rust
        Body::Kanban(_) if live && !labeled => Some(
```

```rust
        Body::Note(note) if live && !labeled => {
```

```rust
        Body::Editor(e) if live && !labeled => {
```

In the header (`let header = div()…`), replace `.children(dot)` with `.children(header_dot)`.

In the root's style, replace the background match:

```rust
        .bg(match body {
            Body::Note(note) => note
                .color
                .map_or(ws.ui.surface, |color| ws.theme.note_bg(color)),
            _ => ws.ui.surface,
        })
```

with `.bg(bg)`, and replace `.child(header)` `.child(content)` (lines 490-491) with:

```rust
        .map(|el| {
            // A label lies over the top of the body: the header is drawn after it then.
            if labeled {
                el.child(content).child(header)
            } else {
                el.child(header).child(content)
            }
        })
```

(The header is the root's only child in the flow and the body is absolute: the order changes what is drawn on top,
not where anything is. The root's `overflow_hidden` clips the label to the card.)

- [ ] **Step 6: Run them to see them pass**

Run: `cargo test -p templum --bin templum lod::`
Expected: PASS (7 tests). Then `cargo test -p templum --bin templum -- navigation blocks menus` — the title
double-click, the block moves and the menus still pass at 100%.

- [ ] **Step 7: Check by eye**

`cargo run -p templum`: a terminal running `sleep 30` in one block and another focused; zoom out to 33% and 25%
(Ctrl+−): titles are small tags at the top-left corners, readable, the dot beside the title; the close button and
grip still show on hover; back to 50%: the centered headers as before.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum/src/block.rs crates/templum/src/workspace/tests/lod.rs && \
git commit -F - <<'EOF'
feat: keep block titles and terminal status dots readable at far zoom

At 33% and 25% a block's title is a label of 12 px text at its card's corner,
with the terminal's status dot at 8 px; drag it to move the block, double-click
it to frame the block.

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 5: A row's text as bars, in `templum-terminal`

**Files:**
- Modify: `crates/templum-terminal/src/snapshot.rs:31-36` (`Bar` after `Span`), `:50-106` (`Row::bars`), tests
  (`:297-562`)
- Modify: `crates/templum-terminal/src/lib.rs:21` (export `Bar`)

**Interfaces:**
- Consumes: `Row { text, runs }`, `Run { len, fg, dim, … }`, `Span { start, len }`.
- Produces: `templum_terminal::Bar` and `Row::bars`:
  - `pub struct Bar { pub cols: Span, pub fg: Rgb, pub dim: bool }` (`Clone, Copy, Debug, PartialEq`)
  - `impl Row { pub fn bars(&self) -> Vec<Bar> }` — one bar per run of non-blank cells of one color; columns count
    characters (one per cell; a wide character's spacer is a blank, `snapshot.rs:277-282`).

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/templum-terminal/src/snapshot.rs`:

```rust
    fn bar(start: usize, len: usize, fg: Rgb, dim: bool) -> Bar {
        Bar {
            cols: Span { start, len },
            fg,
            dim,
        }
    }

    fn red() -> Rgb {
        palette::resolve(
            Color::Named(NamedColor::Red),
            &Colors::default(),
            &Palette::default(),
        )
    }

    #[test]
    fn row_bars_are_runs_of_non_blank_cells_of_one_color() {
        let fg = Palette::default().foreground;
        let s = snap(b"\x1b[31mred\x1b[0m ok");
        assert_eq!(s.rows[0].bars(), vec![bar(0, 3, red(), false), bar(4, 2, fg, false)]);
        // A colored run with a space inside: two bars.
        let s = snap(b"\x1b[31mgo on");
        assert_eq!(s.rows[0].bars(), vec![bar(0, 2, red(), false), bar(3, 2, red(), false)]);
        // Leading blanks have none; bold and italic don't cut a bar.
        let s = snap(b"   \x1b[1mab\x1b[0;3mcd");
        assert_eq!(s.rows[0].bars(), vec![bar(3, 4, fg, false)]);
        assert!(s.rows[1].bars().is_empty(), "an empty row");
    }

    #[test]
    fn row_bars_count_columns_in_characters() {
        let fg = Palette::default().foreground;
        // é is two bytes, one column.
        let s = snap("é ab".as_bytes());
        assert_eq!(s.rows[0].bars(), vec![bar(0, 1, fg, false), bar(2, 2, fg, false)]);
        // A wide character's bar is its first column: its spacer is a blank.
        let s = snap("你好x".as_bytes());
        assert_eq!(
            s.rows[0].bars(),
            vec![bar(0, 1, fg, false), bar(2, 1, fg, false), bar(4, 1, fg, false)]
        );
    }

    #[test]
    fn a_dim_run_stays_dim() {
        let fg = Palette::default().foreground;
        let s = snap(b"\x1b[2mdim\x1b[0mlit");
        assert_eq!(s.rows[0].bars(), vec![bar(0, 3, fg, true), bar(3, 3, fg, false)]);
    }
```

In `crates/templum-terminal/src/lib.rs`, line 21, export `Bar`:

```rust
pub use snapshot::{Bar, Cursor, Row, Run, Snapshot, Span};
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-terminal -- bars dim_run`
Expected: FAIL to compile (`cannot find type \`Bar\``, `no method named \`bars\``).

- [ ] **Step 3: The bars**

In `snapshot.rs`, after `pub struct Span { … }` (line 36):

```rust
/// Text drawn from afar: a run of non-blank cells of one color, columns `cols`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub cols: Span,
    pub fg: Rgb,
    pub dim: bool,
}
```

Inside `impl Row`, after `fn trim` (before the `impl`'s closing brace, line 106):

```rust
    /// The row's text as bars: one per run of non-blank cells of one color (bold and italic don't
    /// count), for a terminal seen from far away. One column per character: a wide character's
    /// spacer is a blank, so its bar is its first column.
    pub fn bars(&self) -> Vec<Bar> {
        let mut bars: Vec<Bar> = Vec::new();
        let mut chars = self.text.chars();
        let mut col = 0;
        for run in &self.runs {
            let mut left = run.len;
            while left > 0 {
                let Some(c) = chars.next() else {
                    return bars;
                };
                left = left.saturating_sub(c.len_utf8());
                if c != ' ' {
                    match bars.last_mut() {
                        Some(b)
                            if b.cols.start + b.cols.len == col
                                && b.fg == run.fg
                                && b.dim == run.dim =>
                        {
                            b.cols.len += 1
                        }
                        _ => bars.push(Bar {
                            cols: Span { start: col, len: 1 },
                            fg: run.fg,
                            dim: run.dim,
                        }),
                    }
                }
                col += 1;
            }
        }
        bars
    }
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum-terminal`
Expected: PASS (every snapshot test, the 3 new ones included).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum-terminal/src/snapshot.rs crates/templum-terminal/src/lib.rs && \
git commit -F - <<'EOF'
refactor(terminal): turn a row's text into bars of its colors

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---
### Task 6: Far terminals: bars, the card, the wheel and the find field

**Files:**
- Modify: `crates/templum/src/grid.rs:108-120` (after `cursor_quad`: `DIM`, `BAR_OPACITY`, `bar_quad`), `:122-164`
  (`body` takes `bars`), `:166-266` (`paint` paints bars), tests (`:313-348`)
- Modify: `crates/templum/src/block.rs` — the `Body::Terminal(pane) => frame` arm of the `content` match (lines
  335-367 at HEAD; Task 4 moved them down)
- Modify: `crates/templum/src/workspace/tests/terminals.rs:7` (`until`), `:776` (`ctrl_click_cell`), `:822`
  (`shown_in`): `fn` becomes `pub(super) fn`, so `tests/lod.rs` uses them
- Test: `crates/templum/src/workspace/tests/lod.rs`, `crates/templum/src/grid.rs`

**Interfaces:**
- Consumes: `Row::bars() -> Vec<Bar>`, `Bar { cols: Span, fg: Rgb, dim: bool }` (Task 5); `level` and `labeled` in
  `block::render` (Tasks 3, 4); test helpers `rest_at`, `unfocus`, `shown` (Task 3), `click`, `wheel`, `frame`,
  `first_id` (`tests/mod.rs`); `Workspace::{open_find, go_to_waiting, focus_block, add_note_at, set_dark}`.
- Produces:
  - `pub(crate) const BAR_OPACITY: f32 = 0.6;` and `pub(crate) fn bar_quad(cells: Bounds<Pixels>) -> Bounds<Pixels>`
    in `grid.rs` (Task 9 reuses both);
  - `grid::body(term, snapshot, metrics, zoom, bars: bool, focused, marked, theme, blink_on, ws)`;
  - a far terminal's grid wrapped in `("silhouette", n)`.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/templum/src/grid.rs`:

```rust
    #[test]
    fn a_bar_is_half_its_row_in_its_middle() {
        let cells = Bounds::new(point(px(10.), px(20.)), size(px(24.), px(8.)));
        assert_eq!(
            bar_quad(cells),
            Bounds::new(point(px(10.), px(22.)), size(px(24.), px(4.)))
        );
        // A row 1.5 px high still gets a 1 px bar, in its middle.
        let thin = Bounds::new(point(px(0.), px(0.)), size(px(3.), px(1.5)));
        assert_eq!(bar_quad(thin).size.height, px(1.));
        assert_eq!(bar_quad(thin).origin.y, px(0.25));
    }
```

In `crates/templum/src/workspace/tests/terminals.rs`, make three helpers visible to `tests/lod.rs`:
`fn until(` → `pub(super) fn until(` (line 7), `fn ctrl_click_cell(` → `pub(super) fn ctrl_click_cell(` (line 776),
`fn shown_in(` → `pub(super) fn shown_in(` (line 822).

Append to `crates/templum/src/workspace/tests/lod.rs`:

```rust
/// A focused terminal at 25% is Full: its letters, its cursor (the IME popup follows it) and the
/// keys. When the focus moves to another block, it is bars on the next frame.
#[cfg(unix)]
#[gpui_kit::test]
fn a_focused_far_terminal_draws_its_letters_and_the_one_left_bars(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    ws.update(cx, |ws, _| ws.ime_cursor = None);
    rest_at(handle, &ws, cx, Point::new(rect.x - 10.0, rect.y - 10.0), 0.25);
    assert!(!shown(handle, cx, ("silhouette", n)), "focused: its letters");
    ws.read_with(cx, |ws, _| assert!(ws.ime_cursor.is_some(), "its cursor is drawn"));
    cx.update_window(handle, |_, window, cx| window.input("z", cx))
        .unwrap();
    super::terminals::until("the key to show", handle, &ws, cx, |ws| {
        ws.space.terminal(id).unwrap().snapshot.borrow().text().contains('z')
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.add_note_at(Point::new(rect.x + rect.w + 100.0, rect.y), window, cx)
        })
    })
    .unwrap();
    frame(handle, cx);
    assert!(shown(handle, cx, ("silhouette", n)), "left: bars");
}

/// A far terminal's bars are its colored runs, painted on its grid's cells (the grid's area is
/// where the letters start); a theme switch while far gives them the new palette at once.
#[cfg(unix)]
#[gpui_kit::test]
fn a_far_terminals_bars_are_its_colored_runs_on_its_cells(cx: &mut TestAppContext) {
    use templum_terminal::{Bar, Span};
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    // `cat` prints the line back: "red" in red, then " ok".
    super::terminals::shown_in(&ws, cx, id, "\x1b[31mred\x1b[0m ok\r", "\nred ok");
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(handle, &ws, cx, Point::new(rect.x - 10.0, rect.y - 10.0), 0.25);
    let bars = |ws: &Workspace| {
        let snap = ws.space.terminal(id).unwrap().snapshot.borrow();
        let row = snap.rows.iter().rposition(|r| r.text == "red ok").expect("the row");
        snap.rows[row].bars()
    };
    ws.read_with(cx, |ws, _| {
        let p = &ws.theme.palette;
        assert_eq!(
            bars(ws),
            vec![
                Bar {
                    cols: Span { start: 0, len: 3 },
                    fg: p.ansi[1],
                    dim: false
                },
                Bar {
                    cols: Span { start: 4, len: 2 },
                    fg: p.foreground,
                    dim: false
                },
            ]
        );
    });
    cx.update_window(handle, |_, window, _| {
        let grid = window.find(("silhouette", n)).bounds();
        let body = window.find(("body", n)).bounds();
        let pad = px(chrome::PAD * 0.25);
        assert_eq!(grid.origin, point(body.origin.x + pad, body.origin.y + pad));
    })
    .unwrap();
    let red = ws.read_with(cx, |ws, _| ws.theme.palette.ansi[1]);
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        let now = ws.theme.palette.ansi[1];
        assert_ne!(now, red, "another theme");
        assert_eq!(bars(ws)[0].fg, now, "the bars take its red at once");
    });
}

/// Under 2.5 px (8-point text at 25%) a terminal is its card: its grid isn't painted, so it isn't
/// re-armed, and its output wakes the UI once and then waits, as off screen.
#[cfg(unix)]
#[gpui_kit::test]
fn a_terminal_drawn_as_its_card_isnt_rearmed(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    // 8 × 0.25 = 2 px: the Card level. The grid keeps its cells; only the level changes.
    ws.update(cx, |ws, _| ws.cell.font_size = 8.0);
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(handle, &ws, cx, Point::new(rect.x - 10.0, rect.y - 10.0), 0.25);
    assert!(shown(handle, cx, ("block", n)), "on screen");
    assert!(!shown(handle, cx, ("silhouette", n)), "no bars: its card");
    let armed = |ws: &Workspace| ws.space.terminal(id).unwrap().term.shared().armed();
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(id).unwrap().term.write(b"x\r".to_vec())
    });
    super::terminals::until("its output to wake the UI once", handle, &ws, cx, |ws| {
        !armed(ws)
    });
    for _ in 0..3 {
        frame(handle, cx);
    }
    ws.read_with(cx, |ws, _| assert!(!armed(ws), "a card isn't re-armed"));
}

/// Over a far terminal the wheel moves the canvas and Ctrl+wheel zooms; once it is clicked
/// (focused: Full), the wheel is the terminal's again.
#[gpui_kit::test]
fn the_wheel_over_a_far_terminal_moves_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    let corner = Point::new(rect.x - 10.0, rect.y - 10.0);
    rest_at(handle, &ws, cx, corner, 0.25);
    let c = cx
        .update_window(handle, |_, window, _| {
            window.find(("silhouette", n)).bounds().center()
        })
        .unwrap();
    let at = (f32::from(c.x), f32::from(c.y));
    let camera = |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    let down = ScrollDelta::Pixels(point(px(0.0), px(40.0)));
    let before = camera(cx);
    cx.update_window(handle, |_, window, cx| wheel(window, cx, at, down, false))
        .unwrap();
    assert_ne!(camera(cx).origin, before.origin, "the canvas moved");
    rest_at(handle, &ws, cx, corner, 0.25);
    cx.update_window(handle, |_, window, cx| {
        wheel(window, cx, at, ScrollDelta::Lines(point(0.0, 1.0)), true)
    })
    .unwrap();
    assert_eq!(camera(cx).zoom(), 0.33, "Ctrl+wheel zooms");
    rest_at(handle, &ws, cx, corner, 0.25);
    cx.update_window(handle, |_, window, cx| {
        click(window, cx, at, 1);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(!shown(handle, cx, ("silhouette", n)), "clicked: Full");
    let before = camera(cx);
    cx.update_window(handle, |_, window, cx| wheel(window, cx, at, down, false))
        .unwrap();
    assert_eq!(camera(cx), before, "the wheel is the terminal's");
}

/// Ctrl+click on a URL in an unfocused far terminal opens it: its bars keep the press, which
/// reads the grid. The URL is printed below the rows the label covers.
#[cfg(unix)]
#[gpui_kit::test]
fn ctrl_click_on_a_url_in_a_far_terminal_opens_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    super::terminals::shown_in(
        &ws,
        cx,
        id,
        "\r\r\r\r\r\r\r\r\r\rsee https://example.com/x\r",
        "x\nsee https://example.com/x",
    );
    unfocus(handle, &ws, cx);
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(handle, &ws, cx, Point::new(rect.x - 10.0, rect.y - 10.0), 0.25);
    assert!(shown(handle, cx, ("silhouette", id.0 as usize)));
    let row = ws.read_with(cx, |ws, _| {
        let snap = ws.space.terminal(id).unwrap().snapshot.borrow();
        snap.rows
            .iter()
            .rposition(|r| r.text.starts_with("see https"))
            .unwrap()
    });
    super::terminals::ctrl_click_cell(handle, &ws, cx, id, (row, 8));
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/x"));
}

/// A find field left open on a terminal that lost the focus at 33% is hidden, and comes back with
/// its text when the terminal is focused again.
#[gpui_kit::test]
fn a_find_field_hides_far_away_and_comes_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(handle, &ws, cx, Point::new(rect.x - 10.0, rect.y - 10.0), 0.33);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_find(window, cx));
        window.render_frame(cx);
        window.input("needle", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, ("find", n)), "focused: its field");
    unfocus(handle, &ws, cx);
    assert!(!shown(handle, cx, ("find", n)), "far and left: hidden");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, ("find", n)), "back");
    ws.read_with(cx, |ws, cx| {
        let bar = ws.space.terminal(id).unwrap().find.as_ref().unwrap();
        assert_eq!(bar.input.read(cx).value().as_ref(), "needle");
    });
}

/// "Go to the terminal waiting for you" focuses a far waiting terminal: it is drawn Full.
#[gpui_kit::test]
fn going_to_a_far_waiting_terminal_draws_it_in_full(cx: &mut TestAppContext) {
    use crate::alert::Alert;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let n = id.0 as usize;
    unfocus(handle, &ws, cx);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().alert = Some(Alert::Attention(None))
    });
    let rect = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    rest_at(handle, &ws, cx, Point::new(rect.x - 10.0, rect.y - 10.0), 0.25);
    assert!(shown(handle, cx, ("silhouette", n)), "waiting, far: bars");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.go_to_waiting(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert!(!shown(handle, cx, ("silhouette", n)), "gone to: Full");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum --bin templum -- grid::tests lod::`
Expected: FAIL to compile first (`cannot find function \`bar_quad\``). With Step 3's `bar_quad` alone in place, the
lod tests fail as written: no `("silhouette", n)` element (`missing ElementId` panics or `left: bars` fails),
`a_terminal_drawn_as_its_card_isnt_rearmed` times out on "waited for its output to wake the UI once" (a painted
grid re-arms at once), the wheel test panics on the missing silhouette, the find test fails on "far and left:
hidden". `ctrl_click_on_a_url_in_a_far_terminal_opens_it` and `going_to_a_far_waiting_terminal_draws_it_in_full`
fail on their `silhouette` assertion only; past it they pin what must keep working (the press handler kept on the
bars, the focus rule for `reveal`).

- [ ] **Step 3: Bars in the grid**

In `crates/templum/src/grid.rs`, after `cursor_quad` (ends at line 120):

```rust
/// A dim run's opacity, letters and bars alike.
const DIM: f32 = 0.66;
/// Bars are drawn at this share of their text's opacity: a bar covers its cells solidly where
/// letters cover about half of them, so at full strength bars would read heavier than the text.
pub(crate) const BAR_OPACITY: f32 = 0.6;

/// A bar on `cells`: as wide as they are, half their height (at least 1 px; GPUI snaps a
/// quad's edges to device pixels), in their middle.
pub(crate) fn bar_quad(cells: Bounds<Pixels>) -> Bounds<Pixels> {
    let row = f32::from(cells.size.height);
    let h = (row / 2.0).max(1.0);
    Bounds::new(
        point(cells.origin.x, cells.origin.y + px((row - h) / 2.0)),
        size(cells.size.width, px(h)),
    )
}
```

Replace `body`'s doc comment and signature (lines 122-135) with:

```rust
/// The grid of one terminal: refreshes its snapshot (never blocking) and paints it; with `bars`
/// (far away) its text is bars of its colors, not shaped letters. The focused one also reports
/// its cursor to the workspace, for the IME popup.
#[allow(clippy::too_many_arguments)]
pub(crate) fn body(
    term: SharedTerm,
    snapshot: Rc<RefCell<Snapshot>>,
    metrics: CellMetrics,
    zoom: f32,
    bars: bool,
    focused: bool,
    marked: Option<String>,
    theme: Rc<Theme>,
    blink_on: bool,
    ws: Entity<Workspace>,
) -> impl IntoElement {
```

and its call to `paint` (lines 139-150) with:

```rust
            let cursor = paint(
                &term,
                &snapshot,
                bounds.origin,
                zoom,
                bars,
                &metrics,
                focused,
                blink_on,
                &theme,
                window,
                cx,
            );
```

In `paint`, add the parameter after `z: f32,` (line 172):

```rust
    bars: bool,
```

then, after the selection loop (`for span in &row.selected { … }`, ends at line 213) and before
`if row.text.is_empty() {`:

```rust
        if bars {
            // Far away: a bar per run of non-blank cells of one color, on the letters' cells. No
            // text is shaped, no glyph looked up.
            for bar in row.bars() {
                let alpha = BAR_OPACITY * if bar.dim { DIM } else { 1.0 };
                window.paint_quad(fill(
                    bar_quad(cell(i, bar.cols.start, bar.cols.len)),
                    hsla(bar.fg, alpha),
                ));
            }
            continue;
        }
```

and in the `TextRun` (line 235) use the constant:

```rust
                color: hsla(r.fg, if r.dim { DIM } else { 1.0 }),
```

- [ ] **Step 4: A far terminal's body**

In `crates/templum/src/block.rs`, replace the `Body::Terminal(pane) => frame … ,` arm of the `content` match (from
`Body::Terminal(pane) => frame` through the `.when_some(pane.find…)` call and its comma) with:

```rust
        Body::Terminal(pane) => frame
            // At the Card level nothing is inside: the grid isn't painted, so the terminal isn't
            // re-armed and its output asks for no frames, as off screen.
            .when(live && level != Level::Card, |el| {
                el.cursor_text().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        // Ctrl+click (Cmd on macOS) opens a path or a link; the block's own
                        // press would take the keyboard back from the editor it opened.
                        if e.modifiers.secondary() && ws.open_link(id, e, window, cx) {
                            cx.stop_propagation();
                        } else {
                            ws.begin_select(id, e);
                        }
                    }),
                )
            })
            // Over bars the wheel moves the canvas: a far terminal's history stays still.
            .when(live && level == Level::Full, |el| {
                el.on_scroll_wheel(cx.listener(move |ws, e: &ScrollWheelEvent, _, cx| {
                    ws.scroll_terminal(id, e, cx)
                }))
            })
            .when(level != Level::Card, |el| {
                let grid = grid::body(
                    pane.term.shared().clone(),
                    pane.snapshot.clone(),
                    ws.cell.clone(),
                    z,
                    level == Level::Silhouette,
                    focused,
                    ws.marked.clone().filter(|_| focused),
                    ws.theme.clone(),
                    ws.blink_on,
                    cx.entity(),
                );
                if level == Level::Full {
                    el.child(grid)
                } else {
                    el.child(
                        div()
                            .id(("silhouette", n))
                            .size_full()
                            .child(grid)
                            .test_support(),
                    )
                }
            })
            // Its find field is kept while the block is far, and comes back with its letters.
            .when_some(
                pane.find.as_ref().filter(|_| live && level == Level::Full),
                |el, bar| el.child(find_bar(ws, n, bar, z)),
            ),
```

- [ ] **Step 5: Run them to see them pass**

Run: `cargo test -p templum --bin templum -- grid::tests lod::`
Expected: PASS. Then `cargo test -p templum --bin templum terminals` — every terminal test still passes (they run
at 100%, or focus their terminal).

- [ ] **Step 6: Check by eye**

`cargo run -p templum`; in one terminal `ls --color=always -la /usr/bin | head -40`, in another `htop` (or
`claude` if installed), focus a third block, zoom to 33% and 25%: the far terminals are bars in their text's colors,
on the text's rows, a selection still shows; moving the wheel over them pans the canvas; zoom back to 50%: the
letters come back exactly where the bars were (no jump).

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum/src/grid.rs crates/templum/src/block.rs \
  crates/templum/src/workspace/tests/terminals.rs crates/templum/src/workspace/tests/lod.rs && \
git commit -F - <<'EOF'
perf: draw far terminals as bars of their text's colors

At 33% and 25% a terminal you aren't working in paints each run of colored
text as a bar on its own cells instead of shaping its letters; the wheel over
it moves the canvas. Under 2.5 px its text isn't drawn at all.

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 7: Rendered Markdown left out far away; kanban drawn in full

**Files:**
- Modify: `crates/templum/src/note.rs:15` (import `Level`), `:104-117` (`body` takes `level`; early return)
- Modify: `crates/templum/src/editor.rs:23` (import `Level`), `:632-656` (`body` takes `level`), `:828-834` (`files`
  takes `level`), `:930-939` (the preview)
- Modify: `crates/templum/src/block.rs` — the `Body::Note` and `Body::Editor` arms of the `content` match pass
  `level`
- Test: `crates/templum/src/workspace/tests/lod.rs`

**Interfaces:**
- Consumes: `level` in `block::render` (Task 3); test helpers `open_editor`, `click_file` (`tests/mod.rs`),
  `rest_at`, `unfocus`, `shown` (Task 3); `Workspace::{toggle_preview, toggle_file_preview, add_kanban_at}`.
- Produces: `note::body(ws: &Workspace, id: BlockId, note: &Note, z: f32, level: Level, cx: &mut Context<Workspace>) -> AnyElement`
  and `editor::body(ws: &Workspace, id: BlockId, editor: &Editor, z: f32, level: Level, cx: &mut Context<Workspace>) -> AnyElement`
  (Tasks 9 and 10 build on them); `fn files(ws, id, editor, z, level: Level, cx) -> AnyElement` (editor.rs, private).

- [ ] **Step 1: Write the failing test**

Append to `crates/templum/src/workspace/tests/lod.rs`:

```rust
/// Below Full, rendered Markdown is an empty area: a note shown rendered is its tinted card, an
/// editor's Markdown file keeps its tab row. A kanban block is drawn in full there.
#[gpui_kit::test]
fn far_rendered_markdown_is_left_out_and_kanban_drawn_in_full(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "# Hello\n\nSome *words*.\n").unwrap();
    let (handle, ws) = open(cx);
    let editor = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    let (note, kanban) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.toggle_file_preview(editor, window, cx);
                ws.add_note_at(Point::new(40.0, 900.0), window, cx);
                let note = ws.space.canvas.top().unwrap();
                ws.toggle_preview(note, window, cx);
                ws.add_kanban_at(Point::new(1200.0, 40.0), window, cx);
                let kanban = ws.space.canvas.top().unwrap();
                ws.space.appearing.clear();
                (note.0 as usize, kanban.0 as usize)
            })
        })
        .unwrap();
    let e = editor.0 as usize;
    unfocus(handle, &ws, cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.5);
    assert!(shown(handle, cx, ("note-preview", note)), "the note rendered at 50%");
    assert!(shown(handle, cx, ("file-preview", e)), "the file rendered at 50%");
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 0.33);
    assert!(!shown(handle, cx, ("note-preview", note)), "the note is its card");
    assert!(!shown(handle, cx, ("file-preview", e)), "the file's area is empty");
    assert!(
        shown(handle, cx, ElementId::Name(format!("tab-{e}-0").into())),
        "the tab row stays"
    );
    assert!(shown(handle, cx, ("board-list", kanban)), "the kanban block is drawn in full");
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum --bin templum far_rendered_markdown`
Expected: FAIL on "the note is its card".

- [ ] **Step 3: The note**

In `crates/templum/src/note.rs`, line 15, import `Level`:

```rust
use templum_canvas::{BlockId, Level, Size, chrome};
```

Replace `body`'s doc comment, signature and first line (lines 104-112) with:

```rust
/// The body below the header: the editor, or the rendered text. Both at the block's zoom `z`.
/// Below Full rendered text is left out.
pub(crate) fn body(
    ws: &Workspace,
    id: BlockId,
    note: &Note,
    z: f32,
    level: Level,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = id.0 as usize;
    // Below Full, rendered Markdown is left out: gpui-kit's Markdown view has no layout to draw
    // bars from, and the tinted card, empty, keeps everything in place.
    if note.preview && level != Level::Full {
        return div().size_full().into_any_element();
    }
```

- [ ] **Step 4: The editor**

In `crates/templum/src/editor.rs`, line 23, import `Level`:

```rust
use templum_canvas::{BlockId, Level, Rect, Size, chrome};
```

`body` (line 632) takes the level after `z: f32,`:

```rust
    level: Level,
```

and passes it on: `.child(files(ws, id, editor, z, level, cx))`. `files` (line 828) takes it after `z: f32,` the same
way. In `files`, replace the start of the preview's `let view` (lines 930-939, `let view: AnyElement = if tab.preview {
crate::note::preview( … ) } else {`) with:

```rust
                let view: AnyElement = if tab.preview && level != Level::Full {
                    // Below Full, rendered Markdown is an empty area: no layout to draw bars
                    // from, and everything around it stays in place.
                    div().into_any_element()
                } else if tab.preview {
                    crate::note::preview(
                        ("file-preview", n).into(),
                        ("file-markdown", n).into(),
                        tab.editor.read(cx).value(),
                        &tab.preview_scroll,
                        preview_w,
                        FONT,
                        z,
                    )
                } else {
```

(the `else` branch, the code editor, is unchanged).

In `crates/templum/src/block.rs`, the note and editor arms pass the level:
`note::body(ws, id, n, settled, level, cx)` and `editor::body(ws, id, e, settled, level, cx)`.

- [ ] **Step 5: Run it to see it pass**

Run: `cargo test -p templum --bin templum far_rendered_markdown`
Expected: PASS. Then `cargo test -p templum --bin templum -- notes editor` — the note and editor tests still pass.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum/src/note.rs crates/templum/src/editor.rs crates/templum/src/block.rs \
  crates/templum/src/workspace/tests/lod.rs && \
git commit -F - <<'EOF'
perf: leave rendered Markdown out of notes and files seen from far away

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 8: CHECKPOINT — measure Part 1, report to the maintainer, and STOP

No code. Part 2 starts only after the maintainer approves.

**Files:**
- Modify: this plan's "Measurements → Checkpoint" section
- Possibly: `docs/minors.md`, if the run turns up a minor left for later

- [ ] **Step 1: The full check**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Expected: green. Report it as it is, failures included.

- [ ] **Step 2: Benches, alternating with the "before" binary**

```bash
uptime   # low load, no other build running; otherwise wait
cd ~/Documents/github/Templum && cargo build --release -p templum
cp target/release/templum ~/Documents/github/Templum/target/lod/part1
S=~/Documents/github/Templum/target/lod/alternate.sh B=~/Documents/github/Templum/target/lod/before A=~/Documents/github/Templum/target/lod/part1
DQ=~/Documents/github/DaemonQuest T=~/Documents/github/Templum
$S 3 $DQ --bench-far $B $A
$S 3 $T --bench $B $A
$S 3 $T --bench-mixed $B $A
for k in empty terminal editor note kanban git; do $S 3 $DQ --bench-zoom=$k $B $A; done
$S 3 $T --bench-git $B $A
```

Expected: `--bench-far` p95 down (its terminals and Git blocks no longer shape or build; its editors and notes are
still Full until Part 2, so it may still be over 16.7: Part 2 brings it under); `--bench-zoom=terminal` and
`=git` lower at the 25%–33% end of their sweeps; the others within run-to-run noise (~1 ms on p95). Paste the
medians and the JSON lines into "Measurements → Checkpoint". If `--bench-far` with kanban blocks stands out (compare
with `--bench-zoom=kanban`), say so in the report: the spec's Decisions 2 turns kanban blocks into cards at the
Silhouette level then.

- [ ] **Step 3: Commit the numbers**

```bash
git add docs/plans/2026-10-13-level-of-detail.md && \
git commit -F - <<'EOF'
docs: record the far-zoom benches before and after its first part

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

- [ ] **Step 4: Report to the maintainer, in Brazilian Portuguese, and stop**

Fill in the numbers and send this (short; the maintainer reads it as is):

```text
**Nível de detalhe, parte 1 pronta — preciso do seu OK antes da parte 2.**

O que mudou a 33% e 25%: terminais sem foco viram barras nas cores do texto, nas mesmas linhas e
colunas das letras; um bloco Git sem foco vira o cartão; todo título vira uma etiqueta de 12 px no
canto do bloco, com o ponto de status de 8 px nos terminais; a roda do mouse sobre um bloco distante
move o canvas. O bloco em que você trabalha (focado, ou com o teclado num campo dele) fica sempre
completo. O nível só muda quando a câmera para, nunca no meio do zoom. Editores e notas ainda são
desenhados completos (só o Markdown renderizado fica vazio): a parte 2 faz as barras deles.

Benches (mediana de 3 rodadas alternadas, antes → depois, p95 / max em ms):
- --bench-far: A / A → B / B
- --bench: … ; --bench-mixed: …
- --bench-zoom=empty/terminal/editor/note/kanban/git: …
- --bench-git: …
O que olhar: <a linha que mais mudou e por quê; se o kanban pesa no --bench-far>.
Check completo: <verde | o que falhou>.

Por favor confira à mão no Hyprland (cargo run --release -p templum):
1. Um cockpit de verdade (Claude Code em 2 ou 3 terminais, um editor, uma nota, um bloco Git) a 33%
   e a 25%: as barras parecem o texto e as cores se leem (vermelho de erro, verde de sucesso).
2. Etiquetas e pontos de status legíveis a 25%.
3. Nada pisca durante o zoom, e nada se move quando o zoom termina ou quando você volta para 50%
   (dá para gravar a tela de 50% ↔ 33% e ver quadro a quadro).
4. Digitar num terminal focado a 25% funciona, com teclas mortas (á, ç) e o fcitx5.
5. A roda do mouse sobre um bloco distante move o canvas; depois de clicar nele, rola o bloco.
6. Tema claro e escuro.

Se quiser mudanças visuais (altura ou opacidade das barras, tamanho da etiqueta, o ponto), eu
corrijo, com testes, antes da parte 2. Posso seguir para a parte 2?
```

Then STOP. Do not start Task 9 until the maintainer says so. If the maintainer asks for visual changes, make each
one as a small task in this plan's style (a failing test where the change is logic — a size, a level, a color —
then the change, then the check and a commit on `development`), re-run the benches it touches, report again, and
wait for the approval.

---

# Part 2 — editors and notes

### Task 9: Far notes: the word map

**Files:**
- Create: `crates/templum/src/silhouette.rs`
- Modify: `crates/templum/src/main.rs:25` (`mod silhouette;` after `mod sidebar;`)
- Modify: `crates/templum/src/note.rs:4` (imports), `:17-19` (imports), `:31-34` (`FIXED_LEFT`, `FIXED_TOP` become
  `pub(crate)`), `:37-61` (`Note::far`), `body` (`:104-209` at HEAD, with Task 7's lines): rewritten, with two new
  helpers `taken` and `editor_box`
- Test: `crates/templum/src/silhouette.rs` (unit), `crates/templum/src/workspace/tests/lod.rs`

**Interfaces:**
- Consumes: `EditorState::{line_height, text_bounds, visible_row_range, scroll_offset, range_to_bounds, text}`
  (gpui-base-0.7.1 `src/input/base/state.rs:2992-3013`, `:575-577`, `:3635-3658`, `:1287-1289`; `text()` is a
  `ropey::Rope`, whose `len()` is O(1)); `apply_highlighter_fold_candidates` (`state.rs:877`, test);
  `grid::{bar_quad, BAR_OPACITY, hsla}` (Task 6); `editor::keep_rows`; `note::body(…, level, cx)` (Task 7); test
  helpers `rest_at`, `unfocus`, `shown` (Task 3), `click`, `wheel`, `frame`.
- Produces (`crate::silhouette`):
  - `pub(crate) struct Taken { pub len: usize, pub size: (f32, f32), pub font: SharedString, pub wrap: bool, pub tree: bool }`
    (`Clone, Debug, PartialEq`)
  - `pub(crate) struct Word { pub range: Range<usize>, pub row: usize, pub x: f32, pub w: f32 }`
  - `pub(crate) struct WordMap { pub taken: Taken, pub scroll: (f32, f32), pub words: Vec<Word> }` with
    `pub fn take(state: &EditorState, z: f32, taken: Taken) -> Option<WordMap>` and
    `pub fn cells(&self, origin: (f32, f32), font: f32, s: f32) -> impl Iterator<Item = (f32, f32, f32, f32)> + '_`
  - `pub(crate) fn note_origin(_: &WordMap, s: f32) -> (f32, f32)`
  - `#[derive(Default)] pub(crate) struct Far { full: Option<(f32, Taken)>, pub(crate) map: Option<Rc<WordMap>> }`
    with `pub(crate) fn drawn_full(&mut self, z: f32, taken: Taken)`
  - `pub(crate) fn body(id: ElementId, editor: &Entity<EditorState>, far: &RefCell<Far>, taken: Taken, z: f32, font: f32, origin: fn(&WordMap, f32) -> (f32, f32), color: Hsla, full: impl FnOnce(&mut App) -> AnyElement, cx: &mut App) -> AnyElement`
  - `Note { pub far: RefCell<Far>, … }`; `note::FIXED_LEFT`, `note::FIXED_TOP` are `pub(crate)`.
  - test helpers in `tests/lod.rs`: `fn note_with(handle, ws, cx, at: Point, text: &str) -> (BlockId, Entity<EditorState>)`,
    `fn note_map(ws, cx, id: BlockId) -> Option<Rc<WordMap>>`.

How it works (spec, section 3): on the first frame a note is drawn below Full, the map is taken from gpui-kit's last
layout — each word's row and its span across, in world units — and drawn at the body's settled zoom with gpui-kit's
own row pitch. When the map is stale or missing, the editor is drawn once more, unseen (opacity 0, which GPUI still
lays out and paints, `gpui-pre-0.3.8 src/window.rs:4078-4094`), and the next frame takes the map.

- [ ] **Step 1: Write the failing unit tests**

Create `crates/templum/src/silhouette.rs` with its tests (the code comes in Step 4, above them):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn map(words: Vec<Word>, scroll: (f32, f32)) -> WordMap {
        WordMap {
            taken: Taken {
                len: 0,
                size: (0.0, 0.0),
                font: "".into(),
                wrap: false,
                tree: false,
            },
            scroll,
            words,
        }
    }

    fn word(row: usize, x: f32, w: f32) -> Word {
        Word {
            range: 0..1,
            row,
            x,
            w,
        }
    }

    fn close(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
        [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3)]
            .iter()
            .all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn rows_are_gpui_kits_rounded_line_heights() {
        let m = map(vec![word(0, 0.0, 10.0), word(3, 20.0, 8.0)], (0.0, 0.0));
        let cells: Vec<_> = m.cells((0.0, 0.0), 13.0, 0.33).collect();
        // 1.5 × 13 × 0.33 = 6.435: rows 6 px apart; words scale with the zoom.
        assert!(close(cells[0], (0.0, 0.0, 3.3, 6.0)), "{:?}", cells[0]);
        assert!(close(cells[1], (6.6, 18.0, 2.64, 6.0)), "{:?}", cells[1]);
    }

    #[test]
    fn the_scroll_moves_every_row_by_its_share_of_a_row() {
        // Taken scrolled 2.5 rows down and 1 row across. At 100%, rows of 19.5 → 20 px:
        let m = map(vec![word(3, 10.0, 4.0)], (2.5, 1.0));
        let cell = m.cells((5.0, 7.0), 13.0, 1.0).next().unwrap();
        assert!(close(cell, (5.0 + 10.0 - 20.0, 7.0 + 0.5 * 20.0, 4.0, 20.0)), "{cell:?}");
        // At 25%, rows of 4.875 → 5 px: the same share of a row above the top.
        let cell = m.cells((0.0, 0.0), 13.0, 0.25).next().unwrap();
        assert!(close(cell, (2.5 - 5.0, 0.5 * 5.0, 1.0, 5.0)), "{cell:?}");
    }

    #[test]
    fn a_notes_text_starts_where_its_box_puts_it_at_every_zoom() {
        let m = map(Vec::new(), (0.0, 0.0));
        assert_eq!(note_origin(&m, 1.0), (30.0, 8.0));
        assert_eq!(note_origin(&m, 0.25), (7.5, 2.0));
    }

    #[test]
    fn words_are_runs_of_non_blank_characters() {
        assert_eq!(runs("  let x = 1;"), vec![2..5, 6..7, 8..9, 10..12]);
        assert_eq!(runs("é\tab"), vec![0..2, 3..5]);
        assert_eq!(runs("a\u{3000}b"), vec![0..1, 4..5], "an ideographic space is blank");
        assert!(runs("   ").is_empty());
    }
}
```

and add `mod silhouette;` to `crates/templum/src/main.rs` after `mod sidebar;`.

- [ ] **Step 2: Write the failing headless tests**

In `crates/templum/src/workspace/tests/lod.rs`, add to the imports at the top:

```rust
use std::rc::Rc;

use gpui_kit::component::input::EditorState;

use crate::silhouette::WordMap;
```

and append:

```rust
/// A note at `at` holding `text`, at rest (not popping in), focused; and its editor.
fn note_with(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    at: Point,
    text: &str,
) -> (BlockId, Entity<EditorState>) {
    cx.update_window(handle, |_, window, cx| {
        let (id, editor) = ws.update(cx, |ws, cx| {
            ws.add_note_at(at, window, cx);
            ws.space.appearing.clear();
            let id = ws.space.canvas.top().unwrap();
            (id, ws.space.blocks[&id].note().unwrap().editor.clone())
        });
        editor.update(cx, |e, cx| e.set_value(text.to_string(), window, cx));
        window.render_frame(cx);
        (id, editor)
    })
    .unwrap()
}

fn note_map(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> Option<Rc<WordMap>> {
    ws.read_with(cx, |ws, _| {
        ws.space.blocks[&id].note().unwrap().far.borrow().map.clone()
    })
}

/// An unfocused note at 33% builds no gpui-kit editor: it paints its words as bars, in its text's
/// own box. At 50% it is its editor again.
#[gpui_kit::test]
fn a_far_note_draws_bars_in_its_box_and_at_fifty_its_editor(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (id, _) = note_with(handle, &ws, cx, Point::new(40.0, 40.0), "# Plan\nwrite the bars\n");
    let n = id.0 as usize;
    let corner = Point::new(30.0, 30.0);
    rest_at(handle, &ws, cx, corner, 0.33);
    let full = cx
        .update_window(handle, |_, window, _| window.find(("note-editor", n)).bounds())
        .unwrap();
    unfocus(handle, &ws, cx);
    frame(handle, cx);
    assert!(!shown(handle, cx, ("note-text", n)), "no gpui-kit editor");
    let bars = cx
        .update_window(handle, |_, window, _| window.find(("silhouette", n)).bounds())
        .unwrap();
    assert_eq!(bars, full, "the bars fill the editor's box");
    let map = note_map(&ws, cx, id).expect("a word map");
    assert_eq!(map.words.len(), 5, "#, Plan, write, the, bars");
    rest_at(handle, &ws, cx, corner, 0.5);
    assert!(shown(handle, cx, ("note-text", n)), "Full at 50%");
}

/// Bars line up with letters at every zoom step, within 1 px: a soft-wrapped note scrolled a row
/// and a half, with a folded section on screen, its word map taken at 50% (as after a zoom out
/// from there), against its letters laid out at each step.
#[gpui_kit::test]
fn a_notes_bars_line_up_with_its_letters_at_every_zoom(cx: &mut TestAppContext) {
    use crate::silhouette::{Taken, note_origin};
    use gpui_kit::component::input::FoldRange;
    let mut text = String::from("# Title\nintro\n## Folded\n");
    for i in 0..5 {
        text.push_str(&format!("hidden words {i}\n"));
    }
    text.push_str("## After\n");
    for i in 0..60 {
        text.push_str(&format!(
            "{i:02} the quick brown fox jumps over the lazy dog and keeps on running far\n"
        ));
    }
    let (handle, ws) = open(cx);
    let (id, note) = note_with(handle, &ws, cx, Point::new(40.0, 40.0), &text);
    let n = id.0 as usize;
    let corner = Point::new(30.0, 30.0);
    rest_at(handle, &ws, cx, corner, 1.0);
    // Folds "## Folded" (line 2; lines 3 to 7 hidden) from its gutter icon, drawn on the cursor's
    // line: 14 px, 2 px into the gutter (gpui-base-0.7.1 src/input/base/element.rs:1334-1348).
    let line2 = "# Title\nintro\n".len();
    cx.update_window(handle, |_, window, cx| {
        note.update(cx, |e, cx| {
            e.apply_highlighter_fold_candidates(vec![FoldRange::new(2, 8)], cx);
            e.set_selected_range(line2..line2, cx);
        });
        window.render_frame(cx);
        let e = note.read(cx);
        let (tb, lh) = (e.text_bounds().unwrap(), f32::from(e.line_height().unwrap()));
        let icon = (f32::from(tb.origin.x) + 9.0, f32::from(tb.origin.y) + 2.5 * lh);
        click(window, cx, icon, 1);
        window.render_frame(cx);
    })
    .unwrap();
    let (hidden, after) = (text.find("hidden").unwrap(), text.find("## After").unwrap());
    cx.update_window(handle, |_, _, cx| {
        let b = note.read(cx).range_to_bounds(&(hidden..hidden + 6)).unwrap();
        assert_eq!(b.size.width, px(0.0), "line 3 is folded away");
    })
    .unwrap();
    // A row and a half down: the fold stays on screen.
    cx.update_window(handle, |_, window, cx| {
        note.update(cx, |e, cx| {
            let row = e.line_height().unwrap();
            e.set_scroll_offset(point(px(0.0), -row * 1.5), cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    rest_at(handle, &ws, cx, corner, 0.5);
    let map = cx
        .update_window(handle, |_, _, cx| {
            let taken = Taken {
                len: text.len(),
                size: (0.0, 0.0),
                font: "".into(),
                wrap: true,
                tree: false,
            };
            WordMap::take(note.read(cx), 0.5, taken).expect("laid out")
        })
        .unwrap();
    assert!((map.scroll.0 - 1.5).abs() < 0.01, "taken scrolled: {:?}", map.scroll);
    assert!(
        map.words.iter().all(|w| !(hidden..after).contains(&w.range.start)),
        "no bars for the folded lines"
    );
    for zoom in templum_canvas::ZOOM_STEPS {
        rest_at(handle, &ws, cx, corner, zoom);
        cx.update_window(handle, |_, window, cx| {
            let origin = window.find(("note-editor", n)).bounds().origin;
            let e = note.read(cx);
            let lh = f32::from(e.line_height().unwrap());
            let cells = map.cells(note_origin(&map, zoom), crate::note::FONT, zoom);
            let mut compared = 0;
            for (word, (x, y, w, h)) in map.words.iter().zip(cells) {
                let Some(b) = e.range_to_bounds(&word.range) else {
                    continue;
                };
                if f32::from(b.size.height) > 1.5 * lh || b.size.width <= px(0.0) {
                    continue;
                }
                let (x, y) = (f32::from(origin.x) + x, f32::from(origin.y) + y);
                for (what, bar, letters) in [
                    ("left", x, f32::from(b.origin.x)),
                    ("right", x + w, f32::from(b.origin.x + b.size.width)),
                    ("top", y, f32::from(b.origin.y)),
                    ("height", h, f32::from(b.size.height)),
                ] {
                    assert!(
                        (bar - letters).abs() <= 1.0,
                        "{what} of {:?} at {zoom}: bar {bar}, letters {letters}",
                        &text[word.range.clone()]
                    );
                }
                compared += 1;
            }
            assert!(compared > 30, "{compared} words compared at {zoom}");
        })
        .unwrap();
    }
}

/// A note with no word map yet (a canvas restored at 25%: never drawn Full) is laid out unseen on
/// its first frame, and its next frame takes its word map.
#[gpui_kit::test]
fn a_far_note_with_no_word_map_yet_gets_one_after_its_first_frames(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.add_note_at(Point::new(40.0, 40.0), window, cx);
                let id = ws.space.canvas.top().unwrap();
                let editor = ws.space.blocks[&id].note().unwrap().editor.clone();
                editor.update(cx, |e, cx| {
                    e.set_value("# Plan\nwrite the bars\nthen the label\n".to_string(), window, cx)
                });
                ws.space.appearing.clear();
                ws.space.focused = None;
                ws.space.pristine = false;
                ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), 0.25);
                window.focus(&ws.focus_handle, cx);
                id
            })
        })
        .unwrap();
    frame(handle, cx);
    assert!(shown(handle, cx, ("silhouette", id.0 as usize)));
    assert!(note_map(&ws, cx, id).is_none(), "the first frame lays its editor out, unseen");
    frame(handle, cx);
    let map = note_map(&ws, cx, id).expect("the next frame takes its word map");
    assert_eq!(map.words.len(), 8, "#, Plan, write, the, bars, then, the, label");
}

/// A far note resized (a command, the keyboard elsewhere) gets its word map taken again: one
/// unseen Full frame lays it out at its new width, the next takes it.
#[gpui_kit::test]
fn a_far_notes_word_map_is_taken_again_after_a_resize(cx: &mut TestAppContext) {
    let text: String = (0..20)
        .map(|i| format!("{i} the quick brown fox jumps over the lazy dog\n"))
        .collect();
    let (handle, ws) = open(cx);
    let (id, _) = note_with(handle, &ws, cx, Point::new(40.0, 40.0), &text);
    rest_at(handle, &ws, cx, Point::new(30.0, 30.0), 0.33);
    unfocus(handle, &ws, cx);
    let dog = text.find("dog").unwrap();
    let row = |m: &WordMap| m.words.iter().find(|w| w.range.start == dog).unwrap().row;
    let before = note_map(&ws, cx, id).expect("taken");
    assert_eq!(row(&before), 1, "the first line wraps at 360 wide");
    ws.update(cx, |ws, _| ws.space.canvas.get_mut(id).unwrap().rect.w += 200.0);
    frame(handle, cx);
    frame(handle, cx);
    let after = note_map(&ws, cx, id).expect("taken again");
    assert_eq!(after.taken.size.0, before.taken.size.0 + 200.0);
    assert_eq!(row(&after), 0, "one row at 560 wide");
}

/// Over a far note the wheel moves the canvas; once it is clicked (focused: Full), the wheel
/// scrolls the note again.
#[gpui_kit::test]
fn the_wheel_over_a_far_note_moves_the_canvas(cx: &mut TestAppContext) {
    let text: String = (0..100).map(|i| format!("line {i}\n")).collect();
    let (handle, ws) = open(cx);
    let (id, note) = note_with(handle, &ws, cx, Point::new(40.0, 40.0), &text);
    let n = id.0 as usize;
    let corner = Point::new(30.0, 30.0);
    rest_at(handle, &ws, cx, corner, 0.33);
    unfocus(handle, &ws, cx);
    let c = cx
        .update_window(handle, |_, window, _| {
            window.find(("silhouette", n)).bounds().center()
        })
        .unwrap();
    let at = (f32::from(c.x), f32::from(c.y));
    let camera = |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    let down = ScrollDelta::Pixels(point(px(0.0), px(-40.0)));
    let before = camera(cx);
    cx.update_window(handle, |_, window, cx| wheel(window, cx, at, down, false))
        .unwrap();
    assert_ne!(camera(cx).origin, before.origin, "the canvas moved");
    rest_at(handle, &ws, cx, corner, 0.33);
    cx.update_window(handle, |_, window, cx| {
        click(window, cx, at, 1);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, ("note-text", n)), "clicked: Full");
    let (before, scroll) = (camera(cx), note.read_with(cx, |e, _| e.scroll_offset()));
    cx.update_window(handle, |_, window, cx| {
        wheel(window, cx, at, down, false);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(camera(cx), before, "the wheel is the note's");
    assert_ne!(note.read_with(cx, |e, _| e.scroll_offset()), scroll, "the note scrolled");
}

/// A note's rows survive Full → bars → Full: the same lines are on screen after.
#[gpui_kit::test]
fn a_notes_rows_survive_being_drawn_far_away(cx: &mut TestAppContext) {
    let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
    let (handle, ws) = open(cx);
    let (id, note) = note_with(handle, &ws, cx, Point::new(40.0, 40.0), &text);
    let corner = Point::new(30.0, 30.0);
    rest_at(handle, &ws, cx, corner, 0.5);
    cx.update_window(handle, |_, window, cx| {
        note.update(cx, |e, cx| {
            let row = e.line_height().unwrap();
            e.set_scroll_offset(point(px(0.0), -row * 60.0), cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    let top = |cx: &mut TestAppContext| {
        note.read_with(cx, |e, _| e.visible_row_range().unwrap().start)
    };
    assert_eq!(top(cx), 60);
    unfocus(handle, &ws, cx);
    rest_at(handle, &ws, cx, corner, 0.33);
    assert!(shown(handle, cx, ("silhouette", id.0 as usize)));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx))
    })
    .unwrap();
    rest_at(handle, &ws, cx, corner, 0.33);
    assert_eq!(top(cx), 60, "focused again at 33%");
    rest_at(handle, &ws, cx, corner, 0.5);
    assert_eq!(top(cx), 60, "and at 50%");
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p templum --bin templum -- silhouette:: lod::`
Expected: FAIL to compile (`cannot find struct \`WordMap\``, `no field \`far\` on \`Note\``, `cannot find function
\`runs\``).

- [ ] **Step 4: The word map**

Above the tests in `crates/templum/src/silhouette.rs`:

```rust
//! Editors and notes below Full: their text as one bar per word, on the rows gpui-kit laid the
//! words out on in the body's last Full frame (its word map). See
//! docs/specs/level-of-detail.md, section 3.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::input::EditorState;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, Entity, Hsla, Pixels, Point, SharedString,
    TestSupportExt, canvas, div, fill, point, px, size,
};

use crate::grid::bar_quad;

/// What a word map was taken of: when any of it changes, so may the rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Taken {
    /// The text's length in bytes (a reload from disk drops the map itself: `Tab::reload`).
    pub len: usize,
    /// The block's size, world units.
    pub size: (f32, f32),
    pub font: SharedString,
    /// Soft wrap, and an editor's file tree shown (it narrows the text).
    pub wrap: bool,
    pub tree: bool,
}

/// A run of non-blank characters, or its part on one row where gpui-kit wrapped it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Word {
    /// Its bytes in the text: the alignment tests compare its bar with its letters by it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub range: Range<usize>,
    /// Its row from the text's first, wrapped rows counted.
    pub row: usize,
    /// Its left edge from the text's left edge (the scroll not counted), and its width: world
    /// units.
    pub x: f32,
    pub w: f32,
}

/// The words gpui-kit laid out in a body's last Full frame, kept to be drawn as bars.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WordMap {
    pub taken: Taken,
    /// The scroll when it was taken, in rows: down, and across. gpui-kit's scroll is in pixels
    /// of the rows it laid out, and `editor::keep_rows` rescales it by the rows: rows keep it.
    pub scroll: (f32, f32),
    pub words: Vec<Word>,
}

impl WordMap {
    /// The words of `state`'s last layout, drawn at zoom `z`: each run of non-blank characters
    /// of each line, from the first line on screen until gpui-kit has no bounds for one (past
    /// what it laid out). None before its first layout.
    pub fn take(state: &EditorState, z: f32, taken: Taken) -> Option<WordMap> {
        let lh = f32::from(state.line_height()?);
        // The text's laid-out origin, scroll included: `range_to_bounds` is relative to it.
        let origin = state.text_bounds()?.origin;
        let first = state.visible_row_range()?.start;
        let scroll = state.scroll_offset();
        let text = state.text().to_string();
        let start = text
            .split('\n')
            .take(first)
            .map(|line| line.len() + 1)
            .sum::<usize>()
            .min(text.len());
        // The text's left edge, past the line numbers: where a line's first character is.
        let left = state
            .range_to_bounds(&(start..start))
            .map_or(0.0, |b| f32::from(b.origin.x - origin.x));
        let mut words = Vec::new();
        let mut at = start;
        'lines: for line in text[start..].split('\n') {
            for run in runs(line) {
                let range = at + run.start..at + run.end;
                let Some(parts) = place(state, &text[range.clone()], range.clone(), origin, lh)
                else {
                    break 'lines;
                };
                words.extend(parts.into_iter().map(|(row, x0, x1)| Word {
                    range: range.clone(),
                    row,
                    x: (x0 - left) / z,
                    w: (x1 - x0) / z,
                }));
            }
            at += line.len() + 1;
        }
        Some(WordMap {
            taken,
            scroll: (-f32::from(scroll.y) / lh, -f32::from(scroll.x) / lh),
            words,
        })
    }

    /// Each word's cell at zoom `s`: `(left, top, width, height)`, pixels from the body's box,
    /// its text starting at `origin`. Rows are gpui-kit's, `1.5 × font × s` rounded to whole
    /// pixels as GPUI rounds a line's height (gpui-pre-0.3.8 src/style.rs:553-555,
    /// gpui-component-0.7.1 src/input/editor.rs:14).
    pub fn cells(
        &self,
        origin: (f32, f32),
        font: f32,
        s: f32,
    ) -> impl Iterator<Item = (f32, f32, f32, f32)> + '_ {
        let pitch = (1.5 * font * s).round();
        self.words.iter().map(move |w| {
            (
                origin.0 + w.x * s - self.scroll.1 * pitch,
                origin.1 + (w.row as f32 - self.scroll.0) * pitch,
                w.w * s,
                pitch,
            )
        })
    }
}

/// The byte ranges of `line`'s runs of non-blank characters.
fn runs(line: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                found.push(s..i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        found.push(s..line.len());
    }
    found
}

/// Where gpui-kit put `word` (`range` of the text): `(row, left, right)`, pixels from `origin`,
/// one per row it is on. Empty on a hidden (folded) line: gpui-kit puts its words at the next
/// shown line's start, with no width. None past what it laid out.
fn place(
    state: &EditorState,
    word: &str,
    range: Range<usize>,
    origin: Point<Pixels>,
    lh: f32,
) -> Option<Vec<(usize, f32, f32)>> {
    let at = |b: Bounds<Pixels>| {
        let row = (f32::from(b.origin.y - origin.y) / lh).round().max(0.0) as usize;
        (row, f32::from(b.origin.x - origin.x))
    };
    let b = state.range_to_bounds(&range)?;
    if f32::from(b.size.height) < 1.5 * lh {
        let (row, x) = at(b);
        let w = f32::from(b.size.width);
        return Some(if w > 0.0 {
            vec![(row, x, x + w)]
        } else {
            Vec::new()
        });
    }
    // Wrapped inside the word: a part per row. In gpui-kit's bounds a row's last character ends
    // on the next row: it is as wide as the one before it.
    let mut parts: Vec<(usize, f32, f32)> = Vec::new();
    let mut advance = 0.0;
    for (i, c) in word.char_indices() {
        let start = range.start + i;
        let b = state.range_to_bounds(&(start..start + c.len_utf8()))?;
        if f32::from(b.size.height) < 1.5 * lh {
            advance = f32::from(b.size.width);
        }
        let (row, x) = at(b);
        match parts.last_mut() {
            Some((r, _, right)) if *r == row => *right = x + advance,
            _ => parts.push((row, x, x + advance)),
        }
    }
    Some(parts)
}

/// Where a note's text starts in its box at zoom `s`: its box makes up for gpui-kit's fixed
/// parts, so they all scale (`note.rs`).
pub(crate) fn note_origin(_: &WordMap, s: f32) -> (f32, f32) {
    (crate::note::FIXED_LEFT * s, crate::note::FIXED_TOP * s)
}

/// What a note or an editor tab keeps for drawing below Full.
#[derive(Default)]
pub(crate) struct Far {
    /// The zoom and the `Taken` of the last frame its editor was drawn Full at: gpui-kit's last
    /// layout is of them.
    full: Option<(f32, Taken)>,
    pub(crate) map: Option<Rc<WordMap>>,
}

impl Far {
    /// Its editor is drawn Full this frame, at zoom `z`.
    pub(crate) fn drawn_full(&mut self, z: f32, taken: Taken) {
        self.full = Some((z, taken));
    }
}

/// A note's or an editor tab's text below Full: its word map's bars, in an element (`id`) filling
/// the text's area. The map is taken from gpui-kit's last layout on the first frame after a Full
/// one. When it is stale (another text, size, font, wrap or tree) or missing (a canvas restored
/// far away), `full` (the editor as Full draws it) is drawn once more, unseen but laid out,
/// under the old bars so nothing blinks, and the next frame takes the map.
#[allow(clippy::too_many_arguments)]
pub(crate) fn body(
    id: ElementId,
    editor: &Entity<EditorState>,
    far: &RefCell<Far>,
    taken: Taken,
    z: f32,
    font: f32,
    origin: fn(&WordMap, f32) -> (f32, f32),
    color: Hsla,
    full: impl FnOnce(&mut App) -> AnyElement,
    cx: &mut App,
) -> AnyElement {
    let mut far = far.borrow_mut();
    if let Some((at, of)) = far.full.take()
        && of == taken
    {
        far.map = WordMap::take(editor.read(cx), at, taken.clone()).map(Rc::new);
    }
    let stale = far.map.as_ref().is_none_or(|m| m.taken != taken);
    let unseen = if stale {
        far.full = Some((z, taken));
        Some(full(cx))
    } else {
        None
    };
    let map = far.map.clone();
    drop(far);
    div()
        .id(id)
        .relative()
        .size_full()
        // GPUI lays out and paints what has opacity 0 (gpui-pre-0.3.8 src/window.rs:4078-4094).
        .children(unseen.map(|el| div().absolute().size_full().opacity(0.0).child(el)))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    // The next frame takes the map of what the unseen editor laid out.
                    if stale {
                        window.request_animation_frame();
                    }
                    let Some(map) = map else {
                        return;
                    };
                    for (x, y, w, h) in map.cells(origin(&map, z), font, z) {
                        let cell = Bounds::new(
                            point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                            size(px(w), px(h)),
                        );
                        window.paint_quad(fill(bar_quad(cell), color));
                    }
                },
            )
            .absolute()
            .size_full(),
        )
        .test_support()
        .into_any_element()
}
```

- [ ] **Step 5: The note's body below Full**

In `crates/templum/src/note.rs`:

- line 4: `use std::cell::{Cell, RefCell};`
- line 15: unchanged (`use templum_canvas::{BlockId, Level, Size, chrome};`, from Task 7)
- after `use crate::board::Label;` (line 18):

```rust
use crate::grid::{BAR_OPACITY, hsla};
use crate::silhouette::{self, Far};
```

- lines 31 and 33: `pub(crate) const FIXED_LEFT …` and `pub(crate) const FIXED_TOP …` (the word map draws the
  text where they put it).
- in `struct Note`, after `pub preview_scroll: PreviewScroll,`:

```rust
    /// Its word map, drawn far away (`silhouette.rs`).
    pub far: RefCell<Far>,
```

- in `Note::new`, after `preview_scroll: PreviewScroll::default(),`: `far: RefCell::default(),`

Replace `body` (its doc comment through its closing brace, as Task 7 left it) with:

```rust
/// The body below the header at the block's zoom `z`: the editor, or the rendered text; below
/// Full, the text's bars (rendered text is left out).
pub(crate) fn body(
    ws: &Workspace,
    id: BlockId,
    note: &Note,
    z: f32,
    level: Level,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = id.0 as usize;
    let content = ws
        .space
        .canvas
        .get(id)
        .map_or(Size::default(), |b| chrome::content_rect(b.rect).size());
    if level != Level::Full {
        // Rendered Markdown has no layout to draw bars from: the tinted card, empty, keeps
        // everything in place.
        if note.preview {
            return div().size_full().into_any_element();
        }
        // Its words as bars: no Markdown keys, and the wheel moves the canvas.
        let taken = taken(ws, note, content, cx);
        let color = hsla(ws.theme.palette.foreground, BAR_OPACITY);
        return silhouette::body(
            ("silhouette", n).into(),
            &note.editor,
            &note.far,
            taken,
            z,
            FONT,
            silhouette::note_origin,
            color,
            |cx| editor_box(ws, n, note, content, z, cx),
            cx,
        );
    }
    let inner = if note.preview {
        preview(
            ("note-preview", n).into(),
            ("note-markdown", n).into(),
            note.text(cx).into(),
            &note.preview_scroll,
            content.w - 2.0 * BORDER,
            FONT,
            z,
        )
    } else {
        let taken = taken(ws, note, content, cx);
        note.far.borrow_mut().drawn_full(z, taken);
        editor_box(ws, n, note, content, z, cx)
    };
    // The wheel scrolls the note and stops there; with Ctrl it zooms the canvas.
    let (enter, key) = (note.editor.clone(), note.editor.clone());
    div()
        .size_full()
        .text_size(px(FONT * z))
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        // Markdown keys, in the note's own text only (not its search field): before the editor
        // takes Enter, which it binds; Ctrl+B and Ctrl+I it doesn't.
        .when(!note.preview, |el| {
            el.capture_action(move |e: &Enter, window, cx| {
                let done = enter.update(cx, |state, cx| {
                    state.focus_handle(cx).is_focused(window)
                        && !e.shift
                        && if e.secondary {
                            tick(state, window, cx)
                        } else {
                            continue_list(state, window, cx)
                        }
                });
                if done {
                    cx.stop_propagation();
                }
            })
            .on_key_down(move |e: &KeyDownEvent, window, cx| {
                let Some(marker) = emphasis(&e.keystroke) else {
                    return;
                };
                let done = key.update(cx, |state, cx| {
                    let focused = state.focus_handle(cx).is_focused(window);
                    if focused {
                        emphasize(state, marker, window, cx);
                    }
                    focused
                });
                if done {
                    cx.stop_propagation();
                }
            })
        })
        .child(inner)
        .into_any_element()
}

/// What the note's word map is of.
fn taken(ws: &Workspace, note: &Note, content: Size, cx: &App) -> silhouette::Taken {
    silhouette::Taken {
        len: note.editor.read(cx).text().len(),
        size: (content.w, content.h),
        font: ws.cell.font.family.clone(),
        wrap: true,
        tree: false,
    }
}

/// The note's editor as Full draws it, at zoom `z`. Its fixed parts would make the text start
/// further in, wrap at other words and show fewer lines the further the canvas is zoomed out:
/// the editor is as much larger (or smaller) and moved by its fixed left and top parts' share,
/// so the text starts, wraps and fits as if they zoomed too. Its lines change only when the
/// block is resized. At zoom 1, exactly the note's box.
fn editor_box(
    ws: &Workspace,
    n: usize,
    note: &Note,
    content: Size,
    z: f32,
    cx: &mut App,
) -> AnyElement {
    let (across, down) = (FIXED_LEFT + FIXED_RIGHT, FIXED_TOP + FIXED_BOTTOM);
    crate::editor::keep_rows(&note.editor, FONT, z, cx);
    div()
        .id(("note-editor", n))
        .size_full()
        .font_family(ws.cell.font.family.clone())
        .child(
            div()
                .id(("note-text", n))
                .relative()
                .left(px(-FIXED_LEFT * (1.0 - z)))
                .top(px(-FIXED_TOP * (1.0 - z)))
                .w(px(
                    ((content.w - 2.0 * BORDER - across) * z + across).max(0.0)
                ))
                .h(px(((content.h - down) * z + down).max(0.0)))
                .child(
                    // On the editor itself: it sets the theme's code size on itself, over a
                    // parent's.
                    Editor::new(&note.editor)
                        .appearance(false)
                        .bordered(false)
                        .text_size(px(FONT * z))
                        .h(relative(1.0)),
                )
                .test_support(),
        )
        .test_support()
        .into_any_element()
}
```

(The Full branch is the code at HEAD, `note.rs:118-208`, with the editor box moved into `editor_box` and the
word map told what it was drawn at.)

- [ ] **Step 6: Run them to see them pass**

Run: `cargo test -p templum --bin templum -- silhouette:: lod::`
Expected: PASS (4 unit tests; the 6 new headless tests and every earlier one in `lod.rs`). If the alignment test
fails on "line 3 is folded away", the click missed the fold icon: print `note.read(cx).text_bounds()` and the line
height before the click and compare with gpui-base-0.7.1 `src/input/base/element.rs:1334-1348` (the icon sits at
`origin_x + 2`, `bounds.y + offset_y + (line_height - 14) / 2`, 14 px wide). Then
`cargo test -p templum --bin templum -- notes navigation` — the note and zoom tests still pass (they use a note just
added, which holds the keyboard: Full).

- [ ] **Step 7: Check by eye**

`cargo run -p templum`: a note with a few paragraphs and headings, another block focused, zoom to 33% and 25%: the
note's lines are bars where its words were; zoom back to 50%: the letters are exactly where the bars were. Fold a
section and scroll the note halfway first: the same.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum/src/silhouette.rs crates/templum/src/main.rs crates/templum/src/note.rs \
  crates/templum/src/workspace/tests/lod.rs && \
git commit -F - <<'EOF'
perf: draw far notes as bars of their words

At 33% and 25% a note you aren't working in no longer runs gpui-kit's editor:
its words are bars on the rows it last showed them on, and the wheel over it
moves the canvas.

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 10: Far editors: the word map, the tree's empty panel

**Files:**
- Modify: `crates/templum/src/silhouette.rs` (`TAB_LEFT`, `TAB_TOP`, `NUMBERS_MARGIN`; `WordMap::numbers`, set by
  `take`; `tab_origin`; tests: the `map` helper gains `numbers`, one new test)
- Modify: `crates/templum/src/editor.rs:4` (imports), `:25-28` (imports), `:245-259` (`Tab::far`), `:285-301`
  (`Tab::reload` drops it), `:442-452` (`open_tab`), `body` (`:630-656`), `files` (the code area: `:940-958` at HEAD)
- Modify: `docs/minors.md` (a new section, "Far zoom (level of detail)")
- Test: `crates/templum/src/silhouette.rs`, `crates/templum/src/workspace/tests/lod.rs`

**Interfaces:**
- Consumes: Task 9's `silhouette::{Taken, WordMap, Far, body}`; `grid::{BAR_OPACITY, hsla}`; `editor::keep_rows`;
  `Workspace::check_files` (`workspace/files.rs:424`, `pub(super)`); `gpui_kit::component::input::Undo` (test); test
  helpers `open_editor`, `click_file` (`tests/mod.rs`), `rest_at`, `unfocus`, `shown` (Task 3).
- Produces:
  - `WordMap { pub numbers: f32, … }` — the line numbers' digits' width, world units;
  - `pub(crate) fn tab_origin(map: &WordMap, s: f32) -> (f32, f32)`;
  - `Tab { pub far: RefCell<Far>, … }`; `fn code(ws: &Workspace, tab: &Tab, read_only: bool, z: f32, cx: &mut App) -> AnyElement`
    (editor.rs, private);
  - test helpers in `tests/lod.rs`: `fn editor_with(handle, ws, cx, files: &[(&str, &str)]) -> (tempfile::TempDir, BlockId, Entity<EditorState>)`,
    `fn tab_map(ws, cx, id: BlockId) -> Option<Rc<WordMap>>`.

- [ ] **Step 1: Write the failing tests**

In `crates/templum/src/silhouette.rs`'s tests, give the `map` helper the new field — in its `WordMap { … }`, after
`scroll,` add `numbers: 0.0,` — and append:

```rust
    #[test]
    fn an_editor_tabs_text_starts_past_its_line_numbers() {
        // Digits 0.6 em wide at 13 px: three of them (a file under 1,000 lines), or four.
        let three = WordMap {
            numbers: 3.0 * 0.6 * 13.0,
            ..map(Vec::new(), (0.0, 0.0))
        };
        let four = WordMap {
            numbers: 4.0 * 0.6 * 13.0,
            ..map(Vec::new(), (0.0, 0.0))
        };
        assert!((tab_origin(&three, 0.25).0 - 17.85).abs() < 1e-4, "6 + 5.85 + 6");
        assert!((tab_origin(&four, 0.25).0 - 19.8).abs() < 1e-4, "6 + 7.8 + 6");
        assert!((tab_origin(&four, 1.0).0 - 43.2).abs() < 1e-4, "6 + 31.2 + 6");
        assert_eq!(tab_origin(&three, 0.5).1, 8.0, "the top padding doesn't scale");
    }
```

Append to `crates/templum/src/workspace/tests/lod.rs`:

```rust
/// An editor on a folder holding `files` (name, text), its first file open and focused, at rest.
fn editor_with(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    files: &[(&str, &str)],
) -> (tempfile::TempDir, BlockId, Entity<EditorState>) {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in files {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    let id = open_editor(handle, ws, cx, dir.path());
    click_file(handle, 0, cx);
    let state = ws.read_with(cx, |ws, _| {
        ws.space.editor(id).unwrap().active_tab().unwrap().editor.clone()
    });
    (dir, id, state)
}

fn tab_map(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> Option<Rc<WordMap>> {
    ws.read_with(cx, |ws, _| {
        let tab = ws.space.editor(id).unwrap().active_tab().unwrap();
        tab.far.borrow().map.clone()
    })
}

/// An unfocused editor at 33% builds no gpui-kit editor and no tree rows: its file's words are
/// bars where its code was, its tab row stays. The level changes when a glide ends: out from
/// 50%, bars from the frame it ends (the map of its last frame); in from 33%, its code.
#[gpui_kit::test]
fn a_far_editor_draws_bars_where_its_code_was(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_dir, id, _) =
        editor_with(handle, &ws, cx, &[("a.rs", "fn main() {\n    println!(\"hi\");\n}\n")]);
    let n = id.0 as usize;
    let corner = Point::new(30.0, 30.0);
    rest_at(handle, &ws, cx, corner, 0.33);
    let code = cx
        .update_window(handle, |_, window, _| window.find(("code", n)).bounds())
        .unwrap();
    unfocus(handle, &ws, cx);
    assert!(!shown(handle, cx, ("code", n)), "no gpui-kit editor");
    assert!(!shown(handle, cx, ("tree", n)), "the tree's panel is empty");
    let bars = cx
        .update_window(handle, |_, window, _| window.find(("silhouette", n)).bounds())
        .unwrap();
    assert_eq!(bars, code, "bars where the code was");
    assert!(shown(handle, cx, ElementId::Name(format!("tab-{n}-0").into())), "the tab row stays");
    let words = tab_map(&ws, cx, id).expect("taken").words.len();
    assert_eq!(words, 5, "fn, main(), {{, println!(\"hi\");, }}");
    rest_at(handle, &ws, cx, corner, 0.5);
    for (step, during, landed) in [(-1, "code", "silhouette"), (1, "silhouette", "code")] {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let pivot = ws.center(window);
                ws.zoom_to(step, pivot, cx);
            })
        })
        .unwrap();
        ws.update(cx, restart_glide);
        std::thread::sleep(Duration::from_millis(40));
        frame(handle, cx);
        assert!(shown(handle, cx, (during, n)), "mid-glide, step {step}: {during}");
        std::thread::sleep(crate::motion::FLIGHT + Duration::from_millis(50));
        frame(handle, cx);
        assert!(
            shown(handle, cx, (landed, n)) && !shown(handle, cx, (during, n)),
            "landed, step {step}: {landed}"
        );
    }
}

/// Bars line up with letters at every zoom step, within 1 px: an editor tab of 1,000 lines (four
/// digits of line numbers) scrolled 10 rows down and 6 rows' worth across, its word map taken at
/// 50%, against its letters laid out at each step.
#[gpui_kit::test]
fn an_editor_tabs_bars_line_up_with_its_letters_at_every_zoom(cx: &mut TestAppContext) {
    use crate::silhouette::{Taken, tab_origin};
    let words: Vec<String> = (0..30).map(|j| format!("w{j}")).collect();
    let text: String = (0..1000)
        .map(|i| format!("line{i} {}\n", words.join(" ")))
        .collect();
    let (handle, ws) = open(cx);
    let (_dir, id, state) = editor_with(handle, &ws, cx, &[("a.txt", text.as_str())]);
    let n = id.0 as usize;
    let corner = Point::new(30.0, 30.0);
    rest_at(handle, &ws, cx, corner, 0.5);
    cx.update_window(handle, |_, window, cx| {
        state.update(cx, |e, cx| {
            let row = e.line_height().unwrap();
            e.set_scroll_offset(point(-row * 6.0, -row * 10.0), cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    let map = cx
        .update_window(handle, |_, _, cx| {
            let taken = Taken {
                len: text.len(),
                size: (0.0, 0.0),
                font: "".into(),
                wrap: false,
                tree: true,
            };
            WordMap::take(state.read(cx), 0.5, taken).expect("laid out")
        })
        .unwrap();
    assert!(
        (map.scroll.0 - 10.0).abs() < 0.01 && (map.scroll.1 - 6.0).abs() < 0.01,
        "taken scrolled: {:?}",
        map.scroll
    );
    for zoom in templum_canvas::ZOOM_STEPS {
        rest_at(handle, &ws, cx, corner, zoom);
        cx.update_window(handle, |_, window, cx| {
            let origin = window.find(("code", n)).bounds().origin;
            let e = state.read(cx);
            let lh = f32::from(e.line_height().unwrap());
            let cells = map.cells(tab_origin(&map, zoom), crate::editor::FONT, zoom);
            let mut compared = 0;
            for (word, (x, y, w, h)) in map.words.iter().zip(cells) {
                let Some(b) = e.range_to_bounds(&word.range) else {
                    continue;
                };
                if f32::from(b.size.height) > 1.5 * lh || b.size.width <= px(0.0) {
                    continue;
                }
                let (x, y) = (f32::from(origin.x) + x, f32::from(origin.y) + y);
                for (what, bar, letters) in [
                    ("left", x, f32::from(b.origin.x)),
                    ("right", x + w, f32::from(b.origin.x + b.size.width)),
                    ("top", y, f32::from(b.origin.y)),
                    ("height", h, f32::from(b.size.height)),
                ] {
                    assert!(
                        (bar - letters).abs() <= 1.0,
                        "{what} of {:?} at {zoom}: bar {bar}, letters {letters}",
                        &text[word.range.clone()]
                    );
                }
                compared += 1;
            }
            assert!(compared > 50, "{compared} words compared at {zoom}");
        })
        .unwrap();
    }
}

/// A far editor whose file changes on disk shows the new text's bars a frame later, even when
/// the new text is as long as the old: the reload drops its word map, one unseen Full frame lays
/// the new text out, the next takes it.
#[gpui_kit::test]
fn a_far_editor_shows_its_files_new_text_after_a_frame(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (dir, id, _) = editor_with(handle, &ws, cx, &[("a.txt", "old words\n")]);
    rest_at(handle, &ws, cx, Point::new(30.0, 30.0), 0.33);
    unfocus(handle, &ws, cx);
    assert_eq!(tab_map(&ws, cx, id).expect("taken").words.len(), 2);
    // As long as before: only the reload can tell the map is stale. A later modification time.
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(dir.path().join("a.txt"), "a b c d e\n").unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.check_files(window, cx))
    })
    .unwrap();
    assert!(tab_map(&ws, cx, id).is_none(), "the reload dropped the map");
    frame(handle, cx);
    frame(handle, cx);
    let map = tab_map(&ws, cx, id).expect("taken again");
    assert_eq!(map.words.len(), 5, "a, b, c, d, e");
}

/// A focused editor at 25% is Full and typing reaches it; when the focus moves to another block,
/// it is bars on the next frame.
#[gpui_kit::test]
fn a_focused_far_editor_takes_typing_and_the_one_left_draws_bars(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (_dir, id, state) = editor_with(handle, &ws, cx, &[("a.txt", "one\n")]);
    let n = id.0 as usize;
    rest_at(handle, &ws, cx, Point::new(30.0, 30.0), 0.25);
    assert!(shown(handle, cx, ("code", n)), "focused: Full at 25%");
    cx.update_window(handle, |_, window, cx| {
        window.input("zz", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(state.read_with(cx, |e, _| e.value().contains("zz")), "typing reaches it");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.add_note_at(Point::new(3000.0, 0.0), window, cx))
    })
    .unwrap();
    frame(handle, cx);
    assert!(shown(handle, cx, ("silhouette", n)), "left: bars");
}

/// Over a far editor the wheel moves the canvas; once it is clicked (focused: Full), the wheel
/// scrolls its file again.
#[gpui_kit::test]
fn the_wheel_over_a_far_editor_moves_the_canvas(cx: &mut TestAppContext) {
    let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
    let (handle, ws) = open(cx);
    let (_dir, id, state) = editor_with(handle, &ws, cx, &[("a.txt", text.as_str())]);
    let n = id.0 as usize;
    let corner = Point::new(30.0, 30.0);
    rest_at(handle, &ws, cx, corner, 0.33);
    unfocus(handle, &ws, cx);
    let c = cx
        .update_window(handle, |_, window, _| {
            window.find(("silhouette", n)).bounds().center()
        })
        .unwrap();
    let at = (f32::from(c.x), f32::from(c.y));
    let camera = |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    let down = ScrollDelta::Pixels(point(px(0.0), px(-40.0)));
    let before = camera(cx);
    cx.update_window(handle, |_, window, cx| wheel(window, cx, at, down, false))
        .unwrap();
    assert_ne!(camera(cx).origin, before.origin, "the canvas moved");
    rest_at(handle, &ws, cx, corner, 0.33);
    cx.update_window(handle, |_, window, cx| {
        click(window, cx, at, 1);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(shown(handle, cx, ("code", n)), "clicked: Full");
    let (before, scroll) = (camera(cx), state.read_with(cx, |e, _| e.scroll_offset()));
    cx.update_window(handle, |_, window, cx| {
        wheel(window, cx, at, down, false);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(camera(cx), before, "the wheel is the editor's");
    assert_ne!(state.read_with(cx, |e, _| e.scroll_offset()), scroll, "its file scrolled");
}

/// An editor's state survives Full → bars → Full: its tabs and the active one, its rows on screen,
/// its selection, its text and its undo history.
#[gpui_kit::test]
fn an_editors_state_survives_being_drawn_far_away(cx: &mut TestAppContext) {
    use gpui_kit::component::input::Undo;
    let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
    let (handle, ws) = open(cx);
    let (_dir, id, state) =
        editor_with(handle, &ws, cx, &[("a.txt", text.as_str()), ("b.txt", "b\n")]);
    // b.txt opened, then a.txt shown again: two tabs, the first active.
    click_file(handle, 1, cx);
    click_file(handle, 0, cx);
    let corner = Point::new(30.0, 30.0);
    rest_at(handle, &ws, cx, corner, 0.5);
    cx.update_window(handle, |_, window, cx| {
        window.input("typed ", cx);
        state.update(cx, |e, cx| {
            // A selection on line 45, on screen once scrolled 40 rows down.
            let at = e.value().find("line 45").unwrap();
            e.set_selected_range(at..at + 4, cx);
            let row = e.line_height().unwrap();
            e.set_scroll_offset(point(px(0.0), -row * 40.0), cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    let seen = |cx: &mut TestAppContext| {
        state.read_with(cx, |e, _| (e.visible_row_range(), e.selected_range(), e.value()))
    };
    let before = seen(cx);
    unfocus(handle, &ws, cx);
    rest_at(handle, &ws, cx, corner, 0.33);
    assert!(shown(handle, cx, ("silhouette", id.0 as usize)));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx))
    })
    .unwrap();
    rest_at(handle, &ws, cx, corner, 0.5);
    assert_eq!(seen(cx), before, "its rows, selection and text as they were");
    ws.read_with(cx, |ws, _| {
        let e = ws.space.editor(id).unwrap();
        assert_eq!((e.tabs.len(), e.active), (2, Some(0)));
    });
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(Undo), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_ne!(state.read_with(cx, |e, _| e.value()), before.2, "its undo history is there");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum --bin templum -- silhouette:: lod::`
Expected: FAIL to compile (`no field \`numbers\``, `cannot find function \`tab_origin\``, `no field \`far\` on
\`Tab\``).

- [ ] **Step 3: The editor tab's text origin**

In `crates/templum/src/silhouette.rs`, after the imports:

```rust
/// gpui-kit's fixed pixels around an editor tab's text: the code editor's padding, left
/// (`input_px().min(6)`) and top (`input_py()`, 8 at size Medium; gpui-component-0.7.1
/// src/input/input.rs:588-602), and the margin right of the line numbers (gpui-base-0.7.1
/// src/input/base/element.rs:51).
const TAB_LEFT: f32 = 6.0;
const TAB_TOP: f32 = 8.0;
const NUMBERS_MARGIN: f32 = 6.0;
```

In `struct WordMap`, after `pub scroll: (f32, f32),`:

```rust
    /// The line numbers' digits' width, world units: an editor tab's text starts past them. (A
    /// note's text starts where its box puts it: `note_origin`.)
    pub numbers: f32,
```

In `WordMap::take`, the `Some(WordMap { … })` gains, after the `scroll` line:

```rust
            numbers: (left - NUMBERS_MARGIN) / z,
```

After `note_origin`:

```rust
/// Where an editor tab's text starts in its area at zoom `s`: gpui-kit's fixed padding, the line
/// numbers (their digits scale with the text) and their fixed margin. Editor tabs have no fold
/// gutter (`editor.rs`, `folding(false)`).
pub(crate) fn tab_origin(map: &WordMap, s: f32) -> (f32, f32) {
    (TAB_LEFT + map.numbers * s + NUMBERS_MARGIN, TAB_TOP)
}
```

- [ ] **Step 4: The editor's body below Full**

In `crates/templum/src/editor.rs`:

- line 4: `use std::cell::{Cell, RefCell};`
- after `use crate::menu::Target;` (line 25):

```rust
use crate::grid::{BAR_OPACITY, hsla};
use crate::silhouette::{self, Far};
```

- in `struct Tab`, after `pub preview_scroll: crate::note::PreviewScroll,`:

```rust
    /// Its word map, drawn far away (`silhouette.rs`).
    pub far: RefCell<Far>,
```

- in `open_tab`'s `Tab { … }`, after `preview_scroll: crate::note::PreviewScroll::default(),`:
  `far: RefCell::default(),`
- in `Tab::reload`, after `self.mtime = mtime(&self.path);`:

```rust
        // Its rows are of the old text, which may have been as long: a word map is taken again.
        *self.far.get_mut() = Far::default();
```

Replace `body` (its doc comment through its closing brace, as Task 7 left it) with:

```rust
/// The body below the header, at the block's zoom `z`: the tree on the left (when shown) and
/// the files on the right. Below Full the tree is its empty panel and the file its bars.
pub(crate) fn body(
    ws: &Workspace,
    id: BlockId,
    editor: &Editor,
    z: f32,
    level: Level,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    if !editor.folder.is_dir() {
        return gone(id, &editor.folder, ui, z, cx);
    }
    let tree = editor.show_tree.then(|| {
        if level == Level::Full {
            tree_panel(id, editor, ui, z, cx)
        } else {
            // Its empty panel: the same width and border, no rows.
            div()
                .w(px(TREE_W * z))
                .h_full()
                .flex_shrink_0()
                .border_r_1()
                .border_color(ui.hairline)
                .into_any_element()
        }
    });
    // The wheel scrolls the tree or the file and stops there; with Ctrl it zooms the canvas.
    // Below Full there is nothing to scroll: the wheel moves the canvas.
    div()
        .size_full()
        .flex()
        .when(level == Level::Full, |el| {
            el.on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
                if !e.modifiers.control {
                    cx.stop_propagation();
                }
            })
        })
        .children(tree)
        .child(files(ws, id, editor, z, level, cx))
        .into_any_element()
}
```

In `files`, replace the code editor branch — the final `else { keep_rows(&tab.editor, FONT, z, cx); div().id(("code",
n)) … .into_any_element() }` of `let view` (lines 940-958 at HEAD) — with:

```rust
                } else {
                    let taken = silhouette::Taken {
                        len: tab.editor.read(cx).text().len(),
                        size: ws
                            .space
                            .canvas
                            .get(id)
                            .map_or((0.0, 0.0), |b| (b.rect.w, b.rect.h)),
                        font: ws.cell.font.family.clone(),
                        wrap: editor.wrap,
                        tree: editor.show_tree,
                    };
                    let read_only = read_only.is_some();
                    if level == Level::Full {
                        tab.far.borrow_mut().drawn_full(z, taken);
                        div()
                            .id(("code", n))
                            .size_full()
                            .child(code(ws, tab, read_only, z, cx))
                            .test_support()
                            .into_any_element()
                    } else {
                        // Its words as bars: no editor is built, its scroll stays as it was.
                        silhouette::body(
                            ("silhouette", n).into(),
                            &tab.editor,
                            &tab.far,
                            taken,
                            z,
                            FONT,
                            silhouette::tab_origin,
                            hsla(ws.theme.palette.foreground, BAR_OPACITY),
                            |cx| code(ws, tab, read_only, z, cx),
                            cx,
                        )
                    }
                };
```

and add, after `files`:

```rust
/// The active file in gpui-kit's editor at zoom `z`, its scroll kept in rows first. The element
/// sets the theme's code font and size itself, and the read-only flag on every frame: they go on
/// it, not on a parent.
fn code(ws: &Workspace, tab: &Tab, read_only: bool, z: f32, cx: &mut App) -> AnyElement {
    keep_rows(&tab.editor, FONT, z, cx);
    EditorView::new(&tab.editor)
        .appearance(false)
        .bordered(false)
        .readonly(read_only)
        .font_family(ws.cell.font.family.clone())
        .text_size(px(FONT * z))
        .h(relative(1.0))
        .into_any_element()
}
```

- [ ] **Step 5: The minor this leaves**

Append to `docs/minors.md`:

```markdown

## Far zoom (level of detail)

- [ ] **A soft-wrapped editor tab's bars keep the wrap of the zoom they were taken at.** An editor tab's wrap width
  keeps gpui-kit's fixed padding and line-number margin (`crates/templum/src/editor.rs`, `code` in `files`), so with
  Soft wrap on its words wrap a little differently at each zoom; far away its bars show the rows of its last Full
  frame, and back at 50% a word or two can move to the next row. Notes don't: their box makes up for those parts
  (`note.rs`, `editor_box`). Fix: give the editor tab's area the note's compensation (`FIXED_LEFT`, `FIXED_RIGHT`).
```

- [ ] **Step 6: Run them to see them pass**

Run: `cargo test -p templum --bin templum -- silhouette:: lod::`
Expected: PASS. Then `cargo test -p templum --bin templum -- editor navigation files` — the editor, tab and zoom
tests still pass.

- [ ] **Step 7: Check by eye**

`cargo run -p templum`: an editor on this repository with `crates/templum/src/block.rs` open, scrolled to the middle
and a little to the right; another block focused; zoom to 33% and 25%: the code's lines are bars where its words were
(indentation shows), the tree is an empty panel; zoom back to 50%: the letters are where the bars were. Have an agent
(or `echo >> file`) change the open file while far: its bars follow within ~2 s.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum/src/silhouette.rs crates/templum/src/editor.rs docs/minors.md \
  crates/templum/src/workspace/tests/lod.rs && \
git commit -F - <<'EOF'
perf: draw far editors as bars of their words

At 33% and 25% an editor you aren't working in no longer runs gpui-kit's
editor or its file tree: its file's words are bars on the rows it last showed
them on, they follow the file when it changes on disk, and the wheel over it
moves the canvas.

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 11: The Git block's lists keep their scroll through its card

**Files:**
- Modify: `crates/templum/src/git_block/mod.rs:13-16` (import `ScrollHandle`), `:298` (three fields after
  `lists_scroll`), `:421` (their values)
- Modify: `crates/templum/src/git_block/history.rs:7-10` (import), `:33` (call), `:262-268` (`commit_column` takes
  the handle), `:366-372` (the details track it)
- Modify: `crates/templum/src/git_block/branches.rs:11-14` (import), `:35` (call), `:97-103` (`compare_column`
  takes the handle), `:205-210` (the list tracks it)
- Modify: `crates/templum/src/git_block/stashes.rs:113-118` (the list tracks it)
- Test: `crates/templum/src/workspace/tests/lod.rs`

**Interfaces:**
- Consumes: Task 3's Git card below Full; test helpers `rest_at`, `unfocus`, `shown` (Task 3), `open_git`,
  `committed_repo`, `git_run`, `git_of`, `settle` (`tests/mod.rs`); `Workspace::{set_git_tab, compare_branch}`.
- Produces: `Git { pub details_scroll: ScrollHandle, pub compare_scroll: ScrollHandle, pub stash_scroll: ScrollHandle, … }`;
  `fn commit_column(ws, id, shown: &Shown, scroll: &ScrollHandle, z, cx)`, `fn compare_column(ws, id, c: &Compare, scroll: &ScrollHandle, z, cx)`
  (private). The three lists register for tests (`.test_support()`).

(Spec, section 5: GPUI keeps an element's own scroll only while it is drawn, `gpui-pre-0.3.8 src/window.rs:1145-1152`;
today these three reset when the block leaves the screen, and a card would reset them without it leaving. The fourth
the spec names, Recently discarded, is a dialog drawn by the window: see "Where the code differs from the spec".)

- [ ] **Step 1: Write the failing tests**

Append to `crates/templum/src/workspace/tests/lod.rs`:

```rust
/// Scrolls `list` with the wheel at 100%, draws its Git block as its card at 33%, and back: the
/// list is where it was.
fn a_lists_scroll_survives_a_card(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    list: ElementId,
    scroll: fn(&crate::git_block::Git) -> gpui_kit::ScrollHandle,
) {
    rest_at(handle, ws, cx, Point::new(0.0, 0.0), 1.0);
    cx.update_window(handle, |_, window, cx| {
        window.scroll(list.clone(), ScrollDelta::Pixels(point(px(0.0), px(-120.0))), cx);
        window.render_frame(cx);
    })
    .unwrap();
    let before = git_of(ws, cx, id, |g| scroll(g).offset().y);
    assert!(before < px(0.0), "the wheel moved {list:?}'s own scroll");
    unfocus(handle, ws, cx);
    rest_at(handle, ws, cx, Point::new(0.0, 0.0), 0.33);
    assert!(!shown(handle, cx, list.clone()), "far away the block is its card");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx))
    })
    .unwrap();
    rest_at(handle, ws, cx, Point::new(0.0, 0.0), 1.0);
    assert!(shown(handle, cx, list.clone()));
    let after = git_of(ws, cx, id, |g| scroll(g).offset().y);
    assert_eq!(after, before, "{list:?} kept its scroll");
}

#[gpui_kit::test]
fn a_commits_details_keep_their_scroll_through_a_card(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let body: String = (1..=80).map(|i| format!("Line {i} of a long message.\n")).collect();
    git_run(repo.path(), &["commit", "-q", "--allow-empty", "-m", "Long", "-m", &body]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.set_git_tab(id, crate::git_block::Tab::History, window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    rest_at(handle, &ws, cx, Point::new(0.0, 0.0), 1.0);
    cx.update_window(handle, |_, window, cx| window.click(("git-commit", 0usize), cx))
        .unwrap();
    settle(handle, cx);
    let list = ElementId::from(("git-commit-details", id.0 as usize));
    a_lists_scroll_survives_a_card(handle, &ws, cx, id, list, |g| g.details_scroll.clone());
    git_of(&ws, cx, id, |g| assert!(g.shown.is_some(), "the commit is still shown"));
}

#[gpui_kit::test]
fn a_comparisons_commits_keep_their_scroll_through_a_card(cx: &mut TestAppContext) {
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "feature"]);
    for i in 0..40 {
        git_run(repo.path(), &["commit", "-q", "--allow-empty", "-m", &format!("f{i}")]);
    }
    git_run(repo.path(), &["checkout", "-q", "main"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.compare_branch(id, "feature".to_string(), window, cx))
    })
    .unwrap();
    settle(handle, cx);
    let list = ElementId::from(("git-compare-list", id.0 as usize));
    a_lists_scroll_survives_a_card(handle, &ws, cx, id, list, |g| g.compare_scroll.clone());
}

#[gpui_kit::test]
fn the_stashes_keep_their_scroll_through_a_card(cx: &mut TestAppContext) {
    let repo = committed_repo();
    for i in 0..15 {
        std::fs::write(repo.path().join("a.txt"), format!("{i}\n")).unwrap();
        git_run(repo.path(), &["stash", "push", "-q", "-m", &format!("s{i}")]);
    }
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.set_git_tab(id, crate::git_block::Tab::Stashes, window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    let list = ElementId::from(("git-stash-list", id.0 as usize));
    a_lists_scroll_survives_a_card(handle, &ws, cx, id, list, |g| g.stash_scroll.clone());
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum --bin templum keep_their_scroll_through_a_card`
Expected: FAIL to compile (`no field \`details_scroll\` on type \`Git\``).

- [ ] **Step 3: The handles**

In `crates/templum/src/git_block/mod.rs`, add `ScrollHandle` to the `gpui_kit` import (lines 13-16). After
`pub lists_scroll: UniformListScrollHandle,` (line 298):

```rust
    /// The scroll of History's commit details, of a comparison's commits and of the stashes:
    /// kept here, as `lists_scroll` is, since GPUI keeps an element's own scroll only while it
    /// is drawn, and far away the block is its card.
    pub details_scroll: ScrollHandle,
    pub compare_scroll: ScrollHandle,
    pub stash_scroll: ScrollHandle,
```

and in `Git::new`, after `lists_scroll: UniformListScrollHandle::new(),` (line 421):

```rust
            details_scroll: ScrollHandle::new(),
            compare_scroll: ScrollHandle::new(),
            stash_scroll: ScrollHandle::new(),
```

In `crates/templum/src/git_block/history.rs`, add `ScrollHandle` to the `gpui_kit` import (lines 7-10); line 33:

```rust
        Some(shown) => commit_column(ws, id, shown, &git.details_scroll, z, cx),
```

`commit_column` takes it after `shown: &Shown,`:

```rust
    scroll: &ScrollHandle,
```

and the details list (lines 366-372) tracks it and registers for tests:

```rust
            div()
                .id(("git-commit-details", n))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(details)
                .children(files)
                .test_support(),
```

In `crates/templum/src/git_block/branches.rs`, add `ScrollHandle` to the `gpui_kit` import (lines 11-14); line 35:

```rust
            compare_column(ws, id, c, &git.compare_scroll, z, cx),
```

`compare_column` takes it after `c: &Compare,`:

```rust
    scroll: &ScrollHandle,
```

and its list (lines 205-210):

```rust
            div()
                .id(("git-compare-list", n))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .children(rows)
                .children(empty)
                .test_support(),
```

In `crates/templum/src/git_block/stashes.rs`, the list (lines 113-118):

```rust
            div()
                .id(("git-stash-list", n))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(&git.stash_scroll)
                .children(rows)
                .test_support(),
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum --bin templum keep_their_scroll_through_a_card`
Expected: PASS (3 tests). Then `cargo test -p templum --bin templum git_` — the Git tests still pass.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add crates/templum/src/git_block/mod.rs crates/templum/src/git_block/history.rs \
  crates/templum/src/git_block/branches.rs crates/templum/src/git_block/stashes.rs \
  crates/templum/src/workspace/tests/lod.rs && \
git commit -F - <<'EOF'
fix(git): keep the commit details', comparison's and stashes' scroll at far zoom

They no longer jump back to the top after the block was seen from far away
(drawn as its card) or left the screen.

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 12: The documentation

**Files:**
- Modify: `docs/ARCHITECTURE.md:12` (the `templum-canvas` row), `:50-52` (the Zoom bullet)
- Modify: `docs/specs/walking-skeleton.md:28-36` (Later)
- Modify: `CLAUDE.md:48-54` (the Performance rule)
- Modify: `docs/specs/level-of-detail.md` (where the code differs, see the top of this plan)

**Interfaces:** none (documentation only; no test: nothing to run but the check).

- [ ] **Step 1: ARCHITECTURE.md**

Line 12, the `templum-canvas` row's responsibility becomes:

```markdown
| `templum-canvas` | lib | Pure model: camera (pan, zoom), blocks, hit-testing, snapping, level of detail, layout (de)serialization | No |
```

In the Zoom bullet, replace

```markdown
  rasterized at its real size and stays sharp. Zoom snaps to fixed steps to keep the glyph cache bounded;
  far-away blocks are drawn simplified (level of detail). A zoom glides to its step over 150 ms through
```

with

```markdown
  rasterized at its real size and stays sharp. Zoom snaps to fixed steps to keep the glyph cache bounded.
  Far away, each block is drawn at a level of detail picked by its text's size on screen at the zoom the camera
  last came to rest at (`Space::rest`; `templum-canvas`'s `lod.rs`), so a level never changes mid-glide: Full
  while the text is 5.5 px or more; Silhouette under it, where a terminal paints bars of its text's colors on the
  letters' cells, an editor or a note bars of the words gpui-kit laid out in its last Full frame
  (`silhouette.rs`), and a Git block is its card; Card under 2.5 px. The block worked in (focused, or holding the
  keyboard) is always Full, and below Full a title is a fixed-size label (specs/level-of-detail.md).
  A zoom glides to its step over 150 ms through
```

- [ ] **Step 2: walking-skeleton.md**

In "Out of scope → Later", delete the line `  - level-of-detail drawing at low zoom;`, and after the list's last item
(`  - the kitty keyboard protocol.`) add:

```markdown

Level-of-detail drawing at low zoom, once listed under Later, is specified in [level-of-detail.md](level-of-detail.md).
```

- [ ] **Step 3: CLAUDE.md**

In the Performance rule, replace

```markdown
  ~/Documents/github/Templum/target/release/templum --bench-zoom=editor`; and `--bench-git` for a change to the Git
  block's diff panel).
```

with

```markdown
  ~/Documents/github/Templum/target/release/templum --bench-zoom=editor`; `--bench-far`, run the same way, for a
  change to drawing at far zoom; and `--bench-git` for a change to the Git block's diff panel).
```

- [ ] **Step 4: The spec matches the code**

In `docs/specs/level-of-detail.md`, apply what "Where the code differs from the spec" (top of this plan) found:

- Section 5's table: the Git row's last cell lists three lists, not four ("the comparison list, the commit details
  and the stash list"); "Recently discarded" is a dialog the window draws (`git_block/discarded.rs`), and it keeps
  its scroll at every level. "**These four get scroll handles**" becomes "**These three get scroll handles**".
- Section 1's table: the terminal font's range is at `config.rs:343` (re-read it; cite the lines that hold it).
- Section 5's kanban row and the Documentation section: the Card level is reachable only by terminals (13 px is
  3.25 px at 25%), so "reset at the Card level … A minor for minors.md" becomes "a kanban block never reaches the
  Card level with today's steps", and the Documentation bullet for `docs/minors.md` is deleted — unless the
  checkpoint (Task 8) turned kanban blocks into cards, in which case keep it and say so.
- Header: `**Status:** Design approved` becomes `**Status:** Implemented`.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && \
git add docs/ARCHITECTURE.md docs/specs/walking-skeleton.md docs/specs/level-of-detail.md CLAUDE.md && \
git commit -F - <<'EOF'
docs: describe the level of detail at far zoom

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

---

### Task 13: Final check, benches, manual checks and the report

No code. The plan opens no pull request: the maintainer decides.

- [ ] **Step 1: The full check**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Expected: green. Report it as it is, failures included.

- [ ] **Step 2: Benches, alternating with the "before" binary**

```bash
uptime   # low load, no other build running; otherwise wait
cd ~/Documents/github/Templum && cargo build --release -p templum
cp target/release/templum ~/Documents/github/Templum/target/lod/final
S=~/Documents/github/Templum/target/lod/alternate.sh B=~/Documents/github/Templum/target/lod/before A=~/Documents/github/Templum/target/lod/final
DQ=~/Documents/github/DaemonQuest T=~/Documents/github/Templum
$S 3 $DQ --bench-far $B $A
$S 3 $T --bench $B $A
$S 3 $T --bench-mixed $B $A
for k in empty terminal editor note kanban git; do $S 3 $DQ --bench-zoom=$k $B $A; done
$S 3 $T --bench-git $B $A
```

Expected: `--bench-far` p95 <= 16.7 ms (success criterion 4); `--bench-zoom=editor` and `=note` lower at the far end
of their sweeps; every other scenario within run-to-run noise of "before", and <= 16.7 where it was. A scenario
worse than noise is a finding: profile it before reporting (do not hide it). Paste the medians and JSON lines into
"Measurements → Final", and commit them:

```bash
git add docs/plans/2026-10-13-level-of-detail.md && \
git commit -F - <<'EOF'
docs: record the far-zoom benches after editors and notes

Claude-Session: https://claude.ai/code/session_01AsWKLBwR1jfbkj6QG9VnUV
EOF
```

- [ ] **Step 3: Report to the maintainer, in Brazilian Portuguese**

Fill in the numbers and send:

```text
**Nível de detalhe pronto (partes 1 e 2), em development, sem PR aberto.**

A 33% e 25%, além da parte 1, editores e notas sem foco viram barras das palavras, nas mesmas linhas
em que o editor as mostrava (dobras, quebra de linha e rolagem incluídas); a árvore de arquivos do
editor fica vazia; Markdown renderizado fica vazio; as listas do bloco Git (detalhes do commit,
comparação, stashes) não perdem mais a rolagem. Um arquivo que muda no disco enquanto está longe
atualiza as barras em ~2 s.

Benches (mediana de 3 rodadas alternadas, antes → depois, p95 / max em ms):
- --bench-far: … (meta: p95 ≤ 16,7)
- --bench: … ; --bench-mixed: …
- --bench-zoom=empty/terminal/editor/note/kanban/git: …
- --bench-git: …
O que olhar: <a linha que mais mudou e por quê>.
Check completo: <verde | o que falhou>. Minors novos em docs/minors.md: <lista, ou nenhum>.

Por favor confira à mão no Hyprland (cargo run --release -p templum):
1. Editores em arquivos que os agentes editam e notas com títulos, a 33% e 25%: as barras parecem as
   linhas do texto (recuo, linhas curtas e longas), na cor do texto, nos temas claro e escuro.
2. Nada se move quando o zoom termina ou quando você volta para 50%: as letras aparecem onde estavam
   as barras (inclusive numa nota rolada até a metade e com uma seção dobrada).
3. Um agente escrevendo num arquivo aberto num editor distante: as barras acompanham.
4. Clicar num editor ou nota distante foca o bloco; o próximo clique põe o cursor.
5. Digitar num editor e numa nota focados a 25% (teclas mortas, fcitx5).
6. A roda do mouse sobre editores e notas distantes move o canvas.
7. As listas do Git (um commit com mensagem longa, uma comparação, vários stashes) mantêm a rolagem
   depois de ir a 33% e voltar.
```

---

## Measurements

Filled in while executing: medians of 3 runs (alternating with the "before" binary from the checkpoint on), each
table followed by its JSON lines. Release build, from `~/Documents/github/DaemonQuest` for `--bench-far` and
`--bench-zoom`, from `~/Documents/github/Templum` for the others.

### Before (Task 1, `~/Documents/github/Templum/target/lod/before`)

Built from 85a6606. The window was tiled by Hyprland to 945 × 512 px (a quarter of a 1920 × 1080 screen), not
its 1280 × 800 default, as in earlier benches on this laptop.

| Scenario | frames | p50 | p95 | p99 | max |
|---|---|---|---|---|---|
| `--bench-far` | 1019 | 9.34 | 13.93 | 18.54 | 198.39 |
| `--bench` | 1335 | 6.27 | 13.44 | 17.97 | 35.35 |
| `--bench-mixed` | 1509 | 6.08 | 9.78 | 13.21 | 32.21 |

```
far   {"frames":983,"p50_ms":9.62,"p95_ms":14.54,"p99_ms":18.54,"max_ms":205.75,"rss_mb":286.2,"idle_rss_mb":235.3}
far   {"frames":1019,"p50_ms":9.34,"p95_ms":13.93,"p99_ms":17.15,"max_ms":198.39,"rss_mb":239.2,"idle_rss_mb":187.2}
far   {"frames":1042,"p50_ms":9.20,"p95_ms":13.47,"p99_ms":20.36,"max_ms":116.92,"rss_mb":238.1,"idle_rss_mb":187.1}
bench {"frames":1335,"p50_ms":6.27,"p95_ms":13.44,"p99_ms":17.07,"max_ms":34.80,"rss_mb":293.4,"idle_rss_mb":184.3}
bench {"frames":1362,"p50_ms":6.20,"p95_ms":12.80,"p99_ms":17.97,"max_ms":38.08,"rss_mb":289.0,"idle_rss_mb":184.6}
bench {"frames":1210,"p50_ms":6.85,"p95_ms":14.94,"p99_ms":20.52,"max_ms":35.35,"rss_mb":291.5,"idle_rss_mb":186.7}
mixed {"frames":1495,"p50_ms":6.09,"p95_ms":10.25,"p99_ms":12.66,"max_ms":33.65,"rss_mb":200.4,"idle_rss_mb":166.4}
mixed {"frames":1534,"p50_ms":6.08,"p95_ms":9.34,"p99_ms":13.21,"max_ms":31.43,"rss_mb":198.0,"idle_rss_mb":166.2}
mixed {"frames":1509,"p50_ms":6.08,"p95_ms":9.78,"p99_ms":14.29,"max_ms":32.21,"rss_mb":196.4,"idle_rss_mb":166.0}
```

The same binary on the other side of the alternation (the script's "after"): `--bench-far` p95 15.52, `--bench`
13.96, `--bench-mixed` 9.79 (medians): about 1 ms of run-to-run noise.

### Checkpoint (Task 8): before → Part 1

`before` against `~/Documents/github/Templum/target/lod/part1` (built from 40666ec), 3 alternating rounds each (5 more
for `--bench`, whose p95 and p99 came out a little higher in the first 3), window 945 × 512 px as above.

| Scenario | p50 | p95 | p99 | max | frames |
|---|---|---|---|---|---|
| `--bench-far` | 9.11 → 7.83 | 16.37 → 13.06 | 18.20 → 16.04 | 119.21 → 151.06 | 1049 → 1180 |
| `--bench` | 6.35 → 6.26 | 13.15 → 14.17 | 17.00 → 20.39 | 27.01 → 32.82 | 1287 → 1317 |
| `--bench-mixed` | 6.13 → 6.11 | 11.93 → 10.01 | 15.85 → 13.78 | 52.69 → 53.88 | 1412 → 1470 |
| `--bench-zoom=empty` | 6.06 → 6.06 | 6.27 → 6.25 | 11.63 → 11.43 | 13.68 → 13.31 | 1633 → 1633 |
| `--bench-zoom=terminal` | 6.07 → 6.07 | 8.54 → 8.33 | 11.99 → 11.72 | 13.41 → 14.59 | 1588 → 1594 |
| `--bench-zoom=editor` | 7.97 → 7.86 | 14.67 → 14.44 | 21.77 → 18.50 | 92.26 → 91.83 | 1161 → 1186 |
| `--bench-zoom=note` | 38.61 → 47.47 | 79.27 → 78.44 | 85.66 → 85.47 | 96.92 → 92.46 | 274 → 263 |
| `--bench-zoom=kanban` | 6.14 → 6.14 | 9.32 → 9.33 | 13.59 → 12.46 | 18.47 → 16.11 | 1483 → 1487 |
| `--bench-zoom=git` | 6.10 → 6.08 | 8.99 → 7.90 | 12.30 → 11.94 | 16.83 → 13.33 | 1531 → 1580 |
| `--bench-git` | 6.06 → 6.07 | 8.93 → 9.26 | 11.59 → 12.00 | 42.90 → 43.73 | 1562 → 1550 |
| `--bench`, 5 more rounds | 6.41 → 6.38 | 13.52 → 12.89 | 18.14 → 17.86 | 26.30 → 34.05 | 1303 → 1325 |

```
far           before {"frames":1037,"p50_ms":9.11,"p95_ms":16.37,"p99_ms":18.2,"max_ms":135.24,"rss_mb":236.7,"idle_rss_mb":186.5}
far           before {"frames":1067,"p50_ms":9.13,"p95_ms":13.26,"p99_ms":15.63,"max_ms":119.21,"rss_mb":239.2,"idle_rss_mb":187.1}
far           before {"frames":1049,"p50_ms":9.03,"p95_ms":16.41,"p99_ms":18.36,"max_ms":117.01,"rss_mb":238.3,"idle_rss_mb":187.2}
far           after  {"frames":1164,"p50_ms":8.13,"p95_ms":12.51,"p99_ms":14.95,"max_ms":183.05,"rss_mb":281.9,"idle_rss_mb":235.0}
far           after  {"frames":1192,"p50_ms":7.72,"p95_ms":13.99,"p99_ms":16.58,"max_ms":117.4,"rss_mb":232.4,"idle_rss_mb":187.5}
far           after  {"frames":1180,"p50_ms":7.83,"p95_ms":13.06,"p99_ms":16.04,"max_ms":151.06,"rss_mb":231.8,"idle_rss_mb":187.1}
bench         before {"frames":1354,"p50_ms":6.3,"p95_ms":12.11,"p99_ms":16.78,"max_ms":25.8,"rss_mb":289.4,"idle_rss_mb":184.6}
bench         before {"frames":1287,"p50_ms":6.35,"p95_ms":14.1,"p99_ms":19.22,"max_ms":41.4,"rss_mb":289.4,"idle_rss_mb":184.5}
bench         before {"frames":1287,"p50_ms":6.63,"p95_ms":13.15,"p99_ms":17.0,"max_ms":27.01,"rss_mb":290.4,"idle_rss_mb":184.9}
bench         after  {"frames":1317,"p50_ms":6.25,"p95_ms":14.17,"p99_ms":20.73,"max_ms":32.82,"rss_mb":289.3,"idle_rss_mb":184.4}
bench         after  {"frames":1345,"p50_ms":6.26,"p95_ms":12.97,"p99_ms":18.0,"max_ms":26.86,"rss_mb":290.1,"idle_rss_mb":184.7}
bench         after  {"frames":1246,"p50_ms":6.54,"p95_ms":14.82,"p99_ms":20.39,"max_ms":44.08,"rss_mb":290.3,"idle_rss_mb":184.8}
mixed         before {"frames":1361,"p50_ms":6.16,"p95_ms":12.6,"p99_ms":18.12,"max_ms":52.69,"rss_mb":208.9,"idle_rss_mb":166.4}
mixed         before {"frames":1415,"p50_ms":6.13,"p95_ms":11.93,"p99_ms":15.85,"max_ms":47.61,"rss_mb":208.5,"idle_rss_mb":166.5}
mixed         before {"frames":1412,"p50_ms":6.12,"p95_ms":11.69,"p99_ms":14.7,"max_ms":70.1,"rss_mb":206.8,"idle_rss_mb":166.4}
mixed         after  {"frames":1475,"p50_ms":6.11,"p95_ms":9.74,"p99_ms":13.78,"max_ms":49.34,"rss_mb":198.2,"idle_rss_mb":166.5}
mixed         after  {"frames":1457,"p50_ms":6.12,"p95_ms":11.2,"p99_ms":13.17,"max_ms":78.1,"rss_mb":199.3,"idle_rss_mb":166.8}
mixed         after  {"frames":1470,"p50_ms":6.11,"p95_ms":10.01,"p99_ms":14.28,"max_ms":53.88,"rss_mb":199.3,"idle_rss_mb":167.3}
zoom=empty    before {"frames":1633,"p50_ms":6.06,"p95_ms":6.27,"p99_ms":11.63,"max_ms":13.68,"rss_mb":160.0,"idle_rss_mb":159.7}
zoom=empty    before {"frames":1610,"p50_ms":6.06,"p95_ms":6.3,"p99_ms":12.17,"max_ms":13.87,"rss_mb":159.6,"idle_rss_mb":159.4}
zoom=empty    before {"frames":1633,"p50_ms":6.06,"p95_ms":6.25,"p99_ms":11.38,"max_ms":13.6,"rss_mb":159.4,"idle_rss_mb":159.1}
zoom=empty    after  {"frames":1633,"p50_ms":6.06,"p95_ms":6.25,"p99_ms":10.15,"max_ms":13.39,"rss_mb":160.2,"idle_rss_mb":160.0}
zoom=empty    after  {"frames":1633,"p50_ms":6.06,"p95_ms":6.24,"p99_ms":11.43,"max_ms":13.31,"rss_mb":159.9,"idle_rss_mb":159.6}
zoom=empty    after  {"frames":1611,"p50_ms":6.06,"p95_ms":6.3,"p99_ms":12.17,"max_ms":13.12,"rss_mb":160.2,"idle_rss_mb":159.9}
zoom=terminal before {"frames":1595,"p50_ms":6.07,"p95_ms":8.42,"p99_ms":10.18,"max_ms":13.24,"rss_mb":240.6,"idle_rss_mb":230.4}
zoom=terminal before {"frames":1588,"p50_ms":6.07,"p95_ms":8.54,"p99_ms":11.99,"max_ms":13.41,"rss_mb":238.7,"idle_rss_mb":228.5}
zoom=terminal before {"frames":1579,"p50_ms":6.08,"p95_ms":8.64,"p99_ms":12.01,"max_ms":13.51,"rss_mb":237.7,"idle_rss_mb":227.7}
zoom=terminal after  {"frames":1594,"p50_ms":6.07,"p95_ms":8.34,"p99_ms":11.72,"max_ms":13.87,"rss_mb":238.0,"idle_rss_mb":228.7}
zoom=terminal after  {"frames":1591,"p50_ms":6.07,"p95_ms":8.33,"p99_ms":12.05,"max_ms":14.59,"rss_mb":237.8,"idle_rss_mb":228.5}
zoom=terminal after  {"frames":1597,"p50_ms":6.07,"p95_ms":8.18,"p99_ms":11.51,"max_ms":15.67,"rss_mb":238.0,"idle_rss_mb":228.4}
zoom=editor   before {"frames":1161,"p50_ms":8.17,"p95_ms":13.68,"p99_ms":21.77,"max_ms":97.14,"rss_mb":261.1,"idle_rss_mb":174.1}
zoom=editor   before {"frames":1138,"p50_ms":7.97,"p95_ms":14.86,"p99_ms":23.33,"max_ms":91.0,"rss_mb":261.2,"idle_rss_mb":174.5}
zoom=editor   before {"frames":1181,"p50_ms":7.5,"p95_ms":14.67,"p99_ms":18.29,"max_ms":92.26,"rss_mb":263.6,"idle_rss_mb":173.9}
zoom=editor   after  {"frames":1192,"p50_ms":7.9,"p95_ms":14.11,"p99_ms":18.5,"max_ms":91.83,"rss_mb":260.7,"idle_rss_mb":174.4}
zoom=editor   after  {"frames":1172,"p50_ms":7.86,"p95_ms":14.44,"p99_ms":18.42,"max_ms":91.01,"rss_mb":260.5,"idle_rss_mb":174.1}
zoom=editor   after  {"frames":1186,"p50_ms":7.26,"p95_ms":15.15,"p99_ms":22.47,"max_ms":96.53,"rss_mb":264.9,"idle_rss_mb":173.8}
zoom=note     before {"frames":280,"p50_ms":28.91,"p95_ms":79.27,"p99_ms":85.66,"max_ms":96.92,"rss_mb":174.8,"idle_rss_mb":168.8}
zoom=note     before {"frames":274,"p50_ms":38.61,"p95_ms":75.95,"p99_ms":85.17,"max_ms":86.65,"rss_mb":174.9,"idle_rss_mb":168.9}
zoom=note     before {"frames":251,"p50_ms":48.76,"p95_ms":83.62,"p99_ms":88.13,"max_ms":96.98,"rss_mb":175.2,"idle_rss_mb":169.0}
zoom=note     after  {"frames":227,"p50_ms":54.92,"p95_ms":78.44,"p99_ms":85.79,"max_ms":89.39,"rss_mb":176.0,"idle_rss_mb":170.0}
zoom=note     after  {"frames":263,"p50_ms":47.47,"p95_ms":80.18,"p99_ms":85.47,"max_ms":95.12,"rss_mb":175.4,"idle_rss_mb":168.9}
zoom=note     after  {"frames":273,"p50_ms":42.71,"p95_ms":76.67,"p99_ms":81.72,"max_ms":92.46,"rss_mb":175.1,"idle_rss_mb":169.1}
zoom=kanban   before {"frames":1483,"p50_ms":6.12,"p95_ms":9.3,"p99_ms":12.28,"max_ms":27.88,"rss_mb":168.1,"idle_rss_mb":160.1}
zoom=kanban   before {"frames":1485,"p50_ms":6.14,"p95_ms":9.32,"p99_ms":13.59,"max_ms":17.01,"rss_mb":168.0,"idle_rss_mb":160.1}
zoom=kanban   before {"frames":1446,"p50_ms":6.14,"p95_ms":9.62,"p99_ms":14.06,"max_ms":18.47,"rss_mb":167.9,"idle_rss_mb":160.0}
zoom=kanban   after  {"frames":1479,"p50_ms":6.14,"p95_ms":9.32,"p99_ms":13.51,"max_ms":16.09,"rss_mb":168.8,"idle_rss_mb":160.9}
zoom=kanban   after  {"frames":1487,"p50_ms":6.14,"p95_ms":9.33,"p99_ms":11.95,"max_ms":16.11,"rss_mb":168.2,"idle_rss_mb":160.4}
zoom=kanban   after  {"frames":1489,"p50_ms":6.13,"p95_ms":9.33,"p99_ms":12.46,"max_ms":16.35,"rss_mb":168.0,"idle_rss_mb":160.2}
zoom=git      before {"frames":1533,"p50_ms":6.09,"p95_ms":8.99,"p99_ms":12.3,"max_ms":16.83,"rss_mb":169.6,"idle_rss_mb":163.1}
zoom=git      before {"frames":1531,"p50_ms":6.1,"p95_ms":8.99,"p99_ms":12.29,"max_ms":22.36,"rss_mb":169.3,"idle_rss_mb":163.0}
zoom=git      before {"frames":1509,"p50_ms":6.1,"p95_ms":9.41,"p99_ms":12.73,"max_ms":16.17,"rss_mb":169.0,"idle_rss_mb":162.5}
zoom=git      after  {"frames":1573,"p50_ms":6.08,"p95_ms":8.5,"p99_ms":11.94,"max_ms":13.33,"rss_mb":168.1,"idle_rss_mb":162.8}
zoom=git      after  {"frames":1580,"p50_ms":6.08,"p95_ms":7.87,"p99_ms":11.99,"max_ms":13.21,"rss_mb":168.4,"idle_rss_mb":163.1}
zoom=git      after  {"frames":1583,"p50_ms":6.08,"p95_ms":7.9,"p99_ms":11.62,"max_ms":14.43,"rss_mb":168.2,"idle_rss_mb":162.8}
git           before {"frames":1562,"p50_ms":6.06,"p95_ms":8.93,"p99_ms":11.59,"max_ms":44.47,"rss_mb":184.5,"idle_rss_mb":173.4}
git           before {"frames":1572,"p50_ms":6.05,"p95_ms":8.09,"p99_ms":11.32,"max_ms":42.8,"rss_mb":185.8,"idle_rss_mb":173.8}
git           before {"frames":1561,"p50_ms":6.07,"p95_ms":8.95,"p99_ms":12.01,"max_ms":42.9,"rss_mb":186.1,"idle_rss_mb":173.7}
git           after  {"frames":1527,"p50_ms":6.07,"p95_ms":9.44,"p99_ms":12.37,"max_ms":43.73,"rss_mb":186.2,"idle_rss_mb":173.7}
git           after  {"frames":1550,"p50_ms":6.08,"p95_ms":9.26,"p99_ms":12.0,"max_ms":45.25,"rss_mb":183.3,"idle_rss_mb":173.4}
git           after  {"frames":1551,"p50_ms":6.06,"p95_ms":9.08,"p99_ms":11.87,"max_ms":42.78,"rss_mb":187.2,"idle_rss_mb":174.0}
bench (5)     before {"frames":1419,"p50_ms":6.17,"p95_ms":11.64,"p99_ms":14.96,"max_ms":24.28,"rss_mb":289.1,"idle_rss_mb":184.4}
bench (5)     before {"frames":1315,"p50_ms":6.37,"p95_ms":13.52,"p99_ms":18.14,"max_ms":28.33,"rss_mb":289.9,"idle_rss_mb":184.6}
bench (5)     before {"frames":1267,"p50_ms":6.71,"p95_ms":13.46,"p99_ms":17.96,"max_ms":26.3,"rss_mb":289.5,"idle_rss_mb":184.1}
bench (5)     before {"frames":1240,"p50_ms":6.63,"p95_ms":15.27,"p99_ms":19.95,"max_ms":27.38,"rss_mb":292.0,"idle_rss_mb":186.2}
bench (5)     before {"frames":1303,"p50_ms":6.41,"p95_ms":13.93,"p99_ms":19.52,"max_ms":24.23,"rss_mb":289.6,"idle_rss_mb":184.9}
bench (5)     after  {"frames":1352,"p50_ms":6.21,"p95_ms":13.19,"p99_ms":17.86,"max_ms":37.08,"rss_mb":291.2,"idle_rss_mb":185.6}
bench (5)     after  {"frames":1325,"p50_ms":6.36,"p95_ms":12.89,"p99_ms":17.44,"max_ms":25.93,"rss_mb":290.7,"idle_rss_mb":185.2}
bench (5)     after  {"frames":1352,"p50_ms":6.38,"p95_ms":12.31,"p99_ms":17.04,"max_ms":34.05,"rss_mb":294.5,"idle_rss_mb":185.6}
bench (5)     after  {"frames":1276,"p50_ms":6.47,"p95_ms":14.47,"p99_ms":20.12,"max_ms":38.3,"rss_mb":290.6,"idle_rss_mb":185.1}
bench (5)     after  {"frames":1318,"p50_ms":6.46,"p95_ms":12.34,"p99_ms":17.92,"max_ms":31.59,"rss_mb":293.0,"idle_rss_mb":187.8}
```

### Final (Task 13): before → Parts 1 and 2

`before` against `~/Documents/github/Templum/target/lod/final` (built from bea0960), 3 alternating rounds each,
from 15:09 to 15:23 (load average 0.99 at the start; no other build ran). The window was tiled by Hyprland to
945 × 512 px as above; no other Templum was open (the windows titled "Templum — …" are Zed).

| Scenario | p50 | p95 | p99 | max | frames |
|---|---|---|---|---|---|
| `--bench-far` | 9.14 → 6.07 | 16.03 → 9.10 | 18.16 → 13.04 | 119.90 → 123.77 | 1043 → 1541 |
| `--bench` | 6.25 → 6.29 | 12.70 → 12.36 | 16.65 → 17.20 | 25.74 → 28.32 | 1345 → 1354 |
| `--bench-mixed` | 6.08 → 6.07 | 9.84 → 7.82 | 13.26 → 12.09 | 20.72 → 31.19 | 1510 → 1582 |
| `--bench-zoom=empty` | 6.06 → 6.06 | 6.27 → 6.25 | 11.48 → 11.33 | 13.42 → 15.23 | 1632 → 1633 |
| `--bench-zoom=terminal` | 6.07 → 6.07 | 8.50 → 8.39 | 12.02 → 12.08 | 13.60 → 13.48 | 1588 → 1595 |
| `--bench-zoom=editor` | 7.20 → 6.30 | 14.40 → 12.67 | 18.68 → 17.11 | 90.66 → 84.45 | 1180 → 1285 |
| `--bench-zoom=note` | 27.88 → 12.03 | 76.43 → 57.02 | 86.07 → 74.08 | 94.02 → 86.18 | 277 → 476 |
| `--bench-zoom=kanban` | 6.14 → 6.14 | 9.41 → 9.25 | 12.59 → 12.38 | 17.02 → 15.26 | 1478 → 1490 |
| `--bench-zoom=git` | 6.10 → 6.07 | 9.04 → 8.07 | 12.62 → 11.77 | 22.25 → 16.02 | 1532 → 1578 |
| `--bench-git` | 6.06 → 6.07 | 8.88 → 8.63 | 11.95 → 11.96 | 42.65 → 44.22 | 1563 → 1569 |

`--bench`'s max: before 25.74, 30.90, 23.01; after 25.09, 36.36, 28.32. A probe that logged every frame over 18 ms
(not committed) put the worst ones of both binaries in glides, at 26–46% and at 110–200%; in 3 more alternating
runs right after a build (load 10) the maxima were before 32.3, 29.1, 35.7 and after 29.7, 29.2, 23.9: run-to-run
noise, no cost found. `--bench-mixed`'s max moves as much between sessions (before 18.7–31.5 here, 47.6–70.1 at the
checkpoint) while its p95 drops by 2 ms.

```
far           before {"frames":1043,"p50_ms":9.08,"p95_ms":16.15,"p99_ms":18.16,"max_ms":119.0,"rss_mb":237.2,"idle_rss_mb":186.6}
far           before {"frames":1025,"p50_ms":9.22,"p95_ms":16.03,"p99_ms":19.85,"max_ms":119.9,"rss_mb":237.7,"idle_rss_mb":186.6}
far           before {"frames":1052,"p50_ms":9.14,"p95_ms":14.9,"p99_ms":17.18,"max_ms":149.38,"rss_mb":238.0,"idle_rss_mb":186.7}
far           after  {"frames":1541,"p50_ms":6.08,"p95_ms":9.1,"p99_ms":13.25,"max_ms":123.77,"rss_mb":272.7,"idle_rss_mb":235.6}
far           after  {"frames":1526,"p50_ms":6.07,"p95_ms":9.26,"p99_ms":12.91,"max_ms":194.27,"rss_mb":224.8,"idle_rss_mb":187.9}
far           after  {"frames":1548,"p50_ms":6.07,"p95_ms":8.55,"p99_ms":13.04,"max_ms":122.16,"rss_mb":225.1,"idle_rss_mb":187.8}
bench         before {"frames":1382,"p50_ms":6.25,"p95_ms":11.93,"p99_ms":16.65,"max_ms":25.74,"rss_mb":289.2,"idle_rss_mb":184.8}
bench         before {"frames":1345,"p50_ms":6.16,"p95_ms":13.79,"p99_ms":18.96,"max_ms":30.9,"rss_mb":289.2,"idle_rss_mb":184.5}
bench         before {"frames":1313,"p50_ms":6.55,"p95_ms":12.7,"p99_ms":16.45,"max_ms":23.01,"rss_mb":290.0,"idle_rss_mb":184.8}
bench         after  {"frames":1334,"p50_ms":6.29,"p95_ms":12.99,"p99_ms":18.42,"max_ms":25.09,"rss_mb":290.3,"idle_rss_mb":184.8}
bench         after  {"frames":1361,"p50_ms":6.22,"p95_ms":12.36,"p99_ms":17.2,"max_ms":36.36,"rss_mb":293.6,"idle_rss_mb":184.9}
bench         after  {"frames":1354,"p50_ms":6.35,"p95_ms":12.28,"p99_ms":16.03,"max_ms":28.32,"rss_mb":290.6,"idle_rss_mb":184.8}
mixed         before {"frames":1542,"p50_ms":6.08,"p95_ms":9.34,"p99_ms":13.26,"max_ms":20.72,"rss_mb":196.4,"idle_rss_mb":166.3}
mixed         before {"frames":1506,"p50_ms":6.09,"p95_ms":9.85,"p99_ms":13.19,"max_ms":18.72,"rss_mb":197.0,"idle_rss_mb":166.3}
mixed         before {"frames":1510,"p50_ms":6.08,"p95_ms":9.84,"p99_ms":13.62,"max_ms":31.54,"rss_mb":197.5,"idle_rss_mb":165.5}
mixed         after  {"frames":1582,"p50_ms":6.07,"p95_ms":7.82,"p99_ms":11.98,"max_ms":46.37,"rss_mb":193.5,"idle_rss_mb":166.3}
mixed         after  {"frames":1589,"p50_ms":6.07,"p95_ms":7.82,"p99_ms":12.09,"max_ms":31.19,"rss_mb":192.4,"idle_rss_mb":166.4}
mixed         after  {"frames":1580,"p50_ms":6.07,"p95_ms":7.97,"p99_ms":12.45,"max_ms":24.54,"rss_mb":193.5,"idle_rss_mb":166.0}
zoom=empty    before {"frames":1633,"p50_ms":6.06,"p95_ms":6.27,"p99_ms":11.48,"max_ms":13.29,"rss_mb":162.9,"idle_rss_mb":159.2}
zoom=empty    before {"frames":1595,"p50_ms":6.06,"p95_ms":6.6,"p99_ms":12.22,"max_ms":18.55,"rss_mb":158.9,"idle_rss_mb":158.6}
zoom=empty    before {"frames":1632,"p50_ms":6.06,"p95_ms":6.27,"p99_ms":11.47,"max_ms":13.42,"rss_mb":159.3,"idle_rss_mb":159.0}
zoom=empty    after  {"frames":1633,"p50_ms":6.06,"p95_ms":6.24,"p99_ms":11.33,"max_ms":13.37,"rss_mb":159.7,"idle_rss_mb":159.5}
zoom=empty    after  {"frames":1633,"p50_ms":6.06,"p95_ms":6.25,"p99_ms":11.33,"max_ms":15.23,"rss_mb":159.6,"idle_rss_mb":159.4}
zoom=empty    after  {"frames":1584,"p50_ms":6.06,"p95_ms":6.7,"p99_ms":12.2,"max_ms":19.22,"rss_mb":159.8,"idle_rss_mb":159.5}
zoom=terminal before {"frames":1588,"p50_ms":6.07,"p95_ms":8.46,"p99_ms":11.94,"max_ms":17.42,"rss_mb":237.9,"idle_rss_mb":227.8}
zoom=terminal before {"frames":1582,"p50_ms":6.07,"p95_ms":8.69,"p99_ms":12.02,"max_ms":13.41,"rss_mb":237.6,"idle_rss_mb":227.5}
zoom=terminal before {"frames":1589,"p50_ms":6.07,"p95_ms":8.5,"p99_ms":12.08,"max_ms":13.6,"rss_mb":237.8,"idle_rss_mb":227.5}
zoom=terminal after  {"frames":1572,"p50_ms":6.07,"p95_ms":8.83,"p99_ms":12.17,"max_ms":14.88,"rss_mb":237.7,"idle_rss_mb":228.6}
zoom=terminal after  {"frames":1595,"p50_ms":6.07,"p95_ms":8.39,"p99_ms":12.05,"max_ms":13.33,"rss_mb":238.3,"idle_rss_mb":229.1}
zoom=terminal after  {"frames":1596,"p50_ms":6.06,"p95_ms":8.13,"p99_ms":12.08,"max_ms":13.48,"rss_mb":238.3,"idle_rss_mb":229.1}
zoom=editor   before {"frames":1198,"p50_ms":7.2,"p95_ms":14.14,"p99_ms":18.38,"max_ms":90.66,"rss_mb":262.5,"idle_rss_mb":174.0}
zoom=editor   before {"frames":1146,"p50_ms":6.76,"p95_ms":15.58,"p99_ms":23.46,"max_ms":97.33,"rss_mb":262.4,"idle_rss_mb":173.9}
zoom=editor   before {"frames":1180,"p50_ms":7.72,"p95_ms":14.4,"p99_ms":18.68,"max_ms":90.18,"rss_mb":262.9,"idle_rss_mb":174.4}
zoom=editor   after  {"frames":1332,"p50_ms":6.16,"p95_ms":12.13,"p99_ms":16.55,"max_ms":84.16,"rss_mb":255.4,"idle_rss_mb":174.3}
zoom=editor   after  {"frames":1282,"p50_ms":6.39,"p95_ms":13.25,"p99_ms":17.43,"max_ms":84.45,"rss_mb":258.2,"idle_rss_mb":174.1}
zoom=editor   after  {"frames":1285,"p50_ms":6.3,"p95_ms":12.67,"p99_ms":17.11,"max_ms":92.9,"rss_mb":257.6,"idle_rss_mb":174.3}
zoom=note     before {"frames":290,"p50_ms":20.29,"p95_ms":76.4,"p99_ms":86.07,"max_ms":95.95,"rss_mb":174.8,"idle_rss_mb":168.8}
zoom=note     before {"frames":277,"p50_ms":27.88,"p95_ms":76.43,"p99_ms":86.02,"max_ms":88.89,"rss_mb":174.5,"idle_rss_mb":168.6}
zoom=note     before {"frames":267,"p50_ms":47.07,"p95_ms":77.46,"p99_ms":86.5,"max_ms":94.02,"rss_mb":178.7,"idle_rss_mb":169.2}
zoom=note     after  {"frames":476,"p50_ms":12.03,"p95_ms":55.97,"p99_ms":74.82,"max_ms":89.38,"rss_mb":175.3,"idle_rss_mb":169.2}
zoom=note     after  {"frames":478,"p50_ms":11.98,"p95_ms":57.02,"p99_ms":74.08,"max_ms":86.18,"rss_mb":175.2,"idle_rss_mb":169.1}
zoom=note     after  {"frames":468,"p50_ms":12.05,"p95_ms":64.25,"p99_ms":73.7,"max_ms":79.81,"rss_mb":175.2,"idle_rss_mb":169.1}
zoom=kanban   before {"frames":1486,"p50_ms":6.14,"p95_ms":9.33,"p99_ms":12.48,"max_ms":16.88,"rss_mb":168.1,"idle_rss_mb":160.1}
zoom=kanban   before {"frames":1478,"p50_ms":6.13,"p95_ms":9.41,"p99_ms":12.77,"max_ms":17.02,"rss_mb":167.8,"idle_rss_mb":159.9}
zoom=kanban   before {"frames":1463,"p50_ms":6.15,"p95_ms":9.49,"p99_ms":12.59,"max_ms":23.17,"rss_mb":168.1,"idle_rss_mb":160.3}
zoom=kanban   after  {"frames":1471,"p50_ms":6.14,"p95_ms":9.39,"p99_ms":12.75,"max_ms":24.58,"rss_mb":168.4,"idle_rss_mb":160.7}
zoom=kanban   after  {"frames":1493,"p50_ms":6.14,"p95_ms":9.25,"p99_ms":12.38,"max_ms":14.98,"rss_mb":168.3,"idle_rss_mb":160.5}
zoom=kanban   after  {"frames":1490,"p50_ms":6.13,"p95_ms":9.22,"p99_ms":12.33,"max_ms":15.26,"rss_mb":168.3,"idle_rss_mb":160.4}
zoom=git      before {"frames":1532,"p50_ms":6.1,"p95_ms":9.0,"p99_ms":12.22,"max_ms":24.09,"rss_mb":169.0,"idle_rss_mb":162.7}
zoom=git      before {"frames":1532,"p50_ms":6.1,"p95_ms":9.04,"p99_ms":12.62,"max_ms":14.41,"rss_mb":169.0,"idle_rss_mb":162.7}
zoom=git      before {"frames":1515,"p50_ms":6.09,"p95_ms":9.36,"p99_ms":13.03,"max_ms":22.25,"rss_mb":169.0,"idle_rss_mb":162.6}
zoom=git      after  {"frames":1572,"p50_ms":6.07,"p95_ms":8.5,"p99_ms":12.08,"max_ms":16.02,"rss_mb":168.2,"idle_rss_mb":162.9}
zoom=git      after  {"frames":1578,"p50_ms":6.08,"p95_ms":8.07,"p99_ms":11.77,"max_ms":14.15,"rss_mb":168.6,"idle_rss_mb":163.2}
zoom=git      after  {"frames":1585,"p50_ms":6.07,"p95_ms":7.56,"p99_ms":11.48,"max_ms":16.17,"rss_mb":168.4,"idle_rss_mb":162.9}
git           before {"frames":1563,"p50_ms":6.07,"p95_ms":8.88,"p99_ms":12.04,"max_ms":42.61,"rss_mb":185.6,"idle_rss_mb":173.7}
git           before {"frames":1563,"p50_ms":6.06,"p95_ms":8.79,"p99_ms":11.88,"max_ms":42.65,"rss_mb":186.3,"idle_rss_mb":173.9}
git           before {"frames":1560,"p50_ms":6.06,"p95_ms":8.9,"p99_ms":11.95,"max_ms":43.03,"rss_mb":185.0,"idle_rss_mb":173.2}
git           after  {"frames":1523,"p50_ms":6.09,"p95_ms":9.43,"p99_ms":12.52,"max_ms":45.03,"rss_mb":187.8,"idle_rss_mb":174.6}
git           after  {"frames":1569,"p50_ms":6.07,"p95_ms":8.63,"p99_ms":11.96,"max_ms":44.22,"rss_mb":184.5,"idle_rss_mb":173.6}
git           after  {"frames":1573,"p50_ms":6.06,"p95_ms":8.36,"p99_ms":11.4,"max_ms":43.99,"rss_mb":185.7,"idle_rss_mb":173.8}
```
