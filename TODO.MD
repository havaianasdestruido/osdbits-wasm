# osdbits-wasm — Master Engineering Plan (`TODO.MD`)

Status Legend:
- `[ ]` Not started / In progress
- `[x]` Completed, acceptance criteria verified, and all required tests passing
- `[!]` Blocked (with root-cause analysis and blocker details)

---

## Phase 0 — Repository Reconnaissance & Baseline Report
- [x] **P00-001** — Inspect repository tree, git history, build system, and original C/VU1 sources (`osdbits/`, `vucode_*.vu`, `ps2expand.c`, `tools/`, `matching/`).
  - **Dependencies:** None
  - **Acceptance Criteria:** Every C, header, assembly, and Python tool file in `osdbits/` and root is inspected and cataloged; no original PS2 files are deleted or broken.
  - **Required Tests:** `git status` clean check; file inventory verification in `docs/PHASE0-RECONNAISSANCE.md`.
- [x] **P00-002** — Analyze all reverse-engineering documents (`docs/OSDSYS-STRUCTURE-REPORT.txt`, `docs/towers-analysis.md`, `docs/glass-refraction.md`, `docs/object-order.md`, `docs/menu-*.md`, `docs/pad.md`, `docs/sound.md`, `docs/rom-survey.md`) and reference screenshots (`docs/*.png`).
  - **Dependencies:** `P00-001`
  - **Acceptance Criteria:** Exact mathematical formulas, pass orders, GS blend modes (`BlendModes[0..10]`), work-buffer roles (`wb3`, `wb4`), camera state machines, and input/audio protocols are documented.
  - **Required Tests:** Baseline architecture report written to `docs/PHASE0-RECONNAISSANCE.md`.
- [x] **P00-003** — Provision and verify Rust 1.88.0 (`x86_64-unknown-linux-gnu` + `wasm32-unknown-unknown`, `clippy`, `rustfmt`) and headless WebGPU-capable Chromium (`150.0.7871.100` with Vulkan SwiftShader) in `/usr/local`.
  - **Dependencies:** `P00-001`
  - **Acceptance Criteria:** `rustc --version`, `cargo build --target wasm32-unknown-unknown --release`, and headless Chromium `navigator.gpu.requestAdapter()` + `requestDevice()` succeed outside `/home/user` without polluting the repo tree.
  - **Required Tests:** Smoke compilation of `wasm32-unknown-unknown` cdylib and headless Chromium WebGPU device acquisition check.
- [x] **P00-004** — Create root `TODO.MD` with Phases 0–11, unique task IDs, acceptance criteria, required tests, dependencies, and Deferred Features & Known Limitations section.
  - **Dependencies:** `P00-001`, `P00-002`, `P00-003`
  - **Acceptance Criteria:** `TODO.MD` exists at repository root and covers all phases and quality gates.
  - **Required Tests:** Markdown structure and task ID uniqueness check.

---

## Phase 1 — Architecture & Legal Asset Strategy
- [ ] **P01-001** — Scaffold hybrid Rust/WASM + TypeScript/WebGPU workspace (`Cargo.toml`, `crates/osdbits-core/`, `package.json`, `tsconfig.json` with `strict: true`, `vite.config.ts`, `vitest.config.ts`, `playwright.config.ts`, `.gitignore`).
  - **Dependencies:** `P00-004`
  - **Acceptance Criteria:** Clean workspace layout where `osdbits/` (original PS2 C code) and `docs/` remain intact; TypeScript compiler runs with `strict: true`, `noUncheckedIndexedAccess: true`, `exactOptionalPropertyTypes: true`; scripts for `build:wasm`, `build`, `test:rust`, `test:unit`, `test:e2e`, `lint`, `typecheck` are wired.
  - **Required Tests:** `cargo check`, `cargo check --target wasm32-unknown-unknown`, `npm run typecheck`.
