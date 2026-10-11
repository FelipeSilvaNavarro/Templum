//! Drawings on the canvas: pen and highlighter strokes, and shapes. World units. No UI.

use crate::{Point, Rect};

/// One of the theme's colors, by name, so drawings follow the theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Foreground,
    Red,
    Yellow,
    Green,
    Cyan,
    Blue,
    Magenta,
}

impl Color {
    pub const ALL: [Color; 7] = [
        Color::Foreground,
        Color::Red,
        Color::Yellow,
        Color::Green,
        Color::Cyan,
        Color::Blue,
        Color::Magenta,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Color::Foreground => "foreground",
            Color::Red => "red",
            Color::Yellow => "yellow",
            Color::Green => "green",
            Color::Cyan => "cyan",
            Color::Blue => "blue",
            Color::Magenta => "magenta",
        }
    }

    pub fn from_name(name: &str) -> Option<Color> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Line,
    Arrow,
    Rect,
    Ellipse,
}

impl ShapeKind {
    pub const ALL: [ShapeKind; 4] = [
        ShapeKind::Line,
        ShapeKind::Arrow,
        ShapeKind::Rect,
        ShapeKind::Ellipse,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ShapeKind::Line => "line",
            ShapeKind::Arrow => "arrow",
            ShapeKind::Rect => "rect",
            ShapeKind::Ellipse => "ellipse",
        }
    }

    pub fn from_name(name: &str) -> Option<ShapeKind> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// A stroke (pen or highlighter) or a shape, in world units.
#[derive(Clone, Debug, PartialEq)]
pub enum Drawing {
    Stroke {
        points: Vec<Point>,
        width: f32,
        color: Color,
        highlight: bool,
    },
    Shape {
        kind: ShapeKind,
        from: Point,
        to: Point,
        width: f32,
        color: Color,
    },
}

/// A pen point closer than this to the previous one isn't kept.
pub const MIN_POINT_DISTANCE: f32 = 1.5;
/// How far (screen pixels) the pen trails the pointer while drawing, like a pulled string, so a
/// shaky hand still draws a smooth line. The caller divides by the zoom.
pub const STRING_PX: f32 = 5.0;
/// How far (screen pixels) from the ink a click still hits it. The caller divides by the zoom.
pub const HIT_PX: f32 = 6.0;
pub const MIN_WIDTH: f32 = 1.0;
pub const MAX_WIDTH: f32 = 64.0;
/// A shape smaller than this on both sides is dropped when the drag ends.
pub const MIN_SHAPE: f32 = 2.0;

impl Drawing {
    /// Pulls a stroke's pen toward the pointer at `p`, stopping `string` short of it (0 puts it
    /// on `p`): a wobble shorter than the string never reaches the ink. The new point is kept
    /// only [`MIN_POINT_DISTANCE`] or more from the last one. Shapes take no points.
    pub fn push_point(&mut self, p: Point, string: f32) -> bool {
        let Drawing::Stroke { points, .. } = self else {
            return false;
        };
        let Some(&last) = points.last() else {
            points.push(p);
            return true;
        };
        let d = distance(last, p);
        if d - string < MIN_POINT_DISTANCE {
            return false;
        }
        let k = (d - string) / d;
        points.push(Point::new(
            last.x + (p.x - last.x) * k,
            last.y + (p.y - last.y) * k,
        ));
        true
    }

    /// Too small to keep: a stroke under 2 points, a shape under [`MIN_SHAPE`] both ways.
    pub fn is_empty(&self) -> bool {
        match self {
            Drawing::Stroke { points, .. } => points.len() < 2,
            Drawing::Shape { from, to, .. } => {
                (from.x - to.x).abs() < MIN_SHAPE && (from.y - to.y).abs() < MIN_SHAPE
            }
        }
    }

    pub fn width(&self) -> f32 {
        match self {
            Drawing::Stroke { width, .. } | Drawing::Shape { width, .. } => *width,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Drawing::Stroke { color, .. } | Drawing::Shape { color, .. } => *color,
        }
    }

