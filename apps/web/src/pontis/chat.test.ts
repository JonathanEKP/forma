/**
 * The chat's logic: what the four intents send, how events become the five kinds
 * of line, and what an approval card is made of. The routes themselves are the
 * sidecar's (`apps/pontis-sidecar/src/intents.ts`); what is checked here is that
 * the bodies are exactly the ones it rebuilds from.
 */
import { describe, expect, it, vi } from "vitest";

import {
  agentWrites,
  approvalView,
  chatReducer,
  createAgentClient,
  DEAD,
  emptyChat,
  pendingApprovals,
  remember,
  runtimeView,
  splitViewLine,
  type ChatState,
} from "./chat";
import type { DurableEvent } from "./events";

const SESSION = "ses_1";

/** A chat attached to `SESSION`, as a window is once it has started one or been reattached to it. */
const mine = (): ChatState => chatReducer(emptyChat, { type: "restore", remembered: { sessionId: SESSION, deviceId: null, dead: null, said: [] } });

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
      deviceId: "dev_1",
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

  it("offers a provider whose sign-in cannot be known, and says it was not verified", () => {
    // Pontis's Codex probe reports authenticated:false with authMode:"unknown" for every Codex.
    const view = runtimeView({ devices: [device({ capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: false, authMode: "unknown" }] })] });
    expect(view).toMatchObject({ at: "online", provider: { id: "codex", authMode: "unknown", usable: true, signInNotVerified: true } });
    expect(view).not.toMatchObject({ provider: { unusableBecause: expect.anything() as unknown } });
  });

  it("still refuses a provider that is not installed, not compatible, or known not to be signed in", () => {
    const provider = (overrides: Record<string, unknown>) =>
      runtimeView({ devices: [device({ capabilities: [{ providerId: "codex", installed: true, compatible: true, authenticated: false, authMode: "unknown", ...overrides }] })] });
    expect(provider({ installed: false })).toMatchObject({ provider: { usable: false, unusableBecause: "not installed on this device" } });
    expect(provider({ compatible: false })).toMatchObject({ provider: { usable: false } });
    expect(provider({ authMode: "not_authenticated" })).toMatchObject({ provider: { usable: false, unusableBecause: "not signed in on this device" } });
  });

  it("distinguishes offline, ambiguous, revoked and none", () => {
    expect(runtimeView({ devices: [device({ online: false })] })).toEqual({ at: "offline", name: "Studio laptop" });
    expect(runtimeView({ devices: [device(), device({ id: "dev_2" })] })).toEqual({ at: "ambiguous" });
    // Revoked is not offline (F-10-4): it never comes back, so the advice differs.
    expect(runtimeView({ devices: [device({ revokedAt: "2026-10-01T00:00:00Z" })] })).toEqual({ at: "revoked", name: "Studio laptop" });
    expect(runtimeView({ devices: [] })).toEqual({ at: "none" });
  });

  it("does not let a revoked device speak for a live one", () => {
    const revoked = device({ id: "dev_old", revokedAt: "2026-10-01T00:00:00Z" });
    expect(runtimeView({ devices: [revoked, device({ online: false })] })).toEqual({ at: "offline", name: "Studio laptop" });
    expect(runtimeView({ devices: [revoked, device()] })).toMatchObject({ at: "online", deviceId: "dev_1" });
  });

  it("reports the newest revoked device when the others are older and offline, independent of list order", () => {
    const older = device({ id: "dev_old", name: "Old laptop", online: false, pairedAt: "2026-10-01T00:00:00Z" });
    const newest = device({ id: "dev_new", name: "New laptop", online: false, revokedAt: "2026-10-03T00:00:00Z", pairedAt: "2026-10-03T00:00:00Z" });
    const expected = { at: "revoked", name: "New laptop" };
    expect(runtimeView({ devices: [older, newest] })).toEqual(expected);
    expect(runtimeView({ devices: [newest, older] })).toEqual(expected);
  });

  it("reports the newest offline device when the revoked device is older, independent of list order", () => {
    const older = device({ id: "dev_old", name: "Old laptop", online: false, revokedAt: "2026-10-01T00:00:00Z", pairedAt: "2026-10-01T00:00:00Z" });
    const newest = device({ id: "dev_new", name: "New laptop", online: false, pairedAt: "2026-10-03T00:00:00Z" });
    const expected = { at: "offline", name: "New laptop" };
    expect(runtimeView({ devices: [older, newest] })).toEqual(expected);
    expect(runtimeView({ devices: [newest, older] })).toEqual(expected);
  });

  it("uses last seen time when pairing time is unavailable", () => {
    const older = device({ id: "dev_old", name: "Old laptop", online: false, pairedAt: "2026-10-01T00:00:00Z" });
    const newest = device({ id: "dev_new", name: "New laptop", online: false, revokedAt: "2026-10-03T00:00:00Z", lastSeenAt: "2026-10-03T00:00:00Z" });
    const expected = { at: "revoked", name: "New laptop" };
    expect(runtimeView({ devices: [older, newest] })).toEqual(expected);
    expect(runtimeView({ devices: [newest, older] })).toEqual(expected);
  });

  it("keeps the session device's state when another device is online or the session device is missing", () => {
    const sessionDevice = device({ id: "dev_session", name: "Session laptop", online: false, revokedAt: "2026-10-03T00:00:00Z" });
    const other = device({ id: "dev_other", name: "Other laptop" });
    expect(runtimeView({ devices: [other, sessionDevice] }, "dev_session")).toMatchObject({
      at: "online",
      deviceId: "dev_other",
      sessionDevice: { at: "revoked", name: "Session laptop" },
    });
    expect(runtimeView({ devices: [other] }, "dev_session")).toMatchObject({
      at: "online",
      sessionDevice: { at: "missing", name: "your device" },
    });
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
      mine(),
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
      mine(),
      ev(1, "session.created"),
      ev(2, "capability.requested", { invocationId: "i1", capabilityId: "design.validate", input: {} }),
      ev(3, "capability.failed", { invocationId: "i1", capabilityId: "design.validate", error: { code: "x", message: "timed out" } }),
    );
    expect(state.lines[0]).toMatchObject({ kind: "capability", status: "failed" });
    expect(state.lines[1]).toMatchObject({ kind: "result", ok: false, text: "Failed: timed out" });
  });

  it("a navigation that no window handled says so on its own line", () => {
    const state = fold(
      mine(),
      ev(1, "session.created"),
      ev(2, "capability.requested", { invocationId: "i1", capabilityId: "ui.setZoom", input: { zoom: 2 } }),
      ev(3, "capability.failed", { invocationId: "i1", capabilityId: "ui.setZoom", error: { code: "ui_unavailable", message: "no window" } }),
    );
    expect(state.lines).toHaveLength(1);
    expect(state.lines[0]).toMatchObject({ kind: "navigation", status: "failed", text: expect.stringContaining("ui_unavailable") as string });
  });

  it("an approval is resolved by its event, and a cancelled one is no longer pending", () => {
    const base = fold(
      mine(),
      ev(1, "session.created"),
      ev(2, "approval.requested", { approvalId: "a1", invocationId: "i", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit" }),
      ev(3, "approval.requested", { approvalId: "a2", invocationId: "j", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit" }),
    );
    const next = fold(base, ev(4, "approval.resolved", { approvalId: "a1", status: "approved" }), ev(5, "approval.cancelled", { approvalId: "a2" }));
    expect(pendingApprovals(next)).toHaveLength(0);
    expect(next.lines.map((line) => (line.kind === "approval" ? line.status : null))).toEqual(["approved", "cancelled"]);
  });

  it("failures end the turn and say why", () => {
    const state = fold(mine(), ev(1, "session.created"), ev(2, "turn.started"), ev(3, "turn.failed", { error: { code: "provider_exited", message: "the provider exited" } }));
    expect(state.working).toBe(false);
    expect(state.lines.at(-1)).toMatchObject({ kind: "notice", tone: "error", text: expect.stringContaining("the provider exited") as string });
  });

  it("ignores a type it does not know", () => {
    const state = fold(mine(), ev(1, "session.created"), ev(2, "something.new", { text: "x" }));
    expect(state.lines).toEqual([]);
  });

  it("shows nothing of a session this window did not start, and is not displaced by one", () => {
    const state = fold(
      emptyChat,
      ev(1, "session.created", {}, "someone-elses"),
      ev(2, "agent.output.completed", { text: "not here" }, "someone-elses"),
      ev(3, "approval.requested", { approvalId: "a", invocationId: "i", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit" }, "someone-elses"),
    );
    expect(state.session).toBeNull();
    expect(state.lines).toEqual([]);
    expect(pendingApprovals(state)).toHaveLength(0);
  });

  it("does not let a replay of earlier sessions displace the one this window started", () => {
    let state = chatReducer(emptyChat, { type: "person", id: "l1", text: "hello" });
    state = chatReducer(state, { type: "started", sessionId: "mine", id: "l2" });
    state = fold(state, ev(1, "session.created", {}, "old"), ev(2, "agent.output.completed", { text: "stale" }, "old"), ev(3, "session.created", {}, "mine"), ev(4, "session.started", {}, "mine"));
    expect(state.session).toEqual({ id: "mine", status: "live" });
    expect(state.lines).toEqual([{ kind: "person", id: "l1", text: "hello" }]);
  });

  it("stopping ends the session on the person's word without waiting for a terminal event", () => {
    let state = fold(mine(), ev(1, "session.created"), ev(2, "session.started"), ev(3, "turn.started"));
    state = chatReducer(state, { type: "stopped", id: "l2" });
    expect(state.working).toBe(false);
    expect(state.session?.status).toBe("ended");
  });
});

describe("the reply as it streams", () => {
  const live = (state: ChatState, text: string, turnId = "t1", sessionId = SESSION) => chatReducer(state, { type: "delta", sessionId, turnId, text });
  const open = () => fold(mine(), ev(1, "session.created"), ev(2, "session.started"), ev(3, "turn.started", { turnId: "t1" }));

  it("accumulates deltas for the turn, transiently, and is working while it does", () => {
    const state = live(live(open(), "Raising "), "the headline");
    expect(state.streaming).toEqual({ turnId: "t1", text: "Raising the headline" });
    expect(state.working).toBe(true);
    expect(state.lines).toEqual([]);
  });

  it("is replaced, not extended, by the completed text — the final text is the whole of it", () => {
    const state = fold(live(live(open(), "Raising "), "the head"), ev(4, "agent.output.completed", { turnId: "t1", text: "Raising the headline to 96pt." }));
    expect(state.streaming).toBeNull();
    expect(state.lines).toEqual([{ kind: "agent", id: "e4", text: "Raising the headline to 96pt." }]);
  });

  it("ignores a delta that lands after its own completion, which the two routes allow", () => {
    const state = live(fold(open(), ev(4, "agent.output.completed", { turnId: "t1", text: "Done." })), " Done.");
    expect(state.streaming).toBeNull();
    expect(state.lines).toHaveLength(1);
  });

  it("starts the next turn's text fresh", () => {
    const state = live(fold(live(open(), "one"), ev(4, "agent.output.completed", { turnId: "t1", text: "one" })), "two", "t2");
    expect(state.streaming).toEqual({ turnId: "t2", text: "two" });
  });

  it("drops what never completed when the turn ends, fails, is cancelled or is stopped", () => {
    for (const end of [ev(4, "turn.completed"), ev(4, "turn.failed", { error: { message: "x" } }), ev(4, "turn.cancelled"), ev(4, "session.cancelled")]) {
      expect(fold(live(open(), "half"), end).streaming).toBeNull();
    }
    expect(chatReducer(live(open(), "half"), { type: "stopped", id: "l" }).streaming).toBeNull();
  });

  it("is for the open session only", () => {
    expect(live(open(), "x", "t1", "another").streaming).toBeNull();
    expect(live(emptyChat, "x").streaming).toBeNull();
    expect(live(fold(open(), ev(4, "session.completed")), "late").streaming).toBeNull();
  });
});

const REPLY = (sequence: number, text: string, sessionId = SESSION) => ev(sequence, "agent.output.completed", { text }, sessionId);

describe("what the person said", () => {
  it("is shown as typed, with the chip of where they were", () => {
    const chip = { design: "ALPHA Launch cover", page: "Page 1", layer: "Headline" };
    const state = chatReducer(emptyChat, { type: "person", id: "l1", text: "make this bigger", chip });
    expect(state.lines).toEqual([{ kind: "person", id: "l1", text: "make this bigger", chip }]);
  });

  it("is placed back among the events it came after when the history is replayed after a reload", () => {
    let live = chatReducer(emptyChat, { type: "started", sessionId: SESSION, id: "l0" });
    live = fold(live, ev(1, "session.created"), ev(2, "session.started"));
    live = chatReducer(live, { type: "person", id: "l1", text: "first", chip: { design: "Cover" } });
    live = fold(live, ev(3, "turn.started"), REPLY(4, "one"), ev(5, "turn.completed"));
    live = chatReducer(live, { type: "person", id: "l2", text: "second" });
    live = fold(live, ev(6, "turn.started"), REPLY(7, "two"), ev(8, "turn.completed"));

    // What the tab keeps, and the page that replays after the reload.
    const kept = remember(live);
    expect(kept?.said.map(({ text, afterSeq }) => [text, afterSeq])).toEqual([["first", 2], ["second", 5]]);
    let reloaded = chatReducer(emptyChat, { type: "restore", remembered: kept! });
    reloaded = fold(reloaded, ev(1, "session.created"), ev(2, "session.started"), ev(3, "turn.started"), REPLY(4, "one"), ev(5, "turn.completed"), ev(6, "turn.started"), REPLY(7, "two"), ev(8, "turn.completed"));
    expect(reloaded.lines.map((line) => (line.kind === "person" || line.kind === "agent" ? `${line.kind}:${line.text}` : line.kind))).toEqual([
      "person:first",
      "agent:one",
      "person:second",
      "agent:two",
    ]);
    expect(reloaded.lines[0]).toMatchObject({ chip: { design: "Cover" } });
  });

  it("keeps a message sent while the session was still starting with that session", () => {
    let state = chatReducer(emptyChat, { type: "person", id: "l1", text: "hello" });
    state = chatReducer(state, { type: "started", sessionId: "mine", id: "l2" });
    expect(remember(state)?.said).toEqual([{ id: "l1", text: "hello", afterSeq: 0, sessionId: "mine" }]);
    expect(state.lines).toEqual([{ kind: "person", id: "l1", text: "hello" }]);
  });

  it("takes the sidecar's line off text that carries it, and shows it as the chip", () => {
    expect(splitViewLine('[forma-view] project=p design=d name="ALPHA \\"cover\\"" page=pg layer=ly\nmake it bigger')).toEqual({
      text: "make it bigger",
      chip: { design: 'ALPHA "cover"', page: "page", layer: "layer" },
    });
    expect(splitViewLine("[forma-view] none\nhello")).toEqual({ text: "hello" });
    expect(splitViewLine("hello\n[forma-view] design=x")).toEqual({ text: "hello\n[forma-view] design=x" });
  });
});

describe("starting a session", () => {
  it("keeps the person's first line when the Runtime's events arrive before the sidecar's answer (F-chat-1)", () => {
    let state = chatReducer(emptyChat, { type: "person", id: "l1", text: "hello" });
    state = chatReducer(state, { type: "starting" });
    // The live stream is quicker than the HTTP answer: `session.created` and `started` come first.
    state = fold(state, ev(1, "session.created", {}, "mine"), ev(2, "session.started", {}, "mine"));
    expect(state.lines).toEqual([{ kind: "person", id: "l1", text: "hello" }]);
    expect(state.session).toBeNull();
    state = chatReducer(state, { type: "started", sessionId: "mine", id: "l2" });
    expect(state.lines).toEqual([{ kind: "person", id: "l1", text: "hello" }]);
    expect(state.session).toEqual({ id: "mine", status: "live" });
    expect(state.owned).toBe("mine");
  });

  it("drops what another session said in the meantime, rather than showing it as this one's", () => {
    let state = chatReducer(emptyChat, { type: "starting" });
    state = fold(state, ev(1, "session.created", {}, "elsewhere"), ev(2, "agent.output.completed", { text: "not mine" }, "elsewhere"), ev(3, "session.created", {}, "mine"));
    state = chatReducer(state, { type: "started", sessionId: "mine", id: "l1" });
    expect(state.lines).toEqual([]);
    expect(state.session?.id).toBe("mine");
  });

  it("forgets what it heard when nothing was started after all: that session was never this window's", () => {
    let state = chatReducer(emptyChat, { type: "starting" });
    state = fold(state, ev(1, "session.created", {}, "latest"), ev(2, "agent.output.completed", { text: "hi" }, "latest"));
    state = chatReducer(state, { type: "startFailed" });
    expect(state.session).toBeNull();
    expect(state.lines).toEqual([]);
    expect(state.held).toEqual([]);
  });
});

describe("a session that cannot be carried on", () => {
  const waiting = () =>
    fold(
      chatReducer(emptyChat, { type: "started", sessionId: SESSION, id: "l0" }),
      ev(1, "session.created"),
      ev(2, "session.started"),
      ev(3, "turn.started"),
      ev(4, "capability.requested", { invocationId: "i", capabilityId: "design.validate", input: {} }),
      ev(5, "approval.requested", { approvalId: "a1", invocationId: "j", capabilityId: "design.applyOperations", risk: "destructive", humanSummary: "Remove" }),
    );

  it("leaves the working state, and turns a pending approval into an expired one with no way to answer it", () => {
    const state = chatReducer(waiting(), { type: "dead", reason: DEAD.revoked });
    expect(state.dead).toEqual({ reason: DEAD.revoked });
    expect(state.working).toBe(false);
    expect(state.session?.status).toBe("ended");
    expect(pendingApprovals(state)).toHaveLength(0);
    expect(state.lines.filter((line) => line.kind === "approval")).toMatchObject([{ status: "expired" }]);
    // A call that was still running when it ended never finished, and no longer says it is running.
    expect(state.lines.filter((line) => line.kind === "capability")).toMatchObject([{ status: "failed" }]);
  });

  it("is not brought back by what the session says afterwards", () => {
    const state = fold(chatReducer(waiting(), { type: "dead", reason: DEAD.offline }), ev(6, "session.started"), ev(7, "turn.started"), ev(8, "approval.requested", { approvalId: "a2", invocationId: "k", capabilityId: "design.applyOperations", risk: "write", humanSummary: "Edit" }));
    expect(state.working).toBe(false);
    expect(state.session?.status).toBe("ended");
    expect(pendingApprovals(state)).toHaveLength(0);
  });

  it("is what a failed, completed or remotely stopped session becomes, each with what to do next", () => {
    const base = fold(mine(), ev(1, "session.created"), ev(2, "session.started"));
    expect(fold(base, ev(3, "session.failed", { error: { message: "the provider exited" } })).dead?.reason).toMatch(/failed: the provider exited.*new one/);
    expect(fold(base, ev(3, "session.completed")).dead?.reason).toMatch(/ended.*new one/);
    expect(fold(base, ev(3, "session.cancelled")).dead?.reason).toMatch(/another window/);
    // This window's own Stop already said so.
    expect(fold(chatReducer(base, { type: "stopped", id: "l1" }), ev(3, "session.cancelled")).dead).toBeNull();
  });

  it("is remembered, so a reload does not forget that the session is over", () => {
    const kept = remember(chatReducer(waiting(), { type: "dead", reason: DEAD.notAuthorized }));
    expect(kept?.dead).toBe(DEAD.notAuthorized);
    const reloaded = chatReducer(emptyChat, { type: "restore", remembered: kept! });
    expect(reloaded.dead).toEqual({ reason: DEAD.notAuthorized });
    expect(reloaded.session?.id).toBe(SESSION);
  });

  it("keeps the old history above a divider when a new session starts, and expires what was waiting", () => {
    const dead = chatReducer(fold(waiting(), REPLY(6, "Working on it.")), { type: "dead", reason: DEAD.replaced });
    const next = chatReducer(dead, { type: "started", sessionId: "ses_2", id: "l9" });
    expect(next.dead).toBeNull();
    expect(next.session).toEqual({ id: "ses_2", status: "starting" });
    const kinds = next.lines.map((line) => line.kind);
    expect(kinds.at(-1)).toBe("divider");
    expect(kinds.slice(0, -1)).toEqual(expect.arrayContaining(["capability", "approval", "agent"]));
    expect(next.lines.filter((line) => line.kind === "approval")).toMatchObject([{ status: "expired" }]);
    // And no divider where there is nothing above it.
    expect(chatReducer(emptyChat, { type: "started", sessionId: "ses_2", id: "l9" }).lines).toEqual([]);
  });

  it("is what an expired delegation becomes: said as an expiry, with the way on (F-30-5)", () => {
    const base = fold(mine(), ev(1, "session.created"), ev(2, "session.started"), ev(3, "turn.started"), ev(4, "capability.requested", { invocationId: "i", capabilityId: "project.list", input: {} }));
    const state = fold(base, ev(5, "capability.failed", { invocationId: "i", capabilityId: "project.list", error: { category: "auth", code: "delegation_expired", message: "the remote application delegation has expired" } }));
    expect(state.dead).toEqual({ reason: DEAD.expired });
    expect(state.working).toBe(false);
    expect(DEAD.expired).toMatch(/expired.*Start a new session/);
    // Any other failure is just a failure: the session carries on.
    const other = fold(base, ev(5, "capability.failed", { invocationId: "i", capabilityId: "project.list", error: { code: "timeout", message: "slow" } }));
    expect(other.dead).toBeNull();
  });

  it("is what a window that did not press Stop learns from the call that Stop cancelled, since no terminal event follows (F-chat-3)", () => {
    const base = fold(mine(), ev(1, "session.created"), ev(2, "session.started"), ev(3, "turn.started"), ev(4, "capability.requested", { invocationId: "i", capabilityId: "design.validate", input: {} }));
    expect(base.working).toBe(true);
    const state = fold(base, ev(5, "capability.failed", { invocationId: "i", capabilityId: "design.validate", error: { code: "approval_cancelled", message: "cancelled" } }));
    expect(state.working).toBe(false);
    // Not a dead session: the Runtime is fine, the turn was just stopped.
    expect(state.dead).toBeNull();
    expect(state.session?.status).toBe("live");
  });

  it("learns which Device runs the session from the session's own events", () => {
    const attached = chatReducer(emptyChat, { type: "started", sessionId: SESSION, id: "l0" });
    const state = fold(attached, { ...ev(1, "session.created"), deviceId: "dev_1" }, { ...ev(2, "session.started"), deviceId: "dev_1" });
    expect(state.deviceId).toBe("dev_1");
    // Another session's events name another Device and say nothing about this one.
    expect(fold(state, { ...ev(3, "session.created", {}, "other"), deviceId: "dev_9" }).deviceId).toBe("dev_1");
  });
});

describe("the agent's writes, as the history reports them", () => {
  const done = (capabilityId: string, result: unknown, sequence = 1) => ev(sequence, "capability.completed", { invocationId: `i${sequence}`, capabilityId, result });

  it("names the design and version of each completed batch, and nothing else", () => {
    const writes = agentWrites([
      done("design.applyOperations", { design_id: "d1", project_id: "p1", version: 12, updated_at: "now" }),
      done("design.get", { id: "d1", version: 12 }, 2),
      done("design.applyOperations", { design_id: "d2", version: 3 }, 3),
      ev(4, "capability.failed", { invocationId: "i4", capabilityId: "design.applyOperations" }),
      done("design.applyOperations", "not an object", 5),
    ]);
    expect(writes).toEqual([
      { designId: "d1", projectId: "p1", version: 12 },
      { designId: "d2", projectId: null, version: 3 },
    ]);
  });
});
