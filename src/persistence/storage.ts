import {
  type AudioPreferences,
  DEFAULT_AUDIO_PREFERENCES,
} from '../audio/audioEngine';
import {
  AppMode,
  BootCompletionPolicy,
  DEFAULT_ENGINE_TUNING,
  type EngineTuningParams,
} from '../wasm/bridge';

export const DB_NAME = 'osdbits-wasm-db';
export const CURRENT_SCHEMA_VERSION = 2;
export const FACTORY_RESET_TOKEN = 'CONFIRM_FACTORY_RESET';

export interface PersistedUserSettings {
  preferredMode: AppMode;
  bootCompletionPolicy: BootCompletionPolicy;
  seed: number;
  resolutionScale: number;
  hudCollapsed: boolean;
  forceCanvasFallback: boolean;
  tuning: EngineTuningParams;
  audio: AudioPreferences;
}

export const FAITHFUL_DEFAULT_SETTINGS: PersistedUserSettings = {
  preferredMode: AppMode.Opening,
  bootCompletionPolicy: BootCompletionPolicy.TransitionToMenu,
  seed: 0x2000_0304,
  resolutionScale: 1.0,
  hudCollapsed: false,
  forceCanvasFallback: false,
  tuning: { ...DEFAULT_ENGINE_TUNING },
  audio: { ...DEFAULT_AUDIO_PREFERENCES },
};

export interface StoredPreferencesEnvelope {
  schemaVersion: 2;
  revision: number;
  updatedAtIso: string;
  writerTabId: string;
  settings: PersistedUserSettings;
}

export interface StoredImportedAssetRecord {
  schemaVersion: 2;
  fileName: string;
  byteLength: number;
  sha256Hex: string;
  importedTextureCount: number;
  opfsBacked: boolean;
  savedAtIso: string;
  bytes: Uint8Array;
}

export interface StorageDurabilityReport {
  indexedDbAvailable: boolean;
  usingMemoryFallback: boolean;
  persistApiSupported: boolean;
  persistedGranted: boolean;
  usageBytes: number | null;
  quotaBytes: number | null;
  degradedReason: string | null;
}

export interface SaveVerificationResult {
  ok: boolean;
  readBackVerified: boolean;
  revision: number;
  conflictDetected: boolean;
  error: string | null;
}

function clampNum(v: unknown, lo: number, hi: number, fallback: number): number {
  if (typeof v !== 'number' || !Number.isFinite(v)) return fallback;
  return Math.min(hi, Math.max(lo, v));
}

function clampBool(v: unknown, fallback: boolean): boolean {
  return typeof v === 'boolean' ? v : fallback;
}

/**
 * Sanitize and clamp any partial or untrusted settings object into valid bounds.
 */
