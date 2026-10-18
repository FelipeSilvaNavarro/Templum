//! A fake engine for tests: the test binary itself, run again on one ignored test that answers
//! each message with the next turn of a script.

use std::io::{BufRead, Write};
use std::path::Path;

use crate::Launch;

/// The variable that names the fake engine's script.
pub const VAR: &str = "TEMPLUM_FAKE_ENGINE";

/// A script's line that ends the fake engine where it is, as a crash would.
pub const EXIT: &str = "<exit>";

/// For each line read, writes the next turn of `script` (turns are separated by a line `---`),
/// a line at a time; after the last turn, reads until the end. A line [`EXIT`] ends it at once.
pub fn run(script: &str, input: impl BufRead, mut output: impl Write) {
    let mut turns = script.split("---\n");
    for _ in input.lines().map_while(Result::ok) {
        for line in turns.next().unwrap_or_default().lines() {
            if line == EXIT {
                return;
            }
            if writeln!(output, "{line}")
                .and_then(|()| output.flush())
                .is_err()
            {
                return;
            }
        }
    }
}

/// Starts the current test binary on its test `test`, the fake engine, with `script`.
pub fn launch(test: &str, script: &Path, cwd: &Path) -> Launch {
    Launch {
        program: std::env::current_exe().expect("the test binary"),
        args: [
            test,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
            "-q",
        ]
        .map(String::from)
        .into(),
        cwd: cwd.to_owned(),
        env: vec![(VAR.into(), script.to_string_lossy().into_owned())],
        env_remove: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn each_line_read_gets_the_next_turn() {
        let mut out = Vec::new();
        super::run("a\n---\nb\nc\n", &b"x\ny\nz\n"[..], &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "a\nb\nc\n");
        let mut out = Vec::new();
        super::run("a\n<exit>\nb\n---\nc\n", &b"x\ny\n"[..], &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "a\n", "it ends at <exit>");
    }
}
