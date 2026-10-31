/**
 * Persistence tests over a real IndexedDB implementation (`fake-indexeddb`).
 * Every case asserts the read-back contract the browser layer relies on.
 */

import { IDBFactory } from 'fake-indexeddb';
import { beforeEach, describe, expect, it } from 'vitest';
import {
  Persistence,
  checksumOf,
  defaultSettings,
  exportSettings,
  importSettings,
  sanitizeSettings,
} from '../../src/storage/persistence';

function installFreshIndexedDb(): void {
  const factory = new IDBFactory();
  Object.defineProperty(globalThis, 'indexedDB', { value: factory, configurable: true });
  Object.defineProperty(globalThis, 'navigator', {
    value: { storage: undefined },
    configurable: true,
  });
}

describe('persistence', () => {
  beforeEach(() => {
    installFreshIndexedDb();
  });

  it('saves settings and verifies them by reading the record back', async () => {
    const store = new Persistence();
    expect(await store.open()).toBe(true);
    const settings = { ...defaultSettings(), language: 4, bootHistory: new Array<number>(21).fill(3) };
    const result = await store.save(settings);
    expect(result.error).toBeNull();
    expect(result.verified).toBe(true);
    expect(result.revision).toBe(1);

    const loaded = await store.load();
    expect(loaded.error).toBeNull();
    expect(loaded.settings).not.toBeNull();
    expect(loaded.settings!.language).toBe(4);
    expect(loaded.settings!.revision).toBe(1);
    store.close();
  });

  it('persists across a re-opened database handle (reload simulation)', async () => {
    const first = new Persistence();
    await first.open();
    await first.save({ ...defaultSettings(), speed: 1.75, screenSize: 2 });
    first.close();

    const second = new Persistence();
    await second.open();
    const loaded = await second.load();
    expect(loaded.settings!.speed).toBeCloseTo(1.75, 5);
    expect(loaded.settings!.screenSize).toBe(2);
    second.close();
  });

  it('increments the revision monotonically so stale writes cannot win', async () => {
    const store = new Persistence();
    await store.open();
    const a = await store.save({ ...defaultSettings(), speed: 1.2 });
    const b = await store.save({ ...defaultSettings(), speed: 0.5 });
    expect(b.revision).toBeGreaterThan(a.revision);
    const loaded = await store.load();
    expect(loaded.settings!.speed).toBeCloseTo(0.5, 5);
    expect(loaded.settings!.revision).toBe(b.revision);
    store.close();
  });

  it('detects a corrupted record, quarantines it and restores defaults', async () => {
    const store = new Persistence();
    await store.open();
    await store.save({ ...defaultSettings(), language: 5 });
    // Tamper directly through a fresh handle.
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open('osdbits-wasm', 1);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('settings', 'readwrite');
      tx.objectStore('settings').put(
        { ...defaultSettings(), language: 5, checksum: 'deadbeef' },
        'settings',
      );
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
    db.close();

    const loaded = await store.load();
    expect(loaded.recoveredFromCorruption).toBe(true);
    expect(loaded.settings).not.toBeNull();
    expect(loaded.settings!.language).toBe(1);
    expect(loaded.error).toMatch(/checksum/i);
    store.close();
  });

  it('reports a migration when the stored schema is older', async () => {
    const store = new Persistence();
    await store.open();
    const legacy = { ...defaultSettings(), schemaVersion: 1, language: 3 };
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open('osdbits-wasm', 1);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('settings', 'readwrite');
      tx.objectStore('settings').put({ ...legacy, checksum: checksumOf(sanitizeSettings(legacy)) }, 'settings');
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
    db.close();
    const loaded = await store.load();
    expect(loaded.migratedFrom).toBe(1);
    expect(loaded.settings!.schemaVersion).toBe(2);
    expect(loaded.settings!.language).toBe(3);
    store.close();
  });

  it('falls back to in-memory state and refuses to claim success when storage is unavailable', async () => {
    Object.defineProperty(globalThis, 'indexedDB', {
      value: undefined,
      configurable: true,
    });
    const store = new Persistence();
    expect(await store.open()).toBe(false);
    const result = await store.save(defaultSettings());
    expect(result.verified).toBe(false);
    expect(result.error).toMatch(/unavailable/i);
    const loaded = await store.load();
    expect(loaded.settings).not.toBeNull();
  });

  it('round-trips an exported settings document and rejects malformed JSON', () => {
    const settings = { ...defaultSettings(), mode: 2, speed: 1.4, bootHistory: new Array<number>(21).fill(7) };
    const json = exportSettings(settings);
    const imported = importSettings(json);
    expect(imported.error).toBeNull();
    expect(imported.settings!.mode).toBe(2);
    expect(imported.settings!.bootHistory[0]).toBe(7);

    const broken = importSettings('{ not json');
    expect(broken.settings).toBeNull();
    expect(broken.error).toBeTruthy();
  });

  it('clamps hostile values instead of trusting the document', () => {
    const sanitized = sanitizeSettings({
      speed: 1e9,
      fogIntensity: -4,
      resolutionScale: 99,
      language: 42,
      clock: [999, -3, Number.NaN],
      bootHistory: [1e9, -1, 'x', null],
      mode: 9,
      assetSource: 'evil',
    });
    expect(sanitized.speed).toBe(2);
    expect(sanitized.fogIntensity).toBe(0);
    expect(sanitized.resolutionScale).toBe(2);
    expect(sanitized.language).toBe(7);
    expect(sanitized.clock[0]).toBe(23);
    expect(sanitized.clock[1]).toBe(0);
    expect(sanitized.clock[2]).toBe(0);
    expect(sanitized.bootHistory[0]).toBe(99);
    expect(sanitized.bootHistory[1]).toBe(0);
    expect(sanitized.bootHistory[2]).toBe(0);
    expect(sanitized.mode).toBe(3);
    expect(sanitized.assetSource).toBe('procedural');
  });

  it('clears every store on reset', async () => {
    const store = new Persistence();
    await store.open();
    await store.save(defaultSettings());
    expect(await store.clear()).toBe(true);
    const loaded = await store.load();
    expect(loaded.settings).toBeNull();
    store.close();
  });
});
