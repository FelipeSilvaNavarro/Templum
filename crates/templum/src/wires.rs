//! Painting the wires between blocks, under them, and the dot that travels along one while a
//! task is handed over (docs/specs/connections.md, Wires).

use std::time::Duration;

use gpui_kit::{Bounds, Hsla, PathBuilder, Pixels, Window, fill, point, px, size};
use templum_canvas::{Camera, Curve, Point, Rect};

/// A wire's width, world units.
pub(crate) const WIDTH: f32 = 2.0;
/// The travelling dot's radius, world units.
const DOT: f32 = 4.0;
/// How long the dot takes from the agent that handed a task over to the one working on it.
pub(crate) const DOT_TRIP: Duration = Duration::from_millis(1200);
/// How often the dot is drawn while one travels in view: about 30 times a second, not at the
/// display's rate (a task can be handed over for minutes).
pub(crate) const DOT_FRAME: Duration = Duration::from_millis(33);

/// A wire's width on screen at `zoom`, pixels: never under one, so at 50% and below (the Card
/// level of docs/specs/level-of-detail.md comes there) it is 1 px.
pub(crate) fn width(zoom: f32) -> f32 {
    (WIDTH * zoom).max(1.0)
}

/// The dot's radius on screen at `zoom`, pixels: never under 2.5, so it shows on a 1 px wire.
pub(crate) fn dot_radius(zoom: f32) -> f32 {
    (DOT * zoom).max(2.5)
}

/// Paints `curves` (world units) through `camera` into `bounds`, then a dot of the color `dot` at
/// each of `dots`; those off screen are skipped.
// ponytail: one path per wire per frame. gpui-fast reuses the triangles of a shape built before
// (path_cache.rs), but while the camera moves the shapes differ by fractions of a pixel and are
// built again; cache them per zoom step as ink.rs does if a bench asks for it.
pub(crate) fn paint(
    curves: &[(Curve, Hsla)],
    dots: &[Point],
    dot: Hsla,
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
    let r = px(dot_radius(z));
    for p in dots.iter().filter(|p| seen.contains(**p)) {
        let c = at(*p);
        let square = Bounds::new(point(c.x - r, c.y - r), size(r * 2.0, r * 2.0));
        window.paint_quad(fill(square, dot).corner_radii(r));
    }
}

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

    #[test]
    fn the_dot_is_4_units_and_never_under_2_5_px() {
        assert_eq!(dot_radius(1.0), 4.0);
        assert_eq!(dot_radius(2.0), 8.0);
        assert_eq!(dot_radius(0.25), 2.5);
    }
}
