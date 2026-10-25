//! Connecting to another Templum: the Connect to a Templum… dialog
//! (docs/specs/templum-to-templum.md, On the mirror: connecting). The code leaves the field the
//! moment Connect is pressed, whatever comes of it.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable as _, Sizable as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, Entity, FontWeight, SharedString, TestSupportExt, Window, div, px};
use templum_api::client::{proxy_from_env, proxy_of};
use templum_api::code::{self, Code, Key};

use crate::grid::hsla;
use crate::theme::{Theme, Ui};
use crate::workspace::Workspace;
use crate::workspace::remote::{Act, Connecting};

/// Where the key goes when a keyring takes it, each platform with its own name for it.
fn keyring_words() -> &'static str {
    if cfg!(target_os = "macos") {
        "The key is kept in this computer's keyring (the Keychain)."
    } else if cfg!(windows) {
        "The key is kept in this computer's keyring (the Credential Manager)."
    } else {
        "The key is kept in this computer's keyring (the Secret Service)."
    }
}

const NO_KEYRING: &str =
    "This computer has no keyring Templum can use: the connection lasts until Templum quits.";
const FILE_SWITCH: &str = "Keep the key in a file only you can read";
const FILE_WARNING: &str = "A copy of the data folder would carry it.";

/// The code's host without its port, for the environment's proxy and `NO_PROXY`.
fn bare_host(code: &Code) -> String {
    let host = code.host();
    match host.rsplit_once(':') {
        Some((name, port)) if !name.ends_with(']') || port.chars().all(|c| c.is_ascii_digit()) => {
            name.trim_matches(['[', ']']).to_string()
        }
        _ => host,
    }
}

/// What the dialog shows and holds.
pub(crate) struct ConnectFlow {
    workspace: Entity<Workspace>,
    code: Entity<InputState>,
    client_id: Entity<InputState>,
    client_secret: Entity<InputState>,
    proxy: Entity<InputState>,
    /// More… is unfolded.
    more: bool,
    /// The proxy was prefilled from the environment (once, for the first code read).
    prefilled: bool,
    /// Keep the key in a file.
    file: bool,
    /// Connect was pressed and the remote has not said yet.
    connecting: bool,
    /// The remote a new code is for (Paste a new code…), else the dialog makes a new one.
    replace: Option<String>,
    error: Option<String>,
    theme: std::rc::Rc<Theme>,
    ui: Ui,
}

impl ConnectFlow {
    /// The code in the field, if it reads.
    fn decoded(&self, cx: &App) -> Option<Code> {
        code::decode(&self.code.read(cx).value()).ok()
    }

    fn dim(&self, id: &'static str, text: impl Into<SharedString>) -> impl IntoElement {
        div()
            .id(id)
            .w_full()
            .overflow_hidden()
            .text_size(px(12.0))
            .text_color(self.ui.title_dim)
            .child(text.into())
            .test_support()
    }

    fn label(&self, text: &'static str) -> impl IntoElement {
        div()
            .mt(px(4.0))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(self.ui.title)
            .child(text)
    }

    /// Connect: the code leaves the field, and the workspace makes the remote.
    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(code) = self.decoded(cx) else {
            return;
        };
        let text = |input: &Entity<InputState>, cx: &App| {
            let value = input.read(cx).value().trim().to_string();
            (!value.is_empty()).then_some(value)
        };
        // (A new code for a remote that left the secret empty keeps the one it has.)
        let secret = text(&self.client_secret, cx).map(Key::new);
        let access = text(&self.client_id, cx).map(|id| (id, secret));
        let proxy = text(&self.proxy, cx);
        let draft = Connecting {
            code,
            access,
            proxy: proxy.clone().map(Key::new),
            file: self.file,
        };
        self.code
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.client_secret
            .update(cx, |input, cx| input.set_value("", window, cx));
        // The proxy stays for a second try, without the login it may carry.
        if let Some((address, _)) = proxy.and_then(|p| proxy_of(&p).ok()) {
            self.proxy
                .update(cx, |input, cx| input.set_value(address, window, cx));
        }
        // A new code for a remote that is already here: the banner tells how it goes.
        if let Some(id) = self.replace.clone() {
            let done = self
                .workspace
                .update(cx, |ws, cx| ws.replace_code(&id, draft, window, cx));
            match done {
                Ok(()) => window.close_dialog(cx),
                Err(why) => self.error = Some(why),
            }
            cx.notify();
            return;
        }
        let made = self
            .workspace
            .update(cx, |ws, cx| ws.connect_code(draft, window, cx));
        match made {
            Ok(_) => {
                self.connecting = true;
                self.error = None;
            }
            Err(why) => self.error = Some(why),
        }
        cx.notify();
    }

    /// The workspace said how the remote went.
    fn heard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.connecting {
            return;
        }
        match self.workspace.read(cx).connect_result() {
            Some(Ok(())) => window.close_dialog(cx),
            Some(Err(why)) => {
                self.connecting = false;
                self.error = Some(why);
            }
            None => return,
        }
        cx.notify();
    }
}

