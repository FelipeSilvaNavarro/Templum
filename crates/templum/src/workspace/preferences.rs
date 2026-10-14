//! config.toml, the themes and the settings panel: reading, applying and saving them.

use std::rc::Rc;
use std::time::Instant;

use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, KeyDownEvent, Keystroke, Window};
use templum_canvas::chrome;
use templum_git::Repo;

use crate::agents;
use crate::config::{self, Config, Edit};
use crate::grid::CellMetrics;
use crate::keys::{self, Command, Keymap, Keys};
use crate::settings::{self, Panel};
use crate::theme::{self, Theme};
use crate::theme_picker::Picker;

use super::{Workspace, missing_themes, show_notification};

impl Workspace {
    /// Saves an agent from the settings: `id` names one already known (one of Templum's, or of
    /// the user's), None a new one of the user's. One of Templum's back at its defaults leaves
    /// config.toml. Returns its id.
    pub(crate) fn save_agent(
        &mut self,
        id: Option<&str>,
        draft: &agents::Draft,
        cx: &mut Context<Self>,
    ) -> Result<String, &'static str> {
        if let Some(problem) = draft.problem() {
            return Err(problem);
        }
        let builtin = id.and_then(templum_terminal::Agent::from_name);
        let pending = self.pending.as_ref().unwrap_or(&self.config);
        let id = match id {
            Some(id) => id.to_string(),
            None => agents::new_id(&draft.name, |x| pending.agents.contains_key(x)),
        };
        match draft.to_config(builtin, &agents::codex_models()) {
            Some(set) => {
                let value = agents::to_value(&set);
                let key = id.clone();
                self.edit(
                    Edit::set("agents", id.clone(), value),
                    |c| {
                        c.agents.insert(key, set);
                    },
                    cx,
                );
            }
            None => self.remove_agent(&id, cx),
        }
        Ok(id)
    }

    /// One of Templum's agents back to its defaults, or one of the user's deleted.
    pub(crate) fn remove_agent(&mut self, id: &str, cx: &mut Context<Self>) {
        let key = id.to_string();
        self.edit(
            Edit::remove("agents", id.to_string()),
            |c| {
                c.agents.remove(&key);
            },
            cx,
        );
    }

    /// Draws with `theme` from the next frame on, in every terminal and gpui-kit widget.
    pub(crate) fn apply(&mut self, theme: Rc<Theme>, cx: &mut Context<Self>) {
        for pane in self.spaces().flat_map(|s| s.terminals().map(|(_, p)| p)) {
            pane.term.set_palette(theme.palette);
        }
        theme::apply_to_kit(&theme, cx);
        self.theme = theme;
        cx.notify();
    }

    /// The system switched between light and dark.
    pub(crate) fn set_dark(&mut self, system_dark: bool, cx: &mut Context<Self>) {
        // GPUI also reports appearance on Wayland decoration and window-control changes.
        if system_dark == self.system_dark {
            return;
        }
        self.system_dark = system_dark;
        // A preview from the other mode must not survive (Esc would restore the wrong theme).
        self.picker = None;
        self.refresh_theme(cx);
    }

    /// Picks the theme for the mode in effect (the system's, unless config.toml pins one).
    pub(super) fn refresh_theme(&mut self, cx: &mut Context<Self>) {
        self.dark = self.config.theme.mode.dark(self.system_dark);
        let theme = theme::pick(
            &self.themes,
            self.config.theme.name(self.dark),
            theme::default_name(self.dark),
        );
        self.apply(theme, cx);
    }

    /// Every config::POLL: reloads config.toml when it changed (or appeared, or went away).
    pub(super) fn poll_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let stamp = config::Stamp::of(self.config_dir.as_deref());
        if stamp != self.config_stamp {
            // The panel's unsaved changes go in first: saving re-reads the file, so the reload
            // brings back both them and what changed by hand. If the file still doesn't parse,
            // the reload says so.
            if !self.unsaved.is_empty() {
                self.save();
            }
            self.config_stamp = config::Stamp::of(self.config_dir.as_deref());
            self.reload_config(window, cx);
        }
        self.poll_theme(window, cx);
    }

    /// Reads the theme in use again when its file changed (saved from an editor block, or any
    /// editor), and applies it. A file that doesn't read keeps the last good colors and says so
    /// once: the stamp is kept, so only the next save is read.
    fn poll_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self
            .themes
            .iter()
            .find(|e| e.theme.name == self.theme.name)
            .and_then(|e| e.path.clone())
        else {
            self.theme_file = None;
            return;
        };
        let stamp = config::Stamp::file(&path);
        if self
            .theme_file
            .as_ref()
            .is_some_and(|(p, s)| *p == path && *s == stamp)
        {
            return;
        }
        self.theme_file = Some((path.clone(), stamp));
        match theme::read(&path) {
            Ok(read) => {
                let read = Rc::new(read);
                if let Some(entry) = self
                    .themes
                    .iter_mut()
                    .find(|e| e.path.as_ref() == Some(&path))
                {
                    entry.theme = read.clone();
                }
                if read != self.theme {
                    self.apply(read, cx);
                }
            }
            Err(err) => {
                let message = format!(
                    "Theme \"{}\": {err}. Keeping the last good one",
                    self.theme.name
                );
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::warning(message));
            }
        }
    }

    /// Reads config.toml again and applies what changed. An invalid file keeps the current
    /// config and says why.
    pub(crate) fn reload_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match config::load(self.config_dir.as_deref()) {
            Ok(new) => {
                if std::mem::take(&mut self.config_failed) {
                    show_notification(
                        window,
                        cx,
                        Notification::info(format!("{} loaded", config::FILE)),
                    );
                }
                // A file that loads is a file a save can write again.
                self.save_error = None;
                // The panel's half-typed shell fields would hide the file's new values.
                if new.shell != self.config.shell {
                    self.drafts.clear();
                }
                self.apply_config(new, window, cx);
            }
            Err(message) => {
                eprintln!("templum: {message}");
                self.config_failed = true;
                show_notification(window, cx, Notification::error(message));
            }
        }
    }

    pub(super) fn apply_config(
        &mut self,
        new: Config,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old = std::mem::replace(&mut self.config, new);
        if self.config.font != old.font {
            self.set_font(window, cx);
        }
        if self.config.theme != old.theme {
            self.picker = None;
            // A theme file may have been added since startup.
            self.themes = theme::load(self.config_dir.as_deref());
            for warning in missing_themes(&self.themes, &self.config.theme) {
                eprintln!("templum: {warning}");
                show_notification(window, cx, Notification::warning(warning));
            }
            self.refresh_theme(cx);
        }
        if self.config.cursor != old.cursor || self.config.terminal != old.terminal {
            let options = self.config.term_options();
            for pane in self.spaces().flat_map(|s| s.terminals().map(|(_, p)| p)) {
                pane.term.shared().set_options(options);
            }
        }
        if self.config.agents != old.agents {
            self.agents = agents::effective(&self.config.agents, &agents::codex_models());
        }
        if self.config.agents != old.agents
            || self.config.git.commit_message != old.git.commit_message
        {
            self.commit_agent = agents::commit_agent(
                &self.agents,
                &self.config.git.commit_message,
                templum_terminal::installed,
            );
        }
        if self.config.keys != old.keys {
            let (keymap, warnings) = Keymap::new(&self.config.keys, cfg!(target_os = "macos"))
                .expect("config::parse validated the shortcuts");
            self.keymap = keymap;
            for warning in warnings {
                eprintln!("templum: {warning}");
                show_notification(window, cx, Notification::warning(warning));
            }
        }
        cx.notify();
    }

    /// A change from the settings panel: in effect from the next frame, written SAVE_DELAY after
    /// the last one. A change that changes nothing is dropped.
    pub(crate) fn edit(
        &mut self,
        edit: Edit,
        change: impl FnOnce(&mut Config),
        cx: &mut Context<Self>,
    ) {
        // Back to the default: what was half-typed for the key goes too (drafts are keyed by it).
        if edit.value.is_none() && self.drafts.remove(&*edit.key).is_some() {
            cx.notify();
        }
        let current = self.pending.as_ref().unwrap_or(&self.config);
        let mut config = current.clone();
        change(&mut config);
        if config == *current {
            return;
        }
        self.pending = Some(config);
        self.unsaved
            .retain(|e| e.table != edit.table || e.key != edit.key);
        self.unsaved.push(edit);
        cx.notify();
    }

    /// Writes the unsaved settings changes to config.toml. If that fails they stay in effect and
    /// unsaved, for the next try, and the message is returned and kept for the panel's footer.
    pub(super) fn save(&mut self) -> Option<String> {
        let edits = std::mem::take(&mut self.unsaved);
        // No config folder: changes last until Templum quits (the panel's footer says so).
        let dir = self.config_dir.clone()?;
        if edits.is_empty() {
            return None;
        }
        // The write merges in a hand edit the poll hasn't seen yet: then the poll must still reload.
        let hand_edited = config::Stamp::of(Some(&dir)) != self.config_stamp;
        match config::save_edits(&dir, &edits) {
            Ok(()) => {
                // Templum's own write: the poll must not reload it.
                if !hand_edited {
                    self.config_stamp = config::Stamp::of(Some(&dir));
                }
                self.save_error = None;
                None
            }
            Err(err) => {
                let message = format!("Not saved: {err}");
                eprintln!("templum: {message}");
                self.unsaved = edits;
                self.save_error = Some(message.clone());
                Some(message)
            }
        }
    }

    /// Applies the settings changes now rather than on the next frame, and writes them: what
    /// opens next (the welcome's first terminal) uses them.
    pub(crate) fn apply_and_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(config) = self.pending.take() {
            self.apply_config(config, window, cx);
        }
        self.save_or_notify(window, cx);
    }

    /// `save`, with a notification when it fails.
    pub(super) fn save_or_notify(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(message) = self.save() {
            show_notification(window, cx, Notification::error(message));
        }
        cx.notify();
    }

    /// New cell metrics: every block keeps its rect, and its terminal gets the columns and rows
    /// that now fit.
    pub(super) fn set_font(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let font = &self.config.font;
        let (cell, warning) = CellMetrics::measure(window, font.family.as_deref(), font.size);
        if let Some(warning) = warning {
            eprintln!("templum: {warning}");
            show_notification(window, cx, Notification::warning(warning));
        }
        self.cell = cell;
        for space in std::iter::once(&mut self.space).chain(&mut self.others) {
            for (id, body) in space.blocks.iter_mut() {
                if let (Some(pane), Some(block)) = (body.terminal_mut(), space.canvas.get(*id)) {
                    let content = chrome::content_rect(block.rect).size();
                    let (cols, rows) = chrome::grid_size(content, self.cell.size);
                    pane.term.resize(cols, rows, self.cell.px());
                }
            }
        }
    }

    /// Opens the theme picker (re-reading the user's themes folder), or closes it like Esc.
    pub(crate) fn toggle_theme_picker(&mut self, cx: &mut Context<Self>) {
        if self.picker.is_some() {
            self.cancel_theme(cx);
            return;
        }
        self.themes = theme::load(self.config_dir.as_deref());
        let selected = self
            .themes
            .iter()
            .position(|e| e.theme.name == self.theme.name);
        self.picker = Some(Picker {
            before: self.theme.clone(),
            selected,
            error: None,
        });
        cx.notify();
    }

    pub(crate) fn preview_theme(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(theme) = self.themes.get(i).map(|e| e.theme.clone()) else {
            return;
        };
        if let Some(picker) = &mut self.picker {
            picker.selected = Some(i);
        }
        self.apply(theme, cx);
    }

    /// ↑/↓ in the picker, wrapping around.
    pub(super) fn move_theme(&mut self, delta: i32, cx: &mut Context<Self>) {
        let n = self.themes.len() as i32;
        if n == 0 {
            return;
        }
        let at = self
            .picker
            .as_ref()
            .and_then(|p| p.selected)
            .map_or(if delta > 0 { -1 } else { 0 }, |i| i as i32);
        self.preview_theme((at + delta).rem_euclid(n) as usize, cx);
    }

    /// Keeps theme `i` for the current mode and saves it. If saving fails, the choice still
    /// applies and the picker stays open to say why.
    pub(crate) fn choose_theme(&mut self, i: usize, cx: &mut Context<Self>) {
        self.preview_theme(i, cx);
        let name = self.theme.name.clone();
        let key = if self.dark { "dark" } else { "light" };
        for config in [Some(&mut self.config), self.pending.as_mut()]
            .into_iter()
            .flatten()
        {
            if self.dark {
                config.theme.dark = name.clone();
            } else {
                config.theme.light = name.clone();
            }
        }
        let saved = match &self.config_dir {
            Some(dir) => config::save_theme(dir, self.dark, &name),
            None => Err(config::SaveError::NoDir),
        };
        match saved {
            Ok(()) => {
                // A panel change of the same theme, not written yet, would write over this choice.
                self.unsaved.retain(|e| e.table != "theme" || e.key != key);
                self.picker = None;
            }
            Err(err) => {
                eprintln!("templum: couldn't save the theme: {err}");
                if let Some(picker) = &mut self.picker {
                    picker.before = self.theme.clone();
                    picker.error = Some(format!("Not saved: {err}"));
                }
            }
        }
        cx.notify();
    }

    /// Esc or a click outside: back to the theme from before the picker opened.
    pub(crate) fn cancel_theme(&mut self, cx: &mut Context<Self>) {
        if let Some(picker) = self.picker.take() {
            self.apply(picker.before, cx);
        }
    }

    /// What the settings' rows choose from: the themes (the folder read again), the installed
    /// monospaced fonts and the shells found. Read when the panel or the welcome opens.
    pub(crate) fn load_lists(&mut self, window: &mut Window) {
        self.themes = theme::load(self.config_dir.as_deref());
        self.fonts = settings::monospace_families(window);
        self.shells = templum_terminal::available_shells();
        self.default_shell = templum_terminal::default_shell_label();
    }

    /// Opens the settings panel (re-reading the themes folder for its lists), or closes it.
    pub(crate) fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.as_ref().is_some_and(|p| !p.closing) {
            self.close_settings(window, cx);
            return;
        }
        self.load_lists(window);
        self.git_version = None;
        let program = self.config.git.program().to_string();
        cx.spawn_in(window, async move |ws, cx| {
            let version = cx
                .background_executor()
                .spawn(
                    async move { Repo::version(&program).map_err(|e| e.first_line().to_string()) },
                )
                .await;
            ws.update(cx, |ws, cx| {
                ws.git_version = Some(version);
                cx.notify();
            })
            .ok();
        })
        .detach();
        // The panel would open under the screen; it has everything the welcome has.
        self.close_screen(window, cx);
        self.close_welcome(window, cx);
        let now = Instant::now();
        self.settings = Some(match &self.settings {
            // Still sliding out: it turns back from where it is.
            Some(closing) => closing.reversed(now),
            None => Panel::new(now),
        });
        window.focus(&self.settings_focus, cx);
        cx.notify();
    }

    /// Shows another section of the settings panel.
    pub(crate) fn set_section(&mut self, section: settings::Section, cx: &mut Context<Self>) {
        if let Some(panel) = &mut self.settings {
            panel.section = section;
            cx.notify();
        }
    }

    /// Slides the panel out, saves what's pending, and gives the keyboard back to the terminals.
    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = &self.settings
            && !panel.closing
        {
            self.settings = Some(panel.reversed(Instant::now()));
        }
        self.drafts.clear();
        self.recording = None;
        self.key_note = None;
        self.save_or_notify(window, cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// Keys while the panel has the focus (and its fields left them): the theme picker's keys, the
    /// shortcuts that don't act on a terminal, and Esc to close it.
    pub(crate) fn on_settings_key(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(command) = self.recording {
            // While recording, no shortcut runs and nothing reaches a terminal.
            cx.stop_propagation();
            // A held key repeats: only its first press counts.
            if e.is_held {
                return;
            }
            let k = &e.keystroke;
            let bare = !k.modifiers.modified();
            match k.key.as_str() {
                "escape" if bare => {
                    self.recording = None;
                    self.key_note = None;
                }
                "backspace" if bare => self.set_shortcut(command, Keys::Many(Vec::new()), cx),
                _ if !stands_as_shortcut(k) => {
                    self.key_note = Some(
                        if cfg!(target_os = "macos") {
                            "Add Cmd, Ctrl or Option, or use an F key"
                        } else if cfg!(windows) {
                            "Add Ctrl, Alt or Win, or use an F key"
                        } else {
                            "Add Ctrl, Alt or Super, or use an F key"
                        }
                        .to_string(),
                    );
                }
                _ => match self.keymap.command(k) {
                    Some(owner) if owner != command => {
                        self.key_note = Some(format!(
                            "{} is already {}",
                            keys::display(&k.unparse()),
                            owner.title()
                        ));
                    }
                    _ => self.set_shortcut(command, Keys::One(k.unparse()), cx),
                },
            }
            cx.notify();
            return;
        }
        // A shortcut just recorded may still be held: its repeats mustn't run it.
        if e.is_held {
            return;
        }
        // Copy, paste and close act on the focused terminal, which doesn't have the keyboard now.
        let command = self.keymap.command(&e.keystroke).filter(|c| {
            !matches!(
                c,
                Command::Copy
                    | Command::Paste
                    | Command::CloseTerminal
                    | Command::PreviousPrompt
                    | Command::NextPrompt
                    | Command::Find
            )
        });
        if self.picker.is_some() || command.is_some() {
            self.command_key(e, command, window, cx);
            cx.stop_propagation();
        } else if e.keystroke.key == "escape" {
            self.close_settings(window, cx);
            cx.stop_propagation();
        }
    }

    /// The next key press with a main key becomes `command`'s only shortcut.
    pub(crate) fn record(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.recording = Some(command);
        self.key_note = None;
        window.focus(&self.settings_focus, cx);
        cx.notify();
    }

    /// "Reset to default" of a shortcut. "Reset All" asks for several in one go, so they are
    /// decided together once the event is over.
    pub(crate) fn reset_shortcut(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.resetting.push(command);
        if self.resetting.len() == 1 {
            cx.defer_in(window, |ws, window, cx| ws.finish_resets(window, cx));
        }
    }

    /// A reset is refused while a command that keeps its shortcuts has one of the defaults, which
    /// would then be set twice. Commands reset together may trade keys (their defaults differ).
    pub(super) fn finish_resets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let macos = cfg!(target_os = "macos");
        let mut resetting = std::mem::take(&mut self.resetting);
        let keys = &self.pending.as_ref().unwrap_or(&self.config).keys;
        let (keymap, _) = Keymap::new(keys, macos).expect("config::parse validated the shortcuts");
        // Refusing one can make another's default stay taken: repeat until nothing changes.
        while let Some((i, owner, default)) = resetting.iter().enumerate().find_map(|(i, c)| {
            c.defaults(macos).iter().find_map(|default| {
                let typed = Keystroke::parse(default).expect("the default shortcuts parse");
                keymap
                    .command(&typed)
                    .filter(|owner| owner != c && !resetting.contains(owner))
                    .map(|owner| (i, owner, *default))
            })
        }) {
            resetting.remove(i);
            let message = format!(
                "Not reset: {} is {} now",
                keys::display(default),
                owner.title()
            );
            show_notification(window, cx, Notification::warning(message));
        }
        for command in resetting {
            self.edit(
                Edit::remove("keys", command.name()),
                |c| c.keys.set(command, None),
                cx,
            );
        }
    }

    pub(super) fn set_shortcut(&mut self, command: Command, keys: Keys, cx: &mut Context<Self>) {
        self.recording = None;
        self.key_note = None;
        let value: toml_edit::Value = match &keys {
            Keys::One(one) => one.as_str().into(),
            Keys::Many(many) => many
                .iter()
                .map(String::as_str)
                .collect::<toml_edit::Array>()
                .into(),
        };
        self.edit(
            Edit::set("keys", command.name(), value),
            |c| c.keys.set(command, Some(keys)),
            cx,
        );
    }

    /// Opens config.toml in the system's editor, writing the commented template first if needed.
    pub(crate) fn open_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.config_dir.clone() else {
            let message = "No config folder: neither HOME nor APPDATA is set".to_string();
            eprintln!("templum: {message}");
            show_notification(window, cx, Notification::error(message));
            return;
        };
        match config::ensure_file(&dir) {
            Ok(path) => cx.open_with_system(&path),
            Err(err) => {
                let message = format!(
                    "Couldn't create {}: {err}",
                    dir.join(config::FILE).display()
                );
                eprintln!("templum: {message}");
                show_notification(window, cx, Notification::error(message));
            }
        }
    }
}

/// A shortcut needs Ctrl, Alt or Super (Cmd), or is an F key: any other key is typing, which the
/// terminals must keep getting.
fn stands_as_shortcut(k: &Keystroke) -> bool {
    let m = &k.modifiers;
    let f_key = k
        .key
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=35).contains(&n));
    m.control || m.alt || m.platform || m.function || f_key
}
