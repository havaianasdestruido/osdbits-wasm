/**
 * Versioned persistence: IndexedDB for structured settings/session data, OPFS (with an
 * IndexedDB fallback) for user-imported binary assets.
 *
 * Contract, enforced by `save()`: **every successful write is read back and verified**
 * (schema version, monotonic revision, and a checksum over the payload) before `save()`
 * reports success. A write that cannot be verified is reported as a failure even if the
 * underlying promise resolved.
 */

export const DB_NAME = 'osdbits-wasm';
export const DB_VERSION = 1;
export const STORE_SETTINGS = 'settings';
export const STORE_ASSETS = 'assets';
export const STORE_META = 'meta';
export const SCHEMA_VERSION = 2;

export interface PersistedSettings {
  schemaVersion: number;
  revision: number;
  updatedAt: number;
  checksum: string;
  mode: number;
  speed: number;
  reducedMotion: boolean;
  fogIntensity: number;
  refractionScale: number;
  resolutionScale: number;
  audioMaster: number;
  audioBgm: number;
  audioSe: number;
  muted: boolean;
  language: number;
  screenSize: number;
  digitalOut: number;
  componentOut: number;
  regionJapan: boolean;
  clock: [number, number, number];
  bootHistory: number[];
  assetSource: 'procedural' | 'imported';
}

export interface SaveResult {
  verified: boolean;
  error: string | null;
  revision: number;
}

export interface LoadResult {
  settings: PersistedSettings | null;
  migratedFrom: number | null;
  error: string | null;
  recoveredFromCorruption: boolean;
}

export function defaultSettings(): PersistedSettings {
  return {
    schemaVersion: SCHEMA_VERSION,
    revision: 0,
    updatedAt: 0,
    checksum: '',
    mode: 0,
    speed: 1,
    reducedMotion: false,
    fogIntensity: 1,
    refractionScale: 1,
    resolutionScale: 1,
    audioMaster: 0.8,
    audioBgm: 0.7,
    audioSe: 0.8,
    muted: false,
    language: 1,
    screenSize: 0,
    digitalOut: 0,
    componentOut: 0,
    regionJapan: false,
    clock: [12, 34, 56],
    bootHistory: [9, 6, 4, 3, 3, 2, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    assetSource: 'procedural',
  };
}

/** FNV-1a over the settings payload with the checksum field excluded. */
export function checksumOf(settings: PersistedSettings): string {
  const clone: Record<string, unknown> = { ...settings };
  delete clone.checksum;
  const json = JSON.stringify(clone, Object.keys(clone).sort());
  let hash = 0x811c9dc5;
  for (let i = 0; i < json.length; i += 1) {
    hash ^= json.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}

/** Validates and clamps a decoded record into a usable `PersistedSettings`. */
export function sanitizeSettings(input: unknown): PersistedSettings {
  const base = defaultSettings();
  if (typeof input !== 'object' || input === null) return base;
  const raw = input as Record<string, unknown>;
  const num = (key: keyof PersistedSettings, min: number, max: number, fallback: number): number => {
    const value = raw[key];
    if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
    return Math.min(max, Math.max(min, value));
  };
  const bool = (key: keyof PersistedSettings, fallback: boolean): boolean =>
    typeof raw[key] === 'boolean' ? (raw[key] as boolean) : fallback;

  const clockRaw = raw.clock;
  const clock: [number, number, number] = Array.isArray(clockRaw) && clockRaw.length === 3
    ? [
        Math.min(23, Math.max(0, Number(clockRaw[0]) || 0)),
        Math.min(59, Math.max(0, Number(clockRaw[1]) || 0)),
        Math.min(59, Math.max(0, Number(clockRaw[2]) || 0)),
      ]
    : base.clock;

  const historyRaw = raw.bootHistory;
  const bootHistory = new Array<number>(21).fill(0);
  if (Array.isArray(historyRaw)) {
    for (let i = 0; i < 21; i += 1) {
      const v = Number(historyRaw[i] ?? 0);
      bootHistory[i] = Number.isFinite(v) ? Math.min(99, Math.max(0, Math.floor(v))) : 0;
    }
  }

  return {
    schemaVersion: SCHEMA_VERSION,
    revision: Math.max(0, Math.floor(num('revision', 0, Number.MAX_SAFE_INTEGER, 0))),
    updatedAt: Math.max(0, Math.floor(num('updatedAt', 0, Number.MAX_SAFE_INTEGER, 0))),
    checksum: typeof raw.checksum === 'string' ? raw.checksum : '',
    mode: Math.floor(num('mode', 0, 3, 0)),
    speed: num('speed', 0.25, 2, 1),
    reducedMotion: bool('reducedMotion', false),
    fogIntensity: num('fogIntensity', 0, 2, 1),
    refractionScale: num('refractionScale', 0, 2, 1),
    resolutionScale: num('resolutionScale', 0.5, 2, 1),
    audioMaster: num('audioMaster', 0, 1, 0.8),
    audioBgm: num('audioBgm', 0, 1, 0.7),
    audioSe: num('audioSe', 0, 1, 0.8),
    muted: bool('muted', false),
    language: Math.floor(num('language', 1, 7, 1)),
    screenSize: Math.floor(num('screenSize', 0, 2, 0)),
    digitalOut: Math.floor(num('digitalOut', 0, 1, 0)),
    componentOut: Math.floor(num('componentOut', 0, 1, 0)),
    regionJapan: bool('regionJapan', false),
    clock,
    bootHistory,
    assetSource: raw.assetSource === 'imported' ? 'imported' : 'procedural',
  };
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_SETTINGS)) db.createObjectStore(STORE_SETTINGS);
      if (!db.objectStoreNames.contains(STORE_ASSETS)) db.createObjectStore(STORE_ASSETS);
      if (!db.objectStoreNames.contains(STORE_META)) db.createObjectStore(STORE_META);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('indexedDB.open failed'));
    request.onblocked = () => reject(new Error('indexedDB.open blocked by another tab'));
  });
}

