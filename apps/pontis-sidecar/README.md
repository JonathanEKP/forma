# forma-pontis — the Pontis integration sidecar

The application-owned sidecar of Pontis ADR-023, as
[docs/pontis-integration-plan.md](../../docs/pontis-integration-plan.md) §2 and §7 describe it:
one Node process holding the Pontis Capability Gateway, delegation issuer, Control Plane and
Realtime Gateway, composed from public Pontis packages. It reaches Forma only through Boundary F
(`/api/v1/pontis`, authenticated with a `forma_svc_` credential). forma-server stays the domain
authority and does not depend on this process. That credential is accepted **only** on
`/api/v1/pontis/*` and `/api/v1/service/resolve`; on any other route forma-server answers
`403 SERVICE_CREDENTIAL_OUT_OF_SCOPE`, so a leaked one is worth Boundary F and nothing more.

This is plan §11 **steps 7 to 9**, plus the real Codex run of step 11, hardened by two fix batches
after a browser break-test: [batch 1](../../docs/pontis-fix-batch-1.md) (the chat per window and its
view line, windows, durable state, the credential's scope) and [batch 2](../../docs/pontis-fix-batch-2.md)
(the smaller defects, and step 12's PV-1 evidence). What PV-1 still waits on is Pontis's: the visual
loop (CR-2.3) and long sessions ([upstream report](../../docs/pontis-upstream-report.md) §1). The live
state is the *Progress* table in plan §11.

## What is Forma's here

| File | What it is |
|---|---|
| `src/capabilities.ts` | the operations Boundary F serves (`forma.guide`, `brand.get` and `assetRequest.create` among them), built from `bindings/generated/capabilityCatalog.ts`; output schemas and bindings |
| `src/ui.ts` | the four editor actions, which have no execution binding at all: they are routed to the person's window, and what is Forma's is who may aim one |
| `src/boundary.ts` | the Boundary F client: fixed operations, bounded answers, Forma's refusals as registry errors |
| `src/person.ts` | who is behind a browser request, asked of `GET /api/v1/service/resolve` |
| `src/intents.ts` | the browser's route to its Runtime: start, input, cancel, approve — and `GET /agent/context`, the application context the event stream has to be asked for by name. It also writes the first line of every message, `[forma-view] project=… design=… name="…" page=… layer=…` (`[forma-view] none` when nothing is open): the editor reports a `view` of ids, the sidecar keeps the ones that are UUIDs and reads the design's name through Boundary F as the person, so the line is always the sidecar's own and what the person types is never the first line |
| `src/live.ts` | what the person's open windows hear while the agent works (durable events from the store with their `sequence`, deltas and presence from the Device's own notification), and `GET /pons/v1/devices` with `online` merged from the live sockets |
| `src/windows.ts` | which window is asking and which agent session its chat belongs to: the fixed-format window id, and the check that a claimed session is this person's before it binds anything |
| `src/sidecar.ts` | the composition |
| `src/guidance.ts` | the `ApplicationAgentGuidance`: the opening context only; the manual is `forma.guide` |
| `src/stack.ts`, `src/ui-window.ts` | test support: the processes of a local run and the person who drives them, and a stand-in for the editor's window. Shared by `local-run.test.ts`, `real-codex.test.ts` and `ui.test.ts`; nothing else may import them, except `browser-stack.ts` |
| `src/browser-stack.ts` | an isolated, multi-instance run for a person at a real browser: the same chain behind a fake identity provider, with Vite and seeded people. A harness, not a test; it names the driver and borrows `stack.ts`'s token minting. [docs/pontis-browser-testing.md](../../docs/pontis-browser-testing.md) |
| `test-driver/` | a test-only Pontis Runtime, in Rust, with FakeProvider or — `FORMA_DRIVER_PROVIDER=codex` — the real Codex in `capability_only` |

Everything else is Pontis, imported from `@pontis/core`, `@pontis/server` and
`@pontis/control-plane` as they ship.

## The Pontis dependency: one seam

Pontis is unpublished (`private: true`), so there is no registry to install it from. Both the
TypeScript packages and the Rust crates the driver links resolve through **one symlink**,
`apps/pontis-sidecar/.pontis`, which is git-ignored because it points at somebody's own checkout:

```bash
ln -s /path/to/pontis apps/pontis-sidecar/.pontis
```

`package.json` links `.pontis/packages/…` and `test-driver/Cargo.toml` paths `.pontis/crates/…`.
When Pontis is distributed for real, those two files change and nothing else does;
`src/package.test.ts` fails if a third place starts naming the checkout. The Pontis packages are
used from their built `dist/`, so the checkout must have been built (`pnpm -r build` there).

Without the seam, `pnpm install --frozen-lockfile` still succeeds (the links dangle) and nothing
else in the repository is affected. `scripts/check.sh` then skips every part of this package and
names each one, rather than passing over code nobody ran. CI has no Pontis checkout, so CI never
exercises this package.

## Running it

```bash
./scripts/check.sh --pontis     # lint, typecheck, driver fmt/clippy, builds, all tests
```

The process, against a running forma-server:

```bash
FORMA_SERVER_URL=http://127.0.0.1:4765 \
FORMA_SERVICE_CREDENTIAL="$(./target/debug/forma-server service-credential sidecar)" \
FORMA_PONTIS_EDITOR_ORIGIN=http://127.0.0.1:5173 \
node apps/pontis-sidecar/src/main.ts      # binds 127.0.0.1:4767, prints READY <origin>
```

Node runs the TypeScript directly (type stripping, Node 24); there is no build step.

`FORMA_PONTIS_DATA_DIR` is where the Control Plane keeps its state: paired devices, unclaimed pairing
codes, revocations and the durable event history (the chat's transcript), as Pontis's append-only
journals (`control-plane.jsonl`, `events.jsonl`; the sidecar has no job queue, so no job journal).
The directory is created `0700` and the files `0600`. **Unset, everything is in memory** and the
sidecar logs `pontis_state_in_memory` at warn level: a restart then unpairs every machine and empties
every conversation. A restart on the same directory keeps all of it, and a Runtime that is still
running reconnects by itself (the test-driver was `online` again about a second after the sidecar
answered). `forma-server backup` does not include this directory yet, so it is not in the daily
backup; [docs/deploy/runbook.md](../../docs/deploy/runbook.md) says what to do about that.
`close()` ends every open editor stream and device socket first, so SIGTERM completes with a window
open.

`FORMA_PONTIS_EDITOR_ORIGIN` is where the editor is served from, and it is the UI channel's whole
allowlist — the value above is Vite's dev server, which proxies the browser's Pontis paths here so
the two share an origin (`FORMA_PONTIS_URL` moves its target off `127.0.0.1:4767`). In the
deployment it is the public name and Caddy does the same. Leave it unset and the four `ui.*`
capabilities answer `ui_unavailable`: no window can connect, which is the fail-closed reading of a
missing setting rather than an open one.

Which paths those are: `/pons/v1/pairing/codes`, `/pons/v1/devices` and `/pons/v1/devices/*`,
`/pons/v1/events`, `/pons/v1/jobs` and `/pons/v1/jobs/*`, `/pons/v1/ui/*`, `/agent/intent` and
`/agent/context` — in
`deploy/caddy/Caddyfile` and `apps/web/vite.config.ts`, kept equal by
`apps/web/src/pontis/proxy.test.ts`. The Runtime's routes (`/pons/v1/pairing/claim`,
`/pons/v1/device/*`) are deliberately not proxied: a remote machine reaching the sidecar is PV-2.

## The local run

`src/local-run.test.ts` drives the whole chain with nothing faked but the model: real
forma-server in hosted mode (session tokens signed with `fixtures/identity`, verified against a
JWKS the test serves), the real sidecar process, and `test-driver`, which pairs a Device key,
opens the device socket and runs a Pontis `SessionManager` with FakeProvider. The test plays the
person through the sidecar's routes and reads the durable event stream and Forma itself.

| Scenario | What is asserted |
|---|---|
| read | `project.list` and `design.get` complete through Boundary F |
| manual | `forma.guide` answers the Pontis rendering of the manual and a `blake3:` revision, through Boundary F |
| brand | `brand.get` answers the kit and the font catalog; `assetRequest.create` leaves one pending request in Forma |
| write | one batch, one new version, the font size changed in Forma |
| cancel | the Runtime abandons `design.validate` mid-flight; the sidecar drops the Boundary F request before Forma answers |
| deny | approval is requested; nothing reaches Boundary F and the design is unchanged, before and after the refusal |
| approve | approval is requested; nothing reaches Boundary F until consent; then exactly one new version, the layer gone |

**Why the driver is Rust.** FakeProvider is the Rust crate `pontis-provider-fake`; `pontisd`
does not register it and its scripts are Rust values, so every FakeProvider run, Pontis's own
included, links the runtime crates. The driver has no library target and is its own Cargo
workspace, so nothing can depend on it and `cargo test --workspace` never builds it.

## The real Codex run

`src/real-codex.test.ts` is the same chain with the model swapped, and it **spends the owner's
tokens**, so it is opt-in: skipped unless `FORMA_REAL_CODEX=1`, never part of `pnpm test` or CI
(listed in `scripts/pontis-skipped-checks.txt`). It needs the Codex release Pontis accepts for
`capability_only` (0.158.0, installed under the gitignored `.codex-pinned/` so the global install is
untouched), a dedicated `CODEX_HOME` there with `auth.json` symlinked to the owner's, and the
`pontis-mcp-bridge` binary. One command, once those exist:

```bash
FORMA_REAL_CODEX_OUT=/tmp/record.json pnpm --filter forma-pontis-sidecar test:real-codex
```

[docs/pontis-step11-real-codex.md](../../docs/pontis-step11-real-codex.md) has the setup, what the
run showed, and — as a section of its own — what it did not.

## In a browser

`src/browser-stack.ts` runs the same chain for a person at the editor rather than a test, one
isolated instance per number so several can run at once, behind a fake identity provider so nothing
reaches a real Supabase project: `node src/browser-stack.ts start 1 --provider codex`. How to start
it, sign in, pick a provider and stop it, and what is not modelled, is in
[docs/pontis-browser-testing.md](../../docs/pontis-browser-testing.md); what testers find goes in
[docs/pontis-browser-findings/](../../docs/pontis-browser-findings/README.md).

## Decisions recorded here

- **The agent learns where the person is from one line the sidecar writes.** `session.input` carries only
  text, so the editor's view (D-3) travels as a prepended `[forma-view]` line, built here and not in the
  browser: a client that could write it could also forge what the agent believes is open. The guidance says
  what it means and that only the first line is the application's. `design.list` is served so "open my
  cover" can become an id. FakeProvider matches inputs byte for byte, so the test driver takes the line off
  before the provider sees it and prints it (`VIEW …`) for `local-run.test.ts` to assert.
- **The test driver does not keep delegations alive.** A session dies about five minutes after its first
  call because Pontis's Runtime refuses a stale delegation before it can refresh it (F-30-5, a Pontis
  defect). A keep-alive here would hide it from the browser re-test; the chat recovers with a notice and
  **New session** instead.
