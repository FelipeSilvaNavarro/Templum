//! The settings panel: it slides in from the right over the canvas. Tabs pick a section, and
//! each field is a row (title, description, control, reset) in one card, built every frame from
//! the workspace's state. A change goes through the workspace, which applies it on the next frame
//! and saves it shortly after.

use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::NumberInput;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Icon, Sizable as _, TITLE_BAR_HEIGHT};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, BoxShadow, ClickEvent, Context, ElementId, Entity, FontWeight, MouseDownEvent,
    SharedString, TestSupportExt, Window, div, point, px,
};

use crate::config::{self, Config, Edit, ThemeConfig, WorkingDirectory};
use crate::grid::hsla;
use crate::keys::Command;
use crate::motion;
use crate::theme::Ui;
use crate::workspace::Workspace;

mod agents;
mod edits;
mod fields;
mod fonts;
mod row;
#[cfg(test)]
mod tests;

use agents::{add_agent, agent_row};
use edits::set_shell;
pub(crate) use edits::{
    args_error, args_text, folder_hint, folder_text, program_text, set_args, set_family,
    set_folder, set_folder_kind, set_git_program, set_program,
};
use fields::{CURSOR, Choice, FONT_SIZE, MODE, Number};
pub(crate) use fields::{NumberState, SCROLLBACK};
pub(crate) use fonts::monospace_families;
use row::{Reset, Row, menu_button, remove, text_input};

/// The panel's width, at most 60% of the canvas.
const WIDTH: f32 = 480.0;

/// The panel's sections, in tab order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Appearance,
    Font,
    Terminal,
    Shell,
    Tools,
    Agents,
    Shortcuts,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::Appearance,
        Section::Font,
        Section::Terminal,
        Section::Shell,
        Section::Tools,
        Section::Agents,
        Section::Shortcuts,
    ];

    fn title(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::Font => "Font",
            Section::Terminal => "Terminal",
            Section::Shell => "Shell",
            Section::Tools => "Tools",
            Section::Agents => "Agents",
            Section::Shortcuts => "Shortcuts",
        }
    }

    /// Its tab's element id.
    fn tab_id(self) -> &'static str {
        match self {
            Section::Appearance => "settings-tab-appearance",
            Section::Font => "settings-tab-font",
            Section::Terminal => "settings-tab-terminal",
            Section::Shell => "settings-tab-shell",
            Section::Tools => "settings-tab-tools",
            Section::Agents => "settings-tab-agents",
            Section::Shortcuts => "settings-tab-shortcuts",
        }
    }

    /// A line under the heading, for what holds for the whole section.
    fn note(self) -> Option<&'static str> {
        match self {
            Section::Shell => Some("Applies to new terminals"),
            Section::Tools => {
                Some("The programs the Git block runs. Checked when the panel opens.")
            }
            Section::Agents => Some(
                "The AI agents in the canvas menu. Change one of Templum's, or add your own; ↺ puts \
                 one of Templum's back as it was.",
            ),
            Section::Shortcuts => Some("Click a shortcut to record a new one"),
            _ => None,
        }
    }
}

/// The open panel, or the closed one still sliding out.
pub(crate) struct Panel {
    pub since: Instant,
    pub closing: bool,
    pub section: Section,
}

impl Panel {
    pub fn new(now: Instant) -> Panel {
        Panel {
            since: now,
            closing: false,
            section: Section::Appearance,
        }
    }

    /// How much of the panel is in view at `now`: 0 hidden, 1 all.
    pub fn shown(&self, now: Instant) -> f32 {
        let t = motion::slide(motion::progress(self.since, now, motion::SLIDE));
        if self.closing { 1.0 - t } else { t }
    }

    /// The panel sliding the other way, from where it is (no jump when turned back mid-slide).
    pub fn reversed(&self, now: Instant) -> Panel {
        let closing = !self.closing;
        let shown = self.shown(now);
        let eased = if closing { 1.0 - shown } else { shown };
        // motion::slide inverted: the progress that eases to `eased`.
        let progress = 1.0 - (1.0 - eased).cbrt();
        Panel {
            since: now
                .checked_sub(motion::SLIDE.mul_f32(progress))
                .unwrap_or(now),
            closing,
            section: self.section,
        }
    }
}

