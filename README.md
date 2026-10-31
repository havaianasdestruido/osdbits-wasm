## GitHub Pages deployment

The CI workflow builds, tests, and deploys the static `dist/` bundle on default-branch pushes. Set **Settings → Pages → Source** to **GitHub Actions** first. See the [deployment guide](docs/testing-and-deployment.md#github-pages-github-actions) for details.

# osdbits-wasm — PlayStation 2 OSDSYS Browser Reconstruction (`Rust/WASM + WebGPU`)

`osdbits-wasm` is a browser-native, deterministic, persistent, clean-room reconstruction of the PlayStation 2 **OSDSYS** boot & menu visual experience, forked from [`aap/osdbits`](osdbits) while preserving 100% of the upstream Emotion Engine / VU1 / GS reverse-engineering sources and notes untouched.

It runs **out-of-the-box with zero proprietary Sony BIOS or ROM assets**, combining a zero-dependency **Rust -> WebAssembly (`wasm32-unknown-unknown`)** simulation core with a multi-pass **WebGPU (TypeScript + WGSL)** renderer, procedural **Web Audio** soundscape, and read-back-verified **IndexedDB / OPFS** persistence.

## Supported Visual Modes

- **Mode A — Opening / Towers (`opening`)**: Deterministic `14x9` memory-card boot-history tower field over the `20x20` radial `HeightGrid`, `vucode_1` 3-light Gouraud shading, `DrawExtraBuf2(80)` temporal glow, 6 scrolling fog layers, 4 colored light orbs + 128-point trails, 5 per-cube 10-pass refractive glass cubes, legal-safe banner overlay, fly-up motion blur, and automatic transition into Mode D.
- **Mode B — Indefinite Tower Idle (`idle`)**: Continuous atmospheric drift over the tower field at `z = 16`.
- **Mode C — Illegal-Disc Red Scene (`illegal-disc`)**: `DrawRedFlare` 7 spinning 16-gon glow fans + 5 lens sprites, `DrawExtraBuf2(112)`, 128 falling crimson cloud patches, 5 red refractive glass cubes, and localized warning text (with explicit UI/inspector disclosure that retail hardware `osdBootParam` is unverified per `osdbits/opening.c:4262`).
- **Mode D — Main Menu & System Configuration (`menu`)**: 7 RTC clock-driven orbs with 150-point decaying ribbons, `16x33` breathing `KABE` tunnel, 12 hexagonal crystal clock rods (`MeshDrawRodSplit` minute progress bar), 5 camera-space glass item cubes, 5-pass `MenuZoomBlur`, and full keyboard/pointer navigation (`System Configuration`, 6-field `Clock Adjustment`, provenance badges distinguishing `Sourced` vs. `Approximate` / `Extension` entries).
- **Mode E — Live Customization & Persistence**: NTSC `60Hz` / PAL `50Hz`, `4:3` / `16:9` / `fit` framing, `0.5x..2.0x` internal resolution scale, memory-card tower simulator (`Captured`, `Full 126`, `Simulated 0..126 games`) + PRNG seed, pass toggles, `prefers-reduced-motion` support, audio mixer, optional client-side user-owned `BIOS`/`TEXIMAGE` import, JSON export/import, and two-step confirmed **Reset to Defaults**.

## Quick Start

```bash
npm ci
npm run dev       # Starts dev server on http://0.0.0.0:5173
npm run build     # Builds production static bundle into dist/
npm run check:all # Runs legal, Rust/fuzz, TS, Vitest, WGSL, WebGPU E2E, visual, perf & prod suites
```

Click **Start OSDSYS** once to synchronously unlock Web Audio (`AudioContext.resume()`) and enter the live WebGPU scene immediately.

## Documentation

- [Master Task Plan, Deferred Features & Known Limitations (`TODO.MD`)](TODO.MD)
- [Phase 0 Reconnaissance & Baseline Audit (`docs/phase0-baseline.md`)](docs/phase0-baseline.md)
- [WASM & WebGPU Single-Ownership Architecture (`docs/wasm-architecture.md`)](docs/wasm-architecture.md)
- [Fidelity & Parameter Provenance Matrix (`docs/fidelity-and-provenance.md`)](docs/fidelity-and-provenance.md)
- [Legal Policy, Clean-Room Assets & Optional Local Resource Import (`docs/legal-and-assets.md`)](docs/legal-and-assets.md)
- [Testing, Visual Regression Tolerances & Deployment Guide (`docs/testing-and-deployment.md`)](docs/testing-and-deployment.md)

---

# Upstream Historical PS2 Reconstruction (`aap/osdbits`)

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

## Layout

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

## Building osdbits

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

## Status

- Opening scene: towers, fog, light streaks, refractive cubes, the
  camera state machine (fly-up and scene end) and the "Sony Computer
  Entertainment" text overlay — done and verified.  A few small init
  helpers remain stubs (see `osdbits/opening.c`).
- Illegal-disc screen: ported (red cloud particles, the flare discs and
  lens sprites, the red glass cubes, localized text, screen fades) —
  not yet verified against the real ROM.
- Menu/browser: notes only, see `docs/menu-rendering-reverse.md`.
