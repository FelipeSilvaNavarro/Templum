//! The branch switcher: the top bar's branch pill opens it. A search over the local branches,
//! then the remote ones; Enter switches (through the question about changes); + New branch…
//! at the foot. See docs/specs/git.md, section 1, Top bar.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::prelude::*;
use gpui_kit::{App, Entity, Window, px};
use templum_canvas::BlockId;
use templum_git::Branch;

use crate::workspace::Workspace;

pub(crate) fn open(
    ws: Entity<Workspace>,
    block: BlockId,
    branches: Vec<Branch>,
    window: &mut Window,
    cx: &mut App,
) {
    let (local, remote): (Vec<Branch>, Vec<Branch>) = branches.into_iter().partition(|b| !b.remote);
    let state = cx.new(|cx| CommandState::new(window, cx));
    let palette = state.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let (ws_pick, ws_new) = (ws.clone(), ws.clone());
        let (local_names, remote_names): (Vec<String>, Vec<String>) = (
            local.iter().map(|b| b.name.clone()).collect(),
            remote.iter().map(|b| b.name.clone()).collect(),
        );
        // Both groups always: their sections stay 0 and 1, filtered or not.
        dialog.title("Switch branch").w(px(420.0)).child(
            Command::new(&palette)
                .group(
                    CommandGroup::new().label("Local").items(
                        local
                            .iter()
                            .map(|b| CommandItem::new().label(b.name.clone()).checked(b.current)),
                    ),
                )
                .group(
                    CommandGroup::new().label("Remote").items(
                        remote
                            .iter()
                            .map(|b| CommandItem::new().label(b.name.clone())),
                    ),
                )
                .placeholder("Search branches")
                .footer(move |_, _, _| {
                    let ws = ws_new.clone();
                    Button::new("git-switcher-new")
                        .label("+ New branch…")
                        .ghost()
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let ws = ws.clone();
                            // The next dialog opens once this one has closed.
                            window.defer(cx, move |window, cx| {
                                ws.update(cx, |ws, cx| ws.branch_from(block, None, window, cx))
                            });
                        })
                })
                .on_confirm(move |at, window, cx| {
                    let (name, remote) = match at.section {
                        0 => (local_names.get(at.row).cloned(), false),
                        _ => (remote_names.get(at.row).cloned(), true),
                    };
                    let Some(name) = name else {
                        return;
                    };
                    window.close_dialog(cx);
                    ws_pick.update(cx, |ws, cx| {
                        ws.switch_branch(block, name, remote, window, cx)
                    });
                }),
        )
    });
    state.update(cx, |state, cx| state.focus(window, cx));
}
