# Agents typed in a terminal: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `claude` or `codex` a person types at the prompt of a Templum terminal (bash, zsh, fish, PowerShell) gets
Templum's MCP server, hooks and status line, as the ones Templum opens do (kanban card 42).

**Architecture:** The words `typed()` appends after an agent's command are also built as raw arguments, and every new
terminal carries them in `TEMPLUM_CLAUDE_ARGS` and `TEMPLUM_CODEX_ARGS` (one word per line, empty when there is
nothing to give). The shell integration scripts define a `claude` and a `codex` function that put those words before
the person's. One private helper holds the rules (data folder, program, WSL, connections, cmd and Windows PowerShell),
so what is typed and what the variables carry never differ.

**Tech Stack:** Rust 2024; bash (3.2 up: macOS), zsh, fish, PowerShell 7 and 5.1; the real-shell PTY tests of
`templum-terminal/src/integration.rs` (`mod shells`, `Home`); headless GPUI tests (`workspace/tests/`).

**Spec:** [docs/specs/connections.md](../specs/connections.md#agents-typed-in-a-terminal) (read that section first:
the rules, what is left out and why), and [shell-integration.md](../specs/shell-integration.md).

## Global Constraints

- Variables: `TEMPLUM_CLAUDE_ARGS`, `TEMPLUM_CODEX_ARGS`. Values are raw words joined by `\n`, no shell quoting;
  Windows' `\` written `/`. **Both are set on every new terminal**, empty when there is nothing to give: an empty value
  stops one inherited from a Templum this one was started from (as `TEMPLUM_BUS` does).
- Claude Code's words: `--settings`, `<data>/claude-statusline.json`, then `--mcp-config=<data>/mcp.json` (with `=`,
  one word) only when `typed` would give `--mcp-config`. Codex's: `-c` and each value of `codex_flags`, unquoted.
- The function puts the words **before** the person's: `command claude <words> "$@"`.
- It adds nothing when the person's words hold `--settings`, `--settings=…`, `--mcp-config` or `--mcp-config=…`
  (Claude Code), or a word starting with `mcp_servers.templum.` (Codex); nor when its variable is empty at the call.
- Defined only when its variable is non-empty as the shell starts and there is no function of that name (zsh: defined
  in `.zshenv` after the user's `.zshenv`, so the `.zshrc`'s replaces it; PowerShell: no alias either). Use the
  `function` keyword in bash and zsh (an alias of the same name would break `claude() {`).
- bash code runs on bash 3.2: no `mapfile`, no `local -n`.
- Nothing on PATH, no file outside Templum's data folder, no new dependency, no `unsafe`; only `crates/templum` uses
  gpui-kit.
- Copy: the switch's label **Agents in Templum**; its description "Claude Code and Codex opened from the canvas or
  typed in a terminal get Templum's server and hooks: they read and type into the blocks wired to them";
  `config.toml`'s comment "The Claude Code and Codex Templum opens, or that are typed in its terminals, get its MCP
  server and hooks: they can read and type into the blocks wired to them (docs/specs/connections.md)."
- Each commit passes `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`,
  on `development`, titled as CLAUDE.md says. A task that changes a spec's behavior updates its lines in the same commit.
  No bench: nothing here paints or types into agents.

## Review Focus

1. **A path with a space in Templum's words** (macOS's `~/Library/Application Support/templum`): one argument in
   every shell. Tasks 1, 2 and 3 put one in their words.
2. **A Templum started from a Templum terminal** (`cargo run` in one): its terminals' variables are empty, never the
   first one's. Task 1 pins it.
3. **An alias named `claude` in the `.bashrc`**, read before the integration defines its function: bash still parses
   the script, and the alias's words come after Templum's. Task 2 pins it.
4. **The agent's exit status** reaches the shell (`claude; echo $?`, the prompt mark's color). Tasks 2 and 3 pin it
   with a fake that exits 3.
5. **`claude` with no argument**, the usual way: Templum's words only; and `claude ''` keeps the empty argument.
   Tasks 2 and 3 pin it.

---

### Task 1: The words as arguments, on every new terminal

