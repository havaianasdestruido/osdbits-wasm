# Known limitations and fidelity report

This document is deliberately blunt. Every claim below is either backed by a command in
`docs/TESTING.md` or marked as unverified.

## 1. What this is, and what it is not

`osdbits-wasm` is a **browser-native reconstruction** of the PlayStation 2 OSDSYS visual
experience, built from the reverse-engineering work in `aap/osdbits` (the C implementation and
the `docs/*.md` analysis notes, both preserved unmodified in this repo).

It is **not** an emulator: no PS2 code runs. It is **not** a copy of Sony's binaries: no BIOS
image, ROM slice, extracted texture, font page or sound bank is present in the repository or in
the built artifact. It is **not** pixel-identical to a retail console, because the assets are
original work (see §4) and several rendering paths are approximations (see §5).

| Legend | Meaning |
|---|---|
| **Implemented** | Code exists and runs. |
| **Tested** | Covered by an executed automated test. |
| **Verified** | The acceptance criteria were independently demonstrated. |
| **Approximate** | Reconstructed without evidence of exact fidelity. |
| **Blocked** | Cannot be verified in this environment. |
| **Deferred** | Not implemented. |

## 2. Verified in this environment

| Area | Status | Evidence |
|---|---|---|
| Deterministic core (math, `osdRand`, MatrixDrive, camera, LCG) | Verified | `cargo test --workspace` - 25 tests |
| `expand()` LZSS decoder + ROMDIR parser (with fuzzed malformed input) | Verified | `resource::tests::*` incl. 50 property round-trips and 200 malformed blobs |
| Procedural asset pack (25 textures, metrics, meshes, `TEXCBINV = ~TEXCBUMP`) | Verified | `assets::tests::*` |
| Tower field from memory-card history (21 × 6 neighbourhoods tiling all 126 cells) | Verified | `opening::tests::test_boot_history_to_tower_field_mapping` |
| Opening camera state machine vs. idle mode | Verified | `opening::tests::test_opening_vs_idle_camera_progression` |
| Pad cooker (65 536-pair region swap, 400-frame repeat/clobber parity) | Verified | `input_audio::tests::*` |
| Audio mailbox ring + 60-tick reverb ramp | Verified | `input_audio::tests::test_sound_mailbox_ring_and_60_frame_reverb_ramp` |
| WASM artifact: instantiation, ABI, per-mode command streams, menu navigation, asset rejection | Verified | `tests/unit/wasm.test.ts` (9 tests, real compiled module in Node) |
| Persistence: verified write/read-back, reload, migration, corruption quarantine, clamping, export/import, reset | Verified | `tests/unit/storage.test.ts` (9 tests, real IndexedDB via fake-indexeddb) |
| Audio activation ordering (`resume()` inside the gesture, no context before it) | Verified | `tests/unit/audio.test.ts` (5 tests with a recording mock) |
| One-click activation, mode switching, menu/config/browser flow, pause, resize, import rejection, ROMDIR import, persistence across reload, reset | Verified (E2E) | `tests/e2e/app.spec.ts` - 11 tests, all passing |
| Production build | Verified | `npm run build` |

## 3. Blocked in this environment (needs a real GPU)

This sandbox's headless Chromium acquires a WebGPU device but the browser **destroys it after
2–4 frames**. This was reproduced with a *minimal* clear-only WebGPU loop that has nothing to do
with this app (`tests/` history; the probe script is documented in `docs/TESTING.md` §5), across
four different launch-flag combinations (SwiftShader, Vulkan+SwiftShader, in-process GPU, ANGLE).
It is therefore an environment limitation, not an application defect — but it does mean the
following cannot be verified here:

- **Sustained GPU rendering** beyond the first submitted frames.
- **Pixel-exact visual regression** against reference captures. The app captures a first-frame
  pixel signature when the readback succeeds, and the E2E test asserts on it opportunistically;
  in this sandbox the readback usually fails, so the test asserts on the *submitted frame's*
  draw/pass/vertex counts instead.
- **Frame-time percentiles / endurance budgets** on real hardware. The simulation itself is
  cheap (≈0.14 ms per frame for 60 frames in Node), but GPU time is unknown here.

The app degrades correctly in this situation: on device loss it keeps simulating, shows
`gpu unavailable — … · simulation running` in the HUD, and retries the device up to three times.
That behaviour *is* verified by `tests/e2e/app.spec.ts` › "a lost GPU device degrades to
simulation-only and never blocks the UI".

## 4. Assets: procedural by default, import optional

