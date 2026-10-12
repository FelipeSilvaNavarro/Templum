//! Tests of the workspace, and the helpers they share.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Entity, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, PlatformInput, ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase, Window, WindowBounds, WindowOptions, point, px, size,
};
use templum_canvas::{BlockId, Camera, Point, Rect, chrome};

use crate::config::Edit;

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

#[cfg(target_os = "macos")]
const NEW: &str = "cmd-t";
#[cfg(not(target_os = "macos"))]
const NEW: &str = "ctrl-shift-t";
#[cfg(target_os = "macos")]
const CLOSE: &str = "cmd-w";
#[cfg(not(target_os = "macos"))]
const CLOSE: &str = "ctrl-shift-w";

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

fn first_id(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> templum_canvas::BlockId {
    ws.read_with(cx, |ws, _| ws.space.canvas.top().unwrap())
}

#[cfg(target_os = "macos")]
const ZOOM_IN: &str = "cmd-=";
#[cfg(not(target_os = "macos"))]
const ZOOM_IN: &str = "ctrl-=";

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

fn builtin_index(name: &str) -> usize {
    crate::theme::BUILTIN
        .iter()
        .position(|b| b.name == name)
        .unwrap()
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

#[cfg(target_os = "macos")]
const PICKER: &str = "cmd-shift-p";
#[cfg(not(target_os = "macos"))]
const PICKER: &str = "ctrl-shift-p";

fn frame(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
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

mod agents;
mod blocks;
mod config_reload;
mod drawing;
mod kanban;
mod menus;
mod navigation;
mod notes;
mod persistence;
mod settings_edits;
mod settings_panel;
mod shortcuts;
mod terminals;
mod themes;
mod workspaces;
