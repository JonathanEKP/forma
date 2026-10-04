# Browser findings — summary across the five areas (2026-10-04)

Five testers, each on its own [browser stack](../pontis-browser-testing.md) instance and Orca
browser tab, tried to break the Pontis → Forma connection. Each one's full record (every case run,
passes included, with reproduction steps and evidence) is its area file. This page groups what they
found **by cause**, because many findings are the same defect seen from different sides, and orders
the groups by what to fix first. It interprets; the area files are the evidence.

| Area | File | Cases | Findings | High |
|---|---|---|---|---|
| Harness (B0) | [00-harness.md](00-harness.md) | — | 7 (2 blockers, **fixed**) | 0 |
| T1 Pairing and devices | [10-pairing-devices.md](10-pairing-devices.md) | 36 | 13 | 2 |
| T2 Chat lifecycle | [20-chat-lifecycle.md](20-chat-lifecycle.md) | 47 | 16 | 3 |
| T3 Approvals | [30-approvals.md](30-approvals.md) | ~35 | 9 | 2 |
| T4 UI channel | [40-ui-channel.md](40-ui-channel.md) | 63 | 14 | 3 |
| T5 Security and proxy | [50-security-proxy.md](50-security-proxy.md) | 84 | 9 | 1 |

About 35 real Codex turns (`gpt-6-luna`, low effort) were spent in total; the rest ran on
FakeProvider, curl and process kills. No product code was changed by a tester. The two blockers the
harness found were fixed with the coordinator's approval before testing began (`e466155`,
`76f2fd0`).

## What held

These were attacked directly and did not break:

- **Account isolation.** No path was found from one account to another's designs, devices, events,
  approvals or windows — through the UI, direct calls with the other person's token, forged ids, or
  asking Codex for the other account's design. B's window never moved for A's agent.
- **Approval guarantees.** Approve is exactly-once (double clicks, replays, two tabs); deny and
  cancel leave Forma unchanged; a forged approval for another person's session is refused; Forma's
  own `required_risk` refuses every unapproved removal; a prompt injection planted in a design's
  text did nothing.
- **The proxy surface.** The Runtime routes (`/pons/v1/pairing/claim`, `/pons/v1/device/*`) are not
  reachable through the editor's origin, with or without path tricks.
- **Secrets.** No token, auth header or service credential appeared in any response, SSE frame,
  error body or log of any instance.
- **The normal paths.** Pairing, a chat turn with a real edit, an approval, and all four `ui.*`
  actions work end to end in a real browser on a single tab.

## What to fix, grouped by cause

### 1. The open editor does not show what the agent did — high

[F-chat-2](20-chat-lifecycle.md), [F-30-3](30-approvals.md).
The agent's write is real (new version, actor `agent`), but the editor keeps the old version and
picture; the person's first sign is a `VERSION_CONFLICT` banner on their own next edit. Nothing
in the editor refetches the design when the agent writes (the cause is not yet traced). This is the
most visible defect in normal use.

### 2. A chat whose session dies cannot be recovered — high

[F-chat-4](20-chat-lifecycle.md), [F-chat-5](20-chat-lifecycle.md), [F-10-2](10-pairing-devices.md),
[F-10-4](10-pairing-devices.md), [F-30-4](30-approvals.md), [F-30-5](30-approvals.md).
Kill or restart the Runtime, revoke the device, or let the session's delegation expire (~5 min in
the harness — whether production's lifetime differs is unconfirmed), and the chat stays on the dead
session: "working…" forever, Send disabled, a pending approval card with live buttons, "This session
isn't allowed to do that" — across reloads, and even after pairing a new device. There is no way to
start a new session. One fix probably covers all six: the chat needs to notice a session is
terminal or unreachable and offer a new one.

### 3. The sidecar keeps its state in memory, and restarts badly — high for a deployment

[F-10-1](10-pairing-devices.md), [F-chat-6](20-chat-lifecycle.md), [F-40-10](40-ui-channel.md),
[F-40-4](40-ui-channel.md), [F-40-2](40-ui-channel.md).
A sidecar restart forgets every device, pairing code and conversation (the Control Plane is
in-memory), while the Runtime stays running and is never told. Open editors stay deaf until
reloaded (seen behind Vite; unknown behind Caddy). And a graceful stop never completes while any
editor window has its UI stream open, so every restart waits for the supervisor's kill. Together:
a routine deploy forces every person to re-pair. Persistence of the Control Plane is a Pontis
composition question (it ships an in-memory store), so this group needs a decision, not just a fix.

### 4. Two windows of one person fight over the UI channel — high

[F-40-1](40-ui-channel.md), [F-50-3](50-security-proxy.md), [F-40-15](40-ui-channel.md),
[F-chat-3](20-chat-lifecycle.md); probably [F-chat-9](20-chat-lifecycle.md).
The UI client is registered under `appSessionId = userId:applicationContextId`
(`apps/pontis-sidecar/src/sidecar.ts`), so every window of the same person in the same account
claims the same registration and evicts the other; the evicted one reconnects 2 s later and evicts
back. Actions land on whichever window holds it at that instant and are reported `handled` — the
"success nobody saw" the CLAUDE.md invariant forbids. A second tab signing in as someone else leaves
the first registered as the old person. F-chat-9 (the stream ending ~2 s after opening) matches the
same cycle but was not confirmed to have had a second window. The fix is a per-window client id.

