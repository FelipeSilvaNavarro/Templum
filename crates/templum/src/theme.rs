//! Themes: Alacritty color files, the interface colors derived from them, the built-ins and the
//! user's themes folder.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::{App, Hsla};
use templum_terminal::{Palette, Rgb};
use toml_edit::DocumentMut;

use crate::board::Label;
use crate::grid::hsla;

const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
const ANSI_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];
/// The interface colors a theme file may set in its `[templum]` table.
const INTERFACE: [&str; 3] = ["accent", "canvas", "dots"];

/// The colors a theme file sets, in `key` order: 20 for the terminal, 3 for the interface.
pub(crate) const COLORS: usize = 23;

/// Color `i`'s table and key in a theme file.
pub(crate) fn key(i: usize) -> (&'static str, &'static str) {
    match i {
        0 => ("colors.primary", "background"),
        1 => ("colors.primary", "foreground"),
        2 => ("colors.cursor", "cursor"),
        3 => ("colors.selection", "background"),
        4..12 => ("colors.normal", ANSI_NAMES[i - 4]),
        12..20 => ("colors.bright", ANSI_NAMES[i - 12]),
        _ => ("templum", INTERFACE[i - 20]),
    }
}

/// A terminal palette plus the interface colors derived from it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Theme {
    pub name: String,
    pub palette: Palette,
    pub is_dark: bool,
    /// The `[templum]` colors the file sets (accent, canvas, dots); None: derived.
    pub interface: [Option<Rgb>; 3],
    pub ui: Ui,
}

/// Interface colors, computed from the terminal colors with one formula for every theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ui {
    /// Under the blocks, opaque.
    pub canvas: Hsla,
    pub dot: Hsla,
    /// Cards on the canvas: blocks, the settings panel, the theme picker. Opaque.
    pub surface: Hsla,
    pub hairline: Hsla,
    /// Block titles: the focused one, the others.
    pub title: Hsla,
    pub title_dim: Hsla,
    pub hint: Hsla,
    pub hover: Hsla,
    /// A hovered block's border.
    pub hover_border: Hsla,
    /// Behind a button while it is pressed.
    pub pressed: Hsla,
    /// A faint fill: settings cards, tabs, key pills.
    pub card: Hsla,
    /// Focused block border, picker selection.
    pub accent: Hsla,
    /// The soft ring around the focused block.
    pub ring: Hsla,
    /// Drawn over selected cells; the text keeps its color.
    pub selection: Hsla,
    pub cursor: Hsla,
    /// IME pre-edit text.
    pub marked: Hsla,
    pub shadow: Hsla,
}

/// Blends every token (in HSL channels) for the crossfade between two themes.
impl gpui_kit::base::animation::Lerp for Ui {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Ui {
            canvas: self.canvas.lerp(&to.canvas, t),
            dot: self.dot.lerp(&to.dot, t),
            surface: self.surface.lerp(&to.surface, t),
            hairline: self.hairline.lerp(&to.hairline, t),
            title: self.title.lerp(&to.title, t),
            title_dim: self.title_dim.lerp(&to.title_dim, t),
            hint: self.hint.lerp(&to.hint, t),
            hover: self.hover.lerp(&to.hover, t),
            hover_border: self.hover_border.lerp(&to.hover_border, t),
            pressed: self.pressed.lerp(&to.pressed, t),
            card: self.card.lerp(&to.card, t),
            accent: self.accent.lerp(&to.accent, t),
            ring: self.ring.lerp(&to.ring, t),
            selection: self.selection.lerp(&to.selection, t),
            cursor: self.cursor.lerp(&to.cursor, t),
            marked: self.marked.lerp(&to.marked, t),
            shadow: self.shadow.lerp(&to.shadow, t),
        }
    }
}

/// A colored note's card has its label's hue, this share of its saturation and this share of the
/// way from the theme's background to its lightness.
const NOTE_CHROMA: f32 = 0.4;
const NOTE_TINT: f32 = 0.10;

impl Theme {
    /// A colored note's card: the theme's background, tinted with the label's hue, so the text
    /// on it stays readable.
    pub fn note_bg(&self, label: Label) -> Hsla {
        hsla(note_tint(&self.palette, label), 1.0)
    }

