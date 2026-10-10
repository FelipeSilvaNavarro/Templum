//! The canvas: camera, blocks, input and painting.
//!
//! GPUI has no subtree transform, so zoom is done by hand (same idea as gpui-whiteboard, MIT):
//! positions, sizes and font size are multiplied by the zoom and everything is laid out again,
//! so glyphs are rasterized at their real size.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use futures::StreamExt;
use gpui::{
    App, BorderStyle, Bounds, ContentMask, Context, Corners, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Font, FontWeight, Hsla, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, Point, Render, Rgba,
    ScrollDelta, ScrollWheelEvent, SharedString, Task, TextAlign, TextRun, UTF16Selection, Window,
    canvas, div, fill, font, outline, point, prelude::*, px, quad, rgb, size,
};

use crate::term::{self, DEFAULT_BG, DEFAULT_FG, Listener, Terminal, Wake};

/// Fixed zoom steps keep the glyph atlas bounded (one rasterization per size).
pub const ZOOM_STEPS: [f32; 14] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0,
];
const ZOOM_1: usize = 6;
// World units are logical pixels at zoom 1.
const FONT_SIZE: f32 = 14.0;
const LINE_H: f32 = 18.0;
const TITLE_H: f32 = 24.0;
const TITLE_FONT: f32 = 12.0;
const PAD: f32 = 6.0;
const HANDLE: f32 = 14.0;
const GAP: f32 = 40.0;
const MIN_W: f32 = 160.0;
const MIN_H: f32 = 90.0;
/// Longest a terminal may show old cells while its PTY thread keeps the lock busy.
const MAX_STALE: Duration = Duration::from_millis(100);

/// Duration of the last canvas paint (cell copy + shaping + quads), read by the bench.
pub static PAINT_US: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World point shown at the canvas' top-left corner.
    pub x: f32,
    pub y: f32,
    pub zoom_idx: usize,
}

impl Camera {
    pub fn zoom(&self) -> f32 {
        ZOOM_STEPS[self.zoom_idx]
    }

    fn to_world(self, sx: f32, sy: f32) -> (f32, f32) {
        (sx / self.zoom() + self.x, sy / self.zoom() + self.y)
    }

    /// Changes zoom keeping the world point under (sx, sy) still.
    fn zoom_to(&mut self, idx: usize, sx: f32, sy: f32) {
        let (wx, wy) = self.to_world(sx, sy);
        self.zoom_idx = idx.min(ZOOM_STEPS.len() - 1);
        self.x = wx - sx / self.zoom();
        self.y = wy - sy / self.zoom();
    }

    fn step(&mut self, delta: i32, sx: f32, sy: f32) {
        let idx = (self.zoom_idx as i32 + delta).clamp(0, ZOOM_STEPS.len() as i32 - 1);
        self.zoom_to(idx as usize, sx, sy);
    }
}

pub struct Block {
    pub id: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub term: Terminal,
    snap: Rc<RefCell<Snapshot>>,
}

#[derive(Clone, Copy)]
enum Drag {
    Pan { last: (f32, f32) },
    Move { id: usize, dx: f32, dy: f32 },
    Resize { id: usize, dx: f32, dy: f32 },
}

pub type FrameHook = Box<dyn FnMut(&mut Board, &mut Window, &mut Context<Board>) -> bool>;

pub struct Board {
    pub cam: Camera,
    pub blocks: Vec<Block>,
    focused: Option<usize>,
    drag: Option<Drag>,
    focus_handle: FocusHandle,
    font: Font,
    /// Cell width in world units (advance of the mono font at FONT_SIZE).
    cell_w: f32,
    wake: Wake,
    /// IME pre-edit text (dead key, compose), drawn at the cursor until committed.
    marked: Option<String>,
    ime_cursor: Option<Bounds<Pixels>>,
    zoom_acc: f32,
    /// Called at the start of every render; returning false removes it (bench/shot drivers).
    pub on_frame: Option<FrameHook>,
    _wake_task: Task<()>,
}

