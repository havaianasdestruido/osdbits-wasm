# osdbits-wasm

Browser-native preservation port in progress. **There is not yet a runnable
browser application or a release-ready WASM/WebGPU build.** The original PS2
implementation and analysis are preserved below; its runtime claims have not
been independently reproduced in this sandbox.

- [Implementation ledger](TODO.MD) — acceptance criteria, tests and blockers.
- [Baseline audit](docs/BASELINE.md) and [source inventory](docs/SOURCE-INVENTORY.md).
- [Architecture](docs/ARCHITECTURE.md) and [executed test evidence](docs/TEST-EVIDENCE.md).
- [Portable development and resource tooling](docs/DEVELOPMENT.md).

The first implemented port foundation is bounded local resource parsing with
synthetic regression tests. It does not require or provide a BIOS. No
proprietary resource data was added. Licensing and inherited-asset provenance
must be clarified before public redistribution.

## Original project — OSDSYS reverse engineering

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
- Menu/configuration: the checkout includes substantial reconstruction in
  `menu.c`, `menuback.c`, `menuconfig.c` and `menutext.c`, in addition to
  `docs/menu-rendering-reverse.md`. RTC/config persistence has stubbed
  hardware side effects; this is not a complete real memory-card browser.
  See the baseline audit for locally unverified behavior.
