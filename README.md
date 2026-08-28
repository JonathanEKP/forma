# Forma

A local design runtime for AI agents.

You ask your usual agent — Claude Code, Codex, any MCP client — for a piece. The
agent reads your Brand Kit and assets, then creates or edits a structured design
document. Forma validates it, renders a deterministic SVG, rasterizes the same
SVG to PNG, and hands the preview back so the agent can look at its own work and
correct it. You can open the React editor and edit the very same document by
hand.

**Forma integrates no LLM provider.** No OpenAI API, no Anthropic API, no image
generation, no separate billing. The intelligence lives in the host you already
pay for; Forma only executes design decisions deterministically.

The full specification is [forma-plan.md](forma-plan.md).

## Status

Phases 0 to 9 of the plan are complete: the domain core and its operations, the
render pipeline, the validator, SQLite persistence with optimistic versioning,
asset ingest with SVG sanitizing, the whole REST API of the plan's section 31 —
projects, brand kits, designs, the operations batch, undo, history, validation,
previews, uploads, asset requests and exports — the React editor, which edits a
design through the same typed operations an agent would send, and the
AssetRequest flow: when an agent needs an image that does not exist, it records
a request, and the assets panel is where you see it, upload the image and watch
the request resolve.

It also speaks MCP over stdio, with the thirteen tools of the plan's section 48:
an agent reads the Brand Kit, creates a design, edits it in atomic batches,
validates it against real measurements, and gets the rendered PNG back as an
image it can actually look at.

Agents get [a skill](skills/forma/SKILL.md): the document schema, the atomic
batch rules, the error recovery table and a worked session, so a host does not
have to be told how to drive Forma call by call.

Hardening is under way: golden render tests, structured request logging,
render-cache cleanup and graceful shutdown are in. Still to come: backup, a
brand panel, and the remote deploy.

## Requirements

Rust (2021 edition), Node 20+, pnpm.

## Running it

```bash
cargo build --release
```

```bash
pnpm install && pnpm build
```

```bash
./target/release/forma-server
```

Then open <http://127.0.0.1:4765>. The server serves the built React app from the
same origin, so that URL is the whole product.

To connect an agent, see [docs/integrations/claude-code.md](docs/integrations/claude-code.md)
or [docs/integrations/codex.md](docs/integrations/codex.md).

## Development

```bash
cargo test --workspace
```

```bash
pnpm dev
```

Before pushing, run everything CI runs:

```bash
./scripts/check.sh
```

`pnpm dev` runs Vite on port 5173 and proxies `/api` to `forma-server`, so run
the server alongside it.

## Layout

| Path | What it is |
| --- | --- |
| `crates/design-core` | Domain types. No infrastructure dependencies. |
| `crates/renderer` | `DesignDocument -> SVG -> PNG`, fonts, bounds inspection. |
| `crates/validator` | Deterministic checks. Measures nothing itself. |
| `crates/persistence` | Repository traits plus SQLite and local-filesystem adapters. |
| `crates/application` | Use cases. Knows nothing about Actix or MCP. |
| `crates/assets` | Upload ingest: magic bytes, hashing, SVG sanitizing. |
| `apps/server` | Actix Web REST API, SQLite, static file serving. |
| `apps/mcp` | MCP server over stdio; a REST client of `apps/server`. |
| `apps/web` | React + TypeScript + Vite editor. |
| `data/fonts` | Fonts the renderer loads. Committed so renders match everywhere. |
| `docs/brand` | Forma's own visual direction and logo assets. Not the `BrandKit` type. |
| `skills/forma` | The agent's operational contract. |

## License

MIT.
