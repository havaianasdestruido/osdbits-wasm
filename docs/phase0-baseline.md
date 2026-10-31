# Phase 0 Baseline & Reconnaissance Report (`osdbits-wasm`)

Date: 2026-10-31  
Base commit inspected: `ea0c68712b28b36fe033ed4fe488d86958de53d2` (`aap/osdbits` fork)

This document records the complete read-only reconnaissance of the existing repository before any code changes were made for `osdbits-wasm`. Every architectural decision, parameter classification, and task in `/TODO.MD` is grounded in the actual contents of this repository.

---

## 1. Repository Layout & Provenance

| Path | Contents | Role in `osdbits-wasm` |
| --- | --- | --- |
| `osdbits/` | Standalone PS2 Emotion Engine (EE) C + VU1 + DMA reconstruction (`main.c`, `inc.h`, `res.h`, `res.c`, `opening.c`, `menu.c`, `menuback.c`, `menuconfig.c`, `menutext.c`, `pad.c`, `sound.c`, `towerchain.dsm`, `vucode_1.vsm`, `Makefile`) and `osdbits/tools/*` GS dump/replay scripts. | Primary behavioral & mathematical specification. Preserved untouched for provenance. |
| `matching/` | Byte-accurate decompilation campaign (`matching/src/anim.c`, `blit.c`, `config.c`, `flare.c`, `fog.c`, `menu.c`, `menudraw.c`, `inventory.tsv`, `inventory-matrix.txt`, `check.py`). | Ground-truth tie-breaker for constants, control flow, and compiler-verified formulas against retail SCPH-10000 `OSDSYS` (`ee-gcc 2.9-ee-991111 -O2`). Preserved untouched. |
| `vucode_1.vu` .. `vucode_7.vu`, `vucode_2.vsm` .. `vucode_6.vsm` | Disassembled and reconstructed VU1 microcode programs (towers in `vucode_1`, Module U menu meshes/screen-blits in `vucode_2`..`vucode_6`). | Exact reference for vertex transforms, lighting, normal projection, and GS packet generation. Preserved untouched. |
| `docs/` | Reverse-engineering reports (`towers-analysis.md`, `glass-refraction.md`, `object-order.md`, `OSDSYS-STRUCTURE-REPORT.txt`, `osdsys-map.md`, `osdsys_re-crossref.md`, `gscmp-FINDINGS.md`, `menu-*.md`, `pad.md`, `sound.md`, `rom-survey.md`, `ee-kernel-survey.md`) and 5 reference captures (`opening.png`, `illegal.png`, `menu.png`, `config1.png`, `config2.png`). | Cross-check documentation and visual layout reference. Note: `OSDSYS-STRUCTURE-REPORT.txt` (2026-08-21) and `gscmp-FINDINGS.md` (2026-09-01) predate fixes already landed in `osdbits/*.c` by 2026-09-02 (`ea0c687`). |
| `tools/extract-res.py`, `ps2expand.c`, `notes.txt` | BIOS `ROMDIR` container unpacker (`TEXIMAGE`, `FNTIMAGE`, `SNDIMAGE`, `--tables`), LZSS-variant `Expand` decompressor, and research notes. | Specification for optional user-supplied resource import; zero extracted BIOS files exist in git (`.gitignore` excludes `osdbits/res/`). |

---

## 2. Existing Buildability & Test Baseline

1. **`osdbits/Makefile`**: Targets PS2 Emotion Engine (`ee-gcc`, `ee-dvp-as`, `ee-ld`) against `/usr/local/freesce/ee` and `/usr/local/sce_24/ee` SDK headers/libraries, and `#include`s extracted Sony BIOS headers in `osdbits/res/*.inc` (`TEX*_EXP.inc`, `FNT*_EXP.inc`, `SND*_EXP.inc`, `FONTDATA.inc`, `MENUGEOM.inc`). **Cannot build in a clean checkout** without proprietary/external PS2 SDK toolchains and user-extracted BIOS resources.
2. **`ps2expand.c`**: Standard host C utility (`gcc -O2 ps2expand.c`); compiles cleanly on Linux `gcc` when needed as a reference oracle for LZ decompression tests.
3. **`osdbits/tools/gsreplay`**: Requires external 32-bit `gcc -m32` + `libgpu2` software GS rasterizer not present in the repository.
4. **Automated Tests / CI**: Zero automated test suites and zero GitHub Actions workflows existed at baseline `ea0c687`.

