//! Tasks handed between agents (docs/specs/connections.md, Delegating): typed into an agent wired
//! to the one handing it over; its answer is the end of that agent's turn, or its `reply`.

use std::time::{Duration, Instant};

use gpui_kit::{App, Context, Task};
use templum_canvas::BlockId;
use templum_mcp::{Reply, tools};
use templum_terminal::{KEY_NAMES, named_key};

use super::bus::{Outcome, target};
use super::waits::Waiting;
use super::{AgentState, Handed, Pane, Space, Workspace};

/// How long after a task is typed its Enter goes: an agent may take keys that come in one burst
/// for a paste, where Enter is a new line.
pub(super) const ENTER_AFTER: Duration = Duration::from_millis(150);
/// How many times, ENTER_AFTER apart, Templum looks for the typed task on the agent's screen
/// before it gives up on its Enter: about 2 s, for an agent slow to draw.
const ENTER_LOOKS: u32 = 13;
/// The lines at the end of a terminal compared to tell whether its screen changed.
const SCREEN_LINES: usize = 100;
/// Looks in a row the screen must stay the same after the agent's terminal is resized: it has
/// redrawn at its new size, and read what was typed meanwhile.
const SETTLED: u32 = 2;
/// How long after a task's Enter an agent whose hooks speak must say it took it (`Submitted`), or
/// open a dialog: only a prompt taken confirms it.
pub(super) const CONFIRM: Duration = Duration::from_secs(3);
/// The person typed into an agent this recently: they are still typing, and a second Enter for a
/// task would send their half-written line.
pub(super) const TYPING: Duration = Duration::from_secs(3);
/// Between two keys `send_keys` presses: an Esc and the key after it in one burst read as
/// Alt+key.
const KEY_GAP: Duration = Duration::from_millis(50);
/// How long after its last key `send_keys` reads the screen it answers with.
const KEYS_SETTLE: Duration = Duration::from_millis(300);
/// The last lines of a screen Templum answers with (`send_keys`, a task not taken).
pub(super) const TAIL_LINES: usize = 20;
/// What a task's first line asks, after who hands it over: Claude Code takes a long paste alone
/// for data it was shown, and does what typed text with it asks.
pub(super) const TASK_LEAD: &str = "Your task is in the pasted text: do it.";
/// How long a terminal says which block pressed keys in it ("answered by").
pub(crate) const ANSWERED_FOR: Duration = Duration::from_secs(10);

/// What a look at an agent's screen, while a typed task waits for its Enter, says to do.
#[derive(Debug, PartialEq)]
enum Step {
    Wait,
    Enter,
    GiveUp,
}

/// The wait for a typed task's Enter (`send_task`): the agent's screen must show the paste. A
/// resize meanwhile redraws it at its new size, which shows nothing of the paste.
struct EnterWait {
    /// The screen the paste must change.
    before: String,
    size: (u16, u16),
    /// Since a resize: the last screen, and how many looks in a row saw it.
    settling: Option<(String, u32)>,
    looks: u32,
}

impl EnterWait {
    fn new(before: String, size: (u16, u16)) -> Self {
        Self {
            before,
            size,
            settling: None,
            looks: 0,
        }
    }

    /// A look at the agent's `screen`, its terminal now `size`; `takes`: Templum may still type
    /// into it; `speaks`: its hooks say when it starts a turn or asks its person, so a dialog of
    /// it would be heard, and its Enter is confirmed. After a resize, the screen settling shows
    /// such an agent read the paste; for another (Codex, whose approval Templum doesn't hear) it
    /// is the new start, and only a change after it is the paste.
    fn look(&mut self, screen: String, size: (u16, u16), takes: bool, speaks: bool) -> Step {
        if !takes {
            return Step::GiveUp;
        }
        self.looks += 1;
        if size != self.size {
            self.size = size;
            self.settling = Some((screen, 0));
            self.looks = 0;
            return Step::Wait;
        }
        if let Some((last, still)) = self.settling.take() {
            let still = if screen == last { still + 1 } else { 0 };
            if still >= SETTLED {
                if speaks {
                    return Step::Enter;
                }
                self.before = screen;
                self.looks = 0;
                return Step::Wait;
            }
            self.settling = Some((screen, still));
            return match (self.looks < ENTER_LOOKS, speaks) {
                (true, _) => Step::Wait,
                // Moving all along: alive, and reading what it is typed.
                (false, true) => Step::Enter,
                (false, false) => Step::GiveUp,
            };
        }
        if screen != self.before {
            Step::Enter
        } else if self.looks < ENTER_LOOKS {
            Step::Wait
        } else {
            Step::GiveUp
        }
    }
}