- **The four `ui.*` capabilities were held back until their handlers existed.** Pontis answers
  `ui_unavailable` when no window is connected, which is meant for a state the person can fix by
  opening Forma; with no handler written anywhere it would have been permanent and every call
  would have cost the agent a turn to learn it. They landed in step 9 together with
  `apps/web/src/pontis/`.
- **The editor implements the three UI routes itself** (plan §10(b) D8). `@pontis/react` would
  have put the seam below in the base product's own build, and §7.3 promises the base product
  stays usable — which includes buildable — without Pontis. `src/ui.test.ts` drives the real
  routes so a drift in the frame or ack shape fails a test here rather than silently killing the
  channel in a browser.
- **The UI channel is same-origin and header-authenticated** (D9). The transport refuses a
  cross-origin `Origin` and checks `Host` against the same list, so the editor reaches these
  routes through the proxy in front of both — `FORMA_PONTIS_EDITOR_ORIGIN` is that one name, and
  unset means no channel rather than an open one. The stream is read with `fetch` rather than
  `EventSource` so the session bearer travels in a header and never in a URL.
- **One registration per window, bound to the window whose chat started the session** (fix batch 1,
  D-2). Pontis replaces a UI client with the same `(user, context, appSessionId)`, and the app session
  used to be `user:context`, so two tabs of one person evicted each other every two seconds. The editor
  sends a per-tab id (`x-forma-window`, a UUID; anything else is refused) and the app session is now
  `user:context:window`; a reload keeps its id and replaces its own registration. The chat's agent
  session travels on the stream (`x-forma-agent-session`) and becomes the client's `agentSessionId` only
  once `ControlPlane.authoritativeSessionContext` — the binding made at `session.start`, recovered from the
  durable events after a restart — says it is this person's, in this person's account. With several
  windows and none bound Pontis answers `ui_unavailable` rather than guess; with exactly one it is the
  target. Nothing is remembered here: an id nobody can verify binds nothing.
