# `osdbits-wasm` System Architecture

## 1. High-Level Component Diagram

```text
+-----------------------------------------------------------------------------+
| Browser Shell (index.html + src/ui/ + src/styles/)                          |
|  - Single-click "Start OSDSYS" launch gate (sync AudioContext.resume())     |
|  - Keyboard/pointer OSDSYS navigation + collapsible customization/diag HUD  |
+-------------------+---------------------+-------------------+---------------+
                    |                     |                   |
                    v                     v                   v
+------------------------+   +------------------------+   +-------------------+
| WebGPU / Fallback      |   | Web Audio Engine       |   | Persistence Layer |
| Renderer (src/renderer)|   | (src/audio/)           |   | (src/storage/)    |
| - Owns GPUDevice,      |   | - Procedural pads &    |   | - Versioned IDB   |
|   textures, WGSL,      |   |   OSDSYS UI chimes     |   |   (v1 -> v2)      |
|   swapchain, resize,   |   | - Autoplay state       |   | - Read-back hash  |
|   device.lost recovery |   |   machine & silent     |   |   verification    |
| - Canvas 2D fallback   |   |   degraded fallback    |   | - BroadcastChannel|
+-----------+------------+   +------------------------+   |   stale-write     |
            ^                                             |   protection      |
            | Zero-copy TypedArray views                  +---------+---------+
            | over WebAssembly.Memory                               |
+-----------+-------------------------------------------------------+---------+
| Rust Core Engine (`crates/osdbits-core` -> `wasm32-unknown-unknown`)        |
|  - Exact `OsdRng` & `TowerRng` LCGs + IEEE-754 deterministic `f32` math     |
|  - Scene engines: Mode A (Opening), Mode B (Idle), Mode C (Illegal Disc),   |
|    Mode D (Menu / Smoke Tunnel / 12 Glass Rods / 5 Glass Cubes), Mode E     |
|  - Clean-room procedural texture/mesh/font generator (`procedural_assets`)  |
|  - Safe bounds-checked `ROMDIR` + LZ `expand()` importer (`bios_import`)    |
|  - Flat `#[repr(C)]` `RenderVertex` + `RenderBatch` + `FrameSummary` output |
+-----------------------------------------------------------------------------+
```

---

## 2. Single WebGPU Ownership Model

To keep WASM lean (`< 250 KB`), compilation fast, and device-loss recovery deterministic, **WebGPU ownership is strictly assigned to TypeScript (`src/renderer/webgpuRenderer.ts`)**, while **simulation and vertex/command preparation are strictly assigned to Rust/WASM (`crates/osdbits-core`)**:

1. **Rust/WASM Responsibilities (`crates/osdbits-core`):**
   - Owns all deterministic PRNG seeds (`OsdRng`, `TowerRng`), simulation clocks, camera state machines, tower history/sprawl, orb trails, rod/cube refraction UV projection, lighting, and UI text/menu layout.
   - Pre-allocates reusable contiguous buffers (`Vec<RenderVertex>`, `Vec<RenderBatch>`, `FrameSummary`) so steady-state frame stepping performs **zero heap allocations**.
   - Never holds browser JS/GPU handles (`wgpu` is intentionally *not* linked into WASM).

2. **TypeScript Renderer Responsibilities (`src/renderer/`):**
   - Owns `GPUAdapter`, `GPUDevice`, `GPUCanvasContext`, WGSL `GPUShaderModule`s, `GPURenderPipeline`s, depth buffer (`depth24plus`), texture atlas array/slots, and PS2 GS-equivalent offscreen working buffers:
     - `mainColor` (`rgba8unorm` render target + copy source)
     - `refractUntinted` (`rgba8unorm` snapshot captured before `menuback` purple tint for the 12 glass clock rods)
     - `refractTinted` (`rgba8unorm` snapshot captured before each Opening/Illegal/Config glass cube so cubes refract previously drawn cubes!)
     - `extraBloom` (half-resolution `rgba8unorm` downsample target for `DrawExtraBuf2` and 5-pass `MenuZoomBlur`)
   - Reads WASM linear memory directly (`new Float32Array(wasmMemory.buffer, vertexPtr, vertexFloatCount)`) and uploads with a single `device.queue.writeBuffer` call per frame.
   - Automatically degrades to `FallbackCanvasRenderer` (`src/renderer/fallbackCanvasRenderer.ts`) if `navigator.gpu` is absent, adapter acquisition returns `null`, or an unrecoverable `device.lost` event occurs.

---

## 3. Zero-Copy Binary ABI (`#[repr(C)]`)