- [ ] **P01-002** — Implement legal asset boundary & ROMDIR / LZSS `expand()` parser in Rust (`crates/osdbits-core/src/resource.rs`).
  - **Dependencies:** `P01-001`
  - **Acceptance Criteria:** Exact port of `ps2expand.c` LZSS `expand()` and `tools/extract-res.py` ROMDIR directory parser (`RESET`, `ROMDIR`, `EXTINFO`, 16-byte alignment rules) + `--tables` extractor (`FONTDATA`, `MENUGEOM`) with strict bounds checking so malformed/truncated/adversarial inputs never panic or read out of bounds.
  - **Required Tests:** Rust unit tests for valid synthetic ROMDIR containers, LZSS round-trip/decompression tests, boundary/truncation error tests, and deterministic property/fuzz tests over arbitrary byte slices.
- [ ] **P01-003** — Implement PS2 texture format decoders & procedural clean-room asset generator in Rust (`crates/osdbits-core/src/assets.rs`).
  - **Dependencies:** `P01-002`
  - **Acceptance Criteria:** Faithful implementation of `opening.c` texture formats (Format 3 white+alpha8, Format 4 black+alpha8, Format 5 lum8+alpha8, PSMCT32, PSMCT16 2-level software mipmap generator, PSMT4 + 16-color CLUT), `menuback.c` `DecodeKabeTile` (128×128 tiled 4-bit expander), `menuconfig.c` grey expander (`b | b<<8 | b<<16 | 0x7F000000` and bitwise complement `TEXCBINV`), and `menutext.c` `FNTASCII`/`FNTEXOSD` + `TEXCMARU` decoders; plus a deterministic clean-room procedural asset pack so `osdbits-wasm` runs out-of-the-box with zero proprietary Sony BIOS files.
  - **Required Tests:** Rust unit tests verifying exact byte layouts, dimensions, alpha ranges, `TEXCBINV == ~TEXCBUMP` invariant, and procedural asset completeness across all 31 texture/font slots + `MENUGEOM` + `FONTDATA`.

---

## Phase 2 — Rust/WASM Core Foundations
- [ ] **P02-001** — Port PS2 deterministic math, `osdRand` LCG, `libvu0` matrix/vector primitives, and `MatrixDrive` 16-bit angle stack (`crates/osdbits-core/src/math.rs`).
  - **Dependencies:** `P01-001`
  - **Acceptance Criteria:** Exact implementation of `osdRand`/`osdSrand` (`seed * 1103515245 + 12345`), `Vec4`, `Mat4`, `sceVu0UnitMatrix`, `sceVu0MulMatrix`, `sceVu0TransMatrix`, `sceVu0RotMatrixX/Y/Z`, `sceVu0CameraMatrix`, `sceVu0NormalLightMatrix`, `sceVu0LightColorMatrix`, `sceVu0ViewScreenMatrix`, `sceVu0ClipAll`, `FTOI4`, and Module U's 16-bit angle `mdSin`/`mdCos` (`65536 = 2π`) and 8-level `MatrixDrive` stack (`mdPush`, `mdPop`, `mdRotX/Y/Z`, `mdTranslate`, `mdScale`).
  - **Required Tests:** Rust unit tests and property-based tests verifying `osdRand` sequence parity, `mdSin² + mdCos² ≈ 1`, matrix associativity, camera/view-screen projection invariants, and `FTOI4` fixed-point conversion.
- [ ] **P02-002** — Port PS2 controller input cooker (`pad.c`) and sound mailbox (`sound.c` `OSDDispatch`) (`crates/osdbits-core/src/input_audio.rs`).
  - **Dependencies:** `P02-001`
  - **Acceptance Criteria:** Exact implementation of `pad.c` region Circle/Cross swap (`region_swap`: Japan `0` vs World `1`), press/release edge detection, UP/DOWN repeat counter (`-1` released, `0` press edge, `count >= 31 && count % 3 == 0` repeat — first repeat at frame 33, then every 3 frames, DOWN press store-over-OR quirk, LEFT/RIGHT press-only), and `sound.c` 128-entry `OSDDispatch` mailbox + 60-tick reverb depth ramp (`count * 0x9ffec / 127 / 60`).
  - **Required Tests:** Exhaustive 65,536-pair button shuffle verification test (matching `docs/pad.md` `swapcheck.c`), 400-frame scripted & randomized hold/repeat test (matching `repcheck.c`), and `OSDDispatch` ring buffer overflow/drain/reverb-ramp unit tests.
