/**
 * The device panel of `pontis/docs/13` § Device panel, on the Agents page
 * (plan §11 step 10a).
 *
 * It sits beside the MCP connection UI and replaces none of it: plan §8 says
 * neither door obsoletes the other. This one is for an agent running on the
 * person's own machine through Pontis; the other is for hosts that connect to
 * Forma's MCP URL.
 *
 * Only a paired Device is drawn as one. In local mode `Device: local` names the
 * transport, not a record (docs/13), and the panel is not mounted there at all.
 */

import { useCallback, useEffect, useState } from "react";

import {
  PontisUnavailable,
  createPairingCode,
  describeAuthMode,
  describeClassification,
  describeSandbox,
  listDevices,
  pairCommand,
  revokeDevice,
  type Device,
  type PairingCode,
  type ProviderInstallation,
} from "./devices";

type Load =
  | { readonly at: "loading" }
  | { readonly at: "unavailable" }
  | { readonly at: "failed"; readonly message: string }
  | { readonly at: "ready"; readonly devices: readonly Device[] };

export function DevicePanel() {
  const [load, setLoad] = useState<Load>({ at: "loading" });
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setLoad({ at: "ready", devices: await listDevices() });
    } catch (cause) {
      setLoad(
        cause instanceof PontisUnavailable
          ? { at: "unavailable" }
          : { at: "failed", message: cause instanceof Error ? cause.message : "the devices could not be read" },
      );
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <section className="mb-10 rounded-2xl border border-edge bg-surface p-5" aria-labelledby="devices-heading">
      <h2 id="devices-heading" className="text-lg font-medium">
        Devices
      </h2>
      <p className="mt-2 text-sm leading-6 text-muted">
        Machines you have paired to run an agent for Forma, locally. This is separate from the MCP
        connection above: either way of connecting an agent keeps working without the other.
      </p>

      {load.at === "loading" && <p className="mt-5 text-sm text-muted">Loading devices…</p>}

      {load.at === "unavailable" && (
        <p role="status" className="mt-5 rounded-xl border border-edge bg-surface-2 px-4 py-3 text-sm text-muted">
          Pontis is unavailable right now, so devices cannot be shown or paired. Nothing else in
          Forma depends on it.
        </p>
      )}

      {load.at === "failed" && (
        <p role="alert" className="mt-5 text-sm text-danger">
          {load.message}
        </p>
      )}

      {load.at === "ready" && (
        <>
          {notice && (
            <p role="alert" className="mt-5 text-sm text-danger">
              {notice}
            </p>
          )}
          <div className="mt-5 space-y-3">
            {load.devices.length === 0 && <p className="text-sm text-muted">No devices paired yet.</p>}
            {load.devices.map((device) => (
              <DeviceCard
                key={device.id}
                device={device}
                onRevoked={() => {
                  setNotice(null);
                  void refresh();
                }}
                onError={(message) => setNotice(message)}
              />
            ))}
          </div>
          <Pairing onUnavailable={() => setLoad({ at: "unavailable" })} />
        </>
      )}
    </section>
  );
}

function DeviceCard({
  device,
  onRevoked,
  onError,
}: {
  device: Device;
  onRevoked: () => void;
  onError: (message: string) => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [pending, setPending] = useState(false);

  async function revoke() {
    setPending(true);
    try {
      await revokeDevice(device.id);
      onRevoked();
    } catch (cause) {
      onError(
        cause instanceof PontisUnavailable
          ? cause.message
          : `${device.name} could not be revoked${cause instanceof Error ? ` (${cause.message})` : ""}`,
      );
    } finally {
      setPending(false);
      setConfirming(false);
    }
  }

  return (
    <article className="rounded-xl border border-edge bg-surface-2 px-4 py-3" aria-label={device.name}>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <p className="truncate text-sm text-paper">{device.name}</p>
          <p className="mt-1 font-mono text-[11px] text-faint">
            {device.platform} · runtime {device.runtimeVersion}
          </p>
          <p className="mt-1 font-mono text-[11px] text-faint">
            <Presence device={device} /> · last seen {device.lastSeenAt ? stamp(device.lastSeenAt) : "never"}
          </p>
        </div>
        {device.revokedAt ? (
          <span className="shrink-0 text-xs text-faint">Revoked {stamp(device.revokedAt)}</span>
        ) : confirming ? (
          <div className="flex shrink-0 gap-2">
            <button
              type="button"
              disabled={pending}
              onClick={() => void revoke()}
              className="rounded-lg border border-danger/50 px-3 py-1.5 text-xs text-danger disabled:opacity-45"
            >
              {pending ? "Revoking…" : "Confirm revoke"}
            </button>
            <button
              type="button"
              disabled={pending}
              onClick={() => setConfirming(false)}
              className="rounded-lg border border-edge px-3 py-1.5 text-xs text-muted hover:border-edge-strong"
            >
              Keep
            </button>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setConfirming(true)}
            aria-label={`Revoke ${device.name}`}
            className="shrink-0 rounded-lg border border-edge px-3 py-1.5 text-xs text-muted
                       hover:border-danger/50 hover:text-danger"
          >
            Revoke
          </button>
        )}
      </div>

      <Providers providers={device.providers} />
    </article>
  );
}

/**
 * Online, offline, or not said.
 *
 * Presence is never stored (SEC-INV-009), so a list route that does not merge
 * it in leaves nothing to show. "Offline" there would claim something nobody
 * measured.
 */
