/**
 * The device panel's client: the Control Plane routes the sidecar mounts, spoken
 * by the browser (plan §11 step 10a).
 *
 * Forma's code against three documented routes, not `@pontis/react` — plan §10(b)
 * D8, for the reason `channel.ts` gives. The session bearer travels in a header,
 * and the paths are same-origin through the proxy in front of both processes.
 *
 * # Unavailable is a first-class answer
 *
 * Plan §7.3: `forma-server` has no dependency on the sidecar, so a sidecar that
 * is down must cost the person this panel and nothing else. Every way of not
 * reaching it — a refused connection, a proxy's 502, a route nothing serves, a
 * body that is not the Control Plane's — is one `PontisUnavailable`, so the page
 * has a single thing to catch. It is told apart from the Control Plane
 * *answering* with a refusal, which is a `PontisError` and a different message.
 */

import { accessToken } from "../identity";

const DEVICES_PATH = "/pons/v1/devices";
const PAIRING_CODES_PATH = "/pons/v1/pairing/codes";

/** One provider the Device last reported (`pontis/docs/07` § Provider status). */
export interface ProviderInstallation {
  readonly providerId: string;
  readonly installed: boolean;
  readonly version: string | null;
  readonly compatible: boolean;
  readonly authenticated: boolean;
  /** `subscription`, `api_key`, `unknown` or `not_authenticated`; kept as sent. */
  readonly authMode: string;
  readonly securityProfile: SecurityProfile;
}

/**
 * ADR-019's four registries, kept as the strings the Control Plane sent.
 *
 * Not narrowed to unions: a value this build has never seen must reach the
 * person as itself rather than be coerced into a neighbour, and `describe*`
 * below handles the known ones explicitly.
 */
export interface SecurityProfile {
  readonly classification: string;
  readonly executionBoundary: string;
  readonly environmentBoundary: string;
  readonly sandboxBoundary: string;
}

export interface Device {
  readonly id: string;
  readonly name: string;
  readonly platform: string;
  readonly runtimeVersion: string;
  readonly pairedAt: string;
  readonly lastSeenAt: string | null;
  readonly revokedAt: string | null;
  /**
   * `null` when the response does not say.
   *
   * `GET /pons/v1/devices` returns the stored record, and presence is never
   * stored (SEC-INV-009), so a host has to merge it in itself. Where it does
   * not, the honest value is "unknown" — rendering it as offline would tell the
   * person a running Device is down.
   */
  readonly online: boolean | null;
  /** `null` is "never reported"; `[]` is "connected and found no provider". */
  readonly providers: readonly ProviderInstallation[] | null;
}

export interface PairingCode {
  readonly code: string;
  readonly expiresAt: string;
  readonly controlPlaneUrl: string;
  readonly applicationId: string;
}

/** The sidecar could not be reached, or what answered was not the Control Plane. */
export class PontisUnavailable extends Error {
  constructor(detail: string) {
    super(`Pontis is unavailable: ${detail}`);
    this.name = "PontisUnavailable";
  }
}

/** The Control Plane answered and said no. `code` is its transport vocabulary. */
export class PontisError extends Error {
  constructor(
    readonly code: string,
    readonly status: number,
  ) {
    super(code);
    this.name = "PontisError";
  }
}

export interface DevicesClient {
  readonly baseUrl?: string;
  readonly fetchImpl?: typeof fetch;
}

export async function listDevices(client: DevicesClient = {}): Promise<readonly Device[]> {
  const body = await call(client, DEVICES_PATH, "GET");
  const devices = isRecord(body) ? body.devices : undefined;
  if (!Array.isArray(devices)) throw new PontisUnavailable("the device list was not in the shape expected");
  return devices.map((entry) => device(entry) ?? malformed("a Device record"));
}

export async function revokeDevice(id: string, client: DevicesClient = {}): Promise<Device> {
  const body = await call(client, `${DEVICES_PATH}/${encodeURIComponent(id)}/revoke`, "POST");
  return (isRecord(body) ? device(body.device) : null) ?? malformed("the revoked Device");
}

/**
 * Mint a pairing code.
 *
 * The code is returned once and the Control Plane keeps only its hash, so the
 * caller has to show it from this response; there is no route to read it again.
 */
export async function createPairingCode(client: DevicesClient = {}): Promise<PairingCode> {
  const body = await call(client, PAIRING_CODES_PATH, "POST");
  if (
    !isRecord(body) ||
    typeof body.code !== "string" ||
    typeof body.expiresAt !== "string" ||
    typeof body.controlPlaneUrl !== "string" ||
    typeof body.applicationId !== "string"
  ) {
    return malformed("the pairing code");
  }
  return {
    code: body.code,
    expiresAt: body.expiresAt,
    controlPlaneUrl: body.controlPlaneUrl,
    applicationId: body.applicationId,
  };
}

/**
 * The command the person runs on the machine to be paired
 * (`pontis/docs/04-runtime.md` § Runtime diagnostics).
 *
 * The display name is the person's own input to the CLI and is deliberately not
 * derived from anything here, so the placeholder is theirs to replace.
 */
export function pairCommand(pairing: PairingCode): string {
  return `pontis pair --origin ${pairing.controlPlaneUrl} --application ${pairing.applicationId} --name "My laptop" ${pairing.code}`;
}

// --- wording -----------------------------------------------------------------

/**
 * What a sandbox boundary is called, and what it is not.
 *
 * `pontis/docs/13` § Device panel: a `provider_managed` sandbox is called exactly
 * that and is never rendered as runtime-enforced or simply "secure". ADR-019 is
 * why — it says only that the responsibility stays with the provider, and makes
 * no claim about strength.
 */
