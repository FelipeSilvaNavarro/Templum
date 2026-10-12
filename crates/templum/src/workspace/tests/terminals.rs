//! Terminal blocks: bells, selection, keys, cursor, prompts and the folder they start in.

use super::*;

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

#[test]
fn macos_option_types_the_layout_character() {
    use crate::workspace::option_types_text;
    // German layout: Option+L types "@".
    assert!(option_types_text("l", Some("@"), true));
    // Option+arrows and Option+Backspace carry no text: they stay Meta.
    assert!(!option_types_text("left", None, true));
    assert!(!option_types_text("backspace", Some("\x7f"), true));
    // Elsewhere Alt is always Meta (AltGr is not reported as Alt on Linux).
    assert!(!option_types_text("l", Some("@"), false));
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

/// Output in a block off screen wakes the UI once and no more until the block is painted; a
/// painted block is re-armed every frame. Scrolled back on screen, the block shows what it
/// printed meanwhile; an off-screen terminal that exits still closes its block.
#[cfg(unix)]
#[gpui_kit::test]
fn only_painted_terminals_are_rearmed(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let shown = first_id(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(NEW, cx);
    })
    .unwrap();
    let hidden = ws.read_with(cx, |ws, _| ws.space.canvas.top().unwrap());
    assert_ne!(shown, hidden);
    // Away: far off screen. Back: where the shown block is, on screen.
    let park = |ws: &mut Workspace, away: bool| {
        let home = ws.space.canvas.get(shown).unwrap().rect;
        let rect = &mut ws.space.canvas.get_mut(hidden).unwrap().rect;
        (rect.x, rect.y) = if away {
            (100_000.0, 100_000.0)
        } else {
            (home.x, home.y)
        };
    };
    ws.update(cx, |ws, _| park(ws, true));
    let text =
        |ws: &Workspace, id: BlockId| ws.space.terminal(id).unwrap().snapshot.borrow().text();
    let armed = |ws: &Workspace, id: BlockId| ws.space.terminal(id).unwrap().term.shared().armed();
    // `cat` echoes the line back: output in both terminals.
    for id in [shown, hidden] {
        ws.read_with(cx, |ws, _| {
            ws.space.terminal(id).unwrap().term.write(b"x\r".to_vec())
        });
    }
    until("both terminals to print", handle, &ws, cx, |ws| {
        text(ws, shown).starts_with("x\nx") && armed(ws, shown) && !armed(ws, hidden)
    });
    ws.read_with(cx, |ws, _| {
        assert_eq!(text(ws, hidden).trim_end(), "", "never read while hidden")
    });
    // A few more frames: the hidden block is still waiting to be painted.
    for _ in 0..3 {
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    ws.read_with(cx, |ws, _| {
        assert!(!armed(ws, hidden), "the hidden block isn't re-armed")
    });
    // Back on screen: its first frames show what it printed.
    ws.update(cx, |ws, _| park(ws, false));
    until("the hidden block to catch up", handle, &ws, cx, |ws| {
        text(ws, hidden).starts_with("x\nx") && armed(ws, hidden)
    });
    // Off screen again, its program exits (Ctrl-D ends `cat`): the block still closes.
    ws.update(cx, |ws, _| park(ws, true));
    ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(hidden)
            .unwrap()
            .term
            .write(b"\x04".to_vec())
    });
    until("the exited block to close", handle, &ws, cx, |ws| {
        ws.space.terminal(hidden).is_none()
    });
}
