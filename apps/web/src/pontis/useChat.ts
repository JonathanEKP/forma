/**
 * The chat's behaviour, held for as long as the editor is on screen (plan §11
 * step 10b).
 *
 * Mounted by the editor rather than by the panel, because the panel is closed by
 * default and an approval does not wait for it to be opened: an agent blocked on
 * consent is the one event the person has to see, so the stream keeps reading
 * with the panel shut and the toggle carries the count.
 *
 * Nothing here runs unless `enabled`. That is `access.at === "ready"` — a
 * hosted Forma with a signed-in person and an account — the same gate as
 * `useUiChannel`. A local Forma has no sidecar, and asking for one would only
 * produce a 404 to explain to nobody.
 */
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";

import { accessToken } from "../identity";
import {
  agentWrites,
  chatReducer,
  createAgentClient,
  DEAD,
  emptyChat,
  OFFLINE_GRACE_MS,
  pendingApprovals,
  remember,
  type AgentClient,
  type AgentWrite,
  type ChatState,
  type Decision,
  type Failure,
  type NoticeCopy,
  type RuntimeDevice,
  type RuntimeView,
  type ViewChip,
  type ViewIds,
} from "./chat";
import { liveFeed } from "./channel";
import { useWindowBinding } from "./useWindowBinding";
import { asDurable, openEventStream, parseLiveFrame, RETRY_MS, type EventStream, type StreamState } from "./events";
import { recall, remembering, type Store } from "./session-store";
import { windowId } from "./window";

/** How often the device list is re-read: presence changes slowly, and a turn does not need it. */
export const DEVICES_MS = 10_000;
/** A delivered approval resolve is only a socket write; wait for the durable approval event. */
export const APPROVAL_CONFIRMATION_MS = 20_000;

/**
 * Where the chat stands with its backend, said once.
 *
 * `offline` covers every way the agent service can be out of reach — no sidecar
 * behind the proxy, the context unreadable, the event page failing — because the
 * person's remedy is the same for all of them and they cannot act on the
 * differences.
 */
export type Connection = StreamState["at"];

export type Runtime = RuntimeView | { readonly at: "unknown" } | { readonly at: "unreachable" } | { readonly at: "unauthenticated" };

export interface Chat {
  readonly enabled: boolean;
  readonly state: ChatState;
  readonly connection: Connection;
  /** The UI stream is delivering, so the page is read slowly instead of quickly. */
  readonly live: boolean;
  readonly runtime: Runtime;
  /** Approvals waiting on the person, for the toggle's badge. */
  readonly pending: number;
  /** Approval ids whose resolve is being sent or is still awaiting the Runtime's event. */
  readonly answering: ReadonlySet<string>;
  /** Approval ids still pending after the confirmation window elapsed. */
  readonly unconfirmed: ReadonlySet<string>;
  /** Why a message cannot be sent right now, or `null` when it can. */
  readonly blocked: string | null;
  /**
   * The session this window's chat started, or `null`: what the window's UI
   * connection is bound to, so the agent's actions reach the window it is
   * talking to (D-2). Never an adopted or ended one.
   */
  readonly sessionId: string | null;
  /** Why the session cannot be carried on, or `null`. Sending waits for `newSession` while it is set. */
  readonly dead: NoticeCopy | null;
  readonly computers: readonly RuntimeDevice[];
  readonly selectedComputerId: string | null;
  readonly needsComputerChoice: boolean;
  readonly chooseComputer: (deviceId: string) => Promise<void>;
  readonly send: (text: string, onAccepted?: () => void) => Promise<void>;
  readonly newSession: () => Promise<void>;
  readonly stop: () => Promise<void>;
  readonly decide: (approvalId: string, decision: Decision) => Promise<void>;
}

export interface ChatOptions {
  readonly enabled: boolean;
  /**
   * What the person has open, read when a message is sent: the ids the sidecar
   * turns into the line in front of it, and the names the chip shows them.
   */
  readonly view?: () => { readonly ids: ViewIds; readonly chip?: ViewChip };
  /** The agent completed a write on a design: the editor's cue to follow it. */
  readonly onWrite?: (write: AgentWrite) => void;
  /** Only tests pass these. */
  readonly client?: AgentClient;
  readonly pollMs?: number;
  readonly offlineGraceMs?: number;
  readonly storage?: Store | null;
}

