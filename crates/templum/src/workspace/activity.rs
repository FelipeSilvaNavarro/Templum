//! An agent's state from its screen, where its hooks say nothing (docs/specs/connections.md, An
//! agent's state): a screen that keeps changing (a spinner, a timer) is an agent at work, one
//! that stays still is not. Codex says only that its turn ended; Claude Code says nothing when
//! Esc stops it while it writes, nor when a tool the person allowed starts.

use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::time::Duration;

use gpui_kit::Context;
use templum_mcp::{Reply, tools};

use super::waits::LAST_LINES;
use super::{AgentState, Body, Workspace};

/// How often Templum looks at the agents' screens.
pub(super) const LOOK: Duration = Duration::from_secs(1);
/// Of the last 4 looks, how many saw the screen change for its agent to be at work: Codex's
/// "Working (5s)" changes once a second, and a look may fall between two changes.
const CHANGING: u32 = 3;
/// How many looks in a row saw the screen still for its agent to be at rest.
const STILL: u32 = 5;

/// The looks at an agent's screen: its content's hash, whether each of the last 8 saw it change
/// (the newest in the lowest bit), and how many there were.
#[derive(Default)]
pub(crate) struct Looks {
    hash: u64,
    changes: u8,
    count: u8,
}

impl Looks {
    /// A look that found the screen's content hashing to `hash`.
    fn see(&mut self, hash: u64) {
        self.changes = (self.changes << 1) | u8::from(hash != self.hash);
        self.hash = hash;
        self.count = self.count.saturating_add(1);
    }

    /// The state these looks give an agent in `state`; None when they say nothing new. A
    /// changing screen is at work, also after a dialog the person answered; a still one ends
    /// only work, never a wait for the person.
    fn state(&self, state: &Option<AgentState>) -> Option<AgentState> {
        let changing = (self.changes & 0b1111).count_ones() >= CHANGING;
        let still = u32::from(self.count) >= STILL && self.changes & ((1u8 << STILL) - 1) == 0;
        match state {
            Some(AgentState::Working) if still => Some(AgentState::Idle),
            None | Some(AgentState::Idle | AgentState::NeedsYou(_)) if changing => {
                Some(AgentState::Working)
            }
            _ => None,
        }
    }
}

impl Workspace {
    /// A look at every agent's screen, in every workspace: the state it gives, where that is new.
    /// A look after the person typed into it tells nothing (their keys change it). An agent at
    /// work whose screen went still ends the task it was on, with its last lines: no event will.
    pub(super) fn look_at_agents(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut still = Vec::new();
        let spaces = std::iter::once(&mut self.space).chain(&mut self.others);
        for pane in spaces
            .flat_map(|s| s.blocks.values_mut())
            .filter_map(Body::terminal_mut)
        {
            if pane.seen.agent.is_none() {
                pane.looks = Looks::default();
                continue;
            }
            let mut hasher = DefaultHasher::new();
            pane.screen().hash(&mut hasher);
            let hash = hasher.finish();
            if pane.typed.replace(false) {
                pane.looks.hash = hash;
                continue;
            }
            pane.looks.see(hash);
            let Some(state) = pane.looks.state(&pane.state) else {
                continue;
            };
            if state == AgentState::Idle {
                still.push(pane.term.id().to_string());
            }
            pane.state = Some(state);
            changed = true;
        }
        for terminal in still {
            let lines = self
                .pane(&terminal)
                .map(|pane| pane.term.shared().lines(LAST_LINES).join("\n"))
                .unwrap_or_default();
            let reply = Reply::ok(format!(
                "Its screen has been still for {STILL} seconds, and Templum heard no answer: it may \
                 be done, stopped, or asking its person. The last lines of its screen:\n{}",
                tools::tail(&lines, tools::MAX_TEXT)
            ));
            self.end_turn(&terminal, reply, cx);
        }
        if changed {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The looks after a first one, each seeing the screen change or not.
    fn saw(changes: &[bool]) -> Looks {
        let mut looks = Looks::default();
        let mut hash = 1;
        looks.see(hash);
        for &changed in changes {
            hash += u64::from(changed);
            looks.see(hash);
        }
        looks
    }

    #[test]
    fn a_screen_that_keeps_changing_is_at_work() {
        // Codex's "Working (5s)" changes once a second: a look may fall between two changes.
        let looks = saw(&[true, false, true, true]);
        assert_eq!(looks.state(&None), Some(AgentState::Working));
        assert_eq!(
            looks.state(&Some(AgentState::Idle)),
            Some(AgentState::Working)
        );
        assert_eq!(
            looks.state(&Some(AgentState::NeedsYou(Some("Allow Bash?".into())))),
            Some(AgentState::Working),
            "the tool the person allowed runs"
        );
        assert_eq!(looks.state(&Some(AgentState::Working)), None, "nothing new");
        assert_eq!(
            saw(&[true, false, false, true]).state(&None),
            None,
            "twice in four looks: a redraw"
        );
    }

    #[test]
    fn a_screen_still_for_five_looks_is_at_rest() {
        let working = Some(AgentState::Working);
        assert_eq!(
            saw(&[true, false, false, false, false, false]).state(&working),
            Some(AgentState::Idle)
        );
        assert_eq!(
            saw(&[true, false, false, false, false]).state(&working),
            None,
            "four"
        );
        assert_eq!(
            saw(&[false; 6]).state(&Some(AgentState::NeedsYou(None))),
            None,
            "a dialog waits for its person, still: it stays yellow"
        );
        assert_eq!(saw(&[false; 6]).state(&None), None);
        assert_eq!(saw(&[false; 6]).state(&Some(AgentState::Idle)), None);
    }
}
