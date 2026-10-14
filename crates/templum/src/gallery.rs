//! The theme gallery: the themes of alacritty-theme, cached in the data folder, shown as cards in
//! their own colors; hovering one previews it, a click installs it (docs/specs/themes.md).

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, ClickEvent, Context, ElementId, SharedString, TestSupportExt, Window, div, px,
};
use serde::Deserialize;

use crate::config;
use crate::settings::theme_card;
use crate::theme::{self, Theme};
use crate::workspace::Workspace;

/// The repository's themes folder, through GitHub's API: one request lists every file.
const LIST: &str = "https://api.github.com/repos/alacritty/alacritty-theme/contents/themes";
/// Where its files are downloaded from; any other address in the listing is refused.
const RAW: &str = "https://raw.githubusercontent.com/alacritty/alacritty-theme/";
/// The cache's folder in the data folder.
const CACHE: &str = "theme-gallery";
/// The listing as GitHub last gave it; each file is `<sha>.toml` beside it.
const INDEX: &str = "index.json";
/// Downloads at a time.
const WORKERS: usize = 16;

/// One file of GitHub's listing.
#[derive(Deserialize)]
struct Listed {
    name: String,
    sha: String,
    download_url: Option<String>,
    #[serde(rename = "type")]
    kind: String,
}

/// The theme files of GitHub's listing: a `.toml` file with a git hash and an address in the
/// repository (a hash names its cached file, and the address is downloaded).
fn listing(json: &str) -> Result<Vec<Listed>, String> {
    let all: Vec<Listed> = serde_json::from_str(json)
        .map_err(|e| format!("GitHub's answer isn't a list of files: {e}"))?;
    Ok(all
        .into_iter()
        .filter(|l| {
            l.kind == "file"
                && l.name.ends_with(".toml")
                && l.sha.len() == 40
                && l.sha.bytes().all(|b| b.is_ascii_hexdigit())
                && l.download_url
                    .as_deref()
                    .is_some_and(|u| u.starts_with(RAW))
        })
        .collect())
}

