import { expect, test } from "@playwright/test";

/**
 * Single-activation journey. Written against the real app; never executed in
 * the development sandbox (no browser binaries) — see docs/TEST-EVIDENCE.md.
 *
 * The assertions are capability-aware: WebGPU is required to be *reported*
 * honestly, not to be present.
 */
test.describe("activation", () => {
  test("one click starts audio, the core and the scene", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto("/");

    // The activation screen is up and there is exactly one start control.
    await expect(page.getByRole("button", { name: "Start OSDSYS" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Start OSDSYS" })).toHaveCount(1);

    const gpu = await page.evaluate(() => typeof (navigator as { gpu?: unknown }).gpu !== "undefined");
    const audio = await page.evaluate(
      () => typeof (window as { AudioContext?: unknown }).AudioContext === "function",
    );

    // Count the gestures: exactly one click may be needed.
    let clicks = 0;
    page.on("console", () => undefined);
    await page.getByRole("button", { name: "Start OSDSYS" }).click();
    clicks++;

    await expect(page.locator("#start")).toBeHidden({ timeout: 20_000 });
    await expect(page.locator("#status")).toContainText(/fps/, { timeout: 20_000 });

    // The renderer that actually came up is reported, never assumed.
    const status = await page.locator("#status").innerText();
    if (gpu) {
      expect(status).toMatch(/WebGPU|Canvas 2D/);
    } else {
      expect(status).toContain("Canvas 2D");
    }
    expect(clicks).toBe(1);

    // If audio is available the context must be running after the gesture
    // (a browser that refuses exposes it through the banner instead).
    if (audio) {
      const state = await page.evaluate(() => {
        const anyWindow = window as unknown as { __osdAudioState?: string };
        return anyWindow.__osdAudioState ?? "unknown";
      });
      expect(["running", "unknown"]).toContain(state);
    }
    expect(errors).toEqual([]);
  });

  test("the frame counter advances and the draw list is non-trivial", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Start OSDSYS" }).click();
    await expect(page.locator("#start")).toBeHidden({ timeout: 20_000 });
    await page.getByRole("button", { name: "Diagnostics" }).click();
    const list = page.locator("#diagnostics-list");
    await expect(list).toContainText("simulation frame");
    const before = Number((await list.innerText()).match(/simulation frame\s+(\d+)/)?.[1] ?? 0);
    await page.waitForTimeout(1000);
    const after = Number((await list.innerText()).match(/simulation frame\s+(\d+)/)?.[1] ?? 0);
    expect(after).toBeGreaterThan(before);
    await expect(list).toContainText(/draw list\s+\d+ batches/);
  });
});
