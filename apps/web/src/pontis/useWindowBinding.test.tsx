import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { useWindowBinding } from "./useWindowBinding";
import { agentBinding } from "./window";

beforeEach(() => agentBinding.set(null));

describe("useWindowBinding", () => {
  it("reports the chat's session, follows it to the next, and lets go when the chat does", () => {
    const { rerender, unmount } = renderHook(({ id }: { id: string | null }) => useWindowBinding(id), {
      initialProps: { id: null as string | null },
    });
    expect(agentBinding.get()).toBeNull();

    rerender({ id: "ses_0123456789abcdef0123456789abcdef" });
    expect(agentBinding.get()).toBe("ses_0123456789abcdef0123456789abcdef");

    rerender({ id: "ses_fedcba9876543210fedcba9876543210" });
    expect(agentBinding.get()).toBe("ses_fedcba9876543210fedcba9876543210");

    unmount();
    expect(agentBinding.get()).toBeNull();
  });
});
