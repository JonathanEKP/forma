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
  useEditor.setState({ selectedPageId: null, selectedLayerId: null, zoom: 1, panel: "properties", draft: null, notice: null });
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
    expect(screen.queryByRole("button", { name: "Agent" })).toBeNull();
    expect(backend.calls.some((call) => call.path === "/agent/context" || call.path === "/pons/v1/events")).toBe(false);
  });
});

describe("the chat of a window", () => {
  it("is closed until asked for, opens beside the canvas, and closes again", async () => {
    renderApp();
    await openDesign("Cover");
    expect(screen.queryByLabelText("Agent chat")).toBeNull();
    fireEvent.focus(await screen.findByRole("textbox", { name: "Ask the agent…" }));
    expect(screen.getByLabelText("Agent chat")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByLabelText("Agent chat")).toBeNull();
  });

  it("keeps the Calm composer in the canvas and removes the old Agent toggle", async () => {
    renderApp();
    await openDesign("Cover");
    expect(await screen.findByRole("textbox", { name: "Ask the agent…" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Agent" })).toBeNull();
  });

  it("keeps editor chrome off the projects view until a design is open", async () => {
    renderApp();
    expect(await screen.findByText("Launch campaign")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^(Undo|Redo|Export)$/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Valid|issues/ })).toBeNull();
  });

  it("opens one rail flyout at a time, closes it from its icon or Escape", async () => {
    renderApp();
    await openDesign("Cover");

    const layers = screen.getByRole("button", { name: "Layers & pages" });
    const assets = screen.getByRole("button", { name: "Assets" });
    expect(layers.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("complementary", { name: "Layers & pages" })).toBeTruthy();
    fireEvent.click(layers);
    expect(screen.queryByRole("complementary", { name: "Layers & pages" })).toBeNull();
    expect(layers.getAttribute("aria-pressed")).toBe("false");

    fireEvent.click(layers);
    expect(screen.getByRole("complementary", { name: "Layers & pages" })).toBeTruthy();
    expect(layers.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(assets);
    expect(screen.queryByRole("complementary", { name: "Layers & pages" })).toBeNull();
    expect(screen.getByRole("complementary", { name: "Assets" })).toBeTruthy();
    expect(assets.getAttribute("aria-pressed")).toBe("true");
    expect(layers.getAttribute("aria-pressed")).toBe("false");

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("complementary", { name: "Assets" })).toBeNull();
    expect(assets.getAttribute("aria-pressed")).toBe("false");

    fireEvent.click(assets);
    fireEvent.click(screen.getByRole("button", { name: "Close Assets" }));
    expect(screen.queryByRole("complementary", { name: "Assets" })).toBeNull();

    fireEvent.click(assets);
    fireEvent.click(assets);
    expect(screen.queryByRole("complementary", { name: "Assets" })).toBeNull();
  });

  it("opens validation as a popover without modal markup", async () => {
    renderApp();
    await openDesign("Cover");
    fireEvent.click(screen.getByRole("button", { name: /Open validation:/ }));

    expect(screen.getByRole("region", { name: "Validation" })).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.querySelector('[aria-modal="true"]')).toBeNull();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("region", { name: "Validation" })).toBeNull();
  });

  it("shows Properties only while a layer is selected", async () => {
    renderApp();
    await openDesign("Cover");
    expect(screen.queryByRole("complementary", { name: "Properties" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /Headline/ }));
    expect(screen.getByRole("complementary", { name: "Properties" })).toBeTruthy();
    expect(within(screen.getByRole("complementary", { name: "Properties" })).getByText(/POSITION/)).toBeTruthy();

    act(() => useEditor.getState().selectLayer(null));
    expect(screen.queryByRole("complementary", { name: "Properties" })).toBeNull();
  });

  it("shows the actual zoom percentage and keeps it through an agent write", async () => {
    renderApp();
    await openDesign("Cover");

    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByLabelText("Canvas zoom 125 percent")).toBeTruthy();
    await waitFor(() => expect(backend.calls.some((call) => call.path === "/pons/v1/events")).toBe(true));
    backend.designs.d1.version = 2;
    backend.push(1, "session.created");
    backend.push(2, "capability.completed", {
      invocationId: "i2",
      capabilityId: "design.applyOperations",
      result: { design_id: "d1", project_id: "p1", version: 2, updated_at: "now" },
    });
    expect(await screen.findByText("The agent updated the design (v2)", {}, { timeout: 4_000 })).toBeTruthy();
    expect(screen.getByLabelText("Canvas zoom 125 percent")).toBeTruthy();
  });

  it("fits the artboard to the available canvas once when a design opens", async () => {
    const originalWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    const originalHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get() {
        return this.classList.contains("editor-canvas-room") ? 800 : originalWidth?.get?.call(this) ?? 0;
      },
    });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", {
      configurable: true,
      get() {
        return this.classList.contains("editor-canvas-room") ? 700 : originalHeight?.get?.call(this) ?? 0;
      },
    });

    try {
      renderApp();
      await openDesign("Cover");
      expect(screen.getByLabelText("Canvas zoom 51 percent")).toBeTruthy();
    } finally {
      if (originalWidth) Object.defineProperty(HTMLElement.prototype, "clientWidth", originalWidth);
      else Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
      if (originalHeight) Object.defineProperty(HTMLElement.prototype, "clientHeight", originalHeight);
      else Reflect.deleteProperty(HTMLElement.prototype, "clientHeight");
    }
  });

  it("expands the floating composer into the dock and focuses its composer", async () => {
    renderApp();
    await openDesign("Cover");
    const collapsed = screen.getByRole("textbox", { name: "Ask the agent…" });
    fireEvent.focus(collapsed);

    expect(await screen.findByLabelText("Agent chat")).toBeTruthy();
    const composer = screen.getByRole("textbox", { name: "Message the agent" });
    await waitFor(() => expect(document.activeElement).toBe(composer));
  });

  it("outlives the design it began in: back to the projects and into another design, the conversation is still there", async () => {
    renderApp();
    await openDesign("Cover");
    fireEvent.focus(await screen.findByRole("textbox", { name: "Ask the agent…" }));
    await screen.findByText(/Studio laptop/);
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "Make the headline bigger" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents()).toEqual([{ type: "session.start", payload: { providerId: "codex" } }]));
    const panel = screen.getByLabelText("Agent chat");
    expect(within(within(panel).getByRole("log")).getByText("Make the headline bigger")).toBeTruthy();

    // Back to the project list: the panel is still open and still holds the conversation.
    fireEvent.click(screen.getByRole("button", { name: "Open Projects" }));
    await screen.findByText("Launch campaign");
    expect(within(within(screen.getByLabelText("Agent chat")).getByRole("log")).getByText("Make the headline bigger")).toBeTruthy();

    // And into another design.
    fireEvent.click(screen.getByText("Launch campaign"));
    fireEvent.click(await screen.findByText("Square post"));
    await screen.findByText("Square post", { selector: "span" });
    expect(within(within(screen.getByLabelText("Agent chat")).getByRole("log")).getByText("Make the headline bigger")).toBeTruthy();
    // One session throughout: moving between designs started nothing.
    expect(backend.intents().filter((intent) => intent.type === "session.start")).toHaveLength(1);
  });

  it("sends where the person is with each message, and shows the context as a chip", async () => {
    renderApp();
    await openDesign("Cover");
    await waitFor(() => expect(useEditor.getState().selectedPageId).toBe("pg1"));
    act(() => useEditor.getState().selectLayer("ly1"));
    fireEvent.focus(await screen.findByRole("textbox", { name: "Ask the agent…" }));
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
    fireEvent.focus(await screen.findByRole("textbox", { name: "Ask the agent…" }));
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
    fireEvent.focus(await screen.findByRole("textbox", { name: "Ask the agent…" }));
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
    fireEvent.click(screen.getByRole("button", { name: "Open Projects" }));
    fireEvent.click(await screen.findByText("Launch campaign"));
    expect(await screen.findByText(/v2 · 1 page/)).toBeTruthy();
  });
});
