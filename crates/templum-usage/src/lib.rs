//! What CLI agents tell about their sessions and their plan limits, read from the files they
//! write. Never their credentials: see docs/specs/usage.md.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub mod claude;
pub mod codex;
pub mod copilot;
pub mod gemini;
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
    /// The model it is for ("Fable"); None: every model.
    pub scope: Option<String>,
}

/// How long the agents Templum opens wait for one of its tools, ms: seven days, so a question
/// can wait for its person. Under JavaScript's largest timer (2³¹ − 1 ms, 24.8 days; a longer
/// one fires at once), which Claude Code and Gemini CLI run on.
pub const MCP_TIMEOUT_MS: u64 = 604_800_000;

/// `<data>/bin/templum` (`templum.exe` on Windows), the link Templum points at itself at each
/// start: the program the files it writes for agents run, the same whatever its version.
pub fn bin(data: &Path, windows: bool) -> PathBuf {
    data.join("bin")
        .join(if windows { "templum.exe" } else { "templum" })
}

/// `bin hook <words>`, as an agent runs a hook: through bash on Unix, PowerShell on Windows, with
/// `bin` in single quotes. None when `bin` holds a `'` or a line break.
pub fn hook_command(bin: &str, windows: bool, words: &str) -> Option<String> {
    if bin.is_empty() || bin.contains(['\'', '\n', '\r']) {
        return None;
    }
    Some(if windows {
        format!("& '{bin}' hook {words}")
    } else {
        format!("'{bin}' hook {words}")
    })
}

/// The agents whose plan limits Templum reads, in the order it shows them.
pub const LIMITED: [&str; 2] = ["claude", "codex"];

/// Puts `other` as `agent`'s limits in `limits` unless the ones there are newer.
pub fn merge(limits: &mut Vec<(String, Limits)>, agent: &str, other: &Limits) {
    match limits.iter_mut().find(|(a, _)| a == agent) {
        Some((_, l)) if l.at >= other.at => {}
        Some((_, l)) => *l = other.clone(),
        None => {
            // In LIMITED's order.
            let rank = |a: &str| {
                LIMITED
                    .iter()
                    .position(|l| *l == a)
                    .unwrap_or(LIMITED.len())
            };
            let at = limits
                .iter()
                .position(|(a, _)| rank(a) > rank(agent))
                .unwrap_or(limits.len());
            limits.insert(at, (agent.to_string(), other.clone()));
        }
    }
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
    /// `~/.qwen`.
    pub qwen: Option<PathBuf>,
    /// `~/.gemini`.
    pub gemini: Option<PathBuf>,
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
            qwen: home.as_ref().map(|h| h.join(".qwen")),
            gemini: home.as_ref().map(|h| h.join(".gemini")),
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

/// The ids of the sessions the agents `procs` have open, in order, to resume each one's own:
/// Claude Code's, Qwen Code's and Codex's. Reads files: off the UI thread.
pub fn session_ids(procs: &[Proc], dirs: &Dirs, now: u64) -> Vec<Option<String>> {
    let mut ids: Vec<Option<String>> = procs
        .iter()
        .map(|p| {
            let dir = match p.agent.as_str() {
                "claude" => dirs.claude.as_deref(),
                // Qwen Code keeps the same file for each of its processes.
                "qwen" => dirs.qwen.as_deref(),
                _ => None,
            };
            claude::session_id(p.pid, dir?)
        })
        .collect();
    let (at, codex): (Vec<usize>, Vec<&Proc>) = procs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.agent == "codex")
        .unzip();
    if let (Some(dir), false) = (&dirs.codex, codex.is_empty()) {
        for (i, id) in at.into_iter().zip(codex::session_ids(&codex, dir, now)) {
            ids[i] = id;
        }
    }
    ids
}

