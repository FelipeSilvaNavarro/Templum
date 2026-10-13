//! A commit message from an agent: the request built from a template, the agent's command run
//! through the user's login shell (with a timeout and a cancel), and its answer split into a
//! summary and a description. No UI: see docs/specs/git.md, section 7.

/// The request when the user hasn't written their own (Settings ▸ Git ▸ Prompt).
pub const PROMPT: &str = "\
Write the commit message for the changes below.

{instructions}

Answer with the message alone, in plain text: the first line is the summary, in at most 72 \
characters; then a blank line; then what changed and why, in a few short lines. No code fences, \
no quotes, nothing before or after the message.

The last commits' summaries in this repository, to follow its style:
{recent_commits}

The changed files:
{files}

{truncated}
The diff:
{diff}
";

/// Settings ▸ Git ▸ Try it's change: a small fix in a file of no repository, so none of the
/// user's code is sent.
pub const SAMPLE_FILE: &str = "M src/greet.rs";
pub const SAMPLE_DIFF: &str = "\
diff --git a/src/greet.rs b/src/greet.rs
--- a/src/greet.rs
+++ b/src/greet.rs
@@ -1,3 +1,3 @@
 pub fn greet(name: &str) -> String {
-    format!(\"Helo, {name}\")
+    format!(\"Hello, {name}!\")
 }
";

/// What a request holds besides its template.
pub struct Request<'a> {
    /// The user's commit rules.
    pub instructions: &'a str,
    /// Every changed file, one per line ("M src/x.rs"), whole even when the diff is cut.
    pub files: &'a [String],
    pub diff: &'a str,
    /// The last commits' summaries, newest first.
    pub recent: &'a [String],
    /// The diff past this many bytes is cut, at a line's end.
    pub max_bytes: usize,
}

/// `template` with its placeholders filled: `{instructions}`, `{files}`, `{diff}`,
/// `{recent_commits}`, and `{truncated}`, a sentence when the diff was cut (else nothing).
pub fn request(template: &str, r: &Request) -> String {
    let (diff, was_cut) = cut(r.diff, r.max_bytes);
    let truncated = if was_cut {
        "The diff was cut: the list of files above is complete."
    } else {
        ""
    };
    fill(
        template,
        &[
            ("instructions", r.instructions),
            ("files", &r.files.join("\n")),
            ("recent_commits", &r.recent.join("\n")),
            ("truncated", truncated),
            ("diff", diff),
        ],
    )
}

/// `template` with each `{name}` of `values` replaced, in one pass: a value's own text is never
/// read for placeholders (a diff can hold "{files}"). Other braces stay as they are.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let found = values
            .iter()
            .find(|(name, _)| after.strip_prefix(name).is_some_and(|r| r.starts_with('}')));
        match found {
            Some((name, value)) => {
                out.push_str(value);
                rest = &after[name.len() + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `diff` up to `max` bytes, cut after the last whole line that fits (never inside a
/// character), and whether it was cut.
fn cut(diff: &str, max: usize) -> (&str, bool) {
    if diff.len() <= max {
        return (diff, false);
    }
    let end = diff.as_bytes()[..max]
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |i| i + 1);
    (&diff[..end], true)
}

/// An agent's answer as a summary (its first line) and a description (the rest), without the
/// code fences an agent may add anyway. None when it has no summary.
pub fn split(answer: &str) -> Option<(String, String)> {
    let text = answer
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    let (summary, description) = text.split_once('\n').unwrap_or((text, ""));
    let summary = summary.trim();
    (!summary.is_empty()).then(|| (summary.to_string(), description.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_in_one_pass() {
        // A value's own braces are never read: a diff can hold "{files}" or a format string.
        assert_eq!(
            fill("{a} and {b}, {c} {a", &[("a", "x{b}"), ("b", "y")]),
            "x{b} and y, {c} {a"
        );
        assert_eq!(fill("{}{{a}}", &[("a", "1")]), "{}{1}");
    }

    #[test]
    fn a_request_has_every_file_and_says_when_its_diff_is_cut() {
        let files = ["M src/a.rs".to_string(), "A new.txt".to_string()];
        let recent = ["Fix the parser".to_string()];
        let diff = "+one\n+two\n+three\n";
        let r = |max_bytes| {
            request(
                "{instructions}|{files}|{recent_commits}|{truncated}|{diff}",
                &Request {
                    instructions: "Imperative.",
                    files: &files,
                    diff,
                    recent: &recent,
                    max_bytes,
                },
            )
        };
        assert_eq!(
            r(1024),
            "Imperative.|M src/a.rs\nA new.txt|Fix the parser||+one\n+two\n+three\n"
        );
        assert_eq!(
            r(12),
            "Imperative.|M src/a.rs\nA new.txt|Fix the parser|\
             The diff was cut: the list of files above is complete.|+one\n+two\n"
        );
        // Cut at a line's end, so never inside a character.
        let wide = "ééé\nééé\n";
        assert_eq!(cut(wide, 9), ("ééé\n", true));
        assert_eq!(cut(wide, 5), ("", true));
        assert_eq!(cut(wide, 14), (wide, false));
        // The default template has every placeholder.
        for name in [
            "instructions",
            "files",
            "diff",
            "recent_commits",
            "truncated",
        ] {
            assert!(PROMPT.contains(&format!("{{{name}}}")), "{name}");
        }
    }

    #[test]
    fn an_answer_splits_into_a_summary_and_a_description() {
        assert_eq!(
            split("Fix the parser\n\nIt read past the end.\nTwice.\n"),
            Some((
                "Fix the parser".to_string(),
                "It read past the end.\nTwice.".to_string()
            ))
        );
        assert_eq!(
            split("```\nFix it\r\n\r\nWhy.\r\n```\n"),
            Some(("Fix it".to_string(), "Why.".to_string())),
            "fences and CRLF"
        );
        assert_eq!(
            split("  Only a summary  \n"),
            Some(("Only a summary".to_string(), String::new()))
        );
        assert_eq!(
            split("Summary\nno blank line"),
            Some(("Summary".to_string(), "no blank line".to_string()))
        );
        assert_eq!(split(" \n```\n```\n"), None);
    }
}
