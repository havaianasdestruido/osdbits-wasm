# Phase 0 Baseline Audit Report (`aap/osdbits` -> `osdbits-wasm`)

Date: 2026-10-31  
Branch: `arena/750f05c6-osdbits-wasm`  
Base Commit: `ea0c68712b28b36fe033ed4fe488d86958de53d2` (`aap/osdbits` `master`)

---

## 1. Executive Summary

`aap/osdbits` is a reverse-engineered PlayStation 2 Emotion Engine (EE) + Vector Unit 1 (VU1) + Graphics Synthesizer (GS) reconstruction and decompilation study of the SCPH-10000 / retail PS2 **OSDSYS** boot sequence, idle tower field, illegal-disc warning screen, and main/system-configuration menu.

The repository intentionally contains **zero proprietary Sony BIOS binaries, ROM textures, font atlases, or audio banks** (`osdbits/res/` is gitignored and populated locally by end users running `osdbits/tools/extract-res.py` on their own dumped BIOS). Targeting the native PS2 `osdbits/Makefile` requires an offline Sony/free PS2 EE toolchain (`/usr/local/freesce/ee`, `/usr/local/sce_24/ee`, `ee-gcc`, `ee-dvp-as`).

All existing reverse-engineering code, VU microcode sources, GS dump analysis scripts, matching decompilation harnesses, and documentation in `osdbits/`, `docs/`, `matching/`, `tools/`, `ps2expand.c`, `notes.txt`, and `vucode_*` are preserved intact as authoritative reference material for `osdbits-wasm`.

---

## 2. Inventory of Existing Repository Contents

| Path | Lines / Files | Role & Findings |
| :--- | :--- | :--- |
| `osdbits/main.c`, `inc.h`, `res.h`, `res.c` | ~1,303 lines | PS2 EE entry point, GS initialization (`640x224` NTSC @ 60 Hz `dt=1.0`, `640x256` PAL @ 50 Hz `dt=1.2`), 78-entry `Res` table, mode router (`MODE_BOOT`, `MODE_IDLE`, `MODE_ILLEGAL`, `MODE_MENU`). |
| `osdbits/opening.c` | 4,355 lines | Complete reconstruction of the Opening / Towers scene (`MODE_BOOT`), Indefinite Idle Towers (`MODE_IDLE`), and Red Illegal-Disc scene (`MODE_ILLEGAL`). |
| `osdbits/towerchain.dsm`, `vucode_1.vsm` | 369 lines | VIF1 DMA chain template and VU1 microcode transforming tower geometry with 4 directional lights, ambient term, and distance alpha fade. |
| `osdbits/menu.c` | 1,695 lines | Module U main scene: 7 RTC clock-driven orbiting light orbs, 50-sample decaying ribbon trails, entry scatter/color-ease, elliptical vignette, and 5-pass bilinear `MenuZoomBlur`. |
| `osdbits/menuback.c` | 1,004 lines | System Configuration background: `16x33` breathing cylindrical smoke tunnel (`TEXCKABE` 64x64 tiled 2x2), pre-tint `wb3`/`wb4` refraction copy, and `{0x37, 0x28, 0x3C}` screen tint. |
| `osdbits/menuconfig.c` | 2,249 lines | System Configuration 3D widgets: 90-frame `cfgAnim`, 12 hexagonal bevelled glass clock rods (5-pass untinted refraction, split active-hour minute fill bar, 2-walk `TEXCFLOW` + `TEXCBUMP`/`TEXCBINV` sparkle bloom), and 5 zig-zag spinning glass cubes (8-pass tinted refraction + `TEXCREFA` reflection + silhouette mask). |
| `osdbits/menutext.c` | 2,117 lines | Proportional OSD font/glyph renderer, button-hint shape icons (`TEXCMARU`), Main Menu (`Browser`, `System Configuration`), System Configuration options, and live `Clock Adjustment` digit editor. |
| `osdbits/pad.c`, `osdbits/sound.c` | 389 lines | DualShock2 button edge/auto-repeat handling (20-frame delay, 3-frame period, regional O/X confirm swap) and IOP `libsd` sound RPC wrapper (`SNDIMAGE` bank). |
| `vucode_1.vu`..`vucode_7.vu`, `vucode_2.vsm`..`vucode_6.vsm` | 12 files | Disassembled and reassembled VU microprograms from OSDSYS (`vucode_1` drives towers; `vucode_2` is uploaded by Module U in retail ROM but never executed—all menu 3D runs on EE). |
| `ps2expand.c`, `tools/extract-res.py`, `osdbits/tools/extract-res.py` | 3 files | Host C decompression tool (`expand()`) and Python `ROMDIR` + LZ extractors (`osdbits/tools/extract-res.py` adds `--tables` for `FONTDATA.inc` and `MENUGEOM.inc`). |
| `osdbits/tools/gsdump-*.py`, `gsmem.py`, `gsreplay/` | 9 files | PCSX2 `.gs` dump decoders, draw-call loggers, VRAM unswizzlers, and a 32-bit C software GS comparison harness (`gsreplay` + `gscmp`). |
| `docs/` | 16 docs + 5 PNGs | Deep architectural reports (`OSDSYS-STRUCTURE-REPORT.txt`, `towers-analysis.md`, `glass-refraction.md`, `object-order.md`, `menu-*.md`, `pad.md`, `sound.md`, `gscmp-FINDINGS.md`) and visual references (`opening.png`, `illegal.png`, `menu.png`, `config1.png`, `config2.png`). |
| `matching/` | 35+ files | Byte-exact `ee-gcc 2.9-ee-991111 -O2` function matching campaign against an external `expanded.bin` (untracked). |

