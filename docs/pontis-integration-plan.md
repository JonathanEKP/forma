# Pontis integration plan — PV-1

**Status:** plan. No code has been written for it.

**Pontis stage:** the MCP/polyglot-host prerequisite gate is accepted as `pv1_ready`
(`pontis/docs/implementation/mcp-host-integration-gate.md`), MP-0→MP-6 are `done`, and PV-1 is
`ready` (`pontis/docs/implementation/product-validation-plan.md` §3). The owner has placed this
repository in scope.

**Pontis contracts this plan is built on:** ADR-010, ADR-015, ADR-016, ADR-019, ADR-020, ADR-022,
ADR-023, ADR-024, ADR-025, ADR-026, ADR-027; `docs/06-capability-system.md`,
`docs/12-sdk.md`, `docs/13-ui-integration.md`, `docs/15-deployment.md`, `docs/09-security.md`.

This document owns the **host side** of the integration. Per ADR-023 Decision 4, Forma's repository
records the capability/domain mapping, user and application-context tests, deployment wiring and
product behaviour; Pontis records only sanitized reusable-platform outcomes and never names this
product.

---

## 1. Authority, and how the two repositories' documents resolve

Forma is the host and the domain authority (ADR-023 Decision 1). Pontis does not own Forma's data,
user identity, account mapping, authorization or business rules.

Two normative hierarchies now touch the same work. They resolve like this:

| Question | Authority |
|---|---|
| What a capability may do, how it is projected, what credentials exist, what the Runtime enforces | Pontis ADRs and specs, in their own precedence order |
| What Forma's domain means, which operations exist, what the editor does, what the product promises | [forma-plan.md](../forma-plan.md) and [CLAUDE.md](../CLAUDE.md) |
| A genuine conflict | Stop and record it. Neither repository's agent resolves it by writing code (`pontis/AGENTS.md` § Normative hierarchy) |

Anything touching identity, tenancy, authorization, credential flow or provider support is the
owner's decision, not this plan's. §10 keeps three different things apart: the Pontis contracts
that already decide the design, the host-side decisions — two now settled, five still open — and the
one real product blocker.

---

## 2. Topology: an application-owned sidecar

ADR-023 requires recording **why** a shape was chosen.

Forma's backend is Rust (`apps/server`, Actix) and serves the built React SPA from
`apps/web/dist`, so the whole product is one origin. The Pontis host SDK (`@pontis/core`,
`@pontis/server`) and the mountable Control Plane (`@pontis/control-plane`) are TypeScript. The
**embedded** shape of ADR-023 Decision 1 is therefore unavailable: it requires a compatible
backend process, and reimplementing the Gateway, delegation issuer or Control Plane in Rust is
what ADR-023's Context explicitly refuses and what `slice-08` §13 counts as a failed reuse proof.

**Decision: the application-owned integration sidecar.** It is Forma's code, deployed by Forma,
composed from public Pontis packages.

```text
browser ── https ──▶ Caddy ─┬─▶ forma-server   (Rust, :4765)  REST + SPA
                            ├─▶ forma-mcp      (Rust, :4766)  existing direct MCP door
                            └─▶ forma-pontis   (Node, :4767)  sidecar
                                  │  Capability Gateway
                                  │  delegation issuer
                                  │  Control Plane + Realtime Gateway   (co-located, ADR-016/023)
                                  │
                                  └── Boundary F ──▶ forma-server domain operations
                                                          │
                                                          └─▶ crates/application use cases
                                            ▲
                                   outbound WSS
                                            │
                              user machine: pontisd + Codex 0.156.1
```

The four Pontis logical components stay in that one process, which is what keeps ADR-015's opaque
reference credentials valid (issuer and validator co-located) and keeps this out of the deferred
shared Control Plane.

The port is a placeholder. `4766` is not reserved but **occupied** — `FORMA_MCP_BIND` defaults to
it and `deploy/caddy/Caddyfile` already proxies `/mcp*` there — so the sidecar takes the next one.
[CLAUDE.md](../CLAUDE.md) § Planned architecture still calls `4766` "reserved for future MCP HTTP";
that line is stale and this plan is where it gets corrected.

---

## 3. Capability catalog, and the domain service behind each one

Every capability binds to a `crates/application` use case, reached over Boundary F. That crate is
already the domain-service layer ADR-010 asks for: every function takes `account: AccountId`, and
`forma-mcp` has never been allowed to bypass it. **No capability reaches SQLite or the filesystem
directly.**

`risk` follows `pontis/docs/06-capability-system.md` § Risk model. Every `command` carries
presentation metadata, which is required at registration.

Ids follow Pontis's capability-id grammar — dot-separated camelCase segments, no underscore —
because `defineCapability` refuses anything else at registration. Four ids first shipped with
underscores and could not be registered at all; `every_id_is_one_pontis_will_register` in
`crates/agent-catalog` now holds the grammar. forma-mcp's tool names are separate and did not move.

### Resources

| Capability | Domain binding | Notes |
|---|---|---|
| `project.list` | `application::projects::list` | bounded list; the PV-1 "bounded read" |
| `project.get` | `application::projects::get` | |
| `brand.get` | `application::projects::get_brand` | the agent must read this before choosing any value |
| `design.list` | `application::designs::list` | |
| `design.get` | `application::designs::get` | carries the version the agent must echo as `base_version` |
| `design.history` | `application::designs::history` | |
| `asset.list` | `application::assets::list` | |
| `template.list` | `application::templates::list` | |
| `assetRequest.get` | `application::asset_requests::get` | |
| `design.validate` | `application::renders::validate` | no side effect, so `resource`, not `command`, despite being an "output" tool in Forma's MCP vocabulary |

### Commands

| Capability | Domain binding | risk | Notes |
|---|---|---|---|
| `design.create` | `application::designs::create` | `write` | |
| `design.applyOperations` | `application::designs::apply` | `write`, elevated | see §3.1 |
| `design.undo` | `application::designs::undo` | `write` | never decrements the version |
| `design.export` | `application::exports::create` | `write` | writes a file under the data dir; not an `external_side_effect` — nothing leaves the deployment |
| `assetRequest.create` | `application::asset_requests::create` | `write` | how the agent asks for an image it does not have |
| `template.createDesign` | `application::templates::create_design` | `write` | |
| `design.delete` | `application::designs::delete` | `destructive` | not required by PV-1; a Forma product decision — §10(b) D3 |

Deliberately absent, matching [CLAUDE.md](../CLAUDE.md) § MCP surface: no `project.create`, no
brand-editing capability, no template-saving capability. An agent works inside a workspace the user
owns, and those three decisions are the user's. Pontis does not change that reasoning.

Also absent, but for a different reason: **`design.renderPreview`**. Forma's MCP tool returns the
PNG as an image content block, which the Pontis projection cannot carry — §10(c). A
structured-only variant (success, dimensions, cache key) is still coherent and is what would let a
paired agent trigger the preview *the person* looks at, so whether it is added depends on how
§10(c) is decided. It is not an oversight.

### 3.1 `design.applyOperations` and dynamic risk elevation

