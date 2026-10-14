//! The workspaces screen: opening and closing it, its search, its cards and its keys.

use super::*;
use crate::space_dialog::Look;

#[cfg(target_os = "macos")]
const WORKSPACES: &str = "cmd-shift-o";
#[cfg(not(target_os = "macos"))]
const WORKSPACES: &str = "ctrl-shift-o";

fn up(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> bool {
    ws.read_with(cx, |ws, _| ws.screen.as_ref().is_some_and(|s| !s.closing))
}

/// Three workspaces, "Main", "Api" and "Web", the last one shown.
fn three(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            for (id, name) in [("w2", "Api"), ("w3", "Web")] {
                ws.new_space(window, cx);
                ws.edit_space(id, name, Look::default(), None, window, cx)
                    .unwrap();
            }
        })
    })
    .unwrap();
}

/// Does `act` in the window, then draws a frame.
fn act(
    handle: AnyWindowHandle,
    cx: &mut TestAppContext,
    act: impl FnOnce(&mut Window, &mut gpui_kit::App),
) {
    cx.update_window(handle, |_, window, cx| {
        act(window, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// Opens the screen with its command, and lets the sheet come up (on the real clock).
fn open_screen(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    act(handle, cx, |w, cx| w.press(WORKSPACES, cx));
    settle_slide(handle, cx);
}

#[gpui_kit::test]
fn the_button_the_name_and_the_command_open_the_screen_and_esc_closes_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    act(handle, cx, |w, cx| w.click("workspaces-button", cx));
    assert!(up(&ws, cx), "the sidebar's button");
    act(handle, cx, |w, cx| w.press("escape", cx));
    assert!(!up(&ws, cx), "Esc");
    act(handle, cx, |w, cx| w.click("workspace-name", cx));
    assert!(up(&ws, cx), "the name in the title bar");
    settle_slide(handle, cx);
    act(handle, cx, |w, cx| w.click("screen-close", cx));
    assert!(!up(&ws, cx), "its ✕");
    act(handle, cx, |w, cx| w.press(WORKSPACES, cx));
    assert!(up(&ws, cx), "the command");
    act(handle, cx, |w, cx| w.press(WORKSPACES, cx));
    assert!(!up(&ws, cx), "the command again, from the search field");
}

#[gpui_kit::test]
fn the_search_filters_the_cards(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    three(handle, &ws, cx);
    open_screen(handle, cx);
    act(handle, cx, |w, cx| {
        assert!(w.try_find("space-card-default").is_some());
        w.input("AP", cx);
    });
    act(handle, cx, |w, _| {
        assert!(w.try_find("space-card-w2").is_some(), "Api");
        assert!(w.try_find("space-card-default").is_none());
        assert!(w.try_find("space-card-w3").is_none());
    });
}

#[gpui_kit::test]
fn clicking_a_card_switches_and_closes_the_screen(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    three(handle, &ws, cx);
    open_screen(handle, cx);
    act(handle, cx, |w, cx| w.click("space-card-default", cx));
    assert!(!up(&ws, cx));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.name, "Main"));
}

#[gpui_kit::test]
fn down_and_enter_in_the_search_open_the_highlighted_card(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    three(handle, &ws, cx);
    open_screen(handle, cx);
    act(handle, cx, |w, cx| w.press("enter", cx));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.name, "Main", "the first card")
    });
    assert!(!up(&ws, cx));
    open_screen(handle, cx);
    // Four cards to a row: Down goes to the last of the three.
    act(handle, cx, |w, cx| w.press("down", cx));
    act(handle, cx, |w, cx| w.press("enter", cx));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.name, "Web"));
    // Typing narrows the cards; Enter opens the first left.
    open_screen(handle, cx);
    act(handle, cx, |w, cx| w.input("api", cx));
    act(handle, cx, |w, cx| w.press("enter", cx));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.name, "Api"));
}
