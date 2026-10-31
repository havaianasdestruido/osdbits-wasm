/**
 * Phase 11 (`P11-002`) — Production Static Bundle (`dist/`) & Preview Server Verification.
 *
 * Runs against the production `vite build` output served by a static server under `/osdbits-wasm/` (like a GitHub Pages project site):
 * - Verifies ZERO external runtime network requests (100% self-contained static bundle).
 * - Verifies ZERO console errors and ZERO uncaught page errors.
 * - Verifies WASM binary fetch/instantiation, single-click `Start OSDSYS` + `AudioContext` unlock,
 *   live WebGPU pixel rendering, and read-back-verified IndexedDB persistence across reload.
 */

import { expect, test } from "@playwright/test";

test("Phase 11 production bundle: zero external network requests, zero console errors, working WASM + WebGPU + Audio + IndexedDB", async ({
  page,
}) => {
  const externalRequests: string[] = [];
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];
  let wasmContentType = "";

  page.on("request", (req) => {
    const url = req.url();
    if (
      !url.startsWith("http://127.0.0.1:4173/") &&
      !url.startsWith("blob:") &&
      !url.startsWith("data:")
    ) {
      externalRequests.push(url);
    }
  });

  page.on("response", (res) => {
    if (res.url().endsWith(".wasm")) {
      wasmContentType = res.headers()["content-type"] ?? "";
    }
  });

  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(msg.text());
    }
  });

  page.on("pageerror", (err) => {
    pageErrors.push(err.message);
  });

  await page.goto("/osdbits-wasm/");
  await page.waitForFunction(
    () => document.getElementById("app")?.dataset.appState === "ready",
    null,
    { timeout: 15_000 },
  );

  // Missing files must not be rewritten to index.html by an SPA fallback.
  expect((await page.request.get("/osdbits-wasm/missing.js")).status()).toBe(404);

  // Verify WASM MIME header & ABI handshake
  expect(wasmContentType).toContain("application/wasm");
  await expect(page.locator("#app")).toHaveAttribute("data-abi-magic", "0x4f534442");

  // Single-click launch in production bundle
  await page.click("#start-osdsys-btn");
  await expect(page.locator("#launch-overlay")).toBeHidden();

  const prodState = await page.evaluate(async () => {
    const h = window.__osdbitsHarness!;
    // Switch to Mode D System Config and verify non-blank WebGPU output in production bundle
    await h.app.switchMode("menu");
    h.bridge.enterSystemConfig();
    for (let i = 0; i < 40; i++) {
      h.bridge.stepFrame();
    }
    const snap = h.bridge.stepFrame();
    h.app.renderer!.renderFrame(h.bridge.getVertexView(), h.bridge.getCommandView(), snap.mode);
    const rb = await h.app.renderer!.captureScenePixels();
    let nonBlack = 0;
    for (let p = 0; p < rb.rgba.length; p += 4) {
      if ((rb.rgba[p] ?? 0) + (rb.rgba[p + 1] ?? 0) + (rb.rgba[p + 2] ?? 0) > 6) {
        nonBlack++;
      }
    }
    const save = await h.storage.saveSettings({
      ...h.app.getSettings(),
      mode: "idle",
      pal: true,
    });
    return {
      audioStatus: h.audio.getStatus(),
      nonBlack,
      savedOk: save.ok && save.verifiedReadBack,
    };
  });

  expect(prodState.audioStatus).toBe("running");
  expect(prodState.nonBlack).toBeGreaterThan(1000);
  expect(prodState.savedOk).toBe(true);

  // Reload production page and confirm persisted settings restored
  await page.reload();
  await page.waitForFunction(
    () => document.getElementById("app")?.dataset.appState === "ready",
    null,
    { timeout: 15_000 },
  );
  const restored = await page.evaluate(() => window.__osdbitsHarness!.app.getSettings());
  expect(restored.mode).toBe("idle");
  expect(restored.pal).toBe(true);

  expect(externalRequests).toEqual([]);
  expect(consoleErrors).toEqual([]);
  expect(pageErrors).toEqual([]);
});
