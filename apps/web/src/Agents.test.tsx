import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

const agentState = vi.hoisted(() => ({
  hosted: false,
  grants: [] as Array<{ client: { id: string; name: string }; granted_at: string }>,
  disconnected: [] as string[],
}));

/**
 * A Forma with no identity provider, which is what a local build is and what CI
 * builds — `apps/web/.env` is gitignored, so `VITE_SUPABASE_*` is absent there.
 *
 * Declared rather than inherited. Without this the file's behaviour depends on
 * whether the machine running it happens to have that file, which is how a test
 * passes for one person and fails for everybody else.
 */
vi.mock("./identity", () => ({
  get supabase() {
    return agentState.hosted ? {} : null;
  },
  accessToken: () => Promise.resolve(null),
  currentSubject: () => Promise.resolve(null),
  onSessionChange: () => () => undefined,
  currentSession: () => Promise.resolve(agentState.hosted ? { access_token: "session" } : null),
  sendMagicLink: () => Promise.resolve(),
  signOut: () => Promise.resolve(),
  listGrants: () => Promise.resolve(agentState.grants),
  revokeGrant: (clientId: string) => {
    agentState.disconnected.push(clientId);
    agentState.grants = agentState.grants.filter((grant) => grant.client.id !== clientId);
    return Promise.resolve();
  },
  authorizationDetails: () => Promise.reject(new Error("no provider")),
  decide: () => Promise.reject(new Error("no provider")),
}));


const TOKEN = `forma_pat_${"a".repeat(64)}`;
const KEY = {
  id: "11111111-1111-4111-8111-111111111111",
  account_id: "00000000-0000-4000-8000-000000000001",
  name: "Codex CLI",
  token_prefix: "forma_pat_aaaaaaaa",
  created_at: "2026-08-30T12:00:00.000Z",
};

