# QA plan and gates

## Layers

| Layer | Command | Gate |
|---|---|---|
| Rust formatting | `cargo fmt -- --check` | must be clean |
| Rust lints | `cargo clippy --offline --all-targets -- -D warnings` | zero warnings |
| Rust unit tests | `cargo test --offline` | all pass (139 tests) |
| Rust property campaigns | `cargo test --offline --test properties` | 4 seeded campaigns pass |
| WASM build | `cargo build --offline --target wasm32-unknown-unknown --release` | builds, size recorded |
| WASM smoke | `node scripts/wasm-smoke.mjs` | all checks pass (ABI, layout, determinism, bounds, PAL, errors) |
| TypeScript types | `npx tsc --noEmit` | clean, `strict: true` |
| Web unit/integration | `npx vitest run` | 68 tests pass: 12 against the real wasm, 6 booting the real app in jsdom, 4 rasterising the Canvas 2D path with `@napi-rs/canvas` |
| Boot wiring | `npx vitest run tests/boot.test.ts` | the real `App` boots against the real `index.html`; a broken element id fails here instead of in a browser |
| Production build | `npx vite build` | succeeds; bundle size recorded |
| Asset guard | `bash scripts/check-no-proprietary-assets.sh` | no proprietary files tracked |
| Upstream guard | `bash scripts/check-upstream-untouched.sh` | upstream C sources byte-identical |
| Browser E2E | `npx playwright test` | runs in CI (blocking): 51 passed / 0 failed / 3 skipped; cannot run in this sandbox |

## CI

`.github/workflows/ci.yml` runs five jobs: `core` (fmt, clippy, tests, property
campaigns), `wasm` (build + Node smoke, artefact uploaded), `web` (typecheck,
vitest, production bundle), `guards` (upstream untouched, no proprietary assets)
and `e2e` (Playwright on Chromium, Firefox and WebKit). All five are blocking;
the E2E job passes on GPU-less runners because the specs assert the documented
fallback path and only skip the adapter-specific cases.

Failures are surfaced through annotations, so a run is diagnosable from the
checks API alone (this sandbox cannot download job logs or artifacts):

* `web/scripts/e2e-live-reporter.mjs` — a Playwright reporter that emits
  `::error file=…::<project> › <title> — <first error line>` while tests run.
* `web/scripts/e2e-summary.mjs` — re-reads the JSON report afterwards and emits
  the same for anything the live reporter missed, plus a totals notice.

## Test design rules

* Every assertion names the behaviour, not the implementation: tests assert
  values derived from the reconstruction's own arithmetic, and expected values
  are produced by *running* the code, never by hand arithmetic (three early test
  bugs came from hand-derived constants; see the repository history notes in
  `docs/TEST-EVIDENCE.md`).
* Failure paths are tested as first-class behaviour: corrupt frames, unknown
  modes, out-of-range options, corrupt settings records, missing storage.
* Determinism is tested at three levels: Rust (`scene` tests + properties),
  Node (wasm smoke), TypeScript (wasm parity test + per-frame hash).
* No test is skipped to make the suite green; the wasm parity suite *reports*
  when the artefact is missing and runs in CI after building it.

## Visual regression

Not implemented yet (`P04-007`, `P10-006`). The intended approach, recorded so
it is not lost:

* Render a fixed seed/option matrix headlessly with a deterministic renderer
  (CPU rasteriser or a pinned GPU), capture the draw list and the framebuffer.
* Compare **draw lists** for exactness (deterministic bytes) and **images** with
  a per-pixel tolerance of ±2/255 and a mismatch budget of 0.5% of pixels, to
  absorb driver-level rasterisation differences while still catching real
  changes.
* Reference images may not be GS dumps (`D-008`); they are our own captures,
  labelled as such.

## Performance budgets

Measured here (Node, wasm release build): step+draw `p50 ≈ 0.001 ms`,
`p95 ≈ 0.3 ms`, draw list ≤ 5 948 vertices / 8 batches; module 116 KB.
Targets for the browser (to be measured, not claimed):

* startup to first frame < 1.5 s on a mid-range laptop;
* p95 GPU frame time < 16.6 ms at 640×224 with the high preset;
* no per-frame allocation after warm-up (buffers are reused);
* steady memory over 10 minutes of idle.
