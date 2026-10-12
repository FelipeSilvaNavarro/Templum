# Performance, batches A and B: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `templum --bench` back under the p95 ≤ 16.7 ms bar and remove the per-frame work that makes a canvas of busy terminals stutter, without changing what the user sees (except a slightly sparser dot grid between zoom steps).

**Architecture:** GPUI is immediate-mode: the whole element tree is rebuilt every frame, which is fine; what is not fine is work done per frame whose result never changes. Batch A removes fixed per-frame costs (dot grid as one scene layer with a dot budget, drawings painted without copying, terminals re-armed only when painted and never waiting on a whole PTY read pass) and adds the tooling to measure (optimized deps in dev builds, a `--bench-mixed` scenario like the user's canvas). Batch B makes a terminal's snapshot lazy: the grid is copied only after the terminal changed (a `dirty` flag set by the PTY thread and by the UI calls that touch the grid), and only the rows alacritty reports as damaged are rebuilt, reusing the row buffers.

**Tech Stack:** Rust 2024, GPUI through `gpui-kit = "=0.7.1"` (gpui-pre 0.3.8, wgpu renderer on Linux), `alacritty_terminal = "=0.26.0"`. Tests: `cargo test --workspace`; `gpui_kit::test` for workspace tests (`crates/templum/src/workspace/tests/`), plain `#[test]` with the headless terminal in `templum-terminal`.

**Spec:** the findings below (measured on 2026-10-12, release build, RTX 3050, 165 Hz, Hyprland) are what this plan implements. The full map with sources lives in the session that produced it; the numbers to beat are in "Baseline".

## Findings this plan implements (the spec)

Baseline `cargo run --release -p templum -- --bench`: p50 8.0–9.6 ms, **p95 16.5–19 ms**, p99 ~25, max 35–50 (run-to-run noise ±1 ms p50 / ±1.5 ms p95). With everything toggled off the floor is p50 6.06 / p95 6.24 (the 165 Hz vsync). Each cost measured by switching it off alone:

| Cost | p95 with it off | Evidence |
|---|---|---|
| 5 terminals printing (`yes`) | 9.5 | `grid.rs:181-184` requests a frame whenever `try_lock` fails; after 100 ms `snapshot.rs:122` takes alacritty's **fair** lock, which waits for the PTY thread's whole `pty_read` pass (`alacritty_terminal/src/event_loop.rs:117`, lease held for the call; `src/sync.rs:33-43`: `lock` waits on `next`, `lock_unfair` only on `data`). `render.rs:188-190` re-arms every terminal of the space, on screen or not, so an off-screen busy terminal keeps the window redrawing. |
| dot grid | 17.0 | `render.rs:103-124`: one rounded `paint_quad` per dot, 3,700 at zoom 1 and 8,200 at zoom 0.67 on 1920×1080 (`dots.rs`: spacing 24, thinned ×4 under 16 px); every primitive outside a layer costs a bounds-tree insert (`gpui-pre-0.3.8/src/scene.rs:97-101`); `Window::paint_layer` shares one order for non-overlapping geometry (`window.rs:4394-4416`). |
| dots + drawings | **12.2** | both fixed costs together already pass the bar |
| drawings (ink) | 15.3 | `drawing.rs:151-153` clones every drawing per frame; `ink.rs:155-161` computes `visible()` twice per drawing per frame |
| `Snapshot::refresh` | 14.8 | `snapshot.rs:117-206` re-copies every visible terminal's grid every frame, never consulting the wake flag; `rows.clear()+resize_with` reallocates every row's buffers |
| terminal text shaping | 15.8 | per-row `shape_line` (left alone here: GPUI's two-frame layout cache makes unchanged rows hits, `text_system/line_layout.rs:458-585`) |

User-like scenario (5 TUI terminals + 3 editors + 3 git + 1 note + 2 kanban): p95 ~12, p99 ~16, **max ~35**; the 9 widget blocks cost ~3 ms of p95 and the spikes (batch C, not this plan).

alacritty facts used by batch B (`~/.cargo/registry/src/*/alacritty_terminal-0.26.0/src`): `Term::damage(&mut self) -> TermDamage` (`term/mod.rs:447-484`, `Full` or `Partial(iter)` of viewport line numbers), `reset_damage()` (`:487-491`); selection is **not** in damage (`:450-452`); `scroll_display`, `scroll_up/down` and `resize` mark full damage; `Grid: Index<Line>` → `Row<Cell>`, `Row: Index<Column>` (`grid/mod.rs:453-460`, `grid/row.rs:193`); `Event::Wakeup` is sent once per `pty_read` batch (`event_loop.rs:165-168`), never throttled.

## Global Constraints

- Only `crates/templum` may depend on the UI framework; `templum-canvas` and `templum-terminal` stay UI-free and are tested headlessly (AGENTS.md).
- `gpui-kit = "=0.7.1"` and `alacritty_terminal = "=0.26.0"` stay pinned; no new dependencies.
- No `unsafe` (workspace denies it). No copyleft code: Zed's `terminal`/`terminal_view` are GPL; nothing is ported from them.
- Every logic change ships a test that fails without it; UI-only changes state how they were verified.
- Commits on `development`, Conventional Commits with an imperative sentence; `perf:` lines are read by users in the CHANGELOG.
- Before "done": `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, plus `cargo run --release -p templum -- --bench` (the p95 ≤ 16.7 bar).
- Every API used must exist in the pinned sources quoted above; read them before writing, never guess.
- `<before>`/`<after>` in a commit message are the bench's p50/p95 (and max where named) measured right before and after that task, median of three runs; never commit the placeholders.

## Review Focus

Inputs the spec implies but that are easy to break; each has a test in the task named:

1. A terminal whose block is off screen (or in a hidden workspace) prints output, then scrolls into view: it must show the new text on its first frame on screen — Task 3's workspace test brings the hidden block back and waits for its text.
2. A terminal off screen exits (Ctrl-D in `cat`): its block must still close although it is never re-armed — Task 3 makes `Event::Exit` wake the UI uncoalesced and its test kills the hidden terminal.
3. Switching the theme must recolor terminal text at once even though no output arrived — Task 5 keys the skip on the palette too; its test changes the palette and expects every row rebuilt.
4. Dragging a selection must update the highlighted spans although alacritty's damage ignores selections — Task 5's `select_start`/`select_update` mark the terminal touched and Task 6 rebuilds everything when the selection changed; the tests assert the spans.
5. Resizing a block to more rows must grow the snapshot and rebuild it fully — Task 6's test resizes the headless terminal from 4 to 6 rows.

---

### Task 1: Dot grid as one layer, with a dot budget

**Files:**
- Modify: `crates/templum-canvas/src/dots.rs` (constants, `dot_grid`, tests)
- Modify: `crates/templum/src/workspace/render.rs:102-124` (`paint_dots`)
- Modify: `docs/specs/polish.md:46` (the thinning rule)

**Interfaces:**
- Consumes: `templum_canvas::{Camera, Point, dot_grid, dot_size}`; `gpui_kit::Window::paint_layer(bounds, |window| ...)` (`gpui-pre-0.3.8/src/window.rs:4399`).
- Produces: `MIN_DOT_GAP = 24.0`, thinning ×2. `DotGrid` and `dot_size` keep their signatures.

What the user sees: at 100% nothing changes (24 px between dots). Between steps the dots are 24–48 px apart on screen instead of 16–64, so zooming out never shows the dense 16 px carpet, and a zoom glide never draws a frame with 8,000 dots.

- [ ] **Step 1: Write the failing tests in `dots.rs`**

Replace the test `dots_are_never_closer_than_16_px_nor_thinned_needlessly` and add the budget test:

```rust
    #[test]
    fn dots_are_never_closer_than_24_px_nor_thinned_needlessly() {
        for (i, zoom) in ZOOM_STEPS.iter().enumerate() {
            let grid = dot_grid(&at_step(i, Point::default()));
            let gap = grid.spacing * zoom;
            assert!(gap >= MIN_DOT_GAP, "zoom {zoom}: {gap} px");
            if grid.spacing > DOT_SPACING {
                assert!(
                    grid.spacing / 2.0 * zoom < MIN_DOT_GAP,
                    "zoom {zoom}: over-thinned"
                );
            }
        }
        assert_eq!(
            dot_grid(&at_step(0, Point::default())).spacing,
            96.0,
            "zoom 0.25"
        );
    }

    /// How many dots a 1920×1080 viewport draws at `zoom`.
    fn dots_on_full_hd(zoom: f32) -> f32 {
        let grid = dot_grid(&Camera::unsnapped(Point::default(), zoom));
        let step = grid.spacing * zoom;
        ((1920.0 / step).ceil() + 1.0) * ((1080.0 / step).ceil() + 1.0)
    }

    #[test]
    fn a_full_hd_viewport_stays_under_3800_dots_at_every_zoom_a_flight_draws() {
        let mut last: Option<f32> = None;
        // 0.25 to 3.0 by 0.02: every zoom a CameraFlight can draw.
        for k in 0..=138 {
            let zoom = 0.25 + k as f32 * 0.02;
            let n = dots_on_full_hd(zoom);
            assert!(n <= 3800.0, "zoom {zoom}: {n} dots");
            if let Some(l) = last {
                // Thinning halves the spacing: at most 4× fewer dots from one zoom to the next.
                assert!(n / l <= 4.5 && l / n <= 4.5, "zoom {zoom}: {l} -> {n}");
            }
            last = Some(n);
        }
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-canvas dots`
Expected: `dots_are_never_closer_than_24_px...` fails (`gap >= MIN_DOT_GAP` with 16 px gaps vs 24) and the budget test fails at zoom 0.67 (8,200 dots) and on the 16× jump.

- [ ] **Step 3: Change the constants and the thinning**

In `dots.rs`:

```rust
/// World units between dots at zoom 1.
pub const DOT_SPACING: f32 = 24.0;
/// Dots closer than this on screen are thinned out, 2× at a time: a Full HD viewport never
/// draws more than ~3,700 of them, and a zoom glide never jumps to a dense frame.
pub const MIN_DOT_GAP: f32 = 24.0;
```

and in `dot_grid`:

```rust
    while spacing * camera.zoom() < MIN_DOT_GAP {
        spacing *= 2.0;
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-canvas dots`
Expected: all pass (`dot_size_follows_the_zoom...` still holds: 3 px ≤ 24/4).

- [ ] **Step 5: Paint the dots as one layer**

In `render.rs`, `paint_dots` becomes:

```rust
/// The dot grid under the blocks: round dots centered on the world lattice. One layer: the
/// dots don't overlap and share a draw order, so GPUI skips the per-primitive bounds lookup.
fn paint_dots(camera: &Camera, color: Hsla, bounds: Bounds<Pixels>, window: &mut Window) {
    let grid = dot_grid(camera);
    let step = grid.spacing * camera.zoom();
    let d = dot_size(camera.zoom());
    let start = camera.to_screen(grid.first);
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    window.paint_layer(bounds, |window| {
        let mut y = start.y;
        while y < h + d {
            let mut x = start.x;
            while x < w + d {
                let at = point(
                    bounds.origin.x + px(x - d / 2.0),
                    bounds.origin.y + px(y - d / 2.0),
                );
                window.paint_quad(
                    fill(Bounds::new(at, size(px(d), px(d)), color).corner_radii(px(d / 2.0)),
                );
                x += step;
            }
            y += step;
        }
    });
}
```

- [ ] **Step 6: Update the spec line**

In `docs/specs/polish.md` line 46, replace "thinned 4× at a time below 16 px" with "thinned 2× at a time below 24 px".

- [ ] **Step 7: Full check and bench**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clean. Then `cargo run --release -p templum -- --bench` three times; note the p50/p95 (expect p95 ~2 ms lower than the baseline; the number goes in the commit body). Open the app and zoom from 25% to 300% with Ctrl+wheel: the dots stay between 24 and 48 px apart and never flicker dense.

- [ ] **Step 8: Commit**

```bash
git add crates/templum-canvas/src/dots.rs crates/templum/src/workspace/render.rs docs/specs/polish.md
git commit -m "perf(canvas): draw the dot grid as one layer and never more than 3,700 dots

The grid thins 2x at a time under 24 px (was 4x under 16 px), so a Full HD view
draws 3,700 dots at most instead of 8,200 at some zooms, and the dots go through
one paint layer instead of one bounds lookup each. Bench: p95 <before> -> <after> ms.

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

### Task 2: Paint the drawings without copying them

**Files:**
- Modify: `crates/templum/src/workspace/drawing.rs:150-179` (`ink_layer`)
- Modify: `crates/templum/src/workspace/render.rs:339` (the call)
- Modify: `crates/templum/src/ink.rs:139-200` (`runs`: visibility computed once)

**Interfaces:**
- Consumes: `Entity<Workspace>::read(&App) -> &Workspace` (as `grid.rs:156` already uses `ws.update` inside a paint closure: the workspace isn't borrowed during paint); `Workspace::{space, drag, theme, ui, ink_cache}` (private fields, visible inside `workspace/`).
- Produces: `Workspace::ink_layer(&self, cx: &Context<Self>) -> impl IntoElement + use<>`; `ink::paint`/`ink::runs` keep their signatures.

- [ ] **Step 1: Make `runs` compute visibility once per drawing**

In `ink.rs`, replace the two `shown` filters:

```rust
    let viewport = Size::new(f32::from(bounds.size.width), f32::from(bounds.size.height));
    // Once per drawing: `bounds()` walks every point.
    let shown: Vec<bool> = drawings
        .iter()
        .map(|d| visible(d, camera, viewport))
        .collect();
    let highlights = (0..drawings.len()).filter(|i| shown[*i] && is_highlight(&drawings[*i]));
    let inks = (0..drawings.len()).filter(|i| shown[*i] && !is_highlight(&drawings[*i]));
```

Run: `cargo test -p templum ink`
Expected: `paint_culls_drawings_outside_the_viewport` and `built_paths_are_kept_until_the_drawing_or_the_zoom_step_changes` still pass (they pin the behavior this refactor must keep).

- [ ] **Step 2: Read the workspace inside the paint closure instead of cloning the drawings**

In `drawing.rs`, replace `ink_layer`:

```rust
    /// The layer that paints the drawings. It reads the workspace at paint time: cloning 200
    /// strokes into the closure every frame is what it avoids.
    pub(super) fn ink_layer(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let entity = cx.entity();
        let cache = self.ink_cache.clone();
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let ws = entity.read(cx);
                let live = match &ws.drag {
                    Some(Drag::Ink(d)) => Some(d),
                    _ => None,
                };
                ink::paint(
                    &ws.space.ink,
                    live,
                    ws.space.selected_ink,
                    &ws.space.view,
                    &ws.theme,
                    ws.ui,
                    bounds,
                    &mut cache.borrow_mut(),
                    window,
                );
            },
        )
        .absolute()
        .size_full()
    }
```

In `render.rs` line 339: `.child(self.ink_layer(cx))`.

- [ ] **Step 3: Run the drawing tests**

Run: `cargo test -p templum drawing`
Expected: all of `workspace/tests/drawing.rs` pass (they draw strokes and check what is painted).

- [ ] **Step 4: Full check and bench**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, then `cargo run --release -p templum -- --bench` three times (the bench has 200 strokes; expect p95 lower). Draw a few strokes with the pen in the app, pan and zoom: they stay put.

- [ ] **Step 5: Commit**

```bash
git add crates/templum/src/workspace/drawing.rs crates/templum/src/workspace/render.rs crates/templum/src/ink.rs
git commit -m "perf(canvas): paint the drawings without copying them every frame

The ink layer reads the workspace when it paints instead of cloning every
stroke into its closure, and each drawing is culled once per frame, not twice.
Bench: p95 <before> -> <after> ms.

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

### Task 3: Re-arm only the terminals on screen; never wait on a whole PTY read

**Files:**
- Modify: `crates/templum-terminal/src/terminal.rs` (`SharedTerm::{rearm, armed}`, remove `Terminal::rearm`, `Event::Exit`, `lock` → `lock_unfair`)
- Modify: `crates/templum-terminal/src/snapshot.rs:122` (`lock` → `lock_unfair`)
- Modify: `crates/templum/src/grid.rs:180-184` (re-arm before refresh)
- Modify: `crates/templum/src/workspace/render.rs:187-190` (remove the re-arm loop)
- Test: `crates/templum/src/workspace/tests/terminals.rs`

**Interfaces:**
- Consumes: `FairMutex::{lock_unfair, try_lock_unfair}` (`alacritty_terminal/src/sync.rs:41-48`); `Shared::{pending, exited, wake_now}` (`terminal.rs:30-85`); test helpers `open`, `first_id`, `until`, `NEW` (`workspace/tests/{mod.rs,terminals.rs}`); `Canvas::{get_mut, top}` (`templum-canvas/src/canvas.rs:57-63`).
- Produces: `SharedTerm::rearm(&self)` and `SharedTerm::armed(&self) -> bool` (public); `Terminal::rearm` is gone.

Why re-arm in the paint and not at the frame start: re-arming must happen *before* the grid is read, so output parsed during the read wakes the UI again (re-arming after the read could lose a wake-up). Re-arming in the paint closure, per painted terminal, gives exactly that, and blocks that aren't painted (off screen, hidden workspace) are never re-armed, so their output can't drive frames.

- [ ] **Step 1: Write the failing workspace test**

Append to `crates/templum/src/workspace/tests/terminals.rs`:

```rust
/// Output in a block off screen wakes the UI once and no more until the block is painted; a
/// painted block is re-armed every frame. Scrolled back on screen, the block shows what it
/// printed meanwhile; an off-screen terminal that exits still closes its block.
#[cfg(unix)]
#[gpui_kit::test]
fn only_painted_terminals_are_rearmed(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let shown = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    let hidden = ws.read_with(cx, |ws, _| ws.space.canvas.top().unwrap());
    assert_ne!(shown, hidden);
    let park = |ws: &mut Workspace, away: bool| {
        let rect = &mut ws.space.canvas.get_mut(hidden).unwrap().rect;
        rect.x = if away { 100_000.0 } else { 0.0 };
        rect.y = 0.0;
    };
    ws.update(cx, |ws, _| park(ws, true));
    let snapshot_text = |ws: &Workspace, id: BlockId| {
        ws.space.terminal(id).unwrap().snapshot.borrow().text()
    };
    let armed = |ws: &Workspace, id: BlockId| ws.space.terminal(id).unwrap().term.shared().armed();
    // `cat` echoes the line back: output in both terminals.
    for id in [shown, hidden] {
        ws.read_with(cx, |ws, _| {
            ws.space.terminal(id).unwrap().term.write(b"x\r".to_vec())
        });
    }
    until("both terminals to print", handle, &ws, cx, |ws| {
        snapshot_text(ws, shown).starts_with("x\nx") && !armed(ws, hidden)
    });
    ws.read_with(cx, |ws, _| {
        assert!(armed(ws, shown), "the painted block is re-armed every frame");
        assert!(!armed(ws, hidden), "the hidden block waits to be painted");
        assert_eq!(snapshot_text(ws, hidden).trim_end(), "", "never read while hidden");
    });
    // Back on screen: its first frame shows what it printed.
    ws.update(cx, |ws, _| park(ws, false));
    until("the hidden block to catch up", handle, &ws, cx, |ws| {
        snapshot_text(ws, hidden).starts_with("x\nx") && armed(ws, hidden)
    });
    // Off screen again, its shell exits (Ctrl-D ends `cat`): the block still closes.
    ws.update(cx, |ws, _| park(ws, true));
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(hidden).unwrap().term.write(b"\x04".to_vec())
    });
    until("the exited block to close", handle, &ws, cx, |ws| {
        ws.space.terminal(hidden).is_none()
    });
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum only_painted_terminals_are_rearmed`
Expected: compile error — `armed` doesn't exist on `SharedTerm`. (Once it exists but before the render change, the hidden block is re-armed every frame and `!armed(ws, hidden)` never holds: the test times out after 10 s with "waited for both terminals to print".)

- [ ] **Step 3: Move `rearm` to `SharedTerm`, add `armed`, make Exit uncoalesced**

In `terminal.rs`, inside `impl SharedTerm` (after `selection_text`):

```rust
    /// Re-arms the coalesced notification: the next output wakes the UI again. Call it right
    /// before reading the grid, so output parsed during the read isn't missed. Only painted
    /// terminals are re-armed: output off screen wakes the UI once, then waits to be painted.
    pub fn rearm(&self) {
        self.1.pending.store(false, Ordering::Release);
    }

    /// False while a wake-up waits to be re-armed (tests).
    pub fn armed(&self) -> bool {
        !self.1.pending.load(Ordering::Acquire)
    }
```

Delete `Terminal::rearm` (lines 475-478). In `Listener::send_event`, the exit arm becomes:

```rust
            // Exit comes after the child's last output has been read (ChildExit comes before).
            // Uncoalesced: a block off screen is never re-armed, and must still close.
            Event::Exit => {
                s.exited.store(true, Ordering::Release);
                s.wake_now();
            }
```

In the test `notifications_are_coalesced_until_rearmed`, change the comment `// what Terminal::rearm does` to `// what SharedTerm::rearm does`.

- [ ] **Step 4: Re-arm in the paint, drop the per-frame loop**

In `grid.rs` `paint`, replace lines 180-184:

```rust
    let mut snap = snapshot.borrow_mut();
    // Re-armed before the read: output parsed during it wakes the UI again.
    term.rearm();
    if !snap.refresh(term, &theme.palette) {
        // The PTY thread is busy: try again next frame.
        window.request_animation_frame();
    }
```

In `render.rs`, delete lines 187-190 (`// Re-arm once per frame...` through the loop).

- [ ] **Step 5: Unfair locks on the UI thread**

The fair `lock()` waits for the PTY thread's lease, held for a whole `pty_read` call (up to 64 KiB parsed); `lock_unfair()` waits only for the parse in progress. In `snapshot.rs:122`: `None if stale => term.0.lock_unfair(),` with the comment:

```rust
            // Unfair: waits for the parse under way (at most MAX_LOCKED_READ bytes), not for
            // the PTY thread's whole read pass as the fair lock would.
```

In `terminal.rs`, replace every `self.0.lock()` / `self.term.0.lock()` / `term.0.lock()` on the UI-thread paths with `lock_unfair()`: `set_options`, `select_start`, `select_update`, `select_clear`, `selection_text`, `write`, `key`, `paste`, `resize`, `jump_in`, `scroll`. Add once, above `impl SharedTerm`:

```rust
// The UI thread locks the grid unfairly: it never waits for the PTY thread's lease (held for a
// whole read pass), only for the parse under way. See alacritty's sync.rs.
```

No test: the result is the same when uncontended; the gain is latency under heavy output, measured by the bench's p99/max.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p templum-terminal && cargo test -p templum only_painted_terminals_are_rearmed`
Expected: PASS.

- [ ] **Step 7: Full check and bench**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, then `cargo run --release -p templum -- --bench` three times (expect p99/max to drop: no more 100 ms stalls on the fair lock). In the app: run `yes` in one terminal, pan with Alt+drag and type in another terminal — both stay responsive; scroll the `yes` block off screen and check CPU in `top` drops.

- [ ] **Step 8: Commit**

```bash
git add crates/templum-terminal/src/terminal.rs crates/templum-terminal/src/snapshot.rs crates/templum/src/grid.rs crates/templum/src/workspace/render.rs crates/templum/src/workspace/tests/terminals.rs
git commit -m "perf(terminal): keep off-screen output from redrawing, and never wait for a whole PTY read

A terminal is re-armed when its block is painted, so a busy terminal off
screen wakes the UI once instead of every frame; its block still closes when
the shell exits. The UI thread takes alacritty's unfair lock: it waits for the
parse under way, not for the PTY thread's whole read pass, which stalled frames
for up to 64 KiB of output. Bench: p95 <before> -> <after>, max <before> -> <after>.

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

### Task 4: Tooling: optimized dependencies in dev builds, a mixed bench scenario

**Files:**
- Modify: `Cargo.toml` (root)
- Modify: `crates/templum/src/main.rs:39,76-78`
- Modify: `crates/templum/src/bench.rs`
- Modify: `AGENTS.md:12-14,43-45`

**Interfaces:**
- Consumes: `Workspace::{open, add_editor_at, add_git_at, add_note_at, add_kanban_at}` (`workspace/{blocks.rs,files.rs,git_ops.rs}`, all `pub(crate)`, no dialogs).
- Produces: `bench::run(window, ws, mixed: bool, cx)`; flag `--bench-mixed`.

- [ ] **Step 1: Dev profile**

Append to the root `Cargo.toml`:

```toml
# `cargo run -p templum` is a usable app: the UI framework, text shaping, tessellation and the
# terminal core are compiled with optimizations even in dev builds. Templum's own crates stay
# at opt-level 0, so they keep fast rebuilds and full debug info.
[profile.dev.package."*"]
opt-level = 2
```

Run: `cargo build -p templum` (a one-time full rebuild of the dependencies; later builds are incremental as before).

- [ ] **Step 2: The mixed scenario**

In `main.rs`:

```rust
    let mixed = std::env::args().any(|a| a == "--bench-mixed");
    let bench = mixed || std::env::args().any(|a| a == "--bench");
```

and `bench::run(window, ws, mixed, cx);`.

In `bench.rs`, the header comment and `run`:

```rust
//! `templum --bench`: 20 terminals, 5 running `yes`, 200 strokes over them, 10 s of animated pan
//! and zoom. `templum --bench-mixed`: a canvas like an agent cockpit instead — 5 terminals
//! redrawing a colored screen 10× a second (an agent's TUI), 3 editors, 3 Git blocks, a note and
//! 2 kanban blocks, no strokes.
//! Prints one JSON line (frame-interval percentiles, memory), then quits. Bar: p95 <= 16.7 ms.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Entity};
use templum_canvas::{Color, Drawing, Point, Rect, ZOOM_STEPS, chrome};

use crate::workspace::{DEFAULT_COLS, DEFAULT_ROWS, Workspace};

/// Typed into the terminals of the mixed scenario: 24 colored lines redrawn 10× a second,
/// each with a number that changes, like an agent's interface.
const TUI_LOOP: &[u8] = b"while :; do printf '\\033[H'; for i in $(seq 24); do printf '\\033[3%dm%5d The quick brown fox jumps over the lazy dog 0123456789 abcdefghijkl\\033[0m\\n' $((i%7+1)) $RANDOM; done; sleep 0.1; done\r";

pub(crate) fn run(window: AnyWindowHandle, ws: Entity<Workspace>, mixed: bool, cx: &mut App) {
    let _ = cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, ws.cell.size);
            if mixed {
                // The repository Templum runs from: a real folder for the editors and Git blocks.
                let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let mut slots =
                    (1..).map(|i| Point::new((i % 5) as f32 * 950.0, (i / 5) as f32 * 750.0));
                let mut next = || slots.next().expect("endless");
                for _ in 0..4 {
                    let p = next();
                    ws.open(Rect::new(p.x, p.y, size.w, size.h), window, cx);
                }
                for _ in 0..3 {
                    ws.add_editor_at(&repo, next(), window, cx);
                }
                for _ in 0..3 {
                    ws.add_git_at(&repo, next(), window, cx);
                }
                ws.add_note_at(next(), window, cx);
                for _ in 0..2 {
                    ws.add_kanban_at(next(), window, cx);
                }
                return;
            }
            for i in 1..20 {
                let (col, row) = ((i % 5) as f32, (i / 5) as f32);
                let rect = Rect::new(col * (size.w + 40.0), row * (size.h + 40.0), size.w, size.h);
                ws.open(rect, window, cx);
            }
            // 200 strokes of 50 points over the terminals: the spec's bar must hold with ink on
            // screen.
            for s in 0..200 {
                let x0 = (s % 20) as f32 * 150.0;
                let y0 = (s / 20) as f32 * 120.0;
                let points = (0..50)
                    .map(|i| {
                        let t = i as f32;
                        Point::new(x0 + t * 3.0, y0 + (t * 0.4).sin() * 20.0)
                    })
                    .collect();
                ws.space.ink.push(Drawing::Stroke {
                    points,
                    width: 4.0,
                    color: Color::Blue,
                    highlight: s % 4 == 0,
                });
            }
        })
    });
```

and where the terminals get their command (inside the timer task, replacing the `yes` loop):

```rust
                let command: &[u8] = if mixed { TUI_LOOP } else { b"yes\r" };
                for (_, pane) in ws.space.terminals().take(5) {
                    pane.term.write(command);
                }
```

The rest of `run` (the camera driving, the JSON line) is unchanged.

- [ ] **Step 3: AGENTS.md**

Under "Commands", after the run line, add:

```markdown
- Dev builds compile the dependencies optimized (`[profile.dev.package."*"]` in `Cargo.toml`), so `cargo run` is
  usable; performance is judged on `--release`.
```

In the "Performance" rule, after `--bench`: "(and `--bench-mixed`, an agent cockpit: TUIs, editors, Git, note, kanban)".

- [ ] **Step 4: Run both scenarios**

Run: `cargo run --release -p templum -- --bench` and `cargo run --release -p templum -- --bench-mixed`.
Expected: each prints one JSON line and quits; in the mixed one the 5 terminals visibly redraw colored lines (watch the window). Then the full check.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/templum/src/main.rs crates/templum/src/bench.rs AGENTS.md
git commit -m "chore: optimize dependencies in dev builds and add a mixed --bench scenario

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

### Task 5: Read a terminal's grid only after it changed

**Files:**
- Modify: `crates/templum-terminal/src/terminal.rs` (`Shared::dirty`, `wake`, `wake_now`, `SharedTerm::touched`, the UI-thread mutators, `feed`)
- Modify: `crates/templum-terminal/src/snapshot.rs` (`Snapshot::{rebuilt, last_palette}`, early return)
- Test: `crates/templum-terminal/src/snapshot.rs` tests

**Interfaces:**
- Consumes: `Shared` (`terminal.rs:30-48`), `Palette: Copy + PartialEq` (`palette.rs:10`).
- Produces: `Shared.dirty: AtomicBool` (`pub(crate)`); `SharedTerm::touched(&self)` (`pub(crate)`); `Snapshot.rebuilt: usize` (public: rows copied by the last `refresh`, 0 when it skipped).

- [ ] **Step 1: Write the failing test**

In `snapshot.rs` tests:

```rust
    #[test]
    fn refresh_copies_only_after_output_or_a_change_from_the_ui() {
        use crate::{SelectionKind, Side};
        let (term, _) = headless(20, 4);
        feed(&term, b"abc");
        let p = Palette::default();
        let mut s = Snapshot::default();
        assert!(s.refresh(&term, &p));
        assert_eq!(s.rebuilt, 4, "the first read copies every row");
        assert!(s.refresh(&term, &p), "nothing happened: still fresh");
        assert_eq!(s.rebuilt, 0, "nothing happened: nothing is copied");
        // Output (feed marks the terminal changed, as the PTY thread's wake-up does).
        feed(&term, b"d");
        s.refresh(&term, &p);
        assert!(s.rebuilt > 0);
        assert_eq!(s.rows[0].text, "abcd");
        s.refresh(&term, &p);
        assert_eq!(s.rebuilt, 0);
        // A selection: alacritty's damage doesn't track it, the UI call does.
        term.select_start(0, 0, Side::Left, SelectionKind::Simple);
        term.select_update(0, 1, Side::Right);
        s.refresh(&term, &p);
        assert_eq!(s.rebuilt, 4);
        assert_eq!(s.rows[0].selected, vec![Span { start: 0, len: 2 }]);
        s.refresh(&term, &p);
        assert_eq!(s.rebuilt, 0);
        // Another palette (a theme change): the colors are resolved again at once.
        let other = Palette {
            ansi: [Rgb { r: 1, g: 2, b: 3 }; 16],
            ..Palette::default()
        };
        feed(&term, b"\x1b[31m!");
        s.refresh(&term, &p);
        s.refresh(&term, &other);
        assert_eq!(s.rebuilt, 4);
        assert_eq!(s.rows[0].runs.last().unwrap().fg, Rgb { r: 1, g: 2, b: 3 });
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum-terminal refresh_copies_only`
Expected: compile error, `rebuilt` doesn't exist.

- [ ] **Step 3: The `dirty` flag**

In `terminal.rs`, `Shared` gets a field after `pending`:

```rust
    /// The grid may differ from what was last copied out of it: set by the PTY thread's wake-ups
    /// and by the UI calls that change the grid; cleared by [`Snapshot::refresh`] under the lock.
    pub(crate) dirty: AtomicBool,
```

initialized `dirty: AtomicBool::new(true)` in `Shared::new`. `wake` and `wake_now` set it first:

```rust
    fn wake(&self) {
        self.dirty.store(true, Ordering::Release);
        if !self.pending.swap(true, Ordering::AcqRel) {
            (self.notify)();
        }
    }

    pub(crate) fn wake_now(&self) {
        self.dirty.store(true, Ordering::Release);
        (self.notify)();
    }
```

In `impl SharedTerm`:

```rust
    /// The UI changed the grid (a scroll, the selection, a resize): the next refresh reads it.
    pub(crate) fn touched(&self) {
        self.1.dirty.store(true, Ordering::Release);
    }
```

Call `self.touched();` at the end of `set_options`, `select_start`, `select_update`, `select_clear`; `self.term.touched();` at the end of `Terminal::write`, `Terminal::resize` (after the resize, before the PTY message), in `Terminal::scroll`'s history branch (after `scroll_display`); and in `jump_in` after `term.scroll_display(...)`: `term_ref.touched()` where `jump_in(term: &SharedTerm, up)` has it as `term` — rename the guard to `grid` there to keep the name:

```rust
    fn jump_in(term: &SharedTerm, up: bool) -> bool {
        let mut grid = term.0.lock_unfair();
        if grid.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        let top = -(grid.grid().display_offset() as i32);
        let starts = crate::tap::prompt_lines(grid.grid());
        let target = if up {
            starts.into_iter().rfind(|&l| l < top)
        } else {
            starts.into_iter().find(|&l| l > top)
        };
        let Some(target) = target else {
            return false;
        };
        grid.scroll_display(Scroll::Delta(top - target));
        let moved = -(grid.grid().display_offset() as i32) != top;
        drop(grid);
        term.touched();
        moved
    }
```

In the test helper `feed`, after `parser.advance(...)`: `term.touched();`.

- [ ] **Step 4: The early return in `refresh`**

In `snapshot.rs`, add to `Snapshot`:

```rust
    /// Rows copied by the last `refresh`: every row, the changed ones, or none (nothing changed).
    pub rebuilt: usize,
    /// The palette the rows were resolved with: another one means every row again.
    last_palette: Option<Palette>,
```

and at the top of `refresh`, before the lock:

```rust
        let heard = &term.1;
        let same_palette = self.last_palette == Some(*palette);
        if self.taken.is_some() && same_palette && !heard.dirty.load(Ordering::Acquire) {
            self.rebuilt = 0;
            return true;
        }
```

right after the lock is obtained:

```rust
        // Cleared under the lock: output parsed after this point sets it again.
        heard.dirty.store(false, Ordering::Release);
```

and at the end, with `taken`: `self.rebuilt = screen_rows; self.last_palette = Some(*palette);`. Add `use std::sync::atomic::Ordering;`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p templum-terminal`
Expected: the new test passes; the existing snapshot tests still pass (`feed` marks the terminal changed; `headless` starts dirty).

- [ ] **Step 6: Full check and bench**

Run the full check, then `cargo run --release -p templum -- --bench` and `--bench-mixed` three times each. In the app: focus a terminal and let the cursor blink with `top` running in another block — both stay correct; switch the theme with the picker: terminal colors change at once.

- [ ] **Step 7: Commit**

```bash
git add crates/templum-terminal/src/terminal.rs crates/templum-terminal/src/snapshot.rs
git commit -m "perf(terminal): read a terminal's grid only after it changed

A frame no longer copies every visible terminal's cells: a terminal is read
again after its shell printed, after a scroll, a selection or a resize, or when
the theme changed. Bench: p95 <before> -> <after> ms.

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

### Task 6: Rebuild only the rows the terminal reports as changed

**Files:**
- Modify: `crates/templum-terminal/src/snapshot.rs` (`Row::clear`, `Snapshot::{prompts, last_selection}`, `refresh`)
- Test: `crates/templum-terminal/src/snapshot.rs` tests

**Interfaces:**
- Consumes: `Term::{damage, reset_damage, grid, columns, screen_lines, renderable_content}` (`term/mod.rs:447-491, 637-642`), `TermDamage` (`term/mod.rs:178-184`), `Grid: Index<Line>`, `Row<Cell>: Index<Column>` (`grid/mod.rs:453`, `grid/row.rs:193`), `SelectionRange: Copy + PartialEq` with `contains(Point)` (`selection.rs:33-56`), `Snapshot.rebuilt` from Task 5.
- Produces: `Row::clear(&mut self)` (private); `Snapshot::rebuilt` now counts the damaged rows only.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn only_the_rows_the_terminal_damaged_are_rebuilt() {
        let (term, _) = headless(20, 4);
        feed(&term, b"abc\r\ndef\r\nghi");
        let p = Palette::default();
        let mut s = Snapshot::default();
        s.refresh(&term, &p);
        assert_eq!(s.rebuilt, 4, "a new terminal is fully damaged");
        // Overwrite the second line: alacritty damages the cursor's old line (the third, where
        // it left) and the line written.
        feed(&term, b"\x1b[2;1HXYZ");
        s.refresh(&term, &p);
        assert_eq!(s.text(), "abc\nXYZ\nghi\n");
        assert_eq!(s.rebuilt, 2);
        // A line feed at the bottom scrolls: everything is damaged.
        feed(&term, b"\x1b[4;1H\r\n");
        s.refresh(&term, &p);
        assert_eq!(s.text(), "XYZ\nghi\n\n");
        assert_eq!(s.rebuilt, 4);
    }

    #[test]
    fn a_resize_rebuilds_every_row_at_the_new_size() {
        use alacritty_terminal::term::test::TermSize;
        let (term, _) = headless(20, 4);
        feed(&term, b"one\r\ntwo");
        let mut s = Snapshot::default();
        s.refresh(&term, &Palette::default());
        term.0.lock_unfair().resize(TermSize::new(20, 6));
        term.touched();
        s.refresh(&term, &Palette::default());
        assert_eq!(s.rows.len(), 6);
        assert_eq!(s.rebuilt, 6);
        assert_eq!(s.text(), "one\ntwo\n\n\n\n");
    }

    #[test]
    fn a_prompts_mark_survives_rows_that_are_not_rebuilt() {
        let (term, _) = headless(20, 4);
        // A prompt whose command failed (see tap.rs), then another prompt.
        feed_heard(
            &term,
            b"\x1b]133;A\x07$ false\r\n\x1b]133;C\x07\x1b]133;D;1\x07\x1b]133;A\x07$ ",
        );
        let mut s = Snapshot::default();
        s.refresh(&term, &Palette::default());
        assert_eq!(s.rows[0].mark, Some(Mark::Ended(Some(1))));
        // Typing on the last prompt's line damages that line only: the first keeps its mark.
        feed(&term, b"ls");
        s.refresh(&term, &Palette::default());
        assert_eq!(s.rebuilt, 1);
        assert_eq!(s.rows[0].mark, Some(Mark::Ended(Some(1))));
        assert_eq!(s.rows[1].text, "$ ls");
    }
```

`feed_heard` lives inside `terminal.rs`'s private `mod tests` (`fn feed_heard(term, bytes)`, after `the_reported_folder_counts_while_the_shell_is_in_front`): move it out of `mod tests` to sit next to `feed`, as `#[cfg(test)] pub(crate) fn feed_heard(term: &SharedTerm, bytes: &[u8])` (same body; `mod tests` keeps using it through its `use super::*`), and import it in `snapshot.rs`'s tests with `use crate::terminal::{feed, feed_heard, headless};`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-terminal rebuilt -- --nocapture`
Expected: `only_the_rows...` fails on `assert_eq!(s.rebuilt, 2)` (4 today); `a_prompts_mark...` fails on `rebuilt == 1`.

- [ ] **Step 3: Rebuild per row**

In `snapshot.rs`, imports:

```rust
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use alacritty_terminal::grid::{Dimensions, Row as GridRow};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::TermDamage;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::CursorShape;
```

`Row` gets:

```rust
    /// Empties the row for a rebuild, keeping its buffers.
    fn clear(&mut self) {
        self.text.clear();
        self.runs.clear();
        self.bgs.clear();
        self.selected.clear();
        self.mark = None;
    }
```

`Snapshot` gets two private fields:

```rust
    /// Each row's prompt number (see tap.rs), kept with the row.
    prompts: Vec<Option<u32>>,
    /// The selection the rows show: alacritty's damage doesn't track it.
    last_selection: Option<SelectionRange>,
```

The new `refresh`:

```rust
    pub fn refresh(&mut self, term: &SharedTerm, palette: &Palette) -> bool {
        let heard = &term.1;
        let same_palette = self.last_palette == Some(*palette);
        if self.taken.is_some() && same_palette && !heard.dirty.load(Ordering::Acquire) {
            self.rebuilt = 0;
            return true;
        }
        let stale = self.taken.is_none_or(|t| t.elapsed() > MAX_STALE);
        let mut term = match term.0.try_lock_unfair() {
            Some(term) => term,
            // Unfair: waits for the parse under way (at most MAX_LOCKED_READ bytes), not for
            // the PTY thread's whole read pass as the fair lock would.
            None if stale => term.0.lock_unfair(),
            None => return false,
        };
        // Cleared under the lock: output parsed after this point sets it again.
        heard.dirty.store(false, Ordering::Release);
        let screen_rows = term.screen_lines();
        // The viewport rows alacritty changed since the last read; None: all of them.
        let damaged: Option<Vec<usize>> = match term.damage() {
            TermDamage::Full => None,
            TermDamage::Partial(lines) => Some(lines.map(|l| l.line).collect()),
        };
        term.reset_damage();
        let blinking = term.cursor_style().blinking;
        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let selection = content.selection;
        // Everything: full damage, a resize, another palette, or a selection change (not in
        // the damage).
        let all = damaged.is_none()
            || self.rows.len() != screen_rows
            || !same_palette
            || selection != self.last_selection;
        if all {
            self.rows.resize_with(screen_rows, Row::default);
            self.prompts.resize(screen_rows, None);
        }
        let lines: Vec<usize> = match damaged {
            Some(lines) if !all => lines,
            _ => (0..screen_rows).collect(),
        };
        self.cursor = None;
        if content.cursor.shape != CursorShape::Hidden {
            let row = content.cursor.point.line.0 + offset;
            if (0..screen_rows as i32).contains(&row) {
                self.cursor = Some(Cursor {
                    row: row as usize,
                    col: content.cursor.point.column.0,
                    shape: content.cursor.shape,
                    blinking,
                });
            }
        }
        let grid = term.grid();
        for &r in &lines {
            let Some(row) = self.rows.get_mut(r) else {
                continue;
            };
            let line = Line(r as i32 - offset);
            rebuild(row, &mut self.prompts[r], &grid[line], line, selection, content.colors, palette);
        }
        self.rebuilt = lines.len();
        // A prompt starts on the first of its lines: the one above the view may be one of them.
        let above = (offset < grid.history_size() as i32)
            .then(|| line_mark(grid, Line(-offset - 1)))
            .flatten();
        drop(term);
        if self.prompts.iter().any(Option::is_some) {
            let marks = heard.marks.lock().unwrap();
            let mut last = above;
            for (row, prompt) in self.rows.iter_mut().zip(&self.prompts) {
                if prompt.is_some() && *prompt != last {
                    row.mark = prompt.and_then(|p| marks.get(&p).copied());
                }
                last = *prompt;
            }
        }
        self.last_selection = selection;
        self.last_palette = Some(*palette);
        self.taken = Some(Instant::now());
        true
    }
```

and the per-row function, below `impl Snapshot`:

```rust
/// Copies one grid line into `row`, with its prompt number if a prompt starts on it.
fn rebuild(
    row: &mut Row,
    prompt: &mut Option<u32>,
    cells: &GridRow<Cell>,
    line: Line,
    selection: Option<SelectionRange>,
    colors: &Colors,
    palette: &Palette,
) {
    row.clear();
    *prompt = None;
    for col in 0..cells.len() {
        let cell = &cells[Column(col)];
        if prompt.is_none() && cell.extra.is_some() {
            *prompt = mark_of(cell);
        }
        let mut fg = palette::resolve(cell.fg, colors, palette);
        let mut bg = palette::resolve(cell.bg, colors, palette);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cell.flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        if bg != palette.background {
            row.push_bg(col, bg);
        }
        if selection.is_some_and(|s| s.contains(Point::new(line, Column(col)))) {
            row.push_selected(col);
        }
        // A wide char's spacer becomes a blank so later glyphs stay on their columns.
        let c = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.c < ' ' {
            ' '
        } else {
            cell.c
        };
        row.push(
            c,
            Run {
                len: 0,
                fg,
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                dim: cell.flags.contains(Flags::DIM),
            },
        );
    }
    row.trim();
}
```

Notes for the implementer: `term.damage()` needs `&mut Term` — the guard derefs mutably; collect the damaged lines into the `Vec` before `reset_damage()` (the iterator borrows the term). `grid[line]` and `self.prompts[r]` are disjoint borrows (`self.rows` vs `self.prompts`), which Rust allows directly on fields. The display-iterator import (`Dimensions`) is still needed for `screen_lines`/`history_size`; `GridRow::len()` is `grid/row.rs:121`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-terminal`
Expected: the three new tests pass, and every existing snapshot/terminal test (`colors_runs_and_trimming`, `wide_chars...`, `selection_spans`, `a_prompt_shows_its_commands_state_on_its_first_line`, `jumps_go_from_prompt_to_prompt`...) still passes — they pin that a partial rebuild shows the same rows a full one did.

- [ ] **Step 5: Full check and bench**

Run the full check; then `--bench` and `--bench-mixed` three times each. In the app with `btop` or Claude Code in a block: the screen updates correctly while you select text with the mouse, scroll the history with the wheel, resize the block by its grip, and after switching the theme.

- [ ] **Step 6: Commit**

```bash
git add crates/templum-terminal/src/snapshot.rs crates/templum-terminal/src/terminal.rs
git commit -m "perf(terminal): rebuild only the rows the terminal reports as changed

A frame copies the lines alacritty damaged since the last read, reusing each
row's buffers, instead of every cell of the screen; a scroll, a resize, a
selection change or a theme change still copy everything. Bench: p95 <before> -> <after> ms.

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

### Task 7: Close the batch: numbers and docs

**Files:**
- Modify: `docs/ARCHITECTURE.md` ("Runtime" bullets on painting and wake-ups)
- Modify: `docs/plans/2026-10-12-performance-a-b.md` (this file: the results table below)

- [ ] **Step 1: Final measurements**

Run three times each and keep the median line: `cargo run --release -p templum -- --bench`, `cargo run --release -p templum -- --bench-mixed`. Fill in:

| Scenario | Baseline p50 / p95 / p99 / max | After A+B p50 / p95 / p99 / max |
|---|---|---|
| `--bench` | 8.0–9.6 / 16.5–19 / ~25 / 35–50 | |
| `--bench-mixed` | 6.3 / 12.0 / 16 / 35 | |

If `--bench` p95 is still above 16.7, stop and report: the next lever is batch C (blocks as cached views), not more of this.

- [ ] **Step 2: ARCHITECTURE.md**

In "Runtime", replace the bullet starting "**Painting never waits on a terminal.**" with:

```markdown
- **Painting never waits on a terminal.** Each frame re-arms the terminals it paints and copies the rows
  alacritty reports as changed into a per-terminal snapshot (nothing when nothing changed); if the PTY
  thread holds the lock, the previous snapshot is drawn and a frame is asked for. After 100 ms it waits,
  with the unfair lock: for the parse under way, never for the PTY thread's whole read pass. Terminals
  off screen are never re-armed, so their output can't drive frames; a shell's exit wakes the UI anyway.
```

- [ ] **Step 3: Full check, commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add docs/ARCHITECTURE.md docs/plans/2026-10-12-performance-a-b.md
git commit -m "docs: describe lazy terminal snapshots and the bench scenarios

Claude-Session: https://claude.ai/code/session_0163jDyGkhBsbBpn1pbYko94"
```

---

## Out of scope (deliberately)

- Blocks as cached `Entity` views (batch C): the structural fix for the editor/git/note/kanban cost and the spikes.
- Zoom glide re-shaping text at every 0.02 step (batch D): hold the text at the start size until the glide ends.
- Caching a `ShapedLine` per terminal row and the per-row `Vec<TextRun>`: with Task 5 and GPUI's two-frame layout cache, what remains is ~2 allocations per non-blank row per frame (tens of µs); add when a profile shows it.
- `status()`/`title()` locks, `format!` tooltips, `is_dir()` per block per frame: microseconds each; not where the time goes.