/// A file's theme name: `github_dark_high_contrast.toml` is "Github Dark High Contrast".
fn pretty(file: &str) -> String {
    file.strip_suffix(".toml")
        .unwrap_or(file)
        .split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut letters = w.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A theme of the gallery.
#[derive(Clone)]
pub(crate) struct Card {
    /// Its file in the repository.
    file: String,
    /// The file as downloaded.
    text: String,
    pub theme: Rc<Theme>,
}

/// The cached themes that read, sorted by name, one per name.
fn load(cache: &Path) -> Vec<Card> {
    let Some(listed) = std::fs::read_to_string(cache.join(INDEX))
        .ok()
        .and_then(|json| listing(&json).ok())
    else {
        return Vec::new();
    };
    let mut cards: Vec<Card> = listed
        .into_iter()
        .filter_map(|l| {
            let text = std::fs::read_to_string(cache.join(format!("{}.toml", l.sha))).ok()?;
            let theme = theme::parse(&pretty(&l.name), &text).ok()?;
            Some(Card {
                file: l.name,
                text,
                theme: Rc::new(theme),
            })
        })
        .collect();
    cards.sort_by_key(|c| c.theme.name.to_lowercase());
    cards.dedup_by(|a, b| a.theme.name.eq_ignore_ascii_case(&b.theme.name));
    cards
}

/// Asks GitHub for the listing and downloads the files not cached yet, then keeps the listing.
/// Blocking: run it off the UI thread. An error only when nothing could be had.
// ponytail: files GitHub no longer lists stay in the cache (a few hundred bytes each); prune what
// the index doesn't name if the folder ever grows.
fn refresh(cache: &Path) -> Result<(), String> {
    // One at a time: a gallery opened again during the first download waits for it (its files
    // and the listing are written through the same temporary names), then finds them cached.
    static REFRESHING: Mutex<()> = Mutex::new(());
    let _one = REFRESHING.lock().unwrap_or_else(|e| e.into_inner());
    let agent = templum_update::agent();
    let json = templum_update::get(&agent, LIST)?;
    let listed = listing(&json)?;
    std::fs::create_dir_all(cache)
        .map_err(|e| format!("Couldn't create {}: {e}", cache.display()))?;
    let mut missing: Vec<&Listed> = listed
        .iter()
        .filter(|l| !cache.join(format!("{}.toml", l.sha)).is_file())
        .collect();
    // Two names may share a file (the same colors): it is downloaded once.
    missing.sort_by_key(|l| &l.sha);
    missing.dedup_by_key(|l| &l.sha);
    let (next, failed) = (AtomicUsize::new(0), AtomicUsize::new(0));
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(missing.len()) {
            scope.spawn(|| {
                while let Some(l) = missing.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let url = l.download_url.as_deref().unwrap_or_default();
                    let saved = templum_update::get(&agent, url).and_then(|text| {
                        // A file not matching its hash (a stale copy GitHub's CDN still serves)
                        // is not kept under it: the next refresh tries again.
                        if blob_sha(&text) != l.sha.to_lowercase() {
                            return Err("not the file GitHub lists".to_string());
                        }
                        config::write_whole(&cache.join(format!("{}.toml", l.sha)), &text)
                            .map_err(|e| e.to_string())
                    });
                    if let Err(err) = saved {
                        eprintln!("templum: couldn't download the theme {}: {err}", l.name);
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    if !missing.is_empty() && failed.load(Ordering::Relaxed) == missing.len() {
        return Err("Couldn't download the themes".to_string());
    }
    config::write_whole(&cache.join(INDEX), &json)
        .map_err(|e| format!("Couldn't write {}: {e}", cache.join(INDEX).display()))
}

/// The hash git gives a file's content, as GitHub's listing names it.
fn blob_sha(text: &str) -> String {
    let mut sha = sha1_smol::Sha1::new();
    sha.update(format!("blob {}\0", text.len()).as_bytes());
    sha.update(text.as_bytes());
    sha.digest().to_string()
}

/// An installed theme's file: the downloaded one, under a line that names its source and license.
pub(crate) fn credited(card: &Card) -> String {
    // A byte-order mark is only allowed first: under the line it would break the file.
    let text = card.text.strip_prefix('\u{feff}').unwrap_or(&card.text);
    format!(
        "# From https://github.com/alacritty/alacritty-theme (themes/{}), Apache-2.0.\n{text}",
        card.file
    )
}

/// What the gallery shows: the cards, and how the refresh went.
struct Shelf {
    cards: Vec<Card>,
    /// Looking for new themes (None), done (Ok), or why it couldn't.
    refreshed: Option<Result<(), String>>,
}

/// Opens the gallery, which needs the config folder (to install) and the data folder (the
/// cache). It shows what is cached at once, then what a refresh brings.
pub(crate) fn open(ws: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let (Some(data), Some(config_dir)) = (ws.data_dir.clone(), ws.config_dir.clone()) else {
        return;
    };
    let cache = data.join(CACHE);
    // Marks what is installed: the folder as it is now.
    ws.themes = theme::load(Some(&config_dir));
    let shelf = Rc::new(RefCell::new(Shelf {
        cards: load(&cache),
        refreshed: cfg!(test).then_some(Ok(())),
    }));
    // Tests stay off the network: they fill the cache themselves.
    if !cfg!(test) {
        let shelf = shelf.clone();
        cx.spawn_in(window, async move |ws, cx| {
            let fetched = cache.clone();
            let done = cx
                .background_executor()
                .spawn(async move { refresh(&fetched) })
                .await;
            ws.update(cx, |_, cx| {
                let mut shelf = shelf.borrow_mut();
                if done.is_ok() {
                    shelf.cards = load(&cache);
                }
                shelf.refreshed = Some(done);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
    let entity = cx.entity();
    // Cleared when the dialog closes: a hover the last frame still dispatches mustn't preview then.
    let open = Rc::new(Cell::new(true));
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Find a theme"));
    let field = search.clone();
    {
        let entity = entity.clone();
        App::subscribe(cx, &search, move |_, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                entity.update(cx, |_, cx| cx.notify());
            }
        })
        .detach();
    }
    window.open_alert_dialog(cx, move |alert, _, cx| {
        let query = search.read(cx).value().trim().to_lowercase();
        let ws_now = entity.read(cx);
        let (ui, dark) = (ws_now.ui, ws_now.dark);
        let font = ws_now.cell.font.family.clone();
        let chosen = ws_now.config.theme.name(dark);
        let shelf_now = shelf.borrow();
        let cards: Vec<_> = shelf_now
            .cards
            .iter()
            .filter(|c| c.theme.is_dark == dark && c.theme.name.to_lowercase().contains(&query))
            .map(|card| {
                let name = &card.theme.name;
                // One already there is shown, and chosen, as it is.
                let installed = ws_now
                    .themes
                    .iter()
                    .find(|e| e.theme.name.eq_ignore_ascii_case(name))
                    .map(|e| e.theme.clone());
                let shown = installed.clone().unwrap_or_else(|| card.theme.clone());
                let id = ElementId::Name(format!("gallery-theme-{name}").into());
                let (hover, click, card) = (entity.clone(), entity.clone(), card.clone());
                let (hover_open, click_open) = (open.clone(), open.clone());
                theme_card(
                    id,
                    &shown,
                    ui,
                    font.clone(),
                    name.eq_ignore_ascii_case(chosen),
                )
                .when(installed.is_some(), |el| {
                    el.child(div().text_size(px(10.0)).opacity(0.7).child("Installed"))
                })
                .on_hover(move |hovered: &bool, _, cx| {
                    if *hovered && hover_open.get() {
                        hover.update(cx, |ws, cx| ws.apply(shown.clone(), cx));
                    }
                })
                .on_click(move |_: &ClickEvent, window, cx| {
                    // Closing this way skips `on_close`, which would put the old theme back.
                    click_open.set(false);
                    window.close_dialog(cx);
                    click.update(cx, |ws, cx| ws.install_theme(&card, window, cx));
                })
                .test_support()
            })
            .collect();
        let status: SharedString = match (&shelf_now.refreshed, shelf_now.cards.is_empty()) {
            (None, true) => "Downloading the themes…".into(),
            (None, false) => "Looking for new themes…".into(),
            (Some(Err(err)), true) => err.clone().into(),
            (Some(Err(err)), false) => {
                format!("{err}: these are the themes downloaded before").into()
            }
            (Some(Ok(())), _) => {
                "From github.com/alacritty/alacritty-theme. Hover to preview, click to install"
                    .into()
            }
        };
        let none = (cards.is_empty() && !shelf_now.cards.is_empty()).then(|| {
            div()
                .py(px(16.0))
                .text_size(px(13.0))
                .text_color(ui.title_dim)
                .child(if query.is_empty() {
                    "No theme for this mode yet."
                } else {
                    "No theme has that name."
                })
        });
        let (restore, closing) = (entity.clone(), open.clone());
        alert
            .title(if dark { "Dark themes" } else { "Light themes" })
            .w(px(600.0))
            .button_props(DialogButtonProps::default().ok_text("Done"))
            // Esc, Done or a click outside: back to the mode's theme, whatever was previewed.
            .on_close(move |_, _, cx| {
                closing.set(false);
                restore.update(cx, |ws, cx| ws.refresh_theme(cx))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("gallery-find")
                            .child(Input::new(&search))
                            .test_support(),
                    )
                    .child(
                        div()
                            .id("gallery-list")
                            .max_h(px(420.0))
                            .overflow_y_scroll()
                            .child(div().grid().grid_cols(3).gap(px(10.0)).children(cards))
                            .children(none)
                            .test_support(),
                    )
                    .child(
                        div()
                            .id("gallery-status")
                            .text_size(px(12.0))
                            .text_color(ui.title_dim)
                            .child(status)
                            .test_support(),
                    ),
            )
    });
    // After opening: the dialog takes the focus when it opens.
    field.update(cx, |input, cx| input.focus(window, cx));
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// One file of a listing, as GitHub's API gives it.
    pub(crate) fn entry(name: &str, sha: &str) -> String {
        format!(
            r#"{{"name": "{name}", "path": "themes/{name}", "sha": "{sha}", "size": 600, "type": "file", "download_url": "{RAW}master/themes/{name}"}}"#
        )
    }

    /// Fills `<data>/theme-gallery` as a refresh would: a listing of `themes` and their files.
    pub(crate) fn seed(data: &Path, themes: &[(&str, &str)]) {
        let cache = data.join(CACHE);
        std::fs::create_dir_all(&cache).unwrap();
        let mut entries = Vec::new();
        for (i, (name, text)) in themes.iter().enumerate() {
            let sha = format!("{i:040x}");
            std::fs::write(cache.join(format!("{sha}.toml")), text).unwrap();
            entries.push(entry(name, &sha));
        }
        std::fs::write(cache.join(INDEX), format!("[{}]", entries.join(","))).unwrap();
    }

    const DRACULA: &str = "[colors.primary]\nbackground = '#282a36'\nforeground = '#f8f8f2'\n\
        [colors.normal]\nblack = '#21222c'\nred = '#ff5555'\ngreen = '#50fa7b'\nyellow = '#f1fa8c'\n\
        blue = '#bd93f9'\nmagenta = '#ff79c6'\ncyan = '#8be9fd'\nwhite = '#f8f8f2'\n";

    #[test]
    fn listing_keeps_theme_files() {
        let sha = "a".repeat(40);
        let json = format!(
            "[{}, {}, {}, {}, {}, {}]",
            entry("dracula.toml", &sha),
            entry("README.md", &sha),
            entry("short.toml", "abc"),
            entry("escape.toml", "../../../../../../../../../../config.tom"),
            entry("evil.toml", &sha).replace(RAW, "https://example.com/"),
            r#"{"name": "dir.toml", "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "type": "dir", "download_url": null}"#,
        );
        let kept: Vec<String> = listing(&json)
            .unwrap()
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(kept, ["dracula.toml"]);
        assert!(listing("{\"message\": \"Not Found\"}").is_err());
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty("dracula.toml"), "Dracula");
        assert_eq!(
            pretty("github_dark_high_contrast.toml"),
            "Github Dark High Contrast"
        );
        assert_eq!(pretty("Cobalt2.toml"), "Cobalt2");
        assert_eq!(pretty("rose-pine-moon.toml"), "Rose Pine Moon");
        assert_eq!(pretty("one_dark.toml"), "One Dark");
        assert_eq!(pretty("alacritty_0_12.toml"), "Alacritty 0 12");
    }

    #[test]
    fn load_skips_missing_and_broken_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(&dir.path().join(CACHE)).is_empty(), "no cache yet");
        seed(
            dir.path(),
            &[
                ("dracula.toml", DRACULA),
                (
                    "citylights.toml",
                    "[colors.primary]\nbackground = '#000000'\n",
                ),
                ("Dracula.toml", DRACULA),
                ("acme.toml", DRACULA),
            ],
        );
        let cache = dir.path().join(CACHE);
        std::fs::remove_file(cache.join(format!("{:040x}.toml", 3))).unwrap();
        let names: Vec<String> = load(&cache).iter().map(|c| c.theme.name.clone()).collect();
        assert_eq!(
            names,
            ["Dracula"],
            "broken, missing and the same name twice left out"
        );
    }

    #[test]
    fn blob_sha_is_gits() {
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(
            blob_sha("hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn credited_drops_a_byte_order_mark() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), &[("bom.toml", &format!("\u{feff}{DRACULA}"))]);
        let card = load(&dir.path().join(CACHE)).remove(0);
        assert!(theme::parse("x", &credited(&card)).is_ok());
    }

    #[test]
    fn credited_names_the_source() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), &[("dracula.toml", DRACULA)]);
        let card = load(&dir.path().join(CACHE)).remove(0);
        let text = credited(&card);
        assert!(text.starts_with(
            "# From https://github.com/alacritty/alacritty-theme (themes/dracula.toml), Apache-2.0.\n"
        ));
        assert_eq!(
            theme::parse("x", &text).unwrap().palette,
            card.theme.palette
        );
    }
}
