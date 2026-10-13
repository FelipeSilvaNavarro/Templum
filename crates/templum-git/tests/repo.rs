//! Runs the real `git` on a temporary repository. CI's runners have it.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use templum_git::{Back, Change, Operation, Pull, Repo, Side, Synced, clone_name, clone_target};

/// A new repository on `main` with an author set, so commits work on a bare runner.
fn fresh() -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::init("git", dir.path()).expect("git init");
    for args in [
        ["symbolic-ref", "HEAD", "refs/heads/main"].as_slice(),
        &["config", "user.name", "Ada Lovelace"],
        &["config", "user.email", "ada@example.com"],
        &["config", "commit.gpgsign", "false"],
        // Files as written: a runner's core.autocrlf would turn restored files into CRLF.
        &["config", "core.autocrlf", "false"],
    ] {
        repo.run(args).unwrap();
    }
    (dir, repo)
}

fn write(root: &Path, name: &str, text: &str) {
    if let Some(parent) = Path::new(name).parent() {
        std::fs::create_dir_all(root.join(parent)).unwrap();
    }
    std::fs::write(root.join(name), text).unwrap();
}

#[test]
fn a_folder_that_isnt_a_repository_is_none_and_one_inside_finds_its_root() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Repo::discover("git", dir.path()).unwrap().is_none());
    let (dir, repo) = fresh();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let found = Repo::discover("git", &dir.path().join("sub"))
        .unwrap()
        .expect("a repo");
    assert!(found.root.join("sub").is_dir());
    assert_eq!(found.root, repo.root);
    assert!(found.git_dir.join("HEAD").is_file());
}

#[test]
fn a_missing_program_is_an_error_that_says_so() {
    let err = Repo::version("templum-no-such-git").unwrap_err();
    assert!(err.missing, "{err}");
    assert_eq!(err.code, None);
    let dir = tempfile::tempdir().unwrap();
    assert!(
        Repo::discover("templum-no-such-git", dir.path())
            .unwrap_err()
            .missing
    );
    assert!(Repo::version("git").unwrap().starts_with("git version "));
}

#[test]
fn status_stage_unstage_and_commit() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    let s = repo.status().unwrap();
    assert_eq!(s.branch.as_deref(), Some("main"));
    assert_eq!(s.oid, None, "no commits yet");
    assert_eq!(s.entries.len(), 1);
    assert_eq!(s.entries[0].unstaged, Some(Change::Untracked));
    repo.stage(&["a.txt"]).unwrap();
    let s = repo.status().unwrap();
    assert_eq!(s.entries[0].staged, Some(Change::Added));
    // Unstaging before the first commit: `git restore --staged` has no HEAD, `git reset` works.
    repo.unstage(&["a.txt"]).unwrap();
    assert_eq!(
        repo.status().unwrap().entries[0].unstaged,
        Some(Change::Untracked)
    );
    repo.stage(&["a.txt"]).unwrap();
    let short = repo.commit("First\n\nA body.", false).unwrap();
    assert_eq!(short.len(), 7, "{short}");
    let s = repo.status().unwrap();
    assert!(s.entries.is_empty());
    assert!(s.oid.is_some());
    assert_eq!(repo.last_message().unwrap(), "First\n\nA body.");
    // Amend keeps the tree and replaces the message.
    repo.commit("First, amended", true).unwrap();
    assert_eq!(repo.last_message().unwrap(), "First, amended");
    assert_eq!(repo.run(&["rev-list", "--count", "HEAD"]).unwrap(), b"1\n");
}

#[test]
fn staging_a_renamed_file_and_one_with_a_space() {
    let (dir, repo) = fresh();
    write(dir.path(), "old.txt", "same\n");
    write(dir.path(), "with space.txt", "x\n");
    repo.stage(&["old.txt", "with space.txt"]).unwrap();
    repo.commit("Two files", false).unwrap();
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).unwrap();
    write(dir.path(), "with space.txt", "y\n");
    let s = repo.status().unwrap();
    let paths: Vec<&str> = s.entries.iter().map(|e| e.path.as_str()).collect();
    // Git lists tracked entries first, untracked ones last.
    assert_eq!(paths, ["old.txt", "with space.txt", "new.txt"]);
    let all: Vec<&str> = paths.clone();
    repo.stage(&all).unwrap();
    let s = repo.status().unwrap();
    let renamed = s.entries.iter().find(|e| e.path == "new.txt").unwrap();
    assert_eq!(renamed.staged, Some(Change::Renamed));
    assert_eq!(renamed.from.as_deref(), Some("old.txt"));
    assert_eq!(
        s.entries.len(),
        2,
        "the rename is one entry: {:?}",
        s.entries
    );
    let d = repo.working_diff(&s, false, &[]).unwrap();
    let renamed = d.staged.iter().find(|f| f.path() == "new.txt").unwrap();
    assert_eq!(renamed.status, Change::Renamed);
    assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
}

#[test]
fn the_working_diff_has_both_sides_the_untracked_files_and_their_counts() {
    use templum_git::Kind;
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a", false).unwrap();
    // a.txt: "zero" staged, "two" not; n.txt untracked; bin untracked and binary.
    write(dir.path(), "a.txt", "zero\none\n");
    repo.stage(&["a.txt"]).unwrap();
    write(dir.path(), "a.txt", "zero\none\ntwo\n");
    write(dir.path(), "n.txt", "new");
    write(dir.path(), "bin", "\0\x01\x02");
    let s = repo.status().unwrap();
    let d = repo.working_diff(&s, false, &[]).unwrap();
    let paths = |files: &[templum_git::FileDiff]| {
        files
            .iter()
            .map(|f| f.path().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        paths(&d.unstaged),
        ["a.txt", "bin", "n.txt"],
        "tracked first, then untracked in the status's order"
    );
    assert_eq!(paths(&d.staged), ["a.txt"]);
    let a = &d.unstaged[0];
    assert_eq!(a.status, Change::Modified);
    assert_eq!((a.added(), a.removed()), (1, 0));
    assert_eq!(
        a.hunks[0].lines.last().map(|l| (l.kind, l.text.as_str())),
        Some((Kind::Added, "two"))
    );
    assert_eq!(a.bytes, Some(13));
    assert_eq!(a.lines, Some(3));
    assert!(d.unstaged[1].binary, "bin");
    assert_eq!(d.unstaged[1].bytes, Some(3));
    assert_eq!(d.unstaged[1].lines, None, "not counted");
    let n = &d.unstaged[2];
    assert_eq!(n.status, Change::Added);
    assert_eq!(n.hunks[0].lines[0].text, "new");
    assert!(n.hunks[0].lines[0].no_newline);
    assert_eq!(n.lines, Some(1), "a last line without a newline counts");
    let staged = &d.staged[0];
    assert_eq!(staged.hunks[0].lines[0].text, "zero");
    assert_eq!(staged.hunks[0].lines[0].kind, Kind::Added);
    // Nothing changed: the same value again (the panel compares them).
    assert_eq!(repo.working_diff(&s, false, &[]).unwrap(), d);
    // A clean tree is two empty lists.
    repo.stage(&["a.txt", "n.txt", "bin"]).unwrap();
    repo.commit("all", false).unwrap();
    let s = repo.status().unwrap();
    assert_eq!(
        repo.working_diff(&s, false, &[]).unwrap(),
        templum_git::WorkingDiff::default()
    );
}

#[test]
fn names_with_spaces_and_accents_come_as_themselves() {
    let (dir, repo) = fresh();
    write(dir.path(), "with space.txt", "a\n");
    write(dir.path(), "café.txt", "a\n");
    repo.stage(&["with space.txt", "café.txt"]).unwrap();
    repo.commit("two", false).unwrap();
    write(dir.path(), "with space.txt", "b\n");
    write(dir.path(), "café.txt", "b\n");
    let s = repo.status().unwrap();
    let d = repo.working_diff(&s, false, &[]).unwrap();
    let mut paths: Vec<&str> = d.unstaged.iter().map(|f| f.path()).collect();
    paths.sort();
    assert_eq!(paths, ["café.txt", "with space.txt"]);
}

#[test]
fn ignore_whitespace_hides_a_change_of_spaces_only() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a", false).unwrap();
    write(dir.path(), "a.txt", "one  \n");
    let s = repo.status().unwrap();
    assert_eq!(repo.working_diff(&s, false, &[]).unwrap().unstaged.len(), 1);
    assert!(
        repo.working_diff(&s, true, &[])
            .unwrap()
            .unstaged
            .is_empty()
    );
}

