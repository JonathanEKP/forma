# Claude Code

Claude Code connects to local MCP servers over stdio, which is what Forma ships
in V1 (plan section 54).

## 1. Build the binaries

```bash
cargo build --release
```

## 2. Start `forma-server`

`forma-mcp` is a client of the server's REST API, not of the database, so the
server has to be running:

```bash
./target/release/forma-server
```

It listens on `127.0.0.1:4765` and serves the built React app from the same
origin when `apps/web/dist` exists.

## 3. Register the MCP server

From the repository root:

```bash
claude mcp add --transport stdio forma -- "$PWD/target/release/forma-mcp"
```

If `forma-server` runs on a different port, pass the URL through the
environment:

```bash
claude mcp add --transport stdio forma --env FORMA_SERVER_URL=http://127.0.0.1:4765 -- "$PWD/target/release/forma-mcp"
```

Use an absolute path for the binary. Do not commit a path from your own machine
into the repository.

## 4. Verify

Ask Claude Code to call `forma_status`. It should report the server version, the
document schema version and the available font families. Then ask it to call
`render_test_card`; it returns a PNG as an MCP image block, so a multimodal host
can actually look at it.

If `forma_status` answers `SERVER_UNREACHABLE`, `forma-server` is not running —
start it rather than retrying the tool.

## Notes

- Forma calls no LLM API of its own. Claude Code supplies the intelligence; Forma
  executes and renders deterministically.
- Logs go to stderr, because stdout carries the MCP protocol.
