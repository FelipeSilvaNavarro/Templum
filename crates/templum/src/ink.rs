//! Painting the drawings above the blocks, and the stroke options. See docs/specs/cockpit.md,
//! section 4.

use gpui_kit::{
    BorderStyle, Bounds, Hsla, PathBuilder, PathStyle, Pixels, StrokeOptions, Window, outline,
    point, px, size,
};
use lyon_tessellation::{LineCap, LineJoin};
use templum_canvas::{Camera, Color, Drawing, Point, Rect, ShapeKind, Size};

use crate::grid::hsla;
use crate::theme::{Theme, Ui};

/// The pen's widths, world units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Width {
    Thin,
    #[default]
    Medium,
    Thick,
}

impl Width {
    pub const ALL: [Width; 3] = [Width::Thin, Width::Medium, Width::Thick];

    pub fn units(self) -> f32 {
        match self {
            Width::Thin => 2.0,
            Width::Medium => 4.0,
            Width::Thick => 8.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Width::Thin => "thin",
            Width::Medium => "medium",
            Width::Thick => "thick",
        }
    }
}

pub(crate) const HIGHLIGHTER_WIDTH: f32 = 16.0;
const HIGHLIGHT_ALPHA: f32 = 0.35;
/// Around a selected drawing's box, screen pixels.
const SELECTION_PAD: f32 = 4.0;

/// A drawing's color in the theme; highlighters are translucent.
pub(crate) fn color_of(color: Color, theme: &Theme, highlight: bool) -> Hsla {
    let p = &theme.palette;
    let rgb = match color {
        Color::Foreground => p.foreground,
        Color::Red => p.ansi[1],
        Color::Yellow => p.ansi[3],
        Color::Green => p.ansi[2],
        Color::Cyan => p.ansi[6],
        Color::Blue => p.ansi[4],
        Color::Magenta => p.ansi[5],
    };
    hsla(rgb, if highlight { HIGHLIGHT_ALPHA } else { 1.0 })
}

/// Whether any of the drawing is in the viewport (screen size), through `camera`.
pub(crate) fn visible(drawing: &Drawing, camera: &Camera, viewport: Size) -> bool {
    let z = camera.zoom();
    let world = Rect::new(
        camera.origin.x,
        camera.origin.y,
        viewport.w / z,
        viewport.h / z,
    );
    drawing.bounds().intersects(&world)
}

/// Paints the drawings: highlighters first, then the rest, then the one being drawn, then the
/// selected one's outline. `bounds` is the layer's, in window coordinates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint(
    drawings: &[Drawing],
    live: Option<&Drawing>,
    selected: Option<usize>,
    camera: &Camera,
    theme: &Theme,
    ui: Ui,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let viewport = Size::new(f32::from(bounds.size.width), f32::from(bounds.size.height));
    let on_screen = |d: &&Drawing| visible(d, camera, viewport);
    let highlights = drawings
        .iter()
        .filter(on_screen)
        .filter(|d| is_highlight(d));
    let inks = drawings
        .iter()
        .filter(on_screen)
        .filter(|d| !is_highlight(d));
    for d in highlights.chain(inks).chain(live) {
        paint_one(d, camera, theme, bounds, window);
    }
    if let Some(d) = selected.and_then(|i| drawings.get(i)) {
        let b = d.bounds();
        let pad = SELECTION_PAD / camera.zoom();
        let tl = to_screen(camera, bounds, Point::new(b.x - pad, b.y - pad));
        let br = to_screen(
            camera,
            bounds,
            Point::new(b.right() + pad, b.bottom() + pad),
        );
        let rect = Bounds::new(tl, size(br.x - tl.x, br.y - tl.y));
        window.paint_quad(outline(rect, ui.accent, BorderStyle::Solid));
    }
}

fn is_highlight(d: &Drawing) -> bool {
    matches!(
        d,
        Drawing::Stroke {
            highlight: true,
            ..
        }
    )
}

fn to_screen(camera: &Camera, bounds: Bounds<Pixels>, p: Point) -> gpui_kit::Point<Pixels> {
    let s = camera.to_screen(p);
    point(bounds.origin.x + px(s.x), bounds.origin.y + px(s.y))
}