#[test]
fn a_whole_file_is_read_with_every_line_as_context() {
    let (dir, repo) = fresh();
    let lines: Vec<String> = (1..=20).map(|i| format!("line {i}\n")).collect();
    write(dir.path(), "a.txt", &lines.concat());
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a", false).unwrap();
    let mut changed = lines.clone();
    changed[9] = "LINE 10\n".to_string();
    write(dir.path(), "a.txt", &changed.concat());
    let s = repo.status().unwrap();
    let d = repo.working_diff(&s, false, &[]).unwrap();
    let h = &d.unstaged[0].hunks[0];
    assert_eq!(
        (h.old_start, h.old_len),
        (7, 7),
        "3 lines of context each side"
    );
    let d = repo
        .working_diff(&s, false, &[(false, "a.txt".to_string())])
        .unwrap();
    let h = &d.unstaged[0].hunks[0];
    assert_eq!(
        (h.old_start, h.old_len, h.new_start, h.new_len),
        (1, 20, 1, 20)
    );
    assert_eq!(h.lines.len(), 21, "20 lines, one of them twice");
    // Asked for the staged side only, the unstaged read stays as it was.
    let d = repo
        .working_diff(&s, false, &[(true, "a.txt".to_string())])
        .unwrap();
    assert_eq!(d.unstaged[0].hunks[0].old_len, 7);
}

#[test]
fn a_conflicted_file_comes_first_with_its_regions() {
    let (dir, repo) = conflicted();
    write(dir.path(), "other.txt", "x\n");
    let s = repo.status().unwrap();
    let d = repo.working_diff(&s, false, &[]).unwrap();
    let paths: Vec<&str> = d.unstaged.iter().map(|f| f.path()).collect();
    assert_eq!(paths, ["a.txt", "other.txt"], "conflicts first");
    let a = &d.unstaged[0];
    assert_eq!(a.status, Change::Conflict);
    assert_eq!(
        (a.hunks[0].context.as_str(), a.hunks[1].context.as_str()),
        ("HEAD", "topic")
    );
    assert_eq!(a.hunks[0].lines[0].text, "main");
    assert_eq!(a.hunks[1].lines[0].text, "topic");
}

#[test]
fn discard_restores_a_tracked_file_and_removes_an_untracked_one() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a", false).unwrap();
    write(dir.path(), "a.txt", "changed\n");
    write(dir.path(), "junk.txt", "x\n");
    let s = repo.status().unwrap();
    let entries: Vec<_> = s.entries.iter().collect();
    repo.discard(&entries).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(!dir.path().join("junk.txt").exists());
    assert!(repo.status().unwrap().entries.is_empty());
}

#[test]
fn ignore_appends_a_line_to_gitignore() {
    let (dir, repo) = fresh();
    write(dir.path(), ".gitignore", "*.log");
    repo.ignore("build/").unwrap();
    repo.ignore("secret.txt").unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
        "*.log\n/build/\n/secret.txt\n",
        "anchored at the root"
    );
}

#[test]
fn the_author_comes_from_the_config() {
    let (_dir, repo) = fresh();
    assert_eq!(
        repo.author().unwrap(),
        Some(("Ada Lovelace".to_string(), "ada@example.com".to_string()))
    );
    // The unset case can't be forced here (a global config would still answer): Task 5's
    // headless test hides the global config behind a wrapper script and covers it.
}

/// `main` and `topic` both changed a.txt: `git merge topic` conflicts.
fn conflicted() -> (tempfile::TempDir, Repo) {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "base\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("base", false).unwrap();
    repo.run(&["checkout", "-q", "-b", "topic"]).unwrap();
    write(dir.path(), "a.txt", "topic\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("topic", false).unwrap();
    repo.run(&["checkout", "-q", "main"]).unwrap();
    write(dir.path(), "a.txt", "main\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("main", false).unwrap();
    assert_eq!(repo.operation(), None);
    let err = repo.run(&["merge", "topic"]).unwrap_err();
    assert_eq!(err.code, Some(1), "{err}");
    (dir, repo)
}

#[test]
fn a_merge_in_progress_is_an_operation_and_aborting_ends_it() {
    let (_dir, repo) = conflicted();
    assert_eq!(repo.operation(), Some(Operation::Merge));
    let s = repo.status().unwrap();
    assert_eq!(s.conflicts().len(), 1);
    let file = std::fs::read_to_string(repo.root.join(&s.conflicts()[0].path)).unwrap();
    assert!(file.contains("<<<<<<<"), "{file}");
    repo.abort(Operation::Merge).unwrap();
    assert_eq!(repo.operation(), None);
    assert!(repo.status().unwrap().entries.is_empty());
}

#[test]
fn a_conflict_takes_a_side_and_the_merge_continues() {
    let (dir, repo) = conflicted();
    repo.resolve("a.txt", Side::Theirs).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "topic\n"
    );
    assert_eq!(
        repo.status().unwrap().conflicts().len(),
        1,
        "not resolved until staged"
    );
    repo.stage(&["a.txt"]).unwrap();
    assert!(repo.status().unwrap().conflicts().is_empty());
    repo.continue_(Operation::Merge).unwrap();
    assert_eq!(repo.operation(), None);
    assert_eq!(repo.run(&["rev-list", "--count", "HEAD"]).unwrap(), b"4\n");
    assert!(
        repo.last_message()
            .unwrap()
            .starts_with("Merge branch 'topic'")
    );
}

#[test]
fn an_error_carries_the_command_and_stderrs_first_line() {
    let (_dir, repo) = fresh();
    let err = repo.run(&["checkout", "no-such-branch"]).unwrap_err();
    assert_eq!(err.command, "git checkout no-such-branch");
    assert_eq!(err.code, Some(1));
    assert!(err.first_line().contains("no-such-branch"), "{err}");
    assert!(!err.needs_input());
    assert_eq!(
        err.to_string(),
        format!("{}: {}", err.command, err.first_line())
    );
}

/// `repo` with a bare `origin` that has its `main`.
fn with_remote(repo: &Repo) -> tempfile::TempDir {
    let remote = tempfile::tempdir().unwrap();
    std::process::Command::new("git")
        .args(["init", "-q", "--bare"])
        .current_dir(remote.path())
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(remote.path())
        .status()
        .unwrap();
    repo.run(&[
        "remote",
        "add",
        "origin",
        &remote.path().display().to_string(),
    ])
    .unwrap();
    repo.run(&["push", "-q", "-u", "origin", "main"]).unwrap();
    remote
}

#[test]
fn fetch_pull_and_push_against_a_local_remote() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    let remote = with_remote(&repo);
    assert_eq!(
        repo.status().unwrap().upstream.as_deref(),
        Some("origin/main")
    );
    // Another clone pushes a commit: we are behind after a fetch.
    let other = tempfile::tempdir().unwrap();
    let clone = Repo::init("git", other.path()).unwrap();
    clone
        .run(&[
            "remote",
            "add",
            "origin",
            &remote.path().display().to_string(),
        ])
        .unwrap();
    clone.run(&["pull", "-q", "origin", "main"]).unwrap();
    clone.run(&["config", "user.name", "B"]).unwrap();
    clone
        .run(&["config", "user.email", "b@example.com"])
        .unwrap();
    write(other.path(), "b.txt", "b\n");
    clone.stage(&["b.txt"]).unwrap();
    clone.commit("b", false).unwrap();
    clone.run(&["push", "-q", "origin", "HEAD:main"]).unwrap();
    repo.fetch().unwrap();
    assert_eq!(repo.status().unwrap().behind, 1);
    repo.pull(Pull::FastForward).unwrap();
    assert_eq!(repo.status().unwrap().behind, 0);
    assert!(dir.path().join("b.txt").is_file());
    // Diverged: ff-only refuses, a merge pull works.
    write(other.path(), "c.txt", "c\n");
    clone.stage(&["c.txt"]).unwrap();
    clone.commit("c", false).unwrap();
    clone.run(&["push", "-q", "origin", "HEAD:main"]).unwrap();
    write(dir.path(), "d.txt", "d\n");
    repo.stage(&["d.txt"]).unwrap();
    repo.commit("d", false).unwrap();
    let err = repo.pull(Pull::FastForward).unwrap_err();
    assert!(templum_git::not_fast_forward(&err.stderr), "{err}");
    repo.pull(Pull::Merge).unwrap();
    assert!(dir.path().join("c.txt").is_file());
    repo.push().unwrap();
    assert_eq!(repo.status().unwrap().ahead, 0);
    // A new branch has no upstream until it is published.
    repo.run(&["checkout", "-q", "-b", "topic"]).unwrap();
    assert_eq!(repo.status().unwrap().upstream, None);
    repo.publish("origin", "topic").unwrap();
    assert_eq!(
        repo.status().unwrap().upstream.as_deref(),
        Some("origin/topic")
    );
}

