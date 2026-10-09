import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Projects } from "./Projects";

const project = {
  id: "p1",
  name: "Launch campaign",
  created_at: "2026-10-01T00:00:00.000Z",
  updated_at: "2026-10-01T00:00:00.000Z",
};

const summary = {
  id: "d1",
  project_id: "p1",
  name: "Launch cover",
  version: 12,
  format: { preset: "instagram_portrait", width: 1080, height: 1350 },
  page_count: 3,
  created_at: "2026-10-01T00:00:00.000Z",
  updated_at: "2026-10-08T10:00:00.000Z",
  actor_type: "agent",
  actor_id: "agent:claude-code",
  edited_at: new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString(),
  first_page_id: "page-1",
};

const template = {
  id: "t1",
  project_id: "p1",
  name: "Launch announcement",
  description: "",
  document: {
    schema_version: 1,
    format: { preset: "square", width: 1080, height: 1080 },
    pages: [{ id: "tp1", name: "Page 1", layers: [] }],
  },
  roles: [],
  created_at: "2026-10-01T00:00:00.000Z",
};

const status = {
  name: "forma-server",
  version: "0.1.0",
  schema_version: 1,
  database: "sqlite",
  fonts: ["Geist", "Inter"],
  projects: 1,
  max_ops_per_batch: 20,
};

function createBackend(options: {
  projects?: unknown[];
  designs?: unknown[];
  templates?: unknown[];
  assets?: unknown[];
  requests?: unknown[];
  unreachable?: boolean;
} = {}) {
  const calls: { path: string; method: string; body: unknown }[] = [];
  let unreachable = options.unreachable ?? false;
  const fetch = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://forma.test");
    const method = init?.method ?? "GET";
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    calls.push({ path: url.pathname, method, body });

    if (unreachable && url.pathname.endsWith("/status")) {
      return new Response("", { status: 503 });
    }

    if (method === "DELETE") return new Response(null, { status: 204 });
    if (url.pathname.endsWith("/preview.png")) {
      return new Response(new Blob(["png"]), { headers: { "content-type": "image/png" } });
    }
    if (method === "POST" && url.pathname === "/api/v1/projects") {
      return Response.json({ ...project, id: "p2", name: (body as { name: string }).name });
    }
    if (method === "POST" && url.pathname === "/api/v1/projects/p1/designs") {
      return Response.json({ id: "d2", project_id: "p1", name: "New card", version: 1 });
    }
    if (method === "POST" && url.pathname === "/api/v1/templates/t1/designs") {
      return Response.json({ id: "d3", project_id: "p1", name: "From template", version: 1 });
    }

    let value: unknown = null;
    if (url.pathname.endsWith("/status")) value = status;
    else if (url.pathname.endsWith("/projects")) value = options.projects ?? [project];
    else if (url.pathname === "/api/v1/projects/p1/designs") value = options.designs ?? [summary];
    else if (url.pathname === "/api/v1/projects/p1/brand") value = null;
    else if (url.pathname === "/api/v1/projects/p1/templates") value = options.templates ?? [];
    else if (url.pathname === "/api/v1/projects/p1/assets") value = options.assets ?? [];
    else if (url.pathname === "/api/v1/projects/p1/asset-requests") value = options.requests ?? [];

    return Response.json(value);
  });

  return { calls, fetch, restore: () => { unreachable = false; } };
}

function renderProjects(onOpen = vi.fn(), options: Parameters<typeof createBackend>[0] = {}) {
  const backend = createBackend(options);
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  vi.stubGlobal("fetch", backend.fetch);
  const view = render(
    <QueryClientProvider client={client}>
      <Projects onOpen={onOpen} />
    </QueryClientProvider>,
  );
  return { ...view, backend, onOpen };
}

function stubPreviewObjectUrls() {
  const TestURL = class extends URL {};
  Object.defineProperty(TestURL, "createObjectURL", { value: vi.fn(() => "blob:preview") });
  Object.defineProperty(TestURL, "revokeObjectURL", { value: vi.fn() });
  vi.stubGlobal("URL", TestURL);
  vi.stubGlobal("IntersectionObserver", undefined);
}

afterEach(() => vi.unstubAllGlobals());

