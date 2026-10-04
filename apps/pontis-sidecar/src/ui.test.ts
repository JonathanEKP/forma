/**
 * The editor actions over the real routes (plan §11 step 9, scenario S7).
 *
 * What this file is for is the hand-written half. The editor implements the
 * three UI routes itself rather than taking `@pontis/react` (plan §10(b) D8), so
 * nothing would otherwise notice if Pontis renamed a frame or changed an ack
 * field — the channel would simply stop working in a browser, where no test
 * looks. So the client here speaks the same wire the editor's does, against the
 * sidecar's actual transport, and the editor's own tests reuse its framing.
 */
import { strict as assert } from 'node:assert';
import { describe, it } from 'node:test';

import { SUBJECT_TOKEN_HEADER } from './boundary.ts';
import { envelope, fakeForma } from './fake-forma.ts';
import { silentLog } from './log.ts';
import { startSidecar, type Sidecar } from './sidecar.ts';
import { UI_ACTIONS } from './ui.ts';
import { connect as connectWindow, drain, send, type Window } from './ui-window.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const OTHER_USER = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const CONTEXT = '22222222-2222-4222-8222-222222222222';
const OTHER_CONTEXT = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const DESIGN = '33333333-3333-4333-8333-333333333333';
const UNREADABLE = '99999999-9999-4999-8999-999999999999';
const LAYER = '44444444-4444-4444-8444-444444444444';
const SESSION = 'ses_ui';
const ORIGIN = 'http://editor.test';
/**
 * What the proxy in front of both sends.
 *
 * Caddy and Vite both pass the browser's `Host` through unchanged, which is why
 * the transport can derive its allowlist from the declared origins at all: the
 * editor's name arrives here, not the sidecar's loopback address. These tests
 * set it for the same reason — they are standing in for that proxy.
 */
const HOST = new URL(ORIGIN).host;

/** A session token the fake resolves, one per person. */
const TOKENS: Record<string, { subject: string; account_id: string }> = {
  'session-user': { subject: USER, account_id: CONTEXT },
  'session-other': { subject: OTHER_USER, account_id: OTHER_CONTEXT },
};

/**
 * forma-server as these tests need it: it resolves the two sessions, serves one
 * readable design, and refuses the other.
 */
async function forma() {
  return await fakeForma((request) => {
    if (request.path === '/api/v1/service/resolve') {
      // The session token travels in its own header, raw: `Authorization`
      // carries the sidecar's own credential on the same request.
      const presented = request.headers[SUBJECT_TOKEN_HEADER];
      const resolved = typeof presented === 'string' ? TOKENS[presented] : undefined;
      return resolved
        ? { status: 200, body: { ...resolved, role: 'owner' } }
        : { status: 401, body: envelope('UNAUTHORIZED', undefined) };
    }
    if (request.path === `/api/v1/pontis/designs/${DESIGN}`) {
      return { status: 200, body: { id: DESIGN, name: 'Cover', version: 1, document: { pages: [] } } };
    }
    if (request.path === `/api/v1/pontis/designs/${UNREADABLE}`) {
      return { status: 404, body: envelope('DESIGN_NOT_FOUND', 'not_found') };
    }
    return { status: 404, body: envelope('NO_SUCH_OPERATION', 'not_found') };
  });
}

/** One connected window, registered for all four actions unless a test says otherwise. */
function connect(sidecar: Sidecar, token: string, origin = ORIGIN): Promise<Window> {
  return connectWindow(sidecar.origin, token, origin, UI_ACTIONS);
}

async function sidecarWith(editorOrigins: readonly string[] | undefined, body: (sidecar: Sidecar) => Promise<void>) {
  const server = await forma();
  const sidecar = await startSidecar({
    formaUrl: server.url,
    serviceCredential: 'forma_svc_test',
    host: '127.0.0.1',
    port: 0,
    ...(editorOrigins ? { editorOrigins } : {}),
    log: silentLog,
  });
  try {
    await body(sidecar);
  } finally {
    await sidecar.close();
    await server.close();
  }
}

/** One invocation through the gateway, as the Runtime would make it. */
function invoke(sidecar: Sidecar, capabilityId: string, input: unknown, invocationId: string, userId = USER, contextId = CONTEXT) {
  return sidecar.agentApp.gateway.invoke({
    userId,
    sessionId: SESSION,
    applicationContextId: contextId,
    invocationId,
    capabilityId,
    input,
  });
}

