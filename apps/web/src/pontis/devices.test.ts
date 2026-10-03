import { describe, expect, it, vi } from "vitest";

import {
  PontisError,
  PontisUnavailable,
  createPairingCode,
  describeAuthMode,
  describeClassification,
  describeSandbox,
  listDevices,
  pairCommand,
  revokeDevice,
} from "./devices";

vi.mock("../identity", () => ({ accessToken: () => Promise.resolve("a-session") }));

const CODEX = {
  providerId: "codex",
  installed: true,
  version: "0.156.1",
  compatible: true,
  authenticated: true,
  authMode: "subscription",
  securityProfile: {
    classification: "secure_default",
    executionBoundary: "supervised_child_process",
    environmentBoundary: "allowlisted",
    sandboxBoundary: "provider_managed",
  },
};

const RECORD = {
  id: "dev_1",
  userId: "user_1",
  applicationId: "forma",
  publicIdentityKey: "k",
  name: "Studio laptop",
  platform: "linux",
  runtimeVersion: "1.2.3",
  pairedAt: "2026-10-01T10:00:00.000Z",
  lastSeenAt: "2026-10-03T09:00:00.000Z",
  revokedAt: null,
  capabilities: [CODEX],
  capabilitiesReportedAt: "2026-10-03T09:00:00.000Z",
};

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

function serving(response: () => Response) {
  return vi.fn<typeof fetch>(() => Promise.resolve(response()));
}

describe("listing devices", () => {
  it("reads the Control Plane's records and sends the session in a header", async () => {
    const fetchImpl = serving(() => json({ protocol: 1, devices: [RECORD] }));

    const devices = await listDevices({ fetchImpl });

    expect(devices).toHaveLength(1);
    expect(devices[0]).toMatchObject({ id: "dev_1", name: "Studio laptop", runtimeVersion: "1.2.3", revokedAt: null });
    expect(devices[0]?.providers?.[0]).toMatchObject({ providerId: "codex", authMode: "subscription" });
    const [url, init] = fetchImpl.mock.calls[0] ?? [];
    expect(url).toBe("/pons/v1/devices");
    expect(init?.headers).toEqual({ authorization: "Bearer a-session" });
    expect(url).not.toContain("a-session");
  });

  it("does not turn missing presence into offline", async () => {
    const [absent] = await listDevices({ fetchImpl: serving(() => json({ devices: [RECORD] })) });
    const [present] = await listDevices({ fetchImpl: serving(() => json({ devices: [{ ...RECORD, online: false }] })) });

    expect(absent?.online).toBeNull();
    expect(present?.online).toBe(false);
  });

  it("keeps a Device that never reported apart from one that found no provider", async () => {
    const [never] = await listDevices({ fetchImpl: serving(() => json({ devices: [{ ...RECORD, capabilities: null }] })) });
    const [none] = await listDevices({ fetchImpl: serving(() => json({ devices: [{ ...RECORD, capabilities: [] }] })) });

    expect(never?.providers).toBeNull();
    expect(none?.providers).toEqual([]);
  });

  it("is unavailable when the sidecar is not there", async () => {
    const refused = vi.fn(() => Promise.reject(new TypeError("fetch failed")));
    await expect(listDevices({ fetchImpl: refused })).rejects.toBeInstanceOf(PontisUnavailable);
  });

  it("is unavailable behind a proxy's 502, and where only the editor answers", async () => {
    const bad = serving(() => new Response("<html>Bad Gateway</html>", { status: 502 }));
    const editor = serving(() => new Response("<!doctype html>", { status: 200, headers: { "content-type": "text/html" } }));
    const crashed = serving(() => json({ error: { code: "internal_error" } }, 500));

    await expect(listDevices({ fetchImpl: bad })).rejects.toBeInstanceOf(PontisUnavailable);
    await expect(listDevices({ fetchImpl: editor })).rejects.toBeInstanceOf(PontisUnavailable);
    await expect(listDevices({ fetchImpl: crashed })).rejects.toBeInstanceOf(PontisUnavailable);
  });

  it("is unavailable rather than half-rendered when a record is malformed", async () => {
    await expect(listDevices({ fetchImpl: serving(() => json({ devices: [{ id: "dev_1" }] })) })).rejects.toBeInstanceOf(
      PontisUnavailable,
    );
    await expect(listDevices({ fetchImpl: serving(() => json({ error: { code: "x" } })) })).rejects.toBeInstanceOf(
      PontisUnavailable,
    );
  });

  it("separates a refusal from an outage", async () => {
    const refused = serving(() => json({ error: { code: "origin_not_allowed", category: "transport" } }, 403));

    const error = await listDevices({ fetchImpl: refused }).catch((cause: unknown) => cause);

    expect(error).toBeInstanceOf(PontisError);
    expect(error).toMatchObject({ code: "origin_not_allowed", status: 403 });
  });
});

