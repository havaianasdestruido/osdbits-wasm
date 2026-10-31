import { IDBFactory as FakeIDBFactory } from 'fake-indexeddb';
import { describe, expect, it } from 'vitest';

import {
  computeSha256Hex,
  FACTORY_RESET_TOKEN,
  FAITHFUL_DEFAULT_SETTINGS,
  migratePreferencesEnvelope,
  PersistenceStore,
} from '../../src/persistence/storage';
import { AppMode, BootCompletionPolicy } from '../../src/wasm/bridge';

/**
 * Minimal deterministic in-RAM IDBFactory mock to unit-test read-back verification,
 * multi-tab revision conflict detection, and SHA-256 tamper rejection in Node.
 */
function createMockIdbFactory(options?: {
  tamperAssetAfterWrite?: boolean;
  mismatchReadBack?: boolean;
}): IDBFactory {
  const stores = new Map<string, Map<string, unknown>>([
    ['preferences', new Map()],
    ['assets', new Map()],
    ['meta', new Map()],
  ]);

  const makeObjectStore = (name: string): IDBObjectStore => {
    const map = stores.get(name)!;
    return {
      get(key: string) {
        const req = {} as IDBRequest<unknown>;
        queueMicrotask(() => {
          let val = map.get(key);
          if (options?.mismatchReadBack && name === 'preferences' && val) {
            val = { ...(val as object), revision: 99999 };
          }
          (req as { result: unknown }).result = val;
          req.onsuccess?.(new Event('success'));
        });
        return req;
      },
      put(val: unknown, key: string) {
        const req = {} as IDBRequest<IDBValidKey>;
        if (
          options?.tamperAssetAfterWrite &&
          name === 'assets' &&
          typeof val === 'object' &&
          val !== null
        ) {
          const rec = val as { bytes?: Uint8Array };
          if (rec.bytes && rec.bytes.length > 0) {
            const corrupted = rec.bytes.slice();
            corrupted[0] = (corrupted[0]! ^ 0xff) & 0xff;
            map.set(key, { ...rec, bytes: corrupted });
          } else {
            map.set(key, val);
          }
        } else {
          map.set(key, structuredClone(val));
        }
        queueMicrotask(() => {
          (req as { result: IDBValidKey }).result = key;
          req.onsuccess?.(new Event('success'));
        });
        return req;
      },
      delete(key: string) {
        map.delete(key);
        const req = {} as IDBRequest<undefined>;
        queueMicrotask(() => req.onsuccess?.(new Event('success')));
        return req;
      },
      clear() {
        map.clear();
        const req = {} as IDBRequest<undefined>;
        queueMicrotask(() => req.onsuccess?.(new Event('success')));
        return req;
      },
    } as unknown as IDBObjectStore;
  };

  const db = {
    objectStoreNames: {
      contains: (n: string) => stores.has(n),
    },
    createObjectStore: (n: string) => {
      stores.set(n, new Map());
      return makeObjectStore(n);
    },
    transaction: () => {
      const tx = {
        objectStore: (n: string) => makeObjectStore(n),
        abort() {
          queueMicrotask(() => tx.onabort?.(new Event('abort')));
        },
        oncomplete: null as ((ev: Event) => void) | null,
        onerror: null as ((ev: Event) => void) | null,
        onabort: null as ((ev: Event) => void) | null,
      };
      setTimeout(() => tx.oncomplete?.(new Event('complete')), 2);
      return tx as unknown as IDBTransaction;
    },
    close: () => undefined,
  } as unknown as IDBDatabase;

  return {
    open: () => {
      const req = {} as IDBOpenDBRequest;
      queueMicrotask(() => {
        (req as { result: IDBDatabase }).result = db;
        req.onsuccess?.(new Event('success'));
      });
      return req;
    },
  } as unknown as IDBFactory;
}

