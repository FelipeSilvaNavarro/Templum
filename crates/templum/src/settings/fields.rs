//! The dropdown and number settings: their options or limits, and what each reads from and
//! writes to the config.

use gpui_kit::component::input::{InputEvent, InputState, NumberInputEvent, StepAction};
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, Entity, Subscription, Window};

use crate::config::{Config, Edit, Mode, Shape};
use crate::workspace::Workspace;

/// A dropdown setting: its options (value, label), the option the config is at, and what
/// choosing one does to the config and writes to the file.
pub(super) struct Choice {
    pub(super) table: &'static str,
    pub(super) key: &'static str,
    pub(super) options: &'static [(&'static str, &'static str)],
    pub(super) get: fn(&Config) -> &'static str,
    pub(super) set: fn(&mut Config, &str),
    pub(super) toml: fn(&str) -> toml_edit::Value,
}

pub(super) static MODE: Choice = Choice {
    table: "theme",
    key: "mode",
    options: &[("system", "System"), ("light", "Light"), ("dark", "Dark")],
    get: |c| match c.theme.mode {
        Mode::System => "system",
        Mode::Light => "light",
        Mode::Dark => "dark",
    },
    set: |c, v| {
        c.theme.mode = match v {
            "light" => Mode::Light,
            "dark" => Mode::Dark,
            _ => Mode::System,
        }
    },
    toml: |v| v.into(),
};

pub(super) static CURSOR: Choice = Choice {
    table: "cursor",
    key: "shape",
    options: &[
        ("block", "Block"),
        ("bar", "Bar"),
        ("underline", "Underline"),
    ],
    get: |c| match c.cursor.shape {
        Shape::Block => "block",
        Shape::Bar => "bar",
        Shape::Underline => "underline",
    },
    set: |c, v| {
        c.cursor.shape = match v {
            "bar" => Shape::Bar,
            "underline" => Shape::Underline,
            _ => Shape::Block,
        }
    },
    toml: |v| v.into(),
};

/// A whole-number setting and its limits.
pub(crate) struct Number {
    pub(super) table: &'static str,
    pub(super) key: &'static str,
    pub(super) min: f64,
    pub(super) max: f64,
    pub(super) step: f64,
    pub(super) get: fn(&Config) -> f64,
    pub(super) set: fn(&mut Config, f64),
}

pub(crate) static SCROLLBACK: Number = Number {
    table: "terminal",
    key: "scrollback",
    min: 0.0,
    max: 100_000.0,
    step: 1000.0,
    get: |c| c.terminal.scrollback as f64,
    set: |c, v| c.terminal.scrollback = v as usize,
};

/// A number field's editor. The value applies on Enter, when the field loses the focus, or on a
/// step (+/−, ↑/↓), never while typing: scrollback passing through "2" on the way to "20000"
/// would trim every terminal's history.
pub(crate) struct NumberState {
    pub input: Entity<InputState>,
    /// The config's value last shown: a different one (a hand edit, a reset) replaces the text.
    pub(super) shown: f64,
    _events: [Subscription; 2],
}

impl NumberState {
    pub fn new(
        ws: Entity<Workspace>,
        n: &'static Number,
        value: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // No step, min or max on the input: it then leaves stepping to us (NumberInputEvent).
        let input = cx.new(|cx| InputState::new(window, cx).default_value(value.to_string()));
        let typed = {
            let ws = ws.clone();
            cx.subscribe_in(
                &input,
                window,
                move |_, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur)
                        && let Ok(v) = input.read(cx).value().trim().parse::<f64>()
                    {
                        set_number(&ws, n, v, cx);
                    }
                },
            )
        };
        let stepped = cx.subscribe_in(
            &input,
            window,
            move |_, input, event: &NumberInputEvent, window, cx| {
                let NumberInputEvent::Step(action) = event;
                let current = input
                    .read(cx)
                    .value()
                    .trim()
                    .parse::<f64>()
                    .unwrap_or_else(|_| (n.get)(&ws.read(cx).config));
                let v = match action {
                    StepAction::Increment => current + n.step,
                    StepAction::Decrement => current - n.step,
                }
                .clamp(n.min, n.max);
                input.update(cx, |input, cx| input.set_value(v.to_string(), window, cx));
                set_number(&ws, n, v, cx);
            },
        );
        Self {
            input,
            shown: value,
            _events: [typed, stepped],
        }
    }
}

/// Typed values may have decimals or go past the limits; the file gets whole numbers within them.
fn set_number(ws: &Entity<Workspace>, n: &'static Number, v: f64, cx: &mut App) {
    let v = v.clamp(n.min, n.max).round();
    ws.update(cx, |ws, cx| {
        ws.edit(Edit::set(n.table, n.key, v as i64), |c| (n.set)(c, v), cx)
    });
}

pub(super) static FONT_SIZE: Number = Number {
    table: "font",
    key: "size",
    min: 6.0,
    max: 72.0,
    step: 1.0,
    get: |c| f64::from(c.font.size),
    set: |c, v| c.font.size = v as f32,
};