---

## 3. Subsystem-by-Subsystem Audit of `osdbits`

### 3.1 Mode A — Opening / Towers (`osdbits/opening.c`, `osdbits/vucode_1.vsm`, `osdbits/towerchain.dsm`, `matching/src/anim.c`, `matching/src/fog.c`)

- **Camera & State Machine (`ProcessOpeningAnimation` `0x215fd0`) — Sourced & Matched (11/11 in `matching/src/anim.c`)**:
  - 8 state thresholds: `openingStateLevels = { 16, 56, 104, 320, 672, 800, 1160, 1160 }`.
  - Starts at `position = (0, 0, 16, 0)`, `fwdDir = (0, 0, 1, 1)`, `upDir = (0, 1, 0, 1)`, `rotation = -0.12 rad`, `positionSpeed.z = 0.04`, `rotationSpeed.z = 0.001`.
  - Frame timestep: `1.0` at NTSC 60 Hz (`640x224` non-interlaced field buffer, `screenAY = 0.457627`), `1.2` at PAL 50 Hz (`640x256`, `screenAY = 0.52627105`), `scrz = 1024.0`.
  - State 0 holds for `2 * fps` frames (`120` NTSC / `100` PAL), then accelerates forward:
    - No-disc / menu path (`osdBootParam` 106/107): `positionAccel2.z = 0.0099`, `rotationAccel.z = 0.000195`.
    - Disc / game-boot path (`osdBootParam` 108/109/110): `positionAccel1.z = 0.0004`, `rotationAccel.z = 0.00008`.
- **Tower Field & Memory Card History (`OpeningInitTowersFog` `0x218e00`, `HeightGrid` `0x217e30`, `vucode_1.vsm`) — Sourced**:
  - `14 x 9` tower slots placed inside a `20 x 20` cell/height grid (`histCells[21][6][2]`, `histSrcPos[14][9][3]`, `histTableC[14]`, `histTableD[14]`).
  - World position transform: `((src.x + 0.85) * 4.0, (src.y - 6.5) * 4.0, (src.z + 4.0) * 12.0 + 150.0)`, with `z += towerC * 30.0 - towerA` where `towerA = max(towerD * 30.0, 3.0)`.
  - `HeightGrid` (`sub_217e30`): two radial bumps (`R = sqrt(5202.0)`) plus deterministic integer hash `-10.0 * (((r*(r+c)/(c+1)) % 11) - 5)`, clamped to `[32, 220]` per-tower brightness.
  - Geometry (`towerchain.dsm` + `vucode_1.vsm`): 6 textured `TRISTRIP` quads (`4 x 4` X/Y footprint `[-2..2]`, half-height `f0 = towerA[r][c]` along Z), top cap + 4 side walls + bottom face, directional lighting via `normalLightMatrix` (`light1 = (0,-1,0)`, `light2 = (0.5,0.5,0)`, `light3 = (-0.5,-0.5,0)` rotated by `-towerSway` unless `towerC == 1.0`), near-to-far Gouraud black fade on side bottoms, and `CLAMP_1` horizontal streak UV behaviour on tower caps (`u = a0 / 256.0`).
  - `SimulateBootHistory`: deterministic procedural memory-card history generator (`ngames`, `maxcount`, or `ngames == 0` full-field test mode) alongside optional captured retail 21-slot history.
- **Half-Width Radial Glow Feedback (`DrawExtraBuf2(80)` + `DrawToExtraBuf2`) — Sourced (`matching/src/blit.c`)**:
  - Previous frame's scene is captured at half horizontal width (`320 x H`) and additively stretched (`4` radial zoom passes, `x = 1 + i*4`, `y = x*2`, alpha `(80 >> i)`) over the freshly drawn towers before capturing again, creating a persistent temporal radial bloom.
