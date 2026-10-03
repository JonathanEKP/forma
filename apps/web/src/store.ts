/**
 * Ephemeral UI state (plan section 9.1).
 *
 * What belongs here is everything the server does not own: which page and layer
 * are selected, the zoom, and the provisional geometry of a drag that has not
 * been committed yet. Anything the server owns lives in TanStack Query, so
 * there is exactly one copy of it.
 */

import type { EditorPanel } from "@forma/bindings/EditorPanel";
import { create } from "zustand";

/** A frame being dragged or resized, before the operation is sent. */
export interface DraftFrame {
  layerId: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * The side panels, from Rust rather than written out again here.
 *
 * `crates/agent-catalog` exports `EditorPanel` because `ui.setPanel`'s schema is
 * built from it, and two hand-kept copies of the same union would have had
 * nothing guarding the drift between them — an agent would have been told
 * `properties, validation, assets, brand` by a schema the editor no longer
 * agreed with.
 */
export type Panel = EditorPanel;

interface EditorState {
  selectedPageId: string | null;
  selectedLayerId: string | null;
  zoom: number;
  panel: Panel;
  /**
   * Set only while a pointer is down. Plan section 38: the pointer move is
   * local state and sends nothing, so a drag is one operation rather than one
   * per frame.
   */
  draft: DraftFrame | null;

  selectPage: (id: string | null) => void;
  selectLayer: (id: string | null) => void;
  setZoom: (zoom: number) => void;
  setPanel: (panel: Panel) => void;
  setDraft: (draft: DraftFrame | null) => void;
}

/** Zoom is clamped so a stray wheel event cannot make the canvas unusable. */
export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 4;

export const useEditor = create<EditorState>((set) => ({
  selectedPageId: null,
  selectedLayerId: null,
  zoom: 1,
  panel: "properties",
  draft: null,

  selectPage: (selectedPageId) =>
    // A layer belongs to one page, so it cannot stay selected across a change.
    set({ selectedPageId, selectedLayerId: null, draft: null }),

  selectLayer: (selectedLayerId) => set({ selectedLayerId, draft: null }),

  setZoom: (zoom) => set({ zoom: Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom)) }),

  setPanel: (panel) => set({ panel }),

  setDraft: (draft) => set({ draft }),
}));