export function describeSandbox(boundary: string): { readonly label: string; readonly note: string } {
  switch (boundary) {
    case "provider_managed":
      return {
        label: "provider_managed",
        note: "The provider is responsible for its sandbox. Pontis makes no claim about its strength.",
      };
    case "runtime_enforced":
      return { label: "runtime_enforced", note: "The Pontis runtime enforces the sandbox." };
    case "absent":
      return { label: "absent", note: "There is no sandbox." };
    case "not_applicable":
      return { label: "not_applicable", note: "This provider does not run work in a sandbox." };
    default:
      return { label: boundary, note: "This version of Forma does not recognise this value." };
  }
}

/**
 * The four classifications stay four visibly different things, shown as the
 * registry ids rather than paraphrases that would blur them (ADR-019).
 */
export function describeClassification(classification: string): {
  readonly label: string;
  readonly note: string;
  readonly tone: "plain" | "caution" | "alert";
} {
  switch (classification) {
    case "secure_default":
      return { label: "secure_default", note: "Enabled by default.", tone: "plain" };
    case "explicit_opt_in":
      return { label: "explicit_opt_in", note: "Enabled by this machine's owner, locally.", tone: "caution" };
    case "test_only":
      return { label: "test_only", note: "For tests; never started in an installed runtime.", tone: "alert" };
    case "unsupported":
      return { label: "unsupported", note: "Not supported.", tone: "alert" };
    default:
      return { label: classification, note: "This version of Forma does not recognise this value.", tone: "alert" };
  }
}

/** `api_key` is the one that costs money, which `pontis/docs/07` says to warn about. */
export function describeAuthMode(mode: string): { readonly label: string; readonly warn: boolean } {
  switch (mode) {
    case "subscription":
      return { label: "subscription", warn: false };
    case "api_key":
      return { label: "api_key — usage is billed directly to that key", warn: true };
    case "not_authenticated":
      return { label: "not signed in", warn: true };
    case "unknown":
      return { label: "unknown", warn: false };
    default:
      return { label: mode, warn: true };
  }
}

// --- transport ---------------------------------------------------------------

async function call(client: DevicesClient, path: string, method: "GET" | "POST"): Promise<unknown> {
  const fetchImpl = client.fetchImpl ?? globalThis.fetch.bind(globalThis);
  const token = await accessToken();

  let response: Response;
  try {
    response = await fetchImpl(`${client.baseUrl ?? ""}${path}`, {
      method,
      headers: token ? { authorization: `Bearer ${token}` } : {},
    });
  } catch {
    throw new PontisUnavailable("the connection was refused");
  }

  let body: unknown;
  try {
    body = (await response.json()) as unknown;
  } catch {
    // A proxy's own 502 page, or the editor's HTML where no proxy rule matched:
    // neither is the Control Plane, whatever the status says.
    throw new PontisUnavailable(`it answered ${response.status} without a Control Plane body`);
  }

  if (response.ok) return body;

  const code = isRecord(body) && isRecord(body.error) && typeof body.error.code === "string" ? body.error.code : null;
  // Control Plane bodies carry a `protocol`; a 5xx without one is a proxy or a
  // crash in front of it, and `internal_error` is the Control Plane failing.
  if (code === null || response.status >= 500) throw new PontisUnavailable(`it answered ${response.status}`);
  throw new PontisError(code, response.status);
}

function malformed(what: string): never {
  throw new PontisUnavailable(`${what} was not in the shape expected`);
}

function device(value: unknown): Device | null {
  if (!isRecord(value)) return null;
  const { id, name, platform, runtimeVersion, pairedAt, lastSeenAt, revokedAt, online, capabilities } = value;
  if (
    typeof id !== "string" ||
    typeof name !== "string" ||
    typeof platform !== "string" ||
    typeof runtimeVersion !== "string" ||
    typeof pairedAt !== "string"
  ) {
    return null;
  }
  let providers: ProviderInstallation[] | null = null;
  if (Array.isArray(capabilities)) {
    providers = [];
    for (const entry of capabilities) {
      const parsed = provider(entry);
      if (!parsed) return null;
      providers.push(parsed);
    }
  } else if (capabilities !== null && capabilities !== undefined) {
    return null;
  }
  return {
    id,
    name,
    platform,
    runtimeVersion,
    pairedAt,
    lastSeenAt: typeof lastSeenAt === "string" ? lastSeenAt : null,
    revokedAt: typeof revokedAt === "string" ? revokedAt : null,
    online: typeof online === "boolean" ? online : null,
    providers,
  };
}

function provider(value: unknown): ProviderInstallation | null {
  if (!isRecord(value) || !isRecord(value.securityProfile)) return null;
  const { providerId, installed, version, compatible, authenticated, authMode } = value;
  const { classification, executionBoundary, environmentBoundary, sandboxBoundary } = value.securityProfile;
  if (
    typeof providerId !== "string" ||
    typeof installed !== "boolean" ||
    typeof compatible !== "boolean" ||
    typeof authenticated !== "boolean" ||
    typeof authMode !== "string" ||
    typeof classification !== "string" ||
    typeof executionBoundary !== "string" ||
    typeof environmentBoundary !== "string" ||
    typeof sandboxBoundary !== "string"
  ) {
    return null;
  }
  return {
    providerId,
    installed,
    version: typeof version === "string" ? version : null,
    compatible,
    authenticated,
    authMode,
    securityProfile: { classification, executionBoundary, environmentBoundary, sandboxBoundary },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
