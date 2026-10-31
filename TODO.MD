# osdbits-wasm implementation ledger

Updated: 2026-10-31. **Not release-ready.** Status: `[ ]` open, `[x]` acceptance/tests passed, `[!]` blocked. Open items explicitly marked “active” are in progress. Never infer browser/GPU verification from CPU tests. See `docs/BASELINE.md`, `docs/SOURCE-INVENTORY.md`, and `docs/TEST-EVIDENCE.md`.

## Phase 0 — Repository reconnaissance and baseline

- [x] `P00-001` — Inventory all tracked sources, assets, scripts and visible history.
  - Acceptance: 141-file inventory with hashes; historical sources retained.
  - Tests: Compare inventory against git ls-files; inspect source gap markers.
  - Dependencies: None.

- [x] `P00-002` — Record actual feature/build/test baseline.
  - Acceptance: Separate upstream claims from locally reproduced results; identify stale README and missing SDK.
  - Tests: make -C osdbits -n; tool/test discovery.
  - Dependencies: P00-001.

- [x] `P00-003` — Establish legal and asset boundaries.
  - Acceptance: No firmware fetched; missing license and inherited screenshot rights explicitly flagged.
  - Tests: Review tracked resource paths and documentation.
  - Dependencies: P00-001.

- [!] `P00-004` — Obtain redistribution clarification.
  - Acceptance: Document authorized code/asset licensing before release.
  - Tests: Provenance audit and release-content review.
  - Dependencies: P00-003.
  - Blocker: Upstream licensing/asset authorization not established; obtain clarification before redistribution.

## Phase 1 — Architecture and engineering foundations

- [x] `P01-001` — Document Rust core / TypeScript renderer ownership and evidence policy.
  - Acceptance: One primary renderer; no DOM in simulation; parameter evidence classes.
  - Tests: Architecture review against real source pipeline.
  - Dependencies: P00-002.

- [x] `P01-002` — Add portable synthetic test harness and controlled tooling.
  - Acceptance: Dependency-locked formatting/lint/unit commands work without firmware.
  - Tests: Execute all Python harness checks and record versions.
  - Dependencies: P00-002.
  - Evidence: portable gate passed locally; see `docs/TEST-EVIDENCE.md`.

- [!] `P01-003` — Provision Rust and browser toolchains.
  - Acceptance: Native Rust and wasm32 compilation plus browser launch demonstrated.
  - Tests: rustc/cargo versions; minimal target and browser execution.
  - Dependencies: P01-001.
  - Blocker: No rustc/cargo/rustup or browser executable installed. Provision authorized toolchains before native/WASM/browser gates.

- [x] `P01-004` — Add CI separating portable and GPU gates.
  - Acceptance: CPU checks run without proprietary dependencies; GPU gate explicit.
  - Tests: Execute equivalent CPU commands locally; parse/assert workflow configuration. Remote Actions execution is not claimed.
  - Dependencies: P01-002.

- [ ] `P01-005` — Create reproducible frontend and WASM build.
  - Acceptance: Strict TypeScript, locked packages, fixed toolchain; complete runtime bundle.
  - Tests: Clean native/WASM/TS build; artifact inspection.
  - Dependencies: P01-003.

## Phase 2 — Core engine extraction and simulation

- [ ] `P02-001` — Port sourced PRNG and brightness grid.
  - Acceptance: Seeded bounded outputs preserve documented integer/f32 semantics.
  - Tests: Known vectors, boundaries and reproducible property tests.
  - Dependencies: P01-003.

- [ ] `P02-002` — Port boot-history tower generation.
  - Acceptance: 21 slot/count/sprawl behavior separated from test full-field mode.
  - Tests: Fixed synthetic histories; occupancy/count bounds; deterministic replay.
  - Dependencies: P02-001.

