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
  chatReducer,
  createAgentClient,
  emptyChat,
  pendingApprovals,
  type AgentClient,
  type ChatState,
  type Decision,
  type RuntimeView,
} from "./chat";
import { liveFeed } from "./channel";
import { useWindowBinding } from "./useWindowBinding";
import { asDurable, openEventStream, parseLiveFrame, RETRY_MS, type EventStream, type StreamState } from "./events";

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
  readonly send: (text: string) => Promise<void>;
  readonly stop: () => Promise<void>;
  readonly decide: (approvalId: string, decision: Decision) => Promise<void>;
}

export interface ChatOptions {
  readonly enabled: boolean;
  /** Only tests pass these. */
  readonly client?: AgentClient;
  readonly pollMs?: number;
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
    case "ambiguous":
      return "More than one device is online.";
    case "online":
      if (!runtime.provider) return `${runtime.name} reports no agent provider.`;
      if (!runtime.provider.usable) return `${runtime.provider.id} is ${runtime.provider.unusableBecause ?? "unavailable"}.`;
      return null;
  }
}

export function useChat({ enabled, client, pollMs }: ChatOptions): Chat {
  const [state, dispatch] = useReducer(chatReducer, emptyChat);
  const [stream, setStream] = useState<Connection>("connecting");
  const [runtime, setRuntime] = useState<Runtime>({ at: "unknown" });
  const [answering, setAnswering] = useState<ReadonlySet<string>>(new Set());
  const [live, setLive] = useState(() => liveFeed.state().at === "connected");
  const seq = useRef(0);
  // The reader and the context it reads, for the live path to hand frames to.
  const reader = useRef<{ stream: EventStream; context: string } | null>(null);
  const queued = useRef<{ sessionId: string; text: string } | null>(null);

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
      const opened = openEventStream({
        applicationContextId: context.value,
        token: accessToken,
        onEvents: (events) => dispatch({ type: "events", events }),
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
  }, [enabled, agent, pollMs]);

  const refreshRuntime = useCallback(async () => {
    const answer = await agent.devices();
    setRuntime(answer.ok ? answer.value : { at: "unreachable" });
  }, [agent]);

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
      void agent.devices().then((answer) => {
        if (!closed) setRuntime(answer.ok ? answer.value : { at: "unreachable" });
      });
    };
    tick();
    const timer = setInterval(tick, DEVICES_MS);
    return () => {
      closed = true;
      clearInterval(timer);
    };
  }, [enabled, agent]);

  // A message typed before the session is live waits for `session.started`: the
  // provider is not there to take input until then, and sending it early is a
  // race the person would see as a message that vanished.
  const session = state.session;
  // D-2: an agent's actions go to the window whose chat started the session.
  useWindowBinding(session?.id ?? null);
  useEffect(() => {
    const waiting = queued.current;
    if (!waiting || !session || session.id !== waiting.sessionId) return;
    if (session.status === "ended") {
      queued.current = null;
      return;
    }
    if (session.status !== "live") return;
    queued.current = null;
    void agent.input(waiting.sessionId, waiting.text).then((answer) => {
      if (!answer.ok) notice("error", answer.failure.message);
    });
  }, [agent, session, notice]);

  const blocked = whyBlocked(stream, runtime);

  const send = useCallback(
    async (raw: string) => {
      const text = raw.trim();
      if (text.length === 0) return;
      dispatch({ type: "person", id: localId(), text });

      if (session && session.status !== "ended") {
        if (session.status === "starting") {
          queued.current = { sessionId: session.id, text };
          return;
        }
        const answer = await agent.input(session.id, text);
        if (!answer.ok) notice("error", answer.failure.message);
        return;
      }

      if (runtime.at !== "online" || !runtime.provider?.usable) {
        notice("error", blocked ?? "There is no device to send that to.");
        return;
      }
      const started = await agent.start(runtime.provider.id);
      if (!started.ok) {
        notice("error", started.failure.message);
        // The likeliest reason is that presence changed since the last read.
        if (started.failure.reason === "runtime_offline" || started.failure.reason === "ambiguous_device") void refreshRuntime();
        return;
      }
      queued.current = { sessionId: started.value.sessionId, text };
      dispatch({ type: "started", sessionId: started.value.sessionId });
    },
    [agent, session, runtime, blocked, notice, localId, refreshRuntime],
  );

  const stop = useCallback(async () => {
    if (!session) return;
    const answer = await agent.cancel(session.id);
    if (answer.ok) dispatch({ type: "stopped", id: localId() });
    else notice("error", answer.failure.message);
  }, [agent, session, notice, localId]);

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
    send,
    stop,
    decide,
  };
}
