//! Codex: its rollout files, `sessions/YYYY/MM/DD/rollout-*.jsonl` under `$CODEX_HOME` or
//! `~/.codex`. The first line is the session's `session_meta` (its folder and start); after
//! each turn a `token_count` event tells the tokens used and the plan's `rate_limits`.

use std::io::BufRead as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike as _};
use serde::Deserialize;

use crate::{Limits, Proc, Session, TAIL, Window, modified, tail};

const DAY: u64 = 24 * 3600;

/// The sessions of the Codex processes `procs`, in order: each process gets its own file.
pub fn sessions(procs: &[&Proc], codex_dir: &Path, now: u64) -> Vec<Option<Session>> {
    let earliest = procs.iter().map(|p| p.started).min().unwrap_or(now);
    let files = rollouts(codex_dir, earliest.saturating_sub(DAY), now + DAY);
    let metas: Vec<(PathBuf, Meta, u64)> = files
        .into_iter()
        .filter_map(|(path, mtime)| Some((meta(&path)?, mtime, path)))
        .map(|(m, mtime, path)| (path, m, mtime))
        .collect();
    let mut taken = vec![false; metas.len()];
    let mut order: Vec<usize> = (0..procs.len()).collect();
    // The newest process first: an older one in the same folder can't take its file.
    order.sort_by(|&a, &b| procs[b].started.cmp(&procs[a].started));
    let mut found = vec![None; procs.len()];
    // The file a process has open is its own, even one from long ago it resumed: those first.
    #[cfg(target_os = "linux")]
    let open: Vec<Option<PathBuf>> = procs.iter().map(|p| open_rollout(p.pid)).collect();
    #[cfg(not(target_os = "linux"))]
    let open: Vec<Option<PathBuf>> = vec![None; procs.len()];
    for (i, path) in open.iter().enumerate() {
        if let Some(path) = path {
            if let Some(j) = metas.iter().position(|(p, ..)| p == path) {
                taken[j] = true;
            }
            found[i] = usage(path);
        }
    }
    for i in order.into_iter().filter(|&i| open[i].is_none()) {
        let p = procs[i];
        let Some(cwd) = &p.cwd else { continue };
        let ours = |j: &usize| !taken[*j] && metas[*j].1.cwd == *cwd;
        // Started by this process; else one it resumed and writes to.
        let pick = (0..metas.len())
            .filter(|j| ours(j) && metas[*j].1.start + 5 >= p.started)
            .max_by_key(|&j| metas[j].1.start)
            .or_else(|| {
                (0..metas.len())
                    .filter(|j| ours(j) && metas[*j].2 + 2 >= p.started)
                    .max_by_key(|&j| metas[j].2)
            });
        if let Some(j) = pick {
            taken[j] = true;
            found[i] = usage(&metas[j].0);
        }
    }
    found
}

/// The plan limits in the newest rollout of the last week that has them: they are the
/// account's. A session just started, or one on an API key, has none yet.
pub fn limits(codex_dir: &Path, now: u64) -> Option<Limits> {
    let mut files = rollouts(codex_dir, now.saturating_sub(7 * DAY), now + DAY);
    files.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));
    // ponytail: the 10 newest only, so a week of keyless sessions isn't read every 5 s
    files
        .into_iter()
        .take(10)
        .find_map(|(path, at)| limits_in(&path, at))
}

fn limits_in(path: &Path, at: u64) -> Option<Limits> {
    let text = tail(path, TAIL)?;
    text.lines().rev().find_map(|line| {
        let event = token_count(line)?;
        let rate = event.rate_limits?;
        if rate.limit_id.as_deref().is_some_and(|id| id != "codex") {
            return None;
        }
        let windows: Vec<Window> = [rate.primary, rate.secondary]
            .into_iter()
            .flatten()
            .filter_map(|w| {
                Some(Window {
                    minutes: w.window_minutes?,
                    used_percent: w.used_percent?,
                    resets_at: w.resets_at?,
                })
            })
            .collect();
        (!windows.is_empty()).then_some(Limits { windows, at })
    })
}

/// The rollout files in the dated folders from `from` to `to` (seconds since the epoch), with
/// their modification times. Codex names the folders by local date: the days around are read.
fn rollouts(codex_dir: &Path, from: u64, to: u64) -> Vec<(PathBuf, u64)> {
    let sessions = codex_dir.join("sessions");
    let mut files = Vec::new();
    let mut day = from - from % DAY;
    while day <= to {
        if let Some(date) = DateTime::from_timestamp(day as i64, 0) {
            let folder = sessions
                .join(format!("{:04}", date.year()))
                .join(format!("{:02}", date.month()))
                .join(format!("{:02}", date.day()));
            if let Ok(entries) = std::fs::read_dir(folder) {
                files.extend(
                    entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
                        .filter_map(|p| Some((modified(&p)?, p)))
                        .map(|(m, p)| (p, m)),
                );
            }
        }
        day += DAY;
    }
    files
}