describe("revoking a device", () => {
  it("posts to the revoke route and returns the revoked record", async () => {
    const revoked = { ...RECORD, revokedAt: "2026-10-03T10:00:00.000Z" };
    const fetchImpl = serving(() => json({ protocol: 1, device: revoked }));

    const result = await revokeDevice("dev_1", { fetchImpl });

    const [url, init] = fetchImpl.mock.calls[0] ?? [];
    expect(url).toBe("/pons/v1/devices/dev_1/revoke");
    expect(init?.method).toBe("POST");
    expect(result.revokedAt).toBe("2026-10-03T10:00:00.000Z");
  });

  it("reports a Device that is not this person's as not found", async () => {
    const missing = serving(() => json({ error: { code: "device_not_found" } }, 404));
    await expect(revokeDevice("dev_x", { fetchImpl: missing })).rejects.toMatchObject({ code: "device_not_found" });
  });

  it("escapes the id into the path", async () => {
    const fetchImpl = serving(() => json({ device: RECORD }));
    await revokeDevice("a/b", { fetchImpl });
    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/pons/v1/devices/a%2Fb/revoke");
  });
});

describe("pairing codes", () => {
  const GRANT = {
    protocol: 1,
    code: "ABCD-EFGH-JKLM",
    expiresAt: "2026-10-03T10:10:00.000Z",
    controlPlaneUrl: "https://forma.example",
    applicationId: "forma",
  };

  it("mints a code with POST and builds the command the person runs", async () => {
    const fetchImpl = serving(() => json(GRANT));

    const pairing = await createPairingCode({ fetchImpl });

    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/pons/v1/pairing/codes");
    expect(fetchImpl.mock.calls[0]?.[1]?.method).toBe("POST");
    expect(pairCommand(pairing)).toBe(
      'pontis pair --origin https://forma.example --application forma --name "My laptop" ABCD-EFGH-JKLM',
    );
  });

  it("is unavailable when the sidecar is not there", async () => {
    const refused = vi.fn(() => Promise.reject(new TypeError("fetch failed")));
    await expect(createPairingCode({ fetchImpl: refused })).rejects.toBeInstanceOf(PontisUnavailable);
  });
});

describe("wording", () => {
  it("calls a provider_managed sandbox exactly that, and never secure or runtime-enforced", () => {
    const sandbox = describeSandbox("provider_managed");

    expect(sandbox.label).toBe("provider_managed");
    expect(`${sandbox.label} ${sandbox.note}`).not.toMatch(/\bsecure\b|runtime[-_ ]enforced/i);
    expect(sandbox.note).toMatch(/no claim about its strength/);
    expect(describeSandbox("runtime_enforced").label).toBe("runtime_enforced");
  });

  it("keeps the four classifications distinct, and shows an unknown one as itself", () => {
    const labels = ["secure_default", "explicit_opt_in", "test_only", "unsupported"].map(
      (c) => describeClassification(c).label,
    );

    expect(new Set(labels).size).toBe(4);
    expect(describeClassification("something_new").label).toBe("something_new");
    expect(describeClassification("explicit_opt_in").tone).not.toBe(describeClassification("secure_default").tone);
  });

  it("warns that an API key bills directly", () => {
    expect(describeAuthMode("api_key").warn).toBe(true);
    expect(describeAuthMode("subscription").warn).toBe(false);
  });
});
