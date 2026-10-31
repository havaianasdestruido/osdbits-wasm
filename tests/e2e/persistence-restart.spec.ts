import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium, expect, test } from "@playwright/test";
import { preparePlaywrightChromiumOptions } from "../../scripts/chromium-env.mjs";

test("Phase 7 persistent storage: survives (a) context reload AND (b) full browser process termination + relaunch on disk userDataDir", async ({
  baseURL,
}) => {
  const userDataDir = mkdtempSync(join(tmpdir(), "osdbits-persistent-profile-"));
  const { launchOptions } = await preparePlaywrightChromiumOptions();

  try {
    // --- Browser Process #1: Write custom settings & binary asset, verify page reload ---
    const ctx1 = await chromium.launchPersistentContext(userDataDir, launchOptions);
    const page1 = await ctx1.newPage();
    await page1.goto(baseURL ?? "http://127.0.0.1:5173");
    await expect(page1.locator("#app")).toHaveAttribute("data-app-state", "ready");

    const written = await page1.evaluate(async () => {
      const { storage } = window.__osdbitsHarness!;
      const loaded = await storage.loadSettings();
      const saveSettingsRes = await storage.saveSettings({
        ...loaded.settings,
        mode: "menu",
        pal: true,
        aspectMode: "16:9",
        language: 4,
        rngSeed: 20261031,
        reducedMotion: true,
        masterVolume: 0.55,
      });
      const sampleAsset = new Uint8Array([0x4f, 0x53, 0x44, 0x42, 0x01, 0x02, 0x03, 0x04]);
      const saveAssetRes = await storage.saveImportedAsset(sampleAsset, ["TEXOWAL0"]);
      return {
        saveSettingsRes,
        assetMeta: saveAssetRes.meta,
        assetVerified: saveAssetRes.verifiedReadBack,
      };
    });

    expect(written.saveSettingsRes.ok).toBe(true);
    expect(written.saveSettingsRes.verifiedReadBack).toBe(true);
    expect(written.assetVerified).toBe(true);

    // (a) Verify reload within the same running browser process
    await page1.reload();
    await expect(page1.locator("#app")).toHaveAttribute("data-app-state", "ready");
    const afterReload = await page1.evaluate(async () => {
      const { storage } = window.__osdbitsHarness!;
      const s = await storage.loadSettings();
      const a = await storage.loadImportedAsset();
      return {
        settings: s.settings,
        revision: s.revision,
        assetSha: a?.meta.sha256 ?? null,
        assetLen: a?.bytes.byteLength ?? 0,
      };
    });
    expect(afterReload.settings.mode).toBe("menu");
    expect(afterReload.settings.rngSeed).toBe(20261031);
    expect(afterReload.assetSha).toBe(written.assetMeta.sha256);
    expect(afterReload.assetLen).toBe(8);

    // Terminate Browser Process #1 completely!
    await ctx1.close();

    // --- Browser Process #2: Cold relaunch against the same on-disk userDataDir ---
    const ctx2 = await chromium.launchPersistentContext(userDataDir, launchOptions);
    const page2 = await ctx2.newPage();
    await page2.goto(baseURL ?? "http://127.0.0.1:5173");
    await expect(page2.locator("#app")).toHaveAttribute("data-app-state", "ready");

    const afterColdRestart = await page2.evaluate(async () => {
      const { storage } = window.__osdbitsHarness!;
      const s = await storage.loadSettings();
      const a = await storage.loadImportedAsset();
      return {
        backend: s.backend,
        settings: s.settings,
        revision: s.revision,
        assetSha: a?.meta.sha256 ?? null,
        assetStorageBackend: a?.meta.storageBackend ?? null,
        assetBytes: a ? Array.from(a.bytes) : [],
      };
    });

    expect(afterColdRestart.backend).toBe("indexeddb");
    expect(afterColdRestart.settings.mode).toBe("menu");
    expect(afterColdRestart.settings.pal).toBe(true);
    expect(afterColdRestart.settings.aspectMode).toBe("16:9");
    expect(afterColdRestart.settings.language).toBe(4);
    expect(afterColdRestart.settings.rngSeed).toBe(20261031);
    expect(afterColdRestart.settings.reducedMotion).toBe(true);
    expect(afterColdRestart.settings.masterVolume).toBe(0.55);
    expect(afterColdRestart.assetSha).toBe(written.assetMeta.sha256);
    expect(afterColdRestart.assetBytes).toEqual([0x4f, 0x53, 0x44, 0x42, 0x01, 0x02, 0x03, 0x04]);

    await ctx2.close();
  } finally {
    rmSync(userDataDir, { recursive: true, force: true });
  }
});
