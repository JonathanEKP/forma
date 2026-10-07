import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ExportButton } from "./Export";
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

function renderButton(report?: ValidationReport) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  render(
    <QueryClientProvider client={client}>
      <ExportButton
        designId="d1"
        pageId="p1"
        pageCount={2}
        report={report}
        disabled={false}
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
  it("exports every page by default and offers the files", async () => {
    const calls = mockFetch();
    renderButton(clean);

    fireEvent.click(screen.getByText("Export"));
    fireEvent.click(screen.getByText("Export 2 pages"));

    await waitFor(() => expect(screen.getByText("01.png")).toBeTruthy());

    const post = calls.find((c) => c.method === "POST");
    expect(post?.body?.scope).toBe("all_pages");
    // Only a single-page export names a page.
    expect(post?.body?.page_id).toBeNull();

    // The version is shown because the export pinned it.
    expect(screen.getByText(/VERSION 7/)).toBeTruthy();
    expect(screen.getByText("02.png")).toBeTruthy();

    const link = screen.getByText("01.png").closest("a");
    expect(link?.getAttribute("href")).toBe("/api/v1/exports/e1/download?file=01.png");
    expect(link?.getAttribute("download")).toBe("01.png");
  });

  it("exports one page when asked", async () => {
    const calls = mockFetch();
    renderButton(clean);

    fireEvent.click(screen.getByText("Export"));
    fireEvent.click(screen.getByText("This page"));
    fireEvent.click(screen.getByText("Export this page"));

    await waitFor(() => expect(calls.some((c) => c.method === "POST")).toBe(true));
    const post = calls.find((c) => c.method === "POST");
    expect(post?.body?.scope).toBe("single_page");
    expect(post?.body?.page_id).toBe("p1");
  });

  it("passes the chosen scale", async () => {
    const calls = mockFetch();
    renderButton(clean);

    fireEvent.click(screen.getByText("Export"));
    fireEvent.click(screen.getByText("2×"));
    fireEvent.click(screen.getByText("Export 2 pages"));

    await waitFor(() => expect(calls.some((c) => c.method === "POST")).toBe(true));
    expect(calls.find((c) => c.method === "POST")?.body?.scale).toBe(2);
  });

  /// Plan section 42: errors mean the design is broken. A person may still
  /// decide to export, but not without being told and not by the same click.
  it("makes exporting a design with errors a deliberate act", () => {
    mockFetch();
    renderButton(broken);

    fireEvent.click(screen.getByText("Export"));

    expect(screen.getByText(/1 validation error/)).toBeTruthy();
    // The action names what it is doing rather than pretending nothing is wrong.
    expect(screen.getByText("Export anyway")).toBeTruthy();
    expect(screen.queryByText("Export 2 pages")).toBeNull();
  });

  it("says nothing about validation when there is nothing to say", () => {
    mockFetch();
    renderButton(clean);

    fireEvent.click(screen.getByText("Export"));
    expect(screen.queryByText(/validation error/)).toBeNull();
    expect(screen.getByText("Export 2 pages")).toBeTruthy();
  });
});
