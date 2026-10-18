//! Settings ▸ Connections (docs/specs/connections.md, Config): the switch for the agents Templum
//! opens, Templum's server for other agents to copy, and Gemini CLI's extension.

use std::path::Path;

use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{App, ClickEvent, Entity, TestSupportExt, Window, div, px};

use super::fields::QUESTION_TIMEOUT;
use super::row::{Row, remove};
use super::sections::number_row;
use crate::config::Edit;
use crate::grid::hsla;
use crate::prompt;
use crate::workspace::{TryIt, Workspace, templum_in};

/// The questions Install and Remove ask: title, text, button.
const INSTALL: (&str, &str, &str) = (
    "Install Templum in Gemini CLI?",
    "Templum runs gemini extensions link with its extension. Every Gemini CLI you open then starts \
     Templum's server and hooks; outside Templum the server shows as disconnected. Remove takes it \
     back.",
    "Install",
);
const REMOVE: (&str, &str, &str) = (
    "Remove Templum from Gemini CLI?",
    "Templum runs gemini extensions uninstall templum.",
    "Remove",
);

pub(super) fn rows(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) -> Vec<Row> {
    let on = ws.config.connections.enabled;
    let sound = ws.config.connections.sound;
    let e = entity.clone();
    let s = entity.clone();
    let mut rows = vec![
        Row::new("connections.enabled", "Agents in Templum")
            .description(
                "Agents opened from the canvas or typed in a terminal get Templum's server and \
                 hooks: they read and type into the blocks wired to them",
            )
            .control(Switch::new("connections-switch").checked(on).on_click(
                move |on: &bool, _, cx| {
                    let on = *on;
                    e.update(cx, |ws, cx| {
                        ws.edit(
                            Edit::set("connections", "enabled", on),
                            |c| c.connections.enabled = on,
                            cx,
                        )
                    })
                },
            ))
            .reset(
                !on,
                remove(entity, "connections", "enabled", |c| {
                    c.connections.enabled = true
                }),
            ),
        number_row(ws, entity, &QUESTION_TIMEOUT, "Questions wait", window, cx).description(
            "How long an agent's question waits for your answer, in minutes. 0 waits until you \
             answer",
        ),
        Row::new("connections.sound", "Sound")
            .description("A short sound when an agent asks you something")
            .control(Switch::new("connections-sound").checked(sound).on_click(
                move |on: &bool, _, cx| {
                    let on = *on;
                    s.update(cx, |ws, cx| {
                        ws.edit(
                            Edit::set("connections", "sound", on),
                            |c| c.connections.sound = on,
                            cx,
                        )
                    })
                },
            ))
            .reset(
                !sound,
                remove(entity, "connections", "sound", |c| {
                    c.connections.sound = true
                }),
            ),
    ];
    let Some(data) = ws.data_dir.as_deref().filter(|_| ws.bus.is_some()) else {
        rows.push(Row::new("connections.none", "No server").description(
            "This Templum runs without its data folder: no agent gets its server, and its \
             terminals have no templum command",
        ));
        return rows;
    };
    let command = format!("{} mcp", templum_in(data).display());
    rows.push(
        Row::new("connections.command", "Other agents")
            .description(
                "Grok Build, Cursor CLI, Kimi Code or your own: add this command to their MCP \
                 servers. It finds its block from the terminal it runs in; an agent that hands a \
                 server only the variables it names needs TEMPLUM_BUS and TEMPLUM_TERMINAL",
            )
            .control(Clipboard::new("connections-copy").value(command.clone()))
            .below(
                div()
                    .id("connections-command")
                    .font_family(ws.cell.font.family.clone())
                    .text_size(px(12.0))
                    .child(command)
                    .test_support(),
            ),
    );
    rows.push(gemini_row(ws, entity, data));
    rows
}

/// Gemini CLI: Install or Remove, each after a question (it spins while Gemini CLI runs), and
/// under it what went wrong, in red.
fn gemini_row(ws: &Workspace, entity: &Entity<Workspace>, data: &Path) -> Row {
    let ui = ws.ui;
    let running = matches!(ws.gemini_job, Some(TryIt::Running(_)));
    let installed = templum_usage::Dirs::of(data)
        .gemini
        .is_some_and(|dir| templum_usage::gemini::installed(&dir));
    let e = entity.clone();
    let button = div()
        .id("gemini-setup")
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(10.0))
        .py(px(3.0))
        .rounded_full()
        .border_1()
        .border_color(ui.hairline)
        .text_size(px(12.0))
        .when(running, |el| {
            el.child(Spinner::new().with_size(UiSize::Size(px(12.0))))
        })
        .when(!running, |el| {
            el.cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .active(move |s| s.bg(ui.pressed))
                .on_click(move |_: &ClickEvent, window, cx| {
                    let e = e.clone();
                    let (title, text, ok) = if installed { REMOVE } else { INSTALL };
                    prompt::confirm(
                        title,
                        text,
                        ok,
                        move |_, cx| e.update(cx, |ws, cx| ws.setup_gemini(!installed, cx)),
                        window,
                        cx,
                    );
                })
        })
        .child(if installed { "Remove" } else { "Install" })
        .test_support();
    let row = Row::new("connections.gemini", "Gemini CLI")
        .description(if installed {
            "Templum's extension is installed: every Gemini CLI you open gets its server and hooks"
        } else {
            "It takes Templum's server and hooks as an extension, installed once for every Gemini \
             CLI you open"
        })
        .control(button);
    match &ws.gemini_job {
        Some(TryIt::Failed(why)) => row.below(
            div()
                .id("gemini-setup-result")
                .p(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(ui.hairline)
                .font_family(ws.cell.font.family.clone())
                .text_size(px(12.0))
                .text_color(hsla(ws.theme.palette.ansi[1], 1.0))
                .child(why.clone())
                .test_support(),
        ),
        _ => row,
    }
}
