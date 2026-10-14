//! Agents' session usage and plan limits, as Templum shows them: the title bar's meter, its
//! popover, a terminal's chip (docs/specs/usage.md).

use chrono::{DateTime, TimeZone};

/// A window's length in a sentence: "5-hour", "weekly", "30-day".
pub(crate) fn window_name(minutes: u32) -> String {
    match minutes {
        10080 => "weekly".to_string(),
        m if m % 1440 == 0 => format!("{}-day", m / 1440),
        m if m % 60 == 0 => format!("{}-hour", m / 60),
        m => format!("{m}-minute"),
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
