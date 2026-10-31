# osdbits-wasm — Master Engineering Plan (`TODO.MD`)

Status legend:
- `[ ]` not started / in progress
- `[x]` completed — acceptance criteria satisfied **and** the required tests executed successfully
- `[!]` blocked (reason stated inline)

Completion vocabulary used below: **Implemented** (code exists), **Tested** (executed and passing),
**Verified** (criteria independently demonstrated), **Approximate** (reconstructed without evidence
of exact fidelity), **Blocked** (cannot be verified here), **Deferred** (not implemented).

Execution evidence for every `[x]` below: `docs/TESTING.md`.

---

## Phase 0 — Repository reconnaissance and baseline
- [x] `P00-001` — Inventory the source tree, build requirements and provenance.
  - Acceptance: every C/asm/Python file catalogued; nothing deleted or modified.
  - Evidence: `docs/PHASE0-RECONNAISSANCE.md`. `git status` shows `osdbits/` untouched.
- [x] `P00-002` — Read all analysis documents and reference screenshots.
  - Acceptance: pass orders, blend modes, work-buffer roles, state machines, pad/audio protocols documented.
  - Evidence: `docs/PHASE0-RECONNAISSANCE.md` §2.
- [x] `P00-003` — Provision Rust 1.88 (native + `wasm32-unknown-unknown`) and a WebGPU-capable headless Chromium outside the repo tree.
  - Evidence: `rustc 1.88.0`, `cargo build --target wasm32-unknown-unknown`, Chromium 150 acquires a device.
- [x] `P00-004` — Create this document with unique IDs, acceptance criteria, tests, dependencies.
  - Evidence: this file.

## Phase 1 — Architecture and engineering foundations
- [x] `P01-001` — Hybrid Rust/WASM + TypeScript workspace with strict TypeScript.
  - Acceptance: `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes` on; scripts for build/test/lint wired.
  - Tests: `npm run typecheck` clean; `npm run build` succeeds.
- [x] `P01-002` — ROMDIR parser + `expand()` LZSS decoder in Rust with bounds checking.
  - Acceptance: byte-exact port of `ps2expand.c` semantics; malformed input never panics.
  - Tests: 3 tests — structured round-trip, **50 randomized property round-trips**, **200 malformed blobs**.
- [x] `P01-003` — PS2 texture decoders + procedural clean-room asset generator.
  - Acceptance: formats 3/4/5, PSMCT16 (+ mipmap), PSMT4+CLUT, KABE tiling, grey page + complement, font pages, rod/cube meshes, 5-cube table.
  - Tests: dimension and complement invariants; metric spot-checks vs the docs.
  - Note: **Approximate** for pixel content — these are original assets, see `docs/LIMITATIONS.md` §4.

## Phase 2 — Rust/WASM core
- [x] `P02-001` — Deterministic math, `osdRand`, `libvu0` primitives, MatrixDrive.
  - Tests: LCG sequence, trig identity over 500 random angles, stack semantics, camera/projection invariants.
- [x] `P02-002` — Pad cooker + sound mailbox.
  - Acceptance: ROM-exact region swap, repeat rule and quirk, 128-entry ring, 60-tick reverb ramp.
  - Tests: **exhaustive 65 536-pair swap check**, **400-frame randomized repeat parity**, mailbox/ramp test.
- [x] `P02-003` — Flat C ABI + zero-copy render command buffer.
  - Acceptance: `GpuVertex` 40 bytes, `RenderPassCmd` 40 bytes, `AudioMailboxEntry` 8 bytes; typed-array views from JS.
  - Tests: exercised by all 9 WASM tests and the whole E2E suite.

## Phase 3 — WebGPU rendering pipeline
- [x] `P03-001` — Device/adapter manager, targets, resize, device-loss recovery.
  - Acceptance: `screen` / `wb3` / `wb4` / depth targets; DPR-aware resize; automatic rebuild on loss with backoff, simulation keeps running.
  - Tests: E2E device-loss test; resize test.
  - Blocked part: sustained rendering (see `P10-003`).
