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
 */

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

export interface Channel {
  close(): void;
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
  let abort: AbortController | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;

  const state = (next: ChannelState) => {
    if (!closed) options.onState?.(next);
  };

  const headers = async (): Promise<Record<string, string>> => {
    const token = await options.token();
    return token ? { authorization: `Bearer ${token}` } : {};
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

  const run = async (): Promise<void> => {
    state({ at: "connecting" });
    abort = new AbortController();
    const response = await fetchImpl(`${base}${STREAM_PATH}`, {
      headers: { accept: "text/event-stream", ...(await headers()) },
      signal: abort.signal,
    });
    if (!response.ok || !response.body) throw new Error(`the UI stream answered ${response.status}`);

    let clientId: string | null = null;
    for await (const frame of frames(response.body, abort.signal)) {
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

  const loop = () => {
    run()
      .catch(() => undefined)
      .finally(() => {
        if (closed) return;
        state({ at: "offline" });
        retry = setTimeout(loop, RECONNECT_MS);
      });
  };
  loop();

  return {
    close: () => {
      closed = true;
      if (retry !== undefined) clearTimeout(retry);
      abort?.abort();
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
export async function* frames(body: ReadableStream<Uint8Array>, signal?: AbortSignal): AsyncGenerator<Frame> {
  const decoder = new TextDecoder();
  const reader = body.getReader();
  let buffer = "";
  try {
    while (!signal?.aborted) {
      const { done, value } = await reader.read();
      if (done) return;
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
