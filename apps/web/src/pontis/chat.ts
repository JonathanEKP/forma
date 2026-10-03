/**
 * The agent chat, without the React: what is sent, how the answer folds into a
 * transcript, and what an approval card says (plan §11 step 10b).
 *
 * Out goes through the sidecar's `POST /agent/intent` (`apps/pontis-sidecar/src/
 * intents.ts`), which rebuilds every payload field by field — the session id is
 * minted there and the approver is the person — so the bodies here carry only
 * what the person decides. In comes through the durable event page of
 * `events.ts`. Nothing here depends on `@pontis/react` (plan D8).
 *
 * `pontis/docs/13` § Human visibility asks for five kinds of line a person can
 * tell apart: the agent's output, the application running a capability, an
 * approval request, the application's result, and a UI navigation. They are the
 * `Line` variants, because a transcript that renders them alike cannot be
 * debugged by the one reading it.
 */
import type { CapabilityRisk } from "@forma/bindings/CapabilityRisk";

import { capabilityCatalog } from "../../../../bindings/generated/capabilityCatalog";
import { isRecord, type DurableEvent } from "./events";

const INTENT_PATH = "/agent/intent";
const CONTEXT_PATH = "/agent/context";
const DEVICES_PATH = "/pons/v1/devices";

// ---------------------------------------------------------------------------
// Outbound: the four intents, and the two reads the chat needs besides.
// ---------------------------------------------------------------------------

export type Decision = "approveOnce" | "deny";

export type FailureReason =
  | "unreachable"
  | "runtime_offline"
  | "ambiguous_device"
  | "not_authorized"
  | "not_authenticated"
  | "not_delivered"
  | "unexpected";

export interface Failure {
  readonly reason: FailureReason;
  /** Said to the person; never the raw code. */
  readonly message: string;
}

export type Outcome<T> = { readonly ok: true; readonly value: T } | { readonly ok: false; readonly failure: Failure };

const MESSAGES: Record<FailureReason, string> = {
  unreachable: "Forma's agent service can't be reached.",
  runtime_offline: "Your Runtime is offline. Start it on your device, then try again.",
  ambiguous_device: "More than one of your devices is online, and Forma can't tell which one to use. Leave one connected.",
  not_authorized: "This session isn't allowed to do that.",
  not_authenticated: "Your sign-in has expired. Sign in again.",
  not_delivered: "The Runtime didn't take that message.",
  unexpected: "The agent service answered something unexpected.",
};

const failure = (reason: FailureReason): Outcome<never> => ({ ok: false, failure: { reason, message: MESSAGES[reason] } });

export interface AgentClientOptions {
  /** The session bearer, read per request: the provider refreshes it underneath. */
  readonly token: () => Promise<string | null>;
  /** Same-origin by construction. Only tests pass one. */
  readonly baseUrl?: string;
  readonly fetchImpl?: typeof fetch;
}

export interface AgentClient {
  /** Which application context the session is bound to — the event stream must name it. */
  context(): Promise<Outcome<string>>;
  devices(): Promise<Outcome<RuntimeView>>;
  start(providerId: string): Promise<Outcome<{ sessionId: string }>>;
  input(sessionId: string, text: string): Promise<Outcome<null>>;
  cancel(sessionId: string): Promise<Outcome<null>>;
  approval(approvalId: string, decision: Decision): Promise<Outcome<null>>;
}

