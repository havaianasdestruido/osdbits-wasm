# Persistence Schema, Migrations & Durability (`osdbits-wasm`)

`osdbits-wasm` persists user preferences, Mode E customizations, audio settings, and optional user-imported `TEXIMAGE` / `ROMDIR` binary containers locally in the browser using versioned IndexedDB (`osdbits-wasm-db`, `CURRENT_SCHEMA_VERSION = 2`) with optional Origin Private File System (OPFS) blob backing (`src/persistence/storage.ts`).

Zero data is ever transmitted to external servers.

---

## 1. IndexedDB Object Stores (`osdbits-wasm-db` v2)

| Store Name | Key | Record Type | Purpose |
| --- | --- | --- | --- |
| `preferences` | `'user-settings'` | `StoredPreferencesEnvelope` | Versioned (`schemaVersion: 2`), monotonically revisioned user preferences, Mode E tuning, and audio mixer state. |
| `assets` | `'imported-teximage'` | `StoredImportedAssetRecord` | Validated user-imported `TEXIMAGE`/`ROMDIR` bytes, SHA-256 hex digest, imported texture count, and OPFS flag. |
| `meta` | `'session-meta'` | Reserved metadata | Extensible session metadata cleared atomically on confirmed factory reset. |

### `StoredPreferencesEnvelope` (Schema v2)

```json
{
  "schemaVersion": 2,
  "revision": 3,
  "updatedAtIso": "2026-10-31T20:00:00.000Z",
  "writerTabId": "tab-a1b2c3d4",
  "settings": {
    "preferredMode": 0,
    "bootCompletionPolicy": 0,
    "seed": 536871684,
    "resolutionScale": 1.0,
    "hudCollapsed": false,
    "forceCanvasFallback": false,
    "tuning": {
      "isPal": false,
      "speedScale": 1.0,
      "idleDrift": 1.0,
      "fogEnabled": true,
      "bloomEnabled": true,
      "refractionEnabled": true,
      "reducedMotion": false,
      "screenSize": 0,
      "showBanner": true,
      "language": 0,
      "confirmWithCircle": false,
      "towerGames": 21
    },
    "audio": {
      "muted": false,
      "masterVolume": 0.7,
      "ambientVolume": 0.55,
      "sfxVolume": 0.75,
      "pauseOnHidden": true
    }
  }
}
```

---

## 2. Schema Migrations (`v1 -> v2`) & Sanitization

- `migratePreferencesEnvelope(raw)` upgrades legacy `v1` flat records (`{ schemaVersion: 1, mode, speedScale, muted, ... }`) into the nested `v2` envelope without losing user selections.
- `sanitizeUserSettings(raw)` clamps every numeric field (`speedScale ∈ [0.25, 2.5]`, `idleDrift ∈ [0.0, 2.0]`, `resolutionScale ∈ [0.5, 2.0]`, `towerGames ∈ [0, 21]`, `language ∈ [0, 6]`, `screenSize ∈ [0, 2]`, `seed ∈ [0, 2^32 - 1]`) and replaces `NaN`/`Infinity` or malformed types with `FAITHFUL_DEFAULT_SETTINGS`.

---

## 3. Mandatory Read-Back Verification & Integrity Checks

1. **Settings Write Verification:** Every `saveSettings()` commits its `readwrite` transaction and immediately opens an independent `readonly` transaction to read back `'user-settings'` and verify `revision` and serialized `settings` equality before reporting `readBackVerified: true`.
2. **Imported Asset SHA-256 Verification:**
   - During import, `osdbits-core` validates the `ROMDIR` + LZ streams in WASM and computes a FIPS 180-4 SHA-256 digest.
   - `saveImportedAsset()` optionally writes the binary to OPFS (`osdbits-imported-teximage.bin`), stores the record in IndexedDB `'assets'`, reads it back in a fresh transaction, and recomputes `SHA-256` via Web Crypto (`crypto.subtle.digest`) to confirm zero storage corruption.
   - On subsequent page reloads or browser restarts, `loadImportedAsset()` recomputes `SHA-256` before passing bytes to WASM, rejecting tampered or truncated blobs cleanly.

---

## 4. Multi-Tab Stale-Write Protection

- Each `PersistenceStore` instance generates a unique `tabId`, tracks `knownRevision`, and listens on `BroadcastChannel('osdbits-wasm-db-sync')`.
- Inside `saveSettings()`, the existing stored `revision` is read inside the `readwrite` transaction prior to `put`:
  - When `options.rejectIfStale` is set and `existingRev > knownRevision`, the transaction aborts and reports `conflictDetected: true`.
  - Otherwise, `nextRevision = Math.max(knownRevision, existingRev) + 1` advances monotonically and broadcasts the updated envelope to sibling tabs so open tabs stay synchronized.

---

## 5. Graceful Fallback, Export/Import & Confirmed Reset

- **Blocked / Private Storage Fallback:** If `indexedDB` is absent or throws `SecurityError`, `PersistenceStore` falls back transparently to in-memory session storage (`usingMemoryFallback: true`) and surfaces the reason in the HUD storage status banner.
- **Storage Durability (`navigator.storage.persist()`):** Users can request durable storage via **Request Durable Storage**; `queryDurability()` reports actual browser grant status (`persistedGranted`) and quota estimates without assuming permission is granted automatically.
- **JSON Export / Import:** Settings can be exported to `osdbits-settings.json` and re-imported with full syntax, root-shape, clamping, and read-back validation.
- **Two-Step Confirmed Factory Reset:** `resetAllConfirmed(FACTORY_RESET_TOKEN)` requires explicit confirmation (`#factory-reset-btn` -> `#confirm-factory-reset-btn`), clears `'preferences'`, `'assets'`, `'meta'`, and OPFS files, restores procedural clean-room textures, and notifies sibling tabs.
