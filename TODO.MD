# `osdbits-wasm` Master Implementation & Verification Plan (`TODO.MD`)

Status legend:
- `[ ]` Not yet completed
- `[x]` Completed — acceptance criteria met and required tests executed and passing
- `[!]` Blocked or partial due to documented environmental/hardware limitation

Reporting status vocabulary used alongside checkboxes: `Implemented`, `Tested`, `Verified`, `Approximate`, `Blocked`, `Deferred`.

---

## Phase 0 — Repository Reconnaissance, Baseline Audit & Master Plan

### [x] `P00-001` — Complete Read-Only Audit of Existing `osdbits` Repository (`Verified`)
- **Objective**: Inspect all existing C/VU/DMA sources (`osdbits/`), decompilation matching sources (`matching/`), VU microcode (`vucode_*`), extraction tools (`tools/`, `ps2expand.c`), and reverse-engineering docs (`docs/`) before modifying any file.
- **Acceptance Criteria**:
  - Every source file in `osdbits/`, key matched TUs in `matching/src/`, VU microprograms, and documentation files inspected.
  - Original historical files remain 100% unmodified (`git diff ea0c687 -- osdbits matching docs/OSDSYS-STRUCTURE-REPORT.txt tools ps2expand.c` is empty).
- **Required Tests**: `git status` and `git diff ea0c687` check confirming zero modifications to historical PS2 files.
- **Dependencies**: None.

### [x] `P00-002` — Audit Sandbox Toolchain, Network Boundaries & Browser/WebGPU Capabilities (`Verified`)
- **Objective**: Determine exact compiler, bundler, headless browser, and WebGPU adapter availability under the sandbox network allowlist without relying on unavailable external CDNs.
- **Acceptance Criteria**:
  - Verified local availability of Node.js `v22.22.3`, Python `3.11.2`, GCC, `@rustbin/*` Rust `1.88.0` (`x86_64-unknown-linux-gnu` + `wasm32-unknown-unknown` + `clippy` + `rustfmt`), `naga-wasm`, and `@sparticuz/chromium@153.0.0` (with bundled `swiftshader.tar.br`).
  - Documented absence of physical `/dev/dri` GPU hardware so SwiftShader Vulkan fallback-adapter WebGPU execution is reported transparently and never misrepresented as discrete GPU hardware validation.
- **Required Tests**: Registry tarball metadata inspection and `/dev/dri` environment probe.
- **Dependencies**: `P00-001`.

### [x] `P00-003` — Author Phase 0 Baseline & Hardware Semantics Report (`Verified`)
- **Objective**: Document buildability, subsystem fidelity (`Sourced` vs. `Approximate` vs. `Stubbed`), PS2 GS/VU blending/projection/refraction semantics, and BIOS/legal boundaries in `docs/phase0-baseline.md`.
- **Acceptance Criteria**:
  - `docs/phase0-baseline.md` written and cross-referenced against `osdbits/*.c`, `vucode_1.vsm`, and `matching/src/*.c`.
  - Explicitly distinguishes retail-matched logic from reconstructed approximations and stubbed screens (`Browser` icon grid, `Version`, `Options`, `osdBootParam` on illegal-disc boot).
- **Required Tests**: File existence, markdown link/section check, and clean diff check against historical directories.
- **Dependencies**: `P00-001`, `P00-002`.

### [x] `P00-004` — Create Root `TODO.MD` Covering Phases 0–11 (`Verified`)
- **Objective**: Establish root `/TODO.MD` with unique task IDs (`P00-*` .. `P11-*`), objectives, acceptance criteria, required tests, dependencies, deferred features, and known limitations.
- **Acceptance Criteria**:
  - `/TODO.MD` exists at repo root with Phases 0–11, unique IDs, no duplicate IDs, and dedicated Deferred Features & Known Limitations sections.
- **Required Tests**: Structural validation check over `TODO.MD` verifying unique IDs and required fields per task.
- **Dependencies**: `P00-003`.

---

## Phase 1 — Reproducible Toolchain, Workspace Layout & Legal Guardrails

### [x] `P01-001` — Bootstrap npm Workspace, TypeScript (`strict: true`), Vite & Vitest (`Verified`)
- **Objective**: Create root `package.json`, `tsconfig.json` (`strict: true`, `noUncheckedIndexedAccess: true`, `noImplicitOverride: true`), `vite.config.ts`, and `vitest.config.ts` while keeping historical `osdbits/` untouched.
- **Acceptance Criteria**:
  - `npm install` completes cleanly using only `registry.npmjs.org` with zero external CDN postinstall downloads.
  - `npm run typecheck` (`tsc --noEmit`), `npm run test:unit` (`vitest run`), and `npm run build:web` succeed on a minimal bootstrap module.
- **Required Tests**: `npm run typecheck`, `npm run test:unit`, `npm run build:web`.
- **Dependencies**: `P00-004`.

### [x] `P01-002` — Configure Local Rust `1.88.0` + `wasm32-unknown-unknown` Toolchain Wrapper (`Verified`)
- **Objective**: Wire `@rustbin/*` (`rustc`, `cargo`, `rust-std` host + `wasm32-unknown-unknown`, `rustfmt`, `clippy`) through deterministic `scripts/rust-tool.mjs` wrappers and Cargo workspace `Cargo.toml`.
- **Acceptance Criteria**:
  - `npm run test:rust` (`cargo test --workspace`), `npm run lint:rust` (`cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check`), and `npm run build:wasm` (`cargo build --target wasm32-unknown-unknown --release`) run cleanly offline after `npm install`.
  - Generated `.wasm` artifact exports deterministic version/ABI handshake symbols and loads in Node/Vitest.
