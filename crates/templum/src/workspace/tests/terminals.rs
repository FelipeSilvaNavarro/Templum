//! Terminal blocks: bells, selection, keys, cursor, prompts and the folder they start in.

use super::*;

/// Draws frames (each hears the terminals) until `done`.
#[cfg(unix)]
pub(super) fn until(
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
pub(super) fn set_active(handle: AnyWindowHandle, active: bool, cx: &mut TestAppContext) {
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
    // The command's end comes before bash prints its next prompt: without that prompt there is
    // nothing below to jump to.
    let last_line = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            let text = snap.text();
            text.lines()
                .rfind(|l| !l.trim().is_empty())
                .unwrap_or("")
                .to_string()
        })
    };
    while last_line(cx) == "100" {
        assert!(Instant::now() < deadline, "bash never printed its prompt");
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

#[cfg(target_os = "macos")]
const WAITING: &str = "cmd-shift-a";
#[cfg(not(target_os = "macos"))]
const WAITING: &str = "ctrl-shift-a";

/// Its shortcut shows each terminal that asked for attention in turn, across workspaces (this
/// one's first), never one that merely finished a command; then it says none is waiting.
#[gpui_kit::test]
fn go_to_waiting_visits_each_waiting_terminal_then_says_none(cx: &mut TestAppContext) {
    use crate::alert::Alert;
    let (handle, ws) = open(cx);
    let first = first_id(&ws, cx);
    let (home, there) = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let home = ws.space.id.clone();
                ws.add_terminal_at(Point::new(900.0, 0.0), window, cx);
                let finished = ws.space.canvas.top().unwrap();
                ws.space.terminal_mut(finished).unwrap().alert =
                    Some(Alert::Finished(templum_terminal::Outcome {
                        code: Some(0),
                        took: std::time::Duration::from_secs(9),
                    }));
                ws.space.terminal_mut(first).unwrap().alert = Some(Alert::Attention(None));
                ws.new_space(window, cx);
                ws.add_terminal_at(Point::new(0.0, 0.0), window, cx);
                let there = ws.space.canvas.top().unwrap();
                ws.space.terminal_mut(there).unwrap().alert =
                    Some(Alert::Attention(Some("Allow?".into())));
                ws.switch_to(0, window, cx);
                ws.focus_block(finished, window, cx);
                (home, (ws.order()[1].id.clone(), there))
            })
        })
        .unwrap();
    let press = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.press(WAITING, cx);
            window.render_frame(cx);
        })
        .unwrap();
        ws.read_with(cx, |ws, _| (ws.space.id.clone(), ws.space.focused))
    };
    assert_eq!(
        press(cx),
        (home.clone(), Some(first)),
        "this workspace's first"
    );
    assert_eq!(
        press(cx),
        (there.0.clone(), Some(there.1)),
        "then the other's"
    );
    let notes = notification_count(handle, cx);
    assert_eq!(press(cx), (there.0, Some(there.1)), "none left: it stays");
    assert_eq!(notification_count(handle, cx), notes + 1, "and says so");
}

/// Shift+Enter sends ESC CR (a newline in Claude Code and Codex) only where an agent runs; a
/// shell gets Enter. `cat` echoes ESC as "^[".
#[cfg(unix)]
#[gpui_kit::test]
fn shift_enter_is_a_newline_only_in_an_agent(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let shown_after = |cx: &mut TestAppContext, mark: &str| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("shift-enter", cx);
        })
        .unwrap();
        ws.read_with(cx, |ws, _| {
            let term = &ws.space.terminal(id).unwrap().term;
            term.write(format!("{mark}\r").into_bytes())
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
            if shown.contains(mark) {
                return shown;
            }
            assert!(Instant::now() < deadline, "no echo: {shown:?}");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let shell = shown_after(cx, "x");
    assert!(!shell.contains("^["), "a shell gets Enter: {shell:?}");
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().seen.agent = Some("claude".into())
    });
    let agent = shown_after(cx, "y");
    assert!(agent.contains("^["), "an agent gets ESC CR: {agent:?}");
}

#[cfg(target_os = "macos")]
const FIND: &str = "cmd-f";
#[cfg(not(target_os = "macos"))]
const FIND: &str = "ctrl-shift-f";

