# Themes you make, edit and download

- **Date:** 2026-10-14
- **Status:** Approved
- **Builds on:** [design-system.md](design-system.md) (theme files, derived colors, the picker),
  [settings.md](settings.md) (the panel's rows and edits) and [updates.md](updates.md) (HTTP through
  `templum-update`)

## Goal

A theme is an Alacritty color file, and any one dropped in the `themes` folder works. But Templum couldn't make one,
change one, or find more: editing the theme in use did nothing until a restart, and the interface colors (the
focused block's border, the canvas, the dots) always came from the terminal colors. Now a person can start a theme
from the one in use, change its colors with color pickers or in its file, and see every change at once; and a
gallery downloads the themes of [alacritty-theme](https://github.com/alacritty/alacritty-theme).

## Theme files

The format stays Alacritty's ([design-system.md](design-system.md#theme-files)), with one optional table Templum
adds:

```toml
[templum]
accent = "#89b4fa"  # the focused block's border and ring, the picker's selection
canvas = "#151520"  # behind the blocks
dots = "#4d5066"    # the dot grid
```

- Each key is optional and is `"#rrggbb"` or `"0xrrggbb"`. A missing key, or a value that isn't a color, is derived
  as before: accent from `normal.blue`, canvas from the background, dots from the foreground.
- `accent` also sets the ring around the focused block (the accent at 40%).
- `dots` is drawn as written, opaque. The derived dots are the foreground at 30% (22% on light themes); a file that
  wants the same look writes that blend over the canvas, which is what a new theme's file shows.
- Alacritty only logs a warning about the table it doesn't know, so the same file still works there.

## Live reload

- The file of the theme in use is checked with `config.toml`, every 0.5 s (modification time and size). When it
  changes, it is read again and applied: saving it from an editor block, or from any editor, shows at once.
- A file that no longer reads (a typo mid-edit, a missing color, the file deleted) keeps the last good colors on
  screen and says so once, in a warning: `Theme "X": missing colors.normal.red. Keeping the last good one`. The next
  save that reads applies.
- Switching between light and dark reads the other theme's file once, so a change made while it wasn't in use
  shows too.
- Templum's own writes (the color editor) don't reload anything: what the file now holds is already on screen.

## Making a theme

Settings ▸ Appearance has a card for the theme in use, under the mode and theme rows:

- **A built-in theme** says "Built in: duplicate it to change its colors" and has **Duplicate…**.
- **One of yours** shows its file (`themes/X.toml`) and has **New theme…**, **Open file** and the color editor.
- **Duplicate…** and **New theme…** ask for a name (proposing "<name> copy"), write `themes/<name>.toml` from the
  theme in use, and make it the theme of the mode in effect.
  - The name can't be empty, start with a dot, or hold `/ \ : * ? " < > |` or control characters.
  - A theme of that name already in the folder is never overwritten: the dialog stays open and says so.
  - A name equal to a built-in's makes a copy that replaces it, as any file in the folder does.
- The new file is a template: comments explain every key and what it colors in Templum, the 20 terminal colors are
  written out, and the `[templum]` keys are written commented out with the values derived now (remove the `#` to
  fix one).
- **Open file** closes the panel and opens the file in an editor block (one already holding it, else any editor,
  else a new one), where saving applies it.
- Without a config folder (no HOME, no APPDATA) the buttons don't show.

## The color editor

For one of your themes, the card lists its colors in four groups, each a swatch with its name:

| Group | Colors |
|---|---|
| Primary | Background, Foreground, Cursor, Selection |
| Normal | Black, Red, Green, Yellow, Blue, Magenta, Cyan, White |
| Bright | the same eight |
| Interface | Accent, Canvas, Dots |

- A click opens gpui-kit's color picker: swatches (the theme's own colors first), sliders and a hex field.
- Every change applies to the whole window at once, terminals included, and is written to the theme's file 0.3 s
  after the last one, or when the panel or Templum closes. Only the changed key is written: the file keeps its
  comments, its other keys, its line endings and its byte-order mark.
- An interface color the file sets has a ↺ that takes the key out, back to the derived color.
- A file that doesn't parse is never written: the change isn't shown, and a notification names the file and the line.
- The picker's alpha slider is ignored: theme colors are opaque.

## The gallery

- **Opens from** "Get more themes…" at the bottom of the title bar's theme picker, and from **Get more themes…** in
  Settings ▸ Appearance. It needs the config and data folders (not `--bench`).
- **Source:** the `themes` folder of `alacritty/alacritty-theme` (Apache-2.0): one GitHub API call lists the files
  with their hashes, and each file not cached yet is downloaded from `raw.githubusercontent.com`, eight at a time.
  Only URLs under `https://raw.githubusercontent.com/alacritty/alacritty-theme/` and 40-hex hashes are accepted.
- **Cache:** `<data folder>/theme-gallery/`: `index.json` (the listing) and one `<hash>.toml` per file. Opening shows
  the cache at once, then refreshes it; offline, or past GitHub's hourly limit, the cache stays with a line saying
  why. Files that don't parse as a theme are left out.
- **Names:** the file name with `_` and `-` as spaces, each word capitalized: `github_dark_high_contrast` is
  "Github Dark High Contrast", `one_dark` is "One Dark".
- **Showing:** the themes of the mode in effect (switch Mode to browse the others), as cards in their own colors,
  with a search field. A theme already installed, or one whose name a theme already has, is marked.
- **Previewing:** hovering a card applies it to the whole window. Esc, **Done** or a click outside goes back to the
  mode's theme.
- **Installing:** a click writes `themes/<Name>.toml` (the file as downloaded, under a comment naming its source and
  license), makes it the mode's theme, and closes the gallery. A theme with that name already there is chosen
  instead; nothing is overwritten.

## Architecture

| File | Change |
|---|---|
| `theme.rs` | `Theme::interface` (the `[templum]` colors as written); `parse` reads them; color slots (`COLORS`, `key`, `colors`); `rgb`, `hex`; `Entry::path`; `read`, `template`, `file_name`, `create` |
| `config.rs` | `Stamp::file(path)`; `apply_edits(text, edits)` and `save_edits_to(path, edits)` under `save_edits`; `SaveError::describe(file)` |
| `workspace/preferences.rs` | `poll_theme`; `set_mode_theme`; `new_theme`; `open_theme_file`; `set_theme_color` and the theme file's pending edits, written with the config's; `install_theme` |
| `settings/colors.rs` (new) | The theme card under Appearance: buttons and the color editor |
| `settings/welcome.rs` | `theme_card`, shared with the gallery |
| `gallery.rs` (new) | Listing, names, cache, refresh, and the dialog |
| `theme_picker.rs` | "Get more themes…" |
| `templum-update/src/fetch.rs` | `get(agent, url)`: a page as text, with the same errors as the release check |

## Testing

Headless tests cover: the `[templum]` table and derived fallbacks; a template read back gives the same theme;
rounding of colors; file names; never overwriting; saving the theme file applies it; a broken file keeps the last
good theme and warns once; the other mode's theme reread on switching; New theme and Duplicate; Open file; the card's
buttons for built-ins and for one's own; color edits applied at once, written once settled, reset, across a mode
switch, and never into a broken file; the listing's checks; names; the cache; the gallery's preview, restore and
install; the picker's way in. The network refresh and the color picker's own popover are checked by hand.
