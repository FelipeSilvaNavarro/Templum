use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Entity, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, PlatformInput, ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase, Window, WindowBounds, WindowOptions, point, px, size,
};

use super::{Workspace, bind_keys};

#[test]
fn a_failure_that_wanted_a_key_or_a_password_says_so() {
    let err = |stderr: &str| templum_git::GitError {
        command: "git push".to_string(),
        code: Some(128),
        stderr: stderr.to_string(),
        missing: false,
    };
    assert_eq!(
        super::git_error_message(&err(
            "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n"
        )),
        "git@github.com: Permission denied (publickey). Git needs your key or password: run it in a terminal."
    );
    assert_eq!(
        super::git_error_message(&err("fatal: not a git repository\n")),
        "fatal: not a git repository"
    );
}

#[test]
fn a_workspace_shows_an_agent_from_its_last_snapshot() {
    use crate::session::{Item, Saved, SavedCamera, SavedRect, TerminalItem, VERSION};
    let with = |agent: Option<&str>| super::Space {
        saved: Some(Saved {
            version: VERSION,
            camera: SavedCamera {
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
            },
            items: vec![Item::Terminal(TerminalItem {
                rect: SavedRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
                folder: None,
                agent: agent.map(String::from),
            })],
            focused: None,
            recent: Vec::new(),
            ink: Vec::new(),
        }),
        ..super::Space::default()
    };
    let known = crate::agents::effective(&Default::default());
    assert!(with(Some("claude")).has_agent(&known));
    assert!(
        !with(Some("vim")).has_agent(&known),
        "not an agent Templum knows"
    );
    assert!(!with(None).has_agent(&known));
    assert!(
        !super::Space::default().has_agent(&known),
        "no snapshot yet"
    );
}

#[test]
fn ctrl_digits_pick_a_workspace() {
    let digit =
        |k: &str, macos| super::workspace_digit(&gpui_kit::Keystroke::parse(k).unwrap(), macos);
    assert_eq!(digit("ctrl-1", false), Some(0));
    assert_eq!(digit("ctrl-9", false), Some(8));
    assert_eq!(digit("cmd-2", true), Some(1));
    assert_eq!(digit("ctrl-2", true), None, "Cmd on macOS");
    assert_eq!(digit("ctrl-0", false), None, "zoom reset");
    assert_eq!(digit("ctrl-shift-1", false), None, "fit all");
    assert_eq!(digit("ctrl-alt-1", false), None);
    assert_eq!(digit("1", false), None);
}

#[test]
fn the_hint_shows_the_shortcuts_in_effect() {
    use crate::keys::{Keymap, Keys, KeysConfig};
    let (keymap, _) = Keymap::new(&KeysConfig::default(), false).unwrap();
    let rows = super::hint_keys(&keymap);
    assert_eq!(rows[0].as_ref().map(|k| k.key.as_str()), Some("t"));
    assert_eq!(rows[1].as_ref().map(|k| k.key.as_str()), Some(","));
    let keys = KeysConfig {
        new_terminal: Some(Keys::Many(vec![])),
        ..KeysConfig::default()
    };
    let (keymap, _) = Keymap::new(&keys, false).unwrap();
    assert!(
        super::hint_keys(&keymap)[0].is_none(),
        "no shortcut, no keys"
    );
}

#[cfg(target_os = "macos")]
const NEW: &str = "cmd-t";
#[cfg(not(target_os = "macos"))]
const NEW: &str = "ctrl-shift-t";
#[cfg(target_os = "macos")]
const CLOSE: &str = "cmd-w";
#[cfg(not(target_os = "macos"))]
const CLOSE: &str = "ctrl-shift-w";
#[cfg(target_os = "macos")]
const UNDO: &str = "cmd-z";
#[cfg(not(target_os = "macos"))]
const UNDO: &str = "ctrl-z";
#[cfg(target_os = "macos")]
const REDO: &str = "cmd-shift-z";
#[cfg(not(target_os = "macos"))]
const REDO: &str = "ctrl-shift-z";

/// A long-lived command that prints nothing, so tests don't depend on shell output.
fn quiet() -> (String, Vec<String>) {
    #[cfg(unix)]
    return ("cat".into(), vec![]);
    #[cfg(windows)]
    return ("cmd".into(), vec!["/K".into()]);
}

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>) {
    open_with(cx, None)
}

/// Opens a workspace whose config folder is `config_dir` (None: no config, no user themes).
fn open_with(
    cx: &mut TestAppContext,
    config_dir: Option<std::path::PathBuf>,
) -> (AnyWindowHandle, Entity<Workspace>) {
    open_in(cx, config_dir, None)
}

/// Opens a workspace with a config folder and a data folder (None: nothing read or saved).
fn open_in(
    cx: &mut TestAppContext,
    config_dir: Option<std::path::PathBuf>,
    data_dir: Option<std::path::PathBuf>,
) -> (AnyWindowHandle, Entity<Workspace>) {
    open_as(cx, Some(quiet()), config_dir, data_dir)
}

/// `open_in` with the program new terminals run.
fn open_as(
    cx: &mut TestAppContext,
    command: Option<(String, Vec<String>)>,
    config_dir: Option<std::path::PathBuf>,
    data_dir: Option<std::path::PathBuf>,
) -> (AnyWindowHandle, Entity<Workspace>) {
    // PTY threads wake the UI from outside the test scheduler.
    cx.executor().allow_parking();
    let opened = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(1280.), px(800.)));
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Workspace::with(command, config_dir, data_dir, window, cx)),
        )
        .expect("open window")
    });
    // The startup block at rest: tests that measure elements must not depend on how far its
    // pop-in got (new_terminals_pop_in_then_settle checks the animation itself).
    opened.1.update(cx, |ws, _| ws.space.appearing.clear());
    opened
}

fn click(window: &mut Window, cx: &mut gpui_kit::App, at: (f32, f32), count: usize) {
    let position = point(px(at.0), px(at.1));
    let modifiers = Modifiers::default();
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count: count,
            first_mouse: false,
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count: count,
        }),
        cx,
    );
}

fn right_click(window: &mut Window, cx: &mut gpui_kit::App, at: (f32, f32)) {
    let position = point(px(at.0), px(at.1));
    let modifiers = Modifiers::default();
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Right,
            position,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Right,
            position,
            modifiers,
            click_count: 1,
        }),
        cx,
    );
}

/// Holds or lets go of the hand key (Alt).
fn hold_alt(window: &mut Window, cx: &mut gpui_kit::App, held: bool) {
    window.dispatch_event(
        PlatformInput::ModifiersChanged(gpui_kit::ModifiersChangedEvent {
            modifiers: Modifiers {
                alt: held,
                ..Default::default()
            },
            capslock: Default::default(),
        }),
        cx,
    );
    window.render_frame(cx);
}

fn press_with(
    window: &mut Window,
    cx: &mut gpui_kit::App,
    button: MouseButton,
    modifiers: Modifiers,
    from: (f32, f32),
    to: (f32, f32),
) {
    let at = |p: (f32, f32)| point(px(p.0), px(p.1));
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button,
            position: at(from),
            modifiers,
            click_count: 1,
            first_mouse: false,
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
            position: at(to),
            pressed_button: Some(button),
            modifiers,
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            button,
            position: at(to),
            modifiers,
            click_count: 1,
        }),
        cx,
    );
}

fn wheel(
    window: &mut Window,
    cx: &mut gpui_kit::App,
    at: (f32, f32),
    delta: ScrollDelta,
    control: bool,
) {
    window.dispatch_event(
        PlatformInput::ScrollWheel(ScrollWheelEvent {
            position: point(px(at.0), px(at.1)),
            delta,
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
            touch_phase: TouchPhase::Moved,
        }),
        cx,
    );
}

#[gpui_kit::test]
fn starts_with_one_focused_terminal(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 1);
        assert_eq!(ws.space.focused, ws.space.canvas.top());
        assert_eq!(ws.space.terminals().count(), 1);
    });
}

#[gpui_kit::test]
fn blocks_of_any_kind_share_one_map(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.new_kanban(window, cx));
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let kanban = ws.space.canvas.top().unwrap();
        assert_eq!(ws.space.blocks.len(), 2);
        assert!(ws.space.terminal(terminal).is_some());
        assert!(ws.space.kanban(terminal).is_none());
        assert!(ws.space.kanban(kanban).is_some());
        assert!(ws.space.terminal(kanban).is_none());
        assert_eq!(ws.space.terminals().count(), 1);
        assert_eq!(ws.space.focused, Some(kanban));
        assert!(ws.space.focused_terminal().is_none());
    });
}

#[gpui_kit::test]
fn startup_terminal_stays_centered_when_the_window_is_resized(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // Tiling compositors resize the window right after it opens.
    cx.simulate_window_resize(handle, size(px(600.), px(400.)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Measured, not computed with viewport(): the canvas area starts at the workspace's
        // origin (below the title bar) and runs to the window's edge.
        let full = window.viewport_size();
        let ws = ws.read(cx);
        let vp = templum_canvas::Size::new(
            f32::from(full.width - ws.origin.x),
            f32::from(full.height - ws.origin.y),
        );
        let b = ws.space.canvas.blocks()[0].rect;
        let center = ws
            .space
            .canvas
            .camera
            .to_screen(templum_canvas::Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0));
        assert!(
            (center.x - vp.w / 2.0).abs() < 1.0 && (center.y - vp.h / 2.0).abs() < 1.0,
            "startup block center on screen: {center:?}, canvas {vp:?}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn shortcuts_open_and_close_terminals(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 2);
        assert_eq!(
            ws.space.focused,
            ws.space.canvas.top(),
            "the new terminal is focused"
        );
        // The startup block already sits at the viewport center: the new one cascades.
        let [first, second] = ws.space.canvas.blocks() else {
            unreachable!()
        };
        assert_eq!(
            (second.rect.x - first.rect.x, second.rect.y - first.rect.y),
            (templum_canvas::CASCADE, templum_canvas::CASCADE)
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(CLOSE, cx);
        window.press(CLOSE, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.canvas.blocks().is_empty());
        assert!(ws.space.terminals().next().is_none());
        assert_eq!(ws.space.focused, None);
    });
}

/// Draws frames (each hears the terminals) until `done`.
#[cfg(unix)]
fn until(
    what: &str,
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    done: impl Fn(&Workspace) -> bool,
) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        if ws.read_with(cx, |ws, _| done(ws)) {
            return;
        }
        assert!(Instant::now() < deadline, "waited for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The terminal rings its bell: `cat` prints back the BEL it reads.
#[cfg(unix)]
fn ring(ws: &Entity<Workspace>, id: templum_canvas::BlockId, cx: &mut TestAppContext) {
    ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(id)
            .unwrap()
            .term
            .write(b"\x07\r".to_vec())
    });
}

#[cfg(unix)]
fn set_active(handle: AnyWindowHandle, active: bool, cx: &mut TestAppContext) {
    if active {
        cx.update_window(handle, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
    } else {
        gpui_kit::VisualTestContext::from_window(handle, cx).deactivate_window();
    }
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_bell_in_another_block_alerts_until_it_is_focused(cx: &mut TestAppContext) {
    use crate::alert::Alert;
    let (handle, ws) = open(cx);
    set_active(handle, true, cx);
    let ringing = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ring(&ws, ringing, cx);
    until("the alert", handle, &ws, cx, |ws| {
        ws.space.terminal(ringing).unwrap().alert.is_some()
    });
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.terminal(ringing).unwrap().alert,
            Some(Alert::Attention(None))
        );
    });
    assert!(
        cx.shown_system_notifications().is_empty(),
        "the window is in front: only the in-app one"
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(ringing, window, cx))
    })
    .unwrap();
    until("the alert to go", handle, &ws, cx, |ws| {
        ws.space.terminal(ringing).unwrap().alert.is_none()
    });
    // The focused terminal, looked at: no news.
    ring(&ws, ringing, cx);
    until("the bell", handle, &ws, cx, |ws| {
        ws.space.terminal(ringing).unwrap().heard.1 == 2
    });
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminal(ringing).unwrap().alert, None)
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn in_the_background_a_bell_is_a_system_notification_unless_turned_off(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // As main does: without it, no system notification (on Windows too).
    cx.update(|cx| cx.set_app_identity("test.Templum", "Templum"));
    let id = first_id(&ws, cx);
    // Rings, then draws until it has been heard `bells` times, and the frame after (when
    // notifications are shown).
    let bell = |bells: u64, cx: &mut TestAppContext| {
        ring(&ws, id, cx);
        until("the bell", handle, &ws, cx, |ws| {
            ws.space.terminal(id).unwrap().heard.1 == bells
        });
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    };
    set_active(handle, false, cx);
    ws.update(cx, |ws, _| ws.config.terminal.notifications = false);
    bell(1, cx);
    assert!(ws.read_with(cx, |ws, _| ws.space.terminal(id).unwrap().alert.is_some()));
    assert_eq!(cx.shown_system_notifications().len(), 0, "turned off");
    // Looked at, then in the background again.
    set_active(handle, true, cx);
    until("the alert to go", handle, &ws, cx, |ws| {
        ws.space.terminal(id).unwrap().alert.is_none()
    });
    set_active(handle, false, cx);
    ws.update(cx, |ws, _| ws.config.terminal.notifications = true);
    bell(2, cx);
    let shown = cx.shown_system_notifications();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].title.as_ref(), "cat");
    assert_eq!(shown[0].body.as_ref(), "Needs your attention");
    bell(3, cx);
    assert_eq!(
        cx.shown_system_notifications().len(),
        1,
        "not again until it is seen"
    );
}

fn first_id(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> templum_canvas::BlockId {
    ws.read_with(cx, |ws, _| ws.space.canvas.top().unwrap())
}

#[gpui_kit::test]
fn double_click_on_empty_canvas_opens_a_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // The canvas starts below the title bar.
        let o = ws.read(cx).origin;
        let at = (f32::from(o.x) + 40.0, f32::from(o.y) + 40.0);
        click(window, cx, at, 1);
        click(window, cx, at, 2);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 2);
        let new = ws.space.canvas.get(ws.space.canvas.top().unwrap()).unwrap();
        assert_eq!((new.rect.x, new.rect.y), (40.0, 40.0));
    });
}

#[gpui_kit::test]
fn close_button_removes_the_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.hover(("block", id.0 as usize), cx);
        window.render_frame(cx);
        window.click(("close", id.0 as usize), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.space.canvas.blocks().is_empty()));
}

#[gpui_kit::test]
fn dragging_the_title_moves_the_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let before = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(("title", id.0 as usize)).bounds().center();
        window.drag(from, point(from.x + px(100.), from.y + px(50.)), cx);
    })
    .unwrap();
    let after = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    assert_eq!((after.x - before.x, after.y - before.y), (100.0, 50.0));
    assert_eq!((after.w, after.h), (before.w, before.h));
}

#[gpui_kit::test]
fn a_dragged_block_snaps_to_a_neighbour_and_shows_a_guide_until_release(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    // A small second block over the first one's top, its horizontal lines (30, 80 and 130
    // below the first's top) clear of the first's (0, half its height, its height).
    let second = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let r = ws.space.canvas.get(first).unwrap().rect;
                ws.open(Rect::new(r.x + 300.0, r.y + 30.0, 200.0, 100.0), window, cx);
                ws.space.appearing.clear();
                ws.space.canvas.top().unwrap()
            })
        })
        .unwrap();
    let (first_rect, second_rect) = ws.read_with(cx, |ws, _| {
        (
            ws.space.canvas.get(first).unwrap().rect,
            ws.space.canvas.get(second).unwrap().rect,
        )
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(("title", second.0 as usize)).bounds().center();
        // Left, so the second's left edge lands 5 px right of the first's: it snaps there.
        let to = point(from.x + px(first_rect.x - second_rect.x + 5.0), from.y);
        let modifiers = Modifiers::default();
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                modifiers,
            }),
            cx,
        );
        let ws_now = ws.read(cx);
        assert_eq!(
            ws_now.space.canvas.get(second).unwrap().rect.x,
            first_rect.x
        );
        assert_eq!(ws_now.space.guides.len(), 1, "a vertical guide");
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
        assert!(ws.read(cx).space.guides.is_empty(), "gone on release");
    })
    .unwrap();
}

#[gpui_kit::test]
fn dragging_the_handle_resizes_block_and_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let (before, cell) = ws.read_with(cx, |ws, _| {
        (ws.space.canvas.get(id).unwrap().rect, ws.cell.size)
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.hover(("block", id.0 as usize), cx);
        window.render_frame(cx);
        let from = window.find(("handle", id.0 as usize)).bounds().center();
        let to = point(from.x + px(cell.w * 10.0), from.y + px(cell.h * 3.0));
        window.drag(from, to, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let rect = ws.space.canvas.get(id).unwrap().rect;
        assert!(rect.w > before.w && rect.h > before.h);
        let expected = templum_canvas::chrome::grid_size(
            templum_canvas::chrome::content_rect(rect).size(),
            cell,
        );
        assert_eq!(ws.space.terminal(id).unwrap().term.grid_size(), expected);
        assert_eq!(
            expected,
            (super::DEFAULT_COLS + 10, super::DEFAULT_ROWS + 3)
        );
    });
}

#[gpui_kit::test]
fn wheel_on_empty_canvas_pans_and_ctrl_wheel_zooms(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        wheel(
            window,
            cx,
            (f32::from(o.x) + 30.0, f32::from(o.y) + 30.0),
            ScrollDelta::Pixels(point(px(0.), px(100.))),
            false,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.origin.y, -100.0)
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        wheel(
            window,
            cx,
            (f32::from(o.x) + 30.0, f32::from(o.y) + 30.0),
            ScrollDelta::Lines(point(0.0, 1.0)),
            true,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.1));
}

#[cfg(target_os = "macos")]
const COPY: &str = "cmd-c";
#[cfg(not(target_os = "macos"))]
const COPY: &str = "ctrl-shift-c";

#[gpui_kit::test]
fn copy_puts_the_selection_on_the_clipboard(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::{SelectionKind, Side, Snapshot};
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    // `cat` echoes its input; on Windows the quiet command is `cmd /K`.
    #[cfg(unix)]
    let input = "hello\r";
    #[cfg(windows)]
    let input = "echo hello\r";
    ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(id)
            .unwrap()
            .term
            .write(input.as_bytes().to_vec())
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("hello") {
            break;
        }
        assert!(Instant::now() < deadline, "no echo, screen: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    ws.read_with(cx, |ws, _| {
        let pane = &ws.space.terminal(id).unwrap();
        let (cols, rows) = pane.term.grid_size();
        let term = pane.term.shared();
        term.select_start(0, 0, Side::Left, SelectionKind::Simple);
        term.select_update(rows as usize - 1, cols as usize - 1, Side::Right);
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(COPY, cx);
    })
    .unwrap();
    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .unwrap_or_default();
    assert!(text.contains("hello"), "clipboard: {text:?}");
}

#[gpui_kit::test]
fn mouse_down_in_a_body_starts_a_selection(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let body = window.find(("body", id.0 as usize)).bounds();
        let from = point(body.origin.x + px(20.), body.origin.y + px(20.));
        window.drag(from, point(from.x + px(80.), from.y), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.focused, Some(id));
        assert!(ws.drag.is_none(), "the selection drag ended on mouse up");
    });
}

#[cfg(target_os = "macos")]
const ZOOM_IN: &str = "cmd-=";
#[cfg(not(target_os = "macos"))]
const ZOOM_IN: &str = "ctrl-=";
#[cfg(target_os = "macos")]
const ZOOM_OUT: &str = "cmd--";
#[cfg(not(target_os = "macos"))]
const ZOOM_OUT: &str = "ctrl--";
#[cfg(target_os = "macos")]
const ZOOM_RESET: &str = "cmd-0";
#[cfg(not(target_os = "macos"))]
const ZOOM_RESET: &str = "ctrl-0";
#[cfg(target_os = "macos")]
const FIT: &str = "cmd-shift-1";
#[cfg(not(target_os = "macos"))]
const FIT: &str = "ctrl-shift-1";

#[gpui_kit::test]
fn zoom_keys_step_and_reset(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_IN, cx);
        window.press(ZOOM_IN, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.25));
    cx.update_window(handle, |_, window, cx| {
        window.press(ZOOM_OUT, cx);
        window.press(ZOOM_RESET, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.0));
}

#[gpui_kit::test]
fn fit_all_brings_every_block_into_view(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(
                templum_canvas::Rect::new(5000.0, 3000.0, 600.0, 400.0),
                window,
                cx,
            )
        });
        window.render_frame(cx);
        window.press(FIT, cx);
        let vp = ws.read(cx).viewport(window);
        assert_eq!(ws.read(cx).space.canvas.visible(vp).count(), 2);
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_theme_change_crossfades_the_chrome(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    let light = ws.read_with(cx, |ws, _| {
        assert_eq!(ws.ui, ws.theme.ui, "the first theme shows at once");
        ws.theme.ui
    });
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    // The fade starts on the first frame that draws the new theme.
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.executor().advance_clock(crate::motion::THEME / 2);
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_ne!(ws.ui, ws.theme.ui, "fading");
        assert_ne!(ws.ui, light, "fading");
    });
    cx.executor()
        .advance_clock(crate::motion::THEME + std::time::Duration::from_millis(10));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.ui, ws.theme.ui, "settled"));
}

#[gpui_kit::test]
fn zoom_keys_glide_to_the_step(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_IN, cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.canvas.camera.zoom(),
            1.1,
            "the target is set at once"
        );
        assert!(ws.space.flight.is_some(), "the view glides");
        assert!(
            ws.space.view.zoom() < 1.1,
            "drawn on the way: {}",
            ws.space.view.zoom()
        );
    });
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.flight.is_none(), "landed");
        assert_eq!(ws.space.view, ws.space.canvas.camera);
    });
}

#[gpui_kit::test]
fn a_wheel_pan_during_a_glide_moves_the_glide_too(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_IN, cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| {
        let (before, _) = ws.space.flight.unwrap();
        ws.pan_by(Point::new(30.0, 0.0));
        let (after, _) = ws.space.flight.expect("still gliding");
        assert_eq!(
            after.to, ws.space.canvas.camera,
            "the glide still lands on the camera"
        );
        let z = before.from.zoom();
        assert!((before.from.origin.x - after.from.origin.x - 30.0 / z).abs() < 1e-3);
    });
}

#[gpui_kit::test]
fn dragging_the_canvas_ends_a_glide_on_the_camera(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_OUT, cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| {
        ws.space.canvas.camera.pan_by(Point::new(5.0, 5.0))
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.flight.is_none());
        assert_eq!(ws.space.view, ws.space.canvas.camera);
    });
}

#[gpui_kit::test]
fn fit_all_glides_then_shows_every_block(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(
                templum_canvas::Rect::new(5000.0, 3000.0, 600.0, 400.0),
                window,
                cx,
            )
        });
        window.render_frame(cx);
        window.press(FIT, cx);
        window.render_frame(cx);
        assert!(ws.read(cx).space.flight.is_some());
    })
    .unwrap();
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let vp = ws.read(cx).viewport(window);
        let ws = ws.read(cx);
        assert_eq!(ws.space.canvas.visible_from(&ws.space.view, vp).count(), 2);
    })
    .unwrap();
}

/// Presses `key`, then types "x" + Enter into `cat`, and returns the screen once "x" shows.
#[cfg(unix)]
fn echo_after(
    key: &str,
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
) -> String {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let id = first_id(ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(id).unwrap().term.write(b"x\r".to_vec())
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains('x') {
            return shown;
        }
        assert!(Instant::now() < deadline, "no echo: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

// gpui-kit's Root binds tab to focus navigation; the shell must get it (completion).
#[cfg(unix)]
#[gpui_kit::test]
fn tab_reaches_the_shell(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let shown = echo_after("tab", handle, &ws, cx);
    let first = shown.lines().next().unwrap_or("");
    assert!(
        first.starts_with(' '),
        "Tab never reached the shell; screen: {shown:?}"
    );
}

#[gpui_kit::test]
fn pinch_over_a_block_zooms(cx: &mut TestAppContext) {
    use gpui_kit::PinchEvent;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let at = window.find(("body", id.0 as usize)).bounds().center();
        window.dispatch_event(
            PlatformInput::Pinch(PinchEvent {
                position: at,
                delta: 0.3,
                modifiers: Modifiers::default(),
                phase: TouchPhase::Moved,
            }),
            cx,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.1));
}

#[gpui_kit::test]
fn follows_the_system_appearance(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // The test window is light.
    ws.read_with(cx, |ws, _| assert_eq!(ws.theme.name, "Catppuccin Latte"));
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "Catppuccin Mocha");
        assert_eq!(ws.space.terminals().count(), 2);
        for pane in ws.space.terminals().map(|(_, p)| p) {
            assert_eq!(
                pane.term.palette(),
                ws.theme.palette,
                "old and new terminals"
            );
        }
    });
    cx.update(|cx| {
        let kit = gpui_kit::component::Theme::global(cx);
        assert!(kit.mode.is_dark());
    });
    ws.read_with(cx, |ws, cx| {
        assert_eq!(
            gpui_kit::component::Theme::global(cx).background,
            ws.theme.ui.canvas,
            "gpui-kit's root paints the opaque canvas"
        );
    });
}

#[gpui_kit::test]
fn config_names_pick_the_theme(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "[theme]\nlight = \"One Light\"\n",
    )
    .unwrap();
    let (_, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.read_with(cx, |ws, _| assert_eq!(ws.theme.name, "One Light"));
}

