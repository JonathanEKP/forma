# 10 — Pairing and devices

Area T1 of the browser test pass: pairing codes and devices on `/agents`, and what they do to the chat.
Run 2026-10-04 (UTC) on **instance 2** (editor `http://127.0.0.1:4822`, sidecar `:4821`), signed in as
person A on `127.0.0.1` and person B on `localhost`. Runtime: the **test-driver with FakeProvider** for
every chat case, the **real `pontis` CLI and daemon** (Pontis `target/debug/pontis`, isolated
`--data-dir`/`--runtime-dir`/`--config` and `HOME`, never the owner's) for the pairing, presence and revoke
cases it can answer better than the driver. No real Codex turn was sent (0 of the ≈25 budget): nothing in
this area depends on the model's judgement. Format: [README](README.md).

How the browser was driven: the Orca tab for instance 2 only (`--page`), DOM clicks for buttons (F-00-5),
`curl`/`node` against `:4821` for the routes a browser cannot reach (`/pons/v1/pairing/claim`, forged and
foreign tokens, many codes at once). Direct claims used a small Ed25519 script kept in the scratchpad, not in
the repository. No token appears in this file or in the evidence.

Harness notes (not product findings): the built `forma-server` and `forma-pontis-test-driver` came from the
main checkout through `FORMA_SERVER_BIN` / `FORMA_PONTIS_DRIVER_BIN` (this worktree had no `target/`); that
driver binary predates the source and *requires* `FORMA_DRIVER_BASE_VERSION`. A driver started without a
held-open stdin exits right after `READY` (documented: "exits when its host closes stdin"), which cost two
runs before I noticed. `orca screenshot` of a scrolled page returned a blank or stitched image; hiding the
other sections of the page through the DOM and screenshotting at scroll 0 gave clean pictures.

## Summary

36 cases, 13 findings. By severity: **0 blocker, 2 high, 4 medium, 5 low, 2 info** (the info items are
limitations or observations, not defects). The two highs are the same hole seen from two sides: the
sidecar keeps no state across a restart, and the chat has no way out of a turn whose Runtime disappeared.

| Id | Severity | One line |
|---|---|---|
| F-10-1 | high | A sidecar restart forgets every device, pairing code and chat; the Runtime stays "running" and silent |
| F-10-2 | high | Revoking a device (or killing its Runtime) mid-turn leaves the chat "working" forever, Send disabled, even after reload and after pairing a new device |
| F-10-3 | medium | The Devices panel is a snapshot: a paired device does not appear, and a dead one stays "online", until a reload |
| F-10-4 | medium | After a revoke the chat says three different things and none of them is "this device was revoked" |
| F-10-5 | medium | `ux` The pairing instructions stop one step early and use words a first-time person does not have |
| F-10-6 | medium | `ux` Pairing a second machine silently breaks the chat (`ambiguous_device`), and the page invites it |
| F-10-7 | low | "last seen" is the last connect/use, not the last sign of life |
| F-10-8 | low | The pairing claim's rate limit answers `bad_request`, is shared across every Runtime on the address, and refuses a valid code |
| F-10-9 | low | `ux` The device list never prunes and cannot tell two machines apart |
| F-10-10 | low | Missing/forged token answers 403 `origin_not_allowed`; a foreign device is 403, not 404 (Pontis) |
| F-10-11 | info | Facts about revoke: idempotent but the second click writes a second durable event; a revoked key can pair again; the real `pontis` says nothing helpful about it |
| F-10-12 | info | Presence of a frozen Runtime takes 32–48 s to read offline; a killed one is instant |
| F-10-13 | low | Two Runtimes holding one device identity displace each other every ~5 s, silently, and the device reads "online" throughout |

## Cases run

`pass` = behaved as the plan/docs say. `FAIL` = a finding. Case ids are for cross-reference only.

| Id | What | Result |
|---|---|---|
| P01 | A, `/agents`: Create pairing code | pass — code, the `pontis pair …` command, "Shown once and expires 2026-10-04 02:48 UTC" ([screenshot](evidence/pairing/10-baseline-devices-panel.png)) |
| P02 | Pair the test-driver with that code; see it listed | pass at the API (`online: true` within 3 s); **FAIL in the UI** until reload → F-10-3 |
| P03 | Real `pontis pair` with the UI's command (plus isolation flags) | pass — `paired Device dev_… for application forma`; the name is the placeholder "My laptop" unless changed |
| P04 | Real `PONTIS_MODE=remote pontis run --foreground` | pass — `runtime.started`, listed online with its real provider row (Codex 0.160.0, `authMode unknown`) |
| P05 | `kill -9` the driver: how long until offline | API offline in **24 ms** (loopback: the kernel closes the socket); **FAIL in the UI** — still "online" 45 s later → F-10-3 |
| P06 | `SIGSTOP` the driver (a frozen or partitioned Runtime), then `SIGCONT` | offline after **31.7 s** and **48.0 s** (two runs); back online 0.55 s after `SIGCONT` → F-10-12 |
| P07 | Revoke from the UI (Revoke → Confirm revoke) | pass — card turns "revoked · Revoked 2026-10-04 02:43 UTC", the device's socket is closed (`ss`: no connection), `device.revoked` appears in the principal stream |
| P08 | Revoke: two activations of Confirm in one tick | pass — one device revoked, no error; two `device.revoked` events → F-10-11 |
| P09 | Chat start after revoke, only device revoked | pass at the API (`409 {"error":"runtime_offline"}` for start, input, cancel, approval); wording → F-10-4 |
| P10 | **Revoke while a chat turn is running** (`Validate the long design.`) | **FAIL** → F-10-2 |
| P11 | `kill -9` the Runtime while a turn is running | **FAIL** → F-10-2 |
| P12 | Reuse a consumed code | pass — `401 pairing_code_consumed: "this pairing code has already been used; generate a new one"` |
| P13 | Use an expired code (lifetime: **10 minutes**, `PAIRING_CODE_TTL_MS`) | pass, with a wrinkle — see *P13 result* below |
| P14 | Garbage: unknown code, empty, whitespace, `code: 5`, non-JSON body, `{}` | pass — `pairing_code_invalid` (same text for unknown and malformed, by design), `bad_request`, `unsupported_protocol_version` |
| P15 | Code minted by B, claimed by a Runtime; where does the device land | pass — B's list only; A's list unchanged; the claim answers with B's `userId`. A has no way to use B's code from A's session (the UI only mints for the signed-in person) |
| P16 | As B: list/revoke A's devices with B's token (API) and `/agents` as B (UI); as A: B's | pass — own list only; revoke of a foreign id is `403 device_not_found` ("no such Device is paired with this account"), identical to a nonexistent id; UI as B shows only B's device |
| P17 | No token, garbage token, tampered payload (A's signature, B's claims), `alg: none`, forged `exp` | pass — all `403 origin_not_allowed`, nothing leaked; wording → F-10-10 |
| P18 | 60 code mints in parallel, then 40 more | pass — all 200, all distinct; no limit on outstanding codes |
| P19 | 25 claims in a row | 429 after the 20th; a **valid** code was refused during the window → F-10-8 |
| P20 | Two online devices for A | pairing/listing pass; chat says "More than one of your devices is online" → F-10-6 |
| P21 | Revoke a device, then pair the **same public key** again (and again, unrevoked) | accepted both times, three records with one key → F-10-11 |
| P22 | Reload with a freshly minted, unclaimed code on screen | pass — the code is gone from the page and still claimable (200); there is no way to see it again |
| P23 | Three activations of Create in one tick | three codes minted, only the last shown; harmless (the others live 10 minutes, unseen) — low, no entry |
| P24 | **Sidecar restart** (devices online, code unclaimed) | **FAIL** → F-10-1 |
| P25 | Hostile device names (`<img onerror>`, `<script>`, 300 chars, newline/tab/emoji/U+202E, empty) | pass — rendered as text, none executed; capped at 120 chars (the UI truncates with `…`), newline → space, empty → "Device"; the right-to-left override survives in the stored name (low, no entry) |
| P26 | Sidecar down, load `/agents` | pass — "Pontis is unavailable right now, so devices cannot be shown or paired. Nothing else in Forma depends on it." |
| P27 | B starts/continues/cancels A's session; B starts a session when only B's (offline) device exists | pass — `runtime_offline`, no leak of A's session |
| P28 | `GET` on `/pairing/codes`, `DELETE` on revoke, path-traversal / SQL-ish device ids | pass — 405, 405, `device_not_found` |
| P29 | Cross-origin `Origin` and spoofed `Host` with a valid bearer | accepted (200) — info: these routes are bearer-only, no cookie, so there is nothing to forge from a page that lacks the token; the UI channel is the one that checks Origin/Host |
| P30 | Real daemon after revoke; re-pair on the same machine | daemon prints `the paired Device has been revoked` and exits 1; `pontis pair` again: `configuration error: a Device is already paired; refusing to replace its identity` → F-10-11 |
| P31 | First-time person walkthrough of the page | `ux` → F-10-5, F-10-6, F-10-9 |
| P32 | Eight Runtimes claim one code at the same instant | pass — exactly one `200`, seven `401 pairing_code_consumed`, one device |
| P33 | A person types the code: lowercase, without dashes, with spaces and padding | pass — all accepted (normalised); a wrong character is `pairing_code_invalid` |
| P34 | Clone a paired Runtime's data dir to a second dir and run both real `pontis` daemons; then revoke | **FAIL (low)** → F-10-13; revoke stopped both (`the paired Device has been revoked`) |
| P35 | Revoked driver cannot act: freeze the driver (`SIGSTOP`), `session.input`, revoke, `SIGCONT` | pass — the revoked driver never ran the buffered input (no `turn.started`, no Boundary F call in `sidecar.log`). Not exercised: replaying a delegation token captured before the revoke (by code `validateDelegation` refuses a revoked device, `control-plane.ts:679`) |
| P36 | Sidecar down: click Create pairing code on an already-loaded `/agents` | pass — the panel switches to the "Pontis is unavailable…" state, no raw error |

### P13 result

Code minted 03:06:02, `expiresAt` 03:16:02.339Z (exactly 10 minutes). Claimed at 03:16:26: first attempt `401
{"code":"pairing_code_expired","message":"this pairing code has expired; generate a new one"}`, no device created;
 **second** attempt `401 pairing_code_consumed` ("this pairing code has already been used; generate a new
one"). The expiry check runs after the atomic consume (`claimPairing`, by design, so two racers produce one
Device), so an expired code is spent by being tried and a person retyping it is told the wrong reason the
second time. (An earlier attempt at this case was lost to a mistake of mine — I claimed the code early — and
another to F-10-1: a sidecar restart turned the code into `pairing_code_invalid`.)

## F-10-1 — A sidecar restart forgets every device, pairing code and conversation, and the Runtime never finds out

- **Severity:** high
- **Status:** fixed (dff7168)
- **Instance / provider:** instance 2, FakeProvider test-driver **and** the real `pontis` daemon

**Steps to reproduce**
1. Instance 2, A signed in. Pair a Runtime (UI code → `pontis pair` or the driver); leave it online. Mint a
   second code and leave it unclaimed. Open the chat and complete one turn.
2. Stop the sidecar (`kill <pid>` of `node …/pontis-sidecar/src/main.ts`) and start it again with the same
   environment (the harness's own `sidecar` process; mine was started by hand with a fresh service credential).
3. `GET /pons/v1/devices`, the Agents page, the chat, `pontis status`, and the unclaimed code.

**Expected** — A deployed restart of the sidecar (a release, a crash, a reboot) leaves a paired machine
paired: the plan treats a Device as a durable record ("Machines you have paired", plan §10a), Pontis
describes pairings that survive a restart (`services/control-plane/src/store.ts`: "a control plane that forgets
its pairings on restart would make …"), and a person has no reason to expect that updating Forma unpairs their
laptop. At the very least the Runtime should be told it is unknown.

**Actual** — Everything is gone. `GET /pons/v1/devices` → `{"protocol":"pons/1.0","devices":[]}`; the
Agents page lists no device; the chat's status becomes "No device is paired. Pair one from the Agents page."
and the history is empty; the unclaimed code answers `401 pairing_code_invalid` ("the pairing code is not a
valid code" — not "expired"). The Runtimes are orphaned and say nothing: the real daemon stays
`Runtime: running / Paired: true / Providers enabled: 1` with the log `runtime.started` and nothing after
it, holds **no socket** (`ss -tnp`: none at 25 s and again 30 s later; the test-driver the same at ~40 s), and
never reports an error. I did not watch beyond about a minute, so a very slow back-off is not excluded. The only recovery is `pontis unpair --yes`, a new code and `pontis pair` — which the
person has no reason to think of, because nothing tells them. The restart also drops every **revocation** record
with the rest.

**Cause (read, not fixed)** — `apps/pontis-sidecar/src/sidecar.ts:90` builds
`new ControlPlane({ applicationId, audience })`, which defaults to the in-memory stores; Pontis ships the
durable `Journal` (`services/control-plane/src/journal.ts`) for exactly this. Nothing in the plan, the
sidecar README or `docs/deploy/*` says the state is in memory, so the production deploy would lose it on every
restart without anyone having been told.

**Evidence** — instance 2 `logs/sidecar.log` (`sidecar_stopping` 02:47:25, `sidecar_started` 02:47:38 and
again at 02:56:57), `GET /pons/v1/devices` before/after quoted above, `pontis status` output quoted above.
Not captured as a screenshot: the browser showed the two sentences quoted.

**Fixed** — the sidecar takes `FORMA_PONTIS_DATA_DIR` and composes the Control Plane with Pontis's
`JournalControlPlaneStore` and `JournalEventStore` there (`0700` directory, `0600` files; there is no
job journal because the sidecar has no job queue). A restart on the same directory keeps devices,
pairing codes still in date, revocations and the event history; `src/durable.test.ts` closes a sidecar
and opens another on the same directory. Unset keeps today's in-memory behaviour and logs
`pontis_state_in_memory` at warn. Browser proof, instance 9: a paired Runtime and a finished chat turn
survived a `SIGTERM` + restart (device still listed, `online` again 1.2 s after the sidecar answered,
transcript present after a reload). **The Runtime reconnects by itself** — that is the test-driver's
(Pontis's `Runtime`) behaviour, not changed here. Not run: the real `pontis` daemon, which the finding
saw hold no socket against an *empty* sidecar; against a sidecar that still knows it, only the
test-driver was observed. `forma-server backup` does not include this directory yet (documented).

## F-10-2 — Revoking a device or killing its Runtime mid-turn leaves the chat stuck for good

- **Severity:** high
- **Status:** open
- **Instance / provider:** instance 2, FakeProvider (the stuck state is in the editor and the event log; no model involved)

**Steps to reproduce**
1. Instance 2, A signed in, one device online (the harness driver). Open "ALPHA Launch cover", open **Agent**.
2. Send `Which projects do I have, and what is in my design?` (answered), then `Validate the long design.`
   (about 3.3 s).
3. While it runs, revoke the device (`POST /pons/v1/devices/<id>/revoke`, the call the Revoke button makes —
   the chat lives in the editor, the button on another page). Variant: `kill -9` the driver instead.
4. Watch the chat for a minute; press **Stop**; reload and reopen the design and the chat; pair a new,
   healthy device (UI code → driver) and look again.

**Expected** — The turn ends: either the Runtime's disappearance is shown ("This device was revoked / went
away; the turn did not finish") and the box is usable again, or at least **Stop** unsticks it. The person is
never left unable to talk to a perfectly good new device. `chat.ts` (the `stopped` action) already says why the Stop
path exists: "waiting for [a terminal event] would leave the box saying 'working' over a session nobody is
running".

**Actual** — The transcript ends with `design.validate Runs design.validate · running` and
**"The agent is working…"** forever. The status line reads "Forma local run is offline. Start the Runtime on
it to chat." (it is *revoked*, not offline). **Send** is disabled, **Stop** is on. Stop says "Your Runtime is
offline. Start it on your device, then try again." and the turn stays "working" — and with a new online
device, "This session isn't allowed to do that." (it routes to the new device, which does not own the
session). After a reload the same stuck turn is replayed from history; with a **new, online device** the status
is healthy and Send is still disabled. The durable events for the session end at `capability.requested
design.validate` (seq 11): no `turn.*`, no `session.failed`. A principal-scoped `device.revoked` is written,
but the chat deliberately ignores principal frames (`events.ts` `asDurable`; `Chat.test.tsx` "ignores … a
principal frame"). A person has no way out; the only thing that cleared it in this run was F-10-1 (a sidecar
restart wiping the history). The kill -9 variant behaves the same (screenshot not taken; same text and
buttons).

**Evidence** — [screenshot](evidence/pairing/10-revoke-mid-turn-chat-stuck.png) (the stuck transcript, the
status line, Send disabled/Stop). Event page excerpt: `10 turn.started`, `11 capability.requested
design.validate`, `head: 11`; principal stream: `3 device.revoked dev_5e99…`. Sidecar log: `boundary_f
design.validate 200 3325 ms` one second *after* the revoke — the work the Runtime had already asked for still
ran and its result went nowhere — then `intent session.cancel delivered:false authorized:false`.

## F-10-3 — The Devices panel is a snapshot: new devices do not appear, a dead one stays "online"

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 2, test-driver and real `pontis`

**Steps to reproduce**
1. A signed in, `/agents` open with one device online. Pair two more Runtimes (UI code, then the driver or
   `pontis pair` + `pontis run`).
2. Look at the page without reloading. Then `kill -9` the one that is online; wait 45 s; look again.

**Expected** — The page that exists to show the state of the person's devices shows it: the new device
appears (it is *the* confirmation that pairing worked, and the page itself asked the person to go and do
something elsewhere), and presence follows reality. Plan §11 step 10c built live delivery for the chat and
presence; the Control Plane writes `device.*` events and the live feed carries them.

**Actual** — `DevicePanel` reads once on mount and after a revoke (`devices-panel.tsx` `refresh`), never
again; there is no polling, no focus refresh and no use of the live feed. After pairing two devices the page
still showed one article (`document.querySelectorAll('article').length === 1`) for 20+ seconds until a
reload showed three. After `kill -9`, the API said `online: false` within 24 ms and the page said
"online · last seen 2026-10-04 02:55 UTC" 45 s later. "Online" is the one word this panel exists to get
right and it is a snapshot of whenever the page was loaded.

The pairing-code card has the same problem: after the code is claimed it still shows "Shown once and expires …" as if live.

**Evidence** — DOM text read from the page (`article` count, `online · last seen …`) against
`GET /pons/v1/devices` in the same minute. No screenshot (the "wrong" state is two lines of text).

## F-10-4 — After a revoke the chat gives three different, unhelpful answers

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 2, FakeProvider

**Steps to reproduce**
1. A's only device is revoked from the Agents page. Open a design and the chat.
2. Read the status line, type a message (the box is enabled), Send.

**Expected** — One consistent, true statement: the device was revoked, pair a new one. Revoked is a
different state from offline: a revoked Runtime never comes back, and "Start it on your device" is exactly
the wrong advice (a real `pontis run` exits with "the paired Device has been revoked").

**Actual** — The status line says **"No device is paired. Pair one from the Agents page."**; the box is
still enabled; Send answers **"Your Runtime is offline. Start it on your device, then try again."** (and
leaves the person's line in the transcript). When revoked devices sit beside never-connected ones the status
instead says "Forma local run is offline. Start the Runtime on it to chat." (same instruction, wrong
reason). `chat.ts` drops revoked devices before it classifies (`runtimeView`) and so cannot say "revoked";
`runtime_offline` is the sidecar's only word for it.

**Evidence** — [screenshot](evidence/pairing/10-chat-after-revoke-none-vs-offline.png) (crop of the panel:
status line and the Send answer side by side).

## F-10-5 — `ux` The pairing instructions stop one step early and use words a first-time person does not have

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 2; the real `pontis` CLI

Walked through as someone who has never heard of Pontis, using only what the page says
([screenshot](evidence/pairing/10-baseline-devices-panel.png)):

1. **What a "device" is.** "Machines you have paired to run an agent for Forma, locally." It does not say
   what runs there (an agent *Runtime*), why one would want that rather than the MCP URL above it, or what
   the person gets (chatting with an agent in the editor). The sentence "This is separate from the MCP
   connection above: either way … keeps working without the other" says what it is *not*. A person
   deciding between the two big sections on the page — "Use Forma with your favourite AI" (URL, OAuth) and
   "Personal access tokens" — gets no sentence that says which to pick; "Devices" sits between them.
2. **"Pontis" is never explained or linked.** The page says "with Pontis installed there and its daemon
   stopped" and the command starts `pontis pair`. Where Pontis comes from is not said, and it is
   unpublished (the sidecar README: `private: true`).
3. **"daemon stopped"** is jargon and a precondition nobody can check from the page (I did not test pairing
   with the daemon running).
4. **The step after pairing is missing.** After `pontis pair` prints `paired Device dev_…`, the device is
   **offline** until `pontis run --foreground` is started; the page never says to start it, and the panel
   will not show anything change (F-10-3). A person who follows the page exactly ends with a paired,
   offline, silent machine.
5. **The command is not copy-safe.** `--name "My laptop"` is the placeholder, so two machines copied
   verbatim are both "My laptop" (the list cannot tell them apart, F-10-9). The command has no flags for the
   data dir, which is right for a person's own machine and means I could not run it verbatim without
   touching `~/.local/share/pontis` — I added `--data-dir/--runtime-dir/--config` before the code.
6. **"expires in minutes"** (text) vs "expires 2026-10-04 02:48 UTC" (the card): the real figure is 10
   minutes and an absolute UTC time is the only way it is said; "Shown once" is true (a reload loses it, P22).
7. **Revoke wording.** "Confirm revoke / Keep" is clear; nothing says what revoke does to a running turn
   (F-10-2) or that the machine must `pontis unpair` before it can pair again (F-10-11).

**Expected** — Plan §7.3 / §10a promise a panel a first-time person can use. Suggested by the above, not a
design: a line saying what the device is *for*, where `pontis` comes from, the two-step "pair, then run",
and a visible state for "paired, waiting for it to start".

**Evidence** — the screenshot; the real CLI output quoted in P03/P30.

## F-10-6 — `ux` Pairing a second machine silently breaks the chat

- **Severity:** medium
- **Status:** open (the routing rule itself is F-00-6, wontfix by design; the page is what is at issue)
- **Instance / provider:** instance 2, test-driver

**Steps to reproduce**
1. A with one online device and a working chat. Mint a code on `/agents`, pair a second Runtime, leave
   both online. Open the chat.

**Expected** — A person who is told "Pair a new machine" is told the consequence, or can pick the machine.

**Actual** — Pairing works and both show online. The chat's status line says "More than one of your devices is online, so
Forma can't tell which one to use." and `POST /agent/intent session.start` answers `409 ambiguous_device`
(the failure message in `chat.ts` adds "Leave one connected."; I did not type into the box in that state, because
the chat was already stuck by F-10-2). The Devices page never mentions that only one Runtime may be online at a time, there is no
way to choose, and the only remedies are to stop one machine or revoke it. A person with a laptop and a
desktop is invited to pair both and then cannot chat from either whenever both are on.

**Evidence** — the chat's status line, read from the DOM; `/pons/v1/devices` listing two `online: true`.

## F-10-7 — "last seen" is the last connect or use, not the last sign of life

- **Severity:** low
- **Status:** open (Pontis stores `lastSeenAt` on connection and on credential use)
- **Instance / provider:** instance 2, test-driver

**Steps to reproduce** — Pair a device at 02:37 and leave it online. Read the card at 02:44.

**Actual** — "online · last seen 2026-10-04 02:37 UTC" for a device that has been connected for seven
minutes; a killed device's "offline · last seen" is the time it connected, not the time it vanished
(driver killed ~02:41, card says 02:39). The time moves only when something new happens (starting a chat turn
moved it to 02:44:21). Wording suggests presence data that the field does not carry.

**Expected** — For online: no "last seen" (or "connected since"); for offline: when it went away — or the
label says what the time is.

**Evidence** — `GET /pons/v1/devices`: `lastSeenAt` 02:37:02.624Z for 7 minutes of `online: true`; killed
device `lastSeenAt` 02:39:23 = its connect time.

## F-10-8 — The pairing rate limit refuses a valid code, shares one bucket, and answers `bad_request`

- **Severity:** low
- **Status:** open (Pontis control plane; Forma's part is that every Runtime reaches the sidecar from one address)
- **Instance / provider:** instance 2

**Steps to reproduce**
1. Send 25 claims in a row (any garbage code) to `POST /pons/v1/pairing/claim`.
2. Within the minute, claim a **valid** code; also `POST /pons/v1/device/challenge`.

**Actual** — 401 for the first 20, then `429 {"error":{"code":"bad_request",…,"message":"bad_request"}}`
for the rest of the 60 s window, for the valid claim and for the challenge route too. The bucket is
`request.socket.remoteAddress` shared by claim, challenge and auth (`services/control-plane/src/http.ts`
`RATE_LIMIT_ATTEMPTS = 20`), so every Runtime and every attacker behind one address — loopback here, a
proxy in the deployment — is one bucket. A Runtime whose credential is valid reconnects without a challenge
(a frozen-and-resumed driver came back in 0.55 s right after a 25-claim burst), so the effect shows on pairing and on anything that needs a challenge (read from the code: the 15-minute
credential refresh; not exercised).

**Expected** — A distinguishable "too many attempts, try again in N s" (the code `bad_request` is also what
a malformed body gets), and the person minting a code is not locked out by someone else's guesses.

A 429 does **not** consume the code: the same code claimed after the window gave `200`.

**Evidence** — status lines above; no screenshot (a CLI-level behaviour).

## F-10-9 — `ux` The device list never prunes and cannot tell two machines apart

- **Severity:** low
- **Status:** open
- **Instance / provider:** instance 2

**Actual** — Every pairing is a card for ever: after about forty minutes of testing the page held 13 cards (DOM count), most revoked or never
connected, nearly all named "Forma local run" (the driver's name; the real CLI's "My laptop" has the same
problem when copied). A card shows no device id, no way to rename, no paired-at date (the API returns `pairedAt`; the card does not show it),
and no way to remove a revoked one. Revoked and live cards are interleaved by
creation order, with the live one near the bottom. Long names are cut with `…` and have no tooltip. Every Revoke button's accessible name is "Revoke Forma local run", so
neither a screen reader nor an automation can say which one it is pressing (I had to count `article`s).

**Expected** — A person with three machines can tell which is which and can clear the graveyard.

**Evidence** — [screenshot of hostile and long names](evidence/pairing/10-baseline-devices-panel.png) is the
clean baseline; the list of 13 same-named cards was read from the DOM.

## F-10-10 — Absent or forged token answers 403 `origin_not_allowed`; a foreign device is 403, not 404

- **Severity:** low
- **Status:** upstream (Pontis control plane's vocabulary; Forma's own `GET /pons/v1/devices` copies it on purpose: "Same refusal the Control Plane's own route gives")
- **Instance / provider:** instance 2

**Actual** — No bearer, a garbage bearer, a payload swapped onto another account's signature, `alg: none`
and a forged `exp` all answer `403 {"error":{"code":"origin_not_allowed","category":"transport"}}`. An
unknown or foreign device id on revoke answers `403 device_not_found`. Nothing leaks (this is the intended
behaviour so that device ids cannot be probed), but a person whose session has expired gets an error code
about an *origin*, and the editor's panel shows it as a generic failure rather than "sign in again".

**Expected** — Not a security matter: `401` with an "expired session" code for an invalid token, so the
editor can say so (the chat already has `not_authenticated`: "Your sign-in has expired. Sign in again.").

**Evidence** — the curl responses quoted in P17 and P16.

## F-10-11 — Facts about revoke worth knowing

- **Severity:** info
- **Status:** open (mostly Pontis)
- **Instance / provider:** instance 2, test-driver and real `pontis`

- **Revoke is idempotent but not quiet.** Two activations of Confirm in one tick left one device revoked and
  **two** `device.revoked` durable events for it (principal stream seq 1 and 2, same `revokedAt`).
- **A revoked key can pair again.** Revoke is per device record, not per key: after revoking, the same
  public key claimed a new code and became a new device (`dev_5ced…`), and claimed a third into a third
  record while the second was live. A person must mint the code, so this is authorised, but "revoke" does
  not blacklist a key.
- **The test-driver is silent after revoke; the real daemon is not.** `forma-pontis-test-driver` stays
  alive with no socket; `pontis run` logs `the paired Device has been revoked` and exits 1.
- **The real CLI's recovery path is not signposted.** `pontis pair` on that machine: `configuration error: a
  Device is already paired; refusing to replace its identity` (exit 2) — no mention of revocation or of
  `pontis unpair --yes`, which works (`local Device key and metadata removed; server-side revocation remains
  unchanged`) — and `pontis status` still prints `Paired: true` for a revoked device.
- **An expired code is spent by trying it.** First claim `pairing_code_expired`, second `pairing_code_consumed`
  (P13).
- **The in-flight work still lands.** The `design.validate` the Runtime had already asked for completed in
  Forma one second after the revoke (see F-10-2); a revoked Runtime's already-delegated call is not cancelled.
- **Device JSON.** `GET /pons/v1/devices` returns the person's own `userId` (the Supabase subject) and
  `publicIdentityKey` for each device. Only the owner can read them; noted because the browser holds them.

## F-10-12 — Presence of a frozen Runtime takes 32–48 s to read offline

- **Severity:** info
- **Status:** open (Pontis realtime gateway's liveness window; not a defect by itself)
- **Instance / provider:** instance 2, test-driver

`kill -9` is instant because the kernel resets the loopback socket. A Runtime that stops answering without
closing the connection (laptop lid, dropped Wi-Fi, `SIGSTOP`) read `online: true` for **31.7 s** and
**48.0 s** in two runs, then flipped, and recovered 0.55 s after `SIGCONT`. During that window the sidecar
routes a chat message to it and the turn never completes (F-10-2): with the driver frozen, `session.input`
answered `{"delivered":true}` (it means "written to the socket"), and after the revoke the driver, resumed,
never ran the buffered input. Combined with F-10-3 the *page* can show
"online" for a laptop that went to sleep minutes ago.

## F-10-13 — Two Runtimes holding one device identity displace each other every ~5 s, silently

- **Severity:** low
- **Status:** open (Pontis realtime gateway / runtime; Forma shows what it is told)
- **Instance / provider:** instance 2, real `pontis run --foreground` (two copies)

**Steps to reproduce**
1. `pontis pair` (UI code) with a scratch `--data-dir`; copy that directory to a second one
   (a restored backup, a cloned VM, or a person following the page's command on two accounts of one machine).
2. Start `pontis run --foreground` against each copy, with different `--runtime-dir`s (the same
   runtime dir is refused: `another Runtime process already holds the provider-workspaces lock`).
3. Watch `GET /pons/v1/devices` and each process's sockets for a minute; then revoke the device.

**Expected** — One connection wins and the other is told why (and says so to its operator), or the second is
refused; at the least the Agents page does not report a steady "online".

**Actual** — Both start (`runtime.started`) and neither prints anything more. The device record's
`lastSeenAt` advanced every ~5 s for the whole minute (03:06:53, :58, 03:07:03, :08, :14, :19, :24, …) and
`online` stayed `true`, while the two processes alternated between holding a socket and holding none
(`ss -tnp`, sampled every 5 s: 0/0, 0/1, 1/1, 0/1, 1/1, 0/1, 0/0, 1/0, …): each connect displaces the other
and each then reconnects. A chat message sent into that state goes to whichever holds the socket at that
instant and a turn is dropped when the other takes over — which is F-10-2's stuck turn without a revoke.
Revoking stopped both daemons cleanly.

**Evidence** — the sampled table above (`logs` of this run are scratchpad files, not committed); both
daemons' logs contain only `runtime.started` until the revoke.
