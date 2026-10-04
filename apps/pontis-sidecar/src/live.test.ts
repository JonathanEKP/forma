/**
 * Live delivery and presence (plan §11 step 10c), through the real pieces: a
 * Device on the real socket, the Control Plane's real event store, and windows on
 * the real UI stream. Only forma-server is a stand-in.
 *
 * What is under test is who hears what. A person's agent speaking into another
 * person's tab is the failure this exists to prevent, so most of the cases are
 * about who must not.
 */
import { strict as assert } from 'node:assert';
import { generateKeyPairSync, randomUUID, sign } from 'node:crypto';
import { request as httpRequest, type IncomingMessage } from 'node:http';
import { describe, it } from 'node:test';

import { canonicalProofMessage } from '@pontis/control-plane';

import { SUBJECT_TOKEN_HEADER } from './boundary.ts';
import { envelope, fakeForma } from './fake-forma.ts';
import { fanOutDurable, liveNotification } from './live.ts';
import { silentLog, type LogFields } from './log.ts';
import { APPLICATION_ID, startSidecar, type Sidecar } from './sidecar.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const OTHER_USER = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const CONTEXT = '22222222-2222-4222-8222-222222222222';
const OTHER_CONTEXT = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const ORIGIN = 'http://editor.test';
const HOST = new URL(ORIGIN).host;
const SESSION = 'ses_live';

const TOKENS: Record<string, { subject: string; account_id: string }> = {
  'session-user': { subject: USER, account_id: CONTEXT },
  'session-other': { subject: OTHER_USER, account_id: OTHER_CONTEXT },
};

async function withSidecar(body: (sidecar: Sidecar) => Promise<void>) {
  const server = await fakeForma((request) => {
    if (request.path !== '/api/v1/service/resolve') return { status: 404, body: envelope('NO_SUCH_OPERATION', 'not_found') };
    const presented = request.headers[SUBJECT_TOKEN_HEADER];
    const resolved = typeof presented === 'string' ? TOKENS[presented] : undefined;
    return resolved ? { status: 200, body: { ...resolved, role: 'owner' } } : { status: 401, body: envelope('UNAUTHORIZED', undefined) };
  });
  const sidecar = await startSidecar({
    formaUrl: server.url,
    serviceCredential: 'forma_svc_test',
    host: '127.0.0.1',
    port: 0,
    editorOrigins: [ORIGIN],
    log: silentLog,
  });
  try {
    await body(sidecar);
  } finally {
    await sidecar.close();
    await server.close();
  }
}

/** One browser window on the UI stream, collecting the `pons.event` frames it is sent. */
interface Window {
  readonly events: Record<string, unknown>[];
  /** Resolves once an event matching `match` has arrived. */
  next(match: (event: Record<string, unknown>) => boolean): Promise<Record<string, unknown>>;
  close(): void;
}

async function openWindow(sidecar: Sidecar, token: string): Promise<Window> {
  const abort = new AbortController();
  const response = await new Promise<IncomingMessage>((resolve, reject) => {
    httpRequest(
      `${sidecar.origin}/pons/v1/ui/stream`,
      { headers: { authorization: `Bearer ${token}`, origin: ORIGIN, host: HOST, accept: 'text/event-stream', 'x-forma-window': randomUUID() }, signal: abort.signal },
      resolve,
    )
      .on('error', reject)
      .end();
  });
  assert.equal(response.statusCode, 200, 'the stream opened');

  const events: Record<string, unknown>[] = [];
  const waiting: { match: (event: Record<string, unknown>) => boolean; resolve: (event: Record<string, unknown>) => void }[] = [];
  let connected: (() => void) | undefined;
  const ready = new Promise<void>((resolve) => {
    connected = resolve;
  });

  let buffer = '';
  response.setEncoding('utf8');
  response.on('data', (chunk: string) => {
    buffer += chunk;
    for (let boundary = buffer.indexOf('\n\n'); boundary !== -1; boundary = buffer.indexOf('\n\n')) {
      const block = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      const name = /^event: (.+)$/mu.exec(block)?.[1];
      const data = /^data: (.+)$/mu.exec(block)?.[1];
      if (name === 'ui.connected') connected?.();
      if (name !== 'pons.event' || data === undefined) continue;
      const parsed: unknown = JSON.parse(data);
      if (typeof parsed !== 'object' || parsed === null) continue;
      const event = parsed as Record<string, unknown>;
      events.push(event);
      for (const [index, waiter] of waiting.entries()) {
        if (waiter.match(event)) {
          waiting.splice(index, 1);
          waiter.resolve(event);
          break;
        }
      }
    }
  });
  response.on('error', () => undefined);
  await ready;

  return {
    events,
    next: (match) =>
      new Promise((resolve, reject) => {
        const seen = events.find(match);
        if (seen) return resolve(seen);
        const timer = setTimeout(() => reject(new Error('no matching event arrived')), 2_000);
        waiting.push({
          match,
          resolve: (event) => {
            clearTimeout(timer);
            resolve(event);
          },
        });
      }),
    close: () => abort.abort(),
  };
}

