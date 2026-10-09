/**
 * Signing in and the first visit (plan §110.2, §107.3, §113.1).
 *
 * A magic link proves the email address before the invite gate is checked. The
 * page keeps that flow explicit: send a link, open it in this browser, then
 * create a workspace only if the address has an invitation.
 */

import { useEffect, useState, type FormEvent, type ReactNode } from "react";

import { ApiError, createAccount, getOnboarding, type Onboarding } from "./api";
import { sendMagicLink, signOut } from "./identity";

type SignInScreen = "entry" | "sent" | "expired";

function hasExpiredLinkError() {
  const fragment = window.location.hash.startsWith("#")
    ? window.location.hash.slice(1)
    : window.location.hash;
  return new URLSearchParams(fragment).get("error_code") === "otp_expired";
}

function clearExpiredLinkFragment() {
  if (!hasExpiredLinkError()) return;
  window.history.replaceState(
    window.history.state,
    "",
    `${window.location.pathname}${window.location.search}`,
  );
}

export function SignIn(
  { returnTo, applicationName }: { returnTo?: string; applicationName?: string } = {},
) {
  const [email, setEmail] = useState("");
  const [screen, setScreen] = useState<SignInScreen>(() =>
    hasExpiredLinkError() ? "expired" : "entry",
  );
  const [error, setError] = useState<string | null>(null);
  const [resendError, setResendError] = useState<string | null>(null);
  const [resendMessage, setResendMessage] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [resendAvailableAt, setResendAvailableAt] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const fromApp = returnTo !== undefined;
  const appName = applicationName?.trim() || "An AI app";

  useEffect(() => {
    clearExpiredLinkFragment();
  }, []);

  useEffect(() => {
    if (screen !== "sent" || resendAvailableAt <= now) return;
    const interval = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [now, resendAvailableAt, screen]);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    setPending(true);
    setError(null);
    setResendError(null);
    try {
      const address = email.trim();
      await sendMagicLink(address, returnTo);
      setEmail(address);
      setResendAvailableAt(Date.now() + 60_000);
      setNow(Date.now());
      setResendMessage(null);
      setScreen("sent");
    } catch {
      setError("We couldn't send your link. Please try again. Your email is still here.");
    } finally {
      setPending(false);
    }
  }

  async function resend() {
    if (pending || Date.now() < resendAvailableAt) return;
    setPending(true);
    setResendError(null);
    setResendMessage("Sending a new link…");
    try {
      await sendMagicLink(email.trim(), returnTo);
      setResendAvailableAt(Date.now() + 60_000);
      setNow(Date.now());
      setResendMessage(`A new link is on its way to ${email}.`);
    } catch {
      setResendAvailableAt(0);
      setNow(Date.now());
      setResendMessage(null);
      setResendError("We couldn't send another link. Please try again.");
    } finally {
      setPending(false);
    }
  }

  function useDifferentEmail() {
    setEmail("");
    setError(null);
    setResendError(null);
    setResendMessage(null);
    setResendAvailableAt(0);
    setScreen("entry");
  }

  if (screen === "expired") {
    return (
      <IdentityFrame>
        <SignInContext fromApp={fromApp} appName={appName} />
        <h1 className="mb-4 text-[28px] font-medium leading-tight tracking-tight md:text-[32px]">
          This link has expired or was already used
        </h1>
        <p className="text-base leading-relaxed text-muted">
          Sign-in links work for a limited time and can only be used once. Get a new link to sign in.
        </p>
        <div className="mt-8">
          <button
            type="button"
            onClick={useDifferentEmail}
            className="min-h-12 w-full rounded-lg bg-signal px-4 py-3 text-sm font-semibold text-ink"
          >
            Get a new link
          </button>
        </div>
      </IdentityFrame>
    );
  }

  if (screen === "sent") {
    const remaining = Math.max(0, Math.ceil((resendAvailableAt - now) / 1000));
    const resendLabel = pending
      ? "Sending…"
      : remaining > 0
        ? `Send again in ${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, "0")}`
        : "Send again";

    return (
      <IdentityFrame>
        <SignInContext fromApp={fromApp} appName={appName} />
        <h1 className="mb-4 text-[28px] font-medium leading-tight tracking-tight md:text-[32px]">
          Check your inbox
        </h1>
        <p className="text-base leading-relaxed text-muted">A sign-in link is on its way.</p>

        <div className="my-6 border-y border-edge py-4 text-base [overflow-wrap:anywhere]">
          <span className="mb-1 block text-[10px] uppercase tracking-[0.12em] text-muted">Sent to</span>
          <span>{email}</span>
        </div>
        <p className="text-sm leading-relaxed text-paper">
          <strong className="font-medium">Open it in this browser</strong> to continue.
        </p>
        <p className="mt-3 text-xs leading-relaxed text-muted">If it hasn’t arrived, check your spam folder.</p>

        {resendError && (
          <p role="alert" className="mt-4 rounded-lg border border-edge px-4 py-3 text-sm text-paper">
            {resendError}
          </p>
        )}
        <div className="mt-6 flex flex-wrap items-center gap-x-6 gap-y-2">
          <button
            type="button"
            onClick={useDifferentEmail}
            className="min-h-11 px-0 text-left text-xs text-muted underline-offset-4 hover:underline"
          >
            Use a different email
          </button>
          <button
            type="button"
            disabled={pending || remaining > 0}
            onClick={() => void resend()}
            className="min-h-11 px-0 text-left text-xs text-muted underline-offset-4 hover:underline disabled:opacity-50 disabled:no-underline"
          >
            {resendLabel}
          </button>
        </div>
        <p role="status" className="mt-2 min-h-6 text-xs leading-relaxed text-muted">
          {resendMessage}
        </p>
      </IdentityFrame>
    );
  }

  return (
    <IdentityFrame>
      <SignInContext fromApp={fromApp} appName={appName} />
      <h1 className="mb-4 text-[28px] font-medium leading-tight tracking-tight md:text-[32px]">
        Sign in to Forma
      </h1>
      <p className="text-base leading-relaxed text-muted">
        Enter your email and we will send you a sign-in link. There is no password.
      </p>

      <form className="mt-8 grid gap-4" onSubmit={(event) => void submit(event)}>
        <div className="grid gap-2">
          <label htmlFor="signin-email" className="text-[13px] text-paper">
            Email
          </label>
          <input
            id="signin-email"
            type="email"
            autoComplete="email"
            inputMode="email"
            required
            autoFocus
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            placeholder="you@example.com"
            aria-describedby={error ? "signin-error" : "signin-helper"}
            aria-invalid={error ? true : undefined}
            className="min-h-12 w-full rounded-lg border border-edge bg-ink px-4 py-3 text-sm text-paper outline-none placeholder:text-muted focus:border-edge-strong"
          />
        </div>
        {error && (
          <div id="signin-error" role="alert" className="rounded-lg border border-edge px-4 py-3 text-sm text-paper">
            <strong className="mb-1 block font-medium">We couldn’t send your link.</strong>
            Please try again. Your email is still here.
          </div>
        )}
        <button
          type="submit"
          disabled={pending || email.trim().length === 0}
          className="min-h-12 w-full rounded-lg bg-signal px-4 py-3 text-sm font-semibold text-ink transition-opacity disabled:opacity-50"
        >
          {pending ? "Sending…" : "Send me a link"}
        </button>
        <p id="signin-helper" className="text-xs leading-relaxed text-muted">
          Open the link in this browser to sign in.
        </p>
        {pending && (
          <p role="status" className="text-xs text-muted">
            Sending your sign-in link…
          </p>
        )}
      </form>
    </IdentityFrame>
  );
}

