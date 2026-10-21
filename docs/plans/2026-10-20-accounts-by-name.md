# Accounts by Name: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kanban card 65: **Add an account** asks for a name (`claudinho`) instead of a folder; that name becomes the
account's id and a command in every shell of the computer (`~/.local/bin/claudinho`), which Templum's terminals also
define as a function carrying Templum's server; a block where an account's agent runs, typed or opened, is that
account's block.

**Architecture:** The command's text and its file handling live in `templum-usage` (UI-free, beside the accounts'
folder logic: `accounts/command.rs`). The name's rules and the commands folder are `crates/templum/src/agents.rs`
functions over injected lookups, so tests use temporary folders. `Workspace` writes a command at Create, removes it at
Delete, and checks every account's command when the agents in effect are computed (`Workspace::commands` keeps each
one's state for the rows and the dialog). The snapshot reads the found agent's variable from its process
(`Detect::var`, `Inspection::folder_var`), and `see` makes the block that account's. Each new terminal gets
`TEMPLUM_ACCOUNTS`, from which the four shell integrations define one function per account. The dialog is rebuilt in
the kanban card dialog's language with its helpers.

**Tech Stack:** Rust 2024; gpui-kit `=0.7.3` (`open_alert_dialog`, `.title(header)`, `.footer(..)`, `Input`,
`InputEvent::PressEnter`); sysinfo 0.31.4 (`ProcessRefreshKind::with_environ`, `Process::environ`); the headless tests
of `crates/templum/src/workspace/tests/` (`window.click`, `cx.simulate_input`, `window.within("popup-menu")`); the
real-shell harness of `crates/templum-terminal/src/integration.rs` (`Home`, `fake_agents`, `with_agents`).

**Spec:** [docs/specs/accounts.md](../specs/accounts.md): "Its name and its command", "Typed in a terminal",
"Settings ▸ Agents ▸ Accounts", "Calls made for card 65". Read those first: they hold every decision and every word
of copy this plan uses.

## Global Constraints

- The commands folder is `home/.local/bin` on every OS (`std::env::home_dir()`); on Windows two files per account,
  `<id>` (sh) and `<id>.cmd`; elsewhere one, `<id>`, mode `0o755`.
- The marker is the second line of every command file: `# templum-account <id> <data>` (sh) or
  `rem templum-account <id> <data>` (cmd), `<data>` being Templum's data folder as `Path::display` writes it. A file
  is Templum's own only with that exact line for that id and data folder; Templum writes, rewrites and deletes only
  its own.
- Name rule 1 is `^[a-z][a-z0-9_-]{0,31}$`. Copy, verbatim from the spec: "Use lowercase letters, digits, - and _,
  starting with a letter"; "{name} is one of Templum's agents"; "templum is Templum's own command"; "{name} is
  already an agent or an account in Templum"; "{name} is a word of your shell"; "{name} is already a command: {path}";
  "Templum still keeps a folder for {name} ({tilde path}): choose another name"; "Couldn't make the command {id}:
  {error}"; "Its command {id} is removed."
- **Tests never touch the person's files:** the commands folder is `None` under `cfg(test)` unless a test sets the
  thread-local `agents::COMMANDS` to a temporary folder (as `agents::OWN_FOLDERS` does); no test reads or writes
  `~/.local/bin`, the person's PATH, or an agent's own folder. CI runs Linux, macOS and Windows and has no agent CLI
  installed: gate OS-specific assertions with `cfg!` and write Windows paths as Windows does.
- Only `crates/templum` uses gpui-kit; no new dependency; no `unsafe`.
- While iterating, run only the affected tests: `cargo test -p <crate> <filter>`. Run the full
  `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` once, in
  Task 7. Two tests fail on a clean tree in this environment and are not this plan's:
  `agents::a_saved_agent_is_resumed_in_its_block` and `persistence::two_blocks_one_folder_two_accounts_resume_their_own`.
- Never wait on a process with `pgrep -f "<its command>"` in a loop: it matches the waiting shell itself. Never inject
  keystrokes or mouse events into windows (`wtype`, `ydotool`…): the person's own Templum window is open.
- Every minor deferred goes into `docs/minors.md` under **Agent accounts**; a minor fixed is deleted in the same commit.
- Commit each task on `development` (never `main`, never push), titled as CLAUDE.md says.

## Review Focus

1. **A name typed with a capital or a space** (`Claudinho`, `claude inho`): the rule-1 words under the field, Create
   faint, nothing written. Task 2 pins the rule, Task 6 the dialog.
