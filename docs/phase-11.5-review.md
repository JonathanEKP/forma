# Phase 11.5, reviewed against the code

The Phase 11.5 document was written against `forma-plan.md` and
`docs/plan-status.md`, and is not yet in this repository. Those describe what Forma is meant to be, not what is in
the tree, so seven things it could not have known are resolved here. Nothing
below contradicts the phase's architecture: identity in Supabase, application
data in SQLite, the account boundary below the HTTP handlers. Two of the seven
turn out to be smaller than they looked, one is already forbidden by the plan,
and one is a decision the phase left implicit that would have been discovered
during implementation, which is the expensive place to discover it.

Each resolution names the file or line it rests on.

---

## 1. Who is allowed in

**The gap.** The phase enables Supabase magic-link registration and Dynamic
Client Registration and never says whether either is open. Supabase's own
documentation warns that DCR "allows unrestricted registration". A deployment
that anyone can sign up to is a different product from the one being built, and
this box is four Ampere cores with SQLite on a local disk.

**Resolution: signing in is not the gate; owning an account is.**

The tenant model already provides this and the phase nearly says so. §11.5.10:
"No membership means the token represents a valid Supabase user but not an
authorized Forma account; reject." A stranger who completes Supabase sign-up
holds an identity and reaches nothing — no project, no asset, no render, no
tool. Open registration at the identity provider is therefore not a data risk.

The single hole is §11.5.5's "a newly authenticated user who does not own an
account may create a new personal account instead". That sentence is what turns
identity into access. Close it:

```sql
account_invites
---------------
email               TEXT PRIMARY KEY NOT NULL
created_at          TEXT NOT NULL
claimed_at          TEXT
claimed_by_user_id  TEXT
```

Creating an account requires the authenticated user's verified email to match an
unclaimed invite. Everyone else keeps `ACCOUNT_REQUIRED`, worded as a state
rather than a failure: *this Forma is invite-only*.

Invites are minted on the machine, next to the backup subcommand
(`apps/server/src/main.rs:29`):

```
forma-server invite <email>
```

**The gate lives in Forma, not in Supabase's signup setting.** Forma owns
membership, and a client never mints server-owned identity — the same reason
`PUT /projects/{id}/brand` takes a `BrandKitDraft`. Relying on a provider
checkbox would put an authorization decision in a system that does not know what
an account is.

**Quotas are deliberately not in this phase.** With an invite gate, every person
who can consume the disk is a person who was invited by name. `DEFAULT_MAX_UPLOAD_BYTES`
(25 MB, `crates/assets/src/lib.rs:18`) still caps a single upload. A per-account
byte ceiling is real work with no evidence behind it yet; §100 says that waits
for the evidence. Revisit when a second account exists that the operator did not
personally invite.

---

## 2. Losing Supabase must not lose the workspace

**The gap.** `account_memberships.user_id` points at a subject that lives in
another system. The legacy claim is one-time and then `FORMA_REMOTE_TOKEN` stops
being accepted for anything. If the Supabase project is deleted, rotated to a
new ref, or migrated, every membership row points at a `sub` that no longer
resolves and **there is no way back into the data** — which is still sitting,
intact and readable, in `forma.db` on a disk the operator owns.

The phase has a one-time bootstrap and no second one.

**Resolution: a second subcommand, and one descriptive column.**

```
forma-server grant-owner <email>
```

It attaches an owner membership for an account, on the machine. Shell access is
the security boundary, exactly as it is for `forma-server backup` — a procedure
that already exists in this shape and needs no new trust.

For it to work, `account_memberships` stores the email alongside the subject:

```sql
account_memberships
-------------------
account_id
user_id        -- authoritative: the Supabase sub
email          -- descriptive: who this was, when granted
role
created_at
```

`email` is never used to authorize anything. It exists so a human reading the
table after the identity provider is gone can tell whose row is whose, and so
`grant-owner` can rebind an account to a fresh `sub` from a new Supabase
project. Recovery becomes: point `FORMA_SUPABASE_*` at the new project, sign in,
run `grant-owner`.

Without this, the phase's own §11.5.5 hardening — "reject every later claim
attempt" — is also the thing that makes the failure permanent.

---

## 3. `projects.account_id NOT NULL`, and the shape of every scoped query

**The gap.** Two decisions the phase states in one line each, both larger than
they read.