The default experience uses **original procedural assets** generated in Rust
(`crates/osdbits-core/src/assets.rs`): tower wall texture, fog layers, SCE banner, illegal-disc
text, KABE tunnel, bump/reflection/flow pages, orb sprites, button icons, and a rasterized
256×480 ASCII font plus a 512×80 symbol page. They reproduce the *role* of each retail resource
(size, format, alpha semantics, tiling) but not its pixels.

Users may optionally import a BIOS image (or a bare `TEXIMAGE`/`FNTIMAGE` container) that they
supplied themselves; the resource is decoded in the tab with the real `expand()` LZSS algorithm
and the ROMDIR rules, and the import is stored locally. **No imported bytes leave the tab and
none are committed to the repository.** An import of a non-ROMDIR file is rejected with a clear
message (tested).

## 5. Approximation register

| Subsystem | Approximation | Why |
|---|---|---|
| Tower field layout | Cell spacing `27 × 26` units around `z = 168`, column half-length `13 + 9.5·height` | The retail cell positions come from `histSrcPos` (`0x27a670`) → `towerPos` (`0x326c10`), which is populated at runtime from a table that is zero in the static image. The *density*, *radial brightness falloff* (`towerGrid`), *per-column depth* and *boot-history→height* mapping are faithful; the exact lattice is not. |
| `histCells[21][6]` | Reconstructed to satisfy the documented invariant (126 cells tiled exactly once) | Same reason: the table is zero in the static image and this workspace has no BIOS to read it from. |
| VU1 tower pipeline | Reimplemented on the CPU: 4 directional lights + ambient with the `vucode_1.vsm` dot-product form, per-vertex far-end blackout, texture `u` offset per column | The real path builds a DMA chain and runs microcode. Emulating VU1 in the browser is unnecessary; the shading result is what is visible. |
| Cube refraction | Screen-space UV displaced by the projected normal with a centre-relative zoom | Matches `cube_21B798`'s formula shape; the per-pass scroll phases are the documented constants. |
| Config clock rods / cubes | 5-pass rods and 8-pass cubes are drawn with the documented pass *kinds* (refraction, emboss, mask composite) but the mesh is a simplified prism/box and the passes are fewer than the ROM's per-face loops | Full `0x22D2E8`/`0x22D920` chains need work-buffer plumbing whose exact register dance is documented but whose visual delta at browser resolution is small. |
| Zoom blur | 5 bilinear shrink/stretch round trips between screen and work buffers | Same count and intent as `0x22C3C0`; extents are simplified. |
| Text | Fixed 8×12 ASCII atlas + 16×2 symbol page, `\7oNNN` and `\7rN.NN` escapes implemented; Shift-JIS not implemented | `FONTM`/`KROM` kanji unpacking is deferred (see §6). Non-ASCII bytes map to `?` rather than indexing past the atlas (this was a real panic, now covered by a regression test). |
| Music | Original synthesized beds per BGM slot, driven by the real `OSDDispatch` message vocabulary | Redistributing the retail `SND*` banks is not permitted. The *message flow* (which slot plays when, the 60-tick reverb ramp, which SE fires on which input) is faithful. |
| Clock | Software RTC seeded from the persisted setting, not the host clock | Deterministic replays; the System Configuration date/time editor writes it back. |
| Region | Japan/World switch for Circle/Cross and the hint bar | `0x204318()` reads `rom0:ROMVER`, which does not exist here. |

## 6. Deferred

- Shift-JIS / kanji text (`FONTM`, `KROM`, `FNTEX000/001`) and the 72-slot VRAM glyph cache.
- The ROM's own 10-pass per-face glass loops (see §5) and the `TEXCBUMP`/`TEXCBINV` two-walk
  bloom at full per-face fidelity.
- Full 3D memory-card icon browser (`ICOB*`): Mode D's Browser screen edits the 21 boot-history
  slots that actually drive the opening instead.
- MechaCon disc certification, `SYSTEM.CNF` parsing and `LoadExecPS2` game boot: hardware
  boot-loader behaviour, represented by Mode C.
- `StSePlay` effect *names* beyond the four used by the ported screens (4 confirm, 5 toggle,
  6 cursor, 10 cancel); the rest are forwarded but synthesize a neutral click.

## 7. Compatibility

- **WebGPU is required for rendering.** Without it the app starts, reports the reason on the
  activation screen and in the HUD, and keeps the simulation (and the menu flow) running.
- **Web Audio is optional.** If it is missing or blocked, the scene runs silently and the
  activation screen says so.
- **IndexedDB is optional.** Without it, settings are kept in memory for the session and every
  save reports `verified: false` — the UI never claims a save succeeded when it could not read
  the record back.
- Browsers are expected to show their normal audio/GPU permission behaviour; nothing here can
  promise otherwise.