function Presence({ device }: { device: Device }) {
  if (device.revokedAt) return <span>revoked</span>;
  if (device.online === null) return <span>presence not reported</span>;
  return <span className={device.online ? "text-paper" : undefined}>{device.online ? "online" : "offline"}</span>;
}

function Providers({ providers }: { providers: readonly ProviderInstallation[] | null }) {
  if (providers === null) {
    return <p className="mt-3 text-xs text-faint">This device has not reported its providers yet.</p>;
  }
  if (providers.length === 0) {
    return <p className="mt-3 text-xs text-faint">This device connected and found no provider installed.</p>;
  }
  return (
    <ul className="mt-3 space-y-2 border-t border-edge pt-3">
      {providers.map((provider) => (
        <ProviderRow key={provider.providerId} provider={provider} />
      ))}
    </ul>
  );
}

const TONE = {
  plain: "border-edge text-muted",
  caution: "border-edge-strong text-paper",
  alert: "border-danger/50 text-danger",
} as const;

function ProviderRow({ provider }: { provider: ProviderInstallation }) {
  const profile = provider.securityProfile;
  const classification = describeClassification(profile.classification);
  const sandbox = describeSandbox(profile.sandboxBoundary);
  const auth = describeAuthMode(provider.authMode);

  return (
    <li className="text-xs leading-5 text-muted">
      <p className="text-sm text-paper">
        {provider.providerId}
        <span className="ml-2 font-mono text-[11px] text-faint">
          {provider.installed ? (provider.version ?? "version unknown") : "not installed"}
          {provider.installed && !provider.compatible ? " · incompatible" : ""}
        </span>
      </p>
      <p>
        Auth: <span className={auth.warn ? "text-danger" : "text-paper"}>{auth.label}</span>
      </p>
      <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
        Security profile:
        <span
          title={classification.note}
          className={`rounded border px-1.5 py-0.5 font-mono text-[11px] ${TONE[classification.tone]}`}
        >
          {classification.label}
        </span>
        <span className="text-faint">{classification.note}</span>
      </p>
      <p>
        Sandbox: <span className="font-mono text-paper">{sandbox.label}</span>{" "}
        <span className="text-faint">{sandbox.note}</span>
      </p>
      <p className="font-mono text-[11px] text-faint">
        execution {profile.executionBoundary} · environment {profile.environmentBoundary}
      </p>
    </li>
  );
}

/**
 * Mint a code, and say what to do with it.
 *
 * The Control Plane returns the code once and keeps only its hash, so what is
 * on screen is the only copy: it stays until the person asks for another.
 */
function Pairing({ onUnavailable }: { onUnavailable: () => void }) {
  const [pairing, setPairing] = useState<PairingCode | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copy, setCopy] = useState<"idle" | "copied" | "failed">("idle");

  async function create() {
    setPending(true);
    setError(null);
    setCopy("idle");
    try {
      setPairing(await createPairingCode());
    } catch (cause) {
      if (cause instanceof PontisUnavailable) onUnavailable();
      else setError(cause instanceof Error ? `A pairing code could not be created (${cause.message})` : "A pairing code could not be created");
    } finally {
      setPending(false);
    }
  }

  const command = pairing ? pairCommand(pairing) : null;

  return (
    <div className="mt-6 border-t border-edge pt-5">
      <p className="text-xs font-medium text-paper">Pair a new machine</p>
      <p className="mt-2 text-xs leading-5 text-muted">
        Create a code, then run the command it gives you on the machine, with Pontis installed
        there and its daemon stopped. A code works once and expires in minutes.
      </p>
      <button
        type="button"
        disabled={pending}
        onClick={() => void create()}
        className="mt-3 rounded-lg bg-signal px-4 py-2 text-sm font-medium text-ink disabled:opacity-45"
      >
        {pending ? "Creating…" : "Create pairing code"}
      </button>
      {error && (
        <p role="alert" className="mt-3 text-sm text-danger">
          {error}
        </p>
      )}
      {pairing && command && (
        <div role="status" className="mt-4 rounded-xl border border-signal/40 bg-signal/8 p-4">
          <p className="text-sm font-medium text-paper">
            Pairing code: <span className="font-mono">{pairing.code}</span>
          </p>
          <p className="mt-1 text-xs leading-5 text-muted">
            Shown once and expires {stamp(pairing.expiresAt)}. Replace the name with whatever you
            want this machine called.
          </p>
          <div className="mt-3 flex items-start gap-2">
            <code className="min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-lg bg-ink px-3 py-2 font-mono text-xs text-paper">
              {command}
            </code>
            <button
              type="button"
              onClick={() => {
                navigator.clipboard
                  ?.writeText(command)
                  .then(() => setCopy("copied"))
                  .catch(() => setCopy("failed"));
              }}
              className="shrink-0 rounded-lg border border-edge px-3 py-2 text-xs text-muted hover:border-edge-strong"
            >
              {copy === "copied" ? "Copied" : copy === "failed" ? "Copy failed" : "Copy"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** `2026-10-03T09:00:00.000Z` as `2026-10-03 09:00 UTC`: unambiguous, and no locale to guess. */
function stamp(iso: string): string {
  return `${iso.slice(0, 16).replace("T", " ")} UTC`;
}