- [ ] `P02-003` — Port camera and opening state machine.
  - Acceptance: Explicit valid states, NTSC/PAL source timing, disc-state simulation labels.
  - Tests: All transitions; zero/negative/NaN/infinite/large dt; fixed-step partition tests.
  - Dependencies: P02-001.

- [ ] `P02-004` — Define frame inspection and validated serialization ABI.
  - Acceptance: Versioned bounded state, stable JS/WASM types; malformed data rejected.
  - Tests: Native round trips, browser WASM instantiation and ABI failure tests.
  - Dependencies: P02-002, P02-003, P01-005.

- [ ] `P02-005` — Add property and bounded fuzz campaigns.
  - Acceptance: No uncontrolled allocations/panics for supported inputs; regression seeds retained.
  - Tests: CI fixed-seed event and binary-input campaigns; minimized regression replay.
  - Dependencies: P02-004.

## Phase 3 — WebGPU rendering foundation

- [ ] `P03-001` — Acquire adapter/device and compile WGSL.
  - Acceptance: Clear unsupported path; errors surfaced; one device owner.
  - Tests: Real adapter, shader/pipeline/bind-group/render-pass tests; negative paths.
  - Dependencies: P01-005.

- [ ] `P03-002` — Port coordinates, projection and GPU geometry.
  - Acceptance: Explicit GS-to-WebGPU axes/depth conversion; bounded reusable buffers.
  - Tests: Matrix vectors, winding/depth, buffer bounds and rendered geometry checks.
  - Dependencies: P03-001, P02-004.

- [ ] `P03-003` — Implement resize/DPR/quality and resource disposal.
  - Acceptance: Clamped valid dimensions; no resource leak through repeated resize.
  - Tests: Invalid sizes/device limits, real resize stress, disposal counters.
  - Dependencies: P03-002.

- [ ] `P03-004` — Recover device loss.
  - Acceptance: No duplicate loops/devices; supported recovery without page reload.
  - Tests: Real device.destroy recovery and rejected replacement tests.
  - Dependencies: P03-003.

## Phase 4 — Opening / Towers reconstruction

- [ ] `P04-001` — Render sourced tower placement and camera motion.
  - Acceptance: Port field, drift/fly-up and source state completion; annotate unsourced values.
  - Tests: Deterministic poses and geometry captures.
  - Dependencies: P02-003, P03-002.

- [ ] `P04-002` — Port fog, streaks and feedback glow.
  - Acceptance: Ordered half-width feedback and atmosphere reconstructed or deviations disclosed.
  - Tests: Pass ordering, deterministic distribution, bounded GPU memory.
  - Dependencies: P04-001.

- [ ] `P04-003` — Port per-object cube refraction.
  - Acceptance: Back/front faces sample separate scene snapshots; not plain alpha boxes.
  - Tests: Real GPU pass validation; fixed-pose glass captures.
  - Dependencies: P04-002.

- [ ] `P04-004` — Implement lawful overlay and transition.
  - Acceptance: Original placeholder default; no unauthorized logo/fonts; automatic completion.
  - Tests: Fade timeline boundary tests; one-click journey to next scene.
  - Dependencies: P04-003, P06-002.

- [!] `P04-005` — Compare with authorized reference evidence.
  - Acceptance: Matched seeds/poses/timing and justified explicit visual tolerances.
  - Tests: Original-vs-port comparison report; do not invent goldens.
  - Dependencies: P04-004.
  - Blocker: No authorized matched ROM/GS captures supplied. Historical screenshots are not matched test goldens.

## Phase 5 — Idle and illegal-disc scenes

- [ ] `P05-001` — Provide indefinite idle field.
  - Acceptance: No artificial exit or simulation growth.
  - Tests: Bounded-state long replay and real GPU endurance.
  - Dependencies: P04-003.

- [ ] `P05-002` — Port illegal-disc fog/flare/lens/glass.
  - Acceptance: Source-grounded red scene, localization and fades; unknowns labeled.
  - Tests: Particle/flare unit tests and deterministic GPU captures.
  - Dependencies: P04-003.

