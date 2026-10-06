/**
 * The Devices section of the Chat in the editor connection page.
 *
 * The browser talks to Forma's three documented Pontis routes. It never imports
 * Pontis's React package, and the session bearer stays in the request header.
 */

import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";

import { liveFeed } from "./channel";
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
import { parseLiveFrame } from "./events";

const DEVICE_REFRESH_MS = 30_000;

type Load =
  | { readonly at: "loading" }
  | { readonly at: "unavailable" }
  | { readonly at: "failed"; readonly message: string }
  | { readonly at: "ready"; readonly devices: readonly Device[] };

interface PairingAttempt {
  readonly code: PairingCode;
  readonly name: string;
  readonly issuedAt: number;
  readonly existingDeviceIds: readonly string[];
}

type PairingStage = "ready" | "waiting" | "paired" | "online" | "expired";

export function DevicePanel() {
  const [load, setLoad] = useState<Load>({ at: "loading" });
  const [notice, setNotice] = useState<string | null>(null);
  const [pairingOpen, setPairingOpen] = useState(false);
  const [pairName, setPairName] = useState("");
  const [pairing, setPairing] = useState<PairingAttempt | null>(null);
  const [pairingError, setPairingError] = useState<string | null>(null);
  const [pairingPending, setPairingPending] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const reads = useRef(0);

  const refresh = useCallback(async () => {
    reads.current += 1;
    const mine = reads.current;
    try {
      const devices = await listDevices();
      if (mine === reads.current) setLoad({ at: "ready", devices });
    } catch (cause) {
      if (mine !== reads.current) return;
      setLoad(
        cause instanceof PontisUnavailable
          ? { at: "unavailable" }
          : { at: "failed", message: cause instanceof Error ? cause.message : "the devices could not be read" },
      );
    }
  }, []);

  useEffect(() => {
    void refresh();
    const stop = liveFeed.subscribe({
      state: (state) => {
        if (state.at === "connected") void refresh();
      },
      frame: (raw) => {
        if (parseLiveFrame(raw)?.type.startsWith("device.")) void refresh();
      },
    });
    const timer = setInterval(() => void refresh(), DEVICE_REFRESH_MS);
    return () => {
      stop();
      clearInterval(timer);
    };
  }, [refresh]);

  const devices = load.at === "ready" ? load.devices : [];
  const active = devices
    .filter((device) => !device.revokedAt)
    .sort((left, right) => Number(right.online === true) - Number(left.online === true) || Date.parse(right.pairedAt) - Date.parse(left.pairedAt));
  const revoked = devices
    .filter((device) => Boolean(device.revokedAt))
    .sort((left, right) => Date.parse(right.revokedAt ?? "") - Date.parse(left.revokedAt ?? ""));
  const pairedDevice = pairing ? findPairedDevice(pairing, active) : null;
  const stage = pairing ? pairingStage(pairing, pairedDevice, now) : "ready";

  useEffect(() => {
    if (!pairing || pairedDevice) return;
    const timer = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(timer);
  }, [pairing, pairedDevice?.id]);

  function openPairing() {
    setPairing(null);
    setPairName("");
    setPairingError(null);
    setPairingOpen(true);
  }

  function closePairing() {
    setPairing(null);
    setPairName("");
    setPairingError(null);
    setPairingOpen(false);
  }

  async function createPairing(name: string) {
    const trimmed = name.trim();
    if (trimmed === "") return;
    setPairingPending(true);
    setPairingError(null);
    const issuedAt = Date.now();
    try {
      const code = await createPairingCode();
      setPairing({
        code,
        name: trimmed,
        issuedAt,
        existingDeviceIds: devices.map((device) => device.id),
      });
      setPairName(trimmed);
      setNow(Date.now());
    } catch (cause) {
      if (cause instanceof PontisUnavailable) setLoad({ at: "unavailable" });
      else setPairingError(cause instanceof Error ? "A pairing code could not be created (" + cause.message + ")" : "A pairing code could not be created");
    } finally {
      setPairingPending(false);
    }
  }

  return (
    <section className="rounded-xl border border-edge bg-surface p-5" aria-labelledby="devices-heading">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h2 id="devices-heading" className="text-base font-medium text-paper">
            Devices
          </h2>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-muted">
            You can pair several computers. When more than one is online, choose which one runs the agent in the chat.
          </p>
        </div>
        {load.at === "ready" && active.length > 0 && (
          <button
            type="button"
            onClick={openPairing}
            className="shrink-0 rounded-lg border border-edge px-3 py-2 text-xs text-muted hover:border-edge-strong hover:text-paper"
          >
            Pair a computer
          </button>
        )}
      </div>

      {load.at === "loading" && <p className="mt-5 text-sm text-muted">Loading computers…</p>}

      {load.at === "unavailable" && (
        <p role="status" className="mt-5 rounded-lg border border-edge bg-surface-2 px-4 py-3 text-sm text-muted">
          Pontis is unavailable right now, so computers cannot be shown or paired. Nothing else in Forma depends on it.
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
          <div className="mt-5">
            {active.length === 0 ? (
              <div className="rounded-lg border border-edge bg-surface-2 px-4 py-4">
                <p className="text-sm text-paper">No computers paired yet.</p>
                <p className="mt-1 text-xs leading-5 text-muted">Pair a computer to use the editor’s chat.</p>
                {!pairingOpen && (
                  <button
                    type="button"
                    onClick={openPairing}
                    className="mt-4 rounded-lg bg-signal px-4 py-2 text-sm font-medium text-ink"
                  >
                    Pair a computer
                  </button>
                )}
              </div>
            ) : (
              <>
                <h3 className="mb-3 text-sm font-medium text-paper">Your computers</h3>
                <div className="space-y-2">
                  {active.map((device) => (
                    <DeviceCard
                      key={device.id}
                      device={device}
                      isWaitingToStart={device.id === pairedDevice?.id && device.online !== true}
                      onRevoked={() => {
                        setNotice(null);
                        void refresh();
                      }}
                      onError={setNotice}
                    />
                  ))}
                </div>
              </>
            )}
          </div>

          {pairingOpen && !pairedDevice && (
            <PairingSteps
              name={pairName}
              attempt={pairing}
              stage={stage}
              now={now}
              pending={pairingPending}
              error={pairingError}
              onNameChange={setPairName}
              onSubmit={(event) => {
                event.preventDefault();
                void createPairing(pairName);
              }}
              onCreateNew={() => {
                if (pairing) void createPairing(pairing.name);
              }}
              onClose={closePairing}
            />
          )}

          {revoked.length > 0 && (
            <details className="mt-5 border-t border-edge pt-2">
              <summary aria-label={"Show " + revoked.length + " revoked computers"} className="cursor-pointer text-xs text-muted">
                Revoked ({revoked.length})
              </summary>
              <div className="mt-2 space-y-2">
                {revoked.map((device) => (
                  <div key={device.id} className="flex flex-wrap justify-between gap-x-4 gap-y-1 border-t border-edge py-3 text-xs">
                    <span className="min-w-0 break-words text-muted">{device.name}</span>
                    <span className="shrink-0 text-faint">Paired {pairedDate(device.pairedAt)} · Revoked {stamp(device.revokedAt ?? "")}</span>
                  </div>
                ))}
              </div>
            </details>
          )}
        </>
      )}
    </section>
  );
}

