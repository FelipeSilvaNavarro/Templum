//! Tests of the settings panel's choices, numbers, slide, fonts and shell arguments.

use super::edits::{join_args, split_args};
use super::fields::{CURSOR, FONT_SIZE, MAX_DIFF, MODE, TIMEOUT};
use super::fonts::{is_monospace, safe_to_load};
use super::*;
use crate::config::Config;

#[test]
fn every_choice_reads_back_what_it_writes() {
    for choice in [&MODE, &CURSOR] {
        for (value, _) in choice.options {
            let mut c = Config::default();
            (choice.set)(&mut c, value);
            assert_eq!((choice.get)(&c), *value, "{}.{}", choice.table, choice.key);
            let text = format!(
                "[{}]\n{} = {}\n",
                choice.table,
                choice.key,
                (choice.toml)(value)
            );
            let parsed = config::parse(&text).unwrap();
            assert_eq!((choice.get)(&parsed), *value, "{text}");
        }
    }
}

#[test]
fn numbers_read_back_what_they_write() {
    for n in [&SCROLLBACK, &FONT_SIZE, &TIMEOUT, &MAX_DIFF] {
        for v in [n.min, n.min + n.step, n.max] {
            let mut c = Config::default();
            (n.set)(&mut c, v);
            assert_eq!((n.get)(&c), v, "{}.{}", n.table, n.key);
            let text = format!("[{}]\n{} = {}\n", n.table, n.key, v as i64);
            assert_eq!((n.get)(&config::parse(&text).unwrap()), v, "{text}");
        }
    }
}

#[test]
fn turning_back_mid_slide_starts_where_the_panel_is() {
    let start = Instant::now();
    let opening = Panel::new(start);
    for quarter in 0..=4 {
        let now = start + motion::SLIDE * quarter / 4;
        let closing = opening.reversed(now);
        assert!(closing.closing);
        assert!(
            (closing.shown(now) - opening.shown(now)).abs() < 1e-4,
            "closing at {quarter}/4"
        );
        let later = now + motion::SLIDE / 8;
        let reopening = closing.reversed(later);
        assert!(!reopening.closing);
        assert!(
            (reopening.shown(later) - closing.shown(later)).abs() < 1e-4,
            "reopening at {quarter}/4"
        );
    }
    let settled = start + motion::SLIDE * 2;
    assert_eq!(opening.reversed(settled).shown(settled), 1.0);
}

#[test]
fn equal_advances_are_monospaced() {
    assert!(is_monospace(8.4, 8.4));
    assert!(is_monospace(8.4, 8.405));
    assert!(!is_monospace(3.9, 12.6));
}

#[test]
fn args_split_on_spaces_and_keep_quoted_ones() {
    assert_eq!(split_args("--login -i").unwrap(), ["--login", "-i"]);
    assert_eq!(split_args("  -c  \"echo hi\" ").unwrap(), ["-c", "echo hi"]);
    assert_eq!(split_args("-c 'say \"hi\"'").unwrap(), ["-c", "say \"hi\""]);
    assert_eq!(split_args("\"\"").unwrap(), [""]);
    assert!(split_args("").unwrap().is_empty());
    assert_eq!(split_args("-c \"echo hi"), Err("Unclosed quote"));
}

#[test]
fn joined_args_split_back() {
    let args: Vec<String> = [
        "--login",
        "a b",
        "it's",
        "say \"hi\"",
        "",
        "printf '%s' \"x\"",
        "echo \"it's\"",
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(split_args(&join_args(&args)).unwrap(), args);
    assert_eq!(join_args(&["--login".into(), "-i".into()]), "--login -i");
}

#[test]
fn only_families_with_i_and_m_are_loaded_on_linux() {
    let installed: Vec<String> = ["Fira Code", "JetBrains Mono", "Noto Color Emoji"]
        .map(String::from)
        .to_vec();
    let with_i_and_m: std::collections::HashSet<String> =
        ["Fira Code", "JetBrains Mono"].map(String::from).into();
    assert_eq!(
        safe_to_load(installed.clone(), Some(with_i_and_m)),
        ["Fira Code", "JetBrains Mono"]
    );
    assert_eq!(
        safe_to_load(installed, None),
        ["JetBrains Mono"],
        "without fc-list, the built-in list"
    );
}
