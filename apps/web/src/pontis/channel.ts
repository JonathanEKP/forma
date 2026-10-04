/**
 * The UI action channel: the editor as a Pontis `UIClient` (plan §11 step 9).
 *
 * An agent asks for `ui.openDesign`, the Gateway routes it to the window this
 * module registered, a handler runs here, and the acknowledgement is what makes
 * the agent's call succeed. Delivery is not success — `pontis/docs/13` §
 * Acknowledgement is explicit, and an action the agent believes landed while the
 * person saw nothing is the failure this contract exists to prevent.
 *
 * # Why this is Forma's code and not `@pontis/react`
 *
 * Plan §10(b) D8. Pontis ships `PontisClient` and `useAgentUIAction`, and
 * `pontis/docs/12-sdk.md` says in as many words that the package is a wrapper
 * and not the contract. Taking it would make the editor — which *is* the base
 * product — fail to build without a private Pontis checkout linked, and §7.3
 * promises the base product stays usable without Pontis. So the contract is
 * implemented here against the three documented routes, and
 * `apps/pontis-sidecar/src/ui.test.ts` drives the real ones so a drift in any of
 * them fails a test rather than quietly killing the channel.
 *
 * # Why `fetch` and not `EventSource`
 *
 * D9. Forma's browser session is a provider JWT presented as a bearer, and
 * `EventSource` cannot set a header. Putting it in the query string would write
 * a live credential into access logs and `Referer`, against an invariant Forma
 * states outright. So the stream is read as a `ReadableStream` and the framing —
 * about a dozen lines — is here.
 *
 * # One registration per window
 *
 * Every UI request names its window (`x-forma-window`, see `window.ts`) and the
 * stream also names the agent session this window's chat is attached to
 * (`x-forma-agent-session`). The sidecar keys the registration by the window and
 * binds the stream to the session once it has checked the session is this
 * person's, which is what lets an agent's action reach the window that is
 * talking to it rather than whichever is newest.
 */
import { agentBinding, windowId as thisWindow } from "./window";

/** What the sidecar sends. Carries an id and its input; never code, never a selector. */
export interface DeliveredAction {
  readonly invocationId: string;
  readonly capabilityId: string;
  readonly input: unknown;
}

/**
 * What a handler answers.
 *
 * `handled` is the only success. `rejected` is a handler that understood the
 * action and declined it — a layer that is not in the design the person has
 * open — and is a better answer than handling it somewhere unexpected.
 */
export type HandlerOutcome =
  | { readonly outcome: "handled"; readonly detail?: string }
  | { readonly outcome: "rejected"; readonly detail: string };

export type ActionHandler = (input: unknown) => HandlerOutcome | Promise<HandlerOutcome>;

export interface ChannelOptions {
  /** The handlers, keyed by capability id. Their keys are what gets registered. */
  readonly handlers: Readonly<Record<string, ActionHandler>>;
  /** The session bearer, read per request: the provider refreshes it underneath. */
  readonly token: () => Promise<string | null>;
  /** Where the UI routes are, same-origin by construction. Only tests pass one. */
  readonly baseUrl?: string;
  readonly fetchImpl?: typeof fetch;
  /** Told what the channel is doing, for the editor's own status line. */
  readonly onState?: (state: ChannelState) => void;
  /**
   * Each `pons.event` frame, parsed and otherwise untouched. The sidecar fans the
   * Device's notifications out to the person's windows on this same stream; what
   * the frames mean is the subscriber's to decide, and none of them is an action.
   */
  readonly onEvent?: (frame: unknown) => void;
  /** This window's id. Defaults to the tab's own (`window.ts`); tests pass one. */
  readonly windowId?: () => Promise<string>;
  /** The agent session the chat is attached to, if any. Defaults to `agentBinding`. */
  readonly agentSession?: () => string | null;
  /**
   * Who is signed in, for noticing that the stored session now belongs to someone
   * else. The token alone cannot say: it is refreshed under the page and changes
   * for the same person.
   */
  readonly subject?: () => Promise<string | null>;
  /** How long a stream may say nothing, not even a heartbeat, before it is treated as dead. Tests only. */
  readonly silenceMs?: number;
  /** How long to wait before reconnecting. Tests only. */
  readonly reconnectMs?: number;
}