export function createAgentClient(options: AgentClientOptions): AgentClient {
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);
  const base = options.baseUrl ?? "";

  const call = async (path: string, init: { method: "GET" | "POST"; body?: unknown }): Promise<Outcome<unknown>> => {
    const token = await options.token();
    let response: Response;
    try {
      response = await fetchImpl(`${base}${path}`, {
        method: init.method,
        headers: {
          ...(init.body === undefined ? {} : { "content-type": "application/json" }),
          ...(token ? { authorization: `Bearer ${token}` } : {}),
        },
        ...(init.body === undefined ? {} : { body: JSON.stringify(init.body) }),
      });
    } catch {
      return failure("unreachable");
    }
    const body: unknown = await response.json().catch(() => null);
    if (response.ok) return { ok: true, value: body };
    // The sidecar's refusals are `{ error: "<code>" }`. Anything that is not one
    // is a proxy answering for a sidecar that is not there.
    const code = isRecord(body) && typeof body.error === "string" ? body.error : null;
    if (code === null) return failure(response.status >= 500 ? "unreachable" : "unexpected");
    switch (code) {
      case "runtime_offline":
      case "ambiguous_device":
      case "not_authorized":
      case "not_authenticated":
        return failure(code);
      default:
        return failure("unexpected");
    }
  };

  const intent = (type: string, payload: Record<string, unknown>) => call(INTENT_PATH, { method: "POST", body: { type, payload } });

  /** An intent whose answer is `{ delivered }` and nothing the caller needs. */
  const delivered = async (type: string, payload: Record<string, unknown>): Promise<Outcome<null>> => {
    const answer = await intent(type, payload);
    if (!answer.ok) return answer;
    return isRecord(answer.value) && answer.value.delivered === true ? { ok: true, value: null } : failure("not_delivered");
  };

  return {
    async context() {
      const answer = await call(CONTEXT_PATH, { method: "GET" });
      if (!answer.ok) return answer;
      const id = isRecord(answer.value) ? answer.value.applicationContextId : undefined;
      return typeof id === "string" && id.length > 0 ? { ok: true, value: id } : failure("unexpected");
    },
    async devices() {
      const answer = await call(DEVICES_PATH, { method: "GET" });
      return answer.ok ? { ok: true, value: runtimeView(answer.value) } : answer;
    },
    async start(providerId) {
      const answer = await intent("session.start", { providerId });
      if (!answer.ok) return answer;
      if (!isRecord(answer.value) || answer.value.delivered !== true || typeof answer.value.sessionId !== "string") {
        return failure("not_delivered");
      }
      return { ok: true, value: { sessionId: answer.value.sessionId } };
    },
    input: (sessionId, text) => delivered("session.input", { sessionId, text }),
    cancel: (sessionId) => delivered("session.cancel", { sessionId }),
    approval: (approvalId, decision) => delivered("approval.resolve", { approvalId, decision }),
  };
}

// ---------------------------------------------------------------------------
// Runtime and provider status.
// ---------------------------------------------------------------------------

export interface ProviderView {
  readonly id: string;
  /** Verbatim: `docs/07` warns that `api_key` is a different bill from `subscription`. */
  readonly authMode: string;
  readonly usable: boolean;
  /** Why it cannot be used, when it cannot. */
  readonly unusableBecause?: string;
  /** Each of these is shown as the registry token it is, never softened (ADR-019). */
  readonly classification?: string;
  readonly sandbox?: string;
}

export type RuntimeView =
  | { readonly at: "online"; readonly name: string; readonly runtimeVersion: string; readonly provider: ProviderView | null }
  | { readonly at: "offline"; readonly name: string }
  | { readonly at: "ambiguous" }
  | { readonly at: "none" };

function providerView(raw: unknown): ProviderView | null {
  if (!isRecord(raw) || typeof raw.providerId !== "string") return null;
  const profile = isRecord(raw.securityProfile) ? raw.securityProfile : {};
  const authMode = typeof raw.authMode === "string" ? raw.authMode : "unknown";
  let unusableBecause: string | undefined;
  if (raw.installed !== true) unusableBecause = "not installed on this device";
  else if (raw.compatible !== true) unusableBecause = "this version is not one Forma supports";
  else if (raw.authenticated !== true) unusableBecause = "not signed in on this device";
  return {
    id: raw.providerId,
    authMode,
    usable: unusableBecause === undefined,
    ...(unusableBecause === undefined ? {} : { unusableBecause }),
    ...(typeof profile.classification === "string" ? { classification: profile.classification } : {}),
    ...(typeof profile.sandboxBoundary === "string" ? { sandbox: profile.sandboxBoundary } : {}),
  };
}

/**
 * Which device a message would go to, and with which provider.
 *
 * The sidecar's own rule is exactly one online device, and says `ambiguous_device`
 * otherwise; this mirrors it so the panel says so before a message is typed
 * rather than after. Of a device's providers the first usable one is shown and
 * used: the paired-agent path is Codex (plan §6.3), and nothing here offers a
 * choice the plan has pinned.
 */
