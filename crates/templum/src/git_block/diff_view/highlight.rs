//! Language colors for the panel's hunks: one highlighter per language, each hunk's old and new
//! sides run through it on first draw, the styles kept per hunk until the diff changes. And
//! the changed words of each removed line paired with an added one.
//! See docs/specs/git.md, section 2, Colors per hunk.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;

use gpui_kit::Hsla;
use gpui_kit::component::Rope;
use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};
use templum_git::{Kind, changed};

use super::rows::Shown;
use super::select::display;
use crate::editor::lang_of;

/// One hunk's colors: per line, in the hunk's order, the colored byte ranges of its display
/// text; and the changed words of a removed line paired with an added one.
#[derive(Default)]
pub(crate) struct HunkColors {
    pub lines: Vec<Vec<(Range<usize>, Hsla)>>,
    pub marks: Vec<Vec<Range<usize>>>,
}

#[derive(Default)]
pub(crate) struct Highlights {
    /// One highlighter per language: building one is a tree-sitter parser and its queries.
    by_lang: HashMap<&'static str, SyntaxHighlighter>,
    /// By (file, hunk), until the diff changes.
    hunks: HashMap<(usize, usize), Rc<HunkColors>>,
}

impl Highlights {
    /// The diff changed: every hunk is colored again on its next draw.
    pub fn clear(&mut self) {
        self.hunks.clear();
    }

    /// The colors of hunk `hunk` of file `file` (`shown`), built the first time.
    pub fn of(
        &mut self,
        file: usize,
        hunk: usize,
        shown: &Shown,
        theme: &HighlightTheme,
    ) -> Rc<HunkColors> {
        if let Some(c) = self.hunks.get(&(file, hunk)) {
            return c.clone();
        }
        let lang = lang_of(Path::new(shown.diff.path()));
        let hl = self
            .by_lang
            .entry(lang)
            .or_insert_with(|| SyntaxHighlighter::new(lang));
        let h = &shown.diff.hunks[hunk];
        let texts: Vec<String> = h.lines.iter().map(|l| display(&l.text)).collect();
        let mut colors = HunkColors {
            lines: vec![Vec::new(); h.lines.len()],
            marks: vec![Vec::new(); h.lines.len()],
        };
        // The old side is the context and removed lines; the new side the context and added
        // ones. Context lines take the new side's colors.
        for side in [Kind::Removed, Kind::Added] {
            let members: Vec<usize> = (0..h.lines.len())
                .filter(|&i| matches!(h.lines[i].kind, Kind::Context) || h.lines[i].kind == side)
                .collect();
            let fragment = members
                .iter()
                .map(|&i| texts[i].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let rope = Rope::from_str(&fragment);
            hl.update(None, &rope, None);
            let styles = hl.styles(&(0..fragment.len()), theme);
            let (mut start, mut j) = (0, 0);
            for &i in &members {
                let end = start + texts[i].len();
                let mut line = Vec::new();
                // ponytail: a style that spans lines (a long string) is seen by each line it
                // crosses; a style starting inside such a span is skipped for that line.
                while j < styles.len() && styles[j].0.start < end {
                    let (r, style) = &styles[j];
                    let (a, b) = (r.start.max(start), r.end.min(end));
                    if a < b
                        && let Some(color) = style.color
                    {
                        line.push((a - start..b - start, color));
                    }
                    if r.end <= end {
                        j += 1;
                    } else {
                        break;
                    }
                }
                if side == Kind::Added || h.lines[i].kind == Kind::Removed {
                    colors.lines[i] = line;
                }
                start = end + 1;
            }
        }
        for (r, a) in h.pairs() {
            let (old, new) = changed(&texts[r], &texts[a]);
            colors.marks[r] = old;
            colors.marks[a] = new;
        }
        let colors = Rc::new(colors);
        self.hunks.insert((file, hunk), colors.clone());
        colors
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;
    use templum_git::parse_diff;

    const RUST: &str = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n\
@@ -1,3 +1,3 @@\n fn one() {}\n-let x = foo(1);\n+let x = bar(1);\n \"a string\"\n";

    #[test]
    fn a_rust_hunk_gets_keyword_colors_on_both_sides_and_word_marks() {
        let shown = Shown {
            staged: false,
            diff: parse_diff(RUST, usize::MAX).pop().unwrap(),
        };
        let theme = HighlightTheme::default_dark();
        let mut hl = Highlights::default();
        let c = hl.of(0, 0, &shown, &theme);
        assert_eq!(c.lines.len(), 4);
        // "fn" (0..2) of the context line is colored, from the new side.
        assert!(
            c.lines[0].iter().any(|(r, _)| r.start == 0 && r.end == 2),
            "{:?}",
            c.lines[0]
        );
        // "let" on the removed line (old side) and on the added one (new side).
        assert!(
            c.lines[1].iter().any(|(r, _)| *r == (0..3)),
            "{:?}",
            c.lines[1]
        );
        assert!(
            c.lines[2].iter().any(|(r, _)| *r == (0..3)),
            "{:?}",
            c.lines[2]
        );
        assert!(!c.lines[3].is_empty(), "the string");
        assert_eq!(c.marks[1], [8..11], "foo");
        assert_eq!(c.marks[2], [8..11], "bar");
        assert!(c.marks[0].is_empty() && c.marks[3].is_empty());
        // The second ask is the same Rc.
        let again = hl.of(0, 0, &shown, &theme);
        assert!(Rc::ptr_eq(&c, &again));
        hl.clear();
        assert!(!Rc::ptr_eq(&c, &hl.of(0, 0, &shown, &theme)));
    }

    #[test]
    fn an_unknown_language_colors_nothing_but_still_marks_words() {
        let text = "diff --git a/notes.xyz b/notes.xyz\n--- a/notes.xyz\n+++ b/notes.xyz\n\
@@ -1 +1 @@\n-hello world\r\n+hello there\r\n";
        let shown = Shown {
            staged: false,
            diff: parse_diff(text, usize::MAX).pop().unwrap(),
        };
        let theme = HighlightTheme::default_light();
        let c = Highlights::default().of(0, 0, &shown, &theme);
        assert!(c.lines.iter().all(Vec::is_empty));
        assert_eq!(c.marks[0], [6..11], "on the display text, without the \\r");
        assert_eq!(c.marks[1], [6..11]);
    }
}
