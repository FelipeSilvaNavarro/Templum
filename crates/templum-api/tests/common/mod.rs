//! What the API's integration tests share: a server on an ephemeral loopback port whose "UI
//! thread" is a fake that records what it is sent and answers like Templum would, and a small
//! HTTP client. Not every test uses all of it.
#![allow(dead_code)]

use futures::StreamExt;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use templum_api::model::*;
use templum_api::store::{Scope, Settings, Store, Token};
use templum_api::{Options, Server, TerminalRef, codes, registry, start};
use templum_mcp::{Incoming, Reply, rpc, tools};
use templum_terminal::{Palette, TermOptions, Terminal};

/// A token with the given scope and workspaces, and its text. Ids are unique in the process.
pub fn token(scope: Scope, workspaces: Option<Vec<&str>>) -> (String, Token) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut store = Store::default();
    let workspaces = workspaces.map(|w| w.into_iter().map(String::from).collect());
    let text = store.mint("phone", scope, None, workspaces, 0).unwrap();
    let mut token = store.tokens.remove(0);
    token.id = NEXT.fetch_add(1, Ordering::Relaxed);
    (text, token)
}

/// One of everything, in workspace w3: the block b7k2m (with an agent), a kanban k9x1d showing
/// board 3, the request r12 of b7k2m, card 40, automation 1 and a usage.
pub fn sample() -> State {
    let mut b7k2m = BlockView {
        id: "b7k2m".into(),
        workspace: "w3".into(),
        kind: "terminal".into(),
        title: "Claude Code".into(),
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: 720.0,
            h: 480.0,
        },
        ..Default::default()
    };
    b7k2m.agent = Some(AgentView {
        agent: "claude".into(),
        state: "needs_you".into(),
        ..Default::default()
    });
    let k9x1d = BlockView {
        id: "k9x1d".into(),
        workspace: "w3".into(),
        kind: "kanban".into(),
        title: "Sprint".into(),
        board: Some(3),
        z: 1,
        ..Default::default()
    };
    State {
        app: App {
            version: "0.1.5".into(),
            api: 1,
            shown: Some("w3".into()),
        },
        workspaces: vec![WorkspaceView {
            id: "w3".into(),
            name: "API".into(),
            shown: true,
            ..Default::default()
        }],
        blocks: vec![b7k2m, k9x1d],
        wires: vec![WireView {
            a: "b7k2m".into(),
            b: "k9x1d".into(),
            label: None,
        }],
        requests: vec![RequestView {
            id: "r12".into(),
            kind: "question".into(),
            block: Some("b7k2m".into()),
            workspace: Some("w3".into()),
            summary: "Which database?".into(),
            ..Default::default()
        }],
        boards: vec![BoardView {
            id: 3,
            name: "Sprint".into(),
            columns: vec![ColumnView {
                id: 7,
                title: "Doing".into(),
                cards: vec![40],
            }],
        }],
        cards: vec![CardView {
            id: 40,
            board: 3,
            column: 7,
            title: "Login".into(),
            ..Default::default()
        }],
        automations: vec![AutomationView {
            id: 1,
            name: "Morning".into(),
            workspace: Some("w3".into()),
            ..Default::default()
        }],
        usage: Some(UsageView::default()),
        view: Some(ViewView {
            workspace: "w3".into(),
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
            zoom: 1.0,
        }),
    }
}

/// What the fake UI thread was sent.
#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    pub params: Value,
    pub terminal: String,
    pub client: String,
    pub conn: u64,
    pub notification: bool,
}

pub struct Fake {
    pub server: Arc<Server>,
    pub base: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
    /// The terminals the fake UI thread lends when a terminal stream opens.
    pub terminals: Arc<Mutex<Vec<TerminalRef>>>,
    pub dir: tempfile::TempDir,
}

impl Fake {
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    /// What the UI thread was asked: every message that wanted an answer.
    pub fn asked(&self) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|s| !s.notification)
            .collect()
    }

    /// What was sent to the UI thread that wasn't the state or a notification.
    pub fn calls(&self) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|s| !s.notification && s.method == "tools/call")
            .collect()
    }

    pub fn notifications(&self, method: &str) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|s| s.notification && s.method == method)
            .collect()
    }

    pub fn call(&self, request: Req) -> Answer {
        request.send(&self.base)
    }
}

/// A server on `127.0.0.1:0`, plain, with a temporary data folder; `tweak` changes its options.
pub fn fake(tokens: Vec<Token>, tweak: impl FnOnce(&mut Options)) -> Fake {
    fake_with(
        Settings {
            port: 0,
            ..Default::default()
        },
        tokens,
        tweak,
    )
}

