/**
 * The Boundary F client against a stand-in forma-server: what it sends, and
 * how each refusal becomes a Pontis registry error (plan §7.1).
 */
import { strict as assert } from 'node:assert';
import { after, describe, it } from 'node:test';

import { CapabilityError } from '@pontis/core';

import {
  APPLICATION_CONTEXT_HEADER,
  FormaBoundary,
  INVOCATION_HEADER,
  REPRESENTED_USER_HEADER,
  SUBJECT_TOKEN_HEADER,
  TIMEOUT_HEADER,
  refusal,
} from './boundary.ts';
import { envelope, fakeForma, type Answer } from './fake-forma.ts';
import type { LogFields } from './log.ts';

const CREDENTIAL = 'forma_svc_sentinel-credential-never-logged';
const WHO = { userId: '11111111-1111-4111-8111-111111111111', applicationContextId: '22222222-2222-4222-8222-222222222222' };
const DESIGN = '33333333-3333-4333-8333-333333333333';

function never(): AbortSignal {
  return new AbortController().signal;
}

async function rejection(promise: Promise<unknown>): Promise<CapabilityError> {
  try {
    await promise;
  } catch (error) {
    assert.ok(error instanceof CapabilityError, `a CapabilityError, not ${String(error)}`);
    return error;
  }
  assert.fail('expected a refusal');
}

describe('Boundary F client', () => {
  const lines: { event: string; fields: LogFields }[] = [];
  const log = (event: string, fields: LogFields = {}) => lines.push({ event, fields });

  after(() => {
    const logged = JSON.stringify(lines);
    assert.ok(!logged.includes(CREDENTIAL), 'the service credential never reaches a log line');
    assert.ok(!logged.includes('session-token-sentinel'), 'a session token never reaches a log line');
  });

  it('sends the service credential, the represented user and the context on every operation', async () => {
    const forma = await fakeForma(({ path }) => {
      if (path.endsWith('/validate')) return { status: 200, body: { valid: true, issues: [] } };
      if (path.endsWith('/operations')) {
        return { status: 200, body: { design_id: DESIGN, project_id: DESIGN, version: 2, updated_at: 'now' } };
      }
      if (path === '/api/v1/pontis/projects') return { status: 200, body: [] };
      return { status: 200, body: { id: DESIGN, project_id: DESIGN, version: 1, document: {} } };
    });
    try {
      const boundary = new FormaBoundary(forma.url, CREDENTIAL, log);
      await boundary.listProjects(WHO, 1024, never());
      await boundary.getDesign(WHO, DESIGN, 1024, never());
      await boundary.validate(WHO, DESIGN, 60_000, 1024, never());
      await boundary.applyOperations(WHO, DESIGN, { base_version: 1, operations: [], approval: null }, 'inv_1', 1024, never());

      assert.deepEqual(
        forma.requests.map(({ method, path }) => `${method} ${path}`),
        [
          'GET /api/v1/pontis/projects',
          `GET /api/v1/pontis/designs/${DESIGN}`,
          `POST /api/v1/pontis/designs/${DESIGN}/validate`,
          `POST /api/v1/pontis/designs/${DESIGN}/operations`,
        ],
      );
      for (const request of forma.requests) {
        assert.equal(request.headers.authorization, `Bearer ${CREDENTIAL}`);
        assert.equal(request.headers[REPRESENTED_USER_HEADER], WHO.userId);
        assert.equal(request.headers[APPLICATION_CONTEXT_HEADER], WHO.applicationContextId);
        // Never marked as the MCP door: the service arm refuses that outright.
        assert.equal(request.headers['x-forma-entry'], undefined);
      }
      assert.equal(forma.requests[2]?.headers[TIMEOUT_HEADER], '60000');
      assert.equal(forma.requests[3]?.headers[INVOCATION_HEADER], 'inv_1');
      assert.deepEqual(JSON.parse(forma.requests[3]?.body ?? ''), { base_version: 1, operations: [], approval: null });
    } finally {
      await forma.close();
    }
  });

  it('asks Forma who a session token belongs to, in its own header, acting for nobody', async () => {
    const forma = await fakeForma(({ headers }) =>
      headers[SUBJECT_TOKEN_HEADER] === 'session-token-sentinel'
        ? { status: 200, body: { subject: WHO.userId, account_id: WHO.applicationContextId, role: 'owner' } }
        : { status: 401, body: envelope('UNAUTHORIZED', undefined) },
    );
    try {
      const boundary = new FormaBoundary(forma.url, CREDENTIAL, log);
      assert.deepEqual(await boundary.resolveSubject('session-token-sentinel'), {
        subject: WHO.userId,
        account_id: WHO.applicationContextId,
        role: 'owner',
      });
      assert.equal(await boundary.resolveSubject('not-a-session'), undefined, 'a token Forma refuses resolves to nobody');
      const [first] = forma.requests;
      assert.equal(first?.path, '/api/v1/service/resolve');
      assert.equal(first?.headers.authorization, `Bearer ${CREDENTIAL}`);
      assert.equal(first?.headers[REPRESENTED_USER_HEADER], undefined);
      assert.equal(first?.headers[APPLICATION_CONTEXT_HEADER], undefined);
      assert.equal(first?.headers['x-forma-entry'], undefined);
    } finally {
      await forma.close();
    }
  });

  it('never puts a model-chosen id anywhere but its own path segment', async () => {
    const forma = await fakeForma(() => ({ status: 404, body: envelope('DESIGN_NOT_FOUND', 'not_found') }));
    try {
      const boundary = new FormaBoundary(forma.url, CREDENTIAL, log);
      await rejection(boundary.getDesign(WHO, '../projects?x=1', 1024, never()));
      assert.equal(forma.requests[0]?.path, '/api/v1/pontis/designs/..%2Fprojects%3Fx%3D1');
    } finally {
      await forma.close();
    }
  });
});