- **6-Layer Scrolling Cloud Fog (`InitFog` `0x214f78`, `DrawFog` `0x215238`) — Sourced / Matched (`matching/src/fog.c`)**:
  - `17 x 17` vertex grid (`x, y in [-48..+48]` step `6`, subtle `(frameCount*51)&0x3fff` sine offset noted in `matching/src/fog.c`), radial blue vertex color falloff centered at `(-5.1, 0)`, drawn across 6 depth planes `z = 134 - l*5` (`l = 0..5`) with scrolling `fogAnimation[l]` UV windows and alpha `20/128`.
- **4 Colored Light Orbs & 128-Point Trails (`DrawLights` `0x216b28`) — Reconstructed / Partially Approximate**:
  - Active while `position.z < 73.0` (`DrawLightsAndCubes` gate).
  - 4 colored orbs (`{120,255,40}`, `{255,90,220}`, `{255,40,30}`, `{180,60,255}`) driven by 4 lagged world-matrix snapshots and `osdRand()` (1999 newlib LCG `seed * 1103515245 + 12345`) jitter, each trailing a 128-sample additive `LINESTRIP` ribbon + `TEXOLIGT` halo sprite.
- **5 Refractive Glass Cubes (`InitCubes`, `DrawCube` `0x217520`) — Sourced**:
  - Active while `position.z < 73.0`. Five deterministic tracks seeded from `cubeSeedTable = { 1, 2, 4, 5, 9 }`, half-extent `1.8`, drifting from `z ~ 103..123` toward `z = 16` with Euler spin.
  - Per-cube screen capture (`CubeCaptureBuffer`) + 10-pass multi-layer glass pipeline (`cubePasses[10]`): back faces refract captured background + scrolling `BLPR`/`REF` layers into offscreen work buffer; front faces refract work buffer (`-0.084` inward zoom + projected normal displacement `0.15`) onto screen, then layer `BLPR`, `REF`, `BLP`, `REF` with Fresnel edge brightening `e = (1 - |n·v|)^2 * 0.5`.
- **Fly-Up Motion Blur, Banner Overlay & Black Fades (`sub_218b20`, `DoSCEText`, `sub_218bd0`) — Sourced**:
  - Motion blur (`z > 56`): `1..3` additive zoom-stretch passes of current screen (`alpha = 64 >> i`).
  - Banner overlay (`DoSCEText`): arms at `z > 18`, fades in/out (`0..240..0` step `4`, clamped to `112`). **Legal policy**: default clean-room mode displays a clearly non-Sony, nostalgia-faithful geometric/typographic banner (`OSDBITS // SYSTEM`), never Sony trademarks, unless the user explicitly imports their own authorized BIOS `TEXOSCE` texture.
  - Black fades: full black for frames `0..1`, then fade-to-black `(z - 72) * 4` reaching full black before `z = 104 / 320`.

### 3.2 Mode B — Indefinite Idle (`MODE_IDLE` in `osdbits/opening.c`)

- Identical tower/glow/fog/lights/cubes pipeline as Mode A, except `positionSpeed.z = 0.0` so the camera hovers indefinitely at `z = 16.0` while gently rolling (`rotationSpeed.z = 0.001`), keeping `z < 18` (no banner overlay, no fly-up, endless ambient drift).

### 3.3 Mode C — Illegal-Disc / Red Screen (`openingType = 1` in `osdbits/opening.c`, `matching/src/flare.c`, `matching/src/fog.c`)

