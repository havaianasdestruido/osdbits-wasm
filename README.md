# osdbits-wasm — Browser-Native PS2 OSDSYS Visual Reconstruction

`osdbits-wasm` is a clean-room, persistent, extensively tested **Rust (`wasm32-unknown-unknown`) + WebGPU + TypeScript** browser reconstruction of the PlayStation 2 `OSDSYS` boot monolith, built directly upon the preserved reverse-engineering research of [`aap/osdbits`](#upstream-ps2-ee-reconstruction--reverse-engineering-notes-aaposdbits).

- **Zero BIOS Required:** Runs out-of-the-box using deterministic clean-room procedural RGBA8 textures, hexagonal bevelled glass meshes, proportional UI glyphs, and procedural Web Audio synthesis. Zero Sony ROM, BIOS, or extracted proprietary assets are included anywhere in this repository.
- **One-Click Launch (`Start OSDSYS`):** A single user gesture unlocks `AudioContext.resume()` synchronously, probes WebGPU (with automatic graceful fallback to Canvas 2D software rendering), loads the `~168 KiB` WASM simulation core, restores IndexedDB preferences, and enters the live scene immediately.
- **Optional Local `TEXIMAGE` / `ROMDIR` Import:** Users who own hardware dumps can optionally select a local `TEXIMAGE` or BIOS container in-browser; `osdbits-core` validates `ROMDIR` offsets, runs bounds-checked LZ `expand()`, verifies SHA-256 integrity, and persists the validated asset locally in IndexedDB/OPFS without uploading anything externally.

---

## Experience Modes (`Modes A–E`)

| Mode | Scene | Status |
| --- | --- | --- |
| **Mode A · Opening / Towers** | 21-slot tower sprawl, 14×9 swaying canopy, 6-layer fog, 4 orbiting lights + trails, 5 10-pass refractive glass cubes, 8-state camera fly-up, conservative clean-room banner, configurable completion policy. | **Verified Reconstruction** *(banner: Approximate by clean-room policy)* |
| **Mode B · Idle Towers** | Indefinite drift over the illuminated tower canopy with customizable speed and drift amplitude (verified stable over 10,000+ steps). | **Verified Reconstruction** |
| **Mode C · Illegal Disc** | `z = 672 -> 800+` ascent, 7-ring crimson flare, 128 falling red fog patches, 5 red refractive cubes, 7-language clean-room disc warning. | **Approximate** *(ported from upstream RE without retail illegal-disc GS dump)* |
| **Mode D · OSDSYS Menu & System Configuration** | 7 RTC-driven orbs + 50-sample `(age^4, age^2, age)` trails + radial bloom; `16×33` breathing smoke tunnel; 12 hexagonal bevelled glass clock rods with active-hour minute fill & 2-walk sparkle; 5 zig-zag config cubes; interactive Main Menu, System Configuration, Clock Adjustment digit editor, Version overlay, and badged Browser showcase (`Arrow` keys, `Enter`, `Escape`, `V`, or pointer click). | **Verified Reconstruction** *(Browser subpage: Approximate)* |
| **Mode E · Customization & Inspector** | Live PRNG seed & deterministic frame stepping, timeline speed, NTSC (`60Hz`)/PAL (`50Hz`), `4:3` / `Full` / `16:9` aspect, render scale, fog/refraction/bloom toggles, `prefers-reduced-motion`, audio mixer, JSON settings export/import, and one-click reset to faithful defaults (`H` toggles HUD). | **Implemented & Tested** |

---

## Quickstart

### Prerequisites

- **Rust** (`stable` with `wasm32-unknown-unknown` target, `rustfmt`, `clippy`)
- **Node.js** `>= 22` and `npm`

### Run Locally

```bash
npm install
npm run dev
```

Build and preview the static hosting bundle (`dist/`, configured with relative `base: './'` for GitHub Pages / Cloudflare Pages / any static server):

```bash
npm run build
npm run preview
```

### Verification & Test Suites

```bash
npm run check:hygiene   # Verify zero Sony BIOS/ROM files & intact upstream sources
npm run check:rust      # cargo fmt --check + clippy -D warnings + 21 unit/property/fuzz tests
npm run test:unit       # Vitest TS unit + fake-indexeddb persistence + audio + fallback suites
npm run test:wasm       # Real WASM C-ABI boundary, determinism & synthetic TEXIMAGE tests
npm run test:perf       # WASM <250 KiB budget & 1,200-frame zero-leak endurance benchmark
npm run test:e2e        # Playwright Dawn WebGPU, Canvas 2D fallback, browser-restart IDB & golden diff
npm run verify:all      # Run the entire verification gate end-to-end
```

---

## Documentation Index

- [`TODO.MD`](TODO.MD) — Phase 0–11 engineering roadmap, acceptance criteria, known limitations, and deferred features.
- [`docs/baseline-audit.md`](docs/baseline-audit.md) — Phase 0 audit of upstream `aap/osdbits` sources, microcode, and RE notes.
- [`docs/architecture.md`](docs/architecture.md) — Zero-copy Rust/WASM <-> WebGPU/Canvas2D single-owner architecture & `#[repr(C)]` ABI.
- [`docs/fidelity-matrix.md`](docs/fidelity-matrix.md) — Subsystem-by-subsystem fidelity & verification status table.
- [`docs/persistence-schema.md`](docs/persistence-schema.md) — IndexedDB/OPFS `v2` schema, `v1 -> v2` migration, read-back verification & stale-write protection.
- [`docs/testing-and-gpu.md`](docs/testing-and-gpu.md) — Layered test guide and honest GPU verification tiers (Mock vs. SwiftShader vs. Hardware GPU).
- [`docs/legal-and-assets.md`](docs/legal-and-assets.md) — Clean-room asset policy, trademark conservatism, and local user-import boundaries *(note: noncommercial preservation design reduces risk by excluding proprietary assets, but does not constitute legal advice or guarantee zero legal risk)*.

---

## Upstream PS2 EE Reconstruction & Reverse-Engineering Notes (`aap/osdbits`)

*The original upstream `README.md` and all PS2 Emotion Engine / VU1 sources (`osdbits/`, `docs/`, `matching/`, `tools/`, `ps2expand.c`, `vucode_*`) are preserved intact below.*

### OSDSYS reverse engineering

Reverse engineering of the PS2 OSDSYS — the boot ROM's on-screen display
(boot animation, browser, system configuration) — from a retail OSDSYS
image.

![the reconstructed opening animation](docs/opening.png)

<table>
  <tr>
    <td align="center"><img src="docs/illegal.png" width="400" alt="the reconstructed illegal disc animation"><br><sub>illegal disc</sub></td>
    <td align="center"><img src="docs/menu.png" width="400" alt="the reconstructed main menu"><br><sub>main menu</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/config1.png" width="400" alt="the reconstructed system configuration"><br><sub>system configuration</sub></td>
    <td align="center"><img src="docs/config2.png" width="400" alt="the reconstructed system configuration (clock)"><br><sub>clock adjustment</sub></td>
  </tr>
</table>

#### Layout

- `osdbits/` — buildable reconstruction of the opening ("towers") boot
  animation as a standalone EE program.  Towers, fog, light streaks and
  the refractive cubes are ported and verified against the real ROM
  (GS-dump and savestate comparisons), running both under PCSX2 and on
  real hardware.
- `docs/` — analysis notes: `towers-analysis.md` is the full trace of the
  tower/cube pipeline (the lab notes behind the code), plus a structure
  report, menu rendering notes, and a cross-reference to the osdsys_re
  symbol database.
- `vucode_*` — the image's VU microcode: disassembly (`.vu`) and
  reconstructed dvp-as source (`.vsm`).
- `tools/` — resource extraction (see below).

#### Building osdbits

The repo contains none of Sony's data.  The textures are extracted from
a PS2 BIOS image you dumped from your own console (the same file PCSX2
uses):

    python3 tools/extract-res.py path/to/bios.bin osdbits/res

Building requires the SCE PS2 EE toolchain (`ee-gcc`, `ee-dvp-as`) with
the SDK at `/usr/local/sce/ee`:

    cd osdbits
    make        # produces main.elf
    make run    # runs on a TOOL via dsedb

The ELF also runs under PCSX2.  Optional arguments
`main.elf [mode] [seed [ngames [nboots [framelimit]]]]`:

- `mode` — `boot` (default): the real boot sequence — camera drift,
  fly-up, "Sony Computer Entertainment" text, scene end; `idle`: stay
  over the tower field forever; `illegal`: the illegal-disc scene
  (red clouds, flare, cubes, "please insert a ... disc" text).
- the numeric args control the simulated memory-card boot history the
  tower field is generated from (`ngames 0` lights up the whole field;
  `framelimit` exits after N frames for scripted screenshots).

#### Status

- Opening scene: towers, fog, light streaks, refractive cubes, the
  camera state machine (fly-up and scene end) and the "Sony Computer
  Entertainment" text overlay — done and verified.  A few small init
  helpers remain stubs (see `osdbits/opening.c`).
- Illegal-disc screen: ported (red cloud particles, the flare discs and
  lens sprites, the red glass cubes, localized text, screen fades) —
  not yet verified against the real ROM.
- Menu/browser: notes only, see `docs/menu-rendering-reverse.md`.