function PairingSteps({
  name,
  attempt,
  stage,
  now,
  pending,
  error,
  onNameChange,
  onSubmit,
  onCreateNew,
  onClose,
}: {
  name: string;
  attempt: PairingAttempt | null;
  stage: PairingStage;
  now: number;
  pending: boolean;
  error: string | null;
  onNameChange: (value: string) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onCreateNew: () => void;
  onClose: () => void;
}) {
  const [installNoteOpen, setInstallNoteOpen] = useState(false);
  const pairingCommand = attempt ? pairCommand(attempt.code, attempt.name) : null;

  return (
    <section className="mt-4 rounded-xl border border-edge bg-surface-2 p-4" aria-label="Pair a computer steps">
      <div className="mb-5 flex items-center justify-between gap-3">
        <h3 className="text-sm font-medium text-paper">Pair a computer</h3>
        <button type="button" onClick={onClose} className="rounded-lg border border-edge px-3 py-1.5 text-xs text-muted hover:text-paper">
          Close
        </button>
      </div>

      <ol className="space-y-5">
        <li className="grid grid-cols-[24px_minmax(0,1fr)] gap-3">
          <span aria-hidden="true" className="grid h-6 w-6 place-items-center rounded-full border border-edge-strong font-mono text-[11px] text-muted">1</span>
          <div className="min-w-0">
            <h4 className="text-sm font-medium text-paper">Install Pontis on that computer.</h4>
            <p className="mt-1 text-xs leading-5 text-muted">Pontis connects the agent on your computer to Forma.</p>
            <p className="mt-2 text-xs">
              <a
                href="#pontis-install-placeholder"
                aria-label="How to install Pontis, placeholder link"
                className="text-paper underline underline-offset-4"
                onClick={(event) => {
                  event.preventDefault();
                  setInstallNoteOpen((open) => !open);
                }}
              >
                How to install
              </a>
              <span className="ml-2 text-faint">Placeholder</span>
            </p>
            {installNoteOpen && <p id="pontis-install-placeholder" className="mt-2 text-xs text-muted">Installation instructions aren’t linked yet.</p>}
          </div>
        </li>

        <li className="grid grid-cols-[24px_minmax(0,1fr)] gap-3">
          <span aria-hidden="true" className="grid h-6 w-6 place-items-center rounded-full border border-edge-strong font-mono text-[11px] text-muted">2</span>
          <div className="min-w-0">
            <h4 className="text-sm font-medium text-paper">Name it and pair it.</h4>
            {stage === "ready" && (
              <form className="mt-3 max-w-sm space-y-2" onSubmit={onSubmit}>
                <label htmlFor="computer-name" className="block text-xs text-muted">
                  Computer name <span className="text-faint">(required)</span>
                </label>
                <input
                  id="computer-name"
                  value={name}
                  onChange={(event) => onNameChange(event.target.value)}
                  maxLength={120}
                  autoComplete="off"
                  aria-describedby="computer-name-help"
                  className="w-full rounded-lg border border-edge bg-ink px-3 py-2 text-sm text-paper outline-none focus:border-signal"
                />
                <p id="computer-name-help" className="text-xs text-faint">Use a name you’ll recognize in the chat.</p>
                <button
                  type="submit"
                  disabled={pending || name.trim() === ""}
                  aria-label="Create a pairing code for the named computer"
                  className="rounded-lg bg-signal px-4 py-2 text-sm font-medium text-ink disabled:cursor-not-allowed disabled:opacity-45"
                >
                  {pending ? "Creating…" : "Create pairing code"}
                </button>
              </form>
            )}

            {stage === "waiting" && attempt && pairingCommand && (
              <div className="mt-3 rounded-lg border border-edge bg-ink p-3">
                <p className="text-xs text-muted">{attempt.name}</p>
                <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
                  <code className="font-mono text-xs text-paper">{attempt.code.code}</code>
                  <CopyButton value={pairingCommand} label={"Copy pairing command for " + attempt.name} />
                </div>
                <pre className="mt-2 whitespace-pre-wrap break-words font-mono text-[11px] leading-5 text-muted">{pairingCommand}</pre>
                <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
                  <span>{expiryLabel(attempt.code.expiresAt, now)}</span>
                  <span>Works once</span>
                </div>
              </div>
            )}

            {stage === "expired" && attempt && (
              <div className="mt-3 rounded-lg border border-signal/50 bg-ink p-3">
                <p className="text-sm font-medium text-signal">Expired</p>
                <p className="mt-1 text-xs text-muted">This code can no longer pair a computer.</p>
                <button type="button" onClick={onCreateNew} disabled={pending} className="mt-3 rounded-lg bg-signal px-4 py-2 text-sm font-medium text-ink disabled:opacity-45">
                  {pending ? "Creating…" : "Create a new code"}
                </button>
              </div>
            )}
            {error && <p role="alert" className="mt-3 text-xs text-danger">{error}</p>}
          </div>
        </li>

        <li className="grid grid-cols-[24px_minmax(0,1fr)] gap-3">
          <span aria-hidden="true" className="grid h-6 w-6 place-items-center rounded-full border border-edge-strong font-mono text-[11px] text-muted">3</span>
          <div className="min-w-0">
            <h4 className="text-sm font-medium text-paper">Start it.</h4>
            <div className="mt-2 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-edge bg-ink px-3 py-2">
              <code className="font-mono text-xs text-paper">pontis run</code>
              <CopyButton value="pontis run" label={attempt ? "Copy pontis run command for " + attempt.name : "Copy pontis run command"} />
            </div>
            <p className="mt-2 text-xs leading-5 text-muted">Keep Pontis running on that computer while you use the editor’s chat.</p>
          </div>
        </li>
      </ol>

      <p role="status" className={"mt-5 border-t border-edge pt-3 text-xs " + (stage === "expired" ? "text-signal" : "text-muted")}>
        {pairingStatus(stage)}
      </p>
    </section>
  );
}

