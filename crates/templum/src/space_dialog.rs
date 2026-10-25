//! A workspace's look (its icon, color and image) and the Edit… dialog that chooses it, with
//! its name and folder. See docs/specs/cockpit.md#6-the-workspaces-screen.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::LazyLock;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, ClickEvent, Context, ElementId, Entity, FontWeight, Hsla, PathPromptOptions,
    TestSupportExt, Window, div, px,
};

use crate::board::Label;
use crate::grid::hsla;
use crate::session;
use crate::share_dialog;
use crate::sidebar;
use crate::theme::{Theme, Ui};
use crate::workspace::{Workspace, show_notification};

/// The colors a workspace can take, by name: the cards' labels, and the accent.
pub(crate) const COLORS: [&str; 7] = [
    "red", "yellow", "green", "cyan", "blue", "magenta", "accent",
];

/// How a workspace shows: an image, else an icon, else its initials; its color tints them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Look {
    pub icon: Option<IconName>,
    /// One of COLORS.
    pub color: Option<String>,
    /// The image's file.
    pub image: Option<PathBuf>,
}

/// Every icon Templum embeds, by name ("git-branch"), in the catalog's order.
static ICONS: LazyLock<Vec<(String, IconName)>> = LazyLock::new(|| {
    IconName::ALL
        .iter()
        .map(|i| {
            let path = i.path();
            let name = path.strip_prefix("icons/").unwrap_or(&path);
            (name.strip_suffix(".svg").unwrap_or(name).to_string(), *i)
        })
        .collect()
});

/// The icon named `name`, among those Templum embeds.
pub(crate) fn icon(name: &str) -> Option<IconName> {
    ICONS.iter().find(|(n, _)| n == name).map(|(_, i)| *i)
}

/// An icon's name, as `icon` takes it.
pub(crate) fn icon_name(icon: IconName) -> &'static str {
    ICONS
        .iter()
        .find(|(_, i)| *i == icon)
        .map_or("", |(n, _)| n.as_str())
}

/// A color by its name, in `theme`.
pub(crate) fn tint(color: &str, theme: &Theme) -> Option<Hsla> {
    if color == "accent" {
        return Some(theme.ui.accent);
    }
    let label = Label::ALL.into_iter().find(|l| l.name() == color)?;
    Some(hsla(theme.palette.ansi[label.ansi()], 1.0))
}

/// The icons shown before anything is typed in the dialog's search.
pub(crate) const SUGGESTED: [&str; 24] = [
    "rocket",
    "folder",
    "code",
    "terminal",
    "server",
    "database",
    "globe",
    "bug",
    "flask-conical",
    "book",
    "briefcase",
    "house",
    "heart",
    "star",
    "zap",
    "cloud",
    "cpu",
    "package",
    "wrench",
    "test-tube",
    "gamepad-2",
    "music",
    "camera",
    "coffee",
];

/// The most icons a search shows: the grid is drawn every frame.
const FOUND_MAX: usize = 120;

/// The icons for a search: the suggested ones for none, else those whose name has the words.
fn find_icons(query: &str) -> Vec<IconName> {
    let query = query.trim().to_lowercase().replace(' ', "-");
    if query.is_empty() {
        return SUGGESTED.iter().filter_map(|n| icon(n)).collect();
    }
    ICONS
        .iter()
        .filter(|(n, _)| n.contains(&query))
        .map(|(_, i)| *i)
        .take(FOUND_MAX)
        .collect()
}

/// The Edit… dialog's state: what Save applies. Cancel drops it.
pub(crate) struct SpaceDialog {
    id: String,
    /// Where Sharing reads who mirrors, and what it revokes.
    workspace: Entity<Workspace>,
    name: Entity<InputState>,
    search: Entity<InputState>,
    look: Look,
    folder: Option<PathBuf>,
    /// Why the image chosen wasn't taken, or why Save was refused.
    note: Option<String>,
    /// The icons found, and the search they are for.
    found: (String, Vec<IconName>),
    theme: Rc<Theme>,
    ui: Ui,
}