**Files:**
- Modify: `crates/templum-mcp/src/lib.rs` (`codex_args`; `codex_flags` built from it)
- Modify: `crates/templum-usage/src/claude.rs` (`settings_file`, `mcp_file`; `settings_arg`, `mcp_arg` built on them)
- Modify: `crates/templum/src/workspace/usage.rs` (`claude_args`, `Gives`, `gives`, `agent_env`; `typed` uses `gives`)
- Modify: `crates/templum/src/workspace/blocks.rs` (`spawn_block`: both env arms get `self.agent_env()`)
- Test: `crates/templum-mcp/src/lib.rs`, `crates/templum-usage/src/claude.rs`, `crates/templum/src/workspace/tests/usage.rs`,
  `crates/templum/src/workspace/tests/bus.rs`

**Interfaces:**
- Produces:
  - `pub fn templum_mcp::codex_args(exe: &str, windows: bool, hooks: bool) -> Option<Vec<String>>`: `-c`, value,
    `-c`, value…, the values `codex_flags` single-quotes; same `None` rule. `codex_flags` becomes these words, each
    value as ` -c '<value>'`.
  - `pub fn templum_usage::claude::settings_file(data: &Path, exe: &Path, windows: bool, status_line: bool, hooks: bool) -> Option<PathBuf>`
    and `pub fn mcp_file(data: &Path, exe: &Path, windows: bool) -> Option<PathBuf>`: what `settings_arg` and
    `mcp_arg` do today (write the file when it changed; `None` under the same conditions, a path `typed_path` refuses
    included), returning the file's path. `settings_arg`/`mcp_arg` = `typed_path` of it.
  - `pub(super) fn claude_args(data: &Path, exe: &Path, windows: bool, limits: bool, connected: bool) -> Vec<String>`
    in `workspace/usage.rs`, beside `claude_flags`, same conditions.
  - `struct Gives { data: PathBuf, exe: PathBuf, connected: bool, codex: bool, on_path: bool }` and
    `fn gives(&self) -> Option<Gives>` (private to `usage.rs`): `None` without a data folder or a program, or under WSL;
    `connected` = `[connections] enabled` and a bus; `codex` = `connected` and the shell is neither cmd nor
    powershell; `on_path` as `typed` computes it. `typed` keeps its output, now through `gives`.
  - `pub(super) fn agent_env(&self) -> Vec<(String, String)>` on `Workspace`: always both variables, in this order:
    `("TEMPLUM_CLAUDE_ARGS", claude_args(..).join("\n"))`, `("TEMPLUM_CODEX_ARGS", codex_args(..).join("\n"))`
    (`""` when `gives` is `None`, or `!codex`, or `codex_args` is `None`).

- [ ] **Step 1: Write the failing tests**
  - `templum-mcp` `codex_args_are_the_flags_unquoted`: `codex_args("/opt/Templum App/templum", false, true)` has 20
    words, every even one `-c`; word 1 is `mcp_servers.templum.command="/opt/Templum App/templum"`; the last is
    `hooks.Interrupt=[{hooks=[{type="command",command="templum hook codex",timeout=3}]}]`; with `hooks` false, 8
    words; `/home/o'neil/templum` gives `None`. The existing `codex_gets_the_server_and_its_hooks_in_single_quotes`
    stays as it is.
  - `templum-usage` `settings_file_is_the_path_unquoted`: in a temp folder joined with `Templum Data` (created),
    `settings_file(.., true, true)` is `<that>/claude-statusline.json` and the file holds what `settings_arg` wrote;
    `settings_arg` gives the same path in double quotes; `mcp_file` likewise `<that>/mcp.json`.
  - `workspace/tests/usage.rs` `new_terminals_carry_what_templum_types_after_claude_and_codex`, with the data folder
    `<tempdir>/Templum Data` (`open_in(cx, None, Some(..))`); `word(k)` = that variable's value in `ws.agent_env()`;
    `p(name)` = the data folder joined with name, as a string with `\` as `/` on Windows:
    - `word("TEMPLUM_CLAUDE_ARGS") == format!("--settings\n{}\n--mcp-config={}", p("claude-statusline.json"), p("mcp.json"))`;
    - on Unix, `word("TEMPLUM_CODEX_ARGS") == codex_args(&program().unwrap().to_string_lossy(), false, true).unwrap().join("\n")`;
    - `config.connections.enabled = false`: Codex's `""`, Claude Code's `"--settings\n" + p("claude-statusline.json")`;
      then also `config.usage.claude_limits = false`: both `""`;
    - back on, shell `powershell`, then `CMD.EXE`: Codex's `""`, Claude Code's as in the first line; `WSL.exe`: both `""`;
    - `open_in(cx, None, None)` (no data folder): both variables present and `""`.
  - `workspace/tests/bus.rs`: `without_a_bus_terminals_name_none`'s script also prints
    `claude=%s;` of `${TEMPLUM_CLAUDE_ARGS-unset}` and the screen shows `claude=;`; 
    `terminals_get_the_bus_and_this_templum_first_on_their_path` also prints the first line of
    `TEMPLUM_CLAUDE_ARGS` (`${TEMPLUM_CLAUDE_ARGS%%<newline>*}`) and the screen shows `--settings`.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p templum-mcp codex_args && cargo test -p templum-usage settings_file && cargo test -p templum new_terminals_carry && cargo test -p templum terminals_get_the_bus without_a_bus`
