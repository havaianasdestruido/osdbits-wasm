import { expect, test } from "@playwright/test";
import { openApp, rendererKind, startScene } from "./support.js";

/**
 * Failure paths. Each one has to be visible to the user and recoverable
 * without a page reload.
 */
test.describe("resilience", () => {
  test("a missing simulation core is reported and blocks activation", async ({ page }) => {
    await page.route("**/osdbits_core.wasm", (route) => route.abort());
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await expect(page.locator("body")).toHaveAttribute("data-boot", "failed", { timeout: 45_000 });
    await expect(page.locator("#banner")).toBeVisible();
    await expect(page.locator("#banner-text")).toContainText(/simulation core failed to load/i);
    await expect(page.getByRole("button", { name: "Start OSDSYS" })).toBeDisabled();
    await expect(page.locator("#capabilities")).toContainText(/simulation core is unavailable/i);
  });

  test("a renderer failure falls back without a reload", async ({ page }) => {
    await page.addInitScript(() => {
      // Force adapter negotiation to fail even in browsers that have WebGPU.
      Object.defineProperty(navigator, "gpu", {
        configurable: true,
        get: () => ({
          requestAdapter: async () => null,
          getPreferredCanvasFormat: () => "bgra8unorm",
        }),
      });
    });
    const errors = await openApp(page);
    await startScene(page);
    expect(await rendererKind(page)).toBe("canvas2d");
    await expect(page.locator("#status")).toContainText(/Canvas 2D/);
    await expect(page.locator("#banner-text")).toContainText(/WebGPU is unavailable/i);
    await expect
      .poll(async () => Number(await page.locator("body").getAttribute("data-frame")), {
        timeout: 20_000,
      })
      .toBeGreaterThan(0);
    expect(errors).toEqual([]);
  });

  test("the retry button restores the scene after a renderer fault", async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "gpu", {
        configurable: true,
        get: () => ({
          requestAdapter: async () => null,
          getPreferredCanvasFormat: () => "bgra8unorm",
        }),
      });
    });
    await openApp(page);
    await startScene(page);
    await expect(page.locator("#banner-retry")).toBeVisible();
    await page.locator("#banner-retry").click();
    await expect(page.locator("body")).toHaveAttribute("data-phase", "running");
    await expect
      .poll(async () => Number(await page.locator("body").getAttribute("data-frame")), {
        timeout: 20_000,
      })
      .toBeGreaterThan(0);
  });

  test("reduced motion holds the camera", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await openApp(page);
    await startScene(page);
    const body = page.locator("body");
    const height1 = await body.getAttribute("data-camera-height");
    expect(height1).toBeTruthy();
    await page.waitForTimeout(800);
    const height2 = await body.getAttribute("data-camera-height");
    expect(height2).toBe(height1);
    // The scene itself keeps running: only the camera is held.
    await expect
      .poll(async () => Number(await body.getAttribute("data-frame")), { timeout: 20_000 })
      .toBeGreaterThan(0);
  });

  test("the page survives a reload mid-scene", async ({ page }) => {
    await openApp(page);
    await startScene(page);
    await page.reload();
    await expect(page.locator("body")).toHaveAttribute("data-boot", "ready", { timeout: 45_000 });
    await expect(page.getByRole("button", { name: "Start OSDSYS" })).toBeEnabled();
    await startScene(page);
    expect(["webgpu", "canvas2d"]).toContain(await rendererKind(page));
  });
});
