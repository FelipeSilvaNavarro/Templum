//! A note block: Markdown text, edited in gpui-kit's editor (tree-sitter highlights it) or shown
//! rendered. Its title is its first heading. See docs/specs/cockpit.md, section 5.

use std::cell::{Cell, RefCell};
use std::ops::Range;

use gpui_kit::component::input::{Editor, EditorState, Enter};
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, Context, Element, ElementId, Entity, Focusable as _, GlobalElementId,
    InspectorElementId, IntoElement, KeyDownEvent, Keystroke, LayoutId, Pixels, ScrollHandle,
    ScrollWheelEvent, SharedString, StyleRefinement, TestSupportExt, Window, div, px, relative,
};
use templum_canvas::{BlockId, Level, Size, chrome};

use crate::block::BORDER;
use crate::board::Label;
use crate::grid::{BAR_OPACITY, hsla};
use crate::silhouette::{self, Far};
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
pub(crate) const FIXED_LEFT: f32 = 6.0 + 6.0 + 18.0;
const FIXED_RIGHT: f32 = 10.0 + 10.0;
pub(crate) const FIXED_TOP: f32 = 8.0;
const FIXED_BOTTOM: f32 = 8.0;

/// A note block: its text lives in the editor; `preview` shows it rendered instead.
pub(crate) struct Note {
    pub editor: Entity<EditorState>,
    pub preview: bool,
    /// Its card's tint, a post-it's color; None: the blocks' own.
    pub color: Option<Label>,
    pub preview_scroll: PreviewScroll,
    /// Its word map, drawn far away (`silhouette.rs`).
    pub far: RefCell<Far>,
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
        Note {
            editor,
            preview,
            color: None,
            preview_scroll: PreviewScroll::default(),
            far: RefCell::default(),
        }
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    /// What sending it to a terminal types: its selection, or all of it (always while it shows
    /// rendered: a selection there is hidden).
    pub fn to_send(&self, cx: &App) -> String {
        let selected = self.editor.read(cx).selected_value();
        if self.preview || selected.trim().is_empty() {
            self.text(cx)
        } else {
            selected.to_string()
        }
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

/// The body below the header at the block's zoom `z`: the editor, or the rendered text; below
/// Full, the text's bars (`block.rs` leaves a note shown rendered out there).
pub(crate) fn body(
    ws: &Workspace,
    id: BlockId,
    note: &Note,
    z: f32,
    level: Level,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let n = id.0 as usize;
    let content = ws
        .space
        .canvas
        .get(id)
        .map_or(Size::default(), |b| chrome::content_rect(b.rect).size());
    // What its word map is of.
    let taken = silhouette::Taken {
        len: note.editor.read(cx).text().len(),
        size: (content.w, content.h),
        font: ws.cell.font.family.clone(),
        wrap: true,
        tree: false,
    };
    if level != Level::Full {
        // Its words as bars: no Markdown keys, and the wheel moves the canvas.
        return silhouette::body(
            ("silhouette", n).into(),
            &note.editor,
            &note.far,
            taken,
            z,
            FONT,
            silhouette::note_origin,
            hsla(ws.theme.palette.foreground, BAR_OPACITY),
            |cx| editor_box(ws, n, note, content, z, cx),
            cx,
        );
    }
    let inner = if note.preview {
        preview(
            ("note-preview", n).into(),
            ("note-markdown", n).into(),
            note.text(cx).into(),
            &note.preview_scroll,
            content.w - 2.0 * BORDER,
            FONT,
            z,
        )
    } else {
        note.far.borrow_mut().full = Some((z, taken));
        editor_box(ws, n, note, content, z, cx)
    };
    // The wheel scrolls the note and stops there; with Ctrl it zooms the canvas.
    let (enter, key) = (note.editor.clone(), note.editor.clone());
    div()
        .size_full()
        .text_size(px(FONT * z))
        .on_scroll_wheel(|e: &ScrollWheelEvent, _, cx| {
            if !e.modifiers.control {
                cx.stop_propagation();
            }
        })
        // Markdown keys, in the note's own text only (not its search field): before the editor
        // takes Enter, which it binds; Ctrl+B and Ctrl+I it doesn't.
        .when(!note.preview, |el| {
            el.capture_action(move |e: &Enter, window, cx| {
                let done = enter.update(cx, |state, cx| {
                    state.focus_handle(cx).is_focused(window)
                        && !e.shift
                        && if e.secondary {
                            tick(state, window, cx)
                        } else {
                            continue_list(state, window, cx)
                        }
                });
                if done {
                    cx.stop_propagation();
                }
            })
            .on_key_down(move |e: &KeyDownEvent, window, cx| {
                let Some(marker) = emphasis(&e.keystroke) else {
                    return;
                };
                let done = key.update(cx, |state, cx| {
                    let focused = state.focus_handle(cx).is_focused(window);
                    if focused {
                        emphasize(state, marker, window, cx);
                    }
                    focused
                });
                if done {
                    cx.stop_propagation();
                }
            })
        })
        .child(inner)
        .into_any_element()
}

/// The note's editor as Full draws it, at zoom `z`. Its fixed parts would make the text start
/// further in, wrap at other words and show fewer lines the further the canvas is zoomed out:
/// the editor is as much larger (or smaller) and moved by its fixed left and top parts' share,
/// so the text starts, wraps and fits as if they zoomed too. Its lines change only when the
/// block is resized. At zoom 1, exactly the note's box.
fn editor_box(
    ws: &Workspace,
    n: usize,
    note: &Note,
    content: Size,
    z: f32,
    cx: &mut App,
) -> AnyElement {
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
                    // On the editor itself: it sets the theme's code size on itself, over a
                    // parent's.
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
}

/// A Markdown preview's scroll, kept through zooms (see `preview`).
#[derive(Default)]
pub(crate) struct PreviewScroll {
    pub(crate) handle: ScrollHandle,
    /// The zoom it was last drawn at.
    zoom: Cell<f32>,
}

/// Markdown `text` rendered at zoom `z` like a photo of it at zoom 1: `w` world units wide, its
/// text `font`, headings, code and spacing (in rems) all scaled by `z`, so it wraps at the same
/// words at every zoom; its scroll follows the zoom.
pub(crate) fn preview(
    id: ElementId,
    markdown: ElementId,
    text: SharedString,
    scroll: &PreviewScroll,
    w: f32,
    font: f32,
    z: f32,
) -> AnyElement {
    crate::editor::keep_list_rows(&scroll.handle, &scroll.zoom, z);
    // gpui-kit's defaults at zoom 1: headings from 14 px, code 13 px.
    let style = TextViewStyle {
        heading_base_font_size: px(14.0 * z),
        code_block: StyleRefinement::default().text_size(px(13.0 * z)),
        ..TextViewStyle::default()
    };
    div()
        .id(id)
        .h_full()
        .w(px(w.max(0.0) * z))
        .overflow_y_scroll()
        .track_scroll(&scroll.handle)
        .text_size(px(font * z))
        .px(px(6.0 * z))
        .child(ScaledRem {
            scale: z,
            child: TextView::markdown(markdown, text)
                .style(style)
                .into_any_element(),
        })
        .test_support()
        .into_any_element()
}

/// Lays out and paints its child with the window's rem size times `scale`: Markdown's spacing
/// is in rems, which no style can scale for one element (gpui-pre-0.3.8 src/window.rs:3032).
struct ScaledRem {
    scale: f32,
    child: AnyElement,
}

impl IntoElement for ScaledRem {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for ScaledRem {
    /// The rem size it laid its child out with.
    type RequestLayoutState = Pixels;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Pixels) {
        let rem = window.rem_size() * self.scale;
        let layout =
            window.with_rem_size(Some(rem), |window| self.child.request_layout(window, cx));
        (layout, rem)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        rem: &mut Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(*rem), |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        rem: &mut Pixels,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(*rem), |window| self.child.paint(window, cx));
    }
}

/// The byte range of the line holding byte `at` of `text`.
fn line_at(text: &str, at: usize) -> Range<usize> {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    start..end
}

/// Enter on a list item: the next item starts on the new line; on an empty item the list
/// ends. False (a plain Enter) anywhere else, or with text selected.
fn continue_list(
    state: &mut EditorState,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> bool {
    if !state.selected_range().is_empty() {
        return false;
    }
    let text = state.value().to_string();
    let at = state.cursor();
    let line = line_at(&text, at);
    let Some(next) = next_item(&text[line.clone()], at - line.start) else {
        return false;
    };
    // The main cursor only: gpui-kit's replace types at every cursor (setting the range drops
    // the others).
    if next.is_empty() {
        state.set_selected_range(line, cx);
        state.replace("", window, cx);
    } else {
        state.set_selected_range(at..at, cx);
        state.replace(format!("\n{next}"), window, cx);
    }
    true
}

/// Ctrl+Enter: the cursor's line becomes a task, or its box is ticked or unticked.
fn tick(state: &mut EditorState, window: &mut Window, cx: &mut Context<EditorState>) -> bool {
    let text = state.value().to_string();
    let at = state.cursor();
    let line = line_at(&text, at);
    let new = toggle_task(&text[line.clone()]);
    // The text after the box keeps the cursor where it was.
    let cursor = (at + new.len())
        .saturating_sub(line.len())
        .clamp(line.start, line.start + new.len());
    state.set_selected_range(line, cx);
    state.replace(new, window, cx);
    state.set_selected_range(cursor..cursor, cx);
    true
}

/// `marker` around the selection, or a pair with the cursor between.
fn emphasize(
    state: &mut EditorState,
    marker: &str,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    let range = state.selected_range();
    let selected = state.value()[range.clone()].to_string();
    // The main selection only (see `continue_list`).
    state.set_selected_range(range.clone(), cx);
    state.replace(format!("{marker}{selected}{marker}"), window, cx);
    if selected.is_empty() {
        let at = range.start + marker.len();
        state.set_selected_range(at..at, cx);
    }
}

/// A list item's indent and marker (`- `, `* `, `+ `, `1. `, `1) `, `- [ ] `), their length in
/// bytes, and the next item's.
fn item_prefix(line: &str) -> Option<(usize, String)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let (len, next) = match ["- ", "* ", "+ "].into_iter().find(|b| rest.starts_with(b)) {
        Some(bullet) => {
            let task = ["[ ] ", "[x] ", "[X] "]
                .iter()
                .any(|t| rest[2..].starts_with(t));
            if task {
                (6, format!("{bullet}[ ] "))
            } else {
                (2, bullet.to_string())
            }
        }
        None => {
            let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            let n: u64 = rest[..digits].parse().ok()?;
            let sep = rest[digits..]
                .chars()
                .next()
                .filter(|c| matches!(c, '.' | ')'))?;
            if !rest[digits + 1..].starts_with(' ') {
                return None;
            }
            (digits + 2, format!("{}{sep} ", n + 1))
        }
    };
    Some((indent + len, format!("{}{next}", &line[..indent])))
}

/// What Enter on `line`, the cursor at its byte `at`, types after the line break: the next
/// item's start, or "" when the item is empty (the list ends). None: not after a marker.
fn next_item(line: &str, at: usize) -> Option<String> {
    let (len, next) = item_prefix(line)?;
    if at < len {
        return None;
    }
    Some(if line[len..].trim().is_empty() {
        String::new()
    } else {
        next
    })
}

/// A task's box ticked or unticked; a list item or a plain line made a task.
fn toggle_task(line: &str) -> String {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let (head, rest) = line.split_at(indent);
    for bullet in ["- ", "* ", "+ "] {
        if let Some(after) = rest.strip_prefix(bullet) {
            if let Some(text) = after.strip_prefix("[ ]") {
                return format!("{head}{bullet}[x]{text}");
            }
            if let Some(text) = after
                .strip_prefix("[x]")
                .or_else(|| after.strip_prefix("[X]"))
            {
                return format!("{head}{bullet}[ ]{text}");
            }
            return format!("{head}{bullet}[ ] {after}");
        }
    }
    format!("{head}- [ ] {rest}")
}

/// Ctrl+B (bold) and Ctrl+I (italic), Cmd on macOS: their Markdown marker.
fn emphasis(k: &Keystroke) -> Option<&'static str> {
    let m = &k.modifiers;
    if !m.secondary() || m.alt || m.shift || m.function {
        return None;
    }
    match k.key.as_str() {
        "b" => Some("**"),
        "i" => Some("*"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_continues_bullets_numbers_and_tasks() {
        assert_eq!(next_item("- a", 3).as_deref(), Some("- "));
        assert_eq!(next_item("  * a", 5).as_deref(), Some("  * "));
        assert_eq!(next_item("9. a", 4).as_deref(), Some("10. "));
        assert_eq!(next_item("1) a", 4).as_deref(), Some("2) "));
        assert_eq!(next_item("- [x] done", 10).as_deref(), Some("- [ ] "));
        assert_eq!(next_item("- a", 1), None, "inside the marker");
        assert_eq!(next_item("plain", 5), None);
        assert_eq!(next_item("1.5 kg", 6), None);
    }

    #[test]
    fn enter_on_an_empty_item_ends_the_list() {
        assert_eq!(next_item("- ", 2).as_deref(), Some(""));
        assert_eq!(next_item("- [ ] ", 6).as_deref(), Some(""));
    }

    #[test]
    fn ctrl_enter_toggles_a_task() {
        assert_eq!(toggle_task("- [ ] a"), "- [x] a");
        assert_eq!(toggle_task("  - [x] a"), "  - [ ] a");
        assert_eq!(toggle_task("* a"), "* [ ] a");
        assert_eq!(toggle_task("buy milk"), "- [ ] buy milk");
    }

    #[test]
    fn emphasis_keys() {
        let k = |s: &str| Keystroke::parse(s).unwrap();
        let (bold, italic) = if cfg!(target_os = "macos") {
            ("cmd-b", "cmd-i")
        } else {
            ("ctrl-b", "ctrl-i")
        };
        assert_eq!(emphasis(&k(bold)), Some("**"));
        assert_eq!(emphasis(&k(italic)), Some("*"));
        assert_eq!(emphasis(&k("b")), None);
        assert_eq!(emphasis(&k(&format!("shift-{bold}"))), None);
    }

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