function DeviceCard({
  device,
  isWaitingToStart,
  onRevoked,
  onError,
}: {
  device: Device;
  isWaitingToStart: boolean;
  onRevoked: () => void;
  onError: (message: string) => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [pending, setPending] = useState(false);
  const revokeButton = useRef<HTMLButtonElement>(null);
  const keepButton = useRef<HTMLButtonElement>(null);
  const wasConfirming = useRef(false);
  const specific = device.name + ", paired " + pairedDate(device.pairedAt);

  useEffect(() => {
    if (confirming) {
      wasConfirming.current = true;
      keepButton.current?.focus();
    } else if (wasConfirming.current) {
      wasConfirming.current = false;
      revokeButton.current?.focus();
    }
  }, [confirming]);

  useEffect(() => {
    if (!confirming) return;
    const escapeKeeps = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setConfirming(false);
      }
    };
    window.addEventListener("keydown", escapeKeeps);
    return () => window.removeEventListener("keydown", escapeKeeps);
  }, [confirming]);

  async function revoke() {
    setPending(true);
    try {
      await revokeDevice(device.id);
      onRevoked();
    } catch (cause) {
      onError(
        cause instanceof PontisUnavailable
          ? cause.message
          : device.name + " could not be revoked" + (cause instanceof Error ? " (" + cause.message + ")" : ""),
      );
    } finally {
      setPending(false);
      setConfirming(false);
    }
  }

  return (
    <article className="rounded-lg border border-edge bg-surface-2 px-4 py-3" aria-label={specific}>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-paper" title={device.name}>{device.name}</p>
          <p className="mt-1 text-xs text-muted">{device.platform} · Paired {pairedDate(device.pairedAt)}</p>
          <p className="mt-2 flex items-center gap-2 text-xs text-muted">
            <span aria-hidden="true" className={"h-1.5 w-1.5 rounded-full border " + (device.online ? "border-paper bg-paper" : "border-muted")} />
            <Presence device={device} />
          </p>
          {isWaitingToStart && (
            <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-2 text-xs text-muted">
              <span>Paired. Waiting for it to start…</span>
              <code className="font-mono text-paper">pontis run</code>
              <CopyButton value="pontis run" label={"Copy pontis run command for " + device.name} />
            </div>
          )}
        </div>
        <button
          ref={revokeButton}
          type="button"
          disabled={confirming || pending}
          onClick={() => setConfirming(true)}
          aria-label={"Revoke " + specific}
          aria-expanded={confirming}
          className="shrink-0 rounded-lg border border-edge px-3 py-1.5 text-xs text-muted hover:border-danger/50 hover:text-danger disabled:opacity-45"
        >
          Revoke
        </button>
      </div>

      <Providers device={device} specific={specific} />

      {confirming && (
        <div className="mt-4 grid gap-3 border-t border-edge pt-3">
          <p className="text-xs leading-5 text-muted">
            <strong className="font-medium text-paper">Revoke {device.name}?</strong> The agent there stops at once and any running turn ends. To pair this computer again later, run <code className="font-mono text-paper">pontis unpair</code> on it first.
          </p>
          <div className="flex gap-2">
            <button
              type="button"
              disabled={pending}
              onClick={() => void revoke()}
              aria-label={"Confirm revoke " + specific}
              className="rounded-lg border border-danger/50 px-3 py-1.5 text-xs text-danger disabled:opacity-45"
            >
              {pending ? "Revoking…" : "Revoke"}
            </button>
            <button
              ref={keepButton}
              type="button"
              disabled={pending}
              onClick={() => setConfirming(false)}
              aria-label={"Keep " + specific}
              className="rounded-lg border border-edge px-3 py-1.5 text-xs text-muted hover:text-paper disabled:opacity-45"
            >
              Keep
            </button>
          </div>
        </div>
      )}
    </article>
  );
}