- **Camera**: Starts at `position = (0, 0, 672)`, `fwdDir = (0, -0.03, 1, 1)`, `openingState = 4`, `positionSpeed.z = 2.1599939`, `positionAccel2.z = -0.0178`, `rotationSpeed.z = 0.00462` — climbs smoothly from `z = 672` and brakes to rest just past `z = 800` where state 6 zeros velocity/acceleration.
- **Visual layers**:
  1. `DrawRedFlare` (`0x21b4c8`, matched byte-exact in `matching/src/flare.c`): 7 spinning 16-gon `TRIFAN` glow discs (`flareRingSmall` + `flareRingLarge`) orbiting `(0, 0, 1160)` on a 201-frame cycle + 5 `TEXOFLAR` lens flare sprites (`FlareThing`), amplified by `DrawExtraBuf2(112)` + `DrawToExtraBuf2()`.
  2. `DrawIllegalFog` (`0x215a20`): 128 falling `3 x 3` red cloud patches (`30 x 30` units, speed `trunc((i+1)*0.02 + 1.2)`), respawning `805` units above camera.
  3. `DrawIllegalCubes` (`0x219ea8`): 5 red-tinted refractive glass cubes (`half = 1.2`, `illegalCubeSeedTable = { 1, 5, 19, 25, 34 }`, `illegalCubePasses` `colorMode = 4` attenuating G/B by `1/3`, zoom `0.8`).
  4. `DoIllegalText` + `DrawIllegalFades`: localized error message fades in at `+1/frame` (clamped at `112`) once `z > 800`; screen fades in from black across `z = 672..800`.
- **Explicitly Unverified Detail**: Exact `osdBootParam` value written by the disc-check thread on real hardware illegal-disc boot is marked unverified in `osdbits/opening.c:4262` (`101` is used so state 6 idles forever without fading out, matching retail GS dump behavior).

### 3.4 Mode D — Main Menu & System Configuration (`osdbits/menu.c`, `menuback.c`, `menuconfig.c`, `menutext.c`, `pad.c`, `sound.c`)

- **Reconstructed from Module U (`0x21C910..0x230440`)**:
  - **Camera**: `scrz = 512.0`, `menuScreenAY = 0.47` (NTSC) / `0.5405` (PAL), `pos = (10.436, 0, -103 + camZOffset)` where `camZOffset` starts at `-100` and eases by `* 0.97` per frame; fixed Euler `(0.031, 0.145, 0) rad`.
  - **7 Clock-Driven Orbs (`UpdateOrbs` `0x2261B8`, `DrawOrbs`)**: Seconds (`s*6 deg`) and minutes (`m*6 + s*0.1 deg`) drive `orbSpin`; 7 orbs with `50 x 3 = 150`-subframe `TrailPoint` ring buffers, non-linear `(f^4, f^2, f)` warm-white-to-deep-blue decay, `TEXCBLUR` outer halo + `TEXCNAVI` bright core, entry scatter fly-in, and Clock-Adjustment orb sink (`orbSpread -> 0`, `trailFade -> 0`, `clockHandOrb` spinning on a `radius = 12` circle).
  - **System Configuration Backdrop (`MenuBackdrop` `0x21D0A0`)**: 16-ribbon x 33-ring breathing `TEXCKABE` cylinder tunnel (`R = 6000`), copied to `wb3`/`wb4`, tinted `{0x37, 0x28, 0x3C} / 128`, framed by 16-segment elliptical `DrawVignette` (`0x22AF10`).
  - **12 Clock Crystal Rods & 5 Item Glass Cubes (`menuconfig.c`)**:
    - 90-frame transition choreography (`cfgAnim = 40 + 40 + 10`).
    - 12 hexagonal bevelled rods (`height = 26`, `radius = 2.6`, bevel `2.34` at `y = 26..26.39`) orbiting `(-22.5 .. -10.75, 0, 47.5)`, front rod dynamically split at `1 - min/60`, rendered with 5-pass clean `wb3` refraction + `SceneFlush` two-walk `TEXCFLOW` + `TEXCBUMP`/`TEXCBINV` emboss sparkle.
    - 5 item glass cubes (`half = 2.64`, identity `cubeCamera`, crack-free `w=0` projection), rendered with 8-pass tinted refraction + `TEXCBUMP` emboss + `TEXCREFA` masked reflection, cyan cursor ease `{0, 150, 200}`, and `-0.1` confirm scale kick.
  - **5-Pass `MenuZoomBlur` (`0x22A220`)**: Screen-wide bilinear shrink/stretch (`inset = 4*(i+1)`, `alpha = {0x60, 0x58, 0x50, 0x48, 0x40}`) applied to the 3D scene before crisp 2D UI text.
  - **2D OSD Chrome (`menutext.c`)**: Main Menu (`Browser`, `System Configuration`), System Configuration (`Clock Adjustment` full Y/M/D h:m:s editor, `Screen Size`, `Digital Out (Optical)`, `Component Video Out`, `Language`), top date/time bar, letterbox bars, button hints, and region confirm/cancel mapping.