`apply_operations` is a batch of up to `MAX_OPS_PER_BATCH = 20` typed `DesignOperation`s applied
atomically. Most are ordinary editing; `RemoveLayer` and `RemovePage` destroy work.

The declared baseline is `write`. A batch containing a removal is elevated by the `requiresApproval`
hook to the class the removal deserves, per `docs/06-capability-system.md` § Dynamic risk
elevation. Elevation is one-directional and the hook can never lower a class.

Two properties of Forma's operation set make this fit cleanly rather than awkwardly:

- **Approval binds to resolved input.** `docs/06` requires a set-valued command to take explicit
  resolved identifiers, never a filter re-evaluated at execution. Forma's operations already carry
  explicit layer and page ids — there is no filter form to remove.
- **The batch is already one atomic unit with one version increment.** One invocation, one approval,
  one idempotency key, matching the `task.delete` reasoning in `pontis/docs/19-reference-app.md`.

Approval scope is settled by the contract, not by this plan. `docs/06` § Approvals supports `once`
(authorizing exactly one `invocation_id`) and a bounded `session` scope. **PV-1 needs only `once`**:
one batch, one invocation id, one human decision. Session scope stays deliberately outside the
initial integration.

That leaves a product observation rather than an open question: an agent doing ordinary editorial
work removes layers routinely, so `once` per batch may prove tiring in use. If it does, session
scope is the bounded answer and the owner can ask for it then, with evidence. It is not a Pontis
blocker and nothing waits on it.

### 3.2 UI capabilities

`pontis/docs/06` § Capability kinds: a `ui` capability has no execution binding, a fixed output
schema, and `risk` is always `read`. It is routed to a `UIClient`, never executed by the Gateway.

Forma's editor keeps exactly this state in Zustand (`apps/web/src/store.ts`), which is what makes
these near one-to-one:

| Capability | Handler |
|---|---|
| `ui.openDesign` | navigate to the design, then `selectPage` |
| `ui.selectLayer` | `useEditor.selectLayer` |
| `ui.setPanel` | `useEditor.setPanel` — `properties`, `validation`, `assets`, `brand` |
| `ui.setZoom` | `useEditor.setZoom`, clamped to `MIN_ZOOM`/`MAX_ZOOM` |

Each still declares an input schema and an authorization binding, and both still run: navigating to
a design the represented user may not read would disclose that it exists.

`apps/web/src/App.tsx` routes on a bare `switch (window.location.pathname)` with no router library,
and an earlier reading of this section said `ui.openDesign` would therefore change location. It
cannot: the four paths in that switch are authentication hand-offs, the open design is React state
on `/`, and any other path answers "Page not found". So the handler lifts the design id instead —
`App` now holds `openDesign` beside its `designId` state and the channel calls it, which also lets
it select the named page in the same commit rather than through a reload that would lose it. Still
an implementation detail, and still not a contract problem.

---

## 4. Boundary F — authentication and trusted context

ADR-023 Decision 2 defines the hop and its obligations. It deliberately does **not** prescribe the
mechanism: "authenticate the sidecar using a host-owned server-to-server mechanism appropriate to
the deployment".

What the hop must carry, and where each value comes from:

| Value | Source | Never from |
|---|---|---|
| sidecar service identity | a host-owned credential | a model argument |
| represented user | the trusted application context the sidecar authenticated | a tool argument |
| `application_context_id` | the same trusted binding (§5) | a tool argument |
| `invocation_id` | the Runtime, forwarded by the Gateway | the model |
| cancellation and a timeout | the caller's `AbortSignal`, which the sidecar client threads through every call | — |

Two of ADR-023 Decision 2's obligations are **not** yet satisfied and are easy to read past:

- **cancellation and timeouts.** The sidecar client passes an `AbortSignal` into every operation.
  Forma's REST surface has no cancellation semantics at all, and shutdown deliberately waits 20s for
  in-flight work. An aborted Boundary F request therefore leaves the work running — and
  `design.validate` renders every page to measure glyphs, which CLAUDE.md records at 8.6s for a
  Story page at scale 4. It is the cheapest capability for a model to call and the most expensive to
  serve.
- **idempotency identifiers.** See §10(b) D6: `base_version` is not a replay ledger.

Forma's existing pieces line up with this better than expected:

- `crates/application` is already the bounded operation surface — there is no generic endpoint
  proxy to build or to refuse.
- `AuthContext` (`crates/design-core/src/account.rs`) already documents `account_id` as "a security
  boundary, not an input field: resolved here, from a credential, and never read from a request
  body, a query parameter, an MCP tool schema or an agent prompt." That is ADR-024's rule, written
  before ADR-024 existed.
- `apps/server/src/routes/helpers.rs` already fails closed in hosted mode, and `actor_from` already
  refuses to trust a caller-supplied actor outside `AuthKind::Local`.
- Forma's optimistic `base_version` UPDATE is **not** an idempotency ledger, and reading it as one
  is the mistake this plan made in an earlier draft. What it does cover is the fixture's *other*
  guard: an approval granted against version N is refused once the design has moved. Replay safety
  is a separate, missing thing — §10(b) D6.

**The precedent to follow, not to copy.** `pontis/examples/polyglot-host-fixture/src/main.rs` is a
Rust host proving this hop: a bearer service credential compared in constant time, `x-fixture-user-id`
and `x-fixture-application-context-id` as trusted headers, fixed operation paths, bounded response
bytes, and a ledger keyed on the invocation scope. Its header names are the fixture's, not a Pontis
contract.

**The credential is decided in §10(b) D1:** a dedicated service credential with its own
`AuthKind`, not a reused PAT. A PAT is not identity-free — `apps/server/src/auth.rs` sets
`user_id: key.created_by_user_id` — so the full reasoning lives there rather than being summarised
twice and getting it wrong once.

---

## 5. Application context — `AccountId` is the mapping

ADR-024 resolves the last open question from the earlier analysis. `ApplicationContextId` means only
"the application-owned authorization context selected for this session"; the host decides what it
maps to.

**Forma's `AccountId` is that value.** It is already the tenant boundary, already threaded through
every repository method as a non-`Option` parameter, and already resolved from a credential rather
than from input. Its UUID form satisfies ADR-024's grammar
(`[A-Za-z0-9][A-Za-z0-9._:-]{0,127}`).

What follows from ADR-024, and what each implies for Forma:

- **The binding is immutable for the session.** Switching account means a new `AgentSession`, not a
  rebind. Forma's web app selects the account before session creation and cannot change it after.
- **Durable history splits.** A principal stream `(application_id, user_id)` carrying only
  `device.revoked`, and an application-context stream per account carrying everything else. A
  frontend holds two cursors with independent gap rules.
- **`UIClient` eligibility requires matching user *and* context.** A tab that switched account stops
  being a valid target for the previous account's session. Forma's SPA must re-register on that
  change.
- **The host revalidates membership on every invocation.** A stored context or a valid delegation
  never preserves access that was revoked. `crates/application` already takes `AccountId` on every
  call, so the check has nowhere to hide.

**One account per user is an assumption, not an invariant — and ADR-024 makes that load-bearing.**
An earlier draft of this plan read `AccountRepository::membership_of` returning
`Option<AccountMembership>` as the model. It is not. The schema permits many:

