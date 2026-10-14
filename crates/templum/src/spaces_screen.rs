//! The workspaces screen: every workspace as a card, found by name, opened with a click or
//! Enter. See docs/specs/cockpit.md#6-the-workspaces-screen.

use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, TITLE_BAR_HEIGHT};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, BoxShadow, ClickEvent, Context, ElementId, Entity, FocusHandle, FontWeight,
    Hsla, KeyDownEvent, MouseButton, MouseDownEvent, SharedString, TestSupportExt, Window, div,
    point, px,
};

use crate::grid::hsla;
use crate::keys::Command;
use crate::menu::Target;
use crate::motion;
use crate::sidebar;
use crate::theme::Ui;
use crate::workspace::{Body, Space, Workspace, workspace_digit};

/// The sheet's widest; narrower windows get a narrower one.
const SHEET_MAX: f32 = 920.0;
/// Below this sheet width, three columns of cards instead of four.
const NARROW: f32 = 760.0;
const CARD_H: f32 = 140.0;
const GAP: f32 = 16.0;

/// The open screen, or the closed one still sliding out.
pub(crate) struct Screen {
    pub since: Instant,
    pub closing: bool,
    pub search: Entity<InputState>,
    /// The card the arrows moved to, among those shown; None: the first, until an arrow.
    pub highlight: Option<usize>,
    /// What had the keyboard before: it gets it back.
    pub before: Option<FocusHandle>,
}

impl Screen {
    /// How much of the sheet is up at `now`: 0 hidden, 1 all.
    pub fn shown(&self, now: Instant) -> f32 {
        motion::slid(self.since, self.closing, now)
    }

    /// The screen sliding the other way, from where it is.
    pub fn reversed(self, now: Instant) -> Screen {
        Screen {
            since: motion::turned(self.since, self.closing, now),
            closing: !self.closing,
            ..self
        }
    }
}

/// What a workspace holds, as its card tells it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Counts {
    pub terminals: usize,
    pub notes: usize,
    pub boards: usize,
    pub editors: usize,
    pub gits: usize,
    /// The agents running, by name, with how many.
    pub agents: Vec<(String, usize)>,
    /// Its programs were ended: the counts are its saved canvas's.
    pub stopped: bool,
}

impl Counts {
    /// "3 terminals · 1 note · 1 Claude Code running"; "Empty" for nothing.
    pub fn line(&self) -> String {
        let count = |n: usize, one: &str, many: &str| match n {
            0 => None,
            1 => Some(format!("1 {one}")),
            n => Some(format!("{n} {many}")),
        };
        let parts: Vec<String> = [
            count(self.terminals, "terminal", "terminals"),
            count(self.notes, "note", "notes"),
            count(self.boards, "board", "boards"),
            count(self.editors, "editor", "editors"),
            count(self.gits, "Git block", "Git blocks"),
        ]
        .into_iter()
        .flatten()
        .chain(
            self.agents
                .iter()
                .map(|(agent, n)| format!("{n} {agent} running")),
        )
        .collect();
        let line = if parts.is_empty() {
            "Empty".to_string()
        } else {
            parts.join(" · ")
        };
        if self.stopped {
            format!("Stopped · {line}")
        } else {
            line
        }
    }

    fn agent(&mut self, name: &str) {
        match self.agents.iter_mut().find(|(a, _)| a == name) {
            Some((_, n)) => *n += 1,
            None => self.agents.push((name.to_string(), 1)),
        }
    }
}

/// A workspace being dragged by its card or its sidebar tile: dropped on another, it takes
/// that one's place. It draws itself under the pointer.
#[derive(Clone)]
pub(crate) struct SpaceDrag {
    pub id: String,
    pub name: SharedString,
    pub ui: Ui,
}

