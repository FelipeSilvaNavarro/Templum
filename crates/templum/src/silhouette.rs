//! Editors and notes below Full: their text as one bar per word, on the rows gpui-kit laid the
//! words out on in the body's last Full frame (its word map). See
//! docs/specs/level-of-detail.md, section 3.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::input::EditorState;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, Entity, Hsla, Pixels, Point, SharedString, TestSupportExt,
    canvas, div, fill, point, px, size,
};

use crate::grid::bar_quad;

/// gpui-kit's fixed pixels around an editor tab's text: the code editor's padding, left
/// (`input_px().min(6)`) and top (`input_py()`, 8 at size Medium; gpui-component-0.7.3
/// src/input/input.rs:588-602), and the margin right of the line numbers (gpui-base-0.7.3
/// src/input/base/element.rs:51).
const TAB_LEFT: f32 = 6.0;
const TAB_TOP: f32 = 8.0;
const NUMBERS_MARGIN: f32 = 6.0;

/// What a word map was taken of: when any of it changes, so may the rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Taken {
    /// The text's length in bytes (a reload from disk drops the map itself: `Tab::reload`).
    pub len: usize,
    /// The block's size, world units.
    pub size: (f32, f32),
    pub font: SharedString,
    /// Soft wrap, and an editor's file tree shown (it narrows the text).
    pub wrap: bool,
    pub tree: bool,
}

/// A run of non-blank characters, or its part on one row where gpui-kit wrapped it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Word {
    /// Its bytes in the text: the alignment tests compare its bar with its letters by it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub range: Range<usize>,
    /// Its row from the text's first, wrapped rows counted.
    pub row: usize,
    /// Its left edge from the text's left edge (the scroll not counted), and its width: world
    /// units.
    pub x: f32,
    pub w: f32,
}

/// The words gpui-kit laid out in a body's last Full frame, kept to be drawn as bars.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WordMap {
    pub taken: Taken,
    /// The scroll when it was taken, in rows: down, and across. gpui-kit's scroll is in pixels
    /// of the rows it laid out, and `editor::keep_rows` rescales it by the rows: rows keep it.
    pub scroll: (f32, f32),
    /// The line numbers' digits' width, world units: an editor tab's text starts past them. (A
    /// note's text starts where its box puts it: `note_origin`.)
    pub numbers: f32,
    pub words: Vec<Word>,
}

impl WordMap {
    /// The words of `state`'s last layout, drawn at zoom `z`: each run of non-blank characters
    /// of each line, from the first line on screen until gpui-kit has no bounds for one (past
    /// what it laid out). None before its first layout.
    pub fn take(state: &EditorState, z: f32, taken: Taken) -> Option<WordMap> {
        let lh = f32::from(state.line_height()?);
        // The text's laid-out origin, scroll included: `range_to_bounds` is relative to it.
        let origin = state.text_bounds()?.origin;
        let first = state.visible_row_range()?.start;
        let scroll = state.scroll_offset();
        let text = state.text().to_string();
        let start = text
            .split('\n')
            .take(first)
            .map(|line| line.len() + 1)
            .sum::<usize>()
            .min(text.len());
        // The text's left edge, past the line numbers: where a line's first character is.
        let left = state
            .range_to_bounds(&(start..start))
            .map_or(0.0, |b| f32::from(b.origin.x - origin.x));
        // Only the words on screen, and as far again past its right and bottom edges (a map
        // may be drawn at a zoom that shows a little more): each word costs gpui-kit a walk
        // from its line's start, and a minified file's line has thousands.
        let view = state.input_bounds();
        let right = f32::from(view.right() + view.size.width - origin.x);
        let bottom = f32::from(view.bottom() + view.size.height - origin.y) / lh;
        let mut words = Vec::new();
        let mut at = start;
        'lines: for line in text[start..].split('\n') {
            for run in runs(line) {
                let range = at + run.start..at + run.end;
                let Some(parts) = place(state, &text[range.clone()], range.clone(), origin, lh)
                else {
                    break 'lines;
                };
                if let Some(&(row, x0, _)) = parts.first() {
                    if row as f32 > bottom {
                        break 'lines;
                    }
                    // The rest of this line is further right.
                    if x0 > right {
                        break;
                    }
                }
                words.extend(parts.into_iter().map(|(row, x0, x1)| Word {
                    range: range.clone(),
                    row,
                    x: (x0 - left) / z,
                    w: (x1 - x0) / z,
                }));
            }
            at += line.len() + 1;
        }
        Some(WordMap {
            taken,
            scroll: (-f32::from(scroll.y) / lh, -f32::from(scroll.x) / lh),
            numbers: (left - NUMBERS_MARGIN) / z,
            words,
        })
    }

    /// Each word's cell at zoom `s`: `(left, top, width, height)`, pixels from the body's box,
    /// its text starting at `origin`. Rows are gpui-kit's, `1.5 × font × s` rounded to whole
    /// pixels as GPUI rounds a line's height (gpui-pre-0.3.9 src/style.rs:553-555,
    /// gpui-component-0.7.3 src/input/editor.rs:14).
    pub fn cells(
        &self,
        origin: (f32, f32),
        font: f32,
        s: f32,
    ) -> impl Iterator<Item = (f32, f32, f32, f32)> + '_ {
        let pitch = (1.5 * font * s).round();
        self.words.iter().map(move |w| {
            (
                origin.0 + w.x * s - self.scroll.1 * pitch,
                origin.1 + (w.row as f32 - self.scroll.0) * pitch,
                w.w * s,
                pitch,
            )
        })
    }
}

