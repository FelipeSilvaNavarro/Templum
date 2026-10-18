//! The Templum Agent (docs/specs/templum-agent.md): the chat the person has with an agent that
//! builds and runs the canvas, and its state on the workspace.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "the pill and the chat call it (chat/)")
)]

use std::collections::{BTreeSet, VecDeque};

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, Focusable as _, KeyDownEvent, Subscription, Task, Window};
use serde_json::Value;
use templum_agent::{Conversation, Engine};
use templum_canvas::BlockId;
use templum_mcp::Replier;
use templum_mcp::control::Parsed;

use crate::keys::Command;
use crate::workspace::Workspace;

pub(crate) mod pill;

/// The Templum Agent's state: one per Templum.
pub(crate) struct TemplumAgent {
    pub conversation: Conversation,
    /// The secret its engine's process says hello with (`TEMPLUM_CONTROL`): new at each start
    /// of the engine, so no older hello matches.
    pub secret: String,
    /// The tools "Allow for this conversation" let through, until New chat or quit.
    pub grants: BTreeSet<String>,
    /// The dangerous calls waiting for the person, oldest first.
    pub approvals: Vec<Approval>,
    pub next_approval: u64,
    /// It wrote something, or asks something, since the chat was last open.
    pub unread: bool,
    /// The chat is open.
    pub open: bool,
    /// Its engine's process, while it runs.
    pub engine: Option<Engine>,
    /// Counts the engine's starts: lines of an engine already gone are dropped.
    pub engine_gen: u64,
    /// Reads the engine's lines on the UI thread.
    pub lines: Option<Task<()>>,
    /// The engine's last lines on stderr, for the failure row when it ends.
    pub stderr: VecDeque<String>,
    /// The person pressed Stop: the turn's end is theirs, not a failure.
    pub stopping: bool,
    /// The settings changed while it worked: its process ends at the turn's end.
    pub restart: bool,
    /// The model the engine said it runs on.
    pub model: Option<String>,
    /// The pill's field, made the first time it takes the keyboard.
    pub input: Option<Entity<TextareaState>>,
    _enter: Option<Subscription>,
    /// The block that had the keyboard before the pill took it.
    pub back: Option<BlockId>,
    /// Whether Claude Code is installed, as the tests say it.
    #[cfg(test)]
    pub installed: Option<bool>,
    /// The fake engine the tests run instead of Claude Code.
    #[cfg(test)]
    pub launch: Option<templum_agent::Launch>,
    /// The words of each start of the engine, as Claude Code would have got them.
    #[cfg(test)]
    pub launched: Vec<Vec<String>>,
}

impl Default for TemplumAgent {
    fn default() -> Self {
        Self {
            conversation: Conversation::new("claude"),
            secret: uuid::Uuid::new_v4().simple().to_string(),
            grants: BTreeSet::new(),
            approvals: Vec::new(),
            next_approval: 0,
            unread: false,
            open: false,
            engine: None,
            engine_gen: 0,
            lines: None,
            stderr: VecDeque::new(),
            stopping: false,
            restart: false,
            model: None,
            input: None,
            _enter: None,
            back: None,
            #[cfg(test)]
            installed: None,
            #[cfg(test)]
            launch: None,
            #[cfg(test)]
            launched: Vec::new(),
        }
    }
}

/// A dangerous call held until the person answers (docs/specs/templum-agent.md#approvals): its
/// connection and request, to answer it; the call; and what the popup says of it, written by
/// Templum from the call.
pub(crate) struct Approval {
    pub id: u64,
    pub conn: u64,
    pub request: Value,
    pub reply: Replier,
    pub parsed: Parsed,
    /// The call's arguments as the engine sent them: its row in the chat.
    pub args: Value,
    pub tool: String,
    /// The action in one sentence: `Close "Codex · api"`.
    pub said: String,
    /// Exactly what will happen: the text to type, the keys, what stops.
    pub details: String,
}

/// The person's answer to an approval.
#[cfg_attr(not(test), expect(dead_code, reason = "the approval popup answers it"))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Answer {
    Once,
    Conversation,
    /// Denied, with their words when they wrote some.
    Deny(Option<String>),
}

