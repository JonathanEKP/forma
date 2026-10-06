import type { DesignRecord, Layer, Page } from "../types";
import { layerId, layerName } from "../types";
import { isRecord } from "./events";

export interface ApprovalChip {
  readonly kind: "page" | "layer";
  readonly id: string;
  readonly label: string;
  readonly pageName: string;
  readonly showPageSuffix: boolean;
}

export interface ApprovalDetails {
  readonly pageCount: number;
  readonly layerCount: number;
  readonly layerCountUnavailable: boolean;
  readonly countLabel: string;
  readonly chips: readonly ApprovalChip[];
  readonly allChips: readonly ApprovalChip[];
  readonly moreCount: number;
  readonly rawIds: readonly string[];
  readonly nameNotice: string | null;
  readonly editSummary: string | null;
  readonly pageIds: readonly string[];
  readonly layerIdsByPage: Readonly<Record<string, readonly string[]>>;
}

export interface ApprovalOperationSource {
  readonly designId: string | null;
  readonly baseVersion: number | null;
  readonly operations: readonly unknown[];
  readonly reason: string | null;
}

export function approvalOperationSource(value: unknown): ApprovalOperationSource {
  if (!isRecord(value)) return { designId: null, baseVersion: null, operations: [], reason: null };
  const operations = Array.isArray(value.operations) ? value.operations : [];
  return {
    designId: typeof value.designId === "string" ? value.designId : null,
    baseVersion: typeof value.baseVersion === "number" ? value.baseVersion : null,
    operations,
    reason: typeof value.reason === "string" ? value.reason : null,
  };
}

const MAX_CHIPS = 5;

function operation(value: unknown): { readonly op: string; readonly fields: Readonly<Record<string, unknown>> } | null {
  if (!isRecord(value) || typeof value.op !== "string") return null;
  return { op: value.op, fields: value };
}

function fieldString(fields: Readonly<Record<string, unknown>>, key: string): string | null {
  const value = fields[key];
  return typeof value === "string" ? value : null;
}

function pageForLayer(record: DesignRecord, id: string): Page | null {
  return record.document.pages.find((candidate) => candidate.layers.some((layer) => layerId(layer) === id)) ?? null;
}

function layerFor(record: DesignRecord, id: string): Layer | null {
  for (const page of record.document.pages) {
    const layer = page.layers.find((candidate) => layerId(candidate) === id);
    if (layer) return layer;
  }
  return null;
}

function countLabel(pageCount: number, layerCount: number, unavailable: boolean, keptPageCount: number): string {
  const pageText = pageCount > 0 ? `${pageCount} ${pageCount === 1 ? "page" : "pages"}` : "";
  const layerText = layerCount > 0 ? `${layerCount} ${layerCount === 1 ? "layer" : "layers"}` : "";

  if (pageText && layerText) return `${pageText} and ${layerText} will be deleted.`;
  if (pageText) {
    return unavailable
      ? `${pageText} will be deleted. Its layers couldn't be counted.`
      : `${pageText} will be deleted.`;
  }

  const deleted = layerText ? `${layerText} will be deleted.` : "The requested items will be deleted.";
  if (keptPageCount > 1) return `${deleted} ${keptPageCount === 2 ? "Both pages stay." : `${keptPageCount} pages stay.`}`;
  return deleted;
}

function missingNameMessage(pageCount: number, layerCount: number): string {
  if (pageCount === 1 && layerCount === 0) return "Forma couldn't load the page's name. Details shows its id.";
  if (pageCount === 0 && layerCount === 1) return "Forma couldn't load the layer's name. Details shows its id.";
  return "Forma couldn't load the names. Details shows the ids.";
}

function editDescription(op: string, fields: Readonly<Record<string, unknown>>, record: DesignRecord | null): string | null {
  const id = fieldString(fields, "layer_id");
  const target = id && record ? layerFor(record, id) : null;
  const name = target ? layerName(target) : id ? "this layer" : "this design";

  if (op === "set_text") {
    const content = fieldString(fields, "content");
    return content === null ? `Also update ${name}.` : `Also edit ${name} to “${content}”.`;
  }
  if (op === "move_layer") return `Also move ${name}.`;
  if (op === "reorder_layer") return `Also reorder ${name}.`;
  if (op === "patch_text_style") return `Also update the text style on ${name}.`;
  if (op === "patch_shape_style") return `Also update the shape style on ${name}.`;
  if (op === "set_image") return `Also update the image on ${name}.`;
  if (op === "set_background") return "Also update a page background.";
  if (op === "set_format") return "Also update the design format.";
  if (op === "add_layer") return "Also add a layer.";
  if (op === "add_page") return "Also add a page.";
  return null;
}

/**
 * Turns the operation list plus the person's REST read of the design into the
 * nouns and targets the approval card can show. Unknown names stay out of the
 * chips; the raw ids remain in Details.
 */
