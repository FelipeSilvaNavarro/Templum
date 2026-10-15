//! The local socket every MCP server, hook and `templum` command started in Templum's terminals
//! talks to (docs/specs/connections.md, The bus). A thread accepts connections; each has a
//! thread that reads its lines and one that writes its answers, so the UI thread only ever
//! sends to a channel.

use std::io::{self, BufRead as _, BufReader, Read, Write};
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::channel::mpsc::UnboundedSender;
use serde_json::json;

use crate::rpc::{self, Incoming};

/// The first line every client sends.
pub const HELLO: &str = "templum/hello";
/// The longest hello read, in bytes: a longer first line closes the connection.
const HELLO_MAX: u64 = 4096;
/// How long a new connection may take to send its hello.
const HELLO_WAIT: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 5 });

/// Who a connection is for: the terminal it runs in (its `TEMPLUM_TERMINAL`), and what it is
/// ("mcp", "cli" or "hook").
#[derive(Clone, Debug, PartialEq)]
pub struct Hello {
    pub terminal: String,
    pub client: String,
}

/// Sends an answer line back to the connection a request came on.
pub type Replier = std::sync::mpsc::Sender<String>;

/// A line from a client, for the UI thread.
pub struct Message {
    pub hello: Hello,
    pub incoming: Incoming,
    /// Answers it. A connection closes once its client is done writing and every replier of
    /// it is dropped.
    pub reply: Replier,
}

/// The bus, while Templum runs. Dropping it stops taking connections and removes its socket.
pub struct Bus {
    /// Its address, as terminals get it in `TEMPLUM_BUS`.
    pub env: String,
    /// Unix: its socket file.
    path: Option<PathBuf>,
    /// Unix: the folder of its own the socket is in, in the temporary folder, when the data
    /// folder's path is too long for a socket.
    dir: Option<PathBuf>,
    closed: Arc<AtomicBool>,
}

/// Starts the bus in Templum's data folder `data`: every client's lines go to `tx`.
pub fn start(data: &Path, tx: UnboundedSender<Message>) -> io::Result<Bus> {
    let (listener, bus, token) = listen(data)?;
    let stop = bus.closed.clone();
    std::thread::Builder::new()
        .name("templum-bus".into())
        .spawn(move || {
            loop {
                let stream = listener.accept();
                // Drop wakes this with a connection of its own.
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (tx, token) = (tx.clone(), token.clone());
                let _ = std::thread::Builder::new()
                    .name("templum-bus-reader".into())
                    .spawn(move || serve(stream, &tx, token.as_deref()));
            }
        })?;
    Ok(bus)
}

impl Drop for Bus {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let _ = connect(&self.env);
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// Reads a connection's lines: its hello first, then each request or notification, handed to
/// `tx` with a replier; a line that isn't JSON-RPC is answered at once. Until its hello is
/// accepted, a connection gets no writer and only `HELLO_MAX` bytes and `HELLO_WAIT` to send it
/// (on Windows, other local users can reach the port).
fn serve(stream: Stream, tx: &UnboundedSender<Message>, token: Option<&str>) {
    if stream.set_read_timeout(Some(HELLO_WAIT)).is_err() {
        return;
    }
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    // Through the reader: what the client sent after its hello stays in its buffer.
    let read = (&mut reader).take(HELLO_MAX).read_line(&mut first);
    let Some(hello) = read
        .ok()
        .filter(|_| first.ends_with('\n'))
        .and_then(|_| hello_of(&first, token))
    else {
        return;
    };
    let Ok(writer) = reader
        .get_ref()
        .set_read_timeout(None)
        .and_then(|()| reader.get_ref().try_clone())
    else {
        return;
    };
    let (reply, replies) = std::sync::mpsc::channel::<String>();
    let _ = std::thread::Builder::new()
        .name("templum-bus-writer".into())
        .spawn(move || write_lines(writer, &replies));
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        match rpc::parse(&line) {
            Ok(incoming) => {
                let message = Message {
                    hello: hello.clone(),
                    incoming,
                    reply: reply.clone(),
                };
                if tx.unbounded_send(message).is_err() {
                    break;
                }
            }
            Err(answer) => {
                let _ = reply.send(answer);
            }
        }
    }
}

