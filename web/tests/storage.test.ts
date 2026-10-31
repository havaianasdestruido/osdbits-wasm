import { describe, expect, it } from "vitest";
import { MemoryStore, Repository, SETTINGS_KEY, openStore } from "../src/storage.js";
import { DEFAULT_SETTINGS, SETTINGS_VERSION } from "../src/settings.js";

describe("Repository", () => {
  it("returns defaults when nothing is stored", async () => {
    const repo = new Repository(new MemoryStore());
    const { settings, migrated } = await repo.loadSettings();
    expect(settings).toEqual(DEFAULT_SETTINGS);
    expect(migrated).toBe(false);
  });

  it("round-trips settings", async () => {
    const store = new MemoryStore();
    const repo = new Repository(store);
    await repo.saveSettings({ ...DEFAULT_SETTINGS, scene: "idle", volume: 0.2, seed: 99 });
    const { settings } = await repo.loadSettings();
    expect(settings.scene).toBe("idle");
    expect(settings.volume).toBe(0.2);
    expect(settings.seed).toBe(99);
  });

  it("migrates a record with an older schema version", async () => {
    const store = new MemoryStore();
    await store.set(SETTINGS_KEY, JSON.stringify({ version: -1, scene: "illegal" }));
    const repo = new Repository(store);
    const { settings, migrated } = await repo.loadSettings();
    expect(migrated).toBe(true);
    expect(settings.version).toBe(SETTINGS_VERSION);
    expect(settings.scene).toBe("illegal");
    // The migrated record is written back.
    const raw = await store.get(SETTINGS_KEY);
    expect(JSON.parse(raw as string).version).toBe(SETTINGS_VERSION);
  });

  it("recovers from a corrupt record and keeps a copy", async () => {
    const store = new MemoryStore();
    await store.set(SETTINGS_KEY, "{not json");
    const repo = new Repository(store);
    const { settings, migrated } = await repo.loadSettings();
    expect(migrated).toBe(true);
    expect(settings).toEqual(DEFAULT_SETTINGS);
    expect(await store.get(`${SETTINGS_KEY}.corrupt`)).toBe("{not json");
  });

  it("persists a session log", async () => {
    const repo = new Repository(new MemoryStore());
    expect((await repo.loadSession()).launches).toBe(0);
    await repo.saveSession({ launches: 3, lastScene: "boot", totalSeconds: 12, lastSeen: 5 });
    const session = await repo.loadSession();
    expect(session.launches).toBe(3);
    expect(session.totalSeconds).toBe(12);
  });

  it("resets both records", async () => {
    const store = new MemoryStore();
    const repo = new Repository(store);
    await repo.saveSettings(DEFAULT_SETTINGS);
    await repo.reset();
    expect(await store.get(SETTINGS_KEY)).toBeNull();
  });

  it("reports non-durable storage", async () => {
    const repo = new Repository(new MemoryStore());
    expect(repo.durable).toBe(false);
    expect(repo.backend).toBe("memory");
  });

  it("falls back to a memory store in a bare Node environment", async () => {
    const store = await openStore();
    expect(["memory", "localstorage", "indexeddb"]).toContain(store.kind);
    await store.set("k", "v");
    expect(await store.get("k")).toBe("v");
    await store.delete("k");
    expect(await store.get("k")).toBeNull();
  });
});
