# Test evidence

Environment for every run below:

| | |
|---|---|
| Date | 2026-10-31 |
| OS | Linux (x86_64), 2 vCPU, 3 GB RAM |
| Rust | 1.88.0, installed by `scripts/bootstrap-rust.sh` into `~/.cache/rust-toolchain` (no rustup, no crates.io) |
| Node | 22.22.3, npm 10.9.8 |
| Browser | **none installed** — no Playwright browser binaries, no GPU |
| Upstream commit | `ea0c68712b28b36fe033ed4fe488d86958de53d2` |

Everything marked "pass" below was executed in this environment. Anything that
needs a browser or a GPU is listed under *Not executed* with the reason.

## Rust core

```text
$ cd crates/osdbits-core
$ cargo fmt -- --check
(no output)                                    # clean

$ cargo clippy --offline --all-targets -- -D warnings
    Finished `dev` profile ...                 # 0 warnings, 0 errors

$ cargo test --offline
running 131 tests
test result: ok. 131 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.15s
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 52.39s
running 0 tests
test result: ok. 0 passed; 0 failed; ... (doc tests)
```

Coverage by module (lib tests): `math` 12, `rng` 6, `field` 14+, `camera` 15,
`fog` 6, `lights` 8, `cubes` 11, `illegal` 9, `text` 7, `frame` 7, `scene` 18,
`abi` 12. Integration: `tests/properties.rs` 4 seeded campaigns.

Two crate-wide clippy lints are allowed on purpose and documented at the top of
`src/lib.rs`: `needless_range_loop` and `too_many_arguments`. The port keeps the
reconstruction's explicit index arithmetic; rewriting those loops as iterator
chains was judged a bigger fidelity risk than the lint.

## WASM artefact

```text
$ cargo build --offline --target wasm32-unknown-unknown --release
    Finished `release` profile [optimized]
$ ls -l target/wasm32-unknown-unknown/release/osdbits_core.wasm
-rwxr-xr-x ... 116575 bytes

$ bash scripts/build-wasm.sh
copied .../osdbits_core.wasm -> web/public/osdbits_core.wasm (116575 bytes)

$ node scripts/wasm-smoke.mjs
ok    exports osd_step
... (40 checks)
ok    every frame has the magic and consistent offsets
ok    draw list stays bounded
ok    replay is bit-identical
ok    boot scene ends
ok    idle never ends
ok    illegal scene runs
ok    PAL virtual screen is 640x256
ok    pass table illegal colour mode
ok    unknown mode is rejected
      frame time (step+draw, node, wasm): p50 0.001 ms p95 0.308 ms
PASS: 0 failed check(s) out of the suite
```

## Web layer

```text
$ cd web && npx tsc --noEmit
(no output)                                    # strict: true, 0 errors

$ npx vitest run
Test Files  8 passed (8)
     Tests  68 passed (68)
```

The 68 tests are: `frame.test.ts` 14 (parser + malformed input), `settings.test.ts`
11, `storage.test.ts` 8, `textures.test.ts` 7, `menu.test.ts` 7,
`wasm.parity.test.ts` 12 (against the real wasm module: ABI, determinism, frame
parsing, bounds, idle/boot ending, options, pass table, error reporting, events,
menu-mode frame, camera-motion option semantics), `boot.test.ts` 6 (the real
`App` booted inside jsdom against the real `index.html` and the compiled wasm)
and `canvas2d.test.ts` 4 (the compatibility renderer rasterised by
`@napi-rs/canvas`, i.e. real pixels, see below).

`canvas2d.test.ts` is the only place the compatibility renderer runs against a
real rasteriser outside a browser; it asserts the opening scene is non-blank
(luminance variance > 4), PAL is 640x256, the illegal scene has red-dominant
pixels, and a scene with every optional layer off still renders.

```text
$ npx vite build
vite v7.3.7 building client environment for production...
✓ 17 modules transformed.
dist/index.html                  4.35 kB │ gzip:  1.54 kB
dist/assets/index-*.css          5.11 kB │ gzip:  1.87 kB
dist/assets/index-*.js          54.71 kB │ gzip: 18.88 kB │ map: 188.29 kB
✓ built in 354ms
```

## Browser end-to-end (GitHub Actions, not this sandbox)

This sandbox cannot download browser binaries (see the table below), so the
Playwright suite runs in CI on every push: Chromium, Firefox and WebKit, the
full app, no GPU.

