# 20 — Chat lifecycle (instance 3)

Area T2 of the Pontis → Forma browser break-test: the in-editor chat, start to end. Run on instance 3
(editor `http://127.0.0.1:4832`, signed in as A, `ALPHA Launch cover` / `ALPHA Square post`), in my own
Orca browser tab, 2026-10-04. Format: [README](README.md). The provider is stated for every case:
**Fake** is FakeProvider (plumbing, free), **Codex** is real Codex 0.158.0 `gpt-6-luna` low. Roughly
19 real Codex turns were spent (budget 25). Everything was seen in the browser unless the *Evidence*
line says it was a log, a process listing or `curl`.

The instance was started twice: first with FakeProvider (all the process-kill cases), then with
`--provider codex` (the agent-behaviour cases). Each start wipes the data, so design ids differ
between the two halves; none of them matter to a finding.

## Summary

| severity | count | ids |
|---|---|---|
| blocker | 0 | |
| high | 3 | F-chat-2, F-chat-4, F-chat-5 |
| medium | 6 | F-chat-1, F-chat-3, F-chat-6, F-chat-7, F-chat-8, F-chat-11 |
| low | 5 | F-chat-9, F-chat-10, F-chat-12, F-chat-13, F-chat-14 |
| info | 2 | F-chat-15, F-chat-16 |

The worst: **F-chat-2** — the agent's edit is real in Forma (a new version, actor `agent`) but the open
editor keeps showing the old version and the old picture, with nothing telling the person; the first
sign is a "changed elsewhere" banner on their own next edit. Next worst, **F-chat-4 / F-chat-5** — once
the Runtime's session is gone (Runtime killed, or restarted), the chat is wedged on that dead session,
across reloads, and has no way to start another.

What works well, for balance: the plumbing is sound. Streaming deltas appear and are replaced by the
final text; cancel ends a Codex turn as `turn.cancelled`; agent output and hostile input never render
as HTML; every outage (sidecar, forma-server, browser offline, UI stream) is named in the status line
within about ten seconds and recovers by itself; nothing was ever written twice.

## Cases run

`pass` = behaved as it should. `fail` = a finding. `note` = worked, with an oddity worth knowing (also
filed). "Both" = Fake and Codex.

