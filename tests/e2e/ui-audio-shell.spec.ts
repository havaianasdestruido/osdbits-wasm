import { expect, test } from "@playwright/test";

test("Phases 6, 8 & 9 E2E: single-click Start OSDSYS + AudioContext unlock, Mode C/D/E interactive UI, two-step reset, and in-place recovery", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(err.message));

  await page.goto("/");
  await page.waitForFunction(
    () => document.getElementById("app")?.dataset.appState === "ready",
    null,
    { timeout: 15_000 },
  );

  // 1. Pre-activation: launch overlay visible, zero audio nodes / resume calls
  await expect(page.locator("#launch-overlay")).toBeVisible();
  const preAudio = await page.evaluate(() => {
    const a = window.__osdbitsHarness!.audio;
    return {
      status: a.getStatus(),
      ctxState: a.getContextState(),
      resumes: a.getResumeCallCount(),
    };
  });
  expect(preAudio).toEqual({
    status: "unactivated",
    ctxState: "uninitialized",
    resumes: 0,
  });

  // 2. Single-click activation on "Start OSDSYS" -> synchronous AudioContext.resume() + immediate scene entry!
  await page.click("#start-osdsys-btn");
  await expect(page.locator("#launch-overlay")).toBeHidden();
  await expect(page.locator("#hud-toolbar")).toBeVisible();

  await expect
    .poll(async () => {
      return page.evaluate(() => {
        const h = window.__osdbitsHarness!;
        return {
          sceneActive: document.getElementById("app")?.dataset.sceneActive,
          resumes: h.audio.getResumeCallCount(),
          ctxState: h.audio.getContextState(),
          status: h.audio.getStatus(),
        };
      });
    })
    .toEqual({
      sceneActive: "true",
      resumes: 1,
      ctxState: "running",
      status: "running",
    });

  // 3. Mode C: switch to Illegal Disc, advance to text fade-in, verify localized text + unverified boot-param provenance
  await page.click('.mode-switch-btn[data-mode="illegal-disc"]');
  await page.evaluate(() => {
    const h = window.__osdbitsHarness!;
    for (let i = 0; i < 65; i++) {
      h.bridge.stepFrame();
    }
    // Trigger one UI sync step
    (h.app as unknown as { lastSnapshot: unknown; renderModeOverlays: () => void }).lastSnapshot =
      h.bridge.stepFrame();
    (h.app as unknown as { renderModeOverlays: () => void }).renderModeOverlays();
  });
  await expect(page.locator("#illegal-hud")).toBeVisible();
  await expect(page.locator("#illegal-msg-text")).toHaveText(
    "Please insert a supported format disc.",
  );
  await expect(page.locator(".illegal-prov-note")).toContainText("unverified");

  // 4. Mode D: interactive keyboard & pointer menu navigation + provenance pills + 6-field clock editor
  await page.click('.mode-switch-btn[data-mode="menu"]');
  await expect(page.locator("#menu-hud")).toBeVisible();
  await expect(page.locator("#menu-items-panel")).toContainText("Browser");
  await expect(page.locator("#menu-items-panel")).toContainText("System Configuration");
  await expect(page.locator("#menu-items-panel")).toContainText("approximate");
  await expect(page.locator("#menu-items-panel")).toContainText("sourced");

  // Navigate via keyboard Down -> Enter into System Configuration
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(page.locator("#menu-screen-title")).toHaveText("System Configuration");

  // Pointer-click "Clock Adjustment" (index 0) -> opens 6-field clock editor bar.
  // The rows fade in during the 90-frame open choreography, but the core ignores
  // input until it finishes (`menu.c:0x227D08`), so the shell keeps them disabled:
  // `click()` therefore waits for the row to become clickable instead of dropping
  // a click that lands inside the transition window.
  const clockAdjustmentRow = page.locator('#menu-items-panel button[data-menu-index="0"]');
  await expect(clockAdjustmentRow).toBeEnabled();
  await clockAdjustmentRow.click();
  await expect(page.locator("#clock-editor-bar")).toBeVisible();
  const initialYear = await page.locator("#clock-editor-bar .clock-field-chip").first().textContent();
  await page.keyboard.press("ArrowUp");
  const nextYear = await page.locator("#clock-editor-bar .clock-field-chip").first().textContent();
  expect(Number(nextYear)).toBe(Number(initialYear) + 1);
  await page.keyboard.press("Escape");
  await expect(page.locator("#clock-editor-bar")).toBeHidden();

  // 5. Mode E: Customize drawer, verified persistence, and two-step Reset to Faithful Defaults
  await page.click("#toggle-customize-btn");
  await expect(page.locator("#customize-drawer")).toBeVisible();
  await page.selectOption("#cfg-video-standard", "pal");
  await page.selectOption("#cfg-language", "3"); // Spanish
  await page.check("#cfg-reduced-motion");

  await expect(page.locator("#persistence-status")).toContainText("Saved & read-back verified");

  // First click on Reset reveals Confirm Reset without resetting immediately
  await page.click("#reset-defaults-btn");
  await expect(page.locator("#confirm-reset-btn")).toBeVisible();
  expect(await page.inputValue("#cfg-video-standard")).toBe("pal");

  // Second click confirms reset back to faithful NTSC English defaults
  await page.click("#confirm-reset-btn");
  await expect(page.locator("#persistence-status")).toContainText("Restored faithful OSDSYS defaults");
  expect(await page.inputValue("#cfg-video-standard")).toBe("ntsc");
  expect(await page.inputValue("#cfg-language")).toBe("1");

  // 6. Inspector toggle & Non-destructive scene error recovery without reload
  await page.click("#toggle-inspector-btn");
  await expect(page.locator("#telemetry-panel")).toBeVisible();
  await expect(page.locator("#telemetry-grid")).toContainText("Provenance");

  await page.evaluate(() => {
    window.__osdbitsHarness!.app.triggerSimulatedFault("Injected E2E GPU/Sim fault");
  });
  await expect(page.locator("#error-banner")).toBeVisible();
  await page.click("#recover-scene-btn");
  await expect(page.locator("#error-banner")).toBeHidden();

  expect(pageErrors).toEqual([]);
});

test("Phase 9 capability fallback: gracefully presents diagnostic UI when navigator.gpu is absent", async ({
  context,
}) => {
  const page = await context.newPage();
  const pageErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(err.message));

  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", {
      configurable: true,
      get: () => undefined,
    });
  });

  await page.goto("/");
  await page.waitForFunction(
    () => document.getElementById("app")?.dataset.appState === "ready",
    null,
    { timeout: 15_000 },
  );

  await expect(page.locator("#webgpu-fallback-panel")).toBeVisible();
  await expect(page.locator("#webgpu-fallback-panel")).toContainText("WebGPU Adapter Unavailable");
  await expect(page.locator("#start-osdsys-btn")).toBeDisabled();
  expect(pageErrors).toEqual([]);
  await page.close();
});
