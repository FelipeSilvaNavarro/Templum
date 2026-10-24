# The Templum API

Your Templum, from anywhere: a phone, a dashboard, another computer. It is an HTTP API inside Templum, off until you
turn it on, that answers only to **tokens** you make. Every tool the Templum Agent has is `POST /v1/tools/{name}`; the
things a dashboard lists are JSON resources; everything that changes comes on one live event stream. Design:
[specs/api.md](specs/api.md).

> **Read this before you make a token.** A **Control** token can make your agents act, and an agent in bypass mode runs
> what it is told. A **Full** token can type into terminals: anyone who holds one can run commands on this computer.
> Keep tokens out of agents' reach (never in a terminal's environment, a prompt or a file an agent reads), give a
> dashboard a **Read** token, and prefer short expiries.

*How these examples were checked:* every route, header, status and event below is exercised by the integration tests in
`crates/templum-api/tests/` (plain and over TLS) and the workspace tests in
`crates/templum/src/workspace/tests/api.rs`, which run the server with a real HTTP client. The `curl` and JavaScript
lines were not run against a running window of the dev build (the author's environment had no display), so try one
first.

## Quickstart

1. **Turn it on:** Settings ▸ API ▸ *Turn on the API*. It listens on `http://127.0.0.1:7411` (this computer only).
2. **Make a token:** *New token…*, a name, what it **Can** do, when it **Expires**, which **Workspaces**. The token is
   shown once; Templum keeps only a fingerprint of it.
3. **Call it:**

   ```sh
   export TOKEN=tpl_…
   curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7411/v1/state
   ```

### From outside this computer

Templum opens a port; the way in is yours. The server refuses plain HTTP on any address but this computer's: off it,
HTTPS is on (Templum's own certificate, or yours).

- **cloudflared** (HTTPS is the tunnel's; Templum stays on `127.0.0.1`):

  ```sh
  cloudflared tunnel --url http://127.0.0.1:7411          # a quick try
  ```

  For good, a named tunnel's config:

  ```yaml
  ingress:
    - hostname: templum.example.com
      service: http://127.0.0.1:7411
    - service: http_status:404
  ```

  Put **Cloudflare Access** (or a VPN) in front of it as a second lock. Behind a tunnel every connection comes from this
  computer; Templum reads the client's address from the last `X-Forwarded-For` entry (the one the proxy added; no other
  header is believed), so failed logins from the internet are counted per client, not against you. A good token passes
  whatever the address's record. Behind a plain TCP forwarder (`ssh -R`, `socat`, `ngrok tcp`) nothing appends
  `X-Forwarded-For`, so the address Templum shows is whatever the client sends; only an HTTP proxy that appends the
  header tells clients apart.
- **Tailscale, a VPN, a reverse proxy:** the same: point it at the port.
- **The LAN:** *Who can reach it* ▸ every network or one address; HTTPS switches to Templum's certificate. Its SHA-256
  fingerprint is in Settings (*Copy*); *Save certificate…* writes a `.pem` to install on a phone. *New certificate*
  makes another (clients that pinned the old one stop connecting).

## Examples

```sh
B=http://127.0.0.1:7411; H="Authorization: Bearer $TOKEN"; J='Content-Type: application/json'

curl -H "$H" $B/v1/agents                                              # the agents and their states
curl -H "$H" -H "$J" -d '{"agent":"claude","workspace":"w3","prompt":"Take card 40"}' $B/v1/agents
curl -H "$H" -H "$J" -d '{"task":"Review the open PRs"}' $B/v1/agents/b7k2m/task
curl -H "$H" -H "$J" -d '{"answers":[["Postgres"]]}' $B/v1/requests/r12/answer
curl -H "$H" -H "$J" -d '{"allow":true}' $B/v1/requests/a3/answer         # an approval: needs a Full token
curl -H "$H" -H "$J" -d '{"column":"Done"}' $B/v1/cards/40/move
curl -H "$H" -H "$J" -X PATCH -d '{"x":120,"y":80,"w":640,"h":400}' $B/v1/blocks/b7k2m
curl -H "$H" -H "$J" -X POST -d '{}' $B/v1/automations/1/run
curl -H "$H" -H "$J" -d '{"board":"Sprint","title":"Login"}' $B/v1/tools/create_card   # any tool, by name
```

A POST, PUT or PATCH with a body must say `Content-Type: application/json`; one with no body needs no type.

### Events from JavaScript

A browser's `EventSource` can't set headers, so it opens with a **ticket**: a key for one stream, 30 seconds, once.

```js
const base = 'https://templum.example.com';
const headers = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' };
const state = {};                                   // collection → id → object

async function connect() {
  const { ticket } = await (await fetch(`${base}/v1/tickets`, { method: 'POST', headers, body: '{}' })).json();
  const source = new EventSource(`${base}/v1/events?ticket=${ticket}`);
  source.addEventListener('snapshot', e => {        // the whole state: replace what you had
    const s = JSON.parse(e.data);
    for (const [name, list] of Object.entries(s))
      if (Array.isArray(list)) state[name] = Object.fromEntries(list.map(o => [o.id, o]));
  });
  for (const kind of ['workspace', 'block', 'wire', 'request', 'board', 'card', 'automation', 'usage']) {
    source.addEventListener(kind, e => {
      const { op, id, value } = JSON.parse(e.data);
      const table = (state[kind + 's'] ??= {});
      op === 'put' ? (table[id] = value) : delete table[id];
    });
  }
  source.addEventListener('notice', e => console.log('notice', JSON.parse(e.data)));
  // EventSource's own retry would reuse the used ticket: close and open again with a new one.
  source.onerror = () => { source.close(); setTimeout(connect, 2000); };
}
connect();
```

Node can send the header itself, and pin Templum's certificate with the fingerprint Settings shows:

```js
import https from 'node:https';
const pin = 'AB:CD:…';                              // Settings ▸ API ▸ Certificate
https.get('https://192.168.1.5:7411/v1/events', {
  headers: { Authorization: `Bearer ${token}` },
  checkServerIdentity: (host, cert) =>
    cert.fingerprint256 === pin ? undefined : new Error('Templum\'s certificate changed'),
}, res => res.on('data', chunk => process.stdout.write(chunk)));
```

## Reference

### Authentication

`Authorization: Bearer tpl_…` is the only way in: no cookie, no basic auth, no token in a URL. A token is `tpl_` and 43
characters (256 random bits); Templum keeps its SHA-256 and compares in constant time. A wrong, expired or revoked
token, no token, a used or stale ticket: `401` with `WWW-Authenticate: Bearer` and the same body whatever the reason.
Failed authentications (a token or ticket was shown and wasn't good; asking with none is not one) are counted per client
address (an IPv6 /64 is one address): at the tenth within 10 minutes the address's failures are answered `429` for 2
seconds, then twice as long at each further failure, up to 15 minutes (`Retry-After`). A good token always passes,
and a failure's connection is closed. The table holds 4096 addresses; past that the rest share one entry. The refusal
answers failures sooner and keeps the log quiet; it does not slow guessing, because a token is checked before the
address's record is looked at. What stops guessing is the token itself: 256 random bits.

Connections: 64 in all, 16 per token; off this computer, 8 that have shown no token per address (the newest makes room
by closing the oldest of that address; the cap is the same for an address the limiter is refusing, since its users open
connections in parallel). Nobody is turned away at the door, whatever the limiter says of their address: a good token
always passes against guessing, also from an address shared with someone who guesses. That does not hold against a
flood of connections: whoever shares the address and opens connections without a token can still starve the
connections of that address that haven't authenticated yet. When the table is full the oldest connection that has
shown no token, of the address holding the most, makes room, and a 65th with all 64 authenticated gets `503`. A
token's 17th connection gets `429`.

**What a token limited to some workspaces can't do:** `overview` (read `GET /v1/state`), `list_cards` or
`create_card` without naming a `board` it sees, open anything in a folder outside its workspaces' folders (`..` and
symlinks followed; a terminal, an editor or an agent in a workspace with no folder; a `worktree` of a repository whose
main tree, as git reports it, lies outside them; on Windows, a folder on a network share or any `\\?\…` path that none of them is on; only a drive letter is the computer's own), or make a command, switch or run one: those are a Full token's whoever it is limited to.
These refusals are all `403` `scope`, a `worktree` outside included. A folder it names is stored as the real path that was checked (`"."` becomes the workspace's folder), and an automation
it saved is checked again at each run: if its folder no longer resolves inside the workspaces' folders (a link was
swapped in), the run is Skipped "Target missing". The folder limits guard what a token can name; they are not a
sandbox: a process the token starts inside a folder (a terminal, an agent) has your reach. Open event streams of a token end when its list of workspaces
changes; the client opens them again.

**Automations follow their token:** an automation saved through the API (`save_automation`, by any token) is turned
off when that token is revoked (Revoke or Revoke all) or expires, and a notification says "Automation <name>, saved by
token '<token name>', was turned off." A rule the token had turned off itself is told of too ("…stays off.") and stays off.
Until the sweep (every few seconds) turns it off, a run of a revoked or expired token's rule is Skipped "Target missing:
its token was revoked". Saving it yourself in the editor (or deleting it) makes it yours: it stays.

A **ticket** (`POST /v1/tickets` → `{"ticket":"tkt_…","expires_in":30}`) opens one `GET /v1/events?ticket=…` within 30
seconds, once, with its token's scope and workspaces. It is the only key a URL may carry, and only on that route.
Revoking a token voids its tickets.

### Scopes

| Scope | May |
|---|---|
| **read** | Every GET, the event stream, and the tools that only read (`overview`, `list_agents`, `read_terminal`, `list_cards`, `read_note`, `read_checklist`, `wait_for`) |
| **control** | Also every other tool that doesn't type for you: opening agents, handing and interrupting tasks, blocks, wires, cards, notes, checklists, showing; answering questions, plans and requests to connect; running, switching and saving automations that run no command |
| **full** | Also `send_to_terminal`, `send_keys`, saving an automation that runs a command, deleting an automation, answering a request to delete or merge a worktree, and answering the Templum Agent's approvals |

Too low a scope: `403` `{"error":{"code":"scope","message":"send_to_terminal needs a Full token."}}`. A call through the
API never shows the Templum Agent's approval: the token is your own hand, and its scope says how far it reaches.

**Workspace-limited tokens** see only those workspaces, their blocks and wires, the requests of their blocks, the boards
a kanban block of theirs shows (with those boards' cards) and the automations for those workspaces; no usage, no
approvals. A block, workspace, board, card, request or automation outside them is answered as one that doesn't exist.
They can't make a workspace, and save automations only for their own workspaces.

### Resources (read scope)

| Path | Answers |
|---|---|
| `GET /v1/state` | The whole state: `app`, `workspaces`, `blocks`, `wires`, `requests`, `boards`, `cards`, `automations`, `usage` |
| `GET /v1/workspaces`, `/{workspace}` | Workspaces in the sidebar's order |
| `GET /v1/blocks`, `/{block}` | Blocks, back to front; `?workspace=w3` filters |
| `GET /v1/agents` | The blocks running an agent, with its state |
| `GET /v1/terminals/{block}/text` | `read_terminal`: `?lines=200&last_command=true` |
| `GET /v1/notes/{block}/text`, `/v1/checklists/{block}` | `read_note`, `read_checklist` |
| `GET /v1/requests`, `/{request}` | What waits for you: questions, plans, requests to connect or delete a worktree, approvals |
| `GET /v1/boards`, `/{board}`, `/v1/cards/{card}` | Boards with their columns' card ids; a card whole |
| `GET /v1/automations`, `/{automation}` | Your automations, with their next and last runs |
| `GET /v1/usage` | The agents' plan limits |
| `GET /v1/tools` | The registry: every tool's name, description, input schema and scope |
| `GET /v1/openapi.json` | The OpenAPI 3.1 document, generated from the registry |
| `GET /v1/events` | The event stream |

Ids are Templum's own: block ids (`b7k2m`), workspace ids (`w3`), board, card, column, item and comment numbers,
automation numbers; a request is `r<n>` (the inbox's) or `a<n>` (the Templum Agent's approvals). Times are Unix seconds.

### Tools and aliases

`POST /v1/tools/{name}` runs any tool with its arguments as the JSON body; the answer is
`{"text":"Created card 40.","workspace":"w3","block":"b7k2m"}` (the tool's words, and what it touched when known). The
tool refusing is `422` `refused` with its sentence; a malformed argument is `400` `invalid`. `GET /v1/tools` lists them,
with the five only the API has:

| Tool | Arguments | Scope |
|---|---|---|
| `answer_request` | `request`, then one of `answers` (a list per question: the labels picked, and at most one other text; a plan is `[["Approve"]]` or `[["Reject"]]`), `allow` (true/false), `dismiss`; `note` | control; full for a worktree's request and an approval |
| `save_automation` | `automation` (as `automations.json` keeps one: `name`, `workspace`, `when`, `do`, `max_runs`), `id` to replace one | control; full when its `do` runs a command |
| `set_automation` | `automation` (its number), `on` | control |
| `run_automation` | `automation` | control |
| `delete_automation` | `automation` | full |

REST aliases name the common calls; path parameters and the body become the tool's arguments, and the path wins:

| Method and path | Tool |
|---|---|
| `POST /v1/workspaces`, `/v1/blocks`, `/v1/show` | `create_workspace`, `add_block`, `show` |
| `PATCH /v1/blocks/{block}`, `DELETE /v1/blocks/{block}` | `move_block` (`x`, `y`, `w`, `h`), `close_block` |
| `POST /v1/wires`, `DELETE /v1/wires/{a}/{b}` | `connect`, `disconnect` |
| `POST /v1/agents`, `/v1/agents/{block}/task`, `/interrupt`, `/stop` | `open_agent`, `delegate`, `interrupt`, `stop_agent` |
| `POST /v1/terminals/{block}/text`, `/keys` | `send_to_terminal`, `send_keys` |
| `POST /v1/requests/{request}/answer` | `answer_request` |
| `POST /v1/boards/{board}/cards`, `PATCH /v1/cards/{card}`, `POST /v1/cards/{card}/move`, `/comments` | `create_card`, `edit_card`, `move_card`, `comment_card` |
| `POST /v1/automations`, `PUT /v1/automations/{automation}` | `save_automation` (the body is the automation) |
| `PATCH /v1/automations/{automation}`, `POST …/run`, `DELETE …` | `set_automation`, `run_automation`, `delete_automation` |

**Who acts.** A call acts as you: no approval, comments by `you`. Tasks it hands (`delegate`, `open_agent`'s prompt) are
typed as `[from API · <token name>, task t7]`, and their end, a `wait_for` with `wait: false` and every other notice
Templum would type into an asker's terminal come as `notice` events. **Tools that wait** (`delegate` with `wait`,
`wait_for`) hold the request up to their timeout; a client that goes away drops what it waited for. **Cloudflare cuts a
response that sends nothing for 100 seconds:** through it, hand the task without `wait` and listen for its `notice`.

### Events

`GET /v1/events` is Server-Sent Events (`text/event-stream`). It opens with `event: snapshot` (the state, as the token
sees it), then one event per change, named for the collection, singular (`workspace`, `block`, `wire` with the id `a-b`
(its two block ids sorted), `request`, `board`, `card`, `automation`, `usage` with the id `usage`):

```
id: 8f3a1c20-1042
event: block
data: {"op":"put","id":"b7k2m","value":{…the block, whole…}}

id: 8f3a1c20-1043
event: request
data: {"op":"delete","id":"r12"}
```

A client mirrors the state by applying puts and deletes to the snapshot. A block that moves to another workspace is one
`put` with its new `workspace`; to a token that doesn't see the new one it is a `delete`. `event: notice` is the one
happening that isn't state: `{"text":…,"block":…,"workspace":…}`. Templum compares the state about four times a second,
only while a stream is open.

- **Ids** are `<run>-<n>`; `run` is random at each start of the server. `Last-Event-ID` (EventSource sends it by itself)
  of this run gets a fresh `snapshot`, then the notices after it that Templum still keeps (the last 256, at most an hour
  old); of another run, the snapshot alone. State is never replayed: the snapshot is the truth.
- **Heartbeat:** a `: ping` comment every 15 seconds.
- **Backpressure:** each stream queues 256 events; a client that falls behind loses its queue and gets a new `snapshot`
  when it catches up.
- **Cut:** revoking the token, its expiry (checked at each heartbeat), *Disconnect*, turning the API off and
  quitting end its streams. Eight streams per token.

### Errors and limits

Every error is `{"error":{"code":"…","message":"…"}}`.

| Status | `code` | When |
|---|---|---|
| 400 | `invalid` | Malformed JSON, an argument that doesn't parse, a tool's argument error |
| 401 | `auth` | No token, a wrong, expired or revoked one, a used or stale ticket |
| 403 | `scope` | The token's scope is too low |
| 404 | `not_found` | No such route, tool or resource (a target the token doesn't see is this too) |
| 413 | `too_large` | A body over 1 MiB |
| 415 | `json` | A POST, PUT or PATCH whose `Content-Type` isn't `application/json` |
| 422 | `refused` | The tool refused; its sentence says what to do next |
| 429 | `slow_down` | Too many failed authentications from the address, calls from the token, or streams; `Retry-After` |
| 503 | `busy` | Too many connections (64), or Templum is quitting |

Request headers within 10 s and 16 KiB; 20 calls a second per token, bursts of 40. Every answer carries
`X-Content-Type-Options: nosniff`, `Cache-Control: no-store`, `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`,
a `Content-Security-Policy` of `default-src 'none'; frame-ancestors 'none'` and `Cross-Origin-Resource-Policy:
same-origin`. There is no `Strict-Transport-Security`: a browser would pin the host for a year, and Templum's
certificate (or a tunnel's) changes. CORS is off unless you list origins in Settings (whole
origins, no `*`): a listed origin gets `Access-Control-Allow-Origin` with its own value, and preflights are answered
without a token.

### The audit log and what is kept

`<data>/api/audit.log` has one JSON line per call: time (`at`, Unix seconds), address, token name, what (a tool's name,
a resource's path; cut at 200 characters) and its status. If calls were ever given up because the disk could not keep up, one line `lost`
says how many (`more`). What anyone can make without a token (connections, refused or failed authentications and
handshakes) is in `<data>/api/noise.log`, one line per address and minute with `more` counting the rest, so a flood of
strangers can't fill a log or push the calls out of `audit.log`. Never a token, a ticket, a body or typed text. At 1 MiB
each becomes `audit.log.1` and `noise.log.1`. `<data>/api/api.json` holds the settings and the tokens' hashes (`0600`, in a `0700` folder on
Unix); a copy of the data folder carries no token that works. Each token's last use (time and address) is in Settings.

### Versioning

`/v1` grows only by additions: new tools, resources, fields, event names and error codes. Clients ignore what they don't
know. A change that would break a client is `/v2`, served beside `/v1` for at least one release. `app.api` in the state
says the newest version served. The whole schema is `GET /v1/openapi.json`.
