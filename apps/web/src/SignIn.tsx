/**
 * Signing in, and the two doors out of having no account (plan §110.2, §107.3,
 * §113.1).
 *
 * One field, because the provider offers one way in: a link sent to an address
 * it then proves. That proof is what the invite gate rests on — an unverified
 * address would let anyone register as an invited one — so nothing here tries
 * to be cleverer than "tell us where to send it".
 */

import { useEffect, useState } from "react";

import { ApiError, createAccount, getOnboarding, type Onboarding } from "./api";
import { sendMagicLink, signOut } from "./identity";

export function SignIn({ returnTo }: { returnTo?: string } = {}) {
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    try {
      await sendMagicLink(email.trim(), returnTo);
      setSent(true);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "the link could not be sent");
    } finally {
      setPending(false);
    }
  }

  if (sent) {
    return (
      <Frame title="Check your inbox">
        <p className="text-sm leading-6 text-muted">
          A sign-in link is on its way to <span className="text-paper">{email}</span>. Open it in
          this browser to continue.
        </p>
      </Frame>
    );
  }

  return (
    <Frame title="Sign in to Forma">
      <form onSubmit={(event) => void submit(event)}>
        <p className="text-sm leading-6 text-muted">
          Enter your email and we will send you a sign-in link. There is no password.
        </p>
        <input
          type="email"
          required
          autoFocus
          value={email}
          onChange={(event) => setEmail(event.target.value)}
          placeholder="you@example.com"
          aria-label="Email address"
          className="mt-5 w-full rounded-xl border border-edge bg-surface px-3.5 py-2.5 text-sm outline-none placeholder:text-faint focus:border-edge-strong"
        />
        <button
          type="submit"
          disabled={pending || email.trim().length === 0}
          className="mt-2.5 w-full rounded-xl bg-signal px-3.5 py-2.5 text-sm font-medium text-ink transition-opacity disabled:opacity-40"
        >
          {pending ? "Sending…" : "Send me a link"}
        </button>
        {error && (
          <p role="alert" className="mt-3 text-xs text-signal">
            {error}
          </p>
        )}
      </form>
    </Frame>
  );
}

/**
 * A verified identity that owns nothing yet.
 *
 * One door: an invitation. Creating an account *is* getting a workspace —
 * an account owns projects, and that relation is the ownership, so there is
 * nothing separate left to claim.
 *
 * There used to be a second door asking a person to paste the deployment's own
 * token into a browser field. It existed because accounts did not, when the
 * deployed database was made; they do now, so it had become a toll on the way to
 * something the account already gives you.
 *
 * Without an invitation this says so plainly rather than looking broken: an
 * invite-only deployment refusing a stranger is working correctly.
 */
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

  return (
    <Frame title="One more step">
      {state?.email && (
        <p className="text-sm leading-6 text-muted">
          Signed in as <span className="text-paper">{state.email}</span>.
        </p>
      )}

      {state?.invited ? (
        <button
          type="button"
          disabled={pending}
          onClick={() => void attempt(createAccount)}
          className="mt-5 w-full rounded-xl bg-signal px-3.5 py-2.5 text-sm font-medium text-ink disabled:opacity-40"
        >
          Create my workspace
        </button>
      ) : (
        <p className="mt-5 text-sm leading-6 text-muted">
          This Forma is invite-only, and there is no open invitation for this address. Ask whoever
          runs it for one.
        </p>
      )}

      {error && (
        <p role="alert" className="mt-4 text-xs text-signal">
          {error}
        </p>
      )}

      <button
        type="button"
        onClick={() => void signOut().then(onReady)}
        className="mt-6 text-xs text-faint underline underline-offset-4"
      >
        Sign out
      </button>
    </Frame>
  );
}

function Frame({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <main className="grid min-h-screen place-items-center px-6">
      <div className="w-full max-w-sm">
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-signal">
          Forma
        </div>
        <h1 className="text-2xl font-medium tracking-tight">{title}</h1>
        <div className="mt-3">{children}</div>
      </div>
    </main>
  );
}
