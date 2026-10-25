//! One remote's life (docs/specs/templum-to-templum.md, When it isn't live): a coordinator task
//! per remote that reacts to messages and never awaits the network itself. Its sub-tasks do:
//! the calls connection (`GET /v1/me`, moves, answers), the event stream, the typing connection
//! and one stream per terminal. They are aborted with the attempt that started them, so a
//! reconnect leaves no old connection behind.

use super::dial::{Place, Proxy, is_loopback, place};
use super::frames::Painter;
use super::http::{Conn, Headers, Task, stream};
use super::sse;
use super::trust::{self, Pinned};
use super::{
    About, Bad, Command, Failure, KeySource, Keys, Limits, Order, Remote, Shared, Status, Update,
    Why,
};
use crate::code::Trust;
use crate::model::{ApplyError, FrameView, MeView};
use crate::registry::INPUT_KEYS;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use templum_mcp::tools::MAX_TEXT;
use templum_terminal::Feeder;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_rustls::rustls::ClientConfig;

/// At most this many terminals stream at once.
const STREAMS_MOST: usize = 12;
/// At most this many keys wait, per terminal; past it the new ones are dropped.
const WAITING_MOST: usize = 1024;
/// At most this many events wait for the coordinator: past it the event stream isn't read, so a
/// home that writes faster than the state is kept is slowed by its own socket, not by memory.
const EVENTS_WAITING_MOST: usize = 64;
/// A terminal stream that ended is opened again after this long.
const REOPEN: Duration = Duration::from_secs(1);
/// A call of the typing connection starts no sooner than this after the previous one did, so
/// keys typed fast batch (about 16 calls a second, under what home's bucket refills).
const PACE: Duration = Duration::from_millis(60);
/// A call home answered "busy" this many times in a row is given up.
const BUSY_TRIES: u32 = 10;

/// `min(30 s, 1 s × 2^(attempt−1)) × (1 + jitter)`, `jitter` in `[-0.2, 0.2]`.
pub(crate) fn backoff(attempt: u32, jitter: f64) -> Duration {
    let base = 1u64 << attempt.saturating_sub(1).min(5);
    Duration::from_secs_f64(base.min(30) as f64 * (1.0 + jitter))
}

