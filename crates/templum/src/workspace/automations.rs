//! Automations at run time (docs/specs/automations.md): a look every AUTOMATION_LOOK finds the
//! rules whose time came, holds a prompt or a command until its target is free, and runs each Do
//! through the operations that already exist. Times are the computer's clock (`SystemTime`,
//! `chrono::Local`): GPUI's timers don't count time asleep, so a time missed while it slept goes
//! at the first look after, once. The rules and their file are `crate::automations`.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, NaiveDateTime, NaiveTime};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::{App, AppContext as _, Context, KeyDownEvent, Task, Window};
use templum_canvas::BlockId;

use std::path::PathBuf;

use templum_mcp::tools;

use super::agents_view::Doing;
use super::bus::Outcome;
use super::delegate::{NOTICE_GAP, TYPING};
use super::ops::{NewBlock, Place};
use super::spawn::Opener;
use super::{Pane, Workspace, show_notification};
use crate::automations::clock::{next_after, time_words, when_problem, when_words};
use crate::automations::{Automation, Do, LastRun, NAME_MAX, Went, When};
use crate::keys::Command;
use crate::launcher;

/// How often the rules that are on are looked at.
pub(crate) const AUTOMATION_LOOK: Duration = Duration::from_secs(1);
/// How long a prompt or a command waits for its target to be free.
pub(crate) const HOLD_MOST: Duration = Duration::from_secs(60 * 60);
/// The most characters of a quoted text in a sentence.
const QUOTE_CHARS: usize = 40;

/// The asker of an automation's tasks: a word no terminal has.
pub(crate) fn asker(id: u64) -> String {
    format!("automation-{id}")
}

/// What an automation is doing, in memory only.
#[derive(Default)]
pub(crate) struct Run {
    /// None: off, or never again.
    pub next: Option<SystemTime>,
    /// Due, waiting for its target.
    pub held: Option<Held>,
    /// A try's answer is on its way (`Tried::Later`): no new try meanwhile.
    pub typing: bool,
    /// The run being typed counts toward Stop after; a time that falls due meanwhile joins it.
    pub typing_counts: bool,
    /// Its last run went other than Ran since the popup was opened.
    pub unseen: bool,
}

pub(crate) struct Held {
    pub since: SystemTime,
    pub why: String,
    /// It is a run that counts toward Stop after (not a Run now).
    pub counts: bool,
}

/// One try of a Do.
pub(super) enum Tried {
    Went(Went, String),
    /// Its answer, Ok(what happened) or Err(why not).
    Later(Task<Result<String, String>>),
    /// Not now: why.
    Hold(String),
}

