import { render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { App } from "./App";

afterEach(() => {
  vi.unstubAllGlobals();
});

test("renders the status the server reports", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({
        name: "forma-server",
        version: "0.1.0",
        schema_version: 1,
        database: "sqlite",
        fonts: ["DejaVu Sans"],
        projects: 0,
      }),
    }),
  );

  render(<App />);
  expect(await screen.findByText(/forma-server 0\.1\.0/)).toBeDefined();
  expect(await screen.findByText("DejaVu Sans")).toBeDefined();
});

test("surfaces the server error code instead of a blank screen", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: () => Promise.resolve({ error: { code: "STORAGE_ERROR", message: "database is locked" } }),
    }),
  );

  render(<App />);
  expect(await screen.findByRole("alert")).toBeDefined();
  expect(await screen.findByText(/STORAGE_ERROR/)).toBeDefined();
});
