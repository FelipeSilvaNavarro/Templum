# Connections, batch 6 (Control): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

## Context

Batches 1 to 5 of [docs/specs/connections.md](../specs/connections.md) are on `development` (draft PR #36 dev→main);
9882cb8 specified batches 6 to 8 (orchestration). Batch 6 gives an agent control over the agents it works with: it
presses real keys in a wired terminal (a dialog's answer), stops an agent it handed a task to, closes the blocks it
opened, and starts an agent on a model and an effort it picks; `send_to_terminal`'s Enter goes a moment after its
text. This plan fills the spec's one open point (Codex's efforts, Decision 1) and names where the code made it
deviate (Decisions 2 to 6); Task 11 writes them into the spec.

**Goal:** `send_keys`, `interrupt`, `close_block` (tools and `templum keys`, `interrupt`, `close`); `spawn_agent`'s
`model` and `effort` (`templum spawn --model --effort`), kept by a terminal item with `opened_by` and resumed with its
agent; "answered by" on a terminal that got another block's keys; `send_to_terminal`'s Enter 150 ms after its text.

**Architecture:** `templum-terminal` names keys (`input::named_key`) and presses them (`Terminal::press`: the key's
bytes in the program's mode, else its character raw, never a paste), and knows each agent's model and effort flags
(`Agent::launch_args`). `templum-mcp` parses the three new calls and the verbs. In the app, `delegate.rs` gets
`send_keys` and `interrupt` (a key every 50 ms on a spawned task, answering through `Outcome::Later`), `spawn.rs` gets
`close_block` (the undoable `close_now`) and passes the flags to `start_agent`; `Pane` keeps `opened_by`, `model`,
`effort` and `keys_from`, saved through `Pane::item` into `session::TerminalItem` and back through
`restore_terminal`; `block.rs` draws "answered by".

**Tech Stack:** Rust 2024; GPUI (gpui-fast 0.1.8) through gpui-kit `=0.7.3` (no new GPUI API: `cx.spawn`,
`background_executor().timer`, `WeakEntity::update` as `send_task` uses them, `window.try_find` in tests); serde;
`alacritty_terminal`'s `TermMode`. Agents checked on 2026-10-16: Claude Code 2.1.286 (`claude --help`), Codex 0.154.0
(`codex --help`, `codex resume --help`, `codex debug models`), Gemini CLI 0.63.0 (`gemini --help`).

**Spec:** [docs/specs/connections.md](../specs/connections.md) at 9882cb8: "Keys, stopping and closing", the
`spawn_agent` model/effort paragraph and table in "Spawning", the Tools rows `spawn_agent`, `send_keys`, `interrupt`,
`close_block`, the command-line rows `spawn`, `keys`, `interrupt`, `close`, Files ("Terminal items (batch 6)"),
Architecture (batch 6 rows), Testing "Batch 6", Delivery item 6, Risks (keys, a model typed into a shell).

## Global Constraints

- Only `crates/templum` depends on gpui-kit; `templum-terminal` and `templum-mcp` stay UI-free and are tested
  headlessly. No new dependency. No `unsafe`. Every logic change ships a test that fails without it.
- Keys, verbatim: "`enter`, `escape`, `tab`, `shift+tab`, `backspace`, `delete`, `up`, `down`, `left`, `right`,
  `home`, `end`, `page_up`, `page_down`, `space`, `ctrl+<letter>`, or one printable character (`2`, `y`). At most 16 a
  call; a name it doesn't know refuses the call before any key goes." "one at a time and 50 ms apart"; "never as a
  paste"; "The result ends with the last 20 lines of the screen, 300 ms after the last key."
- "Refused: the caller's own block, and an agent on another agent's task unless it waits for its person." "the block
  shows **answered by** … after its dot for 10 seconds, and in its dot's tooltip".
- `interrupt`: "presses Esc once … while it works or waits for its person … its waits get "Interrupted by you."
  Refused while it is idle, and while it is on another agent's task."
- `close_block`: "closes a block the caller opened with `spawn_agent` and is still wired to … without asking the
  person, as one step of Ctrl+Z … Any other: "Only the person closes k9x1d: you didn't open it.""
- "`send_to_terminal`'s Enter goes 150 ms after its text, as a task's does" (`delegate::ENTER_AFTER`).
- Flags: Claude Code `--model <m>`, `--effort <e>` (`low`, `medium`, `high`, `xhigh`, `max`); Codex `-m <m>`,
  `-c model_reasoning_effort=<e>` (Decision 1); Gemini CLI `-m <m>`, no effort. "typed after the agent's command and
  before what Templum adds (Claude Code's `--mcp-config` takes every word after it)". "a model is 1 to 64 letters,
  digits, `.`, `-`, `_`, `:` and `/`, beginning with a letter or a digit … an effort is one of its agent's. Anything
  else, an effort for Gemini CLI, or either for another agent is refused before anything opens, a worktree included."
  "The result names them ("…, connected to you, on gpt-5.6-luna (high).")".
- Files: "Terminal items (batch 6) also keep `opened_by` (the block id of the agent that spawned it), `model` and
  `effort`… A pasted block has none; an older file loads with none; an older Templum ignores them."
- Commits on `development` (never `main`), small, Conventional Commits; `feat`/`fix` titles are for a person using
  Templum. End each message with the executing session's `Claude-Session:` line when it has one (batches 1-5 did).
- Before saying done: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`, `cargo deny check`; this batch touches `templum-terminal` and painting (`block.rs`), so `--bench` and
  `--bench-mixed` run too: `p95_ms` ≤ 16.7 each.

## Decisions

1. **Codex's efforts are `low`, `medium`, `high`, `xhigh`, `max`, `ultra`, typed unquoted:
   `-c model_reasoning_effort=high`.** Codex 0.154.0's model catalog (`codex debug models` with a scratch
   `CODEX_HOME`, which starts no session) lists `low`…`ultra` for gpt-6-astra, gpt-5.6-sol and gpt-5.6-terra,
   `low`…`max` for gpt-5.6-luna, `low`…`xhigh` for gpt-5.5, gpt-5.4 and gpt-5.2; `~/.codex/models_cache.json` agrees.
   Codex itself takes any string there (`codex debug prompt-input -c model_reasoning_effort=bogus` passes; `=[1]`
   fails "invalid type: sequence, expected a string"), so Templum's list is the only check. A `-c` value is parsed as
   TOML and a bare word that isn't TOML falls back to a string (`codex --help`); none of the six is a TOML literal
   (`true`, `false`, `inf`, `nan`), so no quotes: it types the same in every shell, `cmd.exe` and PowerShell 5.1
   included, where the MCP flags' quotes don't. Templum doesn't check an effort against the model (`ultra` for
   gpt-5.6-luna is typed): a minor (Task 11).
2. **"answered by" follows any keys another block sends**, not only those into an agent Templum knows waits for its
   person: Codex's trust questions (the spec's manual check answers one with `send_keys ["2"]`) tell no hook. It names
   the caller as its `[from …]` line does: *answered by Claude Code (b7k2m)*. Every key also sets the pane's `typed`,
   so the screen's looks take it as the person's (`activity.rs`): the yellow ends once its screen moves.
3. **`interrupt` drops a spawned agent's prompt still waiting** when its task is the caller's: the prompt is that
   task, and typed later it would start work no one awaits.
4. **`close_block` on a block the caller opened but is no longer wired to** answers as `target` answers any block not
   connected ("isn't connected to you. Ask the person with connect(…)"): the person's Allow gives the consent back.
5. **A block keeps its model and effort while its agent runs there:** `Space::see` drops them when a snapshot finds
   the agent it found before gone, or another agent there (a `claude` typed by hand after the spawned Codex quit must
   not resume with Codex's `-m`). They are set back on restore only when the agent resumes, and only when
   `launch_args` takes them (a hand-edited file).
6. **`start_agent` takes the flags as a new `args: &str`**, appended to the agent's start command before `typed()`'s
   flags; it then has 8 parameters, so it gets `#[allow(clippy::too_many_arguments)]`, as `grid.rs` and `paint.rs`
   do. `open_agent` takes the whole `&tools::Spawn` instead of its prompt (staying at 7).
7. Key names are checked in the app (`templum-mcp` has no `templum-terminal`): the tool's parse checks only 1 to 16
   strings. Names are lowercase as listed; `ctrl+<letter>` is `a`–`z`. Left out (YAGNI): Alt, function keys, uppercase
   aliases, a per-model effort check.

## Review Focus

1. A model carrying shell syntax (`opus;rm -rf ~`, `$(x)`, a space, a quote, a leading `-`, `opus[1m]`), or an effort
   its agent doesn't take: refused before anything opens, a worktree included. (Task 2
   `a_model_and_an_effort_become_each_agents_flags`; Task 8 `spawn_agent_types_its_model_and_effort_first` checks no
   block and no `.worktrees`.)
2. A hand-edited workspace file whose model is unsafe: its agent resumes without it, and the block keeps none. (Task 7,
   the `ccc-3` item of `a_saved_agent_is_resumed_in_its_block`.)
3. A `claude` typed by hand where a spawned Codex quit: no Codex `-m` at the next restart. (Task 7
   `an_agent_that_left_takes_its_model_and_effort_with_it`.)
4. A block closed between two keys: the call ends with an error, no panic, no key into another block. (Task 4
   `send_keys_refuses_its_own_block_an_unknown_key_and_another_agents_task`, its last part.)
5. Claude Code and Codex run the kitty keyboard: Esc must be `ESC[27u` there and a character stays text; with
   application cursor keys, Up is `ESC O A`. (Task 1 `named_keys_send_what_a_keyboard_would`.)

## File Structure

| File | Change |
|---|---|
| `crates/templum-terminal/src/input.rs`, `terminal.rs`, `lib.rs` | `named_key`, `KEY_NAMES`; `Terminal::press` |
| `crates/templum-terminal/src/agent.rs` | `LAUNCH`, `Agent::launch_args` |
| `crates/templum/src/agents.rs` | `AgentEntry::launch_args` |
| `crates/templum-mcp/src/tools.rs`, `cli.rs` | `Call::SendKeys`, `Interrupt`, `CloseBlock`, `Spawn.model/effort`, `MAX_KEYS`; verbs `keys`, `interrupt`, `close`, `spawn --model/--effort` |
| `crates/templum/src/workspace/delegate.rs` | `send_keys`, `press_keys`, `signature`, `interrupt`, `ANSWERED_FOR`, `Pane::answered_by` |
| `crates/templum/src/workspace/bus.rs` | the three calls; `send_to_terminal`'s later Enter |
| `crates/templum/src/workspace/mod.rs`, `blocks.rs` | `Pane.opened_by/model/effort/keys_from`, `Pane::item`, `Space::layout`, `Space::see`; `start_agent`'s `args` |
| `crates/templum/src/session.rs`, `workspace/persist.rs`, `workspace/selection.rs` | `TerminalItem.opened_by/model/effort`, `LayoutBlock::Terminal(TerminalItem, Processes)`; `restore_terminal`; a paste is opened by no one |
| `crates/templum/src/workspace/spawn.rs` | model and effort; `close_block` |
| `crates/templum/src/block.rs` | the "answered by" chip and tooltip |
| tests: `templum-terminal` and `templum-mcp` units, `templum/src/agents.rs`, `session.rs`, `workspace/tests/{delegate,spawn,bus,agents,selection,workspaces}.rs` | per task |
| `docs/specs/connections.md`, `docs/ARCHITECTURE.md`, `docs/minors.md` | Tasks 6, 11 |

Before Task 1: `git status` is clean on `development`, at this plan's commit or later.

---

### Task 1: Named keys, pressed as a keyboard presses them (`templum-terminal`)

**Files:**
- Modify: `crates/templum-terminal/src/input.rs` (after `encode_key`, :20-109; test after
  `plain_keys_encode_like_xterm`, :318)
- Modify: `crates/templum-terminal/src/terminal.rs:770-780` (`press` after `key`)
- Modify: `crates/templum-terminal/src/lib.rs:18` (export `named_key`, `KEY_NAMES`)

**Interfaces:**
- Produces: `pub fn named_key(name: &str) -> Option<KeyInput<'_>>`; `pub const KEY_NAMES: &str = "enter, escape,
  tab, shift+tab, backspace, delete, up, down, left, right, home, end, page_up, page_down, space, ctrl+<letter>, or
  one character (2, y)"`; `Terminal::press(&self, key: &KeyInput)`.

- [ ] **Step 1: Write the failing test** in `input.rs`'s tests:

```rust
    /// The keys an agent names (`send_keys`) send what the person's keyboard would, in the
    /// program's mode; a character is text, typed as it is.
    #[test]
    fn named_keys_send_what_a_keyboard_would() {
        let bytes = |name: &str, mode| encode_key(&named_key(name).unwrap(), mode);
        let (plain, kitty) = (TermMode::empty(), TermMode::DISAMBIGUATE_ESC_CODES);
        assert_eq!(bytes("escape", plain).unwrap(), b"\x1b");
        assert_eq!(bytes("escape", kitty).unwrap(), b"\x1b[27u", "Claude Code, Codex");
        assert_eq!(bytes("up", plain).unwrap(), b"\x1b[A");
        assert_eq!(bytes("up", TermMode::APP_CURSOR).unwrap(), b"\x1bOA");
        assert_eq!(bytes("page_up", plain).unwrap(), b"\x1b[5~");
        assert_eq!(bytes("page_down", plain).unwrap(), b"\x1b[6~");
        assert_eq!(bytes("shift+tab", plain).unwrap(), b"\x1b[Z");
        assert_eq!(bytes("ctrl+c", plain).unwrap(), [3]);
        assert_eq!(bytes("ctrl+c", kitty).unwrap(), b"\x1b[99;5u");
        assert_eq!(bytes("enter", kitty).unwrap(), b"\r");
        assert_eq!(bytes("backspace", plain).unwrap(), b"\x7f");
        assert_eq!(bytes("delete", plain).unwrap(), b"\x1b[3~");
        for text in ["2", "y", "space"] {
            assert_eq!(bytes(text, kitty), None, "{text} is text");
        }
        assert_eq!(named_key("2").unwrap().key_char, Some("2"));
        assert_eq!(named_key("space").unwrap().key_char, Some(" "));
        for bad in ["foo", "ab", "", "ctrl+é", "ctrl+1", "Enter", "\u{1b}", "\n"] {
            assert!(named_key(bad).is_none(), "{bad:?}");
        }
    }
```

- [ ] **Step 2:** `cargo test -p templum-terminal named_keys`. Expected: FAIL, `named_key` not found.
- [ ] **Step 3: Implement `named_key`** (doc: "A key as `send_keys` names it (`KEY_NAMES`), as GPUI would report it;
  None for any other name"):

| Name | `KeyInput` |
|---|---|
| `enter`, `escape`, `tab`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end` | `key` = the name |
| `page_up`, `page_down` | `key` = `pageup`, `pagedown` (GPUI's names, which `encode_key` matches) |
| `shift+tab` | `tab`, `shift` |
| `space` | `space`, `key_char: Some(" ")` |
| `ctrl+` and one of `a`–`z` | that letter (a slice of `name`), `ctrl` |
| one character that isn't a control character | `key` and `key_char` = `name` |
| anything else | None |

  `Terminal::press` (doc: "Presses `key` as the keyboard would: its bytes in the program's mode (`key`), else the
  character it types, raw: never as a paste, which a dialog would ignore"): `key(key)`, and when that returns false,
  `write` its `key_char`'s bytes. It has no test here (it needs a PTY): Task 4's `send_keys_presses_keys_not_a_paste`
  fails without it.
- [ ] **Step 4:** `cargo test -p templum-terminal`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum-terminal/src/input.rs crates/templum-terminal/src/terminal.rs crates/templum-terminal/src/lib.rs
git commit -m "feat(terminal): name the keys an agent may press, sent as the keyboard sends them in the program's mode"
```

---

### Task 2: Each agent's model and effort flags

**Files:**
- Modify: `crates/templum-terminal/src/agent.rs` (`LAUNCH` after `SPECS`, :253; `launch_args` in `impl Agent`, :270;
  test in `tests`, :492)
- Modify: `crates/templum/src/agents.rs` (`AgentEntry::launch_args` after `resume_command_for`, :65-77; test after
  `an_agent_that_takes_a_session_id_is_started_on_one`, :467)

**Interfaces:**
- Produces: `Agent::launch_args(self, model: Option<&str>, effort: Option<&str>) -> Result<String, String>`: the
  words to append to its command, each after a space (`" --model opus --effort high"`), empty for neither; Err a
  sentence for the agent. `AgentEntry::launch_args(&self, model: Option<&str>, effort: Option<&str>) ->
  Result<String, String>`: its builtin's; for the person's own agent (`builtin` None) Ok("") with neither, else Err.

- [ ] **Step 1: Write the failing tests.** In `agent.rs`'s tests:

```rust
    #[test]
    fn a_model_and_an_effort_become_each_agents_flags() {
        let agent = |id| Agent::from_name(id).unwrap();
        let (claude, codex, gemini) = (agent("claude"), agent("codex"), agent("gemini"));
        let ok = |s: &str| -> Result<String, String> { Ok(s.to_string()) };
        assert_eq!(
            claude.launch_args(Some("opus"), Some("high")),
            ok(" --model opus --effort high")
        );
        assert_eq!(
            codex.launch_args(Some("gpt-5.6-luna"), Some("ultra")),
            ok(" -m gpt-5.6-luna -c model_reasoning_effort=ultra")
        );
        assert_eq!(gemini.launch_args(Some("gemini-2.5-pro"), None), ok(" -m gemini-2.5-pro"));
        assert_eq!(claude.launch_args(None, Some("max")), ok(" --effort max"));
        assert_eq!(codex.launch_args(Some("openrouter/qwen-3:free"), None), ok(" -m openrouter/qwen-3:free"));
        assert_eq!(agent("copilot").launch_args(None, None), ok(""));
        let longest = "a".repeat(64);
        assert!(claude.launch_args(Some(&longest), None).is_ok());
        let too_long = "a".repeat(65);
        for bad in ["opus;rm -rf ~", "$(x)", "a b", "'", "-rf", "", "opus[1m]", "~/m", too_long.as_str()] {
            let err = claude.launch_args(Some(bad), None).unwrap_err();
            assert!(err.contains("letters, digits"), "{bad:?}: {err}");
        }
        let err = claude.launch_args(None, Some("ultra")).unwrap_err();
        assert!(err.contains("low, medium, high, xhigh or max"), "Codex's, not Claude Code's: {err}");
        assert!(codex.launch_args(None, Some("HIGH")).is_err());
        let err = gemini.launch_args(None, Some("high")).unwrap_err();
        assert!(err.contains("no effort"), "{err}");
        let err = agent("copilot").launch_args(Some("gpt-5"), None).unwrap_err();
        assert!(err.contains("a model or an effort"), "{err}");
    }
```

  In `crates/templum/src/agents.rs`'s tests:

```rust
    #[test]
    fn only_ours_take_a_model_and_an_effort() {
        let list = agents("[agents]\nmine = { name = \"Mine\", command = \"mine\" }\n");
        assert_eq!(
            by(&list, "claude").launch_args(Some("opus"), None),
            Ok(" --model opus".to_string())
        );
        assert_eq!(by(&list, "mine").launch_args(None, None), Ok(String::new()));
        let err = by(&list, "mine").launch_args(Some("m"), None).unwrap_err();
        assert!(err.contains("Mine"), "{err}");
    }
```

- [ ] **Step 2:** `cargo test -p templum-terminal launch && cargo test -p templum only_ours_take`. Expected: FAIL,
  `launch_args` not found.
- [ ] **Step 3: Implement.** `LAUNCH`: one row per agent that takes them, by id: its model flag, its effort flag and
  its efforts, with the date and versions they were checked (Tech Stack):

| id | model | effort | efforts |
|---|---|---|---|
| `claude` | `--model <m>` | `--effort <e>` | `low`, `medium`, `high`, `xhigh`, `max` |
| `codex` | `-m <m>` | `-c model_reasoning_effort=<e>` | `low`, `medium`, `high`, `xhigh`, `max`, `ultra` |
| `gemini` | `-m <m>` | none | none |

  `launch_args`: neither → Ok(""); an agent with no row → Err("Templum doesn't give {title} a model or an effort.");
  a model that isn't 1 to 64 ASCII letters, digits, `.`, `-`, `_`, `:`, `/` beginning with a letter or a digit →
  Err("{model:?} isn't a model Templum types: a model is 1 to 64 letters, digits, ., -, _, : and /, beginning with a
  letter or a digit."); an effort for an agent with none → Err("{title} takes no effort from Templum."); an effort not
  in its list → Err("{title}'s effort is low, medium, high, xhigh or max." — the list joined with ", " and its last
  with " or "). `AgentEntry::launch_args`: `Some(agent)` → `agent.launch_args`; None with either → Err("Templum
  doesn't give {title} a model or an effort: it is one of your own agents.").
- [ ] **Step 4:** `cargo test -p templum-terminal && cargo test -p templum agents::`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum-terminal/src/agent.rs crates/templum/src/agents.rs
git commit -m "feat(connections): know how Claude Code, Codex and Gemini CLI take a model and an effort"
```

---

### Task 3: The tools and verbs: `send_keys`, `interrupt`, `close_block`, `spawn_agent`'s model and effort

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (`Call` :27-109; `Spawn` :125-135; `list()`: `spawn_agent` :338-363, three
  new tools after `git_status` :438-444; `parse`: `spawn_agent` :577-582, new arms before `"wire"` :626; tests :659-750,
  :812-830, a new test after `wire_is_the_command_lines_alone` :907)
- Modify: `crates/templum-mcp/src/cli.rs` (`VERBS` :13-16; `SPAWN` :22-23 and a `KEYS` const; the `spawn` arm
  :200-218; new arms before `("whoami", _)` :290; their usages :290-304; tests: `only_the_verbs_are_commands` :756, a
  new test after `delegate_reply_and_wait_send_their_calls` :580)
- Modify: `crates/templum/src/workspace/bus.rs:404-436` (a stopgap arm: `call` matches `Call` exhaustively; Tasks 4,
  5 and 9 replace it, as batch 5's Task 1 did)
- Modify: `crates/templum/src/workspace/spawn.rs:33-38` (`..` in the destructuring until Task 8)

**Interfaces:**
- Produces: `Call::SendKeys { block: String, keys: Vec<String> }`, `Call::Interrupt { block: String }`,
  `Call::CloseBlock { block: String }`; `Spawn { agent, prompt, folder, worktree, model: Option<String>, effort:
  Option<String> }` (blank is None, as `prompt`); `pub const MAX_KEYS: usize = 16`.

- [ ] **Step 1: Write the failing tests.** In `every_tool_listed_parses_its_example`, the `spawn_agent` example gains
  `"model": "gpt-5.6-luna", "effort": "high"`, and after `("git_status", …)` add:

```rust
            ("send_keys", json!({ "block": "k9x1d", "keys": ["2", "enter"] })),
            ("interrupt", json!({ "block": "k9x1d" })),
            ("close_block", json!({ "block": "k9x1d" })),
```

  In `the_tools_that_wait_have_their_defaults_and_limits`, the `Spawn` literal (:820) gains `model: None, effort:
  None`, and both "an empty string is no value" and "nor is a blank one" objects gain `"model": ""`/`" "` and
  `"effort": ""`/`"\t"`. New test:

```rust
    /// send_keys takes 1 to 16 keys, as strings; their names are the app's to check.
    #[test]
    fn send_keys_takes_one_to_sixteen_keys() {
        assert_eq!(
            parse("send_keys", &json!({ "block": "b", "keys": ["ctrl+c"] })),
            Ok(Call::SendKeys {
                block: "b".into(),
                keys: vec!["ctrl+c".into()]
            })
        );
        let (full, over) = (vec!["y"; MAX_KEYS], vec!["y"; MAX_KEYS + 1]);
        assert!(parse("send_keys", &json!({ "block": "b", "keys": full })).is_ok());
        for bad in [
            json!({ "block": "b" }),
            json!({ "block": "b", "keys": [] }),
            json!({ "block": "b", "keys": over }),
            json!({ "block": "b", "keys": "enter" }),
            json!({ "block": "b", "keys": [13] }),
        ] {
            let err = parse("send_keys", &bad).unwrap_err();
            assert!(err.contains("\"keys\"") && err.contains("16"), "{bad}: {err}");
        }
        assert_eq!(
            parse("interrupt", &json!({ "block": "b" })),
            Ok(Call::Interrupt { block: "b".into() })
        );
        assert_eq!(
            parse("close_block", &json!({ "block": "b" })),
            Ok(Call::CloseBlock { block: "b".into() })
        );
        assert!(parse("close_block", &json!({})).is_err());
    }
```

  In `cli.rs`, `only_the_verbs_are_commands` adds `assert!(is_verb("keys") && is_verb("interrupt") &&
  is_verb("close"));`, and a new test (same setup as `delegate_reply_and_wait_send_their_calls`):

```rust
    #[test]
    fn the_control_verbs_send_their_calls() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let _ = m.reply.send(tool_result(id, &Reply::ok("ok")));
            }
        });
        for (args, sent) in [
            (
                &["keys", "k9x1d", "2", "enter"][..],
                json!({ "name": "send_keys", "arguments": { "block": "k9x1d", "keys": ["2", "enter"] } }),
            ),
            (
                &["interrupt", "k9x1d"][..],
                json!({ "name": "interrupt", "arguments": { "block": "k9x1d" } }),
            ),
            (
                &["close", "k9x1d"][..],
                json!({ "name": "close_block", "arguments": { "block": "k9x1d" } }),
            ),
            (
                &["spawn", "codex", "--model", "gpt-5.6-luna", "--effort", "high"][..],
                json!({ "name": "spawn_agent", "arguments": { "agent": "codex", "model": "gpt-5.6-luna", "effort": "high" } }),
            ),
        ] {
            assert_eq!(run_with(args, &env(&bus), "").0, 0, "{args:?}");
            assert_eq!(calls.recv_timeout(LIMIT).unwrap(), sent, "{args:?}");
        }
        for (args, usage) in [
            (&["keys", "k9x1d"][..], "usage: templum keys"),
            (&["interrupt"][..], "usage: templum interrupt"),
            (&["close", "a", "b"][..], "usage: templum close"),
            (&["spawn", "codex", "--effort"][..], "usage: templum spawn"),
        ] {
            let (code, _, err) = run_with(args, &env(&bus), "");
            assert_eq!(code, 2, "{args:?}");
            assert!(err.starts_with(usage), "{args:?}: {err}");
        }
    }
```

- [ ] **Step 2:** `cargo test -p templum-mcp`. Expected: FAIL to compile (no `Call::SendKeys`, no `MAX_KEYS`).
- [ ] **Step 3: Implement.**
  - `Call`'s three variants, each with a doc line; `Spawn`'s two fields ("The model and the reasoning effort its
    agent starts on, checked by the app").
  - `list()`, `spawn_agent`: append to its description "With model and effort, its command line starts it on them,
    and so does every resume."; properties `"model": { "type": "string", "description": "Its model: claude
    --model, codex -m, gemini -m. Letters, digits, . - _ : and /." }`, `"effort": { "type": "string",
    "description": "Its reasoning effort. claude: low, medium, high, xhigh, max. codex: low, medium, high, xhigh,
    max, ultra. gemini takes none." }`.
  - Three tools after `git_status`, each `"required": ["block"]`:
    - `send_keys` — "Presses keys in a terminal connected to you, one at a time, as a keyboard does (never as a
      paste): to answer an agent's dialog (a permission, a trust question) or drive a program. Keys: enter, escape,
      tab, shift+tab, backspace, delete, up, down, left, right, home, end, page_up, page_down, space, ctrl+<letter>,
      or one character (2, y). Returns the last 20 lines of its screen after them. Not your own block, nor an agent
      on another agent's task unless it waits for its person."; `"keys": { "type": "array", "items": { "type":
      "string" }, "minItems": 1, "maxItems": MAX_KEYS }`.
    - `interrupt` — "Presses Esc once in an agent connected to you while it works or waits for its person, and ends
      the task you handed it: its waits get "Interrupted by you." Returns its screen after. Refused while it is idle
      or on another agent's task."
    - `close_block` — "Closes a block you opened with spawn_agent and are still connected to, without asking the
      person (they bring it back with Ctrl+Z, its agent resumed). Only the person closes any other block."
  - `parse`: `spawn_agent` gains `model: optional("model")?, effort: optional("effort")?`; `send_keys` → `keys`
    must be an array of 1 to `MAX_KEYS` strings, else Err(format!("{name} needs \"keys\", a list of 1 to {MAX_KEYS}
    key names (enter, escape, ctrl+c, 2…)")); `interrupt`, `close_block` → `text("block")`.
  - `cli.rs`: `VERBS` gains `"keys", "interrupt", "close"`; `const KEYS: &str = "keys <block> <key>… [--json]"`;
    `SPAWN` becomes `"spawn <agent> [--folder <folder>] [--worktree <name>] [--prompt <text>] [--model <model>]
    [--effort <effort>] [--json]"`; the `spawn` arm maps `--model` and `--effort` to their keys; arms
    `("keys", [block, keys @ ..]) if !keys.is_empty()` → `send_keys` `{ block, keys }`, `("interrupt", [block])`,
    `("close", [block])` → `close_block`; usages `("keys", _) => KEYS`, `("interrupt", _) => "interrupt <block>
    [--json]"`, `("close", _) => "close <block> [--json]"` (`every_verbs_usage_offers_json` then covers them).
  - `bus.rs` `call`: `Call::SendKeys { .. } | Call::Interrupt { .. } | Call::CloseBlock { .. } =>
    Reply::err("This Templum can't do that yet.")`; `spawn.rs`: `let tools::Spawn { agent, prompt, folder, worktree,
    .. } = spawn;`.