describe('Forma refusals as registry errors', () => {
  // Plan §7.1 names these four; each pairs the Forma code with the registry
  // class `routes/pontis.rs::classify` gives it.
  for (const [code, status, capabilityError, expected] of [
    ['VERSION_CONFLICT', 409, 'conflict', 'conflict'],
    ['DESIGN_NOT_FOUND', 404, 'not_found', 'not_found'],
    ['INVALID_OP', 400, 'validation_error', 'validation_error'],
    ['ASSET_NOT_FOUND', 404, 'not_found', 'not_found'],
  ] as const) {
    it(`${code} becomes ${expected}, keeping the Forma code and its hint`, () => {
      const error = refusal(status, envelope(code, capabilityError));
      assert.equal(error.code, expected);
      assert.deepEqual(error.details, { forma_code: code, hint: `what to do about ${code}` });
      assert.equal(error.message, `${code} happened`);
    });
  }

  it('reads a reclassified approved batch as resource_state_changed, not conflict', () => {
    assert.equal(refusal(409, envelope('VERSION_CONFLICT', 'resource_state_changed')).code, 'resource_state_changed');
  });

  it('reads a refusal from the authentication arm, which has no registry class, by its status', () => {
    assert.equal(refusal(403, envelope('CONTEXT_FORBIDDEN', undefined)).code, 'authorization_denied');
    assert.equal(refusal(401, envelope('UNAUTHORIZED', undefined)).code, 'authorization_denied');
  });

  it('lets no detail of a Forma fault cross, as Forma itself does not', () => {
    const error = refusal(500, envelope('INTERNAL', 'internal_error', 'table designs is locked'));
    assert.equal(error.code, 'internal_error');
    assert.ok(!error.message.includes('locked'));
    assert.equal(error.details, undefined);
    assert.equal(refusal(503, envelope('STORAGE', 'unavailable')).code, 'unavailable');
  });

  it('treats a class it does not know as unknown, never as success', () => {
    assert.equal(refusal(400, envelope('NEW_THING', 'a_future_class')).code, 'validation_error');
    assert.equal(refusal(502, 'not json at all').code, 'internal_error');
  });
});

describe('what Boundary F answers', () => {
  async function answered(answer: Answer, maxBytes: number): Promise<CapabilityError> {
    const forma = await fakeForma(() => answer);
    try {
      return await rejection(new FormaBoundary(forma.url, CREDENTIAL, () => {}).getDesign(WHO, DESIGN, maxBytes, never()));
    } finally {
      await forma.close();
    }
  }

  it('refuses an answer over the capability bound rather than cutting it, declared or streamed', async () => {
    const big = { status: 200, body: { id: DESIGN, padding: 'x'.repeat(4096) } };
    assert.equal((await answered(big, 1024)).code, 'result_too_large');
    assert.equal((await answered({ ...big, chunked: true }, 1024)).code, 'result_too_large');
  });

  it('answers unavailable when Forma is down, without its address or the credential', async () => {
    const forma = await fakeForma(() => ({ status: 200, body: {} }));
    const url = forma.url;
    await forma.close();
    const error = await rejection(new FormaBoundary(url, CREDENTIAL, () => {}).listProjects(WHO, 1024, never()));
    assert.equal(error.code, 'unavailable');
    assert.ok(!JSON.stringify([error.message, error.details]).includes(CREDENTIAL));
  });

  it('drops the request when the caller abandons it', async () => {
    let reached = (): void => {};
    const arrived = new Promise<void>((resolve) => {
      reached = resolve;
    });
    const forma = await fakeForma(
      () =>
        new Promise<Answer>((resolve) => {
          reached();
          setTimeout(() => resolve({ status: 200, body: { valid: true, issues: [] } }), 5_000).unref();
        }),
    );
    try {
      const abandon = new AbortController();
      const pending = new FormaBoundary(forma.url, CREDENTIAL, () => {}).validate(WHO, DESIGN, 60_000, 1024, abandon.signal);
      await arrived;
      abandon.abort();
      assert.equal((await rejection(pending)).code, 'timeout');
    } finally {
      await forma.close();
    }
  });
});