async function idbPut(db: IDBDatabase, store: string, key: string, value: unknown): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(store, 'readwrite');
    tx.objectStore(store).put(value, key);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error(`write to ${store} failed`));
    tx.onabort = () => reject(tx.error ?? new Error(`write to ${store} aborted`));
  });
}

async function idbGet<T>(db: IDBDatabase, store: string, key: string): Promise<T | undefined> {
  return new Promise<T | undefined>((resolve, reject) => {
    const tx = db.transaction(store, 'readonly');
    const request = tx.objectStore(store).get(key);
    request.onsuccess = () => resolve(request.result as T | undefined);
    request.onerror = () => reject(request.error ?? new Error(`read from ${store} failed`));
  });
}

export class Persistence {
  private db: IDBDatabase | null = null;
  private readonly inMemory = new Map<string, unknown>();
  private lastRevision = 0;
  available = true;
  lastError: string | null = null;

  async open(): Promise<boolean> {
    try {
      this.db = await openDb();
      this.available = true;
      return true;
    } catch (error) {
      this.available = false;
      this.lastError = error instanceof Error ? error.message : String(error);
      return false;
    }
  }

  /** Persists settings and verifies them by reading the record back. */
  async save(settings: PersistedSettings): Promise<SaveResult> {
    const next: PersistedSettings = {
      ...sanitizeSettings(settings),
      revision: Math.max(settings.revision, this.lastRevision) + 1,
      updatedAt: Date.now(),
    };
    next.checksum = checksumOf(next);

    if (!this.db) {
      // Degraded mode: keep it in memory so the session keeps working, but never claim success.
      this.inMemory.set('settings', next);
      this.lastRevision = next.revision;
      return {
        verified: false,
        error: 'Persistent storage is unavailable; settings were kept for this session only',
        revision: next.revision,
      };
    }

    try {
      await idbPut(this.db, STORE_SETTINGS, 'settings', next);
    } catch (error) {
      return {
        verified: false,
        error: error instanceof Error ? error.message : String(error),
        revision: next.revision,
      };
    }

    // Mandatory read-back verification.
    try {
      const readBack = await idbGet<PersistedSettings>(this.db, STORE_SETTINGS, 'settings');
      if (!readBack) {
        return { verified: false, error: 'Read-back returned no record', revision: next.revision };
      }
      const expected = checksumOf(next);
      const actual = checksumOf(sanitizeSettings(readBack));
      if (readBack.revision !== next.revision) {
        return {
          verified: false,
          error: `Revision mismatch after write (stored ${readBack.revision}, wrote ${next.revision})`,
          revision: next.revision,
        };
      }
      if (actual !== expected) {
        return {
          verified: false,
          error: 'Checksum mismatch after write; the record may be corrupt',
          revision: next.revision,
        };
      }
      this.lastRevision = next.revision;
      return { verified: true, error: null, revision: next.revision };
    } catch (error) {
      return {
        verified: false,
        error: error instanceof Error ? error.message : String(error),
        revision: next.revision,
      };
    }
  }

