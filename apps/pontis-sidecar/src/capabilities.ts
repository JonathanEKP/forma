/**
 * The capabilities this sidecar registers: the operations Boundary F serves,
 * each built from its generated catalog entry (plan §10(b) D5).
 *
 * Everything a provider sees — id, kind, description, input schema, risk,
 * bounds, presentation — comes from `bindings/generated/capabilityCatalog.ts`,
 * which `crates/agent-catalog` writes and `scripts/check-bindings.sh` keeps
 * honest. What is written here is what D5 leaves to the sidecar: the output
 * schema and the `authorize`, `requiresApproval` and `execute` bindings.
 *
 * Registered, and why only these:
 *
 * - The operations Boundary F serves today. A catalog entry with no route
 *   behind it would be a tool that can only fail.
 * - `forma.guide`, the manual (plan §6.1). The guidance is only the opening
 *   context; this is where the agent reads the rest, as an ordinary `read`
 *   resource. Nothing gates a write on having called it.
 * - The four `ui.*` actions, which live in [ui.ts](ui.ts) because they are a
 *   different kind of thing: no `execute` binding, a fixed output schema, and a
 *   target that is the person's own open editor rather than Forma. They were
 *   held back until their handlers existed — `ui_unavailable` means "no window
 *   of yours is connected right now", a state the person can fix by opening
 *   Forma, and with no handler written anywhere it would have been permanent and
 *   every call would have cost the agent a turn to learn it.
 */
import { CapabilityError, defineCapability, type ApprovalEvidence, type ApprovalRequirement, type Capability, type JsonSchema } from '@pontis/core';
import type { ApprovalEvidence as FormaApprovalEvidence } from '@forma/bindings/ApprovalEvidence';
import type { CapabilitySpec } from '@forma/bindings/CapabilitySpec';
import type { DesignOperation } from '@forma/bindings/DesignOperation';
import type { DesignRecord } from '@forma/bindings/DesignRecord';
import type { JsonValue } from '@forma/bindings/serde_json/JsonValue';

import { capabilityCatalog } from '../../../bindings/generated/capabilityCatalog.ts';
import type { FormaBoundary, Represented } from './boundary.ts';

/** The capabilities Boundary F serves (`SERVED` in `apps/server/src/routes/pontis.rs`). */
export const SERVED = [
  'project.list',
  'design.list',
  'design.get',
  'design.validate',
  'design.applyOperations',
  'forma.guide',
  'brand.get',
  'assetRequest.create',
] as const;

export type ServedCapability = (typeof SERVED)[number];

/** A represented user, once the gateway has resolved one: a Forma subject. */
export interface FormaUser {
  readonly id: string;
}

/**
 * Whether a removal is in the batch, by operation — exhaustive over the
 * generated `DesignOperation`, the TypeScript half of `required_risk` in
 * `routes/pontis.rs`. A new operation fails the typecheck here until someone
 * decides whether it destroys work, rather than inheriting an answer.
 *
 * This is the first of two boundaries, not the only one (SEC-INV-012): Forma
 * refuses an unapproved removal itself whatever the sidecar decided, so a
 * mistake here can only cost an extra prompt or an `approval_required`, never a
 * removal nobody agreed to.
 */
const DESTROYS = {
  add_page: false,
  remove_page: true,
  reorder_pages: false,
  set_background: false,
  add_layer: false,
  remove_layer: true,
  move_layer: false,
  reorder_layer: false,
  set_text: false,
  patch_text_style: false,
  set_image: false,
  patch_shape_style: false,
  set_format: false,
} as const satisfies Record<DesignOperation['op'], boolean>;

/** How many operations in a batch remove a page or a layer. */
export function removals(operations: readonly unknown[]): number {
  return operations.filter((operation) => {
    const op = typeof operation === 'object' && operation !== null && 'op' in operation ? operation.op : undefined;
    return typeof op === 'string' && Object.hasOwn(DESTROYS, op) && DESTROYS[op as keyof typeof DESTROYS];
  }).length;
}

/**
 * The generated entry for one operation.
 *
 * Takes any catalog id rather than only a served one, because [ui.ts](ui.ts)
 * reads the same catalog for actions Boundary F deliberately does not serve. The
 * throw is the drift guard: an id that has left the catalog fails at startup
 * rather than registering a capability with no definition behind it.
 */
export function specOf(id: string): CapabilitySpec {
  const found = capabilityCatalog.find((entry) => entry.id === id);
  if (!found) throw new Error(`the agent catalog has no ${id}`);
  return found;
}