export function runtimeView(body: unknown): RuntimeView {
  const devices = isRecord(body) && Array.isArray(body.devices) ? (body.devices as unknown[]) : [];
  const live = devices.filter(isRecord).filter((device) => typeof device.revokedAt !== "string");
  const online = live.filter((device) => device.online === true);
  if (online.length > 1) return { at: "ambiguous" };
  const [device] = online;
  if (!device) {
    const [known] = live;
    return known ? { at: "offline", name: typeof known.name === "string" ? known.name : "your device" } : { at: "none" };
  }
  const providers = (Array.isArray(device.capabilities) ? (device.capabilities as unknown[]) : [])
    .map(providerView)
    .filter((provider): provider is ProviderView => provider !== null);
  return {
    at: "online",
    name: typeof device.name === "string" ? device.name : "your device",
    runtimeVersion: typeof device.runtimeVersion === "string" ? device.runtimeVersion : "unknown",
    provider: providers.find((provider) => provider.usable) ?? providers[0] ?? null,
  };
}

// ---------------------------------------------------------------------------
// The approval card.
// ---------------------------------------------------------------------------

export interface ApprovalView {
  readonly approvalId: string;
  readonly capabilityId: string;
  /** The human-readable action, in Forma's terms. */
  readonly action: string;
  readonly parameters: readonly { readonly name: string; readonly value: string }[];
  readonly risk: string;
  /** `null` when neither the request nor the input says how many things are affected. */
  readonly affected: number | null;
  readonly expiresAt: string | null;
}

const spec = (capabilityId: string) => capabilityCatalog.find((entry) => entry.id === capabilityId);

const SHOWN = 140;

/** One parameter value, short enough for a card and honest about what it elided. */
export function describeValue(value: unknown): string {
  if (typeof value === "string") return value.length > SHOWN ? `${value.slice(0, SHOWN)}…` : value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) {
    const items = value as unknown[];
    // A batch of operations is read by what it does, not by its length alone.
    const kinds = items.map((item) => (isRecord(item) && typeof item.op === "string" ? item.op : null));
    if (items.length > 0 && kinds.every((kind) => kind !== null)) {
      return `${items.length} ${items.length === 1 ? "operation" : "operations"}: ${kinds.join(", ")}`;
    }
    return `${items.length} ${items.length === 1 ? "item" : "items"}`;
  }
  if (value === null || value === undefined) return "none";
  const text = JSON.stringify(value);
  return text.length > SHOWN ? `${text.slice(0, SHOWN)}…` : text;
}

/**
 * What a person is shown before consenting (`pontis/docs/13` § Approval panel):
 * capability, human-readable action, important parameters, risk class and the
 * affected resource count.
 *
 * The request already carries these, built by Pontis from the capability's
 * presentation metadata. The catalog is the fallback for each, not a second
 * opinion: the request wins where it speaks, and where it is silent — an older
 * runtime, a field it left out — the card is rebuilt from the same `presentation`
 * and the input the agent sent, because "Allow tool?" is exactly what this
 * exists to avoid.
 */
export function approvalView(payload: Readonly<Record<string, unknown>>, input: unknown): ApprovalView | null {
  const { approvalId, capabilityId } = payload;
  if (typeof approvalId !== "string" || typeof capabilityId !== "string") return null;
  const presentation = spec(capabilityId)?.presentation;
  const given = Array.isArray(payload.importantArguments) ? (payload.importantArguments as unknown[]) : [];

  let parameters = given.filter(isRecord).flatMap((argument) =>
    typeof argument.name === "string" ? [{ name: argument.name, value: describeValue(argument.value) }] : [],
  );
  if (parameters.length === 0 && presentation && isRecord(input)) {
    parameters = presentation.importantArguments
      .filter((name) => name in input)
      .map((name) => ({ name, value: describeValue(input[name]) }));
  }

  let affected: number | null = typeof payload.affectedResourceCount === "number" ? payload.affectedResourceCount : null;
  if (affected === null && presentation?.affectedResourceCountFrom && isRecord(input)) {
    const counted = input[presentation.affectedResourceCountFrom];
    if (Array.isArray(counted)) affected = counted.length;
  }

  return {
    approvalId,
    capabilityId,
    action:
      typeof payload.humanSummary === "string" && payload.humanSummary.length > 0
        ? payload.humanSummary
        : (presentation?.summary ?? capabilityId),
    parameters,
    risk: typeof payload.risk === "string" ? payload.risk : (spec(capabilityId)?.risk ?? "unknown"),
    affected,
    expiresAt: typeof payload.expiresAt === "string" ? payload.expiresAt : null,
  };
}

