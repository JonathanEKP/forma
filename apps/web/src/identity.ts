/**
 * The hosted identity provider, from the browser's side (plan section 110.2).
 *
 * Deliberately thin. Forma's own data never travels through this client: it
 * carries a session and nothing else, and the access token it holds is what
 * `api.ts` presents to `/api/v1/*`. There is no second Forma session cookie
 * layered on top — §106.2 is explicit, and an earlier attempt at one was
 * reverted.
 */

import { createClient, type Session, type SupabaseClient } from "@supabase/supabase-js";

const url = import.meta.env.VITE_SUPABASE_URL;
const publishableKey = import.meta.env.VITE_SUPABASE_PUBLISHABLE_KEY;

/**
 * `null` when this build has no provider configured, which is the local case.
 *
 * Both values are baked in at build time, so a local build simply has neither
 * and the editor never asks anyone to sign in to something that does not exist.
 */
export const supabase: SupabaseClient | null =
  url && publishableKey ? createClient(url, publishableKey) : null;

/**
 * The current access token, or `null`.
 *
 * Read from the client on each call rather than cached here: the library
 * refreshes sessions on its own, and a copy kept in this module would go stale
 * without anything saying so.
 */
export async function accessToken(): Promise<string | null> {
  if (!supabase) return null;
  const { data } = await supabase.auth.getSession();
  return data.session?.access_token ?? null;
}

/**
 * Who is signed in, which is not what the token is: it is refreshed under the
 * page and changes for the same person, so anything asking "is this still the
 * same person" compares this instead.
 */
export async function currentSubject(): Promise<string | null> {
  return (await currentSession())?.user.id ?? null;
}

/**
 * Calls `listener` whenever the provider says the session changed — signed in,
 * out, refreshed — and returns how to stop. The provider's own callback runs
 * inside its lock, so it is only a prompt to look again later, never a place to
 * ask it anything.
 */
export function onSessionChange(listener: () => void): () => void {
  if (!supabase) return () => undefined;
  const { data } = supabase.auth.onAuthStateChange(() => listener());
  return () => data.subscription.unsubscribe();
}

export async function currentSession(): Promise<Session | null> {
  if (!supabase) return null;
  const { data } = await supabase.auth.getSession();
  return data.session;
}

/**
 * Sends the magic link. The person continues in their inbox.
 *
 * `returnTo` exists for the consent page: a provider sends someone there with an
 * `authorization_id`, and coming back to the site root instead would lose the
 * authorization they were in the middle of.
 */
export async function sendMagicLink(email: string, returnTo?: string): Promise<void> {
  if (!supabase) throw new Error("this Forma has no identity provider configured");
  const { error } = await supabase.auth.signInWithOtp({
    email,
    options: { emailRedirectTo: returnTo ?? `${window.location.origin}/` },
  });
  if (error) throw new Error(error.message);
}

export async function signOut(): Promise<void> {
  await supabase?.auth.signOut();
}

// --- the OAuth server, from the consent page's side --------------------------

export type ConsentDetails = {
  authorization_id: string;
  redirect_uri: string;
  client: { id: string; name: string; uri: string; logo_uri: string };
  user: { id: string; email: string };
  scope: string;
};

/**
 * What the provider says about an authorization request.
 *
 * Two shapes, not one: a person who has already consented to these scopes is
 * sent straight back to the client, and showing them a consent screen again
 * would be asking a question that was already answered. The library narrows on
 * `authorization_id` being present, and so does this.
 */
export type Consent =
  | { kind: "decide"; details: ConsentDetails }
  | { kind: "already"; redirectUrl: string };

function client() {
  if (!supabase) throw new Error("this Forma has no identity provider configured");
  return supabase;
}

export async function authorizationDetails(authorizationId: string): Promise<Consent> {
  const { data, error } = await client().auth.oauth.getAuthorizationDetails(authorizationId);
  if (error) throw new Error(error.message);
  if (!data) throw new Error("the provider returned no authorization");

  return "authorization_id" in data
    ? { kind: "decide", details: data }
    : { kind: "already", redirectUrl: data.redirect_url };
}

/**
 * Approve or deny, and hand back where to send the person.
 *
 * `skipBrowserRedirect` is on so the redirect is ours to make and therefore
 * ours to observe: the library would navigate on its own, which works and which
 * no test can watch. The URL is used exactly as returned — never composed here,
 * because the code and state in it are the provider's to decide.
 */
export async function decide(
  authorizationId: string,
  allow: boolean,
): Promise<string> {
  const api = client().auth.oauth;
  const { data, error } = allow
    ? await api.approveAuthorization(authorizationId, { skipBrowserRedirect: true })
    : await api.denyAuthorization(authorizationId, { skipBrowserRedirect: true });

  if (error) throw new Error(error.message);
  if (!data) throw new Error("the provider returned no redirect");
  return data.redirect_url;
}

export type Grant = {
  client: { id: string; name: string; uri: string; logo_uri: string };
  scopes: string[];
  granted_at: string;
};

/**
 * The applications this person has authorized.
 *
 * Read from the provider, which owns them. Forma keeps no second grant
 * database: two records of who is connected is two records that can disagree,
 * and the one that would be wrong is ours.
 */
export async function listGrants(): Promise<Grant[]> {
  const { data, error } = await client().auth.oauth.listGrants();
  if (error) throw new Error(error.message);
  return data ?? [];
}

/**
 * Disconnects an application, immediately.
 *
 * The provider marks the consent revoked, deletes that client's active sessions
 * and invalidates its refresh tokens — so this is not a request that takes
 * effect eventually. Nothing else is touched: other grants, personal access
 * tokens and this browser's own session all keep working.
 */
export async function revokeGrant(clientId: string): Promise<void> {
  const { error } = await client().auth.oauth.revokeGrant({ clientId });
  if (error) throw new Error(error.message);
}
