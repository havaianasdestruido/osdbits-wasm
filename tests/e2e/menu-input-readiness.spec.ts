import { expect, test } from "@playwright/test";

/**
 * Regression test for a CI flake where the Mode D pointer path silently dropped
 * clicks: the System Configuration rows fade in during the last frames of the
 * 90-frame open choreography while the core still ignores input
 * (`menu.c:0x227D08`). A click landing in that window was discarded, so the
 * clock editor never opened. The shell now disables rows until the core accepts
 * input, and this test asserts that invariant from inside the page.
 */
test("Mode D: rows are never clickable before the core accepts input", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () => document.getElementById("app")?.dataset.appState === "ready",
    null,
    { timeout: 15_000 },
  );

  await page.evaluate(() => {
    const w = window as unknown as {
      __menuReadiness: { violations: string[]; sawGatedFrame: boolean; sawReadyFrame: boolean };
    };
    const record: { violations: string[]; sawGatedFrame: boolean; sawReadyFrame: boolean } = {
      violations: [],
      sawGatedFrame: false,
      sawReadyFrame: false,
    };
    w.__menuReadiness = record;
    const sample = () => {
      const panel = document.getElementById("menu-items-panel");
      const hud = document.getElementById("menu-hud");
      const gated = panel?.dataset.menuInteractive === "false";
      const rows = panel?.querySelectorAll("button[data-menu-index]") ?? [];
      const enabled = Array.from(rows).filter((row) => !(row as HTMLButtonElement).disabled);
      if (rows.length > 0 && gated) {
        record.sawGatedFrame = true;
      }
      if (rows.length > 0 && !gated) {
        record.sawReadyFrame = true;
      }
      if (gated && enabled.length > 0) {
        record.violations.push(
          `enabled row "${enabled[0]?.textContent?.trim() ?? ""}" while input was gated`,
        );
      }
      const hints = document.querySelectorAll<HTMLButtonElement>(
        "#menu-hint-confirm, #menu-hint-cancel, #menu-hint-options",
      );
      const wronglyEnabledHint = Array.from(hints).find(
        (hint) => !hint.disabled && hud?.dataset.menuInteractive === "false",
      );
      if (wronglyEnabledHint) {
        record.violations.push(`enabled hint #${wronglyEnabledHint.id} while input was gated`);
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });

  await page.click("#start-osdsys-btn");
  await expect(page.locator("#hud-toolbar")).toBeVisible();
  await page.click('.mode-switch-btn[data-mode="menu"]');
  await expect(page.locator("#menu-items-panel")).toContainText("System Configuration");
  await expect(page.locator("#menu-items-panel")).toHaveAttribute("data-menu-interactive", "true");

  // Enter System Configuration: rows fade in while input is still gated.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");

  // Click the very first row as soon as it exists: `click()` must wait for the
  // core to accept input instead of dropping the confirm inside the transition.
  const clockRow = page.locator('#menu-items-panel button[data-menu-index="0"]');
  await clockRow.click();
  await expect(page.locator("#clock-editor-bar")).toBeVisible();
  await expect(page.locator("#clock-editor-bar .clock-field-chip")).toHaveCount(6);

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
  await expect(page.locator("#menu-items-panel")).toHaveAttribute(
    "data-menu-interactive",
    "true",
    { timeout: 15_000 },
  );
  await expect(page.locator("#menu-screen-title")).toHaveText("");

  const finalObserved = await page.evaluate(
    () =>
      (window as unknown as { __menuReadiness: { violations: string[] } }).__menuReadiness
        .violations,
  );
  expect(finalObserved).toEqual([]);
});
