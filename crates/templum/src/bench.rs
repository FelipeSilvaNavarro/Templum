//! `templum --bench`: 20 terminals, 5 running `yes`, 200 strokes over them, 10 s of animated pan
//! and zoom. `templum --bench-mixed`: a canvas like an agent cockpit instead — 5 terminals
//! redrawing a colored screen 10× a second (an agent's TUI), 3 editors, 3 Git blocks, a note and
//! 2 kanban blocks, each wired to the next, no strokes.
//! `templum --bench-zoom=<kind>`: 10 blocks of one kind (`terminal`, `editor`, `note`, `kanban`,
//! `git`) or none (`empty`), on the folder Templum runs from; the zoom glides step by step from
//! 25% to 200% and back, 4 steps a second, around the view's center. No pan, no output.
//! `templum --bench-git`: one Git block on a repository made in the temp folder, 5 files with
//! 1,000 changed lines each (a 5,000-line diff); the panel scrolls 400 rows a second while the
//! zoom glides between 50% and 200%, Unified for 5 s then Split. Bar: p95 <= 16.7 ms.
//! `templum --bench-far`: an agent cockpit seen from afar, on the folder Templum runs from: 24
//! blocks, 6 a row (8 terminals, 5 redrawing the mixed scenario's colored screen and 3 showing
//! its largest file; 6 editors on its 6 largest files; 4 notes holding its README; 3 Git blocks;
//! 3 kanban blocks), each wired to the next, none focused. The camera rests at 33% for 5 s, then glides to 25% and rests
//! there, while it circles 1,500 world units around the canvas's center.
//! Prints one JSON line (frame-interval percentiles, memory), then quits. Bar: p95 <= 16.7 ms.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Context, Entity, Window};
use templum_canvas::{Camera, Color, Drawing, Point, Rect, ZOOM_STEPS, chrome};

use crate::board::Boards;
use crate::workspace::{DEFAULT_COLS, DEFAULT_ROWS, Workspace};
use crate::{editor, git_block, kanban, note};

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
                wire_in_a_chain(ws);
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
        cx.background_executor().timer(Duration::from_secs(3)).await;
        let idle_rss = rss_mb();
        let _ = cx.update_window(window, |_, _, cx| {
            ws.update(cx, |ws, cx| {
                let command: &[u8] = if mixed { TUI_LOOP } else { b"yes\r" };
                for (_, pane) in ws.space.terminals().take(5) {
                    pane.term.write(command);
                }
                let center = ws
                    .space
                    .canvas
                    .bounds()
                    .map(|b| Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0))
                    .unwrap_or_default();
                // The bench drives the camera: stop keeping the startup terminal centered.
                ws.space.pristine = false;
                measure(ws, idle_rss, move |ws, window, cx, t| {
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
                });
                cx.notify();
            })
        });
    })
    .detach();
}