/// The find field selects what it finds, says when nothing matches, and Esc gives the terminal
/// the keyboard back. Nothing typed in it (Enter included) reaches the shell.
#[cfg(unix)]
#[gpui_kit::test]
fn the_find_field_selects_what_it_finds_and_esc_gives_the_terminal_back(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let screen = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        })
    };
    let typed_until = |cx: &mut TestAppContext, text: &str, shown: &str| {
        ws.read_with(cx, |ws, _| {
            ws.space
                .terminal(id)
                .unwrap()
                .term
                .write(text.as_bytes().to_vec())
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !screen(cx).contains(shown) {
            assert!(Instant::now() < deadline, "{:?}", screen(cx));
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    // cat shows it twice: its echo, then its output.
    typed_until(cx, "hello\r", "hello\nhello");
    let selected = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.space
                .terminal(id)
                .unwrap()
                .term
                .shared()
                .selection_text()
        })
    };
    let missing = |cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            ws.space
                .terminal(id)
                .unwrap()
                .find
                .as_ref()
                .unwrap()
                .missing
        })
    };
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(FIND, cx);
        window.render_frame(cx);
        assert!(
            window.try_find(("find", id.0 as usize)).is_some(),
            "the field"
        );
    })
    .unwrap();
    cx.simulate_input(handle, "hello");
    assert_eq!(selected(cx).as_deref(), Some("hello"));
    assert!(!missing(cx));
    cx.update_window(handle, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(selected(cx).as_deref(), Some("hello"), "the older one");
    cx.simulate_input(handle, "zzz");
    assert!(missing(cx), "no matches");
    assert_eq!(selected(cx), None);
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        let ws = ws.read(cx);
        assert!(ws.space.terminal(id).unwrap().find.is_none(), "closed");
        assert!(
            ws.focus_handle.is_focused(window),
            "the terminal's keys again"
        );
    })
    .unwrap();
    // Nothing reached cat in between: the next line comes right after.
    typed_until(cx, "mark\r", "mark\nmark");
    assert!(
        screen(cx).contains("hello\nhello\nmark\nmark"),
        "{:?}",
        screen(cx)
    );
}

/// A press with Ctrl (Cmd on macOS) at a terminal's cell `(row, col)`.
#[cfg(unix)]
pub(super) fn ctrl_click_cell(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
    (row, col): (usize, usize),
) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let body = window.find(("body", id.0 as usize)).bounds();
        let (cell, z) = {
            let ws = ws.read(cx);
            (ws.cell.size, ws.space.view.zoom())
        };
        let pad = templum_canvas::chrome::PAD * z;
        let position = point(
            body.origin.x + px(pad + (col as f32 + 0.5) * cell.w * z),
            body.origin.y + px(pad + (row as f32 + 0.5) * cell.h * z),
        );
        let modifiers = Modifiers::secondary_key();
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
}

/// Writes `text` into the terminal (cat shows it) and waits until `shown` is on screen.
#[cfg(unix)]
fn shown_in(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId, text: &str, shown: &str) {
    use std::time::{Duration, Instant};
    use templum_terminal::Snapshot;
    ws.read_with(cx, |ws, _| {
        ws.space
            .terminal(id)
            .unwrap()
            .term
            .write(text.as_bytes().to_vec())
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let screen = ws.read_with(cx, |ws, _| {
            let mut snap = Snapshot::default();
            snap.refresh(
                ws.space.terminal(id).unwrap().term.shared(),
                &ws.theme.palette,
            );
            snap.text()
        });
        if screen.contains(shown) {
            return;
        }
        assert!(Instant::now() < deadline, "{screen:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Ctrl+click on `main.rs:2` printed in a terminal opens the file beside it at line 2, in an
/// editor on the repository.
#[cfg(unix)]
#[gpui_kit::test]
fn ctrl_click_on_a_path_opens_it_beside_the_terminal_at_its_line(cx: &mut TestAppContext) {
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/main.rs"), "one\ntwo\nthree\n").unwrap();
    let (handle, ws) = open(cx);
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let rect = templum_canvas::Rect::new(40.0, 40.0, 600.0, 300.0);
                let id = ws
                    .spawn_block(rect, Some(repo.path().join("src")), window, cx)
                    .unwrap();
                ws.space.appearing.clear();
                id
            })
        })
        .unwrap();
    shown_in(&ws, cx, id, "main.rs:2\r", "main.rs:2\nmain.rs:2");
    ctrl_click_cell(handle, &ws, cx, id, (0, 2));
    ws.read_with(cx, |ws, cx| {
        let (eid, e) = ws
            .space
            .blocks
            .iter()
            .find_map(|(id, b)| Some((*id, b.editor()?)))
            .expect("an editor opened");
        assert_eq!(
            e.folder.canonicalize().unwrap(),
            repo.path().canonicalize().unwrap(),
            "the repository, not the terminal's folder"
        );
        let tab = e.active_tab().unwrap();
        assert_eq!(tab.name(), "main.rs");
        assert_eq!(tab.editor.read(cx).cursor_position().line, 1);
        let rect = |b| ws.space.canvas.get(b).unwrap().rect;
        assert_eq!(
            rect(eid).x,
            rect(id).x + rect(id).w + templum_canvas::BESIDE
        );
        assert_eq!(ws.space.focused, Some(eid));
        assert!(
            ws.space
                .terminal(id)
                .unwrap()
                .term
                .shared()
                .selection_text()
                .is_none()
        );
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn ctrl_click_on_a_url_opens_the_browser(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    shown_in(
        &ws,
        cx,
        id,
        "see https://example.com/x\r",
        "x\nsee https://example.com/x",
    );
    ctrl_click_cell(handle, &ws, cx, id, (0, 8));
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/x"));
}

/// The terminal's paste shortcut pastes into its find field, not into the shell.
#[gpui_kit::test]
fn the_paste_shortcut_pastes_into_the_find_field(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("needle".into()));
    let paste = if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    };
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(FIND, cx);
        window.render_frame(cx);
        window.press(paste, cx);
        window.render_frame(cx);
        let ws = ws.read(cx);
        let bar = ws.space.terminal(id).unwrap().find.as_ref().unwrap();
        assert_eq!(bar.input.read(cx).value().as_ref(), "needle");
    })
    .unwrap();
}