### 3a. SQLite has no `ALTER COLUMN`

`projects` today is `id, name, brand_id, created_at, updated_at`
(`migrations/sqlite/0001_init.sql:5`). Adding a `NOT NULL` column to a populated
table is not an `ALTER`; it is SQLite's documented table rebuild — create the new
table, copy, drop, rename — and the deployed database has real rows in it.

The migration is therefore ordered:

1. create `accounts`, `account_memberships`, `account_invites`, `api_keys`;
2. insert the legacy account row;
3. `ALTER TABLE projects ADD COLUMN account_id TEXT REFERENCES accounts (id)` — nullable;
4. backfill every existing project to the legacy account;
5. rebuild `projects` with `account_id TEXT NOT NULL`, keeping the table name so
   the existing `ON DELETE CASCADE` references from `templates`, `designs` and
   `assets` continue to resolve;
6. recreate `idx_projects_updated_at`.

Steps 3–5 in one migration file, in that order. A single `NOT NULL` never
touches a row that has not been backfilled.

### 3b. The account is derived, and the compiler enforces the scope

The phase says not to duplicate `account_id` onto assets, designs, templates or
Brand Kits. **Correct, and it should stay that way**: every one of those already
carries `project_id ... REFERENCES projects (id) ON DELETE CASCADE`
(`migrations/sqlite/0003_templates.sql:7` is the pattern), so ownership is one
hop away and there is exactly one place it is recorded. A second copy is a second
thing that can disagree, and disagreement about who owns a row is the failure
this whole phase exists to prevent.

The cost is a JOIN on the scoped reads. That is the right price.

**What the phase leaves implicit is how 36 methods across 7 traits get scoped
without one being missed.** Auditing them is not a mechanism; auditing them is
hoping. Make it unavoidable:

> Every repository method that reads or writes project-owned state takes an
> `AccountId` as a parameter — not an `Option<AccountId>`, not a field on a
> context struct that a caller may or may not populate.

A method that forgets the scope does not compile. This is the same move as
*repositories stamp their own `created_at`*: the type system carries the
invariant so no call site can be careless. It also satisfies §11.5.4's real
requirement — the scope lives below the HTTP handler — because there is no
unscoped method left to call.

Counted from the tree: `design_repository` 8, `project_repository` 7,
`asset_repository` 5, `asset_request_repository` 5, `template_repository` 4,
`export_repository` 3, `object_store` 4. The object store is the one exception —
it addresses bytes by hash and knows nothing about projects; it is reached only
through an asset row that is already scoped.

---

## 4. "Where practical" is not a security rule

**The gap.** §11.5.4: cross-account access "returns the same not-found result as
an unknown object **where practical**". An isolation rule with an escape clause
is not an isolation rule; it is a preference, and the exceptions are wherever
the implementer was in a hurry.

**Resolution: unconditional, and here is why it is affordable.**

Every scoped read filters by `account_id` and a miss returns `DESIGN_NOT_FOUND`
/ `PROJECT_NOT_FOUND` — the identical code, message and status as an id that was
never issued. There is no branch that can distinguish them, because after 3b
there is no unscoped read to compare against.

`403` appears exactly once, for `ACCOUNT_REQUIRED`: a valid identity with no
membership at all. That leaks nothing about any object, only about the caller,
who already knows.

Never `403` for an object in another account. A `403` is an admission that the
id exists, and ids are guessable in aggregate even when a UUID is not — the
phase says as much in its own test list: *"do not rely only on random UUID
secrecy."*

---

## 5. The Claude live smoke cannot be automated, and the plan already says so

**The gap.** §11.5.23 lists a sixteen-step OAuth flow as an automated test whose
step 8 is "user authenticates" — a magic link, which means reading an inbox —
and a Claude connector smoke driven from "a normal Claude custom connector UI".

**Resolution: split it in three, by what is actually ours to drive.**

**Automated, protocol-level (CI, against the real HTTPS deployment).** Steps
1–7 and 11–16: the unauthenticated `401`, the `WWW-Authenticate` challenge,
Protected Resource Metadata, authorization-server discovery, registration, PKCE,
the code exchange, `tools/list`, a scoped `tools/call`, refresh, and a revoked
grant failing to refresh. The magic link is avoided with a Supabase test user
holding a password (`signInWithPassword`), which is a fixture, not a weakening of
production auth.

