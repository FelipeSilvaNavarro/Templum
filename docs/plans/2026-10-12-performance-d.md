# Performance, batch D (zoom): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Zooming a canvas of editor, note and Git blocks stops stuttering and flickering: their text is laid out once per zoom step instead of at every frame of the zoom glide.

**Architecture:** A zoom glides from one step to the next over 150 ms through zooms rounded to 0.02 (`CameraFlight`), and every block lays itself out at the frame's zoom. gpui-kit's editor element reports its font size every frame and, on a new size, re-measures the whole document line by line — so a glide costs 10–20 full re-measures per document, and the rounding makes the text jitter. GPUI can't scale what it already painted (only SVG sprites take a transformation; glyphs and quads don't), so the fix is in what we feed the editor: the bodies of editor, note and Git blocks are drawn at the zoom step the camera is going to (`canvas.camera`), while the block's frame keeps gliding with the drawn `view`. One layout per step; the glyph atlas only ever sees step sizes for that text. Terminals and kanban keep gliding: they are cheap.

**Tech Stack:** Rust 2024, GPUI through `gpui-kit = "=0.7.1"` (gpui-pre 0.3.8, gpui-base 0.7.1 editor), `templum-canvas`'s `Camera`/`CameraFlight`. Tests: `gpui_kit::test` workspace tests in `crates/templum/src/workspace/tests/`.

**Spec:** the measurements and source facts below (2026-10-12), plus the user's report: "blocos de editor ao realizar zoom in e zoom out as fontes ficam piscando bastante e mudando de tamanho".

## Findings this plan implements (the spec)

`templum --bench-zoom=<kind>` (written in the working tree, Task 1 commits it): 10 blocks of one kind on `~/Documents/github/DaemonQuest`, zoom gliding step by step from 25% to 200% and back, 4 steps a second, no pan. Release build, RTX 3050, 165 Hz, 3 interleaved rounds, medians:

| Blocks | Frames / 10 s | p50 | p95 | p99 | max |
|---|---|---|---|---|---|
| empty canvas | 1617 | 6.06 | 6.36 | 12.1 | 14 |
| 10 terminals (a file `cat`ed) | 1569 | 6.08 | 8.97 | 11.9 | 16 |
| 10 kanban (a board, 12 cards) | 1476 | 6.13 | 9.42 | 12.1 | 15 |
| 10 Git blocks | 1474 | 6.20 | 9.54 | 12.1 | 17 |
| **10 editors** (`DaemonQuest.md`, 90 KB, open) | 1076 | 8.32 | **17.2** | 24.1 | **69** (every round) |
| **10 notes** (`README.md` as text, soft-wrapped) | 808 | 6.67 | **36.5** | 60.0 | **88** |

Results after Task 2 (3 alternating runs against the Task 1 build, medians, ms):

| Blocks | p50 | p95 | p99 | max | frames |
|---|---|---|---|---|---|
| 10 editors, before → after | 8.66 → 6.81 | 17.59 → **13.28** | 22.86 → 18.80 | 71 → 75 | 1059 → 1202 |
| 10 notes, before → after | 6.57 → 6.15 | 37.29 → **12.50** | 61.2 → 50.9 | 81 → 93 | 823 → 1172 |
| 10 terminals (control) | 6.07 → 6.07 | 9.03 → 8.96 | 11.98 → 12.02 | 18 → 18 | 1568 → 1573 |

Found during Task 2: a note's text never zoomed — gpui-component's `Editor` sets the theme's code size on
itself (`gpui-component-0.7.1/src/input/editor.rs:144`), over `note.rs`'s parent `.text_size`, so only the
note's wrap width changed with the zoom, re-wrapping the whole note every glide frame. The size now goes on the
editor itself, as `editor.rs` does. The one whole-document re-measure per zoom step remains (max ~75–90 ms with
10 documents of 90 KB): upstream.

