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
        : url.endsWith("/projects/p1")
          ? { id: "p1", name: "Acme Studio", brand_id: "b1", created_at: "2026-01-01T00:00:00Z" }
          : url.endsWith("/projects/p1/assets")
            ? [
                {
                  id: "a1",
                  project_id: "p1",
                  hash: "abc",
                  kind: "raster",
                  mime_type: "image/png",
                  width: 400,
                  height: 200,
                  byte_size: 12,
                  storage_key: "a1.png",
                  original_filename: "brand-mark.png",
                  origin: "user_upload",
                  attribution: null,
                  created_at: "2026-01-01T00:00:00Z",
                },
              ]
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

    expect(await screen.findByRole("heading", { name: "No Brand Kit yet" })).toBeTruthy();
    expect(screen.getByText(/A Brand Kit keeps this project’s colours, fonts and voice together/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Set up Brand Kit" })).toBeTruthy();
    expect(screen.queryByText(/\$brand\./)).toBeNull();
  });

  it("loads the kit the project already has", async () => {
    mockFetch(existing);
    renderPanel();

    expect(await screen.findByDisplayValue("Acme")).toBeTruthy();
    expect(screen.getByDisplayValue("#FF5436")).toBeTruthy();
    expect(screen.getByDisplayValue("Direct and brief.")).toBeTruthy();
    expect(screen.getByText("Shared by every design in Acme Studio.")).toBeTruthy();
    for (const label of ["Primary", "Secondary", "Accent", "Background", "Surface", "Text", "Text on dark", "Muted"]) {
      expect(screen.getByLabelText(`${label} hex colour`)).toBeTruthy();
    }
    expect(screen.getByRole("combobox", { name: "Logo" }).querySelector("option[value=\"a1\"]")?.textContent).toBe("brand-mark.png");
  });

  /// The rule that matters: a family the renderer lacks is FONT_NOT_FOUND at
  /// render time, so the choice is a list rather than a text field.
  it("only offers fonts the renderer can draw", async () => {
    mockFetch(existing);
    renderPanel();

    await screen.findByDisplayValue("Acme");
    const selects = [
      screen.getByRole("combobox", { name: "Display font" }),
      screen.getByRole("combobox", { name: "Body font" }),
      screen.getByRole("combobox", { name: "Mono font" }),
    ];
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

  it("keeps Save disabled and quiet until a value changes", async () => {
    mockFetch(existing);
    renderPanel();

    await screen.findByDisplayValue("Acme");
    const save = screen.getByRole("button", { name: "Save Brand Kit" });
    expect((save as HTMLButtonElement).disabled).toBe(true);
    expect(save.className).not.toContain("bg-signal");
    expect(screen.getByText("Saving changes how every design in this project looks, right away.")).toBeTruthy();

    fireEvent.change(screen.getByDisplayValue("Acme"), { target: { value: "Acme Ltd" } });
    expect((save as HTMLButtonElement).disabled).toBe(false);
    expect(save.className).toContain("bg-signal");
  });
});
