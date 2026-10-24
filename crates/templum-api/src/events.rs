//! The event stream (docs/specs/api.md, Events): each open `GET /v1/events` has a queue of frames
//! of its own, a view of the state it last saw, and its token's caller. The server compares each
//! new state with that view and queues what differs; a client that reads too slowly loses its
//! queue and gets a new snapshot, so memory stays bounded.

use crate::model::{Caller, Delta, Notice, State, diff};
use bytes::Bytes;
use hyper::body::{Body, Frame};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

/// Frames queued for one stream before it is declared behind.
pub(crate) const QUEUE: usize = 256;
/// Notices kept for a client that reconnects, and how long.
const NOTICES: usize = 256;
const NOTICE_LIFE: Duration = Duration::from_secs(60 * 60);
/// Streams one token may hold open.
pub(crate) const STREAMS_PER_TOKEN: usize = 8;

/// An SSE event: `id`, `event` and `data` on one line each, then a blank line.
pub(crate) fn frame(id: &str, event: &str, data: &Value) -> Bytes {
    Bytes::from(format!("id: {id}\nevent: {event}\ndata: {data}\n\n"))
}

/// What keeps a stream alive through a proxy's idle timeout.
pub(crate) fn ping() -> Bytes {
    Bytes::from_static(b": ping\n\n")
}

/// Event ids: `<run>-<n>`. `run` is random at each start of the server, so a client that comes
/// back after a restart is told to take the snapshot alone.
pub(crate) struct Ids {
    pub run: String,
    n: AtomicU64,
}

impl Ids {
    pub fn new() -> Ids {
        // Eight hex digits from the system's generator: the front of a hash of random text.
        let run = crate::store::hash(&crate::store::random_text("", 8))[..8].to_string();
        Ids {
            run,
            n: AtomicU64::new(1),
        }
    }

    pub fn next(&self) -> (u64, String) {
        let n = self.n.fetch_add(1, Ordering::Relaxed);
        (n, format!("{}-{n}", self.run))
    }

    /// The `n` of an id of this run; None for another run's or a malformed one.
    pub fn parse(&self, id: &str) -> Option<u64> {
        id.strip_prefix(self.run.as_str())?
            .strip_prefix('-')?
            .parse()
            .ok()
    }
}

/// The frames waiting for one client. Past [`QUEUE`] it drops them all and is `behind` until a
/// snapshot replaces them.
#[derive(Default)]
pub(crate) struct Queue {
    frames: VecDeque<Bytes>,
    pub behind: bool,
}

impl Queue {
    pub fn push(&mut self, frame: Bytes) {
        if self.behind {
            return;
        }
        if self.frames.len() >= QUEUE {
            self.frames.clear();
            self.behind = true;
            return;
        }
        self.frames.push_back(frame);
    }

    pub fn take(&mut self) -> Option<Bytes> {
        self.frames.pop_front()
    }

    /// The client caught up: only `snapshot` is queued, and it is no longer behind.
    pub fn resync(&mut self, snapshot: Bytes) {
        self.frames.clear();
        self.frames.push_back(snapshot);
        self.behind = false;
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.frames.len()
    }
}

#[derive(Default)]
struct CellInner {
    queue: Queue,
    waker: Option<Waker>,
    ended: bool,
}

/// A stream's queue, shared by the hub that fills it and the body that sends it.
#[derive(Default)]
pub(crate) struct Cell(Mutex<CellInner>);

impl Cell {
    fn with(&self, f: impl FnOnce(&mut CellInner)) {
        let waker = {
            let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut inner);
            inner.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub fn push(&self, frame: Bytes) {
        self.with(|c| c.queue.push(frame));
    }

    pub fn resync(&self, snapshot: Bytes) {
        self.with(|c| c.queue.resync(snapshot));
    }

    /// The stream is over: its body finishes at once, whatever is queued.
    pub fn end(&self) {
        self.with(|c| {
            c.queue = Queue::default();
            c.ended = true;
        });
    }

    pub fn behind(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .queue
            .behind
    }

    fn poll(&self, cx: &mut Context<'_>) -> Poll<Option<Bytes>> {
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(frame) = inner.queue.take() {
            return Poll::Ready(Some(frame));
        }
        if inner.ended {
            return Poll::Ready(None);
        }
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }

    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.0.lock().unwrap().queue.len()
    }
}

