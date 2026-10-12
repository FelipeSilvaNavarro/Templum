//! Reloading the config file after it was edited by hand.

use super::*;

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
fn a_theme_added_after_startup_can_be_set_in_the_config(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    std::fs::create_dir(dir.path().join("themes")).unwrap();
    std::fs::write(
        dir.path().join("themes").join("mine.toml"),
        include_str!("../../../themes/one-dark.toml"),
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
