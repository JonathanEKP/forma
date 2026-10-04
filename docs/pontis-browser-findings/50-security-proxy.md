# 50 — Security, isolation and the proxy surface

Tester T5, instance **6** (editor `127.0.0.1:4862`, sidecar `4861`, forma-server `4860`), two people paired
(`--devices a,b`, FakeProvider unless a case says Codex). Format: [README](README.md). Probes were run
with `curl` and small Node scripts against raw sockets (so `..`, `//`, `%2f`, absolute-form request
targets and odd `Host` values reach the server unmodified) and from the Orca browser tab. Hostile
tokens were minted with the public fixture key (and a throwaway key, `alg=none`, HS256 …) into a
scratch directory outside the repository; no token, header or credential appears below.

"Editor origin" = through Vite (`:4862`), the way a browser reaches the sidecar. "Sidecar direct" = `:4861`.
A = `person-a-6`, B = `person-b-6`, each in their own account.

## Summary

See the end of the file ("Result") for counts. The one defect worth a decision is
[F-50-1](#f-50-1--the-sidecars-service-credential-is-accepted-on-the-whole-rest-api-and-can-mint-a-pat):
the Boundary F service credential is not confined to `/api/v1/pontis/*`.

## Cases run

### Proxy surface (through the Vite origin)

Expected shape: only `/pons/v1/pairing/codes`, `/pons/v1/devices[/*]`, `/pons/v1/events`, `/pons/v1/jobs[/*]`,
`/pons/v1/ui/*`, `/agent/intent`, `/agent/context` reach the sidecar; the Runtime's routes
(`/pons/v1/pairing/claim`, `/pons/v1/device/*`) never do. "Vite HTML" below means the SPA fallback answered,
i.e. the request did **not** reach the sidecar.

| id | what | result |
|---|---|---|
| P-01 | `GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS` on `/pons/v1/pairing/claim`, `/pons/v1/device`, `/pons/v1/device/socket`, `/pons/v1/device/capabilities` via the editor | pass — GET/HEAD get the SPA shell, every other method a bare Vite `404`; none reaches the sidecar (the sidecar answers these paths with `400 unsupported_protocol_version` / JSON `404` when hit directly) |
| P-02 | `POST` with a valid `{"protocol":"pons/1.0"}` body to `/pons/v1/pairing/claim`, `/pons/v1/device/{challenge,auth,delegation}` via the editor | pass — Vite `404`, no sidecar JSON (direct, the same bodies reach the Runtime handlers and answer `400 bad_request`) |
| P-03 | `//pons/v1/devices`, `/pons//v1/devices` | pass — SPA shell, not proxied |
| P-04 | `/pons/v1/DEVICES`, `/PONS/V1/devices`, `/agent/Intent` (case) | pass — SPA shell |
| P-05 | trailing dot / slash / `;x` / `%00`: `/pons/v1/devices.`, `/pons/v1/devices;x`, `/pons/v1/devices%00`, `/pons/v1/events/`, `/agent/intent/`, `/agent/intent.` | pass — SPA shell (`/pons/v1/devices/` is forwarded by the `devices(/.*)?` pattern and answered by the sidecar's JSON `404`) |
| P-06 | near misses `/pons/v1/eventsx`, `/pons/v1/ui` (no slash), `/pons/v1/device%2Fx`, `/pons/v1/devices%2f..%2fdevice`, backslashes `/pons/v1/devices\..\device\x` | pass — SPA shell |
| P-07 | smuggling a Runtime route through an allowed prefix: `/pons/v1/devices/../device/challenge`, `/pons/v1/devices/%2e%2e/device/challenge`, `/pons/v1/devices/..%2fdevice%2fchallenge`, `/pons/v1/ui/../device/challenge`, `/pons/v1/jobs/../pairing/claim` (POST, valid body) | pass, **with a note** — the pattern matches and Vite forwards the path **unnormalised**, so the sidecar sees `..`; it does not normalise, so every one is its JSON `404 not_found` and nothing runs. Safe only because the sidecar compares paths literally; see F-50-6 |
| P-08 | `/pons/v1/pairing/codes/../claim` (GET and POST) | pass — not matched (the key has no `/.*` after `codes`), Vite `404` / shell |
| P-09 | query smuggling: `/pons/v1/pairing/codes?/../claim`, `/pons/v1/events?x=/pons/v1/device/challenge` | pass — the sidecar sees path `…/codes` and `…/events` respectively (`403` / `405` for the method) |
| P-10 | methods on every **allowed** path (7 methods × 11 paths) | pass — only the methods each route implements get past `405`; `OPTIONS` is answered `204` by Vite itself on all of them (no CORS headers, see P-17) |
| P-11 | WebSocket upgrade (`Connection: Upgrade`) on `/pons/v1/device/socket`, `/pons/v1/devices/x`, `/pons/v1/ui/stream` via the editor | pass — never reaches the sidecar (Vite does not proxy upgrades here: no `ws: true`). The request is left **unanswered until the client gives up** (Vite's HMR upgrade handler), which is an oddity, not a hole. Direct to the sidecar the socket demands a Device proof (`400 protocol_version_incompatible` without one) |
| P-12 | absolute-form request targets `GET http://evil.example/pons/v1/device/challenge`, `GET http://127.0.0.1:4862/pons/v1/devices` | pass — SPA shell |
| P-13 | Capability Gateway paths `/pons/v1/capability/invoke`, `/pons/v1/capabilities` via the editor | pass — not in the proxy list; SPA shell / `404` |
| P-14 | Vite dev-server file routes: `/@fs/etc/passwd`, `/@fs/home/…/auth.json`, `/@fs//etc/passwd`, `/../../../../etc/passwd`, `/%2e%2e/…`, `/.env`, `/apps/web/.env` | pass — `403 Restricted` / SPA shell; no file content. (`/src/*`, `/package.json`, `/node_modules/.vite/…` are served by design in dev) |
| P-15 | `GET /__open-in-editor?file=/etc/passwd` (Vite's launch-editor middleware) | oddity (info, F-50-7) — answered `200`; this is a dev-server feature that asks the machine to open a file |
| P-16 | `Host: evil.example` / `LOCALHOST:4862` against the Vite origin | pass — Vite's own `403 Blocked request. This host … is not allowed` |
| P-17 | Cross-origin reads through the Vite origin: `OPTIONS` and `GET` with `Origin: http://evil.example`, `null`, `http://localhost.evil.example`, then `http://localhost:9999` and `http://127.0.0.1:9999` | oddity (low, F-50-5) — foreign and `null` origins get **no** `access-control-allow-origin`; but **any `localhost`/`127.0.0.1` origin on any port** is echoed back as allowed, on the Pontis routes and on the source files. Vite 6.4.3's default `server.cors`; Caddy (`deploy/caddy/Caddyfile`) sets no CORS headers, so production is not affected |

### Host / Origin on the UI routes (`/pons/v1/ui/{stream,ack,actions}`), valid bearer A

| id | what | result |
|---|---|---|
| H-01 | `Host` = `evil.example`, `evil.example:4862`, `127.0.0.1` (no port), `127.0.0.1:4861`, `127.0.0.1:4862.evil.example`, `[::1]:4862`, `0.0.0.0:4862`, `LOCALHOST:4862`, `127.0.0.1:4862, evil.example` | pass — `403 host_not_allowed` before anything is read (direct and through Vite; Vite itself blocks the first and the upper-case one) |
| H-02 | no `Host` header at all (HTTP/1.1) | pass — `400` from Node, empty body |
| H-03 | `Origin` = `null`, `http://evil.example`, `http://127.0.0.1:4861`, `https://127.0.0.1:4862`, `http://127.0.0.1:4862/` (trailing slash), `HTTP://127.0.0.1:4862`, empty | pass — `403 origin_not_allowed`. Exact string match: a legitimate origin written differently is refused too |
| H-04 | **no `Origin` at all** on stream/ack/actions with a valid bearer and an allowed `Host` | by design, noted — accepted (`200 text/event-stream`, `400 bad_request` for the empty ack body). A same-origin `fetch` GET carries no `Origin`, so refusing would break the editor; the bearer is what authenticates |
| H-05 | `Host: localhost:4862` and `http://localhost:4862` | pass — accepted (the documented second name for B) |
| H-06 | `POST /pons/v1/ui/ack` with `Host: evil.example`, then `Origin: null` | pass — `403` both |

### Authentication (every sidecar route, through the editor origin)

Routes: `GET /pons/v1/devices`, `POST /pons/v1/pairing/codes`, `GET /pons/v1/events` (A's context, and principal scope),
`POST /pons/v1/devices/{id}/revoke`, `GET /agent/context`, `POST /agent/intent`, `GET /pons/v1/ui/stream`,
`POST /pons/v1/ui/ack`, `POST /pons/v1/ui/actions`.

| id | credential presented | result |
|---|---|---|
| A-01 | none; empty `Bearer `; `Bearer` alone; `Basic …`; `Bearer garbage` | pass — `403` on all ten routes (the sidecar's own shape: `origin_not_allowed` / `not_authenticated`, no detail) |
| A-02 | expired JWT (exp −120 s) | pass — `403` everywhere |
| A-03 | JWT signed by another P-256 key, same `kid` | pass — `403` everywhere |
| A-04 | `alg: none` (empty signature) | pass — `403` everywhere |
| A-05 | `alg: HS256` signed with an arbitrary secret | pass — `403` everywhere |
| A-06 | tampered payload (A's token with `sub` swapped to B, original signature) | pass — `403` |
| A-07 | valid signature, subject that has **no account** | pass — `403` everywhere (`ACCOUNT_REQUIRED` from the resolve call) |
| A-08 | wrong `iss`, wrong `aud` (`service_role`), no `kid`, unknown `kid`, `sub` not a uuid, `nbf` in the future, empty signature | pass — `403` everywhere |
| A-09 | lower-case scheme `bearer <valid>` | accepted (RFC 7235: scheme is case-insensitive) — not a finding |
| A-10 | valid token, B, against A's context (`GET /pons/v1/events?scope=application_context&applicationContextId=<A>`) | pass — `403 origin_not_allowed`; B's own context `200` |

### Cross-account (B's valid token against A's resources, FakeProvider)

| id | what | result |
|---|---|---|
| X-01 | B `GET /pons/v1/devices` | pass — only B's device; A's device id never appears |
| X-02 | B `POST /pons/v1/devices/<A's device>/revoke` | pass — `403 device_not_found` ("no such Device is paired with this account", same as a non-existent id); A's device still `online: true`, `revokedAt: null` afterwards |
| X-03 | B events with A's context id; with `after=-1`; with the id upper-cased; with `sessionId=<A's session>`; without a context but `sessionId=<A's>` | pass — `403 origin_not_allowed` each time |
| X-04 | B events with two `applicationContextId` params (own first, A's second) | pass — answers B's own context only (first wins); nothing of A's |
| X-05 | B `principal` scope, with `sessionId=<A's>` | pass — B's own (empty) principal stream |
| X-06 | B `session.input` / `session.cancel` into A's session id | pass — `403 not_authorized` |
| X-07 | A triggers an approval (`Remove the headline.`); B `approval.resolve` with A's `approvalId`, `approveOnce` and `deny` | pass — `403 not_authorized` both; design untouched |
| X-08 | B `session.start` carrying a forged `appContext.applicationContextId` = A, a forged `sessionId`, and A's `deviceId` (in the payload and at the top level) | pass — `200`, but the session is created in **B's** context on **B's** device (`session.created` shows B's account id and B's device); A's event stream is unchanged (9 events before and after) |
| X-09 | B `GET /agent/context` | pass — B's own account id only |
| X-10 | B `GET /api/v1/{designs,projects}/<A's id>` and `/brand` through Forma REST | pass — `404` (not `403`: existence is not confirmed) |
| X-11 | UI channel: B opens `/pons/v1/ui/stream`, then `POST /pons/v1/ui/ack` and `/actions` with **A's `clientId`** | _(see UI section below)_ |

### Boundary F (forma-server, direct, service credential)

| id | what | result |
|---|---|---|
| F-01 | represented user A, context A, design A (control) | `200` |
| F-02 | represented user A, context A, **B's design** | pass — `404 DESIGN_NOT_FOUND` |
| F-03 | A represented in **B's** account, and B in **A's** | pass — `403 CONTEXT_FORBIDDEN` (membership checked on every request) |
| F-04 | missing context header, missing represented-user header, represented user not a uuid | pass — `401 UNAUTHORIZED` |
| F-05 | represented user that has no account | pass — `403 CONTEXT_FORBIDDEN` |
| F-06 | B's session acting on A's project/design (`brand`, `asset-requests`, `validate`, `operations`) | pass — `404` for resources of the other account |
| F-07 | a **user JWT** on `/api/v1/pontis/*` (with and without the represented-user headers) | pass — `403 SERVICE_CREDENTIAL_REQUIRED` |
| F-08 | no credential on `/api/v1/pontis/projects` | pass — `401` |
| F-09 | resolve with the service credential and: expired / other-key / `alg=none` / HS256 / wrong issuer / tampered / missing subject token | pass — `401 SUBJECT_TOKEN_INVALID` each; subject without an account → `403 ACCOUNT_REQUIRED` |
| F-10 | `/api/v1/service/resolve` with a user JWT | pass — `401` ("this endpoint takes a service credential") |
| F-11 | the service credential on the **ordinary REST API** (`/api/v1/projects`, `/api/v1/designs/…`, `/api/v1/api-keys`) | **FAIL** — F-50-1 |

### Data leakage

| id | what | result |
|---|---|---|
| L-01 | every minted token (full, header, payload and signature parts) and the service credential (full and body) searched, as strings and without printing them, in `forma-server.log`, `sidecar.log`, `vite.log`, `identity.log`, `driver-{a,b}.log` after all the probes above (about 220 Boundary F calls, hundreds of rejected requests) | pass — **no hit in any log**. Words `authorization`, `bearer `, `forma_svc_`, `x-forma-subject-token`, `eyJ` do not occur in any log either |
| L-02 | the instance directory outside `logs/` and the database (`state.json`, seed scripts) | pass — `state.json` holds no credential; the only file with JWTs is `signin-{a,b}.js` (mode `0600`, mints the session by design) |
| L-03 | sidecar log vocabulary | pass — only `boundary_f_request`, `boundary_f` (operation, status, ms), `intent` (type, delivered, authorized), `sidecar_started`. No payload, no prompt text, no ids of other accounts |
| L-04 | error bodies of every refusal above | pass — no stack, no token echo, no other account's data; refusals on the sidecar's own routes are deliberately uniform (`origin_not_allowed`) |

## Findings

## F-50-1 — The sidecar's service credential is accepted on the whole REST API, and can mint a PAT

- **Severity:** high
- **Status:** open
- **Instance / provider:** instance 6; no model involved (forma-server alone)

**Steps to reproduce**
1. Start instance 6. Take the sidecar's `FORMA_SERVICE_CREDENTIAL` (it is in the sidecar's environment; the
   harness reads it from `/proc/<sidecar pid>/environ`) and A's subject and account id from `state.json`.
2. `POST http://127.0.0.1:4860/api/v1/projects` with `Authorization: Bearer <service credential>`,
   `x-forma-represented-user: <A's subject>`, `x-forma-application-context: <A's account>`, body
   `{"name":"throwaway"}` → `201`. `DELETE /api/v1/projects/<id>` → `204`.
3. Same headers, `POST /api/v1/api-keys {"name":"probe"}` → `201` with a `token`. `GET /api/v1/projects` with that
   token alone (no service credential, no headers) → `200` with A's projects.
4. Revoke both keys (`DELETE /api/v1/api-keys/<id>` → `204`; done in the run).

**Expected** — Boundary F is "the fixed operations the Pontis sidecar calls" with "no generic proxy" (plan §4,
`routes/pontis.rs` header; README of the sidecar: "reaches Forma only through Boundary F"). A service credential
should open `/api/v1/pontis/*` and `/api/v1/service/resolve` and nothing else, so that a leaked credential is worth
exactly what the sidecar's capability catalog is worth (read, write through `applyOperations`, with Forma's own
`required_risk` as the second boundary).

**Actual** — the service arm of `require_bearer` (`apps/server/src/auth.rs`) authenticates the credential on **every**
path and yields an `AuthContext` of kind `Service` acting as the represented user. Through it I could, for any user I
could name (subject + account id), list/create/delete projects, read designs, list API keys and **mint a personal
access token** that outlives the service credential (revoking the credential does not revoke it). None of those are
Boundary F operations. A destructive `DELETE /projects/{id}` needs no approval at all on this path, although the same
effect through the agent would be `design.applyOperations` + a `RemoveLayer` approval.

**Why it matters** — the credential is the one secret whose compromise the plan sizes (D1: "its own AuthKind … the
credential id is the unit that gets revoked when one leaks"). Today revoking it is not enough after a leak, because it
can already have minted durable, user-attributed PATs. The `actor_from` mapping records these writes as
`service:<id>:<subject>` (good for the audit trail), but nothing stops them.

**Suggested direction (not applied; not a testing-branch change)** — have `require_bearer`'s service arm accept only
`/api/v1/pontis/*` and `RESOLVE_PATH`, answering `403 SERVICE_CREDENTIAL_SCOPE` elsewhere; add a test that every
non-Boundary-F route refuses a service credential.

**Evidence** — probe output (credential redacted): `svc POST /api/v1/projects → 201`, `svc DELETE project → 204`,
`svc POST /api/v1/api-keys → 201`, `minted PAT, no service credential, GET /projects → 200`. Not a browser case: the
credential never reaches a browser, so this was seen with `curl`-level requests against forma-server.

## F-50-2 — An open UI stream is never re-authenticated: it keeps delivering after its token has expired

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 6, FakeProvider (no model involved; sidecar routes only)

**Steps to reproduce**
1. Mint a session token for A that expires in 25 s (fixture key; `exp = now + 25`).
2. Open `GET /pons/v1/ui/stream` (Host `127.0.0.1:4862`) with it — `200 text/event-stream`, `ui.connected`.
3. Wait 35 s. The same token on any fresh request is now `403` (cases A-02).
4. With a fresh token for A, start a session (`POST /agent/intent`, `session.start`) so durable events are produced.

**Expected** — Pontis's `ui-transport.ts` documents "*Reauthenticate the principal for every live event. Context events
additionally require current membership; losing it suppresses only that context's events*". A session that has expired,
been signed out, or lost its membership should stop receiving the person's transcript.

**Actual** — the expired stream received both `pons.event` frames (`sequence` 13 and 14, the new session's events)
and kept receiving heartbeats. The stream stayed open for as long as I held it (50 s).

**Cause (read, not patched)** — `notify` re-runs `options.authenticatePrincipal(request)` with the **stream's own
request object**, and `personResolver` (`apps/pontis-sidecar/src/person.ts`) memoises the answer **per request object**
in a `WeakMap`. For a stream that request lives as long as the connection, so the "re-authentication" returns the answer
from connect time forever. The README's "Nothing is cached across requests. A person is resolved once per request" is true
and is exactly the problem: a stream is one request that never ends. The same applies to the membership check
(`resolveApplicationContext` goes through the same memo), so a person removed from an account would keep hearing it too
(not exercised: this instance has no way to remove a member).

**Impact** — bounded: the stream only carries the person's own account's events, and only until the connection drops
(the editor reconnects every time the page reloads). But it is the defence the transport depends on for sign-out and
revocation, and it silently does nothing. The editor's own refresh keeps tokens fresh, so a person never sees it.

**Suggested direction** — resolve afresh in `notify` (a new object per call, or bypass the memo for streams), or close
the stream when the token's `exp` passes.

**Evidence** — script output: `0s recv HTTP/1.1 200…`, `35s token now expired…`, `35s recv … event: pons.event …
"sequence":13`, `…"sequence":14`, `45s recv keep-alive`, `50s … stream was open: true`. Not seen in a browser.

## F-50-3 — Two windows of the same person take the UI channel from each other forever

- **Severity:** medium
- **Status:** open
- **Instance / provider:** instance 6, FakeProvider; reproduced with a script that runs the editor's own reconnect logic
  against the real routes (the two-tab case could not be held steady in the browser, see F-50-8)

**Steps to reproduce**
1. Sign in as A. Open the editor in two windows (or one window and anything else that opens `/pons/v1/ui/stream` with
   A's token — an `ui.*`-capable second device, a second browser profile).
2. Watch `/pons/v1/ui/stream`.

**Expected** — two windows both stay connected, or the second is told it is a duplicate; an agent's `ui.*` action goes to
a live window.

**Actual** — `UIClientRegistry.connect` replaces any existing client with the same `userId`, `applicationContextId` **and
`appSessionId`**. The sidecar defines `appSessionId` as `<userId>:<applicationContextId>` (`sidecar.ts`), which is the same
for every window of one person, so the newer stream always closes the older. `channel.ts` reconnects 2 s after its stream
ends (`RECONNECT_MS`), which evicts the other window, which reconnects 2 s later, and so on. Measured over 20 s with two
emulated windows against the real routes (via the editor origin):

| | stream opens | actions registrations | Boundary F `service.resolve` calls |
|---|---|---|---|
| 1 window | 1 | 1 | 2 |
| 2 windows | 6 and 5 | 6 and 5 | 22 |

So each window is connected roughly half the time, the chat's "Connected · live" indicator would flip to "offline" and back
every few seconds, an agent `ui.*` action arriving in a gap answers `ui_unavailable`/`ui_action_not_registered`, and the pair
costs ~11× the forma-server calls of a single window, indefinitely, with no backoff. The code's comment on this
(`useUiChannel.ts`: "the registry replaces a second connection from the same browser context rather than accumulating")
assumes a reload, not a second tab.

**Suggested direction** — make `appSessionId` per window (a random id kept in `sessionStorage`, sent with the stream), or
have the sidecar refuse the second stream (`409`) so the client can stop retrying; add a backoff.

**Evidence** — script output: `windows=1 … opens:1 … resolve calls: 2`; `windows=2 … opens:6 / 5 … resolve calls: 22`.
(`MAX_CLIENTS_PER_USER = 8` in Pontis is therefore unreachable: 12 streams from one person left one registered.)

## F-50-4 — The proxy forwards paths with `..` and `%2e%2e` to the sidecar unnormalised

- **Severity:** low
- **Status:** open (nothing exploitable found; a latent assumption)
- **Instance / provider:** instance 6

**Steps to reproduce** — via the editor origin, `POST /pons/v1/devices/../device/challenge` (and `/%2e%2e/`, `..%2f`,
`/pons/v1/ui/../device/…`, `/pons/v1/jobs/../pairing/claim`) with a valid `{"protocol":"pons/1.0"}` body (cases P-07).

**Expected** — the proxy allow-list is a list of paths; a request that *resolves* to a Runtime route is not on it.

**Actual** — the patterns `devices(/.*)?`, `jobs(/.*)?`, `ui/.*` match the raw string, so Vite proxies the request with
the `..` still in it. The sidecar compares paths literally (`path === …`, `startsWith('/pons/v1/')` then exact lookup), so
each of them is a JSON `404 not_found` and no Runtime handler runs. Safe today **only** because nothing downstream
normalises. Anything that did (a future router, a different front proxy, Caddy's own path cleaning in production —
not tested, production is out of scope) would turn the `devices/*`, `jobs/*` and `ui/*` wildcards into a way to the
unauthenticated Runtime routes (`/pons/v1/pairing/claim`, `/pons/v1/device/*`). `proxy.test.ts` covers query smuggling but
not dot segments.

**Suggested direction** — tighten the wildcards to what exists (`devices/[^/]+/revoke`, `jobs/[^/]+/cancel`,
`ui/(stream|ack|actions)`), or refuse any path containing `..`, `%2e` or a double slash at the edge; add the dot-segment
cases to `proxy.test.ts` and the Caddyfile test.

**Evidence** — probe output `ED POST /pons/v1/devices/../device/challenge → 404 {"error":"not_found"}` (the sidecar's own
JSON, so it was forwarded), against `ED POST /pons/v1/device/challenge → 404` (Vite's bare 404, not forwarded).

## F-50-5 — Vite's dev server grants CORS to every `localhost`/`127.0.0.1` origin, on the Pontis routes too

- **Severity:** low
- **Status:** open (dev server only; Caddy sets no CORS headers)
- **Instance / provider:** instance 6

**Steps to reproduce** — `GET /pons/v1/devices` (or `OPTIONS` with `Access-Control-Request-Headers: authorization`) via
the editor origin with `Origin: http://localhost:9999`.

**Expected** — the Pontis routes are same-origin by design (D9: "the sidecar refuses a cross-origin Origin"); no
`Access-Control-Allow-Origin` for anything else.

**Actual** — `Access-Control-Allow-Origin: http://localhost:9999` (and `http://127.0.0.1:9999`) on both, and on `/src/*`.
Foreign (`http://evil.example`), `null` and `http://localhost.evil.example` origins get none. This is Vite 6.4.3's default
`server.cors`. The UI routes themselves still refuse such an `Origin` (the sidecar checks it exactly), but `/pons/v1/devices`,
`/pons/v1/events`, `/agent/context` and `/agent/intent` have no `Origin` check of their own, only the bearer. A page on any
other local port that obtained a person's bearer (it cannot read the editor's `localStorage`) could therefore call them
cross-origin from a browser. Worth knowing because the proxy is documented as *the* thing that keeps the sidecar
same-origin.

**Suggested direction** — `server: { cors: false }` in `apps/web/vite.config.ts` (the app never needs CORS: everything is
same-origin through the proxy).

**Evidence** — `OPTIONS /pons/v1/devices … 204 Access-Control-Allow-Origin: http://localhost:9999`; same for `evil.example`:
no header.

## F-50-6 — Vite's `/__open-in-editor` runs on the owner's machine for any web page

- **Severity:** low (info for a dev-only surface)
- **Status:** open — wontfix candidate; recorded because it is reachable through the editor origin
- **Instance / provider:** instance 6

**Steps to reproduce** — `GET http://127.0.0.1:4862/__open-in-editor?file=/etc/passwd` (one request, run once).

**Actual** — `200` with an empty body, and `logs/vite.log` shows a shell error from the launcher: `mkdir: cannot create
directory '': No such file or directory`, i.e. Vite's launch-editor middleware executed the owner's editor command with an
attacker-chosen file. It needs no credential, and a plain `<img src="http://127.0.0.1:<port>/__open-in-editor?file=…">` on any
web page would send it (the `Host` is the allowed one). Dev-server only: the built app served by forma-server and Caddy has
no such route.

**Suggested direction** — nothing for a product release; the dev instructions could say not to browse untrusted pages while
`pnpm dev` runs, or `server.middlewareMode`/a proxy rule could drop `/__open-in-editor`.

**Evidence** — `vite.log` excerpt above.

## F-50-7 — Console error while using the editor: duplicate React key `#171717`

- **Severity:** low
- **Status:** open
- **Instance / provider:** instance 6, FakeProvider (not Pontis-specific; seen while opening a design and the Agent panel)

**Steps to reproduce** — signed in as A, open `ALPHA project` → `ALPHA Launch cover` → Agent, send a message.

**Actual** — the console reports twice: "Encountered two children with the same key, `#171717`. Keys should be unique so that
components maintain their identity across updates." Something renders a list keyed by a colour (the seeded brand/colour
swatches or a colour picker, `#171717` appears twice in the palette). No other console message appeared during normal use or
after the probes in this file; the network panel showed only `127.0.0.1:4862` and no request ≥ 400.

**Evidence** — `orca console` output (message text above).

## F-50-8 — Harness: every Orca browser tab reloaded itself about every 12 s while other testers were running

- **Severity:** info (harness / environment, not product)
- **Status:** open
- **Instance / provider:** instance 6; any page

**Actual** — from roughly 20:50 local time, a page in my tab (the editor, and also the fake identity provider's JWKS JSON)
lost its `window` state and DOM marker about every 12.4 s (`performance.now()` restarted; a `data-` attribute set on
`<html>` vanished; the same on a brand-new tab). Nothing in the app calls `location.reload`, Vite logged no HMR reload and
the Navigation API hook recorded nothing, so the reload came from outside the page (another tester's `orca reload` without
`--page` is my guess; I did not find the loop). Consequence for testing: stateful browser flows (two tabs held side by side,
a chat session across several turns) cannot be observed in the browser, so the multi-window case (F-50-3) was reproduced with
a script that runs the editor's own channel logic instead, and each browser flow here was written as one in-page script that
finishes inside one page lifetime (`clickthrough → Agent → Send → read transcript`). `orca snapshot` also sometimes showed a
different page state than `orca screenshot` for the same tab, so DOM reads through `orca eval` were used.
