//! Painting the drawings above the blocks, and the stroke options. See docs/specs/cockpit.md,
//! section 4.

use gpui_kit::{
    BorderStyle, Bounds, Hsla, Path, PathBuilder, PathStyle, PathVertex, Pixels, StrokeOptions,
    Window, outline, point, px, size,
};
use lyon_tessellation::{LineCap, LineJoin};
use templum_canvas::{Camera, Color, Drawing, Point, Rect, ShapeKind, Size, ZOOM_STEPS, simplify};

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
/// Strokes are drawn through points at least this far apart on screen (pixels).
const POINT_PX: f32 = 2.0;
/// Points closer than this on screen (pixels) are joined by straight lines, not curves.
const CURVE_PX: f32 = 8.0;
/// How far (pixels) the drawn outline may stray from the exact one: curves and round ends are
/// cut into fewer pieces.
const TOLERANCE_PX: f32 = 0.25;
/// A line at most this wide on screen (pixels) gets square ends, at most `ROUND_JOIN_PX` wide
/// pointed joins.
const THIN_PX: f32 = 2.0;
const ROUND_JOIN_PX: f32 = 6.0;
/// Drawings built again per frame after the zoom reached another step; the others are drawn
/// scaled from their previous step's paths until their turn.
const REBUILDS: usize = 24;

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
    cache: &mut Cache,
    window: &mut Window,
) {
    let (runs, stale) = runs(drawings, live, camera, theme, bounds, cache);
    for (color, path) in runs {
        window.paint_path(path, color);
    }
    if stale {
        window.request_animation_frame();
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

/// The drawings' built paths, kept between frames by their index: one is tessellated again only
/// when it changes or the view reaches another zoom step.
#[derive(Default)]
pub(crate) struct Cache(Vec<Option<Built>>);

struct Built {
    drawing: Drawing,
    zoom: f32,
    /// At `zoom`, with the world's origin at (0, 0).
    path: Path<Pixels>,
}

/// The paths to paint, in order, with their colors: one per run of drawings of one color, so
/// GPUI copies and sorts a few primitives instead of one per stroke. True when some were drawn
/// from another zoom step's paths, scaled: the next frames build them again, a few at a time.
fn runs(
    drawings: &[Drawing],
    live: Option<&Drawing>,
    camera: &Camera,
    theme: &Theme,
    bounds: Bounds<Pixels>,
    cache: &mut Cache,
) -> (Vec<(Hsla, Path<Pixels>)>, bool) {
    let z = camera.zoom();
    // Built at the zoom step nearest the view: a flight between steps scales what was built.
    let step = ZOOM_STEPS[camera.zoom_index()];
    let offset = point(
        bounds.origin.x - px(camera.origin.x * z),
        bounds.origin.y - px(camera.origin.y * z),
    );
    let viewport = Size::new(f32::from(bounds.size.width), f32::from(bounds.size.height));
    // Once per drawing: `bounds()` walks every point.
    let shown: Vec<bool> = drawings
        .iter()
        .map(|d| visible(d, camera, viewport))
        .collect();
    let highlights = (0..drawings.len()).filter(|i| shown[*i] && is_highlight(&drawings[*i]));
    let inks = (0..drawings.len()).filter(|i| shown[*i] && !is_highlight(&drawings[*i]));
    cache.0.resize_with(drawings.len(), || None);
    let mut out = Vec::new();
    let mut rebuilds = 0;
    let mut stale = false;
    for i in highlights.chain(inks) {
        let d = &drawings[i];
        let slot = &mut cache.0[i];
        let current = slot.as_ref().filter(|b| b.drawing == *d);
        // Another step's path does for a frame or two: rebuilding every stroke on the frame the
        // zoom crosses a step would stall it.
        let rebuild = match current {
            None => true,
            Some(b) if b.zoom != step => {
                rebuilds += 1;
                stale |= rebuilds > REBUILDS;
                rebuilds <= REBUILDS
            }
            Some(_) => false,
        };
        if rebuild {
            *slot = build(d, step).map(|path| Built {
                drawing: d.clone(),
                zoom: step,
                path,
            });
        }
        if let Some(b) = slot {
            let color = color_of(d.color(), theme, is_highlight(d));
            append(&mut out, color, &b.path, z / b.zoom, offset);
        }
    }
    if let Some(d) = live
        && let Some(path) = build(d, z)
    {
        let color = color_of(d.color(), theme, is_highlight(d));
        append(&mut out, color, &path, 1.0, offset);
    }
    (out, stale)
}

/// Adds `path`, scaled by `k` then moved by `offset`, to the last run when it has `color`.
fn append(
    out: &mut Vec<(Hsla, Path<Pixels>)>,
    color: Hsla,
    path: &Path<Pixels>,
    k: f32,
    offset: gpui_kit::Point<Pixels>,
) {
    let place = |p: gpui_kit::Point<Pixels>| point(p.x * k + offset.x, p.y * k + offset.y);
    let b = path.bounds;
    let bounds = Bounds::new(place(b.origin), size(b.size.width * k, b.size.height * k));
    let vertices = path.vertices.iter().map(|v| PathVertex {
        xy_position: place(v.xy_position),
        ..v.clone()
    });
    match out.last_mut() {
        Some((c, joined)) if *c == color => {
            joined.bounds = joined.bounds.union(&bounds);
            joined.vertices.extend(vertices);
        }
        _ => {
            let mut joined = Path::new(bounds.origin);
            joined.bounds = bounds;
            joined.vertices.extend(vertices);
            out.push((color, joined));
        }
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

/// A drawing's path at zoom `z`, with the world's origin at (0, 0); None when there is nothing
/// to draw.
fn build(d: &Drawing, z: f32) -> Option<Path<Pixels>> {
    let width = (d.width() * z).max(1.0);
    let at = |p: Point| point(px(p.x * z), px(p.y * z));
    // Round ends and joins cost vertices, and don't show on a thin line (joins: on a narrow one,
    // where a smooth stroke turns a little at each point).
    let cap = if width > THIN_PX {
        LineCap::Round
    } else {
        LineCap::Butt
    };
    let join = if width > ROUND_JOIN_PX {
        LineJoin::Round
    } else {
        LineJoin::Miter
    };
    let mut path = PathBuilder::default().with_style(PathStyle::Stroke(
        StrokeOptions::default()
            .with_line_width(width)
            .with_tolerance(TOLERANCE_PX)
            .with_line_cap(cap)
            .with_line_join(join),
    ));
    match d {
        Drawing::Stroke { points, .. } => {
            // Points closer on screen than a couple of pixels add vertices, not shape.
            let points = simplify(points, POINT_PX / z);
            let first = points.first()?;
            path.move_to(at(*first));
            // Smooth: quadratic curves through the midpoints, each point a control point; where
            // the points are close on screen (always, zoomed out), straight lines look the same.
            let near = |a: Point, b: Point| (a.x - b.x).hypot(a.y - b.y) * z < CURVE_PX;
            for w in points.windows(3) {
                let (prev, ctrl, next) = (w[0], w[1], w[2]);
                if near(prev, ctrl) && near(ctrl, next) {
                    path.line_to(at(ctrl));
                } else {
                    let mid = Point::new((ctrl.x + next.x) / 2.0, (ctrl.y + next.y) / 2.0);
                    path.curve_to(at(mid), at(ctrl));
                }
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
    path.build().ok().filter(|p| !p.vertices.is_empty())
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

    #[test]
    fn built_paths_are_kept_until_the_drawing_or_the_zoom_step_changes() {
        let theme = crate::theme::Theme::new("t", templum_terminal::Palette::default(), None, None);
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)));
        let stroke = |y: f32| Drawing::Stroke {
            points: (0..20).map(|i| Point::new(i as f32 * 10.0, y)).collect(),
            width: 4.0,
            color: Color::Red,
            highlight: false,
        };
        let mut drawings = vec![stroke(100.0)];
        let mut cache = Cache::default();
        let at = |x: f32, zoom: f32| Camera::at(Point::new(x, 0.0), zoom);
        let (first, stale) = runs(&drawings, None, &at(0.0, 1.0), &theme, bounds, &mut cache);
        assert!(!stale);
        let x0 = first[0].1.vertices[0].xy_position.x;
        // Panned: the same path, moved.
        let (panned, _) = runs(&drawings, None, &at(10.0, 1.0), &theme, bounds, &mut cache);
        assert_eq!(panned[0].1.vertices[0].xy_position.x, x0 - px(10.0));
        // Kept: what the cache holds is what is drawn.
        cache.0[0].as_mut().unwrap().path.vertices.clear();
        let (kept, _) = runs(&drawings, None, &at(0.0, 1.0), &theme, bounds, &mut cache);
        assert!(kept[0].1.vertices.is_empty());
        // A changed drawing is built again.
        drawings[0].move_by(0.0, 1.0);
        let (moved, _) = runs(&drawings, None, &at(0.0, 1.0), &theme, bounds, &mut cache);
        assert_eq!(moved[0].1.vertices.len(), first[0].1.vertices.len());
        // Another zoom step: a few are built again per frame, the rest scaled until their turn.
        drawings = (0..REBUILDS + 5).map(|i| stroke(i as f32 * 5.0)).collect();
        runs(&drawings, None, &at(0.0, 1.0), &theme, bounds, &mut cache);
        let step = |cache: &Cache| cache.0.iter().flatten().filter(|b| b.zoom == 1.1).count();
        let (_, stale) = runs(&drawings, None, &at(0.0, 1.1), &theme, bounds, &mut cache);
        assert!(stale, "some wait for the next frame");
        assert_eq!(step(&cache), REBUILDS);
        let (_, stale) = runs(&drawings, None, &at(0.0, 1.1), &theme, bounds, &mut cache);
        assert!(!stale);
        assert_eq!(step(&cache), REBUILDS + 5);
    }
}
