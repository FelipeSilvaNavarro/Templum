# Connections, batch 5 (the rest): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

## Context

Batches 1 to 4 of [docs/specs/connections.md](../specs/connections.md) are on `development` (pushed, 421734d; draft PR
#36 dev→main runs CI). Batch 5 closes the feature: an agent waits for any event in a wired block (`wait_for`); a
person or a script wires two blocks from a terminal (`templum wire`); a wire gets a label; Settings gets a Connections
section; Gemini CLI joins Claude Code and Codex; and an agent's dot follows its screen where its hooks say nothing,
which fixes four minors deferred "to the screen going still (batch 5)". Three findings changed the spec's plan; the
user decided them on 2026-10-15 (Decisions 1-3).

**Goal:** `wait_for` (`idle`, `needs_you`, `command_finished`, `attention`) as a tool and as `templum wait --for`;
`templum wire <a> <b>`; an agent's state from its screen; wire labels (Rename wire…, a pill at the curve's middle,
saved, undoable); Settings ▸ Connections (the switch, the server's command to copy, Gemini CLI's Install/Remove);
Gemini CLI's server and hooks through a Templum extension; `--json` on the new verbs.

**Architecture:** `templum-mcp` gains `tools::Event`, `Call::WaitFor` and the command-line-only `Call::Wire`
(`tools.rs`), the verbs' flags (`cli.rs`) and `hooks::gemini`. `templum-usage` gains `gemini.rs`, which writes the
extension's two files. In the app, `waits.rs` gets `Waiting::Event` (answered by the agent's hooks, a balloon opening,
or a 250 ms look at the terminal's counters, so no frame is needed); a new `activity.rs` looks at every agent's screen
once a second; `wires.rs` gets `templum wire`, labels (kept on `Space` by the ends' block ids) and `Change::Label`;
`crate::wires::label` draws a label as a div under the blocks, as block titles are drawn; `settings/connections.rs`
is the new section, and `workspace/usage.rs` runs Gemini CLI for it.

**Tech Stack:** Rust 2024, GPUI (gpui-fast 0.1.8) through gpui-kit 0.7.3: `Switch`, `Spinner`,
`component::clipboard::Clipboard` (`gpui-component-0.7.3/src/clipboard.rs`: `Clipboard::new(id).value(..)`, a
`Button` with that id), `prompt::ask_text` and `prompt::confirm` (`crates/templum/src/prompt.rs`); `serde_json`;
Gemini CLI 0.63.0 (every Gemini fact below was read in its bundle).

**Spec:** [docs/specs/connections.md](../specs/connections.md): batch 5 of its Delivery: the Tools row `wait_for`, the
command-line rows `templum wait --for` and `templum wire`, `--json`, Wires' **Labels**, Hooks' Gemini row, "What
Templum types into each agent" (Gemini CLI, Others), Config (Settings ▸ Connections), the Architecture row for
`settings/`, and the Risks row "batch 5 may add 'the screen has been still for N seconds'". Task 11 rewrites the spec
where the Decisions changed it.

## Global Constraints

- Only `crates/templum` depends on gpui-kit; `templum-mcp` and `templum-usage` stay UI-free and are tested headlessly.
  No new dependency. No `unsafe`. Every logic change ships a test that fails without it.
- Spec, verbatim: `wait_for` (`block`, `event` (`idle`, `needs_you`, `command_finished`, `attention`), `timeout`):
  "Waits for the event in the block"; needs a wire. "A `timeout` is whole seconds from 1 to 3600 … a longer one is cut
  to 3600."
- "`templum wait <block> [--for <event>] [--timeout <s>]` | `wait_for_idle`, `wait_for`"; "`templum wire <a> <b>` |
  wires two blocks (only from a terminal of the same workspace)".
- "Each verb is one tool call: it prints the result's text, or with `--json` the whole result; a refusal goes to stderr
  with exit code 1; outside Templum or with Templum gone, exit code 2."
- Labels: "**Rename wire…** in a wire's menu; the label sits in a pill at the curve's middle, down to the Silhouette
  level." Files: `"wires": [{ "a": "b7k2m", "b": "t4q8p", "label": "dev server" }]`; "An older Templum ignores both."
- Config: "Off, nothing is typed into agents; the bus still serves the command line. **Settings ▸ Connections** has
  the switch, the server's command for other agents, and Gemini CLI's setup." Others: "**Settings ▸ Connections**
  shows the server's command to copy into their configuration. They know their block anyway: the server inherits
  `TEMPLUM_TERMINAL`."
- Hooks: "Each hook runs `templum hook <agent>`, which reads what the agent hands it, turns it into an event, sends it
  to the bus with a one-second limit, and exits 0 at once, even when Templum is gone." Gemini: "as Claude Code's".
- Commits on `development`, Conventional Commits, `feat`/`fix` titles written for a person using Templum; every
  commit message ends with the line `Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU`.
- Before saying done: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test
  --workspace`; this batch paints (the label), so `--bench` and `--bench-mixed` run too (p95 ≤ 16.7 ms).

## Decisions

The user's, 2026-10-15:

1. **Gemini CLI through an extension.** Gemini CLI 0.63.0 ignores a system settings file
   (`GEMINI_CLI_SYSTEM_SETTINGS_PATH`) unless the file and every folder above it belong to root
   (`checkPosixStatsSecurity`: "is not owned by root (uid 0)"), so the spec's first way can't work, and
   `gemini mcp add` gives no hooks (no dot, no answer to a delegated task). Settings ▸ Connections ▸ Gemini CLI ▸
   **Install** (after a question) writes `<data>/gemini/gemini-extension.json` and `<data>/gemini/hooks/hooks.json`
   and runs `gemini extensions link <data>/gemini --consent`; **Remove** runs `gemini extensions uninstall templum`.
   `~/.gemini/settings.json` is never touched. Every Gemini CLI the person opens then gets the server and the hooks;
   outside Templum the server shows as disconnected (`templum mcp` exits 2) and the hooks do nothing (exit 0).
2. **`templum wire` asks no one**, from any terminal of the workspace, an agent's shell tool included (the user knows
   an agent can wire itself this way). It is a command-line verb only: `tools/list` never shows it, and an MCP client
   calling it gets "Unknown tool".
3. **An agent's state from its screen is in this batch.** Once a second Templum hashes the last 100 lines of every
   agent's screen. Changed at 3 of the last 4 looks (a spinner, Codex's "Working (5s)" counter) → Working, from no
   state, Idle or a wait for the person (a tool the person allowed now runs). Still for 5 looks while Working →
   Idle, and the task it was on ends with its last 40 screen lines (no event will: an interrupt while writing, an
   agent without hooks). A look right after the person typed into the terminal tells nothing. It fixes the minors
   "Codex's dot never says Working", "A Claude Code interrupted while it writes stays on Working", "A Bash command
   sent to the background may show a working Claude Code as idle" (its dot), and "A Claude Code stays waiting for you
   while a tool the person allowed runs".

Mine, within the spec:

4. `wait_for(idle)` is `wait_for_idle` (same answers, same waits). `needs_you` is the yellow dot: the agent's
   `NeedsYou` state or a balloon of its block; answered at once when it already is. `command_finished` and
   `attention` are the next ones after the call: the wait keeps the terminal's counters (`Status.finished`,
   `Status.attention`) from the call and looks at them every 250 ms, so a hidden window (no frames) still answers.
   The caller's own block is refused. The timeout's answer is an error with the last 40 screen lines.
5. A label lives on `Space::labels`, by its ends' block ids (`uid`s, stable through Ctrl+Z and restarts), and stays
   when its wire goes, so a wire brought back by Ctrl+Z (or with its closed block) has its label again. Renaming is a
   step of Ctrl+Z (`Change::Label`), since otherwise Ctrl+Z right after a rename would take the wire away. Blank
   removes the label; the text is put on one line and cut at 60 characters.
6. The pill is a div under the blocks, centered on the curve's middle, like a block's title label: scaled with the
   canvas at Full, its own size (12 px) at Silhouette, none at Card (`level(12, rest)`); grid mode hides it. In the
   wire's color (accent when selected).
7. `--json` already works on every verb (`cli.rs:113-118`, tested at `:493-498`); this batch gives it to the new
   verbs and adds a test that every usage line offers it.
8. Gemini's `AfterTool` hook is heard too (working again after a permission, as Claude Code's `PostToolUse`); the
   extension's server gets `"timeout": 3600000` (Gemini gives up on a tool after 10 minutes otherwise; Codex gets
   `tool_timeout_sec=3600` for the same reason).
9. Left out (YAGNI): rewriting the extension at start, labels in `list_blocks`.

## Review Focus

1. A permission dialog left open (Claude Code, Gemini CLI): its dot stays yellow, the screen look never turns it to
   Working; the person typing a long prompt into an idle agent doesn't show Working. (Task 7: unit test
   `a_screen_still_for_five_looks_is_at_rest`, headless test `a_screen_at_work_says_working_and_one_gone_still_ends_its_task`;
   the dialog itself is manual check 4.)
2. `wait_for(needs_you)` on an agent already waiting (a dialog, or a balloon of its own): answered at once. (Task 5
   test `wait_for_needs_you_now_or_when_it_asks`.)
3. A `wait_for` that never comes (no shell integration, no bell): its timeout answers with the last screen lines,
   closing the watched block answers it, and it never shows a balloon. (Task 5 test
   `a_wait_for_times_out_goes_with_its_block_and_shows_no_balloon`.)
4. A label after Ctrl+Z of a disconnect, of a close, and after a restart; a label in a file on a dropped wire, or a
   blank one, is ignored. (Task 8 tests.)
5. Gemini CLI not found, or failing: Settings says why in red, nothing blocks the main thread. (Task 10 test
   `gemini_cli_links_the_extension_or_says_why_not`.)

## File Structure

| File | Change |
|---|---|
| `crates/templum-mcp/src/tools.rs` | `Event`, `Call::WaitFor`, `Call::Wire`, `cli_only`, `wait_for` in `tools/list` |
| `crates/templum-mcp/src/cli.rs` | `wait --for/--timeout`, `wire`, `hook gemini` |
| `crates/templum-mcp/src/hooks.rs` | `gemini()` |
| `crates/templum-usage/src/gemini.rs` (new), `lib.rs`, `claude.rs` | the extension's files; `write_changed` shared |
| `crates/templum/src/workspace/waits.rs`, `balloon.rs` | `Waiting::Event`, `wait_for`, `look`, `needs_you` |
| `crates/templum/src/workspace/bus.rs` | the new calls; `cli_only` in `serve`; `NeedsYou` answers `wait_for`; `templum_in` |
| `crates/templum/src/workspace/activity.rs` (new), `mod.rs`, `blocks.rs`, `input.rs` | an agent's state from its screen |
| `crates/templum/src/workspace/wires.rs`, `history.rs` | `wire_blocks`, labels, `rename_wire`, `set_label`, `wire_labels`, `Change::Label` |
| `crates/templum/src/session.rs`, `workspace/persist.rs`, `workspace/mod.rs` | `SavedWire.label`, `Space::labels`, saved and read back |
| `crates/templum/src/menu/build.rs` | Rename wire… |
| `crates/templum/src/wires.rs`, `workspace/render.rs`, `bench.rs` | the label's pill; the benches label their wires |
| `crates/templum/src/settings/connections.rs` (new), `settings/mod.rs`, `settings/sections.rs` | Settings ▸ Connections |
| `crates/templum/src/workspace/usage.rs` | `setup_gemini`, `gemini_run` |
| `docs/specs/connections.md`, `docs/ARCHITECTURE.md`, `docs/minors.md`, `docs/plans/` | Task 0, Task 11 |

---

### Task 0: Save this plan

- [ ] **Step 1:** `git status` is clean on `development`, at 421734d or later (`git log --oneline -1`).
- [ ] **Step 2:** Copy this file to `docs/plans/2026-10-15-connections-batch-5.md`. In the spec's header, set
  `- **Status:** Batches 1 to 4 implemented; batch 5 planned ([plan](../plans/2026-10-15-connections-batch-5.md))`.
- [ ] **Step 3:** Commit:

```bash
git add docs/plans/2026-10-15-connections-batch-5.md docs/specs/connections.md
git commit -m "docs(connections): plan batch 5" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 1: `tools.rs`: `wait_for` and the command line's `wire` call

**Files:**
- Modify: `crates/templum-mcp/src/tools.rs` (`Call` :62-102, the `IDLE_WAIT` doc :22, `list()` :284-294, after
  `known` :409, `parse` arms :508-511 and :558-561, tests :614, :715-721, before `bad_arguments_say_which`)
- Modify: `crates/templum/src/workspace/bus.rs:379` (a stopgap arm: `call` matches `Call` exhaustively; Tasks 5 and 6
  replace it, as batch 4 did in 4a7594b)

**Interfaces:**
- Produces: `templum_mcp::tools::Call::WaitFor { block: String, event: Event, timeout: u64 }`,
  `Call::Wire { a: String, b: String }`, `#[derive(Clone, Copy, Debug, PartialEq)] pub enum tools::Event { Idle,
  NeedsYou, CommandFinished, Attention }`, `pub fn tools::cli_only(name: &str) -> bool`.

- [ ] **Step 1: Write the failing tests.** In `every_tool_listed_parses_its_example`, replace

```rust
            ("wait_for_idle", json!({ "block": "k9x1d", "timeout": 60 })),
            (
                "spawn_agent",
```

with

```rust
            ("wait_for_idle", json!({ "block": "k9x1d", "timeout": 60 })),
            (
                "wait_for",
                json!({ "block": "k9x1d", "event": "needs_you", "timeout": 60 }),
            ),
            (
                "spawn_agent",
```

In `the_tools_that_wait_have_their_defaults_and_limits`, after the `wait_for_idle` default assert (`:715-721`), add:

```rust
        assert_eq!(
            parse(
                "wait_for",
                &json!({ "block": "b", "event": "command_finished" })
            ),
            Ok(Call::WaitFor {
                block: "b".into(),
                event: Event::CommandFinished,
                timeout: IDLE_WAIT
            })
        );
        assert_eq!(
            parse(
                "wait_for",
                &json!({ "block": "b", "event": "idle", "timeout": 99_999 })
            ),
            Ok(Call::WaitFor {
                block: "b".into(),
                event: Event::Idle,
                timeout: MAX_WAIT
            })
        );
```

Before `#[test] fn bad_arguments_say_which() {`, add:

```rust
    /// The events tools/list offers are the ones wait_for takes; any other is refused, naming them.
    #[test]
    fn wait_for_takes_the_four_events_it_lists() {
        let tools = list();
        let listed = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "wait_for")
            .unwrap();
        assert_eq!(listed["inputSchema"]["required"], json!(["block", "event"]));
        let events: Vec<Event> = listed["inputSchema"]["properties"]["event"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(
                |name| match parse("wait_for", &json!({ "block": "b", "event": name })) {
                    Ok(Call::WaitFor { event, .. }) => event,
                    other => panic!("{name}: {other:?}"),
                },
            )
            .collect();
        assert_eq!(
            events,
            [
                Event::Idle,
                Event::NeedsYou,
                Event::CommandFinished,
                Event::Attention
            ]
        );
        for bad in [
            json!({ "block": "b" }),
            json!({ "block": "b", "event": "done" }),
            json!({ "block": "b", "event": 1 }),
        ] {
            let err = parse("wait_for", &bad).unwrap_err();
            assert!(
                err.contains("\"event\"") && err.contains("needs_you") && err.contains("attention"),
                "{err}"
            );
        }
        assert!(parse("wait_for", &json!({ "block": "b", "event": "idle", "timeout": 0 })).is_err());
    }

    /// `wire` is the command line's: agents never see it listed, and only `templum wire` makes it.
    #[test]
    fn wire_is_the_command_lines_alone() {
        assert!(!known("wire") && cli_only("wire"));
        assert!(!cli_only("wait_for") && !cli_only("whoami"));
        assert_eq!(
            parse("wire", &json!({ "a": "b7k2m", "b": "k9x1d" })),
            Ok(Call::Wire {
                a: "b7k2m".into(),
                b: "k9x1d".into()
            })
        );
        let err = parse("wire", &json!({ "a": "b7k2m" })).unwrap_err();
        assert!(err.contains("\"b\""), "{err}");
    }
```

- [ ] **Step 2:** Run `cargo test -p templum-mcp tools::`. Expected: FAIL to compile (`no variant named WaitFor`,
  `cannot find type Event`, `cannot find function cli_only`).

- [ ] **Step 3: Write the code.** Change the doc of `IDLE_WAIT` to `/// How long \`wait_for_idle\` and \`wait_for\`
  wait, seconds, unless asked.` In `Call`, after `WaitForIdle { block: String, timeout: u64 },` add:

```rust
    WaitFor {
        block: String,
        event: Event,
        timeout: u64,
    },
```

and replace the end of the enum

```rust
    GitStatus {
        block: Option<String>,
    },
}
```

with

```rust
    GitStatus {
        block: Option<String>,
    },
    /// Wires two blocks: `templum wire`'s alone, never listed to agents.
    Wire {
        a: String,
        b: String,
    },
}

/// What `wait_for` waits for in a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// The agent finishes its turn; at once when it is idle.
    Idle,
    /// The agent waits for its person (a permission, a question); at once when it already does.
    NeedsYou,
    /// The next command that finishes in the terminal (needs the shell's integration).
    CommandFinished,
    /// The terminal rings or sends a notification.
    Attention,
}
```

In `list()`, after the `wait_for_idle` entry (whose `"required": ["block"]` closes at `:293`), add:

```rust
        {
            "name": "wait_for",
            "description": "Waits for an event in a block connected to you. idle: the agent finishes its turn \
                            (at once when it is idle), with its last answer. needs_you: the agent waits for its \
                            person, at a permission prompt or a question (at once when it already does). \
                            command_finished: the next command that finishes in the terminal, with its output \
                            (needs the shell's integration). attention: the terminal rings or sends a \
                            notification. It may take minutes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "block": block,
                    "event": {
                        "type": "string",
                        "enum": ["idle", "needs_you", "command_finished", "attention"],
                    },
                    "timeout": seconds(IDLE_WAIT),
                },
                "required": ["block", "event"],
            },
        },
```

After `known`'s closing `}` add:

```rust

/// `name` is a call only the command line makes (`templum wire`): not listed, so agents never
/// see it.
pub fn cli_only(name: &str) -> bool {
    name == "wire"
}
```

In `parse`, after the `"wait_for_idle" => Call::WaitForIdle { … },` arm, add:

```rust
        "wait_for" => Call::WaitFor {
            block: text("block")?,
            event: match args["event"].as_str() {
                Some("idle") => Event::Idle,
                Some("needs_you") => Event::NeedsYou,
                Some("command_finished") => Event::CommandFinished,
                Some("attention") => Event::Attention,
                _ => {
                    return Err(format!(
                        "{name} needs \"event\": idle, needs_you, command_finished or attention"
                    ));
                }
            },
            timeout: seconds(IDLE_WAIT)?,
        },
```

and after the `"git_status"` arm:

```rust
        "wire" => Call::Wire {
            a: text("a")?,
            b: text("b")?,
        },
```

- [ ] **Step 4:** Run `cargo test -p templum-mcp tools::`. Expected: PASS.

- [ ] **Step 5: The stopgap arm.** In `crates/templum/src/workspace/bus.rs`, after
  `Call::SpawnAgent(spawn) => return self.spawn_agent(terminal, spawn, window, cx),` add
  `Call::WaitFor { .. } | Call::Wire { .. } => Reply::err("Not in this Templum yet."),`. Run
  `cargo check --workspace --all-targets`. Expected: PASS. (Only keeps the build green; Tasks 5 and 6 replace it.)

- [ ] **Step 6:** `cargo fmt --all && cargo clippy -p templum-mcp -p templum --all-targets -- -D warnings`: clean.

- [ ] **Step 7: Commit.**

```bash
git add crates/templum-mcp/src/tools.rs crates/templum/src/workspace/bus.rs
git commit -m "feat(connections): describe wait_for to agents and take templum wire's call" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 2: `cli.rs`: `templum wait --for`, `templum wire`, `--json` on every usage

**Files:**
- Modify: `crates/templum-mcp/src/cli.rs` (`VERBS` :13-16, `WAIT` :19, the wait arms :177-184, the `git` arms near
  :273 and :286; tests :577-580, :611-615, `only_the_verbs_are_commands` :697-703)

**Interfaces:**
- Consumes: Task 1's `wait_for` and `wire` names.
- Produces: `templum wait <block> [--for <event>] [--timeout <s>]` (`wait_for_idle` without `--for`, `wait_for` with
  it, the event passed through: `parse` refuses an unknown one, exit 1 naming the four); `templum wire <a> <b>`.

- [ ] **Step 1: Write the failing tests.** In `delegate_reply_and_wait_send_their_calls`, replace

```rust
            (
                &["wait", "k9x1d", "--timeout", "60"][..],
                json!({ "name": "wait_for_idle", "arguments": { "block": "k9x1d", "timeout": 60 } }),
            ),
```

with

```rust
            (
                &["wait", "k9x1d"][..],
                json!({ "name": "wait_for_idle", "arguments": { "block": "k9x1d" } }),
            ),
            (
                &["wait", "k9x1d", "--timeout", "60"][..],
                json!({ "name": "wait_for_idle", "arguments": { "block": "k9x1d", "timeout": 60 } }),
            ),
            (
                &["wait", "k9x1d", "--for", "needs_you"][..],
                json!({ "name": "wait_for", "arguments": { "block": "k9x1d", "event": "needs_you" } }),
            ),
            (
                // The flags in either order.
                &["wait", "k9x1d", "--timeout", "60", "--for", "command_finished"][..],
                json!({ "name": "wait_for", "arguments": { "block": "k9x1d", "timeout": 60, "event": "command_finished" } }),
            ),
```

In the same test, replace

```rust
        assert_eq!(
            run_with(&["wait", "k9x1d", "--for", "idle"], &env(&bus), "").0,
            2,
            "--for is batch 5's"
        );
```

with

```rust
        assert_eq!(
            run_with(&["wait", "k9x1d", "--for"], &env(&bus), "").0,
            2,
            "no event"
        );
        assert_eq!(
            run_with(&["wait", "k9x1d", "--timeout", "soon"], &env(&bus), "").0,
            2,
            "no number"
        );
```

In `only_the_verbs_are_commands`, add `assert!(is_verb("wire"));` after `assert!(is_verb("ask"));`. After that test
add:

```rust
    #[test]
    fn wire_sends_its_call_and_json_prints_the_whole_result() {
        let (seen, calls) = std::sync::mpsc::channel();
        let (_dir, bus) = test_bus(move |m| {
            if let Incoming::Request { id, params, .. } = &m.incoming {
                let _ = seen.send(params.clone());
                let _ = m.reply.send(tool_result(id, &Reply::ok("Wired.")));
            }
        });
        let (code, out, _) = run_with(&["wire", "b7k2m", "k9x1d", "--json"], &env(&bus), "");
        assert_eq!(code, 0);
        assert_eq!(
            serde_json::from_str::<Value>(&out).unwrap(),
            json!({ "content": [{ "type": "text", "text": "Wired." }] })
        );
        assert_eq!(
            calls.recv_timeout(LIMIT).unwrap(),
            json!({ "name": "wire", "arguments": { "a": "b7k2m", "b": "k9x1d" } })
        );
        let (code, _, err) = run_with(&["wire", "b7k2m"], &env(&bus), "");
        assert_eq!(code, 2);
        assert!(err.starts_with("usage: templum wire"), "{err}");
    }

    /// Every verb that makes a tool call takes --json, and its usage says so.
    #[test]
    fn every_verbs_usage_offers_json() {
        let (none, one): ([String; 0], [String; 1]) = ([], ["x".into()]);
        for verb in VERBS.iter().copied().filter(|v| !matches!(*v, "mcp" | "hook")) {
            let usages: Vec<&str> = [&none[..], &one[..]]
                .into_iter()
                .filter_map(|args| call_of(verb, args).err())
                .collect();
            assert!(!usages.is_empty(), "{verb}");
            for usage in usages {
                assert!(usage.starts_with(verb), "{verb}: {usage}");
                assert!(usage.ends_with("[--json]"), "{usage}");
            }
        }
    }
```

(`every_verbs_usage_offers_json` already passes: a guard, which catches a missing `WIRE` once `wire` joins `VERBS`.)

- [ ] **Step 2:** Run `cargo test -p templum-mcp cli::`. Expected: FAIL (`--for needs_you` exits 2; `is_verb("wire")`
  false; `wire …` exits 2).

- [ ] **Step 3: Write the code.** Replace `VERBS` and `WAIT`:

```rust
const VERBS: &[&str] = &[
    "mcp", "hook", "whoami", "blocks", "read", "send", "notify", "ask", "connect", "delegate",
    "reply", "wait", "spawn", "kanban", "note", "open", "git", "wire",
];
```

```rust
const WAIT: &str =
    "wait <block> [--for idle|needs_you|command_finished|attention] [--timeout <seconds>] [--json]";
const WIRE: &str = "wire <a> <b> [--json]";
```

Replace the two `("wait", …)` arms with:

```rust
        ("wait", [block, more @ ..]) => {
            let mut arguments = json!({ "block": block });
            let mut more = more.iter();
            while let Some(flag) = more.next() {
                let value = *more.next().ok_or(WAIT)?;
                match *flag {
                    "--for" => arguments["event"] = json!(value),
                    "--timeout" => {
                        let seconds: u64 = value.parse().map_err(|_| WAIT)?;
                        arguments["timeout"] = json!(seconds);
                    }
                    _ => return Err(WAIT),
                }
            }
            let name = if arguments.get("event").is_some() {
                "wait_for"
            } else {
                "wait_for_idle"
            };
            (name, arguments)
        }
```

After `("git", ["status", block]) => ("git_status", json!({ "block": block })),` add
`("wire", [a, b]) => ("wire", json!({ "a": a, "b": b })),`, and after
`("git", _) => return Err("git status [<block>] [--json]"),` add `("wire", _) => return Err(WIRE),`.

- [ ] **Step 4:** Run `cargo test -p templum-mcp cli::`. Expected: PASS.
- [ ] **Step 5:** `cargo fmt --all && cargo clippy -p templum-mcp --all-targets -- -D warnings`: clean.
- [ ] **Step 6: Commit.**

```bash
git add crates/templum-mcp/src/cli.rs
git commit -m "feat(connections): templum wait --for an event, and templum wire, in a Templum terminal" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 3: Gemini CLI's hooks become events; `templum hook gemini`

**Files:**
- Modify: `crates/templum-mcp/src/hooks.rs` (after `codex` :79; test after `codexs_notify_is_a_finished_turn`)
- Modify: `crates/templum-mcp/src/cli.rs` (`hook_event` :376-387; the hook test :656-664)

**Interfaces:**
- Produces: `pub fn templum_mcp::hooks::gemini(input: &serde_json::Value) -> Option<AgentEvent>`; `templum hook gemini`
  reads the hook's JSON on stdin.

- [ ] **Step 1: Write the failing test** in hooks.rs's tests:

```rust
    #[test]
    fn gemini_clis_hooks_become_events_as_claude_codes() {
        let event = |input: Value| gemini(&input);
        for source in ["startup", "resume", "clear"] {
            assert_eq!(
                event(json!({ "hook_event_name": "SessionStart", "source": source })),
                Some(AgentEvent::Started),
                "{source}"
            );
        }
        assert_eq!(
            event(json!({ "hook_event_name": "BeforeAgent", "prompt": "hi" })),
            Some(AgentEvent::Working)
        );
        // After a permission the person granted, the tool's end says it works again.
        assert_eq!(
            event(json!({ "hook_event_name": "AfterTool", "tool_name": "run_shell_command" })),
            Some(AgentEvent::Working)
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "AfterAgent",
                "prompt": "hi",
                "prompt_response": "Done.",
            })),
            Some(AgentEvent::Stopped {
                message: Some("Done.".into())
            })
        );
        assert_eq!(
            event(json!({ "hook_event_name": "AfterAgent" })),
            Some(AgentEvent::Stopped { message: None })
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "ToolPermission",
                "message": "Tool Shell requires execution",
                "details": {},
            })),
            Some(AgentEvent::NeedsYou {
                message: Some("Tool Shell requires execution".into())
            }),
            "waiting for you as soon as the dialog opens"
        );
        assert_eq!(
            event(json!({
                "hook_event_name": "Notification",
                "notification_type": "Other",
                "message": "x",
            })),
            None,
            "other notifications say nothing of the agent's state"
        );
        assert_eq!(
            event(json!({ "hook_event_name": "SessionEnd", "reason": "exit" })),
            Some(AgentEvent::Ended)
        );
        for other in ["BeforeTool", "BeforeModel", "AfterModel", "PreCompress"] {
            assert_eq!(event(json!({ "hook_event_name": other })), None, "{other}");
        }
        assert_eq!(event(json!("garbage")), None);
    }
```

- [ ] **Step 2:** Run `cargo test -p templum-mcp gemini`. Expected: FAIL to compile (`cannot find function gemini`).
- [ ] **Step 3: Write the code** after `codex`'s closing `}`:

```rust

/// A Gemini CLI hook's input (as of 0.63), read as Claude Code's. None for what Templum doesn't
/// follow.
pub fn gemini(input: &Value) -> Option<AgentEvent> {
    let text = |key: &str| input[key].as_str().map(str::to_string);
    Some(match input["hook_event_name"].as_str()? {
        "SessionStart" => AgentEvent::Started,
        // After a permission the person granted, the tool's end says it works again.
        "BeforeAgent" | "AfterTool" => AgentEvent::Working,
        "AfterAgent" => AgentEvent::Stopped {
            message: text("prompt_response"),
        },
        // Its permission dialog opened. The hook prints nothing, so the dialog still decides.
        "Notification" if input["notification_type"] == "ToolPermission" => {
            AgentEvent::NeedsYou {
                message: text("message"),
            }
        }
        "SessionEnd" => AgentEvent::Ended,
        _ => return None,
    })
}
```

- [ ] **Step 4:** Run `cargo test -p templum-mcp gemini`. Expected: PASS.
- [ ] **Step 5: Write the failing CLI test.** In `a_hook_sends_its_event_prints_nothing_and_never_fails`, after the
  Codex assert (`events.recv_timeout(LIMIT).unwrap().1 == … "Fixed." …`), add:

```rust
        // Gemini CLI's, on stdin as Claude Code's: its stdout must stay empty too.
        let gemini = r#"{"hook_event_name":"AfterAgent","prompt_response":"Done."}"#;
        assert_eq!(
            run_with(&["hook", "gemini"], &env(&bus), gemini),
            (0, String::new(), String::new())
        );
        assert_eq!(
            events.recv_timeout(LIMIT).unwrap().1,
            Incoming::Notification {
                method: "templum/event".into(),
                params: json!({ "event": "stopped", "message": "Done." }),
            }
        );
```

- [ ] **Step 6:** Run `cargo test -p templum-mcp a_hook_sends`. Expected: FAIL (`recv_timeout` times out after 5 s).
- [ ] **Step 7: Write the code.** Replace `hook_event`:

```rust
/// The event an agent's hook hands over: Claude Code's and Gemini CLI's on stdin, Codex's as an
/// argument.
fn hook_event(args: &[String], stdin: &mut dyn Read) -> Option<hooks::AgentEvent> {
    let mut on_stdin = || {
        let mut input = String::new();
        stdin.read_to_string(&mut input).ok()?;
        serde_json::from_str::<Value>(&input).ok()
    };
    match args.first()?.as_str() {
        "claude" => hooks::claude(&on_stdin()?),
        "gemini" => hooks::gemini(&on_stdin()?),
        "codex" => hooks::codex(&serde_json::from_str(args.get(1)?).ok()?),
        _ => None,
    }
}
```

- [ ] **Step 8:** Run `cargo test -p templum-mcp hook`. Expected: PASS.
- [ ] **Step 9:** `cargo fmt --all && cargo clippy -p templum-mcp --all-targets -- -D warnings`: clean.
- [ ] **Step 10: Commit.**

```bash
git add crates/templum-mcp/src/hooks.rs crates/templum-mcp/src/cli.rs
git commit -m "feat(connections): follow a Gemini CLI's turns through its hooks" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 4: The Gemini CLI extension (`templum-usage`)

**Files:**
- Create: `crates/templum-usage/src/gemini.rs`
- Modify: `crates/templum-usage/src/lib.rs:10` (`pub mod gemini;`), `crates/templum-usage/src/claude.rs:254`
  (`write_changed` becomes `pub(crate)`)

**Interfaces:**
- Produces: `templum_usage::gemini::{NAME: &str = "templum", UNLINK: [&str; 3], extension(data: &Path, windows: bool)
  -> Option<PathBuf> /* <data>/gemini */, hook_command(bin: &str, windows: bool) -> Option<String>,
  link_args(folder: &Path) -> Vec<OsString>, installed(gemini_dir: &Path) -> bool}`.

- [ ] **Step 1: Write the failing tests.** In lib.rs replace `pub mod codex;` with `pub mod codex;\npub mod gemini;`;
  in claude.rs make `write_changed` `pub(crate)`. Create `crates/templum-usage/src/gemini.rs`:

```rust
//! Gemini CLI: the extension that gives it Templum's MCP server and hooks.
//!
//! Gemini CLI has no flag for either, and ignores a system settings file root doesn't own, so
//! Templum writes an extension to its data folder that the person links once, agreeing:
//! `gemini extensions link <data>/gemini --consent`. It names `<data>/bin/templum`, the link
//! Templum points at itself at each start, so it outlives upgrades.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::claude::write_changed;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn read(path: &Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn the_extension_names_templums_server_and_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let folder = extension(dir.path(), false).unwrap();
        assert_eq!(folder, dir.path().join("gemini"));
        let bin = dir.path().join("bin").join("templum");
        let bin = bin.to_string_lossy();
        assert_eq!(
            read(&folder.join("gemini-extension.json")),
            json!({
                "name": "templum",
                "version": "1.0.0",
                "mcpServers": {
                    "templum": { "command": bin, "args": ["mcp"], "timeout": 3_600_000 }
                },
            })
        );
        let hook = json!([{
            "hooks": [{ "type": "command", "command": format!("'{bin}' hook gemini"), "timeout": 5000 }]
        }]);
        assert_eq!(
            read(&folder.join("hooks").join("hooks.json")),
            json!({ "hooks": {
                "SessionStart": hook,
                "BeforeAgent": hook,
                "AfterTool": hook,
                "AfterAgent": hook,
                "Notification": hook,
                "SessionEnd": hook,
            }})
        );
    }

    #[test]
    fn an_unchanged_extension_isnt_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let folder = extension(dir.path(), false).unwrap();
        let files = [
            folder.join("gemini-extension.json"),
            folder.join("hooks").join("hooks.json"),
        ];
        let long_ago = UNIX_EPOCH + Duration::from_secs(1_000_000);
        for file in &files {
            let open = std::fs::File::options().write(true).open(file).unwrap();
            open.set_modified(long_ago).unwrap();
        }
        extension(dir.path(), false).unwrap();
        for file in &files {
            let modified = std::fs::metadata(file).unwrap().modified().unwrap();
            assert_eq!(modified, long_ago, "{file:?}");
        }
    }

    #[test]
    fn a_hook_runs_templum_in_single_quotes_through_bash_or_powershell() {
        assert_eq!(
            hook_command("/home/u/.local/share/templum/bin/templum", false).as_deref(),
            Some("'/home/u/.local/share/templum/bin/templum' hook gemini")
        );
        assert_eq!(
            hook_command(r"C:\Users\Jane Doe\AppData\Roaming\templum\bin\templum.exe", true)
                .as_deref(),
            Some(r"& 'C:\Users\Jane Doe\AppData\Roaming\templum\bin\templum.exe' hook gemini")
        );
        // Single quotes keep $, ` and " as they are; only a ' or a line break can't go inside.
        assert_eq!(
            hook_command("/home/$x/templum", false).as_deref(),
            Some("'/home/$x/templum' hook gemini")
        );
        assert_eq!(hook_command("/home/o'neil/templum", false), None);
        assert_eq!(hook_command("/home/o'neil/templum", true), None);
        assert_eq!(hook_command("/home/a\nb/templum", false), None);
        assert_eq!(hook_command("", false), None);
        let dir = tempfile::tempdir().unwrap();
        let folder = extension(dir.path(), true).unwrap();
        let bin = dir.path().join("bin").join("templum.exe");
        assert_eq!(
            read(&folder.join("hooks").join("hooks.json"))["hooks"]["AfterAgent"][0]["hooks"][0]
                ["command"],
            format!("& '{}' hook gemini", bin.display())
        );
        assert_eq!(
            read(&folder.join("gemini-extension.json"))["mcpServers"]["templum"]["command"],
            bin.display().to_string()
        );
        let quoted = dir.path().join("o'neil");
        assert_eq!(extension(&quoted, false), None);
        assert!(!quoted.exists(), "nothing written");
    }

    #[test]
    fn gemini_has_the_extension_once_linked() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!installed(dir.path()));
        std::fs::create_dir_all(dir.path().join("extensions").join("templum")).unwrap();
        assert!(installed(dir.path()));
        assert_eq!(
            link_args(Path::new("/data/gemini")),
            ["extensions", "link", "/data/gemini", "--consent"].map(OsString::from)
        );
        assert_eq!(UNLINK, ["extensions", "uninstall", "templum"]);
    }
}
```

- [ ] **Step 2:** Run `cargo test -p templum-usage gemini::`. Expected: FAIL to compile (`extension`, `hook_command`,
  `installed`, `link_args`, `UNLINK` not found).
- [ ] **Step 3: Write the code** between `use crate::claude::write_changed;` and `#[cfg(test)]`:

```rust

/// The extension's name, and its folder's in `~/.gemini/extensions`.
pub const NAME: &str = "templum";
/// What `gemini` is given to remove the extension.
pub const UNLINK: [&str; 3] = ["extensions", "uninstall", NAME];
/// The Gemini CLI hooks Templum follows a session's turns with.
const HOOKED: [&str; 6] = [
    "SessionStart",
    "BeforeAgent",
    "AfterTool",
    "AfterAgent",
    "Notification",
    "SessionEnd",
];

/// Writes the extension to `<data>/gemini`, for Windows when `windows`, and returns its folder.
/// None when Templum's program can't go in a hook's command, or a file can't be written.
pub fn extension(data: &Path, windows: bool) -> Option<PathBuf> {
    let bin = data
        .join("bin")
        .join(if windows { "templum.exe" } else { "templum" });
    let bin = bin.to_string_lossy();
    let command = hook_command(&bin, windows)?;
    let hook = json!([{ "hooks": [{ "type": "command", "command": command, "timeout": 5000 }] }]);
    let hooks: serde_json::Map<String, serde_json::Value> = HOOKED
        .iter()
        .map(|event| (event.to_string(), hook.clone()))
        .collect();
    let folder = data.join("gemini");
    // Gemini CLI gives up on a tool after 10 minutes unless told: Templum's waits last an hour
    // at most (ms).
    let server = json!({ "command": bin, "args": ["mcp"], "timeout": 3_600_000 });
    write_changed(
        &folder.join("gemini-extension.json"),
        &json!({ "name": NAME, "version": "1.0.0", "mcpServers": { "templum": server } }),
    )?;
    write_changed(
        &folder.join("hooks").join("hooks.json"),
        &json!({ "hooks": hooks }),
    )?;
    Some(folder)
}

/// `bin hook gemini`, as Gemini CLI runs a hook: through bash on Unix, PowerShell on Windows,
/// with `bin` in single quotes. None when `bin` holds a `'` or a line break.
pub fn hook_command(bin: &str, windows: bool) -> Option<String> {
    if bin.is_empty() || bin.contains(['\'', '\n', '\r']) {
        return None;
    }
    Some(if windows {
        format!("& '{bin}' hook gemini")
    } else {
        format!("'{bin}' hook gemini")
    })
}

