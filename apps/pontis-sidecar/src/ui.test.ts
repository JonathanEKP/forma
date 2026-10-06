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
import { generateKeyPairSync, randomUUID } from 'node:crypto';
import { describe, it } from 'node:test';

import { SUBJECT_TOKEN_HEADER } from './boundary.ts';
import { envelope, fakeForma } from './fake-forma.ts';
import { silentLog } from './log.ts';
import { APPLICATION_ID, startSidecar, type Sidecar } from './sidecar.ts';
import { UI_ACTIONS } from './ui.ts';
import { connect as connectWindow, drain, send, WINDOW_HEADER, type Window, type WindowOptions } from './ui-window.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const OTHER_USER = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const CONTEXT = '22222222-2222-4222-8222-222222222222';
const OTHER_CONTEXT = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const DESIGN = '33333333-3333-4333-8333-333333333333';
const UNREADABLE = '99999999-9999-4999-8999-999999999999';
const LAYER = '44444444-4444-4444-8444-444444444444';
const SESSION = 'ses_00000000000000000000000000000000';
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

const defaultSessionBound = new WeakSet<Sidecar>();

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
function connect(sidecar: Sidecar, token: string, options: WindowOptions = {}, origin = ORIGIN): Promise<Window> {
  const withDefaultSession = options.agentSession === undefined && token === 'session-user';
  const prepare = async (): Promise<Window> => {
    if (withDefaultSession && !defaultSessionBound.has(sidecar)) {
      const deviceId = await pair(sidecar, USER);
      sidecar.controlPlane.bindSessionContext({ sessionId: SESSION, userId: USER, deviceId, applicationContextId: CONTEXT });
      defaultSessionBound.add(sidecar);
    }
    return await connectWindow(sidecar.origin, token, origin, UI_ACTIONS, withDefaultSession ? { ...options, agentSession: SESSION } : options);
  };
  return prepare();
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

  it('returns a rejected view action to the agent as rejected, not handled', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const window = await connect(sidecar, 'session-user');
      try {
        const invocation = invoke(sidecar, 'ui.setZoom', { zoom: 2 }, 'inv_no_design');
        const action = await window.next();
        assert.equal(action.capabilityId, 'ui.setZoom');

        assert.equal(await window.ack(action.invocationId, 'rejected', 'open a design first'), 200);
        const outcome = await invocation;
        assert.equal(outcome.status, 'failed');
        assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'ui_action_rejected');
        assert.equal(outcome.status === 'failed' ? outcome.error.message : '', 'open a design first');
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

describe('a design id that is not an id', () => {
  it('is a validation error, not a permission answer: a typo names no resource, so there is nothing to disclose (F-40-5)', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const window = await connect(sidecar, 'session-user');
      try {
        for (const [id, input] of [
          ['ui.openDesign', { design_id: 'ALPHA Square post' }],
          ['ui.openDesign', { design_id: '' }],
          ['ui.selectLayer', { design_id: 'not-a-uuid', layer_id: LAYER }],
        ] as const) {
          const outcome = await invoke(sidecar, id, input, `inv_${id}_${String(input.design_id).length}`);
          assert.equal(outcome.status, 'failed');
          assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'validation_error', JSON.stringify(input));
        }
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

/** Waits for the server to have noticed windows going: a closed client is dropped asynchronously. */
async function registered(sidecar: Sidecar, count: number): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (sidecar.agentApp.uiClients.all().length === count) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  assert.equal(sidecar.agentApp.uiClients.all().length, count);
}

/** A paired Device record, with no socket: enough to hold a session's binding. */
async function pair(sidecar: Sidecar, userId: string): Promise<string> {
  const jwk = generateKeyPairSync('ed25519').publicKey.export({ format: 'jwk' });
  assert.ok(typeof jwk.x === 'string');
  const { code } = await sidecar.controlPlane.createPairingCode(userId);
  const device = await sidecar.controlPlane.claimPairing({
    code,
    applicationId: APPLICATION_ID,
    publicKey: jwk.x,
    metadata: { name: 'Workstation', platform: 'linux', runtimeVersion: '0.1.0' },
  });
  return device.id;
}

/** `ses_` and 32 hex digits: the shape the intent route mints. */
const session = (): string => `ses_${randomUUID().replaceAll('-', '')}`;

describe('one registration per window (F-40-1, F-50-3)', () => {
  it('keeps two windows of one person connected side by side instead of evicting each other', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const first = await connect(sidecar, 'session-user');
      const second = await connect(sidecar, 'session-user');
      try {
        assert.equal(sidecar.agentApp.uiClients.all().length, 2, 'both are registered');
        const outcome = await Promise.race([first.ended.then(() => 'first ended'), new Promise((resolve) => setTimeout(() => resolve('both open'), 150))]);
        assert.equal(outcome, 'both open', 'the newer window did not close the older one');
        assert.notEqual(first.clientId, second.clientId);
      } finally {
        first.close();
        second.close();
      }
    });
  });

  it('makes a reload — the same window id again — replace its own registration', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const before = await connect(sidecar, 'session-user', { windowId: randomUUID() });
      const after = await connect(sidecar, 'session-user', { windowId: before.windowId });
      try {
        await before.ended;
        assert.deepEqual(
          sidecar.agentApp.uiClients.all().map((client) => client.id),
          [after.clientId],
        );
      } finally {
        after.close();
      }
    });
  });

  it('scopes the app session to the window, so two tabs are two clients', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const tab = await connect(sidecar, 'session-user');
      try {
        const [client] = sidecar.agentApp.uiClients.all();
        assert.equal(client?.appSessionId, `${USER}:${CONTEXT}:${tab.windowId}`);
      } finally {
        tab.close();
      }
    });
  });

  it('refuses a stream with no window id, or one that is not the fixed format', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      for (const header of [undefined, '', 'tab 1', 'x'.repeat(200), `${randomUUID()}\r\nx-evil: 1`, '../../etc']) {
        const response = await send(`${sidecar.origin}/pons/v1/ui/stream`, {
          headers: { authorization: 'Bearer session-user', origin: ORIGIN, host: HOST, ...(header === undefined ? {} : { [WINDOW_HEADER]: header }) },
        }).catch(() => undefined);
        // A header node refuses to write never reaches the server at all.
        if (!response) continue;
        assert.equal(response.statusCode, 403, `window id ${JSON.stringify(header)}`);
        await drain(response);
      }
      assert.equal(sidecar.agentApp.uiClients.all().length, 0);
    });
  });

  it('does not let another person act through a window id it learned', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const mine = await connect(sidecar, 'session-user');
      try {
        const answer = await send(`${sidecar.origin}/pons/v1/ui/ack`, {
          method: 'POST',
          headers: { authorization: 'Bearer session-other', origin: ORIGIN, host: HOST, 'content-type': 'application/json', [WINDOW_HEADER]: mine.windowId },
          body: JSON.stringify({ clientId: mine.clientId, invocationId: 'inv_x', outcome: 'handled' }),
        });
        assert.equal(answer.statusCode, 403);
        await drain(answer);
      } finally {
        mine.close();
      }
    });
  });
});