export type ChannelState =
  | { readonly at: "connecting" }
  | { readonly at: "connected"; readonly clientId: string }
  | { readonly at: "offline" };

const STREAM_PATH = "/pons/v1/ui/stream";
const ACTIONS_PATH = "/pons/v1/ui/actions";
const ACK_PATH = "/pons/v1/ui/ack";

/**
 * How long to wait before reconnecting a stream that ended.
 *
 * `EventSource` would have retried on its own; reading the stream with `fetch`
 * means owning that. Fixed rather than backing off: the only thing on the other
 * end is a sidecar on the same host as the proxy, and a channel that takes
 * minutes to come back after a restart is one the person finds broken.
 */
export const RECONNECT_MS = 2_000;

/**
 * How long a stream may be silent before it is abandoned and reopened.
 *
 * The sidecar sends a `: keep-alive` comment every 15 s (`pontis/packages/sdk-server`
 * `HEARTBEAT_MS`), so two missed beats and a margin is a stream nothing is writing to. A proxy
 * that keeps the browser's response open after its upstream died — Vite's dev proxy does, which
 * is F-40-4 — never ends the stream, so "it ended" alone cannot be the only way to notice, and
 * until this the editor stayed registered with a sidecar that no longer knew it, deaf, with
 * nothing on screen saying so. Caddy is reasoned about in the finding; the watchdog does not
 * depend on the answer.
 */
export const SILENCE_MS = 35_000;

export interface Channel {
  close(): void;
  /**
   * Open a fresh stream — because the chat's session changed — and let go of the
   * old one only once the new one is registered, so there is no gap in which an
   * action finds no window.
   */
  refresh(): void;
  /**
   * Re-read who is signed in. If it is no longer the person the stream was opened
   * for, drop that stream at once and open one as the new person: the old one is
   * registered as someone whose requests this window can no longer make, and
   * their agent's actions would time out against it (F-40-15).
   */
  recheck(): Promise<void>;
}

/**
 * Connect, register the handlers and keep the stream open until `close`.
 *
 * Returns immediately; the connection happens in the background and reports
 * itself through `onState`. A channel that cannot connect is not an error the
 * editor shows anyone — there may simply be no sidecar — so it retries quietly.
 */
