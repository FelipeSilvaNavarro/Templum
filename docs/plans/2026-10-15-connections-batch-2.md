# Connections, batch 2 (wires): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a person wires two blocks of a workspace with the Wire tool or a block's menu; the wire is a curve under
the blocks that follows them, can be selected, deleted and taken back with Ctrl+Z, goes with a closed block and comes
back with it, and comes back after a restart; an agent reads and types into the terminals wired to it, and is told
which blocks are.

**Architecture:** `templum-canvas` gains `wire.rs`: `Wire` (two `BlockId`s, no direction) and `Curve` (the cubic
from the facing edge of one rect to the facing edge of the other, a point on it, hitting it). The app keeps
`Space::wires: Vec<Wire>` by canvas id; `workspace/wires.rs` wires and unwires (each an undo step), runs the Wire
tool, selects a wire and lists the curves to paint; `wires.rs` paints them in the full-size layer under the blocks
(`render.rs`), where the dots are. The workspace file saves them by block id (`uid`); the bus checks
`Space::wired` at every call.

**Tech Stack:** Rust 2024, GPUI (gpui-fast 0.1.8) through gpui-kit 0.7.3: `PathBuilder::stroke` +
`cubic_bezier_to`, `Window::paint_path`, `IconName::Cable`/`Unplug` (Lucide, bundled by `AllAssets`), gpui-kit's
`PopupMenu::submenu_with_icon`; `serde` for the file.

**Spec:** [docs/specs/connections.md](../specs/connections.md) (batch 2 of its Delivery: §What a person sees ▸
Wires, §Files, §Architecture rows marked 2, §Testing items marked batch 2, and "tools check wires").

## Global Constraints

- Only `crates/templum` depends on gpui-kit; `templum-canvas` stays UI-free and is unit-tested headlessly.
- No new dependency. No `unsafe`. Every logic change ships a test that fails without it.
- Spec, verbatim: "**The Wire tool** (a cable icon, in the toolbar's Pointer group, after Hand): press on a block,
  drag, and a curve follows the pointer; release on another block of the workspace to wire them. Released anywhere
  else, nothing happens. The tool stays on until another tool is picked."
- "**A block's menu:** **Connect to ▸** lists the workspace's blocks not wired to it, by title; **Disconnect from ▸**
  the ones that are."
- "**A wire** is a curve from the edge of one block that faces the other to the edge of the other, drawn on the
  canvas below the blocks in the title's dim color (`ui.title_dim`). It follows its blocks as they move or are
  resized."
- "**Selecting:** a click on a wire selects it (accent color); Delete or Backspace removes it; Esc unselects;
  right-click on it offers **Disconnect**."
- "**Undo:** connecting and disconnecting are steps of Ctrl+Z and Ctrl+Shift+Z. Closing a block removes its wires,
  and undoing the close brings them back."
- "**A block wired twice** to the same block, or to itself, is refused silently."
- "**Far away** (the Card level) a wire is 1 pixel wide. **Grid mode** hides wires, as it hides drawings."
- "**Moving a block to another workspace** drops its wires."
- File: "the canvas [gains] its wires, by block id. A wire whose block didn't come back is dropped. An older Templum
  ignores [it]": `"wires": [{ "a": "b7k2m", "b": "4qm2a" }]`, `version` stays 1.
- A wire has no direction: "both ends may use each other". A caller may use its own block or one wired to it,
  checked at each call.
- Not in this batch: the travelling dot and `connect` (batch 3), wire labels and `templum wire` (batch 5), wires
  between workspaces or to drawings (out of scope).
- Commits on `development`, Conventional Commits, `feat`/`fix` titles written for a person using Templum; every
  commit ends with the line `Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ`.
- Before saying done: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`; this batch paints, so `--bench`, `--bench-mixed` and `--bench-far` run too (p95 ≤ 16.7 ms).

## Decisions

The spec leaves these open; the plan takes them as written here.

1. **Wires are kept by canvas id** (`Space::wires: Vec<Wire>`, `Wire { a: BlockId, b: BlockId }`), like every other
   per-block state; block ids (`uid`) are only for the file and for agents. `Wire`'s equality ignores direction, so
   remapping one end (undo) never breaks it.
2. **The facing edges:** the left/right pair when the gap between the rects across is the wider, else the
   top/bottom pair; from the middle of one edge to the middle of the other. The curve leaves and arrives square to
   the edges, its control points halfway along that axis (signed, no minimum: no loops for close or overlapping
   blocks).
3. **Width:** 2 world units, never under 1 px: 1 px at every zoom up to 50%, which is where the Card level (text
   under 2.5 px) comes for any font of 5 px or more. No level lookup.
4. **No wire cache of our own:** one stroked path per wire per frame, culled to the view; gpui-fast keeps the
   triangles of a shape it built before (`gpui-fast-0.1.8/src/fast/path_cache.rs:55-80`). If a bench fails, cache
   per zoom step as `ink.rs` does (the spec's Risks row).
5. **The benches:** `--bench-mixed` and `--bench-far` wire their blocks in a chain (each to the next), so their numbers
   include wires; `--bench` stays as it is.
6. **The Wire tool** takes the pointer with a layer over the blocks, as the drawing tools do. A press on the empty
   canvas does nothing. While the pointer is over another block, the curve already runs to that block's facing edge
   (the wire it would make), in the accent. Pressing leaves no block focused, so Ctrl+Z reaches the canvas.
7. **Selecting:** with Select, on the empty canvas only (blocks stop presses, so a wire's stretch under a block isn't
   hit there); drawings are above wires and are hit first. One kind of selection at a time (blocks, a drawing, a
   wire). Focusing or adding a block drops the wire's selection, and Delete/Backspace remove a wire only while no
   block is focused.
8. **Right-click on a wire** opens its menu (Disconnect) without selecting it, as a drawing's menu does.
9. **The block menu:** Connect to ▸ and Disconnect from ▸ come after Fullscreen and before Move to workspace ▸, each
   only when its list isn't empty, the blocks back to front by title, icons `cable` and `unplug`. Only on the canvas:
   a tile's menu (grid mode, which hides wires) has neither.
10. **Undo:** `Change::Connect(Wire)` and `Change::Disconnect(Wire)`; closing keeps the closed blocks' wires in
    `Change::Open(items, wires)`; a block brought back with a new canvas id renames its wires' ends in every step, as
    the other steps' ids are renamed. A wire step whose block is gone is skipped.
11. **Moving blocks to another workspace** drops all their wires, those between blocks moved together too (the
    spec's sentence, literally).
12. **What agents are told, until `connect` exists (batch 3):** the refusal stays batch 1's
    "`k9x1d isn't connected to you. Ask the person to wire it to your block on the canvas.`", and the instructions say
    "In this workspace, not connected: …" without "(ask with connect)": telling an agent to call a tool it doesn't have
    would fail. The kanban and note sentence of the spec's example comes with their tools (batch 4).
13. **Restoring:** wires attach by `uid` after the items took their ids back; a uid two items claimed belongs to the
    one that kept it, so its wires go there.

## Review Focus

1. **Delete or Backspace while a terminal holds the keyboard after a wire was selected:** the terminal gets the key,
   the wire stays. Task 5's `a_terminal_with_the_keyboard_keeps_its_backspace`.
2. **Two wired blocks closed one after the other, then both brought back with Ctrl+Z:** the wire comes back between
   their new canvas ids. Task 2's `a_wire_comes_back_between_two_blocks_closed_one_after_the_other`.
3. **A saved file whose wires name a missing uid, a uid two items claim, one block twice, or one pair twice:** only
   the valid, distinct wires come back; nothing panics. Task 6's `a_saved_wire_to_a_missing_or_doubled_block_is_dropped`.
4. **A click where a wire runs under a drawing or a block:** the drawing or the block gets it, not the wire. Task 5's
   `a_wire_under_a_drawing_or_a_block_isnt_the_one_clicked`.
5. **A wire taken away while an agent is connected:** its next tool call is refused at once, and `initialize`
   reflects the wires of that moment. Task 7's `a_wired_terminal_is_read_and_typed_into_until_it_is_unwired`.

---

### Task 1: `templum-canvas`: the wire and its curve

**Files:**
- Create: `crates/templum-canvas/src/wire.rs`
- Modify: `crates/templum-canvas/src/lib.rs:12-29` (module and re-exports)
- Modify: `crates/templum-canvas/src/ink.rs:293` (`segment_distance` becomes `pub(crate)`)
- Test: inline `#[cfg(test)] mod tests` in `wire.rs`

**Interfaces:**
- Produces: `templum_canvas::Wire { pub a: BlockId, pub b: BlockId }` (`Copy`, `Debug`, `PartialEq`/`Eq` ignoring
  direction); `Wire::new(a: BlockId, b: BlockId) -> Option<Wire>` (None for one block); `Wire::touches(self, id:
  BlockId) -> bool`. `templum_canvas::Curve { pub from: Point, pub c1: Point, pub c2: Point, pub to: Point }`;
  `Curve::between(a: Rect, b: Rect) -> Curve`; `Curve::at(&self, t: f32) -> Point`; `Curve::hit(&self, p: Point,
  tolerance: f32) -> bool`; `Curve::bounds(&self) -> Rect`.

- [ ] **Step 1: Write the failing tests**