/// The response body of a stream: its frames as they come. Dropping it (the client left, or the
/// stream ended) runs `on_drop`, which takes the stream out of the hub.
pub(crate) struct StreamBody {
    cell: Arc<Cell>,
    on_drop: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl StreamBody {
    pub fn new(cell: Arc<Cell>, on_drop: impl FnOnce() + Send + Sync + 'static) -> StreamBody {
        StreamBody {
            cell,
            on_drop: Some(Box::new(on_drop)),
        }
    }
}

impl Body for StreamBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        self.cell
            .poll(cx)
            .map(|frame| frame.map(|bytes| Ok(Frame::data(bytes))))
    }
}

impl Drop for StreamBody {
    fn drop(&mut self) {
        if let Some(on_drop) = self.on_drop.take() {
            on_drop();
        }
    }
}

/// One open stream.
pub(crate) struct Entry {
    pub cell: Arc<Cell>,
    pub token: u64,
    pub caller: Caller,
    /// The state as this stream's caller last saw it.
    pub last: Arc<State>,
    pub conn: u64,
}

struct Kept {
    n: u64,
    at: Instant,
    notice: Notice,
    frame: Bytes,
}

#[derive(Default)]
pub(crate) struct Hub {
    /// How many streams were ever opened: a state already published is new to a stream opened since.
    pub opened: u64,
    streams: HashMap<u64, Entry>,
    next_stream: u64,
    notices: VecDeque<Kept>,
}

/// A stream the hub took in: its id and its queue, which already holds the snapshot.
pub(crate) struct Opened {
    pub id: u64,
    pub cell: Arc<Cell>,
    /// It is the only one open: the UI thread's tick must start.
    pub first: bool,
}

impl Hub {
    pub fn len(&self) -> usize {
        self.streams.len()
    }

    pub fn of_token(&self, token: u64) -> usize {
        self.streams.values().filter(|e| e.token == token).count()
    }

    /// Takes in a stream whose first frames are `snapshot` and then the kept notices after
    /// `after` its caller sees. Err when its token holds too many already.
    pub fn open(
        &mut self,
        caller: Caller,
        view: State,
        conn: u64,
        snapshot: Bytes,
        after: Option<u64>,
        now: Instant,
    ) -> Result<Opened, ()> {
        if self.of_token(caller.token) >= STREAMS_PER_TOKEN {
            return Err(());
        }
        self.expire(now);
        let cell = Arc::new(Cell::default());
        cell.push(snapshot);
        if let Some(after) = after {
            for kept in self
                .notices
                .iter()
                .filter(|k| k.n > after && caller.sees(k.notice.workspace.as_deref()))
            {
                cell.push(kept.frame.clone());
            }
        }
        let id = self.next_stream;
        self.next_stream += 1;
        self.opened += 1;
        let first = self.streams.is_empty();
        let entry = Entry {
            cell: cell.clone(),
            token: caller.token,
            caller,
            last: Arc::new(view),
            conn,
        };
        self.streams.insert(id, entry);
        Ok(Opened { id, cell, first })
    }

    /// The stream is over, whoever ended it.
    pub fn close(&mut self, id: u64) -> Option<Entry> {
        let entry = self.streams.remove(&id)?;
        entry.cell.end();
        Some(entry)
    }

