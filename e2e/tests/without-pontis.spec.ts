import { expect, test, type Page } from "@playwright/test";

/**
 * Scenario S11 of docs/pontis-pv1-e2e.md: the base application without Pontis.
 *
 * Forma has no Pontis integration yet, so today this passes trivially. That is
 * the point of writing it first. It is the one scenario whose failure means the
 * product got worse: once the sidecar lands, any path by which the editor comes
 * to depend on it — a request it waits on, a panel that claims an agent is
 * there — fails here rather than in front of a person who never installed it.
 *
 * "Absent" means gone, not configured off: this run starts forma-server alone,
 * with no sidecar and no `pontisd`, which is exactly what the shared config
 * already does. The other half of S11 — `forma-mcp` still serving the direct
 * door — has no browser in it and belongs to `apps/mcp/tests/agent_flow.rs`.
 */

/** Any path segment named `pons`, which is where the sidecar's routes live. */
const PONS_PATH = /(^|\/)pons(\/|$)/;

/**
 * Words that claim something is there to act: an agent, a device, a runtime.
 *
 * Inviting a connection ("Connect an agent", "Agent connections") is fine and
 * is what a local Forma should say. Asserting one exists is not, because with
 * nothing running there is nothing it could be true of.
 */
const CLAIMS_PRESENCE = [
  /\b(agent|assistant|device|runtime|sidecar|pontis|session)s?\b[^.\n]{0,40}\b(connected|available|online|ready|running|attached|active)\b/i,
  /\b(connected|available|online|ready|running|attached|active)\b[^.\n]{0,40}\b(agent|assistant|device|runtime|sidecar|pontis|session)s?\b/i,
  /\bConnected applications\b/i,
];

interface Traffic {
  pons: string[];
  serverErrors: string[];
}

/** Records every request and socket that touches Pontis, and every 5xx. */
function watchTraffic(page: Page): Traffic {
  const traffic: Traffic = { pons: [], serverErrors: [] };
  page.on("request", (request) => {
    if (PONS_PATH.test(new URL(request.url()).pathname)) traffic.pons.push(request.url());
  });
  page.on("websocket", (socket) => {
    if (PONS_PATH.test(new URL(socket.url()).pathname)) traffic.pons.push(socket.url());
  });
  page.on("response", (response) => {
    if (response.status() >= 500) traffic.serverErrors.push(`${response.status()} ${response.url()}`);
  });
  return traffic;
}

/** Fails if the visible page claims an agent, device or runtime is present. */
async function expectNoPresenceClaim(page: Page, where: string) {
  const text = await page.locator("body").innerText();
  for (const claim of CLAIMS_PRESENCE) {
    expect(text, `${where} claims something is connected or available`).not.toMatch(claim);
  }
}

/** The version the editor is showing, from the header. */
async function readVersion(page: Page): Promise<number> {
  const text = await page.locator("header").getByText(/^v\d+$/).textContent();
  return Number.parseInt((text ?? "v0").slice(1), 10);
}

/** Resolves when the editor has fetched a rendered preview of `version`. */
function previewOf(page: Page, version: number) {
  return page.waitForResponse(
    (response) =>
      /\/preview\.svg\?/.test(response.url()) &&
      new URL(response.url()).searchParams.get("v") === String(version) &&
      response.ok(),
  );
}

test("the base product is fully usable while Pontis is absent", async ({ page }) => {
  const traffic = watchTraffic(page);

  // --- the editor loads, with nothing claiming to be connected ---------------
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Give visual ideas a form." })).toBeVisible();
  await expect(page.getByRole("button", { name: "Undo" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Export", exact: true })).toHaveCount(0);
  await expectNoPresenceClaim(page, "the projects page");

  // The page that would carry a connection status, if anything did.
  await page.getByRole("link", { name: "Connect an agent" }).click();
  await expect(page.getByRole("heading", { name: "Agent connections" })).toBeVisible();
  await expectNoPresenceClaim(page, "the agents page");
  await page.getByRole("link", { name: "← Projects" }).click();

  // --- a project opens ------------------------------------------------------
  await page.getByPlaceholder("New project").fill("Standalone");
  await page.getByPlaceholder("New project").press("Enter");
  await page.getByRole("button", { name: /Standalone/ }).click();

  await page.getByRole("button", { name: "Set up" }).click();
  await page.getByRole("button", { name: "Create a Brand Kit" }).click();
  await page.getByRole("button", { name: "Save Brand Kit" }).click();
  await expect(page.getByRole("button", { name: "Saved" })).toBeVisible();
  await page.getByRole("button", { name: "Close" }).click();

  // --- a design renders its preview -----------------------------------------
  await page.getByPlaceholder("New design").fill("Offline");
  const firstPreview = previewOf(page, 1);
  await page.getByPlaceholder("New design").press("Enter");
  await expect(page.getByRole("button", { name: "Open Projects" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Layers & pages", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByLabel(/Canvas zoom \d+ percent/)).toBeVisible();
  await expect(page.getByRole("complementary", { name: "Properties" })).toHaveCount(0);
  await firstPreview;
  await expectNoPresenceClaim(page, "the editor");

  // --- an edit applies through the normal UI path, and the version advances --
  const before = await readVersion(page);
  const editedPreview = previewOf(page, before + 1);
  await page.getByRole("button", { name: "+ Rect" }).click();
  await expect.poll(() => readVersion(page)).toBe(before + 1);

  // The render reflects the edit: usvg keeps a `layer-{uuid}` group per layer,
  // so the new rectangle is in the drawn preview, not only in the layer list.
  await editedPreview;
  await expect(page.locator('g[id^="layer-"]').first()).toBeAttached();

  // --- an export completes --------------------------------------------------
  await page.getByRole("button", { name: "Export", exact: true }).click();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: /^Export \d+ page/ }).click();

  const link = page.getByRole("link", { name: /\.png/ }).first();
  await expect(link).toBeVisible({ timeout: 30_000 });
  await link.click();

  const file = await download;
  expect(file.suggestedFilename()).toMatch(/\.png$/);
  const stream = await file.createReadStream();
  const chunks: Buffer[] = [];
  for await (const chunk of stream) chunks.push(Buffer.from(chunk as Uint8Array));
  const bytes = Buffer.concat(chunks);
  expect(bytes.subarray(1, 4).toString()).toBe("PNG");

  await expectNoPresenceClaim(page, "the editor after export");

  // --- and nothing along the way reached for a sidecar ----------------------
  expect(traffic.pons, "the app made a request to a /pons/ path").toEqual([]);
  expect(traffic.serverErrors, "a route answered 5xx").toEqual([]);
});