- [ ] **P02-003** — Design and implement the zero-copy WASM C-ABI & render command buffer (`crates/osdbits-core/src/ffi.rs`, `src/wasm/bridge.ts`).
  - **Dependencies:** `P01-003`, `P02-001`, `P02-002`
  - **Acceptance Criteria:** Flat `#[repr(C)]` vertex (`PosUvColorVertex`), instance, uniform, and `RenderPassCmd` structs exposed via WASM linear memory pointers + TypeScript wrapper (`WasmCoreBridge`) with bounds-checked typed array views and JSON state inspection/import/export.
  - **Required Tests:** Rust ABI layout/alignment tests and Vitest WASM integration tests instantiating the compiled `osdbits_core.wasm` binary in Node and stepping simulation frames.

---

## Phase 3 — WebGPU Rendering Pipeline
- [ ] **P03-001** — Implement WebGPU device/adapter manager, capability probe, canvas resize/aspect-ratio manager, and device-loss recovery (`src/renderer/gpu-context.ts`).
  - **Dependencies:** `P02-003`
  - **Acceptance Criteria:** Acquires WebGPU adapter & device, configures canvas (`rgba8unorm` / `bgra8unorm`), allocates offscreen targets (`screenColor`, `depthTexture`, `wb3` = `extraBuf1`, `wb4` = `extraBuf2`, `snapshotTexture`), handles resolution scaling (`0.5x`..`2.0x`) and aspect modes (`4:3`, `Full`, `16:9`), and provides structured diagnostics when WebGPU is unavailable or lost.
  - **Required Tests:** Unit tests for resolution/viewport math and Playwright WebGPU initialization + resize tests in Chromium.
- [ ] **P03-002** — Implement WGSL shaders and pipeline cache for PS2 GS blend modes (`BlendModes[0..10]`), samplers (`CLAMP` vs `REPEAT`, `LINEAR` vs `NEAREST`), and depth states (`src/renderer/shaders.ts`, `src/renderer/pipeline.ts`).
  - **Dependencies:** `P03-001`
  - **Acceptance Criteria:** WGSL vertex/fragment shaders and WebGPU blend states accurately implementing GS `ALPHA_1` modes: opaque (`abe=0`), source-alpha `(Cs-Cd)*As + Cd` (mode 4), additive by `As` `Cs*As + Cd` (mode 5), subtractive by `As` `Cd - Cs*As` (mode 6), fixed-alpha additive `Cs*FIX + Cd` (mode 0), fixed-alpha blend `(Cs-Cd)*FIX + Cd` (mode 2), and dest-alpha additive `Cs*Ad + Cd` (mode 8), including PS2 `0x80 = 1.0` alpha scaling (`As / 128.0`) and `MODULATE` (`texel * vertexColor * 2.0`).
  - **Required Tests:** Shader compilation tests and WebGPU offscreen render pass pixel-value verification tests in Chromium.
- [ ] **P03-003** — Implement multi-pass frame graph executor & pixel readback inspector (`src/renderer/frame-renderer.ts`).
  - **Dependencies:** `P03-002`
  - **Acceptance Criteria:** Consumes the WASM `RenderPassCmd` stream and vertex buffer each frame, switches render targets (`screen`, `wb3`, `wb4`), performs `copyTextureToTexture` snapshots (`CubeCaptureBuffer`, `DrawToExtraBuf2`, `MenuBackdrop` copies), executes 3D/2D draw calls, blits final output to the swapchain canvas, and supports deterministic RGBA pixel readback (`readbackFramePixels()`) for automated visual tests.
  - **Required Tests:** WebGPU integration test verifying multi-target ping-pong (`screen` ↔ `wb3` ↔ `wb4`) and pixel readback checksums.