#[gpui_kit::test]
fn title_bar_buttons_reset_zoom_and_open_terminals(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(ZOOM_IN, cx);
        window.render_frame(cx);
        window.click("zoom-level", cx);
        window.render_frame(cx);
        window.click("tool-terminal", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.zoom(), 1.0);
        assert_eq!(ws.space.canvas.blocks().len(), 2);
    });
}

#[gpui_kit::test]
fn focus_does_not_shift_the_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let focused = window.find(("body", id.0 as usize)).bounds();
        ws.update(cx, |ws, _| ws.space.focused = None);
        window.render_frame(cx);
        let unfocused = window.find(("body", id.0 as usize)).bounds();
        assert_eq!(focused, unfocused);
    })
    .unwrap();
}

#[gpui_kit::test]
fn close_button_appears_on_hover(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(("close", id.0 as usize)).is_none(),
            "hidden until hovered"
        );
        window.hover(("block", id.0 as usize), cx);
        window.render_frame(cx);
        assert!(window.try_find(("close", id.0 as usize)).is_some());
        assert!(window.try_find(("handle", id.0 as usize)).is_some());
    })
    .unwrap();
}

fn builtin_index(name: &str) -> usize {
    crate::theme::BUILTIN
        .iter()
        .position(|b| b.name == name)
        .unwrap()
}

#[gpui_kit::test]
fn theme_picker_previews_cancels_and_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("One Dark")), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "One Dark", "hover previews")
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("escape", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "Catppuccin Latte", "Esc puts it back");
        assert!(ws.picker.is_none());
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "Catppuccin Frappé");
        assert!(ws.picker.is_none());
    });
    let saved = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(saved.contains("light = \"Catppuccin Frappé\""), "{saved}");
}

#[gpui_kit::test]
fn clicking_outside_the_picker_cancels(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("Apple Light")), cx);
        window.render_frame(cx);
        window.click("picker-backdrop", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "Catppuccin Latte");
        assert!(ws.picker.is_none());
        assert_eq!(
            ws.space.canvas.blocks().len(),
            1,
            "the click didn't reach the canvas"
        );
    });
}

#[gpui_kit::test]
fn a_broken_config_is_not_overwritten(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[theme\n").unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.click(("theme-item", builtin_index("One Dark")), cx);
        window.render_frame(cx);
        assert!(window.try_find("picker-error").is_some());
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.theme.name, "One Dark", "the choice applies anyway");
        assert!(ws.picker.is_some(), "the picker stays open to show why");
    });
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[theme\n");
}

#[gpui_kit::test]
fn appearance_change_closes_the_picker(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("One Light")), cx);
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_none());
        assert_eq!(ws.theme.name, "Catppuccin Mocha");
    });
}

#[gpui_kit::test]
fn appearance_event_without_a_mode_change_keeps_the_picker(cx: &mut TestAppContext) {
    // GPUI also reports appearance on Wayland decoration and window-control changes.
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("One Dark")), cx);
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.set_dark(false, cx));
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_some(), "still light: the picker stays open");
        assert_eq!(ws.theme.name, "One Dark", "and the preview stays");
    });
}

#[gpui_kit::test]
fn new_terminals_pop_in_then_settle(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
        let ws = ws.read(cx);
        let new = ws.space.canvas.top().unwrap();
        assert!(
            ws.space.appearing.contains_key(&new),
            "the new block is animating"
        );
    })
    .unwrap();
    std::thread::sleep(crate::motion::OPEN + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.appearing.is_empty(), "settled")
    });
}

#[gpui_kit::test]
fn a_closed_block_fades_as_a_ghost_that_ignores_clicks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let n = first_id(&ws, cx).0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(CLOSE, cx);
        window.render_frame(cx);
        // Checked right after the frame that drew it: the ghost lives 0.18 s, and a slow
        // machine may spend longer than that on the clicks below.
        assert_eq!(ws.read(cx).space.ghosts.len(), 1);
        // A double click where the ghost is reaches the canvas: a terminal opens there.
        let at = window.find(("ghost", n)).bounds().center();
        let at = (f32::from(at.x), f32::from(at.y));
        click(window, cx, at, 1);
        click(window, cx, at, 2);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.canvas.blocks().len(),
            1,
            "the click went through the ghost"
        );
    });
    std::thread::sleep(crate::motion::CLOSE + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.ghosts.is_empty(), "faded away")
    });
}

#[gpui_kit::test]
fn blink_toggles_only_while_the_cursor_blinks(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.cursor_blinks = false;
        ws.tick_blink(cx);
        assert!(ws.blink_on, "steady cursors stay visible");
        ws.cursor_blinks = true;
        ws.tick_blink(cx);
        assert!(!ws.blink_on);
        ws.tick_blink(cx);
        assert!(ws.blink_on);
    });
}

fn write_config(dir: &std::path::Path, text: &str) {
    std::fs::write(dir.join("config.toml"), text).unwrap();
}

fn reload(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.reload_config(window, cx))
    })
    .unwrap();
}

fn notification_count(handle: AnyWindowHandle, cx: &mut TestAppContext) -> usize {
    use gpui_kit::component::WindowExt as _;
    cx.update_window(handle, |_, window, cx| window.notifications(cx).len())
        .unwrap()
}

#[gpui_kit::test]
fn a_bigger_font_keeps_blocks_and_shrinks_grids(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let id = first_id(&ws, cx);
    let (rect, cell, grid) = ws.read_with(cx, |ws, _| {
        (
            ws.space.canvas.get(id).unwrap().rect,
            ws.cell.size,
            ws.space.terminal(id).unwrap().term.grid_size(),
        )
    });
    write_config(dir.path(), "[font]\nsize = 28\n");
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.canvas.get(id).unwrap().rect,
            rect,
            "the block keeps its rect"
        );
        assert!(ws.cell.size.w > cell.w);
        let now = ws.space.terminal(id).unwrap().term.grid_size();
        assert!(now.0 < grid.0 && now.1 < grid.1, "{now:?} vs {grid:?}");
    });
}

#[gpui_kit::test]
fn theme_mode_applies_on_reload_and_an_old_window_table_is_ignored(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    write_config(
        dir.path(),
        "[window]\nblur = false\n[theme]\nmode = \"dark\"\n",
    );
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| {
        assert!(!ws.config_failed, "the old [window] table loads");
        assert_eq!(
            ws.theme.name, "Catppuccin Mocha",
            "dark pinned on a light system"
        );
    });
}

#[gpui_kit::test]
fn cursor_options_reach_open_terminals(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let id = first_id(&ws, cx);
    write_config(dir.path(), "[cursor]\nshape = \"bar\"\nblink = true\n");
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| {
        let mut snap = templum_terminal::Snapshot::default();
        snap.refresh(
            ws.space.terminal(id).unwrap().term.shared(),
            &ws.theme.palette,
        );
        let c = snap.cursor.unwrap();
        assert_eq!(
            (c.shape, c.blinking),
            (templum_terminal::CursorShape::Beam, true)
        );
    });
}

#[gpui_kit::test]
fn an_invalid_file_keeps_the_config_and_says_why(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    write_config(dir.path(), "[font]\nsize = 20\n");
    reload(handle, &ws, cx);
    write_config(dir.path(), "fnot = 1\n");
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.config.font.size, 20.0, "kept"));
    assert_eq!(notification_count(handle, cx), 1, "the error");
    write_config(dir.path(), "[font]\nsize = 20\n");
    reload(handle, &ws, cx);
    assert_eq!(notification_count(handle, cx), 2, "\"config.toml loaded\"");
}

#[gpui_kit::test]
fn missing_font_warns_but_applies_the_rest(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    write_config(dir.path(), "[font]\nfamily = \"No Such Font\"\nsize = 20\n");
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.cell.font_size, 20.0));
    assert_eq!(notification_count(handle, cx), 1, "the warning");
}

#[gpui_kit::test]
fn theme_reload_closes_the_picker(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.hover(("theme-item", builtin_index("One Light")), cx);
    })
    .unwrap();
    write_config(dir.path(), "[theme]\nlight = \"Apple Light\"\n");
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_none());
        assert_eq!(ws.theme.name, "Apple Light");
    });
}

#[gpui_kit::test]
fn new_terminals_inherit_the_focused_folder(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    // The plain path (cmd.exe can't start in a canonical \\?\ path), compared canonicalized.
    let start_dir = tempfile::tempdir().unwrap();
    let start = start_dir.path().to_path_buf();
    // A TOML literal string: Windows backslashes need no escaping.
    write_config(
        dir.path(),
        &format!("[shell]\nworking_directory = '{}'\n", start.display()),
    );
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    write_config(dir.path(), "[shell]\nworking_directory = \"inherit\"\n");
    reload(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    let new = ws.read_with(cx, |ws, _| ws.space.canvas.top().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let cwd = ws.read_with(cx, |ws, _| ws.space.terminal(new).unwrap().term.cwd());
        if let Some(cwd) = cwd {
            assert_eq!(cwd.canonicalize().unwrap(), start.canonicalize().unwrap());
            break;
        }
        assert!(Instant::now() < deadline, "no cwd");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn a_theme_added_after_startup_can_be_set_in_the_config(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    std::fs::create_dir(dir.path().join("themes")).unwrap();
    std::fs::write(
        dir.path().join("themes").join("mine.toml"),
        include_str!("../../themes/one-dark.toml"),
    )
    .unwrap();
    write_config(dir.path(), "[theme]\nlight = \"mine\"\n");
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.theme.name, "mine"));
    assert_eq!(notification_count(handle, cx), 0);
}

#[gpui_kit::test]
fn an_unknown_theme_name_warns(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    write_config(dir.path(), "[theme]\nlight = \"Nope\"\n");
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.theme.name, "Catppuccin Latte"));
    assert_eq!(notification_count(handle, cx), 1, "the warning");
}

#[cfg(target_os = "macos")]
const PICKER: &str = "cmd-shift-p";
#[cfg(not(target_os = "macos"))]
const PICKER: &str = "ctrl-shift-p";

#[gpui_kit::test]
fn remapped_shortcuts_apply_on_reload(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    write_config(dir.path(), "[keys]\nnew_terminal = \"ctrl-alt-n\"\n");
    reload(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.canvas.blocks().len(),
            1,
            "the old shortcut is gone"
        )
    });
    cx.update_window(handle, |_, window, cx| window.press("ctrl-alt-n", cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 2, "the new one works")
    });
}

#[gpui_kit::test]
fn the_picker_shortcut_toggles_and_others_wait(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(PICKER, cx);
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_some());
        assert_eq!(
            ws.space.canvas.blocks().len(),
            1,
            "no new terminal under the picker"
        );
    });
    cx.update_window(handle, |_, window, cx| window.press(PICKER, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.picker.is_none(), "closed like Esc"));
}

#[gpui_kit::test]
fn choosing_in_dark_mode_saves_the_dark_theme(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("theme-button", cx);
        window.render_frame(cx);
        window.click(("theme-item", builtin_index("One Dark")), cx);
    })
    .unwrap();
    let saved = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(saved.contains("dark = \"One Dark\""), "{saved}");
    assert!(!saved.contains("light"), "{saved}");
}

#[gpui_kit::test]
fn clicking_the_empty_canvas_unfocuses_terminals(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        click(
            window,
            cx,
            (f32::from(o.x) + 40.0, f32::from(o.y) + 40.0),
            1,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, None));
}

#[gpui_kit::test]
fn panning_the_canvas_keeps_the_focused_terminal(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let before = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            ws.read(cx).space.canvas.camera.origin
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        let o = ws.read(cx).origin;
        let from = point(o.x + px(40.), o.y + px(40.));
        window.drag(from, point(from.x + px(100.), from.y + px(50.)), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.focused, Some(id));
        let after = ws.space.canvas.camera.origin;
        assert_eq!(
            (before.x - after.x, before.y - after.y),
            (100.0, 50.0),
            "it panned"
        );
    });
}

#[test]
fn macos_option_types_the_layout_character() {
    use super::option_types_text;
    // German layout: Option+L types "@".
    assert!(option_types_text("l", Some("@"), true));
    // Option+arrows and Option+Backspace carry no text: they stay Meta.
    assert!(!option_types_text("left", None, true));
    assert!(!option_types_text("backspace", Some("\x7f"), true));
    // Elsewhere Alt is always Meta (AltGr is not reported as Alt on Linux).
    assert!(!option_types_text("l", Some("@"), false));
}

use crate::config::Edit;

fn frame(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
}

#[gpui_kit::test]
fn a_settings_edit_applies_on_the_next_frame(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    write_config(dir.path(), "# mine\n[cursor]\nblink = false\n");
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let cell = ws.read_with(cx, |ws, _| ws.cell.size);
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("font", "size", 28_i64),
            |c| c.font.size = 28.0,
            cx,
        );
        // Checked inside the update: the test app draws the window as soon as it ends.
        assert_eq!(ws.config.font.size, 14.0, "not before the frame");
    });
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.font.size, 28.0);
        assert!(ws.cell.size.w > cell.w, "measured with the window");
    });
    let path = dir.path().join("config.toml");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# mine\n[cursor]\nblink = false\n",
        "not saved yet"
    );
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# mine\n[cursor]\nblink = false"), "{text}");
    assert!(text.contains("[font]\nsize = 28"), "{text}");
}

#[gpui_kit::test]
fn a_reset_removes_the_key_from_the_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    write_config(
        dir.path(),
        "[cursor]\nblink = true # mine\nshape = \"bar\"\n",
    );
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    // What a field's "reset to default" does.
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::remove("cursor", "blink"),
            |c| c.cursor.blink = false,
            cx,
        )
    });
    frame(handle, cx);
    ws.update(cx, |ws, _| {
        assert!(!ws.config.cursor.blink);
        ws.save();
    });
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(!text.contains("blink"), "{text}");
    assert!(text.contains("shape = \"bar\""), "{text}");
}

#[gpui_kit::test]
fn edits_are_saved_once_they_settle(cx: &mut TestAppContext) {
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let path = dir.path().join("config.toml");
    let scrollback = |n: usize, cx: &mut TestAppContext| {
        ws.update(cx, |ws, cx| {
            ws.edit(
                Edit::set("terminal", "scrollback", n as i64),
                |c| c.terminal.scrollback = n,
                cx,
            )
        });
        frame(handle, cx);
    };
    scrollback(1000, cx);
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert!(!path.exists(), "not before 0.3 s");
    scrollback(2000, cx);
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert!(!path.exists(), "the second edit restarted the wait");
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("scrollback = 2000"), "{text}");
    assert!(!text.contains("1000"), "one write, the last value: {text}");
}

#[gpui_kit::test]
fn an_edit_that_changes_nothing_is_not_saved(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("terminal", "scrollback", 10_000_i64),
            |c| c.terminal.scrollback = 10_000,
            cx,
        );
        assert!(ws.unsaved.is_empty());
        assert!(ws.save().is_none());
    });
    assert!(!dir.path().join("config.toml").exists());
}

#[gpui_kit::test]
fn a_broken_file_is_never_overwritten_and_says_why(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    // Broken after startup, as if the user were halfway through an edit.
    write_config(dir.path(), "fnot = = 1\n");
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("cursor", "blink", true),
            |c| c.cursor.blink = true,
            cx,
        )
    });
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.save_or_notify(window, cx))
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("config.toml")).unwrap(),
        "fnot = = 1\n"
    );
    ws.read_with(cx, |ws, _| {
        assert!(ws.config.cursor.blink, "still in effect");
        assert_eq!(
            ws.save_error.as_deref(),
            Some("Not saved: config.toml has an error on line 1")
        );
    });
    assert_eq!(notification_count(handle, cx), 1);
    write_config(dir.path(), "[cursor]\nblink = true\n");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.poll_config(window, cx))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.save_error, None, "fixed"));
}

#[gpui_kit::test]
fn closing_the_window_saves_what_is_pending(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("cursor", "blink", true),
            |c| c.cursor.blink = true,
            cx,
        )
    });
    // The test's handle would keep the workspace alive.
    drop(ws);
    cx.update_window(handle, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("blink = true"), "{text}");
}

#[gpui_kit::test]
fn without_a_config_folder_edits_apply_and_nothing_is_written(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("cursor", "blink", true),
            |c| c.cursor.blink = true,
            cx,
        )
    });
    frame(handle, cx);
    ws.update(cx, |ws, _| {
        assert!(ws.config.cursor.blink);
        assert_eq!(ws.save(), None);
        assert_eq!(ws.save_error, None, "the footer says it, not an error");
    });
}

#[cfg(target_os = "macos")]
const SETTINGS: &str = "cmd-,";
#[cfg(not(target_os = "macos"))]
const SETTINGS: &str = "ctrl-,";

fn panel_open(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> bool {
    ws.read_with(cx, |ws, _| ws.settings.as_ref().is_some_and(|p| !p.closing))
}

/// Lets the panel's slide finish (it runs on the real clock, like the block animations).
fn settle_slide(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    std::thread::sleep(crate::motion::SLIDE + std::time::Duration::from_millis(50));
    frame(handle, cx);
}

#[gpui_kit::test]
fn the_settings_shortcut_and_buttons_open_and_close_the_panel(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(panel_open(&ws, cx), "the shortcut opens it");
    cx.update_window(handle, |_, window, cx| {
        window.press(SETTINGS, cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(!panel_open(&ws, cx), "and closes it");
    settle_slide(handle, cx);
    ws.read_with(cx, |ws, _| assert!(ws.settings.is_none(), "slid out"));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    assert!(panel_open(&ws, cx), "the gear opens it");
    cx.update_window(handle, |_, window, cx| window.click("settings-close", cx))
        .unwrap();
    assert!(!panel_open(&ws, cx), "the close button closes it");
}

#[gpui_kit::test]
fn the_panel_opens_on_appearance_and_tabs_switch_sections(cx: &mut TestAppContext) {
    let (handle, _ws) = open(cx);
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("field-theme.mode").is_some());
        assert!(window.try_find("shortcut-new_terminal").is_none());
        window.click("settings-tab-shortcuts", cx);
        window.render_frame(cx);
        assert!(window.try_find("shortcut-new_terminal").is_some());
        assert!(window.try_find("field-theme.mode").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_blink_switch_edits_the_config_and_its_reset_removes_the_key(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-terminal", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("reset-cursor.blink").is_none(),
            "no reset while at the default"
        );
        window.click("field-cursor.blink", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(ws.read(cx).config.cursor.blink, "the switch turned it on");
        assert!(window.try_find("reset-cursor.blink").is_some());
        window.click("reset-cursor.blink", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(!ws.read(cx).config.cursor.blink, "back to the default");
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap_or_default();
    assert!(!text.contains("blink"), "{text}");
}

#[gpui_kit::test]
fn the_integration_and_notification_switches_turn_them_off_and_back(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    let on = |ws: &Workspace| {
        (
            ws.config.shell.integration,
            ws.config.terminal.notifications,
        )
    };
    for (tab, field, reset, line) in [
        (
            "settings-tab-shell",
            "field-shell.integration",
            "reset-shell.integration",
            "integration = false",
        ),
        (
            "settings-tab-terminal",
            "field-terminal.notifications",
            "reset-terminal.notifications",
            "notifications = false",
        ),
    ] {
        cx.update_window(handle, |_, window, cx| {
            window.click(tab, cx);
            window.render_frame(cx);
            assert!(window.try_find(reset).is_none());
            window.click(field, cx);
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .unwrap();
        ws.update(cx, |ws, _| ws.save());
        let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
        assert!(text.contains(line), "{text}");
        cx.update_window(handle, |_, window, cx| {
            window.click(reset, cx);
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .unwrap();
    }
    assert_eq!(ws.read_with(cx, |ws, _| on(ws)), (true, true), "back on");
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        !text.contains("integration") && !text.contains("notifications"),
        "{text}"
    );
}

#[cfg(target_os = "macos")]
const PREVIOUS_PROMPT: &str = "cmd-up";
#[cfg(all(unix, not(target_os = "macos")))]
const PREVIOUS_PROMPT: &str = "ctrl-shift-up";
#[cfg(target_os = "macos")]
const NEXT_PROMPT: &str = "cmd-down";
#[cfg(all(unix, not(target_os = "macos")))]
const NEXT_PROMPT: &str = "ctrl-shift-down";

/// A command whose output scrolled its prompt out of view: the shortcut brings the prompt to
/// the top, the other goes back down.
#[cfg(unix)]
#[gpui_kit::test]
fn the_command_shortcuts_jump_between_prompts(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    // Bash 4.4 brought PS0, which reports a command's start (macOS has 3.2).
    let ps0 = std::process::Command::new("bash")
        .args([
            "-c",
            "(( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 404 ))",
        ])
        .status()
        .is_ok_and(|s| s.success());
    if !ps0 {
        eprintln!("skipped: this bash doesn't report commands");
        return;
    }
    let data = tempfile::tempdir().unwrap();
    let bash = Some(("bash".to_string(), Vec::new()));
    let (handle, ws) = open_as(cx, bash, None, Some(data.path().to_path_buf()));
    let id = first_id(&ws, cx);
    let top = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            (snap.rows[0].text.clone(), snap.rows[0].mark)
        })
    };
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(id).unwrap().term.write(&b"seq 100\r"[..])
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while ws.read_with(cx, |ws, _| {
        ws.space.terminal(id).unwrap().term.status().finished
    }) < 1
    {
        assert!(Instant::now() < deadline, "seq never ended");
        std::thread::sleep(Duration::from_millis(50));
    }
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(PREVIOUS_PROMPT, cx);
    })
    .unwrap();
    let (text, mark) = top(cx);
    assert!(text.ends_with("seq 100"), "{text:?}");
    assert_eq!(mark, Some(templum_terminal::Mark::Ended(Some(0))));
    cx.update_window(handle, |_, window, cx| window.press(NEXT_PROMPT, cx))
        .unwrap();
    assert!(
        !top(cx).0.contains("seq"),
        "back at the bottom: {:?}",
        top(cx)
    );
}

/// The startup terminal runs bash with Templum's integration; turned off, a new one doesn't.
#[cfg(unix)]
#[gpui_kit::test]
fn new_terminals_get_the_shell_integration_unless_turned_off(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let data = tempfile::tempdir().unwrap();
    let bash = Some(("bash".to_string(), Vec::new()));
    let (handle, ws) = open_as(cx, bash, None, Some(data.path().to_path_buf()));
    assert!(data.path().join("shell-integration/templum.bash").is_file());
    let shown = |id, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        })
    };
    let ask = |id, want: &str, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.space
                .terminal(id)
                .unwrap()
                .term
                .write(&b"echo \"[${__templum_installed-no}]\"\r"[..])
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !shown(id, cx).contains(want) {
            assert!(Instant::now() < deadline, "no {want}: {}", shown(id, cx));
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let first = first_id(&ws, cx);
    ask(first, "[1]\n", cx);
    ws.update(cx, |ws, _| ws.config.shell.integration = false);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    let second = ws.read_with(cx, |ws, _| ws.space.focused.unwrap());
    assert_ne!(first, second);
    ask(second, "[no]\n", cx);
}

#[gpui_kit::test]
fn the_agents_tab_resets_ours_and_deletes_yours(cx: &mut TestAppContext) {
    use crate::agents::Draft;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").unwrap();
        let draft = Draft {
            command: "claude --model opus".into(),
            ..Draft::of(claude)
        };
        ws.save_agent(Some("claude"), &draft, cx).unwrap();
        let mine = Draft {
            name: "Mine".into(),
            command: "mine".into(),
            resume: String::new(),
            show: true,
        };
        ws.save_agent(None, &mine, cx).unwrap();
    });
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-agents", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("field-agents.mine").is_some(),
            "yours is listed"
        );
        assert!(
            window.try_find("reset-agents.codex").is_none(),
            "unchanged: no ↺"
        );
        window.click("reset-agents.claude", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        let claude = ws
            .read(cx)
            .agents
            .iter()
            .find(|a| a.id == "claude")
            .unwrap()
            .clone();
        assert_eq!(claude.command, "claude", "back to Templum's");
        // Yours is last: scroll down to it.
        window.scroll(
            "settings-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-5000.))),
            cx,
        );
        window.render_frame(cx);
        window.click("edit-agent-mine", cx);
        window.render_frame(cx);
    })
    .unwrap();
    // The dialog slides in for 0.25 s first: presses meanwhile are lost.
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("delete-agent", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(ws.read(cx).agents.iter().all(|a| a.id != "mine"), "deleted");
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_shell_list_sets_the_program_and_its_arguments(cx: &mut TestAppContext) {
    use templum_terminal::ShellOption;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    // Two shells "found" on this computer.
    ws.update(cx, |ws, _| {
        ws.shells = vec![
            ShellOption {
                label: "Git Bash".into(),
                program: "C:\\Git\\bin\\bash.exe".into(),
                args: vec!["--login".into(), "-i".into()],
            },
            ShellOption {
                label: "PowerShell 7".into(),
                program: "pwsh".into(),
                args: vec!["-NoLogo".into()],
            },
        ];
        ws.default_shell = "Git Bash".into();
    });
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-shell", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("reset-shell.shell").is_none(),
            "the default"
        );
        window
            .within("field-shell.shell")
            .click("menu-shell.shell", cx);
        window.render_frame(cx);
        // Default (Git Bash), Git Bash, PowerShell 7.
        window.within("popup-menu").click(2usize, cx);
        window.render_frame(cx);
        window.render_frame(cx);
        let shell = ws.read(cx).config.shell.clone();
        assert_eq!(shell.program.as_deref(), Some("pwsh"));
        assert_eq!(shell.args, ["-NoLogo"]);
        assert!(window.try_find("reset-shell.shell").is_some());
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        text.contains("program = \"pwsh\"") && text.contains("-NoLogo"),
        "{text}"
    );
    cx.update_window(handle, |_, window, cx| {
        window.click("reset-shell.shell", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        let shell = ws.read(cx).config.shell.clone();
        assert_eq!(
            (shell.program, shell.args.len()),
            (None, 0),
            "back to the default"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn esc_closes_the_panel_only_from_inside_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
        window.press("escape", cx);
    })
    .unwrap();
    assert!(!panel_open(&ws, cx), "Esc in the panel closes it");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
    })
    .unwrap();
    settle_slide(handle, cx);
    // Click the terminal's left part, clear of the panel on the right.
    cx.update_window(handle, |_, window, cx| {
        let (o, r) = {
            let ws = ws.read(cx);
            let rect = ws.space.canvas.get(id).unwrap().rect;
            (ws.origin, ws.space.canvas.camera.rect_to_screen(rect))
        };
        let at = (
            f32::from(o.x) + r.x + 20.0,
            f32::from(o.y) + r.y + templum_canvas::chrome::TITLE_H + 20.0,
        );
        click(window, cx, at, 1);
        window.render_frame(cx);
        window.press("escape", cx);
        let focused = ws.read(cx).focus_handle.is_focused(window);
        assert!(focused, "keys go to the terminal");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.focused, Some(id)));
    assert!(panel_open(&ws, cx), "Esc went to the terminal");
}

