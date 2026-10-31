# Architecture

```
┌──────────────────────── browser tab ────────────────────────┐
│  web/src/app.ts      DOM, settings form, menu, boot flow     │
│  web/src/engine.ts   fixed-step loop, renderer fallback      │
│        │                 │                    │              │
│        │                 │                    └── audio.ts    │
│        │                 │                        (WebAudio)  │
│        │                 ├── gpu/renderer.ts + shaders.ts     │
│        │                 │      (WebGPU, WGSL)                │
│        │                 └── canvas2d.ts (compatibility)      │
│        │                                                     │
│        └── wasm.ts ──► wasm module (Rust, no imports)         │
│                │                                             │
│                └── frame.ts parses the draw list              │
│                                                              │
│  storage.ts (IndexedDB → localStorage → memory) + settings.ts │
└──────────────────────────────────────────────────────────────┘
```

## The split

**Rust (`crates/osdbits-core`) owns behaviour and geometry.** It is
deliberately dependency-free (crates.io is unreachable in the development
environment and the core has to stay portable), runs natively for tests and as
`wasm32-unknown-unknown` for the browser, and never touches the DOM, the GPU or
the clock. Given `(mode, seed, options, frame index)` it produces the same
pixels' worth of triangle data every time.

Modules:

| Module | Responsibility |
|---|---|
| `math` | row-major `Mat4`, `apply`/`mul`, `rot_*`, `camera_matrix`, `project_ndc`, GS constants |
| `rng` | the two reconstruction LCGs (`TowerRand`, `OsdRand`) |
| `camera` | the `ProcessOpeningAnimation` state machine and motion integration |
| `field` | boot-history simulator, radial height grid, tower field generation |
| `fog` | 6-layer 17×17 additive fog patch animation and UVs |
| `lights` | 4 light streaks: positions, history ring, 128-sample trail |
| `cubes` | 5 cube instances, per-face winding test, colour/UV modes, pass tables |
| `illegal` | 128 red-cloud particles, 7 flare discs, fades, lens sprites |
| `text` | SCE and illegal text fade state machines and screen rects |
| `frame` | the version-1 draw-list writer and hash |
| `scene` | combines everything into one frame per `step()`/`draw()` |
| `abi` | the `extern "C"` exports (`docs/WASM-ABI.md`) |

**TypeScript owns everything the browser must own:** WebGPU device and
pipelines, texture upload, audio synthesis, persistence, focus management and
the single-activation flow.

## One frame

1. `engine.tick()` accumulates wall-clock time and calls `core.step()` up to
   four times (fixed 50/60 Hz from the core, scaled by the animation-speed
   setting; reduced motion skips stepping entirely).
2. `core.draw()` returns one flat `Float32Array`: header → batch records →
   vertices → cube records.
3. `frame.ts` validates it (magic, version, offsets, ranges, finiteness) and
   throws `FrameError` on anything inconsistent.
4. The renderer walks the batches, maps `(prim, tex, blend)` onto pipelines and
   draws. Cube records are expanded to lit triangles at the position the core
   marked (`H_CUBE_AFTER_BATCH`).
5. The app updates the DOM text overlays from the header alphas and the
   diagnostics panel from `osd_state()`.

## Failure handling

| Failure | Behaviour |
|---|---|
| WebGPU missing or adapter refused | `Canvas2dRenderer`, banner explains, no reload |
| WebGPU device lost / draw throws | destroy, fall back to Canvas 2D once, keep running |
| Canvas 2D draw throws too | loop stops, fatal banner with the message, Retry button |
| wasm fetch/instantiate/ABI mismatch | start button disabled, banner with the exact reason |
| Panic inside the core | hook stores the message; `osd_last_error()` surfaces it |
| Storage unavailable | memory store, UI says settings will not persist |
| Settings record corrupt | copy kept under `osdbits.settings.v1.corrupt`, defaults restored |

## Determinism

There is no wall clock, no `Math.random`, no hash-map iteration and no
floating-point reduction order that depends on the host in the simulation path.
`frame_hash()` (FNV-1a over the frame words, −0 normalised to +0) is exposed
through the ABI and asserted in Rust tests, in the Node smoke test and in the
browser parity test.

Deviations from the reconstruction are listed in `docs/PORTING-NOTES.md` and the
status vocabulary (Implemented / Tested / Verified / Approximate / Blocked /
Deferred) is used consistently there and in `docs/LIMITATIONS.md`.