export function openChannel(options: ChannelOptions): Channel {
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);
  const base = options.baseUrl ?? "";
  const actions = Object.keys(options.handlers);
  let closed = false;
  let retry: ReturnType<typeof setTimeout> | undefined;
  /** Every attempt still in flight, so `close` can end them all. */
  const attempts = new Set<AbortController>();
  /** Bumped by `refresh`: an attempt from an earlier one does not reconnect itself. */
  let current = 0;
  /** The stream that is registered right now, and who it was opened for. */
  let live: { readonly abort: AbortController; readonly person: string | null } | undefined;
  const silenceMs = options.silenceMs ?? SILENCE_MS;
  const reconnectMs = options.reconnectMs ?? RECONNECT_MS;

  const state = (next: ChannelState) => {
    if (!closed) options.onState?.(next);
  };

  const window = options.windowId ?? thisWindow;
  const agentSession = options.agentSession ?? (() => agentBinding.get());

  const headers = async (): Promise<Record<string, string>> => {
    const token = await options.token();
    return { "x-forma-window": await window(), ...(token ? { authorization: `Bearer ${token}` } : {}) };
  };

  /**
   * Answer one action.
   *
   * A handler that throws is `failed`, not `rejected`: the two mean different
   * things to the agent, and reporting a bug as a considered refusal would send
   * it off to rephrase something that was never the problem.
   */
  const handle = async (clientId: string, action: DeliveredAction): Promise<void> => {
    const handler = options.handlers[action.capabilityId];
    let ack: { outcome: string; detail?: string };
    if (!handler) {
      ack = { outcome: "not_registered" };
    } else {
      try {
        ack = await handler(action.input);
      } catch (cause) {
        ack = { outcome: "failed", detail: cause instanceof Error ? cause.message : "the handler threw" };
      }
    }
    await fetchImpl(`${base}${ACK_PATH}`, {
      method: "POST",
      headers: { "content-type": "application/json", ...(await headers()) },
      body: JSON.stringify({ clientId, invocationId: action.invocationId, ...ack }),
    }).catch(() => {
      // The sidecar times the invocation out on its own, and it has already
      // told the agent the action failed. Nothing here can improve that.
    });
  };

  const run = async (controller: AbortController, generation: number): Promise<void> => {
    // Nothing is said about connecting while a stream is still registered: a
    // refresh opens the next one beside it, and the status line should not
    // flicker for what the person cannot see.
    if (live === undefined) state({ at: "connecting" });
    // Armed before the request, so a connect that never answers is abandoned too, and re-armed on
    // every chunk, heartbeat comments included: they are what `frames` otherwise swallows.
    let silence: ReturnType<typeof setTimeout> | undefined;
    const listen = () => {
      clearTimeout(silence);
      silence = setTimeout(() => controller.abort(), silenceMs);
    };
    listen();
    try {
      await stream(controller, listen, generation);
    } finally {
      clearTimeout(silence);
    }
  };

  const stream = async (controller: AbortController, heard: () => void, generation: number): Promise<void> => {
    const person = (await options.subject?.()) ?? null;
    const session = agentSession();
    const response = await fetchImpl(`${base}${STREAM_PATH}`, {
      headers: {
        accept: "text/event-stream",
        ...(await headers()),
        ...(session ? { "x-forma-agent-session": session } : {}),
      },
      signal: controller.signal,
    });
    if (!response.ok || !response.body) throw new Error(`the UI stream answered ${response.status}`);

    let clientId: string | null = null;
    for await (const frame of frames(response.body, controller.signal, heard)) {
      if (frame.event === "ui.connected") {
        const announced = parse(frame.data);
        if (!isRecord(announced) || typeof announced.clientId !== "string") continue;
        clientId = announced.clientId;
        // Registered after the stream opens, which is the order the transport
        // expects: until this lands the client handles nothing, and an action
        // arriving in that window is `ui_action_not_registered` rather than a
        // silent drop.
        await fetchImpl(`${base}${ACTIONS_PATH}`, {
          method: "POST",
          headers: { "content-type": "application/json", ...(await headers()) },
          body: JSON.stringify({ clientId, actions }),
        });
        // A refresh asked for while this one was connecting has already
        // replaced it; registering would only make a neighbour of the newer one.
        if (generation !== current) {
          controller.abort();
          return;
        }
        const previous = live;
        live = { abort: controller, person };
        if (previous && previous.abort !== controller) previous.abort.abort();
        state({ at: "connected", clientId });
        continue;
      }
      if (frame.event === "pons.event") {
        const delivered = parse(frame.data);
        if (delivered !== null) options.onEvent?.(delivered);
        continue;
      }
      if (frame.event !== "ui.action" || clientId === null) continue;
      const action = parse(frame.data);
      if (!isRecord(action) || typeof action.invocationId !== "string" || typeof action.capabilityId !== "string") {
        continue;
      }
      // Not awaited: a handler that takes a moment must not hold up the frame
      // after it, and the acknowledgement names its own invocation.
      void handle(clientId, {
        invocationId: action.invocationId,
        capabilityId: action.capabilityId,
        input: action.input,
      });
    }
  };

  /**
   * One attempt, and the retry that follows it if it ends on its own.
   *
   * An attempt that ends because a newer one has taken over — `refresh`, or the
   * server closing a window's older stream when its replacement connects — is not
   * a drop. Retrying it would evict the newer stream, which would retry in turn:
   * the two-second ping-pong this whole module exists to stop. Only the current
   * generation reconnects.
   */
  const start = () => {
    const mine = new AbortController();
    attempts.add(mine);
    const generation = current;
    run(mine, generation)
      .catch(() => undefined)
      .finally(() => {
        attempts.delete(mine);
        if (live?.abort === mine) live = undefined;
        if (closed || generation !== current) return;
        state({ at: "offline" });
        retry = setTimeout(start, reconnectMs);
      });
  };
  start();

  const refresh = () => {
    if (closed) return;
    if (retry !== undefined) clearTimeout(retry);
    current += 1;
    start();
  };

  return {
    close: () => {
      closed = true;
      if (retry !== undefined) clearTimeout(retry);
      for (const attempt of attempts) attempt.abort();
    },
    refresh,
    recheck: async () => {
      if (closed || !options.subject) return;
      const person = (await options.subject()) ?? null;
      if (live === undefined || live.person === person) return;
      live.abort.abort();
      live = undefined;
      refresh();
    },
  };
}

