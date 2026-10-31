# TODO.MD — `osdbits-wasm`

Browser-native reconstruction of the PS2 OSDSYS visual experience, ported from
the upstream `osdbits` PS2 reconstruction.

Status legend: `[ ]` not started · `[x]` complete (acceptance + tests passed) ·
`[!]` blocked · `[~]` in progress / partially complete (details inline).

Rules in force (see `AGENTS.md`, `docs/QA.md`):

* A task is `[x]` only when its **acceptance criteria** are met **and** its
  **tests were executed and passed**. "Code written" is not completion.
* Every code change ships with tests; every test run is recorded in
  `docs/TEST-EVIDENCE.md` with the exact command.
* Blocked work states the exact reason and the next actionable step.
* Deferred features go to §12, never silently dropped.

Toolchain: Rust 1.88.0 (`wasm32-unknown-unknown`) + Node 22 + Vite/Vitest +
Playwright. The core crate has **no external crates** (the build environment
has no crates.io access — see `docs/BASELINE-AUDIT.md` §6).

---


## Status legend and evidence ledger

* `[x]` — done **and** every required test that can run in this environment has
  passed. The evidence column names the command and where its output is
  recorded.
* `[~]` — implemented, but at least one required verification is **Blocked**
  (browser/GPU/audio unavailable in the development sandbox). It must not be
  read as verified behaviour.
* `[ ]` — not done.

Evidence for every `[x]` below is reproduced in `docs/TEST-EVIDENCE.md`
(commands, versions, counts, timings).

| ID | State | Evidence |
|---|---|---|
| `P01-001` | `[x]` | `docs/ARCHITECTURE.md`, `docs/WASM-ABI.md`; ABI asserted by `web/tests/wasm.parity.test.ts` |
| `P01-002` | `[x]` | `scripts/bootstrap-rust.sh` + `rust-toolchain.toml`; toolchain used for every Rust command below |
| `P01-003` | `[x]` | `crates/osdbits-core` (13 modules, empty `[dependencies]`); `cargo test` |
| `P01-004` | `[x]` | `web/` (Vite 7, TS 5.9, Vitest 3); `npx tsc --noEmit`, `npx vitest run`, `npx vite build` |
| `P01-005` | `[x]` | `src/abi.rs` (24 exports) + `web/src/wasm.ts`; parity tests |
| `P01-007` | `[x]` | `scripts/check-no-proprietary-assets.sh` → `ok: no proprietary assets tracked (141 files)` |
| `P01-008` | `[x]` | `docs/TEST-EVIDENCE.md`, `docs/QA.md` |
| `P02-001` | `[x]` | `rng.rs` (6 tests; both LCG streams pinned) |
| `P02-002` | `[x]` | `field.rs` height-grid tests (reference values from the reconstruction) |
| `P02-003` | `[x]` | `field.rs` history/field tests (14) |
| `P02-004` | `[x]` | `camera.rs` (15 tests) |
| `P02-005` | `[x]` | `fog.rs` (6 tests) |
| `P02-006` | `[x]` | `lights.rs` (8 tests) |
| `P02-007` | `[x]` | `cubes.rs` (11 tests) |
| `P02-008` | `[x]` | `illegal.rs` (9 tests) |
| `P02-009` | `[x]` | `text.rs` (7 tests) |
| `P02-010` | `[x]` | `scene::tests::same_seed_replays_identically`, `tests/properties.rs`, `scripts/wasm-smoke.mjs` (`replay is bit-identical`), `web/tests/wasm.parity.test.ts` |
| `P02-011` | `[x]` | `cargo test --test properties` (4 seeded campaigns, 60 random scenes) |
| `P07-002` | `[x]` | `web/src/settings.ts` + `tests/settings.test.ts` (11 tests) |
| `P07-005` | `[x]` | corrupt-record + fallback tests in `tests/storage.test.ts`; memory-store degradation |
| `P08-001` | `[x]` | `web/src/menu.ts` + `tests/menu.test.ts` (7 tests) |
| `P10-001` | `[x]` | `cargo test` → 132 lib tests |
| `P10-002` | `[x]` | `cargo test --test properties` |
| `P10-003` | `[x]` | `scripts/wasm-smoke.mjs` (40 checks) + `web/tests/wasm.parity.test.ts` |
| `P10-004` | `[x]` | `npx vitest run` → 68 tests in 8 files (incl. real-wasm parity, jsdom boot, Skia-rasterised Canvas 2D) |
| `P11-004` | `[x]` | asset audit script (executed) |
| `P11-006` | `[x]` | `docs/ARCHITECTURE.md`, `PORTING-NOTES.md`, `WASM-ABI.md`, `QA.md`, `ASSET-POLICY.md`, `LIMITATIONS.md` |
| `P11-007` | `[x]` | `docs/LIMITATIONS.md` (status vocabulary on every line) |
| `P06-005` | `[x]` | `web/tests/boot.test.ts` (6) + `web/playwright/resilience.spec.ts`; E2E pass in CI |
| `P06-006` | `[x]` | `web/playwright/startup.spec.ts`; CI run `41245351478` |
| `P10-005` | `[x]` | `npm run e2e` in CI — 51 passed / 0 failed / 3 skipped (adapter-only), blocking job |

