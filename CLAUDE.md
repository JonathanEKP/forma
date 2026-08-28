# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Current state

[forma-plan.md](forma-plan.md) is a ~4,200-line execution plan and the spec of record.
Before implementing anything, read the relevant section (numbered headings —
`grep -n '^#\{1,3\} '` lists them).

**Phases 0 to 2 are complete.** The workspace, domain core, operations, full render
pipeline (text, shapes, images), REST server and MCP stdio server all exist and are tested.
The blocking spike of §36 resolved to **Option A**: `<g id="layer-{uuid}">` wrappers survive
usvg normalization, so the editor can address rendered geometry by layer id directly.

Next is **Phase 3 (Validator)**. What does NOT exist yet: design persistence (only a
`projects` table), the REST/MCP surface for designs, the validator crate, and the editor.
The golden-test baseline of §63 is also still missing — §93.3 says not to invent a
pixel-diff threshold before there are real renders to measure, and now there are.

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
| `validator` | deterministic checks (bounds, text, assets, brand) | |
| `assets` | upload, hashing, MIME, dimensions, sanitization | agents/MCP concepts |
| `persistence` | repository traits + sqlite/local_fs adapters (postgres/supabase later) | |
| `mcp-api` | MCP tool schemas + handlers calling forma-server | design logic |

Frontend: React + TypeScript + Vite, TanStack Query (server state), Zustand (ephemeral UI
state: selection, zoom, pan, in-progress drag/resize), Tailwind. No WASM, no Yew.

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
  hand-writes a duplicate — server DTOs included.
- **Mutations are typed operations, not arbitrary JSON patches.** The public surface is
  `apply_operations` with a fixed `DesignOperation` enum; there is deliberately no
  `UpdateLayer { arbitrary_json }`. A patch aimed at the wrong layer type returns
  `INVALID_OP` rather than silently no-op'ing. `MAX_OPS_PER_BATCH = 20`.
- **Batches are atomic**: validate → apply to an in-memory copy → validate structure →
  persist in one transaction → single version increment.
- **Optimistic concurrency**: every mutation carries `base_version`; the UPDATE is guarded
  by `WHERE version = ?`, and zero affected rows means `VERSION_CONFLICT`. Applies equally
  to React and MCP clients.
- **Undo never decrements the version.** Restoring snapshot v9 over v10 produces v11.
- **No silent leniency.** Structural errors and out-of-range values reject the batch with
  an error/hint; clamping/normalization only where explicitly documented. Warnings are for
  valid-but-suspicious states.
- **`ImageLayer` holds only `asset_id`** — never a URL or a local path. Importing a remote
  image is a separate operation that first creates an `Asset`.
- **Two bounding boxes, never one.** `RenderInspection` reports `layout` (usvg's object
  box — for text it spans the line boxes, so it is what TEXT_OVERFLOW_* must compare
  against a frame) and `painted` (what survives clipping, which is what OUTSIDE_CANVAS
  must use, since a `cover` image is *meant* to overflow its frame). They differ routinely.
  usvg's own `layer_bounding_box` accounts for filters but **not** clip paths, so `painted`
  is computed by intersecting the clip explicitly — do not swap it back for usvg's value.
- **Image layers get their bytes from an `AssetSource`**, and the renderer embeds them as
  data URIs. It never resolves a path or a URL, so it needs no network or filesystem access
  while drawing. A missing asset is `ASSET_NOT_FOUND`, never a blank space.
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
block, so multimodal hosts can actually look at the piece.

When an image is missing, the agent creates an `AssetRequest` (Pending → Resolved by the
user uploading) instead of blocking or inventing an asset.

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
still a TODO in that workflow — wire it up with `ts-rs` in Phase 1. Golden renderer tests
need no separate step: they are ordinary workspace tests.

`forma-mcp` logs to **stderr**, never stdout — stdout carries the MCP protocol, and a
stray println breaks the transport.

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
