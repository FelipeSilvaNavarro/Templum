# 1. Pure Rust, GPU-rendered UI, no webview

- **Date:** 2026-10-10
- **Status:** Accepted

## Context

Templum shows an unbounded number of terminals on a pannable, zoomable canvas, and aims to be as native and
light as possible on macOS, Linux and Windows. We first considered Tauri, where the UI runs in the system webview:

- **With xterm.js**, each terminal needs its own WebGL context, and webviews cap them (around 16). Zooming either
  blurs the text or forces every terminal to redraw, and every output byte crosses IPC into JavaScript.
- **With emulation in Rust and a custom WebGL renderer**, the webview no longer pays for itself. We would have two
  languages, an IPC hop every frame, hundreds of MB of baseline memory, and WebKitGTK rendering bugs on Linux.

## Decision

Templum is a single Rust application that draws its UI directly on the GPU. Terminal emulation (VT parsing, grid,
PTY including Windows ConPTY) uses [`alacritty_terminal`](https://crates.io/crates/alacritty_terminal). The UI
framework is chosen in ADR-0002.

## Consequences

- One language and toolchain, no IPC, a small memory footprint, and the same behavior on every platform.
- Menus, settings and dialogs are built with a Rust UI toolkit instead of HTML/CSS.
- Contributors need Rust. AI assistants know Rust GPU toolkits less well than web stacks, so we pin exact versions
  and keep permissively licensed reference code at hand.
- Installers and updates come from `cargo-packager` instead of Tauri's bundler.
