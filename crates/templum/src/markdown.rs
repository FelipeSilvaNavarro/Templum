//! Markdown rendered like a photo of it at zoom 1: an editor's `.md` file shown rendered.

use std::cell::Cell;
use std::sync::Arc;

use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, ScrollHandle, SharedString, StyleRefinement, TestSupportExt, Window, div, px,
    rems,
};

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
    // gpui-kit 0.7.1's look at zoom 1: headings from 14 px, code 13 px, paragraphs 1 rem apart.
    let style = TextViewStyle {
        paragraph_gap: rems(1.0),
        heading_base_font_size: px(14.0 * z),
        // Pinned at every zoom: with a 14 px base and no resolver gpui-kit sizes headings from
        // the body text instead (gpui-component-0.7.3 src/text/compat.rs:339-345), so they
        // would shrink at zoom 1 exactly. The resolver is that default's own scale.
        heading_font_size: Some(Arc::new(|level, base| {
            base * match level {
                1 => 2.0,
                2 => 1.5,
                3 => 1.25,
                4 => 1.125,
                _ => 1.0,
            }
        })),
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
/// is in rems, which no style can scale for one element (gpui-pre-0.3.9 src/window.rs:3150).
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
