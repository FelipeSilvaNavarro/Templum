# The Templum API: your Templum, from anywhere

- **Date:** 2026-10-24
- **Status:** Batch 1 implemented (kanban card 171, "the Templum ecosystem API"; plan:
  [2026-10-24-api.md](../plans/2026-10-24-api.md)); batch 2 and Templum to Templum are designed here for
  later plans. For people: [docs/api.md](../api.md).
- **Builds on:** [connections.md](connections.md) (the bus, block ids, the tools, the `templum` command line),
  [templum-agent.md](templum-agent.md) (the control server: a tool for every hand action, and their danger),
  [agents-view.md](agents-view.md) (an agent's state), [ask-inbox.md](ask-inbox.md) (the requests),
  [automations.md](automations.md), [settings.md](settings.md), [workspaces.md](workspaces.md)

The person said "pode decidir, fomente bem a ideia para ser uma funcionalidade bem boa" (decide it, grow the idea into
a really good feature) and, on security, "take the proper measures to be safe, with encrypted connections". The calls
below are the orchestrator's, marked *the orchestrator's call*; what the designer settled is marked *the designer's
call*.

## Intent

Card 171, in the person's words (translated): "My idea is that the user can expose an API of their Templum, so we have
an ecosystem. I have a Templum open on one computer and another on another computer; I could connect to the API and
see everything that is happening, move blocks, create blocks, do literally everything. The same for mobile: the person
leaves home and gets an ask_user on the phone, or fires other agents, or sees how things are going. And Templum to
Templum: personal PC and work PC. We need a key system: keys with an expiry date or unlimited. Most important: Templum
only opens the access ports; the user is responsible for exposing it on the web with a domain, VPN or whatever — we
have no server. I thought HTTP/HTTPS, because with a cloudflared tunnel (my case) raw TCP is a hassle. How to sync the
canvas in real time? One user moves something and it changes on the other PC, and terminals are seen live too. Two
parts: 1) a global API with access to everything and access control, all documented, so a user can build their own
dashboard on it; 2) Templum-to-Templum connections. Every feature of the system must be in the API, or everything
breaks."