- [x] `P03-002` — WGSL shaders + pipeline cache for GS blend/depth/wrap states.
  - Acceptance: modes opaque / 0x44 / 0x48 / 0x42 / FIX-additive / FIX-blend / dest-alpha, per-primitive CLAMP vs REPEAT, GS `0x80 = 1.0` preserved.
  - Tests: pipelines compile and execute inside the E2E runs on a real device.
- [x] `P03-003` — Frame graph executor + pixel readback.
  - Acceptance: consumes the command stream, switches targets inside one encoder, performs capture copies, presents via a full-screen triangle.
  - Tests: E2E asserts first-frame draw/pass/vertex counts; readback used opportunistically where the device survives.

## Phase 4 — Mode A (Opening) and Mode B (Idle)
- [x] `P04-001` — Camera state machine, states 0..7, idle mode.
  - Tests: state 7 reached with `cam_pos.z = 1160`, idle stays bounded and never transitions.
- [x] `P04-002` — Tower field from the 21-slot boot history, radial grid, per-face lighting.
  - Acceptance: 21×6 neighbourhoods tile all 126 cells exactly once; 0/6/126 active-tower cases; `vucode_1`-form shading.
  - Tests: mapping test + the assertion that the table tiles the field.
- [x] `P04-003` — Fog layers, lights + trails, 5 ten-pass refractive cubes, fly-up blur, fades, SCE overlay.
  - Tests: command stream contains draws for all of them in every mode run; illegal/menu/opening emit distinct geometry.
  - **Approximate**: pass counts and mesh complexity, see `docs/LIMITATIONS.md` §5.

## Phase 5 — Mode C (Illegal disc)
- [x] `P05-001` — Red flare rings + lens sprites, 128-patch red fog, red glass cubes, localised prompt, fades.
  - Tests: `illegal` unit test (counts, wrapping, curves); WASM test emits geometry for mode 2; E2E mode-switch test.
  - **Approximate**: cube pass table collapsed relative to `illegalCubePasses`.

## Phase 6 — Mode D (Menu, System Configuration, browser)
- [x] `P06-001` — Software RTC, 7 orbs + 50-sample trails, KABE tunnel, blue composite, zoom blur, vignette.
  - Tests: clock rollover/hold; orb trail length and entry fade; command stream in menu mode.
  - **Approximate**: zoom-blur extents, vignette radii simplified.
- [x] `P06-002` — 12 rods + 5 cubes, 90-frame choreography, selection colours, confirm kick.
  - Tests: 90-frame state progression, cube colour easing to `{0,150,200}`, `-0.1` kick.
  - **Approximate**: 5-pass rods / 8-pass cubes drawn with documented pass kinds but simplified meshes.
- [x] `P06-003` — 2D OSD text engine, localised strings, item list, value editors, Clock Adjustment, top bar, letterbox, hint bar, memory-card browser.
  - Tests: tokenizer/escapes, non-ASCII panic regression, leap years, navigation into and out of config, clock edit + cancel, E2E flow reaching `system_config` and `browser`.
  - **Deferred**: Shift-JIS kanji.

## Phase 7 — One-click activation and audio
- [x] `P07-001` — Web Audio engine driven by the OSD mailbox.
  - Acceptance: `resume()` synchronously in the gesture; 8 BGM slots; SE 4/5/6/10; reverb send from the ramp; graceful failure.
  - Tests: 5 audio tests incl. call-order proof.
  - **Approximate**: the beds are original synthesized material.
- [x] `P07-002` — Activation screen + single-click boot.
  - Acceptance: capability badges resolved before the click; one click unlocks audio, creates the device, instantiates WASM, restores settings, starts the loop.
  - Tests: E2E activation test; no second initialisation step exists.