- [ ] **Step 4:** `cargo test -p templum-mcp && cargo build -p templum`. Expected: PASS, builds.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum-mcp/src/tools.rs crates/templum-mcp/src/cli.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/spawn.rs
git commit -m "feat(connections): describe send_keys, interrupt, close_block and spawn_agent's model and effort, with templum keys, interrupt and close"
```

---

### Task 4: `send_keys` presses keys in a wired terminal

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs` (consts after `CONFIRM` :27; `send_keys`, `press_keys`,
  `signature` in `impl Workspace` :167)
- Modify: `crates/templum/src/workspace/mod.rs:149-184` (`Pane.keys_from`), `blocks.rs:412-432` (`keys_from: None`)
- Modify: `crates/templum/src/workspace/bus.rs` (the `SendKeys` arm replaces Task 3's stopgap for it)
- Test: `crates/templum/src/workspace/tests/delegate.rs` (after `wait_for_idle_is_wait_for_idle_and_the_rest_need_a_wire`,
  :660; its `prints` and `heard` helpers are there)

**Interfaces:**
- Consumes: `templum_terminal::{named_key, KEY_NAMES}`, `Terminal::press` (Task 1); `Call::SendKeys` (Task 3).
- Produces: `Pane.keys_from: Option<(String, Instant)>` (who pressed keys in it last, and when);
  `Workspace::send_keys(&mut self, from: &str, block: &str, keys: Vec<String>, cx: &mut Context<Self>) -> Outcome`;
  `Workspace::press_keys(&self, worker: String, by: String, keys: Vec<String>, said: String, cx: &mut Context<Self>)
  -> Task<Reply>`; `Workspace::signature(&self, space: &Space, id: BlockId, cx: &App) -> String` (`"{title}
  ({uid})"`, the title as `about` gives it); consts `KEY_GAP = 50 ms`, `KEYS_SETTLE = 300 ms`, `KEYS_LINES = 20`.

- [ ] **Step 1: Write the failing tests** (add `use crate::workspace::Handed;` and `std::time::Duration` where
  needed):

```rust
/// Keys go as a keyboard sends them, never as a paste: a dialog takes `2` as its option, where a
/// pasted one would land in the agent's input.
#[cfg(unix)]
#[gpui_kit::test]
fn send_keys_presses_keys_not_a_paste(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    // `cat` prints the sequence back: the terminal now brackets pastes, as Claude Code's does.
    prints(&ws, cx, theirs, b"\x1b[?2004hon\r");
    screen_with(&ws, cx, theirs, "2004honon");
    let keys = json!({ "block": them, "keys": ["2", "escape"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "send_keys", keys);
    let (failed, text) = heard(cx, &answers);
    assert!(
        !failed && text.starts_with(&format!("Pressed 2 keys in {them}.")),
        "{text}"
    );
    assert!(text.ends_with("2^["), "the screen after them: {text}");
    let shown = screen_with(&ws, cx, theirs, "2^[");
    assert!(!shown.contains("200~"), "never a paste: {shown}");
}

/// Keys go into an agent waiting for its person, as the person's: once its screen moves, it works.
#[cfg(unix)]
#[gpui_kit::test]
fn send_keys_takes_a_dialog_as_the_persons_keys(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let asks = json!({ "event": "needs_you", "message": "Allow Bash?" });
    event(handle, &ws, cx, &worker, asks);
    let keys = json!({ "block": them, "keys": ["2"] });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "send_keys", keys);
    let (failed, text) = heard(cx, &answers);
    assert!(!failed && text.starts_with(&format!("Pressed 1 key in {them}.")), "{text}");
    ws.read_with(cx, |ws, _| {
        let (by, _) = ws.space.terminal(theirs).unwrap().keys_from.clone().expect("who");
        assert!(by.ends_with(&format!("({uid})")), "{by}");
    });
    ticks(&ws, cx, theirs, 0, 4);
    assert_eq!(state_of(&ws, cx, theirs), Some(AgentState::Working));
}

#[cfg(unix)]
#[gpui_kit::test]
fn send_keys_refuses_its_own_block_an_unknown_key_and_another_agents_task(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let pressing = |block: &str, keys: serde_json::Value| json!({ "block": block, "keys": keys });
    let (failed, text) = call(handle, &ws, cx, &terminal, "send_keys", pressing(&uid, json!(["y"])));
    assert!(failed && text.contains("yours"), "{text}");
    let (failed, text) =
        call(handle, &ws, cx, &terminal, "send_keys", pressing(&them, json!(["y", "foo"])));
    assert!(failed && text.contains("\"foo\"") && text.contains("page_up"), "{text}");
    let typed = ws.read_with(cx, |ws, _| ws.space.terminal(theirs).unwrap().typed.get());
    assert!(!typed, "no key went before the refusal");
    // On another agent's task: refused while it works, not while it waits for its person.
    ws.update(cx, |ws, _| {
        let now = Instant::now();
        ws.space.terminal_mut(theirs).unwrap().handed = Some(Handed {
            by: "elsewhere".into(),
            since: now,
            until: now + std::time::Duration::from_secs(600),
        });
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "send_keys", pressing(&them, json!(["y"])));
    assert!(failed && text.contains("another agent"), "{text}");
    event(handle, &ws, cx, &worker, json!({ "event": "needs_you", "message": "Allow Bash?" }));
    let y = pressing(&them, json!(["y"]));
    let answers = call_later(handle, &ws, cx, &terminal, 2, "send_keys", y);
    assert!(!heard(cx, &answers).0, "it waits for its person: its dialog takes keys");
    // Its block closes between two keys: the call ends there.
    let ab = pressing(&them, json!(["a", "b"]));
    let answers = call_later(handle, &ws, cx, &terminal, 3, "send_keys", ab);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.close_now(&[theirs], crate::workspace::history::Then::New, window, cx)
        })
    })
    .unwrap();
    let (failed, text) = heard(cx, &answers);
    assert!(failed && text.contains("closed"), "{text}");
}
```

- [ ] **Step 2:** `cargo test -p templum send_keys_`. Expected: FAIL ("This Templum can't do that yet.").
- [ ] **Step 3: Implement.** `send_keys`, refusing with `Outcome::Now` in this order: `target` (wire); the caller's
  own block ("This block is yours: send_keys presses keys in another block."); not a terminal ("{block} isn't a
  terminal."); a key `named_key` doesn't know ("{key:?} isn't a key Templum knows. Keys: {KEY_NAMES}."); its task
  (`Pane::task(now)`) handed by another than `from` while its state isn't `NeedsYou` ("{block} is on a task another
  agent handed it: only that agent presses keys in it while it works. wait_for_idle(\"{block}\") waits for its
  end."). Then `Outcome::Later(press_keys(worker, signature(caller), keys, "Pressed {n} key(s) in {block}.", cx))`
  ("key" for one). `press_keys` re-finds the pane before each key, so a closed block ends the call:

```rust
        cx.spawn(async move |ws, cx| {
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    cx.background_executor().timer(KEY_GAP).await;
                }
                let pressed = ws.update(cx, |ws, cx| {
                    let (Some(pane), Some(key)) = (ws.pane_mut(&worker), named_key(key)) else {
                        return false;
                    };
                    // The person's keys, for the screen's looks: a dialog they answer ends.
                    pane.typed.set(true);
                    pane.keys_from = Some((by.clone(), Instant::now()));
                    pane.term.press(&key);
                    cx.notify();
                    true
                });
                if !pressed.unwrap_or(false) {
                    return Reply::err("Its block was closed before every key was pressed.");
                }
            }
            cx.background_executor().timer(KEYS_SETTLE).await;
            let lines = ws
                .update(cx, |ws, _| {
                    ws.pane(&worker)
                        .map(|p| p.term.shared().lines(KEYS_LINES).join("\n"))
                })
                .ok()
                .flatten()
                .unwrap_or_default();
            Reply::ok(format!("{said} Its screen now:\n{}", tools::tail(&lines, tools::MAX_TEXT)))
        })
```

  `bus.rs`: `Call::SendKeys { block, keys } => return self.send_keys(terminal, &block, keys, cx),`.
- [ ] **Step 4:** `cargo test -p templum send_keys_ && cargo test -p templum workspace::tests::delegate`. Expected:
  PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/mod.rs crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): an agent presses keys in a wired terminal, as a keyboard does, to answer another agent's dialog"
```

---

### Task 5: `interrupt` stops an agent and ends the caller's task

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs` (`interrupt` after `send_keys`)
- Modify: `crates/templum/src/workspace/bus.rs` (the `Interrupt` arm)
- Test: `crates/templum/src/workspace/tests/delegate.rs`

**Interfaces:**
- Consumes: `press_keys`, `signature` (Task 4); `end_turn` (`delegate.rs:446`).
- Produces: `Workspace::interrupt(&mut self, from: &str, block: &str, cx: &mut Context<Self>) -> Outcome`.

- [ ] **Step 1: Write the failing test:**

```rust
#[cfg(unix)]
#[gpui_kit::test]
fn interrupt_presses_esc_and_ends_the_callers_task(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let stop = json!({ "block": them });
    let (failed, text) = call(handle, &ws, cx, &terminal, "interrupt", stop.clone());
    assert!(failed && text.contains("idle"), "{text}");
    let (failed, text) = call(handle, &ws, cx, &terminal, "interrupt", json!({ "block": uid }));
    assert!(failed && text.contains("yours"), "{text}");
    let task = json!({ "block": them, "task": "count to a million", "wait": true });
    let waiting = call_later(handle, &ws, cx, &terminal, 1, "delegate", task);
    let stopped = call_later(handle, &ws, cx, &terminal, 2, "interrupt", stop.clone());
    assert_eq!(answered(&waiting), (true, "Interrupted by you.".to_string()));
    ws.read_with(cx, |ws, _| {
        assert!(ws.space.terminal(theirs).unwrap().handed.is_none())
    });
    let (failed, text) = heard(cx, &stopped);
    assert!(
        !failed
            && text.starts_with(&format!("Pressed Esc in {them}, and ended the task you handed it."))
            && text.contains("^["),
        "{text}"
    );
    // Another agent's task: only that agent stops it.
    ws.update(cx, |ws, _| {
        let now = Instant::now();
        ws.space.terminal_mut(theirs).unwrap().handed = Some(crate::workspace::Handed {
            by: "elsewhere".into(),
            since: now,
            until: now + std::time::Duration::from_secs(600),
        });
    });
    let (failed, text) = call(handle, &ws, cx, &terminal, "interrupt", stop);
    assert!(failed && text.contains("another agent"), "{text}");
}
```

- [ ] **Step 2:** `cargo test -p templum interrupt_presses`. Expected: FAIL ("can't do that yet").
- [ ] **Step 3: Implement `interrupt`.** Refusals, `Outcome::Now`, in order: `target`; its own block ("This block is
  yours: interrupt stops another agent."); not a terminal; no agent (`seen.agent` None or `shell_in_front`: "{block}
  runs no agent: send_keys presses keys in a terminal."). `mine` = `pane.handed` is the caller's (`by == from`, also
  past its timeout, as `wait_for_idle`'s `mine`); not `mine` and `task(now)` is Some → "{block} is on a task another
  agent handed it: only that agent interrupts it."; not `mine` and not `busy(now)` → "{block} is idle: there's
  nothing to interrupt." When `mine`: `end_turn(worker, Reply::err("Interrupted by you."))` and drop its `prompt`
  (Decision 3), before the key, so the waits hear at once. Then `Outcome::Later(press_keys(worker, by,
  vec!["escape".into()], said, cx))`, `said` = "Pressed Esc in {block}, and ended the task you handed it." or "Pressed
  Esc in {block}.". `bus.rs`: `Call::Interrupt { block } => return self.interrupt(terminal, &block, cx),`.
- [ ] **Step 4:** `cargo test -p templum workspace::tests::delegate`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/delegate.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): an agent stops the agent it handed a task to with interrupt"
```

---

### Task 6: `send_to_terminal`'s Enter goes after its text

**Files:**
- Modify: `crates/templum/src/workspace/bus.rs:372-385` (the `SendToTerminal` arm)
- Test: `crates/templum/src/workspace/tests/bus.rs:270-291` (`a_terminal_types_into_itself_and_reads_it_back`)
- Modify: `docs/minors.md` (delete "**`send_to_terminal`'s Enter comes in the same burst as its text.**", under
  "Left by batch 3")

**Interfaces:**
- Consumes: `delegate::ENTER_AFTER` (150 ms).

- [ ] **Step 1: Make the test fail without the change.** Replace its `screen_with(&ws, cx, first, "hello therehello
  there");` with:

```rust
    // The tty shows the text as it is typed; its Enter waits ENTER_AFTER on the test's clock, so
    // `cat` hasn't printed it again yet.
    screen_with(&ws, cx, first, "hello there");
    std::thread::sleep(Duration::from_millis(300));
    let alone = screen_until(&ws, cx, first, |_| true);
    assert_eq!(alone.matches("hello there").count(), 1, "{alone}");
    after(cx, crate::workspace::delegate::ENTER_AFTER.as_millis() as u64);
    // `cat` prints it again on the next row once Enter sent the line (rows are joined).
    screen_with(&ws, cx, first, "hello therehello there");
```

- [ ] **Step 2:** `cargo test -p templum a_terminal_types_into_itself`. Expected: FAIL (count 2: Enter came with the
  text).
- [ ] **Step 3: Implement.** With `enter`, the arm keeps the terminal's id and `cx.spawn`s (detached) a
  `timer(ENTER_AFTER)`, then writes `\r` to `ws.pane(&terminal)` if it is still there; the reply stays `Now`.
- [ ] **Step 4:** `cargo test -p templum workspace::tests::bus`. Expected: PASS
  (`a_wired_terminal_is_read_and_typed_into_until_it_is_unwired` sends no Enter).
- [ ] **Step 5: Commit** (the minor goes with its fix, AGENTS.md).

```bash
git add crates/templum/src/workspace/bus.rs crates/templum/src/workspace/tests/bus.rs docs/minors.md
git commit -m "fix(connections): send_to_terminal's Enter goes a moment after its text, so an agent sends it instead of starting a new line"
```

---

### Task 7: A terminal keeps who opened it, its model and its effort

**Files:**
- Modify: `crates/templum/src/session.rs` (`TerminalItem` :644-662; `LayoutBlock` :702-708; `snapshot` :714-762;
  `sample()` :915-945; tests: `a_snapshot_keeps_the_order_and_the_focus` :1540, a new test after
  `a_note_color_reads_back_and_unknown_or_missing_is_none` :862)
- Modify: `crates/templum/src/workspace/mod.rs` (`Pane` :149-184; `impl Pane` :186; `Body::item` :342-351;
  `Space::layout` :583-597; `Space::see` :685-711), `blocks.rs:412-432` (the new fields, None)
- Modify: `crates/templum/src/workspace/persist.rs` (`resume_agent` :136-160 becomes `restore_terminal`; its calls
  :63-65, :178-181)
- Modify: `crates/templum/src/workspace/selection.rs:191` (a paste is opened by no one)
- Test: `crates/templum/src/workspace/tests/agents.rs:75-121` and a new test; `tests/selection.rs:248-270`;
  `tests/workspaces.rs:17` (compiles with the new fields)

**Interfaces:**
- Consumes: `AgentEntry::launch_args` (Task 2).
- Produces: `Pane.opened_by: Option<String>` (a block id), `Pane.model`, `Pane.effort: Option<String>`;
  `session::TerminalItem.opened_by/model/effort: Option<String>` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]`); `Pane::item(&self, rect: templum_canvas::Rect, uid: String) -> session::TerminalItem` (folder
  and agent as last seen, no session, the three fields); `LayoutBlock::Terminal(TerminalItem, Processes)`;
  `Workspace::restore_terminal(&mut self, id: BlockId, saved: &session::TerminalItem, resume: bool)`.

- [ ] **Step 1: Write the failing tests.** In `session.rs`'s tests:

```rust
    #[test]
    fn a_terminal_keeps_who_opened_it_its_model_and_effort() {
        let text = r#"{"kind":"terminal","rect":{"x":0,"y":0,"w":1,"h":1},"agent":"codex",
                      "opened_by":"b7k2m","model":"gpt-5.6-luna","effort":"high"}"#;
        let item: Item = serde_json::from_str(text).unwrap();
        let Item::Terminal(t) = &item else { panic!("{item:?}") };
        assert_eq!(
            (t.opened_by.as_deref(), t.model.as_deref(), t.effort.as_deref()),
            (Some("b7k2m"), Some("gpt-5.6-luna"), Some("high"))
        );
        let back = serde_json::to_string(&item).unwrap();
        for part in [r#""opened_by":"b7k2m""#, r#""model":"gpt-5.6-luna""#, r#""effort":"high""#] {
            assert!(back.contains(part), "{back}");
        }
        // A file from before has none, and none is written.
        let older: Item =
            serde_json::from_str(r#"{"kind":"terminal","rect":{"x":0,"y":0,"w":1,"h":1}}"#).unwrap();
        let Item::Terminal(o) = &older else { panic!("{older:?}") };
        assert_eq!((&o.opened_by, &o.model, &o.effort), (&None, &None, &None));
        let plain = serde_json::to_string(&older).unwrap();
        assert!(
            !plain.contains("opened_by") && !plain.contains("model") && !plain.contains("effort"),
            "{plain}"
        );
    }
```

  `a_snapshot_keeps_the_order_and_the_focus`: its two `LayoutBlock::Terminal(rect, Processes::default(), uid)` become
  `LayoutBlock::Terminal(TerminalItem { uid, rect: rect.into(), folder: None, agent: None, session: None,
  opened_by, model, effort }, Processes::default())`, the first with `opened_by: Some("b7k2m".into()), model:
  Some("opus".into()), effort: Some("high".into())`, and it asserts:

```rust
        let Item::Terminal(first) = &saved.items[0] else { panic!() };
        assert_eq!(
            (first.opened_by.as_deref(), first.model.as_deref(), first.effort.as_deref()),
            (Some("b7k2m"), Some("opus"), Some("high")),
            "what the UI thread knows of it, kept"
        );
```

  `tests/agents.rs` `a_saved_agent_is_resumed_in_its_block`: the `bbb-2` item gains
  `"opened_by":"b7k2m","model":"opus","effort":"high"`; a fourth item at `"x":2400` has
  `"agent":"claude","session":"ccc-3","model":"a b"`; `wanted` becomes `["claude --continue", "claude --resume aaa-1",
  "claude --resume bbb-2 --model opus --effort high", "claude --resume ccc-3"]`; keep the last `shown` outside the
  loop and add after it:

```rust
    assert!(
        !shown.iter().any(|s| s.contains("--model a")),
        "a model it can't type safely isn't typed: {shown:?}"
    );
    ws.read_with(cx, |ws, _| {
        let kept: Vec<_> = ws
            .space
            .terminals()
            .filter(|(_, p)| p.opened_by.is_some() || p.model.is_some() || p.effort.is_some())
            .map(|(_, p)| (p.opened_by.clone(), p.model.clone(), p.effort.clone()))
            .collect();
        assert_eq!(
            kept,
            [(Some("b7k2m".into()), Some("opus".into()), Some("high".into()))]
        );
    });
```

  `tests/selection.rs` `a_reopened_terminal_resumes_its_agent_but_a_pasted_one_does_not`: before `close_now`, set
  `(pane.opened_by, pane.model, pane.effort) = (Some("b7k2m".into()), Some("opus".into()), Some("high".into()));`;
  after the undo assert `back`'s three are those; after the paste assert `pasted`'s `opened_by` is None ("a pasted
  block was opened by the person").

  A new test in `tests/agents.rs`:

```rust
/// The model and the effort are its agent's: a snapshot that finds that agent gone, or another
/// one there (`claude` typed by hand after the spawned Codex quit), drops them.
#[gpui_kit::test]
fn an_agent_that_left_takes_its_model_and_effort_with_it(cx: &mut TestAppContext) {
    use super::delegate::snapshot;
    use templum_terminal::Inspection;
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let spawned = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.update(cx, |ws, _| {
            let pane = ws.space.terminal_mut(id).unwrap();
            (pane.opened_by, pane.model, pane.effort) =
                (Some("b7k2m".into()), Some("gpt-5.6-luna".into()), Some("high".into()));
        })
    };
    let kept = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            let p = ws.space.terminal(id).unwrap();
            (p.opened_by.clone(), p.model.clone(), p.effort.clone())
        })
    };
    let found = |agent: Option<&str>, pid: Option<u32>| Inspection {
        agent: agent.map(str::to_string),
        pid,
        ..Default::default()
    };
    spawned(&ws, cx);
    // Not started yet, then running: kept.
    snapshot(handle, &ws, cx, id, found(None, None));
    snapshot(handle, &ws, cx, id, found(Some("codex"), Some(1)));
    assert_eq!(kept(&ws, cx).1.as_deref(), Some("gpt-5.6-luna"));
    snapshot(handle, &ws, cx, id, found(Some("claude"), Some(2)));
    assert_eq!(kept(&ws, cx), (Some("b7k2m".into()), None, None), "still opened by it");
    spawned(&ws, cx);
    snapshot(handle, &ws, cx, id, found(None, None));
    assert_eq!(kept(&ws, cx).1, None, "its agent left");
}
```

- [ ] **Step 2:** `cargo test -p templum session:: && cargo test -p templum workspace::tests::agents && cargo test -p
  templum a_reopened_terminal`. Expected: FAIL to compile (no `opened_by`).
- [ ] **Step 3: Implement.**
  - `TerminalItem`'s three fields, each documented ("The block id of the agent that opened it with `spawn_agent`,
    which may close it; none for one the person opened or pasted", "The model and the effort `spawn_agent` gave its
    agent, typed again when it resumes"). The `TerminalItem` literals in `sample()` and `tests/workspaces.rs` get the
    three as None.
  - `Pane`'s three fields (doc as above); `Pane::item`; `Body::item`'s terminal arm becomes
    `session::Item::Terminal(pane.item(rect, String::new()))`; `Space::layout`'s terminal arm becomes
    `LayoutBlock::Terminal(pane.item(self.home(b), uid), pane.term.processes())`.
  - `LayoutBlock::Terminal(TerminalItem, Processes)` (doc: "A terminal as the UI thread knows it: its id, rect,
    opener, model and effort; its folder, agent and session come from its processes, read in the background").
    `snapshot` builds `Item::Terminal(TerminalItem { folder, agent, session, ..item.clone() })` from what it found.
  - `Space::see`: first for each pane, before its `if seen.agent.is_none()` block, `if pane.seen.pid.is_some() &&
    seen.agent != pane.seen.agent` drop `model` and `effort` (Decision 5: `pid` is known once a snapshot found the
    agent, so one still starting keeps them).
  - `persist.rs`: `restore_terminal` (doc: "A saved terminal `saved`, just started as the terminal `id`: who opened
    it, and, when `resume`, its agent typed back with its session, model and effort") sets `opened_by`; when `resume`
    and the agent is known, it types `{resume command}{args}` (`args` = `launch_args(model, effort)`, empty when
    refused) through `typed()`, and sets `model`/`effort` only when `launch_args` took them. Both call sites call it
    with `in_its_folder`.
  - `selection.rs:191`: the arm becomes `{ t.agent = None; t.opened_by = None; }` (a model needs its agent, which a
    paste drops).
- [ ] **Step 4:** `cargo test -p templum`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/session.rs crates/templum/src/workspace/mod.rs crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/persist.rs crates/templum/src/workspace/selection.rs crates/templum/src/workspace/tests/agents.rs crates/templum/src/workspace/tests/selection.rs crates/templum/src/workspace/tests/workspaces.rs
git commit -m "feat(connections): a terminal keeps the agent that opened it and its agent's model and effort through a restart and Ctrl+Z"
```

