//! One field's row (title, description, control, reset), and the controls the rows are built
//! from.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, AnyElement, App, ClickEvent, Context, ElementId, Entity, FontWeight, SharedString,
    Subscription, TestSupportExt, Window, div, px,
};

use crate::config::{Config, Edit};
use crate::theme::Ui;
use crate::workspace::Workspace;

/// Puts one setting back to its default.
pub(super) type Reset = Rc<dyn Fn(&mut Window, &mut App)>;

/// One field: a title and a description on the left, its control on the right.
pub(super) struct Row {
    /// `table.key`: the control's element id is `field-<key>`, its reset's `reset-<key>`.
    key: SharedString,
    title: SharedString,
    description: Option<SharedString>,
    control: Option<AnyElement>,
    /// Drawn under the title, full width (the font preview).
    below: Option<AnyElement>,
    /// Some while the value isn't the default.
    pub(super) reset: Option<Reset>,
}

impl Row {
    pub(super) fn new(key: impl Into<SharedString>, title: impl Into<SharedString>) -> Row {
        Row {
            key: key.into(),
            title: title.into(),
            description: None,
            control: None,
            below: None,
            reset: None,
        }
    }

    pub(super) fn description(mut self, text: impl Into<SharedString>) -> Row {
        self.description = Some(text.into());
        self
    }

    pub(super) fn control(mut self, control: impl IntoElement) -> Row {
        self.control = Some(control.into_any_element());
        self
    }

    pub(super) fn below(mut self, el: impl IntoElement) -> Row {
        self.below = Some(el.into_any_element());
        self
    }

    /// The reset button shows while `dirty`.
    pub(super) fn reset(
        mut self,
        dirty: bool,
        reset: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Row {
        self.reset = dirty.then(|| Rc::new(reset) as Reset);
        self
    }

    pub(super) fn render(self, ui: Ui, first: bool) -> impl IntoElement {
        let key = self.key;
        let reset = self.reset.map(|reset| {
            div()
                .id(ElementId::Name(format!("reset-{key}").into()))
                .flex()
                .items_center()
                .justify_center()
                .size(px(24.0))
                .rounded_full()
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .active(move |s| s.bg(ui.pressed))
                .child(
                    Icon::new(IconName::Undo2)
                        .size(px(13.0))
                        .text_color(ui.title_dim),
                )
                .on_click(move |_: &ClickEvent, window, cx| reset(window, cx))
                .test_support()
        });
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .px(px(14.0))
            .py(px(12.0))
            .when(!first, |el| el.border_t_1().border_color(ui.hairline))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(self.title),
                            )
                            .when_some(self.description, |el, text| {
                                el.child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(ui.title_dim)
                                        .child(text),
                                )
                            }),
                    )
                    .when_some(self.control, |el, control| {
                        el.child(
                            div()
                                .id(ElementId::Name(format!("field-{key}").into()))
                                .flex_shrink_0()
                                .child(control)
                                .test_support(),
                        )
                    })
                    // The reset button's place is kept, so controls line up.
                    .child(div().w(px(24.0)).flex_shrink_0().children(reset)),
            )
            .children(self.below)
    }
}

/// Removes `table.key` from config.toml: the default again.
pub(super) fn remove(
    entity: &Entity<Workspace>,
    table: &'static str,
    key: &'static str,
    to_default: impl Fn(&mut Config) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let entity = entity.clone();
    move |_, cx| {
        entity.update(cx, |ws, cx| {
            ws.edit(Edit::remove(table, key), &to_default, cx)
        })
    }
}

/// A button showing the current option, opening a menu of them; the current one is checked.
pub(super) fn menu_button(
    id: impl Into<ElementId>,
    options: Vec<(SharedString, SharedString)>,
    current: SharedString,
    scrollable: bool,
    pick: impl Fn(SharedString, &mut App) + 'static,
) -> impl IntoElement {
    let label = options
        .iter()
        .find(|(value, _)| *value == current)
        .map_or(current.clone(), |(_, label)| label.clone());
    let pick = Rc::new(pick);
    Button::new(id)
        .label(label)
        .dropdown_caret(true)
        .outline()
        .small()
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            options
                .iter()
                .fold(menu, |menu, (value, label)| {
                    let pick = pick.clone();
                    let value = value.clone();
                    menu.item(
                        PopupMenuItem::new(label.clone())
                            .checked(value == current)
                            .on_click(move |_, _, cx| pick(value.clone(), cx)),
                    )
                })
                .scrollable(scrollable)
        })
}

/// A text field's editor; each change goes to `set`.
struct TextState {
    input: Entity<InputState>,
    _change: Subscription,
}

/// A text field showing `value`; typing calls `set` with the whole text.
pub(super) fn text_input(
    entity: &Entity<Workspace>,
    key: &'static str,
    value: String,
    set: fn(&mut Workspace, &str, &mut Context<Workspace>),
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let value = SharedString::from(value);
    let e = entity.clone();
    let initial = value.clone();
    let state = window.use_keyed_state(
        SharedString::from(format!("text-{key}")),
        cx,
        move |window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx).default_value(initial));
            let _change = cx.subscribe(&input, move |_, input, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = input.read(cx).value();
                    e.update(cx, |ws, cx| set(ws, &text, cx));
                }
            });
            TextState { input, _change }
        },
    );
    // What the workspace shows wins over the field's text: a hand edit, a reset.
    state.update(cx, |state, cx| {
        if state.input.read(cx).value() != value {
            state
                .input
                .update(cx, |input, cx| input.set_value(value.clone(), window, cx));
        }
    });
    Input::new(&state.read(cx).input)
        .small()
        .w(px(200.0))
        .into_any_element()
}
