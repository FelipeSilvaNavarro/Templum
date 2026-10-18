//! The Templum Agent's engine (docs/specs/templum-agent.md#a-conversations-life): started with the
//! first message, resumed by its session after Stop, a crash, a settings change or a restart, its
//! lines folded into the conversation, and the conversation's file.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "the pill and the chat call it (chat/)")
)]

use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt as _;
use gpui_kit::{Context, Window};
use templum_agent::claude::{self, Session};
use templum_agent::{ChatEvent, Conversation, Engine, EngineLine, Launch, Load};

use super::Workspace;
use super::approvals::STOPPED;
use super::bus::templum_in;
use crate::chat::Answer;

/// How many of the engine's stderr lines are kept.
const STDERR_LINES: usize = 20;
/// How long a turn has to end after Stop before its process is ended.
const STOP_WAIT: Duration = Duration::from_secs(2);

impl Workspace {
    /// `<data>/agent/conversation.json`; None without a data folder.
    fn conversation_file(&self) -> Option<PathBuf> {
        Some(
            self.data_dir
                .as_ref()?
                .join("agent")
                .join("conversation.json"),
        )
    }

    /// The conversation as it was when Templum quit. One that doesn't read is set aside, and the
    /// chat starts empty.
    pub(super) fn restore_conversation(&mut self) {
        let Some(path) = self.conversation_file() else {
            return;
        };
        match Conversation::load(&path) {
            Load::Some(conversation) => self.templum_agent.conversation = *conversation,
            Load::Unreadable => eprintln!(
                "templum: {} didn't read: it is set aside as conversation.json.bak",
                path.display()
            ),
            Load::None => {}
        }
    }

    pub(super) fn save_conversation(&self) {
        let Some(path) = self.conversation_file() else {
            return;
        };
        if let Err(e) = self.templum_agent.conversation.save(&path) {
            eprintln!("templum: couldn't write {}: {e}", path.display());
        }
    }

    /// The person sends `text`: what waits for their approval is denied with it, and the engine
    /// takes it, started first when none runs.
    pub(crate) fn send_to_agent(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if text.trim().is_empty() {
            return;
        }
        let waiting: Vec<u64> = self.templum_agent.approvals.iter().map(|a| a.id).collect();
        for id in waiting {
            self.answer_approval(id, Answer::Deny(Some(text.clone())), window, cx);
        }
        self.templum_agent.conversation.person(&text);
        if self.templum_agent.engine.is_none()
            && let Err(why) = self.start_engine(cx)
        {
            self.templum_agent.conversation.ended(&why);
            self.save_conversation();
            cx.notify();
            return;
        }
        let sent = self
            .templum_agent
            .engine
            .as_mut()
            .map(|engine| engine.send(&claude::message(&text)));
        if let Some(Err(e)) = sent {
            self.kill_engine();
            let why = format!("Claude Code stopped: {e}. Your next message starts it again.");
            self.templum_agent.conversation.ended(&why);
            self.deny_waiting(STOPPED, cx);
            self.save_conversation();
        }
        cx.notify();
    }

