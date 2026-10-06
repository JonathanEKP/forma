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

import type { FormaBoundary } from './boundary.ts';
import { isUuid, specOf } from './capabilities.ts';
import type { Log } from './log.ts';
import type { Person, ResolvePerson } from './person.ts';
import { authoritativeApprovalDevice, authoritativeSessionDevice } from './windows.ts';

export const INTENT_PATH = '/agent/intent';
export const CONTEXT_PATH = '/agent/context';

const MAX_BODY_BYTES = 64 * 1024;

/** PV-1 needs `once` only (plan §3.1); a session grant is not offered. */
const DECISIONS = ['approveOnce', 'deny'] as const;

/** How long the person's message waits for the name of the design they have open. */
const NAME_READ_MS = 3_000;

/** The longest design name the view line carries. */
const MAX_NAME_CHARS = 80;

export const VIEW_PREFIX = '[forma-view]';

/**
 * What the person is looking at, as the editor reports it. Every field is an id
 * the editor read off its own state, and none is trusted: `parseView` keeps a
 * field only if it is a UUID.
 */
export interface View {
  readonly projectId?: string;
  readonly designId?: string;
  readonly pageId?: string;
  readonly layerId?: string;
}

/**
 * The view as far as it is well formed. A field that is not a UUID is dropped,
 * never the message: a bad id costs the agent a hint, not the person a sentence.
 */
export function parseView(raw: unknown): View {
  if (!isRecord(raw)) return {};
  const field = (value: unknown): string | undefined => (typeof value === 'string' && isUuid(value) ? value.toLowerCase() : undefined);
  const projectId = field(raw.projectId);
  const designId = field(raw.designId);
  const pageId = field(raw.pageId);
  const layerId = field(raw.layerId);
  return {
    ...(projectId ? { projectId } : {}),
    ...(designId ? { designId } : {}),
    ...(pageId ? { pageId } : {}),
    ...(layerId ? { layerId } : {}),
  };
}

/**
 * A design name as one quoted token: control characters and line breaks gone,
 * at most 80 characters, and `\` and `"` escaped so the name cannot end its own
 * quotes. A name is the one free-text field of the line, and a person (or a
 * teammate) chose it.
 */
