//! Drawing the workspace every frame.

use std::time::Instant;

use gpui_kit::base::motion::{MotionStatus, Transition, transition_with_status};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Bounds, Context, DispatchPhase, ElementInputHandler, FontWeight, Hsla, Keystroke,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    TestSupportExt, Window, canvas, div, fill, point, px, size,
};
use templum_canvas::{BlockId, Camera, Gesture, Guide, Point, dot_grid, dot_size};

use crate::block;
use crate::menu;
use crate::motion;
use crate::settings;
use crate::sidebar;
use crate::theme_picker;
use crate::title_bar;
use crate::toolbar;

use super::{Drag, SAVE_DELAY, Tool, Workspace, hint_keys};

impl Workspace {
    /// Every BLINK: toggles a blinking cursor, and makes sure a steady one is shown.
    pub(crate) fn tick_blink(&mut self, cx: &mut Context<Self>) {
        let next = !self.cursor_blinks || !self.blink_on;
        if next != self.blink_on {
            self.blink_on = next;
            cx.notify();
        }
    }

    /// The empty canvas: the name, a line about it, and how to start, with the shortcuts in
    /// effect.
    pub(super) fn hint(&self) -> impl IntoElement {
        let ui = self.ui;
        let pill = move |text: &'static str| settings::chip(div(), ui).child(text);
        let row = |keys: AnyElement, label: &'static str| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.0))
                .w(px(280.0))
                .child(div().text_color(ui.title).child(label))
                .child(keys)
        };
        let [new, settings] = hint_keys(&self.keymap);
        let keys = |stroke: Option<Keystroke>| match stroke {
            Some(stroke) => settings::chip(Kbd::new(stroke), ui).into_any_element(),
            None => pill("No shortcut").into_any_element(),
        };
        div()
            .id("empty-hint")
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(28.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(crate::grid::hsla(self.theme.palette.foreground, 1.0))
                    .child("Templum"),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(ui.hint)
                    .child("An infinite canvas of terminals"),
            )
            .child(
                div()
                    .mt(px(20.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .text_size(px(13.0))
                    .child(row(keys(new), "New terminal"))
                    .child(row(
                        pill("Double-click").into_any_element(),
                        "Open one where you click",
                    ))
                    .child(row(
                        pill(if cfg!(target_os = "macos") {
                            "Hold Option"
                        } else {
                            "Hold Alt"
                        })
                        .into_any_element(),
                        "Move the canvas over blocks",
                    ))
                    .child(row(keys(settings), "Settings")),
            )
            .test_support()
    }
}

/// The dot grid under the blocks: round dots centered on the world lattice. One layer: the
/// dots don't overlap and share a draw order, so GPUI skips the per-primitive bounds lookup.
fn paint_dots(camera: &Camera, color: Hsla, bounds: Bounds<Pixels>, window: &mut Window) {
    let grid = dot_grid(camera);
    let step = grid.spacing * camera.zoom();
    let d = dot_size(camera.zoom());
    let start = camera.to_screen(grid.first);
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    window.paint_layer(bounds, |window| {
        let mut y = start.y;
        while y < h + d {
            let mut x = start.x;
            while x < w + d {
                let at = point(
                    bounds.origin.x + px(x - d / 2.0),
                    bounds.origin.y + px(y - d / 2.0),
                );
                window.paint_quad(
                    fill(Bounds::new(at, size(px(d), px(d))), color).corner_radii(px(d / 2.0)),
                );
                x += step;
            }
            y += step;
        }
    });
}

/// The snap guides: 1 px lines in the accent color, world units to screen.
fn paint_guides(
    camera: &Camera,
    color: Hsla,
    bounds: Bounds<Pixels>,
    guides: &[Guide],
    window: &mut Window,
) {
    for guide in guides {
        let (a, b) = match *guide {
            Guide::Vertical { x, y0, y1 } => (Point::new(x, y0), Point::new(x, y1)),
            Guide::Horizontal { y, x0, x1 } => (Point::new(x0, y), Point::new(x1, y)),
        };
        let (a, b) = (camera.to_screen(a), camera.to_screen(b));
        let rect = Bounds::new(
            point(bounds.origin.x + px(a.x), bounds.origin.y + px(a.y)),
            size(px((b.x - a.x).max(1.0)), px((b.y - a.y).max(1.0))),
        );
        window.paint_quad(fill(rect, color));
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(mut hook) = self.on_frame.take()
            && hook(self, window, cx)
        {
            self.on_frame = Some(hook);
        }
        // Settings changes apply here, where the window is at hand; the save waits for them to settle.
        if let Some(config) = self.pending.take() {
            self.apply_config(config, window, cx);
            self._save = cx.spawn_in(window, async move |ws, cx| {
                cx.background_executor().timer(SAVE_DELAY).await;
                ws.update_in(cx, |ws, window, cx| ws.save_or_notify(window, cx))
                    .ok();
            });
        }
        let fade = transition_with_status(
            "theme-ui",
            self.theme.ui,
            Transition::new(motion::THEME),
            window,
            cx,
        );
        // Blending ends a rounding error away from the theme: take the theme's own colors.
        self.ui = match fade.status {
            MotionStatus::Running | MotionStatus::Delayed => fade.value,
            MotionStatus::Idle | MotionStatus::Finished => self.theme.ui,
        };
        // The focused terminal's painting sets it again this frame, if its cursor blinks.
        self.cursor_blinks = false;
        let exited: Vec<BlockId> = self
            .space
            .terminals()
            .filter(|(_, pane)| pane.term.exited())
            .map(|(id, _)| id)
            .collect();
        for id in exited {
            self.close(id, window, cx);
        }
        // Re-arm once per frame: output is coalesced to frame rate.
        for (_, pane) in self.space.terminals() {
            pane.term.rearm();
        }
        self.hear(window, cx);

        let vp = self.viewport(window);
        if self.space.pristine
            && let Some(b) = self.space.canvas.bounds()
        {
            let z = self.space.canvas.camera.zoom();
            self.space.canvas.camera.origin = Point::new(
                b.x + b.w / 2.0 - vp.w / 2.0 / z,
                b.y + b.h / 2.0 - vp.h / 2.0 / z,
            );
        }
        // Animations: drop the finished ones, and keep frames coming while any runs.
        let now = Instant::now();
        self.space
            .appearing
            .retain(|_, since| motion::progress(*since, now, motion::OPEN) < 1.0);
        self.space
            .ghosts
            .retain(|g| motion::progress(g.since, now, motion::CLOSE) < 1.0);
        if let Some(panel) = &self.settings {
            let t = motion::progress(panel.since, now, motion::SLIDE);
            if t < 1.0 {
                window.request_animation_frame();
            } else if panel.closing {
                self.settings = None;
            }
        }
        if !self.space.appearing.is_empty() || !self.space.ghosts.is_empty() {
            window.request_animation_frame();
        }
        // The view glides to the camera. Anything that moved the camera itself (a drag, the
        // startup centering) ends the flight: the view is then the camera.
        self.space.view = match self.space.flight {
            Some((flight, since)) if flight.to == self.space.canvas.camera => {
                let t = motion::progress(since, now, motion::FLIGHT);
                if t < 1.0 {
                    window.request_animation_frame();
                    flight.at(motion::slide(t))
                } else {
                    self.space.flight = None;
                    self.space.canvas.camera
                }
            }
            _ => {
                self.space.flight = None;
                self.space.canvas.camera
            }
        };
        let visible: Vec<_> = self
            .space
            .canvas
            .visible_from(&self.space.view, vp)
            .copied()
            .collect();
        let mut blocks: Vec<_> = visible
            .into_iter()
            .map(|b| {
                let look = self
                    .space
                    .appearing
                    .get(&b.id)
                    .map_or(motion::Look::REST, |since| {
                        motion::opening(motion::progress(*since, now, motion::OPEN))
                    });
                let body = &self.space.blocks[&b.id];
                block::render(self, b, body, look, true, window, cx)
            })
            .collect();
        // Ghosts fade above the blocks, where they were.
        blocks.extend(self.space.ghosts.iter().map(|g| {
            let look = motion::closing(motion::progress(g.since, now, motion::CLOSE));
            block::render(self, g.block, &g.body, look, false, window, cx)
        }));

        let entity = cx.entity();
        let focus = self.focus_handle.clone();
        let camera = self.space.view;
        let dot = self.ui.dot;
        let guides = self.space.guides.clone();
        let accent = self.ui.accent;
        // A full-size layer under the blocks: the IME handler, the workspace origin, and
        // window-wide drag listeners (drags keep working outside the window).
        let layer = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                paint_dots(&camera, dot, bounds, window);
                paint_guides(&camera, accent, bounds, &guides, window);
                window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx);
                entity.update(cx, |ws, _| ws.origin = bounds.origin);
                let on_move = entity.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        on_move.update(cx, |ws, cx| {
                            // Alt let go while the window wasn't listening (another window had
                            // the keyboard): the next move puts the hand right.
                            if e.modifiers.alt != ws.hand {
                                cx.notify();
                            }
                            // Any button: the middle one pans too.
                            if e.pressed_button.is_some() {
                                ws.drag_to(e.position, e.modifiers.shift, cx);
                            } else {
                                ws.end_drag();
                            }
                        });
                    }
                });
                let on_up = entity.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        on_up.update(cx, |ws, cx| match e.button {
                            MouseButton::Left => ws.release(e.position, cx),
                            MouseButton::Middle => ws.end_drag(),
                            _ => {}
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full();

        self.hand = window.modifiers().alt;
        let hand = self.hand || self.tool == Tool::Hand;
        let panning = matches!(self.drag, Some(Drag::Canvas(Gesture::Pan { .. })));
        let area = div()
            .id("workspace")
            .relative()
            .flex_1()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_background_down))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(Self::on_background_right_down),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|ws, e: &MouseDownEvent, _, _| ws.begin_pan(e.position)),
            )
            .on_scroll_wheel(cx.listener(Self::on_background_scroll))
            .on_pinch(cx.listener(Self::on_pinch))
            .child(layer)
            .children(blocks)
            // The drawings, above the blocks; with a drawing tool on, a layer over them takes the
            // pointer.
            .child(self.ink_layer())
            .when(self.tool.draws(), |el| {
                el.child(
                    div()
                        .id("ink-layer")
                        .absolute()
                        .size_full()
                        .cursor_crosshair()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                ws.begin_ink(e);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .on_scroll_wheel(cx.listener(Self::on_background_scroll))
                        .on_pinch(cx.listener(Self::on_pinch))
                        .test_support(),
                )
            })
            // The hand: while Alt is held, a layer over the blocks takes the pointer, and a drag
            // or the wheel moves the canvas wherever it is.
            .when(hand, |el| {
                el.child(
                    div()
                        .id("hand")
                        .absolute()
                        .size_full()
                        .map(|el| {
                            if panning {
                                el.cursor_grabbing()
                            } else {
                                el.cursor_grab()
                            }
                        })
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                ws.begin_pan(e.position);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .on_scroll_wheel(cx.listener(Self::on_background_scroll))
                        .on_pinch(cx.listener(Self::on_pinch))
                        .test_support(),
                )
            })
            .when(self.space.canvas.blocks().is_empty(), |el| {
                el.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(self.hint()),
                )
            })
            .child(toolbar::render(self, cx));
        let picker = self
            .picker
            .as_ref()
            .map(|p| theme_picker::render(self, p, cx));
        if self.recording.is_some() && !self.settings_focus.is_focused(window) {
            self.recording = None;
            self.key_note = None;
        }
        let panel = self
            .settings
            .as_ref()
            .map(|p| settings::render(self, p, now, vp.w, window, cx));
        div()
            .id("window")
            // Alt held or let go: the hand comes or goes.
            .on_modifiers_changed(cx.listener(|_, _: &ModifiersChangedEvent, _, cx| cx.notify()))
            .size_full()
            .flex()
            .flex_col()
            // The settings panel slides in from beyond the right edge.
            .overflow_hidden()
            .bg(self.ui.canvas)
            .child(title_bar::render(self, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .when(self.sidebar, |el| el.child(sidebar::render(self, cx)))
                    .child(area),
            )
            .children(panel)
            .children(picker)
            .children(self.menu.as_ref().map(|m| menu::render(m, window)))
    }
}