/// What `gemini` is given to install the extension in `folder`, the person agreeing: a link,
/// so the files Templum rewrites are the ones Gemini CLI reads.
pub fn link_args(folder: &Path) -> Vec<OsString> {
    vec![
        "extensions".into(),
        "link".into(),
        folder.into(),
        "--consent".into(),
    ]
}

/// Gemini CLI, whose folder is `gemini_dir` (`~/.gemini`), has the extension.
pub fn installed(gemini_dir: &Path) -> bool {
    gemini_dir.join("extensions").join(NAME).exists()
}
```

  (`write_changed` goes through `write_atomic`, which makes the missing folders, `hooks/` included,
  `claude.rs:126-135`. The `o'neil` test pins "nothing written": `hook_command` refuses before any write.)
- [ ] **Step 4:** Run `cargo test -p templum-usage`. Expected: PASS (4 new tests, claude's still pass).
- [ ] **Step 5:** `cargo fmt --all && cargo clippy -p templum-usage --all-targets -- -D warnings`: clean.
- [ ] **Step 6: Commit.**

```bash
git add crates/templum-usage/src/gemini.rs crates/templum-usage/src/lib.rs crates/templum-usage/src/claude.rs
git commit -m "feat(connections): write the Gemini CLI extension that gives it Templum's server and hooks" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 5: `wait_for` in the app

**Files:**
- Modify: `crates/templum/src/workspace/waits.rs` (whole: `Waiting`, `summary`, `wait`, `expire`, `orphaned`,
  `balloon`, `tell_asking`; new `wait_for`, `needs_you`, `look`, `look_until_it_comes`)
- Modify: `crates/templum/src/balloon.rs:32-34, :104-134` (the matches on `Waiting` gain `Waiting::Event`)
- Modify: `crates/templum/src/workspace/bus.rs` (`call`: the `WaitFor` arm; `agent_event`: `NeedsYou`)
- Test: `crates/templum/src/workspace/tests/delegate.rs`

**Interfaces:**
- Consumes: `tools::Event`, `Call::WaitFor` (Task 1); `Workspace::wait_for_idle` (`delegate.rs:273`); `target`
  (`bus.rs:590`); `Workspace::balloon` (`waits.rs:318`).
- Produces: `Waiting::Event { of: String, uid: String, event: Event, since: (u64, u64) }`;
  `Workspace::wait_for(&self, from: &str, block: &str, event: Event, timeout: u64) -> Outcome`;
  `Workspace::needs_you(&mut self, terminal: &str, what: Option<&str>, cx)`; `pub(super) const LAST_LINES` (Task 7).

- [ ] **Step 1: Write the failing tests** at the end of `tests/delegate.rs` (`result_of` joins the `use super::bus::{…}`
  import):

```rust
#[gpui_kit::test]
fn wait_for_needs_you_now_or_when_it_asks(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (worker, them) = me(&ws, cx, theirs);
    let needs_you = json!({ "block": them, "event": "needs_you" });
    let answers = call_later(handle, &ws, cx, &terminal, 1, "wait_for", needs_you.clone());
    assert!(answers.try_recv().is_err(), "it waits");
    event(
        handle,
        &ws,
        cx,
        &worker,
        json!({ "event": "needs_you", "message": "Allow Bash?" }),
    );
    let asks = format!("{them} is waiting for its person: Allow Bash?");
    assert_eq!(answered(&answers), (false, asks.clone()));
    let (failed, text) = call(handle, &ws, cx, &terminal, "wait_for", needs_you.clone());
    assert_eq!((failed, text), (false, asks), "at once while it still asks");
    // Its own question, in its balloon, is a wait for its person too.
    event(handle, &ws, cx, &worker, json!({ "event": "working" }));
    let answers = call_later(handle, &ws, cx, &terminal, 2, "wait_for", needs_you);
    let _question = call_later(
        handle,
        &ws,
        cx,
        &worker,
        3,
        "ask_user",
        json!({ "question": "Which branch?" }),
    );
    assert_eq!(
        answered(&answers),
        (false, format!("{them} is waiting for its person: Which branch?"))
    );
}

#[gpui_kit::test]
fn wait_for_idle_is_wait_for_idle_and_the_rest_need_a_wire(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, uid) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "wait_for",
        json!({ "block": them, "event": "idle" }),
    );
    assert!(!failed && text.contains("is idle"), "{text}");
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "wait_for",
        json!({ "block": uid, "event": "attention" }),
    );
    assert!(failed && text.contains("yours"), "{text}");
    ws.update(cx, |ws, cx| {
        ws.disconnect(Wire::new(mine, theirs).unwrap(), cx);
    });
    let (failed, text) = call(
        handle,
        &ws,
        cx,
        &terminal,
        "wait_for",
        json!({ "block": them, "event": "attention" }),
    );
    assert!(failed && text.contains("isn't connected to you"), "{text}");
}

