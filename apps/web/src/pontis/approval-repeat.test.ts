import { describe, expect, it } from "vitest";

import { chatReducer, emptyChat } from "./chat";
import type { DurableEvent } from "./events";

const SESSION = "ses_approval_repeat";
const CAPABILITY = "design.applyOperations";
const input = {
  design_id: "design-1",
  base_version: 6,
  operations: [{ op: "remove_layer", layer_id: "layer-headline" }],
};

function event(sequence: number, type: string, payload: Record<string, unknown>): DurableEvent {
  return { sequence, type, sessionId: SESSION, payload };
}

describe("repeat after a denied approval", () => {
  it("says the earlier denial was remembered and marks the repeated tool as not run", () => {
    let state = chatReducer(emptyChat, { type: "started", sessionId: SESSION, id: "local-start" });
    state = chatReducer(state, {
      type: "events",
      events: [
        event(1, "session.created", {}),
        event(2, "session.started", {}),
        event(3, "turn.started", { turnId: "turn-1" }),
        event(4, "capability.requested", { invocationId: "inv-1", capabilityId: CAPABILITY, input }),
        event(5, "approval.requested", {
          approvalId: "apr-1",
          invocationId: "inv-1",
          capabilityId: CAPABILITY,
          risk: "destructive",
          humanSummary: "Delete the layer “Headline” from “Launch”",
          importantArguments: [{ name: "operations", value: input.operations }, { name: "design_id", value: input.design_id }],
        }),
        event(6, "approval.resolved", { approvalId: "apr-1", status: "denied" }),
        event(7, "capability.failed", { invocationId: "inv-1", capabilityId: CAPABILITY, error: { code: "approval_denied", message: "you declined this action, so nothing was changed" } }),
      ],
    });

    expect(state.lines.find((line) => line.kind === "approval")?.status).toBe("denied");
    expect(state.lines.some((line) => line.kind === "approval-repeat")).toBe(false);

    state = chatReducer(state, {
      type: "events",
      events: [
        event(8, "capability.requested", { invocationId: "inv-2", capabilityId: CAPABILITY, input }),
        event(9, "capability.failed", { invocationId: "inv-2", capabilityId: CAPABILITY, error: { code: "approval_denied", message: "you declined this action, so nothing was changed" } }),
      ],
    });

    expect(state.lines.find((line) => line.kind === "approval-repeat")).toMatchObject({
      text: "You declined this earlier in this session, so it wasn't asked again.",
      hint: "Ask the agent again if you change your mind.",
    });
    expect(state.lines.some((line) => line.kind === "capability" && line.invocationId === "inv-2" && line.status === "not_run")).toBe(true);
    expect(state.lines.some((line) => line.kind === "result" && line.text === "Not run")).toBe(true);
  });
});
