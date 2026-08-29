# Agents

Forma is a local design runtime driven over MCP. The agent decides the design;
Forma executes and renders it deterministically.

## When to load the skill

Load [skills/forma/SKILL.md](skills/forma/SKILL.md) before touching a design.

If your host discovers skills itself, it already has this one: the same file is
reachable at `.claude/skills/forma/SKILL.md` (Claude Code) and
`.agents/skills/forma/SKILL.md` (Codex, opencode), both symlinks to the path
above. To use Forma from a project that is not this repository, copy or link the
skill into `~/.claude/skills/forma/` or `~/.agents/skills/forma/`.
It is the operational contract: the document schema, the valid fields, the
atomic batch rules, the error recovery table and a worked session. Working from
memory instead is how an agent invents a field the renderer cannot draw.

## Starting Forma

```bash
cargo build --release
```

```bash
./target/release/forma-server
```

The server listens on `127.0.0.1:4765` and serves the editor at that URL. It has
to be running before the MCP server is any use: `forma-mcp` is a client of its
REST API, not of the database.

Then register the MCP server with your host — see
[docs/integrations/claude-code.md](docs/integrations/claude-code.md) or
[docs/integrations/codex.md](docs/integrations/codex.md).

## What an agent cannot do

Create a project, or edit a Brand Kit. Both belong to the person, and there is
deliberately no tool for either. If the project or the kit you need does not
exist, say so and ask.

Forma also generates no images. When a design needs one that does not exist, an
agent records an `AssetRequest` and a person uploads it.

## Working on Forma itself

That is a different job, and [CLAUDE.md](CLAUDE.md) covers it.
