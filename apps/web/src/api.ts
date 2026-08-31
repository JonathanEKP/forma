/**
 * Thin client for forma-server.
 *
 * Response shapes are generated from Rust (plan section 30) and never
 * redeclared here: CI fails if the checked-in bindings drift.
 */

import type { ApiKey } from "@forma/bindings/ApiKey";
import type { Asset } from "@forma/bindings/Asset";
import type { AssetRequest } from "@forma/bindings/AssetRequest";
import type { BrandKit } from "@forma/bindings/BrandKit";
import type { BrandKitDraft } from "@forma/bindings/BrandKitDraft";
import type { CreateApiKeyRequest } from "@forma/bindings/CreateApiKeyRequest";
import type { CreatedApiKey } from "@forma/bindings/CreatedApiKey";
import type { DesignOperation } from "@forma/bindings/DesignOperation";
import type { DesignRecord } from "@forma/bindings/DesignRecord";
import type { DesignSummary } from "@forma/bindings/DesignSummary";
import type { Export } from "@forma/bindings/Export";
import type { ExportScope } from "@forma/bindings/ExportScope";
import type { Project } from "@forma/bindings/Project";
import type { Status } from "@forma/bindings/Status";
import type { ValidationReport } from "@forma/bindings/ValidationReport";
import type { Version } from "@forma/bindings/Version";
import type { VersionSummary } from "@forma/bindings/VersionSummary";

export type { BrandKitDraft } from "@forma/bindings/BrandKitDraft";
export type { Export, ExportScope };

export type {
  ApiKey,
  Asset,
  AssetRequest,
  BrandKit,
  DesignOperation,
  DesignRecord,
  DesignSummary,
  Project,
  Status,
  ValidationReport,
  Version,
  VersionSummary,
};

export interface ApiErrorBody {
  code: string;
  message: string;
  details?: unknown;
  hint?: string;
}

/**
 * A failed call, carrying the stable code of plan section 32.
 *
 * The code is what callers branch on; the HTTP status is a coarser signal that
 * the server derives from it.
 */
export class ApiError extends Error {
  readonly code: string;
  readonly hint?: string;
  readonly details?: unknown;
  readonly status: number;

  constructor(body: ApiErrorBody, status: number) {
    super(body.message);
    this.name = "ApiError";
    this.code = body.code;
    this.hint = body.hint;
    this.details = body.details;
    this.status = status;
  }

  /** The design moved under us: refetch, rebase, resend (plan section 27). */
  get isConflict(): boolean {
    return this.code === "VERSION_CONFLICT";
  }
}

/** The two versions a conflict reports, when it reports them. */
export interface ConflictDetails {
  expected: number;
  actual: number;
}

export function conflictDetails(error: ApiError): ConflictDetails | null {
  const details = error.details;
  if (typeof details !== "object" || details === null) return null;

  const record = details as Record<string, unknown>;
  const expected = record.expected;
  const actual = record.actual;

  return typeof expected === "number" && typeof actual === "number"
    ? { expected, actual }
    : null;
}

/**
 * The one place where untyped data enters the app.
 *
 * `Response.json()` is typed as `any` by the DOM lib, which would let `any`
 * spread silently through everything downstream. It is funnelled through
 * `unknown` here and asserted exactly once, against a shape generated from
 * Rust. Nothing outside this function ever handles an unasserted payload.
 */
async function readJson(response: Response): Promise<unknown> {
  return (await response.json()) as unknown;
}

/** Who the server records as the author of a mutation (plan section 61). */
const ACTOR_HEADER = { "x-forma-actor": "human" } as const;

async function fail(response: Response): Promise<never> {
  const payload = (await readJson(response).catch(() => null)) as {
    error?: ApiErrorBody;
  } | null;

  throw new ApiError(
    payload?.error ?? { code: "SERVER_ERROR", message: `HTTP ${response.status}` },
    response.status,
  );
}

async function request<T>(
  path: string,
  init?: { method?: string; body?: unknown },
): Promise<T> {
  const hasBody = init?.body !== undefined;

  const response = await fetch(`/api/v1${path}`, {
    method: init?.method ?? "GET",
    headers: hasBody
      ? { "content-type": "application/json", ...ACTOR_HEADER }
      : ACTOR_HEADER,
    body: hasBody ? JSON.stringify(init.body) : undefined,
  });

  if (!response.ok) await fail(response);
  if (response.status === 204) return undefined as T;

  return (await readJson(response)) as T;
}
// --- personal access tokens -----------------------------------------------

export const listApiKeys = () => request<ApiKey[]>("/api-keys");

export const createApiKey = (body: CreateApiKeyRequest) =>
  request<CreatedApiKey>("/api-keys", { method: "POST", body });

export const revokeApiKey = (id: string) =>
  request<void>(`/api-keys/${id}`, { method: "DELETE" });

// --- meta -----------------------------------------------------------------

export const getStatus = () => request<Status>("/status");

// --- projects -------------------------------------------------------------