impl Board {
    pub fn new(count: usize, grid: (u16, u16), window: &mut Window, cx: &mut Context<Self>) -> Self {
        let font = pick_font(window);
        let font_id = window.text_system().resolve_font(&font);
        let cell_w = window
            .text_system()
            .advance(font_id, px(FONT_SIZE), 'm')
            .map(|s| f32::from(s.width))
            .unwrap_or(FONT_SIZE * 0.6);

        let (wake, mut rx) = Wake::new();
        let wake_task = cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });

        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let mut board = Self {
            cam: Camera { x: -GAP, y: -GAP, zoom_idx: ZOOM_1 },
            blocks: Vec::new(),
            focused: None,
            drag: None,
            focus_handle,
            font,
            cell_w,
            wake,
            marked: None,
            ime_cursor: None,
            zoom_acc: 0.0,
            on_frame: None,
            _wake_task: wake_task,
        };
        let per_row = (count as f32).sqrt().ceil().max(1.0) as usize;
        let (cols, rows) = grid;
        let (w, h) = board.block_size(cols, rows);
        for i in 0..count {
            let (x, y) = ((i % per_row) as f32 * (w + GAP), (i / per_row) as f32 * (h + GAP));
            let term = Terminal::spawn(i as u64, cols, rows, board.cell_px(), board.wake.clone())
                .expect("failed to spawn shell in a PTY");
            board.blocks.push(Block { id: i, x, y, w, h, term, snap: Rc::default() });
        }
        board.focused = board.blocks.first().map(|b| b.id);
        board
    }

    fn block_size(&self, cols: u16, rows: u16) -> (f32, f32) {
        (cols as f32 * self.cell_w + 2.0 * PAD, rows as f32 * LINE_H + TITLE_H + 2.0 * PAD)
    }

    fn grid_size(&self, w: f32, h: f32) -> (u16, u16) {
        let cols = ((w - 2.0 * PAD) / self.cell_w).floor().max(2.0);
        let rows = ((h - TITLE_H - 2.0 * PAD) / LINE_H).floor().max(1.0);
        (cols as u16, rows as u16)
    }

    fn cell_px(&self) -> (u16, u16) {
        (self.cell_w.round() as u16, LINE_H as u16)
    }

    pub fn world_center(&self) -> (f32, f32) {
        let right = self.blocks.iter().map(|b| b.x + b.w).fold(0.0, f32::max);
        let bottom = self.blocks.iter().map(|b| b.y + b.h).fold(0.0, f32::max);
        (right / 2.0, bottom / 2.0)
    }

    fn focused_term(&self) -> Option<&Terminal> {
        let id = self.focused?;
        self.blocks.iter().find(|b| b.id == id).map(|b| &b.term)
    }

    fn block_mut(&mut self, id: usize) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    fn on_mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        let (sx, sy) = (f32::from(e.position.x), f32::from(e.position.y));
        let (wx, wy) = self.cam.to_world(sx, sy);
        let hit = self.blocks.iter().rposition(|b| {
            wx >= b.x && wx <= b.x + b.w && wy >= b.y && wy <= b.y + b.h
        });
        self.drag = match hit {
            None => Some(Drag::Pan { last: (sx, sy) }),
            Some(i) => {
                // Bring to front.
                let b = self.blocks.remove(i);
                let id = b.id;
                let drag = if wx > b.x + b.w - HANDLE && wy > b.y + b.h - HANDLE {
                    Some(Drag::Resize { id, dx: b.x + b.w - wx, dy: b.y + b.h - wy })
                } else if wy < b.y + TITLE_H {
                    Some(Drag::Move { id, dx: wx - b.x, dy: wy - b.y })
                } else {
                    None
                };
                self.blocks.push(b);
                self.focused = Some(id);
                drag
            }
        };
        cx.notify();
    }

    fn on_mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag else { return };
        if e.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        let (sx, sy) = (f32::from(e.position.x), f32::from(e.position.y));
        let (wx, wy) = self.cam.to_world(sx, sy);
        let cell = self.cell_px();
        match drag {
            Drag::Pan { last } => {
                self.cam.x -= (sx - last.0) / self.cam.zoom();
                self.cam.y -= (sy - last.1) / self.cam.zoom();
                self.drag = Some(Drag::Pan { last: (sx, sy) });
            }
            Drag::Move { id, dx, dy } => {
                if let Some(b) = self.block_mut(id) {
                    (b.x, b.y) = (wx - dx, wy - dy);
                }
            }
            Drag::Resize { id, dx, dy } => {
                let Some(b) = self.blocks.iter().find(|b| b.id == id) else { return };
                let (w, h) = ((wx + dx - b.x).max(MIN_W), (wy + dy - b.y).max(MIN_H));
                let (cols, rows) = self.grid_size(w, h);
                let b = self.block_mut(id).expect("block exists");
                (b.w, b.h) = (w, h);
                b.term.resize(cols, rows, cell);
            }
        }
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag = None;
    }

    fn on_scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let (sx, sy) = (f32::from(e.position.x), f32::from(e.position.y));
        if e.modifiers.control {
            self.zoom_acc += match e.delta {
                ScrollDelta::Lines(d) => d.y * 50.0,
                ScrollDelta::Pixels(d) => f32::from(d.y),
            };
            if self.zoom_acc.abs() >= 50.0 {
                self.cam.step(self.zoom_acc.signum() as i32, sx, sy);
                self.zoom_acc = 0.0;
            }
        } else {
            let d = e.delta.pixel_delta(px(LINE_H * 2.0));
            self.cam.x -= f32::from(d.x) / self.cam.zoom();
            self.cam.y -= f32::from(d.y) / self.cam.zoom();
        }
        cx.notify();
    }

    fn on_pinch(&mut self, e: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_acc += e.delta;
        if self.zoom_acc.abs() >= 0.15 {
            let (sx, sy) = (f32::from(e.position.x), f32::from(e.position.y));
            self.cam.step(self.zoom_acc.signum() as i32, sx, sy);
            self.zoom_acc = 0.0;
            cx.notify();
        }
    }

    fn on_key_down(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.focused_term() else { return };
        let k = &e.keystroke;
        let app_cursor = t.term.lock().mode().contains(TermMode::APP_CURSOR);
        let m = k.modifiers;
        if let Some(bytes) = term::encode_key(&k.key, k.key_char.as_deref(), m.control, m.alt, m.shift, app_cursor) {
            t.write(bytes);
            // Stops the platform from also feeding key_char to the text-input path.
            cx.stop_propagation();
        }
    }
}

