import { expect, test } from "@playwright/test";

test("the menu is fully keyboard operable", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Start OSDSYS" }).click();
  await expect(page.locator("#start")).toBeHidden({ timeout: 20_000 });

  await page.locator("body").press("m");
  const menu = page.getByRole("menu");
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitem").first()).toHaveAttribute("aria-selected", "true");

  await page.keyboard.press("ArrowDown");
  await expect(menu.getByRole("menuitem").nth(1)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowUp");
  await expect(menu.getByRole("menuitem").first()).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("End");
  await expect(menu.getByRole("menuitem").last()).toHaveAttribute("aria-selected", "true");

  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(page.getByRole("button", { name: "Menu" })).toBeFocused();
});

test("choosing a scene switches the running scene", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Start OSDSYS" }).click();
  await expect(page.locator("#start")).toBeHidden({ timeout: 20_000 });
  await page.getByRole("button", { name: "Menu" }).click();
  await page.getByRole("menuitem", { name: /Illegal-disc scene/ }).click();
  await page.getByRole("button", { name: "Diagnostics" }).click();
  await expect(page.locator("#diagnostics-list")).toContainText(/scene\s+1/);
});