export const listProjects = () => request<Project[]>("/projects");

export const createProject = (name: string) =>
  request<Project>("/projects", { method: "POST", body: { name } });

export const getProject = (id: string) => request<Project>(`/projects/${id}`);

/**
 * The editable half of a Brand Kit.
 *
 * The `BrandId` and the kit's version are the server's to decide, which is why
 * this is a draft rather than a `BrandKit` (plan section 13).
 */
export const putBrand = (projectId: string, draft: BrandKitDraft) =>
  request<BrandKit>(`/projects/${projectId}/brand`, { method: "PUT", body: draft });

export const getBrand = (projectId: string) =>
  request<BrandKit | null>(`/projects/${projectId}/brand`);

// --- designs --------------------------------------------------------------

export const listDesigns = (projectId: string) =>
  request<DesignSummary[]>(`/projects/${projectId}/designs`);

export const createDesign = (projectId: string, name: string) =>
  request<DesignRecord>(`/projects/${projectId}/designs`, {
    method: "POST",
    body: { name },
  });

export const getDesign = (id: string) => request<DesignRecord>(`/designs/${id}`);

export const getHistory = (id: string) =>
  request<VersionSummary[]>(`/designs/${id}/history`);

/**
 * The single mutation entry point (plan section 28).
 *
 * `baseVersion` is what the caller last read. A design that moved in between
 * answers VERSION_CONFLICT rather than silently overwriting the other writer.
 */
export const applyOperations = (
  id: string,
  baseVersion: Version,
  operations: DesignOperation[],
) =>
  request<DesignRecord>(`/designs/${id}/operations`, {
    method: "POST",
    body: { base_version: baseVersion, operations },
  });

export const undo = (id: string, baseVersion: Version) =>
  request<DesignRecord>(`/designs/${id}/undo`, {
    method: "POST",
    body: { base_version: baseVersion },
  });

export const validateDesign = (id: string) =>
  request<ValidationReport>(`/designs/${id}/validate`, { method: "POST" });

// --- assets ---------------------------------------------------------------

export const listAssets = (projectId: string) =>
  request<Asset[]>(`/projects/${projectId}/assets`);

export async function uploadAsset(projectId: string, file: File): Promise<Asset> {
  const form = new FormData();
  form.append("file", file);

  // No content-type header: the browser sets it with the multipart boundary.
  const response = await fetch(`/api/v1/projects/${projectId}/assets`, {
    method: "POST",
    headers: ACTOR_HEADER,
    body: form,
  });

  if (!response.ok) await fail(response);
  return (await readJson(response)) as Asset;
}

export const assetContentUrl = (id: string) => `/api/v1/assets/${id}/content`;

// --- asset requests -------------------------------------------------------

export const listAssetRequests = (projectId: string) =>
  request<AssetRequest[]>(`/projects/${projectId}/asset-requests`);

export const resolveAssetRequest = (id: string, assetId: string) =>
  request<AssetRequest>(`/asset-requests/${id}/resolve`, {
    method: "POST",
    body: { asset_id: assetId },
  });

export const cancelAssetRequest = (id: string) =>
  request<AssetRequest>(`/asset-requests/${id}/cancel`, { method: "POST" });

// --- exports ---------------------------------------------------------------

export interface ExportedFile {
  filename: string;
  byte_size: number;
}

export interface ExportResult {
  export: Export;
  files: ExportedFile[];
}

/**
 * Renders the design to PNG files and records what was written.
 *
 * The export pins the version it came from, so the files stay reproducible
 * after the design moves on (plan section 71).
 */
export const createExport = (
  designId: string,
  scope: ExportScope,
  pageId: string | null,
  scale: number,
) =>
  request<ExportResult>(`/designs/${designId}/exports`, {
    method: "POST",
    body: { scope, page_id: pageId, scale },
  });

/** A plain link: the server sends the bytes as an attachment. */
export const exportDownloadUrl = (exportId: string, filename: string) =>
  `/api/v1/exports/${exportId}/download?file=${encodeURIComponent(filename)}`;

// --- rendering ------------------------------------------------------------

/**
 * The authoritative SVG for one page, with text already flattened to paths.
 *
 * The editor draws this and puts its interaction layer on top; it never
 * reimplements typography (plan section 37). The version is in the URL so a
 * cached response can never be shown against a newer document.
 */
export const previewSvgUrl = (designId: string, pageId: string, version: Version) =>
  `/api/v1/designs/${designId}/pages/${pageId}/preview.svg?v=${version}`;

export const previewPngUrl = (
  designId: string,
  pageId: string,
  version: Version,
  scale = 1,
) => `/api/v1/designs/${designId}/pages/${pageId}/preview.png?scale=${scale}&v=${version}`;

export async function fetchPreviewSvg(
  designId: string,
  pageId: string,
  version: Version,
): Promise<string> {
  const response = await fetch(previewSvgUrl(designId, pageId, version));
  if (!response.ok) await fail(response);
  return await response.text();
}