/// A key or a word in a small outlined pill.
pub(crate) fn chip<E: Styled>(el: E, ui: Ui) -> E {
    el.px(px(6.0))
        .py(px(2.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(ui.hairline)
        .bg(ui.card)
        .text_color(ui.title)
        .text_size(px(11.0))
}

pub(crate) fn render(
    ws: &Workspace,
    panel: &Panel,
    now: Instant,
    canvas_w: f32,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let width = WIDTH.min(canvas_w * 0.6);
    let shown = panel.shown(now);
    let entity = cx.entity();

    let close = div()
        .id("settings-close")
        .flex()
        .items_center()
        .justify_center()
        .size(px(26.0))
        .rounded_full()
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .active(move |s| s.bg(ui.pressed))
        .child(
            Icon::new(IconName::Close)
                .size(px(14.0))
                .text_color(ui.title),
        )
        .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| ws.close_settings(window, cx)))
        .test_support();
    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(48.0))
        .px(px(16.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Settings"),
        )
        .child(close);
    let tabs = div()
        .flex()
        .flex_wrap()
        .gap(px(4.0))
        .px(px(12.0))
        .pb(px(12.0))
        .border_b_1()
        .border_color(ui.hairline)
        .children(Section::ALL.map(|section| tab(ui, section, section == panel.section, cx)));

    let section = panel.section;
    let rows = rows(ws, &entity, section, window, cx);
    let resets: Vec<Reset> = rows.iter().filter_map(|r| r.reset.clone()).collect();
    let heading = div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(20.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(section.title()),
        )
        .when(!resets.is_empty(), |el| {
            el.child(
                div()
                    .id("settings-reset-section")
                    .px(px(10.0))
                    .py(px(3.0))
                    .rounded_full()
                    .text_size(px(12.0))
                    .text_color(ui.title)
                    .cursor_pointer()
                    .hover(move |s| s.bg(ui.hover))
                    .active(move |s| s.bg(ui.pressed))
                    .child("Reset all")
                    // Together, in one event: shortcuts reset at once may trade keys.
                    .on_click(move |_: &ClickEvent, window, cx| {
                        for reset in &resets {
                            reset(window, cx);
                        }
                    })
                    .test_support(),
            )
        });
    let card = div()
        .flex()
        .flex_col()
        .rounded(px(10.0))
        .border_1()
        .border_color(ui.hairline)
        .bg(ui.card)
        .children(
            rows.into_iter()
                .enumerate()
                .map(|(i, row)| row.render(ui, i == 0)),
        );
    let content = div()
        .id("settings-scroll")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .test_support()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .p(px(16.0))
                .child(heading)
                .when_some(section.note(), |el, note| {
                    el.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(ui.title_dim)
                            .child(note),
                    )
                })
                .child(div().mt(px(4.0)).child(card))
                .when(section == Section::Agents, |el| el.child(add_agent(ui, cx))),
        );

    let path = ws.config_dir.as_ref().map(|d| d.join(config::FILE));
    let status = match (&path, &ws.save_error) {
        (None, _) => Some((
            "No config folder: changes last until Templum quits".to_string(),
            ui.title_dim,
        )),
        (Some(_), Some(error)) => Some((error.clone(), hsla(ws.theme.palette.ansi[1], 1.0))),
        (Some(_), None) => None,
    };
    let footer = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .px(px(16.0))
        .py(px(12.0))
        .border_t_1()
        .border_color(ui.hairline)
        .text_size(px(12.0))
        .when_some(path, |el, path| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .truncate()
                            .text_color(ui.title_dim)
                            .child(path.display().to_string()),
                    )
                    .child(
                        Button::new("edit-config")
                            .label("Edit config.toml")
                            .small()
                            .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| {
                                ws.open_config(window, cx)
                            })),
                    ),
            )
        })
        .when_some(status, |el, (text, color)| {
            el.child(
                div()
                    .id("settings-status")
                    .text_color(color)
                    .child(text)
                    .test_support(),
            )
        });

    div()
        .id("settings-panel")
        .absolute()
        .top(TITLE_BAR_HEIGHT)
        .bottom(px(0.0))
        .right(px(-width * (1.0 - shown)))
        .w(px(width))
        .flex()
        .flex_col()
        .bg(ws.ui.surface)
        .text_color(hsla(ws.theme.palette.foreground, 1.0))
        .border_l_1()
        .border_color(ui.hairline)
        .shadow(vec![BoxShadow {
            color: ui.shadow,
            offset: point(px(-8.0), px(0.0)),
            blur_radius: px(32.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .track_focus(&ws.settings_focus)
        .on_key_down(cx.listener(Workspace::on_settings_key))
        // A press anywhere in the panel stops recording a shortcut (a row's click, on release,
        // starts it again).
        .capture_any_mouse_down(cx.listener(|ws, _: &MouseDownEvent, _, cx| {
            if ws.recording.take().is_some() {
                ws.key_note = None;
                cx.notify();
            }
        }))
        // Nothing under the panel (blocks, the canvas) gets its presses or its wheel.
        .occlude()
        .child(header)
        .child(tabs)
        .child(content)
        .child(footer)
        .test_support()
        .into_any_element()
}

fn tab(ui: Ui, section: Section, selected: bool, cx: &mut Context<Workspace>) -> AnyElement {
    div()
        .id(section.tab_id())
        .px(px(12.0))
        .py(px(4.0))
        .rounded_full()
        .border_1()
        .text_size(px(12.0))
        .cursor_pointer()
        .map(|el| {
            if selected {
                el.bg(ui.hover)
                    .border_color(ui.hairline)
                    .font_weight(FontWeight::MEDIUM)
            } else {
                el.border_color(gpui_kit::transparent_black())
                    .text_color(ui.title)
                    .hover(move |s| s.bg(ui.hover))
            }
        })
        .active(move |s| s.bg(ui.pressed))
        .child(section.title())
        .on_click(cx.listener(move |ws, _: &ClickEvent, _, cx| ws.set_section(section, cx)))
        .test_support()
        .into_any_element()
}

/// The section's rows, read from the workspace as it is now.
fn rows(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    section: Section,
    window: &mut Window,
    cx: &mut App,
) -> Vec<Row> {
    match section {
        Section::Appearance => {
            let names: Vec<SharedString> = ws
                .themes
                .iter()
                .map(|e| SharedString::from(e.theme.name.clone()))
                .collect();
            vec![
                choice_row(ws, entity, &MODE, "Mode")
                    .description("System follows the desktop's light or dark setting"),
                theme_row(ws, entity, names.clone(), false),
                theme_row(ws, entity, names, true),
            ]
        }
        Section::Font => vec![
            font_family_row(ws, entity),
            number_row(ws, entity, &FONT_SIZE, "Size", window, cx)
                .description("Points at 100% zoom"),
            Row::new("font.preview", "Preview").below(preview(ws)),
        ],
        Section::Terminal => {
            let blink = ws.config.cursor.blink;
            let notifications = ws.config.terminal.notifications;
            let e = entity.clone();
            let n = entity.clone();
            vec![
                choice_row(ws, entity, &CURSOR, "Cursor").description("Block, bar or underline"),
                Row::new("cursor.blink", "Blink")
                    .description("The focused terminal's cursor blinks")
                    .control(Switch::new("blink-switch").checked(blink).on_click(
                        move |on: &bool, _, cx| {
                            let on = *on;
                            e.update(cx, |ws, cx| {
                                ws.edit(
                                    Edit::set("cursor", "blink", on),
                                    |c| c.cursor.blink = on,
                                    cx,
                                )
                            })
                        },
                    ))
                    .reset(
                        blink,
                        remove(entity, "cursor", "blink", |c| c.cursor.blink = false),
                    ),
                number_row(ws, entity, &SCROLLBACK, "Scrollback", window, cx)
                    .description("Lines of history per terminal"),
                Row::new("terminal.notifications", "System notifications")
                    .description(
                        "When Templum is in the background and a long command ends or a terminal \
                         asks for attention",
                    )
                    .control(
                        Switch::new("notifications-switch")
                            .checked(notifications)
                            .on_click(move |on: &bool, _, cx| {
                                let on = *on;
                                n.update(cx, |ws, cx| {
                                    ws.edit(
                                        Edit::set("terminal", "notifications", on),
                                        |c| c.terminal.notifications = on,
                                        cx,
                                    )
                                })
                            }),
                    )
                    .reset(
                        !notifications,
                        remove(entity, "terminal", "notifications", |c| {
                            c.terminal.notifications = true
                        }),
                    ),
            ]
        }
        Section::Shell => {
            let shell = &ws.config.shell;
            let integration = shell.integration;
            let e = entity.clone();
            let mut rows = vec![
                shell_row(ws, entity),
                Row::new("shell.program", "Program")
                    .description("Empty: the default shell, above. Set by the list, or typed")
                    .control(text_input(
                        entity,
                        "shell.program",
                        program_text(ws),
                        set_program,
                        window,
                        cx,
                    ))
                    .reset(
                        shell.program.is_some(),
                        remove(entity, "shell", "program", |c| c.shell.program = None),
                    ),
                Row::new("shell.args", "Arguments")
                    .description(
                        args_error(ws)
                            .unwrap_or("Separated by spaces; \"…\" or '…' keeps spaces inside one"),
                    )
                    .control(text_input(
                        entity,
                        "shell.args",
                        args_text(ws),
                        set_args,
                        window,
                        cx,
                    ))
                    .reset(
                        !shell.args.is_empty(),
                        remove(entity, "shell", "args", |c| c.shell.args.clear()),
                    ),
                folder_kind_row(ws, entity),
            ];
            if matches!(shell.working_directory, WorkingDirectory::Path(_)) {
                rows.push(
                    Row::new("shell.folder", "Folder")
                        .description(folder_hint(ws).unwrap_or("A path; ~/ is the home folder"))
                        .control(text_input(
                            entity,
                            "shell.folder",
                            folder_text(ws),
                            set_folder,
                            window,
                            cx,
                        )),
                );
            }
            rows.push(
                Row::new("shell.integration", "Shell integration")
                    .description(
                        "bash, zsh, fish and PowerShell tell Templum their folder, and when \
                         commands end and how",
                    )
                    .control(
                        Switch::new("integration-switch")
                            .checked(integration)
                            .on_click(move |on: &bool, _, cx| {
                                let on = *on;
                                e.update(cx, |ws, cx| {
                                    ws.edit(
                                        Edit::set("shell", "integration", on),
                                        |c| c.shell.integration = on,
                                        cx,
                                    )
                                })
                            }),
                    )
                    .reset(
                        !integration,
                        remove(entity, "shell", "integration", |c| {
                            c.shell.integration = true
                        }),
                    ),
            );
            rows
        }
        Section::Tools => {
            let program = ws.config.git.program.clone();
            let changed = ws.config.git.program() != "git";
            let version = match &ws.git_version {
                Some(Ok(v)) => v.clone(),
                Some(Err(e)) => format!("Not found: {e}"),
                None => "Looking…".to_string(),
            };
            vec![
                Row::new("git.program", "Git")
                    .description(version)
                    .control(text_input(
                        entity,
                        "git.program",
                        program.clone(),
                        set_git_program,
                        window,
                        cx,
                    ))
                    .reset(
                        changed,
                        remove(entity, "git", "program", |c| {
                            c.git.program = "git".to_string()
                        }),
                    ),
            ]
        }
        Section::Agents => ws
            .agents
            .iter()
            .map(|agent| agent_row(ws, entity, agent))
            .collect(),
        Section::Shortcuts => Command::ALL
            .map(|command| shortcut_row(ws, entity, command))
            .into_iter()
            .collect(),
    }
}

fn choice_row(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    choice: &'static Choice,
    title: &'static str,
) -> Row {
    let current = (choice.get)(&ws.config);
    let default = (choice.get)(&Config::default());
    let options = choice
        .options
        .iter()
        .map(|(value, label)| (SharedString::from(*value), SharedString::from(*label)))
        .collect();
    let e = entity.clone();
    Row::new(format!("{}.{}", choice.table, choice.key), title)
        .control(menu_button(
            ElementId::Name(format!("menu-{}.{}", choice.table, choice.key).into()),
            options,
            current.into(),
            false,
            move |value, cx| {
                e.update(cx, |ws, cx| {
                    ws.edit(
                        Edit::set(choice.table, choice.key, (choice.toml)(&value)),
                        |c| (choice.set)(c, &value),
                        cx,
                    )
                })
            },
        ))
        .reset(
            current != default,
            remove(entity, choice.table, choice.key, move |c| {
                (choice.set)(c, default)
            }),
        )
}

/// `theme.light` or `theme.dark`: the built-in themes and the user's themes folder.
fn theme_row(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    names: Vec<SharedString>,
    dark: bool,
) -> Row {
    let key = if dark { "dark" } else { "light" };
    let current = ws.config.theme.name(dark).to_string();
    let default = ThemeConfig::default().name(dark).to_string();
    let e = entity.clone();
    Row::new(
        format!("theme.{key}"),
        if dark { "Dark theme" } else { "Light theme" },
    )
    .description(if dark {
        "Used in dark mode"
    } else {
        "Used in light mode"
    })
    .control(menu_button(
        ElementId::Name(format!("menu-theme.{key}").into()),
        names.into_iter().map(|n| (n.clone(), n)).collect(),
        current.clone().into(),
        true,
        move |name, cx| {
            e.update(cx, |ws, cx| {
                ws.edit(
                    Edit::set("theme", key, name.to_string()),
                    |c| set_theme(c, dark, name.to_string()),
                    cx,
                )
            })
        },
    ))
    .reset(
        current != default,
        remove(entity, "theme", key, move |c| {
            set_theme(c, dark, default.clone())
        }),
    )
}

fn font_family_row(ws: &Workspace, entity: &Entity<Workspace>) -> Row {
    let options = std::iter::once((
        SharedString::default(),
        SharedString::from("Default (built-in list)"),
    ))
    .chain(ws.fonts.iter().map(|f| (f.clone(), f.clone())))
    .collect();
    let current = ws.config.font.family.clone().unwrap_or_default();
    let e = entity.clone();
    Row::new("font.family", "Family")
        .description("Installed monospaced fonts")
        .control(menu_button(
            "menu-font.family",
            options,
            current.into(),
            true,
            move |family, cx| e.update(cx, |ws, cx| set_family(ws, &family, cx)),
        ))
        .reset(
            ws.config.font.family.is_some(),
            remove(entity, "font", "family", |c| c.font.family = None),
        )
}

fn folder_kind_row(ws: &Workspace, entity: &Entity<Workspace>) -> Row {
    let current = match ws.config.shell.working_directory {
        WorkingDirectory::Inherit => "inherit",
        WorkingDirectory::Home => "home",
        WorkingDirectory::Path(_) => "folder",
    };
    let e = entity.clone();
    Row::new("shell.working_directory", "Start folder")
        .description("Where a new terminal starts")
        .control(menu_button(
            "menu-shell.working_directory",
            vec![
                ("inherit".into(), "Inherit from the focused block".into()),
                ("home".into(), "Home".into()),
                ("folder".into(), "A folder…".into()),
            ],
            current.into(),
            false,
            move |kind, cx| e.update(cx, |ws, cx| set_folder_kind(ws, &kind, cx)),
        ))
        .reset(
            current != "inherit",
            remove(entity, "shell", "working_directory", |c| {
                c.shell.working_directory = WorkingDirectory::Inherit
            }),
        )
}

fn number_row(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    n: &'static Number,
    title: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Row {
    let value = (n.get)(&ws.config);
    let default = (n.get)(&Config::default());
    let e = entity.clone();
    let key = SharedString::from(format!("number-{}.{}", n.table, n.key));
    let state = window.use_keyed_state(key, cx, move |window, cx| {
        NumberState::new(e, n, value, window, cx)
    });
    // A hand edit of the file, or a reset, replaces the text.
    state.update(cx, |state, cx| {
        if state.shown != value {
            state.shown = value;
            state.input.update(cx, |input, cx| {
                input.set_value(value.to_string(), window, cx)
            });
        }
    });
    Row::new(format!("{}.{}", n.table, n.key), title)
        .control(NumberInput::new(&state.read(cx).input).small().w_32())
        .reset(
            value != default,
            remove(entity, n.table, n.key, move |c| (n.set)(c, default)),
        )
}

/// One command's shortcuts as keys; a click starts recording. Resetting removes
/// `keys.<command>` (the OS defaults again).
fn shortcut_row(ws: &Workspace, entity: &Entity<Workspace>, command: Command) -> Row {
    let recording = ws.recording == Some(command);
    let strokes = ws.keymap.keystrokes(command);
    let ui = ws.ui;
    let e = entity.clone();
    let control = div()
        .id(ElementId::Name(
            format!("shortcut-{}", command.name()).into(),
        ))
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .py(px(3.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(if recording {
            ui.accent
        } else {
            gpui_kit::transparent_black()
        })
        .text_size(px(12.0))
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .map(|el| {
            if recording {
                el.child("Press a shortcut…")
            } else if strokes.is_empty() {
                el.text_color(ui.title_dim).child("None")
            } else {
                el.children(strokes.iter().cloned().map(|s| chip(Kbd::new(s), ui)))
            }
        })
        .on_click(move |_: &ClickEvent, window, cx| {
            e.update(cx, |ws, cx| ws.record(command, window, cx))
        })
        .test_support();
    let description = match &ws.key_note {
        Some(note) if recording => Some(SharedString::from(note.clone())),
        _ if recording => Some("Esc cancels; Backspace leaves it without a shortcut".into()),
        _ => None,
    };
    let e = entity.clone();
    let row = Row::new(format!("keys.{}", command.name()), command.title())
        .control(control)
        .reset(ws.config.keys.get(command).is_some(), move |window, cx| {
            e.update(cx, |ws, cx| ws.reset_shortcut(command, window, cx))
        });
    match description {
        Some(text) => row.description(text),
        None => row,
    }
}

/// A few lines of terminal output in the chosen font, size and theme.
fn preview(ws: &Workspace) -> impl IntoElement {
    let family = ws.cell.font.family.clone();
    let size = ws.config.font.size;
    let p = ws.theme.palette;
    let line = |parts: &[(&str, templum_terminal::Rgb)]| {
        div().flex().children(
            parts
                .iter()
                .map(|(text, color)| div().text_color(hsla(*color, 1.0)).child(text.to_string())),
        )
    };
    div()
        .w_full()
        .p(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(ws.ui.hairline)
        .bg(hsla(p.background, 1.0))
        .font_family(family)
        .text_size(px(size))
        .child(line(&[("~/templum", p.ansi[4])]))
        .child(line(&[("❯ ", p.ansi[2]), ("cargo test", p.foreground)]))
        .child(line(&[
            ("test result: ", p.foreground),
            ("ok", p.ansi[2]),
            (". 135 passed", p.foreground),
        ]))
}

/// The shell new terminals run: the default, or one found on this computer. One set by hand that
/// isn't in the list shows as Custom.
fn shell_row(ws: &Workspace, entity: &Entity<Workspace>) -> Row {
    let shell = &ws.config.shell;
    let current = match &shell.program {
        None => "default".to_string(),
        Some(program) => ws
            .shells
            .iter()
            .position(|s| s.program == *program && s.args == shell.args)
            .map_or_else(|| "custom".to_string(), |i| i.to_string()),
    };
    let mut options: Vec<(SharedString, SharedString)> = vec![(
        "default".into(),
        format!("Default ({})", ws.default_shell).into(),
    )];
    options.extend(
        ws.shells
            .iter()
            .enumerate()
            .map(|(i, s)| (i.to_string().into(), s.label.clone().into())),
    );
    if current == "custom" {
        options.push(("custom".into(), "Custom".into()));
    }
    let (e, shells) = (entity.clone(), ws.shells.clone());
    let reset = entity.clone();
    Row::new("shell.shell", "Shell")
        .description("What new terminals run")
        .control(menu_button(
            "menu-shell.shell",
            options,
            current.into(),
            false,
            move |value, cx| {
                let chosen = value.parse::<usize>().ok().and_then(|i| shells.get(i));
                if chosen.is_none() && value != "default" {
                    return;
                }
                e.update(cx, |ws, cx| set_shell(ws, chosen, cx))
            },
        ))
        .reset(shell.program.is_some(), move |_, cx| {
            reset.update(cx, |ws, cx| set_shell(ws, None, cx))
        })
}

fn set_theme(c: &mut Config, dark: bool, name: String) {
    if dark {
        c.theme.dark = name;
    } else {
        c.theme.light = name;
    }
}
