import { fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  decided: null as { id: string; allow: boolean } | null,
  redirect: "https://client.example/callback?code=abc&state=xyz",
}));

vi.mock("./identity", () => ({
  supabase: {},
  currentSession: () => Promise.resolve(provider.session),
  authorizationDetails: () => Promise.resolve(provider.details),
  decide: (id: string, allow: boolean) => {
    provider.decided = { id, allow };
    return Promise.resolve(provider.redirect);
  },
  accessToken: () => Promise.resolve(null),
  sendMagicLink: () => Promise.resolve(),
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
    provider.decided = null;
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

    expect(screen.getByText(/cannot access projects belonging to another Forma account/i)).toBeTruthy();
    // The scopes are the provider's, and must not be dressed up as Forma
    // permissions. Naming them here would say the opposite.
    expect(screen.queryByText(/openid/i)).toBeNull();
    expect(screen.getByText("https://client.example/callback")).toBeTruthy();
  });

  it("sends the person exactly where the provider says, on Allow", async () => {
    render(<Consent />);
    await waitFor(() => expect(screen.getByText(/Allow/)).toBeTruthy());
    fireEvent.click(screen.getByText(/Allow/));

    await waitFor(() => expect(provider.decided).toEqual({ id: "auth-1", allow: true }));
    expect(assign).toHaveBeenCalledWith(provider.redirect);
  });

  it("and on Deny, which the client is entitled to be told about", async () => {
    render(<Consent />);
    await waitFor(() => expect(screen.getByText(/Deny/)).toBeTruthy());
    fireEvent.click(screen.getByText(/Deny/));

    await waitFor(() => expect(provider.decided).toEqual({ id: "auth-1", allow: false }));
    expect(assign).toHaveBeenCalledWith(provider.redirect);
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
    // The flow the provider started must survive the detour; returning to the
    // site root would lose the authorization entirely.
    expect(provider.decided).toBeNull();
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
