//! GPUI feasibility spike (throwaway): an infinite canvas of terminal blocks.
//!
//! `templum [--terminals N]` interactive; `--bench` stress + frame timing JSON; `--shot` zoom
//! captures; `--selftest` synthetic mouse/keyboard checks.

mod term;
mod view;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, AsyncApp, Bounds, Entity, Keystroke, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PlatformInput, ScrollDelta, ScrollWheelEvent, TitlebarOptions,
    TouchPhase, Window, WindowBounds, WindowHandle, WindowOptions, point, px, size,
};

use crate::view::{Board, Camera, PAINT_US, ZOOM_STEPS};

enum Mode {
    Normal,
    Bench,
    Shot { cmd: String, tag: String },
    SelfTest,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let mode = if args.iter().any(|a| a == "--bench") {
        Mode::Bench
    } else if args.iter().any(|a| a == "--shot") {
        Mode::Shot {
            // --color=always: with --color=auto, ls emits no color into the pipe to head.
            cmd: arg("--cmd").unwrap_or_else(|| "ls --color=always /usr/bin | head -40".into()),
            tag: arg("--tag").map(|t| format!("{t}-")).unwrap_or_default(),
        }
    } else if args.iter().any(|a| a == "--selftest") {
        Mode::SelfTest
    } else {
        Mode::Normal
    };
    let (count, grid) = match mode {
        Mode::Bench => (20, (80, 24)),
        Mode::Shot { .. } => (1, (100, 44)),
        Mode::SelfTest => (2, (80, 24)),
        Mode::Normal => (
            arg("--terminals").and_then(|n| n.parse().ok()).unwrap_or(4),
            (80, 24),
        ),
    };

    gpui_kit::application().run(move |cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1600.0), px(950.0)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Templum".into()),
                ..Default::default()
            }),
            app_id: Some("templum".into()),
            // Bench must not be throttled to 30 fps if the window loses focus.
            inactive_frame_interval: if matches!(mode, Mode::Bench) {
                None
            } else {
                Some(Duration::from_micros(33_333))
            },
            ..Default::default()
        };
        let window = cx
            .open_window(options, |window, cx| {
                cx.new(|cx| Board::new(count, grid, window, cx))
            })
            .expect("failed to open window");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        match mode {
            Mode::Normal => {}
            Mode::Bench => bench(window, cx),
            Mode::Shot { cmd, tag } => shot(window, cmd, tag, cx),
            Mode::SelfTest => selftest(window, cx),
        }
        cx.activate(true);
    });
}

async fn sleep(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor()
        .timer(Duration::from_millis(ms))
        .await;
}

fn rss_mb() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kb = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    (kb / 1024.0 * 10.0).round() / 10.0
}

/// CPU time (user + system) in ms of a /proc stat file; 100 ticks/s (Linux default CLK_TCK).
fn cpu_ms(stat: &str) -> f64 {
    let stat = std::fs::read_to_string(stat).unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit_once(") ")
        .map(|(_, f)| f.split(' ').collect())
        .unwrap_or_default();
    let ticks = |i: usize| {
        fields
            .get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) * 10.0
}

/// Nearest-rank percentile of sorted values.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((p / 100.0) * (sorted.len() - 1) as f64).round() as usize]
}

