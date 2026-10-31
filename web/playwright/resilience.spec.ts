import { expect, test } from "@playwright/test";

test("a missing core is reported, not hidden", async ({ page }) => {
  await page.route("**/osdbits_core.wasm", (route) => route.abort());
  await page.goto("/");
  await expect(page.locator("#banner")).toBeVisible({ timeout: 20_000 });
  await expect(page.locator("#banner-text")).toContainText(/simulation core failed to load/i);
  await expect(page.getByRole("button", { name: "Start OSDSYS" })).toBeDisabled();
});

test("the app recovers from a renderer failure without a reload", async ({ page }) => {
  await page.addInitScript(() => {
    // Force the WebGPU path to fail at adapter negotiation.
    Object.defineProperty(navigator, "gpu", {
      configurable: true,
      get: () => ({
        requestAdapter: async () => null,
        getPreferredCanvasFormat: () => "bgra8unorm",
      }),
    });
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Start OSDSYS" }).click();
  await expect(page.locator("#start")).toBeHidden({ timeout: 20_000 });
  await expect(page.locator("#status")).toContainText(/Canvas 2D/);
  await expect(page.locator("#banner-text")).toContainText(/WebGPU is unavailable/i);
  // Still animating after the fallback.
  await page.waitForTimeout(500);
  await expect(page.locator("#status")).toContainText(/fps/);
});

test("reduced motion freezes the camera", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await page.getByRole("button", { name: "Start OSDSYS" }).click();
  await expect(page.locator("#start")).toBeHidden({ timeout: 20_000 });
  await page.getByRole("button", { name: "Diagnostics" }).click();
  const list = page.locator("#diagnostics-list");
  const height1 = (await list.innerText()).match(/camera height\s+([\d.]+)/)?.[1];
  await page.waitForTimeout(700);
  const height2 = (await list.innerText()).match(/camera height\s+([\d.]+)/)?.[1];
  expect(height1).toBe(height2);
});