2. **An installed Templum and a `--dev` one with the same account name:** each sees the other's command as taken and
   never rewrites nor deletes it. Task 1 pins `is_own` with another data folder; Task 2 pins rule 6 with it.
3. **A data folder with a space, a `'` or a `%`** (macOS's `Application Support`, a Windows user named `O'Neil`): the
   command's text quotes it. Task 1 pins the exact texts.
4. **Deleting an account while a typed `claudinho` runs in a plain block:** refused, since the block is now the
   account's. Task 4's headless test asserts `account_in_use` after `see`.
5. **An account made by card 37's dialog** (`name = "Claude Code 2"`, a chosen `folder`): it gets the command
   `claude2` at start, and Edit ▸ Save keeps `name` and `folder`. Task 3 pins the start; Task 6 the edit.

---

### Task 1: The command file

**Files:**
- Create: `crates/templum-usage/src/accounts/command.rs`
- Modify: `crates/templum-usage/src/accounts.rs` (`pub mod command;` beside `mod folder;`)

**Interfaces:**
- Produces, in `templum_usage::accounts::command`:
  - `pub struct Command<'a> { pub id: &'a str, pub title: &'a str, pub program: &'a str, pub var: &'a str, pub folder: &'a Path, pub data: &'a Path }` (`title`: the agent's, "Claude Code")
  - `pub fn files(c: &Command, windows: bool) -> Vec<(String, String)>`: each file's name and text
  - `pub fn is_own(path: &Path, id: &str, data: &Path) -> bool`
  - `pub fn write(dir: &Path, c: &Command) -> io::Result<bool>`: true when it wrote; `ErrorKind::AlreadyExists` when a
    file of that name isn't Templum's own (nothing written)
  - `pub fn remove(dir: &Path, id: &str, data: &Path) -> io::Result<()>`: removes only Templum's own files of `id`
  - `pub const SHELL_WORDS: &[&str]`

- [ ] **Step 1: Write the failing tests** in `command.rs`'s `#[cfg(test)] mod tests`:

```rust
fn claudinho<'a>(folder: &'a Path, data: &'a Path) -> Command<'a> {
    Command { id: "claudinho", title: "Claude Code", program: "claude", var: "CLAUDE_CONFIG_DIR", folder, data }
}

#[test]
fn the_command_runs_its_agent_on_its_folder() {
    let (data, folder) = (Path::new("/d/it's 100%"), Path::new("/d/it's 100%/accounts/claudinho"));
    let sh = "#!/bin/sh\n# templum-account claudinho /d/it's 100%\n\
        # Made by Templum: opens Claude Code on its account claudinho (Settings ▸ Agents ▸ Accounts).\n\
        # Deleted with the account; Templum writes it again when it changes.\n\
        export CLAUDE_CONFIG_DIR='/d/it'\\''s 100%/accounts/claudinho'\nexec claude \"$@\"\n";
    assert_eq!(files(&claudinho(folder, data), false), [("claudinho".to_string(), sh.to_string())]);
    let windows = files(&claudinho(folder, data), true);
    assert_eq!(windows[0], ("claudinho".to_string(), sh.to_string()));
    let cmd = "@echo off\r\nrem templum-account claudinho /d/it's 100%\r\n\
        rem Made by Templum: opens Claude Code on its account claudinho (Settings > Agents > Accounts).\r\n\
        rem Deleted with the account; Templum writes it again when it changes.\r\n\
        setlocal\r\nset \"CLAUDE_CONFIG_DIR=/d/it's 100%%/accounts/claudinho\"\r\nclaude %*\r\nexit /b %errorlevel%\r\n";
    assert_eq!(windows[1], ("claudinho.cmd".to_string(), cmd.to_string()));
}

#[test]
fn a_command_is_written_and_known_as_templums() { /* tempdir `dir`, data `d`: write → Ok(true); every name of
   files(.., cfg!(windows)) exists; is_own(dir/claudinho, "claudinho", d); !is_own(.., "claudio", d);
   !is_own(.., "claudinho", other); on unix mode & 0o777 == 0o755; no file ending ".templum-new" in dir */ }

#[test]
fn a_file_of_someone_else_is_never_touched() { /* dir/claudinho = "#!/bin/sh\necho mine\n": write → Err with
   kind AlreadyExists, text unchanged; remove → Ok, text unchanged. Same with the command another data folder wrote. */ }

#[test]
fn it_is_written_again_only_when_its_text_differs() { /* write → Ok(true); write again → Ok(false); with another
   folder → Ok(true) and the file holds the new folder */ }

#[test]
fn removing_one_leaves_the_others() { /* write claudinho and claudio; remove claudinho: none of its files left,
   claudio's all there */ }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p templum-usage command`