/** A paired Device with its socket open, speaking the Runtime's side of the wire. */
interface Device {
  readonly id: string;
  /** Publishes one notification as the Runtime would. */
  publish(type: string, payload: Record<string, unknown>, sessionId?: string): void;
  close(): Promise<void>;
}

/** Node's WebSocket takes request headers as a second argument; its types do not say so. */
type HeaderedWebSocket = new (url: string, options: { headers: Record<string, string> }) => WebSocket;

async function pairDevice(sidecar: Sidecar, userId: string): Promise<Device> {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const jwk = publicKey.export({ format: 'jwk' });
  assert.ok(typeof jwk.x === 'string');
  const { code } = await sidecar.controlPlane.createPairingCode(userId);
  const device = await sidecar.controlPlane.claimPairing({
    code,
    applicationId: APPLICATION_ID,
    publicKey: jwk.x,
    metadata: { name: 'Workstation', platform: 'linux', runtimeVersion: '0.1.0' },
  });
  const challenge = await sidecar.controlPlane.issueChallenge(device.id);
  const signature = sign(
    null,
    Buffer.from(
      canonicalProofMessage({
        context: challenge.context,
        deviceId: device.id,
        challengeId: challenge.challengeId,
        nonce: challenge.nonce,
        issuedAt: challenge.issuedAt,
        expiresAt: challenge.expiresAt,
        audience: challenge.audience,
      }),
      'utf8',
    ),
    privateKey,
  ).toString('base64url');
  const issued = await sidecar.controlPlane.authenticate({ deviceId: device.id, challengeId: challenge.challengeId, signature });

  const Socket = WebSocket as unknown as HeaderedWebSocket;
  const socket = new Socket(`${sidecar.origin.replace('http', 'ws')}/pons/v1/device/socket`, {
    headers: {
      'X-Pons-Protocol-Version': '1.0',
      'X-Pons-Device-Id': device.id,
      'X-Pons-Client': 'pontis-runtime/test',
      Authorization: `Bearer ${issued.credential}`,
    },
  });
  await new Promise<void>((resolve, reject) => {
    socket.addEventListener('open', () => resolve());
    socket.addEventListener('error', () => reject(new Error('the device socket was refused')));
  });

  let origin = 0;
  return {
    id: device.id,
    publish: (type, payload, sessionId) => {
      origin += 1;
      socket.send(
        JSON.stringify({
          protocol: 'pons/1.0',
          messageId: `msg_${type}_${origin}`,
          type,
          ...(sessionId ? { sessionId } : {}),
          originSequence: origin,
          sentAt: new Date().toISOString(),
          payload,
        }),
      );
    },
    close: () =>
      new Promise<void>((resolve) => {
        socket.addEventListener('close', () => resolve());
        socket.close();
      }),
  };
}

async function get(sidecar: Sidecar, path: string, token?: string): Promise<{ status: number; body: unknown }> {
  const response = await fetch(`${sidecar.origin}${path}`, { headers: token ? { authorization: `Bearer ${token}` } : {} });
  return { status: response.status, body: await response.json() };
}

const isType = (type: string) => (event: Record<string, unknown>) => event.type === type;

