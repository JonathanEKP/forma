/**
 * The four editor actions (plan §3.2, §11 step 9).
 *
 * A `ui` capability is routed to a connected `UIClient` and never executed here
 * — `defineUiCapability` has nowhere to put domain work, deliberately. So what
 * this module owns is not an implementation but the two things Pontis leaves to
 * the application: which actions exist, and who may aim one.
 *
 * # Why `authorize` reaches Forma for two of them
 *
 * Navigating is not a write, but it still names a resource. `pontis/docs/13` §
 * UI actions and `pontis/packages/capability-core/src/ui.ts` both make the point
 * with the case that matters: opening a design the represented user may not read
 * would disclose that it exists. The agent learns design ids from
 * `design.list`, which is already scoped to the account — but it does not have
 * to, and an id is guessable in principle, so the check runs on the id in hand
 * rather than on where it came from.
 *
 * The check is a Boundary F read, which means Forma answers it: the same
 * membership revalidation every other call gets, and `not_found` and
 * `authorization_denied` collapse to the same `false` here because telling them
 * apart is the disclosure.
 *
 * `ui.setPanel` and `ui.setZoom` name no resource at all. They get the context
 * check every capability gets and nothing more — there is no third thing to ask.
 */
import { CapabilityError, defineUiCapability, type Capability, type JsonSchema } from '@pontis/core';

import type { FormaBoundary, Represented } from './boundary.ts';
import { isUuid, specOf, type FormaUser } from './capabilities.ts';

/** The editor actions, in the order the catalog declares them. */
export const UI_ACTIONS = ['ui.openDesign', 'ui.selectLayer', 'ui.setPanel', 'ui.setZoom'] as const;

export type UiAction = (typeof UI_ACTIONS)[number];

interface DesignTarget {
  readonly design_id: string;
}

/** The generated half, in the shape `defineUiCapability` takes. */
function declaredUi(id: UiAction): { id: string; description: string; input: JsonSchema; bounds: { maxResultBytes: number; timeoutMs: number } } {
  const entry = specOf(id);
  if (entry.kind !== 'ui') throw new Error(`${id} is not a ui capability in the agent catalog`);
  const input = entry.inputSchema;
  if (typeof input !== 'object' || input === null || Array.isArray(input)) {
    throw new Error(`the catalog input schema for ${id} is not a JSON object`);
  }
  return { id: entry.id, description: entry.description, input, bounds: entry.bounds };
}

/**
 * The context check every capability gets: the session's context is an
 * `AccountId` and the represented user is a Forma subject. Not the
 * authorization — Forma re-decides membership on every Boundary F call.
 */
function addressable({ user, applicationContextId }: { user: FormaUser; applicationContextId: string }): boolean {
  return isUuid(user.id) && isUuid(applicationContextId);
}

function representing(user: FormaUser, applicationContextId: string): Represented {
  return { userId: user.id, applicationContextId };
}

/**
 * Whether the represented user may read this design.
 *
 * A refusal from Forma is `false`, not a throw: `authorize` answers a question
 * and the gateway turns a `false` into `authorization_denied` on its own.
 * Anything that is not a refusal — the server being down, a bounded read
 * overrun — is not an answer, and is rethrown so it reports as itself rather
 * than as a permission the person does not have.
 */
async function mayRead(
  boundary: FormaBoundary,
  user: FormaUser,
  applicationContextId: string,
  designId: string,
  signal: AbortSignal,
): Promise<boolean> {
  if (!addressable({ user, applicationContextId })) return false;
  // A string that is not an id names no resource, so there is nothing to disclose
  // by saying so — and "you may not" for a typo sends an agent away rather than
  // to fix the id (F-40-5). Same wording as `design.get`.
  if (typeof designId !== 'string' || !isUuid(designId)) {
    throw new CapabilityError('validation_error', `design id ${JSON.stringify(designId)} is not a UUID`);
  }
  try {
    await boundary.getDesign(
      representing(user, applicationContextId),
      designId,
      specOf('design.get').bounds.maxResultBytes,
      signal,
    );
    return true;
  } catch (cause) {
    if (cause instanceof CapabilityError && (cause.code === 'authorization_denied' || cause.code === 'not_found')) {
      return false;
    }
    throw cause;
  }
}

export function uiCapabilities(boundary: FormaBoundary): Capability[] {
  const openDesign = defineUiCapability<DesignTarget, FormaUser>({
    ...declaredUi('ui.openDesign'),
    authorize: ({ user, applicationContextId, input, signal }) =>
      mayRead(boundary, user, applicationContextId, input.design_id, signal),
  });

  // The same check as `ui.openDesign`, on the same field. The layer id is not
  // checked here: whether that layer is in the design the person has open is
  // something the editor knows and the server would have to guess at, and the
  // editor answers it by rejecting the action rather than by handling it.
  const selectLayer = defineUiCapability<DesignTarget, FormaUser>({
    ...declaredUi('ui.selectLayer'),
    authorize: ({ user, applicationContextId, input, signal }) =>
      mayRead(boundary, user, applicationContextId, input.design_id, signal),
  });

  const setPanel = defineUiCapability<unknown, FormaUser>({
    ...declaredUi('ui.setPanel'),
    authorize: addressable,
  });

  const setZoom = defineUiCapability<unknown, FormaUser>({
    ...declaredUi('ui.setZoom'),
    authorize: addressable,
  });

  return [openDesign, selectLayer, setPanel, setZoom];
}
