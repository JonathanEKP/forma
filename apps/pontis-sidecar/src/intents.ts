/**
 * How the person's browser talks to their paired Runtime: start a session, send
 * input, cancel, answer an approval. Pontis exports the delivery
 * (`DeviceGateway.sendIntentForContext`, which re-checks the user and context
 * against the session) but not this route, because who the person is and which
 * of their Devices to address are the host's to decide.
 *
 * Every payload is rebuilt field by field, never spread from the request: the
 * application context comes from the resolved person, the approver is the
 * person, and the session id is minted here. Nothing the page sends can widen
 * any of them.
 */
import { randomUUID } from 'node:crypto';
import type { IncomingMessage, ServerResponse } from 'node:http';

import type { DeviceGateway, ControlPlane, IntentType } from '@pontis/control-plane';
import { PONS_PROTOCOL_VERSION } from '@pontis/core';

import type { Log } from './log.ts';
import type { Person, ResolvePerson } from './person.ts';

export const INTENT_PATH = '/agent/intent';
export const CONTEXT_PATH = '/agent/context';

const MAX_BODY_BYTES = 64 * 1024;

/** PV-1 needs `once` only (plan §3.1); a session grant is not offered. */
const DECISIONS = ['approveOnce', 'deny'] as const;

export type IntentHandler = (request: IncomingMessage, response: ServerResponse) => Promise<boolean>;

export function createIntentRoute(options: {
  readonly controlPlane: ControlPlane;
  readonly devices: DeviceGateway;
  readonly resolvePerson: ResolvePerson;
  readonly log: Log;
}): IntentHandler {
  return async (request, response) => {
    if ((request.url ?? '').split('?')[0] !== INTENT_PATH) return false;
    if (request.method !== 'POST') return reply(response, 405, { error: 'method_not_allowed' });

    const person = await options.resolvePerson(request);
    if (!person) return reply(response, 403, { error: 'not_authenticated' });

    let body: unknown;
    try {
      body = await readBody(request);
    } catch {
      return reply(response, 400, { error: 'bad_request' });
    }
    const intent = rebuild(body, person);
    if (!intent) return reply(response, 400, { error: 'envelope_malformed' });

    const online = (await options.controlPlane.listDevices(person.userId)).filter(
      (device) => !device.revokedAt && options.devices.isOnline(device.id),
    );
    if (online.length !== 1) {
      return reply(response, 409, { error: online.length === 0 ? 'runtime_offline' : 'ambiguous_device' });
    }
    const [device] = online;
    if (!device) return reply(response, 409, { error: 'runtime_offline' });
    const delivery = await options.devices.sendIntentForContext(person, {
      protocol: PONS_PROTOCOL_VERSION,
      messageId: `msg_${randomUUID().replaceAll('-', '')}`,
      type: intent.type,
      deviceId: device.id,
      sentAt: new Date().toISOString(),
      payload: intent.payload,
    });
    options.log('intent', { type: intent.type, delivered: delivery.delivered, authorized: delivery.authorized });
    if (!delivery.authorized) return reply(response, 403, { error: 'not_authorized' });
    return reply(response, 200, {
      delivered: delivery.delivered,
      ...(intent.type === 'session.start' ? { sessionId: intent.sessionId } : {}),
    });
  };
}

/**
 * Which application context the person's session is bound to: the one value the
 * event stream asks the browser to name and the browser has no way to learn.
 *
 * `GET /pons/v1/events?scope=application_context` refuses a context the caller
 * does not already hold, and every session event lives in that scope. It is
 * served from here, off the same `resolvePerson` the Control Plane routes ask,
 * rather than derived a second way in forma-server — so the page is told exactly
 * what the session is bound to and the two cannot disagree. The answer carries
 * the id and nothing else: no user, no role.
 */
export function createContextRoute(options: { readonly resolvePerson: ResolvePerson }): IntentHandler {
  return async (request, response) => {
    if ((request.url ?? '').split('?')[0] !== CONTEXT_PATH) return false;
    if (request.method !== 'GET') return reply(response, 405, { error: 'method_not_allowed' });
    const person = await options.resolvePerson(request);
    if (!person) return reply(response, 403, { error: 'not_authenticated' });
    return reply(response, 200, { applicationContextId: person.applicationContextId });
  };
}

type Rebuilt =
  | { readonly type: 'session.start'; readonly sessionId: string; readonly payload: unknown }
  | { readonly type: Exclude<IntentType, 'session.start'>; readonly payload: unknown };

function rebuild(body: unknown, person: Person): Rebuilt | undefined {
  if (!isRecord(body) || !isRecord(body.payload)) return undefined;
  const { payload } = body;
  switch (body.type) {
    case 'session.start': {
      if (!isNonEmpty(payload.providerId)) return undefined;
      const sessionId = `ses_${randomUUID().replaceAll('-', '')}`;
      return {
        type: 'session.start',
        sessionId,
        payload: {
          sessionId,
          providerId: payload.providerId,
          appContext: { applicationContextId: person.applicationContextId },
        },
      };
    }
    case 'session.input':
      if (!isNonEmpty(payload.sessionId) || typeof payload.text !== 'string') return undefined;
      return { type: 'session.input', payload: { sessionId: payload.sessionId, text: payload.text } };
    case 'session.cancel':
      if (!isNonEmpty(payload.sessionId)) return undefined;
      return { type: 'session.cancel', payload: { sessionId: payload.sessionId } };
    case 'approval.resolve': {
      const decision = DECISIONS.find((candidate) => candidate === payload.decision);
      if (!isNonEmpty(payload.approvalId) || !decision) return undefined;
      return {
        type: 'approval.resolve',
        payload: { approvalId: payload.approvalId, decision, resolvedBy: person.userId },
      };
    }
    default:
      return undefined;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isNonEmpty(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 128;
}

async function readBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request as AsyncIterable<unknown>) {
    if (!(chunk instanceof Uint8Array)) throw new Error('unreadable body');
    size += chunk.byteLength;
    if (size > MAX_BODY_BYTES) throw new Error('body too large');
    chunks.push(Buffer.from(chunk));
  }
  const parsed: unknown = JSON.parse(Buffer.concat(chunks).toString('utf8'));
  return parsed;
}

function reply(response: ServerResponse, status: number, body: unknown): true {
  const bytes = Buffer.from(JSON.stringify(body));
  response.writeHead(status, {
    'content-type': 'application/json',
    'content-length': String(bytes.length),
    'cache-control': 'no-store',
  });
  response.end(bytes);
  return true;
}
