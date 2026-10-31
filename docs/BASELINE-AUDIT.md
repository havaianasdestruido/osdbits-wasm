# Baseline audit — `osdbits` (upstream fork) as found

Commit audited: `ea0c687` (`docs: add scene screenshots, 2x2 grid in the README`),
branch `master`, repository `havaianasdestruido/osdbits-wasm` (fork of `aap/osdbits`).

This document records **what actually exists** in the repository before any
change made by the `osdbits-wasm` work. Everything below was established by
reading the tree, the sources, the analysis notes and git history, and by
attempting the documented build in this environment. Nothing here is inferred
from the README alone.

---

## 1. Inventory

| Area | Files | Lines | Notes |
|---|---|---|---|
| PS2 reconstruction (EE C) | `osdbits/main.c`, `opening.c`, `menu.c`, `menuback.c`, `menutext.c`, `menuconfig.c`, `pad.c`, `sound.c`, `res.c`, `inc.h`, `res.h` | 12 940 | Whole program is C for the Emotion Engine, no C++ |
| VU microcode | `vucode_1.vu/.vsm` … `vucode_7.vu`, `osdbits/towerchain.dsm`, `osdbits/vucode_1.vsm` | — | Disassembly (`.vu`) + reconstructed dvp-as source (`.vsm`), byte-compared against the ROM per `docs/towers-analysis.md` |
| Build | `osdbits/Makefile` | 91 | SCE/freesce EE toolchain, hard-coded `/usr/local/freesce/ee` and `/usr/local/sce_24/ee` |
| Resource extraction | `tools/extract-res.py`, `osdbits/tools/extract-res.py` | — | Requires a user-supplied retail BIOS image |
| GS tooling | `osdbits/tools/gsdump-*.py`, `gsmem.py`, `gsreplay/*` | ~2 000 | GS dump decode / replay / PNG capture harness (native C + Python) |
| Matching campaign | `matching/` (`campaign/`, `src/*.c`, `*.txt`, `inventory.tsv`) | ~10 000 | Function-by-function binary matching against the real ROM |
| Analysis notes | `docs/*.md`, `docs/OSDSYS-STRUCTURE-REPORT.txt` | ~15 000 | Deep reverse-engineering notes: towers, glass/refraction, menu, sound, pad, kernel |
| Reference captures | `docs/opening.png`, `illegal.png`, `menu.png`, `config1.png`, `config2.png` | — | Screenshots of the *reconstruction* running (not raw GS dumps) |
| CI | — | — | None. No `.github/`, no test runner, no automated checks |
| Tests | — | — | None. No unit, property, integration or visual tests of any kind |

`.gitignore` correctly excludes build output, extracted Sony data
(`osdbits/res/`, `vucode_[0-9]`), local savestates and BIOS-derived reference
material. Only the reconstruction's own screenshots are tracked.

## 2. What is actually implemented (behaviour demonstrated by code)

### Opening / towers — implemented, claims ROM-verified
`osdbits/opening.c` (4 355 lines) is a complete reimplementation of the boot
animation's frame loop:

* **Camera state machine** (`ProcessOpeningAnimation`, `0x215fd0`): states
  0–7, thresholds `{16,56,104,320,672,800,1160,1160}`, semi-implicit
  integration of acceleration/speed/position and roll. States 1/2/3/6/7
  implement the disc-detection branch, boot-message dispatch, the fly-up
  acceleration and the 128-frame end latch. Ported with per-line ROM
  addresses in the comments.
* **Tower field**: 21-entry memory-card boot history → 6-cell neighbourhoods →
  `towerFlags/C/D/A/B`, radial `HeightGrid` (`0x217e30`), per-cell sway,
  per-cell quantised rotation term, Gouraud vertex colours (near lit / far
  black), the `u` clamp-band ST trick, and the VU1 chain patching
  (`towerPatchParams/Tags/Vertices/ST`, `towerKick`). The six per-tower quad
  packets including their static geometry are in `osdbits/towerchain.dsm`.
* **Fog**: 6 scrolling layers of 16×16 additive patches (`DrawFog`).
* **Light streaks**: 4 orbiting lights, 4-deep matrix history billboards and a
  128-entry projected trail polyline (`DrawLights`).
* **Refractive cubes**: 5 instances, per-face normal/visibility(view-angle)
  setup (`CubeTransformAndClip`, `CubeFaceSetup`) and the real 10-pass
  composite (`cubePasses`) sampling the current frame / env maps, plus the
  illegal-scene variant (`illegalCubePasses`).