/// 20 terminals, 5 running `yes`, camera circling and zooming through every step for 10 s.
fn bench(window: WindowHandle<Board>, cx: &mut App) {
    cx.spawn(async move |cx| {
        // Let the shells start and the first frames settle before measuring idle memory.
        sleep(cx, 3000).await;
        let idle_rss = rss_mb();
        // Knobs to split the cost: BENCH_NO_YES=1, BENCH_ZOOM_IDX=<step index> (pan only).
        let no_yes = std::env::var_os("BENCH_NO_YES").is_some();
        let fixed_zoom: Option<usize> = std::env::var("BENCH_ZOOM_IDX").ok().and_then(|v| v.parse().ok());
        let _ = window.update(cx, |board, _, cx| {
            for b in board.blocks.iter().take(if no_yes { 0 } else { 5 }) {
                b.term.write(&b"yes\r"[..]);
            }
            let (cx0, cy0) = board.world_center();
            // The UI thread is the main thread, whose tid is the pid.
            let main_stat = format!("/proc/self/task/{}/stat", std::process::id());
            let (main_cpu0, proc_cpu0) = (cpu_ms(&main_stat), cpu_ms("/proc/self/stat"));
            let start = Instant::now();
            let mut last: Option<Instant> = None;
            let mut intervals: Vec<f64> = Vec::new();
            let mut paints: Vec<f64> = Vec::new();
            board.on_frame = Some(Box::new(move |board, window, cx| {
                let now = Instant::now();
                if let Some(prev) = last {
                    intervals.push((now - prev).as_secs_f64() * 1000.0);
                    paints.push(PAINT_US.load(Ordering::Relaxed) as f64 / 1000.0);
                }
                last = Some(now);
                let t = (now - start).as_secs_f32();
                if t >= 10.0 {
                    intervals.sort_by(f64::total_cmp);
                    paints.sort_by(f64::total_cmp);
                    let vp = window.viewport_size();
                    let frames = intervals.len() + 1;
                    let main_cpu = cpu_ms(&main_stat) - main_cpu0;
                    let proc_cpu = cpu_ms("/proc/self/stat") - proc_cpu0;
                    println!(
                        r#"{{"frames":{},"p50_ms":{:.2},"p95_ms":{:.2},"p99_ms":{:.2},"max_ms":{:.2},"rss_mb":{},"idle_rss_mb":{},"paint_p50_ms":{:.2},"paint_p95_ms":{:.2},"ui_thread_cpu_ms_per_frame":{:.2},"process_cpu_cores":{:.2},"viewport":[{},{}]}}"#,
                        frames,
                        percentile(&intervals, 50.0),
                        percentile(&intervals, 95.0),
                        percentile(&intervals, 99.0),
                        intervals.last().copied().unwrap_or(0.0),
                        rss_mb(),
                        idle_rss,
                        percentile(&paints, 50.0),
                        percentile(&paints, 95.0),
                        main_cpu / frames as f64,
                        proc_cpu / (now - start).as_secs_f64() / 1000.0,
                        f32::from(vp.width),
                        f32::from(vp.height),
                    );
                    cx.quit();
                    return false;
                }
                let n = ZOOM_STEPS.len();
                let k = (t / 0.25) as usize % (2 * (n - 1));
                let zoom_idx = match fixed_zoom {
                    Some(idx) => idx,
                    None if k < n => k,
                    None => 2 * (n - 1) - k,
                };
                let z = ZOOM_STEPS[zoom_idx];
                let a = t * std::f32::consts::TAU / 4.0;
                let vp = window.viewport_size();
                board.cam = Camera {
                    x: cx0 + 400.0 * a.cos() - f32::from(vp.width) / 2.0 / z,
                    y: cy0 + 400.0 * a.sin() - f32::from(vp.height) / 2.0 / z,
                    zoom_idx,
                };
                window.request_animation_frame();
                true
            }));
            cx.notify();
        });
    })
    .detach();
}

fn mouse(window: &mut Window, cx: &mut App, kind: &str, (x, y): (f32, f32), modifiers: Modifiers) {
    let position = point(px(x), px(y));
    let button = MouseButton::Left;
    let event = match kind {
        "down" => PlatformInput::MouseDown(MouseDownEvent {
            button,
            position,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }),
        "move" => PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: Some(button),
            modifiers,
        }),
        "up" => PlatformInput::MouseUp(MouseUpEvent {
            button,
            position,
            modifiers,
            click_count: 1,
        }),
        _ => PlatformInput::ScrollWheel(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Lines(point(0.0, 1.0)),
            modifiers,
            touch_phase: TouchPhase::Moved,
        }),
    };
    window.dispatch_event(event, cx);
}

