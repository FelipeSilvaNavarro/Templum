//! Claude Code: its status line, through Templum's relay, and its transcripts.
//!
//! Claude Code hands its status line command a JSON document on stdin
//! (https://code.claude.com/docs/en/statusline), with the session's cost and context and, for
//! Pro/Max subscribers, the plan's `rate_limits`. The Claude Code Templum opens gets
//! `--settings <data>/claude-statusline.json`, which makes Templum itself (`templum
//! --claude-statusline`) that command: the relay writes the document where Templum reads it,
//! then runs the user's own status line.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::{Limits, Proc, Session, TAIL, Window, modified, tail};

/// The argument that makes Templum the relay.
pub const RELAY_FLAG: &str = "--claude-statusline";

/// The settings file the Claude Code Templum opens is given, in Templum's data folder.
const SETTINGS: &str = "claude-statusline.json";

/// A relay file older than this when Templum starts is deleted (a week's window, and a day).
pub const KEEP_SECS: u64 = 8 * 24 * 3600;

/// What Templum reads of the status line's input.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Status {
    model: Option<Model>,
    cost: Option<Cost>,
    context_window: Option<ContextWindow>,
    rate_limits: Option<RateLimits>,
    workspace: Option<Workspace>,
    cwd: Option<PathBuf>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Model {
    display_name: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Cost {
    total_cost_usd: Option<f64>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ContextWindow {
    total_input_tokens: Option<u64>,
    total_output_tokens: Option<u64>,
    used_percentage: Option<f32>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct RateLimits {
    five_hour: Option<RateWindow>,
    seven_day: Option<RateWindow>,
}

#[derive(Deserialize)]
struct RateWindow {
    used_percentage: f32,
    resets_at: u64,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Workspace {
    project_dir: Option<PathBuf>,
}

/// The relay, given Claude Code's status line `input` in the terminal `terminal` (its
/// `TEMPLUM_TERMINAL`): writes it to `relay_dir`, then runs the user's own status line, found
/// in the project's settings or `claude_dir`'s, and returns what it printed. Never fails:
/// Claude Code would show the error in its status line.
pub fn relay(
    input: &[u8],
    relay_dir: &Path,
    terminal: Option<&str>,
    claude_dir: Option<&Path>,
) -> Vec<u8> {
    if let Some(terminal) = terminal.filter(|t| valid_id(t)) {
        let _ = write_atomic(&relay_dir.join(format!("{terminal}.json")), input);
    }
    let status: Status = serde_json::from_slice(input).unwrap_or_default();
    let project = status
        .workspace
        .and_then(|w| w.project_dir)
        .or(status.cwd)
        .or_else(|| std::env::current_dir().ok());
    match users_status_line(project.as_deref(), claude_dir) {
        Some(command) => run(&command, input),
        None => Vec::new(),
    }
}

/// A terminal id as Templum makes them: `<pid>-<n>`. Anything else could name a file elsewhere.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit() || c == '-')
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().expect("a file in a folder");
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().expect("a file name").to_string_lossy();
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// The status line command the user set, as Claude Code would pick it without `--settings`:
/// the project's local settings, its shared ones, then the user's. None when there is none, or
/// when it is the relay itself.
fn users_status_line(project: Option<&Path>, claude_dir: Option<&Path>) -> Option<String> {
    let project = project.map(|p| p.join(".claude"));
    let files = [
        project.as_ref().map(|p| p.join("settings.local.json")),
        project.as_ref().map(|p| p.join("settings.json")),
        claude_dir.map(|c| c.join("settings.json")),
    ];
    let line = files.into_iter().flatten().find_map(|file| {
        let text = std::fs::read(file).ok()?;
        let settings: serde_json::Value = serde_json::from_slice(&text).ok()?;
        settings.get("statusLine").cloned()
    })?;
    let command = line.get("command")?.as_str()?;
    (!command.trim().is_empty() && !command.contains(RELAY_FLAG)).then(|| command.to_string())
}

/// Runs `command` in the shell with `input` on its stdin; its output, or nothing.
fn run(command: &str, input: &[u8]) -> Vec<u8> {
    #[cfg(unix)]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    // ponytail: unverified that Claude Code runs status lines through cmd on Windows (it may
    // use Git Bash); check by hand, then pick the same shell.
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    };
    let child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let Ok(mut child) = child else {
        return Vec::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A status line that doesn't read its input still runs.
        let _ = stdin.write_all(input);
    }
    child
        .wait_with_output()
        .map(|o| o.stdout)
        .unwrap_or_default()
}

/// Writes the settings file the Claude Code Templum opens is given, naming `exe` (Templum's
/// program) as its status line, and returns its path as typed after `--settings` in a shell.
/// None when either path can't be typed safely in every shell.
pub fn settings_arg(data: &Path, exe: &Path, windows: bool) -> Option<String> {
    let path = data.join(SETTINGS);
    let typed = typed_path(&path.to_string_lossy(), windows)?;
    let exe = typed_path(&exe.to_string_lossy(), windows)?;
    let settings = serde_json::json!({
        "statusLine": {
            "type": "command",
            "command": format!("{exe} {RELAY_FLAG}"),
            "padding": 0,
        }
    });
    let text = serde_json::to_vec_pretty(&settings).expect("JSON");
    if std::fs::read(&path).ok().as_deref() != Some(&text[..]) {
        write_atomic(&path, &text).ok()?;
    }
    Some(typed)
}

/// `path` as one word in bash, zsh, fish, PowerShell and cmd: as is when it is plain, in double
/// quotes otherwise; Windows' `\` as `/`, which they all take. None when it holds a character
/// one of them would expand inside double quotes.
pub fn typed_path(path: &str, windows: bool) -> Option<String> {
    let path = if windows {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    if path.is_empty() || path.contains(['"', '$', '%', '!', '`', '\\', '\n']) {
        return None;
    }
    let plain = path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-:+@=,".contains(c));
    Some(if plain { path } else { format!("\"{path}\"") })
}

/// The session of the Claude Code `proc`: from the relay's file when it wrote one for this
/// process, else from Claude Code's transcript.
pub fn session(proc: &Proc, relay_dir: &Path, claude_dir: Option<&Path>) -> Option<Session> {
    relayed(proc, relay_dir).or_else(|| transcript(proc, claude_dir?))
}

fn relayed(proc: &Proc, relay_dir: &Path) -> Option<Session> {
    let path = relay_dir.join(format!("{}.json", proc.terminal));
    // Written for an earlier agent in the same terminal: not this one's. Start times are about
    // a second off on Linux.
    if modified(&path)? + 2 < proc.started {
        return None;
    }
    let status: Status = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
    let context = status.context_window.unwrap_or_default();
    let tokens = match (context.total_input_tokens, context.total_output_tokens) {
        (None, None) => None,
        (i, o) => Some(i.unwrap_or(0) + o.unwrap_or(0)),
    };
    Some(Session {
        model: status.model.and_then(|m| m.display_name),
        context_percent: context.used_percentage,
        context_tokens: None,
        tokens,
        cost_usd: status.cost.and_then(|c| c.total_cost_usd),
    })
}

/// Claude Code's own record of a process (`sessions/<pid>.json`; undocumented).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Running {
    session_id: String,
    cwd: PathBuf,
}

/// The context in tokens of the session's last answer, from its transcript.
fn transcript(proc: &Proc, claude_dir: &Path) -> Option<Session> {
    let running = claude_dir
        .join("sessions")
        .join(format!("{}.json", proc.pid));
    let running: Running = serde_json::from_slice(&std::fs::read(running).ok()?).ok()?;
    if !valid_session_id(&running.session_id) {
        return None;
    }
    let file = claude_dir
        .join("projects")
        .join(project_folder(&running.cwd))
        .join(format!("{}.jsonl", running.session_id));
    let text = tail(&file, TAIL)?;
    text.lines().rev().find_map(|line| {
        let entry: serde_json::Value = serde_json::from_str(line).ok()?;
        if entry.get("type")?.as_str()? != "assistant"
            || entry.get("isSidechain").and_then(|s| s.as_bool()) == Some(true)
        {
            return None;
        }
        let message = entry.get("message")?;
        let usage = message.get("usage")?;
        let count = |key: &str| usage.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
        Some(Session {
            model: message
                .get("model")
                .and_then(|m| m.as_str())
                .map(str::to_string),
            context_tokens: Some(
                count("input_tokens")
                    + count("cache_creation_input_tokens")
                    + count("cache_read_input_tokens"),
            ),
            ..Session::default()
        })
    })
}

/// A session id names a file: letters, digits and `-` only.
fn valid_session_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The folder Claude Code keeps a project's transcripts in: its path with every character
/// that isn't a letter or a digit as `-`.
fn project_folder(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The plan limits in the newest relay file that has them: they are the account's.
pub fn limits(relay_dir: &Path) -> Option<Limits> {
    let mut files: Vec<(u64, PathBuf)> = std::fs::read_dir(relay_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| Some((modified(&p)?, p)))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.into_iter().find_map(|(at, path)| {
        let status: Status = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        let rate = status.rate_limits?;
        let windows: Vec<Window> = [(300, rate.five_hour), (10080, rate.seven_day)]
            .into_iter()
            .filter_map(|(minutes, w)| {
                let w = w?;
                Some(Window {
                    minutes,
                    used_percent: w.used_percentage,
                    resets_at: w.resets_at,
                })
            })
            .collect();
        (!windows.is_empty()).then_some(Limits { windows, at })
    })
}

/// Deletes the relay files older than [`KEEP_SECS`] at `now`.
pub fn prune(relay_dir: &Path, now: u64) {
    let Ok(entries) = std::fs::read_dir(relay_dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if modified(&path).is_some_and(|m| m + KEEP_SECS < now) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn set_modified(path: &Path, secs: u64) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    }

    fn proc(terminal: &str, started: u64) -> Proc {
        Proc {
            agent: "claude".into(),
            pid: 4242,
            started,
            cwd: None,
            terminal: terminal.into(),
        }
    }

    const STATUS: &str = r#"{
        "session_id": "s1",
        "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
        "cost": {"total_cost_usd": 3.1},
        "context_window": {"total_input_tokens": 15500, "total_output_tokens": 1200,
                           "context_window_size": 200000, "used_percentage": 42},
        "rate_limits": {"five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
                        "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600}}
    }"#;

    #[cfg(unix)]
    #[test]
    fn the_relay_keeps_its_input_and_prints_the_users_status_line() {
        let dir = tempfile::tempdir().unwrap();
        let relay_dir = dir.path().join("relay");
        let claude = dir.path().join("claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("settings.json"),
            r#"{"statusLine": {"type": "command", "command": "printf mine; wc -c >/dev/null"}}"#,
        )
        .unwrap();
        let out = relay(STATUS.as_bytes(), &relay_dir, Some("12-3"), Some(&claude));
        assert_eq!(out, b"mine");
        assert_eq!(
            std::fs::read_to_string(relay_dir.join("12-3.json")).unwrap(),
            STATUS
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_projects_local_status_line_wins() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let claude = dir.path().join("claude");
        std::fs::create_dir_all(project.join(".claude")).unwrap();
        std::fs::create_dir_all(&claude).unwrap();
        let line = |text: &str| format!(r#"{{"statusLine": {{"command": "printf {text}"}}}}"#);
        std::fs::write(claude.join("settings.json"), line("user")).unwrap();
        std::fs::write(project.join(".claude/settings.json"), line("shared")).unwrap();
        let input = format!(
            r#"{{"workspace": {{"project_dir": "{}"}}}}"#,
            project.display()
        );
        let out = relay(input.as_bytes(), dir.path(), None, Some(&claude));
        assert_eq!(out, b"shared");
        std::fs::write(project.join(".claude/settings.local.json"), line("local")).unwrap();
        let out = relay(input.as_bytes(), dir.path(), None, Some(&claude));
        assert_eq!(out, b"local");
    }

    #[test]
    fn the_relay_never_runs_itself_nor_writes_a_strange_name() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join("claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("settings.json"),
            r#"{"statusLine": {"command": "/opt/templum --claude-statusline"}}"#,
        )
        .unwrap();
        let relay_dir = dir.path().join("relay");
        assert!(relay(b"{}", &relay_dir, Some("../x"), Some(&claude)).is_empty());
        assert!(relay(b"{}", &relay_dir, Some(""), Some(&claude)).is_empty());
        assert!(!relay_dir.exists());
    }

    #[test]
    fn a_relay_file_tells_the_session_and_the_limits() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1-0.json"), STATUS).unwrap();
        let s = session(&proc("1-0", now() - 60), dir.path(), None).unwrap();
        assert_eq!(
            s,
            Session {
                model: Some("Opus".into()),
                context_percent: Some(42.0),
                context_tokens: None,
                tokens: Some(16700),
                cost_usd: Some(3.1),
            }
        );
        let l = limits(dir.path()).unwrap();
        assert_eq!(l.windows.len(), 2);
        assert_eq!(
            l.windows[0],
            Window {
                minutes: 300,
                used_percent: 23.5,
                resets_at: 1738425600,
            }
        );
        assert_eq!(l.windows[1].minutes, 10080);
    }

    /// A file written for an agent the terminal ran before isn't this one's.
    #[test]
    fn a_relay_file_older_than_the_agent_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("1-0.json");
        std::fs::write(&path, STATUS).unwrap();
        set_modified(&path, now() - 100);
        assert_eq!(session(&proc("1-0", now() - 10), dir.path(), None), None);
        assert!(session(&proc("1-0", now() - 101), dir.path(), None).is_some());
    }

    #[test]
    fn the_newest_relay_file_with_limits_wins() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("1-0.json");
        std::fs::write(&old, STATUS).unwrap();
        set_modified(&old, now() - 100);
        let newer = STATUS.replace("23.5", "60");
        std::fs::write(dir.path().join("1-1.json"), newer).unwrap();
        // The newest has none: an API key user, or before the first answer.
        let newest = dir.path().join("1-2.json");
        std::fs::write(&newest, r#"{"session_id": "s3"}"#).unwrap();
        set_modified(&newest, now() + 5);
        let l = limits(dir.path()).unwrap();
        assert_eq!(l.windows[0].used_percent, 60.0);
    }

    #[test]
    fn without_the_relay_the_transcript_tells_the_context() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path();
        std::fs::create_dir_all(claude.join("sessions")).unwrap();
        std::fs::write(
            claude.join("sessions/4242.json"),
            r#"{"pid": 4242, "sessionId": "abc-1", "cwd": "/home/u/my app", "kind": "interactive"}"#,
        )
        .unwrap();
        let project = claude.join("projects/-home-u-my-app");
        std::fs::create_dir_all(&project).unwrap();
        let line = |sidechain: bool, input: u64| {
            format!(
                r#"{{"type":"assistant","isSidechain":{sidechain},"message":{{"model":"claude-opus-5-5","usage":{{"input_tokens":{input},"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":5}}}}}}"#
            )
        };
        let text = [
            line(false, 1),
            line(false, 2),
            line(true, 9),
            r#"{"type":"user"}"#.to_string(),
        ]
        .join("\n");
        std::fs::write(project.join("abc-1.jsonl"), text).unwrap();
        let s = session(&proc("1-0", 0), &dir.path().join("relay"), Some(claude)).unwrap();
        assert_eq!(s.context_tokens, Some(1102));
        assert_eq!(s.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(s.context_percent, None);
    }

    #[test]
    fn a_path_is_typed_as_one_word_in_every_shell() {
        assert_eq!(
            typed_path("/home/u/.local/share/templum/claude-statusline.json", false).as_deref(),
            Some("/home/u/.local/share/templum/claude-statusline.json")
        );
        assert_eq!(
            typed_path("/Users/u/Library/Application Support/Templum/c.json", false).as_deref(),
            Some("\"/Users/u/Library/Application Support/Templum/c.json\"")
        );
        assert_eq!(
            typed_path(r"C:\Users\Jane Doe\AppData\Roaming\templum\c.json", true).as_deref(),
            Some("\"C:/Users/Jane Doe/AppData/Roaming/templum/c.json\"")
        );
        assert_eq!(typed_path("/home/$USER/c.json", false), None);
        assert_eq!(typed_path(r"C:\100%\c.json", true), None);
    }

    #[test]
    fn the_settings_file_names_templum_as_the_status_line() {
        let dir = tempfile::tempdir().unwrap();
        let typed = settings_arg(dir.path(), Path::new("/opt/Templum/templum"), false).unwrap();
        assert_eq!(typed, dir.path().join(SETTINGS).display().to_string());
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join(SETTINGS)).unwrap()).unwrap();
        assert_eq!(
            written["statusLine"]["command"],
            "/opt/Templum/templum --claude-statusline"
        );
        assert_eq!(
            settings_arg(dir.path(), Path::new("/opt/$x/templum"), false),
            None
        );
    }

    #[test]
    fn old_relay_files_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("1-0.json");
        let recent = dir.path().join("1-1.json");
        std::fs::write(&old, "{}").unwrap();
        std::fs::write(&recent, "{}").unwrap();
        set_modified(&old, now() - KEEP_SECS - 10);
        set_modified(&recent, now() - KEEP_SECS + 3600);
        prune(dir.path(), now());
        assert!(!old.exists());
        assert!(recent.exists());
    }
}