- [ ] `P05-003` — Validate scene switching and source fidelity.
  - Acceptance: No leaked resources; no unsupported ROM-accuracy claim.
  - Tests: Switch stress and authorized illegal-disc reference comparison.
  - Dependencies: P05-001, P05-002.

## Phase 6 — Browser shell and one-click startup

- [ ] `P06-001` — Implement activation and silent-safe audio lifecycle.
  - Acceptance: resume invoked synchronously in initial click; failure does not block visuals.
  - Tests: Click-order, rejected resume, suspension, duplicate context and cleanup tests.
  - Dependencies: P01-005.

- [ ] `P06-002` — Initialize WASM/WebGPU/resources in one journey.
  - Acceptance: One click starts animation automatically; retries without reload.
  - Tests: Real-browser E2E; duplicate clicks; initialization error/retry tests.
  - Dependencies: P06-001, P03-001, P02-004.

- [ ] `P06-003` — Handle visibility, input and restart.
  - Acceptance: No giant background timestep; single active loop; reset reproducible.
  - Tests: Visibility/keyboard/restart E2E and listener disposal tests.
  - Dependencies: P06-002.

## Phase 7 — Persistent storage and resource management

- [x] `P07-001` — Harden inherited resource decoder and both CLI entry points.
  - Acceptance: Bounded input/output, valid ROMDIR spans/names and safe LZ references; shared implementation.
  - Tests: Synthetic literal/backref/ROMDIR, malformed cases, fixed-seed fuzz, CLI and original-C parity.
  - Dependencies: P01-002.
  - Evidence: portable gate passed locally; see `docs/TEST-EVIDENCE.md`.

- [ ] `P07-002` — Implement versioned IndexedDB migrations.
  - Acceptance: Validate records, atomic saves verified by readback, explicit corruption/quota fallback.
  - Tests: Real IDB success/failure/migration/interrupted write tests.
  - Dependencies: P01-005.

- [ ] `P07-003` — Persist settings with stale-write protection.
  - Acceptance: Latest update wins and failures allow degraded launch.
  - Tests: Concurrent saves, readback, reload and genuine persistent-profile browser restart.
  - Dependencies: P07-002, P06-002.

- [ ] `P07-004` — Add export/import/reset and storage policy UI.
  - Acceptance: Bounded validated JSON; confirmed reset; eviction caveat; persistence request handled.
  - Tests: Malformed/oversize imports, round trip, cancelled/accepted reset and denied request.
  - Dependencies: P07-003.

- [ ] `P07-005` — Support authorized optional resources and hashes.
  - Acceptance: Default needs no BIOS; bounded imports never execute code; integrity checked; OPFS only if justified.
  - Tests: Synthetic imports, hash mismatch, unsupported types, quota and interrupted-file tests.
  - Dependencies: P07-001, P07-002.

## Phase 8 — OSDSYS-inspired menu and customization

- [ ] `P08-001` — Port evidenced menu and navigation subset.
  - Acceptance: Use existing menu sources, not just notes; distinguish simulated hardware.
  - Tests: Keyboard/pointer navigation, focus, transition and configurable-entry tests.
  - Dependencies: P06-003.

- [ ] `P08-002` — Add faithful defaults and customization.
  - Acceptance: Scene/camera/speed/quality/fog/audio/color/scaling settings validated.
  - Tests: Boundary tests, reload persistence and restored defaults.
  - Dependencies: P08-001, P07-004.

- [ ] `P08-003` — Expose known reconstruction limits.
  - Acceptance: No fake memory-card/disc functionality presented as real hardware.
  - Tests: UI text and behavior E2E review.
  - Dependencies: P08-001.

## Phase 9 — Compatibility, accessibility, and resilience