**Automated, browser (Playwright).** Steps 8–10 are our own `/oauth/consent`
page: authorization details rendered, approve and deny, redirect to the exact
`redirect_url`. This is our app, which is what `e2e/` is for.

**Manual, documented as a checklist.** The Claude connector run. This is not
pragmatism — **plan §67 forbids it**: Playwright is for testing our app, and the
plan explicitly rules out driving external AI services with it. The phase's
requirement that no manual bearer be configured is still checked; a person
checks it, against a written procedure, and records the result in
`docs/deploy/status.md` under *verified here* rather than *reported*.

The same applies to the OpenAI run, with the additional finding in §8 below.

---

## 6. `GET /skill` should be compiled in

**The gap.** The phase asks for a public route returning the canonical skill. It
does not say how the process gets the file, and reading it from disk at runtime
makes it a deploy artifact that can go missing, go stale against the binary, or
`404` after a partial deploy.

**Resolution: `include_str!`, exactly as the repo already does.**

`apps/mcp/tests/skill_matches_the_code.rs:15` reads the skill with
`include_str!("../../../skills/forma/SKILL.md")`. The route uses the same
constant path, serves `text/markdown; charset=utf-8`, and is public. It cannot
drift from the binary that serves it, cannot be forgotten by `install.sh`, and
needs no file on the machine.

Served by `forma-server`, which already owns the web origin, so Caddy needs no
new route. The existing skill test gains one assertion: the bytes the route
serves are the bytes the test checks.

The repository symlinks (`.claude/skills/`, `.agents/skills/`) are unaffected —
they serve local discovery, this serves remote.

---

## 7. The CSP is nearly free, and there is exactly one injection point

**The gap.** §11.5.6 is right that browser bearer tokens make XSS credential
theft, and asks for a strict CSP. It cannot know what the editor loads.

**Resolution: it loads nothing external, so the policy can be maximal.**

Checked: the editor's fonts are self-hosted woff2
(`apps/web/src/fonts/Geist-Variable-latin.woff2`), there is no CDN, no Google
Fonts link in `apps/web/index.html`, and no `fetch` to any absolute URL anywhere
in `apps/web/src`. Supabase adds exactly one origin, to `connect-src` only:

```
default-src 'self';
script-src  'self';
style-src   'self';
style-src-attr 'unsafe-inline';
img-src     'self' data: blob:;
font-src    'self';
connect-src 'self' https://<project-ref>.supabase.co;
frame-ancestors 'none';
base-uri    'none';
object-src  'none';
```

`style-src-attr 'unsafe-inline'` is required and is not a loophole: React writes
inline `style` attributes (`apps/web/src/editor/Canvas.tsx` sets transforms that
way), and attribute styles cannot execute script. `img-src data:` is required
because the renderer embeds asset bytes as data URIs.

**Set it in `forma-server`, not in Caddy.** A header only present in the hosted
deployment is a header no test ever sees; one served by the binary is exercised
by the Playwright walkthrough on every run, and a violation shows up locally
where it is cheap.

**The one injection point, named.** `apps/web/src/editor/Canvas.tsx:122` writes
the renderer's SVG with `dangerouslySetInnerHTML`. Its input is generated by our
own renderer from the document, and uploaded SVG assets are sanitized by
re-serializing through usvg (`crates/assets/src/svg.rs`) — a whitelist by
construction, which drops scripts and handlers because usvg never understood
them rather than because a filter matched them. The existing test asserts a
`<script>` does not survive.

So the CSP here is defence in depth over a boundary that is already sound, not a
patch over a known hole. It stays required: it is the difference between one
sound boundary and two, and after this phase what sits behind it is a credential.

---

## 8. What the sources say that the phase document does not

Verified against the primary references on 2026-08-30, the date the phase gives
for its own research.

**Supabase's OAuth 2.1 server is in beta**, and "free to use during the beta
period" across all plans. The phase records the capability and not the maturity.
It does not change the design — the resource-server boundary is standard OAuth
and survives a provider change — but twenty-four acceptance criteria resting on
a beta feature with undefined future pricing belongs in the constraints section,
not discovered later.

**ChatGPT accepts OAuth or nothing.** Its documented MCP authentication options
are OAuth or none; there is no manual bearer header. This *strengthens* the
phase: §11.5.12 presents the PAT as the compatibility path for hosts where OAuth
is inconvenient, but for ChatGPT the PAT is not a fallback at all. OAuth is not
the preferred route there, it is the only one. The PAT's real constituency is
CLI hosts — Codex CLI, Claude Code, Gemini CLI — which is where the phase
already points it.

