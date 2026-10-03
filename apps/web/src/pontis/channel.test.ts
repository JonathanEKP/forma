/**
 * The channel's half of the contract: framing, registration, acknowledgement.
 *
 * The wire itself is tested against the real sidecar routes in
 * `apps/pontis-sidecar/src/ui.test.ts`. What this file covers is what that one
 * cannot reach — the browser's side of a stream that ends, a handler that
 * throws, an action for an id nobody registered.
 */
import { describe, expect, it, vi } from "vitest";

import { frames, openChannel, type ChannelState } from "./channel";

const CLIENT = "uic_1";

function stream(chunks: readonly string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  });
}

async function collect(chunks: readonly string[]) {
  const seen: { event: string; data: string }[] = [];
  for await (const frame of frames(stream(chunks))) seen.push(frame);
  return seen;
}

describe("the SSE framing", () => {
  it("reads a frame split across chunks, which is the normal case on a socket", async () => {
    expect(await collect(['event: ui.ac', 'tion\ndata: {"a":1}', "\n\n"])).toEqual([
      { event: "ui.action", data: '{"a":1}' },
    ]);
  });

  it("reads several frames out of one chunk", async () => {
    expect(await collect(['event: a\ndata: 1\n\nevent: b\ndata: 2\n\n'])).toEqual([
      { event: "a", data: "1" },
      { event: "b", data: "2" },
    ]);
  });

  it("ignores the heartbeat comment, which otherwise looks like an empty frame", async () => {
    expect(await collect([": keep-alive\n\n", "event: a\ndata: 1\n\n"])).toEqual([
      { event: "a", data: "1" },
    ]);
  });

  it("drops an incomplete trailing frame rather than guessing at it", async () => {
    expect(await collect(["event: a\ndata: 1\n"])).toEqual([]);
  });
});

/**
 * A fetch that answers the three routes, with the stream held open so the
 * channel behaves as it does against a real sidecar.
 */
function fakeSidecar(options: { actions?: readonly unknown[]; onPost?: (path: string, body: unknown) => void } = {}) {
  const encoder = new TextEncoder();
  let push: ((chunk: string) => void) | undefined;
  const posts: { path: string; body: unknown; headers: Record<string, string> }[] = [];

  const fetchImpl = vi.fn((input: string, init?: RequestInit) => {
    const path = input;
    const headers = (init?.headers ?? {}) as Record<string, string>;
    if (path.endsWith("/stream")) {
      const body = new ReadableStream<Uint8Array>({
        start(controller) {
          push = (chunk) => controller.enqueue(encoder.encode(chunk));
          push(`event: ui.connected\ndata: ${JSON.stringify({ clientId: CLIENT })}\n\n`);
          for (const action of options.actions ?? []) {
            push(`event: ui.action\ndata: ${JSON.stringify(action)}\n\n`);
          }
        },
      });
      return Promise.resolve(new Response(body, { status: 200 }));
    }
    const sent = init?.body;
    const body: unknown = typeof sent === "string" ? JSON.parse(sent) : undefined;
    posts.push({ path, body, headers });
    options.onPost?.(path, body);
    return Promise.resolve(new Response('{"ok":true}', { status: 200 }));
  });

  return { fetchImpl: fetchImpl as unknown as typeof fetch, posts, push: (chunk: string) => push?.(chunk) };
}

const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

async function until(condition: () => boolean, attempts = 50): Promise<void> {
  for (let i = 0; i < attempts; i += 1) {
    if (condition()) return;
    await settled();
  }
  throw new Error("the channel never got there");
}

