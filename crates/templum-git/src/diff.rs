//! `git diff` output, parsed: files, their hunks, their lines. Pure: tested on recorded output.
//! See docs/specs/git.md, section 2.

use crate::parse::Change;

/// Past this many changed lines a file is folded by default (with Show diff).
pub const LARGE: u32 = 2000;

/// One file of a diff, as `git diff` prints it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    /// The path before (a rename's old one); None for a new file.
    pub old_path: Option<String>,
    /// The path after; None for a deleted file.
    pub new_path: Option<String>,
    pub status: Change,
    pub binary: bool,
    /// `old mode 100644` → `new mode 100755`.
    pub mode: Option<(String, String)>,
    pub hunks: Vec<Hunk>,
    /// Its diff was longer than the limit: the hunks were dropped.
    pub too_large: bool,
    /// The working file's size and line count, counted by `Repo::working_diff`; None when the
    /// file is gone or wasn't counted.
    pub bytes: Option<u64>,
    pub lines: Option<u32>,
}

impl FileDiff {
    pub fn empty() -> FileDiff {
        FileDiff {
            old_path: None,
            new_path: None,
            status: Change::Modified,
            binary: false,
            mode: None,
            hunks: Vec::new(),
            too_large: false,
            bytes: None,
            lines: None,
        }
    }

    /// The path now (the old one for a deleted file).
    pub fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("")
    }

    fn count(&self, kind: Kind) -> u32 {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == kind)
            .count() as u32
    }

    pub fn added(&self) -> u32 {
        self.count(Kind::Added)
    }

    pub fn removed(&self) -> u32 {
        self.count(Kind::Removed)
    }

    pub fn changed(&self) -> u32 {
        self.added() + self.removed()
    }

    /// A lock file, a minified file or a source map.
    pub fn generated(&self) -> bool {
        let name = self.path().rsplit('/').next().unwrap_or("");
        matches!(
            name,
            "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml"
        ) || name.ends_with(".min.js")
            || name.ends_with(".min.css")
            || name.ends_with(".map")
    }

    /// Folded by default, with Show diff: generated, or over `LARGE` changed lines.
    pub fn large(&self) -> bool {
        self.generated() || self.changed() > LARGE
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    /// What git names after `@@ … @@`: the function or heading, often empty.
    pub context: String,
    pub lines: Vec<Line>,
}

impl Hunk {
    /// Where it is: git's context, else "Lines 12–20" (the new side).
    pub fn title(&self) -> String {
        if !self.context.is_empty() {
            return self.context.clone();
        }
        let end = self.new_start + self.new_len.saturating_sub(1);
        if end > self.new_start {
            format!("Lines {}–{end}", self.new_start)
        } else {
            format!("Line {}", self.new_start)
        }
    }