    pub fn new(name: &str, palette: Palette, cursor: Option<Rgb>, selection: Option<Rgb>) -> Self {
        // About CIE L* 50.
        let is_dark = luminance(palette.background) < 0.18;
        let fg = palette.foreground;
        let blue = palette.ansi[4];
        let ui = Ui {
            canvas: hsla(
                mix(palette.background, BLACK, if is_dark { 0.30 } else { 0.04 }),
                1.0,
            ),
            // Strong enough to find your way by; lighter on light themes, where gray reads darker.
            dot: hsla(fg, if is_dark { 0.30 } else { 0.22 }),
            surface: hsla(palette.background, 1.0),
            hairline: hsla(fg, 0.10),
            title: hsla(fg, 0.60),
            title_dim: hsla(fg, 0.40),
            hint: hsla(fg, 0.40),
            hover: hsla(fg, 0.08),
            hover_border: hsla(fg, 0.22),
            pressed: hsla(fg, 0.14),
            card: hsla(fg, 0.04),
            accent: hsla(blue, 1.0),
            ring: hsla(blue, 0.40),
            selection: hsla(selection.unwrap_or(blue), 0.35),
            cursor: hsla(cursor.unwrap_or(fg), 1.0),
            marked: hsla(palette.ansi[3], 1.0),
            shadow: hsla(BLACK, if is_dark { 0.45 } else { 0.15 }),
        };
        Self {
            name: name.to_string(),
            palette,
            is_dark,
            interface: [None; 3],
            ui,
        }
    }

    /// Every color, in `key` order, as drawn: a derived one as worked out now. Dots that are
    /// derived are translucent, so theirs is the blend over the canvas they look like.
    pub fn colors(&self) -> [Rgb; COLORS] {
        let p = &self.palette;
        let canvas = rgb(self.ui.canvas);
        let dots =
            self.interface[2].unwrap_or_else(|| mix(canvas, rgb(self.ui.dot), self.ui.dot.a));
        let mut colors = [BLACK; COLORS];
        colors[0] = p.background;
        colors[1] = p.foreground;
        colors[2] = rgb(self.ui.cursor);
        colors[3] = rgb(self.ui.selection);
        colors[4..20].copy_from_slice(&p.ansi);
        colors[20] = rgb(self.ui.accent);
        colors[21] = canvas;
        colors[22] = dots;
        colors
    }

    /// The `[templum]` colors in place of the derived ones. Dots are drawn as written, opaque.
    fn with_interface(mut self, interface: [Option<Rgb>; 3]) -> Self {
        let [accent, canvas, dots] = interface;
        if let Some(accent) = accent {
            self.ui.accent = hsla(accent, 1.0);
            self.ui.ring = hsla(accent, 0.40);
        }
        if let Some(canvas) = canvas {
            self.ui.canvas = hsla(canvas, 1.0);
        }
        if let Some(dots) = dots {
            self.ui.dot = hsla(dots, 1.0);
        }
        self.interface = interface;
        self
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum ThemeError {
    Toml(String),
    Missing(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Toml(message) => write!(f, "not valid TOML: {message}"),
            Self::Missing(key) => write!(f, "missing {key}"),
        }
    }
}

/// Reads a theme in Alacritty's format, plus Templum's own `[templum]` table. Only the colors
/// Templum uses are read; everything else in the file is ignored.
pub(crate) fn parse(name: &str, text: &str) -> Result<Theme, ThemeError> {
    // Windows editors may save a byte-order mark, which TOML doesn't allow.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| ThemeError::Toml(e.message().to_string()))?;
    let color = |table: &str, key: &str| {
        doc.get("colors")?
            .get(table)?
            .get(key)?
            .as_str()
            .and_then(parse_hex)
    };
    let need = |table: &str, key: &str| {
        color(table, key).ok_or_else(|| ThemeError::Missing(format!("colors.{table}.{key}")))
    };
    let background = need("primary", "background")?;
    let foreground = need("primary", "foreground")?;
    let mut ansi = [BLACK; 16];
    for (i, key) in ANSI_NAMES.iter().enumerate() {
        ansi[i] = need("normal", key)?;
        ansi[i + 8] = color("bright", key).unwrap_or(ansi[i]);
    }
    let palette = Palette {
        foreground,
        background,
        ansi,
    };
    // A value that isn't a color counts as missing, as in `[colors]`.
    let interface = INTERFACE.map(|key| doc.get("templum")?.get(key)?.as_str().and_then(parse_hex));
    Ok(Theme::new(
        name,
        palette,
        color("cursor", "cursor"),
        color("selection", "background"),
    )
    .with_interface(interface))
}

/// "#rrggbb" or "0xrrggbb". Anything else (Alacritty's "CellForeground"...) is not a color.
fn parse_hex(s: &str) -> Option<Rgb> {
    let hex = s
        .strip_prefix('#')
        .or_else(|| s.strip_prefix("0x"))
        .or_else(|| s.strip_prefix("0X"))?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    })
}

