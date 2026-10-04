# 00 — Findings from building the browser stack

Found while building [the harness](../pontis-browser-testing.md) and proving it in the Orca browser
(2026-10-03/04, instances 1 and 2, FakeProvider and real Codex 0.158.0 / `gpt-6-luna` / low). Format:
[README](README.md). Each was seen in the browser or in that run's logs.

What the proof run did: signed in as A (and, on the `localhost` origin, as B), opened a design,
opened the chat, sent one short message through Codex and got an answer through `project.list`
(and a second one, which showed F-00-4), saw the device online on `/agents`, ran instances 1 and 2
at the same time with separate data, and confirmed the editor talked only to `127.0.0.1` /
`localhost` hosts. The model recorded in the Codex rollout was `gpt-6-luna`, effort `low`.

## F-00-1 — The Vite dev proxy never forwarded the polled event page, so the chat could not connect

- **Severity:** blocker
- **Status:** fixed (`e466155`)
- **Instance / provider:** instance 1, real Codex (any provider; nothing to do with the model)

**Steps to reproduce**
1. Start an instance, sign in as A, open a design, open the Agent panel.

**Expected** — "Connected · live" once the sidecar is reachable.

**Actual** — "Can't reach the agent service. Retrying." forever, although every request in the network
panel was `200`. `GET /pons/v1/events?scope=application_context&applicationContextId=…&after=0`
returned **`index.html`** (`200 text/html`), so the event reader parsed nothing and reported offline.

**Cause** — Vite tests a proxy key against `req.url` *including the query string*. The key was
anchored with `$` (`…|events|…)$`), so a request with a query missed it and fell through to the SPA
fallback. `proxy.test.ts` tested bare paths only, so it could not see it; Caddy's `path` matcher
never sees a query, so production was never affected (`deploy/caddy/Caddyfile` matchers confirmed to
be path-only; the new test asserts the Caddy side with the query stripped).

**Fix** — the pattern accepts an optional `(\?.*)?` before `$`. `proxy.test.ts` now asserts that the
event URL with a query matches, and that a query cannot smuggle a Runtime route or a near miss
(`/pons/v1/device/x?/pons/v1/events`, `/pons/v1/pairing/claim?x`, `/pons/v1/eventsx?x=1`, …).

**Evidence** — before the fix, `curl` of that URL through the editor's origin returned the HTML shell
instead of `{"protocol":"pons/1.0",…}`; after it the status line read "Connected · live"
([screenshot](evidence/00-harness-codex-chat.png)).

## F-00-2 — The chat could never send to the real Codex: "not signed in on this device"

- **Severity:** blocker
- **Status:** fixed (`76f2fd0`); the cause is on Pontis's side and is recorded as an upstream observation
- **Instance / provider:** instance 1, real Codex

**Steps to reproduce**
1. Start an instance with `--provider codex`, sign in as A, open the Agent panel.

**Expected** — Send enabled for an installed, compatible Codex.

**Actual** — `codex · unknown`, "Unavailable: not signed in on this device.", Send disabled.
`GET /pons/v1/devices` reported `installed: true, compatible: true, authenticated: false,
authMode: "unknown"`.

**Cause** — Pontis's `CodexProvider::probe_detailed` sets `authenticated: false, authMode: unknown`
for *every* installed Codex: App Server has no supported auth-status query, and the code says
`Unknown` is the honest answer. `providerView()` in `apps/web/src/pontis/chat.ts` read anything but
`authenticated === true` as "not signed in". The same would hit any user of a real Pontis Runtime.
Step 11 never saw it: it drove the sidecar's routes and never consulted the device's provider status.

