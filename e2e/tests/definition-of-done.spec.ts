import { expect, test, type Page } from "@playwright/test";
import { fileURLToPath } from "node:url";

/**
 * The Definition of Done of plan section 79, walked in a browser.
 *
 * Section 79 lists twenty steps. Six of them are the agent's — connect over
 * MCP, query the brand, create, validate, preview, export — and those are
 * already covered against the real binaries in `apps/mcp/tests/agent_flow.rs`.
 * Repeating them here would be slower and prove less. What this covers is the
 * half no other layer can reach: a person, in a browser, going from an empty
 * Forma to a PNG on disk, and the moment where their edit collides with an
 * agent's.
 *
 * Step 20 — that Forma calls no LLM API — is an architectural property, not
 * something a click can demonstrate.
 */

const HERO = fileURLToPath(new URL("../../fixtures/assets/hero.png", import.meta.url));
const LOCAL_CSP = [
  "default-src 'self';",
  "script-src 'self';",
  "style-src 'self';",
  "style-src-attr 'unsafe-inline';",
  "img-src 'self' data: blob:;",
  "font-src 'self';",
  "connect-src 'self';",
  "frame-ancestors 'none';",
  "base-uri 'none';",
  "object-src 'none';",
].join(" ");

function watchForCspViolations(page: Page): string[] {
  const violations: string[] = [];
  page.on("console", (message) => {
    if (
      message.type() === "error" &&
      /content security policy|violates the following content security policy directive/i.test(
        message.text(),
      )
    ) {
      violations.push(message.text());
    }
  });
  return violations;
}

test("every hosted SPA entry point reaches the application shell", async ({ page }) => {
  const cspViolations = watchForCspViolations(page);
  for (const [path, heading] of [
    ["/", "Give visual ideas a form."],
    ["/login", "Sign in to Forma"],
    // Local has no provider, so the consent page says that rather than
    // offering a sign-in that could never complete.
    ["/oauth/consent?authorization_id=request-1", "This authorization cannot be shown"],
    ["/agents", "Agent connections"],
  ]) {
    const response = await page.goto(path);
    expect(response?.status(), `${path} did not serve the SPA`).toBe(200);
    expect(response?.headers()["content-security-policy"]).toBe(LOCAL_CSP);
    await expect(page.getByRole("heading", { name: heading })).toBeVisible();
  }
  expect(cspViolations, "the application emitted a CSP violation").toEqual([]);
});

/** Section 79 steps 3 and 4: a project, and the Brand Kit everything assumes. */
async function createProjectWithBrand(page: Page, name: string) {
  await page.goto("/");

  await page.getByPlaceholder("New project").fill(name);
  await page.getByPlaceholder("New project").press("Enter");
  await page.getByRole("button", { name: new RegExp(name) }).click();

  // A project starts with no Brand Kit, and says what that costs.
  await expect(page.getByText(/No Brand Kit yet/)).toBeVisible();

  await page.getByRole("button", { name: "Set up" }).click();
  await page.getByRole("button", { name: "Create a Brand Kit" }).click();
  await page.getByRole("button", { name: "Save Brand Kit" }).click();
  await expect(page.getByRole("button", { name: "Saved" })).toBeVisible();
  await page.getByRole("button", { name: "Close" }).click();
}

async function createDesign(page: Page, name: string) {
  await page.getByPlaceholder("New design").fill(name);
  await page.getByPlaceholder("New design").press("Enter");
  // Creating a design opens it.
  await expect(page.getByRole("button", { name: "← Projects" })).toBeVisible();
}

/// A page nothing links to is a page most people do not have.
test("the agents page can be reached by navigating, not only by knowing its URL", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("link", { name: "Connect an agent" }).click();
  await expect(page.getByRole("heading", { name: "Agent connections" })).toBeVisible();

  await page.getByRole("link", { name: "← Projects" }).click();
  await expect(page.getByRole("heading", { name: "Give visual ideas a form." })).toBeVisible();
});

