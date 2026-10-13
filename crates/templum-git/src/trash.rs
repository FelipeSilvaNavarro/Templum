//! The trash: what a discard throws away, kept 7 days (the newest 50 discards), so it can be put
//! back. One folder per discard in Templum's data folder: the files as they were, and a manifest.
//! See docs/specs/git.md, section 4, The trash.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Past either, a discard keeps no copy, and the question says so before it is done.
pub const MAX_FILES: usize = 500;
pub const MAX_BYTES: u64 = 100 * 1024 * 1024;
/// Discards older than this, or past the newest KEEP, are deleted.
pub const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
pub const KEEP: usize = 50;

/// The trash's folder (`trash/` in Templum's data folder).
#[derive(Clone, Debug)]
pub struct Trash {
    dir: PathBuf,
}

/// One discard, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    /// Its folder's name: they sort by time.
    pub id: String,
    /// The repository's root.
    pub root: PathBuf,
    /// Unix seconds.
    pub time: u64,
    /// What was done: "Discarded src/x.rs".
    pub what: String,
    /// Each path, relative to the root, and whether it existed before the discard.
    pub files: Vec<(String, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Restored {
    /// Put back; the discard left the trash.
    Done,
    /// These files changed since the discard, so nothing was written: `force` overwrites them.
    Changed(Vec<String>),
}

/// How many files a discard of `paths` keeps, and their size.
pub fn measure(root: &Path, paths: &[String]) -> (usize, u64) {
    let bytes = paths
        .iter()
        .filter_map(|p| fs::symlink_metadata(root.join(p)).ok())
        .map(|m| m.len())
        .sum();
    (paths.len(), bytes)
}

/// A discard this size gets a copy.
pub fn fits(files: usize, bytes: u64) -> bool {
    files <= MAX_FILES && bytes <= MAX_BYTES
}

/// A file's state, to tell later whether it changed: its size and modification time (a link
/// itself, not its target), or `-` when it doesn't exist.
fn stamp(path: &Path) -> String {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{}:{mtime}", m.len())
        }
        Err(_) => "-".to_string(),
    }
}

impl Trash {
    pub fn new(dir: PathBuf) -> Trash {
        Trash { dir }
    }

    /// Copies `paths` (relative to `root`) as they are now, before a discard. On a failure the
    /// half-made entry is removed: the caller must not discard then.
    pub fn put(
        &self,
        root: &Path,
        paths: &[String],
        what: &str,
        now: SystemTime,
    ) -> io::Result<Kept> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let id = format!(
            "{:013}-{}-{}",
            since.as_millis(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let entry = self.dir.join(&id);
        let kept = Kept {
            id,
            root: root.to_path_buf(),
            time: since.as_secs(),
            what: what.to_string(),
            files: Vec::new(),
        };
        self.fill(&entry, kept, paths).inspect_err(|_| {
            let _ = fs::remove_dir_all(&entry);
        })
    }

    fn fill(&self, entry: &Path, mut kept: Kept, paths: &[String]) -> io::Result<Kept> {
        let files = entry.join("files");
        fs::create_dir_all(&files)?;
        for (i, path) in paths.iter().enumerate() {
            let from = kept.root.join(path);
            let meta = fs::symlink_metadata(&from).ok();
            match &meta {
                Some(m) if m.file_type().is_symlink() => {
                    let target = fs::read_link(&from)?;
                    fs::write(
                        files.join(format!("{i}.link")),
                        target.to_string_lossy().as_bytes(),
                    )?;
                }
                Some(_) => {
                    fs::copy(&from, files.join(i.to_string()))?;
                }
                None => {}
            }
            kept.files.push((path.clone(), meta.is_some()));
        }
        let mut manifest = format!(
            "{}\0{}\0{}\0",
            kept.root.to_string_lossy(),
            kept.time,
            kept.what
        );
        for (path, existed) in &kept.files {
            manifest.push_str(&format!("{path}\0{}\0", u8::from(*existed)));
        }
        fs::write(entry.join("manifest"), manifest)?;
        Ok(kept)
    }

    /// After the discard: each file's state then, so a restore can tell one changed since.
    pub fn seal(&self, kept: &Kept) -> io::Result<()> {
        let after: String = kept
            .files
            .iter()
            .map(|(path, _)| stamp(&kept.root.join(path)) + "\0")
            .collect();
        fs::write(self.dir.join(&kept.id).join("after"), after)
    }

    /// Every discard kept, newest first. One that can't be read is left out.
    pub fn list(&self) -> Vec<Kept> {
        let mut all: Vec<Kept> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| read_manifest(&e.path(), e.file_name().to_string_lossy().into_owned()))
            .collect();
        all.sort_by(|a, b| b.id.cmp(&a.id));
        all
    }

