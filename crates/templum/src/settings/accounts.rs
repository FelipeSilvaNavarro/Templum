//! Settings ▸ Agents ▸ Accounts: a row per account, "Add an account", and the dialog that adds or
//! edits one (docs/specs/accounts.md, Settings ▸ Agents ▸ Accounts).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, FontWeight, SharedString,
    Subscription, TestSupportExt, Window, div, px,
};
use templum_usage::accounts::{Found, Kind};

use super::row::{Row, menu_button};
use crate::agents::{AccountDraft, AgentEntry, new_account_id, own_state, tilde};
use crate::theme::Ui;
use crate::workspace::{Workspace, show_notification};

/// The Accounts heading, its rows and "Add an account", under the agents.
pub(super) fn accounts(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = ws.ui;
    let rows: Vec<Row> = ws
        .agents
        .iter()
        .filter(|a| a.account.is_some())
        .map(|a| account_row(ws, entity, a))
        .collect();
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .mt(px(14.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Accounts"),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(ui.title_dim)
                .child("Another login of an agent, with its own folder: choose what it shares with the others."),
        )
        .when(!rows.is_empty(), |el| el.child(super::card(ui, rows)))
        .child(
            div()
                .id("add-account")
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(14.0))
                .py(px(10.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(ui.hairline)
                .text_size(px(13.0))
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .active(move |s| s.bg(ui.pressed))
                .child(Icon::new(IconName::Plus).size(px(14.0)))
                .child("Add an account")
                .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| {
                    account_dialog(ws, None, window, cx)
                }))
                .test_support(),
        )
        .into_any_element()
}

/// The agent an account is of, by its title ("Claude Code").
fn agent_title(agents: &[AgentEntry], kind: &Kind) -> String {
    agents
        .iter()
        .find(|a| a.id == kind.agent)
        .map_or_else(|| kind.agent.to_string(), |a| a.title.clone())
}

/// One account: its agent and folder; whether the canvas's menu lists it; Edit.
fn account_row(ws: &Workspace, entity: &Entity<Workspace>, entry: &AgentEntry) -> Row {
    let ui = ws.ui;
    let id = entry.id.clone();
    let account = entry.account.as_ref().expect("an account");
    let description = format!(
        "Account of {} · {}",
        agent_title(&ws.agents, account.kind),
        tilde(&account.folder)
    );
    let show = {
        let (e, entry) = (entity.clone(), entry.clone());
        Switch::new(ElementId::Name(format!("show-account-{id}").into()))
            .checked(entry.show)
            .on_click(move |on: &bool, window, cx| {
                let draft = AccountDraft {
                    show: *on,
                    ..AccountDraft::of(&entry)
                };
                let id = entry.id.clone();
                e.update(cx, |ws, cx| {
                    ws.save_account(Some(&id), &draft, window, cx).ok()
                });
            })
    };
    let pill = |name: String, label: &'static str| {
        div()
            .id(ElementId::Name(name.into()))
            .px(px(10.0))
            .py(px(3.0))
            .rounded_full()
            .border_1()
            .border_color(ui.hairline)
            .text_size(px(12.0))
            .cursor_pointer()
            .hover(move |s| s.bg(ui.hover))
            .active(move |s| s.bg(ui.pressed))
            .child(label)
    };
    let log_in = {
        let (e, id) = (entity.clone(), id.clone());
        pill(format!("log-in-{id}"), "Log in")
            .on_click(move |_: &ClickEvent, window, cx| {
                let id = id.clone();
                e.update(cx, |ws, cx| {
                    ws.close_settings(window, cx);
                    ws.log_in(&id, window, cx);
                })
            })
            .test_support()
    };
    let edit = {
        let (e, entry) = (entity.clone(), entry.clone());
        pill(format!("edit-account-{id}"), "Edit")
            .on_click(move |_: &ClickEvent, window, cx| {
                let entry = entry.clone();
                e.update(cx, |ws, cx| account_dialog(ws, Some(entry), window, cx))
            })
            .test_support()
    };
    Row::new(format!("accounts.{id}"), entry.title.clone())
        .description(description)
        .control(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(show)
                .child(log_in)
                .child(edit),
        )
}