---

### Task 8: `spawn_agent` starts its agent on a model and an effort

**Files:**
- Modify: `crates/templum/src/workspace/spawn.rs` (`spawn_agent` :26-109; `open_agent` :111-176)
- Modify: `crates/templum/src/workspace/blocks.rs` (`start_agent` :313-336; `launch_agent`'s call :305)
- Test: `crates/templum/src/workspace/tests/spawn.rs` (after `spawn_agent_opens_a_wired_agent_beside_the_caller`, :119)

**Interfaces:**
- Consumes: `AgentEntry::launch_args` (Task 2); `Spawn.model/effort` (Task 3); `Pane.opened_by/model/effort` (Task 7).
- Produces: `start_agent(&mut self, entry: Option<&AgentEntry>, command: &str, args: &str, folder: &Path, rect: Rect,
  window, cx) -> Option<BlockId>` (`args` after the start command, before `typed()`'s flags); `open_agent(&mut self,
  from: &str, entry: &AgentEntry, folder: &Path, spawn: &tools::Spawn, window, cx) -> Reply`.

- [ ] **Step 1: Write the failing test:**

```rust
/// The model and the effort go right after the agent's command (`cat` here), before what Templum
/// adds; anything it can't type safely opens nothing, a worktree included.
#[gpui_kit::test]
fn spawn_agent_types_its_model_and_effort_first(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "model": "gpt-5.6-luna", "effort": "high" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(
        !failed && text.ends_with("connected to you, on gpt-5.6-luna (high)."),
        "{text}"
    );
    let new = spawned(&ws, cx, mine);
    screen_with(&ws, cx, new, "cat -m gpt-5.6-luna -c model_reasoning_effort=high");
    ws.read_with(cx, |ws, _| {
        let p = ws.space.terminal(new).unwrap();
        assert_eq!(
            (p.opened_by.as_deref(), p.model.as_deref(), p.effort.as_deref()),
            (Some(uid.as_str()), Some("gpt-5.6-luna"), Some("high"))
        );
    });
    let spawn = json!({ "agent": "claude", "folder": folder.path(), "model": "opus", "effort": "max" });
    let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", spawn);
    assert!(!failed, "{text}");
    screen_with(&ws, cx, spawned(&ws, cx, mine), "cat --model opus --effort max");
    let repo = committed_repo();
    let blocks = ws.read_with(cx, |ws, _| ws.space.blocks.len());
    for (bad, why) in [
        (json!({ "agent": "codex", "model": "opus;rm -rf ~" }), "letters, digits"),
        (json!({ "agent": "gemini", "effort": "high" }), "no effort"),
        (json!({ "agent": "copilot", "model": "gpt-5" }), "a model or an effort"),
        (
            json!({ "agent": "codex", "effort": "extreme", "folder": repo.path(), "worktree": "w" }),
            "low, medium",
        ),
    ] {
        let (failed, text) = call(handle, &ws, cx, &terminal, "spawn_agent", bad.clone());
        assert!(failed && text.contains(why), "{bad}: {text}");
    }
    assert_eq!(ws.read_with(cx, |ws, _| ws.space.blocks.len()), blocks, "nothing opened");
    assert!(!repo.path().join(".worktrees").exists(), "no worktree made");
}
```

- [ ] **Step 2:** `cargo test -p templum spawn_agent_types_its_model`. Expected: FAIL (no flags typed; the bad ones
  open blocks).
- [ ] **Step 3: Implement.**
  - `spawn_agent` keeps `spawn` whole (reading `spawn.agent`, `spawn.folder`, `spawn.worktree` by reference); right
    after finding the entry, `entry.launch_args(spawn.model.as_deref(), spawn.effort.as_deref())`, an Err its
    `Outcome::Now(Reply::err(why))`, before `installed` and the worktree. Both paths call `open_agent(&from, &entry,
    folder, &spawn, window, cx)` (the worktree's job moves `spawn` in).
  - `open_agent`: `args` = `entry.launch_args(..)` again (`unwrap_or_default`: checked above); `ws.start_agent(
    Some(entry), &entry.command, &args, folder, rect, window, cx)`; then the new pane's `opened_by` = the caller's
    uid, `model`, `effort` = the spawn's; `said` gets before its final ".": `", on {m} ({e})"`, `", on {m}"` or
    `", with {e} effort"`; the prompt is `spawn.prompt.clone()`.
  - `start_agent` (Decision 6, `#[allow(clippy::too_many_arguments)]`): the command is
    `format!("{}{args}", <start command or command>)`; `launch_agent` passes `""`.
- [ ] **Step 4:** `cargo test -p templum workspace::tests::spawn && cargo test -p templum workspace::tests::agents`.
  Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/tests/spawn.rs
git commit -m "feat(connections): spawn_agent starts its agent on the model and the effort it is given"
```

---

### Task 9: `close_block` closes what the caller opened

**Files:**
- Modify: `crates/templum/src/workspace/spawn.rs` (`close_block` after `open_agent`; `use super::bus::target`,
  `super::history::Then`)
- Modify: `crates/templum/src/workspace/bus.rs` (the `CloseBlock` arm; the stopgap goes)
- Test: `crates/templum/src/workspace/tests/spawn.rs`

**Interfaces:**
- Consumes: `Pane.opened_by` (Tasks 7, 8); `close_now` (`blocks.rs:533`); `in_space` (`spaces.rs:372`); `target`
  (`bus.rs:621`).
- Produces: `Workspace::close_block(&mut self, from: &str, block: &str, window: &mut Window, cx: &mut
  Context<Self>) -> Reply`.

- [ ] **Step 1: Write the failing test:**

```rust
/// An agent closes the blocks it opened while it is wired to them, as one step of Ctrl+Z, which
/// brings one back still opened by it, its agent resumed with its model.
#[gpui_kit::test]
fn close_block_closes_only_what_the_caller_opened(cx: &mut TestAppContext) {
    let (handle, ws, mine) = spawner(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let folder = tempfile::tempdir().unwrap();
    let spawn = json!({ "agent": "codex", "folder": folder.path(), "model": "gpt-5.6-luna" });
    assert!(!call(handle, &ws, cx, &terminal, "spawn_agent", spawn).0);
    let new = spawned(&ws, cx, mine);
    let (_, them) = me(&ws, cx, new);
    let close = json!({ "block": them });
    let (failed, text) = call(handle, &ws, cx, &terminal, "close_block", close.clone());
    assert!(!failed && text.starts_with(&format!("Closed {them}.")), "{text}");
    ws.read_with(cx, |ws, _| assert!(ws.space.block_by_uid(&them).is_none()));
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| assert!(ws.undo_canvas(false, window, cx)))
    })
    .unwrap();
    let back = spawned(&ws, cx, mine);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.uids[&back], them);
        let pane = ws.space.terminal(back).unwrap();
        assert_eq!(pane.opened_by.as_deref(), Some(uid.as_str()));
    });
    screen_with(&ws, cx, back, "codex resume --last -m gpt-5.6-luna");
    // A block the person opened, the caller's own, and one no longer wired: the person's.
    let other = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(Rect::new(40.0, 900.0, 400.0, 300.0), window, cx);
                let other = ws.space.canvas.top().unwrap();
                assert!(ws.connect(mine, other, cx));
                other
            })
        })
        .unwrap();
    let (_, theirs) = me(&ws, cx, other);
    for block in [&theirs, &uid] {
        let (failed, text) = call(handle, &ws, cx, &terminal, "close_block", json!({ "block": block }));
        assert!(
            failed && text == format!("Only the person closes {block}: you didn't open it."),
            "{text}"
        );
    }
    ws.update(cx, |ws, cx| assert!(ws.disconnect(Wire::new(mine, back), cx)));
    let (failed, text) = call(handle, &ws, cx, &terminal, "close_block", close);
    assert!(failed && text.contains("isn't connected to you"), "{text}");
}
```

- [ ] **Step 2:** `cargo test -p templum close_block_closes`. Expected: FAIL ("can't do that yet").
- [ ] **Step 3: Implement `close_block`:** `target` (Decision 4); the pane's `opened_by` isn't the caller's uid (any
  other kind of block has none) → "Only the person closes {block}: you didn't open it."; else
  `in_space(&space_id, |ws| ws.close_now(&[id], Then::New, window, cx))` (asks no one; one Ctrl+Z step, also in a
  hidden workspace) and "Closed {block}. The person can bring it back with Ctrl+Z." `bus.rs`: `Call::CloseBlock {
  block } => self.close_block(terminal, &block, window, cx),`.
- [ ] **Step 4:** `cargo test -p templum workspace::tests::spawn`. Expected: PASS.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/spawn.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/tests/spawn.rs
git commit -m "feat(connections): an agent closes the blocks it opened, and Ctrl+Z brings them back"
```