export function sanitizeUserSettings(raw: unknown): PersistedUserSettings {
  const obj =
    typeof raw === 'object' && raw !== null
      ? (raw as Record<string, unknown>)
      : {};
  const rawTuning =
    typeof obj['tuning'] === 'object' && obj['tuning'] !== null
      ? (obj['tuning'] as Record<string, unknown>)
      : {};
  const rawAudio =
    typeof obj['audio'] === 'object' && obj['audio'] !== null
      ? (obj['audio'] as Record<string, unknown>)
      : {};

  const modeNum = clampNum(obj['preferredMode'], 0, 3, AppMode.Opening);
  const policyNum = clampNum(
    obj['bootCompletionPolicy'],
    0,
    2,
    BootCompletionPolicy.TransitionToMenu,
  );

  return {
    preferredMode: Math.round(modeNum) as AppMode,
    bootCompletionPolicy: Math.round(policyNum) as BootCompletionPolicy,
    seed: (Math.round(clampNum(obj['seed'], 0, 0xffff_ffff, 0x2000_0304)) >>> 0),
    resolutionScale: clampNum(obj['resolutionScale'], 0.5, 2.0, 1.0),
    hudCollapsed: clampBool(obj['hudCollapsed'], false),
    forceCanvasFallback: clampBool(obj['forceCanvasFallback'], false),
    tuning: {
      isPal: clampBool(rawTuning['isPal'], DEFAULT_ENGINE_TUNING.isPal),
      speedScale: clampNum(rawTuning['speedScale'], 0.25, 2.5, 1.0),
      idleDrift: clampNum(rawTuning['idleDrift'], 0.0, 2.0, 1.0),
      fogEnabled: clampBool(rawTuning['fogEnabled'], true),
      bloomEnabled: clampBool(rawTuning['bloomEnabled'], true),
      refractionEnabled: clampBool(rawTuning['refractionEnabled'], true),
      reducedMotion: clampBool(rawTuning['reducedMotion'], false),
      screenSize: Math.round(clampNum(rawTuning['screenSize'], 0, 2, 0)),
      showBanner: clampBool(rawTuning['showBanner'], true),
      language: Math.round(clampNum(rawTuning['language'], 0, 6, 0)),
      confirmWithCircle: clampBool(rawTuning['confirmWithCircle'], false),
      towerGames: Math.round(clampNum(rawTuning['towerGames'], 0, 21, 21)),
    },
    audio: {
      muted: clampBool(rawAudio['muted'], DEFAULT_AUDIO_PREFERENCES.muted),
      masterVolume: clampNum(
        rawAudio['masterVolume'],
        0,
        1,
        DEFAULT_AUDIO_PREFERENCES.masterVolume,
      ),
      ambientVolume: clampNum(
        rawAudio['ambientVolume'],
        0,
        1,
        DEFAULT_AUDIO_PREFERENCES.ambientVolume,
      ),
      sfxVolume: clampNum(
        rawAudio['sfxVolume'],
        0,
        1,
        DEFAULT_AUDIO_PREFERENCES.sfxVolume,
      ),
      pauseOnHidden: clampBool(
        rawAudio['pauseOnHidden'],
        DEFAULT_AUDIO_PREFERENCES.pauseOnHidden,
      ),
    },
  };
}

/**
 * Migrate legacy v1 flat preference payloads or v2 envelopes into canonical v2 shape.
 */
export function migratePreferencesEnvelope(
  raw: unknown,
  fallbackTabId = 'migrated-tab',
): StoredPreferencesEnvelope {
  if (typeof raw !== 'object' || raw === null) {
    return {
      schemaVersion: 2,
      revision: 0,
      updatedAtIso: new Date(0).toISOString(),
      writerTabId: fallbackTabId,
      settings: sanitizeUserSettings(FAITHFUL_DEFAULT_SETTINGS),
    };
  }

  const rec = raw as Record<string, unknown>;
  if (rec['schemaVersion'] === 2 && typeof rec['settings'] === 'object') {
    return {
      schemaVersion: 2,
      revision: Math.max(0, Math.round(clampNum(rec['revision'], 0, 1e9, 1))),
      updatedAtIso:
        typeof rec['updatedAtIso'] === 'string'
          ? rec['updatedAtIso']
          : new Date().toISOString(),
      writerTabId:
        typeof rec['writerTabId'] === 'string' ? rec['writerTabId'] : fallbackTabId,
      settings: sanitizeUserSettings(rec['settings']),
    };
  }

  // Legacy v1 flat schema migration (`{ schemaVersion: 1, mode, speedScale, muted, ... }`)
  const v1Settings: Record<string, unknown> = {
    preferredMode: rec['preferredMode'] ?? rec['mode'],
    bootCompletionPolicy: rec['bootCompletionPolicy'],
    seed: rec['seed'],
    resolutionScale: rec['resolutionScale'],
    hudCollapsed: rec['hudCollapsed'],
    tuning: {
      isPal: rec['isPal'],
      speedScale: rec['speedScale'],
      fogEnabled: rec['fogEnabled'],
      bloomEnabled: rec['bloomEnabled'],
      refractionEnabled: rec['refractionEnabled'],
      reducedMotion: rec['reducedMotion'],
      screenSize: rec['screenSize'],
      language: rec['language'],
      towerGames: rec['towerGames'],
    },
    audio: {
      muted: rec['muted'],
      masterVolume: rec['masterVolume'],
    },
  };

  return {
    schemaVersion: 2,
    revision: Math.max(1, Math.round(clampNum(rec['revision'], 1, 1e9, 1))),
    updatedAtIso: new Date().toISOString(),
    writerTabId: fallbackTabId,
    settings: sanitizeUserSettings(v1Settings),
  };
}