**Prefer Supabase's per-client audience.** The phase proposes a single flat
audience, `https://forma.hintly.online/mcp`. Supabase's own MCP example sets
`https://mcp.myapp.com/{client_id}` and gates the hook on an approved-agents
table. The per-client form gives finer revocation and carries the DCR
approval gate that §1 above otherwise has to invent. Validate the prefix,
accept the client suffix.

**Asymmetric signing is required, not recommended**, whenever the `openid` scope
is requested — which the consent flow does. The phase asks for RS256/ES256 "so
OIDC can work correctly", which reaches the right configuration by a softer
route than the documentation takes.

**Revocation is immediate.** The phase says the provider "eventually receives an
auth failure". Supabase marks the consent revoked, deletes active sessions for
that client and invalidates its refresh tokens at once. §11.5.16 can promise
more than it does.

**DCR and the transport are both dated.** MCP `2026-07-28` deprecates DCR in
favour of CIMD ("will be removed in a future version"), and its stateless core
removes the `initialize`/`initialized` handshake and `Mcp-Session-Id` — which is
precisely what `apps/mcp/src/http.rs` uses today through `LocalSessionManager`.
The phase is right that the transport is a separate decision. Worth writing down
that the two age together, so whoever migrates the transport already knows CIMD
is waiting on the same trip.

---

## What this does not change

The architecture. Supabase for identity and not for application data; SQLite and
the local filesystem still authoritative; the account boundary enforced below the
HTTP handlers; `FORMA_AUTH_MODE=local` keeping local Forma free of any of it;
rmcp's Host allowlist untouched; Phase 12 still a separate, optional phase.

And the phase's own framing of acceptance, which is the best sentence in it: not
"OAuth works", but a person connecting a mainstream agent through that provider's
normal flow, the agent working only inside that person's account, and the
connection revocable without affecting anyone else.

## One structural recommendation, restated

Sections 3, 4 and the tenant model they serve **require no Supabase at all**.
Accounts, the scoped repositories, the legacy migration and the cross-account
tests can be built and shipped under `FORMA_AUTH_MODE=local` against the
singleton account, with no provider, no JWT and no OAuth.

That is also where all the security risk lives: 36 methods, 8 use-case modules,
and the question of whether one person's work can reach another's. Everything
verified in §8 is in beta, dated, or both.

Build the boundary first, with its tests. Plug identity into a boundary that is
already proven.

---

# Part two: the seven sections that assert something about this code

The phase's other sections are design over an external provider and were checked
against the primary sources instead. These seven make claims about the tree, so
they were checked against the tree. Findings are ordered by what they cost to
discover late.

---

## §11.5.21 — the guard that would stop meaning anything

**The finding, and the largest one in this audit.**

`Config::binding_is_safe` (`apps/server/src/config.rs:53`) refuses to start when
the bind address is not loopback and no `FORMA_REMOTE_TOKEN` is set. It is the
one configuration the server will not accept, and it rests on a premise:
*loopback means only this machine can reach it.*

The deployment binds `127.0.0.1:4765` (`deploy/forma.env.example:14`) with Caddy
in front. **That premise has been false since Caddy went in front of it** — the
whole internet reaches loopback, through the proxy. It is masked today, and only
because `FORMA_REMOTE_TOKEN` happens to be set, so the guard returns early on
its first line and its reasoning is never reached.

§11.5.21 retires that token once the legacy account is claimed. At that moment:

- `binding_is_safe` sees a loopback bind and returns `Ok`;
- `require_bearer` (`apps/server/src/auth.rs`) treats *no token configured* as
  its **pass-through** case — which is correct today, because no token means
  local;
- so a hosted deployment with `FORMA_AUTH_MODE` unset, or a missing
  `FORMA_SUPABASE_JWKS_URL`, starts cleanly and serves the entire API
  unauthenticated to the internet, with nothing in any log saying so.

Both halves have to change together:

1. `binding_is_safe` becomes a function of the auth mode, not of the bind
   address alone. In `supabase` mode it requires the Supabase issuer and JWKS
   configuration and refuses without them. The current check stays for `local`.