/// The byte ranges of `line`'s runs of non-blank characters.
fn runs(line: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                found.push(s..i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        found.push(s..line.len());
    }
    found
}

/// Where gpui-kit put `word` (`range` of the text): `(row, left, right)`, pixels from `origin`,
/// one per row it is on. Empty on a hidden (folded) line: gpui-kit puts its words at the next
/// shown line's start, with no width. None past what it laid out.
fn place(
    state: &EditorState,
    word: &str,
    range: Range<usize>,
    origin: Point<Pixels>,
    lh: f32,
) -> Option<Vec<(usize, f32, f32)>> {
    let at = |b: Bounds<Pixels>| {
        let row = (f32::from(b.origin.y - origin.y) / lh).round().max(0.0) as usize;
        (row, f32::from(b.origin.x - origin.x))
    };
    let b = state.range_to_bounds(&range)?;
    if f32::from(b.size.height) < 1.5 * lh {
        let (row, x) = at(b);
        let w = f32::from(b.size.width);
        return Some(if w > 0.0 {
            vec![(row, x, x + w)]
        } else {
            Vec::new()
        });
    }
    // Wrapped inside the word: a part per row. In gpui-kit's bounds a row's last character ends
    // on the next row: it is as wide as the one before it.
    let mut parts: Vec<(usize, f32, f32)> = Vec::new();
    let mut advance = 0.0;
    for (i, c) in word.char_indices() {
        let start = range.start + i;
        let b = state.range_to_bounds(&(start..start + c.len_utf8()))?;
        if f32::from(b.size.height) < 1.5 * lh {
            advance = f32::from(b.size.width);
        }
        let (row, x) = at(b);
        match parts.last_mut() {
            Some((r, _, right)) if *r == row => *right = x + advance,
            _ => parts.push((row, x, x + advance)),
        }
    }
    Some(parts)
}

/// Where a note's text starts in its box at zoom `s`: its box makes up for gpui-kit's fixed
/// parts, so they all scale (`note.rs`).
pub(crate) fn note_origin(_: &WordMap, s: f32) -> (f32, f32) {
    (crate::note::TEXT_LEFT * s, crate::note::FIXED_TOP * s)
}

/// Where an editor tab's text starts in its area at zoom `s`: gpui-kit's fixed padding, the line
/// numbers (their digits scale with the text) and their fixed margin. Editor tabs have no fold
/// gutter (`editor.rs`, `folding(false)`).
pub(crate) fn tab_origin(map: &WordMap, s: f32) -> (f32, f32) {
    (TAB_LEFT + map.numbers * s + NUMBERS_MARGIN, TAB_TOP)
}

/// What a note or an editor tab keeps for drawing below Full.
#[derive(Default)]
pub(crate) struct Far {
    /// The zoom and the `Taken` of the last frame its editor was drawn Full at: gpui-kit's last
    /// layout is of them.
    pub(crate) full: Option<(f32, Taken)>,
    pub(crate) map: Option<Rc<WordMap>>,
}

