//! Settings ▸ Git: the agent that writes commit messages (Generate ✨ beside a Git block's
//! summary), its model, its limits, the person's rules and the request, and Try it. See
//! docs/specs/git.md, section 7.

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{App, ClickEvent, Context, Entity, SharedString, TestSupportExt, Window, div, px};
use templum_git::message;

use super::fields::{MAX_DIFF, TIMEOUT};
use super::row::{Row, menu_button, remove, text_area, text_input};
use super::sections::number_row;
use crate::config::Edit;
use crate::grid::hsla;
use crate::workspace::{TryIt, Workspace};

/// The table these rows write.
const TABLE: &str = "git.commit_message";

pub(super) fn rows(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) -> Vec<Row> {
    let settings = &ws.config.git.commit_message;
    let mut rows = vec![
        agent_row(ws, entity),
        model_row(ws, entity),
        Row::new("git.commit_message.other_model", "Another model")
            .description("Typed: one the list doesn't have")
            .control(text_input(
                entity,
                "git.commit_message.other_model",
                typed_model(ws),
                set_typed_model,
                window,
                cx,
            )),
        number_row(ws, entity, &TIMEOUT, "Timeout", window, cx)
            .description("Seconds the agent has to answer"),
        number_row(ws, entity, &MAX_DIFF, "Max diff size", window, cx).description(
            "KB of the diff sent; past it the diff is cut, the list of files stays whole",
        ),
        Row::new("git.commit_message.instructions", "Instructions")
            .description(
                "Your commit rules, like Conventional Commits, English, imperative. A \
                 repository's own, from the ⚙ beside Generate, win there",
            )
            .below(text_area(
                entity,
                "git.commit_message.instructions",
                settings.instructions.clone(),
                2,
                set_commit_instructions,
                window,
                cx,
            ))
            .reset(
                !settings.instructions.is_empty(),
                remove(entity, TABLE, "instructions", |c| {
                    c.git.commit_message.instructions.clear()
                }),
            ),
        Row::new("git.commit_message.prompt", "Prompt (advanced)")
            .description(
                "What the agent is asked: {instructions}, {files}, {diff}, {recent_commits} and \
                 {truncated} are filled in. ↺ resets it to the default",
            )
            .below(text_area(
                entity,
                "git.commit_message.prompt",
                settings
                    .prompt
                    .clone()
                    .unwrap_or_else(|| message::PROMPT.to_string()),
                10,
                set_commit_prompt,
                window,
                cx,
            ))
            .reset(
                settings.prompt.is_some(),
                remove(entity, TABLE, "prompt", |c| {
                    c.git.commit_message.prompt = None
                }),
            ),
    ];
    if ws.commit_agent.is_some() {
        rows.push(try_row(ws, entity));
    }
    rows
}

/// The agents with a generate command; choosing one puts the model back to its recommended one.
fn agent_row(ws: &Workspace, entity: &Entity<Workspace>) -> Row {
    let settings = &ws.config.git.commit_message;
    let choices: Vec<(SharedString, SharedString)> = ws
        .agents
        .iter()
        .filter(|a| a.generate.is_some())
        .map(|a| (a.id.clone().into(), a.title.clone().into()))
        .collect();
    let current: SharedString = ws
        .commit_agent
        .as_ref()
        .map(|a| a.id.clone())
        .or_else(|| settings.agent.clone())
        .unwrap_or_else(|| "None".to_string())
        .into();
    let about = match (&ws.commit_agent, &settings.agent) {
        (Some(a), _) if a.model.is_empty() && a.command.contains("{model}") => {
            "No model: type one below".to_string()
        }
        (Some(a), _) if !a.ready => format!("Not installed: {}", a.program()),
        (Some(_), _) => "Generate ✨, beside a Git block's summary, asks it".to_string(),
        (None, Some(id)) => format!("{id} has no generate command: set one in Agents"),
        (None, None) => "No agent has a generate command: set one in Agents".to_string(),
    };
    let (e, r) = (entity.clone(), entity.clone());
    Row::new("git.commit_message.agent", "Agent")
        .description(about)
        .control(menu_button(
            "menu-git.commit_message.agent",
            choices,
            current,
            false,
            move |id, cx| e.update(cx, |ws, cx| ws.set_commit_agent(&id, cx)),
        ))
        .reset(settings.agent.is_some(), move |_, cx| {
            r.update(cx, |ws, cx| {
                ws.edit(
                    Edit::remove(TABLE, "agent"),
                    |c| c.git.commit_message.agent = None,
                    cx,
                );
                ws.edit(
                    Edit::remove(TABLE, "model"),
                    |c| c.git.commit_message.model = None,
                    cx,
                );
            })
        })
}

