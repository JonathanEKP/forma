/**
 * The durable event stream, read the way `pontis/docs/08` § Client rules says to
 * (plan §11 step 10b).
 *
 * # It is a page, not a stream
 *
 * `GET /pons/v1/events` answers one JSON document — `{ events, head }` after a
 * cursor — and the "live half" the doc mentions is the application's own
 * delivery channel, which Forma's sidecar does not have. So this polls. That is
 * also why `frames` from `channel.ts` is not used here: nothing on this route is
 * SSE, and reaching for the parser would be reading a body that is not framed.
 * The cost is that `agent.output.delta` — which is never durable — does not
 * exist for this reader: output arrives whole, as `agent.output.completed`.
 *
 * # The three rules that matter
 *
 * Applied to the one scope this reads, the person's application context:
 * a `sequence` already applied is ignored; a `sequence` past the next expected
 * one is a gap, and the answer is to ask again from the cursor rather than skip
 * ahead; and the cursor advances only once an event has been handed on. A page
 * that fails any of those is never half-applied.
 */

/** One durable event: the fields the chat reads. Anything else is not trusted or kept. */
export interface DurableEvent {
  readonly sequence: number;
  readonly type: string;
  readonly sessionId?: string;
  readonly payload: Readonly<Record<string, unknown>>;
}

export interface EventPage {
  readonly events: readonly DurableEvent[];
  readonly head: number;
}

export type StreamState =
  | { readonly at: "connecting" }
  | { readonly at: "live" }
  | { readonly at: "offline" }
  /** The server's history has a hole the cursor cannot get past. Said, not hidden. */
  | { readonly at: "gap"; readonly after: number };

const EVENTS_PATH = "/pons/v1/events";

/** Between polls once caught up. A turn is seconds long; this is what "streaming" costs here. */
export const POLL_MS = 1_500;

/** Before retrying a page that could not be read at all. */
export const RETRY_MS = 5_000;

/**
 * How many times in a row a gap is re-read from the cursor before it is
 * reported. One re-read is the normal cure — a replica that was a page behind —
 * and a hole that survives three is the server's, not the network's.
 */
export const MAX_RESYNCS = 3;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function toEvent(value: unknown): DurableEvent | null {
  if (!isRecord(value)) return null;
  const { sequence, type, sessionId, payload } = value;
  if (typeof sequence !== "number" || !Number.isSafeInteger(sequence) || sequence < 1) return null;
  if (typeof type !== "string") return null;
  return {
    sequence,
    type,
    ...(typeof sessionId === "string" ? { sessionId } : {}),
    payload: isRecord(payload) ? payload : {},
  };
}

/**
 * Read one answer, or `null` if it is not the page that was asked for.
 *
 * The scope is checked against the context the request named: sequences are
 * never comparable across scopes, so a page for another one would corrupt the
 * cursor silently if it were accepted.
 */
export function parseEventPage(body: unknown, applicationContextId: string): EventPage | null {
  if (!isRecord(body) || !Array.isArray(body.events) || typeof body.head !== "number") return null;
  const { scope } = body;
  if (
    !isRecord(scope) ||
    scope.kind !== "application_context" ||
    scope.applicationContextId !== applicationContextId
  ) {
    return null;
  }
  const events: DurableEvent[] = [];
  for (const raw of body.events as unknown[]) {
    const event = toEvent(raw);
    // One malformed event makes the page untrustworthy as a sequence: skipping
    // it would look like a gap at best and a silent loss at worst.
    if (!event) return null;
    events.push(event);
  }
  return { events, head: body.head };
}

export interface Reconciled {
  /** What to apply, in order. */
  readonly fresh: readonly DurableEvent[];
  readonly cursor: number;
  /** A sequence past the next expected one was met; nothing after it was taken. */
  readonly gap: boolean;
  /** Events at or below the cursor that were dropped. */
  readonly duplicates: number;
}

/**
 * Fold a page into the cursor.
 *
 * Pure, so the rules above are tested without a clock or a network. Stops at the
 * first gap rather than taking what follows it: applying event 7 when 6 is
 * missing is "applying a state change out of order", which the contract forbids.
 */