describe('an action goes to the window whose chat started the session (D-2)', () => {
  it('delivers to the bound window only, and the other window never handles it', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const device = await pair(sidecar, USER);
      const bound = session();
      sidecar.controlPlane.bindSessionContext({ sessionId: bound, userId: USER, deviceId: device, applicationContextId: CONTEXT });
      const chat = await connect(sidecar, 'session-user', { agentSession: bound });
      const bystander = await connect(sidecar, 'session-user', { agentSession: '' });
      try {
        const invocation = sidecar.agentApp.gateway.invoke({
          userId: USER,
          sessionId: bound,
          applicationContextId: CONTEXT,
          invocationId: 'inv_bound',
          capabilityId: 'ui.setZoom',
          input: { zoom: 2 },
        });
        const action = await chat.next();
        assert.equal(action.capabilityId, 'ui.setZoom');
        await chat.ack(action.invocationId, 'handled');
        const outcome = await invocation;
        assert.deepEqual(outcome.status === 'completed' ? outcome.result : null, { outcome: 'handled', clientId: chat.clientId });
        await assert.rejects(bystander.next(200), 'the other window was never asked');
      } finally {
        chat.close();
        bystander.close();
      }
    });
  });

  it('answers ui_unavailable with two windows and none bound, rather than guessing', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const one = await connect(sidecar, 'session-user', { agentSession: '' });
      const two = await connect(sidecar, 'session-user', { agentSession: '' });
      try {
        const outcome = await invoke(sidecar, 'ui.setZoom', { zoom: 2 }, 'inv_ambiguous');
        assert.equal(outcome.status, 'failed');
        assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'ui_unavailable');
        await assert.rejects(one.next(200));
        await assert.rejects(two.next(200));
      } finally {
        one.close();
        two.close();
      }
    });
  });

  it('refuses an action when the bound window is gone, even with one other window open', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const device = await pair(sidecar, USER);
      const bound = session();
      sidecar.controlPlane.bindSessionContext({ sessionId: bound, userId: USER, deviceId: device, applicationContextId: CONTEXT });
      const chat = await connect(sidecar, 'session-user', { agentSession: bound });
      const other = await connect(sidecar, 'session-user', { agentSession: '' });
      try {
        chat.close();
        await registered(sidecar, 1);
        const outcome = await sidecar.agentApp.gateway.invoke({
          userId: USER,
          sessionId: bound,
          applicationContextId: CONTEXT,
          invocationId: 'inv_gone_one',
          capabilityId: 'ui.setZoom',
          input: { zoom: 3 },
        });
        assert.equal(outcome.status, 'failed');
        assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'ui_unavailable');
        await assert.rejects(other.next(200), 'the remaining window was never asked');
      } finally {
        other.close();
      }
    });
  });

  it('binds nothing for a session id of another person, and the forger is not an eligible target', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const device = await pair(sidecar, USER);
      const mine = session();
      sidecar.controlPlane.bindSessionContext({ sessionId: mine, userId: USER, deviceId: device, applicationContextId: CONTEXT });
      // The forger has a Device of their own, so the check really does ask about it.
      await pair(sidecar, OTHER_USER);
      const one = await connect(sidecar, 'session-user', { agentSession: '' });
      const two = await connect(sidecar, 'session-user', { agentSession: '' });
      const forger = await connect(sidecar, 'session-other', { agentSession: mine });
      try {
        const bound = sidecar.agentApp.uiClients.all().map((client) => client.agentSessionId);
        assert.deepEqual(bound, [undefined, undefined, undefined], 'nobody is bound');
        const outcome = await invoke(sidecar, 'ui.setZoom', { zoom: 2 }, 'inv_forged');
        assert.equal(outcome.status === 'failed' ? outcome.error.code : '', 'ui_unavailable');
        await assert.rejects(forger.next(200), 'the forger heard nothing');
      } finally {
        one.close();
        two.close();
        forger.close();
      }
    });
  });

  it('binds nothing for a session id the Control Plane has never seen, or one bound in another account', async () => {
    await sidecarWith([ORIGIN], async (sidecar) => {
      const device = await pair(sidecar, USER);
      const elsewhere = session();
      sidecar.controlPlane.bindSessionContext({ sessionId: elsewhere, userId: USER, deviceId: device, applicationContextId: OTHER_CONTEXT });
      const unknown = await connect(sidecar, 'session-user', { agentSession: session() });
      const wrongAccount = await connect(sidecar, 'session-user', { agentSession: elsewhere });
      const malformed = await connect(sidecar, 'session-user', { agentSession: 'not a session' });
      try {
        assert.deepEqual(
          sidecar.agentApp.uiClients.all().map((client) => client.agentSessionId),
          [undefined, undefined, undefined],
        );
      } finally {
        unknown.close();
        wrongAccount.close();
        malformed.close();
      }
    });
  });
});
