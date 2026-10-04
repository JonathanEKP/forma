import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";

/**
 * The device panel on the Agents page (plan §11 step 10a).
 *
 * `fetch` is the only fake: the page, the gate that decides whether the panel
 * exists and the client under it all run for real. What is worth proving is the
 * gate (a local Forma shows nothing Pontis-related), that an unreachable sidecar
 * costs the page only its panel (§7.3), and the wording ADR-019 and docs/13 fix.
 */
const identity = vi.hoisted(() => ({ hosted: true }));

vi.mock("../identity", () => ({
  get supabase() {
    return identity.hosted ? {} : null;
  },
  accessToken: () => Promise.resolve(identity.hosted ? "a-session" : null),
  currentSubject: () => Promise.resolve(null),
  onSessionChange: () => () => undefined,
  currentSession: () => Promise.resolve(identity.hosted ? { access_token: "a-session" } : null),
  sendMagicLink: () => Promise.resolve(),
  signOut: () => Promise.resolve(),
  listGrants: () => Promise.resolve([]),
  revokeGrant: () => Promise.resolve(),
  authorizationDetails: () => Promise.reject(new Error("no provider")),
  decide: () => Promise.reject(new Error("no provider")),
}));

const CODEX = {
  providerId: "codex",
  installed: true,
  version: "0.156.1",
  compatible: true,
  authenticated: true,
  authMode: "api_key",
  securityProfile: {
    classification: "secure_default",
    executionBoundary: "supervised_child_process",
    environmentBoundary: "allowlisted",
    sandboxBoundary: "provider_managed",
  },
};

const ACP = {
  ...CODEX,
  providerId: "acp-tool",
  version: null,
  authMode: "subscription",
  securityProfile: { ...CODEX.securityProfile, classification: "explicit_opt_in" },
};

interface DeviceRecord {
  id: string;
  name: string;
  platform: string;
  runtimeVersion: string;
  pairedAt: string;
  lastSeenAt: string | null;
  revokedAt: string | null;
  online: boolean | undefined;
  capabilities: unknown[] | null;
}

function record(over: Partial<DeviceRecord> = {}): DeviceRecord {
  return {
    id: "dev_1",
    name: "Studio laptop",
    platform: "linux",
    runtimeVersion: "1.2.3",
    pairedAt: "2026-10-01T10:00:00.000Z",
    lastSeenAt: "2026-10-03T09:00:00.000Z",
    revokedAt: null,
    online: true,
    capabilities: [CODEX, ACP],
    ...over,
  };
}

let devices: DeviceRecord[];
let sidecarUp: boolean;
let accountless: boolean;
let requests: Array<{ url: string; method: string }>;

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

function route(url: string, method: string): Response {
  if (url.endsWith("/api/v1/identity")) return json({ required: identity.hosted });
  if (url.endsWith("/api/v1/projects")) {
    return accountless ? json({ error: { code: "ACCOUNT_REQUIRED", message: "no account" } }, 403) : json([]);
  }
  if (url.endsWith("/api/v1/api-keys")) return json([]);
  if (url.startsWith("/pons/")) {
    if (!sidecarUp) return new Response("Bad Gateway", { status: 502 });
    if (url === "/pons/v1/devices") return json({ protocol: 1, devices });
    if (url === "/pons/v1/pairing/codes" && method === "POST") {
      return json({
        protocol: 1,
        code: "ABCD-EFGH-JKLM",
        expiresAt: "2026-10-03T10:10:00.000Z",
        controlPlaneUrl: "https://forma.example",
        applicationId: "forma",
      });
    }
    const revoke = /^\/pons\/v1\/devices\/([^/]+)\/revoke$/.exec(url);
    if (revoke && method === "POST") {
      devices = devices.map((d) => (d.id === revoke[1] ? { ...d, revokedAt: "2026-10-03T11:00:00.000Z" } : d));
      return json({ protocol: 1, device: devices.find((d) => d.id === revoke[1]) });
    }
  }
  return json({ error: { code: "NOT_FOUND", message: url } }, 404);
}

