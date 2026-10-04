/**
 * What the hook wires to the channel: the chat's session reopens the stream, and
 * a change of who is signed in is looked at.
 *
 * The channel itself is replaced, because its behaviour has its own tests in
 * `channel.test.ts`; what is under test here is only that the two are connected.
 */
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { agentBinding } from "./window";

const channel = { close: vi.fn(), refresh: vi.fn(), recheck: vi.fn(() => Promise.resolve()) };

vi.mock("./channel", () => ({
  openChannel: vi.fn(() => channel),
  liveFeed: { setState: vi.fn(), publish: vi.fn() },
}));

const unsubscribe = vi.fn();
let authListener: (() => void) | undefined;
vi.mock("../identity", () => ({
  accessToken: vi.fn(() => Promise.resolve("t")),
  currentSubject: vi.fn(() => Promise.resolve("person")),
  onSessionChange: (listener: () => void) => {
    authListener = listener;
    return unsubscribe;
  },
}));

import { useUiChannel } from "./useUiChannel";

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  agentBinding.set(null);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useUiChannel", () => {
  it("reopens the stream when the chat's session changes, so the window names it", () => {
    const { unmount } = renderHook(() => useUiChannel({ enabled: true, handlers: {} }));
    expect(channel.refresh).not.toHaveBeenCalled();

    act(() => agentBinding.set("ses_0123456789abcdef0123456789abcdef"));
    expect(channel.refresh).toHaveBeenCalledTimes(1);

    unmount();
    agentBinding.set("ses_fedcba9876543210fedcba9876543210");
    expect(channel.refresh).toHaveBeenCalledTimes(1);
    expect(channel.close).toHaveBeenCalled();
  });

  it("looks at who is signed in when the provider or another tab says the session moved", () => {
    renderHook(() => useUiChannel({ enabled: true, handlers: {} }));

    authListener?.();
    window.dispatchEvent(new StorageEvent("storage", { key: "sb-x-auth-token" }));
    vi.runAllTimers();

    expect(channel.recheck).toHaveBeenCalledTimes(2);
  });

  it("stops listening when the editor goes", () => {
    const { unmount } = renderHook(() => useUiChannel({ enabled: true, handlers: {} }));
    unmount();
    expect(unsubscribe).toHaveBeenCalled();
    window.dispatchEvent(new StorageEvent("storage"));
    vi.runAllTimers();
    expect(channel.recheck).not.toHaveBeenCalled();
  });

  it("opens nothing where there is no sidecar to connect to", () => {
    renderHook(() => useUiChannel({ enabled: false, handlers: {} }));
    agentBinding.set("ses_0123456789abcdef0123456789abcdef");
    expect(channel.refresh).not.toHaveBeenCalled();
  });
});
