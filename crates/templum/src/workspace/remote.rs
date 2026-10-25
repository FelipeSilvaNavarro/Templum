//! Remote workspaces (docs/specs/templum-to-templum.md, The remote workspace): a `Space` that
//! mirrors a workspace of another Templum. The index keeps its connection, never a canvas;
//! `templum_api::client::Mirror` follows it on a thread of its own and wakes this one.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use futures::StreamExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::{ClipboardItem, Context, Window};
use templum_api::client::{self, Command, Kept, KeySource, Mirror, Secrets, Status, Update, Why};
use templum_api::code::{self, Code, Key, Trust};
use templum_api::model::{AgentView, BlockView, MeView, State, WorkspaceView};
use templum_canvas::{BlockId, Camera, Point, Rect, Size, Wire, chrome};
use templum_terminal::Terminal;

use super::{
    Body, DEFAULT_COLS, DEFAULT_ROWS, Mirrored, Pane, RemotePane, Space, Workspace,
    show_notification,
};
use crate::session::{Keeping, RemoteEntry};

/// The most blocks a mirror takes from home, the front ones: a canvas of home's is not trusted to
/// have a size this computer can draw.
pub(crate) const MOST_BLOCKS: usize = 500;

/// The most wires a mirror takes from home: each is drawn every frame, against every block.
pub(crate) const MOST_WIRES: usize = 200;

/// The farthest from the origin a block or a camera of home's may sit: past it the canvas' union overflows
/// and the dot grid has no end to paint.
pub(super) const MOST_FAR: f32 = 1e7;

/// The longest a question of home's agent is, in characters.
const ASKS_CHARS: usize = 200;

/// What a remote workspace keeps besides its canvas.
pub(crate) struct RemoteSpace {
    pub entry: RemoteEntry,
    pub status: Status,
    /// Who the share is, from home's `GET /v1/me`.
    pub me: Option<MeView>,
    /// Home's workspace, as the newest state shows it.
    pub home: Option<WorkspaceView>,
    /// The last round trip to home.
    pub latency: Option<Duration>,
    /// The last time it was live.
    pub live_at: Option<SystemTime>,
    /// When `status` last changed.
    pub since: Instant,
    /// Home's block ids, which never become the ids agents name blocks by (`Space::uids`).
    pub homes: HashMap<String, BlockId>,
    /// `homes` the other way round.
    names: HashMap<BlockId, String>,
    /// The camera is the person's: home's view or the blocks' fit placed it once.
    pub placed: bool,
    /// The space's icon and color are home's, so the index doesn't keep them as the person's.
    pub home_look: bool,
    /// The terminals the client was last told to stream (home's id, the block here), sorted, and
    /// when that set was last chosen.
    pub streaming: Vec<(String, BlockId)>,
    pub chosen: Option<Instant>,
    /// The client was told to connect it this run: a remote kept nowhere and not started has
    /// nothing to try again, only a new code.
    pub started: bool,
    /// The terminals the wheel already told "Only the live screen is mirrored for now.".
    pub wheel_told: std::collections::HashSet<BlockId>,
}

impl RemoteSpace {
    pub fn new(entry: RemoteEntry) -> RemoteSpace {
        RemoteSpace {
            placed: entry
                .camera
                .is_some_and(|c| c.x.abs() <= MOST_FAR && c.y.abs() <= MOST_FAR),
            started: false,
            home_look: false,
            streaming: Vec::new(),
            chosen: None,
            wheel_told: Default::default(),
            entry,
            status: Status::Connecting,
            me: None,
            home: None,
            latency: None,
            live_at: None,
            since: Instant::now(),
            homes: HashMap::new(),
            names: HashMap::new(),
        }
    }

    /// The canvas and the tile are dimmed: it isn't live, nor on its way to being.
    pub fn dim(&self) -> bool {
        !matches!(self.status, Status::Live | Status::Connecting)
    }

    /// What the mirror's API shows of it: never its blocks (docs/specs/templum-to-templum.md,
    /// What the mirror's API shows).
    pub fn view(&self) -> templum_api::model::RemoteView {
        templum_api::model::RemoteView {
            computer: self
                .me
                .as_ref()
                .map(|m| m.computer.clone())
                .filter(|c| !c.is_empty()),
            host: host_of(&self.entry.url),
            way: self.entry.way.clone(),
            status: self.status.word().to_string(),
            latency_ms: self.latency.map(|l| l.as_millis() as u64),
        }
    }

    /// Home's id of the block `id` here.
    pub fn home_of(&self, id: BlockId) -> Option<String> {
        self.names.get(&id).cloned()
    }

    /// The connection the client makes, with its key from `key`. An index that names no way in
    /// Templum knows, or a pin that is missing, reads as one the key was not kept for, which the
    /// client refuses (`Remote::binds`).
    pub fn remote(&self, key: KeySource) -> client::Remote {
        let entry = &self.entry;
        client::Remote {
            url: entry.url.clone(),
            trust: match entry.way.as_str() {
                "pinned" => Trust::Pinned(entry.pin.clone().unwrap_or_default()),
                "loopback" => Trust::Loopback,
                _ => Trust::Public,
            },
            trust_added: entry.trust_added_authority,
            access_id: entry.access_id.clone(),
            proxy: entry.proxy.clone(),
            key,
        }
    }
}

impl RemoteEntry {
    /// The entry for a pasted `code`, whose key will be kept under `entry`.
    pub(crate) fn of(code: &Code, entry: String) -> RemoteEntry {
        let (way, pin) = match &code.trust {
            Trust::Pinned(pin) => ("pinned", Some(pin.clone())),
            Trust::Public => ("public", None),
            Trust::Loopback => ("loopback", None),
        };
        RemoteEntry {
            url: code.url.clone(),
            way: way.into(),
            pin,
            entry,
            ..RemoteEntry::default()
        }
    }
}

/// What the Connect dialog hands over: the code and what More… added.
#[derive(Debug)]
pub(crate) struct Connecting {
    pub code: Code,
    /// A Cloudflare Access service token: Client ID and Client Secret (which prints as `tpl_…`).
    pub access: Option<(String, Key)>,
    /// The proxy as typed, `http://[user:password@]host:port`; a `Key` so a login in it never
    /// prints.
    pub proxy: Option<Key>,
    /// Keep the key in a file (no keyring answers).
    pub file: bool,
}

/// The most terminals of one remote streamed at a time (the client's own cap is the same).
pub(crate) const MOST_STREAMS: usize = 12;
/// How often the set of terminals to stream is chosen again, at most.
pub(crate) const STREAM_EVERY: Duration = Duration::from_millis(100);