describe('a Device notification reaches its owner\'s windows', () => {
  it('delivers an output delta, unsequenced, to the session\'s own context and no other account\'s window', async () => {
    await withSidecar(async (sidecar) => {
      const device = await pairDevice(sidecar, USER);
      sidecar.controlPlane.bindSessionContext({ sessionId: SESSION, userId: USER, deviceId: device.id, applicationContextId: CONTEXT });
      const mine = await openWindow(sidecar, 'session-user');
      const theirs = await openWindow(sidecar, 'session-other');
      try {
        device.publish('agent.output.delta', { turnId: 'turn_1', text: 'Rais' }, SESSION);

        const delta = await mine.next(isType('agent.output.delta'));
        assert.deepEqual(delta.payload, { turnId: 'turn_1', text: 'Rais' });
        assert.equal(delta.sessionId, SESSION);
        assert.ok(!('sequence' in delta), 'a delta is never stored, so it has no position');

        // Another frame, after the first, to prove the other window had every chance to hear.
        device.publish('agent.output.delta', { turnId: 'turn_1', text: 'e' }, SESSION);
        await mine.next((event) => event.type === 'agent.output.delta' && isRecord(event.payload) && event.payload.text === 'e');
        assert.deepEqual(theirs.events, [], 'another account\'s window heard nothing');
      } finally {
        mine.close();
        theirs.close();
        await device.close();
      }
    });
  });

  it('delivers a stored event with its sequence, and the same event is on the page at that position', async () => {
    await withSidecar(async (sidecar) => {
      const device = await pairDevice(sidecar, USER);
      sidecar.controlPlane.bindSessionContext({ sessionId: SESSION, userId: USER, deviceId: device.id, applicationContextId: CONTEXT });
      const mine = await openWindow(sidecar, 'session-user');
      const theirs = await openWindow(sidecar, 'session-other');
      try {
        device.publish('agent.output.completed', { turnId: 'turn_1', text: 'Done.' }, SESSION);

        const frame = await mine.next(isType('agent.output.completed'));
        assert.equal(frame.sequence, 1);
        assert.deepEqual(frame.scope, { kind: 'application_context', applicationContextId: CONTEXT });
        assert.deepEqual(frame.payload, { turnId: 'turn_1', text: 'Done.' });
        // Bookkeeping the store keeps is not for a browser. The Device is the exception, because the page
        // already carries it and the chat compares it: a frame without it would leave a window that
        // heard `session.created` live unable to tell that the Runtime has since been replaced.
        for (const field of ['streamKey', 'originMessageId', 'id']) assert.ok(!(field in frame), `${field} is not sent`);
        assert.equal(frame.deviceId, device.id, 'the frame names the Device that wrote it, as the page does');

        const page = await get(sidecar, `/pons/v1/events?scope=application_context&applicationContextId=${CONTEXT}&after=0`, 'session-user');
        const stored = isRecord(page.body) && Array.isArray(page.body.events) ? (page.body.events as unknown[]) : [];
        assert.ok(stored.some((event) => isRecord(event) && event.sequence === frame.sequence && event.type === 'agent.output.completed'));
        assert.deepEqual(theirs.events, []);
      } finally {
        mine.close();
        theirs.close();
        await device.close();
      }
    });
  });

  it('does not deliver a notification for a session that is not this Device\'s, or not bound at all', async () => {
    await withSidecar(async (sidecar) => {
      const device = await pairDevice(sidecar, USER);
      const other = await pairDevice(sidecar, OTHER_USER);
      // Bound to the other person's Device: this Device claiming it is not its own session.
      sidecar.controlPlane.bindSessionContext({ sessionId: 'ses_theirs', userId: OTHER_USER, deviceId: other.id, applicationContextId: OTHER_CONTEXT });
      const mine = await openWindow(sidecar, 'session-user');
      const theirs = await openWindow(sidecar, 'session-other');
      try {
        device.publish('agent.output.delta', { text: 'into someone else\'s session' }, 'ses_theirs');
        device.publish('agent.output.delta', { text: 'into nothing' }, 'ses_unbound');
        // A marker the owner does hear, sent after, so absence above is not just slowness.
        device.publish('device.status', { status: 'ready' });
        await mine.next(isType('device.status'));
        assert.deepEqual(mine.events.map((event) => event.type), ['device.status']);
        assert.deepEqual(theirs.events, []);
      } finally {
        mine.close();
        theirs.close();
        await device.close();
        await other.close();
      }
    });
  });

  it('delivers a session-less notification to the owner\'s windows only', async () => {
    await withSidecar(async (sidecar) => {
      const device = await pairDevice(sidecar, USER);
      const mine = await openWindow(sidecar, 'session-user');
      const theirs = await openWindow(sidecar, 'session-other');
      try {
        device.publish('device.status', { status: 'ready' });
        await mine.next(isType('device.status'));
        await new Promise((resolve) => setTimeout(resolve, 50));
        assert.deepEqual(theirs.events, []);
      } finally {
        mine.close();
        theirs.close();
        await device.close();
      }
    });
  });
});

