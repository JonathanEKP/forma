# Pontis fix batch 1 — the browser findings' high groups, and the open-design context

Agreed with the owner on 2026-10-04, after the browser break-testing
([pontis-browser-findings/SUMMARY.md](pontis-browser-findings/SUMMARY.md)). This page is the
**specification** four implementation agents work from, so they implement rather than decide. If
something here turns out to be wrong against the code, the agent asks the coordinator rather than
improvising. Live state of the batch is the §11 *Progress* table in
[pontis-integration-plan.md](pontis-integration-plan.md).

## Scope

In: summary groups **1, 2, 3, 4, 5** and **point 6** (the agent does not know which design is open).

Out, deliberately:
- the `ux` notes (approval cards naming layer/design, the pairing page, a cue when the agent moves
  the view) — they come out of the owner's UX conversation;
- summary group 7 (Stop leaving Codex App Servers alive, sending mid-turn, >64 KiB messages, …) —
  a later short batch; do not regress them, and record it if one is fixed in passing;
- Pontis-side observations — the Pontis repository is not touched;
- anything in the visual loop: `design.render`, image results, the "render → agent inspects →
  fix" cycle. Pontis is building image results (CR-2.2 → CR-2.3) and Forma waits for it;
- production: nothing is deployed (PV-2 is not authorized); deployment changes are documentation.

## Decisions (owner, 2026-10-04)

| # | Decision |
|---|---|
| D-1 | **One chat per window**, at the app level: it survives moving between designs and projects. A reload re-attaches the window to the session it had (kept per tab, e.g. `sessionStorage`). Signing out ends it. |
| D-2 | **Agent actions go to the window whose chat started the session.** That window binds to the session; with several windows and none bound, Pontis answers `ui_unavailable` and the agent says so. Never "most recent" or "focused". |
| D-3 | **Each message carries the person's view**: the editor sends a structured view; the **sidecar** validates it (UUIDs only), reads the design's name through Boundary F as the person, and prepends one fixed-format line. If the read fails, the line carries the id alone. The person's own text can never be taken for that line. |
| D-4 | **The person sees their message as typed, plus a small chip** naming the context sent (design · page · layer). On reload the chat parses the line back into the chip. |
| D-5 | **Only `design.list` is newly served.** Every other catalog-but-unserved operation stays unserved. |
| D-6 | **A dead session is recovered explicitly**: a notice with the reason in plain words, a **New session** button, the old history kept above a divider, and pending approval cards marked expired with no buttons. Nothing auto-starts. |
| D-7 | **When the agent writes the open design**, the editor refetches it, keeps the selection if the layer still exists, drops an in-progress drag, and shows a short notice: "The agent updated the design (v12)". |
| D-8 | **Durable sidecar state** under a new `FORMA_PONTIS_DATA_DIR`, using the journal stores Pontis ships. Unset means in-memory as today, with a loud warning in the log. Files `0600`. `forma-server backup` does not include it yet (documented). |
| D-9 | **The `forma_svc_` credential is accepted only on `/api/v1/pontis/*` and `/api/v1/service/resolve`**; anywhere else it is refused 403 with the stable code `SERVICE_CREDENTIAL_OUT_OF_SCOPE`. |
| D-10 | **Push after every merge** with the gates green. |
| D-11 | **Final re-test**: the findings this batch closes, plus one normal-path pass per area (≈15 Codex turns). |
| D-12 | Deployment: `FORMA_PONTIS_DATA_DIR` is documented in the runbook only; production is not touched. |

## Facts established before dispatch

- **The 5-minute session death is not a harness setting.** Pontis issues an `AppDelegation` for
  `APP_DELEGATION_TTL_MS = 5 min` (`pontis/services/control-plane/src/control-plane.ts`), and the
  real Runtime refreshes it 15 s early (`pontis/crates/runtime/src/delegation.rs`). A session dying
  at ~5 min therefore means a refresh fails somewhere — in the test-driver or in the sidecar's
  routes. Finding which is part of task A.
- **Why windows evict each other**: Pontis's `UIClientRegistry` closes an existing client with the
  same `(userId, applicationContextId, appSessionId)`
  (`pontis/packages/sdk-server/src/ui-clients.ts`), and Forma sets
  `appSessionId = userId:applicationContextId` (`apps/pontis-sidecar/src/sidecar.ts`), so every
  window of one person shares it. Routing (same file): a client bound to the AgentSession wins;
  else the only client; else `ui_unavailable`. Binding is per connection, through the
  `agentSessionId` that `authenticatePrincipal` returns.
- **`session.input` carries only text** (`apps/pontis-sidecar/src/intents.ts`), so the view
  travels as a prepended line.
- **`design.list` is already declared** in `crates/agent-catalog/src/operations.rs`
  (`Doors::PontisOnly`) but absent from `SERVED` in `apps/server/src/routes/pontis.rs` and
  `apps/pontis-sidecar/src/capabilities.ts` (seven entries each).
