# 3. gpui-fast as the GPUI engine under gpui-kit

- **Date:** 2026-10-14
- **Status:** Accepted

## Context

[gpui-fast](https://github.com/longbridge/gpui-fast) is a fork of GPUI from the same upstream Zed commit as the
`gpui-pre` 0.3.9 snapshot gpui-kit pins. It keeps GPUI's API and changes how frames are drawn: views, layout nodes and
text measurements are reused while nothing they read changed ("retained mode"), scrolled content is cached as GPU
tiles ("scroll layers"), and the wgpu renderer records frames differently. gpui-kit 0.7.3 switches its whole stack
to it with one feature, `gpui-fast`.

Templum draws almost everything in one view (`Workspace`), and panning or zooming moves every block, so the large
gains its README reports (few views changing per frame) don't apply as-is; the lower-level caches and the renderer do.
Scroll layers never apply to the canvas (the camera is not a scroll container).

We measured both engines with the same commit, release builds, alternating 5 runs of each (Hyprland, 165 Hz,
RTX 3050). Median p95 frame interval, and Templum's own CPU time divided by frames drawn:

| Bench | p95 gpui-pre → gpui-fast | CPU per frame |
|---|---|---|
| `--bench` | 14.76 → 13.15 ms | 16.1 → 14.0 ms (−13%) |
| `--bench-mixed` | 8.41 → 7.89 ms | 2.7 → 2.4 ms (−11%) |
| `--bench-far` | 9.55 → 6.61 ms | 3.7 → 2.7 ms (−27%) |
| `--bench-zoom=editor` | 12.68 → 11.48 ms | 4.7 → 4.0 ms (−15%) |
| `--bench-zoom=terminal` | 8.45 → 8.18 ms | 2.4 → 2.0 ms (−17%) |
| `--bench-git` | 9.01 → 7.75 ms | 3.2 → 2.7 ms (−16%) |
| `--bench-zoom=note` | 51.6 → 54.2 ms | 13.7 → 14.6 ms (+7%) |

The note zoom was already far over the bar (gpui-kit's editor re-wraps the whole note at each size) and is slightly
slower. Memory is 2–7 MB higher.

## Decision

Build gpui-kit with its `gpui-fast` feature, gpui-fast's crates locked at 0.1.8. No Templum code changes: the PTY wake,
the animations and every subscription already notify the entity they change.

## Consequences

- **Read the right source.** GPUI's code is in `~/.cargo/registry/src/*/gpui-fast-*` now; `gpui-pre` is still
  compiled (gpui-kit depends on it unconditionally) but no longer drawn with.
- **Render state must be notified.** A view whose `render` reads something that is not an entity, a global or a
  list/scroll state keeps its last frame until `cx.notify()`. `GPUI_VIEW_RETENTION=0` turns view reuse off to check
  whether a stale screen comes from it; `GPUI_SCROLL_LAYERS=0` turns scroll tiles off.
- **The fork is young** (v0.1.0 on 2026-10-06, nine releases in nine days). Its crates move with `cargo update -p`,
  one at a time, like gpui-kit's; returning to `gpui-pre` is removing the feature.
- **Bigger gains need views.** Blocks as their own views, notified by their own PTY wake, would let retained mode
  skip every block that didn't change.
