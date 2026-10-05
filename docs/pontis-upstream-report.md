# Pontis issues found while integrating Forma (2026-10-04)

This report is for the Pontis team. Forma (a design runtime) integrates Pontis through an
application-owned sidecar built from `@pontis/server` and `@pontis/control-plane`, a Rust test Runtime
linking the Pontis runtime crates, and a real `pontisd` / Codex 0.158.0 (`capability_only`). Five agents
then tried to break the whole chain in a real browser. Most of what they found was Forma's and is fixed.
**What is below is what we believe is Pontis's.** The Pontis repository was not modified.

Each item gives the observed behaviour, where we think the cause is, and a suggested fix. File paths
are into the Pontis repository unless they start with `forma:`. Evidence (screenshots, event excerpts,
reproduction steps) is in Forma's findings files: `forma:docs/pontis-browser-findings/`.

| # | Issue | Severity for a host | Finding |
|---|---|---|---|
| 1 | A session dies about 5 minutes after its first capability call: the delegation is refused before it can be refreshed | **high — affects production** | F-30-5, F-30-10 |
| 2 | No terminal event is written after `session.cancel` | medium | F-chat-3 |
| 3 | The Codex probe never reports sign-in state | medium | F-00-2 |
| 4 | Every refusal on the control-plane and UI routes is `403 origin_not_allowed` | low | F-10-10, F-40-13 |
| 5 | The pairing rate limit shares one bucket per address and answers `bad_request` | low | F-10-8 |
| 6 | Two Runtimes on one device identity displace each other every ~5 s, silently | low | F-10-13 |
| 7 | `lastSeenAt` is the last connect or use, not the last sign of life | low | F-10-7 |
| 8 | A frozen Runtime reads `online` for 32–48 s | info | F-10-12 |
| 9 | An oversize `/ui/actions` body resets the connection instead of answering `413` | low | F-50-9 |
| 10 | A polyglot host has to write Rust to use FakeProvider | info | sidecar README |
| 11 | A Runtime-refused approval resolve still returns `delivered: true` to the host | **low** | F-30-6 |
| 12 | No per-session stop intent: a host cannot end one session without stopping its Device | low | S12 |

---

## 1. A session dies ~5 minutes after its first call — the delegation is refused before it can be refreshed

**Severity: high.** Every session, on every provider, becomes unusable about five minutes after it
first calls a capability. The real `pontisd` Runtime shares the code path, so this is not specific to
our test driver.

**Observed.** A session made a capability call at 06:58:59 (the Runtime obtains its `AppDelegation`,
`APP_DELEGATION_TTL_MS = 5 min`). The same call at 07:04:22 failed with `delegation_expired`, and
`capability.requested` and `capability.failed` carry the **same millisecond**: the call was refused
inside the Runtime, before any request reached the host's gateway. The same happened on Codex
(grant ≈03:02:29, last success 03:06:54, first failure 03:08:13). After that, every call in the session
fails, including the one that would raise an approval.

**Cause, as we read it.** In `crates/runtime/src/delegation.rs`, the "refresh 15 s early" logic
(`DELEGATION_REFRESH_MARGIN`) lives only in `SessionDelegations::bearer_for`, which is reached when a
call goes out to the gateway. But every call first passes the synchronous
`SessionDelegations::validate_session`, through `RemoteAppDelegation::validate_session` on the
FakeProvider path and `McpSessionBinding::validate_authority` in `mcp.rs` on the Codex path. That
check returns `DelegationExpired` as soon as the held grant is no longer `is_live`, which happens with
15 s or less left. So once a grant goes stale, the call is refused before `bearer_for` can replace it,
and the session never recovers.

**Suggested fix**, either of:
1. `validate_session` treats a stale-but-refreshable grant as valid and lets the call proceed to
   `bearer_for`, which refreshes it; or
2. the Runtime refreshes proactively at TTL − margin for every live session.

**A second effect on Codex (F-30-10).** On the Codex path the refusal leaves **no durable event at
all**: the turn shows only `turn.started`, `agent.output.completed` and `turn.completed`, and the model
tells the person "the application delegation has expired, please reconnect". A host therefore cannot
detect the expiry and offer a new session. On FakeProvider a `capability.failed` with
`delegation_expired` is written, and Forma shows "session expired" with a New session button. Fixing
the refresh removes both. Independently, a refused call writing a `capability.failed` (or a
session-level event) on every provider path would let hosts handle any future expiry the same way.

**Evidence:** `forma:docs/pontis-browser-findings/30-approvals.md` (F-30-5, F-30-10) and
`evidence/retest/F-30-5-*`, `F-30-10-*`.

## 2. No terminal event after `session.cancel`

