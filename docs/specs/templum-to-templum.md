# Templum to Templum: a workspace of another computer, as if it were here

- **Date:** 2026-10-25
- **Status:** Designed (kanban card 171, part 2). Batch 1a (home's side) implemented; the mirror's side (batch 1b to
  1d) is next.
- **Builds on:** [api.md](api.md) (the API: tokens, scopes, workspace-limited tokens, TLS and the fingerprint, the
  state and its event stream, terminal frames, `terminal_input`), [workspaces.md](workspaces.md) and
  [cockpit.md](cockpit.md#6-the-workspaces-screen) (the sidebar, the Edit… dialog), [ask-inbox.md](ask-inbox.md)
  (requests), [connections.md](connections.md) (block ids, the bus), [shell-integration.md](shell-integration.md)
- **For people:** [docs/api.md](../api.md) gains a section as batch 1 lands.

The person said, for this card: "Pode decidir, fomente bem a ideia para ser uma funcionalidade bem boa … não precisa
ficar perguntando para mim, sobre segurança tome medidas cabíveis para ser seguro e criptografado as conexões." (Decide,
grow the idea into a really good feature, don't ask; on security, take the proper measures: safe and encrypted.) The
orchestrator's calls are marked *the orchestrator's call*; everything else the designer settled is marked *the
designer's call*.

## Intent

The person has a personal computer and a work computer. At the office they want to keep working on what runs at home,
"good enough that the user on the other connection feels as if the project were local". Templum only opens a port; the
person exposes it (a domain, a VPN, a cloudflared tunnel, their own case). There is no Templum server.

Part 1 built everything a second Templum needs on home's side: workspace-limited tokens, a state complete enough to
draw, its live deltas, terminal frames and typed input. Part 2 is the two ends that use it:

- **Share**, on home: a workspace's Edit… dialog makes a token limited to that workspace and shows one **connection
  code** to paste on the other computer.
- **Connect**, on the other computer: the code becomes a **remote workspace** in the sidebar. It mirrors home's
  workspace: its blocks where they are, its terminals live, its agents' states, its requests. Typing, moving a block and
  answering a request go to home through the API; the mirror draws what home says next. Nothing of home's runs on the
  mirror (*the orchestrator's call*: **home is the truth**).

## Goals

1. **The person's case works in batch 1:** from the work computer, through a cloudflared tunnel, open home's workspace,
   see it live, type into its terminals with a real keyboard, move and resize its blocks, answer its agents' requests.
2. **Two clicks on each side.** Home: Edit… ▸ Share… ▸ Make code ▸ Copy. Mirror: Connect to a Templum… ▸ paste ▸
   Connect.
3. **Encrypted, and checked.** Always TLS off loopback. A direct connection pins Templum's certificate; a tunnel's is
   checked against the web's authorities, and a network that reads HTTPS (a company proxy) is told plainly.
4. **The key is kept safely.** The token lives in the system's keyring, never in a log, `config.toml`, the data folder
   (unless the person chooses a file) or a crash.
5. **Honest when it isn't live.** A remote that drops dims and freezes, says it is reconnecting and reconnects by
   itself; one that is cut says why and what to do; the round trip shows.
6. **Feels local where it can, says so where it can't.** What a mirror doesn't do yet is shown, never silently broken.
7. **Every feature in the API** (*the orchestrator's call*, CLAUDE.md): shares are in home's API state and tools; a
   remote workspace is in the mirror's state.

## Non-goals

- A server, relay or rendezvous Templum runs; NAT traversal; discovery on the LAN (*the orchestrator's call*: Templum
  only opens ports).
- Running anything of home's on the mirror, or syncing files, Git or editors' files between the two.
- A mirror acting as a relay: the mirror's agents, block server, control server and API never reach home through it
  (*the designer's call*: a chain of tokens is a hole).
- Two-way sync, merging or offline edits: the mirror has no copy of its own to reconcile.
- Sharing several workspaces with one code; sharing between two people (the same person's two computers is the case).
- A mobile client (part 1's API is the phone's way in).
- End-to-end encryption *through* a tunnel (see [Later batches](#later-batches)).

## Anti-goals (a failure even if it works)

- A token readable in the mirror's data folder, a log, `config.toml`, a crash, a URL or a process's environment.
- A connection that silently trusts a certificate it wasn't told to trust, or follows a redirect with the token.
- The mirror's UI thread waiting on the network or the keyring; painting slowed by a remote.
- Memory that grows with what home sends (a flood of frames, a huge state, a slow UI).
- Keys typed while offline delivered later, blind.
- A block of the remote workspace written into the mirror's notes, boards, checklists or canvas files.
- Something on the mirror that looks like it works and does nothing at home.
- A revoked share that keeps showing home live.

## Words

- **Home:** the Templum whose workspace is shared. **The mirror:** the other Templum, showing it.
- **Share:** a token limited to one workspace, Control or Full, with an expiry, made from that workspace's Edit…
  dialog. It is an API token like any other, with its share's details beside it.
- **Connection code:** one line holding the address, the token and (direct only) the certificate's fingerprint. Shown
  once, like a token.
- **Remote workspace:** the mirror's sidebar tile and canvas for a share.
- **Way in:** **pinned** (direct: LAN, VPN, Tailscale; the mirror pins Templum's certificate) or **public** (a tunnel
  or a proxy with a public certificate; the mirror checks the web's authorities).

## What a person sees

### On home: sharing a workspace

**The Edit… dialog** gains a **Sharing** part below Folder (*the orchestrator's call*: share lives in the workspace's
Edit… dialog). Unlike the rest of the dialog, what it does applies at once, not on Save (*the designer's call*: a
revoke must not wait for Save).

- With no share: "Let another Templum mirror this workspace: watch it live, move its blocks, answer its agents, type
  into its terminals." and **Share…**.
- One row per share: its name; under it, dim, "Full" or "Control", "expires in 29 days" / "never expires" / "expired
  2 days ago" (yellow), and "mirrored now from 203.0.113.7" (with the accent dot) / "last used 3 h ago from
  203.0.113.7" / "never used"; **Revoke** on the right, at once, no question (a share is cheap to make again). Revoking
  ends its streams, so the mirror goes dark at once.

**Share…** opens a dialog (*the designer's call* for its fields):

| Field | Control |
|---|---|
| Name | One line, required, at most 60 characters, default "Another computer" |
| Can | **Full** (default): "Watch, move blocks, answer requests and type into terminals. Anyone with the code can run commands on this computer." **Control:** "Watch, move blocks and answer requests. No typing." |
| Expires | In 1 day, 7 days, **30 days** (default), 90 days, 1 year, Never |
| The other computer reaches this one | **Through a tunnel or proxy** (cloudflared, Caddy, nginx, your own certificate), with **Address**: `https://templum.example.com` (an optional path prefix is kept), remembered for the next share. **Directly** (same network, VPN, Tailscale), with **Address**: host and port, prefilled with this computer's address on its main network and the API's port, `192.168.1.5:7411` (a Tailscale name may replace it) |

- The default way in follows Settings ▸ API: Templum's certificate on a network other than loopback picks Directly;
  anything else picks Through a tunnel.
- **The API must fit the way chosen**, and the dialog says so with one button that fixes it (*the designer's call*: a
  share must work when its code is pasted):
  - API off: "Sharing needs the API. **Turn it on** (this computer only, port 7411)." for a tunnel; for Directly,
    "**Turn it on** for every network, with Templum's certificate".
  - Directly while the API listens only on this computer, or without Templum's certificate: "Directly needs the API on
    every network with Templum's certificate. **Apply**".
  - Through a tunnel with an address that isn't `https://` (or `http://` to `127.0.0.1`, `::1` or `localhost`, for an
    `ssh -L` forward): "The address must start with https://."
- **Make code** makes the token and shows the code once, whole, in the terminal's font, with **Copy**, and: "Paste it in
  the other Templum: sidebar ▸ Connect to a Templum… Templum keeps only a fingerprint of its key: this code can't be
  shown again." **Done** closes it; the text is gone from memory.

**The tile** of a workspace that is mirrored now (a share's event stream is open) gets a small cast mark at its
bottom-right; tooltip "Mirrored now on Another computer (203.0.113.7)". The workspaces screen's card shows the same
(*the designer's call*: the person always sees who is watching).

**Elsewhere on home:**

- Settings ▸ API lists a share's token with the other tokens, its line saying "share of API"; Revoke there is the same.
- **New certificate** in Settings ▸ API also says how many direct shares it breaks: "Clients that pinned the old one
  stop connecting, 2 mirrors connected directly among them: share again for them."
- **Deleting a workspace** leaves its shares' tokens seeing nothing (part 1 already takes the id out of every token's
  list, so a later workspace given the same id is never theirs), and the mirror says "deleted" from `GET /v1/me`
  (*the designer's call*: revoking them would turn every mirror's message into "revoked", since part 1 answers every
  `401` alike). Settings ▸ API lists them as "share of a deleted workspace" until they are revoked there, expire, or
  the mirror's Forget revokes them.
- What a mirror does looks as part 1 says an API call does: typed keys show "answered by API · Another computer", a
  task handed is `[from API · Another computer, task t7]`.

### On the mirror: connecting

The sidebar gets a **Connect to a Templum…** button under **+** (a plug icon, tooltip of the same words; *the
designer's call*: visible, where the person looks for workspaces). The workspaces screen's header gets the same button
beside New workspace, and the keymap a `connect_templum` command with no default key.

**The Connect dialog:**

- **Connection code:** a field, focused. As soon as it holds a code, a line under it says what it is:
  "templum.example.com · checks its public certificate" or "192.168.1.5:7411 · pins Templum's certificate
  AB:CD:…:9F", or what is wrong: "This isn't a connection code: one starts with tplc1_.", "The code is cut short: copy
  it again whole.", "This code is from a newer Templum: update this one.", "This code's address isn't encrypted:
  Templum connects in plain HTTP only to this computer itself."
- **More…** (folded): **Cloudflare Access service token**, Client ID and Client Secret, for a tunnel with Cloudflare
  Access in front (docs/api.md recommends it): every request carries them (*the designer's call*: the second lock the
  docs recommend must not lock the mirror out). **Proxy**, for a network that only reaches out through one
  ([The client](#the-client)).
- Under the field, where the key goes: "The key is kept in this computer's keyring (Keychain / Credential Manager / the
  Secret Service)." With no keyring that answers: "This computer has no keyring Templum can use: the connection lasts
  until Templum quits." and a switch, off: **Keep the key in a file only you can read** (`0600` in the data folder),
  with "A copy of the data folder would carry it." ([Where the key is kept](#where-the-key-is-kept)).
- **Connect** tries it: "Connecting…"; on success the dialog closes and the remote workspace opens; a failure stays in
  the dialog in the words of [When it isn't live](#when-it-isnt-live). The code leaves the field's memory either way, and
  if the clipboard still holds it after a success, Templum clears it (*the designer's call*).

### The remote workspace

- **Its tile:** home's color and icon (or initials), and a small remote mark at its bottom-right. Tooltip:
  "API, on winx-desktop · 84 ms". It joins the sidebar's order (dragged like any tile), Ctrl+Tab, Ctrl+1–9 and the
  workspaces screen (its card: "Mirror of winx-desktop · 3 terminals · 1 Claude Code working").
- **A banner** over the canvas's top edge, window chrome at a fixed scale like the Templum Agent's pill: "Mirror of
  **API** on **winx-desktop** · Full · 84 ms", an ⓘ that lists what isn't mirrored yet, and **Disconnect**. The round
  trip turns yellow past 250 ms, with the tooltip "Each key waits about 300 ms for winx-desktop's answer." Three days
  before the share expires: "· ends in 3 days: share again on winx-desktop".
- **Its canvas:** home's blocks at home's rectangles, back to front, with home's titles, agents' dots and chips (state,
  model, context), and the wires between them. The camera is the mirror's own: the first time, it shows what home's
  canvas shows when home shows that workspace (the state's `view`), else it fits the blocks; then it stays where the
  person leaves it, kept across restarts. Zoom, levels of detail, snapping and focus are local, as everywhere.
- **Terminals** show home's live screen, drawn by a viewer, in the mirror's own fonts and colors (home's palette
  numbers take the mirror's palette, as part 1 says). Home's whole grid fits the block. Selecting and copying text,
  and find, work on the live screen.
  - A Claude Code subagent's transcript says "Read-only: a subagent's transcript" in its footer.
  - With a Control share, the first key typed into a terminal shows "Typing needs a Full share: share again on
    winx-desktop." once per terminal.
  - Scrolling back isn't mirrored in batch 1: the wheel over a remote terminal shows, once, "Only the live screen is
    mirrored for now."
- **Typing** in a focused remote terminal is the mirror's keyboard on home's terminal: every key and combination,
  F-keys, Alt and Ctrl combos, arrows with modifiers, Home, End, PageUp, PageDown, Insert, Delete, dead keys and IME
  text; Paste is a paste. Templum's own shortcuts stay the mirror's, and those that yield to programs yield as they
  would at home (the frame says when a program took over).
- **Moving and resizing** a block works as locally, with snapping and Ctrl+Z; home moves its block and the mirror keeps
  following home.
- **Requests:** the Requests button counts the remote workspace's requests while it is shown; the inbox lists them
  and answers them (a question's options, a plan's Approve or Reject, Allow or Deny). One that arrives plays the sound
  and shows the notification a local one would, titled "API on winx-desktop".
- **Blocks not mirrored yet** (notes, checklists, editors, boards in batch 1): a card in the block's place, with its
  kind's icon, its title and "Not mirrored yet: open it on winx-desktop." Ink and shapes aren't shown yet; the ⓘ says so.
- **Not on a remote workspace yet** (each disabled where it shows, with the tooltip "Not on a mirrored workspace
  yet"): adding blocks (toolbar, menus, drops), closing blocks, wiring, the drawing tools, grid mode, Move to
  workspace, Copy and Paste of blocks, Stop, the Git popup.
- **Right-click the tile:** Open, Edit…, Disconnect (or Connect), Forget…. **Edit…** shows the connection (address,
  way in, the pin, what the share can do, when it expires, the round trip), **Paste a new code…**, and the local look
  (icon, color); the name is home's ("named on winx-desktop"). **Forget…** asks "Forget API on winx-desktop?" with
  **Also revoke the share on winx-desktop** (on while connected): it revokes the share with its own token, deletes the
  key and removes the tile.

### When it isn't live

*The orchestrator's call:* a remote that drops dims its tile and freezes its blocks; it reconnects with backoff and
`Last-Event-ID`. The words and which states retry are *the designer's call*.

While it isn't live, the tile is dimmed, the canvas keeps its last blocks and frames under a dim veil, the agents' dots
go grey, and typing shows "Not connected: what you type isn't sent." (keys are dropped, never queued). Moving and
answering are disabled.

| What happened | The banner says | Retries by itself |
|---|---|---|
| Home doesn't answer: refused, timed out, no route, a name that doesn't resolve, or the tunnel's 502, 503, 504, 530 | "Reconnecting to winx-desktop… try 3, next in 8 s. Live 2 min ago." **Try now** | Yes |
| A stream went silent for 45 s (three missed heartbeats) | The same | Yes |
| 429 or 503 `busy` | The same, after `Retry-After` | Yes |
| 401, and the share's expiry has passed | "This share of API expired. Share again on winx-desktop, then **Paste a new code…**" · **Forget** | No |
| 401 otherwise | "winx-desktop revoked this share. Share again there, then **Paste a new code…**" · **Forget** | No |
| `GET /v1/me` lists no workspace (it was deleted at home) | "API was deleted on winx-desktop." · **Forget** | No |
| The workspace is stopped at home (`stopped`, no blocks) | Not dimmed: a card in the canvas's middle, "API is stopped on winx-desktop." **Start it** (`show`, which starts it and shows it on winx-desktop's screen; the button's tooltip says so) | — |
| `event: replaced`: another computer connected with this share's code | "Another computer connected with this share's code, so this one stopped. Each computer needs its own share." **Connect here again** · **Forget** | No |
| Pinned, and the certificate's fingerprint differs | "winx-desktop's certificate changed since you connected. If you made a New certificate there, share again and paste the new code. If you didn't, someone may be between the two computers: don't connect." **Paste a new code…** · **Forget** | No |
| Public, and the certificate isn't valid | "templum.example.com's certificate isn't valid: it expired / it is for another name / no authority this computer trusts signed it. Fix it where the tunnel or proxy runs, then **Try again**." A self-signed one adds: "If it is Templum's own, share Directly instead: it is pinned." | No |
| Public, valid only through an authority this computer added | [A private authority](#a-private-authority) | No |
| Cloudflare Access answered (a redirect to `*.cloudflareaccess.com`, or its 403) | "Cloudflare Access stands in front of templum.example.com. Add a service token in Edit… ▸ More…, and allow it in the Access policy." | No |
| Something that isn't a Templum answered (HTML, a 404 without Templum's error body) | "templum.example.com didn't answer as a Templum (HTTP 404, text/html). Check that the tunnel points at port 7411." **Try again** | No |
| Home is too old (Templum's 404 for `GET /v1/me`) | "winx-desktop runs a Templum that can't be mirrored: update it." **Try again** | No |
| The proxy refused | "The proxy proxy.corp:8080 refused the connection (407: it asks for a login Templum can't give)." **Try again** | No |
| The keyring refused the key (locked, denied) | "Templum couldn't read this connection's key from the keyring. **Try again**, or **Paste a new code…**" | No |

- **Backoff:** 1 s, doubling to 30 s, each wait ±20%; back to 1 s once a connection has been live for a minute.
  **Try now** retries at once (*the designer's call*).
- **Resuming:** the event stream reopens with `Last-Event-ID`; home sends a fresh snapshot (part 1's rule), which
  replaces the mirror's blocks; terminal streams reopen whole.
- **Starting Templum** connects every remote that has its key, off the UI thread; until then the tile shows
  "Connecting…" and the canvas is empty.
- **Disconnect** closes every connection and keeps the remote, dimmed: "Disconnected. **Connect**".

## The shape

### The connection code

*The designer's call* (the orchestrator left its format open):

- `tplc1_` followed by the base64url (no padding) of a JSON object:
  `{"url":"https://templum.example.com","token":"tpl_…","pin":"ab12…"}`. `pin` (the certificate's SHA-256, 64 hex
  characters, no colons) is there only for a pinned share. `url` is the address with its scheme, port and optional
  path prefix; the mirror calls `<url>/v1/…`.
- `1` is the format's version: a mirror that meets `tplc2_` says the code is from a newer Templum. Unknown JSON fields
  are ignored. The prefix lets a secret scanner be taught to find a leaked code, as `tpl_` does for tokens.
- The way in is read from the code, never guessed: **a pin means pinned; `https://` without one means public;
  `http://` is accepted only for a loopback host**. The mirror never falls back from one to another.
- No names in the code: the mirror reads home's computer name and workspace name from home once connected, so a code
  can't claim to be someone else's.

### Trust: how the mirror checks who answers

*The orchestrator's call:* always TLS unless loopback; two trust modes. How the mirror tells them apart and what each
checks is *the designer's call*.

| Way in | TLS check | Server name (SNI) |
|---|---|---|
| **Pinned** | The leaf certificate's SHA-256 equals the pin, compared in constant time; its handshake signature is verified with ring's algorithms. Names and dates are not looked at: Templum's certificate is made for `localhost` and lasts five years, and the pin is its identity | The host when it is a name; none for an IP |
| **Public** | The web PKI as the system checks it (rustls-platform-verifier): chain, name, dates, by Windows' and macOS's own verifiers (which fetch a root Windows hasn't loaded yet, and obey the computer's policies) and on Linux with the system's roots (rustls-native-certs). Then the [private authority](#a-private-authority) check | The host |
| **Loopback** (`http://127.0.0.1`, `::1`, `localhost`) | None: plain HTTP to this computer, for an `ssh -L` forward or a test | — |

- TLS 1.2 and 1.3, ALPN `http/1.1`, ring's crypto, as the server.
- **No redirects are followed**, and the token, the Access headers and every call go only to the code's host: a
  redirect is "didn't answer as a Templum".
- **A changed pin is never accepted by a click.** The way out is a new code, which carries a new token too (*the
  designer's call*: a "trust the new certificate" button would undo pinning exactly when it matters).

#### A private authority

A work computer may sit behind a company proxy that reads HTTPS: it installs its own authority in the system's roots
and signs a certificate for every site. Through it, the token and every frame of home's terminals pass in the clear
to the company, and a Full token would let it run commands at home. The person's case is exactly a work computer.

So after the system accepts a public certificate, the mirror checks the chain the server sent again with Mozilla's
list (webpki-roots, already built for the updater). If only the system accepts it, the connection is refused with:
"HTTPS to templum.example.com is signed by an authority this computer added, not a public one. A company proxy or
security program may be reading this connection: your key, what home's terminals show and what you type. With a Full
share it could run commands on winx-desktop. Connect only if you added that authority yourself (a home lab, your own
proxy)." **Don't connect** (default) · **Connect anyway**. Connect anyway is kept for that remote only
(`trust_added_authority`) and shown in its Edit…; a new code asks again (*the designer's call*: the system's check
stays the one the orchestrator chose, and the person learns what it means before trusting it). A server that sends
its chain without the intermediate certificate lands here too (the system may fetch the missing one, Mozilla's check
doesn't); cloudflared's edge, Caddy and nginx with a full chain send it whole, and the message's tooltip says "or the
server doesn't send its whole chain".

### The client

*The designer's call* for all of it; the orchestrator asked to prefer what the tree has.

- **One thread for every remote**, with a single-threaded tokio runtime, as the server has: connections, streams,
  calls, timers, backoff and the keyring's calls (on its blocking pool). It talks to the UI thread only through
  channels the UI thread drains and a wake-up; the UI thread never waits on it.
- **HTTP:** hyper 1's client connections (`hyper::client::conn::http1`, its `client` feature, already compiled in the
  tree) over tokio-rustls, one connection per stream, no pool. Calls take two keep-alive connections: one for typing
  (its order matters), one for everything else.
- **Server-Sent Events** are read by a small parser of the client's own (fields `event`, `data`, `id`, comments), with
  a cap of 16 MiB per event: a bigger one drops that stream, which reopens.
- **Connections per remote,** under home's 16 per token: one event stream, at most 12 terminal streams, two for calls,
  one spare for a reconnect's overlap.
- **Proxies:** the Connect dialog's More… has **Proxy** (`http://proxy.corp:8080`, optional), prefilled from
  `HTTPS_PROXY` (or `https_proxy`, `ALL_PROXY`) unless `NO_PROXY` names the host, and kept with the remote. Through it
  the mirror opens an HTTP `CONNECT`, with Basic credentials when the proxy's URL holds them (kept in the keyring with
  the key, never in the index), then its TLS inside, end to end. The system's own proxy settings and PAC files are not
  read, nor NTLM or Kerberos asked (*the designer's call*: a work network often has a proxy, and an app started from the
  Start menu or the Dock doesn't get the variables, so the person can type it; a refusal is told, never guessed around).
- **Every request** carries `Authorization: Bearer`, the Access headers when set, `User-Agent: Templum/<version>
  (mirror)` and `Templum-Mirror: <session>`, a random value new at each connect or reconnect.
- **A reconnect clears the old connections** (*the designer's call*: a laptop that slept or changed networks leaves
  its connections open at home, where nothing notices them dead for many minutes, and they would hold the token's 16).
  When a share token opens an event stream with a session other than the one its connections carry, home first sends
  `event: replaced` on that token's other streams, then closes every connection of the token from another session. A
  mirror that gets `replaced` while its own connections are alive stops and says so (see
  [When it isn't live](#when-it-isnt-live)), so two computers holding the same code don't knock each other off in turn.
  Tokens Settings made keep part 1's rules.
- **Liveness:** a stream that sends nothing for 45 s is dead. When no call ran for 15 s, the mirror calls
  `GET /v1/me`: it keeps the round trip fresh and learns a changed expiry.

### Mirroring the state

- **Opening:** `GET /v1/me` (the token: its scope, its workspace, its expiry; home's computer name), then
  `GET /v1/events?view=true`. The snapshot is the truth; puts and deletes change it; a new snapshot replaces it
  (part 1's rules). The mirror reuses `templum-api`'s state types to read them.
- **A remote workspace is a workspace** on the mirror (*the designer's call*: the sidebar, switching, the camera,
  levels of detail, selection, snapping, moving and undo then work as they do), marked as remote, with these rules:
  - **Never saved** as a canvas: only its index entry ([Persistence on the mirror](#persistence-on-the-mirror)). Its
    blocks come from home each time.
  - **Never in the mirror's stores:** no note, checklist, board or card of home's is written to the mirror's
    `notes.json`, `checklists.json` or `boards.json`.
  - **Home's block ids stay home's:** they live in the remote's own table, not in the ids the mirror's agents name
    blocks by, so no local tool can reach a remote block, and a local block with the same five letters is never
    confused with it.
  - **Never a relay:** the mirror's block server and control server don't see remote blocks (`overview` lists the
    remote workspace as "API (mirror of winx-desktop)" with no blocks; a tool naming it is refused "API is a mirror of
    winx-desktop: act on it there."); the mirror's API shows the remote workspace, never its content
    ([What the mirror's API shows](#what-the-mirrors-api-shows)).
- **Blocks by kind:** a `terminal` is a viewer (as a subagent's block is); `kanban`, `note`, `checklist` and `editor`
  are a "not mirrored yet" card in batch 1 (one `Body` arm for it); a kind the mirror doesn't know is that card too,
  with its kind's name.
- **Agents:** a remote terminal's dot, chips and "needs you" notice come from its block's `agent` in the state, not
  from hooks.
- **The UI thread applies what changed** at most once a frame: the client keeps, per remote, the newest state not yet
  taken (a later one replaces it), so a busy UI never piles up deltas.

### Terminals

- **Which stream:** the terminals on screen at Full detail, nearest the canvas's center first, at most 12, and always
  the focused one; the UI thread tells the client when that set changes, at most every 100 ms. Others keep their last
  frame (a terminal that comes into view gets live within a round trip). A remote workspace not on screen streams no
  terminal; its state stream stays open, for requests and dots (*the designer's call*: home paints only what is on
  screen, the mirror streams only what it paints).
- **A frame becomes bytes for the viewer** on the client's thread, which feeds the viewer's terminal as a PTY reader
  would and wakes the UI (painting never waits on it). A whole frame clears the screen; a row is a cursor move, its runs
  in SGR and an erase to the line's end; then the cursor (position, DECSCUSR shape, shown or hidden) and the title.
  The test batch 2 wrote, which redraws a frame into a viewer and reads back the same screen, becomes this code.
- **What home sends is drawn, never run** (*the designer's call*): every control character (C0, C1, ESC) in a run's
  text or the title is dropped before the bytes are made, the title is cut at 256 characters, colors are parsed strictly
  (0–255 or `#rrggbb`), and a frame larger than 2000 × 1000 cells (home's own largest grid) is refused. A home that
  went wrong can show false things; it can't make the mirror's terminal do anything.
- **Memory:** a viewer keeps no scrollback; per terminal the client holds at most one frame not yet fed (a newer one is
  merged in: its rows replace those rows, a whole frame replaces everything).
- **The size is home's:** the viewer has home's columns and rows, and its cells are scaled so that grid fills the
  block's content (the smaller of the two ratios), at the nearest of the zoom's font steps, so the glyph cache stays
  bounded. Resizing the block at the mirror sends a move; home re-grids; a whole frame brings the new size.
- **Modes:** each frame says whether a program took over ([What home's API gains](#what-homes-api-gains)), so the
  mirror yields the shortcuts home would yield, and a later batch can send the mouse.

### The keyboard

*The orchestrator offered two ways:* extend `terminal_input`'s named keys, or add raw bytes encoded by the mirror.
**The designer's call: named keys, encoded at home.** The bytes a key makes depend on the terminal's modes at the
moment it arrives: application cursor keys, the kitty keyboard's flags (which Claude Code and Codex turn on: that is how
Shift+Enter reaches them), bracketed paste. Only home knows them live; the mirror sees them a round trip late. Home
already encodes keys for its own keyboard (`templum-terminal`'s `encode_key`), so a named key reaches the program as a
key pressed at home does, and raw bytes would be a second encoder that is wrong whenever a mode changed in flight.

- **A key's name:** `[ctrl+][alt+][shift+]<key>`, the modifiers in any order. `<key>` is `enter`, `escape`, `tab`,
  `backspace`, `delete`, `insert`, `up`, `down`, `left`, `right`, `home`, `end`, `page_up`, `page_down`, `space`,
  `f1`–`f12`, or one character as typed, after Shift and the layout (`A`, `@`, `ç`, `á`). `ctrl+shift+p`, `alt+left`,
  `shift+f5`, `ctrl+alt+delete`. Today's names (`shift+tab`, `ctrl+<letter>`, one character) mean what they meant. For
  a letter, home takes its lowercase as the key and the letter as its text, as GPUI reports one.
  (A known limit: a shifted symbol with Ctrl or Alt reaches the kitty protocol with the symbol as its key, not the
  unshifted key under it; the mirror would have to send both, if a program ever needs it.)
- **The mirror names what GPUI reports:** a key with modifiers by its name; a character typed (dead keys and IME
  commits included) as one-character keys; AltGr's characters as characters (the same rule `encode_key` applies);
  Templum's own shortcuts never leave the mirror.
- **`terminal_input`'s `keys`** take these names, 1 to 256 per call, pressed back to back, with 50 ms after a key
  that sends a lone ESC (`escape`, `ctrl+[`, `ctrl+3`, `shift+escape`; not `alt+…`) that another key follows (so a
  program still reads Esc alone where the person paused, now that the network bunches keys). `send_keys` takes the same names and keeps its 16 keys and 50 ms between each.
- **Order and pace:** per terminal, one call in flight; keys typed meanwhile wait and go in the next call, up to 1024
  waiting (past that: "Typing faster than winx-desktop answers: some keys were dropped."). A paste is `text` (bracketed
  when the program asked for it, at home), at most 256 KiB ("Paste at most 256 KiB into a mirrored terminal.").
- **No predictive echo** (*the designer's call*): a character shows when home's frame shows it. The round trip is in
  the banner instead.

### Moving and resizing

*The orchestrator's call:* a move goes to home and the mirror draws what the next event says. Drawing only that would
make a drag trail the pointer by a round trip and a 100 ms tick, so (*the designer's call*):

- While the person drags or resizes, the block follows the pointer locally, and nothing is sent; on release the mirror
  sends one `move_block` per block it moved (a drag sent as it went would be dozens of Ctrl+Z steps at home and, for a
  selection, past the token's 20 calls a second).
- Until home's state shows the rectangle sent, or 2 s after the release, the mirror's rectangle stands over home's;
  then home's wins, whatever it is (the person at home moved it too: the last move home took is the truth).
- Every rectangle the person changes on the mirror is sent: a drag, a resize, Ctrl+Z and Ctrl+Shift+Z, a selection
  moved together. A refusal (`422`) puts the block back where home has it and shows the refusal's sentence.
- A known limit: a selection of five blocks moved on the mirror is five Ctrl+Z steps at home, one per `move_block`. A
  `move_block` for several blocks would make it one; nothing in batch 1 needs it.

### Requests

The state's `requests` of the remote workspace's blocks feed the mirror's inbox for that workspace, drawn as local
requests are; an answer is `answer_request` through the API (control scope; a worktree's request needs a Full share,
and says so where its buttons are). A request answered at home, or by another mirror, leaves on the next delete event.
The Templum Agent's approvals are never seen by a share (part 1's rule for limited tokens).

### What home's API gains

*The orchestrator's call:* what part 2 adds on home is in the API. The shapes are *the designer's call*; each is an
addition to `/v1`.

- **`GET /v1/me`** (read): who the token is.

  ```json
  { "name": "Another computer", "scope": "full", "workspaces": ["w3"], "expires": 1764150000, "share": "s4",
    "computer": "winx-desktop", "version": "0.1.6", "api": 1 }
  ```

  `computer` is the host's name (sysinfo's, already in the tree); `share` is null for a token Settings made.
- **`shares`** in the state, and `share` events:

  ```json
  { "id": "s4", "workspace": "w3", "name": "Another computer", "scope": "full", "created": 1761560000,
    "expires": 1764150000, "way": "public", "address": "https://templum.example.com",
    "last": { "at": 1761561000, "addr": "203.0.113.7" }, "mirrored": true }
  ```

  `mirrored` is true while an event stream of its token is open. A token for all workspaces sees every share; a limited
  token sees only the share it is.
- **Tools** (the API's own, like the automations': *the designer's call*, an agent never makes a way into the
  computer, so the control server doesn't have them):

  | Tool | Arguments | Does | Scope |
  |---|---|---|---|
  | `share_workspace` | `workspace`, `name`, `scope` (`control` or `full`), `expires` (Unix seconds, or null for never), `way` (`public` or `pinned`), `address` | Makes the share as the dialog does, with its checks and words; the answer's `text` is "Shared API as s4.", and its `code` field holds the connection code, once | full, and a token for all workspaces |
  | `revoke_share` | `share` (omitted: the calling token's own) | Revokes it; its streams end | full and a token for all workspaces; any share may revoke itself |

  Aliases: `GET /v1/shares`, `POST /v1/workspaces/{workspace}/shares`, `DELETE /v1/shares/{share}`. Refusals:
  "A pinned share needs HTTPS with Templum's certificate: Settings ▸ API ▸ HTTPS." and "The address must start with
  https://.". Settings ▸ API's settings themselves stay the person's to change in the app (part 1's non-goal).
- **A block's `read_only`:** true for a terminal that takes no input (a subagent's transcript); left out otherwise.
- **A workspace's `icon`:** its Lucide name, or null; the mirror draws home's tile.
- **A frame's `modes`:** `{"full_screen":true,"mouse":true,"kitty_keys":true}`, each present only when true: the
  three a program takes the keyboard by at home (`Modes`, read with the screen under the same `try_lock`).
- **`terminal_input`'s keys** as [The keyboard](#the-keyboard) says.
- **The mirror's session:** `Templum-Mirror` and `event: replaced`, as [The client](#the-client) says, for share tokens.
- **The audit log groups a mirror's chatter** (*the designer's call*): `terminal_input` and `GET /v1/me` calls are one
  line per token, call and minute, with `more` counting the rest, as the noise log does, so a morning of typing can't
  rotate the other calls out of `audit.log`'s 2 MiB. Every other call keeps its own line.
- **The share is kept with its token** in `api.json` (`share`: workspace, way, address), so Settings ▸ API, the Edit…
  dialog and the state read it from one place; revoking the token is revoking the share.

### What the mirror's API shows

A remote workspace is in the mirror's state as a workspace with `remote`:
`{"computer":"winx-desktop","host":"templum.example.com","way":"public","status":"live","latency_ms":84}` (`status`:
`connecting`, `live`, `reconnecting`, `cut`, `refused`, `disconnected`), and its blocks never are (*the designer's
call*: data in the state is this feature's API surface, CLAUDE.md's "or its data in the API's state"; connecting needs
a code, a secret the API doesn't take).

### Persistence on the mirror

`workspaces/index.json`'s entry of a remote workspace (id `r<n>`, the smallest unused) gains `remote`: the `url`, the
`way`, the `pin`, home's workspace id, the keyring entry's name, `trust_added_authority`, the Access Client ID, the
proxy's address without its credentials, whether the key is in a file, and the camera. No `<id>.json` is written for it. None of these is a secret. An older Templum
reading the index shows it as an empty workspace and drops the connection when it next writes the index (a downgrade
loses the connection, never a secret).

## Security

### Where the key is kept

*The orchestrator asked* for the system's keyring or a `0600` file, justified. **The designer's call: the system's
keyring**, with a file only when there is none and the person asks for it.

- **Why the keyring.** Part 1 promises that "a copy of the data folder carries no token that works": home keeps only
  hashes. A key in a mirror's data folder would break that promise on the mirror, which holds a working key to home
  (often Full). A data folder gets copied, backed up and synced; the agents that run on the mirror run as the person and
  read files. The keyring keeps the key out of the folder, encrypted at rest under the person's login (Keychain, the
  Credential Manager's DPAPI, the Secret Service's collection), and an agent would have to ask for it on purpose rather
  than find it in a `grep`. The file would be simpler; it would also be the one place on either computer where a copy
  of the folder is a working key.
- **The crates:** `keyring-core` 1 (its `Entry`, `set_default_store` and the `mock` store the tests use) with
  `apple-native-keyring-store` (macOS, `keychain`), `windows-native-keyring-store` (Windows) and
  `zbus-secret-service-keyring-store` (Linux and the BSDs, `rt-async-io-crypto-rust`: `secret-service` 5 on zbus 5
  with async-io, which the tree already builds for the terminals' scopes, with RustCrypto's aes, cbc, hkdf and sha2 and
  `num`, which the lock already has, though a newer aes or cbc may come as a second version). New: those four and
  `secret-service`, all MIT or Apache-2.0, maintained by the keyring-rs project; the plan checks the lock's diff and
  `cargo deny check` (the `keyring` umbrella crate's defaults would bring stores Templum doesn't use).
- **Entries:** service "Templum" ("Templum (dev)" for `--dev`), account `remote-<8 random characters>`; the share's
  token and, when set, the Access Client Secret and the proxy's credentials, as one JSON value.
- **No keyring** (a Linux session without a Secret Service, a keyring that refuses): the connection lasts until Templum
  quits, unless the person turns on **Keep the key in a file only you can read**: `<data>/remotes/<entry>.key`, `0600`
  in a `0700` folder, written through `private_write` as `api.json` is. Removed by Forget.
- **On macOS** an update changes Templum's ad-hoc signature, so the Keychain may ask once whether Templum may read its
  item; denying it is the keyring's refusal in [When it isn't live](#when-it-isnt-live).
- **Read and written off the UI thread** (an unlocked keyring may prompt); read once per start, then held only by the
  client's thread.
- **Never shown:** the key is a type whose `Debug` and `Display` print `tpl_…`; no error, log line, notification,
  panic message, `config.toml`, index entry or environment variable ever holds it, nor the code. A test formats every
  error and state of a remote and finds no `tpl_` in them.

### What crosses and who sees it

- **Pinned:** only home and the mirror. **Public through cloudflared:** the mirror, Cloudflare's edge (it ends TLS),
  then cloudflared's own encrypted tunnel to home, then loopback. Every hop is encrypted; Cloudflare sees what passes.
  docs/api.md says so, and recommends Cloudflare Access with a service token and short expiries.
- **Home's side** is part 1's: the share is a token, with its limits, audit log lines, last use and rate limits; home's
  per-token limits bound a mirror even if it went wrong.

### Threat model

| Threat | What stops it, or limits it |
|---|---|
| **A leaked code** (a chat, a screenshot, the clipboard's history) | It is a token: one workspace, Control or Full, an expiry (30 days by default), Revoke in Edit… or Settings. Shown once; `tplc1_` for scanners; the mirror clears the clipboard after connecting. Home's tile shows when it is mirrored and from where. |
| **A copied or stolen mirror data folder** | No key in it: the keyring holds it (unless the person chose the file, warned). |
| **An agent on the mirror computer** | The key is never in a file Templum writes (by default), an environment or a tool's answer; the mirror never relays, so its agents and API can't reach home. On Linux and Windows a process of the same user can still ask the keyring: docs/api.md says to keep agents you don't trust away from a mirror with a Full share. |
| **Someone between, on a direct network** | The pin: a different certificate is refused, and only a new code moves past it. |
| **Someone between on a tunnel's path; a company proxy reading HTTPS** | The web PKI checks name and dates; an authority the computer added is refused unless the person accepts it for that remote, told what it means. |
| **The tunnel's provider** | Sees the traffic by the person's choice; Cloudflare Access as a second lock; short expiries. End-to-end through tunnels is a later card. |
| **A home that went wrong, or a code to someone else's Templum** | The mirror draws and never runs: control characters dropped, sizes capped, events capped at 16 MiB, no redirects. The banner names the computer and the address the mirror is connected to. What the person types goes to that Templum, as with any remote shell. |
| **A revoked or expired share** | Its streams end at once (part 1); every call is `401`; the mirror stops, says why and never retries a `401`. |
| **Home's certificate key stolen** (from home's data folder) | Pinned mirrors could be fooled until home makes a New certificate, which breaks the pins; the person shares again. |
| **A mirror that floods** (a bug) | Home's limits per token (16 connections, 20 calls a second) and the mirror's own budget under them. |
| **Two hands on one block** | Home is the truth: the last move home took wins; the mirror's own rectangle lasts 2 s after a release at most. |

## Constraints

- **Crates** (CLAUDE.md): the client is UI-free and lives in `templum-api`, which becomes "Templum's HTTP API, both
  ends" (*the designer's call*: it reuses the state's and frames' types, `tls.rs`'s fingerprint and `store.rs`'s
  private writes; a crate of its own would duplicate them). Its new parts: the connection code, the client (TLS with
  both ways in, proxies, calls, the SSE reader, backoff), a remote's state (snapshot and deltas applied, the newest kept),
  frames to viewer bytes, and the key's keeping. `templum-terminal` gains the key names in `named_key` and the modes
  in its screen. `crates/templum` holds the glue: the remote workspaces, their blocks, the streams' set, typing, moves,
  requests, the dialogs, the sidebar's button and marks, and the share tools on home's side.
- **Dependencies** (*the orchestrator's call*: the tree first): hyper's `client` feature (already built, for gpui's
  HTTP client), tokio-rustls, webpki-roots 1 (built, for ureq), ring, base64, serde_json. **New:**
  `rustls-platform-verifier` 0.7 (MIT or Apache-2.0, the rustls project's), because the system's roots read as a list
  (rustls-native-certs) miss the roots Windows loads only when its own verifier asks for them, and a work computer is
  often Windows; it brings, all already in `Cargo.lock`, rustls-native-certs and openssl-probe on Linux,
  security-framework on macOS, windows-sys on Windows. And the keyring's crates above.
- **The UI thread never waits** on the network or the keyring; it drains channels.
- **Painting:** a remote terminal is a viewer, painted as any terminal; frames are fed off the UI thread. A new
  `--bench-mirror` runs one Templum as both ends: the `--bench-mixed` cockpit shared over loopback and its remote
  workspace on screen, its terminals streaming; `p95_ms` ≤ 16.7, pasted in the pull request.
- **Memory is bounded:** per remote, one pending state, at most 12 terminal streams, one pending frame each, events of
  at most 16 MiB, 1024 waiting keys; viewers without scrollback.
- **Tests** run on Linux, macOS and Windows, with no agent CLI, no real keyring (keyring-core's mock store, or the file
  in a temporary folder), no network beyond loopback, no sleeps (conditions with a deadline):
  - unit: the code (round trip, cut short, newer, plain HTTP refused), the pinned verifier (a match, a mismatch, a
    bad signature), the public check with verifiers the test builds from roots it makes (the client takes its "system"
    and "public" verifiers as rustls verifiers; rcgen: an authority and a leaf for `localhost`; a chain the test's
    "system" verifier accepts and its "public" one doesn't is the private authority), `CONNECT` through a proxy the
    test runs on loopback, the session's `replaced` (two clients, one code), the SSE
    reader (fields, multi-line data, comments, the cap), backoff, frame to bytes (sanitized: an ESC in a run is
    dropped), the key names (each new name to its bytes in each mode), the redacted key;
  - an integration test in `templum-api`: a server and a client in one process, over plain loopback and over TLS
    pinned: snapshot, deltas, a terminal's frames, `terminal_input`, `GET /v1/me`, a revoke that cuts the client with
    `401`, a changed pin refused, the server stopped and started again and the client back with `Last-Event-ID`;
  - the end to end test (*the orchestrator's call*): a headless workspace that shares one of its workspaces and
    mirrors it in a remote workspace over loopback, in one process: home's blocks appear at home's rectangles;
    what a shell at home prints shows in the mirror's viewer; a key typed on the mirror reaches home's terminal; a move
    on the mirror moves home's block; a question answered on the mirror is answered at home; Revoke in the Edit…
    dialog cuts the mirror; the remote is never saved as a canvas and no local tool sees its blocks.
- **Every feature in the API:** docs/api.md gains `GET /v1/me`, `shares`, the share tools, `read_only`, `icon`,
  `modes`, the key names, `Templum-Mirror` and `replaced`, and the audit log's grouping, and a "Templum to Templum"
  section for people (share, cloudflared, connect, what each way in checks, Cloudflare Access with a service token, a
  proxy, what a mirror can't do yet).
- **Specs to update as batch 1 lands:** api.md (its Templum to Templum section points here), workspaces.md and
  cockpit.md (remote tiles, the Connect button, the Edit… dialog's Sharing), settings.md (shares among the tokens),
  ARCHITECTURE.md (`templum-api`'s client, the remote workspaces' glue), CLAUDE.md (`--bench-mirror`), this file's
  Status and As built.

## Batches

*The orchestrator's call:* each batch usable on its own; batch 1 the smallest that makes the person's case real;
later batches only as far as batch 1 prepares for them.

- **Batch 1: keep working from another computer.**
  - Home: the Edit… dialog's Sharing and the Share dialog; the code; the tile's mark; shares in Settings ▸ API (and
    those of a deleted workspace); New certificate's count. API: `GET /v1/me`, `shares` and `share` events,
    `share_workspace`, `revoke_share`, `read_only`, `icon`, `modes`, the key names, the mirror's session and
    `replaced`, the audit log's grouping.
  - Mirror: Connect to a Templum…; the code; both ways in and the private authority check; the proxy; Cloudflare
    Access's service token; the keyring and the file; the remote tile, banner and canvas; terminals live (12 at most);
    the keyboard; moving and resizing; requests with their sound and notification; the "not mirrored yet" cards and the
    disabled actions; every state of [When it isn't live](#when-it-isnt-live), Start of a stopped workspace included;
    Edit…, Disconnect, Forget; the mirror's API state.
  - `--bench-mirror`, the tests above, docs/api.md.
- **Batch 2: the rest of the canvas, live** (sketched only; its design comes with its plan):
  - Boards drawn from the state and acted on (`move_card`, `create_card`, `edit_card`, `comment_card`); checklists
    and notes' text (home's state gains them for the blocks that show them), checklists checked from the mirror;
    ink and shapes (home's state gains them) shown.
  - Adding and closing blocks, wires, handing a task, opening an agent at home from the mirror; the Agents popup with
    remote agents.
  - Scrolling back (`read_terminal` into the viewer's history) and the mouse (from `modes`).
  - Batch 1 prepares it: the "not mirrored yet" card is the one `Body` arm these replace; the state and frames only
    grow; a remote workspace is already a workspace.

### Later batches

Sketched so batch 1 closes no door, each its own card: editing notes and checklists from the mirror; drawing; an
editor's files through the API; following home's camera; **sealed through tunnels**: Templum's pinned TLS carried
inside one WebSocket the tunnel passes, so not even the tunnel's provider sees the traffic (WebSocket is part 1's
non-goal, so it would be its own card).

## Settled after the builder's read

A builder read this design before it was handed over; each answer is *the designer's call* and is written in above:

- **Windows' lazy roots and a proxy from the Start menu:** rustls-platform-verifier for the public way in, and a Proxy
  field beside the variables.
- **Dead connections holding the token's 16 after a laptop sleeps:** the mirror's session header, and `replaced`.
- **A drag as dozens of Ctrl+Z steps and calls at home:** moves are sent on release, one per block.
- **A stopped workspace:** the mirror says so and offers Start, through `show`.
- **Revoke on delete vs "deleted":** deleting doesn't revoke; `GET /v1/me` tells the mirror.
- **Typing filling the audit log:** `terminal_input` and `GET /v1/me` lines grouped per token and minute.

## Settled by the orchestrator

The designer left four questions for the person; the person asked not to be asked, so each is *the orchestrator's
call*, taking the designer's recommendation:

- **The office's proxy:** built as written (a typed or environment proxy, Basic auth, the company-proxy warning);
  NTLM and PAC proxies are out of batch 1 and named in docs/api.md.
- **A share's default scope:** Full, since typing needs it, with the warning that the code runs commands at home;
  Control one click away.
- **A share's default expiry:** 30 days, as tokens; the mirror warns 3 days before.
- **The tunnel's provider sees the traffic** in the public way in: accepted for batch 1, Cloudflare Access's service
  token supported, sealing through tunnels a later card.

## Left to the builder

The looks of the banner, the marks, the cards and the dialogs (theme tokens only); words of tooltips not quoted here;
the threads' and channels' layout inside the rules above; how a workspace is marked remote and the "not mirrored yet"
`Body` arm is named; the SSE reader's shape; how the scaled cells are computed within the font steps; the JSON
names of the index entry; the constants' and functions' names; which local tools need the remote guard, as long as the
end to end test proves none sees a remote block.

## As built

### Batch 1a (home's side)

Plan: [2026-10-25-t2t-batch-1a-home.md](../plans/2026-10-25-t2t-batch-1a-home.md). What the builder chose or changed:

- **Key names** are parsed once, in `templum_terminal::named_key` (`NamedKey`, owned; `.input()` is the `KeyInput`), for
  `send_keys` and `terminal_input`. `ctrl+1` and `ctrl+é` were refused before and are names now (the program gets
  what `encode_key` makes of them: nothing for `ctrl+1`; for `ctrl+é` nothing in legacy mode and `CSI 233;5u` in
  the kitty protocol). `alt+X` and `alt+shift+x` send `ESC X`: only Ctrl drops the typed character.
- **`modes`** ride on `Screen` (`typing` always false) and on every frame; a change of them alone is a frame with no
  rows. The OpenAPI `Frame` schema leaves `modes` optional (older homes send none).
- **A share** is `Token::share` (`Share { workspace, way, address }`) in `api.json`; `Store::tunnel` remembers the
  last tunnel address. `Store::mint_share` makes the token and answers its text once.
- **A limited token's view of the state** now holds its own share, so the event stream no longer shares one
  comparison among limited tokens of the same workspaces: the key is the token as well.
- **`share_workspace`** leaves `expires` optional: null or left out is never (the Share… dialog's default is 30 days).
  A Directly address with a scheme, or none, is refused "Give Directly's address as host:port."; the code is tried
  through `code::decode` before the token is made, so a code the other Templum cannot read is never made.
- **`revoke_share`**'s registry scope is Read (nominally "full, or the share itself"); the glue asks Full of a token
  for all workspaces to revoke another's.
- **The mirror's session:** a replacing event stream is looked for before the token's 16-connection check, so stale
  connections can't keep the new one out. The replaced streams get `event: replaced` as their last frame
  (`Cell::end_with`; `Cell::end` drops what is queued); their connections close with the body, and the connections
  with nothing streaming are aborted. The streaming ones are marked replaced at once, so they no longer count against
  the 16, and are cut after 2 s if their body has not flushed (a sleeping laptop's full send buffer would keep them
  for minutes). A request without `Templum-Mirror` replaces nothing. Each replacement writes a `mirror_replaced`
  line in the audit log.
- **A share revoking itself** gets its answer: the sweep that drops a dead token's connections spares only the
  `revoke_share` call being answered (its next request is `401`; its streams end through the hub). Every other call of
  that token, a waiting `delegate` or `wait_for` included, is cut with the token, and so is an event stream still
  opening when the revoke lands.
- **Replaced connections are capped:** a token's replaced connections that are still alive (a streaming one has 2 s to
  flush `replaced`) are at most 16; past that the oldest are aborted at once, so a mirror in a reconnect loop can't
  fill the 64 connections of the server.
- **The audit log** groups `terminal_input` and `GET /v1/me` as the noise is grouped (key: address, call, token, status), into
  `audit.log`; the test that expected two `terminal_input` lines now expects one. A mirror's grouped line that the full noise half gives up is counted in the `lost` line, like any call.
- **The Edit… dialog** keeps **Share…** below the rows too, so one more computer can be shared with. The Share…
  dialog makes its own footer: Make code is disabled while the API doesn't fit or the name is empty.
- **`Server::mirrored`** says the address of a token's oldest connection with an event stream; the stream's opening
  and its end tell the UI thread (`CLIENTS`), which draws the sidebar's mark from the server.