/// Whether `agent` still has its session `id` to resume. Gemini CLI deletes a session that has
/// no message yet when it exits, and can't resume one it doesn't find
/// (`tmp/<project>/chats/session-<time>-<the id's first 8>.jsonl`); the others are taken to.
pub fn session_kept(agent: &str, id: &str, dirs: &Dirs) -> bool {
    let (Some(dir), "gemini") = (&dirs.gemini, agent) else {
        return true;
    };
    let name = format!("-{}.jsonl", id.get(..8).unwrap_or(id));
    let Ok(projects) = std::fs::read_dir(dir.join("tmp")) else {
        return false;
    };
    projects.flatten().any(|project| {
        std::fs::read_dir(project.path().join("chats"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|f| f.file_name().to_string_lossy().ends_with(&name))
    })
}

/// A session id names a file and is typed into a shell: letters, digits and `-` only.
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
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
    fn a_hook_runs_templum_in_single_quotes_through_bash_or_powershell() {
        assert_eq!(
            hook_command("/home/u/.local/share/templum/bin/templum", false, "gemini").as_deref(),
            Some("'/home/u/.local/share/templum/bin/templum' hook gemini")
        );
        assert_eq!(
            hook_command(
                r"C:\Users\Jane Doe\AppData\Roaming\templum\bin\templum.exe",
                true,
                "gemini"
            )
            .as_deref(),
            Some(r"& 'C:\Users\Jane Doe\AppData\Roaming\templum\bin\templum.exe' hook gemini")
        );
        // Single quotes keep $, ` and " as they are; only a ' or a line break can't go inside.
        assert_eq!(
            hook_command("/home/$x/templum", false, "gemini").as_deref(),
            Some("'/home/$x/templum' hook gemini")
        );
        assert_eq!(hook_command("/home/o'neil/templum", false, "gemini"), None);
        assert_eq!(hook_command("/home/o'neil/templum", true, "gemini"), None);
        assert_eq!(hook_command("/home/a\nb/templum", false, "gemini"), None);
        assert_eq!(hook_command("", false, "gemini"), None);
        assert_eq!(
            bin(Path::new("/d"), false),
            Path::new("/d").join("bin").join("templum")
        );
        assert_eq!(
            bin(Path::new("/d"), true),
            Path::new("/d").join("bin").join("templum.exe")
        );
    }

    #[test]
    fn the_newer_limits_win_and_keep_their_place() {
        let at = |at: u64| Limits {
            windows: Vec::new(),
            at,
        };
        let mut limits = vec![("codex".to_string(), at(5))];
        merge(&mut limits, "claude", &at(10));
        assert_eq!(limits[0], ("claude".to_string(), at(10)));
        merge(&mut limits, "claude", &at(7));
        assert_eq!(limits[0].1.at, 10, "older");
        merge(&mut limits, "claude", &at(12));
        assert_eq!(limits[0].1.at, 12);
        assert_eq!(limits.len(), 2);
    }

    #[test]
    fn each_agent_is_read_by_its_own_reader() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let dirs = Dirs {
            relay: relay_dir(data),
            claude: None,
            codex: Some(data.join("codex")),
            qwen: Some(data.join("qwen")),
            gemini: None,
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
    fn claude_and_qwen_tell_a_process_its_session_in_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs {
            relay: dir.path().join("relay"),
            claude: Some(dir.path().join("claude")),
            codex: None,
            qwen: Some(dir.path().join("qwen")),
            gemini: None,
        };
        for (agent, id) in [("claude", "c-1"), ("qwen", "q-2")] {
            let sessions = dir.path().join(agent).join("sessions");
            std::fs::create_dir_all(&sessions).unwrap();
            let json = format!(r#"{{"pid": 7, "sessionId": "{id}", "cwd": "/srv"}}"#);
            std::fs::write(sessions.join("7.json"), json).unwrap();
        }
        let proc = |agent: &str| Proc {
            agent: agent.into(),
            pid: 7,
            started: 0,
            cwd: Some("/srv".into()),
            terminal: String::new(),
        };
        let ids = session_ids(&[proc("claude"), proc("qwen"), proc("gemini")], &dirs, 0);
        assert_eq!(ids, [Some("c-1".into()), Some("q-2".into()), None]);
    }

    #[test]
    fn a_gemini_session_is_kept_only_while_its_file_is() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs {
            relay: dir.path().join("relay"),
            claude: None,
            codex: None,
            qwen: None,
            gemini: Some(dir.path().to_path_buf()),
        };
        let id = "019a1f2e-7b3c-7d4e-8f5a-6b7c8d9e0f1a";
        assert!(!session_kept("gemini", id, &dirs), "no folder yet");
        let chats = dir.path().join("tmp/my-app/chats");
        std::fs::create_dir_all(&chats).unwrap();
        assert!(!session_kept("gemini", id, &dirs));
        std::fs::write(chats.join("session-2026-10-14T10-00-019a1f2e.jsonl"), "{}").unwrap();
        assert!(session_kept("gemini", id, &dirs));
        assert!(!session_kept(
            "gemini",
            "11111111-7b3c-7d4e-8f5a-6b7c8d9e0f1a",
            &dirs
        ));
        assert!(session_kept("grok", id, &dirs), "the others keep theirs");
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