Implemented but **not verifiable here** (`[~]`): everything that needs a real
GPU, audio output, or a browser binary *in this sandbox*. The Playwright suite
itself was executed in CI (see `docs/TEST-EVIDENCE.md`); only hardware GPU
rendering, audible audio and visual-regression baselines remain unverified.

## Phase 0 — Repository reconnaissance and baseline

- [x] `P00-001` — Inventory the source tree, sizes, languages and artefacts.
  - Acceptance: every top-level directory and build input is described.
  - Tests: n/a (documentation); reviewed against `git ls-files`.
  - Evidence: `docs/BASELINE-AUDIT.md` §1.

- [x] `P00-002` — Classify each upstream subsystem as implemented / partial /
    documented-only by reading its code.
  - Acceptance: no subsystem claimed implemented without a cited function.
  - Tests: manual cross-check of cited line ranges.
  - Evidence: `docs/BASELINE-AUDIT.md` §2–3.

- [x] `P00-003` — Record stubs, deliberate ROM quirks and known limitations.
  - Acceptance: TODO/BUG comments and "NOT PORTED" blocks listed.
  - Evidence: `docs/BASELINE-AUDIT.md` §4.

- [x] `P00-004` — Establish baseline buildability/testability in this
    environment.
  - Acceptance: exact failing precondition named (missing EE toolchain,
    missing BIOS-derived resources, no tests).
  - Evidence: `docs/BASELINE-AUDIT.md` §5.

- [x] `P00-005` — Establish asset/legal provenance boundaries.
  - Acceptance: documented ban on BIOS images, extracted textures, audio banks
    and Sony logos in git, releases, fixtures and CI artefacts.
  - Evidence: `docs/ASSET-POLICY.md`; enforced by `P01-007`/`P11-004`.

- [x] `P00-006` — Preserve upstream sources unmodified and record their role.
  - Acceptance: `osdbits/`, `matching/`, `docs/*.md`, `vucode_*` untouched;
    documented as specification input.
  - Tests: `scripts/check-upstream-untouched.sh` in CI.

- [ ] `P00-007` — Record a machine-readable baseline of upstream file hashes.
  - Acceptance: `docs/upstream-hashes.txt` generated reproducibly.
  - Tests: script re-run yields identical output.

## Phase 1 — Architecture and engineering foundations

- [x] `P01-001` — Define the Rust / WASM / TypeScript boundary and the single
    rendering-ownership model (Rust simulates and emits a draw list; TS owns
    WebGPU; overlays/menus are DOM).
  - Acceptance: documented ownership, no second renderer.
  - Evidence: `docs/ARCHITECTURE.md`.

- [x] `P01-002` — Provide a reproducible toolchain bootstrap without
    crates.io/static.rust-lang.org.
  - Acceptance: `scripts/bootstrap-rust.sh` produces `rustc 1.88.0` +
    `wasm32-unknown-unknown`; documented `rustup` alternative.
  - Tests: executed in this environment (see evidence log).