pub fn fake_with(settings: Settings, tokens: Vec<Token>, tweak: impl FnOnce(&mut Options)) -> Fake {
    let dir = tempfile::tempdir().unwrap();
    let mut options = Options::from(&settings, dir.path()).unwrap();
    tweak(&mut options);
    let tls = options.tls.is_some();
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let server = Arc::new(start(options, tokens, tx).unwrap());
    let base = format!(
        "{}://127.0.0.1:{}",
        if tls { "https" } else { "http" },
        server.addr().port()
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let terminals: Arc<Mutex<Vec<TerminalRef>>> = Arc::default();
    let lent = terminals.clone();
    // A weak hold, so that dropping the fake still drops the server.
    let lender = Arc::downgrade(&server);
    std::thread::spawn(move || {
        // Calls that never get an answer keep their repliers, as a waiting task does.
        let mut held = Vec::new();
        while let Some(message) = futures::executor::block_on(rx.next()) {
            let (method, params, id) = match &message.incoming {
                Incoming::Request { method, params, id } => {
                    (method.clone(), params.clone(), Some(id.clone()))
                }
                Incoming::Notification { method, params } => (method.clone(), params.clone(), None),
                Incoming::Response => continue,
            };
            record.lock().unwrap().push(Seen {
                method: method.clone(),
                params: params.clone(),
                terminal: message.hello.terminal.clone(),
                client: message.hello.client.clone(),
                conn: message.conn,
                notification: id.is_none(),
            });
            let Some(id) = id else { continue };
            let name = params["name"].as_str().unwrap_or_default();
            let line = match method.as_str() {
                templum_api::STATE => rpc::result(&id, serde_json::to_value(sample()).unwrap()),
                templum_api::TERMINAL => {
                    let block = params["block"].as_str().unwrap_or_default();
                    let all = lent.lock().unwrap().clone();
                    if block == "n0te" {
                        tools::tool_result(&id, &Reply::err("n0te isn't a terminal."))
                    } else if all.iter().any(|t| t.block == block) {
                        if let Some(server) = lender.upgrade() {
                            server.set_terminals(all);
                        }
                        tools::tool_result(&id, &Reply::ok("Streaming."))
                    } else {
                        rpc::error(&id, codes::NOT_FOUND, &registry::no("block", block))
                    }
                }
                "tools/call" if name == "close_block" => {
                    tools::tool_result(&id, &Reply::err("No."))
                }
                "tools/call" if name == "delegate" && params["arguments"]["wait"] == true => {
                    held.push(message.reply);
                    continue;
                }
                "tools/call" if name == "read_note" => {
                    tools::tool_result(&id, &Reply::ok("did read_note"))
                }
                _ => tools::tool_result(&id, &Reply::ok(format!("did {name}"))),
            };
            let _ = message.reply.send(line);
        }
    });
    Fake {
        server,
        base,
        seen,
        terminals,
        dir,
    }
}

/// The calls' log and the noise's, one after the other.
pub fn logs(dir: &std::path::Path) -> String {
    let folder = templum_api::store::folder(dir);
    ["audit.log", "noise.log"]
        .iter()
        .map(|name| std::fs::read_to_string(folder.join(name)).unwrap_or_default())
        .collect()
}

/// A terminal that runs nothing: feed it bytes (`Terminal::feed`) and it shows them.
pub fn viewer(cols: u16, rows: u16) -> Terminal {
    Terminal::viewer(
        "v",
        (cols, rows),
        Palette::default(),
        TermOptions::default(),
        Arc::new(|| {}),
    )
}

/// Lends `term` to the fake's next terminal streams, as the block `block` of `workspace`.
pub fn lend(fake: &Fake, block: &str, workspace: &str, term: &Terminal) {
    fake.terminals.lock().unwrap().push(TerminalRef {
        block: block.into(),
        workspace: workspace.into(),
        term: term.shared().clone(),
    });
}

/// How a stream is opened: with a token, or with a ticket (`?ticket=` after the path).
pub enum Auth<'a> {
    Bearer(&'a str),
    Ticket(&'a str),
}

/// An open stream: its status and headers, and its lines as they arrive.
pub struct Stream {
    pub status: u16,
    pub headers: ureq::http::HeaderMap,
    lines: Receiver<String>,
}

#[derive(Debug)]
pub struct Event {
    pub id: String,
    pub event: String,
    pub data: Value,
}

/// `GET path` (with its query, if any) as a stream.
pub fn open(fake: &Fake, path: &str, auth: Auth, last_event_id: Option<&str>) -> Stream {
    let request = match auth {
        Auth::Bearer(token) => req("GET", path).token(token),
        Auth::Ticket(ticket) => {
            let joint = if path.contains('?') { '&' } else { '?' };
            req("GET", &format!("{path}{joint}ticket={ticket}"))
        }
    };
    let request = match last_event_id {
        Some(id) => request.header("Last-Event-ID", id),
        None => request,
    };
    let built = request.build(&fake.base);
    let (lines, receiver) = channel();
    let (head, heads) = channel();
    std::thread::spawn(move || {
        // No overall limit: a stream lasts. The test's reads are what time out.
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(None)
            .build();
        let agent: ureq::Agent = config.into();
        let response = agent.run(built).unwrap();
        let _ = head.send((response.status().as_u16(), response.headers().clone()));
        let mut reader = BufReader::new(response.into_body().into_reader());
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if lines.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let (status, headers) = heads
        .recv_timeout(Duration::from_secs(10))
        .expect("response headers");
    Stream {
        status,
        headers,
        lines: receiver,
    }
}

impl Stream {
    fn line(&self) -> Option<String> {
        match self.lines.recv_timeout(Duration::from_secs(10)) {
            Ok(line) => Some(line.trim_end_matches('\n').to_string()),
            Err(RecvTimeoutError::Disconnected) => None,
            Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for a line"),
        }
    }

    /// The next event, heartbeats skipped; None when the stream has ended.
    pub fn next(&self) -> Option<Event> {
        let (mut id, mut event, mut data) = (String::new(), String::new(), Value::Null);
        loop {
            let line = self.line()?;
            if line.is_empty() {
                if !event.is_empty() {
                    return Some(Event { id, event, data });
                }
            } else if line.starts_with(':') {
                // a comment
            } else if let Some(v) = line.strip_prefix("id: ") {
                id = v.into();
            } else if let Some(v) = line.strip_prefix("event: ") {
                event = v.into();
            } else if let Some(v) = line.strip_prefix("data: ") {
                data = serde_json::from_str(v).unwrap();
            }
        }
    }

    pub fn event(&self) -> Event {
        self.next().expect("the stream ended")
    }

    /// The next heartbeat: any line that is a comment, before an event.
    pub fn ping(&self) -> bool {
        loop {
            match self.line() {
                Some(line) if line == ": ping" => return true,
                Some(line) if line.is_empty() => {}
                Some(_) => return false,
                None => return false,
            }
        }
    }

    /// The stream ended (its body is over).
    pub fn ended(&self) -> bool {
        loop {
            match self.lines.recv_timeout(Duration::from_secs(10)) {
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected) => return true,
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
    }
}

/// Waits (up to 10 s) until `condition` holds.
pub fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A client that returns statuses as answers, not errors.
pub fn http() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(10)))
        .build();
    config.into()
}

/// A client for a server whose certificate it doesn't check (Templum's own is self-signed).
pub fn https_unverified() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .disable_verification(true)
        .build();
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(10)))
        .tls_config(tls)
        .build();
    config.into()
}