Expected: FAIL to compile (`files`, `write`… not defined).

- [ ] **Step 3: Implement** `files`, `is_own`, `write`, `remove`, `SHELL_WORDS` in `command.rs`.
  - sh: the folder always in single quotes, `'` as `'\''`; the program bare when it holds only letters, digits, `-`,
    `_`, `.`, `/`, else single-quoted. Lines end with `\n`. cmd: lines end with `\r\n`; `%` in the folder as `%%`.
  - `is_own`: reads the file only when it is under 4 KiB; its second line, without `# ` or `rem `, equals
    `templum-account {id} {data.display()}`.
  - `write`: makes `dir` (`create_dir_all`); for each file, an existing one not own → `AlreadyExists`; same text → skip;
    else write `<name>.templum-new` (0o755 on unix), then `std::fs::rename` over the name. Returns whether any file
    was written.
  - `SHELL_WORDS` (bash, zsh, fish, PowerShell builtins and keywords), exactly:
    `alias autoload bg bind bindkey break builtin caller case catch cd class command compgen complete contains continue
    count data declare dirs disown do done echo elif else emit emulate enable end enum esac eval exec exit export false
    fc fg fi filter finally for foreach function functions getopts hash help history if in integer jobs kill let local
    logout mapfile noglob not or param popd print printf process pushd pwd read readarray readonly repeat return select
    set setopt shift shopt source status string suspend switch test then throw time times trap true try type typeset
    ulimit umask unalias unset unsetopt until using wait whence where which while zle zmodload zstyle and begin`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p templum-usage command`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit** — `git commit -m "feat(accounts): write and remove an account's command file, never someone else's"`

---

### Task 2: The name's rules and the commands folder

**Files:**
- Modify: `crates/templum/src/agents.rs` (beside `new_account_id`; tests in its `mod tests`)
- Modify: `crates/templum-terminal/src/agent.rs` (`program_dirs`, near line 796), `crates/templum-terminal/src/lib.rs` (re-export)

**Interfaces:**
- Consumes: Task 1's `is_own`, `SHELL_WORDS`.
- Produces:
  - `templum_terminal::shell_path_dirs() -> Vec<PathBuf>`: `path_var()`'s folders and, on Unix, `SHELL_PATH`'s once
    read (`program_dirs` starts from it)
  - `templum_terminal::on_path(dir: &Path, dirs: &[PathBuf]) -> bool`: compared canonicalized when both exist, else
    as written
  - `agents::commands_dir() -> Option<PathBuf>`: `home_dir()/.local/bin`; under `cfg(test)` the thread-local
    `pub(crate) static COMMANDS: RefCell<Option<PathBuf>>` (None by default), as `OWN_FOLDERS`
  - `agents::is_command_word(id: &str) -> bool`: rule 1
  - `agents::command_problem(id: &str, data: Option<&Path>, commands: Option<&Path>, program: impl Fn(&str) -> Option<PathBuf>) -> Option<String>`: rules 1, 2, 4, 5, 6
  - `agents::name_problem(name: &str, taken: impl Fn(&str) -> bool, data: Option<&Path>, commands: Option<&Path>, program: impl Fn(&str) -> Option<PathBuf>) -> Option<String>`: rules 1 to 7, in the spec's order; `program` is `templum_terminal::locate` in the app

- [ ] **Step 1: Write the failing tests** in `agents.rs`:

```rust
#[test]
fn a_name_must_be_a_lowercase_word() {
    let rule = "Use lowercase letters, digits, - and _, starting with a letter";
    for bad in ["Claudinho", "claude inho", "1claude", "-x", "", "clau$de", &"a".repeat(33)] {
        assert_eq!(name_problem(bad, |_| false, None, None, |_| None).as_deref(), Some(rule), "{bad}");
    }
    for good in ["claudinho", "claudio", "work-2", "a_b", &"a".repeat(32)] {
        assert_eq!(name_problem(good, |_| false, None, None, |_| None), None, "{good}");
    }
}

#[test]
fn a_name_isnt_an_agent_a_shell_word_nor_taken() {
    let p = |n: &str| name_problem(n, |x| x == "claudio", None, None, |_| None);
    assert_eq!(p("claude").as_deref(), Some("claude is one of Templum's agents"));
    assert_eq!(p("templum").as_deref(), Some("templum is Templum's own command"));
    assert_eq!(p("claudio").as_deref(), Some("claudio is already an agent or an account in Templum"));
    for word in ["cd", "source", "end", "test"] {
        assert_eq!(p(word), Some(format!("{word} is a word of your shell")));
    }
}

