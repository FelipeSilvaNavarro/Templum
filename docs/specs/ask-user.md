# Questions from agents: `ask_user` v2

- **Date:** 2026-10-17
- **Status:** Implemented (kanban card 34, "Correções ask_user"); plan:
  [2026-10-17-ask-user-v2.md](../plans/2026-10-17-ask-user-v2.md)
- **Builds on:** [connections.md](connections.md#requests-from-agents) (the balloon, the tools, the notices typed
  back), [settings.md](settings.md), [shell-integration.md](shell-integration.md#news-and-notifications) (alerts and
  notifications)

## Goal

An agent that asks the person something through `ask_user` gets less than Claude Code's own `AskUserQuestion`: one
question, buttons or a text field but never both, a bare string back. The person, meanwhile, misses questions: they
expire after 30 minutes and make no sound. This brings `ask_user` close to `AskUserQuestion`, whose shape agents
already know (several questions in one dialog, a header, options with a description and a preview, checkboxes, an
Other field, a note, an answer per question), keeps what Templum's has and the native one lacks (a timeout, the
dialog on the asking block, parallel calls, any number of options), lets a question wait until it is answered, and
tells the person with a sound and, when they are away, a notification and an alert.

**Success criteria**

1. Today's calls (`question`, `options` as strings, `timeout`; `templum ask`) show today's dialog, with Dismiss added,
   and get the answer as plain text: one of their options, or what was typed when they have none.
2. A call with `questions` shows them all in one dialog: each with its header, its options (label, description; a
   checkbox when `multiSelect`), a preview beside the options, and an Other field; one note for the whole call. The
   answer comes back as JSON, an answer per question.
3. By default a question waits until the person answers or dismisses it; Settings ▸ Connections can set a limit.
4. An agent whose client stops waiting doesn't lose the person's answer: it is typed into its terminal.
5. A request makes a short sound; one the person doesn't see is a notification and an alert (the workspace's dot,
   Ctrl+Shift+A).
6. The dialog works with the keyboard, and scales like a photo at every zoom.

## Out of scope

