import { describe, expect, it } from "vitest";

import { createAgentBinding, createWindowId, randomId, type WindowIdEnvironment } from "./window";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;

/** A browser's storage and Web Locks, one object per tab; locks are shared by tabs of one `Locks`. */
class Locks {
  readonly held = new Set<string>();
  request(name: string, _options: { ifAvailable: boolean }, callback: (lock: object | null) => unknown): Promise<unknown> {
    if (this.held.has(name)) return Promise.resolve(callback(null));
    this.held.add(name);
    return Promise.resolve(callback({ name }));
  }
}

function tab(locks: Locks | null, storage: Map<string, string> | null, ids: string[] = []): WindowIdEnvironment {
  return {
    storage: storage && { getItem: (key) => storage.get(key) ?? null, setItem: (key, value) => void storage.set(key, value) },
    locks: locks as unknown as LockManager | null,
    random: () => ids.shift() ?? randomId(),
  };
}

describe("the window id", () => {
  it("is the same for as long as the tab lives, so every request names one window", async () => {
    const id = createWindowId(tab(new Locks(), new Map()));
    const first = await id();
    expect(first).toMatch(UUID);
    expect(await id()).toBe(first);
  });

  it("survives a reload, which is a new page with the same session storage", async () => {
    const storage = new Map<string, string>();
    const before = await createWindowId(tab(new Locks(), storage))();
    // The old page is gone, and the browser released its lock with it.
    const after = await createWindowId(tab(new Locks(), storage))();
    expect(after).toBe(before);
  });

  it("is not shared by a duplicated tab, which copies the storage but not the lock", async () => {
    const locks = new Locks();
    const original = await createWindowId(tab(locks, new Map()))();
    const copied = new Map([["forma.window", original]]);
    const duplicate = await createWindowId(tab(locks, copied))();
    expect(duplicate).not.toBe(original);
    expect(duplicate).toMatch(UUID);
    // And the duplicate keeps its own from then on, through its own reloads.
    expect(copied.get("forma.window")).toBe(duplicate);
  });

  it("falls back to the stored id where the browser has no Web Locks", async () => {
    const storage = new Map([["forma.window", "11111111-1111-4111-8111-111111111111"]]);
    expect(await createWindowId(tab(null, storage))()).toBe("11111111-1111-4111-8111-111111111111");
  });

  it("still gives one id to a page whose storage is blocked", async () => {
    const id = createWindowId(tab(new Locks(), null));
    expect(await id()).toMatch(UUID);
  });

  it("builds a UUID of the shape the sidecar accepts", () => {
    expect(randomId()).toMatch(UUID);
  });
});

describe("the agent binding", () => {
  it("tells subscribers when the chat's session changes, and not when it stays", () => {
    const binding = createAgentBinding();
    const seen: (string | null)[] = [];
    const leave = binding.subscribe(() => seen.push(binding.get()));
    binding.set("ses_a");
    binding.set("ses_a");
    binding.set(null);
    leave();
    binding.set("ses_b");
    expect(seen).toEqual(["ses_a", null]);
  });
});