pub(crate) fn run_zoom(window: AnyWindowHandle, ws: Entity<Workspace>, kind: String, cx: &mut App) {
    let _ = cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            // Only the blocks measured: the startup terminal goes.
            let startup: Vec<_> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
            for id in startup {
                ws.close(id, window, cx);
            }
            let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let file = largest_files(&dir, 1).into_iter().next();
            let size = match kind.as_str() {
                "editor" => editor::SIZE,
                "note" => note::SIZE,
                "kanban" => kanban::SIZE,
                "git" => git_block::SIZE,
                _ => chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, ws.cell.size),
            };
            let readme = std::fs::read_to_string(dir.join("README.md")).unwrap_or_default();
            let board = (kind == "kanban").then(|| sample_board(&mut ws.boards));
            let count = if kind == "empty" { 0 } else { 10 };
            for i in 0..count {
                let at = Point::new(
                    (i % 5) as f32 * (size.w + 40.0),
                    (i / 5) as f32 * (size.h + 40.0),
                );
                match kind.as_str() {
                    "editor" => {
                        if let (Some(id), Some(file)) =
                            (ws.add_editor_at(&dir, at, window, cx), &file)
                        {
                            ws.open_file(id, file, None, window, cx);
                        }
                    }
                    "note" => {
                        ws.add_note_at(at, window, cx);
                        let id = ws.space.canvas.top().expect("just added");
                        if let Some(tab) = ws.space.blocks.get(&id).and_then(|b| b.note()?.tab()) {
                            tab.editor
                                .update(cx, |e, cx| e.set_value(readme.clone(), window, cx));
                        }
                    }
                    "kanban" => {
                        ws.add_kanban_at(at, window, cx);
                        let id = ws.space.canvas.top().expect("just added");
                        if let Some(k) = ws.space.kanban_mut(id) {
                            k.board = board;
                        }
                    }
                    "git" => {
                        ws.add_git_at(&dir, at, window, cx);
                    }
                    _ => {
                        ws.open(Rect::new(at.x, at.y, size.w, size.h), window, cx);
                        let id = ws.space.canvas.top().expect("just added");
                        if let (Some(pane), Some(file)) = (ws.space.terminal(id), &file) {
                            pane.term
                                .write(format!("cat '{}'\r", file.display()).into_bytes());
                        }
                    }
                }
            }
        })
    });
    cx.spawn(async move |cx| {
        // Let the shells start, the files open and the Git blocks read.
        cx.background_executor().timer(Duration::from_secs(3)).await;
        let idle_rss = rss_mb();
        let _ = cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.space.pristine = false;
                let center = ws
                    .space
                    .canvas
                    .bounds()
                    .map(|b| Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0))
                    .unwrap_or_default();
                let vp = ws.viewport(window);
                let low = ZOOM_STEPS[0];
                ws.space.canvas.camera = Camera::at(
                    Point::new(center.x - vp.w / 2.0 / low, center.y - vp.h / 2.0 / low),
                    low,
                );
                ws.space.flight = None;
                let high = ZOOM_STEPS
                    .iter()
                    .position(|&z| z == 2.0)
                    .expect("200% is a step");
                measure(ws, idle_rss, move |ws, window, cx, t| {
                    let k = (t / 0.25) as usize % (2 * high);
                    let target = if k <= high { k } else { 2 * high - k };
                    let step = target as i32 - ws.space.canvas.camera.zoom_index() as i32;
                    if step != 0 {
                        let pivot = ws.center(window);
                        ws.zoom_to(step, pivot, cx);
                    }
                });
                cx.notify();
            })
        });
    })
    .detach();
}

