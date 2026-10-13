//! The Git block's words for people: each Git word's one-sentence tooltip, and the sentences
//! that say where a branch stands. See docs/specs/git.md, Words.

use templum_git::{Operation, Status};

/// The tooltip of a Git word, as the spec's Words table gives it ("" for a word it hasn't).
pub(crate) fn tip(word: &str) -> &'static str {
    match word {
        "Stage" => "Choose this change to go into the next commit.",
        "Unstage" => "Take this change out of the next commit; the file keeps it.",
        "Discard" => {
            "Throw this change away and go back to the last commit's version. You can restore it from Recently discarded."
        }
        "Commit" => {
            "Save a version of the project with the staged changes, and a message saying what they do."
        }
        "Amend" => {
            "Replace the last commit with this one (its changes and message), instead of adding a new one."
        }
        "Push" => "Send your commits to the server (the remote), so others can get them.",
        "Pull" => "Get the server's new commits into this branch.",
        "Fetch" => "Look at what changed on the server, without changing your files.",
        "Sync" => "Pull, then push: your branch and the server's end up the same.",
        "Stash" => "Put your uncommitted changes aside, to bring them back later.",
        "Rebase" => "Put your commits on top of the others', as if you had started after them.",
        "Merge" => "Join another branch's commits into this one.",
        _ => "",
    }
}

/// An icon button's tooltip: the word, then its sentence ("Stage: Choose this change…").
pub(crate) fn named(word: &str) -> String {
    format!("{word}: {}", tip(word))
}

/// "1 commit", "3 commits".
pub(crate) fn commits(n: u32) -> String {
    if n == 1 {
        "1 commit".to_string()
    } else {
        format!("{n} commits")
    }
}

/// "1 changed file", "3 changed files".
pub(crate) fn changed_files(n: usize) -> String {
    if n == 1 {
        "1 changed file".to_string()
    } else {
        format!("{n} changed files")
    }
}

/// Where the branch stands with the server, and the tooltip saying it in words. Empty on a
/// detached HEAD and before the first commit.
pub(crate) fn server_state(s: &Status) -> (String, String) {
    if s.branch.is_none() || s.oid.is_none() {
        return (String::new(), String::new());
    }
    if s.upstream.is_none() || s.gone {
        let tip = if s.gone {
            "Its branch on the server was deleted. Publish branch sends it again."
        } else {
            "This branch isn't on the server yet. Publish branch sends it."
        };
        return ("not published".to_string(), tip.to_string());
    }
    if s.ahead == 0 && s.behind == 0 {
        return (
            "up to date".to_string(),
            "Your branch and the server's had the same commits at the last fetch.".to_string(),
        );
    }
    let mine = match s.ahead {
        0 => "none of yours to push".to_string(),
        1 => "1 commit of yours not pushed yet".to_string(),
        n => format!("{n} commits of yours not pushed yet"),
    };
    let theirs = match s.behind {
        0 => "nothing new on the server".to_string(),
        n => format!("{n} on the server you don't have"),
    };
    (
        format!("↑{} ↓{}", s.ahead, s.behind),
        format!("{mine}, {theirs}"),
    )
}

/// "origin/main" → "main": the name a person switches to.
// ponytail: assumes the remote's name has no '/'; read `git remote` if one does.
pub(crate) fn local_name(branch: &str) -> &str {
    branch.split_once('/').map_or(branch, |(_, b)| b)
}

/// The banner's sentence while a merge, rebase, cherry-pick or revert is under way.
pub(crate) fn operation_sentence(op: Operation, conflicts: usize) -> String {
    if conflicts > 0 {
        let files = if conflicts == 1 {
            "1 file".to_string()
        } else {
            format!("{conflicts} files")
        };
        return format!(
            "Git couldn't join the changes in {files} by itself. Choose which version stays in each part."
        );
    }
    let what = match op {
        Operation::Merge => "Merging another branch into this one.",
        Operation::Rebase => "Rebasing: your commits are being put on top of another branch.",
        Operation::CherryPick => "Copying a commit onto this branch (cherry-pick).",
        Operation::Revert => "Adding a commit that undoes another (revert).",
    };
    format!("{what} Continue to finish it, or Abort to go back.")
}

/// What a Sync did: "Up to date", "Got 3 commits", "Sent 2 commits", "Got 3 commits, sent 2".
pub(crate) fn synced(got: u32, sent: u32) -> String {
    match (got, sent) {
        (0, 0) => "Up to date".to_string(),
        (g, 0) => format!("Got {}", commits(g)),
        (0, s) => format!("Sent {}", commits(s)),
        (g, s) => format!("Got {}, sent {s}", commits(g)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(upstream: Option<&str>, ahead: u32, behind: u32, gone: bool) -> Status {
        Status {
            branch: Some("main".into()),
            oid: Some("0d817e9".into()),
            upstream: upstream.map(Into::into),
            ahead,
            behind,
            gone,
            entries: Vec::new(),
        }
    }

    #[test]
    fn every_word_has_its_sentence() {
        for word in [
            "Stage", "Unstage", "Discard", "Commit", "Amend", "Push", "Pull", "Fetch", "Sync",
            "Stash", "Rebase", "Merge",
        ] {
            assert!(tip(word).ends_with('.'), "{word}");
        }
        assert_eq!(
            tip("Sync"),
            "Pull, then push: your branch and the server's end up the same."
        );
        assert_eq!(tip("Nope"), "");
    }

    #[test]
    fn where_a_branch_stands() {
        assert_eq!(
            server_state(&status(Some("origin/main"), 2, 1, false)),
            (
                "↑2 ↓1".to_string(),
                "2 commits of yours not pushed yet, 1 on the server you don't have".to_string()
            )
        );
        assert_eq!(
            server_state(&status(Some("origin/main"), 0, 0, false)).0,
            "up to date"
        );
        assert_eq!(server_state(&status(None, 0, 0, false)).0, "not published");
        assert_eq!(
            server_state(&status(Some("origin/main"), 0, 0, true)).0,
            "not published",
            "gone: never up to date"
        );
        let mut detached = status(None, 0, 0, false);
        detached.branch = None;
        assert_eq!(server_state(&detached).0, "");
    }

    #[test]
    fn what_an_operation_under_way_says() {
        use templum_git::Operation;
        assert_eq!(
            operation_sentence(Operation::Merge, 2),
            "Git couldn't join the changes in 2 files by itself. Choose which version stays in each part."
        );
        assert_eq!(
            operation_sentence(Operation::Rebase, 1),
            "Git couldn't join the changes in 1 file by itself. Choose which version stays in each part."
        );
        assert!(operation_sentence(Operation::Rebase, 0).starts_with("Rebasing"));
        assert!(
            operation_sentence(Operation::Merge, 0)
                .ends_with("Continue to finish it, or Abort to go back.")
        );
        assert_eq!(local_name("origin/main"), "main");
        assert_eq!(local_name("main"), "main");
    }

    #[test]
    fn what_sync_did() {
        assert_eq!(synced(0, 0), "Up to date");
        assert_eq!(synced(3, 2), "Got 3 commits, sent 2");
        assert_eq!(synced(1, 0), "Got 1 commit");
        assert_eq!(synced(0, 2), "Sent 2 commits");
        assert_eq!(changed_files(3), "3 changed files");
    }
}