| id | what | provider | result |
|---|---|---|---|
| C01 | open design, open the chat, read the status | Both | pass — "Connected · live", device, provider, classification chips |
| C02 | first message of a window: "Which projects do I have…" | Fake | note — answered, but the "You" line vanished: **F-chat-1** |
| C03 | `What layers are on this page?` (nothing else said) | Codex | fail — agent does not know which design is open: **F-chat-11** |
| C04 | small edit: `In design <id>, change the tagline…` | Codex | agent side pass (v2, actor `agent`, validated); canvas fail: **F-chat-2** |
| C05 | small edit `Raise the headline to 96pt.` | Fake | same: Forma v2, editor still v1: **F-chat-2** |
| C06 | streaming deltas | Codex | pass — a partial `AGENT …` row appears, then the final text replaces it |
| C07 | final answer, capability lines, results | Both | pass |
| C08 | edit by design *name* (`the design named "ALPHA Launch cover"`) | Codex | pass — `project.list` → `design.get` → `brand.get` → `applyOperations` → `validate`, Forma v4 |
| C09 | the person edits in the stale editor after an agent edit | Fake | pass — conflict banner "This design changed elsewhere. You were editing v1; it is now v2. Your last change was not applied." and nothing overwritten |
| C10 | banner's *Reload* | Fake | pass — header becomes v2 |
| C11 | leave and re-open the design, then *Undo* | Codex | pass — v3, `actor_type: human`, the agent's footer edit undone |
| C12 | empty message | Fake | pass — Send disabled |
| C13 | whitespace-only message, via Send and via Enter | Fake | pass — nothing sent |
| C14 | `<img onerror>`, `<script>`, `<b>`, `javascript:` link as the *person's* text | Fake | pass — shown literally, `window.__pwned` never set, 0 elements created |
| C15 | the same HTML as the *agent's* reply (asked verbatim) | Codex | pass — shown literally |
| C16 | unicode: accents, CJK, ZWJ emoji, Hebrew/Arabic, zero-width | Fake | pass |
| C17 | U+202E right-to-left override in the text | Fake | note — kept, rendered as given: **F-chat-12** |
| C18 | control characters (NUL, BEL, ESC, DEL) | Fake | pass — accepted, shown stripped; the driver's echo shows them escaped |
| C19 | 50,000-character message | Fake | pass — accepted and rendered, no horizontal overflow (`scrollWidth == clientWidth`) |
| C20 | 70,000-character message | Fake | fail — "The agent service answered something unexpected.": **F-chat-10** |
| C21 | `/agent/intent` direct: no token, bad JSON, unknown `type`, bogus session id, 60 KB, 100 KB to 3 MB bodies | `curl` | pass — `403 not_authenticated`, `400 bad_request`, `400 envelope_malformed`, `403 not_authorized`, `403` (60 KB, session not found), `400 bad_request` (≥ 100 KB; `MAX_BODY_BYTES = 64 KiB`, `intents.ts:25`). Note `403` for a missing token, not `401`: **F-chat-16** |
| C22 | cancel mid-turn | Fake | pass — capability line "Failed: the capability invocation was cancelled while contacting the gateway", "You stopped the agent." |
| C23 | cancel mid-turn | Codex | pass — durable `turn.cancelled`, no late output after the stop |
| C24 | cancel twice (double click) | Both | note — two cancel intents, two identical notices: **F-chat-14** |
| C25 | cancel, then send | Both | note — works (new session), but the old session and its Codex App Server stay: **F-chat-7** |
| C26 | send while a turn is running | Fake | note — accepted; two turns ran at once and their lines interleave |
| C27 | send while a turn is running | Codex | fail — the second message breaks the first turn's tool call: **F-chat-8** |
| C28 | close the panel mid-turn, reopen | Fake | pass — the result is there; the toggle carries state, the hook lives in `Editor` |
| C29 | an unsent draft across close/reopen | Fake | note — the draft is lost (ux, below) |
| C30 | reload after a finished turn: is the history what was shown live? | Codex | pass with the known gap — same lines, same order, no duplicates, no missing; the person's own lines are gone (`[person,capability,result,agent]` live, `[capability,result,agent]` replayed) |
| C31 | reload mid-turn | Codex | note — a half sentence appears first: **F-chat-13** |
| C32 | reload during the *session-start* window (first message after a Stop) | Codex | fail — the message is silently lost and an empty session is left: **F-chat-1** (second form) |
| C33 | leave the design and come back | Both | note — documented precisely under *Leave and come back* below |
| C34 | two tabs of A, same design, chat open in both | Fake | pass — tab 2 mirrors the whole turn live |
| C35 | two tabs of A on *different* designs (`Launch cover`, `Square post`), Codex streaming | Codex | pass — both show the same conversation and the delta row |
| C36 | stop in tab 2 while tab 1's turn runs | Fake | fail — tab 1 stays "working": **F-chat-3** |
| C37 | `kill -9` the driver mid-turn | Fake | fail — **F-chat-4** |
| C38 | `kill -9` the driver mid-turn | Codex | fail — **F-chat-4**; the three Codex App Servers of the instance died with it, no orphans (pass) |
| C39 | start a new Runtime (re-pair) after C37 | Fake | fail — **F-chat-5** |
| C40 | `kill -9` the sidecar mid-turn, wait, restart | Fake | pass for the status line (within ~10 s "Can't reach the agent service. Retrying.", sends say "Forma's agent service can't be reached."); the restart itself is **F-chat-6** |
| C41 | `kill -9` forma-server mid-turn, restart | Fake | pass — "Can't reach the agent service. Retrying.", the running capability ends "Failed: Forma could not be reached", chat recovers on its own |
| C42 | browser offline (`set offline on`) with the chat open, then online | Both | pass — status flips, a send says "Forma's agent service can't be reached.", recovers to "Connected · live" |
| C43 | the UI stream blocked (`network route **/pons/v1/ui/** --abort`) | Fake | pass — status reads "Connected" without "· live", the reply still arrives in under a second by polling; on removing the route "· live" returns |
| C44 | status line over a minute (Codex instance) | Codex | note — "live" flaps every few seconds: **F-chat-9** |
| C45 | person B, no device paired | Fake-instance | pass — see *States* |
| C46 | durable events vs live, two tabs, reload ×6: duplicated or missing lines | Both | pass — none seen |
| C47 | the Runtime offline, the person tries to send | Fake | pass — "Your Runtime is offline. Start it on your device, then try again." |

