/**
 * The chat's logic: what the four intents send, how events become the five kinds
 * of line, and what an approval card is made of. The routes themselves are the
 * sidecar's (`apps/pontis-sidecar/src/intents.ts`); what is checked here is that
 * the bodies are exactly the ones it rebuilds from.
 */
import { describe, expect, it, vi } from "vitest";

import {
  approvalView,
  chatReducer,
  createAgentClient,
  emptyChat,
  pendingApprovals,
  runtimeView,
  type ChatState,
} from "./chat";
import type { DurableEvent } from "./events";

const SESSION = "ses_1";

function client(answer: (path: string, init: RequestInit) => Response | Promise<Response>) {
  const calls: { path: string; method: string; body: unknown; authorization: string | null }[] = [];
  const fetchImpl = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const options = init ?? {};
    calls.push({
      path,
      method: options.method ?? "GET",
      body: typeof options.body === "string" ? (JSON.parse(options.body) as unknown) : undefined,
      authorization: new Headers(options.headers).get("authorization"),
    });
    return await answer(path, options);
  });
  return { calls, agent: createAgentClient({ token: () => Promise.resolve("tok"), fetchImpl }) };
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

describe("the four intents", () => {
  it("start sends only the provider, and returns the id the sidecar minted", async () => {
    const { agent, calls } = client(() => json({ delivered: true, sessionId: SESSION }));
    expect(await agent.start("codex")).toEqual({ ok: true, value: { sessionId: SESSION } });
    expect(calls[0]).toMatchObject({
      path: "/agent/intent",
      method: "POST",
      body: { type: "session.start", payload: { providerId: "codex" } },
      authorization: "Bearer tok",
    });
  });

  it("input carries the session and the text", async () => {
    const { agent, calls } = client(() => json({ delivered: true }));
    expect((await agent.input(SESSION, "Raise the headline")).ok).toBe(true);
    expect(calls[0]?.body).toEqual({ type: "session.input", payload: { sessionId: SESSION, text: "Raise the headline" } });
  });

  it("cancel names the session and nothing else", async () => {
    const { agent, calls } = client(() => json({ delivered: true }));
    expect((await agent.cancel(SESSION)).ok).toBe(true);
    expect(calls[0]?.body).toEqual({ type: "session.cancel", payload: { sessionId: SESSION } });
  });

  it("approval sends the id and one of the two decisions — never who approved", async () => {
    const { agent, calls } = client(() => json({ delivered: true }));
    await agent.approval("apr_1", "approveOnce");
    await agent.approval("apr_1", "deny");
    expect(calls.map((call) => call.body)).toEqual([
      { type: "approval.resolve", payload: { approvalId: "apr_1", decision: "approveOnce" } },
      { type: "approval.resolve", payload: { approvalId: "apr_1", decision: "deny" } },
    ]);
  });

  it("puts the bearer in a header and never in the URL", async () => {
    const { agent, calls } = client(() => json({ delivered: true }));
    await agent.cancel(SESSION);
    expect(calls[0]?.path).not.toContain("tok");
  });
});

describe("failures are said in words", () => {
  it("an unreachable sidecar is `unreachable`, not a thrown error", async () => {
    const { agent } = client(() => Promise.reject(new TypeError("fetch failed")));
    expect(await agent.input(SESSION, "hi")).toMatchObject({ ok: false, failure: { reason: "unreachable" } });
  });

  it("a proxy's 502 for a sidecar that is not there is `unreachable` too", async () => {
    const { agent } = client(() => new Response("bad gateway", { status: 502 }));
    expect(await agent.start("codex")).toMatchObject({ ok: false, failure: { reason: "unreachable" } });
  });

  it("maps the sidecar's refusals", async () => {
    for (const code of ["runtime_offline", "ambiguous_device", "not_authorized", "not_authenticated"] as const) {
      const { agent } = client(() => json({ error: code }, code === "runtime_offline" || code === "ambiguous_device" ? 409 : 403));
      expect(await agent.input(SESSION, "hi")).toMatchObject({ ok: false, failure: { reason: code } });
    }
  });

  it("an intent the Runtime did not take is not a success", async () => {
    const { agent } = client(() => json({ delivered: false }));
    expect(await agent.input(SESSION, "hi")).toMatchObject({ ok: false, failure: { reason: "not_delivered" } });
  });
});

describe("the application context", () => {
  it("is asked of the sidecar, which is the only place that knows it", async () => {
    const { agent, calls } = client(() => json({ applicationContextId: "ctx" }));
    expect(await agent.context()).toEqual({ ok: true, value: "ctx" });
    expect(calls[0]).toMatchObject({ path: "/agent/context", method: "GET", authorization: "Bearer tok" });
  });

  it("is not guessed when the answer has none", async () => {
    const { agent } = client(() => json({}));
    expect((await agent.context()).ok).toBe(false);
  });
});

