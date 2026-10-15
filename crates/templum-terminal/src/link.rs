//! What Ctrl+click opens in a terminal: a link a program marked (OSC 8), else a URL or a file
//! path in the text under the pointer, with its line (`src/main.rs:42`, `a.ts(12,5)`).

use std::path::{Path, PathBuf};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;

use crate::SharedTerm;
use crate::terminal::{Listener, grid_point};

/// A link found in a terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Url(String),
    /// As written: relative paths are resolved with [`resolve`].
    File {
        path: PathBuf,
        line: Option<u32>,
    },
}

impl SharedTerm {
    /// The link at a viewport cell (row 0 = top visible line), if any.
    pub fn link_at(&self, row: usize, col: usize) -> Option<Target> {
        let term = self.0.lock_unfair();
        let mut point = grid_point(&term, row, col);
        if term.grid()[point].flags.contains(Flags::WIDE_CHAR_SPACER) && point.column.0 > 0 {
            point.column -= 1;
        }
        // Templum's own links (shell integration) are text: what is under them may be a path.
        if let Some(link) = term.grid()[point].hyperlink()
            && !link.uri().starts_with("templum:")
        {
            return uri(link.uri());
        }
        let (text, at) = word_around(&term, point)?;
        parse(&text, at)
    }
}

/// A marked link's target: a web page, or a file:// one's file.
fn uri(uri: &str) -> Option<Target> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return Some(Target::Url(uri.to_string()));
    }
    let path = uri.strip_prefix("file://")?;
    // file://host/path: the path starts at its first slash.
    let path = &path[path.find('/')?..];
    Some(Target::File {
        path: PathBuf::from(crate::tap::percent_decode(path)),
        line: None,
    })
}

/// The run of non-blank characters around `point`, across wrapped lines, and the point's index
/// in it. None on a blank.
fn word_around(term: &Term<Listener>, point: Point) -> Option<(Vec<char>, usize)> {
    let grid = term.grid();
    let last = term.last_column();
    let (top, bottom) = (term.topmost_line(), term.bottommost_line());
    let wraps = |line: Line| grid[line][last].flags.contains(Flags::WRAPLINE);
    let spacer = |p: Point| {
        grid[p]
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    };
    let blank = |p: Point| matches!(grid[p].c, ' ' | '\t' | '\0');
    if blank(point) {
        return None;
    }
    let before = |p: Point| {
        if p.column.0 > 0 {
            Some(Point::new(p.line, p.column - 1))
        } else {
            (p.line > top && wraps(p.line - 1)).then(|| Point::new(p.line - 1, last))
        }
    };
    let after = |p: Point| {
        if p.column < last {
            Some(Point::new(p.line, p.column + 1))
        } else {
            (p.line < bottom && wraps(p.line)).then(|| Point::new(p.line + 1, Column(0)))
        }
    };
    let mut left = Vec::new();
    let mut p = point;
    while let Some(q) = before(p) {
        p = q;
        if spacer(p) {
            continue;
        }
        if blank(p) {
            break;
        }
        left.push(grid[p].c);
    }
    left.reverse();
    let at = left.len();
    let mut text = left;
    text.push(grid[point].c);
    let mut p = point;
    while let Some(q) = after(p) {
        p = q;
        if spacer(p) {
            continue;
        }
        if blank(p) {
            break;
        }
        text.push(grid[p].c);
    }
    Some((text, at))
}

/// Punctuation around a link in prose, never part of it.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', '\'', '"'];
/// Characters that end a path in a program's output (quotes, brackets, box drawing).
fn delimiter(c: char) -> bool {
    "()[]{}<>\"'`,".contains(c) || ('\u{2500}'..='\u{257F}').contains(&c)
}