**Severity: medium.** After a cancel, the Runtime triggers its own cancellation and then the
provider's. With FakeProvider the turn ends and drops its result channel;
`submit_capability_result(...).await?` then fails, and `send_input` returns before reading the
provider's `turn.cancelled`. So no terminal durable event is written, and the error reaches only the
`RemoteAgent` observer (`provider_exited`). It happens on every run.

**Effect on a host.** Only the window that pressed Stop knows the turn ended. Any other window
following the same session through the durable stream keeps showing "working…" until the next turn
completes. Forma clears it when the cancelled call fails with `approval_cancelled`, but a cancel with
no call in flight leaves nothing to go on.

**Suggested fix:** always write a terminal event (`turn.cancelled`) for a cancelled turn, whether or
not a capability call was in flight.

**Task G2 approval-pending sequence (FakeProvider):** the host sees `approval.requested` for
`design.applyOperations`; `session.cancel` returns `delivered: true`; the Runtime publishes
`approval.cancelled`, then `capability.failed(approval_cancelled)`. No `turn.cancelled` or
`turn.completed` is written for that turn, and the test Runtime reports `provider_exited`. The host
then sends `approval.resolve(approveOnce)` and again receives `delivered: true`; no
`REFUSED approval_expired` diagnostic follows. The design stays readable at its original version
and Boundary F records zero `design.applyOperations` calls. This passes G2's no-mutation requirement
and shows the same late-resolution response ambiguity described in item 11. Sanitized evidence:
[`pv1-s6.json`](evidence/step12/pv1-s6.json).

**Evidence:** `forma:docs/pontis-browser-findings/20-chat-lifecycle.md` (F-chat-3) and the sidecar
README section *Cancelling mid-capability loses the turn's end*.

## 3. The Codex probe never reports sign-in state

