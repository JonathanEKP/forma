/**
 * The capabilities as Pontis sees them: built from the generated catalog and
 * nothing else, registrable, and elevated the way Boundary F expects.
 */
import { strict as assert } from 'node:assert';
import { describe, it } from 'node:test';

import { defineCapability, defineUiCapability, type ApprovalEvidence } from '@pontis/core';
import { defineAgentApp } from '@pontis/server';

import { capabilityCatalog } from '../../../bindings/generated/capabilityCatalog.ts';
import { FormaBoundary } from './boundary.ts';
import { SERVED, formaApproval, formaCapabilities, isUuid, removals } from './capabilities.ts';
import { fakeForma } from './fake-forma.ts';
import { GUIDANCE } from './guidance.ts';
import { startSidecar } from './sidecar.ts';
import { silentLog } from './log.ts';
import { UI_ACTIONS } from './ui.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const CONTEXT = '22222222-2222-4222-8222-222222222222';
const DESIGN = '33333333-3333-4333-8333-333333333333';
const LAYER = '44444444-4444-4444-8444-444444444444';
const SESSION = 'ses_capabilities';
const MANUAL = { revision: 'blake3:0000', text: '# Forma\n' };
const PROJECT = '55555555-5555-4555-8555-555555555555';
const BRAND = { brand: null, fonts: ['Geist', 'Geist Mono'] };
const REQUESTED = {
  id: '66666666-6666-4666-8666-666666666666',
  project_id: PROJECT,
  purpose: 'hero photo',
  description: 'A wet street at dusk.',
  status: 'pending',
  created_at: '2026-10-01T00:00:00Z',
};

describe('the catalog, as registered', () => {
  it('declares exactly what the generated catalog says, for each operation Boundary F serves', () => {
    const capabilities = formaCapabilities(new FormaBoundary('http://127.0.0.1:1', 'unused', silentLog));
    assert.deepEqual(capabilities.map((capability) => capability.id), [...SERVED]);
    for (const capability of capabilities) {
      const entry = capabilityCatalog.find(({ id }) => id === capability.id);
      assert.ok(entry, `${capability.id} is in the catalog`);
      const { outputSchema, ...declared } = capability.descriptor();
      // Every field a provider reads, except the output schema D5 leaves to the
      // sidecar, is the generated one — nothing here is hand-written.
      assert.deepEqual(declared, {
        id: entry.id,
        kind: entry.kind,
        description: entry.description,
        inputSchema: entry.inputSchema,
        risk: entry.risk,
        bounds: entry.bounds,
        ...(entry.presentation ? { presentation: entry.presentation } : {}),
      });
      assert.equal(outputSchema.type, 'object', `${capability.id} answers an object, which MCP structured content requires`);
    }
  });

  it('holds every catalog entry, not just the served ones, to what Pontis accepts at registration', () => {
    // The step 6 catalog shipped four ids defineCapability refuses. This is the
    // Pontis-side half of the guard whose Rust half is
    // `every_id_is_one_pontis_will_register`: it runs the real registration.
    for (const entry of capabilityCatalog) {
      const input = entry.inputSchema;
      assert.ok(typeof input === 'object' && input !== null && !Array.isArray(input), `${entry.id} has an object schema`);
      if (entry.kind === 'ui') {
        defineUiCapability({ id: entry.id, description: entry.description, input, bounds: entry.bounds, authorize: () => false });
      } else {
        defineCapability({
          id: entry.id,
          kind: entry.kind,
          description: entry.description,
          input,
          output: { type: 'object' },
          risk: entry.risk,
          bounds: entry.bounds,
          ...(entry.presentation ? { presentation: entry.presentation } : {}),
          authorize: () => false,
          execute: () => ({}),
        });
      }
    }
    // And Pontis's application registration, guidance included, accepts the set.
    defineAgentApp({ id: 'forma', guidance: GUIDANCE, capabilities: [], resolveUser: () => undefined });
  });

  it('counts removals by operation, ignoring anything it does not recognise', () => {
    assert.equal(removals([{ op: 'patch_text_style' }, { op: 'add_page' }]), 0);
    assert.equal(removals([{ op: 'remove_layer' }, { op: 'set_text' }, { op: 'remove_page' }]), 2);
    // Forma refuses an unknown operation outright, so no prompt is owed for one.
    assert.equal(removals([{ op: 'remove_everything' }, { op: 'toString' }, {}, null, 'remove_layer']), 0);
  });

  it('forwards approval evidence in the shape Boundary F reads, without the context Forma already has', () => {
    const evidence: ApprovalEvidence = {
      approvalId: 'apr_1',
      sessionId: SESSION,
      applicationContextId: CONTEXT,
      capabilityId: 'design.applyOperations',
      scope: { kind: 'once', invocationId: 'inv_1' },
      risk: 'destructive',
      grantedAt: '2026-10-01T00:00:00Z',
      expiresAt: '2026-10-01T00:05:00Z',
    };
    // Forma's ApprovalEvidence refuses unknown fields, so an extra one would be
    // a refusal rather than an ignored value.
    assert.deepEqual(formaApproval(evidence), {
      approval_id: 'apr_1',
      session_id: SESSION,
      capability_id: 'design.applyOperations',
      scope: { kind: 'once', invocation_id: 'inv_1' },
      risk: 'destructive',
      granted_at: '2026-10-01T00:00:00Z',
      expires_at: '2026-10-01T00:05:00Z',
    });
  });

  it('accepts only an AccountId-shaped context and a subject-shaped user', () => {
    assert.ok(isUuid(USER));
    assert.ok(!isUuid('ctx_demo'));
    assert.ok(!isUuid(`${USER} `));
  });
});