#[gpui_kit::test]
fn closing_the_panel_saves_at_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("cursor", "blink", true),
            |c| c.cursor.blink = true,
            cx,
        )
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
    })
    .unwrap();
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("blink = true"), "{text}");
}

#[gpui_kit::test]
fn a_number_applies_on_enter_or_a_step_not_while_typing(cx: &mut TestAppContext) {
    use crate::settings::{NumberState, SCROLLBACK};
    use gpui_kit::component::input::{InputEvent, NumberInputEvent, StepAction};
    let (handle, ws) = open(cx);
    let state = cx
        .update_window(handle, |_, window, cx| {
            let ws = ws.clone();
            cx.new(|cx| NumberState::new(ws, &SCROLLBACK, 10_000.0, window, cx))
        })
        .unwrap();
    let scrollback =
        |cx: &mut TestAppContext| ws.read_with(cx, |ws, _| ws.config.terminal.scrollback);
    // Typing "20000" passes through "2": applied, it would trim every terminal's history.
    cx.update_window(handle, |_, window, cx| {
        let input = state.read(cx).input.clone();
        input.update(cx, |input, cx| input.set_value("2", window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(scrollback(cx), 10_000, "not while typing");
    cx.update_window(handle, |_, window, cx| {
        let input = state.read(cx).input.clone();
        input.update(cx, |input, cx| {
            input.set_value("20000", window, cx);
            cx.emit(InputEvent::PressEnter {
                secondary: false,
                shift: false,
            });
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(scrollback(cx), 20_000, "Enter applies it");
    cx.update_window(handle, |_, window, cx| {
        let input = state.read(cx).input.clone();
        input.update(cx, |_, cx| {
            cx.emit(NumberInputEvent::Step(StepAction::Decrement))
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(scrollback(cx), 19_000, "a step applies at once");
}

#[gpui_kit::test]
fn a_change_made_while_the_file_is_broken_is_saved_once_it_is_fixed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    write_config(dir.path(), "fnot = = 1\n");
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("cursor", "blink", true),
            |c| c.cursor.blink = true,
            cx,
        )
    });
    frame(handle, cx);
    ws.update(cx, |ws, _| {
        assert!(ws.save().is_some(), "the file is broken")
    });
    // The user fixes the file, with something else in it; the poll notices.
    write_config(dir.path(), "[cursor]\nshape = \"bar\"\n");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.poll_config(window, cx))
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.config.cursor.blink, "the panel's change survived");
        assert_eq!(
            ws.config.cursor.shape,
            crate::config::Shape::Bar,
            "and the fix applied"
        );
        assert_eq!(ws.save_error, None);
    });
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(
        text.contains("blink = true") && text.contains("shape = \"bar\""),
        "{text}"
    );
}

fn poll(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.poll_config(window, cx))
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_hand_edit_that_a_panel_save_writes_over_still_applies(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    // A hand edit the poll hasn't seen yet, then a panel change saved before the poll comes.
    write_config(dir.path(), "[cursor]\nshape = \"bar\"\n");
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("cursor", "blink", true),
            |c| c.cursor.blink = true,
            cx,
        )
    });
    frame(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    poll(handle, &ws, cx);
    ws.read_with(cx, |ws, _| {
        assert!(ws.config.cursor.blink);
        assert_eq!(
            ws.config.cursor.shape,
            crate::config::Shape::Bar,
            "the hand edit applies too"
        );
    });
}

#[gpui_kit::test]
fn a_hand_edit_of_the_shell_drops_what_the_panel_was_typing(cx: &mut TestAppContext) {
    use crate::settings::{args_error, args_text, set_args};
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| set_args(ws, "--login \"a b", cx));
    frame(handle, cx);
    write_config(dir.path(), "[shell]\nargs = [\"-l\"]\n");
    poll(handle, &ws, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(args_text(ws), "-l", "the file's value shows");
        assert_eq!(args_error(ws), None);
    });
}

#[gpui_kit::test]
fn the_footer_shows_a_failed_save_until_the_file_is_fixed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let status = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("settings-status").is_some()
        })
        .unwrap()
    };
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
    })
    .unwrap();
    assert!(!status(cx), "nothing to say");
    write_config(dir.path(), "fnot = = 1\n");
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("cursor", "blink", true),
            |c| c.cursor.blink = true,
            cx,
        )
    });
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.save_or_notify(window, cx))
    })
    .unwrap();
    assert!(status(cx), "the save error shows");
    write_config(dir.path(), "[cursor]\nblink = true\n");
    poll(handle, &ws, cx);
    assert!(!status(cx), "gone once the file is fixed");
}

#[gpui_kit::test]
fn a_theme_picked_right_after_a_panel_change_is_the_one_saved(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("theme", "light", "One Light"),
            |c| c.theme.light = "One Light".into(),
            cx,
        )
    });
    frame(handle, cx);
    // The picker, before the panel's change is written.
    ws.update(cx, |ws, cx| {
        ws.choose_theme(builtin_index("Apple Light"), cx)
    });
    frame(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("light = \"Apple Light\""), "{text}");
    ws.read_with(cx, |ws, _| assert_eq!(ws.config.theme.light, "Apple Light"));
}

#[gpui_kit::test]
fn a_reset_or_another_folder_choice_drops_what_was_typed(cx: &mut TestAppContext) {
    use crate::settings::{
        args_error, args_text, folder_hint, folder_text, set_args, set_folder, set_folder_kind,
    };
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| set_args(ws, "--login \"a b", cx));
    frame(handle, cx);
    // What the field's "reset to default" does.
    ws.update(cx, |ws, cx| {
        ws.edit(Edit::remove("shell", "args"), |c| c.shell.args.clear(), cx)
    });
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(args_text(ws), "");
        assert_eq!(args_error(ws), None);
    });
    // "~" typed, then Home chosen, then "A folder…" again (it starts at the home folder).
    let steps: [fn(&mut Workspace, &mut Context<Workspace>); 4] = [
        |ws, cx| set_folder_kind(ws, "folder", cx),
        |ws, cx| set_folder(ws, "~", cx),
        |ws, cx| set_folder_kind(ws, "home", cx),
        |ws, cx| set_folder_kind(ws, "folder", cx),
    ];
    for step in steps {
        ws.update(cx, step);
        frame(handle, cx);
    }
    let home = std::env::home_dir().unwrap().display().to_string();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            folder_text(ws),
            home,
            "the folder in effect, not the old \"~\""
        );
        assert_eq!(folder_hint(ws), None);
    });
}

#[gpui_kit::test]
fn the_default_font_removes_the_family(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| {
        crate::settings::set_family(ws, "Some Mono", cx)
    });
    frame(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    let path = dir.path().join("config.toml");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("family = \"Some Mono\"")
    );
    ws.update(cx, |ws, cx| crate::settings::set_family(ws, "", cx));
    frame(handle, cx);
    ws.update(cx, |ws, _| {
        assert_eq!(ws.config.font.family, None);
        ws.save();
    });
    assert!(!std::fs::read_to_string(&path).unwrap().contains("family"));
}

#[gpui_kit::test]
fn an_unclosed_quote_waits_for_the_closing_one(cx: &mut TestAppContext) {
    use crate::settings::set_args;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.update(cx, |ws, cx| set_args(ws, "--login \"a b", cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert!(ws.config.shell.args.is_empty(), "not applied");
        assert_eq!(
            crate::settings::args_text(ws),
            "--login \"a b",
            "kept as typed"
        );
        assert_eq!(crate::settings::args_error(ws), Some("Unclosed quote"));
    });
    ws.update(cx, |ws, cx| set_args(ws, "--login \"a b\"", cx));
    frame(handle, cx);
    ws.update(cx, |ws, _| {
        assert_eq!(ws.config.shell.args, ["--login", "a b"]);
        assert_eq!(crate::settings::args_error(ws), None);
        ws.save();
    });
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("args = [\"--login\", \"a b\"]"), "{text}");
}

#[gpui_kit::test]
fn a_bare_tilde_waits_for_the_rest_of_the_path(cx: &mut TestAppContext) {
    use crate::config::WorkingDirectory;
    use crate::settings::{folder_hint, folder_text, set_folder, set_folder_kind};
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let target = dir.path().join("work");
    ws.update(cx, |ws, cx| set_folder_kind(ws, "folder", cx));
    frame(handle, cx);
    ws.update(cx, |ws, cx| set_folder(ws, "~", cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert!(
            matches!(ws.config.shell.working_directory, WorkingDirectory::Path(_)),
            "\"~\" alone doesn't switch to Home"
        );
        assert_eq!(folder_text(ws), "~", "kept as typed");
        assert!(folder_hint(ws).is_some(), "and the field says why");
    });
    let typed = target.display().to_string();
    ws.update(cx, |ws, cx| set_folder(ws, &typed, cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.config.shell.working_directory,
            WorkingDirectory::Path(target.clone())
        );
        assert_eq!(folder_text(ws), typed);
        assert_eq!(folder_hint(ws), None);
    });
    // Written now, while the temporary folder still exists.
    ws.update(cx, |ws, _| ws.save());
}

#[gpui_kit::test]
fn start_folder_and_program_choices_write_the_file(cx: &mut TestAppContext) {
    use crate::settings::{set_folder_kind, set_program};
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let path = dir.path().join("config.toml");
    let mut check = |change: &dyn Fn(&mut Workspace, &mut Context<Workspace>), want: &str| {
        ws.update(cx, |ws, cx| change(ws, cx));
        frame(handle, cx);
        ws.update(cx, |ws, _| ws.save());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(want), "want {want:?} in {text}");
    };
    check(
        &|ws, cx| set_folder_kind(ws, "home", cx),
        "working_directory = \"~\"",
    );
    check(
        &|ws, cx| set_folder_kind(ws, "inherit", cx),
        "working_directory = \"inherit\"",
    );
    check(&|ws, cx| set_program(ws, "fish", cx), "program = \"fish\"");
    ws.update(cx, |ws, cx| set_program(ws, "", cx));
    frame(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    assert!(!std::fs::read_to_string(&path).unwrap().contains("program"));
}

#[gpui_kit::test]
fn the_program_is_saved_trimmed_but_typing_keeps_its_spaces(cx: &mut TestAppContext) {
    use crate::settings::{program_text, set_program};
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    // Halfway through typing "C:\Program Files\…".
    ws.update(cx, |ws, cx| set_program(ws, "C:\\Program ", cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.shell.program.as_deref(), Some("C:\\Program"));
        assert_eq!(
            program_text(ws),
            "C:\\Program ",
            "the space stays in the field"
        );
    });
    ws.update(cx, |ws, cx| set_program(ws, " fish ", cx));
    frame(handle, cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("program = \"fish\""), "{text}");
    ws.update(cx, |ws, cx| set_program(ws, "  ", cx));
    frame(handle, cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.config.shell.program, None));
}

use crate::keys::{Command, Keys};

/// Opens the panel (unless it is open) and starts recording `command`'s shortcut.
fn start_recording(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    command: Command,
    cx: &mut TestAppContext,
) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Pressed again, the shortcut would close the panel.
        if !ws.read(cx).settings.as_ref().is_some_and(|p| !p.closing) {
            window.press(SETTINGS, cx);
        }
        window.render_frame(cx);
        ws.update(cx, |ws, cx| ws.record(command, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn recording_a_free_shortcut_applies_and_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    start_recording(handle, &ws, Command::NewTerminal, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("ctrl-alt-n", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| {
        assert_eq!(ws.recording, None);
        let pressed = gpui_kit::Keystroke::parse("ctrl-alt-n").unwrap();
        assert_eq!(ws.keymap.command(&pressed), Some(Command::NewTerminal));
        assert_eq!(
            ws.keymap.keystrokes(Command::NewTerminal).len(),
            1,
            "the only one"
        );
        ws.save();
    });
    let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("new_terminal = \"ctrl-alt-n\""), "{text}");
}

#[gpui_kit::test]
fn a_taken_shortcut_keeps_recording(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    cx.update_window(handle, |_, window, cx| window.press(NEW, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.recording, Some(Command::Copy));
        let shortcut = if cfg!(target_os = "macos") {
            "Cmd+T"
        } else {
            "Ctrl+Shift+T"
        };
        assert_eq!(
            ws.key_note.as_deref(),
            Some(format!("{shortcut} is already New terminal").as_str())
        );
        assert_eq!(ws.space.canvas.blocks().len(), 1, "no shortcut ran");
        assert!(ws.config.keys.copy.is_none(), "nothing changed");
    });
}

#[gpui_kit::test]
fn esc_cancels_recording_and_backspace_clears(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.recording, None);
        assert!(ws.config.keys.copy.is_none());
    });
    assert!(
        panel_open(&ws, cx),
        "Esc cancelled the recording, not the panel"
    );
    start_recording(handle, &ws, Command::Copy, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("backspace", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.keys.copy, Some(Keys::Many(vec![])));
        assert!(ws.keymap.keystrokes(Command::Copy).is_empty());
    });
}

#[gpui_kit::test]
fn a_modifier_alone_keeps_recording(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    // What the platforms send for a modifier key: no key press, a change of modifiers.
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::ModifiersChanged(gpui_kit::ModifiersChangedEvent {
                modifiers: Modifiers::shift(),
                capslock: Default::default(),
            }),
            cx,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.recording, Some(Command::Copy));
        assert_eq!(ws.key_note, None);
    });
}

/// A key held down: the platform repeats its press.
fn hold(key: &str, window: &mut Window, cx: &mut gpui_kit::App) {
    window.dispatch_event(
        PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
            keystroke: gpui_kit::Keystroke::parse(key).unwrap(),
            is_held: true,
            prefer_character_input: false,
        }),
        cx,
    );
}

#[gpui_kit::test]
fn ctrl_backspace_and_ctrl_esc_record_like_other_keys(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    for key in ["ctrl-backspace", "ctrl-escape"] {
        start_recording(handle, &ws, Command::Copy, cx);
        cx.update_window(handle, |_, window, cx| {
            window.press(key, cx);
            window.render_frame(cx);
        })
        .unwrap();
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.config.keys.copy, Some(Keys::One(key.into())))
        });
    }
}

#[gpui_kit::test]
fn a_held_key_runs_nothing_from_the_panel(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Settings, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("ctrl-alt-s", cx);
        window.render_frame(cx);
        // Still held after the recording took it: the new shortcut mustn't run.
        hold("ctrl-alt-s", window, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.config.keys.settings,
            Some(Keys::One("ctrl-alt-s".into()))
        )
    });
    assert!(panel_open(&ws, cx), "the repeat didn't close the panel");
}

#[gpui_kit::test]
fn the_theme_picker_gets_its_keys_while_the_panel_has_the_focus(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
        window.press(PICKER, cx);
        window.render_frame(cx);
        window.press("down", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_some(), "the shortcut opened it");
        assert_ne!(ws.theme.name, "Catppuccin Latte", "↓ previews the next one");
    });
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.picker.is_none());
        assert_eq!(ws.theme.name, "Catppuccin Latte");
    });
    assert!(panel_open(&ws, cx), "Esc closed the picker, not the panel");
}

#[gpui_kit::test]
fn canvas_shortcuts_run_from_the_panel_but_terminal_ones_dont(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
        // Close acts on the focused terminal, which doesn't have the keyboard.
        window.press(CLOSE, cx);
        window.press(ZOOM_IN, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 1, "close didn't run");
        assert!(ws.space.canvas.camera.zoom() > 1.0, "zoom did");
    });
    cx.update_window(handle, |_, window, cx| window.press(NEW, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.blocks().len(), 2));
    assert!(panel_open(&ws, cx));
}

#[gpui_kit::test]
fn a_reset_that_would_share_a_shortcut_is_refused(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let taken = Command::NewTerminal.defaults(cfg!(target_os = "macos"))[0];
    write_config(
        dir.path(),
        &format!("[keys]\nnew_terminal = \"ctrl-alt-y\"\nclose_terminal = \"{taken}\"\n"),
    );
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let reset = |command: Command, cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.reset_shortcut(command, window, cx));
            window.render_frame(cx);
        })
        .unwrap();
    };
    reset(Command::NewTerminal, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.config.keys.new_terminal,
            Some(Keys::One("ctrl-alt-y".into())),
            "close_terminal has its default"
        )
    });
    assert_eq!(notification_count(handle, cx), 1, "and says so");
    reset(Command::CloseTerminal, cx);
    reset(Command::NewTerminal, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.keys.new_terminal, None, "free again: reset");
        assert_eq!(ws.config.keys.close_terminal, None);
    });
}

#[gpui_kit::test]
fn a_key_without_ctrl_alt_or_super_is_refused(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::NewTerminal, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("n", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.recording, Some(Command::NewTerminal), "still recording");
        assert!(ws.key_note.is_some(), "and says why");
        assert!(
            ws.config.keys.new_terminal.is_none(),
            "terminals keep their n"
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.press("f5", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.config.keys.new_terminal,
            Some(Keys::One("f5".into())),
            "F keys may stand alone"
        );
    });
}

#[gpui_kit::test]
fn clicking_elsewhere_in_the_panel_stops_recording(cx: &mut TestAppContext) {
    // No config folder: the footer shows its line, a place to click.
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| window.click("settings-status", cx))
        .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.recording, None));
    assert!(panel_open(&ws, cx));
}

#[gpui_kit::test]
fn clicking_a_terminal_stops_recording(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    start_recording(handle, &ws, Command::Copy, cx);
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        let (o, r) = {
            let ws = ws.read(cx);
            let rect = ws.space.canvas.get(id).unwrap().rect;
            (ws.origin, ws.space.canvas.camera.rect_to_screen(rect))
        };
        let at = (
            f32::from(o.x) + r.x + 20.0,
            f32::from(o.y) + r.y + templum_canvas::chrome::TITLE_H + 20.0,
        );
        click(window, cx, at, 1);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.recording, None));
}

#[gpui_kit::test]
fn closing_the_panel_stops_recording(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    start_recording(handle, &ws, Command::Copy, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.close_settings(window, cx);
            assert_eq!(ws.recording, None);
        })
    })
    .unwrap();
}

#[cfg(target_os = "macos")]
const QUIT: &str = "cmd-q";
#[cfg(not(target_os = "macos"))]
const QUIT: &str = "ctrl-shift-q";

#[gpui_kit::test]
fn quitting_takes_a_second_press(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(QUIT, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.quit_armed.is_some(), "the first press only warns")
    });
    assert_eq!(notification_count(handle, cx), 1);
    cx.update_window(handle, |_, window, cx| window.press(QUIT, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.quit_armed.is_none(), "the second one quits")
    });
    assert_eq!(notification_count(handle, cx), 1, "no second warning");
}

#[gpui_kit::test]
fn reset_all_untangles_shortcuts_that_traded_keys(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let macos = cfg!(target_os = "macos");
    let new = Command::NewTerminal.defaults(macos)[0];
    let close = Command::CloseTerminal.defaults(macos)[0];
    write_config(
        dir.path(),
        &format!("[keys]\nnew_terminal = \"{close}\"\nclose_terminal = \"{new}\"\n"),
    );
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    // "Reset All" calls each row's reset in one go, with no frame between them.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.reset_shortcut(Command::NewTerminal, window, cx);
            ws.reset_shortcut(Command::CloseTerminal, window, cx);
        })
    })
    .unwrap();
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.config.keys.new_terminal, None);
        assert_eq!(ws.config.keys.close_terminal, None);
    });
    assert_eq!(notification_count(handle, cx), 0, "nothing refused");
}

#[gpui_kit::test]
fn holding_the_quit_shortcut_is_not_a_second_press(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(QUIT, cx);
        hold(QUIT, window, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.quit_armed.is_some(), "still waiting for a second press")
    });
}

#[gpui_kit::test]
fn a_late_second_press_warns_again(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let long_ago = std::time::Instant::now()
        .checked_sub(super::QUIT_AGAIN + std::time::Duration::from_millis(100))
        .unwrap();
    ws.update(cx, |ws, _| ws.quit_armed = Some(long_ago));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(QUIT, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.quit_armed.is_some_and(|at| at > long_ago), "armed again")
    });
    assert_eq!(notification_count(handle, cx), 1);
}

#[gpui_kit::test]
fn quit_works_from_the_settings_panel(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(SETTINGS, cx);
        window.render_frame(cx);
        window.press(QUIT, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert!(ws.quit_armed.is_some()));
}

use super::{MAX_COLS, MAX_ROWS};
use templum_canvas::{BlockId, Camera, Point, Rect, chrome};

fn write_canvas(dir: &std::path::Path, text: &str) {
    std::fs::create_dir_all(dir.join("workspaces")).unwrap();
    std::fs::write(crate::session::path(dir, crate::session::DEFAULT_ID), text).unwrap();
}

/// Waits for a terminal's shell to report its folder (canonicalized).
fn folder_of(ws: &Entity<Workspace>, id: BlockId, cx: &mut TestAppContext) -> std::path::PathBuf {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(cwd) = ws.read_with(cx, |ws, _| ws.space.terminal(id).unwrap().term.cwd()) {
            return cwd.canonicalize().unwrap();
        }
        assert!(Instant::now() < deadline, "no cwd");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn a_saved_canvas_comes_back(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    // JSON strings: Windows backslashes doubled.
    let json = |p: &std::path::Path| p.display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":-100,"y":50,"zoom":1.1}},"focused":1,"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{}"}},
                  {{"kind":"terminal","rect":{{"x":800,"y":20,"w":600,"h":400}},"folder":"{}"}}]}}"#,
            json(a.path()),
            json(b.path())
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let (ids, rects, camera, focused) = ws.read_with(cx, |ws, _| {
        let blocks = ws.space.canvas.blocks();
        (
            blocks.iter().map(|b| b.id).collect::<Vec<_>>(),
            blocks.iter().map(|b| b.rect).collect::<Vec<_>>(),
            ws.space.canvas.camera,
            ws.space.focused,
        )
    });
    assert_eq!(
        rects,
        [
            Rect::new(0.0, 0.0, 700.0, 450.0),
            Rect::new(800.0, 20.0, 600.0, 400.0)
        ]
    );
    assert_eq!(camera.origin, Point::new(-100.0, 50.0));
    assert_eq!(camera.zoom(), 1.1);
    assert_eq!(focused, Some(ids[1]));
    assert_eq!(folder_of(&ws, ids[0], cx), a.path().canonicalize().unwrap());
    assert_eq!(folder_of(&ws, ids[1], cx), b.path().canonicalize().unwrap());
}

#[gpui_kit::test]
fn a_saved_canvas_with_no_blocks_opens_empty(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    write_canvas(
        data.path(),
        r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[]}"#,
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.canvas.blocks().is_empty());
        assert_eq!(ws.space.focused, None);
    });
}

#[gpui_kit::test]
fn a_folder_thats_gone_starts_at_home_and_says_so(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let gone = data
        .path()
        .join("gone")
        .display()
        .to_string()
        .replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{gone}"}}]}}"#
        ),
    );
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    assert_eq!(notification_count(handle, cx), 1);
    let id = first_id(&ws, cx);
    let home = std::env::home_dir().unwrap().canonicalize().unwrap();
    assert_eq!(folder_of(&ws, id, cx), home);
}

#[gpui_kit::test]
fn a_broken_canvas_file_is_set_aside(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    write_canvas(data.path(), "{nope");
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    assert_eq!(notification_count(handle, cx), 1);
    let bak =
        crate::session::path(data.path(), crate::session::DEFAULT_ID).with_extension("json.bak");
    assert_eq!(std::fs::read_to_string(bak).unwrap(), "{nope");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 1, "as on a first run")
    });
}

