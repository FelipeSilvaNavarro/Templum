//! An account's command: one file in the commands folder that runs its agent on its folder.
//! Templum writes, rewrites and deletes only the files whose second line is its marker for
//! that account and data folder; any other file is never touched. See docs/specs/accounts.md.

use std::io;
use std::path::Path;

/// Words of bash, zsh, fish and PowerShell that a command or a function of that name would break.
pub const SHELL_WORDS: &[&str] = &[
    "alias",
    "autoload",
    "bg",
    "bind",
    "bindkey",
    "break",
    "builtin",
    "caller",
    "case",
    "catch",
    "cd",
    "class",
    "command",
    "compgen",
    "complete",
    "contains",
    "continue",
    "count",
    "data",
    "declare",
    "dirs",
    "disown",
    "do",
    "done",
    "echo",
    "elif",
    "else",
    "emit",
    "emulate",
    "enable",
    "end",
    "enum",
    "esac",
    "eval",
    "exec",
    "exit",
    "export",
    "false",
    "fc",
    "fg",
    "fi",
    "filter",
    "finally",
    "for",
    "foreach",
    "function",
    "functions",
    "getopts",
    "hash",
    "help",
    "history",
    "if",
    "in",
    "integer",
    "jobs",
    "kill",
    "let",
    "local",
    "logout",
    "mapfile",
    "noglob",
    "not",
    "or",
    "param",
    "popd",
    "print",
    "printf",
    "process",
    "pushd",
    "pwd",
    "read",
    "readarray",
    "readonly",
    "repeat",
    "return",
    "select",
    "set",
    "setopt",
    "shift",
    "shopt",
    "source",
    "status",
    "string",
    "suspend",
    "switch",
    "test",
    "then",
    "throw",
    "time",
    "times",
    "trap",
    "true",
    "try",
    "type",
    "typeset",
    "ulimit",
    "umask",
    "unalias",
    "unset",
    "unsetopt",
    "until",
    "using",
    "wait",
    "whence",
    "where",
    "which",
    "while",
    "zle",
    "zmodload",
    "zstyle",
    "and",
    "begin",
];

/// A file bigger than this isn't a command of Templum's.
const MARKER_LIMIT: u64 = 4096;

/// What one account's command needs to know.
pub struct Command<'a> {
    /// The account's id: the name typed in a terminal.
    pub id: &'a str,
    /// Its agent's title ("Claude Code").
    pub title: &'a str,
    /// The first word of its agent's command.
    pub program: &'a str,
    /// The variable that names the agent's folder.
    pub var: &'a str,
    pub folder: &'a Path,
    /// Templum's data folder: two Templums keep their commands apart by it.
    pub data: &'a Path,
}

fn names(id: &str, windows: bool) -> Vec<String> {
    let mut names = vec![id.to_string()];
    if windows {
        names.push(format!("{id}.cmd"));
    }
    names
}

fn word(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
}

/// Each file of the command, by name, with its text: a script for sh (and Git Bash on Windows),
/// and on Windows a `.cmd` for cmd and PowerShell.
pub fn files(c: &Command, windows: bool) -> Vec<(String, String)> {
    let marker = format!("templum-account {} {}", c.id, c.data.display());
    let made = |arrow: &str| {
        format!(
            "Made by Templum: opens {} on its account {} (Settings {arrow} Agents {arrow} Accounts).",
            c.title, c.id
        )
    };
    let program = if word(c.program) {
        c.program.to_string()
    } else {
        format!("'{}'", c.program.replace('\'', "'\\''"))
    };
    let sh = format!(
        "#!/bin/sh\n# {marker}\n# {}\n# Deleted with the account; Templum writes it again when it changes.\nexport {}='{}'\nexec {program} \"$@\"\n",
        made("▸"),
        c.var,
        c.folder.display().to_string().replace('\'', "'\\''"),
    );
    let mut files = vec![(c.id.to_string(), sh)];
    if windows {
        let program = if word(c.program) {
            c.program.to_string()
        } else {
            format!("\"{}\"", c.program)
        };
        let cmd = format!(
            "@echo off\r\nrem {marker}\r\nrem {}\r\nrem Deleted with the account; Templum writes it again when it changes.\r\nsetlocal\r\nset \"{}={}\"\r\n{program} %*\r\nexit /b %errorlevel%\r\n",
            made(">"),
            c.var,
            c.folder.display().to_string().replace('%', "%%"),
        );
        files.push((format!("{}.cmd", c.id), cmd));
    }
    files
}

