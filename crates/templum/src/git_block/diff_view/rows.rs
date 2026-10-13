//! The diff panel's rows: what one scroll shows, built from the files shown. Pure, so it is
//! tested without a window. See docs/specs/git.md, section 2.

use std::collections::HashSet;

use templum_git::FileDiff;

/// One file in the panel: in the Changes section (unstaged, untracked too) or in Staged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    pub staged: bool,
    pub diff: FileDiff,
}

impl Shown {
    /// What the folded, large-shown and whole sets are keyed by.
    pub fn key(&self) -> (bool, String) {
        (self.staged, self.diff.path().to_string())
    }

    /// Digits of the largest line number shown, for the number columns.
    pub fn digits(&self) -> usize {
        let last = self.diff.hunks.last().map_or(1, |h| {
            (h.old_start + h.old_len).max(h.new_start + h.new_len)
        });
        last.max(1).to_string().len()
    }

    /// The first hunk's first line on the new side: where Open in editor goes.
    pub fn first_line(&self) -> u32 {
        self.diff.hunks.first().map_or(1, |h| h.new_start.max(1))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    /// "Changes · 3 files" or "Staged · 1 file".
    Section {
        staged: bool,
        files: usize,
    },
    File {
        file: usize,
    },
    Hunk {
        file: usize,
        hunk: usize,
    },
    Line {
        file: usize,
        hunk: usize,
        line: usize,
    },
    /// "⋯ 40 unchanged lines": before the first hunk, between two, after the last.
    Fold {
        file: usize,
        lines: u32,
    },
    Note {
        file: usize,
        note: Note,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Note {
    Binary,
    TooLarge,
    /// Large or generated: folded until Show diff.
    Large,
    Mode,
}

impl Row {
    /// The file the row belongs to; None for a section heading.
    pub fn file(&self) -> Option<usize> {
        match self {
            Row::Section { .. } => None,
            Row::File { file }
            | Row::Hunk { file, .. }
            | Row::Line { file, .. }
            | Row::Fold { file, .. }
            | Row::Note { file, .. } => Some(*file),
        }
    }
}

/// The rows for `files`: each section's heading, then each file's row and, unless folded, what
/// it holds. A section with no file is left out.
pub(crate) fn rows(
    files: &[Shown],
    folded: &HashSet<(bool, String)>,
    large_shown: &HashSet<(bool, String)>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    for staged in [false, true] {
        let in_section: Vec<usize> = files
            .iter()
            .enumerate()
            .filter(|(_, s)| s.staged == staged)
            .map(|(i, _)| i)
            .collect();
        if in_section.is_empty() {
            continue;
        }
        rows.push(Row::Section {
            staged,
            files: in_section.len(),
        });
        for file in in_section {
            rows.push(Row::File { file });
            let shown = &files[file];
            if folded.contains(&shown.key()) {
                continue;
            }
            let d = &shown.diff;
            if d.too_large {
                rows.push(Row::Note {
                    file,
                    note: Note::TooLarge,
                });
                continue;
            }
            if d.binary {
                rows.push(Row::Note {
                    file,
                    note: Note::Binary,
                });
                continue;
            }
            if d.mode.is_some() {
                rows.push(Row::Note {
                    file,
                    note: Note::Mode,
                });
            }
            if d.large() && !large_shown.contains(&shown.key()) {
                rows.push(Row::Note {
                    file,
                    note: Note::Large,
                });
                continue;
            }
            // The last old line a hunk showed: the gap to the next hunk is folded.
            let mut shown_to = 0;
            for (hunk, h) in d.hunks.iter().enumerate() {
                let gap = h.old_start.saturating_sub(shown_to + 1);
                if gap > 0 {
                    rows.push(Row::Fold { file, lines: gap });
                }
                rows.push(Row::Hunk { file, hunk });
                rows.extend((0..h.lines.len()).map(|line| Row::Line { file, hunk, line }));
                shown_to = h.old_start + h.old_len.saturating_sub(1);
            }
            if let (Some(last), Some(total)) = (d.hunks.last(), d.lines) {
                let end = last.new_start + last.new_len.saturating_sub(1);
                if total > end {
                    rows.push(Row::Fold {
                        file,
                        lines: total - end,
                    });
                }
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use templum_git::parse_diff;

    fn shown(staged: bool, text: &str, lines: Option<u32>) -> Shown {
        let mut diff = parse_diff(text, usize::MAX).pop().unwrap();
        diff.lines = lines;
        Shown { staged, diff }
    }

    const TWO_HUNKS: &str = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n\
@@ -5,3 +5,4 @@\n a\n-b\n+c\n+d\n e\n\
@@ -20,2 +21,2 @@ fn f() {\n x\n-y\n+z\n";

    #[test]
    fn sections_files_hunks_lines_and_folds() {
        let files = [
            shown(false, TWO_HUNKS, Some(30)),
            shown(
                true,
                "diff --git a/y b/y\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-1\n+2\n",
                Some(1),
            ),
        ];
        let got = rows(&files, &HashSet::new(), &HashSet::new());
        use Row::*;
        assert_eq!(
            got,
            [
                Section {
                    staged: false,
                    files: 1
                },
                File { file: 0 },
                Fold { file: 0, lines: 4 },
                Hunk { file: 0, hunk: 0 },
                Line {
                    file: 0,
                    hunk: 0,
                    line: 0
                },
                Line {
                    file: 0,
                    hunk: 0,
                    line: 1
                },
                Line {
                    file: 0,
                    hunk: 0,
                    line: 2
                },
                Line {
                    file: 0,
                    hunk: 0,
                    line: 3
                },
                Line {
                    file: 0,
                    hunk: 0,
                    line: 4
                },
                Fold { file: 0, lines: 12 },
                Hunk { file: 0, hunk: 1 },
                Line {
                    file: 0,
                    hunk: 1,
                    line: 0
                },
                Line {
                    file: 0,
                    hunk: 1,
                    line: 1
                },
                Line {
                    file: 0,
                    hunk: 1,
                    line: 2
                },
                Fold { file: 0, lines: 8 },
                Section {
                    staged: true,
                    files: 1
                },
                File { file: 1 },
                Hunk { file: 1, hunk: 0 },
                Line {
                    file: 1,
                    hunk: 0,
                    line: 0
                },
                Line {
                    file: 1,
                    hunk: 0,
                    line: 1
                },
            ]
        );
        assert_eq!(got[2].file(), Some(0));
        assert_eq!(got[0].file(), None);
    }

    #[test]
    fn no_fold_without_a_line_count_or_a_gap() {
        let files = [shown(false, TWO_HUNKS, None)];
        let got = rows(&files, &HashSet::new(), &HashSet::new());
        assert!(
            !matches!(got.last(), Some(Row::Fold { .. })),
            "the end is unknown"
        );
        let new = shown(
            false,
            &templum_git::new_file_diff("n.txt", "a\nb\n"),
            Some(2),
        );
        let got = rows(&[new], &HashSet::new(), &HashSet::new());
        assert_eq!(
            got.len(),
            5,
            "section, file, hunk, two lines: no fold before line 1 nor after"
        );
    }

    #[test]
    fn a_folded_file_shows_its_row_only_and_an_empty_section_is_left_out() {
        let files = [shown(false, TWO_HUNKS, Some(30))];
        let folded: HashSet<_> = [files[0].key()].into_iter().collect();
        let got = rows(&files, &folded, &HashSet::new());
        assert_eq!(
            got,
            [
                Row::Section {
                    staged: false,
                    files: 1
                },
                Row::File { file: 0 }
            ]
        );
        assert_eq!(files[0].key(), (false, "x.rs".to_string()));
        assert!(rows(&[], &HashSet::new(), &HashSet::new()).is_empty());
    }

    #[test]
    fn special_files_get_a_note() {
        let binary = shown(
            false,
            "diff --git a/i.png b/i.png\nBinary files a/i.png and b/i.png differ\n",
            None,
        );
        let mut huge = shown(
            false,
            "diff --git a/h b/h\n--- a/h\n+++ b/h\n@@ -1 +1 @@\n-a\n+b\n",
            None,
        );
        huge.diff.too_large = true;
        huge.diff.hunks.clear();
        let mode = shown(
            false,
            "diff --git a/r b/r\nold mode 100644\nnew mode 100755\n",
            None,
        );
        let lock = shown(
            true,
            "diff --git a/Cargo.lock b/Cargo.lock\n--- a/Cargo.lock\n+++ b/Cargo.lock\n@@ -1 +1 @@\n-a\n+b\n",
            None,
        );
        let files = [binary, huge, mode, lock];
        let got = rows(&files, &HashSet::new(), &HashSet::new());
        use Row::*;
        assert_eq!(
            got,
            [
                Section {
                    staged: false,
                    files: 3
                },
                File { file: 0 },
                Note {
                    file: 0,
                    note: super::Note::Binary
                },
                File { file: 1 },
                Note {
                    file: 1,
                    note: super::Note::TooLarge
                },
                File { file: 2 },
                Note {
                    file: 2,
                    note: super::Note::Mode
                },
                Section {
                    staged: true,
                    files: 1
                },
                File { file: 3 },
                Note {
                    file: 3,
                    note: super::Note::Large
                },
            ]
        );
        // Show diff: the lock file's hunk shows.
        let large_shown: HashSet<_> = [files[3].key()].into_iter().collect();
        let got = rows(&files, &HashSet::new(), &large_shown);
        assert!(matches!(got[9], Hunk { file: 3, hunk: 0 }));
    }

    #[test]
    fn digits_and_first_line() {
        let s = shown(false, TWO_HUNKS, None);
        assert_eq!(s.digits(), 2, "up to line 23");
        assert_eq!(s.first_line(), 5);
        let new = shown(false, &templum_git::new_file_diff("n.txt", "a\n"), None);
        assert_eq!(new.first_line(), 1);
        let none = shown(
            false,
            "diff --git a/r b/r\nold mode 100644\nnew mode 100755\n",
            None,
        );
        assert_eq!((none.digits(), none.first_line()), (1, 1));
    }
}