#[gpui_kit::test]
fn absurd_numbers_in_the_saved_canvas_are_made_safe(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    write_canvas(
        data.path(),
        r#"{"version":1,"camera":{"x":1e40,"y":0,"zoom":1e40},"focused":7,"items":[
              {"kind":"terminal","rect":{"x":1e40,"y":0,"w":700,"h":450}},
              {"kind":"terminal","rect":{"x":10,"y":10,"w":-5,"h":1e30}}]}"#,
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.read_with(cx, |ws, _| {
        let blocks = ws.space.canvas.blocks();
        assert_eq!(blocks.len(), 1, "the infinite one is skipped");
        let min = chrome::min_block_size(ws.cell.size);
        let max = chrome::block_size(MAX_COLS, MAX_ROWS, ws.cell.size);
        assert_eq!(blocks[0].rect, Rect::new(10.0, 10.0, min.w, max.h));
        assert_eq!(ws.space.focused, None, "index 7 doesn't exist");
        assert_eq!(
            ws.space.canvas.camera,
            Camera::default(),
            "an infinite camera isn't used"
        );
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_unreadable_canvas_file_is_never_written_over(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let data = tempfile::tempdir().unwrap();
    write_canvas(
        data.path(),
        r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[]}"#,
    );
    let path = crate::session::path(data.path(), crate::session::DEFAULT_ID);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    assert_eq!(notification_count(handle, cx), 1);
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.frozen, "its canvas won't be saved");
        assert_eq!(ws.space.canvas.blocks().len(), 1, "as on a first run");
    });
    // The rest still saves: the boards, a new workspace.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.boards.add_board();
            ws.new_space(window, cx);
        })
    })
    .unwrap();
    tick(cx);
    assert!(crate::board::path(data.path()).exists(), "boards saved");
    assert!(crate::session::path(data.path(), "w2").exists(), "w2 saved");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[]}"#,
        "never written over"
    );
}

/// One snapshot period, with the background read-and-write finished.
fn tick(cx: &mut TestAppContext) {
    cx.executor().advance_clock(crate::session::EVERY);
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_canvas_comes_back_as_it_was_left(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open(Rect::new(900.0, 40.0, 600.0, 400.0), window, cx);
            let first = ws.space.canvas.blocks()[0].id;
            ws.space.canvas.get_mut(first).unwrap().rect = Rect::new(-50.0, 60.0, 700.0, 450.0);
            ws.focus_block(first, window, cx);
            ws.space.canvas.camera = Camera::at(Point::new(-40.0, 25.0), 1.25);
            ws.save_canvas_now(cx);
        })
    })
    .unwrap();
    let before = ws.read_with(cx, |ws, _| {
        let blocks = ws.space.canvas.blocks();
        let focused = blocks.iter().position(|b| Some(b.id) == ws.space.focused);
        (blocks.iter().map(|b| b.rect).collect::<Vec<_>>(), focused)
    });
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space.name, "Main",
            "no index: the canvas is the first workspace"
        );
        let blocks = ws.space.canvas.blocks();
        assert_eq!(blocks.iter().map(|b| b.rect).collect::<Vec<_>>(), before.0);
        assert_eq!(
            blocks.iter().position(|b| Some(b.id) == ws.space.focused),
            before.1
        );
        assert_eq!(ws.space.canvas.camera.origin, Point::new(-40.0, 25.0));
        assert_eq!(ws.space.canvas.camera.zoom(), 1.25);
    });
}

#[cfg(target_os = "macos")]
const SECOND: &str = "cmd-2";
#[cfg(not(target_os = "macos"))]
const SECOND: &str = "ctrl-2";

#[gpui_kit::test]
fn a_new_workspace_is_empty_and_the_first_keeps_running(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.new_space(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!((ws.active, ws.others.len()), (1, 1));
        assert_eq!(ws.space.name, "Workspace 2");
        assert!(ws.space.canvas.blocks().is_empty(), "a new one is empty");
        assert_eq!(ws.others[0].name, "Main");
    });
    cx.update_window(handle, |_, window, cx| {
        window.find("workspace-name");
        window.click("workspace-default", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.active, 0);
        assert_eq!(ws.space.focused, Some(first));
        assert!(
            !ws.space.terminal(first).unwrap().term.exited(),
            "still running"
        );
    });
}

#[gpui_kit::test]
fn workspace_keys_cycle_and_jump(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.new_space(window, cx);
        });
        window.render_frame(cx);
        assert_eq!(ws.read(cx).active, 2);
        window.press("ctrl-tab", cx);
        assert_eq!(ws.read(cx).active, 0, "wraps around");
        window.press("ctrl-shift-tab", cx);
        assert_eq!(ws.read(cx).active, 2);
        window.press(SECOND, cx);
        assert_eq!(ws.read(cx).active, 1);
    })
    .unwrap();
}

#[gpui_kit::test]
fn hidden_workspaces_get_theme_changes(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.new_space(window, cx));
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.set_dark(true, cx));
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.others[0].terminal(first).unwrap().term.palette(),
            ws.theme.palette
        );
    });
}

#[gpui_kit::test]
fn renaming_and_deleting_workspaces(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(ws.rename_space("default", "  ", window, cx).is_err());
            assert_eq!(ws.space.name, "Main", "an empty name is refused");
            ws.rename_space("default", " Work ", window, cx).unwrap();
            assert_eq!(ws.space.name, "Work");
            ws.delete_space("default", window, cx);
            assert_eq!(ws.others.len(), 0, "the last one stays");
            ws.new_space(window, cx);
            ws.open(Rect::new(0.0, 0.0, 600.0, 400.0), window, cx);
            ws.save_canvas_now(cx);
            ws.delete_space("w2", window, cx);
            assert_eq!((ws.active, ws.others.len()), (0, 0));
            assert_eq!(ws.space.name, "Work", "the one before it is shown");
        });
        window.render_frame(cx);
        let name = window.find("workspace-name");
        assert!(name.bounds().size.width > px(0.0));
    })
    .unwrap();
    let w2 = crate::session::path(data.path(), "w2");
    assert!(!w2.exists() && w2.with_extension("json.bak").exists());
    let index = crate::session::load_index(data.path());
    let crate::session::Loaded::Saved(index) = index else {
        panic!("no index: {index:?}");
    };
    assert_eq!(index.workspaces.len(), 1);
    assert_eq!(index.workspaces[0].name, "Work");
}

#[gpui_kit::test]
fn every_workspace_comes_back_in_order(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.open(Rect::new(10.0, 20.0, 600.0, 400.0), window, cx);
            ws.open(Rect::new(700.0, 20.0, 600.0, 400.0), window, cx);
            ws.rename_space("w2", "Side project", window, cx).unwrap();
            ws.new_space(window, cx);
            ws.switch_to(1, window, cx);
            ws.toggle_sidebar(window, cx);
            ws.save_canvas_now(cx);
        })
    })
    .unwrap();
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        let names: Vec<&str> = ws.order().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Main", "Side project", "Workspace 3"]);
        assert_eq!(ws.active, 1);
        assert!(!ws.sidebar);
        assert_eq!(ws.space.canvas.blocks().len(), 2);
        assert_eq!(ws.others[0].canvas.blocks().len(), 1, "Main's terminal");
        assert!(ws.others[1].canvas.blocks().is_empty());
    });
}

/// The kanban block on top.
#[cfg(target_os = "macos")]
const NEW_NOTE: &str = "cmd-shift-m";
#[cfg(not(target_os = "macos"))]
const NEW_NOTE: &str = "ctrl-shift-m";

fn top_note(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> BlockId {
    ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        assert!(
            matches!(ws.space.blocks.get(&id), Some(super::Body::Note(_))),
            "the top block is a note"
        );
        id
    })
}

/// Opens a note with the shortcut and returns it.
fn new_note(handle: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut TestAppContext) -> BlockId {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW_NOTE, cx);
        ws.update(cx, |ws, _| ws.space.appearing.clear());
        window.render_frame(cx);
    })
    .unwrap();
    top_note(ws, cx)
}

#[gpui_kit::test]
fn new_note_adds_a_focused_note_whose_heading_titles_it(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    let (handle, ws) = open(cx);
    let id = new_note(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        let ws_now = ws.read(cx);
        assert_eq!(ws_now.space.focused, Some(id));
        let rect = ws_now.space.canvas.get(id).unwrap().rect;
        assert_eq!((rect.w, rect.h), (crate::note::SIZE.w, crate::note::SIZE.h));
        let super::Body::Note(note) = &ws_now.space.blocks[&id] else {
            unreachable!()
        };
        assert!(
            note.editor.read(cx).focus_handle(cx).is_focused(window),
            "the editor has the keyboard"
        );
    })
    .unwrap();
    // Typed text reaches the editor.
    cx.simulate_input(handle, "hi");
    ws.read_with(cx, |ws, cx| {
        let super::Body::Note(note) = &ws.space.blocks[&id] else {
            unreachable!()
        };
        assert_eq!(note.text(cx), "hi");
        assert_eq!(note.title(cx).as_ref(), "Note");
    });
    // A heading becomes the title.
    cx.update_window(handle, |_, window, cx| {
        let super::Body::Note(note) = &ws.read(cx).space.blocks[&id] else {
            unreachable!()
        };
        let editor = note.editor.clone();
        editor.update(cx, |e, cx| e.set_value("# Hello\nhi", window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, cx| {
        let super::Body::Note(note) = &ws.space.blocks[&id] else {
            unreachable!()
        };
        assert_eq!(note.title(cx).as_ref(), "Hello");
    });
}

#[gpui_kit::test]
fn shortcuts_run_from_inside_a_note(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    new_note(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| window.press(NEW, cx))
        .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.blocks.len(), 3, "terminal, note, terminal");
        assert_eq!(ws.space.terminals().count(), 2);
    });
}

#[gpui_kit::test]
fn clicking_a_note_focuses_its_editor(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    let (handle, ws) = open(cx);
    let id = new_note(handle, &ws, cx);
    let terminal = ws.read_with(cx, |ws, _| ws.space.terminals().next().unwrap().0);
    cx.update_window(handle, |_, window, cx| {
        // The terminal gets the keyboard, but stays under the note (focus_block would raise
        // it, and it covers the note).
        ws.update(cx, |ws, cx| {
            ws.space.focused = Some(terminal);
            window.focus(&ws.focus_handle, cx);
        });
        window.render_frame(cx);
        let body = window.find(("note-editor", id.0 as usize)).bounds();
        click(
            window,
            cx,
            (
                f32::from(body.origin.x) + 20.0,
                f32::from(body.origin.y) + 20.0,
            ),
            1,
        );
        window.render_frame(cx);
        let ws_now = ws.read(cx);
        assert_eq!(ws_now.space.focused, Some(id));
        let super::Body::Note(note) = &ws_now.space.blocks[&id] else {
            unreachable!()
        };
        assert!(
            note.editor.read(cx).focus_handle(cx).is_focused(window),
            "the editor keeps the keyboard"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_preview_button_shows_the_rendered_note(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_note(handle, &ws, cx);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.find(("note-editor", n));
        window.hover(("block", n), cx);
        window.render_frame(cx);
        window.click(("preview", n), cx);
        window.render_frame(cx);
        window.find(("note-preview", n));
        assert!(window.try_find(("note-editor", n)).is_none());
        let super::Body::Note(note) = &ws.read(cx).space.blocks[&id] else {
            unreachable!()
        };
        assert!(note.preview);
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_a_focused_note_gives_the_keyboard_back_to_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    new_note(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press(CLOSE, cx);
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(
            ws.space.blocks.values().all(|b| b.note().is_none()),
            "closed"
        );
        assert_eq!(ws.space.terminals().count(), 2, "the shortcut still works");
    });
}

#[gpui_kit::test]
fn previewing_a_focused_note_gives_the_keyboard_back_to_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = new_note(handle, &ws, cx);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.hover(("block", n), cx);
        window.render_frame(cx);
        window.click(("preview", n), cx);
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminals().count(), 2, "the shortcut still works");
    });
}

#[gpui_kit::test]
fn a_note_comes_back_with_its_text_and_mode(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(dir.path().to_path_buf()));
    let id = new_note(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let super::Body::Note(note) = &ws.space.blocks[&id] else {
                unreachable!()
            };
            note.editor
                .update(cx, |e, cx| e.set_value("# Keep me\n- x", window, cx));
            ws.toggle_preview(id, window, cx);
            ws.save_canvas_now(cx);
        });
    })
    .unwrap();
    // As the_canvas_comes_back_as_it_was_left does: a second workspace on the same folder.
    let (_, again) = open_in(cx, None, Some(dir.path().to_path_buf()));
    again.read_with(cx, |ws, cx| {
        let notes: Vec<_> = ws
            .space
            .blocks
            .values()
            .filter_map(|b| match b {
                super::Body::Note(n) => Some((n.text(cx), n.preview)),
                _ => None,
            })
            .collect();
        assert_eq!(notes, [("# Keep me\n- x".to_string(), true)]);
    });
}

#[cfg(target_os = "macos")]
const NEW_EDITOR: &str = "cmd-shift-e";
#[cfg(not(target_os = "macos"))]
const NEW_EDITOR: &str = "ctrl-shift-e";

/// An editor block on `folder`, near the canvas's top-left, read and at rest.
fn open_editor(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    folder: &std::path::Path,
) -> BlockId {
    cx.update_window(handle, |_, window, cx| {
        let id = ws.update(cx, |ws, cx| {
            let id = ws
                .add_editor_at(folder, Point::new(40.0, 40.0), window, cx)
                .unwrap();
            ws.space.appearing.clear();
            id
        });
        window.render_frame(cx);
        id
    })
    .unwrap()
}

#[gpui_kit::test]
fn an_editor_lists_its_folder_and_reads_a_subfolder_when_expanded(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.txt"), "b").unwrap();
    std::fs::write(dir.path().join("A.txt"), "a").unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    let src = dir.path().join("src");
    ws.read_with(cx, |ws, _| {
        let e = ws.space.editor(id).unwrap();
        assert_eq!(
            e.title().as_ref(),
            dir.path().file_name().unwrap().to_str().unwrap()
        );
        let names: Vec<_> = e.dirs[dir.path()].iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["src", "A.txt", "b.txt"],
            "folders first, then files without case"
        );
        assert!(
            !e.dirs.contains_key(&src),
            "a folder isn't read until it is expanded"
        );
        let rect = ws.space.canvas.get(id).unwrap().rect;
        assert_eq!(
            (rect.w, rect.h),
            (crate::editor::SIZE.w, crate::editor::SIZE.h)
        );
        assert_eq!(ws.space.recent, [dir.path()]);
    });
    cx.update_window(handle, |_, window, cx| {
        window.find(("tree", id.0 as usize));
        window.click(("entry", 0usize), cx);
        window.render_frame(cx);
        // main.rs is now the row under src.
        window.find(("entry", 1usize));
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let e = ws.space.editor(id).unwrap();
        assert_eq!(e.dirs[&src][0].name, "main.rs");
        assert!(e.expanded.contains(&src));
    });
    // The header's toggle hides the tree.
    cx.update_window(handle, |_, window, cx| {
        window.hover(("block", id.0 as usize), cx);
        window.render_frame(cx);
        window.click(("tree-toggle", id.0 as usize), cx);
        window.render_frame(cx);
        assert!(window.try_find(("tree", id.0 as usize)).is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_editor_shortcut_asks_for_a_folder_and_a_recent_one_opens_it(cx: &mut TestAppContext) {
    // The dialog slides in in real time: a click on it lands once it is at rest.
    no_motion(cx);
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx);
    ws.update(cx, |ws, _| ws.space.recent = vec![dir.path().to_path_buf()]);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW_EDITOR, cx);
        window.render_frame(cx);
        window.click("recent-0", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let editors: Vec<_> = ws
            .space
            .blocks
            .values()
            .filter_map(|b| b.editor())
            .collect();
        assert_eq!(editors.len(), 1);
        assert_eq!(editors[0].folder, dir.path());
        assert_eq!(ws.space.focused, Some(ws.space.canvas.top().unwrap()));
    });
}

#[gpui_kit::test]
fn an_editor_comes_back_and_one_whose_folder_is_gone_says_so(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let json = |p: &std::path::Path| p.display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"editor","rect":{{"x":0,"y":0,"w":900,"h":600}},"folder":"{}","tree":false}},
                  {{"kind":"editor","rect":{{"x":950,"y":0,"w":100,"h":100}},"folder":"{}"}}]}}"#,
            json(folder.path()),
            json(&folder.path().join("nope"))
        ),
    );
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let (there, gone) = ws.read_with(cx, |ws, _| {
        let ids: Vec<_> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
        (ids[0], ids[1])
    });
    ws.read_with(cx, |ws, _| {
        let e = ws.space.editor(there).unwrap();
        assert_eq!(e.folder, folder.path());
        assert!(!e.show_tree);
        let rect = ws.space.canvas.get(gone).unwrap().rect;
        assert_eq!(
            (rect.w, rect.h),
            (crate::editor::MIN.w, crate::editor::MIN.h),
            "clamped"
        );
    });
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, _| ws.space.appearing.clear());
        window.render_frame(cx);
        assert!(
            window.try_find(("tree", there.0 as usize)).is_none(),
            "tree hidden"
        );
        window.find(("choose-folder", gone.0 as usize));
        // Pointing it at a folder that exists reads it.
        ws.update(cx, |ws, cx| {
            ws.set_editor_folder(gone, folder.path(), cx);
        });
        window.render_frame(cx);
        window.find(("tree", gone.0 as usize));
    })
    .unwrap();
}

const CLOSE_TAB: &str = "ctrl-w";

/// Clicks the tree's row `ix` (a file opens, a folder expands).
fn click_file(handle: AnyWindowHandle, ix: usize, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        window.click(("entry", ix), cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn clicking_a_file_opens_it_in_a_tab_with_its_text_and_language(cx: &mut TestAppContext) {
    use gpui_kit::ElementId;
    use gpui_kit::Focusable as _;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.rs"), "fn main() {}\r\n").unwrap();
    std::fs::write(dir.path().join("notes.md"), "# Hi\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    let n = id.0 as usize;
    click_file(handle, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        let ws_now = ws.read(cx);
        let e = ws_now.space.editor(id).unwrap();
        assert_eq!(e.tabs.len(), 1);
        let tab = e.active_tab().unwrap();
        assert_eq!(
            tab.editor.read(cx).value().as_ref(),
            "fn main() {}\n",
            "CRLF normalized"
        );
        assert_eq!(tab.editor.read(cx).language_name().as_ref(), "rust");
        assert!(matches!(
            tab.content,
            crate::editor::Content::Text {
                crlf: true,
                newline: true,
                read_only: None
            }
        ));
        assert!(
            tab.editor.read(cx).focus_handle(cx).is_focused(window),
            "the file has the keyboard"
        );
        assert!(e.title().starts_with("main.rs — "), "{}", e.title());
        window.find(("code", n));
        window.find(ElementId::Name(format!("tab-{n}-0").into()));
    })
    .unwrap();
    // A second file: a second tab, active; the Markdown preview toggle shows for it.
    click_file(handle, 1, cx);
    cx.update_window(handle, |_, window, cx| {
        let e = ws.read(cx).space.editor(id).unwrap();
        assert_eq!((e.tabs.len(), e.active), (2, Some(1)));
        window.click(("md-preview", n), cx);
        window.render_frame(cx);
        window.find(("file-preview", n));
        assert!(window.try_find(("code", n)).is_none());
        // Clicking the first file's row again shows its tab, no new one.
        window.click(("entry", 0usize), cx);
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert_eq!((e.tabs.len(), e.active), (2, Some(0)));
    })
    .unwrap();
}

#[gpui_kit::test]
fn clicking_inside_an_open_file_keeps_its_editor_focused(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "text").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| window.focus(&ws.focus_handle, cx));
        window.render_frame(cx);
        let body = window.find(("code", id.0 as usize)).bounds();
        click(
            window,
            cx,
            (
                f32::from(body.origin.x) + f32::from(body.size.width) / 2.0,
                f32::from(body.origin.y) + f32::from(body.size.height) / 2.0,
            ),
            1,
        );
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert!(
            e.active_tab()
                .unwrap()
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            "the block's mouse-down gave the keyboard back to the file"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_binary_file_and_a_big_one_are_not_edited(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.bin"), b"\x7fELF\0\x01").unwrap();
    let lines = crate::editor::READ_ONLY_OVER as usize / 2 + 1;
    std::fs::write(dir.path().join("big.txt"), "x\n".repeat(lines)).unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    let n = id.0 as usize;
    click_file(handle, 0, cx);
    cx.update_window(handle, |_, window, _| {
        window.find(("binary", n));
        window.find(("reveal", n));
        assert!(window.try_find(("code", n)).is_none());
    })
    .unwrap();
    click_file(handle, 1, cx);
    cx.update_window(handle, |_, window, cx| {
        window.find(("code", n));
        let e = ws.read(cx).space.editor(id).unwrap();
        assert!(!e.active_tab().unwrap().editor.read(cx).is_editable());
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_w_closes_the_tab_and_the_x_closes_another(cx: &mut TestAppContext) {
    use gpui_kit::ElementId;
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.path().join(name), name).unwrap();
    }
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    let n = id.0 as usize;
    click_file(handle, 0, cx);
    click_file(handle, 1, cx);
    click_file(handle, 2, cx);
    cx.update_window(handle, |_, window, cx| {
        // c.txt is active: Ctrl+W closes it, b.txt becomes active.
        window.press(CLOSE_TAB, cx);
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert_eq!((e.tabs.len(), e.active), (2, Some(1)));
        assert_eq!(e.active_tab().unwrap().name(), "b.txt");
        // The ✕ of a.txt: b.txt stays active, now at 0.
        window.click(ElementId::Name(format!("tab-close-{n}-0").into()), cx);
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert_eq!((e.tabs.len(), e.active), (1, Some(0)));
        assert_eq!(e.active_tab().unwrap().name(), "b.txt");
        window.press(CLOSE_TAB, cx);
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert_eq!((e.tabs.len(), e.active), (0, None));
        // With nothing open, a new terminal shortcut still works: the keyboard is back.
        window.press(NEW, cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.terminals().count(), 2));
}

#[gpui_kit::test]
fn a_saved_editor_comes_back_with_its_files_and_skips_paths_outside_the_folder(
    cx: &mut TestAppContext,
) {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("proj");
    std::fs::create_dir_all(folder.join("src")).unwrap();
    std::fs::write(folder.join("src/a.rs"), "a").unwrap();
    std::fs::write(folder.join("b.txt"), "b").unwrap();
    std::fs::write(root.path().join("outside.txt"), "no").unwrap();
    let json = |p: &std::path::Path| p.display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"editor","rect":{{"x":0,"y":0,"w":900,"h":600}},"folder":"{}",
                    "open":["src/a.rs","../outside.txt","{}","gone.txt","b.txt"],"active":4}}]}}"#,
            json(&folder),
            json(&folder.join("b.txt"))
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let id = first_id(&ws, cx);
    ws.read_with(cx, |ws, _| {
        let e = ws.space.editor(id).unwrap();
        let names: Vec<_> = e.tabs.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            ["a.rs", "b.txt"],
            "outside, absolute and gone: skipped"
        );
        assert_eq!(
            e.active,
            Some(1),
            "the saved index is beyond the kept tabs: the last one"
        );
    });
    // What is saved: relative paths, the active one's position among them.
    let item = ws.read_with(cx, |ws, _| {
        ws.space
            .editor(id)
            .unwrap()
            .item(ws.space.canvas.get(id).unwrap().rect)
    });
    assert_eq!(item.open, ["src/a.rs", "b.txt"]);
    assert_eq!(item.active, Some(1));
}

#[cfg(target_os = "macos")]
const SAVE: &str = "cmd-s";
#[cfg(not(target_os = "macos"))]
const SAVE: &str = "ctrl-s";

#[gpui_kit::test]
fn typing_marks_the_tab_changed_and_ctrl_s_writes_the_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "one\r\ntwo\r\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    cx.simulate_input(handle, "x");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert!(e.active_tab().unwrap().dirty);
        assert!(e.title().starts_with("● a.txt"), "{}", e.title());
        window.press(SAVE, cx);
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert!(!e.active_tab().unwrap().dirty, "saved");
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "xone\r\ntwo\r\n",
        "the file keeps its CRLF and its final newline"
    );
}

/// Dialogs slide in over 250 ms of real time: a click mid-slide can miss its button.
fn no_motion(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

#[gpui_kit::test]
fn closing_with_unsaved_changes_asks_first(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    std::fs::write(dir.path().join("b.txt"), "b").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    cx.simulate_input(handle, "x");
    click_file(handle, 1, cx);
    cx.simulate_input(handle, "y");
    // Ctrl+W on a changed tab: the question; Cancel keeps it.
    cx.update_window(handle, |_, window, cx| {
        window.press(CLOSE_TAB, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        assert_eq!(ws.read(cx).space.editor(id).unwrap().tabs.len(), 2);
    })
    .unwrap();
    // Save writes b.txt and closes the tab.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        window.press(CLOSE_TAB, cx);
        window.render_frame(cx);
        window.within("dialog").click("save", cx);
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.editor(id).unwrap().tabs.len(), 1);
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "yb"
    );
    // Closing the block with a.txt changed: the question; Don't save closes anyway.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.close(id, window, cx));
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        assert!(ws.read(cx).space.editor(id).is_some(), "still there");
        window.within("dialog").click("discard", cx);
        window.render_frame(cx);
        assert!(ws.read(cx).space.editor(id).is_none());
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "a",
        "not written"
    );
}

#[gpui_kit::test]
fn a_failed_save_says_so_and_keeps_the_tab_changed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "a").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    cx.simulate_input(handle, "x");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();
    let before = notification_count(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press(SAVE, cx);
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert!(e.active_tab().unwrap().dirty);
    })
    .unwrap();
    assert_eq!(notification_count(handle, cx), before + 1);
}

/// Rewrites `path` as another program would, a modification time clearly later.
fn change_on_disk(path: &std::path::Path, text: &str) {
    use std::time::{Duration, SystemTime};
    std::fs::write(path, text).unwrap();
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(5))
        .unwrap();
}

