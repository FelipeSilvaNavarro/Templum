//! The bus's messages, answered on the UI thread (docs/specs/connections.md): who calls, the
//! tools, and the agents' turns.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use futures::StreamExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Task, Window};
use serde_json::json;
use templum_canvas::BlockId;
use templum_mcp::{Bus, Call, Incoming, Message, Reply, rpc, tools};

use super::{AgentState, Body, Space, Workspace, show_notification};

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

/// What every terminal gets while the bus runs: its address and Templum's folder first in
/// PATH, where `templum` is this Templum.
fn terminal_env(bus: &Bus, data: &Path) -> Vec<(String, String)> {
    let mut env = vec![("TEMPLUM_BUS".to_string(), bus.env.clone())];
    let path = std::env::var_os("PATH").unwrap_or_default();
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

/// `<data>/bin`, where a link named `templum` points at this Templum.
// ponytail: one link for every Templum sharing the data folder: the last one started wins.
#[cfg(unix)]
fn templum_folder(data: &Path) -> Option<PathBuf> {
    let exe = super::usage::program()?;
    let bin = data.join("bin");
    std::fs::create_dir_all(&bin).ok()?;
    let link = bin.join("templum");
    if std::fs::read_link(&link).ok().as_deref() != Some(exe.as_path()) {
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&exe, &link).ok()?;
    }
    Some(bin)
}

/// The folder of Templum's program, where `templum` is `templum.exe`: no link needed.
#[cfg(not(unix))]
fn templum_folder(_: &Path) -> Option<PathBuf> {
    Some(super::usage::program()?.parent()?.to_path_buf())
}