/// `c` without its alpha, rounded (GPUI's own conversions truncate, which can turn 255 into 254).
pub(crate) fn rgb(c: Hsla) -> Rgb {
    let c = c.to_rgb();
    let f = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

/// "#rrggbb".
pub(crate) fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// WCAG relative luminance.
fn luminance(c: Rgb) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

fn note_tint(palette: &Palette, label: Label) -> Rgb {
    let (bg, color) = (
        hsla(palette.background, 1.0),
        hsla(palette.ansi[label.ansi()], 1.0),
    );
    rgb(Hsla {
        h: color.h,
        s: color.s * NOTE_CHROMA,
        l: bg.l + (color.l - bg.l) * NOTE_TINT,
        a: 1.0,
    })
}

/// Linear blend in sRGB: `t = 0` is `a`, `t = 1` is `b`.
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb {
        r: f(a.r, b.r),
        g: f(a.g, b.g),
        b: f(a.b, b.b),
    }
}

pub(crate) const DEFAULT_LIGHT: &str = "Catppuccin Latte";
pub(crate) const DEFAULT_DARK: &str = "Catppuccin Mocha";
/// The picker's group for themes from the user's folder.
pub(crate) const MY_THEMES: &str = "My themes";

pub(crate) struct Builtin {
    pub group: &'static str,
    pub name: &'static str,
    pub toml: &'static str,
}

/// Embedded in the binary; sources and licenses are in themes/CREDITS.md.
pub(crate) const BUILTIN: [Builtin; 8] = [
    Builtin {
        group: "Catppuccin",
        name: "Catppuccin Latte",
        toml: include_str!("../themes/catppuccin-latte.toml"),
    },
    Builtin {
        group: "Catppuccin",
        name: "Catppuccin Frappé",
        toml: include_str!("../themes/catppuccin-frappe.toml"),
    },
    Builtin {
        group: "Catppuccin",
        name: "Catppuccin Macchiato",
        toml: include_str!("../themes/catppuccin-macchiato.toml"),
    },
    Builtin {
        group: "Catppuccin",
        name: "Catppuccin Mocha",
        toml: include_str!("../themes/catppuccin-mocha.toml"),
    },
    Builtin {
        group: "One",
        name: "One Light",
        toml: include_str!("../themes/one-light.toml"),
    },
    Builtin {
        group: "One",
        name: "One Dark",
        toml: include_str!("../themes/one-dark.toml"),
    },
    Builtin {
        group: "Apple",
        name: "Apple Light",
        toml: include_str!("../themes/apple-light.toml"),
    },
    Builtin {
        group: "Apple",
        name: "Apple Dark",
        toml: include_str!("../themes/apple-dark.toml"),
    },
];

pub(crate) fn default_name(dark: bool) -> &'static str {
    if dark { DEFAULT_DARK } else { DEFAULT_LIGHT }
}

/// One line of the picker.
pub(crate) struct Entry {
    pub group: &'static str,
    pub theme: Rc<Theme>,
    /// The user's file; None for a built-in.
    pub path: Option<PathBuf>,
}

