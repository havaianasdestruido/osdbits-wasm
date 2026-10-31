import { expect, test } from "@playwright/test";
import { hasWebGpuAdapter, openApp, rendererKind, startScene } from "./support.js";

/**
 * Activation journey. These run on every browser in CI, GPU or not: the app
 * must either use WebGPU or say that it fell back, and in both cases the single
 * click has to start the scene.
 */
test.describe("activation", () => {
  test("boot settles and offers exactly one start control", async ({ page }) => {
    const errors = await openApp(page);
    const start = page.getByRole("button", { name: "Start OSDSYS" });
    await expect(start).toBeVisible();
    await expect(start).toBeEnabled();
    await expect(page.getByRole("button", { name: "Start OSDSYS" })).toHaveCount(1);
    await expect(page.locator("#capabilities")).toContainText(/WebGPU (available|unavailable)/);
    await expect(page.locator("#status")).toHaveText("idle");
    expect(errors, "no uncaught page errors before activation").toEqual([]);
  });

  test("one click starts the scene and reports the renderer it got", async ({ page }) => {
    const errors = await openApp(page);
    await startScene(page);

    const kind = await rendererKind(page);
    expect(["webgpu", "canvas2d"]).toContain(kind);

    const status = await page.locator("#status").innerText();
    expect(status).toMatch(/fps/);
    expect(status).toMatch(kind === "webgpu" ? /WebGPU/ : /Canvas 2D/);

    // A fallback is announced, never hidden.
    if (kind === "canvas2d") {
      await expect(page.locator("#banner-text")).toContainText(/WebGPU/);
    }

    // Audio: the context is created inside the gesture. Whether the browser
    // actually produces sound is not observable here, so we only require the
    // state to be reported honestly.
    const audio = await page.locator("body").getAttribute("data-audio");
    expect(["running", "suspended", "none", "closed", "interrupted", null]).toContain(audio);

    expect(errors, "no uncaught page errors after activation").toEqual([]);
  });

  test("WebGPU is used when the browser has an adapter", async ({ page }) => {
    await openApp(page);
    test.skip(!(await hasWebGpuAdapter(page)), "this browser has no WebGPU adapter");
    await startScene(page);
    expect(await rendererKind(page)).toBe("webgpu");
  });

  test("the simulation advances and emits a non-trivial draw list", async ({ page }) => {
    await openApp(page);
    await startScene(page);
    const body = page.locator("body");

    const frame1 = Number(await body.getAttribute("data-frame"));
    expect(frame1).toBeGreaterThanOrEqual(0);
    await expect
      .poll(async () => Number(await body.getAttribute("data-frame")), { timeout: 20_000 })
      .toBeGreaterThan(frame1);

    // The opening scene is 640x224 with towers, fog and light streaks: a frame
    // with no batches would mean the core produced nothing.
    const batches = Number(await body.getAttribute("data-batches"));
    expect(batches).toBeGreaterThan(0);
    expect(await body.getAttribute("data-scene")).toMatch(/^\d+$/);
  });

  test("the drawn canvas is not blank", async ({ page }) => {
    await openApp(page);
    await startScene(page);
    await page.waitForTimeout(1000);
    const variance = await page.evaluate(() => {
      const canvas = document.getElementById("canvas") as HTMLCanvasElement;
      const copy = document.createElement("canvas");
      copy.width = 64;
      copy.height = 32;
      const ctx = copy.getContext("2d");
      if (!ctx) return -1;
      ctx.drawImage(canvas, 0, 0, copy.width, copy.height);
      const data = ctx.getImageData(0, 0, copy.width, copy.height).data;
      let sum = 0;
      let sumSq = 0;
      const n = data.length / 4;
      for (let i = 0; i < data.length; i += 4) {
        const l = data[i] * 0.299 + data[i + 1] * 0.587 + data[i + 2] * 0.114;
        sum += l;
        sumSq += l * l;
      }
      return sumSq / n - (sum / n) ** 2;
    });
    // Not a visual-regression baseline (those need a GPU); just proof that the
    // canvas holds a rendered, varied image rather than a clear colour.
    expect(variance).toBeGreaterThan(1);
  });
});