- **Required Tests**: `npm run test:rust`, `npm run lint:rust`, `npm run build:wasm`, and a Vitest WASM smoke test.
- **Dependencies**: `P01-001`.

### [x] `P01-003` — Configure WGSL Offline Shader Validation Pipeline (`naga-wasm`) (`Verified`)
- **Objective**: Create a shader validation harness using `naga-wasm` so every `.wgsl` shader in `web/src/renderer/shaders/` is parsed, type-checked, and entry-point-verified in `vitest` and CI even without a GPU.
- **Acceptance Criteria**:
  - Valid WGSL shaders pass `naga-wasm` validation; intentionally malformed WGSL fixtures are rejected with clear diagnostics.
- **Required Tests**: Vitest suite `tests/unit/phase1-toolchain.test.ts`.
- **Dependencies**: `P01-001`.

### [x] `P01-004` — Configure Headless Chromium + SwiftShader Vulkan WebGPU Harness for Playwright (`Verified`)
- **Objective**: Unpack and configure `@sparticuz/chromium` + bundled `swiftshader.tar.br` (`VK_ICD_FILENAMES` / WebGPU flags) in a local runtime cache and verify Playwright + WebGPU adapter acquisition.
- **Acceptance Criteria**:
  - Playwright launches headless Chromium offline without `npx playwright install`.
  - Probe test confirms DOM rendering, `IndexedDB`, `AudioContext` construction, and tests `navigator.gpu.requestAdapter()` (reporting adapter info / `isFallbackAdapter` accurately).
- **Required Tests**: `npm run test:e2e:smoke` via `@playwright/test`.
- **Dependencies**: `P01-001`.

### [x] `P01-005` — Legal / Proprietary BIOS Artifact & Provenance Guard Script (`Verified`)
- **Objective**: Implement `scripts/check-legal-artifacts.mjs` (plus tests) ensuring zero Sony BIOS dumps, `ROMDIR` containers, `osdbits/res/*.inc` extractions, or known BIOS hashes/signatures exist in tracked or staged files, and verifying historical `osdbits/` files remain intact.
- **Acceptance Criteria**:
  - Script fails on synthetic `ROMDIR` / `TEXIMAGE` / `osdbits/res/TEXOSCE_EXP.inc` test fixtures and passes on the repository tree.
  - `.gitignore` excludes generated WASM build outputs, `node_modules`, Playwright runtime artifacts, and user BIOS test scratch directories.
- **Required Tests**: Vitest unit tests on `check-legal-artifacts.mjs` + `npm run check:legal`.
- **Dependencies**: `P01-001`.

---

## Phase 2 — Deterministic Rust Core Simulation (`crates/osdbits-core`) — Opening, Idle & Illegal-Disc

### [x] `P02-001` — PS2 Math, Fixed/Float Matrix Pipeline & `osdRand` LCG (`Sourced`)
- **Objective**: Implement `Mat4`, `Vec4`, `sceVu0RotMatrix`, `sceVu0TransMatrix`, `sceVu0CameraMatrix`, `sceVu0ViewScreenMatrix`, `CreateWorldMatrix`, GS-to-WebGPU NDC/reverse-Z projection helpers, `osdSin`/`osdCos` quadrant LUT (`0x3fff` mask), and 1999 newlib `osdRand` (`seed * 1103515245 + 12345`) in `crates/osdbits-core`.
- **Acceptance Criteria**:
  - Bit/ULP-consistent behavior against `osdbits/opening.c` and `matching/src/anim.c` formulas across NTSC (`640x224`, `screenAY=0.457627`) and PAL (`640x256`, `screenAY=0.52627105`).
  - Zero `unsafe` in `osdbits-core`; deterministic across x86_64 and `wasm32`.
- **Required Tests**: Rust unit tests verifying exact `osdRand` sequence, LUT quadrant symmetry, camera/view-screen matrices, and projection bounds.
- **Dependencies**: `P01-002`.

### [x] `P02-002` — Memory Card History & `14x9` / `20x20` Tower Field Generator (`Sourced` + `Approximate` Simulation Option)
- **Objective**: Port `histCells[21][6][2]`, `histSrcPos[14][9][3]`, `histTableC[14]`, `histTableD[14]`, `HeightGrid` (`sub_217e30`), `OpeningInitTowersFog`, deterministic `SimulateBootHistory(seed, ngames, maxcount)`, and retail `realBootHistory` preset into `osdbits-core`.
- **Acceptance Criteria**:
  - `HeightGrid` matches `sub_217e30` (`[32.0, 220.0]` two-bump + integer hash) on all 400 cells.
  - Supports: (1) deterministic seeded game-count/max-playcount simulation, (2) full 126-tower field (`ngames=0`), (3) captured 21-slot retail history preset, and (4) user-customizable tower density/seed.
  - Emits exact per-tower instance descriptors (world position, half-height `towerA`, top cap blend `towerB`, max-flag `towerC`, `HeightGrid` brightness, `u = a0/256` UV offset, sway eligibility).
- **Required Tests**: Rust unit tests checking all 400 `HeightGrid` bounds/landmarks, sprawl threshold (`count >= 14` every 10 plays), eviction rules, and seed reproducibility.
- **Dependencies**: `P02-001`.

### [x] `P02-003` — VU1 Tower Geometry, Sway & Directional Lighting (`Sourced`)
- **Objective**: Port `vucode_1.vsm` + `towerchain.dsm` 6-quad tower mesh generation, `towerSway` (`sin(((frame%360)-180)*deg2rad)*10*deg2rad`), rotated `light1..3` `normalLightMatrix`, per-face Gouraud near-to-far black fade, and frustum visibility culling (`sceVu0ClipAll` semantics).
- **Acceptance Criteria**:
  - Maxed towers (`towerC == 1.0`) stand vertical without sway while non-maxed towers sway by `±10 deg` on a 360-frame cycle.
  - Lighting intensities match VU1 `vclip`/`vmul` column formulas (`1.0, 0.8, 0.8, 0.4` weights, clamped).
