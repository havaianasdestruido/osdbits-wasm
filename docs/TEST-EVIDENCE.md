# Executed validation evidence — 2026-10-31

## Scope and environment

This is the **portable resource-tools milestone**, not a full product test
report. Linux x86_64 Debian 12 sandbox; Python 3.11.2; GCC/cc
12.2.0; Ruff 0.14.3; PyYAML 6.0.3. Node 22.22.3/npm 10.9.8 are present but
no frontend was built. All fixture bytes are constructed synthetically in
memory or temporary directories; no proprietary data was used or fetched.

QA dependencies were installed inside the ignored workspace `.cache/qa`:

```sh
python3 -m pip install --target .cache/qa ruff==0.14.3 PyYAML==6.0.3
PYTHONPATH=.cache/qa bash tools/check-portable.sh
```

Use the venv procedure in `DEVELOPMENT.md` for normal development. No
network-dependent test, proprietary SDK or private ROM is needed by this gate.

## Executed results

| Check | Result |
|---|---|
| Ruff formatting check (both entry points, shared parser, tests) | PASS |
| Ruff lint on the same affected surface | PASS |
| Python compileall on the same affected surface | PASS |
| `python3 -m unittest discover -s tests -v` with QA dependencies installed | PASS — 22 tests, no skips |
| Synthetic known-valid streams decoded by original `ps2expand.c` compiled with `cc -std=c99 -Wall -Wextra -O2` | PASS — 11 streams including all 4 descriptor modes |
| Synthetic font/string/menu geometry output syntax checked with `cc -std=c99 -Wall -Wextra -Werror -fsyntax-only` | PASS |
| Both extractor CLI paths launched from an external working directory | PASS |
| Advanced `--tables` CLI output compared with preserved helper output | PASS — synthetic image only |
| Atomic output failure injection | PASS — previous file preserved and temp cleaned |
| YAML workflow / local-gate / pinned-tool configuration contract tests | PASS — local parse/assertions, not remote Actions execution |
| Numbered TODO IDs, required fields/dependencies, inventory contract | PASS — 12 phases, 46 actionable tasks, 141 baseline paths |
| `git diff --check` | PASS |
| Clean local clone of the implementation snapshot on the same session branch | PASS — identical portable gate, 22 tests, no skips; QA packages supplied from ignored external cache |
| `make -C osdbits -n` and `make -C osdbits` | BLOCKED/FAIL — missing `/usr/local/freesce/ee/lib/crt0.o`; actual build exited 2 |

Clean-checkout verification was actually executed, not inferred:

```sh
git clone --no-hardlinks --single-branch --branch arena/0ebbd172-osdbits-wasm . .cache/clean-checkout
PYTHONPATH=/home/user/osdbits-wasm/.cache/qa bash .cache/clean-checkout/tools/check-portable.sh
```

The local clone had no source changes; the same 22 tests and formatting,
lint/compile checks passed. This verifies only the portable milestone, not
the future complete product release gate or independent dependency installation.

## Seeded campaigns and cases

- Seed `0x05D2026`: 500 literal encode/decode round trips with descriptor
  boundary/truncation cases; 2,000 bounded arbitrary compressed streams.
- Seed `0xD12026`: 1,000 bounded arbitrary directory inputs.
- Seed `0xC2026`: known-valid literal parity vectors for the original C
  decoder; additional hand-encoded overlapping/backreference bit-split cases.
- Explicit malformed cases: short headers/descriptors/literals/references,
  references preceding output, output-size overshoot, maximum-size headers,
  invalid limits, unaligned/missing/unterminated ROMDIR, duplicate/unsafe
  names, bad self spans, out-of-image spans and aggregate/input budgets.
- Valid cases: empty resources, 29/30/31 literal boundaries, all descriptor
  modes, overlapping backreferences, nested and bare resource containers,
  alternate SNDIMAGE selection, metadata skips, deterministic C include text.
- CLI regression: a valid first resource followed by a bad resource creates
  no output directory; missing files return status 2 without traceback.

These are reproducible bounded regression/fuzz-style campaigns, **not** a
claim of exhaustive security proof or a Rust cargo-fuzz campaign. No invalid
bytes are fed to the unsafe historical C decoder.

## Failures investigated during this change

The initial tests-first run failed with `ModuleNotFoundError: osd_resources`
before the shared implementation existed (expected red phase). After
implementation, 16 tests passed. Inspection of the diff caught the advanced
extractor's extra table functions: they were preserved and tested rather
than discarded during consolidation. Three parity/failure tests brought the
suite to 19. Ruff then identified the compatibility `write_inc` export as an
unused import; explicit re-export fixed it without removing the API. Three
project/CI contract tests brought the suite to 22. The complete affected gate
was rerun successfully. No failing test was skipped or weakened.

## Not verified / mandatory future gates

- Rust native tests, clippy/fmt, wasm32 build and browser WASM execution:
  no Rust toolchain installed; implementation not present.
- WebGPU shader/pipeline/passes, real GPU hardware, device loss, visual
  comparisons, endurance/frame-time/memory budgets: renderer not present.
- Audio activation, keyboard UI, browser engines, IndexedDB migration/quota,
  reload persistence and genuine browser-restart persistence: app not present.
- Original hardware/ROM equivalence and GS dump replay: authorized reference
  data, SDK/private replay library and physical hardware unavailable.
- Production build, deployment and deployed-origin smoke/storage tests:
  not implemented; no production URL claimed.
- GitHub-hosted workflow execution: configuration tested locally only.

The portable suite passing does **not** satisfy these gates or the user's
Definition of Done. `TODO.MD` retains their open/blocked status.