## Findings

## F-chat-1 — The first message of a window, and the whole transcript, vanish when the window starts its first session

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 3, FakeProvider and real Codex (both reproduce)

**Steps to reproduce**
1. Sign in as A, open `ALPHA Launch cover`, open the Agent panel, send any message. (A window that has
   not started a session itself — a fresh load of the page — is enough.)
2. Variant: reload, open the chat (it replays the last conversation), send a long Fake turn
   (`Validate the long design.`), press **Stop**, then send `Read the manual first.`

**Expected** — the person's line stays; an earlier conversation does not disappear because one
message was sent (§ the chat keeps "what the person said", `chat.ts` `Line` comment).

**Actual** — (1) the agent's answer arrives but **no "You" line** is ever shown for that message
(`li` kinds `capability,result,capability,result,agent`). The second message in the same session is
shown. (2) In the variant the replayed transcript of 7 lines — including the person's lines and the
"You stopped the agent." notice — is replaced by the 3 lines of the new session.

**Why (from the code, not measured)** — in `chat.ts` `applyEvent`, `session.created` for a session
this window does not own resets the state (`state.owned === null`). The window only learns it owns the
session when `agent.start` answers and `started` is dispatched, and the live stream usually delivers
`session.created` first, wiping the `person` line that `send` had just echoed (and, in the variant,
the replay). From then on `owned` is set and the later messages are fine, which matches what was seen.

**Second form (Codex)** — a message sent in the *starting* state is kept only in a `useRef`
(`queued`). Reloading within about a second of pressing Send loses it without a word: the session is
created and started, no turn ever follows (`ses_… session.created session.started` and nothing else in
the durable page), and after the reload the chat shows the empty-state text, because the latest
session is the empty one and "the latest session replaces an earlier one".

**Evidence** — [before](evidence/chat/F-chat-1-before.png) (after Stop: 7 lines) and
[after](evidence/chat/F-chat-1-after.png) (one more message: 3 lines, no "You"); the event page
excerpt for the second form was read with `curl` against `/pons/v1/events`.

## F-chat-2 — The agent's edit is real in Forma but the open editor never learns of it

- **Severity:** high
- **Status:** open
- **Instance / provider:** instance 3, both (Fake `Raise the headline to 96pt.`; Codex tagline and footer edits)

**Steps to reproduce**
1. Sign in as A, open `ALPHA Launch cover` (header `v1`), open the chat.
2. Codex: `In design <id>, change the tagline text to "Edited by Codex".` (or Fake: `Raise the headline to 96pt.`)
3. Wait for the agent's "…validation passed". Look at the header and the canvas. Wait a minute.

**Expected** — "confirm the edit is real in the canvas and in Forma (new version)": a person who asks
the agent for a change sees it where they are looking (plan §28: the editor and the agent share one
authoring path; the chat sits "beside the work it acts on").

**Actual** — Forma has v2, `actor_type: "agent"` (`/designs/{id}/history`), the tagline reads "Edited by
Codex" there. The editor header still says **v1**, the canvas still draws "A tagline that belongs…", the
**Undo** button is disabled (it only knows v1), and none of this changes with time. A reload, or
leaving the design and re-opening it, shows v2. If the person edits something first, the edit is
refused with the banner "This design changed elsewhere. You were editing v1; it is now v2. Your last
change was not applied." (the safe outcome: nothing is overwritten, *Reload* fixes it). So the data
is never corrupted, but the product tells the person the agent's work "didn't happen" until they trip
over it. `queries.ts` sets `staleTime: Infinity` for the design and nothing invalidates it when a
`design.applyOperations` capability completes in the chat.