#[test]
fn a_remote_that_isnt_there_fails_with_the_command_in_the_error() {
    let (_dir, repo) = fresh();
    repo.run(&["remote", "add", "origin", "/no/such/remote"])
        .unwrap();
    let err = repo.fetch().unwrap_err();
    assert_eq!(err.command, "git fetch --prune");
    assert!(!err.needs_input());
}

#[test]
fn log_files_diffs_tags_branches_and_reverts() {
    let (dir, repo) = fresh();
    assert!(
        repo.log(0, 10).unwrap().is_empty(),
        "no commits: an empty log, not an error"
    );
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    let first = repo.commit("First", false).unwrap();
    write(dir.path(), "a.txt", "one\ntwo\n");
    write(dir.path(), "b.txt", "b\n");
    repo.stage(&["a.txt", "b.txt"]).unwrap();
    repo.commit("Second\n\nWith a body.", false).unwrap();
    let log = repo.log(0, 10).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].subject, "Second");
    assert_eq!(log[0].body, "With a body.");
    assert_eq!(log[0].author, "Ada Lovelace");
    assert!(
        log[0].refs.contains(&"HEAD -> main".to_string()),
        "{:?}",
        log[0].refs
    );
    assert_eq!(log[1].short, first);
    assert!(log[1].parents.is_empty());
    assert_eq!(repo.log(1, 10).unwrap().len(), 1, "skip");
    assert_eq!(repo.log(0, 1).unwrap().len(), 1, "count");
    let files = repo.commit_files(&log[0].hash).unwrap();
    let got: Vec<_> = files.iter().map(|f| (f.change, f.path.as_str())).collect();
    assert_eq!(got, [(Change::Modified, "a.txt"), (Change::Added, "b.txt")]);
    let diff = repo.commit_diff(&log[0].hash, &files[0]).unwrap();
    assert!(diff.contains("+two"), "{diff}");
    let root = repo.commit_files(&log[1].hash).unwrap();
    assert_eq!(root.len(), 1, "a root commit's files");
    assert!(
        repo.commit_diff(&log[1].hash, &root[0])
            .unwrap()
            .contains("+one")
    );
    repo.tag("v1", &log[1].hash).unwrap();
    assert!(
        repo.log(0, 10).unwrap()[1]
            .refs
            .contains(&"tag: v1".to_string())
    );
    repo.new_branch("feat", Some(&log[1].hash)).unwrap();
    let s = repo.status().unwrap();
    assert_eq!(s.branch.as_deref(), Some("feat"));
    assert_eq!(s.oid.as_deref(), Some(log[1].hash.as_str()));
    let err = repo.new_branch("bad name", None).unwrap_err();
    assert!(err.command.starts_with("git check-ref-format"), "{err}");
    repo.run(&["checkout", "-q", "main"]).unwrap();
    repo.revert(&log[0].hash).unwrap();
    assert!(!dir.path().join("b.txt").exists(), "reverted");
    assert!(
        repo.log(0, 1).unwrap()[0]
            .subject
            .starts_with("Revert \"Second\"")
    );
    repo.checkout_commit(&log[1].hash).unwrap();
    assert_eq!(repo.status().unwrap().branch, None, "detached");
}

#[test]
fn branches_checkouts_renames_merges_and_deletes() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("First", false).unwrap();
    let remote = with_remote(&repo);
    repo.new_branch("feat", None).unwrap();
    write(dir.path(), "b.txt", "b\n");
    repo.stage(&["b.txt"]).unwrap();
    repo.commit("b", false).unwrap();
    let b = repo.branches().unwrap();
    let names: Vec<_> = b
        .iter()
        .map(|b| (b.name.as_str(), b.remote, b.current))
        .collect();
    assert_eq!(
        names,
        [
            ("feat", false, true),
            ("main", false, false),
            ("origin/main", true, false)
        ]
    );
    assert_eq!(b[1].upstream.as_deref(), Some("origin/main"));
    assert_eq!(b[0].subject, "b");
    repo.rename_branch("feat", "feature").unwrap();
    assert_eq!(repo.status().unwrap().branch.as_deref(), Some("feature"));
    repo.checkout("main").unwrap();
    assert_eq!(repo.status().unwrap().branch.as_deref(), Some("main"));
    // Local changes in the way: refused, and stash_push clears the way.
    repo.checkout("feature").unwrap();
    write(dir.path(), "a.txt", "changed\n");
    repo.run(&["checkout", "-q", "-b", "other"]).unwrap();
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("other a", false).unwrap();
    repo.checkout("main").unwrap();
    write(dir.path(), "a.txt", "dirty\n");
    let err = repo.checkout("other").unwrap_err();
    assert!(err.stderr.contains("would be overwritten"), "{err}");
    repo.stash_push("Templum: before switching", true).unwrap();
    repo.checkout("other").unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "changed\n"
    );
    repo.checkout("main").unwrap();
    repo.merge("feature").unwrap();
    assert!(dir.path().join("b.txt").is_file());
    let err = repo.delete_branch("other", false).unwrap_err();
    assert!(templum_git::not_merged(&err.stderr), "{err}");
    repo.delete_branch("other", true).unwrap();
    repo.delete_branch("feature", false).unwrap();
    assert_eq!(
        repo.branches()
            .unwrap()
            .iter()
            .filter(|b| !b.remote)
            .count(),
        1
    );
    // A remote branch checks out as a local one tracking it.
    repo.run(&["push", "-q", "origin", "main:topic"]).unwrap();
    repo.fetch().unwrap();
    repo.checkout_remote("origin/topic").unwrap();
    let s = repo.status().unwrap();
    assert_eq!(s.branch.as_deref(), Some("topic"));
    assert_eq!(s.upstream.as_deref(), Some("origin/topic"));
    drop(remote);
}