- **Required Tests**: Rust unit tests verifying vertex positions, normals, cap/side UV coordinates (including `CLAMP_1` smear parameters), sway suppression on `towerC == 1.0`, and culling invariants.
- **Dependencies**: `P02-002`.

### [x] `P02-004` — Opening & Idle Camera Timeline State Machine (`ProcessOpeningAnimation` `0x215fd0`) (`Sourced`)
- **Objective**: Implement the 8-state Opening camera state machine (`openingStateLevels`), NTSC 60 Hz (`dt=1.0`) and PAL 50 Hz (`dt=1.2`) stepping, Mode A (`boot` -> menu or game-boot variant) and Mode B (`idle` indefinite hover at `z=16`), plus user speed multiplier & reduced-motion controls.
- **Acceptance Criteria**:
  - State transitions `0 -> 1 -> 2 -> 3 -> 4 -> 5 -> 6` occur at exact `z` thresholds `{16, 56, 104, 320, 672, 800, 1160}`.
  - Mode B (`Idle`) maintains `position.z == 16.0` indefinitely with continuous roll and zero banner/fade-out triggers.
  - Mode A cleanly signals completion (`transitionToMenu` when `endFrame` fade finishes) so the shell can auto-transition to Mode D or loop per user preference.
- **Required Tests**: Rust unit tests running 1,000+ frames in NTSC/PAL Mode A and 10,000 frames in Mode B verifying monotonic `z` progression in Mode A and exact `z == 16.0` stability in Mode B.
- **Dependencies**: `P02-001`.

### [x] `P02-005` — Opening Fog (`InitFog` / `DrawFog`), Light Orbs/Trails (`DrawLights`) & Refractive Cubes (`DrawCube`) (`Sourced` / `Approximate`)
- **Objective**: Implement 6-layer `17x17` cloud fog state, 4 colored light orbs + 128-entry ring trails (`z < 73` gate), 5 refractive glass cubes (`cubeSeedTable`, drift/respawn, Euler spin, per-pass UV normal projection & Fresnel edge weights), banner fade (`DoSCEText`), motion-blur pass count (`sub_218b20`), and black fades (`sub_218bd0`).
- **Acceptance Criteria**:
  - Same seed + frame index produces bit-identical positions, trails, cube transforms, and pass descriptors.
  - `z >= 73.0` cleanly gates off lights and cubes exactly as in `DrawOpeningScene` (`0x218d80`).
- **Required Tests**: Rust unit tests for fog vertex colors/scroll wraps, 128-point light trail ring wraparound, cube respawn cycles, Fresnel edge `[0, 0.5]` bounds, and `DoSCEText` alpha curve.
- **Dependencies**: `P02-004`.

### [x] `P02-006` — Mode C Illegal-Disc Simulation (`RedFlare`, `IllegalFog`, `IllegalCubes`, Fades) (`Sourced` + Documented `osdBootParam` Note)
- **Objective**: Implement `openingType = 1` starting at `z = 672`, `vz = 2.1599939`, `az2 = -0.0178`, `fwdDir = (0,-0.03,1)`, 7 spinning 16-gon flare rings + 5 `FlareThing` sprites on the 201-frame cycle, 128 falling/respawning `3x3` red fog patches, 5 red glass cubes (`illegalCubeSeedTable`), and `DoIllegalText` / `DrawIllegalFades`.
- **Acceptance Criteria**:
  - Camera climbs from `z = 672` and brakes smoothly into state 6 (`z >= 800`), idling stably without drift or NaN over 10,000+ frames.
  - Localized error string selector supports English, French, Spanish, German, Italian, Dutch, Portuguese, and Japanese romanized/clean-room text (`osdbits/opening.c` / `menutext.c` tables).
- **Required Tests**: Rust unit tests verifying camera braking trajectory (`672 -> ~800`), 201-frame flare periodicity, 128-patch fog respawn bounds, and fade alpha curves.
- **Dependencies**: `P02-005`.

---

## Phase 3 — Module U Menu / System Configuration Simulation & WASM Bridge (`crates/osdbits-wasm`)

### [x] `P03-001` — Module U Clock, 7 Orbiting Orbs (`UpdateOrbs`), Scatter Fly-In & Backdrop Tunnel State (`Sourced`)
- **Objective**: Implement software RTC clock (`ClockTick` NTSC/PAL + explicit initial timestamp injection for determinism!), `UpdateOrbs` (`0x2261B8`) 7-orb kinematics, 150-subframe `TrailPoint` ring buffers with `(f^4, f^2, f)` RGB decay, entry scatter fly-in, Clock-Adjustment orb sink + `clockHandOrb`, and `MenuBackdrop` (`menuback.c`) breathing 16x33 tunnel parameters in `osdbits-core`.
- **Acceptance Criteria**:
  - Orb positions, trail rings, and clock angles are 100% deterministic for any injected initial `(year, month, day, hour, minute, second)` and frame count.
  - Clock rollover (`23:59:59 -> 00:00:00`, leap years `2000`, `2024`, `2026`, `2028`, month lengths) matches `menutext.c` calendar rules.
- **Required Tests**: Rust unit tests for calendar/leap-year math, 150-entry orb trail decay, scatter fly-in convergence, and Clock-Adjustment orb sink/restore transitions.
- **Dependencies**: `P02-001`.