    /// Removed lines paired with the added lines right after them, the i-th with the i-th:
    /// indices into `lines`. The pairs are what the changed words are computed on.
    pub fn pairs(&self) -> Vec<(usize, usize)> {
        let mut pairs = Vec::new();
        let mut i = 0;
        while i < self.lines.len() {
            if self.lines[i].kind != Kind::Removed {
                i += 1;
                continue;
            }
            let removed = i;
            while i < self.lines.len() && self.lines[i].kind == Kind::Removed {
                i += 1;
            }
            let added = i;
            while i < self.lines.len() && self.lines[i].kind == Kind::Added {
                i += 1;
            }
            for k in 0..(added - removed).min(i - added) {
                pairs.push((removed + k, added + k));
            }
        }
        pairs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Context,
    Added,
    Removed,
}

impl Kind {
    /// The gutter's sign, and the line's first byte in a patch.
    pub fn sign(self) -> char {
        match self {
            Kind::Context => ' ',
            Kind::Added => '+',
            Kind::Removed => '-',
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: Kind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    /// Without its leading sign and its '\n'; a CRLF file's lines keep their '\r'.
    pub text: String,
    /// "\ No newline at end of file" followed it.
    pub no_newline: bool,
}

/// `git diff --no-color` output into files. A file whose text is longer than `limit` bytes keeps
/// its headers and drops its hunks (`too_large`). Combined diffs (`diff --cc`, a conflicted
/// file) are skipped.
pub fn parse_diff(text: &str, limit: usize) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    // Where the open file's section started, for the limit; None while skipping.
    let mut start: Option<usize> = None;
    let mut offset = 0;
    // The next line numbers of the open hunk.
    let (mut old_next, mut new_next) = (0u32, 0u32);
    for raw in text.split_inclusive('\n') {
        let at = offset;
        offset += raw.len();
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let (old, new) = git_header_paths(rest);
            let mut file = FileDiff::empty();
            file.old_path = Some(old);
            file.new_path = Some(new);
            files.push(file);
            start = Some(at);
            continue;
        }
        if line.starts_with("diff --cc ") || line.starts_with("diff --combined ") {
            start = None;
            continue;
        }
        let Some(from) = start else {
            continue;
        };
        let file = files.last_mut().expect("opened by diff --git");
        if offset - from > limit {
            file.hunks.clear();
            file.too_large = true;
            start = None;
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@ ") {
            if let Some(hunk) = parse_hunk_header(rest) {
                (old_next, new_next) = (hunk.old_start, hunk.new_start);
                file.hunks.push(hunk);
            }
            continue;
        }
        if let Some(hunk) = file.hunks.last_mut() {
            let (kind, text) = match line.as_bytes().first() {
                Some(b' ') => (Kind::Context, &line[1..]),
                Some(b'+') => (Kind::Added, &line[1..]),
                Some(b'-') => (Kind::Removed, &line[1..]),
                Some(b'\\') => {
                    if let Some(last) = hunk.lines.last_mut() {
                        last.no_newline = true;
                    }
                    continue;
                }
                _ => continue,
            };
            let (old_no, new_no) = match kind {
                Kind::Context => (Some(old_next), Some(new_next)),
                Kind::Added => (None, Some(new_next)),
                Kind::Removed => (Some(old_next), None),
            };
            old_next += u32::from(old_no.is_some());
            new_next += u32::from(new_no.is_some());
            hunk.lines.push(Line {
                kind,
                old_no,
                new_no,
                text: text.to_string(),
                no_newline: false,
            });
            continue;
        }
        // The headers before the first hunk.
        if let Some(mode) = line.strip_prefix("old mode ") {
            file.mode = Some((mode.to_string(), String::new()));
        } else if let Some(mode) = line.strip_prefix("new mode ") {
            if let Some(m) = &mut file.mode {
                m.1 = mode.to_string();
            }
        } else if line.starts_with("new file mode ") {
            file.status = Change::Added;
            file.old_path = None;
        } else if line.starts_with("deleted file mode ") {
            file.status = Change::Deleted;
            file.new_path = None;
        } else if let Some(from) = line.strip_prefix("rename from ") {
            file.status = Change::Renamed;
            file.old_path = Some(from.to_string());
        } else if let Some(to) = line.strip_prefix("rename to ") {
            file.new_path = Some(to.to_string());
        } else if let Some(from) = line.strip_prefix("copy from ") {
            file.status = Change::Added;
            file.old_path = Some(from.to_string());
        } else if let Some(to) = line.strip_prefix("copy to ") {
            file.new_path = Some(to.to_string());
        } else if let Some(p) = line.strip_prefix("--- ") {
            if let Some(p) = named(p, "a/") {
                file.old_path = Some(p);
            }
        } else if let Some(p) = line.strip_prefix("+++ ") {
            if let Some(p) = named(p, "b/") {
                file.new_path = Some(p);
            }
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            file.binary = true;
        }
    }
    files
}

/// "a/x b/x" → ("x", "x"): the split where both halves agree (a name with spaces), else the
/// first " b/". A rename's halves are set again by its `rename from` and `rename to` lines.
fn git_header_paths(rest: &str) -> (String, String) {
    let rest = rest.strip_prefix("a/").unwrap_or(rest);
    let mut first = None;
    for (i, _) in rest.match_indices(" b/") {
        let (left, right) = (&rest[..i], &rest[i + 3..]);
        if left == right {
            return (left.to_string(), right.to_string());
        }
        first.get_or_insert((left.to_string(), right.to_string()));
    }
    first.unwrap_or_else(|| (rest.to_string(), rest.to_string()))
}

/// A `---`/`+++` line's path: without its prefix and the tab git adds after a name with
/// spaces; None for /dev/null.
fn named(p: &str, prefix: &str) -> Option<String> {
    p.trim_end_matches('\t')
        .strip_prefix(prefix)
        .map(str::to_string)
}

/// "-10,3 +12,4 @@ fn x() {" (after "@@ "); a missing length is 1.
fn parse_hunk_header(rest: &str) -> Option<Hunk> {
    let rest = rest.strip_prefix('-')?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, rest) = rest.split_once(" @@")?;
    let range = |s: &str| -> Option<(u32, u32)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let ((old_start, old_len), (new_start, new_len)) = (range(old)?, range(new)?);
    Some(Hunk {
        old_start,
        old_len,
        new_start,
        new_len,
        context: rest.trim().to_string(),
        lines: Vec::new(),
    })
}

/// An untracked file as a new file's diff, so the panel shows it like any other (git has no
/// diff for it): every line added.
pub fn new_file_diff(path: &str, text: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        lines.len()
    );
    for line in lines {
        out.push('+');
        out.push_str(line);
        if !line.ends_with('\n') {
            out.push_str("\n\\ No newline at end of file\n");
        }
    }
    out
}

