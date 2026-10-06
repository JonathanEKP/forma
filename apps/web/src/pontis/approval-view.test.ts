import { describe, expect, it } from "vitest";

import { approvalView } from "./chat";
import { approvalOperationSource } from "./approval-details";

describe("approval operation data", () => {
  it("carries operations, design id and base version from the durable approval event", () => {
    const operations = [
      { op: "remove_layer", layer_id: "layer-1" },
      { op: "set_text", layer_id: "layer-2", content: "ALPHA" },
    ];
    const view = approvalView({
      approvalId: "apr_1",
      invocationId: "inv_1",
      capabilityId: "design.applyOperations",
      humanSummary: "Delete the layer “Footer” from “Launch”",
      reason: "Forma couldn't load the layer's name. Details shows its id.",
      importantArguments: [
        { name: "design_id", value: "design-1" },
        { name: "base_version", value: 6 },
        { name: "operations", value: operations },
      ],
    }, undefined);

    expect(approvalOperationSource(view)).toEqual({
      designId: "design-1",
      baseVersion: 6,
      operations,
      reason: "Forma couldn't load the layer's name. Details shows its id.",
    });
    expect(view?.invocationId).toBe("inv_1");
  });

  it("recovers operations and identifiers from the invocation input when the event omits them", () => {
    const operations = [{ op: "remove_page", page_id: "page-1" }];
    const view = approvalView({
      approvalId: "apr_2",
      invocationId: "inv_2",
      capabilityId: "design.applyOperations",
    }, { design_id: "design-2", base_version: 3, operations });

    expect(approvalOperationSource(view)).toEqual({
      designId: "design-2",
      baseVersion: 3,
      operations,
      reason: null,
    });
  });
});
