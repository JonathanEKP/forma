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

export type NoticeTone = "info" | "needs-you" | "stopped";
export type NoticeAction = "New session" | "Sign in again" | "Open Agents page";

export interface NoticeCopy {
  readonly tone: NoticeTone;
  readonly happened: string;
  readonly next: string;
  readonly action?: NoticeAction;
}

export type FailureReason =
  | "unreachable"
  | "runtime_offline"
  | "device_choice_required"
  | "device_offline"
  | "not_authorized"
  | "not_authenticated"
  | "message_too_large"
  | "not_delivered"
  | "unexpected";

export interface Failure {
  readonly reason: FailureReason;
  /** Component-ready copy; never the raw code. */
  readonly notice: NoticeCopy;
  /** Short control hint for places that cannot render the full notice. */
  readonly message: string;
}

export type Outcome<T> = { readonly ok: true; readonly value: T } | { readonly ok: false; readonly failure: Failure };

const MESSAGES: Record<FailureReason, NoticeCopy> = {
  unreachable: { tone: "needs-you", happened: "Forma's agent service can't be reached.", next: "Check the connection, then try again." },
  runtime_offline: { tone: "needs-you", happened: "No computer is online.", next: "Start a paired computer, then try again.", action: "Open Agents page" },
  device_choice_required: { tone: "needs-you", happened: "Choose a computer to run the agent.", next: "Choose one from Run on before sending." },
  device_offline: { tone: "stopped", happened: "The computer running this session went offline.", next: "Bring it online, then start a new session.", action: "New session" },
  not_authorized: { tone: "stopped", happened: "This session can no longer reach its Runtime.", next: "Start a new session to carry on.", action: "New session" },
  not_authenticated: { tone: "needs-you", happened: "Your sign-in has expired.", next: "Sign in again, then start a new session.", action: "Sign in again" },
  message_too_large: { tone: "needs-you", happened: "That message is too long (max 64 KB).", next: "Shorten it, then try again." },
  not_delivered: { tone: "needs-you", happened: "The Runtime didn't take that message.", next: "Try sending it again when the Runtime is ready." },
  unexpected: { tone: "needs-you", happened: "The agent service answered unexpectedly.", next: "Try again in a moment." },
};

const failure = (reason: FailureReason): Outcome<never> => ({
  ok: false,
  failure: { reason, message: MESSAGES[reason].happened, notice: MESSAGES[reason] },
});

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
  devices(sessionDeviceId?: string | null): Promise<Outcome<RuntimeView>>;
  start(providerId: string, deviceId?: string): Promise<Outcome<{ sessionId: string }>>;
  /** `view` is what the editor shows; the sidecar turns it into the line in front of the text. */
  input(sessionId: string, text: string, view?: ViewIds): Promise<Outcome<null>>;
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
    const error = isRecord(body) ? body.error : undefined;
    const code = typeof error === "string" ? error : isRecord(error) && typeof error.code === "string" ? error.code : null;
    if (response.status === 401 && code === "unauthenticated") return failure("not_authenticated");
    if (code === null) return failure(response.status >= 500 ? "unreachable" : "unexpected");
    switch (code) {
      case "runtime_offline":
      case "device_choice_required":
      case "device_offline":
      case "not_authorized":
      case "not_authenticated":
        return failure(code);
      case "message_too_large":
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
    async devices(sessionDeviceId) {
      const answer = await call(DEVICES_PATH, { method: "GET" });
      return answer.ok ? { ok: true, value: runtimeView(answer.value, sessionDeviceId) } : answer;
    },
    async start(providerId, deviceId) {
      const answer = await intent("session.start", { providerId, ...(deviceId ? { deviceId } : {}) });
      if (!answer.ok) return answer;
      if (!isRecord(answer.value) || answer.value.delivered !== true || typeof answer.value.sessionId !== "string") {
        return failure("not_delivered");
      }
      return { ok: true, value: { sessionId: answer.value.sessionId } };
    },
    input: (sessionId, text, view) => delivered("session.input", { sessionId, text, ...(view && Object.keys(view).length > 0 ? { view } : {}) }),
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
  /**
   * The provider cannot say whether it is signed in (`authMode` `unknown`), so it is offered anyway.
   * Pontis's Codex probe reports exactly this for every installed Codex — it has no supported
   * auth-status query — and treating "cannot be known" as "not signed in" would make Codex
   * unreachable from the chat. A provider that is not signed in fails at `session.start` instead,
   * with the `not_authenticated` failure the chat already words.
   */
  readonly signInNotVerified?: true;
  /** Each of these is shown as the registry token it is, never softened (ADR-019). */
  readonly classification?: string;
  readonly sandbox?: string;
}

type RuntimeStatus =
  | { readonly at: "online"; readonly deviceId: string | null; readonly name: string; readonly runtimeVersion: string; readonly provider: ProviderView | null }
  | { readonly at: "offline"; readonly name: string }
  /** Revoked differs from offline: a revoked Runtime cannot come back, so the advice differs. */
  | { readonly at: "revoked"; readonly name: string }
  | { readonly at: "ambiguous"; readonly devices: readonly RuntimeDevice[] }
  | { readonly at: "none" };

export interface RuntimeDevice {
  readonly id: string;
  readonly name: string;
  readonly runtimeVersion: string;
  readonly provider: ProviderView | null;
  readonly presence: "Online";
}