/// The rollout a Codex process has open, or one of its children has (npm's `codex` is a Node
/// script that starts the native one): exact, even for a session it resumed.
#[cfg(target_os = "linux")]
fn open_rollout(pid: u32) -> Option<PathBuf> {
    let open = |pid: u32| {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .ok()?
            .flatten()
            .filter_map(|e| std::fs::read_link(e.path()).ok())
            .find(|p| {
                p.extension().is_some_and(|e| e == "jsonl")
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("rollout-"))
            })
    };
    open(pid).or_else(|| {
        std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
            .ok()?
            .split_whitespace()
            .filter_map(|c| c.parse().ok())
            .find_map(open)
    })
}

/// What a rollout's first line says about its session.
struct Meta {
    cwd: PathBuf,
    /// When it started, in seconds since the epoch.
    start: u64,
}

#[derive(Deserialize)]
struct Line<T> {
    #[serde(rename = "type")]
    kind: String,
    payload: T,
}

#[derive(Deserialize)]
struct SessionMeta {
    cwd: PathBuf,
    timestamp: String,
}

fn meta(path: &Path) -> Option<Meta> {
    let mut first = String::new();
    std::io::BufReader::new(std::fs::File::open(path).ok()?)
        .read_line(&mut first)
        .ok()?;
    let line: Line<SessionMeta> = serde_json::from_str(&first).ok()?;
    if line.kind != "session_meta" {
        return None;
    }
    let start = DateTime::parse_from_rfc3339(&line.payload.timestamp).ok()?;
    Some(Meta {
        cwd: line.payload.cwd,
        start: u64::try_from(start.timestamp()).ok()?,
    })
}