impl Pane {
    /// Who pressed keys in it (`send_keys`, `interrupt`), while ANSWERED_FOR hasn't passed.
    pub fn answered_by(&self) -> Option<&str> {
        self.keys_from
            .as_ref()
            .filter(|(_, at)| at.elapsed() < ANSWERED_FOR)
            .map(|(by, _)| by.as_str())
    }

    /// The person typed into it: a key, a paste, text.
    pub(super) fn person_typed(&self) {
        self.typed.set(true);
        self.keyed.set(Some(Instant::now()));
    }

    /// The last `TAIL_LINES` lines its terminal shows, as an agent is answered with them.
    pub(super) fn tail(&self) -> String {
        self.term.shared().lines(TAIL_LINES).join("\n")
    }

    /// The end of what its terminal shows, to tell whether it changed.
    pub fn screen(&self) -> String {
        self.term.shared().lines(SCREEN_LINES).join("\n")
    }

    /// The task its agent works on, while its answer is awaited.
    pub fn task(&self, now: Instant) -> Option<&Handed> {
        self.handed.as_ref().filter(|h| now < h.until)
    }

    /// Its agent is at work: on a task, on a turn, on a question to its person, or about to get
    /// its prompt.
    pub fn busy(&self, now: Instant) -> bool {
        self.task(now).is_some()
            || self.prompt.is_some()
            || matches!(
                self.state,
                Some(AgentState::Working | AgentState::NeedsYou(_))
            )
    }

    /// Its shell, not the agent, is in front: an agent seen there quit, though the last snapshot
    /// (up to 5 s old) still has it. Its process group, found in front by a snapshot, tells (on
    /// macOS the shell isn't the terminal's process, `login` is); before one found it, its shell
    /// being in front does. False when the OS can't tell (Windows).
    pub(super) fn shell_in_front(&self) -> bool {
        let p = self.term.processes();
        match (p.foreground, self.seen.pid) {
            (None, _) => false,
            (front, Some(agent)) => front != Some(agent),
            (front, None) => front == p.shell,
        }
    }

    /// Templum may type into it (its block id `uid`): an agent runs there, not waiting for its
    /// person (a permission dialog would take the text as its answer). Never a shell, where the
    /// text would run as a command.
    fn takes_typing(&self, uid: &str) -> Result<(), Reply> {
        if self.seen.agent.is_none() || self.shell_in_front() {
            return Err(Reply::err(format!(
                "{uid} runs no agent: send_to_terminal types into a terminal."
            )));
        }
        if let Some(AgentState::NeedsYou(what)) = &self.state {
            let asks = match what.as_deref().filter(|w| !w.is_empty()) {
                Some(what) => format!(": {what}"),
                None => String::new(),
            };
            return Err(Reply::err(format!(
                "{uid} is waiting for its person{asks}. Templum won't type into it now; send_keys \
                 answers its dialog. Its screen:\n{}",
                tools::tail(&self.tail(), tools::MAX_TEXT)
            )));
        }
        Ok(())
    }

    /// `takes_typing`, for what another agent hands it (`delegate`, `reply`): only once a snapshot
    /// found its agent. One Templum typed there may never have started (its folder's trust
    /// declined), and before a snapshot found it, macOS can't tell its shell is in front.
    fn takes_handing(&self, uid: &str) -> Result<(), Reply> {
        if self.seen.agent.is_some() && self.seen.pid.is_none() {
            return Err(Reply::err(format!(
                "{uid} is still starting: try again in a few seconds."
            )));
        }
        self.takes_typing(uid)
    }
}

