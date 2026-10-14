//! The saved canvases: one JSON file per workspace in the platform's data folder, read at startup
//! and written by snapshots, and the index that lists the workspaces (see
//! docs/specs/persistence.md and docs/specs/workspaces.md).

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use templum_canvas::{
    BlockId, Camera, Color, Drawing, MAX_WIDTH, MIN_WIDTH, Point, Rect, ShapeKind, Split, Tile,
};
use templum_terminal::Processes;

use crate::config;

/// The file format's version: raised only when an older Templum couldn't read the new files.
pub(crate) const VERSION: u32 = 1;
/// How often a snapshot is taken (and written when it changed).
pub(crate) const EVERY: Duration = Duration::from_secs(5);
/// The first workspace's id: the file the version without workspaces wrote.
pub(crate) const DEFAULT_ID: &str = "default";

/// Templum's data folder: `$XDG_DATA_HOME/templum` or `~/.local/share/templum` on Linux,
/// `~/Library/Application Support/Templum` on macOS, `%APPDATA%\templum` on Windows.
pub(crate) fn dir() -> Option<PathBuf> {
    dir_from(
        std::env::consts::OS,
        std::env::var_os("APPDATA"),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::home_dir(),
    )
}

fn dir_from(
    os: &str,
    appdata: Option<OsString>,
    xdg: Option<OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    match os {
        "windows" => appdata.map(|d| PathBuf::from(d).join("templum")),
        "macos" => home.map(|h| {
            h.join("Library")
                .join("Application Support")
                .join("Templum")
        }),
        _ => xdg
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".local").join("share")))
            .map(|d| d.join("templum")),
    }
}

/// A workspace's canvas file.
pub(crate) fn path(dir: &Path, id: &str) -> PathBuf {
    dir.join("workspaces").join(format!("{id}.json"))
}

pub(crate) fn index_path(dir: &Path) -> PathBuf {
    dir.join("workspaces").join("index.json")
}

/// The workspaces, in the sidebar's order, and what the window showed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Index {
    pub version: u32,
    pub workspaces: Vec<IndexEntry>,
    /// The active workspace's position.
    #[serde(default)]
    pub active: usize,
    /// The sidebar is shown.
    #[serde(default = "shown")]
    pub sidebar: bool,
}

fn shown() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct IndexEntry {
    /// Names its file; chosen once, never changed.
    pub id: String,
    pub name: String,
    /// A Lucide icon's name; one Templum doesn't know is none.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "known_icon"
    )]
    pub icon: Option<String>,
    /// One of `space_dialog::COLORS`; another is none.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "known_color"
    )]
    pub color: Option<String>,
    /// A file in `workspaces/icons/`, shown instead of the icon; a name that could leave the
    /// folder is none.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "safe_image"
    )]
    pub image: Option<String>,
    /// Where its new terminals start.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "any_folder"
    )]
    pub folder: Option<PathBuf>,
    /// Its programs were ended; opening it starts them again.
    #[serde(
        default,
        skip_serializing_if = "std::ops::Not::not",
        deserialize_with = "any_true"
    )]
    pub stopped: bool,
}

// The keys a workspace's look added: a value Templum can't use (a typo, a newer version's) is
// none, never a reason to set the whole list aside.

/// A string, or none for anything else.
fn any_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    })
}

fn known_icon<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(any_string(d)?.filter(|n| crate::space_dialog::icon(n).is_some()))
}

fn known_color<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(any_string(d)?.filter(|n| crate::space_dialog::COLORS.contains(&n.as_str())))
}

fn safe_image<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(any_string(d)?.filter(|n| safe_file_name(n)))
}

fn any_folder<'de, D: Deserializer<'de>>(d: D) -> Result<Option<PathBuf>, D::Error> {
    Ok(any_string(d)?.map(PathBuf::from))
}

fn any_true<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(serde_json::Value::deserialize(d)? == serde_json::Value::Bool(true))
}

/// Letters, digits, `-`, `_` and `.`, and no `..`: a name that stays in its folder.
fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

impl Index {
    /// No index yet (a first run, or the version without workspaces): one workspace, whose file
    /// is the one that version wrote.
    pub fn first_run() -> Index {
        Index {
            version: VERSION,
            workspaces: vec![IndexEntry {
                id: DEFAULT_ID.into(),
                name: "Main".into(),
                ..IndexEntry::default()
            }],
            active: 0,
            sidebar: true,
        }
    }
}

/// Letters, digits, `-` and `_` only: an edited index can't name a file outside the folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The id for a new workspace: the smallest `w<n>` from 2 that isn't `taken`.
pub(crate) fn next_id(taken: impl Fn(&str) -> bool) -> String {
    (2..)
        .map(|n| format!("w{n}"))
        .find(|id| !taken(id))
        .expect("an unused id")
}

/// An id is taken by a workspace in the index, or by a file left in the folder (a deleted
/// workspace's `.bak`): a new workspace must never open an old canvas.
pub(crate) fn id_in_folder(dir: &Path, id: &str) -> bool {
    let file = path(dir, id);
    file.exists() || file.with_extension("json.bak").exists()
}

pub(crate) fn load_index(dir: &Path) -> Loaded<Index> {
    match read::<Index>(&index_path(dir)) {
        Loaded::Saved(mut index) => {
            let mut seen = Vec::new();
            index.workspaces.retain(|w| {
                let keep = valid_id(&w.id) && !seen.contains(&w.id);
                seen.push(w.id.clone());
                keep
            });
            if index.workspaces.is_empty() {
                return Loaded::Saved(Index::first_run());
            }
            index.active = index.active.min(index.workspaces.len() - 1);
            // The one shown is never stopped.
            index.workspaces[index.active].stopped = false;
            Loaded::Saved(index)
        }
        other => other,
    }
}

pub(crate) fn save_index(dir: &Path, index: &Index) -> io::Result<()> {
    let path = index_path(dir);
    let text = serde_json::to_string_pretty(index).map_err(io::Error::other)?;
    std::fs::create_dir_all(path.parent().expect("a workspaces folder"))?;
    config::write_whole(&path, &text)
}

