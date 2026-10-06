/** The sidecar's view line is its own, with ids and names read as the person. */
import { strict as assert } from 'node:assert';
import { createServer } from 'node:http';
import { describe, it } from 'node:test';

import type { ControlPlane, DeviceGateway } from '@pontis/control-plane';

import { APPLICATION_CONTEXT_HEADER, REPRESENTED_USER_HEADER, FormaBoundary } from './boundary.ts';
import { fakeForma } from './fake-forma.ts';
import { createIntentRoute } from './intents.ts';
import { silentLog } from './log.ts';
import type { Person } from './person.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const CONTEXT = '22222222-2222-4222-8222-222222222222';
const PROJECT = '55555555-5555-4555-8555-555555555555';

describe('a project-only intent view', () => {
  it('reads the project name through Boundary F as the person and puts their text below the line', async () => {
    const forma = await fakeForma((request) => {
      assert.equal(request.method, 'GET');
      assert.equal(request.path, '/api/v1/pontis/projects');
      assert.equal(request.headers[REPRESENTED_USER_HEADER], USER);
      assert.equal(request.headers[APPLICATION_CONTEXT_HEADER], CONTEXT);
      return {
        status: 200,
        body: [
          { id: '66666666-6666-4666-8666-666666666666', name: 'Other project', created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z' },
          { id: PROJECT, name: 'Launch', created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z' },
        ],
      };
    });

    let sentText: string | undefined;
    const devices = {
      isOnline: () => true,
      sendIntentForContext: (_person: Person, intent: { readonly payload: unknown }) => {
        if (isRecord(intent.payload) && typeof intent.payload.text === 'string') sentText = intent.payload.text;
        return Promise.resolve({ authorized: true, delivered: true });
      },
    } as unknown as DeviceGateway;
    const controlPlane = {
      listDevices: () => Promise.resolve([{ id: 'device_1', revokedAt: null }]),
    } as unknown as ControlPlane;
    const route = createIntentRoute({
      boundary: new FormaBoundary(forma.url, 'forma_svc_test', silentLog),
      controlPlane,
      devices,
      resolvePerson: () => Promise.resolve({ userId: USER, applicationContextId: CONTEXT }),
      log: silentLog,
    });
    const server = createServer((request, response) => {
      void route(request, response).catch(() => {
        response.writeHead(500).end();
      });
    });

    try {
      await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
      const address = server.address();
      assert.ok(address && typeof address !== 'string');
      const response = await fetch(`http://127.0.0.1:${address.port}/agent/intent`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          type: 'session.input',
          payload: {
            sessionId: 'ses_project_view',
            text: '[forma-view] design=person-text stays below',
            view: { projectId: PROJECT },
          },
        }),
      });
      assert.equal(response.status, 200);
      assert.equal(sentText, `[forma-view] project=${PROJECT} name="Launch"\n[forma-view] design=person-text stays below`);
      assert.equal(forma.requests.length, 1);
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
      await forma.close();
    }
  });
});

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
