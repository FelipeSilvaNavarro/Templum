//! The control server, served (docs/specs/templum-agent.md#the-control-server): the Templum
//! Agent's own tools, for its engine's process alone, on every workspace.

use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Window};
use serde_json::{Value, json};
use templum_agent::Target;
use templum_canvas::BlockId;
use templum_mcp::control::{self, AddBlock, ControlCall, Kind, Opening, Parsed};
use templum_mcp::{Call, Hello, Incoming, Replier, Reply, rpc, tools};

use super::bus::{Outcome, agent_state, clean_title};
use super::ops::{NewBlock, Place};
use super::spawn::Opener;
use super::{Space, Workspace, show_notification};

/// What a control client hears when its secret isn't the engine's.
const NOT_IT: &str = "This isn't the Templum Agent's process.";

impl Workspace {
    /// Answers one message of a control connection. Its secret is compared at every message: one
    /// from before the engine's last start is no longer the Templum Agent's.
    pub(super) fn serve_control(
        &mut self,
        hello: Hello,
        conn: u64,
        incoming: Incoming,
        reply: Replier,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let genuine = hello.control.as_deref() == Some(self.templum_agent.secret.as_str());
        let (id, method, params) = match incoming {
            Incoming::Request { id, method, params } => (id, method, params),
            // The engine gave up on a call, or its connection ended: what waits goes.
            Incoming::Notification { method, params } if method == "notifications/cancelled" => {
                return self.drop_approval(conn, &params["requestId"], cx);
            }
            Incoming::Notification { method, .. } if method == templum_mcp::bus::CLOSED => {
                return self.drop_approvals_of(conn, cx);
            }
            Incoming::Notification { .. } | Incoming::Response => return,
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
                    match control::parse(name, arguments) {
                        Ok(parsed) => {
                            let args = arguments.clone();
                            if let Some((id, reply, parsed, args)) =
                                self.gate(conn, id, reply, name, parsed, args, window, cx)
                            {
                                self.run_control(id, reply, name, parsed, &args, window, cx);
                            }
                            return;
                        }
                        Err(why) => tools::tool_result(&id, &Reply::err(why)),
                    }
                }
            }
            _ => rpc::error(&id, rpc::METHOD, &format!("Method not found: {method}")),
        };
        let _ = reply.send(answer);
    }

    /// Runs a control call past its gate and answers it, now or when it is done; what it
    /// touched goes to its row in the chat.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_control(
        &mut self,
        id: Value,
        reply: Replier,
        name: &str,
        parsed: Parsed,
        args: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (outcome, target) = self.control_call(parsed, args, window, cx);
        if let Some(target) = target {
            self.touch(name, args, target, cx);
        }
        match outcome {
            Outcome::Now(done) => {
                let _ = reply.send(tools::tool_result(&id, &done));
            }
            Outcome::Later(job) => {
                cx.spawn(async move |_, _| {
                    let done = job.await;
                    let _ = reply.send(tools::tool_result(&id, &done));
                })
                .detach();
            }
            Outcome::Wait(..) => {
                let refused = Reply::err("Templum can't wait on that for the Templum Agent yet.");
                let _ = reply.send(tools::tool_result(&id, &refused));
            }
        }
    }

    /// What a call touched: its row in the chat shows it, and its block glows.
    pub(super) fn touch(
        &mut self,
        tool: &str,
        args: &Value,
        target: Target,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        let agent = &mut self.templum_agent;
        agent
            .glow
            .retain(|_, at| crate::chat::glow(*at, now).is_some());
        if let Some(uid) = &target.block {
            agent.glow.insert(uid.clone(), now);
        }
        agent.conversation.touched(tool, args, target);
        cx.notify();
    }

    /// A control call: its answer, and what it touched for the chat's row.
    fn control_call(
        &mut self,
        parsed: Parsed,
        args: &Value,
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
            ControlCall::CreateWorkspace {
                name,
                folder,
                color,
            } => return self.create_workspace(&name, folder, color, window, cx),
            ControlCall::Show { workspace, block } => {
                return self.show_for_agent(workspace, block, window, cx);
            }
            ControlCall::AddBlock(add) => return self.add_block_for_agent(add, window, cx),
            ControlCall::OpenAgent(opening) => {
                return (
                    self.open_agent_for_agent(opening, args.clone(), window, cx),
                    None,
                );
            }
            ControlCall::Connect { a, b } => return self.wire_for_agent(&a, &b, true, cx),
            ControlCall::Disconnect { a, b } => return self.wire_for_agent(&a, &b, false, cx),
            ControlCall::CloseBlock { block } => {
                let asker = templum_agent::ASKER;
                return self.on_block(&block, |ws, at| {
                    ws.block_close(at, &block, asker, window, cx)
                });
            }
            ControlCall::Block(Call::SendToTerminal { block, text, enter }) => {
                return match self.find_block(&block) {
                    Some((space, id)) => {
                        let typed = self.type_text((&space, id), &block, &text, enter, cx);
                        let target = Target {
                            workspace: Some(space),
                            block: Some(block),
                        };
                        (typed, Some(target))
                    }
                    None => (Outcome::Now(no_block(&block)), None),
                };
            }
            ControlCall::Block(Call::SendKeys { block, keys }) => {
                return self.keys_for_agent(&block, keys, cx);
            }
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

    /// `create_workspace`: a new workspace, not shown, with its name, folder and color.
    fn create_workspace(
        &mut self,
        name: &str,
        folder: Option<String>,
        color: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Outcome, Option<Target>) {
        let refuse = |why: String| (Outcome::Now(Reply::err(why)), None);
        let name = name.trim();
        if name.is_empty() {
            return refuse("A workspace needs a name.".into());
        }
        let folder = folder.map(|f| crate::launcher::expand(&f));
        if let Some(folder) = folder.as_ref().filter(|f| !f.is_dir()) {
            return refuse(format!("{} isn't a folder.", folder.display()));
        }
        if let Some(color) = color
            .as_deref()
            .filter(|c| !crate::space_dialog::COLORS.contains(c))
        {
            return refuse(format!(
                "{color} isn't a workspace color: red, yellow, green, cyan, blue, magenta or accent."
            ));
        }
        let id = self.make_space();
        if let Some(space) = self.space_by_id_mut(&id) {
            space.name = name.to_string();
            space.folder = folder;
            space.look.color = color;
        }
        self.write_index(window, cx);
        self.templum_agent
            .conversation
            .own
            .workspaces
            .push(id.clone());
        cx.notify();
        let done = Reply::ok(format!("Created workspace {} ({id}).", clean_title(name)));
        let target = Target {
            workspace: Some(id),
            block: None,
        };
        (Outcome::Now(done), Some(target))
    }

    /// `show`: the workspace shown, and the camera flown to the block.
    fn show_for_agent(
        &mut self,
        workspace: Option<String>,
        block: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Outcome, Option<Target>) {
        let shown = match (workspace, block) {
            (_, Some(uid)) => match self.find_block(&uid) {
                Some((space, id)) => {
                    self.reveal(&space, id, window, cx);
                    Ok((space, Some(uid)))
                }
                None => Err(no_block(&uid)),
            },
            (Some(name), None) => self.space_of(&name, true).map(|space| {
                if let Some(to) = self.order().iter().position(|s| s.id == space) {
                    self.switch_to(to, window, cx);
                }
                (space, None)
            }),
            (None, None) => Err(Reply::err("show needs a workspace or a block.")),
        };
        match shown {
            Ok((space, block)) => {
                let name = self
                    .space_named(&space)
                    .map(|s| clean_title(&s.name))
                    .unwrap_or_default();
                let done = match &block {
                    Some(uid) => format!("Showing {uid} in {name} ({space})."),
                    None => format!("Showing {name} ({space})."),
                };
                let target = Target {
                    workspace: Some(space),
                    block,
                };
                (Outcome::Now(Reply::ok(done)), Some(target))
            }
            Err(refused) => (Outcome::Now(refused), None),
        }
    }

    /// `add_block`: a block of a kind in a workspace, where it covers nothing.
    fn add_block_for_agent(
        &mut self,
        add: AddBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Outcome, Option<Target>) {
        let refuse = |why: Reply| (Outcome::Now(why), None);
        let space = match self.space_of(&add.workspace, false) {
            Ok(space) => space,
            Err(refused) => return refuse(refused),
        };
        let Some(home) = self.space_named(&space) else {
            return refuse(Reply::err("That workspace is gone."));
        };
        let (name, home_folder) = (clean_title(&home.name), home.folder.clone());
        let place = match &add.near {
            Some(near) => match home.block_by_uid(near) {
                Some(id) => Place::Near(id),
                None => {
                    return refuse(Reply::err(format!(
                        "There's no block {near} in {name} ({space}): overview lists them."
                    )));
                }
            },
            None => Place::Free,
        };
        let folder = add
            .folder
            .as_deref()
            .map(crate::launcher::expand)
            .map(|f| match &home_folder {
                Some(home) if f.is_relative() => home.join(f),
                _ => f,
            })
            .or(home_folder);
        if let Some(folder) = folder.as_ref().filter(|f| !f.is_dir()) {
            return refuse(Reply::err(format!("{} isn't a folder.", folder.display())));
        }
        let mut made_board = None;
        let (block, what) = match add.kind {
            Kind::Terminal => {
                // A workspace with no folder opens it at home, as a new terminal does.
                let Some(folder) = folder.or_else(std::env::home_dir) else {
                    return refuse(Reply::err(
                        "Name a folder: Templum can't find your home folder.",
                    ));
                };
                let block = NewBlock::Terminal {
                    folder,
                    command: None,
                };
                (block, "a terminal".to_string())
            }
            Kind::Note => (
                NewBlock::Note {
                    text: add.text.unwrap_or_default(),
                },
                "a note".to_string(),
            ),
            Kind::Checklist => {
                let text = add.text.unwrap_or_default();
                let (name, items) = text.split_once('\n').unwrap_or((&text, ""));
                let block = NewBlock::Checklist {
                    name: name.to_string(),
                    items: crate::checklists::parse_items(items),
                };
                (block, "a checklist".to_string())
            }
            Kind::Kanban => {
                let board = match add.board.as_deref().map(|b| self.board_named(b)) {
                    Some(Ok(board)) => board,
                    found => {
                        // None given, or none by that name: a new one, with those columns.
                        let name = match (found, &add.board) {
                            (Some(Err(_)), Some(name)) if name.parse::<u64>().is_err() => {
                                Some(name.trim().to_string())
                            }
                            (Some(Err(refused)), _) => return refuse(refused),
                            _ => None,
                        };
                        let columns: Vec<String> = if add.columns.is_empty() {
                            ["To do", "Doing", "Review", "Done"]
                                .map(String::from)
                                .into()
                        } else {
                            add.columns.clone()
                        };
                        let board =
                            self.edit_boards(|b| b.add_board_with(name.as_deref(), &columns));
                        made_board = Some(board);
                        board
                    }
                };
                let title = self
                    .boards
                    .board(board)
                    .map(|b| clean_title(&b.name))
                    .unwrap_or_default();
                (
                    NewBlock::Kanban { board },
                    format!("a kanban showing board {title:?} ({board})"),
                )
            }
            Kind::Editor => match folder {
                Some(folder) => (NewBlock::Editor { folder }, "an editor".to_string()),
                None => return refuse(Reply::err("An editor needs a folder.")),
            },
        };
        let own = &mut self.templum_agent.conversation.own;
        own.boards.extend(made_board);
        let opened = self.add_block_in(
            &space,
            block,
            place,
            Some(templum_agent::ASKER.into()),
            window,
            cx,
        );
        let id = match opened {
            Ok(id) => id,
            Err(why) => return refuse(Reply::err(why)),
        };
        let Some(home) = self.space_named(&space) else {
            return refuse(Reply::err("That workspace is gone."));
        };
        let uid = home.uids.get(&id).cloned().unwrap_or_default();
        let (note, list) = (
            home.blocks
                .get(&id)
                .and_then(|b| b.note()?.tab().map(|t| t.id)),
            home.blocks.get(&id).and_then(|b| b.checklist()?.list),
        );
        let own = &mut self.templum_agent.conversation.own;
        own.blocks.push(uid.clone());
        own.notes.extend(note);
        own.checklists.extend(list);
        let done = Reply::ok(format!("Added {what} to {name} ({space}) as {uid}."));
        let target = Target {
            workspace: Some(space),
            block: Some(uid),
        };
        (Outcome::Now(done), Some(target))
    }

    /// `open_agent`: `spawn_agent`'s checks, then the agent opened in the workspace, beside
    /// `near` or in the middle of its view, wired to `connect`; in a new worktree, once git made
    /// it. Its row's target is set when it opened (`args`, its call).
    fn open_agent_for_agent(
        &mut self,
        opening: Opening,
        args: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let refuse = |why: String| Outcome::Now(Reply::err(why));
        let spawn = opening.spawn;
        // Parsed with one: no fork, no role.
        let agent = spawn.agent.as_deref().unwrap_or_default();
        let Some(entry) = self.agents.iter().find(|a| a.id == agent).cloned() else {
            let ids: Vec<&str> = self.agents.iter().map(|a| a.id.as_str()).collect();
            return refuse(format!(
                "There's no agent {agent}. These are: {}.",
                ids.join(", ")
            ));
        };
        // Typed into a shell: anything it can't type safely opens nothing, a worktree included.
        let (model, effort) = (spawn.model.as_deref(), spawn.effort.as_deref());
        if let Err(why) = entry.launch_args_with(model, effort, None) {
            return refuse(why);
        }
        if !templum_terminal::installed(entry.program()) {
            return refuse(format!(
                "{} isn't installed on this computer ({} wasn't found).",
                entry.title,
                entry.program()
            ));
        }
        let space = match self.space_of(&opening.workspace, false) {
            Ok(space) => space,
            Err(refused) => return Outcome::Now(refused),
        };
        let Some(home) = self.space_named(&space) else {
            return refuse("That workspace is gone.".into());
        };
        let name = clean_title(&home.name);
        let block = |uid: &str| {
            home.block_by_uid(uid).ok_or_else(|| {
                format!("There's no block {uid} in {name} ({space}): overview lists them.")
            })
        };
        let wire_to: Vec<BlockId> = match opening.connect.iter().map(|uid| block(uid)).collect() {
            Ok(ids) => ids,
            Err(why) => return refuse(why),
        };
        let place = match opening.near.as_deref().map(block) {
            Some(Ok(id)) => Place::Near(id),
            Some(Err(why)) => return refuse(why),
            None => Place::Free,
        };
        let folder = match (spawn.folder.as_deref(), home.folder.clone()) {
            (Some(named), here) => {
                let named = crate::launcher::expand(named);
                match here {
                    Some(here) if named.is_relative() => here.join(named),
                    _ => named,
                }
            }
            (None, Some(here)) => here,
            (None, None) => {
                return refuse(format!(
                    "Workspace {name} has no folder: name one with folder."
                ));
            }
        };
        if !folder.is_dir() {
            return refuse(format!("{} isn't a folder.", folder.display()));
        }
        let worktree = spawn.worktree.clone();
        let open = move |ws: &mut Self,
                         folder: &std::path::Path,
                         window: &mut Window,
                         cx: &mut Context<Self>| {
            let opener = Opener::TemplumAgent;
            let (done, uid) = ws.open_agent_in(
                &space, &entry, None, folder, &spawn, place, &wire_to, opener, window, cx,
            );
            if let Some(uid) = uid {
                ws.templum_agent.conversation.own.blocks.push(uid.clone());
                let target = Target {
                    workspace: Some(space),
                    block: Some(uid),
                };
                ws.touch("open_agent", &args, target, cx);
            }
            done
        };
        self.in_worktree(folder, worktree, open, window, cx)
    }

    /// `send_keys`: the keys pressed in a terminal, once the person allowed it.
    fn keys_for_agent(
        &mut self,
        block: &str,
        keys: Vec<String>,
        cx: &mut Context<Self>,
    ) -> (Outcome, Option<Target>) {
        let Some((space, id)) = self.find_block(block) else {
            return (Outcome::Now(no_block(block)), None);
        };
        let Some(pane) = self.space_named(&space).and_then(|s| s.terminal(id)) else {
            return (
                Outcome::Now(Reply::err(format!("{block} isn't a terminal."))),
                None,
            );
        };
        if let Some(bad) = keys
            .iter()
            .find(|k| templum_terminal::named_key(k).is_none())
        {
            let why = format!(
                "{bad:?} isn't a key Templum knows. Keys: {}.",
                templum_terminal::KEY_NAMES
            );
            return (Outcome::Now(Reply::err(why)), None);
        }
        let worker = pane.term.id().to_string();
        let said = match keys.len() {
            1 => format!("Pressed 1 key in {block}."),
            n => format!("Pressed {n} keys in {block}."),
        };
        let pressed = self.press_keys(worker, "Templum Agent".into(), keys, said, cx);
        let target = Target {
            workspace: Some(space),
            block: Some(block.to_string()),
        };
        (Outcome::Later(pressed), Some(target))
    }

    /// `connect` (`on`) and `disconnect`: a wire between two blocks of one workspace, a step of
    /// that workspace's Ctrl+Z.
    fn wire_for_agent(
        &mut self,
        a: &str,
        b: &str,
        on: bool,
        cx: &mut Context<Self>,
    ) -> (Outcome, Option<Target>) {
        let (Some((space, x)), Some((other, y))) = (self.find_block(a), self.find_block(b)) else {
            let missing = if self.find_block(a).is_none() { a } else { b };
            return (Outcome::Now(no_block(missing)), None);
        };
        if space != other {
            let why = format!("{a} and {b} aren't in one workspace.");
            return (Outcome::Now(Reply::err(why)), None);
        }
        let changed = self
            .in_space(&space, |ws| {
                if on {
                    ws.connect(x, y, cx)
                } else {
                    ws.disconnect(templum_canvas::Wire { a: x, b: y }, cx)
                }
            })
            .unwrap_or(false);
        let done = match (on, changed) {
            (true, true) => Reply::ok(format!("Connected {a} and {b}.")),
            (true, false) => Reply::err(format!(
                "{a} and {b} can't be wired: they already are, or are one block."
            )),
            (false, true) => Reply::ok(format!("Disconnected {a} and {b}.")),
            (false, false) => Reply::err(format!("{a} and {b} aren't wired.")),
        };
        let target = changed.then(|| Target {
            workspace: Some(space),
            block: Some(a.to_string()),
        });
        (Outcome::Now(done), target)
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
    pub(crate) fn find_block(&self, uid: &str) -> Option<(String, BlockId)> {
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
