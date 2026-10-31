# Development and local resource tools

## Current scope

This milestone adds a portable resource-validation foundation and an auditable
roadmap. It is **not** the browser product. No Rust, WASM, WGSL, TypeScript,
IndexedDB or OPFS implementation exists yet. Do not interpret the portable CI
check as WebGPU validation. See `TODO.MD` for mandatory future gates.

## Portable QA

Requires Python 3.11, a C99 compiler (`cc`), Git, and pip. Tests compile the
unchanged historical C decompressor only against **known-valid synthetic**
streams and syntax-check synthetic generated C tables. No PS2 SDK, BIOS,
actual glyphs, textures, audio, browser or hardware is needed.

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements-dev.txt
bash tools/check-portable.sh
```

The gate uses controlled Ruff/PyYAML versions, checks formatting and lint for
the changed portable Python surface, compiles it to bytecode, executes all
unit/integration/regression tests, and runs `git diff --check`. Existing
reverse-engineering scripts are not silently reformatted/linted wholesale.
There is no static Python type checker yet; none is claimed. Frontend strict
TypeScript and Rust fmt/clippy are future tasks, not applicable passing checks.

`.github/workflows/portable.yml` runs the same gate in Ubuntu 24.04/Python
3.11 with a ten-minute limit and read-only repository access. No private
reference artifacts are fetched or uploaded. Actual remote CI execution is
separate from the locally tested workflow configuration.

## Authorized local extraction

These are developer analysis tools, not end-user browser dependencies.
Provide only an image you are authorized to access; extracting data does not
grant redistribution rights. Neither tests nor this repository supply one.

```sh
python3 tools/extract-res.py path/to/your-local-image osdbits/res
python3 tools/extract-res.py path/to/your-local-image osdbits/res --container FNTIMAGE
python3 tools/extract-res.py path/to/your-local-image osdbits/res --container SNDIMAGE
# Advanced historical capability exists only at this entry point:
python3 osdbits/tools/extract-res.py path/to/your-local-image osdbits/res --tables
```

Both resource entry points now share `tools/osd_resources.py`. Their existing
`expand`, `parse_romdir`, `find_romdir` and `write_inc` helper names remain
importable. Resource CLI errors return status 2 with an explanatory message,
without a Python traceback. Run from any working directory. `--raw` optionally
writes expanded bytes as well as C includes. Generated resources remain
ignored; never publish them by assuming noncommercial use is sufficient.

Default resource budgets: 64 MiB input read, 32 MiB expanded per file, 128 MiB
aggregate expanded bytes and 4,096 directory entries. These are defensive
**engineering limits, not verified original PS2 parameters**. The Python API
permits caller-specified positive limits. C include text expands the memory
and disk footprint beyond the binary budget; this offline tool is not a
browser sandbox or general upload service.

Validation rejects unaligned/truncated directory tables, duplicate/unsafe
names, out-of-image spans, invalid C identifiers, truncated LZ streams,
references preceding output, copies overshooting the declared size and
budget violations. Valid overlapping LZ copies and trailing padding remain
supported. Names are limited to safe ASCII ROM filenames; output resources
must also be C identifiers. Previously accepted malformed files may now fail.
Real-firmware compatibility still needs authorized local verification.

All resources are parsed before output-directory creation: invalid input does
not write an earlier good resource before encountering a later bad one.
Each output is fsync'd and atomically replaced, with temporary-file cleanup.
This is **per-file atomic replacement, not an all-files transaction**: disk
failure halfway through extraction can leave a mixture of old/new files.
An explicit output directory is a trusted user choice; do not point it at
an untrusted filesystem. These semantics are not IndexedDB save verification.

The advanced `--tables` workflow preserves its original fixed-address
font/string/geometry extraction and OSDSYS detection heuristic. Input reads
and LZ expansion are now bounded, but the table layout is image-specific;
it is **not** a validated general untrusted-input importer. Its synthetic
regressions verify preservation, not compatibility with every BIOS revision.
`ps2expand.c` is unchanged and still unsafe for malformed input.

## Original build and comparisons

The original `osdbits/Makefile` requires freesce/SCE EE toolchains, libraries,
resources, and VU assembler. No SDK was fetched. Its dry-run is blocked here
by `/usr/local/freesce/ee/lib/crt0.o`. Preserve the existing source and
comparison notes; do not put ROM bytes, GS dumps or extracted includes into
portable fixtures. The libgpu2 replay harness has additional private/i386
requirements and cannot serve as an ordinary CI prerequisite.

## Browser build and deployment

Not available yet. No deployment URL, supported GPU/browser matrix,
performance budget result, audio-activation result or browser-persistence
claim is published. Do not deploy this repository as if it already contains
a usable web application. The next foundation is provisioned native/WASM
tooling and the source-grounded deterministic Rust engine.