### [x] `P03-002` — System Configuration State Machine, 12 Crystal Clock Rods & 5 Glass Item Cubes (`Sourced` + Clean-Room `MENUGEOM`)
- **Objective**: Implement `MenuEnterConfig`/`MenuLeaveConfig` 90-frame choreography (`cfgAnim`), clean-room procedural `rodModel` (16-face bevelled hexagonal prism `h=26, r=2.6 -> 2.34`) and `cubeModel` (6-face cube `half=2.64` + 5 `cubePositions`), dynamic front-rod split (`MeshDrawRodSplit` at `1 - min/60`), cursor cyan color ease `{0, 150, 200}`, confirm `-0.1` scale kick, and full navigation/editor state (`Main Menu`, `System Configuration`, `Clock Adjustment`, configurable custom menu items).
- **Acceptance Criteria**:
  - Works out-of-the-box with clean-room procedural `rodModel`/`cubeModel` AND accepts optional user-imported `MENUGEOM` vertex/face tables if provided.
  - Supports keyboard/pointer/gamepad-mapped actions (`Up`, `Down`, `Left`, `Right`, `Confirm`, `Cancel`, `Display`, `Options`) with configurable confirm/cancel swap.
  - Clearly tags reconstructed OSDSYS items vs. clean-room `osdbits-wasm` customization/browser entries in state metadata.
- **Required Tests**: Rust unit tests covering all menu state transitions, 90-frame `cfgAnim` enter/exit curves, `MeshDrawRodSplit` geometry invariants for `min = 0..59`, and Clock Adjustment editing.
- **Dependencies**: `P03-001`.

### [x] `P03-003` — Unified Render Command Buffer & Frame-State Inspector Snapshot (`Implemented` / `Tested`)
- **Objective**: Design a flat, zero-allocation-steady-state render packet/instance stream and structured `FrameInspection` telemetry struct in `osdbits-core` covering Modes A, B, C, and D (plus Mode E overrides) so Rust owns 100% of simulation/culling/pass-parameter math and WebGPU owns GPU rasterization/blending/targets.
- **Acceptance Criteria**:
  - Steady-state `tick()` reuses pre-allocated `Vec` buffers without unbounded heap growth.
  - `FrameInspection` exposes current mode, frame counter, seed, camera position/rotation, opening state, active tower/orb/cube/patch counts, fade levels, and audio event cues for headless tests and UI inspector.
- **Required Tests**: Rust unit tests verifying complete render-stream validity (finite floats, valid texture/pass enums, bounded vertex/instance counts) across all 4 visual modes.
- **Dependencies**: `P02-006`, `P03-002`.

### [x] `P03-004` — `crates/osdbits-wasm` C-ABI / TypedArray Bridge & TypeScript Loader
- **Objective**: Expose `osdbits-core` via `crates/osdbits-wasm` (`cdylib` for `wasm32-unknown-unknown`) with explicit pointer/length flat buffer views (`Float32Array` / `Uint32Array`) + JSON/binary config & inspection endpoints, wrapped by a strictly typed `WasmSimulationBridge` in TypeScript.
- **Acceptance Criteria**:
  - Zero `any` types in TypeScript bridge; validates ABI version, WASM memory bounds, and pointer alignments on every call.
  - Handles malformed inputs, invalid mode enums, and simulated corrupted/missing WASM bytes with structured typed errors rather than unhandled exceptions.
- **Required Tests**: Vitest WASM instantiation, ABI round-trip, multi-thousand-frame memory stability test, and corrupted-WASM error-handling test.
- **Dependencies**: `P03-003`.

---

## Phase 4 — Clean-Room Procedural Assets & Validated User Resource Importer

### [x] `P04-001` — Clean-Room Procedural Texture, Environment Map & Glyph Atlas Synthesizer
- **Objective**: Generate deterministic, copyright-clean default textures matching every required slot (`WAL0` mipmapped tower stone/grid surface, `FOG0..4` Perlin/FBM cloud layers, `LIGT` radial orb sprite, `BLP`/`BLPR`/`REF` glass caustic & environment maps, `FLAR` lens hexagonal/star flare, `KABE` tunnel weave, `BUMP`/`BINV`/`FLOW`/`REFA` crystal bump & caustic maps, `BLUR`/`NAVI` menu orb sprites, `MARU` clean-room geometric button icons, non-Sony `OSDBITS` opening banner, and clean-room UI glyph atlas).
- **Acceptance Criteria**:
  - 100% procedural in code (seeded, deterministic, zero embedded BIOS bytes, zero Sony logos/trademarks).
  - Produces exact dimensions, mip levels (6 levels for `WAL0`), and alpha semantics (`PSMCT16` `TEXA` equivalent) expected by the renderer.
- **Required Tests**: Unit tests verifying determinism, dimensions, mipmap chains, non-empty alpha/luminance distributions, and absence of NaN/out-of-range texels for every generated slot.
- **Dependencies**: `P01-001`.

### [x] `P04-002` — PS2 LZ `Expand` Decompressor & Validated User Resource Importer (Rust + TS)
- **Objective**: Port `ps2expand.c` LZ decompression + `ROMDIR` (`TEXIMAGE`, `FNTIMAGE`) and resource bundle validation into safe Rust/WASM (with strict size caps, bounds checks, and SHA-256 / FNV-1a integrity digests) so users who own authorized PS2 BIOS dumps or extracted resource bundles can optionally import them locally in the browser without network upload.
- **Acceptance Criteria**:
  - Safe Rust LZ `Expand` matches `ps2expand.c` output byte-for-byte on synthetic compressed test vectors and rejects truncated, cyclic, or zip-bomb payloads safely within a hard output cap.
  - Validates dimensions, pixel formats (`PSMCT32`, `PSMCT16`, `PSMT8`, `PSMT4`), and header magic; falls back cleanly to procedural assets if an imported resource is partial or invalid.
