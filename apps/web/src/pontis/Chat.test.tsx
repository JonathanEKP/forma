/**
 * The chat as a person meets it, with `fetch` as the only seam: the sidecar's
 * three browser routes and the event page are the things it shares with the real
 * thing, so those are what is faked.
 */
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../identity", () => ({
  supabase: null,
  accessToken: () => Promise.resolve("session-token"),
  currentSubject: () => Promise.resolve(null),
  onSessionChange: () => () => undefined,
  currentSession: () => Promise.resolve(null),
}));

import { liveFeed } from "./channel";
import { DEAD } from "./chat";
import { ChatPanel, ChatToggle } from "./Chat";
import { forgetChat } from "./session-store";
import { agentBinding, windowId } from "./window";
import { useChat, type ChatOptions } from "./useChat";

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
  unauthenticatedPaths = new Set<string>();
  cancelGate: Promise<Response> | null = null;
  startGate: Promise<Response> | null = null;
  /** What `session.input` and `session.cancel` are answered with instead of delivery, as the sidecar's refusal code. */
  refuse: string | null = null;
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

  push(sequence: number, type: string, payload: Record<string, unknown> = {}, deviceId = "dev_1") {
    this.events.push({ sequence, type, sessionId: SESSION, deviceId, payload });
  }

  readonly fetch = vi.fn((input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://x");
    const method = init?.method ?? "GET";
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    this.calls.push({ path: url.pathname, search: url.search, method, body });
    const json = (value: unknown, status = 200) => Promise.resolve(new Response(JSON.stringify(value), { status }));
    if (this.down) return Promise.reject(new TypeError("fetch failed"));
    if (this.unauthenticatedPaths.has(url.pathname)) return json({ error: { code: "unauthenticated" } }, 401);

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
        if (type === "session.start" && this.startGate !== null) return this.startGate;
        if (type === "session.cancel" && this.cancelGate !== null) return this.cancelGate;
        if (this.refuse !== null && (type === "session.input" || type === "session.cancel")) {
          return json({ error: this.refuse }, this.refuse === "not_authorized" ? 403 : 409);
        }
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

function Harness({ enabled, offlineGraceMs, view }: { enabled: boolean; offlineGraceMs?: number; view?: ChatOptions["view"] }) {
  const chat = useChat({ enabled, pollMs: 20, ...(offlineGraceMs === undefined ? {} : { offlineGraceMs }), ...(view === undefined ? {} : { view }) });
  return (
    <>
      <ChatToggle chat={chat} open onToggle={() => undefined} />
      <ChatPanel chat={chat} onClose={() => undefined} />
    </>
  );
}

let backend: Backend;

/** This tab's chat already belongs to `SESSION`, as it does after a reload: what the history says of it is shown. */
async function attach(sessionId = SESSION) {
  sessionStorage.setItem(
    "forma.chat",
    JSON.stringify({ context: CONTEXT, window: await windowId(), remembered: { sessionId, deviceId: null, dead: null, said: [] } }),
  );
}

beforeEach(() => {
  // A tab remembers its chat across a reload; a test is not a reload of the one before it.
  sessionStorage.clear();
  localStorage.clear();
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

  it("lets a message through to a provider whose sign-in cannot be known, and says it is unverified", async () => {
    backend.devices = { devices: [{ id: "d", name: "Box", online: true, revokedAt: null, runtimeVersion: "1", capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: false, authMode: "unknown" }] }] };
    render(<Harness enabled />);
    expect(await screen.findByText(/Sign-in not verified/)).toBeTruthy();
    expect(screen.queryByText(/Unavailable/)).toBeNull();
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "hello" } });
    await waitFor(() => expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(false));
  });

  it("says plainly when the device is offline, and will not take a message", async () => {
    backend.devices = { devices: [{ id: "d", name: "Studio laptop", online: false, revokedAt: null, runtimeVersion: "1", capabilities: [] }] };
    render(<Harness enabled />);
    const status = await screen.findByRole("region", { name: "Status" });
    expect(within(status).getByText(/Studio laptop is offline/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "hello" } });
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
  });

  it("opens Run on and explains the disabled Send control when two computers have no saved choice", async () => {
    backend.devices = twoDevices();
    render(<Harness enabled />);
    const runOn = await screen.findByRole("button", { name: "Run on: Choose a computer" });
    await waitFor(() => expect(runOn.getAttribute("aria-expanded")).toBe("true"));
    expect(screen.getByText("Choose a computer to run the agent.")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "hello" } });
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
    expect(backend.intents()).toEqual([]);
  });

  it("uses the last computer chosen in localStorage while multiple computers are online", async () => {
    backend.devices = twoDevices();
    localStorage.setItem("forma.chat.device", "dev_2");
    render(<Harness enabled />);
    await screen.findByRole("button", { name: "Run on: Home desktop" });
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "hello" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents()).toContainEqual({ type: "session.start", payload: { providerId: "codex", deviceId: "dev_2" } }));
  });

  it("automatically uses the only online computer and includes its id in session.start", async () => {
    render(<Harness enabled />);
    await screen.findByText(/Studio laptop/);
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "hello" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents()).toContainEqual({ type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } }));
  });

  it("says to sign in again when the context and device routes return 401", async () => {
    backend.unauthenticatedPaths.add("/agent/context");
    backend.unauthenticatedPaths.add("/pons/v1/devices");
    render(<Harness enabled />);

    const status = await screen.findByRole("region", { name: "Status" });
    expect(within(status).getByText("Your sign-in has expired. Sign in again.")).toBeTruthy();
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

  it("keeps Send disabled while empty and tells the person what the control needs", async () => {
    await ready();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
    expect(screen.getByText("Write a message to send.")).toBeTruthy();
  });

  it("starts a session, waits for it to be live, then sends the text — and shows what comes back as five kinds of line", async () => {
    await ready();
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "Raise the headline" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(backend.intents()).toEqual([{ type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } }]));
    // Nothing is sent as input while the provider is not up.
    await act(() => new Promise((resolve) => setTimeout(resolve, 60)));
    expect(backend.intents()).toHaveLength(1);

    backend.push(1, "session.created");
    backend.push(2, "session.started");
    await waitFor(() =>
      expect(backend.intents()).toEqual([
        { type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } },
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

  it("restores a draft after reload while its first send is pending, without resending it", async () => {
    const text = "Read the manual first.";
    backend.startGate = new Promise<Response>(() => undefined);
    const first = render(<Harness enabled />);
    await screen.findByText(/Studio laptop/);
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: text } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents()).toEqual([{ type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } }]));
    expect(sessionStorage.getItem("forma.chat.draft")).toBe(text);

    first.unmount();
    backend.startGate = null;
    render(<Harness enabled />);
    const draft = await screen.findByRole<HTMLTextAreaElement>("textbox", { name: "Message the agent" });
    expect(draft.value).toBe(text);
    expect(backend.intents()).toEqual([{ type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } }]);

    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents()).toEqual([{ type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } }, { type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } }]));
    backend.push(1, "session.created");
    backend.push(2, "session.started");
    await waitFor(() => expect(backend.intents()).toEqual([
      { type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } },
      { type: "session.start", payload: { providerId: "codex", deviceId: "dev_1" } },
      { type: "session.input", payload: { sessionId: SESSION, text } },
    ]));
    await waitFor(() => expect(sessionStorage.getItem("forma.chat.draft")).toBeNull());
  });

  it("tells the window's channel which session this chat started, and lets go when the chat goes", async () => {
    agentBinding.set(null);
    const { unmount } = render(<Harness enabled />);
    await screen.findByText(/Studio laptop/);
    expect(agentBinding.get()).toBeNull();

    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "Raise the headline" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    // The id the sidecar minted for this chat, which is what an action is routed by (D-2).
    await waitFor(() => expect(agentBinding.get()).toBe(SESSION));

    unmount();
    expect(agentBinding.get()).toBeNull();
  });

  it("shows an approval with capability, action, parameters, risk and count, and sends the decision", async () => {
    await attach();
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
    expect(scope.getByText("Sending…")).toBeTruthy();
    // Held until the Runtime's own event says it landed, so it cannot be answered twice.
    expect(scope.getByRole<HTMLButtonElement>("button", { name: "Deny" }).disabled).toBe(true);

    backend.push(4, "approval.resolved", { approvalId: "apr_1", status: "approved" });
    await screen.findByText("Approved");
  });

  it("lets a pending approval be answered again when the device does not confirm it", async () => {
    await attach();
    await ready();
    backend.push(1, "session.created");
    backend.push(2, "session.started");
    backend.push(3, "approval.requested", {
      approvalId: "apr_1",
      invocationId: "i3",
      capabilityId: "design.applyOperations",
      risk: "write",
      humanSummary: "Edit this design",
      importantArguments: [],
      affectedResourceCount: 1,
    });

    const card = await screen.findByText("Needs your approval");
    const scope = within(card.closest("li") as HTMLElement);
    vi.useFakeTimers();
    fireEvent.click(scope.getByRole("button", { name: "Approve once" }));
    expect(scope.getByText("Sending…")).toBeTruthy();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(20_000);
    });

    expect(scope.getByText("Not confirmed by your device")).toBeTruthy();
    expect(scope.getByRole<HTMLButtonElement>("button", { name: "Approve once" }).disabled).toBe(false);
    expect(scope.getByRole<HTMLButtonElement>("button", { name: "Deny" }).disabled).toBe(false);
    vi.useRealTimers();
  });

  it("stop sends session.cancel for the live session", async () => {
    await attach();
    await ready();
    backend.push(1, "session.created");
    backend.push(2, "session.started");
    backend.push(3, "turn.started");
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    await waitFor(() => expect(backend.intents()).toEqual([{ type: "session.cancel", payload: { sessionId: SESSION } }]));
    await screen.findByText("You stopped the agent.");
  });

  it("keeps Send disabled while a turn runs and leaves Stop available", async () => {
    await inSession();
    backend.push(3, "turn.started");
    await screen.findByText("The agent is working…");
    const box = screen.getByLabelText("Message the agent");
    fireEvent.change(box, { target: { value: "wait until this turn ends" } });

    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
    expect(screen.getByText("The agent is working. Stop it to send now.")).toBeTruthy();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Stop" }).disabled).toBe(false);
    fireEvent.keyDown(box, { key: "Enter", shiftKey: false });
    expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.input")).toHaveLength(1);
  });

  it("ignores a second Stop while the first cancel is pending", async () => {
    await inSession();
    backend.push(3, "turn.started");
    let finish!: (response: Response) => void;
    backend.cancelGate = new Promise((resolve) => {
      finish = resolve;
    });

    const stop = await screen.findByRole("button", { name: "Stop" });
    fireEvent.click(stop);
    fireEvent.click(stop);
    await waitFor(() => expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.cancel")).toHaveLength(1));
    finish(new Response(JSON.stringify({ delivered: true }), { status: 200 }));

    await screen.findByText("You stopped the agent.");
    expect(screen.getByRole("region", { name: "Session information" }).getAttribute("data-tone")).toBe("info");
    expect(screen.getAllByText("You stopped the agent.")).toHaveLength(1);
  });

  it("maps an intent 401 to the sign-in expiry message", async () => {
    await inSession();
    backend.unauthenticatedPaths.add("/agent/intent");
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "try again" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(await screen.findByText("Your sign-in has expired.")).toBeTruthy();
    expect(screen.getByText("Sign in again, then start a new session.")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Session needs you" }).getAttribute("data-tone")).toBe("needs-you");
  });

  it("shows the project context chip and sends only the selected project id", async () => {
    await inSession(() => render(<Harness enabled view={() => ({ ids: { projectId: "project-1" }, chip: { project: "Launch" } })} />));
    expect(screen.getByText("Project “Launch”")).toBeTruthy();
    expect(backend.intents()).toContainEqual({ type: "session.input", payload: { sessionId: SESSION, text: "work on it", view: { projectId: "project-1" } } });
  });

  it("renders UI outcomes as article cards with identifiers inside Details", async () => {
    await inSession();
    backend.push(3, "capability.requested", { invocationId: "select", capabilityId: "ui.selectLayer", input: { design_id: "design-123", layer_id: "layer-456" } });
    backend.push(4, "capability.completed", { invocationId: "select", capabilityId: "ui.selectLayer", result: { outcome: "handled", detail: "Selected “Headline”" } });

    const card = await screen.findByRole("article", { name: "Moved your view" });
    expect(card.parentElement?.tagName).toBe("LI");
    expect(card.querySelector("div")).toBeNull();
    expect(within(card).getByRole("heading", { name: "Moved your view" })).toBeTruthy();
    expect(within(card).getByText("Selected “Headline”").closest("details")).toBeNull();
    const details = card.querySelector("details");
    expect(details?.textContent).toContain("design-123");
    expect(details?.textContent).toContain("layer-456");
    expect(card.querySelector("ul")?.textContent).not.toMatch(/design-123|layer-456/);
  });

  it("shows session information in an anchored popover without a backdrop", async () => {
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "Session information" }));
    const popover = screen.getByRole("region", { name: "Session information" });
    expect(popover.textContent).toContain("This window's session");
    expect(popover.textContent).toContain("Started");
    expect(popover.querySelector("[aria-modal], dialog")).toBeNull();
    expect(document.querySelector("[aria-modal], dialog")).toBeNull();
  });

  it("offers Jump to latest only after the transcript is scrolled up, and follows the newest line after the jump", async () => {
    await inSession();
    const transcript = screen.getByRole("log");
    Object.defineProperty(transcript, "scrollHeight", { configurable: true, value: 600 });
    Object.defineProperty(transcript, "clientHeight", { configurable: true, value: 120 });
    transcript.scrollTop = 0;
    fireEvent.scroll(transcript);
    expect(screen.getByRole("button", { name: "Jump to latest" })).toBeTruthy();

    backend.push(3, "agent.output.completed", { turnId: "latest", text: "The newest reply." });
    await screen.findByText("The newest reply.");
    expect(transcript.scrollTop).toBe(0);
    fireEvent.click(screen.getByRole("button", { name: "Jump to latest" }));
    expect(transcript.scrollTop).toBe(600);
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
  });

  it("keeps an oversized UTF-8 draft and refuses to send it", async () => {
    await ready();
    const draft = "😀".repeat(16_385);
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: draft } });

    expect(await screen.findByText("That message is too long (max 64 KB).")).toBeTruthy();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", { name: "Message the agent" }).value).toBe(draft);
    expect(backend.intents()).toEqual([]);
  });

  it("isolates the direction of person and agent text", async () => {
    await inSession();
    const bidi = "plain \u202eright-to-left\u202c text";
    backend.push(3, "agent.output.completed", { turnId: "t1", text: bidi });

    const person = await screen.findByText("work on it");
    const agent = await screen.findByText(bidi);
    for (const bubble of [person, agent]) {
      expect(bubble.getAttribute("dir")).toBe("auto");
      expect(bubble.style.unicodeBidi).toBe("plaintext");
    }
  });

  it("deny sends deny", async () => {
    await attach();
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
    await attach();
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

/** A tab's chat that has a session, started here, with a turn open. */
async function inSession(render_: () => void = () => render(<Harness enabled offlineGraceMs={150} />)) {
  render_();
  await screen.findByText(/Studio laptop/);
  fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "work on it" } });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(backend.intents()).toHaveLength(1));
  backend.push(1, "session.created");
  backend.push(2, "session.started");
  await waitFor(() => expect(backend.intents()).toHaveLength(2));
}