/// `cat` in block `id` prints `bytes` back, as a program would.
#[cfg(unix)]
fn prints(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId, bytes: &'static [u8]) {
    ws.read_with(cx, |ws, _| {
        ws.space.terminal(id).unwrap().term.write(bytes.to_vec())
    });
}

/// The answer on `answers`, the test's clock running a look at a time: the terminal's own thread
/// reads what `cat` prints back.
#[cfg(unix)]
fn heard(cx: &mut TestAppContext, answers: &std::sync::mpsc::Receiver<String>) -> (bool, String) {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        after(cx, 250);
        if let Ok(line) = answers.try_recv() {
            return result_of(&line);
        }
        assert!(Instant::now() < deadline, "no answer");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[gpui_kit::test]
fn wait_for_a_finished_command_or_a_call_for_attention(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let event = |name: &str| json!({ "block": them, "event": name });
    let finished = call_later(handle, &ws, cx, &terminal, 1, "wait_for", event("command_finished"));
    let attention = call_later(handle, &ws, cx, &terminal, 2, "wait_for", event("attention"));
    after(cx, 500);
    assert!(finished.try_recv().is_err() && attention.try_recv().is_err(), "nothing yet");
    // A command starts and ends with code 3: OSC 133 C, then D;3.
    prints(&ws, cx, theirs, b"\x1b]133;C\x07\x1b]133;D;3\x07\r");
    let (failed, text) = heard(cx, &finished);
    assert!(
        !failed && text.starts_with(&format!("A command finished in {them}, with exit code 3.")),
        "{text}"
    );
    assert!(attention.try_recv().is_err(), "a command isn't a call for attention");
    prints(&ws, cx, theirs, b"\x1b]777;notify;Build;done\x07\r");
    assert_eq!(heard(cx, &attention), (false, format!("{them} asks for attention: done")));
    let bell = call_later(handle, &ws, cx, &terminal, 3, "wait_for", event("attention"));
    prints(&ws, cx, theirs, b"\x07\r");
    assert_eq!(heard(cx, &bell), (false, format!("{them} rang its bell.")));
}

#[gpui_kit::test]
fn a_wait_for_times_out_goes_with_its_block_and_shows_no_balloon(cx: &mut TestAppContext) {
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let soon = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        1,
        "wait_for",
        json!({ "block": them, "event": "attention", "timeout": 1 }),
    );
    assert!(
        ws.read_with(cx, |ws, _| ws.balloon(&terminal).is_none()),
        "a wait asks the person nothing"
    );
    after(cx, 1000);
    let (failed, text) = answered(&soon);
    assert!(
        failed && text.starts_with(&format!("No call for attention in {them} within 1 seconds.")),
        "{text}"
    );
    let closed = call_later(
        handle,
        &ws,
        cx,
        &terminal,
        2,
        "wait_for",
        json!({ "block": them, "event": "command_finished" }),
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.remove_block(theirs, window, cx))
    })
    .unwrap();
    frame(handle, cx);
    assert_eq!(
        answered(&closed),
        (true, format!("{them} was closed while you waited."))
    );
}
```

- [ ] **Step 2:** Run `cargo test -p templum wait_for`. Expected: FAIL ("Not in this Templum yet." from Task 1's
  stopgap).
- [ ] **Step 3: Write the code in `waits.rs`.** Imports: add `use templum_mcp::tools::Event;`,
  `use super::AgentState;`, and change `use super::bus::Outcome;` to `use super::bus::{Outcome, target};`. Make
  `LAST_LINES` `pub(super)`. Add after it:

```rust
/// How often a `wait_for` a command or an alert looks at its terminal's counters.
const EVENT_LOOK: Duration = Duration::from_millis(250);
```

Add to `Waiting`:

```rust
    /// `wait_for`: `event` in the terminal `of` (the block `uid`). `since`: the commands that had
    /// finished there and the alerts it had sent when the call came (`Status.finished`,
    /// `Status.attention`).
    Event {
        of: String,
        uid: String,
        event: Event,
        since: (u64, u64),
    },
```

In `summary`, make the last arm `Waiting::Turn { .. } | Waiting::Event { .. } => String::new(),`. In `wait`, replace

```rust
        if !matches!(wait.what, Waiting::Turn { .. }) {
            self.tell_asking(&wait, window, cx);
        }
        self.waits.push(wait);
        cx.notify();
```

with

```rust
        let looks = matches!(
            wait.what,
            Waiting::Event {
                event: Event::CommandFinished | Event::Attention,
                ..
            }
        );
        if matches!(wait.what, Waiting::Question { .. } | Waiting::Connect { .. }) {
            self.tell_asking(&wait, window, cx);
            // Its block waits for its person now.
            self.needs_you(&wait.from, Some(&wait.summary()), cx);
        }
        self.waits.push(wait);
        if looks {
            self.look_until_it_comes(key, cx);
        }
        cx.notify();
```

In `expire`, replace the `Waiting::Turn { of, uid } => { … }` arm with the two arms below (and the `lines` closure
before the `match`):

```rust
        let screen = |of: &str| {
            let lines = self
                .pane(of)
                .map(|pane| pane.term.shared().lines(LAST_LINES).join("\n"))
                .unwrap_or_default();
            tools::tail(&lines, tools::MAX_TEXT).to_string()
        };
```

```rust
            Waiting::Turn { of, uid } => Reply::err(format!(
                "No answer from {uid} within {seconds} seconds; it may still be working \
                 (wait_for_idle(\"{uid}\") waits again). The last lines of its screen:\n{}",
                screen(of)
            )),
            Waiting::Event { of, uid, event, .. } => Reply::err(format!(
                "No {} in {uid} within {seconds} seconds. The last lines of its screen:\n{}",
                awaited(*event),
                screen(of)
            )),
```

In `orphaned`, before the last arm add

```rust
            Waiting::Event { of, uid, .. } if self.pane(of).is_none() => {
                Some(Reply::err(format!("{uid} was closed while you waited.")))
            }
```

and make the last arm `Waiting::Question { .. } | Waiting::Connect { .. } | Waiting::Turn { .. } |
Waiting::Event { .. } => None,`. In `balloon`, replace `!matches!(w.what, Waiting::Turn { .. })` with
`matches!(w.what, Waiting::Question { .. } | Waiting::Connect { .. })`. In `tell_asking`, make the last arm
`Waiting::Turn { .. } | Waiting::Event { .. } => return,`. Add to `impl Workspace`:

```rust
    /// `wait_for` from the terminal `from`: `event` in the terminal `block`. `idle` is
    /// `wait_for_idle`; `needs_you` answers at once when its agent, or its balloon, already asks
    /// its person; the others wait for the next command or alert.
    pub(super) fn wait_for(&self, from: &str, block: &str, event: Event, timeout: u64) -> Outcome {
        if event == Event::Idle {
            return self.wait_for_idle(from, block, timeout);
        }
        let (space_id, me) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let to = match target(space, me, block) {
            Ok(to) => to,
            Err(refused) => return Outcome::Now(refused),
        };
        if to == me {
            return Outcome::Now(Reply::err("This block is yours: wait for another block."));
        }
        let Some(pane) = space.terminal(to) else {
            return Outcome::Now(Reply::err(format!("{block} isn't a terminal.")));
        };
        let of = pane.term.id().to_string();
        if event == Event::NeedsYou {
            let asks = match &pane.state {
                Some(AgentState::NeedsYou(what)) => Some(what.clone()),
                _ => self.balloon(&of).map(|(wait, _)| Some(wait.summary())),
            };
            if let Some(what) = asks {
                return Outcome::Now(waiting_for_person(block, what.as_deref()));
            }
        }
        let status = pane.term.status();
        let what = Waiting::Event {
            of,
            uid: block.to_string(),
            event,
            since: (status.finished, status.attention),
        };
        Outcome::Wait(what, timeout)
    }

    /// The block of the terminal `terminal` waits for its person (its agent asks for `what`, or
    /// its balloon does): the `wait_for(needs_you)` on it are answered.
    pub(super) fn needs_you(&mut self, terminal: &str, what: Option<&str>, cx: &mut Context<Self>) {
        let waiting: Vec<(u64, String)> = self
            .waits
            .iter()
            .filter_map(|w| match &w.what {
                Waiting::Event {
                    of,
                    uid,
                    event: Event::NeedsYou,
                    ..
                } if of == terminal => Some((w.key, uid.clone())),
                _ => None,
            })
            .collect();
        for (key, uid) in waiting {
            self.answer(key, waiting_for_person(&uid, what), cx);
        }
    }

    /// Looks at the terminal a `wait_for(command_finished | attention)` watches, every EVENT_LOOK,
    /// until the wait `key` is gone: the terminal's own thread counts its commands and alerts, so
    /// no frame has to come (a hidden window draws none).
    fn look_until_it_comes(&self, key: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |ws, cx| {
            loop {
                cx.background_executor().timer(EVENT_LOOK).await;
                let waiting = ws.update(cx, |ws, cx| ws.look(key, cx)).unwrap_or(false);
                if !waiting {
                    break;
                }
            }
        })
        .detach();
    }

    /// The wait `key` is answered when its command or alert came since the call. False once the
    /// wait is gone.
    fn look(&mut self, key: u64, cx: &mut Context<Self>) -> bool {
        let Some(wait) = self.waits.iter().find(|w| w.key == key) else {
            return false;
        };
        let Waiting::Event {
            of,
            uid,
            event,
            since,
        } = &wait.what
        else {
            return false;
        };
        // Its block closed: `drop_orphans` answers it.
        let Some(pane) = self.pane(of) else {
            return true;
        };
        let status = pane.term.status();
        let reply = match event {
            Event::CommandFinished if status.finished > since.0 => {
                let code = match status.last.and_then(|o| o.code) {
                    Some(code) => format!("with exit code {code}"),
                    None => "without an exit code".to_string(),
                };
                let mut text = format!("A command finished in {uid}, {code}.");
                if let Some(output) = pane.term.shared().last_command() {
                    text.push('\n');
                    text.push_str(tools::tail(&output, tools::MAX_TEXT));
                }
                Reply::ok(text)
            }
            Event::Attention if status.attention > since.1 => Reply::ok(match status.message {
                Some(message) => format!("{uid} asks for attention: {message}"),
                None => format!("{uid} rang its bell."),
            }),
            _ => return true,
        };
        self.answer(key, reply, cx);
        false
    }
```

and after `field_focused` at the end of the file:

```rust
/// `wait_for(needs_you)`'s answer: block `uid` waits for its person, for `what` when it says.
fn waiting_for_person(uid: &str, what: Option<&str>) -> Reply {
    Reply::ok(match what.filter(|w| !w.is_empty()) {
        Some(what) => format!("{uid} is waiting for its person: {what}"),
        None => format!("{uid} is waiting for its person."),
    })
}

/// What a `wait_for` waited for, as its timeout says it: "No call for attention in t4q8p…".
fn awaited(event: Event) -> &'static str {
    match event {
        Event::Idle => "end of its turn",
        Event::NeedsYou => "wait for its person",
        Event::CommandFinished => "finished command",
        Event::Attention => "call for attention",
    }
}
```

In `balloon.rs`, make `Waiting::Turn { .. } => return None,` (:34) `Waiting::Turn { .. } | Waiting::Event { .. } =>
return None,` and `Waiting::Turn { .. } => row,` (:134) `Waiting::Turn { .. } | Waiting::Event { .. } => row,`.

In `bus.rs` `call`, replace the stopgap `Call::WaitFor { .. } | Call::Wire { .. } => …` with

```rust
            Call::WaitFor {
                block,
                event,
                timeout,
            } => return self.wait_for(terminal, &block, event, timeout),
            Call::Wire { .. } => Reply::err("Not in this Templum yet."),
```

and in `agent_event` replace `AgentEvent::NeedsYou { message } => pane.state = Some(AgentState::NeedsYou(message)),`
with

```rust
            AgentEvent::NeedsYou { message } => {
                pane.state = Some(AgentState::NeedsYou(message.clone()));
                self.needs_you(terminal, message.as_deref(), cx);
            }
```

- [ ] **Step 4:** Run `cargo test -p templum wait_for` and `cargo test -p templum workspace::tests::requests`.
  Expected: PASS (the balloon tests still see only questions and connect requests).
- [ ] **Step 5:** `cargo fmt --all && cargo clippy -p templum --all-targets -- -D warnings`: clean.
- [ ] **Step 6: Commit.**

```bash
git add crates/templum/src/workspace/waits.rs crates/templum/src/balloon.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "feat(connections): an agent waits for a wired block to need you, finish a command or ring" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 6: `templum wire` in the app

**Files:**
- Modify: `crates/templum/src/workspace/bus.rs` (`serve` :151: `cli_only`; `call`: the `Wire` arm)
- Modify: `crates/templum/src/workspace/wires.rs` (`wire_blocks`)
- Test: `crates/templum/src/workspace/tests/wires.rs`

**Interfaces:**
- Consumes: `Call::Wire`, `tools::cli_only` (Task 1); `Workspace::connect`, `in_space`, `caller`, `space_named`.
- Produces: `Workspace::wire_blocks(&mut self, from: &str, a: &str, b: &str, cx) -> Reply`.

- [ ] **Step 1: Write the failing test** in `tests/wires.rs` (add `use super::bus::{call, call_later, me};` and
  `use serde_json::json;` at the top):

```rust
#[gpui_kit::test]
fn templum_wire_wires_two_blocks_of_its_workspace_as_a_step(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let (from, _) = me(&ws, cx, terminal);
    let [k, n] = uids(&ws, cx, [kanban, note]);
    let (failed, text) = call(handle, &ws, cx, &from, "wire", json!({ "a": k, "b": n }));
    assert_eq!((failed, text), (false, format!("Connected {k} and {n}.")));
    assert!(ws.read_with(cx, |ws, _| ws.space.wired(kanban, note)));
    let (failed, text) = call(handle, &ws, cx, &from, "wire", json!({ "a": n, "b": k }));
    assert_eq!((failed, text), (false, format!("{n} and {k} are already connected.")));
    let (failed, text) = call(handle, &ws, cx, &from, "wire", json!({ "a": k, "b": k }));
    assert_eq!((failed, text), (true, format!("{k} can't be wired to itself.")));
    let (failed, text) = call(handle, &ws, cx, &from, "wire", json!({ "a": k, "b": "zzzzz" }));
    assert_eq!(
        (failed, text),
        (true, "There's no block zzzzz in your workspace: templum blocks lists them.".to_string())
    );
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert!(ws.undo_canvas(false, window, cx), "a step of Ctrl+Z");
            assert!(!ws.space.wired(kanban, note));
        })
    })
    .unwrap();
    // An agent's MCP server never gets it: the command line's alone.
    let answers = call_later(handle, &ws, cx, &from, 1, "wire", json!({ "a": k, "b": n }));
    let line = answers.try_recv().unwrap();
    assert!(line.contains("Unknown tool: wire"), "{line}");
}
```

