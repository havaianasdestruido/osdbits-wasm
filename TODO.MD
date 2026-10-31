# `osdbits-wasm` Master Implementation & Verification Roadmap (`TODO.MD`)

Status Legend:
- `[ ]` Pending / In Progress
- `[x]` Completed (Acceptance criteria met AND required tests executed and passing)
- `[!]` Blocked by environment/hardware constraint (explicitly documented with evidence)

Verification Terminology:
- **Implemented** — Code written and integrated.
- **Tested** — Automated unit/integration/browser tests executed and passing.
- **Verified** — Cross-checked against `osdbits` source constants, deterministic invariants, and real browser execution.
- **Approximate** — Clean-room or reverse-engineering approximation where retail verification is unavailable or legally substituted.
- **Blocked** — Requires physical hardware/capabilities absent in current container (e.g., physical discrete/integrated GPU via `/dev/dri`).
- **Deferred** — Explicitly out-of-scope enhancement tracked transparently.

---

## Phase 0 — Repository Reconnaissance and Baseline Audit

- [x] **P00-001**: Inspect entire existing `aap/osdbits` repository, git history, C/VU/ASM sources, Python/GS tools, matching suite, and reverse-engineering docs.
  - **Objective:** Establish complete technical inventory without modifying or deleting upstream artifacts.
  - **Acceptance Criteria:** Every source file in `osdbits/`, root `vucode_*`, `tools/`, `matching/`, and `docs/` reviewed; exact scene equations, RNGs, state machines, and GS blend pipelines cataloged.
  - **Required Tests:** `git status` clean check; complete line/file inventory verified.
  - **Dependencies:** None.
- [x] **P00-002**: Execute non-destructive baseline checks on upstream C and Python utilities and PS2 `Makefile`.
  - **Objective:** Confirm what compiles/runs on Linux `x86_64` host vs. what depends on offline PS2 SDK or external files.
  - **Acceptance Criteria:** `ps2expand.c` compiled with `-Wall -Wextra`, Python scripts syntax-checked via `py_compile`, and `make -n -C osdbits` failure mode documented.
  - **Required Tests:** Host GCC compile + CLI smoke check of `ps2expand.c`; `python3 -m py_compile` on `tools/*.py` and `osdbits/tools/*.py`.
  - **Dependencies:** `P00-001`.
- [x] **P00-003**: Audit sandbox toolchain, Rust/WASM compilation, and headless Chromium + WebGPU execution capabilities.
  - **Objective:** Empirically verify `rustc`/`cargo`/`rustfmt`/`clippy` (`1.99.0`), `wasm32-unknown-unknown` codegen, and Playwright `Chromium 153.0.8010.0` (WebAssembly, IndexedDB, OPFS, AudioContext, and Dawn/SwiftShader Vulkan 1.3 software WebGPU vs. absence of physical `/dev/dri` GPU).
  - **Acceptance Criteria:** Smoke WASM module compiled and instantiated; headless Chromium WebGPU adapter (`isFallbackAdapter: true`, `swiftshader`) and pixel readback empirically verified.
  - **Required Tests:** Rust native + `wasm32-unknown-unknown` smoke build/test; Playwright Chromium capability probe.
  - **Dependencies:** `P00-001`.
- [x] **P00-004**: Author Phase 0 Baseline Audit Report (`docs/baseline-audit.md`) and initialize `TODO.MD`.
  - **Objective:** Record baseline findings, architecture plan, legal boundaries, and full Phase 0–11 task breakdown prior to writing project code.
  - **Acceptance Criteria:** `docs/baseline-audit.md` and `TODO.MD` written at repository paths; upstream files untouched.
  - **Required Tests:** File presence and clean git diff inspection.
  - **Dependencies:** `P00-001`, `P00-002`, `P00-003`.

---

## Phase 1 — Legal/Asset Policy, Architecture Specification, and Project Scaffolding