- **The service credential acts as any represented user on any route**: `apps/server/src/auth.rs`
  checks the credential and the represented-user headers, not the path (only `RESOLVE_PATH` is
  special-cased). The sidecar calls only `/api/v1/pontis/*` and `/api/v1/service/resolve`.
- **Pontis ships durable stores**: `JournalControlPlaneStore`, `JournalEventStore`,
  `JournalJobStore` over an append-only, fsync'd `Journal`
  (`pontis/services/control-plane/src/{store,events,jobs,journal}.ts`); `ControlPlane` takes
  `store` and an event store in its options.

## Common rules for every agent

- Read `CLAUDE.md`, this page, §11 *Progress*, `apps/pontis-sidecar/README.md` and the findings
  your task closes. Use [pontis-browser-testing.md](pontis-browser-testing.md) to see your change in
  a real browser; pick an instance number nobody else uses (A=7, B=8, C=9, D=none needed).
- **Test first**: a test that fails on the finding, then the fix. Small commits; do not let changes
  pile up. Update the finding's *Status* line to `fixed (<commit>)` in its area file, and your row
  in §11 *Progress*.
- Gates before `worker_done`: `pnpm lint`, typecheck, the web and sidecar test suites, and — if you
  touched Rust — `cargo test --workspace`, `cargo clippy --all-targets --all-features -- -D
  warnings`, `scripts/check-bindings.sh`. `./scripts/check.sh --pontis` covers the sidecar.
- Never log or commit tokens, auth headers, credentials, asset bytes or `~/.codex/auth.json`
  content. Do not touch production, the real Supabase project, `/home/vertex/projects/pontis`, the
  global `codex` or `~/.codex/config.toml`. Nothing in the visual loop (see *Scope*).
- Real Codex (gpt-6-luna, low) only where the model's behaviour is what is being checked; ≤ 8 turns
  per agent.
- A product decision this page does not cover goes to the coordinator as an `ask`.

## Merge order

**D → A → B → C.** All four start in parallel; each later one rebases on `main` before its
`worker_done`. D and A barely overlap anything. B and A meet at the chat's session id (B wires the
binding to what A's chat exposes). B and C meet in `sidecar.ts` and `apps/web/src/pontis/channel.ts`.

---

## Task A — The chat: account-scoped, view-aware, recoverable; the editor follows agent writes

Closes: F-chat-2, F-30-3 (group 1); F-chat-4, F-chat-5, F-10-2, F-10-4, F-30-4, F-30-5 (group 2);
F-00-4, F-chat-11, F-40-6, F-40-5 (point 6). Decisions D-1, D-3 to D-7.

1. **The editor follows agent writes (D-7).** When the window learns of a completed agent write on
   a design (from the durable events it already reads), invalidate that design's query and the
   project's design list. If it is the open design: refetch, keep `selectedLayerId` if the layer
   still exists (else clear it), clear `draft`, and show a transient notice "The agent updated the
   design (v{n})". No conflict banner may be the person's first sign.
2. **The 5-minute death (fact above).** Reproduce on the browser stack, find whether the
   delegation refresh fails in the test-driver or in the sidecar, and fix it where it fails. If the
   fix would be in Pontis, stop and ask. Record the cause in `30-approvals.md` under F-30-5.
3. **Recovery (D-6).** The chat recognises a terminal or unreachable session — session ended or
   failed events, revoked device, refresh failure, `This session isn't allowed to do that`,
   Runtime gone — and leaves the "working…" state for a notice with the reason in plain words and
   a **New session** button. History stays above a divider; pending approval cards become
   "expired" with no buttons. Works across reloads and after pairing a new device.
4. **Chat at the app level (D-1).** Lift the chat state and panel out of the design view so it
   survives navigating designs and projects. Per-tab persistence of the current session id so a
   reload re-attaches; cleared on sign-out. Expose the current session id from the chat (B binds
   the window to it).
5. **The view line (D-3, D-4).** The editor adds `view: { projectId?, designId?, pageId?,
   layerId? }` to `session.input`. The sidecar (`intents.ts`) validates each as a UUID (anything
   else: drop the field, never the message), reads the design through Boundary F as the person to
   get its name, and prepends exactly one first line:

   ```text
   [forma-view] project=<uuid> design=<uuid> name="<name>" page=<uuid> layer=<uuid>
   ```

   Omitted parts are left out; no design open gives `[forma-view] none`. The name: control
   characters and newlines removed, quotes escaped, ≤ 80 characters. If the read fails, `name=` is
   left out. The sidecar always prepends its own line, so text the person types (even starting with
   `[forma-view]`) is never in the first line. Add one sentence to `src/guidance.ts` saying what
   the line means. The chat shows the person's text as typed with a chip (design name · page ·
   layer), parsing the first line back on reload.
6. **Serve `design.list` (D-5).** Boundary F route in `apps/server/src/routes/pontis.rs`
   (account-scoped, as `project.list`), `SERVED` updated on both sides, the sidecar's
   `boundary.ts` and capability registration, a local-run scenario, and the manual/skill drift
   tests green. MCP's `tools/list` stays byte-identical (the operation is `PontisOnly`).
