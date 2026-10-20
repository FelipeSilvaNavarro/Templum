# A Shell for Each Agent: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kanban card 66: every entry of Settings ▸ Agents (Templum's agents, the person's, accounts) has a **Shell**,
Default (Settings ▸ Shell) or one of the shells found, and opens in it wherever Templum opens agents; an agent set to
WSL counts as installed, gets none of Templum's words, and `spawn_agent` refuses it.

**Architecture:** The setting is `shell = "<label>"` in the agent's `[agents]` entry (`AgentConfig::shell`), carried
by `AgentEntry::shell`, `Draft::shell` and `AccountDraft::shell`. `Workspace::agent_shell` looks the label up among
card 60's `Workspace::shells` (an account never gets WSL); `start_agent`, which every opening goes through
(`launch_agent`, `open_agent_in` for `spawn_agent` and the Templum Agent), passes it to `spawn_block_with`, which
keeps it on the `Pane` (saved by label, restored by card 60's code), and judges the typed line and the block's
variables by it (`typed_in`, `agent_vars`). `Workspace::in_wsl` / `agent_installed` / `spawn_problem` decide WSL's
special cases in one place for the menus, Settings, the welcome, `spawn_agent` and the Templum Agent. The dialogs get
a `menu_button` built by one pure function, `settings::agents::shell_choices`.

**Tech Stack:** Rust 2024; gpui-kit `=0.7.3` (`menu_button` in `settings/row.rs`, `window.open_alert_dialog`, an
`Entity` for dialog state as the account dialog's `Form`); `toml_edit` (`agents::to_value`); the headless tests of
`crates/templum/src/workspace/tests/` (`window.click`, `window.within("dialog")`, `window.within("popup-menu")`).

**Spec:** [docs/specs/shells.md, A shell for each agent](../specs/shells.md#a-shell-for-each-agent). Read it first:
it holds every decision this plan follows. Card 60's section above it (and its
[plan](2026-10-20-shell-per-terminal.md)) explains `Workspace::shells`, `default_shell_name`, `Pane::shell`,
`found_shell` and `typed_in`, which this plan builds on.

## Global Constraints

- config.toml: `shell` is a string, a shell's **label** (`"PowerShell 7"`, `"WSL"`, `"fish"`), in any `[agents]`
  entry, accounts included; missing or blank (`text()` trims) is Default. Never a program. No other validation: a
  label not found opens the default with `eprintln!("templum: no shell {label:?} for {title}: the default one opens")`,
  no notification, and the label stays in the file.
- An account's shell is its own (`effective` sets it from the account's entry, never from `..agent.clone()`), and
  never WSL: `agent_shell` gives None for a shell `is_shell(program, "wsl")` on an account, and its dialog's list
  leaves WSL out.
- WSL is judged by `templum_terminal::is_shell(program, "wsl")`: the program's file stem, any case.
  `Workspace::in_wsl` looks at the agent's shell, else `config.shell.program`, and never calls
  `templum_terminal::default_shell()` (it scans the disk on Windows, and Settings asks every frame; the system's
  default is never WSL).
- Copy, verbatim from the spec:
  - list items: `Default (<name>)` (`<name>`: `Workspace::default_shell_name()`), each shell by its label,
    `<label> (not available)`;
  - row suffix: ` · in <label>`, plus ` (not available)` when not found;
  - dialog note while the choice is WSL: `In WSL the agent gets none of Templum's tools, hooks or status line, and
    isn't resumed after a restart.`;
  - `spawn_agent` refusal: `<title> runs in WSL, where Templum can't see it start nor type its prompt: open it from
    the canvas.`
- Element ids: `menu-agent-shell` (agent dialog's list), `agent-shell-wsl` (its note), `menu-account-shell` (account
  dialog's list). List values: `""` for Default, the label for a shell.
- Ctrl+Shift+T, the toolbar's Terminal, **Add ▸ Terminal ▸**, Generate ✨ (commit messages), `list_agents` and
  restoring a block (its own saved shell) don't change. No new argument for `spawn_agent` or `create_block`.
- Only `crates/templum` uses gpui-kit; no new dependency; no `unsafe`. Tests never depend on the machine's shells:
  made-up `Workspace::shells` whose programs are `quiet()` (or, Unix only, a link named `wsl` to `cat`), and
  `C:/Windows/System32/wsl.exe` (forward slashes: a file name on every OS) for a WSL that never runs.
- While iterating, run only the affected tests: `cargo test -p <crate> <filter>`. Run the full
  `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` once, in Task
  6. Two tests fail on a clean tree on this machine only (they read the real `~/.claude`) and are not this plan's:
  `agents::a_saved_agent_is_resumed_in_its_block` and `persistence::two_blocks_one_folder_two_accounts_resume_their_own`.
- Never wait on a process with `pgrep -f "<its command>"` in a loop: it matches the waiting shell itself and never
  ends. Never inject keystrokes or mouse events into windows: the person's own Templum is open; UI is checked by the
  headless tests.
- Windows clippy (optional, Task 6): `cargo clippy -p templum -p templum-usage -p templum-terminal --all-targets
  --target x86_64-pc-windows-msvc -- -D warnings` with `CC_x86_64_pc_windows_msvc=<a script producing an empty COFF
  object>` and `AR_x86_64_pc_windows_msvc=llvm-lib`.
- Another agent works in `.worktrees/kanban-small` on other cards: don't touch it.
- Commit each task on `development` (never `main`, never push), titled as CLAUDE.md says.

## Review Focus

1. **Saving an entry drops its shell.** The row's switch saves `..Draft::of(&agent)` / `..AccountDraft::of(&entry)`,
   and `save_agent` replaces the whole `agents.<id>` value: a shell written by hand must survive the switch, the
   dialog's Save and an account's Save. Task 1's round-trip test and Task 4's switch test pin it.
2. **A config.toml from another computer** (`shell = "PowerShell 7"` on Linux): the agent opens in the default, the
   label stays in the file, the dialog shows `PowerShell 7 (not available)` and Save keeps it. Task 2 (opens the
   default) and Task 4 (`shell_choices`) pin it.
3. **The agent's setting changed after its block was saved**: a restored, undone or pasted agent block runs its own
   saved shell, not the setting as it is now. Task 2's test restores one after the setting is cleared.
4. **Settings ▸ Shell set to WSL with agents at Default**: every agent counts as installed and `spawn_agent` refuses
   them, as one set to WSL. Task 3's tests set `config.shell.program` to WSL.
5. **An account written by hand with `shell = "WSL"`**: it opens in the default, not in WSL where its folder would be
   lost, and its dialog lists `WSL (not available)`. Tasks 2 and 4 pin it.

---

### Task 1: An agent entry's shell in config.toml

**Files:**
- Modify: `crates/templum/src/config.rs` (`AgentConfig::shell`)
- Modify: `crates/templum/src/agents.rs` (`AgentEntry::shell`, `effective`, `Draft`, `AccountDraft`, `to_value`, the
  test literals)
- Modify: `crates/templum/src/settings/agents.rs` (the dialog's `save` keeps the entry's shell), and every full
  literal of `Draft`, `AccountDraft` or `AgentEntry` the compiler names (`settings/accounts.rs`,
  `workspace/tests/agents.rs`, `tests/accounts.rs`, `tests/menus.rs`, `tests/settings_panel.rs`): `shell: None`
- Modify: `docs/specs/agents.md` (the `[agents]` example gets `shell`), `docs/specs/accounts.md` (An account in
  config.toml: an entry with `of` takes `of`, `name`, `show`, `folder`, `share` and `shell`)
- Test: `crates/templum/src/agents.rs` (`tests`)

**Interfaces:**
- Produces:
  - `config::AgentConfig::shell: Option<String>`: the label of the shell it opens in; missing: the default.
  - `agents::AgentEntry::shell: Option<String>`: that label, trimmed, None when missing or blank.
  - `agents::Draft::shell: Option<String>` and `agents::AccountDraft::shell: Option<String>`: `of` copies the entry's;
    `to_config` writes it (for one of Templum's it counts as a change, so Default with nothing else still gives
    `None`).
  - `agents::to_value` writes `shell` with the other strings.

- [ ] **Step 1: Write the failing test** in `agents.rs`'s `tests`:

```rust
#[test]
fn a_shell_is_each_entrys_own_and_drafts_keep_it() {
    let list = agents(
        "[agents.claude]\nshell = \"fish\"\n\
         [agents.mine]\nname = \"Mine\"\ncommand = \"mine\"\nshell = \"WSL\"\n\
         [agents.claude2]\nof = \"claude\"\n\
         [agents.claude3]\nof = \"claude\"\nshell = \"zsh\"\n",
    );
    let claude = by(&list, "claude");
    assert_eq!(claude.shell.as_deref(), Some("fish"));
    assert!(claude.changed);
    assert_eq!(by(&list, "mine").shell.as_deref(), Some("WSL"));
    assert_eq!(by(&list, "codex").shell, None);
    assert_eq!(by(&list, "claude2").shell, None, "an account's own, not its agent's");
    assert_eq!(by(&list, "claude3").shell.as_deref(), Some("zsh"));
    // The switch and the dialog save the whole entry: the shell goes with it.
    let set = Draft::of(claude).to_config(claude.builtin, &[]).unwrap();
    assert_eq!(set.shell.as_deref(), Some("fish"));
    assert!(to_value(&set).to_string().contains("shell = \"fish\""));
    let default = Draft { shell: None, ..Draft::of(claude) };
    assert_eq!(default.to_config(claude.builtin, &[]), None, "Default is Templum's: no entry");
    let mine = by(&list, "mine");
    let set = Draft::of(mine).to_config(None, &[]).unwrap();
    assert_eq!(set.shell.as_deref(), Some("WSL"));
    let account = AccountDraft::of(by(&list, "claude3")).to_config();
    assert_eq!(account.shell.as_deref(), Some("zsh"));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum shell_is_each`
Expected: compile errors (no `shell` on `AgentEntry`, `Draft`, `AgentConfig`).

- [ ] **Step 3: Implement** the Interfaces. In `effective`, ours and theirs take `text(&set_.shell)` /
  `text(&set.shell)`, and the accounts' `AgentEntry` sets `shell` from its own entry before `..agent.clone()`. In
  `settings/agents.rs`, the dialog's `save` builds its `Draft` with `shell: draft.shell.clone()` (Task 4 replaces it
  with the list's choice), so editing an entry never drops a hand-written shell. `check_account` stays: it refuses
  only `command`, `resume`, `generate` and `models`. Update the two spec lines named in Files.

- [ ] **Step 4: Run it to see it pass, with the neighbours**

Run: `cargo test -p templum agents:: && cargo test -p templum config::`
Expected: PASS, except `agents::a_saved_agent_is_resumed_in_its_block` (fails on a clean tree here).

- [ ] **Step 5: Commit**

```bash
git add crates/templum docs/specs/agents.md docs/specs/accounts.md
git commit -m "feat(agents): an agent's entry in config.toml names the shell it opens in"
```

---

### Task 2: An agent opens in its shell

**Files:**
- Modify: `crates/templum-terminal/src/shells.rs` (`is_shell`, its test), `crates/templum-terminal/src/lib.rs` (export)
- Modify: `crates/templum/src/workspace/usage.rs` (`shell_is` uses `is_shell`; `agent_vars`, `server_vars` take the
  shell; `typed` goes)
- Modify: `crates/templum/src/workspace/blocks.rs` (`agent_shell`, `start_agent`, `log_in`)
- Modify: `docs/ARCHITECTURE.md` (the canvas paragraph: an agent's block runs its agent's shell, `[agents] shell`)
- Test: `crates/templum/src/workspace/tests/agents.rs`, `tests/usage.rs` (its `typed` calls become
  `typed_in(None, ..)`, its `agent_vars` calls take `None`)

**Interfaces:**
- Consumes: `AgentEntry::shell` (Task 1); card 60's `Workspace::shells`, `spawn_block_with(rect, folder, vars, shell,
  window, cx)`, `typed_in(shell, agent, command)`, `Pane::shell`, `add_saved`.
- Produces:
  - `templum_terminal::is_shell(program: &str, name: &str) -> bool`: `program`'s file stem is `name`, in any case
    (`Path::file_stem`, `eq_ignore_ascii_case`).
  - `Workspace::agent_shell(&self, entry: &AgentEntry) -> Option<&ShellOption>` (`pub(crate)`, blocks.rs): the shell
    of `entry.shell`'s label in `self.shells`; None without a label, for a label not found, and for WSL on an account
    (`entry.account.is_some()`). Prints nothing (Settings asks every frame).
  - `start_agent` (signature unchanged) runs `agent_shell(entry)`, printing the Global Constraints' stderr line when
    `entry.shell` is set but `agent_shell` is None; passes its program to `agent_vars` and `typed_in`, and the shell to
    `spawn_block_with`. `log_in` runs the account's `agent_shell` the same way.
  - `Workspace::agent_vars(&self, agent: Option<&AgentEntry>, shell: Option<&str>, set: &dyn Fn(&str) -> bool) ->
    Vec<(String, String)>`: `shell` is a program, as `typed_in`'s; `server_vars` gets it and calls `gives(shell)`.
  - `Workspace::typed` is deleted (its one caller moves to `typed_in`); doc comments that name `typed` name `typed_in`.

- [ ] **Step 1: Write the failing tests**

In `shells.rs`'s `tests`:

```rust
#[test]
fn is_shell_names_a_program_by_its_file_stem() {
    assert!(is_shell("C:/Windows/System32/wsl.exe", "wsl"));
    assert!(is_shell("PowerShell.exe", "powershell"));
    assert!(is_shell("/opt/My Shells/fish", "fish"));
    assert!(!is_shell("/usr/bin/wslu", "wsl"));
}
```

In `tests/agents.rs`, a helper and three tests:

```rust
/// Made-up shells (docs/specs/shells.md, A shell for each agent): Quiet runs the tests' quiet
/// program; WSL is a path that never runs. `pub(super)`: `tests/spawn.rs` uses it too.
pub(super) fn agent_shells(ws: &Entity<Workspace>, cx: &mut TestAppContext) -> ShellOption {
    let (program, args) = quiet();
    let quiet_shell = ShellOption { label: "Quiet".into(), program, args };
    let wsl = ShellOption { label: "WSL".into(), program: "C:/Windows/System32/wsl.exe".into(), args: vec![] };
    ws.update(cx, |ws, _| ws.shells = vec![quiet_shell.clone(), wsl]);
    quiet_shell
}

#[gpui_kit::test]
fn an_agent_opens_in_its_own_shell_and_its_block_keeps_it(cx: &mut TestAppContext) {
    // open(cx); quiet = agent_shells(..);
    // with_agents("[agents.claude]\nshell = \"Quiet\"\n[agents.codex]\nshell = \"PowerShell 7\"\n")
    // claude, codex, gemini = launch(handle, &ws, cx, id, folder)
    // claude's pane.shell == Some(quiet); its item's (Pane::item) shell == Some("Quiet")
    // codex's pane.shell == None ("a label not found here: the default"); gemini's == None
    // with_agents(""): the setting is gone; add_saved(session::Item::Terminal(claude's item))
    //   -> the new block's pane.shell == Some(quiet) ("its own saved shell")
}

#[gpui_kit::test]
fn an_account_never_opens_in_wsl(cx: &mut TestAppContext) {
    // data tempdir; open_in(cx, None, Some(data)); quiet = agent_shells(..)
    // with_agents(&format!("{CLAUDE2}shell = \"WSL\"\n[agents.claude3]\nof = \"claude\"\nshell = \"Quiet\"\n"))
    // ws.agent_shell(claude2's entry) == None; launch claude2 -> pane.shell == None, pane.account == Some("claude2")
    // launch claude3 -> pane.shell == Some(quiet); ws.log_in("claude3", window, cx) -> its pane.shell == Some(quiet)
}

/// None of Templum's words cross into WSL (docs/specs/shells.md, What works in WSL).
#[cfg(unix)]
#[gpui_kit::test]
fn an_agent_set_to_wsl_gets_none_of_templums_words(cx: &mut TestAppContext) {
    // As card 60's a_restored_agents_line_follows_its_terminals_shell: data, folder and bin tempdirs;
    // bin/wsl a link to templum_terminal::locate("cat"); open_as(cx, Some(quiet()), None, Some(data));
    // ws.shells = [ShellOption { label: "WSL", program: bin/wsl, args: [] }]
    // with_agents("[agents.claude]\nshell = \"WSL\"\n"); id = launch(claude)
    // shown = super::bus::screen_with(&ws, cx, id, "claude"); assert!(!shown.contains("--settings"), "{shown}")
}
```

In `tests/usage.rs`'s `blocks_templum_opens_get_their_agents_variable`, after the first OpenCode assertion:

```rust
let wsl = Some("C:/Windows/System32/wsl.exe");
assert_eq!(ws.agent_vars(opencode.as_ref(), wsl, &unset), [], "nothing crosses into WSL");
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum-terminal is_shell && cargo test -p templum own_shell`
Expected: compile errors (`is_shell`, `agent_shell` and the new `agent_vars` don't exist), then, once they compile,
`an_agent_opens_in_its_own_shell_and_its_block_keeps_it` fails: claude's block has no shell.

- [ ] **Step 3: Implement** the Interfaces above.

- [ ] **Step 4: Run them to see them pass, with the neighbours**

Run: `cargo test -p templum-terminal shells && cargo test -p templum agents && cargo test -p templum usage && cargo
test -p templum persistence && cargo test -p templum spawn`
Expected: PASS, except the two tests that fail on a clean tree here.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-terminal crates/templum docs/ARCHITECTURE.md
git commit -m "feat(agents): an agent opens in the shell set for it, and its block keeps that shell"
```

---

### Task 3: Agents in WSL count as installed, and spawn_agent refuses them

**Files:**
- Modify: `crates/templum/src/workspace/blocks.rs` (`in_wsl`, `agent_installed`, `spawn_problem`)
- Modify: `crates/templum/src/menu/mod.rs` (`agents_for_menu`: `pub(super)`, asks `agent_installed`)
- Modify: `crates/templum/src/settings/agents.rs` (`agent_row` asks `ws.agent_installed`)
- Modify: `crates/templum/src/welcome.rs` (`open_welcome`'s `installed` asks `agent_installed`)
- Modify: `crates/templum/src/workspace/spawn.rs` (`spawn_agent`), `crates/templum/src/workspace/control.rs` (its
  open of an agent): their `templum_terminal::installed` checks become `spawn_problem`
- Test: `crates/templum/src/workspace/tests/agents.rs`, `tests/spawn.rs`

**Interfaces:**
- Consumes: `agent_shell`, `is_shell` (Task 2).
- Produces (blocks.rs, `pub(crate)`):
  - `Workspace::in_wsl(&self, entry: &AgentEntry) -> bool`: `agent_shell(entry)`'s program, else
    `config.shell.program`, is WSL (`is_shell(p, "wsl")`).
  - `Workspace::agent_installed(&self, entry: &AgentEntry) -> bool`: `in_wsl(entry)` or
    `templum_terminal::installed(entry.program())`.
  - `Workspace::spawn_problem(&self, entry: &AgentEntry) -> Option<String>`: the Global Constraints' WSL refusal when
    `in_wsl`, else today's `"{title} isn't installed on this computer ({program} wasn't found)."` when not
    installed, else None.

- [ ] **Step 1: Write the failing tests**

In `tests/agents.rs`:

```rust
#[gpui_kit::test]
fn an_agent_that_runs_in_wsl_counts_as_installed(cx: &mut TestAppContext) {
    // open(cx); agent_shells(..);
    // with_agents("[agents.mine]\nname = \"Mine\"\ncommand = \"templum-no-such-agent\"\nshell = \"WSL\"\n\
    //              [agents.theirs]\nname = \"Theirs\"\ncommand = \"templum-no-such-agent\"\n")
    // let listed = |ws: &Workspace, id| ws.agents_for_menu().into_iter().find(|(a, _)| a.id == id).unwrap().1;
    // assert!(listed(ws, "mine")); assert!(!listed(ws, "theirs"));
    // ws.config.shell.program = Some("C:/Windows/System32/wsl.exe".into());
    // assert!(listed(ws, "theirs"), "Settings ▸ Shell is WSL: every agent runs there");
    // ws.open_welcome(false, window, cx): ws.welcome's installed contains "mine"
}
```

In `tests/spawn.rs`:

```rust
#[gpui_kit::test]
fn spawn_agent_refuses_an_agent_that_runs_in_wsl(cx: &mut TestAppContext) {
    // (handle, ws, mine) = spawner(cx); (terminal, _) = me(&ws, cx, mine);
    // super::agents::agent_shells(&ws, cx); the "claude" entry's shell = Some("WSL")
    // count the terminals; call(handle, &ws, cx, &terminal, "spawn_agent", json!({ "agent": "claude" }))
    // == (true, "Claude Code runs in WSL, where Templum can't see it start nor type its prompt: open it from the canvas.")
    // the same count: no block opened
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum runs_in_wsl`
Expected: compile error (`agents_for_menu` is private), then both fail: `mine` isn't installed, and the spawn opens a
block.

- [ ] **Step 3: Implement** the Interfaces, and point the five callers in Files at them. The Templum Agent's open
  (`control.rs`) shares `spawn_problem`, so `spawn_agent`'s test covers it; its `list_agents` keeps
  `templum_terminal::installed` (the spec: it says what this computer has).

- [ ] **Step 4: Run them to see them pass, with the neighbours**

Run: `cargo test -p templum agents && cargo test -p templum spawn && cargo test -p templum menus && cargo test -p
templum welcome && cargo test -p templum templum_agent`
Expected: PASS, except `agents::a_saved_agent_is_resumed_in_its_block`.

- [ ] **Step 5: Commit**

```bash
git add crates/templum
git commit -m "feat(agents): an agent set to WSL can be opened though Windows doesn't find it, and spawn_agent says why it can't"
```

---

### Task 4: The Shell list in the agent dialog, and the rows

**Files:**
- Modify: `crates/templum/src/settings/agents.rs` (`shell_choices`, `description`, `agent_row`, `agent_dialog`)
- Modify: `crates/templum/src/settings/accounts.rs` (`account_row`'s description gets the same suffix)
- Modify: `docs/specs/agents.md` (Changing ours, adding yours: the dialog has Shell; a row says its shell)
- Test: `crates/templum/src/settings/tests.rs`, `crates/templum/src/workspace/tests/agents.rs`

**Interfaces:**
- Consumes: `agent_shell`, `agent_installed` (Tasks 2, 3), `is_shell`, `Workspace::default_shell_name`, `menu_button`
  (`settings/row.rs`).
- Produces:
  - `settings::agents::shell_choices(default: &str, shells: &[ShellOption], account: bool, current: Option<&str>) ->
    Vec<(SharedString, SharedString)>` (`pub(super)`): `("", "Default (<default>)")`, then `(label, label)` for each
    shell (without `is_shell(program, "wsl")` ones when `account`), then `(current, "<current> (not available)")`
    when `current` is set and not among them.
  - `settings::agents::description(agent: &AgentEntry, installed: bool, found: bool) -> String`: today's text, then
    ` · in <label>` when `agent.shell` is set, ` (not available)` after it when `!found`. `agent_row` passes
    `ws.agent_installed(agent)` and `ws.agent_shell(agent).is_some()`.
  - The dialog's state: `Entity<Option<String>>` made from `draft.shell`; a pick sets it (`""` → None) and notifies
    the workspace, as the account dialog's `Form` does, so the dialog redraws; Save's `Draft` takes it.

- [ ] **Step 1: Write the failing tests**

In `settings/tests.rs`, and update `an_agent_not_installed_says_how_to_install_it` to pass `found: true`:

```rust
#[test]
fn the_shell_list_is_default_then_every_shell_found() {
    let shell = |label: &str, program: &str| ShellOption { label: label.into(), program: program.into(), args: vec![] };
    let shells = [shell("zsh", "/bin/zsh"), shell("WSL", "C:/Windows/System32/wsl.exe")];
    let pairs = |list: Vec<(SharedString, SharedString)>| -> Vec<(String, String)> {
        list.into_iter().map(|(v, l)| (v.to_string(), l.to_string())).collect()
    };
    let pair = |v: &str, l: &str| (v.to_string(), l.to_string());
    assert_eq!(
        pairs(shell_choices("zsh", &shells, false, None)),
        [pair("", "Default (zsh)"), pair("zsh", "zsh"), pair("WSL", "WSL")]
    );
    assert_eq!(
        pairs(shell_choices("zsh", &shells, true, None)),
        [pair("", "Default (zsh)"), pair("zsh", "zsh")],
        "an account's leaves WSL out"
    );
    let gone = pairs(shell_choices("zsh", &shells, false, Some("fish")));
    assert_eq!(gone.last(), Some(&pair("fish", "fish (not available)")));
    let account = pairs(shell_choices("zsh", &shells, true, Some("WSL")));
    assert_eq!(account.last(), Some(&pair("WSL", "WSL (not available)")));
}

#[test]
fn a_rows_description_says_its_shell() {
    // effective of "[agents]\nclaude = { shell = \"PowerShell 7\" }\nmine = { name = \"Mine\", command = \"mine --go\", shell = \"WSL\" }\n"
    // description(claude, true, true) == "claude · in PowerShell 7"
    // description(claude, true, false) == "claude · in PowerShell 7 (not available)"
    // description(mine, true, true) == "Yours · mine --go · in WSL"
}
```

In `tests/agents.rs`, following `the_agents_dialog_sets_a_commit_message_command` (`tests/git_message.rs`) for the
panel, the tab, the 300 ms slide and the dialog's `ok`:

```rust
#[gpui_kit::test]
fn the_agent_dialog_sets_its_shell_and_the_switch_keeps_it(cx: &mut TestAppContext) {
    // dir; open_with(cx, Some(dir)); agent_shells(..) (Quiet, WSL); ws.config.shell.program = None;
    // ws.default_shell = "zsh"  -> the list: Default (zsh), Quiet, WSL
    // Settings ▸ Agents ▸ edit-agent-claude; within("dialog").click("menu-agent-shell"); popup-menu click(2usize) (WSL)
    // render: within("dialog").try_find("agent-shell-wsl").is_some()
    // click("menu-agent-shell") again, popup-menu click(1usize) (Quiet): the note is gone; within("dialog").click("ok")
    // ws.config.agents["claude"].shell == Some("Quiet")
    // click("show-agent-claude"): config.agents["claude"] has show == Some(false) and shell == Some("Quiet")
    // edit-agent-claude, menu-agent-shell, click(0usize) (Default), ok: config.agents["claude"].shell == None
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p templum shell_list && cargo test -p templum says_its_shell && cargo test -p templum
dialog_sets_its_shell`
Expected: compile errors (`shell_choices`, the new `description`), then the UI test fails: no `menu-agent-shell`.

- [ ] **Step 3: Implement** the Interfaces. The list goes under Resume command, labelled "Shell", the note under it
  only while the current choice's shell is WSL (`is_shell`). `account_row` appends the same suffix from
  `ws.agent_shell(entry)` (`found` as in `agent_row`).

- [ ] **Step 4: Run them to see them pass, with the neighbours**

Run: `cargo test -p templum settings && cargo test -p templum agents && cargo test -p templum git_message`
Expected: PASS, except `agents::a_saved_agent_is_resumed_in_its_block`.

- [ ] **Step 5: Commit**

```bash
git add crates/templum docs/specs/agents.md
git commit -m "feat(agents): pick each agent's shell in its dialog, Default following Settings ▸ Shell"
```

---

### Task 5: The Shell list in the account dialog

**Files:**
- Modify: `crates/templum/src/settings/accounts.rs` (`Form`: `shell`, `shells`, `default_shell`; `Form::new`,
  `to_save`, `render`)
- Modify: `docs/specs/accounts.md` (The dialog: its Shell list, without WSL)
- Test: `crates/templum/src/workspace/tests/settings_panel.rs`

**Interfaces:**
- Consumes: `shell_choices(.., account: true, ..)` (Task 4), `AccountDraft::shell` (Task 1).
- Produces: `Form::shell: Option<String>` (the entry's, None for a new account), and `Form::shells:
  Vec<ShellOption>` and `Form::default_shell: String` read from the workspace in `Form::new`, as `agents` and `taken`
  are. `to_save` puts `shell` in both drafts. The list (`menu-account-shell`, labelled "Shell") sits under the name of
  a new account and under Folder for an edited one.

- [ ] **Step 1: Write the failing test** in `tests/settings_panel.rs`, after
  `editing_an_older_account_keeps_its_name_and_folder` (its config.toml with `[agents.claude2]`, `account_dirs`,
  `open_in`, `open_accounts`, `edit_account`, `pick`, `create`):

```rust
#[gpui_kit::test]
fn the_account_dialog_sets_its_shell_without_wsl(cx: &mut TestAppContext) {
    // config.toml: "[agents.claude2]\nof = \"claude\"\n"; open_in(cx, Some(config), Some(data));
    // ws.shells = [WSL (C:/Windows/System32/wsl.exe), Quiet (quiet())]
    // open_accounts; edit_account(handle, cx, "edit-account-claude2");
    // pick(handle, cx, "menu-account-shell", 1)   // Default, Quiet: WSL left out
    // create(handle, cx); ws.save(); parse config.toml: agents["claude2"].shell == Some("Quiet")
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p templum account_dialog_sets_its_shell`
Expected: FAIL: no `menu-account-shell`.

- [ ] **Step 3: Implement** the Interfaces.

- [ ] **Step 4: Run it to see it pass, with the neighbours**

Run: `cargo test -p templum settings_panel && cargo test -p templum accounts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum docs/specs/accounts.md
git commit -m "feat(accounts): pick an account's shell in its dialog"
```

---

### Task 6: Finish

**Files:**
- Modify: `docs/specs/shells.md` (header: *A shell for each agent* Implemented), `docs/specs/agents.md` and
  `docs/specs/accounts.md` (their **Shells** lines: drop "planned")
- Modify: `docs/minors.md` only if a name changed: its three WSL entries (Programs in a terminal, Connections, Agent
  accounts) name `Space::see`, `Workspace::gives`, `Workspace::spawn_problem` and `Workspace::agent_shell`

- [ ] **Step 1:** Update the docs above; check the minors' names against the code (`grep -n "fn spawn_problem\|fn
  agent_shell" crates/templum/src/workspace/blocks.rs`).
- [ ] **Step 2: Run the full check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: fmt and clippy clean; every test passes but the two that read the real `~/.claude` on this machine. Report
the result as it is, failures included. Optionally run the Windows clippy of the Global Constraints.

- [ ] **Step 3: Commit**

```bash
git add docs
git commit -m "docs: a shell for each agent is implemented"
```

- [ ] **Step 4: Report** what was checked: the headless tests (no window was driven by hand); `is_shell` touches
  `templum-terminal`, so the pull request from `development` to `main` pastes `--bench`'s JSON line (CLAUDE.md,
  Performance); Windows and macOS not run by hand.