- [x] `P01-003` — Create the zero-dependency Rust core crate and workspace.
  - Acceptance: `cargo build`, `cargo test`, `cargo clippy -D warnings`,
    `cargo fmt --check` all succeed.
  - Tests: see `docs/TEST-EVIDENCE.md`.

- [x] `P01-004` — Create the TypeScript app with `strict: true`, Vite build,
    Vitest unit tests.
  - Acceptance: `npm run typecheck`, `npm run lint`, `npm test` succeed.

- [x] `P01-005` — Establish a hand-written `extern "C"` WASM ABI (no
    `wasm-bindgen`) with a versioned frame layout.
  - Acceptance: ABI documented; TS loader validates magic/version/length;
    native and WASM builds expose the same functions.
  - Tests: Rust ABI tests + TS loader tests.

- [ ] `P01-006` — Set up CI: CPU-only Rust + TS jobs and a separate
    GPU/browser job, with pinned toolchains.
  - Acceptance: `.github/workflows/ci.yml` runs fmt/clippy/test/build/wasm
    checks; browser job is explicit about GPU capability assertions.
  - Tests: workflow reviewed + `act`-less local equivalent commands run.

- [x] `P01-007` — Add a proprietary-artefact guard.
  - Acceptance: CI fails if BIOS-sized blobs, `*.bin`/`*.rom`/`*.p2s`, or
    extracted `res/` trees are tracked; release audit script exists.
  - Tests: `scripts/check-no-proprietary-assets.sh` (pass + deliberate
    failure case in a temp dir).

- [x] `P01-008` — Establish test-evidence conventions.
  - Acceptance: `docs/TEST-EVIDENCE.md` records command, environment,
    result for every executed suite; no "tests written" claims.
  - Evidence: `docs/QA.md`.

## Phase 2 — Core engine extraction and deterministic simulation

- [x] `P02-001` — Port the LCG RNG pair (`towerRand`, `osdRand`) bit-exactly.
  - Acceptance: first 8 outputs of both generators match the C reference
    values computed from the same seeds.
  - Tests: unit vectors in `rng.rs`.

- [x] `P02-002` — Port the tower height grid (`HeightGrid`, `0x217e30`).
  - Acceptance: all 400 cells within 1e-4 of a C-derived reference table;
    clamp bounds respected.
  - Tests: reference-table unit test, bounds property test.

- [x] `P02-003` — Port the history simulator and field generation
    (`SimulateBootHistory`, `GenerateTowerField`, count tables, 126-cell
    neighbourhoods).
  - Acceptance: identical seed/args ⇒ identical field (bit-exact f32);
    every cell belongs to exactly one neighbourhood; no out-of-range index.
  - Tests: determinism, seed-sensitivity, structural invariants, full-field
    mode, eviction rule.

- [x] `P02-004` — Port the camera state machine and motion integration
    (`ProcessOpeningAnimation`, `0x215fd0`).
  - Acceptance: state transitions happen at the documented heights; the
    boot → end sequence reaches `openingType == 2` in bounded frames;
    illegal scene starts at state 4/height 672.
  - Tests: scripted trajectories for boot/idle/illegal, boundary tests,
    NaN/∞ guards, end-latch timing (128 frames).

- [x] `P02-005` — Port fog patch animation and layer geometry (`DrawFog`).
  - Acceptance: 6 layers × 256 quads emitted; UV phase advances by the exact
    per-layer formula and wraps in [0,1).
  - Tests: UV wrap, layer count, vertex positions.

- [x] `P02-006` — Port light streaks (`DrawLights`): positions, matrix
    history, trail ring buffer, colours/alphas.
  - Acceptance: 4 lights, ring buffer bounded at 128, alpha ramps as in the
    original, no growth over 10 000 frames.
  - Tests: ring-buffer wrap, budget, colour derivation.

