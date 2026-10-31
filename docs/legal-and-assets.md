# Legal Policy, Clean-Room Assets & Optional Local Resource Import

## 1. Zero Proprietary Sony BIOS / ROM Artifacts Policy

`osdbits-wasm` ships and runs **100% out-of-the-box with clean-room procedural assets** and contains **zero proprietary Sony PlayStation 2 BIOS dumps, ROM images, firmware fonts (`FONTM` / `FNTIMAGE`), extracted `TEXIMAGE` textures, `MENUGEOM` meshes, or `SNDIMAGE` audio samples** anywhere in Git history, source files, test fixtures, visual regression baselines, or release bundles.

> **Important Legal Note**: Noncommercial, educational, or nostalgic intent does **not** eliminate copyright restrictions on redistributing proprietary console firmware or extracted ROM assets. Never commit, upload, or bundle `bios.bin`, `SCPH-*.bin`, `osdbits/res/*.inc`, or raw `ROMDIR` dumps into forks or static deployments of this repository.

---

## 2. Automated Repository Guard (`npm run check:legal`)

`scripts/check-legal-artifacts.mjs` runs automatically before builds (`npm run build`) and in CI (`.github/workflows/ci.yml`), enforcing two hard invariants:

1. **Forbidden Artifact Scan**: Rejects any file matching PS2 BIOS/ROM naming patterns (`scph-*`, `*.rom0`, `*_EXP.inc`, `FONTDATA.inc`, `MENUGEOM.inc`, non-empty `osdbits/res/`, PCSX2 `.p2s`, IDA `.idb`, etc.) and scans binary file headers for `RESET\0\0\0\0\0`, `TEXIMAGE`, `SNDIMAGE`, and `FNTIMAGE` `ROMDIR` signatures.
2. **Historical Provenance Integrity**: Verifies via `git diff ea0c687` that all original `aap/osdbits` PS2 reconstruction files (`osdbits/`, `matching/`, `tools/`, `ps2expand.c`, `vucode_*`) remain unmodified byte-for-byte.

---

## 3. Clean-Room Out-of-the-Box Procedural Assets

When no user resource file has been imported, `web/src/assets/proceduralTextures.ts` deterministically synthesizes all 23 texture slots required by Modes A–D in ~150 ms on startup:

- **`WAL0` (`256x256`, 6 mip levels down to `8x8`)**: Brushed architectural monolith concrete noise with PS2 `PSMCT16` alpha (`0x80`).
- **`FOG0` (`128x128`, Mode C `group = 2`)**: Crimson volumetric FBM cloud patch (`[1.0, 0.2, 0.24]`) used exclusively by `DrawIllegalFog`.
- **`FOG1..FOG4` (`64x64`, Modes A/B `group = 1`)**: Cool atmospheric mist clouds modulated by `fogColors` vertex blue tints.
- **`CRBL`, `FLAR`, `BLUR`, `NAVI`, `DOT`**: Radial Gaussian/diffraction orb cores, lens flares, and clock dots.
- **`KABE`, `FLOW`, `BUMP`, `BINV`, `REFA`, `C64`**: Seamless ribbed tunnel backdrop, crystal caustics, complementary normal-emboss pair (`BUMP + BINV`), and glass specular reflection mask.
- **Legal-Safe Overlays**: Clean-room `OSDBITS` geometric pixel banner (replacing Sony's trademarked `TEXOSCE` logo) and localized `"Please insert a supported format disc."` message.

---

## 4. Optional Local User-Owned Resource Import

Users who legally own a PlayStation 2 console and have dumped their own BIOS image (the same dump used locally with PCSX2 or `tools/extract-res.py`) may optionally load it locally inside their browser:

1. Open **Customize (Mode E)** -> **Optional Authorized Resource Import**.
2. Select your local BIOS dump or `ROMDIR` resource bundle.
3. **100% Client-Side Processing**:
   - The file never leaves your browser (`0` network uploads; verified by `tests/prod/production-bundle.spec.ts`).
   - `crates/osdbits-core/src/expand.rs` locates `RESET` / `ROMDIR` and decompresses `TEXIMAGE` entries in safe, bounds-checked Rust (`#![forbid(unsafe_code)]`, capped at `16 MiB` against malformed/zip-bomb inputs).
   - `web/src/assets/userResourceImporter.ts` validates dimensions and pixel formats (`PSMCT32`, `PSMCT16`, `PSMT8`, `PSMT4`), computes a SHA-256 integrity hash, updates the live WebGPU textures immediately, and persists the verified bundle locally in **OPFS** (or IndexedDB fallback) with mandatory post-write read-back verification.
   - Any unrecognized or corrupt file is cleanly rejected without crashing, preserving clean-room procedural textures.
4. Clicking **Reset to Defaults** -> **Confirm Reset** purges any locally imported asset from OPFS/IndexedDB and restores the 100% procedural clean-room textures.