// ---------------------------------------------------------------------------
// The transcript.
// ---------------------------------------------------------------------------

export type ApprovalStatus = "pending" | "approved" | "denied" | "expired" | "cancelled";

export type Line =
  /** What the person said. Echoed locally: no durable event carries it back. */
  | { readonly kind: "person"; readonly id: string; readonly text: string }
  /** The agent's own words. */
  | { readonly kind: "agent"; readonly id: string; readonly text: string }
  /** The application running a capability the agent asked for. */
  | { readonly kind: "capability"; readonly id: string; readonly invocationId: string; readonly capabilityId: string; readonly summary: string; readonly status: "running" | "done" | "failed" }
  /** What the application answered. */
  | { readonly kind: "result"; readonly id: string; readonly capabilityId: string; readonly ok: boolean; readonly text: string }
  /** A person is being asked to consent. */
  | { readonly kind: "approval"; readonly id: string; readonly approval: ApprovalView; readonly status: ApprovalStatus }
  /** The agent moved the person's view. */
  | { readonly kind: "navigation"; readonly id: string; readonly invocationId: string; readonly capabilityId: string; readonly text: string; readonly status: "running" | "done" | "failed" }
  /** The chat's own voice: a stop, a failure, the end of a session. */
  | { readonly kind: "notice"; readonly id: string; readonly tone: "info" | "error"; readonly text: string };

export type SessionStatus = "starting" | "live" | "ended";

export interface ChatState {
  readonly lines: readonly Line[];
  readonly session: { readonly id: string; readonly status: SessionStatus } | null;
  /**
   * The session this window started, if it has. Events of any other session are
   * ignored: a replay of an earlier conversation must not replace the one the
   * person just opened.
   */
  readonly owned: string | null;
  /** A turn is open: `turn.started` seen, no terminal event yet. */
  readonly working: boolean;
  /** What each invocation asked with, for rebuilding an approval card the request left bare. */
  readonly inputs: Readonly<Record<string, unknown>>;
  /**
   * The agent's reply as it arrives, before it is final.
   *
   * Transient by construction: a delta is never stored, so this is never rebuilt
   * from history and never outlives its turn. `agent.output.completed` carries the
   * whole authoritative text and replaces it — it is not appended to it.
   */
  readonly streaming: { readonly turnId: string; readonly text: string } | null;
  /**
   * Turns whose final output has been seen. The two arrive by different routes, so
   * a delta can land after its own completion, and without this it would bring
   * the finished reply back as a second, half-sized one.
   */
  readonly completed: readonly string[];
}

export const emptyChat: ChatState = { lines: [], session: null, owned: null, working: false, inputs: {}, streaming: null, completed: [] };

export type ChatAction =
  | { readonly type: "events"; readonly events: readonly DurableEvent[] }
  /** An `agent.output.delta` from the live stream: unsequenced, never stored. */
  | { readonly type: "delta"; readonly sessionId: string; readonly turnId: string; readonly text: string }
  | { readonly type: "person"; readonly id: string; readonly text: string }
  /** This window started a session, and the sidecar has answered with its id. */
  | { readonly type: "started"; readonly sessionId: string }
  | { readonly type: "notice"; readonly id: string; readonly tone: "info" | "error"; readonly text: string }
  | { readonly type: "approval"; readonly approvalId: string; readonly status: ApprovalStatus }
  /** The person pressed stop. The Runtime may take a moment to confirm, or never will (see `reduce`). */
  | { readonly type: "stopped"; readonly id: string };