- [x] **P01-001**: Document legal/clean-room asset policy (`docs/legal-and-assets.md`) and system architecture (`docs/architecture.md`).
  - **Objective:** Define strict zero-BIOS repository guarantees, clean-room procedural asset strategy, optional local user BIOS import pipeline, and the single WebGPU ownership boundary (TypeScript owns WebGPU device/resources; Rust/WASM owns deterministic simulation and flat vertex/command buffers).
  - **Acceptance Criteria:** `docs/legal-and-assets.md` and `docs/architecture.md` clearly specify legal rules, memory layout, module boundaries, and fidelity badges without claiming noncommercial immunity.
  - **Required Tests:** Automated repository scan test verifying zero `.bin`/BIOS/proprietary Sony resource files exist in workspace.
  - **Dependencies:** `P00-004`.
- [x] **P01-002**: Scaffold Rust Cargo workspace (`Cargo.toml`, `crates/osdbits-core`) and strict TypeScript/Vite/Vitest/Playwright workspace (`package.json`, `tsconfig.json`, `eslint.config.js`, `vite.config.ts`).
  - **Objective:** Set up reproducible build, formatting, linting, typechecking, WASM packaging, and test runners while keeping upstream `osdbits/`, `matching/`, and `tools/` untouched.
  - **Acceptance Criteria:** `strict: true`, `noUncheckedIndexedAccess: true`, `@typescript-eslint/no-explicit-any: "error"`, `.gitignore` updated cleanly for `target/`, `dist/`, `test-results/`, `__pycache__/` while permitting tracked test fixtures.
  - **Required Tests:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `npm run lint`, `npm run typecheck`, `npm run build:wasm`.
  - **Dependencies:** `P01-001`.

---

## Phase 2 — Rust/WASM Core Simulation, Deterministic RNG, Scene Logic, and Asset Pipeline

- [x] **P02-001**: Implement deterministic IEEE-754 math, PS2 `MatrixDrive` 16-bit trig, and exact `osdRand` / `towerRand` LCGs (`math.rs`, `rng.rs`).
  - **Objective:** Guarantee bit-identical `f32` simulation across native `x86_64` tests and `wasm32-unknown-unknown` in browsers.
  - **Acceptance Criteria:** `OsdRng` (`1103515245 * seed + 12345`, `& 0x7fffffff`) and `TowerRng` (`0x2b992ddf`) match C reference sequences; pure-Rust trig/matrix ops produce zero NaNs/Infs across valid inputs.
  - **Required Tests:** Rust unit tests + `SplitMix64` property tests (`2,000` trig samples + savestate draw `#386 == 4002`) for RNG reproducibility, determinism across resets, and matrix/projection finite bounds.
  - **Dependencies:** `P01-002`.
- [x] **P02-002**: Implement clean-room procedural texture, mesh (`MENUGEOM`), and proportional glyph generator (`procedural_assets.rs`, `osd_font.rs`).
  - **Objective:** Produce deterministic RGBA8 textures (`TEXOTOWR`, `TEXOBLPR`, `TEXOBLP`, `TEXOREF`, `TEXOFOG`, `TEXOFLAR`, `TEXCKABE`, `TEXCBLUR`, `TEXCNAVI`, `TEXCBUMP`, `TEXCBINV = ~TEXCBUMP`, `TEXCFLOW`, `TEXCREFA`, UI font & geometric button glyphs) and exact hexagonal bevelled rod + cube meshes without Sony data.
  - **Acceptance Criteria:** All procedural textures deterministic by seed/version, exact dimensions/formats, bitwise `TEXCBINV` complement invariant satisfied, and zero copyright/trademark strings.
  - **Required Tests:** Rust unit tests verifying dimensions, non-empty alpha/luminance distributions, deterministic checksums, and `TEXCBINV` complement law.
  - **Dependencies:** `P02-001`.