    /// Puts discard `id` back. Unless `force`, a file changed since the discard stops it,
    /// nothing written, and is named. Done, the discard leaves the trash.
    pub fn restore(&self, id: &str, force: bool) -> io::Result<Restored> {
        let entry = self.dir.join(id);
        let kept = read_manifest(&entry, id.to_string()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "this discard is no longer in the trash",
            )
        })?;
        if !force && let Ok(after) = fs::read_to_string(entry.join("after")) {
            let changed: Vec<String> = kept
                .files
                .iter()
                .zip(after.split('\0'))
                .filter(|((path, _), then)| stamp(&kept.root.join(path)) != *then)
                .map(|((path, _), _)| path.clone())
                .collect();
            if !changed.is_empty() {
                return Ok(Restored::Changed(changed));
            }
        }
        let files = entry.join("files");
        for (i, (path, existed)) in kept.files.iter().enumerate() {
            let to = kept.root.join(path);
            let link = files.join(format!("{i}.link"));
            let now = fs::symlink_metadata(&to).ok();
            let is_link = now.as_ref().is_some_and(|m| m.file_type().is_symlink());
            // A link is replaced, never written through; a file that didn't exist goes.
            if now.is_some() && (!existed || link.exists() || is_link) {
                fs::remove_file(&to)?;
            }
            if !existed {
                continue;
            }
            if let Some(dir) = to.parent() {
                fs::create_dir_all(dir)?;
            }
            if link.exists() {
                restore_link(&fs::read_to_string(&link)?, &to)?;
            } else {
                fs::copy(files.join(i.to_string()), &to)?;
            }
        }
        fs::remove_dir_all(&entry)?;
        Ok(Restored::Done)
    }

    pub fn remove(&self, id: &str) -> io::Result<()> {
        fs::remove_dir_all(self.dir.join(id))
    }

    /// Deletes the discards older than MAX_AGE and those past the newest KEEP; how many.
    pub fn prune(&self, now: SystemTime) -> io::Result<usize> {
        let now = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let mut removed = 0;
        for (i, kept) in self.list().into_iter().enumerate() {
            if i >= KEEP || now.saturating_sub(kept.time) > MAX_AGE.as_secs() {
                self.remove(&kept.id)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn read_manifest(entry: &Path, id: String) -> Option<Kept> {
    let text = fs::read_to_string(entry.join("manifest")).ok()?;
    let mut f = text.split('\0');
    let (root, time, what) = (f.next()?, f.next()?.parse().ok()?, f.next()?);
    let mut files = Vec::new();
    while let (Some(path), Some(existed)) = (f.next(), f.next()) {
        files.push((path.to_string(), existed == "1"));
    }
    Some(Kept {
        id,
        root: PathBuf::from(root),
        time,
        what: what.to_string(),
        files,
    })
}

#[cfg(unix)]
fn restore_link(target: &str, to: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, to)
}

/// Without symlinks (git's core.symlinks=false on Windows) a link is a file holding its target.
#[cfg(not(unix))]
fn restore_link(target: &str, to: &Path) -> io::Result<()> {
    fs::write(to, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, tempfile::TempDir, Trash) {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let trash = Trash::new(data.path().join("trash"));
        (root, data, trash)
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn put_keeps_the_files_and_that_one_didnt_exist_and_restore_puts_them_back() {
        let (root, _data, trash) = setup();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/x.rs"), "mine\n").unwrap();
        let paths = vec!["src/x.rs".to_string(), "gone.txt".to_string()];
        let kept = trash
            .put(root.path(), &paths, "Discarded 2 files", at(1_800_000_000))
            .unwrap();
        assert_eq!(
            kept.files,
            [
                ("src/x.rs".to_string(), true),
                ("gone.txt".to_string(), false)
            ]
        );
        // The discard: x.rs back to the committed text, gone.txt (an unstaged deletion) back.
        std::fs::write(root.path().join("src/x.rs"), "committed\n").unwrap();
        std::fs::write(root.path().join("gone.txt"), "brought back by git\n").unwrap();
        trash.seal(&kept).unwrap();
        let listed = trash.list();
        assert_eq!(listed, std::slice::from_ref(&kept));
        assert_eq!(listed[0].what, "Discarded 2 files");
        assert_eq!(listed[0].root, root.path());
        assert_eq!(listed[0].time, 1_800_000_000);
        assert_eq!(trash.restore(&kept.id, false).unwrap(), Restored::Done);
        assert_eq!(
            std::fs::read_to_string(root.path().join("src/x.rs")).unwrap(),
            "mine\n"
        );
        assert!(
            !root.path().join("gone.txt").exists(),
            "it didn't exist before the discard"
        );
        assert!(
            trash.list().is_empty(),
            "a restored discard leaves the trash"
        );
    }

    #[test]
    fn a_file_changed_since_the_discard_is_named_and_left_alone_unless_forced() {
        let (root, _data, trash) = setup();
        std::fs::write(root.path().join("a.txt"), "mine\n").unwrap();
        let kept = trash
            .put(
                root.path(),
                &["a.txt".to_string()],
                "Discarded a.txt",
                SystemTime::now(),
            )
            .unwrap();
        std::fs::write(root.path().join("a.txt"), "committed\n").unwrap();
        trash.seal(&kept).unwrap();
        // Another length: a fast disk can keep the same modification time.
        std::fs::write(root.path().join("a.txt"), "edited since then\n").unwrap();
        assert_eq!(
            trash.restore(&kept.id, false).unwrap(),
            Restored::Changed(vec!["a.txt".to_string()])
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("a.txt")).unwrap(),
            "edited since then\n",
            "nothing written"
        );
        assert_eq!(trash.restore(&kept.id, true).unwrap(), Restored::Done);
        assert_eq!(
            std::fs::read_to_string(root.path().join("a.txt")).unwrap(),
            "mine\n"
        );
    }

    #[test]
    fn pruning_drops_discards_older_than_a_week_and_past_the_newest_fifty() {
        let (root, _data, trash) = setup();
        std::fs::write(root.path().join("a.txt"), "x").unwrap();
        let now = 1_800_000_000;
        let a = ["a.txt".to_string()];
        let old = trash
            .put(root.path(), &a, "old", at(now - 8 * 24 * 3600))
            .unwrap();
        for i in 0..KEEP + 2 {
            trash
                .put(root.path(), &a, &format!("n{i}"), at(now - 100 + i as u64))
                .unwrap();
        }
        assert_eq!(
            trash.prune(at(now)).unwrap(),
            3,
            "the old one, and the two oldest past fifty"
        );
        let left = trash.list();
        assert_eq!(left.len(), KEEP);
        assert!(left.iter().all(|k| k.id != old.id));
        assert_eq!(left[0].what, format!("n{}", KEEP + 1), "newest first");
    }

    #[test]
    fn measure_and_the_cap() {
        let (root, _data, _trash) = setup();
        std::fs::write(root.path().join("a"), "12345").unwrap();
        assert_eq!(
            measure(root.path(), &["a".to_string(), "missing".to_string()]),
            (2, 5)
        );
        assert!(fits(MAX_FILES, MAX_BYTES));
        assert!(!fits(MAX_FILES + 1, 0));
        assert!(!fits(1, MAX_BYTES + 1));
    }

    #[test]
    fn a_discard_that_cant_be_kept_leaves_nothing_behind() {
        let (root, _data, trash) = setup();
        std::fs::create_dir(root.path().join("dir")).unwrap();
        // A folder isn't a file git lists: copying it fails, and the half-made entry goes.
        assert!(
            trash
                .put(root.path(), &["dir".to_string()], "x", SystemTime::now())
                .is_err()
        );
        assert!(trash.list().is_empty());
        assert!(trash.restore("no-such-entry", false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_comes_back_as_a_symlink() {
        let (root, _data, trash) = setup();
        std::os::unix::fs::symlink("target.txt", root.path().join("link")).unwrap();
        let kept = trash
            .put(
                root.path(),
                &["link".to_string()],
                "Discarded link",
                SystemTime::now(),
            )
            .unwrap();
        assert_eq!(
            kept.files,
            [("link".to_string(), true)],
            "a dangling link exists"
        );
        std::fs::remove_file(root.path().join("link")).unwrap();
        trash.seal(&kept).unwrap();
        assert_eq!(trash.restore(&kept.id, false).unwrap(), Restored::Done);
        assert_eq!(
            std::fs::read_link(root.path().join("link")).unwrap(),
            Path::new("target.txt")
        );
    }
}
