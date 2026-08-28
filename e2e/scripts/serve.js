/**
 * Starts forma-server for the end-to-end run.
 *
 * A script rather than a shell one-liner in the config so the data directory is
 * created fresh and removed on the way out: a run that inherited yesterday's
 * projects would not be starting where a new user starts.
 */

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = resolve(here, "../..");
const dataDir = mkdtempSync(join(tmpdir(), "forma-e2e-"));

const binary = process.env.FORMA_BINARY ?? resolve(root, "target/debug/forma-server");

const server = spawn(binary, [], {
  stdio: "inherit",
  env: {
    ...process.env,
    FORMA_BIND: "127.0.0.1:4791",
    FORMA_DATA_DIR: dataDir,
    FORMA_DATABASE_URL: `sqlite://${join(dataDir, "forma.db")}`,
    // The fonts and the built app come from the repository, not the temp dir.
    FORMA_FONT_DIR: resolve(root, "data/fonts"),
    FORMA_WEB_DIR: resolve(root, "apps/web/dist"),
    FORMA_LOG: process.env.FORMA_LOG ?? "warn",
  },
});

const cleanUp = () => {
  server.kill("SIGTERM");
  try {
    rmSync(dataDir, { recursive: true, force: true });
  } catch {
    // The directory is in the OS temp area; leaving it is not worth a failure.
  }
};

process.on("SIGINT", () => { cleanUp(); process.exit(0); });
process.on("SIGTERM", () => { cleanUp(); process.exit(0); });
process.on("exit", cleanUp);

server.on("exit", (code) => process.exit(code ?? 0));