export function reconcile(cursor: number, events: readonly DurableEvent[]): Reconciled {
  const fresh: DurableEvent[] = [];
  let next = cursor;
  let duplicates = 0;
  for (const event of events) {
    if (event.sequence <= next) {
      duplicates += 1;
      continue;
    }
    if (event.sequence !== next + 1) return { fresh, cursor: next, gap: true, duplicates };
    fresh.push(event);
    next = event.sequence;
  }
  return { fresh, cursor: next, gap: false, duplicates };
}

export interface EventStreamOptions {
  readonly applicationContextId: string;
  /** The session bearer, read per request: the provider refreshes it underneath. */
  readonly token: () => Promise<string | null>;
  /** Called with each batch of newly applied events, in order. Never with a duplicate. */
  readonly onEvents: (events: readonly DurableEvent[]) => void;
  readonly onState?: (state: StreamState) => void;
  /** Same-origin by construction. Only tests pass one. */
  readonly baseUrl?: string;
  readonly fetchImpl?: typeof fetch;
  readonly pollMs?: number;
  readonly retryMs?: number;
}

export interface EventStream {
  close(): void;
}

/**
 * Read the context's history from the beginning, then keep reading from where it
 * left off until closed.
 *
 * From the beginning on purpose: the cursor starts at 0, which replays the whole
 * context. A person who reloads mid-session gets the transcript and any approval
 * still waiting, which a stream that began at "now" would lose — and an approval
 * is the one thing an agent is blocked on.
 */
export function openEventStream(options: EventStreamOptions): EventStream {
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);
  const base = options.baseUrl ?? "";
  const pollMs = options.pollMs ?? POLL_MS;
  const retryMs = options.retryMs ?? RETRY_MS;
  let closed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let abort: AbortController | undefined;
  let cursor = 0;
  let resyncs = 0;
  let last: StreamState["at"] | undefined;

  const state = (next: StreamState) => {
    if (closed || last === next.at) return;
    last = next.at;
    options.onState?.(next);
  };

  const read = async (): Promise<EventPage | null> => {
    abort = new AbortController();
    const token = await options.token();
    const query = new URLSearchParams({
      scope: "application_context",
      applicationContextId: options.applicationContextId,
      after: String(cursor),
    });
    const response = await fetchImpl(`${base}${EVENTS_PATH}?${query.toString()}`, {
      headers: token ? { authorization: `Bearer ${token}` } : {},
      signal: abort.signal,
    });
    if (!response.ok) return null;
    const body: unknown = await response.json();
    return parseEventPage(body, options.applicationContextId);
  };

  const schedule = (ms: number) => {
    if (!closed) timer = setTimeout(() => void tick(), ms);
  };

  const tick = async (): Promise<void> => {
    let page: EventPage | null;
    try {
      page = await read();
    } catch {
      page = null;
    }
    if (closed) return;
    if (!page) {
      state({ at: "offline" });
      return schedule(retryMs);
    }

    const result = reconcile(cursor, page.events);
    if (result.fresh.length > 0) options.onEvents(result.fresh);
    cursor = result.cursor;

    if (result.gap) {
      resyncs += 1;
      // Re-read from the cursor straight away: the first answer was a page
      // that skipped something, and waiting does not make it less so.
      if (resyncs < MAX_RESYNCS) return schedule(0);
      state({ at: "gap", after: cursor });
      return schedule(retryMs);
    }
    resyncs = 0;
    state({ at: "live" });
    // More behind the head than one page held: keep reading without the pause.
    // A page that moved nothing is not behind, whatever `head` says — the
    // alternative is asking for the same page forever.
    schedule(result.fresh.length > 0 && cursor < page.head ? 0 : pollMs);
  };

  state({ at: "connecting" });
  void tick();

  return {
    close: () => {
      closed = true;
      if (timer !== undefined) clearTimeout(timer);
      abort?.abort();
    },
  };
}
