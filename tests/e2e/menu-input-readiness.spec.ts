import { expect, test } from "@playwright/test";
import { clickUi, sceneMenuScreen, waitForAppReady } from "./support/uiHarness";

/**
 * Regression test for a CI flake where the Mode D pointer path silently dropped clicks: the
 * System Configuration rows fade in during the last frames of the 90-frame open choreography
 * while the core still ignores input (`menu.c:0x227D08`). A click landing in that window was
 * discarded. The in-app menu draws rows disabled until the core accepts input, and this test
 * asserts that invariant from inside the page, sampling every frame's drawn regions.
 */
test("Mode D: menu rows and hints are never clickable before the core accepts input", async ({ page }) => {
  await page.goto("/");
  await waitForAppReady(page);

  await page.evaluate(() => {
    const w = window as unknown as {
      __menuReadiness: { violations: string[]; sawGatedFrame: boolean; sawReadyFrame: boolean };
    };
    const record = { violations: [] as string[], sawGatedFrame: false, sawReadyFrame: false };
    w.__menuReadiness = record;
    const sample = () => {
      const app = window.__osdbitsHarness?.app;
      const root = document.getElementById("app");
      const regions = app?.getUiRegions() ?? [];
      const menuRegions = regions.filter((r) => /^(menu-|clock-field-)/.test(r.id));
      const gated = root?.dataset.menuInteractive === "false";
      if (menuRegions.length > 0 && gated) {
        record.sawGatedFrame = true;
        const enabled = menuRegions.filter((r) => r.enabled);
        if (enabled.length > 0) {
          record.violations.push(`enabled region "${enabled[0]?.id}" while input was gated`);
        }
      }
      if (menuRegions.length > 0 && !gated) {
        record.sawReadyFrame = true;
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });

  await clickUi(page, "title-start");
  await expect(page.locator("#app")).toHaveAttribute("data-scene-active", "true");
  await page.keyboard.press("4");
  await expect(page.locator("#app")).toHaveAttribute("data-menu-interactive", "true", {
    timeout: 20_000,
  });

  // Enter System Configuration: rows fade in while input is still gated.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect.poll(() => sceneMenuScreen(page)).toBe("config-list");

  // Clicking the first row must wait for the core to accept input instead of dropping the confirm.
  await clickUi(page, "menu-item-0");
  await expect.poll(() => sceneMenuScreen(page)).toBe("config-edit-clock");
  const fieldCount = await page.evaluate(
    () => window.__osdbitsHarness!.app.getLatestSnapshot()?.menuUi?.clockFields?.length ?? 0,
  );
  expect(fieldCount).toBe(6);

  const observed = await page.evaluate(
    () =>
      (
        window as unknown as {
          __menuReadiness: { violations: string[]; sawGatedFrame: boolean; sawReadyFrame: boolean };
        }
      ).__menuReadiness,
  );
  expect(observed.violations).toEqual([]);
  expect(observed.sawGatedFrame).toBe(true);
  expect(observed.sawReadyFrame).toBe(true);

  // Back out: leaving the screen gates input again until the choreography ends.
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.locator("#app")).toHaveAttribute("data-menu-interactive", "true", {
    timeout: 15_000,
  });
  await expect.poll(() => sceneMenuScreen(page)).toBe("main");

  const finalViolations = await page.evaluate(
    () => (window as unknown as { __menuReadiness: { violations: string[] } }).__menuReadiness.violations,
  );
  expect(finalViolations).toEqual([]);
});