impl Render for ConnectFlow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let decoded = self.decoded(cx);
        let typed = self.code.read(cx).value();
        // What the field holds, or what is wrong with it.
        let line = match (&decoded, code::decode(&typed)) {
            (Some(code), _) => Some(code.said()),
            (None, Err(wrong)) if !typed.trim().is_empty() => Some(wrong.to_string()),
            _ => None,
        };
        let red = hsla(self.theme.palette.ansi[1], 1.0);
        let keyring = self.workspace.read(cx).keyring;
        let this = cx.entity();
        let more = self.more;
        let toggle = this.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.label("Connection code"))
            .child(Input::new(&self.code))
            .children(line.map(|line| {
                let wrong = decoded.is_none();
                div()
                    .id("connect-says")
                    .w_full()
                    .overflow_hidden()
                    .text_size(px(12.0))
                    .text_color(if wrong { red } else { self.ui.title_dim })
                    .child(line)
                    .test_support()
            }))
            .child(
                Button::new("connect-more")
                    .label(if more { "Less…" } else { "More…" })
                    .small()
                    .on_click(move |_, _, cx| {
                        toggle.update(cx, |flow, cx| {
                            flow.more = !flow.more;
                            cx.notify();
                        })
                    }),
            )
            .when(more, |el| {
                el.child(self.label("Cloudflare Access service token"))
                    .child(Input::new(&self.client_id))
                    .child(Input::new(&self.client_secret))
                    .child(self.label("Proxy"))
                    .child(Input::new(&self.proxy))
            })
            .child(match keyring {
                Some(false) => {
                    let file = self.file;
                    let toggle = this.clone();
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(self.dim("connect-keyring", NO_KEYRING))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Switch::new("connect-file").checked(file).on_click(
                                    move |on: &bool, _, cx| {
                                        let on = *on;
                                        toggle.update(cx, |flow, cx| {
                                            flow.file = on;
                                            cx.notify();
                                        })
                                    },
                                ))
                                .child(div().text_size(px(12.0)).child(FILE_SWITCH)),
                        )
                        .when(file, |el| {
                            el.child(self.dim("connect-file-says", FILE_WARNING))
                        })
                }
                _ => div().child(self.dim("connect-keyring", keyring_words())),
            })
            .children(
                self.connecting
                    .then(|| self.dim("connect-connecting", "Connecting…")),
            )
            .children(self.workspace.read(cx).connect_choice().map(|words| {
                // A certificate from an authority this computer added: the person decides.
                let choose = |id: &'static str, label: &'static str, act: Act| {
                    let workspace = self.workspace.clone();
                    Button::new(id)
                        .label(label)
                        .small()
                        .on_click(move |_, window, cx| {
                            workspace.update(cx, |ws, cx| {
                                if let Some(id) = ws.pending_connect.clone() {
                                    ws.remote_act(&id, act, window, cx);
                                }
                            })
                        })
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("connect-added")
                            .w_full()
                            .text_size(px(12.0))
                            .text_color(red)
                            .child(words)
                            .test_support(),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(choose("connect-dont", "Don't connect", Act::DontConnect))
                            .child(choose(
                                "connect-anyway",
                                "Connect anyway",
                                Act::ConnectAnyway,
                            )),
                    )
            }))
            .children(self.error.clone().map(|why| {
                div()
                    .id("connect-error")
                    .w_full()
                    .overflow_hidden()
                    .text_size(px(12.0))
                    .text_color(red)
                    .child(why)
                    .test_support()
            }))
    }
}

/// Connect to a Templum…: the code's field, then Connect.
pub(crate) fn open(ws: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    open_as(ws, None, window, cx);
}

/// Paste a new code… for the remote workspace `id`: the same dialog, which replaces that
/// connection's code instead of making a workspace.
pub(crate) fn open_for(
    ws: &mut Workspace,
    id: &str,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    open_as(ws, Some(id.to_string()), window, cx);
}

