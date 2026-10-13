//! One repository and the `git` program that runs on it. Every method runs one command to
//! completion on the calling thread (the UI runs them on its background executor), never waits
//! for input, and gives back typed values or a [`GitError`] with the command and its stderr.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::diff::{self, FileDiff, parse_diff};
use crate::parse::{self, Branch, Change, Commit, CommitFile, Entry, Operation, Stash, Status};

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
    /// The line that says why: what would have asked for a key or a password, a rejected ref,
    /// else git's fatal or error line (its hints come first), else the first line.
    pub fn first_line(&self) -> &str {
        let lines = || self.stderr.lines().map(str::trim).filter(|l| !l.is_empty());
        lines()
            .find(|l| parse::needs_input(l))
            .or_else(|| lines().find(|l| l.starts_with("! [")))
            .or_else(|| lines().find(|l| l.starts_with("fatal:") || l.starts_with("error:")))
            .or_else(|| lines().next())
            .unwrap_or("")
    }

    /// It was cancelled ([`Repo::with_cancel`]), not refused.
    pub fn cancelled(&self) -> bool {
        self.code.is_none() && !self.missing && self.stderr == CANCELLED
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

/// A diff over this isn't shown: the view would stall the window (the editor opens files up to
/// 4 MB editable, too).
pub const DIFF_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Repo {
    program: String,
    /// Set from elsewhere: the running command is killed.
    cancel: Option<Arc<AtomicBool>>,
    /// The working tree's top folder.
    pub root: PathBuf,
    /// Where HEAD, MERGE_HEAD and the like live (a worktree's own, for a linked worktree).
    pub git_dir: PathBuf,
}

/// "git commit -m 'it'\''s done'": the command as a POSIX shell would take it, argument for
/// argument (what "Run in a terminal" types). bash, zsh and fish read it alike.
fn shown(program: &str, args: &[&str]) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    std::iter::once(program)
        .chain(args.iter().copied())
        .map(|a| {
            if !a.is_empty() && a.chars().all(safe) {
                a.to_string()
            } else {
                format!("'{}'", a.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs `program args` in `dir`: no terminal, no prompt, no editor, English messages.
fn run_in(
    program: &str,
    dir: &Path,
    args: &[&str],
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>, GitError> {
    let command = shown(program, args);
    let mut c = Command::new(program);
    c.args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("LC_ALL", "C")
        // Reads never take index.lock: a `git add` in a terminal at that moment would fail.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // No console window flashing behind the app.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let couldnt = |e: io::Error| GitError {
        missing: e.kind() == io::ErrorKind::NotFound,
        command: command.clone(),
        code: None,
        stderr: format!("Couldn't run {program}: {e}"),
    };
    let out = match cancel {
        None => c.output().map_err(couldnt)?,
        Some(cancel) => match until_cancelled(&mut c, cancel).map_err(couldnt)? {
            Some(out) => out,
            None => {
                return Err(GitError {
                    command,
                    code: None,
                    stderr: CANCELLED.to_string(),
                    missing: false,
                });
            }
        },
    };
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

/// What a cancelled command's error says.
const CANCELLED: &str = "Cancelled";

/// Runs `c` to the end, or kills it once `cancel` is set (None then). Its output is read on
/// threads meanwhile, so a chatty command never blocks on a full pipe.
fn until_cancelled(c: &mut Command, cancel: &AtomicBool) -> io::Result<Option<Output>> {
    use std::io::Read;
    let mut child = c.spawn()?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel.load(Ordering::Relaxed) {
            // ponytail: kills git, not what it started (an ssh asking on a terminal stays until
            // that terminal closes); a process group needs a dependency or unsafe.
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(Some(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    }))
}

impl Repo {
    /// "git version 2.55.0", or why `program` can't run.
    pub fn version(program: &str) -> Result<String, GitError> {
        let out = run_in(program, Path::new("."), &["--version"], None)?;
        Ok(String::from_utf8_lossy(&out).trim().to_string())
    }

    /// The repository holding `folder`: Ok(None) when there is none.
    pub fn discover(program: &str, folder: &Path) -> Result<Option<Repo>, GitError> {
        let out = match run_in(
            program,
            folder,
            &["rev-parse", "--show-toplevel", "--absolute-git-dir"],
            None,
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
            cancel: None,
            root: PathBuf::from(root),
            git_dir: PathBuf::from(git_dir),
        }))
    }

    /// `git init` in `folder`.
    pub fn init(program: &str, folder: &Path) -> Result<Repo, GitError> {
        run_in(program, folder, &["init", "-q"], None)?;
        Self::discover(program, folder)?.ok_or_else(|| GitError {
            command: shown(program, &["init", "-q"]),
            code: None,
            stderr: "git init made no repository".to_string(),
            missing: false,
        })
    }

    /// `git clone -- url into`, then the repository there. `into`'s parent is made when
    /// missing. A clone that fails or is cancelled leaves no folder behind when there was none
    /// before (git can't clean up after a kill).
    pub fn clone_into(
        program: &str,
        url: &str,
        into: &Path,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Repo, GitError> {
        let dest = into.to_string_lossy().into_owned();
        let command = shown(program, &["clone", "--", url, &dest]);
        let parent = into.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| GitError {
            command: command.clone(),
            code: None,
            stderr: format!("Couldn't make {}: {e}", parent.display()),
            missing: false,
        })?;
        let existed = into.exists();
        if let Err(e) = run_in(
            program,
            parent,
            &["clone", "-q", "--", url, &dest],
            cancel.as_deref(),
        ) {
            if !existed {
                let _ = std::fs::remove_dir_all(into);
            }
            return Err(e);
        }
        Self::discover(program, into)?.ok_or(GitError {
            command,
            code: None,
            stderr: "git clone made no repository".to_string(),
            missing: false,
        })
    }

    /// This repository, its commands killed once `cancel` is set (fetch, pull and push can
    /// hang on a network or on ssh asking on a terminal).
    pub fn with_cancel(&self, cancel: Arc<AtomicBool>) -> Repo {
        Repo {
            cancel: Some(cancel),
            ..self.clone()
        }
    }

    /// Runs `git args` in the root; Ok is stdout.
    pub fn run(&self, args: &[&str]) -> Result<Vec<u8>, GitError> {
        run_in(&self.program, &self.root, args, self.cancel.as_deref())
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
            None,
        )?;
        run_in(
            program,
            Path::new("."),
            &["config", "--global", "user.email", email],
            None,
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
                command: shown("rm", &["--", &e.path]),
                code: None,
                stderr: err.to_string(),
                missing: false,
            })?;
        }
        Ok(())
    }

    /// Appends `path` to the root's .gitignore (made if missing) as a pattern for that path
    /// alone: anchored at the root, its wildcards escaped. The file's other bytes stay as they
    /// are; one that can't be read is left alone.
    pub fn ignore(&self, path: &str) -> io::Result<()> {
        let file = self.root.join(".gitignore");
        let mut bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        let mut line = String::from("/");
        for c in path.chars() {
            if "\\*?[".contains(c) {
                line.push('\\');
            }
            line.push(c);
        }
        // A trailing space is dropped unless escaped.
        if line.ends_with(' ') {
            line.insert(line.len() - 1, '\\');
        }
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        std::fs::write(file, bytes)
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

    /// `git add --all`: every change, new and deleted files included (Commit all changes).
    pub fn stage_all(&self) -> Result<(), GitError> {
        self.run(&["add", "--all"]).map(drop)
    }

    /// The commit `short` is on the current branch's upstream: pushed, so it can't be taken
    /// back for others. False with no upstream, one gone from the server (its branch deleted
    /// and pruned), and on a detached HEAD.
    pub fn on_upstream(&self, short: &str) -> Result<bool, GitError> {
        if self.run(&["rev-parse", "--verify", "-q", "@{u}"]).is_err() {
            return Ok(false);
        }
        match self.run(&["merge-base", "--is-ancestor", short, "@{u}"]) {
            Ok(_) => Ok(true),
            Err(e) if e.code == Some(1) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// HEAD's full hash (an error before the first commit).
    pub fn head(&self) -> Result<String, GitError> {
        Ok(self
            .text(&["rev-parse", "--verify", "HEAD"])?
            .trim()
            .to_string())
    }

    /// Takes the last commit back, its changes left staged, and gives its message (for the
    /// fields); None, with nothing done, when HEAD isn't `short` any more: a commit made since is
    /// never the one undone. HEAD goes back to `replaced` after an Amend (the commit it replaced,
    /// read before the amend), else to the parent; the repository's first commit has none, so
    /// the branch goes. `update-ref` moves it only from the commit checked here (a commit made
    /// in between is never lost) and leaves the index, as a soft reset does.
    pub fn undo_commit(
        &self,
        short: &str,
        replaced: Option<&str>,
    ) -> Result<Option<String>, GitError> {
        let Ok(head) = self.head() else {
            return Ok(None);
        };
        if short.is_empty() || !head.starts_with(short) {
            return Ok(None);
        }
        let message = self
            .text(&["log", "-1", "--format=%B", &head])?
            .trim_end()
            .to_string();
        let target = match replaced {
            Some(r) => Some(r.to_string()),
            None => self
                .text(&["rev-parse", "--verify", "-q", &format!("{head}~1")])
                .ok()
                .map(|t| t.trim().to_string()),
        };
        match target {
            Some(t) => self.run(&[
                "update-ref",
                "-m",
                "templum: undo commit",
                "HEAD",
                &t,
                &head,
            ])?,
            None => self.run(&["update-ref", "-d", "HEAD", &head])?,
        };
        Ok(Some(message))
    }

    /// `git apply` of `patch` (a `hunk_patch`): on the index with `cached`, the other way with
    /// `reverse`. Written to a temporary file, since `run` gives git no stdin. A patch that no
    /// longer applies (the file changed since it was read) is git's error.
    pub fn apply_patch(&self, patch: &str, cached: bool, reverse: bool) -> Result<(), GitError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let file = std::env::temp_dir().join(format!(
            "templum-{}-{}.patch",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&file, patch).map_err(|e| GitError {
            command: shown(&self.program, &["apply"]),
            code: None,
            stderr: format!("Couldn't write the patch: {e}"),
            missing: false,
        })?;
        let path = file.to_string_lossy().into_owned();
        // nowarn: a user's apply.whitespace=error would refuse a hunk with a trailing space.
        let mut args = vec!["apply", "--whitespace=nowarn"];
        if cached {
            args.push("--cached");
        }
        if reverse {
            args.push("--reverse");
        }
        args.push(&path);
        let result = self.run(&args).map(drop);
        let _ = std::fs::remove_file(&file);
        result
    }
}
/// The working tree's diff: the unstaged changes, untracked files included (built with a new
/// file's headers, after the tracked ones, in the status's order), and the staged ones.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkingDiff {
    pub unstaged: Vec<FileDiff>,
    pub staged: Vec<FileDiff>,
}

/// What one read of the working diff takes of the untracked files, at most: past either, a
/// file is listed unread (`FileDiff::unread`) until it is asked for in `whole`.
const UNTRACKED_FILES: usize = 200;
const UNTRACKED_BYTES: u64 = DIFF_LIMIT as u64;

/// More context lines than any file has: git shows the file whole.
const WHOLE: &str = "-U999999999";

impl Repo {
    /// `git diff` and `git diff --cached`, parsed (`-M`; `-w` with `ignore_whitespace`), each
    /// file's working size and line count counted. The files in `whole` (`(staged, path)`) are
    /// read again with every line as context, for the panel to show them whole.
    pub fn working_diff(
        &self,
        status: &Status,
        ignore_whitespace: bool,
        whole: &[(bool, String)],
    ) -> Result<WorkingDiff, GitError> {
        let mut unstaged = self.side_diff(false, ignore_whitespace, whole)?;
        // An agent can make thousands of new files (a build, an unpacked folder): one read
        // takes the first ones, the others are listed unread until asked for.
        let (mut read, mut bytes) = (0, 0u64);
        for e in status
            .entries
            .iter()
            .filter(|e| e.unstaged == Some(Change::Untracked))
        {
            let size = std::fs::metadata(self.root.join(&e.path)).map_or(0, |m| m.len());
            let asked = whole.iter().any(|(staged, p)| !staged && *p == e.path);
            // Over DIFF_LIMIT it is never read (Too large to show): it costs nothing.
            let huge = size > DIFF_LIMIT as u64;
            if huge || asked || (read < UNTRACKED_FILES && bytes + size <= UNTRACKED_BYTES) {
                if !huge {
                    read += 1;
                    bytes += size;
                }
                unstaged.push(self.untracked_diff(&e.path));
            } else {
                let mut file = FileDiff::empty();
                file.new_path = Some(e.path.clone());
                file.status = Change::Added;
                file.unread = true;
                unstaged.push(file);
            }
        }
        let staged = self.side_diff(true, ignore_whitespace, whole)?;
        let mut all = WorkingDiff { unstaged, staged };
        for file in all.unstaged.iter_mut().chain(all.staged.iter_mut()) {
            self.count(file);
        }
        Ok(all)
    }

    fn side_diff(
        &self,
        staged: bool,
        ignore_whitespace: bool,
        whole: &[(bool, String)],
    ) -> Result<Vec<FileDiff>, GitError> {
        // quotePath off: a name with non-ASCII letters comes as itself, not as "caf\303\251".
        // The prefixes are set, and no external diff runs: a user's diff.mnemonicPrefix,
        // diff.noprefix or diff.external would change what the parser reads.
        let mut args = vec![
            "-c",
            "core.quotePath=false",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-M",
        ];
        if staged {
            args.push("--cached");
        }
        if ignore_whitespace {
            args.push("-w");
        }
        let mut files = parse_diff(&self.text(&args)?, DIFF_LIMIT);
        for (_, path) in whole.iter().filter(|(s, _)| *s == staged) {
            let Some(i) = files.iter().position(|f| f.path() == path) else {
                continue;
            };
            let mut args = args.clone();
            args.push(WHOLE);
            args.push("--");
            if files[i].status == Change::Renamed
                && let Some(from) = files[i].old_path.as_deref()
            {
                args.push(from);
            }
            args.push(path);
            if let Some(file) = parse_diff(&self.text(&args)?, DIFF_LIMIT).pop() {
                files[i] = file;
            }
        }
        Ok(files)
    }

    /// An untracked file as a new file's diff: its text as added lines; binary, or too large
    /// past `DIFF_LIMIT` (not read).
    fn untracked_diff(&self, path: &str) -> FileDiff {
        let full = self.root.join(path);
        let size = std::fs::metadata(&full).map_or(0, |m| m.len());
        if size > DIFF_LIMIT as u64 {
            let mut file = FileDiff::empty();
            file.new_path = Some(path.to_string());
            file.status = Change::Added;
            file.too_large = true;
            return file;
        }
        let bytes = std::fs::read(&full).unwrap_or_default();
        let text = if bytes.iter().take(8 * 1024).any(|b| *b == 0) {
            diff::new_binary_diff(path)
        } else {
            diff::new_file_diff(path, &String::from_utf8_lossy(&bytes))
        };
        parse_diff(&text, DIFF_LIMIT).pop().expect("one file")
    }

    /// The working file's size and line count: the file row's "12 KB", and the fold after
    /// its last hunk.
    fn count(&self, file: &mut FileDiff) {
        let Some(path) = &file.new_path else {
            return;
        };
        let full = self.root.join(path);
        let Ok(meta) = std::fs::metadata(&full) else {
            return;
        };
        file.bytes = Some(meta.len());
        if file.binary || file.too_large || file.unread || meta.len() > DIFF_LIMIT as u64 {
            return;
        }
        // ponytail: reads every changed file on each poll to count its lines; cache by mtime
        // if a tree with many large changed files stalls the background executor.
        if let Ok(bytes) = std::fs::read(&full) {
            let n = bytes.iter().filter(|b| **b == b'\n').count()
                + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
            file.lines = Some(n as u32);
        }
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

    pub fn push(&self) -> Result<(), GitError> {
        self.run(&["push"]).map(drop)
    }

    /// `git push -u <remote> <branch>`: the branch goes to the server and follows it from then.
    pub fn publish(&self, remote: &str, branch: &str) -> Result<(), GitError> {
        self.run(&["push", "-u", remote, branch]).map(drop)
    }

    /// Pushes `branch`, not the one checked out, to its upstream: the remote and the ref it
    /// follows are read from its config, so a branch tracking another name goes there.
    pub fn push_branch(&self, branch: &str) -> Result<(), GitError> {
        let get = |key: String| -> Result<String, GitError> {
            Ok(self.text(&["config", "--get", &key])?.trim().to_string())
        };
        let remote = get(format!("branch.{branch}.remote"))?;
        let merge = get(format!("branch.{branch}.merge"))?;
        self.run(&["push", &remote, &format!("refs/heads/{branch}:{merge}")])
            .map(drop)
    }

    /// The remotes' names, as `git remote` lists them.
    pub fn remotes(&self) -> Result<Vec<String>, GitError> {
        Ok(self
            .text(&["remote"])?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// The branch a pull request would go into: the one `origin/HEAD` names, else `main`, else
    /// `master`; the local one when there is one ("main"), else origin's ("origin/main"). None
    /// when there is none of them.
    pub fn default_branch(&self) -> Option<String> {
        let has = |r: String| self.run(&["rev-parse", "--verify", "-q", &r]).is_ok();
        let mut names = Vec::new();
        // parse_branches leaves origin/HEAD out: it is its own command.
        if let Ok(head) = self.text(&["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"])
            && let Some(name) = head.trim().strip_prefix("origin/")
        {
            names.push(name.to_string());
        }
        names.extend(["main".to_string(), "master".to_string()]);
        names.into_iter().find_map(|name| {
            if has(format!("refs/heads/{name}")) {
                Some(name)
            } else if has(format!("refs/remotes/origin/{name}")) {
                Some(format!("origin/{name}"))
            } else {
                None
            }
        })
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

    /// Local and remote branches, sorted as git lists them (local first).
    pub fn branches(&self) -> Result<Vec<Branch>, GitError> {
        let format = format!("--format={}", parse::BRANCH_FORMAT);
        let out = self.run(&["for-each-ref", &format, "refs/heads", "refs/remotes"])?;
        Ok(parse::parse_branches(&out))
    }

    pub fn checkout(&self, name: &str) -> Result<(), GitError> {
        self.run(&["checkout", "-q", name]).map(drop)
    }

    /// "origin/x": a local x tracking it, checked out.
    pub fn checkout_remote(&self, name: &str) -> Result<(), GitError> {
        self.run(&["checkout", "-q", "--track", name]).map(drop)
    }

    pub fn rename_branch(&self, from: &str, to: &str) -> Result<(), GitError> {
        self.check_ref_format(to)?;
        self.run(&["branch", "-m", from, to]).map(drop)
    }

    /// Merges `name` into the current branch.
    pub fn merge(&self, name: &str) -> Result<(), GitError> {
        self.run(&["merge", "--no-edit", name]).map(drop)
    }

    pub fn delete_branch(&self, name: &str, force: bool) -> Result<(), GitError> {
        self.run(&["branch", if force { "-D" } else { "-d" }, name])
            .map(drop)
    }

    /// The commits only `branch` has (on no other branch, local or remote): what deleting it
    /// loses.
    // ponytail: a name holding glob characters (`*?[`) is read as a pattern by --exclude;
    // escape it if such names show up.
    pub fn unmerged_count(&self, branch: &str) -> Result<u32, GitError> {
        let out = self.text(&[
            "rev-list",
            "--count",
            &format!("refs/heads/{branch}"),
            "--not",
            &format!("--exclude={branch}"),
            "--branches",
            "--remotes",
        ])?;
        Ok(out.trim().parse().unwrap_or(0))
    }

    /// `git stash push -m message`, with untracked files when asked.
    pub fn stash_push(&self, message: &str, untracked: bool) -> Result<(), GitError> {
        let mut args = vec!["stash", "push", "-q", "-m", message];
        if untracked {
            args.push("-u");
        }
        self.run(&args).map(drop)
    }

    pub fn stashes(&self) -> Result<Vec<Stash>, GitError> {
        let format = format!("--format={}", parse::STASH_FORMAT);
        let out = self.run(&["stash", "list", "-z", &format])?;
        Ok(parse::parse_stashes(&out))
    }

    /// `stash@{N}` when it is still `stash`; an error when the list changed since it was read.
    fn stash_ref(&self, stash: &Stash) -> Result<String, GitError> {
        let name = format!("stash@{{{}}}", stash.index);
        let now = self
            .text(&["rev-parse", "--verify", "-q", &name])
            .unwrap_or_default();
        if now.trim() == stash.hash {
            Ok(name)
        } else {
            Err(GitError {
                command: shown(&self.program, &["stash", "list"]),
                code: None,
                stderr: "error: the stash list changed since it was read; it is read again"
                    .to_string(),
                missing: false,
            })
        }
    }

    pub fn stash_apply(&self, stash: &Stash) -> Result<(), GitError> {
        let name = self.stash_ref(stash)?;
        self.run(&["stash", "apply", "-q", &name]).map(drop)
    }

    pub fn stash_pop(&self, stash: &Stash) -> Result<(), GitError> {
        let name = self.stash_ref(stash)?;
        self.run(&["stash", "pop", "-q", &name]).map(drop)
    }

    pub fn stash_drop(&self, stash: &Stash) -> Result<(), GitError> {
        let name = self.stash_ref(stash)?;
        self.run(&["stash", "drop", "-q", &name]).map(drop)
    }

    /// The stash's diff.
    pub fn stash_show(&self, stash: &Stash) -> Result<String, GitError> {
        let name = self.stash_ref(stash)?;
        self.text(&["stash", "show", "-p", "--no-color", &name])
    }
}

/// The work tree holding `file`, without running git: the nearest folder above it with a `.git`
/// (a folder, or a file in a worktree or a submodule).
pub fn root_of(file: &Path) -> Option<&Path> {
    file.ancestors().skip(1).find(|d| d.join(".git").exists())
}

/// The folder `git clone <url>` would make: the URL's last part without `.git` ("app" for
/// `git@github.com:me/app.git` and `https://host/me/app/`).
pub fn clone_name(url: &str) -> String {
    let url = url.trim().trim_end_matches(['/', '\\']);
    let url = url.strip_suffix(".git").unwrap_or(url);
    url.rsplit(['/', ':', '\\'])
        .next()
        .unwrap_or("")
        .to_string()
}

/// Where a clone of `url` into `dest` goes: `dest` itself when it is an empty folder or doesn't
/// exist yet, else a folder named after the repository inside it.
pub fn clone_target(dest: &Path, url: &str) -> PathBuf {
    let empty = std::fs::read_dir(dest).map_or(true, |mut d| d.next().is_none());
    if empty {
        dest.to_path_buf()
    } else {
        dest.join(clone_name(url))
    }
}