- **Required Tests**: Rust unit + fuzz/adversarial tests against `Expand` and `ROMDIR` parser (including cross-check against host-compiled `ps2expand.c` on synthetic payloads) + TypeScript importer tests.
- **Dependencies**: `P03-004`, `P04-001`.

---

## Phase 5 — WebGPU Renderer Core, WGSL Shaders & Modes A / B Pipeline

### [x] `P05-001` — Documented Single-Ownership Rendering Architecture & WebGPU Context Manager
- **Objective**: Implement `WebGpuRenderer` in `web/src/renderer/` with a single documented division of responsibility (Rust/WASM computes simulation + world/view/projection + sorted/culled geometry & pass descriptors; TypeScript/WebGPU owns GPU buffers, render targets, WGSL shaders, and GS-equivalent blend passes), including adapter/device negotiation, resize/DPR/aspect-ratio scaling (`4:3` authentic, `16:9`, `fit`), and `device.lost` recovery.
- **Acceptance Criteria**:
  - Gracefully detects missing `navigator.gpu` or null adapter and transitions shell to a clear diagnostic fallback panel without crashing.
  - Recovers cleanly from simulated `device.lost` / pipeline re-initialization without full page reload.
- **Required Tests**: Unit tests with mock/real WebGPU adapter + WGSL validation tests + browser WebGPU context lifecycle test.
- **Dependencies**: `P01-003`, `P03-004`, `P04-001`.

### [x] `P05-002` — PS2 GS Color/Blend WGSL Shader Library & Offscreen Ping-Pong Framebuffer Graph
- **Objective**: Author and validate all WGSL shaders (`tower.wgsl`, `fog_sprite.wgsl`, `line_trail.wgsl`, `glass_cube.wgsl`, `menu_rod_cube.wgsl`, `blit_post.wgsl`, `ui_overlay.wgsl`) implementing PS2 GS `0x80 = 1.0` overbright modulation, reverse-Z `[0, 1]` (`greater-equal`), `CLAMP_1` UV clamping, and explicit ping-pong render targets (`sceneColorA/B`, `extraBufA/B`, `cubeCapture`, `workBuf3`, `workBuf4`).
- **Acceptance Criteria**:
  - Every `.wgsl` file passes `naga-wasm` validation and `wgsl_reflect` bind-group layout checks.
  - Render target reallocation on canvas/internal-resolution change releases old `GPUTexture`s (`destroy()`) to prevent VRAM leaks.
- **Required Tests**: `naga-wasm` shader validation suite + target lifecycle unit tests verifying texture creation/destruction counts across repeated resizes.
- **Dependencies**: `P05-001`.

### [x] `P05-003` — Mode A (Opening / Towers) & Mode B (Indefinite Idle) Full Multi-Pass WebGPU Pipeline
- **Objective**: Wire exact `DrawOpeningScene` pass order in WebGPU: (1) `DrawTowers` (depth-tested, mipmapped `WAL0`, sway + 3-light Gouraud + `CLAMP_1` cap streaks), (2) `DrawExtraBuf2(80)` 4-pass additive radial zoom glow + `DrawToExtraBuf2` capture, (3) `DrawFog` 6-layer scrolling cloud grid, (4) `DrawLights` 4 colored orbs + 128-point trails (`z < 73`), (5) `DrawCube` 5 per-cube background-captured 10-pass refractive glass cubes (`z < 73`), (6) `sub_218b20` fly-up motion blur (`z > 56`), (7) `DoSCEText` legal-safe banner (`z > 18`), and (8) `sub_218bd0` fades + letterbox/aspect presentation.
- **Acceptance Criteria**:
  - Renders non-blank, depth-accurate, temporally glowing tower fields, refractive cubes, fog, and light trails in headless Chromium WebGPU.
  - Mode A progresses through drift -> banner -> fly-up -> fade; Mode B idles smoothly at `z = 16`.
- **Required Tests**: Browser WebGPU integration tests reading back rendered pixels at deterministic seeds/frames in Mode A (frames `30`, `140`, `220`) and Mode B (frame `300`) to verify scene luminance, color channels, non-zero depth/tower coverage, and fade transitions.
- **Dependencies**: `P05-002`.

---

## Phase 6 — Mode C (Illegal-Disc) & Mode D (Main Menu / System Configuration) WebGPU Pipelines

### [x] `P06-001` — Mode C (Illegal-Disc / Red Screen) WebGPU Pipeline (`Sourced` + Documented Note)
- **Objective**: Wire `DrawIllegalScene` in WebGPU: (1) `DrawRedFlare` 7 spinning 16-gon glow fans + 5 `FLAR` additive lens sprites, (2) `DrawExtraBuf2(112)` + `DrawToExtraBuf2()`, (3) `DrawIllegalFog` 128 falling red cloud patches, (4) `DrawIllegalCubes` 5 red-tinted refractive glass cubes, and (5) localized error text overlay + black fade-in (`z = 672..800`).
- **Acceptance Criteria**:
  - Produces dominant warm-red atmospheric glow, refractive red glass cubes, and crisp localized text past `z > 800`.
  - UI/inspector explicitly notes that retail `osdBootParam` on hardware illegal-disc boot is unverified (`osdbits/opening.c:4262`).
- **Required Tests**: Browser WebGPU integration test verifying red-dominant channel distribution (`R > G`, `R > B`), fade-in progression between frame `5` and frame `180`, and localized text overlay presence.
- **Dependencies**: `P05-003`.

