//! The event stream over real HTTP (docs/specs/api.md, Events).

mod common;

use common::*;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use templum_api::Notice;
use templum_api::model::State;
use templum_api::store::Scope;

/// An open `GET /v1/events`: its status and headers, and its lines as they arrive.
struct Stream {
    status: u16,
    headers: ureq::http::HeaderMap,
    lines: Receiver<String>,
}

#[derive(Debug)]
struct Event {
    id: String,
    event: String,
    data: Value,
}

enum Auth<'a> {
    Bearer(&'a str),
    Ticket(&'a str),
}

fn open(fake: &Fake, auth: Auth, last_event_id: Option<&str>) -> Stream {
    let request = match auth {
        Auth::Bearer(token) => req("GET", "/v1/events").token(token),
        Auth::Ticket(ticket) => req("GET", &format!("/v1/events?ticket={ticket}")),
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
    fn next(&self) -> Option<Event> {
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

    fn event(&self) -> Event {
        self.next().expect("the stream ended")
    }

    /// The next heartbeat: any line that is a comment, before an event.
    fn ping(&self) -> bool {
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
    fn ended(&self) -> bool {
        loop {
            match self.lines.recv_timeout(Duration::from_secs(10)) {
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected) => return true,
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn setup() -> (Fake, String) {
    let (text, token) = token(Scope::Read, None);
    (
        fake(vec![token], |options| {
            options.heartbeat = Duration::from_millis(100)
        }),
        text,
    )
}

#[test]
fn a_stream_starts_with_a_snapshot() {
    let (fake, text) = setup();
    let stream = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(stream.status, 200);
    let header = |name: &str| {
        stream
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    assert!(header("content-type").starts_with("text/event-stream"));
    assert_eq!(
        (
            header("cache-control").as_str(),
            header("x-accel-buffering").as_str()
        ),
        ("no-store", "no")
    );
    assert_eq!(header("x-content-type-options"), "nosniff");
    let first = stream.event();
    assert_eq!(first.event, "snapshot");
    let (run, n) = first.id.split_once('-').unwrap();
    assert!(
        run.len() == 8 && run.chars().all(|c| c.is_ascii_hexdigit()) && n.parse::<u64>().is_ok(),
        "{}",
        first.id
    );
    let state: State = serde_json::from_value(first.data).unwrap();
    assert_eq!(state, sample());
    wait_for("the client to stream", || {
        fake.server.clients().iter().any(|c| c.streaming)
    });
    assert_eq!(fake.server.streams(), 1);
}

#[test]
fn a_published_change_is_a_put_or_a_delete() {
    let (fake, text) = setup();
    let stream = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(stream.event().event, "snapshot");
    let mut moved = sample();
    moved.blocks[0].rect.x = 50.0;
    fake.server.publish(moved.clone());
    let put = stream.event();
    assert_eq!(
        (
            put.event.as_str(),
            put.data["op"].as_str(),
            put.data["id"].as_str()
        ),
        ("block", Some("put"), Some("b7k2m"))
    );
    assert_eq!(put.data["value"]["rect"]["x"], 50.0);
    assert!(put.id.contains('-'));
    // The same state again says nothing; the next change is the next event.
    fake.server.publish(moved.clone());
    let mut answered = moved;
    answered.requests.clear();
    fake.server.publish(answered);
    let gone = stream.event();
    assert_eq!(
        (gone.event.as_str(), gone.data),
        ("request", json!({"op": "delete", "id": "r12"}))
    );
}

#[test]
fn a_limited_token_hears_only_its_workspaces() {
    let (text, token) = token(Scope::Read, Some(vec!["w3"]));
    let fake = fake(vec![token], |options| {
        options.heartbeat = Duration::from_millis(100)
    });
    let stream = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(stream.event().event, "snapshot");
    // A new block in w4 is not heard.
    let mut elsewhere = sample();
    let mut x1 = elsewhere.blocks[0].clone();
    x1.id = "x1".into();
    x1.workspace = "w4".into();
    elsewhere.blocks.push(x1);
    fake.server.publish(elsewhere.clone());
    // b7k2m moves from w3 to w4: a delete (and of what hangs on it); then back: a put.
    let mut moved = elsewhere.clone();
    moved.blocks[0].workspace = "w4".into();
    fake.server.publish(moved);
    let mut kinds = Vec::new();
    for _ in 0..3 {
        let event = stream.event();
        kinds.push((
            event.event,
            event.data["op"].as_str().unwrap().to_string(),
            event.data["id"].as_str().unwrap().to_string(),
        ));
    }
    assert_eq!(
        kinds,
        [
            (
                "block".to_string(),
                "delete".to_string(),
                "b7k2m".to_string()
            ),
            (
                "wire".to_string(),
                "delete".to_string(),
                "b7k2m-k9x1d".to_string()
            ),
            (
                "request".to_string(),
                "delete".to_string(),
                "r12".to_string()
            ),
        ],
        "nothing for x1, then what left with b7k2m"
    );
    fake.server.publish(elsewhere);
    let back = stream.event();
    assert_eq!(
        (
            back.event.as_str(),
            back.data["op"].as_str(),
            back.data["id"].as_str()
        ),
        ("block", Some("put"), Some("b7k2m"))
    );
    assert_eq!(back.data["value"]["workspace"], "w3");
    // Notices: w4's doesn't reach it, w3's does.
    fake.server.notice(Notice {
        text: "in w4".into(),
        block: None,
        workspace: Some("w4".into()),
    });
    fake.server.notice(Notice {
        text: "in w3".into(),
        block: Some("b7k2m".into()),
        workspace: Some("w3".into()),
    });
    loop {
        let event = stream.event();
        if event.event == "notice" {
            assert_eq!(event.data["text"], "in w3");
            break;
        }
    }
}

#[test]
fn a_ticket_opens_one_stream_once() {
    let (text, token) = token(Scope::Read, None);
    let fake = fake(vec![token.clone()], |options| {
        options.ticket_life = Duration::from_millis(400)
    });
    let ticket = |fake: &Fake| fake.call(req("POST", "/v1/tickets").token(&text).json(&json!({})));
    let made = ticket(&fake);
    assert_eq!(made.status, 200);
    let value = made.json["ticket"].as_str().unwrap().to_string();
    assert!(value.starts_with("tkt_") && value.len() == 36, "{value}");
    assert_eq!(made.json["expires_in"], 0, "400 ms is under a second");
    let stream = open(&fake, Auth::Ticket(&value), None);
    assert_eq!(stream.status, 200);
    assert_eq!(stream.event().event, "snapshot");
    let again = open(&fake, Auth::Ticket(&value), None);
    assert_eq!(again.status, 401, "a ticket is single use");
    // A ticket opens a stream and nothing else.
    let other = ticket(&fake).json["ticket"].as_str().unwrap().to_string();
    assert_eq!(
        fake.call(req("GET", &format!("/v1/state?ticket={other}")))
            .status,
        401
    );
    assert_eq!(
        fake.call(req("POST", &format!("/v1/tickets?ticket={other}")))
            .status,
        401
    );
    // The state call above didn't spend it, but time does.
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(
        open(&fake, Auth::Ticket(&other), None).status,
        401,
        "a stale ticket"
    );
    // A ticket needs a token to make it.
    assert_eq!(fake.call(req("POST", "/v1/tickets")).status, 401);
    // The default life is 30 seconds.
    let (_, token) = self::token(Scope::Read, None);
    let normal = self::fake(vec![token], |_| {});
    let (text, token) = self::token(Scope::Read, None);
    normal.server.set_tokens(vec![token]);
    assert_eq!(
        normal
            .call(req("POST", "/v1/tickets").token(&text).json(&json!({})))
            .json["expires_in"],
        30
    );
}

#[test]
fn resuming_gets_a_snapshot_and_the_missed_notices() {
    let (fake, text) = setup();
    let first = open(&fake, Auth::Bearer(&text), None);
    let x = first.event().id;
    drop(first);
    wait_for("the stream to close", || fake.server.streams() == 0);
    fake.server.notice(Notice {
        text: "t1 ended".into(),
        block: Some("b7k2m".into()),
        workspace: Some("w3".into()),
    });
    let back = open(&fake, Auth::Bearer(&text), Some(&x));
    assert_eq!(back.event().event, "snapshot");
    let notice = back.event();
    assert_eq!(
        (notice.event.as_str(), notice.data["text"].as_str()),
        ("notice", Some("t1 ended"))
    );
    // From another run: the snapshot alone, then only heartbeats.
    let strange = open(&fake, Auth::Bearer(&text), Some("00000000-5"));
    assert_eq!(strange.event().event, "snapshot");
    assert!(
        strange.ping(),
        "the next thing is a heartbeat, not a notice"
    );
    // A notice from before the id isn't sent again.
    let later = open(&fake, Auth::Bearer(&text), Some(&notice.id));
    assert_eq!(later.event().event, "snapshot");
    assert!(later.ping());
}

#[test]
fn a_heartbeat_comes_while_nothing_changes() {
    let (fake, text) = setup();
    let stream = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(stream.event().event, "snapshot");
    assert!(stream.ping() && stream.ping());
}

#[test]
fn a_stream_ends_when_its_tokens_workspaces_change() {
    let (text, token) = token(Scope::Read, Some(vec!["w3", "w4"]));
    let fake = fake(vec![token.clone()], |_| {});
    let stream = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(stream.event().event, "snapshot");
    // The same list again: the stream stays.
    fake.server.set_tokens(vec![token.clone()]);
    assert_eq!(fake.server.streams(), 1);
    let mut fewer = token;
    fewer.workspaces = Some(vec!["w3".into()]);
    fake.server.set_tokens(vec![fewer]);
    assert!(stream.ended(), "the body ends");
    wait_for("no stream", || fake.server.streams() == 0);
    assert_eq!(
        open(&fake, Auth::Bearer(&text), None).event().event,
        "snapshot"
    );
}

#[test]
fn revoking_cuts_the_stream_and_its_tickets() {
    let (text, token) = token(Scope::Read, None);
    let fake = fake(vec![token.clone()], |options| {
        options.heartbeat = Duration::from_millis(100)
    });
    let stream = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(stream.event().event, "snapshot");
    let ticket = fake.call(req("POST", "/v1/tickets").token(&text).json(&json!({})));
    let ticket = ticket.json["ticket"].as_str().unwrap().to_string();
    fake.server.set_tokens(vec![]);
    assert!(stream.ended(), "the body ends");
    wait_for("no stream", || fake.server.streams() == 0);
    assert_eq!(open(&fake, Auth::Ticket(&ticket), None).status, 401);
    // An expiry, with no set_tokens: the heartbeat finds it.
    let (text, mut token) = self::token(Scope::Read, None);
    token.expires = Some(now() + 2);
    fake.server.set_tokens(vec![token]);
    let stream = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(stream.event().event, "snapshot");
    assert!(stream.ended(), "its stream ends at its expiry");
    assert_eq!(fake.call(req("GET", "/v1/state").token(&text)).status, 401);
}

#[test]
fn an_expired_tokens_stream_ends_at_the_next_publish_not_the_next_heartbeat() {
    let (text, mut token) = token(Scope::Read, None);
    token.expires = Some(now() + 2);
    // A heartbeat a minute apart: only the state coming in can find the expiry in time.
    let fake = fake(vec![token], |options| {
        options.heartbeat = Duration::from_secs(60)
    });
    let stream = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(stream.event().event, "snapshot");
    let ticker = std::thread::scope(|scope| {
        let handle = scope.spawn(|| {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            while fake.server.streams() > 0 && std::time::Instant::now() < deadline {
                fake.server.publish(sample());
                std::thread::sleep(Duration::from_millis(100));
            }
            fake.server.streams()
        });
        assert!(stream.ended(), "its stream ended within the deadline");
        handle.join().unwrap()
    });
    assert_eq!(ticker, 0);
}

#[test]
fn eight_streams_per_token() {
    let (text, token) = token(Scope::Read, None);
    let (other_text, other) = self::token(Scope::Read, None);
    let fake = fake(vec![token, other], |options| {
        options.heartbeat = Duration::from_millis(100)
    });
    let streams: Vec<Stream> = (0..8)
        .map(|_| open(&fake, Auth::Bearer(&text), None))
        .collect();
    assert!(streams.iter().all(|s| s.status == 200));
    wait_for("8 streams", || fake.server.streams() == 8);
    let ninth = open(&fake, Auth::Bearer(&text), None);
    assert_eq!(ninth.status, 429);
    assert_eq!(
        open(&fake, Auth::Bearer(&other_text), None).status,
        200,
        "another token still opens one"
    );
    drop(streams);
    wait_for("room again", || fake.server.streams() <= 1);
    assert_eq!(open(&fake, Auth::Bearer(&text), None).status, 200);
}

#[test]
fn the_first_stream_asks_for_the_tick() {
    let (fake, text) = setup();
    assert!(
        fake.notifications(templum_api::WATCH).is_empty(),
        "nothing before a stream"
    );
    let first = open(&fake, Auth::Bearer(&text), None);
    first.event();
    wait_for("the watch", || {
        fake.notifications(templum_api::WATCH).len() == 1
    });
    let second = open(&fake, Auth::Bearer(&text), None);
    second.event();
    assert_eq!(
        fake.notifications(templum_api::WATCH).len(),
        1,
        "one tick serves them all"
    );
    drop((first, second));
    wait_for("no stream", || fake.server.streams() == 0);
    let third = open(&fake, Auth::Bearer(&text), None);
    third.event();
    wait_for("the watch again", || {
        fake.notifications(templum_api::WATCH).len() == 2
    });
}