---

## Phase 4 — Mode A & Mode B: Opening / Towers & Idle Experience
- [ ] **P04-001** — Port Opening camera state machine (`ProcessOpeningAnimation` states 0–7), 11 opening types (`'j'..'t'`), and Idle Mode (`openingType = 3`) in Rust (`crates/osdbits-core/src/opening.rs`).
  - **Dependencies:** `P02-003`
  - **Acceptance Criteria:** Faithful implementation of `opening.c`'s `InitAnimation`, `ProcessOpeningAnimation` across Z milestones `{16, 56, 104, 320, 672, 800, 1160, 1160}`, PAL (`50 Hz`) vs NTSC (`60 Hz`) timing scales, Mode A full opening choreography (downward drift → bottom orbit → rapid fly-up → fade out → transition to Main Menu or loop), and Mode B indefinite Idle tower-field mode.
  - **Required Tests:** Rust unit & property tests verifying camera Z progression across states 0..7 in Mode A, indefinite bounded Z in Mode B, and PAL/NTSC step scaling.
- [ ] **P04-002** — Port 14×9 Tower Field simulation, 21-slot memory card boot-history mapping (`histCells`), 20×20 radial `towerGrid`, and `vucode_1` 4-light + fog shading (`crates/osdbits-core/src/opening.rs`).
  - **Dependencies:** `P04-001`
  - **Acceptance Criteria:** Exact `histCells[21][6]` to `towerHeights[14][9]` accumulation, `towerOffsets` jitter (`±10`), `towerVisible` culling, 6 quad strips per tower (`towerchain.dsm`), and `vucode_1.vsm` vertex lighting (`4 directional lights + ambient` clamped and fog-blended by distance) + customizable tower density/height profile in Mode E.
  - **Required Tests:** Rust unit tests verifying empty vs populated boot-history tower counts/heights, `vucode_1` lighting math parity, and deterministic vertex generation.
- [ ] **P04-003** — Port 6 scrolling Fog layers (`DrawFog`), 4 orbiting Lights + 128-point ribbon Trails (`DrawLights`, `DrawTrail`), 5 10-pass Refractive Glass Cubes (`DrawCube`), fly-up motion blur (`sub_218b20`), fade overlay (`sub_218bd0`), and `TEXOSCE` SCE overlay (`DoSCEText`).
  - **Dependencies:** `P04-002`, `P03-003`
  - **Acceptance Criteria:** Complete Opening draw order: `DrawTowers` → `DrawExtraBuf2(80)` → `DrawToExtraBuf2` → `DrawFog` → `DrawLightsAndCubes` (4 lights + 128-point trails + 5 cubes × 10 passes with `CubeCaptureBuffer` and `CubeComputeUV` modes 0/1/2) → fly-up motion blur → fade overlay → `DoSCEText` (`Sony Computer Entertainment`) → optional aspect black bars.
  - **Required Tests:** Rust unit tests for light trail ring buffers, cube UV modes 0/1/2, SCE text fade curve, and Playwright WebGPU visual test rendering Mode A (frames 10, 90, 240, 420) & Mode B and verifying non-blank multi-region luminance/color signatures.

---

## Phase 5 — Mode C: Illegal-Disc / Error Scene
- [ ] **P05-001** — Port Illegal-Disc scene (`DoIllegalDisc`: `DrawRedFlare`, `DrawIllegalFog`, `DrawIllegalCubes`, `DrawIllegalFades`, `DoIllegalText`) in Rust & WebGPU (`crates/osdbits-core/src/illegal.rs`).
  - **Dependencies:** `P04-003`
  - **Acceptance Criteria:** Faithful port of `opening.c`'s Illegal-Disc pipeline:
    1. `DrawRedFlare`: 7 spinning 16-gon polygon discs (small + large) + 5 `TEXOFLAR` sprites + `DrawExtraBuf2(112)` + `DrawToExtraBuf2()`.
    2. `DrawIllegalFog`: 128 falling 3×3 red cloud patches (`TEXOFOG0..4`) with `{148, 30, 38}` red tint and wrap-around at `2π`.
    3. `DrawIllegalCubes`: 5 red-tinted (`{0.42, 0.08, 0.04}`) 10-pass refractive glass cubes (`illegalCubePasses`).
    4. `DrawIllegalFades` & `DoIllegalText`: Localized error text (`"Please insert a PlayStation® or PlayStation®2 format disc."` across J/E/F/S/G/I/P languages) with timed fade-in.
  - **Required Tests:** Rust unit tests for red cloud wrap-around, flare ring geometry, red cube color/UV math, localized text selection, and Playwright WebGPU visual verification confirming dominant red-channel energy and crisp center text overlay.

