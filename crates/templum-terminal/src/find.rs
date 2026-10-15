//! Finding text in a terminal's history: the match found is selected and scrolled into view.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Direction, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::term::viewport_to_point;

use crate::SharedTerm;

/// A search for the text typed, read literally; without a capital it matches either case.
pub struct Find(RegexSearch);

impl Find {
    /// None for an empty query.
    pub fn new(query: &str) -> Option<Find> {
        if query.is_empty() {
            return None;
        }
        RegexSearch::new(&literal(query)).ok().map(Find)
    }
}

/// Where a search goes from the selection (or, with none, from the bottom of the screen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toward {
    /// The nearest match starting at or above the selection's end: typing more keeps it.
    Here,
    /// The next one up, older.
    Older,
    /// The next one down, newer.
    Newer,
}

impl SharedTerm {
    /// Selects the match of `find` `toward` the selection, wrapping around the history, and
    /// scrolls it into view. False when nothing matches (the selection is cleared).
    pub fn find(&self, find: &mut Find, toward: Toward) -> bool {
        let mut term = self.0.lock_unfair();
        let selected = term.selection.as_ref().and_then(|s| s.to_range(&*term));
        let bottom = viewport_to_point(
            term.grid().display_offset(),
            Point::new(term.screen_lines() - 1, term.last_column()),
        );
        let (origin, direction) = match (toward, selected) {
            (Toward::Here, Some(s)) => (s.end, Direction::Left),
            (Toward::Older, Some(s)) => (s.start.sub(&*term, Boundary::None, 1), Direction::Left),
            (Toward::Newer, Some(s)) => (s.end.add(&*term, Boundary::None, 1), Direction::Right),
            (_, None) => (bottom, Direction::Left),
        };
        let found = term.search_next(&mut find.0, origin, direction, Side::Left, None);
        term.selection = found.as_ref().map(|m| {
            let mut selection = Selection::new(SelectionType::Simple, *m.start(), Side::Left);
            selection.update(*m.end(), Side::Right);
            selection
        });
        if let Some(m) = &found {
            term.scroll_to_point(*m.start());
        }
        drop(term);
        self.touched();
        found.is_some()
    }
}

/// `text` as a regex matching it literally.
fn literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{feed, headless};
    use alacritty_terminal::index::Column;

    fn selected(term: &SharedTerm) -> Option<String> {
        term.selection_text()
    }

    fn start(term: &SharedTerm) -> Point {
        let t = term.0.lock();
        t.selection.as_ref().unwrap().to_range(&*t).unwrap().start
    }

    #[test]
    fn typing_finds_the_nearest_match_above_and_enter_goes_further_up() {
        let (term, _) = headless(20, 3);
        // Six lines on a 3-row screen: three go to the history.
        feed(&term, b"cat 1\r\ndog\r\ncat 2\r\nbird\r\ncat 3\r\nfish");
        let mut find = Find::new("cat").unwrap();
        assert!(term.find(&mut find, Toward::Here));
        assert_eq!(selected(&term).as_deref(), Some("cat"));
        let newest = start(&term);
        assert_eq!(newest.column, Column(0));
        // Older, older: the first line, scrolled into view.
        assert!(term.find(&mut find, Toward::Older));
        assert!(term.find(&mut find, Toward::Older));
        let oldest = start(&term);
        assert!(oldest.line < newest.line);
        assert_eq!(term.0.lock().grid().display_offset(), 3, "the view went up");
        // Past the oldest, it wraps to the newest; newer from there wraps to the oldest.
        assert!(term.find(&mut find, Toward::Older));
        assert_eq!(start(&term), newest);
        assert!(term.find(&mut find, Toward::Newer));
        assert_eq!(start(&term), oldest);
        // Typing more keeps the match when it still matches.
        let mut more = Find::new("cat 1").unwrap();
        assert!(term.find(&mut more, Toward::Here));
        assert_eq!(start(&term), oldest);
    }

    #[test]
    fn queries_are_literal_and_smart_case() {
        let (term, _) = headless(20, 3);
        feed(&term, b"a.b axb\r\nHello");
        assert!(term.find(&mut Find::new("a.b").unwrap(), Toward::Here));
        assert_eq!(selected(&term).as_deref(), Some("a.b"));
        term.select_clear();
        assert!(term.find(&mut Find::new("hello").unwrap(), Toward::Here));
        term.select_clear();
        assert!(!term.find(&mut Find::new("HELLO").unwrap(), Toward::Here));
    }

    #[test]
    fn no_match_selects_nothing_and_an_empty_query_is_no_search() {
        let (term, _) = headless(20, 3);
        feed(&term, b"one two");
        term.find(&mut Find::new("two").unwrap(), Toward::Here);
        assert!(!term.find(&mut Find::new("three").unwrap(), Toward::Here));
        assert_eq!(selected(&term), None);
        assert!(Find::new("").is_none());
    }
}
