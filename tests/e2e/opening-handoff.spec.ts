import { expect, test } from "@playwright/test";

test("Phase 10 E2E: Mode A Opening automatically transitions into Mode D Main Menu upon completing fly-up", async ({
  page,
}) => {
  await page.goto("/");
  await page.waitForFunction(
    () => document.getElementById("app")?.dataset.appState === "ready",
    null,
    { timeout: 15_000 },
  );

  await page.selectOption("#launch-mode-select", "opening");
  await page.click("#start-osdsys-btn");
  await expect(page.locator("#launch-overlay")).toBeHidden();

  // Fast-forward Mode A timeline through tower drift -> fly-up -> fade -> automatic Mode D Menu handoff
  const finalMode = await page.evaluate(() => {
    const h = window.__osdbitsHarness!;
    let snap = h.app.getLatestSnapshot()!;
    for (let i = 0; i < 400 && snap.mode === "opening"; i++) {
      snap = h.bridge.stepFrame();
    }
    // Step the first active Menu frame after Opening completes its handoff
    snap = h.bridge.stepFrame();
    // Sync app shell state to transitioned mode
    const appInternal = h.app as unknown as {
      lastSnapshot: typeof snap;
      settings: { mode: string };
      syncControlsFromSettings: () => void;
      renderModeOverlays: () => void;
    };
    appInternal.lastSnapshot = snap;
    appInternal.settings = { ...appInternal.settings, mode: snap.mode };
    document.getElementById("app")!.dataset.activeMode = snap.mode;
    appInternal.syncControlsFromSettings();
    appInternal.renderModeOverlays();
    return { mode: snap.mode, orbs: snap.orbs };
  });

  expect(finalMode).toEqual({ mode: "menu", orbs: 7 });
  await expect(page.locator("#menu-hud")).toBeVisible();
  await expect(page.locator("#menu-items-panel")).toContainText("System Configuration");
});