const device = (overrides: Record<string, unknown> = {}) => ({
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
  ...overrides,
});

describe("runtime status", () => {
  it("names the device, the provider, its auth mode and the registry tokens of its profile", () => {
    expect(runtimeView({ devices: [device()] })).toEqual({
      at: "online",
      name: "Studio laptop",
      runtimeVersion: "0.1.0",
      provider: {
        id: "codex",
        authMode: "subscription",
        usable: true,
        classification: "explicit_opt_in",
        sandbox: "provider_managed",
      },
    });
  });

  it("says why a provider cannot be used", () => {
    const view = runtimeView({ devices: [device({ capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: false, authMode: "not_authenticated" }] })] });
    expect(view).toMatchObject({ at: "online", provider: { usable: false, unusableBecause: "not signed in on this device" } });
  });

  it("distinguishes offline, ambiguous and none, and ignores a revoked device", () => {
    expect(runtimeView({ devices: [device({ online: false })] })).toEqual({ at: "offline", name: "Studio laptop" });
    expect(runtimeView({ devices: [device(), device({ id: "dev_2" })] })).toEqual({ at: "ambiguous" });
    expect(runtimeView({ devices: [device({ revokedAt: "2026-10-01T00:00:00Z" })] })).toEqual({ at: "none" });
    expect(runtimeView({ devices: [] })).toEqual({ at: "none" });
  });
});

describe("an approval card", () => {
  const request = {
    approvalId: "apr_1",
    invocationId: "inv_1",
    capabilityId: "design.applyOperations",
    risk: "destructive",
    humanSummary: "Edit this design: apply the listed changes together as one new version",
    importantArguments: [
      { name: "design_id", value: "d-123" },
      { name: "operations", value: [{ op: "remove_layer", layer_id: "l1" }, { op: "set_text", layer_id: "l2" }] },
    ],
    affectedResourceCount: 2,
    expiresAt: "2026-10-03T12:00:00Z",
  };

  it("shows capability, action, important parameters, risk and the resource count", () => {
    expect(approvalView(request, undefined)).toEqual({
      approvalId: "apr_1",
      capabilityId: "design.applyOperations",
      action: "Edit this design: apply the listed changes together as one new version",
      parameters: [
        { name: "design_id", value: "d-123" },
        { name: "operations", value: "2 operations: remove_layer, set_text" },
      ],
      risk: "destructive",
      affected: 2,
      expiresAt: "2026-10-03T12:00:00Z",
    });
  });

  it("is rebuilt from the catalog's presentation and the agent's input when the request is bare", () => {
    const bare = { approvalId: request.approvalId, invocationId: request.invocationId, capabilityId: request.capabilityId, risk: request.risk };
    const view = approvalView(bare, {
      design_id: "d-123",
      base_version: 4,
      operations: [{ op: "remove_layer", layer_id: "l1" }],
    });
    expect(view?.action).toBe("Edit this design: apply the listed changes together as one new version");
    expect(view?.parameters.map((p) => p.name)).toEqual(["design_id", "base_version", "operations"]);
    expect(view?.affected).toBe(1);
  });

  it("is not made for a request with no id to answer", () => {
    expect(approvalView({ capabilityId: "design.applyOperations" }, undefined)).toBeNull();
  });
});

const ev = (sequence: number, type: string, payload: Record<string, unknown> = {}, sessionId: string | undefined = SESSION): DurableEvent => ({
  sequence,
  type,
  ...(sessionId === undefined ? {} : { sessionId }),
  payload,
});

const fold = (state: ChatState, ...events: DurableEvent[]) => chatReducer(state, { type: "events", events });