/// The images workspaces show instead of their icons.
pub(crate) fn icons_dir(dir: &Path) -> PathBuf {
    dir.join("workspaces").join("icons")
}

/// The image kinds a workspace can show.
pub(crate) const IMAGE_KINDS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "svg"];

/// The largest image a workspace takes.
pub(crate) const IMAGE_MAX: u64 = 4 * 1024 * 1024;

/// Why `file` can't be a workspace's image: not one of IMAGE_KINDS, too big, or unreadable.
pub(crate) fn image_refused(file: &Path) -> Option<String> {
    let kind = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !IMAGE_KINDS.contains(&kind.as_str()) {
        return Some("Choose a PNG, JPG, WebP or SVG image".into());
    }
    match std::fs::metadata(file) {
        Ok(m) if m.len() > IMAGE_MAX => Some("That image is over 4 MB".into()),
        Ok(_) => None,
        Err(err) => Some(format!("Couldn't read the image: {err}")),
    }
}

/// Copies `file` as workspace `id`'s image, `<id>.<kind>` in `icons_dir`, in place of an older
/// one; gives its name.
pub(crate) fn copy_icon(dir: &Path, id: &str, file: &Path) -> io::Result<String> {
    if let Some(why) = image_refused(file) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, why));
    }
    let kind = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let icons = icons_dir(dir);
    std::fs::create_dir_all(&icons)?;
    let name = format!("{id}.{kind}");
    // Copied aside first: the file chosen may be the older image itself.
    let part = icons.join(format!("{name}.part"));
    std::fs::copy(file, &part)?;
    for older in IMAGE_KINDS {
        remove_icon(dir, &format!("{id}.{older}"))?;
    }
    std::fs::rename(&part, icons.join(&name))?;
    Ok(name)
}

/// Removes a workspace's image (`name` in `icons_dir`); one already gone is fine.
pub(crate) fn remove_icon(dir: &Path, name: &str) -> io::Result<()> {
    if !safe_file_name(name) {
        return Ok(());
    }
    match std::fs::remove_file(icons_dir(dir).join(name)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// A deleted workspace's canvas is kept as `<id>.json.bak`, replacing an older one.
pub(crate) fn set_aside(dir: &Path, id: &str) -> io::Result<()> {
    let path = path(dir, id);
    match std::fs::rename(&path, path.with_extension("json.bak")) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub version: u32,
    pub camera: SavedCamera,
    /// Back to front: the last is drawn on top.
    pub items: Vec<Item>,
    /// The focused item's index in `items`.
    #[serde(default)]
    pub focused: Option<usize>,
    /// The folders agents were last opened in on this canvas, newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent: Vec<String>,
    /// The drawings, above the blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ink: Vec<InkItem>,
    /// Grid mode's tiles, by item index (docs/specs/grid.md); the camera and the items' rects are
    /// the canvas's still.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<SavedTile>,
}

/// Grid mode's tree of tiles: an item, by its index in `items`, or a pair. `split` is "beside"
/// (side by side), anything else one above the other: a word no Templum knows never loses the
/// file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum SavedTile {
    Item(usize),
    Pair {
        split: String,
        a: Box<SavedTile>,
        b: Box<SavedTile>,
    },
}

impl SavedTile {
    /// `tile` with each block as its item's index (`index`); None when a block isn't an item.
    pub fn of(tile: &Tile, index: &impl Fn(BlockId) -> Option<usize>) -> Option<SavedTile> {
        Some(match tile {
            Tile::Leaf(id) => SavedTile::Item(index(*id)?),
            Tile::Pair(split, a, b) => SavedTile::Pair {
                split: match split {
                    Split::Beside => "beside",
                    Split::Above => "above",
                }
                .to_string(),
                a: Box::new(SavedTile::of(a, index)?),
                b: Box::new(SavedTile::of(b, index)?),
            },
        })
    }

    /// The tiles, `block` giving each item's block: items that didn't come back are left out (a
    /// pair left with one half is that half). None without any, or when an item is in it twice.
    pub fn tile(&self, block: impl Fn(usize) -> Option<BlockId> + Copy) -> Option<Tile> {
        let mut items = Vec::new();
        self.items(&mut items);
        let count = items.len();
        items.sort_unstable();
        items.dedup();
        if items.len() != count {
            return None;
        }
        self.build(block)
    }

    fn items(&self, into: &mut Vec<usize>) {
        match self {
            SavedTile::Item(i) => into.push(*i),
            SavedTile::Pair { a, b, .. } => {
                a.items(into);
                b.items(into);
            }
        }
    }

    fn build(&self, block: impl Fn(usize) -> Option<BlockId> + Copy) -> Option<Tile> {
        match self {
            SavedTile::Item(i) => block(*i).map(Tile::Leaf),
            SavedTile::Pair { split, a, b } => match (a.build(block), b.build(block)) {
                (Some(a), Some(b)) => Some(Tile::Pair(
                    if split == "beside" {
                        Split::Beside
                    } else {
                        Split::Above
                    },
                    Box::new(a),
                    Box::new(b),
                )),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            },
        }
    }
}

/// The world point at the canvas's top left, and the zoom factor.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedCamera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum Item {
    Terminal(TerminalItem),
    Kanban(KanbanItem),
    Note(NoteItem),
    Editor(EditorItem),
    Git(GitItem),
}

impl Item {
    pub fn rect_mut(&mut self) -> &mut SavedRect {
        match self {
            Item::Terminal(t) => &mut t.rect,
            Item::Kanban(k) => &mut k.rect,
            Item::Note(n) => &mut n.rect,
            Item::Editor(e) => &mut e.rect,
            Item::Git(g) => &mut g.rect,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct NoteItem {
    pub rect: SavedRect,
    /// The notes it shows as tabs, by their ids in `notes.json`, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<u64>,
    /// The tab on screen, by its index in `notes`.
    #[serde(default)]
    pub active: usize,
    /// A file from before `notes.json`: its one note's text, moved there when it is read.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Before `notes.json` too: that note's color, by name ("yellow"); none when missing or
    /// unknown.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::board::known_label"
    )]
    pub color: Option<crate::board::Label>,
}

/// An editor block (docs/specs/cockpit.md, section 7).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct EditorItem {
    pub rect: SavedRect,
    pub folder: String,
    /// The open files, relative to the folder, in tab order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open: Vec<String>,
    /// The active tab's index in `open`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<usize>,
    /// The file tree is shown.
    #[serde(default = "shown")]
    pub tree: bool,
}