function renderAgents() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe("agent credentials", () => {
  let listed: Array<typeof KEY & { revoked_at?: string }>;

  beforeEach(() => {
    listed = [];
    agentState.hosted = false;
    agentState.grants = [];
    agentState.disconnected = [];
    window.history.replaceState({}, "", "/agents/mcp");

    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const url =
          typeof input === "string"
            ? input
            : input instanceof URL
              ? input.href
              : input.url;
        const method = init?.method ?? "GET";

        if (url.endsWith("/api/v1/identity")) {
          return Promise.resolve(
            new Response(JSON.stringify({ required: agentState.hosted }), {
              status: 200,
              headers: { "content-type": "application/json" },
            }),
          );
        }

        if (url.endsWith("/api/v1/projects")) {
          return Promise.resolve(new Response("[]", { status: 200, headers: { "content-type": "application/json" } }));
        }

        if (!url.includes("/api/v1/api-keys")) {
          return Promise.resolve(
            new Response(
              JSON.stringify({ error: { code: "NOT_FOUND", message: url } }),
              { status: 404, headers: { "content-type": "application/json" } },
            ),
          );
        }

        if (method === "POST") {
          listed = [KEY];
          return Promise.resolve(
            new Response(JSON.stringify({ key: KEY, token: TOKEN }), {
              status: 201,
              headers: { "content-type": "application/json" },
            }),
          );
        }

        if (method === "DELETE") {
          listed = [{ ...KEY, revoked_at: "2026-08-30T12:01:00.000Z" }];
          return Promise.resolve(new Response(null, { status: 204 }));
        }

        return Promise.resolve(
          new Response(JSON.stringify(listed), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
        );
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    window.history.replaceState({}, "", "/");
  });

  it("shows a new token once, copies it, and can revoke its safe list entry", async () => {
    renderAgents();

    const oauth = screen.getByRole("heading", { name: "Use Forma from your AI app" });
    const pats = screen.getByRole("heading", { name: "Personal access tokens" });
    expect(oauth.compareDocumentPosition(pats) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    expect(await screen.findByText(/No personal access tokens yet/)).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Key name"), {
      target: { value: "Codex CLI" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create token" }));

    expect(await screen.findByText(TOKEN)).toBeTruthy();
    expect(screen.getAllByText(TOKEN)).toHaveLength(1);
    expect(screen.getByText(/will not appear in the list again/i)).toBeTruthy();
    expect(await screen.findByText(/^forma_pat_aaaaaaaa… · created/)).toBeTruthy();

    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    fireEvent.click(screen.getByRole("button", { name: "Copy newly created token" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(TOKEN));
    fireEvent.click(screen.getByRole("button", { name: "Dismiss the full token for Codex CLI after saving it" }));
    expect(screen.queryByText(TOKEN)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Revoke Codex CLI" }));
    const keep = screen.getByRole("button", { name: "Keep Codex CLI, created 30 Aug" });
    expect(document.activeElement).toBe(keep);
    fireEvent.keyDown(keep, { key: "Escape" });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Revoke Codex CLI" }));
    expect(screen.queryByRole("button", { name: "Confirm revoke Codex CLI, created 30 Aug" })).toBeNull();
    expect(screen.queryByText("Revoked")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Revoke Codex CLI" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm revoke Codex CLI, created 30 Aug" }));
    await waitFor(() => expect(screen.getByText("Revoked")).toBeTruthy());
  });

  it("routes between the two connection pages and marks the current route", () => {
    window.history.replaceState({}, "", "/agents");
    const chat = renderAgents();

    expect(screen.getByRole("heading", { name: "Chat in the editor" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Chat in the editor" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("link", { name: "AI apps (MCP)" }).getAttribute("href")).toBe("/agents/mcp");

    chat.unmount();
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    expect(screen.getByRole("heading", { name: "Use Forma from your AI app" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "AI apps (MCP)" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("link", { name: "Chat in the editor" }).getAttribute("href")).toBe("/agents");
  });

  it("explains MCP and defaults to the Claude setup tab", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    expect(screen.getByText("MCP is a standard way for an AI app to use tools in another app.")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Your MCP URL" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Pick your app" })).toBeTruthy();
    expect(screen.getByRole("tablist", { name: "Pick your app" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Claude" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel", { name: "Claude" })).toBeTruthy();
    expect(screen.getByText(/Free allows one connector/i)).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Devices" })).toBeNull();
  });

  it("switches the app setup with clicks and wrapping tab keys", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const claude = screen.getByRole("tab", { name: "Claude" });
    const chatgpt = screen.getByRole("tab", { name: "ChatGPT" });
    const vscode = screen.getByRole("tab", { name: "VS Code (GitHub Copilot)" });
    fireEvent.click(chatgpt);
    expect(chatgpt.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel", { name: "ChatGPT" })).toBeTruthy();
    expect(screen.queryByRole("tabpanel", { name: "Claude" })).toBeNull();

    fireEvent.keyDown(chatgpt, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Claude Code" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(screen.getByRole("tab", { name: "Claude Code" }), { key: "Home" });
    expect(claude.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(claude, { key: "End" });
    expect(vscode.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(vscode, { key: "ArrowRight" });
    expect(claude.getAttribute("aria-selected")).toBe("true");
  });

  it("uses the live MCP URL in the selected app snippets and copies snippets", async () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    fireEvent.click(screen.getByRole("tab", { name: "Claude Code" }));
    const liveUrl = `${window.location.origin}/mcp`;
    const snippet = screen.getByLabelText("Claude Code OAuth setup command");
    expect(snippet.textContent).toContain(liveUrl);
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    fireEvent.click(screen.getByRole("button", { name: "Copy Claude Code OAuth setup command" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(snippet.textContent));
  });

  it("does not offer a ChatGPT token path", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();
    fireEvent.click(screen.getByRole("tab", { name: "ChatGPT" }));

    expect(screen.queryByText("Use a token instead")).toBeNull();
    expect(screen.getByText(/Pro is read only/i)).toBeTruthy();
    expect(screen.getByText(/admin must publish/i)).toBeTruthy();
  });

  it("warns that Gemini CLI saves an expanded token in its config", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();
    fireEvent.click(screen.getByRole("tab", { name: "Gemini CLI" }));
    fireEvent.click(screen.getByText("Use a token instead"));

    expect(screen.getByText(/saves the bearer token in Gemini's config/i)).toBeTruthy();
    expect(screen.getByText(/does not document an environment-variable reference/i)).toBeTruthy();
    expect(screen.getByLabelText("Gemini CLI token setup command").textContent).toContain("$FORMA_TOKEN");
  });

  it("renders no token value in MCP setup guidance", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    expect(document.body.textContent).not.toMatch(/forma_pat_[a-zA-Z0-9]{16,}/);
    expect(screen.getAllByRole("link", { name: "Official guide" }).length).toBeGreaterThan(0);
  });

  it("lists the eight supported apps in the decided order", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toEqual([
      "Claude",
      "ChatGPT",
      "Claude Code",
      "Codex CLI",
      "Gemini CLI",
      "OpenCode",
      "Cursor",
      "VS Code (GitHub Copilot)",
    ]);
  });

  it("uses the wider MCP column, divided sections, and bordered URL row", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const page = screen.getByRole("main");
    expect(page.classList.contains("mcp-page")).toBe(true);
    for (const title of ["Your MCP URL", "Pick your app", "Personal access tokens"]) {
      const section = screen.getByRole("heading", { name: title }).closest("section");
      expect(section?.classList.contains("mcp-section")).toBe(true);
      expect(section?.classList.contains("border")).toBe(false);
    }

    const copy = screen.getByRole("button", { name: "Copy your MCP URL" });
    expect(copy.parentElement?.classList.contains("mcp-url-row")).toBe(true);
  });

  it("disconnects an authorized application inline", async () => {
    agentState.hosted = true;
    agentState.grants = [{ client: { id: "claude", name: "Claude" }, granted_at: "2026-10-04T12:00:00.000Z" }];
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    expect(await screen.findByText("Claude")).toBeTruthy();
    const disconnect = screen.getByRole("button", { name: "Disconnect Claude" });
    fireEvent.click(disconnect);
    const keep = screen.getByRole("button", { name: "Keep Claude, connected 4 Oct" });
    expect(document.activeElement).toBe(keep);
    fireEvent.keyDown(keep, { key: "Escape" });
    expect(document.activeElement).toBe(disconnect);
    expect(agentState.disconnected).toEqual([]);

    fireEvent.click(disconnect);
    fireEvent.click(screen.getByRole("button", { name: "Confirm disconnect Claude, connected 4 Oct" }));

    await waitFor(() => expect(agentState.disconnected).toEqual(["claude"]));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
