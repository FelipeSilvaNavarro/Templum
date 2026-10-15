//! Level of detail: how much of a block is drawn, from how big its text is on screen (its size at
//! zoom 1 times the zoom), like InDesign's "Greek Type Below". Pure: unit-tested here.
//! See docs/specs/level-of-detail.md, section 1.

/// Text smaller than this on screen (pixels) is drawn as bars: about the largest default size at
/// 33% (4.62 px) and the smallest at 50% (6.0 px) apart, so every default block changes level at
/// the same step and none sits on the edge.
pub const GREEK_BELOW: f32 = 5.5;
/// Under this a row is about 3 px and its bars 1 px lines: the block is its card.
pub const CARD_BELOW: f32 = 2.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Drawn as it is: the text is readable.
    Full,
    /// The text is bars in its own colors, on the letters' rows and columns.
    Silhouette,
    /// The block's card with its title and status, nothing inside.
    Card,
}

/// The level of text `text` units high at zoom 1, drawn at `zoom`.
pub fn level(text: f32, zoom: f32) -> Level {
    let on_screen = text * zoom;
    if on_screen >= GREEK_BELOW {
        Level::Full
    } else if on_screen >= CARD_BELOW {
        Level::Silhouette
    } else {
        Level::Card
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ZOOM_STEPS;

    /// A terminal (14), an editor, a note or a kanban block (13), a Git block or a title (12).
    const DEFAULTS: [f32; 3] = [12.0, 13.0, 14.0];

    #[test]
    fn default_sizes_are_bars_at_25_and_33_and_letters_from_50() {
        for text in DEFAULTS {
            for zoom in [0.25, 0.33] {
                assert_eq!(level(text, zoom), Level::Silhouette, "{text} at {zoom}");
            }
            for zoom in ZOOM_STEPS.into_iter().filter(|&z| z >= 0.5) {
                assert_eq!(level(text, zoom), Level::Full, "{text} at {zoom}");
            }
        }
    }

    /// Float error can't flip a level: no step puts a default size within 0.25 px of a threshold.
    #[test]
    fn no_step_puts_a_default_size_near_a_threshold() {
        for text in DEFAULTS {
            for zoom in ZOOM_STEPS {
                for threshold in [GREEK_BELOW, CARD_BELOW] {
                    let off = (text * zoom - threshold).abs();
                    assert!(off >= 0.25, "{text} at {zoom}: {off} px from {threshold}");
                }
            }
        }
    }

    #[test]
    fn small_fonts_turn_to_cards_and_large_ones_keep_their_letters() {
        assert_eq!(level(9.0, 0.25), Level::Card, "2.25 px");
        assert_eq!(level(10.0, 0.25), Level::Silhouette, "2.5 px");
        assert_eq!(level(17.0, 0.33), Level::Full, "5.61 px");
    }
}