**Evidence** — [Codex](evidence/chat/F-chat-2-codex-edit-canvas-stale.png): the transcript says the
tagline now reads "Edited by Codex" next to a canvas that still shows the old tagline and `v1`;
[the conflict banner](evidence/chat/F-chat-2-stale-canvas-conflict.png).

## F-chat-3 — Stopping from one tab leaves another tab saying "The agent is working…"

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 3, FakeProvider

**Steps to reproduce**
1. Two tabs of A on the same design, chat open in both (tab 2 replays the session).
2. Tab 1: `Validate the long design.` Within a second, tab 2: **Stop**.

**Expected** — both windows end the turn.

**Actual** — tab 2 shows "You stopped the agent." Tab 1 shows the capability failed, but the row "The
agent is working…" and the **Stop** button stay (checked 6 s and 16 s later). The Runtime writes no terminal
event after a cancel (the Pontis finding in the sidecar README), and only the pressing window ends the
turn locally; it clears only when the next turn completes. Related: the pressing window also marks the
*session* ended, although the Runtime's session is still alive (the next message there starts a second
session — see F-chat-7).

**Evidence** — [screenshot of tab 1](evidence/chat/F-chat-3-other-tab-stuck-working.png) after the stop;
the durable page showed `turn.started, capability.requested, capability.failed` and no `turn.cancelled`.

## F-chat-4 — Killing the Runtime mid-turn leaves "working…" forever, even after a reload

- **Severity:** high
- **Status:** open
- **Instance / provider:** instance 3, Fake and Codex (`kill -9` of the driver, which is the Runtime)

**Steps to reproduce**
1. Send a message and, a second later, `kill -9` the driver (pid in `/tmp/forma-browser-stack/3/state.json`).
2. Watch the chat; press **Stop**; send another message; reload and re-open the chat.

**Expected** — a turn whose Runtime is gone ends, visibly, as failed; Stop or a reload gets the person
a usable chat once a Runtime is back.

**Actual** — the status line correctly turns to "Forma local run is offline. Start the Runtime on it
to chat." within ~10 s. But the capability line stays "design.validate … · running" and "The agent is
working…" stays; **Stop** answers "Your Runtime is offline. Start it on your device, then try again." and
changes nothing; a send says the same. After a **reload** the transcript is rebuilt from the durable
page, whose last event is `turn.started` (no terminal event ever arrives), so the chat *again* shows a
running capability and "working…" — the state is now permanent.

**Evidence** — [screenshot](evidence/chat/F-chat-4-driver-killed-stuck-working.png) (offline status
above a still-running capability). With Codex the three App Servers of the instance died with the driver
(`ps`: none left), so no orphan processes.

## F-chat-5 — After a Runtime restart the chat is wedged on the dead session, and cannot start another

- **Severity:** high
- **Status:** open
- **Instance / provider:** instance 3, FakeProvider (continuation of F-chat-4)

**Steps to reproduce**
1. Do F-chat-4, then start a new driver for A (fresh pairing code from `POST /pons/v1/pairing/codes`,
   `tail -f /dev/null | forma-pontis-test-driver` so its stdin stays open). The status line becomes
   "Forma local run · online · Runtime test-driver".
2. Send a message. Press **Stop**. Reload and try again.

**Expected** — the chat notices its session is gone and starts a new one (as it does for a stopped one),
or says so.

**Actual** — every send answers "This session isn't allowed to do that." (sidecar log:
`intent session.input delivered:false authorized:false`) and shows the person's line plus that error;
**Stop** says the same; the row "The agent is working…" stays. A reload does not help: the chat
replays the same latest session from the durable page and keeps using it. The only way out found was
out of band — `curl -X POST /agent/intent` with `session.start`, after which the open window picked the
new session up. A person has no such way: after a laptop sleep/reboot (Runtime restart) the chat is
unusable until something else starts a session.

