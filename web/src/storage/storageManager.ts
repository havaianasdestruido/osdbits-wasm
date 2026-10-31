/**
 * Versioned Persistent Storage Engine (`P07-001`) backed by `IndexedDB` + optional `OPFS`
 * with `v1 -> v2` schema migrations, monotonic revision stale-write protection,
 * `BroadcastChannel` multi-tab sync, mandatory post-write read-back verification,
 * SHA-256 asset integrity verification, JSON export/import, and confirmed reset.
 */

import { computeFnv1aHex, computeSha256Hex } from "../assets/proceduralTextures";
import type { AspectPresentationMode } from "../renderer/webgpuRenderer";
import type { OsdLanguageCode, TowerPresetSpec, VisualModeName } from "../wasm/bridge";

export const STORAGE_SCHEMA_VERSION = 2;
export const RESET_CONFIRMATION_TOKEN = "CONFIRM_RESET_OSDBITS";

export interface OsdbitsSettings {
  mode: VisualModeName;
  bootToGame: boolean;
  autoTransitionToMenu: boolean;
  pal: boolean;
  aspectMode: AspectPresentationMode;
  resolutionScale: number;
  language: OsdLanguageCode;
  rngSeed: number;
  towerPreset: TowerPresetSpec;
  speedMultiplier: number;
  camOffsetX: number;
  camOffsetY: number;
  reducedMotion: boolean;
  enableFog: boolean;
  enableLights: boolean;
  enableCubes: boolean;
  enableGlow: boolean;
  enableMotionBlur: boolean;
  letterbox: boolean;
  confirmCancelSwap: boolean;
  audioMuted: boolean;
  masterVolume: number;
  ambientVolume: number;
  sfxVolume: number;
}

export const DEFAULT_FAITHFUL_SETTINGS: Readonly<OsdbitsSettings> = {
  mode: "opening",
  bootToGame: false,
  autoTransitionToMenu: true,
  pal: false,
  aspectMode: "4:3",
  resolutionScale: 1.0,
  language: 1,
  rngSeed: 1,
  towerPreset: { kind: "captured" },
  speedMultiplier: 1.0,
  camOffsetX: 0.0,
  camOffsetY: 0.0,
  reducedMotion: false,
  enableFog: true,
  enableLights: true,
  enableCubes: true,
  enableGlow: true,
  enableMotionBlur: true,
  letterbox: false,
  confirmCancelSwap: true,
  audioMuted: false,
  masterVolume: 0.75,
  ambientVolume: 0.7,
  sfxVolume: 0.8,
};

export interface PersistedSettingsEnvelopeV2 {
  schemaVersion: 2;
  revision: number;
  updatedAtIso: string;
  checksumFnv1a: string;
  settings: OsdbitsSettings;
}

export interface PersistedAssetMeta {
  schemaVersion: 2;
  revision: number;
  sha256: string;
  fnv1a: string;
  byteLength: number;
  storageBackend: "opfs" | "idb";
  importedSlots: string[];
  savedAtIso: string;
}

export type StorageBackendStatus = "indexeddb" | "memory-fallback";

export interface SaveVerificationResult {
  ok: boolean;
  revision: number;
  verifiedReadBack: boolean;
  backend: StorageBackendStatus;
  conflict?: boolean;
  warning?: string;
}

export interface StorageManagerOptions {
  dbName?: string;
  channelName?: string;
  idbFactory?: IDBFactory | null;
  enableOpfs?: boolean;
  simulateQuotaExceeded?: boolean;
  onRemoteUpdate?: (settings: OsdbitsSettings, revision: number) => void;
}

const STORE_SETTINGS = "settings";
const STORE_ASSETS_META = "assets_meta";
const STORE_ASSETS_BLOBS = "assets_blobs";
const SETTINGS_KEY = "user_settings";
const ASSET_KEY = "imported_resource";
const OPFS_FILENAME = "osdbits_user_resource.bin";