/// The link in `text` at index `at`: a URL, else a path (with its line). Plain words, times and
/// version numbers aren't links.
fn parse(text: &[char], at: usize) -> Option<Target> {
    let s: String = text.iter().collect();
    // Byte offsets of each char, to map `at`.
    let offsets: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
    let at = *offsets.get(at)?;
    for scheme in ["https://", "http://"] {
        if let Some(start) = s.find(scheme) {
            let url = trim_url(&s[start..]);
            if (start..start + url.len()).contains(&at) {
                return Some(Target::Url(url.to_string()));
            }
        }
    }
    let run = s.trim_end_matches(TRAILING);
    let run = trim_unmatched(run);
    let (body, line) = strip_line(run);
    // The piece of the path under the pointer (or the last one, when it is on the line): at
    // most its last char's start, a char boundary.
    let at = if at < body.len() {
        at
    } else {
        body.char_indices().last().map_or(0, |(i, _)| i)
    };
    let start = body[..at].rfind(delimiter).map_or(0, |i| i + 1);
    let end = body[at..].find(delimiter).map_or(body.len(), |i| at + i);
    let piece = body[start..end].trim_end_matches(TRAILING);
    let is_path = piece.chars().any(char::is_alphabetic)
        && (piece.contains('/')
            || piece.contains('\\')
            || piece.starts_with('~')
            || has_extension(piece)
            || line.is_some());
    is_path.then(|| Target::File {
        path: PathBuf::from(piece),
        line,
    })
}

/// A URL without the punctuation after it, nor a closing bracket it doesn't open.
fn trim_url(url: &str) -> &str {
    let url = url
        .split(|c: char| c.is_whitespace() || "<>\"'`".contains(c))
        .next()
        .unwrap_or("");
    trim_unmatched(url.trim_end_matches(TRAILING)).trim_end_matches(TRAILING)
}

/// `text` without closing brackets at its end that nothing in it opens.
fn trim_unmatched(mut text: &str) -> &str {
    while let Some(last) = text.chars().last() {
        let open = match last {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            _ => break,
        };
        if text.matches(open).count() >= text.matches(last).count() {
            break;
        }
        text = &text[..text.len() - 1];
    }
    text
}

/// A path's line: `:42`, `:42:7` or `(12,5)` at its end, removed.
fn strip_line(text: &str) -> (&str, Option<u32>) {
    if let Some(inner) = text.strip_suffix(')')
        && let Some(open) = inner.rfind('(')
    {
        let numbers = &inner[open + 1..];
        let line = numbers.split(',').next().and_then(|n| n.parse().ok());
        if line.is_some() && numbers.split(',').all(|n| n.parse::<u32>().is_ok()) {
            return (&text[..open], line);
        }
    }
    let mut body = text;
    let mut numbers = Vec::new();
    while let Some(colon) = body.rfind(':') {
        let Ok(n) = body[colon + 1..].parse::<u32>() else {
            break;
        };
        numbers.push(n);
        body = &body[..colon];
        if numbers.len() == 2 {
            break;
        }
    }
    match numbers.last() {
        Some(&line) if !body.is_empty() => (body, Some(line)),
        _ => (text, None),
    }
}

/// `name.ext` with a letter first in the extension (not `1.2.3`).
fn has_extension(piece: &str) -> bool {
    let name = piece.rsplit(['/', '\\']).next().unwrap_or(piece);
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && ext.chars().next().is_some_and(char::is_alphabetic)
    })
}