- [x] **P02-003**: Implement bounds-checked PS2 `ROMDIR` + LZ `expand()` parser and GS texture decoder (`bios_import.rs`).
  - **Objective:** Allow optional local user-imported BIOS/resource containers (`TEXIMAGE`, `ROMDIR`) to be validated, decompressed, decoded, and SHA-256/FNV-1a hashed safely in WASM without panicking on arbitrary or corrupt inputs.
  - **Acceptance Criteria:** Matches `ps2expand.c` LZ stream semantics; enforces strict output caps and bounds checks; returns typed `ImportError` on truncated/corrupt/oversized inputs.
  - **Required Tests:** NIST FIPS 180-4 SHA-256 test vectors, synthetic LZ round-trip unit tests, corrupt/truncated corpus tests, and `SplitMix64` bounded fuzz tests (1,500+ arbitrary and mutated byte slices verifying zero panics and bounded memory).
  - **Dependencies:** `P02-001`.
- [x] **P02-004**: Implement Opening (`Mode A`), Indefinite Idle (`Mode B`), and Illegal-Disc (`Mode C`) deterministic simulation and geometry emitters (`opening.rs`).
  - **Objective:** Reconstruct `osdbits/opening.c` + `vucode_1.vsm`: 21-slot tower history & sprawl, 14×9 swaying tower field with 4-light shading, 8-state camera timeline, 6-layer fog, 4 orbiting lights + trails, 5 10-pass refractive glass cubes, black fades, Idle canopy lock, and Illegal-disc red flares, falling red fog patches, and red refractive cubes.
  - **Acceptance Criteria:** Frame-by-frame determinism for fixed seed; Mode A transitions cleanly at timeline end; Mode B runs indefinitely without numeric drift; Mode C climbs `672 -> 800+` with red palette and flares.
  - **Required Tests:** Rust unit & property tests (`32` random seeds) for tower sprawl schedule, camera state transitions, idle stability over 10,000+ steps, illegal-disc invariants, and finite vertex coordinates.
  - **Dependencies:** `P02-001`, `P02-002`.
- [x] **P02-005**: Implement OSDSYS Menu & System Configuration (`Mode D`) and Customization (`Mode E`) simulation & geometry emitters (`menu.rs`).
  - **Objective:** Reconstruct `menu.c`, `menuback.c`, `menuconfig.c`, and `menutext.c`: 7 RTC-driven orbs + 50-sample `(age^4, age^2, age)` trails + entry scatter + radial bloom + vignette; `16×33` breathing `TEXCKABE` smoke tunnel + tint; 12 hexagonal bevelled glass clock rods (split active-hour minute fill bar + 2-walk `TEXCFLOW` sparkle); 5 zig-zag glass config cubes + kick animation; interactive Main Menu, System Configuration, Clock Adjustment digit editor, `[Approximate]` Browser showcase, and Mode E tuning parameters.
  - **Acceptance Criteria:** Clock angles, rod split ratio (`1 - min/60`), `cfgAnim` transitions, pad/keyboard/pointer navigation, O/X regional swap, and Mode E overrides (speed, camera drift, fog, bloom, refraction, reduced motion) behave deterministically.
  - **Required Tests:** Rust + WASM unit/integration tests covering orb orbital math, menu state machine navigation, pointer hit-testing, Clock Adjustment digit wrapping, and Mode E parameter clamping.
  - **Dependencies:** `P02-001`, `P02-002`.
- [x] **P02-006**: Implement flat zero-copy Render Command Buffer & C-ABI WASM exports (`render_list.rs`, `wasm_api.rs`, `src/wasm/bridge.ts`).
  - **Objective:** Expose clean, allocation-stable pointers/views (`RenderVertex`, `RenderBatch`, `FrameTelemetry`) across the Rust/WASM <-> TypeScript boundary.
  - **Acceptance Criteria:** `#[repr(C)]` struct sizes/alignments verified at compile time and across WASM ABI (`32B / 32B / 64B`); zero per-frame heap growth in steady state; clean reset/seed/mode/input/clock/customization/import API.
  - **Required Tests:** Rust ABI layout assertions + Node/Vitest real WASM instantiation (`tests/wasm/engine-wasm.test.ts`), multi-frame stepping, seed reproducibility, synthetic `TEXIMAGE` import/reset, and linear-memory stability tests.
  - **Dependencies:** `P02-003`, `P02-004`, `P02-005`.

