/**
 * What the person has open, as the chat reports it with each message (D-3, D-4).
 *
 * The ids go to the sidecar, which validates them and writes the line the agent
 * reads; the names stay here, for the chip the person sees. Both come from what
 * the editor already holds — the design from the query cache, the page and layer
 * from the store — so asking costs no request and cannot disagree with the screen.
 */
import type { QueryClient } from "@tanstack/react-query";

import type { DesignRecord } from "../api";
import { keys } from "../queries";
import { useEditor } from "../store";
import { layerId } from "../types";
import type { ViewChip, ViewIds } from "./chat";

export function currentView(queries: QueryClient, designId: string | null): { ids: ViewIds; chip?: ViewChip } {
  if (designId === null) return { ids: {} };
  const record = queries.getQueryData<DesignRecord>(keys.design(designId));
  const { selectedPageId, selectedLayerId } = useEditor.getState();
  const page = record?.document.pages.find((candidate) => candidate.id === selectedPageId);
  const layer = page?.layers.find((candidate) => layerId(candidate) === selectedLayerId);

  const ids: ViewIds = {
    designId,
    ...(record ? { projectId: record.project_id } : {}),
    ...(page ? { pageId: page.id } : {}),
    ...(layer ? { layerId: layerId(layer) } : {}),
  };
  // A design that has not loaded yet has no name to show; the ids still go, and the sidecar reads the name.
  if (!record) return { ids };
  return { ids, chip: { design: record.name, ...(page ? { page: page.name } : {}), ...(layer ? { layer: layer.name } : {}) } };
}
