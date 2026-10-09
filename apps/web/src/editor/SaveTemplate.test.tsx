import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SaveAsTemplateButton } from "./SaveTemplate";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("saving a design as a template", () => {
  it("asks for a name, posts it to the design template route, and confirms quietly", async () => {
    let call: { url: string; method: string; body: unknown } | undefined;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
        call = {
          url,
          method: init?.method ?? "GET",
          body: typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : null,
        };
        return Promise.resolve(
          new Response(JSON.stringify({ id: "t1", project_id: "p1", name: "Spring cover", description: "", document: {}, roles: [], created_at: "2026-10-08T00:00:00Z" }), {
            status: 201,
            headers: { "content-type": "application/json" },
          }),
        );
      }),
    );
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });

    render(
      <QueryClientProvider client={client}>
        <SaveAsTemplateButton designId="d1" designName="Launch cover" projectId="p1" />
      </QueryClientProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Save as template" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Template name" }), {
      target: { value: "Spring cover" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save template" }));

    await waitFor(() => expect(call?.method).toBe("POST"));
    expect(call?.url).toBe("/api/v1/designs/d1/template");
    expect(call?.body).toEqual({ name: "Spring cover", description: "" });
    expect(await screen.findByText("Template saved.")).toBeTruthy();
  });
});
