/**
 * The reader's half of `pontis/docs/08` § Client rules. The wire is the real
 * route's, driven in `apps/pontis-sidecar/src/local-run.test.ts`; what is here is
 * what that cannot reach — a gap, a duplicate, a page for the wrong scope.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  MAX_RESYNCS,
  openEventStream,
  parseEventPage,
  reconcile,
  type DurableEvent,
  type StreamState,
} from "./events";

const CONTEXT = "22222222-2222-4222-8222-222222222222";

const event = (sequence: number, type = "turn.started"): DurableEvent => ({
  sequence,
  type,
  sessionId: "ses_1",
  payload: {},
});

const urlOf = (input: RequestInfo | URL): string =>
  typeof input === "string" ? input : input instanceof URL ? input.href : input.url;

const wire = (sequence: number) => ({ sequence, type: "turn.started", sessionId: "ses_1", payload: {} });

describe("parsing a page", () => {
  const page = (overrides: Record<string, unknown> = {}) => ({
    protocol: "pons/1.0",
    scope: { kind: "application_context", applicationContextId: CONTEXT },
    events: [wire(1), wire(2)],
    head: 2,
    ...overrides,
  });

  it("reads the events and the head", () => {
    const parsed = parseEventPage(page(), CONTEXT);
    expect(parsed?.head).toBe(2);
    expect(parsed?.events.map((e) => e.sequence)).toEqual([1, 2]);
  });

  it("refuses a page for another scope, whose sequences are not comparable", () => {
    expect(parseEventPage(page({ scope: { kind: "principal" } }), CONTEXT)).toBeNull();
    expect(
      parseEventPage(page({ scope: { kind: "application_context", applicationContextId: "other" } }), CONTEXT),
    ).toBeNull();
  });

  it("refuses the whole page when one event is malformed, rather than skipping it into a false gap", () => {
    expect(parseEventPage(page({ events: [wire(1), { type: "x" }] }), CONTEXT)).toBeNull();
  });

  it("refuses what is not a page at all", () => {
    expect(parseEventPage(null, CONTEXT)).toBeNull();
    expect(parseEventPage({ events: [] }, CONTEXT)).toBeNull();
  });
});

describe("reconciling against the cursor", () => {
  it("takes the next sequences in order and advances the cursor", () => {
    const result = reconcile(0, [event(1), event(2), event(3)]);
    expect(result.fresh.map((e) => e.sequence)).toEqual([1, 2, 3]);
    expect(result).toMatchObject({ cursor: 3, gap: false, duplicates: 0 });
  });

  it("ignores a sequence already applied", () => {
    const result = reconcile(2, [event(1), event(2), event(3)]);
    expect(result.fresh.map((e) => e.sequence)).toEqual([3]);
    expect(result).toMatchObject({ cursor: 3, gap: false, duplicates: 2 });
  });

  it("stops at a gap and takes nothing after it", () => {
    const result = reconcile(2, [event(3), event(5), event(6)]);
    expect(result.fresh.map((e) => e.sequence)).toEqual([3]);
    expect(result).toMatchObject({ cursor: 3, gap: true });
  });
});

describe("the polling stream", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  /** A fetch that answers each call with the next page, recording the cursor it was asked from. */
  function pages(...answers: (readonly number[] | "down")[]) {
    const asked: string[] = [];
    const headers: (string | null)[] = [];
    let call = 0;
    const fetchImpl = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const url = new URL(urlOf(input), "http://x");
      asked.push(`${url.searchParams.get("scope")}|${url.searchParams.get("applicationContextId")}|${url.searchParams.get("after")}`);
      headers.push(new Headers(init?.headers).get("authorization"));
      const answer = answers[Math.min(call, answers.length - 1)];
      call += 1;
      if (answer === "down") return Promise.reject(new TypeError("fetch failed"));
      const head = Math.max(0, ...(answer ?? []));
      return Promise.resolve(
        new Response(
          JSON.stringify({
            scope: { kind: "application_context", applicationContextId: CONTEXT },
            events: (answer ?? []).map(wire),
            head,
          }),
          { status: 200 },
        ),
      );
    });
    return { fetchImpl, asked, headers };
  }

  function open(source: ReturnType<typeof pages>) {
    const applied: number[] = [];
    const states: StreamState["at"][] = [];
    const stream = openEventStream({
      applicationContextId: CONTEXT,
      token: () => Promise.resolve("session-token"),
      fetchImpl: source.fetchImpl,
      onEvents: (events) => applied.push(...events.map((e) => e.sequence)),
      onState: (state) => states.push(state.at),
    });
    return { applied, states, stream };
  }

  it("asks for the context's scope from cursor 0, with the bearer in a header", async () => {
    const source = pages([1, 2]);
    const { applied, stream } = open(source);
    await vi.advanceTimersByTimeAsync(0);
    expect(applied).toEqual([1, 2]);
    expect(source.asked[0]).toBe(`application_context|${CONTEXT}|0`);
    expect(source.headers[0]).toBe("Bearer session-token");
    const first = source.fetchImpl.mock.calls[0]?.[0];
    expect(first === undefined ? "" : urlOf(first)).not.toContain("session-token");
    stream.close();
  });

  it("resumes from the cursor on the next poll, and ignores a duplicate that comes back", async () => {
    // The second answer repeats 2, as a page from a lagging replica would.
    const source = pages([1, 2], [2, 3]);
    const { applied, stream } = open(source);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(2_000);
    expect(source.asked[1]).toBe(`application_context|${CONTEXT}|2`);
    expect(applied).toEqual([1, 2, 3]);
    stream.close();
  });

  it("resyncs from the cursor on a gap instead of skipping, and applies the repaired history in order", async () => {
    // 1 arrives, then a page that jumps to 3; the immediate re-read has 2 and 3.
    const source = pages([1], [3], [2, 3]);
    const { applied, stream } = open(source);
    await vi.advanceTimersByTimeAsync(0); // 1
    expect(applied).toEqual([1]);
    // The poll that meets the gap re-reads at once, so both land in this window.
    await vi.advanceTimersByTimeAsync(2_000);
    expect(source.asked.slice(1)).toEqual([
      `application_context|${CONTEXT}|1`, // the page that jumped to 3
      `application_context|${CONTEXT}|1`, // the resync, from the same cursor
    ]);
    // 3 was never applied ahead of 2.
    expect(applied).toEqual([1, 2, 3]);
    stream.close();
  });

  it("reports a hole that survives its resyncs, and keeps the cursor where it was", async () => {
    const source = pages([1], [3]);
    const { applied, states, stream } = open(source);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(2_000);
    for (let i = 0; i < MAX_RESYNCS; i += 1) await vi.advanceTimersByTimeAsync(0);
    expect(states.at(-1)).toBe("gap");
    expect(applied).toEqual([1]);
    stream.close();
  });

  it("says offline when the page cannot be read, and recovers", async () => {
    const source = pages("down", [1]);
    const { applied, states, stream } = open(source);
    await vi.advanceTimersByTimeAsync(0);
    expect(states.at(-1)).toBe("offline");
    await vi.advanceTimersByTimeAsync(6_000);
    expect(states.at(-1)).toBe("live");
    expect(applied).toEqual([1]);
    stream.close();
  });

  it("stops reading once closed", async () => {
    const source = pages([1]);
    const { stream } = open(source);
    await vi.advanceTimersByTimeAsync(0);
    stream.close();
    const calls = source.fetchImpl.mock.calls.length;
    await vi.advanceTimersByTimeAsync(30_000);
    expect(source.fetchImpl.mock.calls.length).toBe(calls);
  });
});
