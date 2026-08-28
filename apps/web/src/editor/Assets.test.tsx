import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Assets } from "./Assets";
import type { Asset, AssetRequest } from "../api";

/**
 * Driven through `fetch`, like the rest of the suite: that is the only thing
 * the editor shares with the server, so mocking our own client would only prove
 * the tests agree with themselves.
 */
interface Call {
  url: string;
  method: string;
}

function mockFetch(
  handler: (url: string, method: string) => { status?: number; body: unknown },
) {
  const calls: Call[] = [];

  const fetchMock = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
    const url =
      typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const method = init?.method ?? "GET";
    calls.push({ url, method });

    const { status = 200, body } = handler(url, method);
    return Promise.resolve(
      new Response(JSON.stringify(body), {
        status,
        headers: { "content-type": "application/json" },
      }),
    );
  });

  vi.stubGlobal("fetch", fetchMock);
  return calls;
}

function renderPanel(onPlace = vi.fn()) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  render(
    <QueryClientProvider client={client}>
      <Assets projectId="p1" onPlace={onPlace} disabled={false} />
    </QueryClientProvider>,
  );

  return onPlace;
}

const pendingRequest: AssetRequest = {
  id: "r1",
  project_id: "p1",
  design_id: "d1",
  purpose: "hero photo",
  description: "A futuristic coffee shop at night",
  suggested_prompt: "wide shot, neon, rain",
  requested_width: 1600,
  requested_height: 900,
  status: "pending",
  resolved_asset_id: null,
  created_at: "2026-08-28T00:00:00.000Z",
  resolved_at: null,
};

const uploaded: Asset = {
  id: "a1",
  project_id: "p1",
  hash: "abc",
  kind: "raster",
  mime_type: "image/png",
  width: 1600,
  height: 900,
  byte_size: 7753,
  storage_key: "assets/ab/c/abc.png",
  original_filename: "cafe.png",
  origin: "user_upload",
  attribution: null,
  created_at: "2026-08-28T00:00:00.000Z",
};

const file = () => new File(["bytes"], "cafe.png", { type: "image/png" });

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("the asset request flow", () => {
  it("shows what the agent asked for, including the prompt it suggested", async () => {
    mockFetch((url) => ({
      body: url.includes("asset-requests") ? [pendingRequest] : [],
    }));

    renderPanel();

    expect(await screen.findByText("hero photo")).toBeTruthy();
    expect(screen.getByText(/futuristic coffee shop/)).toBeTruthy();
    // Forma does not generate images, so the prompt is for the user to take
    // elsewhere and bring back.
    expect(screen.getByText("wide shot, neon, rain")).toBeTruthy();
    expect(screen.getByText(/1600×900/)).toBeTruthy();
  });

  it("uploads the image and resolves the request in that order", async () => {
    let resolved = false;
    const calls = mockFetch((url, method) => {
      if (url.includes("/resolve")) {
        resolved = true;
        return { body: { ...pendingRequest, status: "resolved", resolved_asset_id: "a1" } };
      }
      if (url.includes("/assets") && method === "POST") return { body: uploaded };
      if (url.includes("asset-requests")) {
        return { body: resolved ? [{ ...pendingRequest, status: "resolved" }] : [pendingRequest] };
      }
      return { body: [] };
    });

    renderPanel();
    await screen.findByText("hero photo");

    const drop = screen.getByText(/Drop the image here to resolve/);
    fireEvent.drop(drop, { dataTransfer: { files: [file()] } });

    await waitFor(() => expect(calls.some((c) => c.url.includes("/resolve"))).toBe(true));

    // The upload has to happen first: resolving needs an asset id.
    const upload = calls.findIndex((c) => c.url.includes("/assets") && c.method === "POST");
    const resolve = calls.findIndex((c) => c.url.includes("/resolve"));
    expect(upload).toBeGreaterThanOrEqual(0);
    expect(upload).toBeLessThan(resolve);

    // And the request stops being actionable once it is resolved.
    await waitFor(() => expect(screen.queryByText("hero photo")).toBeNull());
  });

  it("does not offer to resolve a request that is already settled", async () => {
    mockFetch((url) => ({
      body: url.includes("asset-requests")
        ? [
            { ...pendingRequest, id: "r2", status: "resolved" },
            { ...pendingRequest, id: "r3", status: "cancelled" },
          ]
        : [],
    }));

    renderPanel();

    await waitFor(() => expect(screen.getByText(/LIBRARY/)).toBeTruthy());
    expect(screen.queryByText("hero photo")).toBeNull();
    expect(screen.queryByText(/Drop the image here to resolve/)).toBeNull();
  });

  it("surfaces the server's code when an upload is refused", async () => {
    mockFetch((url, method) => {
      if (url.includes("/assets") && method === "POST") {
        return {
          status: 400,
          body: {
            error: {
              code: "INVALID_ASSET_TYPE",
              message: "that file is not an image",
              hint: "Upload a PNG, JPEG, WebP or SVG.",
            },
          },
        };
      }
      return { body: url.includes("asset-requests") ? [pendingRequest] : [] };
    });

    renderPanel();
    await screen.findByText("hero photo");

    fireEvent.drop(screen.getByText(/Drop the image here to resolve/), {
      dataTransfer: { files: [file()] },
    });

    // A generic failure would leave the user guessing which file to try next.
    expect(await screen.findByText("INVALID_ASSET_TYPE")).toBeTruthy();
    expect(screen.getByText(/not an image/)).toBeTruthy();
  });

  it("copies the suggested prompt", async () => {
    mockFetch((url) => ({
      body: url.includes("asset-requests") ? [pendingRequest] : [],
    }));
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal("navigator", { clipboard: { writeText } });

    renderPanel();
    fireEvent.click(await screen.findByText("Copy prompt"));

    expect(writeText).toHaveBeenCalledWith("wide shot, neon, rain");
    expect(await screen.findByText("Copied")).toBeTruthy();
  });
});

describe("the asset library", () => {
  it("places an asset through the same operation an agent would send", async () => {
    mockFetch((url) => ({
      body: url.includes("asset-requests") ? [] : [uploaded],
    }));

    const onPlace = renderPanel();
    fireEvent.click(await screen.findByTitle(/Place cafe.png/));

    expect(onPlace).toHaveBeenCalledWith(uploaded);
  });

  it("explains itself when there is nothing to show", async () => {
    mockFetch(() => ({ body: [] }));
    renderPanel();

    expect(await screen.findByText(/ask your agent for one/)).toBeTruthy();
  });
});