/// Writes the answers until every replier is gone, then closes the connection.
fn write_lines(mut stream: Stream, lines: &std::sync::mpsc::Receiver<String>) {
    for line in lines {
        if stream
            .write_all(line.as_bytes())
            .and_then(|()| stream.flush())
            .is_err()
        {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

fn hello_of(line: &str, token: Option<&str>) -> Option<Hello> {
    let Ok(Incoming::Notification { method, params }) = rpc::parse(line) else {
        return None;
    };
    (method == HELLO && params["token"].as_str() == token).then(|| Hello {
        terminal: params["terminal"].as_str().unwrap_or_default().to_string(),
        client: params["client"].as_str().unwrap_or_default().to_string(),
    })
}

/// A client's first line.
pub fn hello_line(terminal: &str, client: &str, token: Option<&str>) -> String {
    rpc::notification(
        HELLO,
        json!({ "terminal": terminal, "client": client, "token": token }),
    )
}

/// Connects to the bus at `env` (a `TEMPLUM_BUS`): the stream, and the token its hello carries.
pub fn connect(env: &str) -> io::Result<(Stream, Option<String>)> {
    let bad = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a bus address: {env}"),
        )
    };
    if let Some(rest) = env.strip_prefix("tcp:") {
        let (address, token) = rest.rsplit_once(':').ok_or_else(bad)?;
        let address: std::net::SocketAddr = address.parse().map_err(|_| bad())?;
        let stream =
            std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(1))?;
        return Ok((Stream::Tcp(stream), Some(token.to_string())));
    }
    #[cfg(unix)]
    if let Some(path) = env.strip_prefix("unix:") {
        let stream = std::os::unix::net::UnixStream::connect(path)?;
        return Ok((Stream::Unix(stream), None));
    }
    Err(bad())
}

/// A connection to the bus.
pub enum Stream {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    Tcp(std::net::TcpStream),
}

impl Stream {
    pub fn try_clone(&self) -> io::Result<Stream> {
        Ok(match self {
            #[cfg(unix)]
            Stream::Unix(s) => Stream::Unix(s.try_clone()?),
            Stream::Tcp(s) => Stream::Tcp(s.try_clone()?),
        })
    }

    pub fn set_read_timeout(&self, limit: Option<Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(limit),
            Stream::Tcp(s) => s.set_read_timeout(limit),
        }
    }

    /// Writes no more: the other side reads the end, and may still answer.
    pub fn finish(&self) -> io::Result<()> {
        self.shutdown(Shutdown::Write)
    }

    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.shutdown(how),
            Stream::Tcp(s) => s.shutdown(how),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
            Stream::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
            Stream::Tcp(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
            Stream::Tcp(s) => s.flush(),
        }
    }
}

enum Listener {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixListener),
    #[cfg(windows)]
    Tcp(std::net::TcpListener),
}

impl Listener {
    fn accept(&self) -> io::Result<Stream> {
        match self {
            #[cfg(unix)]
            Listener::Unix(l) => l.accept().map(|(s, _)| Stream::Unix(s)),
            #[cfg(windows)]
            Listener::Tcp(l) => l.accept().map(|(s, _)| Stream::Tcp(s)),
        }
    }
}

/// Unix: a socket at `<data>/bus/<pid>-<n>.sock` in a folder only the user can open. The
/// counter keeps two buses of one process apart (tests open several workspaces).
#[cfg(unix)]
fn listen(data: &Path) -> io::Result<(Listener, Bus, Option<String>)> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    use std::sync::atomic::AtomicU64;
    let folder = data.join("bus");
    std::fs::create_dir_all(&folder)?;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?;
    remove_stale(&folder);
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        "{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut path = folder.join(&name);
    let mut dir = None;
    // macOS takes socket paths of 104 bytes at most. The temporary folder may be shared (/tmp):
    // the socket goes in a folder of its own there, with a name no one can guess and take
    // first, that only the user can open. Short, for macOS's long temporary folder.
    if path.as_os_str().len() > 100 {
        let random = uuid::Uuid::new_v4().simple().to_string();
        let private = std::env::temp_dir().join(format!("templum-{}", &random[..12]));
        // Not create_all: a folder already there fails.
        std::fs::DirBuilder::new().mode(0o700).create(&private)?;
        path = private.join("bus.sock");
        dir = Some(private);
    }
    // Made before the socket: dropping it on an error below removes what was made.
    let bus = Bus {
        env: format!("unix:{}", path.display()),
        path: Some(path.clone()),
        dir,
        closed: Arc::default(),
    };
    if path.as_os_str().len() > 100 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("no socket path short enough: {}", path.display()),
        ));
    }
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok((Listener::Unix(listener), bus, None))
}

