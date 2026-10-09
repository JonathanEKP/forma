import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Onboard, SignIn } from "./SignIn";

const identity = vi.hoisted(() => ({
  accessToken: vi.fn(),
  sendMagicLink: vi.fn(),
  signOut: vi.fn(),
}));

vi.mock("./identity", () => ({
  accessToken: identity.accessToken,
  sendMagicLink: identity.sendMagicLink,
  signOut: identity.signOut,
}));

function jsonResponse(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("sign-in and first visit", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    identity.accessToken.mockResolvedValue(null);
    identity.sendMagicLink.mockResolvedValue(undefined);
    identity.signOut.mockResolvedValue(undefined);
    window.history.replaceState({}, "", "/login");
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("renders the approved sign-in frame and passwordless email form", () => {
    render(<SignIn />);

    expect(screen.getByRole("heading", { name: "Sign in to Forma" })).toBeTruthy();
    expect(screen.getByLabelText("Email")).toBeTruthy();
    expect(screen.getByText(/There is no password\./)).toBeTruthy();
    expect(screen.getByText("A workspace for your visual ideas.")).toBeTruthy();
    expect(screen.getByText("Invite-only")).toBeTruthy();
    expect(screen.queryByLabelText(/password/i)).toBeNull();
  });

  it("keeps the AI-app return context and shows all inbox instructions", async () => {
    render(<SignIn returnTo="http://localhost/oauth/consent?authorization_id=auth-1" />);
    fireEvent.change(screen.getByLabelText("Email"), { target: { value: "alex@example.com" } });
    fireEvent.click(screen.getByRole("button", { name: "Send me a link" }));

    await waitFor(() => expect(screen.getByRole("heading", { name: "Check your inbox" })).toBeTruthy());
    expect(identity.sendMagicLink).toHaveBeenCalledWith(
      "alex@example.com",
      "http://localhost/oauth/consent?authorization_id=auth-1",
    );
    expect(screen.getByText(/An AI app wants to connect to Forma\./)).toBeTruthy();
    expect(screen.getByText(/Sign in to continue\./)).toBeTruthy();
    expect(screen.getByText(/After signing in/)).toBeTruthy();
    expect(screen.getByText("Sent to")).toBeTruthy();
    expect(screen.getByText("alex@example.com")).toBeTruthy();
    expect(screen.getByText(/Open it in this browser/)).toBeTruthy();
    expect(screen.getByText(/check your spam folder/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Use a different email" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Send again in 1:00" })).toHaveProperty("disabled", true);
  });

  it("keeps the address when sending fails", async () => {
    identity.sendMagicLink.mockRejectedValueOnce(new Error("provider unavailable"));
    render(<SignIn />);
    const field = screen.getByLabelText("Email");
    fireEvent.change(field, { target: { value: "alex@example.com" } });
    fireEvent.click(screen.getByRole("button", { name: "Send me a link" }));

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toMatch(/couldn’t send your link/i);
    expect(screen.getByLabelText("Email")).toHaveProperty("value", "alex@example.com");
  });

  it("allows another send only after the 60-second wait", async () => {
    vi.useFakeTimers();
    render(<SignIn />);
    fireEvent.change(screen.getByLabelText("Email"), { target: { value: "alex@example.com" } });
    fireEvent.click(screen.getByRole("button", { name: "Send me a link" }));
    await act(async () => {
      await Promise.resolve();
    });

    const resend = screen.getByRole("button", { name: "Send again in 1:00" });
    expect(resend).toHaveProperty("disabled", true);
    await act(() => vi.advanceTimersByTime(59_000));
    expect(screen.getByRole("button", { name: "Send again in 0:01" })).toHaveProperty("disabled", true);
    await act(() => vi.advanceTimersByTime(1_000));
    const available = screen.getByRole("button", { name: "Send again" });
    expect(available).toHaveProperty("disabled", false);
    await act(async () => {
      fireEvent.click(available);
      await Promise.resolve();
    });
    expect(identity.sendMagicLink).toHaveBeenCalledTimes(2);
    expect(screen.getByText(/A new link is on its way to alex@example.com/)).toBeTruthy();
  });

  it("recognizes an expired sign-in link and clears the return fragment", async () => {
    window.history.replaceState({}, "", "/login#error=access_denied&error_code=otp_expired");
    render(<SignIn />);

    expect(await screen.findByRole("heading", { name: "This link has expired or was already used" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Get a new link" })).toBeTruthy();
    expect(window.location.hash).toBe("");
    fireEvent.click(screen.getByRole("button", { name: "Get a new link" }));
    expect(screen.getByRole("heading", { name: "Sign in to Forma" })).toBeTruthy();
  });

  it("shows the invited workspace step and signs out", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(jsonResponse({ invited: true, email: "alex@example.com" })));
    const onReady = vi.fn();
    render(<Onboard onReady={onReady} />);

    expect(await screen.findByRole("heading", { name: "One more step" })).toBeTruthy();
    expect(screen.getByText("Your workspace is home to your projects and designs. Nothing else to set up.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    await waitFor(() => expect(identity.signOut).toHaveBeenCalledOnce());
    expect(onReady).toHaveBeenCalledOnce();
  });

  it("offers sign out and a different email when there is no invitation", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(jsonResponse({ invited: false, email: "alex@example.com" })));
    render(<Onboard onReady={vi.fn()} />);

    expect(await screen.findByText(/no open invitation for this address/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Sign out" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Use a different email" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Create my workspace" })).toBeNull();
  });
});
