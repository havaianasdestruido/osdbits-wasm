import { expect, test } from "@playwright/test";

test("Phases 5 & 6 WebGPU integration: Modes A/B/C/D pixel readback, zero validation errors, resize VRAM cleanup, and device.lost recovery", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("#app")).toHaveAttribute("data-app-state", "ready");

  const report = await page.evaluate(async () => {
    const harness = window.__osdbitsHarness;
    if (!harness) {
      throw new Error("__osdbitsHarness not initialized");
    }
    const { bridge } = harness;
    const renderer = await harness.createRenderer(0.5); // 320x224 fast deterministic offscreen target

    function summarizePixels(rgba: Uint8Array): {
      nonBlackRatio: number;
      avgR: number;
      avgG: number;
      avgB: number;
      litAvgR: number;
      litAvgG: number;
      litAvgB: number;
    } {
      const pxCount = rgba.length / 4;
      let nonBlack = 0;
      let sumR = 0;
      let sumG = 0;
      let sumB = 0;
      let litR = 0;
      let litG = 0;
      let litB = 0;
      for (let i = 0; i < rgba.length; i += 4) {
        const r = rgba[i] ?? 0;
        const g = rgba[i + 1] ?? 0;
        const b = rgba[i + 2] ?? 0;
        sumR += r;
        sumG += g;
        sumB += b;
        if (r + g + b > 6) {
          nonBlack++;
          litR += r;
          litG += g;
          litB += b;
        }
      }
      const litDiv = Math.max(1, nonBlack);
      return {
        nonBlackRatio: nonBlack / pxCount,
        avgR: sumR / pxCount,
        avgG: sumG / pxCount,
        avgB: sumB / pxCount,
        litAvgR: litR / litDiv,
        litAvgG: litG / litDiv,
        litAvgB: litB / litDiv,
      };
    }

    const validationErrors: string[] = [];

    // 1. Mode A (Opening): step & render frames 30, 140 (banner + cubes + towers), 220 (fly-up)
    bridge.initScene({ mode: "opening", rngSeed: 1, autoTransitionToMenu: false });
    let snap = bridge.stepFrame();
    for (let f = 2; f <= 30; f++) {
      snap = bridge.stepFrame();
      if (f >= 28) {
        const err = await renderer.renderFrameChecked(
          bridge.getVertexView(),
          bridge.getCommandView(),
          snap.mode,
        );
        if (err) validationErrors.push(`ModeA@f${f}: ${err}`);
      }
    }
    const modeAF30 = summarizePixels((await renderer.captureScenePixels()).rgba);

    for (let f = 31; f <= 140; f++) {
      snap = bridge.stepFrame();
    }
    {
      const err = await renderer.renderFrameChecked(
        bridge.getVertexView(),
        bridge.getCommandView(),
        snap.mode,
      );
      if (err) validationErrors.push(`ModeA@f140: ${err}`);
    }
    const modeAF140 = summarizePixels((await renderer.captureScenePixels()).rgba);

    for (let f = 141; f <= 220; f++) {
      snap = bridge.stepFrame();
    }
    {
      const err = await renderer.renderFrameChecked(
        bridge.getVertexView(),
        bridge.getCommandView(),
        snap.mode,
      );
      if (err) validationErrors.push(`ModeA@f220: ${err}`);
    }
    const modeAF220 = summarizePixels((await renderer.captureScenePixels()).rgba);

    // 2. Mode B (Idle) at frame 300
    bridge.initScene({ mode: "idle", rngSeed: 1 });
    for (let f = 1; f <= 300; f++) {
      snap = bridge.stepFrame();
    }
    {
      const err = await renderer.renderFrameChecked(
        bridge.getVertexView(),
        bridge.getCommandView(),
        snap.mode,
      );
      if (err) validationErrors.push(`ModeB@f300: ${err}`);
    }
    const modeBF300 = summarizePixels((await renderer.captureScenePixels()).rgba);

    // 3. Mode C (Illegal-Disc) at frame 5 (fading in from black) and frame 180 (red glow + cubes + text)
    bridge.initScene({ mode: "illegal-disc", rngSeed: 1, language: 1 });
    for (let f = 1; f <= 5; f++) {
      snap = bridge.stepFrame();
    }
    {
      const err = await renderer.renderFrameChecked(
        bridge.getVertexView(),
        bridge.getCommandView(),
        snap.mode,
      );
      if (err) validationErrors.push(`ModeC@f5: ${err}`);
    }
    const modeCF5 = summarizePixels((await renderer.captureScenePixels()).rgba);

    for (let f = 6; f <= 180; f++) {
      snap = bridge.stepFrame();
      if (f >= 178) {
        const err = await renderer.renderFrameChecked(
          bridge.getVertexView(),
          bridge.getCommandView(),
          snap.mode,
        );
        if (err) validationErrors.push(`ModeC@f${f}: ${err}`);
      }
    }
    const modeCF180 = summarizePixels((await renderer.captureScenePixels()).rgba);

    // 4. Mode D (Main Menu orbs -> System Configuration KABE + 12 crystal rods + 5 glass cubes)
    bridge.initScene({
      mode: "menu",
      rngSeed: 1,
      clock: { year: 2026, month: 10, day: 31, hour: 15, minute: 20, second: 30 },
    });
    for (let f = 1; f <= 130; f++) {
      snap = bridge.stepFrame();
    }
    {
      const err = await renderer.renderFrameChecked(
        bridge.getVertexView(),
        bridge.getCommandView(),
        snap.mode,
      );
      if (err) validationErrors.push(`ModeD_Main@f130: ${err}`);
    }
    const modeDMain = summarizePixels((await renderer.captureScenePixels()).rgba);

    bridge.enterSystemConfig();
    for (let f = 1; f <= 95; f++) {
      snap = bridge.stepFrame();
    }
    {
      const err = await renderer.renderFrameChecked(
        bridge.getVertexView(),
        bridge.getCommandView(),
        snap.mode,
      );
      if (err) validationErrors.push(`ModeD_SysCfg@f225: ${err}`);
    }
    const modeDSysCfg = summarizePixels((await renderer.captureScenePixels()).rgba);

    // Exercise Mode D Clock Adjustment interaction (orbs sink, clock fields edited & confirmed)
    bridge.dispatchMenuAction("confirm");
    for (let f = 1; f <= 30; f++) {
      snap = bridge.stepFrame();
    }
    const clockScreenBeforeCommit = snap.menuUi?.screen ?? "none";
    bridge.dispatchMenuAction("up");
    bridge.dispatchMenuAction("confirm");
    snap = bridge.stepFrame();
    const clockScreenAfterCommit = snap.menuUi?.screen ?? "none";
    {
      const err = await renderer.renderFrameChecked(
        bridge.getVertexView(),
        bridge.getCommandView(),
        snap.mode,
      );
      if (err) validationErrors.push(`ModeD_ClockAdj: ${err}`);
    }

    // 5. Render-target resize VRAM cleanup check
    const statsBeforeResize = renderer.getResourceStats();
    renderer.setDisplayConfig({ resolutionScale: 1.0 });
    renderer.setDisplayConfig({ resolutionScale: 0.5 });
    const statsAfterResize = renderer.getResourceStats();

    // 6. Simulated device.lost recovery without page reload
    await renderer.simulateDeviceLossAndRecover();
    snap = bridge.stepFrame();
    const errAfterRecover = await renderer.renderFrameChecked(
      bridge.getVertexView(),
      bridge.getCommandView(),
      snap.mode,
    );
    if (errAfterRecover) validationErrors.push(`PostRecovery: ${errAfterRecover}`);
    const postRecoverPixels = summarizePixels((await renderer.captureScenePixels()).rgba);
    const finalStats = renderer.getResourceStats();

    renderer.dispose();

    return {
      validationErrors,
      modeAF30,
      modeAF140,
      modeAF220,
      modeBF300,
      modeCF5,
      modeCF180,
      modeDMain,
      modeDSysCfg,
      clockScreenBeforeCommit,
      clockScreenAfterCommit,
      destroyedDeltaOnTwoResizes:
        statsAfterResize.destroyedTextures - statsBeforeResize.destroyedTextures,
      deviceLostRecoveries: finalStats.deviceLostRecoveries,
      postRecoverPixels,
    };
  });

  // Zero WebGPU validation errors across every pass of Modes A, B, C, and D!
  expect(report.validationErrors).toEqual([]);

  // Mode A & Mode B render substantial non-blank cool-blue/neutral lit geometry & clouds
  expect(report.modeAF30.nonBlackRatio).toBeGreaterThan(0.15);
  expect(report.modeAF140.nonBlackRatio).toBeGreaterThan(0.2);
  expect(report.modeAF220.nonBlackRatio).toBeGreaterThan(0.05);
  expect(report.modeBF300.nonBlackRatio).toBeGreaterThan(0.2);

  // Mode C (Illegal-Disc): fades up from near-black at frame 5 to warm red dominance (R > G and R > B) at frame 180!
  expect(report.modeCF180.avgR).toBeGreaterThan(report.modeCF5.avgR + 5);
  expect(report.modeCF180.litAvgR).toBeGreaterThan(report.modeCF180.litAvgG * 1.25);
  expect(report.modeCF180.litAvgR).toBeGreaterThan(report.modeCF180.litAvgB * 1.25);

  // Mode D: Main Menu 7 orbs render luminous trails/cores on dark void, and System Config fills backdrop + rods + cubes!
  expect(report.modeDMain.nonBlackRatio).toBeGreaterThan(0.005);
  expect(report.modeDSysCfg.nonBlackRatio).toBeGreaterThan(0.5);
  expect(report.modeDSysCfg.litAvgB).toBeGreaterThan(report.modeDSysCfg.litAvgG * 0.9);
  expect(report.clockScreenBeforeCommit).toBe("config-edit-clock");
  expect(report.clockScreenAfterCommit).toBe("config-list");

  // Two resolution changes destroy 7 * 2 = 14 old render-target GPUTextures cleanly
  expect(report.destroyedDeltaOnTwoResizes).toBe(14);

  // Device loss recovery succeeded and rendered valid frames without reloading the page
  expect(report.deviceLostRecoveries).toBe(1);
  expect(report.postRecoverPixels.nonBlackRatio).toBeGreaterThan(0.5);
});