/// The built-ins, then the valid themes of `<config_dir>/themes` sorted by name. A user theme
/// with a built-in's name replaces it.
pub(crate) fn load(config_dir: Option<&Path>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = BUILTIN
        .iter()
        .map(|b| Entry {
            group: b.group,
            theme: Rc::new(parse(b.name, b.toml).expect("built-in themes are valid")),
            path: None,
        })
        .collect();
    let mut mine = config_dir
        .map(|dir| read_folder(&dir.join("themes")))
        .unwrap_or_default();
    mine.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    for (theme, path) in mine {
        entries.retain(|e| e.theme.name != theme.name);
        entries.push(Entry {
            group: MY_THEMES,
            theme: Rc::new(theme),
            path: Some(path),
        });
    }
    entries
}

/// The theme in the file at `path`, named after the file.
pub(crate) fn read(path: &Path) -> Result<Theme, String> {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&name, &text).map_err(|e| e.to_string())
}

/// Every valid `*.toml` directly inside `folder`. Broken files are reported and skipped.
fn read_folder(folder: &Path) -> Vec<(Theme, PathBuf)> {
    let files = match std::fs::read_dir(folder) {
        Ok(files) => files,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!("templum: couldn't read {}: {e}", folder.display());
            return Vec::new();
        }
    };
    let mut themes = Vec::new();
    for path in files.flatten().map(|f| f.path()) {
        if path.extension().is_none_or(|e| e != "toml") || !path.is_file() {
            continue;
        }
        match read(&path) {
            Ok(theme) => themes.push((theme, path)),
            Err(err) => eprintln!("templum: skipping theme {}: {err}", path.display()),
        }
    }
    themes
}

/// The file of a new theme called `name`: `theme`'s colors, with comments that say what each one
/// colors. The `[templum]` colors `theme` doesn't set are written commented out, with their value
/// now.
pub(crate) fn template(name: &str, theme: &Theme) -> String {
    let colors = theme.colors();
    let mut out = format!(
        "# {name}: a Templum theme, made from {from}.\n\
         # The format is Alacritty's (https://alacritty.org/config-alacritty.html#colors): it works there\n\
         # too. Change a color here, or in Templum's Settings > Appearance; saving this file shows it at once.\n\
         # Colors are \"#rrggbb\".\n",
        name = name.trim(),
        from = theme.name,
    );
    let notes: [(usize, &str); 6] = [
        (
            0,
            "\n[colors.primary]\n# Behind the text: terminals, editors, notes, the blocks' cards.\n",
        ),
        (
            1,
            "# The text, and the base of titles, borders and the dot grid.\n",
        ),
        (2, "\n[colors.cursor]\n"),
        (
            3,
            "\n[colors.selection]\n# Drawn at 35% over selected text.\n",
        ),
        (
            4,
            "\n# The 16 colors programs print with. Blue is the accent unless [templum] sets one, and\n\
             # the others tint colored notes and labels.\n[colors.normal]\n",
        ),
        (12, "\n[colors.bright]\n"),
    ];
    for (i, color) in colors.iter().enumerate().take(20) {
        if let Some((_, note)) = notes.iter().find(|(at, _)| *at == i) {
            out.push_str(note);
        }
        out.push_str(&format!("{} = \"{}\"\n", key(i).1, hex(*color)));
    }
    out.push_str(
        "\n# Templum's own colors. Left commented out, each is worked out from the colors above: take\n\
         # out the # before one to set it.\n[templum]\n",
    );
    let about = [
        "The focused block's border and ring, the picker's selection.",
        "Behind the blocks.",
        "The dot grid.",
    ];
    for (k, about) in about.iter().enumerate() {
        let off = if theme.interface[k].is_some() {
            ""
        } else {
            "# "
        };
        out.push_str(&format!(
            "# {about}\n{off}{} = \"{}\"\n",
            INTERFACE[k],
            hex(colors[20 + k])
        ));
    }
    out
}

/// A theme's file name: refuses what no file system takes, and a leading dot (a hidden file).
pub(crate) fn file_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Type a name");
    }
    if name.starts_with('.') {
        return Err("A name can't start with a dot");
    }
    if name
        .chars()
        .any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c))
    {
        return Err(r#"A name can't have / \ : * ? " < > |"#);
    }
    // Windows opens a device for these, with any extension.
    let upper = name.to_uppercase();
    let device = ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || ["COM", "LPT"].iter().any(|p| {
            upper
                .strip_prefix(p)
                .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit() && n != "0")
        });
    if device {
        return Err("Windows keeps that name for a device");
    }
    Ok(format!("{name}.toml"))
}