#[test]
fn a_name_isnt_a_command_of_the_computer() {
    // A program found: its path. A file of the commands folder: its path, unless it is Templum's own.
    // tempdirs `commands`, `data`; program |n| (n == "gh2").then(|| PathBuf::from("/usr/bin/gh2"))
    // "gh2" → "gh2 is already a command: /usr/bin/gh2"
    // commands/claudinho "#!/bin/sh\n" → "claudinho is already a command: <commands/claudinho>"
    // command::write(commands, claudinho for another data folder) → taken the same way (an installed and a --dev Templum)
    // command::write(commands, claudinho for `data`) first → None (its own), also when `program` finds that file
    // on windows only: commands/claudinho.exe → taken
}

#[test]
fn a_name_whose_folder_is_still_kept_is_refused() {
    // data/accounts/claudinho made → Some(format!("Templum still keeps a folder for claudinho ({}): choose another name",
    //     tilde(&data.join("accounts").join("claudinho"))))
}

#[test]
fn an_older_accounts_id_may_be_a_command() {
    assert_eq!(command_problem("claude2", None, None, |_| None), None);
    assert!(command_problem("Work", None, None, |_| None).is_some());
    assert_eq!(command_problem("test", None, None, |_| None).as_deref(), Some("test is a word of your shell"));
}
```

And in `templum-terminal/src/agent.rs`'s tests:

```rust
#[test]
fn a_folder_is_on_path_through_a_dot_dot() {
    // tempdir t: t/.local/bin and t/.local/share made
    // on_path(&t/.local/bin, &[t/.local/share/../bin]) == true; on_path(.., &[t/other]) == false
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p templum name_ && cargo test -p templum command_problem; cargo test -p templum-terminal on_path`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** `shell_path_dirs`, `on_path` (and make `program_dirs` start from `shell_path_dirs`),
  `commands_dir`, `COMMANDS`, `command_problem`, `name_problem` with the Global Constraints' copy. Rule 5 skips a
  found path that `is_own(path, id, data)`; rule 6 looks at `commands/<id>` and, on Windows, `commands/<id>.*`.

- [ ] **Step 4: Run them to verify they pass** (same commands). Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(accounts): check an account's name as a command: a word, free, not one of the computer's"`

---

### Task 3: Create by name, Delete removes the command, every account's command checked

**Files:**
- Modify: `crates/templum/src/agents.rs` (`Account::named`, `AccountDraft::of`, `effective`; delete `new_account_id`
  and its test `account_ids_count_from_two`)
- Modify: `crates/templum/src/workspace/preferences.rs` (`save_account`, `delete_account`, the config change at ~364)
- Modify: `crates/templum/src/workspace/mod.rs` (the `Workspace` field, its start at ~1339)
- Modify: `crates/templum/src/settings/accounts.rs` (only the calls to `save_account`/`new_account_id`, to compile; the
  dialog itself is Task 6)
- Create: `crates/templum/src/workspace/tests/accounts.rs` (registered in `tests/mod.rs`)
- Modify: `docs/minors.md` (delete "The dialog may show another id than Save writes"; in "The menu switch of an
  account rewrites its entry" drop the part about `name`; reword "A deleted account's id is given to the next one" to
  a typed name given again)

**Interfaces:**
- Consumes: Task 1's `Command`, `write`, `remove`, `is_own`; Task 2's `name_problem`, `command_problem`, `commands_dir`.
- Produces:
  - `Account { …, pub named: bool }`: its title is config.toml's `name`; `AccountDraft::of` writes `name` only then
  - `Workspace::commands: BTreeMap<String, Result<PathBuf, String>>`: per account id, its command's main file
    (`<id>.cmd` on Windows, else `<id>`) or why it has none
  - `Workspace::sync_commands(&mut self) -> Vec<String>`: for each account, writes its command when
    `command_problem` is None (an `AlreadyExists` is a reason, any other error a failure), fills `commands`; returns
    the failures' notification texts
  - `Workspace::save_account(&mut self, id: &str, draft: &AccountDraft, window, cx) -> Result<String, String>`: a new
    account when `id` isn't a key of the pending config: `name_problem` first (taken: the pending keys and
    `self.agents`' ids, `eq_ignore_ascii_case`); after the entry and `apply`, its command written (a failure is a
    notification, not an `Err`)
  - `delete_account` also calls `command::remove` for the account; `Workspace::command_removed(id) -> bool` (its own
    command exists) for Task 6's question

