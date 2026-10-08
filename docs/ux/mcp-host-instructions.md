# MCP host connection instructions

Research date: 2026-10-06. This guide records each host's current documented remote MCP setup; its linked vendor docs are the authority if labels move.

## What Forma offers

Forma's remote MCP endpoint is one Streamable HTTP URL: copy the account URL shown on **/agents/mcp** (production URLs end in `/mcp`). The HTTP transport is selected with `forma-mcp --transport http`; `apps/mcp/src/main.rs:23-48` documents the server URL, bind address, deployment host and issuer settings, and `apps/mcp/src/http.rs:24-28` installs the Streamable HTTP service. Production places this behind a reverse proxy at `/mcp`.

Forma requires authentication. A public deployment advertises a protected-resource document at `/.well-known/oauth-protected-resource/mcp`; `apps/mcp/src/http.rs:61-105` sets the resource to `https://<FORMA_DOMAIN>/mcp`, points `authorization_servers` at `FORMA_SUPABASE_ISSUER`, and advertises bearer headers. Forma publishes that metadata; OAuth authorization, token exchange and dynamic client registration belong to the configured authorization server. `docs/deploy/status.md:210-218` records the hosted PKCE S256 + dynamic-registration flow as verified, while noting Claude connector end to end was not run. For hosts that cannot use that OAuth flow, a Forma personal access token (PAT), prefixed `forma_pat_`, is a bearer credential. The API returns the secret only on creation and stores a hash (`apps/server/src/routes/api_keys.rs:1-5,27-48`; `crates/persistence/src/sqlite/api_key.rs:13,57-85`); account-scoped create/list/revoke routes are in `apps/server/src/routes/api_keys.rs:27-72`. The UI shows a newly created PAT once on /agents/mcp. The internal `FORMA_REMOTE_TOKEN` in `apps/mcp/src/main.rs:43-68` is the MCP service's credential to its backend, not a value the person pastes into a host.

**Repository note (resolved 2026-10-07).** `CLAUDE.md` now says `forma-mcp --transport http` serves MCP on port 4766 by default, and `apps/mcp/src/main.rs` says the service supports stdio and Streamable HTTP.

Use OAuth when the host supports it and PAT headers when the host supports secure header configuration. Never put a PAT in a checked-in or shared config file. A local loopback MCP URL only works for apps running on that same machine; Claude web/Desktop and ChatGPT connect from their cloud services and need the public Forma URL.

## Claude web and Claude Desktop

**Support.** Remote MCP over Streamable HTTP: yes. OAuth: yes, including automatic registration. Bearer request headers: available in limited beta. Plan: custom connectors are available across Free, Pro, Max, Team and Enterprise; Free is limited to one connector. Team Owners/Primary Owners, and Enterprise Owners/Primary Owners or a custom role with Manage access to Libraries, add the organization connector. Claude Desktop remote connectors are cloud-connected; they are not configured through `claude_desktop_config.json`.

**Steps.**

1. In Claude, open **Customize → Connectors → + Add → Add custom connector**. Team/Enterprise owners use **Organization settings → Connectors → Add → Custom → Web**.
2. Enter a name such as **Forma** and paste `<FORMA_MCP_URL>`; choose **Continue**.
3. Choose **Sign in now**, then select **Register automatically** for the OAuth client and finish the Forma authorization in the browser.
4. On Team/Enterprise, members then open **Customize → Connectors → Connect**.

**Credential and pitfalls.** Use Forma OAuth. The cloud service must reach the public URL; a loopback URL will not work. A PAT is only an option for accounts with Anthropic's limited-beta fixed request-header feature: select **No sign in** and add `Authorization: Bearer <TOKEN>` as a request header. Prefer OAuth. Desktop uses the same remote connector flow.

**Verification.** Docs only; not verified here because this host requires a public URL.

