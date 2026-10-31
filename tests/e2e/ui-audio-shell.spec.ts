import { expect, test, type Page } from "@playwright/test";
import {
  clickUi,
  drawnTextIncludes,
  expectOnlyApplicationVisible,
  sceneMenuScreen,
  uiRegionVisible,
  waitForAppReady,
} from "./support/uiHarness";

async function clockYear(page: Page): Promise<number | null> {
  return page.evaluate(() => {
    const fields = window.__osdbitsHarness?.app.getLatestSnapshot()?.menuUi?.clockFields;
    return fields ? Number(fields[0]) : null;
  });
}

test("UILess: the title is the only UI at rest, and one canvas click starts the scene with audio unlocked", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(err.message));

  await page.goto("/");
  await waitForAppReady(page);

  // 1. Pre-activation: only the application is visible; the title is drawn into the canvas.
  await expectOnlyApplicationVisible(page);
  await expect(page.locator("#app")).toHaveAttribute("data-ui-screen", "title");
  await expect.poll(() => drawnTextIncludes(page, "START OSDSYS")).toBe(true);
  const preAudio = await page.evaluate(() => {
    const a = window.__osdbitsHarness!.audio;
    return { status: a.getStatus(), ctxState: a.getContextState(), resumes: a.getResumeCallCount() };
  });
  expect(preAudio).toEqual({ status: "unactivated", ctxState: "uninitialized", resumes: 0 });

  // 2. Single pointer activation: synchronous AudioContext.resume() + immediate scene entry.
  await clickUi(page, "title-start");
  await expect(page.locator("#app")).toHaveAttribute("data-scene-active", "true");
  await expect(page.locator("#app")).toHaveAttribute("data-renderer-backend", "webgpu");
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const h = window.__osdbitsHarness!;
        return {
          resumes: h.audio.getResumeCallCount(),
          ctxState: h.audio.getContextState(),
          status: h.audio.getStatus(),
        };
      }),
    )
    .toEqual({ resumes: 1, ctxState: "running", status: "running" });

  // 3. Mode C: key 3 switches to the Red Disc; the localized warning is drawn in-app.
  await page.keyboard.press("3");
  await expect(page.locator("#app")).toHaveAttribute(
    "data-illegal-text",
    "Please insert a supported format disc.",
    { timeout: 20_000 },
  );
  await expect.poll(() => drawnTextIncludes(page, "unverified")).toBe(true);

  // 4. Mode D: keyboard 4 opens the menu; provenance tags are drawn in-app.
  await page.keyboard.press("4");
  await expect(page.locator("#app")).toHaveAttribute("data-ui-screen", "menu");
  await expect(page.locator("#app")).toHaveAttribute("data-menu-interactive", "true", {
    timeout: 20_000,
  });
  await expect.poll(() => drawnTextIncludes(page, "Browser")).toBe(true);
  await expect.poll(() => drawnTextIncludes(page, "System Configuration")).toBe(true);
  await expect.poll(() => drawnTextIncludes(page, "[APPROX]")).toBe(true);
  await expect.poll(() => drawnTextIncludes(page, "[SOURCED]")).toBe(true);

  // Keyboard into System Configuration, then pointer-click "Clock Adjustment" (index 0).
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect.poll(() => sceneMenuScreen(page)).toBe("config-list");
  await clickUi(page, "menu-item-0");
  await expect.poll(() => sceneMenuScreen(page)).toBe("config-edit-clock");
  const fieldCount = await page.evaluate(
    () => window.__osdbitsHarness!.app.getLatestSnapshot()?.menuUi?.clockFields?.length ?? 0,
  );
  expect(fieldCount).toBe(6);
  const initialYear = await clockYear(page);
  await page.keyboard.press("ArrowUp");
  await expect.poll(() => clockYear(page)).toBe((initialYear ?? 0) + 1);
  await page.keyboard.press("Escape");
  await expect.poll(() => sceneMenuScreen(page)).not.toBe("config-edit-clock");

  // 5. Mode E: the options screen is drawn in-app and every setting is a canvas row.
  await page.keyboard.press("c");
  await expect(page.locator("#app")).toHaveAttribute("data-ui-screen", "options");
  await clickUi(page, "opt-video");
  await expect.poll(() => page.evaluate(() => window.__osdbitsHarness!.app.getSettings().pal)).toBe(true);
  await clickUi(page, "opt-lang");
  await clickUi(page, "opt-lang");
  await expect
    .poll(() => page.evaluate(() => window.__osdbitsHarness!.app.getSettings().language))
    .toBe(3);
  await clickUi(page, "opt-reduced");
  await expect
    .poll(() => page.evaluate(() => window.__osdbitsHarness!.app.getSettings().reducedMotion))
    .toBe(true);
  await expect
    .poll(() => page.evaluate(() => window.__osdbitsHarness!.app.getPanelStatus().persistence))
    .toContain("Saved & read-back verified");

  // Two-step reset: first press reveals the confirm row without resetting.
  await clickUi(page, "opt-reset");
  await expect.poll(() => uiRegionVisible(page, "opt-reset-confirm")).toBe(true);
  expect(await page.evaluate(() => window.__osdbitsHarness!.app.getSettings().pal)).toBe(true);
  await clickUi(page, "opt-reset-confirm");
  await expect
    .poll(() => page.evaluate(() => window.__osdbitsHarness!.app.getPanelStatus().persistence))
    .toContain("Restored faithful OSDSYS defaults");
  const restored = await page.evaluate(() => {
    const s = window.__osdbitsHarness!.app.getSettings();
    return { pal: s.pal, language: s.language };
  });
  expect(restored).toEqual({ pal: false, language: 1 });

  // Diagnostics: a sub-screen of options with live telemetry and provenance, then back.
  await clickUi(page, "opt-diagnostics");
  await expect(page.locator("#app")).toHaveAttribute("data-ui-screen", "diagnostics");
  await expect.poll(() => drawnTextIncludes(page, "LIVE TELEMETRY")).toBe(true);
  await expect.poll(() => drawnTextIncludes(page, "PROVENANCE")).toBe(true);
  await page.keyboard.press("Escape");
  await expect(page.locator("#app")).toHaveAttribute("data-ui-screen", "options");
  await page.keyboard.press("Escape");
  await expect(page.locator("#app")).toHaveAttribute("data-ui-screen", "menu");

  // 6. Scene error: minimal in-app box with a recover row, then recovery without reload.
  await page.evaluate(() => {
    window.__osdbitsHarness!.app.triggerSimulatedFault("Injected E2E GPU/Sim fault");
  });
  await expect(page.locator("#app")).toHaveAttribute("data-runtime-error", "true");
  await expect.poll(() => drawnTextIncludes(page, "SCENE ERROR")).toBe(true);
  await clickUi(page, "error-recover");
  await expect(page.locator("#app")).not.toHaveAttribute("data-runtime-error", "true");

  // 7. At rest again: only the application; the corner pop-up hides after idling.
  await expectOnlyApplicationVisible(page);
  await expect(page.locator("#app")).not.toHaveAttribute("data-popup-visible", "true", {
    timeout: 8_000,
  });

  expect(pageErrors).toEqual([]);
});