**Severity: medium.** `CodexProvider::probe_detailed` reports `authenticated: false, authMode: unknown`
for every installed, compatible Codex (`crates/providers/codex/src/lib.rs`, around the comment "App
Server deliberately does not expose a supported auth-status query… Unknown is honest").

**Effect on a host.** A host that reads `authenticated` literally, as Forma first did, shows "not
signed in" and disables chat for every Codex user. Forma now treats `unknown` as usable ("sign-in not
verified"), so a signed-out Codex fails at `session.start` instead. Every host has to rediscover this.

**Suggested fix**, either of:
- report what can be known (for example that the credential file is present) as a distinct state; or
- document in the provider contract that `authMode: unknown` with `authenticated: false` means
  "cannot be checked", not "signed out".

**Evidence:** `forma:docs/pontis-browser-findings/00-harness.md` (F-00-2).

## 4. Every refusal is `403 origin_not_allowed`

**Severity: low.** On the control-plane routes and on `/pons/v1/ui/*` (`packages/sdk-server/src/ui-transport.ts`),
all of these answer `403 origin_not_allowed`, with the correct `Origin` and `Host`:
- no bearer, an empty or garbage bearer, a broken signature, `alg: none`, the `Basic` scheme;
- a valid token presented with someone else's `clientId`.

An unknown or foreign device id on revoke is `403 device_not_found`.

**Effect.** Nothing leaks. But an expired session is reported as an *origin* problem: the host cannot
say "sign in again", operators cannot tell a misconfigured origin from an expired token, and a UI
stream retries every 2 s forever behind the wrong message.

**Suggested fix:** keep the non-oracle property, but use distinct codes: `401 unauthenticated` for a
missing, invalid or expired token, `403 origin_not_allowed` only for the origin check, and
`403 client_not_yours` for someone else's `clientId`.

**Evidence:** `10-pairing-devices.md` (F-10-10), `40-ui-channel.md` (F-40-13).

## 5. The pairing rate limit shares one bucket per address and answers `bad_request`

**Severity: low.** `services/control-plane/src/http.ts` keys the limiter
(`RATE_LIMIT_ATTEMPTS = 20` per 60 s) on `request.socket.remoteAddress`, shared by claim, challenge
and auth. After 20 bad claims, a valid claim and the challenge route both get
`429 {"error":{"code":"bad_request"}}` for the rest of the window. A 429 does not consume the code.

**Effect.** Behind a proxy, or on loopback, every Runtime and every attacker share one bucket. So
someone guessing codes locks out the person pairing, and anything needing a challenge, such as the
15-minute credential refresh (read from the code, not exercised). The code is the same one a
malformed body gets.

**Suggested fix:**
- a distinct `rate_limited` code with a retry-after;
- a bucket that a deployment behind a proxy can key on something other than the socket address;
- and possibly a separate bucket for credential refresh.

**Evidence:** `10-pairing-devices.md` (F-10-8).

## 6. Two Runtimes on one device identity displace each other silently

**Severity: low.** This happens when two `pontis run --foreground` processes use copies of the same
`--data-dir`: a restored backup, a cloned VM, or one machine used under two accounts. Both start, and
neither logs anything beyond `runtime.started`. They take the device socket from each other every ~5 s,
the device reads a steady `online`, and a turn is dropped whenever the other one takes over. Revoking
the device stops both cleanly.

**Suggested fix:** the gateway refuses a second concurrent connection for one device (or tells the
displaced one why), and the displaced Runtime says so to its operator.

**Evidence:** `10-pairing-devices.md` (F-10-13).

## 7. `lastSeenAt` is not "last seen"

**Severity: low.** `lastSeenAt` is written on connection and on credential use. So an online device
shows the time it connected, and a device that died shows its connect time, not when it went away.

**Suggested fix:** update the timestamp on liveness, or rename the field to what it is
(`lastConnectedAt`).

**Evidence:** `10-pairing-devices.md` (F-10-7).

## 8. A frozen Runtime reads `online` for 32–48 s

**Info.** A Runtime that stops answering without closing its socket (a closed laptop lid, dropped
Wi-Fi, `SIGSTOP`) read `online: true` for 31.7 s and 48.0 s in two runs. During that window,
`session.input` answers `{"delivered": true}`, which only means the input was written to the socket,
and the turn never completes. That is the realtime gateway's liveness window. It's not a defect by
itself, but a host cannot tell "delivered" from "will run".

**Evidence:** `10-pairing-devices.md` (F-10-12).

## 9. Oversize `/ui/actions` body resets the connection

**Severity: low.** `POST /pons/v1/ui/actions` with about 39 KB of action names (the cap is 16 KiB)
closes the connection with no response, although `ui-transport.ts` has a `413 body_too_large` branch.
Below the cap, the number and length of action names are not limited.

**Suggested fix:** answer the `413` the code intends, and bound the action count and name length.

**Evidence:** `50-security-proxy.md` (F-50-9).

## 10. A polyglot host has to write Rust to use FakeProvider

**Info.** ADR-023 says a non-Rust host integrates through a TypeScript sidecar. But the deterministic
provider is the Rust crate `pontis-provider-fake`, which `pontisd` does not register. So exercising a
sidecar against it means linking five runtime crates in a host-written Rust driver, as Forma's
`apps/pontis-sidecar/test-driver` does. Pontis's own `polyglot-host-fixture` is the precedent,
which suggests a reusable driver belongs on Pontis's side for every polyglot host.

## 11. A Runtime-refused approval resolve still returns `delivered: true` to the host

**Severity: low.** Replaying an approve or deny for a used, cancelled or expired approval receives
HTTP 200 `{"delivered":true}` from `/agent/intent`, even when the Runtime refuses it as
`approval_expired` and leaves the approval state unchanged. The only refusal trace is the test
Runtime's observer/log; the response gives the host no resolve outcome.

**Cause, as we read it.** `DeviceGateway.sendIntentForContext` reports only `{authorized, delivered}`
to its caller. The Runtime refusal is reported to `IntentObserver`/logs, but no outcome is written
back through the protocol or durable event stream for the host to observe.

**Suggested fix:** report the Runtime's resolve outcome back to the host, including the approval id
and refusal code, so a caller can distinguish a resolve accepted by the Runtime from one that was
merely written to the Device socket.

**Additional expiry sequence, reproduced on FakeProvider and Codex 0.158.0:** the host receives
`approval.requested`; it waits for `approval.resolved` with `status=expired`; the Runtime emits
`capability.failed` with `approval_expired`, then `turn.completed`; the host sends a late
`approval.resolve(decision=approveOnce)` and receives HTTP 200 `{"delivered":true}`. No Runtime
`REFUSED approval_expired` diagnostic follows, but the design remains readable at its original
version and the late resolution causes zero Boundary F writes. S6 expiry therefore passes its
terminal/no-mutation requirement; the Runtime refusal remains invisible to the host as described
above. Sanitized evidence: [`pv1-gaps.json`](evidence/step12/pv1-gaps.json).

**Evidence:** `forma:docs/pontis-browser-findings/30-approvals.md` (F-30-6), cases A06, A11 and A17;
Task G1 expiry replay in [`pontis-pv1-evidence.md`](pontis-pv1-evidence.md#s6--approval--full-stack).

---

## What held, for the record

These were attacked directly and held, all on Pontis code:
- **User boundary on UI routing:** another account's window never received an action.
- **Approvals:** exactly-once under double clicks, replays and two tabs; deny and cancel were inert;
  a forged approval for another person's session was refused.
- **Delegation scoping:** a delegation never reached another account's data.
- **Durable journals:** devices, pairing codes and event history survived a sidecar restart when the
  sidecar was composed with them, and the Runtime reconnected by itself in about 1.2 s.
- **Fail-closed routing:** with several unbound windows, the router answered `ui_unavailable` rather
  than guessing.
