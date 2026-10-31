import { expect, test } from "@playwright/test";
import { hasDurableStorage, openApp, openPanel, startScene } from "./support.js";

/**
 * Persistence. IndexedDB is the primary store, localStorage the fallback and
 * memory the last resort. The specs accept both outcomes but insist that the UI
 * says which one happened — a browser without storage must not silently lose
 * settings without telling the user.
 */
test.describe("persistence", () => {
  test("settings survive a reload, or the UI says they do not", async ({ page }) => {
    await openApp(page);
    await openPanel(page, "Settings");
    await page.locator("#field-quality").selectOption("low");
    await page.locator("#settings-save").click();
    const saved = await page.locator("#settings-status").innerText();
    expect(saved).toMatch(/saved|applied/i);

    const durable = /saved/i.test(saved);
    await page.reload();
    await expect(page.locator("body")).toHaveAttribute("data-boot", "ready", { timeout: 45_000 });
    await openPanel(page, "Settings");

    if (durable) {
      await expect(page.locator("#field-quality")).toHaveValue("low");
    } else {
      await expect(page.locator("#field-quality")).toHaveValue("high");
      await expect(page.locator("#capabilities")).toContainText(/will not persist/i);
    }
  });

  test("reset restores the documented defaults", async ({ page }) => {
    await openApp(page);
    await openPanel(page, "Settings");
    await page.locator("#field-seed").fill("1234");
    await page.locator("#settings-save").click();
    await page.locator("#settings-reset").click();
    await expect(page.locator("#settings-status")).toContainText(/defaults restored/i);
    await expect(page.locator("#field-seed")).toHaveValue("4242");
    await expect(page.locator("#field-quality")).toHaveValue("high");
  });

  test("the chosen scene is restored on the next start", async ({ page }) => {
    await openApp(page);
    await openPanel(page, "Settings");
    const durable = await hasDurableStorage(page);
    test.skip(!durable, "this browser exposes no durable storage, so nothing can be restored");
    await page.locator("#field-scene").selectOption("illegal");
    await page.locator("#settings-save").click();
    await page.reload();
    await expect(page.locator("body")).toHaveAttribute("data-boot", "ready", { timeout: 45_000 });
    await startScene(page);
    // Scene 1 is the illegal-disc scene; the core reports it in the header.
    await expect(page.locator("body")).toHaveAttribute("data-scene", "1", { timeout: 20_000 });
  });
});
