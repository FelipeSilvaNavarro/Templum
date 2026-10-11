//! One repository and the `git` program that runs on it. Every method runs one command to
//! completion on the calling thread (the UI runs them on its background executor), never waits
//! for input, and gives back typed values or a [`GitError`] with the command and its stderr.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::parse::{self, Change, Commit, CommitFile, Entry, Operation, Status};

/// A command that failed: which one, how, and what git said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitError {
    /// "git push origin main", as a shell would take it.
    pub command: String,
    /// The exit code; None when it couldn't run or was killed by a signal.
    pub code: Option<i32>,
    pub stderr: String,
    /// The program isn't installed (or isn't on PATH).
    pub missing: bool,
}

impl GitError {
    pub fn first_line(&self) -> &str {
        self.stderr
            .lines()
            // Git's hints come before the fatal line; the fatal line says why.
            .find(|l| l.starts_with("fatal:") || l.starts_with("error:"))
            .or_else(|| self.stderr.lines().next())
            .unwrap_or("")
    }

    /// It failed because git would have asked for a password or a key.
    pub fn needs_input(&self) -> bool {
        parse::needs_input(&self.stderr)
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.command, self.first_line())
    }
}

impl std::error::Error for GitError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    program: String,
    /// The working tree's top folder.
    pub root: PathBuf,
    /// Where HEAD, MERGE_HEAD and the like live (a worktree's own, for a linked worktree).
    pub git_dir: PathBuf,
}