/// `--bench-far`: see the module's doc.
pub(crate) fn run_far(window: AnyWindowHandle, ws: Entity<Workspace>, cx: &mut App) {
    let tuis = cx
        .update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                // Only the blocks measured: the startup terminal goes.
                let startup: Vec<_> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
                for id in startup {
                    ws.close(id, window, cx);
                }
                let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let files = largest_files(&dir, 6);
                let readme = std::fs::read_to_string(dir.join("README.md")).unwrap_or_default();
                let board = sample_board(&mut ws.boards);
                let size = chrome::block_size(DEFAULT_COLS, DEFAULT_ROWS, ws.cell.size);
                // Six a row, in the mixed scenario's slots.
                let mut slots =
                    (0..).map(|i| Point::new((i % 6) as f32 * 950.0, (i / 6) as f32 * 750.0));
                let mut next = || slots.next().expect("endless");
                // 8 terminals: the first 5 get the TUI loop once their shells run, 3 show a file.
                let mut tuis = Vec::new();
                for i in 0..8 {
                    let p = next();
                    ws.open(Rect::new(p.x, p.y, size.w, size.h), window, cx);
                    let Some(id) = ws.space.canvas.top() else {
                        continue;
                    };
                    if i < 5 {
                        tuis.push(id);
                    } else if let (Some(pane), Some(file)) = (ws.space.terminal(id), files.first())
                    {
                        pane.term
                            .write(format!("cat '{}'\r", file.display()).into_bytes());
                    }
                }
                for i in 0..6 {
                    let at = next();
                    if let (Some(id), Some(file)) =
                        (ws.add_editor_at(&dir, at, window, cx), files.get(i))
                    {
                        ws.open_file(id, file, None, window, cx);
                    }
                }
                for _ in 0..4 {
                    ws.add_note_at(next(), window, cx);
                    let id = ws.space.canvas.top().expect("just added");
                    if let Some(tab) = ws.space.blocks.get(&id).and_then(|b| b.note()?.tab()) {
                        tab.editor
                            .update(cx, |e, cx| e.set_value(readme.clone(), window, cx));
                    }
                }
                for _ in 0..3 {
                    ws.add_git_at(&dir, next(), window, cx);
                }
                for _ in 0..3 {
                    ws.add_kanban_at(next(), window, cx);
                    let id = ws.space.canvas.top().expect("just added");
                    if let Some(k) = ws.space.kanban_mut(id) {
                        k.board = Some(board);
                    }
                }
                wire_in_a_chain(ws);
                // No block focused: the workspace keeps the keyboard, so every block is drawn
                // at its own level.
                ws.space.focused = None;
                window.focus(&ws.focus_handle, cx);
                tuis
            })
        })
        .unwrap_or_default();
    cx.spawn(async move |cx| {
        // Let the shells start, the files open and the Git blocks read.
        cx.background_executor().timer(Duration::from_secs(3)).await;
        let idle_rss = rss_mb();
        let _ = cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                for id in &tuis {
                    if let Some(pane) = ws.space.terminal(*id) {
                        pane.term.write(TUI_LOOP);
                    }
                }
                ws.space.pristine = false;
                let center = ws
                    .space
                    .canvas
                    .bounds()
                    .map(|b| Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0))
                    .unwrap_or_default();
                let vp = ws.viewport(window);
                let far = ZOOM_STEPS[1];
                ws.space.canvas.camera = Camera::at(
                    Point::new(center.x - vp.w / 2.0 / far, center.y - vp.h / 2.0 / far),
                    far,
                );
                ws.space.flight = None;
                measure(ws, idle_rss, move |ws, window, cx, t| {
                    // 33% for 5 s, then one glide out to 25%, which it keeps.
                    if t >= 5.0 && ws.space.canvas.camera.zoom_index() > 0 {
                        let pivot = ws.center(window);
                        ws.zoom_to(-1, pivot, cx);
                    }
                    // Around the canvas's center, 1,500 world units out, a turn every 4 s:
                    // blocks keep entering and leaving the view.
                    let cam = ws.space.canvas.camera;
                    let z = cam.zoom();
                    let a = t * std::f32::consts::TAU / 4.0;
                    let vp = ws.viewport(window);
                    let want = Point::new(
                        center.x + 1500.0 * a.cos() - vp.w / 2.0 / z,
                        center.y + 1500.0 * a.sin() - vp.h / 2.0 / z,
                    );
                    ws.pan_by(Point::new(
                        (cam.origin.x - want.x) * z,
                        (cam.origin.y - want.y) * z,
                    ));
                });
                cx.notify();
            })
        });
    })
    .detach();
}

/// `--bench-git`: see the module's doc.
pub(crate) fn run_git(window: AnyWindowHandle, ws: Entity<Workspace>, cx: &mut App) {
    let repo = match bench_repo() {
        Ok(repo) => repo,
        Err(e) => {
            eprintln!("templum: --bench-git: {e}");
            cx.quit();
            return;
        }
    };
    let _ = cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let startup: Vec<_> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
            for id in startup {
                ws.close(id, window, cx);
            }
            ws.add_git_at(&repo, Point::new(0.0, 0.0), window, cx);
        })
    });
    cx.spawn(async move |cx| {
        // Let the block find the repository and read its diff.
        cx.background_executor().timer(Duration::from_secs(3)).await;
        let idle_rss = rss_mb();
        let _ = cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.space.pristine = false;
                let id = ws.space.canvas.top().expect("the Git block");
                let rect = ws.space.canvas.get(id).expect("the block").rect;
                let vp = ws.viewport(window);
                ws.space.canvas.camera = Camera::at(
                    Point::new(
                        rect.x + rect.w / 2.0 - vp.w / 2.0,
                        rect.y + rect.h / 2.0 - vp.h / 2.0,
                    ),
                    1.0,
                );
                ws.space.flight = None;
                let step_of = |z: f32| ZOOM_STEPS.iter().position(|&s| s == z).expect("a step");
                let (low, high) = (step_of(0.5), step_of(2.0));
                measure(ws, idle_rss, move |ws, window, cx, t| {
                    // Unified for 5 s, then Split.
                    if t >= 5.0 && ws.space.git(id).is_some_and(|g| !g.split) {
                        ws.set_split(id, true, cx);
                    }
                    // The panel scrolls 400 rows a second, wrapping around.
                    if let Some(git) = ws.space.git(id) {
                        let rows = git.panel.rows.len().max(1);
                        git.panel.scroll.scroll_to_item(
                            (t * 400.0) as usize % rows,
                            gpui_kit::ScrollStrategy::Top,
                        );
                    }
                    // The zoom glides between 50% and 200%, 4 steps a second.
                    let span = high - low;
                    let k = (t / 0.25) as usize % (2 * span);
                    let target = low + if k <= span { k } else { 2 * span - k };
                    let step = target as i32 - ws.space.canvas.camera.zoom_index() as i32;
                    if step != 0 {
                        let pivot = ws.center(window);
                        ws.zoom_to(step, pivot, cx);
                    }
                });
                cx.notify();
            })
        });
    })
    .detach();
}

