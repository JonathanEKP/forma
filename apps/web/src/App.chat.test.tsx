/**
 * The agent chat as the *window's*, not a design's (D-1), and the editor following
 * what the agent writes (D-7). Driven through `App` with `fetch` as the only seam,
 * because what is under test is how the pieces are wired: that a conversation
 * outlives the design it began in, that a message carries where the person was,
 * and that a write by the agent reaches the screen without a conflict first.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./identity", () => ({
  supabase: null,
  accessToken: () => Promise.resolve("session-token"),
  currentSession: () => Promise.resolve({}),
  currentSubject: () => Promise.resolve("subject"),
  onSessionChange: () => () => undefined,
  signOut: () => Promise.resolve(),
}));

import { App } from "./App";
import { windowId } from "./pontis/window";
import { useEditor } from "./store";

const CONTEXT = "22222222-2222-4222-8222-222222222222";
const SESSION = "ses_1";

const layer = (id: string, name: string) => ({
  type: "text",
  id,
  name,
  transform: { x: 0, y: 0, width: 100, height: 40, rotation: 0 },
  text: name,
  font_family: "Geist",
  font_size: 24,
  font_weight: 400,
  color: "#fff",
});

interface FakeDesign {
  id: string;
  name: string;
  version: number;
  layers: { id: string; name: string }[];
}

class Backend {
  calls: { path: string; method: string; body: unknown }[] = [];
  events: Record<string, unknown>[] = [];
  designs: Record<string, FakeDesign> = {
    d1: { id: "d1", name: "Cover", version: 1, layers: [{ id: "ly1", name: "Headline" }, { id: "ly2", name: "Tagline" }] },
    d2: { id: "d2", name: "Square post", version: 1, layers: [] },
  };

  record(id: string) {
    const design = this.designs[id];
    if (!design) return null;
    return {
      id,
      project_id: "p1",
      name: design.name,
      version: design.version,
      created_at: "2026-10-03T00:00:00Z",
      updated_at: "2026-10-03T00:00:00Z",
      document: {
        schema_version: 1,
        format: { preset: "square", width: 1080, height: 1080 },
        pages: [{ id: "pg1", name: "Page 1", layers: design.layers.map((l) => layer(l.id, l.name)) }],
      },
    };
  }

  push(sequence: number, type: string, payload: Record<string, unknown> = {}) {
    this.events.push({ sequence, type, sessionId: SESSION, deviceId: "dev_1", payload });
  }

  readonly fetch = vi.fn((input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://x");
    const method = init?.method ?? "GET";
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    this.calls.push({ path: url.pathname, method, body });
    const json = (value: unknown, status = 200) => Promise.resolve(new Response(JSON.stringify(value), { status }));
    const design = /^\/api\/v1\/designs\/([^/]+)$/u.exec(url.pathname)?.[1];

    if (design) return json(this.record(design) ?? { error: { code: "DESIGN_NOT_FOUND", message: "no" } }, this.designs[design] ? 200 : 404);
    switch (url.pathname) {
      case "/api/v1/identity":
        return json({ required: true });
      case "/api/v1/status":
        return json({ name: "forma-server", version: "0.1.0", schema_version: 1, database: "sqlite", fonts: ["Geist"], projects: 1, max_ops_per_batch: 20 });
      case "/api/v1/projects":
        return json([{ id: "p1", name: "Launch campaign", created_at: "2026-08-28T00:00:00.000Z", updated_at: "2026-08-28T00:00:00.000Z" }]);
      case "/api/v1/projects/p1/designs":
        return json(
          Object.values(this.designs).map((d) => ({ id: d.id, project_id: "p1", name: d.name, version: d.version, format: { preset: "square", width: 1080, height: 1080 }, page_count: 1, created_at: "x", updated_at: "x" })),
        );
      case "/api/v1/projects/p1/brand":
        return json(null);
      case "/agent/context":
        return json({ applicationContextId: CONTEXT });
      case "/pons/v1/devices":
        return json({
          devices: [
            {
              id: "dev_1",
              name: "Studio laptop",
              online: true,
              revokedAt: null,
              runtimeVersion: "0.1.0",
              capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: true, authMode: "subscription" }],
            },
          ],
        });
      case "/pons/v1/events": {
        const after = Number(url.searchParams.get("after"));
        return json({
          scope: { kind: "application_context", applicationContextId: CONTEXT },
          events: this.events.filter((event) => Number(event.sequence) > after),
          head: this.events.length,
        });
      }
      case "/agent/intent": {
        const type = typeof body === "object" && body !== null && "type" in body ? body.type : undefined;
        return json(type === "session.start" ? { delivered: true, sessionId: SESSION } : { delivered: true });
      }
      default:
        return json({ error: { code: "NOT_FOUND", message: url.pathname } }, 404);
    }
  });

  intents(): { type: string; payload: Record<string, unknown> }[] {
    return this.calls.filter((call) => call.path === "/agent/intent").map((call) => call.body as { type: string; payload: Record<string, unknown> });
  }
}

let backend: Backend;

function renderApp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

async function openDesign(name: string) {
  fireEvent.click(await screen.findByText("Launch campaign"));
  fireEvent.click(await screen.findByText(name));
  await screen.findByText(`v${backend.designs[name === "Cover" ? "d1" : "d2"]?.version ?? 1}`, { selector: "span" });
}

beforeEach(() => {
  sessionStorage.clear();
  useEditor.setState({ selectedPageId: null, selectedLayerId: null, draft: null, notice: null });
  backend = new Backend();
  vi.stubGlobal("fetch", backend.fetch);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("where there is no agent", () => {
  it("has no chat, no toggle and no request for one", async () => {
    backend.fetch.mockImplementation((input) => {
      const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      const body = raw.endsWith("/identity") ? { required: false } : raw.endsWith("/projects") ? [] : raw.endsWith("/status") ? { name: "forma-server", version: "0", schema_version: 1, database: "sqlite", fonts: [], projects: 0, max_ops_per_batch: 20 } : null;
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    });
    renderApp();
    await screen.findByText(/No projects yet/);
    expect(screen.queryByRole("button", { name: /Agent/ })).toBeNull();
    expect(backend.calls.some((call) => call.path === "/agent/context" || call.path === "/pons/v1/events")).toBe(false);
  });
});

describe("the chat of a window", () => {
  it("is closed until asked for, opens beside the canvas, and closes again", async () => {
    renderApp();
    await openDesign("Cover");
    expect(screen.queryByLabelText("Agent chat")).toBeNull();
    fireEvent.click(await screen.findByRole("button", { name: /Agent/ }));
    expect(screen.getByLabelText("Agent chat")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByLabelText("Agent chat")).toBeNull();
  });

  it("badges the toggle with an approval that arrives while the panel is shut", async () => {
    // A window whose chat has a session — it was reattached to it after a reload.
    sessionStorage.setItem(
      "forma.chat",
      JSON.stringify({ context: CONTEXT, window: await windowId(), remembered: { sessionId: SESSION, deviceId: null, dead: null, said: [] } }),
    );
    renderApp();
    await openDesign("Cover");
    await screen.findByRole("button", { name: /Agent/ });
    backend.push(1, "session.created");
    backend.push(2, "approval.requested", { approvalId: "apr_1", invocationId: "i", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit", importantArguments: [], affectedResourceCount: 1 });
    expect(await screen.findByLabelText("1 waiting for your approval", {}, { timeout: 4_000 })).toBeTruthy();
  });

  it("outlives the design it began in: back to the projects and into another design, the conversation is still there", async () => {
    renderApp();
    await openDesign("Cover");
    fireEvent.click(await screen.findByRole("button", { name: /Agent/ }));
    await screen.findByText(/Studio laptop/);
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "Make the headline bigger" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents()).toEqual([{ type: "session.start", payload: { providerId: "codex" } }]));
    const panel = screen.getByLabelText("Agent chat");
    expect(within(panel).getByText("Make the headline bigger")).toBeTruthy();

    // Back to the project list: the panel is still open and still holds the conversation.
    fireEvent.click(screen.getByRole("button", { name: "← Projects" }));
    await screen.findByText("Launch campaign");
    expect(within(screen.getByLabelText("Agent chat")).getByText("Make the headline bigger")).toBeTruthy();

    // And into another design.
    fireEvent.click(screen.getByText("Launch campaign"));
    fireEvent.click(await screen.findByText("Square post"));
    await screen.findByText("Square post", { selector: "span" });
    expect(within(screen.getByLabelText("Agent chat")).getByText("Make the headline bigger")).toBeTruthy();
    // One session throughout: moving between designs started nothing.
    expect(backend.intents().filter((intent) => intent.type === "session.start")).toHaveLength(1);
  });

  it("sends where the person is with each message, and shows the context as a chip", async () => {
    renderApp();
    await openDesign("Cover");
    await waitFor(() => expect(useEditor.getState().selectedPageId).toBe("pg1"));
    act(() => useEditor.getState().selectLayer("ly1"));
    fireEvent.click(await screen.findByRole("button", { name: /Agent/ }));
    await screen.findByText(/Studio laptop/);

    backend.push(1, "session.created");
    backend.push(2, "session.started");
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "make this bigger" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents().map((intent) => intent.type)).toEqual(["session.start", "session.input"]), { timeout: 4_000 });

    expect(backend.intents()[1]).toEqual({
      type: "session.input",
      payload: { sessionId: SESSION, text: "make this bigger", view: { designId: "d1", projectId: "p1", pageId: "pg1", layerId: "ly1" } },
    });
    // The person sees what they typed, and a chip naming the context that went with it.
    const panel = screen.getByLabelText("Agent chat");
    expect(within(panel).getByText("make this bigger")).toBeTruthy();
    expect(within(panel).getByLabelText("Sent with: Cover, Page 1, Headline")).toBeTruthy();
  });

  it("re-attaches to its own session after a reload, with what the person said and the chip they said it with", async () => {
    const first = renderApp();
    await openDesign("Cover");
    fireEvent.click(await screen.findByRole("button", { name: /Agent/ }));
    await screen.findByText(/Studio laptop/);
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "hello there" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents()).toHaveLength(1));
    backend.push(1, "session.created");
    backend.push(2, "session.started");
    await waitFor(() => expect(backend.intents().map((intent) => intent.type)).toEqual(["session.start", "session.input"]), { timeout: 4_000 });
    backend.push(3, "turn.started");
    backend.push(4, "agent.output.completed", { turnId: "t", text: "Hi." });
    backend.push(5, "turn.completed");
    await screen.findByText("Hi.", {}, { timeout: 4_000 });

    // A reload: the page is new, the tab's storage is not, and the history comes back from the page.
    first.unmount();
    renderApp();
    await openDesign("Cover");
    fireEvent.click(await screen.findByRole("button", { name: /Agent/ }));
    await screen.findByText("Hi.", {}, { timeout: 4_000 });
    const lines = screen.getAllByRole("listitem").map((item) => item.getAttribute("data-kind"));
    expect(lines.slice(0, 2)).toEqual(["person", "agent"]);
    expect(within(screen.getByLabelText("Agent chat")).getByText("hello there")).toBeTruthy();
    expect(within(screen.getByLabelText("Agent chat")).getByLabelText(/Sent with: Cover/)).toBeTruthy();
    // It went back to its own session and started no other.
    expect(backend.intents().filter((intent) => intent.type === "session.start")).toHaveLength(1);
  });
});

describe("when the agent writes to the open design", () => {
  const written = (version: number, sequence = 3) =>
    backend.push(sequence, "capability.completed", {
      invocationId: `i${sequence}`,
      capabilityId: "design.applyOperations",
      result: { design_id: "d1", project_id: "p1", version, updated_at: "now" },
    });

  async function withChat() {
    renderApp();
    await openDesign("Cover");
    await waitFor(() => expect(useEditor.getState().selectedPageId).toBe("pg1"));
    // The chat has to be reading for the editor to hear of a write.
    await waitFor(() => expect(backend.calls.some((call) => call.path === "/pons/v1/events")).toBe(true));
    backend.push(1, "session.created");
    backend.push(2, "session.started");
  }

  it("refetches the design, says so, and is never first told by a conflict", async () => {
    await withChat();
    backend.designs.d1.version = 2;
    written(2);
    expect(await screen.findByText("The agent updated the design (v2)", {}, { timeout: 4_000 })).toBeTruthy();
    expect(screen.getByText("v2", { selector: "span" })).toBeTruthy();
    expect(screen.queryByText(/changed elsewhere/)).toBeNull();
  });

  it("keeps the selected layer if it still exists, and drops it if the agent removed it", async () => {
    await withChat();
    act(() => useEditor.getState().selectLayer("ly1"));
    backend.designs.d1.version = 2;
    written(2);
    await screen.findByText("The agent updated the design (v2)", {}, { timeout: 4_000 });
    expect(useEditor.getState().selectedLayerId).toBe("ly1");

    backend.designs.d1.version = 3;
    backend.designs.d1.layers = [{ id: "ly2", name: "Tagline" }];
    written(3, 4);
    await screen.findByText("The agent updated the design (v3)", {}, { timeout: 4_000 });
    expect(useEditor.getState().selectedLayerId).toBeNull();
  });

  it("drops a drag that was in progress, because it began on geometry the agent has since changed", async () => {
    await withChat();
    act(() => useEditor.getState().setDraft({ layerId: "ly1", x: 5, y: 5, width: 100, height: 40 }));
    backend.designs.d1.version = 2;
    written(2);
    await screen.findByText("The agent updated the design (v2)", {}, { timeout: 4_000 });
    expect(useEditor.getState().draft).toBeNull();
  });

  it("ignores a write whose version the window already holds, as a replay of the history brings back", async () => {
    await withChat();
    written(1);
    // Nothing newer than the v1 on screen: no refetch of the design beyond the first, and no notice.
    await act(() => new Promise((resolve) => setTimeout(resolve, 300)));
    expect(screen.queryByText(/The agent updated the design/)).toBeNull();
    expect(backend.calls.filter((call) => call.path === "/api/v1/designs/d1")).toHaveLength(1);
  });

  it("refreshes the design list too, so the project view shows the new version", async () => {
    await withChat();
    backend.designs.d1.version = 2;
    written(2);
    await screen.findByText("The agent updated the design (v2)", {}, { timeout: 4_000 });
    // The list was read when the project was opened, at v1; it must not be served as it was.
    fireEvent.click(screen.getByRole("button", { name: "← Projects" }));
    fireEvent.click(await screen.findByText("Launch campaign"));
    expect(await screen.findByText(/v2 · 1 page/)).toBeTruthy();
  });
});
