//! The engine's process (docs/specs/templum-agent.md#the-engine): a child with piped stdio, its
//! lines read on threads of their own and handed over a channel, so the UI thread never waits.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};

use futures::channel::mpsc::UnboundedSender;

/// How to start the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
}

/// A line from the engine, or its end.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineLine {
    Out(String),
    Err(String),
    /// Its stdout closed: the process ended.
    Ended,
}

/// The engine's running process.
pub struct Engine {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Engine {
    /// Starts it; its stdout's lines, then its end, and its stderr's lines arrive on `lines`.
    pub fn start(launch: &Launch, lines: UnboundedSender<EngineLine>) -> std::io::Result<Engine> {
        let mut command = command(launch);
        command
            .args(&launch.args)
            .current_dir(&launch.cwd)
            .envs(launch.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in &launch.env_remove {
            command.env_remove(key);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let out = lines.clone();
        std::thread::spawn(move || {
            read_lines(stdout, |l| out.unbounded_send(EngineLine::Out(l)).is_ok());
            let _ = out.unbounded_send(EngineLine::Ended);
        });
        std::thread::spawn(move || {
            read_lines(stderr, |l| lines.unbounded_send(EngineLine::Err(l)).is_ok());
        });
        Ok(Engine {
            stdin: child.stdin.take(),
            child,
        })
    }

    /// Writes `line` to its stdin.
    pub fn send(&mut self, line: &str) -> std::io::Result<()> {
        let stdin = self.stdin.as_mut().ok_or(std::io::ErrorKind::BrokenPipe)?;
        stdin.write_all(line.as_bytes())?;
        stdin.flush()
    }

    /// Ends the process and waits for it, so its stdout's reader sees the end.
    pub fn stop(&mut self) {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The command that runs `launch.program`: through `cmd /C` for a Windows script, which
/// `CreateProcess` can't start by itself.
fn command(launch: &Launch) -> Command {
    let script = launch
        .program
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if cfg!(windows) && script {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(&launch.program);
        c
    } else {
        Command::new(&launch.program)
    }
}

/// Hands each line of `from` to `each` until the end, or until `each` says to stop. A line that
/// isn't UTF-8 is read lossily rather than taken for the end.
fn read_lines(from: impl Read, mut each: impl FnMut(String) -> bool) {
    for line in BufReader::new(from).split(b'\n').map_while(Result::ok) {
        let line = String::from_utf8_lossy(&line);
        if !each(line.strip_suffix('\r').unwrap_or(&line).to_owned()) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt as _;

    use super::*;

    #[test]
    #[ignore = "the fake engine the engine tests start"]
    fn fake_engine() {
        if let Ok(script) = std::env::var(crate::fake::VAR) {
            let script = std::fs::read_to_string(script).unwrap();
            crate::fake::run(&script, std::io::stdin().lock(), std::io::stdout().lock());
        }
    }

    #[test]
    fn a_message_gets_its_turns_lines_in_order_then_stop_ends_it() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("turns.jsonl");
        std::fs::write(&script, "{\"n\":1}\n{\"n\":2}\n---\n{\"n\":3}\n").unwrap();
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let launch = crate::fake::launch("engine::tests::fake_engine", &script, dir.path());
        let mut engine = Engine::start(&launch, tx).unwrap();
        engine.send("{\"m\":1}\n").unwrap();
        let mut outs = Vec::new();
        while outs.len() < 2 {
            match futures::executor::block_on(rx.next()).unwrap() {
                EngineLine::Out(l) if l.starts_with('{') => outs.push(l),
                _ => {}
            }
        }
        assert_eq!(outs, ["{\"n\":1}", "{\"n\":2}"]);
        engine.stop();
        // Every later line is the end, or the test harness's own words, never a turn's.
        let rest: Vec<EngineLine> = futures::executor::block_on(rx.collect());
        assert!(matches!(rest.last(), Some(EngineLine::Ended)));
        assert!(
            !rest
                .iter()
                .any(|l| matches!(l, EngineLine::Out(o) if o == "{\"n\":3}"))
        );
    }
}