Why (pinned sources under `~/.cargo/registry/src/*/`):
- The editor element sets its font every prepaint: `gpui-base-0.7.1/src/input/base/element.rs:2525-2531` (`state.display_map.set_font(font, text_size, cx)`), with `text_size` from the parent's text style — Templum's `.text_size(px(FONT * z))` (`editor.rs:805,825`, `note.rs:108`, `git_block/changes.rs:111`, `history.rs:231`, `stashes.rs:118`, `commit_form.rs:45`).
- A new size re-measures everything: `display_map/text_wrapper.rs:337-345` (`set_font`: `if self.font.eq(&font) && self.font_size == font_size { return; } … self.update_all(…)`), `:621-623` (`update_all` = `update` over `0..text.len()`), `:365-440` (per line: `measured_wrap_boundaries` + `layout_line`, or `line_wrapper.wrap_line`). Soft-wrapped notes also recompute every wrap point, hence the worst numbers and the visible jumps.
- A glide draws 6 distinct sizes for 1.0→1.1 and up to 22 for 2.0→2.5 (`templum-canvas/src/flight.rs:30`, `FLIGHT_ZOOM_STEP = 0.02`); each is a new `font_size` for the wrapper.
- GPUI has no paint-time scale: `gpui-pre-0.3.8/src/window.rs:4748,4758` paint glyphs with `TransformationMatrix::unit()`; only `paint_svg` (`:4854`) takes a transformation; quads none. So the text can't be scaled "like a photo" during the glide.
- `Workspace.space.canvas.camera` is where the camera is going (always a step); `space.view` is the frame drawn (`workspace/render.rs:224-239`, `view.rs:84-100`); `block::render` computes `z = ws.space.view.zoom() * look.scale` (`block.rs:48`) and hands it to every body (`block.rs:298-307`).
- `EditorState::line_height()` is public and reflects the last layout's font size (`gpui-base-0.7.1/src/input/base/state.rs:3009-3012`): a test can observe which zoom an editor was laid out at.

## Global Constraints

