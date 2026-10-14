# Templum

An infinite canvas of terminals. Spawn as many terminals as you like, place them anywhere,
resize them like blocks, and pan and zoom around your workspace.

> **Status: pre-alpha.** Usable as a canvas of terminals; no session saving or settings yet.

## Why

- **Native and light.** Pure Rust, drawn directly on the GPU. No webview, no Electron.
- **Cross-platform.** macOS, Linux (Wayland and X11) and Windows.
- **Built on proven parts.** Terminal emulation by [`alacritty_terminal`](https://crates.io/crates/alacritty_terminal),
  UI by [GPUI](https://www.gpui.rs) (Zed's GPU framework) through [gpui-kit](https://github.com/longbridge/gpui-kit).
  See [the architecture](docs/ARCHITECTURE.md) and the [decision records](docs/adr/).

## Build from source

You need [rustup](https://rustup.rs); the pinned toolchain installs itself.
Platform prerequisites are listed in [CONTRIBUTING.md](CONTRIBUTING.md).

```sh
git clone https://github.com/KingDasWinx/Templum
cd Templum
cargo run -p templum
```

## Using it

| Mouse | Result |
|---|---|
| Double-click empty canvas | New terminal there |
| Drag a title bar / the bottom-right handle | Move / resize the block |
| Drag a block near another | It snaps to the other's edges or center within 8 px, and a line shows which |
| Drag empty canvas, or scroll over it | Pan |
| Hold Alt (Option on macOS), then drag or scroll anywhere | Pan, even over blocks: they don't react while Alt is held |
| Drag with the middle button, anywhere | Pan |
| Scroll over a terminal | Scroll its history |
| Click, drag or scroll in a program that uses the mouse (htop, vim, Claude Code full screen) | The program gets it; hold Shift to select text instead |
| Ctrl+scroll, or pinch | Zoom |
| Drag in a terminal (double-click: word, triple-click: line) | Select text |
| Right-click empty canvas | Menu: **AI agent ▸** (Claude Code, Codex, Gemini CLI and 17 more) or **Add ▸** Terminal, Kanban board, Note, Editor or Git, where you clicked |
| Right-click a block, card, column, board or workspace tile | Its menu (copy, paste, rename, delete, close…) |
| With a drawing tool on, drag the canvas | Draw; Shift keeps lines at 45° and makes squares and circles |

| Linux / Windows | macOS | Action |
|---|---|---|
| Ctrl+Shift+T | Cmd+T | New terminal |
| Ctrl+Shift+K | Cmd+Shift+K | New kanban board block |
| Ctrl+Shift+M | Cmd+Shift+M | New note |
| Ctrl+Shift+E | Cmd+Shift+E | New editor (asks for a folder) |
| Ctrl+Shift+G | Cmd+Shift+G | New Git block (asks for a folder) |
| Ctrl+S | Cmd+S | Save the file (in an editor) |
| Ctrl+W | Ctrl+W | Close the file's tab (in an editor) |
| Ctrl+G | Cmd+G | Go to line (in an editor) |
| Ctrl+Shift+W | Cmd+W | Close the focused terminal |
| Ctrl+Shift+C / V | Cmd+C / V | Copy / paste |
| Ctrl+= / Ctrl+- | Cmd+= / Cmd+- | Zoom in / out |
| Ctrl+0 | Cmd+0 | Zoom 100% |
| Ctrl+Shift+1 | Cmd+Shift+1 | Fit all blocks |
| Ctrl+, | Cmd+, | Settings |
| Ctrl+Tab / Ctrl+Shift+Tab | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous workspace |
| Ctrl+1 … Ctrl+9 | Cmd+1 … Cmd+9 | Workspace 1 to 9 |
| Ctrl+Shift+N | Cmd+Shift+N | New workspace |
| Ctrl+Shift+Q, twice | Cmd+Q, twice | Quit (the first press only asks for the second) |

In a terminal taken over by a program (full screen, an AI agent, or one that asked for the mouse or the kitty
keyboard), zoom, settings, the theme picker and the prompt jumps leave their keys to it: Ctrl+- undoes in Claude Code.
Shift+Enter is a new line in Claude Code and Codex.

## Workspaces

Each workspace is a whole canvas of its own, such as one per project. The sidebar on the left shows them all: click
one to switch, **+** to add one, and right-click a tile to rename or delete it. The button at the left of the title
bar hides or shows the sidebar. Terminals in the workspaces you aren't looking at keep running, so an agent keeps
working while you look elsewhere; a green dot on a tile means Claude Code or Codex is running in it.

## Themes

Templum follows your system's light or dark mode. Click the **palette** button in the title bar to pick a theme: hovering or the
arrow keys preview it, and a click or Enter keeps it for the current mode. The default is Catppuccin: Latte in
light mode, Mocha in dark mode.

Any theme in [Alacritty's format](https://github.com/alacritty/alacritty-theme) works. Put its `.toml` file in the
`themes` folder, and it shows up under "My themes" the next time you open the picker:

| OS | Folder |
|---|---|
| Linux, macOS | `~/.config/templum/themes/` (or `$XDG_CONFIG_HOME/templum/themes/`) |
| Windows | `%APPDATA%\templum\themes\` |

Your choice is saved in `config.toml`, one folder up:

```toml
[theme]
light = "Catppuccin Latte"
dark = "Catppuccin Mocha"
```

## Settings

Press **Ctrl+,** (**Cmd+,** on macOS), or the ⚙ in the title bar, to open the settings panel on the right. It has
Appearance (light/dark mode and themes), Font (with a preview), Terminal (cursor and
scrollback, system notifications), Shell (for new terminals: pick one of the shells found on your computer, shell
integration), Tools (the `git` program), Agents, Usage (notifications about agents' plan limits) and Shortcuts. The terminals stay usable next to it, so you see each change
as you make it.

- Every change applies at once and is saved to `config.toml` a moment later, keeping your comments. Numbers apply
  when you press Enter or leave the field. A field's ↺ button puts it back to its default, and **Reset all** does
  the whole section, by removing their lines from the file.
- To change a shortcut, click it and press the new one, with Ctrl, Alt or Super (Cmd on macOS), or an F key. Esc
  cancels, Backspace leaves the command without a shortcut, and a shortcut another command already has is refused
  with its name. A reset that would give a shortcut to two commands is refused the same way.
- Shortcuts keep working while the panel has the focus, except copy, paste and close terminal, which need a terminal
  to have it.
- Edits you make by hand to `config.toml` show up in the panel.

## Kanban boards

The toolbar's board button (or Ctrl+Shift+K) puts a kanban block on the canvas, Trello-style. Pick a board
in it or create one: it starts with To do, Doing and Done. Drag cards between columns and columns by their titles,
click a card to edit its title, description and colored labels, and right-click a column or a board to rename or
delete it. Boards aren't tied to a canvas: any block, in any workspace, can show any board (the ⇅ button at the left
of its header switches), and blocks showing the same board stay in sync. Boards are kept in `boards.json`, next to
the `workspaces` folder.

## Notes

The toolbar's note button (or Ctrl+Shift+M) puts a note on the canvas. Write Markdown in it: headings, lists (Tab
indents an item), code. The eye button at the left of its header shows it rendered, and back; the first heading is
the note's title. Notes are saved with the canvas.

## Editor

The toolbar's editor button (or Ctrl+Shift+E) asks for a folder and puts an editor on it: a file tree on the
left (folders are read when you open them), tabs of open files, and a code editor with line numbers and syntax
highlighting for the usual languages. Ctrl+S saves (the file keeps its line endings), Ctrl+W closes the tab,
Ctrl+F searches, Ctrl+H replaces, Ctrl+G goes to a line; Markdown files get the note's preview button. A file
changed by another program is reloaded while you haven't edited it, and flagged with Reload / Keep mine when you
have. Right-click the tree for new files and folders, rename, delete, reveal in the file manager, and a terminal
in that folder beside the editor. Closing a block, a workspace or the window, or quitting, asks about unsaved
files first. A file that isn't UTF-8, or is over 4 MB, opens read-only, so its bytes never change; Tab indents
as the language wants it (a real tab in Makefiles and Go).

## Git

The toolbar's Git button (or Ctrl+Shift+G) asks for a folder and puts a Git block on its repository, with what
GitHub Desktop and the editors' Git panels do. **Changes** lists what is staged and what isn't; a row shows its
diff, and its buttons stage, unstage, discard (after asking) or open the file in an editor beside the block.
Below, a summary, a description, **Amend** and **Commit** (Ctrl+Enter from either field; with no author set, the
block asks for `user.name` and `user.email`). **History** lists the commits with their files and diffs, filtered
as you type; right-click one to check it out, make a branch or a tag from it, revert it or copy its hash.
**Branches** checks out on a click, makes, renames, merges, deletes and publishes them, and offers **Stash and
switch** when local changes are in the way. **Stashes** stashes, applies, pops and drops. The top bar has the
branch, ↑↓ against its upstream, **Fetch**, **Pull** (fast-forward only; otherwise it offers Merge or Rebase) and
**Push** (a branch with no upstream asks before publishing); while one runs, its button cancels it. During a merge, rebase, cherry-pick or revert a banner
offers Abort and, once nothing conflicts, Continue; a conflicted file has Use ours, Use theirs and Mark resolved.

Everything runs through your own `git`, in the background, without ever waiting for input: a command that fails
shows git's message, with **Details** and **Run in a terminal**, which opens a terminal beside the block with the
command typed in, for you to read and run, so your credential helper or SSH agent does what it does in any
terminal. The block refreshes
every 2 seconds while it is on screen, after each action, and when a terminal of the workspace finishes a command
in the repository, so `git add` in a terminal shows at once. Settings ▸ Tools names the `git` program.

**Generate ✨**, beside the summary, has an AI agent write the commit message: Claude Code (`claude -p`) and
Codex (`codex exec`) work as they are, any other agent once its generate command is set in Settings ▸ Agents.
It gets the diff of what you commit (cut past 60 KB, the list of files always whole), the last 10 commit
summaries and your rules (Settings ▸ Git, or a repository's own from the ⚙ beside it), and only fills the
summary and the description, for you to read and commit. A second click stops it, and a message you typed
meanwhile is kept. The first time, it asks before sending your diff to the agent's provider. Settings ▸ Git
chooses the agent and its model, and **Try it** runs it on a sample change.

## The toolbar

The bar at the bottom of the canvas holds the tools: **Select** and **Hand** (move the canvas by dragging anywhere,
over blocks too; Esc goes back to Select), then a button for each kind of block (terminal, note, kanban board, editor, Git) and
the AI agents, then the drawing tools.

## Drawing

The toolbar's pen, highlighter, shapes (line, arrow, rectangle, ellipse) and eraser draw on the canvas, over
the blocks. A pill above the toolbar picks the color (seven, from the theme) and the width. Esc goes back to
Select; then a click on a drawing selects it, a drag moves it, Delete removes it, and Ctrl+Z / Ctrl+Shift+Z
(Cmd on macOS) undo and redo while a drawing tool is on or a drawing is selected. Drawings are saved with the
canvas.

## Your canvas is kept

Templum saves every workspace as you go (every few seconds, and when it quits) and brings them back when it opens:
the view, every terminal's place and size, and a new shell in each terminal's last folder. What was on screen
doesn't come back. The files are in `~/.local/share/templum/workspaces/`
(`~/Library/Application Support/Templum/workspaces/` on macOS, `%APPDATA%\templum\workspaces\` on Windows): one
per workspace, and `index.json` listing them. If Templum can't read a file, it keeps it as `.bak` and starts that
canvas fresh.

If an AI agent was open in a terminal, Templum brings it back there: Claude Code with `claude --continue`, Codex
with `codex resume --last`, and so on for the 20 agents it knows, continuing that folder's latest conversation (or
a new one, for agents that can't resume by folder).

## AI agents

Right-click the canvas, **AI agent ▸**, and pick one: Claude Code, Codex, Gemini CLI, GitHub Copilot CLI, Cursor
CLI, OpenCode, Amp, Kiro, Qwen Code, Grok Build, Kimi Code, Mistral Vibe, Aider, Goose, Crush, Droid, Auggie, Kilo
Code, Cline or Pi. Templum asks which folder it runs in: one of this workspace's recent folders, a folder you choose,
or a path you type. It opens a terminal there and starts the agent; agents you haven't installed show their install
command instead. **Settings ▸ Agents** changes any agent's command or resume command (↺ puts it back), hides
the ones you don't use from the menu, and adds your own agents. The full list, with how each one resumes, is in [docs/specs/agents.md](docs/specs/agents.md).

A terminal running Claude Code or Codex shows its session in its header: how much of the context is used, and the
cost or the tokens. The title bar shows each agent's plan limits in turn (the 5-hour and weekly windows of a Claude
or ChatGPT plan; **Settings ▸ Usage** picks which agents); click it for every agent's limits, when each resets, and
the open sessions. Templum asks Claude Code for its limits every 5 minutes (its `/usage`, which costs nothing). Templum tells
you when a limit nears its end and when one you reached resets (**Settings ▸ Usage**). Claude Code hands its limits
to its status line, so the Claude Code Templum opens gets `--settings` naming Templum's: your own status line still
shows. Templum reads only what the agents write, never their login. See [docs/specs/usage.md](docs/specs/usage.md).

## Shell integration

Templum loads a small script into bash, zsh, fish and PowerShell (Git Bash and Windows PowerShell included), as
Ghostty, Kitty and VS Code do, so the shell tells it its folder and when each command starts and ends. Your shell and
its files stay as they are: they are read as usual. With it:

- the dot before a terminal's title shows a command running, and red when the last one failed;
- a terminal you aren't looking at that ends a command of 5 seconds or more, or asks for attention (a bell, or an
  agent's notification), gets a dot on its block and on its workspace's tile, and a notification; click it to go
  there. With Templum in the background it's also a system notification (Settings ▸ Terminal ▸ System
  notifications);
- new terminals open in the focused one's folder in PowerShell too;
- each command gets a dot in the terminal's left margin, green or red by how it ended, and **Ctrl+Shift+↑** /
  **Ctrl+Shift+↓** (**Cmd+↑** / **Cmd+↓** on macOS) jump to the previous or next command's prompt.

Codex rings the bell when it needs you. For Claude Code, set `"preferredNotifChannel": "terminal_bell"` in its
settings. Turn the integration off in Settings ▸ Shell. Details: [docs/specs/shell-integration.md](docs/specs/shell-integration.md).

## Configuration

Everything is in `config.toml`, in the folder from [Themes](#themes) (`~/.config/templum/` on Linux and macOS,
`%APPDATA%\templum\` on Windows). Press **Ctrl+,** (**Cmd+,** on macOS), or the ⚙ in the title bar, to open the
settings panel; every change applies at once and is saved to the file, comments included. Its **Edit config.toml**
button opens the file in your editor: if it doesn't exist yet, Templum writes it first with every option commented
out. Uncomment a line and save, and it applies at once.

```toml
[font]
family = "JetBrains Mono"
size = 14

[theme]
mode = "system"   # "system" | "light" | "dark"

[shell]
program = "fish"
args = ["--login"]
working_directory = "inherit"   # "inherit" (the focused terminal's folder) | "~" | a path
integration = true              # see Shell integration

[cursor]
shape = "block"   # "block" | "bar" | "underline"
blink = false

[terminal]
scrollback = 10000
notifications = true   # system notifications while Templum is in the background

[usage]
reset = "reached"      # when a plan limit's reset is notified: "off" | "reached" | "half" | "always"
warn = 90              # notify once at this much of a limit: 0 (never) | 80 | 90 | 95
claude_limits = true   # Templum asks Claude Code for its limits
hide = ["codex"]       # agents whose limits the title bar doesn't show

[git]
program = "git"   # the Git block's program: a name on PATH, or a path

[git.commit_message]   # Generate ✨: an AI agent writes the commit message
agent = "claude"       # an agent with a generate command (Settings ▸ Agents)
model = "haiku"
instructions = "Conventional Commits, English, imperative."

[keys]
new_terminal = "ctrl-shift-t"   # one shortcut, a list, or [] to disable
```

A mistake in the file (a typo, a value out of range) shows a notification saying where, and Templum keeps the
last working settings. On Windows, new terminals run Git Bash when Git for Windows is installed, else PowerShell 7,
else Windows PowerShell; Settings ▸ Shell lists every shell found (Git Bash, PowerShell 7, Windows PowerShell, WSL,
Nushell, Command Prompt). The full list is in [docs/specs/config.md](docs/specs/config.md).

## Pre-built binaries

Each release on the [releases page](https://github.com/KingDasWinx/Templum/releases) has, with a `.sha256` beside
every file:

- **macOS** (Apple Silicon and Intel): `.dmg`.
- **Windows** (x64): an installer (`-setup.exe` or `.msi`), or the portable `templum-x86_64-pc-windows-msvc.exe`.
- **Linux** (x86_64 and ARM64): `.deb`, AppImage, or the bare executable in `templum-<target>.tar.gz`; a pacman
  package and its `PKGBUILD` on x86_64.

What changed in each one is in [CHANGELOG.md](CHANGELOG.md). The builds are **unsigned** for now:

- **macOS:** right-click the app → *Open* → *Open* (only needed the first time).
- **Windows:** on the SmartScreen prompt, click *More info* → *Run anyway*.

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md).
Security issues: see [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). You may use, modify and sell Templum, as long as the copyright notice is kept.