/// A line for each variable that wins over every account's login that Templum's environment
/// has (`set`): the person's shell may set one too.
fn override_notes(kind: &Kind, agent: &str, set: impl Fn(&str) -> bool) -> Vec<String> {
    kind.overrides
        .iter()
        .filter(|name| set(name))
        .map(|name| {
            format!(
                "{name} is set in your environment: {agent} uses it whatever this account's login."
            )
        })
        .collect()
}

/// The agents an account can be of, installed first: their ids and titles.
pub(crate) fn agent_choices(agents: &[AgentEntry]) -> Vec<(String, String)> {
    let mut choices: Vec<(bool, String, String)> = templum_usage::accounts::kinds()
        .iter()
        .filter_map(|kind| {
            let agent = agents.iter().find(|a| a.id == kind.agent)?;
            let installed = templum_terminal::installed(agent.program());
            Some((!installed, agent.id.clone(), agent.title.clone()))
        })
        .collect();
    choices.sort_by_key(|(missing, ..)| *missing);
    choices
        .into_iter()
        .map(|(_, id, title)| (id, title))
        .collect()
}

/// The dialog's state: what it was opened with, and the choices made in it.
pub(crate) struct Form {
    /// The account edited; None: a new one.
    id: Option<String>,
    agents: Vec<AgentEntry>,
    data: Option<PathBuf>,
    ui: Ui,
    kind: Option<&'static Kind>,
    name: Entity<InputState>,
    folder: Entity<InputState>,
    /// The folder it had, for one edited.
    was: Option<PathBuf>,
    share: BTreeMap<String, String>,
    /// Items holding a link the person made, and where it points.
    theirs: BTreeMap<&'static str, PathBuf>,
    /// The name a chosen agent gives, put in its field at the next render.
    named: Option<String>,
    ws: gpui_kit::WeakEntity<Workspace>,
    red: gpui_kit::Hsla,
    _folder_change: Subscription,
}

