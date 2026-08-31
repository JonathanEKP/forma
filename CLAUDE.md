# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Current state

[forma-plan.md](forma-plan.md) is a ~4,200-line execution plan and the spec of record.
Before implementing anything, read the relevant section (numbered headings —
`grep -n '^#\{1,3\} '` lists them).

[docs/plan-status.md](docs/plan-status.md) is the audit of what has actually been built
against it, what is still open, and the agreed order. **Read it before picking up work**,
and close a phase only when every task in its §77 list is built or deferred there with a
reason — Phase 6's acceptance passed with two of its panels missing.

**Phases 0 to 9 are complete.** The workspace, domain core and its operations, the full
render pipeline (text, shapes, images), the validator, SQLite persistence with optimistic
versioning and a snapshot per version, the `application` use cases of §8.2, the `assets`
ingest of §8.5, the whole REST surface of §31, the React editor, the AssetRequest flow, and
all the MCP tools, and the agent skill all exist and are tested. The blocking spike of §36 resolved to **Option A**: `<g id="layer-{uuid}">`
wrappers survive usvg normalization, and the editor addresses rendered geometry by layer
id directly.

**Phase 10 (hardening) is complete**, and with it every phase of the V1 plan: golden
renders (§63), structured logging (§60), render-cache cleanup, graceful shutdown and
backups. Upload security and SVG sanitizing landed back in Phase 5, and the error UX in
the §32 envelope. The brand panel, the editor's export control and the §67 Playwright
walkthrough closed afterwards, so the V1 plan has nothing open.

**Phase 11 is deployed and its acceptance is met**: an authorized remote MCP host
creates and renders a piece over `https://forma.hintly.online/mcp`, bearer
authenticated, behind Caddy. The hostname is temporary and borrowed.

The remote deploy has three documents, and they answer different questions:
[docs/deploy/oracle.md](docs/deploy/oracle.md) is what the machine has to be,
[docs/deploy/runbook.md](docs/deploy/runbook.md) is how to deploy onto it, and
[docs/deploy/status.md](docs/deploy/status.md) is where it currently stands —
which keeps *reported* and *verified here* apart, because a status document that
blurs those is how one starts lying.

**What comes next is in [docs/plan-status.md](docs/plan-status.md) under *Agreed order*,
and it is ordered.** Items 1 to 9 are done, item 6 struck on the evidence rather than
built. Items 5 to 8 came out of using the product against a real brief rather than from
the plan, and each carries the §100 criterion that admitted it: a delete for designs (§31
has one for projects and assets but not designs, so nothing an agent makes can be taken
back), the orphan rule in the skill, an overlay on `ImageLayer`, and the templates of §85.
What remains from the plan itself is only the optional Phase 12. Editorial style modes are
on that page too, listed as deliberately excluded, with the measurement that decided it —
read the reason before proposing them again.

## What the product is

A local design runtime **driven by agents over MCP**. The agent (Codex, Claude Code, any
MCP client) makes the design decisions; the Rust backend deterministically executes them
and renders. The app itself integrates **no LLM provider** — no OpenAI/Anthropic/Gemini
API, no image generation, no billing. The intelligence lives in the MCP host.

## Planned architecture

Two Rust processes plus a React SPA:

```
AI host --stdio MCP--> forma-mcp --HTTP localhost--> forma-server --> SQLite + local FS
                                                            ^
                                        forma-web (React) --REST--/
```

- `forma-mcp` is an internal **client of forma-server's REST API**. It must never open
  SQLite directly — a single mutation path keeps versioning, validation, and transactions
  in one authority.
- `forma-server` (Actix Web) binds `127.0.0.1:4765` and also serves the built React
  `apps/web/dist`, so the whole app is one URL. `4766` is reserved for future MCP HTTP.
- Rendering authority is Rust: `DesignDocument -> SVG (usvg) -> PNG (resvg)`. The editor
  preview and the export come from the same pipeline.

Rust workspace crates and their boundaries (§8):

