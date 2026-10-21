//! Tests of the settings panel's choices, numbers, slide, fonts and shell arguments.

use super::edits::{join_args, split_args};
use super::fields::{CURSOR, FONT_SIZE, MAX_DIFF, MODE, QUESTION_TIMEOUT, RESET, TIMEOUT, WARN};
use super::fonts::{is_monospace, safe_to_load};
use super::*;
use crate::config::Config;

#[test]
fn every_choice_reads_back_what_it_writes() {
    for choice in [&MODE, &CURSOR, &RESET, &WARN] {
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
    for n in [
        &SCROLLBACK,
        &FONT_SIZE,
        &TIMEOUT,
        &MAX_DIFF,
        &QUESTION_TIMEOUT,
    ] {
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

#[test]
fn an_agent_not_installed_says_how_to_install_it() {
    let agents = crate::agents::effective(
        &crate::config::parse("[agents]\nmine = { name = \"Mine\", command = \"mine --go\" }\n")
            .unwrap()
            .agents,
        &[],
        None,
    );
    let of = |id: &str| agents.iter().find(|a| a.id == id).unwrap();
    let (claude, mine) = (of("claude"), of("mine"));
    let install = claude.install.unwrap();
    assert_eq!(
        super::agents::description(claude, false, true),
        format!("Not installed · {install}")
    );
    assert_eq!(
        super::agents::description(claude, true, true),
        claude.command
    );
    assert_eq!(
        super::agents::description(mine, false, true),
        "Yours · not found · mine --go"
    );
    assert_eq!(
        super::agents::description(mine, true, true),
        "Yours · mine --go"
    );
}

#[test]
fn the_shell_list_is_default_then_every_shell_found() {
    use super::agents::shell_choices;
    use gpui_kit::SharedString;
    use templum_terminal::ShellOption;
    let shell = |label: &str, program: &str| ShellOption {
        label: label.into(),
        program: program.into(),
        args: vec![],
    };
    let shells = [
        shell("zsh", "/bin/zsh"),
        shell("WSL", "C:/Windows/System32/wsl.exe"),
    ];
    let pairs = |list: Vec<(SharedString, SharedString)>| -> Vec<(String, String)> {
        list.into_iter()
            .map(|(v, l)| (v.to_string(), l.to_string()))
            .collect()
    };
    let pair = |v: &str, l: &str| (v.to_string(), l.to_string());
    assert_eq!(
        pairs(shell_choices("zsh", &shells, false, None)),
        [
            pair("", "Default (zsh)"),
            pair("zsh", "zsh"),
            pair("WSL", "WSL")
        ]
    );
    assert_eq!(
        pairs(shell_choices("zsh", &shells, true, None)),
        [pair("", "Default (zsh)"), pair("zsh", "zsh")],
        "an account's leaves WSL out"
    );
    let gone = pairs(shell_choices("zsh", &shells, false, Some("fish")));
    assert_eq!(gone.last(), Some(&pair("fish", "fish (not available)")));
    let account = pairs(shell_choices("zsh", &shells, true, Some("WSL")));
    assert_eq!(account.last(), Some(&pair("WSL", "WSL (not available)")));
    let found = pairs(shell_choices("zsh", &shells, false, Some("zsh")));
    assert_eq!(found.len(), 3, "a shell found isn't listed twice");
}

#[test]
fn a_rows_description_says_its_shell() {
    let agents = crate::agents::effective(
        &crate::config::parse(
            "[agents]\nclaude = { shell = \"PowerShell 7\" }\n\
             mine = { name = \"Mine\", command = \"mine --go\", shell = \"WSL\" }\n",
        )
        .unwrap()
        .agents,
        &[],
        None,
    );
    let of = |id: &str| agents.iter().find(|a| a.id == id).unwrap();
    let (claude, mine) = (of("claude"), of("mine"));
    assert_eq!(
        super::agents::description(claude, true, true),
        "claude · in PowerShell 7"
    );
    assert_eq!(
        super::agents::description(claude, true, false),
        "claude · in PowerShell 7 (not available)"
    );
    assert_eq!(
        super::agents::description(mine, true, true),
        "Yours · mine --go · in WSL"
    );
}