const oneDevice = (overrides: Record<string, unknown> = {}) => ({
  devices: [
    {
      id: "dev_1",
      name: "Studio laptop",
      online: true,
      revokedAt: null,
      runtimeVersion: "0.1.0",
      capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: true, authMode: "subscription" }],
      ...overrides,
    },
  ],
});

const twoDevices = () => ({
  devices: [
    {
      id: "dev_1",
      name: "Studio laptop",
      online: true,
      revokedAt: null,
      runtimeVersion: "0.1.0",
      capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: true, authMode: "subscription" }],
    },
    {
      id: "dev_2",
      name: "Home desktop",
      online: true,
      revokedAt: null,
      runtimeVersion: "0.1.1",
      lastSeenAt: "2026-10-06T19:45:00Z",
      capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: true, authMode: "subscription" }],
    },
  ],
});

describe("a session that cannot be carried on", () => {
  it("says why in plain words, offers a new session and keeps the history above it — instead of answering every message with the same refusal (F-chat-5)", async () => {
    await inSession();
    backend.push(3, "turn.started");
    backend.push(4, "agent.output.completed", { turnId: "t", text: "Working." });
    backend.push(5, "turn.completed");
    await screen.findByText("Working.");

    backend.refuse = "not_authorized";
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "and again" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    const ended = await screen.findByRole("region", { name: "Session stopped" });
    expect(within(ended).getByText(DEAD.notAuthorized.happened)).toBeTruthy();
    expect(ended.getAttribute("data-tone")).toBe("stopped");
    // Not wedged: no "working", the box does not take a message, and nothing started by itself.
    expect(screen.queryByText("The agent is working…")).toBeNull();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
    expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.start")).toHaveLength(1);

    backend.refuse = null;
    fireEvent.click(within(ended).getByRole("button", { name: "New session" }));
    await waitFor(() => expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.start")).toHaveLength(2));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Session stopped" })).toBeNull());
    expect(document.activeElement).toBe(screen.getByLabelText("Message the agent"));
    // The old conversation is still there, above a divider.
    const kinds = screen.getAllByRole("listitem").map((item) => item.getAttribute("data-kind"));
    expect(kinds).toEqual(expect.arrayContaining(["person", "agent"]));
    expect(screen.getByRole("separator").textContent).toContain("New session");
    expect(kinds.indexOf("divider")).toBeGreaterThan(kinds.lastIndexOf("agent"));
  });

  it("turns a card that was waiting into an expired one with no buttons", async () => {
    await inSession();
    backend.push(3, "approval.requested", { approvalId: "apr_1", invocationId: "i", capabilityId: "design.applyOperations", risk: "destructive", humanSummary: "Remove", importantArguments: [], affectedResourceCount: 1 });
    await screen.findByRole("button", { name: "Approve once" });

    backend.push(4, "session.failed", { error: { message: "the provider exited" } });
    await screen.findByText(/The session failed: the provider exited/);
    expect(screen.queryByRole("button", { name: "Approve once" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Deny" })).toBeNull();
    expect(screen.getByText("Expired before you answered")).toBeTruthy();
  });

  it("is what a revoked device leaves behind, and the status says revoked rather than offline (F-10-2, F-10-4)", async () => {
    await inSession();
    backend.push(3, "turn.started");
    await screen.findByText("The agent is working…");
    backend.devices = oneDevice({ online: false, revokedAt: "2026-10-04T00:00:00Z" });
    // The device list is read on presence frames and every ten seconds; a presence frame is the quick way.
    act(() => liveFeed.publish({ type: "device.revoked", payload: {} }));
    expect(await screen.findByText(DEAD.revoked.happened)).toBeTruthy();
    // Said by the status line (and again by the button's hint): revoked, not offline.
    expect(screen.getAllByText(/Studio laptop was revoked/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/is offline/)).toBeNull();
    expect(screen.queryByText("The agent is working…")).toBeNull();
  });

  it("ends a revoked session as revoked even when two older devices are offline (F-10-14)", async () => {
    backend.devices = {
      devices: [
        { id: "dev_old_1", name: "Old desktop", online: false, revokedAt: null, pairedAt: "2026-10-01T00:00:00Z", capabilities: [] },
        { id: "dev_old_2", name: "Old laptop", online: false, revokedAt: null, pairedAt: "2026-10-02T00:00:00Z", capabilities: [] },
        { id: "dev_1", name: "Studio laptop", online: true, revokedAt: null, pairedAt: "2026-10-03T00:00:00Z", capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: true, authMode: "subscription" }] },
      ],
    };
    await inSession(() => render(<Harness enabled offlineGraceMs={20} />));
    backend.push(3, "turn.started");
    await screen.findByText("The agent is working…");

    backend.devices = {
      devices: [
        { id: "dev_old_1", name: "Old desktop", online: false, revokedAt: null, pairedAt: "2026-10-01T00:00:00Z", capabilities: [] },
        { id: "dev_old_2", name: "Old laptop", online: false, revokedAt: null, pairedAt: "2026-10-02T00:00:00Z", capabilities: [] },
        { id: "dev_1", name: "Studio laptop", online: false, revokedAt: "2026-10-04T00:00:00Z", pairedAt: "2026-10-03T00:00:00Z", capabilities: [] },
      ],
    };
    act(() => liveFeed.publish({ type: "device.revoked", payload: {} }));

    const ended = await screen.findByRole("region", { name: "Session stopped" });
    expect(within(ended).getByText(DEAD.revoked.happened)).toBeTruthy();
    expect(within(ended).getByRole("link", { name: "Open Agents page" })).toBeTruthy();
    expect(screen.getAllByText(/Studio laptop was revoked/).length).toBeGreaterThan(0);
    expect(screen.queryByText(DEAD.offline.happened)).toBeNull();
  });

  it("judges the session device as revoked even when another device is online", async () => {
    await inSession();
    backend.devices = {
      devices: [
        { id: "dev_2", name: "Other laptop", online: true, revokedAt: null, capabilities: [] },
        { id: "dev_1", name: "Studio laptop", online: false, revokedAt: "2026-10-04T00:00:00Z", capabilities: [] },
      ],
    };
    act(() => liveFeed.publish({ type: "device.revoked", payload: {} }));
    expect(await screen.findByText(DEAD.revoked.happened)).toBeTruthy();
    expect(screen.queryByText(DEAD.replaced.happened)).toBeNull();
  });

  it("treats a session device missing from the device list as revoked", async () => {
    await inSession();
    backend.devices = {
      devices: [{ id: "dev_2", name: "Other laptop", online: true, revokedAt: null, capabilities: [] }],
    };
    act(() => liveFeed.publish({ type: "device.online", payload: {} }));
    expect(await screen.findByText(DEAD.revoked.happened)).toBeTruthy();
  });

  it("keeps a session on its own offline computer when another computer is online", async () => {
    await inSession();
    backend.devices = {
      devices: [
        { id: "dev_1", name: "Studio laptop", online: false, revokedAt: null, capabilities: [] },
        { id: "dev_2", name: "Other laptop", online: true, revokedAt: null, capabilities: [] },
      ],
    };
    act(() => liveFeed.publish({ type: "device.online", payload: {} }));
    expect(await screen.findByText(DEAD.offline.happened)).toBeTruthy();
    expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.start")).toHaveLength(1);
  });

  it("stops this session when its computer goes offline and does not switch it when another is online", async () => {
    await inSession(() => render(<Harness enabled offlineGraceMs={600} />));
    backend.devices = oneDevice({ online: false });
    act(() => liveFeed.publish({ type: "device.offline", payload: {} }));
    expect(await screen.findByText(DEAD.offline.happened)).toBeTruthy();
    backend.devices = { devices: [
      { id: "dev_1", name: "Studio laptop", online: false, revokedAt: null, capabilities: [] },
      { id: "dev_2", name: "Home desktop", online: true, revokedAt: null, capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: true, authMode: "subscription" }] },
    ] };
    act(() => liveFeed.publish({ type: "device.online", payload: {} }));
    expect(await screen.findByText(DEAD.offline.happened)).toBeTruthy();
    expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.start")).toHaveLength(1);
  });

  it("starts a fresh session on the chosen computer and leaves the earlier transcript above a named divider", async () => {
    await inSession();
    backend.devices = twoDevices();
    act(() => liveFeed.publish({ type: "device.online", payload: {} }));
    fireEvent.click(await screen.findByRole("button", { name: "Run on: Studio laptop" }));
    fireEvent.click(await screen.findByRole("button", { name: /Home desktop Online/ }));

    await waitFor(() => expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.start")).toHaveLength(2));
    expect(backend.intents()).toContainEqual({ type: "session.start", payload: { providerId: "codex", deviceId: "dev_2" } });
    expect(screen.getByRole("separator").textContent).toMatch(/^New session on Home desktop · \d{2}:\d{2}$/);
    expect(document.activeElement).toBe(screen.getByLabelText("Message the agent"));
    expect(localStorage.getItem("forma.chat.device")).toBe("dev_2");
  });

  it("is what Stop becomes when the Runtime cannot even be told: the turn ends rather than staying 'working' (F-10-2)", async () => {
    await inSession();
    backend.push(3, "turn.started");
    backend.refuse = "runtime_offline";
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    expect(await screen.findByText(DEAD.stopWhileOffline.happened)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
  });

  it("is remembered across a reload, with the history and without starting anything", async () => {
    await inSession();
    backend.refuse = "not_authorized";
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "again" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByText(DEAD.notAuthorized.happened);
    cleanup();

    backend.refuse = null;
    render(<Harness enabled />);
    expect(await screen.findByText(DEAD.notAuthorized.happened)).toBeTruthy();
    expect(screen.getByText("work on it")).toBeTruthy();
    expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.start")).toHaveLength(1);
  });

  it("continues the same session after the person's own Stop", async () => {
    await inSession();
    backend.push(3, "turn.started");
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    await screen.findByText("You stopped the agent.");
    expect(screen.getByRole("region", { name: "Session information" }).getAttribute("data-tone")).toBe("info");
    expect(screen.queryByRole("region", { name: "Session stopped" })).toBeNull();

    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "continue here" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.input")).toHaveLength(2));
    expect(backend.intents()).toContainEqual({ type: "session.input", payload: { sessionId: SESSION, text: "continue here" } });
    expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.start")).toHaveLength(1);
  });
});