function clampNum(v: unknown, min: number, max: number, fallback: number): number {
  if (typeof v !== "number" || !Number.isFinite(v)) return fallback;
  return Math.max(min, Math.min(max, v));
}

function asBool(v: unknown, fallback: boolean): boolean {
  return typeof v === "boolean" ? v : fallback;
}

function sanitizeTowerPreset(raw: unknown): TowerPresetSpec {
  if (typeof raw !== "object" || raw === null) {
    return { kind: "captured" };
  }
  const rec = raw as Record<string, unknown>;
  if (rec.kind === "full") {
    return { kind: "full" };
  }
  if (rec.kind === "simulated") {
    const seed = clampNum(rec.seed, 0, 0xffff_ffff, 1) >>> 0;
    const ngames = Math.round(clampNum(rec.ngames, 0, 126, 36));
    const nboots =
      rec.nboots === undefined ? undefined : Math.round(clampNum(rec.nboots, 0, 50_000, ngames * 3));
    return { kind: "simulated", seed, ngames, nboots };
  }
  return { kind: "captured" };
}

export function sanitizeSettings(raw: unknown): OsdbitsSettings {
  const d = DEFAULT_FAITHFUL_SETTINGS;
  if (typeof raw !== "object" || raw === null) {
    return { ...d, towerPreset: { kind: "captured" } };
  }
  const r = raw as Record<string, unknown>;

  const validModes: readonly VisualModeName[] = ["opening", "idle", "illegal-disc", "menu"];
  const mode: VisualModeName = validModes.includes(r.mode as VisualModeName)
    ? (r.mode as VisualModeName)
    : d.mode;

  const validAspects: readonly AspectPresentationMode[] = ["4:3", "16:9", "fit"];
  const aspectMode: AspectPresentationMode = validAspects.includes(
    r.aspectMode as AspectPresentationMode,
  )
    ? (r.aspectMode as AspectPresentationMode)
    : d.aspectMode;

  const langNum = Math.round(clampNum(r.language, 0, 7, d.language)) as OsdLanguageCode;

  // Support v1 legacy field aliases (`volume` -> `masterVolume`, `muted` -> `audioMuted`)
  const legacyMuted = typeof r.muted === "boolean" ? r.muted : d.audioMuted;
  const legacyVolume = typeof r.volume === "number" ? r.volume : d.masterVolume;

  return {
    mode,
    bootToGame: asBool(r.bootToGame, d.bootToGame),
    autoTransitionToMenu: asBool(r.autoTransitionToMenu, d.autoTransitionToMenu),
    pal: asBool(r.pal, d.pal),
    aspectMode,
    resolutionScale: clampNum(r.resolutionScale, 0.5, 2.0, d.resolutionScale),
    language: langNum,
    rngSeed: clampNum(r.rngSeed, 0, 0xffff_ffff, d.rngSeed) >>> 0,
    towerPreset: sanitizeTowerPreset(r.towerPreset),
    speedMultiplier: clampNum(r.speedMultiplier, 0.0, 4.0, d.speedMultiplier),
    camOffsetX: clampNum(r.camOffsetX, -64.0, 64.0, d.camOffsetX),
    camOffsetY: clampNum(r.camOffsetY, -64.0, 64.0, d.camOffsetY),
    reducedMotion: asBool(r.reducedMotion, d.reducedMotion),
    enableFog: asBool(r.enableFog, d.enableFog),
    enableLights: asBool(r.enableLights, d.enableLights),
    enableCubes: asBool(r.enableCubes, d.enableCubes),
    enableGlow: asBool(r.enableGlow, d.enableGlow),
    enableMotionBlur: asBool(r.enableMotionBlur, d.enableMotionBlur),
    letterbox: asBool(r.letterbox, d.letterbox),
    confirmCancelSwap: asBool(r.confirmCancelSwap, d.confirmCancelSwap),
    audioMuted: asBool(r.audioMuted, legacyMuted),
    masterVolume: clampNum(r.masterVolume, 0.0, 1.0, legacyVolume),
    ambientVolume: clampNum(r.ambientVolume, 0.0, 1.0, d.ambientVolume),
    sfxVolume: clampNum(r.sfxVolume, 0.0, 1.0, d.sfxVolume),
  };
}

