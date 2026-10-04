/**
 * What the person's open windows hear while the agent works, and whether their
 * device is there (plan §11 step 10c).
 *
 * Both are wiring Pontis's reference app already shows
 * (`examples/reference-app/src/bin/remote-server.ts`), left out of the first
 * composition: the Realtime Gateway hands every Device notification to the host,
 * and delivering it to a browser is the host's to do. Without it the editor can
 * only read history, and the agent's reply cannot appear until it is finished.
 *
 * Delivery here is an optimization and never the record (SEC-INV-009, P7): a
 * frame with a `sequence` is a hint that the page has something new, and the page
 * is read from `GET /pons/v1/events` regardless. So a fan-out that fails or
 * misses a window costs latency, not history.
 */
import type { IncomingMessage, ServerResponse } from 'node:http';

import type { ControlPlane, DeviceGateway, DurableEvent } from '@pontis/control-plane';
import { publicDeviceView } from '@pontis/control-plane';
import { PONS_PROTOCOL_VERSION, isDurableNotification } from '@pontis/core';
import type { UIClientRegistry } from '@pontis/server';

import type { Log } from './log.ts';
import type { ResolvePerson } from './person.ts';

export const DEVICES_PATH = '/pons/v1/devices';

/**
 * Which of the windows' registry the fan-out needs: who is connected, and a way
 * to address one context or one person. Named so a test can hand it a stand-in.
 */
type Windows = Pick<UIClientRegistry, 'all' | 'notifyContext' | 'notifyPrincipal'>;

/**
 * Said once, and only what failed and for what kind of frame.
 *
 * Never the payload — it carries what the person typed and what the agent
 * answered — and never the error's own text, which a library is free to fill
 * with whatever it was holding. The error's name is enough to find the cause.
 */
function dropped(log: Log, source: string, type: string, cause: unknown): void {
  log('live_delivery_dropped', { source, type, error: cause instanceof Error ? cause.name : 'unknown' });
}

/**
 * What a window is told about a durable event.
 *
 * Only what a reader of the page would find there: the position, the scope, the
 * type, the session, the Device and the payload. The store's own id, stream key and origin
 * message id are bookkeeping, and nothing in a browser needs them.
 */
function durableFrame(event: DurableEvent) {
  return {
    protocol: PONS_PROTOCOL_VERSION,
    sequence: event.sequence,
    scope: event.scope.kind === 'principal' ? { kind: 'principal' } : { kind: 'application_context', applicationContextId: event.scope.applicationContextId },
    type: event.type,
    ...(event.sessionId ? { sessionId: event.sessionId } : {}),
    // Which Device's Runtime wrote it: the chat tells a session whose Runtime has been
    // replaced from one that is still running by comparing it with the Device it sees.
    ...(event.deviceId ? { deviceId: event.deviceId } : {}),
    payload: event.payload,
    sentAt: event.createdAt,
  };
}

/**
 * Deliver durable events as they are stored, to the windows of the context they
 * belong to.
 *
 * Read from the store rather than from the Device's own notification: by the
 * time a listener runs the event has its `sequence`, and a runtime redelivering
 * its outbox after a reconnect allocates nothing and so is never fanned out
 * twice. A principal-scoped event goes to every window of that person, because
 * a lost context membership must not hide `device.revoked`.
 *
 * Delivery is an optimization, so a failure costs the frame and nothing else.
 * The in-memory store already swallows a throwing listener; this does not lean
 * on that, because a store that did not would let a bad frame fail an append
 * that has already succeeded.
 *
 * Returns the unsubscribe, for `close`.
 */
export function fanOutDurable(controlPlane: ControlPlane, windows: Windows, log: Log): () => void {
  return controlPlane.events.subscribeAll((event) => {
    try {
      const envelope = durableFrame(event);
      if (event.scope.kind === 'principal') {
        windows.notifyPrincipal(event.scope.userId, envelope);
        return;
      }
      const target = windows
        .all()
        .find((candidate) => candidate.userId === event.scope.userId && controlPlane.streamKeyFor(candidate.userId, candidate.applicationContextId) === event.streamKey);
      if (target) windows.notifyContext(target.userId, target.applicationContextId, envelope);
    } catch (cause) {
      dropped(log, 'durable', event.type, cause);
    }
  });
}

