//! Starting Templum at login: the toggle's state, read from the system when the settings panel or
//! the welcome opens, and the changes the toggle and the welcome make (docs/specs/settings.md,
//! Start at login).

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, Window};

use super::{Workspace, show_notification};

impl Workspace {
    /// Reads whether the system starts Templum at login. The registry is read by a program on
    /// Windows, so it isn't read on the UI thread.
    pub(crate) fn load_autostart(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.autostart.entry.clone() else {
            return;
        };
        // What failed before is old news.
        self.autostart.error = None;
        cx.spawn(async move |ws, cx| {
            let on = cx
                .background_executor()
                .spawn(async move { entry.is_on() })
                .await;
            ws.update(cx, |ws, cx| {
                ws.autostart.on = on;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Turns the system's entry on or off. The switch shows the new state at once and the system
    /// is written off the UI thread (Windows runs programs for it); a click meanwhile waits. A
    /// failure is on the toggle's row and in a notification, and the toggle shows what the system
    /// really has.
    pub(crate) fn set_autostart(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.autostart.entry.clone() else {
            return;
        };
        if self.autostart.changing {
            return;
        }
        self.autostart.changing = true;
        self.autostart.on = on;
        self.autostart.error = None;
        cx.notify();
        cx.spawn_in(window, async move |ws, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { entry.set(on).map_err(|err| (err, entry.is_on())) })
                .await;
            ws.update_in(cx, |ws, window, cx| {
                ws.autostart.changing = false;
                if let Err((err, really)) = done {
                    eprintln!("templum: couldn't change starting at login: {err}");
                    ws.autostart.on = really;
                    let message = format!("Couldn't turn starting at login {}: {err}", state(on));
                    show_notification(window, cx, Notification::warning(message.clone()));
                    ws.autostart.error = Some(message);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// At startup, an entry that is on but whose executable is gone gets this one's path
    /// (Templum was moved or installed elsewhere). Off the UI thread, and silent: the person
    /// asked for nothing. A second Templum open (it has no data folder: it keeps nothing) leaves
    /// the entry to the first.
    pub(super) fn refresh_autostart(&self, cx: &mut Context<Self>) {
        let Some(entry) = self
            .autostart
            .entry
            .clone()
            .filter(|_| self.data_dir.is_some())
        else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = entry.refresh() {
                    eprintln!("templum: couldn't update starting at login: {err}");
                }
            })
            .detach();
    }
}

fn state(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}