    /// The box around the ink, half the width out from its points.
    pub fn bounds(&self) -> Rect {
        let (min, max) = match self {
            Drawing::Stroke { points, .. } => points.iter().fold(
                (
                    Point::new(f32::MAX, f32::MAX),
                    Point::new(f32::MIN, f32::MIN),
                ),
                |(lo, hi), p| {
                    (
                        Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                        Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                    )
                },
            ),
            Drawing::Shape { from, to, .. } => (
                Point::new(from.x.min(to.x), from.y.min(to.y)),
                Point::new(from.x.max(to.x), from.y.max(to.y)),
            ),
        };
        let pad = self.width() / 2.0;
        Rect::new(
            min.x - pad,
            min.y - pad,
            max.x - min.x + 2.0 * pad,
            max.y - min.y + 2.0 * pad,
        )
    }

    /// True within `tolerance` plus half the width of the ink itself (a rectangle's edges, an
    /// ellipse's rim), never of the empty inside of a shape.
    pub fn hit(&self, p: Point, tolerance: f32) -> bool {
        let t = tolerance + self.width() / 2.0;
        match self {
            Drawing::Stroke { points, .. } => match points.as_slice() {
                [] => false,
                [only] => distance(*only, p) <= t,
                _ => points
                    .windows(2)
                    .any(|w| segment_distance(p, w[0], w[1]) <= t),
            },
            Drawing::Shape {
                kind: ShapeKind::Line | ShapeKind::Arrow,
                from,
                to,
                ..
            } => segment_distance(p, *from, *to) <= t,
            Drawing::Shape {
                kind: ShapeKind::Rect,
                from,
                to,
                ..
            } => {
                let (a, c) = (*from, *to);
                let b = Point::new(c.x, a.y);
                let d = Point::new(a.x, c.y);
                [(a, b), (b, c), (c, d), (d, a)]
                    .iter()
                    .any(|(s, e)| segment_distance(p, *s, *e) <= t)
            }
            Drawing::Shape {
                kind: ShapeKind::Ellipse,
                from,
                to,
                ..
            } => {
                let (rx, ry) = ((from.x - to.x).abs() / 2.0, (from.y - to.y).abs() / 2.0);
                let center = Point::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
                if rx <= t || ry <= t {
                    // Flat enough to be a line.
                    return segment_distance(p, *from, *to) <= t;
                }
                let (dx, dy) = ((p.x - center.x) / rx, (p.y - center.y) / ry);
                let r = (dx * dx + dy * dy).sqrt();
                (r - 1.0).abs() * rx.min(ry) <= t
            }
        }
    }

    pub fn move_by(&mut self, dx: f32, dy: f32) {
        match self {
            Drawing::Stroke { points, .. } => {
                for p in points {
                    p.x += dx;
                    p.y += dy;
                }
            }
            Drawing::Shape { from, to, .. } => {
                from.x += dx;
                from.y += dy;
                to.x += dx;
                to.y += dy;
            }
        }
    }
}

/// Where a shape's far corner goes with Shift held: lines and arrows snap to multiples of 45°,
/// rectangles and ellipses become squares and circles.
pub fn constrain(from: Point, to: Point, kind: ShapeKind) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    match kind {
        ShapeKind::Line | ShapeKind::Arrow => {
            let len = (dx * dx + dy * dy).sqrt();
            if len == 0.0 {
                return to;
            }
            let step = std::f32::consts::FRAC_PI_4;
            let angle = (dy.atan2(dx) / step).round() * step;
            Point::new(from.x + len * angle.cos(), from.y + len * angle.sin())
        }
        ShapeKind::Rect | ShapeKind::Ellipse => {
            let side = dx.abs().max(dy.abs());
            Point::new(from.x + side * dx.signum(), from.y + side * dy.signum())
        }
    }
}

/// The points at least `min_distance` from the previous one kept, and the last one.
pub fn simplify(points: &[Point], min_distance: f32) -> Vec<Point> {
    let mut kept: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if kept
            .last()
            .is_none_or(|last| distance(*last, *p) >= min_distance)
        {
            kept.push(*p);
        }
    }
    if let Some(end) = points.last()
        && kept.last() != Some(end)
    {
        kept.push(*end);
    }
    kept
}