export function buildApprovalDetails(
  operations: readonly unknown[],
  record: DesignRecord | null,
  reason?: string | null,
): ApprovalDetails {
  const parsed = operations.map(operation).filter((entry): entry is NonNullable<typeof entry> => entry !== null);
  const removedPageIds = [...new Set(parsed.flatMap((entry) => entry.op === "remove_page" ? [fieldString(entry.fields, "page_id")] : []).filter((id): id is string => id !== null))];
  const removedLayerIds = [...new Set(parsed.flatMap((entry) => entry.op === "remove_layer" ? [fieldString(entry.fields, "layer_id")] : []).filter((id): id is string => id !== null))];
  const rawIds = [...new Set(parsed.flatMap((entry) => {
    if (entry.op === "remove_page") return [fieldString(entry.fields, "page_id")];
    if (entry.op === "remove_layer") return [fieldString(entry.fields, "layer_id")];
    return [];
  }).filter((id): id is string => id !== null))];

  const removedPages = removedPageIds.flatMap((id) => {
    const page = record?.document.pages.find((candidate) => candidate.id === id);
    return page ? [page] : [];
  });
  const deletedLayerIds = new Set(removedLayerIds);
  for (const page of removedPages) for (const layer of page.layers) deletedLayerIds.add(layerId(layer));

  const layerIdsByPage: Record<string, string[]> = {};
  const touchedPages = new Set<string>();
  for (const page of removedPages) {
    const ids = page.layers.slice().reverse().map(layerId);
    layerIdsByPage[page.id] = ids;
    touchedPages.add(page.id);
  }
  for (const id of removedLayerIds) {
    const page = record ? pageForLayer(record, id) : null;
    if (!page) continue;
    touchedPages.add(page.id);
    const ids = layerIdsByPage[page.id] ?? [];
    if (!ids.includes(id)) ids.push(id);
    layerIdsByPage[page.id] = ids;
  }

  const chipDrafts: Omit<ApprovalChip, "showPageSuffix">[] = [];
  const seenLayerChips = new Set<string>();
  for (const entry of parsed) {
    if (entry.op === "remove_page") {
      const pageId = fieldString(entry.fields, "page_id");
      const page = pageId ? removedPages.find((candidate) => candidate.id === pageId) : undefined;
      if (!page) continue;
      chipDrafts.push({ kind: "page", id: page.id, label: page.name, pageName: page.name });
      for (const layer of page.layers.slice().reverse()) {
        const id = layerId(layer);
        if (seenLayerChips.has(id)) continue;
        seenLayerChips.add(id);
        chipDrafts.push({ kind: "layer", id, label: layerName(layer), pageName: page.name });
      }
    } else if (entry.op === "remove_layer") {
      const id = fieldString(entry.fields, "layer_id");
      if (!id || seenLayerChips.has(id) || !record) continue;
      const page = pageForLayer(record, id);
      const layer = layerFor(record, id);
      if (!page || !layer) continue;
      seenLayerChips.add(id);
      chipDrafts.push({ kind: "layer", id, label: layerName(layer), pageName: page.name });
    }
  }
  const chips = chipDrafts.map((chip) => ({
    ...chip,
    showPageSuffix: chip.kind === "layer" && touchedPages.size > 1,
  }));

  const layerCount = deletedLayerIds.size;
  const knownRemovedPageIds = new Set(removedPages.map((page) => page.id));
  const layerCountUnavailable = removedPageIds.some((id) => !knownRemovedPageIds.has(id));
  const edits = parsed
    .filter((entry) => entry.op !== "remove_page" && entry.op !== "remove_layer")
    .map((entry) => editDescription(entry.op, entry.fields, record))
    .filter((description): description is string => description !== null);
  const textEdits = edits.filter((description) => description.startsWith("Also edit "));
  const otherEdits = edits.filter((description) => !description.startsWith("Also edit "));
  const combinedTextEdit = textEdits.length > 1
    ? `Also edit ${textEdits.map((description) => description.slice("Also edit ".length).replace(/\.$/u, "")).join(" and ")}.`
    : textEdits[0] ?? null;
  const nameNotice = reason?.trim() || (chips.length < (removedPageIds.length + deletedLayerIds.size) ? missingNameMessage(removedPageIds.length, deletedLayerIds.size) : null);

  return {
    pageCount: removedPageIds.length,
    layerCount,
    layerCountUnavailable,
    countLabel: countLabel(removedPageIds.length, layerCount, layerCountUnavailable, touchedPages.size),
    chips: chips.slice(0, MAX_CHIPS),
    allChips: chips,
    moreCount: Math.max(0, chips.length - MAX_CHIPS),
    rawIds,
    nameNotice,
    editSummary: [combinedTextEdit, ...otherEdits].filter((description): description is string => description !== null).join(" ") || null,
    pageIds: [...new Set([...removedPageIds, ...touchedPages])],
    layerIdsByPage,
  };
}