- [ ] **Step 1: Write the failing tests** in `tests/accounts.rs` (each sets `crate::agents::COMMANDS` to a tempdir and
  `OWN_FOLDERS` to another, opens with `open_in(cx, Some(config), Some(data))`):
  - `creating_an_account_by_name_writes_its_command`: `save_account("claudinho", &AccountDraft { of: "claude".into(),
    show: true, ..Default::default() }, ..)` → `Ok("claudinho")`; after `ws.save()`, config.toml's
    `agents["claudinho"]` has `of = "claude"` and `name == None`; `commands/claudinho` is Templum's own
    (`is_own(.., "claudinho", data)`); `ws.commands["claudinho"]` is `Ok`.
  - `a_taken_name_is_refused_on_save`: `save_account("claude", ..)` → `Err("claude is one of Templum's agents")`; no
    `[agents.claude]` entry.
  - `deleting_an_account_removes_its_command_only`: create `claudinho` and `claudio`; `delete_account("claudinho")` →
    `Ok`; its files gone, `claudio`'s there; a foreign `commands/other` untouched.
  - `an_older_accounts_command_is_made_at_start`: config.toml `CLAUDE2` plus `[agents.test]\nof = "claude"\n` before
    opening → `commands/claude2` is Templum's own; no `commands/test`; `ws.commands["test"] ==
    Err("test is a word of your shell".into())`.
  - In `agents.rs`: `a_default_title_isnt_written_as_a_name`: `AccountDraft::of(by(&agents("[agents]\nclaudinho = { of = \"claude\" }\n"), "claudinho")).name == ""`, and `"Claude Code 2"` for an entry with `name = "Claude Code 2"`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p templum accounts:: && cargo test -p templum default_title`
Expected: FAIL to compile (`save_account` takes `Option<&str>`, no `commands`).