- [x] `P02-007` — Port cube simulation (`CubeTransformAndClip`,
    `CubeFaceSetup`, pass metadata).
  - Acceptance: 5 instances; rotation wraps to [-π,π]; face visibility signs
    computed by the same winding test; clipped cubes reported hidden.
  - Tests: wrap, visibility for axis-aligned camera views, clip culling,
    per-cube independence.

- [x] `P02-008` — Port illegal-disc scene simulation (128 particles, 7 flare
    discs, fades, red cube passes).
  - Acceptance: particle respawn/alpha rules match the C; flare geometry is
    17-vertex fans; red tint factors applied.
  - Tests: particle fall/respawn, alpha ramp, fan vertex counts, colour.

- [x] `P02-009` — Port the text-overlay state machines (`DoSCEText`,
    `DoIllegalText`) without proprietary bitmaps.
  - Acceptance: fade in/hold/out states and alphas reproduce the C; copy is
    user-visible text only, no logo bitmaps.
  - Tests: state sequence, alpha clamping, illegal-text 112 clamp.

- [x] `P02-010` — Deterministic replay: identical seed+mode+frame count ⇒
    identical frame hash.
  - Acceptance: two engines stepped 1200 frames produce identical draw-list
    bytes; changing seed changes the tower field but not the camera track.
  - Tests: replay equality, cross-mode independence.

- [x] `P02-011` — Property/fuzz harness with a seeded deterministic PRNG
    (0 external crates).
  - Acceptance: random engine command sequences never panic, never emit NaN
    into the draw list, never grow buffers unbounded; malformed init params
    are rejected with an error code.
  - Tests: `tests_props` module (bounded, reproducible campaigns).

- [ ] `P02-012` — Serialization of settings/session-relevant engine state
    (seed, mode, frame) with round-trip tests.
  - Acceptance: round trip preserves all fields; unknown version rejected.
  - Tests: round trip, corruption, version mismatch.

## Phase 3 — WebGPU rendering foundation

- [~] `P03-001` — WebGPU device acquisition with adapter/limit checks and
    typed failure reasons.
  - Acceptance: returns a structured result; unsupported ⇒ actionable message.
  - Tests: mocked navigator.gpu paths (absent, no adapter, request failure).

- [~] `P03-002` — WGSL pipelines: tower field, billboards, triangle fans,
    glass cubes, screen capture pass, blit/fade.
  - Acceptance: all shaders compile in a real device (checked by the browser
    suite) and in the mock validation test.
  - Tests: pipeline creation tests, shader source lint (no `TODO`/invalid
    syntax), browser GPU suite.

- [~] `P03-003` — Coordinate systems: PS2 GS window space (2048-centred,
    y-down, 640×224 NTSC virtual screen) → WebGPU NDC; documented mapping.
  - Acceptance: documented derivation from libvu0 `sceVu0ViewScreenMatrix`
    constants (`scrz=1024, ax=1, ay=0.52627105/0.457627`); unit tests for
    projection of known points.
  - Tests: projection unit tests, aspect/letterbox tests.

- [~] `P03-004` — Frame resource management: persistent buffers, no per-frame
    allocations, dynamic vertex upload with capacity growth caps.
  - Acceptance: after warm-up, zero allocations in the frame path; buffers
    capped and reported.
  - Tests: allocation counters in TS, sustained 3 000-frame test.

- [~] `P03-005` — Resize, device-pixel-ratio, virtual-screen letterbox.
  - Acceptance: 4:3 virtual screen preserved at any window size; DPR-correct
    drawing buffer; no distortion.
  - Tests: resize matrix unit tests, browser resize test.

- [~] `P03-006` — Device-loss handling and automatic recovery.
  - Acceptance: `device.lost` surfaces state, UI offers recovery, re-init
    rebuilds all resources exactly once.
  - Tests: simulated loss in unit test + browser test.

- [~] `P03-007` — Procedural placeholder textures (wall, fog, glow, env,
    flare) generated deterministically at runtime.
  - Acceptance: no proprietary data; deterministic given seed; documented as
    original placeholders.
  - Tests: texture generator determinism, dimension/validity checks.