#[test]
fn stashes_are_listed_shown_applied_popped_and_dropped() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("First", false).unwrap();
    assert!(repo.stashes().unwrap().is_empty());
    write(dir.path(), "a.txt", "changed\n");
    write(dir.path(), "new.txt", "n\n");
    repo.stash_push("my work", true).unwrap();
    assert!(repo.status().unwrap().entries.is_empty());
    let s = repo.stashes().unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].index, 0);
    assert_eq!(s[0].message, "On main: my work");
    assert!(repo.stash_show(&s[0]).unwrap().contains("+changed"));
    repo.stash_apply(&s[0]).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "changed\n"
    );
    assert!(dir.path().join("new.txt").is_file());
    assert_eq!(repo.stashes().unwrap().len(), 1, "apply keeps it");
    repo.discard(&repo.status().unwrap().unstaged()).unwrap();
    repo.stash_pop(&s[0]).unwrap();
    assert!(repo.stashes().unwrap().is_empty(), "pop drops it");
    repo.stash_push("again", false).unwrap();
    repo.stash_drop(&repo.stashes().unwrap()[0]).unwrap();
    assert!(repo.stashes().unwrap().is_empty());
}

// Final review I1: a stash is named by its hash, so a list read before another stash was
// pushed (from a terminal, by an agent) can't drop the wrong one.
#[test]
fn a_stale_stash_is_refused_not_mistaken_for_another() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("First", false).unwrap();
    write(dir.path(), "a.txt", "mine\n");
    repo.stash_push("mine", false).unwrap();
    let stale = repo.stashes().unwrap();
    write(dir.path(), "a.txt", "theirs\n");
    repo.stash_push("theirs", false).unwrap();
    let err = repo.stash_drop(&stale[0]).unwrap_err();
    assert!(err.first_line().contains("changed"), "{err}");
    let now = repo.stashes().unwrap();
    assert_eq!(now.len(), 2, "nothing dropped");
    assert_eq!(now[1].message, "On main: mine");
    repo.stash_drop(&now[1]).unwrap();
    assert_eq!(repo.stashes().unwrap()[0].message, "On main: theirs");
}

// Final review I2: the command a failure names is what a shell would run, argument for argument.
#[test]
fn a_failed_commands_text_quotes_its_arguments_for_a_shell() {
    let (_dir, repo) = fresh();
    let err = repo
        .run(&["check-ref-format", "--branch", "it's [1] $x stash@{0}"])
        .unwrap_err();
    assert_eq!(
        err.command,
        r#"git check-ref-format --branch 'it'\''s [1] $x stash@{0}'"#
    );
    #[cfg(unix)]
    {
        let back = std::process::Command::new("sh")
            .args(["-c", &format!("printf '%s\\n' {}", err.command)])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&back.stdout),
            "git\ncheck-ref-format\n--branch\nit's [1] $x stash@{0}\n"
        );
    }
}

// Final review I3: the background status never takes index.lock, which would make a `git add`
// run at that moment in a terminal fail.
#[test]
fn status_leaves_the_index_alone() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("First", false).unwrap();
    // Same content, newer time: a plain `git status` refreshes the index and writes it.
    std::fs::File::options()
        .write(true)
        .open(dir.path().join("a.txt"))
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    let index = repo.git_dir.join("index");
    let before = std::fs::read(&index).unwrap();
    repo.status().unwrap();
    assert_eq!(std::fs::read(&index).unwrap(), before, "not rewritten");
}

// Final review I7: the notification's line says why: the key, the rejection.
#[test]
fn the_first_line_is_the_reason_not_the_summary() {
    let err = |stderr: &str| templum_git::GitError {
        command: "git push".to_string(),
        code: Some(128),
        stderr: stderr.to_string(),
        missing: false,
    };
    let ssh = err(
        "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n\nPlease make sure you have the correct access rights\nand the repository exists.\n",
    );
    assert_eq!(
        ssh.first_line(),
        "git@github.com: Permission denied (publickey)."
    );
    assert!(ssh.needs_input());
    let rejected = err(
        "To github.com:me/x.git\n ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs to 'github.com:me/x.git'\nhint: Updates were rejected because the remote contains work that you do not\n",
    );
    assert_eq!(
        rejected.first_line(),
        "! [rejected]        main -> main (fetch first)"
    );
    let plain = err("hint: something\nfatal: not a git repository\n");
    assert_eq!(plain.first_line(), "fatal: not a git repository");
}

// Final review I6: a huge untracked file isn't read into the diff view.
#[test]
fn a_huge_untracked_file_is_too_large_and_not_read() {
    let (dir, repo) = fresh();
    write(dir.path(), "big.csv", &"a,b\n".repeat(1_310_720));
    let s = repo.status().unwrap();
    let d = repo.working_diff(&s, false, &[]).unwrap();
    let f = &d.unstaged[0];
    assert!(f.too_large);
    assert!(f.hunks.is_empty());
    assert_eq!(f.path(), "big.csv");
    assert_eq!(f.bytes, Some(5_242_880));
    assert_eq!(f.lines, None);
}

// Re-graded from minor: Ignore anchors the path at the root and keeps the rest of the file.
#[test]
fn ignore_anchors_the_path_and_keeps_the_rest_of_the_file() {
    let (dir, repo) = fresh();
    std::fs::write(dir.path().join(".gitignore"), b"*.log\n# caf\xe9\n").unwrap();
    write(dir.path(), "notes.txt", "n\n");
    write(dir.path(), "sub/notes.txt", "n\n");
    write(dir.path(), "a[1].txt", "x\n");
    repo.ignore("notes.txt").unwrap();
    repo.ignore("a[1].txt").unwrap();
    // Not a file name on Windows: only its pattern is checked.
    repo.ignore("all*.tmp").unwrap();
    assert_eq!(
        std::fs::read(dir.path().join(".gitignore")).unwrap(),
        b"*.log\n# caf\xe9\n/notes.txt\n/a\\[1].txt\n/all\\*.tmp\n"
    );
    let paths: Vec<String> = repo
        .status()
        .unwrap()
        .entries
        .into_iter()
        .map(|e| e.path)
        .collect();
    assert!(
        paths.contains(&"sub/notes.txt".to_string()),
        "only the root's: {paths:?}"
    );
    assert!(!paths.contains(&"notes.txt".to_string()), "{paths:?}");
    assert!(!paths.contains(&"a[1].txt".to_string()), "{paths:?}");
}

