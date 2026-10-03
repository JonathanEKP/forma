# forma-pontis — the Pontis integration sidecar

The application-owned sidecar of Pontis ADR-023, as
[docs/pontis-integration-plan.md](../../docs/pontis-integration-plan.md) §2 and §7 describe it:
one Node process holding the Pontis Capability Gateway, delegation issuer, Control Plane and
Realtime Gateway, composed from public Pontis packages. It reaches Forma only through Boundary F
(`/api/v1/pontis`, authenticated with a `forma_svc_` credential). forma-server stays the domain
authority and does not depend on this process.

This is plan §11 **steps 7 to 9**. What is left before PV-1 is the editor's own Pontis surface
(step 10: the device panel and in-app chat), the real Codex run and the evidence.

## What is Forma's here

| File | What it is |
|---|---|
| `src/capabilities.ts` | the operations Boundary F serves (`forma.guide`, `brand.get` and `assetRequest.create` among them), built from `bindings/generated/capabilityCatalog.ts`; output schemas and bindings |
| `src/ui.ts` | the four editor actions, which have no execution binding at all: they are routed to the person's window, and what is Forma's is who may aim one |
| `src/boundary.ts` | the Boundary F client: fixed operations, bounded answers, Forma's refusals as registry errors |
| `src/person.ts` | who is behind a browser request, asked of `GET /api/v1/service/resolve` |
| `src/intents.ts` | the browser's route to its Runtime: start, input, cancel, approve — and `GET /agent/context`, the application context the event stream has to be asked for by name |
| `src/sidecar.ts` | the composition |
| `src/guidance.ts` | the `ApplicationAgentGuidance`: the opening context only; the manual is `forma.guide` |
| `test-driver/` | a test-only Pontis Runtime with FakeProvider, in Rust |

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

`FORMA_PONTIS_EDITOR_ORIGIN` is where the editor is served from, and it is the UI channel's whole
allowlist — the value above is Vite's dev server, which proxies `/pons/v1/ui` here so the two share
an origin. In the deployment it is the public name and Caddy does the same. Leave it unset and the
four `ui.*` capabilities answer `ui_unavailable`: no window can connect, which is the fail-closed
reading of a missing setting rather than an open one.

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

## Decisions recorded here

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