**Evidence** — [screenshot](evidence/chat/F-chat-5-session-wedged-after-runtime-restart.png) (the
error under the person's line, Runtime online); log excerpt from `logs/sidecar.log` above.

## F-chat-6 — Restarting the sidecar forgets every paired device and the whole history

- **Severity:** medium (high for a deployment: a sidecar restart = every person re-pairs)
- **Status:** fixed (dff7168) — with F-10-1
- **Instance / provider:** instance 3, FakeProvider

**Steps to reproduce**
1. Chat open, a turn running; `kill -9` the sidecar (pid in `state.json`); wait; start it again with
   the same environment.
2. `curl` `/pons/v1/devices` and `/pons/v1/events?scope=application_context&applicationContextId=…&after=0`.
   Reload the tab and chat.

**Expected** — pairings and history survive a restart of the process that owns them, or the person is
told to pair again.

**Actual** — `sidecar.ts:90` builds `new ControlPlane({ applicationId, audience })` with no store, so
everything is in memory: `devices: []` and `{"events":[],"head":0}` after the restart. The chat says
"No device is paired. Pair one from the Agents page." — truthful, but it is the same person, who paired
a minute ago. The *open* tab keeps its dead session id (same wedge as F-chat-5: "This session isn't
allowed to do that.") until it is reloaded; a reload then starts clean. The old driver process stayed up
but its device never reappeared in the list (I did not look into why); a new pairing code and a new
driver were needed. The status line
itself behaved: about 10 s after the kill it read "Can't reach the agent service. Retrying." and a send
said "Forma's agent service can't be reached."

**Evidence** — [screenshot while the sidecar was down](evidence/chat/F-chat-6-sidecar-down.png); the
two `curl` answers above (read in the terminal, not in the browser).

**Fixed** with [F-10-1](10-pairing-devices.md): the history is the durable event store, now on disk.
After a restart on the same data directory the chat's transcript came back on reload (instance 9).
What is still not recovered: the *person's own* typed message, which is not a durable event, and the
open tab's dead session id (F-chat-5, Task A).

## F-chat-7 — Stop abandons the session instead of ending it: a Codex App Server per Stop stays alive

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 3, real Codex

**Steps to reproduce**
1. Codex instance, chat open: send a turn, **Stop**, send another message (a new session starts).
2. Repeat once. Count the instance's App Servers: `ps -eo cmd | grep 'bin/codex app-server' | grep pontis-bt-chat`.

**Expected** — Stop ends the turn (or the session it leaves is closed when the window moves on); the
Runtime holds one App Server per person.

**Actual** — the durable page shows the stopped session ends with `turn.cancelled` and never
`session.cancelled/completed`; the chat marks the session ended locally and starts a new one for the
next message, so the old one keeps its App Server (one Codex process tree each, "Each chat session
spawns its own App Server"). After two Stop-then-send cycles: 3 live App Servers for one person, 2
sessions left open in the durable page. They die only with the Runtime.

**Evidence** — process listing in the terminal (pids of `…/codex-linux-x64/…/bin/codex app-server` with
the instance's bridge in their `--config`); durable page read with `curl`.

## F-chat-8 — Sending while a Codex turn runs breaks that turn's tool call

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 3, real Codex

**Steps to reproduce**
1. `Read design <id>, then describe its headline in one sentence.`, then 1.5 s later in the same chat
   `Also reply with the single word: second`.

**Expected** — either the second message waits for the first turn, or the chat does not offer to send;
or the first turn is completed. Nothing in the chat says a message sent while the agent is working is
special.

**Actual** — both sends are accepted (Send stays enabled while "working"). Codex ended with one turn
holding both messages and answered: "I couldn't read the design because Forma returned “requires an
active provider turn.” Please reopen the request in an active Forma turn." The `design.get` the first
message needed failed, the word "second" was never produced, and the error text speaks of provider
internals. With FakeProvider the same action started two turns at once whose lines interleave in the
transcript.

**Evidence** — chat transcript of the Codex run; durable page for the session:
`turn.started agent.output.completed turn.completed turn.started agent.output.completed
agent.output.completed turn.completed`.

## F-chat-9 — The UI stream ends about two seconds after it opens, so "live" flaps constantly (Codex instance)

- **Severity:** low
- **Status:** open (cause not found; not measured on the Fake instance, where the status never flipped
  in ~20 looks)
- **Instance / provider:** instance 3, Codex instance (the stream belongs to the sidecar, not to the provider)

**Steps to reproduce**
1. Codex instance, chat open. Read the status line every 5 s for a minute.
2. In the page: `performance.getEntriesByType('resource').filter(e=>e.name.includes('ui/stream'))`.

**Expected** — "Connected · live" holds; the stream lasts (it sends a keep-alive).

**Actual** — the line alternates "Connected" / "Connected · live" every few seconds. The page's resource
list shows a new `GET /pons/v1/ui/stream` every ~4 s, each lasting ~2 s. Measured outside the browser: a
Node client straight to the sidecar (`127.0.0.1:4831`, with the same origin/host headers) was closed by
the server after 0.86 s and 2.0 s on two tries (no other tab open for A at the time). So deltas that
land in the gaps are lost (the reload mid-turn case, F-chat-13) and the browser reconnects forever. The
final text is never lost: the polled page carries it.

**Evidence** — resource timings `[[0,1],[3,2],[7,2],[12,2],[16,2],…]` (start s, duration s) and the node
client's `ended after 0.862` / `2.026`, from the terminal.

## F-chat-10 — A message over 64 KiB gets "The agent service answered something unexpected."

- **Severity:** low
- **Status:** open
- **Instance / provider:** instance 3, FakeProvider

**Steps to reproduce** — send 70,000 characters (the box has no limit and accepts the paste).

**Expected** — a limit stated in words ("That message is too long (max 64 KB)"), the draft kept.

**Actual** — the intent route has `MAX_BODY_BYTES = 64 * 1024` (`intents.ts:25`) and answers `400
{"error":"bad_request"}`; the chat's client maps unknown codes to "unexpected": "The agent service
answered something unexpected." The draft is cleared on Send and the 70k line stays in the transcript.
(50,000 characters is fine. Multibyte text hits the limit sooner than the box would suggest.)

**Evidence** — chat tail read from the DOM; the `curl` series in C21.

## F-chat-11 — The agent is not told which design is open (confirms F-00-4); naming it is the recovery

- **Severity:** medium
- **Status:** open (the step 11 finding 5 question, answered)
- **Instance / provider:** instance 3, real Codex

**Steps to reproduce**
1. Open `ALPHA Launch cover`, chat, send `What layers are on this page? Answer in one short sentence.`

**Expected** — the agent reads the page the person is looking at.

**Actual** — Codex called `design.get` with an **empty** id ("Failed: no Boundary F operation matches GET
/api/v1/pontis/designs/") and answered "I can't tell which page you mean because no design is open or
identified." Nothing in the chat says to name the design. What works: giving the id works, and so does
giving the *name* — `Make the headline of the design named "ALPHA Launch cover" say HELLO.` made Codex
find the design through `project.list`/`design.get`, edit it and validate (Forma v4). Two consequences: a
vague "this design" fails, and the same account with two designs of one name would be ambiguous. The chat
panel shows the person no indication of which design the agent would act on. The chat also shows the
same conversation in every design of the account (see *Leave and come back*).

**Evidence** — the transcript of the two runs.

## F-chat-12 — A right-to-left override in a message is kept and rendered as typed

- **Severity:** low
- **Status:** open
- **Instance / provider:** instance 3, FakeProvider

**Steps to reproduce** — send `…‮rtl-override‬…`.

**Expected** — bidi control characters are stripped or shown (a person's own message, so the harm is
small; the agent's output and a replayed history are the worrying direction).

**Actual** — U+202E reaches the DOM unchanged (`ESC`, `NUL` and the other control characters were
accepted too and shown stripped). Not exploitable here — React renders text — but the transcript is the
one place where text from a model is shown next to buttons (the approval card), so the same applies to
agent text.

**Evidence** — chat text read from the DOM.

## F-chat-13 — After a reload mid-turn the reply starts in the middle of a word

- **Severity:** low
- **Status:** open
- **Instance / provider:** instance 3, real Codex

**Steps to reproduce** — send a turn that streams (`List 15 distinct color names…`), reload right
away, re-open design and chat within ~3 s.

**Expected** — nothing, until the final text, or the text from its start.

**Actual** — a streaming row appears with a fragment such as "veys depth, richness, and understated
elega…" (the delta stream joined mid-sentence; deltas are never stored), no "working…" row is shown
(the streaming row hides it), and about 5 s later the final text replaces it. Cosmetic, a consequence of
F-chat-9.

**Evidence** — sampled from the DOM in the terminal.

## F-chat-14 — Double Stop sends two cancels and writes two notices

- **Severity:** low
- **Status:** open
- **Instance / provider:** instance 3, Fake and Codex

**Steps to reproduce** — press **Stop** twice quickly (the button stays until the first answer returns).

**Actual** — two `session.cancel` intents (`delivered:true` both) and two identical "You stopped the
agent." lines.

**Evidence** — `logs/sidecar.log` (two `intent session.cancel` lines at the same millisecond).

## F-chat-15 — Agent text is never formatted

- **Severity:** info (ux)
- **Status:** open

Agent replies are shown as plain text with `whitespace-pre-wrap`. Markdown from the model (`**bold**`,
lists with `-`) shows its markers. Safe and consistent, and the right side of the trade for now.

## F-chat-16 — A missing token on `/agent/intent` answers 403, not 401

- **Severity:** info
- **Status:** open (wording of the contract, not behaviour)

`POST /agent/intent` with no `Authorization` → `403 {"error":"not_authenticated"}`. Clients, the chat
included, map the body, so nothing breaks; a generic HTTP client would expect `401`.

## Leave and come back — what is kept and what is lost, precisely

The brief called this known; this is what the browser shows (Codex and Fake agree).

- The URL is `/` everywhere: the editor has no route per design, so a **reload** anywhere lands on the
  project list, and the chat panel is closed (it is closed by default, in `Editor`).
- **← Projects** returns to the project list, not to the project: three clicks to get back to a design.
- Leaving a design and re-opening it (or another design of the same account) → panel closed; open
  it again and the **same conversation** is there: the transcript belongs to the account's application
  context, not to a design. Only the **latest session**, and only the **agent's side** — "You" lines are
  not durable, so a reload or a leave/return drops them (a "You" line exists only in the window that
  sent it).
- Working/ended state is rebuilt from events, so it is right when the Runtime wrote a terminal event and
  stuck when it did not (F-chat-3, F-chat-4).
- An unsent draft is gone when the panel is closed (the draft lives in the panel component).
- A second tab on the same account mirrors the whole conversation live, including deltas, while it has
  not started a session of its own.

## States — what the chat looks like

- **No device paired** (person B): status "No device is paired. Pair one from the Agents page." with a
  link; the box's placeholder reads "No device is paired yet."; Send is disabled and the box still
  accepts typing. Clear and actionable. [screenshot](evidence/chat/ux-no-device-paired.png)
- **Runtime offline**: "Forma local run is offline. Start the Runtime on it to chat." Fine. But with an
  old live session it still lets the person type and answers per message with an error line.
- **Unreachable**: "Can't reach the agent service. Retrying."; sends say "Forma's agent service can't
  be reached." Both honest. No spinner or time of the next attempt.
- **Connecting**: never seen long enough to read.
- **Discoverability of the toggle**: **Agent** is a plain grey button in the header between *Undo* and
  *Properties*, the Properties button is orange from the start and draws the eye; there is no hint on
  first open, no badge except for approvals. [screenshot](evidence/chat/ux-chat-toggle-closed.png) (ux)
- **Error wording**: the chat's own errors are plain. The ones passed through from lower down are not:
  "no Boundary F operation matches GET /api/v1/pontis/designs/" (F-chat-11), "requires an active provider
  turn" (F-chat-8), "The agent service answered something unexpected." (F-chat-10), and for FakeProvider
  "The agent's turn failed: no fixture is scripted for input …" (harness only).
- Codex replies often arrive as two `AGENT` rows (a "I'll read…" preamble and the final text); that is
  the provider's output, not a duplicate.
- A person who sends a message while the chat shows "working…" gets no signal that the message changes
  the turn (F-chat-8).

## Not done

- A second tab where *both* windows started their own session (each "owns" its session and ignores the
  other's): reasoned from `chat.ts`, not run — they would diverge into two conversations.
- A session that fails at start (`session.failed`): the real Codex here is signed in.
- Approvals, pairing and cross-account isolation belong to the other testers.
- No Codex turn was spent on a 50k message: the limit is the same code path as Fake's.