* **Text**: SCE text fade in/out (`DoSCEText`) and the illegal-disc text
  (`DoIllegalText`) with the `sub_2144c0` motion-blur churn.
* **Illegal disc scene**: red cloud particles (128), 7 spinning flare discs,
  lens-flare sprite stack, red glass cubes, fades, screen motion blur.
* **Boot sound**: `OSDDispatch(20500/20501, …)` through the real OSDSND RPC
  path (`sound.c`), boot-param-derived start.

### Menu — partly implemented
`menu.c`, `menuback.c`, `menutext.c`, `menuconfig.c` (~7 000 lines) implement
the *3D main-menu background* (orbs, rods, backdrop tunnel, composite) and
*System Configuration* screen logic, with ROM addresses throughout. The
browser/memory-card/game-list logic is **not** implemented; `README.md` states
this honestly ("Menu/browser: notes only").

### Provenance / legal
* No Sony data in the tree. Textures/audio are generated at build time by
  `tools/extract-res.py` from a user-supplied BIOS.
* Audio banks and sequences are **not** redistributable; only the RPC
  plumbing and sound IDs are in the repo.

## 3. What is documentation only

* `docs/menu-rendering-reverse.md` — analysis of the menu renderer, no code.
* The browser, memory-card manager, disc detection, game list and system
  settings *screens* of the real OSDSYS.
* `docs/osdsys_re-crossref.md` refers to an IDA database (`osdsys_re/`),
  deliberately untracked.

## 4. Stubs and known limitations recorded by the original code

* `main.c`: `InitDraw` contains `// TODO: some unknown function` and the
  VBlank handler has `// TODO: wakeup a thread?`.
* `opening.c`: `MakeOpeningThread` is marked "this is not quite accurate";
  several real helper functions are documented as *not ported*
  (`sub_219f08` init chain is present, `sub_218b20` / `sub_218bd0` are
  dormant debug overlays; `sub_2144c0` blit is partially ported).
* `towerPatch*`: deliberate, documented fidelity bugs are *preserved*
  (`BUG:` comments), and several ROM quirks are reproduced on purpose
  (dead code kept to match, clamp-band ST, `XYZF3` kill float).
* Menu: only the background + config screens; `MenuSelectItem` is a shell.
* Audio: no redistribution; boot chime requires extracted banks.

## 5. Buildability in this environment — measured

| Check | Result |
|---|---|
| `ee-gcc`, `ee-dvp-as`, `dsedb` | **absent**; `/usr/local/sce*` and `/usr/local/freesce` do not exist |
| `osdbits/res/` (BIOS-extracted resources) | **absent**, generated by a script that needs a retail BIOS |
| `make` in `osdbits/` | **cannot run** (no EE toolchain, no resources) |
| Any test command | **none exists** |
| Existing CI | **none** |

**Conclusion:** the upstream PS2 reconstruction is *not buildable or testable
in this environment*. This is an environmental limitation, not a defect of the
upstream code. Consequences for this project:

1. The PS2 C sources are treated as the **specification** (they are the most
   precise description of the original behaviour available without the ROM),
   and are preserved unmodified at the repository root.
2. Every browser-side behaviour ported from them is ported into **Rust** with
   tests, and the port's provenance is recorded per function in
   `docs/PORTING-NOTES.md`.
3. Claims of ROM fidelity can only be *inherited* from the upstream analysis
   notes. This project therefore labels ported behaviour **"ported from the
   reconstruction"**, never "verified against the ROM", unless a comparison
   is reproducible here.
4. Visual validation uses `docs/*.png` (captures of the reconstruction) as the
   only available reference, and is labelled as such.

## 6. Toolchain situation (this environment)

* No `rustc`/`cargo`, no `static.rust-lang.org`, no `crates.io`; the npm
  registry and `github.com`/`api.github.com` are reachable.
* A pinned Rust 1.88.0 toolchain (host + `wasm32-unknown-unknown`) is
  assembled from airgap-friendly npm components by
  `scripts/bootstrap-rust.sh`; see that script for the normal `rustup` path.
* Consequence: **the core crate must have zero third-party dependencies**
  (no `wasm-bindgen`, no `proptest`, no `arbitrary`). The WASM boundary is a
  hand-written `extern "C"` ABI over linear memory, and property/fuzz testing
  is implemented with a small in-crate harness. Both decisions are documented
  in `docs/ARCHITECTURE.md`.

## 7. Baseline test coverage

**Zero.** There is no automated test of any kind in the repository: no Rust,
no JS/TS, no browser, no visual regression, no storage, no CI. The first
task of this project (Phase 0/1) is therefore to create the entire test
infrastructure and to record executed evidence for every claim.