Create `crates/templum-canvas/src/wire.rs` with only the tests for now (the module compiles once Step 3 adds the
code above them):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wires_ends_sit_on_the_facing_edges() {
        let a = Rect::new(0.0, 0.0, 100.0, 50.0);
        // Beside it and lower: its right edge to the other's left one, leaving square to both.
        let c = Curve::between(a, Rect::new(300.0, 200.0, 100.0, 50.0));
        assert_eq!(
            (c.from, c.to),
            (Point::new(100.0, 25.0), Point::new(300.0, 225.0))
        );
        assert_eq!(
            (c.c1, c.c2),
            (Point::new(200.0, 25.0), Point::new(200.0, 225.0))
        );
        // Below it: its bottom edge to the other's top one.
        let c = Curve::between(a, Rect::new(20.0, 300.0, 100.0, 50.0));
        assert_eq!((c.from, c.to), (Point::new(50.0, 50.0), Point::new(70.0, 300.0)));
        // Left of it, and above it.
        let c = Curve::between(a, Rect::new(-400.0, 10.0, 100.0, 50.0));
        assert_eq!((c.from, c.to), (Point::new(0.0, 25.0), Point::new(-300.0, 35.0)));
        let c = Curve::between(a, Rect::new(0.0, -300.0, 100.0, 50.0));
        assert_eq!((c.from, c.to), (Point::new(50.0, 0.0), Point::new(50.0, -250.0)));
        // To the pointer, a rect of no size: it ends there.
        let c = Curve::between(a, Rect::new(500.0, 40.0, 0.0, 0.0));
        assert_eq!(c.to, Point::new(500.0, 40.0));
    }

    #[test]
    fn a_point_on_it_hits_and_one_50_units_off_doesnt() {
        let straight = Curve::between(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(300.0, 0.0, 100.0, 100.0),
        );
        assert!(straight.hit(Point::new(200.0, 50.0), 6.0));
        assert!(straight.hit(Point::new(200.0, 55.0), 6.0), "within the reach");
        assert!(!straight.hit(Point::new(200.0, 100.0), 6.0), "50 off");
        assert!(!straight.hit(Point::new(350.0, 50.0), 6.0), "past its end");
        let s = Curve::between(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            Rect::new(300.0, 200.0, 100.0, 50.0),
        );
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!(s.hit(s.at(t), 0.5), "at {t}");
        }
        assert_eq!(s.at(0.5), Point::new(200.0, 125.0));
        assert_eq!(s.bounds(), Rect::new(100.0, 25.0, 200.0, 200.0));
    }

    #[test]
    fn wires_are_symmetric() {
        let (a, b, c) = (BlockId(1), BlockId(2), BlockId(3));
        assert_eq!(Wire::new(a, b), Wire::new(b, a));
        assert_ne!(Wire::new(a, b), Wire::new(a, c));
        assert_eq!(Wire::new(a, a), None, "a block isn't wired to itself");
        let wire = Wire::new(a, b).unwrap();
        assert!(wire.touches(a) && wire.touches(b) && !wire.touches(c));
        // The curve back is the curve there, run the other way.
        let (r, s) = (
            Rect::new(0.0, 0.0, 100.0, 50.0),
            Rect::new(300.0, 200.0, 80.0, 40.0),
        );
        let (there, back) = (Curve::between(r, s), Curve::between(s, r));
        assert_eq!(
            (back.from, back.c1, back.c2, back.to),
            (there.to, there.c2, there.c1, there.from)
        );
    }
}
```

In `lib.rs`, add `mod wire;` after `mod tile;` (line 15) and `pub use wire::{Curve, Wire};` after the `tile`
re-export (line 29).

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-canvas wire`
Expected: FAIL to compile (`Curve`, `Wire` not found in `super`).

- [ ] **Step 3: Implement**