/// The terminals to stream out of `blocks` (home's id and rectangle of each remote terminal that
/// is drawn in full and in view): the focused one first, then the nearest to `center`, at most
/// [`MOST_STREAMS`].
pub(crate) fn streamed(
    blocks: &[(String, Rect)],
    center: Point,
    focused: Option<&str>,
) -> Vec<String> {
    let far = |r: &Rect| {
        let (x, y) = (r.x + r.w / 2.0 - center.x, r.y + r.h / 2.0 - center.y);
        x.hypot(y)
    };
    let mut near: Vec<&(String, Rect)> = blocks.iter().collect();
    near.sort_by(|a, b| {
        let (a_first, b_first) = (Some(a.0.as_str()) == focused, Some(b.0.as_str()) == focused);
        b_first.cmp(&a_first).then(far(&a.1).total_cmp(&far(&b.1)))
    });
    near.into_iter()
        .take(MOST_STREAMS)
        .map(|(id, _)| id.clone())
        .collect()
}

/// The zoom a remote viewer of `grid` cells is drawn at in a block whose content is `content`
/// (world units), the camera being at `zoom`: the largest step of the zoom's that holds home's
/// whole grid (a cell is `cell` at zoom one), else the smallest.
pub(crate) fn fit_zoom(zoom: f32, content: Size, grid: (u16, u16), cell: Size) -> f32 {
    let (cols, rows) = (f32::from(grid.0.max(1)), f32::from(grid.1.max(1)));
    let k = (content.w / (cols * cell.w)).min(content.h / (rows * cell.h));
    let want = zoom * k * (1.0 + 1e-4);
    templum_canvas::ZOOM_STEPS
        .into_iter()
        .rev()
        .find(|step| *step <= want)
        .unwrap_or(templum_canvas::ZOOM_STEPS[0])
}

/// What the banner over a remote's canvas can be asked to do (its buttons).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Act {
    TryNow,
    Paste,
    Forget,
    StartIt,
    Connect,
    ConnectHere,
    DontConnect,
    ConnectAnyway,
}

impl Act {
    /// The button's words; a retry of a remote that is still trying says "Try now".
    pub(crate) fn label(self, status: &Status) -> &'static str {
        match self {
            Act::TryNow if matches!(status, Status::Reconnecting { .. }) => "Try now",
            Act::TryNow => "Try again",
            Act::Paste => "Paste a new code…",
            Act::Forget => "Forget",
            Act::StartIt => "Start it",
            Act::Connect => "Connect",
            Act::ConnectHere => "Connect here again",
            Act::DontConnect => "Don't connect",
            Act::ConnectAnyway => "Connect anyway",
        }
    }
}

/// What the banner says of a remote: its words, its buttons, and whether the canvas is dimmed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Banner {
    pub text: String,
    pub acts: Vec<Act>,
    pub dim: bool,
}

/// The host of a code's address as a person says it: no port when it is the scheme's own.
pub(crate) fn host_of(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let host = rest.split('/').next().unwrap_or(rest);
    let own = if scheme.eq_ignore_ascii_case("http") {
        ":80"
    } else {
        ":443"
    };
    host.strip_suffix(own).unwrap_or(host).to_string()
}

/// "2 min ago", from whole seconds.
fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds} s ago"),
        60..3600 => format!("{} min ago", seconds / 60),
        3600..86_400 => format!("{} h ago", seconds / 3600),
        _ => format!("{} d ago", seconds / 86_400),
    }
}

/// Days left of a share that ends within three days (the banner says so), else None.
fn ends_in(expires: Option<u64>, wall: SystemTime) -> Option<u64> {
    let now = wall.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs();
    let left = expires?.checked_sub(now)?;
    (left < 3 * 86_400).then(|| left.div_ceil(86_400).max(1))
}

