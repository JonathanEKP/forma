import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Consent } from "./Consent";
import type { ConsentDetails } from "./identity";

/**
 * The consent page's decisions, which are ours; the provider's API is mocked
 * because asserting on its network calls would be testing supabase-js.
 *
 * What is worth proving here is that the page never composes a redirect of its
 * own, never asks a question that was already answered, and never loses the
 * authorization when someone has to sign in first.
 */
type Details = ConsentDetails;

const provider = vi.hoisted(() => ({
  session: null as { access_token: string } | null,
  details: null as
    | { kind: "decide"; details: Details }
    | { kind: "already"; redirectUrl: string }
    | null,
  authorizationFailure: null as Error | null,
  decided: null as { id: string; allow: boolean } | null,
  redirect: "https://client.example/callback?code=abc&state=xyz",
  decisionPromise: null as Promise<string> | null,
  sentMagicLink: null as { email: string; returnTo?: string } | null,
}));

vi.mock("./identity", () => ({
  supabase: {},
  currentSession: () => Promise.resolve(provider.session),
  authorizationDetails: () =>
    provider.authorizationFailure
      ? Promise.reject(provider.authorizationFailure)
      : Promise.resolve(provider.details),
  decide: (id: string, allow: boolean) => {
    provider.decided = { id, allow };
    return provider.decisionPromise ?? Promise.resolve(provider.redirect);
  },
  accessToken: () => Promise.resolve(null),
  sendMagicLink: (email: string, returnTo?: string) => {
    provider.sentMagicLink = { email, returnTo };
    return Promise.resolve();
  },
  signOut: () => Promise.resolve(),
}));

const consenting = {
  kind: "decide" as const,
  details: {
    authorization_id: "auth-1",
    redirect_uri: "https://client.example/callback",
    client: { id: "c1", name: "Claude", uri: "https://claude.ai", logo_uri: "" },
    user: { id: "u1", email: "someone@example.com" },
    scope: "openid email",
  },
};

