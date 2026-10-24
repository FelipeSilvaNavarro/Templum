//! What the API shows as data (docs/specs/api.md, Resources): the state, as one JSON document, the
//! rules for what a workspace-limited token sees of it, and the comparison that finds what changed
//! between two of them. Plain data: `crates/templum` builds it from the workspaces.

use crate::store::{Scope, Token};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Everything the API shows. Times are Unix seconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub app: App,
    pub workspaces: Vec<WorkspaceView>,
    pub blocks: Vec<BlockView>,
    pub wires: Vec<WireView>,
    pub requests: Vec<RequestView>,
    pub boards: Vec<BoardView>,
    pub cards: Vec<CardView>,
    pub automations: Vec<AutomationView>,
    pub usage: Option<UsageView>,
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
        let blocks: Vec<BlockView> = self
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
            usage: None,
            blocks,
        }
    }
}

/// One change to the state: `value` is the whole object for a put, None for a delete.
#[derive(Clone, Debug, PartialEq)]
pub struct Delta {
    pub kind: &'static str,
    pub id: String,
    pub value: Option<Value>,
}

/// What differs between two states, per collection (workspace, block, wire, request, board, card,
/// automation, usage): the deletes, then the puts.
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
    compare("wire", &old.wires, &new.wires, WireView::id, &mut out);
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
        "usage",
        old.usage.as_slice(),
        new.usage.as_slice(),
        |_| "usage".into(),
        &mut out,
    );
    out
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

    fn block(id: &str, workspace: &str, kind: &str) -> BlockView {
        BlockView {
            id: id.into(),
            workspace: workspace.into(),
            kind: kind.into(),
            title: id.into(),
            ..Default::default()
        }
    }

    fn request(id: &str, kind: &str, block: &str) -> RequestView {
        RequestView {
            id: id.into(),
            kind: kind.into(),
            block: (!block.is_empty()).then(|| block.into()),
            ..Default::default()
        }
    }

    fn automation(id: u64, workspace: Option<&str>) -> AutomationView {
        AutomationView {
            id,
            workspace: workspace.map(Into::into),
            ..Default::default()
        }
    }

    /// w3: `b1` (a terminal with a working agent) and `k1` (a kanban showing board 3, card 40);
    /// w4: `b2`; a wire b1–k1; requests r1 (of b1), r2 (of b2), approval a1; boards 3 and 5 (shown
    /// by none); automations for w3, w4 and all workspaces; a usage.
    fn two_workspaces() -> State {
        let mut b1 = block("b1", "w3", "terminal");
        b1.agent = Some(AgentView {
            agent: "claude".into(),
            state: "working".into(),
            ..Default::default()
        });
        let mut k1 = block("k1", "w3", "kanban");
        k1.board = Some(3);
        State {
            app: App {
                version: "0.1.5".into(),
                api: 1,
                shown: Some("w4".into()),
            },
            workspaces: ["w3", "w4"]
                .map(|id| WorkspaceView {
                    id: id.into(),
                    name: id.into(),
                    ..Default::default()
                })
                .into(),
            blocks: vec![b1, k1, block("b2", "w4", "terminal")],
            wires: vec![WireView {
                a: "b1".into(),
                b: "k1".into(),
                label: None,
            }],
            requests: vec![
                request("r1", "question", "b1"),
                request("r2", "question", "b2"),
                request("a1", "approval", ""),
            ],
            boards: [3, 5]
                .map(|id| BoardView {
                    id,
                    name: format!("B{id}"),
                    columns: vec![],
                })
                .into(),
            cards: vec![CardView {
                id: 40,
                board: 3,
                column: 7,
                ..Default::default()
            }],
            automations: vec![
                automation(1, Some("w3")),
                automation(2, Some("w4")),
                automation(3, None),
            ],
            usage: Some(UsageView::default()),
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

    #[test]
    fn diff_puts_what_changed_and_deletes_what_went() {
        let old = two_workspaces();
        let mut new = old.clone();
        new.blocks[0].rect.x = 50.0;
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
        new.wires.push(WireView {
            a: "k1".into(),
            b: "b2".into(),
            label: Some("x".into()),
        });
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
            seen.wires.iter().map(WireView::id).collect::<Vec<_>>(),
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
        state.wires.push(WireView {
            a: "b1".into(),
            b: "b2".into(),
            label: None,
        });
        let seen = state.visible(&caller(&["w3"]));
        assert_eq!(seen.wires.len(), 1);
    }

    #[test]
    fn a_block_that_leaves_a_limited_view_is_a_delete() {
        let old = two_workspaces();
        let mut new = old.clone();
        new.blocks[0].workspace = "w4".into();
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
        let mut view = automation(1, None);
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