pub struct Req {
    method: &'static str,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
    agent: Option<ureq::Agent>,
}

pub fn req(method: &'static str, path: &str) -> Req {
    Req {
        method,
        path: path.into(),
        headers: Vec::new(),
        body: String::new(),
        agent: None,
    }
}

impl Req {
    pub fn token(self, text: &str) -> Req {
        self.header("Authorization", &format!("Bearer {text}"))
    }

    pub fn header(mut self, name: &str, value: &str) -> Req {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn json(self, body: &Value) -> Req {
        self.raw("application/json", &body.to_string())
    }

    pub fn raw(mut self, content_type: &str, body: &str) -> Req {
        self.headers
            .push(("Content-Type".into(), content_type.into()));
        self.body = body.into();
        self
    }

    /// A body that says nothing of what it is: no Content-Type.
    pub fn untyped(mut self, body: &str) -> Req {
        self.body = body.into();
        self
    }

    pub fn agent(mut self, agent: ureq::Agent) -> Req {
        self.agent = Some(agent);
        self
    }

    /// The request, unsent, for a client that wants the body to stream.
    pub fn build(&self, base: &str) -> ureq::http::Request<String> {
        let mut request = ureq::http::Request::builder()
            .method(self.method)
            .uri(format!("{base}{}", self.path));
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        request.body(self.body.clone()).unwrap()
    }

    pub fn send(&self, base: &str) -> Answer {
        let agent = self.agent.clone().unwrap_or_else(http);
        let response = agent
            .run(self.build(base))
            .unwrap_or_else(|e| panic!("{} {}: {e}", self.method, self.path));
        Answer::of(response)
    }
}

#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub headers: ureq::http::HeaderMap,
    pub text: String,
    pub json: Value,
}

impl Answer {
    pub fn of(mut response: ureq::http::Response<ureq::Body>) -> Answer {
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let json = serde_json::from_str(&text).unwrap_or(Value::Null);
        Answer {
            status,
            headers,
            text,
            json,
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// The `error.code` of an error answer.
    pub fn code(&self) -> &str {
        self.json["error"]["code"].as_str().unwrap_or_default()
    }

    pub fn message(&self) -> &str {
        self.json["error"]["message"].as_str().unwrap_or_default()
    }
}
