# osdbits-wasm baseline — 2026-10-31

## Evidence and scope

Starting commit: `ea0c68712b28b36fe033ed4fe488d86958de53d2`, a shallow
checkout with one visible commit (aap, 2026-09-02). Working tree initially
clean. All 141 tracked files were inventoried; see `SOURCE-INVENTORY.md`
for sizes, hashes and source gap markers. No historical sources are removed.
Static inspection does not independently reproduce upstream runtime claims.

## What actually exists

| Area | Evidence | Baseline interpretation |
|---|---|---|
| EE entry / display | `osdbits/main.c`, `inc.h`, `Makefile` | NTSC 640×224 interlaced fields; semaphore/thread-driven frame loop; SCE/freesce-specific APIs and memory addresses |
| Opening | `opening.c` (4,355 lines), `towerchain.dsm`, `vucode_1.vsm` | Substantial implementation, not a stub or a portable library |
| Towers | `HeightGrid`, `GenerateTowerField`, `DrawTowers` | 20×20 brightness grid, 14×9 placement, simulated/captured boot history, 21 title neighbourhoods, count-driven sprawl; full-field test mode is explicitly not original |
| Camera | `ProcessOpeningAnimation` | Frame-based states with height thresholds 16,56,104,320,672,800,1160,1160; PAL step 1.2 vs NTSC 1; disc/boot parameters affect transitions |
| Fog / lights / glass | `opening.c`, `glass-refraction.md`, `towers-analysis.md` | Ordered tower/feedback capture/fog/light/cube passes; per-object screen capture and back/front refraction, not generic transparent boxes |
| Illegal disc | `opening.c` | Red fog particles, flare/lens sprites, cube callback, localized text and fades implemented upstream; ROM equivalence not established here |
| Menu | `menu.c`, `menuback.c`, `menuconfig.c`, `menutext.c` | Actual orb trails, depth ordering, refractive meshes, backdrop, vignette, configuration views and text/editor code exist. README's “notes only” statement is stale |
| Input / audio | `pad.c`, `sound.c`, `docs/pad.md`, `docs/sound.md` | ROM IOP module/padman loading and OSDSND RPC implementation; cannot run in browsers as written |
| Resource loading | `res.c`, `res.h`, two related `extract-res.py` scripts (the `osdbits/` copy adds `--tables` for fonts/strings/menu geometry) | Static includes of externally extracted fonts, textures AND sounds. TEXIMAGE alone is insufficient for all current includes; FNTIMAGE/SNDIMAGE also matter |
| Binary matching | `matching/src`, tables, campaigns, `check.py` | Relocation-masked R5900 instruction comparison, not a portable unit-test suite; requires EE tools + locally supplied expanded ROM |
| GS comparison | dump decoders, `gsmem.py`, `gsreplay/`, `gscmp-FINDINGS.md` | Existing useful local workflow; dumps and proprietary GS model library are absent; do not make ordinary CI depend on them |
| Web app / Rust / tests / CI | No existing manifests, browser entry, Rust crates, test directory or CI | Entire browser integration remains to implement |

## Rendering constraints to preserve

- Original geometry uses PS2/VU matrices and Z conventions, not WebGPU's
  clip/depth conventions. Port with an explicit tested coordinate conversion.
- Scratchpad packet allocation at `0x70000000`, double-buffered VIF/GIF DMA,
  GS frame/texture aliasing and field half-pixel offsets are PS2-only.
- `notes.txt` distinguishes VU programs: program 1 has four directional
  lights/frustum culling; programs 2–7 vary lighting, culling and morph counts.
  These are sources of behavior to translate, not browser executable code.
- Opening feedback is half-width save/composite; cubes snapshot the scene
  per object. A single translucent-material pass is not equivalent.
- `gscmp-FINDINGS.md` documents outstanding configuration differences:
  cube AA/Z mask seams, missing orb work-buffer passes, a missing distortion
  stage, backdrop blending differences and UV residuals. Treat fixes as
  unverified until current code and matching captures demonstrate them.
- Preserve original notes and reference workflow. No new golden images can
  be called ROM references without authorized captures and documented poses.

## Current gaps and defects

See explicit markers in `SOURCE-INVENTORY.md`, including:

- `main.c`: simulated disc/boot glue, fixed language/NTSC defaults, unknown
  drawing initialization and thread-wakeup TODOs; boot-parameter derivation
  is not fully ported.
- `opening.c`: scratchpad safety TODO, unknown initialization, incomplete
  fade interpretation, remaining unknown scene siblings and an unresolved
  framebuffer-reset operation. “Unported siblings” comments partly predate
  implemented feedback functions: do not infer every sibling is empty.
- `menutext.c`: RTC write, NVRAM/config save and digital-output hardware
  effects are printf stubs. Browser persistence is a separate implementation,
  not a claim of physical PS2 hardware behavior.
- `res.c`: deliberately static fake loader; no untrusted-file validation.
- Extraction baseline: unbounded expanded-size header, unchecked truncation
  and backreferences, unrestricted ROMDIR names used as output paths/C
  symbols, duplicate names silently overwritten, entry spans unchecked,
  unaligned ROMDIR accepted despite its documented contract. Two copies can
  drift. These tools must not become a public browser importer unchanged.
- `ps2expand.c`: fixed 32 MiB arrays with unchecked compressed reads/output
  lengths; retains historical provenance but is **not safe for untrusted
  input**. Do not use it as a web service/import endpoint.

## Baseline commands and environment

Linux sandbox, Node `v22.22.3`, npm `10.9.8`, Python `3.11.2`, GCC/make
available. No installed Rust/Cargo/rustup, EE compiler/assembler/SDK, or
browser executable found in the standard inspected paths.

| Command / inspection | Result |
|---|---|
| `git status --short` | clean before changes |
| `git ls-files` | 141 files, including inherited tracked Python 3.14 bytecode |
| `git rev-parse --is-shallow-repository` | true; deeper history not available locally |
| `make -C osdbits -n` | fails: no rule to make `/usr/local/freesce/ee/lib/crt0.o` |
| `command -v rustc cargo` | neither installed; native/WASM build blocked |
| Test/CI discovery | no existing ordinary test suite or CI configuration |
| ROM/GS comparison | not run: no authorized ROM, captures, EE SDK or libgpu2 |
| Browser / GPU / audio / restart persistence | not run: no application yet |

The original makefile uses `/usr/local/freesce/ee` plus
`/usr/local/sce_24/ee`, unlike the older README's single SDK recipe. It
requires `ee-gcc`, `ee-dvp-as`, SDK libraries and extracted resources.
`gsreplay/build.sh` additionally assumes local i386 libraries and a private
libgpu2 archive. No attempt was made to fetch proprietary dependencies.

## Provenance and release boundaries

The inherited five screenshots and microcode/analysis remain untouched;
their presence is not a new redistribution clearance. No repository-wide
license file was found. Review upstream code and screenshot rights before
public release; do not assume a public repository grants a license.
Do not add firmware, expanded ROMs, extracted resources, GS dumps, SDKs,
fonts, sound banks or proprietary GS model archives to releases or CI.
Ordinary tests must use synthetic bytes. User imports require authorization;
noncommercial status does not remove copyright obligations.

## Next deliverable

Establish synthetic, bounded resource validation first, then a portable
Rust deterministic core and a single TypeScript-owned WebGPU renderer.
Full browser product, persistence, fidelity, real-GPU validation and
production deployment remain open release gates in `TODO.MD`.