```sql
-- migrations/sqlite/0004_accounts.sql
PRIMARY KEY (account_id, user_id)
```

and the query that resolves it has no `ORDER BY`:

```sql
-- crates/persistence/src/sqlite/account.rs
SELECT account_id, user_id, email, role, created_at
FROM account_memberships WHERE user_id = ?     -- fetch_optional, unordered
```

Two rows can exist today: `forma-server grant-owner` grants a bootstrap membership to any subject,
including one who already owns an account created through `POST /api/v1/accounts`. Which one wins is
undefined.

Right now that is a latent bug about which projects a browser sees. Under ADR-024 it would decide
the **session-immutable authorization context** — binding an `ApplicationContextId` to a coin flip,
which is the opposite of ADR-024 Decision 1's "selected by the authenticated host application". So
it has to be made deterministic before a session is ever bound: §10(b) D7.

`AccountRole` genuinely does have one variant, `Owner`. ADR-024 explicitly supports a
single-context application ("supplies one stable application-owned value"), and PV-1 bounds its
isolation evidence to what the app supports, so the role limitation is recorded and the
authorization that exists is tested. Nothing is added there on Pontis's account — §10(b) D4.

---

## 6. Guidance and posture

### 6.1 Guidance is the opening context, and the manual is a capability

ADR-027 Decision 2 makes guidance mandatory for an application-operation session, supplied through
the authenticated Gateway discovery response, validated, digest-bound and session-immutable. The
schema is six required fields and a hard budget:

```text
version = 1
role            ≤ 128 bytes
purpose         ≤ 2 KiB
instructions[]  ≤ 32 entries × 1 KiB
out_of_scope[]  ≤ 32 entries × 1 KiB
terminology[]   ≤ 64 entries (term ≤ 128 B, meaning ≤ 512 B)
                 16 KiB canonical total
```

**An earlier draft of this section read that budget as a cap on everything the agent could ever
know, and planned to distil the skill into it. That was wrong, and Pontis said so when asked.** The
budget bounds the *opening context*, not the agent's knowledge. An application may serve its full
manual through a `resource` capability, risk `read`, as an intended use of the existing contract —
ordinary JSON, no `resultContent`, no image support, no MCP Resources, no native skill installed in
the provider.

So nothing is distilled. The reasoning and examples that make a rule followable rather than merely
stated stay whole, and the guidance carries only what must hold even if the manual is never read.

The decisions, settled with the owner:

| | |
|---|---|
| Source | One. A template at `crates/agent-catalog/manual/SKILL.md`, rendered per door through the `{capability.id}` substitution `surface.rs` already performs |
| MCP artifact | `skills/forma/SKILL.md` is generated from it and committed, and stops being edited by hand |
| Pontis rendering | Rendered from the same template, embedded with `include_str!`, not committed |
| Capability | `forma.guide`, `resource`, risk `read`, no input, answering `{ revision, text }` |
| Revision | A content hash of the rendered manual, in the result only and never in the guidance text |
| Execution | A Boundary F route, account-scoped like every other capability |
| Sections | None. The whole manual, with a test that fails at 75% of its 64 KiB bound |
| MCP door | Does not get `forma.guide`. It stays at fourteen tools and byte-identical |
| Obligation | Advisory. Nothing refuses a write because the manual went unread |

Three of those deserve their reason recorded, because each had a plausible alternative.

**The template lives outside `skills/`** because hosts scan that directory — Claude Code reads
`.claude/skills`, Codex walks `.agents/skills` — and a file full of `{design.get}` sitting there is
exactly what should never be loaded by mistake. It lives in the crate that knows how to render it.

**`include_str!` rather than a runtime read**, for the reason that already governs the `/skill`
route: a deployed binary must not be able to serve a manual that differs from the one CI tested.
Editing without a rebuild sounds like a convenience and is precisely the property to refuse.

**The result carries no timestamp and no capability list.** A timestamp would make two identical
reads look different to the invocation ledger's fingerprint. A capability list would give the agent
a second answer to a question its authorized session catalog already answers, with no way to tell
which governs when they disagree.

Two things the budget does not change. Guidance is provider-visible, so nothing sensitive goes in
it. And guidance is advisory — Pontis will not block a write because a manual went unread, and as
its lead put it, a call does not prove comprehension. A rule that *must* hold belongs in the
validator or in a capability's authorization, never in prose.

### Guards

`apps/mcp/tests/skill_matches_the_code.rs` already asserts that every tool, operation, brand token,
error code and layer field the skill names actually exists. Its ten tests keep pointing at the
generated MCP artifact unchanged: that file holds real MCP tool names, so they stay true. Aiming
them at the template would break them, since a template holds ids.

Three tests are added over the template: every `{id}` resolves in the catalog, the template renders
on both doors, and **no MCP tool name appears in it literally.** That third one closes a hole
`render_text` cannot see — it validates braces, so a sentence in prose saying "call `get_design`"
renders cleanly and then lies on the Pontis door. An operation is named with `{id}` or it is not
named.

`scripts/check-bindings.sh` widens to cover the generated skill rather than gaining a sibling
script. Its mechanism is already the right one, and two scripts doing the same job drift on what
each counts as drift.

### 6.2 `capability_only` suits Forma unusually well

ADR-027 Decision 3 makes `capability_only` the secure default, selected by the Runtime and not by
the application: no ambient MCP server, no provider-native filesystem, process, shell, web, browser
or subagent facility reachable by the model, no model-facing network, and failure to verify any
required restriction fails session readiness with `unsupported_feature`.

For most host applications that is a severe restriction. For Forma it is close to ideal: **the agent
designs, it does not code.** It never needed a shell or the repository. Everything it does is
already expressed as typed operations through one mutation path — which is the invariant
[CLAUDE.md](../CLAUDE.md) states as "the editor has no private authoring path", now applied to the
agent as well.

Decision 4's neutral workspace (`<runtime-dir>/provider-workspaces/<session_id>/`, `0700`, no Git
repository, no mount of any source tree, removed on exit) costs Forma nothing.

### 6.3 The provider is pinned

`pontis/docs/07-provider-system.md` and the accepted MP-6 evidence pin real-provider support to
**Codex CLI `0.156.1`** on `linux/x86_64`. `0.154.0` is deterministic-contract only: the real run
failed closed because `unified_exec` remained enabled. A newer release is a new candidate and is not
covered.

Product consequence, stated plainly so the UI does not promise otherwise: **the paired-agent path is
Codex.** Claude and Gemini are Activation-Map-gated in Pontis. They keep working through Forma's
existing direct MCP door, which is a different door with different properties (§8).

---

## 7. Sidecar composition and deployment wiring

### 7.1 Composition

Built from public packages only — `@pontis/core`, `@pontis/server`, `@pontis/control-plane`, and
`@pontis/react` in the SPA. No private imports, no copied internals. The reuse metric in
`slice-08` §13 is zero copied lines of provider, runtime, device-protocol or control-plane code, and
this plan is bound by it.

The sidecar's own code is: the capability definitions of §3, their Boundary F bindings, the
guidance source of §6.1, `resolveUser`, and the domain-error mapping onto Forma's typed codes.