/// What the banner of `remote` says now (docs/specs/templum-to-templum.md, When it isn't live).
/// Every state but Live and Connecting is dim.
pub(crate) fn banner_words(remote: &RemoteSpace, now: Instant, wall: SystemTime) -> Banner {
    let host = host_of(&remote.entry.url);
    let name = remote
        .home
        .as_ref()
        .map(|h| h.name.as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("this workspace");
    let computer = remote
        .me
        .as_ref()
        .map(|m| m.computer.as_str())
        .filter(|c| !c.is_empty())
        .unwrap_or(&host);
    let said = |text: String, acts: &[Act], dim: bool| Banner {
        text,
        acts: acts.to_vec(),
        dim,
    };
    match &remote.status {
        Status::Live if remote.home.as_ref().is_some_and(|h| h.stopped) => said(
            format!("{name} is stopped on {computer}."),
            &[Act::StartIt],
            false,
        ),
        Status::Live => {
            let mut text = format!("Mirror of {name} on {computer}");
            if let Some(me) = &remote.me {
                text += &format!(" · {}", me.scope.word());
            }
            if let Some(latency) = remote.latency {
                text += &format!(" · {} ms", latency.as_millis());
            }
            if let Some(days) = ends_in(remote.me.as_ref().and_then(|m| m.expires), wall) {
                let days = if days == 1 {
                    "1 day".into()
                } else {
                    format!("{days} days")
                };
                text += &format!(" · ends in {days}: share again on {computer}");
            }
            said(text, &[], false)
        }
        Status::Connecting => said("Connecting…".into(), &[], false),
        Status::Reconnecting {
            attempt,
            next,
            live_at,
        } => {
            let left = next.saturating_sub(now.saturating_duration_since(remote.since));
            let mut text = format!(
                "Reconnecting to {computer}… try {attempt}, next in {} s.",
                left.as_secs()
            );
            if let Some(at) = live_at.or(remote.live_at)
                && let Ok(since) = wall.duration_since(at)
            {
                text += &format!(" Live {}.", ago(since.as_secs()));
            }
            said(text, &[Act::TryNow], true)
        }
        Status::Cut(why) => {
            let (text, acts): (String, &[Act]) = match why {
                Why::Expired => (
                    format!(
                        "This share of {name} expired. Share again on {computer}, then Paste a new code…"
                    ),
                    &[Act::Paste, Act::Forget],
                ),
                Why::Revoked => (
                    format!(
                        "{computer} revoked this share. Share again there, then Paste a new code…"
                    ),
                    &[Act::Paste, Act::Forget],
                ),
                Why::Deleted => (format!("{name} was deleted on {computer}."), &[Act::Forget]),
                Why::Replaced => (
                    "Another computer connected with this share's code, so this one stopped. \
                     Each computer needs its own share."
                        .into(),
                    &[Act::ConnectHere, Act::Forget],
                ),
                Why::PinChanged => (
                    format!(
                        "{computer}'s certificate changed since you connected. If you made a New \
                         certificate there, share again and paste the new code. If you didn't, \
                         someone may be between the two computers: don't connect."
                    ),
                    &[Act::Paste, Act::Forget],
                ),
                Why::Certificate(bad) => {
                    let what = match bad {
                        client::Bad::Expired => "it expired",
                        client::Bad::Name => "it is for another name",
                        client::Bad::Issuer | client::Bad::Alone => {
                            "no authority this computer trusts signed it"
                        }
                        client::Bad::Other => "it couldn't be checked",
                    };
                    let mut text = format!(
                        "{host}'s certificate isn't valid: {what}. Fix it where the tunnel or \
                         proxy runs, then Try again."
                    );
                    if *bad == client::Bad::Alone {
                        text += " If it is Templum's own, share Directly instead: it is pinned.";
                    }
                    (text, &[Act::TryNow])
                }
                Why::AddedAuthority => (
                    format!(
                        "HTTPS to {host} is signed by an authority this computer added, not a \
                         public one. A company proxy or security program may be reading this \
                         connection: your key, what home's terminals show and what you type. \
                         With a Full share it could run commands on {computer}. Connect only if \
                         you added that authority yourself (a home lab, your own proxy)."
                    ),
                    &[Act::DontConnect, Act::ConnectAnyway],
                ),
                Why::Access => (
                    format!(
                        "Cloudflare Access stands in front of {host}. Add a service token in \
                         Edit… ▸ More…, and allow it in the Access policy."
                    ),
                    &[Act::TryNow],
                ),
                Why::NotTemplum { status, kind } => {
                    // What answers at the code's URL names its own type: not trusted to be short.
                    let kind = super::bus::clean_to(kind, 60);
                    let got = if *status == 0 {
                        kind
                    } else {
                        format!("HTTP {status}, {kind}")
                    };
                    (
                        format!(
                            "{host} didn't answer as a Templum ({got}). Check that the tunnel \
                             points at port 7411."
                        ),
                        &[Act::TryNow],
                    )
                }
                Why::TooOld => (
                    format!("{computer} runs a Templum that can't be mirrored: update it."),
                    &[Act::TryNow],
                ),
                Why::Proxy { proxy, status } => {
                    let proxy = proxy.split_once("://").map_or(proxy.as_str(), |(_, p)| p);
                    let because = match status {
                        407 => "407: it asks for a login Templum can't give".to_string(),
                        0 => "it can't be used".to_string(),
                        other => format!("HTTP {other}"),
                    };
                    (
                        format!("The proxy {proxy} refused the connection ({because})."),
                        &[Act::TryNow],
                    )
                }
                Why::Keyring => (
                    "Templum couldn't read this connection's key from the keyring. Try again, \
                     or Paste a new code…"
                        .into(),
                    &[Act::TryNow, Act::Paste],
                ),
            };
            said(text, acts, true)
        }
        Status::Disconnected => said("Disconnected. Connect".into(), &[Act::Connect], true),
    }
}

/// The words for a remote that stopped a Connect: the banner's, except that a remote the dialog
/// forgets isn't "reconnecting".
fn failure_words(remote: &RemoteSpace) -> String {
    match remote.status {
        Status::Reconnecting { .. } => {
            format!("Couldn't reach {}.", crate::mirror::computer_of(remote))
        }
        _ => banner_words(remote, Instant::now(), SystemTime::now()).text,
    }
}

/// What a viewer of home's terminal is made with.
struct Viewers {
    cell: templum_canvas::Size,
    palette: templum_terminal::Palette,
    options: templum_terminal::TermOptions,
    notify: templum_terminal::Notify,
}

/// An agent as home tells it, in the shape of a local agent's session, for the chip.
fn session_of(agent: &AgentView) -> Option<templum_usage::Session> {
    let usage = agent.usage.as_ref()?;
    Some(templum_usage::Session {
        model: agent.model.clone(),
        // Home's numbers are drawn in the chip: only ones a chip can show.
        context_percent: usage.context_percent.filter(|p| (0.0..=100.0).contains(p)),
        tokens: usage.tokens,
        cost_usd: usage.cost_usd.filter(|c| (0.0..1e6).contains(c)),
        ..templum_usage::Session::default()
    })
}

/// The tooltip of a control a mirrored workspace doesn't have yet.
pub(crate) const NOT_YET: &str = "Not on a mirrored workspace yet";

impl Space {
    pub(crate) fn is_remote(&self) -> bool {
        self.remote.is_some()
    }

    /// The other computer a remote workspace mirrors: its name once it said, else its address.
    /// Home says its own name, and it goes into an agent's instructions: `clean_title`.
    fn mirror_of(&self) -> Option<String> {
        let remote = self.remote.as_deref()?;
        Some(
            remote
                .me
                .as_ref()
                .map(|m| super::bus::clean_title(&m.computer))
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| host_of(&remote.entry.url)),
        )
    }

    /// How `overview` names a remote workspace: "API (mirror of winx-desktop)".
    pub(super) fn mirror_title(&self) -> Option<String> {
        Some(format!(
            "{} (mirror of {})",
            super::bus::clean_title(&self.name),
            self.mirror_of()?
        ))
    }

    /// What a tool naming a remote workspace is told: "API is a mirror of winx-desktop: act on it
    /// there." Its blocks are home's, never reached from here.
    pub(super) fn mirror_words(&self) -> Option<String> {
        Some(format!(
            "{} is a mirror of {}: act on it there.",
            super::bus::clean_title(&self.name),
            self.mirror_of()?
        ))
    }

    /// The remote terminals of this workspace to stream now, by home's id: those in view (drawn
    /// in full, else only the focused one), the focused first, at most [`MOST_STREAMS`].
    pub(super) fn streamed_set(
        &self,
        remote: &RemoteSpace,
        vp: Size,
        full: bool,
    ) -> Vec<(String, BlockId)> {
        let focused = self.focused.and_then(|id| remote.home_of(id));
        let camera = self.view;
        let center = Point::new(
            camera.origin.x + vp.w / 2.0 / camera.zoom(),
            camera.origin.y + vp.h / 2.0 / camera.zoom(),
        );
        let seen: std::collections::HashSet<BlockId> = self
            .canvas
            .visible_from(&camera, vp)
            .filter(|b| self.shown(b.id))
            .map(|b| b.id)
            .collect();
        let blocks: Vec<(String, Rect)> = remote
            .homes
            .iter()
            .filter(|(home, id)| {
                self.terminal(**id)
                    .and_then(|pane| pane.remote.as_ref())
                    .is_some_and(|r| r.block == **home)
                    && (Some(home.as_str()) == focused.as_deref() || (full && seen.contains(*id)))
            })
            .filter_map(|(home, id)| Some((home.clone(), self.canvas.get(*id)?.rect)))
            .collect();
        streamed(&blocks, center, focused.as_deref())
            .into_iter()
            .filter_map(|home| Some((home.clone(), *remote.homes.get(&home)?)))
            .collect()
    }

    /// Home's blocks, back to front, become this canvas's: new ones are added quietly (no id for
    /// agents, no undo step, no focus, no pop-in), known ones take home's rectangle, title and
    /// agent, and those home no longer has go without a trace.
    fn mirror_blocks(&mut self, remote: &mut RemoteSpace, blocks: &[&BlockView], make: &Viewers) {
        // The front MOST_BLOCKS whose rectangle is sane; a rectangle is clamped like a restored
        // canvas', and one beyond MOST_FAR (or not finite) is left out.
        let mut blocks: Vec<(&BlockView, Rect)> = blocks
            .iter()
            .rev()
            .filter_map(|b| Some((*b, mirrored_rect(b, make.cell)?)))
            .take(MOST_BLOCKS)
            .collect();
        blocks.reverse();
        let wanted: std::collections::HashSet<&str> =
            blocks.iter().map(|(b, _)| b.id.as_str()).collect();
        let gone: Vec<(String, BlockId)> = remote
            .homes
            .iter()
            .filter(|(home, _)| !wanted.contains(home.as_str()))
            .map(|(home, id)| (home.clone(), *id))
            .collect();
        for (home, id) in gone {
            remote.homes.remove(&home);
            remote.names.remove(&id);
            self.canvas.remove(id);
            self.blocks.remove(&id);
            self.appearing.remove(&id);
            self.selected.retain(|s| *s != id);
            self.wires.retain(|w| !w.touches(id));
            if self.hovered == Some(id) {
                self.hovered = None;
            }
            if self.focused == Some(id) {
                self.focused = self.canvas.top();
            }
            if self.selected_wire.is_some_and(|w| w.touches(id)) {
                self.selected_wire = None;
            }
        }
        for (b, rect) in &blocks {
            let rect = *rect;
            let id = match remote.homes.get(&b.id) {
                Some(id) => *id,
                None => {
                    let id = self.canvas.add(rect);
                    self.blocks.insert(id, remote_body(b, rect, make));
                    remote.homes.insert(b.id.clone(), id);
                    remote.names.insert(id, b.id.clone());
                    id
                }
            };
            if let Some(block) = self.canvas.get_mut(id) {
                block.rect = rect;
            }
            match self.blocks.get_mut(&id) {
                Some(Body::Terminal(pane)) => {
                    pane.remote = Some(RemotePane {
                        block: b.id.clone(),
                        title: b.title.clone(),
                        agent: b.agent.clone(),
                        read_only: b.read_only,
                    });
                    pane.usage = b.agent.as_ref().and_then(session_of);
                }
                Some(Body::Mirrored(card)) => card.title.clone_from(&b.title),
                _ => {}
            }
        }
        // Home's order, back to front.
        let order: Vec<BlockId> = blocks
            .iter()
            .filter_map(|(b, _)| remote.homes.get(&b.id).copied())
            .collect();
        let now: Vec<BlockId> = self.canvas.blocks().iter().map(|b| b.id).collect();
        if order != now {
            for id in order {
                self.canvas.raise(id);
            }
        }
    }

    /// Home's wires, between the blocks mirrored here (the first `MOST_WIRES` of them, each
    /// once), and their labels (kept by home's ids).
    fn mirror_wires(&mut self, remote: &RemoteSpace, state: &State) {
        let mut seen = std::collections::HashSet::new();
        let mut wires = Vec::new();
        let mut labels = HashMap::new();
        for w in &state.wires {
            if wires.len() == MOST_WIRES {
                break;
            }
            let (Some(a), Some(b)) = (remote.homes.get(&w.a), remote.homes.get(&w.b)) else {
                continue;
            };
            let key = super::wires::label_key(&w.a, &w.b);
            let Some(wire) = Wire::new(*a, *b) else {
                continue;
            };
            if !seen.insert(key.clone()) {
                continue;
            }
            wires.push(wire);
            if let Some(label) = w.label.clone() {
                labels.insert(key, label);
            }
        }
        if self.wires != wires {
            self.wires = wires;
        }
        if self.selected_wire.is_some_and(|w| !self.wires.contains(&w)) {
            self.selected_wire = None;
        }
        self.labels = labels;
    }
}

