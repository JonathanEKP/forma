# Pontis fix batch 2 — the smaller defects, and step 12's PV-1 evidence

Agreed with the owner on 2026-10-04, after fix batch 1 and its re-test. This page is the
**specification** four Codex agents work from, so they implement rather than decide. If something
here turns out to be wrong against the code, the agent asks the coordinator rather than improvising.
Live state is the §11 *Progress* table in [pontis-integration-plan.md](pontis-integration-plan.md);
batch 1's spec, whose decisions D-1 to D-12 still hold, is [pontis-fix-batch-1.md](pontis-fix-batch-1.md).

## Scope

In: the open Forma-side findings of summary group 7 ([SUMMARY.md](pontis-browser-findings/SUMMARY.md)),
F-50-2, Forma's own refusal codes, and plan §11 **step 12** (PV-1 evidence).

Out, deliberately:
- the `ux` findings — F-10-5, F-10-6, F-10-9, F-30-1, F-30-2, F-30-7, F-chat-15, F-40-14 — for the
  owner's UX conversation, which comes after this batch;
- everything Pontis-side, already written up for the Pontis team in
  [pontis-upstream-report.md](pontis-upstream-report.md); the Pontis repository is not touched;
- **F-30-10** (an expired session on real Codex leaves no notice): it waits for Pontis's fix of
  F-30-5, and Forma does not patch it with a heuristic (E-5);
- the visual loop (`design.render`, image results), which waits for Pontis's CR-2.3;
- production: nothing is deployed.

## Decisions (owner, 2026-10-04)

| # | Decision |
|---|---|
| E-1 | **Send is disabled while a turn runs**; Stop stays available (F-chat-8). Queueing a message is a possible later UX change, not this batch. |
| E-2 | **Stop cancels the turn, not the session** (F-chat-7). The session stays alive and the next message continues it, with its context; the chat no longer marks the session ended on Stop, so no Codex App Server is left behind per Stop. |
| E-3 | **`ui.setZoom` / `ui.setPanel` with no design open are `rejected`** with a typed reason ("open a design first"), never `handled` (F-40-12). |
| E-4 | **Messages over the limit are refused visibly**: the editor shows the limit and does not send past it; the sidecar answers a typed `413 message_too_large` (F-chat-10). |
| E-5 | **F-30-10 waits for Pontis.** No heuristic in Forma. |
| E-6 | **Dev-server surface**: F-50-4 (`..` / `%2e%2e` forwarded) and F-50-5 (CORS) fixed in Vite's and Caddy's config; F-50-6 (`/__open-in-editor`) recorded `wontfix` with the reason (Vite-internal, dev only, loopback only). |
| E-7 | **Forma's own routes answer `401 unauthenticated`** for a missing or invalid token — `/agent/intent`, `/agent/context`, `/pons/v1/devices` — and the editor says "Your sign-in has expired. Sign in again." Pontis's own routes stay as they are (upstream report §4). |
| E-8 | **Step 12 collates and runs the gaps**: an evidence matrix for S1–S13, plus real-Codex runs of what step 11 left unrun (S6 deny / expiry / replay, S8 MCP-request cancel, S12 account switch), ≈10 turns. What Pontis blocks is marked as such, not claimed. |
| E-9 | **Sanitized platform outcomes** are drafted in this repository, ready for the owner to carry to Pontis. |
| E-10 | **All agents are Codex** (`gpt-6-luna`, extra-high effort). |