fn drag(window: &mut Window, cx: &mut App, from: (f32, f32), by: (f32, f32)) {
    let to = (from.0 + by.0, from.1 + by.1);
    mouse(window, cx, "down", from, Modifiers::default());
    mouse(
        window,
        cx,
        "move",
        ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0),
        Modifiers::default(),
    );
    mouse(window, cx, "move", to, Modifiers::default());
    mouse(window, cx, "up", to, Modifiers::default());
}

/// Model state copied out of the board (it cannot stay borrowed while events are dispatched).
struct Probe {
    cam: Camera,
    /// (id, x, y, w, h, cols, rows), back to front.
    blocks: Vec<(usize, f32, f32, f32, f32, u16, u16)>,
}

impl Probe {
    fn of(board: &Entity<Board>, cx: &App) -> Self {
        let board = board.read(cx);
        Self {
            cam: board.cam,
            blocks: board
                .blocks
                .iter()
                .map(|b| (b.id, b.x, b.y, b.w, b.h, b.term.cols, b.term.rows))
                .collect(),
        }
    }

    fn block(&self, id: usize) -> (usize, f32, f32, f32, f32, u16, u16) {
        *self.blocks.iter().find(|b| b.0 == id).expect("block")
    }

    fn screen(&self, wx: f32, wy: f32) -> (f32, f32) {
        (
            (wx - self.cam.x) * self.cam.zoom(),
            (wy - self.cam.y) * self.cam.zoom(),
        )
    }
}

