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
| TypeScript typecheck, unit/integration tests | Verified | `tsc --noEmit`, 56 Vitest tests (10 of them against the real wasm). |
| Production bundle builds | Verified | `vite build` output recorded. |
| Real WebGPU rendering | Blocked | No GPU/browser binary in the development sandbox. The renderer is **unverified on hardware**; the compatibility renderer is the only executed path. |
| Browser E2E (Chromium/Firefox/WebKit) | Blocked (`D-009`) | Playwright specs are committed but have never run here. |
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
* Reduced motion freezes the camera (`camera_motion = false`) but keeps the fog,
  light and cube animation alive; it also stops simulation stepping while the
  tab is hidden.
* `prefers-reduced-motion` is honoured by defaulting `reducedMotion` on first
  run; the user can override it.