describe('the gateway in front of Boundary F', () => {
  async function withGateway(run: (invoke: (capabilityId: string, input: unknown, invocationId: string, approval?: ApprovalEvidence) => ReturnType<ReturnType<typeof defineAgentApp>['gateway']['invoke']>, requests: () => string[]) => Promise<void>) {
    const forma = await fakeForma(({ path }) =>
      path.endsWith('/operations')
        ? { status: 200, body: { design_id: DESIGN, project_id: DESIGN, version: 2, updated_at: '2026-10-01T00:00:00Z' } }
        : path.endsWith('/guide')
          ? { status: 200, body: MANUAL }
          : path.endsWith('/brand')
            ? { status: 200, body: BRAND }
            : path.endsWith('/asset-requests')
              ? { status: 200, body: REQUESTED }
              : { status: 200, body: [] },
    );
    const app = defineAgentApp({
      id: 'forma',
      guidance: GUIDANCE,
      capabilities: formaCapabilities(new FormaBoundary(forma.url, 'forma_svc_test', silentLog)),
      resolveUser: (userId) => (isUuid(userId) ? { id: userId } : undefined),
    });
    try {
      await run(
        (capabilityId, input, invocationId, approval) =>
          app.gateway.invoke({
            invocationId,
            sessionId: SESSION,
            applicationContextId: CONTEXT,
            userId: USER,
            capabilityId,
            input,
            ...(approval ? { approval } : {}),
          }),
        () => forma.requests.map(({ method, path, body }) => `${method} ${path} ${body}`),
      );
    } finally {
      await forma.close();
    }
  }

  it('lets an ordinary batch through as a write, with no consent asked', async () => {
    await withGateway(async (invoke, requests) => {
      const outcome = await invoke(
        'design.applyOperations',
        { design_id: DESIGN, base_version: 1, operations: [{ op: 'patch_text_style', layer_id: LAYER, font_size: 96 }] },
        'inv_write',
      );
      assert.equal(outcome.status, 'completed');
      assert.equal(requests().length, 1);
    });
  });

  it('elevates a removal to destructive and reaches Forma only once consent is attached', async () => {
    await withGateway(async (invoke, requests) => {
      const input = { design_id: DESIGN, base_version: 1, operations: [{ op: 'remove_layer', layer_id: LAYER }] };
      const pending = await invoke('design.applyOperations', input, 'inv_remove');
      assert.equal(pending.status, 'approval_required');
      assert.equal(pending.status === 'approval_required' ? pending.approval.risk : undefined, 'destructive');
      assert.deepEqual(requests(), [], 'nothing reaches Forma before consent');

      const approved = await invoke('design.applyOperations', input, 'inv_remove', {
        approvalId: 'apr_remove',
        sessionId: SESSION,
        applicationContextId: CONTEXT,
        capabilityId: 'design.applyOperations',
        scope: { kind: 'once', invocationId: 'inv_remove' },
        risk: 'destructive',
        grantedAt: new Date(Date.now() - 1_000).toISOString(),
        expiresAt: new Date(Date.now() + 60_000).toISOString(),
      });
      assert.equal(approved.status, 'completed');
      const [sent] = requests();
      assert.ok(sent?.includes('"approval":{"approval_id":"apr_remove"'), sent);
    });
  });

  it('reads the manual as an ordinary resource: no consent, no idempotency key, exactly what Forma answered', async () => {
    await withGateway(async (invoke, requests) => {
      const outcome = await invoke('forma.guide', {}, 'inv_guide');
      assert.equal(outcome.status, 'completed');
      assert.deepEqual(outcome.status === 'completed' ? outcome.result : undefined, MANUAL);
      assert.deepEqual(requests(), ['GET /api/v1/pontis/guide ']);
    });
  });

  it('reads the Brand Kit with the font catalog the renderer can draw', async () => {
    await withGateway(async (invoke, requests) => {
      const outcome = await invoke('brand.get', { project_id: PROJECT }, 'inv_brand');
      assert.equal(outcome.status, 'completed');
      assert.deepEqual(outcome.status === 'completed' ? outcome.result : undefined, BRAND);
      assert.deepEqual(requests(), [`GET /api/v1/pontis/projects/${PROJECT}/brand `]);
    });
  });

  it('requests an asset as a write carrying its invocation id, with the project in the path', async () => {
    const forma = await fakeForma(() => ({ status: 200, body: REQUESTED }));
    const app = defineAgentApp({
      id: 'forma',
      guidance: GUIDANCE,
      capabilities: formaCapabilities(new FormaBoundary(forma.url, 'forma_svc_test', silentLog)),
      resolveUser: (userId) => (isUuid(userId) ? { id: userId } : undefined),
    });
    try {
      const outcome = await app.gateway.invoke({
        invocationId: 'inv_asset',
        sessionId: SESSION,
        applicationContextId: CONTEXT,
        userId: USER,
        capabilityId: 'assetRequest.create',
        input: { project_id: PROJECT, purpose: 'hero photo', description: 'A wet street at dusk.' },
      });
      assert.equal(outcome.status, 'completed');
      const [sent] = forma.requests;
      assert.equal(sent?.path, `/api/v1/pontis/projects/${PROJECT}/asset-requests`);
      assert.equal(sent?.headers['x-forma-invocation-id'], 'inv_asset');
      // Every field Forma's body declares, and nothing it would refuse.
      assert.deepEqual(JSON.parse(sent?.body ?? '{}'), {
        design_id: null,
        purpose: 'hero photo',
        description: 'A wet street at dusk.',
        suggested_prompt: null,
        requested_width: null,
        requested_height: null,
      });
    } finally {
      await forma.close();
    }
  });

  it('refuses a context that is not an account before any domain work', async () => {
    await withGateway(async (_invoke, requests) => {
      const app = defineAgentApp({
        id: 'forma',
        guidance: GUIDANCE,
        capabilities: formaCapabilities(new FormaBoundary('http://127.0.0.1:1', 'unused', silentLog)),
        resolveUser: (userId) => ({ id: userId }),
      });
      const outcome = await app.gateway.invoke({
        invocationId: 'inv_ctx',
        sessionId: SESSION,
        applicationContextId: 'ctx_demo',
        userId: USER,
        capabilityId: 'project.list',
        input: {},
      });
      assert.equal(outcome.status === 'failed' ? outcome.error.code : outcome.status, 'authorization_denied');
      assert.deepEqual(requests(), []);
    });
  });
});