impl Form {
    fn new(
        ws: &Workspace,
        owner: gpui_kit::WeakEntity<Workspace>,
        entry: Option<&AgentEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = entry.map(AccountDraft::of);
        let name_text = draft.as_ref().map(|d| d.name.clone()).unwrap_or_default();
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("What the menu shows")
                .default_value(name_text)
        });
        let folder_text = draft
            .as_ref()
            .and_then(|d| d.folder.clone())
            .unwrap_or_default();
        let folder = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Empty: Templum's folder for it")
                .default_value(folder_text)
        });
        let _folder_change = cx.subscribe(&folder, |form: &mut Form, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                form.read_folder(cx);
            }
        });
        let account = entry.and_then(|e| e.account.as_ref());
        let mut form = Form {
            id: entry.map(|e| e.id.clone()),
            agents: ws.agents.clone(),
            data: ws.data_dir.clone(),
            ui: ws.ui,
            kind: account.map(|a| a.kind),
            name,
            folder,
            was: account.map(|a| a.folder.clone()),
            share: draft.map(|d| d.share).unwrap_or_default(),
            theirs: BTreeMap::new(),
            named: None,
            ws: owner,
            red: crate::grid::hsla(ws.theme.palette.ansi[1], 1.0),
            _folder_change,
        };
        form.read_links(cx);
        form
    }

    /// Its id: the account's, or the one a new account of its agent gets.
    fn id(&self) -> Option<String> {
        if let Some(id) = &self.id {
            return Some(id.clone());
        }
        let kind = self.kind?;
        Some(new_account_id(kind.agent, self.data.as_deref(), |x| {
            self.agents.iter().any(|a| a.id == x)
        }))
    }

    /// The folder its agent would run on: the one typed, else Templum's.
    fn folder_path(&self, cx: &App) -> Option<PathBuf> {
        let typed = self.folder.read(cx).value().trim().to_string();
        if !typed.is_empty() {
            return Some(crate::launcher::expand(&typed));
        }
        Some(self.data.as_ref()?.join("accounts").join(self.id()?))
    }

    /// Who an item may be shared with: its agent's own folder, then its other accounts, by id,
    /// with their state folders.
    fn sources(&self) -> Vec<(String, Option<PathBuf>)> {
        let Some(kind) = self.kind else {
            return Vec::new();
        };
        let me = self.id();
        let accounts = self.agents.iter().filter_map(|a| {
            let account = a.account.as_ref()?;
            (account.kind.agent == kind.agent && Some(&a.id) != me.as_ref())
                .then(|| (a.id.clone(), Some(account.kind.state_dir(&account.folder))))
        });
        std::iter::once((kind.agent.to_string(), own_state(kind, &self.agents)))
            .chain(accounts)
            .collect()
    }

    /// The choices a folder already holds: a link to the same item of one of its agent's
    /// folders becomes that choice, one elsewhere is the person's. Only links add choices: a
    /// folder without one (an empty one, or `/` while its path is typed) changes none.
    fn read_links(&mut self, cx: &App) {
        let (Some(kind), Some(folder)) = (self.kind, self.folder_path(cx)) else {
            return;
        };
        self.theirs.clear();
        if !folder.is_dir() {
            return;
        }
        let sources = self.sources();
        let paths: Vec<PathBuf> = sources.iter().filter_map(|(_, p)| p.clone()).collect();
        for (key, found) in templum_usage::accounts::read(kind, &folder, &paths) {
            match found {
                Found::Theirs(target) => {
                    self.theirs.insert(key, target);
                }
                Found::Linked(state) => {
                    if let Some((id, _)) = sources.iter().find(|(_, p)| p.as_ref() == Some(&state))
                    {
                        self.share.insert(key.to_string(), id.clone());
                    }
                }
                Found::Own => {}
            }
        }
    }

    /// The folder field changed: a folder set up by hand keeps its sharing.
    fn read_folder(&mut self, cx: &mut Context<Self>) {
        self.read_links(cx);
        cx.notify();
    }

    /// The agent of a new account: its name is that agent's title and the id's number.
    fn choose(&mut self, agent: &str, cx: &mut Context<Self>) {
        let Some(kind) = templum_usage::accounts::kind(agent) else {
            return;
        };
        if self.kind == Some(kind) {
            return;
        }
        self.kind = Some(kind);
        self.share.clear();
        let id = self.id().unwrap_or_default();
        let n = id.strip_prefix(kind.agent).unwrap_or_default();
        self.named = Some(format!("{} {n}", agent_title(&self.agents, kind)));
        self.read_links(cx);
        cx.notify();
    }

    /// What Save writes.
    fn draft(&self, cx: &App) -> Option<AccountDraft> {
        let kind = self.kind?;
        let folder = self.folder.read(cx).value().trim().to_string();
        let show = self
            .id
            .as_ref()
            .and_then(|id| self.agents.iter().find(|a| a.id == *id))
            .is_none_or(|a| a.show);
        Some(AccountDraft {
            of: kind.agent.to_string(),
            name: self.name.read(cx).value().to_string(),
            show,
            folder: (!folder.is_empty()).then_some(folder),
            share: self.share.clone(),
        })
    }
}

