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
fn a_huge_untracked_file_is_named_not_shown() {
    let (dir, repo) = fresh();
    write(dir.path(), "big.csv", &"a,b\n".repeat(1_310_720));
    let s = repo.status().unwrap();
    assert_eq!(
        repo.diff(&s.entries[0], false).unwrap(),
        "Too large to show (5.0 MB)"
    );
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