/// Whether `path` is the command Templum wrote for `id` with `data` as its data folder.
pub fn is_own(path: &Path, id: &str, data: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() > MARKER_LIMIT {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Some(line) = text.lines().nth(1) else {
        return false;
    };
    let line = line
        .strip_prefix("# ")
        .or_else(|| line.strip_prefix("rem "));
    line == Some(&format!("templum-account {id} {}", data.display()))
}

/// Writes the command into `dir`, made when missing. Whether anything was written: a file whose
/// text is already right is left alone. `AlreadyExists` when a file of that name isn't Templum's
/// own, with nothing written.
pub fn write(dir: &Path, c: &Command) -> io::Result<bool> {
    let breaks = |p: &Path| p.to_string_lossy().contains(['\n', '\r']);
    if breaks(c.data) || breaks(c.folder) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a line break in a folder's name",
        ));
    }
    std::fs::create_dir_all(dir)?;
    let files = files(c, cfg!(windows));
    for (name, _) in &files {
        let path = dir.join(name);
        if path.symlink_metadata().is_ok() && !is_own(&path, c.id, c.data) {
            let why = format!("{} is not a command of Templum's", path.display());
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, why));
        }
    }
    let mut wrote = false;
    for (name, text) in files {
        let path = dir.join(&name);
        if std::fs::read_to_string(&path).is_ok_and(|now| now == text) {
            continue;
        }
        let new = dir.join(format!("{name}.templum-new"));
        std::fs::write(&new, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&new, &path)?;
        wrote = true;
    }
    Ok(wrote)
}

