import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApprovalBubble, approvalWindowTitle } from "./ApprovalBubble";

const approval = {
  approvalId: "apr_1",
  capabilityId: "design.applyOperations",
  action: "Delete the layer “Headline” from “ALPHA Launch cover”",
  parameters: [],
  risk: "destructive",
  affected: 1,
  expiresAt: "2026-10-06T12:01:00.000Z",
};

afterEach(() => vi.useRealTimers());

describe("the closed-panel approval signal", () => {
  it("shows the sentence, countdown and Review action above the collapsed composer", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-06T12:00:00.000Z"));
    const onReview = vi.fn();
    render(<ApprovalBubble approval={approval} answering={false} onReview={onReview} />);

    expect(screen.getByRole("status", { name: "Pending approval" })).toBeTruthy();
    expect(screen.getByText("Needs your approval")).toBeTruthy();
    expect(screen.getByText(approval.action)).toBeTruthy();
    expect(screen.getByText("Expires in 1:00")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    expect(onReview).toHaveBeenCalledOnce();
  });

  it("replaces the countdown with the sending state", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-06T12:00:00.000Z"));
    render(<ApprovalBubble approval={approval} answering deviceName="MacBook Air" onReview={vi.fn()} />);

    expect(screen.queryByText(/Expires in/u)).toBeNull();
    expect(screen.getByText("Sending your answer… Waiting for confirmation from MacBook Air.")).toBeTruthy();
  });

  it("sets the tab title to the exact approval-needed label while the chat is closed", () => {
    expect(approvalWindowTitle("ALPHA Launch cover", 1, false)).toBe("(1) Approval needed · Forma");
    expect(approvalWindowTitle("ALPHA Launch cover", 2, false)).toBe("(2) Approval needed · Forma");
    expect(approvalWindowTitle("ALPHA Launch cover", 1, true)).toBe("ALPHA Launch cover · Forma");
  });
});
