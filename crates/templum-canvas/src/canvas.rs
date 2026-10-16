//! The blocks on the canvas, in z-order.

use crate::{Camera, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub id: BlockId,
    pub rect: Rect,
}

/// Offset applied by [`Canvas::place_new`] while a spot is taken.
pub const CASCADE: f32 = 32.0;
/// The gap between a block and one placed beside it (world units).
pub const BESIDE: f32 = 24.0;

#[derive(Debug, Default)]
pub struct Canvas {
    pub camera: Camera,
    /// Back-to-front: the last block is drawn on top.
    blocks: Vec<Block>,
    next_id: u64,
}

impl Canvas {
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Adds a block on top. Ids are never reused.
    pub fn add(&mut self, rect: Rect) -> BlockId {
        let id = BlockId(self.next_id);
        self.next_id += 1;
        self.blocks.push(Block { id, rect });
        id
    }

    pub fn remove(&mut self, id: BlockId) -> Option<Block> {
        let i = self.index(id)?;
        Some(self.blocks.remove(i))
    }

    /// Brings a block to the front.
    pub fn raise(&mut self, id: BlockId) {
        if let Some(i) = self.index(id) {
            let block = self.blocks.remove(i);
            self.blocks.push(block);
        }
    }

    pub fn get(&self, id: BlockId) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: BlockId) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    pub fn top(&self) -> Option<BlockId> {
        self.blocks.last().map(|b| b.id)
    }

    /// Blocks intersecting a viewport of `viewport` size (screen units), back-to-front.
    pub fn visible(&self, viewport: Size) -> impl Iterator<Item = &Block> {
        self.visible_from(&self.camera, viewport)
    }

    /// Like [`Canvas::visible`], seen through `camera` (a frame of a flight).
    pub fn visible_from(&self, camera: &Camera, viewport: Size) -> impl Iterator<Item = &Block> {
        let z = camera.zoom();
        let world = Rect::new(
            camera.origin.x,
            camera.origin.y,
            viewport.w / z,
            viewport.h / z,
        );
        self.blocks
            .iter()
            .filter(move |b| b.rect.intersects(&world))
    }

    /// The blocks a selection rectangle `area` (world units) touches, back-to-front.
    pub fn hits(&self, area: Rect) -> Vec<BlockId> {
        self.blocks
            .iter()
            .filter(|b| b.rect.intersects(&area))
            .map(|b| b.id)
            .collect()
    }

    pub fn bounds(&self) -> Option<Rect> {
        self.blocks
            .iter()
            .map(|b| b.rect)
            .reduce(|a, b| a.union(&b))
    }

    /// Where a new block of `size` goes: at `near`, shifted by [`CASCADE`] while another
    /// block's top-left corner is within 8 units.
    pub fn place_new(&self, size: Size, near: Point) -> Rect {
        let taken: Vec<Rect> = self.blocks.iter().map(|b| b.rect).collect();
        cascade(Rect::new(near.x, near.y, size.w, size.h), &taken)
    }

    /// Frames every block; with none, back to 100% around the viewport center.
    pub fn fit_all(&mut self, viewport: Size) {
        match self.bounds() {
            Some(bounds) => self.camera.fit(bounds, viewport),
            None => self
                .camera
                .zoom_reset(Point::new(viewport.w / 2.0, viewport.h / 2.0)),
        }
    }

    fn index(&self, id: BlockId) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }
}

/// Where a block of `size` goes beside `anchors` (a block and those wired to it), over no rect of
/// `taken`: a BESIDE gap right of an anchor, below, left or above it, at its edge or a BESIDE
/// gap past each rect in the way along that side. The spot nearest the anchors all together wins (the
/// sum of its gaps to them), so a block's connections draw it their way; on a tie the first
/// anchor's, in that order of sides. With none free, right of everything.
pub fn free_spot(taken: &[Rect], anchors: &[Rect], size: Size) -> Rect {
    let first = anchors.first().copied().unwrap_or_default();
    let (w, h) = (size.w, size.h);
    let clear = |spot: &Rect| !taken.iter().any(|t| t.intersects(spot));
    let mut best: Option<(f32, Rect)> = None;
    for a in anchors {
        // Along a side: at the anchor's edge, then just past each rect in the way.
        let ys: Vec<f32> = std::iter::once(a.y)
            .chain(
                taken
                    .iter()
                    .flat_map(|t| [t.bottom() + BESIDE, t.y - BESIDE - h]),
            )
            .collect();
        let xs: Vec<f32> = std::iter::once(a.x)
            .chain(
                taken
                    .iter()
                    .flat_map(|t| [t.right() + BESIDE, t.x - BESIDE - w]),
            )
            .collect();
        let (right, left) = (a.right() + BESIDE, a.x - BESIDE - w);
        let (below, above) = (a.bottom() + BESIDE, a.y - BESIDE - h);
        let spots = (ys.iter().map(|&y| Rect::new(right, y, w, h)))
            .chain(xs.iter().map(|&x| Rect::new(x, below, w, h)))
            .chain(ys.iter().map(|&y| Rect::new(left, y, w, h)))
            .chain(xs.iter().map(|&x| Rect::new(x, above, w, h)));
        for spot in spots.filter(clear) {
            let far: f32 = anchors.iter().map(|a| gap(&spot, a).round()).sum();
            if best.is_none_or(|(least, _)| far < least) {
                best = Some((far, spot));
            }
        }
    }
    best.map_or_else(
        || {
            let edge = taken.iter().map(Rect::right).fold(first.right(), f32::max);
            Rect::new(edge + BESIDE, first.y, w, h)
        },
        |(_, spot)| spot,
    )
}

