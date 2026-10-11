//! Runs the real `git` on a temporary repository. CI's runners have it.

use std::path::Path;

use templum_git::{Change, Operation, Pull, Repo, Side};

/// A new repository on `main` with an author set, so commits work on a bare runner.
fn fresh() -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::init("git", dir.path()).expect("git init");
    for args in [
        ["symbolic-ref", "HEAD", "refs/heads/main"].as_slice(),
        &["config", "user.name", "Ada Lovelace"],
        &["config", "user.email", "ada@example.com"],
        &["config", "commit.gpgsign", "false"],
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
    let diff = repo.diff(renamed, true).unwrap();
    assert!(diff.contains("rename from old.txt"), "{diff}");
}

#[test]
fn the_diff_of_each_kind_of_entry() {
    let (dir, repo) = fresh();
    write(dir.path(), "a.txt", "one\n");
    write(dir.path(), "bin", "\0\x01\x02");
    let s = repo.status().unwrap();
    let a = s.entries.iter().find(|e| e.path == "a.txt").unwrap();
    assert_eq!(
        repo.diff(a, false).unwrap(),
        "+++ b/a.txt\n@@ -0,0 +1,1 @@\n+one\n"
    );
    let bin = s.entries.iter().find(|e| e.path == "bin").unwrap();
    assert_eq!(repo.diff(bin, false).unwrap(), "Binary file");
    repo.stage(&["a.txt"]).unwrap();
    repo.commit("a", false).unwrap();
    write(dir.path(), "a.txt", "one\ntwo\n");
    let s = repo.status().unwrap();
    let a = s.entries.iter().find(|e| e.path == "a.txt").unwrap();
    let diff = repo.diff(a, false).unwrap();
    assert!(
        diff.contains("@@ -1 +1,2 @@") && diff.contains("+two"),
        "{diff}"
    );
    repo.stage(&["a.txt"]).unwrap();
    let s = repo.status().unwrap();
    assert!(repo.diff(&s.entries[0], true).unwrap().contains("+two"));
    assert_eq!(
        repo.diff(&s.entries[0], false).unwrap(),
        "",
        "nothing unstaged"
    );
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
        "*.log\nbuild/\nsecret.txt\n"
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
    assert!(
        repo.diff(s.conflicts()[0], false)
            .unwrap()
            .contains("<<<<<<<")
    );
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
    repo.push(None).unwrap();
    assert_eq!(repo.status().unwrap().ahead, 0);
    // A new branch has no upstream until it is published.
    repo.run(&["checkout", "-q", "-b", "topic"]).unwrap();
    assert_eq!(repo.status().unwrap().upstream, None);
    repo.push(Some("topic")).unwrap();
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
