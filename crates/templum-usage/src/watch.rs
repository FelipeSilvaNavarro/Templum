//! When to tell the user about a plan limit: near its end, and when it resets.

use crate::{Limits, Reset, Window};

/// What to tell the user.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `window` crossed the warning line.
    Warn { agent: String, window: Window },
    /// The window of `minutes` reset; `peak` was its highest use seen.
    Reset {
        agent: String,
        minutes: u32,
        peak: f32,
    },
}

/// The windows seen since Templum started, one per agent and length.
pub struct Watch {
    /// Limits read before this (seconds since the epoch) are old news.
    since: u64,
    tracked: Vec<Tracked>,
}

struct Tracked {
    agent: String,
    minutes: u32,
    resets_at: u64,
    peak: f32,
    warned: bool,
}

/// A reset time within this of the one tracked is the same window: Codex's moves by seconds.
const SAME: u64 = 10 * 60;

/// Use from which a window counts as reached.
// ponytail: agents may stop reporting just under 100; 95 counts as reached
pub const REACHED: f32 = 95.0;

impl Watch {
    pub fn new(since: u64) -> Self {
        Self {
            since,
            tracked: Vec::new(),
        }
    }

    /// What `limits` (each agent's, as last read) mean at `now`, under the user's rules: `warn`
    /// is a percentage, 0 for never.
    pub fn step(
        &mut self,
        now: u64,
        limits: &[(String, Limits)],
        reset: Reset,
        warn: u8,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        self.tracked.retain(|t| {
            if t.resets_at > now {
                return true;
            }
            let tell = match reset {
                Reset::Off => false,
                Reset::Reached => t.peak >= REACHED,
                Reset::Half => t.peak >= 50.0,
                Reset::Always => t.peak > 0.0,
            };
            if tell {
                events.push(Event::Reset {
                    agent: t.agent.clone(),
                    minutes: t.minutes,
                    peak: t.peak,
                });
            }
            false
        });
        for (agent, limits) in limits.iter().filter(|(_, l)| l.at >= self.since) {
            for w in limits.windows.iter().filter(|w| w.resets_at > now) {
                let i = self
                    .tracked
                    .iter()
                    .position(|t| &t.agent == agent && t.minutes == w.minutes);
                let t = match i {
                    Some(i) if self.tracked[i].resets_at.abs_diff(w.resets_at) <= SAME => {
                        &mut self.tracked[i]
                    }
                    _ => {
                        // A new window, or one whose reset moved: what came before is gone.
                        if let Some(i) = i {
                            self.tracked.remove(i);
                        }
                        self.tracked.push(Tracked {
                            agent: agent.clone(),
                            minutes: w.minutes,
                            resets_at: w.resets_at,
                            peak: 0.0,
                            warned: false,
                        });
                        self.tracked.last_mut().expect("just pushed")
                    }
                };
                t.resets_at = w.resets_at;
                t.peak = t.peak.max(w.used_percent);
                if warn > 0 && !t.warned && w.used_percent >= f32::from(warn) {
                    t.warned = true;
                    events.push(Event::Warn {
                        agent: agent.clone(),
                        window: w.clone(),
                    });
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: u64 = 3600;

    fn limits(at: u64, used: f32, resets_at: u64) -> Vec<(String, Limits)> {
        vec![(
            "claude".to_string(),
            Limits {
                windows: vec![Window {
                    minutes: 300,
                    used_percent: used,
                    resets_at,
                }],
                at,
            },
        )]
    }

    fn warns(events: &[Event]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, Event::Warn { .. }))
            .count()
    }

    fn resets(events: &[Event]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, Event::Reset { .. }))
            .count()
    }

    #[test]
    fn warns_once_per_window_and_again_in_the_next() {
        let mut w = Watch::new(0);
        assert!(
            w.step(10, &limits(10, 85.0, HOUR), Reset::Off, 90)
                .is_empty()
        );
        assert_eq!(
            warns(&w.step(20, &limits(20, 91.0, HOUR), Reset::Off, 90)),
            1
        );
        assert_eq!(
            warns(&w.step(30, &limits(30, 97.0, HOUR), Reset::Off, 90)),
            0
        );
        // Codex's reset time moves by seconds: the same window.
        assert_eq!(
            warns(&w.step(40, &limits(40, 98.0, HOUR + 5), Reset::Off, 90)),
            0
        );
        // The next window.
        let next = 6 * HOUR;
        assert_eq!(
            warns(&w.step(HOUR + 1, &limits(HOUR, 92.0, next), Reset::Off, 90)),
            1
        );
    }

    #[test]
    fn warn_zero_never_warns() {
        let mut w = Watch::new(0);
        assert!(
            w.step(10, &limits(10, 100.0, HOUR), Reset::Off, 0)
                .is_empty()
        );
    }

    /// The highest use seen in a window decides, under each rule.
    #[test]
    fn a_reset_is_told_by_the_rule_and_the_peak() {
        let told = |reset: Reset, peak: f32| {
            let mut w = Watch::new(0);
            w.step(10, &limits(10, peak, HOUR), reset, 0);
            // Lower later: the peak still counts.
            w.step(20, &limits(20, peak.min(1.0), HOUR), reset, 0);
            resets(&w.step(HOUR, &[], reset, 0))
        };
        assert_eq!(told(Reset::Reached, 95.0), 1);
        assert_eq!(told(Reset::Reached, 94.0), 0);
        assert_eq!(told(Reset::Half, 50.0), 1);
        assert_eq!(told(Reset::Half, 49.0), 0);
        assert_eq!(told(Reset::Always, 2.0), 1);
        assert_eq!(told(Reset::Always, 0.0), 0);
        assert_eq!(told(Reset::Off, 100.0), 0);
    }

    #[test]
    fn a_reset_is_told_once_with_its_window() {
        let mut w = Watch::new(0);
        w.step(10, &limits(10, 99.0, HOUR), Reset::Reached, 0);
        let events = w.step(HOUR + 3, &[], Reset::Reached, 0);
        assert_eq!(
            events,
            vec![Event::Reset {
                agent: "claude".into(),
                minutes: 300,
                peak: 99.0
            }]
        );
        assert!(w.step(HOUR + 8, &[], Reset::Reached, 0).is_empty());
    }

    /// Old news: limits read before Templum started, and windows whose reset had passed.
    #[test]
    fn old_limits_and_past_resets_tell_nothing() {
        let mut w = Watch::new(100);
        assert!(
            w.step(110, &limits(50, 99.0, HOUR), Reset::Always, 90)
                .is_empty()
        );
        assert!(w.step(HOUR + 1, &[], Reset::Always, 90).is_empty());
        let mut w = Watch::new(0);
        assert!(
            w.step(HOUR + 1, &limits(10, 99.0, HOUR), Reset::Always, 90)
                .is_empty()
        );
        assert!(w.step(HOUR + 2, &[], Reset::Always, 90).is_empty());
    }
}
