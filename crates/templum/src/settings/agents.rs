//! The Agents section: a row per agent, "Add an agent", and the dialog that edits or adds one.

use gpui_kit::assets::IconName;
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Icon, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, TestSupportExt, Window, div, px,
};

use super::row::Row;
use crate::agents::{AgentEntry, Draft};
use crate::grid::hsla;
use crate::theme::Ui;
use crate::workspace::{Workspace, show_notification};

/// One agent: its command; whether the canvas's menu lists it; Edit; ↺ for one of Templum's the
/// user changed.
pub(super) fn agent_row(ws: &Workspace, entity: &Entity<Workspace>, agent: &AgentEntry) -> Row {
    let ui = ws.ui;
    let id = agent.id.clone();
    let description = match agent.builtin {
        Some(_) => agent.command.clone(),
        None => format!("Yours · {}", agent.command),
    };
    let show = {
        let (e, agent) = (entity.clone(), agent.clone());
        Switch::new(ElementId::Name(format!("show-agent-{id}").into()))
            .checked(agent.show)
            .on_click(move |on: &bool, _, cx| {
                let draft = Draft {
                    show: *on,
                    ..Draft::of(&agent)
                };
                let id = agent.id.clone();
                e.update(cx, |ws, cx| ws.save_agent(Some(&id), &draft, cx).ok());
            })
    };
    let edit = {
        let (e, agent) = (entity.clone(), agent.clone());
        div()
            .id(ElementId::Name(format!("edit-agent-{id}").into()))
            .px(px(10.0))
            .py(px(3.0))
            .rounded_full()
            .border_1()
            .border_color(ui.hairline)
            .text_size(px(12.0))
            .cursor_pointer()
            .hover(move |s| s.bg(ui.hover))
            .active(move |s| s.bg(ui.pressed))
            .child("Edit")
            .on_click(move |_: &ClickEvent, window, cx| {
                let agent = agent.clone();
                e.update(cx, |ws, cx| agent_dialog(ws, Some(agent), window, cx))
            })
            .test_support()
    };
    let e = entity.clone();
    let reset_id = id.clone();
    Row::new(format!("agents.{id}"), agent.title.clone())
        .description(description)
        .control(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(show)
                .child(edit),
        )
        .reset(agent.builtin.is_some() && agent.changed, move |_, cx| {
            e.update(cx, |ws, cx| ws.remove_agent(&reset_id, cx))
        })
}

/// "Add an agent", under the agents.
pub(super) fn add_agent(ui: Ui, cx: &mut Context<Workspace>) -> AnyElement {
    div()
        .id("add-agent")
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(14.0))
        .py(px(10.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(ui.hairline)
        .text_size(px(13.0))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .active(move |s| s.bg(ui.pressed))
        .child(Icon::new(IconName::Plus).size(px(14.0)))
        .child("Add an agent")
        .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| agent_dialog(ws, None, window, cx)))
        .test_support()
        .into_any_element()
}

/// Editing an agent (`agent`), or adding one (None): its name, command and resume command.
/// Saving one of Templum's back at its defaults takes it out of config.toml.
pub(crate) fn agent_dialog(
    ws: &Workspace,
    agent: Option<AgentEntry>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let ui = ws.ui;
    let red = hsla(ws.theme.palette.ansi[1], 1.0);
    let entity = cx.entity();
    let draft = agent.as_ref().map_or(
        Draft {
            show: true,
            ..Draft::default()
        },
        Draft::of,
    );
    let field = |text: &str,
                 placeholder: &'static str,
                 window: &mut Window,
                 cx: &mut Context<Workspace>| {
        let text = text.to_string();
        cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(text)
        })
    };
    let name = field(&draft.name, "What the menu shows", window, cx);
    let command = field(
        &draft.command,
        "What starts it, like my-agent --tui",
        window,
        cx,
    );
    let resume = field(
        &draft.resume,
        "Empty: a new session when the canvas comes back",
        window,
        cx,
    );
    let builtin = agent.as_ref().and_then(|a| a.builtin);
    let id = agent.as_ref().map(|a| a.id.clone());
    let title = match &agent {
        Some(agent) => format!("Edit {}", agent.title),
        None => "Add an agent".to_string(),
    };
    let save = {
        let (entity, name, command, resume, id) = (
            entity.clone(),
            name.clone(),
            command.clone(),
            resume.clone(),
            id.clone(),
        );
        move |window: &mut Window, cx: &mut App| -> bool {
            let draft = Draft {
                name: name.read(cx).value().to_string(),
                command: command.read(cx).value().to_string(),
                resume: resume.read(cx).value().to_string(),
                show: draft.show,
            };
            let saved = entity.update(cx, |ws, cx| ws.save_agent(id.as_deref(), &draft, cx));
            match saved {
                Ok(_) => true,
                Err(problem) => {
                    show_notification(window, cx, Notification::warning(problem));
                    false
                }
            }
        }
    };
    let first = name.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let label = |text: &'static str| {
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .text_color(ui.title_dim)
                .child(text)
        };
        let default = |text: String| {
            div()
                .text_size(px(11.0))
                .text_color(ui.title_dim)
                .child(text)
        };
        let delete = id.clone().filter(|_| builtin.is_none()).map(|id| {
            let entity = entity.clone();
            div()
                .id("delete-agent")
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .text_color(red)
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child("Delete this agent")
                .on_click(move |_: &ClickEvent, window, cx| {
                    entity.update(cx, |ws, cx| ws.remove_agent(&id, cx));
                    window.close_dialog(cx);
                })
                .test_support()
        });
        let save = save.clone();
        alert
            .confirm()
            .title(title.clone())
            .w(px(480.0))
            .button_props(DialogButtonProps::default().ok_text("Save"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(label("Name"))
                    .child(Input::new(&name))
                    .child(label("Command"))
                    .child(Input::new(&command))
                    .when_some(builtin, |el, agent| {
                        el.child(default(format!("Templum's: {}", agent.command())))
                    })
                    .child(label("Resume command"))
                    .child(Input::new(&resume))
                    .when_some(builtin, |el, agent| {
                        el.child(default(match agent.spec().resume {
                            Some(resume) => format!("Templum's: {resume}"),
                            None => "Templum's: a new session (this agent can't resume by folder)"
                                .to_string(),
                        }))
                    })
                    .children(delete),
            )
            .on_ok(move |_, window, cx| save(window, cx))
    });
    first.update(cx, |input, cx| input.focus(window, cx));
}
