//! An account's folder: its items made its own or links to another folder's, as its choices say
//! (docs/specs/accounts.md, Its folder and its items). Only the links Templum would make are ever
//! replaced or removed, and never what they point at; a real item is moved only in a folder
//! Templum made; the login is never opened.

use std::io;
use std::path::{Path, PathBuf};

use super::{Item, Kind};

/// When an account's choices are applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// The dialog saved them: a real item in the way of a link is kept aside, in Templum's folder.
    Saved,
    /// The account starts: a real item is left, and reported.
    Starting,
}

/// One item's choice.
#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    pub key: &'static str,
    /// The state folder it is shared with; None: its own.
    pub from: Option<PathBuf>,
}

/// What applying an account's choices met.
#[derive(Debug, Default)]
pub struct Report {
    /// `"{label}: {error}"`, one per path that couldn't be done.
    pub problems: Vec<String>,
    /// Shared items the agent replaced with its own copy (found at a start).
    pub replaced: Vec<&'static str>,
    /// Real items kept aside on Save to make their links, and where they went.
    pub kept: Vec<(&'static str, PathBuf)>,
    /// Shared items left real in a folder the person chose, where Templum never moves files.
    pub left: Vec<&'static str>,
    /// Items holding a link Templum didn't make, and where it points.
    pub theirs: Vec<(&'static str, PathBuf)>,
}

/// What a folder holds for an item.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    Own,
    /// A link to the same item of this state folder.
    Linked(PathBuf),
    /// A link Templum wouldn't make, to this target.
    Theirs(PathBuf),
}

/// Why a file stays its own on Windows without the privilege.
pub const NO_FILE_LINKS: &str = "Windows shares a file only with Developer Mode on (Settings ▸ \
                                 System ▸ For developers). Folders share without it.";

/// What is at one path of an item.
enum At {
    Missing,
    Real,
    /// A link to `<one of the sources>/<the same path>`: Templum's.
    Ours(PathBuf),
    Theirs(PathBuf),
}

fn at(path: &Path, name: &str, sources: &[PathBuf]) -> At {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return At::Missing;
    };
    // A junction is a symbolic link to std on Windows too.
    if !meta.file_type().is_symlink() {
        return At::Real;
    }
    let Ok(target) = std::fs::read_link(path) else {
        return At::Real;
    };
    let target = plain(target);
    if sources.iter().any(|s| s.join(name) == target) {
        At::Ours(target)
    } else {
        At::Theirs(target)
    }
}

/// A link's target without Windows' `\\?\`, which a junction's has.
fn plain(target: PathBuf) -> PathBuf {
    match target.to_str().and_then(|t| t.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => target,
    }
}

/// `path`'s name and whether it is a folder.
fn name(path: &str) -> (&str, bool) {
    match path.strip_suffix('/') {
        Some(name) => (name, true),
        None => (path, false),
    }
}

fn make_dir(path: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

fn link(source: &Path, path: &Path, dir: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        let _ = dir;
        std::os::unix::fs::symlink(source, path)
    }
    #[cfg(windows)]
    {
        if dir {
            junction::create(source, path)
        } else {
            std::os::windows::fs::symlink_file(source, path)
        }
    }
}

