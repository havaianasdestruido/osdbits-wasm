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
Test Files  6 passed (6)
     Tests  56 passed (56)
```

The 56 tests are: `frame.test.ts` 14 (parser + malformed input), `settings.test.ts`
11, `storage.test.ts` 8, `textures.test.ts` 7, `menu.test.ts` 7,
`wasm.parity.test.ts` 10 (against the real wasm module: ABI, determinism, frame
parsing, bounds, idle/boot ending, options, pass table, error reporting, events,
menu-mode frame).

```text
$ npx vite build
vite v7.3.7 building client environment for production...
✓ 17 modules transformed.
dist/index.html                  4.35 kB │ gzip:  1.54 kB
dist/assets/index-*.css          5.11 kB │ gzip:  1.87 kB
dist/assets/index-*.js          53.75 kB │ gzip: 18.65 kB │ map: 185.77 kB
✓ built in 329ms
```

## Repository guards

```text
$ bash scripts/check-no-proprietary-assets.sh
ok: no proprietary assets tracked (141 files checked)

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
| `npx playwright test` (Chromium/Firefox/WebKit) | **Blocked** | No browser binaries; downloads from `cdn.playwright.dev` / `objects.githubusercontent.com` fail with HTTP 000 in this sandbox. The specs are committed. |
| WebGPU rendering on hardware | **Blocked** | No GPU adapter, no browser. The compatibility renderer is the only rendering path executed anywhere so far, and even it could not be exercised in a browser here. |
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