/// Removes the files of `id`'s command that are Templum's own for `data`; any other stays.
pub fn remove(dir: &Path, id: &str, data: &Path) -> io::Result<()> {
    for name in names(id, cfg!(windows)) {
        let path = dir.join(name);
        if is_own(&path, id, data) {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claudinho<'a>(folder: &'a Path, data: &'a Path) -> Command<'a> {
        Command {
            id: "claudinho",
            title: "Claude Code",
            program: "claude",
            var: "CLAUDE_CONFIG_DIR",
            folder,
            data,
        }
    }

    #[test]
    fn the_command_runs_its_agent_on_its_folder() {
        let (data, folder) = (
            Path::new("/d/it's 100%"),
            Path::new("/d/it's 100%/accounts/claudinho"),
        );
        let sh = "#!/bin/sh\n# templum-account claudinho /d/it's 100%\n\
            # Made by Templum: opens Claude Code on its account claudinho (Settings ▸ Agents ▸ Accounts).\n\
            # Deleted with the account; Templum writes it again when it changes.\n\
            export CLAUDE_CONFIG_DIR='/d/it'\\''s 100%/accounts/claudinho'\nexec claude \"$@\"\n";
        assert_eq!(
            files(&claudinho(folder, data), false),
            [("claudinho".to_string(), sh.to_string())]
        );
        let windows = files(&claudinho(folder, data), true);
        assert_eq!(windows[0], ("claudinho".to_string(), sh.to_string()));
        let cmd = "@echo off\r\nrem templum-account claudinho /d/it's 100%\r\n\
            rem Made by Templum: opens Claude Code on its account claudinho (Settings > Agents > Accounts).\r\n\
            rem Deleted with the account; Templum writes it again when it changes.\r\n\
            setlocal\r\nset \"CLAUDE_CONFIG_DIR=/d/it's 100%%/accounts/claudinho\"\r\nclaude %*\r\nexit /b %errorlevel%\r\n";
        assert_eq!(windows[1], ("claudinho.cmd".to_string(), cmd.to_string()));
    }

    #[test]
    fn a_program_with_a_space_is_quoted() {
        let c = Command {
            program: "/opt/my agent/claude",
            ..claudinho(Path::new("/f"), Path::new("/d"))
        };
        assert!(
            files(&c, false)[0]
                .1
                .contains("exec '/opt/my agent/claude' \"$@\"\n")
        );
        assert!(
            files(&c, true)[1]
                .1
                .contains("\"/opt/my agent/claude\" %*\r\n")
        );
    }

    #[test]
    fn a_command_is_written_and_known_as_templums() {
        let (dir, d) = (tempfile::tempdir().unwrap(), Path::new("/d"));
        let c = claudinho(Path::new("/d/accounts/claudinho"), d);
        assert!(write(dir.path(), &c).unwrap());
        for (file, _) in files(&c, cfg!(windows)) {
            assert!(dir.path().join(file).exists());
        }
        assert!(is_own(&dir.path().join("claudinho"), "claudinho", d));
        assert!(!is_own(&dir.path().join("claudinho"), "claudio", d));
        assert!(!is_own(
            &dir.path().join("claudinho"),
            "claudinho",
            Path::new("/other")
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join("claudinho"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert!(
            left.iter()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".templum-new"))
        );
    }

    #[test]
    fn a_file_of_someone_else_is_never_touched() {
        let (dir, d) = (tempfile::tempdir().unwrap(), Path::new("/d"));
        let mine = dir.path().join("claudinho");
        std::fs::write(&mine, "#!/bin/sh\necho mine\n").unwrap();
        let c = claudinho(Path::new("/d/accounts/claudinho"), d);
        assert_eq!(
            write(dir.path(), &c).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            std::fs::read_to_string(&mine).unwrap(),
            "#!/bin/sh\necho mine\n"
        );
        remove(dir.path(), "claudinho", d).unwrap();
        assert_eq!(
            std::fs::read_to_string(&mine).unwrap(),
            "#!/bin/sh\necho mine\n"
        );

        // The command another data folder wrote (an installed Templum and a --dev one).
        std::fs::remove_file(&mine).unwrap();
        let other = Command {
            data: Path::new("/other"),
            ..claudinho(Path::new("/other/accounts/claudinho"), d)
        };
        write(dir.path(), &other).unwrap();
        let theirs = std::fs::read_to_string(&mine).unwrap();
        assert_eq!(
            write(dir.path(), &c).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        remove(dir.path(), "claudinho", d).unwrap();
        assert_eq!(std::fs::read_to_string(&mine).unwrap(), theirs);
    }

    #[test]
    fn it_is_written_again_only_when_its_text_differs() {
        let (dir, d) = (tempfile::tempdir().unwrap(), Path::new("/d"));
        let c = claudinho(Path::new("/d/accounts/claudinho"), d);
        assert!(write(dir.path(), &c).unwrap());
        assert!(!write(dir.path(), &c).unwrap());
        let moved = claudinho(Path::new("/elsewhere"), d);
        assert!(write(dir.path(), &moved).unwrap());
        let text = std::fs::read_to_string(dir.path().join("claudinho")).unwrap();
        assert!(
            text.contains("export CLAUDE_CONFIG_DIR='/elsewhere'\n"),
            "{text}"
        );
    }

    #[test]
    fn removing_one_leaves_the_others() {
        let (dir, d) = (tempfile::tempdir().unwrap(), Path::new("/d"));
        let (a, b) = (
            claudinho(Path::new("/d/a"), d),
            Command {
                id: "claudio",
                ..claudinho(Path::new("/d/b"), d)
            },
        );
        write(dir.path(), &a).unwrap();
        write(dir.path(), &b).unwrap();
        remove(dir.path(), "claudinho", d).unwrap();
        for (file, _) in files(&a, true) {
            assert!(!dir.path().join(file).exists());
        }
        for (file, _) in files(&b, cfg!(windows)) {
            assert!(dir.path().join(file).exists());
        }
    }

    #[test]
    fn a_line_break_in_a_path_would_break_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let c = claudinho(Path::new("/d/accounts/claudinho"), Path::new("/d\n/x"));
        assert_eq!(
            write(dir.path(), &c).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
