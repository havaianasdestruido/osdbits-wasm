# `osdbits-wasm` Architecture & Single-Ownership Rendering Model

## 1. System Overview

`osdbits-wasm` transforms [`aap/osdbits`](../README.md) — a reverse-engineered PlayStation 2 Emotion Engine (EE) + Vector Unit 1 (VU1) + Graphics Synthesizer (GS) reconstruction of the retail OSDSYS boot ROM — into a deterministic, zero-proprietary-asset browser application built on **Rust (`wasm32-unknown-unknown`)**, **WebGPU (TypeScript + WGSL)**, **Web Audio API**, and **IndexedDB / OPFS**.

All historical PS2 source files (`osdbits/`, `matching/`, `vucode_*`, `tools/`, `ps2expand.c`, and original `docs/*.md`) remain 100% untouched at their upstream commit state (`ea0c687`).

---

## 2. Single-Ownership Rendering Division (`Rust/WASM` vs. `TypeScript/WebGPU`)

To avoid duplicated state or split math between Rust and TypeScript, `osdbits-wasm` enforces a strict single-ownership boundary:

| Responsibility | Owner | Implementation |
| --- | --- | --- |
| PRNGs (`OsdRng` LCG + `TowerRng` xorshift), fixed-step timelines, camera splines, state machines | **Rust (`osdbits-core`)** | `crates/osdbits-core/src/{math,opening,menu,scene}.rs` |
| `20x20` radial `HeightGrid`, `14x9` memory-card tower sprawl/eviction, `vucode_1` 3-light Gouraud shading | **Rust (`osdbits-core`)** | `crates/osdbits-core/src/towers.rs` |
| Guard-band frustum culling, column-major view/projection (`screenAY`), reverse-Z `near / w`, `(S, T, Q)` packing | **Rust (`osdbits-core`)** | `crates/osdbits-core/src/{math,towers,opening,menu}.rs` |
| Pre-allocated flat vertex (`12 x f32`) & draw-command (`8 x u32`) streams + JSON telemetry snapshot | **Rust (`osdbits-wasm`)** | `crates/osdbits-wasm/src/lib.rs` (`#![forbid(unsafe_code)]` core, zero `unsafe` blocks in ABI crate) |
| GPU adapter/device lifecycle, `device.lost` recovery, DPR/aspect viewport scaling (`4:3`, `16:9`, `fit`) | **TypeScript (`WebGpuRenderer`)** | `web/src/renderer/webgpuRenderer.ts` |
| Offscreen framebuffer ping-pong graph (`sceneColor`, `scratchColor`, `capturedScreen`, `workBuf`, `extraBufA/B`, `depthTex`) | **TypeScript + WGSL** | `web/src/renderer/shaders/*.wgsl` |
| Clean-room procedural textures (`23` slots) & optional local `ROMDIR`/`TEXIMAGE` import | **TypeScript + Rust LZ** | `web/src/assets/{proceduralTextures,userResourceImporter}.ts` + `crates/osdbits-core/src/expand.rs` |
| Browser-safe procedural ambient soundscape & single-gesture `AudioContext.resume()` | **TypeScript (`AudioEngine`)** | `web/src/audio/audioEngine.ts` |
| Versioned `IndexedDB` + optional `OPFS` persistence with mandatory post-write read-back verification | **TypeScript (`StorageManager`)** | `web/src/storage/storageManager.ts` |

---

## 3. WASM C-ABI & Zero-Steady-State-Allocation Stream Contract

`crates/osdbits-wasm` exposes a minimal C-ABI (`ABI_MAGIC = 0x4F534442`, `ABI_VERSION = 1`) consumed by `web/src/wasm/bridge.ts` (`WasmSimulationBridge`):

- **Universal Vertex Stride (`12 x f32 = 48 bytes`)**:
  - `clip_pos: [f32; 4]` — WebGPU homogeneous clip coordinates (`[ndc_x * w, ndc_y * w, near, w]` with reverse-Z `[0, 1]` and `depthCompare: "greater-equal"`).
  - `uv_q: [f32; 4]` — `[u, v, q, aux]`, interpolated linearly in WGSL as `stq = vec3f(u * q, v * q, q)` and divided per-fragment (`stq.xy / stq.z`) to match the PS2 GS hardware `(S, T, Q)` perspective divider.
  - `gs_rgba: [f32; 4]` — raw PS2 GS vertex color `[R, G, B, A]` where `128.0` (`0x80`) represents unity (`1.0`) modulation.