/// A note's or an editor tab's text below Full: its word map's bars, in an element (`id`) filling
/// the text's area. The map is taken from gpui-kit's last layout on the first frame after a Full
/// one. When it is stale (another text, size, font, wrap or tree) or missing (a canvas restored
/// far away), `full` (the editor as Full draws it) is drawn once more, unseen but laid out,
/// under the old bars so nothing blinks, and the next frame takes the map.
#[allow(clippy::too_many_arguments)]
pub(crate) fn body(
    id: ElementId,
    editor: &Entity<EditorState>,
    far: &RefCell<Far>,
    taken: Taken,
    z: f32,
    font: f32,
    origin: fn(&WordMap, f32) -> (f32, f32),
    color: Hsla,
    full: impl FnOnce(&mut App) -> AnyElement,
    cx: &mut App,
) -> AnyElement {
    let mut far = far.borrow_mut();
    if let Some((at, of)) = far.full.take()
        && of == taken
    {
        far.map = WordMap::take(editor.read(cx), at, taken.clone()).map(Rc::new);
    }
    let stale = far.map.as_ref().is_none_or(|m| m.taken != taken);
    let unseen = if stale {
        far.full = Some((z, taken));
        Some(full(cx))
    } else {
        None
    };
    let map = far.map.clone();
    drop(far);
    div()
        .id(id)
        .relative()
        .size_full()
        // GPUI lays out and paints what has opacity 0 (gpui-pre-0.3.9 src/window.rs:4212-4228).
        .children(unseen.map(|el| div().absolute().size_full().opacity(0.0).child(el)))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    // The next frame takes the map of what the unseen editor laid out.
                    if stale {
                        window.request_animation_frame();
                    }
                    let Some(map) = map else {
                        return;
                    };
                    for (x, y, w, h) in map.cells(origin(&map, z), font, z) {
                        let cell = Bounds::new(
                            point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                            size(px(w), px(h)),
                        );
                        window.paint_quad(fill(bar_quad(cell), color));
                    }
                },
            )
            .absolute()
            .size_full(),
        )
        .test_support()
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(words: Vec<Word>, scroll: (f32, f32)) -> WordMap {
        WordMap {
            taken: Taken {
                len: 0,
                size: (0.0, 0.0),
                font: "".into(),
                wrap: false,
                tree: false,
            },
            scroll,
            numbers: 0.0,
            words,
        }
    }

    fn word(row: usize, x: f32, w: f32) -> Word {
        Word {
            range: 0..1,
            row,
            x,
            w,
        }
    }

    fn close(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
        [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3)]
            .iter()
            .all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn rows_are_gpui_kits_rounded_line_heights() {
        let m = map(vec![word(0, 0.0, 10.0), word(3, 20.0, 8.0)], (0.0, 0.0));
        let cells: Vec<_> = m.cells((0.0, 0.0), 13.0, 0.33).collect();
        // 1.5 × 13 × 0.33 = 6.435: rows 6 px apart; words scale with the zoom.
        assert!(close(cells[0], (0.0, 0.0, 3.3, 6.0)), "{:?}", cells[0]);
        assert!(close(cells[1], (6.6, 18.0, 2.64, 6.0)), "{:?}", cells[1]);
    }

    #[test]
    fn the_scroll_moves_every_row_by_its_share_of_a_row() {
        // Taken scrolled 2.5 rows down and 1 row across. At 100%, rows of 19.5 → 20 px:
        let m = map(vec![word(3, 10.0, 4.0)], (2.5, 1.0));
        let cell = m.cells((5.0, 7.0), 13.0, 1.0).next().unwrap();
        assert!(
            close(cell, (5.0 + 10.0 - 20.0, 7.0 + 0.5 * 20.0, 4.0, 20.0)),
            "{cell:?}"
        );
        // At 25%, rows of 4.875 → 5 px: the same share of a row above the top.
        let cell = m.cells((0.0, 0.0), 13.0, 0.25).next().unwrap();
        assert!(close(cell, (2.5 - 5.0, 0.5 * 5.0, 1.0, 5.0)), "{cell:?}");
    }

    #[test]
    fn a_notes_text_starts_where_its_box_puts_it_at_every_zoom() {
        let m = map(Vec::new(), (0.0, 0.0));
        assert_eq!(note_origin(&m, 1.0), (14.0, 8.0));
        assert_eq!(note_origin(&m, 0.25), (3.5, 2.0));
    }

    #[test]
    fn words_are_runs_of_non_blank_characters() {
        assert_eq!(runs("  let x = 1;"), vec![2..5, 6..7, 8..9, 10..12]);
        assert_eq!(runs("é\tab"), vec![0..2, 3..5]);
        assert_eq!(
            runs("a\u{3000}b"),
            vec![0..1, 4..5],
            "an ideographic space is blank"
        );
        assert!(runs("   ").is_empty());
    }

    #[test]
    fn an_editor_tabs_text_starts_past_its_line_numbers() {
        // Digits 0.6 em wide at 13 px: three of them (a file under 1,000 lines), or four.
        let three = WordMap {
            numbers: 3.0 * 0.6 * 13.0,
            ..map(Vec::new(), (0.0, 0.0))
        };
        let four = WordMap {
            numbers: 4.0 * 0.6 * 13.0,
            ..map(Vec::new(), (0.0, 0.0))
        };
        assert!(
            (tab_origin(&three, 0.25).0 - 17.85).abs() < 1e-4,
            "6 + 5.85 + 6"
        );
        assert!(
            (tab_origin(&four, 0.25).0 - 19.8).abs() < 1e-4,
            "6 + 7.8 + 6"
        );
        assert!(
            (tab_origin(&four, 1.0).0 - 43.2).abs() < 1e-4,
            "6 + 31.2 + 6"
        );
        assert_eq!(
            tab_origin(&three, 0.5).1,
            8.0,
            "the top padding doesn't scale"
        );
    }
}
