/**
 * Server state (plan section 9.1).
 *
 * Every read the server owns goes through TanStack Query, so the app has one
 * copy of it and one place that decides when it is stale.
 */

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";

import * as api from "./api";
import type { DesignOperation, DesignRecord, Version } from "./api";

export const keys = {
  status: ["status"] as const,
  projects: ["projects"] as const,
  project: (id: string) => ["project", id] as const,
  brand: (projectId: string) => ["brand", projectId] as const,
  designs: (projectId: string) => ["designs", projectId] as const,
  design: (id: string) => ["design", id] as const,
  history: (id: string) => ["history", id] as const,
  validation: (id: string, version: Version) => ["validation", id, version] as const,
  assets: (projectId: string) => ["assets", projectId] as const,
  assetRequests: (projectId: string) => ["asset-requests", projectId] as const,
  preview: (designId: string, pageId: string, version: Version) =>
    ["preview", designId, pageId, version] as const,
};

export const useStatus = () =>
  useQuery({ queryKey: keys.status, queryFn: api.getStatus });

export const useProjects = () =>
  useQuery({ queryKey: keys.projects, queryFn: api.listProjects });

export const useBrand = (projectId: string | null) =>
  useQuery({
    queryKey: keys.brand(projectId ?? ""),
    queryFn: () => api.getBrand(projectId ?? ""),
    enabled: projectId !== null,
  });

export const useDesigns = (projectId: string | null) =>
  useQuery({
    queryKey: keys.designs(projectId ?? ""),
    queryFn: () => api.listDesigns(projectId ?? ""),
    enabled: projectId !== null,
  });

export const useDesign = (designId: string | null) =>
  useQuery({
    queryKey: keys.design(designId ?? ""),
    queryFn: () => api.getDesign(designId ?? ""),
    enabled: designId !== null,
  });

export const useHistory = (designId: string | null) =>
  useQuery({
    queryKey: keys.history(designId ?? ""),
    queryFn: () => api.getHistory(designId ?? ""),
    enabled: designId !== null,
  });

export const useAssets = (projectId: string | null) =>
  useQuery({
    queryKey: keys.assets(projectId ?? ""),
    queryFn: () => api.listAssets(projectId ?? ""),
    enabled: projectId !== null,
  });

/**
 * The rendered page, keyed by version.
 *
 * A version's document never changes, so a cached entry can never be stale —
 * only unused. That is what makes the preview safe to keep across a selection
 * change and what stops a flash of the old page on every edit.
 */
export const usePreview = (
  designId: string | null,
  pageId: string | null,
  version: Version | null,
) =>
  useQuery({
    queryKey: keys.preview(designId ?? "", pageId ?? "", version ?? 0),
    queryFn: () => api.fetchPreviewSvg(designId ?? "", pageId ?? "", version ?? 0),
    enabled: designId !== null && pageId !== null && version !== null,
    staleTime: Infinity,
  });

/**
 * Validation is pinned to a version for the same reason: rerunning it against
 * an unchanged document would return the same report.
 */
export const useValidation = (designId: string | null, version: Version | null) =>
  useQuery({
    queryKey: keys.validation(designId ?? "", version ?? 0),
    queryFn: () => api.validateDesign(designId ?? ""),
    enabled: designId !== null && version !== null,
    staleTime: Infinity,
  });

/**
 * Applies a batch and refreshes what it invalidated.
 *
 * The new record is written straight into the cache rather than refetched: the
 * server already returned it, and a round trip here would show the old preview
 * for a frame.
 */
export function useApplyOperations(
  designId: string | null,
): UseMutationResult<
  DesignRecord,
  Error,
  { baseVersion: Version; operations: DesignOperation[] }
> {
  const client = useQueryClient();

  return useMutation({
    mutationFn: ({ baseVersion, operations }) =>
      api.applyOperations(designId ?? "", baseVersion, operations),

    onSuccess: (record) => {
      client.setQueryData(keys.design(record.id), record);
      void client.invalidateQueries({ queryKey: keys.history(record.id) });
      void client.invalidateQueries({ queryKey: keys.designs(record.project_id) });
    },
  });
}

export function useUndo(designId: string | null) {
  const client = useQueryClient();

  return useMutation({
    mutationFn: (baseVersion: Version) => api.undo(designId ?? "", baseVersion),
    onSuccess: (record) => {
      client.setQueryData(keys.design(record.id), record);
      void client.invalidateQueries({ queryKey: keys.history(record.id) });
    },
  });
}

export function useCreateProject() {
  const client = useQueryClient();

  return useMutation({
    mutationFn: (name: string) => api.createProject(name),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: keys.projects });
    },
  });
}

export function useCreateDesign(projectId: string | null) {
  const client = useQueryClient();

  return useMutation({
    mutationFn: (name: string) => api.createDesign(projectId ?? "", name),
    onSuccess: (record) => {
      client.setQueryData(keys.design(record.id), record);
      void client.invalidateQueries({ queryKey: keys.designs(record.project_id) });
    },
  });
}

export function useUploadAsset(projectId: string | null) {
  const client = useQueryClient();

  return useMutation({
    mutationFn: (file: File) => api.uploadAsset(projectId ?? "", file),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: keys.assets(projectId ?? "") });
    },
  });
}

export const useAssetRequests = (projectId: string | null) =>
  useQuery({
    queryKey: keys.assetRequests(projectId ?? ""),
    queryFn: () => api.listAssetRequests(projectId ?? ""),
    enabled: projectId !== null,
  });

/**
 * Uploading the image an agent asked for, then marking the request resolved.
 *
 * The two steps are one mutation because a half-done resolve is the confusing
 * state: the asset would exist while the request still looked outstanding, and
 * the user would have no way to tell which upload it was waiting for. The
 * server refuses to resolve a request twice, so a retry after a failed second
 * step cannot silently swap the asset (plan section 22).
 */
export function useResolveAssetRequest(projectId: string | null) {
  const client = useQueryClient();

  return useMutation({
    mutationFn: async ({ requestId, file }: { requestId: string; file: File }) => {
      const asset = await api.uploadAsset(projectId ?? "", file);
      return api.resolveAssetRequest(requestId, asset.id);
    },
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: keys.assetRequests(projectId ?? "") });
      void client.invalidateQueries({ queryKey: keys.assets(projectId ?? "") });
    },
  });
}

export function useCancelAssetRequest(projectId: string | null) {
  const client = useQueryClient();

  return useMutation({
    mutationFn: (requestId: string) => api.cancelAssetRequest(requestId),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: keys.assetRequests(projectId ?? "") });
    },
  });
}