describe("the transcript keeps the five kinds of line apart", () => {
  it("agent output, capability execution, its result, an approval request and a navigation", () => {
    const state = fold(
      emptyChat,
      ev(1, "session.created"),
      ev(2, "session.started"),
      ev(3, "turn.started"),
      ev(4, "agent.output.completed", { text: "Looking at your design." }),
      ev(5, "capability.requested", { invocationId: "i1", capabilityId: "design.get", input: { design_id: "d" } }),
      ev(6, "capability.completed", { invocationId: "i1", capabilityId: "design.get", result: {} }),
      ev(7, "capability.requested", { invocationId: "i2", capabilityId: "ui.selectLayer", input: { layer_id: "l1" } }),
      ev(8, "capability.completed", { invocationId: "i2", capabilityId: "ui.selectLayer", result: { outcome: "handled", detail: "Headline" } }),
      ev(9, "approval.requested", { approvalId: "a1", invocationId: "i3", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit", importantArguments: [], affectedResourceCount: 1 }),
    );
    expect(state.lines.map((line) => line.kind)).toEqual(["agent", "capability", "result", "navigation", "approval"]);
    expect(state.lines[1]).toMatchObject({ kind: "capability", status: "done" });
    expect(state.lines[3]).toMatchObject({ kind: "navigation", status: "done", text: expect.stringContaining("Headline") as string });
    expect(state.working).toBe(true);
    expect(pendingApprovals(state)).toHaveLength(1);
  });

  it("a failed capability is a failed result carrying the reason", () => {
    const state = fold(
      emptyChat,
      ev(1, "session.created"),
      ev(2, "capability.requested", { invocationId: "i1", capabilityId: "design.validate", input: {} }),
      ev(3, "capability.failed", { invocationId: "i1", capabilityId: "design.validate", error: { code: "x", message: "timed out" } }),
    );
    expect(state.lines[0]).toMatchObject({ kind: "capability", status: "failed" });
    expect(state.lines[1]).toMatchObject({ kind: "result", ok: false, text: "Failed: timed out" });
  });

  it("a navigation that no window handled says so on its own line", () => {
    const state = fold(
      emptyChat,
      ev(1, "session.created"),
      ev(2, "capability.requested", { invocationId: "i1", capabilityId: "ui.setZoom", input: { zoom: 2 } }),
      ev(3, "capability.failed", { invocationId: "i1", capabilityId: "ui.setZoom", error: { code: "ui_unavailable", message: "no window" } }),
    );
    expect(state.lines).toHaveLength(1);
    expect(state.lines[0]).toMatchObject({ kind: "navigation", status: "failed", text: expect.stringContaining("ui_unavailable") as string });
  });

  it("an approval is resolved by its event, and a cancelled one is no longer pending", () => {
    const base = fold(
      emptyChat,
      ev(1, "session.created"),
      ev(2, "approval.requested", { approvalId: "a1", invocationId: "i", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit" }),
      ev(3, "approval.requested", { approvalId: "a2", invocationId: "j", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit" }),
    );
    const next = fold(base, ev(4, "approval.resolved", { approvalId: "a1", status: "approved" }), ev(5, "approval.cancelled", { approvalId: "a2" }));
    expect(pendingApprovals(next)).toHaveLength(0);
    expect(next.lines.map((line) => (line.kind === "approval" ? line.status : null))).toEqual(["approved", "cancelled"]);
  });

  it("failures end the turn and say why", () => {
    const state = fold(emptyChat, ev(1, "session.created"), ev(2, "turn.started"), ev(3, "turn.failed", { error: { code: "provider_exited", message: "the provider exited" } }));
    expect(state.working).toBe(false);
    expect(state.lines.at(-1)).toMatchObject({ kind: "notice", tone: "error", text: expect.stringContaining("the provider exited") as string });
  });

  it("ignores a type it does not know", () => {
    const state = fold(emptyChat, ev(1, "session.created"), ev(2, "something.new", { text: "x" }));
    expect(state.lines).toEqual([]);
  });

  it("a reload replays to the latest session, not the first", () => {
    const state = fold(
      emptyChat,
      ev(1, "session.created", {}, "old"),
      ev(2, "agent.output.completed", { text: "from before" }, "old"),
      ev(3, "session.created", {}, "new"),
      ev(4, "agent.output.completed", { text: "the open one" }, "new"),
    );
    expect(state.session?.id).toBe("new");
    expect(state.lines).toEqual([{ kind: "agent", id: "e4", text: "the open one" }]);
  });

  it("does not let a replay of earlier sessions displace the one this window started", () => {
    let state = chatReducer(emptyChat, { type: "person", id: "l1", text: "hello" });
    state = chatReducer(state, { type: "started", sessionId: "mine" });
    state = fold(state, ev(1, "session.created", {}, "old"), ev(2, "agent.output.completed", { text: "stale" }, "old"), ev(3, "session.created", {}, "mine"), ev(4, "session.started", {}, "mine"));
    expect(state.session).toEqual({ id: "mine", status: "live" });
    expect(state.lines).toEqual([{ kind: "person", id: "l1", text: "hello" }]);
  });

  it("stopping ends the session on the person's word without waiting for a terminal event", () => {
    let state = fold(emptyChat, ev(1, "session.created"), ev(2, "session.started"), ev(3, "turn.started"));
    state = chatReducer(state, { type: "stopped", id: "l2" });
    expect(state.working).toBe(false);
    expect(state.session?.status).toBe("ended");
  });
});