---

## 3. Baseline Build & Tooling Check Results

Non-destructive checks executed prior to project scaffolding:

1. **`ps2expand.c` (Host C):** Compiles cleanly with `gcc -O2 -Wall -Wextra` with zero warnings and prints expected CLI usage (`usage: ps2expand input [output]`).
2. **Python Utilities (`tools/*.py`, `osdbits/tools/*.py`):** All scripts pass `python3 -m py_compile`; CLI `--help` executes cleanly on Python 3.11.2.
3. **`osdbits/Makefile` (PS2 Target):** `make -n -C osdbits` fails immediately as expected (`No rule to make target '/usr/local/freesce/ee/lib/crt0.o'`) because the proprietary/offline PS2 EE SDK and user-extracted `osdbits/res/*.inc` files are absent by design.
4. **`osdbits/tools/gsreplay`:** Cannot link on this 64-bit host without external 32-bit `libgpu2-patched.a` (`/u/aap/othersrc/libgpu2/port/libgpu2-patched.a`) and 32-bit X11 multilib libraries.
5. **Existing Automated Tests:** None exist yet in the repository (`matching/check.py` requires an external Sony `expanded.bin` and `ee-gcc`).

---

## 4. Verified Reconstruction Constants vs. Known Gaps in Upstream `osdbits`

### Verified from `osdbits/*.c` and `docs/*.md`
- **Deterministic PRNGs:**
  - `osdRand` / `menuRand`: `seed = seed * 1103515245 + 12345; return (seed & 0x7fffffff) * 4.656613e-10f` (`lightsSeed = 4002`).
  - `towerRand`: `seed = seed * 1103515245 + 12345` seeded with `0x2b992ddf`.
- **Opening & Idle (`opening.c`):**
  - 21-slot × 6-cell tower history ring, 14 initial pre-advances, sprawl relocation at `count >= 14 && (count - 14) % 10 == 0`.
  - 20×20 two-bump height grid (`[32, 220]` hash-dithered), `14×9` tower grid (`126` active columns), `towerSway`, clamped top-cap UV `a0`, 4-light VU1 shading.
  - 8-state camera timeline (`{16, 56, 104, 320, 672, 800, 1160, 1160}`), `camPosZ` (`15971 -> 3371 -> -21029`), `MODE_IDLE` canopy lock (`camState <= 3`).
  - 6-layer `17×17` fog mesh, 4 orbiting lights with 128-point trails, 5 10-pass refractive glass cubes (`uvArg = -0.084f`, Fresnel `(1 - l)^2 * 0.5`, per-cube backdrop capture), fly-up blur, black fades, and letterbox bars.
- **Illegal-Disc (`opening.c`):**
  - Starts at `z = 672, camState = 4`; 7 spinning 16-gon flare rings + 5 `TEXOFLAR` lens sprites + bloom factor `112/128`, 128 falling red fog patches, 5 red 10-pass glass cubes (`illegalCubePasses`), `DrawIllegalFades`.
- **Menu & System Configuration (`menu.c`, `menuback.c`, `menuconfig.c`, `menutext.c`, `docs/menu-config.md`):**
  - 7 RTC-driven orbs (`rateX = sec * 65536 / 60`, `rateY = min * 65536 / 60`, `mdRotY(rateY * 1100)`, `mdRotX((i + 21) * rateX)`), 50-sample trails (`(age^4, age^2, age)` RGB decay), entry scatter, 5-pass `MenuZoomBlur`, `(74, 37)` vignette.
  - `16×33` `TEXCKABE` breathing tunnel (`R = 6000`), pre-tint refraction buffer, `{0x37, 0x28, 0x3C}` tint.
  - 12 hexagonal bevelled glass rods (circumradius `2.6`, height `26`, bevel `2.34` at `y = 26..26.39`, `R = 20` carousel, 5-pass untinted refraction, split active-hour minute fill bar, 2-walk `TEXCFLOW` + `TEXCBUMP`/`TEXCBINV` bloom).
  - 5 zig-zag glass cubes (`±2.64`, camera-space positions `(-11.5,-11.5,47.5)`..`(-11.25,11.5,47.5)`, 8-pass tinted refraction + `TEXCREFA` + silhouette mask).

