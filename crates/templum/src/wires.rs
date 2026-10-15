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
// ponytail: one path per wire per frame. gpui-fast reuses the triangles of a shape built before
// (path_cache.rs), but while the camera moves the shapes differ by fractions of a pixel and are
// built again; cache them per zoom step as ink.rs does if a bench asks for it.
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
