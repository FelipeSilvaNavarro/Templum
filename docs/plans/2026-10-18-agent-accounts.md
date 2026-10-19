# Agent accounts: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A person adds accounts of an agent (`claude2`, `codex2`) in Settings ▸ Agents, each with its own folder whose
items are its own or links to another account's, and opens, spawns, assigns, resumes and meters each one as an agent
of its own (kanban card 37).

**Architecture:** An account is an `[agents.<id>]` entry with `of`; `agents::effective` turns it into an `AgentEntry`
whose `builtin` is its agent and whose `account` holds its folder and shares, so every list of agents has it. A new
UI-free crate, `templum-accounts`, knows each agent's variable, items and login, and applies an account's choices to
its folder (symbolic links; junctions for Windows folders). A terminal Templum opens for an account gets the variable
in its shell's environment and remembers the account (`Pane::account`, saved as `"account"`); the snapshot names the
account as the block's agent when it finds the account's agent there, and reads its session from the account's folder.

**Tech Stack:** Rust 2024; `std::os::unix::fs::symlink`, `std::os::windows::fs::symlink_file`, the `junction` crate
(Windows only); `serde`/`toml_edit` (config); headless GPUI tests (`crates/templum/src/workspace/tests/`).

**Spec:** [docs/specs/accounts.md](../specs/accounts.md). Read it whole first: the table of agents, what a change of
choice does to a file, the calls made and what is left out.

## Global Constraints

- An account is `[agents.<id>]` with `of = "<one of Templum's agents with accounts>"`; it takes only `of`, `name`,
  `show`, `folder`, `share`. `share` maps an item key to the agent's id or another account of the same agent; a missing
  key is own. Error copy: `"{FILE}: agents.{id}.{key}: an account runs its agent's command"` for `command`, `resume`,
  `generate`, `models`.
- New ids: `<agent id><n>`, the first free `n` from 2 (`claude2`). Default name in the dialog: `"<agent title> <n>"`;
  missing in the file: `"<agent title> (<id>)"`.
- Default folder: `<data>/accounts/<id>`. Kept aside: `<item>.own`, then `<item>.own-2`, `<item>.own-3`…
- Never touched: every name in the spec table's **Login** column, in every function of `templum-accounts`.
- Windows: folders by junction (`junction` crate, MIT); files by `symlink_file`, refused without the privilege with
  "Windows shares a file only with Developer Mode on (Settings ▸ System ▸ For developers). Folders share without it."
  Never `std::fs::hard_link`.
- Saved block field: `"account"` (string, skipped when none). The saved `"agent"` of an account's block running its
  agent is the account's id.
- The relay's added field: `"templum_config_dir"`.
- Only `crates/templum` uses gpui-kit; no `unsafe`; the one new dependency is `junction` under
  `[target.'cfg(windows)'.dependencies]` of `templum-accounts` (check `cargo deny check` accepts it).
- The specs already point at accounts.md ("planned") from agents.md, settings.md, usage.md, kanban.md, connections.md
  and persistence.md; a task changes them only where its code differs from accounts.md, and Task 12 drops "planned".
- Each commit passes `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`,
  on `development`, titled as CLAUDE.md says (`feat(accounts): …` for what a person uses). A task that changes a
  spec's behavior updates its lines in the same commit. No bench: nothing here paints or touches `templum-terminal`.
- Tests that make links run on Unix; the Windows ones are `#[cfg(windows)]` (CI runs them).

## Review Focus

1. **A link never deletes its target.** Own-again, delete-account and every replace remove the link only; a
   junction on Windows is removed with `std::fs::remove_dir`, never `remove_dir_all` on what it points at. Tasks 2 and
   11 pin it with a file inside the target that must survive.
2. **The login is never touched**, even when the person shares "everything" or picks a folder that already has a
   login. Task 2 puts a `.credentials.json` in both folders and checks its bytes after every operation.
3. **An account's agent typed again in its block after it quit** is still the account (the shell's variable), and the
   snapshot names it so. Task 5 pins it.
4. **Two blocks in one folder, one `claude`, one `claude2`**: each resumes its own session from its own folder after a
   restart. Task 6 pins it.
5. **A cycle written by hand** (`claude3.share.skills = "claude4"`, `claude4.share.skills = "claude3"`) is a config
   error, never a link loop. Task 3 pins it.

---

### Task 1: `templum-accounts`: each agent's folder, items and login

**Files:**
- Create: `crates/templum-accounts/Cargo.toml` (workspace package fields, `[lints] workspace = true`, `tempfile` as a
  dev-dependency as the other crates have it)
- Create: `crates/templum-accounts/src/lib.rs`, `crates/templum-accounts/src/kinds.rs`
- Modify: `docs/ARCHITECTURE.md` (crates table: the new row, "All eight crates")
- Test: `crates/templum-accounts/src/kinds.rs`