impl SpaceDialog {
    fn pick_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let file = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(err)) => {
                    let message = format!("Couldn't open the file picker: {err}");
                    eprintln!("templum: {message}");
                    cx.update(|window, cx| {
                        show_notification(
                            window,
                            cx,
                            gpui_kit::component::notification::Notification::warning(message),
                        )
                    })
                    .ok();
                    None
                }
            };
            if let Some(file) = file {
                this.update(cx, |d, cx| d.take_image(file, cx)).ok();
            }
        })
        .detach();
    }

    /// The image chosen: shown in the preview, or refused with why.
    fn take_image(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        self.note = session::image_refused(&file);
        if self.note.is_none() {
            self.look.image = Some(file);
        }
        cx.notify();
    }

    fn pick_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity();
        crate::launcher::pick_folder(
            Rc::new(
                move |folder: &std::path::Path, _: &mut Window, cx: &mut App| {
                    this.update(cx, |d, cx| {
                        d.folder = Some(folder.to_path_buf());
                        cx.notify();
                    });
                    // No block is opened.
                    None
                },
            ),
            window,
            cx,
        );
    }

    fn label(&self, text: &'static str) -> impl IntoElement {
        div()
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(self.ui.title)
            .child(text)
    }

    /// Sharing: applied at once, not on Save. With no share, what sharing does and Share…; else a
    /// row per share with Revoke, and Share… for one more.
    fn sharing(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ui, theme) = (self.ui, self.theme.clone());
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let ws = self.workspace.read(cx);
        let rows: Vec<_> = ws
            .share_rows(&self.id)
            .into_iter()
            .map(|row| {
                let from = ws.mirrored_from(&row.id).unwrap_or_default();
                (row, from)
            })
            .collect();
        let space = self.id.clone();
        let share = {
            let workspace = self.workspace.clone();
            move |window: &mut Window, cx: &mut App| {
                let space = space.clone();
                workspace.update(cx, |ws, cx| {
                    share_dialog::open(ws, &space, window, cx);
                })
            }
        };
        let yellow = hsla(theme.palette.ansi[3], 1.0);
        let none = rows.is_empty();
        let list = rows.into_iter().map(|(row, from)| {
            let (first, second) = share_dialog::sharing_words(&row, &from, at);
            let expired = row.expires.is_some_and(|end| end <= at);
            let token = row.id[1..].parse::<u64>().unwrap_or(0);
            let workspace = self.workspace.clone();
            div()
                .id(ElementId::Name(format!("space-share-{}", row.id).into()))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .w_full()
                                .overflow_hidden()
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(ui.title)
                                .child(row.name.clone()),
                        )
                        .child(
                            div()
                                .id(ElementId::Name(
                                    format!("space-share-{}-first", row.id).into(),
                                ))
                                .w_full()
                                .overflow_hidden()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(if expired { yellow } else { ui.title_dim })
                                .child(first)
                                .test_support(),
                        )
                        .child(
                            div()
                                .id(ElementId::Name(
                                    format!("space-share-{}-last", row.id).into(),
                                ))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .overflow_hidden()
                                .text_size(px(11.0))
                                .text_color(ui.title_dim)
                                .when(row.mirrored, |el| {
                                    el.child(div().size(px(6.0)).rounded_full().bg(ui.accent))
                                })
                                .child(div().truncate().child(second))
                                .test_support(),
                        ),
                )
                .child(
                    Button::new(ElementId::Name(format!("space-revoke-{}", row.id).into()))
                        .label("Revoke")
                        .small()
                        .on_click(move |_: &ClickEvent, _, cx| {
                            workspace.update(cx, |ws, cx| ws.revoke_token(token, cx))
                        }),
                )
                .test_support()
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .when(none, |el| {
                el.child(
                    div()
                        .id("space-share-invite")
                        .text_size(px(12.0))
                        .text_color(ui.title_dim)
                        .child(share_dialog::INVITE)
                        .test_support(),
                )
            })
            .children(list)
            .child(
                div().flex().child(
                    Button::new("space-share")
                        .label("Share…")
                        .small()
                        .on_click(move |_: &ClickEvent, window, cx| share(window, cx)),
                ),
            )
    }

    fn swatches(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = self.ui;
        let fg = hsla(self.theme.palette.foreground, 1.0);
        let chosen = self.look.color.clone();
        let colors = std::iter::once(None).chain(COLORS.iter().map(|c| Some(c.to_string())));
        div().flex().gap(px(8.0)).children(colors.map(|color| {
            let fill = color.as_deref().and_then(|c| tint(c, &self.theme));
            let on = chosen == color;
            let id = format!("space-color-{}", color.as_deref().unwrap_or("none"));
            div()
                .id(ElementId::Name(id.into()))
                .size(px(22.0))
                .rounded_full()
                .cursor_pointer()
                .border_2()
                .border_color(if on { fg } else { ui.hairline })
                .when_some(fill, |el, fill| el.bg(fill))
                .on_click(cx.listener(move |d, _: &ClickEvent, _, cx| {
                    d.look.color = color.clone();
                    cx.notify();
                }))
                .test_support()
        }))
    }

    fn icons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = self.ui;
        let color = self
            .look
            .color
            .as_deref()
            .and_then(|c| tint(c, &self.theme))
            .unwrap_or(ui.title);
        let chosen = self.look.icon;
        let name = self.name.read(cx).value().to_string();
        let cell = |id: String, on: bool| {
            div()
                .id(ElementId::Name(id.into()))
                .flex()
                .items_center()
                .justify_center()
                .size(px(36.0))
                .rounded(px(8.0))
                .border_2()
                .cursor_pointer()
                .map(move |el| {
                    if on {
                        el.border_color(ui.accent).bg(ui.hover)
                    } else {
                        el.border_color(gpui_kit::transparent_black())
                            .hover(move |s| s.bg(ui.hover))
                    }
                })
        };
        // The first cell is no icon: the initials.
        let none = cell("space-icon-none".into(), chosen.is_none())
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(ui.title)
            .child(sidebar::initials(&name))
            .on_click(cx.listener(|d, _: &ClickEvent, _, cx| {
                d.look.icon = None;
                cx.notify();
            }))
            .test_support();
        let cells = self.found.1.iter().map(|&i| {
            cell(format!("space-icon-{}", icon_name(i)), chosen == Some(i))
                .child(Icon::new(i).size(px(18.0)).text_color(color))
                .on_click(cx.listener(move |d, _: &ClickEvent, _, cx| {
                    d.look.icon = Some(i);
                    cx.notify();
                }))
                .test_support()
        });
        div()
            .id("space-icons")
            .flex()
            .flex_wrap()
            .gap(px(4.0))
            .max_h(px(168.0))
            .overflow_y_scroll()
            .child(none)
            .children(cells)
    }
}