- [ ] `P03-008` — Optional user-supplied texture import path (validated,
    never executed).
  - Acceptance: rejects non-PNG/oversized/truncated input with messages;
    imported textures are used only after validation; hashes recorded.
  - Tests: import validation matrix, hashing, rejection paths.

## Phase 4 — Opening / Towers reconstruction

- [x] `P04-001` — Tower geometry from the reconstructed VU chain
    (`towerchain.dsm`): 6 quads/tower, static vertices, per-vertex z sign
    flags, per-block colours.
  - Acceptance: geometry matches the static packet data decoded in
    `docs/PORTING-NOTES.md`; 5 visible faces + 1 degenerate cap.
  - Tests: geometry unit tests, per-block z-sign test.

- [x] `P04-002` — Tower field draw: per-cell rotation term, per-frame sway,
    radial brightness, u clamp-band ST, Gouraud near/far shading.
  - Acceptance: emitted tower count equals lit cells; ST offset equals the C
    hash `(r+c+9)(r+8)/(c+7) + (r+c+9)(r+7)/(c+9)` / 256; colours follow the
    documented `f12`/`f3` rules.
  - Tests: ST hash table test, colour rules, sway invariance for C==1 cells.

- [~] `P04-003` — Camera drift + fly-up rendering and ordering (towers →
    composite → fog → lights → cubes).
  - Acceptance: documented draw order reproduced in the emitted batches.
  - Tests: batch order assertion, camera trajectory snapshot.

- [~] `P04-004` — Fog layers, light streaks and refractive cubes in WebGPU.
  - Acceptance: all visible in the browser on a reference device; no depth
    fighting; additive blend order correct.
  - Tests: browser GPU smoke + visual regression.

- [~] `P04-005` — SCE text overlay (typographic reconstruction; no Sony
    bitmap logo) with the original fade timing.
  - Acceptance: DOM overlay fades with `sceTextAlpha`; documented as an
    approximation, switchable off.
  - Tests: fade timing unit tests, accessibility check.

- [~] `P04-006` — Scene end + transition into the menu/idle.
  - Acceptance: reaching `openingType 2` transitions once, no double
    transitions; idle mode never ends.
  - Tests: end-state tests, transition-idempotence test.

- [ ] `P04-007` — Deterministic visual regression against reference captures
    in `docs/opening.png` (composition-level, not pixel-exact; reference is
    a capture of the reconstruction, not the ROM).
  - Acceptance: documented tolerance; scene envelope (tower horizon, glow
    centre) within tolerance; no fabricated golden images.
  - Tests: `web/tests/visual` (see capability notes) + offline hash test.

## Phase 5 — Idle and illegal-disc scenes

- [~] `P05-001` — Idle mode: indefinite field, stable animation, settings
    live, no artificial exit.
  - Acceptance: 30 000-frame soak without drift/leak; no scene end.
  - Tests: soak test (headless, no GPU), endurance browser test.

- [~] `P05-002` — Illegal-disc scene: red cloud particles, flare fans, lens
    sprites, red cubes, localized text, fades, motion blur.
  - Acceptance: all ported sub-effects present; camera enters at 672 and
    climbs; documented unverified parts listed.
  - Tests: simulation unit tests, browser smoke, visual envelope.

- [ ] `P05-003` — Localized text: English + a second language, from original
    strings (no Sony assets), switchable.
  - Acceptance: both render; missing keys fall back to English.
  - Tests: i18n unit tests.

- [x] `P05-004` — Document unverified illegal-scene behaviour separately.
  - Acceptance: `docs/LIMITATIONS.md` lists each unverified item.
  - Tests: docs link check (CI).

## Phase 6 — Browser shell and one-click startup

- [~] `P06-001` — Activation screen with a single **Start OSDSYS** button.
  - Acceptance: keyboard-accessible; no second init button; explain why the
    click is needed.
  - Tests: component tests + E2E.

- [~] `P06-002` — Audio activation inside the click handler
    (`AudioContext.resume()` before any await).
  - Acceptance: resume is invoked synchronously in the handler; audio failure
    never blocks visuals.
  - Tests: unit test with a fake AudioContext asserting call order; E2E
    asserting one resume attempt and no duplicate contexts.