`ink.rs:293`: `fn segment_distance` → `pub(crate) fn segment_distance` (the wire's hit reuses it).

Above the tests in `wire.rs`:

```rust
//! Wires between blocks (docs/specs/connections.md, Wires): which two blocks, and the curve
//! drawn between them. World units. No UI.

use crate::ink::segment_distance;
use crate::{BlockId, Point, Rect};

/// Pieces a curve is cut into to hit it: each is a few dozen pixels at most on screen, closer to
/// the curve than a click's reach.
const SEGMENTS: usize = 32;

/// Two blocks wired together. It has no direction: the wire from `a` to `b` is the wire from `b`
/// to `a`.
#[derive(Clone, Copy, Debug)]
pub struct Wire {
    pub a: BlockId,
    pub b: BlockId,
}

impl Wire {
    /// The wire between `a` and `b`; None when they are one block.
    pub fn new(a: BlockId, b: BlockId) -> Option<Wire> {
        (a != b).then_some(Wire { a, b })
    }

    /// One of its ends is block `id`.
    pub fn touches(self, id: BlockId) -> bool {
        self.a == id || self.b == id
    }
}

impl PartialEq for Wire {
    fn eq(&self, other: &Wire) -> bool {
        (self.a, self.b) == (other.a, other.b) || (self.a, self.b) == (other.b, other.a)
    }
}

impl Eq for Wire {}

/// A wire's curve: a cubic Bézier from `from`, the middle of the edge of one block that faces the
/// other, to `to`, the middle of the other's edge facing back. It leaves and arrives square to
/// those edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    pub from: Point,
    pub c1: Point,
    pub c2: Point,
    pub to: Point,
}

impl Curve {
    /// The curve from rect `a` to rect `b`: between their facing sides (left and right) when the
    /// gap between them across is the wider, else between their top and bottom. A rect of no
    /// size is a point: the pointer, while the Wire tool draws.
    pub fn between(a: Rect, b: Rect) -> Curve {
        let (ca, cb) = (center(a), center(b));
        let across = (b.x - a.right()).max(a.x - b.right());
        let down = (b.y - a.bottom()).max(a.y - b.bottom());
        if across >= down {
            let (from, to) = if cb.x >= ca.x {
                (Point::new(a.right(), ca.y), Point::new(b.x, cb.y))
            } else {
                (Point::new(a.x, ca.y), Point::new(b.right(), cb.y))
            };
            // Halfway across, each end pulls the curve out square to its edge.
            let pull = (to.x - from.x) / 2.0;
            Curve {
                from,
                c1: Point::new(from.x + pull, from.y),
                c2: Point::new(to.x - pull, to.y),
                to,
            }
        } else {
            let (from, to) = if cb.y >= ca.y {
                (Point::new(ca.x, a.bottom()), Point::new(cb.x, b.y))
            } else {
                (Point::new(ca.x, a.y), Point::new(cb.x, b.bottom()))
            };
            let pull = (to.y - from.y) / 2.0;
            Curve {
                from,
                c1: Point::new(from.x, from.y + pull),
                c2: Point::new(to.x, to.y - pull),
                to,
            }
        }
    }

    /// The point `t` of the way along it: 0 is `from`, 1 is `to`.
    pub fn at(&self, t: f32) -> Point {
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        Point::new(
            a * self.from.x + b * self.c1.x + c * self.c2.x + d * self.to.x,
            a * self.from.y + b * self.c1.y + c * self.c2.y + d * self.to.y,
        )
    }

    /// `p` is within `tolerance` of it.
    pub fn hit(&self, p: Point, tolerance: f32) -> bool {
        let along = |i: usize| self.at(i as f32 / SEGMENTS as f32);
        (0..SEGMENTS).any(|i| segment_distance(p, along(i), along(i + 1)) <= tolerance)
    }

    /// The box its four points span: the curve lies inside it.
    pub fn bounds(&self) -> Rect {
        [self.c1, self.c2, self.to]
            .iter()
            .fold(Rect::corners(self.from, self.from), |r, p| {
                r.union(&Rect::corners(*p, *p))
            })
    }
}

fn center(r: Rect) -> Point {
    Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum-canvas`
Expected: PASS (the new three and the existing ones).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/templum-canvas
git commit -m "feat(canvas): run a wire's curve from the edge of one block that faces the other" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 2: Wiring blocks: the model, its undo steps, closing, and a block's menu

The first real use of wiring is the block menu, so it comes with the model (no code without a caller).

**Files:**
- Create: `crates/templum/src/workspace/wires.rs`
- Modify: `crates/templum/src/workspace/mod.rs` (`mod wires;` after `mod view;` :31; `Wire` in the
  `templum_canvas` import :51-54; `Space::wires` after `selected_ink` :456; `Space::empty` :564-586)
- Modify: `crates/templum/src/workspace/history.rs` (`Change` :14-21, `ids` :24-30, `undo_canvas` :80-120, `reopen`
  :124-152)
- Modify: `crates/templum/src/workspace/blocks.rs` (`close_now` :507-535, `take_block` :561-586)
- Modify: `crates/templum/src/menu/mod.rs` (`tail` :160-185, `Tail::Canvas` :474-476)
- Modify: `crates/templum/src/menu/build.rs` (import :12, `Tail::Canvas` arm :198-210, a new `wiring` function)
- Modify: `crates/templum/src/workspace/tests/menus.rs:211-212` (the terminal's menu gained an item)
- Create: `crates/templum/src/workspace/tests/wires.rs`; add `mod wires;` to `workspace/tests/mod.rs` between
  `mod welcome;` and `mod workspaces;` (:650-651)

**Interfaces:**
- Consumes: `templum_canvas::{Wire, Wire::new, Wire::touches}` (Task 1).
- Produces: `Space::wires: Vec<Wire>`; `Space::wired(&self, a: BlockId, b: BlockId) -> bool` (`pub(crate)`);
  `Space::add_wire(&mut self, a: BlockId, b: BlockId) -> Option<Wire>` (`pub(crate)`, no undo step);
  `Space::unwire(&mut self, id: BlockId)` (`pub(super)`); `Workspace::connect(&mut self, a: BlockId, b: BlockId, cx:
  &mut Context<Self>) -> bool` and `Workspace::disconnect(&mut self, wire: Wire, cx: &mut Context<Self>) -> bool`
  (`pub(crate)`, each a step); `Workspace::set_wire(&mut self, wire: Wire, on: bool, then: Then, cx: &mut
  Context<Self>) -> bool` (`pub(super)`); `history::Change::{Connect(Wire), Disconnect(Wire), Open(Vec<(BlockId,
  session::Item)>, Vec<Wire>)}`; `menu::Tail::Canvas { group, connect: Vec<(BlockId, SharedString)>, disconnect:
  Vec<(BlockId, SharedString)> }`.

- [ ] **Step 1: Write the failing tests**

`crates/templum/src/workspace/tests/wires.rs`:

```rust
//! Wires between blocks (docs/specs/connections.md, Wires).

use templum_canvas::Wire;

use super::*;
use crate::workspace::history::Then;

/// A click on the empty canvas: no block keeps the keyboard.
fn unfocus(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    let empty = at(ws, cx, (700.0, 300.0));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        click(window, cx, empty, 1);
    })
    .unwrap();
}

/// The block ids (`uid`) of blocks `ids`.
fn uids<const N: usize>(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    ids: [BlockId; N],
) -> [String; N] {
    ws.read_with(cx, |ws, _| ids.map(|id| ws.space.uids[&id].clone()))
}

fn wires(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> Vec<Wire> {
    ws.read_with(cx, |ws, _| ws.space.wires.clone())
}

/// Right-clicks block `id`'s title, picks the menu's item `index` (separators count), then the
/// submenu's item `sub`.
fn pick(handle: AnyWindowHandle, cx: &mut TestAppContext, id: BlockId, index: usize, sub: usize) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.right_click(("title", id.0 as usize), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(index, cx);
        window.render_frame(cx);
        window.within("submenu").within("popup-menu").click(sub, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn two_blocks_are_wired_once_and_never_to_themselves(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        assert!(ws.connect(kanban, note, cx));
        assert!(!ws.connect(note, kanban, cx), "already wired, whichever end asks");
        assert!(!ws.connect(kanban, kanban, cx), "a block to itself");
        assert!(!ws.connect(kanban, BlockId(999), cx), "a block that isn't there");
        assert!(ws.space.wired(note, kanban));
        assert_eq!(ws.space.wires.len(), 1);
        assert_eq!(ws.space.undo.len(), 1, "the refused ones are no steps");
    });
}

#[gpui_kit::test]
fn ctrl_z_takes_a_wire_back_and_ctrl_shift_z_makes_it_again(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let wire = Wire::new(kanban, note).unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            assert!(ws.undo_canvas(false, window, cx));
            assert!(ws.space.wires.is_empty());
            assert!(ws.undo_canvas(true, window, cx));
            assert_eq!(ws.space.wires, [wire]);
            assert!(ws.disconnect(wire, cx));
            assert!(ws.space.wires.is_empty());
            assert!(ws.undo_canvas(false, window, cx), "unwiring is a step too");
            assert_eq!(ws.space.wires, [wire]);
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_a_block_takes_its_wires_and_undoing_brings_them_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let [kanban_uid] = uids(&ws, cx, [kanban]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            ws.connect(kanban, terminal, cx);
            ws.connect(note, terminal, cx);
            ws.close_now(&[kanban], Then::New, window, cx);
            assert_eq!(ws.space.wires, [Wire::new(note, terminal).unwrap()]);
            assert!(ws.undo_canvas(false, window, cx));
            let back = ws.space.block_by_uid(&kanban_uid).expect("back, with its id");
            assert!(ws.space.wired(back, note) && ws.space.wired(back, terminal));
            assert_eq!(ws.space.wires.len(), 3);
            // Closed again (Ctrl+Shift+Z), back again (Ctrl+Z): its wires each time.
            assert!(ws.undo_canvas(true, window, cx));
            assert_eq!(ws.space.wires.len(), 1);
            assert!(ws.undo_canvas(false, window, cx));
            assert_eq!(ws.space.wires.len(), 3);
        })
    })
    .unwrap();
}

/// Two wired blocks closed one after the other, both brought back: each got a new canvas id, and
/// the wire comes back between them.
#[gpui_kit::test]
fn a_wire_comes_back_between_two_blocks_closed_one_after_the_other(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let [k, n] = uids(&ws, cx, [kanban, note]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            ws.close_now(&[kanban], Then::New, window, cx);
            ws.close_now(&[note], Then::New, window, cx);
            assert!(ws.space.wires.is_empty());
            assert!(ws.undo_canvas(false, window, cx));
            assert!(ws.space.wires.is_empty(), "its other end is still closed");
            assert!(ws.undo_canvas(false, window, cx));
            let (kanban, note) = (
                ws.space.block_by_uid(&k).unwrap(),
                ws.space.block_by_uid(&n).unwrap(),
            );
            assert_eq!(ws.space.wires, [Wire::new(kanban, note).unwrap()]);
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_block_moved_to_another_workspace_leaves_its_wires(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            ws.connect(note, terminal, cx);
            ws.move_blocks(vec![kanban], None, window, cx);
        })
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let counts: Vec<usize> = ws.spaces().map(|s| s.wires.len()).collect();
        assert_eq!(counts.iter().sum::<usize>(), 1, "note–terminal stays: {counts:?}");
    });
}

#[gpui_kit::test]
fn a_pasted_copy_of_a_wired_block_comes_without_wires(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    unfocus(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
        ws.space.selected = vec![kanban];
    });
    cx.update_window(handle, |_, window, cx| {
        window.press(COPY, cx);
        window.press(PASTE, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_ne!(ws.space.selected, [kanban], "the copy is selected");
    });
    assert_eq!(wires(&ws, cx), [Wire::new(kanban, note).unwrap()]);
}

#[gpui_kit::test]
fn a_blocks_menu_connects_and_disconnects_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    // Board list, —, Fullscreen, Connect to ▸ (the terminal, the note: back to front).
    pick(handle, cx, kanban, 3, 1);
    assert_eq!(wires(&ws, cx), [Wire::new(kanban, note).unwrap()]);
    // Board list, —, Fullscreen, Connect to ▸ (the terminal), Disconnect from ▸ (the note).
    pick(handle, cx, kanban, 4, 0);
    assert!(wires(&ws, cx).is_empty());
    // Both were steps: Ctrl+Z wires them again.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))
    })
    .unwrap();
    assert_eq!(wires(&ws, cx), [Wire::new(kanban, note).unwrap()]);
}
```

(Right-clicking the kanban block focuses and raises it (`block.rs:626-637`), so the canvas order the menu lists is
terminal, note, kanban; the startup terminal is first, `three_blocks` adds the kanban block then the note.)

Add `mod wires;` to `workspace/tests/mod.rs` between `mod welcome;` and `mod workspaces;`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum wires`
Expected: FAIL to compile (`connect`, `wired`, `wires` not found).

- [ ] **Step 3: The model and the steps**

`crates/templum/src/workspace/wires.rs`:

```rust
//! Wires between blocks (docs/specs/connections.md, Wires): wiring and unwiring them, each a
//! step of Ctrl+Z.

use gpui_kit::Context;
use templum_canvas::{BlockId, Wire};

use super::history::{Change, Then};
use super::{Space, Workspace};

impl Space {
    /// `a` and `b` are wired.
    pub(crate) fn wired(&self, a: BlockId, b: BlockId) -> bool {
        Wire::new(a, b).is_some_and(|w| self.wires.contains(&w))
    }

    /// Wires `a` and `b`, as no step of Ctrl+Z (a restored canvas, the bench). None for one
    /// block, two already wired, or a block not on this canvas.
    pub(crate) fn add_wire(&mut self, a: BlockId, b: BlockId) -> Option<Wire> {
        let wire = Wire::new(a, b)?;
        if self.wired(a, b) || self.canvas.get(a).is_none() || self.canvas.get(b).is_none() {
            return None;
        }
        self.wires.push(wire);
        Some(wire)
    }

    /// False when they weren't wired.
    fn remove_wire(&mut self, wire: Wire) -> bool {
        let before = self.wires.len();
        self.wires.retain(|w| *w != wire);
        self.wires.len() < before
    }

    /// Block `id` leaves the canvas: its wires go.
    pub(super) fn unwire(&mut self, id: BlockId) {
        self.wires.retain(|w| !w.touches(id));
    }
}

impl Workspace {
    /// Wires `a` and `b`, a step Ctrl+Z takes back. False, silently, when they can't be: one
    /// block, already wired, or one isn't on the canvas.
    pub(crate) fn connect(&mut self, a: BlockId, b: BlockId, cx: &mut Context<Self>) -> bool {
        self.set_wire(Wire { a, b }, true, Then::New, cx)
    }

    /// Unwires them, a step Ctrl+Z takes back. False when they weren't wired.
    pub(crate) fn disconnect(&mut self, wire: Wire, cx: &mut Context<Self>) -> bool {
        self.set_wire(wire, false, Then::New, cx)
    }

    /// Wires (`on`) or unwires `wire`; the change that takes it back is kept as `then` says.
    pub(super) fn set_wire(
        &mut self,
        wire: Wire,
        on: bool,
        then: Then,
        cx: &mut Context<Self>,
    ) -> bool {
        let done = if on {
            self.space.add_wire(wire.a, wire.b).is_some()
        } else {
            self.space.remove_wire(wire)
        };
        if done {
            let back = if on {
                Change::Disconnect(wire)
            } else {
                Change::Connect(wire)
            };
            self.keep(back, then);
            cx.notify();
        }
        done
    }
}
```

`workspace/mod.rs`: `mod wires;` after `mod view;`; add `Wire` to the `use templum_canvas::{…}` list; in `Space`,
after `selected_ink` (:456):

```rust
    /// The wires between its blocks (docs/specs/connections.md), oldest first.
    pub wires: Vec<Wire>,
```

and in `Space::empty`, after `self.selected_ink = None;`: `self.wires.clear();`.

`history.rs`: `use templum_canvas::{BlockId, Rect, Wire};`, then:

```rust
pub(crate) enum Change {
    /// Close these blocks (they were added).
    Close(Vec<BlockId>),
    /// Bring these blocks back as they were when closed, by the ids they had, with the wires they
    /// had.
    Open(Vec<(BlockId, session::Item)>, Vec<Wire>),
    /// Put these blocks at these rects (they were moved or resized).
    Place(Vec<(BlockId, Rect)>),
    /// Wire these blocks (they were unwired).
    Connect(Wire),
    /// Unwire them (they were wired).
    Disconnect(Wire),
}

impl Change {
    fn ids(&mut self) -> Vec<&mut BlockId> {
        match self {
            Change::Close(ids) => ids.iter_mut().collect(),
            Change::Open(items, wires) => items
                .iter_mut()
                .map(|(id, _)| id)
                .chain(wires.iter_mut().flat_map(|Wire { a, b }| [a, b]))
                .collect(),
            Change::Place(rects) => rects.iter_mut().map(|(id, _)| id).collect(),
            Change::Connect(Wire { a, b }) | Change::Disconnect(Wire { a, b }) => vec![a, b],
        }
    }
}
```

In `undo_canvas`, replace the `gone` computation and the `match change` (:96-117):

```rust
            // A step whose blocks are all gone (a shell that exited) has nothing to take back; nor
            // has a wire's when one of its blocks is.
            let wire = matches!(change, Change::Connect(_) | Change::Disconnect(_));
            let missing = |id: &&mut BlockId| self.space.canvas.get(**id).is_none();
            let gone = !matches!(change, Change::Open(..))
                && if wire {
                    change.ids().iter().any(missing)
                } else {
                    change.ids().iter().all(missing)
                };
            if gone {
                self.take_step(then);
                continue;
            }
            match change {
                // It may ask first (a terminal at work, unsaved files).
                Change::Close(ids) => self.close_blocks(ids, then, window, cx),
                Change::Open(items, wires) => {
                    self.take_step(then);
                    self.reopen(items, wires, then, window, cx);
                }
                Change::Place(rects) => {
                    self.take_step(then);
                    self.place(rects, then, cx);
                }
                Change::Connect(wire) => {
                    self.take_step(then);
                    self.set_wire(wire, true, then, cx);
                }
                Change::Disconnect(wire) => {
                    self.take_step(then);
                    self.set_wire(wire, false, then, cx);
                }
            }
            return true;
```

`reopen` takes the wires and brings them back once the blocks are:

```rust
    /// Brings closed blocks back where they were, selected (a terminal as a shell in its folder,
    /// resuming its agent), with their wires to the blocks still there. The steps that named them,
    /// and their wires, name them by their new ids.
    fn reopen(
        &mut self,
        items: Vec<(BlockId, session::Item)>,
        mut wires: Vec<Wire>,
        then: Then,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (old, items): (Vec<_>, Vec<_>) = items.into_iter().unzip();
        let added = self.add_selected(&items, window, cx);
        for (item, new) in items.iter().zip(&added) {
            if let Some(new) = new {
                self.adopt_uid(*new, item.uid());
            }
        }
        let mut ids = Vec::new();
        for (old, new) in old.into_iter().zip(added) {
            let Some(new) = new else { continue };
            let steps = self.space.undo.iter_mut().chain(&mut self.space.redo);
            let ends = wires.iter_mut().flat_map(|Wire { a, b }| [a, b]);
            for id in steps.flat_map(|(_, change)| change.ids()).chain(ends) {
                if *id == old {
                    *id = new;
                }
            }
            ids.push(new);
        }
        // A wire whose other block is closed too, or gone, stays out.
        for wire in wires {
            self.space.add_wire(wire.a, wire.b);
        }
        if !ids.is_empty() {
            self.keep(Change::Close(ids), then);
        }
    }
```

(Canvas ids are never reused (`templum-canvas/src/canvas.rs:32-38`), so a new id never equals an old one and the
renaming can't chain.)

`blocks.rs`, `close_now` (:507-535): collect the closed blocks' wires before they go, and keep them in the step:

```rust
        let items: Vec<_> = self
            .space
            .canvas
            .blocks()
            .iter()
            .filter(|b| ids.contains(&b.id))
            .map(|b| (b.id, self.space.item_of(b, cx)))
            .collect();
        // Their wires go with them; undoing the close brings them back.
        let wires: Vec<_> = self
            .space
            .wires
            .iter()
            .filter(|w| ids.iter().any(|id| w.touches(*id)))
            .copied()
            .collect();
        let was_focused = self.space.focused.is_some_and(|id| ids.contains(&id));
        for id in ids {
            self.remove_block(*id, window, cx);
        }
        if was_focused {
            self.space.focused = None;
        }
        if !items.is_empty() {
            self.keep(Change::Open(items, wires), then);
        }
```

`take_block` (:561-586), the one way a block leaves a canvas (closing, a shell's exit, moving to another
workspace): after `self.space.uids.remove(&id);` add `self.space.unwire(id);`.

- [ ] **Step 4: The block menu**

`menu/mod.rs`, `Tail` (:474-476):

```rust
    /// On the canvas: Fullscreen, for the block or the selection it was in; Connect to ▸ the
    /// blocks not wired to it and Disconnect from ▸ those that are, by title, back to front.
    Canvas {
        group: Vec<BlockId>,
        connect: Vec<(BlockId, SharedString)>,
        disconnect: Vec<(BlockId, SharedString)>,
    },
```

`tail` (:160-162), the `None` arm:

```rust
            None => {
                let (disconnect, connect) = self
                    .space
                    .canvas
                    .blocks()
                    .iter()
                    .filter(|b| b.id != block)
                    .map(|b| (b.id, crate::block::title(self, &self.space.blocks[&b.id], cx)))
                    .partition(|(id, _)| self.space.wired(block, *id));
                Tail::Canvas {
                    group,
                    connect,
                    disconnect,
                }
            }
```

`menu/build.rs`: `use templum_canvas::{BlockId, Point, Wire};`; the `Tail::Canvas` arm (:198-210) becomes:

```rust
                    Tail::Canvas {
                        group,
                        connect,
                        disconnect,
                    } => {
                        let menu = menu.item(item(
                            ws,
                            "Fullscreen",
                            Some(IconName::Maximize2),
                            move |ws, window, cx| {
                                let ids = if group.contains(&block) {
                                    group.clone()
                                } else {
                                    vec![block]
                                };
                                ws.enter_grid(ids, window, cx)
                            },
                        ));
                        let menu = wiring(ws, menu, block, connect, true, window, cx);
                        wiring(ws, menu, block, disconnect, false, window, cx)
                    }
```

and beside `send_to`:

```rust
/// Connect to ▸ (`on`) the blocks listed, or Disconnect from ▸ them; nothing when there are none.
fn wiring(
    ws: &Entity<Workspace>,
    menu: PopupMenu,
    block: BlockId,
    blocks: Vec<(BlockId, SharedString)>,
    on: bool,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    if blocks.is_empty() {
        return menu;
    }
    let (icon, label) = if on {
        (IconName::Cable, "Connect to")
    } else {
        (IconName::Unplug, "Disconnect from")
    };
    let ws = ws.clone();
    menu.submenu_with_icon(
        Some(Icon::new(icon)),
        label,
        window,
        cx,
        move |menu, _, _| {
            blocks.iter().fold(menu, |menu, (other, title)| {
                let other = *other;
                menu.item(item(&ws, title.clone(), None, move |ws, _, cx| {
                    if on {
                        ws.connect(block, other, cx);
                    } else {
                        ws.disconnect(Wire { a: block, b: other }, cx);
                    }
                }))
            })
        },
    )
}
```

(`IconName::Cable` and `IconName::Unplug` are Lucide's `cable.svg` and `unplug.svg` in
`gpui-kit-assets-0.7.3/assets/icons/`; `main.rs:73` loads `gpui_kit::assets::AllAssets`, every bundled icon.
`submenu_with_icon`: `gpui-component-0.7.3/src/menu/popup_menu.rs:707-714`.)

`workspace/tests/menus.rs:211-212`: the terminal's menu now lists the kanban block under Connect to ▸:

```rust
        // Copy, Paste, —, Fullscreen, Connect to ▸, Move to workspace ▸, Close terminal.
        window.within("popup-menu").click(6usize, cx);
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p templum wires`, then `cargo test -p templum` (every menu test clicks items by index: only
`menus.rs:212` moves).
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/templum
git commit -m "feat(connections): connect and disconnect blocks from a block's menu, with Ctrl+Z" -m "A closed block's wires come back when Ctrl+Z brings it back; a block moved to another workspace leaves its wires." -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 3: Painting wires under the blocks

**Files:**
- Create: `crates/templum/src/wires.rs` (and `mod wires;` in `main.rs` between `mod welcome;` and `mod workspace;`
  :37-38)
- Modify: `crates/templum/src/workspace/wires.rs` (`Space::curve`, `Workspace::wire_curves`)
- Modify: `crates/templum/src/workspace/render.rs:431-446` (the layer under the blocks)
- Modify: `crates/templum/src/bench.rs` (doc :1-16, mixed :36-58, far :291-297, a `wire_in_a_chain` helper)
- Test: `crates/templum/src/wires.rs` (unit), `crates/templum/src/workspace/tests/wires.rs`

**Interfaces:**
- Consumes: `Space::wires`, `Space::add_wire` (Task 2); `Curve::{between, bounds}` (Task 1).
- Produces: `crate::wires::{WIDTH: f32 (pub(crate), world units), width(zoom: f32) -> f32, paint(curves: &[(Curve,
  Hsla)], camera: &Camera, bounds: Bounds<Pixels>, window: &mut Window)}`; `Space::curve(&self, wire: Wire) ->
  Option<Curve>` (`pub(super)`); `Workspace::wire_curves(&self) -> Vec<(Curve, Hsla)>` (`pub(super)`).

- [ ] **Step 1: Write the failing tests**

Append to `workspace/tests/wires.rs`:

```rust
#[gpui_kit::test]
fn wires_follow_their_blocks_in_the_titles_dim_color_and_grid_mode_hides_them(
    cx: &mut TestAppContext,
) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            // The kanban block (40, 40, 220 × 200) beside the note (300, 40, 220 × 200): from the
            // middle of one's right edge to the middle of the other's left edge.
            let curves = ws.wire_curves();
            assert_eq!(curves.len(), 1);
            assert_eq!(
                (curves[0].0.from, curves[0].0.to),
                (Point::new(260.0, 140.0), Point::new(300.0, 140.0))
            );
            assert_eq!(curves[0].1, ws.ui.title_dim);
            // The note moves down: the wire follows.
            ws.space.canvas.get_mut(note).unwrap().rect.y = 100.0;
            assert_eq!(ws.wire_curves()[0].0.to, Point::new(300.0, 200.0));
            ws.enter_grid(vec![kanban, note], window, cx);
            assert!(ws.wire_curves().is_empty(), "grid mode hides wires");
            assert_eq!(ws.space.wires.len(), 1, "and keeps them");
            ws.leave_grid(window, cx);
            assert_eq!(ws.wire_curves().len(), 1);
        });
        // A frame paints them.
        window.render_frame(cx);
    })
    .unwrap();
}
```

`crates/templum/src/wires.rs`, its test module (the code above it comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wire_is_2_units_wide_and_never_under_a_pixel() {
        // At 50% and below, where the Card level comes for any font of 5 px or more: 1 px.
        assert_eq!(width(0.25), 1.0);
        assert_eq!(width(0.33), 1.0);
        assert_eq!(width(0.5), 1.0);
        assert_eq!(width(1.0), 2.0);
        assert_eq!(width(2.0), 4.0);
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum wires`
Expected: FAIL to compile (`wire_curves`, `width` not found).