impl Render for SpaceDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let ui = self.ui;
        div()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(8.0))
            .bg(ui.surface)
            .border_1()
            .border_color(ui.accent)
            .shadow(vec![BoxShadow {
                color: ui.shadow,
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(20.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .text_size(px(13.0))
            .text_color(ui.title)
            .opacity(0.92)
            .child(self.name.clone())
    }
}

/// Lets `el`, workspace `space`'s card or tile at position `at`, be dragged and dropped on.
pub(crate) fn draggable(
    el: gpui_kit::Stateful<gpui_kit::Div>,
    space: &Space,
    at: usize,
    ui: Ui,
    cx: &mut Context<Workspace>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let drag = SpaceDrag {
        id: space.id.clone(),
        name: space.name.clone().into(),
        ui,
    };
    el.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
        .drag_over::<SpaceDrag>(move |s, _, _, _| s.border_color(ui.accent))
        .on_drop(cx.listener(move |ws, drag: &SpaceDrag, window, cx| {
            ws.move_space(&drag.id, at, window, cx);
            cx.stop_propagation();
        }))
}

/// Columns of cards in a sheet `width` wide.
fn columns(width: f32) -> usize {
    match width {
        w if w >= NARROW => 4,
        w if w >= 520.0 => 3,
        w if w >= 340.0 => 2,
        _ => 1,
    }
}

/// The sheet's width in a window `width` wide.
fn sheet_width(width: f32) -> f32 {
    SHEET_MAX.min(width - 32.0).max(0.0)
}

impl Workspace {
    /// Opens the screen, its search field taking the keyboard, or closes it.
    pub(crate) fn toggle_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        match self.screen.take() {
            Some(open) if !open.closing => {
                self.screen = Some(open);
                self.close_screen(window, cx);
                return;
            }
            // Still sliding out: it turns back from where it is.
            Some(closing) => self.screen = Some(closing.reversed(now)),
            None => {
                let search =
                    cx.new(|cx| InputState::new(window, cx).placeholder("Find a workspace"));
                let ws = cx.entity();
                App::subscribe(cx, &search, move |_, event: &InputEvent, cx| {
                    if let InputEvent::Change = event {
                        ws.update(cx, |ws, cx| {
                            if let Some(screen) = &mut ws.screen {
                                screen.highlight = None;
                            }
                            cx.notify();
                        });
                    }
                })
                .detach();
                self.screen = Some(Screen {
                    since: now,
                    closing: false,
                    search,
                    highlight: None,
                    before: window.focused(cx),
                });
            }
        }
        self.menu = None;
        if let Some(screen) = &self.screen {
            screen.search.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    /// Slides the screen out, and gives the keyboard back to what had it.
    pub(crate) fn close_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.screen.as_ref().is_some_and(|s| !s.closing) {
            return;
        }
        let Some(open) = self.screen.take() else {
            return;
        };
        let before = open.before.clone();
        self.screen = Some(open.reversed(Instant::now()));
        window.focus(before.as_ref().unwrap_or(&self.focus_handle), cx);
        cx.notify();
    }

    /// The cards shown: the workspaces whose name has the search's text, by their position.
    fn screen_cards(&self, cx: &App) -> Vec<usize> {
        let query = self
            .screen
            .as_ref()
            .map(|s| s.search.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        self.order()
            .iter()
            .enumerate()
            .filter(|(_, s)| s.name.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect()
    }

    /// Shows the workspace at `at` (its position) and closes the screen.
    fn open_card(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.close_screen(window, cx);
        self.switch_to(at, window, cx);
    }

    /// Keys while the search field has them (and left them): Esc closes, Up and Down move the
    /// highlight a row, Enter opens it, Ctrl+digit jumps; the workspaces' and the window's
    /// shortcuts run. Anything else is typing.
    pub(crate) fn on_screen_key(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let k = &e.keystroke;
        let plain = !k.modifiers.modified();
        let cards = self.screen_cards(cx);
        let width = sheet_width(f32::from(window.viewport_size().width));
        let step = columns(width);
        match k.key.as_str() {
            "escape" if plain => self.close_screen(window, cx),
            "up" | "down" if plain => {
                if let Some(screen) = &mut self.screen
                    && !cards.is_empty()
                {
                    let at = screen.highlight.unwrap_or(0);
                    screen.highlight = Some(if k.key == "up" {
                        at.saturating_sub(step)
                    } else {
                        (at + step).min(cards.len() - 1)
                    });
                    cx.notify();
                }
            }
            "enter" if plain => {
                let at = self.screen.as_ref().and_then(|s| s.highlight).unwrap_or(0);
                if let Some(&card) = cards.get(at) {
                    self.open_card(card, window, cx);
                }
            }
            _ => {
                if let Some(to) = workspace_digit(k, cfg!(target_os = "macos"))
                    && to <= self.others.len()
                {
                    self.open_card(to, window, cx);
                } else if let Some(command) = self.keymap.command(k).filter(|c| {
                    matches!(
                        c,
                        Command::Workspaces
                            | Command::NewWorkspace
                            | Command::NextWorkspace
                            | Command::PreviousWorkspace
                            | Command::ToggleSidebar
                            | Command::Settings
                            | Command::Quit
                    )
                }) {
                    self.run(command, window, cx);
                } else {
                    return;
                }
            }
        }
        cx.stop_propagation();
    }

    /// What `space` holds: its blocks by kind and the agents running in its terminals.
    pub(crate) fn counts(&self, space: &Space) -> Counts {
        let mut counts = Counts {
            stopped: space.stopped,
            ..Counts::default()
        };
        if space.stopped {
            use crate::session::Item;
            for item in space.saved.iter().flat_map(|s| &s.items) {
                match item {
                    Item::Terminal(_) => counts.terminals += 1,
                    Item::Note(_) => counts.notes += 1,
                    Item::Kanban(_) => counts.boards += 1,
                    Item::Editor(_) => counts.editors += 1,
                    Item::Git(_) => counts.gits += 1,
                }
            }
            return counts;
        }
        for body in space.blocks.values() {
            match body {
                Body::Terminal(pane) => {
                    counts.terminals += 1;
                    if let Some(agent) = self.agent_name(pane) {
                        counts.agent(agent);
                    }
                }
                Body::Note(_) => counts.notes += 1,
                Body::Kanban(_) => counts.boards += 1,
                Body::Editor(_) => counts.editors += 1,
                Body::Git(_) => counts.gits += 1,
            }
        }
        counts
    }
}

pub(crate) fn render(
    ws: &Workspace,
    screen: &Screen,
    now: Instant,
    window: &Window,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let shown = screen.shown(now);
    let size = window.viewport_size();
    let (w, h) = (f32::from(size.width), f32::from(size.height));
    let width = sheet_width(w);
    let cols = columns(width);
    let cards = ws.screen_cards(cx);
    let query = screen.search.read(cx).value().to_string();
    // Enter opens the first card until an arrow moves on: show which, once there's a choice.
    let highlight = screen.highlight.or((!query.trim().is_empty()).then_some(0));
    let order = ws.order();
    let fg = hsla(ws.theme.palette.foreground, 1.0);
    let header = div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fg)
                .child("Workspaces"),
        )
        .child(div().w(px(240.0)).child(Input::new(&screen.search)))
        .child(div().flex_1())
        .child(
            header_button("screen-new", ui)
                .gap(px(6.0))
                .child(Icon::new(IconName::Plus).size(px(14.0)))
                .child("New workspace")
                .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| ws.add_space(window, cx)))
                .test_support(),
        )
        .child(
            header_button("screen-close", ui)
                .child(Icon::new(IconName::X).size(px(14.0)))
                .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| ws.close_screen(window, cx)))
                .test_support(),
        );
    let grid = div()
        .id("screen-cards")
        .grid()
        .grid_cols(cols as u16)
        .gap(px(GAP))
        .children(
            cards
                .iter()
                .enumerate()
                .map(|(n, &at)| card(ws, order[at], at, highlight == Some(n), cx)),
        )
        .when(cards.is_empty(), |el| {
            el.child(
                div()
                    .text_size(px(13.0))
                    .text_color(ui.title_dim)
                    .child("No workspace has that name"),
            )
        });
    let sheet = div()
        .id("screen-sheet")
        .absolute()
        .left(px((w - width) / 2.0))
        // It comes up from below the window.
        .top(px(24.0 + (1.0 - shown) * h))
        .w(px(width))
        .max_h(px((h - f32::from(TITLE_BAR_HEIGHT) - 48.0).max(0.0)))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .p(px(24.0))
        .bg(ui.surface)
        .text_color(fg)
        .rounded(px(10.0))
        .border_1()
        .border_color(ui.hairline)
        .shadow(vec![BoxShadow {
            color: ui.shadow,
            offset: point(px(0.0), px(10.0)),
            blur_radius: px(30.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .overflow_y_scroll()
        // Presses inside the sheet don't reach the scrim.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_key_down(cx.listener(Workspace::on_screen_key))
        .child(header)
        .child(grid)
        .test_support();
    div()
        .id("screen")
        .absolute()
        .top(TITLE_BAR_HEIGHT)
        .left_0()
        .right_0()
        .bottom_0()
        .occlude()
        .bg(ui.canvas.opacity(0.6 * shown))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                ws.close_screen(window, cx);
                cx.stop_propagation();
            }),
        )
        .child(sheet)
        .test_support()
        .into_any_element()
}

