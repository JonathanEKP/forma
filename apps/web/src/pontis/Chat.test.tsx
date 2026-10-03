/**
 * The chat as a person meets it, with `fetch` as the only seam: the sidecar's
 * three browser routes and the event page are the things it shares with the real
 * thing, so those are what is faked.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../identity", () => ({
  supabase: null,
  accessToken: () => Promise.resolve("session-token"),
  currentSession: () => Promise.resolve(null),
}));

import { Editor } from "../editor/Editor";
import { liveFeed } from "./channel";
import { ChatPanel, ChatToggle } from "./Chat";
import { useChat } from "./useChat";

const CONTEXT = "22222222-2222-4222-8222-222222222222";
const SESSION = "ses_1";

interface Call {
  readonly path: string;
  readonly search: string;
  readonly method: string;
  readonly body: unknown;
}

/** The sidecar and the Control Plane, as far as the chat can tell. */
class Backend {
  calls: Call[] = [];
  events: Record<string, unknown>[] = [];
  down = false;
  devices: unknown = {
    devices: [
      {
        id: "dev_1",
        name: "Studio laptop",
        online: true,
        revokedAt: null,
        runtimeVersion: "0.1.0",
        capabilities: [
          {
            providerId: "codex",
            installed: true,
            compatible: true,
            authenticated: true,
            authMode: "subscription",
            securityProfile: {
              classification: "explicit_opt_in",
              executionBoundary: "supervised_child_process",
              environmentBoundary: "allowlisted",
              sandboxBoundary: "provider_managed",
            },
          },
        ],
      },
    ],
  };

  push(sequence: number, type: string, payload: Record<string, unknown> = {}) {
    this.events.push({ sequence, type, sessionId: SESSION, payload });
  }

  readonly fetch = vi.fn((input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://x");
    const method = init?.method ?? "GET";
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    this.calls.push({ path: url.pathname, search: url.search, method, body });
    const json = (value: unknown, status = 200) => Promise.resolve(new Response(JSON.stringify(value), { status }));
    if (this.down) return Promise.reject(new TypeError("fetch failed"));

    switch (url.pathname) {
      case "/agent/context":
        return json({ applicationContextId: CONTEXT });
      case "/pons/v1/devices":
        return json(this.devices);
      case "/pons/v1/events": {
        const after = Number(url.searchParams.get("after"));
        return json({
          scope: { kind: "application_context", applicationContextId: CONTEXT },
          events: this.events.filter((event) => Number(event.sequence) > after),
          head: this.events.length,
        });
      }
      case "/agent/intent": {
        const type = isRecord(body) ? body.type : undefined;
        return json(type === "session.start" ? { delivered: true, sessionId: SESSION } : { delivered: true });
      }
      default:
        return json({ error: { code: "NOT_FOUND" } }, 404);
    }
  });