describe("the projects home", () => {
  it("shows project details, design history, page-one preview and image requests", async () => {
    stubPreviewObjectUrls();
    const request = {
      id: "r1",
      project_id: "p1",
      design_id: "d1",
      purpose: "Campaign hero photo",
      description: "A bright street scene",
      status: "pending",
      created_at: new Date().toISOString(),
    };
    const { backend } = renderProjects(vi.fn(), { requests: [request] });

    fireEvent.click(await screen.findByRole("button", { name: /launch campaign/i }));

    expect(await screen.findByText((_, element) =>
      element?.tagName === "P" && element.textContent?.includes("1 design") === true,
    )).toBeTruthy();
    expect(screen.getByText(/no brand kit yet/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Set up" })).toBeTruthy();
    expect(screen.getByText("Campaign hero photo")).toBeTruthy();
    expect(screen.getByRole("link", { name: /campaign hero photo/i }).getAttribute("href"))
      .toBe("/?design=d1&panel=images");
    const card = (await screen.findByRole("button", { name: "Open Launch cover" })).closest("article");
    const cardText = card?.textContent?.replace(/\s+/gu, " ") ?? "";
    expect(cardText).toContain("v12 · 3 pages · 1080 × 1350");
    expect(cardText).toContain("Edited by Claude Code · 2 h ago");
    expect(screen.getByText("2 h ago", { selector: "time" }).className).toContain("whitespace-nowrap");
    const preview = await screen.findByRole("img", { name: /page 1 of launch cover/i });
    expect(preview.getAttribute("loading")).toBe("lazy");
    expect(preview.className).toContain("object-contain");
    expect(preview.parentElement?.className).toContain("h-[180px]");
    expect(preview.parentElement?.className).toContain("sm:h-[216px]");
    expect(preview.parentElement?.className).toContain("bg-surface");
    expect(preview.parentElement?.className).toContain("p-3");
    expect(screen.getByText("Page 1 preview")).toBeTruthy();
    fireEvent.load(preview);
    expect(screen.queryByText("Page 1 preview")).toBeNull();
    expect(backend.calls.some((call) => call.path === "/api/v1/designs/d1/pages/page-1/preview.png"))
      .toBe(true);
    expect(screen.getByText("Design details")).toBeTruthy();
  });

  it("shows the quiet placeholder when the page preview fails to load", async () => {
    stubPreviewObjectUrls();
    renderProjects();
    fireEvent.click(await screen.findByRole("button", { name: /launch campaign/i }));

    const preview = await screen.findByRole("img", { name: /page 1 of launch cover/i });
    fireEvent.error(preview);

    const placeholder = screen.getByText("Preview unavailable");
    expect(placeholder.className).toContain("text-faint");
    expect(screen.queryByRole("img", { name: /page 1 of launch cover/i })).toBeNull();
  });

  it("uses one fixed preview well for square, portrait, story, and landscape designs", async () => {
    const dimensions = [
      [1080, 1080],
      [1080, 1350],
      [1080, 1920],
      [1920, 1080],
    ];
    const designs = dimensions.map(([width, height], index) => ({
      ...summary,
      id: `d${index + 1}`,
      name: ["Square", "Portrait", "Story", "Landscape"][index],
      format: { preset: "custom", width, height },
      first_page_id: `page-${index + 1}`,
    }));
    renderProjects(vi.fn(), { designs });
    fireEvent.click(await screen.findByRole("button", { name: /launch campaign/i }));

    for (const name of ["Square", "Portrait", "Story", "Landscape"]) {
      const open = await screen.findByRole("button", { name: `Open ${name}` });
      const well = open.firstElementChild;
      expect(well?.className).toContain("h-[180px]");
      expect(well?.className).toContain("sm:h-[216px]");
      expect(well?.className).toContain("bg-surface");
    }
  });

  it("offers a blank design form with the requested format choices and opens the created design", async () => {
    const onOpen = vi.fn();
    const { backend } = renderProjects(onOpen);
    fireEvent.click(await screen.findByRole("button", { name: /launch campaign/i }));
    fireEvent.click(screen.getByRole("button", { name: "New design" }));
    fireEvent.change(screen.getByLabelText("Design name"), { target: { value: "Square launch" } });
    fireEvent.click(screen.getByRole("radio", { name: "Portrait" }));
    fireEvent.click(screen.getByRole("button", { name: "Create & open" }));

    await waitFor(() => expect(onOpen).toHaveBeenCalledWith("d2"));
    expect(backend.calls.find((call) => call.path === "/api/v1/projects/p1/designs" && call.method === "POST")?.body)
      .toEqual({ name: "Square launch", format: { preset: "instagram_portrait", width: null, height: null } });
  });

  it("uses the project's template endpoint when a template is selected", async () => {
    const onOpen = vi.fn();
    const { backend } = renderProjects(onOpen, { templates: [template] });
    fireEvent.click(await screen.findByRole("button", { name: /launch campaign/i }));
    fireEvent.click(screen.getByRole("button", { name: "New design" }));
    fireEvent.change(screen.getByLabelText("Design name"), { target: { value: "From template" } });
    fireEvent.change(await screen.findByRole("combobox", { name: "Start from a template" }), { target: { value: "t1" } });
    fireEvent.click(screen.getByRole("button", { name: "Create & open" }));

    await waitFor(() => expect(onOpen).toHaveBeenCalledWith("d3"));
    expect(backend.calls.some((call) => call.path === "/api/v1/templates/t1/designs" && call.method === "POST"))
      .toBe(true);
  });

  it("keeps design deletion on its card until confirmed and focuses Keep design", async () => {
    const { backend } = renderProjects();
    fireEvent.click(await screen.findByRole("button", { name: /launch campaign/i }));
    const open = await screen.findByRole("button", { name: "Open Launch cover" });
    const card = open.closest("article");
    if (!card) throw new Error("The design card should be an article.");
    fireEvent.click(within(card).getByRole("button", { name: /delete launch cover/i }));

    expect(within(card).getByText(/delete design/i)).toBeTruthy();
    const keep = within(card).getByRole("button", { name: "Keep design" });
    expect(document.activeElement).toBe(keep);
    fireEvent.click(within(card).getByRole("button", { name: "Delete design" }));

    await waitFor(() => expect(backend.calls.some((call) => call.path === "/api/v1/designs/d1" && call.method === "DELETE"))
      .toBe(true));
  });

  it("names project contents and requires its exact name before deletion", async () => {
    const asset = { id: "a1", project_id: "p1", kind: "image", content_hash: "x", original_filename: "photo.png", mime_type: "image/png", width: 10, height: 10, byte_size: 100, created_at: "2026-10-01T00:00:00.000Z" };
    const { backend } = renderProjects(vi.fn(), { templates: [template], assets: [asset] });
    fireEvent.click(await screen.findByRole("button", { name: /launch campaign/i }));
    fireEvent.click(screen.getByRole("button", { name: /delete project launch campaign/i }));

    expect(await screen.findByText(/1 design, 1 image and 1 template/i)).toBeTruthy();
    const submit = screen.getByRole("button", { name: "Delete project" });
    expect(submit.hasAttribute("disabled")).toBe(true);
    fireEvent.change(screen.getByLabelText(/type launch campaign to confirm/i), { target: { value: "Launch campaign" } });
    expect(submit.hasAttribute("disabled")).toBe(false);
    fireEvent.click(submit);
    await waitFor(() => expect(backend.calls.some((call) => call.path === "/api/v1/projects/p1" && call.method === "DELETE"))
      .toBe(true));
  });

  it("shows the first visit state", async () => {
    renderProjects(vi.fn(), { projects: [] });
    expect(await screen.findByRole("heading", { name: /start with a project/i })).toBeTruthy();
  });

  it("labels an unknown agent without exposing its id", async () => {
    renderProjects(vi.fn(), { designs: [{ ...summary, actor_id: "agent:mcp" }] });
    fireEvent.click(await screen.findByRole("button", { name: /launch campaign/i }));

    const actorLabel = await screen.findByText((_, element) =>
      element?.tagName === "SPAN" && element.textContent?.includes("Edited by An agent") === true,
    );
    expect(actorLabel.textContent).toContain("2 h ago");
    expect(screen.queryByText(/agent:mcp/i)).toBeNull();
  });

  it("shows server trouble and retries the status request", async () => {
    const { backend } = renderProjects(vi.fn(), { unreachable: true });
    expect(await screen.findByRole("heading", { name: "Forma can't connect right now" })).toBeTruthy();

    backend.restore();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("button", { name: "Open Launch cover" })).toBeTruthy();
  });
});