export type RuntimeView = RuntimeStatus & {
  /** What happened to the device this window's session was using, if it is known. */
  readonly sessionDevice?: { readonly id: string; readonly at: "online" | "offline" | "revoked" | "missing"; readonly name: string };
};

function providerView(raw: unknown): ProviderView | null {
  if (!isRecord(raw) || typeof raw.providerId !== "string") return null;
  const profile = isRecord(raw.securityProfile) ? raw.securityProfile : {};
  const authMode = typeof raw.authMode === "string" ? raw.authMode : "unknown";
  let unusableBecause: string | undefined;
  if (raw.installed !== true) unusableBecause = "not installed on this device";
  else if (raw.compatible !== true) unusableBecause = "this version is not one Forma supports";
  else if (raw.authenticated !== true && authMode !== "unknown") unusableBecause = "not signed in on this device";
  return {
    id: raw.providerId,
    authMode,
    usable: unusableBecause === undefined,
    ...(unusableBecause === undefined ? {} : { unusableBecause }),
    ...(unusableBecause === undefined && raw.authenticated !== true ? { signInNotVerified: true as const } : {}),
    ...(typeof profile.classification === "string" ? { classification: profile.classification } : {}),
    ...(typeof profile.sandboxBoundary === "string" ? { sandbox: profile.sandboxBoundary } : {}),
  };
}

/**
 * Which device a message would go to, what status to show, and with which provider.
 *
 * The sidecar requires a named choice when several computers are online. A
 * session keeps the computer its Runtime started on, even if
 * another device is online. Without a session device, the newest pairing decides
 * which offline state to show; the server's list order carries no meaning.
 * Of a device's providers the first usable one is shown and used: the
 * paired-agent path is Codex (plan §6.3), and nothing here offers a choice the
 * plan has pinned.
 */
export function runtimeView(body: unknown, sessionDeviceId?: string | null): RuntimeView {
  const devices = isRecord(body) && Array.isArray(body.devices) ? (body.devices as unknown[]) : [];
  const known = devices.filter(isRecord);
  const live = known.filter((device) => typeof device.revokedAt !== "string");
  const online = live.filter((device) => device.online === true);
  const sessionDevice = sessionDeviceId === undefined || sessionDeviceId === null
    ? undefined
    : known.find((device) => device.id === sessionDeviceId);
  const sessionStatus: RuntimeView["sessionDevice"] =
    sessionDeviceId === undefined || sessionDeviceId === null
      ? undefined
      : sessionDevice === undefined
        ? { id: sessionDeviceId, at: "missing" as const, name: "your device" }
        : {
            id: sessionDeviceId,
            at: typeof sessionDevice.revokedAt === "string" ? "revoked" : sessionDevice.online === true ? "online" : "offline",
            name: typeof sessionDevice.name === "string" ? sessionDevice.name : "your device",
          };
  const withSession = (view: RuntimeStatus): RuntimeView =>
    sessionStatus === undefined ? view : { ...view, sessionDevice: sessionStatus };
  if (online.length > 1) {
    const choices = online.flatMap((candidate): RuntimeDevice[] => {
      if (typeof candidate.id !== "string") return [];
      const providers = (Array.isArray(candidate.capabilities) ? (candidate.capabilities as unknown[]) : [])
        .map(providerView)
        .filter((provider): provider is ProviderView => provider !== null);
      return [{
        id: candidate.id,
        name: deviceName(candidate),
        runtimeVersion: typeof candidate.runtimeVersion === "string" ? candidate.runtimeVersion : "unknown",
        provider: providers.find((provider) => provider.usable) ?? providers[0] ?? null,
        presence: "Online",
      }];
    });
    return withSession({ at: "ambiguous", devices: choices });
  }
  const [device] = online;
  if (!device) {
    if (sessionStatus?.at === "revoked" || sessionStatus?.at === "missing") {
      return withSession({ at: "revoked", name: sessionStatus.name });
    }
    if (sessionStatus?.at === "offline") return withSession({ at: "offline", name: sessionStatus.name });

    const latestLive = [...live].sort(newerDeviceFirst)[0];
    const latestRevoked = known.filter((candidate) => typeof candidate.revokedAt === "string").sort(newerDeviceFirst)[0];
    if (latestRevoked && (!latestLive || pairedTime(latestRevoked) > pairedTime(latestLive))) {
      return withSession({ at: "revoked", name: deviceName(latestRevoked) });
    }
    if (latestLive) return withSession({ at: "offline", name: deviceName(latestLive) });
    return latestRevoked ? withSession({ at: "revoked", name: deviceName(latestRevoked) }) : withSession({ at: "none" });
  }
  const providers = (Array.isArray(device.capabilities) ? (device.capabilities as unknown[]) : [])
    .map(providerView)
    .filter((provider): provider is ProviderView => provider !== null);
  const provider = providers.find((candidate) => candidate.usable) ?? providers[0] ?? null;
  return withSession({
    at: "online",
    deviceId: typeof device.id === "string" ? device.id : null,
    name: deviceName(device),
    runtimeVersion: typeof device.runtimeVersion === "string" ? device.runtimeVersion : "unknown",
    provider,
  });
}

function deviceName(device: Record<string, unknown>): string {
  return typeof device.name === "string" ? device.name : "your device";
}

