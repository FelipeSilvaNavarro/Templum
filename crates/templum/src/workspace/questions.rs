//! An agent's question in its dialog (docs/specs/ask-user.md): its fields, what the person picks,
//! Send and Dismiss.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, Window};
use templum_mcp::Reply;
use templum_mcp::ask::{Ask, Form};

use super::Workspace;
use super::waits::{Field, Wait, Waiting, field_focused};

/// A dismissed question's answer.
const DISMISSED: &str = "The person dismissed the question without answering.";

/// The question `key`'s fields: an Other field per question (a question without options: its
/// field) and, asked with `questions`, the note. Typing in an Other field picks it; Enter in any
/// sends.
pub(super) fn fields(
    key: u64,
    ask: &Ask,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> (Vec<Field>, Option<Field>) {
    let others = ask
        .questions
        .iter()
        .enumerate()
        .map(|(q, question)| {
            let placeholder = question.options.is_empty().then_some("Your answer");
            field(key, Some(q), placeholder, window, cx)
        })
        .collect();
    let note = ask
        .json
        .then(|| field(key, None, Some("Note (optional)"), window, cx));
    (others, note)
}

/// A field of the question `key`: question `q`'s Other field, or with None the note.
fn field(
    key: u64,
    q: Option<usize>,
    placeholder: Option<&'static str>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Field {
    let input = cx.new(|cx| {
        let input = InputState::new(window, cx);
        match placeholder {
            Some(text) => input.placeholder(text),
            None => input,
        }
    });
    let events = cx.subscribe_in(
        &input,
        window,
        move |ws, input, event: &InputEvent, window, cx| match event {
            InputEvent::Change => {
                if let Some(q) = q {
                    let text = input.read(cx).value().to_string();
                    ws.other_typed(key, q, &text, cx);
                }
            }
            InputEvent::PressEnter { .. } => ws.send_question(key, window, cx),
            InputEvent::Focus | InputEvent::Blur => {}
        },
    );
    (input, events)
}

impl Workspace {
    /// The question `key`, and what the person picked in it.
    fn form_of(&mut self, key: u64) -> Option<(&Ask, &mut Form)> {
        self.waits
            .iter_mut()
            .find(|w| w.key == key)
            .and_then(|w| match &mut w.what {
                Waiting::Question { ask, form, .. } => Some((&*ask, form)),
                _ => None,
            })
    }

    /// Question `q`'s Other field of the question `key` holds `text` now.
    fn other_typed(&mut self, key: u64, q: usize, text: &str, cx: &mut Context<Self>) {
        if let Some((ask, form)) = self.form_of(key) {
            form.typed(ask, q, text);
            cx.notify();
        }
    }

    /// A click on option `i` of question `q` of the question `key`: picked, or checked; one
    /// single-select question answers with it at once.
    pub(crate) fn pick_option(
        &mut self,
        key: u64,
        q: usize,
        i: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((ask, form)) = self.form_of(key) else {
            return;
        };
        form.pick(ask, q, i);
        let quick = ask.quick();
        cx.notify();
        if quick {
            self.send_question(key, window, cx);
        }
    }

    /// Send, or Enter in a field: the question `key` is answered once each of its questions has
    /// an answer.
    pub(crate) fn send_question(&mut self, key: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(wait) = self.waits.iter().find(|w| w.key == key) else {
            return;
        };
        let (Some((others, note)), Waiting::Question { ask, form, .. }) =
            (self.texts(wait, cx), &wait.what)
        else {
            return;
        };
        if let Some(answers) = form.answers(ask, &others) {
            let text = ask.answer(&answers, &note);
            self.close_question(key, Reply::ok(text), window, cx);
        }
    }

    /// Dismiss: the question `key` is refused.
    pub(crate) fn dismiss_question(
        &mut self,
        key: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_question(key, Reply::err(DISMISSED), window, cx);
    }

    /// Answers the question `key` with `reply`; the keyboard goes back to the canvas when one of
    /// its fields had it.
    fn close_question(
        &mut self,
        key: u64,
        reply: Reply,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typing = self
            .waits
            .iter()
            .any(|w| w.key == key && field_focused(w, window, cx));
        if self.answer(key, reply, cx) && typing {
            window.focus(&self.focus_handle, cx);
        }
    }

    /// A question's Other texts and its note ("" without one).
    pub(crate) fn texts(&self, wait: &Wait, cx: &App) -> Option<(Vec<String>, String)> {
        let Waiting::Question { others, note, .. } = &wait.what else {
            return None;
        };
        let text = |(input, _): &Field| input.read(cx).value().to_string();
        Some((
            others.iter().map(text).collect(),
            note.as_ref().map(text).unwrap_or_default(),
        ))
    }
}