export function chatReducer(state: ChatState, action: ChatAction): ChatState {
  switch (action.type) {
    case "events":
      return action.events.reduce(applyEvent, state);
    case "delta":
      return applyDelta(state, action.sessionId, action.turnId, action.text);
    case "person":
      return { ...state, lines: [...state.lines, { kind: "person", id: action.id, text: action.text }] };
    case "started":
      return { ...emptyChat, lines: state.lines, session: { id: action.sessionId, status: "starting" }, owned: action.sessionId };
    case "notice":
      return { ...state, lines: [...state.lines, { kind: "notice", id: action.id, tone: action.tone, text: action.text }] };
    case "approval":
      return { ...state, lines: setApproval(state.lines, action.approvalId, action.status) };
    case "stopped":
      // Ended on the person's word, not the Runtime's. The Runtime does not
      // always write a terminal event for a cancelled turn (the Pontis finding
      // recorded in local-run.test.ts), so waiting for one would leave the box
      // saying "working" over a session nobody is running.
      return {
        ...state,
        working: false,
        streaming: null,
        session: state.session ? { ...state.session, status: "ended" } : null,
        lines: [...state.lines, { kind: "notice", id: action.id, tone: "info", text: "You stopped the agent." }],
      };
  }
}

const isUi = (capabilityId: string) => capabilityId.startsWith("ui.");

function applyDelta(state: ChatState, sessionId: string, turnId: string, text: string): ChatState {
  if (state.session?.id !== sessionId || state.session.status === "ended") return state;
  if (state.completed.includes(turnId)) return state;
  const streaming = state.streaming?.turnId === turnId ? { turnId, text: state.streaming.text + text } : { turnId, text };
  return { ...state, working: true, streaming };
}

function setApproval(lines: readonly Line[], approvalId: string, status: ApprovalStatus): readonly Line[] {
  return lines.map((line) => (line.kind === "approval" && line.approval.approvalId === approvalId ? { ...line, status } : line));
}

function errorText(error: unknown): string {
  return isRecord(error) && typeof error.message === "string" ? error.message : "no reason was given";
}

function applyEvent(state: ChatState, event: DurableEvent): ChatState {
  const { sessionId, payload } = event;
  const id = `e${event.sequence}`;

  if (event.type === "session.created" && sessionId !== undefined && sessionId !== state.session?.id && state.owned === null) {
    // The latest session's transcript replaces an earlier one, so a reload lands
    // on the conversation that was open rather than the first one ever had.
    state = { ...emptyChat, session: { id: sessionId, status: "starting" } };
  }
  if (sessionId === undefined || sessionId !== state.session?.id) return state;

  const status = (next: SessionStatus): ChatState => ({ ...state, session: { id: sessionId, status: next } });

  switch (event.type) {
    case "session.started":
      return state.session.status === "ended" ? state : status("live");
    case "session.completed":
      return { ...status("ended"), working: false, streaming: null, lines: [...state.lines, { kind: "notice", id, tone: "info", text: "The session ended." }] };
    case "session.cancelled":
      return { ...status("ended"), working: false, streaming: null };
    case "session.failed":
      return {
        ...status("ended"),
        working: false,
        streaming: null,
        lines: [...state.lines, { kind: "notice", id, tone: "error", text: `The session failed: ${errorText(payload.error)}` }],
      };
    case "turn.started":
      return { ...state, working: true };
    case "turn.completed":
    case "turn.cancelled":
      return { ...state, working: false, streaming: null };
    case "turn.failed":
      return { ...state, working: false, streaming: null, lines: [...state.lines, { kind: "notice", id, tone: "error", text: `The agent's turn failed: ${errorText(payload.error)}` }] };
    case "agent.output.completed": {
      // Replaces whatever was streaming for the turn: the final text is the whole
      // of it, so appending would say everything twice.
      const turnId = typeof payload.turnId === "string" ? payload.turnId : null;
      const settled: ChatState = {
        ...state,
        streaming: turnId === null || state.streaming?.turnId === turnId ? null : state.streaming,
        completed: turnId === null ? state.completed : [...state.completed, turnId],
      };
      return typeof payload.text === "string" && payload.text.length > 0
        ? { ...settled, lines: [...state.lines, { kind: "agent", id, text: payload.text }] }
        : settled;
    }
    case "capability.requested":
      return requested(state, id, payload);
    case "capability.completed":
    case "capability.failed":
      return answered(state, id, event.type === "capability.completed", payload);
    case "approval.requested": {
      const approval = approvalView(payload, typeof payload.invocationId === "string" ? state.inputs[payload.invocationId] : undefined);
      return approval ? { ...state, lines: [...state.lines, { kind: "approval", id, approval, status: "pending" }] } : state;
    }
    case "approval.resolved": {
      const resolved = payload.status === "approved" || payload.status === "denied" || payload.status === "expired" ? payload.status : null;
      return typeof payload.approvalId === "string" && resolved ? { ...state, lines: setApproval(state.lines, payload.approvalId, resolved) } : state;
    }
    case "approval.cancelled":
      return typeof payload.approvalId === "string" ? { ...state, lines: setApproval(state.lines, payload.approvalId, "cancelled") } : state;
    default:
      // An unrecognised type is skipped, not shown: a newer control plane may
      // emit something this build does not know, and raw JSON in a chat is worse
      // than nothing (`pontis/docs/08` § Client rules).
      return state;
  }
}