---

## Phase 6 — Mode D: OSDSYS Menu & System Configuration Reconstruction
- [ ] **P06-001** — Port Module U Main Menu 3D scene (`menu.c` & `menuback.c`): software RTC, 7 orbiting Orbs + 50-sample Trails, `DrawKabeRibbon` 16×33 tunnel, deep-blue composite, 5-pass `ZoomBlur`, and 16-segment `DrawVignette` (`crates/osdbits-core/src/menu.rs`).
  - **Dependencies:** `P02-003`, `P03-003`
  - **Acceptance Criteria:** Faithful implementation of `menu.c` and `menuback.c`: software clock (`hh:mm:ss`), 7 orbs with radius easing to `10 + 7.25 * (min/60)` (or `0` when clock editing), 50-sample trails drawn to screen and `wb3`, `TEXCKABE` 16×33 breathing cylinder tunnel (gated by `MenuBackdropVisible`), `{0x37, 0x28, 0x3C, 0x80}` screen tint composite, 5-pass `ZoomBlur` (`319.25 × 149.25`), and 16-segment `DrawVignette`.
  - **Required Tests:** Rust unit tests for orb trail ring buffer, clock angle easing, vignette geometry, and zoom-blur UV offsets.
- [ ] **P06-002** — Port System Configuration 3D Glass (`menuconfig.c`): 90-frame timer choreography, 12 hexagonal Clock Rods (with front-rod minute split & 2-walk bloom), and 5 spinning Config Cubes (with 8-pass refraction/emboss & `TEXCREFA` mask composite) (`crates/osdbits-core/src/menuconfig.rs`).
  - **Dependencies:** `P06-001`
  - **Acceptance Criteria:** Faithful implementation of `menuconfig.c`:
    1. `CfgTimer` 90-frame (`40+40+10` NTSC) choreography opening tunnel fade, 12-rod carousel, and at frame 40 the 5 config cubes.
    2. 12 hexagonal bevelled rods (`menuRodVerts`, 16 faces), front rod split at `1 - minutes/60` with `{167, 217, 255}` cyan-white lower segment, 5-pass refraction/emboss + 2-walk `TEXCFLOW` + `TEXCBUMP`/`TEXCBINV` bloom (`alpha = 30`).
    3. 5 config cubes (`menuCubeVerts`, 6 faces, identity camera at `z = 47.5`), selected cube easing to `{0, 150, 200}` with `-0.1` confirm kick, 8-pass refraction/emboss + `MeshDrawCubeMask` composite.
  - **Required Tests:** Rust unit tests for `CfgTimer` state transitions, `MeshTransform`/`MeshFresnel` math, front-rod split geometry, cube selection color easing, and WebGPU render pass tests.
