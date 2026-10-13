//! The diff panel: every working change in one scroll (docs/specs/git.md, section 2).

// The panel uses these from the next commit on (plan Task 5 removes this).
#![allow(dead_code, unused_imports)]

mod rows;

pub(crate) use rows::{Note, Row, Shown, rows};
