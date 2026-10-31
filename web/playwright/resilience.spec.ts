import { expect, test } from "@playwright/test";
import { openApp, openPanel, rendererKind, startScene } from "./support.js";

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

  test("reduced motion holds the camera while the scene keeps animating", async ({ page }) => {
    await openApp(page);
    // Set it explicitly rather than through media emulation, so the assertion
    // is about the app, not about which browser emulates prefers-reduced-motion.
    await openPanel(page, "Settings");
    await page.locator("#field-reducedMotion").check();
    await page.locator("#settings-save").click();
    await startScene(page);

    await expect(page.locator("html")).toHaveAttribute("data-camera-motion", "off");
    const body = page.locator("body");
    // The opening camera path is held as well; the core reports z = 16.
    await expect.poll(async () => await body.getAttribute("data-camera-height")).toBe("16.00");
    const first = Number(await body.getAttribute("data-frame"));
    const height = await body.getAttribute("data-camera-height");
    await page.waitForTimeout(900);
    expect(await body.getAttribute("data-camera-height")).toBe(height);
    // ...and the simulation still advances (fog, lights, text).
    expect(Number(await body.getAttribute("data-frame"))).toBeGreaterThan(first);
  });

  test("the system reduced-motion preference is honoured", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    const reported = await page.evaluate(
      () => globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false,
    );
    test.skip(!reported, "this browser does not emulate prefers-reduced-motion");
    await openApp(page);
    await expect(page.locator("html")).toHaveAttribute("data-camera-motion", "off");
    await openPanel(page, "Settings");
    await expect(page.locator("#field-reducedMotion")).toBeChecked();
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