**Fix** — a provider is usable when installed and compatible and (`authenticated === true` or
`authMode === "unknown"`). The status says "Sign-in not verified: this provider can't report it. If it
isn't signed in, the first message will say so." A known-signed-out provider (`not_authenticated`)
and a missing or incompatible one are still refused. A Codex that really is signed out fails at
`session.start` with the `not_authenticated` failure the chat already words (not exercised here: the
owner's Codex is signed in).

**Upstream (Pontis)** — worth raising: the probe could report what it knows (the `auth.json` is
present) or the contract could say that `unknown` is a usable state; today a host must choose.

**Evidence** — `/pons/v1/devices` excerpt above; after the fix the screenshot shows the panel with
the unverified note and a Codex answer.

## F-00-3 — The first session on a brand-new `CODEX_HOME` fails: "timed out waiting for thread/start"

- **Severity:** medium
- **Status:** open (mitigated in the harness; the cause is cold-start time in Codex or the Runtime's timeout)
- **Instance / provider:** instance 1, real Codex, freshly created `CODEX_HOME` (reproduced twice, once per new home)

**Steps to reproduce**
1. Create a new `CODEX_HOME` (`setup-codex` on a machine without `.codex-pinned/home`).
2. Start an instance with `--provider codex` *without* the warm-up (earlier harness revision) and
   send a message from the chat.

**Expected** — the message is answered.

**Actual** — the chat shows "The session failed: Codex App Server timed out waiting for thread/start";
`driver-a.log` has `REFUSED provider_protocol_error Codex App Server timed out waiting for
thread/start`. The next message, in a new session, worked. The home's state databases
(`state_5.sqlite`, …) were created during the failed attempt.

**Mitigation** — on a cold home `start` opens one free warm-up session (no prompt is sent) and retries
until it starts, so a tester's first message is not the one that pays. The warm-up's own cancel logs
`REFUSED provider_cancelled Codex has no active turn to interrupt`, which is expected there.

**Evidence** — `logs/driver-a.log` of the run, quoted above.

## F-00-4 — The chat's agent does not know which design is open, and guesses ids

- **Severity:** medium
- **Status:** fixed (68d861c)
- **Instance / provider:** instance 1, real Codex

**Steps to reproduce**
1. Sign in as A, open "ALPHA Launch cover", open the Agent panel.
2. Send: `In one short sentence, what is the headline text of this design?`

**Expected** — the agent reads the design the person is looking at.

**Actual** — Codex called `project.list` (completed), then `design.get` with an id that does not exist
(`Failed: design 60fae6a7-45cc-4d01-8317-d90baa4eb412 does not exist`), `forma.guide`, and `design.get`
with a second invented id, then answered: "I can't identify the headline yet: I only found the
project, and no design listing is available in this session. Send me the design ID and I can read
its headline."

**Why** — there is no `design.list` capability, and the editor does not hand the open design to the
agent (step 11 finding 5: "step 12 should record whether the chat provides that"). It does not. A
person has to paste an id, or the application context has to carry the open design.

**Evidence** — [screenshot](evidence/00-harness-codex-chat.png) of the transcript.

**Fixed (fix batch 1, Task A)** — see F-chat-11: the editor reports the view with each message and the
sidecar writes the `[forma-view]` line; `design.list` (4793180) answers "what designs are there".

## F-00-5 — Orca's `click` is unreliable on the editor's first interaction after a navigation

- **Severity:** info (harness ergonomics, not a product defect)
- **Status:** open — documented workaround
- **Instance / provider:** any

**Actual** — after `orca goto`/reload, `orca click --element @eN` on the project button returned
`clicked` and changed nothing, sometimes twice running; a click through the DOM
(`orca eval --expression "…button.click()…"`) always took. `orca fill` on the chat box was fine.
Refs also go stale after any re-render, so snapshot again before clicking.

**Workaround** — in [pontis-browser-testing.md](../pontis-browser-testing.md#driving-the-editor-with-orca).

## F-00-6 — The sidecar routes to exactly one online device per person

- **Severity:** info
- **Status:** wontfix (by design: `ambiguous_device`)

A person with two online Runtimes gets `ambiguous_device` from `/agent/intent`, and the chat says it
cannot tell which to use. The harness pairs one device per person; a tester who wants to probe the
ambiguous case can pair a second one with the pairing code on the Agents page and a second driver.

## F-00-7 — Things the fake identity provider does not model

- **Severity:** info
- **Status:** wontfix (scope)

No magic-link sign-in, no OAuth consent page, no OAuth grants (the Agents page's "Connected
applications" list answers empty). The first run showed the Agents page printing "the fake identity
provider has no /auth/v1/user/oauth/grants" until that one route was added; any other Supabase route
the editor starts using will surface the same way, in the page and in `logs/identity.log`.