fn paint_one(
    d: &Drawing,
    camera: &Camera,
    theme: &Theme,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let z = camera.zoom();
    let width = px((d.width() * z).max(1.0));
    let at = |p: Point| to_screen(camera, bounds, p);
    let mut path = PathBuilder::default().with_style(PathStyle::Stroke(
        StrokeOptions::default()
            .with_line_width(f32::from(width))
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round),
    ));
    match d {
        Drawing::Stroke { points, .. } => {
            let Some(first) = points.first() else {
                return;
            };
            path.move_to(at(*first));
            // Smooth: quadratic curves through the midpoints, each point a control point.
            for w in points.windows(2).skip(1) {
                let (ctrl, next) = (w[0], w[1]);
                let mid = Point::new((ctrl.x + next.x) / 2.0, (ctrl.y + next.y) / 2.0);
                path.curve_to(at(mid), at(ctrl));
            }
            if let Some(last) = points.last().filter(|_| points.len() >= 2) {
                path.line_to(at(*last));
            }
        }
        Drawing::Shape {
            kind: ShapeKind::Line,
            from,
            to,
            ..
        } => {
            path.move_to(at(*from));
            path.line_to(at(*to));
        }
        Drawing::Shape {
            kind: ShapeKind::Arrow,
            from,
            to,
            width,
            ..
        } => {
            path.move_to(at(*from));
            path.line_to(at(*to));
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.0 {
                let head = (4.0 * width).max(10.0);
                let (ux, uy) = (dx / len, dy / len);
                let (s, c) = (30.0f32.to_radians().sin(), 30.0f32.to_radians().cos());
                for sign in [1.0, -1.0] {
                    // The unit vector rotated by ±30°, pointing back from the tip.
                    let rx = -(ux * c - uy * s * sign);
                    let ry = -(ux * s * sign + uy * c);
                    path.move_to(at(*to));
                    path.line_to(at(Point::new(to.x + rx * head, to.y + ry * head)));
                }
            }
        }
        Drawing::Shape {
            kind: ShapeKind::Rect,
            from,
            to,
            ..
        } => {
            let corners = [
                at(*from),
                at(Point::new(to.x, from.y)),
                at(*to),
                at(Point::new(from.x, to.y)),
            ];
            path.add_polygon(&corners, true);
        }
        Drawing::Shape {
            kind: ShapeKind::Ellipse,
            from,
            to,
            ..
        } => {
            let (cx, cy) = ((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
            let (rx, ry) = ((to.x - from.x).abs() / 2.0, (to.y - from.y).abs() / 2.0);
            const SIDES: usize = 48;
            let pts: Vec<_> = (0..SIDES)
                .map(|i| {
                    let a = i as f32 / SIDES as f32 * std::f32::consts::TAU;
                    at(Point::new(cx + rx * a.cos(), cy + ry * a.sin()))
                })
                .collect();
            path.add_polygon(&pts, true);
        }
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color_of(d.color(), theme, is_highlight(d)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use templum_canvas::{Camera, Color, Drawing, Point, Size};

    #[test]
    fn paint_culls_drawings_outside_the_viewport() {
        let stroke = |x: f32| Drawing::Stroke {
            points: vec![Point::new(x, 10.0), Point::new(x + 50.0, 20.0)],
            width: 4.0,
            color: Color::Red,
            highlight: false,
        };
        let camera = Camera::at(Point::new(0.0, 0.0), 1.0);
        let viewport = Size::new(800.0, 600.0);
        assert!(visible(&stroke(10.0), &camera, viewport));
        assert!(visible(&stroke(790.0), &camera, viewport), "a bit inside");
        assert!(!visible(&stroke(5000.0), &camera, viewport));
        let far = Camera::at(Point::new(4950.0, 0.0), 1.0);
        assert!(visible(&stroke(5000.0), &far, viewport));
    }

    #[test]
    fn widths_and_colors() {
        assert_eq!(Width::ALL.map(Width::units), [2.0, 4.0, 8.0]);
        let theme = crate::theme::Theme::new("t", templum_terminal::Palette::default(), None, None);
        let red = color_of(Color::Red, &theme, false);
        assert_eq!(red, crate::grid::hsla(theme.palette.ansi[1], 1.0));
        assert_eq!(color_of(Color::Foreground, &theme, true).a, 0.35);
    }
}
