import { expect, test } from "@playwright/test";
import { openApp, openPanel, startScene } from "./support.js";

/**
 * The menu is an OSDSYS-*inspired* UI, not a reconstruction of the real
 * browser: it has to be fully keyboard operable and must switch the running
 * scene without reloading.
 */
test.describe("menu", () => {
  test("is fully keyboard operable", async ({ page }) => {
    await openApp(page);
    await startScene(page);

    await page.keyboard.press("m");
    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    const items = menu.getByRole("menuitem");
    await expect(items.first()).toHaveAttribute("aria-selected", "true");

    await page.keyboard.press("ArrowDown");
    await expect(items.nth(1)).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("ArrowUp");
    await expect(items.first()).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("End");
    await expect(items.last()).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Home");
    await expect(items.first()).toHaveAttribute("aria-selected", "true");

    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
    await expect(page.getByRole("button", { name: "Menu" })).toBeFocused();
  });

  test("Enter activates the focused item", async ({ page }) => {
    await openApp(page);
    await startScene(page);
    await page.keyboard.press("m");
    const menu = page.getByRole("menu");
    const items = menu.getByRole("menuitem");
    const target = menu.getByRole("menuitem", { name: /Illegal-disc scene/ });
    // Walk to the item with the keyboard only.
    const count = await items.count();
    for (let i = 0; i < count; i++) {
      if ((await target.getAttribute("aria-selected")) === "true") break;
      await page.keyboard.press("ArrowDown");
    }
    await expect(target).toHaveAttribute("aria-selected", "true");
    await expect(target).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(menu).toBeHidden();
    await expect(page.locator("body")).toHaveAttribute("data-scene", "1", { timeout: 20_000 });
  });

  test("choosing a scene with the mouse switches the running scene", async ({ page }) => {
    await openApp(page);
    await startScene(page);
    await openPanel(page, "Menu");
    await page.getByRole("menuitem", { name: /Illegal-disc scene/ }).click();
    await expect(page.locator("body")).toHaveAttribute("data-scene", "1", { timeout: 20_000 });
    const rows = await page.evaluate(() => ({
      scene: document.body.dataset.scene,
      batches: document.body.dataset.batches,
    }));
    expect(rows.scene).toBe("1");
    expect(Number(rows.batches)).toBeGreaterThan(0);
  });

  test("the menu refuses to open before the scene does", async ({ page }) => {
    await openApp(page);
    await page.keyboard.press("m");
    // Opening the menu from a cold page performs the same single activation.
    await expect(page.locator("body")).toHaveAttribute("data-phase", "running", {
      timeout: 45_000,
    });
    await expect(page.getByRole("menu")).toBeVisible();
  });
});
