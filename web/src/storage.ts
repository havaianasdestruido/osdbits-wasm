/**
 * Persistence: one settings record plus a tiny session log, behind a narrow
 * interface so the logic is unit-testable without a browser.
 *
 * IndexedDB is used in the app (OPFS adds nothing for a record this small);
 * localStorage is the fallback when IndexedDB is unavailable (private mode,
 * storage disabled); an in-memory store is used in tests and as the last
 * resort, in which case the UI says persistence is unavailable.
 */

import { normalizeSettings, SETTINGS_VERSION, type Settings, DEFAULT_SETTINGS } from "./settings.js";

export interface KeyValueStore {
  readonly kind: "indexeddb" | "localstorage" | "memory";
  /** Persistence actually survives a reload? */
  readonly durable: boolean;
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
  delete(key: string): Promise<void>;
}

export const SETTINGS_KEY = "osdbits.settings.v1";
export const SESSION_KEY = "osdbits.session.v1";
export const DB_NAME = "osdbits-wasm";
export const DB_STORE = "kv";

/** In-memory store: tests, and the degraded mode when nothing else works. */
export class MemoryStore implements KeyValueStore {
  readonly kind = "memory" as const;
  readonly durable = false;
  private readonly map = new Map<string, string>();

  async get(key: string): Promise<string | null> {
    return this.map.get(key) ?? null;
  }
  async set(key: string, value: string): Promise<void> {
    this.map.set(key, value);
  }
  async delete(key: string): Promise<void> {
    this.map.delete(key);
  }
}

/** `localStorage` wrapper (synchronous API behind the async interface). */
export class LocalStorageStore implements KeyValueStore {
  readonly kind = "localstorage" as const;
  readonly durable = true;
  constructor(private readonly storage: Storage) {}

  async get(key: string): Promise<string | null> {
    return this.storage.getItem(key);
  }
  async set(key: string, value: string): Promise<void> {
    this.storage.setItem(key, value);
  }
  async delete(key: string): Promise<void> {
    this.storage.removeItem(key);
  }
}

function requestToPromise<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("IndexedDB request failed"));
  });
}

/** Minimal IndexedDB key/value store (one object store, one version). */
export class IndexedDbStore implements KeyValueStore {
  readonly kind = "indexeddb" as const;
  readonly durable = true;
  private constructor(private readonly db: IDBDatabase) {}

  static async open(name = DB_NAME, store = DB_STORE): Promise<IndexedDbStore> {
    const indexedDb: IDBFactory | undefined = globalThis.indexedDB;
    if (!indexedDb) {
      throw new Error("IndexedDB is not available");
    }
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDb.open(name, 1);
      request.onupgradeneeded = () => {
        const database = request.result;
        if (!database.objectStoreNames.contains(store)) {
          database.createObjectStore(store);
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error ?? new Error("IndexedDB open failed"));
      request.onblocked = () => reject(new Error("IndexedDB open blocked by another tab"));
    });
    return new IndexedDbStore(db);
  }

  private tx(mode: IDBTransactionMode): IDBObjectStore {
    return this.db.transaction(DB_STORE, mode).objectStore(DB_STORE);
  }

  async get(key: string): Promise<string | null> {
    const value = await requestToPromise<unknown>(this.tx("readonly").get(key));
    return typeof value === "string" ? value : null;
  }

  async set(key: string, value: string): Promise<void> {
    await requestToPromise(this.tx("readwrite").put(value, key));
  }

  async delete(key: string): Promise<void> {
    await requestToPromise(this.tx("readwrite").delete(key));
  }

  close(): void {
    this.db.close();
  }
}

/**
 * Open the best available store. Never throws: falls back to memory and
 * reports what happened so the UI can warn about lost persistence.
 */
export async function openStore(): Promise<KeyValueStore> {
  try {
    return await IndexedDbStore.open();
  } catch {
    /* fall through */
  }
  try {
    if (typeof localStorage !== "undefined") {
      // Probe: some browsers expose localStorage but throw on write.
      localStorage.setItem("osdbits.probe", "1");
      localStorage.removeItem("osdbits.probe");
      return new LocalStorageStore(localStorage);
    }
  } catch {
    /* fall through */
  }
  return new MemoryStore();
}

export interface SessionLog {
  launches: number;
  lastScene: string;
  totalSeconds: number;
  lastSeen: number;
}

export const EMPTY_SESSION: SessionLog = {
  launches: 0,
  lastScene: "boot",
  totalSeconds: 0,
  lastSeen: 0,
};

export function normalizeSession(raw: unknown): SessionLog {
  if (typeof raw !== "object" || raw === null) return { ...EMPTY_SESSION };
  const r = raw as Record<string, unknown>;
  const n = (v: unknown, f: number) => (typeof v === "number" && Number.isFinite(v) ? v : f);
  return {
    launches: Math.max(0, Math.round(n(r.launches, 0))),
    lastScene: typeof r.lastScene === "string" ? r.lastScene.slice(0, 32) : "boot",
    totalSeconds: Math.max(0, n(r.totalSeconds, 0)),
    lastSeen: Math.max(0, n(r.lastSeen, 0)),
  };
}

/** Settings + session repository used by the UI. */
export class Repository {
  constructor(private readonly store: KeyValueStore) {}

  get durable(): boolean {
    return this.store.durable;
  }
  get backend(): KeyValueStore["kind"] {
    return this.store.kind;
  }

  async loadSettings(): Promise<{ settings: Settings; migrated: boolean }> {
    const raw = await this.store.get(SETTINGS_KEY);
    if (raw === null) {
      return { settings: { ...DEFAULT_SETTINGS }, migrated: false };
    }
    try {
      const parsed: unknown = JSON.parse(raw);
      const before = (parsed as { version?: unknown } | null)?.version;
      const settings = normalizeSettings(parsed);
      const migrated = before !== SETTINGS_VERSION;
      if (migrated) {
        await this.saveSettings(settings);
      }
      return { settings, migrated };
    } catch {
      // Corrupt record: keep a copy for debugging, then reset.
      await this.store.set(`${SETTINGS_KEY}.corrupt`, raw.slice(0, 4096));
      return { settings: { ...DEFAULT_SETTINGS }, migrated: true };
    }
  }

  async saveSettings(settings: Settings): Promise<void> {
    await this.store.set(SETTINGS_KEY, JSON.stringify({ ...settings, version: SETTINGS_VERSION }));
  }

  async loadSession(): Promise<SessionLog> {
    const raw = await this.store.get(SESSION_KEY);
    if (raw === null) return { ...EMPTY_SESSION };
    try {
      return normalizeSession(JSON.parse(raw));
    } catch {
      return { ...EMPTY_SESSION };
    }
  }

  async saveSession(session: SessionLog): Promise<void> {
    await this.store.set(SESSION_KEY, JSON.stringify(session));
  }

  async reset(): Promise<void> {
    await this.store.delete(SETTINGS_KEY);
    await this.store.delete(SESSION_KEY);
  }
}