/// A button with no fill, the pointer's fills the rest of the interface has. (`ghost()` shows the
/// kit's accent under the pointer, which the accent's own text color is not meant to sit on.)
fn quiet(ui: Ui, cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .foreground(ui.title)
        .hover(ui.hover)
        .active(ui.pressed)
}

impl Render for SpaceDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = self.ui;
        let name = self.name.read(cx).value().to_string();
        let query = self.search.read(cx).value().to_string();
        if self.found.0 != query {
            self.found = (query.clone(), find_icons(&query));
        }
        let red = hsla(self.theme.palette.ansi[1], 1.0);
        let preview = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(sidebar::tile_face(&self.look, &name, &self.theme, ui))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if name.trim().is_empty() {
                        "Workspace".to_string()
                    } else {
                        name.trim().to_string()
                    }),
            );
        let image = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                Button::new("space-image-choose")
                    .label("Choose an image…")
                    .small()
                    .on_click(
                        cx.listener(|d, _: &ClickEvent, window, cx| d.pick_image(window, cx)),
                    ),
            )
            .when(self.look.image.is_some(), |el| {
                el.child(
                    Button::new("space-image-remove")
                        .label("Remove image")
                        .small()
                        .custom(quiet(ui, cx))
                        .on_click(cx.listener(|d, _: &ClickEvent, _, cx| {
                            d.look.image = None;
                            cx.notify();
                        })),
                )
            });
        let folder = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .id("space-folder")
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(ui.title)
                    .child(
                        self.folder
                            .as_ref()
                            .map_or("None: new terminals follow the settings".into(), |f| {
                                f.display().to_string()
                            }),
                    )
                    .test_support(),
            )
            .child(
                Button::new("space-folder-choose")
                    .label("Choose…")
                    .small()
                    .on_click(
                        cx.listener(|d, _: &ClickEvent, window, cx| d.pick_folder(window, cx)),
                    ),
            )
            .when(self.folder.is_some(), |el| {
                el.child(
                    Button::new("space-folder-clear")
                        .label("Clear")
                        .small()
                        .custom(quiet(ui, cx))
                        .on_click(cx.listener(|d, _: &ClickEvent, _, cx| {
                            d.folder = None;
                            cx.notify();
                        })),
                )
            });
        div()
            .id("space-dialog")
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(preview)
            .child(self.label("Name"))
            .child(Input::new(&self.name))
            .child(self.label("Color"))
            .child(self.swatches(cx))
            .child(self.label("Icon"))
            .child(Input::new(&self.search))
            .child(self.icons(cx))
            .child(self.label("Image"))
            .child(image)
            .child(self.label("Folder"))
            .child(folder)
            .child(self.label("Sharing"))
            .child(self.sharing(cx))
            .children(self.note.clone().map(|note| {
                div()
                    .id("space-note")
                    .text_size(px(12.0))
                    .text_color(red)
                    .child(note)
                    .test_support()
            }))
            .test_support()
    }
}