2. `require_bearer`'s open case is gated on `FORMA_AUTH_MODE=local`, not on the
   absence of a token. "No credential configured" must stop being a synonym for
   "no credential required".

This is the same class of bug as the one the guard was written for, arriving by
the opposite route: not a public bind with no token, but a loopback bind that
the world can reach.

---

## §11.5.8 and §11.5.20 — the consent page 404s, measured

**The finding.** The phase requires three new SPA routes: `/login`,
`/oauth/consent`, `/agents`. Supabase redirects the user to
`https://forma.hintly.online/oauth/consent?authorization_id=<id>`.

`App.tsx:6` says it plainly — *"Two views, no router"* — and the server serves
the bundle with `actix_files::Files::new("/").index_file("index.html")`
(`apps/server/src/main.rs:122`) and **no `default_handler`**. `index_file` only
answers a directory request; an unknown path is a 404.

Run against the built app rather than reasoned about:

```
GET /                -> 200
GET /oauth/consent   -> 404
GET /login           -> 404
GET /agents          -> 404
```

So the OAuth flow does not fail at the token exchange or at a signature check.
It fails when Supabase hands the person back to us and we answer 404 — the step
that only appears when a real provider drives a real browser, which is the most
expensive place to find it.

Two changes, both small, neither optional:

- a `default_handler` on the static service that returns `index.html` for any
  non-`/api/` path that is not a file, so client-side routes resolve;
- a router in the SPA, which V1 deliberately did without and which this phase
  is the first real reason to add. `App.tsx`'s own comment anticipated it: *"it
  becomes worth adding when a URL has to be shareable, which is a Phase 11
  concern."*

**And Caddy needs a third block.** `handle /mcp*` strips the prefix
(`deploy/caddy/Caddyfile`), and `/.well-known/oauth-protected-resource/mcp` does
not match `/mcp*` — it falls through to `handle { reverse_proxy 127.0.0.1:4765 }`,
reaches `forma-server`, and 404s exactly as above. Protected Resource Metadata
needs its own `handle` to `127.0.0.1:4766` **without** `strip_prefix`, or
`forma-mcp` will receive a path it does not expect.

---

## §11.5.11 and §11.5.18 — `forma-mcp` holds one credential for its lifetime

**The finding.** `ServerClient::new(base_url).with_token(token)` is built once,
from the process environment (`apps/mcp/src/main.rs:70` for stdio,
`apps/mcp/src/http.rs` for the HTTP transport, where the session factory closure
clones the *same* constant token per session).

That is exactly right for what exists: one deployment, one credential, and the
MCP process is an internal client of the REST API presenting the deployment's
own token.

After this phase every inbound MCP request carries a **different person's**
identity — an OAuth access token or a PAT — and `forma-mcp` has to reach
`forma-server` as that person. A process-wide credential cannot express that.

The phase never says this, because from the outside `forma-mcp` looks like a
resource server and internally it is also a client. Both halves change:

- inbound: validate the OAuth JWT or PAT and resolve an `AuthContext`
  (§11.5.9, §11.5.10 — the phase covers this);
- outbound: forward that identity per request rather than per process, so
  `forma-server` scopes on the caller and not on the proxy.

The cheapest form that preserves the existing invariant — *`forma-mcp` must
never open SQLite; a single mutation path keeps versioning and validation in one
authority* — is for `forma-mcp` to pass the caller's credential straight
through, and for `forma-server` to do the validation once, for both entry
points. That keeps one verifier rather than two that can disagree about who
someone is.

---

## §11.5.17 — the actor is caller-controlled today

**The finding.** `actor_from` (`apps/server/src/routes/helpers.rs:50`) reads the
`x-forma-actor` header and trusts it completely: absent means `Human`,
`agent:claude` means an agent with that id. Any caller can write anything.

`forma-mcp` sets it from the host's `clientInfo.name` (`apps/mcp/src/server.rs:180`),
which is itself supplied by the host.

Today that is not a vulnerability: one credential, one person, and §61's history
is a convenience for that person. With accounts it becomes a forgeable audit
trail — and the phase is right that the actor must come from authenticated
context.

The change is smaller than it sounds because the helper is already the single
choke point: no handler reads the header itself. `actor_from` takes the
`AuthContext` and derives the actor from it, per §11.5.17's mapping. The header
may survive as *descriptive* client metadata alongside the derived actor, never
as its source.

---