### 5. The sidecar's service credential opens the whole REST API — high (security)

[F-50-1](50-security-proxy.md).
The `forma_svc_` credential is accepted on all of `/api/v1/*`, not only Boundary F, and can mint a
lasting personal access token for any user it can name. Nothing crossed accounts in practice, but
the credential is far wider than its job. Narrowing it to `/api/v1/pontis` and `/api/v1/service/*`
is a forma-server change; it is the one finding with a security fix that does not depend on Pontis.

### 6. The agent does not know which design is open, and cannot find one — medium

[F-00-4](00-harness.md), [F-chat-11](20-chat-lifecycle.md), [F-40-6](40-ui-channel.md),
[F-40-5](40-ui-channel.md).
Step 11's open question, answered in the browser: the agent guesses ids, and naming the design is
the only recovery. `ui.openDesign` is usable only with ids the person pastes, because there is no
way to list designs. This needs the owner's decision on what context the chat sends.

### 7. Smaller defects worth a pass

- **Stop and send.** Stop abandons rather than ends, leaving one Codex App Server alive per Stop
  ([F-chat-7](20-chat-lifecycle.md)); sending while a turn runs breaks that turn's tool call
  ([F-chat-8](20-chat-lifecycle.md)); double Stop sends two cancels
  ([F-chat-14](20-chat-lifecycle.md)); the first message of a window and the transcript vanish when
  its first session starts ([F-chat-1](20-chat-lifecycle.md)).
- **UI channel semantics.** `ui_action_timeout` does not mean the action did not happen
  ([F-40-3](40-ui-channel.md)); `setZoom`/`setPanel` are `handled` with no design showing
  ([F-40-12](40-ui-channel.md)); an expired token keeps an open stream alive
  ([F-50-2](50-security-proxy.md), medium).
- **Edges.** Messages over 64 KiB get a generic error ([F-chat-10](20-chat-lifecycle.md)); a reply
  after a mid-turn reload starts mid-word ([F-chat-13](20-chat-lifecycle.md)); a right-to-left
  override is rendered as typed ([F-chat-12](20-chat-lifecycle.md)); actions mid-drag
  ([F-40-7](40-ui-channel.md)); `selectLayer` in the same tick as `openDesign`
  ([F-40-8](40-ui-channel.md)); duplicate React key warning ([F-50-7](50-security-proxy.md)).
- **Dev server only.** The proxy forwards `..` unnormalised, harmlessly so far
  ([F-50-4](50-security-proxy.md)); Vite's CORS and `/__open-in-editor`
  ([F-50-5](50-security-proxy.md), [F-50-6](50-security-proxy.md)).

## UX notes for the design conversation

Collected from the `ux`-tagged findings, for the owner's UX review rather than as bugs:

- **Approval cards** name neither the layer nor the design, describe a page removal as one element,
  and hide their deadline; a pending approval is easy to miss with the chat closed
  ([F-30-1](30-approvals.md), [F-30-2](30-approvals.md), [F-30-7](30-approvals.md)). "Hide it" and
  "blank it" go through without approval by design ([F-30-8](30-approvals.md)).
- **Pairing** instructions stop one step early and use words a first-time person does not have;
  pairing a second machine silently breaks the chat; the device list never prunes and cannot tell
  two machines apart; the Devices panel is a snapshot ([F-10-5](10-pairing-devices.md),
  [F-10-6](10-pairing-devices.md), [F-10-9](10-pairing-devices.md), [F-10-3](10-pairing-devices.md)).
- **The agent moving the view** has no cue, and a failed action's card contradicts itself
  ([F-40-14](40-ui-channel.md)).
- **Agent text is never formatted** ([F-chat-15](20-chat-lifecycle.md)); errors after a revoke give
  three different unhelpful answers ([F-10-4](10-pairing-devices.md)).

## Pontis-side observations (not Forma's to change)

To raise with Pontis later; the Pontis repository was not touched:
the Codex probe never reports authentication ([F-00-2](00-harness.md)); `lastSeenAt` is set on
connect and use, not liveness ([F-10-7](10-pairing-devices.md)); the pairing rate limit shares one
bucket and answers `bad_request` ([F-10-8](10-pairing-devices.md)); refusals answer 403
`origin_not_allowed` whatever the cause ([F-10-10](10-pairing-devices.md),
[F-40-13](40-ui-channel.md)); offline takes 32–48 s to read ([F-10-12](10-pairing-devices.md)); two
Runtimes on one identity displace each other silently ([F-10-13](10-pairing-devices.md)); an
over-size UI body resets the connection instead of `413` ([F-50-9](50-security-proxy.md)); the
Control Plane store is in-memory (group 3).

## Not covered

Production Caddy (F-40-4's behaviour there is unknown); a hidden background tab; membership removal
mid-stream; approval after a driver restart and a long-idle Codex approval; delegation-token replay
after revoke. The Orca tabs reloaded themselves about every 12 s while several testers ran
([F-50-8](50-security-proxy.md)), so multi-window cases were partly reproduced by script.
