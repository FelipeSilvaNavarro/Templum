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

/// Restarts the camera's glide from now, so a test's next frame draws it on the way.
fn restart_glide(ws: &mut Workspace, _: &mut Context<Workspace>) {
    if let Some((_, since)) = &mut ws.space.flight {
        *since = std::time::Instant::now();
    }
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

/// Clicks the tree's row `ix` (a file opens, a folder expands).
fn click_file(handle: AnyWindowHandle, ix: usize, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        window.click(("entry", ix), cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// Dialogs slide in over 250 ms of real time: a click mid-slide can miss its button.
fn no_motion(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

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

/// Waits for the block's next status read (a change made behind its back).
fn settle_read(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: BlockId,
) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.refresh_git(id, window, cx))
    })
    .unwrap();
    settle(handle, cx);
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
                // The size the tests were written at (a new block is 1200 × 760 now): their rows
                // and buttons stay inside the window.
                ws.space.canvas.get_mut(id).unwrap().rect = Rect::new(40.0, 40.0, 800.0, 560.0);
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

/// A repository with one commit holding a.txt ("one\n").
fn committed_repo() -> tempfile::TempDir {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "First"]);
    repo
}

/// a.txt of 30 lines committed, then lines 2, 15 and 28 changed: three hunks.
fn three_hunk_repo() -> tempfile::TempDir {
    let repo = git_repo();
    let lines: Vec<String> = (1..=30).map(|i| format!("line {i}\n")).collect();
    std::fs::write(repo.path().join("a.txt"), lines.concat()).unwrap();
    git_run(repo.path(), &["add", "a.txt"]);
    git_run(repo.path(), &["commit", "-q", "-m", "a"]);
    let mut changed = lines;
    for i in [1, 14, 27] {
        changed[i] = format!("LINE {}\n", i + 1);
    }
    std::fs::write(repo.path().join("a.txt"), changed.concat()).unwrap();
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

mod agents;
mod blocks;
mod config_reload;
mod drawing;
mod editor;
mod editor_tabs;
mod git_changes;
mod git_commit;
mod git_compare;
mod git_history;
mod git_hunks;
mod git_keys;
mod git_layout;
mod git_panel;
mod git_status;
mod git_switch;
mod git_sync;
mod git_trash;
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
