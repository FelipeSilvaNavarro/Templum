//! Wires between blocks (docs/specs/connections.md, Wires): wiring and unwiring them, each a
//! step of Ctrl+Z.

use std::time::Instant;

use gpui_kit::{Context, Hsla, MouseDownEvent, MouseUpEvent};
use templum_canvas::{BlockId, Curve, HIT_PX, Point, Rect, Wire};

use super::history::{Change, Then};
use super::{Drag, Space, Workspace};

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

    /// The top block drawn at a world point.
    pub(super) fn block_at(&self, p: Point) -> Option<BlockId> {
        self.canvas
            .blocks()
            .iter()
            .rev()
            .find(|b| self.shown(b.id) && b.rect.contains(p))
            .map(|b| b.id)
    }

    /// `wire`'s curve between its blocks' rects; None when one isn't on the canvas.
    pub(super) fn curve(&self, wire: Wire) -> Option<Curve> {
        Some(Curve::between(
            self.canvas.get(wire.a)?.rect,
            self.canvas.get(wire.b)?.rect,
        ))
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
            .filter_map(|w| {
                let color = if self.space.selected_wire == Some(*w) {
                    self.ui.accent
                } else {
                    self.ui.title_dim
                };
                Some((self.space.curve(*w)?, color))
            })
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

    /// The dots on the wires along which a task is handed over (docs/specs/connections.md, Wires):
    /// each on its way from the agent that handed the task over to the one working on it, a trip
    /// every DOT_TRIP; at the middle when `still` (reduced motion). None in grid mode, which hides
    /// wires, nor without a wire between the two.
    pub(super) fn wire_dots(&self, now: Instant, still: bool) -> Vec<Point> {
        if self.space.grid.is_some() {
            return Vec::new();
        }
        self.space
            .terminals()
            .filter_map(|(to, pane)| {
                let handed = pane.task(now)?;
                let (from, _) = self
                    .space
                    .terminals()
                    .find(|(_, p)| p.term.id() == handed.by)?;
                let wire = *self
                    .space
                    .wires
                    .iter()
                    .find(|w| **w == Wire { a: from, b: to })?;
                let phase = if still {
                    0.5
                } else {
                    let gone = now.saturating_duration_since(handed.since).as_secs_f32();
                    (gone / crate::wires::DOT_TRIP.as_secs_f32()) % 1.0
                };
                // The curve runs from the wire's `a` to its `b`.
                let t = if wire.a == from { phase } else { 1.0 - phase };
                Some(self.space.curve(wire)?.at(t))
            })
            .collect()
    }

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

    /// The Wire tool let go over the canvas: on another block, the two are wired. Let go
    /// anywhere else, `release` drops the drag.
    pub(super) fn end_wire(&mut self, e: &MouseUpEvent, cx: &mut Context<Self>) {
        if let Some(Drag::Wire { from, .. }) = self.drag.take() {
            let at = self.space.canvas.camera.to_world(self.local(e.position));
            if let Some(to) = self.space.block_at(at) {
                self.connect(from, to, cx);
            }
            cx.notify();
        }
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
