/**
 * What each editor action does (plan §3.2).
 *
 * The table in §3.2 is nearly one to one with the store, which is the point: an
 * agent pointing at a layer should move the same selection a click moves, not a
 * second highlight that only it can set. So every handler goes through
 * `useEditor` — the editor has no private path for an agent any more than it has
 * one for itself.
 *
 * Two of the four name a design, and the sidecar has already asked Forma whether
 * the represented user may read it. What it cannot ask is whether that design is
 * the one the person has open, or whether the layer is in it: the server does
 * not know what this window is showing. Those are answered here, and a wrong
 * answer is `rejected` — a typed refusal the agent can act on — never a silent
 * success. `pontis/docs/13` § Acknowledgement is the rule being kept: do not
 * report that a UI action worked when nothing happened.
 */
import type { QueryClient } from "@tanstack/react-query";

import type { DesignRecord } from "../api";
import * as api from "../api";
import { keys } from "../queries";
import { MAX_ZOOM, MIN_ZOOM, useEditor, type Panel } from "../store";
import { layerId } from "../types";
import type { ActionHandler, HandlerOutcome } from "./channel";

const PANELS: readonly Panel[] = ["properties", "validation", "assets", "brand"];

/** What the editor must lend the channel: the one piece of state `App` owns. */
export interface EditorReach {
  /** The design the window is showing, or `null` on the project list. */
  openDesignId: () => string | null;
  /** Show a design, which `App` holds as state rather than in the URL. */
  openDesign: (designId: string) => void;
  /** For verifying a page or layer id, and warming what the editor is about to read. */
  queries: QueryClient;
}

const handled = (detail: string): HandlerOutcome => ({ outcome: "handled", detail });
const rejected = (detail: string): HandlerOutcome => ({ outcome: "rejected", detail });

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * The design, from the cache or the server.
 *
 * Fetched rather than only read: `ui.openDesign` is usually the first thing an
 * agent does with a design, so the cache is empty exactly when the answer
 * matters, and the editor is about to ask for the same record anyway. One
 * request verifies the ids and warms what the next render needs.
 */
async function record(reach: EditorReach, designId: string): Promise<DesignRecord | null> {
  try {
    return await reach.queries.fetchQuery({
      queryKey: keys.design(designId),
      queryFn: () => api.getDesign(designId),
    });
  } catch {
    return null;
  }
}

export function uiHandlers(reach: EditorReach): Record<string, ActionHandler> {
  const handlers: Record<string, ActionHandler> = {
    "ui.openDesign": async (input) => {
      if (!isRecord(input) || typeof input.design_id !== "string") return rejected("design_id is required");
      const designId = input.design_id;
      const pageId = typeof input.page_id === "string" ? input.page_id : null;

      const design = await record(reach, designId);
      // The sidecar authorized the read, so a failure here is the design having
      // gone or the server being unreachable — not a permission answer to
      // repeat, and not something to open a blank editor for.
      if (!design) return rejected(`design ${designId} could not be read`);
      if (pageId !== null && !design.document.pages.some((page) => page.id === pageId)) {
        return rejected(`design ${designId} has no page ${pageId}`);
      }

      reach.openDesign(designId);
      // Named page or first: either way the selection is set here rather than
      // left to the editor's own default, so the acknowledgement can say which
      // page the person is now looking at.
      const page = pageId ?? design.document.pages[0]?.id ?? null;
      useEditor.getState().selectPage(page);
      return handled(page === null ? design.name : `${design.name}, page ${page}`);
    },

    "ui.selectLayer": async (input) => {
      if (!isRecord(input) || typeof input.design_id !== "string" || typeof input.layer_id !== "string") {
        return rejected("design_id and layer_id are required");
      }
      const { design_id: designId, layer_id: wanted } = input;
      // Deliberately not an implicit navigation. The capability says "the
      // design the person has open", and quietly switching designs to satisfy a
      // selection would move them somewhere they did not ask to be; the agent
      // has `ui.openDesign` for that and the refusal says so.
      if (reach.openDesignId() !== designId) {
        return rejected(`the person does not have design ${designId} open; call ui.openDesign first`);
      }
      const design = await record(reach, designId);
      if (!design) return rejected(`design ${designId} could not be read`);
      const page = design.document.pages.find((candidate) =>
        candidate.layers.some((layer) => layerId(layer) === wanted),
      );
      if (!page) return rejected(`design ${designId} has no layer ${wanted}`);

      // A layer belongs to one page and `selectPage` clears the selection, so
      // the page has to move first when the layer is on another one.
      const editor = useEditor.getState();
      if (editor.selectedPageId !== page.id) editor.selectPage(page.id);
      useEditor.getState().selectLayer(wanted);
      return handled(`layer ${wanted} on page ${page.id}`);
    },

    "ui.setPanel": (input) => {
      if (!isRecord(input) || typeof input.panel !== "string") return rejected("panel is required");
      const panel = PANELS.find((candidate) => candidate === input.panel);
      // The schema already refuses anything else, so this is the second line
      // rather than the first; it exists because the editor is the thing that
      // would otherwise render an unknown panel as a blank column.
      if (!panel) return rejected(`${input.panel} is not one of ${PANELS.join(", ")}`);
      if (reach.openDesignId() === null) return rejected("open a design first");
      useEditor.getState().setPanel(panel);
      return handled(panel);
    },

    "ui.setZoom": (input) => {
      if (!isRecord(input) || typeof input.zoom !== "number" || !Number.isFinite(input.zoom)) {
        return rejected("zoom is required");
      }
      // The schema's range and the store's clamp are the same numbers, which
      // `crates/agent-catalog` says they must be. Refusing here rather than
      // clamping keeps that true in the one case it could stop being: a value
      // the agent believes it set and the editor silently changed.
      if (input.zoom < MIN_ZOOM || input.zoom > MAX_ZOOM) {
        return rejected(`zoom ${input.zoom} is outside ${MIN_ZOOM} to ${MAX_ZOOM}`);
      }
      if (reach.openDesignId() === null) return rejected("open a design first");
      useEditor.getState().setZoom(input.zoom);
      return handled(`zoom ${input.zoom}`);
    },
  };

  for (const id of Object.keys(handlers)) {
    const handler = handlers[id];
    if (!handler) continue;
    handlers[id] = (input) =>
      useEditor.getState().draft !== null
        ? rejected("the person is dragging")
        : handler(input);
  }

  return handlers;
}
