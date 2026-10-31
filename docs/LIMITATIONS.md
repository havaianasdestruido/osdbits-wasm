# Limitations

Every line is labelled with the status vocabulary from `docs/PORTING-NOTES.md`.
Nothing here is aspirational wording: an item marked **Blocked** means the
verification could not be performed in this environment, not that it is known to
work.

## Fidelity

| Item | Status | Detail |
|---|---|---|
| Scene sequencing, camera state machine, motion integration | Implemented, Tested | Ported 1:1 from `ProcessOpeningAnimation`; unit + property tests. |
| Tower field generation (history, height grid, per-cell colour/ST) | Implemented, Tested | Deterministic and asserted; **Approximate** only in the per-face light constant. |
| VU1 microcode execution | Deferred (`D-001`) | Geometry is recomputed in Rust; the microcode is not executed. |
| Fog layers (geometry/depth/scroll/colours) | Implemented, Tested | Texture *contents* are procedural. |
| Light streaks and trails | Implemented, Tested | Positions, ring sampling and alphas ported. |
| Cube simulation (culling, winding, per-face light) | Implemented, Tested | Ten-pass GS compositing collapsed to one lit pass (**Approximate**). |
| Illegal scene: 128 particles, 7 flare discs, fades | Implemented, Tested | Never compared against a real GS dump (`D-008`). |
| Text fade state machines and screen rects | Implemented, Tested | Glyphs are our own typography (`D-004`). |
| Original textures, fonts, sounds | Blocked (`D-002`, `D-003`) | Not distributable; procedural substitutes. |
| Menu (mode D) | Implemented, Approximate | A reconstruction-*inspired* menu; OSDSYS's browser/memory-card/config screens are not reconstructed (`D-005`). |
| Localisation of the illegal-disc text | Deferred | English only today (`P05-003`). |

## Verification

| Item | Status | Detail |
|---|---|---|
| Rust unit/property tests, clippy, fmt | Verified | Commands and output in `docs/TEST-EVIDENCE.md`. |
| WASM artefact instantiated from Node | Verified | `scripts/wasm-smoke.mjs`: 40+ checks including determinism. |
| TypeScript typecheck, unit/integration tests | Verified | `tsc --noEmit`, 68 Vitest tests (12 against the real wasm, 6 booting the real app in jsdom, 4 rasterising the compatibility renderer). |
| Production bundle builds | Verified | `vite build` output recorded. |
| Real WebGPU rendering | Blocked | No GPU/browser binary in the development sandbox. The renderer is **unverified on hardware**; the compatibility renderer is the only executed path. |
| Browser E2E (Chromium/Firefox/WebKit) | Verified in CI, Blocked here | 51 passed / 0 failed / 3 skipped (the adapter-only cases) on GitHub Actions run `41245351478`; the job is blocking. No browser binaries can be installed in the development sandbox, so the suite never ran locally. |
| Visual regression against GS dumps | Blocked (`D-008`) | No authorised captures. |
| Audio output | Blocked | Cannot be heard or measured here; the synthesis path is unit-checked only for capability detection. |
| Performance budgets in a browser | Partial | Core frame cost measured under Node (`p50 0.001 ms`, `p95 0.3 ms` for step+draw); GPU frame time unmeasured. |

## Behaviour that is intentionally different

* The opening's fade-to-black (`sub_218bd0`) and letterbox bars are drawn as
  screen-space quads; the DOM text sits above them, exactly as `DoText` runs
  after the fade in the original.
* The boot sequence ends once the core latches `openingType == 2`; the app then
  keeps the last frame and offers restart / idle / illegal from the toolbar.
  An idle scene (mode B) never ends.
* Reduced motion sets `camera_motion = false`, which holds the camera pose in
  every scene — including the opening's scripted ascent, which is driven by the
  same integrator — while the fog, light, cube and text animation keeps running.
  Pinned by two Rust tests and two wasm parity tests.
* The core snapshots its options in `osd_init()` (`osd_set_option()` only writes
  the global copy), so changing any simulation option restarts the scene. The
  shell waits for the stored settings before starting the first scene, so the
  first frame is identical to a reloaded one.
* `prefers-reduced-motion` is honoured by defaulting `reducedMotion` on first
  run; the user can override it.
* Hiding the tab suspends audio. The simulation stops because the browser
  throttles `requestAnimationFrame` (not because of our code), and the loop
  clamps the elapsed time on return so it does not fast-forward.
