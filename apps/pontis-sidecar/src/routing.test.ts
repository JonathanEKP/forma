import { strict as assert } from 'node:assert';
import { generateKeyPairSync } from 'node:crypto';
import { createServer } from 'node:http';
import { describe, it } from 'node:test';

import { ControlPlane, DeviceGateway, type IntentEnvelope } from '@pontis/control-plane';

import { FormaBoundary } from './boundary.ts';
import { createIntentRoute } from './intents.ts';
import { silentLog } from './log.ts';
import type { Person } from './person.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const OTHER_USER = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const CONTEXT = '22222222-2222-4222-8222-222222222222';
const OTHER_CONTEXT = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const PERSON: Person = { userId: USER, applicationContextId: CONTEXT };
const SESSION = 'ses_0123456789abcdef0123456789abcdef';

interface RouteResponse {
  readonly status: number;
  readonly body: Record<string, unknown>;
}

interface RoutingHarness {
  readonly controlPlane: ControlPlane;
  readonly online: Set<string>;
  readonly sent: IntentEnvelope[];
  readonly pair: (userId?: string) => Promise<string>;
  readonly intent: (body: unknown) => Promise<RouteResponse>;
}

async function withRoutingHarness(run: (harness: RoutingHarness) => Promise<void>): Promise<void> {
  const controlPlane = new ControlPlane({ applicationId: 'forma', audience: 'http://127.0.0.1:4767' });
  const devices = new DeviceGateway({ controlPlane });
  const online = new Set<string>();
  const sent: IntentEnvelope[] = [];
  devices.isOnline = (deviceId) => online.has(deviceId);
  devices.sendIntentForContext = (_context, envelope) => {
    sent.push(envelope);
    return Promise.resolve({ authorized: true, delivered: true });
  };

  const handler = createIntentRoute({
    boundary: new FormaBoundary('http://127.0.0.1:1', 'forma_svc_test', silentLog),
    controlPlane,
    devices,
    resolvePerson: () => Promise.resolve(PERSON),
    log: silentLog,
  });
  const server = createServer((request, response) => {
    void handler(request, response).then((handled) => {
      if (!handled) response.writeHead(404).end();
    });
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  const origin = `http://127.0.0.1:${address.port}`;

  try {
    await run({
      controlPlane,
      online,
      sent,
      pair: async (userId = USER) => {
        const { code } = await controlPlane.createPairingCode(userId);
        const publicJwk = generateKeyPairSync('ed25519').publicKey.export({ format: 'jwk' });
        assert.ok(typeof publicJwk.x === 'string');
        const device = await controlPlane.claimPairing({
          code,
          applicationId: 'forma',
          publicKey: publicJwk.x,
          metadata: { name: 'Workstation', platform: 'linux', runtimeVersion: '0.1.0' },
        });
        return device.id;
      },
      intent: async (body) => {
        const response = await fetch(`${origin}/agent/intent`, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(body),
        });
        const parsed: unknown = await response.json();
        assert.ok(isRecord(parsed));
        return { status: response.status, body: parsed };
      },
    });
  } finally {
    await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
    await devices.close();
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

describe('device routing for agent intents', () => {
  it('uses the only online device when session.start names none', async () => {
    await withRoutingHarness(async ({ online, pair, intent, sent }) => {
      const deviceId = await pair();
      online.add(deviceId);

      const response = await intent({ type: 'session.start', payload: { providerId: 'codex' } });

      assert.equal(response.status, 200);
      assert.equal(response.body.delivered, true);
      assert.equal(typeof response.body.sessionId, 'string');
      assert.equal(sent[0]?.deviceId, deviceId);
      const runtimePayload = sent[0]?.payload;
      assert.ok(isRecord(runtimePayload));
      assert.equal(Object.hasOwn(runtimePayload, 'deviceId'), false);
    });
  });

  it('requires a choice when several own devices are online', async () => {
    await withRoutingHarness(async ({ online, pair, intent, sent }) => {
      online.add(await pair());
      online.add(await pair());

      const response = await intent({ type: 'session.start', payload: { providerId: 'codex' } });

      assert.equal(response.status, 409);
      assert.equal(response.body.error, 'device_choice_required');
      assert.equal(sent.length, 0, 'no session starts before the person chooses a device');
    });
  });

  it('routes a named online choice even when other devices are online', async () => {
    await withRoutingHarness(async ({ online, pair, intent, sent }) => {
      const chosen = await pair();
      online.add(await pair());
      online.add(chosen);

      const response = await intent({ type: 'session.start', payload: { providerId: 'codex', deviceId: chosen } });

      assert.equal(response.status, 200);
      assert.equal(sent[0]?.deviceId, chosen);
    });
  });

  it('refuses a named device that is offline instead of switching to another online device', async () => {
    await withRoutingHarness(async ({ online, pair, intent, sent }) => {
      const chosen = await pair();
      online.add(await pair());

      const response = await intent({ type: 'session.start', payload: { providerId: 'codex', deviceId: chosen } });

      assert.equal(response.status, 409);
      assert.equal(response.body.error, 'device_offline');
      assert.equal(sent.length, 0);
    });
  });

  it('does not accept or identify another person\'s device id', async () => {
    await withRoutingHarness(async ({ online, pair, intent, sent }) => {
      const foreign = await pair(OTHER_USER);
      online.add(await pair());

      const response = await intent({ type: 'session.start', payload: { providerId: 'codex', deviceId: foreign } });

      assert.equal(response.status, 409);
      assert.equal(response.body.error, 'device_offline');
      assert.equal(JSON.stringify(response.body).includes(foreign), false);
      assert.equal(sent.length, 0);
    });
  });

  it('does not accept a revoked device id', async () => {
    await withRoutingHarness(async ({ controlPlane, online, pair, intent, sent }) => {
      const revoked = await pair();
      await controlPlane.revokeDevice(USER, revoked);
      online.add(await pair());

      const response = await intent({ type: 'session.start', payload: { providerId: 'codex', deviceId: revoked } });

      assert.equal(response.status, 409);
      assert.equal(response.body.error, 'device_offline');
      assert.equal(sent.length, 0);
    });
  });

  it('routes input to its session device while another device is online', async () => {
    await withRoutingHarness(async ({ controlPlane, online, pair, intent, sent }) => {
      const sessionDevice = await pair();
      const otherDevice = await pair();
      controlPlane.bindSessionContext({ sessionId: SESSION, userId: USER, deviceId: sessionDevice, applicationContextId: CONTEXT });
      online.add(sessionDevice);
      online.add(otherDevice);

      const response = await intent({ type: 'session.input', payload: { sessionId: SESSION, text: 'Continue' } });

      assert.equal(response.status, 200);
      assert.equal(sent[0]?.deviceId, sessionDevice);
    });
  });

  it('returns device_offline when the session device is offline, even if another is online', async () => {
    await withRoutingHarness(async ({ controlPlane, online, pair, intent, sent }) => {
      const sessionDevice = await pair();
      online.add(await pair());
      controlPlane.bindSessionContext({ sessionId: SESSION, userId: USER, deviceId: sessionDevice, applicationContextId: CONTEXT });

      const response = await intent({ type: 'session.cancel', payload: { sessionId: SESSION } });

      assert.equal(response.status, 409);
      assert.equal(response.body.error, 'device_offline');
      assert.equal(sent.length, 0);
    });
  });

  it('returns device_offline when the session device is revoked', async () => {
    await withRoutingHarness(async ({ controlPlane, online, pair, intent, sent }) => {
      const revoked = await pair();
      const other = await pair();
      await controlPlane.revokeDevice(USER, revoked);
      controlPlane.bindSessionContext({ sessionId: SESSION, userId: USER, deviceId: revoked, applicationContextId: CONTEXT });
      online.add(other);

      const response = await intent({ type: 'session.input', payload: { sessionId: SESSION, text: 'Continue' } });

      assert.equal(response.status, 409);
      assert.equal(response.body.error, 'device_offline');
      assert.equal(sent.length, 0);
    });
  });

  it('routes cancel to the session device while another device is online', async () => {
    await withRoutingHarness(async ({ controlPlane, online, pair, intent, sent }) => {
      const sessionDevice = await pair();
      online.add(sessionDevice);
      online.add(await pair());
      controlPlane.bindSessionContext({ sessionId: SESSION, userId: USER, deviceId: sessionDevice, applicationContextId: CONTEXT });

      const response = await intent({ type: 'session.cancel', payload: { sessionId: SESSION } });

      assert.equal(response.status, 200);
      assert.equal(sent[0]?.deviceId, sessionDevice);
    });
  });

  it('returns the same device_offline refusal for a session bound to another person', async () => {
    await withRoutingHarness(async ({ controlPlane, online, pair, intent, sent }) => {
      const foreignDevice = await pair(OTHER_USER);
      online.add(await pair());
      controlPlane.bindSessionContext({ sessionId: SESSION, userId: OTHER_USER, deviceId: foreignDevice, applicationContextId: OTHER_CONTEXT });

      const response = await intent({ type: 'session.input', payload: { sessionId: SESSION, text: 'Continue' } });

      assert.equal(response.status, 409);
      assert.equal(response.body.error, 'device_offline');
      assert.equal(sent.length, 0);
    });
  });

  it('returns device_offline for an approval that has no device binding for this person', async () => {
    await withRoutingHarness(async ({ online, pair, intent, sent }) => {
      online.add(await pair());

      const response = await intent({ type: 'approval.resolve', payload: { approvalId: 'apr_other', decision: 'deny' } });

      assert.equal(response.status, 409);
      assert.equal(response.body.error, 'device_offline');
      assert.equal(sent.length, 0);
    });
  });

  it('routes an approval answer through its verified session device', async () => {
    await withRoutingHarness(async ({ controlPlane, online, pair, intent, sent }) => {
      const sessionDevice = await pair();
      const otherDevice = await pair();
      controlPlane.bindSessionContext({ sessionId: SESSION, userId: USER, deviceId: sessionDevice, applicationContextId: CONTEXT });
      await controlPlane.appendEvent({
        userId: USER,
        applicationContextId: CONTEXT,
        type: 'approval.requested',
        sessionId: SESSION,
        deviceId: sessionDevice,
        payload: { approvalId: 'apr_route' },
        originMessageId: 'msg_approval_route',
      });
      online.add(sessionDevice);
      online.add(otherDevice);

      const response = await intent({ type: 'approval.resolve', payload: { approvalId: 'apr_route', decision: 'approveOnce' } });

      assert.equal(response.status, 200);
      assert.equal(sent[0]?.deviceId, sessionDevice);
    });
  });
});