impl Render for Board {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Re-arm the PTY wakeup only once per frame: output is coalesced to frame rate.
        self.wake.clear();
        if let Some(mut hook) = self.on_frame.take()
            && hook(self, window, cx)
        {
            self.on_frame = Some(hook);
        }

        let z = self.cam.zoom();
        let vp = window.viewport_size();
        let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
        // Only blocks intersecting the viewport are painted.
        let blocks: Vec<BlockFrame> = self
            .blocks
            .iter()
            .filter_map(|b| {
                let r = [(b.x - self.cam.x) * z, (b.y - self.cam.y) * z, b.w * z, b.h * z];
                let visible = r[0] < vw && r[1] < vh && r[0] + r[2] > 0.0 && r[1] + r[3] > 0.0;
                visible.then(|| BlockFrame {
                    rect: r,
                    term: b.term.term.clone(),
                    snap: b.snap.clone(),
                    title: format!("term {}  {}x{}", b.id, b.term.cols, b.term.rows).into(),
                    focused: self.focused == Some(b.id),
                })
            })
            .collect();
        let scene = Scene {
            blocks,
            zoom: z,
            font: self.font.clone(),
            cell_w: self.cell_w,
            marked: self.marked.clone(),
            board: cx.entity(),
            focus: self.focus_handle.clone(),
        };