- Only `crates/templum` may depend on the UI framework (AGENTS.md); `gpui-kit = "=0.7.1"` stays pinned; no new dependencies; no `unsafe`; no copyleft code.
- Every logic change ships a test that fails without it; UI-only changes state how they were verified.
- Commits on `development`, Conventional Commits with an imperative sentence; `perf:` lines are read by users in the CHANGELOG.
- Before "done": `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, plus the benches named in each task (the p95 ≤ 16.7 bar).
- Measure with interleaved runs (the laptop's frame times drift ~1 ms as it warms up); a scratch script that alternates two binaries and prints medians was used for batches A/B.
- Every API used must exist in the pinned sources quoted above; read them before writing, never guess.

## Review Focus

1. Zooming with the wheel several times quickly (a new glide retargets mid-glide): the text must land on the final step, never on an intermediate zoom — Task 2's test zooms twice before landing.
2. A block popping in (`look.scale` 0.88→1.02) must still scale its text with the pop, as today — Task 2 keeps `look.scale` in the settled zoom; the existing `new_terminals_pop_in_then_settle` test and a visual check cover it.
3. A note in preview mode (Markdown rendered by `TextView`) must behave like the editor mode — Task 2 passes the settled zoom to the whole note body, both modes.
4. A Git block's diff, history diff, stash diff and commit form all use editors — Task 2 passes the settled zoom to the whole Git body, so every `EditorView` in `changes.rs`, `history.rs`, `stashes.rs`, `commit_form.rs` gets it.
5. When the camera isn't gliding (pan, idle, startup centering), the settled zoom equals the drawn zoom — Task 2's test asserts the landed line height equals the mid-glide one and differs from the 100% one.

---

### Task 1: Commit the zoom benches

**Files:**
- Already modified in the working tree (uncommitted): `crates/templum/src/bench.rs` (`run_zoom`, `measure`, `sample_file`, `sample_board`; `run` now uses `measure`), `crates/templum/src/main.rs` (`--bench-zoom=<kind>`)
- Modify: `AGENTS.md:44-47` (the Performance rule)

**Interfaces:**
- Produces: `templum --bench-zoom=<empty|terminal|editor|note|kanban|git>`: 10 blocks of that kind (none for `empty`) on the folder Templum runs from, zoom gliding 25%↔200% around the view's center for 10 s, one JSON line, quit. Editors open the largest `.rs`/`.md`/`.toml` file under the folder; notes hold its `README.md`; kanban blocks show a board with 12 cards; terminals `cat` that file.

- [ ] **Step 1: Check the tree is what this plan measured**

Run: `git status --short` — expected: `M crates/templum/src/bench.rs`, `M crates/templum/src/main.rs`, and this plan file. Run: `cargo clippy --workspace --all-targets -- -D warnings` — expected: clean.

- [ ] **Step 2: AGENTS.md**

In the "Performance" rule, after "(and `--bench-mixed`, an agent cockpit: TUIs, editors, Git, a note, kanban)", add: " and `--bench-zoom=<kind>` for a change to zooming — run it from a real project folder, e.g. `cd ~/Documents/github/DaemonQuest && ~/Documents/github/Templum/target/release/templum --bench-zoom=editor`".

- [ ] **Step 3: Run one scenario**

Run: `cargo build --release -p templum && (cd ~/Documents/github/DaemonQuest && ~/Documents/github/Templum/target/release/templum --bench-zoom=note)`
Expected: 10 notes open with the README's text, the zoom glides, one JSON line prints after ~13 s (p95 in the 30s, max in the 80s: the baseline this plan beats), the app quits.

- [ ] **Step 4: Full check and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/templum/src/bench.rs crates/templum/src/main.rs AGENTS.md docs/plans/2026-10-12-performance-d.md
git commit -m "chore: add --bench-zoom, ten blocks of one kind under an animated zoom

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

### Task 2: Lay out editor, note and Git bodies at the zoom step the camera is going to

**Files:**
- Modify: `crates/templum/src/block.rs:46-56` (the zooms) and `:298-307` (the bodies)
- Test: `crates/templum/src/workspace/tests/navigation.rs` (next to `zoom_keys_glide_to_the_step`)

**Interfaces:**
- Consumes: `Workspace.space.{view, canvas.camera, flight}`; `motion::Look`; `Workspace::add_note_at(at, window, cx)` (`workspace/blocks.rs:50`), `Workspace::zoom_to(delta, anchor, cx)` and `Workspace::center(&window)` (`workspace/view.rs:77,103`, both `pub(crate)`); `Body::note()` (`workspace/mod.rs:165`); `Note.editor: Entity<EditorState>` (`note.rs:22`); `EditorState::line_height() -> Option<Pixels>`; test helpers `open`, `restart_glide` (`workspace/tests/mod.rs`), `crate::motion::FLIGHT`.
- Produces: `editor::body`, `note::body`, `git_block::body` receive the settled zoom; `kanban::body` and the terminal grid keep the drawn zoom. No signature changes.

What the user sees: during the 150 ms glide, the text, tree rows and tabs of an editor, note or Git block are already at the size of the step the zoom is going to, while the block's frame finishes gliding there (for a zoom in, the frame catches up with slightly clipped content for a few frames; for a zoom out, with a small gap). No flicker, no size jitter, and a glide no longer costs a document re-measure per frame. Terminals and kanban blocks glide exactly as before.

- [ ] **Step 1: Write the failing test**

Append to `crates/templum/src/workspace/tests/navigation.rs`:

```rust
/// A note's text is laid out at the zoom step the camera is going to, not at the zooms a glide
/// passes through: gpui-kit's editor re-measures its whole document on every font size.
#[gpui_kit::test]
fn a_notes_text_lands_on_the_zoom_step_before_the_glide_does(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.add_note_at(Point::new(40.0, 40.0), window, cx);
            ws.space.appearing.clear();
        });
        window.render_frame(cx);
    })
    .unwrap();
    let note = ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        ws.space.blocks[&id].note().unwrap().editor.clone()
    });
    let line_height = |cx: &mut TestAppContext| note.read_with(cx, |e, _| e.line_height().unwrap());
    let at_100 = line_height(cx);
    // Two steps, the second asked mid-glide (as two wheel notches do): the target is 1.25,
    // the glide passes through zooms in between. Called directly: the note has the keyboard.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        ws.update(cx, |ws, cx| {
            let pivot = ws.center(window);
            ws.zoom_to(1, pivot, cx);
            ws.zoom_to(1, pivot, cx);
        });
        ws.update(cx, restart_glide);
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(40));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.zoom(), 1.25);
        let z = ws.space.view.zoom();
        assert!(ws.space.flight.is_some() && z > 1.0 && z < 1.25, "mid-glide at {z}");
    });
    let mid_glide = line_height(cx);
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.space.flight.is_none(), "landed"));
    let landed = line_height(cx);
    assert!(landed > at_100, "125% lays the text out larger: {at_100:?} -> {landed:?}");
    assert_eq!(
        mid_glide, landed,
        "mid-glide the text is already at the step's size"
    );
}
```

`restart_glide` resets the glide's clock, so the 40 ms sleep puts the next frame about a quarter of the way through it (eased: zoom ≈ 1.1–1.15, rounded to 0.02), strictly between the ends.

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum --bin templum a_notes_text_lands_on_the_zoom_step`
Expected: FAIL on the last assertion — `mid_glide` is the line height for a zoom between 1.0 and 1.25 (e.g. 13 × 1.12 px-based), `landed` is for 1.25. If it fails on `mid-glide at …` instead, the sleep missed the glide window: use 30 ms and re-run; the window is 150 ms.

- [ ] **Step 3: The settled zoom**

In `block.rs`, after `let z = ws.space.view.zoom() * look.scale;`:

```rust
    // Text-heavy bodies lay out at the step the zoom is going to, not at each zoom a glide
    // passes through: gpui-kit's editor re-measures its whole document on every font size.
    // The frame still glides (`r`), so the content is at its final size a few frames early.
    let settled = ws.space.canvas.camera.zoom() * look.scale;
```