### [x] `P06-002` — Mode D Main Menu & System Configuration WebGPU Pipeline (`Orbs`, `Backdrop`, `Clock Rods`, `Glass Cubes`, `ZoomBlur`, `2D UI`)
- **Objective**: Wire Module U rendering in WebGPU:
  - 7 clock-driven `Orbs` + 150-point additive trails + `BLUR`/`NAVI` sprites (drawn to both main scene and `wb3`).
  - `MenuBackdrop` 16x33 breathing `KABE` tunnel + `{0x37,0x28,0x3C}` purple-blue screen tint + `DrawVignette`.
  - 12 hexagonal crystal clock rods (`MeshDrawRodSplit` on front rod + 5-pass clean `wb3` refraction + `SceneFlush` `FLOW`/`BUMP`/`BINV` emboss sparkle).
  - 5 camera-space glass item cubes (8-pass tinted refraction + `BUMP` emboss + `REFA` masked reflection + cyan selection ease + confirm kick).
  - 5-pass `MenuZoomBlur` on 3D layer followed by crisp 2D OSD typography, top clock bar, and bottom button hints.
- **Acceptance Criteria**:
  - Main Menu (`Browser`, `System Configuration` + clearly marked `osdbits-wasm` quick-scene/customization entries) and System Configuration (`Clock Adjustment`, `Screen Size`, `Digital Out`, `Component Video Out`, `Language`) render and respond interactively via keyboard and pointer.
- **Required Tests**: Browser WebGPU integration tests covering Main Menu orb rendering, transition into System Configuration (verifying backdrop + crystal rods + glass cubes), and Clock Adjustment interaction.
- **Dependencies**: `P05-003`.

---

## Phase 7 — Browser Persistent Storage (`IndexedDB` + `OPFS`), Migrations & Read-Back Verification

### [x] `P07-001` — Versioned `IndexedDB` + Optional `OPFS` Persistence Engine with Read-Back Verification
- **Objective**: Implement `StorageManager` (`web/src/storage/`) with schema versioning (`v1` -> `v2` migration path tested!), atomic IndexedDB transactions, monotonic revision counter + `BroadcastChannel` multi-tab coordination to prevent stale overwrites, mandatory post-write read-back verification (`writeAndVerify`), SHA-256 integrity checks for imported binary assets (stored in OPFS when available with automatic IndexedDB chunk fallback), JSON/bundle export & import, and confirmed factory reset.
- **Acceptance Criteria**:
  - Every settings or asset save reads back the committed record in a fresh read transaction and verifies structural + digest equality before resolving.
  - Handles corrupt payloads, schema upgrades, quota exceeded errors, and unavailable IndexedDB/private-mode restrictions gracefully by falling back to in-memory session storage with clear non-blocking user notification.
- **Required Tests**:
  - Vitest unit tests (`fake-indexeddb`) covering `v1 -> v2` migrations, corrupt record recovery, stale-revision conflict rejection, quota error simulation, export/import round-trips, and reset confirmation.
  - Real Chromium Playwright persistence tests verifying (a) reload/reopen within same userDataDir AND (b) full browser process termination and relaunch against a persistent `userDataDir` on disk!
- **Dependencies**: `P01-004`, `P04-002`.

---

## Phase 8 — Browser-Safe Web Audio Soundscape & Single-Click Activation

### [x] `P08-001` — Clean-Room Procedural PS2-Inspired Ambient Soundscape & UI Foley (`Web Audio API`)
- **Objective**: Implement `AudioEngine` (`web/src/audio/`) generating an original, copyright-clean atmospheric ambient drone/chime/wind soundscape (inspired by early-2000s ambient synthesis, containing zero Sony `SNDIMAGE` samples unless explicitly imported by the user) + subtle menu cursor/confirm/cancel/error tones, master/ambient/SFX volume controls, mute toggle, and reduced-sensory support.
- **Acceptance Criteria**:
  - Zero audio nodes emit sound and no autoplay error is thrown before user activation.
  - `audioEngine.activateFromUserGesture()` calls `AudioContext.resume()` **synchronously inside the user's single `Start OSDSYS` click/keydown handler** before any `await`, and the app proceeds directly into the selected scene with no second click required.
  - Automatically suspends/ramps smoothly on `visibilitychange` (hidden) and resumes cleanly when visible (unless muted), and degrades gracefully if `AudioContext` is missing or blocked.
- **Required Tests**: Unit tests with Web Audio mock verifying synchronous `resume()` inside gesture handler + Playwright E2E tests verifying silent pre-activation state, single-click startup into live scene + running `AudioContext`, mute persistence, and visibility-change suspend/resume.
- **Dependencies**: `P01-004`.

---

## Phase 9 — PS2-Aesthetic Browser Shell, Mode E Customization, Accessibility & Diagnostics

### [x] `P09-001` — Minimalist PS2-Inspired Browser Shell, Single-Click Launch & Mode E Customization Drawer
- **Objective**: Build the complete accessible TypeScript UI shell (`web/src/ui/`):
  - Clean pre-activation landing overlay with **Start OSDSYS** primary CTA (keyboard + pointer accessible), mode selector, capability badge, and zero clutter.
  - Mode switcher & customization drawer (**Mode E**): Scene (`Opening`, `Idle`, `Illegal Disc`, `Menu / Config`), NTSC `60Hz` vs. PAL `50Hz` timing/framing, aspect mode (`4:3 Authentic`, `16:9 Widescreen`, `Stretch`), internal resolution scale (`0.5x`..`2.0x`), camera speed / manual orbit offset, memory-card tower count & max height & deterministic PRNG seed, fog/glow/refraction/motion-blur toggles, reduced-motion mode, audio mixer, optional local BIOS/resource import + integrity status, JSON settings export/import, and two-step confirmed **Reset to Faithful Defaults**.
  - Live Collapsible Telemetry / Provenance Inspector showing FPS, p50/p95 frame time, WebGPU adapter status (`fallback` vs. hardware), active WASM simulation state, and parameter provenance (`Sourced` / `Approximate` / `Unverified` badges per mode).
  - Non-destructive runtime error boundary with **Recover Scene** button (re-initializes WASM + WebGPU without full page reload).