Forma's error envelope is already stable and code-first (`{"error": {"code", "message", "details"}}`),
which makes the mapping onto `docs/06`'s typed registry mechanical rather than interpretive:
`VERSION_CONFLICT` → `conflict`, `DESIGN_NOT_FOUND` → `not_found`, `INVALID_OP` →
`validation_error`, `ASSET_NOT_FOUND` → `not_found`. Each mapping gets a test; none is invented at
call time.

### 7.2 Deployment

`deploy/caddy/Caddyfile` already fronts two upstreams and already sets `flush_interval -1` on the
MCP path — the unbuffered-streaming requirement `pontis/docs/15-deployment.md` § What the reverse
proxy must do names as load-bearing. Adding the sidecar is a third `handle` block plus a
`forma-pontis.service` beside the existing units, and the WebSocket `Upgrade` must be forwarded for
the device socket.

Three things `docs/15-deployment.md` makes the host's responsibility and that this plan therefore
owes explicitly: the pairing-code rate limit at the proxy (the Control Plane's own limiter keys on
the peer address, which behind a proxy is the proxy's), browser `Origin`/cookie behaviour at the
public origin, and trusted-proxy/client-IP handling.

`forma.hintly.online` already terminates public-CA TLS on an Ampere box behind Caddy, which is the
topology PV-2 requires and the TLS path OQ-011 left unproven. **PV-2 touches production and is not
authorized by this plan**, and PV-2 is outside it — see §10's boundary note.

### 7.3 The base product stays usable without Pontis

ADR-023 Consequences and PV-1's own scope both require it, and Forma satisfies it structurally:
`forma-server` has no dependency on the sidecar. If `forma-pontis` is down, the editor, the REST
API and `forma-mcp` are unaffected; only the in-app chat and pairing are unavailable. A test asserts
this rather than a sentence claiming it.

---

## 8. Two doors, one definition

ADR-023 Decision 3 permits it explicitly: an application may keep operating another MCP server,
Pontis neither adapts it nor treats its catalog as authoritative, and ADR-022 generates the
provider-facing catalog from Pontis capabilities only.

So Forma ends with two agent-facing surfaces:

| | `forma-mcp` (existing) | Pontis capabilities (new) |
|---|---|---|
| Who reaches it | any MCP host, directly | a paired Runtime, Codex 0.156.1 |
| Credential | PAT or OAuth token | none reaches the model; delegation is server-held |
| Approvals | none | local policy + human approval |
| UI actions | none | typed, routed to a `UIClient` |
| Where the chat lives | the host's own client | inside Forma |

Neither obsoletes the other, and both must describe the same product. The risk ADR-022 opens by
naming — several catalogs that drift — is now Forma's to manage across its own two surfaces, which
is not a question either repository's documents answer.

Forma has unusually good machinery for it: `ts-rs` already generates TypeScript bindings from Rust
into `bindings/generated/`, and `scripts/check-bindings.sh` already fails CI when they drift. A
descriptor generated from the same Rust source that defines `DesignOperation` would make one
definition enforceable rather than aspirational.

That is Forma-internal architecture, so it is mine to decide rather than the owner's to
adjudicate, and it is not a Pontis requirement. It is decided in §10(b) D5 and scheduled as part of
step 6.

---

## 9. Test scenarios and evidence

PV-1 §5 fixes the required shape. Mapped onto Forma:

| PV-1 requirement | Forma scenario |
|---|---|
| bounded read | "Which projects do I have, and what is the brand of this one?" → `project.list`, `brand.get` |
| low-risk write | "Raise the headline to 96pt" → `design.applyOperations`, one batch, `write` |
| approval-requiring mutation | a batch carrying `RemoveLayer`, approved at `once` scope |
| typed UI action | "Show me the layer that overflows" → `design.validate`, then `ui.selectLayer` |
| FakeProvider and real Codex | the same capability definitions, unchanged, on both |
| user / role / context isolation | bounded to what Forma supports; the limitation is recorded (§5) |
| idempotency | one `invocation_id` replayed returns the first result and commits once — needs the ledger of §10(b) D6 |
| denial / expiry / cancellation / replay | approval refused, expired, cancelled mid-turn, and re-delivered |
| base app usable without Pontis | §7.3 |
| the agent understands its role | it uses capabilities for design work and declines unrelated general-purpose tasks — ADR-027's product-behaviour evidence |

Forma's existing layers carry most of this without new infrastructure: `cargo test --workspace`,
the Playwright walkthrough in `e2e/` for the browser half, and `apps/mcp/tests/agent_flow.rs` for
the agent half. The new layers are the sidecar's own tests and the Boundary F suite.

Evidence lives in this repository per ADR-023 Decision 4. Pontis receives sanitized
reusable-platform outcomes only, and no Forma URL, credential or user data enters its documents.

---

## 10. Three kinds of thing, kept apart

This section replaces an earlier single list that mixed them, which made a Forma product choice look
like a Pontis requirement and a settled contract look like an open question.

### (a) Pontis contracts that already determine the design

Nothing here is open. Each row is decided by a document, and the design follows it.

| Determined | By |
|---|---|
| Capabilities bind to existing domain services, never to storage | ADR-010; `crates/application` is already that layer |
| One canonical definition per operation; `resource`, `command`, `ui` | `docs/06-capability-system.md` |
| Every `command` carries presentation metadata or fails registration | `docs/06` § Presentation metadata |
| The Runtime generates the session MCP catalog: one same-named tool per authorized capability, no generic dispatcher, shell, URL or endpoint | ADR-022; SEC-INV-016 |
| Forma's own MCP server may continue, is not imported and is not Pontis authority | ADR-023 Decision 3 |
| The sidecar shape, with Gateway, delegation issuer, Control Plane and Realtime co-located | ADR-023 Decisions 1–2; ADR-016 |
| Boundary F must authenticate the sidecar, bind user and context from trusted context, re-run host authorization, and expose no generic proxy | ADR-023 Decision 2 |
| `application_context_id` is host-selected, opaque, required, session-immutable, never a model argument | ADR-024 Decisions 1–3 |
| Durable history splits into a principal stream and one per application context | ADR-024 Decision 4 |
| `UIClient` eligibility requires matching user *and* application context | ADR-024 Decision 2 |
| `ApplicationAgentGuidance` is required, bounded to its six fields and 16 KiB, digest-bound and session-immutable | ADR-027 Decision 2 |
| `capability_only` is the Runtime-selected secure default; the application cannot grant local authority | ADR-027 Decisions 1, 3 |
| A neutral ephemeral per-session workspace | ADR-027 Decision 4 |
| Approval scope for PV-1 is `once`, keyed on the `invocation_id` | `docs/06` § Approvals |
| Real-provider support is Codex CLI `0.156.1` on `linux/x86_64` | `docs/07-provider-system.md`; accepted MP-6 evidence |
| Capability results are structured JSON validated against the output schema; media capabilities are not activated | ADR-026 Decision 2; `pontis/crates/runtime/src/mcp.rs` § `project_result` |

And what Forma already satisfies without new work:

| Requirement | What satisfies it |
|---|---|
| domain-service reuse and no direct storage path | `crates/application`, the only mutation path; `forma-mcp` has never been allowed to bypass it |
| context never taken from model input | `AuthContext::account_id`, already documented and implemented as a resolved boundary |
| one atomic unit per invocation, one version increment | the guarded UPDATE and `MAX_OPS_PER_BATCH = 20` |
| explicit resolved identifiers rather than filters | `DesignOperation` carries layer and page ids |
| typed, stable error codes | Forma's §32 envelope, mapped in §7.2 |
| unbuffered streaming through the proxy | `flush_interval -1`, already in `deploy/caddy/Caddyfile` |
| base app independent of the agent path | `forma-server` has no dependency on the sidecar |
| public-CA TLS on a real domain | `forma.hintly.online`, already deployed |
| the agent needs no shell or filesystem | it never had one |

### (b) Host-side decisions

These are Forma's, not Pontis's: Pontis states the obligation and deliberately leaves the mechanism
to the host, or does not speak to the question at all. The owner delegated these, and D1 and D2 were
reviewed by an independent second reading before being settled.

#### Decided

**D1 — Boundary F is authenticated by a dedicated service credential with its own `AuthKind`.**

`apps/server/src/auth.rs` dispatches on the credential's **prefix alone**: deployment token, then
`forma_pat_`, then JWT, in mutually exclusive arms whose ordering is itself documented as what keeps
the classes from being confused. So reusing a PAT here does not change the sidecar's credential — it
changes what the string `forma_pat_…` *means server-wide*. Every PAT already minted, by any owner,
would become a credential whose blast radius on leak is "any account, any user, any context" instead
of "one account", without anyone re-issuing or re-reviewing one. Gating that on a flag on `ApiKey`
is this same decision with a worse name and a shared prefix.

A PAT is also not identity-free: `auth.rs` sets `user_id: key.created_by_user_id`, the subject who
minted it, permanently. A sidecar serving several people would stamp every request with that one
person — which is the audit trail ADR-024 exists to keep honest.

The dedicated credential gets its **own prefix**, so the arms in `require_bearer` stay mutually
exclusive by construction. It reuses what PATs already proved: blake3 hashing, the prefix as an
index hint, constant-time comparison, revocation. And a new `AuthKind` variant forces the
exhaustive match in `routes/helpers.rs` to make a reviewed decision about what §61's history
records, rather than inheriting one.

*Accepted cost:* a fourth credential class in a file whose header enumerates three, a new
minting and revocation path, and churn in `bindings/generated/AuthKind.ts`. That cost is additive
and local; option A's cost is a silent redefinition of a credential already in circulation.

*Standing debt, not an objection to this decision.* Neither repository has settled its credential
model — Forma's grew one class at a time and Pontis has not fixed its side either, so this is
plumbed rather than designed, deliberately and with the owner's agreement. A fifth class added the
same way is where the arms stop being obvious by reading. The pass that should eventually order it
is recorded in [plan-status.md](plan-status.md) § *Credentials and identity need a deliberate pass*,
scheduled after PV-1 so it is designed around the classes that turn out to be load-bearing rather
than the ones predicted.

*Two requirements, or this is worse than the alternative it replaced.* The service arm must **refuse**
a request that carries no represented-user and context inputs, and must **refuse** one carrying
`x-forma-entry`. Otherwise the credential alone is full account access, and `forma-mcp`'s `admits()`
— which probes `/api/v1/status` with no such inputs — would admit a leaked sidecar credential at the
`/mcp` door. Both are tests, not comments. Note also that `AuthKind` is exported with
`rename_all = "snake_case"`, where `McpOAuth` becomes `"mcp_o_auth"`; pick a variant name that does
not mangle.

**D2 — `forma-server` resolves the session; the sidecar asks it.**

The decisive argument is not "one verifier", which is what an earlier draft of this plan said. It is
that **the alternative does not remove the hop it exists to remove.** Pontis requires two
resolutions, not one: `getAuthenticatedUser(request)` *and* the authorized application context
(`pontis/docs/12-sdk.md` § Adapter boundary), and ADR-024 Decision 1 requires the host to verify the
user may *enter* the selected context at session creation. That context is Forma's `AccountId`,
which is a row in `account_memberships` — only `forma-server` can answer it, and the sidecar is
forbidden the database. So a sidecar that verified the JWT itself would verify it in TypeScript
**and still call `forma-server`** for membership: a second implementation of `jwt.rs` bought for
zero saved round trips.

One endpoint answers both halves at the existing choke point. The precedent for its shape is already
in the repository: `ServerClient::admits` in `apps/mcp/src/client.rs` asks `forma-server` and reads
the status code rather than verifying anything itself.

What a second implementation would have to re-derive correctly, in another language, each with a
recorded reason and a permissive default to drift toward: `alg` pinned to ES256 and read only in
order to refuse; `validate_aud = false` plus a hand-rolled two-shape audience check whose agent form
must match the token's own `client_id`; `email_verified` read from the top level and never from
`user_metadata`; 5s leeway instead of the library's 60; and a JWKS refresh floor where an empty key
set does not clear the cache.

*Accepted cost:* `forma-server` sits on the authentication path of every session start — an extra
hop, extra latency, and a hard dependency. It also cuts against §7.3: with `forma-server` down the
sidecar cannot resolve a user at all. Accepted, because there is nothing useful a resolved user
could do while the domain service is unreachable — every capability binds to `crates/application`
over that same hop. The resolution is cached for the session, which ADR-024's immutable binding
permits, and authorization still re-runs per invocation, which ADR-024 requires regardless.

*Interaction:* D2 makes D1 close to forced. The resolve call is one request carrying two credentials
— the sidecar's own, and the subject token being asked about — and `presented()` reads exactly one
`Authorization` header. A dedicated arm ("service credential in `Authorization`, subject token in a
named header, verified by the same `Verifier`") fits that structure; growing the PAT branch a second
credential check is where the class separation starts to blur. The sidecar must also **not** set
`x-forma-entry` on that call, since a browser session token is verified with `Accepts::Either`.

**D5 — One definition across Forma's two agent surfaces, generated from one Rust source.**

Pontis was asked directly and answered that two independently authored catalogs — one Rust/MCP, one
Pontis TypeScript — are not compatible with "define once". That reading is stronger than the text
supports. ADR-023 Decision 3 says "one canonical **Pontis** capability definition per operation it
chooses to expose", and the same decision then permits an application to keep operating another MCP
server, stating that Pontis "neither adapts it nor treats its catalog as authoritative". The scope
of define-once is the Pontis catalog. How Forma keeps its own two surfaces aligned is Forma's
problem, exactly as this entry originally said.

The recommendation is adopted anyway, on the engineering argument rather than the contractual one:
nineteen operations authored twice by hand will drift, and a drifting agent contract is worse than
no contract because the agent follows it confidently and is wrong. That is the documented reason
`apps/mcp/tests/skill_matches_the_code.rs` exists, and the same reasoning applies here with more
surface area.

So: one descriptor source in Rust — id, description, input schema, kind, risk, presentation
metadata — with `forma-mcp` and the generated sidecar descriptor both deriving from it, and a
conformance test failing CI when either drifts. `authorize`/`execute` bindings stay hand-written in
the sidecar; they are genuinely application code. Pontis does not validate how the generation works,
and does not need to.

The disagreement is recorded rather than escalated because it changes no built artifact. If Pontis
later holds the stronger reading as contractual, this implementation already satisfies it.

*Schedule effect:* `apps/mcp/src/server.rs` authors the catalog today — name and description in each
`#[tool(...)]` attribute, schema derived from its `Parameters<T>` struct, fourteen times. Re-pointing
it at the registry is real work and belongs in step 6 rather than beside it.

**D8 — The editor speaks the three UI routes itself; it does not depend on `@pontis/react`.**

`pontis/packages/sdk-react` ships `PontisClient` and `useAgentUIAction`, and
`pontis/docs/12-sdk.md` is explicit that the package is "a wrapper, not the contract" — canonical UI
action registration must not require React, and the reference frontend is plain HTML precisely to
keep proving it. Taking the wrapper would mean `apps/web` resolved a dependency through the one seam
`apps/pontis-sidecar` keeps into the private Pontis checkout. §7.3 says the base product stays
usable without Pontis, and that has to include *building*: the editor is the base product, and a
build that fails without a Pontis checkout present would break the promise in the most basic way
there is. The sidecar may depend on that checkout because the sidecar *is* the integration; the
editor may not, and keeping that seam in one place is itself a tested rule.

What the editor implements instead is small and fully specified by
`pontis/packages/sdk-server/src/ui-transport.ts`: three paths
(`/pons/v1/ui/stream`, `/pons/v1/ui/actions`, `/pons/v1/ui/ack`), the `ui.connected` and `ui.action`
frames, and the four ack outcomes. `apps/web/src/pontis/channel.ts` is that client, and
`apps/pontis-sidecar/src/ui.test.ts` drives the real routes so a drift in any of the three shows up
as a failing test rather than a dead channel in a browser.

**D9 — The UI channel is same-origin, and the stream carries its bearer in a header.**

Two constraints meet here. The transport refuses a cross-origin `Origin` and validates `Host`
against the same declared list, which is its DNS-rebinding defence; and Forma's browser session is a
provider JWT that `api.ts` sends as `Authorization: Bearer`, which `EventSource` cannot set at all.

So the channel is same-origin through whatever already fronts the editor — Caddy routes
`/pons/v1/ui/*` to `127.0.0.1:4767` in the deployment, Vite proxies it in development — and the
sidecar declares that one public origin. Nothing new is exposed: the sidecar stays on loopback and
the proxy was already the only thing the internet talks to.

And the stream is read with `fetch` plus a `ReadableStream`, not `EventSource`, so the bearer travels
in a header. The alternative was a token in the query string, which `pontis/docs/13` does for the
reference app's stand-in for a session cookie; a real provider JWT in a URL lands in access logs and
in `Referer`, and "never log tokens" is an invariant Forma states in as many words. The cost is a
dozen lines of SSE framing, which is the smaller price.

In `FORMA_AUTH_MODE=local` there is no sidecar and no channel: `/service/resolve` needs a subject and
an account, and local mode has neither. The editor connects nothing and the four `ui.*` capabilities
are simply not offered.

#### Still open

**D3 — Whether `design.delete` becomes a capability. Not required by PV-1.**
[docs/plan-status.md](plan-status.md) item 5 deliberately gave the agent no delete tool: "deciding
that a piece of someone's work should stop existing is not the agent's call." Pontis changes the
premise — an approval is exactly a human making that call with the target in front of them. But
PV-1's approval-requiring mutation is already satisfied by `design.applyOperations` carrying a
`RemoveLayer`, so this is a Forma product decision with no schedule pressure on it.

**D4 — Whether Forma grows a second role. Not required by PV-1.**
See §5. PV-1's isolation evidence is bounded to what the app supports. If it is ever built it will
be for Forma's own product reasons, not to satisfy a gate.

**D6 — A host-side invocation ledger. Built in `307a74a` (step 4); kept here for its reasoning.**
ADR-023 Decision 2 requires the hop to carry idempotency identifiers, and
`pontis/examples/polyglot-host-fixture/src/main.rs` keeps the ledger **in the host**: the stored
result is returned on replay, and a different payload under the same key is refused with
`invocation_conflict`. Forma has no such table — `grep -ri idempot` over `crates/` and `apps/`
returns one unrelated comment. Replaying `apply_operations` with the same `base_version` after the
first succeeded answers `VERSION_CONFLICT`, an error, not the first result. That is the wrong shape:
idempotency means the replay returns the original answer.

This is new persisted work in `forma-server` and it was missing from the step list. It is mine to
design rather than the owner's to adjudicate, but it is recorded here because it changes the
schedule, and the gap stays invisible until the sidecar retries a timeout that actually committed.

**D7 — Deterministic account resolution. Built in `d1e819f` (step 1); kept here for its reasoning.**
See §5. Either constrain the data — a unique index on `user_id`, or `grant` refusing a second
account — or make resolution explicit and ordered. Which one depends on whether Forma ever intends a
user to belong to more than one account, which is D4's question, so the two should be answered
together. It has to be settled before any `ApplicationContextId` is bound, because ADR-024 makes
that binding immutable for the life of the session.

### (c) The visual loop, now answered and still waiting

**Resolved as a contract, not yet active.** This section used to record the visual loop as the one
functional blocker the accepted contracts did not address, and asked the owner to choose between
proceeding structurally and requesting a media review. Pontis ran that review: ADR-028 defines
bounded rich capability results and mediated artifacts.

What that settles:

- text and structured JSON reports are already supported, which is what makes the manual of §6.1
  possible at all;
- ADR-028's first activation adds **one bounded inline PNG**, and nothing else;
- PDF, audio, video, downloadable files and mediated references or URLs stay deferred to explicit
  packages, and no alternative route may be introduced to deliver them.

So the answer moved from "a capability result cannot carry a picture" to "it will, within a bounded
profile, when that profile activates." The structural and documentary integration may proceed now.
The visual loop and PV-1's exit both wait on `pv1_visual_ready`.

What this changes in how Forma writes for the agent, and it is the part worth carrying: **the manual
must distinguish an image it received from a mechanical validation, and must not claim that the
agent will never be able to see the render.** An earlier draft of ours did exactly that, and it
would have been wrong twice over — wrong as a permanent claim, and wrong as advice, because an
agent told it is blind forever stops asking.

The honest wording is door-neutral and true on both doors today: when a render reaches you, look at
it; when it does not, say so and lean on the validator rather than asserting the piece looks right.
That needs no conditional in the template and survives the activation without an edit.

Still not proposed, and still deliberately: base64 in an output schema, an improvised URL, a private
artifact handle, a fetch-by-id tool, or any other parallel path. ADR-022 and SEC-INV-016 exist to
prevent exactly that, and a workaround built while waiting is harder to remove than the wait.

### A boundary note, not a question

PV-2 mutates a production-like VPS with real DNS and TLS, and is outside this plan.
`forma.hintly.online` is the obvious candidate and already the right topology. I will not touch it,
and I will ask for authorization in as many words when PV-1 passes.

### Recorded skew

Pontis requires Rust 1.88+ and pins `rmcp ~3.2.0`; Forma's workspace is on `rmcp 3.1`. Separate
workspaces, so not a conflict today. Written down so it is not discovered later if anything is ever
shared.

---

## 11. Order of work

### Progress — the live state, kept here so any agent can pick the work up

Read this before the list below; the list is the plan, this is what happened to it.

| Step | State | Where |
|---|---|---|
| 1 Deterministic account resolution (D7) | done | `d1e819f` |
| 2 Service credential (D1) | done | `78c0209` |
| 3 Resolve endpoint (D2) | done | `6209edf` |
| 4 Invocation ledger (D6) | done | `307a74a` |
| 5 Cancellation on validate | done | `3fe5d64` |
| 6 Descriptor registry (D5) and Boundary F bindings | done | `d40d763`, `dadfcb2` |
| 7 Sidecar composition, local run against FakeProvider | done | `c0c7744` |
| 8 Manual and guidance (§6.1) | done | `ee7be64` |
| 9 UI capability handlers (D8, D9) | done | `c852e91` |
| 10 Device panel and in-app chat | **done** — 10a, 10b, 10c merged; one known limitation below | merges `142688b`, `fa428d8`, `e63dd5d` |
| 10a Device panel and proxy routes | done, merged into `main` | `70dae4e`, `d6b000c`, `ad74373`, `f37b358`, `bd4cd8b`, `6edc1b6` (branch `JonathanEKP/pontis-10a-devices`) |
| 10b In-app chat and `/agent/context` | done, merged into `main` | `93dde1a`, `58b754f`, `f081782`, `0a34a10`, `005d9d0` (branch `JonathanEKP/pontis-10b-chat`) |
| 10c Live delivery and device presence | done, merged into `main` | `f501cc9`, `ce96b88`, `93731c2`, `821e33d` (branch `JonathanEKP/pontis-10c-live`) |
| 11 Real Codex local run | **done** — Codex 0.158.0, `capability_only`, `gpt-6-luna` at low effort; posture verified 7/7; S1, S2, S6, S7, S8 and S13 passed on real turns (7 turns, one retry spent strengthening S8). What was *not* run is its own section in [pontis-step11-real-codex.md](pontis-step11-real-codex.md). Merged into `main` | `bc72278`, `d3d95c9`, `19c633c`, `748e862`, `7383789`, evidence `c089a10` |
| Browser testing harness | **done** — `apps/pontis-sidecar/src/browser-stack.ts`: an isolated instance per number (ports `4800 + 10N`), fake identity provider, two seeded accounts, Runtime on FakeProvider or real Codex; proved in the Orca browser with a real Codex answer. How to use it: [pontis-browser-testing.md](pontis-browser-testing.md); testers' findings: [pontis-browser-findings/](pontis-browser-findings/README.md). Building it found and fixed two product bugs that kept the chat from working in a browser (Vite proxy, Codex auth `unknown`), see [00-harness.md](pontis-browser-findings/00-harness.md) | branch `JonathanEKP/pontis-b0-browser-harness` |
| Browser break-testing (T1–T5) | **done** — five testers, ~265 cases, 61 findings (11 high), no cross-account path and no leaked secret. Grouped by cause and ordered for fixing in [pontis-browser-findings/SUMMARY.md](pontis-browser-findings/SUMMARY.md); none fixed yet | merges on `main`, 2026-10-04 |
| Fix batch 1 (summary groups 1–5, open-design context) | **specified, not dispatched** — the agreed spec, decisions and task split (A chat, B windows, C durable state, D credential, E re-test) are in [pontis-fix-batch-1.md](pontis-fix-batch-1.md). Merge order D → A → B → C | |
| 12 PV-1 evidence | not started; follows 11 | |

**Step 11, as dispatched (2026-10-03).** The owner authorized a real run on their Codex account
and asked for the cheapest test model, `gpt-6-luna`. Two facts shaped it:

- **The installed Codex is `0.160.0`, which Pontis refuses.** `pontis/docs/07-provider-system.md`
  accepts `capability_only` only on exact rows — `0.154.0`, `0.156.1`, `0.158.0` on linux/x86_64 —
  and fails closed with `unsupported_feature` before spawning anything else. So the run uses
  **`0.158.0`**, the newest accepted row, installed into a gitignored folder under the repo for
  this run only; the owner's global `codex` is not touched. Adding `0.160.0` to the matrix is
  Pontis's process (deterministic contract plus a real run, then accepted), not Forma's.
- **The Codex driver has no model option.** The model comes from Codex's own config, so the run
  gets a dedicated `CODEX_HOME` holding `model = "gpt-6-luna"`, low reasoning effort and a symlink
  to the owner's `~/.codex/auth.json`: same account, and the owner's own config (`gpt-6.1-sol`)
  is never edited. The driver's `with_executable`, `with_environment` and
  `with_reasoning_effort` are the seams.

The run is opt-in — gated on an environment variable, never part of `pnpm test` or CI — because it
spends the owner's tokens. Budget: one real turn per scenario, at most one retry, and the worker
stops and asks rather than looping. Transcripts are kept as evidence for step 12, with no token,
header or credential in them.

**Open from the browser harness (2026-10-04).** Pontis's Codex probe never reports authentication
(`authenticated: false`, `authMode: unknown` for every installed Codex — App Server has no supported
auth-status query), so the chat treated every Codex as signed out and could not send. Forma now treats
`unknown` as usable and says "Sign-in not verified" (`76f2fd0`); a truly signed-out Codex fails at
`session.start`. That is a Pontis-side observation to raise later — the Pontis repository was not
touched. Also open: the chat's agent is not told which design is open and guesses ids
([F-00-4](pontis-browser-findings/00-harness.md)).

**Where it stands after step 10 (2026-10-03).** Everything up to step 10 is on `main` and green:
161 web tests, 62 sidecar tests including the FakeProvider local run, and S11. What no test has
done is drive a real browser against a live sidecar with a real Runtime — that is step 11. Known
limitation, accepted for now: the chat's state lives in `Editor`, so it resets when the person
leaves a design; lifting it to `App` is small and waits until someone uses the chat in anger.
**Step 11 needs the owner** — a real Codex install and its credential on their machine — so the
coordinator stopped here rather than start it.

**Step 10, as dispatched (2026-10-03).** Run by a coordinating agent through Orca, each worker in
its own worktree, merged into `main` as it lands. Split so the two never edit the same file:

Branches `JonathanEKP/pontis-10a-devices` and `JonathanEKP/pontis-10b-chat`, worktrees under
`/home/vertex/orca/workspaces/forma/`, Orca run `run_a75a43683949`. Workers are Claude Sonnet 5.5 at
high effort; the coordinator reviews and merges each branch into `main` and records the commit here.

- **10a — device panel**, on the Agents page beside the existing MCP connection UI, plus the
  proxy routes every browser-facing Pontis path needs (Caddy and Vite). Owns `Agents.tsx`,
  `deploy/caddy/Caddyfile`, `apps/web/vite.config.ts`, `apps/web/src/pontis/devices*`.
- **10b — in-app chat**, in the editor: intents out, the event stream in, approval cards from
  presentation metadata, and UI navigation shown as its own kind of line. Owns
  `apps/web/src/pontis/chat*`, `apps/web/src/pontis/events*` and the editor mount point.
  **10b state: built and tested, not yet seen in a browser against a live sidecar.** `93dde1a` adds
  `GET /agent/context` to the sidecar (the editor cannot learn its `applicationContextId` any other
  way, and `/pons/v1/events` refuses a context the caller does not name — the proxy needs that
  route too), `58b754f` the event reader, `f081782` the intents and transcript, `0a34a10` the panel
  and its mount. Two limits worth knowing: `/pons/v1/events` is a polled JSON page, so the reply
  arrives whole as `agent.output.completed` and there is no token-level streaming (a live tail would
  need a delivery channel the sidecar does not have), and no durable event echoes what the person
  typed, so a reload restores the agent's side of a conversation and not theirs.

Decisions taken for step 10, low-risk and delegated:

- **"Replacing the Agents page" does not remove the MCP door.** §8 is explicit that neither door
  obsoletes the other, so the PAT and OAuth-grant UI stays; the page gains the device panel. The
  chat lives in the editor, where the person is looking at the work it acts on.
- **D8 holds for both surfaces.** Neither depends on `@pontis/react`; both reuse
  `apps/web/src/pontis/channel.ts`'s `frames` rather than writing a second SSE parser.
- **The editor learns its `applicationContextId` from the sidecar, `GET /agent/context`.** 10b
  found that nothing exposed it, and the Control Plane's event page requires it for every session
  event. Served by the sidecar rather than forma-server because the sidecar is what binds the
  session's context: the editor gets exactly that value, not a second derivation that could
  disagree. Built by 10b; proxied by 10a. (Also corrected: `/pons/v1/events` is a polled JSON page
  with an `after` cursor, not SSE.)
- **Open from 10a: presence.** `GET /pons/v1/devices` returns the stored Device record, which has no
  `online` (SEC-INV-009: presence is never durable), and the sidecar does not merge it in the way
  Pontis's reference app does (`remote-server.ts`, `online: devices.isOnline(id)`). The panel shows
  "presence not reported" rather than guess; it renders online/offline the moment the response
  carries a boolean `online`. Merging it in is a sidecar change (`isOnline` is already used by
  `intents.ts`), left to whoever owns `apps/pontis-sidecar`. `caddy validate` was not run (no Caddy
  on the dev machine); `deploy/install.sh` does it on deploy.
- **10c — live delivery and presence, found in review of 10a and 10b.** 10b polls the durable event
  page and concluded the agent's reply could not stream. It can: Pontis's reference app
  (`examples/reference-app/src/bin/remote-server.ts`) fans each Device notification out to the
  person's own tabs with `uiClients.notifyContext` / `notifyPrincipal`, which arrives on the UI
  stream of step 9 as a `pons.event` frame — `agent.output.delta` included, unsequenced because it
  is never persisted. Forma's `sidecar.ts` builds its `DeviceGateway` without that fan-out. The
  UI stream is delivery, the event page stays history: a frame with a `sequence` advances nothing
  on its own, and a gap still resyncs from the page. The same review found the device panel's
  "presence not reported": the sidecar does not merge `devices.isOnline` into `/pons/v1/devices`
  the way the reference app does. Both are sidecar wiring the reference app already shows, so they
  are one task, given to the 10b worker because it holds the chat's model.
  **10c state:** `apps/pontis-sidecar/src/live.ts` does both. Stored events are fanned out from
  `controlPlane.events.subscribeAll` with their `sequence` (so a runtime's redelivery is never sent
  twice), deltas and presence from the gateway's `onNotification` only to the context the session
  was started in, after the reference app's three checks; `/pons/v1/devices` is answered ahead of the
  Control Plane's route with `online` from `devices.isOnline`. In the editor the channel forwards
  `pons.event` to a feed, the chat offers sequenced frames to the event reader (same cursor, same
  rules, a skip re-reads from the cursor) and shows `agent.output.delta` in a transient slot that
  `agent.output.completed` replaces; the page is read every 10s instead of 1.5s while the stream is
  connected, and at once whenever that changes. Frames are trimmed to position, scope, type, session
  and payload — the store's id, stream key and origin message id are not sent. Every public API the
  reference app uses was exported, so nothing was invented. Also noted, not fixed:
  the chat's state lives in `Editor`, so it resets when the person leaves a design.
- **Only browser routes go through the proxy:** `/pons/v1/pairing/codes`, `/pons/v1/devices*`,
  `/pons/v1/events`, `/pons/v1/jobs*`, `/agent/intent`, `/agent/context`, `/pons/v1/ui/*`. The Runtime's own routes
  (`pairing/claim`, `device/*`) are a remote Device reaching the sidecar, which is PV-2 and not
  authorized by this plan.


D1 and D2 are settled, so nothing blocks the start. D7 blocks step 3 and D6 blocks step 8; both are
in the list rather than waiting outside it. The product blocker in (c) does not block any step — it
bounds what the finished thing can be claimed to do.

1. **Deterministic account resolution** (D7). Constrain or order it, with a test that two
   memberships cannot produce an arbitrary winner. First, because every later step binds a context.
2. **The service credential** (D1): a new `AuthKind`, its own prefix, blake3 hashing and
   constant-time comparison reusing the PAT machinery — plus the two negative tests, that the arm
   refuses a request with no represented user and refuses one carrying `x-forma-entry`.
3. **The resolve endpoint** (D2): subject, account and role from one authenticated call, verified by
   the existing `Verifier`, with the sidecar cached for the session.
4. **The host-side invocation ledger** (D6), keyed on the invocation scope, returning the stored
   result on replay and refusing a different payload under the same key.
5. **Cancellation on the operations Boundary F can abort**, starting with `design.validate`, which
   is the cheapest to call and the most expensive to serve.
6. **The descriptor registry in Rust** (D5), then the capability descriptors and their Boundary F
   bindings, with the error mapping and its tests. Re-pointing `apps/mcp` at the registry and its
   drift guard are part of this step, not a follow-up.
7. **The sidecar composition** and its local run against `FakeProvider`.
8. **The manual and the guidance** (§6.1): the template, the generated MCP artifact and its
    widened drift check, `forma.guide` with its Boundary F route, and bounded guidance drafted for
    review. No distillation — the budget bounds the opening context, not the agent's knowledge.
9. **The UI capability handlers** in the editor.
10. **The device panel and in-app chat** replacing the Agents page.
11. **The real Codex local run**, against whichever release the evidence matrix accepts when this
    step is reached. `0.156.1` is the only accepted row today and the owner tracks releases ahead of
    it, so the version is settled with Pontis rather than fixed here.
12. **PV-1 evidence** in this repository; sanitized platform outcomes to Pontis.

Steps 1 to 5 are all `forma-server` work in Rust and none of them needs the sidecar to exist, so
they are testable before any TypeScript is written.

The scenarios that prove each step are in [pontis-pv1-e2e.md](pontis-pv1-e2e.md). S11 — the base
product with Pontis absent — is written first, ahead of step 1: it needs nothing from this plan and
it guards a promise Forma is already making.