/// Drives move / resize / pan / ctrl+scroll zoom / click-focus / typing through GPUI's real
/// hit-testing and dispatch (synthetic events, no OS pointer), checks the model, captures a shot.
fn selftest(handle: WindowHandle<Board>, cx: &mut App) {
    cx.spawn(async move |cx| {
        sleep(cx, 1500).await;
        let board = handle.entity(cx).expect("board");
        let mut report = Vec::new();
        let _ = cx.update_window(handle.into(), |_, window, cx| {
            // Move: drag block 1 by its title bar.
            let p = Probe::of(&board, cx);
            let (_, x0, y0, ..) = p.block(1);
            drag(window, cx, p.screen(x0 + 60.0, y0 + 10.0), (60.0, 300.0));
            let (_, x1, y1, ..) = Probe::of(&board, cx).block(1);
            report.push(format!(
                "move: block1 ({x0},{y0}) -> ({x1},{y1}) ok={}",
                (x1 - x0, y1 - y0) == (60.0, 300.0)
            ));

            // Resize: drag block 0's bottom-right handle inwards; the PTY gets the new size.
            let p = Probe::of(&board, cx);
            let (_, x, y, w, h, cols, rows) = p.block(0);
            let corner = p.screen(x + w - 4.0, y + h - 4.0);
            drag(window, cx, corner, (-200.0, -150.0));
            let (.., cols1, rows1) = Probe::of(&board, cx).block(0);
            report.push(format!(
                "resize: {cols}x{rows} -> {cols1}x{rows1} ok={}",
                cols1 < cols && rows1 < rows
            ));

            // Pan: drag empty background.
            let before = Probe::of(&board, cx).cam;
            let vp = window.viewport_size();
            drag(
                window,
                cx,
                (f32::from(vp.width) - 10.0, f32::from(vp.height) - 10.0),
                (-100.0, -50.0),
            );
            let after = Probe::of(&board, cx).cam;
            report.push(format!(
                "pan: ({},{}) -> ({},{}) ok={}",
                before.x,
                before.y,
                after.x,
                after.y,
                (after.x - before.x, after.y - before.y) == (100.0, 50.0)
            ));

            // Zoom: ctrl+scroll; the world point under the pointer must stay put.
            let at = (300.0, 200.0);
            let world = |c: Camera| (at.0 / c.zoom() + c.x, at.1 / c.zoom() + c.y);
            let before = after;
            mouse(
                window,
                cx,
                "scroll",
                at,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            );
            let after = Probe::of(&board, cx).cam;
            let (w0, w1) = (world(before), world(after));
            let anchored = (w0.0 - w1.0).abs() < 0.01 && (w0.1 - w1.1).abs() < 0.01;
            report.push(format!(
                "zoom: {} -> {} anchored={anchored}",
                before.zoom(),
                after.zoom()
            ));
        });
        sleep(cx, 300).await;
        let _ = cx.update_window(handle.into(), |_, window, cx| {
            // Focus: click block 0's body (block 1 is in front), then type.
            let p = Probe::of(&board, cx);
            let (_, x, y, ..) = p.block(0);
            let body = p.screen(x + 40.0, y + 60.0);
            mouse(window, cx, "down", body, Modifiers::default());
            mouse(window, cx, "up", body, Modifiers::default());
            report.push(format!(
                "focus: front block = {}",
                Probe::of(&board, cx).blocks.last().map_or(99, |b| b.0)
            ));
        });
        // Printable keys go through the text-input handler, the rest through on_key_down.
        // Expected: "15 56" (stty size), "Ok çá Xãé" (arrows), "^C", "abc" (backspace), "/usr/share/" (tab).
        for keys in [
            // Plain bash: the user's interactive shell (ble.sh) redraws too slowly for 400 ms steps.
            "e x e c space b a s h space - - n o r c enter",
            "s t t y space s i z e enter",
            "e c h o space shift-o k space ç á space ã é left left shift-x enter",
            "s l e e p space 9 enter",
            "ctrl-c",
            "e c h o space a b c d backspace enter",
            "e c h o space / u s r / s h a r tab",
            "enter",
        ] {
            let _ = cx.update_window(handle.into(), |_, window, cx| {
                for k in keys.split(' ') {
                    window.dispatch_keystroke(Keystroke::parse(k).expect("keystroke"), cx);
                }
            });
            sleep(cx, 400).await;
        }
        board.update(cx, |board, cx| {
            let b = board.blocks.iter().find(|b| b.id == 0).expect("block 0");
            board.cam = Camera {
                x: b.x - 16.0,
                y: b.y - 16.0,
                zoom_idx: ZOOM_STEPS.iter().position(|&z| z == 1.0).expect("1.0"),
            };
            cx.notify();
        });
        sleep(cx, 500).await;
        capture("/tmp/templum-selftest.png");
        for line in report {
            println!("{line}");
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

fn capture(path: &str) {
    // Geometry of *this* process' window only (matched by pid), never the full screen.
    let script = format!(
        r#"g=$(hyprctl clients -j | jq -r '.[] | select(.pid == {pid}) | "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"' | head -1); [ -n "$g" ] && grim -g "$g" {path}"#,
        pid = std::process::id(),
    );
    let ok = std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .status()
        .is_ok_and(|s| s.success());
    println!("{} {path}", if ok { "captured" } else { "FAILED" });
}

/// One terminal running `cmd`, captured at zoom 0.5, 1 and 2 with grim, window region only.
fn shot(window: WindowHandle<Board>, cmd: String, tag: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        sleep(cx, 1000).await;
        let _ = window.update(cx, |board, _, _| {
            board.blocks[0].term.write(format!("{cmd}\r").into_bytes())
        });
        sleep(cx, 2000).await;
        for zoom in [0.5, 1.0, 2.0] {
            let _ = window.update(cx, |board, _, cx| {
                let zoom_idx = ZOOM_STEPS
                    .iter()
                    .position(|&z| z == zoom)
                    .expect("zoom is a step");
                let b = &board.blocks[0];
                board.cam = Camera {
                    x: b.x - 16.0 / zoom,
                    y: b.y - 16.0 / zoom,
                    zoom_idx,
                };
                cx.notify();
            });
            sleep(cx, 700).await;
            capture(&format!("/tmp/templum-{tag}zoom-{zoom}.png"));
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}
