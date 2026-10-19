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
        let across = (b.x - a.right()).max(a.x - b.right());
        let down = (b.y - a.bottom()).max(a.y - b.bottom());
        if across >= down {
            sides(a, b)
        } else {
            ends(a, b)
        }
    }

    /// The curve from `a` to `b` over none of `others` (the blocks around) when one is: that of
    /// [`Curve::between`], else between the other pair of edges, else an elbow from a side of one
    /// to the top or bottom of the other; with none, the one over the fewest. Only the rects
    /// between the two can be in the way, and the first clear curve wins: a wire with nothing in
    /// its way costs one look.
    pub fn around(a: Rect, b: Rect, others: impl Iterator<Item = Rect>) -> Curve {
        let first = Curve::between(a, b);
        let span = a.union(&b);
        let near: Vec<Rect> = others.filter(|r| r.intersects(&span)).collect();
        let over = |c: &Curve| {
            let points: Vec<Point> = (1..SEGMENTS)
                .map(|i| c.at(i as f32 / SEGMENTS as f32))
                .collect();
            (near.iter())
                .filter(|r| points.iter().any(|p| r.contains(*p)))
                .count()
        };
        let mut best = (over(&first), first);
        let across = (b.x - a.right()).max(a.x - b.right());
        let down = (b.y - a.bottom()).max(a.y - b.bottom());
        let others = [
            (across > 0.0).then(|| sides(a, b)),
            (down > 0.0).then(|| ends(a, b)),
            elbow(a, b),
            elbow(b, a).map(|c| Curve {
                from: c.to,
                c1: c.c2,
                c2: c.c1,
                to: c.from,
            }),
        ];
        for curve in others.into_iter().flatten() {
            if best.0 == 0 {
                break;
            }
            let n = over(&curve);
            if n < best.0 {
                best = (n, curve);
            }
        }
        best.1
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

/// Between the sides of `a` and `b` that face each other across, leaving square to them.
fn sides(a: Rect, b: Rect) -> Curve {
    let (ca, cb) = (center(a), center(b));
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
}

/// Between the top and bottom of `a` and `b` that face each other, leaving square to them.
fn ends(a: Rect, b: Rect) -> Curve {
    let (ca, cb) = (center(a), center(b));
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

/// An elbow from the side of `a` facing `b` to the top or bottom of `b` facing `a`, rounded at its
/// corner; None when `b`'s middle isn't past that side, or `a`'s past that top or bottom.
fn elbow(a: Rect, b: Rect) -> Option<Curve> {
    let (ca, cb) = (center(a), center(b));
    let x = if cb.x > a.right() {
        a.right()
    } else if cb.x < a.x {
        a.x
    } else {
        return None;
    };
    let y = if ca.y < b.y {
        b.y
    } else if ca.y > b.bottom() {
        b.bottom()
    } else {
        return None;
    };
    let corner = Point::new(cb.x, ca.y);
    Some(Curve {
        from: Point::new(x, ca.y),
        c1: corner,
        c2: corner,
        to: Point::new(cb.x, y),
    })
}

fn center(r: Rect) -> Point {
    Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

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
        assert_eq!(
            (c.from, c.to),
            (Point::new(50.0, 50.0), Point::new(70.0, 300.0))
        );
        // Left of it, and above it.
        let c = Curve::between(a, Rect::new(-400.0, 10.0, 100.0, 50.0));
        assert_eq!(
            (c.from, c.to),
            (Point::new(0.0, 25.0), Point::new(-300.0, 35.0))
        );
        let c = Curve::between(a, Rect::new(0.0, -300.0, 100.0, 50.0));
        assert_eq!(
            (c.from, c.to),
            (Point::new(50.0, 0.0), Point::new(50.0, -250.0))
        );
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
        assert!(
            straight.hit(Point::new(200.0, 55.0), 6.0),
            "within the reach"
        );
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

    /// The wire from a block to the third of a column beside it doesn't run under the second:
    /// it takes the gap between them; with nothing in the way, it is the usual one.
    #[test]
    fn a_wire_goes_around_a_block_in_its_way() {
        let parent = Rect::new(0.0, 0.0, 600.0, 400.0);
        let second = Rect::new(648.0, 348.0, 500.0, 300.0);
        let third = Rect::new(648.0, 696.0, 500.0, 300.0);
        assert_eq!(
            Curve::around(parent, third, std::iter::empty()),
            Curve::between(parent, third)
        );
        assert!(
            (1..SEGMENTS)
                .any(|i| second.contains(Curve::between(parent, third).at(i as f32 / 32.0))),
            "the usual one runs under it"
        );
        let c = Curve::around(parent, third, [second].into_iter());
        assert_eq!(
            (c.from, c.to),
            (Point::new(600.0, 200.0), Point::new(648.0, 846.0))
        );
        assert!((0..=SEGMENTS).all(|i| !second.contains(c.at(i as f32 / 32.0))));
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
