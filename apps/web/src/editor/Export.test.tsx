import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ExportPanel } from "./Export";
import type { ValidationReport } from "../api";

interface Call {
  url: string;
  method: string;
  body: Record<string, unknown> | null;
}

function mockFetch() {
  const calls: Call[] = [];

  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      const body =
        typeof init?.body === "string"
          ? (JSON.parse(init.body) as Record<string, unknown>)
          : null;
      calls.push({ url, method: init?.method ?? "GET", body });

      return Promise.resolve(
        new Response(
          JSON.stringify({
            export: { id: "e1", design_version: 7 },
            files: [
              { filename: "01.png", byte_size: 20480 },
              { filename: "02.png", byte_size: 30720 },
            ],
          }),
          { status: 201, headers: { "content-type": "application/json" } },
        ),
      );
    }),
  );

  return calls;
}

function renderPanel(
  report?: ValidationReport,
  options: { pageCount?: number; onOpenChecks?: () => void; version?: number } = {},
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  render(
    <QueryClientProvider client={client}>
      <ExportPanel
        open
        designId="d1"
        designName="Cover"
        format={{ width: 1080, height: 1440 }}
        pageId="p1"
        pageCount={options.pageCount ?? 2}
        version={options.version ?? 7}
        report={report}
        disabled={false}
        onClose={vi.fn()}
        onOpenChecks={options.onOpenChecks ?? vi.fn()}
      />
    </QueryClientProvider>,
  );
}

const clean: ValidationReport = { valid: true, issues: [] };

const broken: ValidationReport = {
  valid: false,
  issues: [
    {
      code: "TEXT_OVERFLOW_WIDTH",
      severity: "error",
      page_id: "p1",
      layer_id: "l1",
      message: "needs 923px in an 864px box",
      details: null,
    },
  ],
};

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("exporting from the editor", () => {
  it("exports every page by default and offers one download per file", async () => {
    const calls = mockFetch();
    renderPanel(clean);

    expect(screen.getByText("Cover")).toBeTruthy();
    expect(screen.getByText("1080 × 1440 · portrait")).toBeTruthy();
    expect(screen.getByText("PNG · one file for each page.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "All 2" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "This page" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "1× · 1080 × 1440" })).toBeTruthy();
    expect(screen.getByText("The export keeps v7, exactly as it is when you start rendering.")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Export PNG" }));
    await waitFor(() => expect(screen.getByText("Your files are ready")).toBeTruthy());

    const post = calls.find((call) => call.method === "POST");
    expect(post?.body?.scope).toBe("all_pages");
    expect(post?.body?.page_id).toBeNull();
    expect(screen.getByText(/Made from v7/)).toBeTruthy();
    const links = screen.getAllByRole("link");
    expect(links.map((link) => link.textContent)).toEqual(["Download 01.png ↓", "Download 02.png ↓"]);
    expect(links[0].getAttribute("href")).toBe("/api/v1/exports/e1/download?file=01.png");
    expect(links[0].getAttribute("download")).toBe("01.png");
    expect(screen.getByRole("button", { name: "Export again" })).toBeTruthy();
  });

  it("exports just the current page when asked", async () => {
    const calls = mockFetch();
    renderPanel(clean);

    fireEvent.click(screen.getByRole("button", { name: "This page" }));
    fireEvent.click(screen.getByRole("button", { name: "Export PNG" }));

    await waitFor(() => expect(calls.some((call) => call.method === "POST")).toBe(true));
    expect(calls.find((call) => call.method === "POST")?.body?.scope).toBe("single_page");
    expect(calls.find((call) => call.method === "POST")?.body?.page_id).toBe("p1");
  });

  it("shows the resulting pixels for the selected size and sends that scale", async () => {
    const calls = mockFetch();
    renderPanel(clean);

    const twice = screen.getByRole("button", { name: "2× · 2160 × 2880" });
    fireEvent.click(twice);
    expect(twice.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Export PNG" }));

    await waitFor(() => expect(calls.some((call) => call.method === "POST")).toBe(true));
    expect(calls.find((call) => call.method === "POST")?.body?.scale).toBe(2);
  });

  it("lets a person review errors and deliberately export anyway", () => {
    mockFetch();
    const onOpenChecks = vi.fn();
    renderPanel(broken, { onOpenChecks });

    expect(screen.getByText("1 error needs attention. You can still export.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review errors →" }));
    expect(onOpenChecks).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Export anyway" })).toBeTruthy();
  });

  it("uses The page for a one-page design", () => {
    mockFetch();
    renderPanel(clean, { pageCount: 1 });

    expect(screen.getByText("The page")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "This page" })).toBeNull();
  });

  it("renders in an editor side panel, not a popover", () => {
    mockFetch();
    renderPanel(clean);

    expect(screen.getByRole("complementary", { name: "Export" })).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows a rendering state until the server returns the files", async () => {
    let finish: ((response: Response) => void) | undefined;
    vi.stubGlobal(
      "fetch",
      vi.fn((_input: RequestInfo | URL, init?: RequestInit) => {
        if (init?.method === "POST") {
          return new Promise<Response>((resolve) => {
            finish = resolve;
          });
        }
        return Promise.resolve(new Response(JSON.stringify(null), { status: 200, headers: { "content-type": "application/json" } }));
      }),
    );
    renderPanel(clean);

    fireEvent.click(screen.getByRole("button", { name: "Export PNG" }));
    expect(await screen.findByRole("heading", { name: "Rendering…" })).toBeTruthy();
    expect(screen.getByRole("progressbar", { name: "Rendering PNG files" })).toBeTruthy();

    act(() => {
      finish?.(
        new Response(
          JSON.stringify({ export: { id: "e1", design_version: 7 }, files: [{ filename: "01.png", byte_size: 1024 }] }),
          { status: 201, headers: { "content-type": "application/json" } },
        ),
      );
    });

    expect(await screen.findByText("Your files are ready")).toBeTruthy();
  });
});