---

## Phase 3 — WebGPU Renderer, Shaders, Geometry/Effects, Resize/Recovery, and Degraded Mode

- [x] **P03-001**: Implement WGSL shaders and WebGPU multi-pass renderer (`src/renderer/webgpuRenderer.ts`, `src/renderer/shaders/*.wgsl`).
  - **Objective:** Render the WASM command stream faithfully with PS2 GS `2x` texture modulation, `SrcAlpha` / `Additive` / `reverse-subtract` (`Cd - Cs*As`) / silhouette-mask blending, depth testing (`depth24plus`), per-object `copyTextureToTexture` refraction backdrop snapshots (untinted & tinted), and half-width bloom / `MenuZoomBlur` passes.
  - **Acceptance Criteria:** Zero WGSL compilation warnings/errors; explicit single-owner lifecycle (`create`, `renderFrame`, `resize`, `readPixels`, `dispose`); distinguishes hardware GPU vs. `isFallbackAdapter` (`SwiftShader`).
  - **Required Tests:** WGSL syntax/compilation test in real Chromium Dawn WebGPU + render pass pixel readback verification tests.
  - **Dependencies:** `P02-006`.
- [x] **P03-002**: Handle DPR/aspect scaling (`4:3`, `16:9`, `Full`), window resize, `device.lost` recovery, and deterministic Canvas 2D compatibility fallback (`src/renderer/fallbackCanvasRenderer.ts`, `src/renderer/createRenderer.ts`).
  - **Objective:** Ensure resizing never stretches geometry out of aspect, device loss surfaces recovery cleanly, and environments without WebGPU automatically degrade to a working Canvas 2D software renderer with clear user notice.
  - **Acceptance Criteria:** Clean resize across arbitrary viewports; simulated WebGPU absence/loss falls back without uncaught exceptions or blank screens.
  - **Required Tests:** Unit tests with mocked GPU failure/loss + Playwright browser tests exercising both WebGPU mode, simulated device loss, and forced Canvas 2D fallback mode.
  - **Dependencies:** `P03-001`.

---

## Phase 4 — One-Click Startup, Browser Audio Lifecycle, and UX Shell

- [x] **P04-001**: Implement browser-safe procedural Web Audio synthesizer & lifecycle manager (`src/audio/audioEngine.ts`).
  - **Objective:** Synthesize clean-room PS2-inspired ambient boot pad, crystalline tower resonance, illegal-disc low dissonance, and OSDSYS navigation chimes (`tick`, `confirm`, `cancel`, `transition`) with synchronous `AudioContext` unlock inside the user gesture handler.
  - **Acceptance Criteria:** `unlockFromUserGesture()` invokes `new AudioContext()` + `ctx.resume()` synchronously before any `await`; supports master/ambient/SFX gain, mute, optional visibility pause, and silent fallback if Web Audio throws or stays suspended.
  - **Required Tests:** Unit tests covering `suspended` / `running` / `closed` / constructor-throw states + Playwright gesture-unlock verification test.
  - **Dependencies:** `P01-002`.