- [ ] **Step 3: Implement**

`crates/templum/src/wires.rs`, above the tests:

```rust
//! Painting the wires between blocks, under them (docs/specs/connections.md, Wires).

use gpui_kit::{Bounds, Hsla, PathBuilder, Pixels, Window, point, px};
use templum_canvas::{Camera, Curve, Point, Rect};

/// A wire's width, world units.
pub(crate) const WIDTH: f32 = 2.0;

/// A wire's width on screen at `zoom`, pixels: never under one, so at 50% and below (the Card
/// level of docs/specs/level-of-detail.md comes there) it is 1 px.
pub(crate) fn width(zoom: f32) -> f32 {
    (WIDTH * zoom).max(1.0)
}

/// Paints `curves` (world units) through `camera` into `bounds`, those off screen skipped.
// ponytail: one path per wire per frame (gpui-fast keeps the triangles of a shape it built
// before, path_cache.rs); cache them per zoom step as ink.rs does if a bench asks for it.
pub(crate) fn paint(
    curves: &[(Curve, Hsla)],
    camera: &Camera,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let z = camera.zoom();
    let seen = Rect::new(
        camera.origin.x,
        camera.origin.y,
        f32::from(bounds.size.width) / z,
        f32::from(bounds.size.height) / z,
    );
    let at = |p: Point| {
        let s = camera.to_screen(p);
        point(bounds.origin.x + px(s.x), bounds.origin.y + px(s.y))
    };
    for (curve, color) in curves {
        if !curve.bounds().intersects(&seen) {
            continue;
        }
        let mut path = PathBuilder::stroke(px(width(z)));
        path.move_to(at(curve.from));
        path.cubic_bezier_to(at(curve.to), at(curve.c1), at(curve.c2));
        if let Ok(path) = path.build() {
            window.paint_path(path, *color);
        }
    }
}
```

