//! The welcome's settings: each page's groups, made of the theme and shell cards and of the rows
//! the panel's sections build, picked by key. A change goes through the workspace as in the panel.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, Entity, FontWeight, SharedString, Stateful,
    TestSupportExt, Window, div, px,
};

use super::agents::agent_row;
use super::edits::{program_text, set_shell};
use super::row::Row;
use super::sections::{rows, set_theme};
use super::{Section, card};
use crate::config::Edit;
use crate::grid::hsla;
use crate::theme::{Theme, Ui};
use crate::welcome::Page;
use crate::workspace::Workspace;

/// The settings on `page`, group by group (none on the first). `installed`: the agents found on
/// this computer, by id.
pub(crate) fn welcome_groups(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    page: Page,
    installed: &[String],
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let ui = ws.ui;
    let pick = |section: Section, keys: &[&str], window: &mut Window, cx: &mut App| {
        rows(ws, entity, section, window, cx)
            .into_iter()
            .filter(|r| keys.contains(&r.key()))
            .collect::<Vec<Row>>()
    };
    match page {
        Page::Hello => Vec::new(),
        Page::Look => {
            let dark = ws.dark;
            let other = ws.config.theme.name(!dark);
            vec![
                card(ui, pick(Section::Appearance, &["theme.mode"], window, cx)).into_any_element(),
                group(
                    ui,
                    if dark {
                        "Theme for dark mode"
                    } else {
                        "Theme for light mode"
                    },
                    Some(format!(
                        "{} mode keeps {other}: pick its theme with the mode above, or in \
                         Settings ▸ Appearance.",
                        if dark { "Light" } else { "Dark" }
                    )),
                    theme_cards(ws, entity),
                ),
                group(
                    ui,
                    "Font",
                    None,
                    card(ui, rows(ws, entity, Section::Font, window, cx)),
                ),
            ]
        }
        Page::Shell => vec![
            group(
                ui,
                "Shell",
                Some("Applies to new terminals".into()),
                shell_cards(ws, entity),
            ),
            group(
                ui,
                "Terminals",
                None,
                card(
                    ui,
                    pick(
                        Section::Shell,
                        &[
                            "shell.working_directory",
                            "shell.folder",
                            "shell.integration",
                        ],
                        window,
                        cx,
                    ),
                ),
            ),
        ],
        Page::Agents => {
            let found: Vec<Row> = ws
                .agents
                .iter()
                .filter(|a| a.account.is_none() && installed.contains(&a.id))
                .map(|a| agent_row(ws, entity, a))
                .collect();
            let found = if found.is_empty() {
                div()
                    .id("welcome-no-agents")
                    .px(px(14.0))
                    .py(px(12.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(ui.hairline)
                    .bg(ui.card)
                    .text_size(px(13.0))
                    .text_color(ui.title)
                    .child(
                        "None found yet. Settings ▸ Agents lists them all, each with how to \
                         install it.",
                    )
                    .test_support()
                    .into_any_element()
            } else {
                card(ui, found).into_any_element()
            };
            vec![
                group(
                    ui,
                    "On this computer",
                    Some(
                        "The canvas's right-click menu, AI agent ▸, lists the ones switched on, \
                         and opens them in the folder you pick."
                            .into(),
                    ),
                    found,
                ),
                group(
                    ui,
                    "Commit messages",
                    Some(
                        "Generate ✨, beside a Git block's summary, asks this agent to write the \
                         message for you to read."
                            .into(),
                    ),
                    card(
                        ui,
                        pick(
                            Section::Git,
                            &["git.commit_message.agent", "git.commit_message.model"],
                            window,
                            cx,
                        ),
                    ),
                ),
                group(
                    ui,
                    "Plan limits",
                    Some(
                        "Claude Code's and Codex's limits in the title bar, and a warning before \
                         you run out."
                            .into(),
                    ),
                    card(
                        ui,
                        pick(
                            Section::Usage,
                            &["usage.claude_limits", "usage.warn", "usage.reset"],
                            window,
                            cx,
                        ),
                    ),
                ),
            ]
        }
        Page::Ready => {
            let mut rows = pick(Section::Terminal, &["terminal.notifications"], window, cx);
            rows.extend(pick(Section::Updates, &["updates.check"], window, cx));
            vec![group(ui, "Notifications and updates", None, card(ui, rows))]
        }
    }
}

/// A group's title, what it is about, and its controls.
fn group(ui: Ui, title: &'static str, about: Option<String>, body: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .when_some(about, |el, about| {
            el.child(
                div()
                    .text_size(px(12.0))
                    .text_color(ui.title_dim)
                    .child(about),
            )
        })
        .child(body)
        .into_any_element()
}

