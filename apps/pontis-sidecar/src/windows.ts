/**
 * Which window is asking, and which agent session its chat belongs to (plan §11,
 * fix batch 1, task B).
 *
 * Pontis keys a UI client by `(userId, applicationContextId, appSessionId)` and a
 * second connection with the same triple *replaces* the first. Forma used to set
 * `appSessionId` to `userId:applicationContextId`, which every window of one
 * person shares, so two tabs closed each other's stream forever. The window now
 * names itself — one id per tab, which a reload keeps — and that is what goes in
 * the app session: a reload replaces its own registration and a second tab adds
 * one.
 *
 * Neither header is trusted. The window id only has to be the fixed format, since
 * the person is resolved from the bearer and the id is merely *which of their
 * tabs*. The agent session id is a claim about who owns a session, so it is
 * checked against the Control Plane's own record before it binds anything.
 */
import type { IncomingMessage } from 'node:http';

import type { ControlPlane } from '@pontis/control-plane';

import { isUuid } from './capabilities.ts';
import type { Person } from './person.ts';

export const WINDOW_HEADER = 'x-forma-window';
export const AGENT_SESSION_HEADER = 'x-forma-agent-session';

/** The shape `session.start` mints in `intents.ts`: `ses_` and 32 hex digits. */
const AGENT_SESSION = /^ses_[0-9a-f]{32}$/u;

function single(request: IncomingMessage, name: string): string | undefined {
  const value = request.headers[name];
  // A repeated header arrives as an array. Which of two values the sender meant
  // is not a question to answer by taking the first.
  return typeof value === 'string' ? value : undefined;
}

/** The window's id, or `undefined` when the header is missing or not a UUID. */
export function windowId(request: IncomingMessage): string | undefined {
  const value = single(request, WINDOW_HEADER);
  return value !== undefined && isUuid(value) ? value.toLowerCase() : undefined;
}

/** What the window claims, if it is the shape of a session id at all. */
export function claimedAgentSession(request: IncomingMessage): string | undefined {
  const value = single(request, AGENT_SESSION_HEADER);
  return value !== undefined && AGENT_SESSION.test(value) ? value : undefined;
}

/**
 * The claimed session, if and only if it is this person's, in this person's
 * account.
 *
 * Asked of the Control Plane's `authoritativeSessionContext`, which answers from
 * the binding made when the session was started and, after a restart, recovers it
 * from the durable event stream — so nothing here remembers anything, and a
 * sidecar that comes back with a durable store verifies a reloaded window's
 * session the same way. It is asked once per Device the person has, because the
 * binding is by Device. Any failure — a conflicting record, a store that cannot
 * be read — is "not verified", which binds nothing.
 */
export async function verifiedAgentSession(
  controlPlane: ControlPlane,
  person: Person,
  sessionId: string,
): Promise<string | undefined> {
  try {
    for (const device of await controlPlane.listDevices(person.userId)) {
      if (device.revokedAt) continue;
      const bound = await controlPlane.authoritativeSessionContext({
        sessionId,
        userId: person.userId,
        deviceId: device.id,
      });
      if (bound && bound.userId === person.userId && bound.applicationContextId === person.applicationContextId) {
        return sessionId;
      }
    }
  } catch {
    return undefined;
  }
  return undefined;
}