function SignInContext({ fromApp, appName }: { fromApp: boolean; appName: string }) {
  if (!fromApp) return null;
  return (
    <div className="mb-8 border-b border-edge pb-6 text-sm leading-relaxed text-muted">
      <p>
        <strong className="font-medium text-paper">{appName} wants to connect to Forma.</strong> Sign in to continue.
      </p>
      <p className="mt-2 text-xs">After signing in, you’ll return to approve the connection.</p>
    </div>
  );
}

export function Onboard({ onReady }: { onReady: () => void }) {
  const [state, setState] = useState<Onboarding | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    getOnboarding()
      .then(setState)
      .catch((cause: unknown) =>
        setError(cause instanceof ApiError ? cause.message : "could not read your options"),
      );
  }, []);

  async function attempt(action: () => Promise<unknown>) {
    setPending(true);
    setError(null);
    try {
      await action();
      onReady();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "that did not work");
    } finally {
      setPending(false);
    }
  }

  async function leave() {
    setPending(true);
    setError(null);
    try {
      await signOut();
      onReady();
    } catch {
      setError("We couldn't sign you out. Please try again.");
    } finally {
      setPending(false);
    }
  }

  return (
    <IdentityFrame>
      <h1 className="mb-4 text-[28px] font-medium leading-tight tracking-tight md:text-[32px]">
        {state?.invited ? "One more step" : state ? "An invitation is needed" : "One more step"}
      </h1>
      {state?.email && (
        <p className="mb-6 text-sm leading-relaxed text-muted [overflow-wrap:anywhere]">
          Signed in as <span className="text-paper">{state.email}</span>.
        </p>
      )}

      {state?.invited ? (
        <>
          <p className="text-base leading-relaxed text-muted">
            Your workspace is home to your projects and designs. Nothing else to set up.
          </p>
          <div className="mt-8 grid gap-2">
            <button
              type="button"
              disabled={pending}
              onClick={() => void attempt(createAccount)}
              className="min-h-12 w-full rounded-lg bg-signal px-4 py-3 text-sm font-semibold text-ink disabled:opacity-50"
            >
              {pending ? "Creating…" : "Create my workspace"}
            </button>
            <button
              type="button"
              disabled={pending}
              onClick={() => void leave()}
              className="min-h-11 justify-self-start px-0 text-xs text-muted underline-offset-4 hover:underline disabled:opacity-50"
            >
              Sign out
            </button>
          </div>
        </>
      ) : state ? (
        <>
          <p className="text-base leading-relaxed text-muted">
            This Forma is invite-only. There’s no open invitation for this address.
          </p>
          <p className="mt-4 text-xs leading-relaxed text-muted">
            Ask the person who runs Forma to invite this email address.
          </p>
          <div className="mt-6 flex flex-wrap gap-x-6 gap-y-2">
            <button
              type="button"
              disabled={pending}
              onClick={() => void leave()}
              className="min-h-11 px-0 text-left text-xs text-muted underline-offset-4 hover:underline disabled:opacity-50"
            >
              Sign out
            </button>
            <button
              type="button"
              disabled={pending}
              onClick={() => void leave()}
              className="min-h-11 px-0 text-left text-xs text-muted underline-offset-4 hover:underline disabled:opacity-50"
            >
              Use a different email
            </button>
          </div>
        </>
      ) : (
        <p role="status" className="text-sm leading-relaxed text-muted">
          Checking your invitation…
        </p>
      )}

      {error && (
        <p role="alert" className="mt-4 rounded-lg border border-edge px-4 py-3 text-sm text-paper">
          {error}
        </p>
      )}
    </IdentityFrame>
  );
}

