//! Git through the user's own `git` program, run as a command with stable output formats
//! (`status --porcelain=v2 -z`, a log record format, `for-each-ref`), parsed into typed values.
//! No UI, no library binding: credentials and SSH work as they do in the user's terminal.

mod parse;

pub use parse::{
    BRANCH_FORMAT, Branch, Change, Commit, CommitFile, Entry, LOG_DATE, LOG_FORMAT, Operation,
    STASH_FORMAT, Stash, Status, ago, first_hunk_line, needs_input, not_fast_forward, not_merged,
    parse_branches, parse_log, parse_name_status, parse_stashes, parse_status,
};
