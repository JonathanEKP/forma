/**
 * A stand-in for the editor's window: one client of the UI channel, speaking the wire the editor's
 * `apps/web/src/pontis/channel.ts` speaks, against the sidecar's real routes.
 *
 * Shared by `ui.test.ts`, which drives every refusal and timeout of the channel, and by the real
 * Codex run (`real-codex.test.ts`, scenario S7), which needs a window connected before a model's
 * `ui.*` call can ever be `handled`. The framing is deliberately a copy of the editor's rather
 * than an import: the two share no module graph, and if the frames stop matching, the copy under
 * test is the one that fails.
 */
import { strict as assert } from 'node:assert';
import { request as httpRequest, type IncomingMessage } from 'node:http';

/** One connected window, speaking the wire the editor's channel speaks. */
export interface Window {
  readonly clientId: string;
  /** Resolves with the next action delivered, or rejects after `ms`. */
  next(ms?: number): Promise<{ invocationId: string; capabilityId: string; input: unknown }>;
  ack(invocationId: string, outcome: string, detail?: string): Promise<number>;
  register(actions: readonly string[]): Promise<number>;
  close(): void;
}

export async function connect(sidecarOrigin: string, token: string, origin: string, actions: readonly string[]): Promise<Window> {
  const host = new URL(origin).host;
  const abort = new AbortController();
  const response = await send(`${sidecarOrigin}/pons/v1/ui/stream`, {
    headers: { authorization: `Bearer ${token}`, origin, host, accept: 'text/event-stream' },
    signal: abort.signal,
  });
  assert.equal(response.statusCode, 200, 'the stream opened');

  const delivered: { invocationId: string; capabilityId: string; input: unknown }[] = [];
  const waiting: ((action: { invocationId: string; capabilityId: string; input: unknown }) => void)[] = [];
  let announced: ((clientId: string) => void) | undefined;
  const connected = new Promise<string>((resolve) => {
    announced = resolve;
  });

  void (async () => {
    for await (const frame of frames(response, abort.signal)) {
      const data: unknown = JSON.parse(frame.data);
      if (frame.event === 'ui.connected' && isRecord(data) && typeof data.clientId === 'string') {
        announced?.(data.clientId);
      } else if (frame.event === 'ui.action' && isRecord(data)) {
        const action = data as unknown as { invocationId: string; capabilityId: string; input: unknown };
        const next = waiting.shift();
        if (next) next(action);
        else delivered.push(action);
      }
    }
  })().catch(() => undefined);

  const clientId = await connected;
  const post = async (path: string, body: unknown): Promise<number> => {
    const answer = await send(`${sidecarOrigin}${path}`, {
      method: 'POST',
      headers: { authorization: `Bearer ${token}`, origin, host, 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    await drain(answer);
    return answer.statusCode ?? 0;
  };

  const window: Window = {
    clientId,
    next: (ms = 1_000) =>
      new Promise((resolve, reject) => {
        const ready = delivered.shift();
        if (ready) return resolve(ready);
        const timer = setTimeout(() => reject(new Error('no action arrived')), ms);
        waiting.push((action) => {
          clearTimeout(timer);
          resolve(action);
        });
      }),
    ack: (invocationId, outcome, detail) =>
      post('/pons/v1/ui/ack', { clientId, invocationId, outcome, ...(detail ? { detail } : {}) }),
    register: (actions) => post('/pons/v1/ui/actions', { clientId, actions }),
    close: () => abort.abort(),
  };
  await window.register(actions);
  return window;
}

/**
 * The SSE framing, the same shape as the editor's `frames` in
 * `apps/web/src/pontis/channel.ts`. Kept here rather than imported because the
 * two processes share no module graph, and the duplication is the point: if the
 * frames stop matching, the copy under test here is the one that fails.
 */
export async function* frames(body: AsyncIterable<Uint8Array>, signal: AbortSignal) {
  const decoder = new TextDecoder();
  let buffer = '';
  for await (const value of body) {
    if (signal.aborted) return;
    buffer += decoder.decode(value, { stream: true });
    let boundary = buffer.indexOf('\n\n');
    while (boundary !== -1) {
      const block = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      let event = 'message';
      const data: string[] = [];
      for (const line of block.split('\n')) {
        if (line.startsWith(':') || line.length === 0) continue;
        if (line.startsWith('event:')) event = line.slice(6).trim();
        else if (line.startsWith('data:')) data.push(line.slice(5).trim());
      }
      if (data.length > 0) yield { event, data: data.join('\n') };
      boundary = buffer.indexOf('\n\n');
    }
  }
}

/**
 * One request, through `node:http` rather than `fetch`.
 *
 * `fetch` silently drops a `Host` header, and these tests need to set it: they
 * are standing in for Caddy, which passes the browser's `Host` through, and the
 * transport's DNS-rebinding defence is a check on exactly that value. A test
 * that could not set it would have had to declare the sidecar's own loopback
 * address as the editor's origin, which is not the shape anything deploys in.
 */
export function send(
  url: string,
  options: { method?: string; headers: Record<string, string>; body?: string; signal?: AbortSignal },
): Promise<IncomingMessage> {
  return new Promise((resolve, reject) => {
    const outgoing = httpRequest(
      url,
      { method: options.method ?? 'GET', headers: options.headers, ...(options.signal ? { signal: options.signal } : {}) },
      resolve,
    );
    outgoing.on('error', reject);
    outgoing.end(options.body);
  });
}

/** A response body read to the end, for the small JSON answers. */
export async function drain(response: IncomingMessage): Promise<void> {
  // Read and discard: an unread response keeps the socket open, and these tests
  // close the sidecar while asserting that it closes cleanly.
  response.resume();
  await new Promise<void>((resolve) => response.once('end', resolve));
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