/// "git a 'b c'": the command as a notification or a terminal shows it.
fn shown(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .map(|a| {
            if a.is_empty() || a.contains(' ') {
                format!("'{a}'")
            } else {
                a.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs `program args` in `dir`: no terminal, no prompt, no editor, English messages.
fn run_in(program: &str, dir: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    let command = shown(program, args);
    let mut c = Command::new(program);
    c.args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // No console window flashing behind the app.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = c.output().map_err(|e| GitError {
        missing: e.kind() == io::ErrorKind::NotFound,
        command: command.clone(),
        code: None,
        stderr: format!("Couldn't run {program}: {e}"),
    })?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(GitError {
            command,
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            missing: false,
        })
    }
}

impl Repo {
    /// "git version 2.55.0", or why `program` can't run.
    pub fn version(program: &str) -> Result<String, GitError> {
        let out = run_in(program, Path::new("."), &["--version"])?;
        Ok(String::from_utf8_lossy(&out).trim().to_string())
    }

    /// The repository holding `folder`: Ok(None) when there is none.
    pub fn discover(program: &str, folder: &Path) -> Result<Option<Repo>, GitError> {
        let out = match run_in(
            program,
            folder,
            &["rev-parse", "--show-toplevel", "--absolute-git-dir"],
        ) {
            Ok(out) => out,
            Err(e) if e.stderr.contains("not a git repository") => return Ok(None),
            Err(e) => return Err(e),
        };
        let text = String::from_utf8_lossy(&out);
        let mut lines = text.lines();
        let (Some(root), Some(git_dir)) = (lines.next(), lines.next()) else {
            return Ok(None);
        };
        Ok(Some(Repo {
            program: program.to_string(),
            root: PathBuf::from(root),
            git_dir: PathBuf::from(git_dir),
        }))
    }

    /// `git init` in `folder`.
    pub fn init(program: &str, folder: &Path) -> Result<Repo, GitError> {
        run_in(program, folder, &["init", "-q"])?;
        Self::discover(program, folder)?.ok_or_else(|| GitError {
            command: shown(program, &["init", "-q"]),
            code: None,
            stderr: "git init made no repository".to_string(),
            missing: false,
        })
    }

    /// Runs `git args` in the root; Ok is stdout.
    pub fn run(&self, args: &[&str]) -> Result<Vec<u8>, GitError> {
        run_in(&self.program, &self.root, args)
    }

    fn text(&self, args: &[&str]) -> Result<String, GitError> {
        Ok(String::from_utf8_lossy(&self.run(args)?).into_owned())
    }

    pub fn status(&self) -> Result<Status, GitError> {
        let out = self.run(&["status", "--porcelain=v2", "--branch", "-z", "-uall"])?;
        Ok(parse::parse_status(&out))
    }

    /// A merge, rebase, cherry-pick or revert in progress, read from the git dir's markers.
    pub fn operation(&self) -> Option<Operation> {
        let has = |name: &str| self.git_dir.join(name).exists();
        if has("MERGE_HEAD") {
            Some(Operation::Merge)
        } else if has("rebase-merge") || has("rebase-apply") {
            Some(Operation::Rebase)
        } else if has("CHERRY_PICK_HEAD") {
            Some(Operation::CherryPick)
        } else if has("REVERT_HEAD") {
            Some(Operation::Revert)
        } else {
            None
        }
    }

    /// `user.name` and `user.email`, from any config level; None when either is unset.
    pub fn author(&self) -> Result<Option<(String, String)>, GitError> {
        let get = |key: &str| -> Result<Option<String>, GitError> {
            match self.text(&["config", "--get", key]) {
                Ok(v) if !v.trim().is_empty() => Ok(Some(v.trim().to_string())),
                Ok(_) => Ok(None),
                // Exit 1: the key isn't set.
                Err(e) if e.code == Some(1) => Ok(None),
                Err(e) => Err(e),
            }
        };
        Ok(match (get("user.name")?, get("user.email")?) {
            (Some(name), Some(email)) => Some((name, email)),
            _ => None,
        })
    }

    /// Writes both to the global config.
    pub fn set_author(program: &str, name: &str, email: &str) -> Result<(), GitError> {
        run_in(
            program,
            Path::new("."),
            &["config", "--global", "user.name", name],
        )?;
        run_in(
            program,
            Path::new("."),
            &["config", "--global", "user.email", email],
        )?;
        Ok(())
    }

    /// `git add -A -- paths`: new, changed and deleted files alike.
    pub fn stage(&self, paths: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["add", "-A", "--"];
        args.extend_from_slice(paths);
        self.run(&args).map(drop)
    }

    /// `git reset -q -- paths`: works before the first commit, where `restore --staged` doesn't.
    pub fn unstage(&self, paths: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["reset", "-q", "--"];
        args.extend_from_slice(paths);
        self.run(&args).map(drop)
    }

    /// Throws away unstaged changes: tracked files go back to the index, untracked ones are
    /// deleted.
    pub fn discard(&self, entries: &[&Entry]) -> Result<(), GitError> {
        let tracked: Vec<&str> = entries
            .iter()
            .filter(|e| e.unstaged != Some(Change::Untracked))
            .map(|e| e.path.as_str())
            .collect();
        if !tracked.is_empty() {
            let mut args = vec!["restore", "--worktree", "--"];
            args.extend_from_slice(&tracked);
            self.run(&args)?;
        }
        for e in entries
            .iter()
            .filter(|e| e.unstaged == Some(Change::Untracked))
        {
            // -uall lists files, never folders.
            std::fs::remove_file(self.root.join(&e.path)).map_err(|err| GitError {
                command: format!("rm {}", e.path),
                code: None,
                stderr: err.to_string(),
                missing: false,
            })?;
        }
        Ok(())
    }

    /// Appends `path` as a line of the root's .gitignore (made if missing).
    pub fn ignore(&self, path: &str) -> io::Result<()> {
        let file = self.root.join(".gitignore");
        let mut text = std::fs::read_to_string(&file).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(path);
        text.push('\n');
        std::fs::write(file, text)
    }

    /// The entry's diff as `git diff` prints it: the index against HEAD for `staged`, else the
    /// working tree against the index. An untracked file is shown whole as added, a binary
    /// one says "Binary file".
    pub fn diff(&self, entry: &Entry, staged: bool) -> Result<String, GitError> {
        if entry.unstaged == Some(Change::Untracked) {
            let bytes = std::fs::read(self.root.join(&entry.path)).unwrap_or_default();
            if bytes.iter().take(8 * 1024).any(|b| *b == 0) {
                return Ok("Binary file".to_string());
            }
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            let mut out = format!("+++ b/{}\n@@ -0,0 +1,{} @@\n", entry.path, lines.len());
            for line in lines {
                out.push('+');
                out.push_str(line);
                out.push('\n');
            }
            return Ok(out);
        }
        let mut args = vec!["diff", "--no-color", "-M"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        if let Some(from) = &entry.from {
            args.push(from);
        }
        args.push(&entry.path);
        self.text(&args)
    }

    /// `git commit -m message` (`--amend` with `amend`): the new commit's short hash.
    pub fn commit(&self, message: &str, amend: bool) -> Result<String, GitError> {
        let mut args = vec!["commit", "-q", "-m", message];
        if amend {
            args.push("--amend");
        }
        self.run(&args)?;
        Ok(self
            .text(&["rev-parse", "--short", "HEAD"])?
            .trim()
            .to_string())
    }

    /// HEAD's full message, for Amend.
    pub fn last_message(&self) -> Result<String, GitError> {
        Ok(self
            .text(&["log", "-1", "--format=%B"])?
            .trim_end()
            .to_string())
    }
}
/// Which side of a conflict to keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
}

impl Repo {
    /// Takes one side of a conflicted file (it stays unresolved until staged).
    pub fn resolve(&self, path: &str, side: Side) -> Result<(), GitError> {
        let flag = match side {
            Side::Ours => "--ours",
            Side::Theirs => "--theirs",
        };
        self.run(&["checkout", flag, "--", path]).map(drop)
    }

    /// `git <operation> --abort`.
    pub fn abort(&self, op: Operation) -> Result<(), GitError> {
        self.run(&[op.name(), "--abort"]).map(drop)
    }

    /// `git <operation> --continue`; GIT_EDITOR=true keeps the prepared message.
    pub fn continue_(&self, op: Operation) -> Result<(), GitError> {
        self.run(&[op.name(), "--continue"]).map(drop)
    }
}
/// How `git pull` joins the upstream's commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
    FastForward,
    Merge,
    Rebase,
}

