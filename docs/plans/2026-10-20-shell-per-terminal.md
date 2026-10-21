# A Shell for Each Terminal: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kanban card 60: the toolbar's Terminal button opens a small menu of the shells found (Default first), the
canvas menu's **Add ▸ Terminal** becomes a submenu with the same items, and a terminal opened with a chosen shell
keeps it across restarts, undo and paste.

**Architecture:** Detection stays `templum_terminal::available_shells()` (Settings ▸ Shell's list), completed on
Linux/macOS by known shells on the PATH and `-l` on macOS. The workspace reads the list at start (and, as now, when
Settings or the welcome opens). A chosen `ShellOption` goes through `Workspace::open_shell` into `spawn_block_with`,
which runs it and keeps it on the `Pane`; the saved item keeps only its **label**, looked up among the shells found on
restore (an unknown label starts the default). One menu function (`shell_items` in `menu/build.rs`) builds the items
for the toolbar (`Target::Shells`) and for **Add ▸ Terminal ▸**. Templum's words for agents typed in a terminal
(`agent_env`, `typed`) are judged by that terminal's own shell.

**Tech Stack:** Rust 2024; gpui-kit `=0.7.3` (`PopupMenu::item`, `separator`, `submenu_with_icon`, `scrollable`,
`max_h`; submenus nest through `parent_menu`, gpui-component-0.7.1 `src/menu/popup_menu.rs:705`); serde; the headless
tests of `crates/templum/src/workspace/tests/` (`window.click`, `window.within("popup-menu")`,
`window.within("submenu")`).

**Spec:** [docs/specs/shells.md, A shell for each terminal](../specs/shells.md#a-shell-for-each-terminal). Read it
first: it holds every decision this plan follows.

## Global Constraints

- The canvas file stores a shell's **label** only (`"shell": "fish"`), `#[serde(default, skip_serializing_if =
  "Option::is_none")]`, never a program or arguments; `session::VERSION` doesn't change.
- Known shells looked up on the PATH, in this order, after `/etc/shells`: `bash`, `zsh`, `fish`, `nu`, `pwsh`; labels
  are their file names. On macOS (`cfg!(target_os = "macos")`) every Unix shell found gets the arguments `["-l"]`.
- Menu copy: `Default (<name>)` first (icon `IconName::SquareTerminal`), a separator only when other shells follow,
  then each other shell by its label (no icon); the submenu is titled `Terminal` (icon `IconName::SquareTerminal`);
  the list scrolls with `.scrollable(true).max_h(px(420.0))`, as `agent_items` does.
- Ctrl+Shift+T (`Command::NewTerminal`), `new_terminal`, `add_terminal_at`, `open`, agents, `log_in`, "Open a
  terminal here", the bus's and the control server's terminals all keep the default shell: their signatures don't
  change.
- Only `crates/templum` uses gpui-kit; no new dependency; no `unsafe`; tests never depend on which shells the machine
  has (made-up `Workspace::shells`, `Workspace::default_shell` and `quiet()`), except Task 2's startup test, which
  uses the machine's first shell found (CI lists at least `sh` on Linux and macOS, Git Bash or cmd on Windows).
- While iterating, run only the affected tests: `cargo test -p <crate> <filter>`. Run the full
  `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` once, in Task
  5. Two tests fail on a clean tree in this environment and are not this plan's:
  `agents::a_saved_agent_is_resumed_in_its_block` and `persistence::two_blocks_one_folder_two_accounts_resume_their_own`.
- Never wait on a process with `pgrep -f "<its command>"` in a loop: it matches the waiting shell itself and never
  ends.
- Commit each task on `development` (never `main`, never push), titled as CLAUDE.md says.

## Review Focus

1. **A canvas file from another computer or OS** (`"shell": "PowerShell 7"` on Linux, an edited label): the block
   comes back running the default, its shell forgotten, nothing else lost. Task 2's `add_saved` test pins it.
2. **The startup restore runs before the list is read**: `start_space` restores after `Workspace::with`, so the list
   must be filled in the struct literal, not later. Task 2's startup test pins it.
3. **A program typed by hand in Settings** (`/opt/fish/bin/fish`, or a found shell's program with other arguments):
   Default names its file name and the found shell of that name isn't listed twice. Task 4's `default_shell_name`
   test pins the name.
4. **Grid mode**: Add ▸ Terminal ▸ in a tile's menu still opens a block that takes half of the tile. Task 4 updates
   `the_grids_menus_add_blocks` to go through the new submenu.
5. **Esc on the toolbar's menu**: it closes, no terminal opens. Task 4's toolbar test pins it.

---

### Task 1: Shells on the PATH and login shells on macOS

**Files:**
- Modify: `crates/templum-terminal/src/shells.rs` (`available_shells`, `unix_shells`, its tests)

**Interfaces:**
- Produces: `available_shells()` (signature unchanged) lists the known shells found on the PATH and, on macOS, gives
  every Unix shell `["-l"]`. Private: `fn unix_shells(listed: &str, exists: impl Fn(&str) -> bool, which: impl
  Fn(&str) -> Option<String>, login: bool) -> Vec<ShellOption>`.

- [ ] **Step 1: Write the failing test** in `shells.rs`'s `tests`, and pass `|_| None, false` to the existing
  `unix_shells_come_from_etc_shells_once_each`:

```rust
#[test]
fn unix_shells_add_known_shells_on_the_path_and_log_in_on_macos() {
    let listed = "/bin/sh\n/bin/bash\n/usr/bin/zsh\n";
    let which = |name: &str| match name {
        "zsh" => Some("/opt/homebrew/bin/zsh".to_string()),
        "fish" => Some("/opt/homebrew/bin/fish".to_string()),
        "nu" => Some("/home/u/.cargo/bin/nu".to_string()),
        _ => None,
    };
    let shells = unix_shells(listed, |_| true, which, false);
    assert_eq!(labels(&shells), ["sh", "bash", "zsh", "fish", "nu"]);
    assert_eq!(shells[2].program, "/usr/bin/zsh", "the one /etc/shells lists");
    assert_eq!(shells[3].program, "/opt/homebrew/bin/fish");
    assert!(shells.iter().all(|s| s.args.is_empty()));
    let login = unix_shells(listed, |_| true, which, true);
    assert!(login.iter().all(|s| s.args == ["-l"]), "{login:?}");
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum-terminal shells`
Expected: compile error (`unix_shells` takes 2 arguments).

- [ ] **Step 3: Implement.** In `unix_shells`, after the `/etc/shells` loop, push `ShellOption::new(name, path,
  &[])` for each of `["bash", "zsh", "fish", "nu", "pwsh"]` whose label isn't listed yet and `which(name)` finds;
  then, with `login`, set every shell's `args` to `["-l"]`. In `available_shells`'s Unix arm pass `|name|
  crate::agent::locate(name).map(|p| p.display().to_string())` and `cfg!(target_os = "macos")`. Update the doc
  comments (`/etc/shells`, then known shells on the PATH; `-l` on macOS, why: the default is a login shell there).

- [ ] **Step 4: Run it to see it pass**

Run: `cargo test -p templum-terminal shells`
Expected: PASS (all of `shells::tests`).

- [ ] **Step 5: Commit**

```bash
git add crates/templum-terminal/src/shells.rs
git commit -m "feat(terminal): list fish, zsh, Nushell and pwsh found on the PATH, and start shells as login shells on macOS"
```

---

### Task 2: A terminal keeps the shell it was opened with

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs` (`Pane::shell`, `Pane::item`; `Workspace::with` fills `shells` and
  `default_shell`)
- Modify: `crates/templum/src/workspace/blocks.rs` (`open_shell`, `new_terminal`, `add_terminal_at`, `spawn_block`,
  `spawn_block_with`, `start_agent`, `log_in`)
- Modify: `crates/templum/src/workspace/persist.rs` (`restore`, `add_saved`, new `found_shell`)
- Modify: `crates/templum/src/session.rs` (`TerminalItem::shell`; the test literals of `TerminalItem` get `shell:
  None`)
- Test: `crates/templum/src/workspace/tests/terminals.rs`, `tests/persistence.rs`, `tests/history.rs`

**Interfaces:**
- Produces:
  - `Pane::shell: Option<templum_terminal::ShellOption>`: the shell chosen for it in a menu; None: the default.
  - `session::TerminalItem::shell: Option<String>`: that shell's label.
  - `Workspace::open_shell(&mut self, shell: Option<ShellOption>, at: Option<Point>, window: &mut Window, cx: &mut
    Context<Self>)` (`pub(crate)`): a terminal running `shell` (None: the default) in the start folder
    (`start_dir`), its top-left corner at `at` with the default size (`DEFAULT_COLS`×`DEFAULT_ROWS`), else at
    `new_terminal_rect`. `new_terminal` is `open_shell(None, None, ..)`, `add_terminal_at(at)` is
    `open_shell(None, Some(at), ..)`.
  - `Workspace::spawn_block_with(&mut self, rect: Rect, folder: Option<PathBuf>, vars: Vec<(String, String)>, shell:
    Option<ShellOption>, window, cx) -> Option<BlockId>`: runs `shell`'s program and arguments, else `self.command`
    (tests), else `[shell]` in config.toml, else the default; the `Pane` keeps `shell`. Every existing caller passes
    `None`, except the two restore paths.
  - `Workspace::found_shell(&self, label: &str) -> Option<ShellOption>` (persist.rs, private): the shell of that label
    in `self.shells`; None, with `eprintln!("templum: no shell {label:?} here: the default one opens")`.
  - `Workspace::with` sets `shells: templum_terminal::available_shells()` and `default_shell:
    templum_terminal::default_shell_label()` in its struct literal (they are filled before `start_space` restores).

- [ ] **Step 1: Write the failing tests**

In `tests/terminals.rs` (`ShellOption` is `templum_terminal::ShellOption`):

```rust
/// A terminal opened with a shell runs it, and its saved item names it by its label
/// (docs/specs/shells.md, The block keeps its shell).
#[gpui_kit::test]
fn a_terminal_opened_with_a_shell_runs_it_and_saves_its_label(cx: &mut TestAppContext) {
    let (handle, ws) = open_as(cx, Some(agent_cat()), None, None);
    let (program, args) = quiet();
    let quiet_shell = ShellOption { label: "Quiet".into(), program, args };
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open_shell(Some(quiet_shell.clone()), None, window, cx);
                ws.space.canvas.top().unwrap()
            })
        })
        .unwrap();
    ws.read_with(cx, |ws, _| {
        let pane = ws.space.terminal(id).unwrap();
        assert_eq!(pane.shell, Some(quiet_shell.clone()));
        let item = pane.item(templum_canvas::Rect::new(0.0, 0.0, 700.0, 450.0), String::new());
        assert_eq!(item.shell.as_deref(), Some("Quiet"));
        // `agent_cat` is `sh` on Unix: the chosen shell ran, not the workspace's.
        #[cfg(unix)]
        assert_eq!(pane.term.title(), "cat");
    });
}
```

In `tests/persistence.rs`, cross-platform (call `ws.add_saved(&item, window, cx)` inline: the `restore` helper there
is Unix-only):

```rust
/// A saved terminal comes back with the shell of its label among the shells found; one whose label
/// isn't found comes back with the default, its shell forgotten.
#[gpui_kit::test]
fn a_saved_shell_comes_back_and_one_not_found_opens_the_default(cx: &mut TestAppContext) {
    // ws.shells = [ShellOption { label: "Quiet", program/args: quiet() }]
    // add_saved({"kind":"terminal","rect":{x:0,y:900,w:700,h:450},"shell":"Quiet"})
    //   -> Some(id); pane.shell == Some(that ShellOption)
    // add_saved({... "x":800 ..., "shell":"PowerShell 7"})
    //   -> Some(id) (the block exists); pane.shell == None; its item's shell == None
}

/// At start, a canvas file's terminal runs its shell: the startup restore reads the list first.
#[gpui_kit::test]
fn a_canvas_files_terminal_starts_its_shell(cx: &mut TestAppContext) {
    // let first = templum_terminal::available_shells().into_iter().next().expect("CI lists a shell");
    // write_canvas(data, {"version":1,"camera":{..},"items":[{"kind":"terminal","rect":{..},"shell":first.label}]})
    // open_as(cx, Some(quiet()), None, Some(data)); tick(cx);
    // the one terminal's pane.shell == Some(first)
}
```

In `tests/history.rs`:

```rust
/// Ctrl+Z brings a closed terminal back with its shell.
#[gpui_kit::test]
fn ctrl_z_brings_a_closed_terminal_back_with_its_shell(cx: &mut TestAppContext) {
    // ws.shells = [Quiet]; open_shell(Some(Quiet), None); close(id); press(handle, UNDO, cx);
    // the top terminal's pane.shell == Some(Quiet)
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum shell`
Expected: compile errors (`open_shell`, `Pane::shell`, `TerminalItem::shell` don't exist).

- [ ] **Step 3: Implement** the Interfaces above. `Pane::item` writes `shell: self.shell.as_ref().map(|s|
  s.label.clone())`. In `persist.rs`, `restore` calls `spawn_block_with(rect, folder, Vec::new(), shell, ..)` instead
  of `spawn_block`, and `add_saved` passes `shell` too, both with `let shell = t.shell.as_deref().and_then(|l|
  self.found_shell(l));`. Doc comments say why the label: the file never holds a command (persistence.md, Safety).

- [ ] **Step 4: Run them to see them pass, with the neighbours**

Run: `cargo test -p templum shell && cargo test -p templum persistence && cargo test -p templum history && cargo test
-p templum session`
Expected: PASS, except `persistence::two_blocks_one_folder_two_accounts_resume_their_own` (fails on a clean tree).

- [ ] **Step 5: Commit**

```bash
git add crates/templum/src/workspace crates/templum/src/session.rs
git commit -m "feat(canvas): a terminal opened with a shell keeps it after a restart, an undo or a paste"
```

---

### Task 3: Agent words follow the terminal's own shell

**Files:**
- Modify: `crates/templum/src/workspace/usage.rs` (`shell_is`, `gives`, `agent_env`, `typed`, new `typed_in`)
- Modify: `crates/templum/src/workspace/blocks.rs` (`spawn_block_with`), `crates/templum/src/workspace/persist.rs`
  (`restore_terminal`)
- Test: `crates/templum/src/workspace/tests/usage.rs`, `tests/persistence.rs`

**Interfaces:**
- Consumes: `Pane::shell` (Task 2).
- Produces:
  - `fn shell_is(&self, shell: Option<&str>, name: &str) -> bool`: `shell` (a program) is judged, else the
    configured program, else `default_shell()`'s, as now.
  - `fn gives(&self, shell: Option<&str>) -> Option<Gives>`; `server_vars` and the Gemini check pass `None`.
  - `pub(super) fn agent_env(&self, shell: Option<&str>) -> Vec<(String, String)>`: `spawn_block_with` passes
    `shell.as_ref().map(|s| s.program.as_str())`.
  - `pub(super) fn typed_in(&self, shell: Option<&str>, agent: Option<&AgentEntry>, command: &str) -> String`:
    today's `typed` body with `self.gives(shell)`; `typed(agent, command)` stays, as `typed_in(None, ..)`.
    `restore_terminal` types the resume line with `typed_in` and the restored pane's `shell`'s program.

- [ ] **Step 1: Write the failing tests.** The three `ws.agent_env()` calls in `tests/usage.rs` become
  `ws.agent_env(None)`. Add to `tests/usage.rs`:

```rust
/// The words for agents typed in a terminal follow its own shell, not the default
/// (docs/specs/shells.md, Agents typed in a chosen shell).
#[gpui_kit::test]
fn a_terminals_agent_words_follow_its_own_shell(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    let data = parent.path().join("Templum Data");
    std::fs::create_dir(&data).unwrap();
    let (_, ws) = open_in(cx, None, Some(data));
    ws.update(cx, |ws, _| {
        let word = |ws: &Workspace, shell: Option<&str>, key: &str| {
            ws.agent_env(shell).into_iter().find(|(k, _)| k == key).unwrap().1
        };
        ws.config.shell.program = None;
        assert_ne!(word(ws, None, "TEMPLUM_CLAUDE_ARGS"), "");
        // Forward slashes: a file name on every OS.
        let wsl = "C:/Windows/System32/wsl.exe";
        assert_eq!(word(ws, Some(wsl), "TEMPLUM_CLAUDE_ARGS"), "");
        #[cfg(unix)]
        {
            ws.config.shell.program = Some("powershell".into());
            assert_eq!(word(ws, None, "TEMPLUM_CODEX_ARGS"), "");
            assert_ne!(word(ws, Some("/bin/bash"), "TEMPLUM_CODEX_ARGS"), "");
        }
    });
}
```

Add to `tests/persistence.rs`, Unix only, beside the other restore tests (`saved_terminal`, `restore`,
`super::bus::screen_with`):

```rust
/// A restored agent's resume line is typed for its terminal's shell: none of Templum's words in WSL.
#[cfg(unix)]
#[gpui_kit::test]
fn a_restored_agents_line_follows_its_terminals_shell(cx: &mut TestAppContext) {
    // data, folder and bin tempdirs; symlink templum_terminal::locate("cat") as bin/wsl
    // open_as(cx, Some(quiet()), None, Some(data)); ws.shells = [ShellOption { label: "WSL", program: bin/wsl, args: [] }]
    // item = saved_terminal(folder, Some("claude"), None, None) with its TerminalItem's shell = Some("WSL")
    // id = restore(handle, &ws, cx, &item); shown = screen_with(&ws, cx, id, "claude")
    // assert!(!shown.contains("--settings"), "{shown}")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum agent_words && cargo test -p templum terminals_shell`
Expected: compile error (`agent_env` takes no shell), then, once it compiles, the persistence test fails: the line
has `--settings`.

- [ ] **Step 3: Implement** the Interfaces above; update `typed`'s and `agent_env`'s doc comments: the shell judged is
  the terminal's own.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum usage && cargo test -p templum persistence`
Expected: PASS, except `persistence::two_blocks_one_folder_two_accounts_resume_their_own`.

- [ ] **Step 5: Commit**

```bash
git add crates/templum/src/workspace
git commit -m "fix(connections): an agent typed in a terminal gets the words that suit that terminal's own shell"
```

---

### Task 4: The shells' menus

**Files:**
- Modify: `crates/templum/src/menu/mod.rs` (`Target::Shells`, `Shells`, `Items::Shells`, `Items::Canvas`,
  `Tail::Tile`, `shells_for_menu`, `items`, `tail`)
- Modify: `crates/templum/src/menu/build.rs` (`shell_items`, `adding`, `Items::build`)
- Modify: `crates/templum/src/toolbar.rs` (`tool-terminal`'s click)
- Modify: `crates/templum/src/workspace/blocks.rs` (`default_shell_name`)
- Modify: `docs/minors.md` (one entry under **Settings panel**)
- Test: `crates/templum/src/workspace/tests/menus.rs`, `tests/grid.rs`

**Interfaces:**
- Consumes: `Workspace::open_shell` (Task 2); `Workspace::shells`, `Workspace::default_shell`.
- Produces:
  - `Workspace::default_shell_name(&self) -> String` (`pub(crate)`, blocks.rs): what Default names, the shell
    Ctrl+Shift+T opens: with `config.shell.program` set, the label of the shell in `self.shells` with that program and
    `config.shell.args`, else the program's file stem (`Path::file_stem`, not `program_name`: a path may hold spaces);
    without one, `self.default_shell`.
  - `menu::Target::Shells`: the toolbar's Terminal button (no point: the block goes at `new_terminal_rect`).
  - `struct Shells { default: String, others: Vec<ShellOption> }` (`#[derive(Clone)]`, menu/mod.rs): `others` is
    `self.shells` without the shell labelled `default`. `fn shells_for_menu(&self) -> Shells` on `Workspace` in
    menu/mod.rs. `Items::Shells(Shells)`; `Items::Canvas` and `Tail::Tile` get a `shells: Shells` field beside
    `agents`.
  - `fn shell_items(ws: &Entity<Workspace>, menu: PopupMenu, shells: Shells, at: Option<Point>) -> PopupMenu`
    (build.rs): the items of the Global Constraints; each calls `ws.open_shell(shell, at, window, cx)` (None for
    Default). `adding(ws, menu, agents, shells, at, window, cx)` turns Add's Terminal item into
    `submenu_with_icon(.., "Terminal", ..)` building `shell_items(.., Some(at))`, using the Add closure's own
    `window` and `cx`.

- [ ] **Step 1: Write the failing tests** in `tests/menus.rs`:

```rust
/// A made-up list (docs/specs/shells.md, The menus): a shell named as the default, left out, then
/// one that runs the tests' quiet program. The menus show Default (zsh), —, Quiet.
fn made_up_shells(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> ShellOption {
    let (program, args) = quiet();
    let quiet_shell = ShellOption { label: "Quiet".into(), program, args };
    ws.update(cx, |ws, _| {
        ws.config.shell.program = None;
        ws.default_shell = "zsh".into();
        ws.shells = vec![
            ShellOption { label: "zsh".into(), program: "/nonexistent/zsh".into(), args: vec![] },
            quiet_shell.clone(),
        ];
    });
    quiet_shell
}

#[gpui_kit::test]
fn the_toolbar_terminal_lists_the_shells_and_opens_the_one_chosen(cx: &mut TestAppContext) {
    // made_up_shells; click "tool-terminal": ws.menu is some; press "escape": menu none, terminal count unchanged.
    // click "tool-terminal", then within("popup-menu").click(2usize): one more terminal, the top one's
    //   pane.shell == Some(quiet_shell), ws.menu none.
    // click "tool-terminal", then click(0usize) (Default): one more terminal, pane.shell == None.
}

#[gpui_kit::test]
fn add_terminal_lists_the_shells_and_opens_the_one_chosen_where_clicked(cx: &mut TestAppContext) {
    // made_up_shells; right_click the empty canvas at (20, 600) as the_canvas_menu_adds_blocks_where_it_was_opened;
    // popup-menu click 1 (Add ▸), its submenu's click 0 (Terminal ▸), the nested submenu's click 2 (Quiet):
    // the top block's rect origin is the right-click's world point; its pane.shell == Some(quiet_shell).
}

#[gpui_kit::test]
fn default_names_the_shell_new_terminals_run(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    ws.update(cx, |ws, _| {
        ws.default_shell = "zsh".into();
        ws.shells = vec![ShellOption {
            label: "PowerShell 7".into(),
            program: "C:/pwsh/pwsh.exe".into(),
            args: vec!["-NoLogo".into()],
        }];
        ws.config.shell.program = None;
        assert_eq!(ws.default_shell_name(), "zsh");
        ws.config.shell.program = Some("C:/pwsh/pwsh.exe".into());
        ws.config.shell.args = vec!["-NoLogo".into()];
        assert_eq!(ws.default_shell_name(), "PowerShell 7");
        ws.config.shell.args.clear();
        assert_eq!(ws.default_shell_name(), "pwsh", "other arguments: its file name");
        ws.config.shell.program = Some("/opt/fish/bin/fish".into());
        assert_eq!(ws.default_shell_name(), "fish");
    });
}
```

Find how a nested submenu is reached (`within("submenu")` twice, or the second `"submenu"` element) by reading
gpui-kit-0.7.3 `src/test.rs` (`within`) and gpui-component-0.7.1 `src/menu/popup_menu.rs:1393`.

Update the tests the new menus change: in `tests/menus.rs`, `title_bar_buttons_reset_zoom_and_open_terminals` and
`the_toolbar_adds_blocks_and_the_title_bar_lost_its_add_buttons` click `tool-terminal` then
`within("popup-menu").click(0usize)` (Default); `the_canvas_menu_adds_blocks_where_it_was_opened`'s Terminal
(Add's item 0) clicks Default (item 0) in the nested submenu. In `tests/grid.rs`, `the_grids_menus_add_blocks`'s
`pick(handle, cx, kanban, 3, Some(0))` is followed by a click on Default in the nested submenu (extend `pick` or
click inline); its comment lists `Add ▸ Terminal ▸ Default`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum menus`
Expected: compile error (`default_shell_name`), then the new tests fail: the toolbar opens a terminal, not a menu.

- [ ] **Step 3: Implement** the Interfaces above. In `toolbar.rs`, `tool-terminal`'s click becomes
  `ws.open_menu(Target::Shells, e.position(), window, cx)` (its tooltip, naming Ctrl+Shift+T, stays). Add to
  `docs/minors.md` under **## Settings panel**:

```markdown
- [ ] **A shell installed while Templum runs isn't in the shells' menus yet.** `Workspace::shells` is read at start
  and when Settings or the welcome opens (`load_lists`, `crates/templum/src/workspace/preferences.rs`); until then the
  toolbar's Terminal and Add ▸ Terminal ▸ show the older list. Fix: read it again when one of those menus opens, with
  a seam that keeps the tests' made-up list.
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p templum menus && cargo test -p templum grid && cargo test -p templum checklists`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum/src docs/minors.md
git commit -m "feat(canvas): pick the shell of a new terminal from the toolbar's Terminal or Add ▸ Terminal ▸"
```

---

### Task 5: Docs, full check, bench, hands-on check

**Files:**
- Modify: `docs/specs/shells.md` (Status: `Implemented`, the "planned" words gone; **Linux and macOS** under
  *Finding shells* points to *Finding shells, completed*)
- Modify: `docs/specs/context-menus.md` (the empty canvas's row: **Add ▸** Terminal ▸ (Default, then the shells
  found, [shells.md](shells.md#the-menus)); a row after the toolbar's AI agent button: "The toolbar's Terminal button
  | Default (*its name*), then the shells found; the chosen one opens where a new terminal goes")
- Modify: `docs/specs/persistence.md` (after **Accounts**: a **Shells** bullet, the label in `"shell"` started again
  when it is among the shells found, else the default; **Safety**: a `"shell"` is a label looked up among the shells
  found, never a command)
- Modify: `docs/ARCHITECTURE.md` (Persistence paragraph, after the account: "and the shell chosen for it
  (`Pane::shell`, saved as its label in `shell`, [specs/shells.md](specs/shells.md#a-shell-for-each-terminal))")

- [ ] **Step 1: Write the docs above**, in the specs' plain style.

- [ ] **Step 2: Full check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: fmt and clippy clean; every test passes except the two that fail on a clean tree
(`agents::a_saved_agent_is_resumed_in_its_block`, `persistence::two_blocks_one_folder_two_accounts_resume_their_own`).
Report the result as it is, failures included.

- [ ] **Step 3: Bench** (the change touches `templum-terminal`; CLAUDE.md wants its line in the PR)

Run: `cargo run --release -p templum -- --bench`
Expected: one JSON line with `p95_ms` <= 16.7. Keep the line for the pull request.

- [ ] **Step 4: Hands-on check (UI)**

Run: `cargo run -p templum -- --dev`. Click the toolbar's Terminal: the menu opens upward, Default (*name*) first,
then the shells found; pick one: its block's title is that shell's name. Right-click the canvas: Add ▸ Terminal ▸
lists the same; the block opens at the click. Quit and start again: the block runs the same shell. Ctrl+Shift+T
opens the default without a menu. Say in the report what was checked this way; Windows and macOS weren't.

- [ ] **Step 5: Commit**

```bash
git add docs
git commit -m "docs: a shell for each terminal in the menus, the canvas file and the architecture"
```