7. **Browser proof** on instance 7: switch designs mid-conversation and the agent acts on the new
   one; "open my <name>" works through `design.list` + `ui.openDesign`; kill the Runtime mid-turn
   and recover with New session; a session lives past 5 minutes; an agent edit appears without a
   conflict.

## Task B — One registration per window, bound to its chat session

Closes: F-40-1, F-50-3, F-40-15, F-chat-3; check F-chat-9. Decision D-2.

1. The editor generates a window id per tab (kept in `sessionStorage`, so a reload *replaces* its
   own registration instead of adding one) and sends it on the UI routes in a header, e.g.
   `x-forma-window`. The sidecar validates it (fixed format; reject otherwise) and sets
   `appSessionId = userId:applicationContextId:windowId`.
2. Binding (D-2): the window sends its chat's current agent session id (header, e.g.
   `x-forma-agent-session`) when it opens the stream, and reopens the stream when the chat starts
   or re-attaches to a session. In `authenticatePrincipal`, the sidecar returns it as
   `agentSessionId` **only after verifying the session belongs to this person and context**, from a
   durable record (the Control Plane's events or jobs for that context), not an in-memory map —
   C makes the store durable and the check has to survive a restart. An unverifiable id binds
   nothing.
3. After A merges, wire the binding to the session id A's chat exposes.
4. A second tab signing in as someone else registers as that person and the first tab's
   registration is not left as the old person (F-40-15).
5. Tests in `apps/pontis-sidecar/src/ui.test.ts` (real routes, the editor-shaped client): two
   windows of one person both stay connected; an action goes to the bound window and is never
   `handled` by the other; with two unbound windows the agent gets `ui_unavailable`; a forged
   session id of another person binds nothing. Note in the finding whether F-chat-9 (stream ending
   ~2 s after opening) was the same eviction.
6. Browser proof on instance 8: two tabs, the chat in tab 1, the agent's `ui.*` actions move only
   tab 1; close tab 1 and the agent reports it cannot reach a window.

## Task C — Durable sidecar state, a clean stop, and an editor that reconnects

Closes: F-10-1, F-chat-6, F-40-10 (durability); F-40-2 (stop); F-40-4 (reconnect). Decisions D-8,
D-12.

1. `FORMA_PONTIS_DATA_DIR` in `main.ts` (and `SidecarOptions`): when set, compose `ControlPlane`
   with `JournalControlPlaneStore`, a journal event store and a journal job store under that
   directory (created if missing, files `0600`). Unset: in-memory as today and a `warn` log line
   saying state will not survive a restart.
2. The browser stack sets it per instance (inside the instance's temp dir).
3. A restart with the same directory keeps devices, pairing codes still in date, and the durable
   event history (the chat's transcript after reload). Whether the Runtime reconnects by itself is
   Pontis's behaviour: test it with the test-driver and record what happens; do not change Pontis.
4. **Clean stop (F-40-2):** `close()` ends every open UI stream (and the device sockets) before
   closing the server, so SIGTERM completes with editor windows open. Test it.
5. **Reconnect (F-40-4):** find why an open editor stays deaf after a sidecar restart behind Vite
   (the channel already retries every 2 s), fix it, and test that actions reach the window again
   without a reload. State in the finding whether Caddy would behave the same (reason from
   `deploy/caddy/Caddyfile`; it cannot be run here).
6. Document the variable in `apps/pontis-sidecar/README.md`, `docs/pontis-browser-testing.md` and
   the deploy runbook (`docs/deploy/runbook.md`), and that `forma-server backup` does not include
   it yet.
7. Rebase after B: both touch `sidecar.ts` and `channel.ts`.

## Task D — The service credential reaches Boundary F and nothing else

Closes: F-50-1. Decision D-9.

1. In `apps/server/src/auth.rs`, a `forma_svc_` credential is accepted only for paths under
   `/api/v1/pontis/` and for `/api/v1/service/resolve`; anywhere else it is refused 403 with code
   `SERVICE_CREDENTIAL_OUT_OF_SCOPE` in the standard error envelope, before any represented-user
   handling. The check is on the normalised path, so `..`, encoded slashes and case cannot route
   around it.
2. Tests: the credential with valid represented-user headers is refused on a normal REST read, a
   write, and the PAT-minting route (the F-50-1 repro); still accepted on every Boundary F route the
   sidecar uses and on resolve. The local run (`pnpm --filter forma-pontis-sidecar test`) stays
   green.
3. Add the code to wherever error codes are documented, and a line to the CLAUDE.md invariant
   about the credential if one exists (otherwise a sentence under *Config*).

## Task E — Re-test (after all four merge)

The findings closed above, re-run in the browser exactly as their area files describe, plus one
normal-path pass per area; ≈15 Codex turns. Results appended to each area file (`re-tested
<date>: fixed | still open | regressed`) and a short section in `SUMMARY.md`.
