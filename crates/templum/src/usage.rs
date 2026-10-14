//! Agents' session usage and plan limits, as Templum shows them: the title bar's meter, its
//! popover, a terminal's chip (docs/specs/usage.md).

use chrono::{DateTime, TimeZone};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, TITLE_BAR_HEIGHT};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent,
    TestSupportExt, div, point, px,
};
use templum_usage::{REACHED, Session, Window};

use crate::grid::hsla;
use crate::notes;
use crate::title_bar;
use crate::workspace::Workspace;

/// The title bar's meter: the known window closest to its limit. None when no limit is known.
pub(crate) fn meter(ws: &Workspace, cx: &mut Context<Workspace>) -> Option<AnyElement> {
    let now = notes::now();
    let (agent, w) = ws
        .limits
        .iter()
        .flat_map(|(agent, l)| {
            l.windows
                .iter()
                .filter(move |w| w.resets_at > now)
                .map(move |w| (agent, w))
        })
        .max_by(|a, b| a.1.used_percent.total_cmp(&b.1.used_percent))?;
    let ui = ws.ui;
    let title = ws.agent_title(agent);
    // "Claude Code" is "Claude" here: the title bar is narrow.
    let name = title.split_whitespace().next().unwrap_or(&title);
    Some(
        title_bar::button("usage-meter", ui)
            .gap(px(6.0))
            .child(
                Icon::new(IconName::Gauge)
                    .size(px(14.0))
                    .text_color(ui.title),
            )
            .child(format!(
                "{name} {} {:.0}%",
                window_label(w.minutes),
                w.used_percent
            ))
            .child(bar(
                w.used_percent,
                32.0,
                level(ws, w.used_percent),
                ui.hairline,
            ))
            .on_click(cx.listener(|ws, _: &ClickEvent, _, cx| ws.toggle_usage(cx)))
            .test_support()
            .into_any_element(),
    )
}

