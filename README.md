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

Phase 0 of the plan is complete: the monorepo, the domain core, the render
pipeline and a working MCP server over stdio. There is no editor and no design
persistence yet — those are Phases 4 to 6.

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

`pnpm dev` runs Vite on port 5173 and proxies `/api` to `forma-server`, so run
the server alongside it.

## Layout

| Path | What it is |
| --- | --- |
| `crates/design-core` | Domain types. No infrastructure dependencies. |
| `crates/renderer` | `DesignDocument -> SVG -> PNG`, fonts, bounds inspection. |
| `apps/server` | Actix Web REST API, SQLite, static file serving. |
| `apps/mcp` | MCP server over stdio; a REST client of `apps/server`. |
| `apps/web` | React + TypeScript + Vite editor. |
| `data/fonts` | Fonts the renderer loads. Committed so renders match everywhere. |

## License

MIT.