export function quotedName(name: string): string {
  const flat = [...name]
    .map((char) => (/[\p{Cc}\p{Cf}\u2028\u2029]/u.test(char) ? ' ' : char))
    .join('')
    .replace(/\s+/gu, ' ')
    .trim();
  const clipped = [...flat].slice(0, MAX_NAME_CHARS).join('');
  return `"${clipped.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

/**
 * The one line the sidecar puts in front of every message: where the person is.
 *
 * It is always the first line and always the sidecar's own — a message the
 * person types is the text after it, so a person who starts theirs with
 * `[forma-view]` has not written the line. `name` is left out when it could not
 * be read.
 */
export function viewLine(view: View, name?: string): string {
  const parts = [
    view.projectId ? `project=${view.projectId}` : undefined,
    view.designId ? `design=${view.designId}` : undefined,
    (view.designId || view.projectId) && name !== undefined ? `name=${quotedName(name)}` : undefined,
    view.pageId ? `page=${view.pageId}` : undefined,
    view.layerId ? `layer=${view.layerId}` : undefined,
  ].filter((part): part is string => part !== undefined);
  return `${VIEW_PREFIX} ${parts.length > 0 ? parts.join(' ') : 'none'}`;
}

/**
 * The open design's name, read through Boundary F as the person — so a design
 * they cannot read has no name here, and neither does one that is slow or gone.
 * Never throws: the message goes either way.
 */
async function designName(boundary: FormaBoundary, person: Person, designId: string): Promise<string | undefined> {
  try {
    const record = await boundary.getDesign(
      { userId: person.userId, applicationContextId: person.applicationContextId },
      designId,
      specOf('design.get').bounds.maxResultBytes,
      AbortSignal.timeout(NAME_READ_MS),
    );
    return typeof record.name === 'string' ? record.name : undefined;
  } catch {
    return undefined;
  }
}

/** The selected project, read through Boundary F as the person. */
async function projectName(boundary: FormaBoundary, person: Person, projectId: string): Promise<string | undefined> {
  try {
    const projects = await boundary.listProjects(
      { userId: person.userId, applicationContextId: person.applicationContextId },
      specOf('project.list').bounds.maxResultBytes,
      AbortSignal.timeout(NAME_READ_MS),
    );
    return projects.find((project) => project.id.toLowerCase() === projectId.toLowerCase())?.name;
  } catch {
    return undefined;
  }
}

export type IntentHandler = (request: IncomingMessage, response: ServerResponse) => Promise<boolean>;

export function createIntentRoute(options: {
  readonly boundary: FormaBoundary;
  readonly controlPlane: ControlPlane;
  readonly devices: DeviceGateway;
  readonly resolvePerson: ResolvePerson;
  readonly log: Log;
}): IntentHandler {
  return async (request, response) => {
    if ((request.url ?? '').split('?')[0] !== INTENT_PATH) return false;
    if (request.method !== 'POST') return reply(response, 405, { error: 'method_not_allowed' });

    const person = await options.resolvePerson(request);
    if (!person) return reply(response, 401, { error: { code: 'unauthenticated' } });

    let body: unknown;
    try {
      body = await readBody(request);
    } catch (cause) {
      if (cause instanceof BodyTooLarge) return reply(response, 413, { error: { code: 'message_too_large' } });
      return reply(response, 400, { error: 'bad_request' });
    }
    const intent = rebuild(body, person);
    if (!intent) return reply(response, 400, { error: 'envelope_malformed' });
    if (intent.type === 'session.input') {
      // Before the Runtime is asked for anything, so a name read that is slow
      // costs the message its three seconds and nothing else.
      const view = parseView(isRecord(body) && isRecord(body.payload) ? body.payload.view : undefined);
      const name = view.designId
        ? await designName(options.boundary, person, view.designId)
        : view.projectId
          ? await projectName(options.boundary, person, view.projectId)
          : undefined;
      intent.payload.text = `${viewLine(view, name)}\n${intent.payload.text}`;
    }

    const requestPayload = payloadRecord(body);
    let deviceId: string | undefined;
    if (intent.type === 'session.start') {
      const owned = (await options.controlPlane.listDevices(person.userId)).filter((device) => device.userId === person.userId);
      const online = owned.filter((device) => !device.revokedAt && options.devices.isOnline(device.id));
      const choiceWasNamed = requestPayload !== undefined && Object.hasOwn(requestPayload, 'deviceId');
      if (choiceWasNamed) {
        const chosen = typeof requestPayload.deviceId === 'string' ? owned.find((device) => device.id === requestPayload.deviceId) : undefined;
        if (!chosen || chosen.revokedAt || !options.devices.isOnline(chosen.id)) {
          return reply(response, 409, { error: 'device_offline' });
        }
        deviceId = chosen.id;
      } else {
        if (online.length === 0) return reply(response, 409, { error: 'runtime_offline' });
        if (online.length > 1) return reply(response, 409, { error: 'device_choice_required' });
        deviceId = online[0]?.id;
      }
    } else if (intent.type === 'approval.resolve') {
      const approvalId = isRecord(intent.payload) && isNonEmpty(intent.payload.approvalId) ? intent.payload.approvalId : undefined;
      const binding = approvalId ? await authoritativeApprovalDevice(options.controlPlane, person, approvalId) : undefined;
      if (!binding || binding.applicationContextId !== person.applicationContextId || binding.device.revokedAt || !options.devices.isOnline(binding.device.id)) {
        return reply(response, 409, { error: 'device_offline' });
      }
      deviceId = binding.device.id;
    } else {
      const sessionId = requestPayload && isNonEmpty(requestPayload.sessionId) ? requestPayload.sessionId : undefined;
      const binding = sessionId ? await authoritativeSessionDevice(options.controlPlane, person, sessionId) : undefined;
      if (!binding || binding.applicationContextId !== person.applicationContextId || binding.device.revokedAt || !options.devices.isOnline(binding.device.id)) {
        return reply(response, 409, { error: 'device_offline' });
      }
      deviceId = binding.device.id;
    }
    if (!deviceId) return reply(response, 409, { error: 'runtime_offline' });
    const delivery = await options.devices.sendIntentForContext(person, {
      protocol: PONS_PROTOCOL_VERSION,
      messageId: `msg_${randomUUID().replaceAll('-', '')}`,
      type: intent.type,
      deviceId,
      sentAt: new Date().toISOString(),
      payload: intent.payload,
    });
    options.log('intent', { type: intent.type, delivered: delivery.delivered, authorized: delivery.authorized });
    if (!delivery.authorized) {
      const selected = (await options.controlPlane.listDevices(person.userId)).find((device) => device.id === deviceId && device.userId === person.userId);
      if (!selected || selected.revokedAt || !options.devices.isOnline(deviceId)) {
        return reply(response, 409, { error: 'device_offline' });
      }
      return reply(response, 403, { error: 'not_authorized' });
    }
    if (!delivery.delivered) return reply(response, 409, { error: 'device_offline' });
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
    if (!person) return reply(response, 401, { error: { code: 'unauthenticated' } });
    return reply(response, 200, { applicationContextId: person.applicationContextId });
  };
}

type Rebuilt =
  | { readonly type: 'session.start'; readonly sessionId: string; readonly payload: unknown }
  | { readonly type: 'session.input'; readonly payload: { sessionId: string; text: string } }
  | { readonly type: Exclude<IntentType, 'session.start' | 'session.input'>; readonly payload: unknown };

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

function payloadRecord(body: unknown): Record<string, unknown> | undefined {
  return isRecord(body) && isRecord(body.payload) ? body.payload : undefined;
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
    if (size > MAX_BODY_BYTES) throw new BodyTooLarge();
    chunks.push(Buffer.from(chunk));
  }
  const parsed: unknown = JSON.parse(Buffer.concat(chunks).toString('utf8'));
  return parsed;
}

class BodyTooLarge extends Error {}

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