## Phase 8 — Mode E (Customisation)
- [x] `P08-001` — Panel with scene/camera-adjacent controls, appearance, audio, tower presets, resources, settings JSON, diagnostics.
  - Acceptance: every control applies live through the WASM setters; panel reachable by keyboard (F2).
  - Tests: E2E keyboard test, reload-restoration test, reset test, import tests.
  - **Deferred**: free-orbit camera and a per-slot tower editor UI beyond the browser screen's LEFT/RIGHT stepping.

## Phase 9 — Persistent storage
- [x] `P09-001` — Versioned IndexedDB + OPFS with mandatory read-back verification.
  - Acceptance: verification of schema/revision/checksum; migration; corruption quarantine; stale-write protection; degraded mode that never claims success; export/import/reset.
  - Tests: 9 storage tests + E2E reload and reset tests.
  - Note: browser-*context* persistence is verified; a genuine browser **restart** test is not executable here (no persistent profile harness) — recorded as not run.

## Phase 10 — QA
- [x] `P10-001` — Rust unit/property/fuzz suites.
  - Evidence: `cargo test --workspace` → **25 passed**; `cargo clippy … -D warnings` clean; `cargo fmt --check` clean.
- [x] `P10-002` — TypeScript unit + WASM + storage + audio suites.
  - Evidence: `npx vitest run` → **24 passed** (9 WASM, 9 storage, 5 audio, plus the tokenizer/UI tests inside Rust).
- [!] `P10-003` — Playwright E2E + visual regression + endurance/performance.
  - **Executed and passing**: 11 E2E tests covering activation, rendering evidence, device-loss degradation, mode switching, menu flow, persistence across reload, keyboard/pause, resize, import rejection, ROMDIR import, reset.
  - **Blocked**: sustained GPU execution, pixel-based visual regression against reference captures, and frame-time percentile budgets. Root cause: this sandbox's Chromium destroys the WebGPU device after 2–4 frames; reproduced with a minimal clear-only loop across four flag sets (`docs/TESTING.md` §5). The app's degradation path is verified instead.
  - **Not run**: multi-engine (Firefox/WebKit) E2E — only Chromium is available here and only it exposes WebGPU.

## Phase 11 — Documentation and release readiness
- [x] `P11-001` — `README.md`, `docs/ARCHITECTURE-WASM.md`, `docs/LIMITATIONS.md`, `docs/TESTING.md`, provenance notes.
  - Acceptance: every documented command was executed; legal boundaries stated.
  - Evidence: this file + the docs; `npm run build` + all suites green; live preview served.
- [ ] `P11-002` — Deploy to a public static host and run the production smoke + persistence test against that origin.
  - **Not done**: no deployment target is available from this environment. Blocked on hosting credentials/choice.

---

## Deferred features and known limitations (summary)

Full detail in `docs/LIMITATIONS.md`.

1. **No Sony assets in the repo.** Default visuals/fonts/audio are original procedural material;
   optional user BIOS/ROMDIR import is decoded locally and never leaves the tab.
2. **Shift-JIS / kanji** (`FONTM`, `KROM`, `FNTEX000/001`) deferred; non-ASCII maps to `?` rather
   than indexing past the atlas (regression-tested).
3. **Glass fidelity**: opening/illegal cubes and the config rods/cubes reproduce the documented
   pass *structure* (refraction, emboss, mask composite, work-buffer ping-pong) with simplified
   mesh and pass counts.
4. **Tower lattice**: spacing/positions are an approximation; density, per-column depth, radial
   brightness falloff and the boot-history→height mapping are faithful. `histCells` is
   reconstructed to satisfy the documented 126-cell tiling invariant because the real table is
   zero in the static image and no BIOS is available here.
5. **Hardware-only behaviour** (MechaCon certification, `SYSTEM.CNF`, `LoadExecPS2`, NVRAM writes)
   is out of scope and represented by Mode C / the config editors.
6. **Genuine browser-restart persistence** and **multi-engine E2E** are not verified here.
7. **Performance budgets** on real hardware are not established; only the CPU-side simulation cost
   (≈0.14 ms/frame) is measured.