fn distance(a: Point, b: Point) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// Distance from `p` to the segment `a`–`b`.
fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let (abx, aby) = (b.x - a.x, b.y - a.y);
    let len2 = abx * abx + aby * aby;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * abx + (p.y - a.y) * aby) / len2).clamp(0.0, 1.0)
    };
    distance(p, Point::new(a.x + t * abx, a.y + t * aby))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(points: &[(f32, f32)], width: f32) -> Drawing {
        Drawing::Stroke {
            points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            width,
            color: Color::Red,
            highlight: false,
        }
    }

    fn shape(kind: ShapeKind, from: (f32, f32), to: (f32, f32)) -> Drawing {
        Drawing::Shape {
            kind,
            from: Point::new(from.0, from.1),
            to: Point::new(to.0, to.1),
            width: 2.0,
            color: Color::Blue,
        }
    }

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    #[test]
    fn colors_and_kinds_round_trip_through_their_names() {
        for c in Color::ALL {
            assert_eq!(Color::from_name(c.name()), Some(c));
        }
        assert_eq!(Color::from_name("plaid"), None);
        for k in ShapeKind::ALL {
            assert_eq!(ShapeKind::from_name(k.name()), Some(k));
        }
        assert_eq!(ShapeKind::from_name("stroke"), None);
    }

    #[test]
    fn points_closer_than_the_minimum_are_skipped() {
        let mut s = stroke(&[(0.0, 0.0)], 4.0);
        assert!(!s.push_point(Point::new(1.0, 0.0), 0.0), "1 away: skipped");
        assert!(s.push_point(Point::new(2.0, 0.0), 0.0));
        assert!(s.push_point(Point::new(2.0, 1.6), 0.0));
        let Drawing::Stroke { points, .. } = &s else {
            unreachable!()
        };
        assert_eq!(points.len(), 3);
        assert!(
            !shape(ShapeKind::Line, (0.0, 0.0), (1.0, 1.0))
                .clone()
                .push_point(Point::default(), 0.0)
        );
    }

    #[test]
    fn the_pen_trails_the_pointer_by_the_string() {
        let mut s = stroke(&[(0.0, 0.0)], 4.0);
        assert!(
            !s.push_point(Point::new(0.0, 6.0), 5.0),
            "within the string"
        );
        assert!(s.push_point(Point::new(10.0, 0.0), 5.0));
        let Drawing::Stroke { points, .. } = &s else {
            unreachable!()
        };
        assert_eq!(points.last(), Some(&Point::new(5.0, 0.0)));

        // A hand shaking 2 up and down every 8 along a line: the ink strays a quarter as much.
        let mut s = stroke(&[(0.0, 0.0)], 4.0);
        for i in 1..200 {
            let y = 2.0 * (i as f32 * std::f32::consts::TAU / 8.0).sin();
            s.push_point(Point::new(i as f32, y), 5.0);
        }
        let Drawing::Stroke { points, .. } = &s else {
            unreachable!()
        };
        let stray = points.iter().map(|p| p.y.abs()).fold(0.0, f32::max);
        assert!(stray < 0.5, "strays {stray}");
    }

    #[test]
    fn empty_drawings_are_too_small_to_keep() {
        assert!(stroke(&[(0.0, 0.0)], 4.0).is_empty());
        assert!(!stroke(&[(0.0, 0.0), (5.0, 0.0)], 4.0).is_empty());
        assert!(shape(ShapeKind::Rect, (0.0, 0.0), (1.0, 1.0)).is_empty());
        assert!(!shape(ShapeKind::Rect, (0.0, 0.0), (1.0, 3.0)).is_empty());
    }

    #[test]
    fn bounds_cover_the_ink_including_its_width() {
        let s = stroke(&[(10.0, 10.0), (20.0, 50.0), (0.0, 30.0)], 4.0);
        assert_eq!(s.bounds(), Rect::new(-2.0, 8.0, 24.0, 44.0));
        let r = shape(ShapeKind::Ellipse, (50.0, 40.0), (10.0, 20.0));
        assert_eq!(
            r.bounds(),
            Rect::new(9.0, 19.0, 42.0, 22.0),
            "from and to in any order"
        );
    }

    #[test]
    fn hits_count_the_tolerance_and_half_the_width() {
        let s = stroke(&[(0.0, 0.0), (100.0, 0.0)], 4.0);
        assert!(s.hit(Point::new(50.0, 7.9), 6.0), "6 + 2 away");
        assert!(!s.hit(Point::new(50.0, 8.1), 6.0));
        assert!(!s.hit(Point::new(110.0, 0.0), 6.0), "past the end");
        let line = shape(ShapeKind::Arrow, (0.0, 0.0), (100.0, 100.0));
        assert!(line.hit(Point::new(50.0, 54.0), 6.0));
        assert!(
            !line.hit(Point::new(80.0, 20.0), 6.0),
            "inside the box, far from the line"
        );
        let rect = shape(ShapeKind::Rect, (0.0, 0.0), (100.0, 60.0));
        assert!(rect.hit(Point::new(100.0, 30.0), 6.0), "on an edge");
        assert!(
            !rect.hit(Point::new(50.0, 30.0), 6.0),
            "the middle isn't ink"
        );
        let ellipse = shape(ShapeKind::Ellipse, (0.0, 0.0), (200.0, 100.0));
        assert!(ellipse.hit(Point::new(200.0, 50.0), 6.0), "on the rim");
        assert!(!ellipse.hit(Point::new(100.0, 50.0), 6.0), "the center");
        assert!(!ellipse.hit(Point::new(0.0, 0.0), 6.0), "the box's corner");
    }

    #[test]
    fn simplify_keeps_points_apart_and_both_ends() {
        let pts: Vec<Point> = (0..=10).map(|i| Point::new(i as f32, 0.0)).collect();
        let kept = simplify(&pts, 3.0);
        assert_eq!(
            kept.iter().map(|p| p.x).collect::<Vec<_>>(),
            [0.0, 3.0, 6.0, 9.0, 10.0],
            "the end stays, however close"
        );
        assert_eq!(simplify(&pts[..1], 3.0), pts[..1]);
        assert_eq!(simplify(&pts, 0.5), pts, "all far enough apart");
    }

    #[test]
    fn moving_shifts_every_point() {
        let mut s = stroke(&[(0.0, 0.0), (10.0, 10.0)], 2.0);
        s.move_by(5.0, -5.0);
        assert_eq!(s.bounds(), Rect::new(4.0, -6.0, 12.0, 12.0));
        let mut r = shape(ShapeKind::Rect, (0.0, 0.0), (10.0, 10.0));
        r.move_by(1.0, 1.0);
        let Drawing::Shape { from, to, .. } = r else {
            unreachable!()
        };
        assert_eq!((from, to), (Point::new(1.0, 1.0), Point::new(11.0, 11.0)));
    }

    #[test]
    fn shift_constrains_lines_to_45_degrees_and_boxes_to_squares() {
        let o = Point::new(0.0, 0.0);
        assert!(close(
            constrain(o, Point::new(100.0, 10.0), ShapeKind::Line),
            Point::new((100.0f32 * 100.0 + 10.0 * 10.0).sqrt(), 0.0)
        ));
        let d = constrain(o, Point::new(100.0, 95.0), ShapeKind::Arrow);
        assert!((d.x - d.y).abs() < 1e-3 && d.x > 0.0, "45°: {d:?}");
        assert_eq!(
            constrain(o, Point::new(100.0, -30.0), ShapeKind::Rect),
            Point::new(100.0, -100.0)
        );
        assert_eq!(
            constrain(o, Point::new(-20.0, 50.0), ShapeKind::Ellipse),
            Point::new(-50.0, 50.0)
        );
        assert_eq!(constrain(o, o, ShapeKind::Line), o);
    }
}