/** Why the current runtime cannot take a message, in words, or `null` if it can. */
export function whyBlocked(connection: Connection, runtime: Runtime, needsComputerChoice = false): string | null {
  if (needsComputerChoice) return "Choose a computer to run the agent.";
  if (runtime.at === "unauthenticated") return "Your sign-in has expired. Sign in again.";
  if (connection === "offline") return "The agent service can't be reached.";
  switch (runtime.at) {
    case "unknown":
      return "Checking for your device…";
    case "unreachable":
      return "The agent service can't be reached.";
    case "none":
      return "No device is paired yet.";
    case "offline":
      return `${runtime.name} is offline.`;
    case "revoked":
      return `${runtime.name} was revoked. Pair a new device from the Agents page.`;
    case "ambiguous":
      return null;
    case "online":
      if (!runtime.provider) return `${runtime.name} reports no agent provider.`;
      if (!runtime.provider.usable) return `${runtime.provider.id} is ${runtime.provider.unusableBecause ?? "unavailable"}.`;
      return null;
  }
}

const DEVICE_CHOICE_KEY = "forma.chat.device";

function rememberedDevice(): string | null {
  try {
    return globalThis.localStorage.getItem(DEVICE_CHOICE_KEY);
  } catch {
    return null;
  }
}

function onlineComputers(runtime: Runtime): readonly RuntimeDevice[] {
  if (runtime.at === "ambiguous") return runtime.devices;
  if (runtime.at !== "online" || runtime.deviceId === null) return [];
  return [{ id: runtime.deviceId, name: runtime.name, runtimeVersion: runtime.runtimeVersion, provider: runtime.provider, presence: "Online" }];
}

function dividerLabel(deviceName?: string): string {
  const time = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  return deviceName ? `New session on ${deviceName} · ${time}` : `New session · ${time}`;
}

function unavailableNotice(runtime: Runtime, connection: Connection, hasChoices: boolean): NoticeCopy {
  if (hasChoices) return { tone: "needs-you", happened: "Choose a computer to run the agent.", next: "Choose one from Run on before sending." };
  if (runtime.at === "none") return { tone: "needs-you", happened: "No computer is paired.", next: "Pair one, then start a session.", action: "Open Agents page" };
  if (runtime.at === "offline") return { tone: "needs-you", happened: `${runtime.name} is offline.`, next: "Bring it online, then try again." };
  if (runtime.at === "revoked") return { tone: "needs-you", happened: `${runtime.name} was revoked.`, next: "Pair a computer, then start again.", action: "Open Agents page" };
  if (runtime.at === "unauthenticated") return { tone: "needs-you", happened: "Your sign-in has expired.", next: "Sign in again, then start a new session.", action: "Sign in again" };
  if (connection === "offline" || runtime.at === "unreachable") return { tone: "needs-you", happened: "Forma's agent service can't be reached.", next: "Check the connection, then try again." };
  return DEAD.offline;
}

