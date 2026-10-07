import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";

import { useEditor } from "../store";
import type { DesignRecord } from "../types";
import { keys } from "../queries";
import { currentView } from "./view";

const design: DesignRecord = {
  id: "design-1",
  project_id: "project-1",
  name: "ALPHA Launch cover",
  version: 1,
  created_at: "2026-10-06T00:00:00Z",
  updated_at: "2026-10-06T00:00:00Z",
  document: {
    schema_version: 1,
    format: { preset: "square", width: 1080, height: 1080 },
    pages: [
      { id: "page-1", name: "Cover", layers: [] },
      { id: "page-2", name: "Back", layers: [] },
    ],
  },
};

describe("the chat view on the projects page", () => {
  it("sends the selected project id and names it in the chip", () => {
    expect(currentView(new QueryClient(), null, { id: "project-1", name: "Launch" })).toEqual({
      ids: { projectId: "project-1" },
      chip: { project: "Launch" },
    });
  });

  it("has no project chip when the person is on the project list", () => {
    expect(currentView(new QueryClient(), null, null)).toEqual({ ids: {} });
  });

  it("numbers the selected page for the composer chip", () => {
    const queries = new QueryClient();
    queries.setQueryData(keys.design(design.id), design);
    useEditor.setState({ selectedPageId: "page-2", selectedLayerId: null });

    expect(currentView(queries, design.id)).toEqual({
      ids: { designId: "design-1", projectId: "project-1", pageId: "page-2" },
      chip: { design: "ALPHA Launch cover", page: "Back", pageNumber: 2 },
    });

    useEditor.setState({ selectedPageId: null, selectedLayerId: null });
  });
});
