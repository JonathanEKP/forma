/**
 * What a tab remembers of its chat across a reload (D-1).
 *
 * `sessionStorage` rather than `localStorage`, on purpose: it is per tab, which is
 * exactly the scope of "one chat per window", and it dies with the tab. It holds
 * the session's id, the Device it runs on and what the person said — the one part
 * of a transcript no durable event brings back — and never a token, a header or
 * anything the agent answered. It is keyed by the application context, so a
 * different person signing in to the same tab does not inherit it, and
 * `signOut` clears it outright.
 *
 * Every access is guarded: storage can be absent, full or refused (private
 * windows, blocked site data), and a chat that cannot remember still has to work.
 */
import { isRecord } from "./events";
import type { Remembered, Said, ViewChip } from "./chat";

const KEY = "forma.chat";
const DRAFT_KEY = "forma.chat.draft";

export interface Store {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

function browserStore(): Store | null {
  try {
    return globalThis.sessionStorage;
  } catch {
    return null;
  }
}

function chip(raw: unknown): ViewChip | undefined {
  if (!isRecord(raw)) return undefined;
  const text = (value: unknown) => (typeof value === "string" ? value : undefined);
  const parsed: ViewChip = {
    ...(text(raw.design) === undefined ? {} : { design: text(raw.design) }),
    ...(text(raw.page) === undefined ? {} : { page: text(raw.page) }),
    ...(text(raw.layer) === undefined ? {} : { layer: text(raw.layer) }),
  };
  return Object.keys(parsed).length > 0 ? parsed : undefined;
}

function said(raw: unknown): Said | null {
  if (!isRecord(raw) || typeof raw.id !== "string" || typeof raw.text !== "string") return null;
  if (typeof raw.afterSeq !== "number" || !Number.isSafeInteger(raw.afterSeq) || raw.afterSeq < 0) return null;
  const view = chip(raw.chip);
  return { id: raw.id, text: raw.text, afterSeq: raw.afterSeq, sessionId: typeof raw.sessionId === "string" ? raw.sessionId : null, ...(view ? { chip: view } : {}) };
}

/**
 * What this window remembered for `context`, or `null` when it has nothing, or
 * nothing of this person's, or only what a tab it was copied from left behind.
 */
export function recall(context: string, window: string, store: Store | null = browserStore()): Remembered | null {
  try {
    const raw = store?.getItem(KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed) || parsed.context !== context || parsed.window !== window || !isRecord(parsed.remembered)) return null;
    const { sessionId, deviceId, dead, said: lines } = parsed.remembered;
    if (typeof sessionId !== "string" || sessionId.length === 0) return null;
    return {
      sessionId,
      deviceId: typeof deviceId === "string" ? deviceId : null,
      dead: typeof dead === "string" ? dead : null,
      said: Array.isArray(lines) ? (lines as unknown[]).map(said).filter((line): line is Said => line !== null) : [],
    };
  } catch {
    return null;
  }
}

/** Keeps `remembered` for `context`, or forgets everything when there is nothing to keep. */
export function remembering(context: string, window: string, remembered: Remembered | null, store: Store | null = browserStore()): void {
  try {
    if (remembered === null) store?.removeItem(KEY);
    else store?.setItem(KEY, JSON.stringify({ context, window, remembered }));
  } catch {
    // A tab that cannot remember is a tab whose reload starts clean: no worse than before.
  }
}

/** The unsent text in this tab. Unlike chat history, it has no server counterpart. */
export function recallDraft(store: Store | null = browserStore()): string {
  try {
    return store?.getItem(DRAFT_KEY) ?? "";
  } catch {
    return "";
  }
}

/** Keep an unsent message across a reload, or remove it once it is sent. */
export function rememberDraft(text: string, store: Store | null = browserStore()): void {
  try {
    if (text.length === 0) store?.removeItem(DRAFT_KEY);
    else store?.setItem(DRAFT_KEY, text);
  } catch {
    // A tab whose storage is blocked remains usable for this page load.
  }
}

/** Ends the tab's chat: called on sign-out, because the conversation was the signed-in person's. */
export function forgetChat(store: Store | null = browserStore()): void {
  try {
    store?.removeItem(KEY);
    store?.removeItem(DRAFT_KEY);
  } catch {
    // Nothing to forget in a store that cannot be reached.
  }
}
