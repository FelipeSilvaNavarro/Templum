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

## Holding a block

While a block (or a selection) is held by its title, the wheel, or two fingers on a touchpad, pans the canvas, a
mouse's wheel and Ctrl+wheel included, and the block stays under the pointer: it can be carried farther than the
window shows without letting go.

## The middle button

A drag with the middle mouse button pans the canvas anywhere, over blocks too, with no key held. Terminals don't
use the middle button (no paste of the primary selection yet).

## Framing a block

A double-click on a block's title glides the view to show the block centered, at the largest zoom step that fits
above the toolbar. A double-click on the same title again, before the view moved, glides back to where it was.
Like any zoom, this magnifies the block's contents: a terminal keeps its columns and rows. The title, because a
double-click inside a terminal, an editor or a note selects a word.

## The keyboard follows the pointer

**Focus follows the pointer** (Settings ▸ Canvas, `[canvas] focus_follows_pointer`, on by default): once the pointer
moves, the block under it takes the keyboard, no click needed, as Hyprland's `follow_mouse` does and as grid mode's
tiles do ([grid.md](grid.md#moving-around)).

- **The keyboard only.** Blocks overlap on the canvas: the block stays where it is in the stack, so moving across
  the visible edge of a block behind another doesn't bring it to the front. A click still does.
- **Only when the pointer moves.** A block that comes under a still pointer (a wheel pan, a zoom, a glide, a block
  opened, moved or closed) doesn't take the keyboard until the pointer moves; nor does a move reported where the
  pointer already was.
- **Not while** a button is held or a drag goes on, a right-click menu is open, or the keyboard is outside the canvas
  and its blocks (a dialog, the settings panel, the Git popup, the theme picker): it stays there until a click.
- **A selection keeps it.** While blocks, a drawing or a wire are selected, the keyboard stays with them (Delete and
  Copy act on them) and the selection stays; a click ends it as before.
- **The block that had the keyboard lets go as on a click:** a checklist item being edited stops; a note's or an
  editor's text and cursor stay, and its editor takes the keyboard again when the pointer comes back.
- With the setting off, a click focuses, as before. Grid mode's tiles follow either way.

## Testing

**Headless UI:**

- With Alt held, the hand layer exists; a drag over a terminal pans the canvas and selects nothing; the wheel over
  a terminal zooms the canvas; the toolbar shows the Hand on until Alt is let go; letting go of Alt removes the
  layer.
- With the Hand tool, the wheel zooms over blocks and the empty canvas.
- A double-click on a title frames the block above the toolbar; another goes back.
- A middle-button drag over a terminal pans the canvas.
- Moving the pointer over a terminal gives it the keyboard, leaves it under the others and takes it from the note's
  editor; a block coming under a still pointer, or a move to where the pointer is, doesn't take it; a real move
  does.
- The keyboard stays with the setting off, with blocks selected (they stay selected), with a menu open, with a
  button held and with the keyboard in the settings panel; back on the canvas, it follows.

**Manual (Hyprland):** hold Alt over a terminal and over a kanban block; drag and two-finger scroll. Move the pointer
between overlapping terminals, a note and an editor and type in each; scroll the canvas under a still pointer.
