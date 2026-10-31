# Architecture of `osdbits-wasm`

```
┌───────────────────────────────────────────────────────────────────────────┐
│ index.html + src/main.ts  (activation overlay, HUD, customisation panel)   │
├───────────────────────────────┬───────────────────────────────────────────┤
│ src/wasm/bridge.ts            │ src/renderer/*.ts                        │
│  typed views over WASM memory │  GpuContext  → adapter/device/targets     │
│  (vertices, commands, audio)  │  FrameRenderer → per-frame pass graph    │
├───────────────────────────────┴───────────────────────────────────────────┤
│ crates/osdbits-core  (Rust, compiled to wasm32-unknown-unknown)           │
│  math · resource · assets · input_audio · opening · illegal · menu ·       │
│  menuconfig · menutext · render_cmd · ffi                                  │
└───────────────────────────────────────────────────────────────────────────┘
        ▲                                                        ▲
        │ original reverse-engineering reference                 │
   osdbits/*.c  (PS2 EE C, untouched)                      docs/*.md (untouched)
```

## Why this split

The PS2 implementation in `osdbits/` is preserved verbatim: it is the reference, and it is the
only artifact that has ever been compared against real hardware. The browser port re-derives the
same behaviour in a language and runtime that can be tested and shipped, and keeps a one-to-one
mapping between source files so a reader can diff the two:

| Original (PS2) | Port (Rust) | Responsibility |
|---|---|---|
| `opening.c` camera / state machine | `opening.rs` | states 0..7, fly-up, SCE text, fades, idle mode |
| `opening.c` towers + `vucode_1.vsm` | `opening.rs` | field generation, per-face lighting, fog layers, trails, cubes |
| `opening.c` illegal scene | `illegal.rs` | red flare, 128-patch red fog, red glass cubes, localised prompt |
| `menu.c` | `menu.rs` | software clock, 7 orbs, 50-sample trails, vignette, zoom blur |
| `menuback.c` | `menu.rs` (tunnel, composite) | KABE tunnel, blue tint composite |
| `menuconfig.c` | `menuconfig.rs` | 12 rods, 5 cubes, 90-frame timer choreography |
| `menutext.c` | `menutext.rs` | glyph engine, string tables, menus, editors |
| `pad.c` | `input_audio.rs` | region swap, repeat cooker |
| `sound.c` | `input_audio.rs` + `src/audio/` | mailbox, reverb ramp, message vocabulary |
| `res.c` + `tools/extract-res.py` | `resource.rs` + `assets.rs` | ROMDIR, LZSS `expand()`, texture decoders |
| `pktSet*` / `vif1Set*` (GS register writes) | `render_cmd.rs` | flattened draw commands + blend/depth/wrap state |

## Determinism

Every frame is a pure function of `(mode, seed, frame, buttons, settings)`. There is no wall-clock
input, no `Math.random`, and no floating-point reordering between the native and WASM builds. The
Playwright suite relies on this: `window.__osdbits.step(n, buttons)` advances the same simulation
the render loop drives, and asserts exact screen transitions.

The render list is *state-flattened*: `FrameCommandBuffer::push_triangles` coalesces consecutive
draws that share target, texture, blend mode, wrap, depth mode and blend fix, so a tower field of
~600 quads becomes a handful of draws. The command record is exactly 10 `u32`s and a vertex is
exactly 10 `f32`s, which is what lets TypeScript read them as typed arrays with no parsing.

## Render target model

The GS work-buffer scheme is reproduced literally, because the effects depend on it:

| Target | Rust enum | GS role |
|---|---|---|
| `screen` | `RenderTargetId::Screen` (0) | the double-buffered draw buffer |
| `wb3` | `RenderTargetId::WorkBuf3` (1) | `extraBuf1` — pre-object copy |
| `wb4` | `RenderTargetId::WorkBuf4` (2) | `extraBuf2` — per-object capture |

`CopyTargetToTexture` performs the captures (`CubeCaptureBuffer`, `DrawToExtraBuf2`,
`MenuBackdrop`'s two copies) with `copyTextureToTexture`, and the copies are sampled as textures
`ScreenCopy`/`WorkBuf3`/`WorkBuf4`. That is why a cube can refract the frame it is drawn into
without a readback.

## Blend and depth mapping

| GS | WebGPU |
|---|---|
| `abe=0` (opaque) | `src=one, dst=zero` |
| `(Cs−Cd)·As + Cd` (0x44) | `src=src-alpha, dst=one-minus-src-alpha` |
| `Cs·As + Cd` (0x48) | `src=src-alpha, dst=one` |
| `Cd − Cs·As` (0x42) | `reverse-subtract` |
| `Cs·FIX + Cd` | `src=one, dst=one` (FIX folded into the vertex colour) |
| `Cs·Ad + Cd` | `src=one, dst=one` (destination-alpha keying is approximated by additive) |
| `ZTST ALWAYS / GEQUAL / GREATER` + `ZMSK` | `depthCompare` + `depthWriteEnabled` |

GS colours live in `0..2` with `0x80 = 1.0`, and that is preserved end-to-end so the blend maths
matches the console rather than a re-normalised approximation.

## Persistence

`src/storage/persistence.ts` owns the contract: a write is only reported successful after the
record has been read back and its schema version, revision and FNV-1a checksum have been compared.
Corrupt records are quarantined under `meta/corrupt-<ts>` and defaults are restored; a revision
counter makes a stale async write lose to a newer one. Assets go to OPFS when available and always
to IndexedDB (the copy that gets verified).

## Audio

`OSDDispatch` in Rust is the console's mailbox, including the 60-tick reverb ramp. TypeScript
drains it once per step and turns it into sound. The activation click calls
`AudioContext.resume()` synchronously before any `await`, which `tests/unit/audio.test.ts` asserts
by recording call order.

## Build

```
npm run build:wasm   # cargo build --target wasm32-unknown-unknown --release + copy to public/wasm
npm run build        # typecheck + vite build → dist/
npm run test:rust    # cargo test --workspace
npm run test:unit    # vitest (WASM, storage, audio)
npm run test:e2e     # playwright (real Chromium + real WebGPU)
```