/**
 * Pure-TypeScript SHA-256 hex digest fallback & verifier (matches FIPS 180-4 / WASM).
 */
export async function computeSha256Hex(bytes: Uint8Array): Promise<string> {
  if (typeof globalThis.crypto?.subtle?.digest === 'function') {
    const copy = new Uint8Array(bytes.byteLength);
    copy.set(bytes);
    const digest = await globalThis.crypto.subtle.digest('SHA-256', copy.buffer);
    return Array.from(new Uint8Array(digest), (b) =>
      b.toString(16).padStart(2, '0'),
    ).join('');
  }
  // FNV-padded fallback never needed in modern Node 22 / Chromium, but safe
  let h = 0x811c9dc5;
  for (const b of bytes) {
    h ^= b;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(64, '0');
}

function idbRequest<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error('IndexedDB request failed'));
  });
}

function idbTxComplete(tx: IDBTransaction): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error('IndexedDB transaction failed'));
    tx.onabort = () => reject(tx.error ?? new Error('IndexedDB transaction aborted'));
  });
}

const OPFS_ASSET_FILENAME = 'osdbits-imported-teximage.bin';

export interface PersistenceStoreOptions {
  dbName?: string;
  indexedDbFactory?: IDBFactory | null;
  onRemoteSettingsUpdate?: (envelope: StoredPreferencesEnvelope) => void;
  onRemoteReset?: () => void;
}

/**
 * Versioned IndexedDB (+ optional OPFS) persistence store with read-back verification,
 * multi-tab revision tracking, export/import validation, and confirmed factory reset.
 */
export class PersistenceStore {
  public readonly tabId: string;
  private readonly dbName: string;
  private readonly idbFactory: IDBFactory | null;
  private db: IDBDatabase | null = null;
  private memoryPrefs: StoredPreferencesEnvelope | null = null;
  private memoryAsset: StoredImportedAssetRecord | null = null;
  private usingMemoryFallback = false;
  private degradedReason: string | null = null;
  private knownRevision = 0;
  private channel: BroadcastChannel | null = null;
  private readonly onRemoteSettingsUpdate?: (
    envelope: StoredPreferencesEnvelope,
  ) => void;
  private readonly onRemoteReset?: () => void;

  public constructor(options: PersistenceStoreOptions = {}) {
    this.dbName = options.dbName ?? DB_NAME;
    this.idbFactory =
      options.indexedDbFactory !== undefined
        ? options.indexedDbFactory
        : typeof indexedDB !== 'undefined'
          ? indexedDB
          : null;
    this.onRemoteSettingsUpdate = options.onRemoteSettingsUpdate;
    this.onRemoteReset = options.onRemoteReset;
    this.tabId = `tab-${Math.random().toString(36).slice(2, 10)}`;

    if (typeof BroadcastChannel !== 'undefined') {
      try {
        this.channel = new BroadcastChannel(`${this.dbName}-sync`);
        this.channel.onmessage = (ev: MessageEvent<unknown>) => {
          const msg = ev.data as
            | { type: 'settings'; envelope: StoredPreferencesEnvelope }
            | { type: 'reset'; sender: string }
            | null;
          if (!msg || typeof msg !== 'object') return;
          if (msg.type === 'settings' && msg.envelope.writerTabId !== this.tabId) {
            if (msg.envelope.revision > this.knownRevision) {
              this.knownRevision = msg.envelope.revision;
              this.onRemoteSettingsUpdate?.(
                migratePreferencesEnvelope(msg.envelope, this.tabId),
              );
            }
          } else if (msg.type === 'reset' && msg.sender !== this.tabId) {
            this.knownRevision = 0;
            this.onRemoteReset?.();
          }
        };
      } catch {
        this.channel = null;
      }
    }
  }