- **Forma classifies, the sidecar trusts it.** Boundary F's envelope carries `capability_error`
  from `routes/pontis.rs::classify`; the sidecar maps nothing by Forma code, so there is one
  classification. The Forma code and hint travel in `details`.
- **Elevation is exhaustive over `DesignOperation`.** A new operation fails the typecheck in
  `capabilities.ts` until someone decides whether it destroys work. Forma's own `required_risk`
  is the second boundary and refuses an unapproved removal regardless.
- **Nothing is cached across requests.** A person is resolved once per request; Forma revalidates
  membership on every Boundary F call (ADR-024).

## Findings about Pontis

- **Cancelling mid-capability loses the turn's end, with FakeProvider.** On `session.cancel` the
  Runtime triggers its own signal, then the driver's. FakeProvider ends its turn and drops its
  result channel; the Runtime's `submit_capability_result(...).await?` then fails and
  `send_input` returns before reading the provider's `turn.cancelled`, so no terminal durable
  event is written and the error reaches only the `RemoteAgent` observer (`provider_exited`). It
  happens on every run. The cancel test accepts either ending, so it holds when Pontis fixes it.
- **A polyglot host must write Rust to exercise its sidecar against the deterministic provider.**
  ADR-023 says such a host integrates through a TypeScript sidecar, yet FakeProvider is reachable
  only by linking five runtime crates. Pontis's own `polyglot-host-fixture` is the precedent for
  the shape, which suggests this driver belongs on Pontis's side for every polyglot host to reuse.