| | |
|---|---|
| Workflow run | `41245700225` (commit `418caad`) — the E2E job is blocking in this run |
| Result | **51 passed, 0 failed, 3 skipped** in 49 s; all five jobs `success` |
| Skipped | the three "WebGPU is used when the browser has an adapter" cases — runners have no adapter, which the app reports as a fallback |
| Renderer exercised | compatibility (Canvas 2D) — the same path most visitors without WebGPU get |
| Artifacts | `playwright-report` + `test-results` uploaded by the workflow |
| Earlier green run | `41245351478` (commit `70420c8`): same result, 62 s, job still `continue-on-error` |

The job is blocking as of that run. Failures are surfaced twice: the specs emit
`::error` annotations through `web/scripts/e2e-live-reporter.mjs` while running,
and `web/scripts/e2e-summary.mjs` re-reads the JSON report afterwards.

Earlier runs of the same suite (also in CI, recorded rather than hidden):

| Run | Result | What it exposed |
|---|---|---|
| `41233750163` | E2E failed | job wiring; failure detail not readable from here |
| `41242974408` | 0 passed / 51 failed | `$("#start")` → `getElementById("#start")`, i.e. the app never booted in any browser |
| `41244797275` | 45 passed / 3 failed | settings race on activation, clobbered capability text, reduced-motion semantics |


## Repository guards

```text
$ bash scripts/check-no-proprietary-assets.sh
ok: no proprietary assets tracked (206 files checked)

$ bash scripts/check-upstream-untouched.sh
ok: 23 upstream files are byte-identical to ea0c68712b28b36fe033ed4fe488d86958de53d2
```

## Dev server

```text
$ npx vite --host 0.0.0.0 --port 5173 --strictPort
  VITE v7.3.7  ready in 166 ms
  ➜  Local:   http://localhost:5173/

$ curl -s -o /dev/null -w '%{http_code} %{size_download}\n' http://127.0.0.1:5173/
200 4364
$ curl ... /src/main.ts      -> 200 2889   (transformed by Vite)
$ curl ... /osdbits_core.wasm -> 200 116575 type=application/wasm
$ curl ... /src/style.css    -> 200 7415
```

## Not executed (and why)

| Suite | Status | Reason |
|---|---|---|
| `npx playwright test` **in this sandbox** | **Blocked** | No browser binaries; downloads from `cdn.playwright.dev` / `objects.githubusercontent.com` fail with HTTP 000. Executed in CI instead (section above). |
| WebGPU rendering on hardware | **Blocked** | No GPU adapter here, and the CI runners report none either. The WebGPU path is unit-typed, shader-checked by review, and exercised only in the adapter-gated test, which skips. Never claimed as verified. |
| Audio output | **Blocked** | Nothing to play it through; only capability detection is unit-exercised. |
| Visual regression | **Deferred** | Depends on the two rows above; tolerances specified in `docs/QA.md`. |
| Original PS2 build (`ee-gcc`/`ee-dvp-as`) | **Blocked** | Toolchain and SDK absent; upstream's own build is unmodified and untouched. |

## Test bugs found and fixed during development (recorded, not hidden)

1. `rng.rs` expectations hand-computed (20538 vs the real 21932) — fixed by
   running the generator instead of doing arithmetic by hand.
2. A fog test asserted that higher layers scroll *slower*; the real formula
   `(14-l)*0.0001*(l+1)*0.5` makes them faster. Test corrected, code was right.
3. A `lights` test contradicted its own formula; corrected.
4. `parseFrame` initially read word 59 (a `u32` bit pattern written by
   `f32::from_bits`) as a float, which produced denormals — fixed and covered by
   a regression test.
5. The synthetic frame builder in `frame.test.ts` confused *header indices* with
   *section offsets* twice; both mistakes are now commented in the test helper.
6. **Application bugs found by the CI browser job** (none of them were reachable
   from this sandbox, which is why the browser job exists):

   | Symptom | Root cause | Fix |
   |---|---|---|
   | 51/51 E2E failures: the page never left the "loading" state | `$("#start")` passed a CSS selector to `getElementById`, so `App`'s field initialiser threw `missing element ##start` before `boot()` ran | bare element ids; `web/tests/boot.test.ts` now boots the real app in jsdom |
   | The renderer used defaults on the first frame if storage resolved late | `onStart()` read `this.settings` without awaiting the storage chain | `await this.settingsReady` inside the gesture, after audio is resumed |
   | Firefox: "core unavailable" text vanished after a fatal core failure | the late storage resolution called `setCapabilities()`, overwriting the failure line | `setCapabilities()` keeps the failure text when there is no core |
   | WebKit/Firefox: reduced motion did not hold the camera | the option is applied at scene init; the failing runs were the ones where the click beat the settings load (same race as above) | fixed by the settings await; semantics now pinned by 2 Rust + 2 wasm tests |
   | Light trails were offset by twice their thickness | the screen-sprite shader re-expanded CPU-built quads | dedicated absolute-screen-triangle mode (mode 3) |