impl Render for Form {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(name) = self.named.take() {
            self.name
                .update(cx, |input, cx| input.set_value(name, window, cx));
        }
        let ui = self.ui;
        let label = |text: String| {
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .text_color(ui.title_dim)
                .child(text)
        };
        let note = |text: String| {
            div()
                .text_size(px(11.0))
                .text_color(ui.title_dim)
                .child(text)
        };
        let form = cx.entity();
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(label("Agent".into()));
        column = match (self.id.is_some(), self.kind) {
            (true, Some(kind)) => column.child(
                div()
                    .text_size(px(13.0))
                    .child(agent_title(&self.agents, kind)),
            ),
            _ => {
                let f = form.clone();
                let options = agent_choices(&self.agents)
                    .into_iter()
                    .map(|(id, title)| (id.into(), title.into()))
                    .collect();
                let current: SharedString = self.kind.map_or("Choose an agent", |k| k.agent).into();
                column.child(menu_button(
                    "account-agent",
                    options,
                    current,
                    false,
                    move |id, cx| f.update(cx, |f, cx| f.choose(&id, cx)),
                ))
            }
        };
        let Some(kind) = self.kind else {
            return column;
        };
        let id = self.id().unwrap_or_default();
        let agent = agent_title(&self.agents, kind);
        column = column
            .child(label("Name".into()))
            .child(
                div()
                    .id("account-name")
                    .child(Input::new(&self.name))
                    .test_support(),
            )
            .child(note(format!(
                "Its id: {id} (what agents and the canvas file call it)"
            )))
            .child(label("Folder".into()));
        let typed = !self.folder.read(cx).value().trim().is_empty();
        if !typed && let Some(data) = &self.data {
            column = column.child(note(format!(
                "Templum's folder for it ({})",
                tilde(&data.join("accounts").join(&id))
            )));
        }
        let pick = {
            let f = form.clone();
            Button::new("account-choose-folder")
                .label("Choose…")
                .small()
                .outline()
                .on_click(move |_, window, cx| {
                    let f = f.clone();
                    let open: Rc<crate::launcher::Open> = Rc::new(move |folder, window, cx| {
                        let text = tilde(folder);
                        f.update(cx, |f, cx| {
                            f.folder
                                .update(cx, |input, cx| input.set_value(text, window, cx));
                        });
                        None
                    });
                    crate::launcher::pick_folder(open, window, cx);
                })
        };
        column = column.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .id("account-folder")
                        .flex_1()
                        .child(Input::new(&self.folder))
                        .test_support(),
                )
                .child(pick),
        );
        if let (Some(was), Some(now)) = (&self.was, self.folder_path(cx))
            && *was != now
        {
            column = column.child(note(format!(
                "Its old folder stays ({}). Log in again: a login is its folder's.",
                tilde(was)
            )));
        }
        let sources = self.sources();
        let source_label = |id: &str| {
            if id == kind.agent {
                format!("{agent}: its own folder")
            } else {
                self.agents
                    .iter()
                    .find(|a| a.id == id)
                    .map_or_else(|| id.to_string(), |a| a.title.clone())
            }
        };
        let everyone: Vec<(SharedString, SharedString)> = sources
            .iter()
            .map(|(id, _)| (id.clone().into(), source_label(id).into()))
            .collect();
        if !kind.items.is_empty() {
            let f = form.clone();
            column = column
                .child(label("Share everything with".into()))
                .child(menu_button(
                    "account-share-all",
                    everyone.clone(),
                    "Choose…".into(),
                    false,
                    move |to, cx| {
                        f.update(cx, |f, cx| {
                            let Some(kind) = f.kind else { return };
                            for item in kind.items {
                                f.share.insert(item.key.to_string(), to.to_string());
                            }
                            cx.notify();
                        })
                    },
                ));
        }
        let files_share = templum_usage::accounts::files_share();
        let mut items = div()
            .id("account-items")
            .flex()
            .flex_col()
            .gap(px(4.0))
            .max_h(px(260.0))
            .overflow_y_scroll();
        for item in kind.items {
            let paths = item.paths.join(", ");
            let file = !item.paths[0].ends_with('/');
            let current = self.share.get(item.key).cloned().unwrap_or_default();
            let choice: AnyElement = if let Some(target) = self.theirs.get(item.key) {
                note(format!("Linked by you to {}", tilde(target))).into_any_element()
            } else if file && !files_share {
                note(templum_usage::accounts::NO_FILE_LINKS.to_string()).into_any_element()
            } else {
                let f = form.clone();
                let key = item.key;
                let options = std::iter::once(("".into(), "Own".into()))
                    .chain(everyone.iter().cloned())
                    .collect();
                menu_button(
                    ElementId::Name(format!("account-share-{key}").into()),
                    options,
                    current.clone().into(),
                    false,
                    move |to, cx| {
                        f.update(cx, |f, cx| {
                            if to.is_empty() {
                                f.share.remove(key);
                            } else {
                                f.share.insert(key.to_string(), to.to_string());
                            }
                            cx.notify();
                        })
                    },
                )
                .into_any_element()
            };
            // A file the source hasn't written yet is linked once it has.
            let waiting = file
                && !current.is_empty()
                && sources
                    .iter()
                    .find(|(id, _)| *id == current)
                    .and_then(|(_, dir)| dir.as_ref())
                    .is_some_and(|dir| !dir.join(item.paths[0]).exists());
            items = items.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(13.0)).child(item.label))
                            .child(note(if waiting {
                                format!("{paths} · Shared once {agent} writes one")
                            } else {
                                paths
                            })),
                    )
                    .child(choice),
            );
        }
        column = column.child(items);
        let set = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        for line in override_notes(kind, &agent, set) {
            column = column.child(note(line));
        }
        if let Some(text) = kind.note {
            column = column.child(note(text.to_string()));
        }
        let column = column.child(note(
            "Its login is never shared: log in to it with Log in.".to_string(),
        ));
        let Some(id) = self.id.clone() else {
            return column;
        };
        let (ws, red) = (self.ws.clone(), self.red);
        column.child(
            div()
                .id("delete-account")
                .mt(px(8.0))
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .text_color(red)
                .cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .child("Delete this account")
                .on_click(move |_: &ClickEvent, window, cx| {
                    ask_delete(&ws, &id, window, cx);
                })
                .test_support(),
        )
    }
}