and in the bodies match:

```rust
        // A fading note is drawn empty, like a kanban block.
        Body::Note(n) => frame.when(live, |el| el.child(note::body(ws, id, n, settled, cx))),
        // A fading editor is drawn empty, like a note.
        Body::Editor(e) => {
            frame.when(live, |el| el.child(editor::body(ws, id, e, settled, cx)))
        }
        // A fading Git block is drawn empty, like a note.
        Body::Git(g) => {
            frame.when(live, |el| el.child(git_block::body(ws, id, g, settled, cx)))
        }
```

(`Body::Terminal` and `Body::Kanban` keep `z`.)

- [ ] **Step 4: Run the test**

Run: `cargo test -p templum --bin templum a_notes_text_lands_on_the_zoom_step`
Expected: PASS. Then `cargo test -p templum --bin templum navigation editor` — the other zoom and editor tests still pass.

- [ ] **Step 5: Full check, benches, screenshots**

Run the full check. Then, release build, from `~/Documents/github/DaemonQuest`, 3 interleaved rounds each of `--bench-zoom=editor`, `--bench-zoom=note` and `--bench-zoom=terminal` (the terminal one is the control: it must not change):

Expected: editor and note p95 under 16.7 ms; their `max` drops but stays well above the others — one whole-document re-measure per step remains (every 250 ms in the bench, 10 documents of 90 KB), and that is gpui-kit's to fix (its wrapper measures the whole rope on a size change; a visible-range-only measure would be an upstream change). Note the numbers for the commit body.

Visual: open the app with an editor on a real file and a note with text, zoom in and out with Ctrl+wheel and with the keys: the text no longer flickers or jitters during the glide; it is at the new size at once while the frame finishes. Take a screenshot mid-glide with `grim` for the PR.

- [ ] **Step 6: Commit**

```bash
git add crates/templum/src/block.rs crates/templum/src/workspace/tests/navigation.rs
git commit -m "perf(canvas): lay out editor, note and Git text once per zoom step

Zooming no longer re-measures their documents at every frame of the glide,
which made the text flicker and change size and stalled frames for 70-90 ms:
their bodies are drawn at the step the zoom is going to while the block's
frame glides there. --bench-zoom=editor p95 <before> -> <after> ms, max <before> -> <after>;
note p95 <before> -> <after>, max <before> -> <after>; terminals unchanged.

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

Replace `<before>`/`<after>` with the measured medians; never commit the placeholders.

---

### Task 3: Docs

**Files:**
- Modify: `docs/ARCHITECTURE.md:50-54` (the Zoom bullet)
- Modify: `docs/plans/2026-10-12-performance-d.md` (results)

- [ ] **Step 1: ARCHITECTURE.md**

Replace the Zoom bullet with:

```markdown
- **Zoom:** positions, sizes and font size are multiplied by the zoom factor and laid out again, so text is
  rasterized at its real size and stays sharp. Zoom snaps to fixed steps to keep the glyph cache bounded;
  far-away blocks are drawn simplified (level of detail). A zoom glides to its step over 150 ms through
  zooms snapped to 0.02 (`templum-canvas`'s `CameraFlight`); the camera holds the target, the workspace's `view`
  is what is drawn. Terminals and kanban blocks glide with the view; editor, note and Git bodies are laid out
  at the camera's step from the first frame of the glide (gpui-kit's editor re-measures its whole document on
  every font size), while their frames glide.
```

- [ ] **Step 2: Results in this plan**

Under "Findings", add a "Results" table with the before/after medians from Task 2 Step 5 (editor, note, terminal: p50 / p95 / p99 / max).

- [ ] **Step 3: Full check and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add docs/ARCHITECTURE.md docs/plans/2026-10-12-performance-d.md
git commit -m "docs: say which blocks glide with the zoom and which land on the step first

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

## Out of scope (deliberately)

- The one re-measure per zoom step that remains for editors and notes: gpui-kit's `TextWrapper::set_font` measures the whole document; a visible-range-only measure is an upstream change to gpui-base.
- Terminal rows re-shaped at every 0.02 zoom of a glide: measured cheap (`--bench-zoom=terminal` p95 9.0 ms).
- Batch C (blocks as `Entity::cached` views): helps a still canvas, not pan or zoom (`gpui-pre-0.3.8/src/view.rs:485` reuses only at identical bounds); on hold by the user's choice.
- Coarser `FLIGHT_ZOOM_STEP`/`SCALE_STEP`: unnecessary once the heavy bodies ignore the intermediate zooms.