- [x] **P04-002**: Build nostalgic PS2-aesthetic single-gesture startup gate and collapsible accessible HUD shell (`index.html`, `src/styles/osdbits.css`, `src/app/controller.ts`).
  - **Objective:** Clicking or pressing Enter/Space on **Start OSDSYS** unlocks audio, checks WebGPU, loads WASM + procedural/persisted assets, restores IndexedDB preferences, and immediately launches the scene with zero BIOS requirement and no second init button.
  - **Acceptance Criteria:** Single click/keypress transitions from gate to live rendering; HUD drawer hides cleanly for pure immersion (`H` / button) and exposes full keyboard focus order, ARIA labels, and status notices.
  - **Required Tests:** Playwright E2E test verifying single-gesture launch, audio unlock, HUD keyboard navigation, and zero console/page errors.
  - **Dependencies:** `P03-002`, `P04-001`.

---

## Phase 5 — Opening/Towers, Idle Mode, Illegal-Disc Mode, and OSDSYS-Inspired Menu/Customization

- [x] **P05-001**: Integrate and verify Mode A (Opening / Towers) & Mode B (Indefinite Idle Towers) end-to-end in the browser app.
  - **Objective:** Wire Mode A completion behavior (configurable: transition to OSDSYS Menu, transition to Idle Towers, or loop Opening), seed inspector, frame stepping, and Mode B indefinite drift/orbit controls.
  - **Acceptance Criteria:** Opening renders tower field, fog, light streaks, refractive cubes, clean-room title overlay, and transitions cleanly; Idle mode runs continuously with customizable drift.
  - **Required Tests:** Integration & Playwright tests verifying Mode A -> Mode D/B transition, deterministic seed replay, and Mode B continuous frame progression.
  - **Dependencies:** `P02-006`, `P03-002`, `P04-002`.
- [x] **P05-002**: Integrate and verify Mode C (Illegal-Disc Scene) and Mode D (OSDSYS Menu, System Configuration, Clock Adjustment, and Browser Showcase).
  - **Objective:** Wire full keyboard (Arrows, Enter, Escape) and pointer interaction for OSDSYS menu items, live wall-clock / deterministic-clock sync for the 7 orbs and 12 glass clock rods, sound cues, and fidelity badges (`Verified Reconstruction` vs. `Approximate`).
  - **Acceptance Criteria:** Illegal-disc renders red clouds/flares/red refractive cubes; Menu navigates Main Menu <-> System Configuration <-> Clock Adjustment <-> Browser Showcase smoothly by both keyboard and mouse/touch.
  - **Required Tests:** Integration & Playwright E2E tests navigating every menu screen, editing clock digits, switching languages/aspect, and triggering Mode C.
  - **Dependencies:** `P05-001`.
- [x] **P05-003**: Implement Mode E (User Customization & Visual/Audio Tuning) with faithful OSDSYS defaults and instant reset.
  - **Objective:** Expose scene selection, camera speed/drift, NTSC/PAL timing, resolution scale, fog/bloom/refraction toggles, reduced-motion mode, aspect ratio, audio mixer, and one-step reset to faithful defaults.
  - **Acceptance Criteria:** All customizations take effect immediately in live rendering, clamp safely, respect `prefers-reduced-motion` when opted in, and restore faithful defaults on reset.
  - **Required Tests:** Unit & Playwright E2E tests modifying customization controls, verifying live engine state changes, and resetting to defaults.
  - **Dependencies:** `P05-002`.

---

## Phase 6 — IndexedDB/OPFS Persistence, Migrations, Import/Export, and Cross-Restart Behavior

- [x] **P06-001**: Implement versioned IndexedDB + OPFS persistence layer (`src/persistence/storage.ts`) with schema migrations (`v1 -> v2`), read-back verification, and stale-write protection.
  - **Objective:** Persist settings, mode, audio/accessibility preferences, session telemetry, and hashed user-imported assets reliably across reloads and browser restarts, with `docs/persistence-schema.md` documentation.
  - **Acceptance Criteria:** Every write performs transaction commit + independent read-back checksum/revision verification; `v1` records migrate losslessly to `v2`; corrupt records quarantine/recover safely to defaults; `BroadcastChannel` + monotonic `revision` prevent stale tab overwrites; `navigator.storage.persist()` handled gracefully when supported, denied, or absent.
  - **Required Tests:** Unit/integration tests (`fake-indexeddb` + transaction mocks) covering `v1 -> v2` migration, corrupt payloads, quota/unavailable errors, stale-write rejection, JSON export/import, confirmed reset, and asset hash verification.
  - **Dependencies:** `P01-002`.
