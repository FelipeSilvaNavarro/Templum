//! Tool calls that wait (docs/specs/connections.md, Architecture): an agent's question for the
//! person. Each keeps its reply channel in `Workspace::waits` until it is answered, its deadline
//! passes, its call is cancelled or its connection ends, or its block closes.

use std::cell::Cell;
use std::time::{Duration, Instant};

use gpui_kit::component::input::InputState;
use gpui_kit::{App, Context, Entity, Focusable as _, Subscription, Window};
use serde_json::Value;
use templum_canvas::Level;
use templum_mcp::ask::{Ask, Form};
use templum_mcp::tools::Event;
use templum_mcp::{Replier, Reply, tools};

use super::bus::{Outcome, target};
use super::{AgentState, Pane, Workspace};

/// How long after the dialog first shows a question its keys and clicks are ignored: someone
/// typing or clicking in the terminal as it appears doesn't answer it.
pub(crate) const TOO_SOON: Duration = Duration::from_millis(500);
/// The lines of an agent's screen a turn's wait that times out gives.
pub(super) const LAST_LINES: usize = 40;
/// How often a `wait_for` a command or an alert looks at its terminal's counters.
const EVENT_LOOK: Duration = Duration::from_millis(250);
/// How long after a hook says its agent waits for its person its screen is read: the hook comes
/// before its dialog is drawn (it may answer the dialog; Codex's drew 25 ms after).
const DRAWN: Duration = Duration::from_millis(300);

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
    /// When the balloon first drew it, by the executor's clock.
    pub shown: Cell<Option<Instant>>,
}

/// A field of a question's dialog, and what it hears from it.
pub(crate) type Field = (Entity<InputState>, Subscription);

