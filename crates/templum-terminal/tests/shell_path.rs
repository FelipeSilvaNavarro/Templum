//! The login shell's PATH is read on a thread at start; what needs it can wait for it
//! (docs/specs/shells.md, Finding shells, completed). Its own test binary: the read is once per
//! process.
#![cfg(unix)]

#[test]
fn waiting_for_the_login_path_returns_at_once_when_none_is_read_and_ends_once_it_is() {
    assert!(!templum_terminal::shell_path_pending());
    // Nothing started: this must not wait for ever.
    templum_terminal::wait_for_shell_path();
    templum_terminal::read_shell_path();
    templum_terminal::wait_for_shell_path();
    assert!(!templum_terminal::shell_path_pending());
}
