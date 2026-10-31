# Testing guide and execution evidence

All commands run from the repository root. Toolchain used for the recorded runs:

| Tool | Version |
|---|---|
| rustc / cargo | 1.88.0 (installed from `@rustbin/rustc-1.88.0-x86_64-unknown-linux-gnu` + `cargo` + `rust-std` for `x86_64` and `wasm32-unknown-unknown`) |
| clippy / rustfmt | 0.1.88 / 1.8.0 |
| node / npm | 22.22.3 / 10.9.8 |
| Chromium | 150.0.7871.100 (headless, Vulkan + SwiftShader) |
| Playwright | 1.64 |

## 1. Rust: formatting, lints, unit + property + fuzz tests

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Result: **25 passed, 0 failed**; clippy clean; formatting clean.

Coverage by module:

| Module | Tests | What is actually asserted |
|---|---|---|
| `math` | 4 | `osdRand` known sequence, `md_sin²+md_cos²=1` over 500 random angles, `MatrixDrive` push/pop semantics, camera+projection invariants |
| `resource` | 3 | LZSS round-trip on structured data, **50 randomized round-trips** + **200 malformed blobs** with no panic, synthetic ROMDIR container round-trip |
| `assets` | 2 | All 25 textures present with exact dimensions, `TEXCBINV` is the bitwise complement of `TEXCBUMP`, metric spot-checks from the docs, rod/cube face counts, import merges and rebuilds the complement |
| `input_audio` | 3 | Region swap verified exhaustively over all **65 536** byte pairs against the ROM's own shuffle, UP/DOWN repeat + the DOWN-clobbers-UP quirk over **400 randomized frames**, mailbox ring + 60-frame reverb ramp |
| `opening` | 2 | boot history → tower field (0 / 6 / 126 cells), opening state machine reaching state 7 and idle staying bounded |
| `illegal` | 1 | 128 patches / 7 rings / 5 cubes, angle wrapping, fade and text curves |
| `menu` | 2 | clock rollover + hold, orb trail length/entry fade |
| `menuconfig` | 1 | 90-frame choreography, cube selection colours, confirm kick |
| `menutext` | 4 | tokenizer + `\7o`/`\7r` escapes, **non-ASCII never indexes past the atlas** (a real panic regression), leap-year day counts, menu navigation, clock cancel |
| `ffi` | 2 | every mode steps 60+ frames without panicking and emits commands; state JSON reflects settings |

## 2. WASM artifact tests (real compiled module, Node)

```
npm run build:wasm
npx vitest run tests/unit/wasm.test.ts
```

Result: **9 passed**. Asserts the artifact exists, `WebAssembly.validate` passes, the full ABI is
present, the 25 textures have their documented dimensions, every mode emits a well-formed command
stream (first command a clear, all draws triangle-aligned and in range), the opening reaches the
menu and emits `StBgmPlay`/`StSetReverbDepth`, the region-swapped pad drives the menu correctly,
boot history clamps hostile values, garbage imports are rejected, and aggressive screen churn does
not panic.

## 2b. Render-stream tests (independent software rasterizer)

```
npx vitest run tests/unit/render.test.ts
```

Result: **7 passed**. The real WASM command stream is fed to
`tests/support/software-raster.ts` — a separate implementation from the WebGPU renderer — which
implements the GS blend modes, depth modes and CLAMP/REPEAT bilinear sampling. It verifies that the
opening fills the frame with lit tower geometry, that the illegal-disc screen is red-dominant, that
the menu's orb cluster is brighter than the vignetted border, that System Configuration composites
blue over red, that the browser screen draws, that idle mode still renders after 900 frames, and
that a full memory card produces more lit coverage than an empty one. PNG snapshots are written to
`test-results/render/` (`opening`, `illegal`, `menu`, `system-config`, `browser`, `idle-900`,
`idle-full-card`) for human inspection.

This is the visual regression layer that the GPU limitation in §5 would otherwise leave empty: it
validates composition and colour distribution, not pixel-exact GPU output.

## 3. Storage tests

```
npx vitest run tests/unit/storage.test.ts
```

Result: **9 passed**. Verified write/read-back, persistence across a re-opened handle, monotonic
revisions, corruption quarantine, v1→v2 migration reporting, unavailable-storage degradation
(refuses to claim success), export/import round-trip, hostile-value clamping, and reset.

## 4. Audio tests

```
npx vitest run tests/unit/audio.test.ts
```

Result: **5 passed**. Asserts `resume()` runs inside `unlock()` before anything yields, no context
is created before the gesture, missing Web Audio degrades cleanly, BGM/SE routing does not leak
beds, and volume/mute clamping works.

## 5. Browser end-to-end tests (real Chromium, real WebGPU)

```
npm run test:e2e
```

Result: **11 passed**.

| Test | Asserts |
|---|---|
| activation screen states its capabilities | badge states resolved before the click, GPU title present |
| one click starts the engine and renders content | overlay hides, frame advances, first submitted frame has draws/passes/vertices, pixel signature when the readback succeeds, no page errors |
| a lost GPU device degrades to simulation-only | HUD reports `gpu unavailable … simulation running`, overlay stays hidden, stepping continues |
| mode switching produces geometry for every scene | modes 2, 3, 1, 0 each produce vertices/commands |
| deterministic stepping drives menu → config → browser | exact screen transitions |
| settings survive a page reload | language + speed + boot history restored after reload and a single click |
| keyboard navigation and pause | F2 opens/closes the panel; pause freezes the frame counter; resume advances it |
| resizing keeps things healthy | canvas resizes, stepping and commands stay valid |
| importing a non-BIOS file | rejected with "no OSDSYS resources" |
| importing a synthetic ROMDIR container | one resource merged, state reports imported assets |
| reset restores defaults | language back to 1 after confirm |

### The GPU limitation in this sandbox

The headless Chromium here acquires a WebGPU device but the browser **destroys it after 2–4
frames**. This is environmental, not application-specific: a standalone page that only clears the
swap chain loses the device the same way. The probe used to establish that:

```js
// minimal repro, served over http://localhost (WebGPU needs a secure context)
const adapter = await navigator.gpu.requestAdapter();
const device = await adapter.requestDevice();
device.lost.then(i => out.textContent = 'LOST:' + i.reason);
for (let i = 0; i < 300; i++) {
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(),
    clearValue: {r:1,g:0,b:0,a:1}, loadOp:'clear', storeOp:'store' }] });
  p.end();
  device.queue.submit([enc.finish()]);
  await new Promise(r => requestAnimationFrame(r));
}
```

Tried with `--use-webgpu-adapter=swiftshader`, `--enable-features=Vulkan --use-vulkan=swiftshader`,
`--in-process-gpu --disable-gpu-sandbox`, and `--use-angle=swiftshader`; all lost the device within
3 frames. Consequences are documented in `docs/LIMITATIONS.md` §3: sustained-rendering,
visual-regression-against-reference and frame-time-percentile claims are **not** made.

## 6. Full gate

```
npm run build:wasm && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings &&
cargo test --workspace && npm run typecheck && npx vitest run && npx playwright test && npm run build
```
