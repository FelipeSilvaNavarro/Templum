//! Runs the real `git` on a temporary repository. CI's runners have it.

use std::path::Path;

use templum_git::{Change, Operation, Repo};

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

#[test]
fn a_merge_in_progress_is_an_operation_and_aborting_ends_it() {
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
    assert_eq!(repo.operation(), Some(Operation::Merge));
    let s = repo.status().unwrap();
    assert_eq!(s.conflicts().len(), 1);
    assert!(
        repo.diff(s.conflicts()[0], false)
            .unwrap()
            .contains("<<<<<<<")
    );
    repo.run(&["merge", "--abort"]).unwrap();
    assert_eq!(repo.operation(), None);
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