/// How far apart two rects are: 0 when they touch or overlap.
fn gap(a: &Rect, b: &Rect) -> f32 {
    let dx = (b.x - a.right()).max(a.x - b.right()).max(0.0);
    let dy = (b.y - a.bottom()).max(a.y - b.bottom()).max(0.0);
    dx.hypot(dy)
}

/// `rect`, shifted by [`CASCADE`] while one of `taken`'s top-left corners is within 8 units of
/// its own.
pub fn cascade(rect: Rect, taken: &[Rect]) -> Rect {
    let mut p = rect.origin();
    while taken
        .iter()
        .any(|t| (t.x - p.x).abs() < 8.0 && (t.y - p.y).abs() < 8.0)
    {
        p = Point::new(p.x + CASCADE, p.y + CASCADE);
    }
    Rect::new(p.x, p.y, rect.w, rect.h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32) -> Rect {
        Rect::new(x, y, 100.0, 50.0)
    }

    #[test]
    fn add_remove_raise_keep_back_to_front_order() {
        let mut c = Canvas::default();
        let a = c.add(rect(0.0, 0.0));
        let b = c.add(rect(10.0, 0.0));
        let d = c.add(rect(20.0, 0.0));
        assert_ne!(a, b);
        c.raise(a);
        let order: Vec<_> = c.blocks().iter().map(|b| b.id).collect();
        assert_eq!(order, [b, d, a]);
        assert_eq!(c.top(), Some(a));
        assert_eq!(c.remove(b).map(|b| b.id), Some(b));
        assert_eq!(c.remove(b), None);
        assert_eq!(c.blocks().len(), 2);
        let e = c.add(rect(0.0, 0.0));
        assert!(e != a && e != b && e != d, "ids are never reused");
    }

    #[test]
    fn visible_culls_blocks_outside_the_viewport() {
        let mut c = Canvas::default();
        let inside = c.add(rect(10.0, 10.0));
        c.add(rect(5000.0, 10.0));
        let ids: Vec<_> = c.visible(Size::new(800.0, 600.0)).map(|b| b.id).collect();
        assert_eq!(ids, [inside]);
        c.camera.zoom_step(-6, Point::new(0.0, 0.0)); // zoom 0.25: sees 3200 world units
        c.camera.origin = Point::new(2000.0, 0.0);
        assert_eq!(c.visible(Size::new(800.0, 600.0)).count(), 1);
    }

    #[test]
    fn visible_from_culls_with_another_camera() {
        let mut c = Canvas::default();
        c.add(rect(10.0, 10.0));
        let far = c.add(rect(5000.0, 10.0));
        let elsewhere = Camera::at(Point::new(4900.0, 0.0), 1.0);
        let ids: Vec<_> = c
            .visible_from(&elsewhere, Size::new(800.0, 600.0))
            .map(|b| b.id)
            .collect();
        assert_eq!(ids, [far]);
    }

    #[test]
    fn hits_are_the_blocks_a_rectangle_touches_in_any_direction() {
        let mut c = Canvas::default();
        let a = c.add(rect(0.0, 0.0));
        let b = c.add(rect(200.0, 0.0));
        c.add(rect(0.0, 300.0));
        // Dragged from bottom right to top left, over a's corner and b's left half.
        let area = Rect::corners(Point::new(250.0, 20.0), Point::new(90.0, -10.0));
        assert_eq!(area, Rect::new(90.0, -10.0, 160.0, 30.0));
        assert_eq!(c.hits(area), [a, b]);
        assert!(c.hits(Rect::new(500.0, 500.0, 10.0, 10.0)).is_empty());
    }

    #[test]
    fn bounds_is_the_union_of_all_blocks() {
        let mut c = Canvas::default();
        assert_eq!(c.bounds(), None);
        c.add(rect(0.0, 0.0));
        c.add(rect(200.0, 100.0));
        assert_eq!(c.bounds(), Some(Rect::new(0.0, 0.0, 300.0, 150.0)));
    }

    #[test]
    fn place_new_cascades_off_taken_spots() {
        let mut c = Canvas::default();
        let size = Size::new(100.0, 50.0);
        assert_eq!(c.place_new(size, Point::new(0.0, 0.0)), rect(0.0, 0.0));
        c.add(rect(0.0, 0.0));
        c.add(rect(CASCADE + 3.0, CASCADE - 2.0));
        assert_eq!(
            c.place_new(size, Point::new(1.0, 1.0)),
            rect(1.0 + 2.0 * CASCADE, 1.0 + 2.0 * CASCADE)
        );
    }

    #[test]
    fn a_free_spot_sits_to_the_right_and_slides_past_what_is_in_the_way() {
        let anchor = Rect::new(100.0, 50.0, 300.0, 200.0);
        let size = Size::new(100.0, 60.0);
        assert_eq!(
            free_spot(&[anchor], &[anchor], size),
            Rect::new(100.0 + 300.0 + BESIDE, 50.0, 100.0, 60.0)
        );
        let small = Rect::new(100.0 + 300.0 + BESIDE + 3.0, 52.0, 10.0, 10.0);
        assert_eq!(
            free_spot(&[anchor, small], &[anchor], size),
            Rect::new(100.0 + 300.0 + BESIDE, 62.0 + BESIDE, 100.0, 60.0),
            "down its right side, past the block there"
        );
    }

    /// `spot` is over no rect of `taken`.
    fn clear(spot: Rect, taken: &[Rect]) -> bool {
        !taken.iter().any(|t| t.intersects(&spot))
    }

    #[test]
    fn a_free_spot_goes_toward_the_anchors_connections() {
        let a = Rect::new(100.0, 50.0, 300.0, 200.0);
        let size = Size::new(100.0, 60.0);
        assert_eq!(
            free_spot(&[a], &[a], size),
            Rect::new(424.0, 50.0, 100.0, 60.0),
            "alone: on its right"
        );
        // Its connection is below it: the new block goes between them.
        let below = Rect::new(100.0, 700.0, 300.0, 100.0);
        assert_eq!(
            free_spot(&[a, below], &[a, below], size),
            Rect::new(100.0, 274.0, 100.0, 60.0)
        );
    }

    #[test]
    fn a_boxed_in_anchor_sends_the_block_beside_its_connection() {
        let a = Rect::new(100.0, 50.0, 300.0, 200.0);
        let walls = [
            Rect::new(424.0, 0.0, 400.0, 400.0),
            Rect::new(0.0, 274.0, 1000.0, 400.0),
            Rect::new(-500.0, 0.0, 576.0, 400.0),
            Rect::new(0.0, -500.0, 1000.0, 526.0),
        ];
        let far = Rect::new(2000.0, 50.0, 200.0, 100.0);
        let taken: Vec<Rect> = [a, far].into_iter().chain(walls).collect();
        let size = Size::new(100.0, 60.0);
        let spot = free_spot(&taken, &[a, far], size);
        assert!(clear(spot, &taken), "{spot:?}");
        assert!(
            spot.x > 824.0 && spot.right() <= far.x - BESIDE + 0.5,
            "past the walls, toward its connection: {spot:?}"
        );
    }

    /// Whatever is around, the spot found overlaps nothing.
    #[test]
    fn a_free_spot_never_overlaps() {
        let mut seed: u32 = 7;
        let mut next = |n: u32| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ((seed >> 16) % n) as f32
        };
        for _ in 0..200 {
            let taken: Vec<Rect> = (0..12)
                .map(|_| Rect::new(next(1600), next(1200), 50.0 + next(500), 40.0 + next(400)))
                .collect();
            let anchors = [taken[0], taken[1], taken[2]];
            let size = Size::new(80.0 + next(700), 60.0 + next(500));
            let spot = free_spot(&taken, &anchors, size);
            assert_eq!(spot.size(), size);
            assert!(clear(spot, &taken), "{spot:?} in {taken:?}");
        }
    }

    #[test]
    fn fit_all_frames_blocks_or_resets_when_empty() {
        let mut c = Canvas::default();
        c.camera.zoom_step(3, Point::new(0.0, 0.0));
        c.fit_all(Size::new(800.0, 600.0));
        assert_eq!(c.camera.zoom(), 1.0, "empty canvas resets to 100%");
        c.add(Rect::new(0.0, 0.0, 2000.0, 100.0));
        c.fit_all(Size::new(800.0, 600.0));
        assert!(c.camera.zoom() < 1.0);
        assert_eq!(c.visible(Size::new(800.0, 600.0)).count(), 1);
    }
}