impl Workspace {
    /// The block `id` of `space` as an agent reads who wrote to it: `Claude Code (b7k2m)`, its
    /// title as `about` gives it.
    pub(super) fn signature(&self, space: &Space, id: BlockId, cx: &App) -> String {
        let (_, title, _) = self.about(&space.blocks[&id], cx);
        let uid = space.uids.get(&id).cloned().unwrap_or_default();
        format!("{title} ({uid})")
    }

    /// `send_keys` from the terminal `from`: `keys` pressed in the terminal `block`, also into
    /// an agent waiting for its person (the wire is the person's consent that the agents on it
    /// answer each other's dialogs), never into one on another agent's task while it works.
    pub(super) fn send_keys(
        &mut self,
        from: &str,
        block: &str,
        keys: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Outcome {
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return Outcome::Now(refused),
        };
        if to == me {
            return Outcome::Now(Reply::err(
                "This block is yours: send_keys presses keys in another block.",
            ));
        }
        let Some(pane) = space.terminal(to) else {
            return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
        };
        if let Some(bad) = keys.iter().find(|k| named_key(k).is_none()) {
            return Outcome::Now(Reply::err(format!(
                "{bad:?} isn't a key Templum knows. Keys: {KEY_NAMES}."
            )));
        }
        let asks = matches!(pane.state, Some(AgentState::NeedsYou(_)));
        if pane.task(Instant::now()).is_some_and(|h| h.by != from) && !asks {
            return Outcome::Now(Reply::err(format!(
                "{block} is on a task another agent handed it: only that agent presses keys in it \
                 while it works. wait_for_idle(\"{block}\") waits for its end."
            )));
        }
        let worker = pane.term.id().to_string();
        let by = self.signature(space, me, cx);
        let said = match keys.len() {
            1 => format!("Pressed 1 key in {block}."),
            n => format!("Pressed {n} keys in {block}."),
        };
        Outcome::Later(self.press_keys(worker, by, keys, said, cx))
    }