/// Home's rectangle of `b` as this canvas keeps it: None when it is not finite or lies beyond
/// `MOST_FAR`.
fn mirrored_rect(b: &BlockView, cell: Size) -> Option<Rect> {
    let r = Rect::new(b.rect.x, b.rect.y, b.rect.w, b.rect.h);
    if r.x.abs() > MOST_FAR || r.y.abs() > MOST_FAR {
        return None;
    }
    super::persist::restorable_rect(r, cell)
}

/// Home's words, cleaned once where they enter: they reach this computer's tiles, index and API
/// (docs/specs/templum-to-templum.md, threat model). One line each, and short.
fn tidy(state: &mut State) {
    use super::bus::{clean_title, clean_to};
    for w in &mut state.workspaces {
        w.name = clean_title(&w.name);
    }
    for b in &mut state.blocks {
        b.title = clean_title(&b.title);
        b.kind = clean_title(&b.kind);
        if let Some(a) = &mut b.agent {
            a.agent = clean_title(&a.agent);
            a.state = clean_title(&a.state);
            a.asks = a.asks.as_deref().map(|s| clean_to(s, ASKS_CHARS));
            a.model = a.model.as_deref().map(clean_title);
            a.effort = a.effort.as_deref().map(clean_title);
        }
    }
    for w in &mut state.wires {
        w.label = w
            .label
            .as_deref()
            .map(clean_title)
            .filter(|l| !l.is_empty());
    }
}

/// Who home says it is: its computer's name is shown on tiles and in the banner, and the id of its
/// first workspace is written to index.json, so one that is long or has control characters is
/// ignored.
pub(super) fn tidy_me(me: &mut MeView) {
    me.computer = super::bus::clean_title(&me.computer);
    if let Some(ids) = &mut me.workspaces {
        ids.retain(|id| id.len() <= 64 && !id.chars().any(char::is_control));
    }
}

/// The body of home's block `b`, new here at `rect`: a viewer for a terminal, else the card of
/// what isn't mirrored yet.
fn remote_body(b: &BlockView, rect: Rect, make: &Viewers) -> Body {
    if b.kind != "terminal" {
        return Body::Mirrored(Mirrored {
            kind: b.kind.clone(),
            title: b.title.clone(),
        });
    }
    // Home's grid is only a guess from the rectangle until its first frame sets it. Home's
    // rectangle is not trusted: the guess is capped, or a huge one would allocate a huge grid.
    let (cols, rows) = chrome::grid_size(chrome::content_rect(rect).size(), make.cell);
    let grid = (cols.min(DEFAULT_COLS), rows.min(DEFAULT_ROWS));
    let options = templum_terminal::TermOptions {
        scrollback: 0,
        ..make.options
    };
    let term = Terminal::viewer(&b.title, grid, make.palette, options, make.notify.clone());
    let mut pane = Pane::new(term, None, None);
    pane.remote = Some(RemotePane {
        block: b.id.clone(),
        title: b.title.clone(),
        agent: b.agent.clone(),
        read_only: b.read_only,
    });
    Body::Terminal(Box::new(pane))
}