- [ ] `P09-001` — Honor reduced motion and accessible controls.
  - Acceptance: Visible focus, labels, keyboard access and motion-safe default.
  - Tests: Browser reduced-motion, keyboard-only and accessibility checks.
  - Dependencies: P08-002.

- [ ] `P09-002` — Test browser capability matrix.
  - Acceptance: Chromium/Firefox/WebKit assert supported GPU or documented fallback.
  - Tests: Engine-specific E2E; record adapter/browser versions.
  - Dependencies: P06-003, P03-004.

- [ ] `P09-003` — Validate root/subpath deployment and failures.
  - Acceptance: Relative runtime paths; offline/storage/audio/GPU failures recover.
  - Tests: Built-artifact HTTP root/subpath E2E; injected load failures.
  - Dependencies: P09-002, P07-005.

## Phase 10 — Strict QA and performance

- [ ] `P10-001` — Establish deterministic visual regression suite.
  - Acceptance: Fixed seeds/steps/viewports, explicit justified pixel/perceptual tolerances.
  - Tests: Tower/fog/streak/cube/particle/overlay/fade/final-state capture comparisons.
  - Dependencies: P04-004, P05-002.

- [ ] `P10-002` — Measure budgets and sustained stability.
  - Acceptance: Declare machine/resolution; startup, p50/p95/p99 frame time and bounded memory/GPU resources.
  - Tests: Endurance, resize/switch/background tests and resource counters.
  - Dependencies: P09-003.

- [ ] `P10-003` — Execute complete clean-checkout release gate.
  - Acceptance: Formatting/lint/types/native/WASM/unit/property/fuzz/browser/storage/visual all accounted for.
  - Tests: Full clean-checkout suite; no missing required capability called passing.
  - Dependencies: P10-001, P10-002, P02-005.

## Phase 11 — Deployment and release readiness

- [ ] `P11-001` — Audit production bundle and provenance.
  - Acceptance: No firmware/extracted resources/private references; rights clarified.
  - Tests: Artifact-content scan and license/dependency review.
  - Dependencies: P00-004, P10-003.

- [ ] `P11-002` — Deploy static application and test actual origin.
  - Acceptance: Working one-click startup and settings persistence on production origin.
  - Tests: Production smoke, genuine restart persistence and subpath verification.
  - Dependencies: P11-001.

- [ ] `P11-003` — Publish user/developer/limitations documentation.
  - Acceptance: Setup, supported hardware, imports, storage, deviations and measured evidence accurate.
  - Tests: Reproduce documented commands; reconcile every mandatory gate with TODO.
  - Dependencies: P11-002.

## Current next actions

- Provision a usable Rust compiler, wasm32 target and browser environment (`P01-003`); the current sandbox lacks these tools and ordinary outbound access is restricted.
- Implement sourced deterministic logic (`P02-001`–`P02-004`) with executed native AND WASM tests before adding render integration.
- Resolve upstream licensing and obtain authorized matched references before fidelity/release claims.

## Deferred features and known limitations

- Browser engine/rendering, all five usable modes, persistent storage, WASM, audio lifecycle and deployment are **not implemented** in this initial checkout; open phases above are mandatory, not quietly waived.
- Real PS2 memory-card management, physical disc detection, BIOS execution and RTC/NVRAM writes are outside the browser reconstruction; any hardware states must be labeled simulated.
- Exact logo, font and soundtrack redistribution is deferred pending authorization. Default assets must be original/procedural; silence is valid.
- OPFS remains conditional on measured large-file requirements; no competing renderer or emulator backend planned.
- PS2 SDK builds, proprietary libgpu2 replay, original-ROM comparisons, real-GPU behavior and physical-device tests have not been reproduced in this environment.
- Existing `ps2expand.c` remains unsafe for untrusted data and is preserved as historical code, not an import API. Harden separately before exposing it.
- No performance or 60 FPS claim; no browser-restart persistence claim; no release-ready claim.
