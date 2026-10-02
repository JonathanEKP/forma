/**
 * Who is behind a browser request: `getAuthenticatedUser` and
 * `getAuthorizedApplicationContext` of `pontis/docs/12-sdk.md`, answered by
 * forma-server in one call (plan §10(b) D2).
 *
 * The sidecar verifies nothing itself. The request's bearer is a Forma session
 * token; forma-server checks it with the same `Verifier` every other route uses
 * and says whose it is and which account they may enter. That account is the
 * session's `ApplicationContextId` (§5).
 */
import type { IncomingMessage } from 'node:http';

import { bearerToken } from '@pontis/control-plane';

import type { FormaBoundary } from './boundary.ts';

export interface Person {
  readonly userId: string;
  readonly applicationContextId: string;
}

export type ResolvePerson = (request: IncomingMessage) => Promise<Person | undefined>;

/**
 * Resolves each request at most once. The Control Plane's routes ask for the
 * user and then for the context as two hooks; both read the same answer.
 *
 * Nothing is cached across requests. A session's context is bound once, at
 * `session.start`, and kept by the Control Plane for the session's life (ADR-024
 * makes it immutable); what Forma must re-decide every time — whether the person
 * still holds that membership — it re-decides at Boundary F on every call.
 */
export function personResolver(boundary: FormaBoundary): ResolvePerson {
  const answered = new WeakMap<IncomingMessage, Promise<Person | undefined>>();
  return (request) => {
    let answer = answered.get(request);
    if (!answer) {
      answer = resolve(boundary, request);
      answered.set(request, answer);
    }
    return answer;
  };
}

async function resolve(boundary: FormaBoundary, request: IncomingMessage): Promise<Person | undefined> {
  const token = bearerToken(request);
  if (!token) return undefined;
  const resolution = await boundary.resolveSubject(token);
  return resolution ? { userId: resolution.subject, applicationContextId: resolution.account_id } : undefined;
}
