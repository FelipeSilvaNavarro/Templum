//! The bus's messages, answered on the UI thread (docs/specs/connections.md): who calls, the
//! tools, and the agents' turns.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use futures::StreamExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Task, Window};
use serde_json::json;
use templum_canvas::BlockId;
use templum_mcp::ask::Form;
use templum_mcp::{Bus, Call, Incoming, Message, Reply, rpc, tools};

use super::delegate::ENTER_AFTER;
use super::waits::{Asked, Waiting, deadline};
use super::{AgentState, Body, Pane, Space, Workspace, show_notification};

/// Starts the bus in the data folder, and the task that hands its messages to the workspace.
/// Without a data folder (the bench), or when the socket can't be made, there is none.
pub(super) fn start(
    data: Option<&Path>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> (Option<Bus>, Vec<(String, String)>, Task<()>) {
    let Some(data) = data else {
        return (None, Vec::new(), Task::ready(()));
    };
    let (tx, mut rx) = futures::channel::mpsc::unbounded::<Message>();
    let bus = match templum_mcp::bus::start(data, tx) {
        Ok(bus) => bus,
        Err(err) => {
            eprintln!(
                "templum: couldn't start the bus in {}: {err}",
                data.display()
            );
            return (None, Vec::new(), Task::ready(()));
        }
    };
    let env = terminal_env(&bus, data);
    let task = cx.spawn_in(window, async move |ws, cx| {
        while let Some(message) = rx.next().await {
            if ws
                .update_in(cx, |ws, window, cx| ws.serve(message, window, cx))
                .is_err()
            {
                break;
            }
        }
    });
    (Some(bus), env, task)
}

/// What a tool call comes to.
pub(super) enum Outcome {
    /// Its answer, now.
    Now(Reply),
    /// It waits for this, at most these many seconds (`waits.rs`); None: until it comes.
    Wait(Waiting, Option<u64>),
    /// A job answers it when it is done (`spawn_agent` making a worktree).
    Later(Task<Reply>),
}

/// What every terminal gets while the bus runs: its address and Templum's folder first in
/// PATH, where `templum` is this Templum. Taken for each terminal: PATH may have changed since.
pub(super) fn terminal_env(bus: &Bus, data: &Path) -> Vec<(String, String)> {
    let mut env = vec![("TEMPLUM_BUS".to_string(), bus.env.clone())];
    let path = templum_terminal::path_var().unwrap_or_default();
    if let Some(path) = templum_folder(data).and_then(|folder| first_on_path(folder, &path)) {
        env.push(("PATH".to_string(), path));
    }
    env
}

/// `path` (a PATH's value) with `folder` in front, without its empty entries: an empty one is
/// the current folder, where any `templum` or `git` a repository holds would run.
pub(super) fn first_on_path(folder: PathBuf, path: &OsStr) -> Option<String> {
    let rest = std::env::split_paths(path).filter(|p| !p.as_os_str().is_empty());
    std::env::join_paths(std::iter::once(folder).chain(rest))
        .ok()?
        .into_string()
        .ok()
}

/// `<data>/bin`, where `templum` is this Templum: on Unix a link to it; on Windows
/// `templum.exe`, the same file or a copy (its own folder may be Downloads, with no
/// `templum.exe` and any other program in it).
// ponytail: one entry for every Templum sharing the data folder: the last one started wins.
fn templum_folder(data: &Path) -> Option<PathBuf> {
    let exe = super::usage::program()?;
    let bin = data.join("bin");
    std::fs::create_dir_all(&bin).ok()?;
    #[cfg(unix)]
    {
        let link = templum_in(data);
        if std::fs::read_link(&link).ok().as_deref() != Some(exe.as_path()) {
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(&exe, &link).ok()?;
        }
    }
    #[cfg(not(unix))]
    same_or_copy(&exe, &templum_in(data)).ok()?;
    Some(bin)
}

/// `<data>/bin/templum` (`templum.exe` on Windows): this Templum, for what outlives it (another
/// agent's configuration, the Gemini CLI extension).
pub(crate) fn templum_in(data: &Path) -> PathBuf {
    data.join("bin").join(if cfg!(windows) {
        "templum.exe"
    } else {
        "templum"
    })
}

/// Makes `to` a hard link to `exe`, or a copy of it where a link can't be made (another drive),
/// unless its length and modified time are already `exe`'s.
#[cfg_attr(unix, allow(dead_code))] // Used on Windows; tested everywhere.
pub(super) fn same_or_copy(exe: &Path, to: &Path) -> std::io::Result<()> {
    let source = std::fs::metadata(exe)?;
    let modified = source.modified()?;
    if std::fs::metadata(to)
        .is_ok_and(|m| m.len() == source.len() && m.modified().ok() == Some(modified))
    {
        return Ok(());
    }
    let _ = std::fs::remove_file(to);
    if std::fs::hard_link(exe, to).is_err() {
        std::fs::copy(exe, to)?;
        // A copy may get the time it is made (Unix's does): it would be made again at each start.
        let _ = std::fs::File::options()
            .write(true)
            .open(to)
            .and_then(|f| f.set_modified(modified));
    }
    Ok(())
}

impl Workspace {
    /// Answers one message from the bus. A tool call that waits is kept (`waits.rs`) and answered
    /// later; meanwhile the connection's next lines are served.
    pub(super) fn serve(&mut self, message: Message, window: &mut Window, cx: &mut Context<Self>) {
        let Message {
            hello,
            conn,
            incoming,
            reply,
        } = message;
        match incoming {
            Incoming::Request { id, method, params } => {
                let answer = match method.as_str() {
                    "initialize" => {
                        let instructions = self.instructions(&hello.terminal, cx);
                        rpc::result(&id, tools::initialize_result(&params, &instructions))
                    }
                    "ping" => rpc::result(&id, json!({})),
                    "tools/list" => rpc::result(&id, tools::list()),
                    "tools/call" => {
                        let name = params["name"].as_str().unwrap_or_default();
                        if hello.client == "mcp" {
                            self.allowed(&hello.terminal, name, cx);
                        }
                        let cli_only = hello.client == "cli" && tools::cli_only(name);
                        if !tools::known(name) && !cli_only {
                            rpc::error(&id, rpc::PARAMS, &format!("Unknown tool: {name}"))
                        } else {
                            let outcome = match tools::parse(name, &params["arguments"]) {
                                Ok(call) => self.call(&hello.terminal, call, window, cx),
                                Err(why) => Outcome::Now(Reply::err(why)),
                            };
                            match outcome {
                                Outcome::Now(done) => tools::tool_result(&id, &done),
                                Outcome::Wait(what, seconds) => {
                                    let asked = Asked {
                                        conn,
                                        id,
                                        reply,
                                        from: hello.terminal,
                                    };
                                    self.wait(asked, what, seconds, window, cx);
                                    return;
                                }
                                Outcome::Later(job) => {
                                    cx.spawn(async move |_, _| {
                                        let done = job.await;
                                        let _ = reply.send(tools::tool_result(&id, &done));
                                    })
                                    .detach();
                                    return;
                                }
                            }
                        }
                    }
                    _ => rpc::error(&id, rpc::METHOD, &format!("Method not found: {method}")),
                };
                let _ = reply.send(answer);
            }
            Incoming::Notification { method, params } if method == templum_mcp::hooks::METHOD => {
                if let Ok(event) = serde_json::from_value(params) {
                    self.agent_event(&hello.terminal, event, cx);
                }
            }
            // The client gave up on a call (MCP): no answer goes to it.
            Incoming::Notification { method, params } if method == "notifications/cancelled" => {
                self.cancel(conn, &params["requestId"], cx);
            }
            Incoming::Notification { method, .. } if method == templum_mcp::bus::CLOSED => {
                self.closed(conn, cx);
            }
            // `notifications/initialized` and the rest need nothing.
            Incoming::Notification { .. } | Incoming::Response => {}
        }
    }

    /// An agent's hook says where its turn is, and its screen's looks start over. Its turn's end
    /// answers what waited on it.
    fn agent_event(
        &mut self,
        terminal: &str,
        event: templum_mcp::AgentEvent,
        cx: &mut Context<Self>,
    ) {
        use templum_mcp::AgentEvent;
        let Some(pane) = self.pane_mut(terminal) else {
            return;
        };
        // Keys typed before this event answer nothing it brings (a dialog opening now).
        pane.looks.heard();
        pane.typed.set(false);
        if !matches!(event, AgentEvent::Stopped { .. }) {
            pane.speaks = true;
        }
        match event {
            AgentEvent::Submitted { at_enter } => {
                let turn = pane.state == Some(AgentState::Working);
                pane.state = Some(AgentState::Working);
                pane.submitted += 1;
                // Its task, once typed: a spawn's prompt waits in `prompt` until then.
                let typed = pane.prompt.is_none();
                if let Some(handed) = pane.handed.as_mut().filter(|h| typed && !h.taken) {
                    handed.taken = true;
                    // Claude Code says so at its Enter, also for a prompt it queues behind its turn.
                    handed.behind = turn && at_enter;
                }
            }
            AgentEvent::Working => {
                // At work again after a permission. Never a new turn: a command an interrupt left
                // running ends later (Codex).
                if matches!(
                    pane.state,
                    Some(AgentState::Working | AgentState::NeedsYou(_))
                ) {
                    pane.state = Some(AgentState::Working);
                }
                // A tool's end takes the prompt Claude Code queued into the turn under way.
                if let Some(handed) = pane.handed.as_mut() {
                    handed.behind = false;
                }
            }
            AgentEvent::Started => {
                pane.state = Some(AgentState::Idle);
                // A new session: the last one's transcript isn't its own.
                pane.transcript = None;
            }
            AgentEvent::Stopped {
                message,
                transcript,
            } => {
                if transcript.is_some() {
                    pane.transcript = transcript;
                }
                pane.state = Some(AgentState::Idle);
                if let Some(message) = &message {
                    pane.answer = Some(message.clone());
                }
                // The turn the task was typed in on, or the one Claude Code queued it behind: the
                // next turn's end is its own.
                if let Some(handed) = pane.handed.as_mut().filter(|h| !h.taken || h.behind) {
                    handed.behind = false;
                } else {
                    let reply = match message {
                        Some(message) => Reply::ok(message),
                        None => Reply::err(
                            "Its turn ended without an answer: an error, or it was interrupted.",
                        ),
                    };
                    self.end_turn(terminal, reply, cx);
                }
            }
            // A wait that says nothing new (Claude Code's notification about the dialog its
            // `PermissionRequest` opened) keeps what it asks: `allowed` looks for it.
            AgentEvent::NeedsYou { message: None }
                if matches!(pane.state, Some(AgentState::NeedsYou(Some(_)))) => {}
            AgentEvent::NeedsYou { message } => {
                // Already waiting: the same dialog (Claude Code's notification about it).
                let new = !matches!(pane.state, Some(AgentState::NeedsYou(_)));
                pane.state = Some(AgentState::NeedsYou(message.clone()));
                self.needs_you(terminal, message.as_deref(), new, cx);
            }
            AgentEvent::Ended => {
                pane.state = None;
                pane.speaks = false;
                self.agent_left(terminal, cx);
            }
        }
        cx.notify();
    }

    /// The agent in `terminal` calls the Templum tool `name`: when it was asking its person to
    /// allow that tool (`hooks::claude`), they did, and it works again. Its next hook may come
    /// only at the tool's end, minutes later. Another tool's dialog may be a subagent's.
    fn allowed(&mut self, terminal: &str, name: &str, cx: &mut Context<Self>) {
        let Some(pane) = self.pane_mut(terminal) else {
            return;
        };
        let asked = format!("Allow mcp__templum__{name}?");
        if pane.state == Some(AgentState::NeedsYou(Some(asked))) {
            pane.state = Some(AgentState::Working);
            pane.looks.heard();
            pane.typed.set(false);
            cx.notify();
        }
    }

    /// The block whose terminal is `terminal` (its `TEMPLUM_TERMINAL`): its workspace's id and
    /// its id. Looked up at each call: a block moved to another workspace keeps its terminal.
    pub(super) fn caller(&self, terminal: &str) -> Option<(String, BlockId)> {
        self.spaces().find_map(|space| {
            space
                .terminals()
                .find(|(_, pane)| pane.term.id() == terminal)
                .map(|(id, _)| (space.id.clone(), id))
        })
    }

    /// The pane of the terminal `terminal` (its `TEMPLUM_TERMINAL`), in any workspace.
    pub(super) fn pane(&self, terminal: &str) -> Option<&Pane> {
        self.spaces()
            .flat_map(|s| s.blocks.values())
            .filter_map(Body::terminal)
            .find(|pane| pane.term.id() == terminal)
    }

    pub(super) fn pane_mut(&mut self, terminal: &str) -> Option<&mut Pane> {
        std::iter::once(&mut self.space)
            .chain(&mut self.others)
            .flat_map(|s| s.blocks.values_mut())
            .filter_map(Body::terminal_mut)
            .find(|pane| pane.term.id() == terminal)
    }

    pub(super) fn space_named(&self, id: &str) -> Option<&Space> {
        self.spaces().find(|s| s.id == id)
    }

    /// A tool call from the terminal `terminal`: its answer now, or what it waits for.
    fn call(
        &mut self,
        terminal: &str,
        call: Call,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let Some((space_id, me)) = self.caller(terminal) else {
            return Outcome::Now(Reply::err("This terminal isn't a block of this Templum."));
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        Outcome::Now(match call {
            Call::Whoami => {
                let (kind, title, folder) = self.about(&space.blocks[&me], cx);
                let mut text = format!(
                    "id: {}\nkind: {kind}\ntitle: {title}\n",
                    space.uids.get(&me).map_or("", String::as_str)
                );
                if let Some(folder) = folder {
                    text.push_str(&format!("folder: {}\n", folder.display()));
                }
                text.push_str(&format!("workspace: {}", space.name));
                Reply::ok(text)
            }
            Call::ListBlocks => Reply::ok(self.block_lines(space, me, cx).join("\n")),
            Call::ReadTerminal {
                block,
                lines,
                last_command,
            } => {
                let id = match target(space, me, &block) {
                    Ok(id) => id,
                    Err(reply) => return Outcome::Now(reply),
                };
                let Some(pane) = space.terminal(id) else {
                    return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
                };
                let shared = pane.term.shared();
                if last_command {
                    match shared.last_command() {
                        Some(text) => Reply::ok(tools::tail(&text, tools::MAX_TEXT)),
                        None => Reply::err(format!(
                            "No command has finished in {block} yet (it needs the shell's integration)."
                        )),
                    }
                } else {
                    Reply::ok(tools::tail(
                        &shared.lines(lines).join("\n"),
                        tools::MAX_TEXT,
                    ))
                }
            }
            Call::SendToTerminal { block, text, enter } => {
                let id = match target(space, me, &block) {
                    Ok(id) => id,
                    Err(reply) => return Outcome::Now(reply),
                };
                let Some(pane) = space.terminal(id) else {
                    return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
                };
                pane.term.type_in(&text);
                if !enter {
                    return Outcome::Now(Reply::ok(format!("Typed into {block}.")));
                }
                // As a task's: an agent may take keys that come in one burst for a paste, where
                // Enter is a new line. Answered after it: a caller that waits for each answer
                // (a script's `templum send` twice) gets its texts and Enters in order.
                let terminal = pane.term.id().to_string();
                return Outcome::Later(cx.spawn(async move |ws, cx| {
                    cx.background_executor().timer(ENTER_AFTER).await;
                    let entered = ws.update(cx, |ws, _| {
                        ws.pane(&terminal)
                            .map(|pane| pane.term.write(&b"\r"[..]))
                            .is_some()
                    });
                    if entered.unwrap_or(false) {
                        Reply::ok(format!("Typed into {block}."))
                    } else {
                        Reply::err(format!("{block} was closed before its Enter."))
                    }
                }));
            }
            Call::Notify { text } => {
                let (_, title, _) = self.about(&space.blocks[&me], cx);
                self.notify_from(space_id, me, title, text, window, cx);
                Reply::ok("Shown.")
            }
            Call::Connect { block, timeout } => {
                return self.ask_to_connect(terminal, &block, timeout);
            }
            Call::Delegate {
                block,
                task,
                wait,
                timeout,
            } => return self.delegate(terminal, &block, &task, wait, timeout, cx),
            Call::ReplyTo { block, text } => self.reply_to(terminal, &block, &text, cx),
            Call::WaitForIdle { block, timeout } => {
                return self.wait_for_idle(terminal, &block, timeout);
            }
            Call::SpawnAgent(spawn) => return self.spawn_agent(terminal, spawn, window, cx),
            Call::WaitFor {
                block,
                event,
                timeout,
            } => return self.wait_for(terminal, &block, event, timeout),
            Call::Wire { a, b } => self.wire_blocks(terminal, &a, &b, cx),
            Call::ListCards { board, card } => {
                self.list_cards(terminal, board.as_deref(), card, cx)
            }
            Call::CreateCard {
                board,
                column,
                title,
                description,
            } => self.create_card(
                terminal,
                board.as_deref(),
                column.as_deref(),
                &title,
                description,
                cx,
            ),
            Call::MoveCard { card, column } => self.move_card(terminal, card, &column, cx),
            Call::CommentCard { card, text } => self.comment_card(terminal, card, &text, cx),
            Call::ClaimCard { card } => self.claim_card(terminal, card, cx),
            Call::ReadNote { block } => self.read_note(terminal, &block, cx),
            Call::AppendNote { block, text } => {
                self.append_note(terminal, &block, &text, window, cx)
            }
            Call::ReadChecklist { block } => self.read_checklist(terminal, &block),
            Call::CheckItem { block, item, done } => {
                self.check_item(terminal, &block, item, done, cx)
            }
            Call::AddItems { block, text } => self.add_items(terminal, &block, &text, cx),
            Call::OpenFile { path, line } => self.open_file_for(terminal, &path, line, window, cx),
            Call::SendKeys { block, keys } => return self.send_keys(terminal, &block, keys, cx),
            Call::Interrupt { block } => return self.interrupt(terminal, &block, cx),
            Call::CloseBlock { block } => self.close_block(terminal, &block, window, cx),
            Call::AskUser { ask, timeout } => {
                let what = Waiting::Question {
                    form: Form::new(&ask),
                    ask,
                    others: Vec::new(),
                    note: None,
                    away: false,
                    stopped: false,
                };
                let seconds = deadline(timeout, self.config.connections.question_timeout);
                return Outcome::Wait(what, seconds);
            }
        })
    }

    /// A notification from block `me` of workspace `space`, titled `title`; clicking it shows the
    /// block. In the background it is a system notification too.
    pub(super) fn notify_from(
        &self,
        space: String,
        me: BlockId,
        title: String,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ws = cx.entity().downgrade();
        let notification = Notification::new()
            .title(title)
            .message(text)
            .delivery(self.delivery(window))
            .on_click(move |_, window, cx| {
                ws.update(cx, |ws, cx| ws.reveal(&space, me, window, cx))
                    .ok();
            });
        show_notification(window, cx, notification);
    }

    /// A block's kind, title (made safe to hand an agent) and folder, as `whoami` and
    /// `list_blocks` say them.
    pub(super) fn about(&self, body: &Body, cx: &App) -> (&'static str, String, Option<PathBuf>) {
        let title = || crate::block::title(self, body, cx).to_string();
        let (kind, title, folder) = match body {
            Body::Terminal(pane) => (
                "terminal",
                self.agent_name(pane)
                    .map_or_else(|| pane.term.title(), str::to_string),
                pane.seen.folder.clone(),
            ),
            Body::Kanban(_) => ("kanban", title(), None),
            Body::Note(_) => ("note", title(), None),
            Body::Checklist(_) => ("checklist", title(), None),
            Body::Editor(e) => ("editor", title(), Some(e.folder.clone())),
        };
        (kind, clean_title(&title), folder)
    }

    /// The title agents see for the block `uid`, in any workspace; None when none has it.
    pub(crate) fn uid_title(&self, uid: &str, cx: &App) -> Option<String> {
        self.spaces().find_map(|s| {
            let id = s.block_by_uid(uid)?;
            Some(self.about(s.blocks.get(&id)?, cx).1)
        })
    }

    /// One line per block of `space`, back to front: id, kind, title, folder, and its tie to
    /// the caller `me`.
    fn block_lines(&self, space: &Space, me: BlockId, cx: &App) -> Vec<String> {
        space
            .canvas
            .blocks()
            .iter()
            .map(|b| {
                let (kind, title, folder) = self.about(&space.blocks[&b.id], cx);
                let uid = space.uids.get(&b.id).map_or("", String::as_str);
                let folder = folder
                    .map(|f| format!("  {}", f.display()))
                    .unwrap_or_default();
                let tie = if b.id == me {
                    "you"
                } else if space.wired(me, b.id) {
                    "connected"
                } else {
                    "not connected"
                };
                let state = space
                    .terminal(b.id)
                    .filter(|pane| pane.seen.agent.is_some())
                    .and_then(|pane| pane.state.as_ref())
                    .map(|state| match state {
                        AgentState::Working => ", working".to_string(),
                        AgentState::Idle => ", idle".to_string(),
                        AgentState::NeedsYou(Some(what)) if !what.is_empty() => {
                            format!(", waiting for its person: {}", clean_title(what))
                        }
                        AgentState::NeedsYou(_) => ", waiting for its person".to_string(),
                    })
                    .unwrap_or_default();
                format!("{uid}  {kind}  {title}{folder}  {tie}{state}")
            })
            .collect()
    }

    /// What `initialize` tells the agent in `terminal` about itself.
    pub(super) fn instructions(&self, terminal: &str, cx: &App) -> String {
        let Some((space_id, me)) = self.caller(terminal) else {
            return "This terminal isn't a block of this Templum: its tools won't work here."
                .into();
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let (_, title, _) = self.about(&space.blocks[&me], cx);
        let uid = space.uids.get(&me).map_or("", String::as_str);
        // The other blocks, split by their wire to the caller.
        let (connected, others): (Vec<_>, Vec<_>) = space
            .canvas
            .blocks()
            .iter()
            .filter(|b| b.id != me)
            .map(|b| {
                let (kind, title, _) = self.about(&space.blocks[&b.id], cx);
                let uid = space.uids.get(&b.id).map_or("", String::as_str);
                (space.wired(me, b.id), format!("{uid} ({kind} {title:?})"))
            })
            .partition(|(wired, _)| *wired);
        let list = |blocks: Vec<(bool, String)>| {
            if blocks.is_empty() {
                "none".to_string()
            } else {
                blocks
                    .into_iter()
                    .map(|(_, b)| b)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        // Agents first: Claude Code shows its own Agent tool whole and Templum's tools by name
        // only, Codex weighs the first 512 characters most, and Claude Code cuts at 4096 the
        // block lists may outgrow.
        format!(
            "In Templum, an agent the person asks for is a block on this canvas: start one with \
             spawn_agent, or hand work to an agent connected to you with delegate. Your built-in \
             subagents run hidden inside you, where the person can't see them: use them only when \
             the person asks for a subagent. \
             You are block {uid}, {title:?}, in Templum workspace {:?}. Connected to you: {}. In this \
             workspace, not connected (ask with connect): {}. Name blocks by these ids. You may only read \
             or write your own block and those connected to you; connect asks the person for another. \
             list_blocks gives the current list. When a connected kanban has your task, move its card \
             to Doing when you start; when you are done, comment on it what you did and what the next \
             agent needs to know (comment_card), then move it to Review (Done when the board has no \
             Review); write what the next agent needs to know in the connected note too, and check \
             the items of a connected checklist as you finish them. A task you \
             hand over without wait comes back typed into your terminal as [task …]: it isn't the \
             person writing.",
            space.name,
            list(connected),
            list(others),
        )
    }
}

/// The most characters of a title an agent is told.
const TITLE_CHARS: usize = 60;

/// Text as agents are told it, on one line: no control characters, no invisible ones (zero
/// widths, direction overrides, soft hyphens, tags, which can carry hidden text), its line breaks
/// and runs of blanks as one space.
pub(super) fn flat(text: &str) -> String {
    let invisible = |c: char| {
        matches!(
            c,
            '\u{00AD}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
                | '\u{E0000}'..='\u{E007F}'
        )
    };
    let kept: String = text
        .chars()
        .filter(|&c| (c.is_whitespace() || !c.is_control()) && !invisible(c))
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A title as agents are told it. A terminal's is whatever its programs set (OSC 0), and goes
/// into an agent's instructions: `flat`, and at most 60 characters.
pub(super) fn clean_title(title: &str) -> String {
    let flat = flat(title);
    if flat.chars().count() <= TITLE_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(TITLE_CHARS - 1).collect();
    format!("{cut}…")
}

/// The block `uid` names in `space`, when the caller `me` may use it: its own, or one wired to
/// it, looked at on each call.
pub(super) fn target(space: &Space, me: BlockId, uid: &str) -> Result<BlockId, Reply> {
    let Some(id) = space.block_by_uid(uid) else {
        return Err(Reply::err(format!(
            "There's no block {uid} in your workspace: list_blocks lists them."
        )));
    };
    if id != me && !space.wired(me, id) {
        return Err(Reply::err(format!(
            "{uid} isn't connected to you. Ask the person with connect(\"{uid}\")."
        )));
    }
    Ok(id)
}
