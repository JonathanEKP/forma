import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

/**
 * Connected applications, and disconnecting one (plan §112.2).
 *
 * The provider owns grants, so its API is mocked; what is worth proving is that
 * Forma keeps no second copy of who is connected, that a local deployment does
 * not draw a section for connections it cannot have, and that revoking says so
 * plainly when it fails rather than appearing to have worked.
 */
const provider = vi.hoisted(() => ({
  grants: [] as Array<{
    client: { id: string; name: string; uri: string; logo_uri: string };
    scopes: string[];
    granted_at: string;
  }>,
  revoked: [] as string[],
  revokeFails: false,
}));

vi.mock("./identity", () => ({
  supabase: {},
  accessToken: () => Promise.resolve("a-session"),
  currentSubject: () => Promise.resolve(null),
  onSessionChange: () => () => undefined,
  currentSession: () => Promise.resolve({ access_token: "a-session" }),
  sendMagicLink: () => Promise.resolve(),
  signOut: () => Promise.resolve(),
  listGrants: () => Promise.resolve(provider.grants),
  revokeGrant: (clientId: string) => {
    if (provider.revokeFails) return Promise.reject(new Error("the provider refused"));
    provider.revoked.push(clientId);
    provider.grants = provider.grants.filter((g) => g.client.id !== clientId);
    return Promise.resolve();
  },
}));

function server(identityRequired: boolean) {
  return vi.fn((input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const body = url.includes("/api/v1/identity")
      ? { required: identityRequired }
      : url.includes("/api/v1/api-keys")
        ? []
        : { error: { code: "NOT_FOUND", message: url } };
    return Promise.resolve(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
  });
}

function renderAgents() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe("connected applications", () => {
  beforeEach(() => {
    window.history.pushState({}, "", "/agents");
    provider.grants = [
      {
        client: { id: "client-claude", name: "Claude", uri: "https://claude.ai", logo_uri: "" },
        scopes: ["openid", "email"],
        granted_at: "2026-08-31T10:00:00.000Z",
      },
    ];
    provider.revoked = [];
    provider.revokeFails = false;
  });

  afterEach(() => {
    window.history.pushState({}, "", "/");
    vi.unstubAllGlobals();
  });

  it("lists what the provider says is connected", async () => {
    vi.stubGlobal("fetch", server(true));
    renderAgents();

    // Scoped to the section: "Claude" also names a host in the setup guidance
    // above, and a bare text match would pass on the wrong element.
    await waitFor(() => expect(screen.getByText(/connected 2026-08-31/)).toBeTruthy());
    expect(screen.getByText("Disconnect")).toBeTruthy();
  });

  it("disconnects by the client the provider named", async () => {
    vi.stubGlobal("fetch", server(true));
    renderAgents();

    await waitFor(() => expect(screen.getByText("Disconnect")).toBeTruthy());
    fireEvent.click(screen.getByText("Disconnect"));

    await waitFor(() => expect(provider.revoked).toEqual(["client-claude"]));
    await waitFor(() => expect(screen.getByText(/No applications connected/)).toBeTruthy());
  });

  it("says when disconnecting failed rather than appearing to have worked", async () => {
    provider.revokeFails = true;
    vi.stubGlobal("fetch", server(true));
    renderAgents();

    await waitFor(() => expect(screen.getByText("Disconnect")).toBeTruthy());
    fireEvent.click(screen.getByText("Disconnect"));

    await waitFor(() => expect(screen.getByText(/the provider refused/)).toBeTruthy());
    // Still listed, because it is still connected.
    expect(screen.getByText(/connected 2026-08-31/)).toBeTruthy();
    expect(provider.revoked).toEqual([]);
  });

  it("draws no connections section on a Forma that has no provider", async () => {
    vi.stubGlobal("fetch", server(false));
    renderAgents();

    // The page itself is reachable; the section for something impossible is not
    // drawn empty, which would read as a connection that failed.
    await waitFor(() => expect(screen.getByText(/Agent connections/)).toBeTruthy());
    expect(screen.queryByText(/Connected applications/)).toBeNull();
  });
});
