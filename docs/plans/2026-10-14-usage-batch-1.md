# Agent usage, batch 1 (Claude Code and Codex): Implementation Plan

**Goal:** a terminal running Claude Code or Codex shows its session's usage; the title bar shows the plan limits;
a notification says when a limit nears its end and when one that was reached resets; Settings ▸ Usage tunes it.

**Spec:** [docs/specs/usage.md](../specs/usage.md). Decisions made with the maintainer: Claude Code's limits only
for the Claude Code Templum types (`--settings`, never editing the user's files); never an agent's credentials;
reset notifications by default only for a reached window (95%), the warning at 90%; meter + popover + chip;
`rusqlite` allowed, but only in batch 3.

**Architecture:** a new UI-free crate, `templum-usage`, reads the agents' files; the workspace reads it in the
background after each 5-second snapshot, from the processes the snapshot found (`Inspection` keeps the agent's pid
and start time), and keeps each terminal's `Session` and each agent's `Limits`; `Watch` turns limits into events.

## Steps (each with the test that fails without it)

1. `templum-terminal`: `Inspection.pid` and `.started`; `Terminal::id`, exported as `TEMPLUM_TERMINAL`.
   Tests: `inspect_finds_an_agent_and_its_folder`, `each_terminal_gets_its_own_id_in_its_environment`.
2. The snapshot hands each terminal its whole inspection (`SpaceSnapshot`, `Space::see`).
   Test: `a_snapshot_tells_each_terminal_its_folder_and_agent`.
3. `templum-usage`: `Session`, `Window`, `Limits`, `Reset`, `Proc`, `Dirs`, `tail`, `Watch`. Tests in `watch.rs`.
4. `claude.rs`: the relay, the settings file and its typed path, the relay file and the transcript, limits, pruning.
5. `codex.rs`: rollouts matched to processes (`/proc` on Linux), sessions and limits; `read`.
6. `[usage]` in `config.rs`; Settings ▸ Usage. Tests: config parsing, the template, the choices, the switch.
7. `templum --claude-statusline` in `main.rs`; `Workspace::typed` on launch and resume.
   Test: `the_claude_code_templum_opens_gets_its_status_line`.
8. `read_usage` / `usage_done` / notifications (`workspace/usage.rs`); relay files pruned at startup.
   Test: `a_limit_near_its_end_is_told_once`.
9. The meter, the popover, the chip (`usage.rs`, `title_bar.rs`, `block.rs`, `render.rs`; Esc in `input.rs`).
   Tests: `the_meter_opens_the_usage_and_a_session_shows_its_terminal`, the chip's and reset times' unit tests.
10. Docs: the spec, ARCHITECTURE.md, README, agents.md's out-of-scope line, minors.

## Manual checks (Hyprland)

1. Claude Code from **AI agent ▸**: after an answer, the chip; on Pro/Max, the meter. `ps` shows `--settings`.
2. The meter opens the popover; a session row focuses its terminal; Esc and a click outside close it.
3. `claude` typed by hand: the chip with the context in tokens.
4. Codex: after a turn, its chip and its limits in the popover.
5. Settings ▸ Usage writes `[usage]`; `warn = 80` on an account past 80% notifies once.
6. A status line of your own in `~/.claude/settings.json` still shows in the Claude Code Templum opens.
