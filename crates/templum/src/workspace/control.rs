//! The control server, served (docs/specs/templum-agent.md#the-control-server): the Templum
//! Agent's own tools, for its engine's process alone, on every workspace.

use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Window};
use serde_json::json;
use templum_agent::Target;
use templum_canvas::BlockId;
use templum_mcp::control::{self, ControlCall, Parsed};
use templum_mcp::{Call, Hello, Incoming, Replier, Reply, rpc, tools};

use super::bus::{Outcome, agent_state, clean_title};
use super::{Space, Workspace, show_notification};

/// What a control client hears when its secret isn't the engine's.
const NOT_IT: &str = "This isn't the Templum Agent's process.";

impl Workspace {
    /// Answers one message of a control connection. Its secret is compared at every message: one
    /// from before the engine's last start is no longer the Templum Agent's.
    pub(super) fn serve_control(
        &mut self,
        hello: Hello,
        incoming: Incoming,
        reply: Replier,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let genuine = hello.control.as_deref() == Some(self.templum_agent.secret.as_str());
        let Incoming::Request { id, method, params } = incoming else {
            return;
        };
        let answer = match method.as_str() {
            "initialize" => {
                let instructions = if genuine {
                    self.control_instructions(cx)
                } else {
                    NOT_IT.to_string()
                };
                let mut result = tools::initialize_result(&params, &instructions);
                result["serverInfo"]["name"] = json!(control::SERVER);
                rpc::result(&id, result)
            }
            "ping" => rpc::result(&id, json!({})),
            "tools/list" => rpc::result(&id, control::list()),
            "tools/call" if !genuine => tools::tool_result(&id, &Reply::err(NOT_IT)),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or_default();
                let arguments = &params["arguments"];
                if !control::known(name) {
                    rpc::error(&id, rpc::PARAMS, &format!("Unknown tool: {name}"))
                } else {
                    let (outcome, target) = match control::parse(name, arguments) {
                        Ok(parsed) => self.control_call(parsed, window, cx),
                        Err(why) => (Outcome::Now(Reply::err(why)), None),
                    };
                    if let Some(target) = target {
                        let conversation = &mut self.templum_agent.conversation;
                        conversation.touched(name, arguments, target);
                    }
                    match outcome {
                        Outcome::Now(done) => tools::tool_result(&id, &done),
                        Outcome::Later(job) => {
                            cx.spawn(async move |_, _| {
                                let done = job.await;
                                let _ = reply.send(tools::tool_result(&id, &done));
                            })
                            .detach();
                            return;
                        }
                        Outcome::Wait(..) => tools::tool_result(
                            &id,
                            &Reply::err("Templum can't wait on that for the Templum Agent yet."),
                        ),
                    }
                }
            }
            _ => rpc::error(&id, rpc::METHOD, &format!("Method not found: {method}")),
        };
        let _ = reply.send(answer);
    }

    /// A control call: its answer, and what it touched for the chat's row.
    fn control_call(
        &mut self,
        parsed: Parsed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Outcome, Option<Target>) {
        let reply = match parsed.call {
            ControlCall::Overview => Reply::ok(self.overview(cx)),
            ControlCall::ListAgents => Reply::ok(self.agent_list()),
            ControlCall::Block(Call::ReadTerminal {
                block,
                lines,
                last_command,
            }) => match self.find_block(&block) {
                Some((space, id)) => self.terminal_text((&space, id), &block, lines, last_command),
                None => no_block(&block),
            },
            ControlCall::Block(Call::ReadNote { block }) => match self.find_block(&block) {
                Some((space, id)) => self.note_text((&space, id), &block, cx),
                None => no_block(&block),
            },
            ControlCall::Block(Call::ListCards { board, card }) => {
                let board = match card {
                    Some(card) => self
                        .boards
                        .find_card(card)
                        .map(|(board, _)| board)
                        .ok_or_else(|| {
                            Reply::err(format!("There's no card {card}: list_cards lists them."))
                        }),
                    None => self.board_or_only(board.as_deref()),
                };
                match board {
                    Ok(board) => self.cards_of(board, card, None, cx),
                    Err(refused) => refused,
                }
            }
            ControlCall::Block(Call::CreateCard(new)) => {
                let board = match self.board_or_only(new.board.as_deref()) {
                    Ok(board) => board,
                    Err(refused) => return (Outcome::Now(refused), None),
                };
                return match self.card_new(board, new, cx) {
                    Ok(card) => {
                        self.templum_agent.conversation.own.cards.push(card);
                        let target = self.board_shown(board);
                        (
                            Outcome::Now(Reply::ok(format!("Created card {card}."))),
                            target,
                        )
                    }
                    Err(refused) => (Outcome::Now(refused), None),
                };
            }
            ControlCall::Block(Call::EditCard(edit)) => {
                let target = self.card_shown(edit.card);
                return (Outcome::Now(self.card_edit(edit, None, cx)), target);
            }
            ControlCall::Block(Call::MoveCard { card, column }) => {
                let target = self.card_shown(card);
                return (Outcome::Now(self.card_move(card, &column, cx)), target);
            }
            ControlCall::Block(Call::CommentCard { card, text }) => {
                let target = self.card_shown(card);
                let done = self.card_comment(card, &text, templum_agent::ASKER, cx);
                return (Outcome::Now(done), target);
            }
            ControlCall::Block(Call::AppendNote { block, text }) => {
                return self.on_block(&block, |ws, at| {
                    ws.note_append(at, &block, &text, window, cx)
                });
            }
            ControlCall::Block(Call::ReadChecklist { block }) => match self.find_block(&block) {
                Some((space, id)) => self.checklist_text((&space, id), &block),
                None => no_block(&block),
            },
            ControlCall::Block(Call::AddItems { block, text }) => {
                return self.on_block(&block, |ws, at| ws.items_add(at, &block, &text, cx));
            }
            ControlCall::Block(Call::CheckItem { block, item, done }) => {
                return self.on_block(&block, |ws, at| ws.item_check(at, &block, item, done, cx));
            }
            ControlCall::Block(Call::Notify { text }) => {
                let notification = Notification::new()
                    .title("Templum Agent")
                    .message(text)
                    .delivery(self.delivery(window));
                show_notification(window, cx, notification);
                Reply::ok("Shown.")
            }
            _ => Reply::err("Templum doesn't serve that tool to the Templum Agent yet."),
        };
        (Outcome::Now(reply), None)
    }

    /// `f` on the block `uid` names, in any workspace; its answer, and the block for the row.
    fn on_block(
        &mut self,
        uid: &str,
        f: impl FnOnce(&mut Self, (&str, BlockId)) -> Reply,
    ) -> (Outcome, Option<Target>) {
        let Some((space, id)) = self.find_block(uid) else {
            return (Outcome::Now(no_block(uid)), None);
        };
        let done = f(self, (&space, id));
        let target = (!done.is_error).then(|| Target {
            workspace: Some(space),
            block: Some(uid.to_string()),
        });
        (Outcome::Now(done), target)
    }

    /// The board `name` names, or the only one when it is left out.
    fn board_or_only(&self, name: Option<&str>) -> Result<u64, Reply> {
        match (name, self.boards.boards.as_slice()) {
            (Some(name), _) => self.board_named(name),
            (None, [one]) => Ok(one.id),
            (None, []) => Err(Reply::err(
                "There are no boards: add_block a kanban with a board first.",
            )),
            (None, _) => Err(Reply::err(format!(
                "There are several boards: {}. Name one with board.",
                self.boards
                    .boards
                    .iter()
                    .map(|b| format!("{:?} ({})", clean_title(&b.name), b.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    /// A kanban block that shows `board`, for an action's row: the shown workspace's first.
    fn board_shown(&self, board: u64) -> Option<Target> {
        self.order().into_iter().find_map(|space| {
            let id = space
                .canvas
                .blocks()
                .iter()
                .find(|b| space.kanban(b.id).and_then(|k| k.board) == Some(board))?
                .id;
            Some(Target {
                workspace: Some(space.id.clone()),
                block: space.uids.get(&id).cloned(),
            })
        })
    }

    fn card_shown(&self, card: u64) -> Option<Target> {
        self.board_shown(self.boards.find_card(card)?.0)
    }

    /// The block `uid` names, in any workspace: its workspace's id and its id.
    pub(super) fn find_block(&self, uid: &str) -> Option<(String, BlockId)> {
        self.spaces()
            .find_map(|s| Some((s.id.clone(), s.block_by_uid(uid)?)))
    }

    /// `initialize`'s instructions: a line per workspace, in the sidebar's order.
    fn control_instructions(&self, _: &App) -> String {
        let mut lines: Vec<String> = self
            .order()
            .into_iter()
            .map(|space| {
                let blocks = space.canvas.blocks().len();
                let working = space
                    .terminals()
                    .filter(|(_, pane)| {
                        pane.agent().is_some()
                            && matches!(pane.state, Some(super::AgentState::Working))
                    })
                    .count();
                format!(
                    "{} ({}){}: {blocks} block{}, {working} agent{} working",
                    clean_title(&space.name),
                    space.id,
                    self.space_state(space),
                    if blocks == 1 { "" } else { "s" },
                    if working == 1 { "" } else { "s" },
                )
            })
            .collect();
        lines.push("Call overview for the blocks.".into());
        lines.join("\n")
    }

    /// `, shown`, `, stopped` or nothing.
    fn space_state(&self, space: &Space) -> &'static str {
        if space.id == self.space.id {
            ", shown"
        } else if space.stopped {
            ", stopped"
        } else {
            ""
        }
    }

    /// `overview`'s answer: every workspace, its blocks and its wires.
    fn overview(&self, cx: &App) -> String {
        let own = &self.templum_agent.conversation.own;
        let mut text = Vec::new();
        for space in self.order() {
            let folder = space
                .folder
                .as_ref()
                .map(|f| format!(", folder {}", f.display()))
                .unwrap_or_default();
            text.push(format!(
                "Workspace {:?} ({}){}{folder}:",
                clean_title(&space.name),
                space.id,
                self.space_state(space),
            ));
            let uid = |id: &BlockId| space.uids.get(id).map_or("", String::as_str);
            for b in space.canvas.blocks() {
                let (kind, title, folder) = self.about(&space.blocks[&b.id], cx);
                let mut line = format!("  {}  {kind}  {title:?}", uid(&b.id));
                if let Some(folder) = folder {
                    line.push_str(&format!("  {}", folder.display()));
                }
                let r = b.rect;
                line.push_str(&format!("  at {:.0},{:.0} {:.0}×{:.0}", r.x, r.y, r.w, r.h));
                if let Some(pane) = space.terminal(b.id) {
                    if let Some(agent) = pane.agent() {
                        line.push_str(&format!("  agent {agent}{}", agent_state(space, b.id)));
                    }
                    if let Some(by) = &pane.opened_by {
                        line.push_str(&format!("  opened by {by}"));
                    }
                }
                if let Some(board) = space
                    .kanban(b.id)
                    .and_then(|k| k.board)
                    .and_then(|b| self.boards.board(b))
                {
                    line.push_str(&format!(
                        "  board {:?} ({})",
                        clean_title(&board.name),
                        board.id
                    ));
                }
                if own.blocks.iter().any(|o| o == uid(&b.id)) {
                    line.push_str("  (yours)");
                }
                text.push(line);
            }
            if space.canvas.blocks().is_empty() {
                text.push("  no blocks".into());
            }
            if !space.wires.is_empty() {
                let wires: Vec<String> = space
                    .wires
                    .iter()
                    .map(|w| format!("{}–{}", uid(&w.a), uid(&w.b)))
                    .collect();
                text.push(format!("  wires: {}", wires.join(", ")));
            }
        }
        tools::tail(&text.join("\n"), tools::MAX_TEXT).to_string()
    }

    /// `list_agents`' answer: a line per agent Templum can open.
    fn agent_list(&self) -> String {
        let lines: Vec<String> = self
            .agents
            .iter()
            .map(|a| {
                let installed = if templum_terminal::installed(a.program()) {
                    "installed"
                } else {
                    "not installed"
                };
                let model = if a.launch_args_with(Some("m"), None, None).is_ok() {
                    "takes a model"
                } else {
                    "no model"
                };
                let efforts = a.builtin.map(|b| b.efforts()).unwrap_or_default();
                let efforts = if efforts.is_empty() {
                    "no effort".to_string()
                } else {
                    format!("efforts {}", efforts.join(", "))
                };
                let account = if a.account.is_some() {
                    "  an account"
                } else {
                    ""
                };
                format!(
                    "{}  {:?}  {installed}  {model}  {efforts}{account}",
                    a.id,
                    clean_title(&a.title)
                )
            })
            .collect();
        lines.join("\n")
    }
}

/// The answer for a block no workspace has.
fn no_block(uid: &str) -> Reply {
    Reply::err(format!("There's no block {uid}: overview lists them."))
}