function pairedTime(device: Record<string, unknown>): number {
  const pairedAt = typeof device.pairedAt === "string" ? Date.parse(device.pairedAt) : Number.NaN;
  if (Number.isFinite(pairedAt)) return pairedAt;
  const lastSeenAt = typeof device.lastSeenAt === "string" ? Date.parse(device.lastSeenAt) : Number.NaN;
  return Number.isFinite(lastSeenAt) ? lastSeenAt : Number.NEGATIVE_INFINITY;
}

function newerDeviceFirst(left: Record<string, unknown>, right: Record<string, unknown>): number {
  const leftTime = pairedTime(left);
  const rightTime = pairedTime(right);
  if (leftTime !== rightTime) return rightTime > leftTime ? 1 : -1;
  const leftId = typeof left.id === "string" ? left.id : "";
  const rightId = typeof right.id === "string" ? right.id : "";
  return leftId < rightId ? -1 : leftId > rightId ? 1 : 0;
}

// ---------------------------------------------------------------------------
// The approval card.
// ---------------------------------------------------------------------------

export interface ApprovalView {
  readonly approvalId: string;
  readonly invocationId: string | null;
  readonly capabilityId: string;
  /** The human-readable action, in Forma's terms. */
  readonly action: string;
  readonly parameters: readonly { readonly name: string; readonly value: string }[];
  readonly risk: string;
  /** `null` when neither the request nor the input says how many things are affected. */
  readonly affected: number | null;
  readonly expiresAt: string | null;
  /** Raw context retained for the editor to describe targets using its REST read. */
  readonly designId: string | null;
  readonly baseVersion: number | null;
  readonly operations: readonly unknown[];
  readonly reason: string | null;
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
  const rawArgument = (name: string): unknown => {
    const argument = given.find((entry) => isRecord(entry) && entry.name === name);
    if (isRecord(argument)) return argument.value;
    return isRecord(input) ? input[name] : undefined;
  };

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
    invocationId: typeof payload.invocationId === "string" ? payload.invocationId : null,
    capabilityId,
    action:
      typeof payload.humanSummary === "string" && payload.humanSummary.length > 0
        ? payload.humanSummary
        : (presentation?.summary ?? capabilityId),
    parameters,
    risk: typeof payload.risk === "string" ? payload.risk : (spec(capabilityId)?.risk ?? "unknown"),
    affected,
    expiresAt: typeof payload.expiresAt === "string" ? payload.expiresAt : null,
    designId: typeof rawArgument("design_id") === "string" ? rawArgument("design_id") as string : null,
    baseVersion: typeof rawArgument("base_version") === "number" ? rawArgument("base_version") as number : null,
    operations: Array.isArray(rawArgument("operations")) ? rawArgument("operations") as unknown[] : [],
    reason: typeof payload.humanReason === "string" ? payload.humanReason : typeof payload.reason === "string" ? payload.reason : null,
  };
}

// ---------------------------------------------------------------------------
// The transcript.
// ---------------------------------------------------------------------------

export type ApprovalStatus = "pending" | "approved" | "denied" | "expired" | "cancelled";

/** Where the person was when they said it, in the words they know: names, not ids. */
export interface ViewChip {
  readonly project?: string;
  readonly design?: string;
  readonly page?: string;
  readonly layer?: string;
}

/** What the editor reports with a message; the sidecar validates it and writes the line (`intents.ts`). */
export interface ViewIds {
  readonly projectId?: string;
  readonly designId?: string;
  readonly pageId?: string;
  readonly layerId?: string;
}

export type Line =
  /**
   * What the person said, as typed. Echoed locally: no durable event carries it
   * back, so the window keeps it (see `said`). The chip is the context that went
   * with it.
   */
  | { readonly kind: "person"; readonly id: string; readonly text: string; readonly chip?: ViewChip }
  /** Where an earlier session's history ends and a new one begins. */
  | { readonly kind: "divider"; readonly id: string; readonly text: string }
  /** The agent's own words. */
  | { readonly kind: "agent"; readonly id: string; readonly text: string }
  /** The application running a capability the agent asked for. */
  | { readonly kind: "capability"; readonly id: string; readonly invocationId: string; readonly capabilityId: string; readonly summary: string; readonly status: "running" | "done" | "failed" | "not_run" }
  /** What the application answered. */
  | { readonly kind: "result"; readonly id: string; readonly capabilityId: string; readonly ok: boolean; readonly text: string }
  /** A person is being asked to consent. */
  | { readonly kind: "approval"; readonly id: string; readonly approval: ApprovalView; readonly status: ApprovalStatus }
  /** A previously denied action was recognized and was not run again. */
  | { readonly kind: "approval-repeat"; readonly id: string; readonly text: string; readonly hint: string }
  /** A UI action, presented by what happened rather than by a tool name. */
  | {
      readonly kind: "navigation";
      readonly id: string;
      readonly invocationId: string;
      readonly capabilityId: string;
      readonly heading: string;
      readonly summaries: readonly string[];
      readonly details: string;
      readonly status: "running" | "done" | "failed";
    }
  /** A quiet, readable explanation for a request the Forma schema refused. */
  | { readonly kind: "refusal"; readonly id: string; readonly heading: string; readonly summary: string; readonly details: string }
  /** The chat's own voice: a stop, a failure, the end of a session. */
  | { readonly kind: "notice"; readonly id: string; readonly notice: NoticeCopy };

