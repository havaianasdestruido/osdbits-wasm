import { expect, test } from "@playwright/test";
import { clickUi, drawnTextIncludes, waitForAppReady } from "./support/uiHarness";

test("Phase 10 E2E: Mode A Opening automatically transitions into Mode D Main Menu upon completing fly-up", async ({
  page,
}) => {
  await page.goto("/");
  await waitForAppReady(page);

  await clickUi(page, "title-start");
  await expect(page.locator("#app")).toHaveAttribute("data-scene-active", "true");

  // Fast-forward Mode A timeline through tower drift -> fly-up -> fade -> automatic Mode D Menu handoff.
  const finalMode = await page.evaluate(() => {
    const h = window.__osdbitsHarness!;
    let snap = h.app.getLatestSnapshot()!;
    for (let i = 0; i < 400 && snap.mode === "opening"; i++) {
      snap = h.bridge.stepFrame();
    }
    // Step the first active Menu frame after Opening completes its handoff.
    snap = h.bridge.stepFrame();
    return { mode: snap.mode, orbs: snap.orbs };
  });

  expect(finalMode).toEqual({ mode: "menu", orbs: 7 });
  // The app's own loop picks up the transition on its next step.
  await expect(page.locator("#app")).toHaveAttribute("data-ui-screen", "menu");
  await expect.poll(() => drawnTextIncludes(page, "System Configuration")).toBe(true);
});
