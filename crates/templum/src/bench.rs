//! `templum --bench`: 20 terminals, 5 running `yes`, 200 strokes over them, 10 s of animated pan
//! and zoom. `templum --bench-mixed`: a canvas like an agent cockpit instead — 5 terminals
//! redrawing a colored screen 10× a second (an agent's TUI), 3 editors, 3 Git blocks, a note and
//! 2 kanban blocks, no strokes.
//! Prints one JSON line (frame-interval percentiles, memory), then quits. Bar: p95 <= 16.7 ms.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Entity};
use templum_canvas::{Color, Drawing, Point, Rect, ZOOM_STEPS, chrome};

use crate::workspace::{DEFAULT_COLS, DEFAULT_ROWS, Workspace};

/// Typed into the terminals of the mixed scenario: 24 colored lines redrawn 10× a second,
/// each with a number that changes, like an agent's interface.
const TUI_LOOP: &[u8] = b"while :; do printf '\\033[H'; for i in $(seq 24); do printf '\\033[3%dm%5d The quick brown fox jumps over the lazy dog 0123456789 abcdefghijkl\\033[0m\\n' $((i%7+1)) $RANDOM; done; sleep 0.1; done\r";

pub(crate) fn run(window: AnyWindowHandle, ws: Entity<Workspace>, mixed: bool, cx: &mut App) {
    let _ = cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, ws.cell.size);
            if mixed {
                // The folder Templum runs from: a real one for the editors and Git blocks.
                let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let mut slots =
                    (1..).map(|i| Point::new((i % 5) as f32 * 950.0, (i / 5) as f32 * 750.0));
                let mut next = || slots.next().expect("endless");
                for _ in 0..4 {
                    let p = next();
                    ws.open(Rect::new(p.x, p.y, size.w, size.h), window, cx);
                }
                for _ in 0..3 {
                    ws.add_editor_at(&repo, next(), window, cx);
                }
                for _ in 0..3 {
                    ws.add_git_at(&repo, next(), window, cx);
                }
                ws.add_note_at(next(), window, cx);
                for _ in 0..2 {
                    ws.add_kanban_at(next(), window, cx);
                }
                return;
            }
            for i in 1..20 {
                let (col, row) = ((i % 5) as f32, (i / 5) as f32);
                let rect = Rect::new(col * (size.w + 40.0), row * (size.h + 40.0), size.w, size.h);
                ws.open(rect, window, cx);
            }
            // 200 strokes of 50 points over the terminals: the spec's bar must hold with ink on
            // screen.
            for s in 0..200 {
                let x0 = (s % 20) as f32 * 150.0;
                let y0 = (s / 20) as f32 * 120.0;
                let points = (0..50)
                    .map(|i| {
                        let t = i as f32;
                        Point::new(x0 + t * 3.0, y0 + (t * 0.4).sin() * 20.0)
                    })
                    .collect();
                ws.space.ink.push(Drawing::Stroke {
                    points,
                    width: 4.0,
                    color: Color::Blue,
                    highlight: s % 4 == 0,
                });
            }
        })
    });
    cx.spawn(async move |cx| {
        // Let the shells start before measuring idle memory.
        cx.background_executor()
            .timer(Duration::from_secs(3))
            .await;
        let idle_rss = rss_mb();
        let _ = cx.update_window(window, |_, _, cx| {
            ws.update(cx, |ws, cx| {
                let command: &[u8] = if mixed { TUI_LOOP } else { b"yes\r" };
                for (_, pane) in ws.space.terminals().take(5) {
                    pane.term.write(command);
                }
                let center = ws
                    .space.canvas
                    .bounds()
                    .map(|b| Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0))
                    .unwrap_or_default();
                // The bench drives the camera: stop keeping the startup terminal centered.
                ws.space.pristine = false;
                let start = Instant::now();
                let mut last: Option<Instant> = None;
                let mut intervals: Vec<f64> = Vec::new();
                ws.on_frame = Some(Box::new(move |ws, window, cx| {
                    let now = Instant::now();
                    if let Some(prev) = last {
                        intervals.push((now - prev).as_secs_f64() * 1000.0);
                    }
                    last = Some(now);
                    let t = (now - start).as_secs_f32();
                    if t >= 10.0 {
                        intervals.sort_by(f64::total_cmp);
                        let p = |q: f64| percentile(&intervals, q);
                        let json =
                            |v: Option<f64>| v.map_or("null".to_string(), |v| format!("{v:.1}"));
                        println!(
                            r#"{{"frames":{},"p50_ms":{:.2},"p95_ms":{:.2},"p99_ms":{:.2},"max_ms":{:.2},"rss_mb":{},"idle_rss_mb":{}}}"#,
                            intervals.len() + 1,
                            p(50.0),
                            p(95.0),
                            p(99.0),
                            intervals.last().copied().unwrap_or(0.0),
                            json(rss_mb()),
                            json(idle_rss),
                        );
                        cx.quit();
                        return false;
                    }
                    // Zoom sweeps every step back and forth (4 per second) while panning in a circle.
                    let n = ZOOM_STEPS.len();
                    let k = (t / 0.25) as usize % (2 * (n - 1));
                    let target = if k < n { k } else { 2 * (n - 1) - k };
                    // Through the workspace, as the keys and the wheel do: zoom steps glide.
                    let step = target as i32 - ws.space.canvas.camera.zoom_index() as i32;
                    if step != 0 {
                        ws.zoom_to(step, Point::default(), cx);
                    }
                    let cam = ws.space.canvas.camera;
                    let z = cam.zoom();
                    let a = t * std::f32::consts::TAU / 4.0;
                    let vp = window.viewport_size();
                    let want = Point::new(
                        center.x + 400.0 * a.cos() - f32::from(vp.width) / 2.0 / z,
                        center.y + 400.0 * a.sin() - f32::from(vp.height) / 2.0 / z,
                    );
                    ws.pan_by(Point::new(
                        (cam.origin.x - want.x) * z,
                        (cam.origin.y - want.y) * z,
                    ));
                    window.request_animation_frame();
                    true
                }));
                cx.notify();
            })
        });
    })
    .detach();
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

/// Resident memory in MB (Linux only).
fn rss_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: f64 = status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb / 1024.0)
}