- [ ] **P06-003** — Port 2D OSD Text Engine & Interactive Menu / System Configuration / Memory Card Browser UI (`menutext.c`) (`crates/osdbits-core/src/menutext.rs`).
  - **Dependencies:** `P06-002`
  - **Acceptance Criteria:** Faithful implementation of `menutext.c` glyph emitter (`FNTASCII` + `FNTEXOSD`, `\7oNNN` & `\7rN.NN` escapes), Main Menu (`Browser`, `System Configuration`, `Version Information`), System Configuration 5 items (`Clock Adjustment` 6-field editor freezing clock & sinking orbs to center, `Screen Size` 4:3/Full/16:9, `DIGITAL OUT (OPTICAL)` On/Off, `Component Video Out` Y Cb/Pb Cr/Pr vs RGB, `Language` 7 languages), top date/time bar, letterbox bars, `TEXCMARU` button hint bar, plus an interactive Memory Card / Scene Browser screen (`Browser` item) inspecting the 21 boot-history slots that drive the Opening towers (clearly badged as `Interactive Browser Extension`).
  - **Required Tests:** Rust unit tests for string measurement, escape parsing, menu navigation, Clock Adjustment leap-year/month-length clamping, and Playwright E2E navigation tests across Main Menu, System Configuration, Clock Adjustment, Version Information, and Browser.

---

## Phase 7 — Single-Button Activation & Web Audio Integration
- [ ] **P07-001** — Implement Web Audio synthesizer & `OSDDispatch` sequencer (`src/audio/audio-engine.ts`).
  - **Dependencies:** `P02-002`
  - **Acceptance Criteria:**
    1. `unlockFromUserGesture()` calls `new AudioContext()` (if needed) and `audioContext.resume()` **synchronously inside the click event handler** before any `await`.
    2. Drains the WASM `OSDDispatch` mailbox every frame, honoring the 60-tick stereo reverb fade-in ramp (`StSetReverbDepth`), 8 BGM slots (`0: SNDBOOTS` PS2 startup chord, `1: SNDTNNLS` tower wind/drone, `2: SNDCLOKS` System Configuration ambient glass pad, `6: SNDWARNS` Illegal-Disc ominous low red chord, `7: SNDRCLKS` swirling orb arpeggio), fade-outs (`StBgmStop`), and authentic OSDSYS UI sound effects (`StSePlay(1, n)` for `n=4` confirm, `5` toggle, `6` cursor move, `10` cancel/back).
    3. Gracefully handles muted/suspended/unavailable audio contexts without blocking visual rendering.
  - **Required Tests:** Unit tests verifying synchronous `AudioContext.resume()` invocation inside click handler (spying on call order before first `await`), `OSDDispatch` message routing for BGM/SE/fade/reverb, and mute/volume persistence.
- [ ] **P07-002** — Build the Single-Button Activation Experience (`Start OSDSYS`) & boot flow (`src/app/boot-controller.ts`, `src/ui/activation-overlay.ts`).
  - **Dependencies:** `P03-003`, `P07-001`
  - **Acceptance Criteria:** On initial load, displays a clean PS2-inspired activation overlay with a prominent primary button (`Start OSDSYS`), capability badges (WebGPU, WebAssembly, Storage, Audio), and quick-start mode selector. Clicking `Start OSDSYS` synchronously unlocks audio, initializes WebGPU & WASM, restores saved settings/assets from IndexedDB/OPFS, hides the overlay, and starts the animation immediately without requiring a second click.
  - **Required Tests:** Playwright E2E test verifying single-click activation transitions from overlay to active WebGPU rendering + running frame counter + unlocked audio state.

---

