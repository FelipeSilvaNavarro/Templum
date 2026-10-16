//! Runs a real command in a real PTY.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use templum_terminal::{
    Agent, Notify, Palette, Processes, Snapshot, SpawnOptions, TermOptions, Terminal, inspect,
};

#[test]
fn runs_a_command_and_reports_its_exit() {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let notify: Notify = Arc::new(move || {
        let _ = tx.lock().unwrap().send(());
    });
    #[cfg(unix)]
    let command = (
        "sh".to_string(),
        vec!["-c".to_string(), "printf hello".to_string()],
    );
    #[cfg(windows)]
    let command = (
        "cmd".to_string(),
        vec!["/C".to_string(), "echo hello".to_string()],
    );
    let term = Terminal::spawn(
        SpawnOptions {
            cols: 40,
            rows: 5,
            cell_px: (8, 16),
            command: Some(command),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: None,
            integration: None,
            env: Vec::new(),
        },
        notify,
    )
    .expect("spawn");

    let deadline = Instant::now() + Duration::from_secs(15);
    while !term.exited() {
        assert!(Instant::now() < deadline, "the command never exited");
        let _ = rx.recv_timeout(Duration::from_millis(100));
        term.shared().rearm();
    }
    let mut snapshot = Snapshot::default();
    snapshot.refresh(term.shared(), &Palette::default());
    assert!(
        snapshot.text().contains("hello"),
        "screen was: {:?}",
        snapshot.text()
    );
    assert!(!term.title().trim().is_empty());
}

#[test]
fn shells_start_in_the_home_directory() {
    let home = std::env::home_dir().expect("a home directory");
    let notify: Notify = Arc::new(|| {});
    #[cfg(unix)]
    let command = ("sh".to_string(), vec!["-c".to_string(), "pwd".to_string()]);
    #[cfg(windows)]
    let command = ("cmd".to_string(), vec!["/C".to_string(), "cd".to_string()]);
    let term = Terminal::spawn(
        SpawnOptions {
            cols: 250,
            rows: 5,
            cell_px: (8, 16),
            command: Some(command),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: None,
            integration: None,
            env: Vec::new(),
        },
        notify,
    )
    .expect("spawn");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !term.exited() {
        assert!(Instant::now() < deadline, "the command never exited");
        std::thread::sleep(Duration::from_millis(50));
        term.shared().rearm();
    }
    let mut snapshot = Snapshot::default();
    snapshot.refresh(term.shared(), &Palette::default());
    let home = home.to_string_lossy();
    assert!(
        snapshot.text().lines().any(|l| l.trim_end() == home),
        "expected {home:?}, screen was: {:?}",
        snapshot.text()
    );
}

