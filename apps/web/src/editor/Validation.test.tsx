import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Validation, validationMessage } from "./Validation";
import { useEditor } from "../store";
import type { ValidationReport } from "../api";
import type { ValidationIssue } from "@forma/bindings/ValidationIssue";
import type { DesignRecord } from "../types";

const record: DesignRecord = {
  id: "d1",
  project_id: "p1",
  name: "Cover",
  version: 4,
  created_at: "2026-10-08T00:00:00Z",
  updated_at: "2026-10-08T00:00:00Z",
  document: {
    schema_version: 1,
    format: { preset: "square", width: 1080, height: 1080 },
    pages: [
      {
        id: "p1",
        name: "Cover",
        layers: [
          {
            type: "text",
            id: "l1",
            name: "Headline",
            text: "Hello",
            font_family: "Geist",
            font_size: 48,
            font_weight: 500,
            line_height: 1.2,
            letter_spacing: 0,
            align: "left",
            color: "#171717",
            visible: true,
            locked: false,
            opacity: 1,
            transform: { x: 10, y: 10, width: 100, height: 60, rotation_deg: 0 },
          },
        ],
      },
    ],
  },
};

const issue = (overrides: Partial<ValidationIssue> = {}): ValidationIssue => ({
  code: "TEXT_OVERFLOW_WIDTH",
  severity: "error",
  page_id: "p1",
  layer_id: "l1",
  message: "Headline needs more width",
  details: { measured: 120, available: 100 },
  ...overrides,
});

afterEach(() => {
  useEditor.setState({ selectedPageId: null, selectedLayerId: null });
  vi.restoreAllMocks();
});

describe("the Checks panel", () => {
  it("shows a clean summary and offers Check again", () => {
    render(
      <Validation
        record={record}
        report={{ valid: true, issues: [] }}
        loading={false}
        onCheckAgain={vi.fn()}
      />,
    );

    expect(screen.getByText("No errors.")).toBeTruthy();
    expect(screen.getByText("This design can be exported.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Check again" })).toBeTruthy();
  });

  it("renders a plain message and layer/page names, with the code folded", () => {
    const report: ValidationReport = {
      valid: false,
      issues: [
        issue(),
        issue({
          code: "OUTSIDE_SAFE_AREA",
          severity: "warning",
          page_id: "p1",
          layer_id: "l1",
        }),
      ],
    };
    const onSelect = vi.fn();
    render(<Validation record={record} report={report} loading={false} onSelect={onSelect} onCheckAgain={vi.fn()} />);

    expect(screen.getByText("1 error needs attention")).toBeTruthy();
    expect(screen.getByText("1 warning · review these before exporting. Export anyway is available.")).toBeTruthy();
    expect(screen.getByText("This text is too wide for its box.")).toBeTruthy();
    expect(screen.getAllByText("Headline · Cover")).toHaveLength(2);
    for (const summary of screen.getAllByText("Technical detail")) {
      expect(summary.closest("details")?.open).toBe(false);
    }

    fireEvent.click(screen.getByRole("button", { name: /This text is too wide/ }));
    expect(useEditor.getState().selectedPageId).toBe("p1");
    expect(useEditor.getState().selectedLayerId).toBe("l1");
    expect(onSelect).toHaveBeenCalledOnce();
  });

  it("shows the technical code only after opening its disclosure", () => {
    render(
      <Validation
        record={record}
        report={{ valid: false, issues: [issue()] }}
        loading={false}
        onCheckAgain={vi.fn()}
      />,
    );

    const disclosure = screen.getByText("Technical detail").closest("details");
    expect(disclosure?.open).toBe(false);
    fireEvent.click(screen.getByText("Technical detail"));
    expect(disclosure?.open).toBe(true);
    expect(screen.getByText("TEXT_OVERFLOW_WIDTH")).toBeTruthy();
  });

  it("keeps a plain, reader-facing message for every validator code", () => {
    const codes = [
      "INVALID_PAGE_SIZE",
      "MISSING_LAYER",
      "DUPLICATE_LAYER_ID",
      "INVALID_OPACITY",
      "INVALID_DIMENSION",
      "OUTSIDE_CANVAS",
      "OUTSIDE_SAFE_AREA",
      "MISSING_BRAND_FONT",
      "UNKNOWN_BRAND_TOKEN",
      "COLOR_OUTSIDE_BRAND",
      "ASSET_NOT_FOUND",
      "ASSET_UNREADABLE",
      "INVALID_ASSET_TYPE",
      "FONT_NOT_FOUND",
      "TEXT_INVALID_CHARACTER",
      "FONT_TOO_SMALL",
      "TEXT_LOW_CONTRAST",
      "TEXT_OVERFLOW_WIDTH",
      "TEXT_OVERFLOW_HEIGHT",
    ];

    for (const code of codes) {
      const message = validationMessage(issue({ code }));
      expect(message.length, code).toBeGreaterThan(8);
      expect(message).not.toContain(code);
    }
  });
});
