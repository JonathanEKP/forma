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

/**
 * Where the chat stands with its backend, said once.
 *
 * `offline` covers every way the agent service can be out of reach — no sidecar
 * behind the proxy, the context unreadable, the event page failing — because the
 * person's remedy is the same for all of them and they cannot act on the
 * differences.
 */
export type Connection = StreamState["at"];

export type Runtime = RuntimeView | { readonly at: "unknown" } | { readonly at: "unreachable" };

export interface Chat {
  readonly enabled: boolean;
  readonly state: ChatState;
  readonly connection: Connection;
  /** The UI stream is delivering, so the page is read slowly instead of quickly. */
  readonly live: boolean;
  readonly runtime: Runtime;
  /** Approvals waiting on the person, for the toggle's badge. */
  readonly pending: number;
  /** Approval ids already sent and not yet confirmed by the Runtime's own event. */
  readonly answering: ReadonlySet<string>;
  /** Why a message cannot be sent right now, or `null` when it can. */
  readonly blocked: string | null;
  /**
   * The session this window's chat started, or `null`: what the window's UI
   * connection is bound to, so the agent's actions reach the window it is
   * talking to (D-2). Never an adopted or ended one.
   */
  readonly sessionId: string | null;
  /** Why the session cannot be carried on, or `null`. Sending waits for `newSession` while it is set. */
  readonly dead: string | null;
  readonly send: (text: string) => Promise<void>;
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
export function whyBlocked(connection: Connection, runtime: Runtime): string | null {
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
      return "More than one device is online.";
    case "online":
      if (!runtime.provider) return `${runtime.name} reports no agent provider.`;
      if (!runtime.provider.usable) return `${runtime.provider.id} is ${runtime.provider.unusableBecause ?? "unavailable"}.`;
      return null;
  }
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
  const [answering, setAnswering] = useState<ReadonlySet<string>>(new Set());
  const [live, setLive] = useState(() => liveFeed.state().at === "connected");
  const seq = useRef(0);
  // The reader and the context it reads, for the live path to hand frames to.
  const reader = useRef<{ stream: EventStream; context: string } | null>(null);
  const queued = useRef<{ sessionId: string; text: string; view: ViewIds } | null>(null);