- **Acceptance Criteria**:
  - Full keyboard navigation (`Tab`, `Enter`/`Space`, `Escape`, arrow keys, shortcut keys) with visible `:focus-visible` rings, ARIA roles/labels, and `prefers-reduced-motion` awareness.
  - Every customization persists automatically with read-back verification and **Reset to Defaults** restores exact faithful OSDSYS parameters.
  - Mode D controls mirror the core's input gate (`MenuSim::input_ready`, `menu.c:0x227D08`) with the `disabled` attribute, so rows fading in during the 90-frame System Configuration choreography can never swallow a pointer click, and menu rows are updated in place instead of being re-created every clock tick.
- **Required Tests**: Vitest UI state/accessibility tests + Playwright E2E flows testing single-click startup, keyboard-only navigation, Mode A/B/C/D/E switching, customization persistence across full browser restart, asset validation error handling, scene recovery without reload, and Mode D input-readiness gating (`tests/e2e/menu-input-readiness.spec.ts`).
- **Dependencies**: `P06-001`, `P06-002`, `P07-001`, `P08-001`.

---

## Phase 10 — Comprehensive Testing: Property/Fuzz, Visual Regression, E2E & Performance/Leak Suites

### [x] `P10-001` — Rust Property-Based & Adversarial Fuzz Test Suite (`10,000+` Randomized Cases)
- **Objective**: Build a deterministic, seed-reproducible property & fuzz runner in Rust testing arbitrary seeds, extreme frame counts, rapid mode switches, calendar boundary dates, arbitrary `(ngames, maxcount)` inputs, malformed/corrupted LZ `Expand` & `ROMDIR` byte streams, and camera acceleration edge cases.
- **Acceptance Criteria**:
  - Zero panics, zero integer overflows, zero NaN/Inf render coordinates across `10,000+` generated cases.
  - Any failure prints minimal shrunk seed/input for deterministic replay.
- **Required Tests**: `npm run test:rust` (including `osdbits-core` property & fuzz suites).
- **Dependencies**: `P04-002`.

### [x] `P10-002` — Deterministic Visual Regression Golden Suite Across Modes A, B, C, D
- **Objective**: Capture and verify deterministic fixed-seed, fixed-clock, fixed-resolution (`640x480`) WebGPU frames across:
  - Mode A Opening (early drift frame `24`, banner + cubes frame `90`, fly-up blur frame `215`)
  - Mode B Idle (frame `180`)
  - Mode C Illegal Disc (red flare + cubes + message frame `160`)
  - Mode D Main Menu (clock `12:15:30`, frame `120`) and System Configuration (rods + cubes, frame `120`)
- **Acceptance Criteria**:
  - Check-in canonical PNG baselines + per-pixel RMSE / max-channel-delta / structural histogram assertions with explicitly documented SwiftShader tolerances (`maxMeanChannelDiff <= 1.5 / 255`, `mismatchRatio(>4/255) <= 0.2%`).
  - Verified repeatable across consecutive clean headless runs.
- **Required Tests**: `npm run test:visual` via Playwright.
- **Dependencies**: `P09-001`.

### [x] `P10-003` — End-to-End Browser Suite, Unsupported-WebGPU Fallback & Endurance / Memory-Leak Benchmarks
- **Objective**: Implement full Playwright E2E suite (`tests/e2e/`) covering:
  - Single-click **Start OSDSYS** + synchronous `AudioContext.resume()` verification.
  - Mode A -> D automatic opening-to-menu handoff, Mode B, Mode C, Mode D clock/config editing, Mode E customization + export/import + reset.
  - Real persistent `userDataDir` full browser process close & relaunch verification.
  - Simulated WebGPU unavailability (`navigator.gpu` disabled) fallback UI verification & simulated `device.lost` live recovery without page reload.
  - Endurance & leak benchmark: 600-frame multi-mode stress + 30 rapid mode switches + 20 canvas resizes measuring WASM linear memory growth (`0` unbounded growth), GPU texture leak counter (`activeTextures` bounded), and p50/p95/p99 simulation & frame timings (explicitly reporting SwiftShader CPU-rasterizer vs. WASM simulation budgets!).
- **Acceptance Criteria**:
  - 100% of Playwright E2E, fallback, persistence-restart, and leak/benchmark assertions execute and pass with zero skipped/disabled tests.
- **Required Tests**: `npm run test:e2e` and `npm run test:perf`.
- **Dependencies**: `P10-002`.

---

## Phase 11 — CI Workflows, Legal Audit, Production Build Verification & Documentation

### [x] `P11-001` — GitHub Actions CI Workflows (`.github/workflows/ci.yml`)
- **Objective**: Author complete, reproducible GitHub Actions workflow running legal/BIOS artifact checks, Rust `fmt`/`clippy`/`test`/property tests, WASM release build, WGSL `naga` shader validation, TypeScript `tsc --noEmit`, Vitest unit tests, production Vite build, and Playwright WebGPU + persistence + visual regression tests.
- **Acceptance Criteria**:
  - Workflow YAML syntax & script targets validated locally against the exact `npm run ...` scripts executed in this workspace.
