# Moving the canvas over blocks

- **Date:** 2026-10-10
- **Status:** Implemented
- **Builds on:** [walking-skeleton.md](walking-skeleton.md) (panning and zooming) and [polish.md](polish.md)

## Goal

On a canvas full of blocks, the empty spaces that pan it run out: a drag over a terminal selects its text, and the
wheel scrolls its history. Templum gets two ways to move the canvas wherever the pointer is.

## The hand: hold Alt

- While **Alt** is held (**Option** on macOS), a layer covers the canvas, blocks included, with the open-hand
  cursor (the closed hand while dragging), and the toolbar shows the Hand tool on; letting go shows the tool that
  was on before:
  - a drag pans the canvas;
  - the wheel, or two fingers on a touchpad, zooms around the pointer, as the Hand tool does;
  - Ctrl+wheel and pinching zoom, as anywhere.
- Blocks don't react meanwhile: no focus, no selection, no move, no hover.
- Letting go of Alt removes the layer. A drag already under way goes on until the button is released.
- The empty canvas's hint says so: "Hold Alt · Move the canvas over blocks".

Alt alone types nothing in a terminal, and Alt+key still reaches it (the layer only takes the pointer). On the dev
machine (Hyprland), window bindings use Super, so Alt with the mouse is free.

## The middle button

A drag with the middle mouse button pans the canvas anywhere, over blocks too, with no key held. Terminals don't
use the middle button (no paste of the primary selection yet).

## Framing a block

A double-click on a block's title glides the view to show the block centered, at the largest zoom step that fits
above the toolbar. A double-click on the same title again, before the view moved, glides back to where it was.
Like any zoom, this magnifies the block's contents: a terminal keeps its columns and rows. The title, because a
double-click inside a terminal, an editor or a note selects a word.

## Testing

**Headless UI:**

- With Alt held, the hand layer exists; a drag over a terminal pans the canvas and selects nothing; the wheel over
  a terminal zooms the canvas; the toolbar shows the Hand on until Alt is let go; letting go of Alt removes the
  layer.
- With the Hand tool, the wheel zooms over blocks and the empty canvas.
- A double-click on a title frames the block above the toolbar; another goes back.
- A middle-button drag over a terminal pans the canvas.

**Manual (Hyprland):** hold Alt over a terminal and over a kanban block; drag and two-finger scroll.