(`PathBuilder::stroke` and `cubic_bezier_to(to, control_a, control_b)`: `gpui-fast-0.1.8/src/path_builder.rs:89`,
`:145-154`; `Window::paint_path`: `gpui-fast-0.1.8/src/window.rs:4804`.)

`workspace/wires.rs`: imports become `use gpui_kit::{Context, Hsla};` and
`use templum_canvas::{BlockId, Curve, Wire};`; add to `impl Space`:

```rust
    /// `wire`'s curve between its blocks' rects; None when one isn't on the canvas.
    pub(super) fn curve(&self, wire: Wire) -> Option<Curve> {
        Some(Curve::between(
            self.canvas.get(wire.a)?.rect,
            self.canvas.get(wire.b)?.rect,
        ))
    }
```

and to `impl Workspace`:

```rust
    /// The wires to paint under the blocks, with their colors: the title's dim one. Grid mode
    /// hides them.
    pub(super) fn wire_curves(&self) -> Vec<(Curve, Hsla)> {
        if self.space.grid.is_some() {
            return Vec::new();
        }
        self.space
            .wires
            .iter()
            .filter_map(|w| Some((self.space.curve(*w)?, self.ui.title_dim)))
            .collect()
    }
```

`render.rs`, the layer under the blocks (:431-446): read the curves with the other captured values, and paint them
between the dots and the snap guides:

```rust
        let accent = self.ui.accent;
        // The wires, under the blocks (none in grid mode).
        let wires = self.wire_curves();
        // A full-size layer under the blocks: the IME handler, the workspace origin, and
        // window-wide drag listeners (drags keep working outside the window).
        let layer = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                // Grid mode shows no canvas.
                if !tiled {
                    paint_dots(&camera, dot, bounds, window);
                    crate::wires::paint(&wires, &camera, bounds, window);
                    paint_guides(&camera, accent, bounds, &guides, window);
                }
```

(the rest of the closure unchanged). The layer is the area's first child (`render.rs:519`), so every block paints
over the wires.

`bench.rs`: the benches paint wires too. At the end of the file:

```rust
/// Wires each block to the next, back to front, as a cockpit's are: the benches paint wires.
fn wire_in_a_chain(ws: &mut Workspace) {
    let ids: Vec<_> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
    for pair in ids.windows(2) {
        ws.space.add_wire(pair[0], pair[1]);
    }
}
```