/// Windows: TCP on the loopback at a port the system picks, and a token every hello carries.
#[cfg(windows)]
fn listen(_: &Path) -> io::Result<(Listener, Bus, Option<String>)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let bus = Bus {
        env: format!("tcp:127.0.0.1:{port}:{token}"),
        path: None,
        dir: None,
        closed: Arc::default(),
    };
    Ok((Listener::Tcp(listener), bus, Some(token)))
}

/// Removes the socket files no Templum listens on any more (one that crashed).
#[cfg(unix)]
fn remove_stale(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let refused = path.extension().is_some_and(|e| e == "sock")
            && std::os::unix::net::UnixStream::connect(&path)
                .is_err_and(|e| e.kind() == io::ErrorKind::ConnectionRefused);
        if refused {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// A bus in a temporary folder whose messages `answer` handles on a thread of its own.
#[cfg(test)]
pub(crate) fn test_bus(answer: impl Fn(Message) + Send + 'static) -> (tempfile::TempDir, Bus) {
    use futures::StreamExt as _;
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let bus = start(dir.path(), tx).unwrap();
    std::thread::spawn(move || {
        while let Some(message) = futures::executor::block_on(rx.next()) {
            answer(message);
        }
    });
    (dir, bus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// A read that waits longer fails the test instead of hanging it.
    fn time_limit(stream: &Stream) {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
    }

    /// Reads after what `stream` sent: the connection was closed (the end, or a reset). Not the
    /// time limit: that is still open.
    fn assert_closed(stream: Stream) {
        let mut rest = String::new();
        let read = match BufReader::new(stream).read_line(&mut rest) {
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                panic!("still open")
            }
            read => read.unwrap_or(0),
        };
        assert_eq!(read, 0, "{rest}");
    }

    fn pong(message: Message) {
        if let Incoming::Request { id, .. } = &message.incoming {
            let answer =
                json!({ "terminal": message.hello.terminal, "client": message.hello.client });
            let _ = message.reply.send(rpc::result(id, answer));
        }
    }

    fn client(bus: &Bus) -> (Stream, BufReader<Stream>) {
        let (mut stream, token) = connect(&bus.env).unwrap();
        time_limit(&stream);
        stream
            .write_all(hello_line("123-4", "cli", token.as_deref()).as_bytes())
            .unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        (stream, reader)
    }

    fn answer(reader: &mut BufReader<Stream>) -> Value {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn a_request_is_answered_on_its_connection() {
        let (_dir, bus) = test_bus(pong);
        let scheme = if cfg!(unix) { "unix:" } else { "tcp:" };
        assert!(bus.env.starts_with(scheme), "{}", bus.env);
        let (mut stream, mut reader) = client(&bus);
        stream
            .write_all(rpc::request(1, "ping", Value::Null).as_bytes())
            .unwrap();
        let answer = answer(&mut reader);
        assert_eq!(answer["id"], 1);
        assert_eq!(
            answer["result"],
            json!({ "terminal": "123-4", "client": "cli" })
        );
    }

    #[test]
    fn a_slow_answer_never_holds_back_the_next() {
        // The first request is answered after the second.
        let held = std::sync::Mutex::new(None::<Message>);
        let (_dir, bus) = test_bus(move |message| {
            let mut held = held.lock().unwrap();
            match held.take() {
                None => *held = Some(message),
                Some(first) => {
                    pong(message);
                    pong(first);
                }
            }
        });
        let (mut stream, mut reader) = client(&bus);
        stream
            .write_all(rpc::request(1, "slow", Value::Null).as_bytes())
            .unwrap();
        stream
            .write_all(rpc::request(2, "quick", Value::Null).as_bytes())
            .unwrap();
        let ids = [
            answer(&mut reader)["id"].clone(),
            answer(&mut reader)["id"].clone(),
        ];
        assert_eq!(ids, [json!(2), json!(1)]);
    }

    #[test]
    fn a_bad_line_is_answered_and_the_next_still_served() {
        let (_dir, bus) = test_bus(pong);
        let (mut stream, mut reader) = client(&bus);
        stream.write_all(b"{oops\n").unwrap();
        stream
            .write_all(rpc::request(2, "ping", Value::Null).as_bytes())
            .unwrap();
        assert_eq!(answer(&mut reader)["error"]["code"], rpc::PARSE);
        assert_eq!(answer(&mut reader)["id"], 2);
    }

    #[test]
    fn a_connection_without_its_hello_is_closed() {
        let (_dir, bus) = test_bus(pong);
        for first in [
            rpc::request(1, "ping", Value::Null),
            hello_line("1-1", "cli", Some("wrong")),
        ] {
            let (mut stream, _) = connect(&bus.env).unwrap();
            time_limit(&stream);
            stream.write_all(first.as_bytes()).unwrap();
            // It may already be closed.
            let _ = stream.write_all(rpc::request(2, "ping", Value::Null).as_bytes());
            assert_closed(stream);
        }
    }

    #[test]
    fn a_hello_too_long_or_too_slow_closes_the_connection() {
        let (_dir, bus) = test_bus(pong);
        // A right hello, but longer than any Templum sends.
        let (mut stream, token) = connect(&bus.env).unwrap();
        time_limit(&stream);
        let terminal = "1".repeat(HELLO_MAX as usize);
        let _ = stream.write_all(hello_line(&terminal, "cli", token.as_deref()).as_bytes());
        let _ = stream.write_all(rpc::request(2, "ping", Value::Null).as_bytes());
        assert_closed(stream);
        // Nothing sent: closed once the hello's time is up, before the test's own limit.
        let (stream, _) = connect(&bus.env).unwrap();
        time_limit(&stream);
        let started = std::time::Instant::now();
        assert_closed(stream);
        assert!(started.elapsed() >= HELLO_WAIT / 2);
    }

    #[test]
    fn a_client_done_writing_gets_its_answers_then_the_end() {
        let (_dir, bus) = test_bus(pong);
        let (mut stream, mut reader) = client(&bus);
        stream
            .write_all(rpc::request(1, "ping", Value::Null).as_bytes())
            .unwrap();
        stream.finish().unwrap();
        assert_eq!(answer(&mut reader)["id"], 1);
        let mut rest = String::new();
        assert_eq!(
            reader.read_line(&mut rest).unwrap(),
            0,
            "closed after its answers"
        );
    }

    #[test]
    fn nothing_to_connect_to_is_an_error() {
        assert!(connect("nonsense").is_err());
        #[cfg(unix)]
        assert!(connect("unix:/nonexistent/templum.sock").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_goes_with_the_bus_and_a_stale_one_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join("bus").join("1-0.sock");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        // A socket file nobody listens on, as a Templum that crashed leaves it.
        drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
        assert!(stale.exists());
        let (tx, _rx) = futures::channel::mpsc::unbounded();
        let bus = start(dir.path(), tx).unwrap();
        assert!(!stale.exists());
        let path = PathBuf::from(bus.env.strip_prefix("unix:").unwrap());
        assert!(path.exists());
        drop(bus);
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_data_folder_too_long_for_a_socket_puts_it_in_a_private_temporary_folder() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("d".repeat(100));
        std::fs::create_dir(&data).unwrap();
        let (tx, _rx) = futures::channel::mpsc::unbounded();
        let bus = start(&data, tx).unwrap();
        let path = PathBuf::from(bus.env.strip_prefix("unix:").unwrap());
        let private = path.parent().unwrap().to_path_buf();
        assert_eq!(private.parent().unwrap(), std::env::temp_dir());
        let mode = std::fs::metadata(&private).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "only the user opens it");
        // It answers there.
        connect(&bus.env).unwrap();
        drop(bus);
        assert!(!private.exists(), "the folder goes with the bus");
    }
}
