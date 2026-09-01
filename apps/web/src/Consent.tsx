/**
 * Authorizing an agent (plan §111.3).
 *
 * The provider sends a person here with an `authorization_id`, and this page's
 * whole job is to let them decide knowingly. Two things it must not do: compose
 * a redirect URL of its own — the code and state in it are the provider's — and
 * imply that the scopes being requested are Forma permissions. They are not.
 * Supabase's scopes govern identity data; what a connection can reach in Forma
 * is decided by the account boundary, and that is what the wording explains.
 */

import { useEffect, useState } from "react";

import { getIdentity } from "./api";
import {
  authorizationDetails,
  currentSession,
  decide,
  supabase,
  type ConsentDetails,
} from "./identity";
import { SignIn } from "./SignIn";

type State =
  | { at: "loading" }
  | { at: "anonymous" }
  | { at: "decide"; details: ConsentDetails }
  | { at: "failed"; why: string };

export function Consent() {
  const authorizationId = new URLSearchParams(window.location.search).get("authorization_id");
  const [state, setState] = useState<State>({ at: "loading" });
  const [pending, setPending] = useState(false);

  useEffect(() => {
    if (!authorizationId) {
      setState({ at: "failed", why: "This link is missing its authorization." });
      return;
    }

    let live = true;
    void (async () => {
      try {
        // The server decides whether this deployment has identity, not the
        // bundle's own configuration — a build can carry provider values while
        // the server it is served by runs in local mode, and then an
        // authorization means nothing here.
        if (!(await getIdentity()).required || !supabase) {
          if (live) {
            setState({
              at: "failed",
              why: "This Forma has no identity provider, so there is no agent authorization to show.",
            });
          }
          return;
        }

        if (!(await currentSession())) {
          if (live) setState({ at: "anonymous" });
          return;
        }
        const consent = await authorizationDetails(authorizationId);
        if (!live) return;

        // Already answered: send them on rather than asking twice.
        if (consent.kind === "already") {
          window.location.assign(consent.redirectUrl);
          return;
        }
        setState({ at: "decide", details: consent.details });
      } catch (cause) {
        if (live) {
          setState({
            at: "failed",
            why: cause instanceof Error ? cause.message : "the authorization could not be read",
          });
        }
      }
    })();
    return () => {
      live = false;
    };
  }, [authorizationId]);

  async function answer(allow: boolean) {
    if (!authorizationId) return;
    setPending(true);
    try {
      // Exactly what the provider returned. Anything assembled here would be a
      // redirect the client did not ask for.
      window.location.assign(await decide(authorizationId, allow));
    } catch (cause) {
      setState({
        at: "failed",
        why: cause instanceof Error ? cause.message : "the decision could not be recorded",
      });
      setPending(false);
    }
  }

  if (state.at === "loading") return null;

  // Signing in has to come back to this exact URL, authorization and all, or
  // the flow the provider started is lost.
  if (state.at === "anonymous") return <SignIn returnTo={window.location.href} />;

  if (state.at === "failed") {
    return (
      <Frame title="This authorization cannot be shown">
        <p className="text-sm leading-6 text-muted">{state.why}</p>
        <p className="mt-3 text-xs text-faint">
          Start the connection again from the application that sent you here.
        </p>
      </Frame>
    );
  }

  const { client, user } = state.details;

  return (
    <Frame title={`${client.name} wants to use Forma`}>
      <p className="text-sm leading-6 text-muted">
        This connection can use Forma&rsquo;s agent tools on projects in your account, including
        reading branding and assets and creating or editing designs, previews and exports.
      </p>
      <p className="mt-3 text-sm leading-6 text-muted">
        It cannot access projects belonging to another Forma account.
      </p>

      <dl className="mt-5 space-y-1.5 rounded-xl border border-edge bg-surface px-3.5 py-3 text-xs">
        <Row label="Application">{client.name}</Row>
        {client.uri && <Row label="Website">{client.uri}</Row>}
        <Row label="Returns to">{state.details.redirect_uri}</Row>
        <Row label="Signed in as">{user.email}</Row>
      </dl>

      <div className="mt-4 flex gap-2">
        <button
          type="button"
          disabled={pending}
          onClick={() => void answer(false)}
          className="flex-1 rounded-xl border border-edge px-3.5 py-2.5 text-sm disabled:opacity-40"
        >
          Deny
        </button>
        <button
          type="button"
          disabled={pending}
          onClick={() => void answer(true)}
          className="flex-1 rounded-xl bg-signal px-3.5 py-2.5 text-sm font-medium text-ink disabled:opacity-40"
        >
          Allow
        </button>
      </div>

      <p className="mt-4 text-[11px] leading-relaxed text-faint">
        You can disconnect this application at any time from the Agents page. Doing so ends its
        access immediately.
      </p>
    </Frame>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex justify-between gap-4">
      <dt className="text-faint">{label}</dt>
      <dd className="truncate text-right text-paper">{children}</dd>
    </div>
  );
}

function Frame({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <main className="grid min-h-screen place-items-center px-6">
      <div className="w-full max-w-md">
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-signal">
          Forma
        </div>
        <h1 className="text-2xl font-medium tracking-tight">{title}</h1>
        <div className="mt-3">{children}</div>
      </div>
    </main>
  );
}
