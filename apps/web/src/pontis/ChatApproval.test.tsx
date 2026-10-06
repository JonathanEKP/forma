import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApprovalCard } from "./ChatApproval";
import type { ApprovalStatus } from "./chat";
import { useEditor } from "../store";
import type { DesignRecord } from "../types";
import { keys } from "../queries";
import type { ApprovalCardView } from "./ChatApproval";

const baseApproval: ApprovalCardView = {
  approvalId: "apr_1",
  invocationId: "inv_1",
  capabilityId: "design.applyOperations",
  action: "Delete the layer “Headline” from “ALPHA Launch cover”",
  parameters: [],
  risk: "destructive",
  affected: 1,
  expiresAt: "2026-10-06T12:01:00.000Z",
  designId: null,
  baseVersion: null,
  operations: [],
  reason: null,
};

const designRecord: DesignRecord = {
  id: "design-1",
  project_id: "project-1",
  name: "ALPHA Launch cover",
  version: 6,
  created_at: "2026-10-06T00:00:00Z",
  updated_at: "2026-10-06T00:00:00Z",
  document: {
    schema_version: 1,
    format: { preset: "square", width: 1080, height: 1080 },
    pages: [{
      id: "p-cover",
      name: "Cover",
      layers: [{
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
        transform: { x: 0, y: 0, width: 100, height: 40, rotation_deg: 0 },
      }],
    }],
  },
};

function card(status: ApprovalStatus = "pending", overrides: Partial<Parameters<typeof ApprovalCard>[0]> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <ApprovalCard
        approval={baseApproval}
        status={status}
        answering={false}
        unconfirmed={false}
        onDecide={vi.fn()}
        {...overrides}
      />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  useEditor.setState({ approvalHighlight: null, openDesignId: null, selectedLayerId: null, selectedPageId: null, draft: null });
  vi.useRealTimers();
});

describe("approval card states", () => {
  it("shows the risk, action, deadline and the only two answers while pending", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-06T12:00:00.000Z"));
    card();

    expect(screen.getByText("NEEDS YOUR APPROVAL")).toBeTruthy();
    expect(screen.getByText("DESTRUCTIVE")).toBeTruthy();
    expect(screen.getByText(baseApproval.action)).toBeTruthy();
    expect(screen.getByText("Expires in 1:00").classList.contains("editor-approval-countdown")).toBe(true);
    expect(screen.getByRole("button", { name: "Approve once" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Deny" })).toBeTruthy();
  });

  it("counts down from expiresAt and marks under thirty seconds urgent", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-06T12:00:00.000Z"));
    card();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(31_000);
    });

    const countdown = screen.getByText("Expires in 0:29");
    expect(countdown.getAttribute("data-urgency")).toBe("urgent");
  });

  it("replaces the deadline and both buttons while sending", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-06T12:00:00.000Z"));
    card("pending", { answering: true });

    expect(screen.getByText("Sending your answer… Waiting for confirmation from your device.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Approve once" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Deny" })).toBeNull();
    expect(screen.queryByText(/Expires in/u)).toBeNull();
  });

  it("restores both actions with a quiet unconfirmed explanation", () => {
    card("pending", { unconfirmed: true });

    expect(screen.getByText("Not confirmed by your device")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Approve once" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Deny" })).toBeTruthy();
  });

  it("shows Details collapsed with design and target ids, base version and raw operations", () => {
    const approval = {
      ...baseApproval,
      designId: designRecord.id,
      baseVersion: 6,
      operations: [{ op: "remove_layer", layer_id: "l-headline" }],
    };
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } });
    client.setQueryData(keys.design(designRecord.id), designRecord);
    render(
      <QueryClientProvider client={client}>
        <ApprovalCard approval={approval} status="pending" answering={false} unconfirmed={false} onDecide={vi.fn()} />
      </QueryClientProvider>,
    );
    const details = screen.getByText("Details").closest("details") as HTMLDetailsElement;

    expect(details.open).toBe(false);
    fireEvent.click(screen.getByText("Details"));
    expect(details.open).toBe(true);
    expect(screen.getByText("design_id")).toBeTruthy();
    expect(screen.getByText("base_version")).toBeTruthy();
    expect(screen.getByText("l-headline")).toBeTruthy();
    expect(screen.getByText(/remove_layer/u)).toBeTruthy();
  });

  it("highlights pending layers without selecting them and only offers the canvas toggle for this design", () => {
    const approval = {
      ...baseApproval,
      designId: designRecord.id,
      baseVersion: 6,
      operations: [{ op: "remove_layer", layer_id: "l-headline" }],
    };
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } });
    client.setQueryData(keys.design(designRecord.id), designRecord);
    useEditor.setState({ openDesignId: designRecord.id, selectedPageId: "p-cover", selectedLayerId: "l-headline" });
    render(
      <QueryClientProvider client={client}>
        <ApprovalCard approval={approval} status="pending" answering={false} unconfirmed={false} onDecide={vi.fn()} />
      </QueryClientProvider>,
    );
    const toggle = screen.getByRole("button", { name: "Show on canvas" });

    fireEvent.click(toggle);

    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(useEditor.getState().approvalHighlight?.layerIdsByPage["p-cover"]).toEqual(["l-headline"]);
    expect(useEditor.getState().selectedLayerId).toBeNull();
  });

  it.each([
    ["approved", "Approved. Deleted the layer “Headline”."],
    ["denied", "Denied. “Headline” was kept."],
    ["expired", "Expired before you answered. “Headline” was kept."],
    ["cancelled", "Cancelled. “Headline” was kept."],
  ] as const)("shows the settled one-line outcome for %s", (status, outcome) => {
    card(status);

    expect(screen.getByText(outcome)).toBeTruthy();
    if (status !== "approved") expect(screen.getByText("Ask the agent again if you change your mind.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Approve once" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Deny" })).toBeNull();
  });
});
