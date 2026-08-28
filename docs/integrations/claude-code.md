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
document schema version and the available font families.

If it answers `SERVER_UNREACHABLE`, `forma-server` is not running — start it
rather than retrying the tool.

For a real end-to-end check, create a project in the editor at
<http://127.0.0.1:4765> and give it a Brand Kit, then ask Claude Code for a
piece. It should call `get_brand`, `create_design`, `apply_operations`,
`validate_design` and `render_preview` — the last returns the PNG as an MCP
image block, so a multimodal host can look at what it made.

There is deliberately no tool for creating a project or editing a Brand Kit: an
agent works inside a workspace you own.

## Notes

- Forma calls no LLM API of its own. Claude Code supplies the intelligence; Forma
  executes and renders deterministically.
- Logs go to stderr, because stdout carries the MCP protocol.