/// What a wait waits for.
pub(crate) enum Waiting {
    /// `ask_user` (docs/specs/ask-user.md): its questions, what the person picked, each
    /// question's Other field, and the note (asked with `questions` only). `wait` makes the
    /// fields. `away`: Esc gave the keyboard back to the terminal, until a press on the dialog.
    /// `stopped`: its client gave up on the call; the answer is typed into its terminal.
    Question {
        ask: Ask,
        form: Form,
        others: Vec<Field>,
        note: Option<Field>,
        away: bool,
        stopped: bool,
    },
    /// `connect`: the person's Allow or Deny to wiring the asking block to block `to` (its block
    /// id).
    Connect { to: String },
    /// `delegate` with `wait`, `wait_for_idle`: the end of the turn of the agent in the terminal
    /// `of` (the block `uid`).
    Turn { of: String, uid: String },
    /// `wait_for`: `event` in the terminal `of` (the block `uid`). `since`: the commands that had
    /// finished there and the alerts it had sent when the call came (`Status.finished`,
    /// `Status.attention`).
    Event {
        of: String,
        uid: String,
        event: Event,
        since: (u64, u64),
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
    /// Not drawn yet, or drawn under TOO_SOON ago: a press on it is ignored.
    pub fn too_soon(&self, cx: &App) -> bool {
        self.shown
            .get()
            .is_none_or(|at| cx.background_executor().now().duration_since(at) < TOO_SOON)
    }

    /// What it asks the person, on one line: the dot's tooltip.
    pub fn summary(&self) -> String {
        match &self.what {
            Waiting::Question { ask, .. } => ask.summary(),
            Waiting::Connect { to } => format!("connect to {to}"),
            Waiting::Turn { .. } | Waiting::Event { .. } => String::new(),
        }
    }
}

impl Workspace {
    /// Keeps `asked` until what it waits for comes, or `seconds` pass (None: no deadline); a
    /// question gets its fields. The person hears of a request when its block isn't on screen.
    pub(super) fn wait(
        &mut self,
        asked: Asked,
        mut what: Waiting,
        seconds: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.next_wait += 1;
        let key = self.next_wait;
        if let Waiting::Question {
            ask, others, note, ..
        } = &mut what
        {
            (*others, *note) = super::questions::fields(key, ask, window, cx);
        }
        // ponytail: one timer per wait, left to run out once it is answered (it then finds
        // nothing); a call waits an hour at most, a question a day.
        if let Some(seconds) = seconds {
            cx.spawn(async move |ws, cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(seconds))
                    .await;
                ws.update(cx, |ws, cx| ws.expire(key, seconds, cx)).ok();
            })
            .detach();
        }
        let wait = Wait {
            key,
            conn: asked.conn,
            id: asked.id,
            reply: asked.reply,
            from: asked.from,
            what,
            shown: Cell::new(None),
        };
        let looks = matches!(
            wait.what,
            Waiting::Event {
                event: Event::CommandFinished | Event::Attention,
                ..
            }
        );
        if matches!(
            wait.what,
            Waiting::Question { .. } | Waiting::Connect { .. }
        ) {
            self.tell_asking(&wait, window, cx);
            // Its block waits for its person now. The agent that handed it a task hears nothing:
            // the person sees the balloon, and that agent can't answer it.
            self.needs_you(&wait.from, Some(&wait.summary()), false, cx);
        }
        self.waits.push(wait);
        if looks {
            self.look_until_it_comes(key, cx);
        }
        cx.notify();
    }

    /// Answers the wait `key`, once: it is gone after. False when it already was. A question
    /// whose call was cancelled goes without an answer: MCP sends none to a cancelled call.
    pub(super) fn answer(&mut self, key: u64, reply: Reply, cx: &mut Context<Self>) -> bool {
        let Some(i) = self.waits.iter().position(|w| w.key == key) else {
            return false;
        };
        let wait = self.waits.remove(i);
        if !matches!(wait.what, Waiting::Question { stopped: true, .. }) {
            let _ = wait.reply.send(tools::tool_result(&wait.id, &reply));
        }
        cx.notify();
        true
    }

    /// The deadline of the wait `key`: withdrawn, and the agent told so.
    fn expire(&mut self, key: u64, seconds: u64, cx: &mut Context<Self>) {
        let Some(wait) = self.waits.iter().find(|w| w.key == key) else {
            return;
        };
        let screen = |of: &str| {
            let lines = self
                .pane(of)
                .map(|pane| pane.term.shared().lines(LAST_LINES).join("\n"))
                .unwrap_or_default();
            tools::tail(&lines, tools::MAX_TEXT).to_string()
        };
        let reply = match &wait.what {
            Waiting::Question { .. } | Waiting::Connect { .. } => Reply::err(format!(
                "No one answered within {seconds} seconds; the request was withdrawn."
            )),
            Waiting::Turn { of, uid } => Reply::err(format!(
                "No answer from {uid} within {seconds} seconds; it may still be working \
                 (wait_for_idle(\"{uid}\") waits again). The last lines of its screen:\n{}",
                screen(of)
            )),
            Waiting::Event { of, uid, event, .. } => Reply::err(format!(
                "No {} in {uid} within {seconds} seconds. The last lines of its screen:\n{}",
                awaited(*event),
                screen(of)
            )),
        };
        self.answer(key, reply, cx);
    }

    /// MCP's `notifications/cancelled` for the call `id` of connection `conn`: dropped, with no
    /// answer (the client wants none). A question stays, its answer to be typed into its
    /// terminal (docs/specs/ask-user.md, When the client stops waiting).
    pub(super) fn cancel(&mut self, conn: u64, id: &Value, cx: &mut Context<Self>) {
        self.waits.retain_mut(|w| {
            if w.conn != conn || w.id != *id {
                return true;
            }
            match &mut w.what {
                Waiting::Question { stopped, .. } => {
                    *stopped = true;
                    true
                }
                _ => false,
            }
        });
        cx.notify();
    }

    /// A call from the terminal `from` waits for the turn of the agent in the terminal `of`.
    pub(super) fn waits_on(&self, from: &str, of: &str) -> bool {
        self.waits
            .iter()
            .any(|w| w.from == from && matches!(&w.what, Waiting::Turn { of: o, .. } if o == of))
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
            Waiting::Connect { to } if !self.uid_taken(to) => Some(Reply::err(format!(
                "{to} was closed; the request was withdrawn."
            ))),
            Waiting::Turn { of, uid } if self.pane(of).is_none() => {
                Some(Reply::err(format!("{uid} was closed before it answered.")))
            }
            Waiting::Event { of, uid, .. } if self.pane(of).is_none() => {
                Some(Reply::err(format!("{uid} was closed while you waited.")))
            }
            Waiting::Question { .. }
            | Waiting::Connect { .. }
            | Waiting::Turn { .. }
            | Waiting::Event { .. } => None,
        }
    }

    /// `connect` from the terminal `from`: the person's answer to wait for, unless there is
    /// nothing to ask.
    pub(super) fn ask_to_connect(&self, from: &str, block: &str, timeout: u64) -> Outcome {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let Some(to) = space.block_by_uid(block) else {
            return Outcome::Now(Reply::err(format!(
                "There's no block {block} in your workspace: list_blocks lists them."
            )));
        };
        if to == me {
            return Outcome::Now(Reply::err(format!(
                "{block} is your own block: you may use it already."
            )));
        }
        if space.wired(me, to) {
            return Outcome::Now(Reply::ok(format!("{block} is already connected to you.")));
        }
        let asked = self
            .waits
            .iter()
            .any(|w| w.from == from && matches!(&w.what, Waiting::Connect { to } if to == block));
        if asked {
            return Outcome::Now(Reply::err(format!(
                "You already asked to connect to {block}; the person hasn't answered yet."
            )));
        }
        Outcome::Wait(
            Waiting::Connect {
                to: block.to_string(),
            },
            Some(timeout),
        )
    }

    /// The person's Allow (`allow`) or Deny to the request to connect `key`. Allow wires the two
    /// blocks, a step Ctrl+Z takes back, as wiring by hand is.
    pub(crate) fn answer_connect(&mut self, key: u64, allow: bool, cx: &mut Context<Self>) {
        let Some(wait) = self.waits.iter().find(|w| w.key == key) else {
            return;
        };
        let Waiting::Connect { to } = &wait.what else {
            return;
        };
        let to = to.clone();
        let reply = match self.caller(&wait.from) {
            _ if !allow => Reply::err(format!("The person didn't connect you to {to}.")),
            None => Reply::err("Your block was closed; the request was withdrawn."),
            Some((space_id, me)) => {
                let other = self
                    .space_named(&space_id)
                    .and_then(|space| space.block_by_uid(&to));
                match other {
                    Some(other) => {
                        self.in_space(&space_id, |ws| ws.connect(me, other, cx));
                        Reply::ok(format!("Connected to {to}."))
                    }
                    None => Reply::err(format!("{to} isn't in your workspace any more.")),
                }
            }
        };
        self.answer(key, reply, cx);
    }

    /// "*Claude Code* wants to connect to *Dev server*": a request to connect as the person reads
    /// it, the blocks by their titles.
    pub(crate) fn connect_line(&self, wait: &Wait, cx: &App) -> String {
        let Waiting::Connect { to } = &wait.what else {
            return String::new();
        };
        let Some((space_id, me)) = self.caller(&wait.from) else {
            return String::new();
        };
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let title = |id| self.about(&space.blocks[&id], cx).1;
        let from = title(me);
        let target = space.block_by_uid(to).map_or_else(|| to.clone(), title);
        format!("{from} wants to connect to {target}")
    }

    /// What the agent in terminal `terminal` asks the person, oldest first: the first, and how
    /// many.
    pub(crate) fn balloon(&self, terminal: &str) -> Option<(&Wait, usize)> {
        let mut asking = self.waits.iter().filter(|w| {
            w.from == terminal
                && matches!(w.what, Waiting::Question { .. } | Waiting::Connect { .. })
        });
        let first = asking.next()?;
        Some((first, 1 + asking.count()))
    }

    /// The terminal whose dialog has the keyboard: one of its fields has it, or its block does
    /// (focused, the workspace holding the keyboard), its oldest request is a question, and Esc
    /// didn't give the keyboard back.
    pub(crate) fn balloon_keys(&self, window: &Window, cx: &App) -> Option<&str> {
        if let Some(terminal) = self.balloon_typing(window, cx) {
            return Some(terminal);
        }
        if !self.canvas_has_keys(window) {
            return None;
        }
        let pane = self.space.focused_terminal()?;
        let (wait, _) = self.balloon(pane.term.id())?;
        matches!(wait.what, Waiting::Question { away: false, .. }).then_some(wait.from.as_str())
    }

    /// The terminal whose balloon's field has the keyboard, if one does: the keys are the field's.
    pub(crate) fn balloon_typing(&self, window: &Window, cx: &App) -> Option<&str> {
        self.waits
            .iter()
            .find(|w| field_focused(w, window, cx))
            .map(|w| w.from.as_str())
    }

    /// The person doesn't see the asking block's dialog (the block is in another workspace,
    /// outside the view, hidden by grid mode, or too small on screen for its dialog to be drawn,
    /// or the window is in the background): a notification says what it asks; clicking it shows
    /// the block.
    fn tell_asking(&self, wait: &Wait, window: &mut Window, cx: &mut Context<Self>) {
        let Some((space_id, me)) = self.caller(&wait.from) else {
            return;
        };
        // As `block::render` draws it: in full, or worked in.
        let full = self.space.focused == Some(me)
            || templum_canvas::level(self.cell.font_size, self.space.rest) == Level::Full;
        let shown = window.is_window_active()
            && space_id == self.space.id
            && self.space.shown(me)
            && full
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
            Waiting::Question { ask, .. } => format!("Asks you: {}", ask.summary()),
            Waiting::Connect { .. } => self.connect_line(wait, cx),
            Waiting::Turn { .. } | Waiting::Event { .. } => return,
        };
        self.notify_from(space_id, me, title, text, window, cx);
    }

    /// `wait_for` from the terminal `from`: `event` in the terminal `block`. `idle` is
    /// `wait_for_idle`; `needs_you` answers at once when its agent, or its balloon, already asks
    /// its person; the others wait for the next command or alert.
    pub(super) fn wait_for(&self, from: &str, block: &str, event: Event, timeout: u64) -> Outcome {
        if event == Event::Idle {
            return self.wait_for_idle(from, block, timeout);
        }
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return Outcome::Now(refused),
        };
        if to == me {
            return Outcome::Now(Reply::err("This block is yours: wait for another block."));
        }
        let Some(pane) = space.terminal(to) else {
            return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
        };
        let of = pane.term.id().to_string();
        if event == Event::NeedsYou {
            let asks = match &pane.state {
                Some(AgentState::NeedsYou(what)) => Some(what.clone()),
                _ => self.balloon(&of).map(|(wait, _)| Some(wait.summary())),
            };
            if let Some(what) = asks {
                return Outcome::Now(waiting_for_person(block, what.as_deref(), &pane.tail()));
            }
        }
        let status = pane.term.status();
        let what = Waiting::Event {
            of,
            uid: block.to_string(),
            event,
            since: (status.finished, status.attention),
        };
        Outcome::Wait(what, Some(timeout))
    }

    /// The block of the terminal `terminal` waits for its person (its agent asks for `what`, or
    /// its balloon does): the `wait_for(needs_you)` on it are answered DRAWN later, with its
    /// screen as drawn by then. `tell`: a new wait, which the agent that handed it a task hears of,
    /// unless a `wait_for(needs_you)` of its was just answered with it.
    pub(super) fn needs_you(
        &mut self,
        terminal: &str,
        what: Option<&str>,
        tell: bool,
        cx: &mut Context<Self>,
    ) {
        let terminal = terminal.to_string();
        let what = what.map(str::to_string);
        cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(DRAWN).await;
            let _ = ws.update(cx, |ws, cx| {
                let screen = ws.pane(&terminal).map(Pane::tail).unwrap_or_default();
                let waiting: Vec<(u64, String, String)> = ws
                    .waits
                    .iter()
                    .filter_map(|w| match &w.what {
                        Waiting::Event {
                            of,
                            uid,
                            event: Event::NeedsYou,
                            ..
                        } if *of == terminal => Some((w.key, uid.clone(), w.from.clone())),
                        _ => None,
                    })
                    .collect();
                // The agent that handed it its task, when a call of its hears the dialog now.
                let asker = ws
                    .pane(&terminal)
                    .and_then(|p| p.handed.as_ref())
                    .map(|h| h.by.clone());
                let heard = waiting
                    .iter()
                    .any(|(_, _, from)| Some(from) == asker.as_ref());
                for (key, uid, _) in waiting {
                    ws.answer(key, waiting_for_person(&uid, what.as_deref(), &screen), cx);
                }
                if tell && !heard {
                    ws.tell_waiting(&terminal, what.as_deref(), &screen, cx);
                }
            });
        })
        .detach();
    }

    /// Looks at the terminal a `wait_for(command_finished | attention)` watches, every EVENT_LOOK,
    /// until the wait `key` is gone: the terminal's own thread counts its commands and alerts, so
    /// no frame has to come (a hidden window draws none).
    fn look_until_it_comes(&self, key: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |ws, cx| {
            loop {
                cx.background_executor().timer(EVENT_LOOK).await;
                let waiting = ws.update(cx, |ws, cx| ws.look(key, cx)).unwrap_or(false);
                if !waiting {
                    break;
                }
            }
        })
        .detach();
    }

    /// The wait `key` is answered when its command or alert came since the call, or its block
    /// closed. False once the wait is gone.
    fn look(&mut self, key: u64, cx: &mut Context<Self>) -> bool {
        let Some(wait) = self.waits.iter().find(|w| w.key == key) else {
            return false;
        };
        let Waiting::Event {
            of,
            uid,
            event,
            since,
        } = &wait.what
        else {
            return false;
        };
        // Its block closed: answered here, as no frame may come to `drop_orphans`.
        let Some(pane) = self.pane(of) else {
            let reply = Reply::err(format!("{uid} was closed while you waited."));
            self.answer(key, reply, cx);
            return false;
        };
        let status = pane.term.status();
        let reply = match event {
            Event::CommandFinished if status.finished > since.0 => {
                let code = match status.last.and_then(|o| o.code) {
                    Some(code) => format!("with exit code {code}"),
                    None => "without an exit code".to_string(),
                };
                let mut text = format!("A command finished in {uid}, {code}.");
                if let Some(output) = pane.term.shared().last_command() {
                    text.push('\n');
                    text.push_str(tools::tail(&output, tools::MAX_TEXT));
                }
                Reply::ok(text)
            }
            Event::Attention if status.attention > since.1 => Reply::ok(match status.message {
                Some(message) => format!("{uid} asks for attention: {message}"),
                None => format!("{uid} rang its bell."),
            }),
            _ => return true,
        };
        self.answer(key, reply, cx);
        false
    }
}