#[derive(Deserialize)]
struct TokenCount {
    #[serde(rename = "type")]
    kind: String,
    info: Option<Info>,
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct Info {
    total_token_usage: TokenUsage,
    last_token_usage: TokenUsage,
    model_context_window: Option<u64>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct TokenUsage {
    input_tokens: u64,
    total_tokens: u64,
}

#[derive(Deserialize)]
struct RateLimits {
    limit_id: Option<String>,
    primary: Option<RateWindow>,
    secondary: Option<RateWindow>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct RateWindow {
    used_percent: Option<f32>,
    window_minutes: Option<u32>,
    resets_at: Option<u64>,
}

fn token_count(line: &str) -> Option<TokenCount> {
    let line: Line<TokenCount> = serde_json::from_str(line).ok()?;
    (line.kind == "event_msg" && line.payload.kind == "token_count").then_some(line.payload)
}

/// The session's usage, from its last `token_count` that has one.
fn usage(path: &Path) -> Option<Session> {
    let text = tail(path, TAIL)?;
    text.lines().rev().find_map(|line| {
        let info = token_count(line)?.info?;
        let context_percent = info
            .model_context_window
            .filter(|&w| w > 0)
            .map(|w| (info.last_token_usage.input_tokens as f64 / w as f64 * 100.0) as f32);
        Some(Session {
            context_percent,
            tokens: Some(info.total_token_usage.total_tokens),
            ..Session::default()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-08T22:15:12Z
    const START: u64 = 1_791_497_712;

    fn rollout(dir: &Path, name: &str, cwd: &str, start: &str, lines: &[String]) -> PathBuf {
        let folder = dir.join("sessions/2026/10/08");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("rollout-{name}.jsonl"));
        let meta = format!(
            r#"{{"timestamp":"{start}","type":"session_meta","payload":{{"id":"{name}","timestamp":"{start}","cwd":"{cwd}","cli_version":"0.154.0"}}}}"#
        );
        let text = std::iter::once(meta)
            .chain(lines.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, text).unwrap();
        path
    }

    fn count(input: u64, total: u64, limits: &str) -> String {
        format!(
            r#"{{"type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":{total}}},"last_token_usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":2}},"model_context_window":200000}},"rate_limits":{limits}}}}}"#
        )
    }

    const LIMITS: &str = r#"{"limit_id":"codex","primary":{"used_percent":2.0,"window_minutes":300,"resets_at":1791500000},"secondary":{"used_percent":19.5,"window_minutes":10080,"resets_at":1791900000},"rate_limit_reached_type":null}"#;

    fn proc(cwd: &str, started: u64) -> Proc {
        Proc {
            agent: "codex".into(),
            // No such process: the folder and start decide.
            pid: u32::MAX,
            started,
            cwd: Some(cwd.into()),
            terminal: "1-0".into(),
        }
    }

    #[test]
    fn a_process_reads_the_session_it_started() {
        let dir = tempfile::tempdir().unwrap();
        rollout(
            dir.path(),
            "a",
            "/srv/app",
            "2026-10-08T22:15:12.504Z",
            &[count(50_000, 1234, LIMITS), count(100_000, 5678, "null")],
        );
        let p = proc("/srv/app", START - 1);
        let found = sessions(&[&p], dir.path(), START + 60);
        let s = found[0].clone().unwrap();
        assert_eq!(s.tokens, Some(5678));
        assert_eq!(s.context_percent, Some(50.0));
    }

    /// Two Codex in one folder each get their own; one started after the file isn't its.
    #[test]
    fn two_processes_in_one_folder_each_get_their_own_file() {
        let dir = tempfile::tempdir().unwrap();
        rollout(
            dir.path(),
            "a",
            "/srv/app",
            "2026-10-08T22:15:12Z",
            &[count(1, 111, "null")],
        );
        rollout(
            dir.path(),
            "b",
            "/srv/app",
            "2026-10-08T22:25:12Z",
            &[count(1, 222, "null")],
        );
        let (a, b) = (proc("/srv/app", START), proc("/srv/app", START + 600));
        let found = sessions(&[&a, &b], dir.path(), START + 900);
        assert_eq!(found[0].as_ref().unwrap().tokens, Some(111));
        assert_eq!(found[1].as_ref().unwrap().tokens, Some(222));
        // Started after both files were last written, elsewhere: none is its.
        let late = proc("/srv/other", START + 3600);
        assert_eq!(sessions(&[&late], dir.path(), START + 3700), vec![None]);
    }

    /// On Linux the file a Codex has open is its own, even a session from long ago it resumed.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_rollout_a_process_has_open_is_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("sessions/2020/01/01");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("rollout-old.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{}",
                r#"{"type":"session_meta","payload":{"cwd":"/srv/app","timestamp":"2020-01-01T00:00:00Z"}}"#,
                count(1, 444, "null")
            ),
        )
        .unwrap();
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("exec 3<\"$0\"; sleep 10")
            .arg(&path)
            .spawn()
            .unwrap();
        let p = Proc {
            pid: child.id(),
            ..proc("/srv/app", START)
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let found = loop {
            let found = sessions(&[&p], dir.path(), START + 60);
            if found[0].is_some() || std::time::Instant::now() > deadline {
                break found;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(found[0].as_ref().map(|s| s.tokens), Some(Some(444)));
    }

    #[test]
    fn a_turn_without_info_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let null_info = r#"{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":null}}"#;
        rollout(
            dir.path(),
            "a",
            "/srv/app",
            "2026-10-08T22:15:12Z",
            &[count(1, 333, "null"), null_info.to_string()],
        );
        let p = proc("/srv/app", START);
        let found = sessions(&[&p], dir.path(), START + 60);
        assert_eq!(found[0].as_ref().unwrap().tokens, Some(333));
    }

    #[test]
    fn limits_come_from_codexs_own_rate_limits() {
        let dir = tempfile::tempdir().unwrap();
        let premium = r#"{"limit_id":"premium","primary":null,"secondary":null}"#;
        let one_window = r#"{"limit_id":"codex","primary":{"used_percent":100.0,"window_minutes":300,"resets_at":1791500000},"secondary":null,"rate_limit_reached_type":"primary"}"#;
        let a = rollout(
            dir.path(),
            "a",
            "/srv/app",
            "2026-10-08T22:15:12Z",
            &[count(1, 1, LIMITS), count(1, 1, premium)],
        );
        let file = std::fs::File::options().write(true).open(&a).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(START))
            .unwrap();
        let l = limits(dir.path(), START + 60).unwrap();
        assert_eq!(
            l.windows,
            vec![
                Window {
                    minutes: 300,
                    used_percent: 2.0,
                    resets_at: 1791500000,
                },
                Window {
                    minutes: 10080,
                    used_percent: 19.5,
                    resets_at: 1791900000,
                },
            ]
        );
        rollout(
            dir.path(),
            "b",
            "/srv/app",
            "2026-10-08T22:25:12Z",
            &[count(1, 1, one_window)],
        );
        let l = limits(dir.path(), START + 60).unwrap();
        assert_eq!(l.windows.len(), 1);
        assert_eq!(l.windows[0].used_percent, 100.0);
        // A Codex just started has no limits yet: the older file's stay.
        let c = rollout(dir.path(), "c", "/srv/app", "2026-10-08T22:35:12Z", &[]);
        let file = std::fs::File::options().write(true).open(&c).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
            .unwrap();
        let l = limits(dir.path(), START + 60).unwrap();
        assert_eq!(l.windows[0].used_percent, 100.0);
    }
}