    /// Ends the streams whose caller `dead` says so of; the connections they were on.
    pub fn cut(&mut self, dead: impl Fn(&Caller) -> bool) {
        let gone: Vec<u64> = self
            .streams
            .iter()
            .filter(|(_, e)| dead(&e.caller))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.close(id);
        }
    }

    /// The streams on connection `conn`.
    pub fn on_conn(&self, conn: u64) -> bool {
        self.streams.values().any(|e| e.conn == conn)
    }

    pub fn ping(&self) {
        for entry in self.streams.values() {
            entry.cell.push(ping());
        }
    }

    fn expire(&mut self, now: Instant) {
        while self
            .notices
            .front()
            .is_some_and(|k| now.duration_since(k.at) > NOTICE_LIFE)
        {
            self.notices.pop_front();
        }
        while self.notices.len() > NOTICES {
            self.notices.pop_front();
        }
    }

    /// Keeps a notice and queues it on every stream whose caller sees its workspace.
    pub fn notice(&mut self, ids: &Ids, notice: Notice, now: Instant) {
        let (n, id) = ids.next();
        let frame = frame(
            &id,
            "notice",
            &serde_json::to_value(&notice).unwrap_or(Value::Null),
        );
        for entry in self
            .streams
            .values()
            .filter(|e| e.caller.sees(notice.workspace.as_deref()))
        {
            entry.cell.push(frame.clone());
        }
        self.notices.push_back(Kept {
            n,
            at: now,
            notice,
            frame,
        });
        self.expire(now);
    }
}

/// One change as an event: a `put` carries the object whole, a `delete` its id.
fn delta_frame(id: &str, delta: Delta) -> Bytes {
    let data = match delta.value {
        Some(value) => json!({ "op": "put", "id": delta.id, "value": value }),
        None => json!({ "op": "delete", "id": delta.id }),
    };
    frame(id, delta.kind, &data)
}

/// A snapshot event of a view.
pub(crate) fn snapshot(ids: &Ids, view: &State) -> Bytes {
    frame(
        &ids.next().1,
        "snapshot",
        &serde_json::to_value(view).unwrap_or(Value::Null),
    )
}

/// Hands states to the streams, skipping one equal to the last handed over (a person who is
/// not moving anything) unless a stream opened since, which has not seen it.
#[derive(Default)]
pub(crate) struct Publisher {
    last: Option<(State, u64)>,
}

impl Publisher {
    /// Whether it compared: false for a state the streams have all seen.
    pub fn publish(&mut self, hub: &Mutex<Hub>, ids: &Ids, state: State) -> bool {
        let opened = hub.lock().unwrap_or_else(|e| e.into_inner()).opened;
        if self
            .last
            .as_ref()
            .is_some_and(|(last, at)| *at == opened && *last == state)
        {
            return false;
        }
        publish(hub, ids, &state);
        self.last = Some((state, opened));
        true
    }
}

