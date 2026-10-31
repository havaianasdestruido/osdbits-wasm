# Fidelity, Parameter Provenance & Reconstruction Matrix

`osdbits-wasm` strictly tracks the provenance of every subsystem against [`aap/osdbits`](../osdbits) and its reverse-engineering notes (`docs/towers-analysis.md`, `docs/menu-rendering-reverse.md`, `docs/menu-config.md`). We never invent undocumented retail OSDSYS behavior or claim hardware verification where upstream notes mark a detail approximated or unverified.

## Provenance Categories

- **`Sourced`**: Directly ported from decompiled/disassembled retail OSDSYS code and constants in `osdbits/*.c`, `osdbits/vucode_1.vsm`, and `matching/`.
- **`Measured`**: Verified against PCSX2 savestates or GS register dumps documented in `docs/*.md`.
- **`Approximated`**: Clean-room visual or procedural substitute used where copyrighted Sony ROM assets (`TEXIMAGE`, `FNTIMAGE`, `SNDIMAGE`, `MENUGEOM`) are absent or where retail delegates to external subsystems (`Browser`, `rom0:OSDSND`).
- **`Unknown / Unverified`**: Explicitly noted in `osdbits` as not yet confirmed against physical PS2 hardware RAM/boot state.

---

## Subsystem & Parameter Provenance Table

| Mode / Subsystem | Parameter / Algorithm | Category | Upstream Source & Notes |
| --- | --- | --- | --- |
| **PRNG & Init** | `osdRand = (seed * 1103515245 + 12345) & 0x7fffffff` (`Init()` 385 draws -> `lightsSeed = 4002` at `#386`) | `Sourced` + `Measured` | `osdbits/opening.c:190..265` (savestate-verified `#386` draw). |
| **Tower PRNG** | `TowerRng` (`0x2b992ddf ^ seed`) sprawl & eviction over `HIST_CELLS[21][6]` | `Sourced` | `osdbits/opening.c:3305..3475`. |
| **Projection & Aspect** | `screenAY = 0.457627` (NTSC 60Hz `dt=1.0`) / `0.52627105` (PAL 50Hz `dt=1.2`); Menu `0.47` / `0.5405` | `Sourced` | `osdbits/opening.c:360`, `osdbits/menu.c:295`. |
| **Mode A / B Towers** | `20x20` `HeightGrid` radial falloff, `14x9` `HIST_SRC_POS`, `vucode_1` 6-quad lighting (`1.0 / 0.8 / 0.8 / 0.4`), `CLAMP_1` cap streaks | `Sourced` + `Measured` | `osdbits/vucode_1.vsm`, `docs/towers-analysis.md`. |
| **Mode A Timeline** | `ProcessOpeningAnimation` (`z = 16` drift -> `z > 18` banner -> fly-up spline -> `z >= 320` black fade -> Menu handoff) | `Sourced` | `osdbits/opening.c:1770..1925`, `2183..2191`, `2721..2739`. |
| **Mode A / B / C Glow** | `DrawExtraBuf2(fix)` (`fix = 80` Opening, `112` Illegal; `BlendModes[2] = {0,1,2,1}`) + half-width `DrawToExtraBuf2()` | `Sourced` | `osdbits/opening.c:1993..2076`. |
| **Mode A / B Fog & Cubes** | 6 scrolling `DrawFog` cloud layers + 5 per-cube 10-pass refractive glass cubes (`CubeCaptureBuffer` `TEXA{0x7f, AEM=1}`) | `Sourced` | `osdbits/opening.c:1038..1180`, `1490..1760`. |
| **Mode A / B Light Orbs** | 4 colored orbs + 128-entry trails (`z < 73`) | `Sourced` (motion/trails) + `Approximated` (clean-room sprite) | `osdbits/opening.c:1184..1310`. |
| **Mode C Illegal Disc** | `DrawRedFlare` (7 spinning 16-gon fans + 5 `FLAR` lens sprites), 128 falling `FOG0` clouds, 5 red glass cubes, fades (`z = 672..800`) | `Sourced` | `osdbits/opening.c:2286..2615`. |
| **Mode C Boot Param** | Exact integer `osdBootParam` written by hardware disc-check thread on unauthorized disc boot | `Unknown / Unverified` | `osdbits/opening.c:4262` (`101..105`, `113`, `116` all yield the identical infinite red scene; disclosed in Mode C HUD & Inspector). |
| **Mode D Orbs & Tunnel** | 7 RTC clock-driven `Orbs` + 150-point `(f^4, f^2, f)` trails (twin-drawn to `scene` + `wb3`), `16x33` `KABE` tunnel, `{0x37,0x28,0x3C}` tint, `DrawVignette`, 5-pass `MenuZoomBlur` | `Sourced` + `Measured` | `osdbits/menu.c:1015..1485`, `osdbits/menuback.c`. |
| **Mode D Config Geometry** | 90-frame `cfgAnim`, 12 clock rods (`MeshDrawRodSplit` minute bar), 5 glass cubes (`w = 0` camera-space `{0,150,200}` selection ease + `-0.1` confirm kick) | `Sourced` (choreography/passes) + `Approximated` (clean-room 16-face bevelled hex prism & 6-face cube meshes unless user imports `MENUGEOM`) | `osdbits/menuconfig.c`, `docs/menu-config.md`. |
| **Mode D Subscreens** | `System Configuration` & `Clock Adjustment` (`menutext.c` leap-year calendar rules) vs. `Browser` & `Version` | `Sourced` (`System Configuration` / `Clock`) vs. `Approximated` (`Browser` / `Version` labeled `[Approximate]` in UI) | `osdbits/menu.c:1690`, `osdbits/menutext.c`. |
| **Default Textures & Banners** | 23 procedural texture slots (`WAL0` 6 mips, `FOG0` crimson, `FOG1..4` mist, `KABE`, `BUMP`, `BINV`, `REFA`, `OSDBITS` legal-safe banner) | `Approximated` | `web/src/assets/proceduralTextures.ts` (zero Sony ROM bytes; replaceable locally via user-owned BIOS `TEXIMAGE` import). |
| **Web Audio Soundscape** | Procedural suspended-harmonic ambient pad + crystal UI foley | `Approximated` | `web/src/audio/audioEngine.ts` (`osdbits/sound.c` only issues IOP `rom0:OSDSND` RPCs; zero `SNDIMAGE` samples shipped). |
