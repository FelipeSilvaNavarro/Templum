//! Tasks handed between agents (docs/specs/connections.md, Delegating): typed into an agent wired
//! to the one handing it over; its answer is the end of that agent's turn, or its `reply`.

use std::time::{Duration, Instant};

use gpui_kit::Context;
use templum_mcp::Reply;

use super::bus::{Outcome, target};
use super::waits::Waiting;
use super::{AgentState, Handed, Pane, Workspace};

/// How long after a task is typed its Enter goes: an agent may take keys that come in one burst
/// for a paste, where Enter is a new line.
pub(super) const ENTER_AFTER: Duration = Duration::from_millis(150);

impl Pane {
    /// The task its agent works on, while its answer is awaited.
    pub fn task(&self, now: Instant) -> Option<&Handed> {
        self.handed.as_ref().filter(|h| now < h.until)
    }

    /// Its agent is at work: on a task, on a turn, or on a question to its person.
    pub fn busy(&self, now: Instant) -> bool {
        self.task(now).is_some()
            || matches!(
                self.state,
                Some(AgentState::Working | AgentState::NeedsYou(_))
            )
    }

    /// Templum may type into it (its block id `uid`): an agent runs there, not waiting for its
    /// person (a permission dialog would take the text as its answer). Never a shell, where the
    /// text would run as a command.
    fn takes_typing(&self, uid: &str) -> Result<(), Reply> {
        if self.seen.agent.is_none() {
            return Err(Reply::err(format!(
                "{uid} runs no agent: send_to_terminal types into a terminal."
            )));
        }
        if matches!(self.state, Some(AgentState::NeedsYou(_))) {
            return Err(Reply::err(format!(
                "{uid} is waiting for its person: Templum won't type into it now. Try again later."
            )));
        }
        Ok(())
    }
}

impl Workspace {
    /// Types `text` into the agent in terminal `terminal` as one paste, then Enter a moment later.
    pub(super) fn send_task(&self, terminal: &str, text: &str, cx: &mut Context<Self>) {
        let Some(pane) = self.pane(terminal) else {
            return;
        };
        pane.term.type_in(text);
        let terminal = terminal.to_string();
        cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(ENTER_AFTER).await;
            ws.update(cx, |ws, _| {
                if let Some(pane) = ws.pane(&terminal) {
                    pane.term.write(&b"\r"[..]);
                }
            })
            .ok();
        })
        .detach();
    }

    /// `delegate` from the terminal `from`: `task` typed into the agent `block` under a line
    /// saying who hands it over; with `wait`, the call waits for its answer.
    pub(super) fn delegate(
        &mut self,
        from: &str,
        block: &str,
        task: &str,
        wait: bool,
        timeout: u64,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let now = Instant::now();
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return Outcome::Now(refused),
        };
        if to == me {
            return Outcome::Now(Reply::err(
                "This block is yours: hand the task to another agent.",
            ));
        }
        let Some(pane) = space.terminal(to) else {
            return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
        };
        if let Err(refused) = pane.takes_typing(block) {
            return Outcome::Now(refused);
        }
        if pane.task(now).is_some() {
            return Outcome::Now(Reply::err(format!(
                "{block} is already on a task another agent handed it: wait_for_idle(\"{block}\") \
                 waits for its end."
            )));
        }
        let worker = pane.term.id().to_string();
        let (_, title, _) = self.about(&space.blocks[&me], cx);
        let uid = space.uids.get(&me).cloned().unwrap_or_default();
        self.send_task(&worker, &format!("[from {title} ({uid})]\n{task}"), cx);
        if let Some(pane) = self.pane_mut(&worker) {
            pane.handed = Some(Handed {
                by: from.to_string(),
                since: now,
                until: now + Duration::from_secs(timeout),
            });
        }
        cx.notify();
        if wait {
            let what = Waiting::Turn {
                of: worker,
                uid: block.to_string(),
            };
            Outcome::Wait(what, timeout)
        } else {
            Outcome::Now(Reply::ok(format!(
                "Handed to {block}. wait_for_idle(\"{block}\") gives its answer once it is done, \
                 unless it replies sooner."
            )))
        }
    }

    /// `reply` from the terminal `from` to the agent `block`: its calls waiting on the caller
    /// (`delegate` with `wait`, `wait_for_idle`) get `text`; with none, the text is typed into it
    /// as a message. Either way the task it handed the caller is answered.
    pub(super) fn reply_to(
        &mut self,
        from: &str,
        block: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return refused,
        };
        if to == me {
            return Reply::err("This block is yours: reply to the agent that handed you the task.");
        }
        let Some(pane) = space.terminal(to) else {
            return Reply::err(format!("{block} isn't a terminal."));
        };
        let asker = pane.term.id().to_string();
        let waiting: Vec<u64> = self
            .waits
            .iter()
            .filter(|w| {
                w.from == asker && matches!(&w.what, Waiting::Turn { of, .. } if of == from)
            })
            .map(|w| w.key)
            .collect();
        if waiting.is_empty() {
            if let Err(refused) = pane.takes_typing(block) {
                return refused;
            }
            let (_, title, _) = self.about(&space.blocks[&me], cx);
            let uid = space.uids.get(&me).cloned().unwrap_or_default();
            self.send_task(&asker, &format!("[reply from {title} ({uid})]\n{text}"), cx);
        }
        if let Some(mine) = self.pane_mut(from) {
            if mine.handed.as_ref().is_some_and(|h| h.by == asker) {
                mine.handed = None;
            }
            mine.answer = Some(text.to_string());
        }
        let answered = !waiting.is_empty();
        for key in waiting {
            self.answer(key, Reply::ok(text), cx);
        }
        cx.notify();
        Reply::ok(if answered {
            format!("Answered {block}.")
        } else {
            format!("Typed into {block}.")
        })
    }

    /// `wait_for_idle` from the terminal `from` on the agent `block`: its last answer now when it
    /// is idle, else once its turn, or the task it is on, ends.
    pub(super) fn wait_for_idle(&self, from: &str, block: &str, timeout: u64) -> Outcome {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return Outcome::Now(refused),
        };
        if to == me {
            return Outcome::Now(Reply::err("This block is yours: wait for another agent."));
        }
        let Some(pane) = space.terminal(to) else {
            return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
        };
        if pane.seen.agent.is_none() {
            return Outcome::Now(Reply::err(format!(
                "{block} runs no agent: read_terminal reads its screen."
            )));
        }
        if pane.busy(Instant::now()) {
            let what = Waiting::Turn {
                of: pane.term.id().to_string(),
                uid: block.to_string(),
            };
            return Outcome::Wait(what, timeout);
        }
        Outcome::Now(Reply::ok(pane.answer.clone().unwrap_or_else(|| {
            format!("{block} is idle; Templum has heard no answer from it.")
        })))
    }

    /// The agent in the terminal `terminal` ended its turn, or its session: the task it was on is
    /// no longer handed over, and the calls waiting on it get `reply`.
    pub(super) fn end_turn(&mut self, terminal: &str, reply: Reply, cx: &mut Context<Self>) {
        if let Some(pane) = self.pane_mut(terminal) {
            pane.handed = None;
        }
        let waiting: Vec<u64> = self
            .waits
            .iter()
            .filter(|w| matches!(&w.what, Waiting::Turn { of, .. } if of == terminal))
            .map(|w| w.key)
            .collect();
        for key in waiting {
            self.answer(key, reply.clone(), cx);
        }
    }
}