## §11.5.22 — the envelope has no 403 at all

**The finding.** `ApiError` offers `bad_request`, `unauthorized`,
`payload_too_large` and `internal` (`apps/server/src/error.rs:50-88`), and
`status_for` maps codes to `CONFLICT`, `NOT_FOUND`, `BAD_REQUEST`,
`UNPROCESSABLE_ENTITY`, `PAYLOAD_TOO_LARGE` and `INTERNAL_SERVER_ERROR`.

**`FORBIDDEN` appears nowhere.** §11.5.22 requires `403 ACCOUNT_REQUIRED` —
a valid identity with no Forma account — which is the one status this phase
genuinely needs and the codebase has never had.

Trivial to add, and worth naming because the alternative is that an implementer
reaches for `unauthorized` and a caller cannot tell "your credential is bad"
from "your credential is fine and you have no workspace". Those need different
actions from the person reading them.

The remaining new codes (`INVALID_TOKEN`, `LEGACY_ACCOUNT_ALREADY_CLAIMED`,
`INVALID_BOOTSTRAP_TOKEN`, `API_KEY_REVOKED`) slot into `status_for` with no
structural change. `AUTH_REQUIRED` is a synonym for the existing `UNAUTHORIZED`
and should not be added twice — the code is part of the contract agents recover
against, and two codes for one condition is how that contract rots.

---

## §11.5.23 — the protocol test has no home in CI

**The finding.** The phase lists a sixteen-step OAuth flow "against the real
HTTPS deployment" among its tests. Every test in this repository today runs
against a temporary SQLite and a locally built binary; CI has no deployment,
no domain and no certificate.

There is already a precedent for exactly this shape, and it is not the test
suite: `scripts/smoke-remote.py` runs from off the machine against the real
domain, reports pass/fail counts, and cleans up after itself. It is run by a
person after a deploy, and its results are recorded in `docs/deploy/status.md`
under *verified here*.

The OAuth protocol flow belongs there — extending `smoke-remote.py`, or beside
it — not in `cargo test`. Calling it a CI test would produce a test that is
skipped on every run, which is worse than a script someone actually executes.

The consent page's own behaviour (authorization details rendered, approve,
deny, redirect) is ours and belongs in `e2e/` with Playwright, where it runs on
every push.

---

## §11.5.19 — local mode is compatible, with one caveat

**Checked and sound.** `forma-mcp` on stdio takes no credential of its own
(`apps/mcp/src/main.rs:69`); it forwards `FORMA_REMOTE_TOKEN` to the REST API
only when one is set, and locally none is. The security boundary is the process
that launched it, exactly as §11.5.19 states. A singleton account bootstrapped
at startup preserves this without a Supabase account existing anywhere.

**The caveat is the one already stated under §11.5.21**: local mode's safety
currently rests on `require_bearer` treating "no token" as open. Once that is
gated on `FORMA_AUTH_MODE=local`, local mode keeps exactly the behaviour it has
now — and the hosted deployment loses the failure mode where forgetting to
configure identity is indistinguishable from choosing not to require it.

---

## §11.5.18 — two things I went looking for and did not find

Reported because a clean result is worth as much as a finding, and because the
next reader should not repeat the search.

**The export download is not a path-traversal hole.** `/exports/{id}/download`
takes a caller-supplied `?file=`, which looked like the most likely place for
one. `application::exports::download` already rejects any name containing `/`,
`\` or `..` before joining it to the storage key
(`crates/application/src/exports.rs:208`). It needs account scoping like every
other read, and nothing more.

**Cached renders need no separate scoping.** Previews are keyed by design id,
version and brand version, and are reached only through a design lookup. Scoping
that lookup covers them; there is no route that takes a cache path directly.

---

## What the audit changes about the order

Nothing about the architecture, and one thing about sequencing.

Three of these findings — the guard in §11.5.21, the single credential in
§11.5.18, and the missing 403 — are prerequisites that sit *below* identity, in
the same layer as the account boundary. They can and should land in the first
slice, with the tenant model, under `FORMA_AUTH_MODE=local`, before any provider
exists.

The routing findings (§11.5.8, §11.5.20) belong to the identity slice, because
there is nothing to route to until there is a consent page. But they are worth
writing into the plan now, because a 404 discovered by a real provider driving a
real browser is the most expensive failure in the whole phase, and it is
currently invisible in a document that says three routes are "required".