        div()
            .size_full()
            .bg(rgb(0x0e0e12))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .on_pinch(cx.listener(Self::on_pinch))
            .child(canvas(|_, _, _| {}, move |bounds, _, window, cx| scene.paint(bounds, window, cx)).size_full())
    }
}

/// What the paint closure needs from one block, copied out at render time.
struct BlockFrame {
    rect: [f32; 4],
    term: std::sync::Arc<FairMutex<Term<Listener>>>,
    snap: Rc<RefCell<Snapshot>>,
    title: SharedString,
    focused: bool,
}

struct Scene {
    blocks: Vec<BlockFrame>,
    zoom: f32,
    font: Font,
    cell_w: f32,
    marked: Option<String>,
    board: Entity<Board>,
    focus: FocusHandle,
}

impl Scene {
    fn paint(self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let started = std::time::Instant::now();
        window.handle_input(&self.focus, ElementInputHandler::new(bounds, self.board.clone()), cx);
        let z = self.zoom;
        let o = bounds.origin;
        let mut ime_cursor = None;
        for b in &self.blocks {
            let [x, y, w, h] = b.rect;
            let r = Bounds::new(o + point(px(x), px(y)), size(px(w), px(h)));
            let radius = px(6.0 * z);
            let border = if b.focused { 0x6a9fb5 } else { 0x3a3a40 };
            window.paint_quad(quad(r, radius, hsla(DEFAULT_BG, 1.0), px(if b.focused { 2.0 } else { 1.0 }), rgb(border), BorderStyle::Solid));
            let title_r = Bounds::new(r.origin, size(r.size.width, px(TITLE_H * z)));
            let top = Corners { top_left: radius, top_right: radius, bottom_left: px(0.0), bottom_right: px(0.0) };
            window.paint_quad(quad(title_r, top, rgb(0x26262c), px(0.0), rgb(border), BorderStyle::Solid));
            let run = TextRun {
                len: b.title.len(),
                font: self.font.clone(),
                color: rgb(0xb0b0b8).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let title = window.text_system().shape_line(b.title.clone(), px(TITLE_FONT * z), &[run], None);
            let _ = title.paint(r.origin + point(px(8.0 * z), px(0.0)), px(TITLE_H * z), TextAlign::Left, None, window, cx);

            let body = Bounds::new(
                r.origin + point(px(PAD * z), px((TITLE_H + PAD) * z)),
                size(px(w - 2.0 * PAD * z), px(h - (TITLE_H + 2.0 * PAD) * z)),
            );
            window.with_content_mask(Some(ContentMask { bounds: body }), |window| {
                let cursor = paint_grid(&b.term, &b.snap, body.origin, z, &self.font, self.cell_w, b.focused, window, cx);
                if b.focused {
                    ime_cursor = cursor;
                    if let (Some(c), Some(text)) = (cursor, &self.marked) {
                        paint_marked(text, c, z, &self.font, window, cx);
                    }
                }
            });

            let handle = Bounds::new(
                r.origin + point(px(w - HANDLE * z), px(h - HANDLE * z)),
                size(px(HANDLE * z), px(HANDLE * z)),
            );
            window.paint_quad(fill(handle, rgb(border)).corner_radii(Corners { bottom_right: radius, ..Default::default() }));
        }
        self.board.update(cx, |board, _| board.ime_cursor = ime_cursor);
        PAINT_US.store(started.elapsed().as_micros() as u64, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct Row {
    text: String,
    runs: Vec<(usize, Hsla, bool)>,
    bgs: Vec<(usize, usize, Rgb)>,
}

impl Row {
    fn push(&mut self, c: char, fg: Hsla, bold: bool) {
        self.text.push(c);
        match self.runs.last_mut() {
            Some((len, f, b)) if *f == fg && *b == bold => *len += c.len_utf8(),
            _ => self.runs.push((c.len_utf8(), fg, bold)),
        }
    }

    fn push_bg(&mut self, col: usize, bg: Rgb) {
        match self.bgs.last_mut() {
            Some((start, len, b)) if *b == bg && *start + *len == col => *len += 1,
            _ => self.bgs.push((col, 1, bg)),
        }
    }

    /// Trailing blanks are not shaped.
    fn trim(&mut self) {
        let mut extra = self.text.len() - self.text.trim_end_matches(' ').len();
        self.text.truncate(self.text.len() - extra);
        while extra > 0 {
            let last = self.runs.last_mut().expect("runs cover text");
            let n = last.0.min(extra);
            last.0 -= n;
            extra -= n;
            if last.0 == 0 {
                self.runs.pop();
            }
        }
    }
}

/// Cells copied out of the `Term`, so painting does not wait on a busy PTY thread.
#[derive(Default)]
pub struct Snapshot {
    rows: Vec<Row>,
    cursor: Option<(i32, usize)>,
    taken: Option<Instant>,
}

impl Snapshot {
    /// Re-reads the grid unless the PTY thread holds the lock (e.g. under `yes`), in which case
    /// the previous frame's cells are reused; past MAX_STALE it waits for the lock.
    /// Returns false when it kept stale cells.
    fn refresh(&mut self, term: &FairMutex<Term<Listener>>) -> bool {
        let stale = self.taken.is_none_or(|t| t.elapsed() > MAX_STALE);
        let term = match term.try_lock_unfair() {
            Some(term) => term,
            None if stale => term.lock(),
            None => return false,
        };
        let content = term.renderable_content();
        let off = content.display_offset as i32;
        self.cursor = (content.cursor.shape != CursorShape::Hidden)
            .then(|| (content.cursor.point.line.0 + off, content.cursor.point.column.0));
        self.rows.clear();
        for item in content.display_iter {
            let row = (item.point.line.0 + off) as usize;
            if self.rows.len() <= row {
                self.rows.resize_with(row + 1, Row::default);
            }
            let cell = item.cell;
            let mut fg = term::resolve(cell.fg, content.colors);
            let mut bg = term::resolve(cell.bg, content.colors);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags.contains(Flags::HIDDEN) {
                fg = bg;
            }
            if bg != DEFAULT_BG {
                self.rows[row].push_bg(item.point.column.0, bg);
            }
            let alpha = if cell.flags.contains(Flags::DIM) { 0.66 } else { 1.0 };
            // A wide char's spacer becomes a blank so later glyphs stay on their columns.
            let c = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.c < ' ' { ' ' } else { cell.c };
            self.rows[row].push(c, hsla(fg, alpha), cell.flags.contains(Flags::BOLD));
        }
        drop(term);
        self.rows.iter_mut().for_each(Row::trim);
        self.taken = Some(Instant::now());
        true
    }
}

/// Paints the terminal grid; returns the cursor cell bounds (for the IME popup).
#[allow(clippy::too_many_arguments)]
fn paint_grid(
    term: &FairMutex<Term<Listener>>,
    snap: &RefCell<Snapshot>,
    origin: Point<Pixels>,
    z: f32,
    font: &Font,
    cell_w: f32,
    focused: bool,
    window: &mut Window,
    cx: &mut App,
) -> Option<Bounds<Pixels>> {
    let mut snap = snap.borrow_mut();
    if !snap.refresh(term) {
        // Output is still pending: try again next frame.
        window.request_animation_frame();
    }
    let (cw, lh) = (cell_w * z, LINE_H * z);
    let bold = Font { weight: FontWeight::BOLD, ..font.clone() };
    for (i, row) in snap.rows.iter().enumerate() {
        let y = origin.y + px(lh * i as f32);
        for (start, len, bg) in &row.bgs {
            let r = Bounds::new(point(origin.x + px(cw * *start as f32), y), size(px(cw * *len as f32), px(lh)));
            window.paint_quad(fill(r, hsla(*bg, 1.0)));
        }
        if row.text.is_empty() {
            continue;
        }
        let runs: Vec<TextRun> = row
            .runs
            .iter()
            .map(|&(len, color, is_bold)| TextRun {
                len,
                font: if is_bold { bold.clone() } else { font.clone() },
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            })
            .collect();
        // force_width pins glyph n to column n, like a terminal grid.
        let line = window.text_system().shape_line(row.text.clone().into(), px(FONT_SIZE * z), &runs, Some(px(cw)));
        let _ = line.paint(point(origin.x, y), px(lh), TextAlign::Left, None, window, cx);
    }

    let (line, col) = snap.cursor?;
    let r = Bounds::new(
        point(origin.x + px(cw * col as f32), origin.y + px(lh * line as f32)),
        size(px(cw), px(lh)),
    );
    if focused {
        window.paint_quad(fill(r, hsla(DEFAULT_FG, 0.6)));
    } else {
        window.paint_quad(outline(r, hsla(DEFAULT_FG, 0.6), BorderStyle::Solid));
    }
    Some(r)
}

fn paint_marked(text: &str, at: Bounds<Pixels>, z: f32, font: &Font, window: &mut Window, cx: &mut App) {
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: rgb(0xfeca88).into(),
        background_color: Some(rgb(0x26262c).into()),
        underline: Some(gpui::UnderlineStyle { thickness: px(1.0), color: None, wavy: false }),
        strikethrough: None,
    };
    let line = window.text_system().shape_line(text.to_string().into(), px(FONT_SIZE * z), &[run], None);
    let _ = line.paint_background(at.origin, at.size.height, TextAlign::Left, None, window, cx);
    let _ = line.paint(at.origin, at.size.height, TextAlign::Left, None, window, cx);
}

fn hsla(c: Rgb, a: f32) -> Hsla {
    Rgba { r: c.r as f32 / 255.0, g: c.g as f32 / 255.0, b: c.b as f32 / 255.0, a }.into()
}

fn pick_font(window: &Window) -> Font {
    let names = window.text_system().all_font_names();
    let wanted = std::env::var("TEMPLUM_FONT").ok();
    let candidates = wanted.iter().map(String::as_str).chain([
        "JetBrainsMono Nerd Font Mono",
        "JetBrainsMono Nerd Font",
        "JetBrains Mono",
        "DejaVu Sans Mono",
        "Liberation Mono",
        "Adwaita Mono",
    ]);
    for family in candidates {
        if names.iter().any(|n| n == family) {
            return font(family.to_string());
        }
    }
    font("monospace")
}

/// Text input path (IME, dead keys, composed chars): GPUI calls this for any key that
/// `on_key_down` did not consume.
impl EntityInputHandler for Board {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|m| 0..m.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        if let Some(t) = self.focused_term() {
            t.write(text.as_bytes().to_vec());
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = (!text.is_empty()).then(|| text.to_string());
        cx.notify();
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        self.ime_cursor
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let mut cam = Camera { x: 100.0, y: -50.0, zoom_idx: ZOOM_1 };
        let before = cam.to_world(300.0, 200.0);
        cam.step(3, 300.0, 200.0);
        assert_eq!(cam.zoom(), 1.5);
        let after = cam.to_world(300.0, 200.0);
        assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);
        cam.step(-100, 0.0, 0.0);
        assert_eq!(cam.zoom(), ZOOM_STEPS[0]);
    }

    #[test]
    fn trim_keeps_runs_in_sync() {
        let mut row = Row::default();
        let (a, b) = (hsla(DEFAULT_FG, 1.0), hsla(DEFAULT_BG, 1.0));
        "ab".chars().for_each(|c| row.push(c, a, false));
        "  ".chars().for_each(|c| row.push(c, b, false));
        row.trim();
        assert_eq!(row.text, "ab");
        assert_eq!(row.runs.iter().map(|r| r.0).sum::<usize>(), 2);
    }
}