function renderAgents() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe("the device panel on the Agents page", () => {
  beforeEach(() => {
    devices = [record()];
    sidecarUp = true;
    accountless = false;
    requests = [];
    identity.hosted = true;
    window.history.replaceState({}, "", "/agents");
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input instanceof URL ? input.pathname : input.url;
        const method = init?.method ?? "GET";
        requests.push({ url, method });
        return Promise.resolve(route(url, method));
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows a paired device with its runtime, providers, auth mode and security profile", async () => {
    renderAgents();

    const card = await screen.findByRole("article", { name: "Studio laptop" });
    expect(within(card).getByText(/runtime 1\.2\.3/)).toBeTruthy();
    expect(within(card).getByText(/online/)).toBeTruthy();
    expect(within(card).getByText(/last seen 2026-10-03 09:00 UTC/)).toBeTruthy();
    expect(within(card).getByText("codex")).toBeTruthy();
    expect(within(card).getByText("0.156.1")).toBeTruthy();
    expect(within(card).getByText(/api_key — usage is billed directly/)).toBeTruthy();
    expect(within(card).getAllByText(/execution supervised_child_process/)).toHaveLength(2);
  });

  it("calls a provider_managed sandbox exactly that and keeps the classifications distinct", async () => {
    renderAgents();

    const card = await screen.findByRole("article", { name: "Studio laptop" });
    expect(within(card).getAllByText("provider_managed")).toHaveLength(2);
    expect(within(card).getAllByText(/Pontis makes no claim about its strength/).length).toBeGreaterThan(0);
    expect(within(card).getByText("secure_default", { selector: "span[title]" })).toBeTruthy();
    expect(within(card).getByText("explicit_opt_in", { selector: "span[title]" })).toBeTruthy();
    expect(card.textContent ?? "").not.toMatch(/runtime[-_ ]enforced/);
  });

  it("does not call a device offline when the response says nothing about presence", async () => {
    devices = [record({ online: undefined })];
    renderAgents();

    const card = await screen.findByRole("article", { name: "Studio laptop" });
    expect(within(card).getByText(/presence not reported/)).toBeTruthy();
    expect(within(card).queryByText(/offline/)).toBeNull();
  });

  it("tells a device that never reported apart from one with no provider", async () => {
    devices = [record({ id: "dev_1", capabilities: null }), record({ id: "dev_2", name: "Bare box", capabilities: [] })];
    renderAgents();

    expect(await screen.findByText(/has not reported its providers yet/)).toBeTruthy();
    expect(screen.getByText(/found no provider installed/)).toBeTruthy();
  });

  it("revokes only after a confirmation, then shows the device revoked", async () => {
    renderAgents();

    fireEvent.click(await screen.findByRole("button", { name: "Revoke Studio laptop" }));
    expect(requests.some((r) => r.url.endsWith("/revoke"))).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Confirm revoke" }));

    expect(await screen.findByText(/Revoked 2026-10-03 11:00 UTC/)).toBeTruthy();
    expect(requests).toContainEqual({ url: "/pons/v1/devices/dev_1/revoke", method: "POST" });
    expect(screen.queryByRole("button", { name: "Revoke Studio laptop" })).toBeNull();
  });

  it("creates a pairing code and shows the command to run", async () => {
    renderAgents();

    fireEvent.click(await screen.findByRole("button", { name: "Create pairing code" }));

    expect(await screen.findByText("ABCD-EFGH-JKLM")).toBeTruthy();
    expect(
      screen.getByText('pontis pair --origin https://forma.example --application forma --name "My laptop" ABCD-EFGH-JKLM'),
    ).toBeTruthy();
    expect(requests).toContainEqual({ url: "/pons/v1/pairing/codes", method: "POST" });
  });

  it("says Pontis is unavailable and leaves the rest of the page working", async () => {
    sidecarUp = false;
    renderAgents();

    expect(await screen.findByText(/Pontis is unavailable/)).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Personal access tokens" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Use Forma with your favourite AI" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Create pairing code" })).toBeNull();
  });

  it("draws nothing Pontis-related on a local Forma, and does not ask the sidecar", async () => {
    identity.hosted = false;
    renderAgents();

    expect(await screen.findByRole("heading", { name: "Personal access tokens" })).toBeTruthy();
    await waitFor(() => expect(requests.some((r) => r.url.endsWith("/api/v1/identity"))).toBe(true));
    expect(screen.queryByRole("heading", { name: "Devices" })).toBeNull();
    expect(requests.filter((r) => r.url.startsWith("/pons/"))).toEqual([]);
  });

  it("draws no panel for a signed-in person without an account", async () => {
    accountless = true;
    renderAgents();

    expect(await screen.findByRole("heading", { name: "Personal access tokens" })).toBeTruthy();
    await waitFor(() => expect(requests.some((r) => r.url.endsWith("/api/v1/projects"))).toBe(true));
    expect(screen.queryByRole("heading", { name: "Devices" })).toBeNull();
    expect(requests.filter((r) => r.url.startsWith("/pons/"))).toEqual([]);
  });
});
