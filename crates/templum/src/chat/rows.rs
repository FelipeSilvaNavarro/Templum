//! The chat's rows (docs/specs/templum-agent.md#rows): what the person wrote, what the agent
//! wrote, and each action as one line Templum writes.

use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Icon, Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, ElementId, SharedString, TestSupportExt, div, px, relative,
};
use serde_json::Value;
use templum_agent::{ActionState, Row};

use crate::grid::hsla;
use crate::workspace::Workspace;

/// Row `n` of the conversation, `row`.
pub(crate) fn row(ws: &Workspace, n: usize, row: &Row, cx: &mut Context<Workspace>) -> AnyElement {
    let ui = ws.ui;
    let red = hsla(ws.theme.palette.ansi[1], 1.0);
    let id = ElementId::Name(format!("agent-row-{n}").into());
    let small = |text: String, color| div().text_size(px(12.0)).text_color(color).child(text);
    let el = div().id(id).w_full().flex();
    let el = match row {
        Row::Person { text, queued } => el.justify_end().child(
            div()
                .max_w(relative(0.8))
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(12.0))
                .bg(ui.hover)
                .text_size(px(13.0))
                .text_color(ui.title)
                .child(text.clone())
                .when(*queued, |el| el.child(small("Queued".into(), ui.title_dim))),
        ),
        Row::Text { text } => el.child(
            div()
                .w_full()
                .text_size(px(13.0))
                .text_color(ui.title)
                .child(
                    TextView::markdown(
                        ElementId::Name(format!("agent-md-{n}").into()),
                        SharedString::from(text.clone()),
                    )
                    .style(crate::markdown::style(1.0))
                    .selectable(true),
                ),
        ),
        Row::Thinking => el.child(small("Thinking".into(), ui.title_dim)),
        Row::Action {
            tool,
            input,
            state,
            said,
            target,
            ..
        } => {
            let (icon, color): (AnyElement, _) = match state {
                ActionState::Running => (
                    Spinner::new()
                        .color(ui.title_dim)
                        .with_size(UiSize::Size(px(12.0)))
                        .into_any_element(),
                    ui.title,
                ),
                ActionState::Failed => (
                    Icon::new(icon(tool, input))
                        .size(px(14.0))
                        .text_color(red)
                        .into_any_element(),
                    ui.title,
                ),
                _ => (
                    Icon::new(icon(tool, input))
                        .size(px(14.0))
                        .text_color(ui.title_dim)
                        .into_any_element(),
                    ui.title,
                ),
            };
            let sentence = match state {
                ActionState::Running => format!("{}…", templum_mcp::control::title(tool)),
                ActionState::Denied => "You denied this.".to_string(),
                ActionState::Failed => templum_mcp::control::title(tool).to_string(),
                ActionState::Done => said.clone(),
            };
            el.flex_col()
                .when(target.is_some(), |el| {
                    el.cursor_pointer().hover(move |s| s.bg(ui.hover)).on_click(
                        cx.listener(move |ws, _: &ClickEvent, window, cx| {
                            ws.show_row(n, window, cx)
                        }),
                    )
                })
                .rounded(px(6.0))
                .px(px(4.0))
                .py(px(2.0))
                .child(
                    div().flex().items_center().gap(px(8.0)).child(icon).child(
                        div()
                            .text_size(px(13.0))
                            .text_color(if *state == ActionState::Denied {
                                ui.title_dim
                            } else {
                                color
                            })
                            .child(sentence),
                    ),
                )
                .when(*state == ActionState::Failed && !said.is_empty(), |el| {
                    el.child(
                        div()
                            .pl(px(22.0))
                            .text_size(px(12.0))
                            .text_color(red.opacity(0.75))
                            .child(said.clone()),
                    )
                })
        }
        Row::Reads { count } => el.child(small(
            format!("Looked at the canvas ({count})"),
            ui.title_dim,
        )),
        Row::Approval { said, answer } => el.child(
            div()
                .flex()
                .gap(px(6.0))
                .text_size(px(12.0))
                .child(div().text_color(ui.title).child(said.clone()))
                .child(div().text_color(ui.title_dim).child(answer.clone())),
        ),
        Row::End { ms, cost_usd } => {
            let mut parts = Vec::new();
            if let Some(ms) = ms {
                parts.push(crate::alert::took(std::time::Duration::from_millis(*ms)));
            }
            if let Some(cost) = cost_usd {
                parts.push(format!("${cost:.2}"));
            }
            el.justify_end()
                .child(small(parts.join(" · "), ui.title_dim))
        }
        Row::Failure { text } => el.child(
            div()
                .text_size(px(13.0))
                .text_color(red)
                .child(text.clone()),
        ),
        Row::Note { text } => el.justify_center().child(small(text.clone(), ui.title_dim)),
    };
    el.test_support().into_any_element()
}

/// The icon of a tool's kind: workspaces, terminals, agents, boards and cards, notes,
/// checklists, wires, showing.
fn icon(tool: &str, input: &Value) -> IconName {
    match tool {
        "create_workspace" => IconName::LayoutGrid,
        "show" => IconName::Eye,
        "add_block" => match input["kind"].as_str() {
            Some("kanban") => IconName::Kanban,
            Some("note") => IconName::StickyNote,
            Some("checklist") => IconName::ListChecks,
            _ => IconName::SquareTerminal,
        },
        "open_agent" | "list_agents" | "notify" => IconName::Bot,
        "list_cards" | "create_card" | "edit_card" | "move_card" | "comment_card" => {
            IconName::Kanban
        }
        "read_note" | "append_note" => IconName::StickyNote,
        "read_checklist" | "add_items" | "check_item" => IconName::ListChecks,
        "connect" | "disconnect" => IconName::Cable,
        _ => IconName::SquareTerminal,
    }
}