/// Removes the link at `path`, never what it points at.
fn unlink(path: &Path, dir: bool) -> io::Result<()> {
    // A junction (or a folder's symbolic link) is removed as a folder on Windows: its target stays.
    if cfg!(windows) && dir {
        std::fs::remove_dir(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// The first free `<path>.own`, `<path>.own-2`, …
fn aside(path: &Path) -> PathBuf {
    let with = |suffix: String| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    (1..)
        .map(|n| {
            with(if n == 1 {
                ".own".into()
            } else {
                format!(".own-{n}")
            })
        })
        .find(|p| std::fs::symlink_metadata(p).is_err())
        .expect("a free name")
}

/// Whether `path` is a link (a junction too, on Windows).
fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Whether `a` and `b` are one file or folder, links followed.
fn same(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

fn own_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".own");
    PathBuf::from(name)
}

/// Applies an account's `shares` to its `folder`: each item a link to the same item of the
/// state folder it is shared with, or its own. `sources` are every state folder a share of this
/// agent may point at: a link to one of them is Templum's, and only those are replaced or removed.
/// `made`: Templum made `folder` (`<data>/accounts/<id>`), the only one where it moves a real item
/// (aside on Save, back when its own again), unless it or its state folder is a link; in a folder
/// the person chose it only links.
pub fn apply(
    kind: &Kind,
    folder: &Path,
    shares: &[Share],
    sources: &[PathBuf],
    when: When,
    made: bool,
) -> Report {
    apply_with(kind, folder, shares, sources, when, made, files_share())
}

/// `apply`, where a file can be shared by a link (`files`) or stays its own.
fn apply_with(
    kind: &Kind,
    folder: &Path,
    shares: &[Share],
    sources: &[PathBuf],
    when: When,
    made: bool,
    files: bool,
) -> Report {
    let mut report = Report::default();
    let state = kind.state_dir(folder);
    if let Err(e) = make_dir(&state) {
        report.problems.push(format!("{}: {e}", state.display()));
        return report;
    }
    // Made, but a link to a folder of the person's (`~/.claude`): theirs, as one they chose.
    let made = made && !is_link(folder) && !is_link(&state);
    for item in kind.items {
        let from = shares
            .iter()
            .find(|s| s.key == item.key)
            .and_then(|s| s.from.as_deref());
        for path in item.paths {
            let (name, dir) = name(path);
            if let Err(e) = apply_path(
                item,
                &state.join(name),
                name,
                dir,
                from,
                sources,
                when,
                made,
                files,
                &mut report,
            ) {
                let problem = format!("{}: {e}", item.label);
                if !report.problems.contains(&problem) {
                    report.problems.push(problem);
                }
            }
        }
    }
    report
}

#[allow(clippy::too_many_arguments)]
fn apply_path(
    item: &'static Item,
    path: &Path,
    name: &str,
    dir: bool,
    from: Option<&Path>,
    sources: &[PathBuf],
    when: When,
    made: bool,
    files: bool,
    report: &mut Report,
) -> io::Result<()> {
    let here = at(path, name, sources);
    if let At::Theirs(target) = &here {
        if !report.theirs.iter().any(|(k, _)| *k == item.key) {
            report.theirs.push((item.key, target.clone()));
        }
        return Ok(());
    }
    let Some(from) = from else {
        // Its own: only a link of Templum's goes, and what was kept aside comes back.
        if let At::Ours(_) = here {
            unlink(path, dir)?;
            let own = own_path(path);
            if made && std::fs::symlink_metadata(&own).is_ok() {
                std::fs::rename(own, path)?;
            }
        }
        return Ok(());
    };
    let source = from.join(name);
    if dir {
        if !source.exists() {
            std::fs::create_dir_all(&source)?;
        }
    } else if std::fs::symlink_metadata(&source).is_err() {
        // Linked at a later start, once the agent wrote it.
        return Ok(());
    } else if !files {
        // It stays its own: Save says why, beside the item too; a start would say it every time.
        let linked = matches!(&here, At::Ours(target) if *target == source);
        if !linked && when == When::Saved {
            return Err(io::Error::other(NO_FILE_LINKS));
        }
        return Ok(());
    }
    // Never a link to itself: the account's folder, or the item, is the source's through a link.
    if path.parent().is_some_and(|p| same(p, from))
        || matches!(here, At::Real) && same(path, &source)
    {
        return Err(io::Error::other(format!(
            "it is {}'s own through a link, so it isn't linked to itself",
            from.display()
        )));
    }
    match here {
        At::Missing => link(&source, path, dir),
        At::Ours(target) if target == source => Ok(()),
        At::Ours(_) => {
            unlink(path, dir)?;
            link(&source, path, dir)
        }
        At::Real if !made => {
            if !report.left.contains(&item.key) {
                report.left.push(item.key);
            }
            Ok(())
        }
        At::Real if when == When::Saved => {
            let aside = aside(path);
            std::fs::rename(path, &aside)?;
            report.kept.push((item.key, aside));
            link(&source, path, dir)
        }
        At::Real => {
            if !report.replaced.contains(&item.key) {
                report.replaced.push(item.key);
            }
            Ok(())
        }
        At::Theirs(_) => Ok(()),
    }
}

/// What `folder` already holds, item by item (an item's first path decides), for the dialog.
pub fn read(kind: &Kind, folder: &Path, sources: &[PathBuf]) -> Vec<(&'static str, Found)> {
    let state = kind.state_dir(folder);
    kind.items
        .iter()
        .map(|item| {
            let (name, _) = name(item.paths[0]);
            let found = match at(&state.join(name), name, sources) {
                At::Ours(target) => {
                    Found::Linked(target.parent().map(Path::to_path_buf).unwrap_or(target))
                }
                At::Theirs(target) => Found::Theirs(target),
                At::Missing | At::Real => Found::Own,
            };
            (item.key, found)
        })
        .collect()
}

/// Deletes an account's `folder`: its items' links first (only the links), then the folder,
/// which never follows a link. A folder that is a link itself: only that link.
pub fn delete(kind: &Kind, folder: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(folder)?.file_type().is_symlink() {
        return unlink(folder, true);
    }
    let state = kind.state_dir(folder);
    // A state folder that is a link (Gemini CLI's `.gemini` to `~/.gemini`): only the link goes.
    let items = if is_link(&state) { &[][..] } else { kind.items };
    for path in items.iter().flat_map(|i| i.paths) {
        let (name, dir) = name(path);
        let path = state.join(name);
        if is_link(&path) {
            unlink(&path, dir)?;
        }
    }
    std::fs::remove_dir_all(folder)
}

/// Whether a file can be shared: always on Unix; on Windows, whether a file's symbolic link can
/// be made (Developer Mode, or an administrator), tried once.
pub fn files_share() -> bool {
    #[cfg(unix)]
    {
        true
    }
    #[cfg(windows)]
    {
        static CAN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *CAN.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("templum-link-{}", std::process::id()));
            let ok = std::fs::create_dir_all(&dir).is_ok()
                && std::fs::write(dir.join("a"), "").is_ok()
                && std::os::windows::fs::symlink_file(dir.join("a"), dir.join("b")).is_ok();
            let _ = std::fs::remove_dir_all(&dir);
            ok
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::accounts::{Keychain, item};
    use std::fs;

    const FAKE: Kind = Kind {
        agent: "fake",
        var: "FAKE_HOME",
        own: ".fake",
        state: None,
        items: &[
            item("skills", "Skills", &["skills/"]),
            item("settings", "Settings", &["settings.json"]),
            item("agents", "Agents", &["agents/"]),
            item("commands", "Commands", &["commands/"]),
        ],
        login: &[".credentials.json"],
        login_command: "fake login",
        login_hint: None,
        overrides: &[],
        keychain: Keychain::Never,
    };

    /// A source folder and an account's, each with its login.
    struct Two {
        _dir: tempfile::TempDir,
        root: PathBuf,
        src: PathBuf,
        acc: PathBuf,
    }

    impl Two {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let (src, acc) = (root.join("src"), root.join("acc"));
            for (d, login) in [(&src, "source login"), (&acc, "account login")] {
                fs::create_dir_all(d).unwrap();
                fs::write(d.join(".credentials.json"), login).unwrap();
            }
            Self {
                _dir: dir,
                root,
                src,
                acc,
            }
        }

        fn share(&self, key: &'static str) -> Vec<Share> {
            vec![Share {
                key,
                from: Some(self.src.clone()),
            }]
        }

        fn apply(&self, shares: &[Share], when: When) -> Report {
            let report = apply(
                &FAKE,
                &self.acc,
                shares,
                std::slice::from_ref(&self.src),
                when,
                true,
            );
            self.logins_intact();
            report
        }

        fn logins_intact(&self) {
            assert_eq!(
                fs::read(self.src.join(".credentials.json")).unwrap(),
                b"source login"
            );
            if self.acc.exists() {
                assert_eq!(
                    fs::read(self.acc.join(".credentials.json")).unwrap(),
                    b"account login"
                );
            }
        }
    }

    #[test]
    fn a_shared_folder_is_a_link_to_the_sources() {
        let t = Two::new();
        let report = t.apply(&t.share("skills"), When::Saved);
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert_eq!(
            fs::read_link(t.acc.join("skills")).unwrap(),
            t.src.join("skills")
        );
        assert!(t.src.join("skills").is_dir(), "made empty");
    }

    #[test]
    fn a_missing_source_file_is_linked_later() {
        let t = Two::new();
        let report = t.apply(&t.share("settings"), When::Saved);
        assert!(report.problems.is_empty());
        assert!(fs::symlink_metadata(t.acc.join("settings.json")).is_err());
        fs::write(t.src.join("settings.json"), "{}").unwrap();
        t.apply(&t.share("settings"), When::Starting);
        assert_eq!(
            fs::read_link(t.acc.join("settings.json")).unwrap(),
            t.src.join("settings.json")
        );
    }

    #[test]
    fn a_real_item_is_kept_aside_on_save_and_left_at_a_start() {
        let t = Two::new();
        fs::create_dir_all(t.acc.join("skills/a")).unwrap();
        t.apply(&t.share("skills"), When::Saved);
        assert!(t.acc.join("skills.own/a").is_dir());
        assert!(is_link(&t.acc.join("skills")));

        fs::remove_file(t.acc.join("skills")).unwrap();
        fs::create_dir_all(t.acc.join("skills/b")).unwrap();
        let report = t.apply(&t.share("skills"), When::Saved);
        assert!(t.acc.join("skills.own-2/b").is_dir(), "skills.own is taken");
        assert_eq!(
            report.kept,
            [("skills", t.acc.join("skills.own-2"))],
            "said"
        );
        assert!(t.acc.join("skills.own/a").is_dir());

        fs::write(t.src.join("settings.json"), "{}").unwrap();
        fs::write(t.acc.join("settings.json"), "mine").unwrap();
        let report = t.apply(&t.share("settings"), When::Starting);
        assert!(!is_link(&t.acc.join("settings.json")));
        assert_eq!(fs::read(t.acc.join("settings.json")).unwrap(), b"mine");
        assert_eq!(report.replaced, ["settings"]);
    }

    /// Without the privilege a file needs (Windows without Developer Mode), a shared file stays
    /// its own: Save says why, a start says nothing (it would at every start).
    #[test]
    fn a_file_that_cant_be_linked_is_told_on_save_only() {
        let t = Two::new();
        fs::write(t.src.join("settings.json"), "{}").unwrap();
        let sources = std::slice::from_ref(&t.src);
        let report = apply_with(
            &FAKE,
            &t.acc,
            &t.share("settings"),
            sources,
            When::Starting,
            true,
            false,
        );
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert!(fs::symlink_metadata(t.acc.join("settings.json")).is_err());
        let report = apply_with(
            &FAKE,
            &t.acc,
            &t.share("settings"),
            sources,
            When::Saved,
            true,
            false,
        );
        assert_eq!(report.problems, [format!("Settings: {NO_FILE_LINKS}")]);
        assert!(fs::symlink_metadata(t.acc.join("settings.json")).is_err());
        // Folders share without it.
        let report = apply_with(
            &FAKE,
            &t.acc,
            &t.share("skills"),
            sources,
            When::Starting,
            true,
            false,
        );
        assert!(report.problems.is_empty());
        assert!(is_link(&t.acc.join("skills")));
        t.logins_intact();
    }

    #[test]
    fn own_again_removes_only_the_link() {
        let t = Two::new();
        t.apply(&t.share("skills"), When::Saved);
        fs::write(t.src.join("skills/keep"), "x").unwrap();
        t.apply(&[], When::Saved);
        assert!(fs::symlink_metadata(t.acc.join("skills")).is_err());
        assert!(t.src.join("skills/keep").is_file());

        fs::create_dir_all(t.acc.join("skills.own/x")).unwrap();
        t.apply(&t.share("skills"), When::Saved);
        assert!(is_link(&t.acc.join("skills")));
        t.apply(&[], When::Starting);
        assert!(t.acc.join("skills/x").is_dir(), "skills.own came back");
        assert!(!t.acc.join("skills.own").exists());
        assert!(t.src.join("skills/keep").is_file());
    }

    #[test]
    fn a_chain_links_to_the_middle_account() {
        let t = Two::new();
        let (c, c3, c4) = (t.src.clone(), t.root.join("c3"), t.root.join("c4"));
        let from = |dir: &Path| {
            vec![Share {
                key: "skills",
                from: Some(dir.to_path_buf()),
            }]
        };
        let made = true;
        apply(
            &FAKE,
            &c3,
            &from(&c),
            &[c.clone(), c4.clone()],
            When::Saved,
            made,
        );
        apply(
            &FAKE,
            &c4,
            &from(&c3),
            &[c.clone(), c3.clone()],
            When::Saved,
            made,
        );
        assert_eq!(fs::read_link(c4.join("skills")).unwrap(), c3.join("skills"));
        fs::write(c.join("skills/f"), "seen").unwrap();
        assert_eq!(fs::read(c4.join("skills/f")).unwrap(), b"seen");
        t.logins_intact();
    }

    #[test]
    fn delete_leaves_what_links_point_at() {
        let t = Two::new();
        t.apply(&t.share("skills"), When::Saved);
        fs::write(t.src.join("skills/keep"), "x").unwrap();
        delete(&FAKE, &t.acc).unwrap();
        assert!(!t.acc.exists());
        assert!(t.src.join("skills/keep").is_file());
        t.logins_intact();
    }

    #[test]
    fn delete_removes_a_folder_that_is_a_link_and_nothing_in_it() {
        let t = Two::new();
        let dotfiles = t.root.join("dotfiles/skills");
        fs::create_dir_all(&dotfiles).unwrap();
        std::os::unix::fs::symlink(&dotfiles, t.src.join("skills")).unwrap();
        let linked = t.root.join("linked");
        std::os::unix::fs::symlink(&t.src, &linked).unwrap();
        delete(&FAKE, &linked).unwrap();
        assert!(fs::symlink_metadata(&linked).is_err(), "the link goes");
        assert_eq!(
            fs::read_link(t.src.join("skills")).unwrap(),
            dotfiles,
            "the person's own link in its target stays"
        );
        t.logins_intact();
    }

    /// What the person's folder holds, by path: nothing of it is moved, whatever is shared.
    fn untouched(folder: &Path) {
        assert_eq!(fs::read(folder.join("skills/a")).unwrap(), b"mine");
        assert_eq!(fs::read(folder.join("settings.json")).unwrap(), b"mine");
        assert!(!is_link(&folder.join("skills")) && !is_link(&folder.join("settings.json")));
        assert!(
            fs::read_dir(folder)
                .unwrap()
                .all(|e| { !e.unwrap().file_name().to_string_lossy().contains(".own") })
        );
    }

    #[test]
    fn a_chosen_folders_real_items_are_never_moved() {
        let t = Two::new();
        let chosen = t.root.join("Documents");
        fs::create_dir_all(chosen.join("skills")).unwrap();
        fs::write(chosen.join("skills/a"), "mine").unwrap();
        fs::write(chosen.join("settings.json"), "mine").unwrap();
        fs::write(t.src.join("settings.json"), "{}").unwrap();
        let shares = [t.share("skills"), t.share("settings"), t.share("agents")].concat();
        let sources = std::slice::from_ref(&t.src);
        for when in [When::Saved, When::Starting] {
            let report = apply(&FAKE, &chosen, &shares, sources, when, false);
            untouched(&chosen);
            assert_eq!(report.left, ["skills", "settings"], "said");
            assert!(report.kept.is_empty() && report.replaced.is_empty());
            assert!(
                is_link(&chosen.join("agents")),
                "what is missing is still linked"
            );
        }
        // Its own again: Templum's link goes, a `.own` there stays where it is.
        fs::create_dir_all(chosen.join("agents.own")).unwrap();
        apply(&FAKE, &chosen, &[], sources, When::Saved, false);
        assert!(fs::symlink_metadata(chosen.join("agents")).is_err());
        assert!(chosen.join("agents.own").is_dir());
        t.logins_intact();
    }

    /// Templum's folder for an account that is a link to the folder it shares from: nothing is
    /// kept aside there, nor linked to itself.
    #[test]
    fn a_folder_linked_to_its_source_isnt_linked_to_itself() {
        let t = Two::new();
        fs::create_dir_all(t.src.join("skills")).unwrap();
        fs::write(t.src.join("skills/a"), "mine").unwrap();
        fs::write(t.src.join("settings.json"), "mine").unwrap();
        let linked = t.root.join("linked");
        std::os::unix::fs::symlink(&t.src, &linked).unwrap();
        let shares = [t.share("skills"), t.share("settings")].concat();
        let report = apply(
            &FAKE,
            &linked,
            &shares,
            std::slice::from_ref(&t.src),
            When::Saved,
            true,
        );
        untouched(&t.src);
        let itself = |label| {
            format!(
                "{label}: it is {}'s own through a link, so it isn't linked to itself",
                t.src.display()
            )
        };
        assert_eq!(report.problems, [itself("Skills"), itself("Settings")]);
        assert!(report.kept.is_empty());

        // An item of the source that is a link to the account's own one.
        fs::remove_dir_all(t.src.join("skills")).unwrap();
        fs::create_dir_all(t.acc.join("skills")).unwrap();
        fs::write(t.acc.join("skills/a"), "mine").unwrap();
        std::os::unix::fs::symlink(t.acc.join("skills"), t.src.join("skills")).unwrap();
        let report = t.apply(&t.share("skills"), When::Saved);
        assert_eq!(fs::read(t.acc.join("skills/a")).unwrap(), b"mine");
        assert!(!is_link(&t.acc.join("skills")) && !t.acc.join("skills.own").exists());
        assert_eq!(report.problems, [itself("Skills")]);
    }

    /// A folder of the person's own, like `~/.claude`, with its items.
    fn theirs(folder: &Path) {
        fs::create_dir_all(folder.join("skills")).unwrap();
        fs::write(folder.join("skills/a"), "mine").unwrap();
        fs::write(folder.join("settings.json"), "mine").unwrap();
    }

    /// Templum's folder for an account that is a link to a folder of the person's: its real items
    /// are left as in a folder they chose, and nothing kept aside comes back there.
    #[test]
    fn a_folder_linked_elsewhere_moves_nothing_there() {
        let t = Two::new();
        let dotclaude = t.root.join("dotclaude");
        theirs(&dotclaude);
        fs::write(t.src.join("settings.json"), "{}").unwrap();
        let linked = t.root.join("linked");
        std::os::unix::fs::symlink(&dotclaude, &linked).unwrap();
        let shares = [t.share("skills"), t.share("settings")].concat();
        let sources = std::slice::from_ref(&t.src);
        for when in [When::Saved, When::Starting] {
            let report = apply(&FAKE, &linked, &shares, sources, when, true);
            untouched(&dotclaude);
            assert_eq!(report.left, ["skills", "settings"], "said");
            assert!(report.kept.is_empty() && report.replaced.is_empty());
        }
        fs::create_dir_all(dotclaude.join("agents.own")).unwrap();
        std::os::unix::fs::symlink(t.src.join("agents"), dotclaude.join("agents")).unwrap();
        apply(&FAKE, &linked, &[], sources, When::Saved, true);
        assert!(fs::symlink_metadata(dotclaude.join("agents")).is_err());
        assert!(dotclaude.join("agents.own").is_dir(), "never renamed back");
        t.logins_intact();
    }

    /// The same for a kind whose files are in a subfolder (Gemini CLI's `.gemini`) that is a link:
    /// and deleting the account removes only that link, never a link of the person's in it.
    #[test]
    fn a_state_folder_linked_elsewhere_moves_nothing_there() {
        const STATE: Kind = Kind {
            state: Some(".fake"),
            ..FAKE
        };
        let t = Two::new();
        let dotfake = t.root.join("dotfake");
        theirs(&dotfake);
        let dotfiles = t.root.join("dotfiles/agents");
        fs::create_dir_all(&dotfiles).unwrap();
        std::os::unix::fs::symlink(&dotfiles, dotfake.join("agents")).unwrap();
        std::os::unix::fs::symlink(&dotfake, t.acc.join(".fake")).unwrap();
        fs::write(t.src.join("settings.json"), "{}").unwrap();
        let shares = [t.share("skills"), t.share("settings")].concat();
        let report = apply(
            &STATE,
            &t.acc,
            &shares,
            std::slice::from_ref(&t.src),
            When::Saved,
            true,
        );
        untouched(&dotfake);
        assert_eq!(report.left, ["skills", "settings"], "said");
        delete(&STATE, &t.acc).unwrap();
        assert!(!t.acc.exists());
        untouched(&dotfake);
        assert_eq!(fs::read_link(dotfake.join("agents")).unwrap(), dotfiles);
        t.logins_intact();
    }

    /// The same kind when Templum's folder itself is the link and the subfolder in its target is
    /// real (a Gemini account's folder linked to the person's home).
    #[test]
    fn a_folder_linked_to_a_home_moves_nothing_in_its_state_folder() {
        const STATE: Kind = Kind {
            state: Some(".fake"),
            ..FAKE
        };
        let t = Two::new();
        let home = t.root.join("home");
        theirs(&home.join(".fake"));
        let linked = t.root.join("linked");
        std::os::unix::fs::symlink(&home, &linked).unwrap();
        fs::write(t.src.join("settings.json"), "{}").unwrap();
        let shares = [t.share("skills"), t.share("settings")].concat();
        let report = apply(
            &STATE,
            &linked,
            &shares,
            std::slice::from_ref(&t.src),
            When::Saved,
            true,
        );
        untouched(&home.join(".fake"));
        assert_eq!(report.left, ["skills", "settings"], "said");
        t.logins_intact();
    }

    #[test]
    fn a_link_templum_didnt_make_is_left() {
        let t = Two::new();
        let dotfiles = t.root.join("dotfiles/skills");
        fs::create_dir_all(&dotfiles).unwrap();
        std::os::unix::fs::symlink(&dotfiles, t.acc.join("skills")).unwrap();
        for shares in [t.share("skills"), Vec::new()] {
            let report = t.apply(&shares, When::Saved);
            assert_eq!(fs::read_link(t.acc.join("skills")).unwrap(), dotfiles);
            assert_eq!(report.theirs, [("skills", dotfiles.clone())]);
        }
    }

    #[test]
    fn read_finds_a_folder_set_up_by_hand() {
        let t = Two::new();
        let dotfiles = t.root.join("dotfiles/agents");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::create_dir_all(t.src.join("skills")).unwrap();
        std::os::unix::fs::symlink(t.src.join("skills"), t.acc.join("skills")).unwrap();
        std::os::unix::fs::symlink(&dotfiles, t.acc.join("agents")).unwrap();
        fs::create_dir_all(t.acc.join("commands")).unwrap();
        let found = read(&FAKE, &t.acc, std::slice::from_ref(&t.src));
        assert_eq!(
            found,
            [
                ("skills", Found::Linked(t.src.clone())),
                ("settings", Found::Own),
                ("agents", Found::Theirs(dotfiles)),
                ("commands", Found::Own),
            ]
        );
        t.logins_intact();
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use crate::accounts::{Keychain, item};
    use std::fs;

    const FAKE: Kind = Kind {
        agent: "fake",
        var: "FAKE_HOME",
        own: ".fake",
        state: None,
        items: &[
            item("skills", "Skills", &["skills/"]),
            item("settings", "Settings", &["settings.json"]),
        ],
        login: &[".credentials.json"],
        login_command: "fake login",
        login_hint: None,
        overrides: &[],
        keychain: Keychain::Never,
    };

    #[test]
    fn a_folder_is_a_junction_and_a_file_needs_the_privilege() {
        let dir = tempfile::tempdir().unwrap();
        let (src, acc) = (dir.path().join("src"), dir.path().join("acc"));
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("settings.json"), "{}").unwrap();
        let shares = [
            Share {
                key: "skills",
                from: Some(src.clone()),
            },
            Share {
                key: "settings",
                from: Some(src.clone()),
            },
        ];
        let report = apply(
            &FAKE,
            &acc,
            &shares,
            std::slice::from_ref(&src),
            When::Saved,
            true,
        );
        assert!(junction::exists(acc.join("skills")).unwrap());
        fs::write(src.join("skills/keep"), "x").unwrap();
        assert!(acc.join("skills/keep").is_file());
        if files_share() {
            assert!(
                fs::symlink_metadata(acc.join("settings.json"))
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        } else {
            assert_eq!(report.problems, [format!("Settings: {NO_FILE_LINKS}")]);
        }
        delete(&FAKE, &acc).unwrap();
        assert!(
            src.join("skills/keep").is_file(),
            "removing a junction keeps its target"
        );
    }
}
