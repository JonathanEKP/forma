import { defineConfig } from "@playwright/test";

/**
 * End-to-end against the real product (plan section 67).
 *
 * Every other layer cuts the system somewhere: the Vitest suite mocks `fetch`,
 * the Rust integration tests have no browser, the MCP tests have no person.
 * This one fakes nothing — a real forma-server, the built React app it serves,
 * and a real browser clicking through it.
 *
 * That is the only way to catch the failure this layer exists for: the backend
 * can do something and the interface never exposes it. Two of those shipped
 * before this existed.
 */

const PORT = 4791;

export default defineConfig({
  testDir: "./tests",
  // One worker: the tests share one server and one database, and a second
  // worker creating projects underneath them would make failures unreadable.
  workers: 1,
  fullyParallel: false,
  // Locally a flake should be a failure, not a retry that hides it. CI retries
  // once, because a genuinely flaky E2E there costs a red build on a good
  // commit — and the retry count is itself the signal that one has crept in.
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["list"]] : [["list"]],

  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    // Kept only for a failure: a passing run should leave nothing behind.
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },

  /**
   * Starts the real server on a data directory of its own.
   *
   * `FORMA_DATA_DIR` points somewhere disposable so a run never touches the
   * developer's own projects, and starts empty so the walkthrough begins where
   * a new user does.
   */
  webServer: {
    command: "node scripts/serve.js",
    url: `http://127.0.0.1:${PORT}/api/v1/health`,
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