**Interfaces:**
- Produces:
  - `pub struct Item { pub key: &'static str, pub label: &'static str, pub paths: &'static [&'static str] }`:
    paths relative to the agent's state folder, a folder ending in `/` (`"projects/"`, `"history.jsonl"`); an item's
    paths are shared together.
  - `pub enum Keychain { Never, MacOs, Always }` (where the agent keeps its login outside its folder).
  - `pub struct Kind { pub agent: &'static str, pub var: &'static str, pub own: &'static str, pub state: Option<&'static str>, pub items: &'static [Item], pub login: &'static [&'static str], pub login_command: &'static str, pub login_hint: Option<&'static str>, pub overrides: &'static [&'static str], pub keychain: Keychain, pub note: Option<&'static str> }`:
    `own` is the variable's default under home (`".claude"`; Gemini CLI `""`, the home itself); `state` the subfolder
    the variable's folder holds (Gemini CLI `Some(".gemini")`, else None); `login` the never-touched names (a folder
    ending in `/`); `login_hint` what to type in the agent (`"/auth"`, `"/login"`); `overrides` the variables that
    win over a login; `note` the dialog's line. All values: the spec's [Which agents](../specs/accounts.md#which-agents).
  - `pub fn kinds() -> &'static [Kind]` (the spec table's rows, in its order) and `pub fn kind(agent: &str) -> Option<&'static Kind>`.
  - `impl Kind { pub fn own_folder(&self, var: Option<std::ffi::OsString>, home: Option<&Path>) -> Option<PathBuf>; pub fn state_dir(&self, folder: &Path) -> PathBuf; pub fn item(&self, key: &str) -> Option<&'static Item> }`:
    `own_folder` is the variable's value when non-empty, else `home/own`; `state_dir` is `folder` or `folder/state`.

- [ ] **Step 1: Write the failing tests** in `kinds.rs`:
  `no_item_is_a_login`: for every kind, no item path (trailing `/` trimmed) equals a `login` name (trimmed), lies
  under one, or holds one. `keys_are_unique_and_named`: keys unique per kind, each `label` and `paths` non-empty,
  `var` non-empty, no path absolute or with `..`. `the_agents_are_the_specs`: `kinds()` ids are exactly `claude`,
  `codex`, `gemini`, `copilot`, `pi`, `grok`, `kimi`, `crush`; `kind("claude").unwrap().var == "CLAUDE_CONFIG_DIR"`,
  its `login` contains `".credentials.json"` and `".claude.json"`, its `sessions` item's paths are `["projects/",
  "file-history/", "plans/"]`; `kind("codex").unwrap().login` contains `"auth.json"` and no codex path ends in
  `.sqlite`; `kind("gemini").unwrap().item("settings") == None`; `kind("crush").unwrap().items.is_empty()`;
  `kind("opencode") == None`. `own_folder_follows_the_variable`: Claude Code with `Some("/x")` gives `/x`; `Some("")`
  and `None` give `<home>/.claude`; no home and no variable give `None`; Gemini CLI with none gives `<home>`.
  `state_dir_is_the_folder_or_its_subfolder` (Gemini CLI: `<folder>/.gemini`).
- [ ] **Step 2:** `cargo test -p templum-accounts`: fails to compile (no crate items).
- [ ] **Step 3:** Write `kinds.rs` from the spec's table, row by row (keys, paths, labels, login names, login
  commands, notes exactly as the spec gives them); `lib.rs` re-exports `kinds::*` and documents the crate in one
  paragraph pointing at `docs/specs/accounts.md`.
- [ ] **Step 4:** `cargo test -p templum-accounts`: PASS.
- [ ] **Step 5: Commit** `feat(accounts): know where each agent keeps its settings, sessions and login`

### Task 2: Applying an account's choices to its folder

**Files:**
- Create: `crates/templum-accounts/src/folder.rs`
- Modify: `crates/templum-accounts/Cargo.toml` (`junction` for Windows), `src/lib.rs`
- Test: `crates/templum-accounts/src/folder.rs`

**Interfaces:**
- Consumes: Task 1's `Kind`, `Item`.
- Produces:
  - `pub enum When { Saved, Starting }`
  - `pub struct Share { pub key: &'static str, pub from: Option<PathBuf> }`: `from` is the source's **state dir**
    (another account's `state_dir`, or the agent's own folder's); None: own.
  - `#[derive(Default)] pub struct Report { pub problems: Vec<String>, pub replaced: Vec<&'static str>, pub theirs: Vec<(&'static str, PathBuf)> }`
    (`theirs`: items holding a link Templum didn't make, and where it points).
  - `pub fn apply(kind: &Kind, folder: &Path, shares: &[Share], sources: &[PathBuf], when: When) -> Report`:
    `sources` are every state dir a share of this agent may point at (its own folder's and each account's); a link is
    Templum's when its target is `<one of sources>/<the same path>`, and only those are ever replaced or removed.
  - `pub enum Found { Own, Linked(PathBuf /* the source state dir */), Theirs(PathBuf /* the link's target */) }` and
    `pub fn read(kind: &Kind, folder: &Path, sources: &[PathBuf]) -> Vec<(&'static str, Found)>`: what a folder
    already holds, item by item (an item's first path decides), for the dialog.
  - `pub fn delete(kind: &Kind, folder: &Path, shares: &[Share]) -> std::io::Result<()>`
  - `pub fn files_share() -> bool` (Unix: true; Windows: whether a file symbolic link can be made, tried once in the
    temporary folder and cached in a `OnceLock`).

`apply` does, for each path of each item, exactly the spec's
[Making and changing it](../specs/accounts.md#making-and-changing-it) list: makes `state_dir(folder)` (0700 on Unix)
and a missing source folder (a path ending in `/`); links a shared path when it is free (a missing source *file* is
skipped silently); re-points a link; on `Saved` renames a real one to the first free `<path>.own[-n]` then links, on
`Starting` leaves it and pushes the item's key to `replaced` (once per item); own: removes a link and renames
`<path>.own` back when present. A link is "ours to replace" when `symlink_metadata` says symlink (or a junction on
Windows: `junction::exists`) and its target is Templum's (above); any other link is left and listed in `theirs`.
Ours are removed with `remove_file` (Unix) or `std::fs::remove_dir` (a Windows junction). Every
error becomes `problems`: `"{label}: {error}"`. It never opens a `login` name. `delete` removes each item path that is
a link (only the link), then `remove_dir_all(folder)`.

- [ ] **Step 1: Write the failing tests** (Unix, `tempfile::tempdir`, a fake `Kind` with a folder item `skills`, a
  file item `settings.json` and login `[".credentials.json"]`; both folders get a `.credentials.json` whose bytes are
  checked after each step):
  - `a_shared_folder_is_a_link_to_the_sources`: after `apply(Saved)`, `skills` is a symlink to `<src>/skills`, which
    exists (made empty).
  - `a_missing_source_file_is_linked_later`: `settings.json` absent in source → no link, no problem; created in the
    source then `apply(Starting)` → linked.
  - `a_real_item_is_kept_aside_on_save_and_left_at_a_start`: a real `skills/a` in the account → `Saved`: `skills.own/a`
    exists and `skills` is a link; with `skills.own` taken, the next goes to `skills.own-2`; a real `settings.json` at
    `Starting` stays a file and `report.replaced == ["settings"]`.
  - `own_again_removes_only_the_link`: shared then own → `skills` gone, `<src>/skills/keep` still there; with
    `skills.own` present it is renamed back to `skills`.
  - `a_chain_links_to_the_middle_account`: c4 shares from c3's state dir, c3 from c's → `c4/skills` reads
    `c3/skills` (`read_link`), and a file made in `c/skills` is seen through `c4/skills`.
  - `delete_leaves_what_links_point_at`: `delete` → the account folder gone, `<src>/skills/keep` and the source's
    `.credentials.json` intact.
  - `a_link_templum_didnt_make_is_left`: `skills` linked by hand to `<dotfiles>/skills` → with the item shared and
    with it own, `apply` leaves the link pointing there and `report.theirs == [("skills", <dotfiles>/skills)]`.
  - `read_finds_a_folder_set_up_by_hand`: links to `<src>/skills` and `<dotfiles>/agents`, a real `commands/` →
    `read` gives `skills: Linked(<src>)`, `agents: Theirs(<dotfiles>/agents)`, `commands: Own`.
  - `#[cfg(windows)] a_folder_is_a_junction_and_a_file_needs_the_privilege`: `skills` is a junction; with
    `files_share() == false`, `settings.json` gives the Global Constraints' copy in `problems`.
- [ ] **Step 2:** `cargo test -p templum-accounts folder`: FAIL (no `apply`).
- [ ] **Step 3:** Implement `folder.rs` as above; `junction = "2"` under `[target.'cfg(windows)'.dependencies]`.
- [ ] **Step 4:** `cargo test -p templum-accounts` and `cargo deny check`: PASS.
- [ ] **Step 5: Commit** `feat(accounts): share an account's items with another folder by links, never its login`

### Task 3: `of`, `folder` and `share` in config.toml

**Files:**
- Modify: `crates/templum/Cargo.toml` (`templum-accounts` path dependency)
- Modify: `crates/templum/src/config.rs` (`AgentConfig`, the `[agents]` checks at `:472-500`)
- Test: `crates/templum/src/config.rs` (tests module, beside `agents_change_ours_and_add_the_users`)

**Interfaces:**
- Produces: `AgentConfig { …, pub of: Option<String>, pub folder: Option<String>, pub share: Option<BTreeMap<String, String>> }`.
  The checks live in one `pub(crate) fn check_account(id: &str, agents: &BTreeMap<String, AgentConfig>) -> Result<(), String>`,
  which `[agents]`' validation and Task 10's dialog both call.

Rules (each an error naming `agents.{id}…`, as the others): `of` not a `templum_accounts::kind` → "`of` is an agent
with accounts: claude, codex, …"; an account with `command`, `resume`, `generate` or `models` (copy above); a `share`
key not one of its kind's items; a `share` value neither `of` nor an account whose `of` is the same; following one
key's targets from an account comes back to it ("agents.claude3.share.skills: claude4 shares it back"); `folder`
equal (after `~` expansion) to another account's. `name`/`command` aren't needed when `of` is set.

- [ ] **Step 1: Write the failing tests:** `accounts_parse_with_their_shares` (the spec's three entries parse; `of`,
  `share["skills"] == "claude"`); `an_account_is_refused_what_it_inherits` (`command` → error containing
  `agents.claude2.command`); `an_accounts_shares_are_checked` (unknown key `nope`, a target `codex`, a target
  `codex2` with `of = "codex"`, and the cycle each give an error naming the key); `two_accounts_cant_share_a_folder`;
  `of_must_have_accounts` (`of = "aider"`).
- [ ] **Step 2:** `cargo test -p templum config::tests::account`: FAIL.
- [ ] **Step 3:** Implement the fields and checks.
- [ ] **Step 4:** `cargo test -p templum config`: PASS.
- [ ] **Step 5: Commit** `feat(accounts): accounts of an agent in config.toml`

### Task 4: Accounts among the agents in effect

**Files:**
- Modify: `crates/templum/src/agents.rs` (`AgentEntry`, `effective`, new `Account`, `new_account_id`, `homes`)
- Modify: `crates/templum/src/workspace/mod.rs:1216` and `crates/templum/src/workspace/preferences.rs:232`
  (`effective` gets the data folder)
- Modify: `crates/templum/src/workspace/persist.rs:334` (detection leaves accounts out)
- Modify: `crates/templum/src/launcher.rs:77-80`, `crates/templum/src/workspace/blocks.rs:317-338` (`launch_agent`
  by id: two entries share a command now)
- Test: `crates/templum/src/agents.rs`; the headless tests calling `launch_agent` (update their argument)

**Interfaces:**
- Consumes: Task 1's `kind`, `Kind::own_folder`, `state_dir`; Task 2's `Share`; Task 3's fields.
- Produces:
  - `pub(crate) struct Account { pub kind: &'static templum_accounts::Kind, pub folder: PathBuf, pub chosen: bool, pub share: BTreeMap<String, String> }`
    (`chosen`: the person picked the folder; false: `<data>/accounts/<id>`).
  - `AgentEntry { …, pub account: Option<Account> }`; an account's entry has `builtin: Some(its agent)`,
    `install: Some(its agent's)`, `changed: false`, and its agent's effective `command`, `resume`, `generate`,
    `models`; `title` = `name` or `"<agent title> (<id>)"`. Accounts come last, by id; none without a data folder.
  - `pub(crate) fn effective(config: &BTreeMap<String, AgentConfig>, codex: &[String], data: Option<&Path>) -> Vec<AgentEntry>`
  - `impl AgentEntry { pub fn var(&self) -> Option<(&'static str, PathBuf)>; pub fn shares(&self, agents: &[AgentEntry]) -> Vec<templum_accounts::Share>; pub fn sources(&self, agents: &[AgentEntry]) -> Vec<PathBuf> }`:
    `var` is `(kind.var, folder)`; `shares` resolves each item to its source's state dir (the agent's own folder from
    `Kind::own_folder(std::env::var_os(var), home)`, or the target account's `state_dir(folder)`), `from: None` for own;
    `sources` is the agent's own folder's state dir and every account of that agent's, this one's left out.
  - `pub(crate) fn new_account_id(of: &str, taken: impl Fn(&str) -> bool) -> String`
  - `pub(crate) struct Home { pub id: String, pub agent: String, pub state: PathBuf }` and
    `pub(crate) fn homes(agents: &[AgentEntry]) -> Vec<Home>` (one per account: its id, its agent's id, its state dir).
  - `pub(crate) fn launch_agent(&mut self, agent: &str /* an entry's id */, folder: &Path, at: Point, window, cx) -> Option<BlockId>`

- [ ] **Step 1: Write the failing tests** in `agents.rs`: `an_account_runs_its_agents_command` (config with
  `claude = { command = "claude --model opus" }` and `claude2 = { of = "claude" }` → claude2's command is
  `claude --model opus`, `builtin == Some(Agent::CLAUDE)`, title `"Claude Code (claude2)"`, last in the list, folder
  `<data>/accounts/claude2`; with `data == None` no claude2); `its_variable_names_its_folder` (`var()` ==
  `("CLAUDE_CONFIG_DIR", <folder>)`, `folder = "~/w"` expands, `folder = "~/w/"` gives the same text without the
  trailing separator: the macOS Keychain names a login by that exact text); `its_shares_point_at_their_sources` (`skills =
  "claude"` → the agent's own folder's state dir; `skills = "claude3"` → claude3's; unset → `from: None`);
  `account_ids_count_from_two` (`claude2`, then `claude3` when taken); `accounts_arent_looked_for_in_terminals`
  (the `Detect` list built as `persist.rs:334` builds it has no `claude2`).
- [ ] **Step 2:** `cargo test -p templum agents::tests`: FAIL.
- [ ] **Step 3:** Implement; `launch_agent` finds by `a.id == agent`; `launcher::ask_folder` passes `agent.id`.
- [ ] **Step 4:** `cargo test -p templum`: PASS (the menu tests opening agents still pass with ids).
- [ ] **Step 5: Commit** `feat(accounts): an account is one more agent, on its own folder`

### Task 5: Opening an account: its variable, its folder, its block

**Files:**
- Modify: `crates/templum/src/workspace/usage.rs:170-197` (`agent_vars` adds the account's variable)
- Modify: `crates/templum/src/workspace/blocks.rs:347-375` (`start_agent`: `Pane::account`, apply at the start)
- Modify: `crates/templum/src/workspace/mod.rs` (`Pane::account`; `see()` at `:793` names the account)
- Modify: `crates/templum/src/agents.rs` (`in_block`)
- Modify: `crates/templum-usage/src/lib.rs` (`Proc::home`, `Proc::account`), `crates/templum/src/workspace/usage.rs:55-63`
  (`proc_of` gives a process its agent's id, not the account's, and the account's folder)
- Test: `crates/templum/src/agents.rs`, `crates/templum/src/workspace/tests/agents.rs` (opening agents)

**Interfaces:**
- Consumes: Task 4's `AgentEntry::var`, `shares`, `homes`, `Home`; Task 2's `apply`, `When::Starting`, `Report`.
- Produces:
  - `Pane { …, pub account: Option<String> }` (the account id whose variable its shell has).
  - `pub(crate) fn in_block(found: Option<&str>, account: Option<&Home>) -> Option<String>`: the account's id when
    `found == Some(account.agent)`, else `found`.
  - `see()` replaces each inspection's `agent` by `in_block(found, <the pane's account's Home>)` before any of its
    checks (`flags_for`, `left`), so the pane's agent is the account's id from then on.
  - `Workspace::tell_account(&self, entry: &AgentEntry, report: &Report, window, cx)`: one warning notification
    listing `problems`, and one per `replaced` key with the spec's copy ("Claude Code 2's settings.json isn't shared
    any more: Claude Code wrote its own. Edit the account to share it again.").
  - `Workspace::prepare_account(&mut self, entry: &AgentEntry, window, cx)`: `apply(…, entry.sources(&self.agents), When::Starting)` and
    `tell_account`; nothing for an entry without `account`. Every start of an account calls it (here `start_agent`;
    Task 6 the restore; Task 11 Log in).
  - `templum_usage::Proc { …, pub home: Option<PathBuf>, pub account: Option<String> }`: the account's state dir and
    id, None for the agent's own folder; `agent` stays the agent's id (`claude`). `Workspace::proc_of(&self, pane)`
    (was the free `proc_of`) fills them from the pane's agent entry. The readers use `home` from Task 6 on.

`agent_vars(entry, set)` puts `entry.var()` first, whatever `set` says (the account's folder must win over Templum's
environment). `start_agent` calls `prepare_account` before spawning, then sets `pane.account`. The menu's cover title
is the account's name.

- [ ] **Step 1: Write the failing tests:** `in_block_names_the_account` (unit: `claude` found in a claude2 block →
  `claude2`; `codex` found there → `codex`; no account → as found); headless
  `an_account_opens_with_its_folder_in_its_shell` (config `claude2 = { of = "claude" }` with a temp data folder:
  `launch_agent("claude2", …)` → the new terminal's spawn environment has `CLAUDE_CONFIG_DIR=<data>/accounts/claude2`
  (read it as the existing OpenCode test reads `OPENCODE_CONFIG_CONTENT`, `tests/bus.rs:617`), `pane.account ==
  Some("claude2")`, the folder exists); headless `the_snapshot_names_an_accounts_agent` (feed `see()` an inspection
  with agent `claude` for a block whose `account` is claude2 → `pane.agent() == Some("claude2")`, and its
  `flags_for` of `claude2` keeps the model); `claude_typed_again_in_its_block_is_the_account` (the block's agent
  quits — an inspection with no agent — then `claude` is found again → `claude2`).
- [ ] **Step 2:** run them: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p templum`: PASS.
- [ ] **Step 5: Commit** `feat(accounts): an account opens on its own folder and names its block`

### Task 6: An account's block across restarts

**Files:**
- Modify: `crates/templum/src/session.rs` (`TerminalItem::account`; `session_ids` gives each `Proc` its home)
- Modify: `crates/templum/src/workspace/mod.rs:262-272` (`Pane::item` keeps `account`), `:680-700` (`layout`
  passes `accounts`)
- Modify: `crates/templum/src/workspace/persist.rs:159-235` (`add_saved` spawns with the account's variable even out
  of its folder; `restore_terminal` sets `pane.account`, reads `session_kept` in its folder)
- Modify: `crates/templum-usage/src/lib.rs` (`session_ids`, `session_kept` read `Proc::home`)
- Test: `crates/templum/src/session.rs`, `crates/templum-usage/src/lib.rs`, `crates/templum/src/workspace/tests/persistence.rs`

**Interfaces:**
- Consumes: Task 5's `Home`, `in_block`, `Pane::account`.
- Produces:
  - `Layout { …, pub accounts: Arc<Vec<agents::Home>> }`: `Layout::snapshot` gives each terminal's processes their
    account's `home` (when `in_block` names the account) for `session_ids`, and saves `in_block`'s agent.
  - `TerminalItem { …, #[serde(default, skip_serializing_if = "Option::is_none")] pub account: Option<String> }`
  - `session_ids` and `claude::session_id` read `proc.home` before `dirs.claude`/`dirs.codex`.
  - `pub fn session_kept(agent: &str, id: &str, dirs: &Dirs, home: Option<&Path>) -> bool`
  - `add_saved` calls Task 5's `prepare_account` for an item whose `account` is a known account, before spawning;
    an `account` Templum doesn't know (deleted, or hand-edited) gives no variable and `pane.account = None`.

- [ ] **Step 1: Write the failing tests:** `session.rs`: `an_items_account_is_kept` (JSON round trip with
  `"account":"claude2"`; a file without it loads with `None`); `templum-usage`: `a_session_id_is_read_from_the_accounts_folder`
  (`sessions/<pid>.json` only under the account's state dir; `Proc { home: Some(dir), .. }` finds it, the default
  doesn't); headless `persistence.rs`: `two_blocks_one_folder_two_accounts_resume_their_own` (saved items `claude`
  with session `a` and `claude2` with session `b` and `account: claude2` → the first types `claude --resume a` with no
  `CLAUDE_CONFIG_DIR` in its environment, the second `claude --resume b` with it); `an_account_block_keeps_its_folder_without_its_agent`
  (an item with `account: claude2`, no agent, folder gone → the shell still gets the variable and `pane.account`);
  `a_deleted_accounts_block_comes_back_plain` (an item with `account: gone` → no variable, `pane.account == None`).
- [ ] **Step 2:** run them: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`: PASS.
- [ ] **Step 5: Commit** `feat(accounts): an account's block comes back on its account and its own session`

### Task 7: Limits and usage per account

**Files:**
- Modify: `crates/templum-usage/src/claude.rs` (`relay` writes `templum_config_dir`; `limits` per folder; `poll`
  gets the folder), `crates/templum-usage/src/codex.rs` (`limits_of`), `crates/templum-usage/src/lib.rs` (`read`)
- Modify: `crates/templum/src/main.rs:170-185` (the relay's folder from its environment, as today)
- Modify: `crates/templum/src/workspace/usage.rs` (`claude_poll_job` → jobs, `claude_polled` per id, `usage_done`
  keys), `crates/templum/src/workspace/mod.rs:1278-1300` (the poll loop runs each job)
- Modify: `crates/templum/src/settings/sections.rs:282-300` (a title-bar switch per account with limits)
- Test: `crates/templum-usage/src/claude.rs`, `codex.rs`, `crates/templum/src/workspace/tests/usage.rs`

**Interfaces:**
- Consumes: Task 5's `Proc::home`, `Proc::account`; Task 4's `homes`; `sessions_shared` is whether any account's
  `share` names that folder for `sessions`, or this account shares its own.
- Produces:
  - `pub fn relay(input: &[u8], relay_dir: &Path, terminal: Option<&str>, claude_dir: Option<&Path>, config_dir: Option<&Path>) -> Vec<u8>`:
    the file written is `input` with `"templum_config_dir": "<config_dir>"` added at its top level when `config_dir`
    is set and `input` is a JSON object; byte for byte `input` otherwise.
  - `pub fn claude::limits(relay_dir: &Path, config_dir: Option<&Path>) -> Option<Limits>`: the newest relay file
    whose `templum_config_dir` is `config_dir` (None: files without it).
  - `pub fn claude::poll(program: &Path, dir: &Path, config_dir: Option<&Path>, now: u64) -> Option<Limits>`
    (`CLAUDE_CONFIG_DIR` set on the command when `Some`).
  - `pub fn codex::limits_of(files: &[PathBuf]) -> Option<Limits>`: the newest (by modified time) of these rollouts'
    limits, through the private `limits_in` that `limits` uses (`codex.rs:93`).
  - `pub struct AgentHome { pub id: String, pub agent: String, pub dir: PathBuf, pub sessions_shared: bool }` in
    `templum-usage`, and `pub fn read(procs: &[Proc], dirs: &Dirs, homes: &[AgentHome], now: u64) -> (Vec<Option<Session>>, Vec<(String, Limits)>)`:
    limits keyed by account id (the agent's id for its own folder); a Codex home with `sessions_shared` takes
    `limits_of` its running processes' rollout files only. `homes` includes the agents' own folders.
  - `Workspace::claude_poll_jobs(&self) -> Vec<(String /* id */, PathBuf, PathBuf, Option<PathBuf>)>` (id, program,
    probe folder, config dir): the agent's own, and each Claude Code account seen running since start
    (`Workspace::accounts_ran: HashSet<String>`, filled by `see()`).

- [ ] **Step 1: Write the failing tests:** `the_relay_names_its_folder` (`config_dir = Some(/a)` → the written file
  parses with `templum_config_dir == "/a"` and every input field; `None` → identical bytes); `limits_are_the_folders_own`
  (two relay files, one per folder, each `limits(…, folder)` gets its own, and a third folder's file counts for neither
  nor for the agent's own); `shared_codex_sessions_give_each_its_processes_limits`
  (two rollouts in one `sessions/`, the newer codex2's: codex's limits are its own process's file's); headless
  `each_account_has_its_row_in_the_meter` (limits for `claude` and `claude2` → two entries titled "Claude Code" and
  the account's name; `usage.hide = ["claude2"]` hides only it); Settings ▸ Usage shows "Claude Code 2 in the title bar".
- [ ] **Step 2:** run them: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`: PASS.
- [ ] **Step 5: Commit** `feat(accounts): plan limits and their notifications per account`

### Task 8: Cards and `spawn_agent` for an account

**Files:**
- Modify: `crates/templum/src/workspace/content.rs:127-140` (`runs`)
- Modify: `crates/templum-mcp/src/tools.rs:465` (the `agent` description)
- Test: `crates/templum/src/workspace/tests/content.rs` (the kanban tools), `crates/templum/src/workspace/tests/spawn.rs`

**Interfaces:**
- Consumes: Task 4's `AgentEntry::account`.
- Produces: `runs(pane_agent, agent)`: an account `agent` matches only `pane_agent == agent`; otherwise as today (so a
  claude2 block takes a card for `claude` by program). Description: `"Which agent: claude, codex, gemini, … or an
  account, like claude2 (an id of Settings ▸ Agents)."`

- [ ] **Step 1: Write the failing tests:** `a_card_for_an_account_is_its_own` (`claim_card` of a card for
  `agent:claude2` from a `claude` block is refused, from a claude2 block taken; a card for `agent:claude` is taken by
  the claude2 block); `spawn_agent_opens_an_account` (`spawn_agent` with `agent: "claude2"`, `model: "opus"` → a block
  whose agent is claude2, `CLAUDE_CONFIG_DIR` in its environment, `--model opus` typed).
- [ ] **Step 2:** run them: FAIL (the first).
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`: PASS.
- [ ] **Step 5: Commit** `feat(accounts): a card for an account, and spawn_agent on one`

### Task 9: Generate ✨ and Gemini CLI's extension on an account's folder

**Files:**
- Modify: `crates/templum/src/agents.rs` (`CommitAgent::env`), `crates/templum-git/src/message.rs:250-262` (the
  command gets it), the caller in `crates/templum/src/workspace/git_message.rs`
- Modify: `crates/templum/src/workspace/usage.rs:454-560` (`tell_gemini` checks the account's state dir;
  `setup_gemini` and `gemini_run` take a variable)
- Test: `crates/templum/src/agents.rs`, `crates/templum-git/src/message.rs`, `crates/templum/src/workspace/tests/usage.rs`

**Interfaces:**
- Consumes: Task 4's `AgentEntry::var`.
- Produces: `CommitAgent { …, pub env: Option<(String, String)> }` (the account's variable); templum-git's run
  function gets `env: &[(String, String)]` set on its `Command`; `Workspace::setup_gemini(&mut self, install: bool, account: Option<&str>, cx)`
  and `gemini_run(program, data, install, env: Option<(String, String)>)`.

- [ ] **Step 1: Write the failing tests:** `generate_runs_on_the_accounts_folder` (`commit_agent` with agent
  `claude2` → `env == Some(("CLAUDE_CONFIG_DIR", folder))`); in `message.rs`, a fake generate command that prints
  `$CLAUDE_CONFIG_DIR` returns the folder; headless `a_gemini_account_without_the_extension_is_told` (a Gemini account
  whose `extensions` is own and empty → the notice; one sharing `extensions` with a folder that has `templum` → none).
- [ ] **Step 2:** run them: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`: PASS.
- [ ] **Step 5: Commit** `feat(accounts): Generate and Gemini CLI's extension use the account's folder`

### Task 10: Settings ▸ Agents ▸ Accounts: the rows and the dialog

**Files:**
- Create: `crates/templum/src/settings/accounts.rs` (rows, "Add an account", the dialog)
- Modify: `crates/templum/src/settings/mod.rs:37,310` (the Accounts heading and button under Add an agent),
  `crates/templum/src/settings/sections.rs:244-248` and `crates/templum/src/settings/welcome.rs:94-99` (agents'
  rows leave accounts out)
- Modify: `crates/templum/src/agents.rs` (`AccountDraft`), `crates/templum/src/workspace/preferences.rs`
  (`save_account`)
- Test: `crates/templum/src/agents.rs`, `crates/templum/src/workspace/tests/settings_panel.rs`

**Interfaces:**
- Consumes: Tasks 2–4 (`apply`, `When::Saved`, `files_share`, `effective`, `new_account_id`, `shares`).
- Produces:
  - `pub(crate) struct AccountDraft { pub of: String, pub name: String, pub show: bool, pub folder: Option<String>, pub share: BTreeMap<String, String> }`
    with `fn of(entry: &AgentEntry) -> AccountDraft`, `fn to_config(&self) -> AgentConfig` and
    `fn problem(&self, id: &str, agents: &BTreeMap<String, AgentConfig>) -> Option<String>` (Task 3's
    `check_account` on the agents with this entry in them).
  - `Workspace::save_account(&mut self, id: Option<&str>, draft: &AccountDraft, window, cx) -> Result<String, String>`:
    the edit (`Edit::set("agents", id, agents::to_value(&config))`, `to_value` writing `of`, `folder`, `share` as an
    inline table too), then `apply(…, When::Saved)` on the entry the draft makes, `tell_account` on its report.

The dialog follows the spec's [The dialog](../specs/accounts.md#the-dialog) order and copy, and
[Changing its folder](../specs/accounts.md#changing-its-folder): when a folder is chosen (or an account is edited),
its items' dropdowns start from `templum_accounts::read` (`Linked` → that source's id, `Own`, `Theirs` → the "Linked by
you to …" line); `save_account` refuses a new folder while `delete_account` would refuse (Task 11's checks, which this
task writes as `Workspace::account_in_use(&self, id: &str) -> Option<String>`, the refusal's copy). Its dropdowns are
a button opening a `PopupMenu`, as the open card's Assignee row does (`crates/templum/src/kanban.rs:805`); **Choose…**
uses `launcher::pick_folder`.

- [ ] **Step 1: Write the failing tests:** `an_account_draft_round_trips` (`to_config` then `to_value` then
  `config::parse` gives the same entry, `share` included); `a_draft_with_a_cycle_is_refused`; headless
  `adding_an_account_in_settings_writes_it_and_its_links` (click Add an account, choose Claude Code, "Share everything
  with" Claude Code, Save → `[agents.claude2]` with `of` and every key `= "claude"`, its folder has `skills` linked to
  the agent's own folder (a temp `CLAUDE_CONFIG_DIR` in the test)); `the_agents_rows_leave_accounts_out` (no
  `agents.claude2` row among the agents, one `accounts.claude2` row under Accounts); `reset_all_leaves_accounts`
  (Settings ▸ Agents' Reset all with `claude` changed and `claude2` present: `claude` reset, `claude2` kept);
  `a_folder_set_up_by_hand_keeps_its_sharing` (choose a folder whose `skills` links to the agent's own and whose
  `agents` links to a dotfiles folder → the dialog's `skills` is "Claude Code: its own folder", `agents` shows "Linked
  by you to …", and Save leaves both links); `a_folder_change_waits_for_the_blocks_and_keeps_the_old_one` (an open
  claude2 block → refused; closed → the new folder has the links, the old one still exists).
- [ ] **Step 2:** run them: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p templum`: PASS. Then by hand (`cargo run -p templum -- --dev`): add, edit a share,
  see the link in the folder.
- [ ] **Step 5: Commit** `feat(accounts): add and edit an agent's accounts in Settings ▸ Agents`

### Task 11: Log in and Delete

**Files:**
- Modify: `crates/templum/src/settings/accounts.rs` (the row's Log in, the dialog's Delete and its question)
- Modify: `crates/templum/src/workspace/blocks.rs` (`log_in`), `crates/templum/src/workspace/preferences.rs`
  (`delete_account`)
- Test: `crates/templum/src/workspace/tests/settings_panel.rs`

**Interfaces:**
- Consumes: Task 5's `agent_vars`, `Pane::account`, `prepare_account`; Task 2's `delete`; Task 10's `account_in_use`.
- Produces:
  - `Workspace::log_in(&mut self, id: &str, window, cx) -> Option<BlockId>`: a terminal at the view's center (the
    rule of `blocks.rs:858`), in the home folder, after `prepare_account`, with the account's variable and `pane.account`, its kind's
    `login_command` typed with Enter; with a `login_hint`, a notification "Type /auth in Gemini CLI to log in."
    (the hint and the account's agent title).
  - `Workspace::delete_account(&mut self, id: &str, cx) -> Result<(), String>`: `Err(account_in_use(id))` (Task 10:
    a block of a running workspace whose `pane.account` is it, with or without its agent, or another account's
    `share` naming it); else the edit removing the entry and, when `!account.chosen`, `templum_accounts::delete`.

- [ ] **Step 1: Write the failing tests:** `log_in_opens_a_block_of_the_account` (the block's environment has the
  variable, `claude auth login` — the kind's command — typed, `pane.account` set); `deleting_an_account_removes_its_folder_only`
  (a temp source with `skills/keep` → after delete the account's folder is gone, `skills/keep` stays, `[agents]` has no
  claude2); `an_account_in_use_or_shared_isnt_deleted` (a block running it, a block of it with only its shell, claude4
  sharing from it: each `Err` with its copy); `a_chosen_folder_stays` (`folder` set → the folder is still there).
- [ ] **Step 2:** run them: FAIL.
- [ ] **Step 3:** Implement; the question uses `crate::prompt::confirm` with the spec's lines (the keychain line for
  `Keychain::Always`, and for `Keychain::MacOs` under `cfg!(target_os = "macos")`).
- [ ] **Step 4:** `cargo test -p templum`: PASS. By hand: Log in to a second Claude Code account, open it from the
  menu, check `/status` names the other account.
- [ ] **Step 5: Commit** `feat(accounts): log in to an account, and delete it with its folder`

### Task 12: Docs and the minors

**Files:**
- Modify: `docs/specs/accounts.md` (Status: Implemented), `docs/specs/agents.md` (Status line),
  `docs/ARCHITECTURE.md` (the `Pane::account` sentence in the persistence paragraph), `docs/minors.md` (a new area
  **Agent accounts** with the spec's [Known limits](../specs/accounts.md#known-limits), each with its file and line)
- Modify, only if Step 1 says so: `crates/templum-accounts/src/kinds.rs`, `docs/specs/accounts.md` (Grok Build's items)
- Modify: the planned-status pointers added to `docs/specs/agents.md`, `settings.md`, `usage.md`, `kanban.md`,
  `connections.md`, `persistence.md` ("planned" removed)

- [ ] **Step 1:** By hand, with Grok Build installed: a `grok2` sharing `skills` with `grok` lists a skill of
  `~/.grok/skills` (`/skills`). If it doesn't, take Grok Build's items out of `kinds.rs` and of the spec's list (its
  account shares nothing, as Crush's) in this commit, with the test of Task 1 updated.
- [ ] **Step 2:** Write the minors' entries: where (`file:line`), what a person sees, a way to fix, as the other areas do.
- [ ] **Step 3:** `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`: PASS.
- [ ] **Step 4: Commit** `docs: agent accounts are implemented, and their known limits`
