/**
 * The four editor actions against the real store.
 *
 * `useEditor` is not mocked here, deliberately: the thing worth proving is that
 * an agent pointing at a layer moves the same selection a click moves, and a
 * fake store would prove only that the handler called a fake.
 */
import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as api from "../api";
import { keys } from "../queries";
import { useEditor } from "../store";
import { uiHandlers, type EditorReach } from "./handlers";

vi.mock("../api", () => ({ getDesign: vi.fn() }));

const DESIGN = "d-1";
const OTHER = "d-2";

const record = {
  id: DESIGN,
  project_id: "p-1",
  name: "Cover",
  version: 3,
  created_at: "2026-10-01T00:00:00Z",
  updated_at: "2026-10-01T00:00:00Z",
  document: {
    schema_version: 1,
    format: { width: 1080, height: 1350 },
    pages: [
      { id: "page-1", name: "One", layers: [{ type: "text", id: "layer-1", name: "Title" }] },
      { id: "page-2", name: "Two", layers: [{ type: "shape", id: "layer-2", name: "Bar" }] },
    ],
  },
};

function harness(openDesignId: string | null = DESIGN) {
  const queries = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const opened: string[] = [];
  let current = openDesignId;
  const reach: EditorReach = {
    openDesignId: () => current,
    openDesign: (id) => {
      opened.push(id);
      current = id;
    },
    queries,
  };
  return { handlers: uiHandlers(reach), opened, queries };
}

beforeEach(() => {
  useEditor.setState({ selectedPageId: null, selectedLayerId: null, zoom: 1, panel: "properties", draft: null });
  vi.mocked(api.getDesign).mockReset();
  vi.mocked(api.getDesign).mockResolvedValue(record as unknown as Awaited<ReturnType<typeof api.getDesign>>);
});

describe("ui.openDesign", () => {
  it("shows the design and selects its first page", async () => {
    const { handlers, opened } = harness(null);
    const outcome = await handlers["ui.openDesign"]?.({ design_id: DESIGN });
    expect(outcome).toEqual({ outcome: "handled", detail: "Cover, page page-1" });
    expect(opened).toEqual([DESIGN]);
    expect(useEditor.getState().selectedPageId).toBe("page-1");
  });

  it("selects the page it was given", async () => {
    const { handlers } = harness(null);
    await handlers["ui.openDesign"]?.({ design_id: DESIGN, page_id: "page-2" });
    expect(useEditor.getState().selectedPageId).toBe("page-2");
  });

  it("refuses a page the design does not have, rather than opening it anyway", async () => {
    const { handlers, opened } = harness(null);
    expect(await handlers["ui.openDesign"]?.({ design_id: DESIGN, page_id: "page-9" })).toEqual({
      outcome: "rejected",
      detail: `design ${DESIGN} has no page page-9`,
    });
    expect(opened).toEqual([]);
  });

  it("refuses when the design cannot be read, rather than opening a blank editor", async () => {
    const { handlers, opened } = harness(null);
    vi.mocked(api.getDesign).mockRejectedValue(new Error("gone"));
    expect(await handlers["ui.openDesign"]?.({ design_id: DESIGN })).toEqual({
      outcome: "rejected",
      detail: `design ${DESIGN} could not be read`,
    });
    expect(opened).toEqual([]);
  });

  it("warms the cache the editor is about to read, so the one request does both jobs", async () => {
    const { handlers, queries } = harness(null);
    await handlers["ui.openDesign"]?.({ design_id: DESIGN });
    expect(queries.getQueryData(keys.design(DESIGN))).toEqual(record);
    expect(vi.mocked(api.getDesign)).toHaveBeenCalledTimes(1);
  });
});

describe("ui.selectLayer", () => {
  it("selects the layer, moving to its page first", async () => {
    const { handlers } = harness();
    useEditor.getState().selectPage("page-1");
    expect(await handlers["ui.selectLayer"]?.({ design_id: DESIGN, layer_id: "layer-2" })).toEqual({
      outcome: "handled",
      detail: "layer layer-2 on page page-2",
    });
    expect(useEditor.getState().selectedPageId).toBe("page-2");
    expect(useEditor.getState().selectedLayerId).toBe("layer-2");
  });

  it("does not navigate to another design to satisfy a selection", async () => {
    const { handlers, opened } = harness(OTHER);
    expect(await handlers["ui.selectLayer"]?.({ design_id: DESIGN, layer_id: "layer-1" })).toEqual({
      outcome: "rejected",
      detail: `the person does not have design ${DESIGN} open; call ui.openDesign first`,
    });
    expect(opened).toEqual([]);
    expect(useEditor.getState().selectedLayerId).toBe(null);
  });

  it("refuses a layer that is not in the design", async () => {
    const { handlers } = harness();
    expect(await handlers["ui.selectLayer"]?.({ design_id: DESIGN, layer_id: "layer-9" })).toEqual({
      outcome: "rejected",
      detail: `design ${DESIGN} has no layer layer-9`,
    });
    expect(useEditor.getState().selectedLayerId).toBe(null);
  });
});

describe("ui.setPanel and ui.setZoom", () => {
  it("switches the panel", async () => {
    const { handlers } = harness();
    expect(await handlers["ui.setPanel"]?.({ panel: "validation" })).toEqual({
      outcome: "handled",
      detail: "validation",
    });
    expect(useEditor.getState().panel).toBe("validation");
  });

  it("refuses a panel that is not one of the four", async () => {
    const { handlers } = harness();
    expect(await handlers["ui.setPanel"]?.({ panel: "history" })).toEqual({
      outcome: "rejected",
      detail: "history is not one of properties, validation, assets, brand",
    });
    expect(useEditor.getState().panel).toBe("properties");
  });

  it("sets the zoom", async () => {
    const { handlers } = harness();
    expect(await handlers["ui.setZoom"]?.({ zoom: 2 })).toEqual({ outcome: "handled", detail: "zoom 2" });
    expect(useEditor.getState().zoom).toBe(2);
  });

  it("refuses a zoom outside the range instead of clamping one the agent believes it set", async () => {
    const { handlers } = harness();
    expect(await handlers["ui.setZoom"]?.({ zoom: 12 })).toEqual({
      outcome: "rejected",
      detail: "zoom 12 is outside 0.05 to 4",
    });
    expect(useEditor.getState().zoom).toBe(1);
  });
});