/// A repository in the temp folder: 5 files of 2,000 lines committed, then every 4th line of
/// each changed (500 removed and 500 added per file: a 5,000-line diff with context).
fn bench_repo() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("templum-bench-git");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
    let repo = templum_git::Repo::init("git", &dir).map_err(|e| e.to_string())?;
    for args in [
        ["config", "user.name", "Bench"].as_slice(),
        &["config", "user.email", "bench@example.com"],
        &["config", "commit.gpgsign", "false"],
    ] {
        repo.run(args).map_err(|e| e.to_string())?;
    }
    let names: Vec<String> = (0..5).map(|i| format!("src/file{i}.rs")).collect();
    let text = |changed: bool| -> String {
        (0..2000)
            .map(|i| {
                let value = if changed && i % 4 == 0 { i + 1 } else { i };
                format!("fn line_{i}() -> u32 {{ {value} }}\n")
            })
            .collect()
    };
    for name in &names {
        std::fs::write(dir.join(name), text(false)).map_err(|e| e.to_string())?;
    }
    let paths: Vec<&str> = names.iter().map(String::as_str).collect();
    repo.stage(&paths).map_err(|e| e.to_string())?;
    repo.commit("bench", false).map_err(|e| e.to_string())?;
    for name in &names {
        std::fs::write(dir.join(name), text(true)).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

/// From now on every frame calls `drive` with the seconds since now; after 10 s prints the frame
/// intervals' percentiles and the memory as one JSON line, and quits.
fn measure(
    ws: &mut Workspace,
    idle_rss: Option<f64>,
    mut drive: impl FnMut(&mut Workspace, &mut Window, &mut Context<Workspace>, f32) + 'static,
) {
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
            let json = |v: Option<f64>| v.map_or("null".to_string(), |v| format!("{v:.1}"));
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
        drive(ws, window, cx, t);
        window.request_animation_frame();
        true
    }));
}

/// The `n` largest Rust, Markdown or TOML files under `dir` (not in `target` or a hidden
/// folder), largest first: text for the editors and the terminals to show.
fn largest_files(dir: &Path, n: usize) -> Vec<PathBuf> {
    let mut found: Vec<(u64, PathBuf)> = Vec::new();
    let mut folders = vec![(dir.to_path_buf(), 0)];
    while let Some((folder, depth)) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') || name == "target" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                if depth < 5 {
                    folders.push((path, depth + 1));
                }
            } else if path
                .extension()
                .is_some_and(|x| x == "rs" || x == "md" || x == "toml")
            {
                found.push((entry.metadata().map_or(0, |m| m.len()), path));
            }
        }
    }
    // A stable sort: of files of one size, the first found comes first, as before.
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    found.into_iter().take(n).map(|(_, path)| path).collect()
}

/// A board with 4 cards in each of its columns.
fn sample_board(boards: &mut Boards) -> u64 {
    let board = boards.add_board();
    let columns: Vec<u64> = boards
        .board(board)
        .map(|b| b.columns.iter().map(|c| c.id).collect())
        .unwrap_or_default();
    for (i, column) in columns.into_iter().enumerate() {
        for n in 1..=4 {
            if let Some(card) = boards.add_card(board, column) {
                boards.edit_card(board, card, |c| {
                    c.title = format!("Card {n} of column {}", i + 1)
                });
            }
        }
    }
    board
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

/// Wires each block to the next, back to front, as a cockpit's are, each labeled: the benches
/// paint wires and their labels.
fn wire_in_a_chain(ws: &mut Workspace) {
    let ids: Vec<_> = ws.space.canvas.blocks().iter().map(|b| b.id).collect();
    for pair in ids.windows(2) {
        if let Some(wire) = ws.space.add_wire(pair[0], pair[1])
            && let Some(key) = ws.space.label_key(wire)
        {
            ws.space.labels.insert(key, "dev server".into());
        }
    }
}