/**
 * The Realtime Gateway's `onNotification`, for what is not stored: output
 * deltas, and the device presence announcements.
 *
 * A session-scoped notification goes only to the context that session was
 * started in, and only if that session belongs to this Device's owner and was
 * routed to this Device — the same three checks the reference app makes, so a
 * Device cannot speak into a session that is not its own. Anything without a
 * session goes to the owner's windows.
 *
 * The lookup is asynchronous and nothing awaits it, so a rejection — or a throw
 * from a window's write inside the callback — would be an unhandled rejection,
 * which ends the process on Node 15 and later and with it every person's chat
 * and UI channel. It is caught here instead: the frame is dropped and the page
 * still has it, or, for a delta, the completed text will.
 */
export function liveNotification(controlPlane: ControlPlane, windows: Windows, log: Log) {
  return (
    deviceId: string,
    envelope: { readonly protocol: string; readonly type: string; readonly sessionId?: string; readonly sentAt: string; readonly payload: unknown },
  ): void => {
    // Durable ones are delivered, stamped, from the store.
    if (isDurableNotification(envelope.type)) return;
    void controlPlane
      .getDevice(deviceId)
      .then((device) => {
        if (!device) return;
        const frame = {
          protocol: envelope.protocol,
          type: envelope.type,
          ...(envelope.sessionId ? { sessionId: envelope.sessionId } : {}),
          payload: envelope.payload,
          sentAt: envelope.sentAt,
        };
        if (envelope.sessionId) {
          const context = controlPlane.getSessionContext(envelope.sessionId);
          if (!context || context.userId !== device.userId || context.deviceId !== deviceId) return;
          windows.notifyContext(device.userId, context.applicationContextId, frame);
        } else {
          windows.notifyPrincipal(device.userId, frame);
        }
      })
      .catch((cause: unknown) => dropped(log, 'notification', envelope.type, cause));
  };
}

/**
 * `GET /pons/v1/devices`, with presence.
 *
 * Pontis answers this route itself and cannot say who is online: presence is
 * never durable (SEC-INV-009), so it is not on the stored record, and it is read
 * from the live sockets here at the moment of asking and merged in — never
 * written down. Registered ahead of the Control Plane's routes so this answer is
 * the one given; revoking and the rest of `/pons/v1/devices/*` are untouched.
 */
export function createDevicesRoute(options: {
  readonly controlPlane: ControlPlane;
  readonly devices: Pick<DeviceGateway, 'isOnline'>;
  readonly resolvePerson: ResolvePerson;
}): (request: IncomingMessage, response: ServerResponse) => Promise<boolean> {
  return async (request, response) => {
    if ((request.url ?? '').split('?')[0] !== DEVICES_PATH || request.method !== 'GET') return false;
    const person = await options.resolvePerson(request);
    if (!person) {
      // Same refusal the Control Plane's own route gives, so a client cannot tell which one answered.
      return reply(response, 403, { protocol: PONS_PROTOCOL_VERSION, error: { code: 'origin_not_allowed', category: 'transport', message: 'origin_not_allowed' } });
    }
    const records = await options.controlPlane.listDevices(person.userId);
    return reply(response, 200, {
      protocol: PONS_PROTOCOL_VERSION,
      devices: records.map((device) => ({ ...publicDeviceView(device), online: options.devices.isOnline(device.id) })),
    });
  };
}

function reply(response: ServerResponse, status: number, body: unknown): true {
  const bytes = Buffer.from(JSON.stringify(body));
  response.writeHead(status, { 'content-type': 'application/json', 'content-length': String(bytes.length), 'cache-control': 'no-store' });
  response.end(bytes);
  return true;
}