- [ ] **Step 2:** Run `cargo test -p templum templum_wire`. Expected: FAIL (`call` gets "Unknown tool: wire": `serve`
  doesn't take it yet).
- [ ] **Step 3: Write the code.** In `serve`, replace `if !tools::known(name) {` with

```rust
                        let cli_only = hello.client == "cli" && tools::cli_only(name);
                        if !tools::known(name) && !cli_only {
```

In `call`, replace `Call::Wire { .. } => Reply::err("Not in this Templum yet."),` with
`Call::Wire { a, b } => self.wire_blocks(terminal, &a, &b, cx),`. In `workspace/wires.rs`, add `use
templum_mcp::Reply;` and to `impl Workspace`:

```rust
    /// `templum wire <a> <b>` from the terminal `from`: wires two blocks of its workspace, a step
    /// Ctrl+Z takes back. A person's command, or a script's (an agent's shell's too): it asks no
    /// one.
    pub(super) fn wire_blocks(
        &mut self,
        from: &str,
        a: &str,
        b: &str,
        cx: &mut Context<Self>,
    ) -> Reply {
        let (space_id, _) = self.caller(from).expect("call found the caller");
        let space = self.space_named(&space_id).expect("the caller's workspace");
        let find = |uid: &str| {
            space.block_by_uid(uid).ok_or_else(|| {
                Reply::err(format!(
                    "There's no block {uid} in your workspace: templum blocks lists them."
                ))
            })
        };
        let (x, y) = match (find(a), find(b)) {
            (Ok(x), Ok(y)) => (x, y),
            (Err(refused), _) | (_, Err(refused)) => return refused,
        };
        if x == y {
            return Reply::err(format!("{a} can't be wired to itself."));
        }
        if space.wired(x, y) {
            return Reply::ok(format!("{a} and {b} are already connected."));
        }
        match self.in_space(&space_id, |ws| ws.connect(x, y, cx)) {
            Some(true) => Reply::ok(format!("Connected {a} and {b}.")),
            _ => Reply::err(format!("{a} and {b} can't be wired.")),
        }
    }
```

- [ ] **Step 4:** Run `cargo test -p templum templum_wire`. Expected: PASS.
- [ ] **Step 5:** `cargo fmt --all && cargo clippy -p templum --all-targets -- -D warnings`: clean.
- [ ] **Step 6: Commit.**

```bash
git add crates/templum/src/workspace/bus.rs crates/templum/src/workspace/wires.rs crates/templum/src/workspace/tests/wires.rs
git commit -m "feat(connections): templum wire connects two blocks from a terminal, and Ctrl+Z takes it back" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 7: An agent's state from its screen

**Files:**
- Create: `crates/templum/src/workspace/activity.rs`
- Modify: `crates/templum/src/workspace/mod.rs` (`mod activity;`; `Pane` gains `looks` and `typed`; `Workspace`
  gains `_looks: Task<()>`, started in `Workspace::new` next to `snapshots` :1069; `use std::cell::Cell`)
- Modify: `crates/templum/src/workspace/blocks.rs:412-425` (the `Pane` literal), `crates/templum/src/workspace/input.rs`
  (:428 paste, :744 key, :971 IME text: the person typed)
- Test: unit tests in `activity.rs`; `crates/templum/src/workspace/tests/delegate.rs`

**Interfaces:**
- Consumes: `Pane::screen()` (`delegate.rs:24`), `Workspace::end_turn` (`delegate.rs:317`), `waits::LAST_LINES`
  (Task 5).
- Produces: `activity::LOOK: Duration`, `activity::Looks` (Default), `Pane.looks: activity::Looks`,
  `Pane.typed: Cell<bool>`, `Workspace::look_at_agents(&mut self, cx)`.

- [ ] **Step 1: Write the failing unit tests.** Create `crates/templum/src/workspace/activity.rs` with its doc,
  imports and tests only (add `mod activity;` to `workspace/mod.rs`):

```rust
//! An agent's state from its screen, where its hooks say nothing (docs/specs/connections.md, An
//! agent's state): a screen that keeps changing (a spinner, a timer) is an agent at work, one
//! that stays still is not. Codex says only that its turn ended; Claude Code says nothing when
//! Esc stops it while it writes, nor when a tool the person allowed starts.

use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::time::Duration;

use gpui_kit::Context;
use templum_mcp::{Reply, tools};

use super::waits::LAST_LINES;
use super::{AgentState, Body, Workspace};

#[cfg(test)]
mod tests {
    use super::*;

    /// The looks after a first one, each seeing the screen change or not.
    fn saw(changes: &[bool]) -> Looks {
        let mut looks = Looks::default();
        let mut hash = 1;
        looks.see(hash);
        for &changed in changes {
            hash += u64::from(changed);
            looks.see(hash);
        }
        looks
    }

    #[test]
    fn a_screen_that_keeps_changing_is_at_work() {
        // Codex's "Working (5s)" changes once a second: a look may fall between two changes.
        let looks = saw(&[true, false, true, true]);
        assert_eq!(looks.state(&None), Some(AgentState::Working));
        assert_eq!(looks.state(&Some(AgentState::Idle)), Some(AgentState::Working));
        assert_eq!(
            looks.state(&Some(AgentState::NeedsYou(Some("Allow Bash?".into())))),
            Some(AgentState::Working),
            "the tool the person allowed runs"
        );
        assert_eq!(looks.state(&Some(AgentState::Working)), None, "nothing new");
        assert_eq!(
            saw(&[true, false, false, true]).state(&None),
            None,
            "twice in four looks: a redraw"
        );
    }

    #[test]
    fn a_screen_still_for_five_looks_is_at_rest() {
        let working = Some(AgentState::Working);
        assert_eq!(
            saw(&[true, false, false, false, false, false]).state(&working),
            Some(AgentState::Idle)
        );
        assert_eq!(saw(&[true, false, false, false, false]).state(&working), None, "four");
        assert_eq!(
            saw(&[false; 6]).state(&Some(AgentState::NeedsYou(None))),
            None,
            "a dialog waits for its person, still: it stays yellow"
        );
        assert_eq!(saw(&[false; 6]).state(&None), None);
        assert_eq!(saw(&[false; 6]).state(&Some(AgentState::Idle)), None);
    }
}
```

- [ ] **Step 2:** Run `cargo test -p templum activity::`. Expected: FAIL to compile (`Looks` not found).
- [ ] **Step 3: Write `Looks`** between the imports and the tests:

```rust
/// How often Templum looks at the agents' screens.
pub(super) const LOOK: Duration = Duration::from_secs(1);
/// Of the last 4 looks, how many saw the screen change for its agent to be at work: Codex's
/// "Working (5s)" changes once a second, and a look may fall between two changes.
const CHANGING: u32 = 3;
/// How many looks in a row saw the screen still for its agent to be at rest.
const STILL: u32 = 5;

/// The looks at an agent's screen: its content's hash, whether each of the last 8 saw it change
/// (the newest in the lowest bit), and how many there were.
#[derive(Default)]
pub(crate) struct Looks {
    hash: u64,
    changes: u8,
    count: u8,
}

impl Looks {
    /// A look that found the screen's content hashing to `hash`.
    fn see(&mut self, hash: u64) {
        self.changes = (self.changes << 1) | u8::from(hash != self.hash);
        self.hash = hash;
        self.count = self.count.saturating_add(1);
    }

    /// The state these looks give an agent in `state`; None when they say nothing new. A
    /// changing screen is at work, also after a dialog the person answered; a still one ends
    /// only work, never a wait for the person.
    fn state(&self, state: &Option<AgentState>) -> Option<AgentState> {
        let changing = (self.changes & 0b1111).count_ones() >= CHANGING;
        let still = u32::from(self.count) >= STILL && self.changes & ((1u8 << STILL) - 1) == 0;
        match state {
            Some(AgentState::Working) if still => Some(AgentState::Idle),
            None | Some(AgentState::Idle | AgentState::NeedsYou(_)) if changing => {
                Some(AgentState::Working)
            }
            _ => None,
        }
    }
}
```

- [ ] **Step 4:** Run `cargo test -p templum activity::`. Expected: PASS.
- [ ] **Step 5: Write the failing headless test** at the end of `tests/delegate.rs` (cfg(unix): `cat` prints):

```rust
/// `cat` in block `id` prints `text` back, and the test waits until its screen shows it.
#[cfg(unix)]
fn shows(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: BlockId, text: &str) {
    let line = format!("{text}\r").into_bytes();
    ws.read_with(cx, |ws, _| ws.space.terminal(id).unwrap().term.write(line));
    screen_with(ws, cx, id, text);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_screen_at_work_says_working_and_one_gone_still_ends_its_task(cx: &mut TestAppContext) {
    use crate::workspace::AgentState;
    let (handle, ws, mine, theirs) = two_agents(cx);
    let (terminal, _) = me(&ws, cx, mine);
    let (_, them) = me(&ws, cx, theirs);
    let state = |ws: &Entity<Workspace>, cx: &mut TestAppContext| {
        ws.read_with(cx, |ws, _| ws.space.terminal(theirs).unwrap().state.clone())
    };
    // The person types: their keys change the screen, and tell nothing.
    for i in 0..4 {
        ws.read_with(cx, |ws, _| ws.space.terminal(theirs).unwrap().typed.set(true));
        shows(&ws, cx, theirs, &format!("typed {i}"));
        after(cx, 1000);
    }
    assert_eq!(state(&ws, cx), None);
    // Its own output, a second apart: at work.
    for i in 0..4 {
        shows(&ws, cx, theirs, &format!("tick {i}"));
        after(cx, 1000);
    }
    assert_eq!(state(&ws, cx), Some(AgentState::Working));
    let idle = call_later(handle, &ws, cx, &terminal, 1, "wait_for_idle", json!({ "block": them }));
    assert!(idle.try_recv().is_err(), "it waits while the agent works");
    for _ in 0..5 {
        after(cx, 1000);
    }
    assert_eq!(state(&ws, cx), Some(AgentState::Idle));
    let (failed, text) = answered(&idle);
    assert!(
        !failed && text.starts_with("Its screen has been still for 5 seconds") && text.contains("tick 3"),
        "{text}"
    );
}
```

- [ ] **Step 6:** Run `cargo test -p templum a_screen_at_work`. Expected: FAIL to compile (`typed` not a field).
- [ ] **Step 7: Write the code.** In `workspace/mod.rs`: `use std::cell::Cell;` (beside `RefCell`); in `Pane` after
  `prompt`:

```rust
    /// The looks at its screen: its agent's state where its hooks say nothing (`activity.rs`).
    pub looks: activity::Looks,
    /// The person typed into it since the last look, which then tells nothing.
    pub typed: Cell<bool>,
```

In the `Pane` literal of `blocks.rs` add `looks: Default::default(), typed: Default::default(),`. In `Workspace` add
`/// Looks at the agents' screens each \`activity::LOOK\`.` `_looks: Task<()>,`, and in `Workspace::new` after
`snapshots`:

```rust
        let looks = cx.spawn(async move |ws, cx| {
            loop {
                cx.background_executor().timer(activity::LOOK).await;
                if ws.update(cx, |ws, cx| ws.look_at_agents(cx)).is_err() {
                    break;
                }
            }
        });
```

and `_looks: looks,` in the struct literal (beside `_snapshots: snapshots,`). In `input.rs` call
`pane.typed.set(true);` just before `pane.term.paste(&text);` (:428), before `if pane.term.key(&input) {` (:744) and
before `pane.term.write(text.as_bytes().to_vec());` (:971). In `activity.rs` add after `impl Looks`:

```rust
impl Workspace {
    /// A look at every agent's screen, in every workspace: the state it gives, where that is new.
    /// A look after the person typed into it tells nothing (their keys change it). An agent at
    /// work whose screen went still ends the task it was on, with its last lines: no event will.
    pub(super) fn look_at_agents(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut still = Vec::new();
        let spaces = std::iter::once(&mut self.space).chain(&mut self.others);
        for pane in spaces
            .flat_map(|s| s.blocks.values_mut())
            .filter_map(Body::terminal_mut)
        {
            if pane.seen.agent.is_none() {
                pane.looks = Looks::default();
                continue;
            }
            let mut hasher = DefaultHasher::new();
            pane.screen().hash(&mut hasher);
            let hash = hasher.finish();
            if pane.typed.replace(false) {
                pane.looks.hash = hash;
                continue;
            }
            pane.looks.see(hash);
            let Some(state) = pane.looks.state(&pane.state) else {
                continue;
            };
            if state == AgentState::Idle {
                still.push(pane.term.id().to_string());
            }
            pane.state = Some(state);
            changed = true;
        }
        for terminal in still {
            let lines = self
                .pane(&terminal)
                .map(|pane| pane.term.shared().lines(LAST_LINES).join("\n"))
                .unwrap_or_default();
            let reply = Reply::ok(format!(
                "Its screen has been still for {STILL} seconds, and Templum heard no answer: it may \
                 be done, stopped, or asking its person. The last lines of its screen:\n{}",
                tools::tail(&lines, tools::MAX_TEXT)
            ));
            self.end_turn(&terminal, reply, cx);
        }
        if changed {
            cx.notify();
        }
    }
}
```

- [ ] **Step 8:** Run `cargo test -p templum a_screen_at_work` then `cargo test -p templum` (the whole crate: a test
  that advances the clock past 5 looks with a still "Working" agent now sees Idle; fix such a test only when the new
  state is right, else stop and report). Expected: PASS.
- [ ] **Step 9:** `cargo fmt --all && cargo clippy -p templum --all-targets -- -D warnings`: clean.
- [ ] **Step 10: Commit.**

```bash
git add crates/templum/src/workspace/activity.rs crates/templum/src/workspace/mod.rs crates/templum/src/workspace/blocks.rs crates/templum/src/workspace/input.rs crates/templum/src/workspace/tests/delegate.rs
git commit -m "fix(connections): an agent's dot follows its screen where its hooks say nothing, so Codex shows Working" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 8: Wire labels: Rename wire…, undo, save

**Files:**
- Modify: `crates/templum/src/workspace/mod.rs` (`Space::labels` after `selected_wire` :498; `Space::empty` :633;
  `layout` :580-589)
- Modify: `crates/templum/src/workspace/wires.rs` (`label_key`, `LABEL_MAX`, `Space::label_key`, `Space::label`,
  `Workspace::rename_wire`, `Workspace::set_label`)
- Modify: `crates/templum/src/workspace/history.rs` (`Change::Label`, `ids`, `undo_canvas`)
- Modify: `crates/templum/src/session.rs:345-350` (`SavedWire.label`) and its test :964-980
- Modify: `crates/templum/src/workspace/persist.rs:74-83` (read labels back)
- Modify: `crates/templum/src/menu/build.rs:1007-1014` (Rename wire…)
- Test: `crates/templum/src/workspace/tests/wires.rs` (and `right_clicking_a_wire_offers_disconnect` clicks index 1)

**Interfaces:**
- Produces: `Space::labels: HashMap<(String, String), String>`; `wires::label_key(a: &str, b: &str) -> (String,
  String)`; `wires::LABEL_MAX: usize = 60`; `Space::label_key(&self, Wire) -> Option<(String, String)>`;
  `Space::label(&self, Wire) -> Option<&str>`; `Workspace::rename_wire(&mut self, Wire, &str, cx) -> bool`;
  `Workspace::set_label(&mut self, Wire, Option<String>, Then, cx) -> bool`; `Change::Label(Wire, Option<String>)`;
  `SavedWire { a, b, label: Option<String> }`.

- [ ] **Step 1: Write the failing tests.** In `session.rs` rename the test to
  `wires_are_written_by_block_id_with_their_labels_and_left_out_when_there_are_none` and replace its body after the
  first assert with:

```rust
        saved.wires = vec![
            SavedWire {
                a: "b7k2m".into(),
                b: "4qm2a".into(),
                label: None,
            },
            SavedWire {
                a: "b7k2m".into(),
                b: "t4q8p".into(),
                label: Some("dev server".into()),
            },
        ];
        let text = serde_json::to_string(&saved).unwrap();
        assert!(
            text.contains(
                r#""wires":[{"a":"b7k2m","b":"4qm2a"},{"a":"b7k2m","b":"t4q8p","label":"dev server"}]"#
            ),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<Saved>(&text).unwrap(), saved);
```

In `tests/wires.rs`, change `right_clicking_a_wire_offers_disconnect`'s `click(0usize, cx)` to `click(1usize, cx)`
(Rename wire… comes first), and add:

```rust
#[gpui_kit::test]
fn rename_wire_labels_it_and_ctrl_z_takes_the_label_back(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = apart(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
    });
    let wire = Wire::new(kanban, note).unwrap();
    let on = on_the_wire(&ws, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        right_click(window, cx, on);
        window.render_frame(cx);
        window.within("popup-menu").click(0usize, cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "Rename wire… asks for the label");
        window.input("dev server", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            assert_eq!(ws.space.label(wire), Some("dev server"));
            assert!(ws.undo_canvas(false, window, cx));
            assert!(ws.space.wired(kanban, note), "Ctrl+Z takes the label, not the wire");
            assert_eq!(ws.space.label(wire), None);
            assert!(ws.undo_canvas(true, window, cx));
            assert_eq!(ws.space.label(wire), Some("dev server"));
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_label_is_one_line_of_60_at_most(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let wire = Wire::new(kanban, note).unwrap();
    ws.update(cx, |ws, cx| {
        assert!(!ws.rename_wire(wire, "api", cx), "not wired: nothing to name");
        ws.connect(kanban, note, cx);
        assert!(ws.rename_wire(wire, "  dev \t server\n", cx));
        assert_eq!(ws.space.label(wire), Some("dev server"));
        assert_eq!(
            ws.space.label(Wire::new(note, kanban).unwrap()),
            Some("dev server"),
            "either way round"
        );
        assert!(!ws.rename_wire(wire, "dev server", cx), "the same label is no step");
        assert!(ws.rename_wire(wire, &"x".repeat(200), cx));
        assert_eq!(ws.space.label(wire).map(|l| l.chars().count()), Some(60));
        assert!(ws.rename_wire(wire, "   ", cx), "blank: no label");
        assert_eq!(ws.space.label(wire), None);
        assert_eq!(ws.space.label(Wire::new(kanban, terminal).unwrap()), None);
    });
}

#[gpui_kit::test]
fn a_label_comes_back_with_its_wire(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = three_blocks(handle, &ws, cx);
    let [kanban_uid] = uids(&ws, cx, [kanban]);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.connect(kanban, note, cx);
            let wire = Wire::new(kanban, note).unwrap();
            ws.rename_wire(wire, "board", cx);
            ws.disconnect(wire, cx);
            assert!(ws.undo_canvas(false, window, cx));
            assert_eq!(ws.space.label(wire), Some("board"), "Ctrl+Z of the disconnect");
            ws.close_now(&[kanban], Then::New, window, cx);
            assert!(ws.undo_canvas(false, window, cx));
            let back = ws.space.block_by_uid(&kanban_uid).unwrap();
            assert_eq!(
                ws.space.label(Wire::new(back, note).unwrap()),
                Some("board"),
                "by its blocks' ids, through a close"
            );
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_label_comes_back_after_a_restart(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    let (kanban, note, terminal) = three_blocks(handle, &ws, cx);
    let [k, n, t] = uids(&ws, cx, [kanban, note, terminal]);
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
        ws.connect(note, terminal, cx);
        ws.rename_wire(Wire::new(note, terminal).unwrap(), "dev server", cx);
        ws.save_canvas_now(cx);
    });
    let saved =
        std::fs::read_to_string(data.path().join("workspaces").join("default.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        saved["wires"],
        json!([{ "a": k, "b": n }, { "a": n, "b": t, "label": "dev server" }])
    );
    let (_, again) = open_in(cx, None, Some(data.path().to_path_buf()));
    again.read_with(cx, |ws, _| {
        let id = |uid: &str| ws.space.block_by_uid(uid).unwrap();
        assert_eq!(ws.space.label(Wire::new(id(&n), id(&t)).unwrap()), Some("dev server"));
        assert_eq!(ws.space.label(Wire::new(id(&k), id(&n)).unwrap()), None);
    });
}

/// A file edited by hand: a label is trimmed, a blank one is none, and one on a wire that is
/// dropped (its block is missing) stays out.
#[gpui_kit::test]
fn a_saved_label_needs_its_wire_and_some_text(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let kanban = |x: f32, uid: &str| {
        format!(r#"{{"kind":"kanban","uid":"{uid}","rect":{{"x":{x},"y":0,"w":300,"h":200}}}}"#)
    };
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[{},{},{}],"wires":[{{"a":"aaaaa","b":"bbbbb","label":"  api  "}},{{"a":"bbbbb","b":"ccccc","label":"   "}},{{"a":"aaaaa","b":"zzzzz","label":"gone"}}]}}"#,
            kanban(0.0, "aaaaa"),
            kanban(400.0, "bbbbb"),
            kanban(800.0, "ccccc"),
        ),
    );
    let (_, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    ws.read_with(cx, |ws, _| {
        let id = |uid: &str| ws.space.block_by_uid(uid).unwrap();
        assert_eq!(ws.space.label(Wire::new(id("aaaaa"), id("bbbbb")).unwrap()), Some("api"));
        assert_eq!(ws.space.label(Wire::new(id("bbbbb"), id("ccccc")).unwrap()), None);
        assert_eq!(ws.space.labels.len(), 1, "a label on a dropped wire stays out");
    });
}
```

- [ ] **Step 2:** Run `cargo test -p templum label` and `cargo test -p templum session::`. Expected: FAIL to compile
  (`label` not a field of `SavedWire`; `rename_wire`, `Space::label` not found).
- [ ] **Step 3: Write the code.** `session.rs`, `SavedWire`:

```rust
pub(crate) struct SavedWire {
    pub a: String,
    pub b: String,
    /// Its name (Rename wire…), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}
```

`workspace/mod.rs`, after `selected_wire` in `Space`:

```rust
    /// The wires' labels, by the block ids (`uids`) of their ends, the smaller first. A label
    /// stays when its wire goes: Ctrl+Z bringing the wire back, or its closed block, brings it
    /// back too. Only the wires there are save theirs.
    pub labels: HashMap<(String, String), String>,
```

In `Space::empty`, after `self.selected_wire = None;` add `self.labels.clear();`. In `layout`, give the `SavedWire`
literal `label: self.label(*w).map(str::to_string),`. In `workspace/wires.rs` add before `impl Space`:

```rust
/// The longest a label is, in characters.
pub(crate) const LABEL_MAX: usize = 60;

/// A label's key from its wire's ends' block ids, in either order.
pub(crate) fn label_key(a: &str, b: &str) -> (String, String) {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    (a.to_string(), b.to_string())
}
```

to `impl Space`:

```rust
    /// The key of `wire`'s label; None while one of its blocks has no id.
    pub(crate) fn label_key(&self, wire: Wire) -> Option<(String, String)> {
        Some(label_key(self.uids.get(&wire.a)?, self.uids.get(&wire.b)?))
    }

    /// `wire`'s label, if it has one.
    pub(crate) fn label(&self, wire: Wire) -> Option<&str> {
        self.labels.get(&self.label_key(wire)?).map(String::as_str)
    }
```

and to `impl Workspace`:

```rust
    /// Rename wire…: `text` on one line, at most LABEL_MAX characters, becomes `wire`'s label;
    /// blank, it has none. A step Ctrl+Z takes back. False when the wire is gone or nothing
    /// changes.
    pub(crate) fn rename_wire(&mut self, wire: Wire, text: &str, cx: &mut Context<Self>) -> bool {
        if !self.space.wires.contains(&wire) {
            return false;
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let label = (!text.is_empty()).then(|| text.chars().take(LABEL_MAX).collect());
        self.set_label(wire, label, Then::New, cx)
    }

    /// Gives `wire` `label` (None: none); the change that takes it back is kept as `then` says.
    /// False when nothing changes.
    pub(super) fn set_label(
        &mut self,
        wire: Wire,
        label: Option<String>,
        then: Then,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(key) = self.space.label_key(wire) else {
            return false;
        };
        let before = match label.clone() {
            Some(label) => self.space.labels.insert(key, label),
            None => self.space.labels.remove(&key),
        };
        if before == label {
            return false;
        }
        self.keep(Change::Label(wire, before), then);
        cx.notify();
        true
    }
```

`history.rs`: add to `Change`

```rust
    /// Give this wire this label, or none (it had another).
    Label(Wire, Option<String>),
```

in `ids` make the wire arm `Change::Connect(Wire { a, b }) | Change::Disconnect(Wire { a, b }) |
Change::Label(Wire { a, b }, _) => vec![a, b],`; in `undo_canvas` make
`let wire = matches!(change, Change::Connect(_) | Change::Disconnect(_) | Change::Label(..));` and add the arm

```rust
                Change::Label(wire, label) => {
                    self.take_step(then);
                    self.set_label(wire, label, then, cx);
                }
```

`persist.rs`: replace the wires loop with

```rust
        for wire in &saved.wires {
            let (Some(a), Some(b)) = (
                self.space.block_by_uid(&wire.a),
                self.space.block_by_uid(&wire.b),
            ) else {
                continue;
            };
            let label = wire.label.as_deref().map(str::trim).filter(|l| !l.is_empty());
            if self.space.add_wire(a, b).is_some()
                && let Some(label) = label
            {
                let key = super::wires::label_key(&wire.a, &wire.b);
                let label = label.chars().take(super::wires::LABEL_MAX).collect();
                self.space.labels.insert(key, label);
            }
        }
```

`menu/build.rs`, replace the `Items::Wire { wire } => menu.item(item(ws, "Disconnect", …))` arm with

```rust
            Items::Wire { wire } => menu
                .item(item(ws, "Rename wire…", None, move |ws, window, cx| {
                    let label = ws.space.label(wire).unwrap_or_default().to_string();
                    let entity = cx.entity();
                    prompt::ask_text(
                        "Rename wire",
                        "Rename",
                        label,
                        move |text, _, cx| {
                            entity.update(cx, |ws, cx| ws.rename_wire(wire, &text, cx));
                            true
                        },
                        window,
                        cx,
                    )
                }))
                .item(item(
                    ws,
                    "Disconnect",
                    Some(IconName::Unplug),
                    move |ws, _, cx| {
                        ws.disconnect(wire, cx);
                    },
                )),
```

and update `Items::Wire`'s doc (`menu/mod.rs:664`) to "A wire: Rename wire…, Disconnect".

- [ ] **Step 4:** Run `cargo test -p templum label`, `cargo test -p templum session::`,
  `cargo test -p templum workspace::tests::wires`. Expected: PASS.
- [ ] **Step 5:** `cargo fmt --all && cargo clippy -p templum --all-targets -- -D warnings`: clean.
- [ ] **Step 6: Commit.**

```bash
git add crates/templum/src/session.rs crates/templum/src/workspace/mod.rs crates/templum/src/workspace/wires.rs crates/templum/src/workspace/history.rs crates/templum/src/workspace/persist.rs crates/templum/src/menu/build.rs crates/templum/src/menu/mod.rs crates/templum/src/workspace/tests/wires.rs
git commit -m "feat(connections): name a wire with Rename wire… in its menu; Ctrl+Z takes the name back" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 9: Wire labels: the pill

**Files:**
- Modify: `crates/templum/src/wires.rs` (`LABEL_FONT`, `LABEL_WIDTH`, `label_scale`, `label`; unit test)
- Modify: `crates/templum/src/workspace/wires.rs` (`wire_color`, `wire_labels`; `wire_curves` uses `wire_color`)
- Modify: `crates/templum/src/workspace/render.rs:439-465, :538-539` (the labels, between the layer and the blocks)
- Modify: `crates/templum/src/bench.rs:571-577` (the benches label their wires)
- Test: `crates/templum/src/workspace/tests/wires.rs`

**Interfaces:**
- Consumes: `Space::label` (Task 8).
- Produces: `crate::wires::label_scale(rest: f32, zoom: f32) -> Option<f32>`; `crate::wires::label(n: usize, text:
  String, at: Point, s: f32, color: Hsla, ui: Ui) -> Div`; `Workspace::wire_labels(&self) -> Vec<(Point, String,
  Hsla)>`.

- [ ] **Step 1: Write the failing tests.** In `wires.rs`'s tests:

```rust
    #[test]
    fn a_label_scales_at_full_keeps_its_size_at_silhouette_and_goes_at_card() {
        assert_eq!(label_scale(1.0, 1.0), Some(1.0));
        assert_eq!(label_scale(0.5, 0.5), Some(0.5), "12 px at 50% is 6 px: Full");
        assert_eq!(label_scale(0.25, 0.25), Some(1.0), "Silhouette: its own 12 px");
        assert_eq!(label_scale(0.2, 0.2), None, "Card");
        assert_eq!(label_scale(1.0, 0.8), Some(0.8), "the level at rest, the size as drawn");
    }
```

In `tests/wires.rs`:

```rust
#[gpui_kit::test]
fn a_label_sits_at_the_wires_middle_down_to_25_percent_and_grid_mode_hides_it(
    cx: &mut TestAppContext,
) {
    let (handle, ws) = open(cx);
    let (kanban, note, _) = apart(handle, &ws, cx);
    ws.update(cx, |ws, cx| {
        ws.connect(kanban, note, cx);
        ws.rename_wire(Wire::new(kanban, note).unwrap(), "board", cx);
        let labels = ws.wire_labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(
            (labels[0].0, labels[0].1.as_str(), labels[0].2),
            (Point::new(480.0, 180.0), "board", ws.ui.title_dim)
        );
    });
    for zoom in [1.0, 0.5, 0.25] {
        ws.update(cx, |ws, _| {
            ws.space.canvas.camera = Camera::at(Point::new(0.0, 0.0), zoom);
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(("wire-label", 0usize)).is_some(), "at {zoom}");
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.enter_grid(vec![kanban, note], window, cx));
        window.render_frame(cx);
        assert!(
            window.try_find(("wire-label", 0usize)).is_none(),
            "grid mode hides it"
        );
    })
    .unwrap();
}
```

- [ ] **Step 2:** Run `cargo test -p templum label_s` and `cargo test -p templum a_label_sits`. Expected: FAIL to
  compile (`label_scale`, `wire_labels` not found).
- [ ] **Step 3: Write the code.** `crates/templum/src/wires.rs`: imports become

```rust
use gpui_kit::prelude::*;
use gpui_kit::{
    Bounds, Div, Hsla, PathBuilder, Pixels, TestSupportExt as _, Window, div, fill, point, px, size,
};
use templum_canvas::{Camera, Curve, Level, Point, Rect, level};

use crate::theme::Ui;
```

and add after `dot_radius`:

```rust
/// A label's text size at zoom 1, pixels: a block title's.
const LABEL_FONT: f32 = 12.0;
/// The widest a label's pill grows at zoom 1; longer text ends in "…".
const LABEL_WIDTH: f32 = 240.0;

/// How much a wire's label is scaled, the canvas resting at `rest` and drawn at `zoom`: with the
/// canvas at the Full level, at its own size at Silhouette (as a block's title label is), and not
/// drawn at Card (docs/specs/connections.md, Wires).
pub(crate) fn label_scale(rest: f32, zoom: f32) -> Option<f32> {
    match level(LABEL_FONT, rest) {
        Level::Full => Some(zoom),
        Level::Silhouette => Some(1.0),
        Level::Card => None,
    }
}

/// The `n`th label drawn: `text` in a pill centered on `at` (pixels from the canvas's origin),
/// scaled by `s`, in its wire's `color`.
pub(crate) fn label(n: usize, text: String, at: Point, s: f32, color: Hsla, ui: Ui) -> Div {
    let (w, h) = (LABEL_WIDTH * s, LABEL_FONT * 2.0 * s);
    div()
        .absolute()
        .left(px(at.x - w / 2.0))
        .top(px(at.y - h / 2.0))
        .w(px(w))
        .h(px(h))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id(("wire-label", n))
                .max_w(px(w))
                .px(px(6.0 * s))
                .py(px(2.0 * s))
                .truncate()
                .rounded_full()
                .border_1()
                .border_color(ui.hairline)
                .bg(ui.surface)
                .text_size(px(LABEL_FONT * s))
                .text_color(color)
                .child(text)
                .test_support(),
        )
}
```

`workspace/wires.rs`: add to `impl Workspace`

```rust
    /// A wire's color: the title's dim one, the accent while it is selected.
    fn wire_color(&self, wire: Wire) -> Hsla {
        if self.space.selected_wire == Some(wire) {
            self.ui.accent
        } else {
            self.ui.title_dim
        }
    }

    /// The wires' labels to draw: each at its curve's middle (world units), in its wire's color.
    /// Grid mode hides them.
    pub(super) fn wire_labels(&self) -> Vec<(Point, String, Hsla)> {
        if self.space.grid.is_some() {
            return Vec::new();
        }
        self.space
            .wires
            .iter()
            .filter_map(|w| {
                let label = self.space.label(*w)?;
                Some((self.space.curve(*w)?.at(0.5), label.to_string(), self.wire_color(*w)))
            })
            .collect()
    }
```

and in `wire_curves` replace the `let color = if … { … } else { … };` block with `let color = self.wire_color(*w);`.
`render.rs`: after `let in_view = Rect::new(…);` add

```rust
        // The wires' labels: on the wires, under the blocks.
        let labels: Vec<_> = crate::wires::label_scale(self.space.rest, zoom)
            .map(|s| {
                self.wire_labels()
                    .into_iter()
                    .filter(|(at, ..)| in_view.contains(*at))
                    .enumerate()
                    .map(|(n, (at, text, color))| {
                        crate::wires::label(n, text, camera.to_screen(at), s, color, self.ui)
                    })
                    .collect()
            })
            .unwrap_or_default();
```

and between `.child(layer)` and `.children(blocks)` (:538-539) add `.children(labels)`. `bench.rs`: replace
`wire_in_a_chain`'s loop with

```rust
    for pair in ids.windows(2) {
        if let Some(wire) = ws.space.add_wire(pair[0], pair[1])
            && let Some(key) = ws.space.label_key(wire)
        {
            ws.space.labels.insert(key, "dev server".into());
        }
    }
```

and its doc to "Wires each block to the next, back to front, as a cockpit's are, each labeled: the benches paint
wires and their labels."

- [ ] **Step 4:** Run `cargo test -p templum label_s`, `cargo test -p templum a_label_sits`,
  `cargo test -p templum workspace::tests::wires`. Expected: PASS.
- [ ] **Step 5:** `cargo fmt --all && cargo clippy -p templum --all-targets -- -D warnings`: clean.
- [ ] **Step 6: Commit.**

```bash
git add crates/templum/src/wires.rs crates/templum/src/workspace/wires.rs crates/templum/src/workspace/render.rs crates/templum/src/bench.rs crates/templum/src/workspace/tests/wires.rs
git commit -m "feat(connections): a wire's name shows in a pill at its middle, readable from far away" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 10: Settings ▸ Connections

**Files:**
- Create: `crates/templum/src/settings/connections.rs`
- Modify: `crates/templum/src/settings/mod.rs` (`mod connections;`; `Section::Connections` after `Agents` in the enum,
  `ALL: [Section; 11]`, `title` "Connections", `tab_id` "settings-tab-connections", `note`)
- Modify: `crates/templum/src/settings/sections.rs` (`Section::Connections => super::connections::rows(ws, entity),`)
- Modify: `crates/templum/src/workspace/mod.rs` (`gemini_job: Option<TryIt>` + `gemini_job: None` beside
  `message_try`; `pub(crate) use bus::templum_in;`)
- Modify: `crates/templum/src/workspace/bus.rs` (`templum_in`, used by `templum_folder`)
- Modify: `crates/templum/src/workspace/usage.rs` (`setup_gemini`, `gemini_run`)
- Test: `crates/templum/src/workspace/tests/settings_panel.rs`, `crates/templum/src/workspace/tests/usage.rs`

**Interfaces:**
- Consumes: `templum_usage::gemini::{extension, link_args, UNLINK, installed}` (Task 4); `templum_usage::Dirs::of`;
  `templum_terminal::locate`; `TryIt` (`workspace/git_message.rs:28`); `prompt::confirm`.
- Produces: `crate::workspace::templum_in(data: &Path) -> PathBuf`; `Workspace::setup_gemini(&mut self, install:
  bool, cx)`; `workspace::usage::gemini_run(gemini: Option<PathBuf>, data: &Path, install: bool) -> Result<(),
  String>`.

- [ ] **Step 1: Write the failing tests.** In `settings_panel.rs`'s
  `the_integration_and_notification_switches_turn_them_off_and_back`, add `ws.config.connections.enabled` to `on`'s
  tuple, the row `("settings-tab-connections", "field-connections.enabled", "reset-connections.enabled",
  "enabled = false")` to the loop, `(true, true, true, true)` to the "back on" assert, and
  `&& !text.contains("enabled")` to the last assert. Add:

```rust
#[gpui_kit::test]
fn connections_give_the_servers_command_to_copy_and_ask_before_gemini(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-connections", cx);
        window.render_frame(cx);
        assert!(window.try_find("connections-command").is_some());
        window.click("connections-copy", cx);
        window.click("gemini-setup", cx);
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "it asks first");
        window.within("dialog").click("cancel", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let bin = data
        .path()
        .join("bin")
        .join(if cfg!(windows) { "templum.exe" } else { "templum" });
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()),
        Some(format!("{} mcp", bin.display()))
    );
    ws.read_with(cx, |ws, _| assert!(ws.gemini_job.is_none(), "Cancel runs nothing"));
}
```

In `tests/usage.rs`:

```rust
#[cfg(unix)]
#[test]
fn gemini_cli_links_the_extension_or_says_why_not() {
    use crate::workspace::usage::gemini_run;
    let data = tempfile::tempdir().unwrap();
    // `true` and `false` stand in for gemini: whatever they are given, they succeed or fail.
    assert_eq!(gemini_run(Some("true".into()), data.path(), true), Ok(()));
    assert!(
        data.path().join("gemini").join("gemini-extension.json").exists(),
        "the extension is written first"
    );
    let why = gemini_run(Some("false".into()), data.path(), false).unwrap_err();
    assert!(why.starts_with("Gemini CLI failed"), "{why}");
    let why = gemini_run(None, data.path(), true).unwrap_err();
    assert!(why.contains("didn't find Gemini CLI"), "{why}");
}
```

- [ ] **Step 2:** Run `cargo test -p templum connections_` and `cargo test -p templum gemini_cli`. Expected: FAIL to
  compile (`gemini_job`, `gemini_run` not found).
- [ ] **Step 3: Write the code.** `workspace/bus.rs`:

```rust
/// `<data>/bin/templum` (`templum.exe` on Windows): this Templum, for what outlives it (another
/// agent's configuration, the Gemini CLI extension).
pub(crate) fn templum_in(data: &Path) -> PathBuf {
    data.join("bin")
        .join(if cfg!(windows) { "templum.exe" } else { "templum" })
}
```

and in `templum_folder` use `templum_in(data)` for the Unix link (`let link = templum_in(data);`) and the Windows copy
(`same_or_copy(&exe, &templum_in(data)).ok()?;`). `workspace/mod.rs`: `pub(crate) use bus::templum_in;`; in
`Workspace` beside `message_try`:

```rust
    /// Settings ▸ Connections ▸ Gemini CLI: Install or Remove running, or what went wrong.
    pub(crate) gemini_job: Option<TryIt>,
```

and `gemini_job: None,` in `Workspace::new`. `workspace/usage.rs`: imports `std::ffi::OsString`, `std::sync::Arc`,
`super::TryIt`; add to `impl Workspace`:

```rust
    /// Settings ▸ Connections ▸ Gemini CLI: installs Templum's extension (`install`) or removes
    /// it, in the background; Settings shows what went wrong, if anything did.
    pub(crate) fn setup_gemini(&mut self, install: bool, cx: &mut Context<Self>) {
        let Some(data) = self.data_dir.clone() else {
            return;
        };
        if matches!(self.gemini_job, Some(TryIt::Running(_))) {
            return;
        }
        self.gemini_job = Some(TryIt::Running(Arc::default()));
        cx.notify();
        cx.spawn(async move |ws, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    gemini_run(templum_terminal::locate("gemini"), &data, install)
                })
                .await;
            ws.update(cx, |ws, cx| {
                ws.gemini_job = done.err().map(TryIt::Failed);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
```

and after `impl Workspace`:

```rust
/// Runs Gemini CLI (`gemini`, where Templum found it, if it did) to link Templum's extension,
/// written first (`install`), or to uninstall it; what went wrong, whole, if anything did.
// ponytail: no deadline; a Gemini CLI that hangs keeps the button spinning until Templum quits.
pub(super) fn gemini_run(
    gemini: Option<PathBuf>,
    data: &Path,
    install: bool,
) -> Result<(), String> {
    use templum_usage::gemini;
    let program = gemini.ok_or("Templum didn't find Gemini CLI (gemini) on this computer.")?;
    let args: Vec<OsString> = if install {
        let folder = gemini::extension(data, cfg!(windows)).ok_or(
            "Templum couldn't write its extension: its folder's path holds a ' or a line break, \
             or can't be written.",
        )?;
        gemini::link_args(&folder)
    } else {
        gemini::UNLINK.iter().map(OsString::from).collect()
    };
    let mut command = std::process::Command::new(program);
    command.args(args).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = command
        .output()
        .map_err(|e| format!("Couldn't run Gemini CLI: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Err(format!("Gemini CLI failed ({}):\n{}", out.status, said.trim()))
}
```

`settings/mod.rs`: the note for `Section::Connections`:
`Some("What the agents in the canvas get from Templum: its MCP server, to read and type into the blocks wired to them, and its hooks, for the dot on their terminal.")`.
Create `settings/connections.rs`:

```rust
//! Settings ▸ Connections (docs/specs/connections.md, Config): the switch for the agents Templum
//! opens, Templum's server for other agents to copy, and Gemini CLI's extension.

use std::path::Path;

use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, Size as UiSize};
use gpui_kit::prelude::*;
use gpui_kit::{ClickEvent, Entity, TestSupportExt, div, px};

use super::row::{Row, remove};
use crate::config::Edit;
use crate::grid::hsla;
use crate::prompt;
use crate::workspace::{TryIt, Workspace, templum_in};

/// The questions Install and Remove ask: title, text, button.
const INSTALL: (&str, &str, &str) = (
    "Install Templum in Gemini CLI?",
    "Templum runs gemini extensions link with its extension. Every Gemini CLI you open then starts \
     Templum's server and hooks; outside Templum the server shows as disconnected. Remove takes it \
     back.",
    "Install",
);
const REMOVE: (&str, &str, &str) = (
    "Remove Templum from Gemini CLI?",
    "Templum runs gemini extensions uninstall templum.",
    "Remove",
);

pub(super) fn rows(ws: &Workspace, entity: &Entity<Workspace>) -> Vec<Row> {
    let on = ws.config.connections.enabled;
    let e = entity.clone();
    let mut rows = vec![
        Row::new("connections.enabled", "Agents Templum opens")
            .description(
                "Claude Code and Codex opened from the canvas get Templum's server and hooks: they \
                 read and type into the blocks wired to them",
            )
            .control(Switch::new("connections-switch").checked(on).on_click(
                move |on: &bool, _, cx| {
                    let on = *on;
                    e.update(cx, |ws, cx| {
                        ws.edit(
                            Edit::set("connections", "enabled", on),
                            |c| c.connections.enabled = on,
                            cx,
                        )
                    })
                },
            ))
            .reset(
                !on,
                remove(entity, "connections", "enabled", |c| {
                    c.connections.enabled = true
                }),
            ),
    ];
    let Some(data) = ws.data_dir.as_deref().filter(|_| ws.bus.is_some()) else {
        rows.push(Row::new("connections.none", "No server").description(
            "This Templum runs without its data folder: no agent gets its server, and its \
             terminals have no templum command",
        ));
        return rows;
    };
    let command = format!("{} mcp", templum_in(data).display());
    rows.push(
        Row::new("connections.command", "Other agents")
            .description(
                "Grok Build, Pi, OpenCode or your own: add this command to their MCP servers. It \
                 finds its block from the terminal it runs in; an agent that hands a server only \
                 the variables it names needs TEMPLUM_BUS and TEMPLUM_TERMINAL",
            )
            .control(Clipboard::new("connections-copy").value(command.clone()))
            .below(
                div()
                    .id("connections-command")
                    .font_family(ws.cell.font.family.clone())
                    .text_size(px(12.0))
                    .child(command)
                    .test_support(),
            ),
    );
    rows.push(gemini_row(ws, entity, data));
    rows
}

/// Gemini CLI: Install or Remove, each after a question (it spins while Gemini CLI runs), and
/// under it what went wrong, in red.
fn gemini_row(ws: &Workspace, entity: &Entity<Workspace>, data: &Path) -> Row {
    let ui = ws.ui;
    let running = matches!(ws.gemini_job, Some(TryIt::Running(_)));
    let installed = templum_usage::Dirs::of(data)
        .gemini
        .is_some_and(|dir| templum_usage::gemini::installed(&dir));
    let e = entity.clone();
    let button = div()
        .id("gemini-setup")
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(10.0))
        .py(px(3.0))
        .rounded_full()
        .border_1()
        .border_color(ui.hairline)
        .text_size(px(12.0))
        .when(running, |el| {
            el.child(Spinner::new().with_size(UiSize::Size(px(12.0))))
        })
        .when(!running, |el| {
            el.cursor_pointer()
                .hover(move |s| s.bg(ui.hover))
                .active(move |s| s.bg(ui.pressed))
                .on_click(move |_: &ClickEvent, window, cx| {
                    let e = e.clone();
                    let (title, text, ok) = if installed { REMOVE } else { INSTALL };
                    prompt::confirm(
                        title,
                        text,
                        ok,
                        move |_, cx| e.update(cx, |ws, cx| ws.setup_gemini(!installed, cx)),
                        window,
                        cx,
                    );
                })
        })
        .child(if installed { "Remove" } else { "Install" })
        .test_support();
    let row = Row::new("connections.gemini", "Gemini CLI")
        .description(if installed {
            "Templum's extension is installed: every Gemini CLI you open gets its server and hooks"
        } else {
            "It takes Templum's server and hooks as an extension, installed once for every Gemini \
             CLI you open"
        })
        .control(button);
    match &ws.gemini_job {
        Some(TryIt::Failed(why)) => row.below(
            div()
                .id("gemini-setup-result")
                .p(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(ui.hairline)
                .font_family(ws.cell.font.family.clone())
                .text_size(px(12.0))
                .text_color(hsla(ws.theme.palette.ansi[1], 1.0))
                .child(why.clone())
                .test_support(),
        ),
        _ => row,
    }
}
```

- [ ] **Step 4:** Run `cargo test -p templum settings_panel`, `cargo test -p templum gemini_cli`,
  `cargo test -p templum workspace::tests::usage`. Expected: PASS.
- [ ] **Step 5:** `cargo fmt --all && cargo clippy -p templum --all-targets -- -D warnings`: clean. UI check:
  `cargo run -p templum`, open Settings ▸ Connections, look at the three rows in light and dark themes.
- [ ] **Step 6: Commit.**

```bash
git add crates/templum/src/settings crates/templum/src/workspace/mod.rs crates/templum/src/workspace/bus.rs crates/templum/src/workspace/usage.rs crates/templum/src/workspace/tests/settings_panel.rs crates/templum/src/workspace/tests/usage.rs
git commit -m "feat(connections): Settings ▸ Connections turns the agents' connections off, gives other agents the server and installs it in Gemini CLI" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
```

---

### Task 11: Docs, minors, the full check, the benches, push

**Files:** `docs/specs/connections.md`, `docs/ARCHITECTURE.md`, `docs/minors.md`

- [ ] **Step 1: The spec.** In `docs/specs/connections.md`:
  - Header: `- **Status:** Batches 1 to 5 implemented`.
  - Out of scope, first bullet: replace "The one exception is Gemini CLI, below, and only after the person agrees."
    with "Gemini CLI gets an extension instead, below, only after the person agrees, and never through its settings
    file."
  - Wires, the Labels bullet: "**Labels:** **Rename wire…** in a wire's menu names it (blank removes the name; it is
    put on one line and cut at 60 characters), a step of Ctrl+Z. The label sits in a pill at the curve's middle, in
    the wire's color, scaled with the canvas at the Full level and at its own size at the Silhouette level, as a
    block's title is; grid mode hides it. It comes back with its wire: Ctrl+Z, a closed block brought back, a
    restart."
  - An agent's state, after the table: "**From its screen.** Where its hooks say nothing, its screen tells: once a
    second Templum looks at the last 100 lines of every agent's terminal. One that changed at 3 of the last 4 looks (a
    spinner, Codex's "Working (5s)") is an agent at work, also after a dialog the person answered; one that stayed
    still for 5 looks while it worked is not, and the task it was on ends with its last 40 screen lines. A look right
    after the person typed into it tells nothing. A wait for the person is never ended by a still screen."
  - Hooks: the sentence "Esc while the model writes fires no hook of its own: the dot stays on Working until the
    next prompt, or until `idle_prompt` comes … Deferred: [minors](../minors.md#connections)." becomes "Esc while the
    model writes fires no hook of its own: the dot stays on Working until its screen has been still for 5 seconds
    ([An agent's state](#an-agents-state))."
  - Hooks table: Gemini's row "| Gemini CLI | the hooks of Templum's extension (below), JSON on stdin |
    `SessionStart` started, `BeforeAgent` working, `AfterTool` working again after a permission, `AfterAgent` done
    with `prompt_response`, `Notification` (`ToolPermission`) waiting for the person, `SessionEnd` ended |"; the
    Others row "| Others | — | none: their screen (An agent's state), and Delegating point 6 |".
  - Delegating point 6: "An agent Templum gets no events from answers through `reply`, or its task ends once its
    screen has been still for 5 seconds after working, with its last screen lines, or at its timeout."
  - Tools, the `wait_for` row's "Does": "Waits for the event in the block: `idle` as `wait_for_idle`; `needs_you`, its
    agent or its balloon asking the person (at once when it already does); `command_finished`, the next command to
    finish there, with its output (needs [shell integration](shell-integration.md)); `attention`, its next bell or
    notification".
  - What Templum types: the Gemini paragraph becomes "**Gemini CLI:** it has no flag for MCP servers or hooks, and
    ignores a system settings file root doesn't own. **Settings ▸ Connections ▸ Gemini CLI ▸ Install**, after a
    question, writes `<data>/gemini/gemini-extension.json` (the server, `<data>/bin/templum mcp`, with an hour's
    timeout) and `<data>/gemini/hooks/hooks.json` (the hooks, `'<data>/bin/templum' hook gemini`; `& '…'` on Windows,
    where Gemini CLI runs hooks through PowerShell), and runs `gemini extensions link <data>/gemini --consent`;
    **Remove** runs `gemini extensions uninstall templum`. `~/.gemini/settings.json` is never touched. Every Gemini
    CLI the person opens then has the server and the hooks, typed by hand or not, whatever `[connections]` says;
    outside Templum the server shows as disconnected and the hooks do nothing. Gemini CLI starts no stdio server in a
    folder it doesn't trust; it asks when it first opens one."
  - Others: "**Settings ▸ Connections** shows `<data>/bin/templum mcp` with a Copy button."
  - The command line: add `| \`templum hook gemini\` | Gemini CLI's hooks (JSON on stdin) | 5 |`; the `templum wire`
    row's Tool cell "wires two blocks of the workspace of the terminal it runs in, a step of Ctrl+Z; not a tool (an
    agent's MCP server never lists it); it asks no one, so an agent's shell tool can wire its own block (the person's
    choice)".
  - Config: "**Settings ▸ Connections** (batch 5) has the switch, the server's command for other agents with a Copy
    button, and Gemini CLI's Install and Remove."
  - Architecture: add rows `| \`templum-usage/src/gemini.rs\` (new) | the Gemini CLI extension | 5 |`,
    `| \`templum/src/workspace/activity.rs\` (new) | an agent's state from its screen | 5 |`,
    `| \`templum/src/settings/connections.rs\` (new), \`workspace/usage.rs\` | Settings ▸ Connections; running Gemini
    CLI | 5 |`, and add "labels (`Change::Label`)" to the wires row.
  - Testing: append to Unit (`templum-mcp`) "(batch 5) `wait_for` takes its four events and refuses another; `wire`
    is parsed but never listed; Gemini CLI's hooks become their events; `wait --for` and `wire` send their calls, and
    every verb's usage offers `--json`"; add "**Unit (`templum-usage`, batch 5):** the extension's two files exactly,
    not rewritten unchanged, Windows' hook command, a `'` in the path gives none." and "**Unit (`templum`, batch
    5):** the screen's looks (3 of 4 changed: at work; 5 still: at rest; a wait for the person stays); a label's scale
    per level."; append to Headless UI "(batch 5) `wait_for` answers `needs_you` at once or when the agent or its
    balloon asks, a command's end and code, a bell and a notification, its timeout with the last lines, a closed
    block, and shows no balloon; `templum wire` wires a step Ctrl+Z takes back and an MCP client can't call it; a
    changing screen says Working and a still one ends the task, the person's typing not counting; Rename wire…
    labels, Ctrl+Z, a disconnect, a close and a restart keep it, a hand-edited file's label is trimmed or dropped;
    the pill shows at 100%, 50% and 25% and not in grid mode; Settings ▸ Connections' switch, its Copy button and
    Gemini's question; Gemini CLI's run, its failure and its absence."; append the manual checks below to Manual.
  - Risks: the Gemini row becomes "| Gemini CLI ignores a system settings file root doesn't own | An extension the
    person installs once from Settings ▸ Connections |"; the "agent without events" row's answer becomes "`reply`;
    its screen going still ends its task (`activity.rs`); else the wait's timeout with the last screen lines"; add
    "| A spinner drawn under a permission dialog would turn its yellow dot to Working | Checked by hand with Claude
    Code and Gemini CLI (manual check 4) |" and "| Windows: PowerShell may not wait for, or hand stdin to,
    `templum.exe` (a windowed program) in a Gemini hook | Checked by hand on Windows; else `$input \| & '<bin>' hook
    gemini` |".
  - Files: the `wires` example already has `label`; add "A label is trimmed and cut at 60 characters; a blank one is
    none."
- [ ] **Step 2: `docs/ARCHITECTURE.md`**: in `templum-usage`'s row add "the Gemini CLI extension (`gemini.rs`)".
- [ ] **Step 3: `docs/minors.md`** (## Connections): delete "A Claude Code interrupted while it writes stays on
  Working for a while" and "A Claude Code stays "waiting for you" while a tool the person allowed runs" (fixed by
  Task 7). Rewrite "Codex's dot never says "Working"" as "**Codex's dot never turns yellow.** Codex says nothing when
  its approval dialog opens (`codex`, `crates/templum-mcp/src/hooks.rs`): its dot says Working while its screen moves
  (`crates/templum/src/workspace/activity.rs`) and nothing once the dialog sits still; a person doesn't see it waits
  for them, and `delegate` may type into the dialog. Fix: a Codex hook for approvals, when Codex has one." Rewrite
  "A Bash command sent to the background may show a working Claude Code as idle" so its symptom is the remaining
  one: the dot comes back to Working within 3 seconds (Task 7), but a task handed to it ends at once with "Its turn
  ended without an answer"; keep its fix (tell Esc from Ctrl+B with manual check 14). Add any minor the reviews of
  this batch defer, under ## Connections, "Left by batch 5".
- [ ] **Step 4: The full check:** `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo
  test --workspace` and `cargo deny check`. Expected: all pass; report the test count.
- [ ] **Step 5: The benches** (Linux, release): `cargo run --release -p templum -- --bench` and
  `cargo run --release -p templum -- --bench-mixed`; keep both JSON lines for the PR. Expected: `p95_ms` ≤ 16.7 each.
- [ ] **Step 6: Commit and push.**

```bash
git add docs/specs/connections.md docs/ARCHITECTURE.md docs/minors.md
git commit -m "docs(connections): batch 5 in the spec: Gemini's extension, wait_for, templum wire, labels, the screen's looks" -m "Claude-Session: https://claude.ai/code/session_01EYN2Ndnbrv88WrHFTEdKcU"
git push origin development
```

  Pushing updates draft PR #36 (dev→main), which runs CI on the 3 OS: watch it (`gh pr checks 36`) and paste the bench
  JSON lines into its description.

## Manual checks (Hyprland, `cargo run --release -p templum`)

1. **Gemini CLI:** Settings ▸ Connections ▸ Gemini CLI ▸ Install ▸ Install; the row then says installed and offers
   Remove; `gemini extensions list` shows `templum`. Open Gemini CLI from the canvas in a trusted folder: `/mcp` lists
   `templum` connected; "list the blocks" works; the dot turns accent while it works, yellow at a tool permission,
   none when done. Remove; `gemini extensions list` no longer shows it.
2. **Claude Code delegates to Gemini CLI** (wired): the answer comes back as `delegate`'s result.
3. **Codex's dot:** Codex from the menu: the dot turns accent within ~3 s of a prompt and goes when it is done.
4. **A dialog stays yellow:** Claude Code at a Bash permission dialog, untouched for 15 s: still yellow. Allow a
   `sleep 20`: the dot turns accent within ~3 s. Esc while Claude Code writes: the dot goes within ~5 s. Typing a
   long prompt into an idle Claude Code: no accent dot.
5. **`templum wait`:** in a terminal with shell integration wired to another, `templum wait <other> --for
   command_finished` while `sleep 3; false` runs there: it prints the command and exit code 1; `--for attention` and
   `printf '\a'` there: "rang its bell".
6. **`templum wire a b`** from a terminal; the wire appears; Ctrl+Z on the canvas takes it back.
7. **Labels:** right-click a wire ▸ Rename wire… ▸ "dev server": the pill at the middle; zoom to 50% and 25% (still
   readable, 12 px); grid mode hides it; restart keeps it; Ctrl+Z after a rename takes the name, not the wire.
8. **Settings ▸ Connections:** the switch off, then a Claude Code from the menu: its command has no `--mcp-config`;
   the Copy button copies `<data>/bin/templum mcp`.
9. **Windows** (when one is at hand): Gemini CLI's dot follows its turns (its hooks run `templum.exe` through
   PowerShell).

## Self-review

- Spec coverage: `wait_for` (Tasks 1, 2, 5), `templum wire` (1, 2, 6), `--json` (2), labels (8, 9), Settings ▸
  Connections (10), Gemini CLI (3, 4, 10), the screen's looks (7), docs (11). Gemini's server for "Others" agents:
  Task 10's Copy row.
- Types: `tools::Event` (Task 1) is used by Tasks 5's `wait_for`; `Waiting::Event` (5) by `balloon.rs`;
  `waits::LAST_LINES` made `pub(super)` in 5, used in 7; `Space::label_key` (8) used by Task 9's bench; `templum_in`
  (10) used by Settings; `gemini_job`/`setup_gemini` (10) named the same in the row and the test.