function Presence({ device }: { device: Device }) {
  if (device.online === null) return <span>Presence not reported</span>;
  return <span className={device.online ? "text-paper" : undefined}>{device.online ? "Online" : "Offline"}</span>;
}

function Providers({ device, specific }: { device: Device; specific: string }) {
  if (device.providers === null) {
    return <p className="mt-3 border-t border-edge pt-3 text-xs text-faint">This computer has not reported its providers yet.</p>;
  }
  if (device.providers.length === 0) {
    return <p className="mt-3 border-t border-edge pt-3 text-xs text-faint">This computer connected and found no provider installed.</p>;
  }
  const summary = device.providers.map(providerSummary).join(" · ");
  return (
    <details className="mt-3 border-t border-edge pt-2">
      <summary aria-label={"Provider details for " + specific} className="cursor-pointer break-words text-xs text-muted">
        {summary}
      </summary>
      <dl className="mt-3 grid grid-cols-[max-content_minmax(0,1fr)] gap-x-3 gap-y-2 font-mono text-[11px] leading-5 text-faint">
      <dt>Device id</dt><dd className="break-all">{device.id}</dd>
        <dt>Runtime</dt><dd>{device.runtimeVersion}</dd>
        {device.providers.map((provider) => <ProviderDetail key={provider.providerId} provider={provider} />)}
      </dl>
    </details>
  );
}