impl TemplumAgent {
    /// Whether the engine's program can be run.
    pub fn installed(&self) -> bool {
        #[cfg(test)]
        if let Some(installed) = self.installed {
            return installed;
        }
        templum_terminal::installed("claude")
    }
}

/// The placeholder of the pill's field.
const PLACEHOLDER: &str = "Ask Templum to build, run or tidy your canvas…";

impl Workspace {
    /// The `agent` shortcut: the pill takes the keyboard (the chat opens when the conversation
    /// has rows); pressed again while the pill has it, the chat closes and the keyboard goes back.
    pub(crate) fn toggle_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agent_has_keys(window, cx) {
            self.leave_agent(window, cx);
            return;
        }
        if !self.templum_agent.installed() {
            self.open_agent_settings(window, cx);
            return;
        }
        let input = self.agent_input(window, cx);
        self.templum_agent.back = self.space.focused;
        input.update(cx, |input, cx| input.focus(window, cx));
        let agent = &mut self.templum_agent;
        if !agent.conversation.rows.is_empty() {
            agent.open = true;
            agent.unread = false;
        }
        cx.notify();
    }

    /// Review, on the pill that asks: the pill takes the keyboard and the chat opens at the
    /// approval.
    pub(crate) fn review_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.agent_has_keys(window, cx) {
            let input = self.agent_input(window, cx);
            self.templum_agent.back = self.space.focused;
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        self.templum_agent.open = true;
        self.templum_agent.unread = false;
        cx.notify();
    }

    /// Esc in the pill: the chat closes and the block that had the keyboard has it again. The
    /// text stays.
    pub(crate) fn leave_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.templum_agent.open = false;
        match self
            .templum_agent
            .back
            .take()
            .filter(|id| self.space.canvas.get(*id).is_some())
        {
            Some(id) => self.focus_block(id, window, cx),
            None => window.focus(&self.focus_handle, cx),
        }
        cx.notify();
    }

    /// Whether the pill's field has the keyboard.
    pub(crate) fn agent_has_keys(&self, window: &Window, cx: &gpui_kit::App) -> bool {
        self.templum_agent
            .input
            .as_ref()
            .is_some_and(|i| i.read(cx).focus_handle(cx).is_focused(window))
    }

    /// The pill's field: 1 to 8 lines, Enter sends, Shift+Enter is a new line.
    fn agent_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        if let Some(input) = &self.templum_agent.input {
            return input.clone();
        }
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true)
                .placeholder(PLACEHOLDER)
        });
        let enter = cx.subscribe_in(&input, window, |ws, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                ws.submit_to_agent(window, cx);
            }
        });
        self.templum_agent.input = Some(input.clone());
        self.templum_agent._enter = Some(enter);
        input
    }

    /// Enter in the pill: its text goes to the Templum Agent, the field empties, the chat opens.
    pub(crate) fn submit_to_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.templum_agent.installed() {
            self.open_agent_settings(window, cx);
            return;
        }
        let Some(input) = self.templum_agent.input.clone() else {
            return;
        };
        let text = input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        self.templum_agent.open = true;
        self.templum_agent.unread = false;
        self.send_to_agent(text, window, cx);
    }

    /// Settings ▸ Templum Agent.
    pub(crate) fn open_agent_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settings.as_ref().is_some_and(|p| !p.closing) {
            self.toggle_settings(window, cx);
        }
        self.set_section(crate::settings::Section::TemplumAgent, cx);
    }

    /// Keys while the pill or the chat has the keyboard: Esc gives it back; Templum's shortcuts
    /// run, as with the settings panel focused; nothing reaches a terminal.
    pub(crate) fn on_agent_key(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let k = &e.keystroke;
        if k.key == "escape" && !k.modifiers.modified() {
            self.leave_agent(window, cx);
            cx.stop_propagation();
            return;
        }
        // Copy, paste and close act on the focused terminal, which doesn't have the keyboard now.
        let command = self.keymap.command(k).filter(|c| {
            !matches!(
                c,
                Command::Copy
                    | Command::Paste
                    | Command::CloseTerminal
                    | Command::PreviousPrompt
                    | Command::NextPrompt
                    | Command::Find
            )
        });
        if command.is_some() {
            self.command_key(e, command, window, cx);
            cx.stop_propagation();
        }
    }
}