/// A random number in `[-0.2, 0.2]`.
fn jitter() -> f64 {
    let mut bytes = [0u8; 2];
    let _ = SystemRandom::new().fill(&mut bytes);
    f64::from(u16::from_le_bytes(bytes)) / 65535.0 * 0.4 - 0.2
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Whether the way in fits the address: plain HTTP only to this computer itself, TLS only to
/// anything else. The key never goes in clear off this computer, and no "TLS" nobody checks.
fn fits(trust: &Trust, place: &Place) -> bool {
    match trust {
        Trust::Loopback => !place.https && is_loopback(&place.host),
        Trust::Pinned(_) | Trust::Public => place.https,
    }
}

/// What the coordinator is told: by the UI thread, and by its sub-tasks (each with the attempt
/// it belongs to; those of an ended attempt are ignored).
pub(crate) enum Msg {
    Command(Command),
    Forget {
        revoke: bool,
    },
    Me {
        epoch: u64,
        result: Result<(MeView, Duration), Failure>,
    },
    Opened {
        epoch: u64,
    },
    Event {
        epoch: u64,
        event: sse::Event,
        /// The event's place among those that wait: given back when it is handled or dropped.
        _room: OwnedSemaphorePermit,
    },
    /// The attempt failed. `quiet`: a live stream ended on home's side.
    Lost {
        epoch: u64,
        failure: Failure,
        quiet: bool,
    },
    Called {
        epoch: u64,
        about: About,
        result: Result<Value, Failure>,
        took: Duration,
    },
    Typed {
        epoch: u64,
        block: String,
        result: Result<Value, Failure>,
        took: Duration,
    },
    /// A terminal's block is gone or isn't a terminal: no stream until the set changes.
    Stopped {
        epoch: u64,
        block: String,
    },
    /// The backoff ran out.
    Retry {
        epoch: u64,
    },
}

/// The thread: a runtime, and one coordinator task per remote.
pub(crate) fn run(
    keys: Keys,
    out: Arc<Shared>,
    limits: Limits,
    mut orders: UnboundedReceiver<Order>,
) {
    let Ok(runtime) = Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
    else {
        return;
    };
    runtime.block_on(async {
        let mut remotes: HashMap<String, (UnboundedSender<Msg>, tokio::task::JoinHandle<()>)> =
            HashMap::new();
        while let Some(order) = orders.recv().await {
            match order {
                Order::Connect { id, round, remote } => {
                    if let Some((_, old)) = remotes.remove(&id) {
                        old.abort();
                    }
                    let (tx, rx) = unbounded_channel();
                    let life = Life::new(
                        &id,
                        round,
                        *remote,
                        keys.clone(),
                        out.clone(),
                        limits,
                        tx.clone(),
                    );
                    remotes.insert(id, (tx, tokio::spawn(life.run(rx))));
                }
                Order::Send(id, msg) => {
                    if let Some((tx, _)) = remotes.get(&id) {
                        let _ = tx.send(msg);
                    }
                }
                // The task finishes by itself (it revokes, deletes the key, ends).
                Order::Forget(id, revoke) => {
                    if let Some((tx, _)) = remotes.remove(&id) {
                        let _ = tx.send(Msg::Forget { revoke });
                    }
                }
                Order::Probe => {
                    let (keys, out) = (keys.clone(), out.clone());
                    tokio::task::spawn_blocking(move || {
                        out.push("", 0, Update::Keyring(keys.answers()));
                    });
                }
            }
        }
    });
    runtime.shutdown_background();
}

/// What a connection needs, new for each attempt (its session is).
struct Net {
    place: Place,
    tls: Option<Arc<ClientConfig>>,
    proxy: Option<Proxy>,
    headers: Headers,
}

/// What an attempt is made of: dropping it ends every task and connection.
struct Attempt {
    net: Arc<Net>,
    calls: UnboundedSender<Call>,
    typing: Option<UnboundedSender<Batch>>,
    events_started: bool,
    _tasks: Vec<Task>,
}

struct Call {
    about: About,
    tool: &'static str,
    args: Value,
}

struct Batch {
    block: String,
    args: Value,
}

/// What one remote is ready with once its key is loaded.
struct Ready {
    place: Place,
    tls: Option<Arc<ClientConfig>>,
    /// The public verifier, to forget its verdicts when the person presses Try now.
    checked: Option<Arc<trust::Checked>>,
    proxy: Option<Proxy>,
    secrets: super::Secrets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Disconnected or cut: nothing runs.
    Idle,
    Opening,
    Live,
    /// Waiting out the backoff.
    Waiting,
}

/// One waiting piece of typing.
enum Item {
    Key(String),
    Paste(String),
}

#[derive(Default)]
struct Queue {
    items: VecDeque<Item>,
    keys: usize,
    paste_bytes: usize,
    /// `Dropped` was said and the queue hasn't drained since: not said again.
    dropped: bool,
}

struct Life {
    id: String,
    round: u64,
    remote: Remote,
    keys: Keys,
    out: Arc<Shared>,
    limits: Limits,
    tx: UnboundedSender<Msg>,
    ready: Option<Ready>,
    phase: Phase,
    /// The last status pushed, so a repeat isn't.
    status: Option<Status>,
    /// Which attempt is current: messages of another are ignored.
    epoch: u64,
    failures: u32,
    rechecking: bool,
    last_id: Option<String>,
    me: Option<MeView>,
    last_live: Option<SystemTime>,
    live_since: Option<Instant>,
    attempt: Option<Attempt>,
    timer: Option<Task>,
    wanted: Vec<(String, Feeder)>,
    blocked: HashSet<String>,
    /// Each open stream, and the viewer it feeds.
    streams: HashMap<String, (Feeder, Task)>,
    queues: HashMap<String, Queue>,
    /// The blocks with something waiting, oldest first.
    turns: VecDeque<String>,
    typing_busy: bool,
    /// The blocks typed into while not live that were told so, until live again.
    refused: HashSet<String>,
}

impl Life {
    fn new(
        id: &str,
        round: u64,
        remote: Remote,
        keys: Keys,
        out: Arc<Shared>,
        limits: Limits,
        tx: UnboundedSender<Msg>,
    ) -> Life {
        Life {
            id: id.into(),
            round,
            remote,
            keys,
            out,
            limits,
            tx,
            ready: None,
            phase: Phase::Idle,
            status: None,
            epoch: 0,
            failures: 0,
            rechecking: false,
            last_id: None,
            me: None,
            last_live: None,
            live_since: None,
            attempt: None,
            timer: None,
            wanted: Vec::new(),
            blocked: HashSet::new(),
            streams: HashMap::new(),
            queues: HashMap::new(),
            turns: VecDeque::new(),
            typing_busy: false,
            refused: HashSet::new(),
        }
    }

    async fn run(mut self, mut inbox: UnboundedReceiver<Msg>) {
        self.start().await;
        while let Some(msg) = inbox.recv().await {
            if !self.handle(msg).await {
                return;
            }
        }
    }

    fn say(&self, update: Update) {
        self.out.push(&self.id, self.round, update);
    }

    fn status(&mut self, status: Status) {
        if self.status.as_ref() != Some(&status) {
            self.status = Some(status.clone());
            self.say(Update::Status(status));
        }
    }

    /// Loads the key and builds the verifier, then makes the first attempt.
    async fn start(&mut self) {
        self.status(Status::Connecting);
        match self.prepare().await {
            Ok(()) => self.begin(),
            Err(why) => self.cut(why),
        }
    }

    async fn prepare(&mut self) -> Result<(), Why> {
        if self.ready.is_some() {
            return Ok(());
        }
        let secrets = match self.remote.key.clone() {
            KeySource::Memory(secrets) => secrets,
            KeySource::Save {
                entry,
                secrets,
                file,
            } => {
                let (keys, kept) = (self.keys.clone(), secrets.clone());
                let saved = blocking(move || keys.save(&entry, &kept, file))
                    .await
                    .unwrap_or_else(|| Err("Templum couldn't keep this connection's key.".into()));
                match saved {
                    Ok(kept) => self.say(Update::Kept(kept)),
                    Err(sentence) => {
                        self.say(Update::Refused {
                            about: About::Show,
                            sentence,
                        });
                        return Err(Why::Keyring);
                    }
                }
                secrets
            }
            KeySource::Kept { entry, file } => {
                let keys = self.keys.clone();
                blocking(move || keys.load(&entry, file))
                    .await
                    .and_then(Result::ok)
                    .ok_or(Why::Keyring)?
            }
        };
        if !self.remote.binds(&secrets) {
            self.say(Update::Refused {
                about: About::Show,
                sentence:
                    "This connection's address or way in changed outside Templum, so its key \
                           isn't sent. Connect again with a new code."
                        .into(),
            });
            return Err(Why::Keyring);
        }
        let place = place(&self.remote.url).ok_or_else(|| Why::NotTemplum {
            status: 0,
            kind: "no address".into(),
        })?;
        if !fits(&self.remote.trust, &place) {
            self.say(Update::Refused {
                about: About::Show,
                sentence: "The address and the way it is reached don't agree, so the key isn't \
                           sent."
                    .into(),
            });
            return Err(Why::NotTemplum {
                status: 0,
                kind: "an address that doesn't fit how it is reached".into(),
            });
        }
        let mut checked = None;
        let tls = match &self.remote.trust {
            Trust::Loopback => None,
            Trust::Pinned(pin) => {
                let pinned = Pinned::new(pin).ok_or(Why::Certificate(Bad::Other))?;
                Some(trust::config(Arc::new(pinned)))
            }
            Trust::Public => {
                let (system, public) = blocking(trust::web)
                    .await
                    .and_then(Result::ok)
                    .ok_or(Why::Certificate(Bad::Other))?;
                let verifier =
                    Arc::new(trust::Checked::new(system, public, self.remote.trust_added));
                checked = Some(verifier.clone());
                Some(trust::config(verifier))
            }
        };
        let proxy = match &self.remote.proxy {
            // This computer is never reached through a proxy.
            _ if self.remote.trust == Trust::Loopback => None,
            None => None,
            Some(address) => Some(
                Proxy::new(address, secrets.proxy_auth.as_deref()).ok_or_else(|| Why::Proxy {
                    proxy: address.clone(),
                    status: 0,
                })?,
            ),
        };
        self.ready = Some(Ready {
            place,
            tls,
            checked,
            proxy,
            secrets,
        });
        Ok(())
    }

    /// Everything of the old attempt dropped: its tasks, connections, streams and waiting keys.
    /// What its tasks already said is ignored from now on: the epoch moves.
    fn drop_attempt(&mut self) {
        self.epoch += 1;
        self.attempt = None;
        self.timer = None;
        self.streams.clear();
        self.queues.clear();
        self.turns.clear();
        self.typing_busy = false;
    }

    /// A new attempt, with a session of its own: the calls task opens its connection and asks
    /// `GET /v1/me`.
    fn begin(&mut self) {
        self.drop_attempt();
        self.phase = Phase::Opening;
        self.live_since = None;
        let Some(ready) = &self.ready else { return };
        let net = Arc::new(Net {
            place: ready.place.clone(),
            tls: ready.tls.clone(),
            proxy: ready.proxy.clone(),
            headers: Headers {
                key: ready.secrets.token.clone(),
                access: self
                    .remote
                    .access_id
                    .clone()
                    .zip(ready.secrets.access_secret.clone()),
                session: crate::store::random_text("", 12),
                host: ready.place.host_header(),
            },
        });
        let (calls, inbox) = unbounded_channel();
        let task = Task::spawn(calls_task(
            net.clone(),
            self.epoch,
            inbox,
            self.tx.clone(),
            self.limits.me_every,
        ));
        self.attempt = Some(Attempt {
            net,
            calls,
            typing: None,
            events_started: false,
            _tasks: vec![task],
        });
    }

    /// Stops for good, until the person tries again.
    fn cut(&mut self, why: Why) {
        self.drop_attempt();
        self.phase = Phase::Idle;
        self.live_since = None;
        self.status(Status::Cut(why));
    }

    fn lost(&mut self, failure: Failure, quiet: bool) {
        match failure {
            Failure::Auth => {
                let ended = self
                    .me
                    .as_ref()
                    .and_then(|me| me.expires)
                    .is_some_and(|end| unix_now() >= end);
                self.cut(if ended { Why::Expired } else { Why::Revoked });
            }
            Failure::Stop(why) => self.cut(why),
            Failure::Busy(wait) => self.retry(wait, false),
            Failure::Down(_) | Failure::Missing(_) | Failure::Refused(_) | Failure::TooLarge => {
                self.retry(Duration::ZERO, quiet)
            }
        }
    }

    fn retry(&mut self, floor: Duration, quiet: bool) {
        let was_live = self.phase == Phase::Live;
        let lived = self.live_since.take().map(|since| since.elapsed());
        if lived.is_some() {
            self.last_live = Some(SystemTime::now());
        }
        if lived.is_some_and(|lived| lived >= self.limits.live_resets) {
            self.failures = 0;
        }
        // A live stream that home ended may mean a revoke or a deleted workspace: look again at
        // once, so the person reads that, not "reconnecting". (Not twice in a row if the look
        // came back live and ended within a second.)
        let looks = quiet
            && was_live
            && (!self.rechecking || lived.is_some_and(|l| l >= Duration::from_secs(1)));
        if looks {
            self.rechecking = true;
            self.begin();
            return;
        }
        self.rechecking = false;
        self.failures += 1;
        let next = backoff(self.failures, jitter()).max(floor);
        self.drop_attempt();
        self.phase = Phase::Waiting;
        self.status(Status::Reconnecting {
            attempt: self.failures,
            next,
            live_at: self.last_live,
        });
        let (tx, epoch) = (self.tx.clone(), self.epoch);
        self.timer = Some(Task::spawn(async move {
            tokio::time::sleep(next).await;
            let _ = tx.send(Msg::Retry { epoch });
        }));
    }

    /// Whether the coordinator goes on.
    async fn handle(&mut self, msg: Msg) -> bool {
        match msg {
            Msg::Forget { revoke } => {
                self.forget(revoke).await;
                return false;
            }
            Msg::Command(command) => self.command(command).await,
            Msg::Retry { epoch } if epoch == self.epoch && self.phase == Phase::Waiting => {
                self.begin();
            }
            Msg::Me { epoch, result } if epoch == self.epoch => self.me_arrived(result),
            Msg::Opened { epoch } if epoch == self.epoch => self.live(),
            Msg::Event { epoch, event, .. } if epoch == self.epoch => self.event(event),
            Msg::Lost {
                epoch,
                failure,
                quiet,
            } if epoch == self.epoch => self.lost(failure, quiet),
            Msg::Called {
                epoch,
                about,
                result,
                took,
            } if epoch == self.epoch => self.called(about, result, took),
            Msg::Typed {
                epoch,
                block,
                result,
                took,
            } if epoch == self.epoch => self.typed(block, result, took),
            Msg::Stopped { epoch, block } if epoch == self.epoch => {
                self.streams.remove(&block);
                self.blocked.insert(block);
            }
            _ => {}
        }
        true
    }

    fn me_arrived(&mut self, result: Result<(MeView, Duration), Failure>) {
        let (me, took) = match result {
            Ok(done) => done,
            // A refresh that couldn't reach home: the stream tells soon enough.
            Err(
                Failure::Down(_)
                | Failure::Busy(_)
                | Failure::TooLarge
                | Failure::Refused(_)
                | Failure::Missing(_),
            ) if self.phase == Phase::Live => {
                return;
            }
            Err(failure) => return self.lost(failure, false),
        };
        self.say(Update::Me(me.clone()));
        self.say(Update::Latency(took));
        let deleted = me.workspaces.as_ref().is_some_and(Vec::is_empty);
        self.me = Some(me);
        if deleted {
            return self.cut(Why::Deleted);
        }
        let Some(attempt) = &mut self.attempt else {
            return;
        };
        if attempt.events_started {
            return;
        }
        attempt.events_started = true;
        let task = Task::spawn(events_task(
            attempt.net.clone(),
            self.epoch,
            self.last_id.clone(),
            self.tx.clone(),
            self.limits.silence,
        ));
        attempt._tasks.push(task);
    }

    /// The event stream is open.
    fn live(&mut self) {
        let Some(attempt) = &mut self.attempt else {
            return;
        };
        self.phase = Phase::Live;
        self.refused.clear();
        self.live_since = Some(Instant::now());
        let (typing, inbox) = unbounded_channel();
        attempt.typing = Some(typing);
        attempt._tasks.push(Task::spawn(typing_task(
            attempt.net.clone(),
            self.epoch,
            inbox,
            self.tx.clone(),
            self.limits.typing_patience,
        )));
        self.status(Status::Live);
        self.sync_terminals();
    }

    fn event(&mut self, event: sse::Event) {
        if event.event == crate::REPLACED {
            return self.cut(Why::Replaced);
        }
        if let Some(id) = event.id {
            self.last_id = Some(id);
        }
        let applied = serde_json::from_str::<Value>(&event.data)
            .map_err(|e| ApplyError::Malformed(format!("a malformed {} event: {e}", event.event)))
            .and_then(|data| {
                self.out
                    .apply(&self.id, self.round, &event.event, &data, event.data.len())
            });
        match applied {
            Ok(_) => {}
            Err(ApplyError::TooLarge) => self.lost(Failure::TooLarge, false),
            Err(why) => self.lost(Failure::Down(why.to_string()), false),
        }
    }

    async fn command(&mut self, command: Command) {
        match command {
            Command::Disconnect => {
                self.drop_attempt();
                self.phase = Phase::Idle;
                self.live_since = None;
                self.status(Status::Disconnected);
            }
            Command::TryNow => match self.phase {
                Phase::Waiting | Phase::Opening => self.begin(),
                Phase::Idle => {
                    self.failures = 0;
                    self.rechecking = false;
                    // The person may have fixed what the last verdict named.
                    if let Some(checked) = self.ready.as_ref().and_then(|r| r.checked.as_ref()) {
                        checked.forget();
                    }
                    self.start().await;
                }
                Phase::Live => {}
            },
            Command::Terminals(set) => {
                let set: Vec<_> = set.into_iter().take(STREAMS_MOST).collect();
                // Home refused a stream: it is asked again when the terminals shown change, not
                // when they are only put in another order.
                let same = set.len() == self.wanted.len()
                    && set
                        .iter()
                        .all(|(b, _)| self.wanted.iter().any(|(w, _)| w == b));
                if !same {
                    self.blocked.clear();
                }
                self.wanted = set;
                self.sync_terminals();
            }
            Command::Keys { block, keys } => self.type_keys(block, keys),
            Command::Paste { block, text } => self.type_paste(block, text),
            Command::Move { block, rect } => {
                let args = json!({
                    "block": block, "x": rect.x, "y": rect.y, "w": rect.w, "h": rect.h,
                });
                self.call(About::Move(block), "move_block", args);
            }
            Command::Answer { request, args } => {
                let mut fields = match args {
                    Value::Object(fields) => fields,
                    _ => serde_json::Map::new(),
                };
                fields.insert("request".into(), request.clone().into());
                self.call(
                    About::Answer(request),
                    "answer_request",
                    Value::Object(fields),
                );
            }
            Command::Show => {
                let workspace = self
                    .me
                    .as_ref()
                    .and_then(|me| me.workspaces.as_ref())
                    .and_then(|list| list.first().cloned());
                match workspace {
                    Some(workspace) => {
                        self.call(About::Show, "show", json!({ "workspace": workspace }))
                    }
                    None => self.say(Update::Refused {
                        about: About::Show,
                        sentence: "Not connected.".into(),
                    }),
                }
            }
        }
    }

    /// Hands a call to the calls task, or says it can't be made now.
    fn call(&mut self, about: About, tool: &'static str, args: Value) {
        let sent = match (&self.attempt, self.phase) {
            (Some(attempt), Phase::Live) => attempt
                .calls
                .send(Call {
                    about: about.clone(),
                    tool,
                    args,
                })
                .is_ok(),
            _ => false,
        };
        if !sent {
            self.say(Update::Refused {
                about,
                sentence: "Not connected.".into(),
            });
        }
    }

    fn called(&mut self, about: About, result: Result<Value, Failure>, took: Duration) {
        match result {
            Ok(_) => self.say(Update::Latency(took)),
            Err(Failure::Refused(sentence) | Failure::Missing(sentence)) => {
                self.say(Update::Refused { about, sentence })
            }
            Err(Failure::Down(_) | Failure::Busy(_) | Failure::TooLarge) => {
                self.say(Update::Refused {
                    about,
                    sentence: "Couldn't reach the other computer.".into(),
                })
            }
            Err(failure) => self.lost(failure, false),
        }
    }

    /// Opens a stream for each wanted terminal that has none, and ends those no longer wanted.
    fn sync_terminals(&mut self) {
        let Some(attempt) = &self.attempt else { return };
        if self.phase != Phase::Live {
            return;
        }
        let wanted = &self.wanted;
        // A stream feeds the viewer it was opened for: another viewer for the block, another stream.
        self.streams.retain(|block, (feeder, _)| {
            wanted
                .iter()
                .any(|(w, f)| w == block && f.same_viewer(feeder))
        });
        for (block, feeder) in &self.wanted {
            if self.streams.contains_key(block) || self.blocked.contains(block) {
                continue;
            }
            self.streams.insert(
                block.clone(),
                (
                    feeder.clone(),
                    Task::spawn(terminal_task(
                        attempt.net.clone(),
                        self.epoch,
                        block.clone(),
                        feeder.clone(),
                        self.tx.clone(),
                        self.limits.silence,
                    )),
                ),
            );
        }
    }

    fn queue_of(&mut self, block: &str) -> &mut Queue {
        if !self.turns.iter().any(|b| b == block) && !self.queues.contains_key(block) {
            self.turns.push_back(block.into());
        }
        self.queues.entry(block.into()).or_default()
    }

    /// Typing while not live is never sent late; the person is told once per block, until live
    /// again (the status may still say Live while the look after a stream's end is made).
    fn not_live(&mut self, block: String) {
        if self.refused.insert(block.clone()) {
            self.say(Update::Refused {
                about: About::Keys(block),
                sentence: "Not connected: what you type isn't sent.".into(),
            });
        }
    }

    fn type_keys(&mut self, block: String, keys: Vec<String>) {
        if self.phase != Phase::Live {
            return self.not_live(block);
        }
        let queue = self.queue_of(&block);
        let room = WAITING_MOST.saturating_sub(queue.keys);
        let dropped = keys.len() > room;
        for key in keys.into_iter().take(room) {
            queue.items.push_back(Item::Key(key));
            queue.keys += 1;
        }
        if dropped {
            self.said_dropped(block);
        }
        self.pump();
    }

    fn type_paste(&mut self, block: String, text: String) {
        if self.phase != Phase::Live {
            return self.not_live(block);
        }
        if text.len() > MAX_TEXT {
            return self.say(Update::Refused {
                about: About::Keys(block),
                sentence: "Paste at most 256 KiB into a mirrored terminal.".into(),
            });
        }
        let queue = self.queue_of(&block);
        if queue.paste_bytes + text.len() > MAX_TEXT {
            return self.said_dropped(block);
        }
        queue.paste_bytes += text.len();
        queue.items.push_back(Item::Paste(text));
        self.pump();
    }

    /// `Dropped`, once while the block's queue stays full.
    fn said_dropped(&mut self, block: String) {
        let queue = self.queue_of(&block);
        if !std::mem::replace(&mut queue.dropped, true) {
            self.say(Update::Dropped(block));
        }
    }

    /// Sends the next piece of typing if the typing connection is free: the waiting keys of the
    /// block that has waited longest (at most `INPUT_KEYS`), or its paste, in order.
    fn pump(&mut self) {
        if self.typing_busy {
            return;
        }
        let Some(sender) = self.attempt.as_ref().and_then(|a| a.typing.clone()) else {
            return;
        };
        while let Some(block) = self.turns.pop_front() {
            let Some(queue) = self.queues.get_mut(&block) else {
                continue;
            };
            let args = match queue.items.pop_front() {
                None => {
                    self.queues.remove(&block);
                    continue;
                }
                Some(Item::Paste(text)) => {
                    queue.paste_bytes -= text.len();
                    json!({ "block": block, "text": text })
                }
                Some(Item::Key(first)) => {
                    let mut keys = vec![first];
                    while keys.len() < INPUT_KEYS
                        && matches!(queue.items.front(), Some(Item::Key(_)))
                    {
                        if let Some(Item::Key(key)) = queue.items.pop_front() {
                            keys.push(key);
                        }
                    }
                    queue.keys -= keys.len();
                    json!({ "block": block, "keys": keys })
                }
            };
            if queue.keys < WAITING_MOST && queue.paste_bytes == 0 {
                queue.dropped = false;
            }
            if queue.items.is_empty() {
                self.queues.remove(&block);
            } else {
                self.turns.push_back(block.clone());
            }
            self.typing_busy = sender.send(Batch { block, args }).is_ok();
            return;
        }
    }

    fn typed(&mut self, block: String, result: Result<Value, Failure>, took: Duration) {
        self.typing_busy = false;
        match result {
            Ok(_) => self.say(Update::Latency(took)),
            Err(failure) => {
                // What waits behind a failed call is not sent late.
                self.queues.remove(&block);
                self.turns.retain(|b| *b != block);
                match failure {
                    Failure::Refused(sentence) | Failure::Missing(sentence) => {
                        self.say(Update::Refused {
                            about: About::Keys(block),
                            sentence,
                        })
                    }
                    // Home stayed busy through every try, or the call failed (it is not made
                    // again: `terminal_input` is not idempotent): what was waiting is gone.
                    Failure::Busy(_) | Failure::Down(_) | Failure::TooLarge => {
                        self.say(Update::Dropped(block))
                    }
                    failure => return self.lost(failure, false),
                }
            }
        }
        self.pump();
    }

    /// Revokes the share with its own token when asked and live, deletes the key, ends.
    async fn forget(&mut self, revoke: bool) {
        let net = match (revoke, &self.attempt, self.phase) {
            (true, Some(attempt), Phase::Live) => Some(attempt.net.clone()),
            _ => None,
        };
        // The streams go first: the revoke may wait on a slow home for a minute, and a hostile
        // one would fill that minute with events nobody reads.
        self.drop_attempt();
        if let Some(net) = net {
            // Its result is ignored: whether or not home heard, the key goes.
            if let Ok(mut conn) = Conn::open(&net.place, net.tls.clone(), net.proxy.as_ref()).await
            {
                let _ = conn
                    .call(
                        "POST",
                        "/v1/tools/revoke_share",
                        Some(&json!({})),
                        &net.headers,
                    )
                    .await;
            }
        }
        let entry = match &self.remote.key {
            KeySource::Kept { entry, file } | KeySource::Save { entry, file, .. } => {
                Some((entry.clone(), *file))
            }
            KeySource::Memory(_) => None,
        };
        if let Some((entry, file)) = entry {
            let keys = self.keys.clone();
            let _ = blocking(move || keys.delete(&entry, file)).await;
        }
    }
}

/// Runs `work` on the runtime's blocking pool: the keyring may wait for the person. None: it
/// panicked.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

fn down(error: impl std::fmt::Display) -> Failure {
    Failure::Down(error.to_string())
}

/// The calls connection: `GET /v1/me` first, then the calls it is given in order, and `GET
/// /v1/me` again when none came for `every`.
async fn calls_task(
    net: Arc<Net>,
    epoch: u64,
    mut inbox: UnboundedReceiver<Call>,
    tx: UnboundedSender<Msg>,
    every: Duration,
) {
    let mut conn: Option<Conn> = None;
    let mut job = None;
    loop {
        if conn.as_ref().is_none_or(|c| !c.open_still()) {
            match Conn::open(&net.place, net.tls.clone(), net.proxy.as_ref()).await {
                Ok(open) => conn = Some(open),
                Err(failure) => {
                    let _ = match job {
                        None => tx.send(Msg::Me {
                            epoch,
                            result: Err(failure),
                        }),
                        Some(Call { about, .. }) => tx.send(Msg::Called {
                            epoch,
                            about,
                            result: Err(failure),
                            took: Duration::ZERO,
                        }),
                    };
                    job = match next_call(&mut inbox, every).await {
                        Some(next) => next,
                        None => return,
                    };
                    continue;
                }
            }
        }
        let Some(open) = conn.as_mut() else { continue };
        let started = Instant::now();
        match job.take() {
            None => {
                let result = open
                    .call("GET", "/v1/me", None, &net.headers)
                    .await
                    .and_then(|value| {
                        serde_json::from_value::<MeView>(value).map_err(|_| {
                            Failure::Stop(Why::NotTemplum {
                                status: 200,
                                kind: "application/json".into(),
                            })
                        })
                    });
                let took = started.elapsed();
                if result.is_err() {
                    conn = None;
                }
                let _ = tx.send(Msg::Me {
                    epoch,
                    result: result.map(|me| (me, took)),
                });
            }
            Some(Call { about, tool, args }) => {
                let path = format!("/v1/tools/{tool}");
                let mut tries = 0;
                // A call home answered "busy" is made again after it says to wait.
                let result = loop {
                    match open.call("POST", &path, Some(&args), &net.headers).await {
                        Err(Failure::Busy(wait)) if tries + 1 < BUSY_TRIES => {
                            tries += 1;
                            tokio::time::sleep(wait.max(PACE)).await;
                        }
                        done => break done,
                    }
                };
                if result.is_err() {
                    conn = None;
                }
                let _ = tx.send(Msg::Called {
                    epoch,
                    about,
                    result,
                    took: started.elapsed(),
                });
            }
        }
        job = match next_call(&mut inbox, every).await {
            Some(next) => next,
            None => return,
        };
    }
}

/// The next call, `Some(None)` when none came in `every` (time to ask `GET /v1/me`), None when
/// the coordinator is gone.
async fn next_call(inbox: &mut UnboundedReceiver<Call>, every: Duration) -> Option<Option<Call>> {
    match tokio::time::timeout(every, inbox.recv()).await {
        Ok(Some(call)) => Some(Some(call)),
        Ok(None) => None,
        Err(_) => Some(None),
    }
}

/// The event stream: opens it, says so, and reads it until it ends, goes silent or fails.
async fn events_task(
    net: Arc<Net>,
    epoch: u64,
    last_id: Option<String>,
    tx: UnboundedSender<Msg>,
    silence: Duration,
) {
    let lost = |failure, quiet| {
        let _ = tx.send(Msg::Lost {
            epoch,
            failure,
            quiet,
        });
    };
    let mut events = match stream(
        &net.place,
        net.tls.clone(),
        net.proxy.as_ref(),
        "/v1/events?view=true",
        &net.headers,
        last_id.as_deref(),
    )
    .await
    {
        Ok(events) => events,
        Err(failure) => return lost(failure, false),
    };
    let _ = tx.send(Msg::Opened { epoch });
    let mut reader = sse::Reader::default();
    let room = Arc::new(Semaphore::new(EVENTS_WAITING_MOST));
    loop {
        match tokio::time::timeout(silence, events.chunk()).await {
            Err(_) => return lost(Failure::Down("the stream went silent".into()), false),
            Ok(None) => return lost(Failure::Down("the stream ended".into()), true),
            // Home ending it (it finished, or closed the connection) is `quiet`; a fault of the
            // connection (a reset, bytes that aren't HTTP) is not.
            Ok(Some(Err(failure))) => return lost(failure, false),
            Ok(Some(Ok(bytes))) => match reader.push(&bytes) {
                Err(sse::TooLarge) => return lost(Failure::TooLarge, false),
                Ok(found) => {
                    for event in found {
                        // Never closed: it only waits while the coordinator catches up.
                        let Ok(_room) = room.clone().acquire_owned().await else {
                            return;
                        };
                        let _ = tx.send(Msg::Event {
                            epoch,
                            event,
                            _room,
                        });
                    }
                }
            },
        }
    }
}

/// The typing connection: one call at a time, in the order it is given, each starting at least
/// `PACE` after the previous one did, so the keys typed meanwhile go together. A call home answers
/// "busy" is made again after it says to wait, for `patience` at most (the coordinator keeps what
/// waits behind it).
async fn typing_task(
    net: Arc<Net>,
    epoch: u64,
    mut inbox: UnboundedReceiver<Batch>,
    tx: UnboundedSender<Msg>,
    patience: Duration,
) {
    let mut conn: Option<Conn> = None;
    let mut last: Option<Instant> = None;
    while let Some(Batch { block, args }) = inbox.recv().await {
        let started = Instant::now();
        let mut tries = 0;
        let mut waited = Duration::ZERO;
        let result = loop {
            if let Some(last) = last {
                tokio::time::sleep_until((last + PACE).into()).await;
            }
            last = Some(Instant::now());
            let result = async {
                if conn.as_ref().is_none_or(|c| !c.open_still()) {
                    conn = Some(Conn::open(&net.place, net.tls.clone(), net.proxy.as_ref()).await?);
                }
                let open = conn.as_mut().ok_or_else(|| down("no connection"))?;
                open.call(
                    "POST",
                    "/v1/tools/terminal_input",
                    Some(&args),
                    &net.headers,
                )
                .await
            }
            .await;
            match result {
                Err(Failure::Busy(wait))
                    if tries + 1 < BUSY_TRIES && waited + wait.max(PACE) <= patience =>
                {
                    tries += 1;
                    waited += wait.max(PACE);
                    // `wait` is at most 30 s (`retry_after`), so this cannot overflow.
                    last = Some(Instant::now() + wait.saturating_sub(PACE));
                }
                done => break done,
            }
        };
        if result.is_err() {
            conn = None;
        }
        let _ = tx.send(Msg::Typed {
            epoch,
            block,
            result,
            took: started.elapsed(),
        });
    }
}

/// One terminal's frames, painted into its viewer as they are read; opened again after a second
/// when it ends or fails, while it is wanted.
async fn terminal_task(
    net: Arc<Net>,
    epoch: u64,
    block: String,
    feeder: Feeder,
    tx: UnboundedSender<Msg>,
    silence: Duration,
) {
    if !block
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return;
    }
    let path = format!("/v1/terminals/{block}/stream");
    loop {
        let mut wait = REOPEN;
        match stream(
            &net.place,
            net.tls.clone(),
            net.proxy.as_ref(),
            &path,
            &net.headers,
            None,
        )
        .await
        {
            Ok(mut events) => {
                let (mut reader, mut painter) = (sse::Reader::default(), Painter::default());
                'read: loop {
                    let bytes = match tokio::time::timeout(silence, events.chunk()).await {
                        Ok(Some(Ok(bytes))) => bytes,
                        _ => break,
                    };
                    let Ok(found) = reader.push(&bytes) else {
                        break;
                    };
                    for event in found.iter().filter(|e| e.event == "frame") {
                        let Ok(frame) = serde_json::from_str::<FrameView>(&event.data) else {
                            break 'read;
                        };
                        if painter.paint(&frame, &feeder).is_err() {
                            break 'read;
                        }
                    }
                }
            }
            Err(Failure::Missing(_) | Failure::Refused(_)) => {
                let _ = tx.send(Msg::Stopped { epoch, block });
                return;
            }
            Err(failure @ (Failure::Auth | Failure::Stop(_))) => {
                let _ = tx.send(Msg::Lost {
                    epoch,
                    failure,
                    quiet: false,
                });
                return;
            }
            // Home said "busy": not before it says.
            Err(Failure::Busy(busy)) => wait = wait.max(busy),
            Err(_) => {}
        }
        tokio::time::sleep(wait).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::block_on;
    use crate::client::http::stream_request;
    use crate::code::Key;

    fn near(a: Duration, seconds: f64) -> bool {
        (a.as_secs_f64() - seconds).abs() < 1e-9
    }

    #[test]
    fn backoff_doubles_to_thirty_seconds_with_a_fifth_either_way() {
        for (attempt, seconds) in [
            (1, 1.0),
            (2, 2.0),
            (3, 4.0),
            (5, 16.0),
            (6, 30.0),
            (40, 30.0),
        ] {
            assert!(near(backoff(attempt, 0.0), seconds), "{attempt}");
        }
        assert!(near(backoff(3, 0.2), 4.8) && near(backoff(3, -0.2), 3.2));
        for _ in 0..100 {
            let j = jitter();
            assert!((-0.2..=0.2).contains(&j), "{j}");
        }
    }

    #[test]
    fn the_event_stream_resumes_with_the_last_id() {
        let remote = Remote {
            url: "http://127.0.0.1:9".into(),
            trust: Trust::Loopback,
            trust_added: false,
            access_id: None,
            proxy: None,
            key: KeySource::Memory(super::super::Secrets {
                token: Key::new("tpl_x".into()),
                access_secret: None,
                proxy_auth: None,
                url: "http://127.0.0.1:9".into(),
                trust: Trust::Loopback,
                trust_added: false,
                proxy: None,
            }),
        };
        let (tx, _inbox) = unbounded_channel();
        let out = Arc::new(Shared::for_tests());
        let mut life = Life::new(
            "r1",
            1,
            remote,
            Keys::without_keyring(None),
            out,
            Limits::default(),
            tx,
        );
        let session = |life: &Life| {
            life.attempt
                .as_ref()
                .map(|a| a.net.headers.clone())
                .unwrap()
        };
        let (first, second) = block_on(async {
            life.prepare().await.unwrap();
            life.begin();
            let first = session(&life);
            life.begin();
            (first, session(&life))
        });
        assert_ne!(
            first.session, second.session,
            "each attempt has a session of its own"
        );

        let request = stream_request("", "/v1/events?view=true", &second, Some("r-7")).unwrap();
        assert_eq!(request.uri().to_string(), "/v1/events?view=true");
        assert_eq!(request.headers()["last-event-id"], "r-7");
        assert_eq!(
            request.headers()["templum-mirror"].to_str().unwrap(),
            second.session
        );
        assert_eq!(request.headers()["accept"], "text/event-stream");
        let fresh = stream_request("/base", "/v1/events?view=true", &second, None).unwrap();
        assert_eq!(fresh.uri().to_string(), "/base/v1/events?view=true");
        assert!(fresh.headers().get("last-event-id").is_none());
    }

    // -- A life, and servers that say what a test needs --

    fn life_at(url: &str) -> Life {
        let remote = Remote {
            url: url.into(),
            trust: Trust::Loopback,
            trust_added: false,
            access_id: None,
            proxy: None,
            key: KeySource::Memory(super::super::Secrets {
                token: Key::new("tpl_x".into()),
                access_secret: None,
                proxy_auth: None,
                url: url.into(),
                trust: Trust::Loopback,
                trust_added: false,
                proxy: None,
            }),
        };
        let (tx, _inbox) = unbounded_channel();
        Life::new(
            "r1",
            1,
            remote,
            Keys::without_keyring(None),
            Arc::new(Shared::for_tests()),
            Limits::default(),
            tx,
        )
    }

    /// Hands each connection to `serve` on a thread of its own. The port.
    fn server(serve: impl Fn(std::net::TcpStream) + Send + Sync + 'static) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let serve = Arc::new(serve);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let serve = serve.clone();
                std::thread::spawn(move || serve(stream));
            }
        });
        port
    }

    /// The next request's head and body; None when the client is gone.
    fn request(stream: &mut std::net::TcpStream) -> Option<(String, String)> {
        use std::io::Read;
        let (mut head, mut byte) = (Vec::new(), [0u8; 1]);
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).ok()? == 0 {
                return None;
            }
            head.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        let length = head
            .to_ascii_lowercase()
            .lines()
            .find_map(|l| l.strip_prefix("content-length: ").map(str::to_string))
            .and_then(|n| n.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; length];
        stream.read_exact(&mut body).ok()?;
        Some((head, String::from_utf8_lossy(&body).into_owned()))
    }

    fn net_at(port: u16) -> Arc<Net> {
        let place = place(&format!("http://127.0.0.1:{port}")).unwrap();
        Arc::new(Net {
            headers: Headers {
                key: Key::new("tpl_x".into()),
                access: None,
                session: "s1".into(),
                host: place.host_header(),
            },
            place,
            tls: None,
            proxy: None,
        })
    }

    /// Counts a move asked of the fake; whether it is the first.
    fn counted_move(asked: &std::sync::Mutex<usize>) -> bool {
        let mut asked = asked.lock().unwrap();
        *asked += 1;
        *asked == 1
    }

    fn viewer() -> templum_terminal::Terminal {
        templum_terminal::Terminal::viewer(
            "v",
            (10, 2),
            templum_terminal::Palette::default(),
            templum_terminal::TermOptions::default(),
            Arc::new(|| {}),
        )
    }

    // -- What an attempt that fails asks of the remote --

    #[test]
    fn a_401_after_the_share_ends_is_expired_and_before_it_is_revoked() {
        let ends = |expires| {
            let mut life = life_at("http://127.0.0.1:9");
            life.me = Some(MeView {
                expires,
                ..Default::default()
            });
            life.lost(Failure::Auth, false);
            life.status
        };
        assert_eq!(ends(Some(unix_now() - 5)), Some(Status::Cut(Why::Expired)));
        assert_eq!(
            ends(Some(unix_now() + 3600)),
            Some(Status::Cut(Why::Revoked))
        );
        assert_eq!(ends(None), Some(Status::Cut(Why::Revoked)));
    }

    #[test]
    fn home_saying_busy_sets_the_floor_of_the_wait() {
        block_on(async {
            let mut life = life_at("http://127.0.0.1:9");
            life.lost(Failure::Busy(Duration::from_secs(40)), false);
            let Some(Status::Reconnecting { next, attempt, .. }) = life.status else {
                panic!("{:?}", life.status);
            };
            assert_eq!(attempt, 1);
            assert!(next >= Duration::from_secs(40), "{next:?}");
            life.lost(Failure::Down("x".into()), false);
            let Some(Status::Reconnecting { next, .. }) = life.status else {
                panic!()
            };
            assert!(
                next < Duration::from_secs(4),
                "a plain failure has no floor: {next:?}"
            );
        });
    }

    #[test]
    fn try_now_restarts_an_opening_attempt_too() {
        block_on(async {
            let mut life = life_at("http://127.0.0.1:9");
            life.prepare().await.unwrap();
            life.begin();
            let epoch = life.epoch;
            assert_eq!(life.phase, Phase::Opening);
            life.command(Command::TryNow).await;
            assert!(life.epoch > epoch, "a new attempt");
            assert_eq!(life.phase, Phase::Opening);
        });
    }

    #[test]
    fn try_now_forgets_the_verdicts_a_public_certificate_was_trusted_by() {
        use crate::client::trust::tests as certs;
        use tokio_rustls::rustls::client::danger::ServerCertVerifier;
        use tokio_rustls::rustls::pki_types::{ServerName, UnixTime};
        block_on(async {
            let mut life = life_at("http://127.0.0.1:9");
            life.prepare().await.unwrap();
            let a = certs::ca("a");
            let platform = certs::Platform::counting(certs::roots(&[&a]));
            let checked = Arc::new(trust::Checked::new(
                platform.clone(),
                certs::roots(&[&a]),
                false,
            ));
            life.ready.as_mut().unwrap().checked = Some(checked.clone());
            let leaf = certs::leaf(&a, "localhost", false);
            let (name, now) = (ServerName::try_from("localhost").unwrap(), UnixTime::now());
            let ask = || checked.verify_server_cert(&leaf.der, &[], &name, &[], now);
            assert!(ask().is_ok() && ask().is_ok());
            assert_eq!(platform.asked(), 1, "a fine chain is kept");
            // Cut, then Try now: the person may have taken the authority away since.
            life.cut(Why::Revoked);
            life.command(Command::TryNow).await;
            assert!(ask().is_ok());
            assert_eq!(platform.asked(), 2, "asked anew");
        });
    }

    #[test]
    fn a_snapshot_past_the_caps_makes_the_remote_try_again() {
        block_on(async {
            let mut life = life_at("http://127.0.0.1:9");
            life.out.inner().current.insert("r1".into(), 1);
            let big = crate::model::State {
                blocks: vec![Default::default(); 10_001],
                ..Default::default()
            };
            life.event(sse::Event {
                event: "snapshot".into(),
                data: serde_json::to_string(&big).unwrap(),
                id: None,
            });
            assert!(matches!(life.status, Some(Status::Reconnecting { .. })));
            assert_eq!(life.out.inner().states.len(), 0, "nothing of it was kept");
        });
    }

    // -- Typing --

    #[test]
    fn typing_calls_are_spaced_and_a_busy_one_is_made_again() {
        let seen: Arc<std::sync::Mutex<Vec<(Instant, String)>>> = Arc::default();
        let log = seen.clone();
        let port = server(move |mut stream| {
            use std::io::Write;
            while let Some((_, body)) = request(&mut stream) {
                let first = {
                    let mut log = log.lock().unwrap();
                    log.push((Instant::now(), body));
                    log.len() == 1
                };
                let answer = if first {
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}"
                };
                if stream.write_all(answer.as_bytes()).is_err() {
                    return;
                }
            }
        });
        let (tx, mut results) = unbounded_channel();
        let (batches, inbox) = unbounded_channel();
        let batch = |keys: &[&str]| Batch {
            block: "b".into(),
            args: json!({"block": "b", "keys": keys}),
        };
        block_on(async {
            let _task = Task::spawn(typing_task(
                net_at(port),
                1,
                inbox,
                tx,
                Duration::from_secs(5),
            ));
            batches.send(batch(&["a", "b"])).unwrap();
            batches.send(batch(&["c"])).unwrap();
            for _ in 0..2 {
                let Some(Msg::Typed { result, .. }) = results.recv().await else {
                    panic!("typed")
                };
                assert!(result.is_ok(), "{result:?}");
            }
        });
        let seen = seen.lock().unwrap();
        let bodies: Vec<Value> = seen
            .iter()
            .map(|(_, b)| serde_json::from_str(b).unwrap())
            .collect();
        assert_eq!(
            bodies,
            [
                json!({"block": "b", "keys": ["a", "b"]}),
                json!({"block": "b", "keys": ["a", "b"]}),
                json!({"block": "b", "keys": ["c"]}),
            ],
            "the busy call again, then the next"
        );
        for pair in seen.windows(2) {
            let gap = pair[1].0 - pair[0].0;
            assert!(gap >= PACE / 2, "{gap:?}");
        }
    }

    /// A typing task, patient for `patience`, whose home answers every call with 429 and
    /// `retry_after`; the answers it got and what the first batch came to.
    fn typing_against_busy_home(
        retry_after: &'static str,
        patience: Duration,
    ) -> (usize, Result<Value, Failure>) {
        let asked: Arc<std::sync::Mutex<usize>> = Arc::default();
        let counted = asked.clone();
        let port = server(move |mut stream| {
            use std::io::Write;
            while request(&mut stream).is_some() {
                *counted.lock().unwrap() += 1;
                let answer = format!(
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: {retry_after}\r\nContent-Length: 0\r\n\r\n"
                );
                if stream.write_all(answer.as_bytes()).is_err() {
                    return;
                }
            }
        });
        let (tx, mut results) = unbounded_channel();
        let (batches, inbox) = unbounded_channel();
        let result = block_on(async {
            let _task = Task::spawn(typing_task(net_at(port), 1, inbox, tx, patience));
            batches
                .send(Batch {
                    block: "b".into(),
                    args: json!({"block": "b", "keys": ["a"]}),
                })
                .unwrap();
            let Some(Msg::Typed { result, .. }) = results.recv().await else {
                panic!("typed")
            };
            result
        });
        let asked = *asked.lock().unwrap();
        (asked, result)
    }

    #[test]
    fn typing_is_given_up_after_ten_busy_answers() {
        let (asked, result) = typing_against_busy_home("0", Duration::from_secs(5));
        assert!(matches!(result, Err(Failure::Busy(_))), "{result:?}");
        assert_eq!(asked, usize::try_from(BUSY_TRIES).unwrap());
    }

    #[test]
    fn typing_told_to_wait_long_drops_its_keys_instead_of_sending_them_late() {
        // 600 s is held to 30 s, past what typing waits: one answer, no second call, no sleep.
        let (asked, result) = typing_against_busy_home("600", Duration::from_secs(5));
        assert!(matches!(result, Err(Failure::Busy(_))), "{result:?}");
        assert_eq!(asked, 1);
        // A number that would overflow an `Instant` is held the same way, and does not panic.
        let (asked, result) =
            typing_against_busy_home("18446744073709551615", Duration::from_secs(5));
        assert!(matches!(result, Err(Failure::Busy(_))), "{result:?}");
        assert_eq!(asked, 1);
    }

    #[test]
    fn typing_waits_its_patience_in_all_over_the_busy_answers() {
        // Each answer is waited out for a `PACE` at least; 150 ms hold two such waits (60 + 60),
        // not a third: the call is made three times, then given up.
        let (asked, result) = typing_against_busy_home("0", Duration::from_millis(150));
        assert!(matches!(result, Err(Failure::Busy(_))), "{result:?}");
        assert_eq!(asked, 3);
    }

    #[test]
    fn a_typing_call_that_fails_drops_what_waited_and_says_so() {
        // Home takes the call and closes the connection without an answer.
        let port = server(|mut stream| {
            let _ = request(&mut stream);
        });
        let (tx, mut results) = unbounded_channel();
        let (batches, inbox) = unbounded_channel();
        let result = block_on(async {
            let _task = Task::spawn(typing_task(
                net_at(port),
                1,
                inbox,
                tx,
                Duration::from_secs(5),
            ));
            batches
                .send(Batch {
                    block: "b".into(),
                    args: json!({"block": "b", "keys": ["a"]}),
                })
                .unwrap();
            let Some(Msg::Typed { result, .. }) = results.recv().await else {
                panic!("typed")
            };
            result
        });
        assert!(matches!(result, Err(Failure::Down(_))), "{result:?}");

        // The coordinator keeps no key of it, and tells the person.
        let mut life = life_at("http://127.0.0.1:9");
        life.out.inner().current.insert("r1".into(), 1);
        life.queues.insert(
            "b".into(),
            Queue {
                items: VecDeque::from([Item::Key("b".into())]),
                keys: 1,
                ..Default::default()
            },
        );
        life.turns.push_back("b".into());
        // (A failure it didn't know would reconnect, which needs the runtime.)
        block_on(async { life.typed("b".into(), result, Duration::ZERO) });
        assert!(life.queues.is_empty() && life.turns.is_empty());
        let said = life.out.inner().updates.clone();
        assert!(
            said.iter()
                .any(|(_, u)| matches!(u, Update::Dropped(b) if b == "b")),
            "{said:?}"
        );
    }

    #[test]
    fn a_move_home_answers_busy_to_is_made_again() {
        let moves: Arc<std::sync::Mutex<usize>> = Arc::default();
        let counted = moves.clone();
        let me = serde_json::to_string(&MeView::default()).unwrap();
        let port = server(move |mut stream| {
            use std::io::Write;
            while let Some((head, _)) = request(&mut stream) {
                let answer = if head.starts_with("GET /v1/me ") {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{me}",
                        me.len()
                    )
                } else if counted_move(&counted) {
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nRetry-After: 0\r\nContent-Length: 82\r\n\r\n{\"error\":{\"code\":\"busy\",\"message\":\"Too many connections: try again in a moment.\"}}".to_string()
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}".to_string()
                };
                if stream.write_all(answer.as_bytes()).is_err() {
                    return;
                }
            }
        });
        let (tx, mut results) = unbounded_channel();
        let (calls, inbox) = unbounded_channel();
        block_on(async {
            let _task = Task::spawn(calls_task(
                net_at(port),
                1,
                inbox,
                tx,
                Duration::from_secs(60),
            ));
            assert!(matches!(
                results.recv().await,
                Some(Msg::Me { result: Ok(_), .. })
            ));
            calls
                .send(Call {
                    about: About::Move("b".into()),
                    tool: "move_block",
                    args: json!({"block": "b"}),
                })
                .unwrap();
            let Some(Msg::Called { result, .. }) = results.recv().await else {
                panic!("called")
            };
            assert!(result.is_ok(), "{result:?}");
        });
        assert_eq!(*moves.lock().unwrap(), 2, "once refused, once made");
    }

    // -- The event stream --

    #[test]
    fn only_a_clean_end_of_the_stream_is_home_ending_it() {
        let ends = |tail: &'static str| {
            let port = server(move |mut stream| {
                use std::io::Write;
                if request(&mut stream).is_some() {
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{tail}"
                        )
                        .as_bytes(),
                    );
                }
            });
            let (tx, mut inbox) = unbounded_channel();
            block_on(async {
                let _task = Task::spawn(events_task(
                    net_at(port),
                    1,
                    None,
                    tx,
                    Duration::from_secs(5),
                ));
                loop {
                    match inbox.recv().await {
                        Some(Msg::Lost { quiet, .. }) => return quiet,
                        Some(_) => {}
                        None => panic!("no end"),
                    }
                }
            })
        };
        assert!(ends("0\r\n\r\n"), "a stream that finished");
        assert!(
            ends("5\r\nhello"),
            "a connection home closed before the body ended"
        );
        assert!(!ends("zz\r\n"), "bytes that aren't HTTP");
    }

    #[test]
    fn a_home_that_floods_events_is_held_by_its_socket_not_kept_in_memory() {
        use std::io::Write;
        // Far more than a socket holds, so a mirror that reads everything is seen reading it.
        const MOST: usize = 32 << 20;
        let (done, written) = std::sync::mpsc::channel();
        let port = server(move |mut stream| {
            if request(&mut stream).is_none() {
                return;
            }
            // A write that stays blocked this long is a client that stopped reading.
            let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
            let events = "event: block\ndata: {\"op\":\"delete\",\"id\":\"x\"}\n\n".repeat(300);
            let chunk = format!("{:x}\r\n{events}\r\n", events.len());
            let mut sent = 0;
            if stream.write_all(head.as_bytes()).is_ok() {
                while sent < MOST && stream.write_all(chunk.as_bytes()).is_ok() {
                    sent += chunk.len();
                }
            }
            let _ = done.send(sent);
        });
        let (tx, mut inbox) = unbounded_channel();
        let sent = block_on(async {
            let _task = Task::spawn(events_task(
                net_at(port),
                1,
                None,
                tx,
                Duration::from_secs(30),
            ));
            // Nothing takes the events: the coordinator is as slow as can be. A deadline, not a
            // fixed wait: the server is done when its writes block.
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                if let Ok(sent) = written.try_recv() {
                    break sent;
                }
                assert!(
                    Instant::now() < deadline,
                    "the server's writes never blocked"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        assert!(
            sent < MOST / 2,
            "the server wrote {sent} bytes before it was held"
        );
        let mut waiting = 0;
        while let Ok(msg) = inbox.try_recv() {
            waiting += usize::from(matches!(msg, Msg::Event { .. }));
        }
        assert!(waiting <= EVENTS_WAITING_MOST, "{waiting} events wait");
    }

    // -- Terminal streams --

    #[test]
    fn a_stream_follows_the_viewer_of_its_block() {
        block_on(async {
            let mut life = life_at("http://127.0.0.1:9");
            life.prepare().await.unwrap();
            life.begin();
            life.phase = Phase::Live;
            let (first, second) = (viewer(), viewer());
            let on = |life: &Life, viewer: &templum_terminal::Terminal| {
                life.streams["b7k2m"].0.same_viewer(&viewer.feeder())
            };
            life.command(Command::Terminals(vec![("b7k2m".into(), first.feeder())]))
                .await;
            assert!(on(&life, &first));
            // The same viewer again (another order, a new handle): the stream goes on.
            life.command(Command::Terminals(vec![("b7k2m".into(), first.feeder())]))
                .await;
            assert!(on(&life, &first) && life.streams.len() == 1);
            // Another viewer for the block: a stream of its own, not the old one feeding the old.
            life.command(Command::Terminals(vec![("b7k2m".into(), second.feeder())]))
                .await;
            assert!(on(&life, &second) && !on(&life, &first));
        });
    }

    #[test]
    fn a_terminal_stream_told_busy_waits_as_long_as_home_says() {
        let (tx, mut asked) = unbounded_channel();
        let port = server(move |mut stream| {
            use std::io::Write;
            if request(&mut stream).is_some() {
                let _ = tx.send(Instant::now());
                let _ = stream.write_all(
                    b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });
        let (tx, _inbox) = unbounded_channel();
        let gap = block_on(async {
            let _task = Task::spawn(terminal_task(
                net_at(port),
                1,
                "b7k2m".into(),
                viewer().feeder(),
                tx,
                Duration::from_secs(5),
            ));
            let first = asked.recv().await.unwrap();
            let second = tokio::time::timeout(Duration::from_secs(10), asked.recv())
                .await
                .expect("it opens again")
                .unwrap();
            second - first
        });
        assert!(
            gap >= Duration::from_millis(1900),
            "not before Retry-After: {gap:?}"
        );
        assert!(gap < Duration::from_secs(5), "{gap:?}");
    }
}