- `connect`'s dialog and timeout (it only shares the sound).
- New flags for `templum ask`: it keeps today's shape (one question, `--option`).
- MCP's `structuredContent` and `outputSchema`: the JSON is the result's text. Add them when a client uses them.
- Progress notifications that keep a client's idle timer alive. Add them if a client gives up on a call despite the
  timeout Templum gives it (see [When the client stops waiting](#when-the-client-stops-waiting)).
- A note per question (one per call), a sound of Templum's own, choosing the sound.

## The tool

### Arguments

```json
{
  "questions": [
    {
      "question": "Which database should the service use?",
      "header": "Database",
      "options": [
        { "label": "Postgres", "description": "What production runs", "preview": "DATABASE_URL=postgres://…\npool = 10" },
        { "label": "SQLite", "description": "One file, no server" }
      ],
      "multiSelect": false
    },
    {
      "question": "Which features go in the first release?",
      "header": "Features",
      "options": [{ "label": "Auth" }, { "label": "Search" }, { "label": "Export" }],
      "multiSelect": true
    }
  ],
  "timeout": 600
}
```

- **`questions`**: one or more, no maximum (the dialog scrolls). Each has:
  - `question`, required and not blank; no two questions of a call share their text (the answer is keyed by it);
  - `header`, optional: a short label, shown up to 12 characters, a longer one cut with "…";
  - `options`, optional, any number; none leaves only the Other field. An option is `{ "label", "description",
    "preview" }` (only `label` required, not blank, unique within its question), or a plain string, its label;
  - `multiSelect`, false unless given.
- **`question`** with **`options`** (strings), today's shape: one single-select question, answered as plain text, in
  today's dialog: a button per option and no Other, or a field when it has none; no note. Scripts that check for one
  of their options keep working. A call with both `question` and `questions` is refused: "Give question or questions,
  not both."
- **`timeout`**: whole seconds from 1 to 3600, as every tool's; optional ([How long it waits](#how-long-it-waits)).
- A refusal names the place: "questions[1].options[0] needs a label", "questions[0] and questions[2] have the same
  text: the answers are keyed by it."
- The schema lists both shapes and requires neither, so a client that checks arguments against it before calling
  still sends today's calls. Option items are declared as objects only (no `anyOf`, which not every client's model
  API takes); strings are accepted all the same.
- The description, which agents read: "Asks the person in a dialog on your block, as AskUserQuestion does: questions,
  each with a header, options (label, description, preview) and multiSelect; every question also gets an Other field,
  and the person may add a note. Returns {"answers": {question: answer, or a list for multiSelect}, "note": …}. Waits
  until the person answers: no limit unless you give timeout (seconds) or the person set one; if your client stops
  waiting, the answer is typed into your terminal later. The older question with options (strings) still works and
  returns the answer as text."

### The answer

- **A `questions` call:** the result's text is one JSON object:
  `{"answers": {"Which database should the service use?": "SQLite", "Which features go in the first release?":
  ["Auth", "Export"]}, "note": "keep it small"}`. A single-select question's answer is a string; a multiSelect's is a
  list, in the options' order with Other's text last. Other's text is the answer as typed (trimmed). `note` is there
  only when the person wrote one.
- **A `question` call:** the option's label or the text typed, as today.
- **Dismissed:** an error result, "The person dismissed the question without answering."
- **Past its deadline:** as today, "No one answered within 600 seconds; the request was withdrawn."

### How long it waits

- **The setting** `[connections] question_timeout`: minutes, 0 to 1440, 0 by default, which waits until the person
  answers.
- **Which wins:** the shorter of the call's `timeout` and the setting (when not 0); with neither, there is no
  deadline. The call's timeout is the agent's own need ("answer within two minutes or I go on with the default"), the
  setting is the person's cap on how long questions hang around; taking the shorter honors both, and neither can make
  the other wait longer than it said. A change to the setting applies to questions asked after it.
- `connect` keeps its 1800-second default.

### When the client stops waiting

What each client does with a long tool call (read in their documentation, 2026-10-17):

| Client | A long call | What Templum does |
|---|---|---|
| Claude Code | Gives up on a call that sends nothing for 1800 s, its idle timeout for stdio servers (`CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT`; seen 2026-10-17 with a Templum `mcp.json` without `timeout`: `MCP server "templum" tool "wait_for_idle" sent no response or progress for 1800s; aborting. If this server is configured in your MCP settings, set a per-server "timeout" (ms)…`). A server's own `timeout` lifts it (Claude Code v2.1.203 or later, the version that brought the idle timeout to stdio servers: before it they had none), and replaces `MCP_TOOL_TIMEOUT` (100000000 ms, about 28 hours, by default). Moves a call past 2 minutes to the background, where its documentation says it keeps waiting; not checked by hand yet ([minors](../minors.md#connections)). Esc in the agent cancels the call (`notifications/cancelled`) | The `mcp.json` it is given sets `"timeout": 604800000` on Templum's server |
| Codex | Gives up at the server's `tool_timeout_sec` | `tool_timeout_sec=604800` (it was 3600) |
| Gemini CLI | Gives up at the server's `timeout` (ms; 10 minutes unless set); the MCP TypeScript SDK sends `notifications/cancelled` when it does | The extension's `timeout` is 604800000 (it was 3600000); one linked before gets it at Templum's next start |
| Others (the command in Settings ▸ Connections) | Their own limits | The fallback below |

- Seven days is longer than any question is left in practice, and stays under the largest JavaScript timer (2³¹ − 1
  ms, 24.8 days; a longer one fires at once), which Claude Code and Gemini CLI run on.
- **A cancelled question** (the client gave up, or the person pressed Esc in the agent) stays in the dialog, which
  says under its first line: "The agent stopped waiting: your answer will be typed into its terminal." Answered, the
  answer is typed into the agent's terminal as a notice (`queue_notice`, the path of
  [connections.md's Delegating 10](connections.md#delegating): typed once the agent takes typing):
  `[answer to your question "Which database should the service use?"] The answer is in the pasted text.`, then on
  the next line `{"answers": …}`, the text the tool would have returned; the question is the first question's first
  line (at most 60 characters, cut with "…"). A long answer is a paste in Claude Code ("[Pasted text]"), which it
  takes for data unless typed text comes with it: the first line is that text, as delegate's notices. Dismiss drops
  it. Its deadline, if it has one, drops it silently: no one waits for the refusal.
- The connection ending (the agent quit, `templum ask` stopped with Ctrl+C) withdraws its questions, as today, and
  so does closing the block.
- Either way the person's answer reaches the agent: as the call's result while the client waits, typed in once it
  cancelled. Claude Code's 1800-second idle timeout is lifted by the server `timeout` Templum gives it, so a question
  waits there as long as the person takes; a Claude Code given no such `timeout` (opened by hand with its own MCP
  settings) gives up at 1800 s, and whether that abort sends a cancel isn't known.
- A client that drops a call without a cancel loses an answer that comes after it gave up; that happens only past its
  own timeout, which Templum sets to seven days where it can.

## The dialog

The balloon of [connections.md](connections.md#requests-from-agents) (centered on the terminal under its title bar,
80% of its width, at most 80% of its height and scrolling past that, the yellow frame and "Waiting for your answer",
"1 of 3" when several requests of the block wait in turn) holds the call's questions:

```
● Waiting for your answer                                         1 of 2
 Database  Which database should the service use?
   ○ Postgres                        ┌──────────────────────────────┐
     What production runs            │ DATABASE_URL=postgres://…    │
   ● SQLite                          │ pool = 10                    │
     One file, no server             └──────────────────────────────┘
   ○ Other [                    ]
 ───────────────────────────────────────────────────────────────────────
 Features  Which features go in the first release?
           Pick any
   ☑ Auth
   ☐ Search
   ☑ Export
   ☐ Other [                    ]
 ───────────────────────────────────────────────────────────────────────
 Note [ optional                                                     ]
 Dismiss                                                           Send
```

- **Questions** stack in their order, a hairline between them. Each starts with its header in a chip (a rounded
  frame in `title_dim`), then its text, whole; a multiSelect's says "Pick any" under it, dim.
- **Options:** one per row, whole and wrapped: a radio (single-select) or a checkbox (multiSelect) drawn with divs,
  a circle or a rounded square filled with the accent when picked; the label; the description under it, in
  `title_dim`, at 12 world units (the label at 14, as today's text).
- **Other** is every question's last row: its radio or checkbox, "Other" and a field. Typing in the field picks it
  (in a single-select question, the option picked before is unpicked); emptying it unpicks it; picking an option of a
  single-select question unpicks Other, its text kept. A question without options shows the field alone, with no
  radio, as today's question without options.
- **Preview:** a question with an option that has a preview lays out in two columns, its options on the left (45%),
  the preview on the right: the highlighted option's ([Keyboard](#keyboard); hovering an option highlights it), else
  the picked one's, else the first one's with a preview; an option without one shows "No preview" dim. In the
  terminal's font at 12 world units, never wrapped, in a frame on the canvas color, at most 18 lines tall, scrolling
  both ways.
- **Note:** one field after the questions, "Note (optional)".
- **The foot:** Dismiss on the left; Send on the right, enabled once every question has an answer (an option picked,
  or Other's text), dim before.
- **One single-select question:** a click on an option answers at once, with the note typed so far, as today's
  buttons do.
- **Today's shape** draws today's dialog: its options as buttons, a click answering; or, with none, the field and
  Send. No Other row, no note; Dismiss at the foot as for any question.
- **A cancelled question** says "The agent stopped waiting: your answer will be typed into its terminal." under the
  first line, in `title_dim`.
- Every size is a world size × the zoom, the fields and the preview's font included, so the dialog scales like a
  photo: no zoom changes where its text wraps. It is drawn at the Full level of detail only, as today.

### Keyboard

- The dialog has the keyboard while its block does (focused, or under the pointer with focus following it), from the
  moment it shows, until Esc gives it back to the terminal. Given back, it stays the terminal's, also when the
  pointer leaves the block and comes back, until a click on the dialog takes it again. Meanwhile no key reaches the
  terminal; shortcuts still run, as they do from today's field.
- **The highlight** is on one row (an option, an Other row, the note or Send) or on none, as the dialog opens. ↓ and
  Tab put it on the first row, ↑ and Shift+Tab on Send, then move it; hovering a row puts it there. With no row
  highlighted, Space and Enter do nothing: a key meant for the terminal while the pointer drifts over the block
  can't answer.
- **Space** picks the highlighted option (single-select) or checks and unchecks it (multiSelect).
- **Enter** on a single-select question's option picks it first. Then, anywhere (an option, an Other field, the note,
  Send): once every question has an answer it sends; before, it moves the highlight to the next question without one
  (wrapping to the first). So one single-select question answers on Enter, as on a click, and a question without
  options sends what was typed, as today; a multiSelect's options are checked with Space, never Enter.
- On an Other row or the note, the field has the keyboard (GPUI's focus) and types; Tab and Shift+Tab, and ↑ and ↓
  where the field lets them through, move on.
- **Esc** gives the keyboard back to the terminal; the dialog stays.
- **Presses too soon:** keys and clicks on its options, Send and Dismiss are ignored for 0.5 s after the dialog first
  shows a request (it arrives, or the one before it is answered): someone typing or clicking in the terminal as it
  appears doesn't answer it.
- A request to connect keeps its buttons and takes no keys.

## Telling the person

- **Notification:** as today (connections.md, since 1bb9cf1): when the person doesn't see the dialog (another
  workspace, outside the view, grid mode, too small for the Full level and not focused, or the window in the
  background), a notification says "Asks you: *the first question*", with "(and 2 more)" when the call has more; the
  system's too in the background, when `terminal.notifications` is on. A click shows the block.
- **Alert:** in that same case the asking block gets an alert, "Asks you: …", as a bell gives one: its workspace's
  tile shows the yellow dot and Ctrl+Shift+A (`go_to_waiting`) goes there, so a person coming back finds the question.
  Seeing the block clears the alert, as any alert's. No second notification comes from it.
- **Sound:** a short system sound when a question or a request to connect arrives, seen or not, unless
  `[connections] sound = false`; at most one every 2 seconds, so parallel calls don't stack. The platform's own
  player plays it in a background thread; one missing or failing is silence, never an error:
  - Linux and the BSDs: `canberra-gtk-play -i message-new-instant -d Templum` (the desktop's sound theme, which
    honors its switch for event sounds); when it is missing or fails, `pw-play`, then `paplay`, with
    `/usr/share/sounds/freedesktop/stereo/message-new-instant.oga` when that file exists.
  - macOS: `afplay /System/Library/Sounds/Glass.aiff`.
  - Windows: `powershell -NoProfile -NonInteractive -Command "[System.Media.SystemSounds]::Asterisk.Play();
    Start-Sleep -Milliseconds 1500"` with `CREATE_NO_WINDOW`, as Templum's other commands (the user's sound scheme;
    the sleep lets the sound finish before PowerShell exits).
  - **No audio crate:** GPUI's system notifications are silent everywhere (gpui-fast-linux's notify-rust call sets
    no sound hint, its Windows toast is `silent`, its macOS content sets no sound), and an audio crate (rodio, through
    cpal) would add ALSA's headers to every Linux build and libasound to the packages, for one chime.

## Settings

Settings ▸ Connections, two rows after the switch for the agents Templum opens:

| Field | Control | Option |
|---|---|---|
| Questions wait | Number 0–1440 minutes, step 5; its description: "How long an agent's question waits for your answer, in minutes. 0 waits until you answer" | `connections.question_timeout` |
| Sound | Switch, on: "A short sound when an agent asks you something" | `connections.sound` |

`config.toml`'s template:

```toml
[connections]
## The Claude Code and Codex Templum opens get its MCP server and hooks: they can read and
## type into the blocks wired to them (docs/specs/connections.md).
# enabled = true
## How long an agent's question waits for your answer, in minutes; 0 waits until you answer.
# question_timeout = 0
## A short sound when an agent asks you something.
# sound = true
```

A `question_timeout` outside 0–1440 is an invalid file, as any bad value is.

## Architecture

| File | Change |
|---|---|
| `templum-mcp/src/ask.rs` (new) | `Ask` (the questions, and whether the call used `questions`), `Question`, `Choice`; parsing both shapes; the answer's text (JSON or plain), the notice's; `Form`: what is picked, the rows, the highlight, answered or not. Headless, unit-tested |
| `templum-mcp/src/tools.rs` | the schema and description; `Call::AskUser { ask, timeout: Option<u64> }` |
| `templum-mcp/src/lib.rs` | Codex's `tool_timeout_sec=604800` |
| `templum-usage/src/claude.rs`, `gemini.rs` | the server's `timeout`, 604800000 ms |
| `templum/src/config.rs`, `settings/connections.rs`, `settings/fields.rs` | `question_timeout`, `sound`; their rows |
| `templum/src/workspace/waits.rs` | the deadline from the setting; a cancel keeps a question; the front-of-queue time |
| `templum/src/workspace/questions.rs` (new) | a question's fields (one per question's Other, the note), picks, Send, Dismiss, keys, the answer typed back once cancelled, the alert |
| `templum/src/workspace/input.rs` | the dialog's keys before the terminal's |
| `templum/src/balloon.rs` | the dialog |
| `templum/src/chime.rs` (new) | the platform's player |