/// A Git block (docs/specs/cockpit.md, section 8).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct GitItem {
    pub rect: SavedRect,
    pub folder: String,
    /// The tab shown: "changes", "history", "branches" or "stashes".
    #[serde(default = "changes")]
    pub tab: String,
    /// The commit message being written, left out when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// "folded": the left column is folded. Left out when it isn't.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lists: String,
    /// "split": the diff panels are side by side. Left out when they aren't.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub diff: String,
}

fn changes() -> String {
    "changes".to_string()
}

/// A drawing as the file holds it (docs/specs/cockpit.md, section 4): a `stroke` with `points`,
/// or a `line`, `arrow`, `rect` or `ellipse` with `from` and `to`. Points are `[x, y]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct InkItem {
    pub kind: String,
    pub color: String,
    pub width: f32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub highlight: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<[f32; 2]>,
}

impl InkItem {
    pub fn of(drawing: &Drawing) -> InkItem {
        let xy = |p: &Point| [p.x, p.y];
        match drawing {
            Drawing::Stroke {
                points,
                width,
                color,
                highlight,
            } => InkItem {
                kind: "stroke".into(),
                color: color.name().into(),
                width: *width,
                highlight: *highlight,
                points: points.iter().map(xy).collect(),
                from: None,
                to: None,
            },
            Drawing::Shape {
                kind,
                from,
                to,
                width,
                color,
            } => InkItem {
                kind: kind.name().into(),
                color: color.name().into(),
                width: *width,
                highlight: false,
                points: Vec::new(),
                from: Some(xy(from)),
                to: Some(xy(to)),
            },
        }
    }

