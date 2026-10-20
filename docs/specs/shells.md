# Shells

- **Date:** 2026-10-11; [A shell for each terminal](#a-shell-for-each-terminal) 2026-10-20
- **Status:** Implemented, *A shell for each terminal* too (kanban card 60,
  [plan](../plans/2026-10-20-shell-per-terminal.md))
- **Builds on:** [config.md](config.md) (`[shell]`), [settings.md](settings.md),
  [context-menus.md](context-menus.md), [persistence.md](persistence.md) and
  [shell-integration.md](shell-integration.md)

## Goal

On Windows a terminal ran Windows PowerShell 5.1, `alacritty_terminal`'s default when Templum names no program: a
plain prompt, few colors, and a poor fit for AI agents, which expect a POSIX shell. Templum now picks a better
default there, and the settings list the shells found on the computer, on every OS.

## The default

| OS | When `shell.program` is not set |
|---|---|
| Windows | The first found of **Git Bash** (`bash.exe --login -i`), **PowerShell 7** (`pwsh -NoLogo`), **Windows PowerShell** (`powershell -NoLogo`) |
| Linux, macOS | The user's login shell (`$SHELL`), as before |

## Finding shells

**Windows**, in this order (each only when its file exists):

| Shell | Where | Arguments |
|---|---|---|
| Git Bash | `bin\bash.exe` beside the `git` in PATH (`…\Git\cmd\git.exe`), else `Git\bin\bash.exe` under `%ProgramFiles%`, `%ProgramFiles(x86)%`, or `%LOCALAPPDATA%\Programs` | `--login -i` |
| PowerShell 7 | `pwsh` in PATH, else `%ProgramFiles%\PowerShell\7\pwsh.exe` | `-NoLogo` |
| Windows PowerShell | `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`, else `powershell` in PATH | `-NoLogo` |
| WSL | `%SystemRoot%\System32\wsl.exe` | none |
| Nushell | `nu` in PATH | none |
| Command Prompt | `%SystemRoot%\System32\cmd.exe` | none |

Git for Windows' login profile doesn't change folder (its `/etc/profile`, checked on 2026-10-11), so Git Bash starts
in the terminal's folder, which agents opened in a project need.

**Linux and macOS:** the shells `/etc/shells` lists that exist, once each by name, in its order, leaving out
`nologin`, `false`, `git-shell`, `rbash`, `tmux`, `screen` and `systemd-home-fallback-shell`; then the known shells it
doesn't name, and `-l` on macOS ([Finding shells, completed](#finding-shells-completed)).

## Settings

**Settings ▸ Shell** gets a **Shell** row above Program and Arguments:

- a list: **Default (*its name*)**, then every shell found;
- choosing one sets `shell.program` (its path) and `shell.args` (its arguments); **Default** removes both;
- a program typed by hand that isn't in the list shows as **Custom**;
- ↺ goes back to the default.

The shells are found when the panel opens. Like the rest of Shell, the choice applies to new terminals.

## Testing

**Unit (`templum-terminal`):** with a made-up Windows (paths and PATH):

- every shell found, in order, with its arguments, and Git Bash as the default;
- Git Bash beside the `git` in PATH;
- without Git Bash, PowerShell 7, then Windows PowerShell by name when nothing is found;
- `/etc/shells` read once per name, without the excluded ones and missing files.

**Headless UI:** choosing PowerShell 7 in the list sets the program and its arguments in config.toml, and ↺ removes
them.

**Manual (Windows):** a new terminal runs Git Bash in the right folder; an agent opened from the canvas runs in it;
the list shows the installed shells. Not checked yet: no Windows machine at hand.

## A shell for each terminal

Kanban card 60, from the person: *"Terminals with different shells: when creating a terminal, list the options and
let me pick one: in the right-click menu the terminal item gets a submenu that lists them, and in the action bar a
click on the terminal opens a small dropdown upward, like the agents'."*

### Goal

Settings ▸ Shell picks the shell of **every** new terminal. A person who works mostly in zsh but now and then wants
fish, Nushell, PowerShell 7 or WSL had to change the setting, open a terminal and change it back. Now each new
terminal can run any shell found on the computer, chosen where terminals are made, and keeps it across restarts.
The setting stays what it was: the default.

**Success criteria**

1. The toolbar's Terminal button and the canvas menu's **Add ▸ Terminal ▸** list the shells found; the one chosen
   runs in the new block, on Linux, macOS and Windows.
2. Ctrl+Shift+T (Cmd+T) and every other way a terminal opens still run the default, as before.
3. A terminal opened with a chosen shell comes back with it after a restart, an undo of its close, or a paste.
4. All tests in [Testing](#testing-1) pass in CI on the three OS, whatever shells the machine has.

**Not in scope:** a shell for agents (AI agent ▸ opens its agent in the default shell, as now), a shell argument for
the agents' `create_block`, new integration scripts, a setting for the menu, shells over SSH.

### Finding shells, completed

The list is `available_shells()`, the one Settings ▸ Shell shows, with two additions on Linux and macOS:

- **Known shells on the PATH.** After `/etc/shells`, each of `bash`, `zsh`, `fish`, `nu`, `pwsh` that it didn't
  name, where Templum finds programs (`templum_terminal::locate`: the PATH, the login shell's PATH, `~/.cargo/bin`…),
  by its path. Homebrew's fish and zsh, and Nushell from cargo, aren't in `/etc/shells` until someone runs `chsh`.
- **Login shells on macOS.** Every shell found gets `-l`, as the default (`$SHELL`, which alacritty starts as a login
  shell there): a shell started from Finder's Templum otherwise misses `/etc/zprofile`'s `path_helper` and the
  `brew shellenv` of `~/.zprofile`. bash, zsh, fish, pwsh, nu, csh, tcsh, ksh, dash and sh all take `-l`. A shell
  chosen in Settings before this change has no `-l`: Settings shows it as **Custom** until it is chosen again.

Labels stay as they are: Windows names (**Git Bash**, **PowerShell 7**, **Windows PowerShell**, **WSL**,
**Nushell**, **Command Prompt**), the program's file name elsewhere (**zsh**, **fish**, **nu**).

The list and the default's name are read when Templum starts and, as before, when Settings or the welcome opens
(`Workspace::shells`, `Workspace::default_shell`). Reading them costs a file read and a few `stat`s.

### The menus

Both menus list the same items, built by one function:

| Item | Opens |
|---|---|
| **Default (*name*)** | What Ctrl+Shift+T opens: `shell.program` and `shell.args` when set, else the system's default. *Name* is the label of the shell found with that program and those arguments, else the program's file name; with no program set, `Workspace::default_shell` (the name Settings shows) |
| — | A separator, when other shells follow |
| *each shell found* | That shell, with its arguments. The one labelled as the default's *name* is left out: it is the first item already |

- **The toolbar's Terminal button** opens this menu at the click, as **AI agent** opens the agents: the menu is kept
  inside the window, so from the toolbar at the bottom it opens upward. The block opens where a new terminal goes
  (`new_terminal_rect`: the view's center, or beside the blocks there). The tooltip still names Ctrl+Shift+T, which
  opens the default without a menu. The menu opens even when the default is the only item: one more click, the
  same gesture every time.
- **The empty canvas's menu**: **Add ▸ Terminal** becomes **Add ▸ Terminal ▸**, a submenu with the items above;
  the block's top-left corner goes where the person right-clicked. The same in grid mode's **Add ▸** (a tile's
  menu, a gap's), where the block takes half of the tile, as now.
- The list scrolls past 420 px, as the agents' does (macOS lists eight shells).

### The block keeps its shell

- A terminal opened from a shell item remembers it (`Pane::shell`: the shell found, label, program and arguments).
  One opened with **Default** or any other way remembers nothing and follows the default.
- Its saved item gets `"shell"`: the shell's **label** (`"fish"`, `"PowerShell 7"`), written only when set; an older
  Templum ignores it. Undo of a close and Copy then Paste keep it: both go through `Pane::item` and `add_saved`.
  Move to workspace moves the running block, its shell with it.
- At startup and from a saved item (`add_saved`: undo, paste), the block starts the shell of that label among the
  shells found (`Workspace::shells`). **Safety:** the file names a shell, never a command, as it names an agent
  ([persistence.md](persistence.md)): an edited file can only pick a shell the computer lists.
- **A label not among the shells found** (uninstalled, another computer's, edited) starts the default instead, and
  the block forgets it: the canvas loses no block for a shell that is gone. A line goes to stderr; no notification.
- The block's title is the shell's file name until the shell sets one (`Terminal::title`), as now.

### Shell integration

Unchanged, and decided by the program's file name ([shell-integration.md](shell-integration.md#loading-the-scripts)):
bash (Git Bash too), zsh, fish, PowerShell 7 and Windows PowerShell get it, with `-l`/`--login` handled as now. The
others (sh, dash, ksh, csh, tcsh, Nushell, Command Prompt, WSL) open without it: no command marks or prompt jumps, no
"Finished after…" news, and the folder comes from the OS. `shell.integration` off turns it off for every shell.

### Agents typed in a chosen shell

What a new terminal's environment gets for the agents a person types there (`TEMPLUM_CLAUDE_ARGS`,
`TEMPLUM_CODEX_ARGS`…, [connections.md](connections.md#agents-typed-in-a-terminal)) depends on the shell: nothing in
WSL, no Codex words in Command Prompt or Windows PowerShell. It is now judged by **the terminal's own shell**, not the
default: a Windows PowerShell terminal opened from the menu while the default is Git Bash gets no Codex words. The
same for the agent's resume command typed into a restored terminal.

### Decisions

Taken by the planner (card 60 left them open):

1. **One list for Settings, the welcome and both menus** (`available_shells`), completed by known shells on the PATH
   and `-l` on macOS, rather than a second detection.
2. **A click on the toolbar's Terminal always opens the menu**, as AI agent does; the keyboard shortcut stays the
   one-step default. Rejected: a split button (one more control in the toolbar), a click that opens the default and
   a long press for the menu (hidden).
3. **Default first, by the name of what it opens**, the found shell of that name not repeated.
4. **The shell is the block's, saved with it by its label**, looked up among the shells found on restore; a default
   terminal saves nothing and follows Settings. Rejected: saving the program and its arguments, which would make the
   canvas file a command Templum runs at start.
5. **A shell not found falls back to the default** on restore, with no notification.
6. **No integration for more shells**: the shells most people use already have it; Nushell, cmd and WSL would each
   need their own script.
7. **The list is read at start and when Settings opens**, not at each menu: a shell installed while Templum runs
   shows after Settings opens or a restart ([minors.md](../minors.md#settings-panel)). Reading it at each menu would
   take the tests' made-up list away.
8. **Agents keep the default shell**; their resume and environment words follow the terminal's own shell.
9. **No new setting.**

### Testing

**Unit (`templum-terminal`):**

- `unix_shells` adds `bash`, `zsh`, `fish`, `nu`, `pwsh` found on a made-up PATH after `/etc/shells`, once each by
  name, by their path; a name `/etc/shells` lists isn't added again;
- with `login`, every shell found gets `-l`; without, none.

**Headless UI (`workspace/tests/`)**, with a made-up list (`Workspace::shells`, `Workspace::default_shell`) whose
shells run the tests' quiet program, so no test depends on the machine's shells:

- the toolbar's Terminal opens a menu; its third item (after Default and the separator) opens a terminal that runs
  that shell (`Pane::shell`; on Unix its title is the program's name), the found shell named as the default being
  left out; Default opens one with no shell of its own;
- **Add ▸ Terminal ▸** a shell opens it at the right-click;
- **Default (*name*)**'s name: the configured shell's label, the file name of a program typed by hand, the system's
  name without a program;
- a terminal's saved item carries its shell's label; a canvas file whose `shell` is the first shell found restores
  the block running it; a saved item whose label isn't found restores it running the default, its shell forgotten;
  closing it and Ctrl+Z bring it back with it;
- `agent_env` for a terminal running `wsl.exe` gives no agent words while the default gives them; on Unix, with
  Windows PowerShell configured, a terminal running bash gets the Codex words;
- the existing tests that click the toolbar's Terminal or **Add ▸ Terminal** pick **Default** in the new menu.

**Manual (Linux):** the toolbar's Terminal opens the menu upward with the shells found; one picked runs in its block
(its title); after a restart it runs again. Windows (Git Bash, PowerShell 7, WSL, cmd) and macOS (`-l`, Homebrew's
fish) not checked by hand: no machine at hand.