// Final review I4: a command that hangs (ssh asking on a terminal, a stalled network) can be
// cancelled: the process is killed and the error says so.
#[cfg(unix)]
#[test]
fn a_cancelled_command_is_killed_and_says_so() {
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let (dir, _) = fresh();
    let bin = tempfile::tempdir().unwrap();
    let slow = bin.path().join("slow-git");
    std::fs::write(
        &slow,
        "#!/bin/sh\nif [ \"$1\" = fetch ]; then exec sleep 30; fi\nexec git \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&slow, std::fs::Permissions::from_mode(0o755)).unwrap();
    let repo = Repo::discover(&slow.display().to_string(), dir.path())
        .unwrap()
        .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let hanging = repo.with_cancel(cancel.clone());
    let started = std::time::Instant::now();
    let fetch = std::thread::spawn(move || hanging.fetch());
    std::thread::sleep(std::time::Duration::from_millis(200));
    cancel.store(true, Ordering::Relaxed);
    let err = fetch.join().unwrap().unwrap_err();
    assert!(err.cancelled(), "{err}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "killed, not waited for"
    );
    assert!(repo.status().is_ok(), "the repo itself still answers");
}

#[test]
fn root_of_finds_the_nearest_git_entry() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(repo.join("src/sub")).unwrap();
    let file = repo.join("src/sub/a.rs");
    assert_eq!(templum_git::root_of(&file), Some(repo.as_path()));
    // A worktree's or a submodule's .git is a file.
    let module = repo.join("src/sub");
    std::fs::write(module.join(".git"), "gitdir: ../../.git/modules/sub").unwrap();
    assert_eq!(templum_git::root_of(&file), Some(module.as_path()));
    let outside = dir.path().join("loose.txt");
    assert_eq!(templum_git::root_of(&outside), None);
}

#[test]
fn a_users_diff_prefixes_and_external_diff_are_overridden() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a", false).unwrap();
    // What a user's config can set: i/ and w/ prefixes, none at all, a diff program of its own.
    for args in [
        ["config", "diff.mnemonicPrefix", "true"].as_slice(),
        &["config", "diff.noprefix", "true"],
        &["config", "diff.external", "templum-no-such-diff"],
    ] {
        repo.run(args).unwrap();
    }
    write(dir.path(), "a.txt", "two\n");
    let s = repo.status().unwrap();
    let d = repo.working_diff(&s, false, &[]).unwrap();
    assert_eq!(d.unstaged.len(), 1);
    assert_eq!(d.unstaged[0].path(), "a.txt");
    assert_eq!(d.unstaged[0].old_path.as_deref(), Some("a.txt"));
    assert_eq!(d.unstaged[0].added(), 1);
}

#[test]
fn past_the_budget_untracked_files_are_listed_unread_until_asked_for() {
    let (dir, repo) = fresh();
    // An agent unpacked a folder with no .gitignore: more new files than one read takes.
    for i in 0..205 {
        write(dir.path(), &format!("out/f{i:03}.txt"), "x\n");
    }
    let s = repo.status().unwrap();
    let d = repo.working_diff(&s, false, &[]).unwrap();
    assert_eq!(d.unstaged.len(), 205, "every file is listed");
    let read = d.unstaged.iter().filter(|f| !f.unread).count();
    assert_eq!(read, 200);
    let last = d.unstaged.last().unwrap();
    assert_eq!(last.path(), "out/f204.txt");
    assert!(last.unread && last.hunks.is_empty());
    assert_eq!(last.bytes, Some(2), "its size, from the folder");
    assert_eq!(last.lines, None, "not read, not counted");
    // Asked for (Show diff): read whatever the budget says.
    let d = repo
        .working_diff(&s, false, &[(false, "out/f204.txt".to_string())])
        .unwrap();
    let last = d.unstaged.last().unwrap();
    assert!(!last.unread);
    assert_eq!(last.hunks[0].lines[0].text, "x");
}

/// a.txt of 30 lines committed, then lines 2, 15 and 28 changed: three hunks.
fn three_hunks() -> (tempfile::TempDir, Repo) {
    let (dir, repo) = fresh();
    let lines: Vec<String> = (1..=30).map(|i| format!("line {i}\n")).collect();
    write(dir.path(), "a.txt", &lines.concat());
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a", false).unwrap();
    let mut changed = lines;
    for i in [1, 14, 27] {
        changed[i] = format!("LINE {}\n", i + 1);
    }
    write(dir.path(), "a.txt", &changed.concat());
    (dir, repo)
}

fn working(repo: &Repo) -> templum_git::WorkingDiff {
    repo.working_diff(&repo.status().unwrap(), false, &[])
        .unwrap()
}

#[test]
fn stage_unstage_and_discard_one_hunk_of_three() {
    let (dir, repo) = three_hunks();
    let d = working(&repo);
    assert_eq!(d.unstaged[0].hunks.len(), 3);
    let patch = templum_git::hunk_patch(&d.unstaged[0], 1).unwrap();
    repo.apply_patch(&patch, true, false).unwrap();
    let d = working(&repo);
    assert_eq!(d.staged[0].hunks.len(), 1, "the middle hunk staged");
    let added = d.staged[0].hunks[0]
        .lines
        .iter()
        .find(|l| l.kind == templum_git::Kind::Added)
        .unwrap();
    assert_eq!(added.text, "LINE 15");
    assert_eq!(d.unstaged[0].hunks.len(), 2, "the other two not");
    let patch = templum_git::hunk_patch(&d.staged[0], 0).unwrap();
    repo.apply_patch(&patch, true, true).unwrap();
    let d = working(&repo);
    assert!(d.staged.is_empty(), "unstaged again");
    assert_eq!(d.unstaged[0].hunks.len(), 3);
    let patch = templum_git::hunk_patch(&d.unstaged[0], 2).unwrap();
    repo.apply_patch(&patch, false, true).unwrap();
    let text = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert!(
        text.contains("LINE 2\n") && text.contains("LINE 15\n"),
        "{text}"
    );
    assert!(
        text.contains("line 28\n") && !text.contains("LINE 28"),
        "the last discarded"
    );
}

#[test]
fn a_hunk_patch_that_no_longer_applies_is_refused() {
    let (dir, repo) = three_hunks();
    let patch = templum_git::hunk_patch(&working(&repo).unstaged[0], 1).unwrap();
    // An agent changed that part since the diff was read.
    let text = std::fs::read_to_string(dir.path().join("a.txt"))
        .unwrap()
        .replace("LINE 15", "other");
    write(dir.path(), "a.txt", &text);
    let err = repo.apply_patch(&patch, false, true).unwrap_err();
    assert!(err.stderr.contains("does not apply"), "{err:?}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        text,
        "untouched"
    );
}