- **Stubbed / Unreconstructed in `osdbits/` (to be clearly labeled in `osdbits-wasm`)**:
  - Memory Card / Disc `Browser` icon grid (`MenuSelectItem(0)` is a no-op in `osdbits/menu.c`; in `osdbits-wasm`, selecting Browser or custom menu entries will clearly indicate browser-shell customization/approximation rather than claiming retail `mcman` icon-grid reconstruction).
  - `Version Information` and `Options` sub-screens (stubbed in `osdbits`; if exposed in Mode D/E, explicitly labeled as clean-room browser/inspector panels).
  - Retail `FNTASCII`/`FNTEXOSD`/`FONTM` bitmap font tables and `MENUGEOM.inc` vertex tables require BIOS extraction in `osdbits`; `osdbits-wasm` will synthesize clean-room procedural equivalents out-of-the-box (`cubeModel` half-extent `2.64`, `rodModel` bevelled hexagonal prism from the exact dimensions in `menuconfig.c`, clean-room UI font/icons) while optionally accepting user-imported `extract-res.py` / BIOS containers.

---

## 4. PS2 GS / VU Hardware Semantics vs. WebGPU Translation Rules

1. **PS2 GS Color Scale (`0x80 = 1.0`) & Overbright Modulation**:
   - On PS2 GS, `MODULATE` computes `out = clamp(floor(tex * vtx / 128), 0, 255)`, allowing `vtx` or `tex` values above `128` to brighten up to `2.0x` before clamping to 8-bit `UNORM`. Every WGSL fragment shader must explicitly divide product colors by `(128.0 / 255.0)` (i.e. multiply normalized `[0..1]` colors by `255.0 / 128.0 = 1.9921875` where vertex colors come from `0..255` GS integers) and clamp to `[0.0, 1.0]` before blend, matching GS hardware saturation.
2. **PS2 GS Alpha Blend Unit `((A - B) * C >> 7) + D`**:
   - Common modes used across `osdbits`:
     - `(Cs - Cd) * As + Cd` (`0, 1, 0, 1`): Standard source-alpha lerp where `As = 128 (0x80)` means `1.0` opacity! Notice WebGPU standard `src-alpha` treats `1.0` (`255`) as opaque, whereas GS `As` in `[0..255]` scales by `As / 128.0` (capable of `>1.0` extrapolation if `As > 128`, though `osdbits` alphas are in `0..128`). Scaling shader output `alpha = gs_alpha / 128.0` makes standard WebGPU blend factors exact for `gs_alpha <= 128`!
     - `(Cs - 0) * As + Cd` (`0, 2, 0, 1`): Source-alpha-weighted additive blend (`src-alpha` + `one`).
     - `(Cs - 0) * FIX + Cd` (`0, 2, 2, 1`, `FIX = c`): Fixed-factor additive blend (`constant` + `one` with `blendConstant = c / 128.0`, or pre-multiplying `rgb * (c / 128.0)` with `one` + `one`!). Pre-multiplying or `blendConstant` avoids extra pipeline permutations while remaining bit-faithful to `c / 128.0`.
     - `(Cs - Cd) * FIX + Cd` (`0, 1, 2, 1`, `FIX = c`): Fixed-factor linear interpolation (`c / 128.0`).
     - `(0 - Cs) * As + Cd` (`2, 0, 0, 1`): Subtractive blend (`TEXCBINV` dark emboss pass in `SceneFlush` -> WebGPU `reverse-subtract`!).