| Crate | Owns | Must not depend on |
| --- | --- | --- |
| `design-core` | domain types, operations, versions | Actix, SQLx, MCP, HTTP, filesystem |
| `application` | use cases (`ApplyDesignOperations`, `RenderPreview`, …) | Actix, MCP |
| `renderer` | scene → SVG → PNG, fonts, text bbox | Actix |
| ↳ | *depends on `assets`, deliberately: see the byte-identification invariant below* | |
| `validator` | deterministic checks (bounds, text, assets, brand) | renderer, usvg |
| `assets` | upload, hashing, MIME, dimensions, sanitization, **and identifying what a pile of bytes is** | agents/MCP concepts |
| `persistence` | repository traits + sqlite/local_fs adapters (postgres/supabase later) | Actix, MCP |
| `mcp-api` | MCP tool schemas + handlers calling forma-server | design logic |

Frontend: React + TypeScript + Vite, TanStack Query (server state), Zustand (ephemeral UI
state: selection, zoom, pan, in-progress drag/resize), Tailwind. No WASM, no Yew.

## Visual direction

[docs/brand/README.md](docs/brand/README.md) is Forma's own brand direction — Ink
`#171717`, Paper `#F4F1EA`, Signal `#FF5A36`, dark-first and warm-neutral, with the logo
assets in `docs/brand/assets/`. [docs/brand/index.html](docs/brand/index.html) is the
visual version. It is **direction, not a spec**: adapt layouts and derived tones, preserve
the semantics.

Forma's identity is **not** the `BrandKit` domain type. `BrandKit` is the *user's* brand,
per project, resolved by the renderer for the designs they produce. Signal orange belongs
to the editor chrome only — never in `renderer`, never in `design-core`, never as a
default `BrandKit` value. The sample palette in plan §13 uses `#FF5436`, two characters
from Signal and unrelated to it; do not unify them.

## Invariants to preserve when implementing

- **No `any`, ever.** Not written, not leaked. `@typescript-eslint/no-explicit-any` plus
  the type-aware `no-unsafe-*` rules run as errors and `pnpm lint` uses `--max-warnings 0`,
  so a violation fails CI. Untyped data enters the app in exactly one place —
  `readJson` in [api.ts](apps/web/src/api.ts) — which funnels `Response.json()` through
  `unknown` and asserts once against a generated binding. Widen that boundary and the rules
  stop protecting anything. Prefer `unknown` plus narrowing; never reach for `any` or a
  disable comment to silence a type error.
- **Rust is the source of truth for types.** TS bindings are generated with `ts-rs` into
  `bindings/generated/` (committed), and `scripts/check-bindings.sh` fails CI when they
  drift. The export dir comes from `TS_RS_EXPORT_DIR` in `.cargo/config.toml`; ts-rs writes
  them during `cargo test`. The frontend imports them as `@forma/bindings/<Type>` and never
  hand-writes a duplicate — server DTOs included. That mapping lives in `paths` with **no
  `baseUrl`**: TypeScript resolves `paths` relative to the tsconfig without it, and
  `baseUrl` is deprecated in TS 7. Do not add it back.
- **Mutations are typed operations, not arbitrary JSON patches.** The public surface is
  `apply_operations` with a fixed `DesignOperation` enum; there is deliberately no
  `UpdateLayer { arbitrary_json }`. A patch aimed at the wrong layer type returns
  `INVALID_OP` rather than silently no-op'ing. `MAX_OPS_PER_BATCH = 20`.
- **Batches are atomic**: validate → apply to an in-memory copy → validate structure →
  persist in one transaction → single version increment.
- **The guarded UPDATE is the whole concurrency story.**
  `UPDATE designs SET version = ? ... WHERE id = ? AND version = ?`. Zero affected rows is
  ambiguous on its own, so `SqliteDesignRepository::save` then asks whether the row exists
  and returns `VERSION_CONFLICT` (refetch and rebase) or `DESIGN_NOT_FOUND` (give up) —
  never one standing in for the other. Each save writes the design row and its snapshot in
  one transaction, so a design can never exist at a version with no snapshot.
- **Optimistic concurrency**: every mutation carries `base_version`; the UPDATE is guarded
  by `WHERE version = ?`, and zero affected rows means `VERSION_CONFLICT`. Applies equally
  to React and MCP clients.