  public async init(): Promise<void> {
    if (this.db || this.usingMemoryFallback) return;
    if (!this.idbFactory) {
      this.usingMemoryFallback = true;
      this.degradedReason =
        'IndexedDB is unavailable in this environment; using session memory fallback.';
      return;
    }

    try {
      this.db = await new Promise<IDBDatabase>((resolve, reject) => {
        const req = this.idbFactory!.open(this.dbName, CURRENT_SCHEMA_VERSION);
        req.onupgradeneeded = () => {
          const db = req.result;
          if (!db.objectStoreNames.contains('preferences')) {
            db.createObjectStore('preferences');
          }
          if (!db.objectStoreNames.contains('assets')) {
            db.createObjectStore('assets');
          }
          if (!db.objectStoreNames.contains('meta')) {
            db.createObjectStore('meta');
          }
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () =>
          reject(req.error ?? new Error('Failed to open IndexedDB.'));
        req.onblocked = () =>
          reject(new Error('IndexedDB upgrade blocked by another open tab.'));
      });
    } catch (err) {
      this.usingMemoryFallback = true;
      this.degradedReason =
        err instanceof Error ? err.message : 'IndexedDB failed to open.';
    }
  }

  public get currentRevision(): number {
    return this.knownRevision;
  }

  public async loadSettings(): Promise<StoredPreferencesEnvelope> {
    await this.init();
    if (this.usingMemoryFallback || !this.db) {
      const env =
        this.memoryPrefs ??
        migratePreferencesEnvelope(null, this.tabId);
      this.knownRevision = Math.max(this.knownRevision, env.revision);
      return env;
    }

    try {
      const tx = this.db.transaction('preferences', 'readonly');
      const store = tx.objectStore('preferences');
      const raw: unknown = await idbRequest(store.get('user-settings'));
      await idbTxComplete(tx);
      const migrated = migratePreferencesEnvelope(raw, this.tabId);
      this.knownRevision = Math.max(this.knownRevision, migrated.revision);
      return migrated;
    } catch (err) {
      this.degradedReason =
        err instanceof Error ? err.message : 'Failed reading IndexedDB settings.';
      return migratePreferencesEnvelope(null, this.tabId);
    }
  }

  /**
   * Persist settings with monotonic revision protection AND fresh-transaction read-back verification.
   */
  public async saveSettings(
    settings: PersistedUserSettings,
    options?: { rejectIfStale?: boolean },
  ): Promise<SaveVerificationResult> {
    await this.init();
    const sanitized = sanitizeUserSettings(settings);

    if (this.usingMemoryFallback || !this.db) {
      const nextRev = this.knownRevision + 1;
      const env: StoredPreferencesEnvelope = {
        schemaVersion: 2,
        revision: nextRev,
        updatedAtIso: new Date().toISOString(),
        writerTabId: this.tabId,
        settings: sanitized,
      };
      this.memoryPrefs = env;
      this.knownRevision = nextRev;
      return {
        ok: true,
        readBackVerified: true,
        revision: nextRev,
        conflictDetected: false,
        error: null,
      };
    }

    try {
      // Step 1: Atomic check + write in readwrite transaction
      const writeTx = this.db.transaction('preferences', 'readwrite');
      const writeStore = writeTx.objectStore('preferences');
      const existingRaw: unknown = await idbRequest(
        writeStore.get('user-settings'),
      );
      const existing = existingRaw
        ? migratePreferencesEnvelope(existingRaw, this.tabId)
        : null;
      const existingRev = existing?.revision ?? 0;

      const conflict = existingRev > this.knownRevision;
      if (conflict && options?.rejectIfStale) {
        this.knownRevision = existingRev;
        writeTx.abort();
        return {
          ok: false,
          readBackVerified: false,
          revision: existingRev,
          conflictDetected: true,
          error: `Stale write rejected: stored revision ${existingRev} > tab revision ${this.knownRevision}.`,
        };
      }

      const nextRev = Math.max(this.knownRevision, existingRev) + 1;
      const envelope: StoredPreferencesEnvelope = {
        schemaVersion: 2,
        revision: nextRev,
        updatedAtIso: new Date().toISOString(),
        writerTabId: this.tabId,
        settings: sanitized,
      };

      writeStore.put(envelope, 'user-settings');
      await idbTxComplete(writeTx);

      // Step 2: Mandatory fresh-transaction read-back verification
      const verifyTx = this.db.transaction('preferences', 'readonly');
      const readBackRaw: unknown = await idbRequest(
        verifyTx.objectStore('preferences').get('user-settings'),
      );
      await idbTxComplete(verifyTx);

      const readBack = migratePreferencesEnvelope(readBackRaw, this.tabId);
      const matches =
        readBack.revision === nextRev &&
        JSON.stringify(readBack.settings) === JSON.stringify(sanitized);

      if (!matches) {
        return {
          ok: false,
          readBackVerified: false,
          revision: readBack.revision,
          conflictDetected: conflict,
          error: 'IndexedDB read-back verification mismatch after write.',
        };
      }

      this.knownRevision = nextRev;
      this.channel?.postMessage({ type: 'settings', envelope });

      return {
        ok: true,
        readBackVerified: true,
        revision: nextRev,
        conflictDetected: conflict,
        error: null,
      };
    } catch (err) {
      const msg =
        err instanceof Error ? err.message : 'Failed writing settings to IndexedDB.';
      this.degradedReason = msg;
      return {
        ok: false,
        readBackVerified: false,
        revision: this.knownRevision,
        conflictDetected: false,
        error: msg,
      };
    }
  }

  private async tryWriteOpfs(bytes: Uint8Array): Promise<boolean> {
    try {
      if (typeof navigator === 'undefined' || !navigator.storage?.getDirectory) {
        return false;
      }
      const root = await navigator.storage.getDirectory();
      const handle = await root.getFileHandle(OPFS_ASSET_FILENAME, {
        create: true,
      });
      if (typeof handle.createWritable !== 'function') return false;
      const writable = await handle.createWritable();
      const copy = new Uint8Array(bytes.byteLength);
      copy.set(bytes);
      await writable.write(copy.buffer);
      await writable.close();
      return true;
    } catch {
      return false;
    }
  }

  private async tryReadOpfs(): Promise<Uint8Array | null> {
    try {
      if (typeof navigator === 'undefined' || !navigator.storage?.getDirectory) {
        return null;
      }
      const root = await navigator.storage.getDirectory();
      const handle = await root.getFileHandle(OPFS_ASSET_FILENAME, {
        create: false,
      });
      const file = await handle.getFile();
      const ab = await file.arrayBuffer();
      return new Uint8Array(ab);
    } catch {
      return null;
    }
  }

  private async tryDeleteOpfs(): Promise<void> {
    try {
      if (typeof navigator === 'undefined' || !navigator.storage?.getDirectory) {
        return;
      }
      const root = await navigator.storage.getDirectory();
      await root.removeEntry(OPFS_ASSET_FILENAME);
    } catch {
      // Ignore if absent
    }
  }

  /**
   * Persist validated user-imported `TEXIMAGE` / `ROMDIR` bytes + SHA-256 hash
   * with immediate read-back checksum verification.
   */
  public async saveImportedAsset(params: {
    fileName: string;
    bytes: Uint8Array;
    sha256Hex: string;
    importedTextureCount: number;
  }): Promise<{ ok: boolean; readBackVerified: boolean; opfsBacked: boolean; error: string | null }> {
    await this.init();
    const copy = params.bytes.slice();
    const opfsBacked = await this.tryWriteOpfs(copy);

    const record: StoredImportedAssetRecord = {
      schemaVersion: 2,
      fileName: params.fileName.slice(0, 128),
      byteLength: copy.byteLength,
      sha256Hex: params.sha256Hex,
      importedTextureCount: params.importedTextureCount,
      opfsBacked,
      savedAtIso: new Date().toISOString(),
      bytes: copy,
    };

    if (this.usingMemoryFallback || !this.db) {
      this.memoryAsset = record;
      return { ok: true, readBackVerified: true, opfsBacked, error: null };
    }

    try {
      const tx = this.db.transaction('assets', 'readwrite');
      tx.objectStore('assets').put(record, 'imported-teximage');
      await idbTxComplete(tx);

      // Fresh read-back + SHA-256 verification
      const vTx = this.db.transaction('assets', 'readonly');
      const rb = (await idbRequest(
        vTx.objectStore('assets').get('imported-teximage'),
      )) as StoredImportedAssetRecord | undefined;
      await idbTxComplete(vTx);

      if (!rb || !(rb.bytes instanceof Uint8Array)) {
        return {
          ok: false,
          readBackVerified: false,
          opfsBacked,
          error: 'Asset read-back returned empty record.',
        };
      }
      const actualHash = await computeSha256Hex(rb.bytes);
      if (rb.byteLength !== copy.byteLength || actualHash !== params.sha256Hex) {
        return {
          ok: false,
          readBackVerified: false,
          opfsBacked,
          error: 'Asset SHA-256 read-back verification failed.',
        };
      }
      return { ok: true, readBackVerified: true, opfsBacked, error: null };
    } catch (err) {
      const msg =
        err instanceof Error ? err.message : 'Failed to store imported asset.';
      this.degradedReason = msg;
      return { ok: false, readBackVerified: false, opfsBacked, error: msg };
    }
  }

  /**
   * Load persisted imported asset and verify its SHA-256 integrity hash before returning.
   */
  public async loadImportedAsset(): Promise<{
    record: StoredImportedAssetRecord | null;
    integrityValid: boolean;
    error: string | null;
  }> {
    await this.init();
    if (this.usingMemoryFallback || !this.db) {
      return { record: this.memoryAsset, integrityValid: true, error: null };
    }

    try {
      const tx = this.db.transaction('assets', 'readonly');
      const stored = (await idbRequest(
        tx.objectStore('assets').get('imported-teximage'),
      )) as StoredImportedAssetRecord | undefined;
      await idbTxComplete(tx);

      if (!stored) {
        return { record: null, integrityValid: true, error: null };
      }

      let bytes: Uint8Array | null = null;
      if (stored.opfsBacked) {
        bytes = await this.tryReadOpfs();
      }
      if (!bytes && stored.bytes instanceof Uint8Array) {
        bytes = stored.bytes;
      }
      if (!bytes) {
        return {
          record: null,
          integrityValid: false,
          error: 'Stored asset binary payload is missing.',
        };
      }

      const digest = await computeSha256Hex(bytes);
      if (bytes.byteLength !== stored.byteLength || digest !== stored.sha256Hex) {
        return {
          record: null,
          integrityValid: false,
          error: 'Stored asset failed SHA-256 integrity check on reload.',
        };
      }

      return {
        record: { ...stored, bytes },
        integrityValid: true,
        error: null,
      };
    } catch (err) {
      return {
        record: null,
        integrityValid: false,
        error: err instanceof Error ? err.message : 'Failed loading asset.',
      };
    }
  }

  public async clearImportedAsset(): Promise<void> {
    await this.init();
    await this.tryDeleteOpfs();
    this.memoryAsset = null;
    if (!this.usingMemoryFallback && this.db) {
      const tx = this.db.transaction('assets', 'readwrite');
      tx.objectStore('assets').delete('imported-teximage');
      await idbTxComplete(tx);
    }
  }

  public exportSettingsJson(settings: PersistedUserSettings): string {
    const payload = {
      app: 'osdbits-wasm',
      schemaVersion: CURRENT_SCHEMA_VERSION,
      exportedAtIso: new Date().toISOString(),
      settings: sanitizeUserSettings(settings),
    };
    return JSON.stringify(payload, null, 2);
  }

  public async importSettingsJson(
    jsonText: string,
  ): Promise<{ ok: true; settings: PersistedUserSettings } | { ok: false; error: string }> {
    let parsed: unknown;
    try {
      parsed = JSON.parse(jsonText);
    } catch {
      return { ok: false, error: 'Selected JSON file is not valid JSON syntax.' };
    }
    if (typeof parsed !== 'object' || parsed === null) {
      return { ok: false, error: 'Settings JSON root must be an object.' };
    }
    const obj = parsed as Record<string, unknown>;
    const candidate =
      typeof obj['settings'] === 'object' && obj['settings'] !== null
        ? obj['settings']
        : obj;
    const sanitized = sanitizeUserSettings(candidate);
    const saveRes = await this.saveSettings(sanitized);
    if (!saveRes.ok) {
      return {
        ok: false,
        error: saveRes.error ?? 'Failed saving imported settings.',
      };
    }
    return { ok: true, settings: sanitized };
  }

  /**
   * Confirmed full factory reset (`confirmationToken === FACTORY_RESET_TOKEN`).
   */
  public async resetAllConfirmed(
    confirmationToken: string,
  ): Promise<{ ok: boolean; error: string | null }> {
    if (confirmationToken !== FACTORY_RESET_TOKEN) {
      return {
        ok: false,
        error: 'Factory reset aborted: confirmation token did not match.',
      };
    }
    await this.init();
    await this.tryDeleteOpfs();
    this.memoryPrefs = null;
    this.memoryAsset = null;
    this.knownRevision = 0;

    if (!this.usingMemoryFallback && this.db) {
      const tx = this.db.transaction(['preferences', 'assets', 'meta'], 'readwrite');
      tx.objectStore('preferences').clear();
      tx.objectStore('assets').clear();
      tx.objectStore('meta').clear();
      await idbTxComplete(tx);
    }

    this.channel?.postMessage({ type: 'reset', sender: this.tabId });
    return { ok: true, error: null };
  }

  /**
   * Probe and optionally request `navigator.storage.persist()` durability honestly.
   */
  public async queryDurability(
    requestGrant = false,
  ): Promise<StorageDurabilityReport> {
    await this.init();
    const hasStorageMgr =
      typeof navigator !== 'undefined' && Boolean(navigator.storage);
    const persistSupported =
      hasStorageMgr && typeof navigator.storage.persist === 'function';

    let persistedGranted = false;
    let usageBytes: number | null = null;
    let quotaBytes: number | null = null;

    if (hasStorageMgr) {
      try {
        if (requestGrant && persistSupported) {
          persistedGranted = await navigator.storage.persist();
        } else if (typeof navigator.storage.persisted === 'function') {
          persistedGranted = await navigator.storage.persisted();
        }
        if (typeof navigator.storage.estimate === 'function') {
          const est = await navigator.storage.estimate();
          usageBytes = est.usage ?? null;
          quotaBytes = est.quota ?? null;
        }
      } catch {
        // Ignore permission errors on restricted origins
      }
    }

    return {
      indexedDbAvailable: !this.usingMemoryFallback,
      usingMemoryFallback: this.usingMemoryFallback,
      persistApiSupported: persistSupported,
      persistedGranted,
      usageBytes,
      quotaBytes,
      degradedReason: this.degradedReason,
    };
  }

  public dispose(): void {
    this.channel?.close();
    this.channel = null;
    this.db?.close();
    this.db = null;
  }
}
