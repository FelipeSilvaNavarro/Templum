//! What the API shows as data (docs/specs/api.md, Resources): the state, as one JSON document, the
//! rules for what a workspace-limited token sees of it, and the comparison that finds what changed
//! between two of them. Plain data: `crates/templum` builds it from the workspaces.

use crate::store::{Scope, Token, Way};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Everything the API shows. Times are Unix seconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub app: App,
    pub workspaces: Vec<Arc<WorkspaceView>>,
    pub blocks: Vec<Arc<BlockView>>,
    pub wires: Vec<Arc<WireView>>,
    pub requests: Vec<Arc<RequestView>>,
    pub boards: Vec<Arc<BoardView>>,
    pub cards: Vec<Arc<CardView>>,
    pub automations: Vec<Arc<AutomationView>>,
    /// The shares of workspaces: a token for all workspaces sees every one, a limited token only
    /// the share it is. Left out by homes older than shares.
    #[serde(default)]
    pub shares: Vec<Arc<ShareView>>,
    pub usage: Option<UsageView>,
    /// What the shown workspace's canvas shows; None before its first paint.
    pub view: Option<ViewView>,
}

/// The world rectangle a workspace's canvas shows, and its zoom (docs/specs/api.md, Batch 2).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewView {
    pub workspace: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub zoom: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct App {
    /// Templum's version.
    pub version: String,
    /// The newest API version served.
    pub api: u32,
    /// The workspace on screen.
    pub shown: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceView {
    pub id: String,
    pub name: String,
    pub folder: Option<String>,
    pub color: Option<String>,
    pub shown: bool,
    pub stopped: bool,
    /// Its Lucide icon name, or None for its initials.
    #[serde(default)]
    pub icon: Option<String>,
    /// Where a mirrored workspace comes from; left out for a local one. A remote workspace's
    /// blocks are never in the state (docs/specs/templum-to-templum.md, What the mirror's API shows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteView>,
}

/// A remote workspace: the other computer it mirrors and how the connection stands.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RemoteView {
    /// The other computer's name, once it said.
    pub computer: Option<String>,
    /// The address's host (and port), as the code gave it.
    pub host: String,
    /// `pinned`, `public` or `loopback`.
    pub way: String,
    /// `connecting`, `live`, `reconnecting`, `cut`, `refused` or `disconnected`.
    pub status: String,
    pub latency_ms: Option<u64>,
}

/// A workspace shared with another Templum (docs/specs/templum-to-templum.md): an API token
/// limited to it, with how its other computer reaches this one. Never the token's text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShareView {
    /// `s<token id>`.
    pub id: String,
    /// The shared workspace's id, even when that workspace is gone.
    pub workspace: String,
    pub name: String,
    pub scope: Scope,
    pub created: u64,
    pub expires: Option<u64>,
    pub way: Way,
    pub address: String,
    pub last: Option<LastView>,
    /// An event stream of its token is open now.
    pub mirrored: bool,
}

/// A token's newest use.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LastView {
    pub at: u64,
    pub addr: String,
}

/// Who a token is, as `GET /v1/me` answers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeView {
    pub name: String,
    pub scope: Scope,
    /// The workspaces it sees; None: all of them. Empty: the one it was made for is gone.
    pub workspaces: Option<Vec<String>>,
    pub expires: Option<u64>,
    /// Its share's id; None for a token Settings made.
    pub share: Option<String>,
    /// This computer's name.
    pub computer: String,
    pub version: String,
    pub api: u32,
}

impl MeView {
    pub fn of(token: &Token, computer: &str) -> MeView {
        MeView {
            name: token.name.clone(),
            scope: token.scope,
            workspaces: token.workspaces.clone(),
            expires: token.expires,
            share: token.share_id(),
            computer: computer.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            api: 1,
        }
    }
}

/// A terminal's screen, as a terminal stream's `frame` event carries it (docs/specs/api.md,
/// Batch 2): clients decode it, a mirror draws it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameView {
    pub size: SizeView,
    /// None while the program hides it.
    pub cursor: Option<CursorView>,
    /// The program's own title (OSC 0/2).
    pub title: Option<String>,
    /// Every row is here; otherwise only the rows that changed.
    pub whole: bool,
    pub rows: Vec<RowView>,
    /// What the program took of the keyboard and mouse; every frame carries it, and a change of
    /// it alone is a frame with no rows. Left out by homes older than this field.
    #[serde(default)]
    pub modes: ModesView,
}