call `wire_in_a_chain(ws);` in the mixed branch right before its `return;` (:58), and in `run_far` right before
`// No block focused: …` (:294). In the module doc, the mixed scenario's "a note and 2 kanban blocks, no strokes"
becomes "a note and 2 kanban blocks, each wired to the next, no strokes", and the far one's "3 kanban blocks), none
focused" becomes "3 kanban blocks), each wired to the next, none focused".

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum wires`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/templum
git commit -m "feat(connections): draw wires under the blocks, following them as they move" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 4: The Wire tool

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs` (`Drag` :340-384, `Tool` :673-683)
- Modify: `crates/templum/src/workspace/drawing.rs:144` (`begin_ink`'s match)
- Modify: `crates/templum/src/workspace/wires.rs` (`Space::block_at`, `begin_wire`, `end_wire`, the drawn wire in
  `wire_curves`)
- Modify: `crates/templum/src/workspace/input.rs` (`drag_to` :151-245, `release` :436-469)
- Modify: `crates/templum/src/workspace/render.rs` (a layer after the ink layer :536-556)
- Modify: `crates/templum/src/toolbar.rs:43-71` (the Pointer group)
- Test: `crates/templum/src/workspace/tests/wires.rs`

**Interfaces:**
- Consumes: `Workspace::connect` (Task 2), `Workspace::wire_curves` (Task 3), `Curve::between`.
- Produces: `Tool::Wire`; `Drag::Wire { from: BlockId, to: Point }`; `Space::block_at(&self, p: Point) ->
  Option<BlockId>` (`pub(super)`); `Workspace::begin_wire(&mut self, e: &MouseDownEvent)` and
  `Workspace::end_wire(&mut self, from: BlockId, at: Point, cx: &mut Context<Self>)` (`pub(super)`).

- [ ] **Step 1: Write the failing test**

In `workspace/tests/wires.rs`, add `use crate::workspace::Tool;` to the imports, then:

```rust
#[gpui_kit::test]
fn the_wire_tool_wires_the_block_pressed_to_the_one_let_go_on(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let (from, over, empty) = (
        at(&ws, cx, (150.0, 140.0)),
        at(&ws, cx, (410.0, 140.0)),
        at(&ws, cx, (700.0, 300.0)),
    );
    let event = |p: (f32, f32)| point(px(p.0), px(p.1));
    let plain = Modifiers::default();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-wire", cx);
        window.render_frame(cx);
        assert_eq!(window.find("tool-wire").selected(), Some(true));
        // Let go on the empty canvas, or on the block it started on: nothing.
        press_with(window, cx, MouseButton::Left, plain, from, empty);
        press_with(window, cx, MouseButton::Left, plain, from, from);
        assert!(ws.read(cx).space.wires.is_empty());
        // Held over the note: the curve already runs to its facing edge.
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: event(from),
                modifiers: plain,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: event(over),
                pressed_button: Some(MouseButton::Left),
                modifiers: plain,
            }),
            cx,
        );
        let drawn = ws.read(cx).wire_curves();
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].0.to, Point::new(300.0, 140.0));
        assert_eq!(drawn[0].1, ws.read(cx).ui.accent);
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: event(over),
                modifiers: plain,
                click_count: 1,
            }),
            cx,
        );
        window.render_frame(cx);
        let w = ws.read(cx);
        assert_eq!(w.space.wires, [Wire::new(kanban, note).unwrap()]);
        assert_eq!(w.tool, Tool::Wire, "it stays on");
        assert_eq!(w.space.focused, None, "Ctrl+Z reaches the canvas");
        // Ctrl+Z takes it back; Esc puts Select back.
        window.press(UNDO, cx);
        assert!(ws.read(cx).space.wires.is_empty());
        window.press("escape", cx);
        assert_eq!(ws.read(cx).tool, Tool::Select);
    })
    .unwrap();
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum the_wire_tool`
Expected: FAIL to compile (`Tool::Wire` not found).

- [ ] **Step 3: Implement**

`workspace/mod.rs`, `Tool`, after `Hand`:

```rust
    /// Wires two blocks: pressed on one, let go on another. Stays on.
    Wire,
```

`Drag`, after `MoveInk`:

```rust
    /// The Wire tool pressed on block `from`; the pointer is at the world point `to`.
    Wire { from: BlockId, to: Point },
```

`drawing.rs:144`: `Tool::Select | Tool::Hand | Tool::Wire => return,`.

`workspace/wires.rs`: imports become `use gpui_kit::{Context, Hsla, MouseDownEvent};`,
`use templum_canvas::{BlockId, Curve, Point, Rect, Wire};` and `use super::{Drag, Space, Workspace};`. Add to
`impl Space`:

```rust
    /// The top block drawn at a world point.
    pub(super) fn block_at(&self, p: Point) -> Option<BlockId> {
        self.canvas
            .blocks()
            .iter()
            .rev()
            .find(|b| self.shown(b.id) && b.rect.contains(p))
            .map(|b| b.id)
    }
```

and to `impl Workspace`:

```rust
    /// A press with the Wire tool: on a block, a wire starts from it. No block keeps the
    /// keyboard, so Ctrl+Z takes the wire back rather than reaching a terminal.
    pub(super) fn begin_wire(&mut self, e: &MouseDownEvent) {
        self.space.pristine = false;
        self.press = None;
        let at = self.space.canvas.camera.to_world(self.local(e.position));
        self.drag = self
            .space
            .block_at(at)
            .map(|from| Drag::Wire { from, to: at });
        self.space.focused = None;
    }

    /// The Wire tool let go at the world point `at`: on another block, the two are wired.
    pub(super) fn end_wire(&mut self, from: BlockId, at: Point, cx: &mut Context<Self>) {
        if let Some(to) = self.space.block_at(at) {
            self.connect(from, to, cx);
        }
        cx.notify();
    }
```

`wire_curves` gains the wire being drawn, after the wires (replace the function):

```rust
    /// The wires to paint under the blocks, with their colors: the title's dim one; and the one
    /// the Wire tool draws, in the accent, to the facing edge of the block under the pointer or
    /// to the pointer. Grid mode hides them.
    pub(super) fn wire_curves(&self) -> Vec<(Curve, Hsla)> {
        if self.space.grid.is_some() {
            return Vec::new();
        }
        let mut curves: Vec<(Curve, Hsla)> = self
            .space
            .wires
            .iter()
            .filter_map(|w| Some((self.space.curve(*w)?, self.ui.title_dim)))
            .collect();
        if let Some(Drag::Wire { from, to }) = &self.drag
            && let Some(a) = self.space.canvas.get(*from)
        {
            let end = self
                .space
                .block_at(*to)
                .filter(|b| b != from)
                .and_then(|b| self.space.canvas.get(b))
                .map_or(Rect::new(to.x, to.y, 0.0, 0.0), |b| b.rect);
            curves.push((Curve::between(a.rect, end), self.ui.accent));
        }
        curves
    }
```

`input.rs`, `drag_to`, an arm before `None => return,`:

```rust
            Some(Drag::Wire { to, .. }) => *to = self.space.canvas.camera.to_world(p),
```

`release`, first thing in the function:

```rust
        // The Wire tool let go: on another block, they are wired.
        if let Some(Drag::Wire { from, .. }) = self.drag {
            let at = self.space.canvas.camera.to_world(self.local(pos));
            self.end_wire(from, at, cx);
        }
```

(`end_drag` at its end clears the drag, as for every other.)

`render.rs`, after the ink layer's `.when(self.tool.draws() && !tiled, …)` (:536-556):

```rust
            // The Wire tool: a layer over the blocks takes the pointer, as a drawing tool's does.
            .when(self.tool == Tool::Wire && !tiled, |el| {
                el.child(
                    div()
                        .id("wire-layer")
                        .absolute()
                        .size_full()
                        .cursor_crosshair()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &MouseDownEvent, window, cx| {
                                window.focus(&ws.focus_handle, cx);
                                ws.begin_wire(e);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .on_scroll_wheel(cx.listener(Self::on_background_scroll))
                        .on_pinch(cx.listener(Self::on_pinch))
                        .test_support(),
                )
            })
```

The pointer's moves and its release reach the drag through the window-wide listeners the layer under the blocks
registers (`render.rs:449-484`; ADR-0002 lesson 6: an element's own move listener fires only while the pointer is
over it).

`toolbar.rs`, the Pointer group (:43-71), a third button after Hand:

```rust
        button(
            "tool-wire",
            IconName::Cable,
            "Wire: connect two blocks",
            tool == Tool::Wire,
            ui,
        )
        .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| ws.set_tool(Tool::Wire, window, cx)))
        .test_support()
        .into_any_element(),
```

Grid mode ends the tool (`tiling.rs:313` sets Select), and Esc already puts Select back from any tool
(`input.rs:612-623`).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum wires && cargo test -p templum drawing && cargo test -p templum navigation`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/templum
git commit -m "feat(connections): wire two blocks with the Wire tool" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 5: Selecting a wire, deleting it, its menu

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs` (`Space::selected_wire` after `wires`; `Space::empty`)
- Modify: `crates/templum/src/workspace/wires.rs` (`remove_wire`, `unwire`, `wire_at`, the selected color)
- Modify: `crates/templum/src/workspace/input.rs` (`on_background_right_down` :52-57, Esc :612-617, Delete after
  :624-632, `on_background_down` :764-787)
- Modify: `crates/templum/src/workspace/drawing.rs:17` (`set_tool`), `blocks.rs:42` (`added`) and `:708`
  (`focus_block`), `selection.rs:119` (`toggle_selected`), `tiling.rs:303` (`calm`), `spaces.rs:219` (`switch_to`)
- Modify: `crates/templum/src/menu/mod.rs` (`Target` :51-52, `items` :284, `Items` :634-636),
  `crates/templum/src/menu/build.rs:954-959`
- Test: `crates/templum/src/workspace/tests/wires.rs`

**Interfaces:**
- Consumes: `Space::curve`, `Workspace::disconnect`, `crate::wires::WIDTH` (Tasks 2-3).
- Produces: `Space::selected_wire: Option<Wire>`; `Workspace::wire_at(&self, world: Point) -> Option<Wire>`
  (`pub(super)`); `menu::Target::Wire(Wire)`, `menu::Items::Wire { wire: Wire }`.

- [ ] **Step 1: Write the failing tests**

In `workspace/tests/wires.rs`, the `templum_canvas` import becomes
`use templum_canvas::{Color, Drawing, ShapeKind, Wire};`; then:

```rust
/// Where the straight wire between the kanban block and the note runs (y 140, x 260 to 300), as
/// a window position.
fn on_the_wire(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> (f32, f32) {
    at(ws, cx, (280.0, 140.0))
}

#[gpui_kit::test]
fn a_click_selects_a_wire_and_delete_removes_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let wire = Wire::new(kanban, note).unwrap();
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
    });
    let on = on_the_wire(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        click(window, cx, on, 1);
        window.render_frame(cx);
        let w = ws.read(cx);
        assert_eq!(w.space.selected_wire, Some(wire));
        assert_eq!(w.space.focused, None, "Delete and Backspace go to the wire");
        assert_eq!(w.wire_curves()[0].1, w.ui.accent);
        window.press("escape", cx);
        assert_eq!(ws.read(cx).space.selected_wire, None, "Esc unselects");
        click(window, cx, on, 1);
        window.press("backspace", cx);
        assert!(ws.read(cx).space.wires.is_empty());
        assert_eq!(ws.read(cx).space.selected_wire, None);
        window.press(UNDO, cx);
        assert_eq!(ws.read(cx).space.wires, [wire], "Ctrl+Z brings it back");
    })
    .unwrap();
}

/// A selected wire never takes a terminal's Backspace: a block that took the keyboard since
/// (clicked, or just added) drops the wire's selection, and with a block focused the key is
/// never the wire's.
#[gpui_kit::test]
fn a_terminal_with_the_keyboard_keeps_its_backspace(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let wire = Wire::new(kanban, note).unwrap();
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
    });
    let on = on_the_wire(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        click(window, cx, on, 1);
        window.click(("body", terminal.0 as usize), cx);
        assert_eq!(ws.read(cx).space.focused, Some(terminal));
        assert_eq!(ws.read(cx).space.selected_wire, None);
        window.press("backspace", cx);
        click(window, cx, on, 1);
        window.press(NEW, cx);
        assert_eq!(ws.read(cx).space.selected_wire, None);
        window.press("backspace", cx);
        ws.update(cx, |ws, _| {
            ws.space.selected_wire = Some(wire);
            ws.space.focused = Some(terminal);
        });
        window.press("backspace", cx);
        assert_eq!(ws.read(cx).space.wires, [wire], "every Backspace was a terminal's");
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_wire_under_a_drawing_or_a_block_isnt_the_one_clicked(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
        // A line drawn across the wire: drawings are above wires.
        ws.space.ink.push(Drawing::Shape {
            kind: ShapeKind::Line,
            from: Point::new(280.0, 100.0),
            to: Point::new(280.0, 180.0),
            width: 4.0,
            color: Color::Red,
        });
    });
    let on = on_the_wire(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        click(window, cx, on, 1);
        assert_eq!(ws.read(cx).space.selected_ink, Some(0));
        assert_eq!(ws.read(cx).space.selected_wire, None);
        ws.update(cx, |ws, _| {
            ws.space.ink.clear();
            ws.space.selected_ink = None;
            // The terminal, behind the other two, now covers the gap the wire crosses.
            ws.space.canvas.get_mut(terminal).unwrap().rect = Rect::new(250.0, 100.0, 300.0, 200.0);
        });
        window.render_frame(cx);
        click(window, cx, on, 1);
        assert_eq!(ws.read(cx).space.focused, Some(terminal), "the block's click");
        assert_eq!(ws.read(cx).space.selected_wire, None);
    })
    .unwrap();
}

#[gpui_kit::test]
fn right_clicking_a_wire_offers_disconnect(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
    });
    let on = on_the_wire(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        right_click(window, cx, on);
        window.render_frame(cx);
        assert!(ws.read(cx).menu.is_some(), "the wire's menu");
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
        assert!(ws.read(cx).space.wires.is_empty());
    })
    .unwrap();
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum wires`
Expected: FAIL to compile (`selected_wire` not found).

- [ ] **Step 3: The selection**

`workspace/mod.rs`, `Space`, after `wires`:

```rust
    /// The selected wire: Delete removes it. Never with a block focused or selected.
    pub selected_wire: Option<Wire>,
```

`Space::empty`: after `self.wires.clear();` add `self.selected_wire = None;`.

`workspace/wires.rs`: `use templum_canvas::{BlockId, Curve, HIT_PX, Point, Rect, Wire};`; `remove_wire` and
`unwire` drop the selection with the wire:

```rust
    /// False when they weren't wired.
    fn remove_wire(&mut self, wire: Wire) -> bool {
        let before = self.wires.len();
        self.wires.retain(|w| *w != wire);
        if self.selected_wire == Some(wire) {
            self.selected_wire = None;
        }
        self.wires.len() < before
    }

    /// Block `id` leaves the canvas: its wires go.
    pub(super) fn unwire(&mut self, id: BlockId) {
        self.wires.retain(|w| !w.touches(id));
        if self.selected_wire.is_some_and(|w| w.touches(id)) {
            self.selected_wire = None;
        }
    }
```

add to `impl Workspace`:

```rust
    /// The wire under a world point, within a click's reach; the newest first.
    pub(super) fn wire_at(&self, world: Point) -> Option<Wire> {
        let reach = HIT_PX / self.space.canvas.camera.zoom() + crate::wires::WIDTH / 2.0;
        self.space
            .wires
            .iter()
            .rev()
            .copied()
            .find(|w| self.space.curve(*w).is_some_and(|c| c.hit(world, reach)))
    }
```

and in `wire_curves`, the wires' color (the `filter_map`) becomes:

```rust
            .filter_map(|w| {
                let color = if self.space.selected_wire == Some(*w) {
                    self.ui.accent
                } else {
                    self.ui.title_dim
                };
                Some((self.space.curve(*w)?, color))
            })
```

`input.rs`, `on_background_down`: drawings are hit first, then wires; one selection at a time:

```rust
        if e.modifiers.shift && e.click_count == 1 {
            self.space.selected_ink = None;
            self.space.selected_wire = None;
            self.begin_marquee(p);
            cx.notify();
            return;
        }
        if e.click_count == 1
            && self.tool == Tool::Select
            && let Some(index) = self.ink_at(world)
        {
            self.space.selected_ink = Some(index);
            self.space.selected_wire = None;
            self.space.focused = None;
            self.drag = Some(Drag::MoveInk {
                index,
                last: world,
                moved: false,
            });
            self.press = None;
            cx.notify();
            return;
        }
        // A wire, under the drawings: a click selects it for Delete, and no block keeps the
        // keyboard.
        if e.click_count == 1
            && self.tool == Tool::Select
            && let Some(wire) = self.wire_at(world)
        {
            self.space.selected_wire = Some(wire);
            self.space.selected_ink = None;
            self.space.selected.clear();
            self.space.focused = None;
            self.press = None;
            cx.notify();
            return;
        }
        let had_ink = self.space.selected_ink.take().is_some();
        if self.space.selected_wire.take().is_some() || had_ink {
            cx.notify();
        }
```

`on_background_right_down`, after the drawing's menu (:53-56):

```rust
        if let Some(wire) = self.wire_at(at) {
            self.open_menu(menu::Target::Wire(wire), e.position, window, cx);
            return;
        }
```

`on_key_down`: Esc's condition (:614-617) gains `|| self.space.selected_wire.is_some()` (it calls `set_tool`, which
unselects); after the selected drawing's Delete (:624-632):

