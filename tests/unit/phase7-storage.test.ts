import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import {
  DEFAULT_FAITHFUL_SETTINGS,
  RESET_CONFIRMATION_TOKEN,
  StorageManager,
} from "../../web/src/storage/storageManager";

describe("Phase 7 — IndexedDB StorageManager, Migrations, Read-Back & Conflict Protection", () => {
  it("saves settings with mandatory read-back verification, JSON export/import, and two-step confirmed reset", async () => {
    const idb = new IDBFactory();
    const mgr = new StorageManager({ dbName: "test_v2_basic", idbFactory: idb, enableOpfs: false });

    const init = await mgr.init();
    expect(init.backend).toBe("indexeddb");
    expect(init.settings).toEqual(DEFAULT_FAITHFUL_SETTINGS);

    const custom = {
      ...DEFAULT_FAITHFUL_SETTINGS,
      mode: "menu" as const,
      pal: true,
      aspectMode: "16:9" as const,
      language: 7 as const,
      rngSeed: 987654,
      towerPreset: { kind: "simulated" as const, seed: 55, ngames: 64, nboots: 300 },
      masterVolume: 0.42,
    };

    const saveRes = await mgr.saveSettings(custom);
    expect(saveRes.ok).toBe(true);
    expect(saveRes.verifiedReadBack).toBe(true);
    expect(saveRes.revision).toBe(1);

    const exported = await mgr.exportSettingsJson();
    expect(exported).toContain('"mode": "menu"');

    // Unconfirmed reset MUST be rejected
    await expect(mgr.resetToFaithfulDefaults("WRONG_TOKEN")).rejects.toThrow(
      /confirmation token required/,
    );

    // Confirmed reset restores faithful defaults and verifies read-back
    const resetRes = await mgr.resetToFaithfulDefaults(RESET_CONFIRMATION_TOKEN);
    expect(resetRes.ok).toBe(true);
    expect(resetRes.verifiedReadBack).toBe(true);
    const afterReset = await mgr.loadSettings();
    expect(afterReset.settings).toEqual(DEFAULT_FAITHFUL_SETTINGS);

    // Re-importing exported JSON restores custom settings cleanly
    const impRes = await mgr.importSettingsJson(exported);
    expect(impRes.ok).toBe(true);
    expect(impRes.verifiedReadBack).toBe(true);
    expect((await mgr.loadSettings()).settings).toEqual(custom);

    mgr.close();
  });

  it("migrates legacy v1 schema records cleanly into v2 and repairs corrupted checksums", async () => {
    const idb = new IDBFactory();

    // Seed a legacy v1 record directly in IndexedDB
    await new Promise<void>((resolve, reject) => {
      const req = idb.open("test_v1_mig", 2);
      req.onupgradeneeded = () => {
        req.result.createObjectStore("settings");
        req.result.createObjectStore("assets_meta");
        req.result.createObjectStore("assets_blobs");
      };
      req.onsuccess = () => {
        const db = req.result;
        const tx = db.transaction("settings", "readwrite");
        tx.objectStore("settings").put(
          {
            version: 1,
            revision: 3,
            mode: "illegal-disc",
            pal: true,
            muted: true,
            volume: 0.35,
          },
          "user_settings",
        );
        tx.oncomplete = () => {
          db.close();
          resolve();
        };
        tx.onerror = () => reject(tx.error);
      };
    });

    const mgr = new StorageManager({ dbName: "test_v1_mig", idbFactory: idb, enableOpfs: false });
    const init = await mgr.init();
    expect(init.migratedFromV1).toBe(true);
    expect(init.settings.mode).toBe("illegal-disc");
    expect(init.settings.pal).toBe(true);
    expect(init.settings.audioMuted).toBe(true);
    expect(init.settings.masterVolume).toBe(0.35);
    expect(init.revision).toBe(4);
    mgr.close();

    // Corrupt checksum on disk -> manager detects corruption and restores faithful defaults
    await new Promise<void>((resolve, reject) => {
      const req = idb.open("test_v1_mig", 2);
      req.onsuccess = () => {
        const db = req.result;
        const tx = db.transaction("settings", "readwrite");
        tx.objectStore("settings").put(
          {
            schemaVersion: 2,
            revision: 9,
            updatedAtIso: "2026-10-31T00:00:00Z",
            checksumFnv1a: "deadbeef",
            settings: { ...DEFAULT_FAITHFUL_SETTINGS, mode: "idle" },
          },
          "user_settings",
        );
        tx.oncomplete = () => {
          db.close();
          resolve();
        };
        tx.onerror = () => reject(tx.error);
      };
    });

    const mgrCorrupt = new StorageManager({
      dbName: "test_v1_mig",
      idbFactory: idb,
      enableOpfs: false,
    });
    const recovered = await mgrCorrupt.init();
    expect(recovered.settings).toEqual(DEFAULT_FAITHFUL_SETTINGS);
    expect(recovered.warning).toMatch(/checksum mismatch/i);
    mgrCorrupt.close();
  });

  it("prevents stale multi-tab overwrites and falls back cleanly on missing IDB or quota errors", async () => {
    const sharedIdb = new IDBFactory();
    const tabA = new StorageManager({ dbName: "test_conflict", idbFactory: sharedIdb, enableOpfs: false });
    const tabB = new StorageManager({ dbName: "test_conflict", idbFactory: sharedIdb, enableOpfs: false });

    await tabA.init();
    await tabB.init();

    // Tab A writes revision 1
    const rA = await tabA.saveSettings({ ...DEFAULT_FAITHFUL_SETTINGS, mode: "idle" }, 0);
    expect(rA.ok).toBe(true);
    expect(rA.revision).toBe(1);

    // Tab B attempts stale write expecting base revision 0 -> rejected cleanly without clobbering!
    const rB = await tabB.saveSettings({ ...DEFAULT_FAITHFUL_SETTINGS, mode: "menu" }, 0);
    expect(rB.ok).toBe(false);
    expect(rB.conflict).toBe(true);
    expect((await tabA.loadSettings()).settings.mode).toBe("idle");

    // Quota error simulation falls back to session memory cleanly
    tabA.setSimulateQuotaExceeded(true);
    const quotaSave = await tabA.saveSettings({ ...DEFAULT_FAITHFUL_SETTINGS, reducedMotion: true });
    expect(quotaSave.backend).toBe("memory-fallback");
    expect((await tabA.loadSettings()).settings.reducedMotion).toBe(true);

    // Unavailable IDB falls back to session memory cleanly
    const noIdb = new StorageManager({ idbFactory: null });
    const noIdbInit = await noIdb.init();
    expect(noIdbInit.backend).toBe("memory-fallback");

    tabA.close();
    tabB.close();
    noIdb.close();
  });

  it("persists imported binary assets with SHA-256 read-back verification", async () => {
    const idb = new IDBFactory();
    const mgr = new StorageManager({ dbName: "test_assets", idbFactory: idb, enableOpfs: false });
    await mgr.init();

    const payload = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    const saved = await mgr.saveImportedAsset(payload, ["TEXOWAL0", "TEXCKABE"]);
    expect(saved.ok).toBe(true);
    expect(saved.verifiedReadBack).toBe(true);
    expect(saved.meta.sha256).toMatch(/^[0-9a-f]{64}$/);

    const loaded = await mgr.loadImportedAsset();
    expect(loaded).not.toBeNull();
    expect(Array.from(loaded!.bytes)).toEqual(Array.from(payload));
    expect(loaded!.meta.importedSlots).toEqual(["TEXOWAL0", "TEXCKABE"]);

    mgr.close();
  });
});
