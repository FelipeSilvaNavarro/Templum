//! GPUI feasibility spike (throwaway): an infinite canvas of terminal blocks.
//!
//! `templum [--terminals N]` interactive; `--bench` stress + frame timing JSON; `--shot` zoom captures.

mod term;
mod view;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use gpui::{App, AppContext, AsyncApp, Bounds, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions, px, size};

use crate::view::{Board, Camera, PAINT_US, ZOOM_STEPS};

enum Mode {
    Normal,
    Bench,
    Shot { cmd: String, tag: String },
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let mode = if args.iter().any(|a| a == "--bench") {
        Mode::Bench
    } else if args.iter().any(|a| a == "--shot") {
        Mode::Shot {
            // --color=always: with --color=auto, ls emits no color into the pipe to head.
            cmd: arg("--cmd").unwrap_or_else(|| "ls --color=always /usr/bin | head -40".into()),
            tag: arg("--tag").map(|t| format!("{t}-")).unwrap_or_default(),
        }
    } else {
        Mode::Normal
    };
    let (count, grid) = match mode {
        Mode::Bench => (20, (80, 24)),
        Mode::Shot { .. } => (1, (100, 44)),
        Mode::Normal => (arg("--terminals").and_then(|n| n.parse().ok()).unwrap_or(4), (80, 24)),
    };

    gpui_kit::application().run(move |cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1600.0), px(950.0)), cx))),
            titlebar: Some(TitlebarOptions { title: Some("Templum".into()), ..Default::default() }),
            app_id: Some("templum".into()),
            // Bench must not be throttled to 30 fps if the window loses focus.
            inactive_frame_interval: if matches!(mode, Mode::Bench) { None } else { Some(Duration::from_micros(33_333)) },
            ..Default::default()
        };
        let window = cx
            .open_window(options, |window, cx| cx.new(|cx| Board::new(count, grid, window, cx)))
            .expect("failed to open window");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        match mode {
            Mode::Normal => {}
            Mode::Bench => bench(window, cx),
            Mode::Shot { cmd, tag } => shot(window, cmd, tag, cx),
        }
        cx.activate(true);
    });
}

async fn sleep(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
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
    let fields: Vec<&str> = stat.rsplit_once(") ").map(|(_, f)| f.split(' ').collect()).unwrap_or_default();
    let ticks = |i: usize| fields.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
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

/// One terminal running `cmd`, captured at zoom 0.5, 1 and 2 with grim, window region only.
fn shot(window: WindowHandle<Board>, cmd: String, tag: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        sleep(cx, 1000).await;
        let _ = window.update(cx, |board, _, _| board.blocks[0].term.write(format!("{cmd}\r").into_bytes()));
        sleep(cx, 2000).await;
        for zoom in [0.5, 1.0, 2.0] {
            let _ = window.update(cx, |board, _, cx| {
                let zoom_idx = ZOOM_STEPS.iter().position(|&z| z == zoom).expect("zoom is a step");
                let b = &board.blocks[0];
                board.cam = Camera { x: b.x - 16.0 / zoom, y: b.y - 16.0 / zoom, zoom_idx };
                cx.notify();
            });
            sleep(cx, 700).await;
            let path = format!("/tmp/templum-{tag}zoom-{zoom}.png");
            // Geometry of *this* process' window only (matched by pid), never the full screen.
            let script = format!(
                r#"g=$(hyprctl clients -j | jq -r '.[] | select(.pid == {pid}) | "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"' | head -1); [ -n "$g" ] && grim -g "$g" {path}"#,
                pid = std::process::id(),
            );
            let ok = std::process::Command::new("sh").arg("-c").arg(script).status().is_ok_and(|s| s.success());
            println!("{} {path}", if ok { "captured" } else { "FAILED" });
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}
