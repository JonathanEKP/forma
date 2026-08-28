# Codex

Codex acts as an MCP client over stdio (plan section 53).

> Verify this against the Codex version you have installed. The plan deliberately
> avoids freezing a configuration that upstream may change, and the exact key
> names below should be confirmed before relying on them.

## 1. Build and start the server

```bash
cargo build --release
```

```bash
./target/release/forma-server
```

## 2. Point Codex at `forma-mcp`

Codex reads its MCP servers from its own configuration file. The entry needs the
absolute path to the binary and, if the server is not on the default port, the
`FORMA_SERVER_URL` environment variable:

```toml
[mcp_servers.forma]
command = "/absolute/path/to/forma/target/release/forma-mcp"
args = ["--transport", "stdio"]

[mcp_servers.forma.env]
FORMA_SERVER_URL = "http://127.0.0.1:4765"
```

Keep personal absolute paths out of this repository.

## 3. Verify

Call `forma_status` first. Then, with a project and a Brand Kit created in the
editor, ask for a piece: the flow is `get_brand`, `create_design`,
`apply_operations`, `validate_design`, `render_preview`.

`render_preview` returns a real PNG as an MCP image block. If your Codex build
cannot display images, do not claim the design looks right — report that the
mechanical validation passed and leave the visual check to the user (plan
section 51.8).
