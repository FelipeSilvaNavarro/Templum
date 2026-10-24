//! The requests inbox's model (docs/specs/ask-inbox.md): the requests of the workspace shown,
//! the one selected, and why each is asked.

use gpui_kit::{Context, Window};

use super::Workspace;
use super::waits::{Wait, Waiting};
use crate::board::Assignee;

/// The lines of a handed task the inbox keeps.
const TASK_LINES: usize = 3;

/// What is known of why an agent asks: each only when known.
#[derive(Debug, PartialEq)]
pub(crate) struct Why {
    /// The kanban card it works on: its id and title.
    pub card: Option<(u64, String)>,
    /// The first lines of the task it was handed.
    pub task: Option<String>,
    /// What the agent said with `why`.
    pub agent: Option<String>,
}

// Drawn by the inbox's view, not written yet.
#[cfg_attr(not(test), allow(dead_code))]
impl Workspace {
    /// The requests waiting for the person in the workspace shown (questions, plans and yes/no
    /// requests whose terminal is here), oldest first.
    pub(crate) fn inbox(&self) -> Vec<&Wait> {
        self.waits
            .iter()
            .filter(|w| matches!(w.what, Waiting::Question { .. } | Waiting::Allow { .. }))
            .filter(|w| {
                self.caller(&w.from)
                    .is_some_and(|(space, _)| space == self.space.id)
            })
            .collect()
    }

    /// The request the inbox has selected: the one `inbox_at` names if it is listed, else the
    /// first.
    pub(crate) fn inbox_selected(&self) -> Option<&Wait> {
        let inbox = self.inbox();
        let at = inbox.iter().find(|w| Some(w.key) == self.inbox_at);
        at.or(inbox.first()).copied()
    }

    /// Opens the inbox with the keyboard, on the request `at` when it is listed; the Git popup
    /// closes.
    pub(crate) fn open_inbox(
        &mut self,
        at: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inbox_open = true;
        self.git_open = false;
        if let Some(at) = at
            && self.inbox().iter().any(|w| w.key == at)
        {
            self.inbox_at = Some(at);
        }
        self.inbox_focus.focus(window, cx);
        cx.notify();
    }

    /// Closes the inbox; the keyboard goes back to the canvas only if the inbox had it.
    pub(crate) fn close_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inbox_open = false;
        if self.inbox_focus.contains_focused(window, cx) {
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }

    pub(crate) fn toggle_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inbox_open {
            self.close_inbox(window, cx);
        } else {
            self.open_inbox(None, window, cx);
        }
    }

    /// The requests `gone` (keys) ended: if the selected one is among them, the next is selected,
    /// or with none after it the first. Only a question or a yes/no request is selected, and
    /// those end only in the workspace shown. Called wherever requests end.
    pub(super) fn inbox_settle(&mut self, gone: &[u64]) {
        let Some(at) = self.inbox_at.filter(|at| gone.contains(at)) else {
            return;
        };
        self.inbox_at = self.inbox().iter().map(|w| w.key).find(|&key| key > at);
    }

    /// Why the agent that made `wait` asks, as far as Templum knows and the agent said.
    pub(crate) fn why(&self, wait: &Wait) -> Why {
        let uid = self.uid_of(&wait.from);
        // Its newest card not yet in its board's last column.
        let card = self
            .boards
            .boards
            .iter()
            .flat_map(|board| {
                let open = board.columns.len().saturating_sub(1);
                board.columns[..open].iter().flat_map(|c| &c.cards)
            })
            .filter(|card| card.assignee == Some(Assignee::Block(uid.clone())))
            .max_by_key(|card| card.id)
            .map(|card| (card.id, card.title.clone()));
        let task = self
            .pane(&wait.from)
            .and_then(|pane| pane.handed.as_ref())
            .map(|handed| {
                let lines: Vec<&str> = handed.text.lines().take(TASK_LINES).collect();
                lines.join("\n")
            })
            .filter(|task| !task.is_empty());
        let agent = match &wait.what {
            Waiting::Question { ask, .. } => ask.why.clone(),
            _ => None,
        };
        Why { card, task, agent }
    }
}