#[test]
fn starts_in_the_given_folder_and_reports_it() {
    // Started in the plain path (cmd.exe can't start in a canonical \\?\ path), compared
    // canonicalized: macOS's temp folder is behind a symlink, Windows's may be a short name.
    let dir = std::env::temp_dir();
    // Long-lived, so the process is still there to ask.
    #[cfg(unix)]
    let command = (
        "sh".to_string(),
        vec!["-c".to_string(), "sleep 20".to_string()],
    );
    #[cfg(windows)]
    let command = ("cmd".to_string(), vec!["/K".to_string()]);
    let term = Terminal::spawn(
        SpawnOptions {
            cols: 40,
            rows: 5,
            cell_px: (8, 16),
            command: Some(command),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: Some(dir.clone()),
            integration: None,
            env: Vec::new(),
        },
        Arc::new(|| {}),
    )
    .expect("spawn");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(cwd) = term.cwd() {
            assert_eq!(cwd.canonicalize().unwrap(), dir.canonicalize().unwrap());
            break;
        }
        assert!(Instant::now() < deadline, "cwd() never answered");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// macOS's login shell runs under `login`, which stays alive as the shell's parent; nested
/// shells (`nix develop`, `poetry shell`) look the same. The folder must follow the shell the
/// user types into, not the process Templum started.
#[cfg(unix)]
#[test]
fn cwd_follows_the_shell_in_the_foreground() {
    let start = tempfile::tempdir().unwrap();
    let moved = tempfile::tempdir().unwrap();
    // `; true` keeps the outer sh alive (no exec), like login.
    let command = (
        "sh".to_string(),
        vec!["-c".to_string(), "sh -i; true".to_string()],
    );
    let term = Terminal::spawn(
        SpawnOptions {
            cols: 80,
            rows: 5,
            cell_px: (8, 16),
            command: Some(command),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: Some(start.path().to_path_buf()),
            integration: None,
            env: Vec::new(),
        },
        Arc::new(|| {}),
    )
    .expect("spawn");
    term.write(format!("cd '{}'\r", moved.path().display()).into_bytes());
    let want = moved.path().canonicalize().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let cwd = term.cwd().and_then(|c| c.canonicalize().ok());
        if cwd.as_ref() == Some(&want) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cwd stayed {cwd:?}, want {want:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A long-lived process in `dir`, so it's still there to ask.
fn long_lived(dir: &std::path::Path) -> Terminal {
    #[cfg(unix)]
    let command = (
        "sh".to_string(),
        vec!["-c".to_string(), "sleep 20".to_string()],
    );
    #[cfg(windows)]
    let command = ("cmd".to_string(), vec!["/K".to_string()]);
    Terminal::spawn(
        SpawnOptions {
            cols: 40,
            rows: 5,
            cell_px: (8, 16),
            command: Some(command),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: Some(dir.to_path_buf()),
            integration: None,
            env: Vec::new(),
        },
        Arc::new(|| {}),
    )
    .expect("spawn")
}

#[test]
fn inspect_reads_several_terminals_at_once() {
    // Plain paths to start in (cmd.exe can't start in a canonical \\?\ path), compared canonicalized.
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let terms = [long_lived(a.path()), long_lived(b.path())];
    let want = [a.path(), b.path()].map(|p| p.canonicalize().unwrap());
    let processes: Vec<Processes> = terms.iter().map(Terminal::processes).collect();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found: Vec<_> = inspect(&processes, &known())
            .into_iter()
            .map(|i| i.folder.and_then(|f| f.canonicalize().ok()))
            .collect();
        if found == want.iter().cloned().map(Some).collect::<Vec<_>>() {
            assert!(
                inspect(&processes, &known())
                    .iter()
                    .all(|i| i.agent.is_none()),
                "a shell isn't an agent"
            );
            break;
        }
        assert!(Instant::now() < deadline, "found {found:?}, want {want:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(inspect(&[], &known()).is_empty());
}

#[test]
fn inspect_finds_an_agent_and_its_folder() {
    let bin = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let claude = fake_claude(bin.path()).display().to_string();
    #[cfg(unix)]
    let command = (claude, vec!["30".to_string()]);
    #[cfg(windows)]
    let command = (claude, vec!["/K".to_string()]);
    let options = || SpawnOptions {
        cols: 40,
        rows: 5,
        cell_px: (8, 16),
        command: Some(command.clone()),
        palette: Palette::default(),
        options: TermOptions::default(),
        working_directory: Some(work.path().to_path_buf()),
        integration: None,
        env: Vec::new(),
    };
    // Another test may fork while `fake_claude` still has its copy open for writing; that
    // child keeps the handle until it execs, and running `claude` meanwhile fails with
    // "Text file busy" on Linux. The window is short, so try again.
    let term = loop {
        match Terminal::spawn(options(), Arc::new(|| {})) {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(Duration::from_millis(50));
            }
            spawned => break spawned.expect("spawn"),
        }
    };
    let want = work.path().canonicalize().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found = inspect(&[term.processes()], &known()).pop().unwrap();
        let folder = found.folder.as_ref().and_then(|f| f.canonicalize().ok());
        if found.agent.as_deref() == Some("claude") && folder.as_ref() == Some(&want) {
            // The agent's process, for reading its usage: its pid and when it started.
            assert!(found.pid.is_some(), "found {found:?}");
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let started = found.started.expect("a start time");
            assert!(now.abs_diff(started) < 60, "started {started}, now {now}");
            break;
        }
        assert!(Instant::now() < deadline, "found {found:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// What the terminal's programs read in `TEMPLUM_TERMINAL`: Claude Code's status line, run by
/// Templum's relay, names its file after it.
#[test]
fn each_terminal_gets_its_own_id_in_its_environment() {
    let printed = |command: (String, Vec<String>)| {
        let term = Terminal::spawn(
            SpawnOptions {
                cols: 60,
                rows: 5,
                cell_px: (8, 16),
                command: Some(command),
                palette: Palette::default(),
                options: TermOptions::default(),
                working_directory: None,
                integration: None,
                env: Vec::new(),
            },
            Arc::new(|| {}),
        )
        .expect("spawn");
        let deadline = Instant::now() + Duration::from_secs(15);
        while !term.exited() {
            assert!(Instant::now() < deadline, "the command never exited");
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut snapshot = Snapshot::default();
        snapshot.refresh(term.shared(), &Palette::default());
        (term.id().to_string(), snapshot.text())
    };
    #[cfg(unix)]
    let command = || {
        (
            "sh".to_string(),
            vec![
                "-c".to_string(),
                "printf 'id=%s;' \"$TEMPLUM_TERMINAL\"".to_string(),
            ],
        )
    };
    #[cfg(windows)]
    let command = || {
        (
            "cmd".to_string(),
            vec!["/C".to_string(), "echo id=%TEMPLUM_TERMINAL%;".to_string()],
        )
    };
    let (a, a_text) = printed(command());
    let (b, b_text) = printed(command());
    assert!(!a.is_empty());
    assert_ne!(a, b);
    assert!(
        a_text.contains(&format!("id={a};")),
        "screen was: {a_text:?}"
    );
    assert!(
        b_text.contains(&format!("id={b};")),
        "screen was: {b_text:?}"
    );
}

/// What Templum adds to a terminal's environment reaches its programs (the bus's address).
#[test]
fn extra_variables_reach_the_shell() {
    #[cfg(unix)]
    let command = (
        "sh".to_string(),
        vec![
            "-c".to_string(),
            "printf 'bus=%s;' \"$TEMPLUM_BUS\"".to_string(),
        ],
    );
    #[cfg(windows)]
    let command = (
        "cmd".to_string(),
        vec!["/C".to_string(), "echo bus=%TEMPLUM_BUS%;".to_string()],
    );
    let term = Terminal::spawn(
        SpawnOptions {
            cols: 60,
            rows: 5,
            cell_px: (8, 16),
            command: Some(command),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: None,
            integration: None,
            env: vec![("TEMPLUM_BUS".into(), "unix:/x.sock".into())],
        },
        Arc::new(|| {}),
    )
    .expect("spawn");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !term.exited() {
        assert!(Instant::now() < deadline, "the command never exited");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut snapshot = Snapshot::default();
    snapshot.refresh(term.shared(), &Palette::default());
    assert!(
        snapshot.text().contains("bus=unix:/x.sock;"),
        "{:?}",
        snapshot.text()
    );
}

/// A harmless long-running program, copied under an agent's name into `bin`.
fn fake_claude(bin: &std::path::Path) -> std::path::PathBuf {
    #[cfg(unix)]
    let (from, to) = (
        ["/bin/sleep", "/usr/bin/sleep"]
            .into_iter()
            .find(|p| std::path::Path::new(p).exists())
            .expect("sleep"),
        bin.join("claude"),
    );
    #[cfg(windows)]
    let (from, to) = (r"C:\Windows\System32\cmd.exe", bin.join("claude.exe"));
    std::fs::copy(from, &to).unwrap();
    to
}

/// The session an agent was started on, from its arguments: a shell named `pi` that waits.
#[cfg(unix)]
#[test]
fn inspect_reads_the_session_an_agent_was_started_on() {
    let bin = tempfile::tempdir().unwrap();
    let pi = bin.path().join("pi");
    std::fs::copy("/bin/sh", &pi).unwrap();
    let id = "019a1f2e-7b3c-7d4e-8f5a-6b7c8d9e0f1a";
    // `; :` keeps the shell from becoming `sleep`.
    let args = ["-c", "sleep 30; :", "pi", "--session-id", id];
    let options = || SpawnOptions {
        cols: 40,
        rows: 5,
        cell_px: (8, 16),
        command: Some((pi.display().to_string(), args.map(String::from).to_vec())),
        palette: Palette::default(),
        options: TermOptions::default(),
        working_directory: None,
        integration: None,
        env: Vec::new(),
    };
    // As in `inspect_finds_an_agent_and_its_folder`: a fork may hold the copy open a moment.
    let term = loop {
        match Terminal::spawn(options(), Arc::new(|| {})) {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(Duration::from_millis(50));
            }
            spawned => break spawned.expect("spawn"),
        }
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found = inspect(&[term.processes()], &known()).pop().unwrap();
        if found.agent.as_deref() == Some("pi") {
            assert_eq!(found.session.as_deref(), Some(id));
            break;
        }
        assert!(Instant::now() < deadline, "found {found:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The usual case: the user starts the agent from the shell, in another folder than the shell's.
#[test]
fn inspect_finds_an_agent_the_shell_started() {
    let bin = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let sub = work.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let claude = fake_claude(bin.path());
    #[cfg(unix)]
    let command = ("sh".to_string(), vec!["-i".to_string()]);
    // The outer cmd stays in `work`; the agent (a cmd copy) moves to `sub` and waits.
    #[cfg(windows)]
    let command = (
        "cmd".to_string(),
        [
            "/K",
            &claude.display().to_string(),
            "/K",
            "cd",
            "/d",
            &sub.display().to_string(),
        ]
        .map(String::from)
        .to_vec(),
    );
    let term = Terminal::spawn(
        SpawnOptions {
            cols: 200,
            rows: 5,
            cell_px: (8, 16),
            command: Some(command),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: Some(work.path().to_path_buf()),
            integration: None,
            env: Vec::new(),
        },
        Arc::new(|| {}),
    )
    .expect("spawn");
    // A job of the interactive shell, in its own folder; the shell stays in `work`.
    #[cfg(unix)]
    term.write(
        format!(
            "sh -c 'cd \"{}\" && exec \"{}\" 30'\r",
            sub.display(),
            claude.display()
        )
        .into_bytes(),
    );
    let want = sub.canonicalize().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found = inspect(&[term.processes()], &known()).pop().unwrap();
        let folder = found.folder.as_ref().and_then(|f| f.canonicalize().ok());
        if found.agent.as_deref() == Some("claude") && folder.as_ref() == Some(&want) {
            break;
        }
        assert!(Instant::now() < deadline, "found {found:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Templum's own agents, as the app gives them to `inspect`.
fn known() -> Vec<templum_terminal::Detect> {
    Agent::all().map(Agent::detect).collect()
}

/// Processes that ignore the hangup a closing terminal sends still end: those read with `tree`
/// before are killed by `kill_left`.
#[cfg(unix)]
#[test]
fn a_tree_is_ended_even_when_it_ignores_hangup() {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
    let term = Terminal::spawn(
        SpawnOptions {
            cols: 40,
            rows: 5,
            cell_px: (8, 16),
            command: Some((
                "sh".to_string(),
                vec![
                    "-c".to_string(),
                    "trap '' HUP; sleep 1000 & sleep 1000".to_string(),
                ],
            )),
            palette: Palette::default(),
            options: TermOptions::default(),
            working_directory: None,
            integration: None,
            env: Vec::new(),
        },
        Arc::new(|| {}),
    )
    .expect("spawn");
    let shell = term.processes().shell.expect("the shell's pid");
    let deadline = Instant::now() + Duration::from_secs(10);
    let tree = loop {
        let tree = templum_terminal::tree(&[shell]);
        if tree.len() >= 2 {
            break tree;
        }
        assert!(
            Instant::now() < deadline,
            "the sleeps never started: {tree:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(tree.iter().any(|r| r.pid == shell), "the shell is in it");
    let alive = || {
        let pids: Vec<Pid> = tree.iter().map(|r| Pid::from_u32(r.pid)).collect();
        let mut system = System::new();
        system
            .refresh_processes_specifics(ProcessesToUpdate::Some(&pids), ProcessRefreshKind::new());
        tree.iter()
            .filter(|r| {
                system.process(Pid::from_u32(r.pid)).is_some_and(|p| {
                    p.start_time() == r.started && p.status() != ProcessStatus::Zombie
                })
            })
            .count()
    };
    drop(term);
    std::thread::sleep(Duration::from_millis(300));
    assert!(alive() > 0, "the hangup was ignored");
    templum_terminal::kill_left(&tree);
    let deadline = Instant::now() + Duration::from_secs(10);
    while alive() > 0 {
        assert!(Instant::now() < deadline, "{} still running", alive());
        std::thread::sleep(Duration::from_millis(50));
    }
}
