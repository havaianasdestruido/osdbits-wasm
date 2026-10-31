import { expect, test } from "@playwright/test";

/**
 * Settings must survive a reload. Only meaningful when the browser grants
 * IndexedDB or localStorage; when it does not, the app must say so instead of
 * pretending.
 */
test("settings survive a reload", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.locator("#field-quality").selectOption("low");
  await page.locator("#settings-save").click();
  await expect(page.locator("#settings-status")).toContainText(/saved|applied/);

  const durable = (await page.locator("#settings-status").innerText()).includes("saved");
  await page.reload();
  await page.getByRole("button", { name: "Settings" }).click();
  if (durable) {
    await expect(page.locator("#field-quality")).toHaveValue("low");
  } else {
    // Honest degraded path: defaults, and the UI states that nothing persists.
    await expect(page.locator("#field-quality")).toHaveValue("high");
    await expect(page.locator("#capabilities")).toContainText(/will not persist/i);
  }
});

test("reset restores defaults", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.locator("#field-seed").fill("1234");
  await page.locator("#settings-save").click();
  await page.locator("#settings-reset").click();
  await expect(page.locator("#settings-status")).toContainText(/defaults restored/);
  await expect(page.locator("#field-seed")).toHaveValue("4242");
});