function ProviderDetail({ provider }: { provider: ProviderInstallation }) {
  const profile = provider.securityProfile;
  const classification = describeClassification(profile.classification);
  const sandbox = describeSandbox(profile.sandboxBoundary);
  const auth = describeAuthMode(provider.authMode);

  return (
    <>
      <dt className="col-span-2 mt-2 border-t border-edge pt-2 text-paper">{provider.providerId} {provider.version ?? "version unknown"}</dt>
      <dt>Installed</dt><dd>{provider.installed ? "yes" : "no"}{provider.installed && !provider.compatible ? " · incompatible" : ""}</dd>
      <dt>Sign-in</dt><dd>{provider.authenticated ? "signed in" : auth.label}</dd>
      <dt>Auth mode</dt><dd>{auth.label}</dd>
      <dt>Security</dt>
      <dd>
        <span title={classification.note}>{classification.label}</span> · {classification.note}
      </dd>
      <dt>Sandbox</dt><dd>{sandbox.label} · {sandbox.note}</dd>
      <dt>Execution</dt><dd className="break-all">execution {profile.executionBoundary}</dd>
      <dt>Environment</dt><dd className="break-all">environment {profile.environmentBoundary}</dd>
    </>
  );
}

function CopyButton({ value, label }: { value: string; label: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

  async function copy() {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("clipboard unavailable");
      await navigator.clipboard.writeText(value);
      setState("copied");
    } catch {
      setState("failed");
    }
  }

  return (
    <span className="inline-flex flex-col items-start gap-1">
      <button type="button" onClick={() => void copy()} aria-label={label} className="rounded-lg border border-edge px-3 py-1.5 text-xs text-muted hover:border-edge-strong hover:text-paper">
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy"}
      </button>
      {state === "failed" && <span role="status" className="text-[11px] text-muted">Clipboard unavailable. Select the text and copy it manually.</span>}
    </span>
  );
}

function findPairedDevice(attempt: PairingAttempt, devices: readonly Device[]): Device | null {
  return devices.find((device) =>
    !attempt.existingDeviceIds.includes(device.id) &&
    device.name === attempt.name &&
    Date.parse(device.pairedAt) >= attempt.issuedAt - 5_000,
  ) ?? null;
}

function pairingStage(attempt: PairingAttempt, device: Device | null, now: number): PairingStage {
  if (Date.parse(attempt.code.expiresAt) <= now) return "expired";
  if (device?.online === true) return "online";
  if (device) return "paired";
  return "waiting";
}

function pairingStatus(stage: PairingStage): string {
  switch (stage) {
    case "ready": return "Ready to pair your computer";
    case "waiting": return "Waiting for the pairing…";
    case "paired": return "Paired. Waiting for it to start…";
    case "online": return "Online";
    case "expired": return "Expired";
  }
}

function expiryLabel(expiresAt: string, now: number): string {
  const seconds = Math.max(0, Math.ceil((Date.parse(expiresAt) - now) / 1_000));
  const minutes = Math.floor(seconds / 60);
  return "Expires in " + minutes + ":" + String(seconds % 60).padStart(2, "0");
}

function providerSummary(provider: ProviderInstallation): string {
  const name = provider.providerId.toLowerCase() === "codex"
    ? "Codex"
    : provider.providerId.replace(/[-_]/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
  const version = provider.version ?? "version unknown";
  const signIn = provider.authenticated ? "signed in" : "not signed in";
  const boundary = provider.securityProfile.sandboxBoundary;
  const sandbox = boundary === "absent" || boundary === "not_applicable"
    ? "no sandbox"
    : boundary === "provider_managed"
      ? "provider managed sandbox"
      : boundary === "runtime_enforced"
        ? "runtime sandbox"
        : boundary;
  return name + " " + version + " · " + signIn + " · " + sandbox;
}

function pairedDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "date unknown";
  return new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short", timeZone: "UTC" }).format(date);
}

/** `2026-10-03T09:00:00.000Z` as an unambiguous timestamp for revocation history. */
function stamp(iso: string): string {
  return iso.length >= 16 ? iso.slice(0, 16).replace("T", " ") + " UTC" : "date unknown";
}