describe("the channel", () => {
  it("registers its handler ids after the stream announces the client, not before", async () => {
    const sidecar = fakeSidecar();
    const states: ChannelState[] = [];
    const channel = openChannel({
      handlers: { "ui.setZoom": () => ({ outcome: "handled" }) },
      token: () => Promise.resolve("session-token"),
      baseUrl: "http://sidecar.test",
      fetchImpl: sidecar.fetchImpl,
      onState: (state) => states.push(state),
    });
    try {
      await until(() => sidecar.posts.length > 0);
      expect(sidecar.posts[0]?.path).toBe("http://sidecar.test/pons/v1/ui/actions");
      expect(sidecar.posts[0]?.body).toEqual({ clientId: CLIENT, actions: ["ui.setZoom"] });
      // The bearer travels in a header. A query string would put a live
      // credential in access logs, which is the whole reason this is not an
      // `EventSource`.
      expect(sidecar.posts[0]?.headers.authorization).toBe("Bearer session-token");
      await until(() => states.some((state) => state.at === "connected"));
      expect(states.at(-1)).toEqual({ at: "connected", clientId: CLIENT });
    } finally {
      channel.close();
    }
  });

  it("acknowledges an action with what its handler answered", async () => {
    const sidecar = fakeSidecar({
      actions: [{ invocationId: "inv_1", capabilityId: "ui.setZoom", input: { zoom: 2 } }],
    });
    const seen: unknown[] = [];
    const channel = openChannel({
      handlers: {
        "ui.setZoom": (input) => {
          seen.push(input);
          return { outcome: "handled", detail: "zoom 2" };
        },
      },
      token: () => Promise.resolve("session-token"),
      baseUrl: "http://sidecar.test",
      fetchImpl: sidecar.fetchImpl,
    });
    try {
      await until(() => sidecar.posts.some(({ path }) => path.endsWith("/ack")));
      expect(seen).toEqual([{ zoom: 2 }]);
      expect(sidecar.posts.find(({ path }) => path.endsWith("/ack"))?.body).toEqual({
        clientId: CLIENT,
        invocationId: "inv_1",
        outcome: "handled",
        detail: "zoom 2",
      });
    } finally {
      channel.close();
    }
  });

  it("answers not_registered for an id it has no handler for, rather than letting it time out", async () => {
    const sidecar = fakeSidecar({
      actions: [{ invocationId: "inv_2", capabilityId: "ui.openDesign", input: {} }],
    });
    const channel = openChannel({
      handlers: { "ui.setZoom": () => ({ outcome: "handled" }) },
      token: () => Promise.resolve(null),
      baseUrl: "http://sidecar.test",
      fetchImpl: sidecar.fetchImpl,
    });
    try {
      await until(() => sidecar.posts.some(({ path }) => path.endsWith("/ack")));
      expect(sidecar.posts.find(({ path }) => path.endsWith("/ack"))?.body).toEqual({
        clientId: CLIENT,
        invocationId: "inv_2",
        outcome: "not_registered",
      });
    } finally {
      channel.close();
    }
  });

  it("reports a handler that throws as failed, not as a considered refusal", async () => {
    const sidecar = fakeSidecar({
      actions: [{ invocationId: "inv_3", capabilityId: "ui.setZoom", input: {} }],
    });
    const channel = openChannel({
      handlers: {
        "ui.setZoom": () => {
          throw new Error("the store blew up");
        },
      },
      token: () => Promise.resolve(null),
      baseUrl: "http://sidecar.test",
      fetchImpl: sidecar.fetchImpl,
    });
    try {
      await until(() => sidecar.posts.some(({ path }) => path.endsWith("/ack")));
      expect(sidecar.posts.find(({ path }) => path.endsWith("/ack"))?.body).toEqual({
        clientId: CLIENT,
        invocationId: "inv_3",
        outcome: "failed",
        detail: "the store blew up",
      });
    } finally {
      channel.close();
    }
  });

  it("goes offline and stops when closed, rather than reconnecting forever", async () => {
    const fetchImpl = vi.fn(() => Promise.reject(new Error("no sidecar")));
    const states: ChannelState[] = [];
    const channel = openChannel({
      handlers: { "ui.setZoom": () => ({ outcome: "handled" }) },
      token: () => Promise.resolve(null),
      fetchImpl,
      onState: (state) => states.push(state),
    });
    await until(() => states.some((state) => state.at === "offline"));
    channel.close();
    const attempts = fetchImpl.mock.calls.length;
    // A closed channel is closed: no sidecar is not an error the editor shows
    // anyone, but it is not a reason to keep asking after the editor is gone.
    await settled();
    expect(fetchImpl.mock.calls.length).toBe(attempts);
  });
});