Today everything Templum does happens in front of its window. An agent that asks a question while the person is out
waits until they come back; a dashboard, a phone or a second computer has no way in. The agents already have two doors
(the block server and the Templum Agent's control server, both on a local socket), but neither is reachable from
outside the computer, and neither has keys a person can hand out and take back.

The **Templum API** is a third door: an HTTP server inside Templum, off until the person turns it on, that answers only
to **tokens** the person makes, with a scope and an expiry, and revokes at will. It serves the same tools the control
server serves, resource-shaped JSON for the things a dashboard lists, and one live stream of everything that changes.
Templum never runs a server of its own: the person points a tunnel, a proxy or a VPN at the port.

## Goals

1. **Off and private by default.** Nothing listens until the person turns the API on in Settings ▸ API; then it
   listens on `127.0.0.1`, where a cloudflared tunnel or a reverse proxy reaches it.
2. **Keys the person controls.** Tokens with a name, a scope (read, control, full), an expiry (a date, N days, never)
   and optionally the workspaces they may see; shown once, stored only as a hash, revoked at once, each with its last
   use.
3. **Everything, in one registry.** Every control tool is `POST /v1/tools/{name}`; the main things have REST
   resources; the OpenAPI document is generated from the same registry, so a tool added is an endpoint and its docs.
4. **Live.** `GET /v1/events` streams a snapshot, then every change: blocks added, moved, resized, removed; agents'
   states; requests opened and answered; cards; automations; resumable after a dropped connection.
5. **The phone case works on batch 1's API alone:** see the requests (questions, plans, yes/no requests, the Templum
   Agent's approvals) and answer them; see the agents and their states; open an agent and hand one a task; read and
   move cards.
6. **Encrypted off the computer.** Any address other than loopback requires TLS: Templum's own certificate, whose
   fingerprint it shows for pinning, or the person's.
7. **Documented for people:** [docs/api.md](../api.md) with a quickstart (turn it on, make a token, run cloudflared,
   curl, events from JavaScript), and `GET /v1/openapi.json` for tools.
8. **Ready for Templum to Templum:** what a second Templum needs to mirror a workspace (workspace-limited tokens,
   stable ids, a state complete enough to draw, a pinned certificate) is in batch 1; terminals' live frames in batch 2.

## Non-goals

*The orchestrator's call* for each:

- A server or relay Templum runs; any hosted service.
- User accounts, passwords, OAuth: tokens only.
- GraphQL; raw TCP; WebSocket (SSE does everything below: [Why SSE](#why-sse)).
- Part 2's connection UI (a remote workspace in the sidebar): a later card ([Templum to Templum](#templum-to-templum)).
- A mobile app, or a web page served by Templum (the person: "eu disse que queria só a api e o sistema de conexão",
  only the API and the connection system): a phone reaches Templum through the API with a client of its own.

*The designer's call:*

- Changing `config.toml` from the API (`set_setting` stays a planned control tool), or reading files outside what the
  tools already read (an editor's files, attachments' bytes).
- MCP over HTTP (the agents keep the local bus): the API's shape is REST and SSE.

## Anti-goals (a failure even if it works)

- Anything listening on the network that the person didn't turn on, or listening beyond loopback in plain HTTP.
- A token readable from the data folder, a log, a URL, a crash report or the audit log.
- A feature that works in the app and not in the API, from this card on (CLAUDE.md's rule).
- The UI thread waiting on the network, or painting slowed while a client is connected.
- An agent in a terminal getting the API's powers through the bus.
- A stream that grows memory without bound because one client reads slowly.

## Words

- **Token:** the key a client sends in `Authorization: Bearer tpl_…`. Made in Settings ▸ API, shown once.
- **Scope:** what a token may do: **read**, **control** or **full**, each including the one before.
- **Client:** an open connection that showed a valid token.
- **Ticket:** a short-lived, single-use key for `GET /v1/events` from a browser, which can't set headers there.
- **State:** everything the API shows as data, as one JSON document: workspaces, blocks (with their agents), wires,
  requests, boards, cards, automations, usage.
- **Event:** one change to the state, or a happening (a notice), on the stream.

## What a person sees

### Settings ▸ API

A new section after Connections (*the designer's call*: next to the other doors). From top to bottom:

| Field | Control | Saved as |
|---|---|---|
| Turn on the API | Switch, off. Under it, when on: "Listening on http://127.0.0.1:7411" (the address in use, `https` with TLS), with **Copy**, or why it isn't ("Port 7411 is in use: choose another port.") | `enabled` |
| Who can reach it | Dropdown: **This computer only (127.0.0.1)**, **Every network of this computer (0.0.0.0)**, **One address…** (a text field, an IP of this computer) | `listen` |
| Port | Number 1024–65535, default **7411** (*the designer's call*: fixed, so a tunnel's config stays right) | `port` |
| HTTPS | Dropdown: **Off: a tunnel or proxy gives HTTPS** (only with This computer only), **Templum's certificate**, **My certificate…** (two file fields: certificate and private key, PEM) | `tls` |
| Certificate | With Templum's certificate: its SHA-256 fingerprint (`AB:CD:…`, 32 pairs) with **Copy**, **Save certificate…** (writes a `.pem` to install on a phone) and **New certificate** (asks first: "Clients that pinned the old one stop connecting.") | `<data>/api/cert.pem` |
| Allowed web pages | Text, one origin per line (`https://dash.example.com`); empty: no web page may call the API (CORS off) | `origins` |

Picking **Every network** or **One address…** while HTTPS is Off switches HTTPS to Templum's certificate and says so
under the field: "Off this computer, the API needs HTTPS." With This computer only and HTTPS Off, the line under HTTPS
says: "Plain HTTP on this computer: give HTTPS with your tunnel or proxy (cloudflared does)."

**Tokens**, under the fields:

- One row per token: its name; under it, dim, its scope, its workspaces ("all workspaces" or their names), "expires
  in 29 days" / "never expires" / "expired 2 days ago" (yellow), and "last used 3 min ago from 203.0.113.7" / "never
  used". **Revoke** on the right, at once (no question: a token is cheap to make again).
- **New token…** opens a dialog: **Name** (one line, at most 60 characters, required); **Can** (Read: see everything,
  terminals' text included; Control: also act, answer requests, open agents, hand tasks, run automations — the
  default; Full: also type into terminals and press keys — with the line "Anyone with a Full token can run commands on
  this computer."); **Expires** (In 1 day, 7 days, **30 days** (default), 90 days, 1 year, On a date… (`YYYY-MM-DD`,
  the end of that day, local time), Never); **Workspaces** (All, or chosen ones with toggles). **Make token**.
- The dialog then shows the token once, whole, in the terminal's font, with **Copy** and the line "Copy it now:
  Templum keeps only a fingerprint of it and can't show it again." **Done** closes it; the text is gone from memory.
- **Revoke all** under the list (asks: "Revoke all 3 tokens? Every client is cut at once.").

**Connected now**: one row per client: the token's name, its address, since when, "listening to events" when it holds
a stream; **Disconnect** (closes its connection; the token keeps working). "No client is connected." when empty.

### The title bar

While the API is on, a pill before the usage meter: **API** with the number of clients ("API · 2"); tooltip "The API
listens on http://127.0.0.1:7411 · 2 clients connected". A click opens Settings ▸ API. When it can't listen, the pill
is yellow, "API off", tooltip the reason. Off, no pill (*the designer's call*: the person always sees a door that is
open).

### The rest of the app

Nothing else changes: an action that came through the API looks as if the person had made it by hand (a card moved, a
block opened, a question answered: the inbox's row goes), with these words where an author shows:

- A task handed through the API is typed as `[from API · phone, task t7]` (the token's name), so the agent and the
  person can tell.
- A notification sent through the API is titled "API · phone".
- A comment written through the API is the person's (`by: "you"`): the token is theirs (*the designer's call*).

## The API's shape

*The orchestrator's call:* HTTP only. JSON REST under `/v1`; real time by Server-Sent Events; input by POST.

### Why SSE

An event stream is one long HTTP response, which cloudflared, Cloudflare, nginx and Caddy pass through as is, and which
browsers resume by themselves (`EventSource`, `Last-Event-ID`). Everything here sends from Templum to the client;
everything the client sends is a POST. WebSocket would add an upgrade every proxy must allow and a second protocol,
for nothing the plan needs (*the designer's call*: batch 2's terminal input is a POST per keystroke burst, under
interactive typing rates; if a measured round trip through a tunnel proves too slow for typing, a later card may add
one WebSocket for input only).

### Authentication

- **`Authorization: Bearer <token>` is the only way in** (*the orchestrator's call*). No cookie, no basic auth, no
  token in a URL (a URL lands in proxy logs, browser history and referrers).
- **Tickets:** `POST /v1/tickets` (authenticated) answers `{"ticket":"tkt_…","expires_in":30}`. A ticket opens one
  `GET /v1/events?ticket=…` within 30 seconds, once, with its token's scope and workspaces; it is the only key a URL
  may carry, and only on the event stream (and batch 2's terminal streams). Revoking the token voids its tickets.
- **A token** is `tpl_` and 43 base64url characters: 256 random bits from the system's generator (ring's
  `SystemRandom`). The prefix lets secret scanners (GitHub's) find a leaked one (*the designer's call*).
- **Stored as** its SHA-256, in hex; a presented token is hashed and compared with every stored hash in constant time.
  (A slow hash is for guessable passwords; a 256-bit random token can't be guessed, *the designer's call*.)
- **Failed authentication** (no token, a wrong one, an expired or revoked one, a used or stale ticket): `401` with
  `WWW-Authenticate: Bearer` and `{"error":{"code":"auth","message":"…"}}`, the same body whatever the reason.

### Scopes

*The orchestrator's call* for the three; *the designer's call* for how a tool gets one:

| Scope | May |
|---|---|
| **read** | Every GET, the event stream, and the tools that only read (`overview`, `list_agents`, `read_terminal`, `list_cards`, `read_note`, `read_checklist`, `wait_for`) |
| **control** | Also every other tool whose danger in the control server is **–**, **Ask own** or **Ask own when losing** ([templum-agent.md](templum-agent.md#tools)): opening agents, handing and interrupting tasks, blocks, wires, cards, notes, checklists, showing; answering questions, plans and requests to connect; running, switching and saving automations that run no command |
| **full** | Also the tools whose danger is **Ask** or **Ask each** (`send_to_terminal`, `send_keys`, and those to come: `run`, `set_setting`, deletions that lose work), saving an automation that runs a command, deleting an automation, answering a request to delete or merge a worktree, and answering the Templum Agent's approvals |

- A tool's scope comes from the registry: a control tool is **read** when the registry marks it as reading
  (`templum_mcp::control::reads`), **full** when `control::rule` gives Ask or Ask each, **control** otherwise. A new
  tool is control until marked otherwise, and a dangerous one is already marked for the Templum Agent's gate.
- **The scope replaces the gate.** A call through the API never shows the Templum Agent's approval: the token is the
  person's own hand, and its scope says how far it reaches (*the designer's call*). So answering the Templum Agent's
  approvals takes **full**: a token that may run the call itself may approve it. (templum-agent.md's "approvals are
  answered only by clicks and keys in Templum's popup, never by a tool, a chat message or a CLI" gains: "or by the
  API's full tokens, the person's own hand".)
- Too low a scope: `403` `{"error":{"code":"scope","message":"send_to_terminal needs a Full token."}}`.

### Workspaces a token may see

A token made for chosen workspaces (*the orchestrator's call*; the rules are *the designer's call*):

- It sees those workspaces, their blocks and wires, the requests of their blocks, the boards a kanban block of theirs
  shows (with those boards' cards), and the automations whose Where is one of them. It sees no usage (an account's,
  not a workspace's), no automation for all workspaces and none of the Templum Agent's approvals.
- It acts only there: every block, workspace, board, card, request and automation a call names must be one it sees;
  `create_workspace` is refused. A target it doesn't see is answered exactly as one that doesn't exist ("There's no
  block k9x1d: GET /v1/blocks lists them."), so a limited token can't learn what lies outside.
- A workspace it names that is deleted is gone for it; a block moved out of its workspaces leaves its view (a
  `delete` event), one moved in joins it.
- The check runs on the UI thread, on the live workspaces, at the call. Which arguments name targets is one list in
  the registry (`block`, `a`, `b`, `near`, `connect`, `for`, `workspace`, `board`, `card`, `request`, `automation`);
  a test fails when a tool has an argument the list doesn't classify.

### Resources

Every GET reads a fresh state from the UI thread (one round trip), filtered for the token
(*the designer's call*: no cache to go stale).

| Method and path | Answers | Scope |
|---|---|---|
| `GET /v1/state` | The whole state: `{app, workspaces, blocks, wires, requests, boards, cards, automations, usage}` | read |
| `GET /v1/workspaces`, `/v1/workspaces/{workspace}` | Workspaces in the sidebar's order | read |
| `GET /v1/blocks`, `/v1/blocks/{block}` | Blocks, back to front; `?workspace=w3` filters | read |
| `GET /v1/agents` | The blocks running an agent, with its state | read |
| `GET /v1/terminals/{block}/text` | `read_terminal`; `?lines=200&last_command=true` | read |
| `GET /v1/notes/{block}/text`, `/v1/checklists/{block}` | `read_note`, `read_checklist` | read |
| `GET /v1/requests`, `/v1/requests/{request}` | What waits for the person, oldest first | read |
| `GET /v1/boards`, `/v1/boards/{board}`, `/v1/cards/{card}` | Boards with their columns' card ids; a card whole | read |
| `GET /v1/automations`, `/v1/automations/{automation}` | The person's automations, with their next and last runs | read |
| `GET /v1/usage` | The agents' plan limits | read |
| `GET /v1/tools` | The registry: every tool's name, description, input schema and scope | read |
| `GET /v1/openapi.json` | The OpenAPI 3.1 document, generated | read |
| `GET /v1/events` | The event stream ([Events](#events)) | read |

The state, in short (*the designer's call*; the OpenAPI document has every field):

```json
{ "app": { "version": "0.1.5", "api": 1, "shown": "w3" },
  "workspaces": [{ "id": "w3", "name": "API", "folder": "/home/me/api", "color": "blue", "shown": true,
                   "stopped": false }],
  "blocks": [{ "id": "b7k2m", "workspace": "w3", "kind": "terminal", "title": "Claude Code · api",
               "folder": "/home/me/api", "rect": { "x": 0, "y": 0, "w": 720, "h": 480 }, "z": 3,
               "opened_by": null, "board": null,
               "agent": { "agent": "claude", "state": "needs_you", "asks": "Allow Bash?", "subagents": 0,
                          "since": 1761560000, "model": "opus", "effort": "high",
                          "usage": { "context_percent": 41.0, "tokens": 120000, "cost_usd": 1.92 } } }],
  "wires": [{ "a": "b7k2m", "b": "k9x1d", "label": null }],
  "requests": [{ "id": "r12", "kind": "question", "block": "b7k2m", "workspace": "w3", "since": 1761560100,
                 "summary": "Which database?", "why": { "card": { "id": 152, "title": "Attachments" }, "task": null,
                 "agent": "Choosing the store decides the migration." },
                 "questions": [{ "question": "Which database?", "header": "DB", "multi": false,
                                 "options": [{ "label": "Postgres", "description": null, "preview": null }] }],
                 "plan": null, "buttons": null, "stopped": false, "scope": "control" }],
  "boards": [{ "id": 3, "name": "Sprint", "columns": [{ "id": 7, "title": "Doing", "cards": [40, 41] }] }],
  "cards": [{ "id": 40, "board": 3, "column": 7, "title": "Login", "description": "…", "priority": "high",
              "assignee": { "block": "b7k2m" }, "checklist": [{ "id": 41, "text": "Build", "done": false }],
              "comments": [{ "number": 51, "by": "b7k2m", "text": "…", "at": 1761550000, "edited": false }],
              "attachments": [{ "name": "shot.png", "size": 48128 }] }],
  "automations": [{ "id": 1, "name": "Morning review", "on": true, "workspace": "w3", "when": { "…": "…" },
                    "do": { "…": "…" }, "runs": 3, "last": { "at": 1761555600, "went": "ran", "about": "…" },
                    "next": 1761642000, "sentence": "Every weekday at 09:00, type into abc12" }],
  "usage": { "limits": [{ "agent": "claude", "at": 1761559000,
                          "windows": [{ "minutes": 300, "used_percent": 62.0, "resets_at": 1761570000,
                                        "scope": null }] }] } }
```

- Ids are the app's own: block ids (`b7k2m`), workspace ids (`w3`), board, card, column, item and comment numbers,
  automation numbers. A request's id is `r<n>` for the inbox's (a question, a plan, a yes/no request) and `a<n>` for the
  Templum Agent's approvals.
- An agent's `state` is the Agents view's ([agents-view.md](agents-view.md#an-agents-state)): `needs_you`,
  `starting`, `waiting_subagents`, `working`, `done`, `idle`; `since` is when it last changed kind, in Unix seconds.
  Times are Unix seconds everywhere.
- A request's `kind` is `question`, `plan`, `connect`, `remove_worktree`, `merge_worktree` or `approval`; `buttons`
  holds a yes/no request's two words (`["Allow","Deny"]`); `scope` is what answering it takes; an approval carries
  `tool`, `said`, `details` and `facts` (Templum's words) and `reason` (the model's).
- A card's attachments give names and sizes, never paths (*the designer's call*).

### Tools

*The orchestrator's call:* the API is built on the registry the control server already has.

- **`POST /v1/tools/{name}`** with the tool's arguments as the JSON body runs any tool of the registry: every control
  tool ([templum-agent.md](templum-agent.md#tools)) and the API's own below. The answer is
  `{"text":"Created card 40.","workspace":"w3","block":"b7k2m"}` (the tool's text, and what it touched when Templum
  knows it). A refusal is `422` with the tool's sentence. A malformed argument is `400` with the parser's sentence.
- **The API's own tools** (in `templum_api`, served only by the API: automations are the person's, not an agent's,
  automations.md's non-goal stands):

  | Tool | Arguments | Does | Scope |
  |---|---|---|---|
  | `answer_request` | `request`, then one of `answers` (a list per question of the labels picked, and at most one other text), `allow` (true or false), `dismiss` (true); `note` | Answers a request as the inbox's buttons do (an approval's `allow` is Allow once; `note` is a denial's words); `Answered.` | control; full for a worktree's and an approval |
  | `save_automation` | `automation` (as `automations.json` keeps one: `name`, `on`, `workspace`, `when`, `do`, `max_runs`, `cooldown`), `id` (to replace one) | Saves it as the editor's Save does, with the editor's checks and words; its id | control; full when its Do runs a command |
  | `set_automation` | `automation`, `on` | Its switch | control |
  | `run_automation` | `automation` | Run now | control |
  | `delete_automation` | `automation` | Deletes it | full |

- **`move_block`** joins the control server now, as templum-agent.md's batch 3 lists it, with `block`, `x`, `y`, `w`,
  `h` (any of them; the rest keep): a step of that workspace's Ctrl+Z, `–` for the Templum Agent's gate. Its
  `workspace` argument (a move to another workspace) stays for the control server's batch 3 (*the designer's call*).
- **REST aliases** name the common calls (*the designer's call*: each one is a row of the registry's route table that
  names its tool; path parameters and the body become the tool's arguments, query values typed by its schema):

  | Method and path | Tool |
  |---|---|
  | `POST /v1/workspaces` | `create_workspace` |
  | `POST /v1/blocks` | `add_block` |
  | `PATCH /v1/blocks/{block}` | `move_block` |
  | `DELETE /v1/blocks/{block}` | `close_block` |
  | `POST /v1/show` | `show` |
  | `POST /v1/wires` / `DELETE /v1/wires/{a}/{b}` | `connect` / `disconnect` |
  | `POST /v1/agents` | `open_agent` |
  | `POST /v1/agents/{block}/task` | `delegate` |
  | `POST /v1/agents/{block}/interrupt`, `/stop` | `interrupt`, `stop_agent` |
  | `POST /v1/terminals/{block}/text` / `/keys` | `send_to_terminal` / `send_keys` |
  | `POST /v1/requests/{request}/answer` | `answer_request` |
  | `POST /v1/boards/{board}/cards` | `create_card` |
  | `PATCH /v1/cards/{card}`, `POST /v1/cards/{card}/move`, `POST /v1/cards/{card}/comments` | `edit_card`, `move_card`, `comment_card` |
  | `POST /v1/automations`, `PUT /v1/automations/{automation}` | `save_automation` |
  | `PATCH /v1/automations/{automation}`, `POST /v1/automations/{automation}/run`, `DELETE /v1/automations/{automation}` | `set_automation`, `run_automation`, `delete_automation` |

- **Who acts.** A call through the API acts as the person: no approval, comments by `you`. Its tasks (`delegate`,
  `open_agent`'s prompt) are handed by the reserved asker `api` (a terminal id is `<pid>-<n>`, never `api`, as
  `templum-agent` is never one), typed as `[from API · <token name>, task t7]`; their end, a watch's match
  (`wait_for` with `wait: false`) and every other notice Templum would type into an asker's terminal become `notice`
  events instead ([Events](#events)).
- **Tools that wait** (`delegate` with `wait`, `wait_for`) hold the HTTP request up to their timeout (3600
  s at most). A client that goes away drops what it waited for, as an agent's closed connection does. Cloudflare cuts a
  response that sends nothing for 100 seconds: through it, hand the task without `wait` and listen for its `notice`
  (docs/api.md says so).

### Events

`GET /v1/events` (*the orchestrator's call*: SSE, resumable ids, heartbeats; the shape is *the designer's call*):

- `Content-Type: text/event-stream`, `Cache-Control: no-store`, `X-Accel-Buffering: no` (nginx would buffer it).
- **On connect:** `event: snapshot`, the whole state as the token sees it. Then one event per change:

  ```
  id: 8f3a-1042
  event: block
  data: {"op":"put","id":"b7k2m","value":{…the block, whole…}}

  id: 8f3a-1043
  event: request
  data: {"op":"delete","id":"r12"}

  id: 8f3a-1044
  event: notice
  data: {"text":"[task t7 ended …] …","block":"b7k2m","workspace":"w3"}
  ```

- **Event names** are the state's collections, singular: `workspace`, `block`, `wire` (id `a-b`, the two block ids
  sorted), `request`, `board`, `card`, `automation`, `usage` (id `usage`); `put` carries the whole object, `delete` its
  id. A block that moves to another workspace is one `put` with its new `workspace`. Agents' states ride on their
  blocks. A client mirrors the state by applying puts and deletes to the snapshot.
- **`notice`** is the one happening that isn't state: a task the API handed ended or its agent needs its person, a
  watch the API set matched. It goes to every stream whose token sees its block.
- **Ids** are `<run>-<n>`: `run` is random at each start of the server, `n` counts up. **Resuming** with
  `Last-Event-ID` (EventSource sends it by itself) gets a fresh `snapshot`, then the notices after that id when it is of
  this run and the server still keeps them (the last 256, at most an hour old); from another run, the snapshot alone.
  State is never replayed: the snapshot is the truth.
- **How changes are found:** while at least one stream is open, the UI thread builds the state at most every 250 ms
  and hands it to the server, which compares it, per stream, with what that stream last saw (filtered for its token)
  and sends the differences. Nothing is built while no stream is open (*the designer's call*: one comparison of plain
  data instead of a hook in every feature, so a feature in the state is live without more code).
- **Heartbeat:** a comment line (`: ping`) every 15 seconds, under every proxy's idle timeout (Cloudflare's is 100 s).
- **Backpressure:** each stream has a queue of 256 events. A client that falls behind loses its queue and gets a new
  `snapshot` when it catches up; the server never holds more than that per client.
- **Cut:** revoking a token, its expiry (checked at each heartbeat), Disconnect, turning the API off and Templum
  quitting end its streams.
- **Batch 2's terminal streams** follow the same rules on their own path: [Batches](#batches).

### Errors and limits

| Status | `code` | When |
|---|---|---|
| 400 | `invalid` | Malformed JSON, an unknown argument value, a tool's argument error |
| 401 | `auth` | No token, a wrong, expired or revoked one, a used or stale ticket |
| 403 | `scope` | The token's scope is too low |
| 404 | `not_found` | No such route, tool or resource (a target the token doesn't see is this too) |
| 413 | `too_large` | A body over 1 MiB |
| 415 | `json` | A POST, PUT or PATCH whose `Content-Type` isn't `application/json` |
| 422 | `refused` | The tool refused (its sentence says what to do next) |
| 429 | `slow_down` | Too many failed authentications from the address, or too many calls from the token; `Retry-After` |
| 503 | `busy` | Too many connections, or Templum is quitting |

Every error body is `{"error":{"code":"…","message":"…"}}`. *The designer's call* for the limits: a body of 1 MiB
(`send_to_terminal` and a plan take 256 KiB of text); request headers within 10 s and 16 KiB; 64 connections at once;
8 event streams per token; 20 calls a second per token, bursts of 40 (each call is work on the UI thread).

### Versioning

`/v1` grows only by additions: new tools, resources, fields, event names and error codes. Clients ignore what they
don't know. A change that would break a client is `/v2`, served beside `/v1` for at least one release
(*the designer's call*). `app.api` in the state says the newest version served, `app.version` Templum's.

## Security

*The orchestrator's call* for every requirement below; *the designer's call* for the details named so.

### Off and local by default

- The API is off until Settings ▸ API turns it on, and listens on `127.0.0.1:7411` when it is. No data folder
  (`--bench`, a second Templum without one): no API.
- **Plain HTTP only on loopback.** The server refuses to start in plain HTTP on any other address (a check in the API
  crate, tested), whatever the file says.

### Encryption

- **TLS is built in** with rustls (in the tree) and ring's crypto: TLS 1.2 and 1.3, ALPN `http/1.1`.
- **Templum's certificate:** made at the first need with rcgen, an ECDSA P-256 key, valid five years, for `localhost`,
  `127.0.0.1` and `::1`, kept in `<data>/api/cert.pem` and `key.pem`. Its fingerprint is the SHA-256 of its DER,
  shown as 32 hex pairs, as Node's `fingerprint256` gives it: a client pins it (docs/api.md shows Node and curl; Templum
  to Templum will). **New certificate** replaces both files. *The designer's call:* names don't matter for pinning, and
  a browser warns about any self-signed certificate.
- **The person's certificate:** a PEM certificate chain and a PEM private key, read at each start of the server; a
  file that doesn't load keeps the API off, with the reason in Settings and the title bar.
- No `Strict-Transport-Security`: a browser would pin the host for a year, and Templum's certificate (or a tunnel's)
  changes. (Revised after the security review.)

### Every answer

`X-Content-Type-Options: nosniff`, `Cache-Control: no-store`, `Referrer-Policy: no-referrer`,
`X-Frame-Options: DENY`, `Content-Security-Policy: default-src 'none'; frame-ancestors 'none'`,
`Cross-Origin-Resource-Policy: same-origin`.

### Browsers

- **CORS is off** unless the person lists origins. A listed origin (compared whole: scheme, host, port; no `*`, *the
  designer's call*: a wildcard would let any page holding a token call in) gets `Access-Control-Allow-Origin` with its
  own value, `Vary: Origin`, and preflights answered with the methods, `Authorization`, `Content-Type` and
  `Last-Event-ID`, for 10 minutes. No credentials mode: there are no cookies.
- A POST, PUT or PATCH must be `application/json`: a form or a `no-cors` request from a page can't be one.

### Brute force and floods

- **Failed authentications** (credentials shown that weren't good; none shown is not one) are counted per client
  address, an IPv6 /64 as one: after 10 within 10 minutes, the address's failures are answered `429` for 2 seconds, then
  twice as long at each further failure, up to 15 minutes (`Retry-After`). A good token always passes, so no address
  can lock the person out, and a failure's connection is closed. The table is capped at 4096 addresses, swept once a
  second; past the cap the rest share one entry. Connections: 64, 16 per token; off loopback, 8 that have shown no
  token per address (an IPv6 /64; the newest stranger evicts the oldest, also for an address the limiter is
  refusing: its users open connections in parallel); nobody is refused at accept, because the token is looked
  at only once it speaks (a shared public address, a proxy that isn't loopback, the shared overflow entry); a full
  table lets in a new one by evicting the oldest stranger of the address holding the most strangers.
- **The client address** is the connection's peer, unless the peer is loopback (a tunnel, a proxy on this computer):
  then the last entry of `X-Forwarded-For` (the one the proxy added; `CF-Connecting-IP` is not believed, a client can
  send it), else the peer (*the designer's call*: without this, a tunnel makes every client one
  address and an attacker would lock the person out).
- **Floods:** the limits of [Errors and limits](#errors-and-limits).

### Visibility

- The title bar's pill; Settings' clients with Disconnect; each token's last use (time and address).
- **The audit log**, `<data>/api/audit.log`, written by the server: one JSON line per call: time, address, token
  name, what (a tool's name, a resource's path), its status; and, if calls were given up because the disk could not
  keep up, one `lost` line with the count (`more`). Never a token, a ticket, a body or typed text. At 1 MiB it becomes
  `audit.log.1` (replacing the older one) and a new file starts (*the designer's call*: 2 MiB at most).
- **The noise log**, `<data>/api/noise.log`: what anyone can make without a token (connections, refused or failed
  authentications, failed TLS handshakes), one line per address and minute with `more` counting the rest, in a file pair
  of its own (1 MiB, then `noise.log.1`), so a flood of strangers from any number of addresses can never rotate the
  calls out of `audit.log`.

### Persistence

- `<data>/api/api.json` holds the settings and the tokens' hashes, written through a temporary file and a rename, in a
  folder made `0700` with files `0600` on Unix (the certificate's key too). A file Templum can't read is set aside as
  `api.json.bak`, the API stays off, and a notification says so.
- **Why not `config.toml`** (*the designer's call*): it is often kept in a dotfiles repository and synced between
  computers; a listening port, its tokens and its certificate belong to one computer.
- A copy of the data folder carries no token that works: only hashes.

### Local processes

- The bus refuses a hello whose client is `api` (the API's calls are made inside Templum, never on the socket), so an
  agent in a terminal can't borrow the API's reach (*the designer's call*, tested in templum-mcp).
- Tokens are never in a terminal's environment. An agent with a shell reaches `127.0.0.1:7411` like any program, and
  meets `401` without a token.

### Threat model

| Threat | What stops it, or limits it |
|---|---|
| **A leaked token** | Its scope (read can't act), its workspaces, its expiry (30 days by default). Revoke cuts it and its streams at once; Revoke all. Its last use and the audit log show misuse. Never in a URL, so not in logs or history; `tpl_` lets scanners find it in a public repository. |
| **A stolen data folder** (a backup, a synced copy) | No working token: SHA-256 hashes of 256 random bits. The certificate's key is there: an attacker who also sits on the network could pose as this Templum to clients that pinned it: **New certificate** (clients re-pin) and Revoke all. Files are `0600`, the folder `0700`, against other users of the computer. |
| **An exposed LAN port** | Off by default, loopback by default. Off loopback, TLS only. Bearer only, on every route but CORS preflights. Rate limits. The title bar shows it is open. |
| **A malicious web page** in the person's browser | No ambient credential (no cookie), so it can't call in the person's name; JSON only, so no form posts; CORS off unless listed; DNS rebinding gets nothing without a token; tickets single-use, 30 s. Security headers keep an answer from being framed or read as HTML. |
| **Brute force** | 2²⁵⁶ tokens: that is what stops guessing. The per-address growing refusal answers failures sooner and keeps the log quiet; it does not slow guessing (authentication runs first). Tunnel-aware addresses; the audit log. |
| **A client that floods or reads slowly** | 64 connections, header and body limits, 20 calls a second per token, 256 events queued per stream, then a snapshot. The UI thread builds the state 4 times a second at most, only while a stream is open. |
| **Someone on the network between** | TLS off loopback; a tunnel's hop to Templum is loopback; HSTS over TLS; pinning. |
| **An agent on this computer** | The bus refuses `api` hellos; tokens never reach terminals. A Full token pasted into an agent gives it the person's hand: docs/api.md says to keep tokens out of agents. |

What a token is worth: **control** lets its holder make agents act, and an agent in bypass mode runs what it is told.
docs/api.md says so, and recommends **read** for dashboards, Cloudflare Access (or a VPN) in front of a tunnel as a
second lock, and short expiries.

## Constraints

- **Crates** (*the orchestrator's call*): the API is a new UI-free crate, `templum-api` (the server, tokens, TLS, the
  registry and its OpenAPI, the state's types and their comparison, the events, the audit log), tested headlessly with
  real HTTP. `crates/templum` holds only the glue: the state built from the workspaces, the calls served on the UI
  thread, Settings ▸ API and the title bar's pill. `templum-mcp` gains `move_block`, `control::reads`, the bus's refusal
  of `api` hellos and its connection numbers for the API, and the check of an answer to a question.
- **The same path as the control server:** the API's threads hand each call to the UI thread as a bus `Message`
  (`client: "api"`, the token's id as its terminal) through the same channel the bus's threads write to, and wait for
  its line; the UI thread never touches a socket and never waits on the network.
- **Dependencies** (*the orchestrator's call*: in the tree first): hyper 1 (its `server` and `http1` features, new on a
  crate in the tree), hyper-util (`tokio`: its IO adapter and timer), http-body-util, bytes, tokio (`rt`, `net`,
  `time`, `sync`, `io-util`, all already on), tokio-rustls and rustls (ring), rustls-pki-types (its PEM reader), ring
  (SHA-256, random), base64, form_urlencoded, futures, serde, serde_json; ureq in the tests. **New:** `rcgen` with
  `ring` and `pem` (MIT or Apache-2.0), which brings `yasna` (MIT or Apache-2.0) and `pem` (MIT): a certificate is
  ASN.1 that no crate in the tree writes, and writing DER by hand is where certificates go wrong. `cargo deny check`
  passes with them.
- **The server** runs on its own thread with a single-threaded tokio runtime, and its blocking waits for the UI's
  answers on tokio's blocking pool (64 threads at most).
- **Performance:** with the API off, nothing runs; on with no stream, nothing runs but the listener; with streams, one
  state built 4 times a second at most, its comparison on the server's thread. `--bench` and `--bench-mixed` keep
  `p95_ms` ≤ 16.7 (they have no data folder, so no API: the pull request says so and pastes their lines).
- **Tests** (CLAUDE.md): unit tests in `templum-api` and `templum-mcp`; integration tests that start the server on an
  ephemeral loopback port and call it with ureq in plain HTTP and over TLS; headless workspace tests for the glue. None
  depends on HOME, config or installed agent CLIs; none sleeps to synchronize (they wait on conditions, with a
  deadline).
- **Every feature in the API** (*the orchestrator's call*, CLAUDE.md's new rule): from this card on, a feature ships
  its API surface — a tool in the registry, or its data in the state — and its line in docs/api.md, in the same change.
  A feature that is a control tool is in the API by itself; one that is only UI adds its data to the state's types.
- Specs to update as batch 1 lands: templum-agent.md (`move_block` built; the gate's sentence on the API's full
  tokens), connections.md (the bus refuses `api`), settings.md (Settings ▸ API), ask-inbox.md (answered from the API),
  automations.md (the API's tools; the non-goal for agents stays), ARCHITECTURE.md (the crate, `workspace/api.rs`),
  docs/guide.md (the API in a few lines), this file's Status and As built.

## Batches

*The orchestrator's split, kept* (*the designer's call*: each batch is usable on its own; batch 1 alone makes the
phone case work).

- **Batch 1 (the plan):** the crate; tokens, tickets, scopes and workspace limits; settings and their file; TLS with
  Templum's certificate or the person's; the server with its limits, headers, CORS, rate limits and audit log; the
  registry with `POST /v1/tools/{name}`, the aliases, `move_block`, the API's own tools, `GET /v1/tools` and the
  OpenAPI document; the resources; the event stream with snapshots, puts, deletes, notices, resume, heartbeats and
  backpressure; Settings ▸ API and the title bar's pill; docs/api.md.
- **Batch 2: live terminals and a finer canvas.**
  - `GET /v1/terminals/{block}/stream` (SSE, read scope): `event: frame` with the terminal's size, cursor, title and
    rows, each row its text and its style runs (foreground, background as palette indexes or RGB, bold, italic,
    underline, inverse); first the whole screen, then only the rows that changed, at most 20 frames a second. The
    frames are read on the server's thread with the terminal's lock taken by `try_lock` (a held lock skips a frame,
    never waits), so neither the PTY's thread nor painting is slowed; a terminal's `Arc` reaches the server through
    the glue. A client behind gets the newest frame, whole, never a backlog. The scrollback is `read_terminal`'s.
  - `POST /v1/terminals/{block}/input` (full scope): `{"text":…}` as a paste, or `{"keys":[…]}` as `send_keys`'
    names; no resize (a viewer scales the frame, as zoom does).
  - The canvas while it moves: the state's tick goes to 100 ms while a block is dragged or resized, so a mirror follows
    a drag; the shown workspace's camera as an optional `view` event.

## Templum to Templum

*Part 2, a later card* (*the orchestrator's call*). Designed here only as far as part 1 must prepare for it.

- **The idea:** on the work computer, a **remote workspace** in the sidebar shows a workspace of the home computer as
  if it were local: its blocks where they are, its terminals live, its agents' dots, its board; moving a block, typing,
  answering, handing a task all reach home. A workspace's Edit… dialog gets **Share** on the home side: it makes a
  token limited to that workspace (control or full) and shows the address, the token and the fingerprint to paste on
  the other side.
- **What batch 1 gives it:** workspace-limited tokens; a state with every field a block needs to be drawn (kind, rect,
  z, title, agent, wires, board) and stable ids; puts and deletes to follow it; `move_block`, `add_block`,
  `close_block`, the card and note tools to act; Templum's certificate and its fingerprint to pin.
- **What batch 2 gives it:** terminal frames with styles, drawn by a viewer terminal (`Terminal::viewer`, which shows
  what Templum feeds it), and input by POST.
- **Left to its card:** the connection dialog and where the remote address, token and pin are kept (the system's
  keyring or the data folder, hashed tokens are no use there); the pinned TLS client; who wins when both sides move a
  block (home is the truth: a mirror sends `move_block` and draws what the next event says); a remote that goes
  offline (its tile dims, its blocks freeze, it reconnects with `Last-Event-ID`); notes and editors' content; latency
  shown.

## Left to the builder

The pill's and the rows' exact looks (theme tokens only), the dialog's layout, the wording of tooltips not quoted here,
the constants' and functions' names, how the route table is written, the OpenAPI document's descriptions beyond the
tools' own, the audit line's field names, and how the server's tasks are arranged on its runtime.

## As built

### Batch 1

What the builder chose where the design left it open, and what differs:

- **Code:** `crates/templum-api/` (`store.rs` settings, tokens and their file; `tls.rs`; `model.rs` the state and its
  comparison; `registry.rs` and `openapi.rs`; `server.rs`; `events.rs`; `audit.rs`), the glue in
  `crates/templum/src/workspace/{api.rs,api_state.rs}`, `settings/api.rs` and `title_bar.rs`'s pill. The control
  server's `control_call` takes an `Actor` (the Templum Agent, or a token).
- **Failed authentications:** the address's failures are refused from its **tenth** (the plan's unit test said the
  eleventh; the spec's sentence and the integration test say the tenth). A failure made after a refusal ends, while the
  address is still in trouble (within 10 minutes of it), doubles the wait; ten minutes of calm start it over.
- **A POST, PUT or PATCH with a body must say `application/json`;** with no body, no type is needed (the review's
  finding: a body-less `curl -X POST` was `415`).
- **Body limits:** an oversize body is read on to 1 MiB more and dropped so the client can read the `413`; past that
  the connection closes. A body must arrive within 30 seconds.
- **Connections past 64** are answered `503 busy` in plain HTTP (64 more at once, then dropped); over TLS they are
  closed. A handshake takes at most 10 seconds.
- **Workspace limits** also cover what the spec lists only by argument name: `save_automation`'s `id`, its
  `automation.workspace` (required for a limited token, and one it sees) and the blocks its `do` types into.
  `for` (a card's assignee) takes `anyone`, an agent's id or a block the token sees. `create_workspace` is `403 scope`.
  The server filters every state it gets for the token as well, whatever the UI thread sent.
- **The streams' waker** is a stored `Waker`, not a `Notify`; a stream ends and leaves the hub at once on revoke, and
  its `Connection: close` ends the connection with it.
- **A closed `delegate` with `wait`** frees the task it handed (`handed` is none); other waits just go, as an agent's
  closed connection drops them.
- **Origins** change on a running server (`Server::set_origins`); any other setting restarts it. `api.json` is written
  at once on the UI thread; a token's last use only once a minute and at quit.
- **Cargo.lock** gains `templum-api`, `rcgen`, `yasna`, `pem`, and, as lock-only entries that are not compiled,
  rcgen's optional `x509-parser` and its dependencies (`asn1-rs`, `der-parser`, `oid-registry`, `nom`…).
- **After the security review (sixteen findings):**
  - A token limited to some workspaces: `overview` is refused; `list_cards` and `create_card` must name a board it sees
    (and the error never lists the others); a workspace or board in an argument is resolved as the tool resolves it
    (an id first, then a name in either case) and the id is what is checked and passed on; `add_block`/`open_agent`
    folders must lie, once canonicalized, inside a folder of its workspaces (so must an automation's); a deleted
    workspace's id leaves every token's list.
  - `move_block` clamps the size to the biggest grid a terminal may have and bounds x and y to ±10⁷.
  - Setting, running or replacing a command automation takes a Full token.
  - Authentication comes first and the limiter slows only failures, keyed on the last `X-Forwarded-For` of a loopback
    peer (`CF-Connecting-IP` is not believed), IPv6 by /64, 4096 addresses at most, swept once a second; a failure's
    connection closes; 16 connections per token; a full table evicts the oldest stranger.
  - The audit log is written by a thread of its own and aggregates connection and authentication noise per address
    and minute (`more` counts the rest); HSTS is gone.

- **After the second security review (nine findings):**
  - An automation a limited token saves: `do.agent.folder` and `do.command.folder` (and `folder` of `add_block` and
    `open_agent`) are written back as the canonical path that was checked, so what is stored and run is what was
    checked (`"."` is the workspace's absolute folder); with no folder, a workspace without one is refused. The store
    keeps, per automation, the real folders of the token's workspaces (`confined` in `api.json`); each run checks the
    folder it opens in against them and is Skipped "Target missing: …" when a link was swapped in. The person's own
    save in the editor, or deleting it, lifts that. A limited token's `worktree` on `open_agent` needs the
    repository's main tree to lie inside one of its workspaces' folders.
  - Connections: off loopback, 8 that showed no token per address (an IPv6 /64); a full table evicts the oldest
    stranger of the address holding the most. (A peer the limiter refuses was refused at accept here; the third review
    undid that.)
  - The audit log counts noise in `Audit::write`, before the queue (a table capped at 4096, keyed like the limiter),
    queues only a key's first line and writes its count when the window ends (looked at once a second at most); a
    call line waited up to 200 ms for room in a full queue instead of being dropped. (Never waits now: see the third
    review.)
  - An open stream ends when its token's workspace list is no longer the one it opened with (the sweep compares
    them); the client opens it again.
  - `flush_api` (quit) drains the uses the server still holds before writing.
  - Settings ▸ API: a field is taken on Enter or Blur; a value its setter refuses (a port outside 1024–65535) is
    replaced in the field by the setting, with the reason under it.
  - Streams: an expired token's end at the next publish; the state is compared only when it changed or a stream opened,
    once per kind of caller. A client coming or going is told to the UI (`templum/clients`), which keeps the pill's
    count. A body-less POST needs no Content-Type. The address, port and certificate fields take Enter or leaving the
    field; an edit that changes nothing restarts nothing.
- **After the third security review (nine findings):**
  - Nobody is refused at accept: the limiter only slows failed authentications, so a good token from a shared address
    (CGNAT, an office, a VPN exit, a proxy that isn't loopback) gets in. (The third review also held a refused address
    to one stranger; the fourth undid that, see below.)
  - `api.json` keeps `saved_by` (automation → the id of the token that saved it) beside `confined`, for every rule
    saved through the API. Revoke, Revoke all and expiry (swept at each snapshot, every 5 s) turn those rules off and
    notify "Automation <name>, saved by token '<token name>', was turned off."; the person's own save or delete clears
    `saved_by` as it clears `confined`.
  - The audit writer keeps its files open (reopened after a rotation), collects the noise lines due under the lock and
    writes them after releasing it, and a call line never waits: noise may fill only half of the queue (a counter the
    writer decrements), a call that finds the queue full is counted as `lost` and the writer logs it as one line.
    `Drop` closes the channel (the writer drains it and ends) and waits about a second for a done signal instead of
    joining without a limit. Noise has its own file pair, `noise.log` (1 MiB + one old file); `audit.log` holds only
    lines with a token (and the `lost` line).
  - The canonical path a rule keeps and a folder is opened in is written back in the simplified form (`dunce`):
    `C:\x`, not `\\?\C:\x`, which cmd.exe refuses as a folder. Paths are compared canonical; only the form written
    back changes.
  - A limited token's `worktree` is checked in the background task, before `add_worktree`, against the main tree *git*
    reports (`Repo::main_tree`, which obeys `commondir` and `core.worktree`); the workspace no longer reads `.git`
    files itself, so a planted one can't move the worktree outside the token's folders.
  - On Windows a folder on `UNC`, `VerbatimUNC` or `DeviceNS` is refused before the filesystem is asked about it
    (the SMB session would offer the person's NTLM hash), unless one of the token's workspace folders is on that
    share (`refuses_share`, over `std::path::Prefix`, tested on every platform).
  - `start_api` drains the old server's uses first.
  - **Folder limits are not a sandbox.** They guard what a token can *name* (where it opens a terminal, an editor, an
    agent or a worktree). A process the token starts inside a folder has the person's reach.
- **After the fourth security review (ten findings):**
  - Connections: the cap of 8 strangers per address is the same for an address the limiter refuses: its users open
    connections in parallel (a dashboard, a proxy on a docker bridge, CGNAT), and a cap of one had them abort each
    other. "A good token always gets in" holds against guessing; whoever shares the address and floods connections
    can still starve the connections of that address that haven't authenticated yet.
  - The credentials are checked and the connection marked as authenticated under one lock, so an accept in between
    can't evict it as a stranger. (No test: it needs an accept interleaved with a request; the lock is the proof.)
  - On Windows `share_of` fails closed: only `Disk` and `VerbatimDisk` are the computer's own. `std` matches `\\?\UNC\`
    case-sensitively, so `\\?\unc\x\y` and `\\?\GLOBALROOT\Device\Mup\…` come as a plain `Verbatim` and reach an
    SMB share all the same; a `Verbatim` prefix is refused unless a folder of the token is on the same one. Tested on
    every platform over `Prefix` values.
  - The audit log cuts `what` to 200 characters (a `/v1/tools/{name}` name was the caller's, up to 16 KiB). Noise is
    counted by (address, what, token): a `slow_down` carries a token, and another token's was added to the first's line.
  - A rule saved by a token that is revoked or expired is Skipped "Target missing: its token was revoked" (in
    `automation_target`) until the sweep turns it off; one the token had already turned off is told of ("…stays off.")
    instead of dropped silently.
  - `confined` and `saved_by` entries for automations that don't exist are dropped at load (the ids restart when
    `automations.json` was set aside), unless the automations couldn't be read.
  - A limited token's worktree outside its folders is `403` `scope`, as any other folder.
- **Merged with Automations batch 2 (card 150):** an event rule's `cooldown` is in the state (null for a time rule),
  and the API saves event rules (`when.agent`, `when.command`) as the file keeps them.
- **Unfinished, in [minors.md](../minors.md#api):** an approval's `since` is when it first showed, and a changed
  certificate on disk is read at the next start.