### Upstream Approximations & Gaps Requiring Honest Tracking
1. **Clean-Room Default Assets:** Because Sony `TEXIMAGE`, `FNTIMAGE`, `SNDIMAGE`, `FONTDATA`, and `MENUGEOM` cannot be distributed, `osdbits-wasm` must generate original clean-room procedural textures, font glyphs, and Web Audio synthesis by default, while supporting optional validated local BIOS import (`ROMDIR` + `expand()`). Note: `MENUGEOM` exact vertex/normal/UV equations ARE documented mathematically in `docs/menu-config.md` and can be synthesized procedurally from first principles!
2. **Sony Trademarks:** Retail Opening displays `"Sony Computer Entertainment"`. Per legal policy, default clean-room mode displays a conservative non-infringing title (`"OSDSYS // osdbits-wasm"` or user-toggleable clean overlay) and never embeds Sony logos or trademarks.
3. **Illegal-Disc Localized Text (`DoIllegalText`):** Upstream `opening.c` notes this routine was ported from disassembly without a retail illegal-disc GS dump (`gscmp`) verification; `osdbits-wasm` marks its text placement as `Approximate`.
4. **Browser Submenu:** Upstream `osdbits` leaves `"Browser"` (memory card / disc icon grid) unimplemented in `menutext.c` (only `"Browser"` label on main menu + `"System Configuration"` submenu are reconstructed). `osdbits-wasm` provides a clearly labeled OSDSYS-inspired interactive Browser/Scene-Preview view badged as `Approximate` alongside the verified `System Configuration` submenu.
5. **Minor Upstream GS Divergences (`docs/gscmp-FINDINGS.md`):** Sub-pixel `XYZ2` rounding and minor `FogPatch` phase initialization differences documented by upstream `gscmp` are preserved/documented transparently.

---

## 5. Sandbox & Toolchain Audit

- **Host OS:** Debian GNU/Linux 12 (bookworm) `x86_64`
- **Node.js / npm:** `v22.22.3` / `10.9.8`
- **Rust / WASM Toolchain:** `rustc 1.99.0` stable (`cargo 1.99.0`, `rustfmt 1.10.0`, `clippy 0.1.99`) with both `x86_64-unknown-linux-gnu` and `wasm32-unknown-unknown` targets verified working.
- **Headless Browser & WebGPU:** `Chromium 153.0.8010.0` driven via Playwright (`playwright-core` / `@playwright/test`).
  - **Physical Hardware GPU:** Absent (`/dev/dri` does not exist in container).
  - **Software WebGPU Execution:** Empirically verified working in Chromium 153 via Dawn + Google **SwiftShader Vulkan 1.3** (`vendor: "google"`, `architecture: "swiftshader"`, `isFallbackAdapter: true`), supporting WGSL shader compilation, offscreen render passes, blending, and `copyTextureToBuffer` pixel readback, as well as WebAssembly, IndexedDB, OPFS (`navigator.storage.getDirectory`), and `AudioContext`.
  - **Honest Reporting Policy:** All diagnostics and test reports explicitly distinguish (a) CPU/mock unit tests, (b) real Chromium + Dawn/SwiftShader software-adapter WebGPU execution (`isFallbackAdapter: true`), and (c) physical hardware GPU execution (`Blocked` in this container; covered via dedicated GPU CI workflow for hardware runners).

---

## 6. Target Architecture (`osdbits-wasm`)

- **`crates/osdbits-core` (Rust -> `wasm32-unknown-unknown` + native tests):**
  - Deterministic IEEE-754 `f32` math, exact `osdRand`/`towerRand` LCGs, 16-bit `MatrixDrive` trig, Opening/Idle/Illegal/Menu/Config scene simulation, clean-room procedural texture & mesh synthesis, bounds-checked `ROMDIR` + LZ `expand()` BIOS resource extractor, and flat zero-copy vertex/command buffer generation.
- **`src/renderer/` (TypeScript + WGSL + Canvas 2D Fallback):**
  - Single explicit ownership model: TypeScript manages `GPUDevice`, swapchain, offscreen refraction/bloom textures, and WGSL pipelines (including PS2 GS `2x` modulation, `SrcAlpha`, `Additive`, and `reverse-subtract` subtractive blending); Rust/WASM owns all simulation and draw-command preparation. Automatic fallback to a deterministic Canvas 2D software renderer when WebGPU is unavailable or lost.
- **`src/audio/` (Browser-Safe Web Audio):**
  - Synchronous `AudioContext` creation/`resume()` inside the single **Start OSDSYS** user gesture handler, procedural ambient/crystalline/navigation synthesis, mute/volume controls, and silent degraded fallback if audio initialization fails.
- **`src/storage/` (Versioned IndexedDB + OPFS):**
  - Versioned schema (`v1 -> v2` migrations), mandatory post-write read-back checksum verification, cross-tab `BroadcastChannel` + monotonic revision stale-write guard, `navigator.storage.persist()` handling, JSON settings export/import, confirmed factory reset, and integrity-hashed user asset storage.
- **`src/ui/` (Minimal Nostalgic PS2-Inspired Shell):**
  - Single-gesture **Start OSDSYS** entry gate (works immediately with zero BIOS), keyboard + pointer accessible OSDSYS navigation & HUD customization drawer, reduced-motion support, and live diagnostics/fidelity inspector.