#[gpui_kit::test]
fn a_file_changed_on_disk_is_reloaded_while_clean_and_flagged_while_edited(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::input::Position;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    let n = id.0 as usize;
    click_file(handle, 0, cx);
    let value = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, cx| {
            let tab = ws.space.editor(id).unwrap().active_tab().unwrap();
            tab.editor.read(cx).value().to_string()
        })
    };
    // The cursor on line 2 (0-based 1); a clean tab is reloaded in place, the line kept.
    cx.update_window(handle, |_, window, cx| {
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        let editor = tab.editor.clone();
        editor.update(cx, |e, cx| {
            e.set_cursor_position(
                Position {
                    line: 1,
                    character: 0,
                },
                window,
                cx,
            )
        });
    })
    .unwrap();
    change_on_disk(&path, "uno\ndos\ntres\n");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.check_files(window, cx));
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        let tab = e.active_tab().unwrap();
        assert_eq!(tab.editor.read(cx).value().as_ref(), "uno\ndos\ntres\n");
        assert_eq!(tab.editor.read(cx).cursor_position().line, 1);
        assert!(!tab.dirty && !tab.conflict);
    })
    .unwrap();
    // Edited, then changed on disk: flagged, the bar shows; Keep mine clears the flag.
    cx.simulate_input(handle, "x");
    assert_eq!(
        value(&ws, cx),
        "uno\nxdos\ntres\n",
        "typed on the kept line"
    );
    change_on_disk(&path, "other\n");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.check_files(window, cx));
        window.render_frame(cx);
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        assert!(tab.conflict && tab.dirty);
        window.find(("conflict", n));
        window.click(("keep", n), cx);
        window.render_frame(cx);
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        assert!(!tab.conflict && tab.dirty);
        assert!(window.try_find(("conflict", n)).is_none());
    })
    .unwrap();
    assert_eq!(value(&ws, cx), "uno\nxdos\ntres\n", "kept");
    // Changed again: Reload takes the disk's text.
    change_on_disk(&path, "newest\n");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.check_files(window, cx));
        window.render_frame(cx);
        window.click(("reload", n), cx);
        window.render_frame(cx);
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        assert!(!tab.conflict && !tab.dirty);
    })
    .unwrap();
    assert_eq!(value(&ws, cx), "newest\n");
    // Templum's own save is not a change on disk: nothing is reloaded or flagged.
    cx.simulate_input(handle, "y");
    let typed = value(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press(SAVE, cx);
        ws.update(cx, |ws, cx| ws.check_files(window, cx));
        window.render_frame(cx);
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        assert!(
            !tab.conflict && !tab.dirty,
            "a save is not a change on disk"
        );
    })
    .unwrap();
    assert_eq!(value(&ws, cx), typed);
    // Gone: the tab stays, marked changed, and Save brings the file back.
    std::fs::remove_file(&path).unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.check_files(window, cx));
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        assert!(tab.dirty && tab.mtime.is_none());
        window.press(SAVE, cx);
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        crate::editor::encode(&typed, false, true)
    );
}

#[gpui_kit::test]
fn the_focused_editors_tree_is_read_again_after_two_seconds(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    std::fs::write(dir.path().join("b.txt"), "b").unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.space.focused = Some(id);
            ws.check_files(window, cx);
        });
        let count =
            |cx: &mut gpui_kit::App| ws.read(cx).space.editor(id).unwrap().dirs[dir.path()].len();
        assert_eq!(count(cx), 1, "too soon");
        ws.update(cx, |ws, cx| {
            let e = ws.space.editor_mut(id).unwrap();
            e.tree_read = std::time::Instant::now()
                .checked_sub(crate::editor::TREE_REFRESH)
                .unwrap();
            ws.check_files(window, cx);
        });
        assert_eq!(count(cx), 2);
        window.render_frame(cx);
        window.find(("entry", 1usize));
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_tree_creates_renames_and_deletes_entries(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/a.rs"), "a").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    // The actions, as the menu items call them.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(
                !ws.new_entry(id, dir.path(), false, "", window, cx),
                "empty: refused"
            );
            assert!(
                !ws.new_entry(id, dir.path(), false, "a/b", window, cx),
                "separator: refused"
            );
            assert!(ws.new_entry(id, &dir.path().join("src"), false, "new.rs", window, cx));
        });
        window.render_frame(cx);
        let e = ws.read(cx).space.editor(id).unwrap();
        assert!(dir.path().join("src/new.rs").is_file());
        assert_eq!(e.active_tab().unwrap().name(), "new.rs", "a new file opens");
        assert!(
            e.expanded.contains(&dir.path().join("src")),
            "its folder is expanded"
        );
        ws.update(cx, |ws, cx| {
            assert!(ws.rename_entry(id, &dir.path().join("src"), "lib", window, cx));
        });
        let e = ws.read(cx).space.editor(id).unwrap();
        assert!(dir.path().join("lib/new.rs").is_file());
        assert_eq!(
            e.active_tab().unwrap().path,
            dir.path().join("lib/new.rs"),
            "the tab followed"
        );
        assert!(e.expanded.contains(&dir.path().join("lib")));
        ws.update(cx, |ws, cx| {
            ws.delete_entry(id, &dir.path().join("lib"), window, cx)
        });
        let e = ws.read(cx).space.editor(id).unwrap();
        assert!(!dir.path().join("lib").exists());
        assert!(e.tabs.is_empty(), "tabs on deleted files close");
        assert_eq!(e.dirs[dir.path()].len(), 0);
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_files_menu_asks_before_deleting_it(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("z.txt"), "z").unwrap();
    let (handle, ws) = open(cx);
    open_editor(handle, &ws, cx, dir.path());
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("entry", 0usize), cx);
        window.render_frame(cx);
        assert!(
            ws.read(cx).menu.is_some(),
            "the row's menu, not the block's"
        );
        // Open, Reveal in file manager, Rename…, Delete….
        window.within("popup-menu").click(3usize, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "Delete… asks");
    })
    .unwrap();
    assert!(dir.path().join("z.txt").is_file(), "not yet");
    cx.update_window(handle, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(!dir.path().join("z.txt").exists(), "confirmed: deleted");
}

#[gpui_kit::test]
fn a_terminal_opened_from_the_tree_sits_beside_the_editor(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("entry", 0usize), cx);
        window.render_frame(cx);
        // New file…, New folder…, Open a terminal here, …
        window.within("popup-menu").click(2usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let (terminal, editor_rect, rect) = ws.read_with(cx, |ws, _| {
        let top = ws.space.canvas.top().unwrap();
        assert!(ws.space.terminal(top).is_some(), "a terminal opened");
        let editor_rect = ws.space.canvas.get(id).unwrap().rect;
        (top, editor_rect, ws.space.canvas.get(top).unwrap().rect)
    });
    assert_eq!(
        rect.x,
        editor_rect.x + editor_rect.w + templum_canvas::BESIDE
    );
    assert_eq!(rect.y, editor_rect.y);
    #[cfg(unix)]
    assert_eq!(
        folder_of(&ws, terminal, cx),
        dir.path().join("src").canonicalize().unwrap()
    );
    #[cfg(not(unix))]
    let _ = terminal;
}

#[cfg(target_os = "macos")]
const GO_TO_LINE: &str = "cmd-g";
#[cfg(not(target_os = "macos"))]
const GO_TO_LINE: &str = "ctrl-g";

#[gpui_kit::test]
fn open_in_editor_reuses_the_block_and_opens_the_file_at_a_line(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    std::fs::write(dir.path().join("b.txt"), "1\n2\n3\n").unwrap();
    let (handle, ws) = open(cx);
    // No editor yet: one is made on the folder.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_in_editor(
                dir.path(),
                &dir.path().join("a.txt"),
                None,
                None,
                window,
                cx,
            )
        });
        window.render_frame(cx);
    })
    .unwrap();
    let id = ws.read_with(cx, |ws, _| {
        let editors: Vec<_> = ws
            .space
            .blocks
            .iter()
            .filter(|(_, b)| b.editor().is_some())
            .collect();
        assert_eq!(editors.len(), 1);
        *editors[0].0
    });
    // Far away, then asked for b.txt at line 2: the same block, the camera flies to it.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.space.canvas.camera.origin = Point::new(50_000.0, 50_000.0);
            ws.open_in_editor(
                dir.path(),
                &dir.path().join("b.txt"),
                Some(2),
                None,
                window,
                cx,
            );
        });
        window.render_frame(cx);
        let ws_now = ws.read(cx);
        let e = ws_now.space.editor(id).unwrap();
        assert_eq!(e.tabs.len(), 2);
        assert_eq!(e.active_tab().unwrap().name(), "b.txt");
        assert_eq!(
            e.active_tab()
                .unwrap()
                .editor
                .read(cx)
                .cursor_position()
                .line,
            1
        );
        assert_eq!(ws_now.space.focused, Some(id));
        assert!(ws_now.space.flight.is_some(), "flying to the block");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.space
                .blocks
                .values()
                .filter(|b| b.editor().is_some())
                .count(),
            1,
            "no second editor"
        );
    });
}

#[gpui_kit::test]
fn ctrl_g_asks_for_a_line_and_goes_there(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "1\n2\n3\n4\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press(GO_TO_LINE, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
    })
    .unwrap();
    cx.simulate_input(handle, "3");
    cx.update_window(handle, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        assert_eq!(tab.editor.read(cx).cursor_position().line, 2);
    })
    .unwrap();
}

// Linux only: macOS's zsh takes Ctrl+G as send-break and shows nothing, while bash
// in cooked mode echoes it as ^G.
#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn ctrl_g_reaches_the_terminal_without_an_editor(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let shown = echo_after(GO_TO_LINE, handle, &ws, cx);
    assert!(shown.contains("^G"), "{shown:?}");
}

/// An editor on `dir` with `name` open and typed into, so it has unsaved edits.
fn dirty_tab(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    dir: &std::path::Path,
) -> BlockId {
    let id = open_editor(handle, ws, cx, dir);
    click_file(handle, 0, cx);
    cx.simulate_input(handle, "x");
    id
}

#[gpui_kit::test]
fn closing_a_block_doesnt_ask_about_another_workspaces_files(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let (handle, ws) = open(cx);
    let id = dirty_tab(handle, &ws, cx, dir.path());
    // Another workspace, whose blocks count their ids from 0 again.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            while ws.space.terminal(id).is_none() {
                ws.new_terminal(window, cx);
            }
            ws.close(id, window, cx);
        });
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "not this workspace's file");
        assert!(ws.read(cx).space.terminal(id).is_none(), "closed");
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_the_window_with_unsaved_files_asks_first(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let (handle, ws) = open(cx);
    let mut window_cx = gpui_kit::VisualTestContext::from_window(handle, cx);
    assert!(window_cx.simulate_close(), "nothing unsaved: it closes");
    dirty_tab(handle, &ws, cx, dir.path());
    assert!(!window_cx.simulate_close(), "unsaved: it stays open");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "and asks");
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_file_that_isnt_utf8_opens_read_only_and_keeps_its_bytes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("latin1.txt");
    std::fs::write(&path, b"caf\xe9\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    cx.simulate_input(handle, "x");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        assert!(!tab.editor.read(cx).is_editable(), "read-only");
        assert!(!tab.dirty);
        window.press(SAVE, cx);
    })
    .unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"caf\xe9\n",
        "its bytes are kept"
    );
}

#[gpui_kit::test]
fn renaming_onto_an_existing_file_is_refused(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    std::fs::write(dir.path().join("b.txt"), "b").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(!ws.rename_entry(id, &dir.path().join("a.txt"), "b.txt", window, cx));
            // The same name in another case is the same file only on some disks: renaming to
            // the same name is always allowed.
            assert!(ws.rename_entry(id, &dir.path().join("a.txt"), "a.txt", window, cx));
        });
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "b"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "a"
    );
}

#[gpui_kit::test]
fn deleting_a_workspace_with_unsaved_files_asks_and_can_save_them(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let data = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    dirty_tab(handle, &ws, cx, dir.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.open_menu(
                super::menu::Target::Space("default".into()),
                point(px(100.0), px(100.0)),
                window,
                cx,
            );
        });
        window.render_frame(cx);
        // Rename…, Delete….
        window.within("popup-menu").click(1usize, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        window.within("dialog").click("save", cx);
        window.render_frame(cx);
        assert_eq!(ws.read(cx).others.len(), 0, "deleted");
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "xa",
        "saved first"
    );
}

#[gpui_kit::test]
fn a_file_that_grows_past_4_mb_on_disk_turns_read_only(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log.txt");
    std::fs::write(&path, "start\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    let lines = crate::editor::READ_ONLY_OVER as usize / 2 + 1;
    change_on_disk(&path, &"x\n".repeat(lines));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.check_files(window, cx));
        window.render_frame(cx);
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        assert!(!tab.editor.read(cx).is_editable(), "over 4 MB: read-only");
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_makefile_indents_with_a_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Makefile"), "all:\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    cx.update_window(handle, |_, window, cx| {
        window.press("tab", cx);
        window.render_frame(cx);
        let tab = ws.read(cx).space.editor(id).unwrap().active_tab().unwrap();
        let text = tab.editor.read(cx).value().to_string();
        assert!(text.starts_with('\t'), "{text:?}");
    })
    .unwrap();
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_fifo_in_the_tree_isnt_opened(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("pipe");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    let before = notification_count(handle, cx);
    // Reading it would wait for a writer forever, on the UI thread.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_file(id, &fifo, None, window, cx));
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.editor(id).unwrap().tabs.is_empty())
    });
    assert_eq!(notification_count(handle, cx), before + 1, "it says why");
}

#[gpui_kit::test]
fn choosing_another_folder_keeps_unsaved_tabs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let (handle, ws) = open(cx);
    let id = dirty_tab(handle, &ws, cx, dir.path());
    cx.update_window(handle, |_, _, cx| {
        ws.update(cx, |ws, cx| {
            ws.set_editor_folder(id, other.path(), cx);
        });
        let e = ws.read(cx).space.editor(id).unwrap();
        assert_eq!(e.tabs.len(), 1, "the changed file stays open");
        assert!(e.tabs[0].dirty);
        assert_eq!(e.active, Some(0));
    })
    .unwrap();
}

fn top_kanban(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> BlockId {
    ws.read_with(cx, |ws, _| {
        let id = ws.space.canvas.top().unwrap();
        assert!(ws.space.kanban(id).is_some(), "the top block is a kanban");
        id
    })
}

#[gpui_kit::test]
fn the_toolbar_adds_blocks_and_the_title_bar_lost_its_add_buttons(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.find("toolbar");
        assert!(
            window.try_find("new-terminal").is_none(),
            "no + in the title bar"
        );
        assert!(window.try_find("new-kanban").is_none());
        window.click("tool-terminal", cx);
        window.render_frame(cx);
        window.click("tool-note", cx);
        window.render_frame(cx);
        window.click("tool-kanban", cx);
        window.render_frame(cx);
        window.find("tool-editor");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminals().count(), 2);
        let kinds = |f: fn(&super::Body) -> bool| ws.space.blocks.values().filter(|b| f(b)).count();
        assert_eq!(kinds(|b| b.note().is_some()), 1);
        assert_eq!(kinds(|b| b.kanban().is_some()), 1);
        assert_eq!(ws.tool, super::Tool::Select, "block tools don't stay on");
    });
}

#[gpui_kit::test]
fn the_hand_tool_pans_over_blocks_until_escape(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let before = ws.read_with(cx, |ws, _| {
        (
            ws.space.canvas.camera.origin,
            ws.space.canvas.get(id).unwrap().rect,
        )
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-hand", cx);
        window.render_frame(cx);
        window.find("hand");
        let from = window.find(("title", id.0 as usize)).bounds().center();
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            (f32::from(from.x), f32::from(from.y)),
            (f32::from(from.x) + 40.0, f32::from(from.y) + 10.0),
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tool, super::Tool::Hand);
        assert_eq!(ws.space.canvas.get(id).unwrap().rect, before.1, "not moved");
        assert_ne!(ws.space.canvas.camera.origin, before.0, "panned");
    });
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("hand").is_none(), "the layer is gone");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.tool, super::Tool::Select));
}

#[gpui_kit::test]
fn the_toolbar_agent_button_lists_the_agents(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-agent", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.menu.is_some(), "the agents menu is open")
    });
}

#[test]
fn editor_only_commands_need_a_focused_editor() {
    use crate::keys::Command;
    assert_eq!(super::effective(Some(Command::CloseTab), false), None);
    assert_eq!(
        super::effective(Some(Command::CloseTab), true),
        Some(Command::CloseTab)
    );
    assert_eq!(
        super::effective(Some(Command::NewTerminal), false),
        Some(Command::NewTerminal)
    );
}

#[test]
fn undo_keys_only_count_while_drawing_or_with_a_selection() {
    let key = |s: &str| gpui_kit::Keystroke::parse(s).unwrap();
    assert_eq!(super::undo_key(&key("ctrl-z"), false), Some(false));
    assert_eq!(super::undo_key(&key("ctrl-shift-z"), false), Some(true));
    assert_eq!(super::undo_key(&key("cmd-z"), true), Some(false));
    assert_eq!(super::undo_key(&key("cmd-shift-z"), true), Some(true));
    assert_eq!(super::undo_key(&key("ctrl-z"), true), None, "Cmd on macOS");
    assert_eq!(super::undo_key(&key("ctrl-alt-z"), false), None);
    assert_eq!(super::undo_key(&key("z"), false), None);
    // The workspace asks only while a tool draws or something is selected: see
    // ctrl_z_reaches_the_terminal_with_nothing_to_undo.
}

#[gpui_kit::test]
fn clicking_a_stroke_selects_it_and_delete_removes_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        click(
            window,
            cx,
            (f32::from(o.x) + 70.0, f32::from(o.y) + 22.0),
            1,
        );
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.selected_ink, Some(0));
        assert_eq!(
            ws.read(cx).space.focused,
            None,
            "Delete and Backspace go to the drawing, so no terminal keeps the keyboard"
        );
        assert_eq!(ws.read(cx).space.ink_undo.len(), 1, "a click isn't a step");
        // Dragging it moves it.
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            (f32::from(o.x) + 70.0, f32::from(o.y) + 22.0),
            (f32::from(o.x) + 70.0, f32::from(o.y) + 122.0),
        );
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.ink[0].bounds().y.round(), 118.0);
        assert_eq!(ws.read(cx).space.selected_ink, Some(0), "still selected");
        assert_eq!(ws.read(cx).space.ink_undo.len(), 2, "the move is");
        window.press("escape", cx);
        assert_eq!(ws.read(cx).space.selected_ink, None, "Esc deselects");
        click(
            window,
            cx,
            (f32::from(o.x) + 70.0, f32::from(o.y) + 122.0),
            1,
        );
        assert_eq!(ws.read(cx).space.selected_ink, Some(0));
        window.press("delete", cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.ink.is_empty());
        assert_eq!(ws.space.selected_ink, None);
    });
    // A click on empty canvas with nothing there selects nothing and pans as before.
    cx.update_window(handle, |_, window, cx| {
        let o = ws.read(cx).origin;
        click(
            window,
            cx,
            (f32::from(o.x) + 70.0, f32::from(o.y) + 22.0),
            1,
        );
        assert_eq!(ws.read(cx).space.selected_ink, None);
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_z_undoes_and_redoes_drawing_changes(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    draw(handle, &ws, cx, (20.0, 60.0), (120.0, 60.0));
    cx.update_window(handle, |_, window, cx| {
        window.press(UNDO, cx);
        assert_eq!(ws.read(cx).space.ink.len(), 1);
        window.press(UNDO, cx);
        assert!(ws.read(cx).space.ink.is_empty());
        window.press(REDO, cx);
        assert_eq!(ws.read(cx).space.ink.len(), 1);
        // The eraser over nothing adds no step: undo goes back to the empty canvas.
        window.click("tool-eraser", cx);
        window.render_frame(cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (300.0, 5.0), (300.0, 40.0));
    cx.update_window(handle, |_, window, cx| {
        window.press(UNDO, cx);
        assert!(
            ws.read(cx).space.ink.is_empty(),
            "the eraser drag wasn't a step"
        );
    })
    .unwrap();
}

// Linux: `cat` is a session leader, so its group is orphaned and the terminal's Ctrl+Z
// doesn't stop it; the PTY echoes the key as ^Z.
#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn ctrl_z_reaches_the_terminal_with_nothing_to_undo(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // Nothing drawn, Select on: the key goes to the terminal.
    let shown = echo_after(UNDO, handle, &ws, cx);
    assert!(shown.contains("^Z"), "{shown:?}");
}

#[gpui_kit::test]
fn a_stroke_over_a_terminal_doesnt_steal_its_clicks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let body = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("tool-pen", cx);
            window.render_frame(cx);
            window.find(("body", id.0 as usize)).bounds()
        })
        .unwrap();
    let (x, y) = (
        f32::from(body.origin.x) + 30.0,
        f32::from(body.origin.y) + 30.0,
    );
    cx.update_window(handle, |_, window, cx| {
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            (x, y),
            (x + 100.0, y),
        );
        window.render_frame(cx);
        window.press("escape", cx);
        window.render_frame(cx);
        ws.update(cx, |ws, _| ws.space.focused = None);
        // With Select on, a click on the stroke over the terminal goes to the terminal.
        click(window, cx, (x + 50.0, y), 1);
        window.render_frame(cx);
        let ws_now = ws.read(cx);
        assert_eq!(ws_now.space.ink.len(), 1);
        assert_eq!(ws_now.space.selected_ink, None);
        assert_eq!(ws_now.space.focused, Some(id));
    })
    .unwrap();
}

#[gpui_kit::test]
fn undo_keeps_the_last_50_drawing_changes(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.update(cx, |ws, _| {
        for _ in 0..super::INK_UNDO + 10 {
            ws.remember_ink();
        }
        assert_eq!(ws.space.ink_undo.len(), 50);
    });
}

#[gpui_kit::test]
fn right_clicking_a_drawing_opens_its_menu(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        right_click(window, cx, (f32::from(o.x) + 70.0, f32::from(o.y) + 22.0));
        window.render_frame(cx);
        assert!(ws.read(cx).menu.is_some());
        ws.update(cx, |ws, cx| ws.delete_ink(0, cx));
        assert!(ws.read(cx).space.ink.is_empty());
    })
    .unwrap();
}

/// Drags on the empty canvas near its top-left corner, clear of the startup terminal.
fn draw(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    from: (f32, f32),
    to: (f32, f32),
) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        let at = |p: (f32, f32)| (f32::from(o.x) + p.0, f32::from(o.y) + p.1);
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers::default(),
            at(from),
            at(to),
        );
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_pen_draws_a_stroke_that_comes_back_after_a_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
        window.click("ink-color-red", cx);
        window.click("ink-width-thick", cx);
        window.render_frame(cx);
        window.find("ink-layer");
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (80.0, 50.0));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tool, super::Tool::Pen, "the pen stays on");
        let [
            templum_canvas::Drawing::Stroke {
                points,
                width,
                color,
                highlight,
            },
        ] = ws.space.ink.as_slice()
        else {
            panic!("one stroke: {:?}", ws.space.ink);
        };
        assert_eq!(
            points.last(),
            Some(&ws.space.canvas.camera.to_world(Point::new(80.0, 50.0))),
            "the pen catches up with the pointer on release"
        );
        assert_eq!(
            (*width, *color, *highlight),
            (8.0, templum_canvas::Color::Red, false)
        );
        assert!(
            ws.space.focused.is_some(),
            "drawing doesn't unfocus the terminal"
        );
    });
    // A press without a move is nothing.
    draw(handle, &ws, cx, (200.0, 20.0), (200.0, 20.0));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.ink.len(), 1));
    cx.update_window(handle, |_, _, cx| {
        ws.update(cx, |ws, cx| ws.save_canvas_now(cx))
    })
    .unwrap();
    let (_, again) = open_in(cx, None, Some(dir.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        assert_eq!(ws.space.ink.len(), 1);
        assert_eq!(ws.space.ink[0].width(), 8.0);
    });
}

#[gpui_kit::test]
fn the_highlighter_is_wide_and_translucent(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-highlighter", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("ink-width-thin").is_none(),
            "one width only"
        );
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    ws.read_with(cx, |ws, _| {
        let [
            templum_canvas::Drawing::Stroke {
                width, highlight, ..
            },
        ] = ws.space.ink.as_slice()
        else {
            panic!("one stroke");
        };
        assert_eq!((*width, *highlight), (crate::ink::HIGHLIGHTER_WIDTH, true));
    });
}

