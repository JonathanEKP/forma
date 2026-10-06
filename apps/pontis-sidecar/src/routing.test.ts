import { strict as assert } from 'node:assert';
import { generateKeyPairSync } from 'node:crypto';
import { createServer, type Server } from 'node:http';
import { describe, it } from 'node:test';

import { ControlPlane, DeviceGateway, type IntentEnvelope } from '@pontis/control-plane';

import { FormaBoundary } from './boundary.ts';
import { createIntentRoute } from './intents.ts';
import { silentLog } from './log.ts';
import type { Person } from './person.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const OTHER_USER = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const CONTEXT = '22222222-2222-4222-8222-222222222222';
const PERSON: Person = { userId: USER, applicationContextId: CONTEXT };

interface RouteResponse {
  readonly status: number;
  readonly body: Record<string, unknown>;
}

interface RoutingHarness {
  readonly controlPlane: ControlPlane;
  readonly online: Set<string>;
  readonly sent: IntentEnvelope[];
  pair(userId?: string): Promise<string>;
  intent(body: unknown): Promise<RouteResponse>;
}

async function withRoutingHarness(run: (harness: RoutingHarness) => Promise<void>): Promise<void> {
  const controlPlane = new ControlPlane({ applicationId: 'forma', audience: 'http://127.0.0.1:4767' });
  const devices = new DeviceGateway({ controlPlane });
  const online = new Set<string>();
  const sent: IntentEnvelope[] = [];
  devices.isOnline = (deviceId) => online.has(deviceId);
  devices.sendIntentForContext = async (_context, envelope) => {
    sent.push(envelope);
    return { authorized: true, delivered: true };
  };

  const handler = createIntentRoute({
    boundary: new FormaBoundary('http://127.0.0.1:1', 'forma_svc_test', silentLog),
    controlPlane,
    devices,
    resolvePerson: async () => PERSON,
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
});