No new dependency.

## Testing

- **Unit (`templum-mcp`):** both shapes parse; strings and objects as options; each refusal (blank, duplicate texts or
  labels, both shapes, a bad `multiSelect` or `timeout`); a long header cut; the JSON answer (string, list in option
  order with Other last, note only when written) and the plain one; the notice's line; `Form`: picking in single and
  multi, Other typed and emptied, the rows of each shape, the highlight from none and its moves and wrap, the next
  question without an answer, answered or not; Codex's flag, the `mcp.json` and Gemini's extension
  carry seven days.
- **App (`workspace/tests/requests.rs`, `config.rs`, `settings/tests.rs`):** today's call still draws today's dialog
  and answers its label and its typed text; a `questions` call answers its JSON through clicks, checkboxes, Other and the note; Send stays off
  until every question has an answer; one single-select question answers on a click; Dismiss refuses; no deadline by
  default, the setting's or the call's, the shorter; a cancelled question stays, says so, and its answer is typed as a
  notice; the connection's end still withdraws it; the dialog's keys (moves, Space, Enter, Tab into a field, Esc and
  the keyboard staying the terminal's after it) and none reaching the terminal; Enter with no row highlighted; presses within 0.5 s ignored; the preview of the hovered option; sizes × zoom; the
  notification's "(and N more)"; the alert when unseen and none when seen; a sound is due once per 2 seconds, none
  with the setting off; the two settings' defaults, limits and rows.
- **By hand:** Claude Code, Codex and Gemini CLI each asking with `questions` and getting the JSON; a question left 3
  minutes in Claude Code (is the call cancelled once backgrounded? the answer reaches it either way); Esc in Claude Code while it asks, then an answer typed into
  it; the sound on Linux (with and without `canberra-gtk-play`), macOS and Windows; the dialog at far and near zooms.
- **Bench:** the dialog paints, so the pull request carries `--bench` and `--bench-mixed`.

## Risks

| Risk | What we do |
|---|---|
| A key or click meant for the terminal answers a question that just appeared | Presses within 0.5 s of it showing are ignored; no row is highlighted until an arrow, Tab or the pointer puts one there; Enter sends only once every question has an answer |
| A question with no deadline covers its terminal for good | Dismiss; Esc gives the keyboard back; the agent's own Esc keeps it but says the agent stopped waiting |
| An answer typed into an agent that moved on confuses it | The notice names the question it answers; it is typed only once the agent takes typing (batch 8's rule) |
| A client gives up without a cancel | Seven-day timeouts where Templum sets them; others are documented as the fallback's limit |
| No player on the system | Silence, as a muted desktop; the notification and alert still tell |
| A JavaScript timer past 2³¹ ms fires at once | Seven days (604800000 ms) |
