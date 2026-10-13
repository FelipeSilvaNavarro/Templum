//! The changed words of a removed line and its added line: a longest common subsequence over
//! tokens (runs of letters and digits, runs of spaces, every other character on its own), as
//! byte ranges of each line, adjacent ones merged. See docs/specs/git.md, section 2, Lines.

use std::ops::Range;

/// Past this many tokens on a side the lines are marked whole: the table is tokens × tokens.
const CAP: usize = 400;

/// The byte ranges of `old` and of `new` that differ.
pub fn changed(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let (a, b) = (tokens(old), tokens(new));
    if a.len() > CAP || b.len() > CAP {
        return (whole(old), whole(new));
    }
    let (n, m) = (a.len(), b.len());
    // lcs[i][j]: the longest common subsequence of a[i..] and b[j..].
    let mut lcs = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[a[i].clone()] == new[b[j].clone()] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut common_a, mut common_b) = (vec![false; n], vec![false; m]);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[a[i].clone()] == new[b[j].clone()] {
            common_a[i] = true;
            common_b[j] = true;
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    (ranges(&a, &common_a), ranges(&b, &common_b))
}

// One range is the answer here, not a mistaken `vec![0; n]`.
#[allow(clippy::single_range_in_vec_init)]
fn whole(s: &str) -> Vec<Range<usize>> {
    if s.is_empty() {
        Vec::new()
    } else {
        vec![0..s.len()]
    }
}

/// The tokens that aren't common, adjacent ones merged.
fn ranges(tokens: &[Range<usize>], common: &[bool]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for (t, &c) in tokens.iter().zip(common) {
        if c {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.end == t.start => last.end = t.end,
            _ => out.push(t.clone()),
        }
    }
    out
}

#[derive(PartialEq)]
enum Class {
    Word,
    Space,
    Other,
}

fn tokens(s: &str) -> Vec<Range<usize>> {
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Other
        }
    };
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut last: Option<Class> = None;
    for (i, c) in s.char_indices() {
        let k = class(c);
        let end = i + c.len_utf8();
        match out.last_mut() {
            Some(t) if k != Class::Other && last.as_ref() == Some(&k) => t.end = end,
            _ => out.push(i..end),
        }
        last = Some(k);
    }
    out
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_word_in_the_middle() {
        let (old, new) = changed("let x = foo(1);", "let x = bar(1);");
        assert_eq!(old, [8..11]);
        assert_eq!(new, [8..11]);
    }

    #[test]
    fn an_inserted_word_marks_the_new_side_only() {
        let (old, new) = changed("a c", "a b c");
        assert!(old.is_empty());
        assert_eq!(new, [2..4], "\"b \"");
    }

    #[test]
    fn a_whole_line_changed_is_one_range_each() {
        let (old, new) = changed("alpha", "omega");
        assert_eq!(old, [0..5]);
        assert_eq!(new, [0..5]);
        let (old, new) = changed("", "x");
        assert!(old.is_empty());
        assert_eq!(new, [0..1]);
    }

    #[test]
    fn identical_lines_change_nothing() {
        assert_eq!(changed("same", "same"), (vec![], vec![]));
    }

    #[test]
    fn unicode_words_keep_their_byte_ranges() {
        let (old, new) = changed("café au lait", "café ao lait");
        assert_eq!(old, [6..8], "\"au\": é is two bytes");
        assert_eq!(new, [6..8]);
        let (old, new) = changed("日本語 テキスト", "日本語 文章");
        assert_eq!(old, [10..22]);
        assert_eq!(new, [10..16]);
    }

    #[test]
    fn punctuation_is_its_own_token() {
        let (old, new) = changed("f(a, b)", "f(a, c)");
        assert_eq!(old, [5..6]);
        assert_eq!(new, [5..6]);
    }

    #[test]
    fn past_the_cap_the_lines_are_marked_whole() {
        let long = "x ".repeat(CAP + 1);
        let (old, new) = changed(&long, &format!("{long}y"));
        assert_eq!(old, [0..long.len()]);
        assert_eq!(new, [0..long.len() + 1]);
    }
}