export type SessionStatus = "starting" | "live" | "ended";

/** One thing the person said, with where they were: kept by the window, because nothing else keeps it. */
export interface Said {
  readonly id: string;
  readonly text: string;
  readonly chip?: ViewChip;
  /** The durable sequence applied when it was sent: it goes after that event when the history is replayed. */
  readonly afterSeq: number;
  /** The session it was said in; `null` while that session is still being started. */
  readonly sessionId: string | null;
}

export interface ChatState {
  readonly lines: readonly Line[];
  readonly session: { readonly id: string; readonly status: SessionStatus } | null;
  readonly startedAt: string | null;
  /**
   * The session this window started or was reattached to, if it has one. Events of
   * any other session are ignored: a replay of an earlier conversation, or of
   * another window's, must not replace the one the person has open.
   */
  readonly owned: string | null;
  /**
   * Why the session cannot be carried on, or `null`. A dead session is not a
   * stopped one: the person did not choose it, and the Runtime will not answer
   * it again, so the chat says so and offers a new one rather than a box that
   * answers every message with the same refusal (F-chat-4, F-chat-5, F-10-2).
   */
  readonly dead: NoticeCopy | null;
  /** The Device whose Runtime runs the session, as its own events say. */
  readonly deviceId: string | null;
  /** The highest durable sequence applied, of any session in the context. */
  readonly seq: number;
  /**
   * A session is being started and its id is not known yet. The Runtime's `session.created`
   * reaches the window before the sidecar's answer does, so what a session of unknown id says
   * in this gap is held rather than taken for someone else's conversation and adopted, which
   * wiped the person's first line (F-chat-1).
   */
  readonly starting: boolean;
  readonly held: readonly DurableEvent[];
  /** What the person said in this window, for the transcript to be rebuilt after a reload. */
  readonly said: readonly Said[];
  /** Said lines the replay has not yet reached the place of. */
  readonly unplaced: readonly Said[];
  /** A turn is open: `turn.started` seen, no terminal event yet. */
  readonly working: boolean;
  /** Ignore deltas for a turn that was already in flight when this tab reloaded. */
  readonly suppressDeltas: boolean;
  /** The initial history replay has reached its head. */
  readonly historyReady: boolean;
  /** This window stopped the current turn; its delayed cancellation is expected. */
  readonly stoppedLocally: boolean;
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

export const emptyChat: ChatState = {
  lines: [],
  session: null,
  startedAt: null,
  owned: null,
  dead: null,
  deviceId: null,
  seq: 0,
  starting: false,
  held: [],
  said: [],
  unplaced: [],
  working: false,
  suppressDeltas: false,
  historyReady: false,
  stoppedLocally: false,
  inputs: {},
  streaming: null,
  completed: [],
};

/** What a window keeps, per tab, to find its conversation again after a reload (D-1). */
export interface Remembered {
  readonly sessionId: string;
  readonly startedAt?: string | null;
  readonly deviceId: string | null;
  readonly dead: NoticeCopy | null;
  readonly said: readonly Said[];
}

export type ChatAction =
  | { readonly type: "events"; readonly events: readonly DurableEvent[] }
  /** An `agent.output.delta` from the live stream: unsequenced, never stored. */
  | { readonly type: "delta"; readonly sessionId: string; readonly turnId: string; readonly text: string }
  | { readonly type: "person"; readonly id: string; readonly text: string; readonly chip?: ViewChip }
  /** A session is about to be asked for. */
  | { readonly type: "starting" }
  /** The sidecar refused, or could not be reached: nothing was started. */
  | { readonly type: "startFailed" }
  /** This window started a session, and the sidecar has answered with its id. */
  | { readonly type: "started"; readonly sessionId: string; readonly id: string; readonly dividerText?: string; readonly deviceId?: string; readonly startedAt?: string }
  /** This tab had a session before the page was reloaded. */
  | { readonly type: "restore"; readonly remembered: Remembered }
  /** The first successful history read establishes whether a restored turn was already running. */
  | { readonly type: "historyLoaded" }
  | { readonly type: "notice"; readonly id: string; readonly notice: NoticeCopy }
  | { readonly type: "approval"; readonly approvalId: string; readonly status: ApprovalStatus }
  /** The person pressed stop. The Runtime may take a moment to confirm, or never will (see `reduce`). */
  | { readonly type: "stopped"; readonly id: string }
  /** The session cannot be carried on; `reason` is said to the person in plain words. */
  | { readonly type: "dead"; readonly notice: NoticeCopy }
  /** Nobody is signed in any more: the conversation was theirs and goes with them (D-1). */
  | { readonly type: "reset" };

/** Pending approvals of a session that is over cannot be answered; saying so is the card's whole honesty. */
function expirePending(lines: readonly Line[]): readonly Line[] {
  return lines.map((line) => (line.kind === "approval" && line.status === "pending" ? { ...line, status: "expired" as const } : line));
}

/** A call still "running" in a conversation that is over never finished: it says so, instead of running for ever. */
function settle(lines: readonly Line[]): readonly Line[] {
  return expirePending(lines).map((line) =>
    (line.kind === "capability" || line.kind === "navigation") && line.status === "running" ? { ...line, status: "failed" as const } : line,
  );
}

function die(state: ChatState, notice: NoticeCopy): ChatState {
  if (state.dead) return state;
  return {
    ...state,
    dead: notice,
    working: false,
    streaming: null,
    session: state.session ? { ...state.session, status: "ended" } : null,
    lines: settle(state.lines),
  };
}

/** The lines of what the person said once the replay has reached the event they came after. */
function place(state: ChatState, sequence: number): ChatState {
  if (state.unplaced.length === 0) return state;
  const due = state.unplaced.filter((said) => said.afterSeq <= sequence);
  if (due.length === 0) return state;
  return {
    ...state,
    unplaced: state.unplaced.filter((said) => said.afterSeq > sequence),
    lines: [...state.lines, ...due.map((said): Line => ({ kind: "person", id: said.id, text: said.text, ...(said.chip ? { chip: said.chip } : {}) }))],
  };
}

function step(state: ChatState, event: DurableEvent): ChatState {
  if (state.starting && event.sessionId !== undefined && event.sessionId !== state.session?.id) {
    return { ...state, seq: Math.max(state.seq, event.sequence), held: [...state.held, event] };
  }
  let next = applyEvent(state, event);
  // Nothing a dead session says afterwards can be answered or is still happening.
  if (next.dead && next !== state) next = { ...next, working: false, streaming: null, lines: settle(next.lines) };
  // The Device is learned from the session's own events; the first one names it.
  if (next.deviceId === null && event.deviceId !== undefined && event.sessionId !== undefined && event.sessionId === next.session?.id) {
    next = { ...next, deviceId: event.deviceId };
  }
  return place({ ...next, seq: Math.max(next.seq, event.sequence) }, event.sequence);
}

export function chatReducer(state: ChatState, action: ChatAction): ChatState {
  switch (action.type) {
    case "events":
      return action.events.reduce(step, state);
    case "historyLoaded":
      return state.historyReady
        ? state
        : { ...state, historyReady: true, suppressDeltas: state.suppressDeltas && state.working };
    case "delta":
      return applyDelta(state, action.sessionId, action.turnId, action.text);
    case "person": {
      const said: Said = { id: action.id, text: action.text, afterSeq: state.seq, sessionId: state.owned, ...(action.chip ? { chip: action.chip } : {}) };
      return {
        ...state,
        said: [...state.said, said],
        lines: [...state.lines, { kind: "person", id: action.id, text: action.text, ...(action.chip ? { chip: action.chip } : {}) }],
      };
    }
    case "starting":
      return { ...state, starting: true };
    case "startFailed":
      // What was heard in the meantime is read as it would have been: nothing here started it.
      return state.held.reduce(step, { ...state, starting: false, held: [] });
    case "started": {
      // What was said while the session was still being started belongs to it; the history of
      // every earlier one stays where it was, above a divider, so a new session reads as one.
      const fresh = state.said.filter((said) => said.sessionId === null);
      const freshIds = new Set(fresh.map((said) => said.id));
      const earlier = state.lines.filter((line) => !freshIds.has(line.id));
      const current = state.lines.filter((line) => freshIds.has(line.id));
      const begun: ChatState = {
        ...emptyChat,
        seq: state.seq,
        lines: [
        ...settle(earlier),
          ...(earlier.length > 0 || state.owned !== null ? [{ kind: "divider" as const, id: action.id, text: action.dividerText ?? "New session · 00:00" }] : []),
          ...current,
        ],
        deviceId: action.deviceId ?? null,
        startedAt: action.startedAt ?? new Date().toISOString(),
        said: fresh.map((said) => ({ ...said, sessionId: action.sessionId })),
        session: { id: action.sessionId, status: "starting" },
        owned: action.sessionId,
      };
      // What the new session said before its id was known, in the order it said it.
      return state.held.filter((event) => event.sessionId === action.sessionId).reduce(step, begun);
    }
    case "restore": {
      const { remembered } = action;
      const restored: ChatState = {
        ...emptyChat,
        session: { id: remembered.sessionId, status: "starting" },
        startedAt: remembered.startedAt ?? null,
        owned: remembered.sessionId,
        deviceId: remembered.deviceId,
        suppressDeltas: true,
        said: remembered.said,
        unplaced: remembered.said,
      };
      return remembered.dead === null ? restored : die(restored, remembered.dead);
    }
    case "notice":
      return { ...state, lines: [...state.lines, { kind: "notice", id: action.id, notice: action.notice }] };
    case "approval":
      return { ...state, lines: setApproval(state.lines, action.approvalId, action.status) };
    case "stopped":
      // The turn ends on the person's word, not the Runtime's. The Runtime does not
      // always write a terminal event for a cancelled turn (the Pontis finding
      // recorded in local-run.test.ts), so waiting for one would leave the box
      // saying "working" over a turn the person stopped. The Runtime session remains open.
      return {
        ...state,
        working: false,
        suppressDeltas: false,
        stoppedLocally: true,
        streaming: null,
        lines: [...state.lines, { kind: "notice", id: action.id, notice: { tone: "info", happened: "You stopped the agent.", next: "Start a new session when you're ready.", action: "New session" } }],
      };
    case "dead":
      return die(state, action.notice);
    case "reset":
      return emptyChat;
  }
}

/** What to keep of a chat, or `null` when there is nothing a reload could pick up. */
export function remember(state: ChatState): Remembered | null {
  if (state.owned === null) return null;
  return {
    sessionId: state.owned,
    startedAt: state.startedAt,
    deviceId: state.deviceId,
    dead: state.dead,
    said: state.said.filter((said) => said.sessionId === state.owned),
  };
}

const isUi = (capabilityId: string) => capabilityId.startsWith("ui.");

function applyDelta(state: ChatState, sessionId: string, turnId: string, text: string): ChatState {
  if (state.session?.id !== sessionId || state.session.status === "ended" || state.suppressDeltas) return state;
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

  // A session is this window's only if this window started it or was reattached to it (`restore`): a
  // reload finds its own conversation, and another window's — or the tab this one was copied from —
  // is not shown here, let alone spoken into. One chat per window (D-1).
  if (sessionId === undefined || sessionId !== state.session?.id) return state;

  const status = (next: SessionStatus): ChatState => ({ ...state, session: { id: sessionId, status: next } });

  switch (event.type) {
    case "session.started":
      return state.session.status === "ended" ? state : status("live");
    case "session.completed":
      return die(state, DEAD.completed);
    case "session.cancelled":
      // Stopped by this window, which has already said so; from anywhere else it is news.
      return state.session.status === "ended" || state.stoppedLocally
        ? state
        : die(state, DEAD.otherWindow);
    case "session.failed":
      return die(state, { tone: "stopped", happened: `The session failed: ${errorText(payload.error)}.`, next: "Start a new session to carry on.", action: "New session" });
    case "turn.started":
      return { ...state, working: true, stoppedLocally: false };
    case "turn.completed":
    case "turn.cancelled":
      return { ...state, working: false, suppressDeltas: false, streaming: null };
    case "turn.failed":
      return { ...state, working: false, suppressDeltas: false, streaming: null, lines: [...state.lines, { kind: "notice", id, notice: { tone: "needs-you", happened: `The agent's turn failed: ${errorText(payload.error)}`, next: "Review the response, then try again." } }] };
    case "agent.output.completed": {
      // Replaces whatever was streaming for the turn: the final text is the whole
      // of it, so appending would say everything twice.
      const turnId = typeof payload.turnId === "string" ? payload.turnId : null;
      const settled: ChatState = {
        ...state,
        suppressDeltas: false,
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
      return answered(state, id, true, payload);
    case "capability.failed": {
      const failed = answered(state, id, false, payload);
      // The session's permission to act for the person has a lifetime of about five minutes, and
      // the Runtime does not renew it once it has lapsed (F-30-5): every call after this one fails
      // the same way, so the conversation is over and the person is told so, not left to read the
      // same failure under each message.
      const code = isRecord(payload.error) ? payload.error.code : undefined;
      if (code === "delegation_expired") return die(failed, DEAD.expired);
      if (code === "approval_denied") {
        const invocationId = typeof payload.invocationId === "string" ? payload.invocationId : null;
        return invocationId !== null && repeatsDeniedApproval(state, invocationId)
          ? { ...failed, lines: [...failed.lines, { kind: "approval-repeat", id: `${id}:repeat`, text: "You declined this earlier in this session, so it wasn't asked again.", hint: "Ask the agent again if you change your mind." }] }
          : failed;
      }
      // A call the person's Stop cancelled. Pontis writes no terminal event for the turn when a cancel
      // lands (F-chat-3), so a window that did not press Stop — another tab — would stay "working"
      // for ever; the cancelled call is the one sign it was given, and it ends the turn.
      return code === "approval_cancelled" ? { ...failed, working: false, streaming: null } : failed;
    }
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
const UI_MOVING = "Moving your view…";
const UI_MOVED = "Moved your view";
const UI_REJECTED = "Couldn't move your view";
const UI_UNCONFIRMED = "Not confirmed by this window";
const UI_UNSUPPORTED = "The agent asked for something Forma can't do";

function actionDetails(capabilityId: string, input: unknown, outcome: unknown): string {
  return `${capabilityId}\nInput:\n${JSON.stringify(input, null, 2) ?? "null"}\nOutcome:\n${JSON.stringify(outcome, null, 2) ?? "null"}`;
}

function safeDetail(detail: string, input: unknown): string {
  let result = detail;
  if (isRecord(input)) {
    for (const [field, value] of Object.entries(input)) {
      if (!field.endsWith("_id") || typeof value !== "string" || value.length === 0) continue;
      const label = field === "design_id" ? "that design" : field === "page_id" ? "that page" : field === "layer_id" ? "that layer" : "that item";
      result = result.replaceAll(value, label);
    }
  }
  return result.replace(/\b[0-9a-f]{8}-[0-9a-f-]{27,}\b/gi, "that item");
}

function handledSummary(capabilityId: string, input: unknown, result: unknown): string {
  const detail = isRecord(result) && typeof result.detail === "string" ? safeDetail(result.detail, input).trim() : "";
  if (capabilityId === "ui.openDesign") {
    if (detail.startsWith("Opened ")) return detail;
    const name = detail.split(", page ")[0]?.trim();
    return name ? `Opened “${name}”` : "Opened the requested design.";
  }
  if (capabilityId === "ui.selectLayer") {
    if (detail.startsWith("Selected ")) return detail;
    const name = detail.split(" on page ")[0]?.replace(/^layer\s+/i, "").trim();
    return name && name !== "that layer" ? `Selected “${name}”` : "Selected the requested layer.";
  }
  if (capabilityId === "ui.setZoom") {
    const zoom = isRecord(input) && typeof input.zoom === "number" ? input.zoom : null;
    return zoom === null ? "Changed the canvas zoom." : `Zoom ${Math.round(zoom * 100)}%`;
  }
  if (capabilityId === "ui.setPanel") {
    const panel = isRecord(input) && typeof input.panel === "string" ? input.panel : null;
    return panel ? `Opened the ${panel[0]?.toUpperCase()}${panel.slice(1)} panel` : "Opened the requested panel.";
  }
  return detail || "Your view was updated.";
}

function rejectedSummary(capabilityId: string, input: unknown, message: string): string {
  const normalized = message.toLowerCase();
  if (normalized.includes("open a design first") || normalized.includes("does not have design") || normalized.includes("call ui.opend esign")) {
    return "Open a design first.";
  }
  if (normalized.includes("has no layer")) return "That layer isn't on this page.";
  if (normalized.includes("has no page")) return "That page isn't in this design.";
  if (normalized.includes("could not be read")) return "That design couldn't be opened.";
  if (capabilityId === "ui.setPanel" && isRecord(input) && typeof input.panel === "string") {
    return `Forma doesn't have a ${input.panel} panel.`;
  }
  return "The requested view change couldn't be made.";
}

function unsupportedSummary(capabilityId: string, input: unknown): string {
  if (capabilityId === "ui.setZoom" && isRecord(input) && typeof input.zoom === "number") {
    return input.zoom > 4 ? "Zoom is limited to 400%." : "Zoom is limited to 5%–400%.";
  }
  if (capabilityId === "ui.setPanel" && isRecord(input) && typeof input.panel === "string") {
    return `Forma doesn't have a ${input.panel} panel.`;
  }
  return "That request doesn't match what Forma supports.";
}

function refusalSummary(message: string): string {
  const summary = message.replace(/\b[0-9a-f]{8}-[0-9a-f-]{27,}\b/gi, "that item").trim();
  // Schema diagnostics vary by Runtime version and can expose internal field names.
  // Keep the card deliberately quiet and put the full diagnostic under Details.
  return summary.length > 0 ? "That request doesn't match what Forma supports." : "Forma couldn't use that request.";
}

function requested(state: ChatState, id: string, payload: Readonly<Record<string, unknown>>): ChatState {
  const { invocationId, capabilityId } = payload;
  if (typeof invocationId !== "string" || typeof capabilityId !== "string") return state;
  const inputs = { ...state.inputs, [invocationId]: payload.input };
  const line: Line = isUi(capabilityId)
    ? {
        kind: "navigation",
        id,
        invocationId,
        capabilityId,
        heading: UI_MOVING,
        summaries: [],
        details: actionDetails(capabilityId, payload.input, "waiting for the window"),
        status: "running",
      }
    : { kind: "capability", id, invocationId, capabilityId, summary: spec(capabilityId)?.presentation?.summary ?? `Runs ${capabilityId}`, status: "running" };
  return { ...state, inputs, lines: [...state.lines, line] };
}

function answered(state: ChatState, id: string, ok: boolean, payload: Readonly<Record<string, unknown>>): ChatState {
  const { invocationId, capabilityId } = payload;
  if (typeof invocationId !== "string" || typeof capabilityId !== "string") return state;
  const notRun = !ok && isRecord(payload.error) && payload.error.code === "approval_denied";
  const next: "done" | "failed" | "not_run" = ok ? "done" : notRun ? "not_run" : "failed";
  const lines = state.lines.map((line) =>
    line.kind === "capability" && line.invocationId === invocationId
      ? { ...line, status: next }
      : line.kind === "navigation" && line.invocationId === invocationId
        ? { ...line, status: ok ? "done" as const : "failed" as const }
        : line,
  );

  const input = state.inputs[invocationId];
  const error = isRecord(payload.error) ? payload.error : {};
  const errorCode = typeof error.code === "string" ? error.code : "";
  const errorMessage = typeof error.message === "string" ? error.message : "";

  if (isUi(capabilityId)) {
    const result = isRecord(payload.result) ? payload.result : {};
    const resultOutcome = typeof result.outcome === "string" ? result.outcome : "";
    const details = actionDetails(capabilityId, input, ok ? payload.result : payload.error);
    const navigation = lines.map((line) => {
      if (line.kind !== "navigation" || line.invocationId !== invocationId) return line;
      if (errorCode === "ui_unavailable" || errorCode === "ui_action_timeout") {
        return {
          ...line,
          heading: UI_UNCONFIRMED,
          summaries: ["Nothing confirmed this action in time.", "Your view may not have changed. Check it before asking again."],
          details,
          status: "failed" as const,
        };
      }
      if (errorCode === "validation_error") {
        return { ...line, heading: UI_UNSUPPORTED, summaries: [unsupportedSummary(capabilityId, input)], details, status: "failed" as const };
      }
      if (!ok || resultOutcome === "rejected" || errorCode === "ui_action_rejected") {
        return {
          ...line,
          heading: UI_REJECTED,
          summaries: [rejectedSummary(capabilityId, input, typeof result.detail === "string" ? result.detail : errorMessage)],
          details,
          status: "failed" as const,
        };
      }
      return {
        ...line,
        heading: UI_MOVED,
        summaries: [handledSummary(capabilityId, input, payload.result)],
        details,
        status: "done" as const,
      };
    });
    return { ...state, lines: navigation };
  }

  if (!ok && errorCode === "validation_error") {
    const summary = refusalSummary(errorMessage);
    return {
      ...state,
      lines: [
        ...lines,
        { kind: "refusal", id, heading: UI_UNSUPPORTED, summary, details: actionDetails(capabilityId, input, payload.error) },
      ],
    };
  }

  const text = ok ? "Completed" : notRun ? "Not run" : `Failed: ${errorText(payload.error)}`;
  return { ...state, lines: [...lines, { kind: "result", id, capabilityId, ok, text }] };
}

function canonicalJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (isRecord(value)) {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value) ?? "undefined";
}

function repeatsDeniedApproval(state: ChatState, invocationId: string): boolean {
  const currentInput = state.inputs[invocationId];
  if (!isRecord(currentInput) || !Array.isArray(currentInput.operations) || currentInput.operations.length === 0 || typeof currentInput.design_id !== "string") return false;
  const currentOperations = canonicalJson(currentInput.operations);
  return state.lines.some((line) =>
    line.kind === "approval" &&
    line.status === "denied" &&
    line.approval.invocationId !== invocationId &&
    line.approval.designId === currentInput.design_id &&
    line.approval.operations.length > 0 &&
    canonicalJson(line.approval.operations) === currentOperations,
  );
}

/**
 * Why a session is over, in the words a person can act on. Each ends with what to
 * do, because "the session failed" with nothing after it is where F-chat-5 left them.
 */
export const DEAD = {
  notAuthorized: { tone: "stopped", happened: "This session can no longer reach its Runtime.", next: "Start a new session to carry on.", action: "New session" },
  revoked: { tone: "stopped", happened: "The computer for this session was revoked.", next: "Pair a computer from the Agents page, then start a new session.", action: "Open Agents page" },
  none: { tone: "stopped", happened: "No computer is paired any more.", next: "Pair one from the Agents page, then start a new session.", action: "Open Agents page" },
  replaced: { tone: "stopped", happened: "The computer that ran this session is no longer connected.", next: "Start a new session to carry on.", action: "New session" },
  offline: MESSAGES.device_offline,
  expired: { tone: "stopped", happened: "This session expired because its permission to work in Forma was not renewed.", next: "Start a new session to carry on; what you said stays above it.", action: "New session" },
  stopWhileOffline: MESSAGES.device_offline,
  completed: { tone: "stopped", happened: "The session ended.", next: "Start a new one to carry on.", action: "New session" },
  otherWindow: { tone: "stopped", happened: "The session was stopped from another window.", next: "Start a new one to carry on.", action: "New session" },
} as const;

/** How long a Runtime may stay offline in the middle of a turn before the turn is given up. */
export const OFFLINE_GRACE_MS = 8_000;

/** A write the agent completed on a design, as the chat's history reports it. */
export interface AgentWrite {
  readonly designId: string;
  readonly projectId: string | null;
  readonly version: number;
}

/**
 * The completed `design.applyOperations` calls among these events.
 *
 * What lets the editor learn of the agent's writes from what it already reads
 * (F-chat-2, F-30-3): the capability's own result names the design and the
 * version it produced.
 */
export function agentWrites(events: readonly DurableEvent[]): readonly AgentWrite[] {
  return events.flatMap((event): AgentWrite[] => {
    if (event.type !== "capability.completed" || event.payload.capabilityId !== "design.applyOperations") return [];
    const { result } = event.payload;
    if (!isRecord(result) || typeof result.design_id !== "string" || typeof result.version !== "number") return [];
    return [{ designId: result.design_id, projectId: typeof result.project_id === "string" ? result.project_id : null, version: result.version }];
  });
}

/**
 * The context line a message carries, read back from text that begins with it.
 *
 * The sidecar writes `[forma-view] design=… name="…" page=… layer=…` in front of
 * what the person typed. Nothing durable carries a message back today, but one
 * that does will carry the line, and a chat that showed it would show the person
 * something they never wrote — so it is taken off and shown as the chip.
 */
export function splitViewLine(text: string): { readonly text: string; readonly chip?: ViewChip } {
  if (!text.startsWith("[forma-view] ")) return { text };
  const at = text.indexOf("\n");
  const line = at === -1 ? text : text.slice(0, at);
  const rest = at === -1 ? "" : text.slice(at + 1);
  const name = /name="((?:[^"\\]|\\.)*)"/u.exec(line)?.[1]?.replaceAll(/\\(.)/gu, "$1");
  const has = (key: string) => new RegExp(`(?:^| )${key}=`, "u").test(line);
  const chip: ViewChip = {
    ...(has("project") && !has("design") ? { project: name ?? "project" } : {}),
    ...(has("design") ? { design: name ?? "design" } : {}),
    ...(has("page") ? { page: "page" } : {}),
    ...(has("layer") ? { layer: "layer" } : {}),
  };
  return Object.keys(chip).length > 0 ? { text: rest, chip } : { text: rest };
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