#[gpui_kit::test]
fn the_shapes_pill_picks_an_arrow_and_shift_squares_a_rectangle(cx: &mut TestAppContext) {
    use templum_canvas::{Drawing, ShapeKind};
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-shapes", cx);
        window.render_frame(cx);
        window.find("ink-shapes");
        window.click("shape-arrow", cx);
        window.render_frame(cx);
        assert!(window.try_find("ink-shapes").is_none(), "closed on a pick");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tool, super::Tool::Shape(ShapeKind::Arrow))
    });
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 60.0));
    ws.read_with(cx, |ws, _| {
        let [Drawing::Shape { kind, from, to, .. }] = ws.space.ink.as_slice() else {
            panic!("one shape: {:?}", ws.space.ink);
        };
        assert_eq!(*kind, ShapeKind::Arrow);
        assert_eq!((to.x - from.x, to.y - from.y), (100.0, 40.0));
    });
    // A rectangle with Shift held is a square.
    cx.update_window(handle, |_, window, cx| {
        window.click("tool-shapes", cx);
        window.render_frame(cx);
        window.click("shape-rect", cx);
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        let at = |p: (f32, f32)| (f32::from(o.x) + p.0, f32::from(o.y) + p.1);
        press_with(
            window,
            cx,
            MouseButton::Left,
            Modifiers {
                shift: true,
                ..Default::default()
            },
            at((200.0, 20.0)),
            at((300.0, 60.0)),
        );
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let Some(Drawing::Shape { kind, from, to, .. }) = ws.space.ink.last() else {
            panic!("a shape");
        };
        assert_eq!(*kind, ShapeKind::Rect);
        assert_eq!((to.x - from.x, to.y - from.y), (100.0, 100.0), "squared");
        assert_eq!(ws.last_shape, Some(ShapeKind::Rect));
    });
    // With Select on, Esc closes the pill.
    cx.update_window(handle, |_, window, cx| {
        window.click("tool-select", cx);
        window.click("tool-shapes", cx);
        window.render_frame(cx);
        window.find("ink-shapes");
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("ink-shapes").is_none(), "Esc closes it");
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_eraser_removes_the_stroke_it_passes_over(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-pen", cx);
    })
    .unwrap();
    draw(handle, &ws, cx, (20.0, 20.0), (120.0, 20.0));
    draw(handle, &ws, cx, (20.0, 200.0), (120.0, 200.0));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.ink.len(), 2));
    cx.update_window(handle, |_, window, cx| window.click("tool-eraser", cx))
        .unwrap();
    // Across the first stroke, well away from the second.
    draw(handle, &ws, cx, (70.0, 5.0), (70.0, 40.0));
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.ink.len(), 1);
        assert_eq!(ws.space.ink[0].bounds().y.round(), 198.0);
    });
    // An eraser drag over nothing changes nothing (and, in Task 5, adds no undo step).
    draw(handle, &ws, cx, (300.0, 5.0), (300.0, 40.0));
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.ink.len(), 1));
}

#[gpui_kit::test]
fn the_board_button_opens_a_block_with_the_board_list(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-kanban", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let block = top_kanban(&ws, cx);
    let n = block.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find(("board-list", n)).is_some());
        window.click(("new-board", n), cx);
        window.render_frame(cx);
        assert!(
            window.try_find(("columns", n)).is_some(),
            "the new board shows"
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let board = ws.boards.boards[0].id;
        assert_eq!(ws.space.kanban(block).unwrap().board, Some(board));
        assert_eq!(ws.boards.board(board).unwrap().columns.len(), 3);
    });
}

#[gpui_kit::test]
fn two_blocks_on_one_board_show_the_same_cards(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (a, b, board, todo) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let a = ws.space.canvas.top().unwrap();
                ws.new_board(a, cx);
                ws.new_kanban(window, cx);
                let b = ws.space.canvas.top().unwrap();
                let board = ws.space.kanban(a).unwrap().board.unwrap();
                ws.show_board(b, board, cx);
                let todo = ws.boards.board(board).unwrap().columns[0].id;
                ws.space.appearing.clear();
                (a, b, board, todo)
            })
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Through the block on top: it covers most of the other.
        let add = gpui_kit::ElementId::Name(format!("add-card-{}-{todo}", b.0).into());
        window.click(add, cx);
        window.render_frame(cx);
        let card = ws.read(cx).boards.board(board).unwrap().columns[0].cards[0].id;
        for block in [a, b] {
            let id = gpui_kit::ElementId::Name(format!("card-{}-{card}", block.0).into());
            assert!(window.try_find(id).is_some(), "block {block:?} shows it");
        }
    })
    .unwrap();
    ws.update(cx, |ws, cx| ws.delete_board(board, cx));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for block in [a, b] {
            let list = ("board-list", block.0 as usize);
            assert!(window.try_find(list).is_some(), "back to the list");
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn enter_applies_a_typed_name(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        let target = ws.clone();
        ws.update(cx, |_, cx| {
            crate::prompt::ask_text(
                "Rename workspace",
                "Rename",
                "Main",
                move |text, window, cx| {
                    target
                        .update(cx, |ws, cx| ws.rename_space("default", &text, window, cx))
                        .is_ok()
                },
                window,
                cx,
            )
        });
        window.render_frame(cx);
        window.input("Work", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(ws.read(cx).space.name, "Work");
    })
    .unwrap();
}

#[gpui_kit::test]
fn an_open_card_toggles_labels_and_can_be_deleted(cx: &mut TestAppContext) {
    use crate::board::Label;
    let (handle, ws) = open(cx);
    let (block, board, card) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                let board = ws.space.kanban(block).unwrap().board.unwrap();
                let todo = ws.boards.board(board).unwrap().columns[0].id;
                let card = ws.boards.add_card(board, todo).unwrap();
                ws.space.appearing.clear();
                (block, board, card)
            })
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let id = gpui_kit::ElementId::Name(format!("card-{}-{card}", block.0).into());
        assert!(window.try_find(id).is_some());
        ws.update(cx, |ws, cx| {
            let current = ws.boards.card(board, card).cloned().unwrap();
            crate::kanban::open_card(board, current, false, window, cx)
        });
        window.render_frame(cx);
        // The dialog slides in for 0.25 s first: presses meanwhile are lost.
        std::thread::sleep(std::time::Duration::from_millis(300));
        window.render_frame(cx);
        // The title has the keyboard when the card opens.
        window.input("!", cx);
    })
    .unwrap();
    // Subscribers hear the typing once that update ends.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let title = ws.read(cx).boards.card(board, card).unwrap().title.clone();
        assert!(title.contains('!'), "typing applies at once: {title:?}");
        window.click("label-green", cx);
        window.render_frame(cx);
        let labels = ws.read(cx).boards.card(board, card).unwrap().labels.clone();
        assert_eq!(labels, [Label::Green]);
        window.click("delete-card", cx);
        window.render_frame(cx);
        assert!(ws.read(cx).boards.card(board, card).is_none());
        assert!(
            window.try_find("delete-card").is_none(),
            "the dialog closed"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_wheel_over_a_kanban_block_stays_in_it_and_ctrl_zooms(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (block, doing) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                ws.space.appearing.clear();
                let board = ws.space.kanban(block).unwrap().board.unwrap();
                (block, ws.boards.board(board).unwrap().columns[1].id)
            })
        })
        .unwrap();
    let column = gpui_kit::ElementId::Name(format!("column-{}-{doing}", block.0).into());
    // As a pointer does: over the column first, then the wheel.
    let wheel_over = |window: &mut Window, cx: &mut gpui_kit::App, delta, control| {
        window.hover(column.clone(), cx);
        let at = window.find(column.clone()).bounds().center();
        wheel(
            window,
            cx,
            (f32::from(at.x), f32::from(at.y)),
            delta,
            control,
        );
        window.render_frame(cx);
    };
    let before = ws.read_with(cx, |ws, _| ws.space.canvas.camera);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let x = window.find(column.clone()).bounds().origin.x;
        wheel_over(
            window,
            cx,
            ScrollDelta::Pixels(point(px(0.), px(-200.))),
            false,
        );
        assert_eq!(
            window.find(column.clone()).bounds().origin.x,
            x,
            "a vertical wheel doesn't slide the columns"
        );
        wheel_over(
            window,
            cx,
            ScrollDelta::Pixels(point(px(-200.), px(0.))),
            false,
        );
        assert!(
            window.find(column.clone()).bounds().origin.x < x,
            "a sideways one does"
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera, before, "no pan")
    });
    cx.update_window(handle, |_, window, cx| {
        wheel_over(window, cx, ScrollDelta::Lines(point(0.0, 1.0)), true);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.zoom(), 1.1, "Ctrl zooms")
    });
}

#[gpui_kit::test]
fn holding_alt_moves_the_canvas_over_blocks(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    let before = ws.read_with(cx, |ws, _| ws.space.canvas.camera.origin);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("hand").is_none(), "not without Alt");
        hold_alt(window, cx, true);
        assert!(
            window.try_find("hand").is_some(),
            "the hand covers the canvas"
        );
        // A drag over the terminal: the canvas moves, nothing in the terminal is selected.
        let body = window.find(("body", id.0 as usize)).bounds().center();
        let from = (f32::from(body.x), f32::from(body.y));
        press_with(
            window,
            cx,
            MouseButton::Left,
            alt,
            from,
            (from.0 + 100.0, from.1 + 40.0),
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let moved = ws.space.canvas.camera.origin;
        assert_eq!((moved.x, moved.y), (before.x - 100.0, before.y - 40.0));
        let term = ws.space.terminal(id).unwrap().term.shared();
        assert!(term.selection_text().is_none(), "no selection");
        assert!(ws.drag.is_none(), "the drag ended");
    });
    // The wheel over the terminal zooms the canvas, like the hand tool, and leaves the
    // terminal's history alone.
    cx.update_window(handle, |_, window, cx| {
        let body = window.find(("body", id.0 as usize)).bounds().center();
        window.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: body,
                delta: ScrollDelta::Pixels(point(px(0.), px(-60.))),
                modifiers: alt,
                touch_phase: TouchPhase::Moved,
            }),
            cx,
        );
        hold_alt(window, cx, false);
        assert!(window.try_find("hand").is_none(), "gone when Alt is let go");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 0.9));
}

#[gpui_kit::test]
fn alt_let_go_elsewhere_is_noticed_at_the_next_move(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        hold_alt(window, cx, true);
    })
    .unwrap();
    let redraw = Rc::new(Cell::new(false));
    let seen = redraw.clone();
    cx.update(|cx| cx.observe(&ws, move |_, _| seen.set(true)).detach());
    // Let go while another window had the keyboard: only the next move says so, and the
    // workspace redraws without its hand.
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: point(px(300.), px(300.)),
                pressed_button: None,
                modifiers: Modifiers::default(),
            }),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    assert!(redraw.get(), "the move asks for a frame");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("hand").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_middle_drag_moves_the_canvas_anywhere(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let before = ws.read_with(cx, |ws, _| ws.space.canvas.camera.origin);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let body = window.find(("body", id.0 as usize)).bounds().center();
        let from = (f32::from(body.x), f32::from(body.y));
        press_with(
            window,
            cx,
            MouseButton::Middle,
            Modifiers::default(),
            from,
            (from.0 - 50.0, from.1),
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.origin.x, before.x + 50.0);
        assert!(ws.drag.is_none());
    });
}

#[gpui_kit::test]
fn the_canvas_menu_adds_blocks_where_it_was_opened(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    // Left of the startup terminal, then right of it: empty canvas both times.
    for (item, at) in [(0usize, (20.0, 600.0)), (1, (1100.0, 20.0))] {
        let world = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let o = ws.read(cx).origin;
                let at = (f32::from(o.x) + at.0, f32::from(o.y) + at.1);
                right_click(window, cx, at);
                window.render_frame(cx);
                assert!(ws.read(cx).menu.is_some(), "the menu opened");
                let ws = ws.read(cx);
                ws.space
                    .canvas
                    .camera
                    .to_world(ws.local(point(px(at.0), px(at.1))))
            })
            .unwrap();
        cx.update_window(handle, |_, window, cx| {
            // AI agent ▸, Add ▸ Terminal or Kanban board.
            window.within("popup-menu").click(1usize, cx);
            window.render_frame(cx);
            window
                .within("submenu")
                .within("popup-menu")
                .click(item, cx);
            window.render_frame(cx);
        })
        .unwrap();
        ws.read_with(cx, |ws, _| {
            let top = ws.space.canvas.top().unwrap();
            assert_eq!(ws.space.canvas.get(top).unwrap().rect.origin(), world);
            assert_eq!(ws.space.kanban(top).is_some(), item == 1);
            assert!(ws.menu.is_none(), "a choice closes it");
        });
    }
}

#[gpui_kit::test]
fn the_canvas_menu_opens_an_agent_in_a_recent_folder(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let folder = tempfile::tempdir().unwrap();
    ws.update(cx, |ws, _| {
        ws.space.recent = vec![folder.path().to_path_buf()];
        // One agent, installed everywhere: the menu's first, whatever this computer has.
        ws.agents = vec![crate::agents::AgentEntry {
            id: "mine".into(),
            title: "Mine".into(),
            command: quiet().0,
            resume: None,
            install: None,
            builtin: None,
            changed: false,
            show: true,
        }];
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        right_click(window, cx, (f32::from(o.x) + 20.0, f32::from(o.y) + 600.0));
        window.render_frame(cx);
        // AI agent ▸ its first agent.
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
        window
            .within("submenu")
            .within("popup-menu")
            .click(0usize, cx);
        window.render_frame(cx);
        assert!(
            window.try_find("choose-folder").is_some(),
            "the folder dialog"
        );
    })
    .unwrap();
    // The dialog slides in for 0.25 s first: presses meanwhile are lost.
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("recent-0", cx);
        window.render_frame(cx);
        assert!(window.try_find("choose-folder").is_none(), "it closed");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminals().count(), 2, "the agent's terminal");
        assert_eq!(ws.space.recent, [folder.path()]);
    });
}

#[gpui_kit::test]
fn the_innermost_menu_opens_and_acts_on_what_was_clicked(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let terminal = first_id(&ws, cx);
    let (block, board, card) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.add_kanban_at(Point::new(900.0, 0.0), window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                let board = ws.space.kanban(block).unwrap().board.unwrap();
                let todo = ws.boards.board(board).unwrap().columns[0].id;
                let card = ws.boards.add_card(board, todo).unwrap();
                ws.space.appearing.clear();
                (block, board, card)
            })
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let card_el = gpui_kit::ElementId::Name(format!("card-{}-{card}", block.0).into());
        window.right_click(card_el, cx);
        window.render_frame(cx);
        // Open, —, Delete card: the card's menu, not its block's (Board list, —, Close).
        window.within("popup-menu").click(2usize, cx);
        window.render_frame(cx);
        let ws = ws.read(cx);
        assert!(ws.boards.card(board, card).is_none(), "the card went");
        assert!(ws.space.kanban(block).is_some(), "its block stayed");
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("body", terminal.0 as usize), cx);
        window.render_frame(cx);
        // A second right-click replaces the menu.
        window.right_click(("body", terminal.0 as usize), cx);
        window.render_frame(cx);
        // Copy, Paste, —, Close terminal.
        window.within("popup-menu").click(3usize, cx);
        window.render_frame(cx);
        assert!(
            !ws.read(cx).space.terminal(terminal).is_some(),
            "the terminal closed"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn kanban_blocks_and_boards_come_back(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let board = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.new_kanban(window, cx);
                let block = ws.space.canvas.top().unwrap();
                ws.new_board(block, cx);
                let board = ws.space.kanban(block).unwrap().board.unwrap();
                let todo = ws.boards.board(board).unwrap().columns[0].id;
                ws.boards.add_card(board, todo);
                ws.new_kanban(window, cx);
                ws.save_canvas_now(cx);
                board
            })
        })
        .unwrap();
    let before = ws.read_with(cx, |ws, _| ws.boards.clone());
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        assert_eq!(ws.boards, before);
        let mut shown: Vec<Option<u64>> = ws
            .space
            .canvas
            .blocks()
            .iter()
            .filter_map(|b| ws.space.kanban(b.id).map(|k| k.board))
            .collect();
        shown.sort();
        assert_eq!(shown, [None, Some(board)]);
        assert_eq!(ws.space.terminals().count(), 1, "and the terminal");
    });
}

#[gpui_kit::test]
fn snapshots_write_only_what_changed(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = crate::session::path(data.path(), crate::session::DEFAULT_ID);
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    tick(cx);
    assert!(path.exists(), "the first snapshot is written");
    std::fs::remove_file(&path).unwrap();
    tick(cx);
    assert!(!path.exists(), "nothing changed: nothing written");
    ws.update(cx, |ws, _| {
        ws.space.canvas.camera.pan_by(Point::new(30.0, 0.0))
    });
    tick(cx);
    assert!(path.exists(), "a pan is a change");
}

#[gpui_kit::test]
fn a_failing_canvas_write_says_so_once_and_recovers(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    // A file where the workspaces folder should be: every write fails.
    let blocker = data.path().join("workspaces");
    std::fs::write(&blocker, "").unwrap();
    let pan = |cx: &mut TestAppContext| {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera.pan_by(Point::new(10.0, 0.0))
        })
    };
    pan(cx);
    tick(cx);
    assert_eq!(notification_count(handle, cx), 1, "said once");
    // Still failing: retried every period without saying so again (the first hides after 5 s).
    for _ in 0..3 {
        pan(cx);
        tick(cx);
    }
    assert_eq!(notification_count(handle, cx), 0, "not again");
    std::fs::remove_file(&blocker).unwrap();
    tick(cx);
    assert!(
        crate::session::path(data.path(), crate::session::DEFAULT_ID).exists(),
        "retried"
    );
    assert_eq!(notification_count(handle, cx), 1, "and says it works again");
}

#[gpui_kit::test]
fn closing_the_window_saves_the_canvas(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.update(cx, |ws, _| {
        let first = ws.space.canvas.blocks()[0].id;
        ws.space.canvas.get_mut(first).unwrap().rect = Rect::new(-500.0, 0.0, 700.0, 450.0);
    });
    // The test's handle would keep the workspace alive.
    drop(ws);
    cx.update_window(handle, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    let text = std::fs::read_to_string(crate::session::path(
        data.path(),
        crate::session::DEFAULT_ID,
    ))
    .unwrap();
    assert!(text.contains("-500.0"), "{text}");
}

#[gpui_kit::test]
fn without_a_data_folder_nothing_is_kept(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    tick(cx);
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.saved, None));
}

#[gpui_kit::test]
fn shells_that_cant_start_leave_the_saved_canvas_alone(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let text = r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[
              {"kind":"terminal","rect":{"x":0,"y":0,"w":700,"h":450}}]}"#;
    write_canvas(data.path(), text);
    // A shell setting with a typo: no block can start.
    let broken = Some(("/nonexistent/templum-shell".to_string(), Vec::new()));
    let (_, ws) = open_as(cx, broken, None, Some(data.path().to_path_buf()));
    tick(cx);
    ws.read_with(cx, |ws, _| assert!(ws.space.canvas.blocks().is_empty()));
    assert_eq!(
        std::fs::read_to_string(crate::session::path(
            data.path(),
            crate::session::DEFAULT_ID
        ))
        .unwrap(),
        text,
        "kept for the start after the setting is fixed"
    );
}

#[gpui_kit::test]
fn a_saved_agent_is_resumed_in_its_block(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let folder = folder.path().display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{folder}","agent":"claude"}}]}}"#
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let id = first_id(&ws, cx);
    // The test shell echoes what it is typed.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("claude --continue") {
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn recent_folders_are_newest_first_without_repeats_and_few() {
    use std::path::PathBuf;
    let mut recent = Vec::new();
    for i in 0..12 {
        super::remember(&mut recent, PathBuf::from(format!("/p{i}")));
    }
    super::remember(&mut recent, PathBuf::from("/p8"));
    assert_eq!(recent.len(), super::RECENT);
    assert_eq!(recent[0], PathBuf::from("/p8"));
    assert_eq!(recent[1], PathBuf::from("/p11"));
    assert_eq!(
        recent
            .iter()
            .filter(|p| *p == std::path::Path::new("/p8"))
            .count(),
        1
    );
}

#[gpui_kit::test]
fn an_agent_opens_in_its_folder_and_the_folder_becomes_recent(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let (handle, ws) = open(cx);
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.launch_agent("codex", b.path(), Point::new(0.0, 900.0), window, cx);
                ws.launch_agent("claude", a.path(), Point::new(40.0, 0.0), window, cx)
            })
        })
        .unwrap()
        .expect("a block");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.recent, [a.path(), b.path()]);
        assert_eq!(
            ws.space.canvas.get(id).unwrap().rect.origin(),
            Point::new(40.0, 0.0)
        );
        assert_eq!(ws.space.focused, Some(id));
    });
    assert_eq!(folder_of(&ws, id, cx), a.path().canonicalize().unwrap());
    // The test shell echoes what it is typed.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("claude") {
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn agents_are_changed_reset_added_and_deleted_from_the_settings(cx: &mut TestAppContext) {
    use crate::agents::Draft;
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    let frame = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap()
    };
    let mine = ws.update(cx, |ws, cx| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").unwrap();
        let mut draft = Draft::of(claude);
        draft.command = "claude --model opus".into();
        ws.save_agent(Some("claude"), &draft, cx).unwrap();
        let new = Draft {
            name: "My Agent".into(),
            command: "my-agent --tui".into(),
            resume: String::new(),
            show: true,
        };
        assert_eq!(
            ws.save_agent(
                None,
                &Draft {
                    command: " ".into(),
                    ..new.clone()
                },
                cx
            ),
            Err("An agent needs a command")
        );
        ws.save_agent(None, &new, cx).unwrap()
    });
    assert_eq!(mine, "my-agent");
    frame(cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        text.contains("claude --model opus") && text.contains("my-agent --tui"),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").unwrap();
        assert_eq!(
            (claude.command.as_str(), claude.changed),
            ("claude --model opus", true)
        );
        assert_eq!(ws.agents.last().unwrap().title, "My Agent");
    });
    ws.update(cx, |ws, cx| {
        ws.remove_agent("claude", cx);
        ws.remove_agent("my-agent", cx);
    });
    frame(cx);
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        !text.contains("claude --model") && !text.contains("my-agent"),
        "{text}"
    );
    ws.read_with(cx, |ws, _| {
        let claude = ws.agents.iter().find(|a| a.id == "claude").unwrap();
        assert_eq!((claude.command.as_str(), claude.changed), ("claude", false));
        assert!(ws.agents.iter().all(|a| a.id != "my-agent"));
    });
}

#[gpui_kit::test]
fn a_changed_resume_command_is_what_comes_back(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write_config(
        config.path(),
        "[agents.claude]\nresume = \"claude -c --fast\"\n",
    );
    let folder = tempfile::tempdir().unwrap();
    let folder = folder.path().display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{folder}","agent":"claude"}}]}}"#
        ),
    );
    let (_, ws) = open_in(
        cx,
        Some(config.path().to_path_buf()),
        Some(data.path().to_path_buf()),
    );
    let id = first_id(&ws, cx);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("claude -c --fast") {
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn an_agent_whose_folder_is_gone_isnt_resumed_elsewhere(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let data = tempfile::tempdir().unwrap();
    let here = tempfile::tempdir().unwrap();
    let json = |p: &std::path::Path| p.display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"terminal","rect":{{"x":0,"y":0,"w":700,"h":450}},"folder":"{}","agent":"claude"}},
                  {{"kind":"terminal","rect":{{"x":800,"y":0,"w":700,"h":450}},"folder":"{}","agent":"claude"}}]}}"#,
            json(&data.path().join("gone")),
            json(here.path())
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let ids: Vec<BlockId> = ws.read_with(cx, |ws, _| {
        ws.space.canvas.blocks().iter().map(|b| b.id).collect()
    });
    let shown = |id: BlockId, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        })
    };
    // Both would be typed at once: once the second shows it, the first would too.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !shown(ids[1], cx).contains("claude --continue") {
        assert!(Instant::now() < deadline, "the block in its folder resumes");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !shown(ids[0], cx).contains("claude"),
        "not in the home folder: that's another conversation"
    );
}

#[cfg(target_os = "macos")]
const NEW_GIT: &str = "cmd-shift-g";
#[cfg(not(target_os = "macos"))]
const NEW_GIT: &str = "ctrl-shift-g";

/// Runs `git args` in `dir`, panicking with git's stderr on failure; returns stdout.
fn git_run(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository on `main` in a temporary folder, with an author so commits work.
fn git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git_run(dir.path(), &["init", "-q"]);
    git_run(dir.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git_run(dir.path(), &["config", "user.name", "Ada"]);
    git_run(dir.path(), &["config", "user.email", "ada@example.com"]);
    git_run(dir.path(), &["config", "commit.gpgsign", "false"]);
    // Files as written: a runner's core.autocrlf would turn restored files into CRLF.
    git_run(dir.path(), &["config", "core.autocrlf", "false"]);
    dir
}

/// Lets the background git jobs finish, draws a frame, and lets what the frame started finish.
fn settle(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.run_until_parked();
    frame(handle, cx);
    cx.run_until_parked();
    frame(handle, cx);
}

/// A Git block on `folder`, near the canvas's top-left, looked up and read.
fn open_git(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    folder: &std::path::Path,
) -> BlockId {
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let id = ws
                    .add_git_at(folder, Point::new(40.0, 40.0), window, cx)
                    .unwrap();
                ws.space.appearing.clear();
                id
            })
        })
        .unwrap();
    settle(handle, cx);
    id
}

fn git_of<T>(
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    read: impl FnOnce(&crate::git_block::Git) -> T,
) -> T {
    ws.read_with(cx, |ws, _| read(ws.space.git(id).unwrap()))
}

