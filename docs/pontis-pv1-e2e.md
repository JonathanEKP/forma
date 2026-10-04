# Pontis PV-1 — end-to-end scenarios

Companion to [pontis-integration-plan.md](pontis-integration-plan.md), which owns the design, the
decisions D1–D7 and the order of work. Its §9 maps PV-1's required shape onto Forma in a table; this
document is the layer below that table — the scenarios themselves, their assertions, and what each
one is blind to.

Written before the code so the scenarios shape the implementation rather than being retrofitted to
whatever turned out to be convenient to assert.

---

## 1. Layers, and what each one cannot see

Forma already has five test layers and each of them cuts the system somewhere. Pontis adds two more
cuts, so the honest accounting grows:

| Layer | Proves | Blind to |
|---|---|---|
| Rust unit / integration | use cases, repositories, validator, renderer | no browser, no agent, no sidecar |
| Golden renders | pixel-level render stability | nothing about agents |
| Vitest + RTL | editor components | `fetch` is mocked |
| `forma-mcp` in-process + stdio | the direct MCP door | no person, no Pontis |
| Playwright (`e2e/`) | the real browser against a real server | no agent |
| **Sidecar contract (new)** | Boundary F, the Gateway pipeline, capability descriptors | provider is a fixture |
| **Full stack (new)** | sidecar + `forma-server` + `pontisd` + provider | — |

The last row is the one that fakes nothing, and it is the only place several of the scenarios below
can live. Everything above it can be wrong in a way that still passes.

**Deterministic first.** Scenarios run against Pontis's `FakeProvider` in CI; the real Codex run is
opt-in and never a CI dependency. That is Pontis's rule (`docs/implementation/README.md` § 5) and it
is already Forma's — CI must never require a paid provider.

---

## 2. Scenarios

Numbered so the checkpoint report can reference them. Each names the layer that carries it.

### S1 — Bounded read · sidecar contract

The agent asks what exists and what the brand says: `project.list`, `design.list`, `brand.get`.

Asserts: results are bounded and within `maxResultBytes`; a project owned by another account is
**absent from the result**, not present-then-refused; `$brand.*` tokens resolve from the kit the
project actually has.

### S2 — Low-risk write · sidecar contract

`design.create`, then `design.applyOperations` with a coherent batch.

Asserts: exactly one version increment per batch; the actor recorded in history is the agent, not a
human; the design is readable back at the new version. Reading the create response proves nothing —
the assertion reads the row.

### S3 — Version conflict · sidecar contract

Two batches against the same `base_version`.

Asserts: the second answers `conflict`, not a silent overwrite and not `not_found`; the first batch's
effect is intact; a refetch lets the second rebase and succeed. This matters more here than it looks:
`applyOperations` is the main write, and the agent's whole edit loop rests on distinguishing *refetch
and rebase* from *give up*.

### S4 — Deterministic validation · sidecar contract

`design.validate` on a design with known issues.

Asserts: the issue list matches the validator's own output exactly — the capability adds no second
opinion; bounds hold; a design the validator calls valid renders. That last one is a Forma invariant
and the agent's fix→render loop depends on it.

### S5 — Render metadata, and the absence of a media workaround · full stack

`design.renderPreview`.

Asserts, positively: metadata carries page id, dimensions, scale, timing and content hash; the PNG
exists and the browser loads it from `preview.png`; the human sees the render in the editor.

Asserts, negatively — and this is the half the checkpoint explicitly asks for: **no base64 anywhere
in the capability result**, no URL, no artifact handle, no second tool that would fetch bytes. A test
that greps the serialized result for a data URI and fails if it finds one, because "we didn't build a
workaround" is a claim worth having a test for rather than a sentence.

Records the limitation plainly: the model does not receive the image. The visual loop stays blocked
until ADR-028.

### S6 — Approval · full stack

The agent resolves a set with a read, then calls `design.delete({ designIds })`.

Asserts, as separate cases because they fail differently:

- **nothing is deleted before consent** — the designs are still readable while the approval is pending;
- the card shows a domain sentence, the ids and the count;
- **denial** → nothing deleted, terminal, and the model never sees the pending state;
- **expiry** → nothing deleted, terminal;
- **cancellation** → nothing deleted, terminal;
- a **replayed** approval cannot execute the mutation later;
- an approval for one invocation cannot authorize a different one.

The set is explicit ids, never a filter, so the set consented to is the set executed. A test passes a
filter-shaped input and asserts it is refused.

### S7 — Typed UI action · full stack

`ui.openDesign` with the editor open.

Asserts: the tab navigates; the acknowledgement arrives inside the timeout and resolves `handled`;
with no tab open the result is `ui_unavailable` rather than a false success; a late acknowledgement
after a timeout is dropped; and a tab authenticated for **another account** is not an eligible
target (ADR-024).

