//! Editor blocks: the folder tree and opening files.

use super::*;

#[cfg(target_os = "macos")]
const NEW_EDITOR: &str = "cmd-shift-e";
#[cfg(not(target_os = "macos"))]
const NEW_EDITOR: &str = "ctrl-shift-e";

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
            ws.set_editor_folder(gone, folder.path(), window, cx);
        });
        window.render_frame(cx);
        window.find(("tree", gone.0 as usize));
    })
    .unwrap();
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
        // Open, Reveal in file manager, File history, Blame, Rename…, Delete….
        window.within("popup-menu").click(5usize, cx);
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
        // The glide starts now, however long opening the file took on a slow machine: the frame
        // below must catch it on the way.
        ws.update(cx, restart_glide);
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

#[gpui_kit::test]
fn the_editor_menu_points_it_at_another_folder(cx: &mut TestAppContext) {
    no_motion(cx);
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("b.txt"), "b").unwrap();
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    ws.update(cx, |ws, _| {
        ws.space.recent = vec![other.path().to_path_buf()]
    });
    cx.update_window(handle, |_, window, cx| {
        window.right_click(("title", id.0 as usize), cx);
        window.render_frame(cx);
        // Change folder…, the tree, soft wrap, —, Close.
        window.within("popup-menu").click(0usize, cx);
    })
    .unwrap();
    // The menu fades out for a moment: a click meanwhile would land on it.
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("recent-0", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("choose-folder").is_none(),
            "the dialog closed"
        );
        // A path that isn't a folder changes nothing.
        let missing = other.path().join("missing");
        ws.update(cx, |ws, cx| {
            assert!(ws.set_editor_folder(id, &missing, window, cx).is_none())
        });
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.editor(id).unwrap().folder, other.path())
    });
}

#[cfg(target_os = "macos")]
const OPEN_FILE: &str = "cmd-p";
#[cfg(not(target_os = "macos"))]
const OPEN_FILE: &str = "ctrl-p";

#[gpui_kit::test]
fn ctrl_p_opens_a_file_by_name(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    for f in ["src/lib.rs", "src/main.rs", "README.md"] {
        std::fs::write(dir.path().join(f), "").unwrap();
    }
    let (handle, ws) = open(cx);
    let id = open_editor(handle, &ws, cx, dir.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.focus_block(id, window, cx));
        window.press(OPEN_FILE, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "the picker");
    })
    .unwrap();
    cx.simulate_input(handle, "lib");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    // The palette confirms once its own update is over.
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "closed");
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let tab = ws.space.editor(id).unwrap().active_tab().unwrap();
        assert_eq!(tab.path, dir.path().join("src/lib.rs"));
    });
}

/// Ctrl+P is the editor's only while one has the focus: a terminal gets it otherwise.
#[gpui_kit::test]
fn ctrl_p_reaches_a_focused_terminal(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    let (handle, _ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(OPEN_FILE, cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
    })
    .unwrap();
}