- [x] **P06-002**: Execute real-browser Playwright persistence tests across both page reload AND full persistent-context browser restart (`launchPersistentContext` close + relaunch).
  - **Objective:** Prove genuine disk-backed Chromium IndexedDB/OPFS survival across full browser process termination and relaunch, explicitly separated from simple in-context `page.reload()`.
  - **Acceptance Criteria:** Settings, mode, audio preferences, and imported test asset hashes survive both `page.reload()` and full `context.close()` -> `chromium.launchPersistentContext(userDataDir)` restart; confirmed reset wipes storage cleanly.
  - **Required Tests:** Dedicated Playwright persistence spec (`tests/e2e/persistence-restart.spec.ts`).
  - **Dependencies:** `P05-003`, `P06-001`.

---

## Phase 7 — Comprehensive Test Suite: Rust, WASM, TypeScript, IndexedDB, WebGPU, Playwright E2E

- [x] **P07-001**: Complete Rust unit, `SplitMix64` property-based, bounded fuzz, and WASM ABI integration test suites.
  - **Objective:** Achieve deep coverage of all simulation math, RNGs, tower sprawl, camera timelines, orb/rod/cube kinematics, LZ decompression, and WASM boundary invariants.
  - **Acceptance Criteria:** Zero warnings under `cargo clippy --all-targets -- -D warnings`; all Rust unit, property, fuzz-corpus, and Node/Vitest WASM boundary tests pass deterministically.
  - **Required Tests:** `cargo test --workspace`, Vitest WASM boundary suite (`tests/wasm/`).
  - **Dependencies:** `P02-006`.
- [x] **P07-002**: Complete TypeScript unit/integration, WebGPU Dawn/SwiftShader + mock/degraded tests, and full Playwright E2E user-journey suite.
  - **Objective:** Verify one-click startup, mode switching, keyboard/pointer menu navigation, customization, audio gesture states, settings JSON export/import, factory reset, WebGPU rendering, and forced fallback rendering with zero uncaught errors.
  - **Acceptance Criteria:** Zero console errors or uncaught page errors across all Playwright journeys; GPU test reports explicitly distinguish mock tests vs. Dawn/SwiftShader software adapter (`isFallbackAdapter: true`) vs. unavailable physical GPU (`Blocked`).
  - **Required Tests:** `npm run test:unit`, `npm run test:e2e`.
  - **Dependencies:** `P06-002`, `P07-001`.

---

## Phase 8 — Deterministic Visual Regression and Performance/Memory Verification

- [x] **P08-001**: Implement deterministic headless visual regression harness & baseline snapshots across key scenes (`tests/e2e/visual-regression.spec.ts`).
  - **Objective:** Capture seeded, fixed-clock frames (`Opening` early & mid-canopy, `Idle` towers, `Illegal` red flares/cubes, `Menu` 7-orb constellation, `System Configuration` 12 glass rods & 5 cubes, and `Degraded Fallback`) and enforce non-blank luminance/color/structural pixel-diff thresholds against tracked golden PNGs.
  - **Acceptance Criteria:** Every target scene verified non-blank, structurally distinct, and within strict pixel-diff tolerance across repeated headless runs.
  - **Required Tests:** Playwright visual regression test suite with automated golden image diffing (`tests/e2e/goldens/*.png`).
  - **Dependencies:** `P07-002`.
