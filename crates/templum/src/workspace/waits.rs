//! Tool calls that wait (docs/specs/connections.md, Architecture): an agent's question for the
//! person. Each keeps its reply channel in `Workspace::waits` until it is answered, its deadline
//! passes, its call is cancelled or its connection ends, or its block closes.

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, Entity, Focusable as _, Subscription, Window};
use serde_json::Value;
use templum_mcp::{Replier, Reply, tools};

use super::Workspace;

/// A tool call waiting for its answer.
pub(crate) struct Wait {
    /// Names it for its deadline and the balloon's buttons; counts up, so the oldest is first.
    pub key: u64,
    /// The connection it came on, and its JSON-RPC id: a cancel names both.
    pub conn: u64,
    pub id: Value,
    reply: Replier,
    /// The terminal that asked (its `TEMPLUM_TERMINAL`): its block keeps it when it moves to
    /// another workspace.
    pub from: String,
    pub what: Waiting,
}

/// What a wait waits for.
pub(crate) enum Waiting {
    /// `ask_user`: one of `options`, or what the person types in `field` when there are none.
    Question {
        text: String,
        options: Vec<String>,
        field: Option<(Entity<InputState>, Subscription)>,
    },
}

/// A tool call that waits, as `serve` hands it over: where its answer goes.
pub(super) struct Asked {
    pub conn: u64,
    pub id: Value,
    pub reply: Replier,
    pub from: String,
}

impl Wait {
    /// What it asks the person, on one line: the dot's tooltip.
    pub fn summary(&self) -> String {
        match &self.what {
            Waiting::Question { text, .. } => text.lines().next().unwrap_or_default().to_string(),
        }
    }
}

impl Workspace {
    /// Keeps `asked` until what it waits for comes, or `seconds` pass; a question without options
    /// gets its field. The person hears of a request when its block isn't on screen.
    pub(super) fn wait(
        &mut self,
        asked: Asked,
        mut what: Waiting,
        seconds: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.next_wait += 1;
        let key = self.next_wait;
        if let Waiting::Question { options, field, .. } = &mut what
            && options.is_empty()
        {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Your answer"));
            let events = cx.subscribe_in(
                &input,
                window,
                move |ws, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        ws.send_field(key, window, cx);
                    }
                },
            );
            *field = Some((input, events));
        }
        // ponytail: one timer per wait, left to run out once it is answered (it then finds
        // nothing); a call waits an hour at most.
        cx.spawn(async move |ws, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(seconds))
                .await;
            ws.update(cx, |ws, cx| ws.expire(key, seconds, cx)).ok();
        })
        .detach();
        let wait = Wait {
            key,
            conn: asked.conn,
            id: asked.id,
            reply: asked.reply,
            from: asked.from,
            what,
        };
        self.tell_asking(&wait, window, cx);
        self.waits.push(wait);
        cx.notify();
    }

    /// Answers the wait `key`, once: it is gone after. False when it already was.
    pub(super) fn answer(&mut self, key: u64, reply: Reply, cx: &mut Context<Self>) -> bool {
        let Some(i) = self.waits.iter().position(|w| w.key == key) else {
            return false;
        };
        let wait = self.waits.remove(i);
        let _ = wait.reply.send(tools::tool_result(&wait.id, &reply));
        cx.notify();
        true
    }

    /// The deadline of the wait `key`: withdrawn, and the agent told so.
    fn expire(&mut self, key: u64, seconds: u64, cx: &mut Context<Self>) {
        let Some(wait) = self.waits.iter().find(|w| w.key == key) else {
            return;
        };
        let reply = match &wait.what {
            Waiting::Question { .. } => Reply::err(format!(
                "No one answered within {seconds} seconds; the request was withdrawn."
            )),
        };
        self.answer(key, reply, cx);
    }

    /// MCP's `notifications/cancelled` for the call `id` of connection `conn`: dropped, with no
    /// answer (the client wants none).
    pub(super) fn cancel(&mut self, conn: u64, id: &Value, cx: &mut Context<Self>) {
        self.waits.retain(|w| w.conn != conn || w.id != *id);
        cx.notify();
    }

    /// The connection `conn` ended (its agent quit, its command exited): what it waited for has
    /// no one to answer.
    pub(super) fn closed(&mut self, conn: u64, cx: &mut Context<Self>) {
        self.waits.retain(|w| w.conn != conn);
        cx.notify();
    }

    /// Each frame: a wait whose block is gone (closed, its shell exited, its workspace stopped)
    /// is withdrawn. A block moved to another workspace keeps its terminal, and its waits.
    pub(super) fn drop_orphans(&mut self, cx: &mut Context<Self>) {
        let orphans: Vec<(u64, Reply)> = self
            .waits
            .iter()
            .filter_map(|w| Some((w.key, self.orphaned(w)?)))
            .collect();
        for (key, reply) in orphans {
            self.answer(key, reply, cx);
        }
    }

    /// Why `wait` can't be answered any more, if so.
    fn orphaned(&self, wait: &Wait) -> Option<Reply> {
        if self.caller(&wait.from).is_none() {
            return Some(Reply::err(
                "Your block was closed; the request was withdrawn.",
            ));
        }
        match &wait.what {
            Waiting::Question { .. } => None,
        }
    }

    /// The person's answer to the question `key`: an option, or what they typed. The keyboard
    /// goes back to the canvas when the question's field had it.
    pub(crate) fn answer_question(
        &mut self,
        key: u64,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typing = self
            .waits
            .iter()
            .any(|w| w.key == key && field_focused(w, window, cx));
        if self.answer(key, Reply::ok(text), cx) && typing {
            window.focus(&self.focus_handle, cx);
        }
    }

    /// Send, or Enter in a question's field: its text answers, unless it is blank.
    pub(crate) fn send_field(&mut self, key: u64, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self
            .waits
            .iter()
            .find(|w| w.key == key)
            .and_then(|w| match &w.what {
                Waiting::Question {
                    field: Some((input, _)),
                    ..
                } => Some(input.read(cx).value().to_string()),
                _ => None,
            });
        if let Some(text) = typed.filter(|t| !t.trim().is_empty()) {
            self.answer_question(key, text, window, cx);
        }
    }

    /// What the agent in terminal `terminal` asks the person, oldest first: the first, and how
    /// many.
    pub(crate) fn balloon(&self, terminal: &str) -> Option<(&Wait, usize)> {
        let mut asking = self.waits.iter().filter(|w| w.from == terminal);
        let first = asking.next()?;
        Some((first, 1 + asking.count()))
    }

    /// The person isn't looking at the asking block (it is in another workspace, outside the
    /// view, or hidden by grid mode): a notification says what it asks; clicking it shows the
    /// block.
    fn tell_asking(&self, wait: &Wait, window: &mut Window, cx: &mut Context<Self>) {
        let Some((space_id, me)) = self.caller(&wait.from) else {
            return;
        };
        let shown = space_id == self.space.id
            && self.space.shown(me)
            && self
                .space
                .canvas
                .visible_from(&self.space.view, self.viewport(window))
                .any(|b| b.id == me);
        if shown {
            return;
        }
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let (_, title, _) = self.about(&space.blocks[&me], cx);
        let text = match &wait.what {
            Waiting::Question { text, .. } => format!("Asks you: {text}"),
        };
        self.notify_from(space_id, me, title, text, window, cx);
    }
}

/// The question's field has the keyboard.
fn field_focused(wait: &Wait, window: &Window, cx: &App) -> bool {
    matches!(
        &wait.what,
        Waiting::Question { field: Some((input, _)), .. }
            if input.read(cx).focus_handle(cx).is_focused(window)
    )
}
