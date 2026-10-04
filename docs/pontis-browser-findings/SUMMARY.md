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

*Fix batch 1, Task A: fixed* (`56d3e1c`) — the editor follows the agent's completed write.

### 2. A chat whose session dies cannot be recovered — high

[F-chat-4](20-chat-lifecycle.md), [F-chat-5](20-chat-lifecycle.md), [F-10-2](10-pairing-devices.md),
[F-10-4](10-pairing-devices.md), [F-30-4](30-approvals.md), [F-30-5](30-approvals.md).
Kill or restart the Runtime, revoke the device, or let the session's delegation expire (~5 min in
the harness — whether production's lifetime differs is unconfirmed), and the chat stays on the dead
session: "working…" forever, Send disabled, a pending approval card with live buttons, "This session
isn't allowed to do that" — across reloads, and even after pairing a new device. There is no way to
start a new session. One fix probably covers all six: the chat needs to notice a session is
terminal or unreachable and offer a new one.

*Fix batch 1, Task A: fixed on Forma's side* (`56d3e1c`) — the chat recognises each of these, says why and
offers **New session** with the history kept above a divider. The five-minute death itself is a **Pontis
defect** (a stale delegation is refused before it can be refreshed; production has it too) — see the
Pontis-side observations below and [F-30-5](30-approvals.md).

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

*Fix batch 1, Task A: fixed* (`4793180`, `68d861c`) — each message carries a `[forma-view]` line, and
`design.list` is served.

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

To raise with Pontis later; the Pontis repository was not touched.

**The most important one: a session dies about five minutes after its first call, in production too.**
The Runtime refreshes an `AppDelegation` only in `SessionDelegations::bearer_for`, but every capability
call first runs the synchronous `SessionDelegations::validate_session`, which refuses a grant with ≤ 15 s
left as `delegation_expired` — so a stale grant is rejected before the refresh path can replace it, and the
session never recovers (`pontis/crates/runtime/src/delegation.rs`; Fake and Codex paths alike). Found and
reproduced by fix batch 1, Task A — the exact path, the timing and the two candidate fixes are under
[F-30-5](30-approvals.md). Forma now ends such a session legibly and offers a new one; the cause is Pontis's.

The rest: the Codex probe never reports authentication ([F-00-2](00-harness.md)); `lastSeenAt` is set on
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

## Re-test after fix batch 1 (2026-10-04, Task E)

All four tasks (D, C, B, A) were merged, then every finding marked `fixed (…)` was re-run in a real browser on
instance 10 (editor `127.0.0.1:4902`, a second tab at `localhost:4902` for person B), following its own steps, plus
the open findings the batch touched and one normal-path pass per area. Each finding carries a
`re-tested 2026-10-04:` line with its evidence; screenshots are in
[evidence/retest/](pontis-browser-findings/evidence/retest/). 13 real Codex turns (`gpt-6-luna`, low), two of them
refused by the expiry below; everything else FakeProvider, `curl` and process kills. No product code was changed.

| | count | findings |
|---|---|---|
| Re-tested | 26 | 23 marked `fixed`, plus F-chat-3, F-chat-9, F-30-5 |
| **fixed** (confirmed) | 24 | F-00-1, F-00-2, F-00-4, F-10-1, F-10-2, F-10-4, F-chat-1 (first form only), F-chat-2, F-chat-4, F-chat-5, F-chat-6, F-chat-9, F-chat-11, F-30-3, F-30-4, F-40-1, F-40-2, F-40-4, F-40-5, F-40-6, F-40-10, F-40-15, F-50-1, F-50-3 |
| **still open** | 2 | F-30-5 (upstream: the session still dies about five minutes after its first call, on FakeProvider and on Codex), F-chat-3 (upstream; its two-tab setup can no longer be built, so not re-run) |
| **regressed** | 0 | |
| New findings | 3 | **F-30-10** (high), F-10-14 (low), F-chat-17 (info) |

**The worst new one: F-30-10.** On real Codex an expired session leaves *no* notice and no **New session** button.
F-30-5's recovery keys on a failed capability event, which FakeProvider writes; Codex is refused before any
capability call, so the durable page holds only `turn.started … turn.completed`, the agent says "Please reconnect
Forma" and the chat stays on a session that fails every message, across reloads. The expiry that Task A made legible
is legible only on the harness's own provider, which is the one the five-minute defect hits in production. F-10-14:
a revoked device reads "offline" as soon as an older never-revoked device sits beside it (the fix of F-10-4 holds only
for a person's sole device).

What the re-test showed working, beyond the findings:

- **The view line.** Every message from a design carries `[forma-view] project=… design=… name="…" page=… layer=…`
  (read in the driver's log) and the chat shows the chip under the person's text. Switching designs mid-conversation
  (chat kept, panel open) made Codex read the new one. "Open my ALPHA Launch cover design" ran `design.list` then
  `ui.openDesign`; a malformed id is a `validation_error`. With no design open the line is `none`, as specified
  (F-chat-17 records that this also hides the selected project).
- **One window, one registration.** Chat in tab 1 and an idle tab 2 of the same person: the agent's open-design and
  zoom moved only tab 1. A tab given a copy of another's `sessionStorage` (what Duplicate Tab does) mints its own window
  id and starts with an empty chat. No stream flapped in either tab across more than five minutes. A second tab
  signing in as someone else ended the first tab's stream as the old person within seconds.
- **Durable sidecar state and a clean stop.** `SIGTERM` with three windows and two Runtimes attached: exit in 0.05 s.
  Restart on the same data directory: every device (the revoked one too) is still listed, the Runtime is `online`
  again within ~5 s with no re-pair, the transcript is back after a reload. After `SIGKILL` + restart behind Vite the
  open tab re-registered by itself ~32 s later (the watchdog) and a Codex "Set the zoom to 200%" in the same session
  reached it. Not run: the real `pontis` daemon, Caddy.
- **Normal paths.** Pairing (code → claim → listed → revoke), a chat turn, an edit followed by the editor without a
  conflict (fake and Codex), approve and deny on Codex, a stop followed by another message, all four `ui.*` actions
  (`openDesign`, `selectLayer`, `setZoom`, `setPanel`), and the proxy surface (the Runtime routes are not reachable
  through the editor origin; B reads nothing of A's; Codex asked for B's design id gets "does not exist"). The sidecar's
  service credential is refused `403 SERVICE_CREDENTIAL_OUT_OF_SCOPE` on projects, API keys (the PAT mint) and every
  spelling of the path, and accepted on `/api/v1/pontis/*` ([evidence](pontis-browser-findings/evidence/retest/F-50-1-retest-curl.txt)).

Not re-run: F-chat-1's second form (two tries did not lose the message), the `design.validate` Stop-in-flight half of
F-chat-3, "close the bound tab and the agent reports no window" (covered by `ui.test.ts`), Caddy, the real daemon.