- [x] **P08-002**: Define and verify WASM size, startup latency, steady-state allocation, and multi-hundred-frame endurance budgets (`tests/perf/`).
  - **Objective:** Benchmark release WASM binary size, simulation step CPU cost, zero unbounded WASM/JS heap growth over 1,200+ frames, and clean GPU resource disposal across repeated mode switches.
  - **Acceptance Criteria:** Release WASM `< 250 KB` uncompressed (`167.65 KiB` actual); average WASM simulation + command prep `< 2.5 ms/frame` on sandbox CPU (`~0.24–0.33 ms/frame` actual); steady-state WASM linear memory growth `== 0 bytes` after warm-up across 1,200 frames and 24 mode switches.
  - **Required Tests:** Automated Vitest endurance & benchmark suite (`npm run test:perf`).
  - **Dependencies:** `P07-002`.

---

## Phase 9 — Accessibility, Responsive Layout, Error States, and Nostalgic Polish

- [x] **P09-001**: Audit and refine keyboard-only navigation, visible focus rings, ARIA semantics, `prefers-reduced-motion`, responsive desktop/mobile viewports, and calm error/fallback banners.
  - **Objective:** Ensure refined PS2-era atmospheric minimalism paired with modern accessibility and resilient error states (corrupt import file, blocked storage, unavailable WebGPU, muted/blocked audio).
  - **Acceptance Criteria:** Complete keyboard traversal without focus traps; reduced-motion dampens rapid camera fly-up, flashes, and zoom-blur jitter; mobile (`375x667`) and desktop (`1280x720`, `1920x1080`) layouts render cleanly without overflow.
  - **Required Tests:** Playwright accessibility, reduced-motion, responsive-viewport, and fault-injection E2E tests.
  - **Dependencies:** `P08-001`.

---

## Phase 10 — CI/CD, Static Hosting Build, Security/Privacy Review, and Documentation

- [x] **P10-001**: Configure GitHub Actions CI workflows (`.github/workflows/ci.yml`) separating deterministic CPU/SwiftShader jobs from optional hardware-GPU jobs, plus static production bundle verification.
  - **Objective:** Automate `cargo fmt`, `cargo clippy`, `cargo test`, `wasm32` build, ESLint, `tsc`, Vitest, production Vite build (`base: './'`), BIOS-free repo hygiene check, and Playwright tests.
  - **Acceptance Criteria:** Production `dist/` bundle builds reproducibly with relative asset paths suitable for static hosting (GitHub Pages / Cloudflare Pages / local static server) and zero external telemetry/network calls.
  - **Required Tests:** Local execution of full CI gate script (`npm run verify:all`) + static preview server E2E smoke test against `dist/`.
  - **Dependencies:** `P09-001`, `P08-002`.
- [x] **P10-002**: Complete comprehensive documentation (`README.md`, `docs/fidelity-matrix.md`, `docs/persistence-schema.md`, `docs/testing-and-gpu.md`, `docs/legal-and-assets.md`, `docs/architecture.md`) while preserving upstream README heritage.
  - **Objective:** Document quickstart, single-click launch, architecture, exact fidelity matrix (`Implemented` / `Tested` / `Verified` / `Approximate` / `Blocked` / `Deferred`), persistence schema & migrations, test commands, and legal/asset policy.
  - **Acceptance Criteria:** All documentation accurate, consistent with actual commands and test outputs, and free of unverified claims.
  - **Required Tests:** Documentation command & path verification check.
  - **Dependencies:** `P10-001`.

---

## Phase 11 — Final Full-System Verification and Release Readiness Audit

- [x] **P11-001**: Execute full clean-build and end-to-end verification sweep across all test suites, static bundle preview, git history/hygiene scan, and `TODO.MD` status audit.
  - **Objective:** Confirm every acceptance criterion is genuinely satisfied or transparently marked `[!]` where hardware-blocked, with zero uncommitted build junk or proprietary data.
  - **Acceptance Criteria:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `npm run lint`, `npm run typecheck`, `npm run test:unit`, `npm run test:wasm`, `npm run test:perf`, `npm run build`, and `npm run test:e2e` all pass cleanly; git log clean and logically structured.
  - **Required Tests:** Full `npm run verify:all` clean run.
  - **Dependencies:** `P10-002`.
