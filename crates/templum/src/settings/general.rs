//! Settings ▸ General: starting Templum when the person logs in to the computer. The row reads
//! the system's own entry, so it follows a change made with the system's tools; the welcome has
//! the same row, which applies its choice when it closes (docs/specs/settings.md, Start at login).

use gpui_kit::component::switch::Switch;
use gpui_kit::{Context, Entity, Window};

use super::row::Row;
use crate::workspace::Workspace;

/// What a click on the toggle does.
pub(super) type Choose = fn(&mut Workspace, bool, &mut Window, &mut Context<Workspace>);

pub(super) fn rows(ws: &Workspace, entity: &Entity<Workspace>) -> Vec<Row> {
    vec![autostart_row(
        ws,
        entity,
        ws.autostart.on,
        |ws, on, window, cx| ws.set_autostart(on, window, cx),
    )]
}

/// "Start Templum when you log in", showing `on`; `choose` runs on a click. Without an entry (no
/// home folder) it only says so.
pub(super) fn autostart_row(
    ws: &Workspace,
    entity: &Entity<Workspace>,
    on: bool,
    choose: Choose,
) -> Row {
    let row = Row::new("general.autostart", "Start Templum when you log in");
    if ws.autostart.entry.is_none() {
        return row.description(
            "No home folder was found, so Templum can't add itself to the startup apps",
        );
    }
    let e = entity.clone();
    row.description(ws.autostart.error.clone().unwrap_or_else(|| {
        "Opens Templum when you sign in to this computer. It is your system's own startup \
         entry: switch it off in its startup apps and this follows"
            .into()
    }))
    .control(
        Switch::new("autostart-switch")
            .checked(on)
            .on_click(move |on: &bool, window, cx| {
                let on = *on;
                e.update(cx, |ws, cx| choose(ws, on, window, cx))
            }),
    )
}