    /// None for an unknown kind, a stroke under 2 points, a shape missing an end, or a number
    /// that isn't finite. An unknown color is the foreground; the width is clamped.
    pub fn drawing(&self) -> Option<Drawing> {
        let color = Color::from_name(&self.color).unwrap_or(Color::Foreground);
        if !self.width.is_finite() {
            return None;
        }
        let width = self.width.clamp(MIN_WIDTH, MAX_WIDTH);
        let point =
            |p: [f32; 2]| (p[0].is_finite() && p[1].is_finite()).then(|| Point::new(p[0], p[1]));
        if self.kind == "stroke" {
            let points = self
                .points
                .iter()
                .map(|p| point(*p))
                .collect::<Option<Vec<_>>>()?;
            if points.len() < 2 {
                return None;
            }
            return Some(Drawing::Stroke {
                points,
                width,
                color,
                highlight: self.highlight,
            });
        }
        Some(Drawing::Shape {
            kind: ShapeKind::from_name(&self.kind)?,
            from: point(self.from?)?,
            to: point(self.to?)?,
            width,
            color,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct KanbanItem {
    pub rect: SavedRect,
    /// The board it shows; missing while it shows the board list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct TerminalItem {
    pub rect: SavedRect,
    /// Missing when the OS wouldn't say, or when the path isn't UTF-8 (JSON can't hold it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The agent open in it, by id: resumed in the block's folder when Templum knows that id
    /// (one of its own, or one in `[agents]`); any other value is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The agent's session, by its id: the one resumed, so blocks in the same folder each get
    /// their own back. Missing when the agent's files don't tell it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl From<Rect> for SavedRect {
    fn from(r: Rect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }
    }
}

impl From<SavedRect> for Rect {
    fn from(r: SavedRect) -> Self {
        Rect::new(r.x, r.y, r.w, r.h)
    }
}

/// The canvas as the UI thread has it: what a snapshot needs before the processes are read.
pub(crate) struct Layout {
    pub camera: Camera,
    /// Back to front.
    pub blocks: Vec<LayoutBlock>,
    pub focused: Option<usize>,
    /// Folders agents were opened in, newest first.
    pub recent: Vec<PathBuf>,
    /// The agents to look for in the terminals.
    pub agents: std::sync::Arc<Vec<templum_terminal::Detect>>,
    pub ink: Vec<InkItem>,
    pub grid: Option<SavedTile>,
}

pub(crate) enum LayoutBlock {
    /// A terminal's rect and its processes, read in the background.
    Terminal(Rect, Processes),
    /// Any other kind: its item, complete, built on the UI thread.
    Saved(Item),
}

impl Layout {
    /// Reads the processes (folders, agents, and the agents' sessions in their files, under
    /// Templum's data folder `data`) and builds the file's content, with what was found in each
    /// terminal, in order. Slow: off the UI thread.
    pub fn snapshot(self, data: Option<&Path>) -> (Saved, Vec<templum_terminal::Inspection>) {
        let processes: Vec<Processes> = self
            .blocks
            .iter()
            .filter_map(|b| match b {
                LayoutBlock::Terminal(_, p) => Some(p.clone()),
                LayoutBlock::Saved(_) => None,
            })
            .collect();
        let inspections = templum_terminal::inspect(&processes, &self.agents);
        let sessions = data.map_or_else(Vec::new, |data| session_ids(&inspections, data));
        let mut sessions = sessions.into_iter();
        let mut found = inspections.iter().cloned();
        let saved = Saved {
            version: VERSION,
            camera: SavedCamera {
                x: self.camera.origin.x,
                y: self.camera.origin.y,
                zoom: self.camera.zoom(),
            },
            items: self
                .blocks
                .iter()
                .map(|b| match b {
                    LayoutBlock::Terminal(rect, _) => {
                        // One answer per terminal, in order.
                        let found = found.next().unwrap_or_default();
                        Item::Terminal(TerminalItem {
                            rect: (*rect).into(),
                            folder: found.folder.and_then(utf8),
                            agent: found.agent,
                            session: sessions.next().flatten(),
                        })
                    }
                    LayoutBlock::Saved(item) => item.clone(),
                })
                .collect(),
            focused: self.focused,
            recent: self.recent.into_iter().filter_map(utf8).collect(),
            ink: self.ink,
            grid: self.grid,
        };
        (saved, inspections)
    }
}

/// The session each terminal's agent has open, in order, from the agents' files.
fn session_ids(found: &[templum_terminal::Inspection], data: &Path) -> Vec<Option<String>> {
    let procs: Vec<templum_usage::Proc> = found
        .iter()
        .map(|f| templum_usage::Proc {
            agent: f.agent.clone().unwrap_or_default(),
            pid: f.pid.unwrap_or(0),
            started: f.started.unwrap_or(0),
            cwd: f.folder.clone(),
            // The relay's file isn't read for a session's id.
            terminal: String::new(),
        })
        .collect();
    let dirs = templum_usage::Dirs::of(data);
    templum_usage::session_ids(&procs, &dirs, crate::notes::now())
}

/// A path as JSON can hold it.
pub(crate) fn utf8(path: PathBuf) -> Option<String> {
    path.into_os_string().into_string().ok()
}

/// What reading a saved file found.
#[derive(Debug, PartialEq)]
pub(crate) enum Loaded<T> {
    /// No file: a first run.
    Nothing,
    Saved(T),
    /// A file this Templum can't use, renamed to `to` so it is never written over.
    Moved {
        to: PathBuf,
        why: String,
    },
    /// The file couldn't be read, or set aside: nothing may be written this session.
    Unreadable(String),
}

/// A workspace's canvas.
pub(crate) fn load(dir: &Path, id: &str) -> Loaded<Saved> {
    read(&path(dir, id))
}

/// A file with a `version`: one this Templum can't use (newer, broken) is set aside as `.bak`.
pub(crate) fn read<T: DeserializeOwned>(path: &Path) -> Loaded<T> {
    let path = path.to_path_buf();
    // Bytes, not a string: a file that isn't UTF-8 is bad content to set aside, not a read failure.
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Loaded::Nothing,
        Err(e) => return Loaded::Unreadable(format!("{}: {e}", path.display())),
    };
    #[derive(Deserialize)]
    struct Version {
        version: u32,
    }
    let why = match serde_json::from_slice::<Version>(&bytes) {
        Ok(v) if v.version > VERSION => "it is from a newer Templum".to_string(),
        _ => match serde_json::from_slice::<T>(&bytes) {
            Ok(saved) => return Loaded::Saved(saved),
            Err(e) => e.to_string(),
        },
    };
    let to = path.with_extension("json.bak");
    match std::fs::rename(&path, &to) {
        Ok(()) => Loaded::Moved { to, why },
        Err(e) => Loaded::Unreadable(format!(
            "{}: {why}, and it couldn't be set aside: {e}",
            path.display()
        )),
    }
}

/// Writes a workspace's canvas through [`write_newest`].
pub(crate) fn save(dir: &Path, id: &str, saved: &Saved, generation: u64) -> io::Result<()> {
    let text = serde_json::to_string_pretty(saved).map_err(io::Error::other)?;
    write_newest(&path(dir, id), &text, generation)
}

/// Writes `text` to `path` (creating its folder). Writes take turns, and one older than the last
/// written to the same file is dropped: a background snapshot that finishes after the one taken
/// on quit.
pub(crate) fn write_newest(path: &Path, text: &str, generation: u64) -> io::Result<()> {
    static WRITTEN: LazyLock<Mutex<HashMap<PathBuf, u64>>> = LazyLock::new(Default::default);
    let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    let last = written.entry(path.to_path_buf()).or_default();
    if generation < *last {
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().expect("a data folder"))?;
    config::write_whole(path, text)?;
    *last = generation;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_color_reads_back_and_unknown_or_missing_is_none() {
        let read = |json: &str| match serde_json::from_str::<Item>(json).unwrap() {
            Item::Note(n) => n,
            other => panic!("{other:?}"),
        };
        let base = r#"{"kind":"note","rect":{"x":0,"y":0,"w":1,"h":1},"text":"x""#;
        assert_eq!(read(&format!("{base}}}")).color, None);
        assert_eq!(read(&format!(r#"{base},"color":"teal"}}"#)).color, None);
        let note = read(&format!(r#"{base},"color":"yellow"}}"#));
        assert_eq!(note.color, Some(crate::board::Label::Yellow));
        let back = serde_json::to_string(&Item::Note(note)).unwrap();
        assert!(back.contains(r#""color":"yellow""#), "{back}");
        let plain = serde_json::to_string(&Item::Note(read(&format!("{base}}}")))).unwrap();
        assert!(!plain.contains("color"), "{plain}");
    }

    #[test]
    fn a_git_item_round_trips_and_its_tab_defaults_to_changes() {
        let text = r#"{"kind":"git","rect":{"x":1,"y":2,"w":800,"h":560},"folder":"/p"}"#;
        let item: Item = serde_json::from_str(text).unwrap();
        let Item::Git(g) = &item else {
            panic!("{item:?}")
        };
        assert_eq!(g.folder, "/p");
        assert_eq!(g.tab, "changes");
        let back = serde_json::to_string(&item).unwrap();
        assert!(back.contains(r#""kind":"git""#), "{back}");
        assert!(back.contains(r#""tab":"changes""#), "{back}");
        assert!(!back.contains("summary"), "no message, no key: {back}");
        assert!(!back.contains("lists"), "not folded, no key: {back}");
        assert!(!back.contains(r#""diff""#), "unified, no key: {back}");
        let mut split = g.clone();
        split.diff = "split".into();
        let back = serde_json::to_string(&Item::Git(split)).unwrap();
        assert!(back.contains(r#""diff":"split""#), "{back}");
        let item: Item =
            serde_json::from_str(&text.replace(r#""folder""#, r#""tab":"history","folder""#))
                .unwrap();
        let Item::Git(g) = item else { unreachable!() };
        assert_eq!(g.tab, "history");
        assert_eq!((g.summary.as_str(), g.description.as_str()), ("", ""));
        // A commit message being written comes back.
        let mut draft = g.clone();
        draft.summary = "Fix the build".into();
        draft.description = "It broke.".into();
        let back = serde_json::to_string(&Item::Git(draft.clone())).unwrap();
        let Item::Git(g) = serde_json::from_str(&back).unwrap() else {
            unreachable!()
        };
        assert_eq!(g, draft);
    }

    fn sample() -> Saved {
        Saved {
            version: VERSION,
            camera: SavedCamera {
                x: -120.0,
                y: 40.0,
                zoom: 1.25,
            },
            items: vec![
                Item::Terminal(TerminalItem {
                    rect: SavedRect {
                        x: 0.0,
                        y: 0.0,
                        w: 720.0,
                        h: 460.0,
                    },
                    folder: Some("/home/u/proj".into()),
                    agent: Some("claude".into()),
                    session: Some("0d5e5f4b-1c2d-4e3f-8a9b-0c1d2e3f4a5b".into()),
                }),
                Item::Terminal(TerminalItem {
                    rect: SavedRect {
                        x: 800.0,
                        y: 10.0,
                        w: 500.0,
                        h: 300.0,
                    },
                    folder: None,
                    agent: None,
                    session: None,
                }),
                Item::Kanban(KanbanItem {
                    rect: SavedRect {
                        x: -900.0,
                        y: 0.0,
                        w: 900.0,
                        h: 540.0,
                    },
                    board: Some(7),
                }),
            ],
            focused: Some(1),
            recent: vec!["/home/u/proj".into(), "/srv/site".into()],
            ink: Vec::new(),
            grid: None,
        }
    }

    #[test]
    fn an_editor_item_round_trips_and_its_tree_defaults_to_shown() {
        let text = r#"{"kind":"editor","rect":{"x":1,"y":2,"w":900,"h":600},"folder":"/p","open":["src/main.rs"],"active":0}"#;
        let item: Item = serde_json::from_str(text).unwrap();
        let Item::Editor(e) = &item else {
            panic!("{item:?}")
        };
        assert!(e.tree, "missing: shown");
        assert_eq!(e.open, ["src/main.rs"]);
        assert_eq!(e.active, Some(0));
        let back = serde_json::to_string(&item).unwrap();
        assert!(back.contains(r#""kind":"editor""#), "{back}");
        assert!(back.contains(r#""tree":true"#), "{back}");
    }

    #[test]
    fn dir_follows_each_platform() {
        let home = Some(PathBuf::from("/home/u"));
        assert_eq!(
            dir_from("linux", None, Some("/x".into()), home.clone()),
            Some(PathBuf::from("/x").join("templum"))
        );
        assert_eq!(
            dir_from("linux", None, Some("".into()), home.clone()),
            Some(
                PathBuf::from("/home/u")
                    .join(".local")
                    .join("share")
                    .join("templum")
            )
        );
        assert_eq!(
            dir_from("macos", None, None, home.clone()),
            Some(
                PathBuf::from("/home/u")
                    .join("Library")
                    .join("Application Support")
                    .join("Templum")
            )
        );
        assert_eq!(
            dir_from(
                "windows",
                Some("C:/Users/u/AppData/Roaming".into()),
                None,
                home
            ),
            Some(PathBuf::from("C:/Users/u/AppData/Roaming").join("templum"))
        );
        assert_eq!(dir_from("linux", None, None, None), None);
    }

    #[test]
    fn a_saved_canvas_reads_back_equal() {
        let d = tempfile::tempdir().unwrap();
        save(d.path(), DEFAULT_ID, &sample(), 1).unwrap();
        assert_eq!(load(d.path(), DEFAULT_ID), Loaded::Saved(sample()));
    }

    #[test]
    fn the_file_is_the_spec_format() {
        let text = serde_json::to_string(&sample()).unwrap();
        for part in [
            "\"version\":1",
            "\"camera\":{\"x\":-120.0,\"y\":40.0,\"zoom\":1.25}",
            "\"kind\":\"terminal\"",
            "\"rect\":{\"x\":0.0,\"y\":0.0,\"w\":720.0,\"h\":460.0}",
            "\"folder\":\"/home/u/proj\"",
            "\"agent\":\"claude\"",
            "\"session\":\"0d5e5f4b-1c2d-4e3f-8a9b-0c1d2e3f4a5b\"",
            "\"kind\":\"kanban\"",
            "\"board\":7",
            "\"focused\":1",
            "\"recent\":[\"/home/u/proj\",\"/srv/site\"]",
        ] {
            assert!(text.contains(part), "{part} in {text}");
        }
        assert_eq!(
            text.matches("folder").count(),
            1,
            "a missing folder isn't written"
        );
        assert_eq!(text.matches("agent").count(), 1, "nor a missing agent");
        assert_eq!(text.matches("session").count(), 1, "nor a missing session");
    }

    #[test]
    fn no_file_is_a_first_run() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load(d.path(), DEFAULT_ID), Loaded::Nothing);
    }

    fn write(dir: &Path, text: &str) {
        std::fs::create_dir_all(dir.join("workspaces")).unwrap();
        std::fs::write(path(dir, DEFAULT_ID), text).unwrap();
    }

    fn moved(dir: &Path, text: &str) -> String {
        write(dir, text);
        let Loaded::Moved { to, why } = load(dir, DEFAULT_ID) else {
            panic!("{text} wasn't set aside");
        };
        assert_eq!(to, path(dir, DEFAULT_ID).with_extension("json.bak"));
        assert_eq!(
            std::fs::read_to_string(&to).unwrap(),
            text,
            "the old bytes are kept"
        );
        assert!(!path(dir, DEFAULT_ID).exists());
        why
    }

    #[test]
    fn a_note_item_reads_back_and_an_old_one_keeps_its_text() {
        let d = tempfile::tempdir().unwrap();
        let mut saved = sample();
        saved.items.push(Item::Note(NoteItem {
            rect: SavedRect {
                x: 1.0,
                y: 2.0,
                w: 360.0,
                h: 280.0,
            },
            notes: vec![3, 5],
            active: 1,
            text: String::new(),
            color: None,
        }));
        save(d.path(), "n", &saved, 1).unwrap();
        assert_eq!(load(d.path(), "n"), Loaded::Saved(saved));
        let text = r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[
            {"kind":"note","rect":{"x":0,"y":0,"w":300,"h":200},"text":"t","preview":true}]}"#;
        std::fs::write(path(d.path(), "m"), text).unwrap();
        let Loaded::Saved(read) = load(d.path(), "m") else {
            panic!("not read");
        };
        assert!(matches!(&read.items[0], Item::Note(n) if n.text == "t" && n.notes.is_empty()));
    }

    #[test]
    fn a_grid_reads_back_by_item_index() {
        let d = tempfile::tempdir().unwrap();
        let mut saved = sample();
        let tile = Tile::Pair(
            Split::Beside,
            Box::new(Tile::Leaf(BlockId(7))),
            Box::new(Tile::Leaf(BlockId(3))),
        );
        let index = |id: BlockId| [BlockId(3), BlockId(7)].iter().position(|b| *b == id);
        saved.grid = SavedTile::of(&tile, &index);
        save(d.path(), "g", &saved, 1).unwrap();
        let text = std::fs::read_to_string(path(d.path(), "g")).unwrap();
        assert!(text.contains(r#""split": "beside""#), "{text}");
        let Loaded::Saved(read) = load(d.path(), "g") else {
            panic!("not read");
        };
        assert_eq!(read, saved);
        let back = read
            .grid
            .unwrap()
            .tile(|i| [BlockId(3), BlockId(7)].get(i).copied());
        assert_eq!(back, Some(tile));
    }

    #[test]
    fn a_grid_leaves_out_items_that_didnt_come_back() {
        let grid: SavedTile =
            serde_json::from_str(r#"{"split":"plaid","a":0,"b":{"split":"beside","a":1,"b":2}}"#)
                .unwrap();
        // Item 1 couldn't start: its pair is item 2 alone. An unknown split is one above the other.
        let tile = grid.tile(|i| (i != 1).then_some(BlockId(i as u64)));
        assert_eq!(
            tile,
            Some(Tile::Pair(
                Split::Above,
                Box::new(Tile::Leaf(BlockId(0))),
                Box::new(Tile::Leaf(BlockId(2)))
            ))
        );
        assert_eq!(grid.tile(|_| None), None, "nothing came back");
        let twice: SavedTile = serde_json::from_str(r#"{"split":"beside","a":0,"b":0}"#).unwrap();
        assert_eq!(
            twice.tile(|i| Some(BlockId(i as u64))),
            None,
            "an item twice"
        );
    }

    #[test]
    fn ink_round_trips_and_an_old_file_has_none() {
        use templum_canvas::{Color, Drawing, Point, ShapeKind};
        let d = tempfile::tempdir().unwrap();
        let drawings = vec![
            Drawing::Stroke {
                points: vec![Point::new(1.0, 2.0), Point::new(3.5, 4.0)],
                width: 4.0,
                color: Color::Red,
                highlight: false,
            },
            Drawing::Stroke {
                points: vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
                width: 16.0,
                color: Color::Yellow,
                highlight: true,
            },
            Drawing::Shape {
                kind: ShapeKind::Arrow,
                from: Point::new(0.0, 0.0),
                to: Point::new(120.0, 40.0),
                width: 2.0,
                color: Color::Foreground,
            },
        ];
        let mut saved = sample();
        saved.ink = drawings.iter().map(InkItem::of).collect();
        save(d.path(), "i", &saved, 1).unwrap();
        let Loaded::Saved(read) = load(d.path(), "i") else {
            panic!("not read");
        };
        assert_eq!(read, saved);
        let back: Vec<Drawing> = read.ink.iter().filter_map(InkItem::drawing).collect();
        assert_eq!(back, drawings);
        let text = std::fs::read_to_string(path(d.path(), "i")).unwrap();
        assert!(text.contains(r#""kind": "stroke""#) && text.contains(r#""highlight": true"#));
        assert_eq!(text.matches("highlight").count(), 1, "false is left out");
        // The file plan 1 wrote has no ink.
        let old = sample();
        save(d.path(), "o", &old, 1).unwrap();
        assert!(
            !std::fs::read_to_string(path(d.path(), "o"))
                .unwrap()
                .contains("ink")
        );
        assert_eq!(load(d.path(), "o"), Loaded::Saved(old));
    }

    #[test]
    fn bad_ink_entries_are_dropped_or_fixed() {
        use templum_canvas::{Color, Drawing};
        let item = |json: &str| serde_json::from_str::<InkItem>(json).unwrap().drawing();
        assert_eq!(
            item(r#"{"kind":"stroke","color":"plaid","width":1000,"points":[[0,0],[1,1]]}"#),
            Some(Drawing::Stroke {
                points: vec![
                    templum_canvas::Point::new(0.0, 0.0),
                    templum_canvas::Point::new(1.0, 1.0)
                ],
                width: 64.0,
                color: Color::Foreground,
                highlight: false,
            }),
            "unknown color: foreground; width clamped"
        );
        assert!(
            item(r#"{"kind":"stroke","color":"red","width":0,"points":[[0,0],[1,1]]}"#)
                .is_some_and(|d| d.width() == 1.0)
        );
        assert_eq!(
            item(r#"{"kind":"stroke","color":"red","width":4,"points":[[0,0]]}"#),
            None,
            "one point"
        );
        assert_eq!(
            item(r#"{"kind":"blob","color":"red","width":4,"from":[0,0],"to":[1,1]}"#),
            None
        );
        assert_eq!(
            item(r#"{"kind":"line","color":"red","width":4,"from":[0,0]}"#),
            None,
            "no end"
        );
        assert!(
            serde_json::from_str::<InkItem>(
                r#"{"kind":"stroke","color":"red","width":4,"points":[[0,0],[1,null]]}"#
            )
            .is_err(),
            "null isn't a number: the item doesn't parse, the file is set aside"
        );
        let nan = InkItem {
            kind: "line".into(),
            color: "red".into(),
            width: f32::NAN,
            highlight: false,
            points: Vec::new(),
            from: Some([0.0, 0.0]),
            to: Some([1.0, 1.0]),
        };
        assert_eq!(nan.drawing(), None, "a width that isn't a number");
    }

    #[test]
    fn a_file_this_templum_cant_use_is_set_aside() {
        let d = tempfile::tempdir().unwrap();
        moved(d.path(), "{nope");
        let newer = r#"{"version":2,"camera":{"x":0,"y":0,"zoom":1},"items":[]}"#;
        assert_eq!(moved(d.path(), newer), "it is from a newer Templum");
        let unknown = r#"{"version":1,"camera":{"x":0,"y":0,"zoom":1},"items":[{"kind":"sticker","text":"hi"}]}"#;
        moved(d.path(), unknown);
    }

    #[test]
    fn a_file_that_isnt_utf8_is_set_aside_too() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("workspaces")).unwrap();
        // UTF-16, as some Windows tools write.
        std::fs::write(path(d.path(), DEFAULT_ID), [0xff, 0xfe, b'{', 0]).unwrap();
        assert!(
            matches!(load(d.path(), DEFAULT_ID), Loaded::Moved { .. }),
            "not Unreadable: that would stop every future save"
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            r#"{"version":1,"later":true,"camera":{"x":1,"y":2,"zoom":1,"tilt":3},
               "items":[{"kind":"terminal","rect":{"x":0,"y":0,"w":10,"h":10},"color":"red"}]}"#,
        );
        let Loaded::Saved(saved) = load(d.path(), DEFAULT_ID) else {
            panic!("not loaded");
        };
        assert_eq!(saved.items.len(), 1);
        assert_eq!(saved.focused, None, "missing: nothing focused");
        assert!(saved.recent.is_empty(), "missing: no recent folders");
    }

    #[test]
    fn an_older_write_never_replaces_a_newer_one() {
        let d = tempfile::tempdir().unwrap();
        let mut newer = sample();
        newer.focused = Some(0);
        save(d.path(), DEFAULT_ID, &newer, 7).unwrap();
        save(d.path(), DEFAULT_ID, &sample(), 6).unwrap();
        assert_eq!(load(d.path(), DEFAULT_ID), Loaded::Saved(newer));
    }

    fn two() -> Index {
        Index {
            version: VERSION,
            workspaces: vec![
                IndexEntry {
                    id: DEFAULT_ID.into(),
                    name: "Main".into(),
                    ..IndexEntry::default()
                },
                IndexEntry {
                    id: "w2".into(),
                    name: "Side project".into(),
                    ..IndexEntry::default()
                },
            ],
            active: 1,
            sidebar: false,
        }
    }

    #[test]
    fn an_index_reads_back_equal() {
        let d = tempfile::tempdir().unwrap();
        save_index(d.path(), &two()).unwrap();
        assert_eq!(load_index(d.path()), Loaded::Saved(two()));
        let text = std::fs::read_to_string(index_path(d.path())).unwrap();
        assert!(text.contains("\"name\": \"Side project\""), "{text}");
    }

    #[test]
    fn an_index_entry_keeps_its_look_folder_and_state() {
        let d = tempfile::tempdir().unwrap();
        let mut index = two();
        index.active = 0;
        index.workspaces[1] = IndexEntry {
            icon: Some("rocket".into()),
            color: Some("blue".into()),
            image: Some("w2.png".into()),
            folder: Some(PathBuf::from("/home/me/side")),
            stopped: true,
            ..index.workspaces[1].clone()
        };
        save_index(d.path(), &index).unwrap();
        assert_eq!(load_index(d.path()), Loaded::Saved(index));
    }

    #[test]
    fn an_index_from_before_loads_and_saves_unchanged() {
        let d = tempfile::tempdir().unwrap();
        let text = r#"{"version":1,"workspaces":[{"id":"default","name":"Main"}],"active":0}"#;
        std::fs::create_dir_all(d.path().join("workspaces")).unwrap();
        std::fs::write(index_path(d.path()), text).unwrap();
        let Loaded::Saved(index) = load_index(d.path()) else {
            panic!("unread")
        };
        assert_eq!(index.workspaces[0], Index::first_run().workspaces[0]);
        let written = serde_json::to_string(&index).unwrap();
        for key in ["icon", "color", "image", "folder", "stopped"] {
            assert!(!written.contains(key), "{key} in {written}");
        }
    }

    #[test]
    fn an_unknown_color_or_icon_keeps_the_rest_of_the_index() {
        let d = tempfile::tempdir().unwrap();
        let text = r#"{"version":1,"workspaces":[
            {"id":"default","name":"Main","icon":"no-such-icon","color":"orange",
             "image":7,"folder":["x"],"stopped":"yes"},
            {"id":"w2","name":"Side","icon":"rocket","color":"accent","stopped":true}
        ],"active":1}"#;
        std::fs::create_dir_all(d.path().join("workspaces")).unwrap();
        std::fs::write(index_path(d.path()), text).unwrap();
        let Loaded::Saved(index) = load_index(d.path()) else {
            panic!("unread")
        };
        let (main, side) = (&index.workspaces[0], &index.workspaces[1]);
        assert_eq!(main.name, "Main");
        assert!(main.image.is_none() && main.folder.is_none() && !main.stopped);
        assert_eq!((main.icon.as_deref(), main.color.as_deref()), (None, None));
        assert_eq!(side.icon.as_deref(), Some("rocket"));
        assert_eq!(side.color.as_deref(), Some("accent"));
        assert!(!side.stopped, "the active one is never stopped");
    }

    #[test]
    fn unsafe_image_names_are_dropped() {
        let entry = |image: &str| {
            let text = format!(r#"{{"id":"w2","name":"x","image":{image:?}}}"#);
            serde_json::from_str::<IndexEntry>(&text).unwrap().image
        };
        assert_eq!(entry("w2.png").as_deref(), Some("w2.png"));
        for bad in ["../x.png", "a/b.png", "a\\b.png", "..", ""] {
            assert_eq!(entry(bad), None, "{bad}");
        }
    }

    #[test]
    fn copying_an_icon_replaces_the_older_one_and_refuses_big_files() {
        let d = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let png = src.path().join("a.PNG");
        std::fs::write(&png, b"png").unwrap();
        assert_eq!(copy_icon(d.path(), "w2", &png).unwrap(), "w2.png");
        let webp = src.path().join("b.webp");
        std::fs::write(&webp, b"webp").unwrap();
        assert_eq!(copy_icon(d.path(), "w2", &webp).unwrap(), "w2.webp");
        let mut left: Vec<_> = std::fs::read_dir(icons_dir(d.path()))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["w2.webp"]);
        // The older image chosen again is copied before it is removed.
        let again = icons_dir(d.path()).join("w2.webp");
        assert_eq!(copy_icon(d.path(), "w2", &again).unwrap(), "w2.webp");
        assert_eq!(std::fs::read(&again).unwrap(), b"webp");
        let big = src.path().join("big.jpg");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(IMAGE_MAX + 1)
            .unwrap();
        assert!(copy_icon(d.path(), "w2", &big).is_err());
        let text = src.path().join("a.txt");
        std::fs::write(&text, b"x").unwrap();
        assert!(copy_icon(d.path(), "w2", &text).is_err());
        remove_icon(d.path(), "w2.webp").unwrap();
        remove_icon(d.path(), "w2.webp").unwrap();
        assert!(!again.exists());
    }

    #[test]
    fn no_index_is_one_workspace_named_main_in_default_json() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load_index(d.path()), Loaded::Nothing);
        let first = Index::first_run();
        assert_eq!(first.workspaces.len(), 1);
        assert_eq!(first.workspaces[0].id, DEFAULT_ID);
        assert_eq!(first.workspaces[0].name, "Main");
        assert!(first.sidebar);
    }

    #[test]
    fn a_broken_index_is_set_aside() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("workspaces")).unwrap();
        std::fs::write(index_path(d.path()), "{nope").unwrap();
        assert!(matches!(load_index(d.path()), Loaded::Moved { .. }));
        assert!(index_path(d.path()).with_extension("json.bak").exists());
    }

    #[test]
    fn ids_that_could_leave_the_folder_are_dropped() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("workspaces")).unwrap();
        std::fs::write(
            index_path(d.path()),
            r#"{"version":1,"workspaces":[{"id":"../x","name":"A"},{"id":"a/b","name":"B"},
               {"id":"","name":"C"},{"id":"ok-1_2","name":"D"},{"id":"ok-1_2","name":"E"}],
               "active":9}"#,
        )
        .unwrap();
        let Loaded::Saved(index) = load_index(d.path()) else {
            panic!("not loaded");
        };
        let ids: Vec<&str> = index.workspaces.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["ok-1_2"], "unsafe and repeated ids go");
        assert_eq!(index.active, 0, "kept in range");
        assert!(index.sidebar, "missing: shown");
        std::fs::write(index_path(d.path()), r#"{"version":1,"workspaces":[]}"#).unwrap();
        assert_eq!(load_index(d.path()), Loaded::Saved(Index::first_run()));
    }

    #[test]
    fn a_new_id_is_the_smallest_unused() {
        assert_eq!(next_id(|id| id == "default"), "w2");
        assert_eq!(next_id(|id| ["default", "w2", "w4"].contains(&id)), "w3");
        let d = tempfile::tempdir().unwrap();
        save(d.path(), "w2", &sample(), 1).unwrap();
        set_aside(d.path(), "w2").unwrap();
        assert!(
            id_in_folder(d.path(), "w2"),
            "a deleted workspace's id isn't reused"
        );
        assert!(!id_in_folder(d.path(), "w3"));
    }

    #[test]
    fn a_deleted_workspace_keeps_its_file_as_bak() {
        let d = tempfile::tempdir().unwrap();
        save(d.path(), "w2", &sample(), 1).unwrap();
        set_aside(d.path(), "w2").unwrap();
        assert!(!path(d.path(), "w2").exists());
        assert!(path(d.path(), "w2").with_extension("json.bak").exists());
        set_aside(d.path(), "never-saved").unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_isnt_utf8_is_left_out() {
        use std::os::unix::ffi::OsStringExt as _;
        let odd = PathBuf::from(OsString::from_vec(vec![b'/', 0xff]));
        assert_eq!(utf8(odd), None);
        assert_eq!(utf8(PathBuf::from("/ok")), Some("/ok".to_string()));
    }

    #[test]
    fn a_snapshot_keeps_the_order_and_the_focus() {
        let camera = Camera::at(templum_canvas::Point::new(5.0, 6.0), 1.5);
        let layout = Layout {
            camera,
            // No processes: their folders are unknown.
            blocks: vec![
                LayoutBlock::Terminal(Rect::new(0.0, 0.0, 10.0, 10.0), Processes::default()),
                LayoutBlock::Saved(Item::Kanban(KanbanItem {
                    rect: Rect::new(5.0, 5.0, 10.0, 10.0).into(),
                    board: Some(3),
                })),
                LayoutBlock::Terminal(Rect::new(20.0, 0.0, 30.0, 10.0), Processes::default()),
            ],
            focused: Some(1),
            recent: vec![PathBuf::from("/a"), PathBuf::from("/b")],
            agents: Default::default(),
            ink: Vec::new(),
            grid: None,
        };
        let (saved, _) = layout.snapshot(None);
        assert_eq!(saved.recent, ["/a", "/b"], "recent folders, newest first");
        assert_eq!(saved.version, VERSION);
        assert_eq!(
            saved.camera,
            SavedCamera {
                x: 5.0,
                y: 6.0,
                zoom: 1.5
            }
        );
        let rects: Vec<SavedRect> = saved
            .items
            .iter()
            .map(|item| match item {
                Item::Terminal(t) => t.rect,
                Item::Kanban(k) => k.rect,
                Item::Note(n) => n.rect,
                Item::Editor(e) => e.rect,
                Item::Git(g) => g.rect,
            })
            .collect();
        assert!(matches!(&saved.items[1], Item::Kanban(k) if k.board == Some(3)));
        assert_eq!(
            rects[2],
            SavedRect {
                x: 20.0,
                y: 0.0,
                w: 30.0,
                h: 10.0
            }
        );
        assert_eq!(saved.focused, Some(1));
    }
}