/** What a UI action asked for, in the words of the person it happened to. */
function navigationText(capabilityId: string, input: unknown): string {
  const fields = isRecord(input)
    ? Object.entries(input)
        .map(([name, value]) => `${name} ${describeValue(value)}`)
        .join(", ")
    : "";
  return fields ? `${capabilityId} — ${fields}` : capabilityId;
}

function requested(state: ChatState, id: string, payload: Readonly<Record<string, unknown>>): ChatState {
  const { invocationId, capabilityId } = payload;
  if (typeof invocationId !== "string" || typeof capabilityId !== "string") return state;
  const inputs = { ...state.inputs, [invocationId]: payload.input };
  const line: Line = isUi(capabilityId)
    ? { kind: "navigation", id, invocationId, capabilityId, text: navigationText(capabilityId, payload.input), status: "running" }
    : { kind: "capability", id, invocationId, capabilityId, summary: spec(capabilityId)?.presentation?.summary ?? `Runs ${capabilityId}`, status: "running" };
  return { ...state, inputs, lines: [...state.lines, line] };
}

function answered(state: ChatState, id: string, ok: boolean, payload: Readonly<Record<string, unknown>>): ChatState {
  const { invocationId, capabilityId } = payload;
  if (typeof invocationId !== "string" || typeof capabilityId !== "string") return state;
  const next: "done" | "failed" = ok ? "done" : "failed";
  const lines = state.lines.map((line) =>
    (line.kind === "capability" || line.kind === "navigation") && line.invocationId === invocationId ? { ...line, status: next } : line,
  );

  if (isUi(capabilityId)) {
    // Told apart from a domain result: the navigation line carries its own
    // outcome — the window's, or why there was none — instead of a second line.
    const detail = ok
      ? isRecord(payload.result) && typeof payload.result.detail === "string"
        ? payload.result.detail
        : "handled"
      : isRecord(payload.error) && typeof payload.error.code === "string"
        ? payload.error.code
        : "failed";
    return {
      ...state,
      lines: lines.map((line) => (line.kind === "navigation" && line.invocationId === invocationId ? { ...line, text: `${line.text} · ${detail}` } : line)),
    };
  }

  const text = ok ? "Completed" : `Failed: ${errorText(payload.error)}`;
  return { ...state, lines: [...lines, { kind: "result", id, capabilityId, ok, text }] };
}

/** Approvals still waiting on the person. */
export function pendingApprovals(state: ChatState): readonly Extract<Line, { kind: "approval" }>[] {
  return state.lines.filter((line): line is Extract<Line, { kind: "approval" }> => line.kind === "approval" && line.status === "pending");
}

/** The risk classes, for tests and the card: kept next to the generated type so a new one is a type error. */
export const RISK_LABEL: Record<CapabilityRisk, string> = {
  read: "read",
  write: "write",
  sensitive: "sensitive",
  destructive: "destructive",
  external_side_effect: "external side effect",
};
