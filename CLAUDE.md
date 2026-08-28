# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Current state

[forma-plan.md](forma-plan.md) is a ~4,200-line execution plan and the spec of record.
Before implementing anything, read the relevant section (numbered headings —
`grep -n '^#\{1,3\} '` lists them).

**Phases 0 to 9 are complete.** The workspace, domain core and its operations, the full
render pipeline (text, shapes, images), the validator, SQLite persistence with optimistic
versioning and a snapshot per version, the `application` use cases of §8.2, the `assets`
ingest of §8.5, the whole REST surface of §31, the React editor, the AssetRequest flow, and
all thirteen MCP tools, and the agent skill all exist and are tested. The blocking spike of §36 resolved to **Option A**: `<g id="layer-{uuid}">`
wrappers survive usvg normalization, and the editor addresses rendered geometry by layer
id directly.

**Phase 10 (hardening) is under way.** Done: the golden-render baseline of §63 and the
structured logging of §60. Still open from that list: backup, cleanup (nothing is ever
deleted — soft-deleted assets, old version snapshots and the preview cache all grow
without bound), and graceful shutdown. Beyond it: the brand panel — `Panel` in `store.ts`
still carries a `"brand"` variant with no component behind it — the Playwright E2E of §67,
and the remote deploy of Phases 11 and 12.

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
- Fonts are loaded from `FORMA_FONT_DIR`, not from arbitrary system-installed fonts.
- Never log tokens, auth headers, or asset binary content.

## MCP surface (13 tools, §48)

Read: `forma_status`, `list_projects`, `get_project`, `get_brand`, `list_assets`,
`get_design`, `get_asset_request`. Write: `create_design`, `apply_operations`,
`request_asset`. Output: `validate_design`, `render_preview`, `export_design`.

Keep the tool set small and composable — per-property tools (`add_text`, `move_layer`,
`set_color`) are intentionally absent because `apply_operations` covers them in a batch.
`render_preview` returns both a structured JSON result and a base64 PNG image content
block, so multimodal hosts can actually look at the piece; the base64 is deliberately
stripped from the structured half, since repeating it would double the tokens.

There is no `create_project` and no brand-editing tool: an agent works inside a workspace
the user owns. Read tools carry `readOnlyHint`. The actor recorded on every mutation is
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
React Testing Library on the frontend; Playwright E2E against Forma itself (Playwright is
for testing our app — the plan explicitly forbids driving external AI services with it).

## Config

```
FORMA_BIND=127.0.0.1:4765
FORMA_DATA_DIR=./data
FORMA_DATABASE_URL=sqlite://data/forma.db
FORMA_FONT_DIR=./data/fonts
FORMA_LOG=info
```

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