describe("a window's session", () => {
  const Probe = () => {
    const chat = useChat({ enabled: true, pollMs: 20 });
    return <output aria-label="session">{chat.sessionId ?? "none"}</output>;
  };

  it("is exposed once this window has started one, so the window can be bound to it, and not before", async () => {
    render(<Probe />);
    await act(() => Promise.resolve());
    expect(screen.getByLabelText("session").textContent).toBe("none");
  });

  it("binds the window only to a session this window started, and lets go once it is over (D-2)", async () => {
    agentBinding.set(null);
    // A session in the history that this window did not start is neither shown nor spoken for.
    render(<Harness enabled offlineGraceMs={150} />);
    await screen.findByText(/Studio laptop/);
    backend.push(1, "session.created");
    backend.push(2, "session.started");
    backend.push(3, "agent.output.completed", { turnId: "t", text: "From before." });
    await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
    expect(screen.queryByText("From before.")).toBeNull();
    expect(agentBinding.get()).toBeNull();
    cleanup();

    // One it started is bound — until it dies.
    sessionStorage.clear();
    backend.events = [];
    await inSession();
    await waitFor(() => expect(agentBinding.get()).toBe(SESSION));
    backend.push(3, "session.failed", { error: { message: "gone" } });
    await screen.findByRole("region", { name: "Session stopped" });
    expect(agentBinding.get()).toBeNull();
  });

  it("is forgotten by a tab that signs out", async () => {
    await inSession();
    expect(sessionStorage.getItem("forma.chat")).toContain(SESSION);
    forgetChat();
    expect(sessionStorage.getItem("forma.chat")).toBeNull();
  });

  it("is not taken over by a tab that was copied from this one: it starts with no session of its own to speak into", async () => {
    await inSession();
    backend.push(3, "agent.output.completed", { turnId: "t", text: "Said to the original." });
    await screen.findByText("Said to the original.");
    // Duplicating a tab copies its storage, record and all; the duplicate is handed a window of its own.
    const copied: unknown = JSON.parse(sessionStorage.getItem("forma.chat") ?? "null");
    expect(copied).not.toBeNull();
    cleanup();
    sessionStorage.setItem("forma.chat", JSON.stringify({ ...(copied as Record<string, unknown>), window: "another-window" }));
    agentBinding.set(null);

    render(<Harness enabled />);
    await screen.findByText(/Studio laptop/);
    await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
    expect(screen.queryByText("Said to the original.")).toBeNull();
    expect(screen.queryByText("work on it")).toBeNull();
    expect(screen.queryByRole("region", { name: "Session stopped" })).toBeNull();
    expect(agentBinding.get()).toBeNull();
    // Its first message starts a conversation of its own, not a line into the original's.
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "mine" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(backend.intents().filter((intent) => (intent as { type: string }).type === "session.start")).toHaveLength(2));
  });

  it("is not given to a different person who signs in to the same tab", async () => {
    sessionStorage.setItem(
      "forma.chat",
      JSON.stringify({ context: "someone-else", remembered: { sessionId: "ses_theirs", deviceId: null, dead: null, said: [{ id: "l1", text: "their secret", afterSeq: 0, sessionId: "ses_theirs" }] } }),
    );
    render(<Harness enabled />);
    await screen.findByText(/Studio laptop/);
    expect(screen.queryByText("their secret")).toBeNull();
    expect(screen.getByRole("log").querySelectorAll("li")).toHaveLength(1); // only the empty-state text
  });
});

describe("an unsent draft", () => {
  it("survives a reload in this tab", async () => {
    await attach();
    const first = render(<Harness enabled />);
    await screen.findByText(/Studio laptop/);
    fireEvent.change(screen.getByLabelText("Message the agent"), { target: { value: "keep this draft" } });
    first.unmount();

    render(<Harness enabled />);
    expect((await screen.findByRole<HTMLTextAreaElement>("textbox", { name: "Message the agent" })).value).toBe("keep this draft");
  });
});