- **Undo never decrements the version.** Restoring snapshot v9 over v10 produces v11.
- **No silent leniency.** Structural errors and out-of-range values reject the batch with
  an error/hint; clamping/normalization only where explicitly documented. Warnings are for
  valid-but-suspicious states.
- **The editor has no private authoring path.** Every mutation it makes — inserting a
  layer, dragging, placing an uploaded asset — goes through the same typed
  `DesignOperation` an agent would send (§28). If the editor needs something the operation
  set cannot express, the operation set is what has to change.
- **A client never mints server-owned identity.** `PUT /projects/{id}/brand` takes a
  `BrandKitDraft` — what the brand *is* — while the `BrandId` and the kit's version are
  decided server-side, with the id kept stable across edits so the project's `brand_id`
  does not churn. Every other request body already followed this; the brand route was the
  exception.
- **Darkening a photograph is a property of the photograph.** `ImageOverlay` sits on
  `ImageLayer`; a `ShapeLayer` laid over a photo is opaque to its own edges and leaves
  a seam, which is the single most common way a generated piece announces itself.
  Anchored with `from`, the tint fades to nothing and there is no edge to see. It
  covers the *drawn* image rather than the frame, so a `contain` image is tinted where
  it is and painted bounds do not grow to the frame. Adding it also meant teaching the
  validator that an image layer can carry a colour at all — `Layer::Image(_) => {}` in
  `brand.rs` would otherwise have recreated the font-token bug in a new place.
- **`ImageLayer` holds only `asset_id`** — never a URL or a local path. Importing a remote
  image is a separate operation that first creates an `Asset`.
- **A design the validator calls valid must render.** That is the contract the agent's
  validate → fix → render → export loop (§51.7) rests on, and it is easy to break by
  skipping a check: an unresolvable `$brand.*` token has to be an error even when the
  project has no `BrandKit` at all, because the renderer treats it as fatal. Only checks
  that genuinely need a palette (`COLOR_OUTSIDE_BRAND`) or brand rules (`OUTSIDE_SAFE_AREA`,
  `FONT_TOO_SMALL`) are skipped when there is no kit. `application::renders::validate`
  skips a page that fails to render, on the assumption that the failure already has its own
  issue — so any new render failure mode needs a matching check, or it will be swallowed.
- **Legibility is two measurements, not one.** `TEXT_LOW_CONTRAST` compares the ink
  with what is drawn under it by WCAG luminance ratio *and* by perceptual distance
  (CIE76 ΔE), because an accent red on wet asphalt sits at a ratio of 1.09 and reads
  perfectly — it breaks by hue. A luminance-only check called it the worst offender
  on a cover whose author said it was the one thing that worked, and an error that
  wrong is worse than no check: it sends an agent to fix what is not broken. The
  ΔE line at 25 was calibrated on that cover, where the unreadable greys measured 12
  and the red 75. Both the mean and the per-pixel test weigh colour.
- **Brand resolution is colours *and* fonts, in one place.** `BrandPalette` carries
  both because they resolve at the same moment, and the renderer used to get only
  the colours: it wrote `$brand.display` into the SVG as if it were a family name,
  usvg found no such family and drew **nothing at all**, while the validator — which
  did resolve the token — called the design valid. A cover whose headline had
  vanished passed every check. `BrandKit::font_families` sits beside `color_palette`
  so a caller cannot build one and forget the other, and an unresolvable font token
  is fatal in the renderer exactly like an unresolvable colour.
- **Whatever the renderer can fail on, something must check first.** `validate` skips a
  page that fails to render, so a render failure with no matching check is swallowed and
  the design is reported valid. Auditing `RenderError` against the validator's codes found
  one hole: a text string carrying a control character passed `apply_operations`, validated
  clean, and answered 500 `SVG_PARSE_FAILED` at render — XML 1.0 cannot represent those
  characters at all, escaped or not. `validate_structure` now rejects them at the write
  gate and `TEXT_INVALID_CHARACTER` reports the documents written before it existed. Repeat
  that audit whenever a render failure mode is added.