impl Workspace {
    /// Answers one message from the bus.
    pub(super) fn serve(&mut self, message: Message, window: &mut Window, cx: &mut Context<Self>) {
        let Message {
            hello,
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
                        if tools::known(name) {
                            let reply = match tools::parse(name, &params["arguments"]) {
                                Ok(call) => self.call(&hello.terminal, call, window, cx),
                                Err(why) => Reply::err(why),
                            };
                            tools::tool_result(&id, &reply)
                        } else {
                            rpc::error(&id, rpc::PARAMS, &format!("Unknown tool: {name}"))
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
            // `notifications/initialized` and the rest need nothing.
            Incoming::Notification { .. } | Incoming::Response => {}
        }
    }

    /// An agent's hook says where its turn is.
    fn agent_event(
        &mut self,
        terminal: &str,
        event: templum_mcp::AgentEvent,
        cx: &mut Context<Self>,
    ) {
        use templum_mcp::AgentEvent;
        let pane = std::iter::once(&mut self.space)
            .chain(&mut self.others)
            .flat_map(|s| s.blocks.values_mut())
            .filter_map(Body::terminal_mut)
            .find(|pane| pane.term.id() == terminal);
        let Some(pane) = pane else { return };
        match event {
            AgentEvent::Working => pane.state = Some(AgentState::Working),
            // Its last answer waits for delegating (batch 3).
            AgentEvent::Started | AgentEvent::Stopped { .. } => {
                pane.state = Some(AgentState::Idle);
            }
            AgentEvent::NeedsYou { message } => pane.state = Some(AgentState::NeedsYou(message)),
            AgentEvent::Ended => pane.state = None,
        }
        cx.notify();
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

    fn space_named(&self, id: &str) -> Option<&Space> {
        self.spaces().find(|s| s.id == id)
    }

    fn call(
        &self,
        terminal: &str,
        call: Call,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let Some((space_id, me)) = self.caller(terminal) else {
            return Reply::err("This terminal isn't a block of this Templum.");
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        match call {
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
                    Err(reply) => return reply,
                };
                let Some(pane) = space.terminal(id) else {
                    return Reply::err(format!("{block} isn't a terminal."));
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
                    Err(reply) => return reply,
                };
                let Some(pane) = space.terminal(id) else {
                    return Reply::err(format!("{block} isn't a terminal."));
                };
                pane.term.type_in(&text);
                if enter {
                    pane.term.write(&b"\r"[..]);
                }
                Reply::ok(format!("Typed into {block}."))
            }
            Call::Notify { text } => {
                let (_, title, _) = self.about(&space.blocks[&me], cx);
                let ws = cx.entity().downgrade();
                let notification = Notification::new()
                    .title(title)
                    .message(text)
                    .delivery(self.delivery(window))
                    .on_click(move |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.reveal(&space_id, me, window, cx))
                            .ok();
                    });
                show_notification(window, cx, notification);
                Reply::ok("Shown.")
            }
        }
    }

    /// A block's kind, title (made safe to hand an agent) and folder, as `whoami` and
    /// `list_blocks` say them.
    fn about(&self, body: &Body, cx: &App) -> (&'static str, String, Option<PathBuf>) {
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
            Body::Editor(e) => ("editor", title(), Some(e.folder.clone())),
            Body::Git(_) => ("git", title(), body.folder()),
        };
        (kind, clean_title(&title), folder)
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
                let tie = if b.id == me { "you" } else { "not connected" };
                let state = space
                    .terminal(b.id)
                    .filter(|pane| pane.seen.agent.is_some())
                    .and_then(|pane| pane.state.as_ref())
                    .map(|state| match state {
                        AgentState::Working => ", working",
                        AgentState::Idle => ", idle",
                        AgentState::NeedsYou(_) => ", waiting for its person",
                    })
                    .unwrap_or_default();
                format!("{uid}  {kind}  {title}{folder}  {tie}{state}")
            })
            .collect()
    }

    /// What `initialize` tells the agent in `terminal` about itself.
    fn instructions(&self, terminal: &str, cx: &App) -> String {
        let Some((space_id, me)) = self.caller(terminal) else {
            return "This terminal isn't a block of this Templum: its tools won't work here."
                .into();
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let (_, title, _) = self.about(&space.blocks[&me], cx);
        let uid = space.uids.get(&me).map_or("", String::as_str);
        let others: Vec<String> = space
            .canvas
            .blocks()
            .iter()
            .filter(|b| b.id != me)
            .map(|b| {
                let (kind, title, _) = self.about(&space.blocks[&b.id], cx);
                format!(
                    "{} ({kind} {title:?})",
                    space.uids.get(&b.id).map_or("", String::as_str)
                )
            })
            .collect();
        format!(
            "You are block {uid}, {title:?}, in Templum workspace {:?}. Other blocks there: {}. Name \
             blocks by these ids. You may read and type into your own block; the person connects \
             others to you by wiring them on the canvas. list_blocks gives the current list.",
            space.name,
            if others.is_empty() {
                "none".to_string()
            } else {
                others.join(", ")
            },
        )
    }
}

/// The most characters of a title an agent is told.
const TITLE_CHARS: usize = 60;

/// A title as agents are told it. A terminal's is whatever its programs set (OSC 0), and goes
/// into an agent's instructions: no control characters, its line breaks and runs of blanks as
/// one space, and at most 60 characters.
pub(super) fn clean_title(title: &str) -> String {
    let kept: String = title
        .chars()
        .filter(|c| c.is_whitespace() || !c.is_control())
        .collect();
    let flat = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= TITLE_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(TITLE_CHARS - 1).collect();
    format!("{cut}…")
}

/// The block `uid` names in `space`, when the caller `me` may use it: until wires (batch 2),
/// only its own.
fn target(space: &Space, me: BlockId, uid: &str) -> Result<BlockId, Reply> {
    let Some(id) = space.block_by_uid(uid) else {
        return Err(Reply::err(format!(
            "There's no block {uid} in your workspace: list_blocks lists them."
        )));
    };
    if id != me {
        return Err(Reply::err(format!(
            "{uid} isn't connected to you. Ask the person to wire it to your block on the canvas."
        )));
    }
    Ok(id)
}
