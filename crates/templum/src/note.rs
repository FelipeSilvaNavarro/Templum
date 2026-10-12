//! A note block: Markdown text, edited in gpui-kit's editor (tree-sitter highlights it) or shown
//! rendered. Its title is its first heading. See docs/specs/cockpit.md, section 5.

use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::component::text::TextView;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, ScrollWheelEvent, SharedString, TestSupportExt, Window, div,
    px, relative,
};
use templum_canvas::{BlockId, Size, chrome};

use crate::block::BORDER;
use crate::workspace::Workspace;

/// A new note's size, and the smallest one (world units).
pub(crate) const SIZE: Size = Size { w: 360.0, h: 280.0 };
pub(crate) const MIN: Size = Size { w: 200.0, h: 140.0 };
/// The text's size at zoom 1, world units.
pub(crate) const FONT: f32 = 13.0;
/// What gpui-kit's code editor keeps at fixed pixel sizes at any zoom: its padding
/// (gpui-component-0.7.1 src/input/input.rs:591-600 and sizing.rs:147-166, size Medium: 6 left
/// in a code editor, 10 right, 8 top and bottom) and, inside it, left of the text, the line
/// numbers' margin (6) and the fold icons (18), and right of it a margin its wrap width leaves
/// out (10) (gpui-base-0.7.1 src/input/base/element.rs:50-53, 1102-1124, 2562).
const FIXED_LEFT: f32 = 6.0 + 6.0 + 18.0;
const FIXED_RIGHT: f32 = 10.0 + 10.0;
const FIXED_TOP: f32 = 8.0;
const FIXED_BOTTOM: f32 = 8.0;

/// A note block: its text lives in the editor; `preview` shows it rendered instead.
pub(crate) struct Note {
    pub editor: Entity<EditorState>,
    pub preview: bool,
}

impl Note {
    pub fn new(text: &str, preview: bool, window: &mut Window, cx: &mut App) -> Note {
        let text = text.to_string();
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .soft_wrap(true)
                .line_number(false)
                .default_value(text)
        });
        Note { editor, preview }
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    pub fn title(&self, cx: &App) -> SharedString {
        title(&self.text(cx)).into()
    }

    /// Typing goes to the editor.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
    }
}

/// The first heading with text (`#` to `######`, a space, text), without its hashes; "Note"
/// without one.
pub(crate) fn title(text: &str) -> String {
    text.lines()
        .find_map(|line| {
            let line = line.trim_start();
            let rest = line.trim_start_matches('#');
            let hashes = line.len() - rest.len();
            let heading = (1..=6).contains(&hashes) && rest.starts_with(' ');
            let rest = rest.trim();
            (heading && !rest.is_empty()).then(|| rest.to_string())
        })
        .unwrap_or_else(|| "Note".to_string())
}

/// The body below the header: the editor, or the rendered text. Both at the block's zoom `z`.
pub(crate) fn body(
    ws: &Workspace,
    id: BlockId,
    note: &Note,
    z: f32,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = id.0 as usize;
    let inner = if note.preview {
        div()
            .id(("note-preview", n))
            .size_full()
            .overflow_y_scroll()
            .px(px(6.0 * z))
            .child(TextView::markdown(
                ("note-markdown", n),
                SharedString::from(note.text(cx)),
            ))
            .test_support()
            .into_any_element()
    } else {
        // Its fixed parts would make the text start further in, wrap at other words and show
        // fewer lines the further the canvas is zoomed out: the editor is as much larger (or
        // smaller) and moved by its fixed left and top parts' share, so the text starts, wraps
        // and fits as if they zoomed too. Its lines change only when the block is resized. At
        // zoom 1, exactly the note's box.
        let content = ws
            .space
            .canvas
            .get(id)
            .map_or(Size::default(), |b| chrome::content_rect(b.rect).size());
        let (across, down) = (FIXED_LEFT + FIXED_RIGHT, FIXED_TOP + FIXED_BOTTOM);
        crate::editor::keep_rows(&note.editor, FONT, z, cx);
        div()
            .id(("note-editor", n))
            .size_full()
            .font_family(ws.cell.font.family.clone())
            .child(
                div()
                    .id(("note-text", n))
                    .relative()
                    .left(px(-FIXED_LEFT * (1.0 - z)))
                    .top(px(-FIXED_TOP * (1.0 - z)))
                    .w(px(
                        ((content.w - 2.0 * BORDER - across) * z + across).max(0.0)
                    ))
                    .h(px(((content.h - down) * z + down).max(0.0)))
                    .child(
                        // On the editor itself: it sets the theme's code size on itself, over
                        // a parent's.
                        Editor::new(&note.editor)
                            .appearance(false)
                            .bordered(false)
                            .text_size(px(FONT * z))
                            .h(relative(1.0)),
                    )
                    .test_support(),
            )
            .test_support()
            .into_any_element()
    };
    // The wheel scrolls the note and stops there; with Ctrl it zooms the canvas.
    div()
        .size_full()
        .text_size(px(FONT * z))
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        .child(inner)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_is_the_first_heading_or_note() {
        assert_eq!(title("# Plan\n- x"), "Plan");
        assert_eq!(title("text\n\n## Second  \n"), "Second");
        assert_eq!(title("  ###   Spaced   "), "Spaced");
        assert_eq!(title("#NoSpace\n# Real"), "Real", "a heading needs a space");
        assert_eq!(title("#\n# "), "Note", "empty headings don't count");
        assert_eq!(title("no heading"), "Note");
        assert_eq!(title(""), "Note");
    }
}