/// What a program took at home, each present only when true.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModesView {
    #[serde(default, skip_serializing_if = "is_false")]
    pub full_screen: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub mouse: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub kitty_keys: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SizeView {
    pub cols: usize,
    pub rows: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CursorView {
    pub row: usize,
    pub col: usize,
    /// `block`, `underline` or `beam`.
    pub shape: String,
    pub blinking: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RowView {
    pub y: usize,
    /// Trailing blanks trimmed; an empty row has none.
    pub runs: Vec<RunView>,
}

fn is_false(flag: &bool) -> bool {
    !*flag
}

/// Text of one style. A color is absent for the theme's default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunView {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dim: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inverse: bool,
}

/// A palette slot 0–255, which the viewer's palette draws, or `"#rrggbb"`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    Index(u8),
    Rgb(String),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockView {
    pub id: String,
    pub workspace: String,
    pub kind: String,
    pub title: String,
    pub folder: Option<String>,
    pub rect: Rect,
    /// Back to front.
    pub z: usize,
    pub opened_by: Option<String>,
    /// The board a kanban block shows.
    pub board: Option<u64>,
    pub agent: Option<AgentView>,
    /// A terminal that takes no input (a subagent's transcript); left out otherwise.
    #[serde(default, skip_serializing_if = "is_false")]
    pub read_only: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentView {
    pub agent: String,
    /// The Agents view's: `needs_you`, `starting`, `waiting_subagents`, `working`, `done`, `idle`.
    pub state: String,
    pub asks: Option<String>,
    pub subagents: usize,
    /// When it last changed kind.
    pub since: Option<i64>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub usage: Option<SessionView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    pub context_percent: Option<f32>,
    pub tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WireView {
    pub a: String,
    pub b: String,
    pub label: Option<String>,
}

impl WireView {
    /// A wire's id on the stream: its two ends sorted, joined by `-`.
    pub fn id(&self) -> String {
        if self.a <= self.b {
            format!("{}-{}", self.a, self.b)
        } else {
            format!("{}-{}", self.b, self.a)
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestView {
    /// `r<n>` for the inbox's, `a<n>` for the Templum Agent's approvals.
    pub id: String,
    /// `question`, `plan`, `connect`, `remove_worktree`, `merge_worktree` or `approval`.
    pub kind: String,
    pub block: Option<String>,
    pub workspace: Option<String>,
    pub since: i64,
    pub summary: String,
    pub why: Why,
    pub questions: Vec<QuestionView>,
    pub plan: Option<PlanView>,
    /// A yes/no request's two words.
    pub buttons: Option<[String; 2]>,
    pub stopped: bool,
    /// What answering it takes.
    pub scope: Scope,
    pub tool: Option<String>,
    pub said: Option<String>,
    pub details: Option<String>,
    pub facts: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Why {
    pub card: Option<CardRef>,
    pub task: Option<String>,
    pub agent: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CardRef {
    pub id: u64,
    pub title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuestionView {
    pub question: String,
    pub header: Option<String>,
    pub multi: bool,
    pub options: Vec<OptionView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OptionView {
    pub label: String,
    pub description: Option<String>,
    pub preview: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanView {
    pub title: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BoardView {
    pub id: u64,
    pub name: String,
    pub columns: Vec<ColumnView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ColumnView {
    pub id: u64,
    pub title: String,
    /// The ids of its cards, in order.
    pub cards: Vec<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CardView {
    pub id: u64,
    pub board: u64,
    pub column: u64,
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub priority: Option<String>,
    pub assignee: Option<Assignee>,
    pub checklist: Vec<ItemView>,
    pub comments: Vec<CommentView>,
    /// Names and sizes, never paths.
    pub attachments: Vec<AttachmentView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Assignee {
    pub block: Option<String>,
    pub agent: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemView {
    pub id: u64,
    pub text: String,
    pub done: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CommentView {
    pub number: u64,
    pub by: String,
    pub text: String,
    pub at: i64,
    pub edited: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AttachmentView {
    pub name: String,
    pub size: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AutomationView {
    pub id: u64,
    pub name: String,
    pub on: bool,
    pub workspace: Option<String>,
    pub when: Value,
    #[serde(rename = "do")]
    pub act: Value,
    pub max_runs: Option<u32>,
    pub cooldown: Option<u32>,
    pub runs: u32,
    pub last: Option<Value>,
    pub next: Option<i64>,
    pub sentence: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageView {
    pub limits: Vec<LimitsView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LimitsView {
    pub agent: String,
    pub at: u64,
    pub windows: Vec<WindowView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowView {
    pub minutes: u32,
    pub used_percent: f32,
    pub resets_at: u64,
    pub scope: Option<String>,
}

/// The one happening on the stream that isn't state: a task the API handed ended, an agent needs
/// its person, a watch matched.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub text: String,
    pub block: Option<String>,
    pub workspace: Option<String>,
}

/// Who is asking: a token, as far as seeing goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Caller {
    pub token: u64,
    pub name: String,
    pub scope: Scope,
    /// The workspaces it may see; None: all of them.
    pub workspaces: Option<Vec<String>>,
}

impl Caller {
    pub fn of(token: &Token) -> Caller {
        Caller {
            token: token.id,
            name: token.name.clone(),
            scope: token.scope,
            workspaces: token.workspaces.clone(),
        }
    }

    /// Whether a thing in `workspace` is in view: an unlimited caller sees everything, a limited
    /// one only what is in its list.
    pub fn sees(&self, workspace: Option<&str>) -> bool {
        match (&self.workspaces, workspace) {
            (None, _) => true,
            (Some(list), Some(workspace)) => list.iter().any(|w| w == workspace),
            (Some(_), None) => false,
        }
    }
}

impl State {
    /// The state as `caller` sees it (docs/specs/api.md, Workspaces a token may see). An unlimited
    /// caller gets it whole.
    pub fn visible(&self, caller: &Caller) -> State {
        if caller.workspaces.is_none() {
            return self.clone();
        }
        let blocks: Vec<Arc<BlockView>> = self
            .blocks
            .iter()
            .filter(|block| caller.sees(Some(&block.workspace)))
            .cloned()
            .collect();
        let ids: HashSet<&str> = blocks.iter().map(|block| block.id.as_str()).collect();
        let shown_boards: HashSet<u64> = blocks.iter().filter_map(|block| block.board).collect();
        State {
            app: App {
                shown: self
                    .app
                    .shown
                    .clone()
                    .filter(|shown| caller.sees(Some(shown))),
                ..self.app.clone()
            },
            workspaces: self
                .workspaces
                .iter()
                .filter(|workspace| caller.sees(Some(&workspace.id)))
                .cloned()
                .collect(),
            wires: self
                .wires
                .iter()
                .filter(|wire| ids.contains(wire.a.as_str()) && ids.contains(wire.b.as_str()))
                .cloned()
                .collect(),
            requests: self
                .requests
                .iter()
                .filter(|request| {
                    request.kind != "approval"
                        && request
                            .block
                            .as_deref()
                            .is_some_and(|block| ids.contains(block))
                })
                .cloned()
                .collect(),
            boards: self
                .boards
                .iter()
                .filter(|board| shown_boards.contains(&board.id))
                .cloned()
                .collect(),
            cards: self
                .cards
                .iter()
                .filter(|card| shown_boards.contains(&card.board))
                .cloned()
                .collect(),
            automations: self
                .automations
                .iter()
                .filter(|automation| caller.sees(automation.workspace.as_deref()))
                .cloned()
                .collect(),
            shares: self
                .shares
                .iter()
                .filter(|share| share.id == format!("s{}", caller.token))
                .cloned()
                .collect(),
            usage: None,
            view: self
                .view
                .clone()
                .filter(|view| caller.sees(Some(&view.workspace))),
            blocks,
        }
    }
}

/// The most items a mirrored collection may hold: home sends more than a person uses, and a
/// state past it is "too large", as a stream's event past its limit is.
const COLLECTION_MOST: usize = 10_000;

/// Why an event of the stream couldn't be applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// Not what the stream is meant to send, in words.
    Malformed(String),
    /// More than a mirror keeps in memory.
    TooLarge,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::Malformed(why) => f.write_str(why),
            ApplyError::TooLarge => f.write_str("home's state is larger than Templum keeps"),
        }
    }
}

impl State {
    /// The state a `snapshot` event holds. Parsing is the slow part of a snapshot: do it before
    /// taking any lock, then hand the result to [`State::replace`].
    pub fn from_snapshot(data: &Value) -> Result<State, ApplyError> {
        let new = State::deserialize(data)
            .map_err(|e| ApplyError::Malformed(format!("a malformed snapshot event: {e}")))?;
        let crowded = [
            new.workspaces.len(),
            new.blocks.len(),
            new.wires.len(),
            new.requests.len(),
            new.boards.len(),
            new.cards.len(),
            new.automations.len(),
            new.shares.len(),
        ]
        .into_iter()
        .any(|n| n > COLLECTION_MOST);
        if crowded {
            return Err(ApplyError::TooLarge);
        }
        Ok(new)
    }

    /// Becomes `new` (see [`State::from_snapshot`]). Whether anything changed.
    pub fn replace(&mut self, new: State) -> bool {
        std::mem::replace(self, new) != *self
    }

    /// Applies one event of the stream to a copy of the state a mirror keeps: `snapshot` replaces
    /// it; a collection's event is a put (replace the item with that id, or add it last) or a
    /// delete; `view` sets the camera. Whether the state changed; any other event changes
    /// nothing, and a malformed one says what was wrong.
    pub fn apply(&mut self, event: &str, data: &Value) -> Result<bool, ApplyError> {
        State::parse(event, data)?(self)
    }

    /// Whether [`State::parse`] keeps the items of this kind of event, by id (the others, a kind
    /// this version doesn't know or the view, keep none).
    pub(crate) fn keeps_items(event: &str) -> bool {
        matches!(
            event,
            "workspace"
                | "block"
                | "wire"
                | "request"
                | "board"
                | "card"
                | "automation"
                | "share"
                | "usage"
        )
    }

    /// Reads one event without touching any state: all the parsing, and none of the changing. The
    /// change it returns is quick, so it can be made under a lock the UI thread takes.
    pub fn parse(event: &str, data: &Value) -> Result<Change, ApplyError> {
        match event {
            "snapshot" => {
                let new = State::from_snapshot(data)?;
                Ok(Box::new(move |state| Ok(state.replace(new))))
            }
            "workspace" => edit(event, data, |s| &mut s.workspaces, |w| w.id.clone()),
            "block" => edit(event, data, |s| &mut s.blocks, |b| b.id.clone()),
            "wire" => edit(event, data, |s| &mut s.wires, WireView::id),
            "request" => edit(event, data, |s| &mut s.requests, |r| r.id.clone()),
            "board" => edit(event, data, |s| &mut s.boards, |b| b.id.to_string()),
            "card" => edit(event, data, |s| &mut s.cards, |c| c.id.to_string()),
            "automation" => edit(event, data, |s| &mut s.automations, |a| a.id.to_string()),
            "share" => edit(event, data, |s| &mut s.shares, |s| s.id.clone()),
            "usage" => {
                // At most one, with the id `usage`.
                let one = Edit::<UsageView>::read(event, data, |_| "usage".into())?;
                Ok(Box::new(move |state| {
                    let mut items: Vec<Arc<UsageView>> =
                        state.usage.take().map(Arc::new).into_iter().collect();
                    let changed = one.run(&mut items, |_| "usage".into());
                    state.usage = items.pop().map(Arc::unwrap_or_clone);
                    changed
                }))
            }
            "view" => {
                let new = match data {
                    Value::Null => None,
                    data => Some(from_value(event, data)?),
                };
                Ok(Box::new(move |state| {
                    Ok(std::mem::replace(&mut state.view, new) != state.view)
                }))
            }
            _ => Ok(Box::new(|_| Ok(false))),
        }
    }
}

/// What an event does to a state: made by [`State::parse`], whether the state changed.
pub type Change = Box<dyn FnOnce(&mut State) -> Result<bool, ApplyError>>;

fn from_value<T: serde::de::DeserializeOwned>(event: &str, value: &Value) -> Result<T, ApplyError> {
    T::deserialize(value)
        .map_err(|e| ApplyError::Malformed(format!("a malformed {event} event: {e}")))
}

/// A put or a delete of the item with an id, read from an event.
struct Edit<T> {
    id: String,
    /// The whole item for a put, None for a delete.
    put: Option<T>,
}

impl<T: serde::de::DeserializeOwned + PartialEq> Edit<T> {
    /// A put must carry an item whose own id (`id`, canonical: a number as it is printed) is the
    /// event's: lookups and the client's weights both go by the event's id, so a mismatch would
    /// let a hostile home pile up items the weights don't count.
    fn read(
        event: &str,
        data: &Value,
        id_of: impl Fn(&T) -> String,
    ) -> Result<Edit<T>, ApplyError> {
        let id = data
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ApplyError::Malformed(format!("a malformed {event} event: no id")))?
            .to_string();
        let put = match data.get("op").and_then(Value::as_str) {
            Some("put") => Some(from_value(
                event,
                data.get("value").unwrap_or(&Value::Null),
            )?),
            Some("delete") => None,
            _ => {
                return Err(ApplyError::Malformed(format!(
                    "a malformed {event} event: no op"
                )));
            }
        };
        if put.as_ref().is_some_and(|item| id_of(item) != id) {
            return Err(ApplyError::Malformed(format!(
                "a malformed {event} event: the item's id is not the event's"
            )));
        }
        Ok(Edit { id, put })
    }

    /// Keeps the `Arc` of an item a put equals, so a take shares what didn't change.
    fn run(self, items: &mut Vec<Arc<T>>, id: impl Fn(&T) -> String) -> Result<bool, ApplyError> {
        let Some(item) = self.put else {
            let before = items.len();
            items.retain(|old| id(old) != self.id);
            return Ok(items.len() != before);
        };
        let full = items.len() >= COLLECTION_MOST;
        match items.iter_mut().find(|old| id(old) == self.id) {
            Some(old) if **old == item => Ok(false),
            Some(old) => {
                *old = Arc::new(item);
                Ok(true)
            }
            None if full => Err(ApplyError::TooLarge),
            None => {
                items.push(Arc::new(item));
                Ok(true)
            }
        }
    }
}

/// The change an event of a collection makes: `items` says which collection, `id` an item's id.
fn edit<T: serde::de::DeserializeOwned + PartialEq + 'static>(
    event: &str,
    data: &Value,
    items: fn(&mut State) -> &mut Vec<Arc<T>>,
    id: fn(&T) -> String,
) -> Result<Change, ApplyError> {
    let edit = Edit::read(event, data, id)?;
    Ok(Box::new(move |state| edit.run(items(state), id)))
}

/// One change to the state: `value` is the whole object for a put, None for a delete.
#[derive(Clone, Debug, PartialEq)]
pub struct Delta {
    pub kind: &'static str,
    pub id: String,
    pub value: Option<Value>,
}

/// What differs between two states, per collection (workspace, block, wire, request, board, card,
/// automation, share, usage): the deletes, then the puts.
pub fn diff(old: &State, new: &State) -> Vec<Delta> {
    let mut out = Vec::new();
    compare(
        "workspace",
        &old.workspaces,
        &new.workspaces,
        |w| w.id.clone(),
        &mut out,
    );
    compare(
        "block",
        &old.blocks,
        &new.blocks,
        |b| b.id.clone(),
        &mut out,
    );
    compare("wire", &old.wires, &new.wires, |w| w.id(), &mut out);
    compare(
        "request",
        &old.requests,
        &new.requests,
        |r| r.id.clone(),
        &mut out,
    );
    compare(
        "board",
        &old.boards,
        &new.boards,
        |b| b.id.to_string(),
        &mut out,
    );
    compare(
        "card",
        &old.cards,
        &new.cards,
        |c| c.id.to_string(),
        &mut out,
    );
    compare(
        "automation",
        &old.automations,
        &new.automations,
        |a| a.id.to_string(),
        &mut out,
    );
    compare(
        "share",
        &old.shares,
        &new.shares,
        |s| s.id.clone(),
        &mut out,
    );
    compare(
        "usage",
        old.usage.as_slice(),
        new.usage.as_slice(),
        |_| "usage".into(),
        &mut out,
    );
    out
}

/// Whether two collections are the same items: as many, each the very same `Arc`, in order.
pub fn same<T>(a: &[Arc<T>], b: &[Arc<T>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| Arc::ptr_eq(x, y))
}

/// One collection's deletes (ids gone), then its puts (ids new or changed).
fn compare<T: PartialEq + Serialize>(
    kind: &'static str,
    old: &[T],
    new: &[T],
    id: impl Fn(&T) -> String,
    out: &mut Vec<Delta>,
) {
    let before: HashMap<String, &T> = old.iter().map(|item| (id(item), item)).collect();
    let after: HashSet<String> = new.iter().map(&id).collect();
    for item in old {
        let id = id(item);
        if !after.contains(&id) {
            out.push(Delta {
                kind,
                id,
                value: None,
            });
        }
    }
    for item in new {
        let id = id(item);
        if before.get(&id) != Some(&item) {
            let value = serde_json::to_value(item).unwrap_or(Value::Null);
            out.push(Delta {
                kind,
                id,
                value: Some(value),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn block(id: &str, workspace: &str, kind: &str) -> Arc<BlockView> {
        Arc::new(BlockView {
            id: id.into(),
            workspace: workspace.into(),
            kind: kind.into(),
            title: id.into(),
            ..Default::default()
        })
    }

    fn request(id: &str, kind: &str, block: &str) -> Arc<RequestView> {
        Arc::new(RequestView {
            id: id.into(),
            kind: kind.into(),
            block: (!block.is_empty()).then(|| block.into()),
            ..Default::default()
        })
    }

    fn automation(id: u64, workspace: Option<&str>) -> Arc<AutomationView> {
        Arc::new(AutomationView {
            id,
            workspace: workspace.map(Into::into),
            ..Default::default()
        })
    }

    /// w3: `b1` (a terminal with a working agent) and `k1` (a kanban showing board 3, card 40);
    /// w4: `b2`; a wire b1–k1; requests r1 (of b1), r2 (of b2), approval a1; boards 3 and 5 (shown
    /// by none); automations for w3, w4 and all workspaces; a usage.
    fn two_workspaces() -> State {
        let mut b1 = BlockView::clone(&block("b1", "w3", "terminal"));
        b1.agent = Some(AgentView {
            agent: "claude".into(),
            state: "working".into(),
            ..Default::default()
        });
        let mut k1 = BlockView::clone(&block("k1", "w3", "kanban"));
        k1.board = Some(3);
        State {
            app: App {
                version: "0.1.5".into(),
                api: 1,
                shown: Some("w4".into()),
            },
            workspaces: ["w3", "w4"]
                .map(|id| {
                    Arc::new(WorkspaceView {
                        id: id.into(),
                        name: id.into(),
                        ..Default::default()
                    })
                })
                .into(),
            blocks: vec![Arc::new(b1), Arc::new(k1), block("b2", "w4", "terminal")],
            wires: vec![Arc::new(WireView {
                a: "b1".into(),
                b: "k1".into(),
                label: None,
            })],
            requests: vec![
                request("r1", "question", "b1"),
                request("r2", "question", "b2"),
                request("a1", "approval", ""),
            ],
            boards: [3, 5]
                .map(|id| {
                    Arc::new(BoardView {
                        id,
                        name: format!("B{id}"),
                        columns: vec![],
                    })
                })
                .into(),
            cards: vec![Arc::new(CardView {
                id: 40,
                board: 3,
                column: 7,
                ..Default::default()
            })],
            automations: vec![
                automation(1, Some("w3")),
                automation(2, Some("w4")),
                automation(3, None),
            ],
            shares: vec![],
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

    fn caller(workspaces: &[&str]) -> Caller {
        Caller {
            token: 1,
            name: "w".into(),
            scope: Scope::Read,
            workspaces: Some(workspaces.iter().map(|w| w.to_string()).collect()),
        }
    }

    fn said(deltas: &[Delta]) -> Vec<(&str, &str, bool)> {
        deltas
            .iter()
            .map(|d| (d.kind, d.id.as_str(), d.value.is_some()))
            .collect()
    }

    /// A delta as the stream sends it: the event's name and its data.
    fn framed(delta: Delta) -> (&'static str, Value) {
        let data = match delta.value {
            Some(value) => json!({"op": "put", "id": delta.id, "value": value}),
            None => json!({"op": "delete", "id": delta.id}),
        };
        (delta.kind, data)
    }

    #[test]
    fn a_mirror_applies_what_the_stream_sends() {
        let mut s = two_workspaces();
        let rect = |s: &State, id: &str| s.blocks.iter().find(|b| b.id == id).unwrap().rect.x;

        let mut moved = BlockView::clone(&s.blocks[0]);
        moved.rect.x = 77.0;
        let put = json!({"op": "put", "id": "b1", "value": moved});
        assert_eq!(s.apply("block", &put), Ok(true));
        assert_eq!(rect(&s, "b1"), 77.0);
        assert_eq!(
            s.apply("block", &put),
            Ok(false),
            "the same again changes nothing"
        );

        let added = json!({"op": "put", "id": "zz", "value": block("zz", "w3", "note")});
        assert_eq!(s.apply("block", &added), Ok(true));
        assert_eq!(s.blocks.last().unwrap().id, "zz");

        assert_eq!(
            s.apply("wire", &json!({"op": "delete", "id": "b1-k1"})),
            Ok(true)
        );
        assert!(s.wires.is_empty());
        assert_eq!(
            s.apply("wire", &json!({"op": "delete", "id": "b1-k1"})),
            Ok(false)
        );

        let share = ShareView {
            id: "s4".into(),
            ..Default::default()
        };
        assert_eq!(
            s.apply("share", &json!({"op": "put", "id": "s4", "value": share})),
            Ok(true)
        );
        assert_eq!(s.shares, [Arc::new(share)]);

        assert_eq!(
            s.apply("usage", &json!({"op": "delete", "id": "usage"})),
            Ok(true)
        );
        assert_eq!(s.usage, None);
        assert_eq!(
            s.apply(
                "usage",
                &json!({"op": "put", "id": "usage", "value": UsageView::default()})
            ),
            Ok(true)
        );
        assert_eq!(s.usage, Some(UsageView::default()));

        assert_eq!(s.apply("view", &Value::Null), Ok(true));
        assert_eq!(s.view, None);
        let other = State {
            app: App {
                version: "9".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            s.apply("snapshot", &serde_json::to_value(&other).unwrap()),
            Ok(true)
        );
        assert_eq!(s, other);

        assert_eq!(s.apply("notice", &json!({"text": "x"})), Ok(false));
        assert!(s.apply("block", &json!({"op": "put"})).is_err());
        assert!(
            s.apply("block", &json!({"op": "put", "id": "x", "value": 3}))
                .is_err()
        );
        assert!(s.apply("block", &json!({"op": "move", "id": "x"})).is_err());
        assert!(s.apply("snapshot", &json!([1])).is_err());
    }

    /// A put whose item has another id than the event keeps nothing: the client's weights go by
    /// the event's id, the state by the item's, so a mismatch would hide items from the weights.
    #[test]
    fn a_put_whose_item_has_another_id_is_refused() {
        let mut s = two_workspaces();
        let before = s.clone();
        // A string-id kind.
        let put = json!({"op": "put", "id": "x", "value": block("y", "w3", "note")});
        assert!(matches!(
            s.apply("block", &put),
            Err(ApplyError::Malformed(_))
        ));
        // A numeric-id kind: the item's number as it is printed must be the event's id.
        for id in ["05", "+5", "6", "x"] {
            let put =
                json!({"op": "put", "id": id, "value": BoardView { id: 5, ..Default::default() }});
            assert!(
                matches!(s.apply("board", &put), Err(ApplyError::Malformed(_))),
                "{id}"
            );
        }
        let put = json!({"op": "put", "id": "usage2", "value": UsageView::default()});
        assert!(matches!(
            s.apply("usage", &put),
            Err(ApplyError::Malformed(_))
        ));
        assert_eq!(s, before, "nothing was kept");
        // The same id, canonically, is accepted.
        let put =
            json!({"op": "put", "id": "5", "value": BoardView { id: 5, ..Default::default() }});
        assert_eq!(s.apply("board", &put), Ok(true));

        // Bounded: however many mismatched puts come, the state doesn't grow.
        let mut s = State::default();
        for i in 0..50 {
            let put =
                json!({"op": "put", "id": "x", "value": block(&format!("y{i}"), "w3", "note")});
            let _ = s.apply("block", &put);
        }
        assert!(s.blocks.is_empty());
    }

    #[test]
    fn an_event_is_read_before_any_state_is_touched() {
        let put = json!({"op": "put", "id": "zz", "value": block("zz", "w3", "note")});
        let change = State::parse("block", &put).unwrap();
        // The parsed change is all that is left to do under a lock, and it is made once on any state.
        let mut s = two_workspaces();
        let before = s.blocks.len();
        assert_eq!(change(&mut s), Ok(true));
        assert_eq!(s.blocks.len(), before + 1);
        assert!(State::parse("block", &json!({"op": "put", "id": "x", "value": 3})).is_err());
        assert!(State::parse("block", &json!({"op": "move", "id": "x"})).is_err());
        assert!(State::parse("snapshot", &json!([1])).is_err());
        assert_eq!(
            State::parse("notice", &json!({})).unwrap()(&mut s),
            Ok(false)
        );
    }

    #[test]
    fn a_state_past_the_caps_is_too_large() {
        let blocks = |n: usize| -> Vec<Arc<BlockView>> {
            (0..n)
                .map(|i| block(&format!("b{i}"), "w3", "note"))
                .collect()
        };
        let mut s = State {
            blocks: blocks(COLLECTION_MOST),
            ..Default::default()
        };
        let added = json!({"op": "put", "id": "zz", "value": block("zz", "w3", "note")});
        assert_eq!(s.apply("block", &added), Err(ApplyError::TooLarge));
        assert_eq!(s.blocks.len(), COLLECTION_MOST);
        let again = json!({"op": "put", "id": "b0", "value": block("b0", "w3", "note")});
        assert_eq!(
            s.apply("block", &again),
            Ok(false),
            "an id it has is no growth"
        );

        let big = State {
            blocks: blocks(COLLECTION_MOST + 1),
            ..Default::default()
        };
        let snapshot = serde_json::to_value(&big).unwrap();
        assert_eq!(State::from_snapshot(&snapshot), Err(ApplyError::TooLarge));
        assert_eq!(s.apply("snapshot", &snapshot), Err(ApplyError::TooLarge));
        assert_eq!(s.blocks.len(), COLLECTION_MOST, "untouched");
    }

    #[test]
    fn applying_a_diff_onto_the_old_state_makes_the_new_one() {
        let old = two_workspaces();
        let mut new = old.clone();
        Arc::make_mut(&mut new.blocks[0]).rect.x = 50.0;
        new.blocks.remove(2);
        new.blocks.push(block("b9", "w4", "note"));
        new.wires.clear();
        new.cards.clear();
        new.requests.remove(0);
        Arc::make_mut(&mut new.automations[1]).on = true;
        new.usage = None;
        new.workspaces.push(Arc::new(WorkspaceView {
            id: "w9".into(),
            ..Default::default()
        }));
        let mut mirror = old.clone();
        for delta in diff(&old, &new) {
            let (event, data) = framed(delta);
            assert_eq!(mirror.apply(event, &data), Ok(true), "{event} {data}");
        }
        assert_eq!(mirror, new);
    }

    #[test]
    fn diff_puts_what_changed_and_deletes_what_went() {
        let old = two_workspaces();
        let mut new = old.clone();
        Arc::make_mut(&mut new.blocks[0]).rect.x = 50.0;
        new.cards.clear();
        let deltas = diff(&old, &new);
        assert_eq!(
            said(&deltas),
            [("block", "b1", true), ("card", "40", false)]
        );
        assert_eq!(deltas[0].value.as_ref().unwrap()["rect"]["x"], 50.0);
        assert!(diff(&new, &new).is_empty());
    }

    #[test]
    fn diff_deletes_come_before_puts_and_usage_has_its_own_id() {
        let old = two_workspaces();
        let mut new = old.clone();
        new.blocks.remove(2);
        new.blocks.push(block("b9", "w4", "note"));
        new.usage = None;
        assert_eq!(
            said(&diff(&old, &new)),
            [
                ("block", "b2", false),
                ("block", "b9", true),
                ("usage", "usage", false)
            ]
        );
        assert_eq!(
            said(&diff(&new, &old)),
            [
                ("block", "b9", false),
                ("block", "b2", true),
                ("usage", "usage", true)
            ]
        );
    }

    #[test]
    fn a_wire_is_named_by_its_sorted_ends() {
        let old = two_workspaces();
        let mut new = old.clone();
        new.wires.push(Arc::new(WireView {
            a: "k1".into(),
            b: "b2".into(),
            label: Some("x".into()),
        }));
        let deltas = diff(&old, &new);
        assert_eq!(said(&deltas), [("wire", "b2-k1", true)]);
        assert_eq!(old.wires[0].id(), "b1-k1");
        assert_eq!(
            WireView {
                a: "k1".into(),
                b: "b1".into(),
                label: None
            }
            .id(),
            "b1-k1"
        );
    }

    #[test]
    fn a_limited_caller_sees_the_view_only_of_its_workspaces() {
        let state = two_workspaces();
        let caller = |workspaces: Option<&[&str]>| Caller {
            token: 1,
            name: "t".into(),
            scope: Scope::Read,
            workspaces: workspaces.map(|w| w.iter().map(|s| s.to_string()).collect()),
        };
        let seen = state.visible(&caller(Some(&["w3"])));
        assert_eq!(seen.view.as_ref().map(|v| v.zoom), Some(1.0));
        assert_eq!(state.visible(&caller(Some(&["w4"]))).view, None);
        assert_eq!(state.visible(&caller(None)), state);
    }

    #[test]
    fn a_limited_caller_sees_only_its_workspaces() {
        let state = two_workspaces();
        let seen = state.visible(&caller(&["w3"]));
        let names = |ids: Vec<&str>| ids.into_iter().map(String::from).collect::<Vec<_>>();
        assert_eq!(
            seen.workspaces
                .iter()
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            ["w3"]
        );
        assert_eq!(
            names(seen.blocks.iter().map(|b| b.id.as_str()).collect()),
            ["b1", "k1"]
        );
        assert_eq!(
            seen.wires.iter().map(|w| w.id()).collect::<Vec<_>>(),
            ["b1-k1"]
        );
        assert_eq!(
            seen.requests
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["r1"]
        );
        assert_eq!(seen.boards.iter().map(|b| b.id).collect::<Vec<_>>(), [3]);
        assert_eq!(seen.cards.iter().map(|c| c.id).collect::<Vec<_>>(), [40]);
        assert_eq!(
            seen.automations.iter().map(|a| a.id).collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(seen.usage, None);
        assert_eq!(
            seen.app.shown, None,
            "the shown workspace is w4: not its to know"
        );
        assert_eq!(seen.app.version, "0.1.5");
        let all = Caller {
            workspaces: None,
            ..caller(&[])
        };
        assert_eq!(state.visible(&all), state);
        // Seeing nothing is seeing nothing, not everything.
        let none = state.visible(&caller(&[]));
        assert!(
            none.blocks.is_empty() && none.workspaces.is_empty() && none.automations.is_empty()
        );
    }

    #[test]
    fn a_limited_caller_sees_a_wire_only_when_it_sees_both_ends() {
        let mut state = two_workspaces();
        state.wires.push(Arc::new(WireView {
            a: "b1".into(),
            b: "b2".into(),
            label: None,
        }));
        let seen = state.visible(&caller(&["w3"]));
        assert_eq!(seen.wires.len(), 1);
    }

    #[test]
    fn a_block_that_leaves_a_limited_view_is_a_delete() {
        let old = two_workspaces();
        let mut new = old.clone();
        Arc::make_mut(&mut new.blocks[0]).workspace = "w4".into();
        let w3 = caller(&["w3"]);
        assert_eq!(
            said(&diff(&old.visible(&w3), &new.visible(&w3))),
            [
                ("block", "b1", false),
                ("wire", "b1-k1", false),
                ("request", "r1", false)
            ]
        );
        // Moved back: puts.
        assert_eq!(
            said(&diff(&new.visible(&w3), &old.visible(&w3))),
            [
                ("block", "b1", true),
                ("wire", "b1-k1", true),
                ("request", "r1", true)
            ]
        );
        let everyone = Caller {
            workspaces: None,
            ..w3
        };
        let deltas = diff(&old.visible(&everyone), &new.visible(&everyone));
        assert_eq!(said(&deltas), [("block", "b1", true)]);
        assert_eq!(deltas[0].value.as_ref().unwrap()["workspace"], "w4");
    }

    fn share(id: &str) -> Arc<ShareView> {
        Arc::new(ShareView {
            id: id.into(),
            workspace: "w3".into(),
            name: "Another computer".into(),
            ..Default::default()
        })
    }

    #[test]
    fn a_limited_caller_sees_only_its_own_share() {
        let mut state = two_workspaces();
        state.shares = vec![share("s1"), share("s2")];
        let ids = |caller: &Caller| -> Vec<String> {
            state
                .visible(caller)
                .shares
                .into_iter()
                .map(|s| s.id.clone())
                .collect()
        };
        assert_eq!(
            ids(&Caller {
                token: 2,
                ..caller(&["w3"])
            }),
            ["s2"]
        );
        assert!(
            ids(&Caller {
                token: 7,
                ..caller(&["w3"])
            })
            .is_empty()
        );
        let all = Caller {
            workspaces: None,
            ..caller(&[])
        };
        assert_eq!(ids(&all), ["s1", "s2"]);
    }

    #[test]
    fn shares_are_on_the_stream_as_share_events() {
        let old = two_workspaces();
        let mut new = old.clone();
        new.shares = vec![share("s1")];
        assert_eq!(said(&diff(&old, &new)), [("share", "s1", true)]);
        let mut added = new.clone();
        Arc::make_mut(&mut added.shares[0]).mirrored = true;
        added.shares.push(share("s2"));
        assert_eq!(
            said(&diff(&new, &added)),
            [("share", "s1", true), ("share", "s2", true)]
        );
        assert_eq!(
            said(&diff(&added, &old)),
            [("share", "s1", false), ("share", "s2", false)]
        );
    }

    #[test]
    fn new_fields_serialize_only_when_they_say_something() {
        let mut b = BlockView::clone(&block("b1", "w3", "terminal"));
        assert!(serde_json::to_value(&b).unwrap().get("read_only").is_none());
        b.read_only = true;
        assert_eq!(serde_json::to_value(&b).unwrap()["read_only"], true);
        let w = WorkspaceView::default();
        assert_eq!(serde_json::to_value(&w).unwrap()["icon"], Value::Null);
        // A state from a home older than shares.
        let mut value = serde_json::to_value(two_workspaces()).unwrap();
        value.as_object_mut().unwrap().remove("shares");
        let old: State = serde_json::from_value(value).unwrap();
        assert!(old.shares.is_empty());
    }

    #[test]
    fn a_remote_workspace_says_where_it_mirrors_from() {
        let mut w = WorkspaceView::default();
        assert!(serde_json::to_value(&w).unwrap().get("remote").is_none());
        w.remote = Some(RemoteView {
            computer: Some("winx-desktop".into()),
            host: "templum.example.com".into(),
            way: "public".into(),
            status: "live".into(),
            latency_ms: Some(84),
        });
        let value = serde_json::to_value(&w).unwrap();
        assert_eq!(
            value["remote"],
            json!({"computer": "winx-desktop", "host": "templum.example.com", "way": "public",
                "status": "live", "latency_ms": 84})
        );
        // A home older than mirrors sends none; one that sends it reads back the same.
        let mut old = value.clone();
        old.as_object_mut().unwrap().remove("remote");
        assert_eq!(
            serde_json::from_value::<WorkspaceView>(old).unwrap().remote,
            None
        );
        assert_eq!(serde_json::from_value::<WorkspaceView>(value).unwrap(), w);
    }

    #[test]
    fn me_is_the_token_without_its_hash() {
        let mut store = crate::store::Store::default();
        store
            .mint("phone", Scope::Read, Some(9), Some(vec!["w3".into()]), 1)
            .unwrap();
        let me = MeView::of(&store.tokens[0], "winx-desktop");
        assert_eq!(
            serde_json::to_value(&me).unwrap(),
            json!({"name": "phone", "scope": "read", "workspaces": ["w3"], "expires": 9,
                "share": null, "computer": "winx-desktop",
                "version": env!("CARGO_PKG_VERSION"), "api": 1})
        );
    }

    #[test]
    fn sees_follows_the_list() {
        let limited = caller(&["w3"]);
        assert!(limited.sees(Some("w3")) && !limited.sees(Some("w4")) && !limited.sees(None));
        let all = Caller {
            workspaces: None,
            ..limited
        };
        assert!(all.sees(Some("w9")) && all.sees(None));
    }

    #[test]
    fn the_state_says_do_not_act() {
        let mut view = AutomationView::clone(&automation(1, None));
        view.act = json!({"notify": {"text": "x"}});
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["do"], json!({"notify": {"text": "x"}}));
        assert!(value.get("act").is_none());
        let back: AutomationView = serde_json::from_value(value).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn the_state_round_trips_as_the_spec_shows_it() {
        let state = two_workspaces();
        let text = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<State>(&text).unwrap(), state);
        let value = serde_json::to_value(&state).unwrap();
        assert_eq!(
            value["blocks"][0]["rect"],
            json!({"x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0})
        );
        assert_eq!(value["requests"][0]["scope"], "read");
    }
}
