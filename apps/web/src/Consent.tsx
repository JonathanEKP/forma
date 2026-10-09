/**
 * Authorizing an agent (plan §111.3).
 *
 * The provider owns both the authorization question and the URL that follows
 * the answer. Forma explains the connection in terms of its own account
 * boundary and never treats provider identity scopes as Forma permissions.
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
import { IdentityFrame, SignIn } from "./SignIn";

type State =
  | { at: "loading" }
  | { at: "anonymous" }
  | { at: "decide"; details: ConsentDetails }
  | { at: "sending"; details: ConsentDetails; allow: boolean }
  | { at: "outcome"; details: ConsentDetails; allow: boolean; redirectUrl: string }
  | { at: "failed"; why: string };

function isLocalAddress(address: string) {
  try {
    const hostname = new URL(address).hostname.replace(/^\[|\]$/g, "").toLowerCase();
    return hostname === "localhost" || hostname === "127.0.0.1" || hostname === "::1";
  } catch {
    return false;
  }
}

export function Consent() {
  const authorizationId = new URLSearchParams(window.location.search).get("authorization_id");
  const [state, setState] = useState<State>({ at: "loading" });

  useEffect(() => {
    if (!authorizationId) {
      setState({ at: "failed", why: "This connection link is missing or has expired." });
      return;
    }

    let live = true;
    void (async () => {
      try {
        // The server decides whether this deployment has identity, not the
        // bundle's own configuration — a build can carry provider values while
        // the server it is served by runs in local mode.
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

        // Already answered: send them on without asking twice.
        if (consent.kind === "already") {
          window.location.assign(consent.redirectUrl);
          return;
        }
        setState({ at: "decide", details: consent.details });
      } catch {
        if (live) {
          setState({ at: "failed", why: "This connection link is missing or has expired." });
        }
      }
    })();
    return () => {
      live = false;
    };
  }, [authorizationId]);

  async function answer(allow: boolean) {
    if (!authorizationId || state.at !== "decide") return;
    const details = state.details;
    setState({ at: "sending", details, allow });
    try {
      // Exactly what the provider returned. Anything assembled here would be a
      // redirect the client did not ask for.
      const redirectUrl = await decide(authorizationId, allow);
      setState({ at: "outcome", details, allow, redirectUrl });
      window.location.assign(redirectUrl);
    } catch {
      setState({ at: "failed", why: "Your answer could not be recorded. Start the connection again from the application." });
    }
  }

  if (state.at === "loading") return null;

  // Signing in has to come back to this exact URL, authorization and all, or
  // the flow the provider started is lost.
  if (state.at === "anonymous") return <SignIn returnTo={window.location.href} />;

  if (state.at === "failed") {
    return (
      <IdentityFrame wide footer={false}>
        <h1 className="mb-4 text-[28px] font-medium leading-tight tracking-tight md:text-[32px]">
          This connection cannot be shown
        </h1>
        <p className="text-base leading-relaxed text-paper">{state.why}</p>
        <p className="mt-4 text-base leading-relaxed text-muted">
          Start the connection again from the application that sent you here.
        </p>
      </IdentityFrame>
    );
  }

  if (state.at === "outcome") {
    const { client } = state.details;
    return (
      <IdentityFrame wide footer={false}>
        <h1 className="mb-4 text-[28px] font-medium leading-tight tracking-tight md:text-[32px]">
          {state.allow ? "Connection allowed" : "Connection denied"}
        </h1>
        {state.allow ? (
          <>
            <p className="text-base leading-relaxed text-paper">
              Forma is connected to {client.name}.
            </p>
            <p className="mt-4 text-sm leading-relaxed text-muted">
              Your browser is going back to the app to finish connecting.
            </p>
            <dl className="mt-6 border-y border-edge py-6">
              <Detail label="Returns to" destination={state.details.redirect_uri} />
            </dl>
            <p className="mt-5 text-xs leading-relaxed text-muted">
              To disconnect later, open <a className="text-paper underline underline-offset-4" href="/agents/mcp">AI apps (MCP)</a>
              {" "}→ Connected applications. Disconnecting ends its access immediately.
            </p>
          </>
        ) : (
          <>
            <p className="text-base leading-relaxed text-paper">{client.name} was not connected.</p>
            <p className="mt-4 text-sm leading-relaxed text-muted">
              To try again, start the connection again from {client.name}.
            </p>
            <dl className="mt-6 border-y border-edge py-6">
              <Detail label="Returns to" destination={state.details.redirect_uri} />
            </dl>
          </>
        )}
        <a
          className="mt-5 inline-flex min-h-11 items-center text-xs text-muted underline underline-offset-4"
          href={state.redirectUrl}
        >
          Go back to {client.name}
        </a>
      </IdentityFrame>
    );
  }

  const { client, user } = state.details;
  const website = client.uri.trim();
  const sending = state.at === "sending";
  const answerInProgress = sending ? state.allow : null;

  return (
    <IdentityFrame wide footer={false}>
      <div aria-busy={sending}>
        <h1 className="mb-4 text-[28px] font-medium leading-tight tracking-tight md:text-[32px]">
          {client.name} wants to use Forma
        </h1>
        <p className="text-base leading-relaxed text-muted">
          Allow this app to use Forma’s design tools across <strong className="font-medium text-paper">all projects in your account</strong>.
        </p>

        <section className="mt-8" aria-labelledby="consent-capabilities">
          <h2 id="consent-capabilities" className="mb-3 text-base font-medium">
            What this app can do
          </h2>
          <ul className="grid gap-2 pl-6 text-base leading-relaxed marker:text-muted">
            <li>See your projects, Brand Kits and images</li>
            <li>Create and edit designs</li>
            <li>Make previews and exports</li>
            <li>Ask for images it needs</li>
          </ul>
          <p className="mt-4 text-sm leading-relaxed text-muted">
            It cannot create or delete projects, change a Brand Kit, or access anyone else’s Forma account.
          </p>
        </section>

        <dl className="mt-8 grid gap-5 border-y border-edge py-6">
          <Detail label="Application">{client.name}</Detail>
          {website && <Detail label="Website">{website}</Detail>}
          <Detail label="Returns to" destination={state.details.redirect_uri} />
          <Detail label="Signed in as">{user.email}</Detail>
        </dl>

        <p className="mt-4 text-xs leading-relaxed text-muted">
          The app supplies its name{website ? " and website" : ""}; Forma does not verify {website ? "them" : "it"}.{" "}
          {website ? (
            "Check that the return address matches the app you meant to connect."
          ) : (
            <strong className="font-medium text-paper">
              Check the return address above. Continue only if it belongs to the app you meant to connect.
            </strong>
          )}
        </p>

        <div className="mt-8 flex gap-2">
          <button
            type="button"
            disabled={sending}
            onClick={() => void answer(false)}
            className="min-h-12 flex-1 rounded-lg border border-edge px-4 py-3 text-sm disabled:opacity-50"
          >
            {answerInProgress === false ? "Sending…" : "Deny"}
          </button>
          <button
            type="button"
            disabled={sending}
            onClick={() => void answer(true)}
            className="min-h-12 flex-1 rounded-lg bg-signal px-4 py-3 text-sm font-semibold text-ink disabled:opacity-50"
          >
            {answerInProgress === true ? "Sending…" : "Allow"}
          </button>
        </div>
        <p role="status" className="mt-2 min-h-6 text-right text-xs leading-relaxed text-muted">
          {sending ? "Sending your answer…" : ""}
        </p>
        <p className="mt-3 text-xs leading-relaxed text-muted">
          You can disconnect this application later from <a className="text-paper underline underline-offset-4" href="/agents/mcp">AI apps (MCP)</a>, under Connected applications. Disconnecting ends its access immediately.
        </p>
      </div>
    </IdentityFrame>
  );
}

function Detail({
  label,
  children,
  destination,
}: {
  label: string;
  children?: string;
  destination?: string;
}) {
  return (
    <div className="grid min-w-0 grid-cols-1 gap-1 md:grid-cols-[112px_minmax(0,1fr)] md:gap-4">
      <dt className="text-sm text-muted">{label}</dt>
      <dd className="min-w-0 text-sm leading-relaxed text-paper [overflow-wrap:anywhere]">
        {destination ? (
          <>
            <span className="font-mono text-[13px] leading-relaxed [overflow-wrap:anywhere]">{destination}</span>
            {isLocalAddress(destination) && (
              <span className="mt-2 block text-xs leading-relaxed text-muted">
                A local address means the app runs on this computer.
              </span>
            )}
          </>
        ) : (
          children
        )}
      </dd>
    </div>
  );
}
