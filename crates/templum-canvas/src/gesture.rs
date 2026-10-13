//! Pointer drags on the canvas. Pointer positions are screen units.

use crate::snap::{Guide, SNAP_PX, snap_move, snap_resize};
use crate::{BlockId, Canvas, Point, Rect, Size};

/// What a pointer move did: the new rect of a resized block (to resize its PTY), and the guides
/// of the snaps in effect.
#[derive(Debug, Default, PartialEq)]
pub struct Update {
    pub resized: Option<(BlockId, Rect)>,
    pub guides: Vec<Guide>,
}

/// The edges a resize drags; the others stay put. A corner drags two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl Edges {
    pub const LEFT: Edges = Edges::new(true, false, false, false);
    pub const TOP: Edges = Edges::new(false, true, false, false);
    pub const RIGHT: Edges = Edges::new(false, false, true, false);
    pub const BOTTOM: Edges = Edges::new(false, false, false, true);
    pub const TOP_LEFT: Edges = Edges::new(true, true, false, false);
    pub const TOP_RIGHT: Edges = Edges::new(false, true, true, false);
    pub const BOTTOM_LEFT: Edges = Edges::new(true, false, false, true);
    pub const BOTTOM_RIGHT: Edges = Edges::new(false, false, true, true);

    const fn new(left: bool, top: bool, right: bool, bottom: bool) -> Edges {
        Edges {
            left,
            top,
            right,
            bottom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// Dragging empty canvas; `last` is the previous pointer position.
    Pan { last: Point },
    /// Dragging a title bar; `grab` is the pointer's world offset from the block's top-left.
    Move { id: BlockId, grab: Point },
    /// Dragging `edges` of a block; `grab` is the pointer's world offset from them.
    Resize {
        id: BlockId,
        edges: Edges,
        grab: Point,
        min: Size,
    },
}

impl Gesture {
    pub fn pan(pointer: Point) -> Self {
        Gesture::Pan { last: pointer }
    }

    pub fn move_block(canvas: &Canvas, id: BlockId, pointer: Point) -> Option<Self> {
        let r = canvas.get(id)?.rect;
        let w = canvas.camera.to_world(pointer);
        Some(Gesture::Move {
            id,
            grab: Point::new(w.x - r.x, w.y - r.y),
        })
    }

    pub fn resize_block(
        canvas: &Canvas,
        id: BlockId,
        edges: Edges,
        pointer: Point,
        min: Size,
    ) -> Option<Self> {
        let r = canvas.get(id)?.rect;
        let w = canvas.camera.to_world(pointer);
        let x = if edges.left { r.x } else { r.right() };
        let y = if edges.top { r.y } else { r.bottom() };
        Some(Gesture::Resize {
            id,
            edges,
            grab: Point::new(x - w.x, y - w.y),
            min,
        })
    }

    /// Applies a pointer move. A moved or resized block snaps to the others' edges and centers
    /// within [`SNAP_PX`] screen pixels; the rect is recomputed from the pointer every time, so
    /// a snap lets go as soon as the pointer is farther. A gesture whose block is gone does nothing.
    pub fn update(&mut self, canvas: &mut Canvas, pointer: Point) -> Update {
        match self {
            Gesture::Pan { last } => {
                canvas
                    .camera
                    .pan_by(Point::new(pointer.x - last.x, pointer.y - last.y));
                *last = pointer;
                Update::default()
            }
            Gesture::Move { id, grab } => {
                let w = canvas.camera.to_world(pointer);
                let others = others(canvas, *id);
                let threshold = SNAP_PX / canvas.camera.zoom();
                let Some(b) = canvas.get_mut(*id) else {
                    return Update::default();
                };
                let raw = Rect::new(w.x - grab.x, w.y - grab.y, b.rect.w, b.rect.h);
                let snapped = snap_move(raw, &others, threshold);
                b.rect = snapped.rect;
                Update {
                    resized: None,
                    guides: snapped.guides,
                }
            }
            Gesture::Resize {
                id,
                edges,
                grab,
                min,
            } => {
                let w = canvas.camera.to_world(pointer);
                let others = others(canvas, *id);
                let threshold = SNAP_PX / canvas.camera.zoom();
                let Some(b) = canvas.get_mut(*id) else {
                    return Update::default();
                };
                let (x, y) = (w.x + grab.x, w.y + grab.y);
                let raw = clamp(b.rect, *edges, x, y, *min);
                let snapped = snap_resize(raw, *edges, &others, threshold);
                let r = snapped.rect;
                b.rect = clamp(
                    raw,
                    *edges,
                    if edges.left { r.x } else { r.right() },
                    if edges.top { r.y } else { r.bottom() },
                    *min,
                );
                Update {
                    resized: Some((*id, b.rect)),
                    guides: snapped.guides,
                }
            }
        }
    }
}

/// `r` with its dragged `edges` moved to `x` (left or right) and `y` (top or bottom), never
/// smaller than `min`: a dragged edge stops where the block is at its smallest.
fn clamp(r: Rect, edges: Edges, x: f32, y: f32, min: Size) -> Rect {
    let (mut left, mut top, mut right, mut bottom) = (r.x, r.y, r.right(), r.bottom());
    if edges.left {
        left = x.min(right - min.w);
    } else if edges.right {
        right = x.max(left + min.w);
    }
    if edges.top {
        top = y.min(bottom - min.h);
    } else if edges.bottom {
        bottom = y.max(top + min.h);
    }
    Rect::new(left, top, right - left, bottom - top)
}

/// Every block's rect but `id`'s: what it can snap to.
fn others(canvas: &Canvas, id: BlockId) -> Vec<Rect> {
    canvas
        .blocks()
        .iter()
        .filter(|b| b.id != id)
        .map(|b| b.rect)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pan_moves_the_camera_with_the_pointer() {
        let mut c = Canvas::default();
        let mut g = Gesture::pan(Point::new(100.0, 100.0));
        assert_eq!(g.update(&mut c, Point::new(130.0, 90.0)).resized, None);
        assert_eq!(c.camera.origin, Point::new(-30.0, 10.0));
    }

    #[test]
    fn move_keeps_the_grab_offset_at_any_zoom() {
        let mut c = Canvas::default();
        let id = c.add(Rect::new(100.0, 100.0, 200.0, 100.0));
        c.camera.zoom_step(5, Point::new(0.0, 0.0)); // zoom 2
        let grab = c.camera.to_screen(Point::new(110.0, 105.0));
        let mut g = Gesture::move_block(&c, id, grab).unwrap();
        g.update(&mut c, Point::new(grab.x + 20.0, grab.y + 40.0));
        assert_eq!(
            c.get(id).unwrap().rect,
            Rect::new(110.0, 120.0, 200.0, 100.0)
        );
    }

    #[test]
    fn resize_reports_the_new_rect_and_clamps_to_minimum() {
        let mut c = Canvas::default();
        let id = c.add(Rect::new(0.0, 0.0, 200.0, 100.0));
        let min = Size::new(150.0, 80.0);
        let mut g =
            Gesture::resize_block(&c, id, Edges::BOTTOM_RIGHT, Point::new(195.0, 95.0), min)
                .unwrap();
        assert_eq!(
            g.update(&mut c, Point::new(295.0, 145.0)).resized,
            Some((id, Rect::new(0.0, 0.0, 300.0, 150.0)))
        );
        assert_eq!(
            g.update(&mut c, Point::new(10.0, 10.0)).resized,
            Some((id, Rect::new(0.0, 0.0, 150.0, 80.0)))
        );
    }

    #[test]
    fn gesture_on_a_removed_block_is_inert() {
        let mut c = Canvas::default();
        let id = c.add(Rect::new(0.0, 0.0, 200.0, 100.0));
        let mut mv = Gesture::move_block(&c, id, Point::new(5.0, 5.0)).unwrap();
        let mut rs = Gesture::resize_block(
            &c,
            id,
            Edges::BOTTOM_RIGHT,
            Point::new(195.0, 95.0),
            Size::new(1.0, 1.0),
        )
        .unwrap();
        c.remove(id);
        assert_eq!(mv.update(&mut c, Point::new(50.0, 50.0)).resized, None);
        assert_eq!(rs.update(&mut c, Point::new(50.0, 50.0)).resized, None);
        assert_eq!(Gesture::move_block(&c, id, Point::new(0.0, 0.0)), None);
    }
    #[test]
    fn a_moved_block_snaps_to_a_neighbour_and_reports_guides() {
        let mut c = Canvas::default();
        c.add(Rect::new(100.0, 100.0, 200.0, 100.0));
        let id = c.add(Rect::new(500.0, 500.0, 50.0, 50.0));
        let mut g = Gesture::move_block(&c, id, Point::new(500.0, 500.0)).unwrap();
        // Dragged so its left edge would be at 105: within 8 px of 100.
        let u = g.update(&mut c, Point::new(105.0, 500.0));
        assert_eq!(u.resized, None);
        assert_eq!(c.get(id).unwrap().rect.x, 100.0);
        assert_eq!(u.guides.len(), 1);
        // Farther: it follows the pointer again, and the guide is gone.
        let u = g.update(&mut c, Point::new(120.0, 500.0));
        assert_eq!(c.get(id).unwrap().rect.x, 120.0);
        assert!(u.guides.is_empty());
    }

    #[test]
    fn the_threshold_follows_the_zoom() {
        let mut c = Canvas::default();
        c.add(Rect::new(100.0, 100.0, 200.0, 100.0));
        let id = c.add(Rect::new(500.0, 500.0, 50.0, 50.0));
        c.camera.zoom_step(5, Point::new(0.0, 0.0)); // zoom 2: 8 px is 4 world units
        let grab = c.camera.to_screen(Point::new(500.0, 500.0));
        let mut g = Gesture::move_block(&c, id, grab).unwrap();
        // Left edge at 105 world units: 5 away, more than 4.
        let to = c.camera.to_screen(Point::new(105.0, 500.0));
        g.update(&mut c, to);
        assert_eq!(c.get(id).unwrap().rect.x, 105.0);
        // At 103: 3 away, snaps.
        let to = c.camera.to_screen(Point::new(103.0, 500.0));
        g.update(&mut c, to);
        assert_eq!(c.get(id).unwrap().rect.x, 100.0);
    }

    #[test]
    fn a_resized_block_snaps_its_width_and_reports_the_rect() {
        let mut c = Canvas::default();
        c.add(Rect::new(100.0, 100.0, 200.0, 100.0));
        let id = c.add(Rect::new(500.0, 500.0, 50.0, 50.0));
        let min = Size::new(10.0, 10.0);
        let mut g =
            Gesture::resize_block(&c, id, Edges::BOTTOM_RIGHT, Point::new(550.0, 550.0), min)
                .unwrap();
        // Width would be 203: snaps to 200.
        let u = g.update(&mut c, Point::new(703.0, 550.0));
        assert_eq!(u.resized, Some((id, Rect::new(500.0, 500.0, 200.0, 50.0))));
        assert_eq!(u.guides.len(), 1);
    }

    #[test]
    fn every_edge_and_corner_resizes_keeping_the_opposite_ones() {
        let rect = Rect::new(100.0, 100.0, 200.0, 100.0);
        let min = Size::new(50.0, 40.0);
        let drag = |edges: Edges, from: Point, to: Point| {
            let mut c = Canvas::default();
            let id = c.add(rect);
            let mut g = Gesture::resize_block(&c, id, edges, from, min).unwrap();
            g.update(&mut c, to).resized.unwrap().1
        };
        // Left edge, grabbed 2 units inside: 30 to the left.
        assert_eq!(
            drag(
                Edges::LEFT,
                Point::new(102.0, 150.0),
                Point::new(72.0, 170.0)
            ),
            Rect::new(70.0, 100.0, 230.0, 100.0)
        );
        // Top edge, down 20.
        assert_eq!(
            drag(
                Edges::TOP,
                Point::new(150.0, 100.0),
                Point::new(150.0, 120.0)
            ),
            Rect::new(100.0, 120.0, 200.0, 80.0)
        );
        // Right edge only: the height stays.
        assert_eq!(
            drag(
                Edges::RIGHT,
                Point::new(300.0, 150.0),
                Point::new(340.0, 190.0)
            ),
            Rect::new(100.0, 100.0, 240.0, 100.0)
        );
        // Bottom-left corner.
        assert_eq!(
            drag(
                Edges::BOTTOM_LEFT,
                Point::new(100.0, 200.0),
                Point::new(90.0, 230.0)
            ),
            Rect::new(90.0, 100.0, 210.0, 130.0)
        );
        // Top-left corner dragged past the opposite corner: stops at the minimum, the
        // bottom-right corner stays.
        assert_eq!(
            drag(
                Edges::TOP_LEFT,
                Point::new(100.0, 100.0),
                Point::new(900.0, 900.0)
            ),
            Rect::new(250.0, 160.0, 50.0, 40.0)
        );
    }

    #[test]
    fn a_dragged_left_edge_snaps_to_a_neighbour() {
        let mut c = Canvas::default();
        c.add(Rect::new(100.0, 0.0, 200.0, 100.0));
        let id = c.add(Rect::new(110.0, 300.0, 200.0, 100.0));
        let min = Size::new(10.0, 10.0);
        let mut g =
            Gesture::resize_block(&c, id, Edges::LEFT, Point::new(110.0, 350.0), min).unwrap();
        // Left edge would be at 104: within 8 of 100. The right edge stays at 310.
        let u = g.update(&mut c, Point::new(104.0, 350.0));
        assert_eq!(u.resized, Some((id, Rect::new(100.0, 300.0, 210.0, 100.0))));
        assert_eq!(u.guides.len(), 1);
    }
}