/// The agent's models, the recommended one (the first) marked.
fn model_row(ws: &Workspace, entity: &Entity<Workspace>) -> Row {
    let (models, model) = ws
        .commit_agent
        .as_ref()
        .map_or((Vec::new(), String::new()), |a| {
            (a.models.clone(), a.model.clone())
        });
    let options = models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let label = if i == 0 {
                format!("{m} (recommended)")
            } else {
                m.clone()
            };
            (SharedString::from(m.clone()), SharedString::from(label))
        })
        .collect();
    let e = entity.clone();
    Row::new("git.commit_message.model", "Model")
        .description("The agent's models; another can be typed below")
        .control(menu_button(
            "menu-git.commit_message.model",
            options,
            model.into(),
            true,
            move |m, cx| e.update(cx, |ws, cx| ws.set_commit_model(&m, cx)),
        ))
        .reset(
            ws.config.git.commit_message.model.is_some(),
            remove(entity, TABLE, "model", |c| {
                c.git.commit_message.model = None
            }),
        )
}

/// What Another model shows: what is being typed while it is the model set, else the model
/// set (none: the recommended one, which the list above shows).
fn typed_model(ws: &Workspace) -> String {
    let set = ws
        .config
        .git
        .commit_message
        .model
        .clone()
        .unwrap_or_default();
    match ws.drafts.get("model") {
        Some(typed) if typed.trim() == set => typed.clone(),
        _ => set,
    }
}

/// Typing in Another model: the text is kept as typed (a space before the next word stays),
/// the model trimmed.
fn set_typed_model(ws: &mut Workspace, text: &str, cx: &mut Context<Workspace>) {
    ws.drafts.insert("model", text.to_string());
    ws.set_commit_model(text, cx);
}

/// `instructions`, kept as typed; empty removes the key.
pub(crate) fn set_commit_instructions(ws: &mut Workspace, text: &str, cx: &mut Context<Workspace>) {
    if text.trim().is_empty() {
        ws.edit(
            Edit::remove(TABLE, "instructions"),
            |c| c.git.commit_message.instructions.clear(),
            cx,
        );
    } else {
        let kept = text.to_string();
        ws.edit(
            Edit::set(TABLE, "instructions", text),
            |c| c.git.commit_message.instructions = kept,
            cx,
        );
    }
}

/// `prompt`: the default, or nothing, removes the key (Reset to default).
pub(crate) fn set_commit_prompt(ws: &mut Workspace, text: &str, cx: &mut Context<Workspace>) {
    if text == message::PROMPT || text.trim().is_empty() {
        ws.edit(
            Edit::remove(TABLE, "prompt"),
            |c| c.git.commit_message.prompt = None,
            cx,
        );
    } else {
        let kept = text.to_string();
        ws.edit(
            Edit::set(TABLE, "prompt", text),
            |c| c.git.commit_message.prompt = Some(kept),
            cx,
        );
    }
}

/// Try it: a button (it spins, and stops the agent, while it runs), and under it the answer, or
/// the whole error in red.
fn try_row(ws: &Workspace, entity: &Entity<Workspace>) -> Row {
    let ui = ws.ui;
    let running = matches!(ws.message_try, Some(TryIt::Running(_)));
    let e = entity.clone();
    let button = div()
        .id("git-try-message")
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(10.0))
        .py(px(3.0))
        .rounded_full()
        .border_1()
        .border_color(ui.hairline)
        .text_size(px(12.0))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .active(move |s| s.bg(ui.pressed))
        .when(running, |el| {
            el.child(Spinner::new().with_size(UiSize::Size(px(12.0))))
        })
        .child(if running { "Stop" } else { "Try it" })
        .on_click(move |_: &ClickEvent, _, cx| e.update(cx, |ws, cx| ws.try_message(cx)))
        .test_support();
    let shown = match &ws.message_try {
        Some(TryIt::Answer(text)) => Some((text.clone(), ui.title)),
        Some(TryIt::Failed(text)) => Some((text.clone(), hsla(ws.theme.palette.ansi[1], 1.0))),
        _ => None,
    };
    let row = Row::new("git.commit_message.try", "Try it")
        .description("Runs the agent on a small sample change, not yours, and shows its answer")
        .control(button);
    match shown {
        Some((text, color)) => row.below(
            div()
                .id("git-try-result")
                .p(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(ui.hairline)
                .font_family(ws.cell.font.family.clone())
                .text_size(px(12.0))
                .text_color(color)
                .child(text)
                .test_support(),
        ),
        None => row,
    }
}
