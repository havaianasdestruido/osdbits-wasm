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
| Web unit/integration | `npx vitest run` | 56 tests pass, including 10 against the real wasm |
| Production build | `npx vite build` | succeeds; bundle size recorded |
| Asset guard | `bash scripts/check-no-proprietary-assets.sh` | no proprietary files tracked |
| Upstream guard | `bash scripts/check-upstream-untouched.sh` | upstream C sources byte-identical |
| Browser E2E | `npx playwright test` | **not run here** (no browser binaries) |

## CI

`.github/workflows/ci.yml` splits CPU-only work (core, wasm, web) from
GPU-dependent work (the Playwright job), which is marked
`continue-on-error: true` and uploads traces. It has not executed on GitHub from
this sandbox; the same commands were run locally and recorded in
`docs/TEST-EVIDENCE.md`.

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
