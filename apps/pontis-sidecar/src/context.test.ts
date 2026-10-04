/**
 * `GET /agent/context`: the one fact the editor needs to read its own event
 * stream and has no way to learn (plan §11 step 10b).
 */
import { strict as assert } from 'node:assert';
import { describe, it } from 'node:test';

import { SUBJECT_TOKEN_HEADER } from './boundary.ts';
import { envelope, fakeForma } from './fake-forma.ts';
import { silentLog } from './log.ts';
import { startSidecar, type Sidecar } from './sidecar.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const CONTEXT = '22222222-2222-4222-8222-222222222222';

async function withSidecar(body: (sidecar: Sidecar) => Promise<void>) {
  const server = await fakeForma((request) => {
    if (request.path !== '/api/v1/service/resolve') return { status: 404, body: envelope('NO_SUCH_OPERATION', 'not_found') };
    return request.headers[SUBJECT_TOKEN_HEADER] === 'session-user'
      ? { status: 200, body: { subject: USER, account_id: CONTEXT, role: 'owner' } }
      : { status: 401, body: envelope('UNAUTHORIZED', undefined) };
  });
  const sidecar = await startSidecar({ formaUrl: server.url, serviceCredential: 'forma_svc_test', host: '127.0.0.1', port: 0, log: silentLog });
  try {
    await body(sidecar);
  } finally {
    await sidecar.close();
    await server.close();
  }
}

describe('GET /agent/context', () => {
  it('tells a person the account their session is bound to, and nothing else', async () => {
    await withSidecar(async (sidecar) => {
      const response = await fetch(`${sidecar.origin}/agent/context`, { headers: { authorization: 'Bearer session-user' } });
      assert.equal(response.status, 200);
      assert.deepEqual(await response.json(), { applicationContextId: CONTEXT });
    });
  });

  it('returns the same typed 401 for a missing or invalid session', async () => {
    await withSidecar(async (sidecar) => {
      for (const headers of [{}, { authorization: 'Bearer nobody' }]) {
        for (const [path, method] of [['/agent/context', 'GET'], ['/agent/intent', 'POST']] as const) {
          const response = await fetch(`${sidecar.origin}${path}`, {
            method,
            headers: { ...headers, ...(method === 'POST' ? { 'content-type': 'application/json' } : {}) },
            ...(method === 'POST' ? { body: '{}' } : {}),
          });
          assert.equal(response.status, 401);
          assert.deepEqual(await response.json(), { error: { code: 'unauthenticated' } });
        }
      }
    });
  });

  it('answers an over-limit intent body with its typed 413 refusal', async () => {
    await withSidecar(async (sidecar) => {
      const response = await fetch(`${sidecar.origin}/agent/intent`, {
        method: 'POST',
        headers: { authorization: 'Bearer session-user', 'content-type': 'application/json' },
        body: JSON.stringify({ type: 'session.input', payload: { sessionId: 'ses_1', text: 'x'.repeat(64 * 1024) } }),
      });
      assert.equal(response.status, 413);
      assert.deepEqual(await response.json(), { error: { code: 'message_too_large' } });
    });
  });

  it('answers a read only', async () => {
    await withSidecar(async (sidecar) => {
      const response = await fetch(`${sidecar.origin}/agent/context`, { method: 'POST', headers: { authorization: 'Bearer session-user' } });
      assert.equal(response.status, 405);
    });
  });
});