**Sources (read 2026-10-06):** [Anthropic: custom remote connectors](https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp); [Claude connector setup](https://claude.com/docs/connectors/custom/add-unlisted).

## ChatGPT

**Support.** Remote Streamable HTTP MCP: yes. OAuth: yes. A static bearer header/PAT field in the ChatGPT web connector flow: no. Developer mode is available on Pro, Business, Enterprise and Edu; Pro supports read/fetch tools, while full write actions are available on Business, Enterprise and Edu. Business workspace owners/admins enable access; Enterprise/Edu admins control it through RBAC.

**Steps.**

1. Enable developer mode: on Business, an owner/admin opens **Workspace settings → Permissions & Roles → Connected Data**; Enterprise/Edu users first need RBAC access, then use **Settings → Apps → Advanced Settings**; Pro users enable it from **Settings → Apps** and can use read/fetch tools only.
2. Select **Workspace settings → Apps → Create** (admins/owners) or **Settings → Apps → Create** (authorized users), enter the app name and `<FORMA_MCP_URL>`, and choose OAuth.
3. Select **Scan Tools**, complete Forma's browser authorization, then choose **Create**.
4. Test the draft in a new chat from the tools menu. A Business admin/owner or authorized Enterprise/Edu publisher publishes it from **Workspace settings → Apps → Drafts**.

**Credential and pitfalls.** Use Forma OAuth. The ChatGPT custom app flow does not document an arbitrary bearer-header field for a Forma PAT, and Forma is not an unauthenticated server. Ensure the authorization server advertises and issues refresh tokens for `offline_access`; otherwise the person may need to authorize again. Admins should refresh the published tool snapshot after Forma's tools change.

**Verification.** Docs only; not verified here because ChatGPT's cloud service requires a public URL.

**Sources (read 2026-10-06):** [OpenAI Help: developer mode and MCP apps](https://help.openai.com/en/articles/12584461-developer-mode-and-mcp-apps-in-chatgpt).

## Claude Code

**Support.** Remote Streamable HTTP: yes. OAuth and fixed bearer headers: yes. No MCP-specific subscription tier is listed in the current MCP docs; organization policy can still restrict servers.

**Steps (PAT).**

1. Create a Forma PAT, then provide it to the shell without saving it in a config file: `read -rsp 'Forma PAT: ' FORMA_TOKEN; echo; export FORMA_TOKEN`.
2. In the project directory, add the server: `claude mcp add --transport http --scope project forma "<FORMA_MCP_URL>"`.
3. In the generated project `.mcp.json`, add the header entry shown below, then run `claude` and approve the project MCP config when prompted.

```json
{
  "mcpServers": {
    "forma": {
      "type": "http",
      "url": "<FORMA_MCP_URL>",
      "headers": {
        "Authorization": "Bearer ${FORMA_TOKEN}"
      }
    }
  }
}
```

Alternatively, configure the remote server without a PAT and run `claude mcp login forma` to use OAuth. Check this server with `claude mcp get forma`.

**Credential and pitfalls.** The PAT is read from the environment at runtime. Claude Code can open the OAuth browser flow; command-line use does not mean browser auth is unavailable. Project-scoped config is pending approval until accepted in an interactive session. Do not use user or local scope when you need this connection to stay project-only.

**Verification (local).** Connected to the isolated local Forma MCP with `claude --print --output-format text --no-session-persistence --strict-mcp-config --mcp-config .mcp.json --max-budget-usd 0.25 'List the tool names available from the forma MCP server. Do not invoke any tools.'`. Sanitized result: listed 14 tools. The project config used `Authorization: Bearer ${FORMA_TOKEN}`; no token was stored in the config.

**Sources (read 2026-10-06):** [Anthropic: Claude Code MCP](https://code.claude.com/docs/en/mcp).

## Codex CLI

**Support.** Remote Streamable HTTP: yes. OAuth and bearer token from an environment variable: yes. No MCP-specific plan gate is listed; account or managed-organization policy may restrict servers.

**Steps (PAT).**

1. Set the environment variable at a shell prompt (not in config): `read -rsp 'Forma PAT: ' FORMA_TOKEN; echo; export FORMA_TOKEN`.
2. Register the remote server: `codex mcp add forma --url "<FORMA_MCP_URL>" --bearer-token-env-var FORMA_TOKEN`.
3. Check with `codex mcp list` or open the TUI and run `/mcp`.

For OAuth instead, omit `--bearer-token-env-var FORMA_TOKEN` and run `codex mcp login forma` after adding the server.

**Credential and pitfalls.** The PAT value stays in the process environment; Codex stores only the variable name in config. If you choose a project-scoped `.codex/config.toml`, Codex reads it only in trusted projects. Managed Codex configuration may block user-defined servers.

**Verification (local).** In an isolated `CODEX_HOME`, ran `codex mcp add forma --url http://127.0.0.1:49166/mcp --bearer-token-env-var FORMA_TOKEN`, then `/mcp` in the TUI. Sanitized result: **forma connected (14 tools)**. The scratch `auth.json` was a symlink to the existing Codex auth file; no Codex global config was touched.

**Sources (read 2026-10-06):** [OpenAI: Codex MCP servers](https://developers.openai.com/codex/mcp).

## Gemini CLI

**Support.** Remote HTTP MCP: yes (Streamable HTTP and SSE). OAuth: yes; the CLI starts browser auth after a 401 and supports DCR. Bearer headers: yes, but current docs do not document environment-variable expansion for a remote HTTP `headers` value. No MCP-specific plan gate is listed; Google account/project policy can still apply, and organization policy can block user-added MCP servers.

**Steps (OAuth recommended).**

1. Add the endpoint: `gemini mcp add --transport http forma "<FORMA_MCP_URL>"`.
2. Start `gemini`, run `/mcp auth forma`, and complete authorization in the browser. Gemini discovers the authorization server from Forma's challenge.
3. Check the connection with `/mcp list`.

**Credential and pitfalls.** Use Forma OAuth to avoid persisting a PAT in Gemini's remote-header config. A Forma PAT can be sent with `gemini mcp add --transport http --header "Authorization: Bearer <TOKEN>" forma "<FORMA_MCP_URL>"`, but the docs do not describe a secure environment reference for that HTTP header. `GEMINI_API_KEY` authenticates Gemini model access, not Forma.

**Verification.** Docs only; Gemini CLI was not installed, and no software was installed for this research.

**Sources (read 2026-10-06):** [Gemini CLI: MCP servers](https://github.com/google-gemini/gemini-cli/blob/main/docs/tools/mcp-server.md); [Gemini CLI: enterprise policy](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/enterprise.md).

## Cursor

**Support.** Remote Streamable HTTP and SSE: yes. OAuth and bearer headers: yes. No MCP-specific IDE plan gate is listed; team administrators can distribute servers and Enterprise policy can allowlist MCP servers.

**Steps (PAT).**

1. Set `FORMA_TOKEN` in the environment Cursor inherits (use a secret manager or a shell prompt; do not put the token in the file).
2. Add a project `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "forma": {
      "url": "<FORMA_MCP_URL>",
      "headers": {
        "Authorization": "Bearer ${env:FORMA_TOKEN}"
      }
    }
  }
}
```

3. Open **Customize** and verify Forma is connected. For OAuth, configure the URL without a bearer header and complete the browser prompt.

**Credential and pitfalls.** The config references an environment variable, not its value. A workspace admin allowlist can prevent the server from starting.

**Verification.** Docs only; Cursor was not installed, and no software was installed for this research.

**Sources (read 2026-10-06):** [Cursor: MCP](https://cursor.com/docs/mcp); [Cursor: plans](https://cursor.com/pricing); [Cursor: team MCP controls](https://cursor.com/docs/mcp).

## Visual Studio Code with GitHub Copilot

**Support.** Remote Streamable HTTP (with SSE fallback): yes. OAuth and bearer headers: yes. MCP needs VS Code 1.99 or newer. Copilot Business/Enterprise requires Copilot access and the organization MCP policy enabled; enterprise admins can restrict server use.

**Steps (OAuth).**

1. Open the Command Palette and run **MCP: Add Server**.
2. Choose **HTTP**, enter `<FORMA_MCP_URL>`, name it `forma`, and save to the workspace `.mcp.json` portable format.
3. Start the server and complete the OAuth browser flow. Check with **MCP: List Servers**.

For a PAT, use the workspace file `.mcp.json` with a password prompt instead of embedding the token:

```json
{
  "mcpServers": {
    "forma": {
      "type": "http",
      "url": "<FORMA_MCP_URL>",
      "headers": {
        "Authorization": "Bearer ${input:forma-token}"
      }
    }
  },
  "inputs": [
    {
      "id": "forma-token",
      "type": "promptString",
      "description": "Forma personal access token",
      "password": true
    }
  ]
}
```

**Credential and pitfalls.** Prefer OAuth. The password input is stored securely by VS Code. Interactive `${input:...}` settings in the legacy `.vscode/mcp.json` format are not forwarded to the Agent Host; use the portable `.mcp.json` or `~/.copilot/mcp-config.json` format for Agent Host sessions. An organization policy may disable MCP.

**Verification.** Docs only; VS Code was not installed for this research.

**Sources (read 2026-10-06):** [VS Code: MCP servers](https://code.visualstudio.com/docs/agent-customization/mcp-servers); [VS Code: MCP configuration](https://code.visualstudio.com/docs/agents/reference/mcp-configuration); [GitHub Copilot: MCP in the IDE](https://docs.github.com/en/copilot/how-tos/copilot-in-your-ide/customize-copilot/extend-copilot-chat-with-mcp); [GitHub Copilot: organization policies](https://docs.github.com/en/copilot/how-tos/administer-copilot/manage-for-organization/manage-policies).

## Windsurf / Cascade

**Support.** The current Windsurf MCP documentation redirects to Cognition's Cascade documentation, which explicitly applies to the legacy Cascade agent. Devin Desktop documents remote HTTP, OAuth, and bearer headers with environment interpolation. Enterprise users must enable MCP in settings; team admins can allowlist servers, and once an allowlist exists, non-allowlisted servers are blocked. Cascade has a 100-tool cap.

**Steps (PAT, legacy Cascade).**

1. Set `FORMA_TOKEN` in the environment Cascade inherits.
2. In Cascade, open **… → MCPs → Open MCP config file** and add Forma to `~/.config/devin/mcp_config.json` (Windows: `%APPDATA%\devin\mcp_config.json`):

```json
{
  "mcpServers": {
    "forma": {
      "serverUrl": "<FORMA_MCP_URL>",
      "headers": {
        "Authorization": "Bearer ${env:FORMA_TOKEN}"
      }
    }
  }
}
```

3. Save the file, return to Cascade, open the MCPs section in the Actions menu, and check the server status. Toggle Forma on if it is disabled.

**Credential and pitfalls.** Use OAuth if preferred by leaving out the header and completing the browser flow. This recipe is for legacy Cascade; the current Windsurf docs redirect to Devin, so confirm the actual product and menu labels before putting it on Forma's page.

**Verification.** Docs only; Windsurf was not installed, and no software was installed for this research.

**Sources (read 2026-10-06):** [Windsurf MCP docs (redirects to Devin Cascade)](https://docs.windsurf.com/windsurf/cascade/mcp); [Cognition: Cascade MCP](https://docs.devin.ai/desktop/cascade/mcp).

## OpenCode

**Support.** Remote Streamable HTTP: yes. OAuth with discovery/DCR and fixed bearer headers: yes. Environment interpolation in HTTP headers: yes. No separate MCP plan requirement is listed; model-provider authentication is separate.

**Steps (PAT).**

1. Set `FORMA_TOKEN` in the environment OpenCode inherits.
2. Add a project-root `opencode.json`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "forma": {
      "type": "remote",
      "url": "<FORMA_MCP_URL>",
      "enabled": true,
      "oauth": false,
      "headers": {
        "Authorization": "Bearer {env:FORMA_TOKEN}"
      }
    }
  }
}
```

3. Run `opencode mcp list` or `opencode mcp debug forma` to check the remote connection.

For OAuth, omit `headers` and `oauth: false`, then run `opencode mcp auth forma` and finish the browser flow.

**Credential and pitfalls.** The project config stores only the environment variable reference. Keep Forma auth separate from your model provider login. OAuth can be disabled by the `oauth: false` setting in the PAT example.

**Verification (local).** In an isolated project and config directory, ran `opencode mcp list` against `http://127.0.0.1:49166/mcp`. Sanitized result: **forma connected**.

**Sources (read 2026-10-06):** [OpenCode: MCP servers](https://opencode.ai/docs/mcp-servers/); [official OpenCode repository](https://github.com/anomalyco/opencode).

## Earlier recommendation for the /agents/mcp page

The owner decisions below supersede this recommendation where they differ.

Keep the approved **Pick your app** cards and short numbered steps, grouping the nine hosts under three headings or expandable cards:

- **Browser apps:** Claude (web/Desktop), ChatGPT.
- **Command line:** Claude Code, Codex CLI, Gemini CLI, OpenCode.
- **Editors:** Cursor, VS Code with Copilot, Windsurf/Cascade.

Use OAuth as the first path for Claude, ChatGPT, and Gemini CLI; show PAT instructions only for clients that can keep a bearer secret out of shared config. OpenCode deserves inclusion as an extra: the official repository showed 208.3k stars when checked on 2026-10-06. VS Code/Copilot is supported by GitHub's October 2025 statement of 20M+ Copilot developers across IDEs, CLI and pull requests. Windsurf's official July 2026 post says it had millions of users, but its MCP docs now redirect to legacy Cascade instructions; label the card **Windsurf / Cascade (legacy docs)** or omit it until the live app labels are confirmed. Those adoption figures are vendor-reported, not MCP usage counts.

Today's `apps/web/src/Agents.tsx` copy needs corrections:

- Claude is not OAuth-only: the current Claude docs list fixed request headers in limited beta. The browser OAuth path is still the recommended default.
- ChatGPT cannot accept a static Forma PAT in its web connector UI, but Codex CLI supports both OAuth and bearer-token environment variables. Combining them under “ChatGPT and Codex” is misleading.
- Claude Code and Gemini CLI can open browser OAuth flows. “Command-line hosts cannot open a browser” is wrong.
- Cursor, VS Code/Copilot, Windsurf/Cascade, and OpenCode are missing from the cards. The PAT section should distinguish hosts with environment-variable-backed headers from Gemini CLI, whose current remote-header docs do not document env expansion.

**Adoption sources (read 2026-10-06):** [GitHub Copilot: 20M+ developers (2025-10-15)](https://github.blog/ai-and-ml/github-copilot/copilot-faster-smarter-and-built-for-how-you-work-now/); [Cognition: Windsurf had millions of users (2026-07-14)](https://cognition.com/blog/one-year-of-building-together); [OpenCode GitHub repository, 208.3k stars when checked](https://github.com/anomalyco/opencode).

## Owner decisions (2026-10-07)

1. The page has eight apps, in order: Claude, ChatGPT, Claude Code, Codex CLI, Gemini CLI, OpenCode, Cursor, and VS
   Code (GitHub Copilot). Windsurf is omitted.
2. “Pick your app” is one wrapping row of accessible tabs and shows one panel at a time, with Claude selected by
   default. Support click, arrow keys, Home/End, and visible keyboard focus; use Signal only for the selected state.
3. Browser OAuth is the main path wherever the app supports it. A PAT is a secondary “Use a token instead” disclosure
   only for apps that accept one; ChatGPT has no token path. Keep PATs in `FORMA_TOKEN` when the host documents
   environment references. For VS Code, use its documented password prompt; Gemini CLI's token header expands into
   and is saved in Gemini's config. Every panel has a short requirement, 2–4 numbered steps, the live MCP URL in
   snippets, Copy buttons, and an official vendor guide.