describe('GET /pons/v1/devices', () => {
  it('carries a boolean online from the live socket, and never stores it', async () => {
    await withSidecar(async (sidecar) => {
      const device = await pairDevice(sidecar, USER);
      const listed = async () => {
        const answer = await get(sidecar, '/pons/v1/devices', 'session-user');
        assert.equal(answer.status, 200);
        assert.ok(isRecord(answer.body) && Array.isArray(answer.body.devices));
        return answer.body.devices as Record<string, unknown>[];
      };

      const online = await listed();
      assert.equal(online.length, 1);
      assert.equal(online[0]?.id, device.id);
      assert.equal(online[0]?.online, true);

      await device.close();
      // The socket's close is observed on the server a moment later.
      const deadline = Date.now() + 2_000;
      let after = await listed();
      while (after[0]?.online === true && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 20));
        after = await listed();
      }
      assert.equal(after[0]?.online, false, 'offline once the socket is gone');
      assert.ok(!('online' in (await sidecar.controlPlane.getDevice(device.id) ?? {})), 'presence is not on the stored record');
    });
  });

  it('lists only the caller\'s devices and returns a typed 401 without a valid session', async () => {
    await withSidecar(async (sidecar) => {
      const device = await pairDevice(sidecar, USER);
      try {
        const theirs = await get(sidecar, '/pons/v1/devices', 'session-other');
        assert.deepEqual(isRecord(theirs.body) ? theirs.body.devices : null, []);
        const anonymous = await get(sidecar, '/pons/v1/devices');
        assert.equal(anonymous.status, 401);
        assert.deepEqual(anonymous.body, { error: { code: 'unauthenticated' } });
        const invalid = await get(sidecar, '/pons/v1/devices', 'not-a-session');
        assert.equal(invalid.status, 401);
        assert.deepEqual(invalid.body, { error: { code: 'unauthenticated' } });
      } finally {
        await device.close();
      }
    });
  });
});

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

describe('a failure while delivering costs the frame, not the process', () => {
  /** Fails the test if anything in the process rejects without a handler. */
  function watchUnhandled(): { unhandled: unknown[]; stop(): void } {
    const unhandled: unknown[] = [];
    const listener = (reason: unknown) => unhandled.push(reason);
    process.on('unhandledRejection', listener);
    return { unhandled, stop: () => void process.off('unhandledRejection', listener) };
  }

  const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

  // Only what the delivery reads of them; nothing here is a real Control Plane.
  const delta = { protocol: 'pons/1.0', type: 'agent.output.delta', sentAt: 'now', payload: { text: 'a private sentence' } };

  it('logs one line and drops the frame when the device lookup rejects', async () => {
    const watch = watchUnhandled();
    const logged: { event: string; fields: LogFields | undefined }[] = [];
    const controlPlane = { getDevice: () => Promise.reject(new TypeError('lookup failed: a private sentence')) };
    const delivered: unknown[] = [];
    const windows = { all: () => [], notifyContext: (...args: unknown[]) => delivered.push(args), notifyPrincipal: (...args: unknown[]) => delivered.push(args) };
    try {
      liveNotification(controlPlane as never, windows, (event, fields) => logged.push({ event, fields }))('dev_x', delta);
      await settle();
      assert.deepEqual(watch.unhandled, [], 'nothing was left unhandled');
      assert.deepEqual(delivered, []);
      assert.equal(logged.length, 1, 'logged once');
      assert.deepEqual(logged[0], { event: 'live_delivery_dropped', fields: { source: 'notification', type: 'agent.output.delta', error: 'TypeError' } });
      // Neither the payload nor the error's own text is in the line.
      assert.ok(!JSON.stringify(logged).includes('private sentence'));
    } finally {
      watch.stop();
    }
  });

  it('does the same when a window\'s write throws inside the callback', async () => {
    const watch = watchUnhandled();
    const logged: string[] = [];
    const controlPlane = { getDevice: () => Promise.resolve({ userId: USER }) };
    const windows = {
      all: () => [],
      notifyContext: () => undefined,
      notifyPrincipal: () => {
        throw new Error('the stream was closed under us');
      },
    };
    try {
      liveNotification(controlPlane as never, windows as never, (event) => logged.push(event))('dev_x', delta);
      await settle();
      assert.deepEqual(watch.unhandled, []);
      assert.deepEqual(logged, ['live_delivery_dropped']);
    } finally {
      watch.stop();
    }
  });

  it('keeps a throwing window out of the event store, and says so once', () => {
    const logged: { event: string; fields: LogFields | undefined }[] = [];
    let subscriber: ((event: never) => void) | undefined;
    const controlPlane = { events: { subscribeAll: (listener: (event: never) => void) => ((subscriber = listener), () => undefined) } };
    const windows = {
      all: () => [],
      notifyContext: () => undefined,
      notifyPrincipal: () => {
        throw new Error('boom');
      },
    };
    fanOutDurable(controlPlane as never, windows as never, (event, fields) => logged.push({ event, fields }));
    const stored = { sequence: 1, type: 'device.revoked', scope: { kind: 'principal', applicationId: 'forma', userId: USER }, payload: {}, createdAt: 'now' };
    assert.doesNotThrow(() => subscriber?.(stored as never));
    assert.deepEqual(logged, [{ event: 'live_delivery_dropped', fields: { source: 'durable', type: 'device.revoked', error: 'Error' } }]);
  });
});