- [!] **P11-002**: Physical discrete/integrated hardware GPU execution & native 60 Hz swapchain frame-pacing verification.
  - **Objective:** Validate WebGPU rendering and native hardware frame timing on physical Vulkan/Metal/D3D12 silicon (`isFallbackAdapter === false`).
  - **Acceptance Criteria:** Execute GPU E2E + frame-time benchmark on a host with physical `/dev/dri` / Metal / D3D12 GPU (`isFallbackAdapter === false`).
  - **Required Tests:** `npm run test:e2e:hardware-gpu` (configured in CI GPU job).
  - **Dependencies:** `P10-001`.
  - **Block Reason:** Current sandbox container has no `/dev/dri` device or physical GPU; WebGPU is verified locally on Chromium 153 Dawn + Google SwiftShader Vulkan 1.3 (`isFallbackAdapter: true`) and Canvas 2D fallback, while hardware-GPU verification is `[!]` Blocked in-sandbox and wired for GPU runners.

---

## Known Limitations & Approximations

1. **Physical Hardware GPU Unavailable in Sandbox (`Blocked`):** `/dev/dri` is not mounted in this Linux container. Real Chromium 153 Dawn WebGPU + WGSL + Vulkan 1.3 rendering is executed and pixel-verified via Google SwiftShader (`isFallbackAdapter: true`), alongside unit mocks and the Canvas 2D compatibility renderer. Native hardware-GPU frame timing (`isFallbackAdapter: false`) requires a GPU-equipped host.
2. **Clean-Room Default Assets (`Approximate` by Legal Design):** Default textures, proportional UI font atlas, geometric button-hint glyphs, and Web Audio synthesis are original procedural reconstructions rather than copyrighted Sony `TEXIMAGE`/`FNTIMAGE`/`SNDIMAGE` ROM data. Users who own dumped hardware may optionally import their authorized BIOS/resource image locally in-browser (`ROMDIR` + LZ `expand()`).
3. **Conservative Opening Banner (`Approximate` by Legal Design):** Default Opening overlay displays a non-infringing clean-room banner rather than Sony Computer Entertainment trademarks.
4. **Illegal-Disc Text Placement (`Approximate`):** As documented in upstream `osdbits/opening.c`, `DoIllegalText` was ported from disassembly without a retail illegal-disc GS dump (`gscmp`) comparison.
5. **Browser Submenu (`Approximate`):** Upstream `osdbits` reconstructs the Main Menu and System Configuration 3D/2D scenes (`menu.c`, `menuback.c`, `menuconfig.c`, `menutext.c`) and leaves the Memory Card `"Browser"` interior unimplemented; `osdbits-wasm` provides a clearly badged OSDSYS-inspired interactive Browser/Tower-Memory showcase view.

---

## Deferred Features

- **Full PS2 Memory Card (`vmc` / `.ps2`) Filesystem Save-Icon 3D Viewer:** Parsing external `.ps2` memory card images and skeletal `icon.sys` 3D character animations inside the Browser submenu is deferred outside core OSDSYS reconstruction scope.
- **Full IOP `libsd` SPU2 Hardware-Exact ADPCM (`SNDIMAGE`) Reverb Emulation:** Default audio uses clean-room Web Audio procedural synthesis, while optional user BIOS/ROM import decodes `TEXIMAGE` (`ROMDIR` + LZ `expand()`); decoding IOP `SNDIMAGE` VAG/ADPCM sample banks and cycle-accurate SPU2 hardware reverb emulation is deferred.
- **Byte-Matching `ee-gcc 2.9-ee-991111` Decompilation Completion:** Upstream `matching/` campaign remains preserved in `matching/` as historical research and is separate from the browser Rust/WASM engine.