### `RenderVertex` (32 bytes, 8 × `f32` / `u32`, 16-byte aligned friendly)
| Offset | Type | Field | Description |
| :--- | :--- | :--- | :--- |
| `0` | `[f32; 4]` | `clip_pos` | Clip-space `(x, y, z, w)` ready for WGSL `@builtin(position)` (`z` mapped to `[0, w]`) |
| `16` | `[f32; 2]` | `uv` | Texture coordinates `(u, v)` (perspective-correct via clip `w` or `q`-scaled) |
| `24` | `[u8; 4]` | `color` | RGBA8 vertex color (`0x80 = 1.0` under PS2 `2x` modulation or `0xFF = 1.0` standard) |
| `28` | `u32` | `flags` | Per-vertex/mode flags (e.g., PS2 `2x` color-modulation bit, fog factor) |

### `RenderBatch` (32 bytes, 8 × `u32`, `#[repr(C)]`)
| Offset | Type | Field | Description |
| :--- | :--- | :--- | :--- |
| `0` | `u32` | `first_vertex` | Start index in `RenderVertex` buffer (triangle list, 3 vertices per triangle) |
| `4` | `u32` | `vertex_count` | Number of vertices in batch |
| `8` | `u32` | `texture_id` | Bound texture slot (`0 = untextured`, `1..15 = procedural/imported textures`, `16 = refractUntinted`, `17 = refractTinted`, `18 = extraBloom`) |
| `12` | `u32` | `blend_mode` | `0 = Opaque`, `1 = SrcAlpha`, `2 = Additive`, `3 = Subtractive (Cd - Cs*As)`, `4 = DestAlphaSilhouette` |
| `16` | `u32` | `depth_mode` | `0 = None`, `1 = ReadOnly (GEqual/LEqual)`, `2 = ReadWrite`, `3 = ClearDepthFirst` |
| `20` | `u32` | `pass_action` | Bitflags before batch: `1 = CaptureUntintedBackdrop`, `2 = CaptureTintedBackdrop`, `4 = CaptureBloomDownsample`, `8 = ClampUV` |
| `24` | `u32` | `target_id` | `0 = MainScreen`, `1 = ExtraBloom` |
| `28` | `u32` | `reserved` | Alignment / shader variant flags |

---

## 4. PS2 GS Blending Mapping to WebGPU

| PS2 GS `ALPHA_1` | Formula | WebGPU `GPUBlendState` Equivalent |
| :--- | :--- | :--- |
| `0x44` `(Cs - Cd)*As + Cd` | Standard source-alpha | `operation: 'add'`, `srcFactor: 'src-alpha'`, `dstFactor: 'one-minus-src-alpha'` |
| `0x48` `(Cs - 0)*As + Cd` | Additive glow/bloom/flare | `operation: 'add'`, `srcFactor: 'src-alpha'`, `dstFactor: 'one'` |
| `0x42` `(0 - Cs)*As + Cd` | Subtractive shadow/vignette (`Cd - Cs*As`) | `operation: 'reverse-subtract'`, `srcFactor: 'src-alpha'`, `dstFactor: 'one'` |
| `0x16` `(Cs - Cd)*Ad + Cd` | Destination-alpha silhouette composite (Config Cubes) | `operation: 'add'`, `srcFactor: 'dst-alpha'`, `dstFactor: 'one-minus-dst-alpha'` |

Note on PS2 GS `TEXA` / `TCC` `2x` Modulation:
On the PS2 GS, modulated vertex color `0x80` (`128/255 ≈ 0.502`) represents unity (`1.0`), allowing vertex colors `129..255` to brighten textures up to `~2.0x`. Our WGSL fragment shader explicitly applies `(v_color * (255.0 / 128.0))` when the batch's `PS2_MODULATE_2X` flag is set, preserving authentic specular highlights on glass rods, cubes, and towers!

---

## 5. Preserved Upstream Layout vs. New `osdbits-wasm` Directories

- **Preserved Untouched:** `osdbits/`, `matching/`, `tools/`, `ps2expand.c`, `notes.txt`, `vucode_*`, and all upstream files in `docs/`.
- **Added for `osdbits-wasm`:**
  - `crates/osdbits-core/` — Rust simulation, assets, `ROMDIR`+LZ importer, and WASM ABI
  - `src/` — Strict TypeScript WebGPU/Canvas2D renderers, Web Audio synth, IndexedDB/OPFS persistence, and UI shell
  - `tests/` — Vitest unit/WASM/storage/perf suites and Playwright real-browser E2E, persistence-restart, and visual-regression suites
  - `scripts/` — WASM build script and repository BIOS/hygiene scanner
  - `.github/workflows/` — Separated CPU/SwiftShader CI and Hardware-GPU CI workflows