  const agent = useMemo(() => client ?? createAgentClient({ token: accessToken }), [client]);
  const localId = useCallback(() => {
    seq.current += 1;
    return `l${seq.current}`;
  }, []);
  const notice = useCallback(
    (tone: "info" | "error", text: string) => dispatch({ type: "notice", id: localId(), tone, text }),
    [localId],
  );

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
          for (const write of agentWrites(events)) writeRef.current?.(write);
        },
        onState: (next) => setStream(next.at),
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
  const readDevices = useCallback(async (): Promise<RuntimeView | { readonly at: "unreachable" } | null> => {
    reads.current += 1;
    const mine = reads.current;
    const answer = await agent.devices(state.deviceId);
    return mine === reads.current ? (answer.ok ? answer.value : { at: "unreachable" }) : null;
  }, [agent, state.deviceId]);
  const refreshRuntime = useCallback(async () => {
    const read = await readDevices();
    if (read) setRuntime(read);
  }, [readDevices]);

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
  const die = useCallback((reason: string) => dispatch({ type: "dead", reason }), []);

  // What a refusal from the sidecar means for the conversation. "This session isn't allowed
  // to do that" is the Runtime saying it does not know the session any more (F-chat-5), which
  // no retry will change; anything else is said and the person tries again.
  const refused = useCallback(
    (failure: Failure) => {
      if (failure.reason === "not_authorized") die(DEAD.notAuthorized);
      else notice("error", failure.message);
    },
    [die, notice],
  );

  // A message typed before the session is live waits for `session.started`: the
  // provider is not there to take input until then, and sending it early is a
  // race the person would see as a message that vanished.
  const session = state.session;
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
    const sessionDevice = "sessionDevice" in runtime ? runtime.sessionDevice : undefined;
    if (sessionDevice?.at === "revoked" || sessionDevice?.at === "missing") return die(DEAD.revoked);
    if (runtime.at === "revoked") return die(DEAD.revoked);
    if (runtime.at === "none") return die(DEAD.none);
    if (runtime.at === "online" && runtime.deviceId !== null && deviceId !== null && runtime.deviceId !== deviceId) {
      return die(DEAD.replaced);
    }
    if ((runtime.at === "offline" || runtime.at === "ambiguous") && working) {
      const timer = setTimeout(() => die(DEAD.offline), offlineGraceMs);
      return () => clearTimeout(timer);
    }
  }, [enabled, sessionEnded, runtime, deviceId, working, die, offlineGraceMs]);

  const blocked = whyBlocked(stream, runtime);
  const dead = state.dead?.reason ?? null;

  const start = useCallback(async (): Promise<string | null> => {
    if (runtime.at !== "online" || !runtime.provider?.usable) {
      notice("error", blocked ?? "There is no device to send that to.");
      return null;
    }
    dispatch({ type: "starting" });
    const started = await agent.start(runtime.provider.id);
    if (!started.ok) {
      dispatch({ type: "startFailed" });
      notice("error", started.failure.message);
      // The likeliest reason is that presence changed since the last read.
      if (started.failure.reason === "runtime_offline" || started.failure.reason === "ambiguous_device") void refreshRuntime();
      return null;
    }
    return started.value.sessionId;
  }, [agent, runtime, blocked, notice, refreshRuntime]);

  const send = useCallback(
    async (raw: string) => {
      const text = raw.trim();
      if (text.length === 0 || dead !== null) return;
      const seen = viewRef.current?.();
      dispatch({ type: "person", id: localId(), text, ...(seen?.chip ? { chip: seen.chip } : {}) });
      const ids = seen?.ids ?? {};

      if (session && session.status !== "ended") {
        if (session.status === "starting") {
          queued.current = { sessionId: session.id, text, view: ids };
          return;
        }
        const answer = await agent.input(session.id, text, ids);
        if (!answer.ok) refused(answer.failure);
        return;
      }

      const sessionId = await start();
      if (sessionId === null) return;
      queued.current = { sessionId, text, view: ids };
      dispatch({ type: "started", sessionId, id: localId() });
    },
    [agent, session, dead, start, refused, localId],
  );

  // Nothing starts by itself: a dead session waits here, with its history above, until the
  // person asks for another (D-6).
  const newSession = useCallback(async () => {
    const sessionId = await start();
    if (sessionId !== null) dispatch({ type: "started", sessionId, id: localId() });
  }, [start, localId]);

  const stop = useCallback(async () => {
    if (!session) return;
    const answer = await agent.cancel(session.id);
    if (answer.ok) dispatch({ type: "stopped", id: localId() });
    // Stopping is the person leaving the conversation: when the Runtime cannot even be told,
    // it is over, and "your Runtime is offline" over a turn that stays running is the wedge.
    else if (answer.failure.reason === "not_authorized") die(DEAD.notAuthorized);
    else if (answer.failure.reason === "runtime_offline") die(DEAD.stopWhileOffline);
    else notice("error", answer.failure.message);
  }, [agent, session, notice, localId, die]);

  const decide = useCallback(
    async (approvalId: string, decision: Decision) => {
      setAnswering((current) => new Set(current).add(approvalId));
      const answer = await agent.approval(approvalId, decision);
      if (answer.ok) return;
      setAnswering((current) => {
        const next = new Set(current);
        next.delete(approvalId);
        return next;
      });
      notice("error", answer.failure.message);
    },
    [agent, notice],
  );

  return {
    enabled,
    state,
    connection: stream,
    live,
    runtime,
    pending: pendingApprovals(state).length,
    answering,
    blocked,
    sessionId,
    dead,
    send,
    newSession,
    stop,
    decide,
  };
}