impl Workspace {
    /// The workspace shown is a mirror of another computer's: what it can't do yet does nothing.
    pub(crate) fn on_remote(&self) -> bool {
        self.space.is_remote()
    }

    /// `r<n>`, the smallest `n` from 1 no workspace has.
    pub(super) fn remote_id(&self) -> String {
        (1..)
            .map(|n| format!("r{n}"))
            .find(|id| !self.spaces().any(|s| s.id == *id))
            .expect("an unused id")
    }

    /// The client, started at the first need. It wakes this workspace through a channel, as a
    /// terminal's reader does, and everything it says is taken at most once per wake.
    pub(super) fn mirror(&mut self, window: &mut Window, cx: &mut Context<Self>) -> &Mirror {
        if self.mirror.is_none() {
            let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let _ = tx.unbounded_send(());
            });
            self.mirror = Some(Mirror::start(self.keys.clone(), wake));
            self._mirror = cx.spawn_in(window, async move |ws, cx| {
                while rx.next().await.is_some() {
                    while rx.try_recv().is_ok() {}
                    if ws
                        .update_in(cx, |ws, window, cx| ws.drain_mirror(window, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        self.mirror.as_ref().expect("started above")
    }

    /// Starts every remote that has its key; one without is disconnected.
    pub(super) fn connect_remotes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let remotes: Vec<_> = self
            .spaces()
            .filter_map(|s| s.remote.as_ref().map(|r| (s.id.clone(), r.entry.clone())))
            .collect();
        for (id, entry) in remotes {
            if entry.key == Keeping::None {
                if let Some(remote) = self.remote_mut(&id) {
                    remote.status = Status::Disconnected;
                }
                continue;
            }
            let key = KeySource::Kept {
                entry: entry.entry.clone(),
                file: entry.key == Keeping::File,
            };
            let Some(remote) = self.remote_mut(&id).map(|r| r.remote(key)) else {
                continue;
            };
            self.mirror(window, cx).connect(&id, remote);
            if let Some(remote) = self.remote_mut(&id) {
                remote.started = true;
            }
        }
        cx.notify();
    }

    fn remote_mut(&mut self, id: &str) -> Option<&mut RemoteSpace> {
        self.space_by_id_mut(id)?.remote.as_deref_mut()
    }

    /// Takes what the client said since the last wake: statuses and the rest into each remote,
    /// then its newest state into the canvas. Once per wake, never per paint.
    pub(super) fn drain_mirror(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mirror) = &self.mirror else {
            return;
        };
        let updates = mirror.updates();
        let ids: Vec<String> = self
            .spaces()
            .filter(|s| s.is_remote())
            .map(|s| s.id.clone())
            .collect();
        let states: Vec<(String, State)> = ids
            .into_iter()
            .filter_map(|id| Some((id.clone(), mirror.state(&id)?)))
            .collect();
        let mut index = false;
        let mut connected = Vec::new();
        let pending = self.pending_connect.clone();
        for (id, update) in updates {
            // The probe's answer has no remote.
            if id.is_empty() {
                if let Update::Keyring(answers) = update {
                    self.keyring = Some(answers);
                }
                continue;
            }
            let Some(remote) = self.remote_mut(&id) else {
                continue;
            };
            match update {
                Update::Status(status) => {
                    remote.since = Instant::now();
                    if status == Status::Live {
                        remote.live_at = Some(SystemTime::now());
                    }
                    remote.status = status.clone();
                    if pending.as_deref() == Some(id.as_str()) {
                        let words = failure_words(remote);
                        connected.push((id, status, words));
                    }
                }
                Update::Me(mut me) => {
                    tidy_me(&mut me);
                    let first = me.workspaces.as_ref().and_then(|w| w.first()).cloned();
                    if first.is_some() && first != remote.entry.home {
                        remote.entry.home = first;
                        index = true;
                    }
                    remote.me = Some(me);
                }
                Update::Latency(latency) => remote.latency = Some(latency),
                Update::Kept(kept) => {
                    remote.entry.key = match kept {
                        Kept::Keyring => Keeping::Keyring,
                        Kept::File => Keeping::File,
                        Kept::Memory => Keeping::None,
                    };
                    index = true;
                }
                Update::Refused { sentence, .. } => {
                    show_notification(
                        window,
                        cx,
                        Notification::warning(super::bus::clean_to(&sentence, 200)),
                    );
                }
                Update::Dropped(_) => {
                    let computer = remote.me.as_ref().map_or("home", |m| m.computer.as_str());
                    let sentence =
                        format!("Typing faster than {computer} answers: some keys were dropped.");
                    show_notification(window, cx, Notification::warning(sentence));
                }
                // Only the probe's, which has no remote.
                Update::Keyring(_) => {}
            }
        }
        for (id, state) in states {
            self.apply_remote(&id, state, window, cx);
        }
        for (id, status, words) in connected {
            self.connect_moved(&id, &status, words, window, cx);
        }
        if index {
            self.write_index(window, cx);
        }
        cx.notify();
    }

    /// The newest state of remote `id` into its workspace: home's blocks at home's rectangles,
    /// back to front, as no step of Ctrl+Z and nothing the mirror's stores or agents know of.
    pub(super) fn apply_remote(
        &mut self,
        id: &str,
        mut state: State,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        tidy(&mut state);
        let viewport = self.viewport(window);
        let make = Viewers {
            cell: self.cell.size,
            palette: self.theme.palette,
            options: self.config.term_options(),
            notify: self.notify.clone(),
        };
        let Some(space) = self.space_by_id_mut(id) else {
            return;
        };
        let Some(mut remote) = space.remote.take() else {
            return;
        };
        let home = state
            .workspaces
            .iter()
            .find(|w| Some(&w.id) == remote.entry.home.as_ref())
            .or(state.workspaces.first())
            .cloned();
        if let Some(home) = &home {
            if !home.name.is_empty() && space.name != home.name {
                space.name.clone_from(&home.name);
            }
            // The person's own look wins over home's.
            if remote.home_look || (space.look.icon.is_none() && space.look.color.is_none()) {
                space.look.icon = home.icon.as_deref().and_then(crate::space_dialog::icon);
                space.look.color = home
                    .color
                    .clone()
                    .filter(|c| crate::space_dialog::COLORS.contains(&c.as_str()));
                remote.home_look = true;
            }
        }
        let mut blocks: Vec<&BlockView> = state
            .blocks
            .iter()
            .filter(|b| home.as_ref().is_some_and(|h| h.id == b.workspace))
            .collect();
        blocks.sort_by_key(|b| b.z);
        space.mirror_blocks(&mut remote, &blocks, &make);
        space.mirror_wires(&remote, &state);
        if !remote.placed && home.is_some() {
            remote.placed = true;
            let mut camera = space.canvas.camera;
            match state
                .view
                .as_ref()
                .filter(|v| Some(&v.workspace) == home.as_ref().map(|h| &h.id))
            {
                Some(v) if v.x.abs() <= MOST_FAR && v.y.abs() <= MOST_FAR => {
                    camera = Camera::at(Point::new(v.x, v.y), v.zoom);
                }
                _ => {
                    if let Some(bounds) = space.canvas.bounds() {
                        camera.fit(bounds, viewport);
                    }
                }
            }
            space.canvas.camera = camera;
            space.view = camera;
            space.flight = None;
        }
        remote.home = home;
        space.remote = Some(remote);
    }

    /// What the person pasted into the Connect dialog becomes a remote workspace, hidden until it
    /// is live. Its key goes to the client to be kept (`KeySource::Save`); the index keeps only the
    /// entry's name. Answers the new workspace's id, or what the dialog should say.
    pub(crate) fn connect_code(
        &mut self,
        draft: Connecting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let (proxy, proxy_auth) = match draft.proxy.as_ref().map(|p| p.expose().trim()) {
            Some(text) if !text.is_empty() => {
                let (address, login) = client::proxy_of(text)?;
                (Some(address), login)
            }
            _ => (None, None),
        };
        let Connecting {
            code, access, file, ..
        } = draft;
        let entry = RemoteEntry {
            access_id: access.as_ref().map(|(id, _)| id.clone()),
            proxy: proxy.clone(),
            ..RemoteEntry::of(&code, client::entry_name())
        };
        let id = self.remote_id();
        let secrets = Secrets {
            token: code.key.clone(),
            access_secret: access.map(|(_, secret)| secret.expose().to_string()),
            proxy_auth,
            url: code.url.clone(),
            trust: code.trust.clone(),
            trust_added: false,
            proxy,
        };
        let mut space = RemoteSpace::new(entry);
        space.started = true;
        let remote = space.remote(KeySource::Save {
            entry: space.entry.entry.clone(),
            secrets,
            file,
        });
        self.connect_clip = Some(code::encode(
            &code.url,
            &code.key,
            match &code.trust {
                Trust::Pinned(pin) => Some(pin.as_str()),
                _ => None,
            },
        ));
        self.others.push(Space {
            id: id.clone(),
            name: "Connecting…".into(),
            remote: Some(Box::new(space)),
            ..Space::default()
        });
        self.pending_connect = Some(id.clone());
        self.connect_done = None;
        self.mirror(window, cx).connect(&id, remote);
        self.write_index(window, cx);
        cx.notify();
        Ok(id)
    }

    /// How the Connect dialog's remote went: None while it tries; Ok once it is live; the words
    /// of what stopped it otherwise (the remote is forgotten then).
    pub(crate) fn connect_result(&self) -> Option<Result<(), String>> {
        self.connect_done.clone()
    }

    /// Remote `id`, the Connect dialog's, said how it is doing. A drain may bring several
    /// statuses: the first one that settles it is the answer.
    pub(super) fn connect_moved(
        &mut self,
        id: &str,
        status: &Status,
        words: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_connect.as_deref() != Some(id) {
            return;
        }
        match status {
            Status::Live => {
                self.pending_connect = None;
                self.connect_done = Some(Ok(()));
                if let Some(at) = self.order().iter().position(|s| s.id == id) {
                    self.switch_to(at, window, cx);
                }
                // The code is a key: it doesn't wait in the clipboard.
                let held = cx.read_from_clipboard().and_then(|item| item.text());
                if held.is_some_and(|text| Some(text.trim()) == self.connect_clip.as_deref()) {
                    cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                }
                self.connect_clip = None;
                self.write_index(window, cx);
            }
            // A certificate from an added authority waits for the person's choice.
            Status::Cut(Why::AddedAuthority) => {}
            Status::Reconnecting { .. } | Status::Cut(_) => {
                self.give_up_connecting(id, window, cx);
                self.connect_done = Some(Err(words));
            }
            Status::Connecting | Status::Disconnected => {}
        }
    }

    /// What a banner button of remote `id` does.
    pub(crate) fn remote_act(
        &mut self,
        id: &str,
        act: Act,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pending = self.pending_connect.as_deref() == Some(id);
        let Some(remote) = self.remote_mut(id) else {
            return;
        };
        let started = remote.started;
        match act {
            // Nothing was ever started for it this run: only a new code can.
            Act::Connect if !started => self.connect_templum(window, cx),
            Act::TryNow | Act::Connect | Act::ConnectHere => {
                self.mirror_send(id, Command::TryNow);
            }
            Act::Paste => self.connect_templum(window, cx),
            Act::Forget => crate::mirror::forget_dialog(self, id, window, cx),
            Act::StartIt => self.mirror_send(id, Command::Show),
            Act::DontConnect if pending => {
                self.give_up_connecting(id, window, cx);
                self.connect_done = Some(Err("Not connected.".into()));
            }
            Act::DontConnect => self.mirror_send(id, Command::Disconnect),
            Act::ConnectAnyway => {
                // Kept for this remote only; the client keeps the key that way and starts again.
                remote.entry.trust_added_authority = true;
                self.mirror_send(id, Command::TrustAdded);
                self.write_index(window, cx);
            }
        }
        cx.notify();
    }

    /// Disconnect (or Connect, when it is disconnected) from the tile's menu and the banner.
    pub(crate) fn toggle_remote(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let live = self
            .spaces()
            .find(|s| s.id == id)
            .and_then(|s| s.remote.as_deref())
            .is_some_and(|r| matches!(r.status, Status::Live | Status::Reconnecting { .. }));
        let act = if live { Act::DontConnect } else { Act::Connect };
        self.remote_act(id, act, window, cx);
    }

    /// Forget remote `id`: its share is revoked at home first when `revoke`, its key deleted, its
    /// connections ended and its workspace removed.
    pub(crate) fn forget_remote(
        &mut self,
        id: &str,
        revoke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_connect.as_deref() == Some(id) {
            self.pending_connect = None;
        }
        self.delete_space_as(id, revoke, window, cx);
    }

    /// What the Connect dialog's remote asks of the person, while it waits for a choice: a
    /// certificate from an authority this computer added.
    pub(crate) fn connect_choice(&self) -> Option<String> {
        let id = self.pending_connect.as_deref()?;
        let remote = self.spaces().find(|s| s.id == id)?.remote.as_deref()?;
        matches!(remote.status, Status::Cut(Why::AddedAuthority))
            .then(|| banner_words(remote, Instant::now(), SystemTime::now()).text)
    }

    /// The Connect dialog is over without a live remote: it is forgotten, key and all.
    pub(crate) fn give_up_connecting(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_connect = None;
        self.connect_clip = None;
        self.delete_space(id, window, cx);
    }

    /// The dialog was closed before the remote was live.
    pub(crate) fn cancel_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_connect.clone() {
            self.give_up_connecting(&id, window, cx);
        }
        self.connect_done = None;
    }

    /// Connect to a Templum…: the dialog that turns a code into a remote workspace.
    pub(crate) fn connect_templum(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::connect_dialog::open(self, window, cx);
    }

    /// Asks whether the system's keyring answers; `keyring` holds the answer when it comes.
    pub(crate) fn probe_keyring(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mirror(window, cx).probe();
    }

    /// Chooses which terminals of each remote workspace the client streams: the ones on screen at
    /// Full detail of the workspace shown while it is live, nearest the canvas's center first,
    /// and always the focused one; none for a workspace not shown or not live. Called as the
    /// canvas is drawn, it chooses at most every [`STREAM_EVERY`] and only tells the client when
    /// the set changed. The time a choice waits for, if one does (`stream_at` repaints then).
    pub(super) fn stream_terminals(&mut self, now: Instant) -> Option<Instant> {
        let mirror = self.mirror.as_ref()?;
        let mut due: Option<Instant> = None;
        let vp = self.api.viewport;
        let font = self.cell.font_size;
        let full = templum_canvas::level(font, self.space.rest) == templum_canvas::Level::Full;
        let shown = std::iter::once((true, &mut self.space))
            .chain(self.others.iter_mut().map(|s| (false, s)));
        for (shown, space) in shown {
            let Some(mut remote) = space.remote.take() else {
                continue;
            };
            let live = shown && remote.status == Status::Live;
            let mut set: Vec<(String, BlockId)> = Vec::new();
            if live {
                if let Some(at) = remote
                    .chosen
                    .filter(|at| now.saturating_duration_since(*at) < STREAM_EVERY)
                {
                    due = Some(due.map_or(at + STREAM_EVERY, |d| d.min(at + STREAM_EVERY)));
                    space.remote = Some(remote);
                    continue;
                }
                remote.chosen = Some(now);
                set = space.streamed_set(&remote, vp, full);
            }
            let mut same: Vec<_> = set.clone();
            same.sort();
            if same != remote.streaming {
                remote.streaming = same;
                let feeders = set
                    .into_iter()
                    .filter_map(|(home, id)| Some((home, space.terminal(id)?.term.feeder())))
                    .collect();
                mirror.send(&space.id, Command::Terminals(feeders));
            }
            space.remote = Some(remote);
        }
        due
    }

    /// Repaints at `due` to make the stream choice that waits, one timer at a time: the canvas
    /// is drawn for the output's rate, not for a choice that only needs a moment.
    pub(super) fn stream_at(&mut self, due: Instant, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.stream_waiting, true) {
            return;
        }
        cx.spawn_in(window, async move |ws, cx| {
            cx.background_executor()
                .timer(due.saturating_duration_since(Instant::now()))
                .await;
            ws.update(cx, |ws, cx| {
                ws.stream_waiting = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The wheel over remote terminal `id`: scrolling back isn't mirrored yet, which is said once
    /// per terminal. True when it was said now.
    pub(crate) fn remote_wheel(
        &mut self,
        id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(remote) = self.space.remote.as_mut() else {
            return false;
        };
        if !remote.wheel_told.insert(id) {
            return false;
        }
        show_notification(
            window,
            cx,
            Notification::info("Only the live screen is mirrored for now."),
        );
        true
    }

    /// Tells remote `id` something; nothing without a client.
    pub(crate) fn mirror_send(&self, id: &str, command: Command) {
        if let Some(mirror) = &self.mirror {
            mirror.send(id, command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize) -> Vec<(String, Rect)> {
        (0..n)
            .map(|i| {
                (
                    format!("b{i:02}"),
                    Rect::new(i as f32 * 100.0, 0.0, 50.0, 50.0),
                )
            })
            .collect()
    }

    #[test]
    fn the_nearest_twelve_stream_and_the_focused_one_always() {
        let blocks = line(15);
        // The center is the 8th block's (index 7).
        let center = Point::new(7.0 * 100.0 + 25.0, 25.0);
        let near = streamed(&blocks, center, None);
        assert_eq!(near.len(), MOST_STREAMS);
        assert_eq!(near[0], "b07", "the nearest first");
        for far in ["b00", "b14"] {
            assert!(
                !near.contains(&far.to_string()),
                "{far} is too far: {near:?}"
            );
        }
        // The 15th is focused: first, and the 12th nearest is out.
        let focused = streamed(&blocks, center, Some("b14"));
        assert_eq!(focused.len(), MOST_STREAMS);
        assert_eq!(focused[0], "b14");
        assert!(!focused.contains(&near[MOST_STREAMS - 1]), "{focused:?}");
        // A focused block that is not among them adds nothing.
        assert_eq!(streamed(&blocks, center, Some("zzz")), near);
    }

    #[test]
    fn fit_zoom_takes_the_largest_step_that_fits() {
        let cell = Size::new(8.0, 16.0);
        let fit = |zoom, w, h| fit_zoom(zoom, Size::new(w, h), (80, 24), cell);
        assert_eq!(fit(1.0, 640.0, 384.0), 1.0);
        assert_eq!(fit(1.0, 320.0, 192.0), 0.5);
        // k = 330 / 640 = 0.515…, and 2 × k = 1.03: the step below it.
        assert_eq!(fit(2.0, 330.0, 400.0), 1.0);
        assert_eq!(
            fit(1.0, 10.0, 10.0),
            0.25,
            "the smallest step when none fits"
        );
        // A bigger block than the grid needs does not stretch past the largest step.
        assert_eq!(fit(3.0, 6400.0, 3840.0), 3.0);
    }

    fn remote_at(url: &str, status: Status) -> RemoteSpace {
        let mut remote = RemoteSpace::new(RemoteEntry {
            url: url.into(),
            way: "public".into(),
            ..RemoteEntry::default()
        });
        remote.home = Some(WorkspaceView {
            name: "API".into(),
            ..WorkspaceView::default()
        });
        remote.me = Some(MeView {
            computer: "winx-desktop".into(),
            scope: templum_api::store::Scope::Full,
            ..MeView::default()
        });
        remote.status = status;
        remote
    }

    #[test]
    fn every_state_has_its_words() {
        let (now, wall) = (Instant::now(), SystemTime::now());
        let cut = |why| remote_at("https://templum.example.com", Status::Cut(why));
        let says = |remote: &RemoteSpace, text: &str, acts: &[Act], dim: bool| {
            let banner = banner_words(remote, now, wall);
            assert_eq!(
                (banner.text.as_str(), banner.acts.as_slice(), banner.dim),
                (text, acts, dim)
            );
        };
        let mut live = remote_at("https://templum.example.com", Status::Live);
        says(&live, "Mirror of API on winx-desktop · Full", &[], false);
        live.latency = Some(Duration::from_millis(84));
        says(
            &live,
            "Mirror of API on winx-desktop · Full · 84 ms",
            &[],
            false,
        );
        live.me.as_mut().unwrap().expires = Some(
            wall.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 2 * 86_400
                + 60,
        );
        says(
            &live,
            "Mirror of API on winx-desktop · Full · 84 ms · ends in 3 days: share again on winx-desktop",
            &[],
            false,
        );
        live.me.as_mut().unwrap().expires = Some(
            wall.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 40 * 86_400,
        );
        says(
            &live,
            "Mirror of API on winx-desktop · Full · 84 ms",
            &[],
            false,
        );
        live.home.as_mut().unwrap().stopped = true;
        says(
            &live,
            "API is stopped on winx-desktop.",
            &[Act::StartIt],
            false,
        );
        says(
            &remote_at("https://templum.example.com", Status::Connecting),
            "Connecting…",
            &[],
            false,
        );
        let mut reconnecting = remote_at(
            "https://templum.example.com",
            Status::Reconnecting {
                attempt: 3,
                next: Duration::from_secs(10),
                live_at: Some(wall - Duration::from_secs(120)),
            },
        );
        reconnecting.since = now - Duration::from_secs(2);
        says(
            &reconnecting,
            "Reconnecting to winx-desktop… try 3, next in 8 s. Live 2 min ago.",
            &[Act::TryNow],
            true,
        );
        reconnecting.status = Status::Reconnecting {
            attempt: 1,
            next: Duration::from_secs(1),
            live_at: None,
        };
        says(
            &reconnecting,
            "Reconnecting to winx-desktop… try 1, next in 0 s.",
            &[Act::TryNow],
            true,
        );
        says(
            &cut(Why::Expired),
            "This share of API expired. Share again on winx-desktop, then Paste a new code…",
            &[Act::Paste, Act::Forget],
            true,
        );
        says(
            &cut(Why::Revoked),
            "winx-desktop revoked this share. Share again there, then Paste a new code…",
            &[Act::Paste, Act::Forget],
            true,
        );
        says(
            &cut(Why::Deleted),
            "API was deleted on winx-desktop.",
            &[Act::Forget],
            true,
        );
        says(
            &cut(Why::Replaced),
            "Another computer connected with this share's code, so this one stopped. Each computer needs its own share.",
            &[Act::ConnectHere, Act::Forget],
            true,
        );
        says(
            &cut(Why::PinChanged),
            "winx-desktop's certificate changed since you connected. If you made a New certificate there, share again and paste the new code. If you didn't, someone may be between the two computers: don't connect.",
            &[Act::Paste, Act::Forget],
            true,
        );
        let invalid = |what: &str| {
            format!(
                "templum.example.com's certificate isn't valid: {what}. Fix it where the tunnel or proxy runs, then Try again."
            )
        };
        for (bad, what) in [
            (client::Bad::Expired, "it expired"),
            (client::Bad::Name, "it is for another name"),
            (
                client::Bad::Issuer,
                "no authority this computer trusts signed it",
            ),
        ] {
            says(
                &cut(Why::Certificate(bad)),
                &invalid(what),
                &[Act::TryNow],
                true,
            );
        }
        says(
            &cut(Why::Certificate(client::Bad::Alone)),
            &format!(
                "{} If it is Templum's own, share Directly instead: it is pinned.",
                invalid("no authority this computer trusts signed it")
            ),
            &[Act::TryNow],
            true,
        );
        says(
            &cut(Why::AddedAuthority),
            "HTTPS to templum.example.com is signed by an authority this computer added, not a public one. A company proxy or security program may be reading this connection: your key, what home's terminals show and what you type. With a Full share it could run commands on winx-desktop. Connect only if you added that authority yourself (a home lab, your own proxy).",
            &[Act::DontConnect, Act::ConnectAnyway],
            true,
        );
        says(
            &cut(Why::Access),
            "Cloudflare Access stands in front of templum.example.com. Add a service token in Edit… ▸ More…, and allow it in the Access policy.",
            &[Act::TryNow],
            true,
        );
        says(
            &cut(Why::NotTemplum {
                status: 404,
                kind: "text/html".into(),
            }),
            "templum.example.com didn't answer as a Templum (HTTP 404, text/html). Check that the tunnel points at port 7411.",
            &[Act::TryNow],
            true,
        );
        // What answers at the code's URL names its own type: only the start of it is shown.
        let long = banner_words(
            &cut(Why::NotTemplum {
                status: 200,
                kind: "x".repeat(400_000),
            }),
            now,
            wall,
        );
        assert!(long.text.chars().count() < 200, "{}", long.text.len());
        says(
            &cut(Why::TooOld),
            "winx-desktop runs a Templum that can't be mirrored: update it.",
            &[Act::TryNow],
            true,
        );
        for (status, because) in [
            (407, "407: it asks for a login Templum can't give"),
            (403, "HTTP 403"),
        ] {
            says(
                &cut(Why::Proxy {
                    proxy: "http://proxy.corp:8080".into(),
                    status,
                }),
                &format!("The proxy proxy.corp:8080 refused the connection ({because})."),
                &[Act::TryNow],
                true,
            );
        }
        says(
            &cut(Why::Keyring),
            "Templum couldn't read this connection's key from the keyring. Try again, or Paste a new code…",
            &[Act::TryNow, Act::Paste],
            true,
        );
        says(
            &remote_at("https://templum.example.com", Status::Disconnected),
            "Disconnected. Connect",
            &[Act::Connect],
            true,
        );
        // A pinned remote shows host and port; a scheme's own port is left out.
        let pinned = remote_at("https://192.168.1.5:7411", Status::Cut(Why::Access));
        assert!(
            banner_words(&pinned, now, wall)
                .text
                .contains("in front of 192.168.1.5:7411.")
        );
        assert_eq!(
            host_of("https://templum.example.com:443/x"),
            "templum.example.com"
        );
    }
}