#[test]
fn hunks_of_a_crlf_file_a_file_without_a_last_newline_and_a_staged_rename() {
    let (dir, repo) = fresh();
    let crlf: String = (1..=12).map(|i| format!("l{i}\r\n")).collect();
    write(dir.path(), "w.txt", &crlf);
    let twenty: String = (1..=20).map(|i| format!("{i}\n")).collect();
    write(dir.path(), "old.txt", &twenty);
    write(dir.path(), "n.txt", "a\nb");
    repo.stage(&["w.txt", "old.txt", "n.txt"]).unwrap();
    repo.commit("base", false).unwrap();
    // CRLF: discard the second of two hunks; the endings stay.
    write(
        dir.path(),
        "w.txt",
        &crlf
            .replace("l1\r\n", "L1\r\n")
            .replace("l12\r\n", "L12\r\n"),
    );
    let d = working(&repo);
    let w = d.unstaged.iter().find(|f| f.path() == "w.txt").unwrap();
    assert_eq!(w.hunks.len(), 2);
    repo.apply_patch(&templum_git::hunk_patch(w, 1).unwrap(), false, true)
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("w.txt")).unwrap(),
        crlf.replace("l1\r\n", "L1\r\n")
    );
    // No newline at the end, before and after: staged as it is.
    write(dir.path(), "n.txt", "a\nB");
    let n = working(&repo)
        .unstaged
        .into_iter()
        .find(|f| f.path() == "n.txt")
        .unwrap();
    repo.apply_patch(&templum_git::hunk_patch(&n, 0).unwrap(), true, false)
        .unwrap();
    assert_eq!(repo.run(&["show", ":n.txt"]).unwrap(), b"a\nB");
    // A staged rename with two hunks: unstaging one keeps the rename staged.
    repo.run(&["mv", "old.txt", "new.txt"]).unwrap();
    // Lines 3 and 18: two hunks, far enough apart.
    let changed: String = (1..=20)
        .map(|i| match i {
            3 | 18 => format!("X{i}\n"),
            _ => format!("{i}\n"),
        })
        .collect();
    write(dir.path(), "new.txt", &changed);
    repo.stage(&["new.txt"]).unwrap();
    let d = working(&repo);
    let renamed = d.staged.iter().find(|f| f.path() == "new.txt").unwrap();
    assert_eq!(renamed.status, Change::Renamed);
    assert_eq!(renamed.hunks.len(), 2);
    repo.apply_patch(&templum_git::hunk_patch(renamed, 1).unwrap(), true, true)
        .unwrap();
    let d = working(&repo);
    let renamed = d.staged.iter().find(|f| f.path() == "new.txt").unwrap();
    assert_eq!(renamed.status, Change::Renamed, "still a rename");
    assert_eq!(renamed.hunks.len(), 1);
}

#[cfg(unix)]
#[test]
fn a_hunk_of_a_file_whose_name_git_quotes() {
    let (dir, repo) = fresh();
    let name = "we\"ird\tname.txt";
    write(dir.path(), name, "a\nb\n");
    repo.stage(&[name]).unwrap();
    repo.commit("w", false).unwrap();
    write(dir.path(), name, "a\nB\n");
    let d = working(&repo);
    assert_eq!(d.unstaged[0].path(), name);
    repo.apply_patch(
        &templum_git::hunk_patch(&d.unstaged[0], 0).unwrap(),
        true,
        false,
    )
    .unwrap();
    assert_eq!(working(&repo).staged[0].path(), name);
}

fn head(repo: &Repo) -> String {
    String::from_utf8(repo.run(&["rev-parse", "--short", "HEAD"]).unwrap())
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn undo_commit_takes_the_last_commit_back_and_keeps_its_changes_staged() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    // The repository's first commit has no parent: HEAD goes away, the file stays staged.
    let first = repo.commit("First", false).unwrap();
    assert_eq!(repo.undo_commit(&first, None).unwrap().unwrap(), "First");
    let s = repo.status().unwrap();
    assert_eq!(s.oid, None, "no commit again");
    assert_eq!(s.entries[0].staged, Some(Change::Added));
    let first = repo.commit("First", false).unwrap();
    write(dir.path(), "a.txt", "two\n");
    repo.stage(&["a.txt"]).unwrap();
    let second = repo.commit("Second\n\nWhy it changed.", false).unwrap();
    assert_eq!(
        repo.undo_commit(&second, None).unwrap().unwrap(),
        "Second\n\nWhy it changed."
    );
    assert_eq!(head(&repo), first);
    assert_eq!(
        repo.status().unwrap().entries[0].staged,
        Some(Change::Modified),
        "its change still staged"
    );
    // After an Amend, the commit the amend replaced comes back, not its parent.
    let second = repo.commit("Second", false).unwrap();
    write(dir.path(), "a.txt", "three\n");
    repo.stage(&["a.txt"]).unwrap();
    let replaced = repo.head().unwrap();
    let amended = repo.commit("Second, amended", true).unwrap();
    assert_eq!(
        repo.undo_commit(&amended, Some(&replaced))
            .unwrap()
            .unwrap(),
        "Second, amended"
    );
    assert_eq!(head(&repo), second);
    // HEAD moved since (a terminal committed again): refused, nothing taken back.
    assert_eq!(repo.undo_commit(&first, None).unwrap(), None);
    assert_eq!(head(&repo), second);
}

#[test]
fn a_commit_is_on_the_upstream_once_pushed() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    let first = repo.commit("a", false).unwrap();
    assert!(!repo.on_upstream(&first).unwrap(), "no upstream yet");
    let _remote = with_remote(&repo);
    assert!(repo.on_upstream(&first).unwrap(), "pushed");
    write(dir.path(), "a.txt", "two\n");
    repo.stage(&["a.txt"]).unwrap();
    let second = repo.commit("b", false).unwrap();
    assert!(!repo.on_upstream(&second).unwrap(), "not pushed");
    repo.run(&["checkout", "-q", "--detach"]).unwrap();
    assert!(!repo.on_upstream(&second).unwrap(), "detached: no upstream");
}

#[test]
fn stage_all_takes_new_changed_and_deleted_files() {
    let (dir, repo) = fresh();
    write(dir.path(), "keep.txt", "1\n");
    write(dir.path(), "gone.txt", "1\n");
    repo.stage(&["keep.txt", "gone.txt"]).unwrap();
    repo.commit("base", false).unwrap();
    write(dir.path(), "keep.txt", "2\n");
    std::fs::remove_file(dir.path().join("gone.txt")).unwrap();
    write(dir.path(), "new/n.txt", "n\n");
    repo.stage_all().unwrap();
    let s = repo.status().unwrap();
    let staged: Vec<_> = s
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.staged))
        .collect();
    assert_eq!(
        staged,
        [
            ("gone.txt", Some(Change::Deleted)),
            ("keep.txt", Some(Change::Modified)),
            ("new/n.txt", Some(Change::Added)),
        ]
    );
    assert!(s.unstaged().is_empty());
}

#[test]
fn a_file_that_isnt_utf8_has_no_hunk_patch() {
    let (dir, repo) = fresh();
    std::fs::write(dir.path().join("l.txt"), b"a\nb\ncaf\xe9\nc\n").unwrap();
    repo.stage(&["l.txt"]).unwrap();
    repo.commit("latin-1", false).unwrap();
    std::fs::write(dir.path().join("l.txt"), b"a\nB\ncaf\xe9\nc\n").unwrap();
    let d = working(&repo);
    assert_eq!(d.unstaged[0].path(), "l.txt");
    assert_eq!(
        templum_git::hunk_patch(&d.unstaged[0], 0),
        None,
        "a patch from the lossy text would write U+FFFD over \\xe9"
    );
}

#[test]
fn undoing_an_amend_after_a_later_commit_was_undone_lands_on_the_replaced_commit() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    let x = repo.commit("X", false).unwrap();
    write(dir.path(), "a.txt", "two\n");
    repo.stage(&["a.txt"]).unwrap();
    let replaced = repo.head().unwrap();
    let x2 = repo.commit("X, amended", true).unwrap();
    write(dir.path(), "a.txt", "three\n");
    repo.stage(&["a.txt"]).unwrap();
    let y = repo.commit("Y", false).unwrap();
    repo.undo_commit(&y, None).unwrap().unwrap();
    assert_eq!(head(&repo), x2);
    // HEAD@{1} is Y now: the amend's Undo must go back to X, not forward to Y.
    repo.undo_commit(&x2, Some(&replaced)).unwrap().unwrap();
    assert_eq!(head(&repo), x);
}

