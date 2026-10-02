/**
 * Test support: an HTTP server standing in for forma-server's Boundary F, which
 * records every request and answers from a table. For the unit tests only — the
 * local run (`local-run.test.ts`) uses the real forma-server.
 */
import { createServer, type IncomingHttpHeaders } from 'node:http';

export interface Recorded {
  readonly method: string;
  readonly path: string;
  readonly headers: IncomingHttpHeaders;
  readonly body: string;
}

export interface Answer {
  readonly status: number;
  readonly body: unknown;
  /** Writes the body without a content-length, so only a streaming reader can bound it. */
  readonly chunked?: boolean;
}

export interface FakeForma {
  readonly url: string;
  readonly requests: Recorded[];
  close(): Promise<void>;
}

export async function fakeForma(answer: (request: Recorded) => Answer | Promise<Answer>): Promise<FakeForma> {
  const requests: Recorded[] = [];
  const server = createServer((request, response) => {
    const chunks: Buffer[] = [];
    request.on('data', (chunk: Buffer) => chunks.push(chunk));
    request.on('end', () => {
      const recorded: Recorded = {
        method: request.method ?? '',
        path: request.url ?? '',
        headers: request.headers,
        body: Buffer.concat(chunks).toString('utf8'),
      };
      requests.push(recorded);
      void Promise.resolve(answer(recorded)).then((reply) => {
        if (response.destroyed) return;
        const bytes = Buffer.from(JSON.stringify(reply.body));
        response.writeHead(reply.status, {
          'content-type': 'application/json',
          ...(reply.chunked ? {} : { 'content-length': String(bytes.length) }),
        });
        response.end(bytes);
      });
    });
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('the fake did not bind');
  return {
    url: `http://127.0.0.1:${address.port}`,
    requests,
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections();
        server.close(() => resolve());
      }),
  };
}

/** A Forma §32 envelope, as `routes/pontis.rs` writes it. */
export function envelope(code: string, capabilityError: string | undefined, message = `${code} happened`): unknown {
  return {
    error: {
      code,
      message,
      hint: `what to do about ${code}`,
      ...(capabilityError ? { capability_error: capabilityError } : {}),
    },
  };
}
