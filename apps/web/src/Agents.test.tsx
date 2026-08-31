import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

const TOKEN = `forma_pat_${"a".repeat(64)}`;
const KEY = {
  id: "11111111-1111-4111-8111-111111111111",
  account_id: "00000000-0000-4000-8000-000000000001",
  name: "Codex CLI",
  token_prefix: "forma_pat_aaaaaaaa",
  created_at: "2026-08-30T12:00:00.000Z",
};

function renderAgents() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe("agent credentials", () => {
  let listed: Array<typeof KEY & { revoked_at?: string }>;

  beforeEach(() => {
    listed = [];
    window.history.replaceState({}, "", "/agents");

    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const url =
          typeof input === "string"
            ? input
            : input instanceof URL
              ? input.href
              : input.url;
        const method = init?.method ?? "GET";

        if (!url.includes("/api/v1/api-keys")) {
          return Promise.resolve(
            new Response(
              JSON.stringify({ error: { code: "NOT_FOUND", message: url } }),
              { status: 404, headers: { "content-type": "application/json" } },
            ),
          );
        }

        if (method === "POST") {
          listed = [KEY];
          return Promise.resolve(
            new Response(JSON.stringify({ key: KEY, token: TOKEN }), {
              status: 201,
              headers: { "content-type": "application/json" },
            }),
          );
        }

        if (method === "DELETE") {
          listed = [{ ...KEY, revoked_at: "2026-08-30T12:01:00.000Z" }];
          return Promise.resolve(new Response(null, { status: 204 }));
        }

        return Promise.resolve(
          new Response(JSON.stringify(listed), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
        );
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    window.history.replaceState({}, "", "/");
  });

  it("shows a new token once and can revoke its safe list entry", async () => {
    renderAgents();

    const oauth = screen.getByRole("heading", { name: "OAuth connections" });
    const pats = screen.getByRole("heading", { name: "Personal access tokens" });
    expect(oauth.compareDocumentPosition(pats) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    expect(await screen.findByText(/No personal access tokens yet/)).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Key name"), {
      target: { value: "Codex CLI" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create API key" }));

    expect(await screen.findByText(TOKEN)).toBeTruthy();
    expect(screen.getByText(/will not appear in the list again/i)).toBeTruthy();
    expect(await screen.findByText(/^forma_pat_aaaaaaaa… · created/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Revoke" }));
    await waitFor(() => expect(screen.getByText("Revoked")).toBeTruthy());
  });
});
