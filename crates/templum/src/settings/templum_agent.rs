//! Settings ▸ Templum Agent (docs/specs/templum-agent.md#settings--templum-agent): the agent the
//! chat runs on, its model and effort, and the pill.

use gpui_kit::component::switch::Switch;
use gpui_kit::{App, Context, ElementId, Entity, SharedString, Window};

use super::row::{Row, menu_button, remove, text_input};
use crate::config::Edit;
use crate::workspace::Workspace;

/// The engines the Templum Agent runs on: Claude Code.
fn claude() -> templum_terminal::Agent {
    templum_terminal::Agent::from_name("claude").expect("Claude Code")
}

pub(super) fn rows(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) -> Vec<Row> {
    let config = &ws.config.templum_agent;
    let label = if templum_terminal::installed("claude") {
        "Claude Code".to_string()
    } else {
        "Claude Code (not installed)".to_string()
    };
    let efforts = std::iter::once((SharedString::default(), SharedString::from("Default")))
        .chain(
            claude()
                .efforts()
                .iter()
                .map(|e| (SharedString::from(*e), SharedString::from(*e))),
        )
        .collect();
    let (e, p) = (entity.clone(), entity.clone());
    let pill = config.pill;
    vec![
        Row::new("templum_agent.agent", "Agent")
            .description("The CLI agent the chat runs on, with your login and plan")
            .control(menu_button(
                ElementId::Name("menu-templum_agent.agent".into()),
                vec![(SharedString::from("claude"), SharedString::from(label))],
                SharedString::from(config.agent.clone()),
                false,
                |_, _| {},
            )),
        Row::new("templum_agent.model", "Model")
            .description(
                "Empty is the agent's own default: 1 to 64 letters, digits, ., -, _, : and /",
            )
            .control(text_input(
                entity,
                "templum_agent.model",
                config.model.clone(),
                set_model,
                window,
                cx,
            ))
            .reset(
                !config.model.is_empty(),
                remove(entity, "templum_agent", "model", |c| {
                    c.templum_agent.model.clear()
                }),
            ),
        Row::new("templum_agent.effort", "Effort")
            .description("How hard it thinks")
            .control(menu_button(
                ElementId::Name("menu-templum_agent.effort".into()),
                efforts,
                SharedString::from(config.effort.clone()),
                false,
                move |effort, cx| e.update(cx, |ws, cx| set_effort(ws, &effort, cx)),
            ))
            .reset(
                !config.effort.is_empty(),
                remove(entity, "templum_agent", "effort", |c| {
                    c.templum_agent.effort.clear()
                }),
            ),
        Row::new("templum_agent.pill", "Prompt pill")
            .description("Its input at the bottom of the canvas; off, the shortcut still opens it")
            .control(Switch::new("templum-agent-pill").checked(pill).on_click(
                move |on: &bool, _, cx| {
                    let on = *on;
                    p.update(cx, |ws, cx| {
                        ws.edit(
                            Edit::set("templum_agent", "pill", on),
                            |c| c.templum_agent.pill = on,
                            cx,
                        )
                    })
                },
            ))
            .reset(
                !pill,
                remove(entity, "templum_agent", "pill", |c| {
                    c.templum_agent.pill = true
                }),
            ),
    ]
}

/// `templum_agent.model`, trimmed: empty removes the key; a model Templum wouldn't type isn't
/// taken (the field goes back to the last one).
fn set_model(ws: &mut Workspace, text: &str, cx: &mut Context<Workspace>) {
    let model = text.trim().to_string();
    if model.is_empty() {
        ws.edit(
            Edit::remove("templum_agent", "model"),
            |c| c.templum_agent.model.clear(),
            cx,
        );
    } else if claude().launch_args(Some(&model), None).is_ok() {
        ws.edit(
            Edit::set("templum_agent", "model", model.clone()),
            move |c| c.templum_agent.model = model.clone(),
            cx,
        );
    }
}

/// `templum_agent.effort`: Default removes the key.
fn set_effort(ws: &mut Workspace, effort: &str, cx: &mut Context<Workspace>) {
    let effort = effort.to_string();
    if effort.is_empty() {
        ws.edit(
            Edit::remove("templum_agent", "effort"),
            |c| c.templum_agent.effort.clear(),
            cx,
        );
    } else {
        ws.edit(
            Edit::set("templum_agent", "effort", effort.clone()),
            move |c| c.templum_agent.effort = effort.clone(),
            cx,
        );
    }
}