    /// Starts the engine on the conversation's session (a new one when it has none), with the
    /// settings' model and effort, a new secret, and the control server as its only MCP server.
    fn start_engine(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let data = self
            .data_dir
            .clone()
            .ok_or("Templum runs without its data folder: the Templum Agent needs one.")?;
        let bus = self
            .bus
            .as_ref()
            .ok_or("Templum's bus isn't running: the Templum Agent can't reach the canvas.")?
            .env
            .clone();
        let dir = data.join("agent");
        let mcp_file = claude::write_mcp(&dir, &templum_in(&data))
            .map_err(|e| format!("Couldn't write {}: {e}", dir.join("mcp.json").display()))?;
        let session = match &self.templum_agent.conversation.session {
            Some(session) => Session::Resume(session.clone()),
            None => Session::New(uuid::Uuid::new_v4().to_string()),
        };
        let settings = &self.config.templum_agent;
        let given = |v: &str| (!v.is_empty()).then(|| v.to_string());
        let options = claude::Options {
            session,
            model: given(&settings.model),
            effort: given(&settings.effort),
            add_dirs: self.agent_folders(),
            mcp_file,
        };
        let secret = uuid::Uuid::new_v4().simple().to_string();
        let env = vec![
            ("TEMPLUM_BUS".to_string(), bus),
            ("TEMPLUM_CONTROL".to_string(), secret.clone()),
        ];
        let args = claude::args(&options);
        #[cfg(test)]
        self.templum_agent.launched.push(args.clone());
        #[cfg(test)]
        let fake = self.templum_agent.launch.clone();
        #[cfg(not(test))]
        let fake: Option<Launch> = None;
        let launch = match fake {
            Some(mut fake) => {
                fake.env.extend(env);
                fake
            }
            None => Launch {
                program: templum_terminal::locate("claude")
                    .ok_or("Claude Code isn't installed: Templum can't find claude.")?,
                args,
                cwd: dir,
                env,
                env_remove: vec!["TEMPLUM_TERMINAL".into()],
            },
        };
        let (lines, mut read) = futures::channel::mpsc::unbounded();
        let engine =
            Engine::start(&launch, lines).map_err(|e| format!("Claude Code didn't start: {e}"))?;
        let agent = &mut self.templum_agent;
        agent.secret = secret;
        agent.engine = Some(engine);
        agent.engine_gen += 1;
        agent.stderr.clear();
        agent.stopping = false;
        let generation = agent.engine_gen;
        agent.lines = Some(cx.spawn(async move |ws, cx| {
            while let Some(line) = read.next().await {
                if ws
                    .update(cx, |ws, cx| ws.engine_line(generation, line, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        Ok(())
    }

    /// The folders it may read: every workspace's and every block's, each once.
    fn agent_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = Vec::new();
        for space in self.spaces() {
            let blocks = space
                .blocks
                .values()
                .filter_map(|b| b.folder().or_else(|| b.terminal()?.seen.folder.clone()));
            for folder in space.folder.clone().into_iter().chain(blocks) {
                if !folders.contains(&folder) {
                    folders.push(folder);
                }
            }
        }
        folders
    }

    /// One line of the engine `generation`: its stream into the conversation, its stderr kept,
    /// its end told.
    fn engine_line(&mut self, generation: u64, line: EngineLine, cx: &mut Context<Self>) {
        if generation != self.templum_agent.engine_gen {
            return;
        }
        match line {
            EngineLine::Err(line) => {
                let stderr = &mut self.templum_agent.stderr;
                stderr.push_back(line);
                if stderr.len() > STDERR_LINES {
                    stderr.pop_front();
                }
            }
            EngineLine::Out(line) => {
                for event in claude::parse(&line) {
                    let agent = &mut self.templum_agent;
                    let ended = matches!(event, ChatEvent::TurnEnded { .. });
                    match event {
                        // The interrupt's end isn't a failure: the person stopped it.
                        ChatEvent::TurnEnded { ms, cost_usd, .. } if agent.stopping => {
                            let end = ChatEvent::TurnEnded {
                                ms,
                                cost_usd,
                                error: None,
                            };
                            agent.conversation.apply(end);
                            agent.conversation.stopped();
                            agent.stopping = false;
                        }
                        event => agent.conversation.apply(event),
                    }
                    agent.unread |= !agent.open;
                    if ended {
                        self.save_conversation();
                        let agent = &self.templum_agent;
                        if agent.restart && !agent.conversation.working {
                            self.kill_engine();
                        }
                    }
                }
            }
            EngineLine::Ended => {
                let working = self.templum_agent.conversation.working;
                self.kill_engine();
                if working {
                    let last = self
                        .templum_agent
                        .stderr
                        .iter()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .map_or("its process ended".to_string(), |l| l.trim().to_string());
                    let why =
                        format!("Claude Code stopped: {last}. Your next message starts it again.");
                    self.templum_agent.conversation.ended(&why);
                    self.deny_waiting(STOPPED, cx);
                    self.save_conversation();
                }
            }
        }
        cx.notify();
    }

    /// Ends the engine's process, if one runs: its lines from then on are dropped.
    fn kill_engine(&mut self) {
        let agent = &mut self.templum_agent;
        if let Some(mut engine) = agent.engine.take() {
            engine.stop();
        }
        agent.engine_gen += 1;
        agent.lines = None;
        agent.restart = false;
        agent.stopping = false;
    }

    /// Stop: what waits for the person is denied, and the turn is interrupted, its process kept;
    /// when the turn hasn't ended STOP_WAIT later, the process is ended.
    pub(crate) fn stop_engine(&mut self, cx: &mut Context<Self>) {
        self.deny_waiting(STOPPED, cx);
        let agent = &mut self.templum_agent;
        let Some(engine) = agent.engine.as_mut() else {
            return;
        };
        if !agent.conversation.working {
            return;
        }
        agent.stopping = true;
        let generation = agent.engine_gen;
        if engine
            .send(&claude::interrupt(&format!("stop-{generation}")))
            .is_err()
        {
            self.kill_engine();
            self.templum_agent.conversation.stopped();
            self.save_conversation();
            cx.notify();
            return;
        }
        cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(STOP_WAIT).await;
            let _ = ws.update(cx, |ws, cx| {
                let agent = &ws.templum_agent;
                if agent.engine_gen == generation && agent.stopping {
                    ws.kill_engine();
                    ws.templum_agent.conversation.stopped();
                    ws.save_conversation();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// New chat: the engine stopped, the session, rows, its own things and grants forgotten.
    pub(crate) fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.deny_waiting(STOPPED, cx);
        self.kill_engine();
        let agent = &mut self.templum_agent;
        agent.conversation = Conversation::new(&self.config.templum_agent.agent);
        agent.conversation.note("New conversation.");
        agent.grants.clear();
        self.save_conversation();
        cx.notify();
    }

    /// Its agent, model or effort changed: the next start takes them. An idle engine ends now
    /// (its next message resumes the session); one at work ends at its turn's end.
    pub(super) fn engine_settings_changed(&mut self) {
        let agent = &mut self.templum_agent;
        if agent.engine.is_none() {
            return;
        }
        if agent.conversation.working {
            agent.restart = true;
        } else {
            self.kill_engine();
        }
    }
}
