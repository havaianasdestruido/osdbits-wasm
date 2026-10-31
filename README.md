# OSDSYS reverse engineering

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

---

# osdbits-wasm — the browser reconstruction

A browser-native port of this reconstruction: the opening (towers, fog, light trails, refractive
cubes, "Sony Computer Entertainment"), the illegal-disc warning screen, and the OSDSYS main menu
with System Configuration — running in Rust → WebAssembly with a WebGPU renderer, Web Audio, and
IndexedDB/OPFS persistence.

**No BIOS file is required.** All default textures, fonts and sounds are original, procedurally
generated material; the PS2 resource decoders (`expand()` LZSS, ROMDIR, the texture formats) are
implemented and exercised by tests, and you can optionally import a BIOS image or a
`TEXIMAGE`/`FNTIMAGE` container you dumped yourself to replace the procedural assets locally. No
imported bytes are committed to this repository and none leave the browser tab.

## Run it

```bash
npm install
npm run dev          # http://localhost:5173 — click "Start OSDSYS" once
```

Production build:

```bash
npm run build        # typecheck + vite build → dist/
npm run preview
```

## Controls

| Input | Action |
|---|---|
| Arrow keys / WASD | D-pad |
| Enter / Space | Cross (confirm outside Japan) |
| Esc / Backspace | Circle (cancel outside Japan) |
| Z / T | Square / Triangle |
| F2 | Customisation panel |
| Shift + Space | Pause |
| Gamepad | Mapped when connected |

Inside Mode D the menu is fully navigable: Browser (the memory-card history that generates the
tower field), System Configuration (Clock Adjustment with its orb-collapse and freeze, Screen Size,
Digital Output, Component Video Out, Language), and Version Information.

## Test it

```bash
cargo test --workspace      # 25 Rust tests (math, LZSS fuzz, pad parity, scenes, text)
npm run typecheck           # strict TypeScript
npx vitest run              # 31 tests: real WASM module, storage, audio gesture ordering,
                            #            and render-stream checks via a software rasterizer
npm run test:e2e            # 11 Playwright tests in Chromium with a real WebGPU device
```

Everything above was executed in this environment; evidence, including the one capability this
sandbox cannot verify (sustained GPU rendering), is in `docs/TESTING.md`.

## Learn how it fits together

- `docs/ARCHITECTURE-WASM.md` — layer boundaries, the original→port file map, render target model,
  blend/depth translation, determinism guarantees.
- `docs/LIMITATIONS.md` — what is verified, what is approximated, what is deferred, and why.
- `docs/PHASE0-RECONNAISSANCE.md` — the audit of the original PS2 implementation this port follows.

The original PS2 sources (`osdbits/`), the VU microcode sources and every analysis note in
`docs/` are preserved unmodified as the reference implementation.