export function IdentityFrame({
  children,
  wide = false,
  footer = true,
}: {
  children: ReactNode;
  wide?: boolean;
  footer?: boolean;
}) {
  return (
    <div className="flex min-h-screen flex-col bg-ink text-paper">
      <header className="flex items-center gap-3 px-6 py-6 md:px-10 md:py-8">
        <svg
          aria-label="Forma"
          className="h-7 w-5 flex-none fill-current"
          role="img"
          viewBox="45.78614916286149 -710.0 520.812117438701 710.0"
        >
          <path d="M123.77092846270928 0V-542.8394216133942Q123.77092846270928 -622.0380517503805 166.74809741248097 -666.0190258751902Q209.72526636225265 -710 300.42770167427705 -710H415.099695585997V-597.8470319634703H333.3592085235921Q302.3592085235921 -597.8470319634703 288.35540334855403 -582.3432267884323Q274.351598173516 -566.8394216133942 274.351598173516 -536.0913242009133V0ZM45.78614916286149 -423.8318112633181V-535.9847792998478H407.351598173516V-423.8318112633181ZM403.80029296875 0V-153.1983642578125H566.5982666015625V0Z" />
        </svg>
        <span className="text-[11px] font-semibold tracking-[0.16em] text-signal">FORMA</span>
      </header>
      <main className="flex flex-1 items-start px-6 py-8 md:items-center md:px-10 md:py-8">
        <div className={`mx-auto w-full ${wide ? "max-w-[560px]" : "max-w-[432px]"}`}>
          {children}
        </div>
      </main>
      {footer && (
        <footer className="flex flex-wrap justify-between gap-2 px-6 py-6 text-[11px] text-muted md:px-10">
          <span>A workspace for your visual ideas.</span>
          <span>Invite-only</span>
        </footer>
      )}
    </div>
  );
}