describe('Phase 6 PersistenceStore unit suite (P06-001..P06-003)', () => {
  it('migrates legacy v1 flat schemas to v2 and clamps out-of-range values', () => {
    const v1 = {
      schemaVersion: 1,
      mode: 3,
      bootCompletionPolicy: 1,
      seed: 4242,
      speedScale: 999,
      language: 99,
      towerGames: -10,
      muted: true,
    };
    const migrated = migratePreferencesEnvelope(v1, 'test-tab');
    expect(migrated.schemaVersion).toBe(2);
    expect(migrated.settings.preferredMode).toBe(AppMode.Menu);
    expect(migrated.settings.bootCompletionPolicy).toBe(
      BootCompletionPolicy.TransitionToIdle,
    );
    expect(migrated.settings.seed).toBe(4242);
    expect(migrated.settings.tuning.speedScale).toBe(2.5);
    expect(migrated.settings.tuning.language).toBe(6);
    expect(migrated.settings.tuning.towerGames).toBe(0);
    expect(migrated.settings.audio.muted).toBe(true);
  });

  it('verifies read-back after IndexedDB writes and guards against stale multi-tab overwrites', async () => {
    const sharedIdb = createMockIdbFactory();
    const tabA = new PersistenceStore({ indexedDbFactory: sharedIdb, dbName: 'test-sync' });
    const tabB = new PersistenceStore({ indexedDbFactory: sharedIdb, dbName: 'test-sync' });

    await tabA.loadSettings();
    await tabB.loadSettings();

    // Tab A writes revision 1
    const writeA = await tabA.saveSettings({
      ...FAITHFUL_DEFAULT_SETTINGS,
      seed: 1111,
    });
    expect(writeA.ok).toBe(true);
    expect(writeA.readBackVerified).toBe(true);
    expect(writeA.revision).toBe(1);

    // Tab B still has knownRevision = 0 -> rejectIfStale detects conflict!
    const staleB = await tabB.saveSettings(
      { ...FAITHFUL_DEFAULT_SETTINGS, seed: 2222 },
      { rejectIfStale: true },
    );
    expect(staleB.ok).toBe(false);
    expect(staleB.conflictDetected).toBe(true);

    // Normal merge increments monotonically to revision 2
    const mergedB = await tabB.saveSettings({
      ...FAITHFUL_DEFAULT_SETTINGS,
      seed: 3333,
    });
    expect(mergedB.ok).toBe(true);
    expect(mergedB.revision).toBe(2);

    tabA.dispose();
    tabB.dispose();
  });

  it('detects read-back mismatches and corrupted asset SHA-256 payloads', async () => {
    const mismatchStore = new PersistenceStore({
      indexedDbFactory: createMockIdbFactory({ mismatchReadBack: true }),
    });
    const badSave = await mismatchStore.saveSettings(FAITHFUL_DEFAULT_SETTINGS);
    expect(badSave.ok).toBe(false);
    expect(badSave.readBackVerified).toBe(false);
    mismatchStore.dispose();

    const tamperStore = new PersistenceStore({
      indexedDbFactory: createMockIdbFactory({ tamperAssetAfterWrite: true }),
    });
    const payload = new Uint8Array([10, 20, 30, 40, 50, 60]);
    const sha = await computeSha256Hex(payload);
    const assetSave = await tamperStore.saveImportedAsset({
      fileName: 'TEXIMAGE',
      bytes: payload,
      sha256Hex: sha,
      importedTextureCount: 3,
    });
    expect(assetSave.ok).toBe(false);
    expect(assetSave.readBackVerified).toBe(false);
    tamperStore.dispose();
  });

  it('supports memory fallback, JSON export/import validation, and confirmed factory reset', async () => {
    const store = new PersistenceStore({ indexedDbFactory: null });
    const durability = await store.queryDurability();
    expect(durability.usingMemoryFallback).toBe(true);

    const invalidImport = await store.importSettingsJson('{not-json');
    expect(invalidImport.ok).toBe(false);

    const exported = store.exportSettingsJson({
      ...FAITHFUL_DEFAULT_SETTINGS,
      preferredMode: AppMode.Illegal,
      seed: 0xabcd_1234,
    });
    const validImport = await store.importSettingsJson(exported);
    expect(validImport.ok).toBe(true);

    const loaded = await store.loadSettings();
    expect(loaded.settings.preferredMode).toBe(AppMode.Illegal);
    expect(loaded.settings.seed).toBe(0xabcd_1234);

    // Unconfirmed reset is rejected
    const badReset = await store.resetAllConfirmed('WRONG_TOKEN');
    expect(badReset.ok).toBe(false);
    expect((await store.loadSettings()).settings.seed).toBe(0xabcd_1234);

    // Confirmed reset wipes back to defaults
    const goodReset = await store.resetAllConfirmed(FACTORY_RESET_TOKEN);
    expect(goodReset.ok).toBe(true);
    expect((await store.loadSettings()).settings.seed).toBe(
      FAITHFUL_DEFAULT_SETTINGS.seed,
    );
    store.dispose();
  });

  it('persists and verifies binary assets and schema upgrades against spec-compliant fake-indexeddb', async () => {
    const idb = new FakeIDBFactory();
    const store1 = new PersistenceStore({ indexedDbFactory: idb, dbName: 'fake-idb-db' });
    const saveRes = await store1.saveSettings({
      ...FAITHFUL_DEFAULT_SETTINGS,
      preferredMode: AppMode.Idle,
      seed: 80808,
    });
    expect(saveRes.ok).toBe(true);
    expect(saveRes.readBackVerified).toBe(true);

    const bytes = new Uint8Array([0x52, 0x45, 0x53, 0x45, 0x54, 1, 2, 3, 4]);
    const sha = await computeSha256Hex(bytes);
    const assetSave = await store1.saveImportedAsset({
      fileName: 'CLEAN_TEST.BIN',
      bytes,
      sha256Hex: sha,
      importedTextureCount: 3,
    });
    expect(assetSave.ok).toBe(true);
    expect(assetSave.readBackVerified).toBe(true);
    store1.dispose();

    // Reopen fresh PersistenceStore instance against same FakeIDBFactory
    const store2 = new PersistenceStore({ indexedDbFactory: idb, dbName: 'fake-idb-db' });
    const restored = await store2.loadSettings();
    expect(restored.settings.preferredMode).toBe(AppMode.Idle);
    expect(restored.settings.seed).toBe(80808);

    const restoredAsset = await store2.loadImportedAsset();
    expect(restoredAsset.integrityValid).toBe(true);
    expect(restoredAsset.record?.sha256Hex).toBe(sha);
    expect(restoredAsset.record?.byteLength).toBe(bytes.byteLength);
    store2.dispose();
  });
});