/// Opens the Edit… dialog of workspace `id`: its name selected, every change shown in the
/// preview, applied by Save (or Enter), dropped by Cancel.
pub(crate) fn open(ws: &Workspace, id: &str, window: &mut Window, cx: &mut Context<Workspace>) {
    let Some(space) = ws.spaces().find(|s| s.id == id) else {
        return;
    };
    let initial = space.name.clone();
    let name = cx.new(|cx| InputState::new(window, cx).default_value(initial));
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Find an icon"));
    let workspace = cx.entity();
    let view = cx.new(|cx| {
        // Who mirrors, and the API's state, change under the dialog: it draws what is now.
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        SpaceDialog {
            id: id.to_string(),
            workspace: workspace.clone(),
            name: name.clone(),
            search: search.clone(),
            look: space.look.clone(),
            folder: space.folder.clone(),
            note: None,
            found: (String::new(), find_icons("")),
            theme: ws.theme.clone(),
            ui: ws.ui,
        }
    });
    for field in [&name, &search] {
        // Weak: the view owns the fields, and the dialog's end drops them all.
        let view = view.downgrade();
        App::subscribe(cx, field, move |_, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                view.update(cx, |_, cx| cx.notify()).ok();
            }
        })
        .detach();
    }
    let shown = view.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (view, workspace) = (view.clone(), workspace.clone());
        alert
            .confirm()
            .title("Edit workspace")
            .w(px(480.0))
            .button_props(DialogButtonProps::default().ok_text("Save"))
            .child(shown.clone())
            .on_ok(move |_, window, cx| save(&view, &workspace, window, cx))
    });
    // After opening: the dialog takes the focus when it opens.
    name.update(cx, |input, cx| {
        input.focus(window, cx);
        input.select_all(window, cx);
    });
}

/// Applies the dialog; false (it stays open, saying why) when it can't.
fn save(
    view: &Entity<SpaceDialog>,
    workspace: &Entity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let d = view.read(cx);
    let (id, name, look, folder) = (
        d.id.clone(),
        d.name.read(cx).value().to_string(),
        d.look.clone(),
        d.folder.clone(),
    );
    match workspace.update(cx, |ws, cx| {
        ws.edit_space(&id, &name, look, folder, window, cx)
    }) {
        Ok(()) => true,
        Err(why) => {
            view.update(cx, |d, cx| {
                d.note = Some(why);
                cx.notify();
            });
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_and_colors_are_found_by_name() {
        assert_eq!(icon("git-branch"), Some(IconName::GitBranch));
        assert_eq!(icon_name(IconName::GitBranch), "git-branch");
        assert_eq!(icon("no-such-icon"), None);
        let theme = Theme::new("t", templum_terminal::Palette::default(), None, None);
        assert_eq!(tint("accent", &theme), Some(theme.ui.accent));
        assert!(COLORS.iter().all(|c| tint(c, &theme).is_some()));
        assert_eq!(tint("pink", &theme), None);
    }

    #[test]
    fn the_suggested_icons_all_exist() {
        for name in SUGGESTED {
            assert!(icon(name).is_some(), "{name}");
        }
        assert_eq!(find_icons("").len(), SUGGESTED.len());
        let git = find_icons("git");
        assert!(git.contains(&IconName::GitBranch) && git.contains(&IconName::GitCommitHorizontal));
        assert_eq!(find_icons("a").len(), FOUND_MAX);
        assert!(find_icons("no such icon").is_empty());
    }
}