/// Asks before deleting the account `id` (docs/specs/accounts.md, Deleting an account), then
/// deletes it and closes its dialog, or says why it can't.
fn ask_delete(ws: &gpui_kit::WeakEntity<Workspace>, id: &str, window: &mut Window, cx: &mut App) {
    let Some(entry) = ws
        .upgrade()
        .and_then(|ws| ws.read(cx).agents.iter().find(|a| a.id == id).cloned())
    else {
        return;
    };
    let Some(account) = &entry.account else {
        return;
    };
    let mut lines = vec![if account.chosen {
        format!("Its folder {} stays.", tilde(&account.folder))
    } else {
        "Its folder is deleted: its login and everything it doesn't share. What it shares stays \
         where it is."
            .to_string()
    }];
    let keychain = match account.kind.keychain {
        templum_usage::accounts::Keychain::Always => true,
        templum_usage::accounts::Keychain::MacOs => cfg!(target_os = "macos"),
        templum_usage::accounts::Keychain::Never => false,
    };
    if keychain {
        lines.push(
            "Its login stays in the system's keychain: run /logout in it first to remove it."
                .to_string(),
        );
    }
    let (ws, id) = (ws.clone(), id.to_string());
    crate::prompt::confirm(
        format!("Delete {}?", entry.title),
        lines.join(" "),
        "Delete",
        move |window, cx| {
            let deleted = ws.update(cx, |ws, cx| ws.delete_account(&id, cx));
            match deleted {
                Ok(Ok(())) => window.close_all_dialogs(cx),
                Ok(Err(why)) => show_notification(window, cx, Notification::warning(why)),
                Err(_) => {}
            }
        },
        window,
        cx,
    );
}

/// Editing an account (`entry`), or adding one (None), in a dialog; Save writes its entry and
/// applies its choices to its folder.
pub(crate) fn account_dialog(
    ws: &Workspace,
    entry: Option<AgentEntry>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let owner = cx.weak_entity();
    let form = cx.new(|cx| Form::new(ws, owner, entry.as_ref(), window, cx));
    let entity = cx.entity();
    let title = match &entry {
        Some(entry) => format!("Edit {}", entry.title),
        None => "Add an account".to_string(),
    };
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (form, entity) = (form.clone(), entity.clone());
        alert
            .confirm()
            .title(title.clone())
            .w(px(520.0))
            .button_props(DialogButtonProps::default().ok_text("Save"))
            .child(form.clone())
            .on_ok(move |_, window, cx| {
                let (id, draft) = form.read_with(cx, |f, cx| (f.id.clone(), f.draft(cx)));
                let Some(draft) = draft else {
                    show_notification(window, cx, Notification::warning("Choose its agent first"));
                    return false;
                };
                let saved = entity.update(cx, |ws, cx| {
                    ws.save_account(id.as_deref(), &draft, window, cx)
                });
                match saved {
                    Ok(_) => true,
                    Err(problem) => {
                        show_notification(window, cx, Notification::warning(problem));
                        false
                    }
                }
            })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_variable_that_wins_over_the_login_is_named() {
        let copilot = templum_usage::accounts::kind("copilot").unwrap();
        let notes = override_notes(copilot, "Copilot CLI", |name| name == "GH_TOKEN");
        assert_eq!(
            notes,
            [
                "GH_TOKEN is set in your environment: Copilot CLI uses it whatever this account's login."
            ]
        );
        let claude = templum_usage::accounts::kind("claude").unwrap();
        assert!(override_notes(claude, "Claude Code", |_| false).is_empty());
        assert_eq!(
            override_notes(claude, "Claude Code", |_| true).len(),
            claude.overrides.len()
        );
    }
}
