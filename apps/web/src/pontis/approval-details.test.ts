import { describe, expect, it } from "vitest";

import type { DesignRecord, Layer, Page } from "../types";
import { buildApprovalDetails } from "./approval-details";

function textLayer(id: string, name: string): Layer {
  return {
    type: "text",
    id,
    name,
    text: name,
    font_family: "Geist",
    font_size: 32,
    font_weight: 400,
    line_height: 1.2,
    letter_spacing: 0,
    align: "left",
    color: "#171717",
    visible: true,
    locked: false,
    opacity: 1,
    transform: { x: 0, y: 0, width: 100, height: 40, rotation_deg: 0 },
  };
}

function page(id: string, name: string, layers: readonly Layer[]): Page {
  return { id, name, layers: [...layers] };
}

function design(pages: readonly Page[]): DesignRecord {
  return {
    id: "design-1",
    project_id: "project-1",
    name: "ALPHA Launch cover",
    version: 6,
    created_at: "2026-10-06T00:00:00Z",
    updated_at: "2026-10-06T00:00:00Z",
    document: { schema_version: 1, format: { preset: "square", width: 1080, height: 1080 }, pages: [...pages] },
  };
}

describe("approval details", () => {
  it("names a single removed layer", () => {
    const record = design([page("p-cover", "Cover", [textLayer("l-headline", "Headline")])]);
    const details = buildApprovalDetails([{ op: "remove_layer", layer_id: "l-headline" }], record);

    expect(details.countLabel).toBe("1 layer will be deleted.");
    expect(details.chips).toEqual([{ kind: "layer", id: "l-headline", label: "Headline", pageName: "Cover", showPageSuffix: false }]);
    expect(details.moreCount).toBe(0);
    expect(details.layerIdsByPage).toEqual({ "p-cover": ["l-headline"] });
  });

  it("counts a removed page separately from its three layers and marks its chip as a page", () => {
    const record = design([
      page("p-cover", "Cover", []),
      page("p-back", "Back", [textLayer("l-headline", "Headline"), textLayer("l-tagline", "Tagline"), textLayer("l-footer", "Footer")]),
    ]);
    const details = buildApprovalDetails([{ op: "remove_page", page_id: "p-back" }], record);

    expect(details.countLabel).toBe("1 page and 3 layers will be deleted.");
    expect(details.chips).toEqual([
      { kind: "page", id: "p-back", label: "Back", pageName: "Back", showPageSuffix: false },
      { kind: "layer", id: "l-footer", label: "Footer", pageName: "Back", showPageSuffix: false },
      { kind: "layer", id: "l-tagline", label: "Tagline", pageName: "Back", showPageSuffix: false },
      { kind: "layer", id: "l-headline", label: "Headline", pageName: "Back", showPageSuffix: false },
    ]);
    expect(details.layerIdsByPage).toEqual({ "p-back": ["l-footer", "l-tagline", "l-headline"] });
  });

  it("shows five chips and a page suffix for eight removed layers across two pages", () => {
    const coverLayers = ["Kicker", "Headline", "Tagline", "Footer", "Accent block"].map((name) => textLayer(`cover-${name}`, name));
    const backLayers = ["Kicker", "Headline", "Tagline", "Footer"].map((name) => textLayer(`back-${name}`, name));
    const record = design([page("p-cover", "Cover", coverLayers), page("p-back", "Back", backLayers)]);
    const operations = [coverLayers[1], coverLayers[2], coverLayers[3], coverLayers[4], ...backLayers].map((layer) => ({ op: "remove_layer", layer_id: layer.id }));
    const details = buildApprovalDetails(operations, record);

    expect(details.countLabel).toBe("8 layers will be deleted. Both pages stay.");
    expect(details.chips).toHaveLength(5);
    expect(details.chips.map((chip) => `${chip.label} · ${chip.pageName}`)).toEqual([
      "Headline · Cover",
      "Tagline · Cover",
      "Footer · Cover",
      "Accent block · Cover",
      "Kicker · Back",
    ]);
    expect(details.moreCount).toBe(3);
  });

  it("puts the removal before quieter text edits in a mixed batch", () => {
    const record = design([
      page("p-cover", "Cover", [textLayer("l-footer", "Footer"), textLayer("l-headline", "Headline"), textLayer("l-tagline", "Tagline")]),
    ]);
    const details = buildApprovalDetails(
      [
        { op: "set_text", layer_id: "l-headline", content: "ALPHA" },
        { op: "remove_layer", layer_id: "l-footer" },
        { op: "set_text", layer_id: "l-tagline", content: "A new chapter starts here." },
      ],
      record,
    );

    expect(details.countLabel).toBe("1 layer will be deleted.");
    expect(details.chips.map((chip) => chip.label)).toEqual(["Footer"]);
    expect(details.editSummary).toBe('Also edit Headline to “ALPHA” and Tagline to “A new chapter starts here.”.');
  });

  it("keeps ids in Details and says when a layer name could not be read", () => {
    const details = buildApprovalDetails([{ op: "remove_layer", layer_id: "l-private-id" }], null);

    expect(details.countLabel).toBe("1 layer will be deleted.");
    expect(details.chips).toEqual([]);
    expect(details.rawIds).toEqual(["l-private-id"]);
    expect(details.nameNotice).toBe("Forma couldn't load the layer's name. Details shows its id.");
  });
});