export function useChat({ enabled, client, pollMs, offlineGraceMs = OFFLINE_GRACE_MS, view, onWrite, storage }: ChatOptions): Chat {
  const [state, dispatch] = useReducer(chatReducer, emptyChat);
  // The application context and the window the tab's memory is keyed by, known once the sidecar has said.
  const [context, setContext] = useState<{ readonly context: string; readonly window: string } | null>(null);
  const viewRef = useRef(view);
  viewRef.current = view;
  const writeRef = useRef(onWrite);
  writeRef.current = onWrite;
  const [stream, setStream] = useState<Connection>("connecting");
  const [runtime, setRuntime] = useState<Runtime>({ at: "unknown" });
  const [preferredDeviceId, setPreferredDeviceId] = useState(rememberedDevice);
  const [answering, setAnswering] = useState<ReadonlySet<string>>(new Set());
  const [unconfirmed, setUnconfirmed] = useState<ReadonlySet<string>>(new Set());
  const [live, setLive] = useState(() => liveFeed.state().at === "connected");
  const seq = useRef(0);
  // The reader and the context it reads, for the live path to hand frames to.
  const reader = useRef<{ stream: EventStream; context: string } | null>(null);
  const queued = useRef<{ sessionId: string; text: string; view: ViewIds; onAccepted?: () => void } | null>(null);
  const stopPending = useRef<string | null>(null);
  const approvalTimers = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  useEffect(
    () => () => {
      for (const timer of approvalTimers.current.values()) clearTimeout(timer);
      approvalTimers.current.clear();
    },
    [],
  );

  const agent = useMemo(() => client ?? createAgentClient({ token: accessToken }), [client]);
  const localId = useCallback(() => {
    seq.current += 1;
    return `l${seq.current}`;
  }, []);
  const notice = useCallback((copy: NoticeCopy) => dispatch({ type: "notice", id: localId(), notice: copy }), [localId]);

  // The context first, then the stream that has to name it. Retried quietly: the
  // sidecar may simply not be up yet, and an editor that stays broken until
  // reloaded is worse than one that says "offline" and recovers.
  useEffect(() => {
    if (!enabled) return;
    let closed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let close: (() => void) | undefined;
    const connect = async () => {
      const context = await agent.context();
      if (closed) return;
      if (!context.ok) {
        if (context.failure.reason === "not_authenticated") setRuntime({ at: "unauthenticated" });
        setStream("offline");
        timer = setTimeout(() => void connect(), RETRY_MS);
        return;
      }
      // What this tab was saying before it was reloaded, before the history is read: the
      // window re-attaches to its own session rather than to whichever is latest (D-1).
      const ownWindow = await windowId();
      if (closed) return;
      const remembered = recall(context.value, ownWindow, storage);
      if (remembered) dispatch({ type: "restore", remembered });
      setContext({ context: context.value, window: ownWindow });
      const opened = openEventStream({
        applicationContextId: context.value,
        token: accessToken,
        onEvents: (events) => {
          dispatch({ type: "events", events });
          const confirmations = events.flatMap((event) => {
            if (event.type !== "approval.resolved" && event.type !== "approval.cancelled") return [];
            return typeof event.payload.approvalId === "string" ? [event.payload.approvalId] : [];
          });
          if (confirmations.length > 0) {
            for (const approvalId of confirmations) {
              const timer = approvalTimers.current.get(approvalId);
              if (timer !== undefined) clearTimeout(timer);
              approvalTimers.current.delete(approvalId);
            }
            setAnswering((current) => {
              const next = new Set(current);
              confirmations.forEach((approvalId) => next.delete(approvalId));
              return next;
            });
            setUnconfirmed((current) => {
              const next = new Set(current);
              confirmations.forEach((approvalId) => next.delete(approvalId));
              return next;
            });
          }
          for (const write of agentWrites(events)) writeRef.current?.(write);
        },
        onState: (next) => {
          setStream(next.at);
          if (next.at === "live") dispatch({ type: "historyLoaded" });
        },
        ...(pollMs === undefined ? {} : { pollMs }),
      });
      opened.setLive(liveFeed.state().at === "connected");
      reader.current = { stream: opened, context: context.value };
      close = () => {
        reader.current = null;
        opened.close();
      };
    };
    setStream("connecting");
    void connect();
    return () => {
      closed = true;
      if (timer !== undefined) clearTimeout(timer);
      close?.();
    };
  }, [enabled, agent, pollMs, storage]);

  // Signed out, or no longer ready: what was said belonged to the person who is gone.
  useEffect(() => {
    if (enabled) return;
    dispatch({ type: "reset" });
    setContext(null);
  }, [enabled]);

  // Kept per tab so a reload finds its way back; written only after the first read of
  // the tab's memory, or the empty chat of a fresh page would erase what it held.
  const remembered = useMemo(() => remember(state), [state]);
  useEffect(() => {
    if (!enabled || context === null) return;
    remembering(context.context, context.window, remembered, storage);
  }, [enabled, context, remembered, storage]);

  // Reads can overlap — a presence frame lands while the ten-second read is out — and the answer that
  // was asked for last is the one that is true: a slower, older one must not overwrite it.
  const reads = useRef(0);
  const readDevices = useCallback(async (): Promise<RuntimeView | { readonly at: "unreachable" | "unauthenticated" } | null> => {
    reads.current += 1;
    const mine = reads.current;
    const answer = await agent.devices(state.deviceId);
    const value = answer.ok ? answer.value : answer.failure.reason === "not_authenticated" ? { at: "unauthenticated" as const } : { at: "unreachable" as const };
    return mine === reads.current ? value : null;
  }, [agent, state.deviceId]);
  const refreshRuntime = useCallback(async () => {
    const read = await readDevices();
    if (read) setRuntime(read);
  }, [readDevices]);

  const computers = onlineComputers(runtime);
  const session = state.session;
  const activeDeviceId = session !== null && session.status !== "ended" ? state.deviceId : null;
  const rememberedOnline = computers.find((device) => device.id === preferredDeviceId);
  const selectedComputerId = activeDeviceId ?? (computers.length === 1 ? computers[0]?.id ?? null : rememberedOnline?.id ?? null);
  const needsComputerChoice = computers.length > 1 && selectedComputerId === null;

  // The live path: what the sidecar fans out to this window while the agent
  // works. A frame with a sequence goes to the reader, which applies it only if
  // it is the next one; one without — a delta, a presence announcement — is
  // never stored and so is handled here and nowhere else.
  useEffect(() => {
    if (!enabled) return;
    return liveFeed.subscribe({
      state: (next) => {
        const connected = next.at === "connected";
        setLive(connected);
        reader.current?.stream.setLive(connected);
      },
      frame: (raw) => {
        const frame = parseLiveFrame(raw);
        if (!frame) return;
        const held = reader.current;
        const durable = held ? asDurable(frame, held.context) : null;
        if (durable) {
          held?.stream.offer(durable);
        } else if (frame.sequence === undefined && frame.type === "agent.output.delta") {
          const { turnId, text } = frame.payload;
          if (frame.sessionId && typeof turnId === "string" && typeof text === "string") {
            dispatch({ type: "delta", sessionId: frame.sessionId, turnId, text });
          }
        } else if (frame.type.startsWith("device.")) {
          // Presence changed; the device list is the answer, and it is cheap.
          void refreshRuntime();
        }
      },
    });
  }, [enabled, refreshRuntime]);

  useEffect(() => {
    if (!enabled) return;
    let closed = false;
    const tick = () => {
      void readDevices().then((read) => {
        if (!closed && read) setRuntime(read);
      });
    };
    tick();
    const timer = setInterval(tick, DEVICES_MS);
    return () => {
      closed = true;
      clearInterval(timer);
    };
  }, [enabled, readDevices]);

  // A session that cannot be carried on is said so once, and the box waits for a new one.
  const die = useCallback((notice: NoticeCopy) => dispatch({ type: "dead", notice }), []);

  // What a refusal from the sidecar means for the conversation. "This session isn't allowed
  // to do that" is the Runtime saying it does not know the session any more (F-chat-5), which
  // no retry will change; anything else is said and the person tries again.
  const refused = useCallback(
    (failure: Failure) => {
      if (failure.reason === "not_authorized") die(DEAD.notAuthorized);
      else if (failure.reason === "device_offline") die(DEAD.offline);
      else notice(failure.notice);
    },
    [die, notice],
  );

  // A message typed before the session is live waits for `session.started`: the
  // provider is not there to take input until then, and sending it early is a
  // race the person would see as a message that vanished.
  // D-2: an agent's actions go to the window whose chat *started* the session — not one it merely
  // adopted from the history, and not one that is over: a dead or stopped conversation has nobody
  // running it, so the window stops speaking for it.
  const sessionId = state.owned !== null && session?.id === state.owned && session.status !== "ended" ? state.owned : null;
  useWindowBinding(sessionId);
  useEffect(() => {
    const waiting = queued.current;
    if (!waiting || !session || session.id !== waiting.sessionId) return;
    if (session.status === "ended") {
      queued.current = null;
      return;
    }
    if (session.status !== "live") return;
    queued.current = null;
    void agent.input(waiting.sessionId, waiting.text, waiting.view).then((answer) => {
      if (!answer.ok) refused(answer.failure);
      else waiting.onAccepted?.();
    });
  }, [agent, session, refused]);

  // The Runtime a session runs on can go: revoked, unpaired, replaced by another, or off in the
  // middle of a turn. Each leaves a conversation nobody is running, and none of them writes the
  // terminal event the chat would otherwise wait for (F-10-2, F-chat-4), so the chat reads it off
  // the device list it already polls. A Runtime that is merely offline between turns is left
  // alone: it may come back, and a send says so if the session did not survive.
  const deviceId = state.deviceId;
  const working = state.working;
  const sessionEnded = session === null || session.status === "ended" || state.dead !== null;
  useEffect(() => {
    if (!enabled || sessionEnded) return;
    // A device read can finish just as a new session switches computers. Do not
    // apply the prior session's offline status to the newly chosen computer.
    const sessionDevice = "sessionDevice" in runtime && runtime.sessionDevice?.id === deviceId ? runtime.sessionDevice : undefined;
    if (sessionDevice?.at === "revoked" || sessionDevice?.at === "missing") return die(DEAD.revoked);
    if (runtime.at === "revoked") return die(DEAD.revoked);
    if (runtime.at === "none") return die(DEAD.none);
    if (sessionDevice?.at === "offline") return die(DEAD.offline);
    if (runtime.at === "online" && runtime.deviceId !== null && deviceId !== null && runtime.deviceId !== deviceId) {
      return die(DEAD.replaced);
    }
    if (runtime.at === "offline" && working) {
      const timer = setTimeout(() => die(DEAD.offline), offlineGraceMs);
      return () => clearTimeout(timer);
    }
  }, [enabled, sessionEnded, runtime, deviceId, working, die, offlineGraceMs]);

  const targetComputer = computers.find((device) => device.id === selectedComputerId);
  const blocked = needsComputerChoice
    ? "Choose a computer to run the agent."
    : runtime.at === "ambiguous"
      ? targetComputer?.provider === null || targetComputer?.provider === undefined
        ? "That computer does not report an agent provider."
        : !targetComputer.provider.usable
          ? `${targetComputer.provider.id} is ${targetComputer.provider.unusableBecause ?? "unavailable"}.`
          : null
      : whyBlocked(stream, runtime);
  const dead = state.dead;

  const start = useCallback(async (requestedDeviceId?: string): Promise<{ readonly sessionId: string; readonly device: RuntimeDevice } | null> => {
    const available = onlineComputers(runtime);
    const selectedId = requestedDeviceId ?? (available.length === 1 ? available[0]?.id : preferredDeviceId && available.some((device) => device.id === preferredDeviceId) ? preferredDeviceId : null);
    const selected = available.find((device) => device.id === selectedId);
    if (!selectedId || !selected) {
      notice(unavailableNotice(runtime, stream, available.length > 1));
      return null;
    }
    if (!selected.provider?.usable) {
      notice({ tone: "needs-you", happened: selected.provider?.unusableBecause ? `${selected.provider.id} is ${selected.provider.unusableBecause}.` : `${selected.name} does not report an agent provider.`, next: "Choose a computer with an available provider.", action: "Open Agents page" });
      return null;
    }
    dispatch({ type: "starting" });
    const started = await agent.start(selected.provider.id, selected.id);
    if (!started.ok) {
      dispatch({ type: "startFailed" });
      notice(started.failure.notice);
      // The likeliest reason is that presence changed since the last read.
      if (started.failure.reason === "runtime_offline" || started.failure.reason === "device_choice_required" || started.failure.reason === "device_offline") void refreshRuntime();
      return null;
    }
    return { sessionId: started.value.sessionId, device: selected };
  }, [agent, runtime, stream, preferredDeviceId, notice, refreshRuntime]);

  const chooseComputer = useCallback(async (requestedDeviceId: string) => {
    const selected = computers.find((device) => device.id === requestedDeviceId);
    if (!selected) return;
    setPreferredDeviceId(requestedDeviceId);
    try {
      globalThis.localStorage.setItem(DEVICE_CHOICE_KEY, requestedDeviceId);
    } catch {
      // The selector remains usable when browser storage is unavailable.
    }
    if (session && session.status !== "ended" && state.deviceId !== null && state.deviceId !== requestedDeviceId) {
      const started = await start(requestedDeviceId);
      if (started) dispatch({ type: "started", sessionId: started.sessionId, deviceId: started.device.id, dividerText: dividerLabel(started.device.name), id: localId() });
    }
  }, [computers, session, state.deviceId, start, localId]);

  const send = useCallback(
    async (raw: string, onAccepted?: () => void) => {
      const text = raw.trim();
      if (text.length === 0 || dead !== null || state.working || needsComputerChoice) return;
      const seen = viewRef.current?.();
      dispatch({ type: "person", id: localId(), text, ...(seen?.chip ? { chip: seen.chip } : {}) });
      const ids = seen?.ids ?? {};

      if (session && session.status !== "ended") {
        if (session.status === "starting") {
          queued.current = { sessionId: session.id, text, view: ids, ...(onAccepted ? { onAccepted } : {}) };
          return;
        }
        const answer = await agent.input(session.id, text, ids);
        if (!answer.ok) refused(answer.failure);
        else onAccepted?.();
        return;
      }

      const started = await start(selectedComputerId ?? undefined);
      if (started === null) return;
      queued.current = { sessionId: started.sessionId, text, view: ids, ...(onAccepted ? { onAccepted } : {}) };
      dispatch({ type: "started", sessionId: started.sessionId, deviceId: started.device.id, dividerText: dividerLabel(), id: localId() });
    },
    [agent, session, dead, state.working, needsComputerChoice, selectedComputerId, start, refused, localId],
  );

  // Nothing starts by itself: a dead session waits here, with its history above, until the
  // person asks for another (D-6).
  const newSession = useCallback(async () => {
    const started = await start(selectedComputerId ?? undefined);
    if (started !== null) dispatch({ type: "started", sessionId: started.sessionId, deviceId: started.device.id, dividerText: dividerLabel(), id: localId() });
  }, [start, selectedComputerId, localId]);

  const stop = useCallback(async () => {
    if (!session || !state.working || stopPending.current === session.id) return;
    const sessionId = session.id;
    stopPending.current = sessionId;
    try {
      const answer = await agent.cancel(sessionId);
      if (answer.ok) dispatch({ type: "stopped", id: localId() });
      // If the Runtime cannot be told, the turn may still be running. End the local chat so it
      // does not offer a message against a turn the person cannot stop.
      else if (answer.failure.reason === "not_authorized") die(DEAD.notAuthorized);
      else if (answer.failure.reason === "runtime_offline") die(DEAD.stopWhileOffline);
      else if (answer.failure.reason === "device_offline") die(DEAD.offline);
      else notice(answer.failure.notice);
    } finally {
      if (stopPending.current === sessionId) stopPending.current = null;
    }
  }, [agent, session, state.working, notice, localId, die]);

  const decide = useCallback(
    async (approvalId: string, decision: Decision) => {
      if (approvalTimers.current.has(approvalId)) return;
      setAnswering((current) => new Set(current).add(approvalId));
      setUnconfirmed((current) => {
        const next = new Set(current);
        next.delete(approvalId);
        return next;
      });
      const timer = setTimeout(() => {
        approvalTimers.current.delete(approvalId);
        setAnswering((current) => {
          const next = new Set(current);
          next.delete(approvalId);
          return next;
        });
        setUnconfirmed((current) => new Set(current).add(approvalId));
      }, APPROVAL_CONFIRMATION_MS);
      approvalTimers.current.set(approvalId, timer);
      const answer = await agent.approval(approvalId, decision);
      if (answer.ok) return;
      const pendingTimer = approvalTimers.current.get(approvalId);
      if (pendingTimer !== undefined) clearTimeout(pendingTimer);
      approvalTimers.current.delete(approvalId);
      setAnswering((current) => {
        const next = new Set(current);
        next.delete(approvalId);
        return next;
      });
      setUnconfirmed((current) => {
        const next = new Set(current);
        next.delete(approvalId);
        return next;
      });
      if (answer.failure.reason === "device_offline") die(DEAD.offline);
      else notice(answer.failure.notice);
    },
    [agent, notice, die],
  );

  return {
    enabled,
    state,
    connection: stream,
    live,
    runtime,
    pending: pendingApprovals(state).length,
    answering,
    unconfirmed,
    blocked,
    sessionId,
    dead,
    computers,
    selectedComputerId,
    needsComputerChoice,
    chooseComputer,
    send,
    newSession,
    stop,
    decide,
  };
}
