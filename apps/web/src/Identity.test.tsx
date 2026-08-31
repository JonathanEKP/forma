import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

/**
 * Which first screen a person meets (plan §110.2 and §107.3).
 *
 * The provider is mocked at `./identity` rather than at `fetch`: everything
 * below that boundary is the library's, and asserting on its network calls
 * would be testing supabase-js rather than Forma.
 */
const session = vi.hoisted(() => ({ current: null as { access_token: string } | null }));

vi.mock("./identity", () => ({
  supabase: {},
  accessToken: () => Promise.resolve(session.current?.access_token ?? null),
  currentSession: () => Promise.resolve(session.current),
  sendMagicLink: () => Promise.resolve(),
  signOut: () => Promise.resolve(),
}));

function server(routes: Record<string, { status: number; body: unknown }>) {
  return vi.fn((input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const match = Object.keys(routes).find((key) => url.includes(key));
    const answer = match
      ? routes[match]
      : { status: 404, body: { error: { code: "NOT_FOUND", message: url } } };
    return Promise.resolve(
      new Response(JSON.stringify(answer.body), {
        status: answer.status,
        headers: { "content-type": "application/json" },
      }),
    );
  });
}

function renderApp() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe("the first screen", () => {
  beforeEach(() => {
    session.current = null;
    window.history.pushState({}, "", "/");
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("goes straight to the workspace when this Forma has no identity", async () => {
    vi.stubGlobal(
      "fetch",
      server({
        "/api/v1/identity": { status: 200, body: { required: false } },
        "/api/v1/status": {
          status: 200,
          body: {
            name: "forma-server",
            version: "0",
            schema_version: 1,
            database: "sqlite",
            fonts: [],
            projects: 0,
            max_ops_per_batch: 20,
          },
        },
        "/api/v1/projects": { status: 200, body: [] },
      }),
    );

    renderApp();

    // No sign-in screen is ever shown for a deployment that has nothing to sign
    // in to, not even for a moment.
    await waitFor(() => expect(screen.getByText(/Projects/i)).toBeTruthy());
    expect(screen.queryByText(/Sign in to Forma/i)).toBeNull();
  });

  it("asks a hosted visitor to sign in", async () => {
    vi.stubGlobal(
      "fetch",
      server({ "/api/v1/identity": { status: 200, body: { required: true } } }),
    );

    renderApp();
    await waitFor(() => expect(screen.getByText(/Sign in to Forma/i)).toBeTruthy());
  });

  it("offers the two doors to a session that owns no account", async () => {
    session.current = { access_token: "a-session" };
    vi.stubGlobal(
      "fetch",
      server({
        "/api/v1/identity": { status: 200, body: { required: true } },
        "/api/v1/projects": {
          status: 403,
          body: { error: { code: "ACCOUNT_REQUIRED", message: "no account yet" } },
        },
        "/api/v1/onboarding": {
          status: 200,
          body: { invited: true, claimable: true, email: "invited@example.com" },
        },
      }),
    );

    renderApp();

    // Wait on the doors themselves, not on the frame around them: the frame
    // renders before the second request that decides what goes in it, so
    // waiting on the title is waiting on the wrong condition.
    await waitFor(() => expect(screen.getByText(/Create my workspace/i)).toBeTruthy());
    expect(screen.getByText(/One more step/i)).toBeTruthy();
    expect(screen.getByText(/Claim existing workspace/i)).toBeTruthy();
    expect(screen.getByText(/invited@example.com/)).toBeTruthy();
  });

  it("says so plainly when there is no invitation, rather than looking broken", async () => {
    session.current = { access_token: "a-session" };
    vi.stubGlobal(
      "fetch",
      server({
        "/api/v1/identity": { status: 200, body: { required: true } },
        "/api/v1/projects": {
          status: 403,
          body: { error: { code: "ACCOUNT_REQUIRED", message: "no account yet" } },
        },
        "/api/v1/onboarding": {
          status: 200,
          body: { invited: false, claimable: false, email: "stranger@example.com" },
        },
      }),
    );

    renderApp();

    await waitFor(() => expect(screen.getByText(/invite-only/i)).toBeTruthy());
    expect(screen.queryByText(/Create my workspace/i)).toBeNull();
    // Nothing to claim, so that door is not offered either.
    expect(screen.queryByText(/Claim existing workspace/i)).toBeNull();
  });
});