/// Compares `state` with what each stream last saw and queues the differences. Streams of one
/// kind of caller (the same workspaces) share the view of the state, and those that last saw the
/// same share the comparison. It runs without the hub's lock: the lock is taken to copy what each
/// stream needs and again to put the frames in, so the UI thread (which sends notices) never waits
/// for a comparison.
pub(crate) fn publish(hub: &Mutex<Hub>, ids: &Ids, state: &State) {
    let lock = || hub.lock().unwrap_or_else(|e| e.into_inner());
    let work: Vec<(u64, Caller, Arc<State>, bool)> = lock()
        .streams
        .iter()
        .map(|(id, e)| (*id, e.caller.clone(), e.last.clone(), e.cell.behind()))
        .collect();
    type Key = Option<Vec<String>>;
    enum Out {
        Snapshot(Bytes),
        Deltas(Vec<Bytes>),
    }
    let mut views: HashMap<Key, Arc<State>> = HashMap::new();
    let mut snapshots: HashMap<Key, Bytes> = HashMap::new();
    let mut diffs: Vec<(Key, Arc<State>, Vec<Bytes>)> = Vec::new();
    let mut done: Vec<(u64, Arc<State>, Out)> = Vec::new();
    for (id, caller, last, behind) in work {
        let key = caller.workspaces.clone();
        let view = views
            .entry(key.clone())
            .or_insert_with(|| Arc::new(state.visible(&caller)))
            .clone();
        let out = if behind {
            let frame = snapshots
                .entry(key)
                .or_insert_with(|| snapshot(ids, &view))
                .clone();
            Out::Snapshot(frame)
        } else if let Some((_, _, frames)) = diffs
            .iter()
            .find(|(k, l, _)| *k == key && (Arc::ptr_eq(l, &last) || **l == *last))
        {
            Out::Deltas(frames.clone())
        } else {
            let frames: Vec<Bytes> = diff(&last, &view)
                .into_iter()
                .map(|d| delta_frame(&ids.next().1, d))
                .collect();
            diffs.push((key, last, frames.clone()));
            Out::Deltas(frames)
        };
        done.push((id, view, out));
    }
    let mut hub = lock();
    for (id, view, out) in done {
        let Some(entry) = hub.streams.get_mut(&id) else {
            continue;
        };
        entry.last = view;
        match out {
            Out::Snapshot(frame) => entry.cell.resync(frame),
            Out::Deltas(frames) => frames.into_iter().for_each(|f| entry.cell.push(f)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BlockView;
    use crate::store::Scope;

    #[test]
    fn a_slow_stream_is_resynced_not_grown() {
        let mut q = Queue::default();
        for i in 0..300 {
            q.push(Bytes::from(format!("{i}")));
        }
        assert!(q.behind && q.len() == 0);
        q.push(Bytes::from("ignored while behind"));
        assert_eq!(q.len(), 0);
        q.resync(Bytes::from("snapshot"));
        assert_eq!((q.take(), q.take()), (Some(Bytes::from("snapshot")), None));
        assert!(!q.behind);
    }

    #[test]
    fn a_queue_holds_exactly_its_size() {
        let mut q = Queue::default();
        for _ in 0..QUEUE {
            q.push(Bytes::from("x"));
        }
        assert!(!q.behind && q.len() == QUEUE);
        q.push(Bytes::from("x"));
        assert!(q.behind);
    }

    #[test]
    fn a_frame_is_one_sse_event() {
        let text = frame("ab-1", "block", &json!({"op": "delete", "id": "b1"}));
        let text = String::from_utf8(text.to_vec()).unwrap();
        // Whatever order the JSON keeps its keys in (a feature of another crate may change it).
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(&lines[..2], ["id: ab-1", "event: block"]);
        let data: serde_json::Value =
            serde_json::from_str(lines[2].strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(data, json!({"op": "delete", "id": "b1"}));
        assert_eq!(&lines[3..], ["", ""], "a blank line ends the event");
    }

    #[test]
    fn ids_belong_to_a_run() {
        let ids = Ids::new();
        assert_eq!(ids.run.len(), 8);
        assert!(ids.run.chars().all(|c| c.is_ascii_hexdigit()));
        let (n, id) = ids.next();
        assert_eq!(ids.parse(&id), Some(n));
        assert_eq!(
            ids.parse("00000000-5"),
            if ids.run == "00000000" { Some(5) } else { None }
        );
        assert_eq!(ids.parse(&format!("{}-x", ids.run)), None);
        assert!(ids.next().0 > n);
    }

    fn caller(token: u64, workspaces: Option<&[&str]>) -> Caller {
        Caller {
            token,
            name: "t".into(),
            scope: Scope::Read,
            workspaces: workspaces.map(|w| w.iter().map(|s| s.to_string()).collect()),
        }
    }

    fn text(bytes: Bytes) -> String {
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn a_hub_keeps_notices_for_the_streams_that_see_them_and_for_a_return() {
        let (ids, mut hub, now) = (Ids::new(), Hub::default(), Instant::now());
        let open = |hub: &mut Hub, c: Caller, after: Option<u64>| {
            hub.open(c, State::default(), 1, Bytes::from("snap"), after, now)
                .unwrap()
        };
        let all = open(&mut hub, caller(1, None), None);
        let w3 = open(&mut hub, caller(2, Some(&["w3"])), None);
        assert!(all.first && !w3.first);
        hub.notice(
            &ids,
            Notice {
                text: "in w4".into(),
                block: None,
                workspace: Some("w4".into()),
            },
            now,
        );
        hub.notice(
            &ids,
            Notice {
                text: "in w3".into(),
                block: None,
                workspace: Some("w3".into()),
            },
            now,
        );
        assert_eq!((all.cell.queued(), w3.cell.queued()), (3, 2));
        // A stream that comes back after notice n gets what followed it, if it sees them.
        let first_n = 1;
        let back = open(&mut hub, caller(2, Some(&["w3"])), Some(first_n));
        assert_eq!(back.cell.queued(), 2, "the snapshot and the notice of w3");
        let none = open(&mut hub, caller(3, Some(&["w3"])), Some(99));
        assert_eq!(none.cell.queued(), 1, "nothing after 99");
        assert!(text(Bytes::from(format!("{}", hub.len()))) == "4");
    }

    #[test]
    fn notices_expire_and_are_capped() {
        let (ids, mut hub, now) = (Ids::new(), Hub::default(), Instant::now());
        for i in 0..300 {
            hub.notice(
                &ids,
                Notice {
                    text: i.to_string(),
                    ..Default::default()
                },
                now,
            );
        }
        assert_eq!(hub.notices.len(), NOTICES);
        let later = now + NOTICE_LIFE + Duration::from_secs(1);
        let opened = hub
            .open(
                caller(1, None),
                State::default(),
                1,
                Bytes::from("s"),
                Some(0),
                later,
            )
            .unwrap();
        assert_eq!(opened.cell.queued(), 1, "an hour on, none is kept");
    }

    #[test]
    fn a_token_holds_eight_streams() {
        let (mut hub, now) = (Hub::default(), Instant::now());
        for _ in 0..STREAMS_PER_TOKEN {
            hub.open(
                caller(1, None),
                State::default(),
                1,
                Bytes::new(),
                None,
                now,
            )
            .unwrap();
        }
        assert!(
            hub.open(
                caller(1, None),
                State::default(),
                1,
                Bytes::new(),
                None,
                now
            )
            .is_err()
        );
        assert!(
            hub.open(
                caller(2, None),
                State::default(),
                1,
                Bytes::new(),
                None,
                now
            )
            .is_ok()
        );
        hub.cut(|caller| caller.token == 1);
        assert_eq!((hub.len(), hub.of_token(1)), (1, 0));
    }

    #[test]
    fn publish_queues_the_differences_and_resyncs_a_stream_that_is_behind() {
        let (ids, hub) = (Ids::new(), Mutex::new(Hub::default()));
        let mut old = State::default();
        old.blocks.push(BlockView {
            id: "b1".into(),
            workspace: "w3".into(),
            ..Default::default()
        });
        let (a, b) = {
            let mut h = hub.lock().unwrap();
            let now = Instant::now();
            let a = h
                .open(caller(1, None), old.clone(), 1, Bytes::new(), None, now)
                .unwrap();
            let b = h
                .open(caller(2, None), old.clone(), 2, Bytes::new(), None, now)
                .unwrap();
            (a, b)
        };
        let _ = (
            a.cell.0.lock().unwrap().queue.take(),
            b.cell.0.lock().unwrap().queue.take(),
        );
        let mut new = old.clone();
        new.blocks[0].title = "moved".into();
        // `b` is far behind.
        for _ in 0..=QUEUE {
            b.cell.push(Bytes::from("x"));
        }
        assert!(b.cell.behind());
        publish(&hub, &ids, &new);
        assert_eq!(a.cell.queued(), 1);
        let first = text(a.cell.0.lock().unwrap().queue.take().unwrap());
        assert!(
            first.contains("event: block\n") && first.contains("\"op\":\"put\""),
            "{first}"
        );
        assert!(!b.cell.behind() && b.cell.queued() == 1);
        let snapshot = text(b.cell.0.lock().unwrap().queue.take().unwrap());
        assert!(
            snapshot.contains("event: snapshot\n") && snapshot.contains("moved"),
            "{snapshot}"
        );
        // The same state again: nothing.
        publish(&hub, &ids, &new);
        assert_eq!((a.cell.queued(), b.cell.queued()), (0, 0));
    }

    #[test]
    fn streams_of_one_caller_share_the_view_and_the_comparison() {
        let (ids, hub) = (Ids::new(), Mutex::new(Hub::default()));
        let mut old = State::default();
        old.blocks.push(BlockView {
            id: "b1".into(),
            ..Default::default()
        });
        let now = Instant::now();
        let opened: Vec<Opened> = {
            let mut h = hub.lock().unwrap();
            [None, None, Some(&["w3"][..])]
                .into_iter()
                .enumerate()
                .map(|(i, workspaces)| {
                    h.open(
                        caller(i as u64 + 1, workspaces),
                        old.clone(),
                        1,
                        Bytes::new(),
                        None,
                        now,
                    )
                    .unwrap()
                })
                .collect()
        };
        let mut new = old.clone();
        new.blocks[0].title = "moved".into();
        publish(&hub, &ids, &new);
        let frame = |o: &Opened| o.cell.0.lock().unwrap().queue.take().unwrap();
        let (a, b) = (frame(&opened[0]), frame(&opened[1]));
        assert_eq!(a, b, "one comparison, one frame (one id) for both");
        let h = hub.lock().unwrap();
        let lasts: Vec<_> = h
            .streams
            .values()
            .map(|e| (e.token, e.last.clone()))
            .collect();
        let last = |token| lasts.iter().find(|(t, _)| *t == token).unwrap().1.clone();
        assert!(Arc::ptr_eq(&last(1), &last(2)), "one view");
        assert!(
            !Arc::ptr_eq(&last(1), &last(3)),
            "another caller's is its own"
        );
    }

    #[test]
    fn a_state_the_streams_have_seen_is_not_compared_again() {
        let (ids, hub) = (Ids::new(), Mutex::new(Hub::default()));
        let mut publisher = Publisher::default();
        let mut state = State::default();
        state.blocks.push(BlockView {
            id: "b1".into(),
            ..Default::default()
        });
        assert!(publisher.publish(&hub, &ids, state.clone()), "the first");
        assert!(
            !publisher.publish(&hub, &ids, state.clone()),
            "the same again"
        );
        let mut moved = state.clone();
        moved.blocks[0].title = "x".into();
        assert!(publisher.publish(&hub, &ids, moved.clone()));
        // A stream that opened since hasn't seen it, whatever its snapshot said.
        hub.lock()
            .unwrap()
            .open(
                caller(1, None),
                State::default(),
                1,
                Bytes::new(),
                None,
                Instant::now(),
            )
            .unwrap();
        assert!(publisher.publish(&hub, &ids, moved.clone()));
        assert!(!publisher.publish(&hub, &ids, moved));
    }

    #[test]
    fn ending_a_stream_ends_its_body() {
        let cell = Arc::new(Cell::default());
        cell.push(Bytes::from("x"));
        let dropped = Arc::new(AtomicU64::new(0));
        let counter = dropped.clone();
        let mut body = StreamBody::new(cell.clone(), move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let mut cx = Context::from_waker(Waker::noop());
        let next = |body: &mut StreamBody, cx: &mut Context| Pin::new(body).poll_frame(cx);
        assert!(matches!(next(&mut body, &mut cx), Poll::Ready(Some(Ok(_)))));
        assert!(matches!(next(&mut body, &mut cx), Poll::Pending));
        cell.push(Bytes::from("y"));
        cell.end();
        assert!(
            matches!(next(&mut body, &mut cx), Poll::Ready(None)),
            "queued frames go with it"
        );
        drop(body);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}