3. **Projection, Aspect Ratio & Reverse Depth**:
   - PS2 `sceVu0ViewScreenMatrix(scrz, ax, ay, cx, cy, zmin, zmax, nearz, farz)` maps view-space `(x, y, z)` (`+X` right, `+Y` down on GS window after positive `scrz` + `+ay`, `+Z` into screen!) to `x_gs = ax * scrz * x / z + cx`, `y_gs = ay * scrz * y / z + cy`, and reverse hyperbolic `1/z` depth (`GEQUAL`).
   - WebGPU NDC has `+X` right, `+Y` up, `z in [0, 1]`. Converting GS screen coordinates `(x_gs, y_gs)` relative to display center `(2048, 2048)` and half-dimensions `(screenW/2, screenH/2)` yields exact NDC `x_ndc = (x_gs - 2048) / (screenW / 2)`, `y_ndc = -(y_gs - 2048) / (screenH / 2)`, with reverse-Z `z_ndc = nearz / z` (`compare: 'greater-equal'`, depth clear `0.0`), preserving exact PS2 aspect ratios (`4:3` display of `640x224` NTSC with `screenAY = 0.457627` / `0.47` and `640x256` PAL with `screenAY = 0.52627105` / `0.5405`, plus widescreen/pillarbox/fullscreen scaling modes).
4. **Refraction & Feedback Render Targets**:
   - WebGPU forbids reading and writing the same texture view in a single render pass. We map PS2 GS VRAM (`fb0/fb1`, `ExtraBuf2`, `CubeCaptureBuffer`, `wb3`, `wb4`) cleanly to explicit ping-pong `GPUTexture` targets (`sceneColorA/B`, `extraBufA/B`, `cubeCapture`, `workBuf3`, `workBuf4`), reproducing per-cube background capture, two-layer back+front refraction, half-width radial glow feedback, `SceneFlush` emboss bloom, and `MenuZoomBlur` without hazards.

---

## 5. Sandbox Environment & Toolchain Capabilities Verified in Phase 0

- **Host OS & CPU**: Linux `6.1.158` `x86_64` (2 vCPUs, 3.8 GiB RAM, ~20 GB free disk), Node.js `v22.22.3`, npm `10.9.8`, Python `3.11.2`, GCC/Make.
- **Outbound Network Allowlist**: `github.com`, `codeload.github.com`, `api.github.com`, `registry.npmjs.org`, `pypi.org`, `files.pythonhosted.org` (`crates.io`, `static.rust-lang.org`, and `cdn.playwright.dev` are unreachable).
- **Reproducible Offline-Friendly Toolchain Strategy**:
  - **Rust 1.88.0 (`rustc`, `cargo`, `rust-std` for `x86_64-unknown-linux-gnu` and `wasm32-unknown-unknown`, `rustfmt`, `clippy`)**: Installed locally in `node_modules` via `@rustbin/*` npm packages from `registry.npmjs.org`, wrapped by local `scripts/cargo.mjs` / `scripts/rustc.mjs` helpers. Rust crates use zero external `crates.io` dependencies for deterministic offline builds and minimal WASM footprint.
  - **WGSL Shader Validation**: `naga-wasm` and `wgsl_reflect` from `registry.npmjs.org` validate every WGSL module during unit/CI testing even outside a GPU context.
  - **Headless Browser + WebGPU Adapter**: `@sparticuz/chromium@153.0.0` from `registry.npmjs.org` bundles Chromium 153 + Vulkan SwiftShader (`swiftshader.tar.br`) with Playwright (`@playwright/test`). Because `/dev/dri` hardware GPU nodes are absent in this Linux container, browser WebGPU integration, visual regression, and E2E suites run against Chromium + Vulkan SwiftShader fallback adapter (`--enable-unsafe-webgpu`, `--use-angle=swiftshader` / Vulkan SwiftShader ICD), and all reports/tasks explicitly distinguish fallback-adapter verification from physical discrete/integrated GPU hardware validation.