export function computeSettingsChecksum(settings: OsdbitsSettings): string {
  const canonical = JSON.stringify(settings);
  return computeFnv1aHex(new TextEncoder().encode(canonical));
}

function idbReq<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error("IndexedDB request failed"));
  });
}

function idbTxComplete(tx: IDBTransaction): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error("IndexedDB transaction error"));
    tx.onabort = () => reject(tx.error ?? new Error("IndexedDB transaction aborted"));
  });
}

export class StorageManager {
  private readonly dbName: string;
  private readonly idbFactory: IDBFactory | null;
  private readonly enableOpfs: boolean;
  private simulateQuotaExceeded: boolean;
  private readonly onRemoteUpdate?: (settings: OsdbitsSettings, revision: number) => void;

  private db: IDBDatabase | null = null;
  private channel: BroadcastChannel | null = null;
  private backend: StorageBackendStatus = "indexeddb";
  private knownRevision = 0;
  private memorySettings: OsdbitsSettings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
  private memoryAssetBytes: Uint8Array | null = null;
  private memoryAssetMeta: PersistedAssetMeta | null = null;
  private lastWarning: string | null = null;

  constructor(options: StorageManagerOptions = {}) {
    this.dbName = options.dbName ?? "osdbits_wasm_db";
    this.idbFactory =
      options.idbFactory !== undefined
        ? options.idbFactory
        : typeof indexedDB !== "undefined"
          ? indexedDB
          : null;
    this.enableOpfs = options.enableOpfs ?? true;
    this.simulateQuotaExceeded = options.simulateQuotaExceeded ?? false;
    this.onRemoteUpdate = options.onRemoteUpdate;

    if (typeof BroadcastChannel !== "undefined") {
      try {
        this.channel = new BroadcastChannel(options.channelName ?? `${this.dbName}_sync`);
        this.channel.onmessage = (ev: MessageEvent) => {
          const data = ev.data as { type?: string; revision?: number } | null;
          if (data && typeof data.revision === "number" && data.revision > this.knownRevision) {
            void this.loadSettings().then((loaded) => {
              this.onRemoteUpdate?.(loaded.settings, loaded.revision);
            });
          }
        };
      } catch {
        this.channel = null;
      }
    }
  }

  getBackendStatus(): StorageBackendStatus {
    return this.backend;
  }

  getKnownRevision(): number {
    return this.knownRevision;
  }

  getLastWarning(): string | null {
    return this.lastWarning;
  }

  setSimulateQuotaExceeded(enabled: boolean): void {
    this.simulateQuotaExceeded = enabled;
  }

  async init(): Promise<{
    settings: OsdbitsSettings;
    revision: number;
    backend: StorageBackendStatus;
    migratedFromV1: boolean;
    warning?: string;
  }> {
    if (!this.idbFactory) {
      this.backend = "memory-fallback";
      this.lastWarning = "IndexedDB unavailable in this browser context; using session memory fallback.";
      return {
        settings: this.memorySettings,
        revision: this.knownRevision,
        backend: this.backend,
        migratedFromV1: false,
        warning: this.lastWarning,
      };
    }

    try {
      this.db = await this.openDatabase();
      this.backend = "indexeddb";
      return await this.loadSettings();
    } catch (err) {
      this.backend = "memory-fallback";
      this.lastWarning = `Storage open failed (${err instanceof Error ? err.message : String(err)}); using session memory fallback.`;
      return {
        settings: this.memorySettings,
        revision: this.knownRevision,
        backend: this.backend,
        migratedFromV1: false,
        warning: this.lastWarning,
      };
    }
  }

