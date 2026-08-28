import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Brand } from "./Brand";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

function mockFetch(brand: unknown) {
  const calls: Call[] = [];

  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      const method = init?.method ?? "GET";
      const body = typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : null;
      calls.push({ url, method, body });

      const payload = url.includes("/status")
        ? {
            name: "forma-server",
            version: "0.1.0",
            schema_version: 1,
            database: "sqlite",
            fonts: ["Geist", "Geist Mono"],
            projects: 1,
            max_ops_per_batch: 20,
          }
        : method === "PUT"
          ? { ...(body as object), id: "b1", project_id: "p1", version: 1 }
          : brand;

      return Promise.resolve(
        new Response(JSON.stringify(payload), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      );
    }),
  );

  return calls;
}

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  render(
    <QueryClientProvider client={client}>
      <Brand projectId="p1" />
    </QueryClientProvider>,
  );
}

const existing = {
  id: "b1",
  project_id: "p1",
  name: "Acme",
  colors: {
    primary: "#FF5436",
    secondary: "#FDBA2D",
    accent: "#7C3AED",
    background: "#101010",
    surface: "#191919",
    text: "#FFFFFF",
    text_inverse: "#101010",
    muted: "#A3A3A3",
  },
  typography: { display: "Geist", body: "Geist", mono: "Geist Mono" },
  logo_asset_id: null,
  voice: "Direct and brief.",
  rules: { safe_margin: 80, minimum_font_size: 24 },
  version: 3,
};

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("the brand panel", () => {
  it("says why a project without a kit cannot render tokens", async () => {
    mockFetch(null);
    renderPanel();

    // The consequence, not just the absence: this is the state that made a
    // design silently fail to render.
    expect(await screen.findByText(/cannot resolve/i)).toBeTruthy();
    expect(screen.getByText("Create a Brand Kit")).toBeTruthy();
  });

  it("loads the kit the project already has", async () => {
    mockFetch(existing);
    renderPanel();

    expect(await screen.findByDisplayValue("Acme")).toBeTruthy();
    expect(screen.getByDisplayValue("#FF5436")).toBeTruthy();
    expect(screen.getByDisplayValue("Direct and brief.")).toBeTruthy();
  });

  /// The rule that matters: a family the renderer lacks is FONT_NOT_FOUND at
  /// render time, so the choice is a list rather than a text field.
  it("only offers fonts the renderer can draw", async () => {
    mockFetch(existing);
    renderPanel();

    await screen.findByDisplayValue("Acme");
    const selects = screen.getAllByRole("combobox");
    expect(selects).toHaveLength(3);

    const options = [...selects[0].querySelectorAll("option")].map((o) => o.textContent);
    expect(options).toEqual(["Geist", "Geist Mono"]);
  });

  it("sends only what a brand is, never its identity or version", async () => {
    const calls = mockFetch(existing);
    renderPanel();

    await screen.findByDisplayValue("Acme");
    fireEvent.change(screen.getByDisplayValue("Acme"), { target: { value: "Rebrand" } });
    fireEvent.click(screen.getByText("Save Brand Kit"));

    await waitFor(() => expect(calls.some((c) => c.method === "PUT")).toBe(true));

    const put = calls.find((c) => c.method === "PUT");
    const body = put?.body as Record<string, unknown>;
    expect(body.name).toBe("Rebrand");
    // The server owns these; a client that sent them could churn the id or
    // reuse a version number.
    expect(body).not.toHaveProperty("id");
    expect(body).not.toHaveProperty("project_id");
    expect(body).not.toHaveProperty("version");
  });

  it("does not offer to save what has not changed", async () => {
    mockFetch(existing);
    renderPanel();

    await screen.findByDisplayValue("Acme");
    expect(screen.getByText("Saved")).toBeTruthy();

    fireEvent.change(screen.getByDisplayValue("Acme"), { target: { value: "Acme Ltd" } });
    expect(screen.getByText("Save Brand Kit")).toBeTruthy();
  });
});