  async load(): Promise<LoadResult> {
    if (!this.db) {
      const memory = this.inMemory.get('settings') as PersistedSettings | undefined;
      return {
        settings: memory ?? null,
        migratedFrom: null,
        error: this.available ? null : this.lastError,
        recoveredFromCorruption: false,
      };
    }
    let raw: unknown;
    try {
      raw = await idbGet<unknown>(this.db, STORE_SETTINGS, 'settings');
    } catch (error) {
      return {
        settings: null,
        migratedFrom: null,
        error: error instanceof Error ? error.message : String(error),
        recoveredFromCorruption: false,
      };
    }
    if (raw === undefined || raw === null) {
      return { settings: null, migratedFrom: null, error: null, recoveredFromCorruption: false };
    }
    try {
      const decoded = raw as Record<string, unknown>;
      const storedVersion = typeof decoded.schemaVersion === 'number' ? decoded.schemaVersion : 1;
      const storedChecksum = typeof decoded.checksum === 'string' ? decoded.checksum : '';
      const sanitized = sanitizeSettings(decoded);
      const expected = checksumOf(sanitized);
      const corrupt = storedChecksum !== '' && storedChecksum !== expected;
      if (corrupt) {
        // Quarantine the bad record rather than silently overwriting it.
        await idbPut(this.db, STORE_META, `corrupt-${Date.now()}`, decoded);
        return {
          settings: sanitizeSettings({ ...defaultSettings(), revision: sanitized.revision }),
          migratedFrom: null,
          error: 'Stored settings failed checksum validation; defaults restored',
          recoveredFromCorruption: true,
        };
      }
      this.lastRevision = sanitized.revision;
      return {
        settings: sanitized,
        migratedFrom: storedVersion < SCHEMA_VERSION ? storedVersion : null,
        error: null,
        recoveredFromCorruption: false,
      };
    } catch (error) {
      return {
        settings: defaultSettings(),
        migratedFrom: null,
        error: error instanceof Error ? error.message : String(error),
        recoveredFromCorruption: true,
      };
    }
  }

  async saveAsset(name: string, data: Uint8Array): Promise<SaveResult> {
    if (!this.db) return { verified: false, error: 'Storage unavailable', revision: 0 };
    try {
      // OPFS where supported; the IndexedDB copy is always the verified one.
      try {
        const root = await navigator.storage.getDirectory();
        const handle = await root.getFileHandle(name, { create: true });
        const writable = await handle.createWritable();
        await writable.write(data as unknown as BufferSource);
        await writable.close();
      } catch {
        /* OPFS unavailable: the IndexedDB copy below still persists the asset */
      }
      const record = { name, size: data.length, checksum: checksumBytes(data), data };
      await idbPut(this.db, STORE_ASSETS, name, record);
      const readBack = await idbGet<{ size: number; checksum: string; data: Uint8Array }>(
        this.db,
        STORE_ASSETS,
        name,
      );
      if (!readBack || readBack.size !== data.length || readBack.checksum !== record.checksum) {
        return { verified: false, error: 'Asset read-back mismatch', revision: 0 };
      }
      return { verified: true, error: null, revision: 0 };
    } catch (error) {
      return {
        verified: false,
        error: error instanceof Error ? error.message : String(error),
        revision: 0,
      };
    }
  }

  async loadAsset(name: string): Promise<Uint8Array | null> {
    if (!this.db) return null;
    const record = await idbGet<{ data: Uint8Array; checksum: string }>(this.db, STORE_ASSETS, name);
    if (!record) return null;
    const actual = checksumBytes(record.data);
    if (actual !== record.checksum) return null;
    return record.data;
  }

  async clear(): Promise<boolean> {
    try {
      if (this.db) {
        await new Promise<void>((resolve, reject) => {
          const tx = this.db!.transaction([STORE_SETTINGS, STORE_ASSETS, STORE_META], 'readwrite');
          tx.objectStore(STORE_SETTINGS).clear();
          tx.objectStore(STORE_ASSETS).clear();
          tx.objectStore(STORE_META).clear();
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error ?? new Error('clear failed'));
        });
      }
      this.inMemory.clear();
      this.lastRevision = 0;
      return true;
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
      return false;
    }
  }

  /** Requests persistent storage; the browser may refuse, which is handled as a normal outcome. */
  async requestPersistent(): Promise<boolean> {
    try {
      if (!navigator.storage?.persist) return false;
      return await navigator.storage.persist();
    } catch {
      return false;
    }
  }

  async estimate(): Promise<{ usage: number; quota: number } | null> {
    try {
      if (!navigator.storage?.estimate) return null;
      const { usage = 0, quota = 0 } = await navigator.storage.estimate();
      return { usage, quota };
    } catch {
      return null;
    }
  }

  close(): void {
    this.db?.close();
    this.db = null;
  }
}

export function checksumBytes(data: Uint8Array): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < data.length; i += 1) {
    hash ^= data[i]!;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}

export function exportSettings(settings: PersistedSettings): string {
  return JSON.stringify({ ...settings, checksum: checksumOf(settings) }, null, 2);
}

export function importSettings(json: string): { settings: PersistedSettings | null; error: string | null } {
  try {
    const parsed: unknown = JSON.parse(json);
    const sanitized = sanitizeSettings(parsed);
    return { settings: sanitized, error: null };
  } catch (error) {
    return { settings: null, error: error instanceof Error ? error.message : String(error) };
  }
}
