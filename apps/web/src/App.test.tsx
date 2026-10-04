import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

/**
 * A Forma with no identity provider, which is what a local build is and what CI
 * builds — `apps/web/.env` is gitignored, so `VITE_SUPABASE_*` is absent there.
 *
 * Declared rather than inherited. Without this the file's behaviour depends on
 * whether the machine running it happens to have that file, which is how a test
 * passes for one person and fails for everybody else.
 */
vi.mock("./identity", () => ({
  supabase: null,
  accessToken: () => Promise.resolve(null),
  currentSubject: () => Promise.resolve(null),
  onSessionChange: () => () => undefined,
  currentSession: () => Promise.resolve(null),
  sendMagicLink: () => Promise.resolve(),
  signOut: () => Promise.resolve(),
  listGrants: () => Promise.resolve([]),
  revokeGrant: () => Promise.resolve(),
  authorizationDetails: () => Promise.reject(new Error("no provider")),
  decide: () => Promise.reject(new Error("no provider")),
}));

import { ApiError, conflictDetails } from "./api";

/**
 * The editor is exercised through `fetch`, because that is the only thing it
 * shares with the server. Mocking our own client instead would prove the tests
 * agree with themselves.
 */
function mockFetch(routes: Record<string, unknown>) {
  return vi.fn((input: RequestInfo | URL) => {
    // `Request` has no useful toString, so each shape is read explicitly.
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;

    const match = Object.keys(routes).find((key) => url.includes(key));

    if (match === undefined) {
      return Promise.resolve(
        new Response(
          JSON.stringify({ error: { code: "NOT_FOUND", message: url } }),
          { status: 404, headers: { "content-type": "application/json" } },
        ),
      );
    }

    return Promise.resolve(
      new Response(JSON.stringify(routes[match]), {
        status: 200,
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

const status = {
  name: "forma-server",
  version: "0.1.0",
  schema_version: 1,
  database: "sqlite",
  fonts: ["Geist"],
  projects: 1,
  max_ops_per_batch: 20,
};

describe("the project view", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", mockFetch({}));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("says the server is unreachable rather than showing an empty shell", async () => {
    renderApp();

    // A local tool that silently shows nothing is worse than one that says the
    // server is down: there is exactly one thing to start.
    expect(await screen.findByText(/forma-server is not reachable/i)).toBeTruthy();
  });
});

describe("public entry points", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", mockFetch({ "/api-keys": [] }));
  });

  afterEach(() => {
    window.history.replaceState({}, "", "/");
    vi.unstubAllGlobals();
  });

  it.each([
    ["/login", "Sign in to Forma"],
    ["/agents", "Agent connections"],
  ])("renders %s without starting the editor", (path, heading) => {
    window.history.replaceState({}, "", path);
    renderApp();

    expect(screen.getByRole("heading", { name: heading })).toBeTruthy();
  });

  /// The consent page used to be a placeholder with a fixed heading. It is a
  /// real page now, and on a Forma with no identity provider — a local one, and
  /// the one CI builds — an authorization cannot mean anything, so it says so
  /// rather than offering a sign-in that could never complete.
  ///
  /// The provider-configured paths are covered in `Consent.test.tsx`, which
  /// mocks one.
  it("says an authorization means nothing where there is no provider", async () => {
    vi.stubGlobal("fetch", mockFetch({ "/api/v1/identity": { required: false } }));
    window.history.replaceState({}, "", "/oauth/consent?authorization_id=request-1");
    renderApp();

    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: "This authorization cannot be shown" }),
      ).toBeTruthy(),
    );
  });
});

describe("the project view with a live server", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      mockFetch({
        "/status": status,
        "/projects": [
          { id: "p1", name: "Launch campaign", created_at: "2026-08-28T00:00:00.000Z", updated_at: "2026-08-28T00:00:00.000Z" },
        ],
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("lists projects and reports what the renderer can draw", async () => {
    renderApp();

    expect(await screen.findByText("Launch campaign")).toBeTruthy();
    // The font catalog is on screen because an agent or a user must not name a
    // family the renderer does not have.
    await waitFor(() => expect(screen.getByText(/fonts: Geist/)).toBeTruthy());
  });

  it("keeps duplicate brand colors from creating duplicate React keys", async () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.stubGlobal(
      "fetch",
      mockFetch({
        "/status": status,
        "/projects/p1/brand": {
          id: "b1",
          project_id: "p1",
          name: "Acme",
          colors: {
            primary: "#171717",
            secondary: "#F59E0B",
            accent: "#7C3AED",
            background: "#0B0B0C",
            surface: "#171717",
            text: "#171717",
            text_inverse: "#FFFFFF",
            muted: "#A1A1AA",
          },
          typography: { display: "Geist", body: "Geist", mono: "Geist" },
          logo_asset_id: null,
          voice: "Direct and brief.",
          rules: { safe_margin: 80, minimum_font_size: 24 },
          version: 1,
        },
        "/projects/p1/designs": [],
        "/projects": [
          { id: "p1", name: "Launch campaign", created_at: "2026-08-28T00:00:00.000Z", updated_at: "2026-08-28T00:00:00.000Z" },
        ],
      }),
    );

    try {
      renderApp();
      fireEvent.click(await screen.findByText("Launch campaign"));
      expect(await screen.findByText("Acme")).toBeTruthy();
      expect(errors.mock.calls.some(([message]) => String(message).includes("same key"))).toBe(false);
    } finally {
      errors.mockRestore();
    }
  });
});

describe("conflict details", () => {
  const conflict = (details: unknown) =>
    new ApiError(
      { code: "VERSION_CONFLICT", message: "changed", details },
      409,
    );

  it("reads both versions so the UI can say what happened", () => {
    expect(conflictDetails(conflict({ expected: 4, actual: 7 }))).toEqual({
      expected: 4,
      actual: 7,
    });
  });

  it("returns null rather than guessing when the shape is not what we expect", () => {
    // The banner degrades to a generic message instead of rendering undefined.
    for (const details of [null, undefined, {}, { expected: "4" }, "nope"]) {
      expect(conflictDetails(conflict(details))).toBeNull();
    }
  });

  it("knows a conflict from a rejection", () => {
    expect(conflict({}).isConflict).toBe(true);
    expect(
      new ApiError({ code: "INVALID_OP", message: "no" }, 400).isConflict,
    ).toBe(false);
  });
});