describe("the consent page", () => {
  let assign: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    // The server is the authority on whether this deployment has identity, so
    // the page asks it before anything else.
    vi.stubGlobal("fetch", () =>
      Promise.resolve(
        new Response(JSON.stringify({ required: true }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      ),
    );
    provider.session = { access_token: "a-session" };
    provider.details = consenting;
    provider.authorizationFailure = null;
    provider.decided = null;
    provider.decisionPromise = null;
    provider.sentMagicLink = null;
    window.history.pushState({}, "", "/oauth/consent?authorization_id=auth-1");
    assign = vi.fn();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...window.location, assign, search: "?authorization_id=auth-1", href: "http://localhost/oauth/consent?authorization_id=auth-1" },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("names the application and says what the connection can reach", async () => {
    render(<Consent />);
    await waitFor(() => expect(screen.getByText(/Claude wants to use Forma/i)).toBeTruthy());

    expect(screen.getByText(/access anyone else’s Forma account/i)).toBeTruthy();
    expect(screen.getByText(/all projects in your account/)).toBeTruthy();
    expect(screen.getByRole("heading", { name: "What this app can do" })).toBeTruthy();
    expect(screen.getByText("See your projects, Brand Kits and images")).toBeTruthy();
    expect(screen.getByText("Create and edit designs")).toBeTruthy();
    expect(screen.getByText("Make previews and exports")).toBeTruthy();
    expect(screen.getByText("Ask for images it needs")).toBeTruthy();
    expect(screen.getByText(/cannot create or delete projects, change a Brand Kit/i)).toBeTruthy();
    expect(screen.getByText(/The app supplies its name and website; Forma does not verify them/i)).toBeTruthy();
    expect(screen.getByRole("link", { name: /AI apps \(MCP\)/i }).getAttribute("href")).toBe("/agents/mcp");
    // The scopes are the provider's, and must not be dressed up as Forma
    // permissions. Naming them here would say the opposite.
    expect(screen.queryByText(/openid/i)).toBeNull();
    expect(screen.getByText("https://client.example/callback")).toBeTruthy();
    expect(screen.getByText("https://client.example/callback").className).not.toContain("truncate");
  });

  it("shows a sending state before returning to the exact provider URL on Allow", async () => {
    let resolveDecision: ((url: string) => void) | undefined;
    provider.decisionPromise = new Promise((resolve) => {
      resolveDecision = resolve;
    });
    render(<Consent />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Allow" })).toBeTruthy());
    const timeout = vi.spyOn(window, "setTimeout");
    fireEvent.click(screen.getByRole("button", { name: "Allow" }));
    expect(screen.getByRole("button", { name: "Sending…" })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: "Deny" })).toHaveProperty("disabled", true);
    expect(screen.getByText("Sending your answer…")).toBeTruthy();

    await act(async () => {
      resolveDecision?.(provider.redirect);
      await provider.decisionPromise;
    });
    expect(provider.decided).toEqual({ id: "auth-1", allow: true });
    expect(assign).toHaveBeenCalledWith(provider.redirect);
    expect(timeout).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: "Connection allowed" })).toBeTruthy();
    expect(screen.getByText("Your browser is going back to the app to finish connecting.")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Go back to Claude" }).getAttribute("href")).toBe(provider.redirect);
    expect(screen.getByText(/To disconnect later/)).toBeTruthy();
  });

  it("and on Deny, which the client is entitled to be told about", async () => {
    render(<Consent />);
    await waitFor(() => expect(screen.getByText(/Deny/)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: "Deny" }));

    await waitFor(() => expect(provider.decided).toEqual({ id: "auth-1", allow: false }));
    expect(assign).toHaveBeenCalledWith(provider.redirect);
    expect(screen.getByRole("heading", { name: "Connection denied" })).toBeTruthy();
    expect(screen.getByText("Claude was not connected.")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Go back to Claude" }).getAttribute("href")).toBe(provider.redirect);
  });

  it("does not ask a question that was already answered", async () => {
    provider.details = { kind: "already", redirectUrl: provider.redirect };
    render(<Consent />);

    await waitFor(() => expect(assign).toHaveBeenCalledWith(provider.redirect));
    expect(screen.queryByText(/Allow/)).toBeNull();
  });

  it("comes back to this exact authorization after signing in", async () => {
    provider.session = null;
    render(<Consent />);

    await waitFor(() => expect(screen.getByText(/Sign in to Forma/i)).toBeTruthy());
    expect(screen.getByText(/An AI app wants to connect to Forma/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Email"), { target: { value: "alex@example.com" } });
    fireEvent.click(screen.getByRole("button", { name: "Send me a link" }));
    await waitFor(() => expect(provider.sentMagicLink?.returnTo).toBe("http://localhost/oauth/consent?authorization_id=auth-1"));
    // The flow the provider started must survive the detour; returning to the
    // site root would lose the authorization entirely.
    expect(provider.decided).toBeNull();
  });

  it("shows the local-address note and a firm warning when the app has no website", async () => {
    provider.details = {
      ...consenting,
      details: {
        ...consenting.details,
        client: { ...consenting.details.client, name: "Claude Code", uri: "" },
        redirect_uri: "http://localhost:33418/callback",
      },
    };
    render(<Consent />);

    await waitFor(() => expect(screen.getByText("Claude Code wants to use Forma")).toBeTruthy());
    expect(screen.queryByText("Website")).toBeNull();
    expect(screen.getByText("http://localhost:33418/callback")).toBeTruthy();
    expect(screen.getByText(/A local address means the app runs on this computer\./)).toBeTruthy();
    expect(screen.getByText(/Forma does not verify it\./)).toBeTruthy();
    expect(screen.getByText(/Continue only if it belongs to the app you meant to connect\./)).toBeTruthy();
  });

  it("renders the unavailable state when the authorization is missing or expired", async () => {
    provider.authorizationFailure = new Error("authorization expired");
    render(<Consent />);

    expect(await screen.findByRole("heading", { name: "This connection cannot be shown" })).toBeTruthy();
    expect(screen.getByText(/connection link is missing or has expired/i)).toBeTruthy();
    expect(screen.getByText(/Start the connection again from the application that sent you here/i)).toBeTruthy();
  });

  it("refuses a link with no authorization rather than guessing one", async () => {
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...window.location, assign, search: "", href: "http://localhost/oauth/consent" },
    });

    render(<Consent />);
    await waitFor(() => expect(screen.getByText(/cannot be shown/i)).toBeTruthy());
    expect(assign).not.toHaveBeenCalled();
  });
});