Expected: FAIL (the functions don't exist; the variables aren't set).

- [ ] **Step 3: Implement the interfaces above.** `claude_args` turns the paths into strings with `\` as `/` on
  Windows. `spawn_block` extends both arms of its `env` (the bus's, and the empty `TEMPLUM_BUS` one) with
  `self.agent_env()`. Doc comments say why both variables are always set.

- [ ] **Step 4: Run the tests of Step 2, then the full check command.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/templum-mcp/src/lib.rs crates/templum-usage/src/claude.rs crates/templum/src/workspace/usage.rs \
  crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/tests/usage.rs crates/templum/src/workspace/tests/bus.rs
git commit -m "refactor(connections): build the words typed after claude and codex as arguments, and give them to new terminals"
```

---

### Task 2: `claude` and `codex` in bash and zsh

**Files:**
- Modify: `crates/templum-terminal/src/integration/templum.bash` (inside the `__templum_installed` block)
- Modify: `crates/templum-terminal/src/integration/zsh/.zshenv` (inside its interactive block)
- Modify: `crates/templum/src/settings/connections.rs:46-49` (label and description), `crates/templum/src/config.rs`
  (the `[connections] enabled` doc comment and the template's two comment lines), and any test naming the old label
- Modify: `docs/specs/settings.md:130` (the row), `docs/specs/connections.md` (the Config block's comment)
- Test: `crates/templum-terminal/src/integration.rs` (`mod shells`)

**Interfaces:**
- Consumes: the variables of Task 1 (the tests set them by hand).
- Produces, for Task 3's tests: in `mod shells`,
  - `Home::spawn_with(&self, program: &str, a: &[&str], env: &[(&str, &str)]) -> Option<Terminal>` (`spawn` calls it
    with none; `env` goes on the `env` command line). It always sets `TEMPLUM_CLAUDE_ARGS` and `TEMPLUM_CODEX_ARGS`,
    to `""` unless `env` gives them: `cargo test` run in a Templum terminal would hand the shell its own;
  - `Home::fake_agents(&self) -> String`: writes `<home>/bin/claude` and `<home>/bin/codex`, executable `sh` scripts
    that write each argument on its own line to `<home>/<name>.args` and exit 3, and returns `PATH=<home>/bin:<PATH>`'s
    value;
  - `fn wait_args(term: &Terminal, home: &Home, name: &str, want: &[&str])`: waits (the module's `wait`) until
    `<home>/<name>.args` holds exactly `want`, one per line, then deletes it;
  - `const CLAUDE_WORDS: &str = "--settings\n/tmp/Templum Data/claude-statusline.json\n--mcp-config=/tmp/Templum Data/mcp.json"`
    and `const CODEX_WORDS: &str = "-c\nmcp_servers.templum.command=\"/opt/templum\"\n-c\nhooks.Stop=[{hooks=[{type=\"command\",command=\"templum hook codex\",timeout=5}]}]"`;
  - `fn typed_agents(term: &Terminal, home: &Home, unset: &str, status: bool)`: the plain session below, `unset`
    being the shell's line that empties the variable (`unset TEMPLUM_CLAUDE_ARGS`; Task 3 passes fish's and
    PowerShell's), `status` whether to check the exit code (bash: `bash_has_ps0()`, as `check` does; 3.2 has no PS0).
    It first waits for the startup folder to be reported, as `check` does, and types its first line with
    `wait_pressing(.., Some(b"\r"))`: PowerShell can drop the first Enter.

- [ ] **Step 1: Write the failing tests** (each shell skipped when not installed, as the module does). `W(s)` = the
  words of `s`, one per line. Typed lines end with `\r`.

  `typed_agents`, in a shell started with `fake_agents()`'s PATH, `TEMPLUM_CLAUDE_ARGS=CLAUDE_WORDS`,
  `TEMPLUM_CODEX_ARGS=CODEX_WORDS`, and no startup file of the person's but the `cd`:

  | Typed | `claude.args` / `codex.args` |
  |---|---|
  | `claude` | `W(CLAUDE_WORDS)`, and with `status` the command's end has code 3 (`term.status().last`) |
  | `claude hi ''` | `W(CLAUDE_WORDS)`, `hi`, `` (an empty line) |
  | `claude --settings x` | `--settings`, `x` |
  | `claude --mcp-config=y hi` | `--mcp-config=y`, `hi` |
  | `codex resume --last` | `W(CODEX_WORDS)`, `resume`, `--last` |
  | `codex -c 'mcp_servers.templum.command="z"'` | `-c`, `mcp_servers.templum.command="z"` |
  | the `unset` line, then `claude hi` | `hi` |

  Tests (in `mod shells`):
  - `bash_gives_typed_agents_templums_words` and `zsh_gives_typed_agents_templums_words`: `typed_agents` (zsh with
    `status` true).
  - `bash_keeps_the_persons_alias_and_function`: `.bashrc` has `alias claude='claude --model opus'` and
    `codex() { command codex mine "$@"; }` before the `cd`; `claude hi` gives `W(CLAUDE_WORDS)`, `--model`, `opus`,
    `hi`; `codex hi` gives `mine`, `hi`.
  - `zsh_keeps_the_persons_alias_and_function`: the same lines in `.zshrc`, same results.
  - `bash_without_the_variables_has_no_function`: started without the two variables; `claude hi` gives `hi`, and
    `codex hi` gives `hi`.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p templum-terminal typed_agents keeps_the_persons without_the_variables`
Expected: FAIL (the args files lack Templum's words). A shell that isn't installed is skipped and proves nothing:
install the missing ones without sudo first, into a temporary folder put first on `PATH` for the run (zsh: `brew
install zsh`, Homebrew is on this machine's PATH; fish: its static Linux tarball; pwsh: its GitHub release,
`powershell-<version>-linux-x64.tar.gz`), and say in the commit message which shells ran.

- [ ] **Step 3: Write the functions.** bash: after the startup files, for each of `claude` and `codex`, define it
  with the `function` keyword when its variable is non-empty and `declare -F` finds no function of that name. zsh:
  the same in the interactive block, with `$+functions[claude]`. At each call: read the variable; empty, or a word of
  the person's that the Global Constraints' rule names, run `command <name> "$@"`; else split the variable on `\n`
  (bash: `IFS=$'\n' builtin read -r -d '' -a words <<< "$w"`; zsh: `"${(@f)…}"`) and run
  `command <name> "${words[@]}" "$@"`. Quote the variable in every assignment: `builtin local w="${TEMPLUM_CLAUDE_ARGS-}"`
  (after `builtin`, bash splits an unquoted one into `local`'s arguments; seen while planning). A comment at the
  top of each block names `docs/specs/connections.md` ("Agents typed in a terminal").

- [ ] **Step 4: The switch's words.** Label, description and comments as the Global Constraints give them, in
  `settings/connections.rs`, `config.rs`, `docs/specs/settings.md` and connections.md's Config block.

- [ ] **Step 5: Run the tests of Step 2, then the full check command.** Expected: PASS (no shell skipped on this
  machine but those not installed).

- [ ] **Step 6: Commit**

```bash
git add crates/templum-terminal/src/integration/templum.bash crates/templum-terminal/src/integration/zsh/.zshenv \
  crates/templum-terminal/src/integration.rs crates/templum/src/settings/connections.rs crates/templum/src/config.rs \
  docs/specs/settings.md docs/specs/connections.md
git commit -m "feat(connections): a claude or codex typed in a bash or zsh terminal gets Templum's server and hooks"
```

---

### Task 3: fish and PowerShell, and the docs

**Files:**
- Modify: `crates/templum-terminal/src/integration/templum.fish`, `crates/templum-terminal/src/integration/templum.ps1`
- Modify: `docs/specs/connections.md` (drop "planned" and "yet": Status, criterion 10, the line after Codex's typing
  rules, the section's first line becomes "Implemented: …", Testing's heading), `docs/specs/shell-integration.md` (drop
  "planned"), `docs/ARCHITECTURE.md` (Configuration: `[connections] enabled` covers the agents typed in its terminals;
  `templum-terminal`'s integration bullet names the two functions)
- Test: `crates/templum-terminal/src/integration.rs` (`mod shells`)

**Interfaces:**
- Consumes: Task 2's `spawn_with`, `fake_agents`, `wait_args`, `typed_agents`, `CLAUDE_WORDS`, `CODEX_WORDS`.

- [ ] **Step 1: Write the failing tests**
  - `fish_gives_typed_agents_templums_words`: `typed_agents` with `set -e TEMPLUM_CLAUDE_ARGS`, `status` true.
  - `powershell_gives_typed_agents_templums_words` (`pwsh -NoLogo`): `typed_agents` with
    `Remove-Item Env:TEMPLUM_CLAUDE_ARGS`, `status` true; the `claude hi ''` line keeps its empty argument as
    `claude hi ""`. The code 3 is the point of `$Global:__TemplumAgentExit` below: without it the prompt says 0.
  - `fish_keeps_the_persons_function`: `config.fish` has `function codex; command codex mine $argv; end` and
    `alias claude 'claude --model opus'`; `codex hi` gives `mine`, `hi`; `claude hi` gives `--model`, `opus`, `hi`
    (fish's alias is a function: the person's wins).
  - `powershell_keeps_the_persons_function_and_alias`: the profile has `function codex { & (Get-Command codex
    -CommandType Application | Select-Object -First 1) mine @args }` and `Set-Alias claude <home>/bin/claude`;
    `codex hi` gives `mine`, `hi`; `claude hi` gives `hi`.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p templum-terminal fish_ powershell_`
Expected: FAIL, with fish and pwsh installed as Task 2's Step 2 says (a skipped shell proves nothing).

- [ ] **Step 3: Write the functions.** fish: in the interactive block, `function claude --wraps claude` when
  `$TEMPLUM_CLAUDE_ARGS` is non-empty and `functions -q claude` is false; words by `string split \n --`; the
  program by `command claude`. PowerShell: `function Global:claude` when `$env:TEMPLUM_CLAUDE_ARGS` is non-empty and
  neither `Function:\claude` nor `Alias:\claude` exists; the program is `Get-Command claude -CommandType
  Application, ExternalScript | Select-Object -First 1` (several on PATH give an array, which `&` can't run); words by
  `-split "`n"` without empty ones; `& $program @words @args`. Same rules for `codex`, and the Global Constraints' rule
  for adding nothing. A function's caller sees `$?` true whatever the program's exit code, so the function then sets
  `$Global:__TemplumAgentExit = $LASTEXITCODE`, and the wrapped `prompt`, when it is set, reports it as the command's
  code (0 or not) and clears it.

- [ ] **Step 4: The docs** listed under Files.

- [ ] **Step 5: Run the tests of Step 2, then the full check command.** Expected: PASS.

- [ ] **Step 6: Check by hand** (Linux, `cargo run -p templum -- --dev`): in a new bash terminal, `claude`: `/mcp`
  lists `templum` connected, the dot turns accent on a prompt and back at its end, another agent's `delegate` to it
  answers; `codex`: `/mcp` lists it, Codex asks once to trust the hooks; `claude mcp list` and `codex resume --last`
  still work; `command claude`: no `templum` in `/mcp`. Say in the commit message what was checked; Windows (Git Bash,
  pwsh 7, Windows PowerShell) and macOS stay unchecked, as the other connection checks.

- [ ] **Step 7: Commit**

```bash
git add crates/templum-terminal/src/integration/templum.fish crates/templum-terminal/src/integration/templum.ps1 \
  crates/templum-terminal/src/integration.rs docs/specs/connections.md docs/specs/shell-integration.md docs/ARCHITECTURE.md
git commit -m "feat(connections): a claude or codex typed in a fish or PowerShell terminal gets Templum's server and hooks"
```
