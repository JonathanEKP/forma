/**
 * Which window this is, and which agent session its chat belongs to.
 *
 * Pontis routes an agent's `ui.*` action to a *window*, and registers one per
 * `(person, account, appSession)`. The sidecar builds the app session from an id
 * this window sends on every UI request, so the id decides what a second
 * connection does: the same id is a reload and replaces the registration, a
 * different one is another window and sits beside it. Without one every window
 * of a person shared a registration and closed each other's stream every two
 * seconds, forever (F-40-1, F-50-3).
 *
 * # What keeps the id stable
 *
 * `sessionStorage`, which is per tab and survives a reload — exactly the lifetime
 * wanted. It also survives *duplicating* a tab, which copies it, so two live tabs
 * could present one id and be back to evicting each other. A Web Lock named for
 * the id closes that: the tab that holds it for as long as it lives is the owner,
 * and a tab that cannot take it mints another. No timer is involved — the browser
 * releases the lock when the document goes. Where there is no Web Locks API the
 * stored id is used as is: duplicated tabs are then the one case that still
 * collide, which is no worse than before.
 */

const STORAGE_KEY = "forma.window";
const LOCK_PREFIX = "forma-window:";

/** The pieces the id depends on, so a test can supply its own browser. */
export interface WindowIdEnvironment {
  readonly storage: Pick<Storage, "getItem" | "setItem"> | null;
  readonly locks: LockManager | null;
  readonly random: () => string;
}

export function browserEnvironment(): WindowIdEnvironment {
  let storage: Storage | null = null;
  try {
    storage = window.sessionStorage;
  } catch {
    // Blocked storage: the id then lasts for this page load, which is what a
    // reload-proof id degrades to and is still one registration per window.
  }
  return {
    storage,
    locks: typeof navigator !== "undefined" && "locks" in navigator ? navigator.locks : null,
    random: randomId,
  };
}

/** A UUID, the format the sidecar accepts, from whatever the page can reach. */
export function randomId(): string {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function remembered(storage: WindowIdEnvironment["storage"]): string | null {
  try {
    return storage?.getItem(STORAGE_KEY) ?? null;
  } catch {
    return null;
  }
}

function remember(storage: WindowIdEnvironment["storage"], id: string): void {
  try {
    storage?.setItem(STORAGE_KEY, id);
  } catch {
    // See `browserEnvironment`.
  }
}

/** Takes the lock for `id` and keeps it until the document goes; `false` if another tab has it. */
function hold(locks: LockManager, id: string): Promise<boolean> {
  return new Promise((resolve) => {
    void locks
      .request(`${LOCK_PREFIX}${id}`, { ifAvailable: true }, (lock) => {
        resolve(lock !== null);
        // Never settles: the lock is released when the page is destroyed.
        return lock === null ? undefined : new Promise<void>(() => undefined);
      })
      .catch(() => resolve(false));
  });
}

export function createWindowId(environment: WindowIdEnvironment): () => Promise<string> {
  let claimed: Promise<string> | undefined;

  const claim = async (): Promise<string> => {
    const stored = remembered(environment.storage);
    if (stored !== null) {
      if (environment.locks === null || (await hold(environment.locks, stored))) return stored;
    }
    const fresh = environment.random();
    remember(environment.storage, fresh);
    if (environment.locks !== null) await hold(environment.locks, fresh);
    return fresh;
  };

  return () => (claimed ??= claim());
}

/** This tab's id, settled once and the same for as long as the page lives. */
export const windowId: () => Promise<string> = createWindowId(browserEnvironment());

/**
 * The agent session this window's chat is attached to, for the stream to name.
 *
 * Pontis binds an action to the window whose chat started the session (D-2), and
 * the only way it can know which is for the window to say so when it opens its
 * stream. The chat sets it; the channel reads it and reopens the stream when it
 * changes. A small store of its own, like `liveFeed`, so neither knows the other.
 */
export interface AgentBinding {
  get(): string | null;
  set(sessionId: string | null): void;
  subscribe(listener: () => void): () => void;
}

export function createAgentBinding(): AgentBinding {
  let current: string | null = null;
  const listeners = new Set<() => void>();
  return {
    get: () => current,
    set: (sessionId) => {
      if (sessionId === current) return;
      current = sessionId;
      for (const listener of [...listeners]) listener();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
  };
}

export const agentBinding: AgentBinding = createAgentBinding();