#[gpui_kit::test]
fn a_git_block_on_a_fresh_repository_lists_an_untracked_file(cx: &mut TestAppContext) {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| {
        assert!(g.repo().is_some(), "found");
        assert_eq!(g.status.branch.as_deref(), Some("main"));
        assert_eq!(g.status.entries.len(), 1);
        assert_eq!(g.status.entries[0].path, "a.txt");
        assert_eq!(
            g.status.entries[0].unstaged,
            Some(templum_git::Change::Untracked)
        );
        assert_eq!(g.author.as_ref().map(|a| a.0.as_str()), Some("Ada"));
    });
    cx.update_window(handle, |_, window, _| {
        assert!(
            window
                .try_find(crate::git_block::row_id(id, "unstaged", 0))
                .is_some()
        );
        assert!(
            window
                .try_find(crate::git_block::row_id(id, "staged", 0))
                .is_none()
        );
        assert!(window.try_find(("branch", id.0 as usize)).is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_folder_outside_a_repository_offers_initialize(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, dir.path());
    git_of(&ws, cx, id, |g| {
        assert!(matches!(g.found, crate::git_block::Found::NotARepo))
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("init", id.0 as usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.repo().is_some(), "initialized");
        assert_eq!(g.status.oid, None, "no commits");
    });
    assert!(dir.path().join(".git").is_dir());
}

#[gpui_kit::test]
fn without_git_the_block_says_so_and_tries_again_when_asked(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    write_config(config.path(), "[git]\nprogram = \"templum-no-such-git\"\n");
    let repo = git_repo();
    let (handle, ws) = open_with(cx, Some(config.path().to_path_buf()));
    let id = open_git(handle, &ws, cx, repo.path());
    git_of(&ws, cx, id, |g| {
        assert!(matches!(&g.found, crate::git_block::Found::NoGit(e) if e.missing))
    });
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find(("retry", id.0 as usize)).is_some());
    })
    .unwrap();
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("git", "program", "git"),
            |c| c.git.program = "git".to_string(),
            cx,
        )
    });
    frame(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click(("retry", id.0 as usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert!(g.repo().is_some()));
}

#[gpui_kit::test]
fn the_status_is_read_again_every_two_seconds_while_on_screen(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, id, |g| g.reads);
    std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.reads, reads + 1);
        assert_eq!(g.status.entries[0].path, "b.txt");
    });
    // Off screen: not read.
    ws.update(cx, |ws, _| {
        ws.space.canvas.camera = Camera::at(Point::new(100_000.0, 100_000.0), 1.0);
        ws.space.view = ws.space.canvas.camera;
    });
    frame(handle, cx);
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.reads, reads + 1));
}

#[gpui_kit::test]
fn a_refresh_asked_while_one_runs_waits_and_runs_once(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, id, |g| g.reads);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            for _ in 0..3 {
                ws.refresh_git(id, window, cx);
            }
        })
    })
    .unwrap();
    git_of(&ws, cx, id, |g| assert!(g.busy && g.pending));
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.reads, reads + 2, "the first, then one for the two asked");
        assert!(!g.busy && !g.pending);
    });
}

#[gpui_kit::test]
fn a_result_for_a_closed_block_is_dropped(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(id, window, cx);
            ws.close(id, window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    ws.read_with(cx, |ws, _| assert!(ws.space.git(id).is_none()));
}

#[gpui_kit::test]
fn new_git_asks_for_a_folder_and_a_saved_block_comes_back_on_its_tab(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = git_repo();
    let data = tempfile::tempdir().unwrap();
    let json = repo.path().display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"git","rect":{{"x":0,"y":0,"w":800,"h":560}},"folder":"{json}","tab":"branches"}}]}}"#
        ),
    );
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    settle(handle, cx);
    let id = first_id(&ws, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.repo().is_some());
        assert_eq!(g.tab, crate::git_block::Tab::Branches);
    });
    let item = ws.read_with(cx, |ws, _| {
        ws.space
            .git(id)
            .unwrap()
            .item(Rect::new(0.0, 0.0, 1.0, 1.0))
    });
    assert_eq!(item.tab, "branches");
    cx.update_window(handle, |_, window, cx| {
        window.press(NEW_GIT, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "the folder dialog");
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_tools_section_shows_gits_version_and_sets_the_program(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.toggle_settings(window, cx));
    })
    .unwrap();
    settle(handle, cx);
    // The panel slides in in real time: clicks land once it is at rest.
    settle_slide(handle, cx);
    ws.read_with(cx, |ws, _| {
        let version = ws.git_version.clone().unwrap().unwrap();
        assert!(version.starts_with("git version "), "{version}");
    });
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-tools", cx);
        window.render_frame(cx);
        assert!(window.try_find("field-git.program").is_some());
        assert!(
            window.try_find("reset-git.program").is_none(),
            "the default"
        );
    })
    .unwrap();
    // What the field calls on each change, as the shell program's tests do.
    ws.update(cx, |ws, cx| {
        crate::settings::set_git_program(ws, "/opt/git", cx)
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        assert_eq!(ws.read(cx).config.git.program, "/opt/git");
        assert!(window.try_find("reset-git.program").is_some());
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        text.contains("[git]") && text.contains("program = \"/opt/git\""),
        "{text}"
    );
}

/// Shell integration's `finished` count: a command run in the repository from a terminal of
/// this workspace refreshes the block.
#[cfg(unix)]
#[gpui_kit::test]
fn a_terminal_finishing_a_command_in_the_repository_refreshes_the_block(cx: &mut TestAppContext) {
    let ps0 = std::process::Command::new("bash")
        .args([
            "-c",
            "(( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 404 ))",
        ])
        .status()
        .is_ok_and(|s| s.success());
    use std::time::{Duration, Instant};
    if !ps0 {
        eprintln!("skipped: this bash doesn't report commands");
        return;
    }
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let data = tempfile::tempdir().unwrap();
    let bash = Some(("bash".to_string(), Vec::new()));
    let (handle, ws) = open_as(cx, bash, None, Some(data.path().to_path_buf()));
    let term = first_id(&ws, cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let reads = git_of(&ws, cx, id, |g| g.reads);
    let line = format!("cd '{}' && git add a.txt\r", repo.path().display());
    ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(term)
            .unwrap()
            .term
            .write(line.into_bytes())
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while ws.read_with(cx, |ws, _| {
        ws.space.terminal(term).unwrap().term.status().finished
    }) < 1
    {
        assert!(Instant::now() < deadline, "the command never ended");
        std::thread::sleep(Duration::from_millis(50));
    }
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.reads > reads, "refreshed by the terminal");
        assert_eq!(
            g.status.entries[0].staged,
            Some(templum_git::Change::Added),
            "{:?}",
            g.status.entries
        );
    });
}

/// A repository with one commit holding a.txt ("one\n").
fn committed_repo() -> tempfile::TempDir {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "First"]);
    repo
}

/// Hovers a row (its buttons show) and clicks one of them.
fn click_row_button(
    handle: AnyWindowHandle,
    cx: &mut TestAppContext,
    row: gpui_kit::ElementId,
    button: gpui_kit::ElementId,
) {
    cx.update_window(handle, |_, window, cx| {
        window.hover(row, cx);
        window.render_frame(cx);
        window.click(button, cx);
    })
    .unwrap();
    settle(handle, cx);
}

fn git_button(id: BlockId, what: &str, list: &str, i: usize) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(format!("git-{what}-{}-{list}-{i}", id.0).into())
}

#[gpui_kit::test]
fn the_plus_button_stages_and_minus_unstages(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    click_row_button(
        handle,
        cx,
        row_id(id, "unstaged", 0),
        git_button(id, "stage", "unstaged", 0),
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.staged().len(), 1);
        assert_eq!(
            g.status.staged()[0].staged,
            Some(templum_git::Change::Added)
        );
        assert!(g.status.unstaged().is_empty());
    });
    click_row_button(
        handle,
        cx,
        row_id(id, "staged", 0),
        git_button(id, "unstage", "staged", 0),
    );
    git_of(&ws, cx, id, |g| {
        assert!(g.status.staged().is_empty());
        assert_eq!(g.status.unstaged().len(), 1);
    });
}

#[gpui_kit::test]
fn stage_all_and_unstage_all(cx: &mut TestAppContext) {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "a\n").unwrap();
    std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-stage-all", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.status.staged().len(), 2));
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-unstage-all", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.status.unstaged().len(), 2));
}

#[gpui_kit::test]
fn discard_asks_first_then_restores_the_file(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        window.hover(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.click(git_button(id, "discard", "unstaged", 0), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "asks first");
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "changed\n"
    );
    cx.update_window(handle, |_, window, cx| {
        window.hover(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.click(git_button(id, "discard", "unstaged", 0), cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "one\n"
    );
    git_of(&ws, cx, id, |g| assert!(g.status.entries.is_empty()));
}

#[gpui_kit::test]
fn a_row_shows_its_diff_and_open_in_editor_opens_the_file_beside_the_block(
    cx: &mut TestAppContext,
) {
    use crate::git_block::row_id;
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(row_id(id, "unstaged", 0), cx)
    })
    .unwrap();
    settle(handle, cx);
    let diff = git_of(&ws, cx, id, |g| {
        assert_eq!(g.selected.as_ref().map(|s| s.1.as_str()), Some("a.txt"));
        assert_eq!(g.diff_line, Some(1));
        g.diff.clone()
    });
    let text = ws.read_with(cx, |_, cx| diff.read(cx).value().to_string());
    assert!(text.contains("+zero"), "{text}");
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find(("git-diff", n)).is_some());
        window.click(("git-open-diff", n), cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, cx| {
        let (eid, e) = ws
            .space
            .blocks
            .iter()
            .find_map(|(id, b)| Some((*id, b.editor()?)))
            .expect("an editor opened");
        assert_eq!(e.active_tab().unwrap().name(), "a.txt");
        assert_eq!(
            e.active_tab()
                .unwrap()
                .editor
                .read(cx)
                .cursor_position()
                .line,
            0
        );
        let (git, editor) = (
            ws.space.canvas.get(id).unwrap().rect,
            ws.space.canvas.get(eid).unwrap().rect,
        );
        assert_eq!(editor.x, git.x + git.w + templum_canvas::BESIDE);
        assert_eq!(editor.y, git.y);
    });
}

#[gpui_kit::test]
fn an_untracked_file_shows_its_content_as_added_and_a_binary_one_says_so(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    std::fs::write(repo.path().join("b.bin"), b"\0\x01").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let shown = |cx: &mut TestAppContext, i: usize| {
        cx.update_window(handle, |_, window, cx| {
            window.click(row_id(id, "unstaged", i), cx)
        })
        .unwrap();
        settle(handle, cx);
        let diff = git_of(&ws, cx, id, |g| g.diff.clone());
        ws.read_with(cx, |_, cx| diff.read(cx).value().to_string())
    };
    assert_eq!(shown(cx, 0), "+++ b/a.txt\n@@ -0,0 +1,1 @@\n+one\n");
    assert_eq!(shown(cx, 1), "Binary file");
}

/// `main` and `topic` both changed a.txt; `git merge topic` is in progress with a conflict.
fn conflicted_repo() -> tempfile::TempDir {
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "topic"]);
    std::fs::write(repo.path().join("a.txt"), "topic\n").unwrap();
    git_run(repo.path(), &["commit", "-q", "-am", "topic"]);
    git_run(repo.path(), &["checkout", "-q", "main"]);
    std::fs::write(repo.path().join("a.txt"), "main\n").unwrap();
    git_run(repo.path(), &["commit", "-q", "-am", "main"]);
    let merge = std::process::Command::new("git")
        .args(["merge", "topic"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(!merge.status.success(), "conflicts");
    repo
}

#[gpui_kit::test]
fn a_conflict_takes_a_side_is_marked_resolved_and_the_merge_continues(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = conflicted_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.operation, Some(templum_git::Operation::Merge));
        assert_eq!(g.status.conflicts().len(), 1);
    });
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find(("git-abort", n)).is_some(), "the banner");
        assert!(
            window.try_find(("git-continue", n)).is_none(),
            "not while conflicted"
        );
    })
    .unwrap();
    click_row_button(
        handle,
        cx,
        row_id(id, "conflicts", 0),
        git_button(id, "ours", "conflicts", 0),
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "main\n"
    );
    click_row_button(
        handle,
        cx,
        row_id(id, "conflicts", 0),
        git_button(id, "resolved", "conflicts", 0),
    );
    git_of(&ws, cx, id, |g| {
        assert!(g.status.conflicts().is_empty());
        assert!(
            g.status.staged().is_empty(),
            "ours is HEAD's content: nothing left to stage"
        );
        assert_eq!(g.operation, Some(templum_git::Operation::Merge));
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-continue", n), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.operation, None);
        assert!(g.status.entries.is_empty());
    });
    assert_eq!(
        git_run(repo.path(), &["rev-list", "--count", "HEAD"]),
        "4\n"
    );
}

#[gpui_kit::test]
fn abort_ends_the_merge(cx: &mut TestAppContext) {
    let repo = conflicted_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-abort", id.0 as usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.operation, None);
        assert!(g.status.entries.is_empty());
    });
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "main\n"
    );
}

#[gpui_kit::test]
fn a_rows_menu_copies_the_path_and_ignores_the_file(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::create_dir(repo.path().join("build")).unwrap();
    std::fs::write(repo.path().join("build/out.o"), "x").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    // Stage, Discard…, Open, Copy path, Ignore.
    cx.update_window(handle, |_, window, cx| {
        window.right_click(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(3usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .unwrap_or_default();
    assert_eq!(text, "build/out.o");
    cx.update_window(handle, |_, window, cx| {
        window.right_click(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        window.within("popup-menu").click(4usize, cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".gitignore")).unwrap(),
        "/build/out.o\n"
    );
    git_of(&ws, cx, id, |g| {
        let paths: Vec<_> = g.status.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, [".gitignore"], "out.o is ignored now");
    });
}

#[cfg(target_os = "macos")]
const COMMIT_KEY: &str = "cmd-enter";
#[cfg(not(target_os = "macos"))]
const COMMIT_KEY: &str = "ctrl-enter";

#[gpui_kit::test]
fn commit_needs_something_staged_and_a_summary_then_commits(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let disabled = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, _| {
            window.try_find(("git-commit", n)).is_none()
        })
        .unwrap()
    };
    assert!(disabled(cx), "nothing staged");
    click_row_button(
        handle,
        cx,
        row_id(id, "unstaged", 0),
        git_button(id, "stage", "unstaged", 0),
    );
    assert!(disabled(cx), "no summary");
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.focus(window, cx));
        window.input("Add a", cx);
        window.render_frame(cx);
        assert!(window.try_find(("git-commit", n)).is_some());
        window.click(("git-commit", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(g.status.entries.is_empty(), "committed");
        assert!(g.status.oid.is_some());
    });
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%s"]),
        "Add a\n"
    );
    ws.read_with(cx, |_, cx| {
        assert_eq!(summary.read(cx).value(), "", "cleared")
    });
    assert_eq!(notification_count(handle, cx), 1, "Committed abc1234");
}

#[gpui_kit::test]
fn ctrl_enter_commits_with_the_description_and_amend_fills_the_fields(cx: &mut TestAppContext) {
    let repo = committed_repo();
    std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
    git_run(repo.path(), &["add", "b.txt"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let (summary, description) =
        git_of(&ws, cx, id, |g| (g.summary.clone(), g.description.clone()));
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.focus(window, cx));
        window.input("Add b", cx);
        description.update(cx, |d, cx| d.focus(window, cx));
        window.input("Why b.", cx);
        window.render_frame(cx);
        window.press(COMMIT_KEY, cx);
    })
    .unwrap();
    settle(handle, cx);
    // %B is the message as stored, and log ends each record with a newline.
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%B"]).trim_end(),
        "Add b\n\nWhy b."
    );
    // Amend: the fields show the last message; off again: they clear.
    cx.update_window(handle, |_, window, cx| window.click(("git-amend", n), cx))
        .unwrap();
    settle(handle, cx);
    ws.read_with(cx, |ws, cx| {
        assert!(ws.space.git(id).unwrap().amend);
        assert_eq!(summary.read(cx).value(), "Add b");
        assert_eq!(description.read(cx).value(), "Why b.");
    });
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("Add b, amended", window, cx));
        window.render_frame(cx);
        window.click(("git-commit", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(repo.path(), &["rev-list", "--count", "HEAD"]),
        "2\n"
    );
    assert_eq!(
        git_run(repo.path(), &["log", "-1", "--format=%s"]),
        "Add b, amended\n"
    );
    git_of(&ws, cx, id, |g| assert!(!g.amend, "off after the commit"));
}

#[gpui_kit::test]
fn without_an_author_commit_is_disabled_and_set_asks_for_one(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = git_repo();
    // The repository's own name and email would win over the global ones Set… writes.
    git_run(repo.path(), &["config", "--unset", "user.name"]);
    git_run(repo.path(), &["config", "--unset", "user.email"]);
    // The global config may name one: hide it from this test's git.
    let home = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let global = home.path().join("gitconfig");
    let wrapper = home.path().join(if cfg!(windows) {
        "git-no-global.cmd"
    } else {
        "git-no-global"
    });
    let script = if cfg!(windows) {
        format!(
            "@set HOME={}\r\n@set GIT_CONFIG_GLOBAL={}\r\n@git %*\r\n",
            home.path().display(),
            global.display()
        )
    } else {
        format!(
            "#!/bin/sh\nHOME='{}' GIT_CONFIG_GLOBAL='{}' exec git \"$@\"\n",
            home.path().display(),
            global.display()
        )
    };
    std::fs::write(&wrapper, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    write_config(
        config.path(),
        &format!(
            "[git]\nprogram = \"{}\"\n",
            wrapper.display().to_string().replace('\\', "\\\\")
        ),
    );
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    let (handle, ws) = open_with(cx, Some(config.path().to_path_buf()));
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| assert_eq!(g.author, None));
    let summary = git_of(&ws, cx, id, |g| g.summary.clone());
    cx.update_window(handle, |_, window, cx| {
        summary.update(cx, |s, cx| s.set_value("x", window, cx));
        window.render_frame(cx);
        assert!(window.try_find(("git-commit", n)).is_none(), "no author");
        window.click(("git-set-author", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        window.input("Grace Hopper", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    // The email's dialog opens once the name's has closed (deferred).
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "then the email");
        window.input("grace@example.com", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(
            g.author,
            Some(("Grace Hopper".to_string(), "grace@example.com".to_string()))
        )
    });
    let written = std::fs::read_to_string(&global).unwrap();
    assert!(written.contains("Grace Hopper"), "{written}");
}

/// A bare remote `origin` for `repo`, with its `main` pushed.
fn add_remote(repo: &std::path::Path) -> tempfile::TempDir {
    let remote = tempfile::tempdir().unwrap();
    git_run(remote.path(), &["init", "-q", "--bare"]);
    // Clones check out its HEAD: main, not git's default branch.
    git_run(remote.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git_run(
        repo,
        &[
            "remote",
            "add",
            "origin",
            &remote.path().display().to_string(),
        ],
    );
    git_run(repo, &["push", "-q", "-u", "origin", "main"]);
    remote
}

#[gpui_kit::test]
fn fetch_shows_the_counts_and_pull_fast_forwards(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let remote = add_remote(repo.path());
    let other = tempfile::tempdir().unwrap();
    git_run(
        other.path(),
        &["clone", "-q", &remote.path().display().to_string(), "."],
    );
    git_run(other.path(), &["config", "user.name", "B"]);
    git_run(other.path(), &["config", "user.email", "b@example.com"]);
    std::fs::write(other.path().join("b.txt"), "b\n").unwrap();
    git_run(other.path(), &["add", "b.txt"]);
    git_run(other.path(), &["commit", "-q", "-m", "b"]);
    git_run(other.path(), &["push", "-q"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!((g.status.ahead, g.status.behind), (0, 0))
    });
    cx.update_window(handle, |_, window, cx| window.click(("git-fetch", n), cx))
        .unwrap();
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.sync, Some(crate::git_block::Sync::Fetch), "a spinner");
    });
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.sync, None);
        assert_eq!(g.status.behind, 1);
    });
    cx.update_window(handle, |_, window, cx| window.click(("git-pull", n), cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.status.behind, 0));
    assert!(repo.path().join("b.txt").is_file());
}

#[gpui_kit::test]
fn push_with_no_upstream_asks_then_publishes(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["checkout", "-q", "-b", "topic"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| assert_eq!(g.status.upstream, None));
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-push", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "asks to publish");
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.upstream.as_deref(), Some("origin/topic"));
    });
}

#[gpui_kit::test]
fn a_pull_that_cant_fast_forward_offers_merge_and_rebase(cx: &mut TestAppContext) {
    // The notification slides in: its buttons take clicks once it is at rest.
    no_motion(cx);
    let repo = committed_repo();
    let remote = add_remote(repo.path());
    let other = tempfile::tempdir().unwrap();
    git_run(
        other.path(),
        &["clone", "-q", &remote.path().display().to_string(), "."],
    );
    git_run(other.path(), &["config", "user.name", "B"]);
    git_run(other.path(), &["config", "user.email", "b@example.com"]);
    std::fs::write(other.path().join("b.txt"), "b\n").unwrap();
    git_run(other.path(), &["add", "b.txt"]);
    git_run(other.path(), &["commit", "-q", "-m", "b"]);
    git_run(other.path(), &["push", "-q"]);
    std::fs::write(repo.path().join("c.txt"), "c\n").unwrap();
    git_run(repo.path(), &["add", "c.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "c"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| window.click(("git-pull", n), cx))
        .unwrap();
    settle(handle, cx);
    assert_eq!(notification_count(handle, cx), 1);
    cx.update_window(handle, |_, window, cx| {
        window.click("git-pull-merge", cx);
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(
        git_run(repo.path(), &["rev-list", "--count", "HEAD"]),
        "4\n",
        "a merge commit"
    );
    assert!(repo.path().join("b.txt").is_file());
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_failed_command_offers_details_and_a_terminal(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    no_motion(cx);
    let repo = committed_repo();
    git_run(repo.path(), &["remote", "add", "origin", "/no/such/remote"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| window.click(("git-fetch", n), cx))
        .unwrap();
    settle(handle, cx);
    assert_eq!(notification_count(handle, cx), 1);
    cx.update_window(handle, |_, window, cx| {
        window.click("git-details", cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "stderr in a dialog");
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        window.click("git-run", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let (term, rect) = ws.read_with(cx, |ws, _| {
        let (tid, _) = ws
            .space
            .terminals()
            .find(|(tid, _)| *tid != first_id_of(ws))
            .expect("a terminal beside the block");
        (tid, ws.space.canvas.get(tid).unwrap().rect)
    });
    let git = ws.read_with(cx, |ws, _| ws.space.canvas.get(id).unwrap().rect);
    assert_eq!(rect.x, git.x + git.w + templum_canvas::BESIDE);
    // The quiet test shell (`cat`) echoes what was typed.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(term).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if shown.contains("git fetch --prune") {
            break;
        }
        assert!(Instant::now() < deadline, "not typed: {shown:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    // Typed, not run: the person reads it and presses Enter (cat would echo a run line
    // a second time).
    std::thread::sleep(Duration::from_millis(300));
    let shown = ws.read_with(cx, |ws, _| {
        let mut snap = Snapshot::default();
        snap.refresh(
            ws.space.terminal(term).unwrap().term.shared(),
            &ws.theme.palette,
        );
        snap.text()
    });
    assert_eq!(shown.matches("git fetch --prune").count(), 1, "{shown:?}");
}

#[cfg(unix)]
fn first_id_of(ws: &Workspace) -> BlockId {
    ws.space.canvas.blocks().first().map(|b| b.id).unwrap()
}

fn commit_file(repo: &std::path::Path, name: &str, text: &str, message: &str) {
    std::fs::write(repo.join(name), text).unwrap();
    git_run(repo, &["add", name]);
    git_run(repo, &["commit", "-q", "-m", message]);
}

/// Opens a Git block on `repo` showing `tab`, loaded.
fn open_git_on(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    repo: &std::path::Path,
    tab: crate::git_block::Tab,
) -> BlockId {
    let id = open_git(handle, ws, cx, repo);
    cx.update_window(handle, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("git-tab-{}-{}", id.0, tab.name()).into()),
            cx,
        )
    })
    .unwrap();
    settle(handle, cx);
    id
}

#[gpui_kit::test]
fn history_lists_the_commits_newest_first_and_shows_one_with_its_files(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second");
    commit_file(repo.path(), "a.txt", "one\ntwo\n", "Third");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        let subjects: Vec<_> = g.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["Third", "Second", "First"]);
        assert!(g.commits[0].refs.contains(&"HEAD -> main".to_string()));
        assert!(!g.more);
    });
    cx.update_window(handle, |_, window, cx| {
        assert!(
            window.try_find(("git-commit", 2usize)).is_some(),
            "three rows"
        );
        window.click(("git-commit", 0usize), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        let shown = g.shown.as_ref().expect("shown");
        assert_eq!(shown.commit.subject, "Third");
        assert_eq!(shown.files.len(), 1);
        assert_eq!(shown.files[0].path, "a.txt");
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("git-commit-file-{n}-0").into()),
            cx,
        )
    })
    .unwrap();
    settle(handle, cx);
    let diff = git_of(&ws, cx, id, |g| g.commit_diff.clone());
    let text = ws.read_with(cx, |_, cx| diff.read(cx).value().to_string());
    assert!(text.contains("+two"), "{text}");
}

#[gpui_kit::test]
fn the_search_filters_by_subject_and_author_without_case(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second");
    git_run(repo.path(), &["config", "user.name", "Grace"]);
    commit_file(repo.path(), "c.txt", "c\n", "Third");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let search = git_of(&ws, cx, id, |g| g.search.clone());
    let filtered = |cx: &mut TestAppContext, text: &str| {
        cx.update_window(handle, |_, window, cx| {
            search.update(cx, |s, cx| s.set_value(text, window, cx));
            window.render_frame(cx);
        })
        .unwrap();
        ws.read_with(cx, |ws, cx| ws.space.git(id).unwrap().filtered(cx))
    };
    assert_eq!(filtered(cx, "SECOND"), [1]);
    assert_eq!(filtered(cx, "grace"), [0]);
    assert_eq!(filtered(cx, ""), [0, 1, 2]);
}

#[gpui_kit::test]
fn load_more_appends_the_next_page(cx: &mut TestAppContext) {
    use crate::git_block::PAGE;
    let repo = committed_repo();
    for i in 0..PAGE {
        git_run(
            repo.path(),
            &["commit", "-q", "--allow-empty", "-m", &format!("c{i}")],
        );
    }
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.commits.len(), PAGE);
        assert!(g.more);
    });
    // The list is virtual: Load more is drawn once the end is scrolled into view.
    cx.update_window(handle, |_, window, cx| {
        // Over a row: the wheel reaches the list around it.
        window.scroll(
            ("git-commit", 0usize),
            ScrollDelta::Pixels(point(px(0.0), px(-100_000.0))),
            cx,
        );
        window.render_frame(cx);
        window.render_frame(cx);
        window.click(("git-load-more", n), cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.commits.len(), PAGE + 1);
        assert!(!g.more);
        assert_eq!(g.commits[PAGE].subject, "First");
    });
}