/// `cat` prints back what it reads: written to it, `request` reaches the terminal as its
/// program's (a TUI turning on the mouse, the kitty keyboard, the full screen).
#[cfg(unix)]
fn program_asks(
    request: &str,
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    asked: impl Fn(templum_terminal::Modes) -> bool,
) {
    let id = first_id(ws, cx);
    ws.read_with(cx, |ws, _| {
        let term = &ws.space.terminal(id).unwrap().term;
        term.write(format!("{request}\r").into_bytes())
    });
    until(request, handle, ws, cx, |ws| {
        asked(ws.space.terminal(id).unwrap().term.shared().modes())
    });
}

/// What `cat` shows: it echoes ESC as "^[".
#[cfg(unix)]
fn screen_of(ws: &Workspace, id: BlockId) -> String {
    let mut snap = templum_terminal::Snapshot::default();
    snap.refresh(
        ws.space.terminal(id).unwrap().term.shared(),
        &ws.theme.palette,
    );
    snap.text()
}

/// A program that asked for the mouse (SGR, with drags) hears a press, the drag and the
/// release, and the wheel; no text gets selected.
#[cfg(unix)]
#[gpui_kit::test]
fn a_program_that_asked_for_the_mouse_hears_it(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    program_asks("\x1b[?1002h\x1b[?1006h", handle, &ws, cx, |m| m.mouse);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let body = window.find(("body", id.0 as usize)).bounds();
        let from = point(body.origin.x + px(20.), body.origin.y + px(20.));
        window.drag(from, point(from.x + px(80.), from.y), cx);
        let at = (f32::from(from.x), f32::from(from.y));
        wheel(window, cx, at, ScrollDelta::Lines(point(0., 1.)), false);
    })
    .unwrap();
    until("the reports", handle, &ws, cx, |ws| {
        screen_of(ws, id).contains("^[[<64;")
    });
    ws.read_with(cx, |ws, _| {
        let shown = screen_of(ws, id);
        assert_eq!(
            shown.matches("^[[<0;").count(),
            2,
            "press, release: {shown:?}"
        );
        assert!(shown.contains("^[[<32;"), "the drag: {shown:?}");
        let pane = ws.space.terminal(id).unwrap();
        assert_eq!(pane.term.shared().selection_text(), None);
        assert!(ws.drag.is_none());
    });
}

/// With the kitty keyboard on, Shift+Enter says so itself, in an agent too (Claude Code makes it
/// a new line).
#[cfg(unix)]
#[gpui_kit::test]
fn shift_enter_in_the_kitty_keyboard(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    program_asks("\x1b[>1u", handle, &ws, cx, |m| m.kitty_keys);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().seen.agent = Some("claude".into())
    });
    cx.update_window(handle, |_, window, cx| window.press("shift-enter", cx))
        .unwrap();
    until("Shift+Enter", handle, &ws, cx, |ws| {
        screen_of(ws, id).contains("^[[13;2u")
    });
}

/// A program full screen gets the keys of the shortcuts that yield to it: Ctrl+- (Claude Code's
/// undo) reaches it, the canvas doesn't zoom. New terminal stays Templum's.
#[cfg(unix)]
#[gpui_kit::test]
fn a_program_full_screen_gets_the_shortcuts_that_yield(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    program_asks("\x1b[?1049h", handle, &ws, cx, |m| m.full_screen);
    let zoom = ws.read_with(cx, |ws, _| ws.space.canvas.camera.zoom());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("ctrl--", cx);
        window.press(NEW, cx);
    })
    .unwrap();
    until("Ctrl+-", handle, &ws, cx, |ws| {
        screen_of(ws, id).contains("^_")
    });
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.canvas.camera.zoom(), zoom);
        assert_eq!(ws.space.canvas.blocks().len(), 2, "a new terminal");
    });
}