- **Flat Draw Command Stride (`8 x u32 = 32 bytes`)**:
  - `[kind, target, texture_id, pipeline_flags, first_vertex, vertex_count, aux0, aux1]`.
- `OsdSceneEngine` clears and reuses its internal `Vec<f32>` and `Vec<u32>` buffers in-place every frame, maintaining zero unbounded WASM linear-memory growth across arbitrarily long sessions.

---

## 4. PS2 GS Alpha / Blend & Offscreen Framebuffer Graph

The PS2 GS uses `[0..128..255]` (`0x80 = 1.0`) color/alpha modulation (`Tf * Cf / 128`) and fixed-factor blending (`BlendModes[0..10]`). `WebGpuRenderer` faithfully models this via:

1. **Unified Texture Alpha Scale (`tex_alpha_scale`)**:
   - Static textures (`textureId < 100`) store raw PS2 `0..255` alpha (`0x80 = 1.0` unity) and pass `tex_alpha_scale = 255.0 / 128.0` to WGSL.
   - Offscreen render targets (`100` `CapturedScreen`, `101..103` `WorkBuf`) store normalized `[0, 1]` alpha and pass `tex_alpha_scale = 1.0`.
2. **`DrawExtraBuf2` + `DrawToExtraBuf2` Temporal Feedback (`opening.c:1993..2076`)**:
   - `extraBufA` and `extraBufB` are allocated at `halfWidth x height`.
   - Each frame, `DrawExtraBuf2(fix)` stretches the previous frame's half-width buffer over `sceneColor` using PS2 `BlendModes[2]` (`(Cs - Cd) * (fix / 128) + Cd`, `fix = 80` in Modes A/B, `fix = 112` in Mode C), and `DrawToExtraBuf2()` squeezes the updated `sceneColor` into the opposite half-width `extraBuf`.
3. **Refractive Cubes & Crystal Clock Rods**:
   - Modes A/B/C capture `sceneColor` into `capturedScreen` using `blit_post.wgsl` mode `0` (`PSMCT24 + TEXA{0x7f, AEM=1}`: non-black alpha = `127/128`, black = `0`) prior to each 10-pass refractive glass cube.
   - Mode D renders twin 7-orb trails into both `sceneColor` (`target = 0`) and `workBuf` (`target = 1`) so the 12 hexagonal crystal clock rods and 5 camera-space glass cubes refract live background orbs without self-sampling hazards (`scratchColor` copy barrier).

---

## 5. Audio & Persistence Lifecycles

- **`AudioEngine` (`web/src/audio/audioEngine.ts`)**:
  - Creates zero running nodes before user interaction.
  - Clicking **Start OSDSYS** calls `audio.activateFromUserGesture()` **synchronously on line 1 of the click event listener** (invoking `AudioContext.resume()` directly inside the user gesture call stack before any microtask boundary) and transitions straight into the running scene without a second click.
  - Suspends automatically on `document.hidden` (`visibilitychange`) and resumes when visible again (unless muted).
- **`StorageManager` (`web/src/storage/storageManager.ts`)**:
  - Stores settings in `IndexedDB` (`osdbits_wasm_db`, schema `v2`, with tested `v1 -> v2` upgrade migration, FNV-1a envelope checksums, monotonic `revision` stale-write rejection, and `BroadcastChannel` tab sync).
  - Every `saveSettings()` and `saveImportedAsset()` performs a **mandatory post-write read-back in a separate `readonly` transaction** (`verifiedReadBack: true`), verifying structural and digest equality before resolving.
  - Large optional user-imported binary archives use **OPFS** (`navigator.storage.getDirectory()`) when available and fall back transparently to `IndexedDB` blob storage or in-memory session storage under private-browsing/quota restrictions.