fn header_button(id: &'static str, ui: Ui) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(28.0))
        .min_w(px(28.0))
        .px(px(8.0))
        .rounded(px(8.0))
        .text_size(px(13.0))
        .text_color(ui.title)
        .cursor_pointer()
        .hover(move |s| s.bg(ui.hover))
        .active(move |s| s.bg(ui.pressed))
}

/// One workspace's card: its tile, its name and what it holds; the active one ringed.
fn card(
    ws: &Workspace,
    space: &Space,
    at: usize,
    highlighted: bool,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let active = at == ws.active;
    let id = space.id.clone();
    let shortcut = (at < 9).then(|| {
        let key = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        SharedString::from(format!("{} · {key}+{}", space.name, at + 1))
    });
    let dot = |color: Hsla| div().size(px(7.0)).rounded_full().bg(color);
    let agent =
        space.has_agent(&ws.agents) || space.terminals().any(|(_, p)| p.seen.agent.is_some());
    let news = space.terminals().any(|(_, p)| p.alert.is_some());
    let el = div().id(ElementId::Name(format!("space-card-{}", space.id).into()));
    draggable(el, space, at, ui, cx)
        .h(px(CARD_H))
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .bg(ui.card)
        .border_1()
        .cursor_pointer()
        .map(|el| {
            if active {
                el.border_color(ui.accent).shadow(vec![BoxShadow {
                    color: ui.ring,
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(2.0),
                    inset: false,
                }])
            } else if highlighted {
                el.border_color(ui.hover_border).bg(ui.hover)
            } else {
                el.border_color(ui.hairline)
                    .hover(move |s| s.border_color(ui.hover_border))
            }
        })
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .child(sidebar::tile_face(&space.look, &space.name, &ws.theme, ui))
                .child(
                    div()
                        .flex()
                        .gap(px(4.0))
                        .when(agent, |el| {
                            el.child(dot(hsla(ws.theme.palette.ansi[2], 1.0)))
                        })
                        .when(news, |el| {
                            el.child(dot(hsla(ws.theme.palette.ansi[3], 1.0)))
                        }),
                ),
        )
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(space.name.clone()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(ui.title_dim)
                .line_clamp(2)
                .child(ws.counts(space).line()),
        )
        .when_some(shortcut, |el, tip| {
            el.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        })
        .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| ws.open_card(at, window, cx)))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                ws.open_menu(Target::Space(id.clone()), e.position, window, cx);
                cx.stop_propagation();
            }),
        )
        .test_support()
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_counts_blocks_by_kind_and_running_agents() {
        let mut counts = Counts {
            terminals: 3,
            notes: 1,
            ..Counts::default()
        };
        counts.agent("Claude Code");
        assert_eq!(
            counts.line(),
            "3 terminals · 1 note · 1 Claude Code running"
        );
        counts.agent("Claude Code");
        counts.agent("Codex");
        counts.boards = 2;
        counts.gits = 1;
        assert_eq!(
            counts.line(),
            "3 terminals · 1 note · 2 boards · 1 Git block · 2 Claude Code running · 1 Codex running"
        );
        assert_eq!(Counts::default().line(), "Empty");
        let stopped = Counts {
            terminals: 2,
            stopped: true,
            ..Counts::default()
        };
        assert_eq!(stopped.line(), "Stopped · 2 terminals");
    }

    #[test]
    fn narrow_sheets_get_fewer_columns() {
        assert_eq!(columns(sheet_width(1400.0)), 4);
        assert_eq!(columns(sheet_width(700.0)), 3);
        assert_eq!(columns(400.0), 2);
    }
}