describe('an editor action, end to end', () => {
  it('reaches the window, and the window\'s acknowledgement is what makes it succeed', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const window = await connect(sidecar, 'session-user');
      try {
        const invocation = invoke(sidecar, 'ui.openDesign', { design_id: DESIGN }, 'inv_open');
        const action = await window.next();
        assert.equal(action.capabilityId, 'ui.openDesign');
        assert.deepEqual(action.input, { design_id: DESIGN });

        assert.equal(await window.ack(action.invocationId, 'handled', 'Cover, page 1'), 200);
        const outcome = await invocation;
        assert.equal(outcome.status, 'completed');
        assert.deepEqual(outcome.status === 'completed' ? outcome.result : null, {
          outcome: 'handled',
          clientId: window.clientId,
          detail: 'Cover, page 1',
        });
      } finally {
        window.close();
      }
    });
  });

  it('answers ui_unavailable with no window open, rather than a success nobody saw', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const outcome = await invoke(sidecar, 'ui.setZoom', { zoom: 2 }, 'inv_nowindow');
      assert.equal(outcome.status, 'failed');
      assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'ui_unavailable');
    });
  });

  it('refuses an action the window has not registered', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const window = await connect(sidecar, 'session-user');
      try {
        assert.equal(await window.register(['ui.setZoom']), 200);
        const outcome = await invoke(sidecar, 'ui.setPanel', { panel: 'validation' }, 'inv_unregistered');
        assert.equal(outcome.status, 'failed');
        assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'ui_action_not_registered');
      } finally {
        window.close();
      }
    });
  });

  it('times out when the window never answers, and drops the late acknowledgement', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const window = await connect(sidecar, 'session-user');
      try {
        // The acknowledgement window is the capability's own timeout, which the
        // catalog sets at 5s for an editor action. Overridden here so the test
        // does not wait it out; what is under test is the behaviour, not the
        // number, and the number is asserted where it is declared.
        const capability = sidecar.agentApp.registry.get('ui.setZoom');
        assert.ok(capability);
        const invocation = sidecar.agentApp.uiRouter.route({
          invocationId: 'inv_timeout',
          capabilityId: 'ui.setZoom',
          userId: USER,
          applicationContextId: CONTEXT,
          agentSessionId: SESSION,
          input: { zoom: 2 },
          timeoutMs: 50,
        });
        const action = await window.next();
        await assert.rejects(invocation, (error: { code?: string }) => error.code === 'ui_action_timeout');

        // The agent has already been told it failed and the person may have
        // moved on, so a late ack changes nothing and is not an error either.
        assert.equal(await window.ack(action.invocationId, 'handled'), 200);
        assert.equal(sidecar.agentApp.uiRouter.isPending('inv_timeout'), false);
      } finally {
        window.close();
      }
    });
  });

  it('does not route a person\'s action to another account\'s window', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const other = await connect(sidecar, 'session-other');
      try {
        // The only window connected belongs to someone else, in another
        // context. ADR-024 Decision 2 makes it ineligible on both counts, and
        // the answer is the same as having no window at all.
        const outcome = await invoke(sidecar, 'ui.setZoom', { zoom: 2 }, 'inv_crossaccount');
        assert.equal(outcome.status, 'failed');
        assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'ui_unavailable');
        await assert.rejects(other.next(200));
      } finally {
        other.close();
      }
    });
  });

  it('authorizes the design before delivering anything, and does not say which refusal it was', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const window = await connect(sidecar, 'session-user');
      try {
        const outcome = await invoke(sidecar, 'ui.selectLayer', { design_id: UNREADABLE, layer_id: LAYER }, 'inv_denied');
        assert.equal(outcome.status, 'failed');
        // Not `not_found`: telling the two apart is the disclosure §3.2 names.
        assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'authorization_denied');
        await assert.rejects(window.next(200), 'nothing reached the window');
      } finally {
        window.close();
      }
    });
  });
});

describe('the UI routes themselves', () => {
  it('refuse every origin when none is declared, rather than accepting any', async () => {
    await sidecarWith(undefined, async (sidecar) => {
      const response = await send(`${sidecar.origin}/pons/v1/ui/stream`, {
        headers: { authorization: 'Bearer session-user', origin: ORIGIN, host: HOST },
      });
      assert.equal(response.statusCode, 403);
      await drain(response);
    });
  });

  it('refuse an origin that is not the editor\'s', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const response = await send(`${sidecar.origin}/pons/v1/ui/stream`, {
        headers: { authorization: 'Bearer session-user', origin: 'http://elsewhere.test', host: HOST },
      });
      assert.equal(response.statusCode, 403);
      await drain(response);
    });
  });

  it('refuse a request with no Forma session', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const response = await send(`${sidecar.origin}/pons/v1/ui/stream`, { headers: { origin: ORIGIN, host: HOST } });
      assert.equal(response.statusCode, 403);
      await drain(response);
    });
  });

  it('declare the acknowledgement window the catalog sets, so the editor is not racing a number nobody wrote down', async () => {
    await sidecarWith([ORIGIN], (sidecar) => {
      for (const id of UI_ACTIONS) {
        const capability = sidecar.agentApp.registry.get(id);
        assert.ok(capability, `${id} is registered`);
        assert.equal(capability.bounds.timeoutMs, 5_000, `${id} waits 5s for the window`);
        assert.equal(capability.risk, 'read', `${id} changes a view, never data`);
      }
      return Promise.resolve();
    });
  });
});