- **The validator measures nothing.** Text bounds come from the renderer and are handed in
  as `MeasuredLayer`; without them the overflow checks are *skipped*, never approximated.
  A second typographic implementation would drift from the render authority. The validator
  therefore depends on `design-core` only — the renderer is a dev-dependency for one
  integration test.
- **Severity decides export, so choose it deliberately.** `OUTSIDE_CANVAS` is an error for
  text (lost copy) but a warning for a bleeding shape (§15 says leaving the canvas is not
  automatically wrong), and an error for anything *entirely* off-canvas. An off-brand
  literal color is a warning, never a rejection (§43). Checks never short-circuit: an agent
  fixing a design wants the whole list.
- **Two bounding boxes, never one.** `RenderInspection` reports `layout` (usvg's object
  box — for text it spans the line boxes, so it is what TEXT_OVERFLOW_* must compare
  against a frame) and `painted` (what survives clipping, which is what OUTSIDE_CANVAS
  must use, since a `cover` image is *meant* to overflow its frame). They differ routinely.
  usvg's own `layer_bounding_box` accounts for filters but **not** clip paths, so `painted`
  is computed by intersecting the clip explicitly — do not swap it back for usvg's value.
- **Image layers get their bytes from an `AssetSource`**, and the renderer embeds them as
  data URIs. It never resolves a path or a URL, so it needs no network or filesystem access
  while drawing. A missing asset is `ASSET_NOT_FOUND`, never a blank space.
- **One authority for what a pile of bytes is.** `assets::probe` decides format and
  dimensions for both ingest and drawing; the renderer calls it rather than keeping its
  own magic-byte reader. It used to keep one, which knew only PNG and JPEG, so an upload
  could be accepted and then go unrecognised at draw time — resvg links `png`,
  `zune-jpeg`, `gif` and `image-webp` and renders a nested SVG data URI, so all four
  formats of §44 work. An SVG keeps its size in the document rather than a header, so
  `probe` reports zero for it and the tree must be read; leaving the zero makes every SVG
  fall through `place`'s guard and silently ignore `contain` and `cover`.
- **A backup is `VACUUM INTO` plus the asset bytes, never a file copy.** A running server
  keeps writes in the write-ahead log, so copying `forma.db` can miss them and copying the
  three files separately is not atomic. `forma-server backup` is safe to run hot and
  refuses a non-empty destination. `previews/` and `exports/` are excluded on purpose and
  the manifest says so — both regenerate from what is kept. There is deliberately no
  restore command: it overwrites the user's current work, so it stays a documented manual
  procedure in [docs/backup.md](docs/backup.md).
- **Shutdown checkpoints explicitly, then closes.** `forma-server` handles SIGINT and
  SIGTERM itself so it can log the stop, lets in-flight work finish (20s, measured against
  the 8.6s a Story page at scale 4 takes), then calls `persistence::checkpoint_and_close`.
  Closing alone *usually* folds the write-ahead log back into `forma.db`, but `close()`
  returns before sqlx's workers finish, so a copy taken right after sometimes got a file
  with none of the writes — a flake that failed about one run in ten and reached CI.
  `PRAGMA wal_checkpoint(TRUNCATE)` is ordinary SQL that finishes before it returns, so
  use `checkpoint_and_close` rather than `pool.close()` anywhere the file is about to be
  read. `forma-mcp` exits when its host closes stdin, so it cannot outlive it.
- **The render cache key includes the brand version.** A `$brand.*` token resolves at
  render time, so editing a Brand Kit changes what a design looks like without touching its
  version. Leaving the brand out of the key served the picture from before the edit — a bug
  that only became reachable once the editor could edit a brand at all. Anything else the
  render depends on has to go in the key too.
- **Derived data is collected; the user's work is not.** The render cache prunes itself:
  writing version N drops everything below `N - KEEP_RECENT_VERSIONS` for that design, so
  the thing that grows the cache is the thing that trims it and no call site can forget.
  Renders of designs that no longer exist are swept at startup against `all_ids()`, because
  a project delete cascades in SQL and never touches the filesystem. Neither touches
  anything a person made: **asset bytes stay** (§74 — historical versions still reference
  them) and **version snapshots stay**, because they are the undo depth and the §61 audit
  trail. A preview is ~26 KB and always regenerable; a snapshot is ~2 KB and is not.
