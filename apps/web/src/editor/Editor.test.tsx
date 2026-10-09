import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Editor } from "./Editor";
import { useEditor } from "../store";
import type { DesignRecord } from "../types";

const record: DesignRecord = {
  id: "d1",
  project_id: "p1",
  name: "Launch cover",
  version: 1,
  created_at: "2026-10-08T00:00:00Z",
  updated_at: "2026-10-08T00:00:00Z",
  document: {
    schema_version: 1,
    format: { preset: "square", width: 1080, height: 1080 },
    pages: [
      {
        id: "page1",
        name: "Cover",
        layers: [
          {
            type: "text",
            id: "layer1",
            name: "Headline",
            text: "Hello",
            font_family: "Geist",
            font_size: 48,
            font_weight: 500,
            line_height: 1.2,
            letter_spacing: 0,
            align: "left",
            color: "#171717",
            visible: true,
            locked: false,
            opacity: 1,
            transform: { x: 20, y: 20, width: 600, height: 120, rotation_deg: 0 },
          },
        ],
      },
    ],
  },
};

function mockFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      if (url.includes("/preview.svg")) {
        return Promise.resolve(new Response("<svg xmlns=\"http://www.w3.org/2000/svg\"/>", { status: 200 }));
      }
      const payload = url.endsWith("/status")
        ? { name: "forma-server", version: "0.1.0", schema_version: 1, database: "sqlite", fonts: ["Geist", "Geist Mono"], projects: 1, max_ops_per_batch: 20 }
        : url.endsWith("/validate")
          ? {
              valid: false,
              issues: [
                {
                  code: "TEXT_OVERFLOW_WIDTH",
                  severity: "error",
                  page_id: "page1",
                  layer_id: "layer1",
                  message: "Headline needs more width",
                  details: null,
                },
              ],
            }
          : url.endsWith("/projects/p1/brand")
            ? null
            : url.endsWith("/projects/p1")
              ? { id: "p1", name: "Acme Studio", brand_id: null, created_at: "2026-10-08T00:00:00Z" }
              : url.includes("/asset-requests") || url.endsWith("/projects/p1/assets")
                ? []
                : record;
      return Promise.resolve(new Response(JSON.stringify(payload), { status: 200, headers: { "content-type": "application/json" } }));
    }),
  );
}

function renderEditor(initialTool?: "layers" | "images" | "brand") {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Editor designId="d1" onBack={vi.fn()} initialTool={initialTool} />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  useEditor.setState({ openDesignId: null, panel: "properties", selectedPageId: null, selectedLayerId: null, draft: null, notice: null, zoom: 1 });
  vi.unstubAllGlobals();
});

describe("editor side panels", () => {
  it("opens the requested tool on first load", async () => {
    mockFetch();
    renderEditor("images");

    expect(await screen.findByRole("complementary", { name: "Images" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Images" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("opens Checks from the top bar as a side panel and selects its target", async () => {
    mockFetch();
    renderEditor("layers");

    const chip = await screen.findByRole("button", { name: "Open checks" });
    fireEvent.click(chip);

    expect(screen.getByRole("complementary", { name: "Checks" })).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(await screen.findByRole("button", { name: /Headline · Cover/ }));
    await waitFor(() => expect(useEditor.getState().selectedPageId).toBe("page1"));
    expect(useEditor.getState().selectedLayerId).toBe("layer1");
  });
});