    /// `interrupt` from the terminal `from`: Esc once in the agent `block` while it works or waits
    /// for its person. The task the caller handed it ends first, its waits hearing so at once,
    /// with the prompt a spawn still keeps for it: typed later, it would start work no one awaits.
    pub(super) fn interrupt(&mut self, from: &str, block: &str, cx: &mut Context<Self>) -> Outcome {
        let now = Instant::now();
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return Outcome::Now(refused),
        };
        if to == me {
            return Outcome::Now(Reply::err(
                "This block is yours: interrupt stops another agent.",
            ));
        }
        let Some(pane) = space.terminal(to) else {
            return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
        };
        if pane.seen.agent.is_none() || pane.shell_in_front() {
            return Outcome::Now(Reply::err(format!(
                "{block} runs no agent: send_keys presses keys in a terminal."
            )));
        }
        // Past its timeout too, as `wait_for_idle` waits for it.
        let mine = pane.handed.as_ref().is_some_and(|h| h.by == from);
        if !mine && pane.task(now).is_some() {
            return Outcome::Now(Reply::err(format!(
                "{block} is on a task another agent handed it: only that agent interrupts it."
            )));
        }
        if !mine && !pane.busy(now) {
            return Outcome::Now(Reply::err(format!(
                "{block} is idle: there's nothing to interrupt."
            )));
        }
        let worker = pane.term.id().to_string();
        let by = self.signature(space, me, cx);
        let said = if mine {
            self.end_turn(&worker, Reply::err("Interrupted by you."), cx);
            if let Some(pane) = self.pane_mut(&worker) {
                pane.prompt = None;
            }
            format!("Pressed Esc in {block}, and ended the task you handed it.")
        } else {
            format!("Pressed Esc in {block}.")
        };
        Outcome::Later(self.press_keys(worker, by, vec!["escape".into()], said, cx))
    }

    /// Presses `keys` in the terminal `worker`, KEY_GAP apart, as keys `by` pressed (the person's,
    /// for the screen's looks); then `said` and its screen, KEYS_SETTLE after the last. The
    /// terminal is found again before each key: one closed meanwhile ends the call there.
    pub(super) fn press_keys(
        &self,
        worker: String,
        by: String,
        keys: Vec<String>,
        said: String,
        cx: &mut Context<Self>,
    ) -> Task<Reply> {
        cx.spawn(async move |ws, cx| {
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    cx.background_executor().timer(KEY_GAP).await;
                }
                let pressed = ws.update(cx, |ws, cx| {
                    let (Some(pane), Some(key)) = (ws.pane_mut(&worker), named_key(key)) else {
                        return false;
                    };
                    // The person's keys, for the screen's looks: a dialog they answer ends.
                    pane.typed.set(true);
                    pane.keys_from = Some((by.clone(), Instant::now()));
                    pane.term.press(&key);
                    if i + 1 == keys.len() {
                        // "answered by" goes with no other frame to take it.
                        cx.spawn(async move |ws, cx| {
                            cx.background_executor().timer(ANSWERED_FOR).await;
                            let _ = ws.update(cx, |_, cx| cx.notify());
                        })
                        .detach();
                    }
                    cx.notify();
                    true
                });
                if !pressed.unwrap_or(false) {
                    return Reply::err("Its block was closed before every key was pressed.");
                }
            }
            cx.background_executor().timer(KEYS_SETTLE).await;
            let lines = ws
                .update(cx, |ws, _| ws.pane(&worker).map(Pane::tail))
                .ok()
                .flatten()
                .unwrap_or_default();
            Reply::ok(format!(
                "{said} Its screen now:\n{}",
                tools::tail(&lines, tools::MAX_TEXT)
            ))
        })
    }

    /// Types `text` into the agent in terminal `terminal`, its first line and the rest as two
    /// pastes, then Enter a moment later,
    /// once its screen shows the paste (`EnterWait`), if Templum still may type into it. A screen
    /// that doesn't (a dialog that ignores pastes, as Codex's approval does, which Templum hears
    /// nothing of), or an agent that started asking its person meanwhile (a working agent's
    /// screen changes all along), gets no Enter: it would pick the dialog's answer for the
    /// person. A `task` (not a reply) then ends at once, untaken. An agent whose hooks speak
    /// confirms its Enter (`confirm`).
    pub(super) fn send_task(&self, terminal: &str, text: &str, task: bool, cx: &mut Context<Self>) {
        let Some(pane) = self.pane(terminal) else {
            return;
        };
        // At work, it queues the task: Codex says it took it only once a turn takes it in.
        let queued = pane.state == Some(AgentState::Working);
        let mut wait = EnterWait::new(pane.screen(), pane.term.grid_size());
        // Claude Code shows a paste of 4 lines or more than 800 characters as "[Pasted text]",
        // which it takes for data unless typed text comes with it: the first line is that text.
        match text.find('\n') {
            Some(at) => {
                pane.term.type_in(&text[..at]);
                pane.term.type_in(&text[at..]);
            }
            None => pane.term.type_in(text),
        }
        let terminal = terminal.to_string();
        cx.spawn(async move |ws, cx| {
            // Some(the prompts it had taken at its Enter, when its hooks speak) once done looking.
            let entered = loop {
                cx.background_executor().timer(ENTER_AFTER).await;
                let step = ws.update(cx, |ws, cx| {
                    let Some(pane) = ws.pane(&terminal) else {
                        return Some(None);
                    };
                    let takes = pane.takes_typing("").is_ok();
                    let size = pane.term.grid_size();
                    match wait.look(pane.screen(), size, takes, pane.speaks) {
                        Step::Wait => None,
                        Step::Enter => {
                            pane.term.write(&b"\r"[..]);
                            Some(pane.speaks.then_some(pane.submitted))
                        }
                        Step::GiveUp => {
                            if task {
                                ws.not_taken(&terminal, cx);
                            }
                            Some(None)
                        }
                    }
                });
                match step {
                    Ok(None) => {}
                    Ok(Some(entered)) => break entered,
                    Err(_) => return,
                }
            };
            let Some(submitted) = entered else {
                return;
            };
            // Its screen just after the Enter: a second one goes only into the same.
            cx.background_executor().timer(ENTER_AFTER).await;
            let Ok(Some(screen)) = ws.update(cx, |ws, _| ws.pane(&terminal).map(Pane::screen))
            else {
                return;
            };
            cx.background_executor().timer(CONFIRM - ENTER_AFTER).await;
            let again = Some(screen.as_str());
            let done = ws
                .update(cx, |ws, cx| {
                    ws.confirm(&terminal, submitted, queued, again, task, cx)
                })
                .unwrap_or(true);
            if !done {
                cx.background_executor().timer(CONFIRM).await;
                let _ = ws.update(cx, |ws, cx| {
                    ws.confirm(&terminal, submitted, queued, None, task, cx)
                });
            }
        })
        .detach();
    }

    /// A look, CONFIRM after its Enter, at the agent in `terminal` that was typed a task or a reply
    /// (docs/specs/connections.md, Delegating 2, **Confirmed**; `submitted`: the prompts it had
    /// taken then). One more prompt taken confirms it, and so does a dialog it opened; an agent at
    /// work when it was typed (`queued`) took it into its queue. Nothing else does: not a turn's
    /// end, nor a moving screen. Otherwise it gets its Enter once more (the first may have joined
    /// the paste as a new line), only while its screen is still `again` (a dialog no hook tells of
    /// would have drawn) and the person hasn't typed into it for TYPING; else a `task` ends
    /// untaken. True when done.
    fn confirm(
        &mut self,
        terminal: &str,
        submitted: u64,
        queued: bool,
        again: Option<&str>,
        task: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.pane(terminal) else {
            return true;
        };
        if pane.submitted != submitted || queued || pane.takes_typing("").is_err() {
            return true;
        }
        let typing = pane.keyed.get().is_some_and(|at| at.elapsed() < TYPING);
        if !typing && again.is_some_and(|screen| screen == pane.screen()) {
            pane.term.write(&b"\r"[..]);
            return false;
        }
        if task {
            self.not_taken(terminal, cx);
        }
        true
    }

    /// The block id of the terminal `terminal`, as agents name it; empty when it is gone.
    pub(super) fn uid_of(&self, terminal: &str) -> String {
        self.caller(terminal)
            .and_then(|(space, id)| self.space_named(&space)?.uids.get(&id).cloned())
            .unwrap_or_default()
    }

    /// The agent in the terminal `terminal` didn't take the task typed into it: the task, if it is
    /// still handed over, ends, and the calls waiting on it hear so, with its screen.
    fn not_taken(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let Some(tail) = self
            .pane(terminal)
            .filter(|p| p.handed.is_some())
            .map(Pane::tail)
        else {
            return;
        };
        let uid = self.uid_of(terminal);
        let why = format!(
            "{uid} didn't take the task: it may be showing a dialog. Look at its block, then try \
             again. The last lines of its screen:\n{}",
            tools::tail(&tail, tools::MAX_TEXT)
        );
        self.end_turn(terminal, Reply::err(why), cx);
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
        if let Err(refused) = pane.takes_handing(block) {
            return Outcome::Now(refused);
        }
        if pane.task(now).is_some() {
            return Outcome::Now(Reply::err(format!(
                "{block} is already on a task another agent handed it: wait_for_idle(\"{block}\") \
                 waits for its end."
            )));
        }
        let worker = pane.term.id().to_string();
        let from_line = format!(
            "[from {}] {TASK_LEAD}\n{task}",
            self.signature(space, me, cx)
        );
        self.send_task(&worker, &from_line, true, cx);
        if let Some(pane) = self.pane_mut(&worker) {
            pane.handed = Some(Handed::new(from, Duration::from_secs(timeout)));
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
            if let Err(refused) = pane.takes_handing(block) {
                return refused;
            }
            let reply = format!(
                "[reply from {}] Its reply is in the pasted text.\n{text}",
                self.signature(space, me, cx)
            );
            self.send_task(&asker, &reply, false, cx);
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
    /// is idle, else once its turn, or the task it is on, ends. A task the caller handed it is
    /// waited for past its deadline while unanswered: Codex never says it works.
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
        let mine = pane.handed.as_ref().is_some_and(|h| h.by == from);
        if mine || pane.busy(Instant::now()) {
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

    /// The agent in the terminal `terminal` is gone: its session ended, a snapshot no longer
    /// finds its process (it crashed, or Codex quit, which says nothing), or one spawned with a
    /// prompt no snapshot found within `STARTS_WITHIN` never started.
    pub(super) fn agent_left(&mut self, terminal: &str, cx: &mut Context<Self>) {
        self.end_turn(
            terminal,
            Reply::err("Its agent ended before it answered."),
            cx,
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (u16, u16) = (80, 24);
    const BIG: (u16, u16) = (100, 30);

    fn wait() -> EnterWait {
        EnterWait::new("› ".into(), SIZE)
    }

    #[test]
    fn enter_goes_once_the_screen_shows_the_paste() {
        let mut w = wait();
        assert_eq!(w.look("› ".into(), SIZE, true, false), Step::Wait);
        assert_eq!(
            w.look("› fix the tests".into(), SIZE, true, false),
            Step::Enter
        );
    }

    #[test]
    fn no_enter_for_a_screen_that_never_shows_the_paste_nor_into_a_dialog() {
        let mut w = wait();
        for _ in 1..ENTER_LOOKS {
            assert_eq!(w.look("› ".into(), SIZE, true, false), Step::Wait);
        }
        assert_eq!(w.look("› ".into(), SIZE, true, false), Step::GiveUp);
        assert_eq!(
            wait().look("› fix".into(), SIZE, false, true),
            Step::GiveUp,
            "a dialog opened: Templum may not type into it"
        );
    }

    /// The person resizes the agent's block: it redraws at its new size, which is no sign that it
    /// read the paste. Its Enter waits until its screen settles.
    #[test]
    fn a_resize_isnt_the_paste() {
        let mut w = wait();
        assert_eq!(w.look("redrawn".into(), BIG, true, true), Step::Wait);
        assert_eq!(w.look("redrawn › fix".into(), BIG, true, true), Step::Wait);
        assert_eq!(w.look("redrawn › fix".into(), BIG, true, true), Step::Wait);
        assert_eq!(
            w.look("redrawn › fix".into(), BIG, true, true),
            Step::Enter,
            "settled: an agent whose dialogs are heard gets its Enter, and confirms it"
        );
        // One whose dialogs Templum doesn't hear (Codex's approval): the settled screen is the
        // new start, and only a change after it is the paste.
        let mut w = wait();
        for _ in 0..3 {
            assert_eq!(w.look("redrawn".into(), BIG, true, false), Step::Wait);
        }
        assert_eq!(w.look("redrawn".into(), BIG, true, false), Step::Wait);
        assert_eq!(
            w.look("redrawn › fix".into(), BIG, true, false),
            Step::Enter
        );
        let mut w = wait();
        for _ in 0..3 {
            w.look("redrawn".into(), BIG, true, false);
        }
        for _ in 1..ENTER_LOOKS {
            assert_eq!(w.look("redrawn".into(), BIG, true, false), Step::Wait);
        }
        assert_eq!(
            w.look("redrawn".into(), BIG, true, false),
            Step::GiveUp,
            "the paste may have been drawn with the redraw: no Enter it can't tell is safe"
        );
    }

    #[test]
    fn a_screen_that_keeps_moving_after_a_resize() {
        let moving = |w: &mut EnterWait, speaks: bool| {
            let mut step = Step::Wait;
            for n in 0..=ENTER_LOOKS {
                step = w.look(format!("frame {n}"), BIG, true, speaks);
                if step != Step::Wait {
                    break;
                }
            }
            step
        };
        assert_eq!(
            moving(&mut wait(), true),
            Step::Enter,
            "alive, and its Enter is confirmed"
        );
        assert_eq!(moving(&mut wait(), false), Step::GiveUp);
    }
}