## Phase 8 — Mode E: User Customization, Controls & Diagnostics UI
- [ ] **P08-001** — Implement Mode E Customization Drawer, Scene Switcher, and Diagnostics HUD (`src/ui/control-panel.ts`, `src/ui/diagnostics-hud.ts`).
  - **Dependencies:** `P04-003`, `P05-001`, `P06-003`, `P07-002`
  - **Acceptance Criteria:** Keyboard- and pointer-accessible HUD and collapsible drawer allowing real-time control of:
    - Scene Mode: Mode A (Opening / Towers), Mode B (Idle Tower Field), Mode C (Illegal Disc / RSOD), Mode D (OSDSYS Main Menu & System Configuration), plus direct jump to System Configuration or Memory Card History Browser.
    - Camera & Simulation: Camera preset (Retail, High Orbit, Low Canyon, Free Orbit), animation speed (`0.25x`..`2.0x`), deterministic PRNG seed, NTSC (`60 Hz`) vs PAL (`50 Hz`) timing, region (`World (Cross=Enter)` vs `Japan (Circle=Enter)`), and boot-history tower preset (`Fresh Console (0 games)`, `Light (5 games)`, `Moderate (12 games)`, `Full Card (21 games)`, or custom slot editor).
    - Visual & Quality: Resolution scale (`0.5x`, `1.0x`, `1.5x`, `2.0x`), aspect ratio (`4:3`, `Full`, `16:9`), fog intensity (`0%`..`200%`), glass refraction strength (`0%`..`200%`), zoom-blur & bloom toggle, and Reduced Motion mode (`prefers-reduced-motion` auto-detection + manual toggle).
    - Audio: Master mute, master volume (`0%`..`100%`), BGM volume, SE volume, and manual sound-test triggers.
    - Resource Pack Manager: Inspect active asset source (`Procedural Clean-Room` vs `User Imported BIOS/ROMDIR`), import local BIOS/ROMDIR file with live validation report, or clear imported assets.
    - Settings Import / Export / Reset to Defaults.
  - **Required Tests:** Unit tests for state validation/clamping and Playwright E2E tests exercising every control and verifying immediate live effect on the simulation/renderer.

---

## Phase 9 — Persistent Storage (IndexedDB + OPFS) & Read-Back Verification
- [ ] **P09-001** — Implement versioned IndexedDB + OPFS storage manager with mandatory read-back verification (`src/storage/persistence.ts`).
  - **Dependencies:** `P01-003`, `P08-001`
  - **Acceptance Criteria:**
    1. Stores structured user settings, OSDSYS system configuration (language, screen size, digital out, component out, clock offset, region), and 21-slot memory card boot history in IndexedDB (`osdbits-wasm-db`), and large imported binary resource bundles in OPFS (with IndexedDB blob store fallback).
    2. **Every write operation reads back the persisted record from storage, verifies schema version, payload deep-equality, and SHA-256 / FNV-1a checksum before returning `verified: true`**.
    3. Implements schema migration (`v0`/`v1` → `v2`), corrupted payload quarantine & fallback to defaults, storage quota / unavailable storage graceful degradation, and monotonic revision counter to prevent stale concurrent writes from overwriting newer state.
  - **Required Tests:** Comprehensive Vitest + Playwright storage test suite covering: write + read-back verification, page reload restoration, schema v1→v2 migration, corrupted JSON/checksum recovery, quota error handling, concurrent write ordering, and JSON export/import/reset round-trip.

---

## Phase 10 — Comprehensive Multi-Layer Testing & Visual Regression
- [ ] **P10-001** — Execute and expand Rust unit, property-based, and fuzz test suites (`crates/osdbits-core/tests/`).
  - **Dependencies:** `P04-003`, `P05-001`, `P06-003`
  - **Acceptance Criteria:** 100% pass rate on `cargo test` covering math, `osdRand`, `MatrixDrive`, `ps2expand`, ROMDIR parser, texture decoders, procedural assets, Opening/Idle/Illegal/Menu/Config state machines, input cooker (`65536` button pairs + `400`-frame repeat), `OSDDispatch`, and randomized fuzz/property tests across 1,000+ random seeds and malformed inputs.
  - **Required Tests:** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **P10-002** — Execute TypeScript unit, WASM integration, audio, and persistence test suites (`tests/unit/`).
  - **Dependencies:** `P07-001`, `P08-001`, `P09-001`
  - **Acceptance Criteria:** 100% pass rate on `npm run test:unit` (Vitest) covering WASM bridge, settings validation, storage migrations/corruption/read-back, audio gesture unlocking, and UI state controllers.
  - **Required Tests:** `npm run test:unit` and `npm run typecheck`.