- **Required Tests**: Workflow schema/script validation test (`tests/ci-workflow.test.ts`) + full local execution of every CI step.
- **Dependencies**: `P10-003`.

### [x] `P11-002` — Production Static Build & Preview Server Verification
- **Objective**: Build optimized production bundle (`dist/`), serve it over HTTP (`0.0.0.0`), and run production smoke + WebGPU + persistence E2E checks against the built bundle to confirm zero dev-only assumptions, correct WASM MIME/loading, offline-capable self-contained assets, and clean console/network logs (zero external runtime network requests).
- **Acceptance Criteria**:
  - Production bundle loads with zero console errors, zero external HTTP requests, working WASM + WebGPU + Audio + IndexedDB persistence.
- **Required Tests**: `npm run test:prod` against built `dist/` bundle.
- **Dependencies**: `P11-001`.

### [x] `P11-003` — Comprehensive User, Architecture, Fidelity/Provenance & Limitations Documentation
- **Objective**: Write/update root `README.md` (preserving `aap/osdbits` attribution & original PS2 instructions while documenting `osdbits-wasm`), `docs/wasm-architecture.md`, `docs/fidelity-and-provenance.md`, `docs/legal-and-assets.md`, and `docs/testing-and-deployment.md`, and finalize `TODO.MD`.
- **Acceptance Criteria**:
  - Every mode, parameter provenance (`Sourced`, `Measured`, `Approximated`, `Unknown`), legal boundary, optional user BIOS import workflow, test command, and static hosting setup (`COOP`/`COEP` optionality, MIME types, Vite base path) is accurately documented.
  - All tasks in `TODO.MD` accurately reflect final verified `[x]` or `[!]` status.
- **Required Tests**: Documentation link/command/status verification test + `npm run check:all`.
- **Dependencies**: `P11-002`.

---

## Deferred Features

1. **Retail PS2 Memory Card / Disc `Browser` 3D Icon Grid (`mcman` / `mcserv` + `icon.sys` `.ico` meshes)**:
   - *Reason*: Out of scope of the existing `aap/osdbits` reconstruction (`MenuSelectItem(0)` in `osdbits/menu.c:1690` is an explicit stub; retail OSDSYS delegates to separate `Browser` subsystems and Memory Card icon decoders). In `osdbits-wasm`, selecting `Browser` in Mode D opens a clearly labeled clean-room Scene & Tower Memory-Card Simulator inspector rather than claiming retail Memory Card Browser reconstruction.
2. **Retail `Version Information` (`0x227028`) and `Options` (`0x223658`) Hardware Sub-Screens**:
   - *Reason*: Stubbed in `osdbits/menutext.c` because they query PlayStation 2 CD/DVD mechacon, EE/IOP ROM versions, and PlayStation 1 driver flags (`rom0:VERSTR`). Exposed only as clearly labeled clean-room build/provenance diagnostics where helpful.
3. **Full Shift-JIS 2-Byte Kanji `FONTM` ROM Glyph Rasterizer**:
   - *Reason*: Retail `rom0:FONTM` is copyrighted Sony firmware font data not present in this repository. Clean-room default mode provides crisp Latin/extended OSD glyphs (`EN/FR/ES/DE/IT/NL/PT`), and optional user-imported `FNTIMAGE` supports `FNTASCII` + `FNTEXOSD`.
4. **IOP `rom0:OSDSND` SPU2 Hardware ADPCM/MIDI Synthesizer Emulation**:
   - *Reason*: Requires copyrighted `SNDIMAGE` samples and IOP SPU2 hardware emulation (`osdbits/sound.c` only issues SIF RPC calls). Replaced by a clean-room Web Audio procedural ambient soundscape + optional user-supplied audio/asset support.

---

## Known Limitations & Hardware / Environmental Boundaries

1. **Headless Sandbox GPU Adapter (`SwiftShader Vulkan` Fallback vs. Physical GPU Hardware)**:
   - This Linux container environment has no `/dev/dri` hardware GPU device nodes. Browser WebGPU tests (`@playwright/test` + `@sparticuz/chromium`) execute real WebGPU API calls, WGSL compilation, render passes, depth/blend pipelines, and pixel readbacks against Chromium's bundled **SwiftShader Vulkan CPU fallback adapter**. While shader semantics, render-graph hazards, and deterministic visual outputs are verified end-to-end, physical discrete/integrated GPU driver quirks (Metal, D3D12, desktop Vulkan/Mesa RADV/ANV) cannot be hardware-timed inside this container and are explicitly reported as fallback-adapter verified rather than physical-GPU verified.
2. **Unverified Retail `osdBootParam` Value on Hardware Illegal-Disc Boot**:
   - As documented in `osdbits/opening.c:4262`, the exact integer `osdBootParam` written by the retail disc-check thread upon detecting an unauthorized disc is unverified against live hardware RAM (values `101..105`, `113`, `116` all produce the identical infinite-idle red screen observed in retail GS dumps).
3. **Approximated Sub-Details Inherited from `osdbits` Clean-Room Reconstruction**:
   - While `ProcessOpeningAnimation`, `vucode_1.vsm` tower lighting/geometry, `HeightGrid`, `DrawExtraBuf2`, `DrawRedFlare`, `InitFog`, and Module U (`menu.c`, `menuback.c`, `menuconfig.c`, `menutext.c`) are sourced/matched against retail disassembly, `DrawLights` particle jitter and procedural default textures (when running without user-imported BIOS `TEXIMAGE`) are clean-room visual approximations and are labeled as such in both documentation and the in-app Provenance Inspector.
