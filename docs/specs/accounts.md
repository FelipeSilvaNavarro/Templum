# Agent accounts

- **Date:** 2026-10-18
- **Status:** Implemented (kanban card 37, "Múltiplas contas do claude"):
  [docs/plans/2026-10-18-agent-accounts.md](../plans/2026-10-18-agent-accounts.md)
- **Builds on:** [agents.md](agents.md) (the agents, Settings ▸ Agents, `[agents]`), [settings.md](settings.md),
  [usage.md](usage.md) (limits are an account's), [connections.md](connections.md) (`spawn_agent`, what is typed into
  an agent, agents typed in a terminal), [persistence.md](persistence.md#agents) and
  [kanban.md](kanban.md#who-a-card-is-for)

## Goal

The card, in English: "Make something for this in Settings. Let a person have several Claude accounts, like claude,
claude1 and so on, and look at other agents too: Codex, Pi, Grok… It must be possible to see what such an agent
shares: a claude2 that is the same as claude but on another account, keeping the sessions, plugins and so on; a
claude3 with everything new, sharing nothing with the other two; a claude4 that shares everything of claude3. Make it
very customizable. Two Codex accounts too: codex opens one account, codex2 another, sharing the same sessions and
plugins or not."

A person who pays for two plans (work and personal, or two Max plans to get past a limit) opens either one from the
canvas as a separate agent, `claude` or `claude2`, and decides item by item what the second one takes from the first:
its settings, its instructions, its skills, its plugins, its sessions. Every agent below keeps everything in one
folder that a variable of its environment moves, so an account is that agent run with its own folder, some of whose
items are links to another account's. Its login is never one of them.

**Success criteria**

1. **Settings ▸ Agents ▸ Accounts** adds an account of any agent of [the table](#which-agents), edits what it shares,
   logs it in and deletes it.
2. An account is an agent like the others: **AI agent ▸** lists it, `spawn_agent` opens it by its id (`claude2`), a
   card can be for it, its block is titled by its name, and it comes back on its own session after a restart.
3. Each item of its folder is its own or another account's, with chains (claude4 takes claude3's, which takes
   claude's); a choice changed later never deletes anything.
4. Templum never reads, links, moves, copies or deletes an agent's login (its credentials and its account state).
5. Plan limits and their notifications are per account.
6. All tests in [Testing](#testing) pass in CI on the 3 OS.

## Out of scope

- **Logging in for the person** or reading a login: the agent's own login runs in a terminal.
- **Accounts of an agent that keeps its login outside its folder** (the system keychain, not told apart by folder):
  two folders would be one account. [Which agents](#which-agents) leaves them out.
- **API keys:** an agent that only takes a key from its environment (Aider) has nothing to tell accounts apart but the
  key; the person sets it in their own shell.
- **A command, resume command, model list or commit-message command of an account's own:** it runs its agent's, as
  Settings ▸ Agents has them.
- **Typing `claude2` in any terminal:** see [Typed in a terminal](#typed-in-a-terminal).
- **Moving an account's content** when its folder changes, and **copying** an item from another account: Templum
  links or leaves alone; a copy is the person's to make.

## Words

- **An account:** an entry of `[agents]` that is another folder of one of Templum's agents ("its agent"), with an id
  (`claude2`) and a name ("Claude Code 2").
- **Its folder:** the folder its agent's variable names while it runs, `<data>/accounts/<id>` unless the person
  chose one.
- **The agent's own folder:** where the agent keeps everything without Templum's account (its variable as Templum's
  environment has it, else its default, `~/.claude`; a variable naming one of its accounts' folders is a Templum
  started in that account's block, and the default is the agent's own). It is the agent's default account, whose id is the agent's
  (`claude`).
- **An item:** one file or folder of an agent's folder that an account may share, from [the table](#which-agents)
  (Claude Code's `skills/`).
- **Own / shared with X:** the item is the account's (a real file or folder in its folder, or nothing yet), or a link
  to the same item of X, the agent's own folder or another account of the same agent.
- **Its login:** the files of the agent's folder that hold its credentials and its account's state, which
  [the table](#which-agents) names; never an item.

## Which agents

Researched on 2026-10-18 from each agent's documentation and source, and, for Claude Code 2.1.286, Codex 0.154,
Gemini CLI 0.63, Copilot CLI 1.0.98, Grok Build 1.0.50, Crush 0.98.1 and Pi 0.85.1, on this machine (links in
[Sources](#sources)). An agent has accounts when one variable moves everything it keeps, its login included, and its
login is told apart by that folder.

| Agent | Variable: the folder it names | The agent's own folder | Its login: never touched | Log in types |
|---|---|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR`: the folder | `~/.claude` | `.credentials.json`, `.credentials.lock`, `.claude.json`, `.config.json`, `backups/`, `remote-settings.json`, `policy-limits.json`, `stats-cache.json`, `mcp-needs-auth-cache.json`, `jobs/`, `daemon/` | `claude auth login` |
| Codex | `CODEX_HOME`: the folder | `~/.codex` | `auth.json`, `.credentials.json` | `codex login` |
| Gemini CLI | `GEMINI_CLI_HOME`: the folder that holds `.gemini/` | `~/.gemini` | `oauth_creds.json`, `google_accounts.json`, `gemini-credentials.json`, `mcp-oauth-tokens.json`, `a2a-oauth-tokens.json`, `.env` | `gemini`, then `/auth` in it |
| GitHub Copilot CLI | `COPILOT_HOME`: the folder | `~/.copilot` | `config.json`, `mcp-oauth-config/`, `mcp-secrets/` | `copilot login` |
| Pi | `PI_CODING_AGENT_DIR`: the folder | `~/.pi/agent` | `auth.json` | `pi`, then `/login` in it |
| Grok Build | `GROK_HOME`: the folder | `~/.grok` | `auth.json`, `mcp_credentials.json`, `managed_config.toml`, `requirements.toml` | `grok login` |
| Kimi Code | `KIMI_CODE_HOME`: the folder | `~/.kimi-code` | `credentials/`, `device_id`, `region` | `kimi login` |
| Crush | `CRUSH_GLOBAL_DATA`: the folder | its data folder (`~/.local/share/crush` on Linux) | `crush.json` | `crush login` |

The items each one shares (key, what the dialog calls it, its paths; a path ending in `/` is a folder). An item with
several paths shares them together: they only make sense as one.

- **Claude Code:** `settings` Settings (`settings.json`); `instructions` Instructions (`CLAUDE.md`); `rules` Rules
  (`rules/`); `skills` Skills (`skills/`); `agents` Subagents (`agents/`); `commands` Commands (`commands/`);
  `output-styles` Output styles (`output-styles/`); `hooks` Hook scripts (`hooks/`); `plugins` Plugins (`plugins/`);
  `keybindings` Keybindings (`keybindings.json`); `sessions` Sessions, for /resume (`projects/`, `file-history/`,
  `plans/`); `history` Prompt history (`history.jsonl`). Claude Code writes `settings.json` and `.claude.json`
  through a link (checked with 2.1.286). Its note: "MCP servers you added with `claude mcp add -s user` live with the
  login: add them again in this account, or keep them in a project's `.mcp.json`. It also asks again to trust each
  folder."
- **Codex:** `config` Settings, MCP servers and profiles (`config.toml`); `instructions` Instructions (`AGENTS.md`);
  `hooks` Hooks (`hooks.json`); `rules` Rules (`rules/`); `agents` Agents (`agents/`); `skills` Skills (`skills/`);
  `plugins` Plugins (`plugins/`); `sessions` Sessions (`sessions/`, `session_index.jsonl`); `history` Prompt history
  (`history.jsonl`). Codex writes `config.toml` through a link (checked with 0.154); that file also keeps which folders
  and hooks the person trusts. Its databases (`state_5.sqlite` and the others) are never shared: two processes of two
  folders on one database is what Codex's lock bugs are made of. Its note: "Codex's own list of sessions to resume
  reads a database of each folder: another account's shared sessions resume by their id. Skills in ~/.agents/skills
  are every account's."
- **Gemini CLI:** `instructions` Instructions (`GEMINI.md`); `commands` Commands (`commands/`); `skills` Skills
  (`skills/`); `extensions` Extensions (`extensions/`). Not `settings.json`: Gemini CLI writes it by a rename, which
  replaces a link with a file (checked with 0.63). Not its sessions: their folders are named from a map
  (`projects.json`) that it rewrites the same way. Its note: "Gemini CLI rewrites its settings.json, so each account
  has its own. Its login is this folder's when you sign in with Google (the default); an API key goes to the system's
  keychain, one for every folder."
- **GitHub Copilot CLI:** `settings` Settings (`settings.json`); `instructions` Instructions
  (`copilot-instructions.md`, `instructions/`); `agents` Agents (`agents/`); `skills` Skills (`skills/`); `hooks`
  Hooks (`hooks/`); `extensions` Extensions (`extensions/`); `mcp` MCP servers (`mcp-config.json`); `lsp` Language
  servers (`lsp-config.json`). Not its sessions (`session-store.db`, a database) nor its plugins (their list is in
  `config.json`, with the login). Its login is in the system's keychain, one per GitHub account, and `config.json`
  says which one this folder uses. Its note: "Copilot CLI not logged in here uses the GitHub CLI's login (gh)."
- **Pi:** `settings` Settings (`settings.json`); `keybindings` Keybindings (`keybindings.json`); `system` System
  prompt (`SYSTEM.md`, `APPEND_SYSTEM.md`); `extensions` Extensions (`extensions/`); `skills` Skills (`skills/`);
  `prompts` Prompts (`prompts/`); `themes` Themes (`themes/`); `tools` Tools (`tools/`); `sessions` Sessions
  (`sessions/`). Not `models.json`: a custom provider's key may be in it.
- **Grok Build:** `config` Settings (`config.toml`); `instructions` Instructions (`AGENTS.md`); `rules` Rules
  (`rules/`); `skills` Skills (`skills/`); `agents` Agents (`agents/`); `commands` Commands (`commands/`); `plugins`
  Plugins (`plugins/`, `installed-plugins/`); `sessions` Sessions (`sessions/`). Not `hooks/`: its sandbox refuses
  links among its hooks' paths. It follows a link inside its folder (checked with 1.0.50 on a temporary home:
  `grok inspect` lists a skill of a `skills/` linked to another folder's).
- **Kimi Code:** `config` Settings (`config.toml`); `instructions` Instructions (`AGENTS.md`); `mcp` MCP servers
  (`mcp.json`); `skills` Skills (`skills/`); `plugins` Plugins (`plugins/`); `sessions` Sessions (`sessions/`).
- **Crush:** none. Its login and the settings it writes are one file, `crush.json`: an account of Crush is a login of
  its own and shares nothing (its sessions are each project's, `.crush/`, already common to every account). Its note
  says so.

**Variables that win over every account's login,** named under the items when Templum's own environment has one (the
person's shell may too): Claude Code `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`,
`CLAUDE_SECURESTORAGE_CONFIG_DIR`; Copilot CLI `COPILOT_GITHUB_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`: "GH_TOKEN is set
in your environment: Copilot CLI uses it whatever this account's login."

**The macOS Keychain.** Claude Code keeps a login on macOS in the Keychain, under a name made from the exact text of
`CLAUDE_CONFIG_DIR` (documented; the same code since 1.0.30): Templum gives an account the same text every time (its
folder as Templum makes it, no trailing separator), so moving Templum's data folder or choosing another folder for the
account asks to log in again. Setting the variable to `~/.claude` would name another, empty entry: the agent's own
folder is the variable left unset. Copilot CLI's login is in the system's keychain on every OS, by GitHub account.

**Left out**, and why:

| Agent | Why |
|---|---|
| OpenCode, Kilo Code | No variable of their own: only `XDG_DATA_HOME`, which every program they start would inherit |
| Cursor CLI | `CURSOR_CONFIG_DIR` moves only `cli-config.json`; its login stays where it was |
| Mistral Vibe | Its key is in the system's keyring under one name for every folder |
| Amp | No variable for its folder; its login is in the keyring |
| Auggie | Only a flag (`--augment-cache-dir`) moves its login; no variable |
| Qwen Code | Its sign-in ended (2026-04-15): an account is an API key |
| Aider | API keys only |
| Kiro CLI | `KIRO_HOME` is ignored by its current version (its issue 10417); its login's place unconfirmed |
| Droid | Its variable (`FACTORY_HOME_OVERRIDE`) is undocumented |
| Cline | Two at once need a second variable (`CLINE_HUB_PORT`) for its fixed port |
| Goose | Its keyring entry is one for every folder unless a second variable (`GOOSE_DISABLE_KEYRING`) is set |

One variable per agent is the rule; Cline and Goose come with a second variable when someone asks.

## An account in config.toml

Accounts are entries of `[agents]` ([agents.md](agents.md#changing-ours-adding-yours)) with `of`:

```toml
[agents.claude2]
of = "claude"                      # the agent it is an account of
name = "Claude Code 2"             # missing: "Claude Code (claude2)"
show = true                        # the canvas's menu lists it
# folder = "~/.claude-work"        # missing: <data>/accounts/claude2
share = { settings = "claude", instructions = "claude", skills = "claude", plugins = "claude", sessions = "claude" }

[agents.claude3]
of = "claude"                      # no share: everything its own

[agents.claude4]
of = "claude"                      # everything of claude3's ("Share everything with" writes every key)
share = { settings = "claude3", instructions = "claude3", rules = "claude3", skills = "claude3", agents = "claude3", commands = "claude3", output-styles = "claude3", hooks = "claude3", plugins = "claude3", keybindings = "claude3", sessions = "claude3", history = "claude3" }
```

- `of` is one of Templum's agents that [has accounts](#which-agents). An entry with `of` takes only `of`, `name`,
  `show`, `folder` and `share`; `command`, `resume`, `generate` and `models` are its agent's in effect (with the
  person's changes) and refused here ("agents.claude2.command: an account runs its agent's command").
- `share` maps an item's key ([the table](#which-agents)) to who it is shared with: the agent's id (its own folder)
  or another account of the same agent. A missing item is the account's own. An unknown key, a target that is
  neither, an account of another agent, or a chain that comes back to the account (claude3's `skills` from claude4,
  claude4's from claude3) is a config error naming the key, and the last valid config stays.
- `folder`: `~` is the home folder. It is a full path (`/…` or `~/…`; `C:\…` on Windows): a relative one would be each
  block's working folder. It can't be the agent's own folder, nor another account's, nor in Templum's folder for
  accounts (`<data>/accounts`, whose folders go with their account: "that is Templum's folder for accounts: leave
  Folder empty"), nor the home folder.
- The id of an account added in Settings is its agent's id and the first free number from 2: `claude2`, `claude3`,
  `codex2`; a number whose `<data>/accounts/<id>` is still there (an old folder kept by a change of folder, with its
  login) is taken. It never changes; the name does. One written by hand may be any free id (`work`).
- An account isn't one of Templum's agents: Settings ▸ Agents' **Reset all** leaves it, and its agent's ↺ too.

## Its folder and its items

### Making and changing it

What the account's choices say is applied when the dialog saves it, and again each time it starts (the menu,
`spawn_agent`, a restore, Log in), so a link that went missing is made again:

- **The folder** is made when missing (0700 on Unix). One the person chooses must be new, empty or already the
  agent's: its login there, or a link at one of its items. Save refuses any other, whose real items it would keep
  aside ("~/Documents already holds files and isn't a Claude Code folder: choose a new or empty folder"), when the
  folder is new to the account; the home folder is never one ("~ is your home folder: choose a folder of its own").
  One that already has files is taken as it is, and the dialog reads its links when it is chosen: a link to the same
  item of the agent's own folder or of an account becomes that item's choice, so a folder set up by hand keeps its
  sharing.
- **Shared item, nothing there:** a link to the same item of who it is shared with. That item is a folder Templum
  makes (empty) when missing, so a link never points at nothing; a file that doesn't exist there yet isn't linked, and
  is linked at a later start once the agent made it (its settings file, written at its first change).
- **A link Templum didn't make** (one pointing anywhere but the same item of the agent's own folder or of one of its
  accounts, like a dotfiles folder): left as it is, whatever the choice; the dialog shows "Linked by you to
  ~/dotfiles/claude/skills" in place of its dropdown. Templum only ever replaces or removes the links it would make.
- **Shared item, a link Templum would make there:** pointed at who it is shared with now (only the link is
  replaced).
- **Shared item, a real file or folder there**, when the dialog saves: it is kept aside as `<item>.own` (then
  `.own-2`, …) and the link made. At a start, nothing is moved: the agent replaced the link with its own copy (some
  write a settings file by a rename), and a notification says so: "Claude Code 2's settings.json isn't shared any
  more: Claude Code wrote its own. Edit the account to share it again." Saving the dialog then keeps that copy aside.
- **Own item, a link Templum would make there:** the link is removed (never what it points at), and `<item>.own`
  comes back when it is there. Otherwise the item stays missing and the agent starts it fresh.
- **Own item, a real one there:** left as it is.
- **The login** ([the table](#which-agents)) is never made, linked, moved, read or deleted; an account's folder
  starts without it, and the agent's login writes it there.
- **Chains** follow: a link points at the other account's item, not at what that one links to, so claude4's `skills`
  is claude3's whatever claude3 decides later.

A problem (a folder that can't be made, a link the OS refuses) is one notification naming the item and the reason, and
the account still opens with what could be done.

### The links on each OS

- **Linux and macOS:** symbolic links, for files and folders.
- **Windows:** a folder is shared by a directory junction, which needs no administrator and no Developer Mode. A file
  is shared by a symbolic link, which Windows allows only with Developer Mode on (or as administrator); without it
  the item stays its own, and the dialog says so beside it: "Windows shares a file only with Developer Mode on
  (Settings ▸ System ▸ For developers). Folders share without it." A hard link is never used: an agent that writes
  its file by a rename would silently stop sharing it.

### Deleting an account

**Delete this account** in its dialog asks first:

- Refused while a block of a running workspace is the account's, its agent running in it or only its shell ("Claude
  Code 2 has 2 blocks: close them first"): typing its agent in one would make its folder again, with a login no row
  manages. Refused too while another account shares one of its items ("Claude Code 4 shares Claude Code 2's skills:
  change it first"). Refused too while another account's folder is in its own, as a config.toml written before
  Templum refused that can have ("Claude Code 3's folder is in Claude Code 2's: change it first").
- A block of it that comes back later (Ctrl+Z, a stopped workspace started, a restore) is a plain terminal: an
  `"account"` Templum doesn't know gives no variable and no account, as in an older Templum.
- **Its folder in Templum's data folder** is deleted with it, its links removed first (what they point at stays; a
  folder that is itself a link: only that link): "Its
  folder is deleted: its login and everything it doesn't share. What it shares stays where it is." For an agent whose
  login is in the system's keychain (Claude Code on macOS, Copilot CLI everywhere), the question adds "Its login stays
  in the system's keychain: run /logout in it first to remove it."
- **A folder the person chose** is kept: "Its folder ~/.claude-work stays."
- Its entry leaves `[agents]`; a card for it keeps the id (the person changes it), as a card for a closed block does.

### Changing its folder

Saving an account with another folder is refused while deleting it would be (its blocks open, another account
sharing from it), with the same words ending "change its folder". Then the account's choices are applied in the new
folder, and **the old one stays**, whether Templum made it or the person chose it: it holds a login, which Templum
never deletes on a change. The dialog says, under the folder: "Its old folder stays
(~/.local/share/templum/accounts/claude2). Log in again: a login is its folder's." On macOS that is so even for the
same files moved: the Keychain names a login by the folder's path.

## Where an account shows up

### Opening it

- **AI agent ▸** lists it after Templum's agents and the person's own (accounts by id), by its name, installed when
  its agent is. The folder dialog is the same ("Open Claude Code 2").
- **`spawn_agent`** takes its id (`agent: "claude2"`), its agent's `model` and `effort`, and a worktree, as for its
  agent. The tool's `agent` description adds "or an account, like claude2".
- **Opened** (the menu, `spawn_agent`, Log in, a restore), its block's shell gets the account's variable in its
  environment (`CLAUDE_CONFIG_DIR=<its folder>`), with what its agent gets ([connections.md](connections.md)), and its
  agent's command is typed as for its agent: Templum's flags (`--settings`, `--mcp-config`, Codex's `-c`…) don't
  depend on the folder. The variable is the shell's, not one command's: the agent typed again in that block, after
  it quits, is the same account.
- **The person's own variable:** a shell whose startup files set the same variable again (an `export
  CLAUDE_CONFIG_DIR=…` in `.bashrc`) runs the agent in that folder instead; Templum can't tell
  ([Known limits](#known-limits)).

### Its block

- A terminal Templum opened for an account **is that account's** (`Pane::account`), saved with the block
  (`"account": "claude2"`), kept by a restart, Ctrl+Z and a move to another workspace, even while no agent runs in it.
- **Its agent:** when the 5-second snapshot finds the account's agent running in such a block, the block's agent is
  the account (`claude2`): its name is the block's title for the person and in `list_blocks` ("Claude Code 2"), its
  usage chip is that session's, Shift+Enter and the question before closing are its agent's. Another agent typed there
  is that agent, on its own folder.
- **Restore:** the block's shell starts with the account's variable, also when its folder is gone and nothing is
  resumed; its session comes back as its agent's does (`claude --resume <id>`, read from the account's folder:
  `sessions/<pid>.json`, the rollouts of its `sessions/`).
- An older Templum ignores `account`, and doesn't know an agent `claude2`: it resumes nothing there.

### Usage and limits

Plan limits are an account's ([usage.md](usage.md)), so each account is its own row of the meter and the popover,
named by its name, and its own **in the title bar** switch in Settings ▸ Usage (`usage.hide` takes its id):

- **Claude Code:** the status-line relay, which Claude Code runs with its environment, writes the folder its
  `CLAUDE_CONFIG_DIR` names into the file it writes (`"templum_config_dir"`); an account's limits come from the
  newest relay file of its folder. The `/usage` probe runs for the agent's own folder as today, and every 5 minutes
  for each Claude Code account that ran since Templum started, with its variable.
- **Codex:** an account's limits come from the rollouts of its own folder's `sessions/`. When several of a Codex's
  folders share one `sessions` (codex2's linked to codex's), its newest rollouts may be either one's: then each one's
  limits come only from the rollouts its own running sessions write (each process's file, as its usage chip finds
  it), and stay as last read while none runs.
- A session's usage chip reads the account's files (Claude Code's transcript fallback, Codex's rollout).
- Notifications name the account: "Claude Code 2: 90% of the 5-hour limit used".
- **Matched by folder:** a session whose relay names an account's folder counts for that account, whatever block it
  runs in (a person's own `CLAUDE_CONFIG_DIR=… claude` in a plain terminal, when that folder is an account's); a
  folder no account has counts for the agent's own (a shell's startup files may name one of the person's).

### Cards for it

[kanban.md](kanban.md#who-a-card-is-for): **Any session of ▸** lists the accounts with the agents. A card for an
account (`agent:claude2`) is taken only by a block whose agent is that account. A card for its agent
(`agent:claude`, "Any Claude Code") is taken by any of the agent's blocks, its accounts' included: they run its program.

### Typed in a terminal

- `claude` typed in a plain terminal is the agent's own folder, as today (cards 42 and 44 unchanged).
- `claude` typed in an account's block is that account: the shell has its variable.
- **`claude2` isn't a command** in Templum's terminals. Telling an agent typed by hand apart by its folder means
  reading the agent process's environment on each OS (Templum knows a block's account because it set the variable),
  and four shells' functions defined from a list; the menu, `spawn_agent` and the account's own block cover the card.
  Added when someone asks: [Considered and left out](#considered-and-left-out).

### Templum's server and hooks

Unchanged per account: they come from what is typed, not from the folder. What does live in the folder:

- **Codex** keeps its trust of Templum's hooks in its `config.toml`: an account with its own asks once more ("Hooks
  need review"); one that shares `config` with a trusted folder is trusted.
- **Gemini CLI** keeps Templum's extension in `extensions/`: an account that shares `extensions` with a folder that
  has it has it. One with its own: the notice of
  [Gemini CLI without its extension](connections.md#gemini-cli-without-its-extension) checks that account's folder,
  and its Install links the extension there (the same `gemini extensions link`, run with the account's variable).
- **Claude Code** keeps the person's user-scope MCP servers in `.claude.json`, with the login: an account doesn't
  share them. Its dialog says so under the items: "MCP servers you added with `claude mcp add -s user` live with the
  login: add them again in this account, or keep them in a project's `.mcp.json`."

### Commit messages

An account is one of the agents **Generate ✨** can use ([git.md](git.md)): its agent's generate command and models,
run with the account's variable, so the message is written on that account's plan.

## Settings ▸ Agents ▸ Accounts

Under the agents' rows and **Add an agent**, a heading **Accounts** with the line "Another login of an agent, with its
own folder: choose what it shares with the others." Accounts aren't among the agents' rows above it (nor in the
welcome's Agents page).

- **A row** per account, by id: its name; "Account of Claude Code · ~/.local/share/templum/accounts/claude2" (the
  folder, home as `~`); a switch (the menu lists it); **Log in**; **Edit**.
- **Add an account** opens the dialog, empty. No row and the button alone when there is none.

### The dialog

"Add an account" or "Edit Claude Code 2":

1. **Agent:** a dropdown of the agents of [the table](#which-agents), installed first; fixed once the account exists
   (shown as text).
2. **Name:** prefilled with the agent's title and the id's number, "Claude Code 2". Under it, "Its id: claude2 (what
   agents and the canvas file call it)".
3. **Folder:** "Templum's folder for it (`<data>/accounts/claude2`)" or a path, with **Choose…** (the system's
   folder picker, [agents.md](agents.md#opening-one)) and a field (`~` is home).
4. **Share everything with:** a dropdown, "Choose…" then the agent ("Claude Code: its own folder") and its other
   accounts; choosing one sets every item below to it. It changes nothing by itself afterwards.
5. **The items**, one row each: its label and its path (`skills/`), and a dropdown: **Own**, the agent ("Claude Code:
   its own folder"), then each other account of that agent. An item Windows can't share without Developer Mode says
   so in place of the dropdown. A shared file whose source has none yet says "Shared once Claude Code writes one".
6. Under the items, its agent's note when it has one (Claude Code's MCP servers, above), and "Its login is never
   shared: log in to it with Log in."
7. **Delete this account**, for one that exists ([Deleting](#deleting-an-account)).

**Save** checks what config.toml would (a cycle, a folder taken) and says it in a notification with the dialog left
open; then writes the entry and applies the folder's choices. Saving an account that runs changes its folder at once:
what its agent has open keeps working, and what it opens next follows the new choice.

### Log in

**Log in** opens a terminal block at the view's center, in the home folder, that is the account's (its variable), and
types its agent's login command from [the table](#which-agents) and Enter. The person logs in there, and closes it or
keeps it. A first start without a login asks to log in anyway: Log in is the short way to do it before opening the
account in a project.

## Calls made for card 37

The person was away; each can be changed.

1. **An account is an `[agents]` entry with `of`,** not a table of its own: it is in every list of agents at once (the
   menu, `spawn_agent`, cards, Generate, usage) with no second list to keep in step.
2. **Its id is `claude2`**, the agent's id and a number, as the card writes it, fixed at creation; its name is free.
3. **The folder is the account.** The variable moves everything an agent keeps, so an account is a folder and links,
   with no copy of the agent's settings in Templum.
4. **Item by item, own or shared with one of the agent's folders**, and "share everything with" filling the rows;
   chains are links to links, so claude4 follows claude3's later choices.
5. **Nothing is ever deleted by a change:** a real item is kept aside as `<item>.own` and comes back when it is own
   again; deleting an account deletes only a folder Templum made, after asking.
6. **Windows shares folders always (junctions) and files only with Developer Mode (symbolic links)**, never hard
   links, which an agent's rename breaks without anyone seeing.
7. **The block is the account's**, through its shell's variable, saved with the block: Templum knows an account's
   session because it set its folder, on every OS, without reading another process's environment.
8. **No `claude2` command in terminals** ([above](#typed-in-a-terminal)); `claude` in an account's block is it.
9. **"Any Claude Code" includes its accounts; a card for claude2 is claude2's only.**
10. **Limits per account**, the relay naming its folder; the `/usage` probe only for accounts that ran this run.
11. **Log in types the agent's own login command** in a block of the account; Templum never touches the login.
12. **The settings live in Settings ▸ Agents**, under the agents, rather than a section of their own: an account is an
    agent there too.
13. **Templum only touches the links it would make** (to the same item of one of the agent's folders); a person's own
    links are left and shown, and a folder set up by hand is read into the dialog's choices.
14. **Deleting an account, or changing its folder, waits for its blocks to close**, its shells included; a block of it
    that comes back afterwards is a plain terminal.
15. **A folder change keeps the old folder:** it holds a login, never deleted except by deleting the account.
16. **Limits follow the folder**, not the block, since a login is its folder's.

## Considered and left out

- **`claude2` typed by hand** (above): a list of accounts in a variable, a function per account in bash, zsh, fish
  and PowerShell, and the agent process's environment read by the snapshot (sysinfo's `environ`, on each OS) to tell
  its folder. When someone asks: those three, and a block's account found by its process's variable first.
- **A copy instead of a link** ("own, starting from claude's"): a copy of `projects/` can be gigabytes, and the
  person can copy one file by hand.
- **`share = "claude3"` for every item at once:** one more form to parse; the dialog's "Share everything with" writes
  every key, so an item a later Templum adds isn't shared without the person saying so.
- **An account's own command or model** (`claude2 = { command = "claude --model opus" }`): the agent's command is the
  account's; a field of its own when someone asks.
- **Sharing the login** (two folders, one login): that is the agent's own folder.

## Known limits

Each goes to [docs/minors.md](../minors.md) under **Agent accounts** with the code that has it.

- A shell whose startup files set the agent's variable again runs the agent in that folder, not the account's.
- An agent that rewrites a shared file by a rename stops sharing it; Templum notices only at the account's next start.
- A card for a deleted account keeps its id until the person changes it.
- Claude Code syncs a claude.ai account's skills and plugins into `skills/synced/` and `plugins/synced/`: two accounts
  sharing `skills` or `plugins` share those too, and each sign-in syncs its own over them.
- Claude Code's Console sign-ins without a key (`~/.config/anthropic`) and Codex's MCP servers' OAuth logins (the
  keyring, by server) are one for every folder.
- Gemini CLI's `advanced.autoConfigureMemory` is read from the account's folder itself, not its `.gemini/` (its issue
  27421).

## Architecture

| File | Change |
|---|---|
| `templum-usage/src/accounts.rs`, `accounts/folder.rs` (no UI; a module of the crate that already knows each agent's folders, not a crate of its own) | Each agent's variable, own folder, items and login; applying an account's choices to its folder, reading a folder's links back, keeping items aside, deleting a folder without following links; junctions on Windows |
| `templum/src/config.rs` | `of`, `folder`, `share` in `[agents.<id>]`, validated (cycles, targets, keys) |
| `templum/src/agents.rs` | Accounts among the agents in effect (`AgentEntry::account`), their variable, their id; left out of detection |
| `templum/src/workspace/blocks.rs`, `usage.rs` | An account's variable in its block's shell, its choices applied at each start, Log in |
| `templum/src/workspace/mod.rs`, `persist.rs`, `session.rs` | `Pane::account`; the snapshot's agent is the account; `"account"` in the canvas file; restore |
| `templum-usage` | A process's account folder (`Proc::home`), the relay's `templum_config_dir`, limits per account |
| `templum/src/workspace/content.rs` | A card for an account |
| `templum/src/settings/accounts.rs` (new) | The Accounts rows and dialog, Log in, Delete |
| `templum-git/src/message.rs` | Generate runs with an account's variable |

## Testing

**Unit (`templum-usage`'s `accounts`):** no agent's item is or holds one of its login's names; every key is unique per agent; a
shared item becomes a link to the other folder's item, a missing source folder is made, a missing source file isn't
linked; a real item is kept aside on save (`.own`, then `.own-2`) and left at a start (reported); own again removes
only the link and brings `.own` back; a link the person made elsewhere is never replaced nor removed, and a folder
set up by hand is read back as choices; a chain links to the middle account's item; the login file is never touched
by any of these; deleting removes the folder's links before the folder and leaves their targets; Windows: a folder is a
junction, a file without the privilege is refused with the copy above.

**Unit (`templum`):** `[agents.claude2] of = "claude"` parses; `command` on an account, an unknown item, a target of
another agent, a cycle and a taken folder are errors; an account inherits its agent's command and resume, with the
person's changes; ids `claude2`, `claude3`; accounts aren't detected as agents; its variable; a card for `claude2` is
taken by a claude2 block only, one for `claude` by both.

**Unit (`templum-usage`):** the relay writes `templum_config_dir` from its environment; limits per folder; Codex limits from a
process's own rollout when sessions are shared; a process's session id read from its account's folder.

**Headless UI:** the menu opens an account with its variable in the shell's environment and its name as the cover's
title; `spawn_agent` takes `claude2`; a snapshot that finds Claude Code in an account's block names the account; the
canvas file keeps `account` and a restore gives the shell the variable; Settings adds an account (an `[agents]` entry
and a folder with its links), edits a share, refuses a cycle, logs in (a block of the account, its login typed) and
deletes one (the folder gone, the shared source left); deleting or moving an account with a block of it open (its
shell alone too) is refused, and a block of a deleted account comes back plain.

**Manual (Hyprland, and macOS for the Keychain):** two Claude Code accounts logged in, each with its own limits;
claude2 sharing claude's sessions resumes one of claude's with `/resume`; codex2 with its own login; a Windows
account sharing `skills` by a junction.

## Risks

- **An older Templum and an account entry:** `[agents]` refuses unknown keys, so an older Templum reports
  `agents.claude2.of` as a config error and keeps its last valid config (at its start: the defaults). Downgrading
  after adding an account needs the entry removed.
- **Agents change their folders:** an item renamed by a new version of an agent is a link to nothing that the agent
  ignores; [the table](#which-agents) is checked again when an agent's version changes what it keeps.
- **Sessions of two accounts in one folder:** an agent's own resume list shows both accounts' sessions; resuming the
  other account's session works where the agent keeps them by id (Claude Code's `projects/`).

## Sources

Read on 2026-10-18. Write-through checks were made in a temporary folder named by each variable, never the person's.

- **Claude Code:** [authentication](https://code.claude.com/docs/en/authentication) (several accounts, the Keychain
  per `CLAUDE_CONFIG_DIR`), [env-vars](https://code.claude.com/docs/en/env-vars),
  [claude-directory](https://code.claude.com/docs/en/claude-directory), [skills](https://code.claude.com/docs/en/skills)
  (synced skills); issues [92252](https://github.com/anthropics/claude-code/issues/92252) (the variable set to
  `~/.claude` names another entry), [79223](https://github.com/anthropics/claude-code/issues/79223),
  [87447](https://github.com/anthropics/claude-code/issues/87447); `claude --help`, `claude auth --help` (2.1.286).
- **Codex:** [config-advanced](https://learn.chatgpt.com/docs/config-file/config-advanced),
  [config-reference](https://learn.chatgpt.com/docs/config-file/config-reference); source:
  [types.rs](https://github.com/openai/codex/blob/main/codex-rs/config/src/types.rs) (`file` is the default store),
  [storage.rs](https://github.com/openai/codex/blob/main/codex-rs/login/src/auth/storage.rs) (the keyring by home),
  [edit.rs](https://github.com/openai/codex/blob/main/codex-rs/core/src/config/edit.rs) (writes through a link),
  [oauth.rs](https://github.com/openai/codex/blob/main/codex-rs/rmcp-client/src/oauth.rs); issues
  [46269](https://github.com/openai/codex/issues/46269), [42447](https://github.com/openai/codex/issues/42447),
  [35555](https://github.com/openai/codex/issues/35555), [4432](https://github.com/openai/codex/issues/4432).
- **Gemini CLI:** [configuration.md](https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/configuration.md),
  [enterprise.md](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/enterprise.md),
  [paths.ts](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/utils/paths.ts),
  [commentJson.ts](https://github.com/google-gemini/gemini-cli/blob/main/packages/cli/src/utils/commentJson.ts)
  (writes by a rename),
  [oauth-credential-storage.ts](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/code_assist/oauth-credential-storage.ts),
  [apiKeyCredentialStorage.ts](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/core/apiKeyCredentialStorage.ts);
  issues [23622](https://github.com/google-gemini/gemini-cli/issues/23622),
  [27421](https://github.com/google-gemini/gemini-cli/issues/27421).
- **GitHub Copilot CLI:** [config dir reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference),
  [authenticate](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/authenticate-copilot-cli);
  `copilot help environment` (1.0.98).
- **Pi:** [config.ts](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/config.ts)
  (`getAgentDir`, `getAuthPath`); `pi --help` (0.85.1).
- **Grok Build:** its bundled user guide (`~/.grok/docs/user-guide/`: configuration, authentication, sandbox) and
  binary (1.0.50).
- **Kimi Code:** [data-locations.md](https://github.com/MoonshotAI/kimi-code/blob/main/docs/en/configuration/data-locations.md).
- **Crush:** [load.go](https://github.com/charmbracelet/crush/blob/main/internal/config/load.go),
  [login.go](https://github.com/charmbracelet/crush/blob/main/internal/cmd/login.go); `crush dirs` (0.98.1).
- **Left out:** OpenCode [global.ts](https://github.com/anomalyco/opencode/blob/dev/packages/util/src/global.ts),
  Kilo Code [global.ts](https://github.com/Kilo-Org/kilocode/blob/main/packages/core/src/global.ts), Cursor CLI
  [configuration](https://cursor.com/docs/cli/reference/configuration), Mistral Vibe
  [keyring.py](https://github.com/mistralai/mistral-vibe/blob/main/vibe/utils/keyring.py), Amp
  [settings](https://ampcode.com/docs/cli/settings), Auggie [reference](https://docs.augmentcode.com/cli/reference),
  Qwen Code [storage.ts](https://github.com/QwenLM/qwen-code/blob/main/packages/core/src/config/storage.ts), Kiro CLI
  [issue 10417](https://github.com/kirodotdev/Kiro/issues/10417), Cline
  [paths.ts](https://github.com/cline/cline/blob/main/sdk/packages/shared/src/storage/paths.ts), Goose
  [paths.rs](https://github.com/block/goose/blob/main/crates/goose/src/config/paths.rs).
- **Windows links:** [hard links and junctions](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions),
  [CreateSymbolicLinkW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createsymboliclinkw),
  [RemoveDirectoryW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-removedirectoryw);
  [`junction`](https://crates.io/crates/junction) 2.1 (MIT); Rust's
  [`remove_dir_all`](https://doc.rust-lang.org/std/fs/fn.remove_dir_all.html) never follows a link
  (CVE-2022-21658's fix).
