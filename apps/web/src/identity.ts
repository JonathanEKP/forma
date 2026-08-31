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

export async function currentSession(): Promise<Session | null> {
  if (!supabase) return null;
  const { data } = await supabase.auth.getSession();
  return data.session;
}

/** Sends the magic link. The person continues in their inbox. */
export async function sendMagicLink(email: string): Promise<void> {
  if (!supabase) throw new Error("this Forma has no identity provider configured");
  const { error } = await supabase.auth.signInWithOtp({
    email,
    options: { emailRedirectTo: `${window.location.origin}/` },
  });
  if (error) throw new Error(error.message);
}

export async function signOut(): Promise<void> {
  await supabase?.auth.signOut();
}