  intents(): unknown[] {
    return this.calls.filter((call) => call.path === "/agent/intent").map((call) => call.body);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function Harness({ enabled }: { enabled: boolean }) {
  const chat = useChat({ enabled, pollMs: 20 });
  return (
    <>
      <ChatToggle chat={chat} open onToggle={() => undefined} />
      <ChatPanel chat={chat} onClose={() => undefined} />
    </>
  );
}

let backend: Backend;

beforeEach(() => {
  backend = new Backend();
  vi.stubGlobal("fetch", backend.fetch);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("when access is not ready", () => {
  it("renders nothing and asks the network for nothing", async () => {
    const { container } = render(<Harness enabled={false} />);
    await act(() => Promise.resolve());
    expect(container.innerHTML).toBe("");
    expect(backend.fetch).not.toHaveBeenCalled();
  });
});

describe("the status block", () => {
  it("names the device, the provider and its auth mode, and the sandbox exactly as the profile says", async () => {
    render(<Harness enabled />);
    const status = await screen.findByRole("region", { name: "Status" });
    await waitFor(() => expect(within(status).getByText(/Studio laptop/)).toBeTruthy());
    expect(within(status).getByText(/codex/)).toBeTruthy();
    expect(within(status).getByText(/subscription/)).toBeTruthy();
    // Verbatim, never "secure" and never "enforced": ADR-019.
    expect(within(status).getByText("sandbox: provider_managed")).toBeTruthy();
    expect(within(status).getByText("explicit_opt_in")).toBeTruthy();
    expect(status.textContent).not.toMatch(/secure|enforced/i);
  });

  it("warns that an API key is a different bill", async () => {
    backend.devices = { devices: [{ id: "d", name: "Box", online: true, revokedAt: null, runtimeVersion: "1", capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: true, authMode: "api_key" }] }] };
    render(<Harness enabled />);
    expect(await screen.findByText(/billed to that API key/)).toBeTruthy();
  });

  it("says plainly when the device is offline, and will not take a message", async () => {
    backend.devices = { devices: [{ id: "d", name: "Studio laptop", online: false, revokedAt: null, runtimeVersion: "1", capabilities: [] }] };
    render(<Harness enabled />);
    expect(await screen.findByText(/Studio laptop is offline/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "hello" } });
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
  });
});

describe("when the sidecar is unreachable", () => {
  it("shows the offline state instead of an empty chat, and sends nothing", async () => {
    backend.down = true;
    render(<Harness enabled />);
    expect(await screen.findByText(/Can't reach the agent service/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "hello" } });
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
    expect(backend.intents()).toEqual([]);
  });
});

describe("a conversation", () => {
  async function ready() {
    render(<Harness enabled />);
    await screen.findByText(/Studio laptop/);
  }

  it("starts a session, waits for it to be live, then sends the text — and shows what comes back as five kinds of line", async () => {
    await ready();
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "Raise the headline" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(backend.intents()).toEqual([{ type: "session.start", payload: { providerId: "codex" } }]));
    // Nothing is sent as input while the provider is not up.
    await act(() => new Promise((resolve) => setTimeout(resolve, 60)));
    expect(backend.intents()).toHaveLength(1);

    backend.push(1, "session.created");
    backend.push(2, "session.started");
    await waitFor(() =>
      expect(backend.intents()).toEqual([
        { type: "session.start", payload: { providerId: "codex" } },
        { type: "session.input", payload: { sessionId: SESSION, text: "Raise the headline" } },
      ]),
    );

    backend.push(3, "turn.started");
    backend.push(4, "agent.output.completed", { text: "On it." });
    backend.push(5, "capability.requested", { invocationId: "i1", capabilityId: "design.get", input: { design_id: "d" } });
    backend.push(6, "capability.completed", { invocationId: "i1", capabilityId: "design.get", result: {} });
    backend.push(7, "capability.requested", { invocationId: "i2", capabilityId: "ui.selectLayer", input: { layer_id: "l1" } });
    backend.push(8, "capability.completed", { invocationId: "i2", capabilityId: "ui.selectLayer", result: { outcome: "handled" } });
    backend.push(9, "approval.requested", { approvalId: "apr_1", invocationId: "i3", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit", importantArguments: [], affectedResourceCount: 1 });

    await screen.findByText("On it.");
    const kinds = () => screen.getAllByRole("listitem").map((item) => item.getAttribute("data-kind"));
    await waitFor(() => expect(kinds()).toEqual(expect.arrayContaining(["person", "agent", "capability", "result", "navigation", "approval", "working"])));
  });

  it("shows an approval with capability, action, parameters, risk and count, and sends the decision", async () => {
    await ready();
    backend.push(1, "session.created");
    backend.push(2, "session.started");
    backend.push(3, "approval.requested", {
      approvalId: "apr_1",
      invocationId: "i3",
      capabilityId: "design.applyOperations",
      risk: "destructive",
      humanSummary: "Edit this design: apply the listed changes together as one new version",
      importantArguments: [
        { name: "design_id", value: "d-123" },
        { name: "operations", value: [{ op: "remove_layer", layer_id: "l1" }] },
      ],
      affectedResourceCount: 1,
    });

    const card = await screen.findByText("Needs your approval");
    const scope = within(card.closest("li") as HTMLElement);
    expect(scope.getByText("design.applyOperations")).toBeTruthy();
    expect(scope.getByText(/Edit this design/)).toBeTruthy();
    expect(scope.getByText("design_id")).toBeTruthy();
    expect(scope.getByText("1 operation: remove_layer")).toBeTruthy();
    expect(scope.getByText("destructive")).toBeTruthy();
    expect(scope.getByText("Affects 1 item.")).toBeTruthy();

    fireEvent.click(scope.getByRole("button", { name: "Approve once" }));
    await waitFor(() =>
      expect(backend.intents()).toContainEqual({ type: "approval.resolve", payload: { approvalId: "apr_1", decision: "approveOnce" } }),
    );
    // Held until the Runtime's own event says it landed, so it cannot be answered twice.
    expect(scope.getByRole<HTMLButtonElement>("button", { name: "Deny" }).disabled).toBe(true);

    backend.push(4, "approval.resolved", { approvalId: "apr_1", status: "approved" });
    await screen.findByText("Approved");
  });

  it("stop sends session.cancel for the live session", async () => {
    await ready();
    backend.push(1, "session.created");
    backend.push(2, "session.started");
    backend.push(3, "turn.started");
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    await waitFor(() => expect(backend.intents()).toEqual([{ type: "session.cancel", payload: { sessionId: SESSION } }]));
    await screen.findByText("You stopped the agent.");
  });

  it("deny sends deny", async () => {
    await ready();
    backend.push(1, "session.created");
    backend.push(2, "approval.requested", { approvalId: "apr_2", invocationId: "i", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit", importantArguments: [], affectedResourceCount: 1 });
    fireEvent.click(await screen.findByRole("button", { name: "Deny" }));
    await waitFor(() => expect(backend.intents()).toEqual([{ type: "approval.resolve", payload: { approvalId: "apr_2", decision: "deny" } }]));
  });
});

describe("the live path", () => {
  const scope = { kind: "application_context", applicationContextId: CONTEXT };
  const durable = (sequence: number, type: string, payload: Record<string, unknown> = {}) => ({ type, sessionId: SESSION, sequence, scope, payload });
  const delta = (text: string) => ({ type: "agent.output.delta", sessionId: SESSION, payload: { turnId: "t1", text } });
  const pageReads = () => backend.calls.filter((call) => call.path === "/pons/v1/events");

  afterEach(() => {
    act(() => liveFeed.setState({ at: "offline" }));
  });

  async function connected() {
    render(<Harness enabled />);
    await screen.findByText(/Studio laptop/);
    await waitFor(() => expect(pageReads().length).toBeGreaterThan(0));
    act(() => liveFeed.setState({ at: "connected", clientId: "c" }));
    await screen.findByText("Connected · live");
  }

  it("shows the reply as it streams, replaces it with the completed text, and ignores a repeat", async () => {
    await connected();
    act(() => {
      liveFeed.publish(durable(1, "session.created"));
      liveFeed.publish(durable(2, "session.started"));
      liveFeed.publish(durable(3, "turn.started", { turnId: "t1" }));
      liveFeed.publish(delta("Raising "));
      liveFeed.publish(delta("the headline"));
    });
    // Nothing was stored for the page to serve: this came by the live path alone.
    const streaming = await screen.findByText("Raising the headline");
    expect(streaming.closest("li")?.getAttribute("data-streaming")).toBe("true");

    act(() => liveFeed.publish(durable(4, "agent.output.completed", { turnId: "t1", text: "Raising the headline to 96pt." })));
    await screen.findByText("Raising the headline to 96pt.");
    expect(screen.queryByText("Raising the headline")).toBeNull();
    expect(document.querySelector("[data-streaming]")).toBeNull();

    // The same sequence again, as a poll would bring it back: one line, not two.
    act(() => liveFeed.publish(durable(4, "agent.output.completed", { turnId: "t1", text: "Raising the headline to 96pt." })));
    act(() => liveFeed.publish(delta(" late")));
    expect(screen.getAllByText("Raising the headline to 96pt.")).toHaveLength(1);
    expect(document.querySelector("[data-streaming]")).toBeNull();
  });

  it("does not apply a frame that skips ahead, and repairs from the page starting at the cursor", async () => {
    await connected();
    act(() => {
      liveFeed.publish(durable(1, "session.created"));
      liveFeed.publish(durable(2, "session.started"));
    });
    // The page has what the stream missed: 3 and 4, and the frame that arrives is 5.
    backend.events = [
      { sequence: 1, type: "session.created", sessionId: SESSION, payload: {} },
      { sequence: 2, type: "session.started", sessionId: SESSION, payload: {} },
      { sequence: 3, type: "agent.output.completed", sessionId: SESSION, payload: { turnId: "a", text: "Missed one" } },
      { sequence: 4, type: "agent.output.completed", sessionId: SESSION, payload: { turnId: "b", text: "Missed two" } },
      { sequence: 5, type: "agent.output.completed", sessionId: SESSION, payload: { turnId: "c", text: "Delivered live" } },
    ];
    act(() => liveFeed.publish(durable(5, "agent.output.completed", { turnId: "c", text: "Delivered live" })));
    expect(screen.queryByText("Delivered live")).toBeNull(); // 5 was not applied ahead of 3 and 4

    await screen.findByText("Delivered live");
    const lines = screen.getAllByRole("listitem").map((item) => item.textContent);
    expect(lines.filter((text) => text?.includes("Missed") || text?.includes("Delivered"))).toEqual([
      expect.stringContaining("Missed one") as string,
      expect.stringContaining("Missed two") as string,
      expect.stringContaining("Delivered live") as string,
    ]);
    expect(pageReads().some((call) => call.search.includes("after=2"))).toBe(true);
  });

  it("reads the page slowly while the stream is connected, and still reads it", async () => {
    await connected();
    await act(() => new Promise((resolve) => setTimeout(resolve, 120)));
    const settled = pageReads().length;
    await act(() => new Promise((resolve) => setTimeout(resolve, 200)));
    // Polling at 20ms would have read ten more times in that window.
    expect(pageReads().length).toBe(settled);
  });

  it("ignores another context's frame, and a principal frame whose sequence is its own", async () => {
    await connected();
    act(() => {
      liveFeed.publish({ type: "session.created", sessionId: SESSION, sequence: 1, scope: { kind: "application_context", applicationContextId: "someone-else" }, payload: {} });
      liveFeed.publish({ type: "device.revoked", sequence: 1, scope: { kind: "principal" }, payload: {} });
      liveFeed.publish(durable(1, "session.created"));
      liveFeed.publish(durable(2, "agent.output.completed", { turnId: "t", text: "Mine" }));
    });
    await screen.findByText("Mine");
  });
});

describe("in the editor", () => {
  const record = {
    id: "d1",
    project_id: "p1",
    name: "Cover",
    version: 1,
    created_at: "2026-10-03T00:00:00Z",
    updated_at: "2026-10-03T00:00:00Z",
    document: { schema_version: 1, format: { preset: "square", width: 1080, height: 1080 }, pages: [{ id: "pg1", name: "Page 1", layers: [] }] },
  };

  function renderEditor(agent: boolean) {
    const base = backend.fetch.getMockImplementation();
    backend.fetch.mockImplementation((input, init) => {
      const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      if (raw.endsWith("/api/v1/designs/d1")) return Promise.resolve(new Response(JSON.stringify(record), { status: 200 }));
      return base ? base(input, init) : Promise.reject(new Error("no backend"));
    });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    return render(
      <QueryClientProvider client={client}>
        <Editor designId="d1" onBack={() => undefined} agent={agent} />
      </QueryClientProvider>,
    );
  }

  it("has no chat, no toggle and no request for one when access is not ready", async () => {
    renderEditor(false);
    await screen.findByText("Cover");
    expect(screen.queryByRole("button", { name: /Agent/ })).toBeNull();
    expect(screen.queryByLabelText("Agent chat")).toBeNull();
    expect(backend.calls.some((call) => call.path === "/agent/context" || call.path === "/pons/v1/events")).toBe(false);
  });

  it("is closed until asked for, then opens beside the canvas", async () => {
    renderEditor(true);
    const toggle = await screen.findByRole("button", { name: /Agent/ });
    expect(screen.queryByLabelText("Agent chat")).toBeNull();
    fireEvent.click(toggle);
    expect(screen.getByLabelText("Agent chat")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByLabelText("Agent chat")).toBeNull();
  });

  it("badges the toggle with an approval that arrives while the panel is shut", async () => {
    renderEditor(true);
    await screen.findByRole("button", { name: /Agent/ });
    backend.push(1, "session.created");
    backend.push(2, "approval.requested", { approvalId: "apr_1", invocationId: "i", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit", importantArguments: [], affectedResourceCount: 1 });
    expect(await screen.findByLabelText("1 waiting for your approval", {}, { timeout: 4_000 })).toBeTruthy();
  });
});