impl Repo {
    pub fn fetch(&self) -> Result<(), GitError> {
        self.run(&["fetch", "--prune"]).map(drop)
    }

    pub fn pull(&self, how: Pull) -> Result<(), GitError> {
        let flag = match how {
            Pull::FastForward => "--ff-only",
            Pull::Merge => "--no-rebase",
            Pull::Rebase => "--rebase",
        };
        self.run(&["pull", flag]).map(drop)
    }

    /// `git push`; with `publish`, `git push -u origin <branch>` for a branch with no upstream.
    pub fn push(&self, publish: Option<&str>) -> Result<(), GitError> {
        match publish {
            Some(branch) => self.run(&["push", "-u", "origin", branch]).map(drop),
            None => self.run(&["push"]).map(drop),
        }
    }
}

impl Repo {
    /// `count` commits of HEAD's history after the first `skip`, newest first. Before the
    /// first commit: none.
    pub fn log(&self, skip: usize, count: usize) -> Result<Vec<Commit>, GitError> {
        let format = format!("--format={}", parse::LOG_FORMAT);
        let date = format!("--date={}", parse::LOG_DATE);
        let skip = format!("--skip={skip}");
        let count = format!("-n{count}");
        match self.run(&["log", "-z", &format, &date, &skip, &count]) {
            Ok(out) => Ok(parse::parse_log(&out)),
            Err(e) if e.stderr.contains("does not have any commits") => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    pub fn commit_files(&self, hash: &str) -> Result<Vec<CommitFile>, GitError> {
        let out = self.run(&[
            "diff-tree",
            "--root",
            "--no-commit-id",
            "-r",
            "-z",
            "-M",
            "--name-status",
            hash,
        ])?;
        Ok(parse::parse_name_status(&out))
    }

    /// One file's diff in a commit.
    pub fn commit_diff(&self, hash: &str, file: &CommitFile) -> Result<String, GitError> {
        let mut args = vec![
            "diff-tree",
            "--root",
            "--no-commit-id",
            "-p",
            "-M",
            "--no-color",
            hash,
            "--",
        ];
        if let Some(from) = &file.from {
            args.push(from);
        }
        args.push(&file.path);
        self.text(&args)
    }

    /// Detached at `hash`.
    pub fn checkout_commit(&self, hash: &str) -> Result<(), GitError> {
        self.run(&["checkout", "-q", "--detach", hash]).map(drop)
    }

    /// A branch named `name` at `from` (HEAD without), checked out. A name git refuses is an
    /// error before anything is made.
    pub fn new_branch(&self, name: &str, from: Option<&str>) -> Result<(), GitError> {
        self.check_ref_format(name)?;
        let mut args = vec!["checkout", "-q", "-b", name];
        if let Some(from) = from {
            args.push(from);
        }
        self.run(&args).map(drop)
    }

    pub fn tag(&self, name: &str, hash: &str) -> Result<(), GitError> {
        self.run(&["tag", name, hash]).map(drop)
    }

    pub fn revert(&self, hash: &str) -> Result<(), GitError> {
        self.run(&["revert", "--no-edit", hash]).map(drop)
    }

    /// Ok when git accepts `name` as a branch name.
    pub fn check_ref_format(&self, name: &str) -> Result<(), GitError> {
        self.run(&["check-ref-format", "--branch", name]).map(drop)
    }
}