/// One of the question's fields has the keyboard.
pub(super) fn field_focused(wait: &Wait, window: &Window, cx: &App) -> bool {
    let Waiting::Question { others, note, .. } = &wait.what else {
        return false;
    };
    others
        .iter()
        .chain(note)
        .any(|(input, _)| input.read(cx).focus_handle(cx).is_focused(window))
}

/// How long a question waits, seconds: the shorter of the call's `timeout` and the setting's
/// minutes when not 0; with neither, until it is answered (None).
pub(super) fn deadline(call: Option<u64>, setting_minutes: u64) -> Option<u64> {
    let setting = (setting_minutes > 0).then_some(setting_minutes * 60);
    match (call, setting) {
        (Some(call), Some(setting)) => Some(call.min(setting)),
        (call, setting) => call.or(setting),
    }
}

/// `wait_for(needs_you)`'s answer: block `uid` waits for its person, for `what` when it says,
/// with its `screen`'s last lines (the dialog as drawn).
fn waiting_for_person(uid: &str, what: Option<&str>, screen: &str) -> Reply {
    let asks = match what.filter(|w| !w.is_empty()) {
        Some(what) => format!("{uid} is waiting for its person: {what}"),
        None => format!("{uid} is waiting for its person."),
    };
    Reply::ok(format!(
        "{asks} Its screen:\n{}",
        tools::tail(screen, tools::MAX_TEXT)
    ))
}

/// What a `wait_for` waited for, as its timeout says it: "No call for attention in t4q8p…".
fn awaited(event: Event) -> &'static str {
    match event {
        Event::Idle => "end of its turn",
        Event::NeedsYou => "wait for its person",
        Event::CommandFinished => "finished command",
        Event::Attention => "call for attention",
    }
}