test("a new user gets from an empty Forma to a PNG on disk", async ({ page }) => {
  const cspViolations = watchForCspViolations(page);
  await createProjectWithBrand(page, "Launch");

  // --- step 5: upload a photo ------------------------------------------------
  // Done from the editor's assets panel, so a design has to exist first.
  await createDesign(page, "Teaser");
  await page.getByRole("button", { name: "assets" }).click();
  await page.locator('input[type="file"]').first().setInputFiles(HERO);
  await expect(page.getByTitle(/Place hero.png/)).toBeVisible({ timeout: 15_000 });

  // --- build a piece by hand, which is Phase 6's acceptance ------------------
  await page.getByRole("button", { name: "+ Text" }).click();
  await expect(page.getByRole("button", { name: /Text/ }).last()).toBeVisible();

  // Place the uploaded photo through the same operation an agent would send.
  await page.getByTitle(/Place hero.png/).click();
  // Wait for the layer to actually land before reading a version. Reading it
  // while a mutation is still in flight is how this test was flaky: it would
  // capture the version from before the placement and then expect one more than
  // that, while two operations had landed.
  await expect(page.getByText("Image", { exact: true })).toBeVisible();

  // --- step 16: move an element ---------------------------------------------
  // Dragged on the canvas rather than typed into a field, because the mechanism
  // worth testing is section 38: the pointer move sends nothing and the pointer
  // up is one operation.
  const versionBefore = await readVersion(page);
  const layer = page.locator("svg rect").filter({ has: page.locator("title") }).last();
  const box = await layer.boundingBox();
  if (!box) throw new Error("the layer overlay has no box to drag");

  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2 + 40, { steps: 8 });
  await page.mouse.up();

  // --- step 17: saved, and exactly once -------------------------------------
  await expect
    .poll(() => readVersion(page), { timeout: 10_000 })
    .toBe(versionBefore + 1);

  // --- steps 10 and 11: validate, and see the render ------------------------
  await page.getByRole("button", { name: "validation" }).click();
  await expect(page.getByText(/No errors|error/)).toBeVisible({ timeout: 15_000 });

  // --- step 14: export, and verify the download -----------------------------
  await page.getByRole("button", { name: "Export", exact: true }).click();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: /^Export \d+ page/ }).click();

  const link = page.getByRole("link", { name: /\.png/ }).first();
  await expect(link).toBeVisible({ timeout: 30_000 });
  await link.click();

  const file = await download;
  expect(file.suggestedFilename()).toMatch(/\.png$/);

  // A PNG, not an error page that happens to have the right name.
  const stream = await file.createReadStream();
  const chunks: Buffer[] = [];
  for await (const chunk of stream) chunks.push(chunk as Buffer);
  const bytes = Buffer.concat(chunks);

  expect(bytes.length).toBeGreaterThan(1000);
  expect(bytes.subarray(1, 4).toString()).toBe("PNG");
  expect(cspViolations, "the editor emitted a CSP violation").toEqual([]);
});

/**
 * Section 79 steps 18 and 19: an edit made elsewhere, and carrying on.
 *
 * The plan describes the agent receiving the conflict; this drives it the other
 * way round, because the agent's side is already covered in Rust and the
 * editor's conflict UI is what has none. The mechanism is the same guarded
 * UPDATE either way.
 */
test("an edit made elsewhere is explained, not silently lost", async ({ page, request }) => {
  await createProjectWithBrand(page, "Concurrent");
  await createDesign(page, "Contested");

  await page.getByRole("button", { name: "+ Rect" }).click();
  await expect.poll(() => readVersion(page)).toBeGreaterThan(1);

  const designId = new URL(page.url()).searchParams.get("design") ?? (await currentDesignId(request));
  const before = await readVersion(page);

  // Somebody else — an agent over MCP — moves the design on underneath us.
  const design = await (await request.get(`/api/v1/designs/${designId}`)).json();
  const pageId = design.document.pages[0].id;
  const elsewhere = await request.post(`/api/v1/designs/${designId}/operations`, {
    headers: { "x-forma-actor": "agent:claude" },
    data: {
      base_version: before,
      operations: [{ op: "set_background", page_id: pageId, color: "$brand.surface" }],
    },
  });
  expect(elsewhere.ok()).toBeTruthy();

  // The browser still holds the old version, so its next edit collides.
  await page.getByRole("button", { name: "+ Rect" }).click();

  // Not a toast: the edit did not happen, and the user has to know before they
  // carry on typing.
  await expect(page.getByText(/This design changed elsewhere/)).toBeVisible({ timeout: 10_000 });

  // And there is a way forward rather than a dead end.
  await page.getByRole("button", { name: /Reload|Refresh/ }).click();
  await expect.poll(() => readVersion(page)).toBeGreaterThan(before);
  await expect(page.getByText(/This design changed elsewhere/)).toBeHidden();
});

/** The version the editor is showing, from the header. */
async function readVersion(page: Page): Promise<number> {
  const text = await page.locator("header").getByText(/^v\d+$/).textContent();
  return Number.parseInt((text ?? "v0").slice(1), 10);
}

async function currentDesignId(request: import("@playwright/test").APIRequestContext) {
  const projects = await (await request.get("/api/v1/projects")).json();
  const designs = await (
    await request.get(`/api/v1/projects/${projects[0].id}/designs`)
  ).json();
  return designs[0].id as string;
}
