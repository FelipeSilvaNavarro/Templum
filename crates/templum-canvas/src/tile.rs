//! Grid mode's layout: blocks tiling an area like Hyprland's dwindle. Each pair of tiles shares
//! its area in halves, side by side or one above the other; a new tile takes half of the one it
//! splits, along its longer side.

use crate::{BlockId, Rect, Size};

/// The space between tiles, and around them (screen pixels).
pub const GAP: f32 = 8.0;

/// How a pair of tiles shares its area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    /// The first half on the left, the second on the right.
    Beside,
    /// The first half above the second.
    Above,
}

impl Split {
    /// How a tile of `size` splits: along its longer side, as dwindle does.
    pub fn of(size: Size) -> Split {
        if size.w >= size.h {
            Split::Beside
        } else {
            Split::Above
        }
    }
}

/// A tree of tiles: a block's, or a pair sharing an area.
#[derive(Clone, Debug, PartialEq)]
pub enum Tile {
    Leaf(BlockId),
    Pair(Split, Box<Tile>, Box<Tile>),
}

/// `area`'s two halves for `split`, `gap` apart; never a negative size.
fn halves(area: Rect, split: Split, gap: f32) -> (Rect, Rect) {
    match split {
        Split::Beside => {
            let first = ((area.w - gap) / 2.0).floor().max(0.0);
            let second = (area.w - gap - first).max(0.0);
            (
                Rect::new(area.x, area.y, first, area.h),
                Rect::new(area.right() - second, area.y, second, area.h),
            )
        }
        Split::Above => {
            let first = ((area.h - gap) / 2.0).floor().max(0.0);
            let second = (area.h - gap - first).max(0.0);
            (
                Rect::new(area.x, area.y, area.w, first),
                Rect::new(area.x, area.bottom() - second, area.w, second),
            )
        }
    }
}

impl Tile {
    /// Its blocks, the first half's before the second's.
    pub fn ids(&self) -> Vec<BlockId> {
        match self {
            Tile::Leaf(id) => vec![*id],
            Tile::Pair(_, a, b) => {
                let mut ids = a.ids();
                ids.extend(b.ids());
                ids
            }
        }
    }

    /// Each block's rect when the tree fills `area`, tiles `gap` apart.
    pub fn rects(&self, area: Rect, gap: f32) -> Vec<(BlockId, Rect)> {
        match self {
            Tile::Leaf(id) => vec![(*id, area)],
            Tile::Pair(split, a, b) => {
                let (ra, rb) = halves(area, *split, gap);
                let mut rects = a.rects(ra, gap);
                rects.extend(b.rects(rb, gap));
                rects
            }
        }
    }

    /// Block `id` takes half of `at`'s tile (the second half), split along its longer side as it
    /// is in `area`. False when `at` isn't tiled.
    pub fn insert(&mut self, at: BlockId, id: BlockId, area: Rect, gap: f32) -> bool {
        match self {
            Tile::Leaf(leaf) if *leaf == at => {
                *self = Tile::Pair(
                    Split::of(area.size()),
                    Box::new(Tile::Leaf(at)),
                    Box::new(Tile::Leaf(id)),
                );
                true
            }
            Tile::Leaf(_) => false,
            Tile::Pair(split, a, b) => {
                let (ra, rb) = halves(area, *split, gap);
                a.insert(at, id, ra, gap) || b.insert(at, id, rb, gap)
            }
        }
    }

    /// `ids` tiling `area`: each takes half of the one before it. None without ids.
    pub fn dwindle(ids: &[BlockId], area: Rect, gap: f32) -> Option<Tile> {
        let (first, rest) = ids.split_first()?;
        let mut tile = Tile::Leaf(*first);
        let mut last = *first;
        for id in rest {
            tile.insert(last, *id, area, gap);
            last = *id;
        }
        Some(tile)
    }

    /// The tree without block `id`: the other half of its pair takes the pair's place. None when
    /// it was the last tile.
    pub fn without(self, id: BlockId) -> Option<Tile> {
        match self {
            Tile::Leaf(leaf) if leaf == id => None,
            Tile::Leaf(_) => Some(self),
            Tile::Pair(split, a, b) => match (a.without(id), b.without(id)) {
                (Some(a), Some(b)) => Some(Tile::Pair(split, Box::new(a), Box::new(b))),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: BlockId = BlockId(1);
    const B: BlockId = BlockId(2);
    const C: BlockId = BlockId(3);

    #[test]
    fn a_pair_shares_its_area_in_halves_a_gap_apart() {
        let pair = Tile::Pair(
            Split::Beside,
            Box::new(Tile::Leaf(A)),
            Box::new(Tile::Leaf(B)),
        );
        assert_eq!(
            pair.rects(Rect::new(0.0, 0.0, 100.0, 50.0), 8.0),
            [
                (A, Rect::new(0.0, 0.0, 46.0, 50.0)),
                (B, Rect::new(54.0, 0.0, 46.0, 50.0))
            ]
        );
        let odd = Tile::Pair(
            Split::Above,
            Box::new(Tile::Leaf(A)),
            Box::new(Tile::Leaf(B)),
        );
        assert_eq!(
            odd.rects(Rect::new(10.0, 10.0, 40.0, 101.0), 8.0),
            [
                (A, Rect::new(10.0, 10.0, 40.0, 46.0)),
                (B, Rect::new(10.0, 64.0, 40.0, 47.0))
            ],
            "the second half takes the odd pixel; both end at the area's edges"
        );
    }

    #[test]
    fn dwindle_splits_each_new_tile_along_the_longer_side() {
        let area = Rect::new(0.0, 0.0, 1000.0, 600.0);
        let tile = Tile::dwindle(&[A, B, C], area, 8.0).unwrap();
        assert_eq!(
            tile,
            Tile::Pair(
                Split::Beside,
                Box::new(Tile::Leaf(A)),
                Box::new(Tile::Pair(
                    Split::Above,
                    Box::new(Tile::Leaf(B)),
                    Box::new(Tile::Leaf(C))
                ))
            ),
            "a | (b / c)"
        );
        assert_eq!(tile.ids(), [A, B, C]);
        assert_eq!(Tile::dwindle(&[], area, 8.0), None);
        let mut one = Tile::Leaf(A);
        assert!(!one.insert(B, C, area, 8.0), "b isn't tiled");
    }

    #[test]
    fn without_a_tile_its_other_half_takes_the_place() {
        let area = Rect::new(0.0, 0.0, 1000.0, 600.0);
        let tile = Tile::dwindle(&[A, B, C], area, 8.0).unwrap();
        let rest = tile.clone().without(B).unwrap();
        assert_eq!(
            rest.rects(area, 8.0)[1],
            (C, Rect::new(504.0, 0.0, 496.0, 600.0)),
            "c has the whole right half"
        );
        assert_eq!(tile.clone().without(BlockId(9)), Some(tile));
        assert_eq!(Tile::Leaf(A).without(A), None, "the last one");
    }

    #[test]
    fn a_tiny_area_has_no_negative_sizes() {
        let tile = Tile::dwindle(&[A, B, C], Rect::new(0.0, 0.0, 5.0, 3.0), 8.0).unwrap();
        assert!(
            tile.rects(Rect::new(0.0, 0.0, 5.0, 3.0), 8.0)
                .iter()
                .all(|(_, r)| r.w >= 0.0 && r.h >= 0.0)
        );
    }
}