/// Writes a new theme file, `<config_dir>/themes/<name>.toml`, never over one already there.
pub(crate) fn create(config_dir: &Path, name: &str, text: &str) -> Result<PathBuf, String> {
    let file = file_name(name)?;
    let folder = config_dir.join("themes");
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("Couldn't create {}: {e}", folder.display()))?;
    let path = folder.join(file);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()));
    match written {
        Ok(()) => Ok(path),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(format!("A theme named \"{}\" already exists", name.trim()))
        }
        Err(e) => Err(format!("Couldn't write {}: {e}", path.display())),
    }
}

pub(crate) fn find(themes: &[Entry], name: &str) -> Option<Rc<Theme>> {
    themes
        .iter()
        .find(|e| e.theme.name == name)
        .map(|e| e.theme.clone())
}

/// The theme called `name`, or `default` (with a warning) when there is none.
pub(crate) fn pick(themes: &[Entry], name: &str, default: &str) -> Rc<Theme> {
    find(themes, name)
        .or_else(|| {
            eprintln!("templum: no theme named {name:?}, using {default:?}");
            find(themes, default)
        })
        .expect("the default themes are built in")
}

/// Gives gpui-kit's own widgets (title bar, notifications) the theme's colors.
pub(crate) fn apply_to_kit(theme: &Theme, cx: &mut App) {
    use gpui_kit::component::{Theme as Kit, ThemeMode};
    let mode = if theme.is_dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    // A mode change reloads gpui-kit's built-in colors: switch first, then set ours.
    Kit::update(cx, |kit| kit.mode = mode);
    let ui = theme.ui;
    let background = hsla(theme.palette.background, 1.0);
    let foreground = hsla(theme.palette.foreground, 1.0);
    Kit::update(cx, |kit| {
        // gpui-kit's root paints `background` under the whole window: the opaque canvas. The title
        // bar is drawn on it.
        kit.background = ui.canvas;
        kit.title_bar = gpui_kit::transparent_black();
        kit.popover = background;
        kit.foreground = foreground;
        kit.popover_foreground = foreground;
        kit.border = ui.hairline;
        kit.title_bar_border = ui.hairline;
        kit.window_border = ui.hairline;
        kit.muted_foreground = ui.title_dim;
        kit.accent = ui.accent;
        kit.ring = ui.accent;
        kit.selection = ui.selection;
        // The code editor's background, line-number gutter and current line come from
        // gpui-kit's highlight theme, fixed per mode (white, or near black): the block's card
        // and its faint fill instead. The gutter is painted opaque over scrolled text, so it
        // takes the card's own color.
        let mut highlight = (*kit.highlight_theme).clone();
        highlight.style.editor_background = Some(ui.surface);
        highlight.style.editor_gutter_background = Some(ui.surface);
        highlight.style.editor_active_line = Some(ui.card);
        highlight.style.editor_foreground = Some(foreground);
        highlight.style.editor_line_number = Some(ui.title_dim);
        highlight.style.editor_active_line_number = Some(foreground);
        kit.highlight_theme = std::sync::Arc::new(highlight);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOCHA: &str = include_str!("../themes/catppuccin-mocha.toml");
    const LATTE: &str = include_str!("../themes/catppuccin-latte.toml");
    const MINIMAL: &str = r##"
[colors.primary]
background = "0x101010"
foreground = "0XE0E0E0"

[colors.normal]
black = "#000000"
red = "#aa0000"
green = "#00aa00"
yellow = "#aaaa00"
blue = "#0000aa"
magenta = "#aa00aa"
cyan = "#00aaaa"
white = "#aaaaaa"
"##;

    fn rgb(v: u32) -> Rgb {
        Rgb {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        }
    }

    #[test]
    fn parses_catppuccin_mocha() {
        let t = parse("Catppuccin Mocha", MOCHA).unwrap();
        assert_eq!(t.name, "Catppuccin Mocha");
        assert_eq!(t.palette.background, rgb(0x1e1e2e));
        assert_eq!(t.palette.foreground, rgb(0xcdd6f4));
        let normal = [
            0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de,
        ];
        let bright = [
            0x585b70, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xa6adc8,
        ];
        let expected: Vec<Rgb> = normal.iter().chain(&bright).map(|&v| rgb(v)).collect();
        assert_eq!(t.palette.ansi.to_vec(), expected);
        assert!(t.is_dark);
    }

    #[test]
    fn accepts_0x_colors_and_copies_missing_bright() {
        let t = parse("Minimal", MINIMAL).unwrap();
        assert_eq!(t.palette.background, rgb(0x101010));
        assert_eq!(t.palette.foreground, rgb(0xe0e0e0));
        assert_eq!(t.palette.ansi[8..], t.palette.ansi[..8]);
    }

    #[test]
    fn missing_required_key_names_it() {
        let text = MINIMAL.replace("red = \"#aa0000\"\n", "");
        assert_eq!(
            parse("x", &text),
            Err(ThemeError::Missing("colors.normal.red".into()))
        );
        assert_eq!(
            parse("x", &text).unwrap_err().to_string(),
            "missing colors.normal.red"
        );
    }

    #[test]
    fn alacritty_cell_colors_count_as_missing() {
        let with_cursor = format!("{MINIMAL}\n[colors.cursor]\ncursor = \"CellForeground\"\n");
        let t = parse("x", &with_cursor).unwrap();
        assert_eq!(t.ui.cursor, hsla(t.palette.foreground, 1.0));
        let bad_bg = MINIMAL.replace("\"0x101010\"", "\"CellBackground\"");
        assert_eq!(
            parse("x", &bad_bg),
            Err(ThemeError::Missing("colors.primary.background".into()))
        );
    }

    #[test]
    fn templum_table_sets_interface_colors() {
        let plain = parse("x", MINIMAL).unwrap();
        assert_eq!(plain.interface, [None; 3]);
        let text = format!(
            "{MINIMAL}\n[templum]\naccent = \"#ff8800\"\ndots = \"0x333333\"\ncanvas = \"CellBackground\"\n"
        );
        let t = parse("x", &text).unwrap();
        assert_eq!(
            t.interface,
            [Some(rgb(0xff8800)), None, Some(rgb(0x333333))]
        );
        assert_eq!(t.ui.accent, hsla(rgb(0xff8800), 1.0));
        assert_eq!(
            t.ui.ring,
            hsla(rgb(0xff8800), 0.40),
            "the ring follows the accent"
        );
        assert_eq!(
            t.ui.dot,
            hsla(rgb(0x333333), 1.0),
            "dots are drawn as written"
        );
        assert_eq!(t.ui.canvas, plain.ui.canvas, "not a color: derived");
        assert_eq!(
            t.ui.selection, plain.ui.selection,
            "the selection stays the file's"
        );
        let canvas = format!("{MINIMAL}\n[templum]\ncanvas = \"#000000\"\n");
        assert_eq!(parse("x", &canvas).unwrap().ui.canvas, hsla(rgb(0), 1.0));
    }

    #[test]
    fn rgb_rounds_back() {
        for builtin in BUILTIN {
            let theme = parse(builtin.name, builtin.toml).unwrap();
            for c in theme.palette.ansi {
                assert_eq!(super::rgb(hsla(c, 1.0)), c, "{}", builtin.name);
                assert_eq!(super::rgb(hsla(c, 0.35)), c, "alpha is dropped");
            }
        }
        assert_eq!(hex(rgb(0x0a84ff)), "#0a84ff");
    }

    #[test]
    fn template_round_trips_every_builtin() {
        for builtin in BUILTIN {
            let theme = parse(builtin.name, builtin.toml).unwrap();
            let text = template("Mine", &theme);
            assert!(text.starts_with("# Mine: a Templum theme, made from "));
            let back = parse(builtin.name, &text).unwrap();
            assert_eq!(back.palette, theme.palette, "{}", builtin.name);
            assert_eq!(back.ui, theme.ui, "{}", builtin.name);
            assert_eq!(back.interface, [None; 3], "commented out");
            assert_eq!(back.colors(), theme.colors());
        }
        let set = format!("{MINIMAL}\n[templum]\naccent = \"#ff8800\"\n");
        let theme = parse("x", &set).unwrap();
        let back = parse("x", &template("y", &theme)).unwrap();
        assert_eq!(back, theme, "a set color stays set");
        // The derived dots, written as a solid color, look the same over the canvas.
        let mocha = parse("m", MOCHA).unwrap();
        let canvas = super::rgb(mocha.ui.canvas);
        assert_eq!(
            mocha.colors()[22],
            mix(canvas, mocha.palette.foreground, 0.30)
        );
    }

    #[test]
    fn color_slots_name_their_keys() {
        assert_eq!(key(0), ("colors.primary", "background"));
        assert_eq!(key(3), ("colors.selection", "background"));
        assert_eq!(key(5), ("colors.normal", "red"));
        assert_eq!(key(19), ("colors.bright", "white"));
        assert_eq!(key(22), ("templum", "dots"));
        let mocha = parse("m", MOCHA).unwrap();
        let colors = mocha.colors();
        assert_eq!(colors[5], rgb(0xf38ba8));
        assert_eq!(colors[2], rgb(0xf5e0dc), "the cursor");
        assert_eq!(colors[20], rgb(0x89b4fa), "the accent is blue");
    }

    #[test]
    fn file_names_refuse_separators_and_dots() {
        assert_eq!(file_name("  My Theme "), Ok("My Theme.toml".into()));
        assert_eq!(file_name("Frappé"), Ok("Frappé.toml".into()));
        assert_eq!(file_name("Console"), Ok("Console.toml".into()));
        assert_eq!(file_name("com0"), Ok("com0.toml".into()));
        for bad in [
            "",
            "   ",
            ".hidden",
            "a/b",
            "a\\b",
            "a:b",
            "what?",
            "tab\tname",
            "nul",
            "Con",
            "com1",
            "LPT9",
        ] {
            assert!(file_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn create_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = create(dir.path(), "Mine", "one").unwrap();
        assert_eq!(path, dir.path().join("themes").join("Mine.toml"));
        assert_eq!(
            create(dir.path(), " Mine ", "two"),
            Err("A theme named \"Mine\" already exists".into())
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one");
        assert!(create(dir.path(), "a/b", "x").is_err());
    }

    #[test]
    fn unknown_tables_are_ignored() {
        let text = format!(
            "{MINIMAL}\n[colors.search.matches]\nforeground = \"#ffffff\"\n\n[window]\nopacity = 0.9\n"
        );
        assert!(parse("x", &text).is_ok());
    }

    #[test]
    fn invalid_toml_is_an_error() {
        assert!(matches!(parse("x", "[colors"), Err(ThemeError::Toml(_))));
    }

    #[test]
    fn parses_bom_and_crlf() {
        let windows = format!("\u{feff}{}", MINIMAL.replace('\n', "\r\n"));
        assert_eq!(parse("x", &windows).unwrap().palette.ansi[1], rgb(0xaa0000));
    }

    #[test]
    fn derived_colors() {
        let mocha = parse("Catppuccin Mocha", MOCHA).unwrap();
        assert_eq!(mocha.ui.accent, hsla(rgb(0x89b4fa), 1.0));
        assert_eq!(mocha.ui.canvas, hsla(rgb(0x151520), 1.0));
        assert_eq!(mocha.ui.dot, hsla(rgb(0xcdd6f4), 0.30));
        assert_eq!(mocha.ui.surface, hsla(rgb(0x1e1e2e), 1.0));
        assert_eq!(mocha.ui.ring, hsla(rgb(0x89b4fa), 0.40));
        assert_eq!(mocha.ui.hover_border, hsla(rgb(0xcdd6f4), 0.22));
        assert_eq!(mocha.ui.pressed, hsla(rgb(0xcdd6f4), 0.14));
        assert_eq!(mocha.ui.card, hsla(rgb(0xcdd6f4), 0.04));
        assert_eq!(mocha.ui.selection, hsla(rgb(0xf5e0dc), 0.35));
        assert_eq!(mocha.ui.cursor, hsla(rgb(0xf5e0dc), 1.0));
        assert_eq!(mocha.ui.shadow, hsla(rgb(0), 0.45));
        let latte = parse("Catppuccin Latte", LATTE).unwrap();
        assert!(!latte.is_dark);
        assert_eq!(latte.ui.canvas, hsla(rgb(0xe5e7eb), 1.0));
        assert_eq!(
            latte.ui.dot,
            hsla(rgb(0x4c4f69), 0.22),
            "lighter dots on light themes"
        );
        assert_eq!(latte.ui.shadow, hsla(rgb(0), 0.15));
        let minimal = parse("Minimal", MINIMAL).unwrap();
        assert_eq!(
            minimal.ui.selection,
            hsla(rgb(0x0000aa), 0.35),
            "no selection: blue"
        );
        assert_eq!(
            minimal.ui.cursor,
            hsla(rgb(0xe0e0e0), 1.0),
            "no cursor: foreground"
        );
        assert_eq!(minimal.ui.marked, hsla(rgb(0xaaaa00), 1.0));
    }
    #[test]
    fn ui_colors_blend_token_by_token() {
        use gpui_kit::base::animation::Lerp as _;
        let mocha = parse("Catppuccin Mocha", MOCHA).unwrap().ui;
        let latte = parse("Catppuccin Latte", LATTE).unwrap().ui;
        assert_eq!(mocha.lerp(&latte, 0.0), mocha);
        assert_eq!(mocha.lerp(&latte, 1.0), latte);
        let half = mocha.lerp(&latte, 0.5);
        assert!((half.canvas.l - (mocha.canvas.l + latte.canvas.l) / 2.0).abs() < 1e-6);
        assert!((half.dot.a - 0.26).abs() < 1e-6, "alphas blend too");
    }

    #[test]
    fn note_tints_keep_text_readable() {
        let contrast = |a: Rgb, b: Rgb| {
            let (la, lb) = (luminance(a), luminance(b));
            (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
        };
        for builtin in BUILTIN {
            let theme = parse(builtin.name, builtin.toml).unwrap();
            for label in Label::ALL {
                let tint = note_tint(&theme.palette, label);
                let ratio = contrast(theme.palette.foreground, tint);
                assert!(ratio >= 4.5, "{} {}: {ratio}", builtin.name, label.name());
            }
        }
    }

    #[test]
    fn all_builtins_load_in_order() {
        let all = load(None);
        assert_eq!(all.len(), BUILTIN.len());
        for (entry, builtin) in all.iter().zip(&BUILTIN) {
            assert_eq!(entry.theme.name, builtin.name);
            assert_eq!(entry.group, builtin.group);
        }
        // Frappé, Macchiato, Mocha, One Dark, Apple Dark.
        assert_eq!(all.iter().filter(|e| e.theme.is_dark).count(), 5);
    }

    #[test]
    fn pick_falls_back_to_the_default() {
        let all = load(None);
        assert_eq!(pick(&all, "One Dark", DEFAULT_DARK).name, "One Dark");
        assert_eq!(
            pick(&all, "No Such Theme", DEFAULT_DARK).name,
            "Catppuccin Mocha"
        );
        assert_eq!(default_name(false), "Catppuccin Latte");
        assert_eq!(default_name(true), "Catppuccin Mocha");
    }

    #[test]
    fn user_themes_load_sorted_and_win_over_builtins() {
        let dir = tempfile::tempdir().unwrap();
        let themes = dir.path().join("themes");
        std::fs::create_dir(&themes).unwrap();
        std::fs::write(themes.join("zeta.toml"), MINIMAL).unwrap();
        std::fs::write(themes.join("One Dark.toml"), MINIMAL).unwrap();
        std::fs::write(themes.join("broken.toml"), "[colors").unwrap();
        std::fs::write(themes.join("notes.txt"), MINIMAL).unwrap();
        let all = load(Some(dir.path()));
        let mine: Vec<&str> = all
            .iter()
            .filter(|e| e.group == MY_THEMES)
            .map(|e| e.theme.name.as_str())
            .collect();
        assert_eq!(mine, ["One Dark", "zeta"]);
        assert_eq!(
            all.len(),
            BUILTIN.len() + 1,
            "One Dark replaced, zeta added"
        );
        assert_eq!(
            find(&all, "One Dark").unwrap().palette.background,
            rgb(0x101010)
        );
    }

    #[test]
    fn a_missing_themes_folder_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(Some(dir.path())).len(), BUILTIN.len());
    }
}