  async loadSettings(): Promise<{
    settings: OsdbitsSettings;
    revision: number;
    backend: StorageBackendStatus;
    migratedFromV1: boolean;
    warning?: string;
  }> {
    if (!this.db || this.backend === "memory-fallback") {
      return {
        settings: sanitizeSettings(this.memorySettings),
        revision: this.knownRevision,
        backend: "memory-fallback",
        migratedFromV1: false,
        warning: this.lastWarning ?? undefined,
      };
    }

    const tx = this.db.transaction(STORE_SETTINGS, "readonly");
    const raw = await idbReq<unknown>(tx.objectStore(STORE_SETTINGS).get(SETTINGS_KEY));
    await idbTxComplete(tx);

    if (raw === undefined || raw === null) {
      this.memorySettings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
      return {
        settings: this.memorySettings,
        revision: this.knownRevision,
        backend: "indexeddb",
        migratedFromV1: false,
      };
    }

    if (typeof raw !== "object") {
      this.lastWarning = "Corrupted settings record detected; restored faithful OSDSYS defaults.";
      this.memorySettings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
      return {
        settings: this.memorySettings,
        revision: this.knownRevision,
        backend: "indexeddb",
        migratedFromV1: false,
        warning: this.lastWarning,
      };
    }

    const rec = raw as Record<string, unknown>;

    // Handle v1 -> v2 schema migration cleanly and persist upgraded v2 envelope!
    if (rec.schemaVersion === 1 || rec.version === 1) {
      const sourcePayload = typeof rec.settings === "object" && rec.settings !== null ? rec.settings : rec;
      const upgraded = sanitizeSettings(sourcePayload);
      const baseRev = typeof rec.revision === "number" ? Math.max(0, Math.trunc(rec.revision)) : 0;
      this.knownRevision = baseRev;
      const saved = await this.saveSettings(upgraded, baseRev);
      return {
        settings: upgraded,
        revision: saved.revision,
        backend: saved.backend,
        migratedFromV1: true,
      };
    }

    if (rec.schemaVersion === 2 && typeof rec.settings === "object" && rec.settings !== null) {
      const sanitized = sanitizeSettings(rec.settings);
      const expectedChecksum = computeSettingsChecksum(sanitized);
      if (typeof rec.checksumFnv1a === "string" && rec.checksumFnv1a !== expectedChecksum) {
        this.lastWarning =
          "Settings checksum mismatch (corrupted record); restored faithful OSDSYS defaults.";
        this.memorySettings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
        return {
          settings: this.memorySettings,
          revision: this.knownRevision,
          backend: "indexeddb",
          migratedFromV1: false,
          warning: this.lastWarning,
        };
      }
      const rev = typeof rec.revision === "number" ? Math.max(0, Math.trunc(rec.revision)) : 1;
      this.knownRevision = Math.max(this.knownRevision, rev);
      this.memorySettings = sanitized;
      return {
        settings: sanitized,
        revision: this.knownRevision,
        backend: "indexeddb",
        migratedFromV1: false,
      };
    }

    this.lastWarning = "Unrecognized settings schema; restored faithful OSDSYS defaults.";
    this.memorySettings = sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS);
    return {
      settings: this.memorySettings,
      revision: this.knownRevision,
      backend: "indexeddb",
      migratedFromV1: false,
      warning: this.lastWarning,
    };
  }

  /**
   * Atomically writes `nextSettings` with stale-revision protection and verifies commit
   * by reading back the record in a separate fresh `"readonly"` transaction!
   */
  async saveSettings(
    nextSettings: OsdbitsSettings,
    expectedBaseRevision = this.knownRevision,
  ): Promise<SaveVerificationResult> {
    const clean = sanitizeSettings(nextSettings);
    const checksum = computeSettingsChecksum(clean);

    if (this.simulateQuotaExceeded) {
      this.backend = "memory-fallback";
      this.memorySettings = clean;
      this.knownRevision = expectedBaseRevision + 1;
      this.lastWarning = "Storage quota exceeded; changes kept in session memory.";
      return {
        ok: true,
        revision: this.knownRevision,
        verifiedReadBack: true,
        backend: "memory-fallback",
        warning: this.lastWarning,
      };
    }

    if (!this.db || this.backend === "memory-fallback") {
      this.memorySettings = clean;
      this.knownRevision = expectedBaseRevision + 1;
      return {
        ok: true,
        revision: this.knownRevision,
        verifiedReadBack: true,
        backend: "memory-fallback",
      };
    }

    try {
      let committedEnvelope: PersistedSettingsEnvelopeV2 | null = null;
      let staleConflict = false;
      let currentStoreRevision = this.knownRevision;

      const writeTx = this.db.transaction(STORE_SETTINGS, "readwrite");
      const store = writeTx.objectStore(STORE_SETTINGS);
      const existing = await idbReq<unknown>(store.get(SETTINGS_KEY));

      if (typeof existing === "object" && existing !== null) {
        const exRev = (existing as { revision?: unknown }).revision;
        if (typeof exRev === "number" && exRev > expectedBaseRevision) {
          staleConflict = true;
          currentStoreRevision = exRev;
        }
      }

      if (!staleConflict) {
        const nextRevision = expectedBaseRevision + 1;
        committedEnvelope = {
          schemaVersion: STORAGE_SCHEMA_VERSION,
          revision: nextRevision,
          updatedAtIso: new Date().toISOString(),
          checksumFnv1a: checksum,
          settings: clean,
        };
        store.put(committedEnvelope, SETTINGS_KEY);
      }

      await idbTxComplete(writeTx);

      if (staleConflict || !committedEnvelope) {
        this.knownRevision = Math.max(this.knownRevision, currentStoreRevision);
        return {
          ok: false,
          revision: currentStoreRevision,
          verifiedReadBack: false,
          backend: "indexeddb",
          conflict: true,
          warning: `Stale write rejected: store is at revision ${currentStoreRevision} (expected ${expectedBaseRevision}).`,
        };
      }

      // Mandatory post-write read-back verification in a brand-new readonly transaction!
      const verifyTx = this.db.transaction(STORE_SETTINGS, "readonly");
      const readBack = await idbReq<PersistedSettingsEnvelopeV2 | undefined>(
        verifyTx.objectStore(STORE_SETTINGS).get(SETTINGS_KEY),
      );
      await idbTxComplete(verifyTx);

      if (
        !readBack ||
        readBack.schemaVersion !== STORAGE_SCHEMA_VERSION ||
        readBack.revision !== committedEnvelope.revision ||
        readBack.checksumFnv1a !== checksum ||
        computeSettingsChecksum(readBack.settings) !== checksum
      ) {
        throw new Error("Post-write IndexedDB read-back verification failed");
      }

      this.knownRevision = committedEnvelope.revision;
      this.memorySettings = clean;
      this.channel?.postMessage({ type: "settings-updated", revision: this.knownRevision });

      return {
        ok: true,
        revision: this.knownRevision,
        verifiedReadBack: true,
        backend: "indexeddb",
      };
    } catch (err) {
      this.backend = "memory-fallback";
      this.memorySettings = clean;
      this.knownRevision = expectedBaseRevision + 1;
      this.lastWarning = `Persistence error (${err instanceof Error ? err.message : String(err)}); kept in session memory.`;
      return {
        ok: false,
        revision: this.knownRevision,
        verifiedReadBack: false,
        backend: "memory-fallback",
        warning: this.lastWarning,
      };
    }
  }

  /**
   * Saves optional user-imported resource archive bytes (preferring OPFS for large blobs with
   * automatic IndexedDB fallback) and verifies SHA-256 read-back equality before resolving!
   */
  async saveImportedAsset(
    bytes: Uint8Array,
    importedSlots: string[],
  ): Promise<{ ok: boolean; meta: PersistedAssetMeta; verifiedReadBack: boolean }> {
    const sha256 = await computeSha256Hex(bytes);
    const fnv1a = computeFnv1aHex(bytes);
    const copy = new Uint8Array(bytes.byteLength);
    copy.set(bytes);

    if (!this.db || this.backend === "memory-fallback" || this.simulateQuotaExceeded) {
      const meta: PersistedAssetMeta = {
        schemaVersion: STORAGE_SCHEMA_VERSION,
        revision: ++this.knownRevision,
        sha256,
        fnv1a,
        byteLength: copy.byteLength,
        storageBackend: "idb",
        importedSlots: [...importedSlots],
        savedAtIso: new Date().toISOString(),
      };
      this.memoryAssetBytes = copy;
      this.memoryAssetMeta = meta;
      return { ok: true, meta, verifiedReadBack: true };
    }

    let usedOpfs = false;
    if (this.enableOpfs && typeof navigator !== "undefined" && typeof navigator.storage?.getDirectory === "function") {
      try {
        const root = await navigator.storage.getDirectory();
        const handle = await root.getFileHandle(OPFS_FILENAME, { create: true });
        if ("createWritable" in handle && typeof handle.createWritable === "function") {
          const writable = await handle.createWritable();
          await writable.write(copy);
          await writable.close();
          usedOpfs = true;
        }
      } catch {
        usedOpfs = false;
      }
    }

    const meta: PersistedAssetMeta = {
      schemaVersion: STORAGE_SCHEMA_VERSION,
      revision: ++this.knownRevision,
      sha256,
      fnv1a,
      byteLength: copy.byteLength,
      storageBackend: usedOpfs ? "opfs" : "idb",
      importedSlots: [...importedSlots],
      savedAtIso: new Date().toISOString(),
    };

    const tx = this.db.transaction([STORE_ASSETS_META, STORE_ASSETS_BLOBS], "readwrite");
    tx.objectStore(STORE_ASSETS_META).put(meta, ASSET_KEY);
    if (!usedOpfs) {
      tx.objectStore(STORE_ASSETS_BLOBS).put(copy, ASSET_KEY);
    }
    await idbTxComplete(tx);

    // Mandatory post-write read-back & SHA-256 verification!
    const loaded = await this.loadImportedAsset();
    if (!loaded || loaded.meta.sha256 !== sha256 || (await computeSha256Hex(loaded.bytes)) !== sha256) {
      throw new Error("Post-write asset SHA-256 read-back verification failed");
    }

    return { ok: true, meta, verifiedReadBack: true };
  }

  async loadImportedAsset(): Promise<{ meta: PersistedAssetMeta; bytes: Uint8Array } | null> {
    if (!this.db || this.backend === "memory-fallback") {
      if (this.memoryAssetMeta && this.memoryAssetBytes) {
        return { meta: this.memoryAssetMeta, bytes: new Uint8Array(this.memoryAssetBytes) };
      }
      return null;
    }

    const tx = this.db.transaction([STORE_ASSETS_META, STORE_ASSETS_BLOBS], "readonly");
    const meta = await idbReq<PersistedAssetMeta | undefined>(
      tx.objectStore(STORE_ASSETS_META).get(ASSET_KEY),
    );
    const idbBlob = await idbReq<Uint8Array | ArrayBuffer | undefined>(
      tx.objectStore(STORE_ASSETS_BLOBS).get(ASSET_KEY),
    );
    await idbTxComplete(tx);

    if (!meta) return null;

    let bytes: Uint8Array | null = null;
    if (
      meta.storageBackend === "opfs" &&
      typeof navigator !== "undefined" &&
      typeof navigator.storage?.getDirectory === "function"
    ) {
      try {
        const root = await navigator.storage.getDirectory();
        const handle = await root.getFileHandle(OPFS_FILENAME);
        const file = await handle.getFile();
        bytes = new Uint8Array(await file.arrayBuffer());
      } catch {
        bytes = null;
      }
    }

    if (!bytes && idbBlob) {
      bytes = idbBlob instanceof Uint8Array ? new Uint8Array(idbBlob) : new Uint8Array(idbBlob);
    }
    if (!bytes) return null;

    const actualSha = await computeSha256Hex(bytes);
    if (actualSha !== meta.sha256) {
      this.lastWarning = "Persisted imported asset failed SHA-256 integrity check; ignored.";
      return null;
    }

    return { meta, bytes };
  }

  async exportSettingsJson(): Promise<string> {
    const current = await this.loadSettings();
    const payload = {
      app: "osdbits-wasm",
      schemaVersion: STORAGE_SCHEMA_VERSION,
      exportedAtIso: new Date().toISOString(),
      checksumFnv1a: computeSettingsChecksum(current.settings),
      settings: current.settings,
    };
    return JSON.stringify(payload, null, 2);
  }

  async importSettingsJson(jsonText: string): Promise<SaveVerificationResult> {
    let parsed: unknown;
    try {
      parsed = JSON.parse(jsonText);
    } catch {
      throw new Error("Invalid JSON file: could not parse settings document");
    }
    if (typeof parsed !== "object" || parsed === null) {
      throw new Error("Invalid settings JSON root object");
    }
    const obj = parsed as Record<string, unknown>;
    const rawSettings = typeof obj.settings === "object" && obj.settings !== null ? obj.settings : obj;
    const sanitized = sanitizeSettings(rawSettings);
    return this.saveSettings(sanitized, this.knownRevision);
  }

  async resetToFaithfulDefaults(confirmationToken: string): Promise<SaveVerificationResult> {
    if (confirmationToken !== RESET_CONFIRMATION_TOKEN) {
      throw new Error("Reset aborted: explicit confirmation token required");
    }

    this.memoryAssetBytes = null;
    this.memoryAssetMeta = null;

    if (this.enableOpfs && typeof navigator !== "undefined" && typeof navigator.storage?.getDirectory === "function") {
      try {
        const root = await navigator.storage.getDirectory();
        await root.removeEntry(OPFS_FILENAME);
      } catch {
        // Ignore if file does not exist
      }
    }

    if (this.db && this.backend === "indexeddb") {
      const clearTx = this.db.transaction(
        [STORE_SETTINGS, STORE_ASSETS_META, STORE_ASSETS_BLOBS],
        "readwrite",
      );
      clearTx.objectStore(STORE_SETTINGS).clear();
      clearTx.objectStore(STORE_ASSETS_META).clear();
      clearTx.objectStore(STORE_ASSETS_BLOBS).clear();
      await idbTxComplete(clearTx);
    }

    this.knownRevision = 0;
    const res = await this.saveSettings(sanitizeSettings(DEFAULT_FAITHFUL_SETTINGS), 0);
    this.channel?.postMessage({ type: "reset", revision: res.revision });
    return res;
  }

  close(): void {
    this.channel?.close();
    this.channel = null;
    this.db?.close();
    this.db = null;
  }

  private openDatabase(): Promise<IDBDatabase> {
    return new Promise<IDBDatabase>((resolve, reject) => {
      if (!this.idbFactory) {
        reject(new Error("IndexedDB unavailable"));
        return;
      }
      const req = this.idbFactory.open(this.dbName, STORAGE_SCHEMA_VERSION);
      req.onupgradeneeded = () => {
        const db = req.result;
        if (!db.objectStoreNames.contains(STORE_SETTINGS)) {
          db.createObjectStore(STORE_SETTINGS);
        }
        if (!db.objectStoreNames.contains(STORE_ASSETS_META)) {
          db.createObjectStore(STORE_ASSETS_META);
        }
        if (!db.objectStoreNames.contains(STORE_ASSETS_BLOBS)) {
          db.createObjectStore(STORE_ASSETS_BLOBS);
        }
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error ?? new Error("Failed to open IndexedDB"));
      req.onblocked = () => reject(new Error("IndexedDB upgrade blocked by another tab"));
    });
  }
}