- [ ] **P10-003** — Execute Playwright end-to-end, WebGPU visual regression, persistence reload, and 300-frame endurance/performance tests in real Chromium (`tests/e2e/`).
  - **Dependencies:** `P10-001`, `P10-002`
  - **Acceptance Criteria:**
    1. Single-button activation test (`Start OSDSYS` → synchronous `AudioContext.resume()` → active WebGPU rendering).
    2. Mode A (Opening), Mode B (Idle), Mode C (Illegal Disc), Mode D (Main Menu, System Configuration, Clock Adjustment, Browser, Version Info), and Mode E (Customization) E2E & visual signature tests comparing rendered WebGPU canvas readbacks against expected structural/color/luminance invariants and saving PNG snapshots.
    3. Persistence reload test: customize scene, speed, language, screen size, and boot history → verify read-back badge → `page.reload()` → click `Start OSDSYS` → verify all settings and 3D scene state restored identically.
    4. Endurance test: run 300+ frames across rapid mode switches (`A → B → C → D → Config → A`) with zero uncaught page errors, zero WebGPU validation errors, and bounded memory growth.
  - **Required Tests:** `npm run test:e2e`.

---

## Phase 11 — Documentation, Provenance & Final Verification Gate
- [ ] **P11-001** — Write comprehensive `README.md`, `docs/ARCHITECTURE-WASM.md`, and legal/provenance documentation.
  - **Dependencies:** `P10-003`
  - **Acceptance Criteria:** Documents the original `aap/osdbits` research provenance, what is faithfully reconstructed (`Implemented` & `Verified`) vs clean-room procedural fallback (`Approximate`), build/test/run instructions, BIOS/ROMDIR user-import instructions, and legal hygiene guarantees.
  - **Required Tests:** Verify all documented commands (`npm run build`, `cargo test`, `npm run test:unit`, `npm run test:e2e`) succeed cleanly and live preview server is running via `start_process`.

---

## Deferred Features & Known Limitations

The following items from the broader PlayStation 2 boot ROM (`scph39001.bin`) are intentionally out of scope or approximated, as documented in `docs/rom-survey.md`, `docs/sound.md`, and `docs/menu-config.md`:
1. **Retail Sony Binary Assets Not Shipped (`Legal Constraint`):** The repository does not include any Sony BIOS dumps (`scph*.bin`), compressed `OSDSYS` ELF payloads, or extracted `TEX*`/`FNT*`/`SND*`/`ICO*` `.inc` files. Out-of-the-box execution uses deterministic procedural clean-room textures, fonts, geometry, and synthesized Web Audio; users may optionally import their own legally dumped PS2 BIOS image or ROMDIR containers in the browser UI for bit-exact retail textures and fonts.
2. **Shift-JIS Kanji ROM (`FONTM` / `KROM`, `Deferred`):** As in `osdbits/menutext.c`, the 2D text engine implements the full Latin/Western European (`FNTASCII`) and OSD symbol (`FNTEXOSD`) bitmap pipelines across the 7 Western languages (`English`, `French`, `Spanish`, `German`, `Italian`, `Dutch`, `Portuguese`). Two-byte Shift-JIS kanji unpacking from the 738 KB `FONTM` module is deferred when no BIOS `FONTM` is loaded, falling back to English glyph metrics for Japanese text labels while preserving Japanese region button mapping (`Circle = Enter`, `Cross = Back`).
3. **Physical CD/DVD Drive Certification & Game ELF Boot (`Out of Scope`):** MechaCon `sceCdBootCertify()`, `cdrom0:\SYSTEM.CNF` parsing, `PS2LOGO` decryption, and `LoadExecPS2()` game execution are hardware boot-loader operations and are represented in the browser via the interactive Mode C (Illegal Disc) and Mode D (Memory Card / Boot History Browser) scenes.
4. **Full 3D Memory Card Icon Browser (`ICOB*`, `Approximate`):** While the 21-slot memory card boot-history table (`histCells[21][6]`) that drives the Opening tower field is fully interactive and editable in the Mode D `Browser` screen, full 3D skeletal icon (`icon.sys` / `.ico`) file-system management is deferred in favor of the OSDSYS boot-history & tower-field inspector.