Coordinator decisions inside the scope (low-risk, not the owner's):

| # | Decision |
|---|---|
| C-1 | **F-50-2**: a UI stream's re-authentication resolves the person afresh, never from the per-request memo, and a stream whose token has expired ends within 60 s even with no events. Reading `exp` from the token payload to schedule the close is acceptable (closing early is only safe); Forma still verifies every token. The editor reconnects with a fresh token. |
| C-2 | **F-40-7**: an action that lands mid-drag is `rejected` with the reason "the person is dragging"; the drag is left untouched. |
| C-3 | **F-40-8**: `ui.selectLayer` arriving while the `ui.openDesign` before it is still loading waits for it, rather than rejecting. |
| C-4 | **F-40-3**: one sentence in `src/guidance.ts`: a `ui_action_timeout` means the outcome is unknown; check before repeating. |
| C-5 | **F-chat-13**: after a reload mid-turn, partial streamed text is not shown; "The agent is replying…" until the completed message. |
| C-6 | **F-chat-14**: a second Stop while a cancel is pending is ignored (one cancel, one notice). |
| C-7 | **F-chat-12**: each message bubble isolates its text's direction (`dir="auto"` and `unicode-bidi: isolate` / `plaintext`), so a bidi control cannot reorder anything outside it. |
| C-8 | **F-chat-1, second form**: the unsent draft is kept per tab (`sessionStorage`) so a reload before sending does not lose it. |
| C-9 | **F-30-6**: an approval resolve the Runtime refuses is not answered `200 delivered:true`; the answer reflects the Runtime's refusal with its code. |
| C-10 | **F-10-3**: the Devices panel follows the live presence frames the UI stream already carries (with a slow poll as fallback): new devices appear, a dead one stops reading "online". |
| C-11 | **F-40-16**: a handler's `detail` is capped to a fixed length before it is acknowledged, so an oversized one cannot reset the connection. |
| C-12 | **F-50-7**: the duplicate React key `#171717`. |

## Common rules for every agent

- Read `CLAUDE.md`, this page, §11 *Progress*, `apps/pontis-sidecar/README.md`, and the finding entries
  your task closes in `docs/pontis-browser-findings/`.
- **Test first** — a test that fails on the finding — then the fix. Small commits; do not let changes pile
  up. Mark each closed finding `fixed (<commit>)` with one line on the cause, and set your row in §11
  *Progress* (a sub-row under "Fix batch 2").
- Gates before `worker_done`: `pnpm lint` (max warnings 0), typecheck, the web and sidecar suites
  (rebuild `apps/pontis-sidecar/test-driver` first if its source changed: the suite runs the compiled
  binary); if Rust is touched, `cargo test --workspace`, `cargo clippy --all-targets --all-features -- -D
  warnings`, `scripts/check-bindings.sh`.
- Browser proof where the task says so, on your own browser-stack instance
  ([pontis-browser-testing.md](pontis-browser-testing.md)) in your own Orca tab; stop it afterwards.
- Never log or commit tokens, auth headers, credentials, asset bytes or `~/.codex/auth.json` content. No
  production, no real Supabase, no edits under `/home/vertex/projects/pontis`, do not touch
  `~/.codex/config.toml` or the global `codex`. Do not stop processes you did not start (the owner's
  servers run on 4765 and 5173).
- Real Codex turns only where the model's behaviour is what is checked: ≤ 4 for tasks F1–F3, ≈10 for F4.

## Merge order

**F3 → F2 → F1 → F4.** All start in parallel; each later one rebases on `main` before `worker_done`.
F1 and F2 both touch `apps/web/src/pontis/`, F1 and F3 both touch the sidecar's routes; F4 is mostly new
documents and runs last so its evidence describes the merged code.

---

## Task F1 — The chat: Stop, Send, limits, refusal codes

Closes: F-chat-7, F-chat-8, F-chat-10, F-chat-12, F-chat-13, F-chat-14, F-chat-1 (second form), F-30-6,
F-chat-16, and Forma's part of F-10-10. Decisions E-1, E-2, E-4, E-7, C-5 to C-9. Browser instance **12**.

1. **E-2 Stop**: verify first what Pontis's `session.cancel` does — turn or session. If it cancels the
   turn and the session lives on, the chat keeps the session open after Stop and the next message
   continues it. If cancel ends the Runtime session, stop and ask. The test proves that a Stop followed
   by a message reuses the session id and starts no second session (F-chat-7's leak).
2. **E-1 Send** disabled while a turn runs (Stop enabled).
3. **E-4 limit**: the sidecar's intent body cap is `MAX_BODY_BYTES = 64 KiB` (`src/intents.ts`). Over it
   the route answers `413 {"error":{"code":"message_too_large"}}` instead of the current generic
   failure; the editor computes the message's encoded size, shows the limit near the box once it is
   approached, and refuses to send past it with a clear line. The view line the sidecar prepends counts
   against the body only in the sidecar; leave room for it.
4. **E-7**: `/agent/intent`, `/agent/context` and `/pons/v1/devices` (Forma's route in `src/live.ts`)
   answer `401 {"error":{"code":"unauthenticated"}}` for a missing or invalid token; other refusals keep
   their codes. The editor maps 401 on these routes to "Your sign-in has expired. Sign in again."
   Update the comment in `live.ts` that says it copies the Control Plane's refusal on purpose.
5. C-5 to C-9 as written above.
6. Browser proof: Stop then continue in the same session; Send disabled mid-turn; an over-limit message
   refused in the editor.

## Task F2 — The UI channel and the Devices panel

Closes: F-40-3, F-40-7, F-40-8, F-40-12, F-40-16, F-10-3, F-50-7. Decisions E-3, C-2, C-3, C-4, C-10 to
C-12. Browser instance **13**.

1. E-3 in `apps/web/src/pontis/handlers.ts`: `setZoom` / `setPanel` with no design open → `rejected`
   with the reason; tests in the handlers suite and in `apps/pontis-sidecar/src/ui.test.ts` (the agent
   sees `rejected`, not `handled`).
2. C-2 (mid-drag → `rejected`, drag untouched), C-3 (`selectLayer` waits for a loading `openDesign`),
   C-11 (cap `detail`), C-4 (guidance sentence), C-12 (React key).
3. C-10: the Devices panel follows the live presence frames (`apps/web/src/pontis/devices*.ts`), with a
   slow poll as fallback; a new device appears and a killed one reads offline without a reload.
4. Browser proof: the agent asks for a zoom with no design open and is refused; a device paired while
   `/agents` is open appears on its own.

## Task F3 — The stream's re-authentication, and the dev proxy

Closes: F-50-2, F-50-4, F-50-5; records F-50-6 `wontfix`. Decisions C-1, E-6. Browser instance **14**.

1. **C-1**: the cause is in `apps/pontis-sidecar/src/person.ts` — `personResolver` memoises per request
   object in a `WeakMap`, and a stream is one request that never ends, so Pontis's per-event
   re-authentication gets the connect-time answer forever (the finding has the reproduction). Make the
   stream's re-authentication resolve afresh (keep the memo for ordinary requests, where it is
   correct), and end a stream whose token has expired within 60 s even when no event is sent. Test both
   with a short-lived fixture token, as the finding did. Confirm in the browser that the editor's
   channel reconnects with a fresh token and keeps working.
2. **E-6**: in `apps/web/vite.config.ts` refuse paths containing `..` or encoded dot segments on the
   proxied Pontis routes and stop granting CORS to every loopback origin; check `deploy/caddy/Caddyfile`
   normalises the same paths (Caddy cannot be run here: reason from its documented matcher behaviour and
   say so). Extend `apps/web/src/pontis/proxy.test.ts`. Record F-50-6 `wontfix` with the reason in its
   finding.

## Task F4 — Step 12: PV-1 evidence, and the sanitized platform outcomes

Decisions E-8, E-9. Browser instance **15** if needed. **Runs last**: start by collating, and do the real
runs after F1–F3 have merged (rebase on `main` first; the coordinator will tell you).

1. **The matrix**: `docs/pontis-pv1-evidence.md`. One row per scenario of
   [pontis-pv1-e2e.md](pontis-pv1-e2e.md) (S1–S13), mapped to the PV-1 requirements of plan §9: status
   (`passed` · `passed on FakeProvider only` · `blocked upstream` · `not run`), and the evidence — test
   names and files, commits, step 11's real-Codex records (`docs/evidence/step11/`), the browser findings
   and re-test lines. A claim without evidence is `not run`. What Pontis blocks is stated with the item:
   the visual loop (`pv1_visual_ready`, CR-2.3) and long sessions (F-30-5).
2. **The gaps, on real Codex** (≈10 turns, `gpt-6-luna`, low effort, through the step 11 runner or the
   browser stack): S6 deny, expiry and replay; S8 cancellation of an in-flight MCP request; S12 account
   switch ends the session. Records sanitized like step 11's (`docs/evidence/step12/`), no tokens,
   headers, emails other than fixtures, or home paths. A failure is recorded, not retried in a loop.
3. **Sanitized platform outcomes**: `docs/pontis-pv1-platform-outcomes.md` — what the integration proves
   about Pontis as a platform, with no Forma URL, credential, user data or product detail Pontis does not
   need (ADR-023 Decision 4; plan §9). The owner carries it to Pontis.
4. Update step 12's row in §11 *Progress* and link the matrix from plan §9.
