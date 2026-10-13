//! Selecting text in the panel. The rows show each line's display text: without a trailing
//! '\r', tabs as 4 spaces.

/// What a tab is drawn as.
pub(crate) const TAB: &str = "    ";

/// A line as it is drawn: without a trailing '\r' (a CRLF file), tabs as 4 spaces.
pub(crate) fn display(text: &str) -> String {
    text.strip_suffix('\r').unwrap_or(text).replace('\t', TAB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_drops_the_cr_and_expands_tabs() {
        assert_eq!(display("a\tb\r"), "a    b");
        assert_eq!(display("plain"), "plain");
        assert_eq!(display("\r"), "");
    }
}
