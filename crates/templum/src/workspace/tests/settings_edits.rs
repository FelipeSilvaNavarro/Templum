//! Settings edits: when they apply, when they are saved, and hand edits of the file.

use super::*;

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
