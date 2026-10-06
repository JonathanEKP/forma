import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Canvas } from "./Canvas";
import { PagesAndLayers } from "./Panels";
import { useEditor } from "../store";
import type { DesignRecord, Layer } from "../types";

const headline: Layer = {
  type: "text",
  id: "l-headline",
  name: "Headline",
  text: "Hello",
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
  transform: { x: 10, y: 10, width: 100, height: 40, rotation_deg: 0 },
};

const record: DesignRecord = {
  id: "design-1",
  project_id: "project-1",
  name: "Design",
  version: 6,
  created_at: "2026-10-06T00:00:00Z",
  updated_at: "2026-10-06T00:00:00Z",
  document: {
    schema_version: 1,
    format: { preset: "square", width: 1080, height: 1080 },
    pages: [{ id: "p-cover", name: "Cover", layers: [headline] }],
  },
};

afterEach(() => {
  cleanup();
  useEditor.setState({ approvalHighlight: null, selectedLayerId: null, selectedPageId: null, draft: null });
  vi.restoreAllMocks();
});

describe("approval layer highlight", () => {
  it("outlines a layer without selecting it and blocks pointer editing while pending", () => {
    useEditor.setState({
      selectedPageId: "p-cover",
      selectedLayerId: null,
      approvalHighlight: {
        approvalId: "apr_1",
        designId: record.id,
        pageIds: ["p-cover"],
        layerIdsByPage: { "p-cover": [headline.id] },
      },
    });
    const onCommit = vi.fn();
    const { container } = render(
      <Canvas record={record} page={record.document.pages[0]} svg={undefined} onCommit={onCommit} disabled={false} lockedLayerIds={new Set([headline.id])} />,
    );
    const rect = container.querySelector('rect[data-approval-highlight="true"]');

    expect(rect?.getAttribute("stroke")).toBe("var(--color-signal)");
    expect(rect?.getAttribute("data-approval-locked")).toBe("true");
    fireEvent.pointerDown(rect as Element, { pointerId: 1, clientX: 20, clientY: 20 });

    expect(useEditor.getState().selectedLayerId).toBeNull();
    expect(useEditor.getState().draft).toBeNull();
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("marks affected Layers rows and explains why they are read-only", () => {
    useEditor.setState({
      selectedPageId: "p-cover",
      selectedLayerId: null,
      approvalHighlight: {
        approvalId: "apr_1",
        designId: record.id,
        pageIds: ["p-cover"],
        layerIdsByPage: { "p-cover": [headline.id] },
      },
    });
    const { container } = render(
      <PagesAndLayers record={record} onOperations={vi.fn()} disabled={false} fontFamily="Geist" onClose={vi.fn()} lockedLayerIds={new Set([headline.id])} />,
    );
    const row = screen.getByRole("button", { name: "Headline, read-only while an approval is pending" });

    expect(row.hasAttribute("disabled")).toBe(true);
    expect(row.getAttribute("data-approval-locked")).toBe("true");
    expect(row.getAttribute("data-approval-highlight")).toBe("true");
    expect(container.querySelector('[data-approval-highlight="true"]')).toBe(row);
  });
});
