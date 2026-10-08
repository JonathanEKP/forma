import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
    const otherApp = screen.getByRole("tab", { name: "Other app" });
    fireEvent.click(chatgpt);
    expect(chatgpt.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel", { name: "ChatGPT" })).toBeTruthy();
    expect(screen.queryByRole("tabpanel", { name: "Claude" })).toBeNull();

    fireEvent.keyDown(chatgpt, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Claude Code" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(screen.getByRole("tab", { name: "Claude Code" }), { key: "Home" });
    expect(claude.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(claude, { key: "End" });
    expect(otherApp.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(otherApp, { key: "ArrowRight" });
    expect(claude.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(claude, { key: "ArrowLeft" });
    expect(otherApp.getAttribute("aria-selected")).toBe("true");
  });

  it("uses the live MCP URL in every app snippet and copies snippets", async () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const liveUrl = `${window.location.origin}/mcp`;
    for (const tab of screen.getAllByRole("tab")) {
      fireEvent.click(tab);
      const snippets = screen.getByRole("tabpanel").querySelectorAll("pre");
      expect(snippets.length).toBeGreaterThan(0);
      expect(Array.from(snippets).some((snippet) => snippet.textContent?.includes(liveUrl))).toBe(true);
    }

    fireEvent.click(screen.getByRole("tab", { name: "Claude Code" }));
    const snippet = screen.getByLabelText("Claude Code registration command");
    expect(snippet.textContent).toContain(liveUrl);
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    fireEvent.click(screen.getByRole("button", { name: "Copy Claude Code registration command" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(snippet.textContent));
  });

  it("shows plan requirements only when the person needs to know them", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const panel = screen.getByRole("tabpanel");
    const tabsWithoutRequirements = ["Claude Code", "Codex CLI", "OpenCode", "Cursor", "Antigravity", "Other app"];
    for (const name of tabsWithoutRequirements) {
      fireEvent.click(screen.getByRole("tab", { name }));
      expect(panel.querySelector(".mcp-host-requirement")).toBeNull();
    }

    fireEvent.click(screen.getByRole("tab", { name: "Claude" }));
    expect(panel.querySelector(":scope > p")?.textContent).toContain("Free allows one connector");
    fireEvent.click(screen.getByRole("tab", { name: "ChatGPT" }));
    expect(panel.querySelector(":scope > p")?.textContent).toBe(
      "Editing needs a Business, Enterprise or Edu plan; on Pro, Forma is read-only. A workspace admin publishes the app.",
    );
    fireEvent.click(screen.getByRole("tab", { name: "VS Code (GitHub Copilot)" }));
    expect(panel.querySelector(":scope > p")?.textContent).toContain("VS Code 1.99+");
  });

  it("uses plain sign-in copy and meaningful sample labels and names", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const labels: ReadonlyArray<readonly [string, string, string]> = [
      ["Claude Code", "Commands", "Claude Code registration command"],
      ["Codex CLI", "Commands", "Codex CLI registration command"],
      ["Antigravity", "mcp_config.json", "Antigravity MCP config"],
      ["OpenCode", "opencode.json", "OpenCode project config"],
      ["OpenCode", "Commands", "OpenCode sign-in command"],
      ["Cursor", ".cursor/mcp.json", "Cursor project config"],
    ];
    for (const [hostName, sampleLabel, accessibleName] of labels) {
      fireEvent.click(screen.getByRole("tab", { name: hostName }));
      const panel = screen.getByRole("tabpanel");
      expect(within(panel).getAllByText(sampleLabel).length).toBeGreaterThan(0);
      expect(panel.querySelector(`pre[aria-label="${accessibleName}"]`)).toBeTruthy();
      expect(screen.getByRole("button", { name: `Copy ${accessibleName}` })).toBeTruthy();
      expect(panel.textContent).toMatch(/browser/i);
    }

    for (const hostName of ["Claude", "ChatGPT", "Claude Code", "Codex CLI", "OpenCode", "Cursor", "VS Code (GitHub Copilot)", "Antigravity"]) {
      fireEvent.click(screen.getByRole("tab", { name: hostName }));
      expect(screen.getByRole("tabpanel").textContent).not.toContain("OAuth");
    }
  });

  it("does not offer a ChatGPT token path", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();
    fireEvent.click(screen.getByRole("tab", { name: "ChatGPT" }));

    expect(screen.queryByText("Use a token instead")).toBeNull();
    expect(screen.getByText("Editing needs a Business, Enterprise or Edu plan; on Pro, Forma is read-only. A workspace admin publishes the app.")).toBeTruthy();
    expect(screen.getByText("ChatGPT signs in in your browser and does not need a token.")).toBeTruthy();

    fireEvent.click(screen.getByRole("tab", { name: "Claude" }));
    expect(screen.queryByText("Use a token instead")).toBeNull();
    expect(screen.getByText("Claude signs in in your browser and does not need a token.")).toBeTruthy();
  });

  it("shows Antigravity browser sign-in first and a documented token header alternative", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();
    fireEvent.click(screen.getByRole("tab", { name: "Antigravity" }));
    expect(screen.getByRole("tabpanel").textContent).toMatch(/sign in in your browser/i);
    expect(screen.getByLabelText("Antigravity MCP config").textContent).toContain("serverUrl");
    expect(screen.getByRole("link", { name: /Official Antigravity setup guide/ }).getAttribute("href")).toBe("https://antigravity.google/docs/mcp");
    fireEvent.click(screen.getByText("Use a token instead"));

    expect(screen.getByText(/Google's docs show bearer tokens as config values; no environment reference is documented/i)).toBeTruthy();
    const tokenSteps = screen.getByText("Use a token instead").closest("details")?.querySelector("ol");
    expect(tokenSteps?.textContent).not.toMatch(/config file|saves? this token/i);
    expect(screen.getByLabelText("Antigravity token config").textContent).toContain("Bearer <token>");
    expect(screen.getByRole("link", { name: "Create a personal access token" }).getAttribute("href")).toBe("#token-form");
  });

  it("explains Other app with the live URL, Streamable HTTP, browser sign-in, and a placeholder header", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();
    fireEvent.click(screen.getByRole("tab", { name: "Other app" }));

    const panel = screen.getByRole("tabpanel", { name: "Other app" });
    const liveUrl = `${window.location.origin}/mcp`;
    expect(panel.textContent).toContain(liveUrl);
    expect(panel.textContent).toContain("Streamable HTTP");
    expect(panel.textContent).toMatch(/browser/i);
    expect(panel.textContent).toContain("Authorization: Bearer");
    expect(panel.textContent).toContain("serverUrl");
    expect(panel.textContent).toContain('"type": "http"');
    expect(panel.textContent).not.toMatch(/forma_pat_[a-zA-Z0-9]{16,}/);
    expect(screen.getByLabelText("Other app MCP config").textContent).toContain(liveUrl);
    expect(screen.getByLabelText("Other app MCP config").textContent).toContain("${FORMA_TOKEN}");
  });

  it("renders no token value in MCP setup guidance for any app", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    for (const tab of screen.getAllByRole("tab")) {
      fireEvent.click(tab);
      const tokenDisclosure = screen.queryByText("Use a token instead");
      if (tokenDisclosure) fireEvent.click(tokenDisclosure);
      expect(document.body.textContent).not.toMatch(/forma_pat_[a-zA-Z0-9]{16,}/);
      if (tab.textContent?.trim() !== "Other app") {
        expect(screen.getByRole("link", { name: /^Official .* setup guide, opens in a new tab$/ })).toBeTruthy();
      }
    }
  });

  it("lists apps in group order, with Other app last", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent);
    expect(tabs).toEqual([
      "Claude",
      "ChatGPT",
      "Claude Code",
      "Codex CLI",
      "OpenCode",
      "Cursor",
      "VS Code (GitHub Copilot)",
      "Antigravity",
      "Other app",
    ]);
  });

  it("groups the single tablist and moves the keyboard across group boundaries", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const tablist = screen.getByRole("tablist");
    expect(screen.getAllByRole("tablist")).toHaveLength(1);
    expect(tablist.getAttribute("aria-orientation")).toBe("vertical");
    const browser = within(screen.getByRole("heading", { name: "In your browser" }).parentElement as HTMLElement);
    const terminal = within(screen.getByRole("heading", { name: "In your terminal" }).parentElement as HTMLElement);
    const editor = within(screen.getByRole("heading", { name: "In your editor" }).parentElement as HTMLElement);
    expect(browser.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Claude", "ChatGPT"]);
    expect(terminal.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Claude Code", "Codex CLI", "OpenCode"]);
    expect(editor.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Cursor", "VS Code (GitHub Copilot)", "Antigravity"]);
    expect(tablist.querySelectorAll('[role="tabpanel"]')).toHaveLength(0);

    const chatgpt = screen.getByRole("tab", { name: "ChatGPT" });
    fireEvent.keyDown(chatgpt, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Claude Code" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(screen.getByRole("tab", { name: "Claude Code" }), { key: "ArrowLeft" });
    expect(chatgpt.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(chatgpt, { key: "ArrowDown" });
    expect(screen.getByRole("tab", { name: "Claude Code" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(screen.getByRole("tab", { name: "Claude Code" }), { key: "ArrowUp" });
    expect(chatgpt.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(screen.getByRole("tab", { name: "Antigravity" }), { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Other app" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(screen.getByRole("tab", { name: "Other app" }), { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Claude" }).getAttribute("aria-selected")).toBe("true");
  });

  it("shows each app mark as an aria-hidden inline SVG", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    for (const tab of screen.getAllByRole("tab")) {
      const mark = tab.querySelector("svg");
      expect(mark).not.toBeNull();
      expect(mark?.getAttribute("aria-hidden")).toBe("true");
      expect(mark?.getAttribute("viewBox")).toBeTruthy();
    }
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

    const urlSection = screen.getByRole("heading", { name: "Your MCP URL" }).closest("section");
    expect(urlSection?.querySelector(".mcp-section-heading")).toBeTruthy();
    expect(urlSection?.querySelector(".mcp-section-content")).toBeTruthy();
    const picker = screen.getByRole("heading", { name: "Pick your app" }).closest("section");
    expect(picker?.querySelector(".mcp-host-layout")).toBeTruthy();
    const tabList = picker?.querySelector(".mcp-host-tabs");
    expect(tabList && (tabList.compareDocumentPosition(screen.getByRole("tabpanel")) & Node.DOCUMENT_POSITION_FOLLOWING)).toBeTruthy();
  });

  it("keeps each numbered step's own code block and copy control", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    const panel = screen.getByRole("tabpanel", { name: "Claude" });
    const steps = panel.querySelectorAll(".mcp-steps > li");
    expect(steps).toHaveLength(4);
    expect(Array.from(steps).map((step) => step.querySelector(".mcp-step-number")?.textContent)).toEqual(["01", "02", "03", "04"]);

    const urlSample = screen.getByLabelText("Claude MCP URL");
    const step = urlSample.closest(".mcp-step");
    expect(step?.querySelector(".mcp-code-sample")).toBeTruthy();
    expect(within(step as HTMLElement).getByRole("button", { name: "Copy Claude MCP URL" })).toBeTruthy();
    expect(panel.querySelectorAll(":scope > .mcp-code-sample")).toHaveLength(0);
  });

  it("shows numbered token steps, links token creation, and opens the selected app's disclosure from the token section", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();
    fireEvent.click(screen.getByRole("tab", { name: "Cursor" }));
    fireEvent.click(screen.getByText("Use a token instead"));

    const disclosure = screen.getByText("Use a token instead").closest("details");
    const tokenSteps = disclosure?.querySelectorAll(".mcp-steps > li");
    expect(tokenSteps).toHaveLength(2);
    expect(disclosure?.querySelector(".mcp-steps code")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Create a personal access token" }).getAttribute("href")).toBe("#token-form");
    expect(screen.getByLabelText("Key name").closest("form")?.id).toBe("token-form");

    const next = screen.getByRole("link", { name: "Follow Cursor's token steps above" });
    expect(next.getAttribute("href")).toBe("#mcp-token-details");
    fireEvent.click(next);
    expect(screen.getByRole("tab", { name: "Cursor" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText("Use a token instead").closest("details")?.hasAttribute("open")).toBe(true);
    expect(document.activeElement).toBe(screen.getByText("Use a token instead"));
  });

  it("explains how to recover if a token leaks", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    expect(screen.getByText("If a token leaks, revoke it here and create a new one.")).toBeTruthy();
  });

  it("uses the portable VS Code token config with a password input", () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();
    fireEvent.click(screen.getByRole("tab", { name: "VS Code (GitHub Copilot)" }));
    fireEvent.click(screen.getByText("Use a token instead"));

    const config = JSON.parse(screen.getByLabelText("VS Code token config").textContent ?? "") as {
      mcpServers?: { forma?: { type?: string; url?: string; headers?: { Authorization?: string } } };
      inputs?: Array<{ id?: string; type?: string; description?: string; password?: boolean }>;
      servers?: unknown;
    };
    expect(config.servers).toBeUndefined();
    expect(config.mcpServers?.forma?.type).toBe("http");
    expect(config.mcpServers?.forma?.url).toBe(`${window.location.origin}/mcp`);
    expect(config.mcpServers?.forma?.headers?.Authorization).toBe("Bearer ${input:forma-token}");
    expect(config.inputs).toEqual([{
      id: "forma-token",
      type: "promptString",
      description: "Forma personal access token",
      password: true,
    }]);
  });

  it("shows token follow-up for the selected app after creating a token without leaking it into setup guidance", async () => {
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();
    fireEvent.click(screen.getByRole("tab", { name: "Claude Code" }));
    fireEvent.change(screen.getByLabelText("Key name"), { target: { value: "Claude Code" } });
    fireEvent.click(screen.getByRole("button", { name: "Create token" }));

    expect(await screen.findByText(TOKEN)).toBeTruthy();
    const follow = screen.getByRole("link", { name: "follow Claude Code's token steps ↑" });
    expect(follow.getAttribute("href")).toBe("#mcp-token-details");
    fireEvent.click(follow);
    expect(screen.getByRole("tab", { name: "Claude Code" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText("Use a token instead").closest("details")?.hasAttribute("open")).toBe(true);
    expect(screen.getByLabelText("Claude Code token config").textContent).not.toContain(TOKEN);
  });

  it("disconnects an authorized application inline", async () => {
    agentState.hosted = true;
    agentState.grants = [{ client: { id: "claude", name: "Claude" }, granted_at: "2026-10-04T12:00:00.000Z" }];
    window.history.replaceState({}, "", "/agents/mcp");
    renderAgents();

    expect(await screen.findByRole("article", { name: /^Claude, connected/ })).toBeTruthy();
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