describe('the composition', () => {
  it('starts one process holding the gateway, control plane and device socket, publishing only what Boundary F serves', async () => {
    const sidecar = await startSidecar({
      formaUrl: 'http://127.0.0.1:1',
      serviceCredential: 'forma_svc_test',
      host: '127.0.0.1',
      port: 0,
      log: silentLog,
    });
    try {
      const discovered = await sidecar.agentApp.gateway.discoverApplication(USER, SESSION, CONTEXT);
      // Boundary F's operations plus the four editor actions, which are routed
      // to the person's window rather than served here (see ui.ts).
      assert.deepEqual(
        discovered.capabilities.map(({ id }) => id).sort(),
        [...SERVED, ...UI_ACTIONS].sort(),
      );
      assert.equal(discovered.guidance.version, 1);

      const devices = await fetch(`${sidecar.origin}/pons/v1/devices`);
      assert.equal(devices.status, 403, 'a request with no Forma session sees no device');
      const intent = await fetch(`${sidecar.origin}/agent/intent`, { method: 'POST', body: '{}' });
      assert.equal(intent.status, 403);
      const unknown = await fetch(`${sidecar.origin}/api/v1/pontis/projects`);
      assert.equal(unknown.status, 404, 'there is no generic proxy to Forma here');
    } finally {
      await sidecar.close();
    }
  });
});
