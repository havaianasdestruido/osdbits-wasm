# Testing Strategy, Persistence Verification & GPU Reporting (`osdbits-wasm`)

`osdbits-wasm` enforces layered verification across Rust, WebAssembly, TypeScript, real Chromium IndexedDB, and WebGPU/Canvas2D rendering, while strictly distinguishing mock tests, software fallback WebGPU adapters, and physical GPU silicon.

---

## 1. Command Reference

| Command | Scope | What It Verifies |
| --- | --- | --- |
| `npm run check:hygiene` | Repository scan | Zero Sony BIOS/ROM/extracted proprietary binaries anywhere in the workspace; all 21 upstream PS2 sources and RE docs intact. |
| `npm run check:rust` | Rust `x86_64` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and 21 Rust unit, `SplitMix64` property-based (`2,000` trig samples, `32` seed replays, `10,000`-step Idle stability), NIST SHA-256, and `1,500`-case bounded `ROMDIR`/LZ fuzz tests. |
| `npm run build` | Production bundle | Compiles release `wasm32-unknown-unknown` (`~168 KiB` < `250 KiB` budget), runs `tsc --noEmit` (`strict: true`), and produces relative-base (`./`) static `dist/` assets. |
| `npm run lint` | Static analysis | Strict ESLint (`typescript-eslint`) across `src/`, `tests/`, `scripts/`, and configs. |
| `npm run test:unit` | Vitest (Node) | Viewport aspect letterboxing/pillarboxing, WGSL source parity, WebGPU unavailable/locked-canvas fallback, synchronous `AudioContext` gesture unlock, and `PersistenceStore` migrations, stale-write conflict detection, tamper rejection, and W3C `fake-indexeddb` round-trips. |
| `npm run test:wasm` | Vitest + WASM | Instantiates real `public/wasm/osdbits_core.wasm`, verifies C-ABI struct layouts (`32B / 32B / 64B`), `GlassBumpInv == !GlassBump` across linear memory, bit-identical seed replay across all 4 modes, menu actions, synthetic `TEXIMAGE` import/reset, and zero memory growth. |
| `npm run test:perf` | Endurance & Size | Verifies release WASM `< 250 KiB`, steps `1,200` frames across `24` mode switches (`~0.33 ms/frame` mean CPU cost vs `< 2.5 ms/frame` budget), and asserts `0 bytes` steady-state WASM linear memory growth. |
| `npm run test:e2e` | Playwright (Chromium) | Executes `app-journeys.spec.ts`, `persistence-restart.spec.ts`, and `visual-regression.spec.ts` against the built `dist/` bundle served via `vite preview`. |
| `npm run test:e2e:hardware-gpu` | Hardware GPU Gate | Runs `tests/e2e/webgpu-hardware.spec.ts` with `REQUIRE_HARDWARE_GPU=1`, asserting `isFallbackAdapter === false` and `< 8 ms` p95 frame pacing on physical GPU runners. |
| `npm run verify:all` | Full Gate | Runs `check:hygiene`, `check:rust`, `build`, `lint`, `test:unit`, `test:wasm`, `test:perf`, and `test:e2e` sequentially. |

To intentionally regenerate visual regression golden PNGs after deliberate rendering changes:

```bash
UPDATE_GOLDENS=1 npx playwright test tests/e2e/visual-regression.spec.ts
```

---

## 2. Honest GPU Verification Tiers

`WebGpuRenderer` (`src/renderer/webgpuRenderer.ts`) inspects `GPUAdapter.info` (`vendor`, `architecture`, `description`, `isFallbackAdapter`) and never conflates software adapters with hardware GPUs:

1. **Tier 1 — Mocked / Degraded Unit Tests (`tests/unit/renderer.test.ts`):**
   - Simulates `navigator.gpu === undefined` and locked `<canvas>` contexts in Node/Vitest, verifying automatic DOM canvas replacement and `FallbackCanvasRenderer` execution.
2. **Tier 2 — Real Chromium Dawn WebGPU on Software Vulkan (`npm run test:e2e` in CPU containers):**
   - Runs real Chromium 153 Dawn WebGPU + WGSL compilation (`getCompilationInfo()`) + 15 blend/depth pipelines + `copyTextureToTexture` refraction + `readPixels()` staging readback over Google SwiftShader Vulkan (`isFallbackAdapter: true`).
   - Reported honestly in UI (`WebGPU Software (SwiftShader)`) and diagnostics (`isFallbackAdapter: true`).
3. **Tier 3 — Physical Hardware GPU (`npm run test:e2e:hardware-gpu` — `Blocked` in container without `/dev/dri`):**
   - Requires a physical Vulkan, Metal, or D3D12 GPU (`isFallbackAdapter === false`). Configured as a separate opt-in job (`hardware-gpu`) in `.github/workflows/ci.yml` and marked `[!]` Blocked in `TODO.MD` (`P11-002`) when running inside a GPU-less Linux container.

---

## 3. Reload vs. Full Browser Restart Persistence Testing

`tests/e2e/persistence-restart.spec.ts` explicitly separates two distinct levels of persistence verification:

1. **In-Context Reload (`page.reload()`):** Verifies same-session page reload survival of settings, Mode E overrides, and imported `TEXIMAGE` SHA-256 hashes.
2. **True Browser Process Termination & Relaunch (`chromium.launchPersistentContext(userDataDir)`):**
   - Launches Chromium against a dedicated on-disk profile directory, customizes preferences, imports a synthetic `TEXIMAGE` container, verifies IndexedDB read-back, and calls `await ctx1.close()` to terminate the browser completely.
   - Relaunches a fresh Chromium process (`ctx2`) against the same `userDataDir` to prove genuine disk-backed survival, then executes the 2-step **Confirmed Factory Reset** and launches `ctx3` to verify disk wipe.