---

### Task 10: "answered by" on the terminal for 10 seconds

**Files:**
- Modify: `crates/templum/src/workspace/delegate.rs` (`ANSWERED_FOR`; `Pane::answered_by` in `impl Pane` :102; the
  redraw in `press_keys`)
- Modify: `crates/templum/src/block.rs` (the chip :180-203; `status_dot` :825-861)
- Test: `crates/templum/src/workspace/tests/delegate.rs`

**Interfaces:**
- Consumes: `Pane.keys_from` (Task 4).
- Produces: `pub(crate) const ANSWERED_FOR: Duration = Duration::from_secs(10)`; `Pane::answered_by(&self) ->
  Option<&str>` (who pressed keys in it, while `ANSWERED_FOR` hasn't passed).

- [ ] **Step 1: Write the failing test:**

```rust
/// Keys from another block name it after the dot, in place of the usage chip, and in the dot's
/// tooltip, for ten seconds.
#[gpui_kit::test]
fn keys_name_who_pressed_them_for_ten_seconds(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let id = first_id(&ws, cx);
    let (terminal, _) = me(&ws, cx, id);
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().seen.agent = Some("codex".into())
    });
    event(handle, &ws, cx, &terminal, json!({ "event": "working" }));
    ws.update(cx, |ws, _| {
        ws.space.terminal_mut(id).unwrap().keys_from =
            Some(("Claude Code (b7k2m)".into(), Instant::now()))
    });
    let tip = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| {
            crate::block::status_dot(ws, ws.space.terminal(id).unwrap()).map(|(_, t)| t.to_string())
        })
    };
    let chip = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(("answered-by", id.0 as usize)).is_some()
        })
        .unwrap()
    };
    assert_eq!(tip(&ws, cx).as_deref(), Some("Working · answered by Claude Code (b7k2m)"));
    assert!(chip(cx));
    ws.update(cx, |ws, _| {
        let at = &mut ws.space.terminal_mut(id).unwrap().keys_from.as_mut().unwrap().1;
        *at = at.checked_sub(std::time::Duration::from_secs(11)).unwrap();
    });
    assert_eq!(tip(&ws, cx).as_deref(), Some("Working"));
    assert!(!chip(cx));
}
```

- [ ] **Step 2:** `cargo test -p templum keys_name_who`. Expected: FAIL (tooltip "Working", no chip).
- [ ] **Step 3: Implement.** `Pane::answered_by`. `status_dot` appends `" · answered by {by}"` to its text while
  `answered_by` is Some. In `block.rs`, the chip: while `answered_by` is Some, a chip with id `("answered-by", n)`,
  text "answered by {by}", tooltip "{by} pressed keys in this terminal", styled as the usage chip and in its place;
  else the usage chip as today (one builder for both, with the id, text and tooltip as its inputs). In `press_keys`,
  after the last key, `cx.spawn` (detached) a `timer(ANSWERED_FOR)` then `cx.notify()`, so the chip goes with no
  other frame.
- [ ] **Step 4:** `cargo test -p templum keys_name_who && cargo test -p templum workspace::tests::usage`. Expected:
  PASS. UI check: `cargo run -p templum`, two wired terminals, `templum keys <other> y` from one: "answered by …"
  after the other's dot, gone after 10 s, in light and dark themes.
- [ ] **Step 5: Commit.**

```bash
git add crates/templum/src/workspace/delegate.rs crates/templum/src/block.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): a terminal says which agent pressed keys in it, for 10 seconds"
```

---

### Task 11: The spec, the minors, the full check, the benches, push

**Files:** `docs/specs/connections.md`, `docs/ARCHITECTURE.md`, `docs/minors.md`

- [ ] **Step 1: The spec** (`docs/specs/connections.md`):
  - Header: `- **Status:** Batches 1 to 6 implemented; 7 and 8 (orchestration) specified`.
  - Spawning's table, the Codex row: "| Codex | `-m <m>` | `-c model_reasoning_effort=<e>`: `low`, `medium`, `high`,
    `xhigh`, `max`, `ultra` (Codex 0.154's models; `ultra` only some of them), unquoted: Codex reads a word that
    isn't TOML as a string |".
  - Spawning, after "The block keeps them: a restart, or Ctrl+Z after a close, resumes its agent with them while that
    agent runs there.": "A snapshot that finds that agent gone, or another agent there, drops them. Templum's agents
    get them also when the person changed their command."
  - Keys, the **Dialogs** bullet: "Keys go into an agent waiting for its person too: the wire is the person's consent
    that the agents on it answer each other's dialogs. Keys from another block count as the person's keys for its
    state ([An agent's state](#an-agents-state)), and the block shows **answered by** *Claude Code (b7k2m)* (the
    caller as its `[from …]` line names it) after its dot for 10 seconds, in place of its usage chip, and in its dot's
    tooltip, whatever it was waiting for: a trust question no hook tells of is no wait Templum knows. Refused: the
    caller's own block, and an agent on another agent's task unless it waits for its person."
  - Keys, `interrupt`: after "ends the task the caller handed it": "(a spawned agent's prompt still waiting is dropped
    with it)".
  - Keys, `close_block`: after the quote, "One it opened but is no longer wired to is refused as any block not
    connected: connect asks the person again."
  - Architecture's batch-6 rows: "| `templum-terminal/src/input.rs`, `terminal.rs`, `agent.rs`; `templum/src/agents.rs`
    | named keys and `Terminal::press`; each agent's model and effort flags (`launch_args`) | 6 |" and "|
    `templum/src/workspace/delegate.rs`, `spawn.rs`, `bus.rs`, `mod.rs`, `blocks.rs`, `persist.rs`, `selection.rs`;
    `session.rs`, `block.rs` | `send_keys`, `interrupt`, `close_block`; `opened_by`, `model` and `effort` saved and
    resumed; "answered by" | 6 |".
  - Testing, Batch 6: append "a snapshot that finds another agent drops the model and the effort; a model Templum
    can't type in a hand-edited file isn't typed".
- [ ] **Step 2: `docs/ARCHITECTURE.md`**, Source layout: `delegate.rs` (… `reply`, `wait_for_idle`, `send_keys` and
  `interrupt`), `spawn.rs` (… its prompt once it is ready, `close_block`); Persistence's last paragraph: "a terminal's
  item also keeps the block that opened it and its agent's model and effort".
- [ ] **Step 3: `docs/minors.md`**, under ## Connections, "Left by batch 6; line numbers are those of `development` at
  <Task 10's commit>." (plus whatever the reviews of this batch defer):
  - **Claude Code's `[1m]` models can't be asked for.** `Agent::launch_args`
    (`crates/templum-terminal/src/agent.rs`) takes letters, digits, `.-_:/` only, so `opus[1m]` (its 1M-token
    context) is refused: brackets need quoting in each shell. An agent asking for it is refused. Fix: quote the model
    per shell (`'…'` for POSIX shells and pwsh) and allow `[` `]`.
  - **`close_block` closes a block the person is typing in.** `close_block`
    (`crates/templum/src/workspace/spawn.rs`) asks no one; Ctrl+Z brings the block back with a new shell, without what
    the person had typed and not sent. Fix: refuse while the person typed into it in the last few seconds (the
    counter batch 8 adds for its notices).
  - **A wrapper set as Claude Code's or Codex's command gets their model flags.** `AgentEntry::launch_args`
    (`crates/templum/src/agents.rs`) gives a model and an effort to Templum's agents whatever their command, where
    `start_command` adds a session id only when the command runs the agent's own program. A person whose
    `[agents] claude = { command = "my-wrapper" }` doesn't pass flags on sees it fail when an agent spawns it with a
    model. Fix: refuse them when `program()` isn't the agent's, as `start_command` does.
  - **Codex's `ultra` is typed for any model.** Codex 0.154 lists `ultra` only for gpt-6-astra, gpt-5.6-sol and
    gpt-5.6-terra (`codex debug models`), and `launch_args` checks against Codex's whole list. A person sees Codex
    refuse or lower it for gpt-5.6-luna (manual check 1 says which). Fix: check the effort against the model's
    `supported_reasoning_levels` in `~/.codex/models_cache.json`, which `codex_models` (`crates/templum/src/agents.rs`)
    already reads.
- [ ] **Step 4: The full check:** `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo
  test --workspace`, then `cargo deny check`. Expected: all pass; report the test count.
- [ ] **Step 5: The benches** (Linux, release): `cargo run --release -p templum -- --bench` and
  `cargo run --release -p templum -- --bench-mixed`; keep both JSON lines. Expected: `p95_ms` ≤ 16.7 each.
- [ ] **Step 6: Commit and push.**

```bash
git add docs/specs/connections.md docs/ARCHITECTURE.md docs/minors.md
git commit -m "docs(connections): batch 6 in the spec: Codex's efforts, answered by, interrupt and close_block's edges"
git push origin development
```

  The push updates draft PR #36 (dev→main), which runs CI on the 3 OS (`gh pr checks 36`): add batch 6 to its title
  and description, with the two bench JSON lines.

## Manual checks (Hyprland, `cargo run --release -p templum`, the user's bash with ble.sh)

1. **Model and effort:** Claude Code from the canvas, wired to nothing, asked "spawn a codex here with model
   gpt-5.6-luna and effort high": the new block's shell shows `codex -m gpt-5.6-luna -c model_reasoning_effort=high
   -c 'mcp_servers.templum…'` and runs it under ble.sh; Codex's `/status` shows that model and effort. Again with
   `effort ultra`: note what Codex does (it lists `ultra` only for some models; the minor says what to fix). A Claude
   Code spawned with `opus` and `max`: `/status` (or its header) shows them.
2. **Restart:** quit Templum and start it: that Codex block resumes `codex resume <id> -m gpt-5.6-luna -c
   model_reasoning_effort=high …`, and the Claude Code one `claude --resume <id> --model opus --effort max --settings …`.
3. **A real dialog:** Claude Code A wired to Claude Code B; B at a Bash permission dialog; A runs `send_keys
   ["1"]` (or `["enter"]`): the dialog is answered, nothing is left in B's input, B's dot turns accent, "answered by
   Claude Code (…)" shows after B's dot for about 10 s and in its tooltip. A Codex in a folder it doesn't trust yet,
   spawned by A: A answers its trust question with `send_keys ["enter"]` or `["2"]` as its screen shows (Esc into
   either agent under its kitty keyboard works as the person's Esc).
4. **interrupt:** A delegates a long task to B with `wait`; A calls `interrupt`: B stops (one Esc; Claude Code's rewind
   doesn't open), A's `delegate` returns "Interrupted by you.", its result ends with B's screen.
5. **close_block and Ctrl+Z:** A closes the B it spawned: B goes at once, no question; Ctrl+Z on the canvas brings B
   back wired to A, resuming with its model; A closes it again. A block the person opened: "Only the person closes …".
6. **send_to_terminal:** A `send_to_terminal` with `enter` into a Claude Code's prompt: the text is sent, not left as
   a new line in its input.
7. **Benches** pasted in PR #36 (Task 11).

## Self-review

- Spec coverage: named keys and `press` (Task 1), the flags (2), the tools and verbs (3), `send_keys` with its
  refusals and the person's-keys rule (4), `interrupt` (5), the later Enter (6), `opened_by`/`model`/`effort` saved,
  resumed, pasted, dropped (7), `spawn_agent`'s model and effort, its result and its refusals before a worktree (8),
  `close_block` with Ctrl+Z (9), "answered by" (10), the spec's open point and deviations, minors, checks (11 and
  Decisions). Testing "Batch 6": every item has its test (Tasks 1-10).
- Types: `named_key`/`KEY_NAMES`/`Terminal::press` (1) used by Task 4; `Agent::launch_args` (2) by
  `AgentEntry::launch_args` (2), used by Tasks 7 and 8; `Call::SendKeys`/`Interrupt`/`CloseBlock`, `Spawn.model/effort`
  (3) by Tasks 4, 5, 9, 8; `press_keys`/`signature`/`keys_from` (4) by Tasks 5 and 10; `Pane.opened_by/model/effort`,
  `Pane::item`, `restore_terminal` (7) by Tasks 8 and 9; `ANSWERED_FOR`/`Pane::answered_by` (10) by `block.rs`.
- The stopgap arm of Task 3 is gone after Task 9; `spawn.rs`'s `..` after Task 8.
