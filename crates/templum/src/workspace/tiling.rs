//! Grid mode (docs/specs/grid.md): a workspace's blocks tiled over its area like Hyprland's
//! dwindle, at zoom 1, and back to the canvas as they were. While it lasts, a tiled block's
//! canvas rect is its tile's and the camera is at the origin at zoom 1, so drawing, the pointer
//! and a terminal's size work as they do on the canvas; their places on the canvas, and the
//! canvas's camera, wait in `Grid`.

use gpui_kit::{Context, Window};
use templum_canvas::{BlockId, Camera, Direction, GAP, Rect, Size, Tile, neighbour};

use super::{Grid, Tool, Workspace};

/// What the tiles fill in a workspace area of `viewport`: all of it inside the gap.
pub(super) fn grid_area(viewport: Size) -> Rect {
    Rect::new(
        GAP,
        GAP,
        (viewport.w - 2.0 * GAP).max(0.0),
        (viewport.h - 2.0 * GAP).max(0.0),
    )
}

impl Workspace {
    /// Ctrl+Shift+Enter: grid mode for the selected blocks, else the focused one; in grid mode,
    /// back to the canvas.
    pub(crate) fn toggle_grid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.space.grid.is_some() {
            self.leave_grid(window, cx);
            return;
        }
        let ids = if self.space.selected.is_empty() {
            self.space.focused.into_iter().collect()
        } else {
            self.space.selected.clone()
        };
        self.enter_grid(ids, window, cx);
    }

    /// A block's fullscreen button: the block in grid mode (with the selection it is in), or, in
    /// grid mode, back to the canvas.
    pub(crate) fn fullscreen(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.space.grid.is_some() {
            self.leave_grid(window, cx);
        } else if self.space.selected.contains(&id) {
            self.enter_grid(self.space.selected.clone(), window, cx);
        } else {
            self.enter_grid(vec![id], window, cx);
        }
    }

    /// Grid mode for `ids`, in their order on the canvas: left to right, then top to bottom.
    pub(crate) fn enter_grid(
        &mut self,
        mut ids: Vec<BlockId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let canvas = &self.space.canvas;
        ids.retain(|id| canvas.get(*id).is_some());
        ids.sort_by(|a, b| {
            let (a, b) = (canvas.get(*a).unwrap().rect, canvas.get(*b).unwrap().rect);
            a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
        });
        let area = grid_area(self.viewport(window));
        let Some(tiles) = Tile::dwindle(&ids, area, GAP) else {
            return;
        };
        self.grid_on(tiles, area, window, cx);
    }

    /// `tiles` fill `area`; each block's canvas rect and the camera wait to go back. The focused
    /// block keeps the keyboard if it is tiled; otherwise the first tile takes it.
    pub(super) fn grid_on(
        &mut self,
        tiles: Tile,
        area: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let space = &mut self.space;
        let homes = tiles
            .ids()
            .into_iter()
            .filter_map(|id| Some((id, space.canvas.get(id)?.rect)))
            .collect();
        let first = tiles.ids()[0];
        space.grid = Some(Grid {
            tiles,
            homes,
            camera: space.canvas.camera,
            area,
        });
        space.canvas.camera = Camera::default();
        self.calm();
        self.retile();
        let focused = self
            .space
            .focused
            .filter(|id| self.space.shown(*id))
            .unwrap_or(first);
        self.focus_block(focused, window, cx);
        cx.notify();
    }

    /// Back to the canvas: every tiled block where it was on it, and the camera as it was.
    pub(crate) fn leave_grid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(grid) = self.space.grid.take() else {
            return;
        };
        for (id, home) in grid.homes {
            if let Some(b) = self.space.canvas.get_mut(id) {
                b.rect = home;
                self.fit_terminal(id, home);
            }
        }
        self.space.canvas.camera = grid.camera;
        self.calm();
        if let Some(id) = self.space.focused {
            self.focus_block(id, window, cx);
        }
        cx.notify();
    }

    /// The tiles fill the grid's area: each tiled block's rect is its tile's, a terminal's grid
    /// following (only when it changed: the window or the sidebar did).
    pub(super) fn retile(&mut self) {
        let Some(grid) = &self.space.grid else {
            return;
        };
        for (id, rect) in grid.tiles.rects(grid.area, GAP) {
            if let Some(b) = self.space.canvas.get_mut(id)
                && b.rect != rect
            {
                b.rect = rect;
                self.fit_terminal(id, rect);
            }
        }
    }

    /// Block `id`, on the canvas, takes half of `at`'s tile (of the last tile when `at` isn't
    /// one); its rect there waits for the grid to end.
    pub(super) fn join_grid(&mut self, id: BlockId, at: Option<BlockId>) {
        let Some(home) = self.space.canvas.get(id).map(|b| b.rect) else {
            return;
        };
        let Some(grid) = &mut self.space.grid else {
            return;
        };
        if grid.homes.contains_key(&id) {
            return;
        }
        let at = at
            .filter(|at| grid.homes.contains_key(at))
            .or_else(|| grid.tiles.ids().last().copied());
        if let Some(at) = at {
            grid.tiles.insert(at, id, grid.area, GAP);
            grid.homes.insert(id, home);
            self.retile();
        }
    }

    /// The focused tile and the one on its `to` side.
    fn beside(&self, to: Direction) -> Option<(BlockId, BlockId)> {
        let grid = self.space.grid.as_ref()?;
        let from = self
            .space
            .focused
            .filter(|id| grid.homes.contains_key(id))?;
        let rects = grid.tiles.rects(grid.area, GAP);
        Some((from, neighbour(&rects, from, to)?))
    }

    /// Ctrl+Alt+arrows: the keyboard goes to the tile on that side.
    pub(super) fn focus_tile(
        &mut self,
        to: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, next)) = self.beside(to) {
            self.focus_block(next, window, cx);
        }
    }

    /// Ctrl+Alt+Shift+arrows: the focused tile trades places with the one on that side.
    pub(super) fn swap_tile(&mut self, to: Direction, cx: &mut Context<Self>) {
        let Some((from, other)) = self.beside(to) else {
            return;
        };
        if let Some(grid) = &mut self.space.grid {
            grid.tiles.swap(from, other);
        }
        self.retile();
        cx.notify();
    }

    /// Block `id` is no tile anymore (it is closing): its tile's other half takes the pair's
    /// place; without tiles, the canvas comes back. Its canvas rect, if it was tiled.
    pub(super) fn untile(&mut self, id: BlockId) -> Option<Rect> {
        let grid = self.space.grid.as_mut()?;
        let home = grid.homes.remove(&id)?;
        match std::mem::replace(&mut grid.tiles, Tile::Leaf(id)).without(id) {
            Some(tiles) => grid.tiles = tiles,
            None => {
                let camera = grid.camera;
                self.space.grid = None;
                self.space.canvas.camera = camera;
                self.calm();
            }
        }
        Some(home)
    }

    /// What only makes sense on the canvas or in the grid ends when the workspace goes from one
    /// to the other: fading and popping blocks, a drag, a glide, the selection, a drawing tool.
    fn calm(&mut self) {
        let space = &mut self.space;
        space.ghosts.clear();
        space.appearing.clear();
        space.selected.clear();
        space.selected_ink = None;
        space.framed = None;
        space.flight = None;
        space.view = space.canvas.camera;
        space.guides.clear();
        space.hovered = None;
        space.pristine = false;
        self.drag = None;
        self.press = None;
        self.placed.clear();
        self.tool = Tool::Select;
        self.shapes_open = false;
        self.menu = None;
    }
}