/// The themes of the mode in effect, each a card in its own colors; a click makes it that mode's
/// theme, as choosing one in the picker does.
fn theme_cards(ws: &Workspace, entity: &Entity<Workspace>) -> AnyElement {
    let ui = ws.ui;
    let dark = ws.dark;
    let key = if dark { "dark" } else { "light" };
    let current = ws.config.theme.name(dark);
    let font = ws.cell.font.family.clone();
    let cards = ws
        .themes
        .iter()
        .filter(|e| e.theme.is_dark == dark)
        .map(|e| {
            let name = e.theme.name.clone();
            let id = ElementId::Name(format!("welcome-theme-{name}").into());
            let entity = entity.clone();
            theme_card(id, &e.theme, ui, font.clone(), name == current)
                .on_click(move |_: &ClickEvent, _, cx| {
                    let name = name.clone();
                    entity.update(cx, |ws, cx| {
                        ws.edit(
                            Edit::set("theme", key, name.as_str()),
                            |c| set_theme(c, dark, name.clone()),
                            cx,
                        )
                    })
                })
                .test_support()
        });
    div()
        .grid()
        .grid_cols(3)
        .gap(px(10.0))
        .children(cards)
        .into_any_element()
}

/// A theme as a card in its own colors: its name, six of its colors and a prompt. `selected`
/// draws the accent around it.
pub(crate) fn theme_card(
    id: ElementId,
    theme: &Theme,
    ui: Ui,
    font: SharedString,
    selected: bool,
) -> Stateful<Div> {
    let p = theme.palette;
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(10.0))
        .border_2()
        .border_color(if selected { ui.accent } else { ui.hairline })
        .bg(hsla(p.background, 1.0))
        .text_color(hsla(p.foreground, 1.0))
        .cursor_pointer()
        .when(!selected, |el| {
            el.hover(move |s| s.border_color(ui.hover_border))
        })
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .truncate()
                .child(SharedString::from(theme.name.clone())),
        )
        .child(
            div().flex().gap(px(4.0)).children(
                (1..7).map(|c| div().size(px(10.0)).rounded_full().bg(hsla(p.ansi[c], 1.0))),
            ),
        )
        .child(
            div()
                .flex()
                .font_family(font)
                .text_size(px(11.0))
                .child(div().text_color(hsla(p.ansi[2], 1.0)).child("❯ "))
                .child("git status"),
        )
}

/// The default shell and each one found on this computer, the one new terminals run checked. A
/// program typed in Settings that isn't among them shows as Custom.
fn shell_cards(ws: &Workspace, entity: &Entity<Workspace>) -> AnyElement {
    let ui = ws.ui;
    let shell = &ws.config.shell;
    // Some(None): the default; Some(Some(i)): the i-th found; None: typed by hand.
    let chosen = match &shell.program {
        None => Some(None),
        Some(program) => ws
            .shells
            .iter()
            .position(|s| s.program == *program && s.args == shell.args)
            .map(Some),
    };
    let e = entity.clone();
    let mut cards = vec![option(
        ui,
        "welcome-shell-default".into(),
        format!("Default ({})", ws.default_shell),
        "The one your system starts".into(),
        chosen == Some(None),
        Some(Box::new(move |cx: &mut App| {
            e.update(cx, |ws, cx| set_shell(ws, None, cx))
        })),
    )];
    for (i, s) in ws.shells.iter().enumerate() {
        let (e, s) = (entity.clone(), s.clone());
        let line = std::iter::once(s.program.as_str())
            .chain(s.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        cards.push(option(
            ui,
            format!("welcome-shell-{i}"),
            s.label.clone(),
            line,
            chosen == Some(Some(i)),
            Some(Box::new(move |cx: &mut App| {
                e.update(cx, |ws, cx| set_shell(ws, Some(&s), cx))
            })),
        ));
    }
    if chosen.is_none() {
        cards.push(option(
            ui,
            "welcome-shell-custom".into(),
            "Custom".into(),
            program_text(ws),
            true,
            None,
        ));
    }
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .children(cards)
        .into_any_element()
}

/// What choosing an option does.
type Pick = Box<dyn Fn(&mut App)>;

/// One choice of a list: a round check, its name and a line under it. `pick` runs on a click.
fn option(
    ui: Ui,
    id: String,
    title: String,
    line: String,
    checked: bool,
    pick: Option<Pick>,
) -> AnyElement {
    div()
        .id(ElementId::Name(id.into()))
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(14.0))
        .py(px(10.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(if checked { ui.accent } else { ui.hairline })
        .bg(if checked { ui.hover } else { ui.card })
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(16.0))
                .flex_shrink_0()
                .rounded_full()
                .border_1()
                .border_color(if checked { ui.accent } else { ui.title_dim })
                .when(checked, |el| {
                    el.child(div().size(px(8.0)).rounded_full().bg(ui.accent))
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(ui.title_dim)
                        .truncate()
                        .child(line),
                ),
        )
        .when_some(pick, |el, pick| {
            el.cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .on_click(move |_: &ClickEvent, _, cx| pick(cx))
        })
        .test_support()
        .into_any_element()
}