function spec(id: ServedCapability): CapabilitySpec {
  return specOf(id);
}

function schema(value: JsonValue): JsonSchema {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('a catalog input schema is a JSON object');
  }
  return value;
}

/** The generated half of a capability, in the shape `defineCapability` takes. */
function declared(id: ServedCapability) {
  const entry = spec(id);
  return {
    id: entry.id,
    kind: entry.kind,
    description: entry.description,
    input: schema(entry.inputSchema),
    risk: entry.risk,
    bounds: entry.bounds,
    ...(entry.presentation ? { presentation: entry.presentation } : {}),
  };
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/iu;

export function isUuid(value: string): boolean {
  return UUID.test(value);
}

/**
 * The refusal before any domain work: the session's context must be what
 * `/service/resolve` produces, an `AccountId`. Not the authorization — Forma
 * revalidates the user's membership in that account on every call, and that is
 * the decision that counts (SEC-INV-004).
 */
function authorize({ user, applicationContextId }: { user: FormaUser; applicationContextId: string }): boolean {
  return isUuid(user.id) && isUuid(applicationContextId);
}

function representing(user: FormaUser, applicationContextId: string): Represented {
  return { userId: user.id, applicationContextId };
}

/** Pontis's approval evidence in the shape Boundary F reads. */
export function formaApproval(approval: ApprovalEvidence): FormaApprovalEvidence {
  return {
    approval_id: approval.approvalId,
    session_id: approval.sessionId,
    capability_id: approval.capabilityId,
    scope:
      approval.scope.kind === 'once'
        ? { kind: 'once', invocation_id: approval.scope.invocationId }
        : { kind: 'session', capability_ids: [...approval.scope.capabilityIds] },
    risk: approval.risk,
    granted_at: approval.grantedAt,
    expires_at: approval.expiresAt,
  };
}

const OBJECT = { type: 'object' } as const;

interface DesignInput {
  readonly design_id: string;
}

interface ProjectInput {
  readonly project_id: string;
}

interface AssetRequestInput {
  readonly project_id: string;
  readonly design_id?: string | null;
  readonly purpose: string;
  readonly description: string;
  readonly suggested_prompt?: string | null;
  readonly requested_width?: number | null;
  readonly requested_height?: number | null;
}

interface ApplyInput {
  readonly design_id: string;
  readonly base_version: number;
  readonly operations: readonly Record<string, unknown>[];
}

const APPROVAL_NAME_READ_MS = 3_000;
const MAX_APPROVAL_NAME_CHARS = 80;
const NAME_READ_FALLBACK = "Forma couldn't load the layer's name. Details shows its id.";
const PAGE_NAME_FALLBACK = "Forma couldn't load the page's name. Details shows its id.";
const MULTIPLE_NAMES_FALLBACK = "Forma couldn't load the names. Details shows the ids.";

interface RemovedPage {
  readonly id: string;
  readonly name: string | undefined;
  readonly index: number;
  readonly layerCount: number;
}

interface RemovedLayer {
  readonly name: string | undefined;
  readonly pageId: string;
}

interface RemovalCopy {
  readonly summary: string;
  readonly reason: string;
  readonly affectedResourceCount: number;
}

function operationIds(operations: readonly Record<string, unknown>[], op: string, key: string): string[] {
  return [...new Set(operations.flatMap((operation) => {
    if (operation.op !== op) return [];
    const value = operation[key];
    return typeof value === 'string' ? [value.toLowerCase()] : [];
  }))];
}

function safeName(name: unknown): string | undefined {
  if (typeof name !== 'string') return undefined;
  const flattened = [...name]
    .map((character) => (/\p{Cc}|\p{Cf}|\u2028|\u2029/u.test(character) ? ' ' : character))
    .join('')
    .replace(/\s+/gu, ' ')
    .trim();
  if (!flattened) return undefined;
  const capped = [...flattened].slice(0, MAX_APPROVAL_NAME_CHARS).join('');
  return capped
    .replaceAll('\\', '\\\\')
    .replaceAll('“', '\\“')
    .replaceAll('”', '\\”')
    .replaceAll('"', '\\"');
}

function quotedApprovalName(name: string | undefined): string | undefined {
  return name === undefined ? undefined : `“${name}”`;
}

function readableDesign(record: DesignRecord | undefined, designId: string): DesignRecord | undefined {
  return record && record.id.toLowerCase() === designId.toLowerCase() ? record : undefined;
}

function operationFallbackCopy(
  pageIds: readonly string[],
  layerIds: readonly string[],
  operations: readonly Record<string, unknown>[],
  designLabel: string,
): RemovalCopy {
  const requestedItems = [
    pageIds.length > 0 ? `${pageIds.length} ${pageIds.length === 1 ? 'page' : 'pages'}` : undefined,
    layerIds.length > 0 ? `${layerIds.length} ${layerIds.length === 1 ? 'layer' : 'layers'}` : undefined,
  ].filter((part): part is string => part !== undefined);
  const reason = requestedItems.length === 1 && pageIds.length === 1
    ? PAGE_NAME_FALLBACK
    : requestedItems.length === 1 && layerIds.length === 1
      ? NAME_READ_FALLBACK
      : MULTIPLE_NAMES_FALLBACK;
  return {
    summary: `Delete ${requestedItems.join(' and ') || 'the requested items'} from ${designLabel}`,
    reason,
    affectedResourceCount: removals(operations),
  };
}

function removalCopy(
  design: DesignRecord | undefined,
  operations: readonly Record<string, unknown>[],
): RemovalCopy {
  const pageIds = operationIds(operations, 'remove_page', 'page_id');
  const layerIds = operationIds(operations, 'remove_layer', 'layer_id');
  const designName = safeName(design?.name);
  const designLabel = quotedApprovalName(designName) ?? 'this design';
  if (!design) return operationFallbackCopy(pageIds, layerIds, operations, designLabel);

  const pages = design.document.pages;
  const removedPages: RemovedPage[] = pageIds.flatMap((id) => {
    const index = pages.findIndex((page) => page.id.toLowerCase() === id);
    const page = pages[index];
    return page && index >= 0
      ? [{ id, name: safeName(page.name), index, layerCount: page.layers.length }]
      : [];
  });
  const removedPageIds = new Set(removedPages.map(({ id }) => id));
  const affectedLayers = new Map<string, RemovedLayer>();

  for (const page of removedPages) {
    const source = pages[page.index];
    if (!source) continue;
    for (const layer of source.layers) {
      const id = layer.id.toLowerCase();
      affectedLayers.set(id, {
        name: safeName(layer.name),
        pageId: page.id,
      });
    }
  }

  for (const id of layerIds) {
    const located = pages.flatMap((page, pageIndex) => {
      const layer = page.layers.find((candidate) => candidate.id.toLowerCase() === id);
      return layer ? [{ page, pageIndex, layer }] : [];
    })[0];
    if (!located || removedPageIds.has(located.page.id.toLowerCase())) continue;
    affectedLayers.set(id, {
      name: safeName(located.layer.name),
      pageId: located.page.id.toLowerCase(),
    });
  }

  if (
    pageIds.some((id) => !removedPages.some((page) => page.id === id)) ||
    layerIds.some((id) => !affectedLayers.has(id))
  ) {
    return operationFallbackCopy(pageIds, layerIds, operations, designLabel);
  }

  const layerList = [...affectedLayers.values()];
  const affectedResourceCount = removedPages.length + layerList.length;

  if (removedPages.length === 0 && layerIds.length === 1) {
    const layer = layerList[0];
    if (!layer?.name) {
      return {
        summary: `Delete 1 layer from ${designLabel}`,
        reason: NAME_READ_FALLBACK,
        affectedResourceCount,
      };
    }
    const layerLabel = quotedApprovalName(layer.name);
    const summary = `Delete the layer ${layerLabel} from ${designLabel}`;
    return {
      summary,
      reason: `This will permanently delete the layer ${layerLabel} from ${designLabel}.`,
      affectedResourceCount,
    };
  }

  if (removedPages.length === 1 && layerIds.length === 0) {
    const page = removedPages[0];
    if (page) {
      const pageName = quotedApprovalName(page.name);
      const pageLabel = `page ${page.index + 1}${pageName ? ` (${pageName})` : ''}`;
      const layers = page.layerCount;
      const layerNoun = layers === 1 ? 'layer' : 'layers';
      const summary = `Delete ${pageLabel} of ${designLabel}${layers > 0 ? ` and the ${layers} ${layerNoun} on it` : ''}`;
      return {
        summary,
        reason: `This will permanently delete ${pageLabel}${layers > 0 ? ` and its ${layers} ${layerNoun}` : ''} from ${designLabel}.`,
        affectedResourceCount,
      };
    }
  }

  if (removedPages.length === 0 && layerList.length > 1) {
    const pageCount = new Set(layerList.map(({ pageId }) => pageId)).size;
    const summary = pageCount > 1
      ? `Delete ${layerList.length} layers across ${pageCount} pages of ${designLabel}`
      : `Delete ${layerList.length} layers from ${designLabel}`;
    const pagesLabel = `${pageCount} ${pageCount === 1 ? 'page' : 'pages'}`;
    return {
      summary,
      reason: `This will permanently delete ${layerList.length} layers across ${pagesLabel} of ${designLabel}.`,
      affectedResourceCount,
    };
  }

  const pageParts = removedPages.map(({ index, name, layerCount }) => {
    const pageName = quotedApprovalName(name);
    const label = `page ${index + 1}${pageName ? ` (${pageName})` : ''}`;
    return `${label}${layerCount > 0 ? ` and its ${layerCount} layers` : ''}`;
  });
  const looseLayers = layerList.filter(({ pageId }) => !removedPageIds.has(pageId));
  const looseNames = looseLayers.flatMap(({ name }) => {
    const layerName = quotedApprovalName(name);
    return layerName ? [`the layer ${layerName}`] : [];
  });
  const removedPart = [
    ...pageParts,
    ...(looseNames.length > 0 ? [looseNames.join(' and ')] : looseLayers.length > 0 ? [`${looseLayers.length} layers`] : []),
  ].join(' and ');
  const summary = `Delete ${removedPart || `${pageIds.length + layerIds.length} items`} from ${designLabel}`;
  const fallbackReason = layerList.some(({ name }) => !name)
    ? MULTIPLE_NAMES_FALLBACK
    : `This will permanently delete ${removedPart || `${pageIds.length + layerIds.length} items`} from ${designLabel}.`;
  return { summary, reason: fallbackReason, affectedResourceCount };
}

export function formaCapabilities(boundary: FormaBoundary): Capability[] {
  const projectList = defineCapability<Record<string, never>, unknown, FormaUser>({
    ...declared('project.list'),
    // A provider result is an object (MCP structured content), so the list
    // travels under a key rather than as a bare array.
    output: {
      type: 'object',
      properties: { projects: { type: 'array', items: OBJECT } },
      required: ['projects'],
      additionalProperties: false,
    },
    authorize,
    execute: async ({ user, applicationContextId, signal }) => ({
      projects: await boundary.listProjects(
        representing(user, applicationContextId),
        spec('project.list').bounds.maxResultBytes,
        signal,
      ),
    }),
  });

  const designList = defineCapability<ProjectInput, unknown, FormaUser>({
    ...declared('design.list'),
    output: {
      type: 'object',
      properties: { designs: { type: 'array', items: OBJECT } },
      required: ['designs'],
      additionalProperties: false,
    },
    authorize,
    execute: async ({ user, applicationContextId, input, signal }) => ({
      designs: await boundary.listDesigns(
        representing(user, applicationContextId),
        input.project_id,
        spec('design.list').bounds.maxResultBytes,
        signal,
      ),
    }),
  });

  const designGet = defineCapability<DesignInput, unknown, FormaUser>({
    ...declared('design.get'),
    output: {
      type: 'object',
      properties: {
        id: { type: 'string' },
        project_id: { type: 'string' },
        version: { type: 'integer', minimum: 0 },
        document: OBJECT,
      },
      required: ['id', 'project_id', 'version', 'document'],
    },
    authorize,
    execute: async ({ user, applicationContextId, input, signal }) =>
      await boundary.getDesign(
        representing(user, applicationContextId),
        input.design_id,
        spec('design.get').bounds.maxResultBytes,
        signal,
      ),
  });

  const designValidate = defineCapability<DesignInput, unknown, FormaUser>({
    ...declared('design.validate'),
    output: {
      type: 'object',
      properties: { valid: { type: 'boolean' }, issues: { type: 'array', items: OBJECT } },
      required: ['valid', 'issues'],
    },
    authorize,
    execute: async ({ user, applicationContextId, input, signal }) => {
      const bounds = spec('design.validate').bounds;
      return await boundary.validate(
        representing(user, applicationContextId),
        input.design_id,
        bounds.timeoutMs,
        bounds.maxResultBytes,
        signal,
      );
    },
  });

  const applyOperations = defineCapability<ApplyInput, unknown, FormaUser>({
    ...declared('design.applyOperations'),
    output: {
      type: 'object',
      properties: {
        design_id: { type: 'string' },
        project_id: { type: 'string' },
        version: { type: 'integer', minimum: 0 },
        updated_at: { type: 'string' },
      },
      required: ['design_id', 'project_id', 'version', 'updated_at'],
      additionalProperties: false,
    },
    authorize,
    // Plan §3.1: the declared class is `write`; a batch that removes work is
    // elevated to `destructive`, and only ever upward.
    requiresApproval: async ({ input, user, applicationContextId }): Promise<ApprovalRequirement | undefined> => {
      if (removals(input.operations) === 0) return undefined;
      let design: DesignRecord | undefined;
      try {
        design = await boundary.getDesign(
          representing(user, applicationContextId),
          input.design_id,
          spec('design.get').bounds.maxResultBytes,
          AbortSignal.timeout(APPROVAL_NAME_READ_MS),
        );
      } catch {
        // The names improve an approval, but a failed read never prevents the
        // person from receiving the approval request.
      }
      const copy = removalCopy(readableDesign(design, input.design_id), input.operations);
      return {
        risk: 'destructive',
        reason: copy.reason,
        summary: copy.summary,
        affectedResourceCount: copy.affectedResourceCount,
      };
    },
    execute: async ({ user, applicationContextId, input, invocationId, approval, signal }) => {
      // The gateway has already refused a removal that carries no consent; an
      // approval here is forwarded, never synthesised, and Forma checks it is
      // for this invocation at this class.
      if (removals(input.operations) > 0 && !approval) {
        throw new CapabilityError('approval_required', 'this batch removes work and carries no approval');
      }
      return await boundary.applyOperations(
        representing(user, applicationContextId),
        input.design_id,
        {
          base_version: input.base_version,
          // Forma parses each operation against its own enum and refuses one it
          // does not know as INVALID_BODY; nothing is interpreted here.
          operations: input.operations as unknown as DesignOperation[],
          approval: approval ? formaApproval(approval) : null,
        },
        invocationId,
        spec('design.applyOperations').bounds.maxResultBytes,
        signal,
      );
    },
  });

  const guide = defineCapability<Record<string, never>, unknown, FormaUser>({
    ...declared('forma.guide'),
    // Exactly what Forma answers: no timestamp and no capability list
    // (`AgentManual`), so two reads are identical and the session catalog stays
    // the only answer to what can be called.
    output: {
      type: 'object',
      properties: { revision: { type: 'string' }, text: { type: 'string' } },
      required: ['revision', 'text'],
      additionalProperties: false,
    },
    authorize,
    execute: async ({ user, applicationContextId, signal }) =>
      await boundary.guide(representing(user, applicationContextId), spec('forma.guide').bounds.maxResultBytes, signal),
  });

  const brandGet = defineCapability<ProjectInput, unknown, FormaUser>({
    ...declared('brand.get'),
    // The kit and the font catalog together, as forma-mcp answers them: the
    // guidance's "only families from the catalog" needs the catalog here.
    output: {
      type: 'object',
      properties: {
        brand: { type: ['object', 'null'] },
        fonts: { type: 'array', items: { type: 'string' } },
      },
      required: ['brand', 'fonts'],
      additionalProperties: false,
    },
    authorize,
    execute: async ({ user, applicationContextId, input, signal }) =>
      await boundary.getBrand(
        representing(user, applicationContextId),
        input.project_id,
        spec('brand.get').bounds.maxResultBytes,
        signal,
      ),
  });

  const assetRequestCreate = defineCapability<AssetRequestInput, unknown, FormaUser>({
    ...declared('assetRequest.create'),
    output: {
      type: 'object',
      properties: {
        id: { type: 'string' },
        project_id: { type: 'string' },
        status: { type: 'string' },
      },
      required: ['id', 'project_id', 'status'],
    },
    authorize,
    execute: async ({ user, applicationContextId, input, invocationId, signal }) =>
      await boundary.createAssetRequest(
        representing(user, applicationContextId),
        input.project_id,
        {
          design_id: input.design_id ?? null,
          purpose: input.purpose,
          description: input.description,
          suggested_prompt: input.suggested_prompt ?? null,
          requested_width: input.requested_width ?? null,
          requested_height: input.requested_height ?? null,
        },
        invocationId,
        spec('assetRequest.create').bounds.maxResultBytes,
        signal,
      ),
  });

  return [projectList, designList, designGet, designValidate, applyOperations, guide, brandGet, assetRequestCreate];
}