#[test]
fn an_upstream_gone_from_the_server_isnt_one() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a", false).unwrap();
    let _remote = with_remote(&repo);
    repo.run(&["switch", "-q", "-c", "topic"]).unwrap();
    write(dir.path(), "a.txt", "two\n");
    repo.stage(&["a.txt"]).unwrap();
    let mine = repo.commit("b", false).unwrap();
    repo.run(&["push", "-q", "-u", "origin", "topic"]).unwrap();
    // The pull request was merged and its branch deleted; a fetch prunes it.
    repo.run(&["push", "-q", "origin", "--delete", "topic"])
        .unwrap();
    repo.run(&["fetch", "-q", "--prune"]).unwrap();
    assert!(!repo.on_upstream(&mine).unwrap());
}

#[test]
fn the_default_branch_is_origin_heads_else_main_else_master() {
    let (dir, repo) = fresh();
    assert_eq!(repo.default_branch(), None, "no commit, no branch");
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    assert_eq!(repo.default_branch().as_deref(), Some("main"));
    repo.run(&["branch", "-m", "main", "master"]).unwrap();
    assert_eq!(repo.default_branch().as_deref(), Some("master"));
    // origin's HEAD names trunk: it wins over master.
    repo.run(&["branch", "-m", "master", "main"]).unwrap();
    let _remote = with_remote(&repo);
    repo.run(&["branch", "trunk"]).unwrap();
    repo.run(&["push", "-q", "origin", "trunk"]).unwrap();
    repo.run(&["remote", "set-head", "origin", "trunk"])
        .unwrap();
    assert_eq!(repo.default_branch().as_deref(), Some("trunk"));
    // No local trunk: the remote one.
    repo.run(&["branch", "-D", "trunk"]).unwrap();
    assert_eq!(repo.default_branch().as_deref(), Some("origin/trunk"));
    // Without origin/HEAD (not fetched after: git 2.48+ would make it again).
    repo.run(&["remote", "set-head", "origin", "-d"]).unwrap();
    assert_eq!(repo.default_branch().as_deref(), Some("main"));
}

#[test]
fn remotes_are_listed_and_publish_goes_to_the_one_named() {
    let (dir, repo) = fresh();
    assert!(repo.remotes().unwrap().is_empty());
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    let _origin = with_remote(&repo);
    let other = tempfile::tempdir().unwrap();
    std::process::Command::new("git")
        .args(["init", "-q", "--bare"])
        .current_dir(other.path())
        .status()
        .unwrap();
    repo.run(&[
        "remote",
        "add",
        "backup",
        &other.path().display().to_string(),
    ])
    .unwrap();
    assert_eq!(repo.remotes().unwrap(), ["backup", "origin"]);
    repo.run(&["checkout", "-q", "-b", "topic"]).unwrap();
    repo.publish("backup", "topic").unwrap();
    assert_eq!(
        repo.status().unwrap().upstream.as_deref(),
        Some("backup/topic")
    );
}

#[test]
fn a_branch_not_checked_out_is_pushed_to_its_own_upstream() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    let remote = with_remote(&repo);
    write(dir.path(), "a.txt", "two\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("two", false).unwrap();
    repo.run(&["checkout", "-q", "-b", "other"]).unwrap();
    repo.push_branch("main").unwrap();
    let theirs = std::process::Command::new("git")
        .args(["rev-parse", "main"])
        .current_dir(remote.path())
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&theirs.stdout).trim(),
        repo.run(&["rev-parse", "main"])
            .map(|o| String::from_utf8_lossy(&o).trim().to_string())
            .unwrap()
    );
}

#[test]
fn the_unmerged_count_is_what_only_that_branch_has() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    repo.run(&["checkout", "-q", "-b", "feat"]).unwrap();
    for n in ["b", "c", "d"] {
        write(dir.path(), n, n);
        repo.stage(&[n]).unwrap();
        repo.commit(n, false).unwrap();
    }
    repo.run(&["checkout", "-q", "main"]).unwrap();
    assert_eq!(repo.unmerged_count("feat").unwrap(), 3);
    repo.run(&["branch", "copy", "feat~1"]).unwrap();
    assert_eq!(
        repo.unmerged_count("feat").unwrap(),
        1,
        "two are on copy too"
    );
}

#[test]
fn branches_have_their_last_commits_date() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    let b = repo.branches().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!((now - b[0].time).abs() < 120, "{} vs {now}", b[0].time);
}

#[test]
fn clone_names_and_targets() {
    assert_eq!(clone_name("git@github.com:me/app.git"), "app");
    assert_eq!(clone_name("https://host/me/app/"), "app");
    assert_eq!(clone_name("https://host/me/app.git/"), "app");
    assert_eq!(clone_name("/srv/repos/tool"), "tool");
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        clone_target(empty.path(), "x/app.git"),
        empty.path(),
        "an empty folder takes it"
    );
    std::fs::write(empty.path().join("f"), "").unwrap();
    assert_eq!(
        clone_target(empty.path(), "x/app.git"),
        empty.path().join("app"),
        "beside what is there"
    );
    let missing = empty.path().join("new");
    assert_eq!(
        clone_target(&missing, "x/app.git"),
        missing,
        "a folder still to make"
    );
}

#[test]
fn clone_makes_the_repository_and_a_failed_one_leaves_nothing() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    let remote = with_remote(&repo);
    let parent = tempfile::tempdir().unwrap();
    let into = parent.path().join("copy");
    let cloned =
        Repo::clone_into("git", &remote.path().display().to_string(), &into, None).unwrap();
    assert!(cloned.root.join("a.txt").is_file());
    let bad = parent.path().join("bad");
    let err = Repo::clone_into(
        "git",
        "/no/such/repository",
        &bad,
        Some(Arc::new(AtomicBool::new(false))),
    )
    .unwrap_err();
    assert!(!err.missing, "{err}");
    assert!(!bad.exists(), "no half-made folder");
}

/// main (a.txt "one") and feat (b.txt), on main.
fn two_branches() -> (tempfile::TempDir, Repo) {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    repo.run(&["checkout", "-q", "-b", "feat"]).unwrap();
    write(dir.path(), "b.txt", "b\n");
    repo.stage(&["b.txt"]).unwrap();
    repo.commit("b", false).unwrap();
    repo.run(&["checkout", "-q", "main"]).unwrap();
    (dir, repo)
}

#[test]
fn leaving_changes_on_a_branch_brings_them_back_when_returning() {
    let (dir, repo) = two_branches();
    write(dir.path(), "a.txt", "changed\n");
    write(dir.path(), "new.txt", "untracked\n");
    repo.stage(&["a.txt"]).unwrap();
    assert_eq!(
        repo.switch("feat", false, Some("main")).unwrap(),
        Back::Nothing
    );
    assert_eq!(repo.status().unwrap().branch.as_deref(), Some("feat"));
    assert!(repo.status().unwrap().entries.is_empty(), "left on main");
    assert_eq!(
        repo.stashes().unwrap()[0].message,
        "On main: templum: left on main"
    );
    assert_eq!(repo.switch("main", false, None).unwrap(), Back::Popped);
    let s = repo.status().unwrap();
    let a = s.entries.iter().find(|e| e.path == "a.txt").unwrap();
    assert_eq!(a.staged, Some(Change::Modified), "staged again");
    assert!(dir.path().join("new.txt").is_file(), "untracked files too");
    assert!(repo.stashes().unwrap().is_empty(), "popped, not applied");
}

