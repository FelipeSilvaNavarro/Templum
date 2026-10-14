//! What CLI agents tell about their sessions and their plan limits, read from the files they
//! write. Never their credentials: see docs/specs/usage.md.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub mod claude;
pub mod codex;
mod watch;

pub use watch::{Event, REACHED, Watch};

/// An agent's session, as its files tell it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub model: Option<String>,
    /// Share of the context window used, 0 to 100.
    pub context_percent: Option<f32>,
    /// Tokens in the context, when the window's size is unknown.
    pub context_tokens: Option<u64>,
    /// Tokens the session used.
    pub tokens: Option<u64>,
    /// The agent's estimate, at API prices.
    pub cost_usd: Option<f64>,
}

/// A plan's rate-limit window.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    /// Its length: 300 is 5 hours, 10080 a week.
    pub minutes: u32,
    /// 0 to 100.
    pub used_percent: f32,
    /// When it resets, in seconds since the Unix epoch.
    pub resets_at: u64,
}

/// An agent's plan limits: its account's, whichever session read them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limits {
    pub windows: Vec<Window>,
    /// When the agent wrote them, in seconds since the Unix epoch.
    pub at: u64,
}

/// What the user wants to hear about a limit's reset (`[usage] reset`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reset {
    Off,
    /// The window was reached (see [`REACHED`]).
    #[default]
    Reached,
    /// Half of the window or more was used.
    Half,
    /// Any of it was used.
    Always,
}

/// An agent's process in a terminal.
#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    /// Its id among Templum's agents: `claude`, `codex`.
    pub agent: String,
    pub pid: u32,
    /// When it started, in seconds since the Unix epoch.
    pub started: u64,
    pub cwd: Option<PathBuf>,
    /// Its terminal's `TEMPLUM_TERMINAL`.
    pub terminal: String,
}

/// Where the agents keep their files, and the relay its own.
#[derive(Clone, Debug)]
pub struct Dirs {
    /// The relay's files: `<data>/usage/claude`.
    pub relay: PathBuf,
    /// `$CLAUDE_CONFIG_DIR` or `~/.claude`.
    pub claude: Option<PathBuf>,
    /// `$CODEX_HOME` or `~/.codex`.
    pub codex: Option<PathBuf>,
}

impl Dirs {
    /// For Templum's data folder `data`, from the environment.
    pub fn of(data: &Path) -> Self {
        let home = std::env::home_dir();
        let env = |name| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Self {
            relay: relay_dir(data),
            claude: env("CLAUDE_CONFIG_DIR").or_else(|| home.as_ref().map(|h| h.join(".claude"))),
            codex: env("CODEX_HOME").or_else(|| home.as_ref().map(|h| h.join(".codex"))),
        }
    }
}

/// Where the relay writes what Claude Code tells it, under Templum's data folder.
pub fn relay_dir(data: &Path) -> PathBuf {
    data.join("usage").join("claude")
}

/// What the agents `procs` tell about their sessions, in order, and each agent's plan limits,
/// at `now` (seconds since the epoch). Reads files: off the UI thread.
pub fn read(
    procs: &[Proc],
    dirs: &Dirs,
    now: u64,
) -> (Vec<Option<Session>>, Vec<(String, Limits)>) {
    let mut sessions: Vec<Option<Session>> = procs
        .iter()
        .map(|p| match p.agent.as_str() {
            "claude" => claude::session(p, &dirs.relay, dirs.claude.as_deref()),
            _ => None,
        })
        .collect();
    let mut limits = Vec::new();
    if let Some(l) = claude::limits(&dirs.relay) {
        limits.push(("claude".to_string(), l));
    }
    if let Some(dir) = &dirs.codex {
        let (at, codex): (Vec<usize>, Vec<&Proc>) = procs
            .iter()
            .enumerate()
            .filter(|(_, p)| p.agent == "codex")
            .unzip();
        if !codex.is_empty() {
            for (i, s) in at.into_iter().zip(codex::sessions(&codex, dir, now)) {
                sessions[i] = s;
            }
        }
        if let Some(l) = codex::limits(dir, now) {
            limits.push(("codex".to_string(), l));
        }
    }
    (sessions, limits)
}

/// The end of a file, at most `max` bytes, from its first whole line: where an agent's latest
/// lines are.
pub(crate) fn tail(path: &Path, max: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    if start > 0 {
        let first = bytes.iter().position(|&b| b == b'\n')?;
        bytes.drain(..=first);
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// How much of a session file is read from its end.
pub(crate) const TAIL: u64 = 1 << 20;

/// A file's modification time, in seconds since the Unix epoch.
pub(crate) fn modified(path: &Path) -> Option<u64> {
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_is_read_by_its_own_reader() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let dirs = Dirs {
            relay: relay_dir(data),
            claude: None,
            codex: Some(data.join("codex")),
        };
        std::fs::create_dir_all(&dirs.relay).unwrap();
        std::fs::write(
            dirs.relay.join("1-0.json"),
            r#"{"cost": {"total_cost_usd": 1.5},
                "rate_limits": {"five_hour": {"used_percentage": 10, "resets_at": 9999999999}}}"#,
        )
        .unwrap();
        let proc = |agent: &str| Proc {
            agent: agent.into(),
            pid: u32::MAX,
            started: 0,
            cwd: Some("/srv".into()),
            terminal: "1-0".into(),
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (sessions, limits) = read(&[proc("claude"), proc("aider")], &dirs, now);
        assert_eq!(sessions[0].as_ref().unwrap().cost_usd, Some(1.5));
        assert_eq!(sessions[1], None);
        assert_eq!(limits.len(), 1);
        assert_eq!(limits[0].0, "claude");
    }

    #[test]
    fn a_tail_starts_at_a_whole_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.jsonl");
        std::fs::write(&path, "first line\nsecond\nthird\n").unwrap();
        assert_eq!(tail(&path, 12).as_deref(), Some("third\n"));
        assert_eq!(
            tail(&path, 1000).as_deref(),
            Some("first line\nsecond\nthird\n")
        );
        assert_eq!(tail(&dir.path().join("none"), 10), None);
    }
}
