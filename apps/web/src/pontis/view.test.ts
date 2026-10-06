import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";

import { currentView } from "./view";

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
});