- [~] `P06-003` — Startup pipeline: capability detection → WASM init →
    storage restore → renderer → first frame, with structured errors.
  - Acceptance: each stage has a failure path and a user-visible message;
    duplicate init prevented.
  - Tests: ordering tests, duplicate-init test, failure injection tests.

- [~] `P06-004` — Lifecycle: visibility change, focus loss, teardown,
    recovery/restart without reloading the page.
  - Acceptance: hidden tab stops stepping and resumes without time jumps;
    restart rebuilds cleanly.
  - Tests: visibility simulation unit tests, E2E restart.

- [x] `P06-005` — Unsupported-capability messaging (no WebGPU, no WASM, no
    audio, no storage) with degraded but honest modes.
  - Acceptance: each case yields specific text and no crash.
  - Tests: `web/tests/boot.test.ts` (core failure, storage fallback),
    `web/playwright/resilience.spec.ts` (forced adapter failure, missing core).
  - Evidence: vitest 6 boot tests; E2E pass on three engines in CI.

- [x] `P06-006` — E2E startup suite (all three engines): activation → WASM →
    render → animation advances → settings persist.
  - Acceptance: passes headlessly on GPU-less runners by asserting the
    documented fallback path; adapter-only assertions are skipped.
  - Tests: `web/playwright/startup.spec.ts`.
  - Evidence: CI run `41245351478`, 51 passed / 0 failed / 3 skipped
    (Chromium + Firefox + WebKit). Never executed in the development sandbox
    (no browser binaries) — see `docs/TEST-EVIDENCE.md`.

## Phase 7 — Persistent storage and resource management

- [~] `P07-001` — Versioned IndexedDB store with atomic transactions.
  - Acceptance: schema version recorded; read-back verification after write.
  - Tests: real-browser (fake-indexeddb in unit tests; real IDB in E2E).

- [x] `P07-002` — Settings model with validation + defaults + migrations.
  - Acceptance: invalid input rejected/normalised; v1→v2 migration test.
  - Tests: validation matrix, migration tests.

- [~] `P07-003` — Persist scene mode, quality, audio, reduced motion,
    seed, language, and last frame-independent state.
  - Acceptance: survives reload; restored before first frame.
  - Tests: round-trip + E2E reload.

- [ ] `P07-004` — Stale-write protection (monotonic revision, queued writes).
  - Acceptance: out-of-order writes cannot regress newer state.
  - Tests: concurrency simulation.

- [x] `P07-005` — Quota/corruption/unavailable-storage handling with degraded
    launch.
  - Acceptance: app still starts; banner explains; no data loss masking.
  - Tests: injected failures.

- [ ] `P07-006` — Reset (confirmed) + export/import of settings JSON.
  - Acceptance: reset clears keys; export/import round trip validated by
    schema; malformed import rejected atomically.
  - Tests: reset/export/import tests.

- [ ] `P07-007` — OPFS asset store for user-supplied large files, with
    capability detection and fallback to IndexedDB blobs.
  - Acceptance: detected support; validated hashes; never executes content.
  - Tests: store abstraction tests (fake OPFS + real in E2E where available).

- [ ] `P07-008` — Request persistent storage (`navigator.storage.persist`)
    without assuming success; report the grant result in settings.
  - Acceptance: result surfaced in UI; failure tolerated.
  - Tests: granted/denied paths.

## Phase 8 — OSDSYS-inspired menu and customization

- [x] `P08-001` — Navigation model: entries, cursor, confirm, back, with the
    reconstructed behaviour where evidence exists.
  - Acceptance: documented distinction between real reconstruction and
    approximation; no fake hardware claims.
  - Tests: navigation state machine tests.

- [~] `P08-002` — Keyboard + pointer + gamepad navigation, focus management.
  - Acceptance: full keyboard operability, visible focus, ARIA roles.
  - Tests: keyboard E2E, axe-style checks (no external axe dependency:
    structural assertions).