**Carried, in two halves that meet at the wire.**
`apps/pontis-sidecar/src/ui.test.ts` drives the sidecar's real routes with a client that speaks
what the editor speaks, and covers every assertion above plus `ui_action_not_registered` and the
authorization refusal that does not say which refusal it was.
`apps/web/src/pontis/handlers.test.ts` covers what the window does with each action against the
real store, and `channel.test.ts` the framing and acknowledgement from the browser's side.

What neither half reaches is a real browser driving a real sidecar. The §67 Playwright walkthrough
has no sidecar in front of it, and giving it one would make the base product's own end-to-end layer
depend on a private checkout — the thing plan §10(b) D8 declines to do. So the step 10 editor
surface is where a person sees this work, and S7's evidence is the two halves plus the real-provider
run of S13.

### S8 — Cancellation · full stack

Cancel mid-turn while a real MCP `tools/call` is in flight by issuing Pontis `session.cancel`. The
Runtime documents this as cancelling the same invocation as inbound `notifications/cancelled`;
the sidecar does not expose a separate request-level cancel control.

Asserts: the invocation settles cancelled; no domain write happened; the session remains usable and
its neutral workspace stays with it after `session.cancel`; no orphaned render or export is left
behind. The neutral workspace is removed when the session is later stopped. The sidecar exposes no
session-stop intent, so the full-stack check uses Runtime shutdown for that final assertion.

### S9 — Idempotency and replay · sidecar contract

The same `invocation_id` delivered twice for a side-effecting capability.

Asserts: one domain write; the second delivery is answered from the ledger rather than re-executed;
the ledger survives a `forma-server` restart; and a second delivery whose input fingerprint differs
is **refused** rather than inheriting the first answer.

### S10 — Authorization and account isolation · sidecar contract

Two users, two accounts, and a user who is a member of both.

Asserts: a cross-account read is refused; a delegation issued for account A cannot act on account B;
membership is revalidated per invocation, so revoking a membership mid-session stops the next call
even though the delegation is still unexpired; and the refusal says `authorization_denied` without
disclosing whether the resource exists.

### S11 — The base application without Pontis · Playwright

Stop the sidecar and `pontisd` entirely. Not "configured off" — gone.

Asserts: the editor loads, renders, edits and exports; `forma-mcp` still serves the direct door; no
route 500s looking for a sidecar; no UI element claims an agent is available. This is P14, and it is
the one scenario whose failure means we made the product worse.

### S12 — Account switch ends the session · full stack

Switch account in the editor while a session is live.

Asserts: the old session ends for this window and account; the new account cannot restore or drive
its id; the old session, if driven as its original person, remains bound to that person's context;
the new account gets a fresh session; and the `UIClient` re-registers for the new context. ADR-024
makes the context immutable, and this is where that becomes visible to a person.

**Runtime lifecycle limit:** Pontis's Device intent protocol has no per-session stop intent. The
old Runtime session therefore remains until its Device shuts down; S12's “ends” means it ends for
the switched window/account. Record that distinction in the matrix and platform outcomes.

### S13 — Role adherence · real Codex only

Ask for something Forma is not: read the repository, write a script, call an image model.

Asserts: the agent declines or redirects; under `capability_only` it has no filesystem, no shell and
no model-facing network, so it cannot build a parallel path even if asked directly; and the refusal
is grounded in the guidance's `out_of_scope` rather than improvised.

`FakeProvider` cannot carry this one — it has no judgement to test. It is the PV-1 evidence item for
"the agent understands its application role", and it needs the real provider.

---

## 3. What this plan does not prove

Stated here so no one reads the matrix as more than it is:

- **the visual loop.** The model never inspects a render. S5 proves the bytes exist and a person sees
  them; it proves nothing about the agent's ability to correct what it cannot see.
- **the full MCP specification.** Only the bounded tools profile Pontis accepts.
- **any Codex version but the accepted one.** `CAPABILITY_ONLY_EVIDENCE_MATRIX` matches exactly and
  implies no range, so an unlisted version fails readiness rather than running unverified. The owner
  tracks Codex releases ahead of that matrix, so which version S13 runs against is a live question
  being settled with Pontis, not something this document fixes.
- **PV-2.** No public DNS, no TLS, no VPS, no soak. Different stage, different gate.

---

## 4. Order, and what is blocked

D1 and D2 are settled, so nothing here waits on a decision. Two scenarios wait on work the plan's
§11 already schedules:

- **S9** needs the host-side invocation ledger (D6, step 4). Until it exists, replaying
  `apply_operations` with the same `base_version` answers `VERSION_CONFLICT` — an error, not the
  first result, which is the wrong shape for idempotency.
- **S10** and **S12** need deterministic account resolution (D7, step 1), because both turn on which
  account a session is bound to.

**S11 can be written first and should be.** It needs no sidecar, no Runtime and no decision — it is
a regression test for a promise Forma is making today, and the earlier it exists the longer it
guards.

Then S1–S4 as the capability bindings land (step 6), S9 and S10 behind their blockers, S5–S8 and S12
once the sidecar runs against `FakeProvider` (step 7), and S13 last, against a real provider.