- **Repositories stamp their own `created_at`.** A caller cannot backdate a row, and one
  that forgets the field cannot persist a blank timestamp. Note that `create` returns a
  struct built in memory rather than re-reading the row, so a test that trusts the create
  response proves nothing about what was stored — read it back.
- **Text has no implicit word-wrap in V1.** Line breaks are explicit `\n` in the string;
  the validator detects overflow using real bounding boxes from `usvg`.
- **Errors have a stable shape** everywhere: `{"error": {"code", "message", "details"}}`.
  Codes like `VERSION_CONFLICT` / `INVALID_OP` are part of the contract the agent recovers
  against.
- `DesignDocument` carries `schema_version` (starts at 1) with a JSON-level
  `migrate_document` path — SQL migrations alone are not enough.
- Every mutation records actor metadata (`Human` / `Agent` / `System`, plus optional
  `actor_id` such as `agent:claude`).
- Bind `127.0.0.1` by default, never `0.0.0.0`; remote mode must be opted into explicitly.
- **Fonts are loaded from `FORMA_FONT_DIR`, in sorted order, and the fallback follows
  the requested weight.** Not from arbitrary system-installed fonts — and not in
  `read_dir` order either: `fontdb` enumerates a directory in whatever order the
  filesystem gives, that order is the face order, and usvg's default selector takes the
  *first* face holding a glyph (its style guard uses `&&`, so a Bold face answers a
  Regular request). The same commit therefore rendered Greek in two different weights on
  two machines. `FontRegistry::from_dir` sorts, and `fonts::fallback_selector` scores by
  weight distance, preferring a match but still using a mismatched face over tofu. The
  goldens are Latin-only and never exercise the fallback, so `tests/geist.rs` is the only
  thing guarding it.
- **The render is identical on x86_64 and aarch64.** Measured, not assumed: the same
  page rendered through the REST API on both came back byte-identical, same SHA-256.
  The rasteriser takes different SIMD paths per architecture, so this was the open
  question behind deploying to an Ampere box, and it is what lets the goldens run
  there unchanged.
- Never log tokens, auth headers, or asset binary content.

## MCP surface (14 tools, §48 plus templates)

Read: `forma_status`, `list_projects`, `get_project`, `get_brand`, `list_assets`,
`list_templates`, `get_design`, `get_asset_request`. Write: `create_design`, `apply_operations`,
`request_asset`. Output: `validate_design`, `render_preview`, `export_design`.

Keep the tool set small and composable — per-property tools (`add_text`, `move_layer`,
`set_color`) are intentionally absent because `apply_operations` covers them in a batch.
`render_preview` returns both a structured JSON result and a base64 PNG image content
block, so multimodal hosts can actually look at the piece; the base64 is deliberately
stripped from the structured half, since repeating it would double the tokens.

**Every payload goes out twice: as text and as `structuredContent`.** The spec asks for
it — "a tool that returns structured content SHOULD also return the serialized JSON in a
TextContent block" — and a host that reads only text is not hypothetical: one answered
`list_projects` with "1 project(s)" and nothing else, which makes every id-taking tool
after it unusable, because ids are the only handles there are. The duplication costs
tokens on hosts that read both; that is the smaller problem of the two.

`list_templates` is the fourteenth, added against a §48 surface fixed at thirteen because
§51.7's own V2 flow reads `get_brand → list_templates → suitable template?`. It is not a
per-property tool and nothing else can express it: "a layout that already worked here"
cannot be fetched through `apply_operations`. That number is now the bar — the next tool
needs the same kind of argument.

There is no `create_project`, no brand-editing tool, and no way to *save* a template: an
agent works inside a workspace the user owns, and deciding which design is worth keeping
is theirs. Read tools carry `readOnlyHint`. The actor recorded on every mutation is
derived from the host's own `clientInfo.name` at initialize, so §61's history reads
`agent:claude-code` rather than a generic label. Tool descriptions are the agent's only
documentation — a one-liner is not enough, and a stdio test asserts they are substantial.