pub(crate) fn unix(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

pub(crate) fn local(secs: i64) -> DateTime<Local> {
    DateTime::from_timestamp(secs, 0)
        .unwrap_or_default()
        .with_timezone(&Local)
}

/// The first time `a` falls due after `now`; None: never.
fn next_for(a: &Automation, now: SystemTime) -> Option<SystemTime> {
    next_after(&a.when, &local(a.since), &DateTime::<Local>::from(now)).map(SystemTime::from)
}

/// A folder as the rule has it ("~/api"), else the path it came to.
fn shown_folder(typed: Option<&str>, path: &std::path::Path) -> String {
    let typed = typed.map(str::trim).filter(|typed| !typed.is_empty());
    typed.map_or_else(|| path.display().to_string(), str::to_string)
}

/// The terminal a command opened still serves `new`: a command again, in the same terminal or
/// folder.
fn keeps_terminal(old: &Do, new: &Do) -> bool {
    match (old, new) {
        (
            Do::Command {
                terminal: t,
                folder: f,
                ..
            },
            Do::Command {
                terminal: t2,
                folder: f2,
                ..
            },
        ) => t == t2 && f == f2,
        _ => false,
    }
}

/// The first line of `text`, at most QUOTE_CHARS characters, in quotes.
fn quote(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.chars().count() > QUOTE_CHARS || text.trim().lines().count() > 1 {
        let cut: String = line.chars().take(QUOTE_CHARS).collect();
        format!("\"{}…\"", cut.trim_end())
    } else {
        format!("\"{line}\"")
    }
}

/// The uid a prompt or a command waits for: the block it is typed into.
fn typed_into(a: &Automation) -> Option<&str> {
    match &a.act {
        Do::Prompt { block, .. } => Some(block),
        Do::Command { terminal, .. } => terminal.as_deref(),
        _ => None,
    }
}

impl Workspace {
    /// At start: a time that passed while Templum was closed is Missed, not run; the next time of
    /// each rule that is on is the first after `now`.
    pub(super) fn start_automations(&mut self, now: SystemTime, cx: &mut Context<Self>) {
        let missed_text = "Templum was closed.";
        for a in &mut self.automations.automations {
            if !a.on {
                continue;
            }
            if a.since == 0 {
                a.since = unix(now);
            }
            let from = a.since.max(a.last.as_ref().map_or(0, |l| l.at));
            let mut unseen = false;
            let first = next_after(&a.when, &local(a.since), &local(from)).map(SystemTime::from);
            if let Some(missed) = first.filter(|first| *first <= now) {
                a.last = Some(LastRun {
                    at: unix(missed),
                    went: Went::Missed,
                    about: missed_text.into(),
                });
                unseen = true;
                // A Once that was missed is as done as one that ran.
                if matches!(a.when, When::Once { .. }) {
                    a.on = false;
                }
            }
            let next = a.on.then(|| next_for(a, now)).flatten();
            self.automation_runs.insert(
                a.id,
                Run {
                    next,
                    unseen,
                    ..Run::default()
                },
            );
        }
        self.look_at_automations(cx);
    }

    /// Some rule is on, or has a run held: there is something to look at.
    fn automations_active(&self) -> bool {
        self.automations.automations.iter().any(|a| a.on)
            || self.automation_runs.values().any(|run| run.held.is_some())
    }

    /// Looks at the automations every AUTOMATION_LOOK while there is something to look at; started
    /// by the first rule turned on (or run by hand and held).
    pub(super) fn look_at_automations(&mut self, cx: &mut Context<Self>) {
        if self.automating || !self.automations_active() {
            return;
        }
        self.automating = true;
        let window = self.window_handle;
        cx.spawn(async move |ws, cx| {
            loop {
                cx.background_executor().timer(AUTOMATION_LOOK).await;
                let more = cx
                    .update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.look_automations(window, cx))
                    })
                    .ok()
                    .and_then(Result::ok);
                if more != Some(true) {
                    break;
                }
            }
        })
        .detach();
    }

    /// One look: the rules whose time came run (or are held), the held ones are tried again. A rule
    /// that is off is looked at only for a run held by hand. False once there is nothing to look at.
    pub(super) fn look_automations(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let now = SystemTime::now();
        let mut changed = false;
        let held = |ws: &Self, id| {
            ws.automation_runs
                .get(&id)
                .is_some_and(|run| run.held.is_some())
        };
        let looked: Vec<Automation> = (self.automations.automations.iter())
            .filter(|a| a.on || held(self, a.id))
            .cloned()
            .collect();
        for a in looked {
            let id = a.id;
            // Added since the start.
            let run = if a.on {
                self.automation_runs.entry(id).or_insert_with(|| {
                    changed = true;
                    Run {
                        next: next_for(&a, now),
                        ..Run::default()
                    }
                })
            } else {
                match self.automation_runs.get_mut(&id) {
                    Some(run) => run,
                    None => continue,
                }
            };
            let due = a.on && run.next.is_some_and(|next| next <= now);
            if due {
                // A sleep runs a missed time once: the next one is after now.
                run.next = next_for(&a, now);
                changed = true;
            }
            if run.typing {
                run.typing_counts |= due;
                continue;
            }
            let counts = match &mut run.held {
                Some(held) => {
                    held.counts |= due;
                    if now.duration_since(held.since).unwrap_or_default() >= HOLD_MOST {
                        let uid = typed_into(&a).unwrap_or_default();
                        let why = format!("{uid} stayed busy for an hour.");
                        let counts = held.counts;
                        self.run_ended(id, Went::Skipped, why, counts, cx);
                        continue;
                    }
                    held.counts
                }
                None if due => true,
                None => continue,
            };
            self.attempt(id, counts, window, cx);
        }
        if changed {
            cx.notify();
        }
        let more = self.automations_active();
        self.automating = more;
        more
    }

    /// Tries the Do of `id` once: it ends (Ran, Failed, Skipped), is held, or waits for an answer.
    /// `counts`: a due run, which counts toward Stop after; a Run now doesn't.
    fn attempt(&mut self, id: u64, counts: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.automations.get(id).cloned() else {
            return;
        };
        if let Err(why) = self.automation_target(&a) {
            self.run_ended(id, Went::Skipped, why, counts, cx);
            return;
        }
        match self.try_do(id, window, cx) {
            Tried::Went(went, about) => self.run_ended(id, went, about, counts, cx),
            Tried::Hold(why) => {
                let run = self.automation_runs.entry(id).or_default();
                match &mut run.held {
                    Some(held) if held.why == why => {}
                    Some(held) => {
                        held.why = why;
                        cx.notify();
                    }
                    None => {
                        let (since, counts) = (SystemTime::now(), counts);
                        run.held = Some(Held { since, why, counts });
                        cx.notify();
                    }
                }
            }
            Tried::Later(task) => {
                let run = self.automation_runs.entry(id).or_default();
                (run.typing, run.typing_counts, run.held) = (true, counts, None);
                let since = a.since;
                cx.spawn(async move |ws, cx| {
                    let answer = task.await;
                    ws.update(cx, |ws, cx| {
                        // A time that fell due while it was typed counted into it.
                        let counts = ws.automation_runs.get_mut(&id).map_or(counts, |run| {
                            run.typing = false;
                            run.typing_counts
                        });
                        let (went, about) = match answer {
                            Ok(about) => (Went::Ran, about),
                            Err(why) => (Went::Failed, why),
                        };
                        // Saved or turned off meanwhile: the new rule starts counting from its
                        // save; the answer is only the last thing that happened.
                        let same = ws
                            .automations
                            .get(id)
                            .is_some_and(|a| a.on && a.since == since);
                        if same {
                            ws.run_ended(id, went, about, counts, cx);
                        } else {
                            ws.run_ended(id, went, about, false, cx);
                            // A time of the new rule fell due while the old task was typed: it
                            // runs at the next look, and a Once then turns off.
                            if counts && ws.automations.get(id).is_some_and(|a| a.on) {
                                if let Some(run) = ws.automation_runs.get_mut(&id) {
                                    run.next = Some(SystemTime::now());
                                }
                                ws.look_at_automations(cx);
                            }
                        }
                    })
                    .ok();
                })
                .detach();
            }
        }
    }

    /// One try of the Do of `id`.
    fn try_do(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) -> Tried {
        let Some(a) = self.automations.get(id).cloned() else {
            return Tried::Went(Went::Skipped, "It was deleted.".into());
        };
        let a_id = a.id;
        match &a.act {
            Do::Notify { text } => {
                let title = match a.name.as_str() {
                    "" => self.automation_sentence(&a, cx),
                    name => name.to_string(),
                };
                let ws = cx.entity().downgrade();
                let notification = Notification::new()
                    .with_type(NotificationType::Info)
                    .title(title)
                    .message(text.clone())
                    .delivery(self.delivery(window))
                    .on_click(move |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.show_automation(a_id, window, cx))
                            .ok();
                    });
                show_notification(window, cx, notification);
                Tried::Went(Went::Ran, "Notified.".into())
            }
            Do::Prompt { block, text } => self.try_prompt(&a, block, text, cx),
            Do::Command {
                terminal: Some(uid),
                text,
                ..
            } => match self.find_block(uid) {
                Some(at) => match self.command_busy_at(&at) {
                    Some(why) => Tried::Hold(format!("Waiting for {uid}: {why}")),
                    None => self.type_command(&a, at, uid, text, cx),
                },
                None => Tried::Went(Went::Skipped, format!("Target missing: {uid} isn't open")),
            },
            Do::Command { folder, text, .. } => {
                self.command_in_new_terminal(&a, folder.as_deref(), text, window, cx)
            }
            Do::Agent {
                agent,
                folder,
                model,
                effort,
                prompt,
            } => self.start_agent_for(
                &a,
                agent,
                folder.as_deref(),
                (model, effort, prompt),
                window,
                cx,
            ),
        }
    }

    /// Why a command can't be typed into the terminal in `pane` now; None: its shell is at its
    /// prompt.
    fn command_busy(&self, pane: &Pane) -> Option<&'static str> {
        let processes = pane.term.processes();
        let in_front = processes
            .foreground
            .is_some_and(|front| Some(front) != processes.shell);
        let recent = |at: Option<Instant>, within| at.is_some_and(|at| at.elapsed() < within);
        if pane.seen.agent.is_some() && !pane.shell_in_front() {
            Some("an agent runs in it")
        } else if pane.term.status().running.is_some() || in_front {
            Some("a command runs in it")
        } else if recent(pane.keyed.get(), TYPING) || pane.has_draft() {
            Some("you are typing in it")
        } else {
            None
        }
    }

    /// `command_busy` of the terminal at `(workspace id, block)`; a block that isn't one is free,
    /// and fails when typed into.
    fn command_busy_at(&self, (space, id): &(String, BlockId)) -> Option<&'static str> {
        let pane = self.space_named(space)?.terminal(*id)?;
        self.command_busy(pane)
    }

    /// Types the command `text` and Enter into the terminal `uid` at `at`.
    fn type_command(
        &mut self,
        a: &Automation,
        (space, id): (String, BlockId),
        uid: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Tried {
        match self.type_text(&asker(a.id), (&space, id), uid, text, true, cx) {
            Outcome::Later(answer) => {
                let uid = uid.to_string();
                Tried::Later(cx.spawn(async move |_, _| {
                    let reply = answer.await;
                    match reply.is_error {
                        true => Err(reply.text),
                        false => Ok(format!("Typed into {uid}.")),
                    }
                }))
            }
            Outcome::Now(reply) if reply.is_error => Tried::Went(Went::Failed, reply.text),
            Outcome::Now(reply) => Tried::Went(Went::Ran, reply.text),
            Outcome::Wait(..) => Tried::Went(Went::Failed, "It waited for an answer.".into()),
        }
    }

    /// The folder a new block opens in: the rule's (`~` expanded), else its workspace's, else the
    /// home folder.
    fn automation_folder(&self, a: &Automation, typed: Option<&str>) -> PathBuf {
        if let Some(typed) = typed.map(str::trim).filter(|typed| !typed.is_empty()) {
            return launcher::expand(typed);
        }
        let space = a.workspace.as_deref().and_then(|id| self.space_named(id));
        space
            .and_then(|space| space.folder.clone())
            .or_else(std::env::home_dir)
            .unwrap_or_default()
    }

    /// The folder a new block of `a` would open in, as the Folder box's placeholder says it.
    pub(crate) fn automation_folder_shown(&self, a: &Automation) -> String {
        self.automation_folder(a, None).display().to_string()
    }

    /// Run a command in a new terminal, or the one the last run kept while it is at its prompt.
    fn command_in_new_terminal(
        &mut self,
        a: &Automation,
        folder: Option<&str>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Tried {
        let Some(space) = a.workspace.clone() else {
            return Tried::Went(Went::Failed, "Choose the workspace it opens in.".into());
        };
        let path = self.automation_folder(a, folder);
        let shown = shown_folder(folder, &path);
        if !path.is_dir() {
            return Tried::Went(Went::Failed, format!("No folder {shown}."));
        }
        // The terminal it kept, while it is open there and free.
        let kept = a.opened.as_deref().and_then(|uid| {
            let (home, id) = self.find_block(uid)?;
            let pane = self.space_named(&home)?.terminal(id)?;
            (home == space && pane.seen.agent.is_none()).then(|| (uid.to_string(), home, id))
        });
        if let Some((uid, home, id)) = kept
            && self.command_busy_at(&(home.clone(), id)).is_none()
        {
            return self.type_command(a, (home, id), &uid, text, cx);
        }
        let new = NewBlock::Terminal {
            folder: path,
            command: Some(text.to_string()),
        };
        match self.add_block_in(&space, new, Place::Free, None, window, cx) {
            Ok(id) => {
                let uid = self
                    .space_named(&space)
                    .and_then(|s| s.uids.get(&id).cloned())
                    .unwrap_or_default();
                if let Some(a) = self.automations.get_mut(a.id) {
                    a.opened = Some(uid.clone());
                }
                Tried::Went(Went::Ran, format!("Opened {uid} in {shown}."))
            }
            Err(why) => Tried::Went(Went::Failed, why),
        }
    }

    /// Start an agent: opened as `spawn_agent` opens one, in the rule's workspace, wired to
    /// nothing, with its first prompt a task from the automation.
    fn start_agent_for(
        &mut self,
        a: &Automation,
        agent: &str,
        folder: Option<&str>,
        (model, effort, prompt): (&Option<String>, &Option<String>, &Option<String>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Tried {
        let Some(entry) = self.agents.iter().find(|e| e.id == agent).cloned() else {
            let why = format!("Target missing: {agent} isn't in Settings ▸ Agents");
            return Tried::Went(Went::Skipped, why);
        };
        let Some(space) = a.workspace.clone() else {
            return Tried::Went(Went::Failed, "Choose the workspace it opens in.".into());
        };
        let (model, effort) = (model.as_deref(), effort.as_deref());
        if let Err(why) = entry.launch_args_with(model, effort, None) {
            return Tried::Went(Went::Failed, why);
        }
        if let Some(problem) = self.spawn_problem(&entry) {
            return Tried::Went(Went::Failed, problem);
        }
        let path = self.automation_folder(a, folder);
        if !path.is_dir() {
            let shown = shown_folder(folder, &path);
            return Tried::Went(Went::Failed, format!("No folder {shown}."));
        }
        let spawn = tools::Spawn {
            agent: Some(agent.to_string()),
            prompt: prompt.clone().filter(|p| !p.trim().is_empty()),
            folder: Some(path.display().to_string()),
            worktree: None,
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
            fork: false,
            role: None,
            args: None,
        };
        let called = match a.name.as_str() {
            "" => when_words(&a.when, Local::now().date_naive()),
            name => name.to_string(),
        };
        let opener = Opener::Automation {
            asker: asker(a.id),
            by: format!("automation \"{called}\""),
        };
        let (reply, _) = self.open_agent_in(
            &space,
            &entry,
            None,
            &path,
            &spawn,
            Place::Free,
            &[],
            opener,
            window,
            cx,
        );
        match reply.is_error {
            true => Tried::Went(Went::Failed, reply.text),
            // The folder as the rule has it: "in ~/api", not the path it expands to.
            false => {
                let in_path = format!(" in {}", path.display());
                let as_typed = format!(" in {}", shown_folder(folder, &path));
                Tried::Went(Went::Ran, reply.text.replacen(&in_path, &as_typed, 1))
            }
        }
    }

    /// Why the agent in `pane` can't take a prompt now; None: it can.
    fn prompt_busy(&self, pane: &Pane) -> Option<&'static str> {
        let doing = self.doing(pane);
        let recent = |at: Option<Instant>, within| at.is_some_and(|at| at.elapsed() < within);
        if pane.agent_starting() || pane.prompt.is_some() || doing == Doing::Starting {
            Some("it is starting")
        } else if matches!(doing, Doing::NeedsYou(_)) {
            Some("it needs you")
        } else if doing.works() {
            Some("it is working")
        } else if pane.task(Instant::now()).is_some() {
            Some("it is on a task")
        } else if pane.has_draft() || recent(pane.keyed.get(), TYPING) {
            Some("you are typing in it")
        } else if recent(pane.sent, NOTICE_GAP) {
            Some("Templum is typing into it")
        } else {
            None
        }
    }

    /// Send a prompt: handed to the agent in `block` as `delegate` hands a task, from the
    /// automation; held while the agent is busy; never typed into a shell.
    fn try_prompt(
        &mut self,
        a: &Automation,
        block: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Tried {
        let Some((space_id, id)) = self.find_block(block) else {
            return Tried::Went(Went::Skipped, format!("Target missing: {block} isn't open"));
        };
        let Some(pane) = self.space_named(&space_id).and_then(|s| s.terminal(id)) else {
            return Tried::Went(Went::Failed, format!("{block} isn't a terminal."));
        };
        if pane.seen.agent.is_none() || pane.shell_in_front() {
            let why = format!("{block} runs no agent now: a prompt is never typed into a shell.");
            return Tried::Went(Went::Failed, why);
        }
        if let Some(why) = self.prompt_busy(pane) {
            return Tried::Hold(format!("Waiting for {block}: {why}"));
        }
        let called = match a.name.as_str() {
            "" => when_words(&a.when, Local::now().date_naive()),
            name => name.to_string(),
        };
        let by = format!("automation \"{called}\"");
        let outcome = self.hand_task(
            &asker(a.id),
            &by,
            (&space_id, id),
            block,
            text,
            false,
            tools::TASK_WAIT,
            None,
            cx,
        );
        let task = self
            .space_named(&space_id)
            .and_then(|s| s.terminal(id))
            .and_then(|pane| pane.handed.as_ref())
            .map_or(0, |handed| handed.id);
        match outcome {
            Outcome::Later(answer) => {
                let block = block.to_string();
                Tried::Later(cx.spawn(async move |_, _| {
                    let reply = answer.await;
                    match reply.is_error {
                        true => Err(reply.text),
                        false => Ok(format!("Typed into {block} as task t{task}.")),
                    }
                }))
            }
            Outcome::Now(reply) if reply.is_error => Tried::Went(Went::Failed, reply.text),
            Outcome::Now(reply) => Tried::Went(Went::Ran, reply.text),
            Outcome::Wait(..) => Tried::Went(Went::Failed, "It waited for an answer.".into()),
        }
    }

    /// Its run ended: `last`, then for a due run (`counts`) its count, Stop after, and a Once
    /// turned off.
    fn run_ended(
        &mut self,
        id: u64,
        went: Went,
        about: String,
        counts: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(a) = self.automations.get_mut(id) else {
            return;
        };
        let run = self.automation_runs.entry(id).or_default();
        run.held = None;
        run.unseen |= went != Went::Ran;
        let mut about = about;
        if counts {
            if went == Went::Ran {
                a.runs += 1;
                if let Some(most) = a.max_runs.filter(|most| a.runs >= *most) {
                    a.on = false;
                    about.push_str(&format!(" Ran {most} of {most} times, now off."));
                }
            }
            if matches!(a.when, When::Once { .. }) {
                a.on = false;
            }
            if !a.on {
                run.next = None;
            }
        }
        a.last = Some(LastRun {
            at: unix(SystemTime::now()),
            went,
            about,
        });
        cx.notify();
    }

    /// Run now: its Do goes now, also when it is off, through the checks any run has; it doesn't
    /// move its next time and doesn't count toward Stop after.
    pub(crate) fn run_automation_now(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trying = self
            .automation_runs
            .get(&id)
            .is_some_and(|run| run.typing || run.held.is_some());
        if self.automations.get(id).is_some() && !trying {
            self.attempt(id, false, window, cx);
            // Held, it is tried again at each look.
            self.look_at_automations(cx);
        }
    }

    /// The switch of a row: turning it on counts from now; off drops its next time and its held
    /// run.
    pub(crate) fn set_automation_on(&mut self, id: u64, on: bool, cx: &mut Context<Self>) {
        let now = SystemTime::now();
        let Some(a) = self.automations.get_mut(id) else {
            return;
        };
        a.on = on;
        if on {
            (a.since, a.runs) = (unix(now), 0);
        }
        let next = on.then(|| next_for(a, now)).flatten();
        let run = self.automation_runs.entry(id).or_default();
        run.next = next;
        run.held = None;
        // A time that fell due while an older task was typed was the old rule's.
        run.typing_counts = false;
        self.look_at_automations(cx);
        cx.notify();
    }

    /// Why the Do of `a` can't act: its workspace or its target is gone or stopped. Err: "Target
    /// missing: abc12 isn't open".
    pub(crate) fn automation_target(&self, a: &Automation) -> Result<(), String> {
        let missing = |why: String| Err(format!("Target missing: {why}"));
        if let Some(id) = &a.workspace {
            match self.spaces().find(|s| s.id == *id) {
                None => return missing("a deleted workspace".into()),
                Some(space) if space.stopped => {
                    return missing(format!("{} is stopped", space.name));
                }
                Some(_) => {}
            }
        }
        match &a.act {
            Do::Prompt { block, .. } if self.find_block(block).is_none() => {
                missing(format!("{block} isn't open"))
            }
            Do::Command {
                terminal: Some(block),
                ..
            } if self.find_block(block).is_none() => missing(format!("{block} isn't open")),
            Do::Agent { agent, .. } if !self.agents.iter().any(|e| e.id == *agent) => {
                missing(format!("{agent} isn't in Settings ▸ Agents"))
            }
            _ => Ok(()),
        }
    }

    /// The preview sentence: "Every weekday at 09:00, type "review open PRs" into Claude Code
    /// (abc12)."
    pub(crate) fn automation_sentence(&self, a: &Automation, cx: &App) -> String {
        let block = |uid: &str| match self.uid_title(uid, cx) {
            Some(title) => format!("{title} ({uid})"),
            None => uid.to_string(),
        };
        let does = match &a.act {
            Do::Prompt { block: uid, text } => format!("type {} into {}", quote(text), block(uid)),
            Do::Command {
                terminal: Some(uid),
                text,
                ..
            } => format!("run {} in {}", quote(text), block(uid)),
            Do::Command { folder, text, .. } => {
                let folder = folder.as_deref().filter(|f| !f.is_empty());
                let folder = folder.map_or(String::new(), |f| format!(" in {f}"));
                format!("run {} in a new terminal{folder}", quote(text))
            }
            Do::Agent {
                agent,
                folder,
                prompt,
                ..
            } => {
                let folder = folder.as_deref().filter(|f| !f.is_empty());
                let folder = folder.map_or(String::new(), |f| format!(" in {f}"));
                let prompt = prompt.as_deref().filter(|p| !p.trim().is_empty());
                let prompt = prompt.map_or(String::new(), |p| format!(" with {}", quote(p)));
                format!("start {}{folder}{prompt}", self.agent_title(agent))
            }
            Do::Notify { text } => format!("notify you: {}", quote(text)),
        };
        let today = Local::now().date_naive();
        format!("{}, {does}.", when_words(&a.when, today))
    }

    /// How many automations ended other than Ran since the popup was opened.
    pub(crate) fn automations_unseen(&self) -> usize {
        self.automation_runs
            .values()
            .filter(|run| run.unseen)
            .count()
    }

    /// A row's right side, line 1: "17:00", "Tomorrow 09:00", "Mon 09:00", "3 Nov 09:00", "Off",
    /// "Never".
    pub(crate) fn next_words(&self, id: u64, now: SystemTime) -> String {
        let Some(a) = self.automations.get(id).filter(|a| a.on) else {
            return "Off".into();
        };
        match self.automation_runs.get(&a.id).and_then(|run| run.next) {
            Some(next) => {
                let (next, now) = (DateTime::<Local>::from(next), DateTime::<Local>::from(now));
                time_words(next.naive_local(), now.naive_local())
            }
            None => "Never".into(),
        }
    }
}

/// The runs of the automations, by id.
pub(crate) type Runs = HashMap<u64, Run>;

/// The draft key of a new automation, not saved yet.
pub(crate) const NEW: u64 = 0;

/// What the editor changes of an automation: a draft equal to the saved one in these is no draft.
fn edits(a: &Automation) -> (&str, &Option<String>, &When, &Do, Option<u32>) {
    (&a.name, &a.workspace, &a.when, &a.act, a.max_runs)
}

impl Workspace {
    /// The ids the list shows: the automations in their order, then the one being written.
    pub(crate) fn automation_ids(&self) -> Vec<u64> {
        let saved = self.automations.automations.iter().map(|a| a.id);
        let new = self.automation_drafts.contains_key(&NEW).then_some(NEW);
        saved.chain(new).collect()
    }

    /// The automation the editor shows: the one chosen if it is listed, else the first.
    pub(crate) fn automation_selected(&self) -> Option<u64> {
        let ids = self.automation_ids();
        self.automations_at
            .filter(|at| ids.contains(at))
            .or(ids.first().copied())
    }

    /// Opens the popup with the keyboard on the list; the other popups close and the badge clears.
    pub(crate) fn open_automations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.automations_open = true;
        self.git_open = false;
        self.inbox_open = false;
        self.agents_open = false;
        for run in self.automation_runs.values_mut() {
            run.unseen = false;
        }
        self.automations_focus.focus(window, cx);
        cx.notify();
    }

    /// Closes the popup; the keyboard goes back to the canvas only if the popup had it.
    pub(crate) fn close_automations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.automations_open = false;
        if self.automations_focus.contains_focused(window, cx) {
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }

    /// The sidebar's button, and the shortcut.
    pub(crate) fn toggle_automations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.automations_open {
            self.close_automations(window, cx);
        } else {
            self.open_automations(window, cx);
        }
    }

    /// Opens the popup on the automation `id`, if it is still there (a notification's click).
    pub(crate) fn show_automation(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.open_automations(window, cx);
        self.select_automation(id, window, cx);
    }

    /// Selects `id` and gives the popup the keyboard on its list.
    pub(crate) fn select_automation(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.automation_ids().contains(&id) {
            self.automations_at = Some(id);
        }
        self.automations_focus.focus(window, cx);
        cx.notify();
    }

    /// ↑ / ↓: the previous or next automation is selected (none past the ends).
    fn automations_step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.automation_ids();
        let Some(at) = self
            .automation_selected()
            .and_then(|id| ids.iter().position(|i| *i == id))
        else {
            return;
        };
        if let Some(next) = at.checked_add_signed(by).and_then(|i| ids.get(i)) {
            self.select_automation(*next, window, cx);
        }
    }

    /// The popup's keys, run before its fields get them: ↑ ↓ select while the list has the
    /// keyboard, Esc in a field gives it back to the list and on the list closes the popup; the
    /// workspace's shortcuts go on, but those that act on the terminal in front.
    pub(crate) fn on_automations_key(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let k = &e.keystroke;
        let command = super::effective(
            self.keymap.command(k),
            false,
            self.space.grid.is_some(),
            false,
        );
        // The theme picker takes no focus: while it is open its keys are its own.
        if self.picker.is_some() {
            self.command_key(e, command, window, cx);
            cx.stop_propagation();
            return;
        }
        let on_list = self.automations_focus.is_focused(window);
        let handled = !k.modifiers.modified()
            && match k.key.as_str() {
                "up" | "down" if on_list => {
                    self.automations_step(if k.key == "up" { -1 } else { 1 }, window, cx);
                    true
                }
                "escape" if on_list => {
                    self.close_automations(window, cx);
                    true
                }
                "escape" => {
                    self.automations_focus.focus(window, cx);
                    true
                }
                _ => false,
            };
        if handled {
            cx.stop_propagation();
            return;
        }
        let command = command.filter(|c| {
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

    /// Why the run of `id` is held, in full: "Waiting for abc12: it is working".
    pub(crate) fn automation_held_why(&self, id: u64) -> Option<String> {
        let run = self.automation_runs.get(&id)?;
        run.held.as_ref().map(|held| held.why.clone())
    }

    /// The editor's last line: while a run is held, why it waits and nothing else; else the last
    /// run in full: "Failed Yesterday 16:00: abc12 runs no agent now…".
    pub(crate) fn automation_last_line(&self, id: u64, now: SystemTime) -> Option<String> {
        if let Some(why) = self.automation_held_why(id) {
            return Some(why);
        }
        let words = self.last_words(id, now)?;
        let about = self.automations.get(id)?.last.as_ref()?.about.clone();
        Some(match about.is_empty() {
            true => words,
            false => format!("{words}: {about}"),
        })
    }

    /// Line 2's right side: "Ran 16:00", "Failed Yesterday 16:00", "Missed 24 Oct 17:00",
    /// "Waiting for abc12"; None before the first run.
    pub(crate) fn last_words(&self, id: u64, now: SystemTime) -> Option<String> {
        let a = self.automations.get(id)?;
        if self
            .automation_runs
            .get(&id)
            .is_some_and(|run| run.held.is_some())
        {
            return Some(match typed_into(a) {
                Some(uid) => format!("Waiting for {uid}"),
                None => "Waiting".into(),
            });
        }
        let last = a.last.as_ref()?;
        let went = match last.went {
            Went::Ran => "Ran",
            Went::Skipped => "Skipped",
            Went::Failed => "Failed",
            Went::Missed => "Missed",
        };
        let (at, now) = (
            local(last.at).naive_local(),
            DateTime::<Local>::from(now).naive_local(),
        );
        Some(format!("{went} {}", time_words(at, now)))
    }

    /// The schedules agents asked for, oldest first: (id, "s3 from Claude Code · api (b7k2m) to
    /// k9x1d", "every 10 min · next 15:10 · <first line>").
    pub(crate) fn schedule_rows(&self) -> Vec<(u64, String, String)> {
        let clock = |t: SystemTime| DateTime::<Local>::from(t).format("%H:%M").to_string();
        self.schedules
            .iter()
            .map(|s| {
                let from = match self.pane(&s.by) {
                    Some(pane) => {
                        let title = pane.term.title();
                        let uid = self.caller(&s.by).and_then(|(space, id)| {
                            self.space_named(&space)?.uids.get(&id).cloned()
                        });
                        let name = match self.agent_name(pane) {
                            Some(agent) if !title.is_empty() => format!("{agent} · {title}"),
                            Some(agent) => agent.to_string(),
                            None => title,
                        };
                        let uid = uid.unwrap_or_default();
                        let to = if uid == s.to { "itself" } else { s.to.as_str() };
                        format!("{name} ({uid}) to {to}")
                    }
                    None => format!("Templum Agent to {}", s.to),
                };
                let line = s.text.lines().next().unwrap_or_default();
                let when = match s.every {
                    Some(every) => format!(
                        "every {} min · next {}",
                        every.as_secs() / 60,
                        clock(s.next)
                    ),
                    None => format!("once · {}", clock(s.next)),
                };
                (
                    s.id,
                    format!("s{} from {from}", s.id),
                    format!("{when} · {line}"),
                )
            })
            .collect()
    }

    /// Duplicate: an off copy after the last, selected.
    pub(crate) fn duplicate_automation(&mut self, id: u64, cx: &mut Context<Self>) -> Option<u64> {
        let copy = self.automations.duplicate(id)?;
        self.automations_at = Some(copy);
        cx.notify();
        Some(copy)
    }

    /// Delete asks first.
    pub(crate) fn ask_delete_automation(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(a) = self.automation_draft(id).cloned() else {
            return;
        };
        let name = match a.name.as_str() {
            "" => self.automation_sentence(&a, cx),
            name => name.to_string(),
        };
        let entity = cx.entity();
        crate::prompt::confirm(
            format!("Delete \"{name}\"?"),
            "It won't run again. This can't be undone.",
            "Delete",
            move |_, cx| {
                entity.update(cx, |ws, cx| ws.delete_automation(id, cx));
            },
            window,
            cx,
        );
    }

    /// Removes the automation, its run and its draft.
    pub(crate) fn delete_automation(&mut self, id: u64, cx: &mut Context<Self>) {
        let ids = self.automation_ids();
        let at = ids.iter().position(|i| *i == id);
        self.automations.remove(id);
        self.automation_runs.remove(&id);
        self.automation_drafts.remove(&id);
        if self.automations_at == Some(id) {
            let left = self.automation_ids();
            self.automations_at =
                at.and_then(|at| left.get(at.min(left.len().saturating_sub(1))).copied());
        }
        cx.notify();
    }

    /// New: a draft at the list's end, selected.
    pub(crate) fn new_automation(&mut self, cx: &mut Context<Self>) {
        let nine = NaiveTime::from_hms_opt(9, 0, 0).unwrap_or_default();
        self.automation_drafts
            .entry(NEW)
            .or_insert_with(|| Automation {
                id: NEW,
                name: String::new(),
                on: true,
                workspace: Some(self.space.id.clone()),
                when: When::Daily { at: nine },
                act: Do::Notify {
                    text: String::new(),
                },
                max_runs: None,
                runs: 0,
                since: 0,
                last: None,
                opened: None,
            });
        self.automations_at = Some(NEW);
        cx.notify();
    }

    /// The draft of `id`, else the saved rule.
    pub(crate) fn automation_draft(&self, id: u64) -> Option<&Automation> {
        self.automation_drafts
            .get(&id)
            .or_else(|| self.automations.get(id))
    }

    /// Changes the draft of `id` (made from the saved rule if there is none); one equal to the
    /// saved rule again is no draft.
    pub(crate) fn edit_automation(
        &mut self,
        id: u64,
        edit: impl FnOnce(&mut Automation),
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self.automation_draft(id).cloned() else {
            return;
        };
        let mut next = current.clone();
        edit(&mut next);
        if edits(&next) == edits(&current) {
            return;
        }
        match self.automations.get(id) {
            Some(saved) if edits(saved) == edits(&next) => {
                self.automation_drafts.remove(&id);
            }
            _ => {
                self.automation_drafts.insert(id, next);
            }
        }
        cx.notify();
    }

    /// What stops `a` from being saved, in the spec's words; None: nothing.
    pub(crate) fn automation_problem(&self, a: &Automation, now: NaiveDateTime) -> Option<String> {
        if a.name.chars().count() > NAME_MAX {
            return Some(format!("A name is at most {NAME_MAX} characters."));
        }
        if let Some(problem) = when_problem(&a.when, now) {
            return Some(problem);
        }
        let blank = |text: &str| text.trim().is_empty();
        let own = match &a.act {
            Do::Prompt { block, .. } if blank(block) => Some("Choose an agent."),
            Do::Prompt { text, .. } if blank(text) => Some("Write the prompt."),
            Do::Command {
                terminal: Some(t), ..
            } if blank(t) => Some("Choose a terminal."),
            Do::Command { text, .. } if blank(text) => Some("Write the command."),
            Do::Agent { agent, .. } if blank(agent) => Some("Choose an agent."),
            Do::Notify { text } if blank(text) => Some("Write the text."),
            _ => None,
        };
        if let Some(problem) = own {
            return Some(problem.into());
        }
        if let Do::Agent {
            agent,
            model,
            effort,
            ..
        } = &a.act
            && let Some(entry) = self.agents.iter().find(|e| e.id == *agent)
            && let Err(why) = entry.launch_args_with(model.as_deref(), effort.as_deref(), None)
        {
            return Some(why);
        }
        let opens = matches!(a.act, Do::Command { terminal: None, .. } | Do::Agent { .. });
        (opens && a.workspace.is_none()).then(|| "Choose the workspace it opens in.".into())
    }

    /// Saves the draft of `id`: a new one is added on; any is counted from now (`since`, no runs,
    /// its held run dropped, `next` computed). Err: its problem, nothing saved.
    pub(crate) fn save_automation(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
    ) -> Result<u64, String> {
        let draft = self.automation_draft(id).cloned().ok_or("It is gone.")?;
        if let Some(problem) = self.automation_problem(&draft, Local::now().naive_local()) {
            return Err(problem);
        }
        let now = SystemTime::now();
        let saved = if id == NEW {
            let new = Automation {
                on: true,
                since: unix(now),
                ..draft
            };
            self.automations.add(new)
        } else {
            let live = self.automations.get_mut(id).ok_or("It is gone.")?;
            // Another kind, terminal or folder is not the terminal the old one kept; another
            // command in it is.
            if !keeps_terminal(&live.act, &draft.act) {
                live.opened = None;
            }
            (live.name, live.workspace, live.when, live.act) =
                (draft.name, draft.workspace, draft.when, draft.act);
            (live.max_runs, live.since, live.runs) = (draft.max_runs, unix(now), 0);
            id
        };
        self.automation_drafts.remove(&id);
        let a = self.automations.get(saved).cloned().ok_or("It is gone.")?;
        let run = self.automation_runs.entry(saved).or_default();
        run.held = None;
        run.typing_counts = false;
        run.next = a.on.then(|| next_for(&a, now)).flatten();
        self.automations_at = Some(saved);
        self.look_at_automations(cx);
        cx.notify();
        Ok(saved)
    }

    /// Revert: the draft goes (a new one is removed).
    pub(crate) fn revert_automation(&mut self, id: u64, cx: &mut Context<Self>) {
        self.automation_drafts.remove(&id);
        if id == NEW && self.automations_at == Some(NEW) {
            self.automations_at = None;
        }
        cx.notify();
    }
}