#[gpui_kit::test]
fn a_commits_menu_branches_from_it_tags_it_and_copies_its_hash(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    commit_file(repo.path(), "b.txt", "b\n", "Second");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let first = git_of(&ws, cx, id, |g| g.commits[1].hash.clone());
    // Checkout…, New branch from here…, Create tag…, Revert…, Copy hash.
    let menu = |cx: &mut TestAppContext, row: usize, item: usize| {
        cx.update_window(handle, |_, window, cx| {
            window.right_click(("git-commit", row), cx);
            window.render_frame(cx);
            window.within("popup-menu").click(item, cx);
            window.render_frame(cx);
        })
        .unwrap();
    };
    menu(cx, 1, 1);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
        window.input("feat", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"));
        assert_eq!(g.status.oid.as_deref(), Some(first.as_str()));
        assert!(
            g.commits[0].refs.iter().any(|r| r.contains("feat")),
            "{:?}",
            g.commits[0].refs
        );
    });
    menu(cx, 0, 2);
    cx.update_window(handle, |_, window, cx| {
        window.input("v1", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(
            g.commits[0].refs.contains(&"tag: v1".to_string()),
            "{:?}",
            g.commits[0].refs
        );
    });
    menu(cx, 0, 4);
    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .unwrap_or_default();
    assert_eq!(text, first);
}

/// Reads asked while another runs wait for it (spec §8: one at a time per block, the second
/// waits); none is dropped.
#[gpui_kit::test]
fn a_commit_clicked_while_a_read_runs_is_still_shown(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let hash = git_of(&ws, cx, id, |g| g.commits[0].hash.clone());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(id, window, cx);
            ws.show_commit(id, hash.clone(), window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        let shown = g
            .shown
            .as_ref()
            .expect("the commit waited for the read, then showed");
        assert_eq!(shown.commit.subject, "First");
        assert_eq!(shown.files.len(), 1);
    });
}

fn branch_row(id: BlockId, remote: bool, i: usize) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(
        format!(
            "git-branch-{}-{}-{i}",
            id.0,
            if remote { "remote" } else { "local" }
        )
        .into(),
    )
}

#[gpui_kit::test]
fn branches_are_listed_with_the_current_one_and_a_click_checks_out(cx: &mut TestAppContext) {
    let repo = committed_repo();
    git_run(repo.path(), &["branch", "feat"]);
    let (handle, ws) = open(cx);
    let id = open_git_on(
        handle,
        &ws,
        cx,
        repo.path(),
        crate::git_block::Tab::Branches,
    );
    git_of(&ws, cx, id, |g| {
        let names: Vec<_> = g
            .branches
            .iter()
            .map(|b| (b.name.as_str(), b.current))
            .collect();
        assert_eq!(names, [("feat", false), ("main", true)]);
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(branch_row(id, false, 0), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feat"));
        assert!(g.branches[0].current, "the list followed");
    });
}

#[gpui_kit::test]
fn new_branch_creates_and_checks_out_and_a_bad_name_is_refused(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    let (handle, ws) = open(cx);
    let id = open_git_on(
        handle,
        &ws,
        cx,
        repo.path(),
        crate::git_block::Tab::Branches,
    );
    let n = id.0 as usize;
    let create = |cx: &mut TestAppContext, name: &str| {
        cx.update_window(handle, |_, window, cx| {
            window.click(("git-new-branch", n), cx);
            window.render_frame(cx);
            assert!(window.has_active_dialog(cx));
            window.input(name, cx);
            window.press("enter", cx);
            window.render_frame(cx);
        })
        .unwrap();
        settle(handle, cx);
    };
    create(cx, "feature/one");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feature/one"));
        assert_eq!(g.branches.len(), 2);
    });
    create(cx, "bad name");
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("feature/one"), "unchanged");
        assert_eq!(g.branches.len(), 2);
    });
    assert_eq!(notification_count(handle, cx), 1, "git's own message");
}

#[gpui_kit::test]
fn a_refused_checkout_offers_stash_and_switch(cx: &mut TestAppContext) {
    // The notification slides in: its buttons take clicks once it is at rest.
    no_motion(cx);
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "other"]);
    commit_file(repo.path(), "a.txt", "other\n", "other a");
    git_run(repo.path(), &["checkout", "-q", "main"]);
    std::fs::write(repo.path().join("a.txt"), "dirty\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git_on(
        handle,
        &ws,
        cx,
        repo.path(),
        crate::git_block::Tab::Branches,
    );
    // main, other.
    cx.update_window(handle, |_, window, cx| {
        window.click(branch_row(id, false, 1), cx)
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("main"), "refused")
    });
    assert_eq!(notification_count(handle, cx), 1);
    cx.update_window(handle, |_, window, cx| window.click("git-stash-switch", cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.branch.as_deref(), Some("other"));
        assert!(g.status.entries.is_empty(), "stashed");
    });
    assert!(git_run(repo.path(), &["stash", "list"]).contains("before switching"));
}

#[gpui_kit::test]
fn a_branchs_menu_renames_merges_publishes_and_deletes_asking_to_force(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["checkout", "-q", "-b", "feat"]);
    commit_file(repo.path(), "b.txt", "b\n", "b");
    git_run(repo.path(), &["checkout", "-q", "main"]);
    let (handle, ws) = open(cx);
    let id = open_git_on(
        handle,
        &ws,
        cx,
        repo.path(),
        crate::git_block::Tab::Branches,
    );
    // feat, main; feat's menu: Rename…, Merge into main…, Delete…, Publish, Copy name.
    let menu = |cx: &mut TestAppContext, row: usize, item: usize| {
        cx.update_window(handle, |_, window, cx| {
            window.right_click(branch_row(id, false, row), cx);
            window.render_frame(cx);
            window.within("popup-menu").click(item, cx);
            window.render_frame(cx);
        })
        .unwrap();
    };
    menu(cx, 0, 0);
    cx.update_window(handle, |_, window, cx| {
        window.press("ctrl-a", cx);
        window.press("cmd-a", cx);
        window.input("feature", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert_eq!(g.branches[0].name, "feature"));
    // Delete… before it has an upstream: unmerged anywhere, so git refuses.
    menu(cx, 0, 2);
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.branches.len(), 3, "not merged: still there")
    });
    assert_eq!(notification_count(handle, cx), 1);
    menu(cx, 0, 3);
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.branches[0].upstream.as_deref(), Some("origin/feature"))
    });
    cx.update_window(handle, |_, window, cx| window.click("git-delete-force", cx))
        .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert!(
            !g.branches.iter().any(|b| b.name == "feature"),
            "{:?}",
            g.branches
        );
    });
    // Merge the remote one into main: origin/feature is the only remote row besides origin/main.
    let remote_rows = git_of(&ws, cx, id, |g| {
        g.branches.iter().filter(|b| b.remote).count()
    });
    assert_eq!(remote_rows, 2);
    cx.update_window(handle, |_, window, cx| {
        window.right_click(branch_row(id, true, 0), cx);
        window.render_frame(cx);
        // Merge into main…, Copy name.
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert!(repo.path().join("b.txt").is_file(), "merged");
}

fn stash_row(id: BlockId, i: usize) -> gpui_kit::ElementId {
    gpui_kit::ElementId::Name(format!("git-stash-{}-{i}", id.0).into())
}

#[gpui_kit::test]
fn stash_changes_asks_for_a_message_and_lists_the_stash(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::Stashes);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-stash-changes", n), cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        window.input("my work", cx);
        window.within("dialog").click("flag", cx);
        window.render_frame(cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.stashes.len(), 1);
        assert_eq!(g.stashes[0].message, "On main: my work");
        assert!(g.status.entries.is_empty(), "untracked went too");
    });
    assert!(!repo.path().join("new.txt").exists());
}

#[gpui_kit::test]
fn a_stash_is_shown_applied_popped_and_dropped(cx: &mut TestAppContext) {
    no_motion(cx);
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    git_run(repo.path(), &["stash", "push", "-q", "-m", "one"]);
    std::fs::write(repo.path().join("a.txt"), "other\n").unwrap();
    git_run(repo.path(), &["stash", "push", "-q", "-m", "two"]);
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::Stashes);
    git_of(&ws, cx, id, |g| assert_eq!(g.stashes.len(), 2));
    cx.update_window(handle, |_, window, cx| window.click(stash_row(id, 1), cx))
        .unwrap();
    settle(handle, cx);
    let diff = git_of(&ws, cx, id, |g| {
        assert_eq!(g.stash_shown, Some(1));
        g.stash_diff.clone()
    });
    let text = ws.read_with(cx, |_, cx| diff.read(cx).value().to_string());
    assert!(text.contains("+changed"), "{text}");
    // Apply, Pop, Drop…, Show.
    let menu = |cx: &mut TestAppContext, row: usize, item: usize| {
        cx.update_window(handle, |_, window, cx| {
            window.right_click(stash_row(id, row), cx);
            window.render_frame(cx);
            window.within("popup-menu").click(item, cx);
            window.render_frame(cx);
        })
        .unwrap();
        settle(handle, cx);
    };
    menu(cx, 1, 0);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "changed\n"
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.stashes.len(), 2, "apply keeps it")
    });
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    menu(cx, 0, 1);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "other\n"
    );
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.stashes.len(), 1, "pop drops it")
    });
    menu(cx, 0, 2);
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| assert!(g.stashes.is_empty()));
}

// Final review, C1: the 2-second read must leave the diffs alone.
#[gpui_kit::test]
fn a_commits_file_diff_survives_the_two_second_read(cx: &mut TestAppContext) {
    let repo = committed_repo();
    commit_file(repo.path(), "a.txt", "one\ntwo\n", "Second");
    let (handle, ws) = open(cx);
    let id = open_git_on(handle, &ws, cx, repo.path(), crate::git_block::Tab::History);
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-commit", 0usize), cx)
    })
    .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("git-commit-file-{n}-0").into()),
            cx,
        )
    })
    .unwrap();
    settle(handle, cx);
    let diff = git_of(&ws, cx, id, |g| g.commit_diff.clone());
    let text =
        |cx: &mut TestAppContext| ws.read_with(cx, |_, cx| diff.read(cx).value().to_string());
    assert!(text(cx).contains("+two"));
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    assert!(text(cx).contains("+two"), "still shown: {:?}", text(cx));
}

#[gpui_kit::test]
fn the_changes_diff_isnt_reset_by_a_read_that_changes_nothing(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    use gpui_kit::component::input::Position;
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        window.click(row_id(id, "unstaged", 0), cx)
    })
    .unwrap();
    settle(handle, cx);
    let diff = git_of(&ws, cx, id, |g| g.diff.clone());
    cx.update_window(handle, |_, window, cx| {
        diff.update(cx, |d, cx| {
            d.set_cursor_position(Position::new(2, 0), window, cx)
        })
    })
    .unwrap();
    cx.executor().advance_clock(crate::git_block::POLL);
    settle(handle, cx);
    ws.read_with(cx, |_, cx| {
        assert_eq!(
            diff.read(cx).cursor_position().line,
            2,
            "where the reader left it"
        )
    });
}

/// `main` with one commit, and `feature` one commit ahead of it (unmerged).
fn repo_with_unmerged_feature() -> tempfile::TempDir {
    let repo = committed_repo();
    git_run(repo.path(), &["checkout", "-q", "-b", "feature"]);
    commit_file(repo.path(), "f.txt", "f\n", "feature work");
    git_run(repo.path(), &["checkout", "-q", "main"]);
    repo
}

// Final review, C2: a notification's buttons act on its own block, in its own workspace.
#[gpui_kit::test]
fn delete_anyway_after_a_switch_acts_on_its_own_repository(cx: &mut TestAppContext) {
    no_motion(cx);
    let (x, y) = (repo_with_unmerged_feature(), repo_with_unmerged_feature());
    let (handle, ws) = open(cx);
    let in_x = open_git(handle, &ws, cx, x.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.delete_now(in_x, "feature".to_string(), false, window, cx)
        })
    })
    .unwrap();
    settle(handle, cx);
    assert_eq!(notification_count(handle, cx), 1, "refused: not merged");
    // A new workspace whose first block takes id 0, like X's terminal: Y's Git block gets
    // X's id.
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.new_space(window, cx);
            ws.add_note_at(Point::new(3000.0, 3000.0), window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    let in_y = open_git(handle, &ws, cx, y.path());
    assert_eq!(in_y, in_x, "block ids repeat across workspaces");
    cx.update_window(handle, |_, window, cx| window.click("git-delete-force", cx))
        .unwrap();
    settle(handle, cx);
    assert!(
        !git_run(y.path(), &["branch", "--list", "feature"]).is_empty(),
        "the other workspace's repository is untouched"
    );
    assert!(
        git_run(x.path(), &["branch", "--list", "feature"]).is_empty(),
        "deleted in X"
    );
}

// Final review I5: a read isn't an action: it hides no button and takes no action's turn.
#[gpui_kit::test]
fn the_rows_buttons_stay_while_a_read_runs(cx: &mut TestAppContext) {
    use crate::git_block::row_id;
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.refresh_git(id, window, cx));
        window.render_frame(cx);
        window.hover(row_id(id, "unstaged", 0), cx);
        window.render_frame(cx);
        assert!(
            window
                .try_find(git_button(id, "stage", "unstaged", 0))
                .is_some()
        );
    })
    .unwrap();
    git_of(&ws, cx, id, |g| assert!(g.busy, "a read was out"));
}

#[gpui_kit::test]
fn publish_asked_during_a_read_waits_for_it(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    git_run(repo.path(), &["checkout", "-q", "-b", "topic"]);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(id, window, cx);
            ws.publish_branch(id, "topic".to_string(), window, cx);
        })
    })
    .unwrap();
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.status.upstream.as_deref(), Some("origin/topic"))
    });
}

#[gpui_kit::test]
fn the_timer_skips_a_block_whose_read_is_still_out(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.refresh_git(id, window, cx);
            ws.poll_git(window, cx);
        })
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        assert!(g.busy);
        assert!(!g.pending, "a slow repository never piles reads up");
    });
}

// Final review I4 in the block: the spinning button cancels its command, quietly.
#[gpui_kit::test]
fn the_running_fetch_button_cancels_it(cx: &mut TestAppContext) {
    let repo = committed_repo();
    let _remote = add_remote(repo.path());
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-fetch", n), cx);
        window.render_frame(cx);
        window.click(("git-fetch", n), cx);
    })
    .unwrap();
    git_of(&ws, cx, id, |g| {
        let cancel = g.cancel.as_ref().expect("a running fetch can be cancelled");
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
    });
    settle(handle, cx);
    git_of(&ws, cx, id, |g| {
        assert_eq!(g.sync, None);
        assert!(g.cancel.is_none());
    });
    assert_eq!(
        notification_count(handle, cx),
        0,
        "cancelling isn't a failure"
    );
}

// Re-graded from minor: a space can be typed in Settings ▸ Tools' Git field.
#[gpui_kit::test]
fn the_git_program_field_keeps_a_typed_space(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.update(cx, |ws, cx| {
        crate::settings::set_git_program(ws, "C:\\Program ", cx)
    });
    ws.update(cx, |ws, _| {
        let git = &ws.pending.as_ref().unwrap_or(&ws.config).git;
        assert_eq!(
            git.program, "C:\\Program ",
            "the field shows what was typed"
        );
        assert_eq!(git.program(), "C:\\Program", "what runs is trimmed");
    });
    ws.update(cx, |ws, cx| crate::settings::set_git_program(ws, "  ", cx));
    ws.update(cx, |ws, _| {
        assert_eq!(
            ws.pending.as_ref().unwrap_or(&ws.config).git.program(),
            "git"
        )
    });
}

// The editor's gutter, background and current line follow Templum's theme, not gpui-kit's
// fixed white (light) or near-black (dark) editor colors.
#[gpui_kit::test]
fn the_editors_gutter_and_current_line_follow_the_theme(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme as _;
    let (_, ws) = open(cx);
    for dark in [true, false] {
        let theme = ws.update(cx, |ws, cx| {
            let theme = crate::theme::pick(&ws.themes, crate::theme::default_name(dark), "");
            ws.apply(theme.clone(), cx);
            theme
        });
        cx.update(|cx| {
            let style = &cx.theme().highlight_theme.style;
            assert_eq!(
                style.editor_background,
                Some(theme.ui.surface),
                "dark: {dark}"
            );
            assert_eq!(
                style.editor_gutter_background,
                Some(theme.ui.surface),
                "dark: {dark}"
            );
            assert_eq!(
                style.editor_active_line,
                Some(theme.ui.card),
                "dark: {dark}"
            );
        });
    }
}

/// Every element inside `block` (registered for tests), relative to the block: its
/// position and size.
fn block_layout(window: &Window, block: BlockId) -> std::collections::BTreeMap<String, [f32; 4]> {
    let id = gpui_kit::ElementId::NamedInteger("block".into(), block.0);
    let origin = window.find(("block", block.0 as usize)).bounds().origin;
    gpui_kit::base::test_support::snapshots(window)
        .into_iter()
        .filter_map(|s| {
            let at = s.path().iter().position(|p| *p == id)?;
            let b = s.bounds();
            Some((
                format!("{:?}", &s.path()[at..]),
                [
                    f32::from(b.origin.x - origin.x),
                    f32::from(b.origin.y - origin.y),
                    f32::from(b.size.width),
                    f32::from(b.size.height),
                ],
            ))
        })
        .collect()
}

// The canvas zoom magnifies the editor and the Git block like a photo: everything inside
// doubles at zoom 2, nothing keeps a fixed size or moves around (hairline borders stay
// 1 px, hence the few pixels of slack).
#[gpui_kit::test]
fn zooming_scales_the_editor_and_git_blocks_like_a_photo(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "fn main() {\n    println!(\"hi\");\n}\n",
    )
    .unwrap();
    let repo = committed_repo();
    std::fs::write(repo.path().join("a.txt"), "zero\none\n").unwrap();
    let (handle, ws) = open(cx);
    let editor = open_editor(handle, &ws, cx, dir.path());
    click_file(handle, 0, cx);
    let git = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let id = ws
                    .add_git_at(repo.path(), Point::new(40.0, 700.0), window, cx)
                    .unwrap();
                ws.space.appearing.clear();
                ws.select_git_row(
                    id,
                    crate::git_block::List::Unstaged,
                    "a.txt".to_string(),
                    window,
                    cx,
                );
                id
            })
        })
        .unwrap();
    settle(handle, cx);
    // Each block measured with the camera on it, at zoom 1 and 2.
    let layout = |cx: &mut TestAppContext, block: BlockId, at: Point, zoom: f32| {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera = Camera::at(at, zoom);
            ws.space.view = ws.space.canvas.camera;
        });
        frame(handle, cx);
        cx.update_window(handle, |_, window, _| block_layout(window, block))
            .unwrap()
    };
    let compare = |name: &str,
                   one: &std::collections::BTreeMap<String, [f32; 4]>,
                   two: &std::collections::BTreeMap<String, [f32; 4]>| {
        assert!(one.len() > 5, "{name}: measured {}", one.len());
        for (key, a) in one {
            let b = two
                .get(key)
                .unwrap_or_else(|| panic!("{name}: gone at zoom 2: {key}"));
            for (x1, x2) in a.iter().zip(b) {
                // Slack: hairlines that stay 1 px add up, more so for what sits at the
                // right edge. A fixed-size widget is off by its own size (14 px, 24 px…).
                let slack = 6.0 + x2.abs() * 0.01;
                assert!(
                    (x1 * 2.0 - x2).abs() <= slack,
                    "{name}: {key}\n zoom 1 {a:?}\n zoom 2 {b:?}"
                );
            }
        }
    };
    let tabs = [
        crate::git_block::Tab::Changes,
        crate::git_block::Tab::History,
        crate::git_block::Tab::Branches,
    ];
    let one = layout(cx, editor, Point::new(0.0, 0.0), 1.0);
    let two = layout(cx, editor, Point::new(0.0, 0.0), 2.0);
    compare("editor", &one, &two);
    for tab in tabs {
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.set_git_tab(git, tab, window, cx))
        })
        .unwrap();
        settle(handle, cx);
        let one = layout(cx, git, Point::new(0.0, 660.0), 1.0);
        let two = layout(cx, git, Point::new(0.0, 680.0), 2.0);
        compare(tab.name(), &one, &two);
    }
}

#[gpui_kit::test]
fn double_clicking_a_title_frames_the_block_and_again_goes_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let (before, vp) = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let ws = ws.read(cx);
            (ws.space.canvas.camera, ws.viewport(window))
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.double_click(("title", id.0 as usize), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let camera = ws.space.canvas.camera;
        assert!(camera.zoom() > before.zoom(), "bigger: {}", camera.zoom());
        let r = camera.rect_to_screen(ws.space.canvas.get(id).unwrap().rect);
        assert!(
            (r.x + r.w / 2.0 - vp.w / 2.0).abs() < 1.0,
            "centered: {r:?}"
        );
        assert!(
            r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= vp.w,
            "on screen: {r:?}"
        );
        assert!(
            r.y + r.h <= vp.h - crate::toolbar::SPACE,
            "clear of the toolbar: {r:?}"
        );
        assert!(ws.drag.is_none(), "a double-click doesn't move the block");
    });
    // The glide ends first: a title still moving isn't where the pointer goes.
    std::thread::sleep(crate::motion::FLIGHT + std::time::Duration::from_millis(50));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.double_click(("title", id.0 as usize), cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera, before, "back where it was")
    });
}

#[gpui_kit::test]
fn with_the_hand_the_wheel_zooms_over_blocks_and_the_canvas(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-hand", cx);
        window.render_frame(cx);
        let body = window.find(("body", id.0 as usize)).bounds().center();
        wheel(
            window,
            cx,
            (f32::from(body.x), f32::from(body.y)),
            ScrollDelta::Lines(point(0.0, 1.0)),
            false,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.1));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        wheel(
            window,
            cx,
            (f32::from(o.x) + 30.0, f32::from(o.y) + 30.0),
            ScrollDelta::Pixels(point(px(0.), px(-60.))),
            false,
        );
    })
    .unwrap();
    ws.read_with(cx, |ws, _| assert_eq!(ws.space.canvas.camera.zoom(), 1.0));
}

#[gpui_kit::test]
fn holding_alt_lights_the_hand_in_the_toolbar_until_let_go(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let lit = |window: &Window, id: &'static str| window.find(id).selected();
        assert_eq!(lit(window, "tool-select"), Some(true));
        assert_eq!(lit(window, "tool-hand"), Some(false));
        hold_alt(window, cx, true);
        assert_eq!(lit(window, "tool-hand"), Some(true));
        assert_eq!(lit(window, "tool-select"), Some(false));
        hold_alt(window, cx, false);
        assert_eq!(lit(window, "tool-select"), Some(true));
        assert_eq!(lit(window, "tool-hand"), Some(false));
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.tool, super::Tool::Select, "Alt doesn't change the tool")
    });
}

/// The folder a terminal's program runs in, once the OS says, canonicalized.
fn cwd_of(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId) -> std::path::PathBuf {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let cwd = ws.read_with(cx, |ws, _| ws.space.terminal(id).unwrap().term.cwd());
        if let Some(cwd) = cwd {
            return cwd.canonicalize().unwrap();
        }
        assert!(Instant::now() < deadline, "no cwd");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[gpui_kit::test]
fn a_new_terminal_starts_in_the_focused_editors_or_git_blocks_folder(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let new_terminal = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| window.press(NEW, cx))
            .unwrap();
        ws.read_with(cx, |ws, _| ws.space.canvas.top().unwrap())
    };
    open_editor(handle, &ws, cx, dir.path());
    let id = new_terminal(cx);
    assert_eq!(cwd_of(&ws, cx, id), dir.path().canonicalize().unwrap());
    open_git(handle, &ws, cx, repo.path());
    let id = new_terminal(cx);
    assert_eq!(cwd_of(&ws, cx, id), repo.path().canonicalize().unwrap());
}

#[gpui_kit::test]
fn a_double_click_on_the_canvas_opens_in_the_folder_of_the_block_it_unfocused(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let start = tempfile::tempdir().unwrap();
    // A TOML literal string: Windows backslashes need no escaping.
    write_config(
        dir.path(),
        &format!(
            "[shell]\nworking_directory = '{}'\n",
            start.path().display()
        ),
    );
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    write_config(dir.path(), "[shell]\nworking_directory = \"inherit\"\n");
    reload(handle, &ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let o = ws.read(cx).origin;
        let at = (f32::from(o.x) + 40.0, f32::from(o.y) + 40.0);
        click(window, cx, at, 1);
        click(window, cx, at, 2);
    })
    .unwrap();
    let new = ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.blocks().len(), 2);
        ws.space.canvas.top().unwrap()
    });
    assert_eq!(cwd_of(&ws, cx, new), start.path().canonicalize().unwrap());
}