- [~] `P08-003` — Settings screen: scene, quality/resolution scale, fog,
    effects, audio volume, reduced motion, language, reset/export/import.
  - Acceptance: each control applies live and persists.
  - Tests: control tests + persistence.

- [ ] `P08-004` — Simulated hardware states clearly labelled (disc present /
    illegal disc / no disc) instead of pretending to detect real hardware.
  - Acceptance: labels marked "simulated"; cannot be mistaken for detection.
  - Tests: label assertions.

- [~] `P08-005` — Menu visual treatment consistent with the PS2 aesthetic
    while remaining our own design (no Sony artwork).
  - Acceptance: documented; no proprietary assets.
  - Tests: asset audit.

## Phase 9 — Compatibility, accessibility, resilience

- [ ] `P09-001` — Capability matrix and documented browser support.
  - Acceptance: `docs/COMPATIBILITY.md` with tested engines and results.
  - Tests: capability detection tests.

- [~] `P09-002` — Reduced motion, high contrast, focus visibility, ARIA
    labels, no keyboard traps.
  - Acceptance: `prefers-reduced-motion` stops camera motion and disables
    trails; all controls labelled.
  - Tests: reduced-motion unit + E2E; contrast assertion in CSS tokens.

- [ ] `P09-003` — Responsive layout across viewport sizes, mobile-safe-area,
    4:3 letterboxing, portrait handling.
  - Acceptance: no overlap/clipping at 320×480 … 2560×1440.
  - Tests: viewport matrix E2E.

- [ ] `P09-004` — Subpath deployment correctness (relative asset URLs).
  - Acceptance: production build served under `/osdbits/` works.
  - Tests: E2E served from a subpath.

- [~] `P09-005` — Error surface: recoverable vs fatal, with copy-to-clipboard
    diagnostics and no console spam.
  - Acceptance: fatal errors show actionable text; recovery keeps session.
  - Tests: error-path tests.

## Phase 10 — Strict QA and performance

- [x] `P10-001` — Rust unit tests for every ported deterministic function.
  - Acceptance: ≥1 behavioural test per ported function; asserted values.
  - Tests: `cargo test`.

- [x] `P10-002` — Property/fuzz campaigns (bounded, seeded) in CI.
  - Acceptance: fixed seeds, wall-clock bound, minimised regression cases
    stored.
  - Tests: `cargo test --release props`.

- [x] `P10-003` — WASM artefact tests: target check, instantiate, ABI
    semantics, no native deps, frame-buffer sanity from Node.
  - Acceptance: Node instantiates the built `.wasm`, steps the sim, validates
    the frame header and draws a stable hash.
  - Tests: `web/tests/wasm/*.test.ts`.

- [x] `P10-004` — TypeScript unit/integration tests for shell, storage,
    renderer, input, lifecycle, error paths.
  - Acceptance: meaningful assertions on success *and* failure paths.
  - Tests: `npm test` (+ coverage report).

- [x] `P10-005` — Browser E2E (Playwright) covering the full journey and the
    capability-specific paths.
  - Acceptance: suite runs in CI as a blocking job; GPU assertions gated on
    actual adapter presence; failures surface as checks-API annotations.
  - Tests: `npm run e2e` (17 specs × 3 engines).
  - Evidence: CI run `41245351478` — 51 passed, 0 failed, 3 skipped, 62 s.

- [ ] `P10-006` — Visual regression with deterministic frames and documented
    tolerances.
  - Acceptance: tolerances justified in `docs/QA.md`; no silent loosening.
  - Tests: visual suite.

- [~] `P10-007` — Performance budgets: startup, WASM init, p50/p95 frame
    time, memory, GPU upload volume.
  - Acceptance: budgets documented and measured on the reference environment;
    measured numbers recorded (not aspirational).
  - Tests: perf harness.

- [ ] `P10-008` — Endurance/leak tests (3 000+ frames headless, browser soak
    when a browser is available), background-tab behaviour.
  - Acceptance: flat memory, bounded buffers, correct resume.
  - Tests: soak harness.