/// An untracked binary file as a new file's diff.
pub fn new_binary_diff(path: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\nBinary files /dev/null and b/{path} differ\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Change;

    const MODIFIED: &str = "diff --git a/src/lib.rs b/src/lib.rs\n\
index 3b18e51..a1b2c3d 100644\n\
--- a/src/lib.rs\n\
+++ b/src/lib.rs\n\
@@ -1,4 +1,5 @@\n\
\x20fn one() {}\n\
-fn two() {}\n\
+fn two() { 2 }\n\
+fn three() {}\n\
\x20fn four() {}\n\
\x20fn five() {}\n\
@@ -20,3 +21,3 @@ fn twenty() {\n\
\x20a\n\
-b\n\
+c\n\
\x20d\n";

    #[test]
    fn a_modified_file_with_two_hunks() {
        let files = parse_diff(MODIFIED, usize::MAX);
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.path(), "src/lib.rs");
        assert_eq!(f.old_path.as_deref(), Some("src/lib.rs"));
        assert_eq!(f.status, Change::Modified);
        assert!(!f.binary && !f.too_large);
        assert_eq!(f.hunks.len(), 2);
        let h = &f.hunks[0];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (1, 4, 1, 5)
        );
        assert_eq!(h.context, "");
        assert_eq!(h.title(), "Lines 1–5");
        let got: Vec<_> = h
            .lines
            .iter()
            .map(|l| (l.kind, l.old_no, l.new_no, l.text.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (Kind::Context, Some(1), Some(1), "fn one() {}"),
                (Kind::Removed, Some(2), None, "fn two() {}"),
                (Kind::Added, None, Some(2), "fn two() { 2 }"),
                (Kind::Added, None, Some(3), "fn three() {}"),
                (Kind::Context, Some(3), Some(4), "fn four() {}"),
                (Kind::Context, Some(4), Some(5), "fn five() {}"),
            ]
        );
        assert_eq!(f.hunks[1].context, "fn twenty() {");
        assert_eq!(f.hunks[1].title(), "fn twenty() {");
        assert_eq!((f.added(), f.removed(), f.changed()), (3, 2, 5));
        assert_eq!(
            h.pairs(),
            [(1, 2)],
            "the first removed with the first added"
        );
    }

    #[test]
    fn pairs_match_removed_runs_with_the_added_run_after_them() {
        let text =
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,4 +1,3 @@\n-a\n-b\n+c\n d\n-e\n+f\n+g\n";
        let h = &parse_diff(text, usize::MAX)[0].hunks[0];
        assert_eq!(h.pairs(), [(0, 2), (4, 5)], "b has no partner; g has none");
        assert_eq!(h.title(), "Lines 1–3");
        let one = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n";
        let h = &parse_diff(one, usize::MAX)[0].hunks[0];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (1, 1, 1, 1)
        );
        assert_eq!(h.title(), "Line 1");
    }

    #[test]
    fn a_rename_with_changes_and_a_pure_rename() {
        let text = "diff --git a/old.rs b/new.rs\n\
similarity index 90%\n\
rename from old.rs\n\
rename to new.rs\n\
index 1111111..2222222 100644\n\
--- a/old.rs\n\
+++ b/new.rs\n\
@@ -1,2 +1,2 @@\n\
\x20x\n\
-y\n\
+z\n\
diff --git a/a.txt b/b.txt\n\
similarity index 100%\n\
rename from a.txt\n\
rename to b.txt\n";
        let files = parse_diff(text, usize::MAX);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].status, Change::Renamed);
        assert_eq!(files[0].old_path.as_deref(), Some("old.rs"));
        assert_eq!(files[0].new_path.as_deref(), Some("new.rs"));
        assert_eq!(files[0].hunks.len(), 1);
        assert_eq!(files[1].status, Change::Renamed);
        assert_eq!(files[1].path(), "b.txt");
        assert!(files[1].hunks.is_empty(), "a pure rename");
    }

    #[test]
    fn binary_new_deleted_and_mode_change() {
        let text = "diff --git a/img.png b/img.png\n\
index 1111111..2222222 100644\n\
Binary files a/img.png and b/img.png differ\n\
diff --git a/new.txt b/new.txt\n\
new file mode 100644\n\
index 0000000..3bd1f0e\n\
--- /dev/null\n\
+++ b/new.txt\n\
@@ -0,0 +1,2 @@\n\
+foo\n\
+bar\n\
diff --git a/gone.txt b/gone.txt\n\
deleted file mode 100644\n\
index 3bd1f0e..0000000\n\
--- a/gone.txt\n\
+++ /dev/null\n\
@@ -1,2 +0,0 @@\n\
-foo\n\
-bar\n\
diff --git a/run.sh b/run.sh\n\
old mode 100644\n\
new mode 100755\n";
        let files = parse_diff(text, usize::MAX);
        assert_eq!(files.len(), 4);
        assert!(files[0].binary);
        assert_eq!(files[0].path(), "img.png");
        let new = &files[1];
        assert_eq!(new.status, Change::Added);
        assert_eq!(new.old_path, None);
        assert_eq!(new.path(), "new.txt");
        assert_eq!(new.hunks[0].lines[1].new_no, Some(2));
        let gone = &files[2];
        assert_eq!(gone.status, Change::Deleted);
        assert_eq!(gone.new_path, None);
        assert_eq!(gone.path(), "gone.txt");
        assert_eq!(gone.hunks[0].lines[1].old_no, Some(2));
        assert_eq!(
            files[3].mode,
            Some(("100644".to_string(), "100755".to_string()))
        );
        assert!(files[3].hunks.is_empty());
    }

    #[test]
    fn crlf_no_newline_and_a_name_with_spaces() {
        let text = "diff --git a/with space.txt b/with space.txt\n\
index 1111111..2222222 100644\n\
--- a/with space.txt\t\n\
+++ b/with space.txt\t\n\
@@ -1,2 +1,2 @@\n\
\x20a\r\n\
-b\r\n\
+c\r\n\
diff --git a/x b/x\n\
--- a/x\n\
+++ b/x\n\
@@ -1 +1 @@\n\
-x\n\
\\ No newline at end of file\n\
+y\n\
\\ No newline at end of file\n";
        let files = parse_diff(text, usize::MAX);
        assert_eq!(
            files[0].path(),
            "with space.txt",
            "the tab after the name is not part of it"
        );
        let lines = &files[0].hunks[0].lines;
        assert_eq!(lines[0].text, "a\r", "kept: batch 2's patches need it back");
        assert_eq!(lines[2].text, "c\r");
        let lines = &files[1].hunks[0].lines;
        assert!(lines[0].no_newline && lines[1].no_newline);
        assert_eq!(lines[1].text, "y");
    }

    #[test]
    fn a_combined_diff_is_skipped_and_a_huge_file_keeps_only_its_headers() {
        let text = "diff --cc c.txt\n\
index 1111111,2222222..0000000\n\
--- a/c.txt\n\
+++ b/c.txt\n\
@@@ -1,1 -1,1 +1,5 @@@\n\
++<<<<<<< HEAD\n\
\x20+ours\n\
++=======\n\
+ theirs\n\
++>>>>>>> topic\n\
diff --git a/ok.txt b/ok.txt\n\
--- a/ok.txt\n\
+++ b/ok.txt\n\
@@ -1 +1 @@\n\
-a\n\
+b\n";
        let files = parse_diff(text, usize::MAX);
        assert_eq!(files.len(), 1, "the conflicted file is left out");
        assert_eq!(files[0].path(), "ok.txt");
        let files = parse_diff(MODIFIED, 80);
        assert!(files[0].too_large, "longer than the limit");
        assert!(files[0].hunks.is_empty());
        assert_eq!(files[0].path(), "src/lib.rs");
    }

    #[test]
    fn an_untracked_file_as_a_new_files_diff() {
        let files = parse_diff(&new_file_diff("n.txt", "one\ntwo"), usize::MAX);
        let f = &files[0];
        assert_eq!(f.status, Change::Added);
        assert_eq!(f.path(), "n.txt");
        let h = &f.hunks[0];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (0, 0, 1, 2)
        );
        assert_eq!(h.lines.len(), 2);
        assert_eq!(h.lines[1].text, "two");
        assert!(h.lines[1].no_newline && !h.lines[0].no_newline);
        let files = parse_diff(&new_binary_diff("b.bin"), usize::MAX);
        assert!(files[0].binary);
        assert_eq!(files[0].status, Change::Added);
        assert!(
            parse_diff(&new_file_diff("e.txt", ""), usize::MAX)[0].hunks[0]
                .lines
                .is_empty()
        );
    }

    #[test]
    fn large_and_generated_files() {
        let mut lock = FileDiff::empty();
        lock.new_path = Some("Cargo.lock".to_string());
        assert!(lock.generated() && lock.large());
        for name in [
            "app.min.js",
            "a/b/style.min.css",
            "x.map",
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
        ] {
            lock.new_path = Some(name.to_string());
            assert!(lock.generated(), "{name}");
        }
        lock.new_path = Some("src/main.rs".to_string());
        assert!(!lock.generated() && !lock.large());
        let big = format!(
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,0 +1,{} @@\n{}",
            LARGE + 1,
            "+l\n".repeat(LARGE as usize + 1)
        );
        assert!(parse_diff(&big, usize::MAX)[0].large());
        assert_eq!(Kind::Added.sign(), '+');
        assert_eq!(Kind::Removed.sign(), '-');
        assert_eq!(Kind::Context.sign(), ' ');
    }
}