test("Phase 9 capability fallback: without navigator.gpu the title says so and offers the reduced Canvas 2D path", async ({
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
  await waitForAppReady(page);
  await expectOnlyApplicationVisible(page);
  await expect(page.locator("#app")).toHaveAttribute("data-webgpu-fallback", "true");

  await expect.poll(() => drawnTextIncludes(page, "WEBGPU ADAPTER UNAVAILABLE")).toBe(true);
  await expect.poll(() => drawnTextIncludes(page, "RENDERER: WEBGPU UNAVAILABLE")).toBe(true);
  expect(await uiRegionVisible(page, "title-start")).toBe(false);

  await clickUi(page, "title-canvas2d");
  await expect(page.locator("#app")).toHaveAttribute("data-renderer-backend", "canvas2d");
  await expect(page.locator("#app")).toHaveAttribute("data-scene-active", "true");
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const canvas = document.getElementById("osdbits-canvas") as HTMLCanvasElement;
        const ctx = canvas.getContext("2d");
        if (!ctx) return 0;
        const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
        let nonBlackSamples = 0;
        for (let index = 0; index < pixels.length; index += 400) {
          if ((pixels[index] ?? 0) + (pixels[index + 1] ?? 0) + (pixels[index + 2] ?? 0) > 30) {
            nonBlackSamples++;
          }
        }
        return nonBlackSamples;
      }),
    )
    .toBeGreaterThan(0);

  expect(pageErrors).toEqual([]);
  await page.close();
});