When an image is missing, the agent creates an `AssetRequest` (Pending → Resolved by the
user uploading) instead of blocking or inventing an asset.

## The agent skill

[skills/forma/SKILL.md](skills/forma/SKILL.md) is the agent's operational contract, in the
thirteen sections §51.2 mandates. [AGENTS.md](AGENTS.md) is deliberately short: it says
when to load the skill and how to start the server.
[docs/agent-design-reference.md](docs/agent-design-reference.md) holds the *why* behind the
rules, and [docs/agent-evaluation.md](docs/agent-evaluation.md) the by-hand prompts for
checking an agent's judgement.

The file lives at `skills/forma/SKILL.md`, which plan §51.2 fixes and which **no host
looks in**. Claude Code reads `.claude/skills/<name>/SKILL.md`, Codex reads
`.agents/skills` walking up from the working directory, and opencode reads both; a bare
`skills/` at the repo root is nobody's convention, so the skill was invisible to every
agent that was not told to open it by path. Symlinks at both locations fix that without
moving anything — copies of a 400-line document diverge, and `skill_matches_the_code.rs`
asserts they are symlinks that still resolve.

A skill that drifts from the code is worse than none, because an agent follows it
confidently and is wrong. `apps/mcp/tests/skill_matches_the_code.rs` asserts that every
tool, operation, brand token, error code and layer field the skill names actually exists,
and that it keeps its mandated section headings. It cannot judge whether the advice is
good — that is what the evaluation prompts are for.

Per §51.11, editorial heuristics for a specific medium (carousel hooks, story arcs) stay
out of this skill; they belong in a future `social-carousel` skill so the core stays usable
for posters and covers.

## Commands

```bash
cargo test --workspace
```

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Run one test: `cargo test -p renderer reports_real_text_bounds` (substring match on the
test name). The frontend equivalent is `pnpm --filter forma-web test -- App.test`.

Run the server, which also serves `apps/web/dist` when it exists:

```bash
./target/debug/forma-server
```

Vite dev server on 5173, proxying `/api` to the Rust server (run both):

```bash
pnpm dev
```

Render the fixture to `data/previews/` to eyeball the renderer:

```bash
cargo run -p renderer --example render_fixture
```

Drive the MCP server by hand without an agent host — initialize, then list or call tools
by piping JSON-RPC lines into it:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"cli","version":"0"}}}' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | ./target/debug/forma-mcp
```

`./scripts/check.sh` runs the whole CI pipeline locally (`--rust` or `--web` to run half).
[.github/workflows/ci.yml](.github/workflows/ci.yml) runs the same commands on push to
main and on PRs, in two cached jobs. The generated-bindings freshness check of §30 is
wired up: `ts-rs` writes the bindings during `cargo test`, so `scripts/check-bindings.sh`
fails the run when the committed ones have drifted. Golden renderer tests need no
separate step: they are ordinary workspace tests.

`forma-mcp` logs to **stderr**, never stdout — stdout carries the MCP protocol, and a
stray println breaks the transport.

**Golden renders** live in `fixtures/golden/`, compared with a pixel diff whose threshold
was measured rather than invented (§93.3): re-rendering is byte-identical, so the noise
floor is 0%, while the smallest change a person would call a regression — a one-pixel
nudge — moves 0.15% of the canvas. The threshold sits at 0.01%, and `measured_sensitivity`
re-measures that gap on every run so it cannot quietly stop meaning anything. To update a
golden: `UPDATE_GOLDEN=1 cargo test -p renderer --test golden`, then **look at the images**
before committing — an updated golden nobody reviewed is a test that stopped testing.

**Logging** is one span per request carrying §60's fields. `request_id`, `project_id`,
`design_id`, `version`, `actor_type`, `actor_id` and `render_ms` are recorded by the shared
helpers in `routes/helpers.rs` rather than by each handler, so a handler cannot forget.
`tool_name` arrives as an `x-forma-tool` header from `forma-mcp`, and `x-request-id`
crosses the process boundary so one trace covers both. Never log tokens, auth headers or
asset bytes.

Testing layers: unit tests per crate; **golden rendering tests** comparing renders of
`fixtures/designs/` against `fixtures/golden/` with a documented pixel-diff threshold
(each golden change needs visual review — do not update goldens mechanically); integration
tests over a temp SQLite doing the full create → operations → validate → render → export
transaction; MCP tests via the in-process transport plus a stdio smoke test; Vitest +
React Testing Library on the frontend; and the Playwright walkthrough in `e2e/`, which
drives a real browser against a real server serving the built app (§67 — Playwright is for
testing our app; the plan explicitly forbids driving external AI services with it).

The end-to-end layer is the only one that fakes nothing: Vitest mocks `fetch`, the Rust
integration tests have no browser, the MCP tests have no person. It exists to catch the
failure where the backend can do something and the interface never exposes it — which
shipped twice before it existed. It covers the browser half of §79; steps 6 to 14 are the
agent's and are covered in `apps/mcp/tests/agent_flow.rs`. **Never wait on a clock in it** —
wait on a condition, or it becomes the flaky layer everyone learns to ignore.

## Config

```
FORMA_BIND=127.0.0.1:4765
FORMA_DATA_DIR=./data
FORMA_DATABASE_URL=sqlite://data/forma.db
FORMA_FONT_DIR=./data/fonts
FORMA_LOG=info