fn open_as(
    ws: &mut Workspace,
    replace: Option<String>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    ws.probe_keyring(window, cx);
    let title = match replace
        .as_deref()
        .and_then(|id| ws.spaces().find(|s| s.id == id))
    {
        Some(space) => format!(
            "Paste a new code for {}",
            space.remote.as_deref().map_or_else(
                || space.name.clone(),
                |r| format!("{} on {}", space.name, crate::mirror::computer_of(r))
            )
        ),
        None => "Connect to a Templum…".to_string(),
    };
    let workspace = cx.entity();
    let code = cx.new(|cx| InputState::new(window, cx).placeholder("tplc1_…"));
    let client_id = cx.new(|cx| InputState::new(window, cx).placeholder("Client ID"));
    let secret_says = match replace {
        Some(_) => "Client Secret (empty keeps it)",
        None => "Client Secret",
    };
    let client_secret = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(secret_says)
            .masked(true)
    });
    let proxy = cx.new(|cx| InputState::new(window, cx).placeholder("http://proxy.corp:8080"));
    // A new code for a remote starts from what that remote reaches home with; the secrets are
    // not shown, and left empty they stay (`replace_code`).
    let (old_id, old_proxy) = replace
        .as_deref()
        .and_then(|id| ws.spaces().find(|s| s.id == id))
        .and_then(|space| space.remote.as_deref())
        .map(|r| (r.entry.access_id.clone(), r.entry.proxy.clone()))
        .unwrap_or_default();
    let more = old_id.is_some() || old_proxy.is_some();
    for (input, value) in [(&client_id, old_id), (&proxy, old_proxy.clone())] {
        if let Some(value) = value {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }
    let view = cx.new(|cx| {
        // The remote's progress and the keyring's answer change what the dialog says.
        cx.observe_in(
            &workspace,
            window,
            |flow: &mut ConnectFlow, _, window, cx| {
                flow.heard(window, cx);
                cx.notify();
            },
        )
        .detach();
        ConnectFlow {
            workspace: workspace.clone(),
            code: code.clone(),
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            proxy: proxy.clone(),
            more,
            prefilled: old_proxy.is_some(),
            file: false,
            connecting: false,
            replace,
            error: None,
            theme: ws.theme.clone(),
            ui: ws.ui,
        }
    });
    let typed = view.downgrade();
    App::subscribe(cx, &code, move |_, event: &InputEvent, cx| {
        if let InputEvent::Change = event {
            typed.update(cx, |_, cx| cx.notify()).ok();
        }
    })
    .detach();
    // The proxy the environment names for the code's host fills its field, once.
    cx.observe_in(&view, window, |_, view, window, cx| {
        let (prefilled, decoded) = {
            let flow = view.read(cx);
            (flow.prefilled, flow.decoded(cx))
        };
        if let (false, Some(code)) = (prefilled, decoded) {
            let found = proxy_from_env(&bare_host(&code), |name| std::env::var(name).ok());
            view.update(cx, |flow, cx| {
                flow.prefilled = true;
                if let Some(found) = found {
                    flow.proxy
                        .update(cx, |input, cx| input.set_value(found, window, cx));
                }
            });
        }
    })
    .detach();
    let (shown, closing) = (view.clone(), view);
    window.open_alert_dialog(cx, move |alert, _, cx| {
        let flow = shown.clone();
        let (ready, connecting) = {
            let flow = flow.read(cx);
            (flow.decoded(cx).is_some(), flow.connecting)
        };
        let connect = flow.clone();
        let footer = div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(
                Button::new("connect-cancel")
                    .label("Cancel")
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                Button::new("connect-go")
                    .label("Connect")
                    .primary()
                    .disabled(!ready || connecting)
                    .on_click(move |_, window, cx| {
                        connect.update(cx, |flow, cx| flow.connect(window, cx))
                    }),
            );
        let (workspace, flow_close) = (flow.read(cx).workspace.clone(), flow.clone());
        alert
            .confirm()
            .title(title.clone())
            .w(px(520.0))
            .footer(footer)
            .child(flow.clone())
            .on_close(move |_, window, cx| {
                // Closed before it was live: nothing of it is left.
                workspace.update(cx, |ws, cx| ws.cancel_connect(window, cx));
                flow_close.update(cx, |flow, _| flow.connecting = false);
            })
    });
    let field = closing.read(cx).code.clone();
    field.update(cx, |input, cx| input.focus(window, cx));
}