/// A path a terminal printed, as a file that exists: `~` is `home`, a relative path is in
/// `cwd`. None for a folder or nothing.
pub fn resolve(path: &Path, cwd: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let path = match path.strip_prefix("~") {
        Ok(rest) => home?.join(rest),
        Err(_) if path.is_relative() => cwd?.join(path),
        Err(_) => path.to_path_buf(),
    };
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{feed, headless};

    fn at(text: &str, i: usize) -> Option<Target> {
        let chars: Vec<char> = text.chars().collect();
        parse(&chars, i)
    }

    fn file(path: &str, line: Option<u32>) -> Option<Target> {
        Some(Target::File {
            path: PathBuf::from(path),
            line,
        })
    }

    #[test]
    fn paths_urls_and_their_lines() {
        assert_eq!(at("src/main.rs", 2), file("src/main.rs", None));
        assert_eq!(at("src/main.rs:42", 0), file("src/main.rs", Some(42)));
        assert_eq!(at("src/main.rs:42:7", 3), file("src/main.rs", Some(42)));
        assert_eq!(at("src/main.rs:42:", 3), file("src/main.rs", Some(42)));
        assert_eq!(at("./a/b.ts(12,5)", 1), file("./a/b.ts", Some(12)));
        assert_eq!(at("./a/b.ts(12,5)", 11), file("./a/b.ts", Some(12)));
        // On the line number after a name ending in a wide char.
        assert_eq!(at("a/日本:3", 5), file("a/日本", Some(3)));
        assert_eq!(at("~/x", 0), file("~/x", None));
        assert_eq!(at("/abs/x.rs", 4), file("/abs/x.rs", None));
        assert_eq!(at("C:\\x\\y.rs:3", 2), file("C:\\x\\y.rs", Some(3)));
        assert_eq!(at("(src/main.rs).", 3), file("src/main.rs", None));
        assert_eq!(at("Update(src/main.rs)", 9), file("src/main.rs", None));
        assert_eq!(at("\"/x/y.py\",", 3), file("/x/y.py", None));
        assert_eq!(
            at("https://e.com/a_(b).", 3),
            Some(Target::Url("https://e.com/a_(b)".into()))
        );
        assert_eq!(
            at("[d](https://x.io/y)", 8),
            Some(Target::Url("https://x.io/y".into()))
        );
        for plain in ["12:30:00", "1.2.3", "hello", "std::fs::read"] {
            assert_eq!(at(plain, 0), None, "{plain}");
        }
    }

    #[test]
    fn a_link_wrapped_over_rows_is_one_link() {
        let (term, _) = headless(10, 4);
        feed(&term, b"xx src/aaaa/bbbb.rs:3 yy");
        // Clicked on its second row.
        assert_eq!(term.link_at(1, 4), file("src/aaaa/bbbb.rs", Some(3)));
        assert_eq!(term.link_at(0, 0), None, "xx");
        assert_eq!(term.link_at(0, 2), None, "a blank");
    }

    #[test]
    fn wide_chars_stay_in_a_path() {
        let (term, _) = headless(20, 2);
        feed(&term, "docs/日本.md".as_bytes());
        // On the dot, and on a wide char's second column.
        assert_eq!(term.link_at(0, 9), file("docs/日本.md", None));
        assert_eq!(term.link_at(0, 6), file("docs/日本.md", None));
    }

    #[test]
    fn marked_links_open_their_uri_and_ours_are_text() {
        let (term, _) = headless(30, 2);
        feed(
            &term,
            b"\x1b]8;;https://e.com\x1b\\click here\x1b]8;;\x1b\\",
        );
        assert_eq!(
            term.link_at(0, 7),
            Some(Target::Url("https://e.com".into()))
        );
        let (term, _) = headless(30, 2);
        feed(&term, b"\x1b]8;;file:///tmp/x%20y.rs\x1b\\x\x1b]8;;\x1b\\");
        assert_eq!(term.link_at(0, 0), file("/tmp/x y.rs", None));
        let (term, _) = headless(30, 2);
        feed(
            &term,
            b"\x1b]8;;templum:cmd/1\x1b\\vim src/main.rs\x1b]8;;\x1b\\",
        );
        assert_eq!(term.link_at(0, 6), file("src/main.rs", None));
    }

    #[test]
    fn resolve_finds_files_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/f.rs"), "").unwrap();
        let d = Some(dir.path());
        assert_eq!(
            resolve(Path::new("sub/f.rs"), d, None),
            Some(dir.path().join("sub/f.rs"))
        );
        assert_eq!(
            resolve(Path::new("~/sub/f.rs"), None, d),
            Some(dir.path().join("sub/f.rs"))
        );
        assert_eq!(resolve(Path::new("sub"), d, None), None, "a folder");
        assert_eq!(resolve(Path::new("nope.rs"), d, None), None);
    }
}
