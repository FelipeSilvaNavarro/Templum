//! The state the API shows (docs/specs/api.md, Resources), built from the workspaces on the UI
//! thread: walked as `overview` walks them, so a feature that is in the state is on the event
//! stream without more code.

use std::time::{Instant, SystemTime};

use gpui_kit::App;
use templum_api::model::{
    AgentView, App as AppView, Assignee, AttachmentView, AutomationView, BlockView, BoardView,
    CardRef, CardView, ColumnView, CommentView, ItemView, LimitsView, OptionView, PlanView,
    QuestionView, Rect, RequestView, SessionView, State, UsageView, Why, WindowView, WireView,
    WorkspaceView,
};
use templum_api::store::Scope;

use super::agents_view::Doing;
use super::waits::{Approved, Waiting};
use super::{Space, Workspace};

/// Unix seconds for an instant of the executor's clock, given the clock's `now` and the time.
fn unix_of(at: Instant, now: Instant, wall: i64) -> i64 {
    wall - now.saturating_duration_since(at).as_secs() as i64
}

fn some(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

impl Workspace {
    /// Everything the API shows, as of now.
    pub(crate) fn api_state(&self, cx: &App) -> State {
        let (now, wall) = (
            cx.background_executor().now(),
            super::automations::unix(SystemTime::now()),
        );
        let spaces = self.order();
        State {
            app: AppView {
                version: env!("CARGO_PKG_VERSION").to_string(),
                api: 1,
                shown: Some(self.space.id.clone()),
            },
            workspaces: spaces
                .iter()
                .map(|space| WorkspaceView {
                    id: space.id.clone(),
                    name: space.name.clone(),
                    folder: space.folder.as_ref().map(|f| f.display().to_string()),
                    color: space.look.color.clone(),
                    shown: space.id == self.space.id,
                    stopped: space.stopped,
                })
                .collect(),
            blocks: spaces
                .iter()
                .flat_map(|space| self.block_views(space, now, wall, cx))
                .collect(),
            wires: spaces.iter().flat_map(|space| wire_views(space)).collect(),
            requests: self.request_views(now, wall),
            boards: self
                .boards
                .boards
                .iter()
                .map(|board| BoardView {
                    id: board.id,
                    name: board.name.clone(),
                    columns: board
                        .columns
                        .iter()
                        .map(|c| ColumnView {
                            id: c.id,
                            title: c.title.clone(),
                            cards: c.cards.iter().map(|card| card.id).collect(),
                        })
                        .collect(),
                })
                .collect(),
            cards: self
                .boards
                .boards
                .iter()
                .flat_map(|board| board.columns.iter().map(move |c| (board.id, c)))
                .flat_map(|(board, column)| {
                    column
                        .cards
                        .iter()
                        .map(move |card| card_view(board, column.id, card))
                })
                .collect(),
            automations: self.automation_views(cx),
            usage: (!self.limits.is_empty()).then(|| UsageView {
                limits: self
                    .limits
                    .iter()
                    .map(|(agent, limits)| LimitsView {
                        agent: agent.clone(),
                        at: limits.at,
                        windows: limits
                            .windows
                            .iter()
                            .map(|w| WindowView {
                                minutes: w.minutes,
                                used_percent: w.used_percent,
                                resets_at: w.resets_at,
                                scope: w.scope.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            }),
        }
    }

    /// The blocks of a workspace, back to front.
    fn block_views(&self, space: &Space, now: Instant, wall: i64, cx: &App) -> Vec<BlockView> {
        space
            .canvas
            .blocks()
            .iter()
            .enumerate()
            .filter_map(|(z, block)| {
                let body = space.blocks.get(&block.id)?;
                let (kind, title, folder) = self.about(body, cx);
                let rect = space.home(block);
                let pane = space.terminal(block.id);
                Some(BlockView {
                    id: space.uids.get(&block.id)?.clone(),
                    workspace: space.id.clone(),
                    kind: kind.to_string(),
                    title,
                    folder: folder.map(|f| f.display().to_string()),
                    rect: Rect {
                        x: rect.x,
                        y: rect.y,
                        w: rect.w,
                        h: rect.h,
                    },
                    z,
                    opened_by: pane.and_then(|p| p.opened_by.clone()),
                    board: space.kanban(block.id).and_then(|k| k.board),
                    agent: pane
                        .filter(|p| p.agent().is_some() && p.follows.is_none())
                        .and_then(|p| self.agent_view(p, now, wall)),
                })
            })
            .collect()
    }

    /// An agent as the Agents view tells it: its state is the dot's, `done` when its turn ended
    /// unseen.
    fn agent_view(&self, pane: &super::Pane, now: Instant, wall: i64) -> Option<AgentView> {
        let (_, since) = pane.watch.shown()?;
        let doing = match self.doing(pane) {
            Doing::Idle if pane.watch.is_done() => Doing::Done,
            doing => doing,
        };
        let (state, asks, subagents) = match &doing {
            Doing::NeedsYou(asks) => ("needs_you", some(asks), 0),
            Doing::Starting => ("starting", None, 0),
            Doing::AwaitsSubagents(n) => ("waiting_subagents", None, *n),
            Doing::Working(n) => ("working", None, *n),
            Doing::Done => ("done", None, 0),
            Doing::Idle => ("idle", None, 0),
        };
        let session = pane.usage.as_ref();
        Some(AgentView {
            agent: pane.agent()?.to_string(),
            state: state.to_string(),
            asks,
            subagents,
            since: Some(unix_of(since, now, wall)),
            model: pane
                .model
                .clone()
                .or_else(|| session.and_then(|s| s.model.clone())),
            effort: pane.effort.clone(),
            usage: session.map(|s| SessionView {
                context_percent: s.context_percent,
                tokens: s.tokens,
                cost_usd: s.cost_usd,
            }),
        })
    }

    /// What waits for the person: the inbox's requests and the Templum Agent's approvals.
    fn request_views(&self, now: Instant, wall: i64) -> Vec<RequestView> {
        let place = |terminal: &str| {
            self.caller(terminal).map(|(space, id)| {
                let uid = self
                    .space_named(&space)
                    .and_then(|s| s.uids.get(&id).cloned());
                (uid, space)
            })
        };
        let mut views: Vec<RequestView> = self
            .waits
            .iter()
            .filter_map(|wait| {
                let (kind, buttons, scope) = match &wait.what {
                    Waiting::Question { ask, .. } if ask.plan.is_some() => {
                        ("plan", None, Scope::Control)
                    }
                    Waiting::Question { .. } => ("question", None, Scope::Control),
                    Waiting::Allow { then } => {
                        let (yes, no) = then.buttons();
                        let buttons = Some([yes.to_string(), no.to_string()]);
                        match then {
                            Approved::Connect { .. } => ("connect", buttons, Scope::Control),
                            Approved::RemoveWorktree { .. } => {
                                ("remove_worktree", buttons, Scope::Full)
                            }
                            Approved::MergeWorktree { .. } => {
                                ("merge_worktree", buttons, Scope::Full)
                            }
                        }
                    }
                    Waiting::Turn { .. } | Waiting::Event { .. } => return None,
                };
                let (block, workspace) = match place(&wait.from) {
                    Some((uid, space)) => (uid, Some(space)),
                    None => (None, None),
                };
                let why = self.why(wait);
                let (questions, plan, stopped) = match &wait.what {
                    Waiting::Question { ask, stopped, .. } => (
                        ask.questions
                            .iter()
                            .map(|q| QuestionView {
                                question: q.text.clone(),
                                header: q.header.clone(),
                                multi: q.multi,
                                options: q
                                    .options
                                    .iter()
                                    .map(|o| OptionView {
                                        label: o.label.clone(),
                                        description: o.description.clone(),
                                        preview: o.preview.clone(),
                                    })
                                    .collect(),
                            })
                            .collect(),
                        ask.plan.as_ref().map(|p| PlanView {
                            title: p.title.clone(),
                            text: p.text.clone(),
                        }),
                        *stopped,
                    ),
                    _ => (Vec::new(), None, false),
                };
                Some(RequestView {
                    id: format!("r{}", wait.key),
                    kind: kind.to_string(),
                    block,
                    workspace,
                    since: unix_of(wait.since, now, wall),
                    summary: wait.summary(),
                    why: Why {
                        card: why.card.map(|(id, title)| CardRef { id, title }),
                        task: why.task,
                        agent: why.agent,
                    },
                    questions,
                    plan,
                    buttons,
                    stopped,
                    scope,
                    ..Default::default()
                })
            })
            .collect();
        views.extend(
            self.templum_agent
                .approvals
                .iter()
                .map(|approval| RequestView {
                    id: format!("a{}", approval.id),
                    kind: "approval".into(),
                    since: approval.shown.get().map_or(0, |at| unix_of(at, now, wall)),
                    summary: approval.said.clone(),
                    scope: Scope::Full,
                    tool: Some(approval.tool.clone()),
                    said: Some(approval.said.clone()),
                    details: Some(approval.details.clone()),
                    facts: Some(approval.facts.clone()),
                    reason: approval.parsed.reason.clone(),
                    ..Default::default()
                }),
        );
        views
    }

    fn automation_views(&self, cx: &App) -> Vec<AutomationView> {
        self.automations
            .automations
            .iter()
            .map(|a| AutomationView {
                id: a.id,
                name: a.name.clone(),
                on: a.on,
                workspace: a.workspace.clone(),
                when: serde_json::to_value(&a.when).unwrap_or_default(),
                act: serde_json::to_value(&a.act).unwrap_or_default(),
                max_runs: a.max_runs,
                cooldown: None,
                runs: a.runs,
                last: a
                    .last
                    .as_ref()
                    .and_then(|last| serde_json::to_value(last).ok()),
                next: self
                    .automation_runs
                    .get(&a.id)
                    .and_then(|run| run.next)
                    .map(super::automations::unix),
                sentence: self.automation_sentence(a, cx),
            })
            .collect()
    }
}

fn wire_views(space: &Space) -> Vec<WireView> {
    space
        .wires
        .iter()
        .filter_map(|wire| {
            let (a, b) = (
                space.uids.get(&wire.a)?.clone(),
                space.uids.get(&wire.b)?.clone(),
            );
            let key = if a <= b {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            };
            Some(WireView {
                a,
                b,
                label: space.labels.get(&key).cloned(),
            })
        })
        .collect()
}

fn card_view(board: u64, column: u64, card: &crate::board::Card) -> CardView {
    CardView {
        id: card.id,
        board,
        column,
        title: card.title.clone(),
        description: card.description.clone(),
        labels: card.labels.iter().map(|l| l.name().to_string()).collect(),
        priority: card.priority.map(|p| p.name().to_string()),
        assignee: card.assignee.as_ref().map(|a| match a {
            crate::board::Assignee::Block(uid) => Assignee {
                block: Some(uid.clone()),
                agent: None,
            },
            crate::board::Assignee::Agent(agent) => Assignee {
                block: None,
                agent: Some(agent.clone()),
            },
        }),
        checklist: card
            .checklist
            .iter()
            .map(|i| ItemView {
                id: i.id,
                text: i.text.clone(),
                done: i.done,
            })
            .collect(),
        comments: card
            .comments
            .iter()
            .map(|c| CommentView {
                number: c.id,
                by: c.by.clone(),
                text: c.text.clone(),
                at: c.at as i64,
                edited: c.edited,
            })
            .collect(),
        attachments: card
            .attachments
            .iter()
            .map(|a| AttachmentView {
                name: a.name.clone(),
                size: a.size,
            })
            .collect(),
    }
}