/// The usage popover under the meter: each agent's limits, then the sessions open, which show
/// their terminals.
pub(crate) fn popover(ws: &Workspace, cx: &mut Context<Workspace>) -> AnyElement {
    let now = notes::now();
    let ui = ws.ui;
    let foreground = hsla(ws.theme.palette.foreground, 1.0);
    let heading = |text: &'static str| {
        div()
            .pt(px(4.0))
            .text_size(px(11.0))
            .text_color(ui.title_dim)
            .child(text)
    };
    let mut list = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .p(px(12.0))
        .child(heading("Plan limits"));
    let mut any = false;
    for (agent, limits) in &ws.limits {
        let windows: Vec<&Window> = limits
            .windows
            .iter()
            .filter(|w| w.resets_at > now)
            .collect();
        if windows.is_empty() {
            continue;
        }
        any = true;
        list = list.child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .child(ws.agent_title(agent)),
        );
        for w in windows {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .child(
                        div()
                            .w(px(52.0))
                            .text_color(ui.title_dim)
                            .child(capitalized(&window_name(w.minutes))),
                    )
                    .child(bar(
                        w.used_percent,
                        88.0,
                        level(ws, w.used_percent),
                        ui.hairline,
                    ))
                    .child(div().w(px(34.0)).child(format!("{:.0}%", w.used_percent)))
                    .child(
                        div()
                            .text_color(ui.title_dim)
                            .child(format!("resets {}", resets_local(w.resets_at, now))),
                    ),
            );
        }
    }
    if !any {
        list = list.child(
            div()
                .text_size(px(12.0))
                .text_color(ui.title_dim)
                .child("None known yet"),
        );
    }
    let mut sessions = Vec::new();
    for space in ws.spaces() {
        let mut found: Vec<_> = space
            .terminals()
            .filter_map(|(id, pane)| {
                let (text, _) = chip(pane.usage.as_ref()?)?;
                let agent = ws.agent_title(pane.seen.agent.as_deref()?);
                let folder = pane
                    .seen
                    .folder
                    .as_ref()
                    .and_then(|f| f.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Some((id, format!("{agent} · {folder}"), text))
            })
            .collect();
        found.sort_by_key(|(id, ..)| *id);
        sessions.extend(
            found
                .into_iter()
                .map(|(id, name, text)| (space.id.clone(), id, name, text)),
        );
    }
    if !sessions.is_empty() {
        list = list.child(heading("Open sessions"));
    }
    for (i, (space, block, name, text)) in sessions.into_iter().enumerate() {
        list = list.child(
            div()
                .id(("usage-session", i))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .h(px(26.0))
                .px(px(6.0))
                .mx(px(-6.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child(div().truncate().child(name))
                .child(div().flex_shrink_0().text_color(ui.title_dim).child(text))
                .on_click(cx.listener(move |ws, _: &ClickEvent, window, cx| {
                    ws.show_session(&space, block, window, cx)
                }))
                .test_support(),
        );
    }
    let panel = div()
        .id("usage-popover")
        .absolute()
        .top(TITLE_BAR_HEIGHT + px(4.0))
        .right(px(8.0))
        .w(px(340.0))
        .bg(ui.surface)
        .text_color(foreground)
        .border_1()
        .border_color(ui.hairline)
        .rounded(px(10.0))
        .shadow(vec![BoxShadow {
            color: ui.shadow,
            offset: point(px(0.0), px(12.0)),
            blur_radius: px(32.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        // Presses inside the panel don't reach the backdrop.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(list)
        .test_support();
    // A press anywhere outside the panel closes it, and nothing below gets it.
    div()
        .id("usage-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|ws, _: &MouseDownEvent, _, cx| {
                ws.toggle_usage(cx);
                cx.stop_propagation();
            }),
        )
        .child(panel)
        .into_any_element()
}

/// A terminal's chip: its text ("42% · $3.10") and its tooltip. None when nothing is known.
pub(crate) fn chip(s: &Session) -> Option<(String, String)> {
    let context = s
        .context_percent
        .map(|p| format!("{p:.0}%"))
        .or_else(|| s.context_tokens.map(|t| format!("{} ctx", tokens(t))));
    let spent = s
        .cost_usd
        .map(|c| format!("${c:.2}"))
        .or_else(|| s.tokens.map(tokens));
    let text = match (context, spent) {
        (Some(c), Some(s)) => format!("{c} · {s}"),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => return None,
    };
    let mut tip = Vec::new();
    if let Some(model) = &s.model {
        tip.push(model.clone());
    }
    match (s.context_percent, s.context_tokens) {
        (Some(p), _) => tip.push(format!("Context: {p:.0}% used")),
        (None, Some(t)) => tip.push(format!("Context: {} tokens", tokens(t))),
        _ => {}
    }
    if let Some(t) = s.tokens {
        tip.push(format!("Tokens: {}", tokens(t)));
    }
    if let Some(c) = s.cost_usd {
        tip.push(format!("Cost: ${c:.2} at API prices"));
    }
    Some((text, tip.join("\n")))
}

/// A token count, short: 950, 84k, 1.2M.
fn tokens(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1000 => format!("{}k", n / 1000),
        n => n.to_string(),
    }
}

/// How close to its limit a window is, as a color: red when reached, yellow from the warning
/// line, the accent otherwise.
fn level(ws: &Workspace, used: f32) -> Hsla {
    let warn = ws.config.usage.warn;
    if used >= REACHED {
        hsla(ws.theme.palette.ansi[1], 1.0)
    } else if warn > 0 && used >= f32::from(warn) {
        hsla(ws.theme.palette.ansi[3], 1.0)
    } else {
        ws.ui.accent
    }
}

/// A thin bar `width` wide, filled to `percent`.
fn bar(percent: f32, width: f32, color: Hsla, track: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .w(px(width))
        .h(px(4.0))
        .rounded_full()
        .bg(track)
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(color)
                .w(px(width * percent.clamp(0.0, 100.0) / 100.0)),
        )
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A window's length in a sentence: "5-hour", "weekly", "30-day".
pub(crate) fn window_name(minutes: u32) -> String {
    match minutes {
        10080 => "weekly".to_string(),
        m if m % 1440 == 0 => format!("{}-day", m / 1440),
        m if m % 60 == 0 => format!("{}-hour", m / 60),
        m => format!("{m}-minute"),
    }
}

/// A window's length, short: "5h", "7d".
pub(crate) fn window_label(minutes: u32) -> String {
    match minutes {
        m if m % 1440 == 0 => format!("{}d", m / 1440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

/// When a limit resets (`at`, seconds since the epoch), at `now`, in `tz`: "in 25 min" within
/// the hour, "at 15:40" today, "Mon 09:00" within the week, "Nov 13" later.
pub(crate) fn resets<Tz: TimeZone>(at: u64, now: u64, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let left = at.saturating_sub(now);
    if left < 3600 {
        return format!("in {} min", left.div_ceil(60).max(1));
    }
    let local = |secs: u64| {
        DateTime::from_timestamp(i64::try_from(secs).unwrap_or(i64::MAX), 0)
            .map(|t| t.with_timezone(tz))
    };
    let (Some(at), Some(now)) = (local(at), local(now)) else {
        return String::new();
    };
    if at.date_naive() == now.date_naive() {
        at.format("at %H:%M").to_string()
    } else if left < 6 * 24 * 3600 {
        at.format("%a %H:%M").to_string()
    } else {
        at.format("%b %-d").to_string()
    }
}

/// [`resets`] in the system's time zone.
pub(crate) fn resets_local(at: u64, now: u64) -> String {
    resets(at, now, &chrono::Local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    #[test]
    fn windows_are_named_by_their_length() {
        assert_eq!(window_name(300), "5-hour");
        assert_eq!(window_name(10080), "weekly");
        assert_eq!(window_name(43200), "30-day");
        assert_eq!(window_label(300), "5h");
        assert_eq!(window_label(10080), "7d");
    }

    #[test]
    fn a_chip_says_the_context_and_the_cost_or_the_tokens() {
        let s = |context_percent, context_tokens, tokens, cost_usd| Session {
            model: None,
            context_percent,
            context_tokens,
            tokens,
            cost_usd,
        };
        let text = |session| chip(&session).map(|(t, _)| t);
        assert_eq!(
            text(s(Some(42.4), None, Some(9), Some(3.1))).as_deref(),
            Some("42% · $3.10")
        );
        assert_eq!(
            text(s(Some(18.0), None, Some(1_234_567), None)).as_deref(),
            Some("18% · 1.2M")
        );
        assert_eq!(
            text(s(None, Some(84_321), None, None)).as_deref(),
            Some("84k ctx")
        );
        assert_eq!(text(s(None, None, None, None)), None);
        let (_, tip) = chip(&s(Some(42.0), None, None, Some(3.1))).unwrap();
        assert!(tip.contains("at API prices"), "{tip}");
    }

    #[test]
    fn a_reset_reads_by_how_far_it_is() {
        // 2026-10-14 12:00 at UTC-3 (São Paulo): a Wednesday.
        let tz = FixedOffset::west_opt(3 * 3600).unwrap();
        let now = 1_791_990_000;
        assert_eq!(resets(now + 25 * 60, now, &tz), "in 25 min");
        assert_eq!(resets(now + 10, now, &tz), "in 1 min");
        assert_eq!(resets(now + 3 * 3600 + 40 * 60, now, &tz), "at 15:40");
        assert_eq!(
            resets(now + 5 * 24 * 3600 - 3 * 3600, now, &tz),
            "Mon 09:00"
        );
        assert_eq!(resets(now + 30 * 24 * 3600, now, &tz), "Nov 13");
    }
}