```rust
        // A selected wire: Delete and Backspace remove it, while no block has the keyboard.
        if let Some(wire) = self.space.selected_wire
            && self.space.focused.is_none()
            && self.canvas_has_keys(window)
            && !k.modifiers.modified()
            && matches!(k.key.as_str(), "delete" | "backspace")
        {
            self.disconnect(wire, cx);
            cx.stop_propagation();
            return;
        }
```

Where a selected drawing is dropped, the selected wire is too (add `… selected_wire = None;` beside each):
`drawing.rs:17` (`set_tool`), `blocks.rs:708` (`focus_block`), `selection.rs:119` (`toggle_selected`),
`tiling.rs:303` (`calm`), `spaces.rs:219` (`switch_to`, `left.selected_wire = None;`); and `blocks.rs:42`
(`added`), after `self.space.selected.clear();`: a block just added takes the keyboard.

- [ ] **Step 4: The wire's menu**

`menu/mod.rs`: `use templum_canvas::{BlockId, Point, Wire};`; `Target`, after `Ink(usize)`:

```rust
    /// A wire between two blocks.
    Wire(Wire),
```

`items`, after `Target::Ink(index) => Items::Ink { index },`: `Target::Wire(wire) => Items::Wire { wire },`.
`Items`, after `Ink { index: usize },`:

```rust
    /// A wire: Disconnect.
    Wire {
        wire: Wire,
    },
```

`menu/build.rs`, after the `Items::Ink` arm (:954-959):

```rust
            Items::Wire { wire } => menu.item(item(
                ws,
                "Disconnect",
                Some(IconName::Unplug),
                move |ws, _, cx| {
                    ws.disconnect(wire, cx);
                },
            )),
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p templum wires && cargo test -p templum drawing && cargo test -p templum selection && cargo test
-p templum menus`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/templum
git commit -m "feat(connections): select a wire to delete it, or disconnect it from its menu" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 6: Saving wires

**Files:**
- Modify: `crates/templum/src/session.rs` (a `SavedWire` beside `SavedTile` :327-340; `Saved` :307-326; `Layout`
  :675-687; `snapshot` :741-742; the tests' literals :946 and :1521; a unit test)
- Modify: `crates/templum/src/workspace/mod.rs` (`Space::layout` :513-539)
- Modify: `crates/templum/src/workspace/persist.rs:68-73` (`restore`)
- Modify the tests' `Saved` literals: `workspace/tests/persistence.rs:319`, `workspace/tests/workspaces.rs:32`
- Test: `session.rs` tests, `workspace/tests/wires.rs`

**Interfaces:**
- Consumes: `Space::add_wire`, `Space::block_by_uid` (batch 1).
- Produces: `session::SavedWire { pub a: String, pub b: String }`; `Saved::wires: Vec<SavedWire>`;
  `Layout::wires: Vec<SavedWire>`.

- [ ] **Step 1: Write the failing tests**

`session.rs`, in its tests module:

```rust
    #[test]
    fn wires_are_written_by_block_id_and_left_out_when_there_are_none() {
        let mut saved = sample();
        assert!(
            !serde_json::to_string(&saved).unwrap().contains("wires"),
            "a file without wires keeps an older Templum's shape"
        );
        saved.wires = vec![SavedWire {
            a: "b7k2m".into(),
            b: "4qm2a".into(),
        }];
        let text = serde_json::to_string(&saved).unwrap();
        assert!(text.contains(r#""wires":[{"a":"b7k2m","b":"4qm2a"}]"#), "{text}");
        assert_eq!(serde_json::from_str::<Saved>(&text).unwrap(), saved);
    }
```

`workspace/tests/wires.rs`:

```rust
#[gpui_kit::test]
fn wires_come_back_after_a_restart(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let [k, n, t] = uids(&ws, cx, [kanban, note, terminal]);
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
        ws.connect(note, terminal, cx);
        ws.save_canvas_now(cx);
    });
    let saved =
        std::fs::read_to_string(data.path().join("workspaces").join("default.json")).unwrap();
    assert!(
        saved.contains(&format!(r#""wires":[{{"a":"{k}","b":"{n}"}},{{"a":"{n}","b":"{t}"}}]"#)),
        "{saved}"
    );
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        let id = |uid: &str| ws.space.block_by_uid(uid).unwrap();
        assert_eq!(ws.space.wires.len(), 2);
        assert!(ws.space.wired(id(&k), id(&n)));
        assert!(ws.space.wired(id(&n), id(&t)));
    });
}

/// A file edited by hand, or one whose block didn't come back: a wire to a missing block, to the
/// block itself and a second one between the same two are dropped. "aaaaa" names two items: the
/// first keeps it (the second gets an id of its own), so its wires are the first one's.
#[gpui_kit::test]
fn a_saved_wire_to_a_missing_or_doubled_block_is_dropped(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let kanban = |x: f32, uid: &str| {
        format!(r#"{{"kind":"kanban","uid":"{uid}","rect":{{"x":{x},"y":0,"w":300,"h":200}}}}"#)
    };
    let wire = |a: &str, b: &str| format!(r#"{{"a":"{a}","b":"{b}"}}"#);
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[{},{},{}],"wires":[{},{},{},{}]}}"#,
            kanban(0.0, "aaaaa"),
            kanban(400.0, "aaaaa"),
            kanban(800.0, "ccccc"),
            wire("aaaaa", "ccccc"),
            wire("aaaaa", "zzzzz"),
            wire("aaaaa", "aaaaa"),
            wire("ccccc", "aaaaa"),
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.read_with(cx, |ws, _| {
        let blocks: Vec<BlockId> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
        assert_eq!(ws.space.wires, [Wire::new(blocks[0], blocks[2]).unwrap()]);
    });
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum wires`
Expected: FAIL to compile (`SavedWire`, `wires` on `Saved` not found).

- [ ] **Step 3: Implement**

`session.rs`, after `SavedTile`'s `impl`:

```rust
/// A wire between two blocks, by their block ids (docs/specs/connections.md, Files).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedWire {
    pub a: String,
    pub b: String,
}
```

`Saved`, after `grid`:

```rust
    /// The wires between the items, by their `uid`s; an older Templum ignores them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wires: Vec<SavedWire>,
```

`Layout`, after `grid`: `pub wires: Vec<SavedWire>,`; in `snapshot`, after `grid: self.grid,`:
`wires: self.wires,`. The literals at `session.rs:946` (`sample`), `session.rs:1521` (a `Layout`),
`tests/persistence.rs:319` and `tests/workspaces.rs:32` gain `wires: Vec::new(),` after `grid: None,`.

`workspace/mod.rs`, `Space::layout`, after `grid: …`:

```rust
            wires: self
                .wires
                .iter()
                .filter_map(|w| {
                    Some(session::SavedWire {
                        a: self.uids.get(&w.a)?.clone(),
                        b: self.uids.get(&w.b)?.clone(),
                    })
                })
                .collect(),
```

`persist.rs`, `restore`, after the `adopt_uid` loop (:69-73):

```rust
        // The wires, by block id, once the blocks have theirs back: one whose block didn't come
        // back, of a block to itself, or a second between two, is dropped.
        for wire in &saved.wires {
            if let (Some(a), Some(b)) = (
                self.space.block_by_uid(&wire.a),
                self.space.block_by_uid(&wire.b),
            ) {
                self.space.add_wire(a, b);
            }
        }
```

(A stopped workspace keeps its file in `Space::saved` and starts through `restore`, so its wires come back too.)

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum wires && cargo test -p templum session && cargo test -p templum persistence && cargo
test -p templum workspaces`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/templum
git commit -m "feat(connections): keep wires through a restart" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 7: The tools check wires

**Files:**
- Modify: `crates/templum/src/workspace/bus.rs` (`block_lines` :315, `instructions` :340-363, `target` :399-413)
- Test: `crates/templum/src/workspace/tests/bus.rs`

**Interfaces:**
- Consumes: `Space::wired` (Task 2), `Workspace::connect`, `Workspace::undo_canvas`.
- Produces: `read_terminal` and `send_to_terminal` accept the caller's own block or one wired to it; `list_blocks`
  says `you`, `connected` or `not connected`; `initialize`'s instructions list "Connected to you" and "In this
  workspace, not connected".

- [ ] **Step 1: Write the failing test**

`workspace/tests/bus.rs`:

```rust
/// The bus checks the wires at every call: wired, a terminal is read and typed into; unwired, the
/// next call is refused. What initialize tells the agent is the wires of that moment.
#[gpui_kit::test]
fn a_wired_terminal_is_read_and_typed_into_until_it_is_unwired(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, mine) = three_blocks(handle, &ws, cx);
    let other = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(Rect::new(600.0, 400.0, 400.0, 300.0), window, cx);
                ws.space.canvas.top().unwrap()
            })
        })
        .unwrap();
    let (terminal, _) = me(&ws, cx, mine);
    let (_, theirs) = me(&ws, cx, other);
    let (k, n) = ws.read_with(cx, |ws, _| {
        (ws.space.uids[&kanban].clone(), ws.space.uids[&note].clone())
    });
    let typed = json!({ "block": theirs, "text": "hi" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "send_to_terminal", typed.clone());
    assert!(failed && text.contains("isn't connected to you"), "{text}");
    ws.update(cx, |ws, cx| assert!(ws.connect(mine, other, cx)));
    let (failed, text) = call(handle, &ws, cx, &terminal, "send_to_terminal", typed.clone());
    assert!(!failed, "{text}");
    let read = json!({ "block": theirs, "lines": 5 });
    let (failed, text) = call(handle, &ws, cx, &terminal, "read_terminal", read);
    assert!(!failed, "{text}");
    let (_, list) = call(handle, &ws, cx, &terminal, "list_blocks", json!({}));
    let line = list.lines().find(|l| l.starts_with(&theirs)).unwrap_or_default();
    assert!(line.ends_with("connected") && !line.contains("not connected"), "{list}");
    let instructions = |cx: &mut TestAppContext| {
        let init = ask(handle, &ws, cx, &terminal, "initialize", json!({}));
        init["result"]["instructions"].as_str().unwrap().to_string()
    };
    let told = instructions(cx);
    assert!(
        told.contains(&format!("Connected to you: {theirs} (terminal \"")),
        "{told}"
    );
    assert!(
        told.contains(&format!("not connected: {k} (kanban \"")) && told.contains(&format!("{n} (note \"")),
        "{told}"
    );
    // Unwired by Ctrl+Z: refused at the next call.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))
    })
    .unwrap();
    let (failed, text) = call(handle, &ws, cx, &terminal, "send_to_terminal", typed);
    assert!(failed && text.contains("isn't connected to you"), "{text}");
    assert!(instructions(cx).contains("Connected to you: none."));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum a_wired_terminal`
Expected: FAIL: the wired `send_to_terminal` is refused ("isn't connected to you").

- [ ] **Step 3: Implement**

`bus.rs`, `target` (:399-413):

```rust
/// The block `uid` names in `space`, when the caller `me` may use it: its own, or one wired to
/// it, looked at on each call.
fn target(space: &Space, me: BlockId, uid: &str) -> Result<BlockId, Reply> {
    let Some(id) = space.block_by_uid(uid) else {
        return Err(Reply::err(format!(
            "There's no block {uid} in your workspace: list_blocks lists them."
        )));
    };
    // ponytail: no `connect` tool yet (batch 3): the refusal sends the agent to the person.
    if id != me && !space.wired(me, id) {
        return Err(Reply::err(format!(
            "{uid} isn't connected to you. Ask the person to wire it to your block on the canvas."
        )));
    }
    Ok(id)
}
```

`block_lines` (:315):

```rust
                let tie = if b.id == me {
                    "you"
                } else if space.wired(me, b.id) {
                    "connected"
                } else {
                    "not connected"
                };
