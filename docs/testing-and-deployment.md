# Testing, Visual Regression Tolerances, Performance Budgets & Deployment Guide

## 1. Local Development & Test Commands

### Prerequisites
- **Node.js 20+ / 22+** (`npm ci` installs all TypeScript, Vite, Vitest, Playwright, WGSL `naga-wasm`, pinned `@rustbin/*` Rust `1.88.0` `x86_64` + `wasm32-unknown-unknown` toolchains, and `@sparticuz/chromium` with SwiftShader Vulkan).

### Command Reference

| Command | Description |
| --- | --- |
| `npm run dev` | Compiles `osdbits_wasm.wasm` and starts Vite dev server on `http://0.0.0.0:5173`. |
| `npm run build` | Runs `check:legal`, builds release WASM, runs `tsc --noEmit`, and outputs production static bundle to `dist/`. |
| `npm run preview` | Serves production `dist/` bundle on `http://0.0.0.0:4173`. |
| `npm run check:legal` | Scans repo for forbidden BIOS/ROM artifacts and verifies `ea0c687` provenance on historical PS2 files. |
| `npm run fmt:rust` / `npm run lint:rust` | Formats Rust workspace / runs `cargo clippy --all-targets -- -D warnings`. |
| `npm run test:rust` | Runs `osdbits-core` & `osdbits-wasm` unit tests + `12,500` SplitMix64 property & adversarial fuzz cases (`property_fuzz.rs`). |
| `npm run typecheck` | Runs `tsc --noEmit` with strict TypeScript settings (`strict: true`, `noUncheckedIndexedAccess: true`). |
| `npm run test:unit` | Runs Vitest suites: toolchain, WASM C-ABI & 3,000-frame zero-growth check, procedural assets & host `ps2expand.c` differential check, `naga-wasm` WGSL validation across all 7 shaders, `fake-indexeddb` migrations & read-back verification, Web Audio gesture mock, UI accessibility, and CI/docs checks. |
| `npm run test:e2e` | Runs Playwright Chromium WebGPU E2E suites: smoke, Modes A/B/C/D GPU pixel readback & `device.lost` recovery, Mode A->D automatic handoff, single-click `Start OSDSYS` + synchronous `AudioContext.resume()`, unsupported-WebGPU fallback UI, and real cold-process browser close + relaunch persistence (`launchPersistentContext`). |
| `npm run test:visual` | Runs deterministic 7-frame `640x448` golden PNG visual regression across Modes A, B, C, and D (`UPDATE_VISUAL_BASELINES=1 npm run test:visual` refreshes baselines). |
| `npm run test:perf` | Runs 600-frame multi-mode endurance + 30 rapid mode switches + 20 resize reallocations verifying zero WASM memory leaks, zero `GPUTexture` VRAM leaks, and percentile (`p50`/`p95`/`p99`) latency budgets. |
| `npm run test:prod` | Builds `dist/` and runs Playwright E2E against `vite preview` verifying zero external network requests and zero console errors. |
| `npm run check:all` | Executes the complete end-to-end verification pipeline (`check:legal` -> `lint:rust` -> `test:rust` -> `test:unit` -> `typecheck` -> `build:web` -> `test:e2e` -> `test:visual` -> `test:perf` -> `test:prod`). |

---

## 2. Deterministic Visual Regression Tolerances (`tests/visual/`)

`tests/visual/visual-regression.spec.ts` captures 7 fixed-seed, fixed-clock (`2026-10-31 12:15:30`), fixed-resolution (`640x448` NTSC `1.0x`) RGBA8 offscreen renders with 16 frames of temporal `ExtraBuf2` warmup:

1. `mode-a-opening-f024.png` (Opening drift, 126 towers)
2. `mode-a-opening-f090.png` (Opening `OSDBITS` banner + 5 refractive glass cubes + orb trails)
3. `mode-a-opening-f215.png` (Opening fly-up motion blur)
4. `mode-b-idle-f180.png` (Indefinite tower hover at `z = 16`)
5. `mode-c-illegal-f160.png` (Red flare + crimson clouds + red glass cubes + localized warning)
6. `mode-d-menu-main-f120.png` (7 RTC clock orbs + decaying ribbons)
7. `mode-d-menu-sysconfig-f120.png` (`KABE` tunnel + 12 crystal clock rods + 5 glass cubes)

**Justified Thresholds (SwiftShader Vulkan CPU Adapter)**:
- `meanAbsChannelDiff <= 1.5 / 255` (`0.59%` mean channel tolerance)
- `mismatchRatio(> 4 / 255) <= 0.2%` (`0.002` max pixels differing by more than 4/255)
- `rmse <= 3.0 / 255`

---

## 3. Hardware vs. Headless SwiftShader Adapter Reporting

In Linux CI / container environments without `/dev/dri` physical GPU nodes, `scripts/chromium-env.mjs` configures Chromium to run real WebGPU and WGSL pipelines over **SwiftShader Vulkan** (`--use-angle=vulkan --use-vulkan=swiftshader --enable-unsafe-webgpu`).
- Measured container benchmarks (`tests/perf/endurance-leak.spec.ts`):
  - **WASM Simulation (`stepFrame`)**: `p50 ~ 0.3 ms`, `p95 ~ 0.5 ms`, `p99 ~ 1.6 ms` (`3,840 KiB` steady-state linear memory, `0` bytes growth).
  - **SwiftShader CPU-Vulkan `renderFrame` submission**: `p50 ~ 1.2 ms`, `p95 ~ 9.2 ms`, `p99 ~ 14.2 ms`.
- Physical GPU driver validation (Metal, D3D12, desktop Vulkan RADV/ANV/NVIDIA) requires physical GPU hardware outside headless containers and is transparently distinguished in `TODO.MD` Known Limitations.

---

## 4. Static Deployment Guide

`npm run build` produces a 100% static, self-contained directory in `dist/` (~`33 kB` gzipped JS+CSS + `~115 kB` WASM) with `base: "./"` so it can be hosted from any root domain or subpath (GitHub Pages, Cloudflare Pages, Netlify, S3, Nginx, Caddy):

1. **HTTPS / Secure Context**: WebGPU (`navigator.gpu`) and OPFS (`navigator.storage.getDirectory()`) require a **[Secure Context](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts)** (`https://` in production, or `http://localhost` / `http://127.0.0.1` locally).
2. **MIME Types**: Ensure your static host serves `.wasm` files with `Content-Type: application/wasm` (standard on all modern CDNs and Vite/Nginx defaults).
3. **No `SharedArrayBuffer` / `COOP`+`COEP` Lock-In**: `osdbits-wasm` uses single-threaded deterministic WASM linear memory and does **not** require `Cross-Origin-Opener-Policy` or `Cross-Origin-Embedder-Policy` headers, making embedding effortless.
4. **Zero Runtime External Dependencies**: At runtime, the deployed app makes zero requests to external CDNs, font servers, analytics endpoints, or telemetry services.
