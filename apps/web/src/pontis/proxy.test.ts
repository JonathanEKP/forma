// @vitest-environment node
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import config from "../../vite.config";

/**
 * The browser's paths to the sidecar, and the Runtime's that must stay off the
 * internet (plan §11 step 10).
 *
 * Caddy and the Vite dev proxy are two spellings of one list, and nothing else
 * would notice them drifting: a route missing from one is a chat that works in
 * development and 404s in production, and a Runtime route added to either is
 * PV-2 shipped without anyone deciding it. The list is asserted here, against
 * the real files.
 */
const BROWSER = [
  "/pons/v1/pairing/codes",
  "/pons/v1/devices",
  "/pons/v1/devices/dev_1/revoke",
  "/pons/v1/events",
  "/pons/v1/jobs",
  "/pons/v1/jobs/job_1/cancel",
  "/pons/v1/ui/stream",
  "/pons/v1/ui/actions",
  "/pons/v1/ui/ack",
  "/agent/intent",
];

/** A remote Device reaching the sidecar, not a person at the editor (PV-2). */
const RUNTIME = [
  "/pons/v1/pairing/claim",
  "/pons/v1/device/challenge",
  "/pons/v1/device/auth",
  "/pons/v1/device/delegation",
  "/pons/v1/device/socket",
];

/** Near misses: a prefix match would let these through. */
const NEAR = ["/pons/v1/devicesx", "/pons/v1/eventsx", "/pons/v1/jobsx", "/agent/intentx", "/pons/v1/pairing"];

function viteMatches(path: string): boolean {
  const proxy = config.server?.proxy ?? {};
  return Object.keys(proxy)
    .filter((key) => key.startsWith("^"))
    .some((key) => new RegExp(key).test(path));
}

/** Caddy `path` matchers: `*` at the end is a prefix, anything else is exact. */
function caddyMatches(patterns: readonly string[], path: string): boolean {
  return patterns.some((pattern) =>
    pattern.endsWith("*") ? path.startsWith(pattern.slice(0, -1)) : path === pattern,
  );
}

const caddyfile = readFileSync(resolve(__dirname, "../../../../deploy/caddy/Caddyfile"), "utf8");

function matcher(name: string): string[] {
  const line = caddyfile.split("\n").find((l) => l.trim().startsWith(`@${name} path `));
  if (!line) throw new Error(`the Caddyfile has no @${name} matcher`);
  return line.trim().split(/\s+/).slice(2);
}

describe("the Pontis proxy routes", () => {
  const stream = matcher("pontis_stream");
  const api = matcher("pontis_api");
  const caddy = (path: string) => caddyMatches([...stream, ...api], path);

  it("routes every browser path in Caddy and in the Vite dev proxy", () => {
    for (const path of BROWSER) {
      expect(caddy(path), `Caddy: ${path}`).toBe(true);
      expect(viteMatches(path), `Vite: ${path}`).toBe(true);
    }
  });

  it("does not route the Runtime's paths, nor a near miss", () => {
    for (const path of [...RUNTIME, ...NEAR]) {
      expect(caddy(path), `Caddy: ${path}`).toBe(false);
      expect(viteMatches(path), `Vite: ${path}`).toBe(false);
    }
  });

  it("flushes the streams and only reaches the sidecar by loopback", () => {
    expect(stream).toEqual(["/pons/v1/events", "/pons/v1/ui/*"]);
    const block = caddyfile.slice(caddyfile.indexOf("handle @pontis_stream"));
    expect(block.slice(0, block.indexOf("}\n\t}") + 3)).toContain("flush_interval -1");
    expect(caddyfile).not.toMatch(/reverse_proxy (?!127\.0\.0\.1:)\S*4767/);
  });

  it("never names a Runtime route outside a comment", () => {
    const live = caddyfile
      .split("\n")
      .filter((line) => !line.trim().startsWith("#"))
      .join("\n");
    expect(live).not.toMatch(/pairing\/claim|\/pons\/v1\/device\//);
  });
});