# Phase 11.5. `local` (default) or `supabase`; the second is specified in plan
# §110-§112 and not built yet, so the server refuses to start with it.
FORMA_AUTH_MODE=local

# Remote only (plan §57). Unset locally.
FORMA_REMOTE_TOKEN=
FORMA_MCP_BIND=127.0.0.1:4766
FORMA_SERVER_URL=http://127.0.0.1:4765
```

**"No credential configured" is not "no credential required".** It is only the
same thing in `FORMA_AUTH_MODE=local`, where the boundary is the process that
started Forma. `require_bearer`'s pass-through is gated on that mode, and
`binding_is_safe` in hosted mode asks whether identity is configured rather than
where the process binds — because the old premise, *loopback means only this
machine can reach it*, stopped being true the day Caddy was put in front of
loopback. It was masked only because a token happened to be set.

**Reaching the network without a token is the one configuration that refuses to
start.** `Config::binding_is_safe` rejects a non-loopback `FORMA_BIND` with no
`FORMA_REMOTE_TOKEN`, and `forma-mcp --transport http` refuses outright — an HTTP
port is reachable by anything that can route to it, unlike stdio, so there is no
local case where running it open is what someone meant. An address that does not
parse counts as remote: a hostname may resolve anywhere, and guessing permissively
is the wrong way round for that question. The token is held as a blake3 hash whose
comparison is constant time, in both processes, and one credential serves the whole
deployment — the host presents it to `forma-mcp`, which presents it onward to
`forma-server`.

Logging is `tracing`/`tracing-subscriber` with structured fields: `request_id`,
`project_id`, `design_id`, `version`, `tool_name`, `render_ms`, `actor_type`, `actor_id`.

## Explicitly out of scope for V1 (§2.2)

Image generation, any LLM provider API, stock search, social publishing, realtime
collaboration, OAuth, payments, WASM/Yew, video, animation, Figma-style auto-layout,
advanced masks, pixel editing, groups (`GroupLayer`), skew transforms, and a mandatory
Supabase dependency. Supabase and Postgres, if they arrive, are persistence adapters —
never a domain dependency. Treat plan §100 ("criterio para añadir una feature") as the
gate before widening scope.

## Language

The plan was originally written in Spanish and was translated to English in full on
2026-08-27, then renamed throughout to **Forma** (processes `forma-server`/`forma-mcp`/
`forma-web`, `FORMA_*` env vars, `data/forma.db`, the `forma_status` MCP tool, and
`skills/forma/SKILL.md`). Domain types keep the `Design*` prefix — `DesignDocument`,
`DesignRecord`, `DesignOperation` — because that names the domain concept, not the
product. Keep code, schema names, and documentation in English.
