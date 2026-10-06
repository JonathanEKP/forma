import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import { liveFeed } from "./channel";

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
let signInExpired: boolean;
let accountless: boolean;
let requests: Array<{ url: string; method: string }>;
let pairingCreatedAt: number | null;

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
    if (url === "/pons/v1/devices" && signInExpired) return json({ error: { code: "unauthenticated" } }, 401);
    if (url === "/pons/v1/devices") return json({ protocol: 1, devices });
    if (url === "/pons/v1/pairing/codes" && method === "POST") {
      pairingCreatedAt = Date.now();
      return json({
        protocol: 1,
        code: "ABCD-EFGH-JKLM",
        expiresAt: new Date(pairingCreatedAt + 10 * 60_000).toISOString(),
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
    signInExpired = false;
    accountless = false;
    requests = [];
    pairingCreatedAt = null;
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

  it("refreshes paired devices and presence from live device frames", async () => {
    renderAgents();
    await screen.findByRole("article", { name: "Studio laptop" });

    devices = [...devices, record({ id: "dev_2", name: "Office workstation" })];
    act(() => liveFeed.publish({ type: "device.online", payload: {} }));
    expect(await screen.findByRole("article", { name: "Office workstation" })).toBeTruthy();

    devices = devices.map((device) => (device.id === "dev_1" ? { ...device, online: false } : device));
    act(() => liveFeed.publish({ type: "device.offline", payload: {} }));
    expect(await within(screen.getByRole("article", { name: "Studio laptop" })).findByText("offline")).toBeTruthy();
  });

  it("refreshes on its slow fallback poll when no presence frame arrives", async () => {
    let poll: (() => void) | undefined;
    const nativeSetInterval = globalThis.setInterval;
    const interval = vi.spyOn(globalThis, "setInterval").mockImplementation((handler, timeout) => {
      if (timeout === 30_000 && typeof handler === "function") poll = () => handler();
      return nativeSetInterval(handler, timeout);
    });

    try {
      renderAgents();
      await screen.findByRole("article", { name: "Studio laptop" });

      devices = [...devices, record({ id: "dev_2", name: "Office workstation" })];
      act(() => poll?.());
      expect(await screen.findByRole("article", { name: "Office workstation" })).toBeTruthy();
    } finally {
      interval.mockRestore();
    }
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

    fireEvent.click(await screen.findByRole("button", { name: "Revoke Studio laptop, paired October 1, 2026" }));
    expect(requests.some((r) => r.url.endsWith("/revoke"))).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Confirm revoke Studio laptop, paired October 1, 2026" }));

    expect(await screen.findByText(/Revoked 2026-10-03 11:00 UTC/)).toBeTruthy();
    expect(requests).toContainEqual({ url: "/pons/v1/devices/dev_1/revoke", method: "POST" });
    expect(screen.queryByRole("button", { name: "Revoke Studio laptop, paired October 1, 2026" })).toBeNull();
  });

  it("creates a named pairing code and shows the copy-safe command", async () => {
    devices = [];
    renderAgents();

    fireEvent.click(await screen.findByRole("button", { name: "Pair a computer" }));
    fireEvent.change(screen.getByLabelText("Computer name"), { target: { value: "Studio laptop" } });
    fireEvent.click(screen.getByRole("button", { name: "Create pairing code for the named computer" }));

    expect(await screen.findByText("ABCD-EFGH-JKLM")).toBeTruthy();
    expect(
      screen.getByText("pontis pair --origin 'https://forma.example' --application 'forma' --name 'Studio laptop' 'ABCD-EFGH-JKLM'"),
    ).toBeTruthy();
    expect(screen.getByText("Waiting for the pairing…")).toBeTruthy();
    expect(requests).toContainEqual({ url: "/pons/v1/pairing/codes", method: "POST" });
  });

  it("tracks pairing, folds the steps into the list, then reports Online", async () => {
    devices = [];
    renderAgents();

    fireEvent.click(await screen.findByRole("button", { name: "Pair a computer" }));
    fireEvent.change(screen.getByLabelText("Computer name"), { target: { value: "Office Mac" } });
    fireEvent.click(screen.getByRole("button", { name: "Create pairing code for the named computer" }));
    expect(await screen.findByText("Waiting for the pairing…")).toBeTruthy();

    devices = [record({
      id: "dev_new",
      name: "Office Mac",
      pairedAt: new Date((pairingCreatedAt ?? Date.now()) + 1).toISOString(),
      online: false,
      capabilities: [],
    })];
    act(() => liveFeed.publish({ type: "device.paired", payload: {} }));

    const device = await screen.findByRole("article", { name: "Office Mac, paired October 6, 2026" });
    expect(within(device).getByText("Paired. Waiting for it to start…")).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Install Pontis on that computer." })).toBeNull();
    expect(within(device).getByText("pontis run")).toBeTruthy();

    devices = devices.map((entry) => ({ ...entry, online: true }));
    act(() => liveFeed.publish({ type: "device.online", payload: {} }));
    expect(await within(device).findByText("Online")).toBeTruthy();
  });

  it("ticks the relative expiry and offers a new code after expiration", async () => {
    vi.setSystemTime(new Date("2026-10-06T12:00:00.000Z"));
    devices = [];
    const interval = vi.spyOn(globalThis, "setInterval");
    renderAgents();

    fireEvent.click(await screen.findByRole("button", { name: "Pair a computer" }));
    fireEvent.change(screen.getByLabelText("Computer name"), { target: { value: "Studio" } });
    fireEvent.click(screen.getByRole("button", { name: "Create pairing code for the named computer" }));
    expect(await screen.findByText("Expires in 10:00")).toBeTruthy();

    const ticker = interval.mock.calls.find((call) => call[1] === 1_000)?.[0];
    expect(typeof ticker).toBe("function");
    vi.setSystemTime(new Date("2026-10-06T12:00:01.000Z"));
    act(() => {
      if (typeof ticker === "function") ticker();
    });
    expect(screen.getByText("Expires in 9:59")).toBeTruthy();

    vi.setSystemTime(new Date("2026-10-06T12:10:00.000Z"));
    act(() => {
      if (typeof ticker === "function") ticker();
    });
    expect(screen.getByText("Expired")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create a new pairing code" })).toBeTruthy();

    interval.mockRestore();
    vi.setSystemTime(new Date());
  });

  it("sorts online computers first, groups revoked devices closed, and expands provider details", async () => {
    devices = [
      record({ id: "offline", name: "Offline workstation", online: false, pairedAt: "2026-10-02T10:00:00.000Z" }),
      record({ id: "live", name: "Live workstation", online: true, pairedAt: "2026-10-01T10:00:00.000Z" }),
      record({ id: "revoked", name: "Old workstation", revokedAt: "2026-10-03T11:00:00.000Z" }),
    ];
    renderAgents();

    const live = await screen.findByRole("article", { name: "Live workstation, paired October 1, 2026" });
    const offline = screen.getByRole("article", { name: "Offline workstation, paired October 2, 2026" });
    expect(live.compareDocumentPosition(offline) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const revoked = screen.getByText("Revoked (1)");
    expect(revoked.closest("details")?.hasAttribute("open")).toBe(false);
    expect(screen.getByRole("article", { name: "Live workstation, paired October 1, 2026" })).toBeTruthy();
    expect(within(live).getByText(/Codex 0\.156\.1 · signed in/)).toBeTruthy();

    fireEvent.click(within(live).getByText(/Codex 0\.156\.1 · signed in/));
    expect(within(live).getByText(/execution supervised_child_process/)).toBeTruthy();
    expect(screen.queryByRole("article", { name: "Old workstation, paired October 1, 2026" })).toBeNull();
  });

  it("puts focus on Keep and Escape keeps the device", async () => {
    renderAgents();
    const revoke = await screen.findByRole("button", { name: "Revoke Studio laptop, paired October 1, 2026" });

    fireEvent.click(revoke);
    const keep = screen.getByRole("button", { name: "Keep Studio laptop, paired October 1, 2026" });
    expect(document.activeElement).toBe(keep);
    fireEvent.keyDown(keep, { key: "Escape" });

    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Revoke Studio laptop, paired October 1, 2026" }));
    expect(requests.some((request) => request.url.endsWith("/revoke"))).toBe(false);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("says Pontis is unavailable and leaves the rest of the page working", async () => {
    sidecarUp = false;
    renderAgents();

    expect(await screen.findByText(/Pontis is unavailable/)).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Chat in the editor" })).toBeTruthy();
    expect(screen.getByRole("link", { name: /Set up MCP/ })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Pair a computer" })).toBeNull();
  });

  it("asks the person to sign in again when the devices route answers 401", async () => {
    signInExpired = true;
    renderAgents();

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Your sign-in has expired. Sign in again.");
  });

  it("draws nothing Pontis-related on a local Forma, and does not ask the sidecar", async () => {
    identity.hosted = false;
    renderAgents();

    expect(await screen.findByRole("heading", { name: "Chat in the editor" })).toBeTruthy();
    await waitFor(() => expect(requests.some((r) => r.url.endsWith("/api/v1/identity"))).toBe(true));
    expect(screen.queryByRole("heading", { name: "Devices" })).toBeNull();
    expect(requests.filter((r) => r.url.startsWith("/pons/"))).toEqual([]);
  });

  it("draws no panel for a signed-in person without an account", async () => {
    accountless = true;
    renderAgents();

    expect(await screen.findByRole("heading", { name: "Chat in the editor" })).toBeTruthy();
    await waitFor(() => expect(requests.some((r) => r.url.endsWith("/api/v1/projects"))).toBe(true));
    expect(screen.queryByRole("heading", { name: "Devices" })).toBeNull();
    expect(requests.filter((r) => r.url.startsWith("/pons/"))).toEqual([]);
  });
});