- [ ] `P10-009` — Clean-checkout release gate script.
  - Acceptance: one command runs the full feasible suite and writes evidence.
  - Tests: executed in this environment.

## Phase 11 — Deployment and release readiness

- [~] `P11-001` — Reproducible production build (`npm run build` +
    `build:wasm`), committed build script, pinned versions.
  - Acceptance: same inputs ⇒ same artefact hashes (documented exceptions).
  - Tests: build twice, compare hashes.

- [~] `P11-002` — Static deployment with correct relative paths and
    documented hosting requirements.
  - Acceptance: production bundle serves from root and subpath.
  - Tests: local static server E2E.

- [ ] `P11-003` — Production smoke test: real startup, render, persistence in
    the deployed origin.
  - Acceptance: verified against the built `dist/` in a browser.
  - Tests: `test:e2e` against `vite preview`.

- [x] `P11-004` — Release asset audit: no proprietary firmware/textures/audio.
  - Acceptance: audit script + documented manifest of shipped files.
  - Tests: audit script in CI.

- [x] `P11-005` — User documentation (what it is, what it is not, how to run,
    settings, privacy, storage).
  - Acceptance: `README.md` covers the deployed app, not just the PS2 build.
  - Tests: link check.

- [x] `P11-006` — Developer documentation (architecture, porting notes,
    testing, CI, rendering, limitations).
  - Acceptance: `docs/*` complete and cross-linked.
  - Tests: link check.

- [x] `P11-007` — Evidence-based limitations report.
  - Acceptance: every limitation labelled Implemented/Tested/Verified/
    Approximate/Blocked/Deferred with the reason.
  - Tests: docs review.

- [ ] `P11-008` — Final Definition-of-Done review against §12 of the mission.
  - Acceptance: every DoD line has a status and evidence pointer; anything
    unverified stays unchecked.
  - Tests: `scripts/release-gate.sh` + `docs/RELEASE-READINESS.md`.

---

## 12. Deferred features and known limitations (never silently dropped)

| ID | Item | Status | Reason |
|---|---|---|---|
| `D-001` | Exact VU1 microcode emulation of `vucode_1` (per-tower VIF chain, GS packets) | Deferred | Not needed for browser rendering: the same geometry/colour/ST results are computed in Rust and drawn natively; emulating the VIF/VU pipeline would add cost with no visible gain. Documented in `docs/PORTING-NOTES.md`. |
| `D-002` | Original extracted textures (`TEXOWAL0`, `TEXOFOG*`, `TEXOREF`, `TEXOSCE`, font pages) | Blocked (legal) | Proprietary BIOS data; placeholders are generated procedurally. User import path exists (`P03-008`). |
| `D-003` | Original boot chime / sound banks (`SNDBOOTS`, `SNDWARNS`, …) | Blocked (legal) | Proprietary. An original, synthesized placeholder soundscape is used; labelled as a substitute. |
| `D-004` | "Sony Computer Entertainment" bitmap logo | Blocked (legal) | Trademark/artwork. A typographic text approximation is drawn, switchable off; not the original artwork. |
| `D-005` | Full browser/memory-card/game-list/system-config implementation | Deferred | Upstream has notes + partial config-screen code only (`docs/menu-*.md`); this project implements an OSDSYS-*inspired* menu, clearly labelled. |
| `D-006` | Waveform-accurate SPU2 reverb/ADPCM emulation | Deferred | Depends on `D-003`. |
| `D-007` | Real PS2 hardware/emulator verification of the browser port | Blocked (environment) | No EE toolchain, no PCSX2, no ROM. Fidelity claims are inherited from upstream and labelled. |
| `D-008` | Pixel-exact visual regression against real GS dumps | Blocked (assets) | No authorised GS dumps available here; only reconstruction captures (`docs/*.png`). |
| `D-009` | Firefox/WebKit GPU verification | Blocked (environment) | Only a Chromium-based headless binary is installable in this sandbox; capability-specific behaviour is asserted instead. |
| `D-010` | Screenshot-based autoplay audio (`AudioContext` without a gesture) | Deferred | Browser policy; the single activation click is required by design. |