#[test]
fn bringing_changes_along_keeps_them_and_a_refused_switch_says_why() {
    let (dir, repo) = two_branches();
    write(dir.path(), "a.txt", "changed\n");
    assert_eq!(repo.switch("feat", false, None).unwrap(), Back::Nothing);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "changed\n"
    );
    repo.run(&["checkout", "-q", "main"]).unwrap();
    // feat changes a.txt too: carrying the change would overwrite it.
    repo.run(&["stash", "-q"]).unwrap();
    repo.run(&["checkout", "-q", "feat"]).unwrap();
    write(dir.path(), "a.txt", "feat's\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a on feat", false).unwrap();
    repo.run(&["checkout", "-q", "main"]).unwrap();
    repo.run(&["stash", "pop", "-q"]).unwrap();
    let err = repo.switch("feat", false, None).unwrap_err();
    assert!(err.stderr.contains("would be overwritten"), "{err}");
}

#[test]
fn a_switch_that_fails_after_leaving_brings_the_changes_back() {
    let (dir, repo) = two_branches();
    write(dir.path(), "a.txt", "changed\n");
    repo.stage(&["a.txt"]).unwrap();
    write(dir.path(), "a.txt", "changed twice\n");
    repo.switch("gone", false, Some("main")).unwrap_err();
    assert!(
        repo.stashes().unwrap().is_empty(),
        "nothing hidden in a stash"
    );
    let s = repo.status().unwrap();
    assert_eq!(s.branch.as_deref(), Some("main"));
    assert_eq!(s.entries[0].staged, Some(Change::Modified));
    assert_eq!(s.entries[0].unstaged, Some(Change::Modified));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "changed twice\n"
    );
}

#[test]
fn the_newest_stash_left_on_a_branch_is_popped_and_the_older_stays() {
    let (dir, repo) = two_branches();
    write(dir.path(), "a.txt", "first\n");
    repo.stash_push(&templum_git::left_on("main"), true)
        .unwrap();
    write(dir.path(), "a.txt", "second\n");
    repo.switch("feat", false, Some("main")).unwrap();
    assert_eq!(repo.switch("main", false, None).unwrap(), Back::Popped);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "second\n"
    );
    assert_eq!(
        repo.stashes().unwrap().len(),
        1,
        "the older one stays in Stashes"
    );
}

#[test]
fn a_pop_that_conflicts_keeps_the_stash_and_says_so() {
    let (dir, repo) = two_branches();
    write(dir.path(), "a.txt", "mine\n");
    repo.switch("feat", false, Some("main")).unwrap();
    // Meanwhile main moves on the same line (a terminal, an agent).
    repo.run(&["checkout", "-q", "main"]).unwrap();
    write(dir.path(), "a.txt", "theirs\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a again", false).unwrap();
    repo.run(&["checkout", "-q", "feat"]).unwrap();
    let back = repo.switch("main", false, None).unwrap();
    assert!(matches!(back, Back::Kept(_)), "{back:?}");
    assert_eq!(repo.stashes().unwrap().len(), 1, "kept");
}

#[test]
fn a_remote_branch_makes_its_tracking_branch_or_goes_to_the_local_one() {
    let (dir, repo) = two_branches();
    let _remote = with_remote(&repo);
    repo.run(&["push", "-q", "origin", "feat"]).unwrap();
    repo.run(&["branch", "-D", "feat"]).unwrap();
    repo.switch("origin/feat", true, None).unwrap();
    let s = repo.status().unwrap();
    assert_eq!(s.branch.as_deref(), Some("feat"));
    assert_eq!(s.upstream.as_deref(), Some("origin/feat"));
    repo.run(&["checkout", "-q", "main"]).unwrap();
    // A local feat exists now: origin/feat goes to it.
    repo.switch("origin/feat", true, None).unwrap();
    assert_eq!(repo.status().unwrap().branch.as_deref(), Some("feat"));
    assert!(dir.path().join("b.txt").is_file());
}

/// `repo` (main, a.txt) with an origin, and a second clone of it whose commits `repo` can get.
fn with_other(repo: &Repo) -> (tempfile::TempDir, tempfile::TempDir, Repo) {
    let remote = with_remote(repo);
    let other_dir = tempfile::tempdir().unwrap();
    let other = Repo::clone_into(
        "git",
        &remote.path().display().to_string(),
        &other_dir.path().join("o"),
        None,
    )
    .unwrap();
    for args in [
        ["config", "user.name", "Grace"].as_slice(),
        &["config", "user.email", "grace@example.com"],
        &["config", "commit.gpgsign", "false"],
    ] {
        other.run(args).unwrap();
    }
    (remote, other_dir, other)
}

fn commit_file(repo: &Repo, name: &str) {
    write(&repo.root, name, name);
    repo.stage(&[name]).unwrap();
    repo.commit(name, false).unwrap();
}

#[test]
fn sync_gets_and_sends_and_counts_both() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    let (_remote, _o, other) = with_other(&repo);
    assert_eq!(
        repo.sync().unwrap(),
        Synced::Done { got: 0, sent: 0 },
        "up to date"
    );
    for n in ["b", "c", "d"] {
        commit_file(&other, n);
    }
    other.push().unwrap();
    assert_eq!(repo.sync().unwrap(), Synced::Done { got: 3, sent: 0 });
    assert!(dir.path().join("d").is_file());
    commit_file(&repo, "e");
    commit_file(&repo, "f");
    assert_eq!(repo.sync().unwrap(), Synced::Done { got: 0, sent: 2 });
    let s = repo.status().unwrap();
    assert_eq!((s.ahead, s.behind), (0, 0));
}

#[test]
fn diverged_sync_changes_nothing_until_joined() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    let (_remote, _o, other) = with_other(&repo);
    commit_file(&other, "theirs");
    other.push().unwrap();
    commit_file(&repo, "mine");
    let head = repo.head().unwrap();
    assert_eq!(
        repo.sync().unwrap(),
        Synced::Diverged {
            ahead: 1,
            behind: 1
        }
    );
    assert_eq!(repo.head().unwrap(), head, "nothing joined yet");
    // Uncommitted changes don't stop the rebase (--autostash).
    write(dir.path(), "a.txt", "dirty\n");
    assert_eq!(
        repo.sync_join(true).unwrap(),
        Synced::Done { got: 1, sent: 1 }
    );
    let log = repo.run(&["log", "--format=%s"]).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log),
        "mine\ntheirs\none\n",
        "mine on top, no merge commit"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "dirty\n"
    );
}

#[test]
fn diverged_sync_joined_by_a_merge() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("one", false).unwrap();
    let (_remote, _o, other) = with_other(&repo);
    commit_file(&other, "theirs");
    other.push().unwrap();
    commit_file(&repo, "mine");
    repo.sync().unwrap();
    assert_eq!(
        repo.sync_join(false).unwrap(),
        Synced::Done { got: 1, sent: 2 },
        "mine and the merge"
    );
    assert_eq!(repo.status().unwrap().ahead, 0);
}
