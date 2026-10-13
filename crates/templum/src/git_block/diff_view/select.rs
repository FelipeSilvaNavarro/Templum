//! Selecting text in the panel. The rows show each line's display text: without a trailing
//! '\r', tabs as 4 spaces.

use std::ops::Range;

use super::rows::{Row, Shown};

/// What a tab is drawn as.
pub(crate) const TAB: &str = "    ";

/// A line as it is drawn: without a trailing '\r' (a CRLF file), tabs as 4 spaces.
pub(crate) fn display(text: &str) -> String {
    text.strip_suffix('\r').unwrap_or(text).replace('\t', TAB)
}

/// A place in the panel: a row, and a byte of its display text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Pos {
    pub row: usize,
    pub col: usize,
}

/// From where the mouse went down to where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub anchor: Pos,
    pub head: Pos,
}

impl Selection {
    /// The start and the end, whichever way it was dragged.
    pub fn ordered(&self) -> (Pos, Pos) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selected bytes of row `row`, whose display text is `len` bytes; None when the
    /// row isn't in the selection. A row in the middle is selected whole, an empty one too.
    pub fn range_in(&self, row: usize, len: usize) -> Option<Range<usize>> {
        let (a, b) = self.ordered();
        if row < a.row || row > b.row {
            return None;
        }
        let start = if row == a.row { a.col.min(len) } else { 0 };
        let end = if row == b.row { b.col.min(len) } else { len };
        Some(start..end)
    }
}

/// The bytes of `text` that `display(text)[range]` came from: a tab is 4 display bytes.
pub(crate) fn to_source(text: &str, range: Range<usize>) -> Range<usize> {
    let text = text.strip_suffix('\r').unwrap_or(text);
    let map = |target: usize| -> usize {
        let mut shown = 0;
        for (i, c) in text.char_indices() {
            if shown >= target {
                return i;
            }
            shown += if c == '\t' { TAB.len() } else { c.len_utf8() };
        }
        text.len()
    };
    map(range.start)..map(range.end)
}

/// The word around byte `col` of `display` (letters, digits and '_'), else the one character
/// there, else `col..col`.
pub(crate) fn word_at(display: &str, col: usize) -> Range<usize> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    // A column from the last frame can fall inside a character of the current text.
    let mut col = col.min(display.len());
    while !display.is_char_boundary(col) {
        col -= 1;
    }
    let start = display[..col]
        .char_indices()
        .rev()
        .take_while(|(_, c)| word(*c))
        .last()
        .map_or(col, |(i, _)| i);
    let end = display[col..]
        .char_indices()
        .take_while(|(_, c)| word(*c))
        .last()
        .map_or(col, |(i, c)| col + i + c.len_utf8());
    if start < end {
        start..end
    } else {
        let next = display[col..].chars().next().map_or(0, char::len_utf8);
        col..col + next
    }
}

/// The selected text: the line rows in the selection, their source text, joined with '\n'.
pub(crate) fn selected_text(rows: &[Row], files: &[Shown], sel: &Selection) -> String {
    let (a, b) = sel.ordered();
    let mut out: Vec<String> = Vec::new();
    for (ix, row) in rows.iter().enumerate().take(b.row + 1).skip(a.row) {
        let Row::Line { file, hunk, line } = row else {
            continue;
        };
        let text = &files[*file].diff.hunks[*hunk].lines[*line].text;
        let shown = display(text);
        let Some(r) = sel.range_in(ix, shown.len()) else {
            continue;
        };
        let r = to_source(text, r);
        out.push(text.strip_suffix('\r').unwrap_or(text)[r].to_string());
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_drops_the_cr_and_expands_tabs() {
        assert_eq!(display("a\tb\r"), "a    b");
        assert_eq!(display("plain"), "plain");
        assert_eq!(display("\r"), "");
    }

    #[test]
    fn to_source_maps_display_bytes_back_through_tabs() {
        assert_eq!(to_source("a\tb", 0..6), 0..3);
        assert_eq!(to_source("a\tb", 5..6), 2..3, "b");
        assert_eq!(to_source("a\tb", 1..5), 1..2, "the tab");
        assert_eq!(
            to_source("x\r", 0..1),
            0..1,
            "the \\r is not in the display text"
        );
        assert_eq!(to_source("café", 0..5), 0..5);
    }

    #[test]
    fn word_at_never_slices_inside_a_character() {
        // A column from the last frame's shaped line can fall inside a character of the
        // current text (the diff changed in between): it is taken back to the boundary.
        assert_eq!(word_at("é x", 1), 0..2);
        assert_eq!(word_at("日本", 4), 0..6);
        assert_eq!(word_at("a", 9), 0..1, "past the end");
    }

    #[test]
    fn word_at_takes_the_word_or_one_character() {
        assert_eq!(word_at("let x_1 = f(a);", 5), 4..7);
        assert_eq!(word_at("let x_1 = f(a);", 4), 4..7, "at its start");
        assert_eq!(word_at("let x_1 = f(a);", 7), 4..7, "at its end");
        assert_eq!(word_at("let x_1 = f(a);", 8), 8..9, "the space");
        assert_eq!(word_at("ab", 2), 0..2, "at the end of the word");
        assert_eq!(word_at("", 0), 0..0, "nothing there");
        assert_eq!(word_at("日本語 x", 3), 0..9);
    }

    #[test]
    fn a_selection_is_ordered_and_gives_each_rows_range() {
        let sel = Selection {
            anchor: Pos { row: 5, col: 2 },
            head: Pos { row: 3, col: 1 },
        };
        assert_eq!(
            sel.ordered(),
            (Pos { row: 3, col: 1 }, Pos { row: 5, col: 2 })
        );
        assert_eq!(sel.range_in(2, 10), None);
        assert_eq!(sel.range_in(3, 10), Some(1..10));
        assert_eq!(
            sel.range_in(4, 0),
            Some(0..0),
            "an empty line in the middle"
        );
        assert_eq!(sel.range_in(5, 10), Some(0..2));
        assert_eq!(sel.range_in(6, 10), None);
        let one = Selection {
            anchor: Pos { row: 1, col: 4 },
            head: Pos { row: 1, col: 2 },
        };
        assert_eq!(one.range_in(1, 10), Some(2..4));
        assert!(!one.is_empty());
        assert!(
            Selection {
                anchor: Pos { row: 1, col: 2 },
                head: Pos { row: 1, col: 2 }
            }
            .is_empty()
        );
    }

    #[test]
    fn selected_text_joins_the_line_rows_source_text() {
        use super::super::rows::{Row, Shown, rows};
        use std::collections::HashSet;
        let text = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n a\tz\r\n-b\n+c\n";
        let files = [Shown {
            staged: false,
            diff: templum_git::parse_diff(text, usize::MAX).pop().unwrap(),
        }];
        let rows = rows(&files, &HashSet::new(), &HashSet::new());
        // Section, File, Hunk, then the three lines at 3, 4, 5.
        assert!(matches!(rows[3], Row::Line { line: 0, .. }));
        let all = Selection {
            anchor: Pos { row: 0, col: 0 },
            head: Pos { row: 5, col: 1 },
        };
        assert_eq!(selected_text(&rows, &files, &all), "a\tz\nb\nc");
        let part = Selection {
            anchor: Pos { row: 3, col: 5 },
            head: Pos { row: 4, col: 1 },
        };
        assert_eq!(selected_text(&rows, &files, &part), "z\nb");
    }
}