```

`instructions` (:340-363), the other blocks split by their wire to the caller:

```rust
        let (connected, others): (Vec<(bool, String)>, Vec<(bool, String)>) = space
            .canvas
            .blocks()
            .iter()
            .filter(|b| b.id != me)
            .map(|b| {
                let (kind, title, _) = self.about(&space.blocks[&b.id], cx);
                let uid = space.uids.get(&b.id).map_or("", String::as_str);
                (space.wired(me, b.id), format!("{uid} ({kind} {title:?})"))
            })
            .partition(|(wired, _)| *wired);
        let list = |blocks: Vec<(bool, String)>| {
            if blocks.is_empty() {
                "none".to_string()
            } else {
                blocks.into_iter().map(|(_, b)| b).collect::<Vec<_>>().join(", ")
            }
        };
        format!(
            "You are block {uid}, {title:?}, in Templum workspace {:?}. Connected to you: {}. In this \
             workspace, not connected: {}. Name blocks by these ids. You may only read or write your \
             own block and those connected to you; the person connects others by wiring them to your \
             block on the canvas. list_blocks gives the current list.",
            space.name,
            list(connected),
            list(others),
        )
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p templum bus`
Expected: PASS (the new test and batch 1's: `another_block_is_refused_until_it_is_connected` still finds the
kanban block refused and listed "not connected").

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/templum
git commit -m "feat(connections): let an agent read and type into the terminals wired to it" -m "list_blocks says which blocks are connected, and the MCP server's instructions list them apart." -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

---

### Task 8: Docs, minors, the full check and the benches

**Files:**
- Modify: `docs/specs/connections.md`, `docs/specs/persistence.md`, `docs/specs/cockpit.md`, `docs/ARCHITECTURE.md`,
  `README.md`, `docs/minors.md`

- [ ] **Step 1: The spec says what was built**

In `docs/specs/connections.md`:
- **Status:** "Batches 1 and 2 implemented; batches 3 to 5 planned".
- **What a person sees ▸ Wires:**
  - the Wire tool bullet ends: "A press on the empty canvas does nothing. While the pointer is over another block
    the curve already runs to its edge, in the accent color. Wiring leaves no block with the keyboard, so Ctrl+Z
    takes the wire back."
  - the block menu bullet ends: "They follow Fullscreen. A tile's menu has neither: grid mode hides wires."
  - the wire bullet: "… in the title's dim color, 2 units wide and never under a pixel. …"
  - the selecting bullet: "a click on a wire where no block or drawing covers it selects it (accent color); Delete or
    Backspace removes it while no block has the keyboard (focusing or adding one unselects it); …"
  - the far-away bullet: "**Far away** a wire is 1 pixel wide: from 50% down, where the Card level of
    [level-of-detail.md](level-of-detail.md) comes."
  - the moving bullet: "… drops its wires, those between blocks moved together too."
- **The MCP server**, after the instructions' example: "Until `connect` (batch 3) they say "In this workspace, not
  connected:" without "(ask with connect)", and end "the person connects others by wiring them to your block on the
  canvas"; the kanban and note sentence comes with their tools (batch 4)." The Results bullet's refusal gains:
  "(until batch 3: `k9x1d isn't connected to you. Ask the person to wire it to your block on the canvas.`)".
- **Files**, the workspace files bullet: "A wire whose block didn't come back, one of a block to itself, and a second
  one between the same two blocks are dropped."

- [ ] **Step 2: Architecture and the other specs**

- `docs/ARCHITECTURE.md`: the `templum-canvas` row: "…, level of detail, wires' curves (`wire.rs`), layout
  (de)serialization"; Source layout, in `workspace/`'s list after `drawing.rs (the drawing tools)`: "`wires.rs` (wires
  between blocks: wiring and unwiring, their undo steps, the Wire tool, selecting a wire, the curves to paint)", and a
  top-level bullet "`wires.rs`: painting the wires in the layer under the blocks, with the dot grid"; the persistence
  sentence "Each item carries its block's `uid` …" gains "; the canvas's `wires` name their blocks by it".
- `docs/specs/persistence.md`, The file, after the `grid` bullet: "- **`wires`:** the wires between blocks
  ([connections.md](connections.md#wires)), each `{"a": "<uid>", "b": "<uid>"}`; missing when there are none. A wire
  whose block didn't come back is dropped; an older Templum ignores them."
- `docs/specs/cockpit.md` §3: the Pointer row lists "**Select** (`mouse-pointer-2`), **Hand** (`hand`), **Wire**
  (`cable`)" and adds "Wire wires two blocks ([connections.md](connections.md#wires)); it stays on until another tool
  is picked or Esc is pressed."; the Architecture line's `Tool` list gains `Wire`.
- `README.md`, "Agents that talk to the canvas": after the first sentence, "Wire blocks together with the Wire tool
  (the cable in the toolbar) or a block's **Connect to ▸**: an agent reads and types into the terminals wired to it."

- [ ] **Step 3: Minors**

Under `## Connections` in `docs/minors.md`, a paragraph "Left by batch 2 (wires); line numbers are those of
`development` at X." (X: `git rev-parse --short HEAD` once Task 7 is committed) and:

- [ ] **Ctrl+Z after Connect to ▸ in a terminal's menu reaches the terminal.** The right-click focuses the block
  (`crates/templum/src/block.rs:626-637`), and the menu's wiring (`wiring`, `crates/templum/src/menu/build.rs`)
  leaves it focused, so the Ctrl+Z that should take the wire back goes to the terminal; the Wire tool leaves no block
  focused and doesn't have this. A person presses Ctrl+Z and sees `^Z` in the shell. Fix: Connect to ▸ and
  Disconnect from ▸ leave no block focused, as `begin_wire` does.

Add any other minor the implementation or a review defers, each with where, what a person sees and a fix.

- [ ] **Step 4: The full check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all green.

- [ ] **Step 5: The benches** (this batch paints)

Run, each on its own, from the Templum folder:
`cargo build --release -p templum`, then `cargo run --release -p templum -- --bench` and
`cargo run --release -p templum -- --bench-mixed`, then for drawing at far zoom
`cd ~/Documents/github/DaemonQuest && ~/Documents/github/Templum/target/release/templum --bench-far`.
Expected: each prints a JSON line with `p95_ms` ≤ 16.7; keep the three lines for the pull request. A line over the
bar: run it once more (other programs load the CPU); over again, stop and report it (the spec's Risks row: cache the
wires' paths per zoom step as `ink.rs` does) instead of committing.

- [ ] **Step 6: Commit**

```bash
git add docs README.md
git commit -m "docs(connections): describe wires, and what agents are told of them" -m "Claude-Session: https://claude.ai/code/session_01F4wHPkNNcdWTQ8aE8CmkxZ"
```

## Manual checks (Hyprland)

1. **The Wire tool:** pick the cable, press on a terminal and drag: an accent curve follows the pointer, and over
   another block it runs to that block's edge; let go on a note: a dim wire under both blocks. Let go on the empty
   canvas: nothing. The tool stays on; Esc puts Select back.
2. **Following:** move and resize a wired block: the wire follows at once; pan and zoom: it glides with the canvas;
   at 25% it is a 1 px line.
3. **Selecting:** click a wire between two blocks: it turns accent; Delete removes it; Ctrl+Z brings it back; Esc
   unselects; right-click on it: Disconnect.
4. **A block's menu:** Connect to ▸ lists the blocks not wired to it by title, Disconnect from ▸ the wired ones.
5. **Closing:** close a wired block: its wires go; Ctrl+Z: the block and its wires come back.
6. **Grid mode:** Fullscreen on two wired blocks: no wires; Back to the canvas: there they are.
7. **Restart:** quit and start Templum: the wires are where they were.
8. **The keyboard:** select a wire, click into a terminal and press Backspace: the terminal erases, the wire stays.
9. **An agent:** Claude Code from AI agent ▸, wired to a terminal running a dev server: "read the last 30 lines of
   <its id>" works; unwire it and ask again: refused, saying to wire it on the canvas. `templum blocks` in the agent's
   terminal shows the dev server "connected" while wired.