- [ ] **Step 3: Implement** the Interfaces above. `sync_commands` runs where the agents in effect are computed: at
  start (`mod.rs` ~1339; its failures shown as the font warning is) and when `config.agents` changes
  (`preferences.rs` ~364). Update every caller of `save_account` (the row's switch, tests in `settings_panel.rs`) to
  pass the id; `new_account_id` goes.

- [ ] **Step 4: Run them to verify they pass**, then the accounts tests already there:

Run: `cargo test -p templum accounts && cargo test -p templum settings_panel`
Expected: PASS, except the dialog tests Task 6 rewrites (`adding_an_account_in_settings_writes_it_and_its_links`,
`typing_a_folder_keeps_the_shares_chosen`, `an_account_folder_in_templums_folder_for_accounts_is_refused`,
`a_new_accounts_id_differs_more_than_by_case`): if one fails only because it expects `claude2`, mark it
`#[ignore = "rewritten in Task 6"]` and say so in the commit body.

- [ ] **Step 5: Commit** — `git commit -m "feat(accounts): an account is made by its name and is a command in your shells, removed with it"`

---

### Task 4: The block's account follows the folder its agent runs on

**Files:**
- Modify: `crates/templum-terminal/src/agent.rs` (`Detect`, line ~394; `Agent::detect`)
- Modify: `crates/templum-terminal/src/terminal.rs` (`Inspection` ~413, `inspect`, `inspect_one`)
- Modify: `crates/templum/src/agents.rs` (`AgentEntry::detect`, new `account_of`)
- Modify: `crates/templum/src/workspace/mod.rs` (`see`, ~877)
- Test: `crates/templum-terminal/src/terminal.rs` tests, `crates/templum/src/agents.rs` tests,
  `crates/templum/src/workspace/tests/accounts.rs`
- Modify: `docs/minors.md` (delete "A shell that sets the agent's variable again runs the agent in that folder": fixed)

**Interfaces:**
- Produces:
  - `Detect { …, pub var: Option<String> }`: the variable naming the agent's folder; `AgentEntry::detect` sets it from
    `templum_usage::accounts::kind(&self.id)` for Templum's agents, `None` otherwise
  - `Inspection { …, pub folder_var: Option<Option<PathBuf>> }`: `None` not read (no `var`, or an empty environment);
    `Some(None)` unset or empty; `Some(Some(path))`
  - `agents::account_of(agent: &str, folder: &Path, homes: &[Home]) -> Option<String>`: the home of `agent` whose
    `state` is `kind.state_dir(folder)`, `folder` normalized by `components()` as `effective` does

- [ ] **Step 1: Write the failing tests**
  - `agents.rs`: `the_folder_found_names_the_account`: homes `[Home { id: "claudinho", agent: "claude", state: "/data/accounts/claudinho" }]`; `account_of("claude", Path::new("/data/accounts/claudinho/"), &homes) == Some("claudinho")`; `/home/me/x` → `None`; agent `"codex"` → `None`.
  - `terminal.rs`, `#[cfg(target_os = "linux")]`: `a_found_agents_folder_variable_is_read`: a tempdir script named
    `claude` (`#!/bin/sh\nsleep 30\n`, 0o755) started twice with `std::process::Command`, once with
    `CLAUDE_CONFIG_DIR=/x/acc`, once with the variable removed; `inspect` with each child as the foreground and
    `Detect { id: "claude", names: vec!["claude".into()], packages: vec![], var: Some("CLAUDE_CONFIG_DIR".into()) }`,
    retried every 50 ms for up to 5 s until its agent is found → `folder_var == Some(Some("/x/acc".into()))`, then
    `Some(None)`; both children killed at the end.
  - `tests/accounts.rs`: `a_typed_account_makes_its_block_the_accounts`: with `claudinho` saved, a plain terminal block
    `id`; `ws.see(vec![(id, Inspection { agent: Some("claude".into()), folder_var: Some(Some(folder)), ..Default::default() })], &agents::homes(&ws.agents))`
    → `pane.account == Some("claudinho")`, `ws.agent_name(pane) == Some("Claude Code (claudinho)")`,
    `ws.account_in_use("claudinho").is_some()`; then `folder_var: None` → unchanged; then `Some(None)` →
    `pane.account == None`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p templum account_of; cargo test -p templum-terminal folder_variable; cargo test -p templum typed_account`
Expected: FAIL to compile.

- [ ] **Step 3: Implement.** `inspect` reads environments in one more refresh, of the found agents' pids whose
  `Detect` has a `var` only (`ProcessesToUpdate::Some`, `ProcessRefreshKind::new().with_environ(UpdateKind::Always)`),
  never in the first refresh (on Windows that one covers every process). In `see`, before `in_block`: when the raw
  `seen.agent` is `Some` and `seen.folder_var` is `Some(var)`, `pane.account = var.and_then(|f| account_of(agent, &f, homes))`.

- [ ] **Step 4: Run them to verify they pass** (same commands). Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(accounts): a block is the account whose folder its agent runs on, typed by hand or not"`

---

### Task 5: `claudinho` in Templum's terminals gets Templum's server

**Files:**
- Modify: `crates/templum/src/agents.rs` (`accounts_env`)
- Modify: `crates/templum/src/workspace/usage.rs` (`agent_env`, ~135)
- Modify: `crates/templum-terminal/src/integration/templum.bash`, `zsh/.zshenv`, `templum.fish`, `templum.ps1` (after
  the agents' functions, in the same block)
- Test: `crates/templum-terminal/src/integration.rs` (the harness), `crates/templum/src/workspace/tests/usage.rs`
- Modify: `docs/specs/shell-integration.md` (one line: the account functions, pointing to accounts.md)

**Interfaces:**
- Consumes: Task 2's `agents::is_command_word`.
- Produces: `agents::accounts_env(agents: &[AgentEntry]) -> String`: a line per account, by id,
  `{id}\t{program}\t{var}\t{folder}` (`AgentEntry::program`, `AgentEntry::var`), skipping ids that aren't command
  words and folders holding `\t`, `\n` or `\r`; joined with `\n`. `agent_env` adds `("TEMPLUM_ACCOUNTS", accounts_env(&self.agents))`, always.

- [ ] **Step 1: Write the failing tests**
  - `agents.rs`: `each_account_is_a_line_of_templum_accounts`: agents from
    `"[agents]\nclaudinho = { of = \"claude\" }\nWork = { of = \"codex\" }\n"` on data `/data` →
    `"claudinho\tclaude\tCLAUDE_CONFIG_DIR\t/data/accounts/claudinho"` (`Work` left out; build the expected folder
    with `Path::join` so Windows passes).
  - `tests/usage.rs`: `a_new_terminal_gets_the_accounts`: with `claudinho` in the agents, `word(ws,
    "TEMPLUM_ACCOUNTS")` starts with `"claudinho\tclaude\t"`; with none, it is `""` (set, empty).
  - `integration.rs`: add `"TEMPLUM_ACCOUNTS"` to the variables `spawn_with` empties, `("claude",
    "CLAUDE_CONFIG_DIR")` to `AGENT_VARS` (its doc: the variables the fake agents record), and
    `("TEMPLUM_ACCOUNTS", ACCOUNTS)` to `with_agents` (`[(&str, &str); 10]`), with
    `const ACCOUNTS: &str = "claudinho\tclaude\tCLAUDE_CONFIG_DIR\t/tmp/Templum Data/accounts/claudinho";`. A helper
    `typed_account(term, home)`: types `claudinho hi` → `claude.args` is `CLAUDE_WORDS`' lines then `hi`, `claude.env`
    is `/tmp/Templum Data/accounts/claudinho`; then types `claude` → `claude.env` is `unset`. Call it from
    `bash_`, `zsh_`, `fish_` and `powershell_gives_typed_agents_templums_words`. In the bash test only, a second
    terminal whose `.bashrc` defines `claudinho() { command claude own; }` → `claude.args` is `own`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p templum templum_accounts; cargo test -p templum gets_the_accounts; cargo test -p templum-terminal gives_typed_agents`
Expected: the first two FAIL to compile; the shell tests FAIL on `claudinho: command not found` (each shell skips
itself when not installed: run where bash, zsh, fish and pwsh are).

- [ ] **Step 3: Implement** `accounts_env`, the `agent_env` line, and in each shell a loop over the lines of
  `TEMPLUM_ACCOUNTS` that defines a function per id unless one of that name exists (bash `declare -F`, zsh
  `$+functions`, fish `functions -q`, PowerShell `Test-Path Function:\<id>`). The function sets the variable to the
  folder for one command (bash/zsh `local -x`; fish `set -lx`; PowerShell sets `$env:` and restores the old value in
  `finally`) and runs the program by its name with the person's words, so Templum's `claude` function applies; it
  returns the agent's status as the agents' functions do. Build it with `eval` from the validated id only; the folder
  is read from the variable at call time or quoted by the shell's own quoting (`printf %q`, `${(q)}`,
  `string escape`, PowerShell's `'` doubled), never pasted raw.

- [ ] **Step 4: Run them to verify they pass** (same commands). Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(accounts): an account's command typed in a Templum terminal gets Templum's server"`

---

### Task 6: The new dialog and rows

**Files:**
- Modify: `crates/templum/src/settings/accounts.rs` (`accounts`, `account_row`, `Form`, `account_dialog`, `ask_delete`)
- Modify: `crates/templum/src/kanban.rs`: make `pill` `pub(crate)`; lift the Priority strip's segment (~1130-1175)
  into `pub(crate) fn segment(ui: Ui, id: impl Into<ElementId>, label: SharedString, on: bool) -> Stateful<Div>` and
  the Delete card button into `pub(crate) fn danger(id: impl Into<ElementId>, label: &'static str, red: Hsla) -> Stateful<Div>`,
  both used by the card dialog as before (its tests unchanged) and by the account dialog
- Test: `crates/templum/src/workspace/tests/settings_panel.rs` (the accounts' tests)

**Interfaces:**
- Consumes: Task 2's `name_problem`, `commands_dir`, `shell_path_dirs`, `on_path`; Task 3's `save_account(id, ..)`,
  `Workspace::commands`, `command_removed`, `delete_account`; `Workspace::close_settings`, `Workspace::log_in`.
- Produces: element ids `account-agent`, `account-name`, `account-setup-same`, `account-setup-fresh`,
  `account-setup-custom`, `account-share-all`, `account-share-{key}`, `account-save`, `delete-account`; the Form's
  `enum Setup { Same, Fresh, Custom }`.

- [ ] **Step 1: Write the failing tests** in `settings_panel.rs` (each sets `COMMANDS` and `OWN_FOLDERS` to tempdirs).
  A helper `add_account(handle, ws, cx, name)`: clicks `add-account`, sleeps 300 ms (the dialog slides in), clicks
  `account-name`, `cx.simulate_input(handle, name)`, renders a frame; and `create(handle, cx)` clicks `account-save`.
  - `creating_an_account_by_name_saves_it_and_logs_in` (replaces
    `adding_an_account_in_settings_writes_it_and_its_links`): `add_account("claudinho")`, `create` → after `ws.save()`
    config.toml's `claudinho` has `of = "claude"`, no `name`, `share` of every item to `"claude"`;
    `data/accounts/claudinho/skills` links to `own/claude/skills`; `commands/claudinho` exists; no dialog open; a
    terminal block with `account == Some("claudinho")` exists.
  - `a_broken_name_keeps_create_from_saving`: `add_account("Claude")`, `create` → the dialog is still open
    (`has_active_dialog`), no `[agents]` entry.
  - `fresh_shares_nothing_and_custom_shows_the_items`: `add_account("claudio")`, click `account-setup-custom` →
    `account-share-skills` can be clicked; click `account-setup-fresh`, `create` → `claudio`'s `share` is `None`.
  - `editing_an_older_account_keeps_its_name_and_folder`: config.toml
    `[agents.claude2]\nof = "claude"\nname = "Claude Code 2"\nfolder = '<tempdir>'\n`; click `edit-account-claude2`,
    `account-setup-fresh`, `account-save` → `name == Some("Claude Code 2")`, `folder` unchanged, `share == None`.
  - Update `an_accounts_row_logs_in_and_its_dialog_deletes_it`: after deleting, `commands/claude2` is gone.
  - Rewrite `a_chosen_folders_own_files_are_never_moved` to start from a config.toml account with that folder and save
    it from Edit with **Same**.
  - Delete `typing_a_folder_keeps_the_shares_chosen`, `an_account_folder_in_templums_folder_for_accounts_is_refused`
    (config.rs's `check_account` tests keep the refusal) and `a_new_accounts_id_differs_more_than_by_case` (Task 2's
    rule 3 test keeps it), and any `#[ignore]` Task 3 added.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p templum settings_panel`
Expected: FAIL (no `account-save`, `account-setup-*`).

- [ ] **Step 3: Implement** the dialog, the rows and the heading's PATH note exactly as the spec's
  "Settings ▸ Agents ▸ Accounts" says (sizes, copy, ids). Notes for the builder:
  - Form drops `folder`, `was`, `named`, `_folder_change`, `read_folder`, `choose`'s naming; it keeps `share`,
    `theirs`, `read_links` (on the account's own folder, for Custom) and `sources`.
  - The agent's default: `agent_choices(&agents)`'s `claude` when installed, else its first installed, else `claude`.
  - Setup from shares: every item `files_share()` allows shared with `kind.agent` → Same; empty → Fresh; else Custom.
    Same writes those items to `kind.agent`; Fresh clears; Custom keeps the rows' choices.
  - The name's line and the faint Create come from `name_problem` with the pending config's keys and
    `templum_terminal::locate`; Enter in the field (`InputEvent::PressEnter`) clicks Create.
  - Create: `save_account(name, ..)`; on `Ok`, `window.close_dialog(cx)`, `ws.close_settings(window, cx)`,
    `ws.log_in(name, window, cx)` (as the row's Log in does); on `Err`, a warning notification, the dialog open.
  - The PATH note shows when `commands_dir()` isn't `on_path` of `shell_path_dirs()`; its Unix and Windows copy is the
    spec's (`cfg!(windows)`), the line to add selectable.
  - `ask_delete` adds "Its command {id} is removed." when `command_removed(id)`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p templum settings_panel && cargo test -p templum kanban`
Expected: PASS. Then look at it: `cargo run -p templum -- --dev`, Settings ▸ Agents ▸ Add an account, in the dark and
a light theme; state in the commit body what was checked by eye (no keystroke injection: use the mouse yourself or
leave the visual check to the person).

- [ ] **Step 5: Commit** — `git commit -m "feat(accounts): a simpler Add an account: type a name, choose Same, Fresh or Custom, and it logs in"`

---

### Task 7: Copy, docs, full check, by hand

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs:748`: `"Which agent: claude, codex, gemini, … or an account's name (Settings ▸ Agents ▸ Accounts). Left out with fork."`
- Modify: `docs/guide.md:250-252` (an account is made by its name, `claudinho`, a command in any terminal);
  `docs/specs/agents.md:9` (`claude2` → `claudinho`); `docs/ARCHITECTURE.md:15` (`templum-usage`'s accounts also
  write each account's command, `accounts/command.rs`)
- Modify: `docs/minors.md` under **Agent accounts**: the five known limits of the spec's "Known limits" added by card
  65, each with where it is, what a person sees and a fix; and anything deferred in Tasks 1-6
- Modify: `docs/specs/accounts.md`: Status "planned" → "Implemented"

- [ ] **Step 1: Run the full check**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: PASS but for the two tests named in Global Constraints; report the output as it is.

- [ ] **Step 2: By hand (Hyprland)**, the spec's "Manual (card 65)": create `claudinho`, log in from the block Create
  opens; `claudinho` in a new Templum terminal (block titled "Claude Code (claudinho)", its status dot moving), in
  another terminal app, in fish; delete it and `claudinho` is gone. Write what was and wasn't checked in the commit body
  and in docs/minors.md (macOS, Windows not checkable here).

- [ ] **Step 3: Bench** (Tasks 4 and 5 touch `templum-terminal`): `cargo run --release -p templum -- --bench` on
  Linux; `p95_ms` must stay <= 16.7. Keep its JSON line for the pull request's description (CLAUDE.md, Performance).

- [ ] **Step 4: Commit** — `git commit -m "docs: accounts by name, their command and what is left for later"`
