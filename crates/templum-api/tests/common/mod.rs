//! What the API's integration tests share: a server on an ephemeral loopback port whose "UI
//! thread" is a fake that records what it is sent and answers like Templum would, and a small
//! HTTP client. Not every test uses all of it.
#![allow(dead_code)]

use futures::StreamExt;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use templum_api::model::*;
use templum_api::store::{Scope, Settings, Store, Token};
use templum_api::{Options, Server, start};
use templum_mcp::{Incoming, Reply, rpc, tools};

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
    pub server: Server,
    pub base: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
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
    let server = start(options, tokens, tx).unwrap();
    let base = format!(
        "{}://127.0.0.1:{}",
        if tls { "https" } else { "http" },
        server.addr().port()
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
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
        dir,
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