/**
 * Where the channel's two outputs wait for whoever wants them.
 *
 * The channel is opened once, high up, because a UI action is addressed to the
 * window; the chat is mounted in the editor, lower down and only sometimes. A
 * subscription in between keeps the channel from knowing about the chat and the
 * chat from owning the connection — and a frame with nobody listening is simply
 * dropped, because the page it duplicates is the record.
 */
export interface LiveFeed {
  subscribe(listener: { readonly frame?: (frame: unknown) => void; readonly state?: (state: ChannelState) => void }): () => void;
  publish(frame: unknown): void;
  setState(state: ChannelState): void;
  state(): ChannelState;
}

export function createLiveFeed(): LiveFeed {
  const listeners = new Set<{ readonly frame?: (frame: unknown) => void; readonly state?: (state: ChannelState) => void }>();
  let current: ChannelState = { at: "offline" };
  return {
    subscribe: (listener) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    publish: (frame) => {
      for (const listener of [...listeners]) listener.frame?.(frame);
    },
    setState: (state) => {
      current = state;
      for (const listener of [...listeners]) listener.state?.(state);
    },
    state: () => current,
  };
}

/** The window's one feed: `useUiChannel` fills it and the chat reads it. */
export const liveFeed: LiveFeed = createLiveFeed();

interface Frame {
  readonly event: string;
  readonly data: string;
}

/**
 * The SSE framing, which is all this channel needs of it: `event:` and `data:`
 * lines, a blank line ending a frame, and a `:` comment line — the heartbeat —
 * ignored. No `id:` and no `retry:`, because the sidecar sends neither.
 */
export async function* frames(
  body: ReadableStream<Uint8Array>,
  signal?: AbortSignal,
  onChunk?: () => void,
): AsyncGenerator<Frame> {
  const decoder = new TextDecoder();
  const reader = body.getReader();
  let buffer = "";
  try {
    while (!signal?.aborted) {
      const { done, value } = await reader.read();
      if (done) return;
      onChunk?.();
      buffer += decoder.decode(value, { stream: true });
      let boundary = buffer.indexOf("\n\n");
      while (boundary !== -1) {
        const frame = parseFrame(buffer.slice(0, boundary));
        buffer = buffer.slice(boundary + 2);
        if (frame) yield frame;
        boundary = buffer.indexOf("\n\n");
      }
    }
  } finally {
    reader.releaseLock();
  }
}

function parseFrame(block: string): Frame | null {
  let event = "message";
  const data: string[] = [];
  for (const line of block.split("\n")) {
    if (line.startsWith(":") || line.length === 0) continue;
    if (line.startsWith("event:")) event = line.slice("event:".length).trim();
    else if (line.startsWith("data:")) data.push(line.slice("data:".length).trim());
  }
  return data.length > 0 ? { event, data: data.join("\n") } : null;
}

function parse(data: string): unknown {
  try {
    return JSON.parse(data) as unknown;
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
